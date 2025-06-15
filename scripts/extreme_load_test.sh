#!/bin/bash

# Extreme load test to find Heimdall's performance ceiling
set -e

SERVER="127.0.0.1:1053"
OUTPUT_DIR="extreme_load_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

mkdir -p "$OUTPUT_DIR"

echo -e "${RED}🔥 EXTREME LOAD TEST - Finding Performance Ceiling 🔥${NC}"
echo -e "${YELLOW}Warning: This will push the server to its limits!${NC}"
echo ""

# Function to run test and check if server is still responsive
run_extreme_test() {
    local test_name="$1"
    local clients="$2"
    local qps="$3"
    local duration="$4"
    
    echo -e "\n${BLUE}Test: $test_name${NC}"
    echo "Clients: $clients, QPS/client: $qps"
    echo "Expected total QPS: $((clients * qps))"
    
    local output_file="$OUTPUT_DIR/${test_name}_${TIMESTAMP}.json"
    
    # Run the test and capture output
    local temp_file="${output_file}.tmp"
    timeout 120 ./target/release/heimdall_load_test \
        --server "$SERVER" \
        --test-type cache-hit \
        --clients "$clients" \
        --qps "$qps" \
        --duration "$duration" \
        --output json \
        --warmup 2 \
        --timeout-ms 10000 \
        --max-loss-percent 5.0 > "$temp_file" 2>&1 || true
    
    # Extract just the JSON from the output
    if [ -f "$temp_file" ]; then
        tail -n +2 "$temp_file" | awk '/^{/{flag=1} flag; /^}/{print; flag=0}' > "$output_file"
        rm -f "$temp_file"
    fi
    
    # Check if server is still responsive
    if timeout 2 dig @127.0.0.1 -p 1053 google.com +short >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Server still responsive${NC}"
        
        # Extract metrics if test completed
        if [ -f "$output_file" ] && grep -q "test_passed" "$output_file" 2>/dev/null; then
            local actual_qps=$(jq -r '.performance.queries_per_second' "$output_file" 2>/dev/null | xargs printf "%.0f" 2>/dev/null || echo "0")
            local p99_latency=$(jq -r '.latency_ms.p99' "$output_file" 2>/dev/null | xargs printf "%.1f" 2>/dev/null || echo "N/A")
            local loss_rate=$(jq -r '.query_stats.loss_rate_percent' "$output_file" 2>/dev/null | xargs printf "%.2f" 2>/dev/null || echo "N/A")
            
            echo "  Actual QPS: $actual_qps"
            echo "  P99 Latency: ${p99_latency}ms"
            echo "  Packet Loss: ${loss_rate}%"
            
            # Check if we're hitting limits
            if [ "$actual_qps" != "0" ]; then
                local expected_qps=$((clients * qps))
                local qps_ratio=$(echo "scale=2; $actual_qps * 100 / $expected_qps" | bc)
                if (( $(echo "$qps_ratio < 80" | bc -l) )); then
                    echo -e "${YELLOW}  ⚠️  Performance degradation detected (achieving ${qps_ratio}% of target QPS)${NC}"
                    return 1
                fi
            fi
        else
            echo -e "${RED}  ✗ Test failed or timed out${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ Server is NOT responsive - may have crashed${NC}"
        return 2
    fi
    
    return 0
}

# Start with moderate load and increase
echo -e "${YELLOW}Phase 1: Baseline Performance${NC}"
run_extreme_test "01_baseline" 100 50 10

echo -e "\n${YELLOW}Phase 2: High Concurrency${NC}"
run_extreme_test "02_high_concurrency_200" 200 50 10
run_extreme_test "03_high_concurrency_500" 500 20 10
run_extreme_test "04_high_concurrency_1000" 1000 10 10

echo -e "\n${YELLOW}Phase 3: High QPS per Client${NC}"
run_extreme_test "05_high_qps_100" 50 100 10
run_extreme_test "06_high_qps_200" 50 200 10
run_extreme_test "07_high_qps_500" 20 500 10

echo -e "\n${YELLOW}Phase 4: Extreme Combined Load${NC}"
run_extreme_test "08_extreme_5k" 100 50 10
run_extreme_test "09_extreme_10k" 200 50 10
run_extreme_test "10_extreme_20k" 200 100 10

echo -e "\n${YELLOW}Phase 5: Breaking Point${NC}"
run_extreme_test "11_breaking_30k" 300 100 10
run_extreme_test "12_breaking_50k" 500 100 10
run_extreme_test "13_breaking_100k" 1000 100 10

# Generate summary
echo -e "\n${YELLOW}Generating Performance Ceiling Report...${NC}"
SUMMARY_FILE="$OUTPUT_DIR/performance_ceiling_${TIMESTAMP}.txt"

echo "Heimdall DNS Performance Ceiling Test" > "$SUMMARY_FILE"
echo "====================================" >> "$SUMMARY_FILE"
echo "Timestamp: $(date)" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

# Find the highest successful QPS
max_qps=0
best_test=""

for json_file in "$OUTPUT_DIR"/*_${TIMESTAMP}.json; do
    if [ -f "$json_file" ] && grep -q "queries_per_second" "$json_file" 2>/dev/null; then
        qps=$(jq -r '.performance.queries_per_second' "$json_file" 2>/dev/null | xargs printf "%.0f" 2>/dev/null || echo "0")
        if [ "$qps" -gt "$max_qps" ]; then
            max_qps=$qps
            best_test=$(basename "$json_file" .json | sed "s/_${TIMESTAMP}//")
        fi
    fi
done

echo "Maximum Sustained QPS: $max_qps" >> "$SUMMARY_FILE"
echo "Best Test: $best_test" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

# Detailed results
echo "Detailed Results:" >> "$SUMMARY_FILE"
for json_file in "$OUTPUT_DIR"/*_${TIMESTAMP}.json; do
    if [ -f "$json_file" ]; then
        test_name=$(basename "$json_file" .json | sed "s/_${TIMESTAMP}//")
        echo "" >> "$SUMMARY_FILE"
        echo "Test: $test_name" >> "$SUMMARY_FILE"
        
        if grep -q "test_config" "$json_file" 2>/dev/null; then
            echo "  Clients: $(jq -r '.test_config.clients' "$json_file" 2>/dev/null || echo "N/A")" >> "$SUMMARY_FILE"
            echo "  QPS/client: $(jq -r '.test_config.qps_per_client' "$json_file" 2>/dev/null || echo "N/A")" >> "$SUMMARY_FILE"
            echo "  Actual QPS: $(jq -r '.performance.queries_per_second' "$json_file" 2>/dev/null | xargs printf "%.0f" 2>/dev/null || echo "N/A")" >> "$SUMMARY_FILE"
            echo "  P50 Latency: $(jq -r '.latency_ms.p50' "$json_file" 2>/dev/null | xargs printf "%.1f" 2>/dev/null || echo "N/A")ms" >> "$SUMMARY_FILE"
            echo "  P99 Latency: $(jq -r '.latency_ms.p99' "$json_file" 2>/dev/null | xargs printf "%.1f" 2>/dev/null || echo "N/A")ms" >> "$SUMMARY_FILE"
            echo "  P99.9 Latency: $(jq -r '.latency_ms.p99_9' "$json_file" 2>/dev/null | xargs printf "%.1f" 2>/dev/null || echo "N/A")ms" >> "$SUMMARY_FILE"
            echo "  Packet Loss: $(jq -r '.query_stats.loss_rate_percent' "$json_file" 2>/dev/null | xargs printf "%.2f" 2>/dev/null || echo "N/A")%" >> "$SUMMARY_FILE"
        else
            echo "  Status: FAILED" >> "$SUMMARY_FILE"
        fi
    fi
done

echo -e "\n${GREEN}✓ Extreme load test completed${NC}"
echo -e "Results saved to: ${BLUE}$OUTPUT_DIR${NC}"
echo -e "Summary: ${BLUE}$SUMMARY_FILE${NC}"

# Display summary
echo -e "\n${YELLOW}=== PERFORMANCE CEILING ===${NC}"
cat "$SUMMARY_FILE" | grep -E "(Maximum Sustained QPS|Best Test)" | while read line; do
    echo -e "${GREEN}$line${NC}"
done