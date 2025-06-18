//! Zone transfer implementation (AXFR/IXFR)
//!
//! This module implements DNS zone transfer protocols as defined in RFC 5936 (AXFR)
//! and RFC 1995 (IXFR).

use super::{Zone, ZoneStore};
use crate::dns::{
    DNSPacket,
    enums::{DNSResourceType, ResponseCode},
    header::DNSHeader,
};
use crate::error::{DnsError, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Zone transfer handler
pub struct ZoneTransferHandler {
    zone_store: Arc<ZoneStore>,
    allowed_transfers: Vec<String>, // IP addresses or subnets allowed to transfer
}

impl ZoneTransferHandler {
    /// Create a new zone transfer handler
    pub fn new(zone_store: Arc<ZoneStore>, allowed_transfers: Vec<String>) -> Self {
        Self {
            zone_store,
            allowed_transfers,
        }
    }

    /// Check if a client is allowed to perform zone transfers
    pub fn is_transfer_allowed(&self, client_addr: &SocketAddr) -> bool {
        if self.allowed_transfers.is_empty() {
            // No restrictions if list is empty
            return true;
        }

        let client_ip = client_addr.ip().to_string();

        // Simple IP matching for now - could be extended to support subnets
        self.allowed_transfers
            .iter()
            .any(|allowed| allowed == &client_ip || allowed == "*")
    }

    /// Handle an AXFR (full zone transfer) request
    pub fn handle_axfr(
        &self,
        query: &DNSPacket,
        client_addr: &SocketAddr,
    ) -> Result<Vec<DNSPacket>> {
        // Check if client is allowed to transfer
        if !self.is_transfer_allowed(client_addr) {
            warn!("Zone transfer denied for client {}", client_addr);
            return Ok(vec![self.create_refused_response(query)]);
        }

        // Validate query
        if query.questions.is_empty() {
            return Ok(vec![self.create_formerr_response(query)]);
        }

        let question = &query.questions[0];
        if question.qtype != DNSResourceType::AXFR {
            return Ok(vec![self.create_formerr_response(query)]);
        }

        let zone_name = question.labels.join(".").to_lowercase();

        // Get the zone
        let zone = match self.zone_store.get_zone(&zone_name) {
            Some(zone) => zone,
            None => {
                info!("AXFR request for non-existent zone: {}", zone_name);
                return Ok(vec![self.create_notauth_response(query)]);
            }
        };

        info!(
            "Processing AXFR request for zone {} from {}",
            zone_name, client_addr
        );

        // Build AXFR response packets
        self.build_axfr_response(query, &zone)
    }

    /// Build AXFR response packets
    fn build_axfr_response(&self, query: &DNSPacket, zone: &Zone) -> Result<Vec<DNSPacket>> {
        let mut packets = Vec::new();
        let mut current_packet = self.create_base_response(query);
        let mut current_size = 512; // Conservative estimate

        // First record must be SOA
        if let Some(soa) = zone.get_soa() {
            let soa_resource = soa
                .to_dns_resource(&zone.origin, zone.default_ttl)
                .map_err(DnsError::ParseError)?;

            current_packet.answers.push(soa_resource.clone());
            current_packet.header.ancount = 1;

            // Add all other records
            for record in zone.records() {
                // Skip the SOA record we already added
                if record.rtype == DNSResourceType::SOA {
                    continue;
                }

                let dns_resource = record
                    .to_dns_resource(&zone.origin, zone.default_ttl)
                    .map_err(DnsError::ParseError)?;

                // Estimate size (rough approximation)
                let record_size =
                    dns_resource.labels.join(".").len() + dns_resource.rdata.len() + 12;

                // Check if we need a new packet (keeping under 16KB for TCP)
                if current_size + record_size > 16384 && !current_packet.answers.is_empty() {
                    // Send current packet
                    packets.push(current_packet);

                    // Start new packet
                    current_packet = self.create_base_response(query);
                    current_size = 512;
                }

                current_packet.answers.push(dns_resource);
                current_packet.header.ancount += 1;
                current_size += record_size;
            }

            // Last record must be SOA again
            current_packet.answers.push(soa_resource);
            current_packet.header.ancount += 1;

            // Send final packet
            packets.push(current_packet);

            debug!(
                "AXFR response contains {} packets for zone {}",
                packets.len(),
                zone.origin
            );
        } else {
            warn!(
                "Zone {} has no SOA record, cannot perform AXFR",
                zone.origin
            );
            return Ok(vec![self.create_servfail_response(query)]);
        }

        Ok(packets)
    }

    /// Handle an IXFR (incremental zone transfer) request
    pub fn handle_ixfr(
        &self,
        query: &DNSPacket,
        client_addr: &SocketAddr,
    ) -> Result<Vec<DNSPacket>> {
        // Check if client is allowed to transfer
        if !self.is_transfer_allowed(client_addr) {
            warn!("Zone transfer denied for client {}", client_addr);
            return Ok(vec![self.create_refused_response(query)]);
        }

        // Validate query
        if query.questions.is_empty() {
            return Ok(vec![self.create_formerr_response(query)]);
        }

        let question = &query.questions[0];
        if question.qtype != DNSResourceType::IXFR {
            return Ok(vec![self.create_formerr_response(query)]);
        }

        let zone_name = question.labels.join(".").to_lowercase();

        // Get the zone
        let zone = match self.zone_store.get_zone(&zone_name) {
            Some(zone) => zone,
            None => {
                info!("IXFR request for non-existent zone: {}", zone_name);
                return Ok(vec![self.create_notauth_response(query)]);
            }
        };

        // Extract client's serial number from authority section
        let client_serial = self.extract_client_serial(query);

        info!(
            "Processing IXFR request for zone {} from {} (client serial: {:?})",
            zone_name, client_addr, client_serial
        );

        // For now, fall back to AXFR
        // TODO: Implement incremental transfers with zone history
        if client_serial.is_some() {
            debug!(
                "IXFR with serial number requested but zone history not implemented, falling back to AXFR"
            );
        }

        // Build IXFR response (currently same as AXFR)
        self.build_ixfr_response(query, &zone, client_serial)
    }

    /// Create a base response packet
    fn create_base_response(&self, query: &DNSPacket) -> DNSPacket {
        DNSPacket {
            header: DNSHeader {
                id: query.header.id,
                qr: true,
                opcode: query.header.opcode,
                aa: true,
                tc: false,
                rd: false,
                ra: false,
                z: 0,
                rcode: ResponseCode::NoError as u8,
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: query.questions.clone(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None, // AXFR typically doesn't use EDNS
        }
    }

    /// Create a REFUSED response
    fn create_refused_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_base_response(query);
        response.header.rcode = ResponseCode::Refused as u8;
        response
    }

    /// Create a FORMERR response
    fn create_formerr_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_base_response(query);
        response.header.rcode = ResponseCode::FormatError as u8;
        response
    }

    /// Create a NOTAUTH response
    fn create_notauth_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_base_response(query);
        response.header.rcode = ResponseCode::NotAuth as u8;
        response
    }

    /// Create a SERVFAIL response
    fn create_servfail_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_base_response(query);
        response.header.rcode = ResponseCode::ServerFailure as u8;
        response
    }

    /// Extract client's serial number from IXFR query authority section
    fn extract_client_serial(&self, query: &DNSPacket) -> Option<u32> {
        // RFC 1995: IXFR query contains SOA in authority section with client's serial
        if query.authorities.is_empty() {
            return None;
        }

        // Look for SOA record in authority section
        for auth in &query.authorities {
            if auth.rtype == DNSResourceType::SOA {
                // SOA RDATA format in wire format:
                // MNAME (domain name)
                // RNAME (domain name)
                // SERIAL (32 bits)
                // REFRESH (32 bits)
                // RETRY (32 bits)
                // EXPIRE (32 bits)
                // MINIMUM (32 bits)

                let rdata = &auth.rdata;
                let mut offset = 0;

                // Skip MNAME (domain name)
                while offset < rdata.len() {
                    let label_len = rdata[offset] as usize;
                    if label_len == 0 {
                        offset += 1;
                        break;
                    }
                    // Handle compression pointers
                    if label_len & 0xC0 == 0xC0 {
                        offset += 2;
                        break;
                    }
                    offset += 1 + label_len;
                }

                // Skip RNAME (domain name)
                while offset < rdata.len() {
                    let label_len = rdata[offset] as usize;
                    if label_len == 0 {
                        offset += 1;
                        break;
                    }
                    // Handle compression pointers
                    if label_len & 0xC0 == 0xC0 {
                        offset += 2;
                        break;
                    }
                    offset += 1 + label_len;
                }

                // Now we should be at the SERIAL field
                if offset + 4 <= rdata.len() {
                    let serial = u32::from_be_bytes([
                        rdata[offset],
                        rdata[offset + 1],
                        rdata[offset + 2],
                        rdata[offset + 3],
                    ]);
                    debug!("Extracted client serial from SOA: {}", serial);
                    return Some(serial);
                }
            }
        }

        None
    }

    /// Build IXFR response packets
    fn build_ixfr_response(
        &self,
        query: &DNSPacket,
        zone: &Zone,
        client_serial: Option<u32>,
    ) -> Result<Vec<DNSPacket>> {
        // Get current zone serial
        let current_serial = zone
            .get_soa()
            .and_then(|soa| soa.serial())
            .ok_or_else(|| DnsError::ParseError("Zone has no SOA record".to_string()))?;

        // Check if we can provide incremental transfer
        if let Some(client_serial) = client_serial {
            if client_serial == current_serial {
                // Client is up to date - return single packet with just SOA
                let mut response = self.create_base_response(query);
                response.header.aa = true;

                if let Some(soa) = zone.get_soa() {
                    let soa_resource = soa
                        .to_dns_resource(&zone.origin, zone.default_ttl)
                        .map_err(DnsError::ParseError)?;
                    response.answers.push(soa_resource);
                    response.header.ancount = 1;
                }

                debug!("IXFR: Client is up to date (serial {})", current_serial);
                return Ok(vec![response]);
            }

            // TODO: Implement incremental transfers when zone history is available
            debug!(
                "IXFR: Client has serial {}, current is {}, but no history available",
                client_serial, current_serial
            );
        }

        // Fall back to AXFR-style response but keep IXFR question
        let mut packets = self.build_axfr_response(query, zone)?;

        // Ensure question sections show IXFR
        for packet in &mut packets {
            if !packet.questions.is_empty() {
                packet.questions[0].qtype = DNSResourceType::IXFR;
            }
        }

        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::enums::DNSResourceClass;
    use crate::dns::question::DNSQuestion;
    use crate::zone::{Zone, ZoneRecord};

    #[test]
    fn test_transfer_allowed() {
        let store = Arc::new(ZoneStore::new());

        // No restrictions
        let handler = ZoneTransferHandler::new(store.clone(), vec![]);
        assert!(handler.is_transfer_allowed(&"192.168.1.1:53".parse().unwrap()));

        // With restrictions
        let handler = ZoneTransferHandler::new(
            store.clone(),
            vec!["192.168.1.0".to_string(), "10.0.0.1".to_string()],
        );
        assert!(!handler.is_transfer_allowed(&"192.168.1.1:53".parse().unwrap()));
        assert!(handler.is_transfer_allowed(&"10.0.0.1:53".parse().unwrap()));

        // Wildcard
        let handler = ZoneTransferHandler::new(store, vec!["*".to_string()]);
        assert!(handler.is_transfer_allowed(&"1.2.3.4:53".parse().unwrap()));
    }

    #[test]
    fn test_axfr_response() {
        let store = Arc::new(ZoneStore::new());
        let handler = ZoneTransferHandler::new(store.clone(), vec![]);

        // Create a test zone
        let mut zone = Zone::new("example.com".to_string(), 3600);

        // Add SOA
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();

        // Add some other records
        zone.add_record(ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::NS,
            "ns1.example.com.".to_string(),
        ))
        .unwrap();

        zone.add_record(ZoneRecord::new(
            "www".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::A,
            "192.0.2.1".to_string(),
        ))
        .unwrap();

        store.add_zone(zone).unwrap();

        // Create AXFR query
        let query = DNSPacket {
            header: DNSHeader {
                id: 1234,
                qr: false,
                opcode: 0,
                aa: false,
                tc: false,
                rd: false,
                ra: false,
                z: 0,
                rcode: 0,
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: vec![DNSQuestion {
                labels: vec!["example".to_string(), "com".to_string()],
                qtype: DNSResourceType::AXFR,
                qclass: DNSResourceClass::IN,
            }],
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: None,
        };

        let client_addr = "127.0.0.1:12345".parse().unwrap();
        let responses = handler.handle_axfr(&query, &client_addr).unwrap();

        assert!(!responses.is_empty());

        // First answer should be SOA
        assert!(!responses[0].answers.is_empty());
        assert_eq!(responses[0].answers[0].rtype, DNSResourceType::SOA);

        // Last answer should also be SOA
        let last_packet = responses.last().unwrap();
        let last_answer = last_packet.answers.last().unwrap();
        assert_eq!(last_answer.rtype, DNSResourceType::SOA);
    }
}
