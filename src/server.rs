use crate::{
    config::DnsConfig, dns::DNSPacket, metrics::DnsMetrics, rate_limiter::DnsRateLimiter,
    resolver::DnsResolver,
};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{Semaphore, broadcast};
use tracing::{debug, error, info, trace, warn};

/// Run UDP server with graceful shutdown support
pub async fn run_udp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
    metrics: Arc<DnsMetrics>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind UDP socket
    let sock: Arc<UdpSocket> = Arc::new(UdpSocket::bind(config.bind_addr).await?);
    info!("UDP DNS server listening on {}", config.bind_addr);

    // Pre-allocate buffer outside loop for efficiency
    let mut buf = vec![0; 4096];

    loop {
        tokio::select! {
            // Handle shutdown signal
            _ = shutdown_rx.recv() => {
                info!("UDP server received shutdown signal");
                info!("UDP server shutdown complete");
                break;
            }

            // Handle incoming UDP packets
            result = sock.recv_from(&mut buf) => {
                let (read_bytes, src_addr) = result?;

                // Check rate limiting first (before semaphore to save resources)
                if !rate_limiter.check_query_allowed(src_addr.ip()) {
                    warn!("Rate limit exceeded for {}, dropping query", src_addr.ip());
                    continue;
                }

                // Acquire semaphore permit before processing query
                let permit = match query_semaphore.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        warn!(
                            "Max concurrent queries reached, dropping query from {}",
                            src_addr
                        );
                        continue;
                    }
                };

                let resolver_clone = resolver.clone();
                let metrics_clone = metrics.clone();
                let query_data = buf[..read_bytes].to_vec();
                let sock_clone = sock.clone();

                // Handle query in a separate task
                tokio::spawn(async move {
                    let _permit = permit; // Keep permit alive for the duration of the query

                    match handle_dns_query(&query_data, &resolver_clone, &metrics_clone, "udp").await {
                        Ok(response_data) => {
                            let final_response = if let Ok(query_packet) = DNSPacket::parse(&query_data) {
                                let max_udp_size = query_packet.max_udp_payload_size();

                                // Check if response is too large for UDP
                                if response_data.len() > max_udp_size as usize {
                                    debug!(
                                        "Response too large for UDP ({}>{} bytes), sending truncated response",
                                        response_data.len(),
                                        max_udp_size
                                    );

                                    // Record truncation in metrics
                                    let reason = if max_udp_size == 512 {
                                        "no_edns"
                                    } else {
                                        "exceeds_edns_limit"
                                    };
                                    metrics_clone.record_truncated_response("udp", reason);

                                    // Create truncated response with TC flag set
                                    let truncated_response = resolver_clone.create_truncated_response(&query_packet);
                                    match truncated_response.serialize() {
                                        Ok(truncated_data) => truncated_data,
                                        Err(e) => {
                                            error!("Failed to serialize truncated response: {:?}", e);
                                            response_data // Fall back to original response
                                        }
                                    }
                                } else {
                                    response_data
                                }
                            } else {
                                // If we can't parse the query, just send the original response
                                response_data
                            };

                            if let Err(e) = sock_clone.send_to(&final_response, src_addr).await {
                                error!("Failed to send UDP response to {}: {:?}", src_addr, e);
                            }
                        }
                        Err(e) => {
                            // Log at debug level for parsing errors, warn for other errors
                            if e.to_string().contains("Invalid DNS packet") {
                                debug!("Malformed UDP packet from {}: {}", src_addr, e);
                            } else {
                                warn!("Failed to handle UDP query from {}: {:?}", src_addr, e);
                            }
                        }
                    }
                });
            }
        }
    }

    Ok(())
}

/// Run TCP server with graceful shutdown support
pub async fn run_tcp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
    metrics: Arc<DnsMetrics>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind TCP listener
    let listener = TcpListener::bind(config.bind_addr).await?;
    info!("TCP DNS server listening on {}", config.bind_addr);

    loop {
        tokio::select! {
            // Handle shutdown signal
            _ = shutdown_rx.recv() => {
                info!("TCP server received shutdown signal");
                info!("TCP server shutdown complete");
                break;
            }

            // Handle incoming TCP connections
            result = listener.accept() => {
                let (stream, src_addr) = result?;
                let resolver = resolver.clone();
                let query_semaphore = query_semaphore.clone();
                let rate_limiter = rate_limiter.clone();
                let metrics = metrics.clone();

                // Handle each TCP connection in a separate task
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_tcp_connection(stream, src_addr, resolver, query_semaphore, rate_limiter, metrics)
                            .await
                    {
                        warn!("TCP connection error from {}: {:?}", src_addr, e);
                    }
                });
            }
        }
    }

    Ok(())
}

async fn handle_dns_query(
    buf: &[u8],
    resolver: &DnsResolver,
    metrics: &DnsMetrics,
    protocol: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Parse the DNS packet
    let packet = match DNSPacket::parse(buf) {
        Ok(packet) => packet,
        Err(e) => {
            debug!(
                "Failed to parse DNS packet: {:?} (packet length: {} bytes)",
                e,
                buf.len()
            );

            // Record the malformed packet in metrics
            let error_type = if e.to_string().contains("InvalidLabel") {
                "invalid_label"
            } else if e.to_string().contains("BufferTooSmall") {
                "buffer_too_small"
            } else if e.to_string().contains("InvalidPacket") {
                "invalid_packet"
            } else {
                "parse_error"
            };
            metrics.record_malformed_packet(protocol, error_type);

            // For malformed packets, we can't create a proper response since we don't have a valid packet ID
            // Return a generic format error response
            return Err(format!("Invalid DNS packet: {}", e).into());
        }
    };

    debug!(
        "Received DNS query: id={}, questions={}, edns={}",
        packet.header.id,
        packet.header.qdcount,
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
            debug!("Query: {} {:?}", domain, question.qtype);
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

async fn handle_tcp_connection(
    mut stream: TcpStream,
    src_addr: std::net::SocketAddr,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
    metrics: Arc<DnsMetrics>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut length_buf = [0u8; 2];

    loop {
        // Read the 2-byte length prefix
        match stream.read_exact(&mut length_buf).await {
            Ok(_) => {}
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

        // Check rate limiting
        if !rate_limiter.check_query_allowed(src_addr.ip()) {
            warn!(
                "Rate limit exceeded for {}, closing TCP connection",
                src_addr.ip()
            );
            break;
        }

        // Acquire semaphore permit for concurrent query limiting
        let _permit = match query_semaphore.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                warn!(
                    "Max concurrent queries reached, closing TCP connection from {}",
                    src_addr
                );
                break;
            }
        };

        // Parse and handle the DNS query
        match handle_dns_query(&message_buf, &resolver, &metrics, "tcp").await {
            Ok(response_data) => {
                // Write length prefix followed by response
                let response_length = response_data.len() as u16;
                stream.write_all(&response_length.to_be_bytes()).await?;
                stream.write_all(&response_data).await?;
                stream.flush().await?;
            }
            Err(e) => {
                // Log at debug level for parsing errors, warn for other errors
                if e.to_string().contains("Invalid DNS packet") {
                    debug!("Malformed TCP packet from {}: {}", src_addr, e);
                } else {
                    warn!("Failed to handle TCP query from {}: {:?}", src_addr, e);
                }
                // For TCP, we should close the connection on errors
                break;
            }
        }
    }

    Ok(())
}
