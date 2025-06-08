#!/bin/bash

# Performance regression check script for Heimdall DNS
# Usage: ./scripts/check_performance.sh [--create-baseline] [--max-regression PERCENT]

set -e

# Default values
MAX_REGRESSION=10.0
ITERATIONS=1000
BASELINE_FILE="benchmarks/baseline.json"
CREATE_BASELINE=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --create-baseline)
            CREATE_BASELINE=true
            shift
            ;;
        --max-regression)
            MAX_REGRESSION="$2"
            shift 2
            ;;
        --iterations)
            ITERATIONS="$2"
            shift 2
            ;;
        --baseline-file)
            BASELINE_FILE="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --create-baseline      Create new performance baseline"
            echo "  --max-regression N     Maximum allowed regression percentage (default: 10.0)"
            echo "  --iterations N         Number of benchmark iterations (default: 1000)"
            echo "  --baseline-file FILE   Path to baseline file (default: benchmarks/baseline.json)"
            echo "  -h, --help            Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "🧪 Heimdall Performance Regression Check"
echo "========================================"
echo "Max regression threshold: ${MAX_REGRESSION}%"
echo "Iterations: ${ITERATIONS}"
echo "Baseline file: ${BASELINE_FILE}"
echo ""

# Build the regression test binary
echo "🔨 Building regression test binary..."
cargo build --release --bin regression_test

# Run the regression test
if [ "$CREATE_BASELINE" = true ]; then
    echo "📊 Creating new performance baseline..."
    ./target/release/regression_test \
        --create-baseline \
        --baseline "$BASELINE_FILE" \
        --iterations "$ITERATIONS"
    echo "✅ Baseline created successfully!"
else
    echo "🔍 Running performance regression tests..."
    ./target/release/regression_test \
        --baseline "$BASELINE_FILE" \
        --max-regression "$MAX_REGRESSION" \
        --iterations "$ITERATIONS"
    
    if [ $? -eq 0 ]; then
        echo ""
        echo "✅ Performance regression check PASSED!"
        echo "All benchmarks are within acceptable performance bounds."
    else
        echo ""
        echo "❌ Performance regression check FAILED!"
        echo "Some benchmarks have regressed beyond the acceptable threshold."
        exit 1
    fi
fi

echo ""
echo "📋 Performance Summary:"
echo "  - Test results are reproducible with: ./scripts/check_performance.sh"
echo "  - Create new baseline with: ./scripts/check_performance.sh --create-baseline"
echo "  - Adjust threshold with: ./scripts/check_performance.sh --max-regression 5.0"