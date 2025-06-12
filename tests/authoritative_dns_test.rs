use heimdall::config::DnsConfig;
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType, ResponseCode},
    question::DNSQuestion,
};
use heimdall::resolver::DnsResolver;
use std::fs;

#[tokio::test]
async fn test_authoritative_response() {
    // Create a temporary zone file
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test.example.com.zone");

    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
@   IN  A   192.0.2.1
www IN  A   192.0.2.2
"#;

    fs::write(&zone_file_path, zone_content).unwrap();

    // Create config with authoritative enabled and zone file
    let config = DnsConfig {
        authoritative_enabled: true,
        zone_files: vec![zone_file_path.to_string_lossy().to_string()],
        ..Default::default()
    };

    // Create resolver
    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Create a query for www.example.com
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.header.rd = true;
    query.questions.push(DNSQuestion {
        labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    });

    // Resolve the query
    let response = resolver.resolve(query, 1234).await.unwrap();

    // Verify response
    assert_eq!(response.header.id, 1234);
    assert!(response.header.qr); // Response flag
    assert!(response.header.aa); // Authoritative answer
    assert_eq!(response.header.rcode, ResponseCode::NoError as u8);
    assert_eq!(response.answers.len(), 1);
    assert_eq!(
        response.answers[0].parsed_rdata.as_ref().unwrap(),
        "192.0.2.2"
    );

    // Clean up
    fs::remove_file(zone_file_path).ok();
}

#[tokio::test]
async fn test_authoritative_nxdomain() {
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test-nxdomain.example.com.zone");

    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
"#;

    fs::write(&zone_file_path, zone_content).unwrap();

    let config = DnsConfig {
        authoritative_enabled: true,
        zone_files: vec![zone_file_path.to_string_lossy().to_string()],
        ..Default::default()
    };

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Query for non-existent domain
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.header.rd = true;
    query.questions.push(DNSQuestion {
        labels: vec![
            "nonexistent".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    });

    let response = resolver.resolve(query, 1234).await.unwrap();

    // Verify NXDOMAIN response
    assert_eq!(response.header.rcode, ResponseCode::NameError as u8);
    assert!(response.header.aa); // Authoritative
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 1); // SOA in authority section
    assert_eq!(response.authorities[0].rtype, DNSResourceType::SOA);

    fs::remove_file(zone_file_path).ok();
}

#[tokio::test]
async fn test_authoritative_nodata() {
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test-nodata.example.com.zone");

    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
www IN  A   192.0.2.1
"#;

    fs::write(&zone_file_path, zone_content).unwrap();

    let config = DnsConfig {
        authoritative_enabled: true,
        zone_files: vec![zone_file_path.to_string_lossy().to_string()],
        ..Default::default()
    };

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Query for AAAA record (which doesn't exist)
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.questions.push(DNSQuestion {
        labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::AAAA,
        qclass: DNSResourceClass::IN,
    });

    let response = resolver.resolve(query, 1234).await.unwrap();

    // Verify NODATA response (NoError with no answer)
    assert_eq!(response.header.rcode, ResponseCode::NoError as u8);
    assert!(response.header.aa);
    assert_eq!(response.answers.len(), 0);
    assert_eq!(response.authorities.len(), 1); // SOA in authority

    fs::remove_file(zone_file_path).ok();
}

#[tokio::test]
async fn test_delegation_response() {
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test-delegation.example.com.zone");

    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Delegation for subdomain
sub IN  NS  ns1.sub.example.com.
sub IN  NS  ns2.sub.example.com.
"#;

    fs::write(&zone_file_path, zone_content).unwrap();

    let config = DnsConfig {
        authoritative_enabled: true,
        zone_files: vec![zone_file_path.to_string_lossy().to_string()],
        ..Default::default()
    };

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Query for something in the delegated subdomain
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.questions.push(DNSQuestion {
        labels: vec![
            "www".to_string(),
            "sub".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    });

    let response = resolver.resolve(query, 1234).await.unwrap();

    // Verify delegation response
    assert_eq!(response.header.rcode, ResponseCode::NoError as u8);
    assert!(!response.header.aa); // NOT authoritative for delegated zone
    assert_eq!(response.answers.len(), 0);
    assert!(!response.authorities.is_empty()); // NS records in authority
    assert_eq!(response.authorities[0].rtype, DNSResourceType::NS);

    fs::remove_file(zone_file_path).ok();
}
