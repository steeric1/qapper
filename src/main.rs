#![allow(dead_code, unused_imports, unused_variables)]

use std::{
    hash::Hash,
    io::{self, Write},
    net::IpAddr,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use log::{info, trace, warn, LevelFilter, SetLoggerError};
use simplelog::{
    ColorChoice, Config as LoggerConfig, ConfigBuilder as LoggerConfigBuilder, SimpleLogger,
    TermLogger, TerminalMode,
};
use surge_ping::{Client as PingClient, Config as PingConfig, PingIdentifier, PingSequence, ICMP};
use tokio::{net::TcpStream, sync::mpsc, time::timeout};

#[tokio::main]
async fn main() {
    let config = Config::parse();
    if config.verbose {
        init_logger().map(|()| warn!("Verbose mode ON"))
    } else {
        SimpleLogger::init(LevelFilter::Info, LoggerConfig::default())
    }
    .expect("Failed to initialize logger!");

    let scanner = Arc::new(
        PortScanner::new(
            config.addrs.iter().any(IpAddr::is_ipv4),
            config.addrs.iter().any(IpAddr::is_ipv6),
            config.ports,
            config.timeout,
        )
        .expect("Failed to create port scanner!"),
    );

    // leaky leaky...
    let addrs: &'static [IpAddr] = Box::leak(config.addrs.into_boxed_slice());

    let mut rx = {
        let (tx, rx) = mpsc::channel(100);

        for (idx, ip) in addrs.iter().enumerate() {
            let (scanner, tx) = (Arc::clone(&scanner), tx.clone());
            tokio::spawn(async move { scanner.scan(ip, tx, idx).await });
        }

        rx
    };

    while let Some((ip, port)) = rx.recv().await {
        info!("Port {port} on {ip} is open!");
    }
}

fn init_logger() -> Result<(), SetLoggerError> {
    let config = LoggerConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Off)
        .set_time_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Off)
        .build();

    TermLogger::init(
        LevelFilter::Trace,
        config,
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
}

struct PortScanner {
    pinger4: Option<PingClient>,
    pinger6: Option<PingClient>,
    ports: Ports,
    timeout: u64,
}

impl PortScanner {
    fn new(v4: bool, v6: bool, ports: Ports, timeout: u64) -> io::Result<Self> {
        if !(v4 || v6) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "tried to create port scanner with no supported IP versions",
            ));
        }

        let pinger4 = v4.then_some(Self::create_pinger(ICMP::V4)?);
        let pinger6 = v6.then_some(Self::create_pinger(ICMP::V6)?);

        Ok(Self {
            pinger4,
            pinger6,
            ports,
            timeout,
        })
    }

    fn create_pinger(version: ICMP) -> io::Result<PingClient> {
        let config = PingConfig::builder().kind(version).build();
        PingClient::new(&config)
    }

    async fn scan(&self, ip: &'static IpAddr, tx: mpsc::Sender<(&IpAddr, u16)>, id: usize) {
        let Some(rtt) = self.ping(ip, id as u16).await else {
            return;
        };

        trace!("{ip} is responding, pinged in {}ms", rtt.as_millis());
        trace!("checking {} ports on {ip}...", self.ports.0.len());

        let mut handles = Vec::with_capacity(self.ports.0.len());
        for &port in &self.ports.0 {
            let timeout = self.timeout;
            handles.push(tokio::spawn(async move {
                Self::check_port(ip, port, timeout).await
            }));
        }

        for h in handles {
            if let Some(port) = h.await.unwrap() {
                tx.send((ip, port)).await.unwrap();
            }
        }
    }

    async fn check_port(ip: &'static IpAddr, port: u16, timeout_ms: u64) -> Option<u16> {
        trace!("Checking {ip}:{port}... (timeout = {timeout_ms}ms)");
        timeout(
            Duration::from_millis(timeout_ms),
            TcpStream::connect((ip.clone(), port)),
        )
        .await
        .map(|_| port)
        .map_err(|_| trace!("{ip}:{port} timed out"))
        .ok()
    }

    async fn ping(&self, ip: &IpAddr, id: u16) -> Option<Duration> {
        let mut pinger = match ip {
            IpAddr::V4(_) => self.pinger4.as_ref(),
            IpAddr::V6(_) => self.pinger6.as_ref(),
        }
        .unwrap()
        .pinger(ip.clone(), PingIdentifier(id))
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

/// Program to quickly scan open ports
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Config {
    /// Comma-separated list of ports or port ranges, e.g. "443,3000-5000". Ranges are inclusive: e.g. 23-45 will scan ports 23, ..., 45
    ports: Ports,

    /// IP addresses to scan. Can be either IPv4 or IPv6
    addrs: Vec<IpAddr>,

    /// Emit verbose logs about the process
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Timeout (ms) when trying to connect to a port to check if it's "open"
    #[arg(short, long, default_value_t = 1000)]
    timeout: u64,
}

#[derive(Clone, Debug)]
struct Ports(Vec<u16>);

impl FromStr for Ports {
    type Err = <u16 as FromStr>::Err;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parsed = vec![];
        for part in value.split(',') {
            match part.split_once('-') {
                Some((lower, upper)) => {
                    let (lower, upper) = (lower.parse::<u16>()?, upper.parse::<u16>()?);
                    assert!(
                        lower <= upper,
                        "Expected port range lower limit be lower than upper limit!"
                    );

                    parsed.reserve((upper - lower + 1).into());
                    parsed.extend((lower..=upper).into_iter());
                }
                None => {
                    let port = part.parse()?;
                    parsed.push(port);
                }
            }
        }

        Ok(Self(parsed))
    }
}
