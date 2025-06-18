#!/bin/bash

echo "=== Heimdall Phase 4 Testing Guide ==="
echo
echo "1. START HEIMDALL WITH AUTHORITATIVE DNS ENABLED:"
echo "   export HEIMDALL_ZONE_FILES=zones/example.com.zone"
echo "   export HEIMDALL_AUTHORITATIVE_ENABLED=true"
echo "   cargo run"
echo
echo "2. TEST AUTHORITATIVE RESPONSES:"
echo "   # Query for A record (should return authoritative answer)"
echo "   dig @127.0.0.1 -p 1053 www.example.com A"
echo
echo "   # Query for MX records"
echo "   dig @127.0.0.1 -p 1053 example.com MX"
echo
echo "   # Query for non-existent domain (should return NXDOMAIN with SOA)"
echo "   dig @127.0.0.1 -p 1053 nonexistent.example.com A"
echo
echo "   # Query for TXT records"
echo "   dig @127.0.0.1 -p 1053 example.com TXT"
echo
echo "3. TEST ZONE TRANSFERS (AXFR):"
echo "   # Perform full zone transfer"
echo "   dig @127.0.0.1 -p 1053 example.com AXFR"
echo
echo "   # Test from unauthorized IP (if restrictions configured)"
echo "   # Configure with: export HEIMDALL_ALLOWED_TRANSFERS='127.0.0.1,192.168.1.0'"
echo
echo "4. TEST DNS NOTIFY:"
echo "   # Heimdall will send NOTIFY to configured secondary servers"
echo "   # Configure with: export HEIMDALL_SECONDARY_SERVERS='192.168.1.2:53,192.168.1.3:53'"
echo "   # Then update zone serial and reload"
echo
echo "5. TEST INCREMENTAL ZONE TRANSFER (IXFR):"
echo "   # Currently falls back to AXFR"
echo "   dig @127.0.0.1 -p 1053 example.com IXFR=2024010100"
echo
echo "6. VERIFY AUTHORITATIVE FLAG:"
echo "   # Look for 'aa' flag in dig output"
echo "   dig @127.0.0.1 -p 1053 example.com SOA +noall +comments"
echo
echo "7. TEST MULTIPLE ZONES:"
echo "   # Create another zone file and add to HEIMDALL_ZONE_FILES"
echo "   export HEIMDALL_ZONE_FILES='zones/example.com.zone,zones/test.local.zone'"
echo
echo "=== Configuration Options ==="
echo "HEIMDALL_ZONE_FILES              - Comma-separated list of zone files"
echo "HEIMDALL_AUTHORITATIVE_ENABLED   - Enable authoritative DNS (true/false)"
echo "HEIMDALL_ALLOWED_TRANSFERS       - IPs allowed to do zone transfers (empty=all)"
echo "HEIMDALL_ALLOWED_NOTIFIERS       - IPs allowed to send NOTIFY (empty=all)"
echo "HEIMDALL_SECONDARY_SERVERS       - Secondary servers to send NOTIFY to"
echo "HEIMDALL_DYNAMIC_UPDATES_ENABLED - Enable dynamic DNS updates (RFC 2136)"