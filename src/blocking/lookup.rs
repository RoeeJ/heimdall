//! Zero-copy domain parsing and lookup utilities

/// Iterator over domain labels in forward order (TLD last)
pub struct DomainLabels<'a> {
    domain: &'a [u8],
    pos: usize,
}

impl<'a> DomainLabels<'a> {
    /// Create a new label iterator
    pub fn new(domain: &'a [u8]) -> Self {
        let mut domain = domain;
        // Handle trailing dot
        if domain.last() == Some(&b'.') {
            domain = &domain[..domain.len() - 1];
        }
        Self { domain, pos: 0 }
    }

    /// Get all labels in reverse order (TLD first) as a vec
    pub fn reversed(self) -> Vec<&'a [u8]> {
        let mut labels: Vec<_> = self.collect();
        labels.reverse();
        labels
    }
}

impl<'a> Iterator for DomainLabels<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.domain.len() {
            return None;
        }

        let start = self.pos;
        while self.pos < self.domain.len() && self.domain[self.pos] != b'.' {
            self.pos += 1;
        }

        let label = &self.domain[start..self.pos];

        // Skip the dot
        if self.pos < self.domain.len() {
            self.pos += 1;
        }

        Some(label)
    }
}

/// Zero-copy domain normalization
pub struct DomainNormalizer;

impl DomainNormalizer {
    /// Check if a domain needs normalization
    #[inline]
    pub fn needs_normalization(domain: &[u8]) -> bool {
        domain.iter().any(|&b| b.is_ascii_uppercase())
            || domain.is_empty()
            || (domain.len() > 1 && domain[domain.len() - 1] == b'.')
    }

    /// Normalize a domain in-place (lowercase)
    /// Returns true if the domain was modified
    pub fn normalize_in_place(domain: &mut [u8]) -> bool {
        let mut modified = false;

        for byte in domain.iter_mut() {
            if byte.is_ascii_uppercase() {
                *byte = byte.to_ascii_lowercase();
                modified = true;
            }
        }

        modified
    }

    /// Get the normalized length (without trailing dot)
    #[inline]
    pub fn normalized_len(domain: &[u8]) -> usize {
        let mut len = domain.len();
        if len > 0 && domain[len - 1] == b'.' {
            len -= 1;
        }
        len
    }
}

/// Fast label comparison (case-insensitive)
#[inline]
pub fn labels_equal_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    // Fast path for short labels (most common case)
    if a.len() <= 8 {
        for i in 0..a.len() {
            if !a[i].eq_ignore_ascii_case(&b[i]) {
                return false;
            }
        }
        true
    } else {
        // For longer labels, use vectorized comparison where possible
        a.eq_ignore_ascii_case(b)
    }
}

/// Extract the registrable part of a domain based on label count
/// For example: "sub.example.com" with suffix_labels=1 returns "example.com"
pub fn extract_registrable_part(domain: &[u8], suffix_labels: usize) -> Option<&[u8]> {
    if suffix_labels == 0 {
        return Some(domain);
    }

    let total_labels = count_labels(domain);

    // Need at least suffix_labels + 1 for a registrable domain
    if total_labels <= suffix_labels {
        return None;
    }

    // If we have exactly suffix_labels + 1, the whole domain is registrable
    if total_labels == suffix_labels + 1 {
        return Some(domain);
    }

    let mut dots_to_skip = total_labels - suffix_labels - 1;
    let mut _pos = 0;

    // Skip dots from the beginning
    if dots_to_skip == 0 {
        return Some(domain);
    }

    for (i, &byte) in domain.iter().enumerate() {
        if byte == b'.' {
            dots_to_skip -= 1;
            if dots_to_skip == 0 {
                return Some(&domain[i + 1..]);
            }
        }
        _pos = i;
    }

    // If we get here, return the whole domain
    Some(domain)
}

/// Count the number of labels in a domain
#[inline]
pub fn count_labels(domain: &[u8]) -> usize {
    if domain.is_empty() {
        return 0;
    }

    let mut count = 1;
    for &b in domain.iter() {
        if b == b'.' {
            count += 1;
        }
    }

    // Handle trailing dot
    if domain[domain.len() - 1] == b'.' {
        count -= 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_labels() {
        let labels: Vec<_> = DomainLabels::new(b"www.example.com").collect();
        assert_eq!(labels, vec![&b"www"[..], &b"example"[..], &b"com"[..]]);

        let labels: Vec<_> = DomainLabels::new(b"example.com.").collect();
        assert_eq!(labels, vec![&b"example"[..], &b"com"[..]]);

        let labels: Vec<_> = DomainLabels::new(b"com").collect();
        assert_eq!(labels, vec![b"com"]);

        let reversed = DomainLabels::new(b"www.example.com").reversed();
        assert_eq!(reversed, vec![&b"com"[..], &b"example"[..], &b"www"[..]]);
    }

    #[test]
    fn test_normalization() {
        assert!(DomainNormalizer::needs_normalization(b"Example.COM"));
        assert!(DomainNormalizer::needs_normalization(b"example.com."));
        assert!(!DomainNormalizer::needs_normalization(b"example.com"));

        let mut domain = b"Example.COM".to_vec();
        assert!(DomainNormalizer::normalize_in_place(&mut domain));
        assert_eq!(&domain, b"example.com");

        assert_eq!(DomainNormalizer::normalized_len(b"example.com."), 11);
        assert_eq!(DomainNormalizer::normalized_len(b"example.com"), 11);
    }

    #[test]
    fn test_label_comparison() {
        assert!(labels_equal_ignore_case(b"example", b"EXAMPLE"));
        assert!(labels_equal_ignore_case(b"com", b"CoM"));
        assert!(!labels_equal_ignore_case(b"example", b"examples"));
        assert!(!labels_equal_ignore_case(b"com", b"net"));
    }

    #[test]
    fn test_extract_registrable() {
        assert_eq!(
            extract_registrable_part(b"www.example.com", 1),
            Some(b"example.com".as_ref())
        );
        assert_eq!(
            extract_registrable_part(b"sub.test.example.co.uk", 2),
            Some(b"example.co.uk".as_ref())
        );
        assert_eq!(
            extract_registrable_part(b"example.com", 1),
            Some(b"example.com".as_ref())
        );
        assert_eq!(extract_registrable_part(b"com", 1), None);
    }

    #[test]
    fn test_count_labels() {
        assert_eq!(count_labels(b"www.example.com"), 3);
        assert_eq!(count_labels(b"example.com"), 2);
        assert_eq!(count_labels(b"com"), 1);
        assert_eq!(count_labels(b"www.example.com."), 3);
        assert_eq!(count_labels(b""), 0);
    }
}
