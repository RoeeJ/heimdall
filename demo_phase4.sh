#!/bin/bash

echo "=== Heimdall Phase 4 Demo ==="
echo

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run a command and show it
run_cmd() {
    echo -e "${BLUE}$ $1${NC}"
    eval $1
    echo
}

echo -e "${YELLOW}Starting Heimdall with authoritative DNS...${NC}"
echo "In another terminal, run:"
echo -e "${GREEN}export HEIMDALL_ZONE_FILES=zones/example.com.zone"
echo "export HEIMDALL_AUTHORITATIVE_ENABLED=true"
echo "cargo run${NC}"
echo
echo "Press Enter when Heimdall is running..."
read

echo -e "${YELLOW}1. Testing Authoritative Responses${NC}"
run_cmd "dig @127.0.0.1 -p 1053 www.example.com A +short"
run_cmd "dig @127.0.0.1 -p 1053 example.com MX +short"
run_cmd "dig @127.0.0.1 -p 1053 example.com SOA +short"

echo -e "${YELLOW}2. Testing NXDOMAIN Response${NC}"
run_cmd "dig @127.0.0.1 -p 1053 notfound.example.com A"

echo -e "${YELLOW}3. Testing Zone Transfer (AXFR)${NC}"
run_cmd "dig @127.0.0.1 -p 1053 example.com AXFR"

echo -e "${YELLOW}4. Checking Authoritative Flag${NC}"
run_cmd "dig @127.0.0.1 -p 1053 example.com ANY +noall +comments | grep flags"

echo -e "${YELLOW}5. Testing Delegation (if subdomain configured)${NC}"
echo "You can test delegation by adding NS records for subdomains in your zone file"

echo -e "${GREEN}Demo complete!${NC}"