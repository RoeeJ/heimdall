# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Heimdall is a DNS server implementation in Rust that listens on port 1053 and processes DNS queries. Currently in early development with foundational packet parsing implemented but no actual DNS resolution logic.

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

# Test iterative queries (dig +trace)
dig +trace google.com @127.0.0.1 -p 1053

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
```

## Architecture Overview

The codebase implements a UDP-based DNS server with the following key components:

### Core Structure
- **main.rs**: Async server loop using Tokio, binds to 127.0.0.1:1053
- **dns/mod.rs**: Main DNSPacket structure and parsing logic
- **dns/header.rs**: DNS header with standard fields (ID, flags, record counts)
- **dns/question.rs**: Question section parsing (domain labels, query type/class)
- **dns/resource.rs**: Resource record structures for answers/authorities/additional
- **dns/enums.rs**: Comprehensive DNS record types (A, AAAA, CNAME, etc.) and classes

### Packet Flow
1. UDP socket receives DNS query packet
2. Packet is parsed into DNSPacket structure
3. Domain names are extracted and printed (stub implementation)
4. Minimal response is generated (only sets QR and RA flags)
5. Raw packet is saved to packet.bin for debugging

### Key Implementation Details
- Uses `bitstream-io` for bit-level packet manipulation
- Implements proper DNS label parsing with length-prefixed strings
- Custom ParseError enum for parsing failures
- Currently no actual DNS resolution - responses are stubs
- The `valid()` method always returns false (needs implementation)

### Current Limitations
- No DNS resolution logic implemented
- No caching mechanism
- UDP only (no TCP support)
- No DNSSEC support
- Minimal error handling in response generation

## Development Reminders
- Whenever we complete any major steps, commit and push to git