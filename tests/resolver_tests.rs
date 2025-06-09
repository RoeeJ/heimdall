use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};
use heimdall::resolver::DnsResolver;

fn create_test_query() -> DNSPacket {
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
            labels: vec!["example".to_string(), "com".to_string()],
            qtype: DNSResourceType::A,
            qclass: DNSResourceClass::IN,
        }],
        answers: vec![],
        authorities: vec![],
        resources: vec![],
        edns: None,
    }
}

#[test]
fn test_config_default() {
    let config = DnsConfig::default();
    assert_eq!(config.bind_addr.port(), 1053);
    assert!(!config.upstream_servers.is_empty());
    assert!(config.enable_caching);
    assert_eq!(config.max_retries, 2);
}

#[test]
fn test_config_from_env() {
    // Test with no environment variables (should use defaults)
    let config = DnsConfig::from_env();
    assert_eq!(config.bind_addr.port(), 1053);

    // Test environment variable override
    unsafe {
        std::env::set_var("HEIMDALL_BIND_ADDR", "0.0.0.0:5353");
    }
    let config = DnsConfig::from_env();
    assert_eq!(config.bind_addr.port(), 5353);

    // Clean up
    unsafe {
        std::env::remove_var("HEIMDALL_BIND_ADDR");
    }
}

#[tokio::test]
async fn test_resolver_creation() {
    let config = DnsConfig::default();
    let resolver = DnsResolver::new(config).await;
    assert!(resolver.is_ok());
}

#[test]
fn test_servfail_response() {
    let config = DnsConfig::default();
    let query = create_test_query();

    // We can't easily test the resolver without network access,
    // but we can test the error response generation
    let rt = tokio::runtime::Runtime::new().unwrap();
    let resolver = rt.block_on(async { DnsResolver::new(config).await.unwrap() });

    let servfail = resolver.create_servfail_response(&query);
    assert!(servfail.header.qr); // Response
    assert_eq!(servfail.header.rcode, 2); // SERVFAIL
    assert_eq!(servfail.header.ancount, 0); // No answers
}

#[test]
fn test_nxdomain_response() {
    let config = DnsConfig::default();
    let query = create_test_query();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let resolver = rt.block_on(async { DnsResolver::new(config).await.unwrap() });

    let nxdomain = resolver.create_nxdomain_response(&query);
    assert!(nxdomain.header.qr); // Response
    assert_eq!(nxdomain.header.rcode, 3); // NXDOMAIN
    assert_eq!(nxdomain.header.ancount, 0); // No answers
}

#[test]
fn test_packet_validation() {
    let mut packet = create_test_query();
    assert!(packet.valid());

    // Test invalid packet - wrong question count (requires comprehensive validation)
    packet.header.qdcount = 2; // But we only have 1 question
    assert!(packet.validate_comprehensive(None).is_err());

    // Fix it
    packet.header.qdcount = 1;
    assert!(packet.valid());
    assert!(packet.validate_comprehensive(None).is_ok());

    // Test invalid label length (should be caught by fast validation)
    packet.questions[0].labels[0] = "a".repeat(64); // Too long
    assert!(packet.validate_comprehensive(None).is_err());

    // Reset to valid state
    packet.questions[0].labels[0] = "example".to_string();
    assert!(packet.valid());

    // Test invalid total name length
    packet.questions[0].labels = vec!["a".repeat(63); 5]; // Total > 255
    assert!(packet.validate_comprehensive(None).is_err());
}

#[test]
fn test_packet_serialization_roundtrip() {
    let original = create_test_query();

    // Serialize and deserialize
    let serialized = original.serialize().expect("Failed to serialize");
    let deserialized = DNSPacket::parse(&serialized).expect("Failed to deserialize");

    // Should be identical
    assert_eq!(original.header.id, deserialized.header.id);
    assert_eq!(original.header.qdcount, deserialized.header.qdcount);
    assert_eq!(original.questions.len(), deserialized.questions.len());
    assert_eq!(
        original.questions[0].labels,
        deserialized.questions[0].labels
    );
    assert_eq!(original.questions[0].qtype, deserialized.questions[0].qtype);
    assert_eq!(
        original.questions[0].qclass,
        deserialized.questions[0].qclass
    );
}
