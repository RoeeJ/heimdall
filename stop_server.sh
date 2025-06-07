#!/bin/bash
# Stop Heimdall DNS server

if [ -f heimdall.pid ]; then
    PID=$(cat heimdall.pid)
    if ps -p $PID > /dev/null; then
        echo "Stopping Heimdall DNS server (PID: $PID)..."
        kill $PID
        rm heimdall.pid
        echo "Server stopped"
    else
        echo "Server not running (stale PID file)"
        rm heimdall.pid
    fi
else
    # Try to find and kill by process name
    if pgrep -f "target/debug/heimdall" > /dev/null; then
        echo "Stopping Heimdall DNS server..."
        pkill -f "target/debug/heimdall"
        echo "Server stopped"
    else
        echo "Heimdall DNS server is not running"
    fi
fi