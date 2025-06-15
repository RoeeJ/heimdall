# ==============================================================================
# Heimdall DNS Server - Production Dockerfile
# ==============================================================================
# This multi-stage Dockerfile builds a production-ready DNS server with
# optimized image size, security, and performance characteristics.
#
# Default target is 'runtime-debian' for better compatibility.

# ==============================================================================
# Build Stage - Use official Rust image with build tools
# ==============================================================================
FROM rustlang/rust:nightly-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user for building
RUN useradd -m -u 1001 appuser

# Set working directory
WORKDIR /app

# Copy dependency files first to leverage Docker layer caching
COPY Cargo.toml Cargo.lock ./
COPY benches/ ./benches/

# Create a dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/lib.rs

# Build dependencies only (this layer will be cached unless Cargo.toml changes)
RUN cargo build --release --bin heimdall && \
    rm -rf src target/release/deps/heimdall* target/release/heimdall*

# Copy the actual source code and assets
COPY src/ ./src/
COPY assets/ ./assets/

# Build the actual application with optimizations
ENV RUSTFLAGS="-C opt-level=3 -C codegen-units=1"
RUN cargo build --release --bin heimdall

# Strip the binary to reduce size
RUN strip target/release/heimdall

# ==============================================================================
# Runtime Stage - Use minimal distroless image
# ==============================================================================
FROM gcr.io/distroless/cc-debian12:latest AS runtime

# Copy CA certificates for upstream DNS resolution
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Create non-root user (distroless doesn't have useradd)
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

# Copy the binary from builder stage
COPY --from=builder /app/target/release/heimdall /usr/local/bin/heimdall

# Set user to non-root
USER 1001:1001

# ==============================================================================
# Runtime Configuration
# ==============================================================================

# Expose DNS ports (both UDP and TCP)
EXPOSE 1053/udp 1053/tcp

# Set environment variables for production defaults
ENV RUST_LOG=heimdall=info,warn
ENV HEIMDALL_BIND_ADDR=0.0.0.0:1053
ENV HEIMDALL_WORKER_THREADS=0
ENV HEIMDALL_BLOCKING_THREADS=512
ENV HEIMDALL_MAX_CONCURRENT_QUERIES=10000
ENV HEIMDALL_UPSTREAM_TIMEOUT=5
ENV HEIMDALL_ENABLE_CACHING=true
ENV HEIMDALL_MAX_CACHE_SIZE=10000
ENV HEIMDALL_DEFAULT_TTL=300

# Add health check (requires dig or nslookup to be added if needed)
# For now, we'll use a simple port check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD timeout 5 bash -c '</dev/tcp/localhost/1053' || exit 1

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/heimdall"]

# ==============================================================================
# Alternative Runtime Stage - Debian Slim (if distroless causes issues)
# ==============================================================================
FROM debian:12-slim AS runtime-debian

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    dnsutils \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user
RUN useradd -m -u 1001 -s /bin/bash appuser

# Create blocklist directory with correct permissions
RUN mkdir -p /heimdall/blocklists && chown -R appuser:appuser /heimdall

# Copy the binary from builder stage
COPY --from=builder /app/target/release/heimdall /usr/local/bin/heimdall

# Set ownership and permissions
RUN chown appuser:appuser /usr/local/bin/heimdall && \
    chmod +x /usr/local/bin/heimdall

# Set working directory
WORKDIR /heimdall

# Switch to non-root user
USER appuser

# Expose DNS ports
EXPOSE 1053/udp 1053/tcp

# Set environment variables
ENV RUST_LOG=heimdall=info,warn
ENV HEIMDALL_BIND_ADDR=0.0.0.0:1053
ENV HEIMDALL_WORKER_THREADS=0
ENV HEIMDALL_BLOCKING_THREADS=512
ENV HEIMDALL_MAX_CONCURRENT_QUERIES=10000
ENV HEIMDALL_UPSTREAM_TIMEOUT=5
ENV HEIMDALL_ENABLE_CACHING=true
ENV HEIMDALL_MAX_CACHE_SIZE=10000
ENV HEIMDALL_DEFAULT_TTL=300

# Health check using dig
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD dig @localhost -p 1053 google.com +time=5 > /dev/null || exit 1

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/heimdall"]

# ==============================================================================
# Build Instructions and Usage
# ==============================================================================

# To build with Debian slim (recommended - default):
# docker build -t heimdall:latest .

# To build with distroless (smaller image but may have GLIBC compatibility issues):
# docker build --target runtime -t heimdall:distroless .

# To run the container:
# docker run -d \
#   --name heimdall-dns \
#   -p 1053:1053/udp \
#   -p 1053:1053/tcp \
#   -e HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53" \
#   -e RUST_LOG=heimdall=debug \
#   heimdall:latest

# For production deployment with custom configuration:
# docker run -d \
#   --name heimdall-dns \
#   --restart unless-stopped \
#   -p 53:1053/udp \
#   -p 53:1053/tcp \
#   -e HEIMDALL_BIND_ADDR=0.0.0.0:1053 \
#   -e HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53,8.8.4.4:53" \
#   -e HEIMDALL_WORKER_THREADS=4 \
#   -e HEIMDALL_MAX_CONCURRENT_QUERIES=20000 \
#   -e HEIMDALL_ENABLE_CACHING=true \
#   -e RUST_LOG=heimdall=info \
#   --memory=512m \
#   --cpus=2 \
#   heimdall:latest

# ==============================================================================
# Default Stage - Use Debian runtime for better compatibility
# ==============================================================================
FROM runtime-debian