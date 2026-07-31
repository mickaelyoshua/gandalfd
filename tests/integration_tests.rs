#[cfg(test)]
mod tests {
    use gandalfd::blocklist::Blocklist;
    use gandalfd::handler::GandalfHandler;
    use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
    use hickory_resolver::{TokioResolver, net::runtime::TokioRuntimeProvider};
    use hickory_server::proto::op::ResponseCode;
    use hickory_server::proto::rr::{RData, Record};
    use hickory_server::server::{Request, RequestHandler, ResponseHandler, ResponseInfo, Server};
    use hickory_server::zone_handler::MessageResponseBuilder;
    use std::sync::Arc;
    use tokio::net::UdpSocket;

    struct MockUpstreamHandler;

    #[async_trait::async_trait]
    impl RequestHandler for MockUpstreamHandler {
        async fn handle_request<R: ResponseHandler, T>(
            &self,
            request: &Request,
            mut response_handle: R,
        ) -> ResponseInfo {
            let builder = MessageResponseBuilder::from_message_request(request);
            let Ok(info) = request.request_info() else {
                let response = builder.error_msg(&request.metadata, ResponseCode::FormErr);
                return response_handle.send_response(response).await.unwrap();
            };

            let name = info.query.name().to_string();
            println!("Mock upstream received query for: '{}'", name);
            if name.starts_with("allowed.com") {
                let record = Record::from_rdata(
                    info.query.name().clone().into(),
                    60,
                    RData::A(std::net::Ipv4Addr::new(93, 184, 216, 34).into()),
                );
                let mut header = request.metadata;
                header.message_type = hickory_server::proto::op::MessageType::Response;
                let response = builder.build(
                    header,
                    std::iter::once(&record),
                    std::iter::empty(),
                    std::iter::empty(),
                    std::iter::empty(),
                );
                response_handle.send_response(response).await.unwrap()
            } else {
                let response = builder.error_msg(&request.metadata, ResponseCode::NXDomain);
                response_handle.send_response(response).await.unwrap()
            }
        }
    }

    #[tokio::test]
    async fn test_gandalf_handler_blocking_and_forwarding() {
        tracing_subscriber::fmt().with_env_filter("debug").init();
        let upstream_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_socket.local_addr().unwrap();

        let mut upstream_server = Server::new(MockUpstreamHandler);
        upstream_server.register_socket(upstream_socket);
        tokio::spawn(async move { upstream_server.block_until_done().await });

        let mut ns = NameServerConfig::udp(upstream_addr.ip());
        ns.connections[0].port = upstream_addr.port();
        let upstream_config = ResolverConfig::from_parts(None, vec![], vec![ns]);

        let mut opts = ResolverOpts::default();
        opts.timeout = std::time::Duration::from_millis(500);
        opts.attempts = 1;
        let mut builder =
            TokioResolver::builder_with_config(upstream_config, TokioRuntimeProvider::default());
        *builder.options_mut() = opts;
        let resolver = builder.build().unwrap();

        let blocklist = Blocklist::parse_list("blocked.com");
        let handler = GandalfHandler {
            blocklist: Arc::new(blocklist),
            resolver,
        };

        let gandalf_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let gandalf_addr = gandalf_socket.local_addr().unwrap();

        let mut gandalf_server = Server::new(handler);
        gandalf_server.register_socket(gandalf_socket);
        tokio::spawn(async move { gandalf_server.block_until_done().await });

        let mut client_ns = NameServerConfig::udp(gandalf_addr.ip());
        client_ns.connections[0].port = gandalf_addr.port();
        let client_config = ResolverConfig::from_parts(None, vec![], vec![client_ns]);

        let mut client_builder =
            TokioResolver::builder_with_config(client_config, TokioRuntimeProvider::default());
        *client_builder.options_mut() = ResolverOpts::default();
        let client_resolver = client_builder.build().unwrap();

        let blocked_res = client_resolver
            .lookup("blocked.com.", hickory_server::proto::rr::RecordType::A)
            .await;
        assert!(
            blocked_res.is_err(),
            "Blocked domains should return an error (NXDOMAIN)"
        );

        let allowed_res = client_resolver
            .lookup("allowed.com.", hickory_server::proto::rr::RecordType::A)
            .await
            .expect("Allowed domains should resolve");
        let ips: Vec<std::net::Ipv4Addr> = allowed_res
            .answers()
            .iter()
            .filter_map(|r| {
                if let RData::A(a) = &r.data {
                    Some(std::net::Ipv4Addr::from(*a))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ips, vec![std::net::Ipv4Addr::new(93, 184, 216, 34)]);
    }

    #[tokio::test]
    async fn test_blocklist_load_from_urls() {
        use mockito::Server;

        let _ = std::fs::remove_file("storage_0.txt");
        let _ = std::fs::remove_file("storage_1.txt");
        let _ = std::fs::remove_file("storage_2.txt");

        let mut server1 = Server::new_async().await;
        let mock1 = server1
            .mock("GET", "/")
            .with_status(200)
            .with_body("0.0.0.0 ads.com\n127.0.0.1 tracker.com")
            .create_async()
            .await;

        let mut server2 = Server::new_async().await;
        let mock2 = server2
            .mock("GET", "/")
            .with_status(200)
            .with_body("0.0.0.0 malware.org\n#comment\n")
            .create_async()
            .await;

        let urls = vec![
            server1.url(),
            server2.url(),
            "http://invalid-url.local".to_string(),
        ];

        let blocklist = gandalfd::blocklist::Blocklist::load_from_urls(&urls).await;

        mock1.assert_async().await;
        mock2.assert_async().await;

        let ads = gandalfd::domain::DomainRef::parse("ads.com").unwrap();
        let malware = gandalfd::domain::DomainRef::parse("malware.org").unwrap();
        let safe = gandalfd::domain::DomainRef::parse("safe.com").unwrap();

        assert!(blocklist.is_blocked(ads));
        assert!(blocklist.is_blocked(malware));
        assert!(!blocklist.is_blocked(safe));

        let _ = std::fs::remove_file("storage_0.txt");
        let _ = std::fs::remove_file("storage_1.txt");
        let _ = std::fs::remove_file("storage_2.txt");
    }

    #[tokio::test]
    async fn test_app_build() {
        use gandalfd::app::GandalfApp;
        use gandalfd::settings::AppSettings;

        let config = AppSettings {
            port: 0,
            upstream_dns: vec![
                "127.0.0.1:53".to_string(),
                "1.1.1.1:53".to_string(),
                "[::1]:53".to_string(),
            ],
            blocklist_urls: vec![],
        };

        let result = GandalfApp::build(config).await;
        assert!(result.is_ok(), "App should build successfully");
    }
}
