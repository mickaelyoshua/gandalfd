use crate::{blocklist::Blocklist, domain::DomainRef};
use hickory_resolver::TokioResolver;
use hickory_server::{
    proto::op::{Header, HeaderCounts, ResponseCode},
    proto::rr::Record,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
    zone_handler::{MessageResponse, MessageResponseBuilder},
};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Core DNS Orchestrator.
/// Routes incoming requests:
/// 1. Synchronous O(1) local lookup in the blocklist.
/// 2. If clean, forwards the query asynchronously to an upstream resolver.
pub struct GandalfHandler {
    pub blocklist: Arc<Blocklist>,
    pub resolver: TokioResolver,
}

#[async_trait::async_trait]
impl RequestHandler for GandalfHandler {
    async fn handle_request<R: ResponseHandler, T>(
        &self,
        request: &Request,
        response_handle: R,
    ) -> ResponseInfo {
        let Ok(info) = request.request_info() else {
            let builder = MessageResponseBuilder::from_message_request(request);
            let response = builder.error_msg(&request.metadata, ResponseCode::FormErr);
            return Self::safe_send(request, response_handle, response).await;
        };

        let query = info.query;
        let name_str = query.name().to_string();
        let domain_clean = name_str.trim_end_matches('.');

        if DomainRef::parse(domain_clean).is_ok_and(|d| self.blocklist.is_blocked(d)) {
            info!(domain = domain_clean, "BLOCKED");
            return Self::send_nxdomain(request, response_handle).await;
        }

        debug!(domain = domain_clean, "FORWARDING");

        match self
            .resolver
            .lookup(name_str.as_str(), query.query_type())
            .await
        {
            Ok(lookup) => {
                let builder = MessageResponseBuilder::from_message_request(request);
                let mut header = request.metadata;
                header.message_type = hickory_server::proto::op::MessageType::Response;
                let response = builder.build(
                    header,
                    lookup.answers().iter(),
                    std::iter::empty::<&Record>(),
                    std::iter::empty::<&Record>(),
                    std::iter::empty::<&Record>(),
                );
                Self::safe_send(request, response_handle, response).await
            }
            Err(e) => {
                debug!("Upstream lookup failed for {}: {}", domain_clean, e);
                Self::send_nxdomain(request, response_handle).await
            }
        }
    }
}

impl GandalfHandler {
    async fn safe_send<'a, R, A, N, S, D>(
        request: &Request,
        mut response_handle: R,
        response: MessageResponse<'_, 'a, A, N, S, D>,
    ) -> ResponseInfo
    where
        R: ResponseHandler,
        A: Iterator<Item = &'a Record> + Send + 'a,
        N: Iterator<Item = &'a Record> + Send + 'a,
        S: Iterator<Item = &'a Record> + Send + 'a,
        D: Iterator<Item = &'a Record> + Send + 'a,
    {
        response_handle
            .send_response(response)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to send response: {}", e);
                Self::fallback_servfail(request)
            })
    }

    fn fallback_servfail(request: &Request) -> ResponseInfo {
        let mut header = Header {
            metadata: request.metadata,
            counts: HeaderCounts::default(),
        };
        header.metadata.response_code = ResponseCode::ServFail;
        header.into()
    }

    /// Blackholes the request by returning an NXDOMAIN response.
    ///
    /// WHY NXDOMAIN vs 0.0.0.0:
    /// Returning a fake 0.0.0.0 IP causes modern clients (especially web browsers)
    /// to attempt a TCP handshake on port 443 for HTTPS. This leaves the socket hanging
    /// until timeout, slowing down page loads. NXDOMAIN forces an immediate DNS-level
    /// failure, closing the loop instantly.
    async fn send_nxdomain<R: ResponseHandler>(
        request: &Request,
        response_handle: R,
    ) -> ResponseInfo {
        let builder = MessageResponseBuilder::from_message_request(request);
        let response = builder.error_msg(&request.metadata, ResponseCode::NXDomain);
        Self::safe_send(request, response_handle, response).await
    }
}
