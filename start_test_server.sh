#!/bin/bash

# Start Heimdall in background for testing
export HEIMDALL_ZONE_FILES=zones/example.com.zone
export HEIMDALL_AUTHORITATIVE_ENABLED=true
export HEIMDALL_BLOCKING_ENABLED=false

# Kill any existing Heimdall process
pkill -f "target/debug/heimdall" 2>/dev/null

# Start in background
cargo run > /tmp/heimdall_test.log 2>&1 &
echo $! > /tmp/heimdall_test.pid

echo "Heimdall started with PID $(cat /tmp/heimdall_test.pid)"
echo "Waiting for server to start..."
sleep 2

echo "Server logs:"
tail -n 20 /tmp/heimdall_test.log