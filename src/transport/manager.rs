use crate::{
    config::DnsConfig,
    error::DnsError,
    metrics::DnsMetrics,
    pool::BufferPool,
    protocol::{doh::DohProtocolHandler, dot::DotProtocolHandler},
    resolver::DnsResolver,
    transport::TransportConfig,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Transport Manager using new protocol handlers
pub struct TransportManager {
    config: TransportConfig,
    shutdown_tx: broadcast::Sender<()>,
}

impl TransportManager {
    /// Create a new transport manager
    pub fn new(config: TransportConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            config,
            shutdown_tx,
        }
    }

    /// Start all configured transport servers
    pub async fn start_servers(
        &self,
        resolver: Arc<DnsResolver>,
        metrics: Option<Arc<DnsMetrics>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut tasks = Vec::new();
        let metrics = metrics
            .unwrap_or_else(|| Arc::new(DnsMetrics::new().expect("Failed to create metrics")));

        // Create a shared buffer pool for all transport protocols
        let buffer_pool = Arc::new(BufferPool::new(4096, 128));

        // Create a minimal DnsConfig for the protocol handlers
        let dns_config = Arc::new(DnsConfig::default());

        // Start DoT server if enabled
        if self.config.enable_dot {
            if let (Some(bind_addr), Some(tls_config)) =
                (&self.config.dot_bind_addr, &self.config.tls_config)
            {
                info!("Starting DoT server on {}", bind_addr);

                // Create TLS acceptor
                let tls_acceptor = match tls_config.create_acceptor().await {
                    Ok(acceptor) => Arc::new(acceptor),
                    Err(e) => {
                        error!("Failed to create TLS acceptor: {}", e);
                        return Err(Box::new(DnsError::Io(format!(
                            "TLS acceptor creation failed: {}",
                            e
                        ))));
                    }
                };

                let listener = Arc::new(TcpListener::bind(bind_addr).await?);

                let handler = Arc::new(DotProtocolHandler::new(
                    listener,
                    tls_acceptor,
                    dns_config.clone(),
                    buffer_pool.clone(),
                    resolver.clone(),
                    metrics.clone(),
                )?);

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                let metrics_clone = metrics.clone();

                let dot_task = tokio::spawn(async move {
                    tokio::select! {
                        result = handler.run_server(metrics_clone) => {
                            if let Err(e) = result {
                                error!("DoT server error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("DoT server shutting down");
                        }
                    }
                });
                tasks.push(dot_task);
            } else {
                warn!("DoT enabled but missing bind address or TLS config");
            }
        }

        // Start DoH server if enabled
        if self.config.enable_doh {
            if let Some(bind_addr) = self.config.doh_bind_addr {
                info!("Starting DoH server on {}", bind_addr);

                let listener = Arc::new(TcpListener::bind(bind_addr).await?);

                let handler = Arc::new(DohProtocolHandler::new(
                    listener,
                    dns_config.clone(),
                    buffer_pool.clone(),
                    resolver.clone(),
                    metrics.clone(),
                ));

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                let metrics_clone = metrics.clone();

                let doh_task = tokio::spawn(async move {
                    tokio::select! {
                        result = handler.run_server(metrics_clone) => {
                            if let Err(e) = result {
                                error!("DoH server error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            info!("DoH server shutting down");
                        }
                    }
                });
                tasks.push(doh_task);
            } else {
                warn!("DoH enabled but no bind address specified");
            }
        }

        // Wait for all transport servers to complete
        for task in tasks {
            if let Err(e) = task.await {
                error!("Transport server task error: {}", e);
            }
        }

        Ok(())
    }

    /// Trigger shutdown of all transport servers
    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
