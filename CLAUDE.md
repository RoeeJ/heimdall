# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Heimdall is a fully functional DNS server implementation in Rust that supports both UDP and TCP protocols on port 1053. Features complete DNS resolution logic, intelligent caching, and robust compression pointer handling.

## Common Development Commands

### Build and Run
```bash
# Build the project
cargo build

# Run the DNS server (listens on port 1053)
cargo run

# Build in release mode
cargo build --release

# Run tests
cargo test

# Run with verbose output
RUST_LOG=debug cargo run
```

### Testing the DNS Server
```bash
# Start the server in background (with logging to heimdall.log)
./start_server.sh

# Stop the server
./stop_server.sh

# Test with a single DNS query
dig google.com @127.0.0.1 -p 1053

# Test iterative queries (use +norecurse instead of +trace due to port limitation)
dig google.com @127.0.0.1 -p 1053 +norecurse

# dig +trace now works with Heimdall (root zone query fix implemented)
dig google.com @127.0.0.1 -p 1053 +trace

# Test modern record types
dig cloudflare.com HTTPS @127.0.0.1 -p 1053  # HTTPS/SVCB records
dig example.com LOC @127.0.0.1 -p 1053       # Location records
dig example.com NAPTR @127.0.0.1 -p 1053     # Naming Authority Pointer
dig example.com SPF @127.0.0.1 -p 1053       # SPF records (usually TXT)

# Use the provided watch script for continuous testing
./watch.sh

# View server logs
tail -f heimdall.log
```

### Development Workflow
```bash
# Format code
cargo fmt

# Check for linting issues
cargo clippy

# Check if project compiles without building
cargo check

# Run a specific test
cargo test test_name

# Run performance regression tests
./scripts/check_performance.sh

# Create new performance baseline
./scripts/check_performance.sh --create-baseline

# Test configurable runtime (2 worker threads, 5 max concurrent queries)
HEIMDALL_WORKER_THREADS=2 HEIMDALL_MAX_CONCURRENT_QUERIES=5 cargo run

# Test cache persistence (saves cache to disk every 60 seconds, rkyv format)
HEIMDALL_CACHE_FILE_PATH="./heimdall_cache.rkyv" HEIMDALL_CACHE_SAVE_INTERVAL=60 cargo run

# Test with rate limiting enabled
HEIMDALL_ENABLE_RATE_LIMITING=true HEIMDALL_QUERIES_PER_SECOND_PER_IP=10 cargo run
```

## Architecture Overview

The codebase implements a production-ready DNS server with both UDP and TCP support:

### Core Structure
- **main.rs**: Concurrent UDP/TCP server loops using Tokio, binds to 127.0.0.1:1053
- **resolver.rs**: Full DNS resolution logic with upstream forwarding and caching integration
- **cache.rs**: Thread-safe DNS cache with TTL awareness and LRU eviction
- **config.rs**: Configuration management with environment variable support
- **dns/mod.rs**: Main DNSPacket structure with complete parsing and serialization
- **dns/header.rs**: DNS header with all standard fields and flags
- **dns/question.rs**: Question section with compression pointer support
- **dns/resource.rs**: Resource records with rdata parsing and reconstruction
- **dns/enums.rs**: Complete DNS record types (A, AAAA, CNAME, MX, TXT, etc.)
- **dns/common.rs**: Shared parsing utilities with compression pointer handling

### Current Features ✅
- **Complete DNS Resolution**: Forward queries to upstream servers (8.8.8.8, 1.1.1.1)
- **Dual Protocol Support**: Concurrent UDP and TCP listeners with automatic fallback
- **Intelligent Caching**: Thread-safe cache with TTL respect and zero-copy rkyv persistence
- **Compression Handling**: Full DNS compression pointer parsing and reconstruction
- **Enhanced Error Handling**: RFC-compliant REFUSED, NOTIMPL, FORMERR, and SERVFAIL responses
- **High Performance**: Sub-millisecond cached responses, zero-copy optimizations
- **Advanced Performance Features**: Query deduplication, connection pooling, parallel queries
- **Security & Validation**: Input validation, rate limiting (per-IP & global), attack detection
- **Health Monitoring**: Automatic failover with exponential backoff and health tracking
- **Configurable Runtime**: Custom Tokio thread pool with concurrency limiting
- **Regression Testing**: Automated performance benchmarking and regression detection
- **Protocol Compliance**: Proper DNS packet validation, opcode handling, and response generation
- **RFC 2308 Compliance**: Complete negative caching with SOA-based TTL handling
- **Modern Record Types**: Full parsing support for HTTPS/SVCB, LOC, NAPTR, DNAME, and SPF records
- **Distributed Systems**: Redis L2 cache, cluster coordination, aggregated metrics

### Packet Flow
1. **Receive**: UDP/TCP socket receives DNS query
2. **Parse**: Complete packet parsing with compression pointer support
3. **Validate**: Check opcode, validate query structure, apply security policies
4. **Error Check**: Return REFUSED/NOTIMPL/FORMERR for invalid queries
5. **Cache Check**: Check cache for existing valid responses (including negative cache)
6. **Resolve**: Forward to upstream servers if cache miss
7. **TCP Fallback**: Automatic TCP retry if UDP response truncated
8. **Parse Response**: Full response parsing with rdata reconstruction
9. **Cache Store**: Store response in cache with TTL awareness
10. **Send**: Return properly formatted response to client

### Key Implementation Details
- Uses `bitstream-io` for bit-level DNS packet manipulation
- Thread-safe caching with `DashMap` for concurrent access
- Proper DNS compression pointer parsing in both directions
- TCP length-prefixed message handling per RFC standards
- Configurable upstream servers and timeout settings
- Comprehensive logging with `tracing` crate

## Development Roadmap

### ✅ Phase 1: Core DNS Functionality (COMPLETED)
- [x] DNS packet parsing and serialization
- [x] UDP server implementation  
- [x] Basic DNS forwarding to upstream servers
- [x] Question and resource record handling
- [x] DNS header flag processing

### ✅ Phase 2: Advanced Features (COMPLETED)  
- [x] **Phase 2.1**: DNS Caching Layer
  - [x] Thread-safe in-memory cache with TTL awareness
  - [x] LRU eviction policy with configurable limits  
  - [x] Cache performance metrics and debugging
  - [x] Sub-millisecond cached response times
- [x] **Phase 2.2**: TCP Protocol Support
  - [x] TCP server with length-prefixed messages
  - [x] Automatic UDP to TCP fallback for truncated responses
  - [x] Concurrent UDP/TCP listeners
- [x] **Phase 2.3**: DNS Compression Fix
  - [x] Complete compression pointer parsing in rdata
  - [x] Proper response serialization with expanded domains
  - [x] Type-specific rdata reconstruction (MX, TXT, NS, etc.)

### ✅ Phase 3: Production Readiness (COMPLETED)
- [x] **Phase 3.1**: Security & Validation
  - [x] Input validation and query rate limiting (16 validation tests, per-IP & global rate limiting)
  - [x] Security hardening and attack detection patterns
  - [ ] DNSSEC support (signing and validation)
- [x] **Phase 3.2**: Advanced Reliability & Performance
  - [x] Connection pooling for upstream queries (5 connections per server)
  - [x] Persistent cache storage (zero-copy rkyv serialization, 83% smaller than JSON)
  - [x] Query pipeline optimization (query deduplication, parallel queries)
  - [x] Health monitoring & automatic failover (exponential backoff, health tracking)
- [x] **Phase 3.3**: Operational Features
  - [x] Metrics export (Prometheus format with comprehensive DNS server metrics)
  - [x] Health check endpoints (basic and detailed status via HTTP)
  - [x] Configuration hot-reloading (file watching + SIGHUP + HTTP endpoint)
  - [x] Graceful shutdown handling (coordinated shutdown of all server components)

### ✅ Phase 4: Enhanced DNS Features & RFC Compliance (COMPLETED)
- [x] **Phase 4.1**: Core RFC Compliance ✅ COMPLETED
  - [x] Complete negative caching (RFC 2308) with SOA-based TTL handling
  - [x] Enhanced error handling (REFUSED, NOTIMPL, FORMERR responses)
  - [x] Comprehensive DNS record type support (85 types)
  - [x] RDATA parsing for 23 critical and modern types (27% coverage)
  - [x] Opcode validation with proper error responses
  - [x] Extended RCODE support (all RFC-defined codes)
  - [x] UDP truncation support with TC flag (RFC 1035)

### 🚀 Phase 5: Advanced DNS Features (NEXT FOCUS)
- [ ] **Phase 5.1**: DNSSEC Validation
  - [ ] Signature validation implementation
  - [ ] Chain of trust verification
  - [ ] Trust anchor management
- [ ] **Phase 5.2**: Authoritative DNS Support
  - [ ] Zone file parsing and serving
  - [ ] SOA record management
  - [ ] Dynamic zone updates
- [ ] **Phase 5.3**: Modern Transport
  - [ ] DNS-over-TLS (DoT)
  - [ ] DNS-over-HTTPS (DoH)

## Development Reminders
- Whenever we complete any major steps, commit and push to git
- All core DNS functionality is now complete and fully tested
- Server handles both UDP and TCP with proper compression support
- Caching provides excellent performance with sub-ms response times
- RFC compliance is a priority - enhanced error handling and negative caching complete
- Modern DNS record types (HTTPS/SVCB, LOC, NAPTR, DNAME, SPF) are fully supported
- Distributed systems features (Redis L2 cache, cluster coordination) are operational
- Always run `cargo fmt` and `cargo clippy` before committing