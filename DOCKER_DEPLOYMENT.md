# Heimdall DNS Server - Docker Deployment Guide

This guide provides comprehensive instructions for deploying the Heimdall DNS server using Docker in various environments.

## Quick Start

### 1. Build and Run (Development)
```bash
# Build the image
docker build -t heimdall:latest .

# Run with development settings
docker run -d \
  --name heimdall-dns \
  -p 1053:1053/udp \
  -p 1053:1053/tcp \
  heimdall:latest
```

### 2. Using Docker Compose (Recommended)
```bash
# Development deployment
docker-compose up -d heimdall-dev

# Production deployment
docker-compose up -d heimdall-prod

# Standard deployment
docker-compose up -d heimdall
```

### 3. Test the DNS Server
```bash
# Test basic functionality
dig google.com @127.0.0.1 -p 1053

# Test with specific record types
dig AAAA google.com @127.0.0.1 -p 1053
dig MX gmail.com @127.0.0.1 -p 1053
```

## Docker Image Variants

### 1. Distroless Image (Default - Recommended for Production)
- **Target**: `runtime`
- **Base**: `gcr.io/distroless/cc-debian12:latest`
- **Size**: ~10-15MB
- **Security**: Minimal attack surface
- **Use Case**: Production deployments

```bash
docker build --target runtime -t heimdall:distroless .
```

### 2. Debian Slim Image (Development/Debugging)
- **Target**: `runtime-debian`
- **Base**: `debian:12-slim`
- **Size**: ~25-35MB
- **Features**: Includes debugging tools (dig, bash)
- **Use Case**: Development, debugging, troubleshooting

```bash
docker build --target runtime-debian -t heimdall:debian .
```

## Configuration

### Environment Variables

All configuration is done through environment variables:

#### Core Settings
- `HEIMDALL_BIND_ADDR`: Address to bind to (default: `127.0.0.1:1053`)
- `HEIMDALL_UPSTREAM_SERVERS`: Comma-separated upstream DNS servers
- `RUST_LOG`: Logging level (e.g., `heimdall=debug`, `heimdall=info`)

#### Performance Tuning
- `HEIMDALL_WORKER_THREADS`: Number of worker threads (0 = auto-detect)
- `HEIMDALL_BLOCKING_THREADS`: Number of blocking threads (default: 512)
- `HEIMDALL_MAX_CONCURRENT_QUERIES`: Max concurrent queries (default: 10000)

#### DNS Resolution
- `HEIMDALL_UPSTREAM_TIMEOUT`: Timeout for upstream queries in seconds
- `HEIMDALL_MAX_RETRIES`: Maximum retries for failed queries
- `HEIMDALL_ENABLE_ITERATIVE`: Enable iterative DNS resolution
- `HEIMDALL_MAX_ITERATIONS`: Maximum iterations for iterative queries

#### Caching
- `HEIMDALL_ENABLE_CACHING`: Enable response caching
- `HEIMDALL_MAX_CACHE_SIZE`: Maximum cache entries
- `HEIMDALL_DEFAULT_TTL`: Default TTL for cached responses

### Example Configurations

#### High-Performance Production
```bash
docker run -d \
  --name heimdall-prod \
  --restart unless-stopped \
  -p 53:1053/udp \
  -p 53:1053/tcp \
  -e HEIMDALL_BIND_ADDR=0.0.0.0:1053 \
  -e HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53,8.8.4.4:53,1.0.0.1:53" \
  -e HEIMDALL_WORKER_THREADS=0 \
  -e HEIMDALL_MAX_CONCURRENT_QUERIES=50000 \
  -e HEIMDALL_MAX_CACHE_SIZE=100000 \
  -e HEIMDALL_ENABLE_CACHING=true \
  -e RUST_LOG=heimdall=info \
  --memory=2g \
  --cpus=4 \
  heimdall:latest
```

#### Development/Testing
```bash
docker run -d \
  --name heimdall-dev \
  -p 1053:1053/udp \
  -p 1053:1053/tcp \
  -e HEIMDALL_BIND_ADDR=0.0.0.0:1053 \
  -e HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53" \
  -e HEIMDALL_WORKER_THREADS=2 \
  -e HEIMDALL_MAX_CONCURRENT_QUERIES=1000 \
  -e RUST_LOG=heimdall=debug \
  heimdall:debian
```

## Production Deployment

### 1. Resource Requirements

#### Minimum Requirements
- **Memory**: 128MB
- **CPU**: 0.5 cores
- **Storage**: 100MB

#### Recommended for Production
- **Memory**: 512MB - 2GB
- **CPU**: 2-4 cores
- **Storage**: 1GB (for logs)

### 2. Security Considerations

#### Non-Root User
The container runs as a non-root user (UID 1001) by default.

#### Read-Only Filesystem
```bash
docker run --read-only \
  --tmpfs /tmp \
  heimdall:latest
```

#### Capability Dropping
```bash
docker run \
  --cap-drop ALL \
  --cap-add NET_BIND_SERVICE \
  --security-opt no-new-privileges:true \
  heimdall:latest
```

### 3. Health Checks

#### Built-in Health Check
The Dockerfile includes health checks that verify the DNS server is responding.

#### Custom Health Check
```bash
# Test health using dig (requires debian image)
docker run --health-cmd="dig @localhost -p 1053 google.com +time=5" heimdall:debian

# Simple port check (works with distroless)
docker run --health-cmd="timeout 3 bash -c '</dev/tcp/localhost/1053'" heimdall:latest
```

### 4. Logging and Monitoring

#### Container Logs
```bash
# View logs
docker logs -f heimdall-dns

# Structured logging with timestamp
docker logs --timestamps heimdall-dns
```

#### Log Levels
- `RUST_LOG=error`: Only errors
- `RUST_LOG=warn`: Warnings and errors
- `RUST_LOG=info`: General information (recommended for production)
- `RUST_LOG=debug`: Detailed debugging
- `RUST_LOG=trace`: Very verbose (development only)

## Deployment Scenarios

### 1. Docker Swarm
```yaml
version: '3.8'
services:
  heimdall:
    image: heimdall:latest
    ports:
      - "53:1053/udp"
      - "53:1053/tcp"
    deploy:
      replicas: 3
      resources:
        limits:
          memory: 512M
          cpus: '1.0'
      placement:
        constraints:
          - node.role == worker
```

### 2. Kubernetes
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: heimdall-dns
spec:
  replicas: 3
  selector:
    matchLabels:
      app: heimdall-dns
  template:
    metadata:
      labels:
        app: heimdall-dns
    spec:
      containers:
      - name: heimdall
        image: heimdall:latest
        ports:
        - containerPort: 1053
          protocol: UDP
        - containerPort: 1053
          protocol: TCP
        env:
        - name: HEIMDALL_BIND_ADDR
          value: "0.0.0.0:1053"
        - name: RUST_LOG
          value: "heimdall=info"
        resources:
          limits:
            memory: "512Mi"
            cpu: "1000m"
          requests:
            memory: "128Mi"
            cpu: "100m"
```

### 3. Docker Compose with Load Balancing
```yaml
version: '3.8'
services:
  heimdall:
    image: heimdall:latest
    deploy:
      replicas: 3
    environment:
      HEIMDALL_BIND_ADDR: "0.0.0.0:1053"
      RUST_LOG: "heimdall=info"
  
  nginx:
    image: nginx:alpine
    ports:
      - "53:53/udp"
    # Custom nginx config for UDP load balancing
```

## Troubleshooting

### 1. Common Issues

#### Permission Denied
```bash
# Ensure the container can bind to the port
docker run --privileged heimdall:latest
# Or use a non-privileged port
-e HEIMDALL_BIND_ADDR=0.0.0.0:1053
```

#### DNS Resolution Fails
```bash
# Check upstream server connectivity
docker exec heimdall-dns dig @1.1.1.1 google.com

# Check logs for errors
docker logs heimdall-dns
```

#### High Memory Usage
```bash
# Reduce cache size
-e HEIMDALL_MAX_CACHE_SIZE=1000

# Limit concurrent queries
-e HEIMDALL_MAX_CONCURRENT_QUERIES=1000
```

### 2. Debugging

#### Access Container Shell (Debian image only)
```bash
docker exec -it heimdall-dns bash
```

#### Network Debugging
```bash
# Check if ports are accessible
docker run --rm --network container:heimdall-dns nicolaka/netshoot netstat -ln

# Test DNS resolution
docker run --rm --network container:heimdall-dns nicolaka/netshoot dig @localhost -p 1053 google.com
```

### 3. Performance Testing

#### Stress Testing
```bash
# Build with stress test binary
docker build -t heimdall:test .

# Run stress test
docker run --rm \
  --network container:heimdall-dns \
  heimdall:test \
  /usr/local/bin/stress_test
```

## Best Practices

1. **Use multi-stage builds** to minimize image size
2. **Run as non-root user** for security
3. **Set resource limits** to prevent resource exhaustion
4. **Use health checks** for container orchestration
5. **Enable logging** with appropriate log levels
6. **Monitor performance** and adjust configuration as needed
7. **Use secrets management** for sensitive configuration
8. **Implement proper backup** for cache persistence if needed
9. **Update base images regularly** for security patches
10. **Test thoroughly** before production deployment