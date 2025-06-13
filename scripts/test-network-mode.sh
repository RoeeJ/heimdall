#!/bin/bash

# Run network-dependent tests that are skipped in CI
# WARNING: This requires internet connectivity and may download files

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Running network-dependent tests...${NC}"
echo -e "${RED}WARNING: This will make network requests and may download files${NC}"
echo ""

# Enable all network features
export HEIMDALL_BLOCKING_ENABLED=true
export HEIMDALL_BLOCKING_DOWNLOAD_PSL=true
export HEIMDALL_BLOCKLIST_AUTO_UPDATE=true
export RUST_BACKTRACE=1

# Function to run tests with timing
run_test_suite() {
    local name=$1
    local cmd=$2
    
    echo -e "\n${BLUE}Running $name...${NC}"
    
    start_time=$(date +%s)
    
    if eval "$cmd"; then
        end_time=$(date +%s)
        duration=$((end_time - start_time))
        echo -e "${GREEN}✓ $name passed (${duration}s)${NC}"
        return 0
    else
        echo -e "${RED}✗ $name failed${NC}"
        return 1
    fi
}

# Check network connectivity first
echo -e "${YELLOW}Checking network connectivity...${NC}"
if ! ping -c 1 8.8.8.8 >/dev/null 2>&1; then
    echo -e "${RED}Error: No network connectivity detected${NC}"
    echo "Network tests require internet access"
    exit 1
fi
echo -e "${GREEN}✓ Network connectivity confirmed${NC}"

# Track results
all_passed=true

# 1. Run ignored tests only
if ! run_test_suite "Network integration tests" "cargo test -- --ignored"; then
    all_passed=false
fi

# 2. Run specific network-dependent test suites
echo -e "\n${YELLOW}Running specific network test suites...${NC}"

# Failover tests
if ! run_test_suite "Failover tests" "cargo test --test failover_tests -- --ignored"; then
    all_passed=false
fi

# Advanced failover tests
if ! run_test_suite "Advanced failover tests" "cargo test --test failover_advanced_test -- --ignored"; then
    all_passed=false
fi

# Performance tests with network
if ! run_test_suite "Performance network tests" "cargo test --test performance_features test_connection_pooling_stats -- --ignored"; then
    all_passed=false
fi

# 3. Optional: Run stress test with real DNS
read -p "Run stress test against real DNS servers? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "\n${YELLOW}Building stress test binary...${NC}"
    cargo build --release --bin stress_test
    
    echo -e "${YELLOW}Running light stress test against 8.8.8.8...${NC}"
    if timeout 30s ./target/release/stress_test --scenario light --server 8.8.8.8:53; then
        echo -e "${GREEN}✓ Stress test completed${NC}"
    else
        echo -e "${YELLOW}⚠ Stress test timed out or failed (this is okay)${NC}"
    fi
fi

# Summary
echo -e "\n${YELLOW}=== Network Test Summary ===${NC}"
if [ "$all_passed" = true ]; then
    echo -e "${GREEN}✅ All network tests passed!${NC}"
else
    echo -e "${RED}❌ Some network tests failed${NC}"
    echo -e "${YELLOW}Note: Network test failures may be due to:${NC}"
    echo "  - Firewall restrictions"
    echo "  - DNS server rate limiting"
    echo "  - Network latency"
    echo "  - Temporary network issues"
fi

# Cleanup note
echo -e "\n${YELLOW}Note: Downloaded files may have been created in:${NC}"
echo "  - blocklists/ directory"
echo "  - /tmp/ directory"
echo "You may want to clean these up if not needed."