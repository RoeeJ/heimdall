#![allow(clippy::field_reassign_with_default)]

use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};

#[test]
fn test_root_zone_query_parsing() {
    // Test parsing a query for the root zone (.) NS record
    // This is what dig +trace sends as its first query

    // Create a DNS query for root NS records
    let mut packet = DNSPacket::default();
    packet.header = DNSHeader {
        id: 12345,
        qr: false, // Query
        opcode: 0, // Standard query
        aa: false,
        tc: false,
        rd: false, // Non-recursive (like dig +trace)
        ra: false,
        z: 0,
        rcode: 0,
        qdcount: 1,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    // Query for root zone NS records
    packet.questions.push(DNSQuestion {
        labels: vec![], // Empty labels = root zone
        qtype: DNSResourceType::NS,
        qclass: DNSResourceClass::IN,
    });

    // Serialize the packet
    let serialized = packet.serialize().expect("Failed to serialize packet");
    println!(
        "Serialized packet ({} bytes): {:02x?}",
        serialized.len(),
        serialized
    );

    // Parse it back
    let parsed = DNSPacket::parse(&serialized).expect("Failed to parse packet");

    // Check the parsed question
    assert_eq!(parsed.header.qdcount, 1);
    assert_eq!(parsed.questions.len(), 1);

    let question = &parsed.questions[0];
    println!("Parsed question labels: {:?}", question.labels);
    println!("Parsed question type: {:?}", question.qtype);
    println!("Parsed question class: {:?}", question.qclass);

    // The labels should be empty for root zone
    assert_eq!(question.labels.len(), 0);
    assert_eq!(question.qtype, DNSResourceType::NS);
    assert_eq!(question.qclass, DNSResourceClass::IN);
}

#[test]
fn test_unknown_type_parsing() {
    // Test what happens with TYPE512 and CLASS256
    use bitstream_io::{BigEndian, BitWrite, BitWriter};

    let mut buf = Vec::new();
    let mut writer = BitWriter::<_, BigEndian>::new(&mut buf);

    // Write a minimal DNS header
    writer.write_var::<u16>(16, 12345).unwrap(); // ID
    writer.write_var::<u8>(1, 0).unwrap(); // QR (query)
    writer.write_var::<u8>(4, 0).unwrap(); // Opcode
    writer.write_var::<u8>(1, 0).unwrap(); // AA
    writer.write_var::<u8>(1, 0).unwrap(); // TC
    writer.write_var::<u8>(1, 0).unwrap(); // RD
    writer.write_var::<u8>(1, 0).unwrap(); // RA
    writer.write_var::<u8>(3, 0).unwrap(); // Z
    writer.write_var::<u8>(4, 0).unwrap(); // RCODE
    writer.write_var::<u16>(16, 1).unwrap(); // QDCOUNT
    writer.write_var::<u16>(16, 0).unwrap(); // ANCOUNT
    writer.write_var::<u16>(16, 0).unwrap(); // NSCOUNT
    writer.write_var::<u16>(16, 0).unwrap(); // ARCOUNT

    // Write a question with root zone
    writer.write_var::<u8>(8, 0).unwrap(); // Root zone (null label)
    writer.write_var::<u16>(16, 512).unwrap(); // TYPE512
    writer.write_var::<u16>(16, 256).unwrap(); // CLASS256

    // Writer will be dropped automatically

    println!(
        "Test packet with TYPE512/CLASS256 ({} bytes): {:02x?}",
        buf.len(),
        buf
    );

    // Try to parse it
    match DNSPacket::parse(&buf) {
        Ok(parsed) => {
            println!("Parsed successfully!");
            println!("Question type: {:?}", parsed.questions[0].qtype);
            println!("Question class: {:?}", parsed.questions[0].qclass);
        }
        Err(e) => {
            println!("Parse error: {:?}", e);
        }
    }
}
