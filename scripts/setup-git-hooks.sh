#!/bin/bash

# Setup script for git hooks in Heimdall project

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Heimdall Git Hooks Setup${NC}"
echo "========================"
echo ""
echo "This script will help you set up pre-commit hooks for the Heimdall project."
echo ""
echo "Available options:"
echo -e "${GREEN}1)${NC} Full pre-commit hook (recommended for final commits)"
echo "   - Runs cargo fmt check"
echo "   - Runs clippy with all warnings as errors"
echo "   - Builds the project in release mode"
echo "   - Runs the full test suite"
echo ""
echo -e "${YELLOW}2)${NC} Fast pre-commit hook (for iterative development)"
echo "   - Runs cargo fmt check"
echo "   - Runs clippy with all warnings as errors"
echo "   - Runs cargo check (compilation only)"
echo "   - Skips full test suite for faster commits"
echo ""
echo -e "${RED}3)${NC} Disable pre-commit hooks"
echo "   - Removes any existing pre-commit hook"
echo ""

read -p "Please select an option (1-3): " choice

case $choice in
    1)
        echo -e "\n${GREEN}Setting up full pre-commit hook...${NC}"
        cp .git/hooks/pre-commit.fast .git/hooks/pre-commit.fast.backup 2>/dev/null || true
        cp .git/hooks/pre-commit .git/hooks/pre-commit.backup 2>/dev/null || true
        
        # Create the full pre-commit hook if it doesn't exist
        if [ ! -f .git/hooks/pre-commit ]; then
            cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash

# Pre-commit hook for Heimdall project
# Runs clippy and test suite before allowing commits

set -e

echo "🔍 Running pre-commit checks..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
    else
        echo -e "${RED}✗ $2${NC}"
    fi
}

# 1. Check if there are any changes to commit
if git diff --cached --quiet; then
    echo "No changes staged for commit"
    exit 0
fi

echo -e "\n${YELLOW}Step 1: Running cargo fmt check...${NC}"
if ! cargo fmt -- --check; then
    echo -e "${RED}Error: Code is not properly formatted${NC}"
    echo "Please run 'cargo fmt' to fix formatting issues"
    exit 1
fi
print_status 0 "Code formatting check passed"

echo -e "\n${YELLOW}Step 2: Running clippy...${NC}"
if ! cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    echo -e "${RED}Error: Clippy found issues${NC}"
    echo "Please fix the clippy warnings before committing"
    exit 1
fi
print_status 0 "Clippy check passed"

echo -e "\n${YELLOW}Step 3: Building the project...${NC}"
if ! cargo build --release; then
    echo -e "${RED}Error: Build failed${NC}"
    echo "Please ensure the project builds successfully before committing"
    exit 1
fi
print_status 0 "Build successful"

echo -e "\n${YELLOW}Step 4: Running test suite...${NC}"
if ! cargo test; then
    echo -e "${RED}Error: Tests failed${NC}"
    echo "Please ensure all tests pass before committing"
    exit 1
fi
print_status 0 "All tests passed"

echo -e "\n${GREEN}✅ All pre-commit checks passed!${NC}"
echo "Proceeding with commit..."
EOF
            chmod +x .git/hooks/pre-commit
        fi
        
        echo -e "${GREEN}✓ Full pre-commit hook installed!${NC}"
        echo -e "${YELLOW}Note: This will run the full test suite before each commit.${NC}"
        echo -e "${YELLOW}Use 'git commit --no-verify' to skip hooks if needed.${NC}"
        ;;
    
    2)
        echo -e "\n${YELLOW}Setting up fast pre-commit hook...${NC}"
        
        # Create the fast pre-commit hook
        cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash

# Fast pre-commit hook for Heimdall project
# Only runs clippy and quick checks (no full test suite)

set -e

echo "🚀 Running fast pre-commit checks..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
    else
        echo -e "${RED}✗ $2${NC}"
    fi
}

# 1. Check if there are any changes to commit
if git diff --cached --quiet; then
    echo "No changes staged for commit"
    exit 0
fi

echo -e "\n${YELLOW}Step 1: Running cargo fmt check...${NC}"
if ! cargo fmt -- --check; then
    echo -e "${RED}Error: Code is not properly formatted${NC}"
    echo "Please run 'cargo fmt' to fix formatting issues"
    exit 1
fi
print_status 0 "Code formatting check passed"

echo -e "\n${YELLOW}Step 2: Running clippy...${NC}"
if ! cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    echo -e "${RED}Error: Clippy found issues${NC}"
    echo "Please fix the clippy warnings before committing"
    exit 1
fi
print_status 0 "Clippy check passed"

echo -e "\n${YELLOW}Step 3: Checking if project compiles...${NC}"
if ! cargo check --workspace --all-targets; then
    echo -e "${RED}Error: Compilation check failed${NC}"
    echo "Please ensure the project compiles before committing"
    exit 1
fi
print_status 0 "Compilation check passed"

echo -e "\n${GREEN}✅ Fast pre-commit checks passed!${NC}"
echo -e "${YELLOW}Note: This is the fast mode - full test suite was skipped${NC}"
echo "Proceeding with commit..."
EOF
        chmod +x .git/hooks/pre-commit
        
        echo -e "${GREEN}✓ Fast pre-commit hook installed!${NC}"
        echo -e "${YELLOW}Note: Test suite will NOT run before commits.${NC}"
        echo -e "${YELLOW}Remember to run 'cargo test' manually before pushing.${NC}"
        ;;
    
    3)
        echo -e "\n${RED}Disabling pre-commit hooks...${NC}"
        if [ -f .git/hooks/pre-commit ]; then
            rm .git/hooks/pre-commit
            echo -e "${GREEN}✓ Pre-commit hook removed${NC}"
        else
            echo "No pre-commit hook found"
        fi
        ;;
    
    *)
        echo -e "${RED}Invalid option. Please run the script again and select 1, 2, or 3.${NC}"
        exit 1
        ;;
esac

echo ""
echo "Setup complete!"