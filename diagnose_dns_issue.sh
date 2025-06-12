#!/bin/bash

# DNS Issue Diagnostic Script for Heimdall
# This script helps diagnose specific DNS resolution issues

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
HEIMDALL_IP="${HEIMDALL_IP:-127.0.0.1}"
HEIMDALL_PORT="${HEIMDALL_PORT:-1053}"
UPSTREAM_DNS="${UPSTREAM_DNS:-8.8.8.8}"
PROBLEM_DOMAIN="${1:-}"

if [ -z "$PROBLEM_DOMAIN" ]; then
    echo "Usage: $0 <domain> [record_type]"
    echo "Example: $0 github.com A"
    echo ""
    echo "This script will diagnose DNS resolution issues for the specified domain"
    exit 1
fi

RECORD_TYPE="${2:-A}"

echo -e "${BLUE}=== DNS Issue Diagnostics for $PROBLEM_DOMAIN ===${NC}"
echo "Heimdall: ${HEIMDALL_IP}:${HEIMDALL_PORT}"
echo "Upstream: ${UPSTREAM_DNS}"
echo "Record Type: ${RECORD_TYPE}"
echo ""

# Function to run and display dig output
run_diagnostic_dig() {
    local server="$1"
    local port="${2:-53}"
    local extra_opts="${3:-}"
    
    echo -e "${YELLOW}Query: dig @${server} -p ${port} ${PROBLEM_DOMAIN} ${RECORD_TYPE} ${extra_opts}${NC}"
    dig @${server} -p ${port} ${PROBLEM_DOMAIN} ${RECORD_TYPE} ${extra_opts}
    echo ""
}

# 1. Basic query comparison
echo -e "${BLUE}1. Basic Query Comparison${NC}"
echo -e "${GREEN}Heimdall Response:${NC}"
run_diagnostic_dig "$HEIMDALL_IP" "$HEIMDALL_PORT" "+noall +answer +authority +additional +comments"

echo -e "${GREEN}Upstream Response:${NC}"
run_diagnostic_dig "$UPSTREAM_DNS" "53" "+noall +answer +authority +additional +comments"

# 2. Check response flags
echo -e "${BLUE}2. Response Flags Analysis${NC}"
echo -e "${GREEN}Heimdall Flags:${NC}"
heimdall_flags=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +noall +comments | grep "flags:")
echo "$heimdall_flags"

echo -e "${GREEN}Upstream Flags:${NC}"
upstream_flags=$(dig @${UPSTREAM_DNS} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +noall +comments | grep "flags:")
echo "$upstream_flags"
echo ""

# 3. Check EDNS support
echo -e "${BLUE}3. EDNS Support Check${NC}"
echo -e "${GREEN}Heimdall EDNS:${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +edns=0 +noall +comments | grep -E "(EDNS:|OPT PSEUDOSECTION)" -A 3

echo -e "${GREEN}Upstream EDNS:${NC}"
dig @${UPSTREAM_DNS} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +edns=0 +noall +comments | grep -E "(EDNS:|OPT PSEUDOSECTION)" -A 3
echo ""

# 4. Test with different query flags
echo -e "${BLUE}4. Query Flag Variations${NC}"

echo -e "${YELLOW}Without recursion (RD=0):${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +norecurse +short

echo -e "${YELLOW}With AD flag (DNSSEC):${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +adflag +short

echo -e "${YELLOW}With CD flag (checking disabled):${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +cdflag +short
echo ""

# 5. Test TCP
echo -e "${BLUE}5. TCP Query Test${NC}"
echo -e "${GREEN}Heimdall TCP:${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +tcp +short

echo -e "${GREEN}Upstream TCP:${NC}"
dig @${UPSTREAM_DNS} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +tcp +short
echo ""

# 6. Check for truncation
echo -e "${BLUE}6. Truncation Check${NC}"
tc_check=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +ignore | grep "flags:" | grep "tc")
if [ -n "$tc_check" ]; then
    echo -e "${YELLOW}Truncation detected! Response may be incomplete over UDP${NC}"
else
    echo -e "${GREEN}No truncation detected${NC}"
fi
echo ""

# 7. Test related record types
echo -e "${BLUE}7. Related Record Types${NC}"
if [ "$RECORD_TYPE" = "A" ]; then
    echo -e "${YELLOW}Checking AAAA records:${NC}"
    dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} AAAA +short
    
    echo -e "${YELLOW}Checking CNAME records:${NC}"
    dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} CNAME +short
fi
echo ""

# 8. Raw packet analysis
echo -e "${BLUE}8. Raw Response Analysis${NC}"
echo -e "${YELLOW}Response size:${NC}"
heimdall_size=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} | grep "MSG SIZE" | awk '{print $NF}')
upstream_size=$(dig @${UPSTREAM_DNS} ${PROBLEM_DOMAIN} ${RECORD_TYPE} | grep "MSG SIZE" | awk '{print $NF}')
echo "Heimdall: ${heimdall_size} bytes"
echo "Upstream: ${upstream_size} bytes"
echo ""

# 9. Timing analysis
echo -e "${BLUE}9. Query Timing Analysis${NC}"
echo -e "${YELLOW}Running 5 queries to check consistency:${NC}"
for i in {1..5}; do
    time=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} | grep "Query time:" | awk '{print $4}')
    echo "Query $i: ${time}ms"
    sleep 0.1
done
echo ""

# 10. Check cache behavior
echo -e "${BLUE}10. Cache Behavior Check${NC}"
echo -e "${YELLOW}First query (potentially cache miss):${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN}.cache-test ${RECORD_TYPE} +stats | grep "Query time:"

echo -e "${YELLOW}Second query (should be cached):${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN}.cache-test ${RECORD_TYPE} +stats | grep "Query time:"
echo ""

# 11. DNSSEC validation
echo -e "${BLUE}11. DNSSEC Validation Check${NC}"
echo -e "${GREEN}Heimdall DNSSEC query:${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +dnssec +short

echo -e "${GREEN}Check for AD flag (authenticated data):${NC}"
ad_flag=$(dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} ${PROBLEM_DOMAIN} ${RECORD_TYPE} +dnssec +noall +comments | grep "flags:" | grep " ad")
if [ -n "$ad_flag" ]; then
    echo -e "${GREEN}AD flag present - DNSSEC validated${NC}"
else
    echo -e "${YELLOW}AD flag not present - may not be DNSSEC validated${NC}"
fi
echo ""

# 12. Special character handling
echo -e "${BLUE}12. Special Cases${NC}"
echo -e "${YELLOW}Testing with trailing dot:${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "${PROBLEM_DOMAIN}." ${RECORD_TYPE} +short

echo -e "${YELLOW}Testing case variations:${NC}"
dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$(echo $PROBLEM_DOMAIN | tr '[:lower:]' '[:upper:]')" ${RECORD_TYPE} +short
echo ""

# Summary
echo -e "${BLUE}=== Diagnostic Summary ===${NC}"
echo "1. Check if response codes match between Heimdall and upstream"
echo "2. Verify EDNS is working correctly"
echo "3. Ensure TCP fallback works for large responses"
echo "4. Check if DNSSEC validation is causing issues"
echo "5. Compare response sizes and timing"
echo ""
echo "If you're still experiencing issues, please check:"
echo "- Heimdall logs for any error messages"
echo "- Network connectivity between Heimdall and upstream servers"
echo "- Firewall rules that might block DNS traffic"
echo "- Whether the issue is specific to certain record types or domains"