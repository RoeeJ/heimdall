use crate::config::DnsConfig;
use crate::dns::DNSPacket;
use crate::error::{DnsError, Result};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, error, info, trace, warn};

static QUERY_ID_COUNTER: AtomicU16 = AtomicU16::new(1);

#[derive(Debug)]
pub struct DnsResolver {
    config: DnsConfig,
    client_socket: UdpSocket,
}

impl DnsResolver {
    pub async fn new(config: DnsConfig) -> Result<Self> {
        // Bind to a random port for upstream queries
        let client_socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(DnsError::Io)?;
        
        info!("DNS resolver initialized with {} upstream servers", 
            config.upstream_servers.len());
        debug!("Upstream servers: {:?}", config.upstream_servers);
        
        Ok(Self {
            config,
            client_socket,
        })
    }
    
    /// Resolve a DNS query by forwarding it to upstream servers
    pub async fn resolve(&self, mut query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
        // Generate a new query ID for upstream request
        let upstream_id = QUERY_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        query.header.id = upstream_id;
        
        debug!("Resolving query: original_id={}, upstream_id={}, questions={}", 
            original_id, upstream_id, query.header.qdcount);
        
        // Try each upstream server
        let mut last_error = None;
        
        for (attempt, &upstream_addr) in self.config.upstream_servers.iter().enumerate() {
            match self.query_upstream(&query, upstream_addr).await {
                Ok(mut response) => {
                    // Restore original query ID
                    response.header.id = original_id;
                    info!("Successfully resolved query from upstream {} (attempt {})", 
                        upstream_addr, attempt + 1);
                    return Ok(response);
                }
                Err(e) => {
                    warn!("Failed to resolve from upstream {}: {:?}", upstream_addr, e);
                    last_error = Some(e);
                    
                    // If this isn't the last server, continue to next
                    if attempt < self.config.upstream_servers.len() - 1 {
                        continue;
                    }
                }
            }
        }
        
        // All upstream servers failed
        error!("All upstream servers failed to resolve query");
        Err(last_error.unwrap_or(DnsError::Parse("No upstream servers available".to_string())))
    }
    
    /// Query a specific upstream server
    async fn query_upstream(&self, query: &DNSPacket, upstream_addr: SocketAddr) -> Result<DNSPacket> {
        // Serialize the query
        let query_bytes = query.serialize()
            .map_err(|e| DnsError::Parse(format!("Failed to serialize query: {:?}", e)))?;
        
        trace!("Sending {} bytes to upstream {}", query_bytes.len(), upstream_addr);
        
        // Send query with retries
        for retry in 0..=self.config.max_retries {
            match self.send_query_with_timeout(&query_bytes, upstream_addr).await {
                Ok(response) => {
                    if retry > 0 {
                        debug!("Query succeeded on retry {}", retry);
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if retry < self.config.max_retries {
                        debug!("Query attempt {} failed, retrying: {:?}", retry + 1, e);
                        // Brief delay before retry
                        tokio::time::sleep(Duration::from_millis(100 * (retry + 1) as u64)).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        
        unreachable!("Loop should have returned")
    }
    
    /// Send query with timeout
    async fn send_query_with_timeout(&self, query_bytes: &[u8], upstream_addr: SocketAddr) -> Result<DNSPacket> {
        let query_future = async {
            // Send the query
            self.client_socket.send_to(query_bytes, upstream_addr).await
                .map_err(DnsError::Io)?;
            
            // Wait for response
            let mut response_buf = vec![0u8; 4096];
            let (response_len, response_addr) = self.client_socket.recv_from(&mut response_buf).await
                .map_err(DnsError::Io)?;
            
            // Verify response came from the server we queried
            if response_addr != upstream_addr {
                return Err(DnsError::InvalidPacket(
                    format!("Response from unexpected address: {}", response_addr)
                ));
            }
            
            // Log the raw response for debugging
            trace!("Raw response data ({} bytes): {:02x?}", response_len, &response_buf[..response_len.min(64)]);
            
            // Parse the response
            let response = DNSPacket::parse(&response_buf[..response_len])
                .map_err(|e| {
                    // Log more details about the parsing failure
                    debug!("Failed to parse response from {}: {:?}", upstream_addr, e);
                    debug!("Response length: {} bytes", response_len);
                    debug!("First 64 bytes: {:02x?}", &response_buf[..response_len.min(64)]);
                    DnsError::Parse(format!("Failed to parse response: {:?}", e))
                })?;
            
            // Log parsed response details
            debug!("Parsed response: questions={}, answers={}, authorities={}, additional={}",
                response.header.qdcount, response.header.ancount, response.header.nscount, response.header.arcount);
            
            for (i, answer) in response.answers.iter().enumerate() {
                debug!("Answer {}: type={:?}, class={:?}, ttl={}, rdlength={}, rdata={:02x?}",
                    i, answer.rtype, answer.rclass, answer.ttl, answer.rdlength, 
                    &answer.rdata[..answer.rdata.len().min(16)]);
            }
            
            trace!("Received response: {} bytes, {} answers", 
                response_len, response.header.ancount);
            
            Ok(response)
        };
        
        // Apply timeout
        timeout(self.config.upstream_timeout, query_future)
            .await
            .map_err(|_| DnsError::Parse("Upstream query timeout".to_string()))?
    }
    
    /// Create a SERVFAIL response for when resolution fails
    pub fn create_servfail_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true;   // This is a response
        response.header.ra = true;   // Recursion available
        response.header.rcode = 2;   // SERVFAIL
        response.header.ancount = 0; // No answers
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records
        
        // Clear answer sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();
        
        response
    }
    
    /// Create a NXDOMAIN response for non-existent domains
    pub fn create_nxdomain_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true;   // This is a response
        response.header.ra = true;   // Recursion available
        response.header.rcode = 3;   // NXDOMAIN
        response.header.ancount = 0; // No answers
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records
        
        // Clear answer sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();
        
        response
    }
}