use dns::DNSPacket;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod dns;
pub mod error;
pub mod config;
pub mod resolver;

use config::DnsConfig;
use resolver::DnsResolver;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "heimdall=info,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = DnsConfig::from_env();
    info!("Heimdall DNS Server starting up");
    info!("Configuration: bind_addr={}, upstream_servers={:?}", 
        config.bind_addr, config.upstream_servers);
    
    // Create resolver
    let resolver = DnsResolver::new(config.clone()).await?;
    
    // Bind server socket
    let sock = UdpSocket::bind(config.bind_addr).await?;
    info!("DNS server listening on {}", config.bind_addr);

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
                
                // Log the domain being queried
                for question in &packet.questions {
                    let domain = question.labels
                        .iter()
                        .filter(|l| !l.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(".");
                    if !domain.is_empty() {
                        info!("Query from {}: {} {:?}", src_addr, domain, question.qtype);
                    }
                }
                
                // Resolve the query using upstream servers
                let response = match resolver.resolve(packet.clone(), packet.header.id).await {
                    Ok(response) => {
                        debug!("Successfully resolved query id={}, answers={}", 
                            response.header.id, response.header.ancount);
                        response
                    }
                    Err(e) => {
                        warn!("Failed to resolve query: {:?}", e);
                        resolver.create_servfail_response(&packet)
                    }
                };
                
                // Send response back to client
                match response.serialize() {
                    Ok(serialized) => {
                        if let Err(e) = sock.send_to(&serialized, src_addr).await {
                            error!("Failed to send response to {}: {:?}", src_addr, e);
                        }
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
