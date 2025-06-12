use crate::dns::DNSPacket;
use crate::dns::resource::DNSResource;
use crate::dns::enums::{DNSResourceType, ResponseCode};
use super::{DnsSecError, errors::Result};
use tracing::{debug, trace};

/// NSEC/NSEC3 denial of existence validator
pub struct DenialOfExistenceValidator;

impl DenialOfExistenceValidator {
    /// Create a new denial of existence validator
    pub fn new() -> Self {
        Self
    }
    
    /// Validate denial of existence for a query
    pub fn validate_denial(
        &self,
        packet: &DNSPacket,
        qname: &str,
        qtype: DNSResourceType,
    ) -> Result<()> {
        // Check if this is a negative response
        if packet.header.rcode != ResponseCode::NameError.to_u8() && 
           packet.header.ancount > 0 {
            // Not a negative response, no denial validation needed
            return Ok(());
        }
        
        debug!("Validating denial of existence for {} {:?}", qname, qtype);
        
        // Look for NSEC3 records first (more common in modern DNS)
        let nsec3_records: Vec<&DNSResource> = packet.authorities.iter()
            .filter(|rr| rr.rtype == DNSResourceType::NSEC3)
            .collect();
            
        if !nsec3_records.is_empty() {
            return self.validate_nsec3_denial(&nsec3_records, qname, qtype);
        }
        
        // Look for NSEC records
        let nsec_records: Vec<&DNSResource> = packet.authorities.iter()
            .filter(|rr| rr.rtype == DNSResourceType::NSEC)
            .collect();
            
        if !nsec_records.is_empty() {
            return self.validate_nsec_denial(&nsec_records, qname, qtype);
        }
        
        // No NSEC/NSEC3 records found
        Err(DnsSecError::DenialOfExistenceFailed)
    }
    
    /// Validate NSEC denial
    fn validate_nsec_denial(
        &self,
        nsec_records: &[&DNSResource],
        qname: &str,
        qtype: DNSResourceType,
    ) -> Result<()> {
        for nsec in nsec_records {
            // Parse NSEC data
            if let Some(parsed) = &nsec.parsed_rdata {
                // NSEC format: next_domain types...
                let parts: Vec<&str> = parsed.split(' ').collect();
                if parts.is_empty() {
                    continue;
                }
                
                let owner = nsec.labels.join(".");
                let next_domain = parts[0];
                
                trace!("NSEC record: {} -> {}", owner, next_domain);
                
                // Check if qname falls in the gap between owner and next
                if self.name_in_range(&owner, next_domain, qname) {
                    // Name is in range, check if it's a type denial
                    if owner == qname {
                        // Same name, check if the type is denied
                        let denied_types = self.parse_nsec_types(&parts[1..]);
                        if !denied_types.contains(&qtype) {
                            debug!("NSEC proves non-existence of type {:?} at {}", qtype, qname);
                            return Ok(());
                        }
                    } else {
                        // Name doesn't exist
                        debug!("NSEC proves non-existence of name {}", qname);
                        return Ok(());
                    }
                }
            }
        }
        
        Err(DnsSecError::DenialOfExistenceFailed)
    }
    
    /// Validate NSEC3 denial
    fn validate_nsec3_denial(
        &self,
        nsec3_records: &[&DNSResource],
        qname: &str,
        qtype: DNSResourceType,
    ) -> Result<()> {
        // For NSEC3, we need to:
        // 1. Hash the query name using the NSEC3 parameters
        // 2. Find the NSEC3 record that covers this hash
        // 3. Verify the denial
        
        for nsec3 in nsec3_records {
            if let Some(parsed) = &nsec3.parsed_rdata {
                // NSEC3 format: algorithm flags iterations salt next_hash types...
                let parts: Vec<&str> = parsed.split(' ').collect();
                if parts.len() < 5 {
                    continue;
                }
                
                let algorithm = parts[0].parse::<u8>().unwrap_or(0);
                let _flags = parts[1].parse::<u8>().unwrap_or(0);
                let iterations = parts[2].parse::<u16>().unwrap_or(0);
                let salt = parts[3];
                let next_hash = parts[4];
                
                // Only SHA-1 (algorithm 1) is currently defined for NSEC3
                if algorithm != 1 {
                    continue;
                }
                
                // Hash the query name
                let qname_hash = self.compute_nsec3_hash(qname, salt, iterations)?;
                let owner_hash = nsec3.labels.join(".");
                
                trace!("NSEC3: owner_hash={}, next_hash={}, qname_hash={}", 
                    owner_hash, next_hash, qname_hash);
                
                // Check if the hash falls in the range
                if self.hash_in_range(&owner_hash, next_hash, &qname_hash) {
                    if owner_hash == qname_hash {
                        // Same hash, check type denial
                        let denied_types = if parts.len() > 5 {
                            self.parse_nsec_types(&parts[5..])
                        } else {
                            Vec::new()
                        };
                        
                        if !denied_types.contains(&qtype) {
                            debug!("NSEC3 proves non-existence of type {:?}", qtype);
                            return Ok(());
                        }
                    } else {
                        // Hash doesn't exist
                        debug!("NSEC3 proves non-existence of name");
                        return Ok(());
                    }
                }
            }
        }
        
        Err(DnsSecError::DenialOfExistenceFailed)
    }
    
    /// Check if a name falls in the range between two domain names
    fn name_in_range(&self, owner: &str, next: &str, name: &str) -> bool {
        // Canonical ordering for DNS names
        let owner_lower = owner.to_lowercase();
        let next_lower = next.to_lowercase();
        let name_lower = name.to_lowercase();
        
        // Handle wrap-around (when next < owner)
        if next_lower < owner_lower {
            // Wraps around: name should be >= owner OR <= next
            name_lower >= owner_lower || name_lower <= next_lower
        } else {
            // Normal case: owner <= name <= next
            name_lower >= owner_lower && name_lower <= next_lower
        }
    }
    
    /// Check if a hash falls in the range between two hashes
    fn hash_in_range(&self, owner: &str, next: &str, hash: &str) -> bool {
        // For base32 encoded hashes, we can compare them as strings
        let owner_lower = owner.to_lowercase();
        let next_lower = next.to_lowercase();
        let hash_lower = hash.to_lowercase();
        
        // Handle wrap-around
        if next_lower < owner_lower {
            hash_lower >= owner_lower || hash_lower <= next_lower
        } else {
            hash_lower >= owner_lower && hash_lower <= next_lower
        }
    }
    
    /// Parse NSEC type bitmap
    fn parse_nsec_types(&self, type_parts: &[&str]) -> Vec<DNSResourceType> {
        let mut types = Vec::new();
        
        for part in type_parts {
            if let Ok(type_num) = part.parse::<u16>() {
                if let Some(rtype) = DNSResourceType::from_u16(type_num) {
                    types.push(rtype);
                }
            }
        }
        
        types
    }
    
    /// Compute NSEC3 hash
    fn compute_nsec3_hash(&self, name: &str, salt: &str, iterations: u16) -> Result<String> {
        use ring::digest;
        
        // Decode salt (or use empty if "-")
        let salt_bytes = if salt == "-" {
            Vec::new()
        } else {
            hex::decode(salt).map_err(|_| DnsSecError::InvalidNsec3Parameters)?
        };
        
        // Convert name to wire format (lowercase labels)
        let mut wire_name = Vec::new();
        for label in name.split('.') {
            if !label.is_empty() {
                wire_name.push(label.len() as u8);
                wire_name.extend_from_slice(label.to_lowercase().as_bytes());
            }
        }
        wire_name.push(0); // Root label
        
        // Initial hash: H(name || salt)
        let mut hash_input = wire_name.clone();
        hash_input.extend_from_slice(&salt_bytes);
        let mut hash = digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, &hash_input);
        
        // Iterate: H(H(...) || salt)
        for _ in 0..iterations {
            let mut next_input = hash.as_ref().to_vec();
            next_input.extend_from_slice(&salt_bytes);
            hash = digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, &next_input);
        }
        
        // Encode as base32 (DNS uses a specific base32 variant)
        let encoded = base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            hash.as_ref()
        ).to_lowercase();
        
        Ok(encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_name_in_range() {
        let validator = DenialOfExistenceValidator::new();
        
        // Normal range
        assert!(validator.name_in_range("a.example.com", "c.example.com", "b.example.com"));
        assert!(!validator.name_in_range("a.example.com", "c.example.com", "d.example.com"));
        
        // Wrap-around range
        assert!(validator.name_in_range("x.example.com", "b.example.com", "a.example.com"));
        assert!(validator.name_in_range("x.example.com", "b.example.com", "z.example.com"));
    }
    
    #[test]
    fn test_nsec3_hash_computation() {
        let validator = DenialOfExistenceValidator::new();
        
        // Test with no salt and 0 iterations
        let hash = validator.compute_nsec3_hash("example.com", "-", 0).unwrap();
        assert!(!hash.is_empty());
        
        // Test with salt
        let hash_with_salt = validator.compute_nsec3_hash("example.com", "aabbccdd", 1).unwrap();
        assert!(!hash_with_salt.is_empty());
        assert_ne!(hash, hash_with_salt);
    }
}