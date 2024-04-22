use std::{collections::HashMap, io, net::IpAddr, sync::Arc, time::Duration};

use log::{error, trace};
use surge_ping::{Client as PingClient, Config as PingConfig, PingIdentifier, PingSequence, ICMP};
use tokio::{net::TcpStream, sync::mpsc, time::timeout};

use crate::ports::{Ports, PortsStatus};

pub struct PortScanner<Callback>
where
    Callback: FnMut(&'static IpAddr, u16, bool) -> (),
{
    inner: Arc<ScannerInner<'static>>,
    channel: (PortSender<'static>, PortReceiver<'static>),
    on_checked: Callback,
}

impl<Callback> PortScanner<Callback>
where
    Callback: FnMut(&'static IpAddr, u16, bool) -> (),
{
    pub fn new(
        ports: Ports,
        addrs: &'static [IpAddr],
        timeout: u64,
        on_checked: Callback,
    ) -> io::Result<Self> {
        let inner = ScannerInner::new(ports, addrs, timeout).map(Arc::new)?;

        Ok(Self {
            inner,
            channel: mpsc::channel(100),
            on_checked,
        })
    }

    pub async fn scan(mut self) -> HashMap<IpAddr, PortsStatus> {
        let (tx, mut rx) = self.channel;

        for (idx, ip) in self.inner.addrs.iter().enumerate() {
            let inner = Arc::clone(&self.inner);
            let tx = tx.clone();

            // TODO: avoid collisions
            let id = (idx % (u16::MAX as usize)) as u16;

            tokio::spawn(async move { inner.scan_ip(ip, tx, id).await });
        }

        // if we don't do this, the loop below will never end going loopy loopy...
        drop(tx);

        let mut map = HashMap::new();
        while let Some((ip, port, open)) = rx.recv().await {
            (self.on_checked)(ip, port, open);

            map.entry(*ip)
                .or_insert(PortsStatus::new(self.inner.ports.len()))
                .record(port, open);
        }

        for status in map.values_mut() {
            status.sort();
        }

        map
    }
}

struct ScannerInner<'a> {
    pinger4: Option<PingClient>,
    pinger6: Option<PingClient>,
    ports: Ports,
    addrs: &'a [IpAddr],
    timeout: u64,
}

impl<'a> ScannerInner<'a> {
    fn new(ports: Ports, addrs: &'a [IpAddr], timeout: u64) -> io::Result<Self> {
        let (pinger4, pinger6) = Self::create_pingers(addrs)?;

        Ok(Self {
            pinger4,
            pinger6,
            ports,
            addrs,
            timeout,
        })
    }

    fn create_pingers(addrs: &'a [IpAddr]) -> io::Result<(Option<PingClient>, Option<PingClient>)> {
        let pinger4 = addrs
            .iter()
            .any(IpAddr::is_ipv4)
            .then_some(Self::create_pinger(ICMP::V4)?);

        let pinger6 = addrs
            .iter()
            .any(IpAddr::is_ipv6)
            .then_some(Self::create_pinger(ICMP::V6)?);

        if pinger4.is_none() && pinger6.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "tried to create port scanner with no supported IP versions",
            ));
        }

        Ok((pinger4, pinger6))
    }

    fn create_pinger(version: ICMP) -> io::Result<PingClient> {
        let config = PingConfig::builder().kind(version).build();
        PingClient::new(&config)
    }

    async fn scan_ip(&self, ip: &'static IpAddr, tx: PortSender<'a>, id: u16) {
        let Some(rtt) = self.ping(ip, id).await else {
            trace!("{ip} isn't responding");
            return;
        };

        trace!("{ip} is responding, pinged in {}ms", rtt.as_millis());
        trace!("Checking {} ports on {ip}...", self.ports.len());

        let mut handles = Vec::with_capacity(self.ports.len());
        for &port in &*self.ports {
            let timeout = self.timeout;
            handles.push(tokio::spawn(async move {
                Self::check_port(ip, port, timeout).await
            }));
        }

        for h in handles {
            let (port, open) = h.await.unwrap();
            tx.send((ip, port, open)).await.unwrap();
        }
    }

    async fn check_port(ip: &'static IpAddr, port: u16, timeout_ms: u64) -> (u16, bool) {
        let res = timeout(
            Duration::from_millis(timeout_ms),
            TcpStream::connect((*ip, port)),
        )
        .await;

        if let Ok(Err(e)) = &res {
            error!("Unexpected error: {e:#?}");
        }

        (port, res.is_ok())
    }

    async fn ping(&self, ip: &IpAddr, id: u16) -> Option<Duration> {
        let mut pinger = match ip {
            IpAddr::V4(_) => self.pinger4.as_ref(),
            IpAddr::V6(_) => self.pinger6.as_ref(),
        }
        .unwrap()
        .pinger(*ip, PingIdentifier(id))
        .await;

        trace!("Pinging {ip}...");

        let payload = [0; 56];
        pinger
            .ping(PingSequence(0), &payload)
            .await
            .map(|(_, rtt)| rtt)
            .ok()
    }
}

type PortSender<'a> = mpsc::Sender<(&'a IpAddr, u16, bool)>;
type PortReceiver<'a> = mpsc::Receiver<(&'a IpAddr, u16, bool)>;
