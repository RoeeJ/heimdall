#!/bin/bash

# Technical Debt Monitoring Script for Heimdall DNS Server
# Automatically scans for common technical debt patterns

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEBT_DIR="${PROJECT_ROOT}/notes/tech-debt"
SRC_DIR="${PROJECT_ROOT}/src"

echo "🔍 Heimdall Technical Debt Monitor"
echo "=================================="
echo "Project: ${PROJECT_ROOT}"
echo "Generated: $(date)"
echo ""

# Function to count and report patterns
report_pattern() {
    local pattern="$1"
    local description="$2"
    local severity="$3"
    local files
    
    files=$(rg -l "${pattern}" "${SRC_DIR}" --type rust 2>/dev/null || true)
    local count
    count=$(echo "$files" | grep -c . || echo "0")
    
    if [ "$count" -gt 0 ]; then
        echo "${severity} ${description}: ${count} occurrences"
        if [ "$count" -le 10 ]; then
            # Show specific locations for small counts
            rg -n "${pattern}" "${SRC_DIR}" --type rust 2>/dev/null | head -20 | sed 's/^/    /'
        else
            echo "    Files: $(echo "$files" | wc -l) files affected"
            echo "$files" | head -5 | sed 's/^/    /'
            if [ "$count" -gt 5 ]; then
                echo "    ... and $((count - 5)) more files"
            fi
        fi
        echo ""
    fi
}

# Safety Issues
echo "🔴 CRITICAL SAFETY ISSUES"
echo "========================"

report_pattern "\.unwrap\(\)" "Unwrap calls (panic risk)" "🔴"
report_pattern "\.expect\([^)]*\)" "Expect calls (check context)" "🟡"
report_pattern "panic!" "Explicit panics" "🔴"
report_pattern "unreachable!" "Unreachable code" "🟡"

# Error Handling
echo "🟡 ERROR HANDLING PATTERNS"
echo "========================="

report_pattern "Box<dyn.*Error" "Generic error types" "🟡"
report_pattern "unwrap_or\(" "Error swallowing patterns" "🟡"
report_pattern "\.ok\(\)" "Result conversion to Option" "🟡"

# Performance Issues
echo "🟢 PERFORMANCE PATTERNS"
echo "======================"

report_pattern "\.clone\(\)" "Clone operations" "🟢"
report_pattern "\.to_string\(\)" "String allocations" "🟢"
report_pattern "\.to_owned\(\)" "Ownership conversions" "🟢"

# Debug/Logging Issues
echo "🟡 LOGGING AND DEBUG"
echo "==================="

report_pattern "println!" "Debug print statements" "🟡"
report_pattern "dbg!" "Debug macros" "🟡"
report_pattern "eprintln!" "Error print statements" "🟡"

# Configuration Issues
echo "🟡 CONFIGURATION PATTERNS"
echo "========================"

report_pattern "env::var.*unwrap" "Unsafe env var access" "🔴"
report_pattern "parse.*unwrap_or" "Config parsing with fallbacks" "🟡"

# Code Quality
echo "🟢 CODE QUALITY METRICS"
echo "======================"

echo "📊 File Statistics:"
echo "    Source files: $(find "${SRC_DIR}" -name "*.rs" | wc -l)"
echo "    Test files: $(find "${PROJECT_ROOT}/tests" -name "*.rs" 2>/dev/null | wc -l || echo "0")"
echo "    Total lines: $(find "${SRC_DIR}" -name "*.rs" -exec wc -l {} + | tail -1 | awk '{print $1}')"

echo ""
echo "📊 Function/Type Counts:"
pub_fns=$(rg -c "pub fn " "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
pub_structs=$(rg -c "pub struct " "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
pub_enums=$(rg -c "pub enum " "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
test_fns=$(rg -c "#\[test\]|#\[tokio::test\]" "${PROJECT_ROOT}/tests" --type rust 2>/dev/null | awk -F: '{sum += $2} END {print sum+0}' || echo "0")

echo "    Public functions: ${pub_fns}"
echo "    Public structs: ${pub_structs}"
echo "    Public enums: ${pub_enums}"
echo "    Test functions: ${test_fns}"

# Test Coverage Analysis
echo ""
echo "📊 Test Coverage Estimate:"
if [ "$test_fns" -gt 0 ] && [ "$pub_fns" -gt 0 ]; then
    coverage_ratio=$(echo "scale=1; ${test_fns} * 100 / ${pub_fns}" | bc -l 2>/dev/null || echo "unknown")
    echo "    Test to function ratio: ${coverage_ratio}% (${test_fns} tests for ${pub_fns} public functions)"
else
    echo "    Unable to calculate test coverage ratio"
fi

# TODO/FIXME tracking
echo ""
echo "📝 TODO/FIXME Comments:"
todo_count=$(rg -c "TODO|FIXME|XXX|HACK" "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
if [ "$todo_count" -gt 0 ]; then
    echo "    Total TODO/FIXME comments: ${todo_count}"
    rg -n "TODO|FIXME|XXX|HACK" "${SRC_DIR}" --type rust | head -10 | sed 's/^/    /'
    if [ "$todo_count" -gt 10 ]; then
        echo "    ... and $((todo_count - 10)) more"
    fi
else
    echo "    ✅ No TODO/FIXME comments found"
fi

# Generate summary report
echo ""
echo "📋 DEBT SUMMARY"
echo "==============="

# Count critical issues
critical_unwraps=$(rg -c "\.unwrap\(\)" "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
critical_env=$(rg -c "env::var.*unwrap" "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')
debug_prints=$(rg -c "println!|dbg!|eprintln!" "${SRC_DIR}" --type rust | awk -F: '{sum += $2} END {print sum+0}')

echo "🔴 Critical Issues: $((critical_unwraps + critical_env))"
echo "🟡 High Priority: ${debug_prints}"
echo "🟢 Medium Priority: (see detailed analysis above)"

if [ $((critical_unwraps + critical_env)) -gt 0 ]; then
    echo ""
    echo "⚠️  RECOMMENDED ACTIONS:"
    echo "1. Address all critical safety issues immediately"
    echo "2. Review error handling patterns"
    echo "3. Consider code review for high-risk areas"
fi

# Save report to file
REPORT_FILE="${DEBT_DIR}/latest_scan_$(date +%Y%m%d_%H%M%S).txt"
echo ""
echo "💾 Report saved to: ${REPORT_FILE}"

# Run the scan again and save to file
{
    echo "# Technical Debt Scan Report"
    echo "Generated: $(date)"
    echo "Project: ${PROJECT_ROOT}"
    echo ""
    "$0" 2>&1
} > "${REPORT_FILE}"

echo "🏁 Scan complete!"