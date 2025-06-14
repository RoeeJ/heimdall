use crate::blocking::psl::PublicSuffixList;
/// DNS blocking functionality for Heimdall
/// Supports multiple blocklist formats and efficient domain blocking
use crate::error::{DnsError, Result};
use crate::pool::StringInterner;
use dashmap::DashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tracing::{debug, info, warn};

pub mod arena;
pub mod blocker_v2;
pub mod builder;
pub mod lookup;
pub mod parser;
pub mod psl;
pub mod trie;
pub mod updater;

pub use parser::{BlocklistFormat, BlocklistParser};
pub use updater::{BlocklistSource, BlocklistUpdater, default_blocklist_sources};

/// Blocking mode determines how blocked queries are handled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockingMode {
    /// Return NXDOMAIN for blocked domains
    #[default]
    NxDomain,
    /// Return 0.0.0.0 for A queries and :: for AAAA queries
    ZeroIp,
    /// Return a custom IP address
    CustomIp(std::net::IpAddr),
    /// Return REFUSED
    Refused,
}

impl BlockingMode {
    /// Parse blocking mode from string
    pub fn parse_str(mode: &str) -> Self {
        match mode.to_lowercase().as_str() {
            "nxdomain" => BlockingMode::NxDomain,
            "zero_ip" => BlockingMode::ZeroIp,
            "refused" => BlockingMode::Refused,
            _ => BlockingMode::NxDomain, // Default
        }
    }

    /// Parse blocking mode with optional custom IP
    pub fn from_str_with_ip(mode: &str, custom_ip: Option<&std::net::IpAddr>) -> Self {
        match mode.to_lowercase().as_str() {
            "nxdomain" => BlockingMode::NxDomain,
            "zero_ip" => BlockingMode::ZeroIp,
            "refused" => BlockingMode::Refused,
            "custom_ip" => {
                if let Some(ip) = custom_ip {
                    BlockingMode::CustomIp(*ip)
                } else {
                    BlockingMode::NxDomain // Fallback if no IP provided
                }
            }
            _ => BlockingMode::NxDomain, // Default
        }
    }
}

/// Statistics for blocking operations
#[derive(Debug, Default)]
pub struct BlockingStats {
    /// Total number of blocked domains
    pub total_blocked_domains: AtomicU64,
    /// Number of queries blocked
    pub queries_blocked: AtomicU64,
    /// Number of queries allowed
    pub queries_allowed: AtomicU64,
    /// Last blocklist update time
    pub last_update: std::sync::Mutex<Option<Instant>>,
}

impl BlockingStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_blocked(&self) {
        self.queries_blocked.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_allowed(&self) {
        self.queries_allowed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_block_rate(&self) -> f64 {
        let blocked = self.queries_blocked.load(Ordering::Relaxed);
        let allowed = self.queries_allowed.load(Ordering::Relaxed);
        let total = blocked + allowed;
        if total == 0 {
            0.0
        } else {
            (blocked as f64 / total as f64) * 100.0
        }
    }
}

/// DNS blocking engine
pub struct DnsBlocker {
    /// Blocked domains stored in a concurrent hashmap for fast lookups
    blocked_domains: Arc<DashMap<String, BlocklistEntry>>,
    /// Blocked domain patterns (for wildcard blocking)
    blocked_patterns: Arc<DashMap<String, BlocklistEntry>>,
    /// Allowlist for domains that should never be blocked
    allowlist: Arc<DashMap<String, ()>>,
    /// Blocking mode
    mode: BlockingMode,
    /// Statistics
    stats: Arc<BlockingStats>,
    /// Enable wildcard blocking (*.domain.com)
    enable_wildcards: bool,
    /// Public Suffix List for domain deduplication
    psl: Arc<PublicSuffixList>,
    /// Cache for normalized domains to avoid repeated allocations
    normalized_cache: Arc<DashMap<String, Arc<str>>>,
    /// String interner for domain strings
    string_interner: Arc<StringInterner>,
}

/// Source of a blocklist entry
#[derive(Debug, Clone)]
pub struct BlocklistEntry {
    /// Name of the blocklist
    pub list_name: String,
    /// When this entry was added
    pub added: Instant,
}

impl DnsBlocker {
    /// Create a new DNS blocker
    pub fn new(mode: BlockingMode, enable_wildcards: bool) -> Self {
        let psl = Arc::new(PublicSuffixList::default());

        // Try to load the embedded common suffixes as a fallback
        if let Err(e) = psl.load_common_suffixes() {
            warn!("Failed to load common suffixes: {}", e);
        }

        Self {
            blocked_domains: Arc::new(DashMap::new()),
            blocked_patterns: Arc::new(DashMap::new()),
            allowlist: Arc::new(DashMap::new()),
            mode,
            stats: Arc::new(BlockingStats::new()),
            enable_wildcards,
            psl,
            normalized_cache: Arc::new(DashMap::with_capacity(10000)),
            string_interner: Arc::new(StringInterner::new(50000)), // Intern up to 50k domains
        }
    }

    /// Initialize the PSL by downloading the full list
    pub async fn initialize_psl(&self) -> Result<()> {
        info!("Downloading Public Suffix List...");
        match self.psl.load_from_url().await {
            Ok(count) => {
                info!("Successfully loaded {} PSL rules", count);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to download PSL: {}, using fallback list", e);
                // Fallback list already loaded in new()
                Ok(())
            }
        }
    }

    /// Get the registrable domain (eTLD+1) using the Public Suffix List
    /// For example: "test1.ads.com" -> "ads.com", "test.example.co.uk" -> "example.co.uk"
    fn get_registrable_domain(&self, domain: &str) -> Option<String> {
        // For blocking purposes, single-label domains (TLDs) should return themselves
        // to prevent accidentally blocking entire TLDs
        if !domain.contains('.') {
            return Some(domain.to_string());
        }

        self.psl.get_registrable_domain(domain)
    }

    /// Get normalized (lowercase) domain from cache or create and cache it
    fn get_normalized_domain(&self, domain: &str) -> Arc<str> {
        // Fast path: check if already cached
        if let Some(normalized) = self.normalized_cache.get(domain) {
            return Arc::clone(&normalized);
        }

        // Slow path: normalize and cache
        let normalized = self.string_interner.intern(&domain.to_lowercase());
        self.normalized_cache
            .insert(domain.to_string(), Arc::clone(&normalized));
        normalized
    }

    /// Check if a domain should be blocked
    pub fn is_blocked(&self, domain: &str) -> bool {
        // Get normalized domain from cache
        let domain_normalized = self.get_normalized_domain(domain);

        // Check allowlist first
        if self.allowlist.contains_key(domain_normalized.as_ref()) {
            self.stats.record_allowed();
            return false;
        }

        // Check exact domain match
        if self
            .blocked_domains
            .contains_key(domain_normalized.as_ref())
        {
            debug!("Domain {} blocked (exact match)", domain);
            self.stats.record_blocked();
            return true;
        }

        // Check if this domain is a subdomain of any blocked domain
        // Optimize by caching the suffix checks
        let result = self.check_domain_suffixes(&domain_normalized);

        if result {
            self.stats.record_blocked();
        } else {
            self.stats.record_allowed();
        }

        result
    }

    /// Check domain suffixes efficiently
    fn check_domain_suffixes(&self, domain: &str) -> bool {
        let dot_positions: Vec<usize> = domain
            .char_indices()
            .filter_map(|(i, c)| if c == '.' { Some(i) } else { None })
            .collect();

        // Check the domain itself first
        if self.blocked_domains.contains_key(domain) {
            debug!("Domain {} blocked (exact match)", domain);
            return true;
        }

        // Check suffixes starting from each dot position
        for &pos in &dot_positions {
            let suffix = &domain[pos + 1..];

            // Check if this suffix is in the exact blocked domains
            if self.blocked_domains.contains_key(suffix) {
                debug!(
                    "Domain {} blocked (subdomain of blocked domain: {})",
                    domain, suffix
                );
                return true;
            }

            // Check wildcard patterns if enabled
            if self.enable_wildcards && self.blocked_patterns.contains_key(suffix) {
                debug!("Domain {} blocked (wildcard match: *.{})", domain, suffix);
                return true;
            }
        }

        false
    }

    /// Add domain to blocklist without deduplication (for bulk loading)
    fn add_to_blocklist_fast(&self, domain: &str, source: BlocklistEntry) {
        if let Some(stripped) = domain.strip_prefix("*.") {
            if self.enable_wildcards {
                let pattern = stripped.to_lowercase();
                self.blocked_patterns.insert(pattern, source);
            }
        } else {
            let domain_lower = domain.to_lowercase();
            self.blocked_domains.insert(domain_lower, source);
        }
    }

    /// Load blocklist from file
    pub fn load_blocklist(
        &self,
        path: &Path,
        format: BlocklistFormat,
        list_name: &str,
    ) -> Result<usize> {
        info!(
            "Loading blocklist {} from {:?} (format: {:?})",
            list_name, path, format
        );

        let file = File::open(path)
            .map_err(|e| DnsError::Io(format!("Failed to open blocklist file: {}", e)))?;
        let reader = BufReader::new(file);
        let parser = BlocklistParser::new(format);

        let mut count = 0;
        let mut line_count = 0;
        let start_time = std::time::Instant::now();

        for line in reader.lines() {
            let line = line.map_err(|e| DnsError::Io(format!("Failed to read line: {}", e)))?;
            line_count += 1;

            // Log progress every 10000 lines
            if line_count % 10000 == 0 {
                debug!(
                    "Processing blocklist {}: {} lines processed, {} domains added",
                    list_name, line_count, count
                );
            }

            if let Some(domain) = parser.parse_line(&line) {
                // For bulk loading, skip expensive deduplication
                let source = BlocklistEntry {
                    list_name: list_name.to_string(),
                    added: Instant::now(),
                };
                let before_count = self.blocked_domains.len() + self.blocked_patterns.len();
                self.add_to_blocklist_fast(&domain, source);
                let after_count = self.blocked_domains.len() + self.blocked_patterns.len();

                // Only increment count if a new entry was actually added
                if after_count > before_count {
                    count += 1;
                }
            }
        }

        let elapsed = start_time.elapsed();
        info!(
            "Processed {} lines in {:.2}s for blocklist {}",
            line_count,
            elapsed.as_secs_f32(),
            list_name
        );

        self.stats.total_blocked_domains.store(
            (self.blocked_domains.len() + self.blocked_patterns.len()) as u64,
            Ordering::Relaxed,
        );

        if let Ok(mut last_update) = self.stats.last_update.lock() {
            *last_update = Some(Instant::now());
        }

        // After bulk loading, optionally deduplicate
        if count > 1000 {
            info!(
                "Loaded {} domains from blocklist {}. Skipping deduplication for performance.",
                count, list_name
            );
        } else {
            info!("Loaded {} domains from blocklist {}", count, list_name);
        }
        Ok(count)
    }

    /// Load multiple blocklists
    pub fn load_blocklists(&self, blocklists: &[(String, BlocklistFormat, String)]) -> Result<()> {
        let mut total_loaded = 0;

        for (path, format, name) in blocklists {
            match self.load_blocklist(Path::new(path), *format, name) {
                Ok(count) => total_loaded += count,
                Err(e) => warn!("Failed to load blocklist {}: {}", name, e),
            }
        }

        info!("Total blocked domains loaded: {}", total_loaded);
        Ok(())
    }

    /// Add domain to allowlist
    pub fn add_to_allowlist(&self, domain: &str) {
        let normalized = self.get_normalized_domain(domain);
        self.allowlist.insert(normalized.to_string(), ());
        debug!("Added {} to allowlist", domain);
    }

    /// Add domain to blocklist with PSL-based intelligent deduplication
    pub fn add_to_blocklist(&self, domain: &str, source: &str) {
        let source = BlocklistEntry {
            list_name: source.to_string(),
            added: Instant::now(),
        };

        if let Some(stripped) = domain.strip_prefix("*.") {
            if self.enable_wildcards {
                let pattern = stripped.to_lowercase();
                self.blocked_patterns.insert(pattern.clone(), source);
                self.stats
                    .total_blocked_domains
                    .fetch_add(1, Ordering::Relaxed);
                debug!("Added wildcard pattern *.{} to blocklist", pattern);
            }
        } else {
            let domain_lower = self.get_normalized_domain(domain).to_string();

            // Get the registrable domain using PSL
            let registrable = self.get_registrable_domain(&domain_lower);

            // Debug logging
            debug!(
                "Adding domain: {}, registrable: {:?}",
                domain_lower, registrable
            );

            // Determine what domain to add based on PSL
            let domain_to_add = if let Some(reg_domain) = registrable {
                // If we're trying to add a subdomain, check if the parent is already blocked
                if domain_lower != reg_domain && self.blocked_domains.contains_key(&reg_domain) {
                    debug!(
                        "Domain {} already covered by blocked parent domain {}",
                        domain, reg_domain
                    );
                    return;
                }

                // If we're adding the registrable domain itself, remove any redundant subdomains
                if domain_lower == reg_domain {
                    let mut to_remove = Vec::new();
                    for entry in self.blocked_domains.iter() {
                        let existing = entry.key();
                        // Check if existing domain is a subdomain of the registrable domain
                        if existing != &reg_domain
                            && existing.ends_with(&format!(".{}", reg_domain))
                        {
                            // Verify it's actually a subdomain using PSL
                            if let Some(existing_registrable) =
                                self.get_registrable_domain(existing)
                            {
                                if existing_registrable == reg_domain {
                                    to_remove.push(existing.clone());
                                }
                            }
                        }
                    }

                    // Remove redundant entries
                    for redundant in to_remove {
                        self.blocked_domains.remove(&redundant);
                        debug!(
                            "Removed redundant domain {} (covered by {})",
                            redundant, reg_domain
                        );
                    }
                }

                // Use the original domain that was requested to be blocked
                domain_lower.clone()
            } else {
                // No registrable domain found, use the original
                domain_lower.clone()
            };

            // Add the domain
            self.blocked_domains.insert(domain_to_add.clone(), source);
            self.stats
                .total_blocked_domains
                .fetch_add(1, Ordering::Relaxed);
            debug!("Added {} to blocklist", domain);
        }
    }

    /// Add a blocked domain (convenience method)
    pub fn add_blocked_domain(&self, domain: &str) {
        self.add_to_blocklist(domain, "manual");
    }

    /// Remove domain from blocklist
    pub fn remove_from_blocklist(&self, domain: &str) -> bool {
        let domain_lower = domain.to_lowercase();
        let removed = self.blocked_domains.remove(&domain_lower).is_some();

        if removed {
            self.stats
                .total_blocked_domains
                .fetch_sub(1, Ordering::Relaxed);
            debug!("Removed {} from blocklist", domain);
        }

        removed
    }

    /// Clear all blocklists
    pub fn clear_blocklists(&self) {
        self.blocked_domains.clear();
        self.blocked_patterns.clear();
        self.stats.total_blocked_domains.store(0, Ordering::Relaxed);
        info!("Cleared all blocklists");
    }

    /// Get blocking mode
    pub fn mode(&self) -> BlockingMode {
        self.mode
    }

    /// Get blocking mode (for resolver)
    pub fn blocking_mode(&self) -> BlockingMode {
        self.mode
    }

    /// Get statistics
    pub fn stats(&self) -> &BlockingStats {
        &self.stats
    }

    /// Get total number of blocked domains
    pub fn blocked_domain_count(&self) -> usize {
        self.blocked_domains.len() + self.blocked_patterns.len()
    }

    /// Export blocklist to file
    pub fn export_blocklist(&self, path: &Path) -> Result<()> {
        use std::io::Write;

        let file = std::fs::File::create(path)
            .map_err(|e| DnsError::Io(format!("Failed to create export file: {}", e)))?;
        let mut writer = std::io::BufWriter::new(file);

        // Write exact domains
        for entry in self.blocked_domains.iter() {
            writeln!(writer, "{}", entry.key())
                .map_err(|e| DnsError::Io(format!("Failed to write domain: {}", e)))?;
        }

        // Write wildcard patterns
        for entry in self.blocked_patterns.iter() {
            writeln!(writer, "*.{}", entry.key())
                .map_err(|e| DnsError::Io(format!("Failed to write pattern: {}", e)))?;
        }

        writer
            .flush()
            .map_err(|e| DnsError::Io(format!("Failed to flush export file: {}", e)))?;

        info!(
            "Exported {} domains to {:?}",
            self.blocked_domain_count(),
            path
        );
        Ok(())
    }

    /// Get detailed statistics
    pub fn get_stats(&self) -> BlockerStats {
        BlockerStats {
            total_blocked_domains: self.blocked_domains.len() + self.blocked_patterns.len(),
            total_wildcard_rules: self.blocked_patterns.len(),
            total_exact_rules: self.blocked_domains.len(),
            allowlist_size: self.allowlist.len(),
            blocklists_loaded: 0, // This could be tracked separately if needed
        }
    }
}

/// Detailed blocker statistics
#[derive(Debug, Clone)]
pub struct BlockerStats {
    pub total_blocked_domains: usize,
    pub total_wildcard_rules: usize,
    pub total_exact_rules: usize,
    pub allowlist_size: usize,
    pub blocklists_loaded: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_domain_blocking() {
        let blocker = DnsBlocker::new(BlockingMode::NxDomain, false);

        blocker.add_to_blocklist("ads.example.com", "test");

        assert!(blocker.is_blocked("ads.example.com"));
        assert!(blocker.is_blocked("ADS.EXAMPLE.COM")); // Case insensitive
        assert!(!blocker.is_blocked("example.com"));
        assert!(!blocker.is_blocked("notads.example.com"));
    }

    #[test]
    fn test_wildcard_blocking() {
        let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

        blocker.add_to_blocklist("*.doubleclick.net", "test");

        assert!(blocker.is_blocked("ads.doubleclick.net"));
        assert!(blocker.is_blocked("tracker.ads.doubleclick.net"));
        assert!(!blocker.is_blocked("doubleclick.net")); // Wildcard should NOT match base domain
        assert!(!blocker.is_blocked("notdoubleclick.net"));
    }

    #[test]
    fn test_allowlist() {
        let blocker = DnsBlocker::new(BlockingMode::NxDomain, false);

        blocker.add_to_blocklist("example.com", "test");
        blocker.add_to_allowlist("example.com");

        assert!(!blocker.is_blocked("example.com"));
    }

    #[test]
    fn test_statistics() {
        let blocker = DnsBlocker::new(BlockingMode::NxDomain, false);

        blocker.add_to_blocklist("ads.example.com", "test");

        // Test blocked query
        assert!(blocker.is_blocked("ads.example.com"));
        assert_eq!(blocker.stats().queries_blocked.load(Ordering::Relaxed), 1);

        // Test allowed query
        assert!(!blocker.is_blocked("good.example.com"));
        assert_eq!(blocker.stats().queries_allowed.load(Ordering::Relaxed), 1);

        // Check block rate
        assert_eq!(blocker.stats().get_block_rate(), 50.0);
    }

    #[test]
    fn test_psl_behavior() {
        let blocker = DnsBlocker::new(BlockingMode::NxDomain, false);

        // Test various domains
        let test_cases = vec![
            ("example.com", "example.com"),
            ("test.example.com", "example.com"),
            ("deep.test.example.com", "example.com"),
            ("example.co.uk", "example.co.uk"),
            ("test.example.co.uk", "example.co.uk"),
            ("deep.test.example.co.uk", "example.co.uk"),
            ("bbc.co.uk", "bbc.co.uk"),
            ("com", "com"), // TLD only
        ];

        for (domain, expected) in test_cases {
            let registrable = blocker.get_registrable_domain(domain);
            assert_eq!(
                registrable,
                Some(expected.to_string()),
                "Failed for domain: {}",
                domain
            );
        }
    }
}
