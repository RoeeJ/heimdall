#!/bin/bash
# Build Heimdall Docker image ensuring fresh compilation

echo "🔨 Building Heimdall Docker image..."

# Remove any old images to ensure fresh build
docker rmi heimdall:latest heimdall:debian 2>/dev/null || true

# Build with Debian runtime (recommended)
echo "📦 Building with Debian runtime..."
docker build --no-cache --target runtime-debian -t heimdall:debian -t heimdall:latest .

# Optional: Build distroless version
# echo "📦 Building distroless version..."
# docker build --no-cache --target runtime -t heimdall:distroless .

echo "✅ Build complete!"
echo ""
echo "🚀 To run the container:"
echo "docker run -d --name heimdall-dns -p 1053:1053/udp -p 1053:1053/tcp heimdall:latest"
echo ""
echo "🧪 To test:"
echo "dig google.com @localhost -p 1053"