#!/bin/bash

echo "=== Testing DNSSEC Implementation ==="
echo

# Start Heimdall with DNSSEC enabled
echo "Starting Heimdall with DNSSEC enabled..."
HEIMDALL_DNSSEC_ENABLED=true cargo run > /tmp/heimdall_dnssec.log 2>&1 &
HEIMDALL_PID=$!
sleep 3

echo "Testing DNSSEC-signed domain (cloudflare.com)..."
dig @127.0.0.1 -p 1053 cloudflare.com A +dnssec | grep -E "(RRSIG|flags:)"

echo
echo "Testing non-DNSSEC domain (google.com)..."
dig @127.0.0.1 -p 1053 google.com A +dnssec | grep -E "(RRSIG|flags:)"

echo
echo "Testing with strict mode..."
kill $HEIMDALL_PID
sleep 1

HEIMDALL_DNSSEC_ENABLED=true HEIMDALL_DNSSEC_STRICT=true cargo run > /tmp/heimdall_dnssec_strict.log 2>&1 &
HEIMDALL_PID=$!
sleep 3

echo "Testing DNSSEC-signed domain with strict mode..."
dig @127.0.0.1 -p 1053 cloudflare.com A +dnssec 2>&1 | grep -E "(RRSIG|flags:|status:)"

echo
echo "Cleaning up..."
kill $HEIMDALL_PID

echo
echo "Done! Check logs at /tmp/heimdall_dnssec*.log for details"