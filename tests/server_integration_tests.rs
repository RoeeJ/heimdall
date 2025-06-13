// Server integration tests
//
// These tests start actual DNS servers and may make network calls.
// They are marked with #[ignore] to prevent them from running in CI
// or when running the standard test suite.
//
// To run these tests locally:
//   cargo test --test server_integration_tests -- --ignored
//
// Note: These tests may fail if:
// - Port binding fails (another process using the port)
// - Network access is restricted
// - DNS servers (8.8.8.8) are unreachable

use heimdall::{
    config::DnsConfig,
    dns::DNSPacket,
    dns::enums::ResponseCode,
    metrics::DnsMetrics,
    rate_limiter::DnsRateLimiter,
    resolver::DnsResolver,
    server::{run_tcp_server, run_udp_server},
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::{Semaphore, broadcast},
    time::timeout,
};

// Helper to create a basic DNS query packet bytes
fn create_dns_query_bytes(domain: &str, query_type: u16) -> Vec<u8> {
    let mut packet = Vec::new();

    // Header (12 bytes)
    packet.extend_from_slice(&[0x12, 0x34]); // ID
    packet.extend_from_slice(&[0x01, 0x00]); // Flags: QR=0, OPCODE=0, RD=1
    packet.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    packet.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0

    // Question section
    for label in domain.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0); // Root label

    packet.extend_from_slice(&query_type.to_be_bytes()); // QTYPE
    packet.extend_from_slice(&[0x00, 0x01]); // QCLASS = IN

    packet
}

// Helper to start a test server with random port
async fn start_test_server(
    tcp: bool,
) -> (
    SocketAddr,
    broadcast::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let mut config = DnsConfig::default();

    // Find an available port
    let addr = if tcp {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        addr
    } else {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        drop(socket);
        addr
    };

    config.bind_addr = addr;
    config.upstream_servers = vec!["8.8.8.8:53".parse().unwrap()];
    config.upstream_timeout = Duration::from_secs(2);

    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );
    let query_semaphore = Arc::new(Semaphore::new(100));
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let handle = if tcp {
        tokio::spawn(async move {
            let _ = run_tcp_server(
                config,
                resolver,
                query_semaphore,
                rate_limiter,
                metrics,
                shutdown_rx,
            )
            .await;
        })
    } else {
        tokio::spawn(async move {
            let _ = run_udp_server(
                config,
                resolver,
                query_semaphore,
                rate_limiter,
                metrics,
                shutdown_rx,
            )
            .await;
        })
    };

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    (addr, shutdown_tx, handle)
}

#[tokio::test]
#[ignore] // This test requires starting a server
async fn test_udp_server_basic_query() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Send A record query for example.com
    let query = create_dns_query_bytes("example.com", 1);
    socket.send_to(&query, addr).await.unwrap();

    // Receive response
    let mut buf = vec![0; 4096];
    let result = timeout(Duration::from_secs(10), socket.recv_from(&mut buf)).await;

    assert!(result.is_ok(), "Should receive response");
    let (size, _) = result.unwrap().unwrap();
    assert!(size >= 12, "Response should have at least header");

    // Verify response can be parsed
    let response = DNSPacket::parse(&buf[..size]);
    assert!(response.is_ok(), "Response should be valid DNS packet");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // This test requires starting a server
async fn test_tcp_server_basic_query() {
    let (addr, shutdown_tx, _handle) = start_test_server(true).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Send A record query for example.com with length prefix
    let query = create_dns_query_bytes("example.com", 1);
    let length = (query.len() as u16).to_be_bytes();
    stream.write_all(&length).await.unwrap();
    stream.write_all(&query).await.unwrap();
    stream.flush().await.unwrap();

    // Read response length
    let mut length_buf = [0u8; 2];
    let result = timeout(Duration::from_secs(10), stream.read_exact(&mut length_buf)).await;
    assert!(result.is_ok(), "Should receive response length");

    let response_length = u16::from_be_bytes(length_buf) as usize;
    assert!(response_length > 0, "Response should not be empty");

    // Read response
    let mut response_buf = vec![0; response_length];
    stream.read_exact(&mut response_buf).await.unwrap();

    // Verify response can be parsed
    let response = DNSPacket::parse(&response_buf);
    assert!(response.is_ok(), "Response should be valid DNS packet");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // This test requires starting a server
async fn test_server_graceful_shutdown() {
    let (addr, shutdown_tx, handle) = start_test_server(false).await;

    // Verify server is running
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let query = create_dns_query_bytes("test.com", 1);
    socket.send_to(&query, addr).await.unwrap();

    // Send shutdown signal
    let _ = shutdown_tx.send(());

    // Server should shut down gracefully
    let result = timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "Server should shut down within timeout");
}

#[tokio::test]
#[ignore] // This test requires starting a server
async fn test_malformed_packet_handling() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Send malformed packet (too short)
    let malformed = vec![0x00, 0x01];
    socket.send_to(&malformed, addr).await.unwrap();

    // Server should continue running - test with valid query
    tokio::time::sleep(Duration::from_millis(100)).await;

    let query = create_dns_query_bytes("test.com", 1);
    socket.send_to(&query, addr).await.unwrap();

    let mut buf = vec![0; 4096];
    let result = timeout(Duration::from_secs(5), socket.recv_from(&mut buf)).await;
    assert!(
        result.is_ok(),
        "Server should still respond after malformed packet"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // This test requires starting a server
async fn test_concurrent_queries() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let mut handles = vec![];

    // Send 10 concurrent queries
    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let domain = format!("test{}.example.com", i);
            let query = create_dns_query_bytes(&domain, 1);

            socket.send_to(&query, addr).await.unwrap();

            let mut buf = vec![0; 4096];
            let result = timeout(Duration::from_secs(10), socket.recv_from(&mut buf)).await;

            result.is_ok()
        });
        handles.push(handle);
    }

    // All queries should succeed
    let mut successes = 0;
    for handle in handles {
        if handle.await.unwrap() {
            successes += 1;
        }
    }

    assert_eq!(successes, 10, "All concurrent queries should succeed");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_rate_limiting() {
    let mut config = DnsConfig::default();
    config.rate_limit_config.enable_rate_limiting = true;
    config.rate_limit_config.queries_per_second_per_ip = 2; // Very low limit for testing
    config.rate_limit_config.burst_size_per_ip = 3; // Allow small burst

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    drop(socket);

    config.bind_addr = addr;
    config.upstream_servers = vec!["8.8.8.8:53".parse().unwrap()];

    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );
    let query_semaphore = Arc::new(Semaphore::new(100));
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let _handle = tokio::spawn(async move {
        let _ = run_udp_server(
            config,
            resolver,
            query_semaphore,
            rate_limiter,
            metrics,
            shutdown_rx,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Send queries in rapid succession to trigger rate limiting
    let query = create_dns_query_bytes("example.com", 1);

    // First send a burst to fill the bucket
    for _ in 0..5 {
        client.send_to(&query, addr).await.unwrap();
    }

    // Small delay to let rate limiter process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Then send more queries that should be rate limited
    for _ in 0..15 {
        client.send_to(&query, addr).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Wait a bit more to allow responses to arrive
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Count responses received (with longer timeout since server might be processing)
    let mut responses = 0;
    let mut buf = vec![0; 4096];

    for _ in 0..20 {
        let result = timeout(Duration::from_millis(50), client.recv_from(&mut buf)).await;

        if result.is_ok() {
            responses += 1;
        }
    }

    // Should receive fewer responses due to rate limiting
    // With a limit of 5 queries per second and sending 20 rapidly,
    // we should get around 5-10 responses (allowing for timing variations)
    println!("Received {} responses out of 20 queries", responses);
    assert!(
        responses <= 15,
        "Rate limiting should drop some queries (got {})",
        responses
    );
    assert!(responses > 0, "Some queries should succeed");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_invalid_opcode_response() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Create query with STATUS opcode (2) which is not implemented
    let mut packet = Vec::new();
    packet.extend_from_slice(&[0x56, 0x78]); // ID
    packet.extend_from_slice(&[0x10, 0x00]); // Flags: QR=0, OPCODE=2, RD=1
    packet.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    packet.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0

    // Add question
    packet.extend_from_slice(&[0x07]); // Length of "example"
    packet.extend_from_slice(b"example");
    packet.extend_from_slice(&[0x03]); // Length of "com"
    packet.extend_from_slice(b"com");
    packet.push(0); // Root label
    packet.extend_from_slice(&[0x00, 0x01]); // QTYPE = A
    packet.extend_from_slice(&[0x00, 0x01]); // QCLASS = IN

    socket.send_to(&packet, addr).await.unwrap();

    let mut buf = vec![0; 4096];
    let (size, _) = socket.recv_from(&mut buf).await.unwrap();

    let response = DNSPacket::parse(&buf[..size]).unwrap();
    assert_eq!(
        response.header.rcode,
        ResponseCode::NotImplemented as u8,
        "Should return NOTIMPL for unsupported opcode"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_zone_transfer_refused() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Create AXFR query (type 252)
    let query = create_dns_query_bytes("example.com", 252);
    socket.send_to(&query, addr).await.unwrap();

    let mut buf = vec![0; 4096];
    let (size, _) = socket.recv_from(&mut buf).await.unwrap();

    let response = DNSPacket::parse(&buf[..size]).unwrap();
    assert_eq!(
        response.header.rcode,
        ResponseCode::Refused as u8,
        "Should return REFUSED for zone transfer"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_any_query_refused() {
    let (addr, shutdown_tx, _handle) = start_test_server(false).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Create ANY query (type 255)
    let query = create_dns_query_bytes("example.com", 255);
    socket.send_to(&query, addr).await.unwrap();

    let mut buf = vec![0; 4096];
    let (size, _) = socket.recv_from(&mut buf).await.unwrap();

    let response = DNSPacket::parse(&buf[..size]).unwrap();
    assert_eq!(
        response.header.rcode,
        ResponseCode::Refused as u8,
        "Should return REFUSED for ANY query"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_tcp_connection_handling() {
    let (addr, shutdown_tx, _handle) = start_test_server(true).await;

    // Test multiple sequential queries on same connection
    let mut stream = TcpStream::connect(addr).await.unwrap();

    for i in 0..3 {
        let domain = format!("test{}.com", i);
        let query = create_dns_query_bytes(&domain, 1);
        let length = (query.len() as u16).to_be_bytes();

        stream.write_all(&length).await.unwrap();
        stream.write_all(&query).await.unwrap();
        stream.flush().await.unwrap();

        let mut length_buf = [0u8; 2];
        stream.read_exact(&mut length_buf).await.unwrap();

        let response_length = u16::from_be_bytes(length_buf) as usize;
        let mut response_buf = vec![0; response_length];
        stream.read_exact(&mut response_buf).await.unwrap();

        let response = DNSPacket::parse(&response_buf);
        assert!(response.is_ok(), "Query {} should get valid response", i);
    }

    drop(stream);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
#[ignore] // Requires server startup and may make network calls
async fn test_max_concurrent_queries_limit() {
    let mut config = DnsConfig::default();

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    drop(socket);

    config.bind_addr = addr;
    config.upstream_servers = vec!["8.8.8.8:53".parse().unwrap()];
    config.max_concurrent_queries = 2; // Very low limit

    let metrics = Arc::new(DnsMetrics::new().unwrap());
    let resolver = Arc::new(
        DnsResolver::new(config.clone(), Some(metrics.clone()))
            .await
            .unwrap(),
    );
    let query_semaphore = Arc::new(Semaphore::new(config.max_concurrent_queries));
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()).unwrap());
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let _handle = tokio::spawn(async move {
        let _ = run_udp_server(
            config,
            resolver,
            query_semaphore,
            rate_limiter,
            metrics,
            shutdown_rx,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send 5 queries simultaneously
    let mut handles = vec![];
    for i in 0..5 {
        let handle = tokio::spawn(async move {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let query = create_dns_query_bytes(&format!("slow{}.com", i), 1);
            socket.send_to(&query, addr).await.unwrap();

            let mut buf = vec![0; 4096];
            let result = timeout(Duration::from_secs(10), socket.recv_from(&mut buf)).await;

            result.is_ok()
        });
        handles.push(handle);
    }

    let mut successful = 0;
    for handle in handles {
        if handle.await.unwrap() {
            successful += 1;
        }
    }

    // At least 2 should succeed (the concurrent limit)
    assert!(
        successful >= 2,
        "At least concurrent limit queries should succeed"
    );

    let _ = shutdown_tx.send(());
}
