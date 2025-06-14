//! RFC 2136 Dynamic DNS Update implementation
//!
//! This module provides secure dynamic DNS update functionality with:
//! - TSIG authentication for secure updates
//! - Prerequisite checking for conditional updates
//! - Add, delete, and replace operations
//! - Policy-based access control

use crate::dns::resource::DNSResource;
use crate::dns::{DNSPacket, enums::*};
use crate::zone::{Zone, ZoneStore};
use std::sync::Arc;
use tracing::{debug, info, warn};

pub mod operations;
pub mod policy;
pub mod tsig;

pub use operations::{PrerequisiteCheck, UpdateOperation};
pub use policy::{UpdatePermission, UpdatePolicy};
pub use tsig::{TsigAlgorithm, TsigKey, TsigVerifier};

/// Errors that can occur during dynamic updates
#[derive(Debug, Clone)]
pub enum UpdateError {
    /// The zone is not found or not authoritative
    NotAuth(String),
    /// The update was refused due to policy
    Refused(String),
    /// TSIG authentication failed
    NotVerified(String),
    /// A prerequisite was not satisfied
    PrereqFailed(String),
    /// The update operation failed
    UpdateFailed(String),
    /// Internal server error
    ServerError(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NotAuth(msg) => write!(f, "Not authoritative: {}", msg),
            UpdateError::Refused(msg) => write!(f, "Update refused: {}", msg),
            UpdateError::NotVerified(msg) => write!(f, "TSIG verification failed: {}", msg),
            UpdateError::PrereqFailed(msg) => write!(f, "Prerequisite failed: {}", msg),
            UpdateError::UpdateFailed(msg) => write!(f, "Update failed: {}", msg),
            UpdateError::ServerError(msg) => write!(f, "Server error: {}", msg),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Dynamic DNS update processor
pub struct DynamicUpdateProcessor {
    zone_store: Arc<ZoneStore>,
    tsig_verifier: TsigVerifier,
    update_policy: UpdatePolicy,
}

impl DynamicUpdateProcessor {
    /// Create a new dynamic update processor
    pub fn new(
        zone_store: Arc<ZoneStore>,
        tsig_keys: Vec<TsigKey>,
        update_policy: UpdatePolicy,
    ) -> Self {
        Self {
            zone_store,
            tsig_verifier: TsigVerifier::new(tsig_keys),
            update_policy,
        }
    }

    /// Process a DNS UPDATE message
    pub async fn process_update(&self, packet: &DNSPacket) -> Result<DNSPacket, UpdateError> {
        // Verify this is an UPDATE opcode
        if packet.header.opcode != 5 {
            return Err(UpdateError::ServerError(
                "Not an UPDATE message".to_string(),
            ));
        }

        info!("Processing DNS UPDATE for id={}", packet.header.id);

        // Extract zone from question section
        let zone_name = if packet.questions.is_empty() {
            return Err(UpdateError::ServerError(
                "No zone specified in UPDATE".to_string(),
            ));
        } else {
            packet.questions[0].labels.join(".")
        };

        // Check if we're authoritative for this zone
        if self.zone_store.find_zone(&zone_name).is_none() {
            warn!("UPDATE for non-authoritative zone: {}", zone_name);
            return Err(UpdateError::NotAuth(format!(
                "Not authoritative for zone {}",
                zone_name
            )));
        }

        // Verify TSIG if present
        let authenticated_key = if let Some(tsig_record) = self.extract_tsig(packet) {
            match self.tsig_verifier.verify(packet, &tsig_record) {
                Ok(key_name) => {
                    info!("TSIG verification successful for key: {}", key_name);
                    Some(key_name)
                }
                Err(e) => {
                    warn!("TSIG verification failed: {}", e);
                    return Err(UpdateError::NotVerified(e.to_string()));
                }
            }
        } else {
            debug!("No TSIG present in UPDATE message");
            None
        };

        // Check update policy
        if !self
            .update_policy
            .is_allowed(&zone_name, &authenticated_key, packet)
        {
            warn!("UPDATE denied by policy for zone: {}", zone_name);
            return Err(UpdateError::Refused(
                "Update not allowed by policy".to_string(),
            ));
        }

        // Parse update sections
        let prerequisites = self.parse_prerequisites(&packet.answers)?;
        let updates = self.parse_updates(&packet.authorities)?;

        // Get the zone for modification
        let mut zone = self
            .zone_store
            .get_zone_mut(&zone_name)
            .ok_or_else(|| UpdateError::ServerError("Zone not found".to_string()))?;

        // Check prerequisites
        for prereq in &prerequisites {
            if !self.check_prerequisite(&zone, prereq)? {
                info!("Prerequisite check failed: {:?}", prereq);
                return Err(UpdateError::PrereqFailed(
                    "Prerequisite not satisfied".to_string(),
                ));
            }
        }

        // Apply updates
        for update in &updates {
            self.apply_update(&mut zone, update)?;
        }

        // Update zone serial
        zone.update_serial();

        info!(
            "UPDATE successful for zone: {} (new serial: {})",
            zone_name, zone.serial
        );

        // Create success response
        Ok(self.create_update_response(packet, ResponseCode::NoError))
    }

    /// Extract TSIG record from additional section
    fn extract_tsig(&self, packet: &DNSPacket) -> Option<DNSResource> {
        packet
            .resources
            .iter()
            .find(|rr| rr.rtype == DNSResourceType::TSIG)
            .cloned()
    }

    /// Parse prerequisite records from answer section
    fn parse_prerequisites(
        &self,
        answers: &[DNSResource],
    ) -> Result<Vec<PrerequisiteCheck>, UpdateError> {
        let mut prerequisites = Vec::new();

        for rr in answers {
            let prereq = match (rr.rclass, rr.ttl, rr.rtype) {
                // RRset exists (value independent)
                (DNSResourceClass::ANY, 0, rtype) if rtype != DNSResourceType::ANY => {
                    PrerequisiteCheck::RRsetExists {
                        name: rr.labels.join("."),
                        rtype,
                    }
                }
                // RRset exists (value dependent)
                (DNSResourceClass::IN, 0, rtype) => PrerequisiteCheck::RRsetExistsValue {
                    name: rr.labels.join("."),
                    rtype,
                    rdata: rr.rdata.clone(),
                },
                // Name is in use
                (DNSResourceClass::ANY, 0, DNSResourceType::ANY) => {
                    PrerequisiteCheck::NameExists(rr.labels.join("."))
                }
                // RRset does not exist
                (DNSResourceClass::NONE, 0, rtype) if rtype != DNSResourceType::ANY => {
                    PrerequisiteCheck::RRsetNotExists {
                        name: rr.labels.join("."),
                        rtype,
                    }
                }
                // Name is not in use
                (DNSResourceClass::NONE, 0, DNSResourceType::ANY) => {
                    PrerequisiteCheck::NameNotExists(rr.labels.join("."))
                }
                _ => {
                    return Err(UpdateError::ServerError(
                        "Invalid prerequisite format".to_string(),
                    ));
                }
            };
            prerequisites.push(prereq);
        }

        Ok(prerequisites)
    }

    /// Parse update operations from authority section
    fn parse_updates(
        &self,
        authorities: &[DNSResource],
    ) -> Result<Vec<UpdateOperation>, UpdateError> {
        let mut updates = Vec::new();

        for rr in authorities {
            let update = match (rr.rclass, rr.rtype) {
                // Add to an RRset
                (DNSResourceClass::IN, rtype) => UpdateOperation::Add {
                    name: rr.labels.join("."),
                    ttl: rr.ttl,
                    rtype,
                    rdata: rr.rdata.clone(),
                },
                // Delete an RRset
                (DNSResourceClass::ANY, rtype) if rtype != DNSResourceType::ANY => {
                    UpdateOperation::DeleteRRset {
                        name: rr.labels.join("."),
                        rtype,
                    }
                }
                // Delete all RRsets at a name
                (DNSResourceClass::ANY, DNSResourceType::ANY) => {
                    UpdateOperation::DeleteName(rr.labels.join("."))
                }
                // Delete specific RR
                (DNSResourceClass::NONE, rtype) => UpdateOperation::DeleteRR {
                    name: rr.labels.join("."),
                    rtype,
                    rdata: rr.rdata.clone(),
                },
                _ => {
                    return Err(UpdateError::ServerError(
                        "Invalid update format".to_string(),
                    ));
                }
            };
            updates.push(update);
        }

        Ok(updates)
    }

    /// Check a prerequisite against the zone
    fn check_prerequisite(
        &self,
        zone: &Zone,
        prereq: &PrerequisiteCheck,
    ) -> Result<bool, UpdateError> {
        operations::check_prerequisite(zone, prereq)
    }

    /// Apply an update operation to the zone
    fn apply_update(&self, zone: &mut Zone, update: &UpdateOperation) -> Result<(), UpdateError> {
        operations::apply_update(zone, update)
    }

    /// Create UPDATE response packet
    fn create_update_response(&self, request: &DNSPacket, rcode: ResponseCode) -> DNSPacket {
        let mut response = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };

        // Copy header with response settings
        response.header.id = request.header.id;
        response.header.qr = true; // This is a response
        response.header.opcode = 5; // UPDATE opcode
        response.header.aa = true; // We're authoritative
        response.header.tc = false;
        response.header.rd = request.header.rd;
        response.header.ra = false;
        response.header.z = 0;
        response.header.rcode = rcode as u8;

        // Copy the zone (question) section
        if !request.questions.is_empty() {
            response.questions = vec![request.questions[0].clone()];
            response.header.qdcount = 1;
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_error_display() {
        let err = UpdateError::NotAuth("example.com".to_string());
        assert_eq!(err.to_string(), "Not authoritative: example.com");

        let err = UpdateError::Refused("Policy denied".to_string());
        assert_eq!(err.to_string(), "Update refused: Policy denied");
    }
}
