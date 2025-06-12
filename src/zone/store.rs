use super::{Result, Zone, ZoneError};
use crate::dns::enums::DNSResourceType;
use crate::dns::resource::DNSResource;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Store for managing multiple DNS zones
pub struct ZoneStore {
    /// Zones indexed by origin (lowercase)
    zones: Arc<RwLock<HashMap<String, Zone>>>,
}

impl ZoneStore {
    /// Create a new zone store
    pub fn new() -> Self {
        Self {
            zones: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a zone to the store
    pub fn add_zone(&self, zone: Zone) -> Result<()> {
        let origin = zone.origin.to_lowercase();
        info!("Adding zone: {}", origin);

        // Validate zone before adding
        zone.validate()?;

        let mut zones = self.zones.write();
        zones.insert(origin.clone(), zone);

        debug!("Zone {} added successfully", origin);
        Ok(())
    }

    /// Remove a zone from the store
    pub fn remove_zone(&self, origin: &str) -> Result<Zone> {
        let origin = origin.to_lowercase();
        info!("Removing zone: {}", origin);

        let mut zones = self.zones.write();
        zones.remove(&origin).ok_or(ZoneError::ZoneNotFound(origin))
    }

    /// Get a zone by origin
    pub fn get_zone(&self, origin: &str) -> Option<Zone> {
        let origin = origin.to_lowercase();
        let zones = self.zones.read();
        zones.get(&origin).cloned()
    }

    /// Find the zone that is authoritative for a given name
    pub fn find_zone(&self, name: &str) -> Option<Zone> {
        let name_lower = name.to_lowercase();
        let zones = self.zones.read();

        // Find the longest matching zone
        let mut best_match: Option<(&String, &Zone)> = None;
        let mut best_match_len = 0;

        for (origin, zone) in zones.iter() {
            if zone.is_authoritative_for(&name_lower) {
                let origin_len = origin.len();
                if origin_len > best_match_len {
                    best_match = Some((origin, zone));
                    best_match_len = origin_len;
                }
            }
        }

        best_match.map(|(_, zone)| zone.clone())
    }

    /// Load a zone from file
    pub fn load_zone_file<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        use super::ZoneParser;

        let path = path.as_ref();
        info!("Loading zone file: {}", path.display());

        let mut parser = ZoneParser::new();
        let zone = parser.parse_file(path)?;
        let origin = zone.origin.clone();

        self.add_zone(zone)?;

        Ok(origin)
    }

    /// Reload a zone from its file
    pub fn reload_zone(&self, origin: &str) -> Result<()> {
        let zone = self
            .get_zone(origin)
            .ok_or_else(|| ZoneError::ZoneNotFound(origin.to_string()))?;

        if let Some(file_path) = zone.file_path {
            info!("Reloading zone {} from {}", origin, file_path);
            self.load_zone_file(&file_path)?;
            Ok(())
        } else {
            Err(ZoneError::ParseError(
                "Zone has no associated file".to_string(),
            ))
        }
    }

    /// Get all zone origins
    pub fn list_zones(&self) -> Vec<String> {
        let zones = self.zones.read();
        zones.keys().cloned().collect()
    }

    /// Get zone count
    pub fn zone_count(&self) -> usize {
        let zones = self.zones.read();
        zones.len()
    }

    /// Query for records
    pub fn query(&self, name: &str, rtype: DNSResourceType) -> QueryResult {
        // Find authoritative zone
        if let Some(zone) = self.find_zone(name) {
            // Check if this is a delegation
            if let Some((delegation_point, ns_records)) = zone.find_delegation(name) {
                if delegation_point != zone.origin && delegation_point != name {
                    // This is a delegation
                    let zone_origin = zone.origin.clone();
                    let zone_ttl = zone.default_ttl;
                    return QueryResult::Delegation {
                        zone: zone_origin.clone(),
                        delegation_point,
                        ns_records: ns_records
                            .into_iter()
                            .filter_map(|r| r.to_dns_resource(&zone_origin, zone_ttl).ok())
                            .collect(),
                    };
                }
            }

            // Try to get records
            match zone.to_dns_resources(name, rtype) {
                Ok(records) => {
                    if records.is_empty() {
                        // No records of requested type
                        if zone.get_records(name, None).is_empty() {
                            // Name doesn't exist
                            QueryResult::NXDomain {
                                zone: zone.origin.clone(),
                                soa: zone.get_soa().and_then(|soa| {
                                    soa.to_dns_resource(&zone.origin, zone.default_ttl).ok()
                                }),
                            }
                        } else {
                            // Name exists but no records of this type
                            QueryResult::NoData {
                                zone: zone.origin.clone(),
                                soa: zone.get_soa().and_then(|soa| {
                                    soa.to_dns_resource(&zone.origin, zone.default_ttl).ok()
                                }),
                            }
                        }
                    } else {
                        QueryResult::Success {
                            zone: zone.origin.clone(),
                            records,
                            authoritative: true,
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to convert records: {}", e);
                    QueryResult::Error(e.to_string())
                }
            }
        } else {
            // Not authoritative
            QueryResult::NotAuthoritative
        }
    }

    /// Get statistics for all zones
    pub fn stats(&self) -> StoreStats {
        let zones = self.zones.read();
        let mut stats = StoreStats {
            zone_count: zones.len(),
            ..Default::default()
        };

        for zone in zones.values() {
            let zone_stats = zone.stats();
            stats.total_records += zone_stats.total_records;
            stats.total_a_records += zone_stats.a_records;
            stats.total_aaaa_records += zone_stats.aaaa_records;
            stats.total_ns_records += zone_stats.ns_records;
            stats.total_soa_records += zone_stats.soa_records;
        }

        stats
    }
}

impl Default for ZoneStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a zone query
#[derive(Debug, Clone)]
pub enum QueryResult {
    /// Successful query with records
    Success {
        zone: String,
        records: Vec<DNSResource>,
        authoritative: bool,
    },
    /// Name exists but no records of requested type
    NoData {
        zone: String,
        soa: Option<DNSResource>,
    },
    /// Name does not exist
    NXDomain {
        zone: String,
        soa: Option<DNSResource>,
    },
    /// Query is for a delegated subdomain
    Delegation {
        zone: String,
        delegation_point: String,
        ns_records: Vec<DNSResource>,
    },
    /// Not authoritative for this query
    NotAuthoritative,
    /// Error processing query
    Error(String),
}

impl QueryResult {
    /// Check if this is a positive result
    pub fn is_success(&self) -> bool {
        matches!(self, QueryResult::Success { .. })
    }

    /// Check if this is authoritative
    pub fn is_authoritative(&self) -> bool {
        match self {
            QueryResult::Success { authoritative, .. } => *authoritative,
            QueryResult::NoData { .. }
            | QueryResult::NXDomain { .. }
            | QueryResult::Delegation { .. } => true,
            _ => false,
        }
    }
}

/// Store statistics
#[derive(Debug, Default, Clone)]
pub struct StoreStats {
    pub zone_count: usize,
    pub total_records: usize,
    pub total_a_records: usize,
    pub total_aaaa_records: usize,
    pub total_ns_records: usize,
    pub total_soa_records: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone::{Zone, ZoneRecord};

    #[test]
    fn test_zone_store() {
        let store = ZoneStore::new();

        // Create a test zone
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add SOA record
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            crate::dns::enums::DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();

        // Add NS record
        let ns = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            crate::dns::enums::DNSResourceClass::IN,
            DNSResourceType::NS,
            "ns1.example.com.".to_string(),
        );
        zone.add_record(ns).unwrap();

        // Add zone to store
        store.add_zone(zone).unwrap();

        // Test finding zone
        assert!(store.get_zone("example.com").is_some());
        assert!(store.find_zone("www.example.com").is_some());
        assert!(store.find_zone("example.org").is_none());
    }

    #[test]
    fn test_zone_query() {
        let store = ZoneStore::new();

        // Create and populate zone
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add SOA
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            crate::dns::enums::DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();

        // Add NS
        let ns = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            crate::dns::enums::DNSResourceClass::IN,
            DNSResourceType::NS,
            "ns1.example.com.".to_string(),
        );
        zone.add_record(ns).unwrap();

        // Add A record
        let a = ZoneRecord::new(
            "www".to_string(),
            Some(3600),
            crate::dns::enums::DNSResourceClass::IN,
            DNSResourceType::A,
            "192.0.2.1".to_string(),
        );
        zone.add_record(a).unwrap();

        store.add_zone(zone).unwrap();

        // Test queries
        match store.query("www.example.com", DNSResourceType::A) {
            QueryResult::Success { records, .. } => {
                assert_eq!(records.len(), 1);
            }
            _ => panic!("Expected success"),
        }

        // Test NXDOMAIN
        match store.query("nonexistent.example.com", DNSResourceType::A) {
            QueryResult::NXDomain { .. } => {}
            _ => panic!("Expected NXDOMAIN"),
        }

        // Test NoData
        match store.query("www.example.com", DNSResourceType::AAAA) {
            QueryResult::NoData { .. } => {}
            _ => panic!("Expected NoData"),
        }
    }
}
