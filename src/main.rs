mod constants;
mod dns;
mod prelude;

use anyhow::Result;
use std::collections::HashSet;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};

use constants::*;
pub use prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the resolver with Redis
    let resolver = DnsResolver::new(&get_redis_url(), FORWARD_DNS_SERVER)
        .await
        .expect("Failed to create resolver");

    // Initialize blocklist and public suffixes
    let mut blocklist = HashSet::new();
    let mut public_suffixes = HashSet::new();

    // Download and parse blocklist
    let response = reqwest::get(BLOCKLIST_URL).await?;
    let content = response.text().await?;
    for line in content.lines() {
        let line = line.trim().to_lowercase();
        if !line.is_empty() && !line.starts_with('#') {
            let parts: Vec<&str> = line.split(' ').collect();
            if parts.len() > 1 {
                blocklist.insert(parts[1].to_string());
            }
        }
    }

    // Download and parse public suffix list
    let response = reqwest::get(PUBLIC_SUFFIX_LIST_URL).await?;
    let content = response.text().await?;
    for line in content.lines() {
        let line = line.trim().to_lowercase();
        if !line.is_empty()
            && !line.starts_with("//")
            && !line.starts_with('!')
            && !line.starts_with('*')
        {
            public_suffixes.insert(line);
        }
    }

    // Set the lists in the resolver
    resolver.set_blocklist(blocklist).await;
    resolver.set_public_suffixes(public_suffixes).await;

    let resolver = std::sync::Arc::new(resolver);
    let udp_server = UdpSocket::bind(("0.0.0.0", PORT)).await?;
    let tcp_server = TcpListener::bind(("0.0.0.0", PORT)).await?;

    println!("Server is running on port {}", PORT);

    let resolver_udp = resolver.clone();
    tokio::spawn(async move {
        loop {
            let mut buffer = [0; MAX_UDP_PACKET_SIZE];
            let (amt, src) = match udp_server.recv_from(&mut buffer).await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Failed to receive UDP packet: {}", e);
                    continue;
                }
            };

            let query_data = &buffer[..amt];
            match resolver_udp.handle_query(query_data).await {
                Ok(response) => {
                    if let Err(e) = udp_server.send_to(&response, src).await {
                        eprintln!("Failed to send UDP response: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to handle query: {}", e),
            }
        }
    });

    let resolver_tcp = resolver.clone();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match tcp_server.accept().await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Failed to accept TCP connection: {}", e);
                    continue;
                }
            };

            // Read length prefix (2 bytes)
            let mut length_buf = [0u8; 2];
            let length = match stream.read_exact(&mut length_buf).await {
                Ok(_) => u16::from_be_bytes(length_buf) as usize,
                Err(e) => {
                    eprintln!("Failed to read message length: {}", e);
                    continue;
                }
            };

            // Read DNS message
            let mut buf = vec![0u8; length];
            if let Err(e) = stream.read_exact(&mut buf).await {
                eprintln!("Failed to read DNS message: {}", e);
                continue;
            }

            match resolver_tcp.handle_query(&buf).await {
                Ok(response) => {
                    // Write length prefix
                    let length_bytes = (response.len() as u16).to_be_bytes();
                    if let Err(e) = stream.write_all(&length_bytes).await {
                        eprintln!("Failed to write response length: {}", e);
                        continue;
                    }

                    // Write response
                    if let Err(e) = stream.write_all(&response).await {
                        eprintln!("Failed to send TCP response: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to handle query: {}", e),
            }
        }
    });

    std::thread::park();
    Ok(())
}

fn get_redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or("redis://127.0.0.1/".to_string())
}
