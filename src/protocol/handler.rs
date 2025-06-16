use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::sync::OwnedSemaphorePermit;

use crate::dns::DNSPacket;
use crate::error::Result;
use crate::metrics::DnsMetrics;
use crate::pool::PooledItem;
use crate::resolver::DnsResolver;

use super::{ConnectionState, MetricEvent};

#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// Protocol-specific configuration
    type Config: Send + Sync;

    /// Connection state for stateful protocols (use () for stateless)
    type ConnectionState: ConnectionState + Send + Sync;

    /// Get protocol name for metrics/logging
    fn protocol_name(&self) -> &'static str;

    /// Get protocol port number
    fn port(&self) -> u16;

    /// Check rate limiting for a client
    async fn check_rate_limit(&self, client_addr: SocketAddr) -> Result<()>;

    /// Acquire processing permit (semaphore)
    async fn acquire_permit(&self) -> Result<OwnedSemaphorePermit>;

    /// Get buffer from pool
    fn get_buffer(&self, size: usize) -> PooledItem<Vec<u8>>;

    /// Parse incoming DNS query
    async fn parse_query(&self, data: &[u8]) -> Result<DNSPacket>;

    /// Process DNS query through resolver
    async fn process_query(
        &self,
        query: DNSPacket,
        resolver: &DnsResolver,
        metrics: &DnsMetrics,
        client_addr: SocketAddr,
    ) -> Result<DNSPacket>;

    /// Serialize response to bytes
    fn serialize_response(&self, response: &DNSPacket) -> Result<Vec<u8>>;

    /// Send response to client (protocol-specific)
    async fn send_response(&self, response: Vec<u8>, client: &Self::ConnectionState) -> Result<()>;

    /// Record protocol-specific metrics
    fn record_metrics(&self, metrics: &DnsMetrics, event: MetricEvent);

    /// Create error response for various error conditions
    fn create_error_response(&self, query: &DNSPacket, error_code: u8) -> DNSPacket;

    /// Validate incoming query
    fn validate_query(&self, packet: &DNSPacket) -> Result<()>;

    /// Handle special opcodes (e.g., server status)
    fn handle_special_opcodes(&self, packet: &DNSPacket) -> Option<DNSPacket>;
}
