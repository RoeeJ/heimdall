use super::{DnsSecAlgorithm, calculate_key_tag};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// A DNSSEC trust anchor
#[derive(Debug, Clone)]
pub struct TrustAnchor {
    /// Domain name this anchor is for
    pub domain: String,
    /// Key tag
    pub key_tag: u16,
    /// Algorithm
    pub algorithm: DnsSecAlgorithm,
    /// Public key data
    pub public_key: Vec<u8>,
    /// Flags (usually 257 for KSK)
    pub flags: u16,
    /// Protocol (always 3 for DNSSEC)
    pub protocol: u8,
}

impl TrustAnchor {
    /// Create a new trust anchor
    pub fn new(
        domain: String,
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
    ) -> Option<Self> {
        let algorithm = DnsSecAlgorithm::from_u8(algorithm)?;
        let key_tag = calculate_key_tag(flags, protocol, algorithm.to_u8(), &public_key);

        Some(Self {
            domain,
            key_tag,
            algorithm,
            public_key,
            flags,
            protocol,
        })
    }

    /// Check if this is a Key Signing Key (KSK)
    pub fn is_ksk(&self) -> bool {
        self.flags & 0x0001 != 0
    }

    /// Check if this is a Zone Signing Key (ZSK)
    pub fn is_zsk(&self) -> bool {
        self.flags & 0x0100 != 0
    }
}

/// Trust anchor store for managing DNSSEC trust anchors
pub struct TrustAnchorStore {
    /// Map of domain -> Vec<TrustAnchor>
    anchors: Arc<RwLock<HashMap<String, Vec<TrustAnchor>>>>,
}

impl TrustAnchorStore {
    /// Create a new trust anchor store with default root trust anchors
    pub fn new() -> Self {
        let mut store = Self {
            anchors: Arc::new(RwLock::new(HashMap::new())),
        };

        // Add default root trust anchors
        store.add_root_trust_anchors();

        store
    }

    /// Add the current root trust anchors
    fn add_root_trust_anchors(&mut self) {
        // Root KSK-2024 (Key Tag 20326)
        // This is the current root trust anchor as of 2024
        let root_ksk_2024 = TrustAnchor::new(
            ".".to_string(),
            257, // KSK flag
            3,   // Protocol
            8,   // RSASHA256
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                "AwEAAaz/tAm8yTn4Mfeh5eyI96WSVexTBAvkMgJzkKTOiW1vkIbzxeF3\
                +/4RgWOq7HrxRixHlFlExOLAJr5emLvN7SWXgnLh4+B5xQlNVz8Og8kv\
                ArMtNROxVQuCaSnIDdD5LKyWbRd2n9WGe2R8PzgCmr3EgVLrjyBxWezF\
                0jLHwVN8efS3rCj/EWgvIWgb9tarpVUDK/b58Da+sqqls3eNbuv7pr+e\
                oZG+SrDK6nWeL3c6H5Apxz7LjVc1uTIdsIXxuOLYA4/ilBmSVIzuDWfd\
                RUfhHdY6+cn8HFRm+2hM8AnXGXws9555KrUB5qihylGa8subX2Nn6UwN\
                R1AkUTV74bU=",
            )
            .unwrap(),
        )
        .unwrap();

        // Root KSK-2017 (Key Tag 19036) - Still active during rollover
        let root_ksk_2017 = TrustAnchor::new(
            ".".to_string(),
            257, // KSK flag
            3,   // Protocol
            8,   // RSASHA256
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                "AwEAAagAIKlVZrpC6Ia7gEzahOR+9W29euxhJhVVLOyQbSEW0O8gcCjF\
                FVQUTf6v58fLjwBd0YI0EzrAcQqBGCzh/RStIoO8g0NfnfL2MTJRkxoX\
                bfDaUeVPQuYEhg37NZWAJQ9VnMVDxP/VHL496M/QZxkjf5/Efucp2gaD\
                X6RS6CXpoY68LsvPVjR0ZSwzz1apAzvN9dlzEheX7ICJBBtuA6G3LQpz\
                W5hOA2hzCTMjJPJ8LbqF6dsV6DoBQzgul0sGIcGOYl7OyQdXfZ57relS\
                Qageu+ipAdTTJ25AsRTAoub8ONGcLmqrAmRLKBP1dfwhYB4N7knNnulq\
                QxA+Uk1ihz0=",
            )
            .unwrap(),
        )
        .unwrap();

        let mut anchors = self.anchors.write();
        anchors.insert(".".to_string(), vec![root_ksk_2024, root_ksk_2017]);
    }

    /// Add a trust anchor
    pub fn add_anchor(&self, anchor: TrustAnchor) {
        let mut anchors = self.anchors.write();
        anchors
            .entry(anchor.domain.clone())
            .or_default()
            .push(anchor);
    }

    /// Get trust anchors for a domain
    pub fn get_anchors(&self, domain: &str) -> Option<Vec<TrustAnchor>> {
        let anchors = self.anchors.read();

        // Try exact match first
        if let Some(anchors) = anchors.get(domain) {
            return Some(anchors.clone());
        }

        // Try parent domains
        let mut labels: Vec<&str> = domain.split('.').collect();
        while !labels.is_empty() {
            labels.remove(0);
            let parent = if labels.is_empty() {
                "."
            } else {
                &labels.join(".")
            };

            if let Some(anchors) = anchors.get(parent) {
                return Some(anchors.clone());
            }
        }

        None
    }

    /// Find a trust anchor by key tag
    pub fn find_by_key_tag(&self, domain: &str, key_tag: u16) -> Option<TrustAnchor> {
        self.get_anchors(domain)?
            .into_iter()
            .find(|anchor| anchor.key_tag == key_tag)
    }

    /// Clear all trust anchors (useful for testing)
    pub fn clear(&self) {
        self.anchors.write().clear();
    }

    /// Get the number of domains with trust anchors
    pub fn domain_count(&self) -> usize {
        self.anchors.read().len()
    }
}

impl Default for TrustAnchorStore {
    fn default() -> Self {
        Self::new()
    }
}
