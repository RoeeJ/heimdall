# Heimdall DNS Server Roadmap

## Vision
Transform Heimdall into a high-performance, adblocking DNS server with custom domain management capabilities, suitable for home labs and small networks.

## Phase 1: Core DNS Functionality (Foundation)
**Goal**: Implement a fully functional DNS resolver

### 1.1 Basic Resolution
- [ ] Implement upstream DNS query forwarding
- [ ] Add configurable upstream DNS servers (e.g., 1.1.1.1, 8.8.8.8)
- [ ] Support for multiple upstream servers with fallback
- [ ] Implement proper DNS response generation
- [ ] Fix the `valid()` method to properly validate packets
- [ ] Add comprehensive error handling

### 1.2 Protocol Support
- [ ] Complete implementation of all common DNS record types (A, AAAA, CNAME, MX, TXT, SOA, NS)
- [ ] Add support for EDNS0 (Extended DNS)
- [ ] Implement DNS compression for responses
- [ ] Add TCP support (required for large responses)
- [ ] Implement proper TTL handling

### 1.3 Testing & Reliability
- [ ] Unit tests for all DNS packet components
- [ ] Integration tests with real DNS queries
- [ ] Stress testing framework
- [ ] Logging system with configurable levels
- [ ] Metrics collection (query count, response times, errors)

## Phase 2: Performance Optimization
**Goal**: Achieve high-performance suitable for production use

### 2.1 Caching Layer
- [ ] Implement in-memory DNS cache with TTL respect
- [ ] Add cache hit/miss metrics
- [ ] Configurable cache size limits
- [ ] Cache persistence option (save/restore on restart)
- [ ] Negative caching for NXDOMAIN responses

### 2.2 Performance Enhancements
- [ ] Connection pooling for upstream queries
- [ ] Parallel upstream queries for redundancy
- [ ] Query deduplication (coalesce identical concurrent queries)
- [ ] Optimized data structures for domain lookups
- [ ] Zero-copy packet handling where possible
- [ ] SIMD optimizations for packet parsing

### 2.3 Scalability
- [ ] Multi-threaded packet processing
- [ ] Load balancing across CPU cores
- [ ] Configurable worker thread pool
- [ ] Benchmark suite for performance regression testing

## Phase 3: Adblocking Features
**Goal**: Implement efficient adblocking with minimal performance impact

### 3.1 Blocklist Management
- [ ] Support for multiple blocklist formats (hosts, domains, AdBlock syntax)
- [ ] Automatic blocklist downloading and updates
- [ ] Blocklist compilation into efficient data structures
- [ ] Support for popular lists (EasyList, EasyPrivacy, etc.)
- [ ] Allowlist support for exceptions

### 3.2 Blocking Engine
- [ ] Efficient domain matching using tries or bloom filters
- [ ] Wildcard domain blocking (*.doubleclick.net)
- [ ] Regex pattern support for advanced blocking
- [ ] CNAME cloaking detection and blocking
- [ ] Configurable blocking response (NXDOMAIN, 0.0.0.0, custom)

### 3.3 Analytics
- [ ] Blocked query statistics
- [ ] Per-client blocking metrics
- [ ] Top blocked domains dashboard
- [ ] Query log with filtering capabilities

## Phase 4: Custom Domain Management
**Goal**: Support for local/custom domains

### 4.1 Local DNS Records
- [ ] Configuration file for custom DNS records
- [ ] Support for common local TLDs (.local, .lan, .lab, .home)
- [ ] Dynamic record management API
- [ ] Wildcard domain support
- [ ] Reverse DNS (PTR) records for local IPs

### 4.2 Service Discovery
- [ ] mDNS/Bonjour compatibility
- [ ] SRV record support for services
- [ ] Integration with Docker/Kubernetes for container discovery
- [ ] DHCP integration for automatic hostname registration

### 4.3 Split-Horizon DNS
- [ ] Different responses based on client IP
- [ ] Internal vs external domain resolution
- [ ] VPN client detection and routing

## Phase 5: Management & Monitoring
**Goal**: Production-ready management interface

### 5.1 Configuration Management
- [ ] YAML/TOML configuration file support
- [ ] Hot-reload configuration without restart
- [ ] Configuration validation
- [ ] Environment variable support

### 5.2 API & Web Interface
- [ ] REST API for management
- [ ] Real-time WebSocket updates
- [ ] Web dashboard for monitoring
- [ ] Query log viewer
- [ ] Blocklist management UI
- [ ] Custom domain management UI

### 5.3 Integration
- [ ] Prometheus metrics export
- [ ] Grafana dashboard templates
- [ ] Syslog support
- [ ] Docker image with multi-arch support
- [ ] Kubernetes Helm chart
- [ ] SystemD service files

## Phase 6: Advanced Features
**Goal**: Enterprise-grade features

### 6.1 Security
- [ ] DNSSEC validation
- [ ] DNS-over-HTTPS (DoH) support
- [ ] DNS-over-TLS (DoT) support
- [ ] Rate limiting and DDoS protection
- [ ] Query source IP validation

### 6.2 High Availability
- [ ] Primary/secondary server synchronization
- [ ] Distributed caching with Redis
- [ ] Health check endpoints
- [ ] Automatic failover support

### 6.3 Advanced Filtering
- [ ] Time-based blocking rules
- [ ] Client-specific blocking policies
- [ ] Parental control features
- [ ] Malware domain blocking
- [ ] AI-based threat detection

## Implementation Strategy

### Priority Order
1. **Phase 1** - Without basic DNS functionality, nothing else matters
2. **Phase 2** - Performance is critical for a DNS server
3. **Phase 3** - Core differentiating feature (adblocking)
4. **Phase 4** - Essential for home lab use cases
5. **Phase 5** - Required for production deployment
6. **Phase 6** - Nice-to-have advanced features

### Technology Choices
- **Async Runtime**: Continue with Tokio for high concurrency
- **Web Framework**: Consider Axum or Actix for API/Web UI
- **Cache Storage**: In-memory with optional Redis backend
- **Configuration**: TOML for human-friendly config files
- **Metrics**: prometheus-rust for metrics export
- **Logging**: tracing crate for structured logging

### Performance Targets
- < 1ms average resolution time for cached queries
- < 50ms for upstream queries
- Support for 10,000+ queries per second on modest hardware
- Memory usage < 100MB for 1 million cached entries
- Blocklist loading < 5 seconds for 1 million domains

### Testing Strategy
- Unit tests for all components
- Integration tests with real DNS infrastructure
- Benchmark suite for performance tracking
- Chaos testing for reliability
- Security audit before v1.0 release