use dns::DNSPacket;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod dns;
pub mod error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "heimdall=debug,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let port = 1053;
    let bind_addr = format!("127.0.0.1:{}", port);
    let sock = UdpSocket::bind(&bind_addr).await?;
    info!("DNS server listening on {}", bind_addr);

    // Pre-allocate buffer outside loop for efficiency
    let mut buf = vec![0; 4096];
    
    loop {
        let (read_bytes, src_addr) = sock.recv_from(&mut buf).await?;
        
        // Parse the DNS packet
        match DNSPacket::parse(&buf[..read_bytes]) {
            Ok(packet) => {
                debug!("Received DNS query from {}: id={}, questions={}", 
                    src_addr, packet.header.id, packet.header.qdcount);
                trace!("Full packet header: {:?}", packet.header);
                
                // For now, generate a basic response
                let response = packet.generate_response();
                match response.serialize() {
                    Ok(serialized) => {
                        sock.send_to(&serialized, src_addr).await?;
                    }
                    Err(e) => error!("Failed to serialize response: {:?}", e),
                }
            }
            Err(e) => {
                warn!("Failed to parse packet from {}: {:?}", src_addr, e);
                trace!("Raw packet data: {:?}", &buf[..read_bytes]);
            }
        }
    }
}
