# DNS Diagnostics Tools for Heimdall

This directory contains several diagnostic scripts to help troubleshoot DNS resolution issues when using Heimdall as a DNS server.

## Overview

When applications break or websites fail to load with Heimdall, these tools help identify the root cause by comparing Heimdall's behavior with standard DNS servers and testing various DNS patterns.

## Tools

### 1. `test_dns_compatibility.sh`
Comprehensive compatibility test suite that checks various DNS features and compares results with upstream DNS servers.

**Usage:**
```bash
# Basic usage (tests localhost:1053)
./test_dns_compatibility.sh

# Test remote Heimdall instance
./test_dns_compatibility.sh -h 192.168.1.100 -p 1053

# Compare with specific upstream DNS
./test_dns_compatibility.sh -u 1.1.1.1

# Verbose mode (detailed logging)
./test_dns_compatibility.sh -v
```

**What it tests:**
- Basic DNS queries (A, AAAA, MX, TXT, etc.)
- EDNS support
- TCP fallback
- Case sensitivity
- Response times
- DNSSEC support
- Edge cases (NXDOMAIN, NODATA, root queries)
- Concurrent query handling
- Large response handling
- Consistency with upstream DNS

### 2. `diagnose_dns_issue.sh`
Deep diagnostic tool for investigating specific domain resolution issues.

**Usage:**
```bash
# Diagnose issues with a specific domain
./diagnose_dns_issue.sh github.com

# Test specific record type
./diagnose_dns_issue.sh gmail.com MX

# Test with custom Heimdall location
HEIMDALL_IP=192.168.1.100 ./diagnose_dns_issue.sh example.com
```

**What it analyzes:**
- Side-by-side comparison with upstream DNS
- Response flags analysis
- EDNS support verification
- Different query flag combinations
- TCP query testing
- Truncation detection
- Raw packet analysis
- Query timing consistency
- Cache behavior
- DNSSEC validation status

### 3. `monitor_dns_health.sh`
Real-time monitoring tool that continuously checks DNS health and reports issues.

**Usage:**
```bash
# Basic monitoring (checks every 5 seconds)
./monitor_dns_health.sh

# Custom check interval
./monitor_dns_health.sh -i 10

# Verbose mode (show all queries)
./monitor_dns_health.sh -v

# Monitor remote instance
./monitor_dns_health.sh -h 192.168.1.100 -p 1053
```

**What it monitors:**
- Query success rate
- Response times (alerts on >100ms)
- Failed queries
- Edge case handling
- Metrics endpoint (if available)
- Periodic health summaries

### 4. `test_application_dns.sh`
Tests DNS patterns used by common applications to identify compatibility issues.

**Usage:**
```bash
# Test all application patterns
./test_application_dns.sh

# Test against remote Heimdall
HEIMDALL_IP=192.168.1.100 ./test_application_dns.sh
```

**What it tests:**
- Web browser patterns (dual-stack, HTTPS records)
- Email client patterns (MX, SPF, DMARC)
- VoIP/SIP patterns (SRV, NAPTR)
- Container/Kubernetes patterns
- CDN and load balancer patterns
- Security software patterns
- Gaming console patterns
- IoT device patterns
- Streaming service patterns
- Special cases (IDN, case randomization)

## Common Issues and Solutions

### 1. NXDOMAIN not returned for non-existent domains
**Symptom:** Applications hang or timeout instead of failing quickly
**Check:** Run `diagnose_dns_issue.sh non-existent-domain.com`
**Solution:** Ensure Heimdall returns proper NXDOMAIN responses

### 2. Large responses truncated
**Symptom:** Some domains don't resolve, especially those with many records
**Check:** Look for TC (truncation) flag in diagnostic output
**Solution:** Ensure TCP fallback is working and EDNS is properly supported

### 3. DNSSEC validation failures
**Symptom:** Domains that work with other DNS servers fail with Heimdall
**Check:** Run tests with `HEIMDALL_DNSSEC_STRICT=false`
**Solution:** Check DNSSEC configuration or disable strict validation

### 4. Slow responses
**Symptom:** Websites load slowly
**Check:** Use `monitor_dns_health.sh` to track response times
**Solution:** Check cache configuration and upstream server health

### 5. Intermittent failures
**Symptom:** Random DNS resolution failures
**Check:** Use `monitor_dns_health.sh` for extended period
**Solution:** Check for rate limiting, connection limits, or resource exhaustion

## Debugging Workflow

1. **Start with the compatibility test:**
   ```bash
   ./test_dns_compatibility.sh
   ```
   This gives you a broad overview of what's working and what's not.

2. **For specific domain issues:**
   ```bash
   ./diagnose_dns_issue.sh problematic-domain.com
   ```
   This provides detailed analysis of why a specific domain might be failing.

3. **For intermittent issues:**
   ```bash
   ./monitor_dns_health.sh -v
   ```
   Leave this running to catch issues as they happen.

4. **For application-specific problems:**
   ```bash
   ./test_application_dns.sh
   ```
   This helps identify if the issue is with specific DNS patterns.

## Logging

All scripts create detailed log files with timestamps:
- `heimdall_test_YYYYMMDD_HHMMSS.log` - Compatibility test results
- `heimdall_monitor_YYYYMMDD_HHMMSS.log` - Health monitoring data

## Integration with CI/CD

These scripts can be integrated into your CI/CD pipeline:

```yaml
# Example GitHub Actions workflow
- name: Start Heimdall
  run: ./start_server.sh

- name: Run DNS compatibility tests
  run: ./test_dns_compatibility.sh
  
- name: Test application patterns
  run: ./test_application_dns.sh
```

## Tips

1. Always compare Heimdall's behavior with a known-good DNS server (like 8.8.8.8)
2. Pay attention to response flags, especially TC (truncated) and AD (authenticated data)
3. Check both UDP and TCP queries - some applications require TCP
4. Monitor response times - even successful queries can cause issues if they're slow
5. Test with actual domain names your applications use
6. Check Heimdall's logs (`heimdall.log`) for internal errors

## Contributing

If you discover new DNS patterns that break with Heimdall, please add them to the test scripts and submit a pull request.