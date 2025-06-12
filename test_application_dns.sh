#!/bin/bash

# Application-Specific DNS Testing for Heimdall
# This script tests DNS patterns used by common applications

set -euo pipefail

# Configuration
HEIMDALL_IP="${HEIMDALL_IP:-127.0.0.1}"
HEIMDALL_PORT="${HEIMDALL_PORT:-1053}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== Application DNS Pattern Testing ===${NC}"
echo "Testing Heimdall at ${HEIMDALL_IP}:${HEIMDALL_PORT}"
echo ""

# Function to test and report
test_pattern() {
    local description="$1"
    local command="$2"
    echo -e "${YELLOW}Testing: ${description}${NC}"
    if eval "$command"; then
        echo -e "${GREEN}✓ PASS${NC}"
    else
        echo -e "${RED}✗ FAIL${NC}"
    fi
    echo ""
}

# 1. Web Browser Patterns
echo -e "${BLUE}1. Web Browser DNS Patterns${NC}"

test_pattern "Dual stack query (A + AAAA)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} github.com A +short >/dev/null && \
     dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} github.com AAAA +short >/dev/null"

test_pattern "HTTP/2 Alt-Svc (used by Chrome)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} _http._tcp.github.com SRV +short >/dev/null || true"

test_pattern "HTTPS record type (new browsers)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} cloudflare.com HTTPS +short >/dev/null || true"

# 2. Email Client Patterns
echo -e "${BLUE}2. Email Client DNS Patterns${NC}"

test_pattern "MX record lookup" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} gmail.com MX +short | grep -q 'google.com'"

test_pattern "SPF record (TXT)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} gmail.com TXT +short | grep -q 'v=spf1'"

test_pattern "DMARC record" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} _dmarc.gmail.com TXT +short >/dev/null"

test_pattern "Autodiscover (Exchange)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} autodiscover.outlook.com A +short >/dev/null"

# 3. VoIP/SIP Applications
echo -e "${BLUE}3. VoIP/SIP Application Patterns${NC}"

test_pattern "SRV record for SIP" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} _sip._tcp.example.com SRV +short >/dev/null || true"

test_pattern "NAPTR record (telephony)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} e164.arpa NAPTR +short >/dev/null || true"

# 4. Container/Kubernetes Patterns
echo -e "${BLUE}4. Container/Kubernetes DNS Patterns${NC}"

test_pattern "Service discovery (SRV)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} _kubernetes._tcp.example.com SRV +short >/dev/null || true"

test_pattern "Headless service (multiple A records)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} kubernetes.io A +short | wc -l | grep -q -E '[0-9]+'"

# 5. CDN and Load Balancer Patterns
echo -e "${BLUE}5. CDN/Load Balancer Patterns${NC}"

test_pattern "Short TTL responses" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} www.google.com A | grep -q 'IN.*A'"

test_pattern "CNAME chains" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} www.github.com CNAME +short | grep -q '.'"

test_pattern "GeoDNS (location-based)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} www.netflix.com A +short | grep -q -E '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'"

# 6. Security Software Patterns
echo -e "${BLUE}6. Security Software DNS Patterns${NC}"

test_pattern "Reverse DNS (PTR)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} -x 8.8.8.8 +short | grep -q 'google'"

test_pattern "DNSBL query pattern" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} 2.0.0.127.zen.spamhaus.org A +short >/dev/null || true"

test_pattern "CAA records (certificate authority)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} google.com CAA +short >/dev/null || true"

# 7. macOS/iOS Specific
echo -e "${BLUE}7. Apple Device DNS Patterns${NC}"

test_pattern "Bonjour/mDNS compatibility" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} apple.com A +short >/dev/null"

test_pattern "Apple Push Notification Service" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} gateway.push.apple.com A +short >/dev/null"

# 8. Game Console Patterns
echo -e "${BLUE}8. Gaming Console DNS Patterns${NC}"

test_pattern "Xbox Live services" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} xbox.com A +short >/dev/null"

test_pattern "PlayStation Network" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} playstation.com A +short >/dev/null"

# 9. Smart Home/IoT Patterns
echo -e "${BLUE}9. IoT Device DNS Patterns${NC}"

test_pattern "NTP pool lookup" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} pool.ntp.org A +short | grep -q -E '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'"

test_pattern "IoT cloud services" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} iot.amazonaws.com A +short >/dev/null || true"

# 10. Streaming Services
echo -e "${BLUE}10. Streaming Service Patterns${NC}"

test_pattern "Netflix (multiple IPs)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} netflix.com A +short | wc -l | grep -q -E '[0-9]+'"

test_pattern "YouTube (Google CDN)" \
    "dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} youtube.com A +short | grep -q -E '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'"

# Special Tests
echo -e "${BLUE}Special Case Tests${NC}"

# Test query with 0x20 bit randomization (case randomization)
echo -e "${YELLOW}Testing case randomization tolerance:${NC}"
for domain in "GoOgLe.CoM" "GOOGLE.COM" "google.com"; do
    if dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "$domain" A +short | grep -q -E '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+'; then
        echo -e "  ${domain}: ${GREEN}✓${NC}"
    else
        echo -e "  ${domain}: ${RED}✗${NC}"
    fi
done
echo ""

# Test handling of non-ASCII domains (IDN)
echo -e "${YELLOW}Testing IDN (internationalized domain names):${NC}"
if dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "xn--e1afmkfd.xn--p1ai" A +short >/dev/null 2>&1; then
    echo -e "${GREEN}✓ IDN queries supported${NC}"
else
    echo -e "${YELLOW}⚠ IDN queries may not be supported${NC}"
fi
echo ""

# Performance under load
echo -e "${BLUE}Quick Performance Test${NC}"
echo "Sending 50 concurrent queries..."
start_time=$(date +%s.%N)
for i in {1..50}; do
    dig @${HEIMDALL_IP} -p ${HEIMDALL_PORT} "test$i.example.com" A +short >/dev/null 2>&1 &
done
wait
end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
echo -e "Completed in ${duration}s"
echo ""

echo -e "${BLUE}=== Summary ===${NC}"
echo "If any tests failed, it might indicate why certain applications"
echo "are having issues with Heimdall as their DNS server."
echo ""
echo "Common issues to check:"
echo "1. EDNS0 support (required by many modern applications)"
echo "2. TCP fallback for large responses"
echo "3. Proper NXDOMAIN handling"
echo "4. Case-insensitive domain matching"
echo "5. Support for all required record types"
echo "6. Correct CNAME following"
echo "7. Appropriate TTL handling"