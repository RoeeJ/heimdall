#![allow(clippy::field_reassign_with_default)]

use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    question::DNSQuestion,
    resource::DNSResource,
};

/// Test basic NXDOMAIN negative caching per RFC 2308
#[test]
fn test_nxdomain_negative_caching() {
    let cache = DnsCache::new(1000, 300); // 5 minute negative TTL

    // Create NXDOMAIN response
    let mut nxdomain_response = DNSPacket::default();
    nxdomain_response.header.id = 1234;
    nxdomain_response.header.qr = true;
    nxdomain_response.header.rcode = 3; // NXDOMAIN
    nxdomain_response.header.qdcount = 1;
    nxdomain_response.header.ancount = 0;
    nxdomain_response.header.nscount = 1;

    // Add question
    let question = DNSQuestion {
        labels: vec![
            "nonexistent".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    nxdomain_response.questions.push(question.clone());

    // Add SOA record in authority section with minimum TTL
    let mut soa_record = DNSResource::default();
    soa_record.labels = vec!["example".to_string(), "com".to_string()];
    soa_record.rtype = DNSResourceType::SOA;
    soa_record.rclass = DNSResourceClass::IN;
    soa_record.ttl = 600; // SOA record TTL

    // SOA rdata: MNAME(example.com.) RNAME(admin.example.com.) SERIAL REFRESH RETRY EXPIRE MINIMUM(120)
    soa_record.rdata = create_soa_rdata(
        "example.com.",
        "admin.example.com.",
        2023100101,
        3600,
        1800,
        604800,
        120,
    );
    soa_record.rdlength = soa_record.rdata.len() as u16;

    nxdomain_response.authorities.push(soa_record);

    // Cache the NXDOMAIN response
    let cache_key = CacheKey::from_question(&question);
    cache.put(cache_key.clone(), nxdomain_response.clone());

    // Verify it was cached
    assert_eq!(cache.size(), 1);

    // Retrieve from cache
    let cached_response = cache.get(&cache_key).expect("Should find cached NXDOMAIN");
    assert_eq!(cached_response.header.rcode, 3);
    assert_eq!(cached_response.header.ancount, 0);

    // Check statistics
    let stats = cache.stats();
    assert_eq!(
        stats
            .nxdomain_responses
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(stats.hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        stats
            .negative_hits
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

/// Test NODATA negative caching per RFC 2308
#[test]
fn test_nodata_negative_caching() {
    let cache = DnsCache::new(1000, 300);

    // Create NODATA response (RCODE=0, no answers)
    let mut nodata_response = DNSPacket::default();
    nodata_response.header.id = 5678;
    nodata_response.header.qr = true;
    nodata_response.header.rcode = 0; // NOERROR
    nodata_response.header.qdcount = 1;
    nodata_response.header.ancount = 0; // No answers (NODATA)
    nodata_response.header.nscount = 1;

    // Add question for MX record
    let question = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string()],
        qtype: DNSResourceType::MX, // Query for MX, but none exists
        qclass: DNSResourceClass::IN,
    };
    nodata_response.questions.push(question.clone());

    // Add SOA record in authority section
    let mut soa_record = DNSResource::default();
    soa_record.labels = vec!["example".to_string(), "com".to_string()];
    soa_record.rtype = DNSResourceType::SOA;
    soa_record.rclass = DNSResourceClass::IN;
    soa_record.ttl = 300;

    // SOA with minimum TTL of 60 seconds
    soa_record.rdata = create_soa_rdata(
        "example.com.",
        "admin.example.com.",
        2023100102,
        3600,
        1800,
        604800,
        60,
    );
    soa_record.rdlength = soa_record.rdata.len() as u16;

    nodata_response.authorities.push(soa_record);

    // Cache the NODATA response
    let cache_key = CacheKey::from_question(&question);
    cache.put(cache_key.clone(), nodata_response.clone());

    // Verify it was cached
    assert_eq!(cache.size(), 1);

    // Retrieve from cache
    let cached_response = cache.get(&cache_key).expect("Should find cached NODATA");
    assert_eq!(cached_response.header.rcode, 0);
    assert_eq!(cached_response.header.ancount, 0);

    // Check statistics
    let stats = cache.stats();
    assert_eq!(
        stats
            .nodata_responses
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(stats.hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        stats
            .negative_hits
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

/// Test SOA minimum TTL extraction per RFC 1035
#[test]
fn test_soa_minimum_ttl_calculation() {
    let cache = DnsCache::new(1000, 600); // 10 minute negative TTL max

    // Create NXDOMAIN response with SOA minimum TTL of 180 seconds
    let mut nxdomain_response = DNSPacket::default();
    nxdomain_response.header.rcode = 3;
    nxdomain_response.header.qdcount = 1;
    nxdomain_response.header.nscount = 1;

    let question = DNSQuestion {
        labels: vec!["test".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    nxdomain_response.questions.push(question.clone());

    // SOA record with TTL=900 but minimum field=180
    let mut soa_record = DNSResource::default();
    soa_record.rtype = DNSResourceType::SOA;
    soa_record.ttl = 900; // SOA record TTL
    soa_record.rdata = create_soa_rdata(
        "example.com.",
        "admin.example.com.",
        2023100103,
        3600,
        1800,
        604800,
        180,
    );
    soa_record.rdlength = soa_record.rdata.len() as u16;

    nxdomain_response.authorities.push(soa_record);

    // Cache should use the minimum of SOA TTL and SOA minimum field = min(900, 180) = 180
    let cache_key = CacheKey::from_question(&question);
    cache.put(cache_key.clone(), nxdomain_response);

    // Get the cached entry and check TTL
    let cached_response = cache.get(&cache_key).expect("Should find cached response");

    // The TTL in the cached response should be approximately 180 seconds (allowing for processing time)
    // We check that it's less than the SOA record TTL (900) and close to the minimum (180)
    assert!(cached_response.authorities[0].ttl <= 180);
    assert!(cached_response.authorities[0].ttl > 170); // Allow for some processing time
}

/// Test that negative TTL is capped by configured maximum
#[test]
fn test_negative_ttl_capping() {
    let cache = DnsCache::new(1000, 60); // 1 minute negative TTL cap

    // Create NXDOMAIN with SOA minimum of 300 seconds (should be capped to 60)
    let mut nxdomain_response = DNSPacket::default();
    nxdomain_response.header.rcode = 3;
    nxdomain_response.header.qdcount = 1;
    nxdomain_response.header.nscount = 1;

    let question = DNSQuestion {
        labels: vec![
            "capped".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    nxdomain_response.questions.push(question.clone());

    // SOA with minimum TTL of 300 seconds
    let mut soa_record = DNSResource::default();
    soa_record.rtype = DNSResourceType::SOA;
    soa_record.ttl = 300;
    soa_record.rdata = create_soa_rdata(
        "example.com.",
        "admin.example.com.",
        2023100104,
        3600,
        1800,
        604800,
        300,
    );
    soa_record.rdlength = soa_record.rdata.len() as u16;

    nxdomain_response.authorities.push(soa_record);

    // Cache the response
    let cache_key = CacheKey::from_question(&question);
    cache.put(cache_key.clone(), nxdomain_response);

    // Get cached response - TTL should be capped at 60 seconds
    let cached_response = cache.get(&cache_key).expect("Should find cached response");
    assert!(cached_response.authorities[0].ttl <= 60);
}

/// Test positive response caching (should not be affected by negative caching logic)
#[test]
fn test_positive_response_caching() {
    let cache = DnsCache::new(1000, 300);

    // Create positive response with answers
    let mut positive_response = DNSPacket::default();
    positive_response.header.rcode = 0;
    positive_response.header.qdcount = 1;
    positive_response.header.ancount = 1;

    let question = DNSQuestion {
        labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };
    positive_response.questions.push(question.clone());

    // Add A record answer
    let mut a_record = DNSResource::default();
    a_record.rtype = DNSResourceType::A;
    a_record.ttl = 3600;
    a_record.rdata = vec![192, 0, 2, 1]; // 192.0.2.1
    a_record.rdlength = 4;

    positive_response.answers.push(a_record);

    // Cache the positive response
    let cache_key = CacheKey::from_question(&question);
    cache.put(cache_key.clone(), positive_response);

    // Verify it was cached
    let cached_response = cache.get(&cache_key).expect("Should find cached response");
    assert_eq!(cached_response.header.rcode, 0);
    assert_eq!(cached_response.header.ancount, 1);

    // Check statistics - should not count as negative
    let stats = cache.stats();
    assert_eq!(
        stats
            .nxdomain_responses
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        stats
            .nodata_responses
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        stats
            .negative_hits
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(stats.hits.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// Test cache debug info includes negative caching statistics
#[test]
fn test_cache_debug_info_with_negative_stats() {
    let cache = DnsCache::new(1000, 300);

    // Add some negative responses
    let nxdomain_key = CacheKey::new(
        "nonexistent.example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let mut nxdomain_response = DNSPacket::default();
    nxdomain_response.header.rcode = 3;
    nxdomain_response.header.qdcount = 1;
    cache.put(nxdomain_key.clone(), nxdomain_response);

    let nodata_key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::MX,
        DNSResourceClass::IN,
    );
    let mut nodata_response = DNSPacket::default();
    nodata_response.header.rcode = 0;
    nodata_response.header.qr = true; // Mark as response
    nodata_response.header.aa = true; // Authoritative answer for NODATA
    nodata_response.header.qdcount = 1;
    nodata_response.header.ancount = 0;
    cache.put(nodata_key.clone(), nodata_response);

    // Access both cached entries
    cache.get(&nxdomain_key);
    cache.get(&nodata_key);

    // Check debug info includes negative stats
    let debug_info = cache.debug_info();
    assert!(debug_info.contains("negative_hits=2"));
    assert!(debug_info.contains("NXDOMAIN=1"));
    assert!(debug_info.contains("NODATA=1"));
    assert!(debug_info.contains("hit_rate=100.00%"));
}

/// Helper function to create SOA rdata in wire format
fn create_soa_rdata(
    mname: &str,
    rname: &str,
    serial: u32,
    refresh: u32,
    retry: u32,
    expire: u32,
    minimum: u32,
) -> Vec<u8> {
    let mut rdata = Vec::new();

    // Encode MNAME
    encode_domain_name(&mut rdata, mname);

    // Encode RNAME
    encode_domain_name(&mut rdata, rname);

    // Encode 32-bit values
    rdata.extend_from_slice(&serial.to_be_bytes());
    rdata.extend_from_slice(&refresh.to_be_bytes());
    rdata.extend_from_slice(&retry.to_be_bytes());
    rdata.extend_from_slice(&expire.to_be_bytes());
    rdata.extend_from_slice(&minimum.to_be_bytes());

    rdata
}

/// Helper function to encode domain name in DNS wire format
fn encode_domain_name(buffer: &mut Vec<u8>, domain: &str) {
    for label in domain.split('.') {
        if !label.is_empty() {
            buffer.push(label.len() as u8);
            buffer.extend_from_slice(label.as_bytes());
        }
    }
    buffer.push(0); // Null terminator
}
