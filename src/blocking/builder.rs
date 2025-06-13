use crate::blocking::arena::{SharedArena, StringArena};
use crate::blocking::lookup::{count_labels, DomainLabels, DomainNormalizer};
use crate::blocking::parser::{BlocklistFormat, BlocklistParser};
use crate::blocking::psl::PublicSuffixList;
use crate::blocking::trie::{CompressedTrie, NodeFlags};
use crate::error::{DnsError, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Builder for creating an efficient blocklist with PSL-based deduplication
pub struct BlocklistBuilder {
    /// Temporary storage for domains before building the trie
    domains: HashMap<Vec<u8>, BlocklistSource>,
    /// The PSL instance for deduplication
    psl: Arc<PublicSuffixList>,
    /// Enable wildcard blocking
    enable_wildcards: bool,
    /// Total domains processed (before deduplication)
    total_processed: usize,
}

/// Source information for a blocked domain
#[derive(Debug, Clone)]
pub struct BlocklistSource {
    pub list_name: String,
    pub is_wildcard: bool,
}

impl BlocklistBuilder {
    /// Create a new blocklist builder
    pub fn new(psl: Arc<PublicSuffixList>, enable_wildcards: bool) -> Self {
        Self {
            domains: HashMap::new(),
            psl,
            enable_wildcards,
            total_processed: 0,
        }
    }

    /// Add a domain to the blocklist with PSL-based deduplication
    pub fn add_domain(&mut self, domain: &str, source: &str) {
        self.total_processed += 1;

        // Handle wildcard domains
        let (domain, is_wildcard) = if let Some(stripped) = domain.strip_prefix("*.") {
            if self.enable_wildcards {
                (stripped, true)
            } else {
                return; // Skip wildcards if not enabled
            }
        } else {
            (domain, false)
        };

        // Normalize domain to lowercase bytes
        let mut domain_bytes = domain.as_bytes().to_vec();
        DomainNormalizer::normalize_in_place(&mut domain_bytes);

        // Get the registrable domain for deduplication
        if let Some(registrable_str) = self.psl.get_registrable_domain(domain) {
            let registrable_bytes = registrable_str.as_bytes().to_vec();

            // For PSL-based deduplication:
            // - If we're adding "example.com", remove all subdomains
            // - If we're adding "sub.example.com" and "example.com" exists, skip
            if domain_bytes == registrable_bytes {
                // We're adding a registrable domain - remove all subdomains
                self.domains.retain(|existing, _| {
                    if existing.len() > registrable_bytes.len() {
                        // Check if it's a subdomain
                        let suffix_start = existing.len() - registrable_bytes.len();
                        if existing[suffix_start..] == registrable_bytes &&
                           existing[suffix_start - 1] == b'.' {
                            debug!("Removing redundant subdomain: {:?}", String::from_utf8_lossy(existing));
                            false
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                });
            } else {
                // We're adding a subdomain - check if parent is already blocked
                if self.domains.contains_key(&registrable_bytes) {
                    debug!("Skipping subdomain {:?}, parent {:?} already blocked", 
                           String::from_utf8_lossy(&domain_bytes),
                           String::from_utf8_lossy(&registrable_bytes));
                    return;
                }
            }
        }

        // Add the domain
        let source = BlocklistSource {
            list_name: source.to_string(),
            is_wildcard,
        };
        
        if self.domains.insert(domain_bytes.clone(), source).is_none() {
            debug!("Added domain: {:?}", String::from_utf8_lossy(&domain_bytes));
        }
    }

    /// Load domains from a blocklist file
    pub fn load_file(
        &mut self,
        path: &Path,
        format: BlocklistFormat,
        list_name: &str,
    ) -> Result<usize> {
        info!("Loading blocklist {} from {:?} (format: {:?})", list_name, path, format);

        let file = File::open(path)
            .map_err(|e| DnsError::Io(format!("Failed to open blocklist file: {}", e)))?;
        let reader = BufReader::new(file);
        let parser = BlocklistParser::new(format);

        let initial_count = self.domains.len();

        for line in reader.lines() {
            let line = line.map_err(|e| DnsError::Io(format!("Failed to read line: {}", e)))?;

            if let Some(domain) = parser.parse_line(&line) {
                self.add_domain(&domain, list_name);
            }
        }

        let added = self.domains.len() - initial_count;
        info!("Loaded {} unique domains from {} (processed: {})", 
              added, list_name, self.total_processed - initial_count);

        Ok(added)
    }

    /// Build the final compressed trie
    pub fn build(self) -> Result<(CompressedTrie, SharedArena, usize)> {
        info!("Building compressed trie from {} unique domains (total processed: {})",
              self.domains.len(), self.total_processed);

        // Estimate arena capacity
        let avg_domain_len = 20;
        let capacity = self.domains.len() * avg_domain_len;
        let mut arena = StringArena::with_capacity(capacity);

        // Build a mapping of domain offsets to flags
        let mut entries = Vec::with_capacity(self.domains.len());

        for (domain, source) in self.domains {
            if let Some(offset) = arena.add(&domain) {
                let mut flags = NodeFlags::default();
                flags.set_blocked();
                
                if source.is_wildcard {
                    flags.set_wildcard();
                }

                entries.push((offset, flags));
            } else {
                warn!("Failed to add domain to arena: {:?}", String::from_utf8_lossy(&domain));
            }
        }

        // Convert to shared arena
        let shared_arena = arena.into_shared();
        let mut trie = CompressedTrie::new(shared_arena.clone());

        // Insert all entries into the trie
        for (offset, flags) in entries {
            if let Some(domain) = shared_arena.get(offset.0, offset.1) {
                trie.insert(domain, flags);
            }
        }

        let deduplication_savings = self.total_processed - trie.node_count();
        info!("Built trie with {} nodes (deduplicated {} domains, {:.1}% reduction)",
              trie.node_count(), deduplication_savings,
              (deduplication_savings as f64 / self.total_processed as f64) * 100.0);

        Ok((trie, shared_arena, trie.node_count()))
    }
}

/// Blocklist statistics after building
#[derive(Debug)]
pub struct BlocklistStats {
    pub total_processed: usize,
    pub unique_domains: usize,
    pub trie_nodes: usize,
    pub arena_size: usize,
    pub deduplication_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_psl_deduplication() {
        let psl = Arc::new(PublicSuffixList::new());
        // Load test PSL data
        let _ = psl.load_from_string("com\nco.uk\n");

        let mut builder = BlocklistBuilder::new(psl, false);

        // Add domains
        builder.add_domain("ads.example.com", "test");
        builder.add_domain("tracker.ads.example.com", "test");
        builder.add_domain("example.com", "test"); // Should remove the subdomains

        // Only example.com should remain
        assert_eq!(builder.domains.len(), 1);
        assert!(builder.domains.contains_key(b"example.com"));
    }

    #[test]
    fn test_wildcard_handling() {
        let psl = Arc::new(PublicSuffixList::new());
        let mut builder = BlocklistBuilder::new(psl, true);

        builder.add_domain("*.doubleclick.net", "test");
        
        assert_eq!(builder.domains.len(), 1);
        let (_, source) = builder.domains.iter().next().unwrap();
        assert!(source.is_wildcard);
    }
}