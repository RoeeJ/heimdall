#!/bin/bash

# Run load test for Heimdall DNS server
#
# This script runs various load test scenarios against a running Heimdall instance

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
SERVER="127.0.0.1:1053"
OUTPUT_DIR="load_test_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--server)
            SERVER="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  -s, --server HOST:PORT   DNS server to test (default: 127.0.0.1:1053)"
            echo "  -o, --output DIR         Output directory for results (default: load_test_results)"
            echo "  -h, --help              Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo -e "${BLUE}🚀 Heimdall DNS Load Test Suite${NC}"
echo -e "${BLUE}================================${NC}"
echo "Server: $SERVER"
echo "Output: $OUTPUT_DIR"
echo ""

# Check if server is running
echo -e "${YELLOW}Checking if DNS server is reachable...${NC}"
if timeout 2 dig @${SERVER%:*} -p ${SERVER#*:} example.com +short >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Server is responding${NC}"
else
    echo -e "${RED}✗ Server is not responding at $SERVER${NC}"
    echo "Please start Heimdall first with: ./start_server.sh"
    exit 1
fi

# Build the load test tool if needed
echo -e "\n${YELLOW}Building load test tool...${NC}"
cargo build --release --bin heimdall_load_test >/dev/null 2>&1
echo -e "${GREEN}✓ Load test tool ready${NC}"

# Function to run a test scenario
run_test() {
    local test_name="$1"
    local test_type="$2"
    local clients="$3"
    local qps="$4"
    local duration="$5"
    local extra_args="${6:-}"
    
    echo -e "\n${BLUE}Running: $test_name${NC}"
    echo "Test type: $test_type"
    echo "Clients: $clients, QPS/client: $qps, Duration: ${duration}s"
    
    local output_file="$OUTPUT_DIR/${test_name}_${TIMESTAMP}.json"
    
    ./target/release/heimdall_load_test \
        --server "$SERVER" \
        --test-type "$test_type" \
        --clients "$clients" \
        --qps "$qps" \
        --duration "$duration" \
        --output json \
        --warmup 5 \
        $extra_args > "$output_file" 2>&1
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Test completed successfully${NC}"
        # Extract and display key metrics
        local qps_total=$(jq -r '.performance.queries_per_second' "$output_file" 2>/dev/null | xargs printf "%.1f")
        local p99_latency=$(jq -r '.latency_ms.p99' "$output_file" 2>/dev/null | xargs printf "%.2f")
        local loss_rate=$(jq -r '.query_stats.loss_rate_percent' "$output_file" 2>/dev/null | xargs printf "%.2f")
        echo "  Total QPS: $qps_total"
        echo "  P99 Latency: ${p99_latency}ms"
        echo "  Loss Rate: ${loss_rate}%"
    else
        echo -e "${RED}✗ Test failed${NC}"
    fi
}

# Test Suite
echo -e "\n${YELLOW}Starting load test suite...${NC}"

# Test 1: Baseline performance (light load)
run_test "01_baseline" "mixed" 10 10 30

# Test 2: Cache hit performance
run_test "02_cache_hit" "cache-hit" 50 20 30

# Test 3: Cache miss stress
run_test "03_cache_miss" "cache-miss" 20 10 30

# Test 4: High concurrent clients
run_test "04_high_concurrency" "mixed" 200 5 30

# Test 5: High QPS per client
run_test "05_high_qps" "mixed" 10 100 30

# Test 6: NXDOMAIN handling
run_test "06_nxdomain" "nx-domain" 50 10 20

# Test 7: Mixed record types
run_test "07_record_types" "record-types" 25 10 30

# Test 8: Large response handling
run_test "08_large_response" "large-response" 20 5 20

# Test 9: Sustained load test
run_test "09_sustained_load" "mixed" 100 10 60

# Test 10: Stress test
run_test "10_stress_test" "stress" 200 20 30 "--max-loss-percent 5.0"

# Generate summary report
echo -e "\n${YELLOW}Generating summary report...${NC}"
SUMMARY_FILE="$OUTPUT_DIR/summary_${TIMESTAMP}.txt"

echo "Heimdall Load Test Summary" > "$SUMMARY_FILE"
echo "=========================" >> "$SUMMARY_FILE"
echo "Timestamp: $(date)" >> "$SUMMARY_FILE"
echo "Server: $SERVER" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

for json_file in "$OUTPUT_DIR"/*_${TIMESTAMP}.json; do
    if [ -f "$json_file" ]; then
        test_name=$(basename "$json_file" .json | sed "s/_${TIMESTAMP}//")
        echo "Test: $test_name" >> "$SUMMARY_FILE"
        if [ -s "$json_file" ] && jq -e . >/dev/null 2>&1 < "$json_file"; then
            echo "  Queries/sec: $(jq -r '.performance.queries_per_second' "$json_file" | xargs printf "%.1f")" >> "$SUMMARY_FILE"
            echo "  P99 Latency: $(jq -r '.latency_ms.p99' "$json_file" | xargs printf "%.2f")ms" >> "$SUMMARY_FILE"
            echo "  Loss Rate: $(jq -r '.query_stats.loss_rate_percent' "$json_file" | xargs printf "%.2f")%" >> "$SUMMARY_FILE"
            echo "  Status: $(jq -r '.test_passed' "$json_file")" >> "$SUMMARY_FILE"
        else
            echo "  Status: FAILED" >> "$SUMMARY_FILE"
        fi
        echo "" >> "$SUMMARY_FILE"
    fi
done

echo -e "${GREEN}✓ Load test suite completed${NC}"
echo -e "Results saved to: ${BLUE}$OUTPUT_DIR${NC}"
echo -e "Summary: ${BLUE}$SUMMARY_FILE${NC}"

# Display summary
echo -e "\n${YELLOW}Summary:${NC}"
cat "$SUMMARY_FILE"