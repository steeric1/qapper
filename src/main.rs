mod ports;
mod scanner;

use std::net::IpAddr;

use clap::Parser;
use log::{warn, LevelFilter, SetLoggerError};
use ports::Ports;
use scanner::PortScanner;
use simplelog::{ColorChoice, ConfigBuilder as LoggerConfigBuilder, TermLogger, TerminalMode};

#[tokio::main]
async fn main() {
    let config = Config::parse();
    if config.verbose {
        init_logger(LevelFilter::Trace).map(|()| warn!("Verbose mode ON"))
    } else {
        init_logger(LevelFilter::Error)
    }
    .expect("Failed to initialize logger!");

    // leaky leaky...
    let addrs: &'static [IpAddr] = Box::leak(config.addrs.into_boxed_slice());
    let on_checked = move |_ip, _port, _open: bool| {};

    let scanner = PortScanner::new(config.ports, addrs, config.timeout, on_checked)
        .expect("Failed to create port scanner!");

    let map = scanner.scan().await;
    for (ip, status) in map.iter() {
        println!("{ip}:\n\t{}", status.to_string().replace(";", "\n\t"));
    }
}

fn init_logger(filter: LevelFilter) -> Result<(), SetLoggerError> {
    let config = LoggerConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Off)
        .set_time_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Off)
        .build();

    TermLogger::init(filter, config, TerminalMode::Mixed, ColorChoice::Auto)
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
