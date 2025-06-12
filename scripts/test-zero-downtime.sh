#!/bin/bash
# Test DNS availability during Heimdall deployment/updates
# This helps verify that MetalLB maintains connectivity during pod rotations

set -euo pipefail

# Configuration
DNS_VIP="${DNS_VIP:-10.0.0.53}"  # Set your MetalLB VIP
TEST_DOMAIN="${TEST_DOMAIN:-google.com}"
TEST_INTERVAL="${TEST_INTERVAL:-0.1}"  # 100ms between tests
NAMESPACE="${NAMESPACE:-default}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Statistics
SUCCESS=0
FAILURE=0
CONSECUTIVE_FAILURES=0
MAX_CONSECUTIVE_FAILURES=0
START_TIME=$(date +%s)

# Cleanup function
cleanup() {
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    
    echo -e "\n\n${GREEN}=== Test Summary ===${NC}"
    echo "Duration: ${DURATION} seconds"
    echo "Total queries: $((SUCCESS + FAILURE))"
    echo -e "Successful: ${GREEN}${SUCCESS}${NC}"
    echo -e "Failed: ${RED}${FAILURE}${NC}"
    echo "Success rate: $(awk "BEGIN {printf \"%.2f\", ${SUCCESS}/($SUCCESS+$FAILURE)*100}")%"
    echo "Max consecutive failures: ${MAX_CONSECUTIVE_FAILURES}"
    
    if [ $FAILURE -eq 0 ]; then
        echo -e "\n${GREEN}✓ ZERO DOWNTIME ACHIEVED!${NC}"
        exit 0
    else
        echo -e "\n${RED}✗ Downtime detected during deployment${NC}"
        exit 1
    fi
}

trap cleanup EXIT INT TERM

# Function to test DNS
test_dns() {
    local start_ms=$(date +%s%3N)
    
    if timeout 1 dig @${DNS_VIP} +short +time=1 +tries=1 ${TEST_DOMAIN} > /dev/null 2>&1; then
        local end_ms=$(date +%s%3N)
        local duration=$((end_ms - start_ms))
        
        SUCCESS=$((SUCCESS + 1))
        CONSECUTIVE_FAILURES=0
        
        # Show response time in green if fast, yellow if slow
        if [ $duration -lt 50 ]; then
            echo -ne "\r${GREEN}✓${NC} Success: ${SUCCESS} | Failed: ${FAILURE} | Response: ${GREEN}${duration}ms${NC}    "
        else
            echo -ne "\r${GREEN}✓${NC} Success: ${SUCCESS} | Failed: ${FAILURE} | Response: ${YELLOW}${duration}ms${NC}    "
        fi
    else
        FAILURE=$((FAILURE + 1))
        CONSECUTIVE_FAILURES=$((CONSECUTIVE_FAILURES + 1))
        
        if [ $CONSECUTIVE_FAILURES -gt $MAX_CONSECUTIVE_FAILURES ]; then
            MAX_CONSECUTIVE_FAILURES=$CONSECUTIVE_FAILURES
        fi
        
        echo -e "\n${RED}✗ $(date '+%Y-%m-%d %H:%M:%S.%3N'): DNS query failed! (consecutive: ${CONSECUTIVE_FAILURES})${NC}"
        echo -ne "\r${RED}✗${NC} Success: ${SUCCESS} | Failed: ${FAILURE} | ${RED}FAILURE${NC}              "
    fi
}

# Function to monitor deployment status in background
monitor_deployment() {
    while true; do
        local status=$(kubectl get deployment heimdall -n ${NAMESPACE} -o jsonpath='{.status.conditions[?(@.type=="Progressing")].status}' 2>/dev/null || echo "Unknown")
        local ready=$(kubectl get deployment heimdall -n ${NAMESPACE} -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
        local desired=$(kubectl get deployment heimdall -n ${NAMESPACE} -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")
        
        if [ "$status" = "True" ] && [ "$ready" = "$desired" ]; then
            echo -e "\n${GREEN}Deployment complete: ${ready}/${desired} replicas ready${NC}"
            break
        fi
        
        sleep 5
    done
}

# Main test loop
main() {
    echo -e "${GREEN}=== DNS Zero-Downtime Test ===${NC}"
    echo "Testing DNS at ${DNS_VIP} for domain ${TEST_DOMAIN}"
    echo "Test interval: ${TEST_INTERVAL}s"
    echo "Press Ctrl+C to stop"
    echo ""
    
    # Start deployment monitoring in background if requested
    if [ "${MONITOR_DEPLOYMENT:-false}" = "true" ]; then
        monitor_deployment &
        MONITOR_PID=$!
    fi
    
    # Pre-deployment test
    echo "Running pre-deployment test..."
    for i in {1..10}; do
        test_dns
        sleep $TEST_INTERVAL
    done
    
    if [ $FAILURE -gt 0 ]; then
        echo -e "\n${RED}Warning: DNS already failing before deployment!${NC}"
    fi
    
    echo -e "\n\nStarting continuous monitoring..."
    
    # Continuous testing
    while true; do
        test_dns
        sleep $TEST_INTERVAL
    done
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --vip)
            DNS_VIP="$2"
            shift 2
            ;;
        --domain)
            TEST_DOMAIN="$2"
            shift 2
            ;;
        --interval)
            TEST_INTERVAL="$2"
            shift 2
            ;;
        --namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        --monitor-deployment)
            MONITOR_DEPLOYMENT=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --vip <IP>              MetalLB VIP address (default: 10.0.0.53)"
            echo "  --domain <DOMAIN>       Domain to test (default: google.com)"
            echo "  --interval <SECONDS>    Test interval (default: 0.1)"
            echo "  --namespace <NS>        Kubernetes namespace (default: default)"
            echo "  --monitor-deployment    Monitor deployment status"
            echo "  --help                  Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Run the main test
main