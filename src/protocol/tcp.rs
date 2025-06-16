use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
use tracing::{error, warn};

use crate::config::DnsConfig;
use crate::dns::{DNSPacket, DNSRcode};
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::pool::{BufferPool, PooledItem};
use crate::resolver::DnsResolver;

use super::{
    ConnectionState, MetricEvent, MetricsRecorder, PermitManager, ProtocolHandler, QueryProcessor,
    RateLimiter, ResponseStatus, StandardMetricsRecorder,
};

#[derive(Debug)]
pub struct TcpConnectionState {
    id: u64,
    _addr: SocketAddr,
    _stream: TcpStream,
    last_activity: Instant,
}

impl TcpConnectionState {
    pub fn new(id: u64, addr: SocketAddr, stream: TcpStream) -> Self {
        Self {
            id,
            _addr: addr,
            _stream: stream,
            last_activity: Instant::now(),
        }
    }
}

impl ConnectionState for TcpConnectionState {
    fn id(&self) -> u64 {
        self.id
    }

    fn last_activity(&self) -> Instant {
        self.last_activity
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    fn is_idle(&self, timeout: std::time::Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

pub struct TcpProtocolHandler {
    listener: Arc<TcpListener>,
    config: Arc<DnsConfig>,
    buffer_pool: Arc<BufferPool>,
    rate_limiter: Arc<RateLimiter>,
    permit_manager: Arc<PermitManager>,
    query_processor: Arc<QueryProcessor>,
    metrics_recorder: StandardMetricsRecorder,
}

impl TcpProtocolHandler {
    pub fn new(
        listener: Arc<TcpListener>,
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

        let permit_manager = Arc::new(PermitManager::new(config.max_concurrent_queries, "TCP"));

        let query_processor = Arc::new(QueryProcessor::new(buffer_pool.clone(), resolver, metrics));

        Self {
            listener,
            config,
            buffer_pool,
            rate_limiter,
            permit_manager,
            query_processor,
            metrics_recorder: StandardMetricsRecorder,
        }
    }

    pub async fn run_server(self: Arc<Self>, metrics: Arc<DnsMetrics>) -> Result<()> {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let handler = self.clone();
                    let metrics = metrics.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handler.handle_tcp_connection(stream, addr, &metrics).await
                        {
                            error!("Failed to handle TCP connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("TCP accept error: {}", e);
                    return Err(DnsError::from(e));
                }
            }
        }
    }

    async fn handle_tcp_connection(
        &self,
        mut stream: TcpStream,
        addr: SocketAddr,
        metrics: &DnsMetrics,
    ) -> Result<()> {
        // Record connection established
        self.record_metrics(
            metrics,
            MetricEvent::ConnectionEstablished {
                protocol: "TCP".to_string(),
            },
        );

        // Check rate limit
        if let Err(e) = self.check_rate_limit(addr).await {
            self.record_metrics(
                metrics,
                MetricEvent::ResponseSent {
                    protocol: "TCP".to_string(),
                    status: ResponseStatus::RateLimited,
                },
            );
            return Err(e);
        }

        // Acquire permit for the entire connection
        let _permit = self.acquire_permit().await?;

        // Handle multiple queries on the same connection
        loop {
            // Read message length (2 bytes)
            let mut len_buf = [0u8; 2];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Connection closed by client
                    break;
                }
                Err(e) => {
                    error!("Failed to read TCP message length: {}", e);
                    break;
                }
            }

            let msg_len = u16::from_be_bytes(len_buf) as usize;
            if msg_len == 0 || msg_len > 65535 {
                warn!("Invalid TCP message length: {}", msg_len);
                break;
            }

            // Read the DNS message
            let mut msg_buf = vec![0u8; msg_len];
            if let Err(e) = stream.read_exact(&mut msg_buf).await {
                error!("Failed to read TCP message: {}", e);
                break;
            }

            // Record bytes received
            self.record_metrics(
                metrics,
                MetricEvent::BytesReceived {
                    protocol: "TCP".to_string(),
                    bytes: msg_len + 2,
                },
            );

            // Process the query
            match self.process_tcp_query(&msg_buf, addr, metrics).await {
                Ok(response) => {
                    // Send response with length prefix
                    let response_len = response.len() as u16;
                    let mut tcp_response = Vec::with_capacity(2 + response.len());
                    tcp_response.extend_from_slice(&response_len.to_be_bytes());
                    tcp_response.extend_from_slice(&response);

                    if let Err(e) = stream.write_all(&tcp_response).await {
                        error!("Failed to send TCP response: {}", e);
                        break;
                    }

                    // Record bytes sent
                    self.record_metrics(
                        metrics,
                        MetricEvent::BytesSent {
                            protocol: "TCP".to_string(),
                            bytes: tcp_response.len(),
                        },
                    );

                    self.record_metrics(
                        metrics,
                        MetricEvent::ResponseSent {
                            protocol: "TCP".to_string(),
                            status: ResponseStatus::Success,
                        },
                    );
                }
                Err(e) => {
                    error!("Failed to process TCP query from {}: {}", addr, e);

                    // Try to send error response
                    if let Ok(query) = DNSPacket::parse(&msg_buf) {
                        let error_response = self.create_error_response(&query, DNSRcode::SERVFAIL);
                        let error_bytes = error_response.to_bytes();
                        let response_len = error_bytes.len() as u16;

                        let mut tcp_response = Vec::with_capacity(2 + error_bytes.len());
                        tcp_response.extend_from_slice(&response_len.to_be_bytes());
                        tcp_response.extend_from_slice(&error_bytes);

                        let _ = stream.write_all(&tcp_response).await;
                    }

                    self.record_metrics(
                        metrics,
                        MetricEvent::ResponseSent {
                            protocol: "TCP".to_string(),
                            status: ResponseStatus::Error,
                        },
                    );

                    break;
                }
            }
        }

        // Record connection closed
        self.record_metrics(
            metrics,
            MetricEvent::ConnectionClosed {
                protocol: "TCP".to_string(),
            },
        );

        Ok(())
    }

    async fn process_tcp_query(
        &self,
        data: &[u8],
        addr: SocketAddr,
        _metrics: &DnsMetrics,
    ) -> Result<Vec<u8>> {
        self.query_processor.process_query(data, "TCP", addr).await
    }
}

#[async_trait]
impl ProtocolHandler for TcpProtocolHandler {
    type Config = DnsConfig;
    type ConnectionState = TcpConnectionState;

    fn protocol_name(&self) -> &'static str {
        "TCP"
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
        // This is handled directly in handle_tcp_connection due to TCP's
        // length-prefixed message format
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
