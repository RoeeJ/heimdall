# Heimdall DNS Server

A high-performance, production-ready DNS server written in Rust with advanced caching, security features, and Kubernetes-native deployment support.

## Features

### Core DNS Functionality
- **Full DNS Protocol Support**: Complete implementation of DNS query/response handling
- **Dual Protocol**: Concurrent UDP and TCP listeners with automatic TCP fallback
- **DNS Compression**: Full compression pointer support for efficient packet handling
- **Multiple Record Types**: A, AAAA, CNAME, MX, TXT, NS, SOA, PTR, and more

### Performance & Caching
- **Two-Tier Cache Architecture**:
  - **L1 Cache**: Local in-memory cache with sub-millisecond response times
  - **L2 Cache**: Optional Redis backend for distributed caching across replicas
- **Intelligent Caching**: TTL-aware caching with LRU eviction
- **Cache Persistence**: Zero-copy rkyv serialization for cache snapshots
- **Query Optimization**: Deduplication, parallel queries, and connection pooling

### Security & Reliability
- **Rate Limiting**: Per-IP and global rate limiting with configurable thresholds
- **Input Validation**: Comprehensive DNS packet validation
- **Health Monitoring**: Automatic failover with exponential backoff
- **Attack Detection**: Protection against common DNS attacks
- **Graceful Shutdown**: Coordinated shutdown of all components
- **DNS Blocking**: Block unwanted domains with support for multiple blocklist formats

### Operations & Monitoring
- **Prometheus Metrics**: Comprehensive metrics export
- **Health Endpoints**: HTTP health checks and detailed status
- **Configuration Hot-Reload**: File watching, SIGHUP, and HTTP endpoint
- **Structured Logging**: Detailed tracing with configurable levels

## Quick Start

### Running Locally

```bash
# Build and run
cargo run

# Or run with custom configuration
RUST_LOG=debug cargo run

# Test the server
dig google.com @127.0.0.1 -p 1053
```

### Docker

```bash
# Build the image
docker build -t heimdall .

# Run the container
docker run -p 1053:1053/udp -p 1053:1053/tcp -p 8080:8080 heimdall

# Test
dig google.com @127.0.0.1 -p 1053
```

### Kubernetes with Helm

```bash
# Install with Helm (includes zero-downtime configuration by default)
helm install heimdall ./helm/heimdall

# Install with Redis enabled (default)
helm install heimdall ./helm/heimdall --set redis.enabled=true

# Install with custom blocklist storage size
helm install heimdall ./helm/heimdall --set blocklistPersistence.size=1Gi

# Get the external IP
kubectl get svc heimdall

# Test the DNS server
dig google.com @<EXTERNAL-IP>
```

#### Zero-Downtime Deployments

The Helm chart now includes production-ready defaults for zero-downtime deployments:

- **High Availability**: 3 replicas with pod anti-affinity
- **Smart Health Checks**: Readiness probes test actual DNS functionality (UDP & TCP)
- **Rolling Updates**: Never removes pods until replacements are ready
- **Session Affinity**: DNS queries stick to the same pod
- **Traffic Management**: Only routes to fully ready pods
- **Graceful Shutdown**: 60-second termination period

These settings ensure LoadBalancers (including MetalLB) only route traffic to healthy pods during deployments.

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `HEIMDALL_BIND_ADDR` | DNS server bind address | `0.0.0.0:1053` |
| `HEIMDALL_HTTP_BIND_ADDR` | HTTP metrics/health address | `0.0.0.0:8080` |
| `HEIMDALL_UPSTREAM_SERVERS` | Comma-separated upstream DNS servers | `8.8.8.8:53,1.1.1.1:53` |
| `HEIMDALL_MAX_CACHE_SIZE` | Maximum cache entries | `10000` |
| `HEIMDALL_ENABLE_CACHING` | Enable/disable caching | `true` |
| `HEIMDALL_CACHE_FILE_PATH` | Path for cache persistence | None |
| `HEIMDALL_ENABLE_RATE_LIMITING` | Enable rate limiting | `false` |
| `HEIMDALL_REDIS_URL` | Redis connection URL | Auto-detected |
| `HEIMDALL_WORKER_THREADS` | Tokio worker threads | `0` (auto) |
| `HEIMDALL_BLOCKING_ENABLED` | Enable DNS blocking | `true` |
| `HEIMDALL_BLOCKING_MODE` | Blocking mode (nxdomain/zero_ip/refused) | `zero_ip` |
| `HEIMDALL_BLOCKLISTS` | Blocklist files (path:format:name) | Default blocklists |

### Configuration File

Create a `heimdall.toml` file:

```toml
bind_addr = "0.0.0.0:1053"
http_bind_addr = "0.0.0.0:8080"

[upstream]
servers = ["8.8.8.8:53", "1.1.1.1:53", "8.8.4.4:53"]
timeout_ms = 2000
max_retries = 3

[cache]
max_size = 10000
default_ttl = 300
save_interval_secs = 300
file_path = "/tmp/heimdall_cache.rkyv"

[rate_limiting]
enabled = true
queries_per_second_per_ip = 100
global_queries_per_second = 10000

[performance]
worker_threads = 0
blocking_threads = 512
max_concurrent_queries = 1000
```

## Redis Integration

Heimdall supports Redis as an optional L2 cache for distributed deployments:

### Auto-Detection

Redis is automatically detected in the following order:
1. `HEIMDALL_REDIS_URL` environment variable
2. `REDIS_URL` environment variable
3. Kubernetes service discovery (`heimdall-redis` service)
4. Disabled if not detected

### Benefits

- **Shared Cache**: Improved hit rates across replicas
- **Persistence**: Cache survives pod restarts
- **Consistency**: Same cached data across all instances
- **Automatic Failover**: Falls back to local cache if Redis is unavailable

### Setup

#### Docker Compose
```yaml
version: '3.8'
services:
  heimdall:
    image: heimdall:latest
    environment:
      - HEIMDALL_REDIS_URL=redis://redis:6379
    depends_on:
      - redis
  
  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes
```

#### Kubernetes
Redis is included in the Helm chart by default. To disable:
```bash
helm install heimdall ./helm/heimdall --set redis.enabled=false
```

## Performance

### Benchmarks

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Cached Response (L1) | <0.1ms | >100K QPS |
| Cached Response (L2/Redis) | ~2ms | >50K QPS |
| Upstream Query | ~50ms | ~20K QPS |
| With Rate Limiting | +0.05ms | ~95% of baseline |

### Performance Tuning

```bash
# Run performance tests
cargo bench

# Check for regressions
./scripts/check_performance.sh

# Create new baseline
./scripts/check_performance.sh --create-baseline
```

## Development

### Prerequisites

- Rust 1.70+
- Docker (optional)
- Kubernetes cluster (optional)

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run tests in CI mode (no network)
./scripts/test-ci-mode.sh

# Run network-dependent tests
./scripts/test-network-mode.sh

# Run with verbose logging
RUST_LOG=heimdall=debug cargo run
```

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Security audit
cargo audit
```

### Git Hooks

We provide pre-commit hooks to ensure code quality:

```bash
# Set up git hooks (interactive)
./scripts/setup-git-hooks.sh

# Options available:
# 1. Full hook - runs fmt, clippy, build, and all tests
# 2. Fast hook - runs fmt, clippy, and compilation check only
# 3. Disable hooks
```

See [docs/git-hooks.md](docs/git-hooks.md) for detailed information about the available hooks.

## DNS Blocking

Heimdall includes powerful DNS blocking capabilities to filter unwanted domains at the network level.

### Quick Start

```bash
# Blocking is enabled by default with zero_ip mode
# To disable blocking:
HEIMDALL_BLOCKING_ENABLED=false cargo run

# To use a different blocking mode:
HEIMDALL_BLOCKING_MODE=nxdomain cargo run
```

### Default Blocklists

Heimdall comes with these blocklists enabled by default:
- **StevenBlack's Hosts**: Unified hosts file blocking ads, malware, and trackers
- **URLhaus Malware Domains**: Active malware domain blocking

These are automatically downloaded on first startup if auto-update is enabled (default).

### Blocking Modes

- **NXDOMAIN**: Return non-existent domain response
- **Zero IP**: Return 0.0.0.0 (A) or :: (AAAA)
- **Custom IP**: Return specified IP address
- **REFUSED**: Return DNS REFUSED response

### Supported Blocklist Formats

- Hosts files (0.0.0.0 ads.example.com)
- Domain lists (one per line)
- AdBlock Plus format
- Pi-hole format
- dnsmasq configuration
- Unbound local-zone format

See [DNS_BLOCKING.md](docs/DNS_BLOCKING.md) for detailed configuration.

## Architecture

```
┌─────────────────┐
│   DNS Client    │
└────────┬────────┘
         │ Query (UDP/TCP)
┌────────▼────────┐
│    Heimdall     │
│  ┌───────────┐  │
│  │ Blocking  │  │ ← Check blocklists
│  └─────┬─────┘  │
│  ┌─────▼─────┐  │
│  │   Cache   │  │ ← L1: Local Memory
│  │  Manager  │  │ ← L2: Redis (optional)
│  └─────┬─────┘  │
│        │        │
│  ┌─────▼─────┐  │
│  │ Resolver  │  │
│  └─────┬─────┘  │
└────────┼────────┘
         │ Forward
┌────────▼────────┐
│ Upstream DNS    │
│ (1.1.1.1, etc) │
└─────────────────┘
```

## Monitoring

### Prometheus Metrics

Access metrics at `http://<server>:8080/metrics`:

- `heimdall_dns_queries_total` - Total DNS queries
- `heimdall_dns_errors_total` - DNS errors by type
- `heimdall_cache_hits_total` - Cache hit count
- `heimdall_cache_size` - Current cache size
- `heimdall_upstream_response_time_seconds` - Upstream query latency
- `heimdall_blocked_queries_total` - Total blocked DNS queries
- `heimdall_blocked_domains_total` - Number of domains in blocklists

### Health Checks

- **Basic**: `GET /health` - Returns 200 if healthy
- **Detailed**: `GET /health/detailed` - Returns JSON with component status

## Security

### Best Practices

1. **Enable Rate Limiting** in production
2. **Use Redis Authentication** for distributed deployments
3. **Configure Network Policies** in Kubernetes
4. **Regular Security Updates** via dependabot
5. **Monitor Metrics** for anomalies

### Threat Model

- Protection against DNS amplification attacks
- Rate limiting prevents abuse
- Input validation prevents malformed packets
- Health checks enable automatic recovery

## Roadmap

See [ROADMAP.md](ROADMAP.md) for detailed development plans.

### Completed Features ✅
- Core DNS server with UDP/TCP
- Advanced caching with persistence
- Security hardening and rate limiting
- Redis integration for distributed caching
- Kubernetes-native deployment
- Performance optimization

### Future Plans 🚀
- DNSSEC support
- Authoritative DNS mode
- Full recursive resolution
- Advanced analytics

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and benchmarks
5. Submit a pull request

## License

MIT License - see LICENSE file for details

## Acknowledgments

- Built with Rust and Tokio for high performance
- Uses bitstream-io for DNS packet handling
- Redis integration powered by redis-rs
- Kubernetes deployment via Helm