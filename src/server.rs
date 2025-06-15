use crate::{
    config::DnsConfig,
    dns::{DNSPacket, DNSPacketRef, enums::DnsOpcode, zero_copy::DNSPacketView},
    metrics::DnsMetrics,
    pool::BufferPool,
    rate_limiter::DnsRateLimiter,
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

    // Create buffer pool for UDP packets with thread-local optimization
    let buffer_pool = Arc::new(BufferPool::with_thread_local(4096, 128)); // 4KB buffers, max 128 in pool

    loop {
        // Get a buffer from the pool (will use thread-local pool if available)
        let mut buf = buffer_pool.get();
        buf.resize(4096, 0);

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
                let buffer_pool_clone = buffer_pool.clone();

                // Handle query in a separate task
                tokio::spawn(async move {
                    let _permit = permit; // Keep permit alive for the duration of the query

                    match handle_dns_query_with_pool(&query_data, &resolver_clone, &metrics_clone, "udp", &buffer_pool_clone).await {
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

    // Create buffer pool for TCP packets (max DNS message size is 64KB) with thread-local optimization
    let buffer_pool = Arc::new(BufferPool::with_thread_local(65536, 32)); // 64KB buffers, max 32 in pool

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
                let buffer_pool = buffer_pool.clone();

                // Handle each TCP connection in a separate task
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_tcp_connection(stream, src_addr, resolver, query_semaphore, rate_limiter, metrics, buffer_pool)
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

async fn handle_dns_query_with_pool(
    buf: &[u8],
    resolver: &DnsResolver,
    metrics: &DnsMetrics,
    protocol: &str,
    buffer_pool: &BufferPool,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Get buffer from pool
    let mut pooled_buffer = buffer_pool.get();

    // Process query and get response directly serialized into pooled buffer
    handle_dns_query_optimized(buf, resolver, metrics, protocol, &mut pooled_buffer).await?;

    // Return the buffer content as a Vec
    Ok(pooled_buffer.to_vec())
}

async fn handle_dns_query_optimized(
    buf: &[u8],
    resolver: &DnsResolver,
    metrics: &DnsMetrics,
    protocol: &str,
    response_buf: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Try zero-copy parsing first for maximum performance
    let packet_view = match DNSPacketView::new(buf) {
        Ok(view) => view,
        Err(_) => {
            // Fallback to DNSPacketRef for compatibility
            let packet_ref = match DNSPacketRef::parse_metadata(buf) {
                Ok(pref) => pref,
                Err(e) => {
                    debug!(
                        "Failed to parse DNS packet metadata: {:?} (packet length: {} bytes)",
                        e,
                        buf.len()
                    );
                    metrics.record_malformed_packet(protocol, "parse_error");
                    return Err(format!("Invalid DNS packet: {}", e).into());
                }
            };

            // Quick validation checks using zero-copy
            if !packet_ref.is_query() {
                debug!("Received DNS response instead of query");
                metrics.record_malformed_packet(protocol, "not_query");
                return Err("Expected DNS query, got response".into());
            }

            // Parse the full packet
            let packet = match DNSPacket::parse_query_fast(buf, &packet_ref) {
                Ok(packet) => packet,
                Err(_) => {
                    // Fallback to full parsing
                    match DNSPacket::parse(buf) {
                        Ok(p) => p,
                        Err(e) => {
                            debug!("Failed to parse DNS packet: {:?}", e);
                            metrics.record_malformed_packet(protocol, "parse_error");
                            return Err(format!("Invalid DNS packet: {}", e).into());
                        }
                    }
                }
            };

            return handle_parsed_query(packet, resolver, metrics, protocol, response_buf).await;
        }
    };

    // Quick validation with zero-copy view
    if !packet_view.is_query() {
        debug!("Received DNS response instead of query");
        metrics.record_malformed_packet(protocol, "not_query");
        return Err("Expected DNS query, got response".into());
    }

    // Check cache with zero-copy domain extraction
    if packet_view.header.qdcount > 0 {
        match packet_view.first_question_domain() {
            Ok(domain) => {
                // Try cache lookup with zero-copy domain
                if let Some(cached_response) = resolver.check_cache_fast(&domain) {
                    // Record cache hit (method might be named differently)
                    // TODO: Add proper cache hit metric recording
                    response_buf.clear();
                    response_buf.extend_from_slice(&cached_response);
                    return Ok(());
                }
            }
            Err(_) => {
                // Continue with full parsing
            }
        }
    }

    // If not in cache or complex query, parse the full packet
    let packet = match DNSPacket::parse(buf) {
        Ok(p) => p,
        Err(e) => {
            debug!("Failed to parse DNS packet: {:?}", e);
            metrics.record_malformed_packet(protocol, "parse_error");
            return Err(format!("Invalid DNS packet: {}", e).into());
        }
    };

    handle_parsed_query(packet, resolver, metrics, protocol, response_buf).await
}

async fn handle_parsed_query(
    packet: DNSPacket,
    resolver: &DnsResolver,
    metrics: &DnsMetrics,
    protocol: &str,
    response_buf: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Validate opcode
    match DnsOpcode::from_u8(packet.header.opcode) {
        Some(opcode) => {
            if !opcode.is_implemented() {
                debug!(
                    "Unsupported opcode {:?} ({}) in query id={}, returning NOTIMPL",
                    opcode, packet.header.opcode, packet.header.id
                );
                metrics.record_error_response("notimpl", protocol);
                let response = resolver.create_notimpl_response(&packet);
                response.serialize_into(response_buf)?;
                return Ok(());
            }

            // Handle UPDATE messages separately
            if opcode == DnsOpcode::Update {
                debug!("Received UPDATE message id={}", packet.header.id);

                // Check if zone store is available
                if let Some(_zone_store) = &resolver.zone_store {
                    // Get update processor (we'll need to add this to resolver)
                    if let Some(update_processor) = &resolver.update_processor {
                        match update_processor.process_update(&packet).await {
                            Ok(response) => {
                                debug!("UPDATE processed successfully id={}", packet.header.id);
                                response.serialize_into(response_buf)?;
                                return Ok(());
                            }
                            Err(e) => {
                                warn!("UPDATE failed: {}", e);
                                use crate::dynamic_update::UpdateError;
                                let response = match e {
                                    UpdateError::NotAuth(_) => {
                                        metrics.record_error_response("notauth", protocol);
                                        resolver.create_notauth_response(&packet)
                                    }
                                    UpdateError::Refused(_) => {
                                        metrics.record_error_response("refused", protocol);
                                        resolver.create_refused_response(&packet)
                                    }
                                    UpdateError::PrereqFailed(_) => {
                                        metrics.record_error_response("nxrrset", protocol);
                                        resolver.create_nxrrset_response(&packet)
                                    }
                                    _ => {
                                        metrics.record_error_response("servfail", protocol);
                                        resolver.create_servfail_response(&packet)
                                    }
                                };
                                response.serialize_into(response_buf)?;
                                return Ok(());
                            }
                        }
                    } else {
                        debug!("UPDATE not supported - no update processor configured");
                        metrics.record_error_response("notimpl", protocol);
                        let response = resolver.create_notimpl_response(&packet);
                        response.serialize_into(response_buf)?;
                        return Ok(());
                    }
                } else {
                    debug!("UPDATE not supported - no zone store configured");
                    metrics.record_error_response("notimpl", protocol);
                    let response = resolver.create_notimpl_response(&packet);
                    response.serialize_into(response_buf)?;
                    return Ok(());
                }
            }
        }
        None => {
            debug!(
                "Invalid opcode {} in query id={}, returning FORMERR",
                packet.header.opcode, packet.header.id
            );
            metrics.record_error_response("formerr", protocol);
            let response = resolver.create_formerr_response(&packet);
            response.serialize_into(response_buf)?;
            return Ok(());
        }
    }

    // Validate the packet has at least one question
    if packet.header.qdcount == 0 {
        debug!(
            "Query id={} has no questions, returning FORMERR",
            packet.header.id
        );
        metrics.record_error_response("formerr", protocol);
        let response = resolver.create_formerr_response(&packet);
        response.serialize_into(response_buf)?;
        return Ok(());
    }

    // Check for policy violations that should return REFUSED
    if should_refuse_query(&packet) {
        debug!(
            "Query id={} violates policy, returning REFUSED",
            packet.header.id
        );
        metrics.record_error_response("refused", protocol);
        let response = resolver.create_refused_response(&packet);
        response.serialize_into(response_buf)?;
        return Ok(());
    }

    // Log the domain being queried
    if packet.header.qdcount > 0 && !packet.questions.is_empty() {
        let question = &packet.questions[0];
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

    // Serialize response directly into provided buffer
    response.serialize_into(response_buf)?;
    Ok(())
}

// Legacy implementation kept for reference
#[allow(dead_code)]
async fn handle_dns_query(
    buf: &[u8],
    resolver: &DnsResolver,
    metrics: &DnsMetrics,
    protocol: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // First try zero-copy parsing for fast rejection of malformed packets
    let packet_ref = match DNSPacketRef::parse_metadata(buf) {
        Ok(pref) => pref,
        Err(e) => {
            debug!(
                "Failed to parse DNS packet metadata: {:?} (packet length: {} bytes)",
                e,
                buf.len()
            );
            metrics.record_malformed_packet(protocol, "parse_error");
            return Err(format!("Invalid DNS packet: {}", e).into());
        }
    };

    // Quick validation checks using zero-copy
    if !packet_ref.is_query() {
        // This is already a response, reject it
        debug!("Received DNS response instead of query");
        metrics.record_malformed_packet(protocol, "not_query");
        return Err("Expected DNS query, got response".into());
    }

    // Now parse the full packet only if initial checks pass
    let packet = match packet_ref.to_owned() {
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
            } else if e.to_string().contains("Parse error") {
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
        "Received DNS query: id={}, opcode={}, questions={}, edns={}",
        packet.header.id,
        packet.header.opcode,
        packet.header.qdcount,
        if packet.supports_edns() { "yes" } else { "no" }
    );
    trace!("Full packet header: {:?}", packet.header);
    if packet.supports_edns() {
        debug!("EDNS info: {}", packet.edns_debug_info());
    }

    // Validate opcode
    match DnsOpcode::from_u8(packet.header.opcode) {
        Some(opcode) => {
            if !opcode.is_implemented() {
                debug!(
                    "Unsupported opcode {:?} ({}) in query id={}, returning NOTIMPL",
                    opcode, packet.header.opcode, packet.header.id
                );
                metrics.record_error_response("notimpl", protocol);
                let response = resolver.create_notimpl_response(&packet);
                let serialized = response.serialize()?;
                return Ok(serialized);
            }
        }
        None => {
            // Invalid opcode value
            debug!(
                "Invalid opcode {} in query id={}, returning FORMERR",
                packet.header.opcode, packet.header.id
            );
            metrics.record_error_response("formerr", protocol);
            let response = resolver.create_formerr_response(&packet);
            let serialized = response.serialize()?;
            return Ok(serialized);
        }
    }

    // Validate the packet has at least one question
    if packet.header.qdcount == 0 {
        debug!(
            "Query id={} has no questions, returning FORMERR",
            packet.header.id
        );
        metrics.record_error_response("formerr", protocol);
        let response = resolver.create_formerr_response(&packet);
        let serialized = response.serialize()?;
        return Ok(serialized);
    }

    // Check for policy violations that should return REFUSED
    if should_refuse_query(&packet) {
        debug!(
            "Query id={} violates policy, returning REFUSED",
            packet.header.id
        );
        metrics.record_error_response("refused", protocol);
        let response = resolver.create_refused_response(&packet);
        let serialized = response.serialize()?;
        return Ok(serialized);
    }

    // Log the domain being queried using zero-copy when possible
    if packet.header.qdcount > 0 {
        // Try to get first question without parsing all
        match packet_ref.first_question() {
            Ok(question) => {
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
            Err(_) => {
                // Fallback to full parsing if zero-copy fails
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
            }
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
    buffer_pool: Arc<BufferPool>,
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

        // Get a buffer from the pool and resize to exact message length
        let mut message_buf = buffer_pool.get();
        message_buf.resize(message_length, 0);
        stream
            .read_exact(&mut message_buf[..message_length])
            .await?;

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
        match handle_dns_query_with_pool(
            &message_buf[..message_length],
            &resolver,
            &metrics,
            "tcp",
            &buffer_pool,
        )
        .await
        {
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

/// Check if a query should be refused based on policy
fn should_refuse_query(packet: &DNSPacket) -> bool {
    use crate::dns::enums::DNSResourceType;

    // Check if query is asking for zone transfers
    for question in &packet.questions {
        match question.qtype {
            DNSResourceType::AXFR | DNSResourceType::IXFR => {
                // Zone transfers are refused
                return true;
            }
            DNSResourceType::ANY => {
                // ANY queries can be refused for security (amplification attack prevention)
                // This is configurable in production systems
                return true;
            }
            _ => {}
        }
    }

    // Additional policy checks can be added here:
    // - Refusing queries from certain IP ranges
    // - Refusing queries for certain domains
    // - Refusing based on query patterns
    // - Refusing based on authentication status

    false
}
