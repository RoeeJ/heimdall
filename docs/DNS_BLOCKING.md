# DNS Blocking in Heimdall

Heimdall includes a powerful DNS blocking feature that allows you to block unwanted domains at the DNS level. This provides network-wide ad blocking, malware protection, and content filtering capabilities.

**Note: DNS blocking is enabled by default in Heimdall!** The server comes pre-configured with:
- **Blocking Mode**: Zero IP (returns 0.0.0.0 for blocked domains)
- **Default Blocklists**: StevenBlack's unified hosts and URLhaus malware domains
- **Auto-Updates**: Enabled with 24-hour update intervals

To disable blocking, set `HEIMDALL_BLOCKING_ENABLED=false`.

## Features

- **Multiple blocklist formats**: Supports hosts files, AdBlock Plus, Pi-hole, dnsmasq, unbound, and simple domain lists
- **Efficient lookups**: Uses concurrent hashmaps for O(1) domain lookups
- **Wildcard support**: Block entire domain hierarchies with `*.domain.com` patterns
- **Allowlisting**: Override blocks for specific domains
- **Multiple blocking modes**: Choose how blocked queries are handled
- **Automatic updates**: Keep blocklists current with periodic downloads
- **Statistics and metrics**: Track blocking effectiveness

## Configuration

### Environment Variables

```bash
# DNS blocking is ENABLED by default
# To disable blocking:
HEIMDALL_BLOCKING_ENABLED=false

# Blocking mode (nxdomain, zero_ip, custom_ip, refused)
# Default is zero_ip
HEIMDALL_BLOCKING_MODE=zero_ip

# Custom IP for blocked domains (only for custom_ip mode)
HEIMDALL_BLOCKING_CUSTOM_IP=127.0.0.1

# Enable wildcard blocking (*.domain.com)
HEIMDALL_BLOCKING_ENABLE_WILDCARDS=true

# Blocklists in format: path:format:name (comma-separated)
# Default blocklists are configured:
# - blocklists/stevenblack-hosts.txt:hosts:StevenBlack
# - blocklists/malware-domains.txt:hosts:MalwareDomains
HEIMDALL_BLOCKLISTS=/path/to/hosts.txt:hosts:MyBlocklist,/path/to/domains.txt:domain_list:BadDomains

# Allowlist domains (comma-separated)
HEIMDALL_ALLOWLIST=safe.example.com,trusted.site.com

# Auto-update blocklists (enabled by default)
HEIMDALL_BLOCKLIST_AUTO_UPDATE=true

# Update interval in seconds (default: 86400 = 24 hours)
HEIMDALL_BLOCKLIST_UPDATE_INTERVAL=86400
```

## Blocking Modes

### 1. NXDOMAIN (Default)
Returns a "non-existent domain" response for blocked queries.

```bash
HEIMDALL_BLOCKING_MODE=nxdomain
```

**Advantages:**
- Clean failure for blocked domains
- No IP conflicts
- Works with all DNS clients

**Response:**
```
;; ->>HEADER<<- opcode: QUERY, status: NXDOMAIN, id: 12345
;; flags: qr rd ra; QUERY: 1, ANSWER: 0, AUTHORITY: 1, ADDITIONAL: 0
```

### 2. Zero IP
Returns 0.0.0.0 for A queries and :: for AAAA queries.

```bash
HEIMDALL_BLOCKING_MODE=zero_ip
```

**Advantages:**
- Fast failure for connection attempts
- Some applications handle this better than NXDOMAIN
- Standard approach used by many blockers

**Response:**
```
blocked.example.com.    300    IN    A    0.0.0.0
```

### 3. Custom IP
Returns a specified IP address for blocked domains.

```bash
HEIMDALL_BLOCKING_MODE=custom_ip
HEIMDALL_BLOCKING_CUSTOM_IP=127.0.0.1
```

**Advantages:**
- Can redirect to a block page
- Useful for monitoring blocked requests
- Flexible for custom implementations

**Response:**
```
blocked.example.com.    300    IN    A    127.0.0.1
```

### 4. REFUSED
Returns a REFUSED response for blocked queries.

```bash
HEIMDALL_BLOCKING_MODE=refused
```

**Advantages:**
- Clear signal that the query was rejected
- No fake DNS data returned
- Compliant with DNS standards

**Response:**
```
;; ->>HEADER<<- opcode: QUERY, status: REFUSED, id: 12345
;; flags: qr rd ra; QUERY: 1, ANSWER: 0, AUTHORITY: 0, ADDITIONAL: 0
```

## Blocklist Formats

### 1. Domain List
Simple text file with one domain per line.

```
# Simple domain list
ads.example.com
tracker.site.com
*.doubleclick.net
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/domains.txt:domain_list:MyDomains
```

### 2. Hosts File
Standard hosts file format with IP addresses.

```
# Hosts file format
127.0.0.1   localhost
0.0.0.0     ads.example.com
0.0.0.0     tracker.site.com
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/hosts.txt:hosts:MyHosts
```

### 3. AdBlock Plus
AdBlock Plus filter syntax (domain rules only).

```
! AdBlock Plus format
||ads.example.com^
||tracker.site.com^
||*.doubleclick.net^
@@||safe.example.com^
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/adblock.txt:adblock:AdBlockList
```

### 4. Pi-hole
Pi-hole compatible format (supports both hosts and domain formats).

```
# Pi-hole format
0.0.0.0 ads.example.com
tracker.site.com
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/pihole.txt:pihole:PiHoleList
```

### 5. dnsmasq
dnsmasq configuration format.

```
# dnsmasq format
address=/ads.example.com/0.0.0.0
server=/tracker.site.com/#
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/dnsmasq.conf:dnsmasq:DnsmasqList
```

### 6. Unbound
Unbound local-zone format.

```
# Unbound format
local-zone: "ads.example.com" refuse
local-zone: "tracker.site.com" static
```

Configuration:
```bash
HEIMDALL_BLOCKLISTS=/path/to/unbound.conf:unbound:UnboundList
```

## Multiple Blocklists

You can load multiple blocklists simultaneously:

```bash
HEIMDALL_BLOCKLISTS=\
/etc/heimdall/ads.txt:hosts:AdsList,\
/etc/heimdall/malware.txt:domain_list:MalwareList,\
/etc/heimdall/custom.txt:adblock:CustomList
```

## Wildcard Blocking

When enabled, Heimdall supports wildcard patterns:

```
# Block all subdomains
*.doubleclick.net

# This will block:
# - ads.doubleclick.net
# - tracker.ads.doubleclick.net
# - any.subdomain.doubleclick.net
# But NOT doubleclick.net itself
```

## Allowlisting

Domains in the allowlist are never blocked, even if they appear in blocklists:

```bash
# Via environment variable
HEIMDALL_ALLOWLIST=safe.ads.com,analytics.mysite.com

# Via API (if implemented)
curl -X POST http://localhost:8080/api/allowlist \
  -d '{"domain": "safe.example.com"}'
```

## Automatic Updates

### Default Blocklists

Heimdall includes configurations for popular blocklists:

1. **StevenBlack's Hosts**
   - Unified hosts file with base extensions
   - Updates daily
   - Blocks ads, malware, and tracking domains

2. **AdGuard DNS Filter**
   - Comprehensive ad blocking
   - Mobile-optimized
   - Updates daily

3. **URLhaus Malware Filter**
   - Active malware domains
   - Updates every 12 hours
   - Critical security protection

### Enabling Auto-Updates

```bash
# Enable automatic updates
HEIMDALL_BLOCKLIST_AUTO_UPDATE=true

# Update every 12 hours
HEIMDALL_BLOCKLIST_UPDATE_INTERVAL=43200
```

### Manual Updates

You can also trigger updates manually:

```bash
# Via signal (if implemented)
kill -USR1 $(pgrep heimdall)

# Via API (if implemented)
curl -X POST http://localhost:8080/api/blocklists/update
```

## Monitoring and Statistics

### Prometheus Metrics

Heimdall exports blocking metrics in Prometheus format:

```
# Total blocked queries
heimdall_blocked_queries_total 15234

# Total domains in blocklists
heimdall_blocked_domains_total 95432

# Allowlist size
heimdall_allowlist_size 25

# Block rate
heimdall_dns_block_rate 12.5
```

### Query Logs

Enable debug logging to see blocking decisions:

```bash
RUST_LOG=heimdall::blocking=debug cargo run
```

Example logs:
```
[DEBUG] Domain ads.example.com blocked (exact match)
[DEBUG] Domain sub.tracker.com blocked (wildcard match: tracker.com)
[DEBUG] Domain safe.ads.com allowed (in allowlist)
```

## Performance Considerations

### Memory Usage

- Each blocked domain uses approximately 100-200 bytes
- 100,000 domains ≈ 10-20 MB RAM
- Wildcard patterns use slightly more memory

### Lookup Performance

- Exact domain lookups: O(1) with hashmap
- Wildcard checks: O(n) where n is domain label count
- Typical lookup time: < 1 microsecond

### Optimization Tips

1. **Use exact domains when possible** - Faster than wildcards
2. **Minimize allowlist size** - Checked on every query
3. **Disable wildcards if not needed** - Saves CPU cycles
4. **Use efficient blocklist formats** - Domain lists parse fastest

## Example Configurations

### Basic Ad Blocking

```bash
HEIMDALL_BLOCKING_ENABLED=true
HEIMDALL_BLOCKING_MODE=zero_ip
HEIMDALL_BLOCKLISTS=/etc/heimdall/ads-hosts.txt:hosts:AdsBlocker
```

### Family-Friendly DNS

```bash
HEIMDALL_BLOCKING_ENABLED=true
HEIMDALL_BLOCKING_MODE=nxdomain
HEIMDALL_BLOCKLISTS=\
/etc/heimdall/adult-content.txt:domain_list:AdultFilter,\
/etc/heimdall/gambling.txt:domain_list:GamblingFilter,\
/etc/heimdall/malware.txt:hosts:MalwareFilter
HEIMDALL_ALLOWLIST=educational.site.com
```

### Enterprise Security

```bash
HEIMDALL_BLOCKING_ENABLED=true
HEIMDALL_BLOCKING_MODE=refused
HEIMDALL_BLOCKING_ENABLE_WILDCARDS=true
HEIMDALL_BLOCKLISTS=\
/etc/heimdall/malware.txt:hosts:Malware,\
/etc/heimdall/phishing.txt:domain_list:Phishing,\
/etc/heimdall/c2-servers.txt:domain_list:C2Servers
HEIMDALL_BLOCKLIST_AUTO_UPDATE=true
HEIMDALL_BLOCKLIST_UPDATE_INTERVAL=3600  # Update hourly
```

## Troubleshooting

### Domain Not Being Blocked

1. Check if blocking is enabled:
   ```bash
   echo $HEIMDALL_BLOCKING_ENABLED
   ```

2. Verify domain is in blocklist:
   ```bash
   grep "example.com" /path/to/blocklist.txt
   ```

3. Check if domain is allowlisted:
   ```bash
   echo $HEIMDALL_ALLOWLIST | grep "example.com"
   ```

4. Enable debug logging:
   ```bash
   RUST_LOG=heimdall::blocking=debug cargo run
   ```

### High Memory Usage

1. Check blocklist sizes:
   ```bash
   wc -l /path/to/blocklists/*
   ```

2. Disable wildcards if not needed:
   ```bash
   HEIMDALL_BLOCKING_ENABLE_WILDCARDS=false
   ```

3. Use more efficient formats (domain_list vs hosts)

### Performance Issues

1. Monitor metrics:
   ```bash
   curl http://localhost:8080/metrics | grep heimdall_blocked
   ```

2. Reduce blocklist update frequency
3. Consider using Redis cache for large deployments

## Security Considerations

1. **Blocklist Sources**: Only use trusted blocklist sources
2. **HTTPS Updates**: Ensure blocklist URLs use HTTPS
3. **Local Storage**: Protect blocklist files with appropriate permissions
4. **Allowlist Carefully**: Each allowlisted domain bypasses all blocking
5. **Monitor Changes**: Track blocklist sizes and content changes

## Integration Examples

### With Docker

```dockerfile
FROM heimdall:latest

# Add custom blocklists
COPY blocklists/ /etc/heimdall/blocklists/

# Configure blocking
ENV HEIMDALL_BLOCKING_ENABLED=true
ENV HEIMDALL_BLOCKING_MODE=zero_ip
ENV HEIMDALL_BLOCKLISTS=/etc/heimdall/blocklists/ads.txt:hosts:Ads
```

### With Kubernetes

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: heimdall-blocking
data:
  ads.txt: |
    0.0.0.0 ads.example.com
    0.0.0.0 tracker.site.com
---
apiVersion: v1
kind: Deployment
spec:
  template:
    spec:
      containers:
      - name: heimdall
        env:
        - name: HEIMDALL_BLOCKING_ENABLED
          value: "true"
        - name: HEIMDALL_BLOCKLISTS
          value: "/config/ads.txt:hosts:AdsBlocker"
        volumeMounts:
        - name: blocklists
          mountPath: /config
      volumes:
      - name: blocklists
        configMap:
          name: heimdall-blocking
```

## Future Enhancements

- [ ] REST API for managing blocklists
- [ ] Real-time blocklist updates via websocket
- [ ] Machine learning for anomaly detection
- [ ] Regex pattern support
- [ ] Time-based blocking rules
- [ ] Per-client blocking policies
- [ ] Blocklist source verification (signatures)
- [ ] Compressed blocklist support
- [ ] Incremental blocklist updates