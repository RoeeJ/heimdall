use crate::cache::redis_backend::RedisConfig;
use redis::aio::ConnectionManager;

/// Helper to get Redis connection from config
pub async fn get_redis_connection() -> Option<ConnectionManager> {
    let config = RedisConfig::from_env();
    if config.enabled && config.url.is_some() {
        match redis::Client::open(config.url.as_ref().unwrap().as_str()) {
            Ok(client) => match ConnectionManager::new(client).await {
                Ok(conn) => {
                    tracing::info!("Connected to Redis for cluster registry");
                    Some(conn)
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to Redis for cluster registry: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to create Redis client for cluster registry: {}", e);
                None
            }
        }
    } else {
        None
    }
}
