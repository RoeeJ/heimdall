#!/bin/bash

# Performance profiling script for Heimdall DNS server
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Check for required tools
check_tools() {
    local missing_tools=()
    
    command -v perf >/dev/null 2>&1 || missing_tools+=("perf")
    command -v flamegraph >/dev/null 2>&1 || missing_tools+=("flamegraph")
    
    if [ ${#missing_tools[@]} -ne 0 ]; then
        echo -e "${YELLOW}Missing required tools: ${missing_tools[*]}${NC}"
        echo "Install with:"
        echo "  - perf: sudo apt-get install linux-tools-common linux-tools-generic"
        echo "  - flamegraph: cargo install flamegraph"
        exit 1
    fi
}

# Profile CPU usage
profile_cpu() {
    echo -e "${BLUE}=== CPU Profiling ===${NC}"
    echo "Starting Heimdall with profiling..."
    
    # Build with debug symbols
    cargo build --release
    
    # Start server in background
    RUST_LOG=warn ./target/release/heimdall &
    SERVER_PID=$!
    
    echo "Waiting for server to start..."
    sleep 5
    
    # Run load test while profiling
    echo "Starting CPU profiling..."
    sudo perf record -F 99 -p $SERVER_PID -g -- sleep 30 &
    PERF_PID=$!
    
    # Run load test
    echo "Running load test..."
    ./target/release/heimdall_load_test \
        --test-type cache-hit \
        --clients 200 \
        --qps 50 \
        --duration 25 \
        --output human
    
    # Wait for perf to finish
    wait $PERF_PID
    
    # Generate flame graph
    echo "Generating flame graph..."
    sudo perf script | stackcollapse-perf.pl | flamegraph.pl > heimdall_cpu_flame.svg
    
    # Kill server
    kill $SERVER_PID
    
    echo -e "${GREEN}CPU flame graph saved to: heimdall_cpu_flame.svg${NC}"
}

# Profile memory usage
profile_memory() {
    echo -e "${BLUE}=== Memory Profiling ===${NC}"
    
    # Use heaptrack if available
    if command -v heaptrack >/dev/null 2>&1; then
        echo "Starting Heimdall with heaptrack..."
        heaptrack ./target/release/heimdall &
        TRACK_PID=$!
        
        sleep 5
        
        # Run load test
        echo "Running load test..."
        ./target/release/heimdall_load_test \
            --test-type mixed \
            --clients 100 \
            --qps 50 \
            --duration 30 \
            --output human
        
        # Stop server
        sleep 2
        pkill -SIGTERM heimdall
        
        wait $TRACK_PID
        
        echo -e "${GREEN}Heaptrack data saved. Analyze with: heaptrack_gui heimdall.*.gz${NC}"
    else
        echo -e "${YELLOW}heaptrack not found. Using /proc monitoring${NC}"
        
        # Start server
        ./target/release/heimdall &
        SERVER_PID=$!
        
        sleep 5
        
        # Monitor memory usage
        echo "Time,RSS(MB),VMS(MB)" > memory_usage.csv
        
        for i in {1..30}; do
            if [ -d "/proc/$SERVER_PID" ]; then
                RSS=$(ps -p $SERVER_PID -o rss= | awk '{print $1/1024}')
                VMS=$(ps -p $SERVER_PID -o vsz= | awk '{print $1/1024}')
                echo "$i,$RSS,$VMS" >> memory_usage.csv
                echo "Memory usage - RSS: ${RSS}MB, VMS: ${VMS}MB"
            fi
            sleep 1
        done
        
        kill $SERVER_PID
        
        echo -e "${GREEN}Memory usage saved to: memory_usage.csv${NC}"
    fi
}

# Profile cache performance
profile_cache() {
    echo -e "${BLUE}=== Cache Performance Analysis ===${NC}"
    
    # Start server
    RUST_LOG=heimdall::cache=debug ./target/release/heimdall &
    SERVER_PID=$!
    
    sleep 5
    
    # Clear cache statistics
    curl -s http://127.0.0.1:8080/cache/stats > cache_stats_before.json
    
    # Run cache-focused tests
    echo "Running cache hit test..."
    ./target/release/heimdall_load_test \
        --test-type cache-hit \
        --clients 50 \
        --qps 100 \
        --duration 20 \
        --output json > cache_hit_results.json
    
    echo "Running cache miss test..."
    ./target/release/heimdall_load_test \
        --test-type cache-miss \
        --clients 50 \
        --qps 20 \
        --duration 20 \
        --output json > cache_miss_results.json
    
    # Get cache statistics
    curl -s http://127.0.0.1:8080/cache/stats > cache_stats_after.json
    
    kill $SERVER_PID
    
    # Analyze cache performance
    echo -e "\n${YELLOW}Cache Performance Summary:${NC}"
    echo "Cache hit rate: $(jq -r '.hit_rate' cache_stats_after.json)%"
    echo "Total lookups: $(jq -r '.total_lookups' cache_stats_after.json)"
    echo "Evictions: $(jq -r '.evictions' cache_stats_after.json)"
}

# Profile with different optimization flags
profile_optimizations() {
    echo -e "${BLUE}=== Testing Optimization Impact ===${NC}"
    
    # Baseline (no optimizations)
    echo "Testing baseline performance..."
    HEIMDALL_DISABLE_OPTIMIZATIONS=true cargo build --release
    
    ./target/release/heimdall &
    SERVER_PID=$!
    sleep 5
    
    ./target/release/heimdall_load_test \
        --test-type cache-hit \
        --clients 100 \
        --qps 50 \
        --duration 10 \
        --output json > baseline_results.json
    
    kill $SERVER_PID
    
    # With optimizations
    echo "Testing with optimizations..."
    cargo build --release --features "simd zero-copy"
    
    ./target/release/heimdall &
    SERVER_PID=$!
    sleep 5
    
    ./target/release/heimdall_load_test \
        --test-type cache-hit \
        --clients 100 \
        --qps 50 \
        --duration 10 \
        --output json > optimized_results.json
    
    kill $SERVER_PID
    
    # Compare results
    echo -e "\n${YELLOW}Performance Comparison:${NC}"
    BASELINE_QPS=$(jq -r '.performance.queries_per_second' baseline_results.json)
    OPTIMIZED_QPS=$(jq -r '.performance.queries_per_second' optimized_results.json)
    
    echo "Baseline QPS: $BASELINE_QPS"
    echo "Optimized QPS: $OPTIMIZED_QPS"
    
    IMPROVEMENT=$(echo "scale=2; (($OPTIMIZED_QPS - $BASELINE_QPS) / $BASELINE_QPS) * 100" | bc)
    echo -e "${GREEN}Improvement: ${IMPROVEMENT}%${NC}"
}

# Main menu
show_menu() {
    echo -e "${YELLOW}=== Heimdall Performance Profiling ===${NC}"
    echo "1. CPU Profiling (flame graph)"
    echo "2. Memory Profiling"
    echo "3. Cache Performance Analysis"
    echo "4. Test Optimization Impact"
    echo "5. Run All Profiles"
    echo "6. Exit"
    echo
    read -p "Select option: " choice
    
    case $choice in
        1) profile_cpu ;;
        2) profile_memory ;;
        3) profile_cache ;;
        4) profile_optimizations ;;
        5) 
            profile_cpu
            profile_memory
            profile_cache
            profile_optimizations
            ;;
        6) exit 0 ;;
        *) echo "Invalid option"; show_menu ;;
    esac
}

# Check for Linux (perf requirement)
if [[ "$OSTYPE" != "linux-gnu"* ]]; then
    echo -e "${YELLOW}Warning: This script is optimized for Linux.${NC}"
    echo "Some profiling features (perf) may not be available on macOS."
    echo "Consider using Instruments.app or dtrace on macOS."
fi

# Main
check_tools
show_menu