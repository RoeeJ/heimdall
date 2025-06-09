use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
};
use std::fs;
use tempfile::tempdir;
use tokio;

#[tokio::test]
async fn test_rkyv_cache_persistence() {
    // Create a temporary directory for the test
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let cache_file_path = temp_dir
        .path()
        .join("test_cache.rkyv")
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

    // Save cache to disk (rkyv format)
    cache.save_to_disk().await.expect("Failed to save cache");

    // Verify cache file was created
    assert!(fs::metadata(&cache_file_path).is_ok());

    // Read the file and verify it's binary (not JSON)
    let file_data = fs::read(&cache_file_path).expect("Failed to read cache file");
    assert!(!file_data.starts_with(b"{")); // Should not be JSON
    println!("rkyv cache file size: {} bytes", file_data.len());

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
async fn test_rkyv_vs_json_size() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let rkyv_path = temp_dir
        .path()
        .join("test_cache.rkyv")
        .to_string_lossy()
        .to_string();
    let json_path = temp_dir
        .path()
        .join("test_cache.json")
        .to_string_lossy()
        .to_string();

    // Create cache with multiple entries
    let cache_rkyv = DnsCache::with_persistence(1000, 300, rkyv_path.clone());
    let cache_json = DnsCache::with_persistence(1000, 300, json_path.clone());

    // Add multiple test entries
    for i in 0..10 {
        let mut test_response = DNSPacket::default();
        test_response.header.id = 1000 + i as u16;
        test_response.header.qr = true;
        test_response.header.ancount = 1;

        let key = CacheKey::new(
            format!("test{}.example.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        cache_rkyv.put(key.clone(), test_response.clone());
        cache_json.put(key, test_response);
    }

    // Save both formats
    cache_rkyv
        .save_to_disk()
        .await
        .expect("Failed to save rkyv cache");

    // For JSON comparison, manually save using serde_json (since cache_json uses rkyv now)
    // This is just for size comparison
    use heimdall::cache::{CacheSnapshot, SerializableCacheEntry};
    use serde_json;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut entries = Vec::new();
    for (key, entry) in cache_json.iter_entries() {
        if !entry.is_expired() {
            let serializable_entry = SerializableCacheEntry::from(&entry);
            entries.push((key, serializable_entry));
        }
    }

    let snapshot = CacheSnapshot {
        entries,
        snapshot_timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        version: 1,
    };

    let json_data = serde_json::to_string_pretty(&snapshot).unwrap();
    tokio::fs::write(&json_path, json_data).await.unwrap();

    // Compare file sizes
    let rkyv_size = fs::metadata(&rkyv_path).unwrap().len();
    let json_size = fs::metadata(&json_path).unwrap().len();

    println!("rkyv size: {} bytes", rkyv_size);
    println!("JSON size: {} bytes", json_size);
    println!(
        "Size ratio (rkyv/JSON): {:.2}",
        rkyv_size as f64 / json_size as f64
    );

    // rkyv should typically be smaller and definitely more efficient to deserialize
    // The exact ratio depends on data structure, but rkyv is usually more compact
}

#[tokio::test]
async fn test_legacy_json_compatibility() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let cache_file_path = temp_dir
        .path()
        .join("legacy_cache.json")
        .to_string_lossy()
        .to_string();

    // Create a legacy JSON cache file manually
    let json_content = r#"{
  "entries": [
    [
      {
        "domain": "legacy.example.com",
        "record_type": "A",
        "record_class": "IN",
        "hash": 1234567890
      },
      {
        "response": {
          "header": {
            "id": 9999,
            "qr": true,
            "opcode": 0,
            "aa": false,
            "tc": false,
            "rd": true,
            "ra": true,
            "z": 0,
            "rcode": 0,
            "qdcount": 1,
            "ancount": 1,
            "nscount": 0,
            "arcount": 0
          },
          "questions": [],
          "answers": [],
          "authorities": [],
          "resources": [],
          "edns": null
        },
        "expiry_timestamp": 9999999999,
        "original_ttl": 300,
        "is_negative": false
      }
    ]
  ],
  "snapshot_timestamp": 1234567890,
  "version": 1
}"#;

    fs::write(&cache_file_path, json_content).expect("Failed to write legacy JSON");

    // Create cache and try to load legacy format
    let cache = DnsCache::with_persistence(1000, 300, cache_file_path);
    let result = cache.load_from_disk().await;

    assert!(result.is_ok(), "Should be able to load legacy JSON format");
    assert_eq!(cache.size(), 1, "Should load one entry from legacy format");

    // The loaded entry should be accessible
    // Get the actual key from the cache (since hash is pre-computed)
    let cached_key = cache.iter_entries().next().map(|(k, _)| k);
    assert!(cached_key.is_some(), "Should have one cached entry");

    let key = cached_key.unwrap();
    println!("Found cached key: {:?}", key);

    let result = cache.get(&key);
    assert!(result.is_some(), "Legacy entry should be accessible");
}
