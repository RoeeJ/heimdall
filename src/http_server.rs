use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc, time::SystemTime};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::{
    cluster_registry::ClusterRegistry, config_reload::ConfigReloader, metrics::DnsMetrics,
    rate_limiter::DnsRateLimiter, resolver::DnsResolver,
};
use tracing::debug;

/// HTTP server for metrics export and health checks
pub struct HttpServer {
    resolver: Arc<DnsResolver>,
    rate_limiter: Option<Arc<DnsRateLimiter>>,
    metrics: Arc<DnsMetrics>,
    config_reloader: Option<Arc<ConfigReloader>>,
    cluster_registry: Option<Arc<ClusterRegistry>>,
    bind_addr: SocketAddr,
}

impl HttpServer {
    pub fn new(
        resolver: Arc<DnsResolver>,
        rate_limiter: Option<Arc<DnsRateLimiter>>,
        metrics: Arc<DnsMetrics>,
        config_reloader: Option<Arc<ConfigReloader>>,
        bind_addr: SocketAddr,
    ) -> Self {
        Self {
            resolver,
            rate_limiter,
            metrics,
            config_reloader,
            cluster_registry: None, // Will be initialized in start()
            bind_addr,
        }
    }

    /// Start the HTTP server
    pub async fn start(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize cluster registry if in Kubernetes and Redis is available
        let cluster_registry = if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
            // Get Redis connection
            let redis_conn = crate::cache::redis_helper::get_redis_connection().await;
            if redis_conn.is_some() {
                let registry = Arc::new(ClusterRegistry::new(redis_conn, self.bind_addr).await);

                // Start heartbeat task
                let registry_clone = registry.clone();
                let resolver_clone = self.resolver.clone();
                tokio::spawn(crate::cluster_registry::heartbeat_task(
                    registry_clone,
                    resolver_clone,
                ));

                info!("Cluster registry initialized with Redis-based coordination");
                Some(registry)
            } else {
                info!("Cluster registry disabled: Redis not available");
                None
            }
        } else {
            debug!("Cluster registry disabled: not running in Kubernetes");
            None
        };

        self.cluster_registry = cluster_registry;

        let app_state = AppState {
            resolver: self.resolver,
            rate_limiter: self.rate_limiter,
            metrics: self.metrics,
            config_reloader: self.config_reloader,
            cluster_registry: self.cluster_registry,
            startup_time: SystemTime::now(),
        };

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/health/detailed", get(detailed_health_check))
            .route("/metrics", get(prometheus_metrics))
            .route("/stats", get(server_stats))
            .route("/cache/stats", get(cache_stats))
            .route("/upstream/stats", get(upstream_stats))
            .route("/config/reload", axum::routing::post(reload_config))
            .with_state(app_state.clone())
            .layer(CorsLayer::permissive());

        info!("Starting HTTP server on {}", self.bind_addr);

        let listener = tokio::net::TcpListener::bind(self.bind_addr).await?;

        // Create a shutdown signal handler
        let cluster_registry = app_state.cluster_registry.clone();
        let shutdown_signal = async move {
            // Wait for ctrl-c or other shutdown signals
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl-c");

            // Unregister from cluster if we have a registry
            if let Some(registry) = cluster_registry {
                if let Err(e) = registry.unregister().await {
                    error!("Failed to unregister from cluster: {}", e);
                } else {
                    info!("Unregistered from cluster");
                }
            }
        };

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await?;

        Ok(())
    }
}

#[derive(Clone)]
struct AppState {
    resolver: Arc<DnsResolver>,
    rate_limiter: Option<Arc<DnsRateLimiter>>,
    metrics: Arc<DnsMetrics>,
    config_reloader: Option<Arc<ConfigReloader>>,
    cluster_registry: Option<Arc<ClusterRegistry>>,
    startup_time: SystemTime,
}

/// Basic health check endpoint
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    // Update metrics before health check
    state
        .metrics
        .update_from_resolver(&state.resolver, state.rate_limiter.as_deref())
        .await;

    // Simple health check - server is healthy if it can respond
    (StatusCode::OK, Json(json!({"status": "healthy"})))
}

/// Detailed health check with comprehensive status
async fn detailed_health_check(State(state): State<AppState>) -> impl IntoResponse {
    state
        .metrics
        .update_from_resolver(&state.resolver, state.rate_limiter.as_deref())
        .await;

    let health_stats = state.resolver.get_server_health_stats();
    let pool_stats = state.resolver.connection_pool_stats().await;

    let mut upstream_health = json!({});
    let mut overall_healthy = true;

    for (server, stats) in &health_stats {
        upstream_health[server.to_string()] = json!({
            "healthy": stats.is_healthy,
            "consecutive_failures": stats.consecutive_failures,
            "success_rate": stats.success_rate,
            "total_requests": stats.total_requests,
            "successful_responses": stats.successful_responses,
            "avg_response_time_ms": stats.avg_response_time.map(|d| d.as_millis())
        });

        if !stats.is_healthy {
            overall_healthy = false;
        }
    }

    let rate_limiter_stats = state
        .rate_limiter
        .as_ref()
        .map(|limiter| limiter.get_stats());

    // Build cache info
    let cache_info = if let Some(cache_stats) = state.resolver.cache_stats() {
        use std::sync::atomic::Ordering;
        json!({
            "size": state.resolver.cache_size().unwrap_or(0),
            "hits": cache_stats.hits.load(Ordering::Relaxed),
            "misses": cache_stats.misses.load(Ordering::Relaxed),
            "hit_rate": cache_stats.hit_rate(),
            "evictions": cache_stats.evictions.load(Ordering::Relaxed),
            "expired_evictions": cache_stats.expired_evictions.load(Ordering::Relaxed)
        })
    } else {
        json!({
            "size": 0,
            "hits": 0,
            "misses": 0,
            "hit_rate": 0.0,
            "evictions": 0,
            "expired_evictions": 0
        })
    };

    // Get cluster information if available
    let cluster_info = if let Some(ref registry) = state.cluster_registry {
        let members = registry.get_members().await;
        let stats = registry.get_stats().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Some(json!({
            "enabled": true,
            "stats": {
                "total_members": stats.total_members,
                "healthy_members": stats.healthy_members,
                "degraded_members": stats.degraded_members,
                "unhealthy_members": stats.unhealthy_members,
                "starting_members": stats.starting_members,
                "stale_members": stats.stale_members
            },
            "members": members.iter().map(|member| json!({
                "id": member.id,
                "address": member.address,
                "hostname": member.hostname,
                "pod_ip": member.pod_ip,
                "status": format!("{:?}", member.status).to_lowercase(),
                "last_heartbeat_seconds_ago": now.saturating_sub(member.last_heartbeat),
                "uptime_seconds": member.stats.uptime_seconds,
                "queries_total": member.stats.queries_total,
                "cache_hits": member.stats.cache_hits,
                "cache_misses": member.stats.cache_misses,
                "cache_size": member.stats.cache_size,
                "cache_hit_rate": if member.stats.cache_hits + member.stats.cache_misses > 0 {
                    member.stats.cache_hits as f64 / (member.stats.cache_hits + member.stats.cache_misses) as f64
                } else { 0.0 }
            })).collect::<Vec<_>>()
        }))
    } else {
        None
    };

    let response = json!({
        "status": if overall_healthy { "healthy" } else { "degraded" },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "cache": cache_info,
        "upstream_servers": upstream_health,
        "connection_pools": pool_stats.iter().map(|(server, count)| {
            (server.to_string(), count)
        }).collect::<std::collections::HashMap<_, _>>(),
        "rate_limiter": rate_limiter_stats.map(|stats| json!({
            "active_ip_limiters": stats.active_ip_limiters,
            "active_error_limiters": stats.active_error_limiters,
            "active_nxdomain_limiters": stats.active_nxdomain_limiters
        })),
        "cluster": cluster_info.unwrap_or_else(|| json!({ "enabled": false }))
    });

    let status_code = if overall_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
}

/// Prometheus metrics endpoint
async fn prometheus_metrics(State(state): State<AppState>) -> impl IntoResponse {
    // Update metrics before export
    state
        .metrics
        .update_from_resolver(&state.resolver, state.rate_limiter.as_deref())
        .await;

    match state.metrics.export() {
        Ok(metrics) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(metrics)
            .unwrap(),
        Err(e) => {
            error!("Failed to export metrics: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Failed to export metrics".to_string())
                .unwrap()
        }
    }
}

/// JSON server statistics endpoint
async fn server_stats(State(state): State<AppState>) -> impl IntoResponse {
    let health_stats = state.resolver.get_server_health_stats();
    let pool_stats = state.resolver.connection_pool_stats().await;
    let rate_limiter_stats = state
        .rate_limiter
        .as_ref()
        .map(|limiter| limiter.get_stats());

    // Build cache info
    let cache_info = if let Some(cache_stats) = state.resolver.cache_stats() {
        use std::sync::atomic::Ordering;
        json!({
            "size": state.resolver.cache_size().unwrap_or(0),
            "hits": cache_stats.hits.load(Ordering::Relaxed),
            "misses": cache_stats.misses.load(Ordering::Relaxed),
            "hit_rate": cache_stats.hit_rate(),
            "evictions": cache_stats.evictions.load(Ordering::Relaxed),
            "expired_evictions": cache_stats.expired_evictions.load(Ordering::Relaxed)
        })
    } else {
        json!({
            "size": 0,
            "hits": 0,
            "misses": 0,
            "hit_rate": 0.0,
            "evictions": 0,
            "expired_evictions": 0
        })
    };

    let response = json!({
        "server": {
            "name": "Heimdall DNS Server",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_seconds": SystemTime::now()
                .duration_since(state.startup_time)
                .unwrap_or_default()
                .as_secs()
        },
        "cache": cache_info,
        "upstream_servers": health_stats.iter().map(|(server, stats)| {
            (server.to_string(), json!({
                "healthy": stats.is_healthy,
                "consecutive_failures": stats.consecutive_failures,
                "success_rate": stats.success_rate,
                "total_requests": stats.total_requests,
                "successful_responses": stats.successful_responses,
                "avg_response_time_ms": stats.avg_response_time.map(|d| d.as_millis())
            }))
        }).collect::<std::collections::HashMap<_, _>>(),
        "connection_pools": pool_stats.iter().map(|(server, count)| {
            (server.to_string(), count)
        }).collect::<std::collections::HashMap<_, _>>(),
        "rate_limiter": rate_limiter_stats.map(|stats| json!({
            "active_ip_limiters": stats.active_ip_limiters,
            "active_error_limiters": stats.active_error_limiters,
            "active_nxdomain_limiters": stats.active_nxdomain_limiters
        })),
        "cluster": if let Some(ref registry) = state.cluster_registry {
            let stats = registry.get_stats().await;
            json!({
                "enabled": true,
                "total_members": stats.total_members,
                "healthy_members": stats.healthy_members,
                "degraded_members": stats.degraded_members,
                "unhealthy_members": stats.unhealthy_members,
                "starting_members": stats.starting_members,
                "stale_members": stats.stale_members
            })
        } else {
            json!({ "enabled": false })
        }
    });

    Json(response)
}

/// Cache-specific statistics endpoint
async fn cache_stats(State(state): State<AppState>) -> impl IntoResponse {
    let debug_info = state.resolver.cache_info();

    // Build cache statistics
    let cache_statistics = if let Some(cache_stats) = state.resolver.cache_stats() {
        use std::sync::atomic::Ordering;
        json!({
            "size": state.resolver.cache_size().unwrap_or(0),
            "hits": cache_stats.hits.load(Ordering::Relaxed),
            "misses": cache_stats.misses.load(Ordering::Relaxed),
            "hit_rate": cache_stats.hit_rate(),
            "evictions": cache_stats.evictions.load(Ordering::Relaxed),
            "expired_evictions": cache_stats.expired_evictions.load(Ordering::Relaxed)
        })
    } else {
        json!({
            "size": 0,
            "hits": 0,
            "misses": 0,
            "hit_rate": 0.0,
            "evictions": 0,
            "expired_evictions": 0
        })
    };

    Json(json!({
        "statistics": cache_statistics,
        "debug_info": debug_info
    }))
}

/// Upstream server statistics endpoint
async fn upstream_stats(State(state): State<AppState>) -> impl IntoResponse {
    let health_stats = state.resolver.get_server_health_stats();
    let debug_info = state.resolver.get_health_debug_info();
    let pool_stats = state.resolver.connection_pool_stats().await;

    Json(json!({
        "servers": health_stats.iter().map(|(server, stats)| {
            (server.to_string(), json!({
                "healthy": stats.is_healthy,
                "consecutive_failures": stats.consecutive_failures,
                "success_rate": stats.success_rate,
                "total_requests": stats.total_requests,
                "successful_responses": stats.successful_responses,
                "avg_response_time_ms": stats.avg_response_time.map(|d| d.as_millis()),
                "connection_pool_size": pool_stats.get(server).unwrap_or(&0)
            }))
        }).collect::<std::collections::HashMap<_, _>>(),
        "debug_info": debug_info
    }))
}

/// Configuration reload endpoint
async fn reload_config(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(ref config_reloader) = state.config_reloader {
        match config_reloader.reload_now().await {
            Ok(()) => {
                info!("Configuration reloaded via HTTP endpoint");
                (
                    StatusCode::OK,
                    Json(json!({
                        "status": "success",
                        "message": "Configuration reloaded successfully"
                    })),
                )
            }
            Err(e) => {
                error!("Failed to reload configuration via HTTP: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "status": "error",
                        "message": format!("Failed to reload configuration: {}", e)
                    })),
                )
            }
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "error",
                "message": "Configuration hot-reload is not enabled"
            })),
        )
    }
}
