/// Automatic blocklist updater
use super::{BlocklistFormat, DnsBlocker};
use crate::error::{DnsError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{debug, error, info, warn};

/// Blocklist source configuration
#[derive(Debug, Clone)]
pub struct BlocklistSource {
    /// Name of the blocklist
    pub name: String,
    /// URL to download from
    pub url: String,
    /// Local file path
    pub path: PathBuf,
    /// Format of the blocklist
    pub format: BlocklistFormat,
    /// Update interval (None means no auto-update)
    pub update_interval: Option<Duration>,
    /// Whether this list is enabled
    pub enabled: bool,
}

/// Blocklist updater
pub struct BlocklistUpdater {
    /// Blocklist sources
    pub sources: Vec<BlocklistSource>,
    /// HTTP client for downloading
    client: reqwest::Client,
    /// Reference to the blocker
    blocker: Arc<DnsBlocker>,
}

impl BlocklistUpdater {
    /// Create a new blocklist updater
    pub fn new(sources: Vec<BlocklistSource>, blocker: Arc<DnsBlocker>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minute timeout for large lists
            .user_agent("Heimdall DNS Server")
            .build()
            .unwrap_or_default();

        Self {
            sources,
            client,
            blocker,
        }
    }

    /// Download and update a single blocklist
    pub async fn update_blocklist(&self, source: &BlocklistSource) -> Result<()> {
        if !source.enabled {
            debug!("Skipping disabled blocklist: {}", source.name);
            return Ok(());
        }

        info!("Updating blocklist: {} from {}", source.name, source.url);

        // Create parent directory if it doesn't exist
        if let Some(parent) = source.path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| DnsError::Io(format!("Failed to create directory: {}", e)))?;
        }

        // Download to temporary file first
        let temp_path = source.path.with_extension("tmp");

        match self.download_to_file(&source.url, &temp_path).await {
            Ok(_) => {
                // Move temp file to final location
                fs::rename(&temp_path, &source.path)
                    .await
                    .map_err(|e| DnsError::Io(format!("Failed to move file: {}", e)))?;

                // Reload the blocklist
                match self
                    .blocker
                    .load_blocklist(&source.path, source.format, &source.name)
                {
                    Ok(count) => {
                        info!("Successfully loaded {} domains from {}", count, source.name);
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to load blocklist {}: {}", source.name, e);
                        Err(e)
                    }
                }
            }
            Err(e) => {
                // Clean up temp file
                let _ = fs::remove_file(&temp_path).await;
                error!("Failed to download blocklist {}: {}", source.name, e);
                Err(e)
            }
        }
    }

    /// Download URL to file
    async fn download_to_file(&self, url: &str, path: &Path) -> Result<()> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| DnsError::Io(format!("Failed to download: {}", e)))?;

        if !response.status().is_success() {
            return Err(DnsError::Io(format!(
                "HTTP error {}: {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            )));
        }

        let content = response
            .bytes()
            .await
            .map_err(|e| DnsError::Io(format!("Failed to read response: {}", e)))?;

        fs::write(path, content)
            .await
            .map_err(|e| DnsError::Io(format!("Failed to write file: {}", e)))?;

        Ok(())
    }

    /// Update all enabled blocklists
    pub async fn update_all(&self) -> Result<()> {
        let mut success_count = 0;
        let mut error_count = 0;

        for source in &self.sources {
            if source.enabled {
                match self.update_blocklist(source).await {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        error_count += 1;
                        warn!("Failed to update blocklist {}: {}", source.name, e);
                    }
                }
            }
        }

        info!(
            "Blocklist update complete: {} successful, {} failed",
            success_count, error_count
        );

        if error_count > 0 && success_count == 0 {
            Err(DnsError::Io("All blocklist updates failed".to_string()))
        } else {
            Ok(())
        }
    }

    /// Load all blocklists from disk (without downloading)
    pub async fn load_all(&self) -> Result<()> {
        self.blocker.clear_blocklists();

        let mut total_loaded = 0;
        for source in &self.sources {
            if source.enabled && source.path.exists() {
                match self
                    .blocker
                    .load_blocklist(&source.path, source.format, &source.name)
                {
                    Ok(count) => {
                        total_loaded += count;
                        info!("Loaded {} domains from {}", count, source.name);
                    }
                    Err(e) => {
                        warn!("Failed to load blocklist {}: {}", source.name, e);
                    }
                }
            }
        }

        info!("Total domains loaded: {}", total_loaded);
        Ok(())
    }

    /// Start automatic update task
    pub async fn start_auto_update(self: Arc<Self>) {
        // Find the shortest update interval
        let min_interval = self
            .sources
            .iter()
            .filter_map(|s| s.update_interval)
            .min()
            .unwrap_or(Duration::from_secs(86400)); // Default to 24 hours

        let mut update_interval = interval(min_interval);
        update_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(
            "Starting blocklist auto-updater with interval: {:?}",
            min_interval
        );

        loop {
            update_interval.tick().await;

            for source in &self.sources {
                if source.enabled && source.update_interval.is_some() {
                    tokio::spawn({
                        let updater = Arc::clone(&self);
                        let source = source.clone();
                        async move {
                            if let Err(e) = updater.update_blocklist(&source).await {
                                error!("Auto-update failed for {}: {}", source.name, e);
                            }
                        }
                    });
                }
            }
        }
    }
}

/// Default blocklist sources
pub fn default_blocklist_sources() -> Vec<BlocklistSource> {
    vec![
        BlocklistSource {
            name: "Multi Ultimate".to_string(),
            url: "https://raw.githubusercontent.com/hagezi/dns-blocklists/main/hosts/ultimate.txt"
                .to_string(),
            path: PathBuf::from("blocklists/multi-ultimate.txt"),
            format: BlocklistFormat::Hosts,
            update_interval: Some(Duration::from_secs(86400)), // 24 hours
            enabled: true,
        },
        BlocklistSource {
            name: "StevenBlack".to_string(),
            url: "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts".to_string(),
            path: PathBuf::from("blocklists/stevenblack-hosts.txt"),
            format: BlocklistFormat::Hosts,
            update_interval: Some(Duration::from_secs(86400)), // 24 hours
            enabled: true,
        },
        BlocklistSource {
            name: "AdGuard DNS".to_string(),
            url: "https://adguardteam.github.io/AdGuardSDNSFilter/Filters/filter.txt".to_string(),
            path: PathBuf::from("blocklists/adguard-dns.txt"),
            format: BlocklistFormat::AdBlockPlus,
            update_interval: Some(Duration::from_secs(86400)),
            enabled: true,
        },
        BlocklistSource {
            name: "EasyList".to_string(),
            url: "https://easylist.to/easylist/easylist.txt".to_string(),
            path: PathBuf::from("blocklists/easylist.txt"),
            format: BlocklistFormat::AdBlockPlus,
            update_interval: Some(Duration::from_secs(86400)),
            enabled: false, // Disabled by default as it's quite large
        },
        BlocklistSource {
            name: "Malware domains".to_string(),
            url: "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-hosts.txt"
                .to_string(),
            path: PathBuf::from("blocklists/malware-domains.txt"),
            format: BlocklistFormat::Hosts,
            update_interval: Some(Duration::from_secs(43200)), // 12 hours
            enabled: true,
        },
    ]
}
