use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
    resource::DNSResource,
};

fn create_test_packet_with_resource(resource: DNSResource) -> Vec<u8> {
    let packet = DNSPacket {
        header: DNSHeader {
            id: 1234,
            qr: true,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: true,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 1,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![DNSQuestion {
            labels: resource.labels.clone(),
            qtype: resource.rtype,
            qclass: resource.rclass,
        }],
        answers: vec![resource],
        authorities: vec![],
        resources: vec![],
        edns: None,
    };

    packet.serialize().unwrap()
}

#[test]
fn test_dnskey_record_parsing() {
    // Build DNSKEY RDATA: Flags=256 (ZSK), Protocol=3, Algorithm=8 (RSA-SHA256), Public Key
    let mut rdata = Vec::new();

    // Flags: 256 (ZSK - Zone Signing Key)
    rdata.extend(&256u16.to_be_bytes());
    // Protocol: 3 (DNSSEC)
    rdata.push(3);
    // Algorithm: 8 (RSA-SHA256)
    rdata.push(8);

    // Mock public key (shortened for test)
    let public_key = vec![
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    rdata.extend(&public_key);

    // Create a mock DNSKEY record
    let dnskey_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::DNSKEY,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(dnskey_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed DNSKEY record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_dnskey = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_dnskey.parsed_rdata.is_some());
    let parsed = parsed_dnskey.parsed_rdata.as_ref().unwrap();

    // Should have format: "256 3 8 <base64_key>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0], "256"); // Flags
    assert_eq!(parts[1], "3"); // Protocol
    assert_eq!(parts[2], "8"); // Algorithm

    // Test helper method
    let fields = parsed_dnskey.get_dnskey_fields().unwrap();
    assert_eq!(fields.0, 256); // flags
    assert_eq!(fields.1, 3); // protocol
    assert_eq!(fields.2, 8); // algorithm
    assert!(!fields.3.is_empty()); // public key base64
}

#[test]
fn test_ds_record_parsing() {
    // Build DS RDATA: Key Tag=12345, Algorithm=8, Digest Type=2 (SHA-256), Digest
    let mut rdata = Vec::new();

    // Key Tag: 12345
    rdata.extend(&12345u16.to_be_bytes());
    // Algorithm: 8 (RSA-SHA256)
    rdata.push(8);
    // Digest Type: 2 (SHA-256)
    rdata.push(2);

    // Mock digest (32 bytes for SHA-256)
    let digest = vec![
        0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67,
        0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45,
        0x67, 0x89,
    ];
    rdata.extend(&digest);

    // Create a mock DS record
    let ds_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::DS,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(ds_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed DS record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_ds = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_ds.parsed_rdata.is_some());
    let parsed = parsed_ds.parsed_rdata.as_ref().unwrap();

    // Should have format: "12345 8 2 <hex_digest>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0], "12345"); // Key Tag
    assert_eq!(parts[1], "8"); // Algorithm
    assert_eq!(parts[2], "2"); // Digest Type

    // Test helper method
    let fields = parsed_ds.get_ds_fields().unwrap();
    assert_eq!(fields.0, 12345); // key tag
    assert_eq!(fields.1, 8); // algorithm
    assert_eq!(fields.2, 2); // digest type
    assert_eq!(fields.3.len(), 64); // hex digest should be 64 chars for SHA-256
}

#[test]
fn test_rrsig_record_parsing() {
    // Build RRSIG RDATA
    let mut rdata = Vec::new();

    // Type Covered: A (1)
    rdata.extend(&1u16.to_be_bytes());
    // Algorithm: 8 (RSA-SHA256)
    rdata.push(8);
    // Labels: 2 (example.com has 2 labels)
    rdata.push(2);
    // Original TTL: 3600
    rdata.extend(&3600u32.to_be_bytes());
    // Signature Expiration: 1735689600 (2025-01-01)
    rdata.extend(&1735689600u32.to_be_bytes());
    // Signature Inception: 1704067200 (2024-01-01)
    rdata.extend(&1704067200u32.to_be_bytes());
    // Key Tag: 12345
    rdata.extend(&12345u16.to_be_bytes());

    // Signer's Name: example.com
    rdata.extend(&[7]); // length of "example"
    rdata.extend(b"example");
    rdata.extend(&[3]); // length of "com"
    rdata.extend(b"com");
    rdata.extend(&[0]); // null terminator

    // Mock signature (shortened for test)
    let signature = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
    rdata.extend(&signature);

    // Create a mock RRSIG record
    let rrsig_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::RRSIG,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(rrsig_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed RRSIG record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_rrsig = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_rrsig.parsed_rdata.is_some());
    let parsed = parsed_rrsig.parsed_rdata.as_ref().unwrap();

    // Should have format: "1 8 2 3600 1735689600 1704067200 12345 example.com <base64_sig>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 9);
    assert_eq!(parts[0], "1"); // Type Covered (A)
    assert_eq!(parts[1], "8"); // Algorithm
    assert_eq!(parts[2], "2"); // Labels
    assert_eq!(parts[3], "3600"); // Original TTL
    assert_eq!(parts[4], "1735689600"); // Sig Expiration
    assert_eq!(parts[5], "1704067200"); // Sig Inception
    assert_eq!(parts[6], "12345"); // Key Tag
    assert_eq!(parts[7], "example.com"); // Signer's Name
    assert!(!parts[8].is_empty()); // Signature (base64)
}

#[test]
fn test_nsec_record_parsing() {
    // Build NSEC RDATA: Next Domain Name + Type Bit Maps
    let mut rdata = Vec::new();

    // Next domain: next.example.com
    rdata.extend(&[4]); // length of "next"
    rdata.extend(b"next");
    rdata.extend(&[7]); // length of "example"
    rdata.extend(b"example");
    rdata.extend(&[3]); // length of "com"
    rdata.extend(b"com");
    rdata.extend(&[0]); // null terminator

    // Type bit maps
    // Window 0 (types 0-255)
    rdata.push(0); // Window number
    rdata.push(6); // Bitmap length (6 bytes = 48 bits)
    // Set bits for A(1), NS(2), SOA(6), MX(15), TXT(16), AAAA(28)
    rdata.extend(&[0x60, 0x00, 0x00, 0x80, 0x00, 0x01]); // Bitmap

    // Create a mock NSEC record
    let nsec_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::NSEC,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(nsec_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed NSEC record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_nsec = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_nsec.parsed_rdata.is_some());
    let parsed = parsed_nsec.parsed_rdata.as_ref().unwrap();

    // Should start with next domain name
    assert!(parsed.starts_with("next.example.com"));
    // Should contain type numbers
    assert!(parsed.contains(" 1 ")); // A
    assert!(parsed.contains(" 2 ")); // NS
}

#[test]
fn test_nsec3_record_parsing() {
    // Build NSEC3 RDATA
    let mut rdata = Vec::new();

    // Hash Algorithm: 1 (SHA-1)
    rdata.push(1);
    // Flags: 0
    rdata.push(0);
    // Iterations: 10
    rdata.extend(&10u16.to_be_bytes());
    // Salt Length: 8
    rdata.push(8);
    // Salt (8 bytes)
    rdata.extend(&[0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89]);

    // Next Hashed Owner Name Length: 20 (SHA-1 produces 20 bytes)
    rdata.push(20);
    // Next Hashed Owner Name (20 bytes)
    let next_hash = vec![
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10, 0x11, 0x12, 0x13, 0x14,
    ];
    rdata.extend(&next_hash);

    // Type bit maps
    // Window 0
    rdata.push(0); // Window number
    rdata.push(3); // Bitmap length
    rdata.extend(&[0x60, 0x00, 0x01]); // Bitmap for A, NS, AAAA

    // Create a mock NSEC3 record
    let nsec3_record = DNSResource {
        labels: vec![
            "ABCDEFGHIJKLMNOP".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::NSEC3,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(nsec3_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed NSEC3 record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_nsec3 = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_nsec3.parsed_rdata.is_some());
    let parsed = parsed_nsec3.parsed_rdata.as_ref().unwrap();

    // Should have format: "1 0 10 abcdef0123456789 <base32_hash> 1 2 28"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert!(parts.len() >= 6);
    assert_eq!(parts[0], "1"); // Hash Algorithm
    assert_eq!(parts[1], "0"); // Flags
    assert_eq!(parts[2], "10"); // Iterations
    assert_eq!(parts[3], "abcdef0123456789"); // Salt (hex)
    // parts[4] is the base32-encoded next hash
    // remaining parts are the type numbers
}

#[test]
fn test_rebuild_rdata_for_dnskey() {
    use base64::Engine;
    let public_key_base64 =
        base64::engine::general_purpose::STANDARD.encode([0x01, 0x02, 0x03, 0x04]);

    let dnskey_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::DNSKEY,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some(format!("257 3 8 {}", public_key_base64)),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(dnskey_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_dnskey = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_dnskey.parsed_rdata.as_ref().unwrap(),
        &format!("257 3 8 {}", public_key_base64)
    );
}

#[test]
fn test_rebuild_rdata_for_ds() {
    let digest_hex = hex::encode([0xAB, 0xCD, 0xEF, 0x01]);

    let ds_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::DS,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some(format!("12345 8 2 {}", digest_hex)),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(ds_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_ds = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_ds.parsed_rdata.as_ref().unwrap(),
        &format!("12345 8 2 {}", digest_hex)
    );
}
