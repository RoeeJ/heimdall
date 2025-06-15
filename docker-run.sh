#!/bin/bash
# Run Heimdall DNS server in Docker

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🚀 Starting Heimdall DNS Server in Docker${NC}"

# Check if container is already running
if docker ps | grep -q heimdall-dns; then
    echo -e "${YELLOW}⚠️  Container 'heimdall-dns' is already running${NC}"
    echo "Stop it with: docker stop heimdall-dns && docker rm heimdall-dns"
    exit 1
fi

# Remove old container if exists
if docker ps -a | grep -q heimdall-dns; then
    echo "Removing old container..."
    docker rm heimdall-dns
fi

# Run container with optimized cache enabled
echo -e "${GREEN}Starting container with optimized cache...${NC}"
docker run -d \
  --name heimdall-dns \
  --restart unless-stopped \
  -p 1053:1053/udp \
  -p 1053:1053/tcp \
  -v "$(pwd)/blocklists:/heimdall/blocklists:ro" \
  -v "$(pwd)/cache:/cache" \
  -e HEIMDALL_BIND_ADDR=0.0.0.0:1053 \
  -e HEIMDALL_UPSTREAM_SERVERS="1.1.1.1:53,8.8.8.8:53,8.8.4.4:53" \
  -e HEIMDALL_WORKER_THREADS=0 \
  -e HEIMDALL_BLOCKING_THREADS=512 \
  -e HEIMDALL_MAX_CONCURRENT_QUERIES=10000 \
  -e HEIMDALL_ENABLE_CACHING=true \
  -e HEIMDALL_MAX_CACHE_SIZE=10000 \
  -e HEIMDALL_DEFAULT_TTL=300 \
  -e HEIMDALL_USE_OPTIMIZED_CACHE=true \
  -e HEIMDALL_BLOCKING_ENABLED=true \
  -e HEIMDALL_BLOCKLIST_AUTO_UPDATE=false \
  -e RUST_LOG=heimdall=info \
  heimdall:latest

# Wait for container to start
echo "Waiting for container to start..."
sleep 3

# Check if container is running
if docker ps | grep -q heimdall-dns; then
    echo -e "${GREEN}✅ Container started successfully!${NC}"
    echo ""
    echo "📊 Container status:"
    docker ps --filter name=heimdall-dns --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
    echo ""
    echo "📝 View logs:"
    echo "  docker logs -f heimdall-dns"
    echo ""
    echo "🧪 Test DNS resolution:"
    echo "  dig google.com @localhost -p 1053"
    echo "  dig example.com @localhost -p 1053 +short"
    echo ""
    echo "🛑 Stop server:"
    echo "  docker stop heimdall-dns && docker rm heimdall-dns"
else
    echo -e "${RED}❌ Failed to start container${NC}"
    echo "Check logs with: docker logs heimdall-dns"
    exit 1
fi