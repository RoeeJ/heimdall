//! DNS NOTIFY implementation (RFC 1996)
//!
//! This module implements the DNS NOTIFY protocol for zone change notifications.

use super::{Zone, ZoneStore};
use crate::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType, ResponseCode},
    header::DNSHeader,
    question::DNSQuestion,
};
use crate::error::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{Duration, timeout};
use tracing::{debug, info, warn};

/// DNS NOTIFY handler
pub struct NotifyHandler {
    zone_store: Arc<ZoneStore>,
    allowed_notifiers: Vec<String>, // IP addresses allowed to send NOTIFY
    secondary_servers: Vec<SocketAddr>, // Secondary servers to notify
}

impl NotifyHandler {
    /// Create a new NOTIFY handler
    pub fn new(
        zone_store: Arc<ZoneStore>,
        allowed_notifiers: Vec<String>,
        secondary_servers: Vec<SocketAddr>,
    ) -> Self {
        Self {
            zone_store,
            allowed_notifiers,
            secondary_servers,
        }
    }

    /// Check if a client is allowed to send NOTIFY
    pub fn is_notify_allowed(&self, client_addr: &SocketAddr) -> bool {
        if self.allowed_notifiers.is_empty() {
            // No restrictions if list is empty
            return true;
        }

        let client_ip = client_addr.ip().to_string();

        // Simple IP matching for now
        self.allowed_notifiers
            .iter()
            .any(|allowed| allowed == &client_ip || allowed == "*")
    }

    /// Handle an incoming NOTIFY message
    pub fn handle_notify(&self, packet: &DNSPacket, client_addr: &SocketAddr) -> Result<DNSPacket> {
        // Check if client is allowed to send NOTIFY
        if !self.is_notify_allowed(client_addr) {
            warn!("NOTIFY denied for client {}", client_addr);
            return Ok(self.create_refused_response(packet));
        }

        // Validate NOTIFY packet
        if packet.header.opcode != 4 {
            // Not a NOTIFY
            return Ok(self.create_formerr_response(packet));
        }

        if packet.questions.is_empty() {
            return Ok(self.create_formerr_response(packet));
        }

        let question = &packet.questions[0];
        let zone_name = question.labels.join(".").to_lowercase();

        // Check if we have this zone
        match self.zone_store.get_zone(&zone_name) {
            Some(zone) => {
                info!(
                    "Received NOTIFY for zone {} from {}",
                    zone_name, client_addr
                );

                // Extract new serial from SOA in answer section if present
                let new_serial = self.extract_serial_from_notify(packet);

                if let Some(new_serial) = new_serial {
                    debug!(
                        "NOTIFY indicates zone {} serial is now {}",
                        zone_name, new_serial
                    );

                    // Check if we need to update
                    if new_serial > zone.serial {
                        info!(
                            "Zone {} serial {} is older than NOTIFY serial {}, transfer needed",
                            zone_name, zone.serial, new_serial
                        );
                        // In a real implementation, this would trigger a zone transfer
                        // For now, we just acknowledge
                    } else {
                        debug!(
                            "Zone {} serial {} is current or newer than NOTIFY serial {}",
                            zone_name, zone.serial, new_serial
                        );
                    }
                }

                // Send acknowledgment
                Ok(self.create_notify_response(packet))
            }
            None => {
                warn!("NOTIFY for non-existent zone: {}", zone_name);
                Ok(self.create_notauth_response(packet))
            }
        }
    }

    /// Send NOTIFY messages to secondary servers for a zone
    pub async fn send_notify(&self, zone_name: &str, socket: Arc<UdpSocket>) -> Result<()> {
        let zone = match self.zone_store.get_zone(zone_name) {
            Some(zone) => zone,
            None => {
                warn!("Cannot send NOTIFY for non-existent zone: {}", zone_name);
                return Ok(());
            }
        };

        info!(
            "Sending NOTIFY for zone {} to {} servers",
            zone_name,
            self.secondary_servers.len()
        );

        // Create NOTIFY packet
        let notify_packet = self.create_notify_packet(&zone);

        // Send to each secondary
        for secondary in &self.secondary_servers {
            match self
                .send_notify_to_server(&notify_packet, secondary, &socket)
                .await
            {
                Ok(_) => debug!("NOTIFY sent to {} for zone {}", secondary, zone_name),
                Err(e) => warn!(
                    "Failed to send NOTIFY to {} for zone {}: {}",
                    secondary, zone_name, e
                ),
            }
        }

        Ok(())
    }

    /// Send NOTIFY to a specific server and wait for response
    async fn send_notify_to_server(
        &self,
        packet: &DNSPacket,
        server: &SocketAddr,
        socket: &Arc<UdpSocket>,
    ) -> Result<()> {
        let packet_bytes = packet.to_bytes();

        // Send packet
        socket.send_to(&packet_bytes, server).await?;

        // Wait for response with timeout
        let mut buf = vec![0u8; 512];
        match timeout(Duration::from_secs(5), socket.recv_from(&mut buf)).await {
            Ok(Ok((len, from))) => {
                if from == *server {
                    // Parse response
                    match DNSPacket::parse(&buf[..len]) {
                        Ok(response) => {
                            if response.header.id == packet.header.id
                                && response.header.qr
                                && response.header.opcode == 4
                            {
                                if response.header.rcode == ResponseCode::NoError as u8 {
                                    debug!("NOTIFY acknowledged by {}", server);
                                } else {
                                    warn!(
                                        "NOTIFY rejected by {} with rcode {}",
                                        server, response.header.rcode
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse NOTIFY response from {}: {}", server, e);
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to receive NOTIFY response from {}: {}", server, e);
            }
            Err(_) => {
                warn!("NOTIFY response timeout from {}", server);
            }
        }

        Ok(())
    }

    /// Create a NOTIFY packet for a zone
    fn create_notify_packet(&self, zone: &Zone) -> DNSPacket {
        let mut packet = DNSPacket {
            header: DNSHeader {
                id: rand::random(),
                qr: false,
                opcode: 4, // NOTIFY
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
            questions: vec![DNSQuestion {
                labels: zone.origin.split('.').map(|s| s.to_string()).collect(),
                qtype: DNSResourceType::SOA,
                qclass: DNSResourceClass::IN,
            }],
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: None,
        };

        // Optionally include SOA record in answer section
        if let Some(soa) = zone.get_soa() {
            if let Ok(soa_resource) = soa.to_dns_resource(&zone.origin, zone.default_ttl) {
                packet.answers.push(soa_resource);
                packet.header.ancount = 1;
            }
        }

        packet
    }

    /// Extract serial number from NOTIFY packet
    fn extract_serial_from_notify(&self, packet: &DNSPacket) -> Option<u32> {
        // Look for SOA record in answer section
        for answer in &packet.answers {
            if answer.rtype == DNSResourceType::SOA {
                // Parse SOA rdata to extract serial
                if let Some(rdata) = &answer.parsed_rdata {
                    let parts: Vec<&str> = rdata.split_whitespace().collect();
                    if parts.len() >= 3 {
                        if let Ok(serial) = parts[2].parse::<u32>() {
                            return Some(serial);
                        }
                    }
                }
            }
        }
        None
    }

    /// Create a NOTIFY response (acknowledgment)
    fn create_notify_response(&self, query: &DNSPacket) -> DNSPacket {
        DNSPacket {
            header: DNSHeader {
                id: query.header.id,
                qr: true,
                opcode: 4, // NOTIFY
                aa: query.header.aa,
                tc: false,
                rd: false,
                ra: false,
                z: 0,
                rcode: ResponseCode::NoError as u8,
                qdcount: query.header.qdcount,
                ancount: 0,
                nscount: 0,
                arcount: 0,
            },
            questions: query.questions.clone(),
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: None,
        }
    }

    /// Create a REFUSED response
    fn create_refused_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_notify_response(query);
        response.header.rcode = ResponseCode::Refused as u8;
        response
    }

    /// Create a FORMERR response
    fn create_formerr_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_notify_response(query);
        response.header.rcode = ResponseCode::FormatError as u8;
        response
    }

    /// Create a NOTAUTH response
    fn create_notauth_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = self.create_notify_response(query);
        response.header.rcode = ResponseCode::NotAuth as u8;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone::{Zone, ZoneRecord};

    #[test]
    fn test_notify_allowed() {
        let store = Arc::new(ZoneStore::new());

        // No restrictions
        let handler = NotifyHandler::new(store.clone(), vec![], vec![]);
        assert!(handler.is_notify_allowed(&"192.168.1.1:53".parse().unwrap()));

        // With restrictions
        let handler = NotifyHandler::new(
            store.clone(),
            vec!["192.168.1.0".to_string(), "10.0.0.1".to_string()],
            vec![],
        );
        assert!(!handler.is_notify_allowed(&"192.168.1.1:53".parse().unwrap()));
        assert!(handler.is_notify_allowed(&"10.0.0.1:53".parse().unwrap()));
    }

    #[test]
    fn test_handle_notify() {
        let store = Arc::new(ZoneStore::new());
        let handler = NotifyHandler::new(store.clone(), vec![], vec![]);

        // Create a test zone
        let mut zone = Zone::new("example.com".to_string(), 3600);
        let soa = ZoneRecord::new(
            "@".to_string(),
            Some(3600),
            DNSResourceClass::IN,
            DNSResourceType::SOA,
            "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
        );
        zone.add_record(soa).unwrap();
        store.add_zone(zone).unwrap();

        // Create NOTIFY packet
        let notify = DNSPacket {
            header: DNSHeader {
                id: 1234,
                qr: false,
                opcode: 4, // NOTIFY
                aa: true,
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
                qtype: DNSResourceType::SOA,
                qclass: DNSResourceClass::IN,
            }],
            answers: vec![],
            authorities: vec![],
            resources: vec![],
            edns: None,
        };

        let client_addr = "127.0.0.1:12345".parse().unwrap();
        let response = handler.handle_notify(&notify, &client_addr).unwrap();

        assert!(response.header.qr);
        assert_eq!(response.header.opcode, 4);
        assert_eq!(response.header.rcode, ResponseCode::NoError as u8);
    }
}
