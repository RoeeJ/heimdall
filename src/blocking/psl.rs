use crate::blocking::arena::{SharedArena, StringArena};
use crate::blocking::lookup::count_labels;
use crate::blocking::trie::{CompressedTrie, NodeFlags};
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info};

/// Helper function to check if a byte slice contains a subsequence
fn contains_seq(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Helper function to trim whitespace from byte slice
fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| !b.is_ascii_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    &bytes[start..end]
}

/// Temporary builder for collecting PSL entries before building the trie
struct TrieBuilder {
    entries: Vec<((u32, u16), NodeFlags)>,
}

impl TrieBuilder {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn add_entry(&mut self, offset: (u32, u16), flags: NodeFlags) {
        self.entries.push((offset, flags));
    }

    fn build(self, arena: SharedArena) -> CompressedTrie {
        let mut trie = CompressedTrie::new(arena.clone());

        // Insert all entries into the trie
        for (offset, flags) in self.entries {
            if let Some(domain) = arena.get(offset.0, offset.1) {
                trie.insert(domain, flags);
            }
        }

        trie
    }
}

/// Public Suffix List for domain validation using zero-copy compressed trie
pub struct PublicSuffixList {
    /// The compressed trie for PSL lookups
    trie: Arc<RwLock<Option<CompressedTrie>>>,
    /// Raw PSL data (kept for zero-copy operation)
    raw_data: Arc<RwLock<Option<Vec<u8>>>>,
    /// Whether the PSL has been loaded
    loaded: Arc<RwLock<bool>>,
}

impl PublicSuffixList {
    /// Create a new PSL instance
    pub fn new() -> Self {
        Self {
            trie: Arc::new(RwLock::new(None)),
            raw_data: Arc::new(RwLock::new(None)),
            loaded: Arc::new(RwLock::new(false)),
        }
    }

    /// Build the compressed trie from PSL data
    fn build_trie_from_data(data: &[u8]) -> Result<CompressedTrie, String> {
        // Estimate capacity: average domain ~20 bytes, assume 10k rules
        let mut arena = StringArena::with_capacity(200_000);
        let mut trie_builder = TrieBuilder::new();

        let mut _in_private_section = false;
        let mut count = 0;

        // Process line by line
        let mut line_start = 0;
        for i in 0..=data.len() {
            if i == data.len() || data[i] == b'\n' {
                let line = &data[line_start..i];
                line_start = i + 1;

                // Trim whitespace
                let line = trim_bytes(line);

                // Skip empty lines and comments
                if line.is_empty() || line.starts_with(b"//") {
                    if contains_seq(line, b"===BEGIN PRIVATE DOMAINS===") {
                        _in_private_section = true;
                    }
                    continue;
                }

                // Parse the rule
                let (domain, is_exception) = if line[0] == b'!' {
                    (&line[1..], true)
                } else {
                    (line, false)
                };

                // Add to arena and trie
                if let Some(offset) = arena.add(domain) {
                    let mut flags = NodeFlags::default();
                    flags.set_psl_boundary();
                    if is_exception {
                        flags.set_exception();
                    }
                    trie_builder.add_entry(offset, flags);
                    count += 1;
                }
            }
        }

        info!("Built PSL trie with {} rules", count);

        // Convert to shared arena and build final trie
        let shared_arena = arena.into_shared();
        let trie = trie_builder.build(shared_arena);

        Ok(trie)
    }

    /// Load PSL from a string (converts to bytes for zero-copy processing)
    pub fn load_from_string(&self, content: &str) -> Result<usize, String> {
        // Convert to bytes for zero-copy processing
        let data = content.as_bytes().to_vec();
        self.load_from_bytes(data)
    }

    /// Load PSL from bytes (zero-copy)
    pub fn load_from_bytes(&self, data: Vec<u8>) -> Result<usize, String> {
        // Build the trie
        let trie = Self::build_trie_from_data(&data)?;
        let rule_count = trie.node_count();

        // Store the trie and raw data
        *self.trie.write() = Some(trie);
        *self.raw_data.write() = Some(data);
        *self.loaded.write() = true;

        Ok(rule_count)
    }

    /// Load PSL from the official URL
    pub async fn load_from_url(&self) -> Result<usize, String> {
        let url = "https://publicsuffix.org/list/public_suffix_list.dat";

        debug!("Downloading PSL from {}", url);

        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to download PSL: {}", e))?;

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read PSL content: {}", e))?;

        self.load_from_bytes(bytes.to_vec())
    }

    /// Load common suffixes as a fallback when the full PSL is not available
    pub fn load_common_suffixes(&self) -> Result<usize, String> {
        let common_suffixes = include_str!("../../assets/common_suffixes.txt");
        self.load_from_string(common_suffixes)
    }

    /// Find the public suffix length for a domain
    #[allow(dead_code)]
    fn find_public_suffix_len(&self, domain: &[u8]) -> usize {
        let trie_guard = self.trie.read();
        if let Some(trie) = trie_guard.as_ref() {
            // The trie handles PSL lookup internally
            if let Some(registrable) = trie.get_registrable_domain(domain) {
                // Calculate suffix length from registrable domain
                let reg_labels = count_labels(registrable);
                let total_labels = count_labels(domain);
                total_labels - reg_labels
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Get the registrable domain (eTLD+1) for a given domain
    pub fn get_registrable_domain(&self, domain: &str) -> Option<String> {
        // Check if PSL is loaded
        if !*self.loaded.read() {
            // Fallback to simple logic if PSL not loaded
            return self.simple_registrable_domain(domain);
        }

        let domain_bytes = domain.as_bytes();
        let trie_guard = self.trie.read();

        if let Some(trie) = trie_guard.as_ref() {
            if let Some(registrable_bytes) = trie.get_registrable_domain(domain_bytes) {
                // Convert back to string
                String::from_utf8(registrable_bytes.to_vec()).ok()
            } else {
                None
            }
        } else {
            self.simple_registrable_domain(domain)
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
