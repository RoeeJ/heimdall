//! DNS Transport Layer Implementation
//!
//! This module provides support for various DNS transport protocols:
//! - Traditional UDP and TCP (RFC 1035)
//! - DNS-over-TLS (DoT) (RFC 7858)
//! - DNS-over-HTTPS (DoH) (RFC 8484)
//! - Future: DNS-over-QUIC (DoQ) (Draft)

pub mod cert_gen;
pub mod doh;
pub mod dot;
pub mod manager;
pub mod tls;

pub use doh::DohServer;
pub use dot::DotServer;
pub use tls::{TlsConfig, TlsError};

use std::net::SocketAddr;

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
            TransportProtocol::Https => 943, // Custom port for Heimdall DoH
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
            enable_dot: true, // Enable DoT by default
            dot_bind_addr: Some("0.0.0.0:8853".parse().expect("Valid DoT address")),
            enable_doh: true, // Enable DoH by default
            doh_bind_addr: Some("0.0.0.0:8943".parse().expect("Valid DoH address")),
            tls_config: Some(TlsConfig::default()), // Use default TLS config with auto-generation
            doh_path: "/dns-query".to_string(),
            doh_enable_well_known: true,
            doh_enable_json_api: true,
            max_connections: 1000,
            connection_timeout: std::time::Duration::from_secs(30),
            keepalive_timeout: std::time::Duration::from_secs(300), // 5 minutes
        }
    }
}

// Re-export the new TransportManager
pub use manager::TransportManager;
