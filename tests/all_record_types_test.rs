#![allow(clippy::field_reassign_with_default)]

use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};
use std::collections::HashSet;

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
fn test_total_supported_record_types() {
    // Collect all supported type numbers
    let mut supported_types = HashSet::new();

    // Test a wide range of type numbers to find all supported ones
    for type_num in 0..=65535u16 {
        let record_type: DNSResourceType = type_num.into();
        if record_type != DNSResourceType::Unknown {
            supported_types.insert(type_num);
        }
    }

    println!(
        "\nTotal supported DNS record types: {}",
        supported_types.len()
    );

    // Group by ranges for better visualization
    let mut core_types = Vec::new(); // 1-255
    let mut extended_types = Vec::new(); // 256-32767
    let mut private_types = Vec::new(); // 32768-65535

    for &type_num in &supported_types {
        if type_num <= 255 {
            core_types.push(type_num);
        } else if type_num <= 32767 {
            extended_types.push(type_num);
        } else {
            private_types.push(type_num);
        }
    }

    core_types.sort();
    extended_types.sort();
    private_types.sort();

    println!("Core types (1-255): {} types", core_types.len());
    println!("Extended types (256-32767): {} types", extended_types.len());
    println!(
        "Private use types (32768-65535): {} types",
        private_types.len()
    );

    // Verify we have at least 65 types as expected
    assert!(
        supported_types.len() >= 65,
        "Expected at least 65 supported types, but found {}",
        supported_types.len()
    );
}

#[test]
fn test_all_types_bidirectional_mapping() {
    // Test that all supported types have proper bidirectional mapping
    let all_types = vec![
        // Original types
        DNSResourceType::A,
        DNSResourceType::NS,
        DNSResourceType::MD,
        DNSResourceType::MF,
        DNSResourceType::CNAME,
        DNSResourceType::SOA,
        DNSResourceType::PTR,
        DNSResourceType::HINFO,
        DNSResourceType::MX,
        DNSResourceType::TXT,
        DNSResourceType::AAAA,
        DNSResourceType::AXFR,
        DNSResourceType::MAILB,
        DNSResourceType::SRV,
        DNSResourceType::SSHFP,
        DNSResourceType::TLSA,
        DNSResourceType::HTTPS,
        DNSResourceType::CAA,
        DNSResourceType::DS,
        DNSResourceType::DNSKEY,
        DNSResourceType::NSEC,
        DNSResourceType::RRSIG,
        DNSResourceType::OPT,
        DNSResourceType::ANY,
        DNSResourceType::IXFR,
        // Phase 1 types
        DNSResourceType::LOC,
        DNSResourceType::NAPTR,
        DNSResourceType::APL,
        DNSResourceType::SPF,
        DNSResourceType::NSEC3,
        DNSResourceType::NSEC3PARAM,
        DNSResourceType::CDNSKEY,
        DNSResourceType::CDS,
        DNSResourceType::SVCB,
        DNSResourceType::SMIMEA,
        DNSResourceType::RP,
        DNSResourceType::AFSDB,
        DNSResourceType::DNAME,
        DNSResourceType::URI,
        // Phase 2 types
        DNSResourceType::KEY,
        DNSResourceType::SIG,
        DNSResourceType::NXT,
        DNSResourceType::DHCID,
        DNSResourceType::IPSECKEY,
        DNSResourceType::HIP,
        DNSResourceType::CSYNC,
        DNSResourceType::ZONEMD,
        DNSResourceType::OPENPGPKEY,
        DNSResourceType::CERT,
        DNSResourceType::KX,
        DNSResourceType::TKEY,
        // Phase 3 types
        DNSResourceType::WKS,
        DNSResourceType::X25,
        DNSResourceType::ISDN,
        DNSResourceType::RT,
        DNSResourceType::NSAP,
        DNSResourceType::NSAPPTR,
        DNSResourceType::PX,
        DNSResourceType::GPOS,
        DNSResourceType::A6,
        DNSResourceType::ATMA,
        DNSResourceType::EID,
        DNSResourceType::NIMLOC,
        DNSResourceType::L32,
        DNSResourceType::L64,
        DNSResourceType::LP,
        DNSResourceType::EUI48,
        DNSResourceType::EUI64,
        DNSResourceType::NID,
        // Phase 4 types
        DNSResourceType::SINK,
        DNSResourceType::NINFO,
        DNSResourceType::RKEY,
        DNSResourceType::TALINK,
        DNSResourceType::NULL,
        DNSResourceType::TSIG,
        DNSResourceType::MINFO,
        DNSResourceType::MB,
        DNSResourceType::MG,
        DNSResourceType::MR,
        DNSResourceType::TA,
        DNSResourceType::DLV,
        DNSResourceType::UNSPEC,
        DNSResourceType::UINFO,
        DNSResourceType::UID,
        DNSResourceType::GID,
    ];

    for record_type in all_types {
        // Convert to u16 and back
        let type_num: u16 = record_type.into();
        let back_to_type: DNSResourceType = type_num.into();

        assert_eq!(
            record_type, back_to_type,
            "Bidirectional mapping failed for {:?} (type number {})",
            record_type, type_num
        );

        // Also test that it can be serialized in a query
        let query = create_query("test.example.com", record_type);
        let serialized = query.serialize();
        assert!(
            serialized.is_ok(),
            "Failed to serialize query with {:?}",
            record_type
        );
    }
}

#[test]
fn test_record_type_coverage_by_category() {
    // Count types by category
    let categories = vec![
        (
            "Core DNS",
            vec![
                DNSResourceType::A,
                DNSResourceType::NS,
                DNSResourceType::CNAME,
                DNSResourceType::SOA,
                DNSResourceType::PTR,
                DNSResourceType::MX,
                DNSResourceType::TXT,
                DNSResourceType::AAAA,
            ],
        ),
        (
            "DNSSEC",
            vec![
                DNSResourceType::DS,
                DNSResourceType::DNSKEY,
                DNSResourceType::NSEC,
                DNSResourceType::RRSIG,
                DNSResourceType::NSEC3,
                DNSResourceType::NSEC3PARAM,
                DNSResourceType::CDNSKEY,
                DNSResourceType::CDS,
                DNSResourceType::KEY,
                DNSResourceType::SIG,
                DNSResourceType::NXT,
            ],
        ),
        (
            "Service Discovery",
            vec![
                DNSResourceType::SRV,
                DNSResourceType::SVCB,
                DNSResourceType::NAPTR,
                DNSResourceType::LOC,
            ],
        ),
        (
            "Mail Related",
            vec![
                DNSResourceType::MX,
                DNSResourceType::SPF,
                DNSResourceType::SMIMEA,
                DNSResourceType::RP,
                DNSResourceType::MINFO,
                DNSResourceType::MB,
                DNSResourceType::MG,
                DNSResourceType::MR,
            ],
        ),
        (
            "Security/Certificates",
            vec![
                DNSResourceType::TLSA,
                DNSResourceType::CAA,
                DNSResourceType::SSHFP,
                DNSResourceType::CERT,
                DNSResourceType::IPSECKEY,
                DNSResourceType::HIP,
                DNSResourceType::OPENPGPKEY,
            ],
        ),
        (
            "Network Infrastructure",
            vec![
                DNSResourceType::WKS,
                DNSResourceType::X25,
                DNSResourceType::ISDN,
                DNSResourceType::RT,
                DNSResourceType::NSAP,
                DNSResourceType::NSAPPTR,
                DNSResourceType::PX,
                DNSResourceType::GPOS,
                DNSResourceType::A6,
                DNSResourceType::ATMA,
            ],
        ),
        (
            "Addressing",
            vec![
                DNSResourceType::EID,
                DNSResourceType::NIMLOC,
                DNSResourceType::L32,
                DNSResourceType::L64,
                DNSResourceType::LP,
                DNSResourceType::EUI48,
                DNSResourceType::EUI64,
                DNSResourceType::NID,
            ],
        ),
        (
            "Zone Management",
            vec![
                DNSResourceType::AXFR,
                DNSResourceType::IXFR,
                DNSResourceType::TSIG,
                DNSResourceType::TKEY,
                DNSResourceType::CSYNC,
                DNSResourceType::ZONEMD,
            ],
        ),
        (
            "Experimental/Future",
            vec![
                DNSResourceType::SINK,
                DNSResourceType::NINFO,
                DNSResourceType::RKEY,
                DNSResourceType::TALINK,
                DNSResourceType::NULL,
                DNSResourceType::TA,
                DNSResourceType::DLV,
                DNSResourceType::UNSPEC,
                DNSResourceType::UINFO,
                DNSResourceType::UID,
                DNSResourceType::GID,
            ],
        ),
    ];

    println!("\nDNS Record Type Coverage by Category:");
    println!("=====================================");

    for (category, types) in categories {
        println!("{}: {} types", category, types.len());
    }
}
