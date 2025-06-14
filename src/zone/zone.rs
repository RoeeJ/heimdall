use super::{Result, ZoneError, ZoneRecord};
use crate::dns::enums::{DNSResourceClass, DNSResourceType};
use crate::dns::resource::DNSResource;
use std::collections::HashMap;
use std::time::SystemTime;

/// Represents a DNS zone with all its records
#[derive(Debug, Clone)]
pub struct Zone {
    /// Zone origin (e.g., "example.com")
    pub origin: String,
    /// Default TTL for records without explicit TTL
    pub default_ttl: u32,
    /// Zone serial number (from SOA)
    pub serial: u32,
    /// All records in the zone, indexed by name
    records: HashMap<String, Vec<ZoneRecord>>,
    /// The SOA record for this zone
    soa_record: Option<ZoneRecord>,
    /// Zone class (usually IN)
    pub class: DNSResourceClass,
    /// Zone file path (if loaded from file)
    pub file_path: Option<String>,
    /// Last modified time
    pub last_modified: SystemTime,
}

impl Zone {
    /// Create a new empty zone
    pub fn new(origin: String, default_ttl: u32) -> Self {
        Self {
            origin: origin.trim_end_matches('.').to_lowercase(),
            default_ttl,
            serial: Self::generate_serial(),
            records: HashMap::new(),
            soa_record: None,
            class: DNSResourceClass::IN,
            file_path: None,
            last_modified: SystemTime::now(),
        }
    }

    /// Generate a serial number based on current date (YYYYMMDDNN format)
    fn generate_serial() -> u32 {
        use chrono::{Datelike, Local};

        let now = Local::now();
        // Add a sequence number (00-99) for multiple updates per day
        now.year() as u32 * 1000000 + now.month() * 10000 + now.day() * 100
    }

    /// Add a record to the zone
    pub fn add_record(&mut self, record: ZoneRecord) -> Result<()> {
        // Special handling for SOA record
        if record.rtype == DNSResourceType::SOA {
            if self.soa_record.is_some() {
                return Err(ZoneError::DuplicateSOA);
            }

            // Parse SOA to extract serial
            let parts: Vec<&str> = record.rdata.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(serial) = parts[2].parse::<u32>() {
                    self.serial = serial;
                }
            }

            self.soa_record = Some(record.clone());
        }

        // Normalize the record name
        let normalized_name = self.normalize_name(&record.name)?;

        // Add to records map - ALL records including SOA
        self.records
            .entry(normalized_name)
            .or_default()
            .push(record);

        self.last_modified = SystemTime::now();
        Ok(())
    }

    /// Get all records for a given name and type
    pub fn get_records(&self, name: &str, rtype: Option<DNSResourceType>) -> Vec<&ZoneRecord> {
        let normalized = match self.normalize_name(name) {
            Ok(n) => n,
            Err(_) => return Vec::new(),
        };

        self.records
            .get(&normalized)
            .map(|records| {
                records
                    .iter()
                    .filter(|r| rtype.is_none() || r.rtype == rtype.unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get the SOA record for this zone
    pub fn get_soa(&self) -> Option<&ZoneRecord> {
        self.soa_record.as_ref()
    }

    /// Check if this zone is authoritative for a given name
    pub fn is_authoritative_for(&self, name: &str) -> bool {
        let name_lower = name.to_lowercase();
        let origin_lower = self.origin.to_lowercase();

        name_lower == origin_lower || name_lower.ends_with(&format!(".{}", origin_lower))
    }

    /// Validate the zone
    pub fn validate(&self) -> Result<()> {
        // Must have SOA record
        if self.soa_record.is_none() {
            return Err(ZoneError::MissingSOA);
        }

        // Should have at least one NS record for the origin
        let ns_records = self.get_records(&self.origin, Some(DNSResourceType::NS));
        if ns_records.is_empty() {
            return Err(ZoneError::ValidationError(
                "Zone must have at least one NS record".to_string(),
            ));
        }

        // Validate all records can be converted
        for (name, records) in &self.records {
            for record in records {
                record
                    .to_dns_resource(&self.origin, self.default_ttl)
                    .map_err(|e| ZoneError::InvalidRecord(format!("{}: {}", name, e)))?;
            }
        }

        Ok(())
    }

    /// Convert zone records to DNS resources for a query
    pub fn to_dns_resources(&self, name: &str, rtype: DNSResourceType) -> Result<Vec<DNSResource>> {
        let records = self.get_records(name, Some(rtype));

        records
            .into_iter()
            .map(|record| {
                record
                    .to_dns_resource(&self.origin, self.default_ttl)
                    .map_err(ZoneError::InvalidRecord)
            })
            .collect()
    }

    /// Get all NS records for the zone
    pub fn get_ns_records(&self) -> Vec<&ZoneRecord> {
        self.get_records(&self.origin, Some(DNSResourceType::NS))
    }

    /// Find the best matching delegation for a name
    pub fn find_delegation(&self, name: &str) -> Option<(String, Vec<&ZoneRecord>)> {
        let name_lower = name.to_lowercase();

        // Check each possible delegation point from most specific to least
        let labels: Vec<&str> = name_lower.split('.').collect();

        for i in 0..labels.len() {
            let potential_delegation = labels[i..].join(".");

            // Skip if it's the zone origin itself
            if potential_delegation == self.origin {
                continue;
            }

            // Check if we have NS records for this subdomain
            let ns_records = self.get_records(&potential_delegation, Some(DNSResourceType::NS));
            if !ns_records.is_empty() {
                return Some((potential_delegation, ns_records));
            }
        }

        None
    }

    /// Normalize a domain name relative to the zone origin
    fn normalize_name(&self, name: &str) -> Result<String> {
        let name = name.trim();

        // If origin is empty, defer normalization by keeping the original name
        if self.origin.is_empty() {
            return Ok(name.to_lowercase());
        }

        let name_lower = name.to_lowercase();
        let origin_lower = self.origin.to_lowercase();

        if name == "@" {
            Ok(origin_lower)
        } else if name.ends_with('.') {
            // Fully qualified - remove trailing dot
            Ok(name.trim_end_matches('.').to_lowercase())
        } else if name.is_empty() {
            Ok(origin_lower)
        } else if name_lower == origin_lower {
            // Already the origin name
            Ok(origin_lower)
        } else if name_lower.ends_with(&format!(".{}", origin_lower)) {
            // Already fully qualified (without trailing dot)
            Ok(name_lower)
        } else {
            // Relative name
            Ok(format!("{}.{}", name_lower, origin_lower))
        }
    }

    /// Get zone statistics
    pub fn stats(&self) -> ZoneStats {
        let mut stats = ZoneStats::default();

        for records in self.records.values() {
            for record in records {
                stats.total_records += 1;
                match record.rtype {
                    DNSResourceType::A => stats.a_records += 1,
                    DNSResourceType::AAAA => stats.aaaa_records += 1,
                    DNSResourceType::NS => stats.ns_records += 1,
                    DNSResourceType::CNAME => stats.cname_records += 1,
                    DNSResourceType::MX => stats.mx_records += 1,
                    DNSResourceType::TXT => stats.txt_records += 1,
                    DNSResourceType::SOA => stats.soa_records += 1,
                    _ => stats.other_records += 1,
                }
            }
        }

        stats
    }

    /// Get all unique domain names in the zone
    pub fn get_all_names(&self) -> Vec<&String> {
        self.records.keys().collect()
    }

    /// Iterate over all records in the zone
    pub fn records(&self) -> impl Iterator<Item = &ZoneRecord> {
        self.records.values().flat_map(|records| records.iter())
    }

    /// Delete a record from the zone
    pub fn delete_record(&mut self, record: &ZoneRecord) -> Result<()> {
        let normalized_name = self.normalize_name(&record.name)?;
        let should_remove_name;
        let was_soa_removed;

        // Use a scope to limit the borrow of self.records
        {
            if let Some(records) = self.records.get_mut(&normalized_name) {
                let initial_len = records.len();

                // Remove matching records
                records.retain(|r| !(r.rtype == record.rtype && r.rdata == record.rdata));

                should_remove_name = records.is_empty();
                was_soa_removed =
                    record.rtype == DNSResourceType::SOA && records.len() < initial_len;
            } else {
                self.last_modified = SystemTime::now();
                return Ok(()); // Record not found is not an error for delete
            }
        }

        // Now we can safely modify self.records again
        if should_remove_name {
            self.records.remove(&normalized_name);
        }

        // Update SOA record tracking if needed
        if was_soa_removed {
            self.soa_record = None;
        }

        self.last_modified = SystemTime::now();
        Ok(())
    }

    /// Update zone serial number
    pub fn update_serial(&mut self) {
        self.serial = Self::generate_serial();

        // Update SOA record if present
        if let Some(soa) = &mut self.soa_record {
            // Parse and update SOA serial
            let parts: Vec<&str> = soa.rdata.split_whitespace().collect();
            if parts.len() == 7 {
                let new_rdata = format!(
                    "{} {} {} {} {} {} {}",
                    parts[0], parts[1], self.serial, parts[3], parts[4], parts[5], parts[6]
                );
                soa.rdata = new_rdata;
            }
        }

        self.last_modified = SystemTime::now();
    }
}

/// Zone statistics
#[derive(Debug, Default, Clone)]
pub struct ZoneStats {
    pub total_records: usize,
    pub a_records: usize,
    pub aaaa_records: usize,
    pub ns_records: usize,
    pub cname_records: usize,
    pub mx_records: usize,
    pub txt_records: usize,
    pub soa_records: usize,
    pub other_records: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_creation() {
        let zone = Zone::new("example.com".to_string(), 3600);
        assert_eq!(zone.origin, "example.com");
        assert_eq!(zone.default_ttl, 3600);
        assert!(zone.serial > 2024000000); // Should be date-based
    }

    #[test]
    fn test_is_authoritative() {
        let zone = Zone::new("example.com".to_string(), 3600);

        assert!(zone.is_authoritative_for("example.com"));
        assert!(zone.is_authoritative_for("www.example.com"));
        assert!(zone.is_authoritative_for("sub.domain.example.com"));
        assert!(!zone.is_authoritative_for("example.org"));
        assert!(!zone.is_authoritative_for("com"));
    }

    #[test]
    fn test_normalize_name() {
        let zone = Zone::new("example.com".to_string(), 3600);

        assert_eq!(zone.normalize_name("@").unwrap(), "example.com");
        assert_eq!(zone.normalize_name("").unwrap(), "example.com");
        assert_eq!(zone.normalize_name("www").unwrap(), "www.example.com");
        assert_eq!(
            zone.normalize_name("www.example.com.").unwrap(),
            "www.example.com"
        );
    }
}
