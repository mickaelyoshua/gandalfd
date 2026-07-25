use gandalfd::{
    blocklist::OptimizedBlocklist, fetcher::Fetcher, handler::GandalfHandler, observability,
    settings::AppSettings,
};
use hickory_resolver::{
    TokioResolver,
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
    net::runtime::TokioRuntimeProvider,
};
use hickory_server::server::Server;
use std::{net::SocketAddr, path::Path, sync::Arc};
use tokio::net::{TcpListener, UdpSocket};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init();

    let config = AppSettings::load();
    info!(?config, "Gandalf DNS Blocker starting...");

    // We attempt a fast local cache read during boot. If the cache is missing or stale,
    // the fetcher automatically falls back to a synchronous network request.
    // In a fully robust environment, we'd spawn a background task to call `force_update()` periodically.
    let mut combined_list = String::new();
    let mut loaded_lists = 0;

    for (i, url) in config.blocklist_urls.iter().enumerate() {
        let cache_file = format!("cache_{}.txt", i);
        let fetcher = Fetcher::new(url, Path::new(&cache_file));

        match fetcher.get_data().await {
            Ok(data) => {
                info!("Loaded {} bytes from {}", data.len(), url);
                combined_list.push_str(&data);
                combined_list.push('\n');
                loaded_lists += 1;
            }
            Err(e) => {
                // Graceful degradation: If a blocklist fails to load (network error, invalid domain),
                // we log it as a warning and continue booting with the remaining lists.
                tracing::warn!(url = %url, error = %e, "Failed to load blocklist. Proceeding without it.");
            }
        }
    }

    if loaded_lists == 0 && !config.blocklist_urls.is_empty() {
        tracing::warn!(
            "All blocklists failed to load. The DNS server will run, but no domains will be blocked."
        );
    }

    // Constructing the optimized blocklist ensures O(1) domain lookups.
    // This is computationally intensive upfront but guarantees minimal latency per DNS request.
    info!("Parsing blocklists...");
    let blocklist = Arc::new(OptimizedBlocklist::parse_list(&combined_list));
    info!("Blocklist loaded in memory.");
    let upstream_ip: std::net::IpAddr = config.upstream_dns.parse()?;
    let ns = NameServerConfig::udp(upstream_ip);
    let resolver_config = ResolverConfig::from_parts(None, vec![], vec![ns]);

    let mut builder =
        TokioResolver::builder_with_config(resolver_config, TokioRuntimeProvider::default());
    *builder.options_mut() = ResolverOpts::default();
    let resolver = builder
        .build()
        .map_err(|e| format!("Failed to build TokioResolver: {}", e))?;

    let handler = GandalfHandler {
        blocklist,
        resolver,
    };
    let mut server = Server::new(handler);
    let listen_addr = format!("0.0.0.0:{}", config.port).parse::<SocketAddr>()?;

    let udp_socket = UdpSocket::bind(listen_addr).await?;
    server.register_socket(udp_socket);

    let tcp_listener = TcpListener::bind(listen_addr).await?;
    // The response buffer size (512) is standard for typical DNS message limits.
    server.register_listener(tcp_listener, std::time::Duration::from_secs(30), 512);

    info!("DNS Server listening on UDP/TCP {}", listen_addr);

    // Block the main thread on the server execution or a termination signal.
    // This ensures pending I/O operations are handled cleanly before process exit.
    tokio::select! {
        _ = server.block_until_done() => {
            info!("Server stopped unexpectedly.");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down Gandalf DNS...");
        }
    }

    Ok(())
}
