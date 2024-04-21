### Qapper

```
Program to quickly scan open ports

Usage: qapper.exe [OPTIONS] <PORTS> [ADDRS]...

Arguments:
  <PORTS>     Comma-separated list of ports or port ranges, e.g. "443,3000-5000". Ranges are inclusive: e.g. 23-45 will scan ports 23, ..., 45
  [ADDRS]...  IP addresses to scan. Can be either IPv4 or IPv6

Options:
  -v, --verbose            Emit verbose logs about the process
  -t, --timeout <TIMEOUT>  Timeout (ms) when trying to connect to a port to check if it's "open" [default: 1000]
  -h, --help               Print help
  -V, --version            Print version 
```