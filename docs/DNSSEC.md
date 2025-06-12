# DNSSEC Validation in Heimdall

Heimdall DNS server includes comprehensive DNSSEC (Domain Name System Security Extensions) validation support to ensure the authenticity and integrity of DNS responses.

## Overview

DNSSEC provides cryptographic authentication of DNS data, protecting against:
- DNS spoofing and cache poisoning attacks
- Man-in-the-middle attacks
- Forged DNS responses

## Features

### Supported Algorithms
- **RSA/SHA-256** (Algorithm 8) - Recommended
- **RSA/SHA-512** (Algorithm 10)
- **ECDSA P-256/SHA-256** (Algorithm 13) - Recommended
- **ECDSA P-384/SHA-384** (Algorithm 14)
- **Ed25519** (Algorithm 15) - Recommended
- **Ed448** (Algorithm 16)

### Validation Capabilities
- **Signature Verification**: Validates RRSIG records against DNSKEY records
- **Chain of Trust**: Verifies delegation from root to leaf zones
- **Trust Anchor Management**: Built-in root trust anchors (KSK-2017 and KSK-2024)
- **Denial of Existence**: NSEC and NSEC3 validation
- **DS Record Validation**: Verifies DNSKEY records against parent DS records
- **Signature Timing**: Checks inception and expiration times

## Configuration

### Environment Variables

```bash
# Enable DNSSEC validation
export HEIMDALL_DNSSEC_ENABLED=true

# Enable strict mode (reject bogus responses)
export HEIMDALL_DNSSEC_STRICT=true
```

### Configuration File

```toml
# heimdall.toml
dnssec_enabled = true
dnssec_strict = false  # Set to true to reject bogus responses
```

### Programmatic Configuration

```rust
use heimdall::config::DnsConfig;

let config = DnsConfig {
    dnssec_enabled: true,
    dnssec_strict: false,
    ..Default::default()
};
```

## Validation Modes

### 1. Permissive Mode (default)
- `dnssec_enabled = true`
- `dnssec_strict = false`
- Validates DNSSEC signatures when present
- Logs validation failures but still returns responses
- Suitable for monitoring and testing

### 2. Strict Mode
- `dnssec_enabled = true`
- `dnssec_strict = true`
- Rejects responses that fail DNSSEC validation
- Returns SERVFAIL for bogus responses
- Recommended for security-critical environments

## Validation Results

Heimdall categorizes DNSSEC validation results into four states:

1. **Secure**: Response is properly signed and validated
2. **Insecure**: Response has no DNSSEC records (unsigned zone)
3. **Bogus**: Response has invalid signatures or failed validation
4. **Indeterminate**: Unable to determine validation status (missing data)

## Testing DNSSEC

### Test Secure Domain
```bash
# Query a DNSSEC-signed domain
dig @127.0.0.1 -p 1053 cloudflare.com A +dnssec

# Should return:
# - A records with IP addresses
# - RRSIG records (signatures)
# - Status: NOERROR
```

### Test Validation Failure
```bash
# Query a domain with broken DNSSEC (test domain)
dig @127.0.0.1 -p 1053 dnssec-failed.org A +dnssec

# With strict mode enabled:
# - Status: SERVFAIL
# - No answer section
```

### Test Unsigned Domain
```bash
# Query an unsigned domain
dig @127.0.0.1 -p 1053 example.com A +dnssec

# Should return:
# - A records without RRSIG
# - Status: NOERROR (insecure but valid)
```

## Trust Anchors

Heimdall includes the current DNSSEC root trust anchors:

- **KSK-2024** (Key Tag: 20326) - Current root KSK
- **KSK-2017** (Key Tag: 19036) - Previous root KSK (for rollover compatibility)

Trust anchors are automatically loaded and do not require manual configuration.

## Performance Considerations

- **Initial Queries**: May be slower due to additional DNSSEC record fetching
- **Cached Responses**: Validation results are cached along with DNS records
- **CPU Usage**: Cryptographic operations increase CPU usage slightly
- **Network Traffic**: DNSSEC responses are larger due to signatures

## Troubleshooting

### Enable Debug Logging
```bash
RUST_LOG=heimdall::dnssec=debug cargo run
```

### Common Issues

1. **All queries return SERVFAIL**
   - Check if `dnssec_strict` is enabled
   - Verify upstream DNS servers support DNSSEC
   - Ensure system time is accurate (for signature validation)

2. **No RRSIG records in responses**
   - Verify DNSSEC is enabled in configuration
   - Check if the queried domain is DNSSEC-signed
   - Ensure DO (DNSSEC OK) flag is set in queries

3. **Validation failures for known-good domains**
   - Check system time synchronization
   - Verify network connectivity to upstream servers
   - Review debug logs for specific validation errors

## Security Recommendations

1. **Production Deployment**
   - Enable DNSSEC validation (`dnssec_enabled = true`)
   - Start with permissive mode for testing
   - Monitor logs for validation failures
   - Switch to strict mode after verification

2. **Upstream Servers**
   - Use DNSSEC-validating upstream resolvers
   - Recommended: 1.1.1.1 (Cloudflare), 8.8.8.8 (Google)
   - Avoid ISP resolvers that may not support DNSSEC

3. **Monitoring**
   - Track DNSSEC validation metrics
   - Alert on sudden increases in validation failures
   - Monitor trust anchor updates

## Implementation Details

Heimdall's DNSSEC implementation follows these RFCs:
- RFC 4033: DNS Security Introduction and Requirements
- RFC 4034: Resource Records for DNS Security Extensions
- RFC 4035: Protocol Modifications for DNS Security Extensions
- RFC 5155: DNS Security (DNSSEC) Hashed Authenticated Denial of Existence
- RFC 6840: Clarifications and Implementation Notes for DNS Security
- RFC 8624: Algorithm Implementation Requirements and Usage Guidance