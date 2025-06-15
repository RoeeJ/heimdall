# Docker Build and Run Guide

## Quick Start

### 1. Build the Docker Image

```bash
# Build with fresh compilation (recommended)
./build-docker.sh

# Or manually with docker command
docker build -t heimdall:latest .
```

### 2. Run the Container

```bash
# Use the provided script
./docker-run.sh

# Or with docker-compose
docker-compose up -d heimdall
```

## GLIBC Compatibility Issue Fix

If you encounter the error:
```
/usr/local/bin/heimdall: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found
```

This means the binary was compiled on a system with a newer GLIBC version than the container runtime. The solution is to compile inside Docker:

1. **Always build inside Docker**: The Dockerfile includes a build stage that compiles Heimdall with the correct GLIBC version
2. **Use Debian runtime**: The default runtime is now `runtime-debian` which has better compatibility
3. **Don't copy pre-built binaries**: Let Docker handle the compilation

## Build Targets

The Dockerfile supports multiple targets:

### Debian Runtime (Default - Recommended)
```bash
docker build -t heimdall:latest .
# or explicitly:
docker build --target runtime-debian -t heimdall:latest .
```
- Based on `debian:12-slim`
- Includes shell and debugging tools
- Better GLIBC compatibility
- Includes `dig` for health checks

### Distroless Runtime (Smaller but Limited)
```bash
docker build --target runtime -t heimdall:distroless .
```
- Based on `gcr.io/distroless/cc-debian12`
- Smaller image size
- No shell access
- May have GLIBC compatibility issues

## Running with Docker Compose

### Development Mode
```bash
docker-compose up heimdall-dev
```

### Production Mode
```bash
docker-compose up -d heimdall-prod
```

### Standard Mode
```bash
docker-compose up -d heimdall
```

## Environment Variables

Key configuration options:

```bash
# Enable optimized cache (Phase 2 performance improvements)
HEIMDALL_USE_OPTIMIZED_CACHE=true

# DNS Configuration
HEIMDALL_BIND_ADDR=0.0.0.0:1053
HEIMDALL_UPSTREAM_SERVERS=1.1.1.1:53,8.8.8.8:53

# Performance Tuning
HEIMDALL_WORKER_THREADS=0  # 0 = auto-detect CPU count
HEIMDALL_MAX_CONCURRENT_QUERIES=10000

# Caching
HEIMDALL_ENABLE_CACHING=true
HEIMDALL_MAX_CACHE_SIZE=10000

# Logging
RUST_LOG=heimdall=info
```

## Volume Mounts

```bash
# Blocklists (read-only)
-v $(pwd)/blocklists:/heimdall/blocklists:ro

# Cache persistence
-v $(pwd)/cache:/cache
```

## Testing the Container

```bash
# Basic DNS query
dig google.com @localhost -p 1053

# Check cache performance
dig example.com @localhost -p 1053 +stats

# View container logs
docker logs -f heimdall-dns

# Check container health
docker inspect heimdall-dns --format='{{.State.Health.Status}}'
```

## Troubleshooting

### Container fails to start
1. Check logs: `docker logs heimdall-dns`
2. Ensure ports aren't in use: `lsof -i :1053`
3. Verify image was built: `docker images | grep heimdall`

### GLIBC errors
1. Rebuild the image: `./build-docker.sh`
2. Use Debian runtime (default)
3. Don't use pre-compiled binaries
4. **IMPORTANT**: The binary MUST be compiled inside Docker, not copied from outside
5. If using CI/CD, ensure the Docker build uses the multi-stage Dockerfile

### Performance issues
1. Enable optimized cache: `HEIMDALL_USE_OPTIMIZED_CACHE=true`
2. Increase worker threads based on CPU cores
3. Adjust cache size based on memory

## CI/CD Integration

The GitHub Actions workflow now builds the Docker image using the multi-stage Dockerfile. This ensures:

1. **Binary Compilation Inside Docker**: The Rust binary is compiled within the Docker build environment
2. **GLIBC Compatibility**: The compilation environment matches the runtime environment
3. **No External Dependencies**: The CI doesn't need to build the binary separately

### Key Changes Made:
- Removed the step that builds the binary outside Docker
- Removed the step that creates a custom Dockerfile with pre-built binary
- Updated to use the existing multi-stage Dockerfile
- The Docker build now handles all compilation internally

### For Custom CI/CD:
If you're using a different CI/CD system, ensure:
```yaml
# Use the multi-stage Dockerfile
docker build -t heimdall:latest .

# Don't copy pre-built binaries
# Don't create custom Dockerfiles that bypass the build stage
```