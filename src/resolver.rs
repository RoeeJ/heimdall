use crate::cache::{CacheKey, DnsCache};
use crate::config::DnsConfig;
use crate::dns::DNSPacket;
use crate::error::{DnsError, Result};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tokio::sync::{broadcast, Mutex};
use dashmap::DashMap;
use std::sync::Arc;
use std::collections::HashMap;
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

/// In-flight query tracking for deduplication
#[derive(Debug)]
struct InFlightQuery {
    /// Broadcast sender to notify all waiting clients
    sender: broadcast::Sender<Result<DNSPacket>>,
    /// Number of clients waiting for this query
    waiting_count: std::sync::atomic::AtomicU32,
}

/// Connection pool for reusing UDP sockets to upstream servers
#[derive(Debug)]
struct ConnectionPool {
    udp_sockets: Arc<Mutex<HashMap<SocketAddr, Vec<UdpSocket>>>>,
    max_connections_per_server: usize,
}

impl ConnectionPool {
    fn new(max_connections_per_server: usize) -> Self {
        Self {
            udp_sockets: Arc::new(Mutex::new(HashMap::new())),
            max_connections_per_server,
        }
    }

    /// Get a UDP socket for the given server, reusing existing connections when possible
    async fn get_udp_socket(&self, server_addr: SocketAddr) -> Result<UdpSocket> {
        let mut pool = self.udp_sockets.lock().await;
        
        // Try to get an existing socket for this server
        if let Some(sockets) = pool.get_mut(&server_addr) {
            if let Some(socket) = sockets.pop() {
                debug!("Reusing pooled UDP socket for {}", server_addr);
                return Ok(socket);
            }
        }

        // No available socket, create a new one
        debug!("Creating new UDP socket for {}", server_addr);
        let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| DnsError::Io(e.to_string()))?;
        socket.connect(server_addr).await.map_err(|e| DnsError::Io(e.to_string()))?;
        
        Ok(socket)
    }

    /// Return a UDP socket to the pool for reuse
    async fn return_udp_socket(&self, server_addr: SocketAddr, socket: UdpSocket) {
        let mut pool = self.udp_sockets.lock().await;
        
        let sockets = pool.entry(server_addr).or_insert_with(Vec::new);
        
        // Only pool the socket if we haven't exceeded the limit
        if sockets.len() < self.max_connections_per_server {
            debug!("Returning UDP socket to pool for {}", server_addr);
            sockets.push(socket);
        } else {
            debug!("Connection pool full for {}, dropping socket", server_addr);
            // Socket will be dropped and closed automatically
        }
    }

    /// Get pool statistics for monitoring
    async fn stats(&self) -> HashMap<SocketAddr, usize> {
        let pool = self.udp_sockets.lock().await;
        pool.iter().map(|(&addr, sockets)| (addr, sockets.len())).collect()
    }
}

#[derive(Debug)]
pub struct DnsResolver {
    config: DnsConfig,
    client_socket: UdpSocket,
    cache: Option<DnsCache>,
    /// In-flight queries for deduplication (query_key -> broadcast channel)
    in_flight_queries: Arc<DashMap<CacheKey, InFlightQuery>>,
    /// Connection pool for upstream queries
    connection_pool: ConnectionPool,
}

impl DnsResolver {
    pub async fn new(config: DnsConfig) -> Result<Self> {
        // Bind to a random port for upstream queries
        let client_socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Initialize cache if enabled
        let cache = if config.enable_caching {
            let cache = DnsCache::new(config.max_cache_size, config.default_ttl);
            info!(
                "DNS cache initialized: max_size={}, negative_ttl={}s",
                config.max_cache_size, config.default_ttl
            );
            Some(cache)
        } else {
            info!("DNS caching disabled");
            None
        };

        info!(
            "DNS resolver initialized with {} upstream servers",
            config.upstream_servers.len()
        );
        debug!("Upstream servers: {:?}", config.upstream_servers);

        Ok(Self {
            config,
            client_socket,
            cache,
            in_flight_queries: Arc::new(DashMap::new()),
            connection_pool: ConnectionPool::new(5), // Pool up to 5 connections per server
        })
    }

    /// Resolve a DNS query with automatic mode detection
    pub async fn resolve(&self, query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
        // Check cache first if enabled and we have questions
        if let Some(cache) = &self.cache {
            if !query.questions.is_empty() {
                let cache_key = CacheKey::from_question(&query.questions[0]);
                if let Some(mut cached_response) = cache.get(&cache_key) {
                    // Restore original query ID
                    cached_response.header.id = original_id;
                    debug!(
                        "Cache hit for query: {} {:?}",
                        cache_key.domain, cache_key.record_type
                    );
                    return Ok(cached_response);
                }

                // Check if this query is already in-flight (query deduplication)
                if let Some(in_flight) = self.in_flight_queries.get(&cache_key) {
                    // Increment waiting count for metrics
                    in_flight.waiting_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    
                    debug!(
                        "Query deduplication: joining in-flight query for {} {:?}",
                        cache_key.domain, cache_key.record_type
                    );
                    
                    // Subscribe to the broadcast channel to get the result
                    let mut receiver = in_flight.sender.subscribe();
                    
                    // Drop the reference to avoid holding the lock
                    drop(in_flight);
                    
                    // Wait for the result
                    match receiver.recv().await {
                        Ok(result) => {
                            match result {
                                Ok(mut response) => {
                                    // Restore original query ID
                                    response.header.id = original_id;
                                    debug!(
                                        "Query deduplication: received response for {} {:?}",
                                        cache_key.domain, cache_key.record_type
                                    );
                                    return Ok(response);
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        Err(_) => {
                            // Channel was closed, fall through to normal resolution
                            debug!(
                                "Query deduplication: channel closed for {} {:?}, falling back to normal resolution",
                                cache_key.domain, cache_key.record_type
                            );
                        }
                    }
                }
            }
        }

        // If we reach here, it's not a cache hit and not in-flight, so we need to resolve
        if !query.questions.is_empty() {
            let cache_key = CacheKey::from_question(&query.questions[0]);
            self.resolve_with_deduplication(query, original_id, cache_key).await
        } else {
            // No questions, resolve directly without deduplication
            let query_mode = QueryMode::from_packet(&query);
            match query_mode {
                QueryMode::Recursive => self.resolve_recursively(query, original_id).await,
                QueryMode::Iterative => {
                    if self.config.enable_iterative {
                        self.resolve_iteratively(query, original_id).await
                    } else {
                        self.resolve_recursively(query, original_id).await
                    }
                }
            }
        }
    }

    /// Resolve a query with deduplication support
    async fn resolve_with_deduplication(
        &self,
        query: DNSPacket,
        original_id: u16,
        cache_key: CacheKey,
    ) -> Result<DNSPacket> {
        // Create a broadcast channel for this query
        let (sender, _receiver) = broadcast::channel(16); // Buffer for up to 16 waiting clients
        
        let in_flight = InFlightQuery {
            sender: sender.clone(),
            waiting_count: std::sync::atomic::AtomicU32::new(1), // Start with 1 (this request)
        };

        // Try to insert our in-flight query  
        if let None = self.in_flight_queries.insert(cache_key.clone(), in_flight) {
            // We're the first to request this query, so we need to resolve it
                debug!(
                    "Query deduplication: initiating query for {} {:?}",
                    cache_key.domain, cache_key.record_type
                );

                let query_mode = QueryMode::from_packet(&query);
                let result = match query_mode {
                    QueryMode::Recursive => self.resolve_recursively(query.clone(), original_id).await,
                    QueryMode::Iterative => {
                        if self.config.enable_iterative {
                            self.resolve_iteratively(query.clone(), original_id).await
                        } else {
                            self.resolve_recursively(query.clone(), original_id).await
                        }
                    }
                };

                // Remove the in-flight query entry
                if let Some((_key, in_flight_entry)) = self.in_flight_queries.remove(&cache_key) {
                    let waiting_count = in_flight_entry.waiting_count.load(std::sync::atomic::Ordering::Relaxed);
                    if waiting_count > 1 {
                        debug!(
                            "Query deduplication: broadcasting result to {} waiting clients for {} {:?}",
                            waiting_count - 1, cache_key.domain, cache_key.record_type
                        );
                    }

                    // Broadcast the result to all waiting clients
                    let _ = sender.send(result.clone());
                }

                // Handle caching for the resolved result
                self.process_result(&result, &query);
                
                result
        } else {
                // Another request beat us to it, so we need to wait for the result
                debug!(
                    "Query deduplication: joining existing in-flight query for {} {:?}",
                    cache_key.domain, cache_key.record_type
                );

                // Increment waiting count for the existing entry
                if let Some(existing) = self.in_flight_queries.get(&cache_key) {
                    existing.waiting_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    
                    let mut receiver = existing.sender.subscribe();
                    drop(existing); // Drop the reference
                    
                    // Wait for the result
                    match receiver.recv().await {
                        Ok(result) => {
                            match result {
                                Ok(mut response) => {
                                    response.header.id = original_id;
                                    Ok(response)
                                }
                                Err(e) => Err(e),
                            }
                        }
                        Err(_) => {
                            // Channel was closed, fall back to normal resolution
                            debug!(
                                "Query deduplication: channel closed for {} {:?}, falling back",
                                cache_key.domain, cache_key.record_type
                            );
                            let query_mode = QueryMode::from_packet(&query);
                            match query_mode {
                                QueryMode::Recursive => self.resolve_recursively(query, original_id).await,
                                QueryMode::Iterative => {
                                    if self.config.enable_iterative {
                                        self.resolve_iteratively(query, original_id).await
                                    } else {
                                        self.resolve_recursively(query, original_id).await
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Entry disappeared, fall back to normal resolution
                    let query_mode = QueryMode::from_packet(&query);
                    match query_mode {
                        QueryMode::Recursive => self.resolve_recursively(query, original_id).await,
                        QueryMode::Iterative => {
                            if self.config.enable_iterative {
                                self.resolve_iteratively(query, original_id).await
                            } else {
                                self.resolve_recursively(query, original_id).await
                            }
                        }
                    }
                }
        }
    }

    /// Process result and handle caching (moved from the main resolve method)
    fn process_result(&self, result: &Result<DNSPacket>, query: &DNSPacket) {
        // Cache the result if successful and caching is enabled
        if let (Ok(response), Some(cache)) = (result, &self.cache) {
            if !query.questions.is_empty() {
                let cache_key = CacheKey::from_question(&query.questions[0]);
                cache.put(cache_key, response.clone());

                // Log cache statistics periodically
                let stats = cache.stats();
                let total_queries = stats.hits.load(std::sync::atomic::Ordering::Relaxed)
                    + stats.misses.load(std::sync::atomic::Ordering::Relaxed);
                if total_queries % 10 == 0 && total_queries > 0 {
                    info!("Cache performance: {}", cache.debug_info());
                }

                // Perform periodic cache cleanup (every 100 queries)
                if total_queries % 100 == 0 && total_queries > 0 {
                    cache.cleanup_expired();
                }
            }
        }
    }

    /// Resolve a DNS query by forwarding it to upstream servers (recursive)
    async fn resolve_recursively(
        &self,
        mut query: DNSPacket,
        original_id: u16,
    ) -> Result<DNSPacket> {
        // Generate a new query ID for upstream request
        let upstream_id = QUERY_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        query.header.id = upstream_id;

        debug!(
            "Resolving query: original_id={}, upstream_id={}, questions={}",
            original_id, upstream_id, query.header.qdcount
        );

        // Try each upstream server
        let mut last_error = None;

        for (attempt, &upstream_addr) in self.config.upstream_servers.iter().enumerate() {
            match self.query_upstream(&query, upstream_addr).await {
                Ok(mut response) => {
                    // Restore original query ID
                    response.header.id = original_id;
                    
                    // If the original query had EDNS, ensure response includes EDNS
                    if query.supports_edns() && response.edns.is_none() {
                        // Add EDNS to response matching client capabilities
                        let client_buffer_size = query.max_udp_payload_size();
                        let server_buffer_size = std::cmp::min(client_buffer_size, 4096); // Cap at 4KB
                        
                        response.add_edns(server_buffer_size, false); // Don't set DO flag in response unless needed
                        debug!("Added EDNS to response: buffer_size={}", server_buffer_size);
                    } else if let (Some(query_edns), Some(response_edns)) = (&query.edns, &mut response.edns) {
                        // Negotiate buffer size between client and server capabilities
                        let client_buffer_size = query_edns.payload_size();
                        let server_buffer_size = std::cmp::min(client_buffer_size, 4096); // Cap at 4KB
                        response_edns.set_payload_size(server_buffer_size);
                        debug!("Negotiated EDNS buffer size: client={}, server={}", client_buffer_size, server_buffer_size);
                    }
                    
                    info!(
                        "Successfully resolved query from upstream {} (attempt {})",
                        upstream_addr,
                        attempt + 1
                    );
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
    async fn query_upstream(
        &self,
        query: &DNSPacket,
        upstream_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        // Serialize the query
        let query_bytes = query
            .serialize()
            .map_err(|e| DnsError::Parse(format!("Failed to serialize query: {:?}", e)))?;

        trace!(
            "Sending {} bytes to upstream {}",
            query_bytes.len(),
            upstream_addr
        );

        // Send query with retries
        for retry in 0..=self.config.max_retries {
            match self
                .send_query_with_timeout(&query_bytes, upstream_addr)
                .await
            {
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

    /// Send query with timeout (try UDP first, fallback to TCP if truncated)
    async fn send_query_with_timeout(
        &self,
        query_bytes: &[u8],
        upstream_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        let query_future = async {
            // Try UDP first
            match self.send_udp_query(query_bytes, upstream_addr).await {
                Ok(response) => {
                    // Check if response is truncated
                    if response.header.tc {
                        debug!("UDP response truncated, retrying with TCP");
                        // Fallback to TCP
                        self.send_tcp_query(query_bytes, upstream_addr).await
                    } else {
                        Ok(response)
                    }
                }
                Err(e) => Err(e),
            }
        };

        // Apply timeout
        timeout(self.config.upstream_timeout, query_future)
            .await
            .map_err(|_| DnsError::Parse("Upstream query timeout".to_string()))?
    }

    /// Send query via UDP using connection pooling
    async fn send_udp_query(
        &self,
        query_bytes: &[u8],
        upstream_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        // Get a socket from the connection pool
        let socket = self.connection_pool.get_udp_socket(upstream_addr).await?;

        // Send the query
        socket.send(query_bytes).await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Wait for response
        let mut response_buf = vec![0u8; 4096];
        let response_len = socket.recv(&mut response_buf).await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Return the socket to the pool for reuse
        self.connection_pool.return_udp_socket(upstream_addr, socket).await;

        // Log the raw response for debugging
        trace!(
            "Raw UDP response data ({} bytes): {:02x?}",
            response_len,
            &response_buf[..response_len.min(64)]
        );

        // Parse the response
        let response = DNSPacket::parse(&response_buf[..response_len]).map_err(|e| {
            // Log more details about the parsing failure
            debug!("Failed to parse UDP response from {}: {:?}", upstream_addr, e);
            debug!("Response length: {} bytes", response_len);
            debug!(
                "First 64 bytes: {:02x?}",
                &response_buf[..response_len.min(64)]
            );
            DnsError::Parse(format!("Failed to parse response: {:?}", e))
        })?;

        self.log_response_details(&response, response_len, "UDP");
        Ok(response)
    }

    /// Send query via TCP
    async fn send_tcp_query(
        &self,
        query_bytes: &[u8],
        upstream_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        // Connect to upstream server
        let mut stream = TcpStream::connect(upstream_addr).await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Send length-prefixed query
        let query_length = query_bytes.len() as u16;
        stream.write_all(&query_length.to_be_bytes()).await.map_err(|e| DnsError::Io(e.to_string()))?;
        stream.write_all(query_bytes).await.map_err(|e| DnsError::Io(e.to_string()))?;
        stream.flush().await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Read response length
        let mut length_buf = [0u8; 2];
        stream.read_exact(&mut length_buf).await.map_err(|e| DnsError::Io(e.to_string()))?;
        let response_length = u16::from_be_bytes(length_buf) as usize;

        // Read response data
        let mut response_buf = vec![0; response_length];
        stream.read_exact(&mut response_buf).await.map_err(|e| DnsError::Io(e.to_string()))?;

        // Log the raw response for debugging
        trace!(
            "Raw TCP response data ({} bytes): {:02x?}",
            response_length,
            &response_buf[..response_length.min(64)]
        );

        // Parse the response
        let response = DNSPacket::parse(&response_buf).map_err(|e| {
            // Log more details about the parsing failure
            debug!("Failed to parse TCP response from {}: {:?}", upstream_addr, e);
            debug!("Response length: {} bytes", response_length);
            debug!(
                "First 64 bytes: {:02x?}",
                &response_buf[..response_length.min(64)]
            );
            DnsError::Parse(format!("Failed to parse response: {:?}", e))
        })?;

        self.log_response_details(&response, response_length, "TCP");
        Ok(response)
    }

    /// Log response details for debugging
    fn log_response_details(&self, response: &DNSPacket, response_len: usize, protocol: &str) {
        debug!(
            "Parsed {} response: questions={}, answers={}, authorities={}, additional={}",
            protocol,
            response.header.qdcount,
            response.header.ancount,
            response.header.nscount,
            response.header.arcount
        );

        for (i, answer) in response.answers.iter().enumerate() {
            let rdata_display = match &answer.parsed_rdata {
                Some(parsed) => format!("parsed={}", parsed),
                None => format!("raw={:02x?}", &answer.rdata[..answer.rdata.len().min(16)])
            };
            debug!(
                "Answer {}: type={:?}, class={:?}, ttl={}, rdlength={}, {}",
                i,
                answer.rtype,
                answer.rclass,
                answer.ttl,
                answer.rdlength,
                rdata_display
            );
        }

        trace!(
            "Received {} response: {} bytes, {} answers",
            protocol, response_len, response.header.ancount
        );
    }

    /// Create a SERVFAIL response for when resolution fails
    pub fn create_servfail_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = 2; // SERVFAIL
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
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = 3; // NXDOMAIN
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
    async fn resolve_iteratively(
        &self,
        mut query: DNSPacket,
        original_id: u16,
    ) -> Result<DNSPacket> {
        debug!("Starting iterative resolution for query id={}", original_id);

        // Get the first question to resolve
        if query.questions.is_empty() {
            return Err(DnsError::InvalidPacket("No questions in query".to_string()));
        }

        let question = &query.questions[0];
        let domain_name = question
            .labels
            .iter()
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
            debug!(
                "Iteration {}: querying {} servers",
                iteration,
                current_servers.len()
            );

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
                        if response.header.aa
                            && (response.header.rcode == 3 || response.header.rcode == 0)
                        {
                            // Authoritative response with no data
                            let mut final_response = response;
                            final_response.header.id = original_id;
                            return Ok(final_response);
                        }

                        // Look for referrals in authority section
                        let mut new_servers = self.extract_referral_servers(&response).await;
                        if !new_servers.is_empty() {
                            debug!(
                                "Found {} referral servers from {}",
                                new_servers.len(),
                                server
                            );
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
            Err(DnsError::Parse(
                "Iterative resolution failed - no more referrals".to_string(),
            ))
        }
    }

    /// Query a single server for iterative resolution
    async fn query_iterative_server(
        &self,
        query: &DNSPacket,
        server: SocketAddr,
    ) -> Result<DNSPacket> {
        // Create a copy of the query with RD=0 for iterative queries
        let mut iterative_query = query.clone();
        iterative_query.header.rd = false; // Don't ask for recursion

        debug!("Sending iterative query to {}", server);

        // Serialize and send
        let query_bytes = iterative_query.serialize().map_err(|e| {
            DnsError::Parse(format!("Failed to serialize iterative query: {:?}", e))
        })?;

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
            if additional.rtype == crate::dns::enums::DNSResourceType::A && additional.rdlength == 4
            {
                // IPv4 address
                if additional.rdata.len() >= 4 {
                    let ip = std::net::Ipv4Addr::new(
                        additional.rdata[0],
                        additional.rdata[1],
                        additional.rdata[2],
                        additional.rdata[3],
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
                    if answer.rtype == crate::dns::enums::DNSResourceType::A && answer.rdlength == 4
                    {
                        if answer.rdata.len() >= 4 {
                            let ip = std::net::Ipv4Addr::new(
                                answer.rdata[0],
                                answer.rdata[1],
                                answer.rdata[2],
                                answer.rdata[3],
                            );
                            return Ok(SocketAddr::new(ip.into(), 53));
                        }
                    }
                }
                Err(DnsError::Parse(format!(
                    "No A record found for nameserver {}",
                    ns_name
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Perform cache maintenance (cleanup expired entries)
    pub fn cleanup_cache(&self) {
        if let Some(cache) = &self.cache {
            cache.cleanup_expired();
        }
    }

    /// Get cache debug information
    pub fn cache_info(&self) -> Option<String> {
        self.cache.as_ref().map(|cache| cache.debug_info())
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> Option<&crate::cache::CacheStats> {
        self.cache.as_ref().map(|cache| cache.stats())
    }

    /// Get connection pool statistics
    pub async fn connection_pool_stats(&self) -> HashMap<SocketAddr, usize> {
        self.connection_pool.stats().await
    }
}
