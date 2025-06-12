use parking_lot::RwLock;
/// Public Suffix List implementation for domain deduplication
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Trie node for efficient PSL lookups
#[derive(Debug, Default)]
struct TrieNode {
    /// Child nodes indexed by domain label
    children: HashMap<String, TrieNode>,
    /// Whether this node represents a public suffix
    is_suffix: bool,
    /// Whether this node is a wildcard (matches any label)
    is_wildcard: bool,
    /// Whether this node is an exception (overrides wildcard)
    is_exception: bool,
}

/// Public Suffix List for domain validation
pub struct PublicSuffixList {
    /// Trie structure for efficient lookups
    trie: Arc<RwLock<TrieNode>>,
    /// Whether the PSL has been loaded
    loaded: Arc<RwLock<bool>>,
}

impl PublicSuffixList {
    /// Create a new PSL instance
    pub fn new() -> Self {
        Self {
            trie: Arc::new(RwLock::new(TrieNode::default())),
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Insert a rule into the trie
    fn insert_rule(trie: &mut TrieNode, labels: Vec<&str>, is_exception: bool) {
        let mut current = trie;

        // Insert labels in reverse order (TLD first)
        for (i, label) in labels.iter().rev().enumerate() {
            let is_last = i == labels.len() - 1;

            if *label == "*" {
                current.is_wildcard = true;
                if is_last {
                    current.is_suffix = true;
                }
                return;
            }

            let label_key = label.to_lowercase();
            current = current.children.entry(label_key).or_default();

            if is_last {
                if is_exception {
                    current.is_exception = true;
                } else {
                    current.is_suffix = true;
                }
            }
        }
    }

    /// Load PSL from a string
    pub fn load_from_string(&self, content: &str) -> Result<usize, String> {
        let mut trie = self.trie.write();
        *trie = TrieNode::default(); // Clear existing data

        let mut count = 0;
        let mut _in_private_section = false;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                // Check for section markers
                if line.contains("===BEGIN PRIVATE DOMAINS===") {
                    _in_private_section = true;
                }
                continue;
            }

            // Parse the rule
            if let Some(rule) = line.strip_prefix('!') {
                // Exception rule
                let labels: Vec<&str> = rule.split('.').collect();
                Self::insert_rule(&mut trie, labels, true);
                count += 1;
            } else {
                // Regular or wildcard rule
                let labels: Vec<&str> = line.split('.').collect();
                Self::insert_rule(&mut trie, labels, false);
                count += 1;
            }
        }

        *self.loaded.write() = true;
        info!("Loaded {} PSL rules into trie", count);

        Ok(count)
    }

    /// Load PSL from the official URL
    pub async fn load_from_url(&self) -> Result<usize, String> {
        let url = "https://publicsuffix.org/list/public_suffix_list.dat";

        debug!("Downloading PSL from {}", url);

        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to download PSL: {}", e))?;

        let content = response
            .text()
            .await
            .map_err(|e| format!("Failed to read PSL content: {}", e))?;

        self.load_from_string(&content)
    }

    /// Load common suffixes as a fallback when the full PSL is not available
    pub fn load_common_suffixes(&self) -> Result<usize, String> {
        let common_suffixes = include_str!("../../assets/common_suffixes.txt");
        self.load_from_string(common_suffixes)
    }

    /// Find the public suffix length for a domain
    fn find_public_suffix_len(&self, labels: &[&str]) -> usize {
        let trie = self.trie.read();
        let mut current = &*trie;
        let mut suffix_len = 0;

        // Traverse from TLD to subdomain (reverse order)
        for (i, label) in labels.iter().rev().enumerate() {
            let label_lower = label.to_lowercase();

            // Check for exact match
            if let Some(node) = current.children.get(&label_lower) {
                current = node;

                // If this is an exception, the public suffix ends here
                if node.is_exception {
                    return i;
                }

                // If this is a suffix, update our length
                if node.is_suffix {
                    suffix_len = i + 1;
                }
            } else if current.is_wildcard {
                // Wildcard matches any label
                suffix_len = i + 1;
                // Can't traverse further with wildcard
                break;
            } else {
                // No match found
                break;
            }
        }

        suffix_len
    }

    /// Get the registrable domain (eTLD+1) for a given domain
    pub fn get_registrable_domain(&self, domain: &str) -> Option<String> {
        // Check if PSL is loaded
        if !*self.loaded.read() {
            // Fallback to simple logic if PSL not loaded
            return self.simple_registrable_domain(domain);
        }

        let domain_lower = domain.to_lowercase();
        let labels: Vec<&str> = domain_lower.split('.').collect();

        if labels.is_empty() {
            return None;
        }

        let suffix_len = self.find_public_suffix_len(&labels);

        // The registrable domain is one label longer than the public suffix
        if suffix_len > 0 && suffix_len < labels.len() {
            let registrable_start = labels.len() - suffix_len - 1;
            let registrable_labels = &labels[registrable_start..];
            Some(registrable_labels.join("."))
        } else if suffix_len == 0 && labels.len() >= 2 {
            // No public suffix found, assume simple TLD
            let registrable_labels = &labels[labels.len() - 2..];
            Some(registrable_labels.join("."))
        } else if suffix_len == 0 && labels.len() == 1 {
            // Single label domain with no PSL info - return it as-is
            Some(domain.to_string())
        } else {
            // Domain is a public suffix itself
            None
        }
    }

    /// Simple fallback logic when PSL is not loaded
    fn simple_registrable_domain(&self, domain: &str) -> Option<String> {
        let parts: Vec<&str> = domain.split('.').collect();

        // Single label domains (like TLDs) return themselves
        if parts.len() == 1 {
            return Some(domain.to_string());
        }

        if parts.len() < 2 {
            return None;
        }

        // Common multi-level TLDs
        if parts.len() >= 3 {
            let potential_tld = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
            let multi_level_tlds = [
                "co.uk", "co.jp", "co.kr", "co.za", "co.nz", "co.in", "co.il", "com.au", "com.br",
                "com.cn", "com.mx", "com.tw", "com.ar", "net.au", "net.br", "net.cn", "net.il",
                "org.uk", "org.au", "org.br", "org.cn", "org.il", "ac.uk", "gov.uk", "gov.au",
                "gov.cn", "gov.il", "edu.au", "edu.cn", "edu.mx",
            ];

            if multi_level_tlds.contains(&potential_tld.as_str()) {
                let registrable_parts = &parts[parts.len().saturating_sub(3)..];
                return Some(registrable_parts.join("."));
            }
        }

        let registrable_parts = &parts[parts.len().saturating_sub(2)..];
        Some(registrable_parts.join("."))
    }
}

/// Default PSL instance with common suffixes for fallback
impl Default for PublicSuffixList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_psl_parsing() {
        let psl = PublicSuffixList::new();

        let test_data = r#"
// Comment
com
co.uk
*.uk
!metro.tokyo.jp
tokyo.jp
"#;

        let count = psl.load_from_string(test_data).unwrap();
        assert_eq!(count, 5); // com, co.uk, *.uk, !metro.tokyo.jp, tokyo.jp

        // Test registrable domain extraction
        assert_eq!(
            psl.get_registrable_domain("example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            psl.get_registrable_domain("www.example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            psl.get_registrable_domain("example.co.uk"),
            Some("example.co.uk".to_string())
        );
        assert_eq!(
            psl.get_registrable_domain("www.example.co.uk"),
            Some("example.co.uk".to_string())
        );

        // Test wildcard - any .uk subdomain except co.uk should be a public suffix
        assert_eq!(
            psl.get_registrable_domain("example.random.uk"),
            Some("example.random.uk".to_string())
        );

        // Test exception
        assert_eq!(
            psl.get_registrable_domain("metro.tokyo.jp"),
            Some("metro.tokyo.jp".to_string())
        );
        assert_eq!(
            psl.get_registrable_domain("test.metro.tokyo.jp"),
            Some("metro.tokyo.jp".to_string())
        );

        // Test that com itself returns None (it's a TLD)
        assert_eq!(psl.get_registrable_domain("com"), None);
    }
}
