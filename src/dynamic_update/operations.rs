//! Dynamic update operations and prerequisite checks

use super::UpdateError;
use crate::dns::enums::{DNSResourceClass, DNSResourceType};
use crate::zone::{Zone, ZoneRecord};
use tracing::{debug, info};

/// Prerequisite conditions for updates
#[derive(Debug, Clone, PartialEq)]
pub enum PrerequisiteCheck {
    /// RRset exists (value independent) - ANY class, type != ANY
    RRsetExists {
        name: String,
        rtype: DNSResourceType,
    },
    /// RRset exists (value dependent) - IN class
    RRsetExistsValue {
        name: String,
        rtype: DNSResourceType,
        rdata: Vec<u8>,
    },
    /// Name is in use - ANY class, type = ANY
    NameExists(String),
    /// RRset does not exist - NONE class, type != ANY
    RRsetNotExists {
        name: String,
        rtype: DNSResourceType,
    },
    /// Name is not in use - NONE class, type = ANY
    NameNotExists(String),
}

/// Update operations
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateOperation {
    /// Add to an RRset - IN class
    Add {
        name: String,
        ttl: u32,
        rtype: DNSResourceType,
        rdata: Vec<u8>,
    },
    /// Delete an RRset - ANY class, type != ANY
    DeleteRRset {
        name: String,
        rtype: DNSResourceType,
    },
    /// Delete all RRsets at a name - ANY class, type = ANY
    DeleteName(String),
    /// Delete specific RR - NONE class
    DeleteRR {
        name: String,
        rtype: DNSResourceType,
        rdata: Vec<u8>,
    },
}

/// Check if a prerequisite is satisfied
pub fn check_prerequisite(zone: &Zone, prereq: &PrerequisiteCheck) -> Result<bool, UpdateError> {
    match prereq {
        PrerequisiteCheck::RRsetExists { name, rtype } => {
            debug!("Checking if RRset exists: {} {:?}", name, rtype);
            let records = zone.get_records(name, Some(*rtype));
            Ok(!records.is_empty())
        }

        PrerequisiteCheck::RRsetExistsValue { name, rtype, rdata } => {
            debug!("Checking if RRset exists with value: {} {:?}", name, rtype);
            let records = zone.get_records(name, Some(*rtype));

            // Convert rdata bytes to string for comparison
            let rdata_str = String::from_utf8_lossy(rdata);

            Ok(records.iter().any(|record| record.rdata == rdata_str))
        }

        PrerequisiteCheck::NameExists(name) => {
            debug!("Checking if name exists: {}", name);
            // Check if any records exist for this name
            let records = zone.get_records(name, None);
            Ok(!records.is_empty())
        }

        PrerequisiteCheck::RRsetNotExists { name, rtype } => {
            debug!("Checking if RRset does not exist: {} {:?}", name, rtype);
            let records = zone.get_records(name, Some(*rtype));
            Ok(records.is_empty())
        }

        PrerequisiteCheck::NameNotExists(name) => {
            debug!("Checking if name does not exist: {}", name);
            let records = zone.get_records(name, None);
            Ok(records.is_empty())
        }
    }
}

/// Apply an update operation to the zone
pub fn apply_update(zone: &mut Zone, update: &UpdateOperation) -> Result<(), UpdateError> {
    match update {
        UpdateOperation::Add {
            name,
            ttl,
            rtype,
            rdata,
        } => {
            info!("Adding record: {} {} {:?}", name, ttl, rtype);

            // Convert rdata bytes to string
            let rdata_str = String::from_utf8_lossy(rdata).to_string();

            // Create new record
            let record = ZoneRecord::new(
                name.clone(),
                Some(*ttl),
                DNSResourceClass::IN,
                *rtype,
                rdata_str,
            );

            // Add to zone
            zone.add_record(record)
                .map_err(|e| UpdateError::UpdateFailed(format!("Failed to add record: {}", e)))?;

            Ok(())
        }

        UpdateOperation::DeleteRRset { name, rtype } => {
            info!("Deleting RRset: {} {:?}", name, rtype);

            // Get all records for this name and type
            let records_to_delete: Vec<ZoneRecord> = zone
                .get_records(name, Some(*rtype))
                .iter()
                .map(|&r| r.clone())
                .collect();

            // Delete each record
            for record in records_to_delete {
                zone.delete_record(&record).map_err(|e| {
                    UpdateError::UpdateFailed(format!("Failed to delete record: {}", e))
                })?;
            }

            Ok(())
        }

        UpdateOperation::DeleteName(name) => {
            info!("Deleting all records at name: {}", name);

            // Get all records for this name
            let records_to_delete: Vec<ZoneRecord> = zone
                .get_records(name, None)
                .iter()
                .map(|&r| r.clone())
                .collect();

            // Delete each record
            for record in records_to_delete {
                zone.delete_record(&record).map_err(|e| {
                    UpdateError::UpdateFailed(format!("Failed to delete record: {}", e))
                })?;
            }

            Ok(())
        }

        UpdateOperation::DeleteRR { name, rtype, rdata } => {
            info!("Deleting specific record: {} {:?}", name, rtype);

            let rdata_str = String::from_utf8_lossy(rdata);

            // Find the specific record
            let record_to_delete = zone
                .get_records(name, Some(*rtype))
                .iter()
                .find(|record| record.rdata == rdata_str)
                .map(|&r| r.clone());

            if let Some(record) = record_to_delete {
                zone.delete_record(&record).map_err(|e| {
                    UpdateError::UpdateFailed(format!("Failed to delete record: {}", e))
                })?;
            } else {
                debug!(
                    "Record not found for deletion: {} {:?} {}",
                    name, rtype, rdata_str
                );
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone::Zone;

    #[test]
    fn test_prerequisite_rrset_exists() {
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add a record
        let record = ZoneRecord::new(
            "www".to_string(),
            Some(300),
            DNSResourceClass::IN,
            DNSResourceType::A,
            "192.0.2.1".to_string(),
        );
        zone.add_record(record).unwrap();

        // Test exists
        let prereq = PrerequisiteCheck::RRsetExists {
            name: "www".to_string(),
            rtype: DNSResourceType::A,
        };
        assert!(check_prerequisite(&zone, &prereq).unwrap());

        // Test not exists
        let prereq = PrerequisiteCheck::RRsetExists {
            name: "www".to_string(),
            rtype: DNSResourceType::AAAA,
        };
        assert!(!check_prerequisite(&zone, &prereq).unwrap());
    }

    #[test]
    fn test_update_add_record() {
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add SOA and NS records to make zone valid
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();

        let ns = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::NS,
            "ns1.example.com.".to_string(),
        );
        zone.add_record(ns).unwrap();

        // Add a new A record
        let update = UpdateOperation::Add {
            name: "test".to_string(),
            ttl: 300,
            rtype: DNSResourceType::A,
            rdata: "192.0.2.1".as_bytes().to_vec(),
        };

        apply_update(&mut zone, &update).unwrap();

        // Verify record was added
        let records = zone.get_records("test", Some(DNSResourceType::A));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].rdata, "192.0.2.1");
    }

    #[test]
    fn test_update_delete_rrset() {
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add SOA and NS records
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();

        let ns = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::NS,
            "ns1.example.com.".to_string(),
        );
        zone.add_record(ns).unwrap();

        // Add some records
        let a1 = ZoneRecord::new(
            "www".to_string(),
            Some(300),
            DNSResourceClass::IN,
            DNSResourceType::A,
            "192.0.2.1".to_string(),
        );
        zone.add_record(a1).unwrap();

        let a2 = ZoneRecord::new(
            "www".to_string(),
            Some(300),
            DNSResourceClass::IN,
            DNSResourceType::A,
            "192.0.2.2".to_string(),
        );
        zone.add_record(a2).unwrap();

        // Delete the A RRset
        let update = UpdateOperation::DeleteRRset {
            name: "www".to_string(),
            rtype: DNSResourceType::A,
        };

        apply_update(&mut zone, &update).unwrap();

        // Verify records were deleted
        let records = zone.get_records("www", Some(DNSResourceType::A));
        assert_eq!(records.len(), 0);
    }
}
