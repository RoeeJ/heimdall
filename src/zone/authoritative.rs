//! Authoritative DNS response generation
//!
//! This module handles generating authoritative responses from zone data.

use super::{QueryResult, Zone, ZoneStore};
use crate::dns::{
    DNSHeader, DNSPacket,
    enums::{DNSResourceType, ResponseCode},
};
use std::sync::Arc;
use tracing::{debug, warn};

/// Authoritative DNS responder
pub struct AuthoritativeResponder {
    zone_store: Arc<ZoneStore>,
}

impl AuthoritativeResponder {
    /// Create a new authoritative responder
    pub fn new(zone_store: Arc<ZoneStore>) -> Self {
        Self { zone_store }
    }

    /// Check if we're authoritative for a domain and generate response
    pub fn generate_response(&self, query: &DNSPacket) -> Option<DNSPacket> {
        // We only handle standard queries
        if query.header.opcode != 0 {
            return None;
        }

        // Process each question
        if query.questions.is_empty() {
            return None;
        }

        let question = &query.questions[0];
        let qname = question.labels.join(".").to_lowercase();

        // Build authoritative response
        let mut response = DNSPacket {
            header: DNSHeader {
                id: query.header.id,
                qr: true,
                opcode: 0,
                aa: true, // Authoritative Answer flag
                tc: false,
                rd: query.header.rd,
                ra: false, // We don't do recursion in authoritative mode
                z: 0,
                rcode: ResponseCode::NoError.to_u8(),
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: vec![question.clone()],
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: query.edns.clone(),
        };

        // Query the zone store directly
        match self.zone_store.query(&qname, question.qtype) {
            QueryResult::Success { records, .. } => {
                debug!("Found {} records for {}", records.len(), qname);
                // Add answer records
                response.answers = records;
                response.header.ancount = response.answers.len() as u16;
            }
            QueryResult::NoData { soa, .. } => {
                debug!("NoData response for {} type {:?}", qname, question.qtype);
                // Name exists but no data for this type
                // Add SOA record in authority section
                if let Some(soa_record) = soa {
                    response.authorities.push(soa_record);
                    response.header.nscount = 1;
                }
            }
            QueryResult::NXDomain { soa, .. } => {
                debug!("NXDomain response for {}", qname);
                // Name doesn't exist
                response.header.rcode = ResponseCode::NameError.to_u8();
                // Add SOA record in authority section
                if let Some(soa_record) = soa {
                    response.authorities.push(soa_record);
                    response.header.nscount = 1;
                }
            }
            QueryResult::Delegation { ns_records, .. } => {
                debug!("Delegation response for {}", qname);
                // We have a delegation
                response.authorities = ns_records;
                response.header.nscount = response.authorities.len() as u16;
                response.header.aa = false; // Not authoritative for delegated zones

                // Add glue records in additional section if available
                if let Some(zone) = self.zone_store.find_zone(&qname) {
                    self.add_additional_records(&mut response, &zone);
                }
            }
            QueryResult::NotAuthoritative => {
                debug!("Not authoritative for {}", qname);
                // We're not authoritative for this query
                return None;
            }
            QueryResult::Error(e) => {
                warn!("Error processing query for {}: {}", qname, e);
                response.header.rcode = ResponseCode::ServerFailure.to_u8();
            }
        }

        Some(response)
    }

    /// Add additional records like glue records
    fn add_additional_records(&self, response: &mut DNSPacket, zone: &Zone) {
        // For NS records in authority section, add corresponding A/AAAA glue records
        let mut glue_records = vec![];

        for auth_record in &response.authorities {
            if auth_record.rtype == DNSResourceType::NS {
                if let Some(ns_name) = &auth_record.parsed_rdata {
                    // Check if this NS is in our zone (needs glue)
                    if ns_name.ends_with(&zone.origin)
                        || ns_name.trim_end_matches('.') == zone.origin
                    {
                        // Add A records
                        if let QueryResult::Success { records, .. } = self
                            .zone_store
                            .query(ns_name.trim_end_matches('.'), DNSResourceType::A)
                        {
                            glue_records.extend(records);
                        }

                        // Add AAAA records
                        if let QueryResult::Success { records, .. } = self
                            .zone_store
                            .query(ns_name.trim_end_matches('.'), DNSResourceType::AAAA)
                        {
                            glue_records.extend(records);
                        }
                    }
                }
            }
        }

        if !glue_records.is_empty() {
            response.resources.extend(glue_records);
            response.header.arcount = response.resources.len() as u16;
        }
    }

    /// Check if we're authoritative for any zone that could answer this query
    pub fn is_authoritative_for(&self, domain: &str) -> bool {
        // Check if the zone store has a zone for this domain
        matches!(
            self.zone_store.query(domain, DNSResourceType::A),
            QueryResult::Success { .. }
                | QueryResult::NoData { .. }
                | QueryResult::NXDomain { .. }
                | QueryResult::Delegation { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::enums::DNSResourceClass;
    use crate::dns::question::DNSQuestion;
    use crate::zone::ZoneParser;

    #[test]
    fn test_authoritative_response() {
        // Create a test zone
        let zone_data = r#"
$ORIGIN example.com.
$TTL 3600
@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
@   IN  NS  ns2.example.com.
@   IN  A   192.0.2.1
www IN  A   192.0.2.2
ns1 IN  A   192.0.2.10
ns2 IN  A   192.0.2.11
"#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_data).unwrap();

        let zone_store = Arc::new(ZoneStore::new());
        zone_store.add_zone(zone).unwrap();

        let responder = AuthoritativeResponder::new(zone_store);

        // Create a test query
        let query = DNSPacket {
            header: DNSHeader {
                id: 1234,
                qr: false,
                opcode: 0,
                aa: false,
                tc: false,
                rd: true,
                ra: false,
                z: 0,
                rcode: 0,
                qdcount: 1,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: vec![DNSQuestion {
                labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
                qtype: DNSResourceType::A,
                qclass: DNSResourceClass::IN,
            }],
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: None,
        };

        let response = responder.generate_response(&query).unwrap();

        // Verify authoritative response
        assert!(response.header.aa); // Authoritative Answer flag
        assert_eq!(response.header.rcode, 0); // NoError
        assert_eq!(response.answers.len(), 1);
        assert_eq!(
            response.answers[0].parsed_rdata,
            Some("192.0.2.2".to_string())
        );
    }
}
