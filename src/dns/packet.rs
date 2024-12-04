use super::{
    DnsAdditional, DnsAnswer, DnsAuthority, DnsHeader, DnsQType, DnsQr, DnsQuestion,
    DnsResourceRecord, DnsResponseCode, DnsWireFormat, EdnsOptionCode, RData,
};
use crate::constants::{EDNS_UDP_SIZE, EDNS_VERSION};
use bitstream_io::{BigEndian, BitReader};
use std::io::Cursor;

#[derive(Debug, Clone)]
pub struct DnsPacket {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsAnswer>,
    pub authorities: Vec<DnsAuthority>,
    pub additional: Vec<DnsAdditional>,
}

impl DnsPacket {
    pub fn new(header: DnsHeader) -> Self {
        DnsPacket {
            header,
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            additional: Vec::new(),
        }
    }

    pub fn update_counts(&mut self) {
        self.header.qdcount = self.questions.len() as u16;
        self.header.ancount = self.answers.len() as u16;
        self.header.nscount = self.authorities.len() as u16;
        self.header.arcount = self.additional.len() as u16;
    }

    pub fn get_client_cookie(&self) -> Option<Vec<u8>> {
        for additional in &self.additional {
            if let DnsQType::OPT = additional.qtype {
                if let RData::OPT { ref options, .. } = additional.rdata {
                    for option in options {
                        if let EdnsOptionCode::Cookie = option.code {
                            if option.data.len() >= 8 {
                                return Some(option.data[0..8].to_vec());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn create_response(&self) -> Self {
        let mut response = DnsPacket {
            header: DnsHeader {
                id: self.header.id,
                qr: DnsQr::Response,
                opcode: self.header.opcode,
                aa: 0,
                tc: 0,
                rd: self.header.rd,
                ra: 1,
                z: 0,
                rcode: DnsResponseCode::NoError,
                qdcount: self.header.qdcount,
                ancount: 0,
                nscount: 0,
                arcount: 1,
            },
            questions: self.questions.clone(),
            answers: Vec::new(),
            authorities: Vec::new(),
            additional: Vec::new(),
        };

        if let Some(client_cookie) = self.get_client_cookie() {
            response
                .additional
                .push(DnsResourceRecord::new_opt_with_cookie(
                    EDNS_UDP_SIZE,
                    0,
                    EDNS_VERSION,
                    false,
                    &client_cookie,
                ));
        } else {
            response.additional.push(DnsResourceRecord::new_opt(
                EDNS_UDP_SIZE,
                0,
                EDNS_VERSION,
                false,
            ));
        }

        response
    }
}

impl DnsWireFormat for DnsPacket {
    fn to_wire(&self) -> Vec<u8> {
        let mut packet = self.clone();
        packet.update_counts();

        let mut bytes = Vec::new();

        // Add header
        bytes.extend(packet.header.to_wire());

        // Add questions
        for question in &packet.questions {
            bytes.extend(question.to_wire());
        }

        // Add answers
        for answer in &packet.answers {
            bytes.extend(answer.to_wire());
        }

        // Add authorities
        for authority in &packet.authorities {
            bytes.extend(authority.to_wire());
        }

        // Add additional records
        for additional in &packet.additional {
            bytes.extend(additional.to_wire());
        }

        bytes
    }

    fn from_wire(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<Self, std::io::Error> {
        // Parse header
        let header = DnsHeader::from_wire(reader)?;

        // Parse questions
        let mut questions = Vec::new();
        for _ in 0..header.qdcount {
            questions.push(DnsQuestion::from_wire(reader)?);
        }

        // Parse answers
        let mut answers = Vec::new();
        for _ in 0..header.ancount {
            answers.push(DnsAnswer::from_wire(reader)?);
        }

        // Parse authorities
        let mut authorities = Vec::new();
        for _ in 0..header.nscount {
            authorities.push(DnsAuthority::from_wire(reader)?);
        }

        // Parse additional records
        let mut additional = Vec::new();
        for _ in 0..header.arcount {
            additional.push(DnsAdditional::from_wire(reader)?);
        }

        Ok(DnsPacket {
            header,
            questions,
            answers,
            authorities,
            additional,
        })
    }
}
