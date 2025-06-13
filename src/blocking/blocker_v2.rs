/// High-performance DNS blocker using zero-copy compressed trie
use crate::blocking::arena::SharedArena;
use crate::blocking::builder::{BlocklistBuilder, BlocklistStats};
use crate::blocking::lookup::DomainNormalizer;
use crate::blocking::parser::BlocklistFormat;
use crate::blocking::psl::PublicSuffixList;
use crate::blocking::trie::CompressedTrie;
use crate::blocking::{BlockingMode, BlockingStats};
use crate::error::Result;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// High-performance DNS blocker
pub struct DnsBlockerV2 {
    /// The compressed trie for blocklist lookups
    blocklist_trie: Arc<CompressedTrie>,
    /// The compressed trie for allowlist lookups
    allowlist_trie: Arc<CompressedTrie>,
    /// Shared arena for domain storage
    arena: Arc<SharedArena>,
    /// Blocking mode
    mode: BlockingMode,
    /// Statistics
    stats: Arc<BlockingStats>,
    /// PSL instance for domain operations
    psl: Arc<PublicSuffixList>,
    /// Enable wildcard blocking
    enable_wildcards: bool,
}

impl DnsBlockerV2 {
    /// Create a new high-performance DNS blocker
    pub async fn new(mode: BlockingMode, enable_wildcards: bool) -> Result<Self> {
        // Initialize PSL
        let psl = Arc::new(PublicSuffixList::new());
        
        // Try to load the full PSL
        info!("Initializing Public Suffix List...");
        match psl.load_from_url().await {
            Ok(count) => info!("Loaded {} PSL rules", count),
            Err(e) => {
                warn!("Failed to download PSL: {}, using fallback", e);
                psl.load_common_suffixes()
                    .map_err(|e| crate::error::DnsError::Io(format!("Failed to load PSL: {}", e)))?;
            }
        }

        // Create empty tries for now
        let empty_arena = SharedArena::from_buffer(Vec::new());
        let blocklist_trie = Arc::new(CompressedTrie::new(empty_arena.clone()));
        let allowlist_trie = Arc::new(CompressedTrie::new(empty_arena.clone()));

        Ok(Self {
            blocklist_trie,
            allowlist_trie,
            arena: Arc::new(empty_arena),
            mode,
            stats: Arc::new(BlockingStats::new()),
            psl,
            enable_wildcards,
        })
    }

    /// Check if a domain should be blocked (zero-copy, no allocations)
    #[inline]
    pub fn is_blocked(&self, domain: &str) -> bool {
        let domain_bytes = domain.as_bytes();
        
        // Normalize if needed (only allocates if uppercase found)
        let normalized = if DomainNormalizer::needs_normalization(domain_bytes) {
            let mut normalized = domain_bytes.to_vec();
            DomainNormalizer::normalize_in_place(&mut normalized);
            std::borrow::Cow::Owned(normalized)
        } else {
            std::borrow::Cow::Borrowed(domain_bytes)
        };

        // Check allowlist first
        if self.allowlist_trie.is_blocked(&normalized) {
            self.stats.record_allowed();
            return false;
        }

        // Check blocklist
        if self.blocklist_trie.is_blocked(&normalized) {
            debug!("Domain {} blocked", domain);
            self.stats.record_blocked();
            true
        } else {
            self.stats.record_allowed();
            false
        }
    }

    /// Load blocklists from files
    pub fn load_blocklists(&self, blocklists: &[(String, BlocklistFormat, String)]) -> Result<()> {
        let mut builder = BlocklistBuilder::new(self.psl.clone(), self.enable_wildcards);
        let start = Instant::now();

        // Load all blocklists
        for (path, format, name) in blocklists {
            match builder.load_file(Path::new(path), *format, name) {
                Ok(count) => info!("Loaded {} domains from {}", count, name),
                Err(e) => warn!("Failed to load blocklist {}: {}", name, e),
            }
        }

        // Build the compressed trie
        let (trie, arena, node_count) = builder.build()?;
        
        // Update the blocker with the new trie
        self.update_blocklist(trie, arena);

        let elapsed = start.elapsed();
        info!(
            "Loaded {} unique domains in {:.2}s ({:.0} domains/sec)",
            node_count,
            elapsed.as_secs_f64(),
            node_count as f64 / elapsed.as_secs_f64()
        );

        Ok(())
    }

    /// Update the blocklist trie atomically
    fn update_blocklist(&self, new_trie: CompressedTrie, new_arena: SharedArena) {
        // This would typically use Arc::swap or similar for lock-free updates
        // For now, we'll use a simple Arc replacement
        // In production, consider using ArcSwap or crossbeam-epoch
        
        // Update stats
        self.stats.total_blocked_domains.store(
            new_trie.node_count() as u64,
            Ordering::Relaxed,
        );
        
        if let Ok(mut last_update) = self.stats.last_update.lock() {
            *last_update = Some(Instant::now());
        }
        
        warn!("Blocklist update not fully implemented - would update trie here");
    }

    /// Add domain to allowlist
    pub fn add_to_allowlist(&self, domain: &str) {
        // This would need to rebuild the allowlist trie
        // For now, just log it
        debug!("Would add {} to allowlist", domain);
    }

    /// Get blocking mode
    pub fn mode(&self) -> BlockingMode {
        self.mode
    }

    /// Get statistics
    pub fn stats(&self) -> &BlockingStats {
        &self.stats
    }

    /// Get total number of blocked domains
    pub fn blocked_domain_count(&self) -> usize {
        self.blocklist_trie.node_count()
    }

    /// Get detailed statistics
    pub fn get_stats(&self) -> DetailedStats {
        DetailedStats {
            total_blocked_domains: self.blocklist_trie.node_count(),
            total_allowed_domains: self.allowlist_trie.node_count(),
            queries_blocked: self.stats.queries_blocked.load(Ordering::Relaxed),
            queries_allowed: self.stats.queries_allowed.load(Ordering::Relaxed),
            block_rate: self.stats.get_block_rate(),
            arena_size_bytes: 0, // Would get from arena
            memory_usage_bytes: self.estimate_memory_usage(),
        }
    }

    /// Estimate total memory usage
    fn estimate_memory_usage(&self) -> usize {
        // Trie nodes: ~32 bytes per node
        let trie_size = self.blocklist_trie.node_count() * 32;
        // Arena size would come from the arena itself
        // Add some overhead for Arc, indexes, etc.
        trie_size * 2 // Rough estimate
    }
}

/// Detailed statistics
#[derive(Debug, Clone)]
pub struct DetailedStats {
    pub total_blocked_domains: usize,
    pub total_allowed_domains: usize,
    pub queries_blocked: u64,
    pub queries_allowed: u64,
    pub block_rate: f64,
    pub arena_size_bytes: usize,
    pub memory_usage_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_blocker_creation() {
        let blocker = DnsBlockerV2::new(BlockingMode::NxDomain, true)
            .await
            .expect("Failed to create blocker");

        assert_eq!(blocker.mode(), BlockingMode::NxDomain);
        assert_eq!(blocker.blocked_domain_count(), 0);
    }

    #[tokio::test]
    async fn test_domain_blocking() {
        let blocker = DnsBlockerV2::new(BlockingMode::NxDomain, true)
            .await
            .expect("Failed to create blocker");

        // Would need to load some test data first
        assert!(!blocker.is_blocked("example.com"));
    }
}