use crate::config::DnsConfig;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};

/// Configuration change notification
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub old_config: DnsConfig,
    pub new_config: DnsConfig,
}

/// Configuration hot-reload manager
pub struct ConfigReloader {
    config: Arc<RwLock<DnsConfig>>,
    change_tx: mpsc::UnboundedSender<ConfigChange>,
    change_rx: Option<mpsc::UnboundedReceiver<ConfigChange>>,
    config_file_path: Option<String>,
}

impl ConfigReloader {
    pub fn new(initial_config: DnsConfig, config_file_path: Option<String>) -> Self {
        let (change_tx, change_rx) = mpsc::unbounded_channel();

        Self {
            config: Arc::new(RwLock::new(initial_config)),
            change_tx,
            change_rx: Some(change_rx),
            config_file_path,
        }
    }

    /// Get the current configuration
    pub async fn get_config(&self) -> DnsConfig {
        self.config.read().await.clone()
    }

    /// Take the change receiver (can only be called once)
    pub fn take_change_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ConfigChange>> {
        self.change_rx.take()
    }

    /// Start watching for configuration changes
    pub async fn start_watching(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(config_path) = &self.config_file_path {
            info!("Starting configuration file watcher for: {}", config_path);

            let config_clone = self.config.clone();
            let change_tx = self.change_tx.clone();
            let config_path = config_path.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::watch_config_file(config_clone, change_tx, config_path).await
                {
                    error!("Configuration file watcher error: {}", e);
                }
            });
        }

        // Also start signal handler for manual reload
        self.start_signal_handler().await;

        Ok(())
    }

    /// Watch configuration file for changes
    async fn watch_config_file(
        config: Arc<RwLock<DnsConfig>>,
        change_tx: mpsc::UnboundedSender<ConfigChange>,
        config_path: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut watcher: RecommendedWatcher = Watcher::new(
            move |result: Result<Event, notify::Error>| match result {
                Ok(event) => {
                    if let Err(e) = tx.send(event) {
                        error!("Failed to send file watch event: {}", e);
                    }
                }
                Err(e) => error!("File watch error: {}", e),
            },
            notify::Config::default(),
        )?;

        // Watch the config file directory (watching the file directly can be unreliable)
        let config_dir = Path::new(&config_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));

        watcher.watch(config_dir, RecursiveMode::NonRecursive)?;

        while let Some(event) = rx.recv().await {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    // Check if the event is for our config file
                    if event
                        .paths
                        .iter()
                        .any(|p| p.to_string_lossy().contains(&config_path))
                    {
                        debug!("Configuration file changed: {:?}", event.paths);

                        // Small delay to ensure file write is complete
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                        if let Err(e) =
                            Self::reload_from_file(&config, &change_tx, &config_path).await
                        {
                            error!("Failed to reload configuration: {}", e);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Start signal handler for manual reload (SIGHUP)
    async fn start_signal_handler(&self) {
        let config = self.config.clone();
        let change_tx = self.change_tx.clone();
        let config_file_path = self.config_file_path.clone();

        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{SignalKind, signal};

                let mut sighup =
                    signal(SignalKind::hangup()).expect("Failed to create SIGHUP handler");

                loop {
                    sighup.recv().await;
                    info!("Received SIGHUP, reloading configuration...");

                    if let Some(ref path) = config_file_path {
                        if let Err(e) = Self::reload_from_file(&config, &change_tx, path).await {
                            error!("Failed to reload configuration from SIGHUP: {}", e);
                        }
                    } else {
                        // Reload from environment variables
                        if let Err(e) = Self::reload_from_env(&config, &change_tx).await {
                            error!("Failed to reload configuration from environment: {}", e);
                        }
                    }
                }
            }

            #[cfg(not(unix))]
            {
                // On non-Unix systems, we can't use SIGHUP, so just wait indefinitely
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                }
            }
        });
    }

    /// Reload configuration from file
    async fn reload_from_file(
        config: &Arc<RwLock<DnsConfig>>,
        change_tx: &mpsc::UnboundedSender<ConfigChange>,
        config_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Read and parse the config file
        let config_content = tokio::fs::read_to_string(config_path).await?;

        // Get current config to use as base for partial updates
        let current_config_snapshot = {
            let current = config.read().await;
            current.clone()
        };

        let new_config = Self::parse_config_file(&config_content, &current_config_snapshot)?;

        // Get current config and update
        let old_config = {
            let mut current_config = config.write().await;
            let old = current_config.clone();
            *current_config = new_config.clone();
            old
        };

        info!("Configuration reloaded from file: {}", config_path);

        // Notify about the change
        let change = ConfigChange {
            old_config,
            new_config,
        };

        if let Err(e) = change_tx.send(change) {
            error!("Failed to send configuration change notification: {}", e);
        }

        Ok(())
    }

    /// Reload configuration from environment variables
    async fn reload_from_env(
        config: &Arc<RwLock<DnsConfig>>,
        change_tx: &mpsc::UnboundedSender<ConfigChange>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let new_config = match DnsConfig::from_env() {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("Failed to reload configuration from environment: {}", e);
                return Err(Box::new(e));
            }
        };

        // Get current config and update
        let old_config = {
            let mut current_config = config.write().await;
            let old = current_config.clone();
            *current_config = new_config.clone();
            old
        };

        info!("Configuration reloaded from environment variables");

        // Notify about the change
        let change = ConfigChange {
            old_config,
            new_config,
        };

        if let Err(e) = change_tx.send(change) {
            error!("Failed to send configuration change notification: {}", e);
        }

        Ok(())
    }

    /// Parse configuration from TOML content and apply it to the current config
    fn parse_config_file(
        content: &str,
        current_config: &DnsConfig,
    ) -> Result<DnsConfig, Box<dyn std::error::Error + Send + Sync>> {
        // Parse TOML to get file-based overrides
        let toml_value: toml::Value = toml::from_str(content)?;

        // Start with a clone of the current config instead of defaults
        let mut config = current_config.clone();

        // Apply partial updates from the TOML file
        config
            .apply_partial_update(&toml_value)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Validate the final configuration
        config
            .validate()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(config)
    }

    /// Manually trigger a configuration reload
    pub async fn reload_now(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref path) = self.config_file_path {
            Self::reload_from_file(&self.config, &self.change_tx, path).await
        } else {
            Self::reload_from_env(&self.config, &self.change_tx).await
        }
    }
}

/// Helper function to handle configuration changes
pub async fn handle_config_changes(mut change_rx: mpsc::UnboundedReceiver<ConfigChange>) {
    while let Some(change) = change_rx.recv().await {
        info!("Processing configuration change...");

        // Log what changed
        if change.old_config.bind_addr != change.new_config.bind_addr {
            warn!(
                "DNS bind address changed: {} -> {} (requires restart)",
                change.old_config.bind_addr, change.new_config.bind_addr
            );
        }

        if change.old_config.upstream_servers != change.new_config.upstream_servers {
            info!(
                "Upstream servers changed: {:?} -> {:?}",
                change.old_config.upstream_servers, change.new_config.upstream_servers
            );
        }

        if change.old_config.enable_caching != change.new_config.enable_caching {
            info!(
                "Caching enabled changed: {} -> {}",
                change.old_config.enable_caching, change.new_config.enable_caching
            );
        }

        if change.old_config.max_cache_size != change.new_config.max_cache_size {
            info!(
                "Cache size changed: {} -> {}",
                change.old_config.max_cache_size, change.new_config.max_cache_size
            );
        }

        if change.old_config.rate_limit_config.enable_rate_limiting
            != change.new_config.rate_limit_config.enable_rate_limiting
        {
            info!(
                "Rate limiting enabled changed: {} -> {}",
                change.old_config.rate_limit_config.enable_rate_limiting,
                change.new_config.rate_limit_config.enable_rate_limiting
            );
        }

        if change.old_config.http_bind_addr != change.new_config.http_bind_addr {
            warn!(
                "HTTP bind address changed: {:?} -> {:?} (requires restart)",
                change.old_config.http_bind_addr, change.new_config.http_bind_addr
            );
        }

        // For now, we log the changes. In a full implementation, we would:
        // 1. Update the resolver with new upstream servers
        // 2. Adjust cache settings
        // 3. Update rate limiting configuration
        // 4. Handle other dynamic settings

        warn!("Note: Some configuration changes may require a server restart to take full effect");
    }
}
