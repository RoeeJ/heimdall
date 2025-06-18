use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tracing::{debug, error, info, warn};

use crate::config::DnsConfig;
use crate::dns::{DNSPacket, DNSRcode};
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::pool::{BufferPool, PooledItem};
use crate::resolver::DnsResolver;
use crate::transport::tls::TlsConfig;

use super::{
    ConnectionState, MetricEvent, MetricsRecorder, PermitManager, ProtocolHandler, QueryProcessor,
    RateLimiter, ResponseStatus, StandardMetricsRecorder,
};

#[derive(Debug)]
pub struct DotConnectionState {
    id: u64,
    addr: SocketAddr,
    stream: TlsStream<TcpStream>,
    last_activity: Instant,
    queries_processed: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl DotConnectionState {
    pub fn new(id: u64, addr: SocketAddr, stream: TlsStream<TcpStream>) -> Self {
        Self {
            id,
            addr,
            stream,
            last_activity: Instant::now(),
            queries_processed: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
}

impl ConnectionState for DotConnectionState {
    fn id(&self) -> u64 {
        self.id
    }

    fn last_activity(&self) -> Instant {
        self.last_activity
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    fn is_idle(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

pub struct DotProtocolHandler {
    listener: Arc<TcpListener>,
    tls_acceptor: Arc<TlsAcceptor>,
    _config: Arc<DnsConfig>,
    buffer_pool: Arc<BufferPool>,
    rate_limiter: Arc<RateLimiter>,
    permit_manager: Arc<PermitManager>,
    query_processor: Arc<QueryProcessor>,
    metrics_recorder: StandardMetricsRecorder,
    connection_timeout: Duration,
    keepalive_timeout: Duration,
    max_message_size: usize,
}

impl DotProtocolHandler {
    pub fn new(
        listener: Arc<TcpListener>,
        tls_config: TlsConfig,
        config: Arc<DnsConfig>,
        buffer_pool: Arc<BufferPool>,
        resolver: Arc<DnsResolver>,
        metrics: Arc<DnsMetrics>,
    ) -> Result<Self> {
        let rate_limiter = Arc::new(RateLimiter::new(super::rate_limiter::RateLimitConfig {
            enabled: config.rate_limit_config.enable_rate_limiting,
            queries_per_second_per_ip: config.rate_limit_config.queries_per_second_per_ip,
            burst_size: config.rate_limit_config.burst_size_per_ip,
            cleanup_interval: Duration::from_secs(
                config.rate_limit_config.cleanup_interval_seconds,
            ),
        }));

        let permit_manager = Arc::new(PermitManager::new(config.max_concurrent_queries, "DoT"));

        let query_processor = Arc::new(QueryProcessor::new(buffer_pool.clone(), resolver, metrics));

        // Create TLS acceptor from config
        let tls_acceptor = Arc::new(
            tls_config
                .create_acceptor_sync()
                .map_err(|e| DnsError::Io(format!("Failed to create TLS acceptor: {}", e)))?,
        );

        Ok(Self {
            listener,
            tls_acceptor,
            _config: config,
            buffer_pool,
            rate_limiter,
            permit_manager,
            query_processor,
            metrics_recorder: StandardMetricsRecorder,
            connection_timeout: Duration::from_secs(30),
            keepalive_timeout: Duration::from_secs(300), // 5 minutes
            max_message_size: 65535,
        })
    }

    pub async fn run_server(self: Arc<Self>, metrics: Arc<DnsMetrics>) -> Result<()> {
        info!("Starting DoT server on {}", self.listener.local_addr()?);

        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let handler = self.clone();
                    let metrics = metrics.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handler.handle_dot_connection(stream, addr, &metrics).await
                        {
                            error!("Failed to handle DoT connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("DoT accept error: {}", e);
                    return Err(DnsError::from(e));
                }
            }
        }
    }

    async fn handle_dot_connection(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
        metrics: &DnsMetrics,
    ) -> Result<()> {
        // Record connection established
        self.record_metrics(
            metrics,
            MetricEvent::ConnectionEstablished {
                protocol: "DoT".to_string(),
            },
        );

        // Check rate limit
        if let Err(e) = self.check_rate_limit(addr).await {
            self.record_metrics(
                metrics,
                MetricEvent::ResponseSent {
                    protocol: "DoT".to_string(),
                    status: ResponseStatus::RateLimited,
                },
            );
            return Err(e);
        }

        // Perform TLS handshake with timeout
        let tls_stream =
            match timeout(self.connection_timeout, self.tls_acceptor.accept(stream)).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    warn!("TLS handshake failed from {}: {}", addr, e);
                    return Err(DnsError::Io(format!("TLS handshake failed: {}", e)));
                }
                Err(_) => {
                    warn!("TLS handshake timeout from {}", addr);
                    return Err(DnsError::Timeout);
                }
            };

        debug!("TLS handshake completed for {}", addr);

        // Acquire permit for the entire connection
        let _permit = self.acquire_permit().await?;

        // Create connection state
        let mut conn_state = DotConnectionState::new(0, addr, tls_stream);

        // Handle multiple queries on the same connection
        loop {
            // Set keepalive timeout
            let result = timeout(
                self.keepalive_timeout,
                self.handle_single_query(&mut conn_state, metrics),
            )
            .await;

            match result {
                Ok(Ok(())) => {
                    // Query processed successfully, continue
                    continue;
                }
                Ok(Err(e)) => {
                    // Error processing query
                    debug!("Error processing DoT query: {}", e);
                    break;
                }
                Err(_) => {
                    // Keepalive timeout
                    debug!("DoT connection idle timeout");
                    break;
                }
            }
        }

        // Record connection closed
        self.record_metrics(
            metrics,
            MetricEvent::ConnectionClosed {
                protocol: "DoT".to_string(),
            },
        );

        info!(
            "DoT connection closed from {} - queries: {}, bytes in: {}, bytes out: {}",
            addr, conn_state.queries_processed, conn_state.bytes_received, conn_state.bytes_sent
        );

        Ok(())
    }

    async fn handle_single_query(
        &self,
        conn_state: &mut DotConnectionState,
        metrics: &DnsMetrics,
    ) -> Result<()> {
        // Read message length (2 bytes)
        let mut len_buf = [0u8; 2];
        match conn_state.stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Connection closed by client
                return Err(DnsError::Io("Connection closed".to_string()));
            }
            Err(e) => {
                error!("Failed to read DoT message length: {}", e);
                return Err(DnsError::from(e));
            }
        }

        let msg_len = u16::from_be_bytes(len_buf) as usize;
        if msg_len == 0 || msg_len > self.max_message_size {
            warn!("Invalid DoT message length: {}", msg_len);
            return Err(DnsError::Parse("Invalid message length".to_string()));
        }

        // Read the DNS message
        let mut msg_buf = vec![0u8; msg_len];
        if let Err(e) = conn_state.stream.read_exact(&mut msg_buf).await {
            error!("Failed to read DoT message: {}", e);
            return Err(DnsError::from(e));
        }

        // Update connection stats
        conn_state.update_activity();
        conn_state.bytes_received += (msg_len + 2) as u64;

        // Record bytes received
        self.record_metrics(
            metrics,
            MetricEvent::BytesReceived {
                protocol: "DoT".to_string(),
                bytes: msg_len + 2,
            },
        );

        // Process the query
        match self
            .process_dot_query(&msg_buf, conn_state.addr, metrics)
            .await
        {
            Ok(response) => {
                // Send response with length prefix
                let response_len = response.len() as u16;
                let mut dot_response = Vec::with_capacity(2 + response.len());
                dot_response.extend_from_slice(&response_len.to_be_bytes());
                dot_response.extend_from_slice(&response);

                if let Err(e) = conn_state.stream.write_all(&dot_response).await {
                    error!("Failed to send DoT response: {}", e);
                    return Err(DnsError::from(e));
                }

                // Update stats
                conn_state.queries_processed += 1;
                conn_state.bytes_sent += dot_response.len() as u64;

                // Record metrics
                self.record_metrics(
                    metrics,
                    MetricEvent::BytesSent {
                        protocol: "DoT".to_string(),
                        bytes: dot_response.len(),
                    },
                );

                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "DoT".to_string(),
                        status: ResponseStatus::Success,
                    },
                );
            }
            Err(e) => {
                error!(
                    "Failed to process DoT query from {}: {}",
                    conn_state.addr, e
                );

                // Try to send error response
                if let Ok(query) = DNSPacket::parse(&msg_buf) {
                    let error_response = self.create_error_response(&query, DNSRcode::SERVFAIL);
                    let error_bytes = error_response.to_bytes();
                    let response_len = error_bytes.len() as u16;

                    let mut dot_response = Vec::with_capacity(2 + error_bytes.len());
                    dot_response.extend_from_slice(&response_len.to_be_bytes());
                    dot_response.extend_from_slice(&error_bytes);

                    let _ = conn_state.stream.write_all(&dot_response).await;
                }

                self.record_metrics(
                    metrics,
                    MetricEvent::ResponseSent {
                        protocol: "DoT".to_string(),
                        status: ResponseStatus::Error,
                    },
                );

                return Err(e);
            }
        }

        Ok(())
    }

    async fn process_dot_query(
        &self,
        data: &[u8],
        addr: SocketAddr,
        _metrics: &DnsMetrics,
    ) -> Result<Vec<u8>> {
        self.query_processor.process_query(data, "DoT", addr).await
    }
}

#[async_trait]
impl ProtocolHandler for DotProtocolHandler {
    type Config = DnsConfig;
    type ConnectionState = DotConnectionState;

    fn protocol_name(&self) -> &'static str {
        "DoT"
    }

    fn port(&self) -> u16 {
        853 // Standard DoT port
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
        // This is handled directly in handle_single_query due to DoT's
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
