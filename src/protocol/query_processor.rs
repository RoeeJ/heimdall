use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, warn};

use crate::dns::{DNSHeader, DNSPacket, DNSRcode};
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::pool::BufferPool;
use crate::resolver::DnsResolver;

pub struct QueryProcessor {
    buffer_pool: Arc<BufferPool>,
    resolver: Arc<DnsResolver>,
    metrics: Arc<DnsMetrics>,
}

impl QueryProcessor {
    pub fn new(
        buffer_pool: Arc<BufferPool>,
        resolver: Arc<DnsResolver>,
        metrics: Arc<DnsMetrics>,
    ) -> Self {
        Self {
            buffer_pool,
            resolver,
            metrics,
        }
    }

    pub async fn process_query(
        &self,
        data: &[u8],
        protocol: &str,
        client_addr: std::net::SocketAddr,
    ) -> Result<Vec<u8>> {
        let start = Instant::now();

        // Parse the query
        let query = match DNSPacket::parse(data) {
            Ok(packet) => packet,
            Err(e) => {
                warn!("Failed to parse DNS query from {}: {}", client_addr, e);
                self.metrics.increment_parse_errors();
                return Err(DnsError::ParseError(e.to_string()));
            }
        };

        debug!(
            "Processing {} query from {} - ID: {}, Questions: {}",
            protocol,
            client_addr,
            query.header.id,
            query.questions.len()
        );

        // Validate query
        if let Err(e) = self.validate_query(&query) {
            warn!("Invalid query from {}: {}", client_addr, e);
            let error_response = self.create_error_response(&query, DNSRcode::FORMERR);
            return Ok(error_response.to_bytes());
        }

        // Handle special opcodes
        if let Some(response) = self.handle_special_opcodes(&query) {
            debug!("Handled special opcode for query {}", query.header.id);
            return Ok(response.to_bytes());
        }

        // Process through resolver
        let response = match self.resolver.resolve_query(&query).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to resolve query {}: {}", query.header.id, e);
                self.metrics.increment_resolution_errors();
                self.create_error_response(&query, DNSRcode::SERVFAIL)
            }
        };

        // Record metrics
        let duration = start.elapsed();
        self.metrics.record_query_duration(duration);
        self.metrics.increment_queries_by_protocol(protocol);

        // Serialize response
        Ok(response.to_bytes())
    }

    pub fn validate_query(&self, packet: &DNSPacket) -> Result<()> {
        // Check if it's a query
        if packet.header.qr {
            return Err(DnsError::ValidationError(
                "Expected query, got response".to_string(),
            ));
        }

        // Check question count
        if packet.questions.is_empty() {
            return Err(DnsError::ValidationError(
                "No questions in query".to_string(),
            ));
        }

        // Validate opcode
        match packet.header.opcode {
            0 | 2 => Ok(()), // QUERY or STATUS
            _ => Err(DnsError::ValidationError(format!(
                "Unsupported opcode: {}",
                packet.header.opcode
            ))),
        }
    }

    pub fn handle_special_opcodes(&self, packet: &DNSPacket) -> Option<DNSPacket> {
        match packet.header.opcode {
            2 => {
                // STATUS opcode
                // Server status request
                let response = DNSPacket {
                    header: DNSHeader {
                        id: packet.header.id,
                        qr: true,
                        opcode: 2, // STATUS
                        aa: false,
                        tc: false,
                        rd: packet.header.rd,
                        ra: true,
                        z: 0,
                        rcode: DNSRcode::NOERROR,
                        qdcount: 0,
                        ancount: 0,
                        nscount: 0,
                        arcount: 0,
                    },
                    ..Default::default()
                };
                Some(response)
            }
            _ => None,
        }
    }

    pub fn create_error_response(&self, query: &DNSPacket, rcode: u8) -> DNSPacket {
        DNSPacket {
            header: DNSHeader {
                id: query.header.id,
                qr: true,
                opcode: query.header.opcode,
                aa: false,
                tc: false,
                rd: query.header.rd,
                ra: true,
                z: 0,
                rcode,
                qdcount: query.questions.len() as u16,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: query.questions.clone(),
            ..Default::default()
        }
    }

    pub fn get_buffer(&self, size: usize) -> crate::pool::PooledItem<Vec<u8>> {
        let mut buffer = self.buffer_pool.get();
        buffer.resize(size, 0);
        buffer
    }
}
