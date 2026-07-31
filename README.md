# Gandalf DNS Blocker

Rust-based DNS server running on Raspberry Pi that filters queries against a blocklist and forwards clean queries.

## Configuration

The application loads configuration from `config.toml` or environment variables prefixed with `GANDALFD__`.

Example `config.toml`:
```toml
port = 5353

# Multiple upstream DNS servers are supported for high availability / failover.
# Must include both IP and port.
upstream_dns = ["1.1.1.1:53", "1.0.0.1:53"]

blocklist_urls = [
    "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts"
]
```