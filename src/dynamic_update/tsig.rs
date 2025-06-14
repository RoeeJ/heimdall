//! TSIG (Transaction Signature) authentication for DNS updates
//!
//! Implements RFC 2845 TSIG for authenticating dynamic DNS updates

use crate::dns::resource::DNSResource;
use crate::dns::{DNSPacket, enums::DNSResourceType};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ring::hmac;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// TSIG algorithm types
#[derive(Debug, Clone, PartialEq)]
pub enum TsigAlgorithm {
    HmacSha256,
    HmacSha384,
    HmacSha512,
}

impl TsigAlgorithm {
    /// Get the algorithm name as used in DNS
    pub fn name(&self) -> &'static str {
        match self {
            TsigAlgorithm::HmacSha256 => "hmac-sha256",
            TsigAlgorithm::HmacSha384 => "hmac-sha384",
            TsigAlgorithm::HmacSha512 => "hmac-sha512",
        }
    }

    /// Get the HMAC algorithm for ring
    fn hmac_algorithm(&self) -> &'static ring::hmac::Algorithm {
        match self {
            TsigAlgorithm::HmacSha256 => &ring::hmac::HMAC_SHA256,
            TsigAlgorithm::HmacSha384 => &ring::hmac::HMAC_SHA384,
            TsigAlgorithm::HmacSha512 => &ring::hmac::HMAC_SHA512,
        }
    }

    /// Parse algorithm from name
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "hmac-sha256" | "hmac-sha256." => Some(TsigAlgorithm::HmacSha256),
            "hmac-sha384" | "hmac-sha384." => Some(TsigAlgorithm::HmacSha384),
            "hmac-sha512" | "hmac-sha512." => Some(TsigAlgorithm::HmacSha512),
            _ => None,
        }
    }
}

/// TSIG key configuration
#[derive(Debug, Clone)]
pub struct TsigKey {
    /// Key name (e.g., "update-key.example.com")
    pub name: String,
    /// Algorithm to use
    pub algorithm: TsigAlgorithm,
    /// Shared secret (base64 encoded)
    pub secret: String,
}

impl TsigKey {
    /// Create a new TSIG key
    pub fn new(name: String, algorithm: TsigAlgorithm, secret: String) -> Self {
        Self {
            name: name.to_lowercase(),
            algorithm,
            secret,
        }
    }
}

/// TSIG verification result
pub type TsigResult<T> = Result<T, TsigError>;

/// TSIG-specific errors
#[derive(Debug, Clone)]
pub enum TsigError {
    /// Invalid TSIG format
    InvalidFormat(String),
    /// Unknown algorithm
    UnknownAlgorithm(String),
    /// Signature verification failed
    VerificationFailed,
    /// Time skew too large
    TimeSkew(i64),
    /// Key not found
    KeyNotFound(String),
    /// Base64 decode error
    DecodeError(String),
}

impl std::fmt::Display for TsigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsigError::InvalidFormat(msg) => write!(f, "Invalid TSIG format: {}", msg),
            TsigError::UnknownAlgorithm(alg) => write!(f, "Unknown TSIG algorithm: {}", alg),
            TsigError::VerificationFailed => write!(f, "TSIG signature verification failed"),
            TsigError::TimeSkew(skew) => write!(f, "TSIG time skew too large: {} seconds", skew),
            TsigError::KeyNotFound(name) => write!(f, "TSIG key not found: {}", name),
            TsigError::DecodeError(msg) => write!(f, "TSIG decode error: {}", msg),
        }
    }
}

impl std::error::Error for TsigError {}

/// TSIG verifier for authenticating DNS messages
pub struct TsigVerifier {
    /// Map of key names to keys
    keys: HashMap<String, TsigKey>,
    /// Maximum allowed time skew in seconds
    max_time_skew: i64,
}

impl TsigVerifier {
    /// Create a new TSIG verifier
    pub fn new(keys: Vec<TsigKey>) -> Self {
        let mut key_map = HashMap::new();
        for key in keys {
            key_map.insert(key.name.clone(), key);
        }

        Self {
            keys: key_map,
            max_time_skew: 300, // 5 minutes default
        }
    }

    /// Set maximum allowed time skew
    pub fn set_max_time_skew(&mut self, seconds: i64) {
        self.max_time_skew = seconds;
    }

    /// Verify TSIG on a DNS packet
    pub fn verify(&self, packet: &DNSPacket, tsig_rr: &DNSResource) -> TsigResult<String> {
        debug!("Verifying TSIG for packet id={}", packet.header.id);

        // Parse TSIG RDATA
        let tsig_data = self.parse_tsig_rdata(&tsig_rr.rdata)?;

        // Look up the key
        let key_name = tsig_rr.labels.join(".");
        let key = self
            .keys
            .get(&key_name)
            .ok_or_else(|| TsigError::KeyNotFound(key_name.clone()))?;

        // Verify algorithm matches
        if key.algorithm.name() != tsig_data.algorithm {
            return Err(TsigError::UnknownAlgorithm(tsig_data.algorithm.clone()));
        }

        // Check time
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let time_diff = (current_time as i64) - (tsig_data.time_signed as i64);
        if time_diff.abs() > self.max_time_skew {
            warn!("TSIG time skew too large: {} seconds", time_diff);
            return Err(TsigError::TimeSkew(time_diff));
        }

        // Compute MAC
        let computed_mac = self.compute_mac(packet, tsig_rr, &tsig_data, key)?;

        // Verify MAC
        if computed_mac != tsig_data.mac {
            warn!("TSIG MAC verification failed");
            return Err(TsigError::VerificationFailed);
        }

        debug!("TSIG verification successful for key: {}", key.name);
        Ok(key.name.clone())
    }

    /// Parse TSIG RDATA
    fn parse_tsig_rdata(&self, rdata: &[u8]) -> TsigResult<TsigData> {
        // TSIG RDATA format:
        // Algorithm Name (domain-name)
        // Time Signed (48-bit)
        // Fudge (16-bit)
        // MAC Size (16-bit)
        // MAC (variable)
        // Original ID (16-bit)
        // Error (16-bit)
        // Other Len (16-bit)
        // Other Data (variable)

        if rdata.len() < 10 {
            return Err(TsigError::InvalidFormat("RDATA too short".to_string()));
        }

        let mut offset = 0;

        // Parse algorithm name (simplified - assumes it ends with a zero byte)
        let algorithm_end = rdata[offset..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| TsigError::InvalidFormat("No algorithm terminator".to_string()))?;

        let algorithm = String::from_utf8_lossy(&rdata[offset..offset + algorithm_end]).to_string();
        offset += algorithm_end + 1;

        if rdata.len() < offset + 10 {
            return Err(TsigError::InvalidFormat(
                "RDATA too short after algorithm".to_string(),
            ));
        }

        // Time Signed (48-bit = 6 bytes)
        let time_signed = u64::from_be_bytes([
            0,
            0,
            rdata[offset],
            rdata[offset + 1],
            rdata[offset + 2],
            rdata[offset + 3],
            rdata[offset + 4],
            rdata[offset + 5],
        ]);
        offset += 6;

        // Fudge (16-bit)
        let fudge = u16::from_be_bytes([rdata[offset], rdata[offset + 1]]);
        offset += 2;

        // MAC Size (16-bit)
        let mac_size = u16::from_be_bytes([rdata[offset], rdata[offset + 1]]) as usize;
        offset += 2;

        if rdata.len() < offset + mac_size + 6 {
            return Err(TsigError::InvalidFormat(
                "RDATA too short for MAC".to_string(),
            ));
        }

        // MAC
        let mac = rdata[offset..offset + mac_size].to_vec();
        offset += mac_size;

        // Original ID (16-bit)
        let original_id = u16::from_be_bytes([rdata[offset], rdata[offset + 1]]);
        offset += 2;

        // Error (16-bit)
        let error = u16::from_be_bytes([rdata[offset], rdata[offset + 1]]);

        Ok(TsigData {
            algorithm,
            time_signed,
            fudge,
            mac,
            original_id,
            error,
        })
    }

    /// Compute MAC for verification
    fn compute_mac(
        &self,
        packet: &DNSPacket,
        tsig_rr: &DNSResource,
        tsig_data: &TsigData,
        key: &TsigKey,
    ) -> TsigResult<Vec<u8>> {
        // Decode the secret
        let secret = BASE64
            .decode(&key.secret)
            .map_err(|e| TsigError::DecodeError(e.to_string()))?;

        // Create HMAC key
        let hmac_key = hmac::Key::new(*key.algorithm.hmac_algorithm(), &secret);

        // Build data to sign
        let mut data = Vec::new();

        // 1. DNS Message (without TSIG record)
        let mut packet_copy = packet.clone();
        // Remove TSIG from resources
        packet_copy
            .resources
            .retain(|rr| rr.rtype != DNSResourceType::TSIG);
        packet_copy.header.arcount = packet_copy.resources.len() as u16;

        // Serialize the packet
        let mut packet_bytes = Vec::new();
        packet_copy
            .serialize_into(&mut packet_bytes)
            .map_err(|e| TsigError::InvalidFormat(format!("Failed to serialize packet: {}", e)))?;
        data.extend_from_slice(&packet_bytes);

        // 2. TSIG variables (RFC 2845 section 3.4.2)
        // - Key name
        let key_name = tsig_rr.labels.join(".");
        data.extend_from_slice(key_name.as_bytes());
        data.push(0); // Null terminator

        // - Class (ANY = 255)
        data.extend_from_slice(&255u16.to_be_bytes());

        // - TTL (0)
        data.extend_from_slice(&0u32.to_be_bytes());

        // - Algorithm name
        data.extend_from_slice(tsig_data.algorithm.as_bytes());
        data.push(0); // Null terminator

        // - Time signed
        data.extend_from_slice(&tsig_data.time_signed.to_be_bytes()[2..]); // 48-bit

        // - Fudge
        data.extend_from_slice(&tsig_data.fudge.to_be_bytes());

        // - Error
        data.extend_from_slice(&tsig_data.error.to_be_bytes());

        // - Other len (0)
        data.extend_from_slice(&0u16.to_be_bytes());

        // Compute HMAC
        let signature = hmac::sign(&hmac_key, &data);
        Ok(signature.as_ref().to_vec())
    }

    /// Sign a DNS packet with TSIG
    pub fn sign(&self, packet: &mut DNSPacket, key_name: &str) -> TsigResult<()> {
        let key = self
            .keys
            .get(key_name)
            .ok_or_else(|| TsigError::KeyNotFound(key_name.to_string()))?;

        // Generate TSIG
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let tsig_data = TsigData {
            algorithm: key.algorithm.name().to_string(),
            time_signed: current_time,
            fudge: 300,      // 5 minutes
            mac: Vec::new(), // Will be computed
            original_id: packet.header.id,
            error: 0,
        };

        // Compute MAC
        let mac = self.compute_mac_for_signing(packet, key, &tsig_data)?;

        // Create TSIG resource record
        let key_labels: Vec<String> = key.name.split('.').map(|s| s.to_string()).collect();
        let rdata = self.build_tsig_rdata(&tsig_data, &mac)?;
        let tsig_rr = DNSResource {
            labels: key_labels,
            rtype: DNSResourceType::TSIG,
            rclass: crate::dns::enums::DNSResourceClass::ANY,
            ttl: 0,
            rdlength: rdata.len() as u16,
            rdata,
            parsed_rdata: None,
            raw_class: None,
        };

        // Add to packet
        packet.resources.push(tsig_rr);
        packet.header.arcount += 1;

        Ok(())
    }

    /// Compute MAC for signing
    fn compute_mac_for_signing(
        &self,
        packet: &DNSPacket,
        key: &TsigKey,
        tsig_data: &TsigData,
    ) -> TsigResult<Vec<u8>> {
        // Similar to compute_mac but for signing
        let secret = BASE64
            .decode(&key.secret)
            .map_err(|e| TsigError::DecodeError(e.to_string()))?;

        let hmac_key = hmac::Key::new(*key.algorithm.hmac_algorithm(), &secret);

        let mut data = Vec::new();

        // Serialize the packet
        let mut packet_bytes = Vec::new();
        packet
            .serialize_into(&mut packet_bytes)
            .map_err(|e| TsigError::InvalidFormat(format!("Failed to serialize packet: {}", e)))?;
        data.extend_from_slice(&packet_bytes);

        // Add TSIG variables
        data.extend_from_slice(key.name.as_bytes());
        data.push(0);
        data.extend_from_slice(&255u16.to_be_bytes()); // Class ANY
        data.extend_from_slice(&0u32.to_be_bytes()); // TTL
        data.extend_from_slice(tsig_data.algorithm.as_bytes());
        data.push(0);
        data.extend_from_slice(&tsig_data.time_signed.to_be_bytes()[2..]);
        data.extend_from_slice(&tsig_data.fudge.to_be_bytes());
        data.extend_from_slice(&tsig_data.error.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes()); // Other len

        let signature = hmac::sign(&hmac_key, &data);
        Ok(signature.as_ref().to_vec())
    }

    /// Build TSIG RDATA
    fn build_tsig_rdata(&self, tsig_data: &TsigData, mac: &[u8]) -> TsigResult<Vec<u8>> {
        let mut rdata = Vec::new();

        // Algorithm name
        rdata.extend_from_slice(tsig_data.algorithm.as_bytes());
        rdata.push(0);

        // Time signed (48-bit)
        rdata.extend_from_slice(&tsig_data.time_signed.to_be_bytes()[2..]);

        // Fudge
        rdata.extend_from_slice(&tsig_data.fudge.to_be_bytes());

        // MAC size
        rdata.extend_from_slice(&(mac.len() as u16).to_be_bytes());

        // MAC
        rdata.extend_from_slice(mac);

        // Original ID
        rdata.extend_from_slice(&tsig_data.original_id.to_be_bytes());

        // Error
        rdata.extend_from_slice(&tsig_data.error.to_be_bytes());

        // Other len
        rdata.extend_from_slice(&0u16.to_be_bytes());

        Ok(rdata)
    }
}

/// Parsed TSIG data
#[derive(Debug)]
struct TsigData {
    algorithm: String,
    time_signed: u64,
    fudge: u16,
    mac: Vec<u8>,
    original_id: u16,
    error: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsig_algorithm_names() {
        assert_eq!(TsigAlgorithm::HmacSha256.name(), "hmac-sha256");
        assert_eq!(TsigAlgorithm::HmacSha384.name(), "hmac-sha384");
        assert_eq!(TsigAlgorithm::HmacSha512.name(), "hmac-sha512");
    }

    #[test]
    fn test_tsig_algorithm_from_name() {
        assert_eq!(
            TsigAlgorithm::from_name("hmac-sha256"),
            Some(TsigAlgorithm::HmacSha256)
        );
        assert_eq!(
            TsigAlgorithm::from_name("HMAC-SHA256"),
            Some(TsigAlgorithm::HmacSha256)
        );
        assert_eq!(TsigAlgorithm::from_name("unknown"), None);
    }

    #[test]
    fn test_tsig_key_creation() {
        let key = TsigKey::new(
            "update-key.example.com".to_string(),
            TsigAlgorithm::HmacSha256,
            "abcdefghijklmnop".to_string(),
        );

        assert_eq!(key.name, "update-key.example.com");
        assert_eq!(key.algorithm, TsigAlgorithm::HmacSha256);
        assert_eq!(key.secret, "abcdefghijklmnop");
    }
}
