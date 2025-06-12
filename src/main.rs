use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod cache;
pub mod cluster_registry;
pub mod config;
pub mod config_reload;
pub mod dns;
pub mod dnssec;
pub mod error;
pub mod graceful_shutdown;
pub mod http_server;
pub mod metrics;
pub mod rate_limiter;
pub mod resolver;
pub mod server;
pub mod validation;
pub mod zone;

use config::DnsConfig;
use config_reload::{ConfigReloader, handle_config_changes};
use graceful_shutdown::GracefulShutdown;
use http_server::HttpServer;
use metrics::DnsMetrics;
use rate_limiter::DnsRateLimiter;
use resolver::DnsResolver;
use server::{run_tcp_server, run_udp_server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration first to get runtime settings
    let config = match DnsConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    // Build custom Tokio runtime with configurable thread pool
    let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();

    // Configure worker threads if specified
    if config.worker_threads > 0 {
        runtime_builder.worker_threads(config.worker_threads);
    }

    // Configure blocking threads if specified
    if config.blocking_threads > 0 {
        runtime_builder.max_blocking_threads(config.blocking_threads);
    }

    // Enable all features and build runtime
    let runtime = runtime_builder
        .enable_all()
        .thread_name("heimdall-worker")
        .build()?;

    // Run the async main function on our custom runtime
    runtime.block_on(async_main(config))
}

async fn async_main(config: DnsConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "heimdall=info,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Heimdall DNS Server starting up");
    info!(
        "Configuration: bind_addr={}, upstream_servers={:?}",
        config.bind_addr, config.upstream_servers
    );
    info!(
        "Runtime configuration: worker_threads={}, blocking_threads={}, max_concurrent_queries={}",
        if config.worker_threads > 0 {
            config.worker_threads.to_string()
        } else {
            "default".to_string()
        },
        config.blocking_threads,
        config.max_concurrent_queries
    );

    // Create metrics registry first
    let metrics = Arc::new(DnsMetrics::new().expect("Failed to create metrics registry"));

    // Create resolver with metrics reference
    let resolver = Arc::new(DnsResolver::new(config.clone(), Some(metrics.clone())).await?);

    // Create semaphore for limiting concurrent queries
    let query_semaphore = Arc::new(Semaphore::new(config.max_concurrent_queries));

    // Create rate limiter
    let rate_limiter = Arc::new(
        match DnsRateLimiter::new(config.rate_limit_config.clone()) {
            Ok(rl) => rl,
            Err(e) => {
                error!("Failed to create rate limiter: {}", e);
                std::process::exit(1);
            }
        },
    );

    info!(
        "Rate limiting enabled: {}, per-IP limit: {} QPS, global limit: {} QPS",
        config.rate_limit_config.enable_rate_limiting,
        config.rate_limit_config.queries_per_second_per_ip,
        config.rate_limit_config.global_queries_per_second
    );

    // Update runtime metrics
    metrics.update_runtime_config(
        if config.worker_threads > 0 {
            config.worker_threads
        } else {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4)
        },
        config.max_concurrent_queries,
    );

    // Set up configuration hot-reloading
    let config_file_path = std::env::var("HEIMDALL_CONFIG_FILE").ok();
    let mut config_reloader = ConfigReloader::new(config.clone(), config_file_path);

    if let Err(e) = config_reloader.start_watching().await {
        warn!("Failed to start configuration watcher: {}", e);
    } else {
        info!("Configuration hot-reload enabled");
        if let Ok(config_path) = std::env::var("HEIMDALL_CONFIG_FILE") {
            info!("Watching config file: {}", config_path);
        }
        info!("Send SIGHUP to reload configuration from environment variables");
    }

    // Start configuration change handler
    let config_change_task = config_reloader
        .take_change_receiver()
        .map(|change_rx| tokio::spawn(handle_config_changes(change_rx)));

    // Wrap config reloader in Arc for sharing with HTTP server
    let config_reloader = Arc::new(config_reloader);

    // Create graceful shutdown coordinator
    let graceful_shutdown = Arc::new(GracefulShutdown::new(resolver.clone()));

    // Start rate limiter cleanup task
    let rate_limiter_cleanup = rate_limiter.clone();
    let cleanup_interval = config.rate_limit_config.cleanup_interval_seconds;
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval));
        loop {
            interval.tick().await;
            rate_limiter_cleanup.cleanup_expired_entries();

            let stats = rate_limiter_cleanup.get_stats();
            if stats.active_ip_limiters > 1000 {
                debug!(
                    "Rate limiter stats: {} active IP limiters, {} error limiters, {} nxdomain limiters",
                    stats.active_ip_limiters,
                    stats.active_error_limiters,
                    stats.active_nxdomain_limiters
                );
            }
        }
    });

    // Start cache persistence task if enabled
    let cache_save_task = if config.cache_save_interval > 0 && resolver.has_cache_persistence() {
        let resolver_cache = resolver.clone();
        let save_interval = config.cache_save_interval;
        info!(
            "Starting cache persistence task: saving every {} seconds",
            save_interval
        );

        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(save_interval));
            loop {
                interval.tick().await;
                if let Err(e) = resolver_cache.save_cache().await {
                    error!("Failed to save cache: {}", e);
                } else {
                    trace!("Cache saved successfully");
                }
            }
        }))
    } else {
        None
    };

    // Start HTTP server for metrics and health checks if enabled
    let http_task = if let Some(http_addr) = config.http_bind_addr {
        info!("Starting HTTP server on {}", http_addr);
        info!("Available endpoints:");
        info!("  http://{}/health - Basic health check", http_addr);
        info!(
            "  http://{}/health/detailed - Detailed health status",
            http_addr
        );
        info!("  http://{}/metrics - Prometheus metrics", http_addr);
        info!("  http://{}/stats - JSON server statistics", http_addr);
        info!("  http://{}/cache/stats - Cache statistics", http_addr);
        info!(
            "  http://{}/upstream/stats - Upstream server statistics",
            http_addr
        );
        info!(
            "  http://{}/cluster/stats - Cluster-wide statistics",
            http_addr
        );
        info!(
            "  POST http://{}/config/reload - Manual configuration reload",
            http_addr
        );

        let http_server = HttpServer::new(
            resolver.clone(),
            Some(rate_limiter.clone()),
            metrics.clone(),
            Some(config_reloader.clone()),
            http_addr,
        );

        Some(tokio::spawn(async move {
            if let Err(e) = http_server.start().await {
                error!("HTTP server error: {:?}", e);
            }
        }))
    } else {
        info!("HTTP server disabled");
        None
    };

    // Start UDP and TCP servers with graceful shutdown support
    let udp_shutdown_rx = graceful_shutdown.subscribe();
    let tcp_shutdown_rx = graceful_shutdown.subscribe();

    let udp_task = tokio::spawn(run_udp_server(
        config.clone(),
        resolver.clone(),
        query_semaphore.clone(),
        rate_limiter.clone(),
        metrics.clone(),
        udp_shutdown_rx,
    ));
    let tcp_task = tokio::spawn(run_tcp_server(
        config.clone(),
        resolver.clone(),
        query_semaphore.clone(),
        rate_limiter.clone(),
        metrics.clone(),
        tcp_shutdown_rx,
    ));

    info!("DNS server listening on {} (UDP and TCP)", config.bind_addr);

    // Setup graceful shutdown signal handling
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for shutdown signal");
        info!("Shutdown signal received, initiating graceful shutdown...");

        // Trigger graceful shutdown
        if let Err(e) = graceful_shutdown.shutdown().await {
            error!("Error during graceful shutdown: {}", e);
        }
    };

    // Wait for any task to exit or shutdown signal
    tokio::select! {
        result = udp_task => {
            error!("UDP server exited: {:?}", result);
        }
        result = tcp_task => {
            error!("TCP server exited: {:?}", result);
        }
        result = cleanup_task => {
            error!("Rate limiter cleanup task exited: {:?}", result);
        }
        result = async {
            if let Some(task) = cache_save_task {
                task.await
            } else {
                // If no cache task, wait forever
                std::future::pending::<Result<(), tokio::task::JoinError>>().await
            }
        } => {
            error!("Cache save task exited: {:?}", result);
        }
        result = async {
            if let Some(task) = http_task {
                task.await
            } else {
                // If no HTTP task, wait forever
                std::future::pending::<Result<(), tokio::task::JoinError>>().await
            }
        } => {
            error!("HTTP server exited: {:?}", result);
        }
        result = async {
            if let Some(task) = config_change_task {
                task.await
            } else {
                // If no config change task, wait forever
                std::future::pending::<Result<(), tokio::task::JoinError>>().await
            }
        } => {
            error!("Configuration change handler exited: {:?}", result);
        }
        _ = shutdown_signal => {
            info!("Heimdall DNS server shutting down gracefully");
        }
    }

    Ok(())
}
