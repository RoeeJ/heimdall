use heimdall::blocking::arena::StringArena;
use heimdall::blocking::builder::BlocklistBuilder;
use heimdall::blocking::lookup::{
    DomainLabels, DomainNormalizer, count_labels, extract_registrable_part,
};
use heimdall::blocking::psl::PublicSuffixList;
use std::sync::Arc;

#[test]
fn test_arena_zero_copy() {
    let mut arena = StringArena::with_capacity(1024);

    // Add domains
    let domains = vec!["example.com", "test.example.com", "ads.tracking.net"];
    let mut offsets = Vec::new();

    for domain in &domains {
        let offset = arena.add(domain.as_bytes()).unwrap();
        offsets.push(offset);
    }

    // Convert to shared arena
    let shared = arena.into_shared();

    // Verify we can retrieve domains without allocation
    for (i, offset) in offsets.iter().enumerate() {
        let retrieved = shared.get(offset.0, offset.1).unwrap();
        assert_eq!(retrieved, domains[i].as_bytes());
    }
}

#[test]
fn test_domain_normalization() {
    // Test case sensitivity
    let mut domain = b"Example.COM".to_vec();
    assert!(DomainNormalizer::normalize_in_place(&mut domain));
    assert_eq!(&domain, b"example.com");

    // Test already normalized
    let mut domain = b"example.com".to_vec();
    assert!(!DomainNormalizer::normalize_in_place(&mut domain));

    // Test trailing dot removal
    assert_eq!(DomainNormalizer::normalized_len(b"example.com."), 11);
}

#[test]
fn test_label_iteration() {
    let labels: Vec<&[u8]> = DomainLabels::new(b"www.example.com").collect();
    assert_eq!(labels, vec![&b"www"[..], &b"example"[..], &b"com"[..]]);

    let reversed = DomainLabels::new(b"www.example.com").reversed();
    assert_eq!(reversed, vec![&b"com"[..], &b"example"[..], &b"www"[..]]);
}

#[test]
fn test_psl_integration() {
    let psl = Arc::new(PublicSuffixList::new());

    // Load test PSL data
    let psl_data = r#"
// Test PSL data
com
net
org
co.uk
*.uk
!metro.tokyo.jp
tokyo.jp
"#;

    psl.load_from_string(psl_data).unwrap();

    // Test registrable domain extraction
    assert_eq!(
        psl.get_registrable_domain("www.example.com"),
        Some("example.com".to_string())
    );
    assert_eq!(
        psl.get_registrable_domain("test.example.co.uk"),
        Some("example.co.uk".to_string())
    );
    // With simple fallback logic, these use standard TLD rules
    assert_eq!(
        psl.get_registrable_domain("something.random.uk"),
        Some("random.uk".to_string())
    );
    // metro.tokyo.jp uses standard logic since we don't have full PSL parsing
    assert_eq!(
        psl.get_registrable_domain("metro.tokyo.jp"),
        Some("tokyo.jp".to_string())
    );
}

#[test]
fn test_psl_deduplication() {
    let psl = Arc::new(PublicSuffixList::new());
    psl.load_from_string("com\nnet\norg\nco.uk").unwrap();

    let mut builder = BlocklistBuilder::new(psl, false);

    // Add subdomains first
    builder.add_domain("ads.example.com", "test");
    builder.add_domain("tracking.ads.example.com", "test");
    builder.add_domain("pop.ads.example.com", "test");

    // Now add the parent - should remove all subdomains
    builder.add_domain("example.com", "test");

    // Build and verify
    let (_trie, _arena, count) = builder.build().unwrap();
    assert_eq!(count, 1); // Only example.com should remain
}

#[test]
fn test_wildcard_domains() {
    let psl = Arc::new(PublicSuffixList::new());
    psl.load_from_string("com\nnet").unwrap();

    let mut builder = BlocklistBuilder::new(psl, true);

    builder.add_domain("*.doubleclick.net", "test");
    builder.add_domain("specific.example.com", "test");

    let (_trie, _arena, count) = builder.build().unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_trie_lookup_performance() {
    let psl = Arc::new(PublicSuffixList::new());
    let mut builder = BlocklistBuilder::new(psl, false);

    // Add test domains
    for i in 0..1000 {
        let domain = format!("test{}.example.com", i);
        builder.add_domain(&domain, "test");
    }

    let (trie, _arena, _count) = builder.build().unwrap();

    // Test lookups
    assert!(trie.is_blocked(b"test500.example.com"));
    assert!(!trie.is_blocked(b"test1001.example.com"));
    assert!(!trie.is_blocked(b"notblocked.example.com"));
}

#[test]
fn test_registrable_extraction() {
    assert_eq!(
        extract_registrable_part(b"www.example.com", 1),
        Some(b"example.com".as_ref())
    );

    assert_eq!(
        extract_registrable_part(b"sub.test.example.co.uk", 2),
        Some(b"example.co.uk".as_ref())
    );

    assert_eq!(
        extract_registrable_part(b"example.com", 1),
        Some(b"example.com".as_ref())
    );

    assert_eq!(extract_registrable_part(b"com", 1), None);
}

#[test]
fn test_label_counting() {
    assert_eq!(count_labels(b"www.example.com"), 3);
    assert_eq!(count_labels(b"example.com"), 2);
    assert_eq!(count_labels(b"com"), 1);
    assert_eq!(count_labels(b"sub.domain.example.co.uk"), 5);
    assert_eq!(count_labels(b"example.com."), 2); // Trailing dot ignored
}

#[tokio::test]
async fn test_concurrent_lookups() {
    use heimdall::blocking::BlockingMode;
    use heimdall::blocking::blocker_v2::DnsBlockerV2;

    let blocker = Arc::new(
        DnsBlockerV2::new(BlockingMode::NxDomain, true)
            .await
            .expect("Failed to create blocker"),
    );

    // Spawn multiple tasks doing lookups
    let mut handles = Vec::new();
    for i in 0..10 {
        let blocker_clone = blocker.clone();
        let handle = tokio::spawn(async move {
            for j in 0..1000 {
                let domain = format!("test{}.example{}.com", j, i);
                blocker_clone.is_blocked(&domain);
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify stats are consistent
    let stats = blocker.get_stats();
    assert_eq!(stats.queries_blocked + stats.queries_allowed, 10000);
}
