use crate::blocking::updater::{BlocklistUpdater, default_blocklist_sources};
use crate::blocking::{BlockingMode, BlocklistFormat, DnsBlocker};
use crate::cache::{CacheKey, DnsCache};
use crate::config::DnsConfig;
use crate::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType, ResponseCode},
    resource::DNSResource,
};
use crate::dnssec::{DnsSecValidator, TrustAnchorStore, ValidationResult};
use crate::error::{DnsError, Result};
use crate::metrics::DnsMetrics;
use crate::zone::{QueryResult, ZoneStore};

/// Helper struct for SOA record fields to avoid too many function parameters
#[derive(Debug, Clone)]
struct SoaFields {
    pub serial: u32,
    pub refresh: u32,
    pub retry: u32,
    pub expire: u32,
    pub minimum: u32,
}
use dashmap::DashMap;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{Mutex, broadcast};
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

/// Server health status tracking
#[derive(Debug)]
struct ServerHealth {
    /// Number of consecutive failures
    consecutive_failures: AtomicU64,
    /// Last failure time
    last_failure: Mutex<Option<Instant>>,
    /// Total requests sent to this server
    total_requests: AtomicU64,
    /// Total successful responses from this server
    successful_responses: AtomicU64,
    /// Average response time (exponential moving average)
    avg_response_time: Mutex<Option<Duration>>,
    /// Whether the server is currently marked as healthy
    is_healthy: std::sync::atomic::AtomicBool,
    /// Last health check time
    last_health_check: Mutex<Option<Instant>>,
}

impl ServerHealth {
    fn new() -> Self {
        Self {
            consecutive_failures: AtomicU64::new(0),
            last_failure: Mutex::new(None),
            total_requests: AtomicU64::new(0),
            successful_responses: AtomicU64::new(0),
            avg_response_time: Mutex::new(None),
            is_healthy: std::sync::atomic::AtomicBool::new(true),
            last_health_check: Mutex::new(None),
        }
    }

    /// Record a successful response
    fn record_success(&self, response_time: Duration) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_responses.fetch_add(1, Ordering::Relaxed);
        self.is_healthy.store(true, Ordering::Relaxed);

        // Update exponential moving average of response time (async-safe)
        if let Ok(mut avg_time) = self.avg_response_time.try_lock() {
            if let Some(current_avg) = *avg_time {
                // EMA with alpha = 0.2 (more weight to recent responses)
                let new_avg = Duration::from_millis(
                    (current_avg.as_millis() as f64 * 0.8 + response_time.as_millis() as f64 * 0.2)
                        as u64,
                );
                *avg_time = Some(new_avg);
            } else {
                *avg_time = Some(response_time);
            }
        }
    }

    /// Record a failure
    fn record_failure(&self) {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut last_failure) = self.last_failure.try_lock() {
            *last_failure = Some(Instant::now());
        }

        // Mark as unhealthy after 3 consecutive failures
        if self.consecutive_failures.load(Ordering::Relaxed) >= 3 {
            self.is_healthy.store(false, Ordering::Relaxed);
        }
    }

    /// Check if the server is currently healthy
    fn is_healthy(&self) -> bool {
        self.is_healthy.load(Ordering::Relaxed)
    }

    /// Check if enough time has passed for a health check retry
    fn should_retry_health_check(&self) -> bool {
        if self.is_healthy() {
            return true; // Always allow healthy servers
        }

        if let Ok(last_check) = self.last_health_check.try_lock() {
            match *last_check {
                Some(last) => {
                    let failures = self.consecutive_failures.load(Ordering::Relaxed);
                    // Exponential backoff: 5s, 10s, 20s, 40s, max 60s
                    let backoff_seconds = std::cmp::min(5 * (2_u64.pow(failures as u32 - 1)), 60);
                    last.elapsed() >= Duration::from_secs(backoff_seconds)
                }
                None => true, // Never checked, allow retry
            }
        } else {
            true // Can't acquire lock, be conservative and allow retry
        }
    }

    /// Update health check timestamp
    fn update_health_check_time(&self) {
        if let Ok(mut last_check) = self.last_health_check.try_lock() {
            *last_check = Some(Instant::now());
        }
    }

    /// Get server statistics
    fn get_stats(&self) -> ServerStats {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_responses.load(Ordering::Relaxed);
        let success_rate = if total > 0 {
            successful as f64 / total as f64
        } else {
            1.0
        };

        let avg_response_time = self
            .avg_response_time
            .try_lock()
            .map(|guard| *guard)
            .unwrap_or(None);

        ServerStats {
            total_requests: total,
            successful_responses: successful,
            success_rate,
            consecutive_failures: self.consecutive_failures.load(Ordering::Relaxed),
            is_healthy: self.is_healthy(),
            avg_response_time,
        }
    }
}

/// Server statistics for monitoring
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub total_requests: u64,
    pub successful_responses: u64,
    pub success_rate: f64,
    pub consecutive_failures: u64,
    pub is_healthy: bool,
    pub avg_response_time: Option<Duration>,
}

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
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;
        socket
            .connect(server_addr)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

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
        pool.iter()
            .map(|(&addr, sockets)| (addr, sockets.len()))
            .collect()
    }
}

pub struct DnsResolver {
    config: DnsConfig,
    #[allow(dead_code)]
    client_socket: UdpSocket,
    cache: Option<DnsCache>,
    /// In-flight queries for deduplication (query_key -> broadcast channel)
    in_flight_queries: Arc<DashMap<CacheKey, InFlightQuery>>,
    /// Connection pool for upstream queries
    connection_pool: ConnectionPool,
    /// Health tracking for upstream servers
    server_health: Arc<DashMap<SocketAddr, ServerHealth>>,
    /// Metrics collector (optional)
    #[allow(dead_code)]
    metrics: Option<Arc<DnsMetrics>>,
    /// Query counter
    query_counter: AtomicU64,
    /// Error counter
    error_counter: AtomicU64,
    /// DNSSEC validator (optional)
    dnssec_validator: Option<Arc<DnsSecValidator>>,
    /// Zone store for authoritative DNS serving
    zone_store: Option<Arc<ZoneStore>>,
    /// DNS blocker (optional)
    pub blocker: Option<Arc<DnsBlocker>>,
}

impl DnsResolver {
    pub async fn new(config: DnsConfig, metrics: Option<Arc<DnsMetrics>>) -> Result<Self> {
        // Bind to a random port for upstream queries
        let client_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Initialize cache if enabled
        let cache = if config.enable_caching {
            let cache = if let Some(cache_path) = &config.cache_file_path {
                info!(
                    "DNS cache initialized with persistence: max_size={}, negative_ttl={}s, file={}",
                    config.max_cache_size, config.default_ttl, cache_path
                );
                let cache = DnsCache::with_persistence(
                    config.max_cache_size,
                    config.default_ttl,
                    cache_path.clone(),
                );

                // Load existing cache from disk
                if let Err(e) = cache.load_from_disk().await {
                    warn!("Failed to load cache from disk: {}", e);
                } else {
                    info!("Loaded cache from disk: {}", cache_path);
                }

                cache
            } else {
                info!(
                    "DNS cache initialized: max_size={}, negative_ttl={}s",
                    config.max_cache_size, config.default_ttl
                );
                DnsCache::new(config.max_cache_size, config.default_ttl)
            };
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

        let server_health = Arc::new(DashMap::new());

        // Initialize health tracking for all upstream servers
        for &server_addr in &config.upstream_servers {
            server_health.insert(server_addr, ServerHealth::new());
        }

        // Initialize DNSSEC validator if enabled
        let dnssec_validator = if config.dnssec_enabled {
            info!("DNSSEC validation enabled");
            let trust_anchors = Arc::new(TrustAnchorStore::new());
            Some(Arc::new(DnsSecValidator::new(trust_anchors)))
        } else {
            info!("DNSSEC validation disabled");
            None
        };

        // Initialize zone store if authoritative serving is enabled
        let zone_store = if config.authoritative_enabled {
            info!("Authoritative DNS serving enabled");
            let store = Arc::new(ZoneStore::new());

            // Load configured zone files
            for zone_file in &config.zone_files {
                match store.load_zone_file(zone_file) {
                    Ok(origin) => info!("Loaded zone {} from {}", origin, zone_file),
                    Err(e) => error!("Failed to load zone file {}: {}", zone_file, e),
                }
            }

            info!("Loaded {} zones", store.zone_count());
            Some(store)
        } else {
            info!("Authoritative DNS serving disabled");
            None
        };

        // Initialize DNS blocker if enabled
        let blocker = if config.blocking_enabled {
            info!("DNS blocking enabled");
            let blocking_mode = match config.blocking_mode.as_str() {
                "nxdomain" => BlockingMode::NxDomain,
                "zero_ip" => BlockingMode::ZeroIp,
                "custom_ip" => {
                    if let Some(ref ip_str) = config.blocking_custom_ip {
                        if let Ok(ip) = ip_str.parse() {
                            BlockingMode::CustomIp(ip)
                        } else {
                            warn!(
                                "Invalid custom blocking IP: {}, falling back to NxDomain",
                                ip_str
                            );
                            BlockingMode::NxDomain
                        }
                    } else {
                        warn!(
                            "Custom IP mode selected but no IP provided, falling back to NxDomain"
                        );
                        BlockingMode::NxDomain
                    }
                }
                "refused" => BlockingMode::Refused,
                _ => {
                    warn!(
                        "Unknown blocking mode: {}, using NxDomain",
                        config.blocking_mode
                    );
                    BlockingMode::NxDomain
                }
            };

            let blocker = Arc::new(DnsBlocker::new(
                blocking_mode,
                config.blocking_enable_wildcards,
            ));

            // Initialize the Public Suffix List for domain deduplication
            if let Err(e) = blocker.initialize_psl().await {
                warn!("Failed to initialize PSL: {}", e);
            }

            // Load allowlist
            for domain in &config.allowlist {
                blocker.add_to_allowlist(domain);
            }
            info!("Loaded {} allowlist entries", config.allowlist.len());

            // Load blocklists
            let mut _total_blocked = 0;
            let mut missing_blocklists = Vec::new();

            for blocklist_spec in &config.blocklists {
                let parts: Vec<&str> = blocklist_spec.split(':').collect();
                if parts.len() == 3 {
                    let path = parts[0];
                    let format = match parts[1] {
                        "domain_list" => BlocklistFormat::DomainList,
                        "hosts" => BlocklistFormat::Hosts,
                        "adblock" => BlocklistFormat::AdBlockPlus,
                        "pihole" => BlocklistFormat::PiHole,
                        "dnsmasq" => BlocklistFormat::Dnsmasq,
                        "unbound" => BlocklistFormat::Unbound,
                        _ => {
                            warn!("Unknown blocklist format: {}", parts[1]);
                            continue;
                        }
                    };
                    let name = parts[2];

                    // Check if file exists
                    let path_buf = std::path::PathBuf::from(path);
                    if !path_buf.exists() {
                        warn!(
                            "Blocklist file not found: {} (will download if auto-update enabled)",
                            path
                        );
                        missing_blocklists.push((path_buf, format, name.to_string()));
                        continue;
                    }

                    match blocker.load_blocklist(&path_buf, format, name) {
                        Ok(count) => {
                            info!("Loaded {} domains from blocklist: {}", count, name);
                            _total_blocked += count;
                        }
                        Err(e) => {
                            error!("Failed to load blocklist {}: {}", name, e);
                        }
                    }
                }
            }

            // If auto-update is enabled and we have missing blocklists, try to download them
            if config.blocklist_auto_update && !missing_blocklists.is_empty() {
                info!("Auto-update enabled, downloading missing blocklists...");

                // Use default blocklist sources
                let mut sources = default_blocklist_sources();

                // Update the update interval from config
                for source in &mut sources {
                    source.update_interval = Some(std::time::Duration::from_secs(
                        config.blocklist_update_interval,
                    ));
                }

                let updater = BlocklistUpdater::new(sources, Arc::clone(&blocker));

                // Try to download missing blocklists
                for (path, _format, name) in missing_blocklists {
                    // Find matching source
                    if let Some(source) = updater.sources.iter().find(|s| s.path == path) {
                        match updater.update_blocklist(source).await {
                            Ok(_) => {
                                info!("Successfully downloaded blocklist: {}", name);
                                // The updater already loads the blocklist into the blocker
                            }
                            Err(e) => {
                                warn!("Failed to download blocklist {}: {}", name, e);
                            }
                        }
                    }
                }

                // Start background auto-updater task if needed
                let updater = Arc::new(updater);
                tokio::spawn(async move {
                    updater.start_auto_update().await;
                });
            }

            info!("Total blocked domains: {}", blocker.blocked_domain_count());

            Some(blocker)
        } else {
            info!("DNS blocking disabled");
            None
        };

        Ok(Self {
            config,
            client_socket,
            cache,
            in_flight_queries: Arc::new(DashMap::new()),
            connection_pool: ConnectionPool::new(5), // Pool up to 5 connections per server
            server_health,
            metrics,
            query_counter: AtomicU64::new(0),
            error_counter: AtomicU64::new(0),
            dnssec_validator,
            zone_store,
            blocker,
        })
    }

    /// Resolve a DNS query with automatic mode detection
    pub async fn resolve(&self, query: DNSPacket, original_id: u16) -> Result<DNSPacket> {
        // Increment query counter
        self.query_counter.fetch_add(1, Ordering::Relaxed);

        // Check for blocked domains if blocking is enabled
        if let Some(blocker) = &self.blocker {
            if !query.questions.is_empty() {
                let question = &query.questions[0];
                let domain = question.labels.join(".");

                if blocker.is_blocked(&domain) {
                    debug!("Domain {} is blocked", domain);

                    // Update blocking metrics if available
                    if let Some(metrics) = &self.metrics {
                        metrics.blocked_queries.inc();
                    }

                    // Return appropriate response based on blocking mode
                    return match blocker.blocking_mode() {
                        BlockingMode::NxDomain => Ok(self.create_nxdomain_response(&query)),
                        BlockingMode::ZeroIp => {
                            Ok(self.create_zero_ip_response(&query, original_id))
                        }
                        BlockingMode::CustomIp(ip) => {
                            Ok(self.create_custom_ip_response(&query, original_id, ip))
                        }
                        BlockingMode::Refused => Ok(self.create_refused_response(&query)),
                    };
                }
            }
        }

        // Check for authoritative answer first if enabled
        if let Some(zone_store) = &self.zone_store {
            if !query.questions.is_empty() {
                let question = &query.questions[0];
                let qname = question.labels.join(".");

                match zone_store.query(&qname, question.qtype) {
                    QueryResult::Success { records, .. } => {
                        debug!(
                            "Authoritative answer for {}: {} records",
                            qname,
                            records.len()
                        );
                        return self.build_authoritative_response(
                            query,
                            original_id,
                            records,
                            ResponseCode::NoError,
                            true,
                        );
                    }
                    QueryResult::NoData { soa, .. } => {
                        debug!("Authoritative NODATA for {}", qname);
                        let soa_records = soa.map(|s| vec![s]).unwrap_or_default();
                        return self.build_authoritative_response(
                            query,
                            original_id,
                            soa_records,
                            ResponseCode::NoError,
                            true,
                        );
                    }
                    QueryResult::NXDomain { soa, .. } => {
                        debug!("Authoritative NXDOMAIN for {}", qname);
                        let soa_records = soa.map(|s| vec![s]).unwrap_or_default();
                        return self.build_authoritative_response(
                            query,
                            original_id,
                            soa_records,
                            ResponseCode::NameError,
                            true,
                        );
                    }
                    QueryResult::Delegation { ns_records, .. } => {
                        debug!("Delegation for {}: {} NS records", qname, ns_records.len());
                        return self.build_authoritative_response(
                            query,
                            original_id,
                            ns_records,
                            ResponseCode::NoError,
                            false,
                        );
                    }
                    QueryResult::NotAuthoritative => {
                        // Fall through to recursive resolution
                        debug!("Not authoritative for {}", qname);
                    }
                    QueryResult::Error(e) => {
                        warn!("Zone query error for {}: {}", qname, e);
                        // Fall through to recursive resolution
                    }
                }
            }
        }

        // Check cache if enabled and we have questions
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
                    in_flight
                        .waiting_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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
            self.resolve_with_deduplication(query, original_id, cache_key)
                .await
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
        if self
            .in_flight_queries
            .insert(cache_key.clone(), in_flight)
            .is_none()
        {
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
                let waiting_count = in_flight_entry
                    .waiting_count
                    .load(std::sync::atomic::Ordering::Relaxed);
                if waiting_count > 1 {
                    debug!(
                        "Query deduplication: broadcasting result to {} waiting clients for {} {:?}",
                        waiting_count - 1,
                        cache_key.domain,
                        cache_key.record_type
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
                existing
                    .waiting_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                let mut receiver = existing.sender.subscribe();
                drop(existing); // Drop the reference

                // Wait for the result
                match receiver.recv().await {
                    Ok(result) => match result {
                        Ok(mut response) => {
                            response.header.id = original_id;
                            Ok(response)
                        }
                        Err(e) => Err(e),
                    },
                    Err(_) => {
                        // Channel was closed, fall back to normal resolution
                        debug!(
                            "Query deduplication: channel closed for {} {:?}, falling back",
                            cache_key.domain, cache_key.record_type
                        );
                        let query_mode = QueryMode::from_packet(&query);
                        match query_mode {
                            QueryMode::Recursive => {
                                self.resolve_recursively(query, original_id).await
                            }
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
                if total_queries % 100 == 0 && total_queries > 0 {
                    debug!("Cache performance: {}", cache.debug_info());
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

        // Use parallel queries if we have multiple upstream servers
        if self.config.upstream_servers.len() > 1 && self.config.enable_parallel_queries {
            match self
                .resolve_with_parallel_queries(&query, original_id)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    warn!(
                        "Parallel queries failed, falling back to sequential: {:?}",
                        e
                    );
                    // Fall through to sequential resolution
                }
            }
        }

        // Sequential fallback (original behavior)
        self.resolve_sequentially(&query, original_id).await
    }

    /// Resolve using parallel queries to multiple upstream servers with health awareness
    async fn resolve_with_parallel_queries(
        &self,
        query: &DNSPacket,
        original_id: u16,
    ) -> Result<DNSPacket> {
        use futures::future::FutureExt;
        use tokio::time::timeout;

        // Get healthy servers for parallel queries
        let servers_to_query = self.get_servers_by_health_priority();

        if servers_to_query.is_empty() {
            error!("No healthy servers available for parallel queries");
            return Err(DnsError::Parse("No healthy servers available".to_string()));
        }

        debug!(
            "Starting parallel queries to {} healthy upstream servers",
            servers_to_query.len()
        );

        // Create futures for each healthy upstream server
        let query_futures: Vec<_> = servers_to_query
            .iter()
            .enumerate()
            .filter_map(|(idx, &upstream_addr)| {
                // Check if we should try this server
                if let Some(health) = self.server_health.get(&upstream_addr) {
                    if !health.should_retry_health_check() {
                        debug!(
                            "Skipping unhealthy server {} in parallel query",
                            upstream_addr
                        );
                        return None;
                    }
                    health.update_health_check_time();
                }

                let query = query.clone();
                Some(
                    async move {
                        debug!(
                            "Parallel query {}: starting query to {}",
                            idx, upstream_addr
                        );
                        let start_time = std::time::Instant::now();

                        match self.query_upstream(&query, upstream_addr).await {
                            Ok(mut response) => {
                                let elapsed = start_time.elapsed();

                                // Record successful response
                                if let Some(health) = self.server_health.get(&upstream_addr) {
                                    health.record_success(elapsed);

                                    // Record individual response time metric
                                    if let Some(metrics) = &self.metrics {
                                        metrics
                                            .upstream_response_time
                                            .with_label_values(&[&upstream_addr.to_string()])
                                            .observe(elapsed.as_secs_f64());
                                    }
                                }

                                debug!(
                                    "Parallel query {}: SUCCESS from {} in {:?}",
                                    idx, upstream_addr, elapsed
                                );

                                // Restore original query ID
                                response.header.id = original_id;

                                // Handle EDNS response setup
                                self.setup_edns_response(&query, &mut response);

                                Ok((response, upstream_addr, elapsed))
                            }
                            Err(e) => {
                                let elapsed = start_time.elapsed();

                                // Record failure
                                if let Some(health) = self.server_health.get(&upstream_addr) {
                                    health.record_failure();
                                }

                                debug!(
                                    "Parallel query {}: FAILED from {} in {:?}: {:?}",
                                    idx, upstream_addr, elapsed, e
                                );
                                Err(e)
                            }
                        }
                    }
                    .boxed(),
                )
            })
            .collect();

        if query_futures.is_empty() {
            warn!("No servers available for parallel queries after health filtering");
            return Err(DnsError::Parse(
                "No healthy servers available for parallel queries".to_string(),
            ));
        }

        // Race all queries with a timeout
        let parallel_timeout =
            std::cmp::min(self.config.upstream_timeout, Duration::from_millis(2000));

        match timeout(parallel_timeout, futures::future::select_ok(query_futures)).await {
            Ok(Ok(((response, upstream_addr, elapsed), _remaining_futures))) => {
                debug!(
                    "Parallel query SUCCESS: {} responded in {:?} (faster than others)",
                    upstream_addr, elapsed
                );
                Ok(response)
            }
            Ok(Err(e)) => {
                warn!("All parallel queries failed: {:?}", e);
                Err(e)
            }
            Err(_) => {
                warn!(
                    "All parallel queries timed out after {:?}",
                    parallel_timeout
                );
                Err(DnsError::Parse(
                    "All parallel queries timed out".to_string(),
                ))
            }
        }
    }

    /// Sequential resolution with automatic failover
    async fn resolve_sequentially(&self, query: &DNSPacket, original_id: u16) -> Result<DNSPacket> {
        let mut last_error = None;

        // Get healthy servers first, then unhealthy ones as fallback
        let servers_to_try = self.get_servers_by_health_priority();

        if servers_to_try.is_empty() {
            error!("No upstream servers available");
            return Err(DnsError::Parse("No upstream servers available".to_string()));
        }

        for (attempt, &upstream_addr) in servers_to_try.iter().enumerate() {
            // Check if we should try this server
            if let Some(health) = self.server_health.get(&upstream_addr) {
                if !health.should_retry_health_check() {
                    debug!(
                        "Skipping unhealthy server {} (in backoff period)",
                        upstream_addr
                    );
                    continue;
                }
                health.update_health_check_time();
            }

            let start_time = Instant::now();
            match self.query_upstream(query, upstream_addr).await {
                Ok(mut response) => {
                    let response_time = start_time.elapsed();

                    // Record successful response
                    if let Some(health) = self.server_health.get(&upstream_addr) {
                        health.record_success(response_time);

                        // Record individual response time metric
                        if let Some(metrics) = &self.metrics {
                            metrics
                                .upstream_response_time
                                .with_label_values(&[&upstream_addr.to_string()])
                                .observe(response_time.as_secs_f64());
                        }

                        debug!(
                            "Successfully resolved query from upstream {} (attempt {}, response_time: {:?})",
                            upstream_addr,
                            attempt + 1,
                            response_time
                        );
                    }

                    // Restore original query ID
                    response.header.id = original_id;

                    // Handle EDNS response setup
                    self.setup_edns_response(query, &mut response);

                    return Ok(response);
                }
                Err(e) => {
                    // Record failure
                    if let Some(health) = self.server_health.get(&upstream_addr) {
                        health.record_failure();
                        let stats = health.get_stats();
                        warn!(
                            "Failed to resolve from upstream {} (attempt {}): {:?} - Server stats: {} failures, {:.1}% success rate",
                            upstream_addr,
                            attempt + 1,
                            e,
                            stats.consecutive_failures,
                            stats.success_rate * 100.0
                        );
                    }

                    last_error = Some(e);

                    // If this isn't the last server, continue to next
                    if attempt < servers_to_try.len() - 1 {
                        continue;
                    }
                }
            }
        }

        // All upstream servers failed
        error!(
            "All upstream servers failed to resolve query after trying {} servers",
            servers_to_try.len()
        );
        Err(last_error.unwrap_or(DnsError::Parse("No upstream servers available".to_string())))
    }

    /// Get upstream servers ordered by health priority (healthy first, then unhealthy)
    fn get_servers_by_health_priority(&self) -> Vec<SocketAddr> {
        let mut healthy_servers = Vec::new();
        let mut unhealthy_servers = Vec::new();

        for &server_addr in &self.config.upstream_servers {
            if let Some(health) = self.server_health.get(&server_addr) {
                if health.is_healthy() {
                    healthy_servers.push(server_addr);
                } else if health.should_retry_health_check() {
                    unhealthy_servers.push(server_addr);
                }
            } else {
                // No health data yet, treat as healthy
                healthy_servers.push(server_addr);
            }
        }

        // Sort healthy servers by average response time (fastest first)
        healthy_servers.sort_by(|&a, &b| {
            let a_health = self.server_health.get(&a);
            let b_health = self.server_health.get(&b);

            match (a_health, b_health) {
                (Some(a_health), Some(b_health)) => {
                    let a_time = a_health
                        .avg_response_time
                        .try_lock()
                        .map(|guard| *guard)
                        .unwrap_or(None)
                        .unwrap_or(Duration::from_millis(1000));
                    let b_time = b_health
                        .avg_response_time
                        .try_lock()
                        .map(|guard| *guard)
                        .unwrap_or(None)
                        .unwrap_or(Duration::from_millis(1000));
                    a_time.cmp(&b_time)
                }
                _ => std::cmp::Ordering::Equal,
            }
        });

        // Return healthy servers first, then unhealthy as fallback
        healthy_servers.extend(unhealthy_servers);
        healthy_servers
    }

    /// Setup EDNS response based on query capabilities
    fn setup_edns_response(&self, query: &DNSPacket, response: &mut DNSPacket) {
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
            debug!(
                "Negotiated EDNS buffer size: client={}, server={}",
                client_buffer_size, server_buffer_size
            );
        }
    }

    /// Query a specific upstream server
    async fn query_upstream(
        &self,
        query: &DNSPacket,
        upstream_addr: SocketAddr,
    ) -> Result<DNSPacket> {
        // Clone query to modify for DNSSEC if needed
        let mut query_to_send = query.clone();

        // Set DNSSEC DO flag if validation is enabled
        if self.dnssec_validator.is_some() {
            // Ensure EDNS is present
            if query_to_send.edns.is_none() {
                query_to_send.add_edns(4096, true); // 4KB buffer, DO flag set
            } else if let Some(edns) = &mut query_to_send.edns {
                edns.set_do_flag(true);
            }
        }

        // Serialize the query
        let query_bytes = query_to_send
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

                    // Perform DNSSEC validation if enabled
                    if let Some(dnssec_validator) = &self.dnssec_validator {
                        if !query.questions.is_empty() {
                            let qname = query.questions[0].labels.join(".");
                            let qtype = query.questions[0].qtype;

                            let validation_result = dnssec_validator
                                .validate_with_denial(&response, &qname, qtype)
                                .await;

                            match validation_result {
                                ValidationResult::Secure => {
                                    debug!("DNSSEC validation successful for {}", qname);
                                }
                                ValidationResult::Insecure => {
                                    debug!("Response is not DNSSEC signed for {}", qname);
                                }
                                ValidationResult::Bogus(reason) => {
                                    warn!("DNSSEC validation failed for {}: {}", qname, reason);
                                    if self.config.dnssec_strict {
                                        // In strict mode, treat bogus responses as failures
                                        return Err(DnsError::Parse(format!(
                                            "DNSSEC validation failed: {}",
                                            reason
                                        )));
                                    }
                                    // In non-strict mode, still return the response but log the warning
                                }
                                ValidationResult::Indeterminate => {
                                    debug!("DNSSEC validation indeterminate for {}", qname);
                                }
                            }
                        }
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
        socket
            .send(query_bytes)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Wait for response
        let mut response_buf = vec![0u8; 4096];
        let response_len = socket
            .recv(&mut response_buf)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Return the socket to the pool for reuse
        self.connection_pool
            .return_udp_socket(upstream_addr, socket)
            .await;

        // Log the raw response for debugging
        trace!(
            "Raw UDP response data ({} bytes): {:02x?}",
            response_len,
            &response_buf[..response_len.min(64)]
        );

        // Parse the response
        let response = DNSPacket::parse(&response_buf[..response_len]).map_err(|e| {
            // Log more details about the parsing failure
            debug!(
                "Failed to parse UDP response from {}: {:?}",
                upstream_addr, e
            );
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
        let mut stream = TcpStream::connect(upstream_addr)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Send length-prefixed query
        let query_length = query_bytes.len() as u16;
        stream
            .write_all(&query_length.to_be_bytes())
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;
        stream
            .write_all(query_bytes)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Read response length
        let mut length_buf = [0u8; 2];
        stream
            .read_exact(&mut length_buf)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;
        let response_length = u16::from_be_bytes(length_buf) as usize;

        // Read response data
        let mut response_buf = vec![0; response_length];
        stream
            .read_exact(&mut response_buf)
            .await
            .map_err(|e| DnsError::Io(e.to_string()))?;

        // Log the raw response for debugging
        trace!(
            "Raw TCP response data ({} bytes): {:02x?}",
            response_length,
            &response_buf[..response_length.min(64)]
        );

        // Parse the response
        let response = DNSPacket::parse(&response_buf).map_err(|e| {
            // Log more details about the parsing failure
            debug!(
                "Failed to parse TCP response from {}: {:?}",
                upstream_addr, e
            );
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
                None => format!("raw={:02x?}", &answer.rdata[..answer.rdata.len().min(16)]),
            };
            debug!(
                "Answer {}: type={:?}, class={:?}, ttl={}, rdlength={}, {}",
                i, answer.rtype, answer.rclass, answer.ttl, answer.rdlength, rdata_display
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
        response.header.rcode = ResponseCode::ServerFailure.to_u8(); // SERVFAIL
        response.header.ancount = 0; // No answers
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records

        // Clear answer sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        // Note: SERVFAIL responses typically don't include SOA records
        // as they indicate a server problem rather than a definitive
        // negative answer about the domain's existence

        response
    }

    /// Create a truncated response for UDP size limits
    pub fn create_truncated_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.tc = true; // Truncated - client should retry with TCP
        response.header.rcode = ResponseCode::NoError.to_u8(); // NOERROR
        response.header.ancount = 0; // No answers (truncated)
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records (except EDNS if present)

        // Clear answer sections to ensure response fits in UDP
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        response
    }

    /// Create a NXDOMAIN response for non-existent domains with proper SOA authority
    pub fn create_nxdomain_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = ResponseCode::NameError.to_u8(); // NXDOMAIN
        response.header.ancount = 0; // No answers
        response.header.arcount = 0; // No additional records

        // Clear answer and additional sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        // Add SOA record in authority section for RFC 2308 compliance
        if !query.questions.is_empty() {
            if let Some(soa_record) = self.create_synthetic_soa_record(&query.questions[0].labels) {
                response.authorities.push(soa_record);
                response.header.nscount = 1;
            } else {
                response.header.nscount = 0;
            }
        } else {
            response.header.nscount = 0;
        }

        response
    }

    /// Create a REFUSED response for policy violations or administrative refusal
    pub fn create_refused_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = ResponseCode::Refused.to_u8(); // REFUSED
        response.header.ancount = 0; // No answers
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records

        // Clear answer sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        response
    }

    /// Create a NOTIMPL response for unsupported operations
    pub fn create_notimpl_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = false; // May not support recursion for this operation
        response.header.rcode = ResponseCode::NotImplemented.to_u8(); // NOTIMPL
        response.header.ancount = 0; // No answers
        response.header.nscount = 0; // No authority records
        response.header.arcount = 0; // No additional records

        // Clear answer sections but preserve question
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        response
    }

    /// Create a FORMERR response for malformed queries
    pub fn create_formerr_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = ResponseCode::FormatError.to_u8(); // FORMERR
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
            return Err(DnsError::Parse("No questions in query".to_string()));
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
        let question = crate::dns::question::DNSQuestion {
            labels: ns_name.split('.').map(|s| s.to_string()).collect(),
            qtype: crate::dns::enums::DNSResourceType::A,
            qclass: crate::dns::enums::DNSResourceClass::IN,
        };
        ns_query.questions.push(question);

        // Resolve using upstream servers
        match self.resolve_recursively(ns_query, 0).await {
            Ok(response) => {
                // Extract first A record
                for answer in &response.answers {
                    if answer.rtype == crate::dns::enums::DNSResourceType::A
                        && answer.rdlength == 4
                        && answer.rdata.len() >= 4
                    {
                        let ip = std::net::Ipv4Addr::new(
                            answer.rdata[0],
                            answer.rdata[1],
                            answer.rdata[2],
                            answer.rdata[3],
                        );
                        return Ok(SocketAddr::new(ip.into(), 53));
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

    /// Get current cache size
    pub fn cache_size(&self) -> Option<usize> {
        self.cache.as_ref().map(|cache| cache.size())
    }

    /// Get connection pool statistics
    pub async fn connection_pool_stats(&self) -> HashMap<SocketAddr, usize> {
        self.connection_pool.stats().await
    }

    /// Save cache to disk if persistence is enabled
    pub async fn save_cache(&self) -> Result<()> {
        if let Some(cache) = &self.cache {
            if cache.has_persistence() {
                cache
                    .save_to_disk()
                    .await
                    .map_err(|e| DnsError::Io(format!("Failed to save cache: {}", e)))?;
                debug!(
                    "Cache saved to disk: {}",
                    cache.cache_file_path().unwrap_or("unknown")
                );
            }
        }
        Ok(())
    }

    /// Check if cache persistence is enabled
    pub fn has_cache_persistence(&self) -> bool {
        self.cache
            .as_ref()
            .is_some_and(|cache| cache.has_persistence())
    }

    /// Get server health statistics for all upstream servers
    pub fn get_server_health_stats(&self) -> HashMap<SocketAddr, ServerStats> {
        let mut stats = HashMap::new();
        for server_addr in &self.config.upstream_servers {
            if let Some(health) = self.server_health.get(server_addr) {
                stats.insert(*server_addr, health.get_stats());
            }
        }
        stats
    }

    /// Get detailed health info for debugging
    pub fn get_health_debug_info(&self) -> String {
        let mut info = String::new();
        info.push_str("=== Upstream Server Health Status ===\n");

        for &server_addr in &self.config.upstream_servers {
            if let Some(health) = self.server_health.get(&server_addr) {
                let stats = health.get_stats();
                info.push_str(&format!(
                    "Server: {} - {} - Requests: {}, Success Rate: {:.1}%, Failures: {}, Avg Response: {:?}\n",
                    server_addr,
                    if stats.is_healthy { "HEALTHY" } else { "UNHEALTHY" },
                    stats.total_requests,
                    stats.success_rate * 100.0,
                    stats.consecutive_failures,
                    stats.avg_response_time.map_or("N/A".to_string(), |d| format!("{:?}", d))
                ));
            }
        }

        info
    }

    /// Force mark a server as healthy (for testing/admin purposes)
    pub fn reset_server_health(&self, server_addr: SocketAddr) {
        if let Some(health) = self.server_health.get(&server_addr) {
            health.consecutive_failures.store(0, Ordering::Relaxed);
            health.is_healthy.store(true, Ordering::Relaxed);
            if let Ok(mut last_failure) = health.last_failure.try_lock() {
                *last_failure = None;
            }
            info!("Reset health status for server: {}", server_addr);
        }
    }

    /// Create a synthetic SOA record for negative responses (RFC 2308 compliance)
    fn create_synthetic_soa_record(&self, query_labels: &[String]) -> Option<DNSResource> {
        // Extract the domain from the query labels
        // For queries like "nonexistent.example.com", we want to create SOA for "example.com"
        if query_labels.is_empty() {
            return None;
        }

        // For simplicity, create a generic SOA record for the queried domain
        // In a real authoritative server, this would be based on actual zone data
        let domain_labels = if query_labels.len() >= 2 {
            // Use the last two labels as the domain (e.g., example.com)
            query_labels[query_labels.len() - 2..].to_vec()
        } else {
            query_labels.to_vec()
        };

        let mut soa_record = DNSResource {
            labels: domain_labels.clone(),
            rtype: DNSResourceType::SOA,
            rclass: DNSResourceClass::IN,
            ttl: 300, // 5 minutes TTL for synthetic SOA
            ..Default::default()
        };

        // Create SOA rdata with default values for a recursive resolver
        let domain_name = format!("{}.", domain_labels.join("."));
        let admin_email = format!("admin.{}.", domain_labels.join("."));

        soa_record.rdata = self.create_soa_rdata(
            &domain_name,
            &admin_email,
            SoaFields {
                serial: 1,      // Serial number
                refresh: 3600,  // Refresh (1 hour)
                retry: 1800,    // Retry (30 minutes)
                expire: 604800, // Expire (1 week)
                minimum: 180,   // Minimum TTL (3 minutes) - used for negative caching per RFC 2308
            },
        );
        soa_record.rdlength = soa_record.rdata.len() as u16;

        Some(soa_record)
    }

    /// Create SOA rdata in DNS wire format
    fn create_soa_rdata(&self, mname: &str, rname: &str, soa_fields: SoaFields) -> Vec<u8> {
        let mut rdata = Vec::new();

        // Encode MNAME (primary nameserver)
        self.encode_domain_name(&mut rdata, mname);

        // Encode RNAME (responsible email)
        self.encode_domain_name(&mut rdata, rname);

        // Encode 32-bit values in network byte order
        rdata.extend_from_slice(&soa_fields.serial.to_be_bytes());
        rdata.extend_from_slice(&soa_fields.refresh.to_be_bytes());
        rdata.extend_from_slice(&soa_fields.retry.to_be_bytes());
        rdata.extend_from_slice(&soa_fields.expire.to_be_bytes());
        rdata.extend_from_slice(&soa_fields.minimum.to_be_bytes());

        rdata
    }

    /// Encode domain name in DNS wire format
    fn encode_domain_name(&self, buffer: &mut Vec<u8>, domain: &str) {
        for label in domain.split('.') {
            if !label.is_empty() {
                buffer.push(label.len() as u8);
                buffer.extend_from_slice(label.as_bytes());
            }
        }
        buffer.push(0); // Null terminator
    }

    /// Get total number of queries handled
    pub fn total_queries(&self) -> u64 {
        self.query_counter.load(Ordering::Relaxed)
    }

    /// Get total number of errors
    pub fn total_errors(&self) -> u64 {
        self.error_counter.load(Ordering::Relaxed)
    }

    /// Build an authoritative DNS response
    fn build_authoritative_response(
        &self,
        query: DNSPacket,
        original_id: u16,
        records: Vec<DNSResource>,
        rcode: ResponseCode,
        authoritative: bool,
    ) -> Result<DNSPacket> {
        let mut response = DNSPacket {
            header: query.header.clone(),
            questions: query.questions.clone(),
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: query.edns.clone(),
        };

        // Set response header flags
        response.header.id = original_id;
        response.header.qr = true; // This is a response
        response.header.aa = authoritative; // Authoritative answer
        response.header.tc = false; // Not truncated
        response.header.rd = query.header.rd; // Copy recursion desired
        response.header.ra = false; // Recursion not available for authoritative answers
        response.header.rcode = rcode as u8;

        // Place records in appropriate section based on type and response code
        match rcode {
            ResponseCode::NoError => {
                if authoritative && !records.is_empty() {
                    // Check if this is a NODATA response (SOA record only)
                    if records.len() == 1 && records[0].rtype == DNSResourceType::SOA {
                        // NODATA - SOA goes in authority section
                        response.authorities = records;
                    } else {
                        // Authoritative answer - records go in answer section
                        response.answers = records;
                    }
                } else {
                    // Delegation - NS records go in authority section
                    response.authorities = records;
                }
            }
            ResponseCode::NameError => {
                // NXDOMAIN - SOA record goes in authority section
                response.authorities = records;
            }
            _ => {
                // Other response codes - records in authority section
                response.authorities = records;
            }
        }

        // Update counts
        response.header.ancount = response.answers.len() as u16;
        response.header.nscount = response.authorities.len() as u16;
        response.header.arcount = response.resources.len() as u16;

        Ok(response)
    }

    /// Check if DNSSEC validation is enabled
    pub fn is_dnssec_enabled(&self) -> bool {
        self.dnssec_validator.is_some()
    }

    /// Create a response with zero IP (0.0.0.0 or ::) for blocked domains
    fn create_zero_ip_response(&self, query: &DNSPacket, original_id: u16) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = ResponseCode::NoError.to_u8();
        response.header.id = original_id;

        // Clear existing sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        // Add appropriate zero IP response based on query type
        if !query.questions.is_empty() {
            let question = &query.questions[0];
            match question.qtype {
                DNSResourceType::A => {
                    // Return 0.0.0.0 for A records
                    let answer = DNSResource {
                        labels: question.labels.clone(),
                        rtype: DNSResourceType::A,
                        rclass: DNSResourceClass::IN,
                        ttl: 300, // 5 minutes
                        rdlength: 4,
                        rdata: vec![0, 0, 0, 0],
                        parsed_rdata: Some("0.0.0.0".to_string()),
                        raw_class: None,
                    };
                    response.answers.push(answer);
                    response.header.ancount = 1;
                }
                DNSResourceType::AAAA => {
                    // Return :: for AAAA records
                    let answer = DNSResource {
                        labels: question.labels.clone(),
                        rtype: DNSResourceType::AAAA,
                        rclass: DNSResourceClass::IN,
                        ttl: 300, // 5 minutes
                        rdlength: 16,
                        rdata: vec![0; 16],
                        parsed_rdata: Some("::".to_string()),
                        raw_class: None,
                    };
                    response.answers.push(answer);
                    response.header.ancount = 1;
                }
                _ => {
                    // For other types, return NODATA (no answers)
                    response.header.ancount = 0;
                }
            }
        }

        response
    }

    /// Create a response with custom IP for blocked domains
    fn create_custom_ip_response(
        &self,
        query: &DNSPacket,
        original_id: u16,
        custom_ip: std::net::IpAddr,
    ) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true; // This is a response
        response.header.ra = true; // Recursion available
        response.header.rcode = ResponseCode::NoError.to_u8();
        response.header.id = original_id;

        // Clear existing sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();

        // Add appropriate custom IP response based on query type and IP version
        if !query.questions.is_empty() {
            let question = &query.questions[0];
            match (question.qtype, &custom_ip) {
                (DNSResourceType::A, std::net::IpAddr::V4(ipv4)) => {
                    // Return custom IPv4 for A records
                    let answer = DNSResource {
                        labels: question.labels.clone(),
                        rtype: DNSResourceType::A,
                        rclass: DNSResourceClass::IN,
                        ttl: 300, // 5 minutes
                        rdlength: 4,
                        rdata: ipv4.octets().to_vec(),
                        parsed_rdata: Some(ipv4.to_string()),
                        raw_class: None,
                    };
                    response.answers.push(answer);
                    response.header.ancount = 1;
                }
                (DNSResourceType::AAAA, std::net::IpAddr::V6(ipv6)) => {
                    // Return custom IPv6 for AAAA records
                    let answer = DNSResource {
                        labels: question.labels.clone(),
                        rtype: DNSResourceType::AAAA,
                        rclass: DNSResourceClass::IN,
                        ttl: 300, // 5 minutes
                        rdlength: 16,
                        rdata: ipv6.octets().to_vec(),
                        parsed_rdata: Some(ipv6.to_string()),
                        raw_class: None,
                    };
                    response.answers.push(answer);
                    response.header.ancount = 1;
                }
                _ => {
                    // Type mismatch or other types, return NODATA
                    response.header.ancount = 0;
                }
            }
        }

        response
    }
}
