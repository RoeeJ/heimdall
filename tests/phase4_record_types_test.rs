#![allow(clippy::field_reassign_with_default)]

use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};

fn create_query(domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let labels: Vec<String> = domain.split('.').map(|s| s.to_string()).collect();

    DNSPacket {
        header: DNSHeader {
            id: 0x1234,
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
            labels,
            qtype,
            qclass: DNSResourceClass::IN,
        }],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

#[test]
fn test_phase4_advanced_type_conversions() {
    // Test all Phase 4 Advanced & Future record type mappings
    let test_mappings = vec![
        // Experimental/Research
        (40, DNSResourceType::SINK),
        (56, DNSResourceType::NINFO),
        (57, DNSResourceType::RKEY),
        (58, DNSResourceType::TALINK),
        (10, DNSResourceType::NULL),
        // Zone Management
        (250, DNSResourceType::TSIG),
        (14, DNSResourceType::MINFO),
        (7, DNSResourceType::MB),
        (8, DNSResourceType::MG),
        (9, DNSResourceType::MR),
        // Additional Types
        (32768, DNSResourceType::TA),
        (32769, DNSResourceType::DLV),
        (103, DNSResourceType::UNSPEC),
        (100, DNSResourceType::UINFO),
        (101, DNSResourceType::UID),
        (102, DNSResourceType::GID),
    ];

    for (number, expected_type) in test_mappings {
        // Test From<u16> conversion
        let parsed_type: DNSResourceType = number.into();
        assert_eq!(
            parsed_type, expected_type,
            "Failed u16->DNSResourceType conversion for {} ({:?})",
            number, expected_type
        );

        // Test Into<u16> conversion
        let back_to_number: u16 = expected_type.into();
        assert_eq!(
            back_to_number, number,
            "Failed DNSResourceType->u16 conversion for {:?}",
            expected_type
        );
    }
}

#[test]
fn test_phase4_experimental_types() {
    let experimental_types = vec![
        ("sink.example.com", DNSResourceType::SINK),
        ("info.example.com", DNSResourceType::NINFO),
        ("rkey.example.com", DNSResourceType::RKEY),
        ("talink.example.com", DNSResourceType::TALINK),
        ("null.example.com", DNSResourceType::NULL),
    ];

    for (domain, record_type) in experimental_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_phase4_zone_management_types() {
    let zone_types = vec![
        ("tsig.example.com", DNSResourceType::TSIG),
        ("minfo.example.com", DNSResourceType::MINFO),
        ("mb.example.com", DNSResourceType::MB),
        ("mg.example.com", DNSResourceType::MG),
        ("mr.example.com", DNSResourceType::MR),
    ];

    for (domain, record_type) in zone_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_phase4_additional_types() {
    let additional_types = vec![
        ("ta.example.com", DNSResourceType::TA),
        ("dlv.example.com", DNSResourceType::DLV),
        ("unspec.example.com", DNSResourceType::UNSPEC),
        ("uinfo.example.com", DNSResourceType::UINFO),
        ("uid.example.com", DNSResourceType::UID),
        ("gid.example.com", DNSResourceType::GID),
    ];

    for (domain, record_type) in additional_types {
        let query = create_query(domain, record_type);

        // Test serialization
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query for {}",
            record_type,
            domain
        );

        // Test parsing
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query for {}",
            record_type,
            domain
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.questions[0].qtype, record_type);
    }
}

#[test]
fn test_all_phase4_types_serialization() {
    let all_phase4_types = vec![
        // Experimental/Research
        DNSResourceType::SINK,
        DNSResourceType::NINFO,
        DNSResourceType::RKEY,
        DNSResourceType::TALINK,
        DNSResourceType::NULL,
        // Zone Management
        DNSResourceType::TSIG,
        DNSResourceType::MINFO,
        DNSResourceType::MB,
        DNSResourceType::MG,
        DNSResourceType::MR,
        // Additional Types
        DNSResourceType::TA,
        DNSResourceType::DLV,
        DNSResourceType::UNSPEC,
        DNSResourceType::UINFO,
        DNSResourceType::UID,
        DNSResourceType::GID,
    ];

    for record_type in all_phase4_types {
        let query = create_query("test.example.com", record_type);

        // Test that the query can be serialized
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize {:?} query",
            record_type
        );

        // Test that the serialized query can be parsed back
        let parsed = DNSPacket::parse(&serialized.unwrap());
        assert!(
            parsed.is_ok(),
            "Failed to parse back {:?} query",
            record_type
        );

        let parsed_query = parsed.unwrap();
        assert_eq!(parsed_query.header.qdcount, 1);
        assert_eq!(parsed_query.questions.len(), 1);
        assert_eq!(parsed_query.questions[0].qtype, record_type);
        assert_eq!(parsed_query.questions[0].qclass, DNSResourceClass::IN);
    }
}

#[test]
fn test_phase4_record_type_display() {
    // Ensure all new types can be properly displayed/debugged
    let display_tests = vec![
        (DNSResourceType::SINK, "SINK"),
        (DNSResourceType::NINFO, "NINFO"),
        (DNSResourceType::RKEY, "RKEY"),
        (DNSResourceType::TALINK, "TALINK"),
        (DNSResourceType::NULL, "NULL"),
        (DNSResourceType::TSIG, "TSIG"),
        (DNSResourceType::MINFO, "MINFO"),
        (DNSResourceType::MB, "MB"),
        (DNSResourceType::MG, "MG"),
        (DNSResourceType::MR, "MR"),
        (DNSResourceType::TA, "TA"),
        (DNSResourceType::DLV, "DLV"),
        (DNSResourceType::UNSPEC, "UNSPEC"),
        (DNSResourceType::UINFO, "UINFO"),
        (DNSResourceType::UID, "UID"),
        (DNSResourceType::GID, "GID"),
    ];

    for (record_type, expected_name) in display_tests {
        let debug_str = format!("{:?}", record_type);
        assert_eq!(
            debug_str, expected_name,
            "Display name mismatch for {:?}",
            record_type
        );
    }
}

#[test]
fn test_phase4_large_type_numbers() {
    // Test that large type numbers (TA, DLV) work correctly
    let ta_type: DNSResourceType = 32768.into();
    assert_eq!(ta_type, DNSResourceType::TA);

    let dlv_type: DNSResourceType = 32769.into();
    assert_eq!(dlv_type, DNSResourceType::DLV);

    // Test reverse conversion
    let ta_num: u16 = DNSResourceType::TA.into();
    assert_eq!(ta_num, 32768);

    let dlv_num: u16 = DNSResourceType::DLV.into();
    assert_eq!(dlv_num, 32769);
}
