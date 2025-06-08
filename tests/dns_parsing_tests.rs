use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
};

// Sample DNS query packet for google.com (A record)
// This is a real DNS query captured from dig google.com
const GOOGLE_COM_QUERY: &[u8] = &[
    0x12, 0x34, // Transaction ID
    0x01, 0x00, // Flags: Standard query
    0x00, 0x01, // Questions: 1
    0x00, 0x00, // Answer RRs: 0
    0x00, 0x00, // Authority RRs: 0
    0x00, 0x00, // Additional RRs: 0
    // Question section
    0x06, b'g', b'o', b'o', b'g', b'l', b'e', // "google"
    0x03, b'c', b'o', b'm', // "com"
    0x00, // Root label
    0x00, 0x01, // Type: A
    0x00, 0x01, // Class: IN
];

#[test]
fn test_parse_dns_header() {
    let packet = DNSPacket::parse(GOOGLE_COM_QUERY).expect("Failed to parse packet");

    assert_eq!(packet.header.id, 0x1234);
    assert!(!packet.header.qr); // Query
    assert_eq!(packet.header.opcode, 0); // Standard query
    assert!(!packet.header.aa);
    assert!(!packet.header.tc);
    assert!(packet.header.rd); // Recursion desired
    assert!(!packet.header.ra);
    assert_eq!(packet.header.z, 0);
    assert_eq!(packet.header.rcode, 0);
    assert_eq!(packet.header.qdcount, 1);
    assert_eq!(packet.header.ancount, 0);
    assert_eq!(packet.header.nscount, 0);
    assert_eq!(packet.header.arcount, 0);
}

#[test]
fn test_parse_dns_question() {
    let packet = DNSPacket::parse(GOOGLE_COM_QUERY).expect("Failed to parse packet");

    assert_eq!(packet.questions.len(), 1);

    let question = &packet.questions[0];
    assert_eq!(question.labels, vec!["google", "com"]);
    assert_eq!(question.qtype, DNSResourceType::A);
    assert_eq!(question.qclass, DNSResourceClass::IN);
}

#[test]
fn test_generate_response() {
    let packet = DNSPacket::parse(GOOGLE_COM_QUERY).expect("Failed to parse packet");
    let response = packet.generate_response();

    // Response should have QR bit set and RA bit set
    assert!(response.header.qr);
    assert!(response.header.ra);

    // Everything else should be copied
    assert_eq!(response.header.id, packet.header.id);
    assert_eq!(response.questions, packet.questions);
}

#[test]
fn test_serialize_packet() {
    let packet = DNSPacket::parse(GOOGLE_COM_QUERY).expect("Failed to parse packet");
    let response = packet.generate_response();
    let serialized = response.serialize().expect("Failed to serialize");

    // At minimum, should have header (12 bytes) + question
    assert!(serialized.len() >= 12);

    // Check that QR bit is set in serialized response
    assert_eq!(serialized[2] & 0x80, 0x80); // QR bit should be 1
}

#[test]
fn test_empty_packet() {
    let result = DNSPacket::parse(&[]);
    assert!(result.is_err());
}

#[test]
fn test_truncated_header() {
    let truncated = &GOOGLE_COM_QUERY[..10]; // Not enough for full header
    let result = DNSPacket::parse(truncated);
    assert!(result.is_err());
}

#[test]
fn test_multiple_questions() {
    let multi_question_packet = vec![
        0x56, 0x78, // Transaction ID
        0x01, 0x00, // Flags: Standard query
        0x00, 0x02, // Questions: 2
        0x00, 0x00, // Answer RRs: 0
        0x00, 0x00, // Authority RRs: 0
        0x00, 0x00, // Additional RRs: 0
        // First question
        0x03, b'w', b'w', b'w', 0x06, b'g', b'o', b'o', b'g', b'l', b'e', 0x03, b'c', b'o', b'm',
        0x00, 0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
        // Second question
        0x04, b'm', b'a', b'i', b'l', 0x06, b'g', b'o', b'o', b'g', b'l', b'e', 0x03, b'c', b'o',
        b'm', 0x00, 0x00, 0x0F, // Type: MX
        0x00, 0x01, // Class: IN
    ];

    let packet = DNSPacket::parse(&multi_question_packet).expect("Failed to parse packet");
    assert_eq!(packet.header.qdcount, 2);
    assert_eq!(packet.questions.len(), 2);

    assert_eq!(packet.questions[0].labels, vec!["www", "google", "com"]);
    assert_eq!(packet.questions[0].qtype, DNSResourceType::A);

    assert_eq!(packet.questions[1].labels, vec!["mail", "google", "com"]);
    assert_eq!(packet.questions[1].qtype, DNSResourceType::MX);
}

#[test]
fn test_label_parsing_edge_cases() {
    // Test with max label length (63)
    let mut max_label_packet = vec![
        0x00, 0x00, // Transaction ID
        0x01, 0x00, // Flags
        0x00, 0x01, // Questions: 1
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Other counts
        0x3F, // Label length: 63 (max allowed)
    ];
    max_label_packet.extend(vec![b'a'; 63]); // 63 'a' characters
    max_label_packet.extend(&[
        0x00, // Root label
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ]);

    let packet = DNSPacket::parse(&max_label_packet).expect("Failed to parse packet");
    assert_eq!(packet.questions[0].labels[0].len(), 63);
    assert_eq!(packet.questions[0].labels[0], "a".repeat(63));
}
