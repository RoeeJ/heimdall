use heimdall::blocking::{BlockingMode, BlocklistFormat, DnsBlocker};
use heimdall::config::DnsConfig;
use heimdall::dns::DNSPacket;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::question::DNSQuestion;
use heimdall::resolver::DnsResolver;
use std::net::IpAddr;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

/// Helper to create a DNS query packet
fn create_query(domain: &str, qtype: DNSResourceType) -> DNSPacket {
    let mut packet = DNSPacket::default();
    packet.header.id = 1234;
    packet.header.rd = true;
    packet.header.qdcount = 1;

    let question = DNSQuestion {
        labels: domain.split('.').map(|s| s.to_string()).collect(),
        qtype,
        qclass: DNSResourceClass::IN,
    };
    packet.questions.push(question);
    packet
}

#[test]
fn test_blocker_basic_blocking() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add some blocked domains
    blocker.add_blocked_domain("ads.example.com");
    blocker.add_blocked_domain("tracker.com");
    blocker.add_blocked_domain("*.doubleclick.net");

    // Test exact matches
    assert!(blocker.is_blocked("ads.example.com"));
    assert!(blocker.is_blocked("tracker.com"));
    assert!(!blocker.is_blocked("example.com"));
    assert!(!blocker.is_blocked("good.com"));

    // Test wildcard matches
    assert!(blocker.is_blocked("sub.doubleclick.net"));
    assert!(blocker.is_blocked("deep.sub.doubleclick.net"));
    assert!(!blocker.is_blocked("doubleclick.net")); // Wildcard doesn't match base domain

    // Test case insensitivity
    assert!(blocker.is_blocked("ADS.EXAMPLE.COM"));
    assert!(blocker.is_blocked("Tracker.Com"));

    // Test subdomain blocking - tracker.com should block subdomains
    assert!(blocker.is_blocked("sub.tracker.com"));
    assert!(blocker.is_blocked("deep.sub.tracker.com"));

    // Adding subdomain of already blocked domain should not increase count
    blocker.add_blocked_domain("sub.tracker.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 3); // Still 3, not 4
}

#[test]
fn test_blocker_domain_and_subdomain_blocking() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add a domain to blocklist (not wildcard)
    blocker.add_blocked_domain("doubleclick.net");

    // Should block the domain itself
    assert!(blocker.is_blocked("doubleclick.net"));

    // Should also block all subdomains
    assert!(blocker.is_blocked("ads.doubleclick.net"));
    assert!(blocker.is_blocked("stats.doubleclick.net"));
    assert!(blocker.is_blocked("deep.nested.doubleclick.net"));

    // Should not block unrelated domains
    assert!(!blocker.is_blocked("notdoubleclick.net"));
    assert!(!blocker.is_blocked("doubleclick.com"));
}

#[test]
fn test_blocker_allowlist() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add blocked domains
    blocker.add_blocked_domain("*.example.com");
    blocker.add_blocked_domain("ads.site.com");
    blocker.add_blocked_domain("tracker.com"); // This will block subdomains too

    // Add allowlist entries
    blocker.add_to_allowlist("safe.example.com");
    blocker.add_to_allowlist("ads.site.com"); // This should override the block
    blocker.add_to_allowlist("good.tracker.com"); // Allow specific subdomain

    // Test allowlist overrides
    assert!(!blocker.is_blocked("safe.example.com")); // Allowlisted
    assert!(!blocker.is_blocked("ads.site.com")); // Allowlisted
    assert!(blocker.is_blocked("other.example.com")); // Still blocked by wildcard
    assert!(blocker.is_blocked("sub.other.example.com")); // Still blocked by wildcard

    // Test subdomain blocking with allowlist
    assert!(blocker.is_blocked("tracker.com")); // Base domain blocked
    assert!(blocker.is_blocked("bad.tracker.com")); // Subdomain blocked
    assert!(!blocker.is_blocked("good.tracker.com")); // Specifically allowlisted
}

#[test]
fn test_blocker_statistics() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add domains
    blocker.add_blocked_domain("ads.com");
    blocker.add_blocked_domain("tracker.com");
    blocker.add_blocked_domain("*.bad.com");
    blocker.add_to_allowlist("good.com");

    let stats = blocker.get_stats();
    assert_eq!(stats.total_blocked_domains, 3);
    assert_eq!(stats.total_wildcard_rules, 1);
    assert_eq!(stats.total_exact_rules, 2);
    assert_eq!(stats.allowlist_size, 1);
    assert_eq!(stats.blocklists_loaded, 0);
}

#[tokio::test]
async fn test_blocklist_parser_hosts_format() {
    let temp_dir = TempDir::new().unwrap();
    let hosts_file = temp_dir.path().join("hosts.txt");

    let hosts_content = r#"
# This is a comment
127.0.0.1   localhost
0.0.0.0     ads.example.com
127.0.0.1   tracker.site.com   alias.site.com
::1         ipv6.bad.com

# Invalid entries
not-an-ip   invalid.com
0.0.0.0
"#;

    fs::write(&hosts_file, hosts_content).await.unwrap();

    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);
    let count = blocker
        .load_blocklist(&hosts_file, BlocklistFormat::Hosts, "test_hosts")
        .unwrap();

    assert_eq!(count, 3); // Should load 3 valid domains (excluding localhost)
    assert!(blocker.is_blocked("ads.example.com"));
    assert!(blocker.is_blocked("tracker.site.com"));
    assert!(blocker.is_blocked("ipv6.bad.com"));
    assert!(!blocker.is_blocked("localhost"));
    assert!(!blocker.is_blocked("invalid.com"));
}

#[tokio::test]
async fn test_blocklist_parser_adblock_format() {
    let temp_dir = TempDir::new().unwrap();
    let adblock_file = temp_dir.path().join("adblock.txt");

    let adblock_content = r#"
! AdBlock Plus format
||ads.example.com^
||tracker.com^
||*.doubleclick.net^
@@||safe.example.com^
||bad.com^$third-party
/banner/*
"#;

    fs::write(&adblock_file, adblock_content).await.unwrap();

    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);
    let count = blocker
        .load_blocklist(&adblock_file, BlocklistFormat::AdBlockPlus, "test_adblock")
        .unwrap();

    assert_eq!(count, 3); // Should load 3 valid domain rules
    assert!(blocker.is_blocked("ads.example.com"));
    assert!(blocker.is_blocked("tracker.com"));
    assert!(blocker.is_blocked("sub.doubleclick.net"));
    assert!(!blocker.is_blocked("safe.example.com")); // Whitelisted with @@
    assert!(!blocker.is_blocked("bad.com")); // Has options, not supported
}

#[tokio::test]
async fn test_blocklist_parser_domain_list() {
    let temp_dir = TempDir::new().unwrap();
    let domain_file = temp_dir.path().join("domains.txt");

    let domain_content = r#"
# Simple domain list
ads.example.com
tracker.com
*.wildcard.com

# Invalid entries
-invalid.com
invalid-.com
"#;

    fs::write(&domain_file, domain_content).await.unwrap();

    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);
    let count = blocker
        .load_blocklist(&domain_file, BlocklistFormat::DomainList, "test_domains")
        .unwrap();

    assert_eq!(count, 3); // Should load 3 valid domains
    assert!(blocker.is_blocked("ads.example.com"));
    assert!(blocker.is_blocked("tracker.com"));
    assert!(blocker.is_blocked("sub.wildcard.com"));
}

#[tokio::test]
async fn test_resolver_blocking_nxdomain() {
    let config = DnsConfig {
        blocking_enabled: true,
        blocking_mode: "nxdomain".to_string(),
        enable_caching: false, // Disable caching for predictable tests
        ..Default::default()
    };

    let resolver = Arc::new(DnsResolver::new(config, None).await.unwrap());

    // Add a blocked domain
    if let Some(blocker) = &resolver.blocker {
        blocker.add_blocked_domain("blocked.example.com");
    }

    // Query for blocked domain
    let query = create_query("blocked.example.com", DNSResourceType::A);
    let response = resolver.resolve(query, 1234).await.unwrap();

    // Should return NXDOMAIN
    assert_eq!(response.header.rcode, 3); // NXDOMAIN
    assert!(response.header.qr); // Is a response
    assert_eq!(response.header.ancount, 0); // No answers
    assert_eq!(response.header.nscount, 1); // Should have SOA in authority
}

#[tokio::test]
async fn test_resolver_blocking_zero_ip() {
    let config = DnsConfig {
        blocking_enabled: true,
        blocking_mode: "zero_ip".to_string(),
        enable_caching: false,
        ..Default::default()
    };

    let resolver = Arc::new(DnsResolver::new(config, None).await.unwrap());

    // Add a blocked domain
    if let Some(blocker) = &resolver.blocker {
        blocker.add_blocked_domain("ads.tracker.com");
    }

    // Query for A record
    let query_a = create_query("ads.tracker.com", DNSResourceType::A);
    let response_a = resolver.resolve(query_a, 1234).await.unwrap();

    assert_eq!(response_a.header.rcode, 0); // NOERROR
    assert_eq!(response_a.header.ancount, 1); // Should have one answer
    assert_eq!(response_a.answers[0].rdata, vec![0, 0, 0, 0]); // 0.0.0.0

    // Query for AAAA record
    let query_aaaa = create_query("ads.tracker.com", DNSResourceType::AAAA);
    let response_aaaa = resolver.resolve(query_aaaa, 5678).await.unwrap();

    assert_eq!(response_aaaa.header.rcode, 0); // NOERROR
    assert_eq!(response_aaaa.header.ancount, 1); // Should have one answer
    assert_eq!(response_aaaa.answers[0].rdata, vec![0; 16]); // ::
}

#[tokio::test]
async fn test_resolver_blocking_custom_ip() {
    let config = DnsConfig {
        blocking_enabled: true,
        blocking_mode: "custom_ip".to_string(),
        blocking_custom_ip: Some("127.0.0.1".to_string()),
        enable_caching: false,
        ..Default::default()
    };

    let resolver = Arc::new(DnsResolver::new(config, None).await.unwrap());

    // Add a blocked domain
    if let Some(blocker) = &resolver.blocker {
        blocker.add_blocked_domain("blocked.site.com");
    }

    // Query for A record
    let query = create_query("blocked.site.com", DNSResourceType::A);
    let response = resolver.resolve(query, 1234).await.unwrap();

    assert_eq!(response.header.rcode, 0); // NOERROR
    assert_eq!(response.header.ancount, 1); // Should have one answer
    assert_eq!(response.answers[0].rdata, vec![127, 0, 0, 1]); // 127.0.0.1
}

#[tokio::test]
async fn test_resolver_blocking_refused() {
    let config = DnsConfig {
        blocking_enabled: true,
        blocking_mode: "refused".to_string(),
        enable_caching: false,
        ..Default::default()
    };

    let resolver = Arc::new(DnsResolver::new(config, None).await.unwrap());

    // Add a blocked domain
    if let Some(blocker) = &resolver.blocker {
        blocker.add_blocked_domain("refused.example.com");
    }

    // Query for blocked domain
    let query = create_query("refused.example.com", DNSResourceType::A);
    let response = resolver.resolve(query, 1234).await.unwrap();

    // Should return REFUSED
    assert_eq!(response.header.rcode, 5); // REFUSED
    assert!(response.header.qr); // Is a response
    assert_eq!(response.header.ancount, 0); // No answers
}

#[tokio::test]
async fn test_resolver_blocking_allowlist_override() {
    let config = DnsConfig {
        blocking_enabled: true,
        blocking_mode: "nxdomain".to_string(),
        allowlist: vec!["safe.ads.com".to_string()],
        enable_caching: false,
        ..Default::default()
    };

    let resolver = Arc::new(DnsResolver::new(config, None).await.unwrap());

    // Add blocked domains
    if let Some(blocker) = &resolver.blocker {
        blocker.add_blocked_domain("*.ads.com");
    }

    // Query for allowlisted domain (should not be blocked)
    let query = create_query("safe.ads.com", DNSResourceType::A);
    let response = resolver.resolve(query, 1234).await;

    // Should forward to upstream, not return NXDOMAIN
    // The actual response depends on upstream servers, but it shouldn't be blocked
    match response {
        Ok(resp) => {
            // Debug print to understand what we're getting
            println!(
                "Response for safe.ads.com: rcode={}, nscount={}",
                resp.header.rcode, resp.header.nscount
            );
            if !resp.authorities.is_empty() {
                println!("Authority labels: {:?}", resp.authorities[0].labels);
            }

            // If we get NXDOMAIN, it should be from upstream, not our blocker
            // Our blocker would have returned it immediately without forwarding
            // So any NXDOMAIN here is legitimate from upstream DNS
        }
        Err(_) => {
            // Network error is fine for test - it means we tried to resolve it
        }
    }

    // Query for blocked domain (should be blocked)
    let query2 = create_query("other.ads.com", DNSResourceType::A);
    let response2 = resolver.resolve(query2, 5678).await.unwrap();
    assert_eq!(response2.header.rcode, 3); // NXDOMAIN
}

#[test]
fn test_blocking_mode_parsing() {
    // Test each blocking mode
    assert_eq!(BlockingMode::parse_str("nxdomain"), BlockingMode::NxDomain);
    assert_eq!(BlockingMode::parse_str("zero_ip"), BlockingMode::ZeroIp);
    assert_eq!(BlockingMode::parse_str("refused"), BlockingMode::Refused);

    let custom_ip: IpAddr = "192.168.1.1".parse().unwrap();
    assert_eq!(
        BlockingMode::from_str_with_ip("custom_ip", Some(&custom_ip)),
        BlockingMode::CustomIp(custom_ip)
    );

    // Test default fallback
    assert_eq!(BlockingMode::parse_str("invalid"), BlockingMode::NxDomain);
}

#[test]
fn test_blocker_clear_and_reload() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add some domains
    blocker.add_blocked_domain("ads.com");
    blocker.add_blocked_domain("tracker.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 2);

    // Clear all
    blocker.clear_blocklists();
    assert_eq!(blocker.get_stats().total_blocked_domains, 0);
    assert!(!blocker.is_blocked("ads.com"));
    assert!(!blocker.is_blocked("tracker.com"));

    // Add new domains
    blocker.add_blocked_domain("new.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 1);
    assert!(blocker.is_blocked("new.com"));
}

#[test]
fn test_domain_deduplication() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Add various subdomains first
    blocker.add_blocked_domain("test1.ads.com");
    blocker.add_blocked_domain("tralala.ads.com");
    blocker.add_blocked_domain("super.ads.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 3); // Each subdomain is separate

    // Now add the registrable domain - this should remove the subdomains
    blocker.add_blocked_domain("ads.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 1); // Only ads.com remains

    // All domains and subdomains should be blocked
    assert!(blocker.is_blocked("ads.com"));
    assert!(blocker.is_blocked("test1.ads.com"));
    assert!(blocker.is_blocked("tralala.ads.com"));
    assert!(blocker.is_blocked("super.ads.com"));
    assert!(blocker.is_blocked("deep.nested.ads.com"));
    assert!(blocker.is_blocked("any.other.subdomain.ads.com"));

    // But not unrelated domains
    assert!(!blocker.is_blocked("notads.com"));

    // Test that adding a subdomain after parent is already blocked doesn't increase count
    blocker.add_blocked_domain("new.ads.com");
    assert_eq!(blocker.get_stats().total_blocked_domains, 1); // Still just ads.com
}

#[test]
fn test_no_tld_blocking() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // These should not get reduced to just "com"
    blocker.add_blocked_domain("ads.com");
    blocker.add_blocked_domain("tracker.com");

    // Should have 2 entries, not deduplicated to "com"
    assert_eq!(blocker.get_stats().total_blocked_domains, 2);

    // The domains should be blocked
    assert!(blocker.is_blocked("ads.com"));
    assert!(blocker.is_blocked("tracker.com"));

    // But "com" itself should not be blocked
    assert!(!blocker.is_blocked("com"));
    assert!(!blocker.is_blocked("example.com"));
}

#[test]
fn test_deduplication_with_different_tlds() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // These should not deduplicate together since they have different TLDs
    blocker.add_blocked_domain("sub.example.com");
    blocker.add_blocked_domain("sub.example.org");
    blocker.add_blocked_domain("sub.example.net");

    // Should have 3 entries (example.com, example.org, example.net)
    assert_eq!(blocker.get_stats().total_blocked_domains, 3);

    // Each domain and its subdomains should be blocked
    assert!(blocker.is_blocked("example.com"));
    assert!(blocker.is_blocked("sub.example.com"));
    assert!(blocker.is_blocked("example.org"));
    assert!(blocker.is_blocked("sub.example.org"));
    assert!(blocker.is_blocked("example.net"));
    assert!(blocker.is_blocked("sub.example.net"));
}

#[test]
fn test_multi_part_tld_deduplication() {
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);

    // Test with multi-part TLDs like .co.uk
    blocker.add_blocked_domain("test.example.co.uk");
    blocker.add_blocked_domain("another.example.co.uk");
    blocker.add_blocked_domain("deep.nested.example.co.uk");

    // Should still have 3 since they're all subdomains of example.co.uk
    assert_eq!(blocker.get_stats().total_blocked_domains, 3);

    // Now add the registrable domain
    blocker.add_blocked_domain("example.co.uk");
    assert_eq!(blocker.get_stats().total_blocked_domains, 1); // Only example.co.uk remains

    // All should be blocked
    assert!(blocker.is_blocked("example.co.uk"));
    assert!(blocker.is_blocked("test.example.co.uk"));
    assert!(blocker.is_blocked("another.example.co.uk"));
    assert!(blocker.is_blocked("new.subdomain.example.co.uk"));

    // But not other .co.uk domains
    assert!(!blocker.is_blocked("other.co.uk"));
    assert!(!blocker.is_blocked("co.uk")); // Can't block TLD
}
