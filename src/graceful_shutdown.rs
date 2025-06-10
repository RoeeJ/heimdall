use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::resolver::DnsResolver;

/// Graceful shutdown coordinator
pub struct GracefulShutdown {
    shutdown_tx: broadcast::Sender<()>,
    components: Arc<Mutex<Vec<ShutdownComponent>>>,
    resolver: Arc<DnsResolver>,
}

/// Type alias for shutdown function result
type ShutdownResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Type alias for shutdown function
type ShutdownFn = Box<dyn Fn() -> tokio::task::JoinHandle<ShutdownResult> + Send + Sync>;

/// A component that needs to be shut down gracefully
struct ShutdownComponent {
    name: String,
    shutdown_fn: ShutdownFn,
}

impl GracefulShutdown {
    pub fn new(resolver: Arc<DnsResolver>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            shutdown_tx,
            components: Arc::new(Mutex::new(Vec::new())),
            resolver,
        }
    }

    /// Get a shutdown receiver for components to listen on
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Register a component for graceful shutdown
    pub async fn register_component<F, Fut>(&self, name: String, shutdown_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ShutdownResult> + Send + 'static,
    {
        let component = ShutdownComponent {
            name,
            shutdown_fn: Box::new(move || {
                let fut = shutdown_fn();
                tokio::spawn(fut)
            }),
        };

        self.components.lock().await.push(component);
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Initiating graceful shutdown...");

        // Step 1: Signal all components to stop accepting new requests
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }

        // Step 2: Wait a bit for in-flight requests to complete
        info!("Waiting for in-flight requests to complete...");
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Step 3: Shut down registered components
        let components = self.components.lock().await;
        let mut handles = Vec::new();

        for component in components.iter() {
            info!("Shutting down component: {}", component.name);
            let handle = (component.shutdown_fn)();
            handles.push((component.name.clone(), handle));
        }

        // Wait for all components to shut down (with timeout)
        for (name, handle) in handles {
            match timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(Ok(()))) => {
                    info!("Component '{}' shut down successfully", name);
                }
                Ok(Ok(Err(e))) => {
                    error!("Component '{}' shutdown failed: {}", name, e);
                }
                Ok(Err(e)) => {
                    error!("Component '{}' shutdown task panicked: {}", name, e);
                }
                Err(_) => {
                    warn!("Component '{}' shutdown timed out", name);
                }
            }
        }

        // Step 4: Save cache
        info!("Saving cache before shutdown...");
        if let Err(e) = self.resolver.save_cache().await {
            error!("Failed to save cache during shutdown: {}", e);
        } else {
            info!("Cache saved successfully during shutdown");
        }

        // Step 5: Final cleanup
        info!("Final cleanup...");
        tokio::time::sleep(Duration::from_millis(100)).await;

        info!("Graceful shutdown completed");
        Ok(())
    }
}

/// Shutdown-aware server tasks
pub mod server_tasks {
    use super::*;
    use crate::{config::DnsConfig, rate_limiter::DnsRateLimiter, resolver::DnsResolver};
    use tokio::sync::Semaphore;

    /// Run UDP server with graceful shutdown support
    pub async fn run_udp_server_graceful(
        config: DnsConfig,
        resolver: Arc<DnsResolver>,
        query_semaphore: Arc<Semaphore>,
        rate_limiter: Arc<DnsRateLimiter>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::dns::DNSPacket;
        use tokio::net::UdpSocket;
        use tracing::{error, info, warn};

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
                    let query_data = buf[..read_bytes].to_vec();
                    let sock_clone = sock.clone();

                    // Handle query in a separate task
                    tokio::spawn(async move {
                        let _permit = permit; // Keep permit alive for the duration of the query

                        match handle_dns_query(&query_data, &resolver_clone).await {
                            Ok(response_data) => {
                                // Check if response is too large for UDP and client supports EDNS
                                if response_data.len() > 512 {
                                    // Try to parse the query to check EDNS support
                                    if let Ok(query_packet) = DNSPacket::parse(&query_data) {
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
        }

        Ok(())
    }

    /// Run TCP server with graceful shutdown support
    pub async fn run_tcp_server_graceful(
        config: DnsConfig,
        resolver: Arc<DnsResolver>,
        query_semaphore: Arc<Semaphore>,
        rate_limiter: Arc<DnsRateLimiter>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::TcpListener;
        use tracing::{info, warn};

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
        }

        Ok(())
    }

    async fn handle_dns_query(
        buf: &[u8],
        resolver: &DnsResolver,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        use crate::dns::DNSPacket;
        use tracing::{debug, info, trace, warn};

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

    async fn handle_tcp_connection(
        mut stream: tokio::net::TcpStream,
        src_addr: std::net::SocketAddr,
        resolver: Arc<DnsResolver>,
        query_semaphore: Arc<Semaphore>,
        rate_limiter: Arc<DnsRateLimiter>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tracing::{debug, warn};

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
}
