use crate::{
    config::DnsConfig,
    metrics::DnsMetrics,
    pool::BufferPool,
    protocol::{tcp::TcpProtocolHandler, udp::UdpProtocolHandler},
    rate_limiter::DnsRateLimiter,
    resolver::DnsResolver,
};
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{Semaphore, broadcast};
use tracing::{error, info};

/// Run UDP server with graceful shutdown support using the new protocol handler
pub async fn run_udp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    _query_semaphore: Arc<Semaphore>,
    _rate_limiter: Arc<DnsRateLimiter>,
    metrics: Arc<DnsMetrics>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind UDP socket
    let socket = Arc::new(UdpSocket::bind(config.bind_addr).await?);
    info!("UDP DNS server listening on {}", config.bind_addr);

    // Create buffer pool
    let buffer_pool = Arc::new(BufferPool::new(128, 4096));

    // Create UDP protocol handler
    let handler = Arc::new(UdpProtocolHandler::new(
        socket,
        Arc::new(config),
        buffer_pool,
        resolver,
        metrics.clone(),
    ));

    // Run server with shutdown support
    tokio::select! {
        result = handler.run_server(metrics) => {
            if let Err(e) = result {
                error!("UDP server error: {}", e);
                return Err(Box::new(e));
            }
        }
        _ = shutdown_rx.recv() => {
            info!("UDP server received shutdown signal");
            info!("UDP server shutdown complete");
        }
    }

    Ok(())
}

/// Run TCP server with graceful shutdown support using the new protocol handler
pub async fn run_tcp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    _query_semaphore: Arc<Semaphore>,
    _rate_limiter: Arc<DnsRateLimiter>,
    metrics: Arc<DnsMetrics>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind TCP listener
    let listener = Arc::new(TcpListener::bind(config.bind_addr).await?);
    info!("TCP DNS server listening on {}", config.bind_addr);

    // Create buffer pool
    let buffer_pool = Arc::new(BufferPool::new(128, 4096));

    // Create TCP protocol handler
    let handler = Arc::new(TcpProtocolHandler::new(
        listener,
        Arc::new(config),
        buffer_pool,
        resolver,
        metrics.clone(),
    ));

    // Run server with shutdown support
    tokio::select! {
        result = handler.run_server(metrics) => {
            if let Err(e) = result {
                error!("TCP server error: {}", e);
                return Err(Box::new(e));
            }
        }
        _ = shutdown_rx.recv() => {
            info!("TCP server received shutdown signal");
            info!("TCP server shutdown complete");
        }
    }

    Ok(())
}
