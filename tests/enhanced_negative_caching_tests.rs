#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::bool_assert_comparison)]

use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType, ResponseCode},
    header::DNSHeader,
    question::DNSQuestion,
};
use heimdall::resolver::DnsResolver;

/// Create a test DNS query packet
fn create_test_query(id: u16, domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let mut header = DNSHeader::default();
    header.id = id;
    header.opcode = 0;
    header.qr = false;
    header.rd = true;
    header.qdcount = 1;

    let question = DNSQuestion {
        labels: domain.split('.').map(|s| s.to_string()).collect(),
        qtype,
        qclass: DNSResourceClass::IN,
    };

    DNSPacket {
        header,
        questions: vec![question],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

#[tokio::test]
async fn test_nxdomain_response_includes_soa() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(12345, "nonexistent.example.com", DNSResourceType::A);

    let response = resolver.create_nxdomain_response(&query);

    // Verify response headers
    assert_eq!(response.header.id, 12345);
    assert_eq!(response.header.qr, true);
    assert_eq!(response.header.ra, true);
    assert_eq!(response.header.rcode, ResponseCode::NameError.to_u8());
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.header.nscount, 1); // Should have SOA in authority section
    assert_eq!(response.header.arcount, 0);

    // Verify sections
    assert_eq!(response.questions.len(), 1);
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 1); // Should have SOA record
    assert_eq!(response.resources.len(), 0);

    // Verify SOA record
    let soa_record = &response.authorities[0];
    assert_eq!(soa_record.rtype, DNSResourceType::SOA);
    assert_eq!(soa_record.rclass, DNSResourceClass::IN);
    assert_eq!(soa_record.labels, vec!["example", "com"]); // Should be for example.com
    assert!(soa_record.ttl > 0);
    assert!(soa_record.rdlength > 0);
    assert!(!soa_record.rdata.is_empty());
}

#[tokio::test]
async fn test_nxdomain_response_with_single_label() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(54321, "com", DNSResourceType::A);

    let response = resolver.create_nxdomain_response(&query);

    // Verify response includes SOA record
    assert_eq!(response.header.nscount, 1);
    assert_eq!(response.authorities.len(), 1);

    let soa_record = &response.authorities[0];
    assert_eq!(soa_record.rtype, DNSResourceType::SOA);
    assert_eq!(soa_record.labels, vec!["com"]); // Should be for .com
}

#[tokio::test]
async fn test_nxdomain_response_with_subdomain() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(9876, "www.nonexistent.example.com", DNSResourceType::AAAA);

    let response = resolver.create_nxdomain_response(&query);

    // Verify response includes SOA record for the parent domain
    assert_eq!(response.header.nscount, 1);
    assert_eq!(response.authorities.len(), 1);

    let soa_record = &response.authorities[0];
    assert_eq!(soa_record.rtype, DNSResourceType::SOA);
    // Should create SOA for example.com (last two labels)
    assert_eq!(soa_record.labels, vec!["example", "com"]);
}

#[tokio::test]
async fn test_nxdomain_response_with_empty_questions() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let mut query = create_test_query(11111, "example.com", DNSResourceType::A);

    // Clear questions to test edge case
    query.questions.clear();
    query.header.qdcount = 0;

    let response = resolver.create_nxdomain_response(&query);

    // Should handle gracefully without SOA
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.authorities.len(), 0);
    assert_eq!(response.header.rcode, ResponseCode::NameError.to_u8());
}

#[tokio::test]
async fn test_soa_rdata_format() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(22222, "test.example.org", DNSResourceType::MX);

    let response = resolver.create_nxdomain_response(&query);

    // Verify SOA record has proper rdata
    assert_eq!(response.authorities.len(), 1);
    let soa_record = &response.authorities[0];

    // SOA rdata should contain:
    // - MNAME (domain name)
    // - RNAME (email)
    // - Serial (4 bytes)
    // - Refresh (4 bytes)
    // - Retry (4 bytes)
    // - Expire (4 bytes)
    // - Minimum (4 bytes)
    // Minimum expected size: domain names + 20 bytes for numbers
    assert!(
        soa_record.rdata.len() >= 30,
        "SOA rdata too small: {} bytes",
        soa_record.rdata.len()
    );
    assert_eq!(soa_record.rdlength, soa_record.rdata.len() as u16);

    // Verify the SOA can be serialized (basic format check)
    let serialized = response.serialize().expect("Response should serialize");
    assert!(serialized.len() > 12); // At least header size
}

#[tokio::test]
async fn test_servfail_response_no_soa() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(33333, "server-error.example.com", DNSResourceType::A);

    let response = resolver.create_servfail_response(&query);

    // SERVFAIL responses should NOT include SOA records
    assert_eq!(response.header.rcode, ResponseCode::ServerFailure.to_u8());
    assert_eq!(response.header.nscount, 0);
    assert_eq!(response.authorities.len(), 0);
    assert_eq!(response.header.ancount, 0);
    assert_eq!(response.answers.len(), 0);
}

#[tokio::test]
async fn test_soa_ttl_for_negative_caching() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();
    let query = create_test_query(44444, "cached.example.net", DNSResourceType::TXT);

    let response = resolver.create_nxdomain_response(&query);

    // Verify SOA TTL is reasonable for negative caching
    assert_eq!(response.authorities.len(), 1);
    let soa_record = &response.authorities[0];

    // TTL should be reasonable (between 1 minute and 1 hour)
    assert!(soa_record.ttl >= 60, "SOA TTL too low: {}", soa_record.ttl);
    assert!(
        soa_record.ttl <= 3600,
        "SOA TTL too high: {}",
        soa_record.ttl
    );
}

#[tokio::test]
async fn test_multiple_label_domain_handling() {
    let resolver = DnsResolver::new(DnsConfig::default(), None).await.unwrap();

    // Test various domain structures
    let test_cases = vec![
        ("a.b.c.d.example.com", vec!["example", "com"]),
        ("very.long.subdomain.test.org", vec!["test", "org"]),
        ("x.y", vec!["x", "y"]),
    ];

    for (domain, expected_soa_labels) in test_cases {
        let query = create_test_query(55555, domain, DNSResourceType::A);
        let response = resolver.create_nxdomain_response(&query);

        assert_eq!(
            response.authorities.len(),
            1,
            "Failed for domain: {}",
            domain
        );
        let soa_record = &response.authorities[0];
        assert_eq!(
            soa_record.labels, expected_soa_labels,
            "Wrong SOA labels for domain: {}",
            domain
        );
    }
}
