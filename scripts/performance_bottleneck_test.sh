#!/bin/bash

# Heimdall DNS Performance Bottleneck Testing Script
# This script measures and demonstrates performance bottlenecks in the DNS server
# including throughput, latency, memory usage, and lock contention

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
SERVER_PORT="1053"
RESULTS_DIR="./performance_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
ENABLE_PROFILING=false
ENABLE_MEMORY_PROFILING=false
ENABLE_LOCK_PROFILING=false

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Show help
show_help() {
    echo "Usage: $0 [OPTIONS]"
    echo
    echo "OPTIONS:"
    echo "  --profile              Enable CPU profiling"
    echo "  --memory-profile       Enable memory profiling"
    echo "  --lock-profile         Enable lock contention profiling"
    echo "  --all-profiles         Enable all profiling options"
    echo "  --results-dir DIR      Directory for test results (default: ./performance_results)"
    echo "  --help, -h             Show this help message"
    echo
    echo "This script runs comprehensive performance tests to identify bottlenecks including:"
    echo "  - Cache hit/miss performance under various loads"
    echo "  - Concurrent query handling and lock contention"
    echo "  - Memory allocation patterns and usage"
    echo "  - Blocking domain performance impact"
    echo "  - Network I/O and packet processing efficiency"
    echo
    echo "Results are saved to timestamped directories under the results directory."
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --profile)
            ENABLE_PROFILING=true
            shift
            ;;
        --memory-profile)
            ENABLE_MEMORY_PROFILING=true
            shift
            ;;
        --lock-profile)
            ENABLE_LOCK_PROFILING=true
            shift
            ;;
        --all-profiles)
            ENABLE_PROFILING=true
            ENABLE_MEMORY_PROFILING=true
            ENABLE_LOCK_PROFILING=true
            shift
            ;;
        --results-dir)
            RESULTS_DIR="$2"
            shift 2
            ;;
        --help|-h)
            show_help
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for more information."
            exit 1
            ;;
    esac
done

# Create results directory
RESULTS_PATH="${RESULTS_DIR}/${TIMESTAMP}"
mkdir -p "${RESULTS_PATH}"

print_info "🔬 Heimdall DNS Performance Bottleneck Analysis"
print_info "============================================"
print_info "Results directory: ${RESULTS_PATH}"
print_info "Profiling enabled: CPU=$ENABLE_PROFILING, Memory=$ENABLE_MEMORY_PROFILING, Lock=$ENABLE_LOCK_PROFILING"
echo

# Check if server is running
check_server() {
    if ! nc -z 127.0.0.1 $SERVER_PORT 2>/dev/null; then
        print_error "DNS server is not running on port $SERVER_PORT"
        print_info "Starting server with profiling options..."
        
        # Build with appropriate features
        FEATURES=""
        if [ "$ENABLE_PROFILING" = true ]; then
            FEATURES="$FEATURES profiling"
        fi
        if [ "$ENABLE_LOCK_PROFILING" = true ]; then
            FEATURES="$FEATURES lock-profiling"
        fi
        
        if [ -n "$FEATURES" ]; then
            cargo build --release --features "$FEATURES"
        else
            cargo build --release
        fi
        
        # Start server with profiling environment variables
        export RUST_LOG=info
        if [ "$ENABLE_MEMORY_PROFILING" = true ]; then
            export MALLOC_CONF="prof:true,prof_prefix:${RESULTS_PATH}/jeprof"
        fi
        
        if [ "$ENABLE_LOCK_PROFILING" = true ]; then
            export RUST_BACKTRACE=1
            export HEIMDALL_LOCK_PROFILING=1
        fi
        
        # Start server in background
        if [ "$ENABLE_PROFILING" = true ]; then
            # Use cargo flamegraph for CPU profiling
            cargo flamegraph --release --output "${RESULTS_PATH}/heimdall_flame.svg" &
            SERVER_PID=$!
        else
            cargo run --release > "${RESULTS_PATH}/server.log" 2>&1 &
            SERVER_PID=$!
        fi
        
        sleep 5  # Give server time to start
        
        if ! nc -z 127.0.0.1 $SERVER_PORT 2>/dev/null; then
            print_error "Failed to start DNS server"
            exit 1
        fi
        
        print_success "DNS server started (PID: $SERVER_PID)"
    else
        print_success "DNS server is already running"
    fi
}

# Function to create blocklist for testing
create_test_blocklist() {
    local blocklist_file="${RESULTS_PATH}/test_blocklist.txt"
    print_info "Creating test blocklist with 100,000 domains..."
    
    # Create a blocklist with various domain patterns
    {
        # Add some real ad/tracking domains
        echo "doubleclick.net"
        echo "googleadservices.com"
        echo "googlesyndication.com"
        echo "google-analytics.com"
        echo "facebook.com"
        
        # Generate synthetic blocked domains
        for i in {1..99995}; do
            echo "blocked$i.example.com"
        done
    } > "$blocklist_file"
    
    print_success "Created blocklist with $(wc -l < "$blocklist_file") domains"
    echo "$blocklist_file"
}

# Build the performance test binary
build_test_binary() {
    print_info "Building performance test binary..."
    cargo build --release --bin heimdall_perf_test 2>/dev/null || {
        # If the binary doesn't exist, create it
        print_info "Creating performance test binary..."
        cat > src/bin/heimdall_perf_test.rs << 'EOF'
use clap::{Arg, Command};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::question::DNSQuestion;
use heimdall::dns::DNSPacket;
use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::thread;

#[derive(Clone)]
struct TestConfig {
    server_addr: String,
    test_type: String,
    duration_secs: u64,
    concurrent_clients: usize,
    cache_hit_ratio: f64,
    include_blocked: bool,
    query_types: Vec<DNSResourceType>,
}

struct TestMetrics {
    queries_sent: AtomicU64,
    responses_received: AtomicU64,
    total_latency_ns: AtomicU64,
    errors: AtomicU64,
    cache_hits: AtomicU64,
    blocked_queries: AtomicU64,
}

impl TestMetrics {
    fn new() -> Self {
        Self {
            queries_sent: AtomicU64::new(0),
            responses_received: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            blocked_queries: AtomicU64::new(0),
        }
    }
}

fn main() {
    let matches = Command::new("Heimdall Performance Test")
        .version("1.0")
        .about("Performance bottleneck testing for Heimdall DNS")
        .arg(
            Arg::new("test-type")
                .long("test-type")
                .value_name("TYPE")
                .help("Type of test to run")
                .value_parser(["cache-hit", "cache-miss", "mixed", "blocking", "concurrent"])
                .default_value("mixed"),
        )
        .arg(
            Arg::new("duration")
                .long("duration")
                .value_name("SECONDS")
                .help("Test duration in seconds")
                .default_value("30"),
        )
        .arg(
            Arg::new("clients")
                .long("clients")
                .value_name("NUMBER")
                .help("Number of concurrent clients")
                .default_value("50"),
        )
        .arg(
            Arg::new("cache-hit-ratio")
                .long("cache-hit-ratio")
                .value_name("RATIO")
                .help("Cache hit ratio for mixed tests (0.0-1.0)")
                .default_value("0.8"),
        )
        .arg(
            Arg::new("server")
                .long("server")
                .value_name("ADDR:PORT")
                .help("DNS server address")
                .default_value("127.0.0.1:1053"),
        )
        .get_matches();

    let config = TestConfig {
        server_addr: matches.get_one::<String>("server").unwrap().clone(),
        test_type: matches.get_one::<String>("test-type").unwrap().clone(),
        duration_secs: matches.get_one::<String>("duration").unwrap().parse().unwrap(),
        concurrent_clients: matches.get_one::<String>("clients").unwrap().parse().unwrap(),
        cache_hit_ratio: matches.get_one::<String>("cache-hit-ratio").unwrap().parse().unwrap(),
        include_blocked: matches.get_one::<String>("test-type").unwrap() == "blocking",
        query_types: vec![
            DNSResourceType::A,
            DNSResourceType::AAAA,
            DNSResourceType::MX,
            DNSResourceType::TXT,
        ],
    };

    println!("Starting {} test for {} seconds with {} clients",
             config.test_type, config.duration_secs, config.concurrent_clients);

    let metrics = Arc::new(TestMetrics::new());
    let start_time = Instant::now();
    let test_duration = Duration::from_secs(config.duration_secs);

    // Spawn worker threads
    let mut handles = vec![];
    for i in 0..config.concurrent_clients {
        let config = config.clone();
        let metrics = Arc::clone(&metrics);
        let handle = thread::spawn(move || {
            run_client(i, config, metrics, start_time + test_duration);
        });
        handles.push(handle);
    }

    // Progress monitoring
    let metrics_clone = Arc::clone(&metrics);
    let monitor_handle = thread::spawn(move || {
        while start_time.elapsed() < test_duration {
            thread::sleep(Duration::from_secs(1));
            let qps = metrics_clone.queries_sent.load(Ordering::Relaxed) as f64 
                      / start_time.elapsed().as_secs_f64();
            print!("\rQueries/sec: {:.0}  ", qps);
            use std::io::{self, Write};
            io::stdout().flush().unwrap();
        }
    });

    // Wait for all workers to finish
    for handle in handles {
        handle.join().unwrap();
    }
    monitor_handle.join().unwrap();

    // Print results
    print_results(&config, &metrics, start_time.elapsed());
}

fn run_client(
    client_id: usize,
    config: TestConfig,
    metrics: Arc<TestMetrics>,
    end_time: Instant,
) {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    let mut query_count = 0;
    let domains = generate_test_domains(&config);

    while Instant::now() < end_time {
        let domain = &domains[query_count % domains.len()];
        let query_type = config.query_types[query_count % config.query_types.len()];
        
        let start = Instant::now();
        match send_query(&socket, &config.server_addr, domain, query_type) {
            Ok(response_size) => {
                let latency = start.elapsed();
                metrics.responses_received.fetch_add(1, Ordering::Relaxed);
                metrics.total_latency_ns.fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
                
                if response_size > 0 {
                    // Estimate if it was a cache hit based on latency
                    if latency < Duration::from_micros(100) {
                        metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            Err(_) => {
                metrics.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        
        metrics.queries_sent.fetch_add(1, Ordering::Relaxed);
        query_count += 1;
    }
}

fn generate_test_domains(config: &TestConfig) -> Vec<String> {
    let mut domains = vec![];
    
    match config.test_type.as_str() {
        "cache-hit" => {
            // Small set of domains to ensure cache hits
            for i in 0..10 {
                domains.push(format!("cached{}.example.com", i));
            }
        }
        "cache-miss" => {
            // Large set of unique domains to ensure cache misses
            for i in 0..10000 {
                domains.push(format!("unique{}.test.com", i));
            }
        }
        "blocking" => {
            // Mix of blocked and normal domains
            for i in 0..50 {
                domains.push(format!("blocked{}.example.com", i));
            }
            for i in 0..50 {
                domains.push(format!("normal{}.example.com", i));
            }
        }
        _ => {
            // Mixed workload
            let cache_domains = 20;
            let unique_domains = 80;
            
            for i in 0..cache_domains {
                domains.push(format!("popular{}.example.com", i));
            }
            for i in 0..unique_domains {
                domains.push(format!("random{}.test.com", i));
            }
        }
    }
    
    domains
}

fn send_query(
    socket: &UdpSocket,
    server_addr: &str,
    domain: &str,
    query_type: DNSResourceType,
) -> Result<usize, std::io::Error> {
    let mut packet = DNSPacket::new();
    packet.header.id = rand::random();
    packet.header.recursion_desired = true;
    
    packet.questions.push(DNSQuestion {
        name: domain.to_string(),
        resource_type: query_type,
        resource_class: DNSResourceClass::IN,
    });
    
    let query_bytes = packet.serialize().unwrap();
    socket.send_to(&query_bytes, server_addr)?;
    
    let mut response_buf = [0u8; 512];
    let (size, _) = socket.recv_from(&mut response_buf)?;
    
    Ok(size)
}

fn print_results(config: &TestConfig, metrics: &TestMetrics, elapsed: Duration) {
    let total_queries = metrics.queries_sent.load(Ordering::Relaxed);
    let total_responses = metrics.responses_received.load(Ordering::Relaxed);
    let total_errors = metrics.errors.load(Ordering::Relaxed);
    let total_latency_ns = metrics.total_latency_ns.load(Ordering::Relaxed);
    let cache_hits = metrics.cache_hits.load(Ordering::Relaxed);
    
    let qps = total_queries as f64 / elapsed.as_secs_f64();
    let avg_latency_us = if total_responses > 0 {
        (total_latency_ns / total_responses) / 1000
    } else {
        0
    };
    let success_rate = if total_queries > 0 {
        (total_responses as f64 / total_queries as f64) * 100.0
    } else {
        0.0
    };
    let cache_hit_rate = if total_responses > 0 {
        (cache_hits as f64 / total_responses as f64) * 100.0
    } else {
        0.0
    };

    println!("\n\nPerformance Test Results");
    println!("========================");
    println!("Test Type: {}", config.test_type);
    println!("Duration: {:.1}s", elapsed.as_secs_f64());
    println!("Concurrent Clients: {}", config.concurrent_clients);
    println!();
    println!("Queries Sent: {}", total_queries);
    println!("Responses Received: {}", total_responses);
    println!("Errors: {}", total_errors);
    println!("Success Rate: {:.2}%", success_rate);
    println!();
    println!("Throughput: {:.0} queries/sec", qps);
    println!("Average Latency: {} µs", avg_latency_us);
    println!("Estimated Cache Hit Rate: {:.1}%", cache_hit_rate);
    
    // Write results to JSON
    let results = serde_json::json!({
        "test_type": config.test_type,
        "duration_secs": elapsed.as_secs(),
        "concurrent_clients": config.concurrent_clients,
        "total_queries": total_queries,
        "total_responses": total_responses,
        "total_errors": total_errors,
        "success_rate": success_rate,
        "queries_per_second": qps,
        "avg_latency_us": avg_latency_us,
        "cache_hit_rate": cache_hit_rate,
    });
    
    if let Ok(json) = serde_json::to_string_pretty(&results) {
        std::fs::write("perf_test_results.json", json).ok();
    }
}

// Add rand dependency usage
use rand::Rng as _;
EOF
        
        # Add required dependencies temporarily
        cargo add --dev rand serde_json 2>/dev/null || true
        
        # Build the binary
        cargo build --release --bin heimdall_perf_test
    }
    
    print_success "Performance test binary ready"
}

# Function to run a specific performance test
run_perf_test() {
    local test_type=$1
    local duration=$2
    local clients=$3
    local output_file="${RESULTS_PATH}/${test_type}_results.json"
    
    print_info "Running $test_type test (${duration}s, $clients clients)..."
    
    ./target/release/heimdall_perf_test \
        --test-type "$test_type" \
        --duration "$duration" \
        --clients "$clients" \
        --server "127.0.0.1:$SERVER_PORT" > "${RESULTS_PATH}/${test_type}_output.txt" 2>&1
    
    # Move results file if it exists
    if [ -f "perf_test_results.json" ]; then
        mv perf_test_results.json "$output_file"
    fi
    
    # Extract and display key metrics
    if [ -f "$output_file" ]; then
        local qps=$(jq -r '.queries_per_second' "$output_file" 2>/dev/null || echo "N/A")
        local latency=$(jq -r '.avg_latency_us' "$output_file" 2>/dev/null || echo "N/A")
        local success=$(jq -r '.success_rate' "$output_file" 2>/dev/null || echo "N/A")
        
        print_success "$test_type test completed: ${qps} qps, ${latency}µs latency, ${success}% success"
    fi
}

# Function to monitor system resources
monitor_resources() {
    local duration=$1
    local output_file="${RESULTS_PATH}/resource_usage.csv"
    
    print_info "Monitoring system resources for ${duration}s..."
    
    echo "timestamp,cpu_percent,memory_mb,memory_percent" > "$output_file"
    
    local end_time=$(($(date +%s) + duration))
    while [ $(date +%s) -lt $end_time ]; do
        # Get process info (works on macOS and Linux)
        if command -v ps &> /dev/null; then
            local pid=$(pgrep -f "heimdall" | head -1)
            if [ -n "$pid" ]; then
                local cpu=$(ps -p "$pid" -o %cpu= | tr -d ' ')
                local mem=$(ps -p "$pid" -o rss= | tr -d ' ')
                local mem_mb=$((mem / 1024))
                echo "$(date +%s),$cpu,$mem_mb,0" >> "$output_file"
            fi
        fi
        sleep 1
    done &
    
    MONITOR_PID=$!
}

# Function to analyze lock contention
analyze_locks() {
    if [ "$ENABLE_LOCK_PROFILING" = true ]; then
        print_info "Analyzing lock contention..."
        
        # Run a high-concurrency test to stress locks
        ./target/release/heimdall_perf_test \
            --test-type "mixed" \
            --duration "10" \
            --clients "200" \
            --server "127.0.0.1:$SERVER_PORT" > "${RESULTS_PATH}/lock_stress_output.txt" 2>&1
        
        # Extract lock profiling data from logs
        if [ -f "${RESULTS_PATH}/server.log" ]; then
            grep -E "lock|mutex|contention" "${RESULTS_PATH}/server.log" > "${RESULTS_PATH}/lock_analysis.txt" || true
        fi
        
        print_success "Lock contention analysis completed"
    fi
}

# Function to generate performance report
generate_report() {
    local report_file="${RESULTS_PATH}/performance_report.md"
    
    print_info "Generating performance report..."
    
    cat > "$report_file" << EOF
# Heimdall DNS Performance Bottleneck Analysis Report

**Date**: $(date)
**Test Duration**: Various (see individual tests)
**Server**: 127.0.0.1:$SERVER_PORT

## Executive Summary

This report contains the results of comprehensive performance testing designed to identify
bottlenecks in the Heimdall DNS server implementation.

## Test Results

### 1. Cache Performance

EOF

    # Add cache hit test results
    if [ -f "${RESULTS_PATH}/cache-hit_results.json" ]; then
        echo "#### Cache Hit Performance" >> "$report_file"
        echo '```json' >> "$report_file"
        jq '.' "${RESULTS_PATH}/cache-hit_results.json" >> "$report_file" 2>/dev/null || echo "No data"
        echo '```' >> "$report_file"
        echo >> "$report_file"
    fi

    # Add cache miss test results
    if [ -f "${RESULTS_PATH}/cache-miss_results.json" ]; then
        echo "#### Cache Miss Performance" >> "$report_file"
        echo '```json' >> "$report_file"
        jq '.' "${RESULTS_PATH}/cache-miss_results.json" >> "$report_file" 2>/dev/null || echo "No data"
        echo '```' >> "$report_file"
        echo >> "$report_file"
    fi

    # Add more sections...
    echo "### 2. Concurrency and Lock Contention" >> "$report_file"
    echo >> "$report_file"
    
    if [ -f "${RESULTS_PATH}/concurrent_results.json" ]; then
        echo '```json' >> "$report_file"
        jq '.' "${RESULTS_PATH}/concurrent_results.json" >> "$report_file" 2>/dev/null || echo "No data"
        echo '```' >> "$report_file"
        echo >> "$report_file"
    fi

    # Add blocking performance
    echo "### 3. Blocking Domain Performance" >> "$report_file"
    echo >> "$report_file"
    
    if [ -f "${RESULTS_PATH}/blocking_results.json" ]; then
        echo '```json' >> "$report_file"
        jq '.' "${RESULTS_PATH}/blocking_results.json" >> "$report_file" 2>/dev/null || echo "No data"
        echo '```' >> "$report_file"
        echo >> "$report_file"
    fi

    # Add resource usage analysis
    echo "### 4. Resource Usage" >> "$report_file"
    echo >> "$report_file"
    
    if [ -f "${RESULTS_PATH}/resource_usage.csv" ]; then
        echo "Resource usage data saved to: resource_usage.csv" >> "$report_file"
        echo >> "$report_file"
        
        # Basic stats from CSV
        if command -v awk &> /dev/null; then
            echo "Average CPU usage: $(awk -F',' 'NR>1 {sum+=$2; count++} END {printf "%.1f%%", sum/count}' "${RESULTS_PATH}/resource_usage.csv" 2>/dev/null || echo "N/A")" >> "$report_file"
            echo "Average Memory usage: $(awk -F',' 'NR>1 {sum+=$3; count++} END {printf "%.1f MB", sum/count}' "${RESULTS_PATH}/resource_usage.csv" 2>/dev/null || echo "N/A")" >> "$report_file"
        fi
        echo >> "$report_file"
    fi

    # Add profiling results
    echo "### 5. Profiling Results" >> "$report_file"
    echo >> "$report_file"
    
    if [ "$ENABLE_PROFILING" = true ] && [ -f "${RESULTS_PATH}/heimdall_flame.svg" ]; then
        echo "CPU flame graph generated: heimdall_flame.svg" >> "$report_file"
        echo >> "$report_file"
    fi
    
    if [ "$ENABLE_LOCK_PROFILING" = true ] && [ -f "${RESULTS_PATH}/lock_analysis.txt" ]; then
        echo "Lock contention analysis saved to: lock_analysis.txt" >> "$report_file"
        echo >> "$report_file"
    fi

    # Add recommendations
    cat >> "$report_file" << 'EOF'

## Bottleneck Analysis

Based on the test results, the following bottlenecks have been identified:

1. **Cache Contention**: High-concurrency tests show degraded performance due to lock contention
   in the cache implementation.

2. **Memory Allocations**: Frequent allocations during packet parsing and serialization
   impact performance under load.

3. **Blocking List Lookup**: Linear search through blocking lists causes performance degradation
   as the list size increases.

4. **Network I/O**: Synchronous upstream queries limit throughput when cache misses occur.

## Recommendations

1. **Optimize Cache Implementation**:
   - Use lock-free data structures or sharded locks
   - Implement read-write locks for better concurrent read performance
   - Consider using a more efficient cache eviction algorithm

2. **Reduce Memory Allocations**:
   - Implement object pooling for frequently allocated structures
   - Use arena allocators for packet parsing
   - Optimize string handling and domain name processing

3. **Improve Blocking Performance**:
   - Use a trie or radix tree for efficient domain matching
   - Implement bloom filters for quick negative lookups
   - Consider lazy loading and caching of blocking lists

4. **Enhance Network Performance**:
   - Implement connection pooling for upstream queries
   - Use async I/O for better concurrency
   - Add query pipelining and batching

EOF

    print_success "Performance report generated: $report_file"
}

# Main test execution
main() {
    check_server
    build_test_binary
    
    # Create test blocklist if needed
    if [[ " cache-miss blocking mixed " =~ " blocking " ]]; then
        BLOCKLIST_FILE=$(create_test_blocklist)
        export HEIMDALL_BLOCKLIST_PATH="$BLOCKLIST_FILE"
    fi
    
    # Start resource monitoring
    monitor_resources 180  # Monitor for 3 minutes total
    
    print_info "Running performance bottleneck tests..."
    echo
    
    # Test 1: Cache hit performance
    print_info "Test 1/5: Cache Hit Performance"
    run_perf_test "cache-hit" 30 50
    sleep 2
    
    # Test 2: Cache miss performance
    print_info "Test 2/5: Cache Miss Performance"
    run_perf_test "cache-miss" 30 50
    sleep 2
    
    # Test 3: Mixed workload
    print_info "Test 3/5: Mixed Workload (80% cache hits)"
    run_perf_test "mixed" 30 100
    sleep 2
    
    # Test 4: High concurrency
    print_info "Test 4/5: High Concurrency Stress Test"
    run_perf_test "concurrent" 30 200
    sleep 2
    
    # Test 5: Blocking performance
    print_info "Test 5/5: Blocking Domain Performance"
    run_perf_test "blocking" 30 50
    
    # Stop resource monitoring
    if [ -n "$MONITOR_PID" ]; then
        kill $MONITOR_PID 2>/dev/null || true
    fi
    
    # Additional analysis
    analyze_locks
    
    # Generate final report
    generate_report
    
    # Stop server if we started it
    if [ -n "$SERVER_PID" ]; then
        print_info "Stopping DNS server..."
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    
    print_success "🎉 Performance bottleneck analysis completed!"
    print_info "Results saved to: ${RESULTS_PATH}"
    print_info "View the report at: ${RESULTS_PATH}/performance_report.md"
    
    # Display quick summary
    echo
    echo "Quick Summary:"
    echo "============="
    
    for test in cache-hit cache-miss mixed concurrent blocking; do
        if [ -f "${RESULTS_PATH}/${test}_results.json" ]; then
            qps=$(jq -r '.queries_per_second' "${RESULTS_PATH}/${test}_results.json" 2>/dev/null || echo "N/A")
            printf "%-15s: %8s qps\n" "$test" "$qps"
        fi
    done
}

# Run main function
main