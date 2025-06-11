use super::{CacheKey, CacheStats};
use crate::dns::DNSPacket;
use crate::error::{DnsError, Result};
use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tracing::{debug, error, info, warn};

/// Trait for cache backends
#[async_trait]
pub trait CacheBackend: Send + Sync {
    /// Get a cache entry
    async fn get(&self, key: &CacheKey) -> Option<CachedEntry>;

    /// Set a cache entry with TTL
    async fn set(&self, key: &CacheKey, entry: CachedEntry);

    /// Remove a cache entry
    async fn remove(&self, key: &CacheKey);

    /// Clear all entries
    async fn clear(&self);

    /// Get the number of entries
    async fn len(&self) -> usize;

    /// Check if cache is empty
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// A cached DNS entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry {
    pub packet: DNSPacket,
    pub expires_at: SystemTime,
    pub cached_at: SystemTime,
}

impl CachedEntry {
    /// Check if the entry is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }

    /// Get remaining TTL in seconds
    pub fn remaining_ttl(&self) -> Duration {
        self.expires_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
    }
}

/// Redis cache backend implementation
pub struct RedisCache {
    client: ConnectionManager,
    key_prefix: String,
    #[allow(dead_code)]
    default_ttl: u64,
}

impl RedisCache {
    /// Create a new Redis cache backend
    pub async fn new(redis_url: &str, key_prefix: String, default_ttl: u64) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| DnsError::Redis(format!("Failed to create Redis client: {}", e)))?;

        let connection_manager = ConnectionManager::new(client)
            .await
            .map_err(|e| DnsError::Redis(format!("Failed to connect to Redis: {}", e)))?;

        info!("Connected to Redis at {}", redis_url);

        Ok(Self {
            client: connection_manager,
            key_prefix,
            default_ttl,
        })
    }

    /// Check if Redis is available
    pub async fn is_available(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Ping Redis to check connectivity
    pub async fn ping(&self) -> Result<()> {
        let mut conn = self.client.clone();
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| DnsError::Redis(format!("Redis ping failed: {}", e)))?;
        Ok(())
    }

    /// Get Redis key for a cache key
    fn redis_key(&self, key: &CacheKey) -> String {
        format!("{}:{}", self.key_prefix, key)
    }
}

#[async_trait]
impl CacheBackend for RedisCache {
    async fn get(&self, key: &CacheKey) -> Option<CachedEntry> {
        let redis_key = self.redis_key(key);
        let mut conn = self.client.clone();

        match conn.get::<_, Vec<u8>>(&redis_key).await {
            Ok(data) => {
                match bincode::deserialize::<CachedEntry>(&data) {
                    Ok(entry) => {
                        if entry.is_expired() {
                            // Remove expired entry
                            let _ = conn.del::<_, ()>(&redis_key).await;
                            None
                        } else {
                            debug!("Redis cache hit for key: {}", key);
                            Some(entry)
                        }
                    }
                    Err(e) => {
                        error!("Failed to deserialize cache entry: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                if !matches!(e.kind(), redis::ErrorKind::TypeError) {
                    debug!("Redis cache miss for key: {} ({})", key, e);
                }
                None
            }
        }
    }

    async fn set(&self, key: &CacheKey, entry: CachedEntry) {
        let redis_key = self.redis_key(key);
        let mut conn = self.client.clone();

        // Calculate TTL
        let ttl = entry.remaining_ttl().as_secs().max(1);

        match bincode::serialize(&entry) {
            Ok(data) => {
                if let Err(e) = conn.set_ex::<_, _, ()>(&redis_key, data, ttl).await {
                    error!("Failed to set cache entry in Redis: {}", e);
                } else {
                    debug!("Cached entry in Redis: {} (TTL: {}s)", key, ttl);
                }
            }
            Err(e) => {
                error!("Failed to serialize cache entry: {}", e);
            }
        }
    }

    async fn remove(&self, key: &CacheKey) {
        let redis_key = self.redis_key(key);
        let mut conn = self.client.clone();

        if let Err(e) = conn.del::<_, ()>(&redis_key).await {
            error!("Failed to remove cache entry from Redis: {}", e);
        }
    }

    async fn clear(&self) {
        let pattern = format!("{}:*", self.key_prefix);
        let mut conn = self.client.clone();

        // Use SCAN to find all keys with our prefix
        let result: redis::RedisResult<(i32, Vec<String>)> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(1000)
            .query_async(&mut conn)
            .await;

        match result {
            Ok((_, keys)) => {
                if !keys.is_empty() {
                    let _: () = conn.del(keys).await.unwrap_or(());
                }
            }
            Err(e) => {
                error!("Failed to scan Redis keys: {}", e);
            }
        }
    }

    async fn len(&self) -> usize {
        let pattern = format!("{}:*", self.key_prefix);
        let mut conn = self.client.clone();
        let mut count = 0;
        let mut cursor = 0;

        loop {
            let result: redis::RedisResult<(i32, Vec<String>)> = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(1000)
                .query_async(&mut conn)
                .await;

            match result {
                Ok((next_cursor, keys)) => {
                    count += keys.len();
                    cursor = next_cursor;
                    if cursor == 0 {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to count Redis keys: {}", e);
                    break;
                }
            }
        }

        count
    }
}

/// A layered cache implementation with L1 (local) and L2 (Redis) layers
pub struct LayeredCache {
    l1: Arc<dyn CacheBackend>,
    l2: Option<Arc<dyn CacheBackend>>,
    stats: Arc<CacheStats>,
}

impl LayeredCache {
    /// Create a new layered cache
    pub fn new(
        l1: Arc<dyn CacheBackend>,
        l2: Option<Arc<dyn CacheBackend>>,
        stats: Arc<CacheStats>,
    ) -> Self {
        Self { l1, l2, stats }
    }

    /// Get from cache, checking L1 first, then L2
    pub async fn get(&self, key: &CacheKey) -> Option<DNSPacket> {
        // Try L1 first
        if let Some(entry) = self.l1.get(key).await {
            self.stats.record_hit();
            return Some(entry.packet);
        }

        // Try L2 if available
        if let Some(l2) = &self.l2 {
            if let Some(entry) = l2.get(key).await {
                // Promote to L1
                self.l1.set(key, entry.clone()).await;
                self.stats.record_hit();
                return Some(entry.packet);
            }
        }

        self.stats.record_miss();
        None
    }

    /// Set in both L1 and L2 caches
    pub async fn set(&self, key: &CacheKey, packet: DNSPacket, ttl: Duration) {
        let entry = CachedEntry {
            packet,
            expires_at: SystemTime::now() + ttl,
            cached_at: SystemTime::now(),
        };

        // Always set in L1
        self.l1.set(key, entry.clone()).await;

        // Set in L2 if available
        if let Some(l2) = &self.l2 {
            l2.set(key, entry).await;
        }
    }

    /// Remove from both caches
    pub async fn remove(&self, key: &CacheKey) {
        self.l1.remove(key).await;
        if let Some(l2) = &self.l2 {
            l2.remove(key).await;
        }
    }

    /// Clear both caches
    pub async fn clear(&self) {
        self.l1.clear().await;
        if let Some(l2) = &self.l2 {
            l2.clear().await;
        }
    }

    /// Get total size (L1 + L2)
    pub async fn len(&self) -> usize {
        let l1_len = self.l1.len().await;
        let l2_len = if let Some(l2) = &self.l2 {
            l2.len().await
        } else {
            0
        };
        l1_len + l2_len
    }

    /// Check if cache is empty
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// Redis connection configuration with auto-detection
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis URL (e.g., redis://localhost:6379)
    pub url: Option<String>,
    /// Enable Redis integration
    pub enabled: bool,
    /// Key prefix for DNS cache entries
    pub key_prefix: String,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Retry attempts for connection
    pub max_retries: u32,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: None,
            enabled: false,
            key_prefix: "heimdall:dns:cache".to_string(),
            connection_timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }
}

impl RedisConfig {
    /// Auto-detect Redis configuration from environment
    pub fn from_env() -> Self {
        let mut config = Self::default();

        // Check for explicit Redis URL
        if let Ok(url) = std::env::var("HEIMDALL_REDIS_URL") {
            config.url = Some(url);
            config.enabled = true;
        } else if let Ok(url) = std::env::var("REDIS_URL") {
            config.url = Some(url);
            config.enabled = true;
        } else {
            // Try to detect Redis in Kubernetes
            if let Ok(host) = std::env::var("HEIMDALL_REDIS_SERVICE_HOST") {
                let port = std::env::var("HEIMDALL_REDIS_SERVICE_PORT")
                    .unwrap_or_else(|_| "6379".to_string());
                config.url = Some(format!("redis://{}:{}", host, port));
                config.enabled = true;
            } else if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
                // We're in Kubernetes, try default Redis service name
                config.url = Some("redis://heimdall-redis:6379".to_string());
                config.enabled = true;
            }
        }

        // Allow explicit disable
        if let Ok(enabled) = std::env::var("HEIMDALL_REDIS_ENABLED") {
            config.enabled = enabled.parse().unwrap_or(false);
        }

        // Custom key prefix
        if let Ok(prefix) = std::env::var("HEIMDALL_REDIS_KEY_PREFIX") {
            config.key_prefix = prefix;
        }

        info!(
            "Redis configuration: enabled={}, url={:?}",
            config.enabled, config.url
        );

        config
    }

    /// Try to connect to Redis with retries
    pub async fn connect(&self) -> Option<RedisCache> {
        if !self.enabled || self.url.is_none() {
            return None;
        }

        let url = self.url.as_ref().unwrap();
        let mut retries = 0;

        loop {
            match RedisCache::new(url, self.key_prefix.clone(), 300).await {
                Ok(cache) => {
                    info!("Successfully connected to Redis");
                    return Some(cache);
                }
                Err(e) => {
                    retries += 1;
                    if retries > self.max_retries {
                        warn!(
                            "Failed to connect to Redis after {} attempts: {}",
                            self.max_retries, e
                        );
                        return None;
                    }
                    warn!(
                        "Failed to connect to Redis (attempt {}/{}): {}",
                        retries, self.max_retries, e
                    );
                    tokio::time::sleep(Duration::from_secs(retries as u64)).await;
                }
            }
        }
    }
}
