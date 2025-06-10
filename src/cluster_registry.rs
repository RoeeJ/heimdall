use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisError};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Member information stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMember {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub pod_ip: String,
    pub last_heartbeat: u64, // Unix timestamp
    pub status: MemberStatus,
    pub stats: MemberStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberStatus {
    Starting,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberStats {
    pub uptime_seconds: u64,
    pub queries_total: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_size: usize,
    pub upstream_errors: u64,
}

/// Redis-based cluster registry for member coordination
pub struct ClusterRegistry {
    redis: Option<ConnectionManager>,
    member_id: String,
    hostname: String,
    address: String,
    pod_ip: String,
    startup_time: SystemTime,
    key_prefix: String,
    ttl_seconds: u64,
}

impl ClusterRegistry {
    pub async fn new(redis: Option<ConnectionManager>, http_addr: SocketAddr) -> Self {
        let member_id = Uuid::new_v4().to_string();

        let hostname = std::env::var("HEIMDALL_POD_NAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        let pod_ip =
            std::env::var("HEIMDALL_POD_IP").unwrap_or_else(|_| http_addr.ip().to_string());

        let address = format!("{}:{}", pod_ip, http_addr.port());

        let key_prefix = std::env::var("HEIMDALL_REDIS_KEY_PREFIX")
            .unwrap_or_else(|_| "heimdall:cluster".to_string());

        info!(
            "Initializing cluster registry: id={}, hostname={}, address={}",
            member_id, hostname, address
        );

        Self {
            redis,
            member_id,
            hostname,
            address,
            pod_ip,
            startup_time: SystemTime::now(),
            key_prefix,
            ttl_seconds: 10, // 10 second TTL, heartbeat every 5 seconds
        }
    }

    /// Register this member in Redis
    pub async fn register(&self, stats: MemberStats) -> Result<(), RedisError> {
        let mut conn = match &self.redis {
            Some(redis) => redis.clone(),
            None => return Ok(()), // No Redis, skip registration
        };

        let member = ClusterMember {
            id: self.member_id.clone(),
            hostname: self.hostname.clone(),
            address: self.address.clone(),
            pod_ip: self.pod_ip.clone(),
            last_heartbeat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            status: MemberStatus::Healthy,
            stats,
        };

        let key = format!("{}:members:{}", self.key_prefix, self.member_id);
        let value = serde_json::to_string(&member).unwrap();

        // Set with TTL
        let _: () = conn.set_ex(&key, value, self.ttl_seconds).await?;

        debug!("Registered cluster member: {}", self.member_id);
        Ok(())
    }

    /// Update member status
    pub async fn update_status(
        &self,
        status: MemberStatus,
        stats: MemberStats,
    ) -> Result<(), RedisError> {
        let mut conn = match &self.redis {
            Some(redis) => redis.clone(),
            None => return Ok(()),
        };

        let member = ClusterMember {
            id: self.member_id.clone(),
            hostname: self.hostname.clone(),
            address: self.address.clone(),
            pod_ip: self.pod_ip.clone(),
            last_heartbeat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            status,
            stats,
        };

        let key = format!("{}:members:{}", self.key_prefix, self.member_id);
        let value = serde_json::to_string(&member).unwrap();

        let _: () = conn.set_ex(&key, value, self.ttl_seconds).await?;
        Ok(())
    }

    /// Get all cluster members
    pub async fn get_members(&self) -> Vec<ClusterMember> {
        let mut conn = match &self.redis {
            Some(redis) => redis.clone(),
            None => return vec![],
        };

        let pattern = format!("{}:members:*", self.key_prefix);

        match conn.keys::<_, Vec<String>>(&pattern).await {
            Ok(keys) => {
                let mut members = Vec::new();

                for key in keys {
                    match conn.get::<_, String>(&key).await {
                        Ok(value) => {
                            if let Ok(member) = serde_json::from_str::<ClusterMember>(&value) {
                                members.push(member);
                            }
                        }
                        Err(e) => warn!("Failed to get member {}: {}", key, e),
                    }
                }

                // Sort by hostname for consistent ordering
                members.sort_by(|a, b| a.hostname.cmp(&b.hostname));
                members
            }
            Err(e) => {
                warn!("Failed to list cluster members: {}", e);
                vec![]
            }
        }
    }

    /// Unregister this member (for graceful shutdown)
    pub async fn unregister(&self) -> Result<(), RedisError> {
        let mut conn = match &self.redis {
            Some(redis) => redis.clone(),
            None => return Ok(()),
        };

        let key = format!("{}:members:{}", self.key_prefix, self.member_id);
        let _: () = conn.del(&key).await?;

        info!("Unregistered cluster member: {}", self.member_id);
        Ok(())
    }

    /// Get cluster statistics
    pub async fn get_stats(&self) -> ClusterStats {
        let members = self.get_members().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let total = members.len();
        let healthy = members
            .iter()
            .filter(|m| m.status == MemberStatus::Healthy)
            .count();
        let degraded = members
            .iter()
            .filter(|m| m.status == MemberStatus::Degraded)
            .count();
        let unhealthy = members
            .iter()
            .filter(|m| m.status == MemberStatus::Unhealthy)
            .count();
        let starting = members
            .iter()
            .filter(|m| m.status == MemberStatus::Starting)
            .count();

        // Consider members stale if no heartbeat for 2x TTL
        let stale = members
            .iter()
            .filter(|m| now - m.last_heartbeat > self.ttl_seconds * 2)
            .count();

        ClusterStats {
            total_members: total,
            healthy_members: healthy,
            degraded_members: degraded,
            unhealthy_members: unhealthy,
            starting_members: starting,
            stale_members: stale,
        }
    }

    /// Get member uptime
    pub fn get_uptime(&self) -> Duration {
        self.startup_time.elapsed().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterStats {
    pub total_members: usize,
    pub healthy_members: usize,
    pub degraded_members: usize,
    pub unhealthy_members: usize,
    pub starting_members: usize,
    pub stale_members: usize,
}

/// Background task to send heartbeats
pub async fn heartbeat_task(
    registry: Arc<ClusterRegistry>,
    resolver: Arc<crate::resolver::DnsResolver>,
) {
    let mut interval = time::interval(Duration::from_secs(5)); // Heartbeat every 5 seconds
    let mut first = true;

    loop {
        interval.tick().await;

        // Collect stats from resolver
        let cache_stats = resolver.cache_stats();
        let stats = MemberStats {
            uptime_seconds: registry.get_uptime().as_secs(),
            queries_total: resolver.total_queries(),
            cache_hits: cache_stats
                .as_ref()
                .map(|s| s.hits.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(0),
            cache_misses: cache_stats
                .as_ref()
                .map(|s| s.misses.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(0),
            cache_size: resolver.cache_size().unwrap_or(0),
            upstream_errors: resolver.total_errors(),
        };

        // Determine health status based on resolver health
        let health_stats = resolver.get_server_health_stats();
        let healthy_upstreams = health_stats.values().filter(|s| s.is_healthy).count();

        let status = if first {
            first = false;
            MemberStatus::Starting
        } else if healthy_upstreams == 0 {
            MemberStatus::Unhealthy
        } else if healthy_upstreams < health_stats.len() {
            MemberStatus::Degraded
        } else {
            MemberStatus::Healthy
        };

        // Send heartbeat
        if let Err(e) = registry.update_status(status, stats).await {
            error!("Failed to send heartbeat: {}", e);
        }
    }
}
