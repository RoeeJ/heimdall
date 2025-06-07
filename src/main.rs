use dns::DNSPacket;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::sync::Arc;

pub mod cache;
pub mod config;
pub mod dns;
pub mod error;
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
    info!(
        "Configuration: bind_addr={}, upstream_servers={:?}",
        config.bind_addr, config.upstream_servers
    );

    // Create resolver (shared between UDP and TCP)
    let resolver = Arc::new(DnsResolver::new(config.clone()).await?);

    // Start UDP and TCP servers concurrently
    let udp_task = tokio::spawn(run_udp_server(config.clone(), resolver.clone()));
    let tcp_task = tokio::spawn(run_tcp_server(config.clone(), resolver.clone()));
    
    info!("DNS server listening on {} (UDP and TCP)", config.bind_addr);

    // Wait for either server to exit (which shouldn't happen)
    tokio::select! {
        result = udp_task => {
            error!("UDP server exited: {:?}", result);
        }
        result = tcp_task => {
            error!("TCP server exited: {:?}", result);
        }
    }

    Ok(())
}

async fn run_udp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind UDP socket
    let sock = UdpSocket::bind(config.bind_addr).await?;
    info!("UDP DNS server listening on {}", config.bind_addr);

    // Pre-allocate buffer outside loop for efficiency
    let mut buf = vec![0; 4096];

    loop {
        let (read_bytes, src_addr) = sock.recv_from(&mut buf).await?;

        // Parse and handle the DNS packet
        match handle_dns_query(&buf[..read_bytes], &resolver).await {
            Ok(response_data) => {
                // Check if response is too large for UDP and client supports EDNS
                if response_data.len() > 512 {
                    // Try to parse the query to check EDNS support
                    if let Ok(query_packet) = dns::DNSPacket::parse(&buf[..read_bytes]) {
                        let max_udp_size = query_packet.max_udp_payload_size();
                        if response_data.len() > max_udp_size as usize {
                            warn!(
                                "Response too large for UDP ({}>{} bytes), client should retry with TCP",
                                response_data.len(), max_udp_size
                            );
                            // TODO: Set TC (truncated) flag in response
                        }
                    }
                }
                
                if let Err(e) = sock.send_to(&response_data, src_addr).await {
                    error!("Failed to send UDP response to {}: {:?}", src_addr, e);
                }
            }
            Err(e) => {
                warn!("Failed to handle UDP query from {}: {:?}", src_addr, e);
            }
        }
    }
}

async fn run_tcp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind TCP listener
    let listener = TcpListener::bind(config.bind_addr).await?;
    info!("TCP DNS server listening on {}", config.bind_addr);

    loop {
        let (stream, src_addr) = listener.accept().await?;
        let resolver = resolver.clone();
        
        // Handle each TCP connection in a separate task
        tokio::spawn(async move {
            if let Err(e) = handle_tcp_connection(stream, src_addr, resolver).await {
                warn!("TCP connection error from {}: {:?}", src_addr, e);
            }
        });
    }
}

async fn handle_tcp_connection(
    mut stream: TcpStream,
    src_addr: std::net::SocketAddr,
    resolver: Arc<DnsResolver>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut length_buf = [0u8; 2];
    
    loop {
        // Read the 2-byte length prefix
        match stream.read_exact(&mut length_buf).await {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Client closed connection
                debug!("TCP connection closed by client {}", src_addr);
                break;
            }
            Err(e) => return Err(e.into()),
        }
        
        let message_length = u16::from_be_bytes(length_buf) as usize;
        
        // Read the DNS message
        let mut message_buf = vec![0; message_length];
        stream.read_exact(&mut message_buf).await?;
        
        // Parse and handle the DNS query
        match handle_dns_query(&message_buf, &resolver).await {
            Ok(response_data) => {
                // Write length prefix followed by response
                let response_length = response_data.len() as u16;
                stream.write_all(&response_length.to_be_bytes()).await?;
                stream.write_all(&response_data).await?;
                stream.flush().await?;
            }
            Err(e) => {
                warn!("Failed to handle TCP query from {}: {:?}", src_addr, e);
                // For TCP, we should close the connection on errors
                break;
            }
        }
    }
    
    Ok(())
}

async fn handle_dns_query(
    buf: &[u8],
    resolver: &DnsResolver,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Parse the DNS packet
    let packet = DNSPacket::parse(buf)?;
    
    debug!(
        "Received DNS query: id={}, questions={}, edns={}",
        packet.header.id, packet.header.qdcount,
        if packet.supports_edns() { "yes" } else { "no" }
    );
    trace!("Full packet header: {:?}", packet.header);
    if packet.supports_edns() {
        debug!("EDNS info: {}", packet.edns_debug_info());
    }

    // Log the domain being queried
    for question in &packet.questions {
        let domain = question
            .labels
            .iter()
            .filter(|l| !l.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(".");
        if !domain.is_empty() {
            info!("Query: {} {:?}", domain, question.qtype);
        }
    }

    // Resolve the query using upstream servers
    let response = match resolver.resolve(packet.clone(), packet.header.id).await {
        Ok(response) => {
            debug!(
                "Successfully resolved query id={}, answers={}",
                response.header.id, response.header.ancount
            );
            response
        }
        Err(e) => {
            warn!("Failed to resolve query: {:?}", e);
            resolver.create_servfail_response(&packet)
        }
    };

    // Serialize response
    let serialized = response.serialize()?;
    Ok(serialized)
}
