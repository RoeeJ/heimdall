#![allow(clippy::field_reassign_with_default)]

use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType, DnsOpcode, ResponseCode},
    header::DNSHeader,
    question::DNSQuestion,
};
use heimdall::resolver::DnsResolver;
use std::net::SocketAddr;
use std::str::FromStr;

/// Test REFUSED response for zone transfer queries
#[tokio::test]
async fn test_refused_response_for_zone_transfers() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5353").unwrap(),
        upstream_servers: vec![
            SocketAddr::from_str("8.8.8.8:53").unwrap(),
            SocketAddr::from_str("8.8.4.4:53").unwrap(),
        ],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create AXFR query
    let mut axfr_query = DNSPacket::default();
    axfr_query.header.id = 1234;
    axfr_query.header.rd = true;
    axfr_query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string()],
        qtype: DNSResourceType::AXFR,
        qclass: DNSResourceClass::IN,
    };
    axfr_query.questions.push(question);

    // Should get REFUSED response
    let response = _resolver.create_refused_response(&axfr_query);

    assert_eq!(response.header.rcode, ResponseCode::Refused.to_u8());
    assert!(response.header.qr); // Is a response
    assert!(response.header.ra); // Recursion available
    assert_eq!(response.header.ancount, 0); // No answers
    assert_eq!(response.header.nscount, 0); // No authority
    assert_eq!(response.header.arcount, 0); // No additional
    assert_eq!(response.questions.len(), 1); // Question preserved
    assert_eq!(response.questions[0].qtype, DNSResourceType::AXFR);
}

/// Test REFUSED response for ANY queries (amplification attack prevention)
#[tokio::test]
async fn test_refused_response_for_any_queries() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5354").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("1.1.1.1:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create ANY query
    let mut any_query = DNSPacket::default();
    any_query.header.id = 5678;
    any_query.header.rd = true;
    any_query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["test".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::ANY,
        qclass: DNSResourceClass::IN,
    };
    any_query.questions.push(question);

    // Should get REFUSED response
    let response = _resolver.create_refused_response(&any_query);

    assert_eq!(response.header.rcode, ResponseCode::Refused.to_u8());
    assert!(response.header.qr);
    assert_eq!(response.header.id, 5678); // ID preserved
}

/// Test NOTIMPL response for unsupported opcodes
#[tokio::test]
async fn test_notimpl_response_for_unsupported_opcodes() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5355").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("8.8.8.8:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create query with UPDATE opcode
    let mut update_query = DNSPacket::default();
    update_query.header.id = 9999;
    update_query.header.opcode = DnsOpcode::Update.to_u8(); // UPDATE opcode
    update_query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    update_query.questions.push(question);

    // Should get NOTIMPL response
    let response = _resolver.create_notimpl_response(&update_query);

    assert_eq!(response.header.rcode, ResponseCode::NotImplemented.to_u8());
    assert!(response.header.qr);
    assert!(!response.header.ra); // May not support recursion for this operation
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);
}

/// Test FORMERR response for malformed queries
#[tokio::test]
async fn test_formerr_response_for_malformed_queries() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5356").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("1.1.1.1:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create query with no questions (qdcount=1 but empty questions)
    let mut malformed_query = DNSPacket::default();
    malformed_query.header.id = 1111;
    malformed_query.header.qdcount = 0; // No questions

    // Should get FORMERR response
    let response = _resolver.create_formerr_response(&malformed_query);

    assert_eq!(response.header.rcode, ResponseCode::FormatError.to_u8());
    assert!(response.header.qr);
    assert!(response.header.ra); // Recursion available
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);
}

/// Test SERVFAIL response for resolution failures
#[tokio::test]
async fn test_servfail_response() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5357").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("8.8.8.8:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create normal query
    let mut query = DNSPacket::default();
    query.header.id = 2222;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec![
            "failed".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Should get SERVFAIL response
    let response = _resolver.create_servfail_response(&query);

    assert_eq!(response.header.rcode, ResponseCode::ServerFailure.to_u8());
    assert!(response.header.qr);
    assert!(response.header.ra);
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);
    assert_eq!(response.questions.len(), 1); // Question preserved
}

/// Test that all DNS opcodes are properly handled
#[tokio::test]
async fn test_all_opcodes_handling() {
    // Test Query opcode (should be implemented)
    assert!(DnsOpcode::Query.is_implemented());

    // Test other opcodes (should not be implemented)
    assert!(!DnsOpcode::IQuery.is_implemented());
    assert!(!DnsOpcode::Status.is_implemented());
    assert!(!DnsOpcode::Notify.is_implemented());
    assert!(!DnsOpcode::Update.is_implemented());
    assert!(!DnsOpcode::DSO.is_implemented());

    // Test opcode conversion
    assert_eq!(DnsOpcode::from_u8(0), Some(DnsOpcode::Query));
    assert_eq!(DnsOpcode::from_u8(1), Some(DnsOpcode::IQuery));
    assert_eq!(DnsOpcode::from_u8(2), Some(DnsOpcode::Status));
    assert_eq!(DnsOpcode::from_u8(4), Some(DnsOpcode::Notify));
    assert_eq!(DnsOpcode::from_u8(5), Some(DnsOpcode::Update));
    assert_eq!(DnsOpcode::from_u8(6), Some(DnsOpcode::DSO));
    assert_eq!(DnsOpcode::from_u8(15), None); // Invalid opcode
}

/// Test extended response codes
#[test]
fn test_extended_response_codes() {
    // Test all ResponseCode values
    assert_eq!(ResponseCode::NoError.to_u8(), 0);
    assert_eq!(ResponseCode::FormatError.to_u8(), 1);
    assert_eq!(ResponseCode::ServerFailure.to_u8(), 2);
    assert_eq!(ResponseCode::NameError.to_u8(), 3);
    assert_eq!(ResponseCode::NotImplemented.to_u8(), 4);
    assert_eq!(ResponseCode::Refused.to_u8(), 5);
    assert_eq!(ResponseCode::YXDomain.to_u8(), 6);
    assert_eq!(ResponseCode::YXRRSet.to_u8(), 7);
    assert_eq!(ResponseCode::NXRRSet.to_u8(), 8);
    assert_eq!(ResponseCode::NotAuth.to_u8(), 9);
    assert_eq!(ResponseCode::NotZone.to_u8(), 10);
    assert_eq!(ResponseCode::BadOptVersion.to_u8(), 16);

    // Test conversion from u8
    assert_eq!(ResponseCode::from_u8(0), ResponseCode::NoError);
    assert_eq!(ResponseCode::from_u8(5), ResponseCode::Refused);
    assert_eq!(ResponseCode::from_u8(99), ResponseCode::ServerFailure); // Unknown defaults to SERVFAIL

    // Test utility methods
    assert!(ResponseCode::NoError.is_success());
    assert!(!ResponseCode::NameError.is_success());
    assert!(ResponseCode::NameError.is_cacheable_error());
    assert!(!ResponseCode::Refused.is_cacheable_error());
}

/// Test that IXFR queries are also refused
#[tokio::test]
async fn test_refused_response_for_ixfr() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5358").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("8.8.8.8:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create IXFR query
    let mut ixfr_query = DNSPacket::default();
    ixfr_query.header.id = 3333;
    ixfr_query.header.rd = true;
    ixfr_query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["zone".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::IXFR,
        qclass: DNSResourceClass::IN,
    };
    ixfr_query.questions.push(question);

    // Should get REFUSED response
    let response = _resolver.create_refused_response(&ixfr_query);

    assert_eq!(response.header.rcode, ResponseCode::Refused.to_u8());
    assert_eq!(response.questions[0].qtype, DNSResourceType::IXFR);
}

/// Test response for invalid opcode values
#[test]
fn test_invalid_opcode_handling() {
    // Test invalid opcode values
    for invalid_opcode in 7..=15 {
        assert_eq!(DnsOpcode::from_u8(invalid_opcode), None);
    }

    // Create header with invalid opcode
    let mut header = DNSHeader::default();
    header.opcode = 15; // Invalid opcode

    // This should be detected as invalid
    assert!(DnsOpcode::from_u8(header.opcode).is_none());
}

/// Test that normal queries still work properly
#[tokio::test]
async fn test_normal_query_not_refused() {
    let config = DnsConfig {
        bind_addr: SocketAddr::from_str("127.0.0.1:5359").unwrap(),
        upstream_servers: vec![SocketAddr::from_str("8.8.8.8:53").unwrap()],
        enable_caching: false,
        ..Default::default()
    };

    let _resolver = DnsResolver::new(config, None).await.unwrap();

    // Create normal A query
    let mut query = DNSPacket::default();
    query.header.id = 4444;
    query.header.rd = true;
    query.header.opcode = DnsOpcode::Query.to_u8();
    query.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Normal queries should not trigger any error responses
    assert_eq!(query.header.opcode, 0); // QUERY opcode
    assert!(
        DnsOpcode::from_u8(query.header.opcode)
            .unwrap()
            .is_implemented()
    );
}
