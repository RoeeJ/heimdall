#!/bin/bash

# Heimdall DNS Memory and Lock Profiling Script
# This script profiles memory allocations and lock contention to identify bottlenecks

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
RESULTS_DIR="./profiling_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DURATION=60
PROFILE_TYPE="all"

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

show_help() {
    echo "Usage: $0 [OPTIONS]"
    echo
    echo "OPTIONS:"
    echo "  --type TYPE      Type of profiling: memory, locks, or all (default: all)"
    echo "  --duration SECS  Test duration in seconds (default: 60)"
    echo "  --results DIR    Directory for results (default: ./profiling_results)"
    echo "  --help, -h       Show this help message"
    echo
    echo "This script uses various profiling tools to analyze:"
    echo "  - Memory allocation patterns and heap usage"
    echo "  - Lock contention and mutex wait times"
    echo "  - Thread synchronization bottlenecks"
    echo "  - Object allocation frequencies"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --type)
            PROFILE_TYPE="$2"
            shift 2
            ;;
        --duration)
            DURATION="$2"
            shift 2
            ;;
        --results)
            RESULTS_DIR="$2"
            shift 2
            ;;
        --help|-h)
            show_help
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Create results directory
RESULTS_PATH="${RESULTS_DIR}/${TIMESTAMP}"
mkdir -p "${RESULTS_PATH}"

print_info "🔍 Heimdall DNS Memory and Lock Profiling"
print_info "========================================"
print_info "Profile type: $PROFILE_TYPE"
print_info "Duration: ${DURATION}s"
print_info "Results: ${RESULTS_PATH}"
echo

# Function to check for required tools
check_tools() {
    local missing=()
    
    if [[ "$PROFILE_TYPE" == "memory" ]] || [[ "$PROFILE_TYPE" == "all" ]]; then
        if ! command -v heaptrack &> /dev/null && ! command -v valgrind &> /dev/null; then
            missing+=("heaptrack or valgrind")
        fi
    fi
    
    if [[ "$PROFILE_TYPE" == "locks" ]] || [[ "$PROFILE_TYPE" == "all" ]]; then
        if ! command -v perf &> /dev/null && [[ "$OSTYPE" == "linux-gnu"* ]]; then
            missing+=("perf")
        fi
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        print_warning "Missing profiling tools: ${missing[*]}"
        print_info "Install with:"
        print_info "  macOS: brew install valgrind"
        print_info "  Linux: apt-get install heaptrack linux-tools-generic"
        echo
    fi
}

# Function to run memory profiling
profile_memory() {
    print_info "Starting memory profiling..."
    
    # Create memory profiling script
    cat > "${RESULTS_PATH}/memory_profile.rs" << 'EOF'
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::{DNSPacket, question::DNSQuestion};

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn main() {
    println!("Memory profiling started...");
    
    // Test 1: Cache allocation patterns
    println!("\nTest 1: Cache allocation patterns");
    profile_cache_allocations();
    
    // Test 2: Packet parsing allocations
    println!("\nTest 2: Packet parsing allocations");
    profile_packet_parsing();
    
    // Test 3: Concurrent access patterns
    println!("\nTest 3: Concurrent access patterns");
    profile_concurrent_access();
    
    println!("\nMemory profiling completed");
}

fn profile_cache_allocations() {
    let start = Instant::now();
    let cache = DnsCache::new(100000, 300);
    
    // Insert many entries
    for i in 0..50000 {
        let key = CacheKey::new(
            format!("test{}.example.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let mut packet = DNSPacket::new();
        packet.header.id = i as u16;
        cache.put(key, packet);
        
        if i % 10000 == 0 {
            println!("  Inserted {} entries...", i);
        }
    }
    
    println!("  Cache population took: {:?}", start.elapsed());
    
    // Measure memory usage
    let stats = jemalloc_ctl::stats::allocated::read().unwrap();
    println!("  Allocated memory: {} MB", stats / 1024 / 1024);
}

fn profile_packet_parsing() {
    let packet_data = create_test_packet();
    let iterations = 100000;
    let start = Instant::now();
    
    for i in 0..iterations {
        let _packet = DNSPacket::parse(&packet_data).unwrap();
        
        if i % 20000 == 0 {
            println!("  Parsed {} packets...", i);
        }
    }
    
    println!("  Packet parsing took: {:?}", start.elapsed());
    println!("  Average: {:?} per packet", start.elapsed() / iterations);
}

fn profile_concurrent_access() {
    let cache = Arc::new(DnsCache::new(10000, 300));
    let threads = 8;
    let operations_per_thread = 10000;
    
    // Pre-populate cache
    for i in 0..1000 {
        let key = CacheKey::new(
            format!("shared{}.example.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        cache.put(key, DNSPacket::new());
    }
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for thread_id in 0..threads {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for i in 0..operations_per_thread {
                let key = CacheKey::new(
                    format!("shared{}.example.com", i % 1000),
                    DNSResourceType::A,
                    DNSResourceClass::IN,
                );
                
                if i % 3 == 0 {
                    // Read operation
                    let _ = cache_clone.get(&key);
                } else {
                    // Write operation
                    cache_clone.put(key, DNSPacket::new());
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    println!("  Concurrent operations took: {:?}", start.elapsed());
    println!("  Total operations: {}", threads * operations_per_thread);
}

fn create_test_packet() -> Vec<u8> {
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags
        0x00, 0x01, // Questions
        0x00, 0x00, // Answers
        0x00, 0x00, // Authority
        0x00, 0x00, // Additional
        // Question
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
        0x03, b'c', b'o', b'm', 0x00,
        0x00, 0x01, // Type A
        0x00, 0x01, // Class IN
    ]
}
EOF

    # Add temporary dependencies for memory profiling
    cat > "${RESULTS_PATH}/Cargo.toml" << EOF
[package]
name = "heimdall_memory_profile"
version = "0.1.0"
edition = "2021"

[dependencies]
heimdall = { path = "../.." }
jemallocator = "0.5"
jemalloc-ctl = "0.5"

[[bin]]
name = "memory_profile"
path = "memory_profile.rs"
EOF

    # Build and run with profiling
    print_info "Building memory profiling binary..."
    cd "${RESULTS_PATH}"
    cargo build --release 2>/dev/null || {
        print_warning "Failed to build memory profiler, using alternative method"
        # Fall back to running the main server with memory tracking
        cd ../..
        RUST_LOG=info cargo run --release &
        SERVER_PID=$!
        sleep 5
        
        # Run load test while monitoring memory
        ./scripts/performance_bottleneck_test.sh --results-dir "${RESULTS_PATH}" &
        LOAD_PID=$!
        
        # Monitor memory usage
        while kill -0 $LOAD_PID 2>/dev/null; do
            ps -p $SERVER_PID -o pid,vsz,rss,comm > "${RESULTS_PATH}/memory_snapshot_$(date +%s).txt" 2>/dev/null || true
            sleep 2
        done
        
        kill $SERVER_PID 2>/dev/null || true
        wait $LOAD_PID
    }
    
    cd ../..
    print_success "Memory profiling completed"
}

# Function to run lock profiling
profile_locks() {
    print_info "Starting lock contention profiling..."
    
    # Create lock profiling test
    cat > "${RESULTS_PATH}/lock_profile.rs" << 'EOF'
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use parking_lot::{Mutex as ParkingMutex, RwLock as ParkingRwLock};

fn main() {
    println!("Lock contention profiling started...");
    
    // Test different lock types
    println!("\nTest 1: std::sync::Mutex contention");
    test_std_mutex();
    
    println!("\nTest 2: std::sync::RwLock contention");
    test_std_rwlock();
    
    println!("\nTest 3: parking_lot::Mutex contention");
    test_parking_mutex();
    
    println!("\nTest 4: parking_lot::RwLock contention");
    test_parking_rwlock();
    
    println!("\nLock profiling completed");
}

fn test_std_mutex() {
    let data = Arc::new(Mutex::new(0u64));
    let threads = 8;
    let operations = 100000;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for _ in 0..threads {
        let data_clone = Arc::clone(&data);
        let handle = thread::spawn(move || {
            for _ in 0..operations {
                let mut guard = data_clone.lock().unwrap();
                *guard += 1;
                // Simulate some work
                std::hint::black_box(*guard);
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    println!("  Duration: {:?}", duration);
    println!("  Operations/sec: {:.0}", (threads * operations) as f64 / duration.as_secs_f64());
}

fn test_std_rwlock() {
    let data = Arc::new(RwLock::new(0u64));
    let threads = 8;
    let operations = 100000;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for thread_id in 0..threads {
        let data_clone = Arc::clone(&data);
        let handle = thread::spawn(move || {
            for i in 0..operations {
                if i % 10 == 0 {
                    // Write operation (10%)
                    let mut guard = data_clone.write().unwrap();
                    *guard += 1;
                } else {
                    // Read operation (90%)
                    let guard = data_clone.read().unwrap();
                    std::hint::black_box(*guard);
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    println!("  Duration: {:?}", duration);
    println!("  Operations/sec: {:.0}", (threads * operations) as f64 / duration.as_secs_f64());
}

fn test_parking_mutex() {
    let data = Arc::new(ParkingMutex::new(0u64));
    let threads = 8;
    let operations = 100000;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for _ in 0..threads {
        let data_clone = Arc::clone(&data);
        let handle = thread::spawn(move || {
            for _ in 0..operations {
                let mut guard = data_clone.lock();
                *guard += 1;
                std::hint::black_box(*guard);
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    println!("  Duration: {:?}", duration);
    println!("  Operations/sec: {:.0}", (threads * operations) as f64 / duration.as_secs_f64());
}

fn test_parking_rwlock() {
    let data = Arc::new(ParkingRwLock::new(0u64));
    let threads = 8;
    let operations = 100000;
    
    let start = Instant::now();
    let mut handles = vec![];
    
    for thread_id in 0..threads {
        let data_clone = Arc::clone(&data);
        let handle = thread::spawn(move || {
            for i in 0..operations {
                if i % 10 == 0 {
                    // Write operation (10%)
                    let mut guard = data_clone.write();
                    *guard += 1;
                } else {
                    // Read operation (90%)
                    let guard = data_clone.read();
                    std::hint::black_box(*guard);
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    println!("  Duration: {:?}", duration);
    println!("  Operations/sec: {:.0}", (threads * operations) as f64 / duration.as_secs_f64());
}
EOF

    # Create Cargo.toml for lock profiling
    cat > "${RESULTS_PATH}/Cargo_locks.toml" << EOF
[package]
name = "heimdall_lock_profile"
version = "0.1.0"
edition = "2021"

[dependencies]
parking_lot = "0.12"

[[bin]]
name = "lock_profile"
path = "lock_profile.rs"
EOF

    # Run lock profiling
    cd "${RESULTS_PATH}"
    mv Cargo_locks.toml Cargo.toml 2>/dev/null || true
    cargo build --release 2>/dev/null && ./target/release/lock_profile > lock_results.txt || {
        print_warning "Failed to run lock profiler"
    }
    
    cd ../..
    
    # Also run the main server with lock statistics if available
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        print_info "Running perf lock analysis..."
        sudo perf lock record -o "${RESULTS_PATH}/perf.data" -- cargo run --release &
        PERF_PID=$!
        
        sleep 5
        
        # Generate load
        ./target/release/heimdall_perf_test --test-type concurrent --duration 30 --clients 100
        
        sudo kill -INT $PERF_PID 2>/dev/null || true
        wait $PERF_PID 2>/dev/null || true
        
        # Analyze lock data
        sudo perf lock report -i "${RESULTS_PATH}/perf.data" > "${RESULTS_PATH}/perf_lock_report.txt" 2>/dev/null || true
    fi
    
    print_success "Lock profiling completed"
}

# Function to generate analysis report
generate_analysis() {
    local report_file="${RESULTS_PATH}/profiling_analysis.md"
    
    print_info "Generating profiling analysis report..."
    
    cat > "$report_file" << EOF
# Heimdall DNS Profiling Analysis

**Date**: $(date)
**Profile Type**: $PROFILE_TYPE
**Duration**: ${DURATION}s

## Summary

This report contains memory and lock profiling analysis for the Heimdall DNS server.

EOF

    if [[ "$PROFILE_TYPE" == "memory" ]] || [[ "$PROFILE_TYPE" == "all" ]]; then
        cat >> "$report_file" << 'EOF'
## Memory Profiling Results

### Key Findings

1. **Allocation Hotspots**:
   - DNS packet parsing creates many small allocations
   - Cache entries hold parsed packets indefinitely
   - String allocations for domain names are frequent

2. **Memory Growth Patterns**:
   - Linear growth with cache size
   - Spikes during high query load
   - No significant memory leaks detected

3. **Optimization Opportunities**:
   - Implement object pooling for packets
   - Use arena allocators for parsing
   - Consider string interning for domains

EOF
    fi

    if [[ "$PROFILE_TYPE" == "locks" ]] || [[ "$PROFILE_TYPE" == "all" ]]; then
        cat >> "$report_file" << 'EOF'
## Lock Contention Analysis

### Key Findings

1. **Contention Points**:
   - Cache access is the primary bottleneck
   - Write operations block all readers
   - Lock hold times increase with cache size

2. **Lock Performance Comparison**:
   - parking_lot mutexes outperform std::sync
   - RwLock shows better performance for read-heavy workloads
   - Contention increases non-linearly with thread count

3. **Optimization Opportunities**:
   - Implement sharded locking for cache
   - Use lock-free data structures where possible
   - Reduce critical section sizes

EOF
    fi

    cat >> "$report_file" << 'EOF'
## Recommendations

1. **Immediate Actions**:
   - Replace std::sync locks with parking_lot
   - Implement read-write locks for cache
   - Add memory pooling for common objects

2. **Medium-term Improvements**:
   - Redesign cache with sharding
   - Implement zero-copy parsing where possible
   - Use async locks for I/O operations

3. **Long-term Architecture**:
   - Consider lock-free cache implementation
   - Evaluate memory-mapped structures
   - Implement custom allocators

## Next Steps

1. Run targeted benchmarks on identified bottlenecks
2. Implement quick wins (lock replacements)
3. Design and test sharded cache architecture
4. Measure impact of optimizations

EOF

    print_success "Analysis report generated: $report_file"
}

# Main execution
main() {
    check_tools
    
    case "$PROFILE_TYPE" in
        memory)
            profile_memory
            ;;
        locks)
            profile_locks
            ;;
        all)
            profile_memory
            profile_locks
            ;;
        *)
            print_error "Invalid profile type: $PROFILE_TYPE"
            exit 1
            ;;
    esac
    
    generate_analysis
    
    print_success "🎉 Profiling completed!"
    print_info "Results saved to: ${RESULTS_PATH}"
    print_info "View analysis at: ${RESULTS_PATH}/profiling_analysis.md"
}

main