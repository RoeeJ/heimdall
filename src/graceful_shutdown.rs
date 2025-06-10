use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::resolver::DnsResolver;

/// Graceful shutdown coordinator
pub struct GracefulShutdown {
    shutdown_tx: broadcast::Sender<()>,
    components: Arc<Mutex<Vec<ShutdownComponent>>>,
    resolver: Arc<DnsResolver>,
}

/// Type alias for shutdown function result
type ShutdownResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Type alias for shutdown function
type ShutdownFn = Box<dyn Fn() -> tokio::task::JoinHandle<ShutdownResult> + Send + Sync>;

/// A component that needs to be shut down gracefully
struct ShutdownComponent {
    name: String,
    shutdown_fn: ShutdownFn,
}

impl GracefulShutdown {
    pub fn new(resolver: Arc<DnsResolver>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            shutdown_tx,
            components: Arc::new(Mutex::new(Vec::new())),
            resolver,
        }
    }

    /// Get a shutdown receiver for components to listen on
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Register a component for graceful shutdown
    pub async fn register_component<F, Fut>(&self, name: String, shutdown_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ShutdownResult> + Send + 'static,
    {
        let component = ShutdownComponent {
            name,
            shutdown_fn: Box::new(move || {
                let fut = shutdown_fn();
                tokio::spawn(fut)
            }),
        };

        self.components.lock().await.push(component);
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Initiating graceful shutdown...");

        // Step 1: Signal all components to stop accepting new requests
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // Step 2: Wait a bit for in-flight requests to complete
        info!("Waiting for in-flight requests to complete...");
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Step 3: Shut down registered components
        let components = self.components.lock().await;
        let mut handles = Vec::new();

        for component in components.iter() {
            info!("Shutting down component: {}", component.name);
            let handle = (component.shutdown_fn)();
            handles.push((component.name.clone(), handle));
        }

        // Wait for all components to shut down (with timeout)
        for (name, handle) in handles {
            match timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(Ok(()))) => {
                    info!("Component '{}' shut down successfully", name);
                }
                Ok(Ok(Err(e))) => {
                    error!("Component '{}' shutdown failed: {}", name, e);
                }
                Ok(Err(e)) => {
                    error!("Component '{}' shutdown task panicked: {}", name, e);
                }
                Err(_) => {
                    warn!("Component '{}' shutdown timed out", name);
                }
            }
        }

        // Step 4: Save cache
        info!("Saving cache before shutdown...");
        if let Err(e) = self.resolver.save_cache().await {
            error!("Failed to save cache during shutdown: {}", e);
        } else {
            info!("Cache saved successfully during shutdown");
        }

        // Step 5: Final cleanup
        info!("Final cleanup...");
        tokio::time::sleep(Duration::from_millis(100)).await;

        info!("Graceful shutdown completed");
        Ok(())
    }
}
