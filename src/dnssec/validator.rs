use ring::signature;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, trace, warn};

use super::{
    DenialOfExistenceValidator, DigestType, DnsSecAlgorithm, DnsSecError, TrustAnchorStore,
    ValidationResult, calculate_key_tag, errors::Result,
};
use crate::dns::DNSPacket;
use crate::dns::enums::{DNSResourceClass, DNSResourceType};
use crate::dns::resource::DNSResource;

/// DNSSEC validator for validating DNS responses
pub struct DnsSecValidator {
    /// Trust anchor store
    trust_anchors: Arc<TrustAnchorStore>,
    /// Current time for signature validation (for testing)
    current_time: Option<u32>,
}

impl DnsSecValidator {
    /// Create a new DNSSEC validator
    pub fn new(trust_anchors: Arc<TrustAnchorStore>) -> Self {
        Self {
            trust_anchors,
            current_time: None,
        }
    }

    /// Set current time for testing
    pub fn set_current_time(&mut self, time: u32) {
        self.current_time = Some(time);
    }

    /// Get current time as Unix timestamp
    fn get_current_time(&self) -> u32 {
        self.current_time.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32
        })
    }

    /// Validate a DNS response
    pub async fn validate(&self, packet: &DNSPacket) -> ValidationResult {
        debug!(
            "Starting DNSSEC validation for packet ID {}",
            packet.header.id
        );

        // Check if response has DNSSEC records
        let has_rrsig = packet
            .answers
            .iter()
            .any(|rr| rr.rtype == DNSResourceType::RRSIG)
            || packet
                .authorities
                .iter()
                .any(|rr| rr.rtype == DNSResourceType::RRSIG);

        if !has_rrsig {
            debug!("No RRSIG records found, response is insecure");
            return ValidationResult::Insecure;
        }

        // Validate each RRset in the response
        match self.validate_rrsets(packet).await {
            Ok(()) => {
                debug!("DNSSEC validation successful");
                ValidationResult::Secure
            }
            Err(e) => {
                warn!("DNSSEC validation failed: {}", e);
                ValidationResult::Bogus(e.to_string())
            }
        }
    }

    /// Validate a DNS response with denial of existence
    pub async fn validate_with_denial(
        &self,
        packet: &DNSPacket,
        qname: &str,
        qtype: DNSResourceType,
    ) -> ValidationResult {
        debug!(
            "Starting DNSSEC validation with denial check for {} {:?}",
            qname, qtype
        );

        // First try regular validation
        let result = self.validate(packet).await;

        // If the response is negative (NXDOMAIN or no answers), validate denial
        if packet.header.ancount == 0 || packet.header.rcode == 3 {
            // Create denial validator
            let denial_validator = DenialOfExistenceValidator::new();

            match denial_validator.validate_denial(packet, qname, qtype) {
                Ok(()) => {
                    debug!("Denial of existence validated");
                    ValidationResult::Secure
                }
                Err(e) => {
                    warn!("Denial validation failed: {}", e);
                    ValidationResult::Bogus(e.to_string())
                }
            }
        } else {
            result
        }
    }

    /// Validate all RRsets in a packet
    async fn validate_rrsets(&self, packet: &DNSPacket) -> Result<()> {
        // Group records by name, type, and class
        let mut rrsets: HashMap<(String, DNSResourceType, DNSResourceClass), Vec<&DNSResource>> =
            HashMap::new();

        // Process all sections
        for record in packet
            .answers
            .iter()
            .chain(packet.authorities.iter())
            .chain(packet.resources.iter())
        {
            if record.rtype != DNSResourceType::RRSIG {
                let name = record.labels.join(".");
                let key = (name, record.rtype, record.rclass);
                rrsets.entry(key).or_default().push(record);
            }
        }

        // Validate each RRset
        for ((name, rtype, rclass), records) in rrsets {
            self.validate_rrset(&name, rtype, rclass, &records, packet)
                .await?;
        }

        Ok(())
    }

    /// Validate a single RRset
    async fn validate_rrset(
        &self,
        name: &str,
        rtype: DNSResourceType,
        rclass: DNSResourceClass,
        records: &[&DNSResource],
        packet: &DNSPacket,
    ) -> Result<()> {
        trace!("Validating RRset: {} {:?} {:?}", name, rtype, rclass);

        // Find RRSIG for this RRset
        let rrsig = self.find_rrsig_for_rrset(name, rtype, packet)?;

        // Parse RRSIG data
        let rrsig_data = self.parse_rrsig(&rrsig)?;

        // Check signature validity period
        self.check_signature_validity(&rrsig_data)?;

        // Find the DNSKEY that can validate this signature
        let dnskey = self.find_validating_dnskey(&rrsig_data, packet).await?;

        // Verify the signature
        self.verify_signature(&rrsig_data, &dnskey, records)?;

        Ok(())
    }

    /// Find RRSIG record for an RRset
    fn find_rrsig_for_rrset(
        &self,
        name: &str,
        rtype: DNSResourceType,
        packet: &DNSPacket,
    ) -> Result<DNSResource> {
        for record in packet
            .answers
            .iter()
            .chain(packet.authorities.iter())
            .chain(packet.resources.iter())
        {
            let record_name = record.labels.join(".");
            if record.rtype == DNSResourceType::RRSIG && record_name == name {
                // Parse type covered from RRSIG
                if record.rdata.len() >= 2 {
                    let type_covered = u16::from_be_bytes([record.rdata[0], record.rdata[1]]);
                    if let Some(covered_type) = DNSResourceType::from_u16(type_covered) {
                        if covered_type == rtype {
                            return Ok(record.clone());
                        }
                    }
                }
            }
        }

        Err(DnsSecError::NoRrsig)
    }

    /// Parse RRSIG record data
    fn parse_rrsig(&self, rrsig: &DNSResource) -> Result<RrsigData> {
        if rrsig.rdata.len() < 18 {
            return Err(DnsSecError::InvalidSignature);
        }

        let type_covered = u16::from_be_bytes([rrsig.rdata[0], rrsig.rdata[1]]);
        let algorithm = rrsig.rdata[2];
        let labels = rrsig.rdata[3];
        let original_ttl = u32::from_be_bytes([
            rrsig.rdata[4],
            rrsig.rdata[5],
            rrsig.rdata[6],
            rrsig.rdata[7],
        ]);
        let sig_expiration = u32::from_be_bytes([
            rrsig.rdata[8],
            rrsig.rdata[9],
            rrsig.rdata[10],
            rrsig.rdata[11],
        ]);
        let sig_inception = u32::from_be_bytes([
            rrsig.rdata[12],
            rrsig.rdata[13],
            rrsig.rdata[14],
            rrsig.rdata[15],
        ]);
        let key_tag = u16::from_be_bytes([rrsig.rdata[16], rrsig.rdata[17]]);

        // Parse signer's name and signature
        let (signer_name, signature_start) = self.parse_domain_name(&rrsig.rdata[18..])?;
        let signature = rrsig.rdata[18 + signature_start..].to_vec();

        Ok(RrsigData {
            type_covered,
            algorithm,
            labels,
            original_ttl,
            sig_expiration,
            sig_inception,
            key_tag,
            signer_name,
            signature,
        })
    }

    /// Parse a domain name from wire format
    fn parse_domain_name(&self, data: &[u8]) -> Result<(String, usize)> {
        let mut labels = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }

            if len > 63 {
                // Compression pointer - not handled in RRSIG signer field
                return Err(DnsSecError::InvalidSignature);
            }

            pos += 1;
            if pos + len > data.len() {
                return Err(DnsSecError::InvalidSignature);
            }

            labels.push(String::from_utf8_lossy(&data[pos..pos + len]).to_string());
            pos += len;
        }

        let name = if labels.is_empty() {
            ".".to_string()
        } else {
            labels.join(".")
        };

        Ok((name, pos))
    }

    /// Check signature validity period
    fn check_signature_validity(&self, rrsig: &RrsigData) -> Result<()> {
        let current_time = self.get_current_time();

        if current_time < rrsig.sig_inception {
            return Err(DnsSecError::SignatureNotYetValid);
        }

        if current_time > rrsig.sig_expiration {
            return Err(DnsSecError::SignatureExpired);
        }

        Ok(())
    }

    /// Find DNSKEY that can validate this signature
    async fn find_validating_dnskey(
        &self,
        rrsig: &RrsigData,
        packet: &DNSPacket,
    ) -> Result<DnskeyData> {
        // First try to find DNSKEY in the packet
        for record in packet
            .answers
            .iter()
            .chain(packet.authorities.iter())
            .chain(packet.resources.iter())
        {
            let record_name = record.labels.join(".");
            if record.rtype == DNSResourceType::DNSKEY && record_name == rrsig.signer_name {
                let dnskey = self.parse_dnskey(record)?;
                if dnskey.key_tag == rrsig.key_tag && dnskey.algorithm == rrsig.algorithm {
                    // Validate this DNSKEY against trust anchors or DS records
                    self.validate_dnskey(&dnskey, &rrsig.signer_name, packet)
                        .await?;
                    return Ok(dnskey);
                }
            }
        }

        Err(DnsSecError::NoDnsKey)
    }

    /// Parse DNSKEY record data
    fn parse_dnskey(&self, dnskey: &DNSResource) -> Result<DnskeyData> {
        if dnskey.rdata.len() < 4 {
            return Err(DnsSecError::InvalidPublicKey);
        }

        let flags = u16::from_be_bytes([dnskey.rdata[0], dnskey.rdata[1]]);
        let protocol = dnskey.rdata[2];
        let algorithm = dnskey.rdata[3];
        let public_key = dnskey.rdata[4..].to_vec();

        let key_tag = calculate_key_tag(flags, protocol, algorithm, &public_key);

        Ok(DnskeyData {
            flags,
            protocol,
            algorithm,
            public_key,
            key_tag,
        })
    }

    /// Validate DNSKEY against trust anchors or DS records
    async fn validate_dnskey(
        &self,
        dnskey: &DnskeyData,
        domain: &str,
        packet: &DNSPacket,
    ) -> Result<()> {
        // Check if this key is a trust anchor
        if let Some(anchor) = self.trust_anchors.find_by_key_tag(domain, dnskey.key_tag) {
            if anchor.algorithm.to_u8() == dnskey.algorithm
                && anchor.public_key == dnskey.public_key
            {
                debug!("DNSKEY validated against trust anchor");
                return Ok(());
            }
        }

        // Otherwise, validate against DS records
        self.validate_dnskey_with_ds(dnskey, domain, packet).await
    }

    /// Validate DNSKEY using DS records
    async fn validate_dnskey_with_ds(
        &self,
        dnskey: &DnskeyData,
        domain: &str,
        packet: &DNSPacket,
    ) -> Result<()> {
        // Find DS records for this domain
        for record in packet.authorities.iter().chain(packet.resources.iter()) {
            let record_name = record.labels.join(".");
            if record.rtype == DNSResourceType::DS && record_name == domain {
                let ds_data = self.parse_ds(record)?;

                // Check if this DS matches our DNSKEY
                if ds_data.key_tag == dnskey.key_tag && ds_data.algorithm == dnskey.algorithm {
                    // Compute digest of DNSKEY and compare
                    let digest = self.compute_dnskey_digest(domain, dnskey, ds_data.digest_type)?;
                    if digest == ds_data.digest {
                        debug!("DNSKEY validated against DS record");
                        return Ok(());
                    }
                }
            }
        }

        Err(DnsSecError::NoDs)
    }

    /// Parse DS record data
    fn parse_ds(&self, ds: &DNSResource) -> Result<DsData> {
        if ds.rdata.len() < 4 {
            return Err(DnsSecError::ValidationError(
                "Invalid DS record".to_string(),
            ));
        }

        let key_tag = u16::from_be_bytes([ds.rdata[0], ds.rdata[1]]);
        let algorithm = ds.rdata[2];
        let digest_type = ds.rdata[3];
        let digest = ds.rdata[4..].to_vec();

        Ok(DsData {
            key_tag,
            algorithm,
            digest_type,
            digest,
        })
    }

    /// Compute digest of DNSKEY for DS validation
    fn compute_dnskey_digest(
        &self,
        domain: &str,
        dnskey: &DnskeyData,
        digest_type: u8,
    ) -> Result<Vec<u8>> {
        let digest_type = DigestType::from_u8(digest_type)
            .ok_or(DnsSecError::UnsupportedDigestType(digest_type))?;

        // Build the data to hash: owner name + DNSKEY RDATA
        let mut data = Vec::new();

        // Add owner name in wire format
        for label in domain.split('.') {
            if !label.is_empty() {
                data.push(label.len() as u8);
                data.extend_from_slice(label.as_bytes());
            }
        }
        data.push(0); // Root label

        // Add DNSKEY RDATA
        data.extend_from_slice(&dnskey.flags.to_be_bytes());
        data.push(dnskey.protocol);
        data.push(dnskey.algorithm);
        data.extend_from_slice(&dnskey.public_key);

        digest_type
            .digest(&data)
            .ok_or(DnsSecError::UnsupportedDigestType(digest_type.to_u8()))
    }

    /// Verify RRSIG signature
    fn verify_signature(
        &self,
        rrsig: &RrsigData,
        dnskey: &DnskeyData,
        records: &[&DNSResource],
    ) -> Result<()> {
        let algorithm = DnsSecAlgorithm::from_u8(rrsig.algorithm)
            .ok_or(DnsSecError::UnsupportedAlgorithm(rrsig.algorithm))?;

        if !algorithm.is_supported() {
            return Err(DnsSecError::UnsupportedAlgorithm(rrsig.algorithm));
        }

        // Build the data to verify
        let signed_data = self.build_signed_data(rrsig, records)?;

        // Get the verification algorithm
        let verify_alg = algorithm
            .ring_algorithm()
            .ok_or(DnsSecError::UnsupportedAlgorithm(rrsig.algorithm))?;

        // Verify the signature
        let public_key = signature::UnparsedPublicKey::new(verify_alg, &dnskey.public_key);

        public_key
            .verify(&signed_data, &rrsig.signature)
            .map_err(|_| DnsSecError::SignatureVerificationFailed)?;

        debug!("Signature verified successfully");
        Ok(())
    }

    /// Build the signed data for signature verification
    fn build_signed_data(&self, rrsig: &RrsigData, records: &[&DNSResource]) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Add RRSIG RDATA (minus the signature itself)
        data.extend_from_slice(&rrsig.type_covered.to_be_bytes());
        data.push(rrsig.algorithm);
        data.push(rrsig.labels);
        data.extend_from_slice(&rrsig.original_ttl.to_be_bytes());
        data.extend_from_slice(&rrsig.sig_expiration.to_be_bytes());
        data.extend_from_slice(&rrsig.sig_inception.to_be_bytes());
        data.extend_from_slice(&rrsig.key_tag.to_be_bytes());

        // Add signer's name in wire format
        for label in rrsig.signer_name.split('.') {
            if !label.is_empty() {
                data.push(label.len() as u8);
                data.extend_from_slice(label.to_lowercase().as_bytes());
            }
        }
        data.push(0); // Root label

        // Sort records by canonical order
        let mut sorted_records = records.to_vec();
        sorted_records.sort_by(|a, b| a.rdata.cmp(&b.rdata));

        // Add each record in canonical form
        for record in sorted_records {
            // Owner name in wire format (lowercase)
            for label in &record.labels {
                if !label.is_empty() {
                    data.push(label.len() as u8);
                    data.extend_from_slice(label.to_lowercase().as_bytes());
                }
            }
            data.push(0); // Root label

            // Type, class, TTL
            let rtype_u16: u16 = record.rtype.into();
            let rclass_u16: u16 = record.rclass.into();
            data.extend_from_slice(&rtype_u16.to_be_bytes());
            data.extend_from_slice(&rclass_u16.to_be_bytes());
            data.extend_from_slice(&rrsig.original_ttl.to_be_bytes());

            // RDATA length and data
            data.extend_from_slice(&(record.rdata.len() as u16).to_be_bytes());
            data.extend_from_slice(&record.rdata);
        }

        Ok(data)
    }
}

/// Parsed RRSIG data
#[derive(Debug)]
struct RrsigData {
    type_covered: u16,
    algorithm: u8,
    labels: u8,
    original_ttl: u32,
    sig_expiration: u32,
    sig_inception: u32,
    key_tag: u16,
    signer_name: String,
    signature: Vec<u8>,
}

/// Parsed DNSKEY data
#[derive(Debug)]
struct DnskeyData {
    flags: u16,
    protocol: u8,
    algorithm: u8,
    public_key: Vec<u8>,
    key_tag: u16,
}

/// Parsed DS data
#[derive(Debug)]
struct DsData {
    key_tag: u16,
    algorithm: u8,
    digest_type: u8,
    digest: Vec<u8>,
}
