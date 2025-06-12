#!/bin/bash

# Comprehensive DNS compatibility test script for Heimdall
# This script tests various DNS scenarios that real applications use

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
HEIMDALL_IP="${HEIMDALL_IP:-127.0.0.1}"
HEIMDALL_PORT="${HEIMDALL_PORT:-1053}"
UPSTREAM_DNS="${UPSTREAM_DNS:-8.8.8.8}"
VERBOSE="${VERBOSE:-0}"

# Test results
PASSED_TESTS=0
FAILED_TESTS=0

# Log file
LOG_FILE="heimdall_test_$(date +%Y%m%d_%H%M%S).log"

# Function to print colored output
print_test() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

print_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASSED_TESTS++))
}

print_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAILED_TESTS++))
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Function to run dig and capture output
run_dig() {
    local query="$1"
    local options="${2:-}"
    local heimdall_result
    local upstream_result
    
    if [ "$VERBOSE" -eq 1 ]; then
        echo "Running: dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${query} ${options}" >> "$LOG_FILE"
    fi
    
    # Query Heimdall
    heimdall_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${query} ${options} 2>&1 || echo "FAILED")
    echo "=== Heimdall Result for: ${query} ${options} ===" >> "$LOG_FILE"
    echo "$heimdall_result" >> "$LOG_FILE"
    
    # Query upstream for comparison
    upstream_result=$(dig @${UPSTREAM_DNS} ${query} ${options} 2>&1 || echo "FAILED")
    echo "=== Upstream Result for: ${query} ${options} ===" >> "$LOG_FILE"
    echo "$upstream_result" >> "$LOG_FILE"
    
    echo "$heimdall_result"
}

# Function to compare response codes
check_response_code() {
    local domain="$1"
    local expected="${2:-NOERROR}"
    local result
    
    result=$(run_dig "$domain" "+short")
    local status=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +noall +comments | grep -i "status:" | awk -F'[,:]' '{print $2}' | tr -d ' ')
    
    if [ "$status" = "$expected" ]; then
        print_pass "Response code for $domain: $status"
    else
        print_fail "Response code for $domain: expected $expected, got $status"
        echo "Full response:" >> "$LOG_FILE"
        dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" >> "$LOG_FILE" 2>&1
    fi
}

# Function to test if we get any answer
check_has_answer() {
    local query="$1"
    local record_type="${2:-A}"
    local result
    
    result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$query" "$record_type" +short 2>&1)
    
    if [ -n "$result" ] && [ "$result" != "FAILED" ] && ! echo "$result" | grep -q "connection refused"; then
        print_pass "$record_type record for $query: Got answer"
    else
        print_fail "$record_type record for $query: No answer"
    fi
}

# Function to test EDNS support
check_edns() {
    local domain="$1"
    local result
    
    result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +bufsize=4096 +noall +comments 2>&1)
    
    if echo "$result" | grep -q "EDNS:"; then
        print_pass "EDNS support for $domain: Supported"
    else
        print_fail "EDNS support for $domain: Not detected"
    fi
}

# Function to test TCP support
check_tcp() {
    local domain="$1"
    local result
    
    result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +tcp +short 2>&1)
    
    if [ -n "$result" ] && [ "$result" != "FAILED" ] && ! echo "$result" | grep -q "connection refused"; then
        print_pass "TCP query for $domain: Success"
    else
        print_fail "TCP query for $domain: Failed"
    fi
}

# Function to test case sensitivity
check_case_sensitivity() {
    local lower_result upper_result mixed_result
    
    lower_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "google.com" +short 2>&1 | sort)
    upper_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "GOOGLE.COM" +short 2>&1 | sort)
    mixed_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "GoOgLe.CoM" +short 2>&1 | sort)
    
    if [ "$lower_result" = "$upper_result" ] && [ "$lower_result" = "$mixed_result" ]; then
        print_pass "Case insensitivity: Working correctly"
    else
        print_fail "Case insensitivity: Different results for different cases"
    fi
}

# Function to test response time
check_response_time() {
    local domain="$1"
    local max_time="${2:-100}" # milliseconds
    local response_time
    
    response_time=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" | grep "Query time:" | awk '{print $4}')
    
    if [ -n "$response_time" ] && [ "$response_time" -lt "$max_time" ]; then
        print_pass "Response time for $domain: ${response_time}ms (< ${max_time}ms)"
    else
        print_warn "Response time for $domain: ${response_time}ms (threshold: ${max_time}ms)"
    fi
}

# Function to test DNSSEC
check_dnssec() {
    local domain="$1"
    local result
    
    result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +dnssec +short 2>&1)
    
    if echo "$result" | grep -q "RRSIG"; then
        print_pass "DNSSEC for $domain: RRSIG records present"
    else
        print_info "DNSSEC for $domain: No RRSIG records (may not be signed)"
    fi
}

# Function to test specific problematic scenarios
test_edge_cases() {
    print_test "Testing edge cases..."
    
    # Test empty response (NODATA)
    check_response_code "example.com" "NOERROR"
    check_has_answer "example.com" "AAAA"  # Should return empty but NOERROR
    
    # Test NXDOMAIN
    check_response_code "this-domain-definitely-does-not-exist-12345.com" "NXDOMAIN"
    
    # Test CNAME following
    check_has_answer "www.google.com" "A"
    check_has_answer "www.google.com" "CNAME"
    
    # Test root domain query
    local root_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} . NS +short 2>&1)
    if [ -n "$root_result" ] && [ "$root_result" != "FAILED" ]; then
        print_pass "Root domain query: Success"
    else
        print_fail "Root domain query: Failed"
    fi
    
    # Test query with trailing dot
    local with_dot=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "google.com." +short 2>&1 | sort)
    local without_dot=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "google.com" +short 2>&1 | sort)
    if [ "$with_dot" = "$without_dot" ]; then
        print_pass "Trailing dot handling: Consistent"
    else
        print_fail "Trailing dot handling: Inconsistent results"
    fi
}

# Function to test common application queries
test_application_scenarios() {
    print_test "Testing common application scenarios..."
    
    # Web browsers often query both A and AAAA
    print_info "Testing dual A/AAAA queries (common in browsers)..."
    check_has_answer "github.com" "A"
    check_has_answer "github.com" "AAAA"
    
    # Email clients query MX records
    print_info "Testing email-related records..."
    check_has_answer "gmail.com" "MX"
    check_has_answer "gmail.com" "TXT"  # SPF records
    
    # SRV records (used by various services)
    print_info "Testing SRV records..."
    check_has_answer "_sip._tcp.example.com" "SRV"
    
    # PTR records (reverse DNS)
    print_info "Testing reverse DNS..."
    check_has_answer "8.8.8.8.in-addr.arpa" "PTR"
}

# Function to test concurrent queries
test_concurrent_queries() {
    print_test "Testing concurrent queries..."
    
    # Launch multiple queries in background
    for i in {1..10}; do
        dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "test$i.example.com" +short &
    done
    
    # Wait for all background jobs
    wait
    
    print_pass "Concurrent queries: Completed"
}

# Function to test large responses
test_large_responses() {
    print_test "Testing large responses..."
    
    # Query for TXT records which can be large
    local txt_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "google.com" TXT +short 2>&1)
    if [ -n "$txt_result" ] && [ "$txt_result" != "FAILED" ]; then
        print_pass "Large TXT record query: Success"
    else
        print_fail "Large TXT record query: Failed"
    fi
    
    # Test truncation handling
    local large_result=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "_spf.google.com" TXT +ignore 2>&1)
    if echo "$large_result" | grep -q "flags:.*tc"; then
        print_info "Truncation detected, testing TCP fallback..."
        check_tcp "_spf.google.com"
    fi
}

# Function to compare with upstream DNS
test_consistency() {
    print_test "Testing consistency with upstream DNS..."
    
    local domains=(
        "google.com"
        "cloudflare.com"
        "github.com"
        "stackoverflow.com"
        "reddit.com"
    )
    
    for domain in "${domains[@]}"; do
        local heimdall_ips=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" +short | sort)
        local upstream_ips=$(dig @${UPSTREAM_DNS} "$domain" +short | sort)
        
        if [ -z "$heimdall_ips" ]; then
            print_fail "No response from Heimdall for $domain"
        elif [ "$heimdall_ips" = "$upstream_ips" ]; then
            print_pass "Consistent results for $domain"
        else
            print_warn "Different results for $domain (may be due to geo-DNS or load balancing)"
            echo "Heimdall: $heimdall_ips" >> "$LOG_FILE"
            echo "Upstream: $upstream_ips" >> "$LOG_FILE"
        fi
    done
}

# Main test execution
main() {
    echo "=== Heimdall DNS Compatibility Test Suite ==="
    echo "Testing Heimdall at ${HEIMDALL_IP}:${HEIMDALL_PORT}"
    echo "Comparing with upstream DNS: ${UPSTREAM_DNS}"
    echo "Log file: ${LOG_FILE}"
    echo ""
    
    # Check if Heimdall is responding
    if ! dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} google.com +short +time=2 &>/dev/null; then
        print_fail "Heimdall is not responding at ${HEIMDALL_IP}:${HEIMDALL_PORT}"
        exit 1
    fi
    
    print_pass "Heimdall is responding"
    echo ""
    
    # Run all test categories
    print_test "Testing basic functionality..."
    check_has_answer "google.com" "A"
    check_has_answer "ipv6.google.com" "AAAA"
    check_has_answer "google.com" "NS"
    check_has_answer "gmail.com" "MX"
    check_has_answer "google.com" "TXT"
    check_has_answer "www.google.com" "CNAME"
    echo ""
    
    print_test "Testing DNS features..."
    check_edns "google.com"
    check_tcp "google.com"
    check_case_sensitivity
    check_response_time "google.com" 100
    check_dnssec "cloudflare.com"
    echo ""
    
    test_edge_cases
    echo ""
    
    test_application_scenarios
    echo ""
    
    test_large_responses
    echo ""
    
    test_concurrent_queries
    echo ""
    
    test_consistency
    echo ""
    
    # Summary
    echo "=== Test Summary ==="
    echo -e "${GREEN}Passed:${NC} $PASSED_TESTS"
    echo -e "${RED}Failed:${NC} $FAILED_TESTS"
    echo "Detailed log: ${LOG_FILE}"
    
    if [ "$FAILED_TESTS" -gt 0 ]; then
        echo ""
        echo "Some tests failed. Check the log file for details."
        exit 1
    fi
}

# Handle command line arguments
while getopts "h:p:u:v" opt; do
    case $opt in
        h)
            HEIMDALL_IP="$OPTARG"
            ;;
        p)
            HEIMDALL_PORT="$OPTARG"
            ;;
        u)
            UPSTREAM_DNS="$OPTARG"
            ;;
        v)
            VERBOSE=1
            ;;
        *)
            echo "Usage: $0 [-h heimdall_ip] [-p heimdall_port] [-u upstream_dns] [-v]"
            exit 1
            ;;
    esac
done

# Run the tests
main