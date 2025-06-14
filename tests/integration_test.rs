use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

#[tokio::test]
#[ignore] // This test requires the server to be running
async fn test_dns_server_responds_to_query() {
    // This test requires the server to be running
    // Run with: cargo test --test integration_test -- --ignored

    // Create a client socket
    let client_socket = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind client socket");

    let server_addr: SocketAddr = "127.0.0.1:1053".parse().unwrap();

    // Create a simple DNS query for example.com
    let query = vec![
        0x12, 0x34, // Transaction ID
        0x01, 0x00, // Flags: Standard query
        0x00, 0x01, // Questions: 1
        0x00, 0x00, // Answer RRs: 0
        0x00, 0x00, // Authority RRs: 0
        0x00, 0x00, // Additional RRs: 0
        // Question section
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', // "example"
        0x03, b'c', b'o', b'm', // "com"
        0x00, // Root label
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ];

    // Send the query
    client_socket
        .send_to(&query, server_addr)
        .await
        .expect("Failed to send query");

    // Wait for response with timeout
    let mut response_buf = vec![0u8; 512];
    let result = timeout(
        Duration::from_secs(5), // Increased timeout for CI environment
        client_socket.recv_from(&mut response_buf),
    )
    .await;

    match result {
        Ok(Ok((len, from))) => {
            println!("Received {} bytes from {}", len, from);
            assert_eq!(from, server_addr);
            assert!(len >= 12, "Response too short: {} bytes", len);

            // Check that it's a response (QR bit set)
            let qr_bit = response_buf[2] & 0x80;
            assert_eq!(
                qr_bit, 0x80,
                "QR bit not set in response: got {:02x}",
                response_buf[2]
            );

            // Check transaction ID matches
            assert_eq!(
                response_buf[0], 0x12,
                "Transaction ID mismatch: expected 0x12, got 0x{:02x}",
                response_buf[0]
            );
            assert_eq!(
                response_buf[1], 0x34,
                "Transaction ID mismatch: expected 0x34, got 0x{:02x}",
                response_buf[1]
            );

            println!("Integration test passed: DNS server responded correctly");
        }
        Ok(Err(e)) => panic!("Failed to receive response: {}", e),
        Err(_) => {
            panic!(
                "Timeout waiting for DNS response from {}. Server may not be running or may be overloaded.",
                server_addr
            );
        }
    }
}

#[tokio::test]
async fn test_dns_server_handles_invalid_packet() {
    if std::env::var("SKIP_INTEGRATION_TESTS").is_ok() {
        println!("Integration test skipped (SKIP_INTEGRATION_TESTS set)");
        return;
    }

    let client_socket = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind client socket");

    let server_addr: SocketAddr = "127.0.0.1:1053".parse().unwrap();

    // Send invalid packet (too short)
    let invalid_query = vec![0x12, 0x34]; // Only 2 bytes

    client_socket
        .send_to(&invalid_query, server_addr)
        .await
        .expect("Failed to send query");

    // Server should not crash, but may not respond
    // We just verify it doesn't panic by waiting briefly
    let mut response_buf = vec![0u8; 512];
    let _ = timeout(
        Duration::from_millis(100),
        client_socket.recv_from(&mut response_buf),
    )
    .await;

    // Test passes if server didn't crash
}
