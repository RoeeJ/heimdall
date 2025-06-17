#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::bool_assert_comparison)]

mod common;
use common::*;
use heimdall::config::DnsConfig;
use heimdall::dns::enums::{DNSResourceType, ResponseCode};
use heimdall::resolver::DnsResolver;

/// Create a test resolver instance  
async fn create_test_resolver_no_arc() -> DnsResolver {
    let config = DnsConfig::default();
    DnsResolver::new(config, None).await.unwrap()
}

#[tokio::test]
async fn test_refused_response() {
    let resolver = create_test_resolver_no_arc().await;
    let query = create_test_query_with_opcode(12345, 0, "example.com", DNSResourceType::A);

    let response = resolver.create_refused_response(&query);

    // Verify response headers
    assert_eq!(response.header.id, 12345);
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, true);
    assert_eq!(response.header.rcode, ResponseCode::Refused.to_u8());
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);

    // Verify question is preserved but no answers
    assert_eq!(response.questions.len(), 1);
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 0);
    assert_eq!(response.resources.len(), 0);

    // Verify question content
    assert_eq!(response.questions[0].labels, vec!["example", "com"]);
    assert_eq!(response.questions[0].qtype, DNSResourceType::A);
}

#[tokio::test]
async fn test_notimpl_response() {
    let resolver = create_test_resolver_no_arc().await;
    let query = create_test_query_with_opcode(54321, 1, "example.com", DNSResourceType::A); // IQUERY opcode

    let response = resolver.create_notimpl_response(&query);

    // Verify response headers
    assert_eq!(response.header.id, 54321);
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, false); // May not support recursion for unsupported operations
    assert_eq!(response.header.rcode, ResponseCode::NotImplemented.to_u8());
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);

    // Verify question is preserved but no answers
    assert_eq!(response.questions.len(), 1);
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 0);
    assert_eq!(response.resources.len(), 0);
}

#[tokio::test]
async fn test_formerr_response() {
    let resolver = create_test_resolver_no_arc().await;
    let mut query = create_test_query_with_opcode(9876, 0, "example.com", DNSResourceType::A);

    // Make the query malformed by setting invalid counters
    query.header.qdcount = 0; // No questions but questions array is not empty

    let response = resolver.create_formerr_response(&query);

    // Verify response headers
    assert_eq!(response.header.id, 9876);
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, true);
    assert_eq!(response.header.rcode, ResponseCode::FormatError.to_u8());
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.header.arcount, 0);

    // Verify all sections are cleared
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 0);
    assert_eq!(response.resources.len(), 0);
}

#[tokio::test]
async fn test_existing_servfail_response_uses_enum() {
    let resolver = create_test_resolver_no_arc().await;
    let query = create_test_query_with_opcode(11111, 0, "example.com", DNSResourceType::A);

    let response = resolver.create_servfail_response(&query);

    // Verify response uses ResponseCode enum
    assert_eq!(response.header.rcode, ResponseCode::ServerFailure.to_u8());
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, true);
    assert_eq!(response.header.ancount, 0);
}

#[tokio::test]
async fn test_existing_nxdomain_response_uses_enum() {
    let resolver = create_test_resolver_no_arc().await;
    let query = create_test_query_with_opcode(22222, 0, "nonexistent.example", DNSResourceType::A);

    let response = resolver.create_nxdomain_response(&query);

    // Verify response uses ResponseCode enum
    assert_eq!(response.header.rcode, ResponseCode::NameError.to_u8());
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, true);
    assert_eq!(response.header.ancount, 0);
}

#[tokio::test]
async fn test_response_code_enum_functionality() {
    // Test ResponseCode enum methods
    assert_eq!(ResponseCode::NoError.to_u8(), 0);
    assert_eq!(ResponseCode::FormatError.to_u8(), 1);
    assert_eq!(ResponseCode::ServerFailure.to_u8(), 2);
    assert_eq!(ResponseCode::NameError.to_u8(), 3);
    assert_eq!(ResponseCode::NotImplemented.to_u8(), 4);
    assert_eq!(ResponseCode::Refused.to_u8(), 5);

    // Test from_u8 conversion
    assert_eq!(ResponseCode::from_u8(0), ResponseCode::NoError);
    assert_eq!(ResponseCode::from_u8(1), ResponseCode::FormatError);
    assert_eq!(ResponseCode::from_u8(3), ResponseCode::NameError);
    assert_eq!(ResponseCode::from_u8(4), ResponseCode::NotImplemented);
    assert_eq!(ResponseCode::from_u8(5), ResponseCode::Refused);
    assert_eq!(ResponseCode::from_u8(255), ResponseCode::ServerFailure); // Unknown codes default to SERVFAIL

    // Test helper methods
    assert!(ResponseCode::NoError.is_success());
    assert!(!ResponseCode::ServerFailure.is_success());
    assert!(ResponseCode::NameError.is_cacheable_error());
    assert!(!ResponseCode::ServerFailure.is_cacheable_error());

    // Test descriptions
    assert_eq!(ResponseCode::NoError.description(), "No error");
    assert_eq!(
        ResponseCode::NameError.description(),
        "Name error (NXDOMAIN)"
    );
    assert_eq!(
        ResponseCode::NotImplemented.description(),
        "Not implemented"
    );
    assert_eq!(ResponseCode::Refused.description(), "Refused");
}

#[tokio::test]
async fn test_response_serialization() {
    let resolver = create_test_resolver_no_arc().await;
    let query = create_test_query_with_opcode(33333, 0, "test.example", DNSResourceType::A);

    // Test that all new response types can be serialized
    let refused = resolver.create_refused_response(&query);
    let notimpl = resolver.create_notimpl_response(&query);
    let formerr = resolver.create_formerr_response(&query);

    // Should not panic
    let _refused_bytes = refused
        .serialize()
        .expect("REFUSED response should serialize");
    let _notimpl_bytes = notimpl
        .serialize()
        .expect("NOTIMPL response should serialize");
    let _formerr_bytes = formerr
        .serialize()
        .expect("FORMERR response should serialize");

    // Verify the serialized responses have correct lengths (at least header + question)
    let query_bytes = query.serialize().expect("Query should serialize");
    assert!(_refused_bytes.len() >= 12); // DNS header is 12 bytes minimum
    assert!(_notimpl_bytes.len() >= 12);
    assert!(_formerr_bytes.len() >= 12);

    // Response should be at least as long as the original query (header + question)
    assert!(_refused_bytes.len() >= query_bytes.len());
    assert!(_notimpl_bytes.len() >= query_bytes.len());
    assert!(_formerr_bytes.len() >= query_bytes.len());
}
