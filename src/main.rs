mod constants;
mod dns;
mod prelude;

use std::net::SocketAddr;

use constants::*;
pub use prelude::*;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufStream},
    net::{TcpListener, TcpStream, UdpSocket},
};
use trust_dns_client::proto::iocompat::AsyncIoTokioAsStd;
use trust_dns_client::{client::AsyncClient, tcp::TcpClientStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the resolver with Redis and Google DNS
    let resolver = DnsResolver::new("redis://127.0.0.1/").await.expect("Failed to create resolver");
    resolver.check_updates().await.expect("Failed to check updates");
    let resolver = std::sync::Arc::new(resolver);

    let udp_server = UdpSocket::bind(("0.0.0.0", PORT)).await?;
    let tcp_server = TcpListener::bind(("0.0.0.0", PORT)).await?;

    println!("Server is running on port {}", PORT);

    let resolver_udp = resolver.clone();
    tokio::spawn(async move {
        loop {
            let mut forward_resolver = create_forward_resolver(FORWARD_DNS_SERVER).await.expect("Failed to create forward resolver");
            let mut buffer = [0; MAX_UDP_PACKET_SIZE];
            let (amt, src) = match udp_server.recv_from(&mut buffer).await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Failed to receive UDP packet: {}", e);
                    continue;
                }
            };

            let adjusted_buffer = match buffer.get(0..amt) {
                Some(buf) => buf,
                None => {
                    eprintln!("Invalid buffer slice");
                    continue;
                }
            };

            let packet =
                match DnsPacket::from_wire(&mut BitReader::new(Cursor::new(adjusted_buffer))) {
                    Ok(packet) => packet,
                    Err(e) => {
                        eprintln!("Failed to parse DNS packet: {}", e);
                        continue;
                    }
                };

            // Use our resolver to handle the query
            let response = match resolver_udp.lookup(&packet, &mut forward_resolver).await {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("Failed to lookup DNS packet: {}", e);
                    continue;
                }
            };
            let response_wire = response.to_wire();

            if let Err(e) = udp_server.send_to(&response_wire, &src).await {
                eprintln!("Failed to send UDP response: {}", e);
            }
        }
    });

    let resolver_tcp = resolver.clone();
    tokio::spawn(async move {
        loop {
            let mut forward_resolver = create_forward_resolver("8.8.8.8").await.expect("Failed to create forward resolver");
            let (mut stream, _) = tcp_server
                .accept()
                .await
                .expect("Failed to accept TCP connection");

            // Read length prefix (2 bytes)
            let mut length_buf = [0u8; 2];
            stream
                .read_exact(&mut length_buf)
                .await
                .expect("Failed to read message length");
            let length = u16::from_be_bytes(length_buf) as usize;

            // Read DNS message
            let mut buf = vec![0u8; length];
            stream
                .read_exact(&mut buf)
                .await
                .expect("Failed to read DNS message");

            let packet = DnsPacket::from_wire(&mut BitReader::new(Cursor::new(&buf)))
                .expect("Failed to parse DNS packet");
            let response = resolver_tcp
                .lookup(&packet, &mut forward_resolver)
                .await
                .expect("Failed to lookup DNS packet");
            let response_wire = response.to_wire();

            // Write length prefix
            let length_bytes = (response_wire.len() as u16).to_be_bytes();
            stream
                .write_all(&length_bytes)
                .await
                .expect("Failed to write response length");

            // Write response
            stream
                .write_all(&response_wire)
                .await
                .expect("Failed to send TCP response");
        }
    });

    std::thread::park();
    Ok(())
}

async fn create_forward_resolver(
    dns_server: &str,
) -> Result<AsyncClient, Box<dyn std::error::Error>> {
    let socket_addr: SocketAddr = format!("{}:53", dns_server).parse()?;
    let (stream, sender) =
        TcpClientStream::<AsyncIoTokioAsStd<TokioTcpStream>>::new(socket_addr.into());
    let client = AsyncClient::new(stream, sender, None);
    let (forward_resolver, bg) = client.await.expect("Failed to create async client");
    tokio::spawn(bg);
    Ok(forward_resolver)
}
