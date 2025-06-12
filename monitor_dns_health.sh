#!/bin/bash

# Real-time DNS Health Monitor for Heimdall
# This script continuously monitors DNS queries and alerts on issues

set -euo pipefail

# Configuration
HEIMDALL_IP="${HEIMDALL_IP:-127.0.0.1}"
HEIMDALL_PORT="${HEIMDALL_PORT:-1053}"
CHECK_INTERVAL="${CHECK_INTERVAL:-5}" # seconds
LOG_FILE="heimdall_monitor_$(date +%Y%m%d_%H%M%S).log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test domains
TEST_DOMAINS=(
    "google.com"
    "cloudflare.com"
    "github.com"
    "example.com"
    "dns.google"
)

# Statistics
TOTAL_QUERIES=0
FAILED_QUERIES=0
SLOW_QUERIES=0
START_TIME=$(date +%s)

# Trap to show summary on exit
trap show_summary EXIT

show_summary() {
    echo ""
    echo -e "${BLUE}=== Monitoring Summary ===${NC}"
    local runtime=$(($(date +%s) - START_TIME))
    echo "Runtime: ${runtime} seconds"
    echo "Total queries: $TOTAL_QUERIES"
    echo -e "${RED}Failed queries: $FAILED_QUERIES${NC}"
    echo -e "${YELLOW}Slow queries (>100ms): $SLOW_QUERIES${NC}"
    if [ $TOTAL_QUERIES -gt 0 ]; then
        local success_rate=$(( (TOTAL_QUERIES - FAILED_QUERIES) * 100 / TOTAL_QUERIES ))
        echo "Success rate: ${success_rate}%"
    fi
}

# Function to check a single domain
check_domain() {
    local domain="$1"
    local start_time=$(date +%s.%N)
    local result
    local query_time
    local status="OK"
    
    ((TOTAL_QUERIES++))
    
    # Run the query
    result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +short +tries=1 +time=2 2>&1)
    local exit_code=$?
    
    # Calculate query time
    local end_time=$(date +%s.%N)
    query_time=$(echo "$end_time - $start_time" | bc)
    query_time_ms=$(echo "$query_time * 1000" | bc | cut -d. -f1)
    
    # Check for failures
    if [ $exit_code -ne 0 ] || [ -z "$result" ] || echo "$result" | grep -q "connection refused\|timed out\|SERVFAIL"; then
        ((FAILED_QUERIES++))
        status="${RED}FAILED${NC}"
        echo -e "[$(date '+%H:%M:%S')] ${domain}: ${status} - ${result}" | tee -a "$LOG_FILE"
    elif [ "$query_time_ms" -gt 100 ]; then
        ((SLOW_QUERIES++))
        status="${YELLOW}SLOW${NC}"
        echo -e "[$(date '+%H:%M:%S')] ${domain}: ${status} (${query_time_ms}ms)" | tee -a "$LOG_FILE"
    else
        # Only show successful queries in verbose mode
        if [ "${VERBOSE:-0}" -eq 1 ]; then
            echo -e "[$(date '+%H:%M:%S')] ${domain}: ${GREEN}OK${NC} (${query_time_ms}ms)"
        fi
    fi
}

# Function to run a batch of health checks
run_health_checks() {
    for domain in "${TEST_DOMAINS[@]}"; do
        check_domain "$domain" &
    done
    wait
}

# Function to check specific scenarios that often break
check_edge_cases() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')] Running edge case checks...${NC}"
    
    # Check NXDOMAIN handling
    local nxdomain_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "this-should-not-exist-$(date +%s).com" +short 2>&1)
    local nxdomain_status=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "this-should-not-exist-$(date +%s).com" +noall +comments | grep "status:" | grep -c "NXDOMAIN" || echo 0)
    
    if [ "$nxdomain_status" -eq 0 ]; then
        echo -e "[$(date '+%H:%M:%S')] ${RED}ISSUE: NXDOMAIN not returned for non-existent domain${NC}" | tee -a "$LOG_FILE"
    fi
    
    # Check large response handling
    local txt_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "google.com" TXT +short 2>&1)
    if [ -z "$txt_result" ]; then
        echo -e "[$(date '+%H:%M:%S')] ${RED}ISSUE: No TXT records returned for google.com${NC}" | tee -a "$LOG_FILE"
    fi
    
    # Check CNAME following
    local cname_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "www.google.com" A +short 2>&1)
    if [ -z "$cname_result" ]; then
        echo -e "[$(date '+%H:%M:%S')] ${RED}ISSUE: CNAME resolution failed for www.google.com${NC}" | tee -a "$LOG_FILE"
    fi
}

# Function to monitor metrics endpoint if available
check_metrics() {
    if command -v curl &> /dev/null; then
        local metrics=$(curl -s "http://${HEIMDALL_IP}:8080/metrics" 2>/dev/null)
        if [ -n "$metrics" ]; then
            echo -e "${BLUE}[$(date '+%H:%M:%S')] Metrics snapshot:${NC}"
            echo "$metrics" | grep -E "dns_queries_total|dns_cache_hits|dns_errors_total" | tail -5
        fi
    fi
}

# Main monitoring loop
main() {
    echo -e "${BLUE}=== Heimdall DNS Health Monitor ===${NC}"
    echo "Monitoring: ${HEIMDALL_IP}:${HEIMDALL_PORT}"
    echo "Check interval: ${CHECK_INTERVAL}s"
    echo "Log file: ${LOG_FILE}"
    echo "Press Ctrl+C to stop"
    echo ""
    
    # Check if Heimdall is responding
    if ! dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} google.com +short +time=2 &>/dev/null; then
        echo -e "${RED}ERROR: Heimdall is not responding at ${HEIMDALL_IP}:${HEIMDALL_PORT}${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}Heimdall is responding. Starting monitoring...${NC}"
    echo ""
    
    local iteration=0
    while true; do
        ((iteration++))
        
        # Run standard health checks
        run_health_checks
        
        # Every 10 iterations, run edge case checks
        if [ $((iteration % 10)) -eq 0 ]; then
            check_edge_cases
        fi
        
        # Every 20 iterations, check metrics
        if [ $((iteration % 20)) -eq 0 ]; then
            check_metrics
        fi
        
        # Show periodic summary
        if [ $((iteration % 60)) -eq 0 ]; then
            show_summary
            echo ""
        fi
        
        sleep "$CHECK_INTERVAL"
    done
}

# Handle arguments
while getopts "h:p:i:v" opt; do
    case $opt in
        h)
            HEIMDALL_IP="$OPTARG"
            ;;
        p)
            HEIMDALL_PORT="$OPTARG"
            ;;
        i)
            CHECK_INTERVAL="$OPTARG"
            ;;
        v)
            VERBOSE=1
            ;;
        *)
            echo "Usage: $0 [-h heimdall_ip] [-p heimdall_port] [-i check_interval] [-v]"
            exit 1
            ;;
    esac
done

# Start monitoring
main