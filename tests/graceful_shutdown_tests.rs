use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use heimdall::graceful_shutdown::GracefulShutdown;
use heimdall::resolver::DnsResolver;
use heimdall::config::DnsConfig;

async fn create_test_resolver() -> Arc<DnsResolver> {
    let config = DnsConfig::default();
    Arc::new(DnsResolver::new(config, None).await.expect("Failed to create resolver"))
}

#[tokio::test]
async fn test_graceful_shutdown_creation() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Should be able to subscribe to shutdown signal
    let mut receiver = shutdown.subscribe();
    
    // Initially no shutdown signal should be sent
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn test_subscribe_multiple_receivers() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Create multiple subscribers
    let mut receiver1 = shutdown.subscribe();
    let mut receiver2 = shutdown.subscribe();
    let mut receiver3 = shutdown.subscribe();
    
    // All should initially have no signal
    assert!(receiver1.try_recv().is_err());
    assert!(receiver2.try_recv().is_err());
    assert!(receiver3.try_recv().is_err());
}

#[tokio::test]
async fn test_register_and_shutdown_component() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    let shutdown_called = Arc::new(Mutex::new(false));
    let shutdown_called_clone = shutdown_called.clone();
    
    // Register a simple component
    shutdown.register_component(
        "test_component".to_string(),
        move || {
            let shutdown_called = shutdown_called_clone.clone();
            async move {
                *shutdown_called.lock().await = true;
                Ok(())
            }
        }
    ).await;
    
    // Perform shutdown
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // Verify component was shut down
    assert!(*shutdown_called.lock().await);
}

#[tokio::test]
async fn test_shutdown_multiple_components() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    let counter = Arc::new(Mutex::new(0));
    
    // Register multiple components
    for i in 0..3 {
        let counter_clone = counter.clone();
        shutdown.register_component(
            format!("component_{}", i),
            move || {
                let counter = counter_clone.clone();
                async move {
                    *counter.lock().await += 1;
                    Ok(())
                }
            }
        ).await;
    }
    
    // Perform shutdown
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // All components should have been shut down
    assert_eq!(*counter.lock().await, 3);
}

#[tokio::test]
async fn test_shutdown_signal_broadcast() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Create multiple subscribers
    let mut receivers = vec![
        shutdown.subscribe(),
        shutdown.subscribe(),
        shutdown.subscribe(),
    ];
    
    // Start shutdown
    tokio::spawn(async move {
        shutdown.shutdown().await.unwrap();
    });
    
    // All receivers should get the signal
    for receiver in &mut receivers {
        // Wait up to 2 seconds for the signal
        tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("Timeout waiting for shutdown signal")
            .expect("Failed to receive shutdown signal");
    }
}

#[tokio::test]
async fn test_component_shutdown_error_handling() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Register a component that fails
    shutdown.register_component(
        "failing_component".to_string(),
        || async {
            Err("Component failure".into())
        }
    ).await;
    
    // Register a component that succeeds
    let success_called = Arc::new(Mutex::new(false));
    let success_called_clone = success_called.clone();
    shutdown.register_component(
        "success_component".to_string(),
        move || {
            let success_called = success_called_clone.clone();
            async move {
                *success_called.lock().await = true;
                Ok(())
            }
        }
    ).await;
    
    // Shutdown should complete despite one component failing
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // The successful component should still have been called
    assert!(*success_called.lock().await);
}

#[tokio::test]
async fn test_component_shutdown_timeout() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Register a component that times out
    shutdown.register_component(
        "slow_component".to_string(),
        || async {
            // Sleep longer than the 5-second timeout
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        }
    ).await;
    
    // Start timing
    let start = tokio::time::Instant::now();
    
    // Shutdown should complete within reasonable time despite timeout
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // Should complete in less than 10 seconds (component timeout is 5s)
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(8));
}

#[tokio::test]
async fn test_component_panic_handling() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Register a component that panics
    shutdown.register_component(
        "panicking_component".to_string(),
        || async {
            panic!("Component panic!");
        }
    ).await;
    
    // Shutdown should complete despite panic
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_shutdown_with_no_components() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    // Shutdown with no components registered should succeed
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_concurrent_component_registration() {
    let resolver = create_test_resolver().await;
    let shutdown = Arc::new(GracefulShutdown::new(resolver));
    
    let counter = Arc::new(Mutex::new(0));
    
    // Register components concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let shutdown_clone = shutdown.clone();
        let counter_clone = counter.clone();
        let handle = tokio::spawn(async move {
            shutdown_clone.register_component(
                format!("concurrent_{}", i),
                move || {
                    let counter = counter_clone.clone();
                    async move {
                        *counter.lock().await += 1;
                        Ok(())
                    }
                }
            ).await;
        });
        handles.push(handle);
    }
    
    // Wait for all registrations
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Perform shutdown
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // All components should have been shut down
    assert_eq!(*counter.lock().await, 10);
}

#[tokio::test]
async fn test_shutdown_ordering() {
    let resolver = create_test_resolver().await;
    let shutdown = GracefulShutdown::new(resolver);
    
    let order = Arc::new(Mutex::new(Vec::new()));
    
    // Register components in specific order
    for i in 0..3 {
        let order_clone = order.clone();
        let component_id = i;
        shutdown.register_component(
            format!("ordered_{}", i),
            move || {
                let order = order_clone.clone();
                async move {
                    order.lock().await.push(component_id);
                    Ok(())
                }
            }
        ).await;
    }
    
    // Perform shutdown
    let result = shutdown.shutdown().await;
    assert!(result.is_ok());
    
    // Components should be shut down in registration order
    let final_order = order.lock().await.clone();
    assert_eq!(final_order, vec![0, 1, 2]);
}