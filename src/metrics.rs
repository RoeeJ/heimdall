use crate::{rate_limiter::DnsRateLimiter, resolver::DnsResolver};
use prometheus::{
    CounterVec, Encoder, Gauge, GaugeVec, HistogramVec, IntCounter, IntGauge, Registry,
    TextEncoder, histogram_opts, opts,
};

/// Prometheus metrics registry and collectors for Heimdall DNS server
pub struct DnsMetrics {
    registry: Registry,

    // Cache metrics
    cache_hits: IntCounter,
    cache_misses: IntCounter,
    cache_evictions: IntCounter,
    cache_size: IntGauge,
    cache_hit_rate: Gauge,

    // RFC 2308 negative caching metrics
    cache_negative_hits: IntCounter,
    cache_nxdomain_responses: IntCounter,
    cache_nodata_responses: IntCounter,
    cache_negative_hit_rate: Gauge,

    // Query metrics
    queries_total: CounterVec,
    query_duration: HistogramVec,
    concurrent_queries: IntGauge,
    malformed_packets: CounterVec,
    truncated_responses: CounterVec,
    error_responses: CounterVec,

    // Upstream server metrics
    upstream_requests: CounterVec,
    upstream_responses: CounterVec,
    pub upstream_response_time: HistogramVec,
    upstream_health_status: GaugeVec,
    upstream_consecutive_failures: GaugeVec,

    // Rate limiting metrics
    rate_limit_drops: CounterVec,
    active_rate_limiters: GaugeVec,

    // Connection pool metrics
    connection_pool_size: GaugeVec,

    // Server runtime metrics
    worker_threads: IntGauge,
    max_concurrent_queries: IntGauge,

    // Blocking metrics
    pub blocked_queries: IntCounter,
    blocked_domains_total: IntGauge,
    allowlist_size: IntGauge,
}

impl DnsMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // Cache metrics
        let cache_hits = IntCounter::with_opts(opts!(
            "heimdall_cache_hits_total",
            "Total number of DNS cache hits"
        ))?;

        let cache_misses = IntCounter::with_opts(opts!(
            "heimdall_cache_misses_total",
            "Total number of DNS cache misses"
        ))?;

        let cache_evictions = IntCounter::with_opts(opts!(
            "heimdall_cache_evictions_total",
            "Total number of DNS cache evictions"
        ))?;

        let cache_size = IntGauge::with_opts(opts!(
            "heimdall_cache_size",
            "Current number of entries in DNS cache"
        ))?;

        let cache_hit_rate = Gauge::with_opts(opts!(
            "heimdall_cache_hit_rate",
            "DNS cache hit rate as a percentage (0-100)"
        ))?;

        // RFC 2308 negative caching metrics
        let cache_negative_hits = IntCounter::with_opts(opts!(
            "heimdall_cache_negative_hits_total",
            "Total number of negative cache hits (NXDOMAIN/NODATA)"
        ))?;

        let cache_nxdomain_responses = IntCounter::with_opts(opts!(
            "heimdall_cache_nxdomain_responses_total",
            "Total number of NXDOMAIN responses cached"
        ))?;

        let cache_nodata_responses = IntCounter::with_opts(opts!(
            "heimdall_cache_nodata_responses_total",
            "Total number of NODATA responses cached"
        ))?;

        let cache_negative_hit_rate = Gauge::with_opts(opts!(
            "heimdall_cache_negative_hit_rate",
            "Negative cache hit rate as a percentage of total cache hits (0-100)"
        ))?;

        // Query metrics
        let queries_total = CounterVec::new(
            opts!(
                "heimdall_queries_total",
                "Total number of DNS queries processed"
            ),
            &["protocol", "query_type", "response_code"],
        )?;

        let query_duration = HistogramVec::new(
            histogram_opts!(
                "heimdall_query_duration_seconds",
                "DNS query processing duration in seconds"
            ),
            &["protocol", "cache_hit"],
        )?;

        let concurrent_queries = IntGauge::with_opts(opts!(
            "heimdall_concurrent_queries",
            "Current number of concurrent DNS queries being processed"
        ))?;

        let malformed_packets = CounterVec::new(
            opts!(
                "heimdall_malformed_packets_total",
                "Total number of malformed DNS packets received"
            ),
            &["protocol", "error_type"],
        )?;

        let truncated_responses = CounterVec::new(
            opts!(
                "heimdall_truncated_responses_total",
                "Total number of responses truncated due to UDP size limits"
            ),
            &["protocol", "reason"],
        )?;

        let error_responses = CounterVec::new(
            opts!(
                "heimdall_error_responses_total",
                "Total number of error responses by type"
            ),
            &["response_type", "protocol"],
        )?;

        // Upstream server metrics
        let upstream_requests = CounterVec::new(
            opts!(
                "heimdall_upstream_requests_total",
                "Total requests sent to upstream servers"
            ),
            &["server"],
        )?;

        let upstream_responses = CounterVec::new(
            opts!(
                "heimdall_upstream_responses_total",
                "Total responses from upstream servers"
            ),
            &["server", "status"],
        )?;

        let upstream_response_time = HistogramVec::new(
            histogram_opts!(
                "heimdall_upstream_response_time_seconds",
                "Response time from upstream DNS servers"
            ),
            &["server"],
        )?;

        let upstream_health_status = GaugeVec::new(
            opts!(
                "heimdall_upstream_health_status",
                "Health status of upstream servers (1=healthy, 0=unhealthy)"
            ),
            &["server"],
        )?;

        let upstream_consecutive_failures = GaugeVec::new(
            opts!(
                "heimdall_upstream_consecutive_failures",
                "Number of consecutive failures for upstream servers"
            ),
            &["server"],
        )?;

        // Rate limiting metrics
        let rate_limit_drops = CounterVec::new(
            opts!(
                "heimdall_rate_limit_drops_total",
                "Total number of queries dropped due to rate limiting"
            ),
            &["limiter_type", "client_ip"],
        )?;

        let active_rate_limiters = GaugeVec::new(
            opts!(
                "heimdall_active_rate_limiters",
                "Number of active rate limiters by type"
            ),
            &["limiter_type"],
        )?;

        // Connection pool metrics
        let connection_pool_size = GaugeVec::new(
            opts!(
                "heimdall_connection_pool_size",
                "Current size of connection pools"
            ),
            &["server"],
        )?;

        // Server runtime metrics
        let worker_threads = IntGauge::with_opts(opts!(
            "heimdall_worker_threads",
            "Number of worker threads configured"
        ))?;

        let max_concurrent_queries = IntGauge::with_opts(opts!(
            "heimdall_max_concurrent_queries",
            "Maximum number of concurrent queries allowed"
        ))?;

        // Blocking metrics
        let blocked_queries = IntCounter::with_opts(opts!(
            "heimdall_blocked_queries_total",
            "Total number of DNS queries blocked"
        ))?;

        let blocked_domains_total = IntGauge::with_opts(opts!(
            "heimdall_blocked_domains_total",
            "Total number of domains in blocklists"
        ))?;

        let allowlist_size = IntGauge::with_opts(opts!(
            "heimdall_allowlist_size",
            "Number of domains in allowlist"
        ))?;

        // Register all metrics
        registry.register(Box::new(cache_hits.clone()))?;
        registry.register(Box::new(cache_misses.clone()))?;
        registry.register(Box::new(cache_evictions.clone()))?;
        registry.register(Box::new(cache_size.clone()))?;
        registry.register(Box::new(cache_hit_rate.clone()))?;
        registry.register(Box::new(cache_negative_hits.clone()))?;
        registry.register(Box::new(cache_nxdomain_responses.clone()))?;
        registry.register(Box::new(cache_nodata_responses.clone()))?;
        registry.register(Box::new(cache_negative_hit_rate.clone()))?;
        registry.register(Box::new(queries_total.clone()))?;
        registry.register(Box::new(query_duration.clone()))?;
        registry.register(Box::new(concurrent_queries.clone()))?;
        registry.register(Box::new(malformed_packets.clone()))?;
        registry.register(Box::new(truncated_responses.clone()))?;
        registry.register(Box::new(error_responses.clone()))?;
        registry.register(Box::new(upstream_requests.clone()))?;
        registry.register(Box::new(upstream_responses.clone()))?;
        registry.register(Box::new(upstream_response_time.clone()))?;
        registry.register(Box::new(upstream_health_status.clone()))?;
        registry.register(Box::new(upstream_consecutive_failures.clone()))?;
        registry.register(Box::new(rate_limit_drops.clone()))?;
        registry.register(Box::new(active_rate_limiters.clone()))?;
        registry.register(Box::new(connection_pool_size.clone()))?;
        registry.register(Box::new(worker_threads.clone()))?;
        registry.register(Box::new(max_concurrent_queries.clone()))?;
        registry.register(Box::new(blocked_queries.clone()))?;
        registry.register(Box::new(blocked_domains_total.clone()))?;
        registry.register(Box::new(allowlist_size.clone()))?;

        Ok(Self {
            registry,
            cache_hits,
            cache_misses,
            cache_evictions,
            cache_size,
            cache_hit_rate,
            cache_negative_hits,
            cache_nxdomain_responses,
            cache_nodata_responses,
            cache_negative_hit_rate,
            queries_total,
            query_duration,
            concurrent_queries,
            malformed_packets,
            truncated_responses,
            error_responses,
            upstream_requests,
            upstream_responses,
            upstream_response_time,
            upstream_health_status,
            upstream_consecutive_failures,
            rate_limit_drops,
            active_rate_limiters,
            connection_pool_size,
            worker_threads,
            max_concurrent_queries,
            blocked_queries,
            blocked_domains_total,
            allowlist_size,
        })
    }

    /// Update metrics from the DNS resolver
    pub async fn update_from_resolver(
        &self,
        resolver: &DnsResolver,
        rate_limiter: Option<&DnsRateLimiter>,
    ) {
        // Update cache metrics
        if let Some(cache_stats) = resolver.cache_stats() {
            use std::sync::atomic::Ordering;

            self.cache_hits.reset();
            self.cache_hits
                .inc_by(cache_stats.hits.load(Ordering::Relaxed));
            self.cache_misses.reset();
            self.cache_misses
                .inc_by(cache_stats.misses.load(Ordering::Relaxed));
            self.cache_evictions.reset();
            self.cache_evictions.inc_by(
                cache_stats.evictions.load(Ordering::Relaxed)
                    + cache_stats.expired_evictions.load(Ordering::Relaxed),
            );
            self.cache_hit_rate.set(cache_stats.hit_rate());

            // RFC 2308 negative caching metrics
            self.cache_negative_hits.reset();
            self.cache_negative_hits
                .inc_by(cache_stats.negative_hits.load(Ordering::Relaxed));
            self.cache_nxdomain_responses.reset();
            self.cache_nxdomain_responses
                .inc_by(cache_stats.nxdomain_responses.load(Ordering::Relaxed));
            self.cache_nodata_responses.reset();
            self.cache_nodata_responses
                .inc_by(cache_stats.nodata_responses.load(Ordering::Relaxed));
            self.cache_negative_hit_rate
                .set(cache_stats.negative_hit_rate());

            // Get cache size separately
            if let Some(cache_size) = resolver.cache_size() {
                self.cache_size.set(cache_size as i64);
            }
        }

        // Update upstream server metrics
        let health_stats = resolver.get_server_health_stats();
        for (server, stats) in health_stats {
            let server_label = server.to_string();

            self.upstream_requests
                .with_label_values(&[&server_label])
                .reset();
            self.upstream_requests
                .with_label_values(&[&server_label])
                .inc_by(stats.total_requests as f64);

            let success_label = "success".to_string();
            self.upstream_responses
                .with_label_values(&[&server_label, &success_label])
                .reset();
            self.upstream_responses
                .with_label_values(&[&server_label, &success_label])
                .inc_by(stats.successful_responses as f64);

            let failed_responses = stats
                .total_requests
                .saturating_sub(stats.successful_responses);
            let failure_label = "failure".to_string();
            self.upstream_responses
                .with_label_values(&[&server_label, &failure_label])
                .reset();
            self.upstream_responses
                .with_label_values(&[&server_label, &failure_label])
                .inc_by(failed_responses as f64);

            self.upstream_health_status
                .with_label_values(&[&server_label])
                .set(if stats.is_healthy { 1.0 } else { 0.0 });

            self.upstream_consecutive_failures
                .with_label_values(&[&server_label])
                .set(stats.consecutive_failures as f64);

            // Note: Individual response times are now recorded directly in the resolver
            // This prevents the histogram buckets from all incrementing at the same rate
        }

        // Update connection pool metrics
        let pool_stats = resolver.connection_pool_stats().await;
        for (server, count) in pool_stats {
            self.connection_pool_size
                .with_label_values(&[&server.to_string()])
                .set(count as f64);
        }

        // Update rate limiting metrics if available
        if let Some(limiter) = rate_limiter {
            let limiter_stats = limiter.get_stats();
            self.active_rate_limiters
                .with_label_values(&["ip"])
                .set(limiter_stats.active_ip_limiters as f64);
            self.active_rate_limiters
                .with_label_values(&["error"])
                .set(limiter_stats.active_error_limiters as f64);
            self.active_rate_limiters
                .with_label_values(&["nxdomain"])
                .set(limiter_stats.active_nxdomain_limiters as f64);
        }
    }

    /// Update runtime configuration metrics
    pub fn update_runtime_config(&self, worker_threads: usize, max_concurrent: usize) {
        self.worker_threads.set(worker_threads as i64);
        self.max_concurrent_queries.set(max_concurrent as i64);
    }

    /// Record a query being processed
    pub fn record_query(
        &self,
        protocol: &str,
        query_type: &str,
        response_code: &str,
        duration: std::time::Duration,
        cache_hit: bool,
    ) {
        self.queries_total
            .with_label_values(&[protocol, query_type, response_code])
            .inc();
        self.query_duration
            .with_label_values(&[protocol, if cache_hit { "hit" } else { "miss" }])
            .observe(duration.as_secs_f64());
    }

    /// Record a malformed packet
    pub fn record_malformed_packet(&self, protocol: &str, error_type: &str) {
        self.malformed_packets
            .with_label_values(&[protocol, error_type])
            .inc();
    }

    /// Record a truncated response
    pub fn record_truncated_response(&self, protocol: &str, reason: &str) {
        self.truncated_responses
            .with_label_values(&[protocol, reason])
            .inc();
    }

    /// Record a rate limit drop
    pub fn record_rate_limit_drop(&self, limiter_type: &str, client_ip: &str) {
        self.rate_limit_drops
            .with_label_values(&[limiter_type, client_ip])
            .inc();
    }

    /// Record an error response (REFUSED, NOTIMPL, FORMERR, etc.)
    pub fn record_error_response(&self, response_type: &str, protocol: &str) {
        self.error_responses
            .with_label_values(&[response_type, protocol])
            .inc();
    }

    /// Set current concurrent queries count
    pub fn set_concurrent_queries(&self, count: i64) {
        self.concurrent_queries.set(count);
    }

    /// Update blocking metrics
    pub fn update_blocking_stats(&self, blocked_domains: usize, allowlist_size: usize) {
        self.blocked_domains_total.set(blocked_domains as i64);
        self.allowlist_size.set(allowlist_size as i64);
    }

    /// Export metrics in Prometheus format
    pub fn export(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }

    /// Add cluster-wide aggregated metrics
    pub async fn add_cluster_metrics(
        &self,
        buffer: &mut String,
        cluster_registry: Option<&crate::cluster_registry::ClusterRegistry>,
    ) {
        if let Some(registry) = cluster_registry {
            let members = registry.get_members().await;

            // Calculate cluster-wide totals
            let total_queries: u64 = members.iter().map(|m| m.stats.queries_total).sum();
            let total_cache_hits: u64 = members.iter().map(|m| m.stats.cache_hits).sum();
            let total_cache_misses: u64 = members.iter().map(|m| m.stats.cache_misses).sum();
            let total_cache_size: usize = members.iter().map(|m| m.stats.cache_size).sum();
            let total_errors: u64 = members.iter().map(|m| m.stats.upstream_errors).sum();

            // Calculate cluster-wide cache hit rate
            let cluster_hit_rate = if total_cache_hits + total_cache_misses > 0 {
                total_cache_hits as f64 / (total_cache_hits + total_cache_misses) as f64
            } else {
                0.0
            };

            // Add cluster metrics in Prometheus format
            buffer.push_str("\n# HELP heimdall_cluster_total_queries Total queries across all cluster members\n");
            buffer.push_str("# TYPE heimdall_cluster_total_queries counter\n");
            buffer.push_str(&format!(
                "heimdall_cluster_total_queries {}\n",
                total_queries
            ));

            buffer.push_str("# HELP heimdall_cluster_cache_hits_total Total cache hits across all cluster members\n");
            buffer.push_str("# TYPE heimdall_cluster_cache_hits_total counter\n");
            buffer.push_str(&format!(
                "heimdall_cluster_cache_hits_total {}\n",
                total_cache_hits
            ));

            buffer.push_str("# HELP heimdall_cluster_cache_misses_total Total cache misses across all cluster members\n");
            buffer.push_str("# TYPE heimdall_cluster_cache_misses_total counter\n");
            buffer.push_str(&format!(
                "heimdall_cluster_cache_misses_total {}\n",
                total_cache_misses
            ));

            buffer.push_str("# HELP heimdall_cluster_cache_hit_rate Cluster-wide cache hit rate\n");
            buffer.push_str("# TYPE heimdall_cluster_cache_hit_rate gauge\n");
            buffer.push_str(&format!(
                "heimdall_cluster_cache_hit_rate {}\n",
                cluster_hit_rate
            ));

            buffer.push_str("# HELP heimdall_cluster_cache_size_total Total cache entries across all cluster members\n");
            buffer.push_str("# TYPE heimdall_cluster_cache_size_total gauge\n");
            buffer.push_str(&format!(
                "heimdall_cluster_cache_size_total {}\n",
                total_cache_size
            ));

            buffer.push_str(
                "# HELP heimdall_cluster_errors_total Total errors across all cluster members\n",
            );
            buffer.push_str("# TYPE heimdall_cluster_errors_total counter\n");
            buffer.push_str(&format!("heimdall_cluster_errors_total {}\n", total_errors));

            buffer.push_str(
                "# HELP heimdall_cluster_members_total Total number of cluster members\n",
            );
            buffer.push_str("# TYPE heimdall_cluster_members_total gauge\n");
            buffer.push_str(&format!(
                "heimdall_cluster_members_total {}\n",
                members.len()
            ));

            // Add per-member metrics
            for member in &members {
                let labels = format!(
                    "hostname=\"{}\",pod_ip=\"{}\"",
                    member.hostname, member.pod_ip
                );

                buffer.push_str(&format!(
                    "heimdall_cluster_member_queries_total{{{}}} {}\n",
                    labels, member.stats.queries_total
                ));

                buffer.push_str(&format!(
                    "heimdall_cluster_member_cache_hit_rate{{{}}} {}\n",
                    labels,
                    if member.stats.cache_hits + member.stats.cache_misses > 0 {
                        member.stats.cache_hits as f64
                            / (member.stats.cache_hits + member.stats.cache_misses) as f64
                    } else {
                        0.0
                    }
                ));

                buffer.push_str(&format!(
                    "heimdall_cluster_member_uptime_seconds{{{}}} {}\n",
                    labels, member.stats.uptime_seconds
                ));
            }
        }
    }
}

impl Default for DnsMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create DNS metrics")
    }
}
