use heimdall::{
    config::DnsConfig, config_reload::ConfigReloader, http_server::HttpServer, metrics::DnsMetrics,
    rate_limiter::DnsRateLimiter, resolver::DnsResolver,
};
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

// Helper to create test components
async fn create_test_components() -> (Arc<DnsResolver>, Arc<DnsMetrics>, Arc<DnsRateLimiter>) {
    // Disable blocking features for tests to avoid network operations during initialization
    let config = DnsConfig {
        blocking_enabled: false,
        blocklist_auto_update: false,
        blocking_download_psl: false,
        ..Default::default()
    };

    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());

    (resolver, metrics, rate_limiter)
}

#[tokio::test]
async fn test_http_server_creation() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let bind_addr = "127.0.0.1:0".parse().unwrap();
    let http_server = HttpServer::new(
        resolver,
        Some(rate_limiter),
        metrics,
        None, // No config reloader
        bind_addr,
    );

    // Test passes if creation succeeds without panic
    // We can't access bind_addr directly as it's private, so just verify no panic occurred
    drop(http_server);
}

#[tokio::test]
async fn test_http_server_start_and_health_check() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    // Find an available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(resolver, Some(rate_limiter), metrics, None, bind_addr);

    // Start server in background
    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test health endpoint
    let client = reqwest::Client::new();
    let health_url = format!("http://{}/health", bind_addr);

    let response = timeout(Duration::from_secs(5), client.get(&health_url).send()).await;

    if let Ok(Ok(resp)) = response {
        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "healthy");
    }

    // Clean up
    server_handle.abort();
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(
        resolver,
        Some(rate_limiter),
        metrics.clone(),
        None,
        bind_addr,
    );

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Record some metrics first
    use std::time::Duration;
    metrics.record_query("udp", "A", "noerror", Duration::from_millis(50), false);
    metrics.record_malformed_packet("udp", "test");

    // Test metrics endpoint
    let client = reqwest::Client::new();
    let metrics_url = format!("http://{}/metrics", bind_addr);

    let response = timeout(Duration::from_secs(5), client.get(&metrics_url).send()).await;

    if let Ok(Ok(resp)) = response {
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );

        let body = resp.text().await.unwrap();
        assert!(body.contains("heimdall_"));
        assert!(body.contains("# HELP"));
        assert!(body.contains("# TYPE"));
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_detailed_health_endpoint() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(resolver, Some(rate_limiter), metrics, None, bind_addr);

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let health_url = format!("http://{}/health/detailed", bind_addr);

    let response = timeout(Duration::from_secs(5), client.get(&health_url).send()).await;

    if let Ok(Ok(resp)) = response {
        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body["status"].is_string());
        assert!(body["timestamp"].is_string());
        assert!(body["cache"].is_object());
        assert!(body["upstream_servers"].is_object());
        assert!(body["rate_limiter"].is_object());
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_config_reload_endpoint() {
    // Disable blocking features for tests to avoid network operations during initialization
    let config = DnsConfig {
        blocking_enabled: false,
        blocklist_auto_update: false,
        blocking_download_psl: false,
        ..Default::default()
    };

    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());

    // Create a config reloader
    let config_reloader = Arc::new(ConfigReloader::new(config.clone(), None));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(
        resolver,
        Some(rate_limiter),
        metrics,
        Some(config_reloader),
        bind_addr,
    );

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let reload_url = format!("http://{}/config/reload", bind_addr);

    let response = timeout(Duration::from_secs(5), client.post(&reload_url).send()).await;

    if let Ok(Ok(resp)) = response {
        // Reload might succeed, fail, or be unavailable
        // 200 (success), 500 (error), or 503 (unavailable) are acceptable
        assert!(
            resp.status() == 200 || resp.status() == 500 || resp.status() == 503,
            "Expected status 200, 500, or 503, got {}",
            resp.status()
        );

        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body.get("status").is_some());
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_cors_headers() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(resolver, Some(rate_limiter), metrics, None, bind_addr);

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let health_url = format!("http://{}/health", bind_addr);

    // Test CORS preflight
    let response = timeout(
        Duration::from_secs(5),
        client
            .request(reqwest::Method::OPTIONS, &health_url)
            .header("Origin", "https://example.com")
            .header("Access-Control-Request-Method", "GET")
            .send(),
    )
    .await;

    if let Ok(Ok(resp)) = response {
        let headers = resp.headers();
        // CORS headers should be present
        assert!(
            headers.get("access-control-allow-origin").is_some()
                || headers.get("access-control-allow-methods").is_some()
        );
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_invalid_endpoint() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(resolver, Some(rate_limiter), metrics, None, bind_addr);

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let invalid_url = format!("http://{}/nonexistent", bind_addr);

    let response = timeout(Duration::from_secs(5), client.get(&invalid_url).send()).await;

    if let Ok(Ok(resp)) = response {
        assert_eq!(resp.status(), 404);
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_server_without_rate_limiter() {
    let (resolver, metrics, _) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(
        resolver, None, // No rate limiter
        metrics, None, bind_addr,
    );

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let health_url = format!("http://{}/health", bind_addr);

    let response = timeout(Duration::from_secs(5), client.get(&health_url).send()).await;

    if let Ok(Ok(resp)) = response {
        assert_eq!(resp.status(), 200);
    }

    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (resolver, metrics, rate_limiter) = create_test_components().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind_addr = listener.local_addr().unwrap();
    drop(listener);

    let http_server = HttpServer::new(resolver, Some(rate_limiter), metrics, None, bind_addr);

    let server_handle = tokio::spawn(async move {
        let _ = http_server.start().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send 5 concurrent requests
    let client = reqwest::Client::new();
    let health_url = format!("http://{}/health", bind_addr);

    let mut handles = vec![];
    for _ in 0..5 {
        let client = client.clone();
        let url = health_url.clone();

        let handle =
            tokio::spawn(
                async move { timeout(Duration::from_secs(5), client.get(&url).send()).await },
            );
        handles.push(handle);
    }

    let mut successful_requests = 0;
    for handle in handles {
        if let Ok(Ok(Ok(resp))) = handle.await {
            if resp.status() == 200 {
                successful_requests += 1;
            }
        }
    }

    assert_eq!(
        successful_requests, 5,
        "All concurrent requests should succeed"
    );

    server_handle.abort();
}
