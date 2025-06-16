//! DNS-over-TLS (DoT) server implementation
//!
//! Implements RFC 7858: Specification for DNS over Transport Layer Security (TLS)
//!
//! Features:
//! - TLS 1.2/1.3 support with modern cipher suites
//! - Connection pooling and session management
//! - Proper DNS message framing over TLS streams
//! - Client certificate validation (optional)
//! - Connection keep-alive and timeouts
//! - Comprehensive metrics and monitoring

use super::tls::{TlsConfig, TlsError};
use crate::dns::DNSPacket;
use crate::metrics::DnsMetrics;
use crate::resolver::DnsResolver;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tracing::{debug, error, info, trace, warn};

/// DNS-over-TLS server
pub struct DotServer {
    bind_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    resolver: Arc<DnsResolver>,
    metrics: Option<Arc<DnsMetrics>>,
    config: DotServerConfig,
    connection_manager: Arc<ConnectionManager>,
}

/// DoT server configuration
#[derive(Debug, Clone)]
pub struct DotServerConfig {
    /// Maximum number of concurrent connections
    pub max_connections: usize,

    /// Connection timeout for TLS handshake
    pub connection_timeout: Duration,

    /// Keep-alive timeout for idle connections
    pub keepalive_timeout: Duration,

    /// Maximum message size for DoT
    pub max_message_size: usize,

    /// Buffer size for reading/writing
    pub buffer_size: usize,

    /// Connection rate limiting (connections per second per IP)
    pub rate_limit_per_ip: Option<u32>,
}

impl Default for DotServerConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            connection_timeout: Duration::from_secs(30),
            keepalive_timeout: Duration::from_secs(300), // 5 minutes
            max_message_size: 65535,                     // Max DNS message size
            buffer_size: 4096,
            rate_limit_per_ip: Some(10), // 10 connections per second per IP
        }
    }
}

/// Connection state tracking
#[derive(Debug)]
struct ConnectionState {
    remote_addr: SocketAddr,
    established_at: Instant,
    last_activity: Instant,
    queries_processed: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl ConnectionState {
    fn new(remote_addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            remote_addr,
            established_at: now,
            last_activity: now,
            queries_processed: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    fn is_idle(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// Connection manager for tracking and limiting connections
#[derive(Debug)]
struct ConnectionManager {
    /// Active connections by connection ID
    connections: RwLock<HashMap<u64, Arc<Mutex<ConnectionState>>>>,

    /// Connection counter
    next_connection_id: AtomicU64,

    /// Rate limiting per IP
    ip_connection_counts: RwLock<HashMap<SocketAddr, (Instant, u32)>>,

    /// Total connection statistics
    total_connections: AtomicU64,
    total_queries: AtomicU64,
    total_bytes_sent: AtomicU64,
    total_bytes_received: AtomicU64,
}

impl ConnectionManager {
    fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            next_connection_id: AtomicU64::new(1),
            ip_connection_counts: RwLock::new(HashMap::new()),
            total_connections: AtomicU64::new(0),
            total_queries: AtomicU64::new(0),
            total_bytes_sent: AtomicU64::new(0),
            total_bytes_received: AtomicU64::new(0),
        }
    }

    /// Check if a new connection from this IP should be allowed
    async fn check_rate_limit(&self, remote_addr: SocketAddr, limit: Option<u32>) -> bool {
        if let Some(limit) = limit {
            let mut ip_counts = self.ip_connection_counts.write().await;
            let now = Instant::now();

            // Clean up old entries (older than 1 second)
            ip_counts.retain(|_, (timestamp, _)| {
                now.duration_since(*timestamp) < Duration::from_secs(1)
            });

            // Check current rate for this IP
            let (_, count) = ip_counts.entry(remote_addr).or_insert((now, 0));

            if *count >= limit {
                debug!(
                    "Rate limit exceeded for IP: {} ({} connections/sec)",
                    remote_addr, count
                );
                return false;
            }

            *count += 1;
        }

        true
    }

    /// Add a new connection
    async fn add_connection(&self, remote_addr: SocketAddr, max_connections: usize) -> Option<u64> {
        let connections = self.connections.read().await;

        // Check connection limit
        if connections.len() >= max_connections {
            warn!(
                "Connection limit reached: {} connections",
                connections.len()
            );
            return None;
        }

        drop(connections);

        let connection_id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
        let state = Arc::new(Mutex::new(ConnectionState::new(remote_addr)));

        let mut connections = self.connections.write().await;
        connections.insert(connection_id, state);

        self.total_connections.fetch_add(1, Ordering::Relaxed);

        info!("New DoT connection {} from {}", connection_id, remote_addr);
        Some(connection_id)
    }

    /// Remove a connection
    async fn remove_connection(&self, connection_id: u64) {
        let mut connections = self.connections.write().await;

        if let Some(state) = connections.remove(&connection_id) {
            let state = state.lock().await;
            let duration = state.established_at.elapsed();

            info!(
                "DoT connection {} closed after {:?}, processed {} queries, {} bytes sent, {} bytes received",
                connection_id,
                duration,
                state.queries_processed,
                state.bytes_sent,
                state.bytes_received
            );
        }
    }

    /// Get connection state
    async fn get_connection(&self, connection_id: u64) -> Option<Arc<Mutex<ConnectionState>>> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).cloned()
    }

    /// Clean up idle connections
    async fn cleanup_idle_connections(&self, timeout: Duration) {
        let connections = self.connections.read().await;
        let mut to_remove = Vec::new();

        for (&connection_id, state) in connections.iter() {
            let state = state.lock().await;
            if state.is_idle(timeout) {
                to_remove.push(connection_id);
            }
        }

        drop(connections);

        if !to_remove.is_empty() {
            let mut connections = self.connections.write().await;
            for connection_id in to_remove {
                if let Some(state) = connections.remove(&connection_id) {
                    let state = state.lock().await;
                    debug!(
                        "Cleaned up idle DoT connection {} from {}",
                        connection_id, state.remote_addr
                    );
                }
            }
        }
    }

    /// Get connection statistics
    async fn get_stats(&self) -> DotServerStats {
        let connections = self.connections.read().await;
        let active_connections = connections.len();

        DotServerStats {
            active_connections,
            total_connections: self.total_connections.load(Ordering::Relaxed),
            total_queries: self.total_queries.load(Ordering::Relaxed),
            total_bytes_sent: self.total_bytes_sent.load(Ordering::Relaxed),
            total_bytes_received: self.total_bytes_received.load(Ordering::Relaxed),
        }
    }
}

/// DoT server statistics
#[derive(Debug, Clone)]
pub struct DotServerStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub total_queries: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
}

impl DotServer {
    /// Create a new DNS-over-TLS server
    pub async fn new(
        bind_addr: SocketAddr,
        tls_config: TlsConfig,
        resolver: Arc<DnsResolver>,
        metrics: Option<Arc<DnsMetrics>>,
        max_connections: usize,
        connection_timeout: Duration,
        keepalive_timeout: Duration,
    ) -> Result<Self, TlsError> {
        let tls_acceptor = tls_config.create_acceptor().await?;

        let config = DotServerConfig {
            max_connections,
            connection_timeout,
            keepalive_timeout,
            ..Default::default()
        };

        Ok(Self {
            bind_addr,
            tls_acceptor,
            resolver,
            metrics,
            config,
            connection_manager: Arc::new(ConnectionManager::new()),
        })
    }

    /// Run the DoT server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Start the TCP listener
        let listener = TcpListener::bind(self.bind_addr).await?;
        info!("DNS-over-TLS server listening on {}", self.bind_addr);

        // Start cleanup task for idle connections
        let connection_manager = Arc::clone(&self.connection_manager);
        let cleanup_timeout = self.config.keepalive_timeout;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Cleanup every minute
            loop {
                interval.tick().await;
                connection_manager
                    .cleanup_idle_connections(cleanup_timeout)
                    .await;
            }
        });

        // Accept connections
        loop {
            match listener.accept().await {
                Ok((stream, remote_addr)) => {
                    // Rate limiting check
                    if !self
                        .connection_manager
                        .check_rate_limit(remote_addr, self.config.rate_limit_per_ip)
                        .await
                    {
                        debug!(
                            "Rate limit exceeded, dropping connection from {}",
                            remote_addr
                        );
                        continue;
                    }

                    // Connection limit check
                    if let Some(connection_id) = self
                        .connection_manager
                        .add_connection(remote_addr, self.config.max_connections)
                        .await
                    {
                        let tls_acceptor = self.tls_acceptor.clone();
                        let resolver = Arc::clone(&self.resolver);
                        let metrics = self.metrics.clone();
                        let config = self.config.clone();
                        let connection_manager = Arc::clone(&self.connection_manager);

                        // Spawn task to handle the connection
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(
                                connection_id,
                                stream,
                                remote_addr,
                                tls_acceptor,
                                resolver,
                                metrics,
                                config,
                                connection_manager.clone(),
                            )
                            .await
                            {
                                error!("DoT connection {} error: {}", connection_id, e);
                            }

                            // Clean up connection
                            connection_manager.remove_connection(connection_id).await;
                        });
                    } else {
                        debug!(
                            "Connection limit reached, dropping connection from {}",
                            remote_addr
                        );
                    }
                }
                Err(e) => {
                    error!("Failed to accept DoT connection: {}", e);
                }
            }
        }
    }

    /// Handle a single DoT connection
    #[allow(clippy::too_many_arguments)]
    async fn handle_connection(
        connection_id: u64,
        stream: TcpStream,
        remote_addr: SocketAddr,
        tls_acceptor: TlsAcceptor,
        resolver: Arc<DnsResolver>,
        metrics: Option<Arc<DnsMetrics>>,
        config: DotServerConfig,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "Starting TLS handshake for connection {} from {}",
            connection_id, remote_addr
        );

        // Perform TLS handshake with timeout
        let tls_stream = timeout(config.connection_timeout, tls_acceptor.accept(stream)).await??;

        info!(
            "TLS handshake completed for connection {} from {}",
            connection_id, remote_addr
        );

        // Record TLS connection metric
        if let Some(metrics) = &metrics {
            metrics.dot_connections_total.inc();
            metrics.dot_active_connections.inc();
        }

        // Handle DNS queries over the TLS connection
        let result = Self::handle_tls_stream(
            connection_id,
            tls_stream,
            remote_addr,
            resolver,
            metrics.clone(),
            config,
            connection_manager,
        )
        .await;

        // Record connection closed metric
        if let Some(metrics) = &metrics {
            metrics.dot_active_connections.dec();
        }

        result
    }

    /// Handle DNS queries over a TLS stream
    async fn handle_tls_stream(
        connection_id: u64,
        mut tls_stream: TlsStream<TcpStream>,
        remote_addr: SocketAddr,
        resolver: Arc<DnsResolver>,
        metrics: Option<Arc<DnsMetrics>>,
        config: DotServerConfig,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut buffer = vec![0u8; config.buffer_size];

        loop {
            // Read DNS message length (2 bytes, network byte order)
            let mut length_buf = [0u8; 2];

            match timeout(
                config.keepalive_timeout,
                tls_stream.read_exact(&mut length_buf),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let message_length = u16::from_be_bytes(length_buf) as usize;

                    // Validate message length
                    if message_length == 0 || message_length > config.max_message_size {
                        warn!(
                            "Invalid DNS message length from {}: {}",
                            remote_addr, message_length
                        );
                        break;
                    }

                    // Resize buffer if needed
                    if buffer.len() < message_length {
                        buffer.resize(message_length, 0);
                    }

                    // Read the DNS message
                    match timeout(
                        config.keepalive_timeout,
                        tls_stream.read_exact(&mut buffer[..message_length]),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            // Update connection state
                            if let Some(state) =
                                connection_manager.get_connection(connection_id).await
                            {
                                let mut state = state.lock().await;
                                state.update_activity();
                                state.bytes_received += (message_length + 2) as u64;
                            }

                            // Process the DNS query
                            let start_time = Instant::now();
                            match Self::process_dns_query(
                                &buffer[..message_length],
                                &resolver,
                                connection_id,
                                remote_addr,
                            )
                            .await
                            {
                                Ok(response_data) => {
                                    let processing_time = start_time.elapsed();

                                    // Send response length and data
                                    let response_length = response_data.len() as u16;
                                    let length_bytes = response_length.to_be_bytes();

                                    if let Err(e) = tls_stream.write_all(&length_bytes).await {
                                        error!(
                                            "Failed to write response length to DoT connection {}: {}",
                                            connection_id, e
                                        );
                                        break;
                                    }

                                    if let Err(e) = tls_stream.write_all(&response_data).await {
                                        error!(
                                            "Failed to write response data to DoT connection {}: {}",
                                            connection_id, e
                                        );
                                        break;
                                    }

                                    // Update connection state
                                    if let Some(state) =
                                        connection_manager.get_connection(connection_id).await
                                    {
                                        let mut state = state.lock().await;
                                        state.queries_processed += 1;
                                        state.bytes_sent += (response_data.len() + 2) as u64;
                                    }

                                    // Update metrics
                                    if let Some(metrics) = &metrics {
                                        metrics.dot_queries_total.inc();
                                        metrics
                                            .dot_query_duration
                                            .with_label_values(&["success"])
                                            .observe(processing_time.as_secs_f64());
                                    }

                                    connection_manager
                                        .total_queries
                                        .fetch_add(1, Ordering::Relaxed);
                                    connection_manager.total_bytes_sent.fetch_add(
                                        (response_data.len() + 2) as u64,
                                        Ordering::Relaxed,
                                    );
                                    connection_manager
                                        .total_bytes_received
                                        .fetch_add((message_length + 2) as u64, Ordering::Relaxed);

                                    trace!(
                                        "DoT query processed for connection {} in {:?}",
                                        connection_id, processing_time
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to process DNS query for DoT connection {}: {}",
                                        connection_id, e
                                    );

                                    // Update error metrics
                                    if let Some(metrics) = &metrics {
                                        metrics.dot_errors_total.inc();
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            debug!(
                                "Failed to read DNS message from DoT connection {}: {}",
                                connection_id, e
                            );
                            break;
                        }
                        Err(_) => {
                            debug!(
                                "DoT connection {} timed out waiting for DNS message",
                                connection_id
                            );
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    debug!(
                        "Failed to read message length from DoT connection {}: {}",
                        connection_id, e
                    );
                    break;
                }
                Err(_) => {
                    debug!(
                        "DoT connection {} timed out waiting for message length",
                        connection_id
                    );
                    break;
                }
            }
        }

        debug!(
            "DoT connection {} from {} closed",
            connection_id, remote_addr
        );
        Ok(())
    }

    /// Process a DNS query and return the response
    async fn process_dns_query(
        query_data: &[u8],
        resolver: &DnsResolver,
        connection_id: u64,
        remote_addr: SocketAddr,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        // Parse the DNS query
        let query = DNSPacket::parse(query_data)?;
        let original_id = query.header.id;

        trace!(
            "DoT connection {} processing query ID {} from {}",
            connection_id, original_id, remote_addr
        );

        // Resolve the query using the resolver
        let response = resolver.resolve(query, original_id).await?;

        // Serialize the response
        let response_data = response.serialize()?;

        Ok(response_data)
    }

    /// Get server statistics
    pub async fn get_stats(&self) -> DotServerStats {
        self.connection_manager.get_stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_server_config_default() {
        let config = DotServerConfig::default();
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert_eq!(config.keepalive_timeout, Duration::from_secs(300));
        assert_eq!(config.max_message_size, 65535);
        assert_eq!(config.buffer_size, 4096);
        assert_eq!(config.rate_limit_per_ip, Some(10));
    }

    #[test]
    fn test_connection_state() {
        let addr = "127.0.0.1:12345".parse().unwrap();
        let mut state = ConnectionState::new(addr);

        assert_eq!(state.remote_addr, addr);
        assert_eq!(state.queries_processed, 0);
        assert_eq!(state.bytes_sent, 0);
        assert_eq!(state.bytes_received, 0);

        state.update_activity();
        assert!(!state.is_idle(Duration::from_secs(1)));

        // Simulate passage of time by manually setting last_activity
        state.last_activity = Instant::now() - Duration::from_secs(10);
        assert!(state.is_idle(Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn test_connection_manager() {
        let manager = ConnectionManager::new();
        let addr = "127.0.0.1:12345".parse().unwrap();

        // Test rate limiting
        assert!(manager.check_rate_limit(addr, Some(5)).await);

        // Test connection management
        let conn_id = manager.add_connection(addr, 100).await.unwrap();
        assert_eq!(conn_id, 1);

        let state = manager.get_connection(conn_id).await.unwrap();
        {
            let mut state = state.lock().await;
            state.queries_processed = 5;
            state.bytes_sent = 1000;
            state.bytes_received = 500;
        }

        manager.remove_connection(conn_id).await;
        assert!(manager.get_connection(conn_id).await.is_none());
    }
}
