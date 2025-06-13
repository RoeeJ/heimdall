use crate::blocking::arena::SharedArena;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Node index in the trie
type NodeIndex = u32;

/// Flags packed into a single byte
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeFlags(u8);

impl NodeFlags {
    const BLOCKED: u8 = 0b00000001;
    const WILDCARD: u8 = 0b00000010;
    const PSL_BOUNDARY: u8 = 0b00000100;
    const EXCEPTION: u8 = 0b00001000;


    #[inline]
    fn is_blocked(self) -> bool {
        self.0 & Self::BLOCKED != 0
    }

    #[inline]
    pub fn set_blocked(&mut self) {
        self.0 |= Self::BLOCKED;
    }

    #[inline]
    fn is_wildcard(self) -> bool {
        self.0 & Self::WILDCARD != 0
    }

    #[inline]
    fn set_wildcard(&mut self) {
        self.0 |= Self::WILDCARD;
    }

    #[inline]
    fn is_psl_boundary(self) -> bool {
        self.0 & Self::PSL_BOUNDARY != 0
    }

    #[inline]
    pub fn set_psl_boundary(&mut self) {
        self.0 |= Self::PSL_BOUNDARY;
    }

    #[inline]
    fn is_exception(self) -> bool {
        self.0 & Self::EXCEPTION != 0
    }

    #[inline]
    pub fn set_exception(&mut self) {
        self.0 |= Self::EXCEPTION;
    }
}

/// A node in the compressed trie
#[derive(Debug, Clone)]
struct TrieNode {
    /// Label stored as offset and length in the arena
    label: (u32, u16),
    /// Child nodes indexed by first byte of their label
    children: SmallVec<[(u8, NodeIndex); 4]>,
    /// Packed flags for this node
    flags: NodeFlags,
}

impl TrieNode {
    fn new(label: (u32, u16)) -> Self {
        Self {
            label,
            children: SmallVec::new(),
            flags: NodeFlags::default(),
        }
    }

    /// Add a child node, maintaining sorted order by first byte
    fn add_child(&mut self, first_byte: u8, index: NodeIndex) {
        match self.children.binary_search_by_key(&first_byte, |&(b, _)| b) {
            Ok(pos) => self.children[pos] = (first_byte, index),
            Err(pos) => self.children.insert(pos, (first_byte, index)),
        }
    }

    /// Find a child node by its first byte
    #[inline]
    fn find_child(&self, first_byte: u8) -> Option<NodeIndex> {
        self.children
            .binary_search_by_key(&first_byte, |&(b, _)| b)
            .ok()
            .map(|pos| self.children[pos].1)
    }
}

/// Compressed trie for efficient domain lookups
pub struct CompressedTrie {
    /// Arena containing all domain strings
    arena: SharedArena,
    /// All nodes in the trie
    nodes: Vec<TrieNode>,
    /// Root nodes indexed by TLD hash for fast lookup
    roots: FxHashMap<u32, NodeIndex>,
}

impl CompressedTrie {
    /// Create a new compressed trie
    pub fn new(arena: SharedArena) -> Self {
        Self {
            arena,
            nodes: Vec::new(),
            roots: FxHashMap::default(),
        }
    }

    /// Get the arena reference
    pub fn arena(&self) -> &SharedArena {
        &self.arena
    }

    /// Get the number of nodes in the trie
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Add a node to the trie and return its index
    fn add_node(&mut self, label: (u32, u16)) -> NodeIndex {
        let index = self.nodes.len() as NodeIndex;
        self.nodes.push(TrieNode::new(label));
        index
    }

    /// Insert a domain into the trie
    pub fn insert(&mut self, domain: &[u8], flags: NodeFlags) {
        // Split domain into labels (in reverse order for efficient lookup)
        let labels = self.split_labels(domain);
        if labels.is_empty() {
            return;
        }

        // Start from TLD
        let tld = labels[0];
        let tld_hash = self.hash_label(tld);

        // Get or create root node for this TLD
        let mut current_idx = match self.roots.get(&tld_hash).copied() {
            Some(idx) => {
                // Check if this is the exact TLD
                let node = &self.nodes[idx as usize];
                if self.labels_match(node.label, tld) {
                    idx
                } else {
                    // Hash collision, need to create a new root
                    let tld_offset = self.find_or_add_to_arena(tld);
                    let new_idx = self.add_node(tld_offset);
                    self.roots.insert(self.hash_label_with_salt(tld, 1), new_idx);
                    new_idx
                }
            }
            None => {
                // Create new root node
                let tld_offset = self.find_or_add_to_arena(tld);
                let new_idx = self.add_node(tld_offset);
                self.roots.insert(tld_hash, new_idx);
                new_idx
            }
        };

        // Process remaining labels
        for (i, &label) in labels.iter().enumerate().skip(1) {
            let is_last = i == labels.len() - 1;
            
            // Special handling for wildcards
            if label == b"*" {
                self.nodes[current_idx as usize].flags.set_wildcard();
                if is_last {
                    self.nodes[current_idx as usize].flags |= flags;
                }
                return;
            }

            // Find or create child node
            let first_byte = label[0];
            let child_idx = match self.nodes[current_idx as usize].find_child(first_byte) {
                Some(idx) => {
                    // Check if labels match
                    let node = &self.nodes[idx as usize];
                    if self.labels_match(node.label, label) {
                        idx
                    } else {
                        // Different label with same first byte, create new node
                        let label_offset = self.find_or_add_to_arena(label);
                        let new_idx = self.add_node(label_offset);
                        self.nodes[current_idx as usize].add_child(first_byte, new_idx);
                        new_idx
                    }
                }
                None => {
                    // Create new child
                    let label_offset = self.find_or_add_to_arena(label);
                    let new_idx = self.add_node(label_offset);
                    self.nodes[current_idx as usize].add_child(first_byte, new_idx);
                    new_idx
                }
            };

            current_idx = child_idx;

            // Set flags on the last node
            if is_last {
                self.nodes[current_idx as usize].flags |= flags;
            }
        }
    }

    /// Check if a domain is blocked
    pub fn is_blocked(&self, domain: &[u8]) -> bool {
        let labels = self.split_labels(domain);
        if labels.is_empty() {
            return false;
        }

        // Start from TLD
        let tld_hash = self.hash_label(labels[0]);
        let mut current_idx = match self.roots.get(&tld_hash) {
            Some(&idx) => {
                // Verify it's the correct TLD
                let node = &self.nodes[idx as usize];
                if !self.labels_match(node.label, labels[0]) {
                    // Try with salt
                    if let Some(&idx) = self.roots.get(&self.hash_label_with_salt(labels[0], 1)) {
                        let node = &self.nodes[idx as usize];
                        if !self.labels_match(node.label, labels[0]) {
                            return false;
                        }
                        idx
                    } else {
                        return false;
                    }
                } else {
                    idx
                }
            }
            None => return false,
        };

        // Check if TLD itself is blocked
        let mut current_node = &self.nodes[current_idx as usize];
        if current_node.flags.is_blocked() {
            return true;
        }

        // Traverse the trie
        for (i, &label) in labels.iter().enumerate().skip(1) {
            // Check wildcard at current level
            if current_node.flags.is_wildcard() && i > 1 {
                // Wildcard matches any subdomain
                return true;
            }

            // Find child node
            let first_byte = label[0];
            match current_node.find_child(first_byte) {
                Some(child_idx) => {
                    let child_node = &self.nodes[child_idx as usize];
                    
                    // Verify label matches
                    if !self.labels_match(child_node.label, label) {
                        return false;
                    }

                    current_idx = child_idx;
                    current_node = child_node;

                    // Check if this node is blocked
                    if current_node.flags.is_blocked() {
                        return true;
                    }
                }
                None => return false,
            }
        }

        false
    }

    /// Get the registrable domain using PSL boundaries
    pub fn get_registrable_domain<'a>(&self, domain: &'a [u8]) -> Option<&'a [u8]> {
        let labels = self.split_labels(domain);
        if labels.is_empty() {
            return None;
        }

        // Find the PSL boundary
        let tld_hash = self.hash_label(labels[0]);
        let mut current_idx = self.roots.get(&tld_hash).copied()?;
        let mut psl_depth = 0;

        // Check TLD node
        let mut current_node = &self.nodes[current_idx as usize];
        if !self.labels_match(current_node.label, labels[0]) {
            return None;
        }
        if current_node.flags.is_psl_boundary() {
            psl_depth = 1;
        }

        // Traverse to find deepest PSL boundary
        for (i, &label) in labels.iter().enumerate().skip(1) {
            if current_node.flags.is_wildcard() && !current_node.flags.is_exception() {
                // Wildcard PSL rule
                psl_depth = i + 1;
                break;
            }

            let first_byte = label[0];
            match current_node.find_child(first_byte) {
                Some(child_idx) => {
                    let child_node = &self.nodes[child_idx as usize];
                    if !self.labels_match(child_node.label, label) {
                        break;
                    }

                    current_idx = child_idx;
                    current_node = child_node;

                    if current_node.flags.is_exception() {
                        // Exception rule - PSL boundary is one level up
                        psl_depth = i;
                        break;
                    } else if current_node.flags.is_psl_boundary() {
                        psl_depth = i + 1;
                    }
                }
                None => break,
            }
        }

        // Registrable domain is one label beyond PSL boundary
        if psl_depth > 0 && psl_depth < labels.len() {
            // Find the byte position where registrable domain starts
            let mut pos = 0;
            let mut label_count = 0;
            for i in 0..domain.len() {
                if domain[i] == b'.' {
                    label_count += 1;
                    if label_count == labels.len() - psl_depth - 1 {
                        pos = i + 1;
                        break;
                    }
                }
            }
            Some(&domain[pos..])
        } else if psl_depth == 0 && labels.len() >= 2 {
            // No PSL info, assume simple TLD
            let dot_pos = domain.iter().rposition(|&b| b == b'.')?;
            let second_dot_pos = domain[..dot_pos].iter().rposition(|&b| b == b'.')?;
            Some(&domain[second_dot_pos + 1..])
        } else {
            None
        }
    }

    /// Split domain into labels in reverse order
    fn split_labels(&self, domain: &[u8]) -> Vec<&[u8]> {
        if domain.is_empty() {
            return Vec::new();
        }

        let mut labels = Vec::new();
        let mut end = domain.len();

        // Handle trailing dot
        if domain[end - 1] == b'.' {
            end -= 1;
        }

        let mut start = end;
        while start > 0 {
            start -= 1;
            if domain[start] == b'.' {
                labels.push(&domain[start + 1..end]);
                end = start;
            }
        }
        labels.push(&domain[0..end]);

        labels
    }

    /// Hash a label for root lookup
    #[inline]
    fn hash_label(&self, label: &[u8]) -> u32 {
        // Simple FNV-1a hash
        let mut hash = 2166136261u32;
        for &byte in label {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(16777619);
        }
        hash
    }

    /// Hash with salt for collision handling
    #[inline]
    fn hash_label_with_salt(&self, label: &[u8], salt: u32) -> u32 {
        let mut hash = self.hash_label(label);
        hash ^= salt;
        hash.wrapping_mul(16777619)
    }

    /// Check if arena label matches the given label
    #[inline]
    fn labels_match(&self, arena_ref: (u32, u16), label: &[u8]) -> bool {
        if let Some(stored) = self.arena.get(arena_ref.0, arena_ref.1) {
            stored.eq_ignore_ascii_case(label)
        } else {
            false
        }
    }

    /// Find label in arena or return a placeholder
    /// (In the full implementation, this would be integrated with arena building)
    fn find_or_add_to_arena(&self, label: &[u8]) -> (u32, u16) {
        // For now, we'll create a temporary allocation
        // In production, this would be handled by the builder
        (0, label.len() as u16)
    }
}

// Implement bitwise OR for NodeFlags
impl std::ops::BitOr for NodeFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        NodeFlags(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for NodeFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocking::arena::StringArena;

    #[test]
    fn test_node_flags() {
        let mut flags = NodeFlags::default();
        assert!(!flags.is_blocked());
        
        flags.set_blocked();
        assert!(flags.is_blocked());
        
        flags.set_wildcard();
        assert!(flags.is_wildcard());
        assert!(flags.is_blocked()); // Should still be blocked
    }

    #[test]
    fn test_trie_node() {
        let mut node = TrieNode::new((0, 3));
        
        node.add_child(b'a', 1);
        node.add_child(b'c', 3);
        node.add_child(b'b', 2);
        
        // Should be sorted
        assert_eq!(node.find_child(b'a'), Some(1));
        assert_eq!(node.find_child(b'b'), Some(2));
        assert_eq!(node.find_child(b'c'), Some(3));
        assert_eq!(node.find_child(b'd'), None);
    }

    #[test]
    fn test_split_labels() {
        let mut arena = StringArena::with_capacity(1024);
        let shared = arena.into_shared();
        let trie = CompressedTrie::new(shared);
        
        let labels = trie.split_labels(b"www.example.com");
        assert_eq!(labels, vec![b"com", b"example", b"www"]);
        
        let labels = trie.split_labels(b"example.co.uk");
        assert_eq!(labels, vec![b"uk", b"co", b"example"]);
        
        let labels = trie.split_labels(b"com");
        assert_eq!(labels, vec![b"com"]);
        
        let labels = trie.split_labels(b"example.com.");
        assert_eq!(labels, vec![b"com", b"example"]);
    }
}