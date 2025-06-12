use heimdall::config::DnsConfig;
use heimdall::config_reload::{ConfigChange, ConfigReloader, handle_config_changes};
use std::io::Write;
use std::net::SocketAddr;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::sync::mpsc;
use tokio::time::timeout;

#[tokio::test]
async fn test_config_reloader_creation() {
    let config = DnsConfig::default();
    let reloader = ConfigReloader::new(config.clone(), None);

    // Get config should return the initial config
    let retrieved = reloader.get_config().await;
    assert_eq!(retrieved.bind_addr, config.bind_addr);
    assert_eq!(retrieved.upstream_servers, config.upstream_servers);
}

#[tokio::test]
async fn test_take_change_receiver() {
    let config = DnsConfig::default();
    let mut reloader = ConfigReloader::new(config, None);

    // Should be able to take receiver once
    let receiver = reloader.take_change_receiver();
    assert!(receiver.is_some());

    // Second call should return None
    let receiver2 = reloader.take_change_receiver();
    assert!(receiver2.is_none());
}

#[tokio::test]
async fn test_reload_from_env() {
    // Set some environment variables
    unsafe {
        std::env::set_var("HEIMDALL_BIND_ADDR", "127.0.0.1:5353");
        std::env::set_var("HEIMDALL_UPSTREAM_SERVERS", "8.8.8.8:53,1.1.1.1:53");
    }

    let initial_config = DnsConfig::default();
    let mut reloader = ConfigReloader::new(initial_config, None);
    let mut change_rx = reloader.take_change_receiver().unwrap();

    // Trigger reload
    reloader.reload_now().await.unwrap();

    // Should receive change notification
    let change = timeout(Duration::from_secs(1), change_rx.recv())
        .await
        .expect("Timeout waiting for change")
        .expect("Expected change notification");

    assert_eq!(
        change.new_config.bind_addr,
        "127.0.0.1:5353".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(change.new_config.upstream_servers.len(), 2);

    // Clean up
    unsafe {
        std::env::remove_var("HEIMDALL_BIND_ADDR");
        std::env::remove_var("HEIMDALL_UPSTREAM_SERVERS");
    }
}

#[tokio::test]
async fn test_reload_from_file() {
    // Create a temporary config file
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
bind_addr = "127.0.0.1:6363"
upstream_servers = ["8.8.8.8:53", "1.1.1.1:53"]
enable_caching = true
max_cache_size = 5000

[rate_limiting]
enable = false
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let initial_config = DnsConfig::default();
    let mut reloader = ConfigReloader::new(
        initial_config,
        Some(temp_file.path().to_string_lossy().to_string()),
    );
    let mut change_rx = reloader.take_change_receiver().unwrap();

    // Trigger reload
    reloader.reload_now().await.unwrap();

    // Should receive change notification
    let change = timeout(Duration::from_secs(1), change_rx.recv())
        .await
        .expect("Timeout waiting for change")
        .expect("Expected change notification");

    assert_eq!(
        change.new_config.bind_addr,
        "127.0.0.1:6363".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(change.new_config.upstream_servers.len(), 2);
    assert!(change.new_config.enable_caching);
    assert_eq!(change.new_config.max_cache_size, 5000);
    assert!(!change.new_config.rate_limit_config.enable_rate_limiting);
}

#[tokio::test]
async fn test_config_change_notification() {
    let old_config = DnsConfig::default();
    let mut new_config = old_config.clone();
    new_config.enable_caching = !old_config.enable_caching;
    new_config.max_cache_size = old_config.max_cache_size * 2;

    let change = ConfigChange {
        old_config: old_config.clone(),
        new_config: new_config.clone(),
    };

    assert_ne!(
        change.old_config.enable_caching,
        change.new_config.enable_caching
    );
    assert_ne!(
        change.old_config.max_cache_size,
        change.new_config.max_cache_size
    );
}

#[tokio::test]
async fn test_handle_config_changes() {
    let (tx, rx) = mpsc::unbounded_channel();

    // Start handler
    let handle = tokio::spawn(handle_config_changes(rx));

    // Send a change
    let old_config = DnsConfig::default();
    let mut new_config = old_config.clone();
    new_config.enable_caching = !old_config.enable_caching;

    let change = ConfigChange {
        old_config,
        new_config,
    };

    tx.send(change).unwrap();

    // Give it time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop sender to end the handler
    drop(tx);

    // Handler should complete
    timeout(Duration::from_secs(1), handle)
        .await
        .expect("Handler timeout")
        .expect("Handler panic");
}

#[tokio::test]
async fn test_invalid_toml_config() {
    // Create a temporary config file with invalid TOML
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "invalid toml {{").unwrap();
    temp_file.flush().unwrap();

    let initial_config = DnsConfig::default();
    let reloader = ConfigReloader::new(
        initial_config,
        Some(temp_file.path().to_string_lossy().to_string()),
    );

    // Reload should fail
    let result = reloader.reload_now().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_config_values() {
    // Create a temporary config file with invalid values
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
bind_addr = "invalid:address"
max_cache_size = -100
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let initial_config = DnsConfig::default();
    let reloader = ConfigReloader::new(
        initial_config,
        Some(temp_file.path().to_string_lossy().to_string()),
    );

    // Reload should fail due to invalid values
    let result = reloader.reload_now().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_config_file_not_found() {
    let initial_config = DnsConfig::default();
    let reloader = ConfigReloader::new(
        initial_config,
        Some("/nonexistent/path/config.toml".to_string()),
    );

    // Reload should fail
    let result = reloader.reload_now().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_http_bind_addr_disabled() {
    // Create a config file that disables HTTP
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
http_bind_addr = "disabled"
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let initial_config = DnsConfig::default();
    let mut reloader = ConfigReloader::new(
        initial_config,
        Some(temp_file.path().to_string_lossy().to_string()),
    );
    let mut change_rx = reloader.take_change_receiver().unwrap();

    // Trigger reload
    reloader.reload_now().await.unwrap();

    // Should receive change notification
    let change = timeout(Duration::from_secs(1), change_rx.recv())
        .await
        .expect("Timeout waiting for change")
        .expect("Expected change notification");

    assert!(change.new_config.http_bind_addr.is_none());
}

#[tokio::test]
async fn test_partial_config_update() {
    // Create a config file with only some values
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
enable_caching = false
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let initial_config = DnsConfig::default();
    let _initial_caching = initial_config.enable_caching;
    let initial_bind_addr = initial_config.bind_addr;

    let mut reloader = ConfigReloader::new(
        initial_config,
        Some(temp_file.path().to_string_lossy().to_string()),
    );
    let mut change_rx = reloader.take_change_receiver().unwrap();

    // Trigger reload
    reloader.reload_now().await.unwrap();

    // Should receive change notification
    let change = timeout(Duration::from_secs(1), change_rx.recv())
        .await
        .expect("Timeout waiting for change")
        .expect("Expected change notification");

    // Only caching should change
    assert!(!change.new_config.enable_caching);
    assert_eq!(change.new_config.bind_addr, initial_bind_addr); // Unchanged
}

#[tokio::test]
async fn test_start_watching_without_file() {
    let config = DnsConfig::default();
    let reloader = ConfigReloader::new(config, None);

    // Should not error when no file path is provided
    let result = reloader.start_watching().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_reload_calls() {
    let initial_config = DnsConfig::default();
    let mut reloader = ConfigReloader::new(initial_config, None);
    let mut change_rx = reloader.take_change_receiver().unwrap();

    // Multiple reloads should all succeed
    for i in 0..3 {
        unsafe {
            std::env::set_var("HEIMDALL_MAX_CACHE_SIZE", format!("{}", 1000 * (i + 1)));
        }
        reloader.reload_now().await.unwrap();

        let change = timeout(Duration::from_secs(1), change_rx.recv())
            .await
            .expect("Timeout waiting for change")
            .expect("Expected change notification");

        assert_eq!(change.new_config.max_cache_size, 1000 * (i + 1));
    }

    unsafe {
        std::env::remove_var("HEIMDALL_MAX_CACHE_SIZE");
    }
}
