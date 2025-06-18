#!/bin/bash
# Demo script for testing zone transfers with Heimdall DNS server

echo "Zone Transfer Demo for Heimdall DNS Server"
echo "=========================================="
echo ""

# Create a test zone file
ZONE_FILE="/tmp/example.com.zone"
cat > "$ZONE_FILE" << 'EOF'
$ORIGIN example.com.
$TTL 3600

; SOA record
@       IN      SOA     ns1.example.com. admin.example.com. (
                        2024010101 ; serial
                        3600       ; refresh (1 hour)
                        900        ; retry (15 minutes)
                        604800     ; expire (1 week)
                        86400      ; minimum (1 day)
                        )

; Name servers
@       IN      NS      ns1.example.com.
@       IN      NS      ns2.example.com.

; A records
@       IN      A       192.0.2.1
ns1     IN      A       192.0.2.10
ns2     IN      A       192.0.2.11
www     IN      A       192.0.2.2
mail    IN      A       192.0.2.3

; MX records
@       IN      MX      10 mail.example.com.

; CNAME records
ftp     IN      CNAME   www.example.com.

; TXT records
@       IN      TXT     "v=spf1 mx -all"
EOF

echo "Created test zone file at: $ZONE_FILE"
echo ""
echo "To test zone transfers:"
echo ""
echo "1. Start Heimdall with zone transfer support:"
echo "   export HEIMDALL_ZONE_FILES=\"$ZONE_FILE\""
echo "   export HEIMDALL_AUTHORITATIVE_ENABLED=true"
echo "   export HEIMDALL_ALLOWED_ZONE_TRANSFERS=\"\"  # Empty = allow all (for testing only!)"
echo "   cargo run"
echo ""
echo "2. In another terminal, test AXFR (full zone transfer):"
echo "   dig @127.0.0.1 -p 1053 example.com AXFR +tcp"
echo ""
echo "3. Test IXFR (incremental zone transfer - currently falls back to AXFR):"
echo "   dig @127.0.0.1 -p 1053 example.com IXFR=2024010100 +tcp"
echo ""
echo "4. Test from a specific IP (for access control testing):"
echo "   export HEIMDALL_ALLOWED_ZONE_TRANSFERS=\"10.0.0.1,192.168.1.0/24\""
echo "   # Then only those IPs can perform zone transfers"
echo ""
echo "Note: Zone transfers require TCP, so +tcp flag is mandatory"