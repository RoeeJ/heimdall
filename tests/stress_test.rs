use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use sysinfo::{System, Pid};
use heimdall::dns::{DNSPacket, enums::{DNSResourceType, DNSResourceClass}, question::DNSQuestion};

/// Stress test configuration
#[derive(Clone, Debug)]
pub struct StressTestConfig {
    /// Number of concurrent clients
    pub concurrent_clients: usize,
    /// Total number of queries to send
    pub total_queries: u64,
    /// Target server address
    pub server_addr: String,
    /// Timeout for each query
    pub query_timeout: Duration,
    /// Query types to test
    pub query_types: Vec<DNSResourceType>,
    /// Domains to query
    pub test_domains: Vec<String>,
    /// Enable EDNS in queries
    pub enable_edns: bool,
    /// EDNS buffer size to request
    pub edns_buffer_size: u16,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            concurrent_clients: 10,
            total_queries: 1000,
            server_addr: "127.0.0.1:1053".to_string(),
            query_timeout: Duration::from_secs(5),
            query_types: vec![DNSResourceType::A, DNSResourceType::AAAA, DNSResourceType::MX],
            test_domains: vec![
                "google.com".to_string(),
                "cloudflare.com".to_string(),
                "github.com".to_string(),
                "stackoverflow.com".to_string(),
                "rust-lang.org".to_string(),
            ],
            enable_edns: true,
            edns_buffer_size: 1232,
        }
    }
}

/// System resource metrics during stress testing
#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub timestamp: Instant,
    pub cpu_usage: f32,
    pub memory_usage_mb: u64,
    pub total_memory_mb: u64,
    pub memory_usage_percent: f32,
}

/// Metrics collected during stress testing
#[derive(Debug)]
pub struct StressTestMetrics {
    pub total_queries_sent: AtomicU64,
    pub total_responses_received: AtomicU64,
    pub total_timeouts: AtomicU64,
    pub total_errors: AtomicU64,
    pub min_response_time_ns: AtomicU64,
    pub max_response_time_ns: AtomicU64,
    pub total_response_time_ns: AtomicU64,
    pub start_time: Instant,
    pub end_time: parking_lot::Mutex<Option<Instant>>,
    pub resource_samples: parking_lot::Mutex<Vec<ResourceMetrics>>,
}

impl StressTestMetrics {
    pub fn new() -> Self {
        Self {
            total_queries_sent: AtomicU64::new(0),
            total_responses_received: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            min_response_time_ns: AtomicU64::new(u64::MAX),
            max_response_time_ns: AtomicU64::new(0),
            total_response_time_ns: AtomicU64::new(0),
            start_time: Instant::now(),
            end_time: parking_lot::Mutex::new(None),
            resource_samples: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn record_response(&self, response_time: Duration) {
        let response_time_ns = response_time.as_nanos() as u64;
        
        self.total_responses_received.fetch_add(1, Ordering::Relaxed);
        self.total_response_time_ns.fetch_add(response_time_ns, Ordering::Relaxed);
        
        // Update min response time
        let mut current_min = self.min_response_time_ns.load(Ordering::Relaxed);
        while response_time_ns < current_min {
            match self.min_response_time_ns.compare_exchange_weak(
                current_min,
                response_time_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }
        
        // Update max response time
        let mut current_max = self.max_response_time_ns.load(Ordering::Relaxed);
        while response_time_ns > current_max {
            match self.max_response_time_ns.compare_exchange_weak(
                current_max,
                response_time_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    pub fn record_timeout(&self) {
        self.total_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_query_sent(&self) {
        self.total_queries_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn finish(&self) {
        *self.end_time.lock() = Some(Instant::now());
    }

    pub fn queries_per_second(&self) -> f64 {
        let total_queries = self.total_queries_sent.load(Ordering::Relaxed) as f64;
        let end_time = *self.end_time.lock();
        let duration = end_time
            .unwrap_or_else(Instant::now)
            .duration_since(self.start_time)
            .as_secs_f64();
        
        if duration > 0.0 {
            total_queries / duration
        } else {
            0.0
        }
    }

    pub fn average_response_time(&self) -> Duration {
        let total_responses = self.total_responses_received.load(Ordering::Relaxed);
        let total_time_ns = self.total_response_time_ns.load(Ordering::Relaxed);
        
        if total_responses > 0 {
            Duration::from_nanos(total_time_ns / total_responses)
        } else {
            Duration::ZERO
        }
    }

    pub fn success_rate(&self) -> f64 {
        let total_sent = self.total_queries_sent.load(Ordering::Relaxed) as f64;
        let total_received = self.total_responses_received.load(Ordering::Relaxed) as f64;
        
        if total_sent > 0.0 {
            (total_received / total_sent) * 100.0
        } else {
            0.0
        }
    }

    pub fn min_response_time(&self) -> Duration {
        let min_ns = self.min_response_time_ns.load(Ordering::Relaxed);
        if min_ns == u64::MAX {
            Duration::ZERO
        } else {
            Duration::from_nanos(min_ns)
        }
    }

    pub fn max_response_time(&self) -> Duration {
        Duration::from_nanos(self.max_response_time_ns.load(Ordering::Relaxed))
    }

    pub fn record_resource_sample(&self, sample: ResourceMetrics) {
        self.resource_samples.lock().push(sample);
    }

    pub fn get_resource_summary(&self) -> Option<(f32, f32, u64, u64)> {
        let samples = self.resource_samples.lock();
        if samples.is_empty() {
            return None;
        }

        let avg_cpu = samples.iter().map(|s| s.cpu_usage).sum::<f32>() / samples.len() as f32;
        let max_cpu = samples.iter().map(|s| s.cpu_usage).fold(0.0f32, f32::max);
        let avg_memory = samples.iter().map(|s| s.memory_usage_mb).sum::<u64>() / samples.len() as u64;
        let max_memory = samples.iter().map(|s| s.memory_usage_mb).max().unwrap_or(0);

        Some((avg_cpu, max_cpu, avg_memory, max_memory))
    }
}

/// DNS stress test runner
pub struct DNSStressTester {
    config: StressTestConfig,
    metrics: Arc<StressTestMetrics>,
    monitor_resources: bool,
}

impl DNSStressTester {
    pub fn new(config: StressTestConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(StressTestMetrics::new()),
            monitor_resources: true,
        }
    }

    pub fn with_resource_monitoring(mut self, enable: bool) -> Self {
        self.monitor_resources = enable;
        self
    }

    /// Create a DNS query packet
    fn create_query(&self, domain: &str, query_type: DNSResourceType, query_id: u16) -> Result<DNSPacket, Box<dyn std::error::Error + Send + Sync>> {
        let mut packet = DNSPacket::default();
        packet.header.id = query_id;
        packet.header.rd = true; // Recursion desired
        packet.header.qdcount = 1;

        // Create question
        let mut question = DNSQuestion::default();
        question.labels = domain.split('.').map(|s| s.to_string()).collect();
        question.qtype = query_type;
        question.qclass = DNSResourceClass::IN;
        packet.questions.push(question);

        // Add EDNS if enabled
        if self.config.enable_edns {
            packet.add_edns(self.config.edns_buffer_size, false);
        }

        Ok(packet)
    }

    /// Send a single DNS query and measure response time
    async fn send_query(&self, socket: &UdpSocket, query: DNSPacket) -> Result<Duration, Box<dyn std::error::Error + Send + Sync>> {
        let query_bytes = query.serialize()
            .map_err(|e| format!("Failed to serialize query: {:?}", e))?;

        let start_time = Instant::now();
        self.metrics.record_query_sent();

        // Send query
        socket.send(&query_bytes).await?;

        // Wait for response with timeout
        let mut response_buf = vec![0u8; 4096];
        match timeout(self.config.query_timeout, socket.recv(&mut response_buf)).await {
            Ok(Ok(response_len)) => {
                let response_time = start_time.elapsed();
                
                // Try to parse the response
                match DNSPacket::parse(&response_buf[..response_len]) {
                    Ok(_response) => {
                        self.metrics.record_response(response_time);
                        Ok(response_time)
                    }
                    Err(e) => {
                        self.metrics.record_error();
                        Err(format!("Failed to parse response: {:?}", e).into())
                    }
                }
            }
            Ok(Err(e)) => {
                self.metrics.record_error();
                Err(format!("Socket error: {}", e).into())
            }
            Err(_) => {
                self.metrics.record_timeout();
                Err("Query timeout".into())
            }
        }
    }

    /// Run stress test with a single client
    async fn run_client(&self, client_id: usize, queries_per_client: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Bind a UDP socket for this client
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(&self.config.server_addr).await?;

        let mut query_id = (client_id as u16) << 8; // Use client_id as prefix for query IDs

        for i in 0..queries_per_client {
            // Cycle through domains and query types
            let domain_idx = (i as usize) % self.config.test_domains.len();
            let type_idx = (i as usize) % self.config.query_types.len();
            
            let domain = &self.config.test_domains[domain_idx];
            let query_type = self.config.query_types[type_idx];
            
            query_id = query_id.wrapping_add(1);
            
            match self.create_query(domain, query_type, query_id) {
                Ok(query) => {
                    if let Err(e) = self.send_query(&socket, query).await {
                        eprintln!("Client {} query {} failed: {}", client_id, i, e);
                    }
                }
                Err(e) => {
                    eprintln!("Client {} failed to create query {}: {}", client_id, i, e);
                    self.metrics.record_error();
                }
            }

            // Small delay to avoid overwhelming the server
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        Ok(())
    }

    /// Start resource monitoring task
    async fn start_resource_monitoring(&self) -> Option<tokio::task::JoinHandle<()>> {
        if !self.monitor_resources {
            return None;
        }

        let metrics = Arc::clone(&self.metrics);
        
        Some(tokio::spawn(async move {
            let mut system = System::new_all();
            let server_pid = Self::find_heimdall_process(&mut system);
            
            loop {
                system.refresh_all();
                
                let total_memory_mb = system.total_memory() / 1024 / 1024;
                let mut cpu_usage = 0.0;
                let mut memory_usage_mb = 0;

                if let Some(pid) = server_pid {
                    if let Some(process) = system.process(pid) {
                        cpu_usage = process.cpu_usage();
                        memory_usage_mb = process.memory() / 1024 / 1024;
                    }
                } else {
                    // If we can't find the specific process, get system averages
                    cpu_usage = system.global_cpu_info().cpu_usage();
                    memory_usage_mb = (system.total_memory() - system.available_memory()) / 1024 / 1024;
                }

                let memory_usage_percent = if total_memory_mb > 0 {
                    (memory_usage_mb as f32 / total_memory_mb as f32) * 100.0
                } else {
                    0.0
                };

                let sample = ResourceMetrics {
                    timestamp: Instant::now(),
                    cpu_usage,
                    memory_usage_mb,
                    total_memory_mb,
                    memory_usage_percent,
                };

                metrics.record_resource_sample(sample);
                
                // Sample every 500ms
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }))
    }

    /// Find the Heimdall DNS server process
    fn find_heimdall_process(system: &mut System) -> Option<Pid> {
        system.refresh_processes();
        
        for (pid, process) in system.processes() {
            let name = process.name();
            let cmd = process.cmd().join(" ");
            
            // Look for heimdall process or cargo run with heimdall
            if name.contains("heimdall") || 
               cmd.contains("heimdall") || 
               (name.contains("cargo") && cmd.contains("run")) {
                return Some(*pid);
            }
        }
        
        None
    }

    /// Run the complete stress test
    pub async fn run(&mut self) -> &StressTestMetrics {
        println!("Starting DNS stress test...");
        println!("Config: {:?}", self.config);
        println!("");

        // Start resource monitoring
        let resource_monitor = self.start_resource_monitoring().await;

        let queries_per_client = self.config.total_queries / self.config.concurrent_clients as u64;
        let remaining_queries = self.config.total_queries % self.config.concurrent_clients as u64;

        let mut handles = Vec::new();

        // Spawn concurrent clients
        for client_id in 0..self.config.concurrent_clients {
            let mut client_queries = queries_per_client;
            if client_id < remaining_queries as usize {
                client_queries += 1; // Distribute remaining queries
            }

            let tester = DNSStressTester {
                config: self.config.clone(),
                metrics: Arc::clone(&self.metrics),
                monitor_resources: self.monitor_resources,
            };

            let handle = tokio::spawn(async move {
                if let Err(e) = tester.run_client(client_id, client_queries).await {
                    eprintln!("Client {} failed: {}", client_id, e);
                }
            });

            handles.push(handle);
        }

        // Wait for all clients to complete
        for handle in handles {
            if let Err(e) = handle.await {
                eprintln!("Client task failed: {}", e);
            }
        }

        // Stop resource monitoring
        if let Some(monitor_handle) = resource_monitor {
            monitor_handle.abort();
        }

        // Finalize metrics
        self.metrics.finish();

        println!("Stress test completed!");
        self.print_results();

        &self.metrics
    }

    /// Print test results
    pub fn print_results(&self) {
        let metrics = &self.metrics;
        
        println!("\n=== DNS Stress Test Results ===");
        println!("Total queries sent:     {}", metrics.total_queries_sent.load(Ordering::Relaxed));
        println!("Successful responses:   {}", metrics.total_responses_received.load(Ordering::Relaxed));
        println!("Timeouts:              {}", metrics.total_timeouts.load(Ordering::Relaxed));
        println!("Errors:                {}", metrics.total_errors.load(Ordering::Relaxed));
        println!("Success rate:          {:.2}%", metrics.success_rate());
        println!("Queries per second:    {:.2}", metrics.queries_per_second());
        println!("Average response time: {:.2}ms", metrics.average_response_time().as_secs_f64() * 1000.0);
        println!("Min response time:     {:.2}ms", metrics.min_response_time().as_secs_f64() * 1000.0);
        println!("Max response time:     {:.2}ms", metrics.max_response_time().as_secs_f64() * 1000.0);
        
        let end_time = *metrics.end_time.lock();
        let total_duration = end_time
            .unwrap_or_else(Instant::now)
            .duration_since(metrics.start_time);
        println!("Total test duration:   {:.2}s", total_duration.as_secs_f64());

        // Print resource usage statistics
        if let Some((avg_cpu, max_cpu, avg_memory, max_memory)) = metrics.get_resource_summary() {
            println!("\n=== System Resource Usage ===");
            println!("Average CPU usage:     {:.1}%", avg_cpu);
            println!("Peak CPU usage:        {:.1}%", max_cpu);
            println!("Average memory usage:  {} MB", avg_memory);
            println!("Peak memory usage:     {} MB", max_memory);
        }
        
        println!("================================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stress_test_light_load() {
        let config = StressTestConfig {
            concurrent_clients: 2,
            total_queries: 10,
            query_timeout: Duration::from_secs(2),
            ..Default::default()
        };

        let mut tester = DNSStressTester::new(config);
        let metrics = tester.run().await;

        // Verify basic metrics
        assert!(metrics.total_queries_sent.load(Ordering::Relaxed) == 10);
        assert!(metrics.success_rate() > 0.0);
    }

    #[tokio::test]
    async fn test_stress_test_edns_disabled() {
        let config = StressTestConfig {
            concurrent_clients: 1,
            total_queries: 5,
            enable_edns: false,
            query_timeout: Duration::from_secs(2),
            ..Default::default()
        };

        let mut tester = DNSStressTester::new(config);
        let metrics = tester.run().await;

        assert!(metrics.total_queries_sent.load(Ordering::Relaxed) == 5);
    }

    #[tokio::test]
    async fn test_metrics_calculation() {
        let metrics = StressTestMetrics::new();
        
        // Test initial state
        assert_eq!(metrics.success_rate(), 0.0);
        assert_eq!(metrics.average_response_time(), Duration::ZERO);
        
        // Record some metrics
        metrics.record_query_sent();
        metrics.record_query_sent();
        metrics.record_response(Duration::from_millis(100));
        metrics.record_timeout();
        
        assert_eq!(metrics.total_queries_sent.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.total_responses_received.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.total_timeouts.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.success_rate(), 50.0);
        assert_eq!(metrics.average_response_time(), Duration::from_millis(100));
    }
}