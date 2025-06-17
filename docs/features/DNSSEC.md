# DNSSEC Implementation

## Overview

Heimdall now includes comprehensive DNSSEC validation support (RFC 4033-4035), providing cryptographic authentication of DNS data. This implementation ensures that DNS responses have not been tampered with and originate from authoritative sources.

## Features

### Core DNSSEC Validation
- **Signature Verification**: Validates RRSIG records using RSA, ECDSA, and EdDSA algorithms
- **Chain of Trust**: Verifies the authentication chain from root to target domain
- **Trust Anchor Management**: Pre-configured with current root KSKs (2024 and 2017)
- **Denial of Existence**: Validates NSEC and NSEC3 records for authenticated negative responses

### Supported Algorithms
- RSA/SHA-1 (Algorithm 5) - Legacy support
- RSA/SHA-256 (Algorithm 8) - Recommended
- RSA/SHA-512 (Algorithm 10)
- ECDSA P-256/SHA-256 (Algorithm 13) - Recommended
- ECDSA P-384/SHA-384 (Algorithm 14)
- Ed25519 (Algorithm 15) - Modern, recommended
- Ed448 (Algorithm 16) - Modern

### Validation Modes
- **Non-strict Mode** (default): Logs validation failures but returns responses
- **Strict Mode**: Rejects responses that fail DNSSEC validation

## Configuration

### Environment Variables
```bash
# Enable DNSSEC validation
HEIMDALL_DNSSEC_ENABLED=true

# Enable strict mode (reject bogus responses)
HEIMDALL_DNSSEC_STRICT=true
```

### Configuration File
```toml
[dnssec]
enabled = true
strict = false
```

## Architecture

### Module Structure
```
src/dnssec/
├── mod.rs           # Main module and ValidationResult enum
├── algorithm.rs     # DNSSEC algorithm definitions
├── digest.rs        # Digest type implementations
├── key_tag.rs       # Key tag calculation (RFC 4034)
├── trust_anchor.rs  # Root trust anchor storage
├── validator.rs     # Main validation logic
├── denial.rs        # NSEC/NSEC3 denial validation
└── errors.rs        # Error types
```

### Validation Flow
1. **Response Reception**: DNS response received from upstream
2. **DNSSEC Check**: If DNSSEC is enabled, validation begins
3. **DNSKEY Lookup**: Find DNSKEY records for the zone
4. **Signature Validation**: Verify RRSIG signatures on records
5. **Chain Building**: Build trust chain to root anchors
6. **DS Validation**: Verify delegation signer records
7. **Result Processing**: Return Secure/Insecure/Bogus/Indeterminate

### Validation Results
- **Secure**: All signatures valid, chain of trust intact
- **Insecure**: Domain not signed with DNSSEC
- **Bogus**: Validation failed (signature invalid, chain broken)
- **Indeterminate**: Unable to determine status

## Implementation Details

### Trust Anchors
The implementation includes hardcoded root trust anchors:
- **KSK-2024** (Key Tag: 38696): Current root signing key
- **KSK-2017** (Key Tag: 20326): Previous root signing key (for rollover)

### Key Tag Calculation
Implements RFC 4034 Appendix B algorithm for computing key tags from DNSKEY records.

### Cryptographic Operations
Uses the `ring` crate for all cryptographic operations:
- RSA signature verification (PKCS#1)
- ECDSA signature verification (P-256, P-384)
- EdDSA signature verification (Ed25519)
- SHA-1, SHA-256, SHA-384, SHA-512 digests

### Denial of Existence
Validates authenticated negative responses:
- **NSEC**: Proves non-existence by showing gaps in ordered namespace
- **NSEC3**: Hashed version of NSEC for zone enumeration protection

## Testing

### Unit Tests
- Algorithm conversion and support tests
- Key tag calculation tests
- NSEC/NSEC3 validation tests
- Trust anchor management tests

### Integration Tests
- DNSSEC configuration tests
- Resolver integration tests
- Validation mode tests

## Security Considerations

### Algorithm Recommendations
- **Recommended**: Ed25519, ECDSA P-256, RSA/SHA-256
- **Acceptable**: RSA/SHA-512, ECDSA P-384
- **Deprecated**: RSA/SHA-1 (supported for compatibility)

### Validation Failures
In non-strict mode, validation failures are logged but responses are returned. This allows:
- Gradual DNSSEC deployment
- Debugging of validation issues
- Compatibility with misconfigured domains

In strict mode, bogus responses are rejected to ensure maximum security.

## Performance Impact

DNSSEC validation adds computational overhead:
- Signature verification: ~1-5ms per response
- Chain building: ~2-10ms for deep hierarchies
- Caching can significantly reduce validation overhead

## Future Enhancements

### Planned Features
1. **DNSSEC-aware Caching**: Cache validation state with responses
2. **RFC 5011 Support**: Automatic trust anchor updates
3. **Performance Optimization**: Parallel validation, signature caching
4. **Extended Validation**: Certificate transparency integration

### Deferred Features
- DNSSEC signing (authoritative server feature)
- Online signing with private keys
- Key management and rotation

## Usage Examples

### Basic DNSSEC Query
```bash
# Start server with DNSSEC validation
HEIMDALL_DNSSEC_ENABLED=true cargo run

# Query DNSSEC-signed domain
dig @127.0.0.1 -p 1053 cloudflare.com A

# Check validation in logs
# [INFO] DNSSEC validation successful for cloudflare.com
```

### Strict Mode
```bash
# Start with strict validation
HEIMDALL_DNSSEC_ENABLED=true HEIMDALL_DNSSEC_STRICT=true cargo run

# Bogus responses will be rejected
# [WARN] DNSSEC validation failed: Signature verification failed
```

## Troubleshooting

### Common Issues
1. **Clock Skew**: Ensure system time is synchronized (NTP)
2. **Missing Chain**: Some domains have broken DNSSEC deployments
3. **Algorithm Support**: Very old or experimental algorithms may not be supported

### Debug Logging
Enable debug logging for detailed validation information:
```bash
RUST_LOG=heimdall::dnssec=debug cargo run
```

## References
- RFC 4033: DNS Security Introduction and Requirements
- RFC 4034: Resource Records for DNS Security Extensions
- RFC 4035: Protocol Modifications for DNS Security Extensions
- RFC 5155: DNS Security (DNSSEC) Hashed Authenticated Denial of Existence
- RFC 8624: Algorithm Implementation Requirements and Usage Guidance