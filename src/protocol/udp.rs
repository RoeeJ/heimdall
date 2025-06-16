use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::OwnedSemaphorePermit;
use tracing::{debug, error};

use crate::config::DnsConfig;
use crate::dns::{DNSPacket, DNSRcode};
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::pool::{BufferPool, PooledItem};
use crate::resolver::DnsResolver;

use super::connection_manager::StatelessConnection;
use super::{
    MetricEvent, MetricsRecorder, PermitManager, ProtocolHandler, QueryProcessor, RateLimiter,
    ResponseStatus, StandardMetricsRecorder,
};

pub struct UdpProtocolHandler {
    socket: Arc<UdpSocket>,
    config: Arc<DnsConfig>,
    buffer_pool: Arc<BufferPool>,
    rate_limiter: Arc<RateLimiter>,
    permit_manager: Arc<PermitManager>,
    query_processor: Arc<QueryProcessor>,
    metrics_recorder: StandardMetricsRecorder,
}

impl UdpProtocolHandler {
    pub fn new(
        socket: Arc<UdpSocket>,
        config: Arc<DnsConfig>,
        buffer_pool: Arc<BufferPool>,
        resolver: Arc<DnsResolver>,
        metrics: Arc<DnsMetrics>,
    ) -> Self {
        let rate_limiter = Arc::new(RateLimiter::new(super::rate_limiter::RateLimitConfig {
            enabled: config.rate_limit_config.enable_rate_limiting,
            queries_per_second_per_ip: config.rate_limit_config.queries_per_second_per_ip,
            burst_size: config.rate_limit_config.burst_size_per_ip,
            cleanup_interval: std::time::Duration::from_secs(
                config.rate_limit_config.cleanup_interval_seconds,
            ),
        }));

        let permit_manager = Arc::new(PermitManager::new(config.max_concurrent_queries, "UDP"));

        let query_processor = Arc::new(QueryProcessor::new(buffer_pool.clone(), resolver, metrics));

        Self {
            socket,
            config,
            buffer_pool,
            rate_limiter,
            permit_manager,
            query_processor,
            metrics_recorder: StandardMetricsRecorder,
        }
    }

    pub async fn run_server(self: Arc<Self>, metrics: Arc<DnsMetrics>) -> Result<()> {
        let mut buf = vec![0u8; 512]; // Standard DNS UDP size

        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let handler = self.clone();
                    let metrics = metrics.clone();
                    let data = buf[..len].to_vec();

                    tokio::spawn(async move {
                        if let Err(e) = handler.handle_udp_query(data, addr, &metrics).await {
                            error!("Failed to handle UDP query from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("UDP socket error: {}", e);
                    return Err(DnsError::from(e));
                }
            }
        }
    }

    async fn handle_udp_query(
        &self,
        data: Vec<u8>,
        addr: SocketAddr,
        metrics: &DnsMetrics,
    ) -> Result<()> {
        // Record query received
        self.record_metrics(
            metrics,
            MetricEvent::QueryReceived {
                protocol: "UDP".to_string(),
            },
        );
        self.record_metrics(
            metrics,
            MetricEvent::BytesReceived {
                protocol: "UDP".to_string(),
                bytes: data.len(),
            },
        );

        // Check rate limit
        if let Err(e) = self.check_rate_limit(addr).await {
            self.record_metrics(
                metrics,
                MetricEvent::ResponseSent {
                    protocol: "UDP".to_string(),
                    status: ResponseStatus::RateLimited,
                },
            );
            return Err(e);
        }

        // Acquire permit
        let _permit = self.acquire_permit().await?;

        // Create a stateless connection state
        let _conn_state = StatelessConnection::new(0);

        // Process query
        match self.process_query_internal(&data, addr, metrics).await {
            Ok(response) => {
                // Send response via UDP
                self.send_udp_response(&response, addr).await?;

                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "UDP".to_string(),
                        status: ResponseStatus::Success,
                    },
                );
            }
            Err(e) => {
                error!("Failed to process UDP query from {}: {}", addr, e);

                // Try to send error response
                if let Ok(query) = DNSPacket::parse(&data) {
                    let error_response = self.create_error_response(&query, DNSRcode::SERVFAIL);
                    let _ = self
                        .send_udp_response(&error_response.to_bytes(), addr)
                        .await;
                }

                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "UDP".to_string(),
                        status: ResponseStatus::Error,
                    },
                );

                return Err(e);
            }
        }

        Ok(())
    }

    async fn process_query_internal(
        &self,
        data: &[u8],
        addr: SocketAddr,
        _metrics: &DnsMetrics,
    ) -> Result<Vec<u8>> {
        self.query_processor.process_query(data, "UDP", addr).await
    }
}

#[async_trait]
impl ProtocolHandler for UdpProtocolHandler {
    type Config = DnsConfig;
    type ConnectionState = StatelessConnection;

    fn protocol_name(&self) -> &'static str {
        "UDP"
    }

    fn port(&self) -> u16 {
        self.config.bind_addr.port()
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
        // For UDP, we need to get the address from somewhere else
        // This is a limitation of the current design - we'll need to refactor
        // For now, this is handled in handle_udp_query
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

// Helper for actual UDP sending (since we need the address)
impl UdpProtocolHandler {
    pub async fn send_udp_response(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        match self.socket.send_to(data, addr).await {
            Ok(sent) => {
                debug!("Sent {} bytes to {}", sent, addr);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send UDP response to {}: {}", addr, e);
                Err(DnsError::from(e))
            }
        }
    }
}
