//! DNS Transport Layer Implementation
//!
//! This module provides support for various DNS transport protocols:
//! - Traditional UDP and TCP (RFC 1035)
//! - DNS-over-TLS (DoT) (RFC 7858)
//! - Future: DNS-over-HTTPS (DoH) (RFC 8484)
//! - Future: DNS-over-QUIC (DoQ) (Draft)

pub mod doh;
pub mod dot;
pub mod tls;

pub use doh::DohServer;
pub use dot::DotServer;
pub use tls::{TlsConfig, TlsError};

use std::net::SocketAddr;
use tokio::sync::broadcast;

/// Transport protocol types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Udp,
    Tcp,
    Tls,
    Https,
    Quic,
}

impl TransportProtocol {
    /// Get the default port for this transport protocol
    pub fn default_port(&self) -> u16 {
        match self {
            TransportProtocol::Udp | TransportProtocol::Tcp => 53,
            TransportProtocol::Tls => 853,
            TransportProtocol::Https => 443,
            TransportProtocol::Quic => 853,
        }
    }

    /// Check if this transport requires encryption
    pub fn is_encrypted(&self) -> bool {
        matches!(
            self,
            TransportProtocol::Tls | TransportProtocol::Https | TransportProtocol::Quic
        )
    }

    /// Check if this transport uses connection-oriented protocol
    pub fn is_connection_oriented(&self) -> bool {
        matches!(
            self,
            TransportProtocol::Tcp | TransportProtocol::Tls | TransportProtocol::Https
        )
    }
}

/// Transport layer configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Enable DNS-over-TLS server
    pub enable_dot: bool,

    /// DoT server bind address
    pub dot_bind_addr: Option<SocketAddr>,

    /// Enable DNS-over-HTTPS server
    pub enable_doh: bool,

    /// DoH server bind address
    pub doh_bind_addr: Option<SocketAddr>,

    /// TLS configuration for DoT/DoH
    pub tls_config: Option<TlsConfig>,

    /// DoH-specific configuration
    pub doh_path: String,

    /// Enable DoH well-known URI support
    pub doh_enable_well_known: bool,

    /// Enable DoH JSON API support
    pub doh_enable_json_api: bool,

    /// Maximum concurrent connections per transport
    pub max_connections: usize,

    /// Connection timeout for encrypted transports
    pub connection_timeout: std::time::Duration,

    /// Keep-alive timeout for persistent connections
    pub keepalive_timeout: std::time::Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            enable_dot: false,
            dot_bind_addr: Some("127.0.0.1:853".parse().expect("Valid DoT address")),
            enable_doh: false,
            doh_bind_addr: Some("127.0.0.1:443".parse().expect("Valid DoH address")),
            tls_config: None,
            doh_path: "/dns-query".to_string(),
            doh_enable_well_known: true,
            doh_enable_json_api: true,
            max_connections: 1000,
            connection_timeout: std::time::Duration::from_secs(30),
            keepalive_timeout: std::time::Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Transport layer manager for all DNS protocols
pub struct TransportManager {
    config: TransportConfig,
    shutdown_tx: broadcast::Sender<()>,
}

impl TransportManager {
    /// Create a new transport manager
    pub fn new(config: TransportConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            config,
            shutdown_tx,
        }
    }

    /// Start all configured transport servers
    pub async fn start_servers(
        &self,
        resolver: std::sync::Arc<crate::resolver::DnsResolver>,
        metrics: Option<std::sync::Arc<crate::metrics::DnsMetrics>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut tasks = Vec::new();

        // Start DoT server if enabled
        if self.config.enable_dot {
            if let (Some(bind_addr), Some(tls_config)) =
                (&self.config.dot_bind_addr, &self.config.tls_config)
            {
                let dot_server = DotServer::new(
                    *bind_addr,
                    tls_config.clone(),
                    resolver.clone(),
                    metrics.clone(),
                    self.config.max_connections,
                    self.config.connection_timeout,
                    self.config.keepalive_timeout,
                )?;

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                let dot_task = tokio::spawn(async move {
                    tokio::select! {
                        result = dot_server.run() => {
                            if let Err(e) = result {
                                tracing::error!("DoT server error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("DoT server shutting down");
                        }
                    }
                });
                tasks.push(dot_task);
            } else {
                tracing::warn!("DoT enabled but missing bind address or TLS config");
            }
        }

        // Start DoH server if enabled
        if self.config.enable_doh {
            if let Some(bind_addr) = self.config.doh_bind_addr {
                let doh_config = crate::transport::doh::DohServerConfig {
                    enable_tls: self.config.tls_config.is_some(),
                    path: self.config.doh_path.clone(),
                    enable_well_known: self.config.doh_enable_well_known,
                    enable_json_api: self.config.doh_enable_json_api,
                    max_connections: self.config.max_connections,
                    ..Default::default()
                };

                let doh_server = DohServer::new(
                    bind_addr,
                    self.config.tls_config.clone(),
                    resolver.clone(),
                    metrics.clone(),
                    doh_config,
                )?;

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                let doh_task = tokio::spawn(async move {
                    tokio::select! {
                        result = doh_server.run() => {
                            if let Err(e) = result {
                                tracing::error!("DoH server error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("DoH server shutting down");
                        }
                    }
                });
                tasks.push(doh_task);
            } else {
                tracing::warn!("DoH enabled but no bind address specified");
            }
        }

        // Wait for all transport servers to complete
        for task in tasks {
            if let Err(e) = task.await {
                tracing::error!("Transport server task error: {}", e);
            }
        }

        Ok(())
    }

    /// Gracefully shutdown all transport servers
    pub async fn shutdown(&self) {
        tracing::info!("Shutting down transport layer");
        let _ = self.shutdown_tx.send(());
    }
}
