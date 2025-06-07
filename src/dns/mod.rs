pub mod common;
pub mod enums;
pub mod header;
pub mod question;
pub mod resource;

use bitstream_io::{BigEndian, BitReader, BitWriter};
use common::PacketComponent;
use header::DNSHeader;
use question::DNSQuestion;
use resource::DNSResource;
use tracing::{debug, trace};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DNSPacket {
    pub header: DNSHeader,
    pub questions: Vec<DNSQuestion>,
    pub answers: Vec<DNSResource>,
    pub authorities: Vec<DNSResource>,
    pub resources: Vec<DNSResource>,
}

#[derive(Debug)]
pub enum ParseError {
    InvalidHeader,
    InvalidLabel,
    InvalidQuestionSection,
    InvalidAnswerSection,
    InvalidAuthoritySection,
    InvalidAdditionalSection,
    InvalidBitStream(String),
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::InvalidBitStream(e.to_string())
    }
}

impl DNSPacket {
    pub fn valid(&self) -> bool {
        false
    }

    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        trace!("Parsing DNS packet, size: {} bytes", buf.len());
        let mut reader = BitReader::<_, BigEndian>::new(buf);
        let mut packet = DNSPacket::default();
        packet.header.read(&mut reader)?;
        debug!("Parsed DNS header: id={}, qr={}, opcode={}, questions={}", 
            packet.header.id, packet.header.qr, packet.header.opcode, packet.header.qdcount);
        for _ in 0..packet.header.qdcount {
            let mut question = DNSQuestion::default();
            question.read(&mut reader)?;
            packet.questions.push(question);
        }

        for _ in 0..packet.header.ancount {
            let mut answer = DNSResource::default();
            answer.read(&mut reader)?;
            packet.answers.push(answer);
        }

        for _ in 0..packet.header.nscount {
            let mut authority = DNSResource::default();
            authority.read(&mut reader)?;
            packet.authorities.push(authority);
        }

        for _ in 0..packet.header.arcount {
            let mut resource = DNSResource::default();
            resource.read(&mut reader)?;
            packet.resources.push(resource);
        }

        Ok(packet)
    }

    pub fn serialize(&self) -> Result<Vec<u8>, ParseError> {
        let mut buf = Vec::new();
        let mut writer: BitWriter<&mut Vec<u8>, BigEndian> = BitWriter::new(&mut buf);
        self.header.write(&mut writer)?;

        for question in self.questions.iter() {
            question.write(&mut writer)?;
        }

        // TODO: Serialize answers, authorities, and additional sections
        
        Ok(buf)
    }

    pub fn generate_response(&self) -> Self {
        let mut packet = self.clone();
        packet.header.qr = true;
        packet.header.ra = true;
        for question in &packet.questions {
            let name = question
                .labels
                .iter()
                .filter(|l| l.len() > 0)
                .map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if name.is_empty() {
                continue;
            }
            debug!("DNS query for: {}", name);
        }
        packet
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_dns_packet() {
        let packet = DNSPacket::default();
        assert_eq!(packet.valid(), false); // Currently always returns false
    }
}
