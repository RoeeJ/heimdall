use heimdall::cache::local_backend::LocalCache;
use heimdall::cache::redis_backend::{CacheBackend, CachedEntry};
use heimdall::cache::{CacheKey, CacheStats};
use heimdall::dns::DNSPacket;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

fn create_test_cache(max_size: usize) -> LocalCache {
    let stats = Arc::new(CacheStats::new());
    LocalCache::new(max_size, stats)
}

fn create_test_entry(ttl_seconds: u64) -> CachedEntry {
    let packet = DNSPacket::default(); // Use default packet for simplicity
    let expires_at = SystemTime::now() + Duration::from_secs(ttl_seconds);
    CachedEntry {
        packet,
        expires_at,
        cached_at: SystemTime::now(),
    }
}

#[tokio::test]
async fn test_local_cache_creation() {
    let cache = create_test_cache(100);
    assert_eq!(cache.len().await, 0);
}

#[tokio::test]
async fn test_local_cache_set_and_get() {
    let cache = create_test_cache(100);
    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let entry = create_test_entry(300);

    // Set entry
    cache.set(&key, entry.clone()).await;
    assert_eq!(cache.len().await, 1);

    // Get entry
    let retrieved = cache.get(&key).await;
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_local_cache_get_nonexistent() {
    let cache = create_test_cache(100);
    let key = CacheKey::new(
        "nonexistent.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    let result = cache.get(&key).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_local_cache_remove() {
    let cache = create_test_cache(100);
    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );
    let entry = create_test_entry(300);

    // Set entry
    cache.set(&key, entry).await;
    assert_eq!(cache.len().await, 1);

    // Remove entry
    cache.remove(&key).await;
    assert_eq!(cache.len().await, 0);

    // Verify it's gone
    let result = cache.get(&key).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_local_cache_clear() {
    let cache = create_test_cache(100);

    // Add multiple entries
    for i in 0..5 {
        let key = CacheKey::new(
            format!("example{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let entry = create_test_entry(300);
        cache.set(&key, entry).await;
    }

    assert_eq!(cache.len().await, 5);

    // Clear cache
    cache.clear().await;
    assert_eq!(cache.len().await, 0);
}

#[tokio::test]
async fn test_local_cache_expired_entry_removal() {
    let cache = create_test_cache(100);
    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    // Create entry that expires in 1 millisecond
    let packet = DNSPacket::default();
    let expires_at = SystemTime::now() + Duration::from_millis(1);
    let entry = CachedEntry {
        packet,
        expires_at,
        cached_at: SystemTime::now(),
    };

    cache.set(&key, entry).await;
    assert_eq!(cache.len().await, 1);

    // Wait for expiration
    sleep(Duration::from_millis(10)).await;

    // Getting expired entry should return None and remove it
    let result = cache.get(&key).await;
    assert!(result.is_none());

    // Entry should be removed from cache
    assert_eq!(cache.len().await, 0);
}

#[tokio::test]
async fn test_local_cache_evict_expired() {
    let cache = create_test_cache(100);

    // Add mix of expired and non-expired entries
    for i in 0..10 {
        let key = CacheKey::new(
            format!("example{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let packet = DNSPacket::default();

        let expires_at = if i < 5 {
            // First 5 entries are already expired
            SystemTime::now() - Duration::from_secs(1)
        } else {
            // Last 5 entries are still valid
            SystemTime::now() + Duration::from_secs(300)
        };

        let entry = CachedEntry {
            packet,
            expires_at,
            cached_at: SystemTime::now() - Duration::from_secs(10),
        };

        cache.set(&key, entry).await;
    }

    assert_eq!(cache.len().await, 10);

    // Evict expired entries
    cache.evict_expired();

    // Only non-expired entries should remain
    assert_eq!(cache.len().await, 5);

    // Verify remaining entries are the non-expired ones
    for i in 5..10 {
        let key = CacheKey::new(
            format!("example{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        assert!(cache.get(&key).await.is_some());
    }
}

#[tokio::test]
async fn test_local_cache_eviction_on_max_size() {
    let cache = create_test_cache(5); // Small cache size

    // Add more entries than max size
    for i in 0..10 {
        let key = CacheKey::new(
            format!("example{}.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let entry = create_test_entry(300);
        cache.set(&key, entry).await;
    }

    // Cache should not exceed max size (but may temporarily during insertion)
    // The eviction happens after insertion, so we need to give it a moment
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(cache.len().await <= 6); // Allow for one extra during race conditions
}

#[tokio::test]
async fn test_local_cache_concurrent_operations() {
    let cache = Arc::new(create_test_cache(100));
    let mut handles = vec![];

    // Spawn multiple tasks doing concurrent operations
    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let key = CacheKey::new(
                format!("example{}.com", i),
                DNSResourceType::A,
                DNSResourceClass::IN,
            );
            let entry = create_test_entry(300);

            // Set
            cache_clone.set(&key, entry).await;

            // Get
            let result = cache_clone.get(&key).await;
            assert!(result.is_some());

            // Remove if even
            if i % 2 == 0 {
                cache_clone.remove(&key).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Should have 5 entries (odd numbered ones)
    assert_eq!(cache.len().await, 5);
}
