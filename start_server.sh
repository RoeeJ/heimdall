#!/bin/bash
# Start Heimdall DNS server in background for testing

# Kill any existing Heimdall instances
pkill -f "target/debug/heimdall" 2>/dev/null

# Build the project
echo "Building Heimdall..."
cargo build

# Start the server in background with logs redirected
echo "Starting Heimdall DNS server..."
RUST_LOG=debug cargo run --bin heimdall > heimdall.log 2>&1 &
SERVER_PID=$!

# Wait a moment for server to start
sleep 2

# Check if server started successfully
if ps -p $SERVER_PID > /dev/null; then
    echo "Heimdall DNS server started successfully (PID: $SERVER_PID)"
    echo "Logs are being written to heimdall.log"
    echo "To stop the server, run: kill $SERVER_PID"
    echo $SERVER_PID > heimdall.pid
else
    echo "Failed to start Heimdall DNS server"
    exit 1
fi