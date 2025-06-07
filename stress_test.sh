#!/bin/bash

# Heimdall DNS Server Stress Testing Script
# This script provides easy access to various stress testing scenarios

set -e

echo "🔥 Heimdall DNS Server Stress Testing Suite 🔥"
echo "==============================================="

# Function to check if server is running
check_server() {
    echo "Checking if DNS server is running on port 1053..."
    if ! nc -z 127.0.0.1 1053 2>/dev/null; then
        echo "❌ DNS server is not running on port 1053"
        echo "   Please start the server first with: ./start_server.sh"
        exit 1
    fi
    echo "✅ DNS server is running"
    echo
}

# Function to run a stress test scenario
run_scenario() {
    local scenario=$1
    local description=$2
    
    echo "🧪 Running $scenario stress test"
    echo "   $description"
    echo "   Starting test..."
    echo
    
    cargo run --bin stress_test -- --scenario "$scenario" --edns
    
    echo
    echo "✅ $scenario test completed"
    echo "----------------------------------------"
    echo
}

# Function to run custom stress test
run_custom() {
    echo "🧪 Running custom stress test with parameters:"
    echo "   $@"
    echo
    
    cargo run --bin stress_test -- "$@"
    
    echo
    echo "✅ Custom test completed"
    echo "----------------------------------------"
    echo
}

# Function to show help
show_help() {
    echo "Usage: $0 [OPTIONS] [SCENARIO]"
    echo
    echo "SCENARIOS:"
    echo "  light      - Light load test (5 clients, 100 queries)"
    echo "  medium     - Medium load test (20 clients, 1,000 queries)"  
    echo "  heavy      - Heavy load test (50 clients, 5,000 queries)"
    echo "  extreme    - Extreme load test (100 clients, 10,000 queries)"
    echo "  endurance  - Endurance test (25 clients, 50,000 queries)"
    echo "  all        - Run all predefined scenarios"
    echo "  custom     - Run with custom parameters (pass additional args)"
    echo
    echo "OPTIONS:"
    echo "  --no-server-check  Skip server availability check"
    echo "  --help, -h         Show this help message"
    echo
    echo "CUSTOM PARAMETERS (use with 'custom' scenario):"
    echo "  -c, --clients <N>         Number of concurrent clients"
    echo "  -q, --queries <N>         Total number of queries"
    echo "  -s, --server <ADDR:PORT>  Target server address"
    echo "  -t, --timeout <SECS>      Query timeout in seconds"
    echo "  --edns                    Enable EDNS in queries"
    echo "  --buffer-size <BYTES>     EDNS buffer size"
    echo "  --query-types <TYPES>     Comma-separated query types (A,AAAA,MX,etc.)"
    echo
    echo "EXAMPLES:"
    echo "  $0 light                              # Run light scenario"
    echo "  $0 all                                # Run all scenarios"
    echo "  $0 custom -c 30 -q 2000 --edns       # Custom test"
    echo "  $0 custom --query-types A,MX,NS      # Test specific record types"
}

# Parse command line arguments
SKIP_SERVER_CHECK=false
SCENARIO=""
CUSTOM_ARGS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            show_help
            exit 0
            ;;
        --no-server-check)
            SKIP_SERVER_CHECK=true
            shift
            ;;
        light|medium|heavy|extreme|endurance|all|custom)
            SCENARIO=$1
            shift
            ;;
        *)
            CUSTOM_ARGS+=("$1")
            shift
            ;;
    esac
done

# Default scenario if none provided
if [[ -z "$SCENARIO" ]]; then
    echo "No scenario specified. Available scenarios:"
    echo "  light, medium, heavy, extreme, endurance, all, custom"
    echo
    echo "Use --help for more information."
    exit 1
fi

# Check if server is running (unless skipped)
if [[ "$SKIP_SERVER_CHECK" != true ]]; then
    check_server
fi

# Build the stress test binary
echo "🔨 Building stress test binary..."
cargo build --bin stress_test
echo "✅ Build completed"
echo

# Run the requested scenario
case $SCENARIO in
    light)
        run_scenario "light" "Quick smoke test with minimal load"
        ;;
    medium)
        run_scenario "medium" "Moderate load to test basic performance"
        ;;
    heavy)
        run_scenario "heavy" "High load to test server limits"
        ;;
    extreme)
        run_scenario "extreme" "Maximum load to test breaking points"
        ;;
    endurance)
        run_scenario "endurance" "Long-running test to check stability"
        ;;
    all)
        echo "🧪 Running complete stress test suite..."
        echo "This will run all predefined scenarios in sequence."
        echo
        
        run_scenario "light" "Quick smoke test with minimal load"
        run_scenario "medium" "Moderate load to test basic performance"
        run_scenario "heavy" "High load to test server limits"
        run_scenario "extreme" "Maximum load to test breaking points"
        
        echo "🎉 All stress tests completed!"
        echo "Check the results above to analyze server performance."
        ;;
    custom)
        if [[ ${#CUSTOM_ARGS[@]} -eq 0 ]]; then
            echo "❌ Custom scenario requires additional parameters"
            echo "   Example: $0 custom -c 30 -q 2000 --edns"
            echo "   Use --help for parameter details"
            exit 1
        fi
        run_custom "${CUSTOM_ARGS[@]}"
        ;;
    *)
        echo "❌ Unknown scenario: $SCENARIO"
        echo "Available scenarios: light, medium, heavy, extreme, endurance, all, custom"
        exit 1
        ;;
esac

echo "🎯 Stress testing completed!"
echo "   Scenario: $SCENARIO"
echo "   Check the detailed results above for performance analysis."