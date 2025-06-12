use heimdall::config::DnsConfig;
use heimdall::resolver::DnsResolver;
use heimdall::dns::{DNSPacket, header::DNSHeader, question::DNSQuestion};
use heimdall::dns::enums::{DNSResourceType, DNSResourceClass};
use std::sync::Arc;

#[tokio::test]
async fn test_dnssec_disabled_by_default() {
    let config = DnsConfig::default();
    assert!(!config.dnssec_enabled);
    assert!(!config.dnssec_strict);
    
    let resolver = DnsResolver::new(config, None).await.unwrap();
    
    // Create a simple query
    let mut packet = DNSPacket::default();
    packet.header.id = 1234;
    packet.header.rd = true;
    packet.header.qdcount = 1;
    
    let question = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    packet.questions.push(question);
    
    // This should work without DNSSEC validation
    // Note: This test requires network access to upstream DNS servers
    // In a real test environment, we'd mock the upstream servers
}

#[tokio::test]
async fn test_dnssec_enabled_configuration() {
    let mut config = DnsConfig::default();
    config.dnssec_enabled = true;
    config.dnssec_strict = false;
    
    let resolver = DnsResolver::new(config, None).await.unwrap();
    
    // The resolver should be created successfully with DNSSEC enabled
    // Actual DNSSEC validation tests would require mock DNS responses
}

#[test]
fn test_dnssec_environment_config() {
    // Test environment variable parsing
    unsafe {
        std::env::set_var("HEIMDALL_DNSSEC_ENABLED", "true");
        std::env::set_var("HEIMDALL_DNSSEC_STRICT", "true");
    }
    
    let config = DnsConfig::from_env().unwrap();
    assert!(config.dnssec_enabled);
    assert!(config.dnssec_strict);
    
    // Clean up
    unsafe {
        std::env::remove_var("HEIMDALL_DNSSEC_ENABLED");
        std::env::remove_var("HEIMDALL_DNSSEC_STRICT");
    }
}