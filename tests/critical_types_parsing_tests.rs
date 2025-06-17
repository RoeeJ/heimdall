mod common;
use common::*;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    resource::DNSResource,
};

#[test]
fn test_tlsa_record_parsing() {
    // Build TLSA RDATA: Certificate Usage=3 (DANE-EE), Selector=1 (SPKI), Matching Type=1 (SHA-256)
    let mut rdata = Vec::new();

    // Certificate Usage: 3 (DANE-EE - End Entity)
    rdata.push(3);
    // Selector: 1 (SubjectPublicKeyInfo)
    rdata.push(1);
    // Matching Type: 1 (SHA-256)
    rdata.push(1);

    // Mock certificate hash (32 bytes for SHA-256)
    let cert_hash = vec![
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
        0x07, 0x08,
    ];
    rdata.extend(&cert_hash);

    // Create a mock TLSA record
    let tlsa_record = DNSResource {
        labels: vec![
            "_443".to_string(),
            "_tcp".to_string(),
            "www".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::TLSA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(tlsa_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed TLSA record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_tlsa = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_tlsa.parsed_rdata.is_some());
    let parsed = parsed_tlsa.parsed_rdata.as_ref().unwrap();

    // Should have format: "3 1 1 <hex_hash>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0], "3"); // Certificate Usage
    assert_eq!(parts[1], "1"); // Selector
    assert_eq!(parts[2], "1"); // Matching Type
    assert_eq!(parts[3].len(), 64); // SHA-256 hash should be 64 hex chars

    // Test helper method
    let fields = parsed_tlsa.get_tlsa_fields().unwrap();
    assert_eq!(fields.0, 3); // cert usage
    assert_eq!(fields.1, 1); // selector
    assert_eq!(fields.2, 1); // matching type
    assert_eq!(fields.3.len(), 64); // certificate hash hex
}

#[test]
fn test_tlsa_full_certificate_parsing() {
    // Test TLSA with full certificate instead of hash (Matching Type = 0)
    let mut rdata = Vec::new();

    // Certificate Usage: 1 (PKIX-TA)
    rdata.push(1);
    // Selector: 0 (Full certificate)
    rdata.push(0);
    // Matching Type: 0 (No hash, full data)
    rdata.push(0);

    // Mock certificate data (shortened for test)
    let cert_data = vec![
        0x30, 0x82, 0x01, 0x0A, 0x02, 0x82, 0x01, 0x01, 0x00, 0xC4, 0xA6, 0xB1, 0xA4, 0x7F, 0x2C,
        0x4B,
    ];
    rdata.extend(&cert_data);

    // Create a mock TLSA record
    let tlsa_record = DNSResource {
        labels: vec![
            "_25".to_string(),
            "_tcp".to_string(),
            "mail".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::TLSA,
        rclass: DNSResourceClass::IN,
        ttl: 7200,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(tlsa_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed TLSA record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_tlsa = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_tlsa.parsed_rdata.is_some());
    let parsed = parsed_tlsa.parsed_rdata.as_ref().unwrap();

    // Should have format: "1 0 0 <hex_cert>"
    assert!(parsed.starts_with("1 0 0 "));
    assert!(parsed.contains("308201")); // DER certificate marker
}

#[test]
fn test_sshfp_record_parsing() {
    // Build SSHFP RDATA: Algorithm=1 (RSA), Type=1 (SHA-1), Fingerprint
    let mut rdata = Vec::new();

    // Algorithm: 1 (RSA)
    rdata.push(1);
    // Fingerprint Type: 1 (SHA-1)
    rdata.push(1);

    // Mock fingerprint (20 bytes for SHA-1)
    let fingerprint = vec![
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xAA, 0xBB, 0xCC,
    ];
    rdata.extend(&fingerprint);

    // Create a mock SSHFP record
    let sshfp_record = DNSResource {
        labels: vec!["host".to_string(), "example".to_string(), "com".to_string()],
        rtype: DNSResourceType::SSHFP,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(sshfp_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed SSHFP record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_sshfp = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_sshfp.parsed_rdata.is_some());
    let parsed = parsed_sshfp.parsed_rdata.as_ref().unwrap();

    // Should have format: "1 1 <hex_fingerprint>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], "1"); // Algorithm (RSA)
    assert_eq!(parts[1], "1"); // Fingerprint Type (SHA-1)
    assert_eq!(parts[2].len(), 40); // SHA-1 fingerprint should be 40 hex chars

    // Test helper method
    let fields = parsed_sshfp.get_sshfp_fields().unwrap();
    assert_eq!(fields.0, 1); // algorithm
    assert_eq!(fields.1, 1); // fingerprint type
    assert_eq!(fields.2.len(), 40); // fingerprint hex
}

#[test]
fn test_sshfp_sha256_parsing() {
    // Test SSHFP with SHA-256 fingerprint
    let mut rdata = Vec::new();

    // Algorithm: 2 (DSA)
    rdata.push(2);
    // Fingerprint Type: 2 (SHA-256)
    rdata.push(2);

    // Mock fingerprint (32 bytes for SHA-256)
    let fingerprint = vec![
        0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67,
        0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45,
        0x67, 0x89,
    ];
    rdata.extend(&fingerprint);

    // Create a mock SSHFP record
    let sshfp_record = DNSResource {
        labels: vec![
            "server".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::SSHFP,
        rclass: DNSResourceClass::IN,
        ttl: 7200,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(sshfp_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed SSHFP record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_sshfp = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_sshfp.parsed_rdata.is_some());
    let parsed = parsed_sshfp.parsed_rdata.as_ref().unwrap();

    // Should have format: "2 2 <hex_fingerprint>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], "2"); // Algorithm (DSA)
    assert_eq!(parts[1], "2"); // Fingerprint Type (SHA-256)
    assert_eq!(parts[2].len(), 64); // SHA-256 fingerprint should be 64 hex chars
}

#[test]
fn test_sshfp_ecdsa_parsing() {
    // Test SSHFP with ECDSA key
    let mut rdata = Vec::new();

    // Algorithm: 3 (ECDSA)
    rdata.push(3);
    // Fingerprint Type: 2 (SHA-256)
    rdata.push(2);

    // Mock fingerprint (32 bytes for SHA-256)
    let fingerprint = vec![
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD,
        0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
        0xCD, 0xEF,
    ];
    rdata.extend(&fingerprint);

    // Create a mock SSHFP record
    let sshfp_record = DNSResource {
        labels: vec![
            "ecdsa-host".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::SSHFP,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(sshfp_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed SSHFP record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_sshfp = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_sshfp.parsed_rdata.is_some());
    let parsed = parsed_sshfp.parsed_rdata.as_ref().unwrap();

    // Should have format: "3 2 <hex_fingerprint>"
    let parts: Vec<&str> = parsed.split(' ').collect();
    assert_eq!(parts[0], "3"); // Algorithm (ECDSA)
    assert_eq!(parts[1], "2"); // Fingerprint Type (SHA-256)
}

#[test]
fn test_rebuild_rdata_for_tlsa() {
    let cert_data_hex = hex::encode([0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE]);

    let tlsa_record = DNSResource {
        labels: vec![
            "_443".to_string(),
            "_tcp".to_string(),
            "www".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::TLSA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some(format!("3 1 1 {}", cert_data_hex)),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(tlsa_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_tlsa = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_tlsa.parsed_rdata.as_ref().unwrap(),
        &format!("3 1 1 {}", cert_data_hex)
    );
}

#[test]
fn test_rebuild_rdata_for_sshfp() {
    let fingerprint_hex = hex::encode([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);

    let sshfp_record = DNSResource {
        labels: vec!["host".to_string(), "example".to_string(), "com".to_string()],
        rtype: DNSResourceType::SSHFP,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some(format!("1 1 {}", fingerprint_hex)),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(sshfp_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_sshfp = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_sshfp.parsed_rdata.as_ref().unwrap(),
        &format!("1 1 {}", fingerprint_hex)
    );
}
