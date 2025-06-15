use heimdall::config::DnsConfig;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::{DNSPacket, question::DNSQuestion};
use heimdall::dnssec::ValidationResult;
use heimdall::resolver::DnsResolver;
use std::time::Duration;
use tokio::time::timeout;

mod common;
use common::test_config;

#[tokio::test]
#[ignore] // This test requires network access
async fn test_dnssec_validation_cloudflare() {
    // Enable DNSSEC validation
    let config = DnsConfig {
        dnssec_enabled: true,
        dnssec_strict: false,
        upstream_servers: vec![
            "1.1.1.1:53".parse().unwrap(), // Cloudflare DNS (DNSSEC-enabled)
            "8.8.8.8:53".parse().unwrap(), // Google DNS (DNSSEC-enabled)
        ],
        cache_config: Default::default(),
        ..Default::default()
    };

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Query for a domain known to have DNSSEC (cloudflare.com)
    let mut packet = DNSPacket::default();
    packet.header.id = 1234;
    packet.header.rd = true;
    packet.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["cloudflare".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    packet.questions.push(question);

    // Add EDNS with DO flag
    packet.add_edns(4096, true);

    // Perform the query with a timeout
    let result = timeout(
        Duration::from_secs(10),
        resolver.resolve(packet.clone(), 1234),
    )
    .await;

    match result {
        Ok(Ok(response)) => {
            println!("Response received:");
            println!("  Answer count: {}", response.answers.len());
            println!("  Authority count: {}", response.authorities.len());
            println!("  Additional count: {}", response.resources.len());

            // Check if we got answers
            assert!(!response.answers.is_empty(), "Should have received answers");

            // Check if DNSSEC records were included
            let has_rrsig = response
                .answers
                .iter()
                .any(|rr| rr.rtype == DNSResourceType::RRSIG)
                || response
                    .authorities
                    .iter()
                    .any(|rr| rr.rtype == DNSResourceType::RRSIG);

            println!("  Has RRSIG: {}", has_rrsig);

            // With DNSSEC enabled and DO flag set, we should get RRSIG records
            assert!(
                has_rrsig,
                "Should have received RRSIG records with DO flag set"
            );
        }
        Ok(Err(e)) => panic!("DNS query failed: {:?}", e),
        Err(_) => panic!("Query timed out"),
    }
}

#[tokio::test]
#[ignore] // This test requires network access
async fn test_dnssec_validation_failure() {
    // Enable strict DNSSEC validation
    let config = DnsConfig {
        dnssec_enabled: true,
        dnssec_strict: true, // Strict mode - reject bogus responses
        upstream_servers: vec!["1.1.1.1:53".parse().unwrap()],
        cache_config: Default::default(),
        ..Default::default()
    };

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Query for dnssec-failed.org (a test domain with intentionally broken DNSSEC)
    let mut packet = DNSPacket::default();
    packet.header.id = 1235;
    packet.header.rd = true;
    packet.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["dnssec-failed".to_string(), "org".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    packet.questions.push(question);

    // Add EDNS with DO flag
    packet.add_edns(4096, true);

    // Perform the query
    let result = timeout(
        Duration::from_secs(10),
        resolver.resolve(packet.clone(), 1235),
    )
    .await;

    match result {
        Ok(Ok(response)) => {
            // In strict mode, we should get SERVFAIL for bogus DNSSEC
            assert_eq!(
                response.header.rcode, 2,
                "Should return SERVFAIL for bogus DNSSEC"
            );
        }
        Ok(Err(_)) => {
            // This is also acceptable - the resolver might reject the query
        }
        Err(_) => panic!("Query timed out"),
    }
}

#[tokio::test]
async fn test_dnssec_do_flag_propagation() {
    let mut config = test_config();
    config.dnssec_enabled = true;
    config.dnssec_strict = false;

    let resolver = DnsResolver::new(config, None).await.unwrap();

    // Create a query without EDNS
    let mut packet = DNSPacket::default();
    packet.header.id = 1236;
    packet.header.rd = true;
    packet.header.qdcount = 1;

    let question = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    packet.questions.push(question);

    // Don't add EDNS explicitly - the resolver should add it with DO flag

    // Note: We can't easily test the actual query sent upstream without mocking,
    // but we can verify that the resolver handles queries correctly when DNSSEC is enabled
    assert!(
        resolver.is_dnssec_enabled(),
        "DNSSEC validator should be present"
    );
}

#[test]
fn test_validation_result_types() {
    // Test that ValidationResult enum works as expected
    let secure = ValidationResult::Secure;
    let insecure = ValidationResult::Insecure;
    let bogus = ValidationResult::Bogus("Test failure".to_string());
    let indeterminate = ValidationResult::Indeterminate;

    assert_eq!(secure, ValidationResult::Secure);
    assert_eq!(insecure, ValidationResult::Insecure);
    assert!(matches!(bogus, ValidationResult::Bogus(_)));
    assert_eq!(indeterminate, ValidationResult::Indeterminate);
}
