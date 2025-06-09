use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_cache_persistence() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let cache_file_path = temp_dir
        .path()
        .join("test_cache.json")
        .to_string_lossy()
        .to_string();

    // Create a cache with persistence
    let cache = DnsCache::with_persistence(1000, 300, cache_file_path.clone());

    // Create a test DNS response
    let mut test_response = DNSPacket::default();
    test_response.header.id = 12345;
    test_response.header.qr = true;
    test_response.header.ancount = 1;

    // Create a test cache key
    let key = CacheKey::new(
        "example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    // Store the response in cache
    cache.put(key.clone(), test_response.clone());

    // Verify the entry exists
    assert!(cache.get(&key).is_some());
    assert_eq!(cache.size(), 1);

    // Save cache to disk
    cache.save_to_disk().await.expect("Failed to save cache");

    // Verify cache file was created
    assert!(fs::metadata(&cache_file_path).is_ok());

    // Create a new cache instance and load from disk
    let new_cache = DnsCache::with_persistence(1000, 300, cache_file_path.clone());
    new_cache
        .load_from_disk()
        .await
        .expect("Failed to load cache");

    // Verify the entry was restored
    let restored_response = new_cache.get(&key);
    assert!(restored_response.is_some());
    assert_eq!(new_cache.size(), 1);

    let restored = restored_response.unwrap();
    assert_eq!(restored.header.id, test_response.header.id);
    assert_eq!(restored.header.qr, test_response.header.qr);

    // Cleanup happens automatically when temp_dir is dropped
}

#[tokio::test]
async fn test_cache_persistence_expired_entries() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let cache_file_path = temp_dir
        .path()
        .join("test_cache_expired.json")
        .to_string_lossy()
        .to_string();

    // Create a cache with very short TTL (1 second)
    let cache = DnsCache::with_persistence(1000, 1, cache_file_path.clone());

    // Create test response
    let mut test_response = DNSPacket::default();
    test_response.header.id = 54321;

    let key = CacheKey::new(
        "expired.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    // Store with short TTL
    cache.put(key.clone(), test_response);

    // Wait for expiry
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Save cache (should skip expired entries)
    cache.save_to_disk().await.expect("Failed to save cache");

    // Load into new cache
    let new_cache = DnsCache::with_persistence(1000, 1, cache_file_path);
    new_cache
        .load_from_disk()
        .await
        .expect("Failed to load cache");

    // Should be empty since entry was expired
    assert_eq!(new_cache.size(), 0);
    assert!(new_cache.get(&key).is_none());
}

#[tokio::test]
async fn test_cache_without_persistence() {
    // Test regular cache without persistence
    let cache = DnsCache::new(1000, 300);

    assert!(!cache.has_persistence());
    assert!(cache.cache_file_path().is_none());

    // Save/load should be no-ops
    cache.save_to_disk().await.expect("Save should be no-op");
    cache.load_from_disk().await.expect("Load should be no-op");
}
