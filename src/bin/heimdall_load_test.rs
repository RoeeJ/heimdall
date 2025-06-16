use clap::{Parser, ValueEnum};
use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
};
use rand::Rng;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::Semaphore;
use tokio::time::timeout;

/// Load testing tool for Heimdall DNS server
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// DNS server address to test
    #[arg(short, long, default_value = "127.0.0.1:1053")]
    server: SocketAddr,

    /// Test scenario to run
    #[arg(short = 't', long, value_enum, default_value = "mixed")]
    test_type: TestType,

    /// Number of concurrent clients
    #[arg(short, long, default_value = "100")]
    clients: usize,

    /// Queries per second per client
    #[arg(short, long, default_value = "10")]
    qps: u32,

    /// Test duration in seconds
    #[arg(short, long, default_value = "60")]
    duration: u64,

    /// Enable warmup period
    #[arg(short, long, default_value = "10")]
    warmup: u64,

    /// Timeout for individual queries (milliseconds)
    #[arg(long, default_value = "5000")]
    timeout_ms: u64,

    /// Print detailed statistics
    #[arg(short, long)]
    verbose: bool,

    /// Output format for results
    #[arg(short, long, value_enum, default_value = "human")]
    output: OutputFormat,

    /// Custom domains file for testing
    #[arg(long)]
    domains_file: Option<String>,

    /// Maximum packet loss percentage to tolerate
    #[arg(long, default_value = "1.0")]
    max_loss_percent: f64,

    /// Target percentile for latency reporting
    #[arg(long, default_value = "99")]
    percentile: u8,
}

#[derive(Debug, Clone, ValueEnum)]
enum TestType {
    /// Random A record queries
    RandomA,
    /// Cache hit test (repeated queries)
    CacheHit,
    /// Cache miss test (unique queries)
    CacheMiss,
    /// Mixed workload (80% cache hit, 20% miss)
    Mixed,
    /// NXDOMAIN responses
    NxDomain,
    /// Various record types
    RecordTypes,
    /// Malformed queries
    Malformed,
    /// Large response test
    LargeResponse,
    /// Stress test with all scenarios
    Stress,
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    /// Human-readable output
    Human,
    /// JSON output
    Json,
    /// CSV output  
    Csv,
    /// Prometheus metrics format
    Prometheus,
}

/// Statistics collector
#[derive(Debug, Default)]
struct Stats {
    queries_sent: AtomicU64,
    responses_received: AtomicU64,
    errors: AtomicU64,
    timeouts: AtomicU64,
    nxdomain: AtomicU64,
    servfail: AtomicU64,
    #[allow(dead_code)]
    cache_hits: AtomicU64,
    #[allow(dead_code)]
    cache_misses: AtomicU64,
}

impl Stats {
    fn increment_queries(&self) {
        self.queries_sent.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_responses(&self) {
        self.responses_received.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_timeouts(&self) {
        self.timeouts.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_nxdomain(&self) {
        self.nxdomain.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_servfail(&self) {
        self.servfail.fetch_add(1, Ordering::Relaxed);
    }

    fn get_summary(&self) -> StatsSummary {
        let sent = self.queries_sent.load(Ordering::Relaxed);
        let received = self.responses_received.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        let timeouts = self.timeouts.load(Ordering::Relaxed);

        StatsSummary {
            queries_sent: sent,
            responses_received: received,
            errors,
            timeouts,
            nxdomain: self.nxdomain.load(Ordering::Relaxed),
            servfail: self.servfail.load(Ordering::Relaxed),
            loss_rate: if sent > 0 {
                if received > sent {
                    0.0 // More responses than sent (timing issue)
                } else {
                    ((sent - received) as f64 / sent as f64) * 100.0
                }
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug)]
struct StatsSummary {
    queries_sent: u64,
    responses_received: u64,
    errors: u64,
    timeouts: u64,
    nxdomain: u64,
    servfail: u64,
    loss_rate: f64,
}

/// Latency histogram for percentile calculations
struct LatencyHistogram {
    buckets: Vec<AtomicU64>,
    bucket_width: Duration,
    max_latency: Duration,
}

impl LatencyHistogram {
    fn new() -> Self {
        // Create buckets: 0-1ms, 1-2ms, ..., up to 1000ms
        let bucket_width = Duration::from_micros(100); // 0.1ms buckets
        let max_latency = Duration::from_secs(1);
        let num_buckets = (max_latency.as_micros() / bucket_width.as_micros()) as usize + 1;

        let buckets = (0..num_buckets).map(|_| AtomicU64::new(0)).collect();

        Self {
            buckets,
            bucket_width,
            max_latency,
        }
    }

    fn record(&self, latency: Duration) {
        let bucket_idx = if latency > self.max_latency {
            self.buckets.len() - 1
        } else {
            (latency.as_micros() / self.bucket_width.as_micros()) as usize
        };

        if bucket_idx < self.buckets.len() {
            self.buckets[bucket_idx].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn get_percentile(&self, percentile: f64) -> Duration {
        let total: u64 = self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).sum();

        if total == 0 {
            return Duration::ZERO;
        }

        let target = ((total as f64) * percentile / 100.0) as u64;
        let mut count = 0u64;

        for (i, bucket) in self.buckets.iter().enumerate() {
            count += bucket.load(Ordering::Relaxed);
            if count >= target {
                return Duration::from_micros((i as u64) * (self.bucket_width.as_micros() as u64));
            }
        }

        self.max_latency
    }

    fn get_mean(&self) -> Duration {
        let mut total_latency = 0u128;
        let mut total_count = 0u64;

        for (i, bucket) in self.buckets.iter().enumerate() {
            let count = bucket.load(Ordering::Relaxed);
            if count > 0 {
                let bucket_latency = (i as u128) * (self.bucket_width.as_micros());
                total_latency += bucket_latency * (count as u128);
                total_count += count;
            }
        }

        if total_count == 0 {
            Duration::ZERO
        } else {
            Duration::from_micros((total_latency / total_count as u128) as u64)
        }
    }
}

/// Generate test domains based on scenario
fn generate_test_domains(test_type: &TestType, count: usize) -> Vec<String> {
    let mut domains = Vec::with_capacity(count);
    let mut rng = rand::rng();

    match test_type {
        TestType::RandomA | TestType::CacheMiss => {
            // Generate unique random domains
            for i in 0..count {
                domains.push(format!(
                    "test-{}-{}.example.com",
                    i,
                    rng.random_range(0..1000000)
                ));
            }
        }
        TestType::CacheHit => {
            // Generate a small set of domains to ensure cache hits
            let cache_domains = [
                "cached.example.com",
                "popular.example.com",
                "frequently-accessed.example.com",
                "common.example.com",
                "shared.example.com",
            ];
            for _ in 0..count {
                domains.push(cache_domains[rng.random_range(0..cache_domains.len())].to_string());
            }
        }
        TestType::Mixed => {
            // 80% from cache set, 20% unique
            let cache_domains = [
                "google.com",
                "cloudflare.com",
                "example.com",
                "github.com",
                "stackoverflow.com",
            ];
            for i in 0..count {
                if rng.random_bool(0.8) {
                    domains
                        .push(cache_domains[rng.random_range(0..cache_domains.len())].to_string());
                } else {
                    domains.push(format!(
                        "unique-{}-{}.example.com",
                        i,
                        rng.random_range(0..1000000)
                    ));
                }
            }
        }
        TestType::NxDomain => {
            // Generate non-existent domains
            for i in 0..count {
                domains.push(format!(
                    "nonexistent-{}-{}.invalid",
                    i,
                    rng.random_range(0..1000000)
                ));
            }
        }
        TestType::LargeResponse => {
            // Domains known to have large responses
            let large_response_domains = [
                "google.com",     // Multiple A records
                "cloudflare.com", // Multiple records
                "microsoft.com",  // Large response
                "amazon.com",     // Multiple records
                "facebook.com",   // Large response
            ];
            for _ in 0..count {
                domains.push(
                    large_response_domains[rng.random_range(0..large_response_domains.len())]
                        .to_string(),
                );
            }
        }
        _ => {
            // Default: mix of different domains
            for i in 0..count {
                domains.push(format!("test-{}.example.com", i));
            }
        }
    }

    domains
}

/// Create a DNS query packet
fn create_dns_query(domain: &str, query_type: DNSResourceType, id: u16) -> Vec<u8> {
    let mut packet = DNSPacket {
        header: DNSHeader {
            id,
            qr: false,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: false,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        },
        ..Default::default()
    };

    let labels: Vec<String> = domain.split('.').map(String::from).collect();
    packet.questions.push(DNSQuestion {
        labels,
        qtype: query_type,
        qclass: DNSResourceClass::IN,
    });

    packet.serialize().unwrap_or_default()
}

/// Run a single client
async fn run_client(
    client_id: usize,
    args: &Args,
    stats: Arc<Stats>,
    latency_histogram: Arc<LatencyHistogram>,
    semaphore: Arc<Semaphore>,
    stop_signal: Arc<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(&args.server).await?;

    let domains = generate_test_domains(&args.test_type, 1000);
    let mut query_id = (client_id * 1000) as u16;

    // Calculate delay between queries
    let delay = Duration::from_secs_f64(1.0 / args.qps as f64);
    let timeout_duration = Duration::from_millis(args.timeout_ms);

    loop {
        // Check if we should stop
        if *stop_signal.borrow() {
            break;
        }

        // Rate limiting
        let _permit = semaphore.acquire().await?;

        // Select domain and query type
        let domain_idx = rand::random::<u32>() as usize % domains.len();
        let domain = &domains[domain_idx];
        let query_type = match &args.test_type {
            TestType::RecordTypes => {
                // Rotate through different record types
                match rand::random::<u8>() % 6 {
                    0 => DNSResourceType::A,
                    1 => DNSResourceType::AAAA,
                    2 => DNSResourceType::MX,
                    3 => DNSResourceType::TXT,
                    4 => DNSResourceType::CNAME,
                    _ => DNSResourceType::NS,
                }
            }
            _ => DNSResourceType::A,
        };

        // Create and send query
        let query = create_dns_query(domain, query_type, query_id);
        query_id = query_id.wrapping_add(1);

        let start = Instant::now();
        stats.increment_queries();

        // Send query with timeout
        match timeout(timeout_duration, async {
            socket.send(&query).await?;
            let mut response_buf = vec![0u8; 4096];
            let len = socket.recv(&mut response_buf).await?;
            response_buf.truncate(len);
            Ok::<Vec<u8>, Box<dyn std::error::Error>>(response_buf)
        })
        .await
        {
            Ok(Ok(response)) => {
                let latency = start.elapsed();
                stats.increment_responses();
                latency_histogram.record(latency);

                // Parse response to check RCODE
                if response.len() >= 12 {
                    let rcode = response[3] & 0x0F;
                    match rcode {
                        3 => stats.increment_nxdomain(),
                        2 => stats.increment_servfail(),
                        _ => {}
                    }
                }
            }
            Ok(Err(e)) => {
                stats.increment_errors();
                if args.verbose {
                    eprintln!("Client {} query error: {}", client_id, e);
                }
            }
            Err(_) => {
                stats.increment_timeouts();
                if args.verbose {
                    eprintln!("Client {} query timeout", client_id);
                }
            }
        }

        // Delay before next query
        tokio::time::sleep(delay).await;
    }

    Ok(())
}

/// Print results in human-readable format
fn print_human_results(
    stats: &StatsSummary,
    latency_histogram: &LatencyHistogram,
    duration: Duration,
    args: &Args,
) {
    println!("\n=== Load Test Results ===");
    println!("Test Type: {:?}", args.test_type);
    println!("Duration: {:.1}s", duration.as_secs_f64());
    println!("Clients: {}", args.clients);
    println!("Target QPS per client: {}", args.qps);
    println!();

    println!("=== Query Statistics ===");
    println!("Total Queries Sent: {}", stats.queries_sent);
    println!("Total Responses: {}", stats.responses_received);
    println!("Total Errors: {}", stats.errors);
    println!("Total Timeouts: {}", stats.timeouts);
    println!("NXDOMAIN Responses: {}", stats.nxdomain);
    println!("SERVFAIL Responses: {}", stats.servfail);
    println!();

    let qps = stats.queries_sent as f64 / duration.as_secs_f64();
    let rps = stats.responses_received as f64 / duration.as_secs_f64();

    println!("=== Performance Metrics ===");
    println!("Queries/sec: {:.1}", qps);
    println!("Responses/sec: {:.1}", rps);
    println!("Packet Loss: {:.2}%", stats.loss_rate);
    println!();

    println!("=== Latency Distribution ===");
    println!(
        "Mean: {:.2}ms",
        latency_histogram.get_mean().as_secs_f64() * 1000.0
    );
    println!(
        "P50: {:.2}ms",
        latency_histogram.get_percentile(50.0).as_secs_f64() * 1000.0
    );
    println!(
        "P90: {:.2}ms",
        latency_histogram.get_percentile(90.0).as_secs_f64() * 1000.0
    );
    println!(
        "P95: {:.2}ms",
        latency_histogram.get_percentile(95.0).as_secs_f64() * 1000.0
    );
    println!(
        "P99: {:.2}ms",
        latency_histogram.get_percentile(99.0).as_secs_f64() * 1000.0
    );
    println!(
        "P99.9: {:.2}ms",
        latency_histogram.get_percentile(99.9).as_secs_f64() * 1000.0
    );

    // Check if test passed based on criteria
    println!("\n=== Test Result ===");
    if stats.loss_rate > args.max_loss_percent {
        println!(
            "❌ FAILED: Packet loss {:.2}% exceeds threshold {:.2}%",
            stats.loss_rate, args.max_loss_percent
        );
    } else {
        println!("✅ PASSED: All metrics within acceptable range");
    }
}

/// Print results in JSON format
fn print_json_results(
    stats: &StatsSummary,
    latency_histogram: &LatencyHistogram,
    duration: Duration,
    args: &Args,
) {
    let result = serde_json::json!({
        "test_config": {
            "test_type": format!("{:?}", args.test_type),
            "duration_seconds": duration.as_secs_f64(),
            "clients": args.clients,
            "qps_per_client": args.qps,
            "server": args.server.to_string(),
        },
        "query_stats": {
            "total_sent": stats.queries_sent,
            "total_received": stats.responses_received,
            "total_errors": stats.errors,
            "total_timeouts": stats.timeouts,
            "nxdomain": stats.nxdomain,
            "servfail": stats.servfail,
            "loss_rate_percent": stats.loss_rate,
        },
        "performance": {
            "queries_per_second": stats.queries_sent as f64 / duration.as_secs_f64(),
            "responses_per_second": stats.responses_received as f64 / duration.as_secs_f64(),
        },
        "latency_ms": {
            "mean": latency_histogram.get_mean().as_secs_f64() * 1000.0,
            "p50": latency_histogram.get_percentile(50.0).as_secs_f64() * 1000.0,
            "p90": latency_histogram.get_percentile(90.0).as_secs_f64() * 1000.0,
            "p95": latency_histogram.get_percentile(95.0).as_secs_f64() * 1000.0,
            "p99": latency_histogram.get_percentile(99.0).as_secs_f64() * 1000.0,
            "p99_9": latency_histogram.get_percentile(99.9).as_secs_f64() * 1000.0,
        },
        "test_passed": stats.loss_rate <= args.max_loss_percent,
    });

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("🚀 Heimdall DNS Load Test");
    println!("Target: {}", args.server);
    println!("Test Type: {:?}", args.test_type);
    println!("Clients: {}", args.clients);
    println!("Duration: {}s ({}s warmup)", args.duration, args.warmup);
    println!();

    // Initialize statistics
    let stats = Arc::new(Stats::default());
    let latency_histogram = Arc::new(LatencyHistogram::new());

    // Rate limiting semaphore
    let total_qps = args.clients * args.qps as usize;
    let semaphore = Arc::new(Semaphore::new(total_qps));

    // Stop signal for graceful shutdown
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

    // Spawn client tasks
    let mut handles = Vec::new();
    for client_id in 0..args.clients {
        let args_clone = args.clone();
        let stats_clone = Arc::clone(&stats);
        let histogram_clone = Arc::clone(&latency_histogram);
        let semaphore_clone = Arc::clone(&semaphore);
        let stop_clone = Arc::new(stop_rx.clone());

        handles.push(tokio::spawn(async move {
            if let Err(e) = run_client(
                client_id,
                &args_clone,
                stats_clone,
                histogram_clone,
                semaphore_clone,
                stop_clone,
            )
            .await
            {
                eprintln!("Client {} error: {}", client_id, e);
            }
        }));
    }

    // Warmup period
    if args.warmup > 0 {
        println!("🔥 Warming up for {}s...", args.warmup);
        tokio::time::sleep(Duration::from_secs(args.warmup)).await;

        // Reset statistics after warmup
        stats.queries_sent.store(0, Ordering::Relaxed);
        stats.responses_received.store(0, Ordering::Relaxed);
        stats.errors.store(0, Ordering::Relaxed);
        stats.timeouts.store(0, Ordering::Relaxed);
        stats.nxdomain.store(0, Ordering::Relaxed);
        stats.servfail.store(0, Ordering::Relaxed);

        // Clear histogram
        for bucket in &latency_histogram.buckets {
            bucket.store(0, Ordering::Relaxed);
        }
    }

    // Run test
    println!("🏃 Running load test for {}s...", args.duration);
    let start_time = Instant::now();

    // Progress reporting
    let progress_stats = Arc::clone(&stats);
    let progress_handle = tokio::spawn(async move {
        let mut last_queries = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let current_queries = progress_stats.queries_sent.load(Ordering::Relaxed);
            let qps = (current_queries - last_queries) / 5;
            println!("Progress: {} queries sent, {} QPS", current_queries, qps);
            last_queries = current_queries;
        }
    });

    // Wait for test duration
    tokio::time::sleep(Duration::from_secs(args.duration)).await;

    // Stop all clients
    let _ = stop_tx.send(true);
    progress_handle.abort();

    // Wait for clients to finish
    for handle in handles {
        let _ = handle.await;
    }

    let test_duration = start_time.elapsed();

    // Print results
    let summary = stats.get_summary();
    match args.output {
        OutputFormat::Human => {
            print_human_results(&summary, &latency_histogram, test_duration, &args)
        }
        OutputFormat::Json => {
            print_json_results(&summary, &latency_histogram, test_duration, &args)
        }
        _ => println!("Output format {:?} not yet implemented", args.output),
    }

    // Exit with appropriate code
    if summary.loss_rate > args.max_loss_percent {
        std::process::exit(1);
    }

    Ok(())
}
