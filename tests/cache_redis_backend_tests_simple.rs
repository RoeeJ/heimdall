use heimdall::cache::local_backend::LocalCache;
use heimdall::cache::redis_backend::{CacheBackend, CachedEntry, LayeredCache, RedisConfig};
use heimdall::cache::{CacheKey, CacheStats};
use heimdall::dns::DNSPacket;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

fn create_test_entry(ttl_seconds: u64) -> CachedEntry {
    let packet = DNSPacket::default();
    let expires_at = SystemTime::now() + Duration::from_secs(ttl_seconds);
    CachedEntry {
        packet,
        expires_at,
        cached_at: SystemTime::now(),
    }
}

#[tokio::test]
async fn test_cached_entry_is_expired() {
    // Test non-expired entry
    let entry = create_test_entry(300);
    assert!(!entry.is_expired());

    // Test expired entry
    let expired_entry = CachedEntry {
        packet: DNSPacket::default(),
        expires_at: SystemTime::now() - Duration::from_secs(10),
        cached_at: SystemTime::now() - Duration::from_secs(310),
    };
    assert!(expired_entry.is_expired());
}

#[tokio::test]
async fn test_cached_entry_remaining_ttl() {
    // Test entry with future expiration
    let entry = create_test_entry(300);
    let ttl = entry.remaining_ttl();
    assert!(ttl.as_secs() > 290 && ttl.as_secs() <= 300);

    // Test expired entry
    let expired_entry = CachedEntry {
        packet: DNSPacket::default(),
        expires_at: SystemTime::now() - Duration::from_secs(10),
        cached_at: SystemTime::now() - Duration::from_secs(310),
    };
    assert_eq!(expired_entry.remaining_ttl(), Duration::ZERO);
}

#[tokio::test]
async fn test_cache_backend_is_empty_default() {
    // Test default implementation of is_empty
    let stats = Arc::new(CacheStats::new());
    let cache = LocalCache::new(100, stats);

    // Initially empty
    assert!(cache.is_empty().await);
    assert_eq!(cache.len().await, 0);

    // Add entry
    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let entry = create_test_entry(300);
    cache.set(&key, entry).await;

    // No longer empty
    assert!(!cache.is_empty().await);
    assert_eq!(cache.len().await, 1);
}

#[tokio::test]
async fn test_redis_config_default() {
    let config = RedisConfig::default();
    assert!(!config.enabled);
    assert!(config.url.is_none());
    assert_eq!(config.key_prefix, "heimdall:dns:cache");
    assert_eq!(config.connection_timeout, Duration::from_secs(5));
    assert_eq!(config.max_retries, 3);
}

#[tokio::test]
async fn test_redis_config_from_env() {
    // Test with HEIMDALL_REDIS_URL
    unsafe {
        std::env::set_var("HEIMDALL_REDIS_URL", "redis://test:6379");
        std::env::set_var("HEIMDALL_REDIS_KEY_PREFIX", "test:prefix");
    }

    let config = RedisConfig::from_env();
    assert!(config.enabled);
    assert_eq!(config.url, Some("redis://test:6379".to_string()));
    assert_eq!(config.key_prefix, "test:prefix");

    // Clean up
    unsafe {
        std::env::remove_var("HEIMDALL_REDIS_URL");
        std::env::remove_var("HEIMDALL_REDIS_KEY_PREFIX");
    }
}

#[tokio::test]
async fn test_redis_config_connect_disabled() {
    let config = RedisConfig {
        enabled: false,
        url: Some("redis://localhost:6379".to_string()),
        ..Default::default()
    };

    let result = config.connect().await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_redis_config_connect_no_url() {
    let config = RedisConfig {
        enabled: true,
        url: None,
        ..Default::default()
    };

    let result = config.connect().await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_layered_cache_l1_only() {
    let stats = Arc::new(CacheStats::new());
    let l1 = Arc::new(LocalCache::new(100, stats.clone()));
    let cache = LayeredCache::new(l1.clone(), None, stats.clone());

    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let packet = DNSPacket::default();

    // Set entry
    cache
        .set(&key, packet.clone(), Duration::from_secs(300))
        .await;

    // Get should find it in L1
    let initial_hits = stats.hits.load(Ordering::Relaxed);
    let result = cache.get(&key).await;
    assert!(result.is_some());
    assert_eq!(stats.hits.load(Ordering::Relaxed), initial_hits + 1);

    // Cache miss
    let miss_key = CacheKey::new(
        "missing.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let initial_misses = stats.misses.load(Ordering::Relaxed);
    let result = cache.get(&miss_key).await;
    assert!(result.is_none());
    assert_eq!(stats.misses.load(Ordering::Relaxed), initial_misses + 1);
}

#[tokio::test]
async fn test_layered_cache_operations() {
    let stats = Arc::new(CacheStats::new());
    let l1 = Arc::new(LocalCache::new(100, stats.clone()));
    let l2 = Arc::new(LocalCache::new(100, stats.clone()));
    let cache = LayeredCache::new(l1.clone(), Some(l2.clone()), stats.clone());

    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let packet = DNSPacket::default();

    // Test set - should set in both layers
    cache
        .set(&key, packet.clone(), Duration::from_secs(300))
        .await;
    assert_eq!(l1.len().await, 1);
    assert_eq!(l2.len().await, 1);

    // Test remove - should remove from both layers
    cache.remove(&key).await;
    assert_eq!(l1.len().await, 0);
    assert_eq!(l2.len().await, 0);

    // Test clear with multiple entries
    for i in 0..5 {
        let key = CacheKey::new(
            format!("example{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let packet = DNSPacket::default();
        cache.set(&key, packet, Duration::from_secs(300)).await;
    }

    assert_eq!(l1.len().await, 5);
    assert_eq!(l2.len().await, 5);

    cache.clear().await;
    assert_eq!(l1.len().await, 0);
    assert_eq!(l2.len().await, 0);
}

#[tokio::test]
async fn test_layered_cache_len() {
    let stats = Arc::new(CacheStats::new());
    let l1 = Arc::new(LocalCache::new(100, stats.clone()));
    let l2 = Arc::new(LocalCache::new(100, stats.clone()));
    let cache = LayeredCache::new(l1.clone(), Some(l2.clone()), stats.clone());

    // Add different entries to each layer
    for i in 0..3 {
        let key = CacheKey::new(
            format!("l1-{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let entry = create_test_entry(300);
        l1.set(&key, entry).await;
    }

    for i in 0..2 {
        let key = CacheKey::new(
            format!("l2-{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let entry = create_test_entry(300);
        l2.set(&key, entry).await;
    }

    // Total should be sum of both
    assert_eq!(cache.len().await, 5);
    assert!(!cache.is_empty().await);
}

#[tokio::test]
async fn test_cached_entry_serialization() {
    let entry = create_test_entry(300);

    // Test that entry can be serialized/deserialized
    let serialized = bincode::serialize(&entry).unwrap();
    let deserialized: CachedEntry = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.cached_at, entry.cached_at);
}
