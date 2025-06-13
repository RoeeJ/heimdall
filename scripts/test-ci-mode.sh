#!/bin/bash

# Run tests with CI configuration to simulate CI environment locally
# This helps catch issues before pushing to CI

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Running tests in CI mode...${NC}"
echo "This simulates the CI environment by disabling network-related features"
echo ""

# Export CI environment variables
export HEIMDALL_BLOCKING_ENABLED=false
export HEIMDALL_BLOCKING_DOWNLOAD_PSL=false
export HEIMDALL_BLOCKLIST_AUTO_UPDATE=false
export RUST_BACKTRACE=1
export SKIP_INTEGRATION_TESTS=1

# Function to run tests with timing
run_test_suite() {
    local name=$1
    local cmd=$2
    
    echo -e "\n${YELLOW}Running $name...${NC}"
    
    start_time=$(date +%s)
    
    if eval "$cmd"; then
        end_time=$(date +%s)
        duration=$((end_time - start_time))
        echo -e "${GREEN}✓ $name passed (${duration}s)${NC}"
        return 0
    else
        echo -e "${RED}✗ $name failed${NC}"
        return 1
    fi
}

# Track overall success
all_passed=true

# 1. Format check
if ! run_test_suite "Format check" "cargo fmt --all -- --check"; then
    echo -e "${YELLOW}Tip: Run 'cargo fmt' to fix formatting issues${NC}"
    all_passed=false
fi

# 2. Clippy
if ! run_test_suite "Clippy" "cargo clippy --workspace --all-targets --all-features -- -D warnings"; then
    all_passed=false
fi

# 3. Build check
if ! run_test_suite "Build check" "cargo check --workspace --all-targets --all-features"; then
    all_passed=false
fi

# 4. Unit tests (excluding ignored)
if ! run_test_suite "Unit tests" "cargo test --workspace --all-features"; then
    all_passed=false
fi

# 5. Doc tests
if ! run_test_suite "Doc tests" "cargo test --doc --all-features"; then
    all_passed=false
fi

# List ignored tests
echo -e "\n${YELLOW}Tests requiring network access (skipped in CI):${NC}"
cargo test --workspace -- --ignored --list 2>/dev/null | grep -E "test::" | sed 's/^/  - /' || echo "  No ignored tests found"

# Summary
echo -e "\n${YELLOW}=== CI Mode Test Summary ===${NC}"
if [ "$all_passed" = true ]; then
    echo -e "${GREEN}✅ All CI tests passed!${NC}"
    echo "Your code should pass CI checks."
    
    echo -e "\n${YELLOW}Optional: Run network tests locally:${NC}"
    echo "  ./scripts/test-network-mode.sh"
    
    exit 0
else
    echo -e "${RED}❌ Some CI tests failed${NC}"
    echo "Please fix the issues before pushing to avoid CI failures."
    
    exit 1
fi