use dns::DNSPacket;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod cache;
pub mod config;
pub mod dns;
pub mod error;
pub mod rate_limiter;
pub mod resolver;
pub mod validation;

use config::DnsConfig;
use rate_limiter::DnsRateLimiter;
use resolver::DnsResolver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration first to get runtime settings
    let config = DnsConfig::from_env();

    // Build custom Tokio runtime with configurable thread pool
    let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();

    // Configure worker threads if specified
    if config.worker_threads > 0 {
        runtime_builder.worker_threads(config.worker_threads);
    }

    // Configure blocking threads if specified
    if config.blocking_threads > 0 {
        runtime_builder.max_blocking_threads(config.blocking_threads);
    }

    // Enable all features and build runtime
    let runtime = runtime_builder
        .enable_all()
        .thread_name("heimdall-worker")
        .build()?;

    // Run the async main function on our custom runtime
    runtime.block_on(async_main(config))
}

async fn async_main(config: DnsConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "heimdall=info,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Heimdall DNS Server starting up");
    info!(
        "Configuration: bind_addr={}, upstream_servers={:?}",
        config.bind_addr, config.upstream_servers
    );
    info!(
        "Runtime configuration: worker_threads={}, blocking_threads={}, max_concurrent_queries={}",
        if config.worker_threads > 0 {
            config.worker_threads.to_string()
        } else {
            "default".to_string()
        },
        config.blocking_threads,
        config.max_concurrent_queries
    );

    // Create resolver (shared between UDP and TCP)
    let resolver = Arc::new(DnsResolver::new(config.clone()).await?);

    // Create semaphore for limiting concurrent queries
    let query_semaphore = Arc::new(Semaphore::new(config.max_concurrent_queries));

    // Create rate limiter
    let rate_limiter = Arc::new(DnsRateLimiter::new(config.rate_limit_config.clone()));

    info!(
        "Rate limiting enabled: {}, per-IP limit: {} QPS, global limit: {} QPS",
        config.rate_limit_config.enable_rate_limiting,
        config.rate_limit_config.queries_per_second_per_ip,
        config.rate_limit_config.global_queries_per_second
    );

    // Start rate limiter cleanup task
    let rate_limiter_cleanup = rate_limiter.clone();
    let cleanup_interval = config.rate_limit_config.cleanup_interval_seconds;
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval));
        loop {
            interval.tick().await;
            rate_limiter_cleanup.cleanup_expired_entries();

            let stats = rate_limiter_cleanup.get_stats();
            if stats.active_ip_limiters > 1000 {
                debug!(
                    "Rate limiter stats: {} active IP limiters, {} error limiters, {} nxdomain limiters",
                    stats.active_ip_limiters,
                    stats.active_error_limiters,
                    stats.active_nxdomain_limiters
                );
            }
        }
    });

    // Start cache persistence task if enabled
    let cache_save_task = if config.cache_save_interval > 0 && resolver.has_cache_persistence() {
        let resolver_cache = resolver.clone();
        let save_interval = config.cache_save_interval;
        info!(
            "Starting cache persistence task: saving every {} seconds",
            save_interval
        );

        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(save_interval));
            loop {
                interval.tick().await;
                if let Err(e) = resolver_cache.save_cache().await {
                    error!("Failed to save cache: {}", e);
                } else {
                    trace!("Cache saved successfully");
                }
            }
        }))
    } else {
        None
    };

    // Start UDP and TCP servers concurrently
    let udp_task = tokio::spawn(run_udp_server(
        config.clone(),
        resolver.clone(),
        query_semaphore.clone(),
        rate_limiter.clone(),
    ));
    let tcp_task = tokio::spawn(run_tcp_server(
        config.clone(),
        resolver.clone(),
        query_semaphore.clone(),
        rate_limiter.clone(),
    ));

    info!("DNS server listening on {} (UDP and TCP)", config.bind_addr);

    // Setup graceful shutdown signal handling
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for shutdown signal");
        info!("Shutdown signal received, saving cache before exit...");

        // Save cache before shutdown
        if let Err(e) = resolver.save_cache().await {
            error!("Failed to save cache during shutdown: {}", e);
        } else {
            info!("Cache saved successfully during shutdown");
        }
    };

    // Wait for any task to exit or shutdown signal
    tokio::select! {
        result = udp_task => {
            error!("UDP server exited: {:?}", result);
        }
        result = tcp_task => {
            error!("TCP server exited: {:?}", result);
        }
        result = cleanup_task => {
            error!("Rate limiter cleanup task exited: {:?}", result);
        }
        result = async {
            if let Some(task) = cache_save_task {
                task.await
            } else {
                // If no cache task, wait forever
                std::future::pending::<Result<(), tokio::task::JoinError>>().await
            }
        } => {
            error!("Cache save task exited: {:?}", result);
        }
        _ = shutdown_signal => {
            info!("Heimdall DNS server shutting down gracefully");
        }
    }

    Ok(())
}

async fn run_udp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind UDP socket
    let sock = Arc::new(UdpSocket::bind(config.bind_addr).await?);
    info!("UDP DNS server listening on {}", config.bind_addr);

    // Pre-allocate buffer outside loop for efficiency
    let mut buf = vec![0; 4096];

    loop {
        let (read_bytes, src_addr) = sock.recv_from(&mut buf).await?;

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
        let query_data = buf[..read_bytes].to_vec();
        let sock_clone = sock.clone();

        // Handle query in a separate task to avoid blocking the main UDP loop
        tokio::spawn(async move {
            let _permit = permit; // Keep permit alive for the duration of the query
            match handle_dns_query(&query_data, &resolver_clone).await {
                Ok(response_data) => {
                    // Check if response is too large for UDP and client supports EDNS
                    if response_data.len() > 512 {
                        // Try to parse the query to check EDNS support
                        if let Ok(query_packet) = dns::DNSPacket::parse(&query_data) {
                            let max_udp_size = query_packet.max_udp_payload_size();
                            if response_data.len() > max_udp_size as usize {
                                warn!(
                                    "Response too large for UDP ({}>{} bytes), client should retry with TCP",
                                    response_data.len(),
                                    max_udp_size
                                );
                                // TODO: Set TC (truncated) flag in response
                            }
                        }
                    }

                    if let Err(e) = sock_clone.send_to(&response_data, src_addr).await {
                        error!("Failed to send UDP response to {}: {:?}", src_addr, e);
                    }
                }
                Err(e) => {
                    warn!("Failed to handle UDP query from {}: {:?}", src_addr, e);
                }
            }
        });
    }
}

async fn run_tcp_server(
    config: DnsConfig,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Bind TCP listener
    let listener = TcpListener::bind(config.bind_addr).await?;
    info!("TCP DNS server listening on {}", config.bind_addr);

    loop {
        let (stream, src_addr) = listener.accept().await?;
        let resolver = resolver.clone();
        let query_semaphore = query_semaphore.clone();
        let rate_limiter = rate_limiter.clone();

        // Handle each TCP connection in a separate task
        tokio::spawn(async move {
            if let Err(e) =
                handle_tcp_connection(stream, src_addr, resolver, query_semaphore, rate_limiter)
                    .await
            {
                warn!("TCP connection error from {}: {:?}", src_addr, e);
            }
        });
    }
}

async fn handle_tcp_connection(
    mut stream: TcpStream,
    src_addr: std::net::SocketAddr,
    resolver: Arc<DnsResolver>,
    query_semaphore: Arc<Semaphore>,
    rate_limiter: Arc<DnsRateLimiter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
