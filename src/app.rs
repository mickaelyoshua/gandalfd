use crate::{blocklist::Blocklist, handler::GandalfHandler, settings::AppSettings};
use hickory_resolver::{
    TokioResolver,
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
    net::runtime::TokioRuntimeProvider,
};
use hickory_server::server::Server;
use std::net::SocketAddr;
use tokio::net::{TcpListener, UdpSocket};
use tracing::info;

pub struct GandalfApp {
    server: Server<GandalfHandler>,
    listen_addr: SocketAddr,
}

impl GandalfApp {
    pub async fn build(config: AppSettings) -> Result<Self, Box<dyn std::error::Error>> {
        let blocklist = Blocklist::load_from_urls(&config.blocklist_urls).await;

        let upstream_ip: std::net::IpAddr = config.upstream_dns.parse()?;
        let ns = NameServerConfig::udp(upstream_ip);
        let resolver_config = ResolverConfig::from_parts(None, vec![], vec![ns]);

        let mut builder =
            TokioResolver::builder_with_config(resolver_config, TokioRuntimeProvider::default());
        // Default options provide standard timeouts and retry logic (e.g., 2 attempts, 5s timeout),
        // preventing the resolver from hanging indefinitely on dead upstream servers.
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
        // Buffer size (512) matches the historical UDP DNS limit, limiting memory per connection.
        // A 30-second timeout ensures dead connections don't leak file descriptors.
        server.register_listener(tcp_listener, std::time::Duration::from_secs(30), 512);

        Ok(Self {
            server,
            listen_addr,
        })
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("DNS Server listening on UDP/TCP {}", self.listen_addr);

        // Block the main task on either the server execution or a termination signal.
        // This ensures pending I/O operations and background tasks are handled cleanly
        // before process exit, rather than abruptly killing the async runtime.
        tokio::select! {
            _ = self.server.block_until_done() => {
                info!("Server stopped unexpectedly.");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down Gandalf DNS...");
            }
        }

        Ok(())
    }
}
