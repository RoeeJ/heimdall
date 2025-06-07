use crate::config::DnsConfig;
use crate::dns::DNSPacket;
use crate::error::{DnsError, Result};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, error, info, trace, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryMode {
    Recursive,
    Iterative,
}

impl QueryMode {
    /// Detect query mode from DNS packet header flags
    pub fn from_packet(packet: &DNSPacket) -> Self {
        if packet.header.rd {
            QueryMode::Recursive
        } else {
            QueryMode::Iterative
        }
    }
}

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
    
    /// Resolve a DNS query with automatic mode detection
    pub async fn resolve(&self, query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
        let query_mode = QueryMode::from_packet(&query);
        
        match query_mode {
            QueryMode::Recursive => self.resolve_recursively(query, original_id).await,
            QueryMode::Iterative => {
                if self.config.enable_iterative {
                    self.resolve_iteratively(query, original_id).await
                } else {
                    // Fall back to recursive if iterative is disabled
                    self.resolve_recursively(query, original_id).await
                }
            }
        }
    }
    
    /// Resolve a DNS query by forwarding it to upstream servers (recursive)
    async fn resolve_recursively(&self, mut query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
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
    
    /// Resolve a DNS query iteratively starting from root servers
    async fn resolve_iteratively(&self, mut query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
        debug!("Starting iterative resolution for query id={}", original_id);
        
        // Get the first question to resolve
        if query.questions.is_empty() {
            return Err(DnsError::InvalidPacket("No questions in query".to_string()));
        }
        
        let question = &query.questions[0];
        let domain_name = question.labels.iter()
            .filter(|l| !l.is_empty())
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join(".");
            
        debug!("Resolving domain: {} iteratively", domain_name);
        
        // Start with root servers
        let mut current_servers = self.config.root_servers.clone();
        let mut iteration = 0;
        let mut last_error = None;
        
        // Generate a new query ID for iterative requests
        let iterative_id = QUERY_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        query.header.id = iterative_id;
        
        while iteration < self.config.max_iterations {
            iteration += 1;
            debug!("Iteration {}: querying {} servers", iteration, current_servers.len());
            
            // Try each server in the current set
            let mut referral_servers = Vec::new();
            
            for &server in &current_servers {
                match self.query_iterative_server(&query, server).await {
                    Ok(response) => {
                        // Check if we got an answer
                        if response.header.ancount > 0 {
                            // We have answers! Restore original ID and return
                            let mut final_response = response;
                            final_response.header.id = original_id;
                            info!("Iterative resolution completed in {} iterations", iteration);
                            return Ok(final_response);
                        }
                        
                        // Check for authoritative no-data or NXDOMAIN
                        if response.header.aa && (response.header.rcode == 3 || response.header.rcode == 0) {
                            // Authoritative response with no data
                            let mut final_response = response;
                            final_response.header.id = original_id;
                            return Ok(final_response);
                        }
                        
                        // Look for referrals in authority section
                        let mut new_servers = self.extract_referral_servers(&response).await;
                        if !new_servers.is_empty() {
                            debug!("Found {} referral servers from {}", new_servers.len(), server);
                            referral_servers.append(&mut new_servers);
                            break; // Use this referral
                        }
                    }
                    Err(e) => {
                        warn!("Failed to query iterative server {}: {:?}", server, e);
                        last_error = Some(e);
                        continue;
                    }
                }
            }
            
            // If we found referral servers, use them for the next iteration
            if !referral_servers.is_empty() {
                current_servers = referral_servers;
                continue;
            }
            
            // No more referrals found, resolution failed
            break;
        }
        
        // Iterative resolution failed
        error!("Iterative resolution failed after {} iterations", iteration);
        if let Some(e) = last_error {
            Err(e)
        } else {
            Err(DnsError::Parse("Iterative resolution failed - no more referrals".to_string()))
        }
    }
    
    /// Query a single server for iterative resolution
    async fn query_iterative_server(&self, query: &DNSPacket, server: SocketAddr) -> Result<DNSPacket> {
        // Create a copy of the query with RD=0 for iterative queries
        let mut iterative_query = query.clone();
        iterative_query.header.rd = false; // Don't ask for recursion
        
        debug!("Sending iterative query to {}", server);
        
        // Serialize and send
        let query_bytes = iterative_query.serialize()
            .map_err(|e| DnsError::Parse(format!("Failed to serialize iterative query: {:?}", e)))?;
        
        self.send_query_with_timeout(&query_bytes, server).await
    }
    
    /// Extract nameserver addresses from authority section of a response
    async fn extract_referral_servers(&self, response: &DNSPacket) -> Vec<SocketAddr> {
        let mut servers = Vec::new();
        
        // Look for NS records in authority section
        for authority in &response.authorities {
            if authority.rtype == crate::dns::enums::DNSResourceType::NS {
                // This is a nameserver record
                // For now, we'll try to resolve the nameserver name
                // In a full implementation, we'd also check the additional section for A/AAAA records
                
                // Extract nameserver name from rdata (simplified parsing)
                if let Ok(ns_name) = self.parse_domain_name_from_rdata(&authority.rdata) {
                    debug!("Found nameserver: {}", ns_name);
                    
                    // Try to resolve the nameserver to an IP address
                    if let Ok(addr) = self.resolve_nameserver_address(&ns_name).await {
                        servers.push(addr);
                    }
                }
            }
        }
        
        // Also check additional section for A/AAAA records of nameservers
        for additional in &response.resources {
            if additional.rtype == crate::dns::enums::DNSResourceType::A && additional.rdlength == 4 {
                // IPv4 address
                if additional.rdata.len() >= 4 {
                    let ip = std::net::Ipv4Addr::new(
                        additional.rdata[0],
                        additional.rdata[1], 
                        additional.rdata[2],
                        additional.rdata[3]
                    );
                    servers.push(SocketAddr::new(ip.into(), 53));
                }
            }
        }
        
        servers
    }
    
    /// Parse a domain name from DNS rdata (simplified)
    fn parse_domain_name_from_rdata(&self, rdata: &[u8]) -> Result<String> {
        if rdata.is_empty() {
            return Err(DnsError::Parse("Empty rdata".to_string()));
        }
        
        let mut name_parts = Vec::new();
        let mut pos = 0;
        
        while pos < rdata.len() {
            let len = rdata[pos] as usize;
            
            if len == 0 {
                break; // End of name
            }
            
            if pos + 1 + len > rdata.len() {
                return Err(DnsError::Parse("Invalid label length in rdata".to_string()));
            }
            
            let label = String::from_utf8_lossy(&rdata[pos + 1..pos + 1 + len]);
            name_parts.push(label.to_string());
            pos += 1 + len;
        }
        
        Ok(name_parts.join("."))
    }
    
    /// Resolve a nameserver hostname to an IP address
    async fn resolve_nameserver_address(&self, ns_name: &str) -> Result<SocketAddr> {
        // For now, use a simple approach - try to resolve using upstream servers
        // In a full implementation, this would be more sophisticated
        
        // Create a query for the nameserver's A record
        let mut ns_query = crate::dns::DNSPacket::default();
        ns_query.header.id = QUERY_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        ns_query.header.rd = true; // Use recursion for NS resolution
        ns_query.header.qdcount = 1;
        
        // Add question for A record
        let mut question = crate::dns::question::DNSQuestion::default();
        question.labels = ns_name.split('.').map(|s| s.to_string()).collect();
        question.qtype = crate::dns::enums::DNSResourceType::A;
        question.qclass = crate::dns::enums::DNSResourceClass::IN;
        ns_query.questions.push(question);
        
        // Resolve using upstream servers
        match self.resolve_recursively(ns_query, 0).await {
            Ok(response) => {
                // Extract first A record
                for answer in &response.answers {
                    if answer.rtype == crate::dns::enums::DNSResourceType::A && answer.rdlength == 4 {
                        if answer.rdata.len() >= 4 {
                            let ip = std::net::Ipv4Addr::new(
                                answer.rdata[0],
                                answer.rdata[1],
                                answer.rdata[2], 
                                answer.rdata[3]
                            );
                            return Ok(SocketAddr::new(ip.into(), 53));
                        }
                    }
                }
                Err(DnsError::Parse(format!("No A record found for nameserver {}", ns_name)))
            }
            Err(e) => Err(e)
        }
    }
}