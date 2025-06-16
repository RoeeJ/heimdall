use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::OwnedSemaphorePermit;
use tracing::{debug, error, info};

use crate::config::DnsConfig;
use crate::dns::DNSPacket;
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::pool::{BufferPool, PooledItem};
use crate::resolver::DnsResolver;

use super::{
    MetricEvent, MetricsRecorder, PermitManager, ProtocolHandler, QueryProcessor, RateLimiter,
    ResponseStatus, StandardMetricsRecorder, connection_manager::StatelessConnection,
};

pub struct DohProtocolHandler {
    listener: Arc<TcpListener>,
    _config: Arc<DnsConfig>,
    buffer_pool: Arc<BufferPool>,
    rate_limiter: Arc<RateLimiter>,
    permit_manager: Arc<PermitManager>,
    query_processor: Arc<QueryProcessor>,
    metrics_recorder: StandardMetricsRecorder,
}

impl DohProtocolHandler {
    pub fn new(
        listener: Arc<TcpListener>,
        config: Arc<DnsConfig>,
        buffer_pool: Arc<BufferPool>,
        resolver: Arc<DnsResolver>,
        metrics: Arc<DnsMetrics>,
    ) -> Self {
        let rate_limiter = Arc::new(RateLimiter::new(super::rate_limiter::RateLimitConfig {
            enabled: config.rate_limit_config.enable_rate_limiting,
            queries_per_second_per_ip: config.rate_limit_config.queries_per_second_per_ip * 2, // Higher for HTTP
            burst_size: config.rate_limit_config.burst_size_per_ip * 2,
            cleanup_interval: Duration::from_secs(
                config.rate_limit_config.cleanup_interval_seconds,
            ),
        }));

        let permit_manager = Arc::new(PermitManager::new(config.max_concurrent_queries, "DoH"));

        let query_processor = Arc::new(QueryProcessor::new(buffer_pool.clone(), resolver, metrics));

        Self {
            listener,
            _config: config,
            buffer_pool,
            rate_limiter,
            permit_manager,
            query_processor,
            metrics_recorder: StandardMetricsRecorder,
        }
    }

    pub async fn run_server(self: Arc<Self>, metrics: Arc<DnsMetrics>) -> Result<()> {
        info!("Starting DoH server on {}", self.listener.local_addr()?);

        // For the protocol handler abstraction, we'll implement a simplified DoH server
        // that handles DNS-over-HTTPS POST requests directly
        loop {
            let (mut stream, addr) = self.listener.accept().await?;
            let handler = self.clone();
            let metrics = metrics.clone();

            tokio::spawn(async move {
                // Read HTTP request (simplified - only handle POST /dns-query)
                let mut buffer = vec![0u8; 4096];
                match tokio::time::timeout(Duration::from_secs(30), stream.read(&mut buffer)).await
                {
                    Ok(Ok(n)) if n > 0 => {
                        // Parse HTTP request (very simplified)
                        let request = String::from_utf8_lossy(&buffer[..n]);

                        // Check if it's a POST to /dns-query
                        if request.starts_with("POST /dns-query") {
                            // Find the DNS message in the body
                            if let Some(body_start) = request.find("\r\n\r\n") {
                                let body_offset = body_start + 4;
                                if body_offset < n {
                                    let dns_data = &buffer[body_offset..n];

                                    // Process the DNS query
                                    match handler.handle_dns_query(dns_data, addr, &metrics).await {
                                        Ok(response) => {
                                            // Send HTTP response
                                            let http_response = format!(
                                                "HTTP/1.1 200 OK\r\n\
                                                Content-Type: application/dns-message\r\n\
                                                Content-Length: {}\r\n\
                                                \r\n",
                                                response.len()
                                            );

                                            let _ =
                                                stream.write_all(http_response.as_bytes()).await;
                                            let _ = stream.write_all(&response).await;
                                        }
                                        Err(e) => {
                                            error!("Failed to process DoH query: {}", e);
                                            let error_response =
                                                b"HTTP/1.1 500 Internal Server Error\r\n\r\n";
                                            let _ = stream.write_all(error_response).await;
                                        }
                                    }
                                }
                            }
                        } else {
                            // Not a supported request
                            let error_response = b"HTTP/1.1 404 Not Found\r\n\r\n";
                            let _ = stream.write_all(error_response).await;
                        }
                    }
                    _ => {
                        // Timeout or error reading
                        debug!("DoH connection timeout or error from {}", addr);
                    }
                }
            });
        }
    }

    async fn handle_dns_query(
        &self,
        data: &[u8],
        addr: SocketAddr,
        metrics: &DnsMetrics,
    ) -> Result<Vec<u8>> {
        // Check rate limit
        if let Err(e) = self.check_rate_limit(addr).await {
            self.record_metrics(
                metrics,
                MetricEvent::ResponseSent {
                    protocol: "DoH".to_string(),
                    status: ResponseStatus::RateLimited,
                },
            );
            return Err(e);
        }

        // Acquire permit
        let _permit = self.acquire_permit().await?;

        // Record query received
        self.record_metrics(
            metrics,
            MetricEvent::QueryReceived {
                protocol: "DoH".to_string(),
            },
        );
        self.record_metrics(
            metrics,
            MetricEvent::BytesReceived {
                protocol: "DoH".to_string(),
                bytes: data.len(),
            },
        );

        // Process query
        match self.query_processor.process_query(data, "DoH", addr).await {
            Ok(response) => {
                self.record_metrics(
                    metrics,
                    MetricEvent::BytesSent {
                        protocol: "DoH".to_string(),
                        bytes: response.len(),
                    },
                );
                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "DoH".to_string(),
                        status: ResponseStatus::Success,
                    },
                );
                Ok(response)
            }
            Err(e) => {
                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "DoH".to_string(),
                        status: ResponseStatus::Error,
                    },
                );
                Err(e)
            }
        }
    }
}

#[async_trait]
impl ProtocolHandler for DohProtocolHandler {
    type Config = DnsConfig;
    type ConnectionState = StatelessConnection;

    fn protocol_name(&self) -> &'static str {
        "DoH"
    }

    fn port(&self) -> u16 {
        8943 // Non-privileged port for DoH
    }

    async fn check_rate_limit(&self, client_addr: SocketAddr) -> Result<()> {
        self.rate_limiter.check_and_consume(client_addr.ip()).await
    }

    async fn acquire_permit(&self) -> Result<OwnedSemaphorePermit> {
        self.permit_manager.acquire().await
    }

    fn get_buffer(&self, size: usize) -> PooledItem<Vec<u8>> {
        let mut buffer = self.buffer_pool.get();
        buffer.resize(size, 0);
        buffer
    }

    async fn parse_query(&self, data: &[u8]) -> Result<DNSPacket> {
        DNSPacket::parse(data).map_err(|e| DnsError::ParseError(e.to_string()))
    }

    async fn process_query(
        &self,
        query: DNSPacket,
        resolver: &DnsResolver,
        _metrics: &DnsMetrics,
        _client_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        resolver.resolve_query(&query).await
    }

    fn serialize_response(&self, response: &DNSPacket) -> Result<Vec<u8>> {
        Ok(response.to_bytes())
    }

    async fn send_response(
        &self,
        _response: Vec<u8>,
        _client: &Self::ConnectionState,
    ) -> Result<()> {
        // This is handled by the HTTP framework
        Ok(())
    }

    fn record_metrics(&self, metrics: &DnsMetrics, event: MetricEvent) {
        self.metrics_recorder.record(metrics, event);
    }

    fn create_error_response(&self, query: &DNSPacket, error_code: u8) -> DNSPacket {
        self.query_processor
            .create_error_response(query, error_code)
    }

    fn validate_query(&self, packet: &DNSPacket) -> Result<()> {
        self.query_processor.validate_query(packet)
    }

    fn handle_special_opcodes(&self, packet: &DNSPacket) -> Option<DNSPacket> {
        self.query_processor.handle_special_opcodes(packet)
    }
}
