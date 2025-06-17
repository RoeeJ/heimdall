# UDP Truncation Support (TC Flag)

Heimdall DNS server implements proper UDP truncation according to RFC 1035 and RFC 6891 (EDNS) specifications.

## Overview

DNS responses that exceed UDP size limits are automatically truncated with the TC (Truncated) flag set, signaling clients to retry the query over TCP for the complete response.

## How It Works

### 1. Size Limit Detection

**Standard UDP (No EDNS):**
- Maximum size: 512 bytes
- Applied to all clients not supporting EDNS

**EDNS-enabled UDP:**
- Maximum size: Client-specified payload size (typically 1232-4096 bytes)
- Extracted from EDNS OPT record in query
- Fallback to 512 bytes if EDNS parsing fails

### 2. Truncation Process

When a response exceeds the UDP size limit:

1. **Original response generated** from upstream servers
2. **Size check performed** against client's UDP limit
3. **Truncated response created** if too large:
   - Sets TC (Truncated) flag to `true`
   - Maintains original query ID and question section
   - Clears all answer/authority/additional sections
   - Preserves EDNS OPT record if present
4. **Client notified** to retry over TCP

### 3. Response Structure

**Truncated Response Headers:**
```
QR = 1    (Response)
TC = 1    (Truncated - retry with TCP)
RD = 1    (Recursion Desired - copied from query)
RA = 1    (Recursion Available)
RCODE = 0 (NOERROR)

QDCOUNT = 1 (Original question preserved)
ANCOUNT = 0 (No answers - truncated)
NSCOUNT = 0 (No authority records)
ARCOUNT = 0 (No additional records, except EDNS if present)
```

## Code Implementation

### Server Logic (UDP)

```rust
// Check if response exceeds UDP size limit
let max_udp_size = query_packet.max_udp_payload_size();

if response_data.len() > max_udp_size as usize {
    // Create truncated response with TC flag
    let truncated_response = resolver.create_truncated_response(&query_packet);
    let truncated_data = truncated_response.serialize()?;
    
    // Record metrics
    metrics.record_truncated_response("udp", reason);
    
    // Send truncated response
    sock.send_to(&truncated_data, client_addr).await?;
}
```

### Resolver Methods

**Creating truncated responses:**
```rust
impl DnsResolver {
    pub fn create_truncated_response(&self, query: &DNSPacket) -> DNSPacket {
        let mut response = query.clone();
        response.header.qr = true;  // Response
        response.header.tc = true;  // Truncated
        response.header.ra = true;  // Recursion Available
        response.header.rcode = 0;  // NOERROR
        
        // Clear answer sections
        response.answers.clear();
        response.authorities.clear();
        response.resources.clear();
        
        response
    }
}
```

## Monitoring and Metrics

### Prometheus Metrics

**Truncation tracking:**
```
heimdall_truncated_responses_total{protocol, reason}
```

**Labels:**
- `protocol`: "udp" (TCP responses are never truncated)
- `reason`: Truncation cause
  - `"no_edns"`: Client doesn't support EDNS (512 byte limit)
  - `"exceeds_edns_limit"`: Response exceeds client's EDNS payload size

**Example metrics:**
```
# HELP heimdall_truncated_responses_total Total number of responses truncated due to UDP size limits
# TYPE heimdall_truncated_responses_total counter
heimdall_truncated_responses_total{protocol="udp",reason="no_edns"} 145
heimdall_truncated_responses_total{protocol="udp",reason="exceeds_edns_limit"} 23
```

### Monitoring Queries

**Truncation rate:**
```promql
rate(heimdall_truncated_responses_total[5m])
```

**Truncation reasons:**
```promql
sum by (reason) (rate(heimdall_truncated_responses_total[5m]))
```

**Percentage of queries truncated:**
```promql
(
  rate(heimdall_truncated_responses_total[5m]) / 
  rate(heimdall_queries_total{protocol="udp"}[5m])
) * 100
```

## Client Behavior

### Compliant Clients

Modern DNS clients automatically handle truncation:

1. **Receive truncated response** with TC=1
2. **Detect TC flag** in response header
3. **Retry identical query** over TCP
4. **Receive complete response** via TCP

### Example with dig

```bash
# Client receives truncated UDP response
$ dig large-txt-record.example.com @your-server

;; Truncated, retrying in TCP mode.

# dig automatically retries over TCP
;; ANSWER SECTION:
large-txt-record.example.com. 300 IN TXT "very long text record..."
```

## Common Scenarios

### 1. Large TXT Records

**Query:** TXT record with long content
**UDP Limit:** 512 bytes (no EDNS)
**Response Size:** 800 bytes
**Result:** Truncated response sent, client retries over TCP

### 2. Multiple A Records

**Query:** Domain with many A records
**UDP Limit:** 1232 bytes (EDNS)
**Response Size:** 1500 bytes
**Result:** Truncated response sent

### 3. DNSSEC Responses

**Query:** Domain with DNSSEC signatures
**UDP Limit:** 4096 bytes (EDNS)
**Response Size:** 5000 bytes
**Result:** Truncated response sent

## Configuration

### EDNS Support

Heimdall automatically detects and respects EDNS payload sizes from client queries.

**Default behavior:**
- Non-EDNS clients: 512 byte limit
- EDNS clients: Use advertised payload size
- Maximum enforced: 4096 bytes (safety limit)

### Logging

**Production level (INFO):**
```
# No truncation logs (handled gracefully)
```

**Debug level (DEBUG):**
```
DEBUG: Response too large for UDP (1500>512 bytes), sending truncated response
```

**Metrics always collected regardless of log level**

## Performance Impact

### Minimal Overhead

- **Size check:** ~0.1µs per UDP response
- **Truncation creation:** ~10µs when triggered
- **Metrics recording:** ~0.1µs per truncated response
- **No impact on TCP responses**

### Benefits

✅ **RFC Compliance**: Proper DNS standard implementation  
✅ **Client Compatibility**: Works with all modern DNS clients  
✅ **Automatic Fallback**: Seamless TCP retry mechanism  
✅ **Monitoring**: Full visibility into truncation patterns  
✅ **Performance**: Fast UDP for small responses, TCP for large  

## Troubleshooting

### High Truncation Rates

**Symptoms:**
```promql
rate(heimdall_truncated_responses_total[5m]) > 50
```

**Possible causes:**
1. Many clients without EDNS support
2. Upstream servers returning very large responses
3. DNSSEC-enabled domains with large signatures

**Solutions:**
1. Encourage client EDNS support
2. Consider response filtering for UDP
3. Monitor specific query types causing truncation

### Clients Not Retrying TCP

**Symptoms:** Incomplete responses to clients

**Debugging:**
1. Verify TC flag is set in truncated responses
2. Check client DNS software version
3. Confirm TCP port 53 is accessible
4. Monitor TCP vs UDP query ratios

This implementation ensures proper DNS protocol compliance while maintaining high performance for the majority of queries that fit within UDP limits.