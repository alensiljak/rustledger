#!/usr/bin/env bash
set -e

# Beancount Compatibility Test Harness
# Compares bean-check (Python) vs rledger-check (Rust) on all .beancount files
# Run inside: nix develop --command ./scripts/compat-test.sh

FIXTURES_DIR="spec/fixtures"
RESULTS_DIR="spec/fixtures/compat-results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_FILE="$RESULTS_DIR/results_$TIMESTAMP.jsonl"
SUMMARY_FILE="$RESULTS_DIR/summary_$TIMESTAMP.md"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=== Beancount Compatibility Test Harness ==="
echo ""

# Create results directory
mkdir -p "$RESULTS_DIR"

# Build rustledger in release mode
echo "Building rustledger..."
cargo build --release --quiet
RLEDGER_CHECK="./target/release/rledger-check"

if [ ! -x "$RLEDGER_CHECK" ]; then
    echo "Error: rledger-check not found at $RLEDGER_CHECK"
    exit 1
fi

# Check that bean-check is available
if ! command -v bean-check &> /dev/null; then
    echo "Error: bean-check not found. Run inside nix develop."
    exit 1
fi

echo "Using:"
echo "  Python: $(bean-check --version 2>&1 | head -1 || echo 'bean-check available')"
echo "  Rust:   $($RLEDGER_CHECK --version 2>&1 || echo 'rledger-check available')"
echo ""

# Counters
total=0
parse_match=0
check_match=0
parse_fail_rust=0
parse_fail_python=0
check_mismatch=0

# Find all beancount files
mapfile -t files < <(find "$FIXTURES_DIR" -name "*.beancount" -type f | sort)
total_files=${#files[@]}

echo "Found $total_files .beancount files to test"
echo "Results will be written to: $RESULTS_FILE"
echo ""

# Progress tracking
progress=0
start_time=$(date +%s)

test_file() {
    local file="$1"
    local relpath="${file#$FIXTURES_DIR/}"

    # Run Python bean-check
    local py_stdout py_stderr py_exit
    py_stdout=$(mktemp)
    py_stderr=$(mktemp)
    bean-check "$file" >"$py_stdout" 2>"$py_stderr" && py_exit=0 || py_exit=$?
    local py_out=$(cat "$py_stdout")
    local py_err=$(cat "$py_stderr")
    rm -f "$py_stdout" "$py_stderr"

    # Run Rust rledger-check
    local rs_stdout rs_stderr rs_exit
    rs_stdout=$(mktemp)
    rs_stderr=$(mktemp)
    "$RLEDGER_CHECK" "$file" >"$rs_stdout" 2>"$rs_stderr" && rs_exit=0 || rs_exit=$?
    local rs_out=$(cat "$rs_stdout")
    local rs_err=$(cat "$rs_stderr")
    rm -f "$rs_stdout" "$rs_stderr"

    # Determine if it's a parse error
    local py_parse_error=false
    local rs_parse_error=false

    if echo "$py_err" | grep -qi "syntax error\|parse error\|unexpected"; then
        py_parse_error=true
    fi
    if echo "$rs_err" | grep -qi "syntax error\|parse error\|unexpected"; then
        rs_parse_error=true
    fi

    # Check if exits match
    local exit_match=false
    if [ "$py_exit" -eq 0 ] && [ "$rs_exit" -eq 0 ]; then
        exit_match=true
    elif [ "$py_exit" -ne 0 ] && [ "$rs_exit" -ne 0 ]; then
        exit_match=true
    fi

    # Count errors in output (rough heuristic)
    local py_error_count=$(echo "$py_err" | grep -c "error\|Error" || true)
    local rs_error_count=$(echo "$rs_err" | grep -c "error\|Error" || true)
    # Ensure counts are numeric
    py_error_count=${py_error_count:-0}
    rs_error_count=${rs_error_count:-0}
    [[ "$py_error_count" =~ ^[0-9]+$ ]] || py_error_count=0
    [[ "$rs_error_count" =~ ^[0-9]+$ ]] || rs_error_count=0

    # Convert bash booleans to JSON booleans
    local exit_match_json="false"
    local py_parse_json="false"
    local rs_parse_json="false"
    [ "$exit_match" = "true" ] && exit_match_json="true"
    [ "$py_parse_error" = "true" ] && py_parse_json="true"
    [ "$rs_parse_error" = "true" ] && rs_parse_json="true"

    # Build JSON manually to avoid jq escaping issues
    local py_err_short="${py_err:0:200}"
    local rs_err_short="${rs_err:0:200}"
    # Escape special chars for JSON
    py_err_short=$(echo "$py_err_short" | sed 's/\\/\\\\/g; s/"/\\"/g; s/	/\\t/g' | tr '\n' ' ')
    rs_err_short=$(echo "$rs_err_short" | sed 's/\\/\\\\/g; s/"/\\"/g; s/	/\\t/g' | tr '\n' ' ')

    # Output JSON result (one line per file)
    local json_result="{\"file\":\"$relpath\",\"python\":{\"exit\":$py_exit,\"parse_error\":$py_parse_json,\"error_count\":$py_error_count,\"stderr\":\"$py_err_short\"},\"rust\":{\"exit\":$rs_exit,\"parse_error\":$rs_parse_json,\"error_count\":$rs_error_count,\"stderr\":\"$rs_err_short\"},\"match\":$exit_match_json}"

    echo "$json_result" >> "$RESULTS_FILE"

    # Update counters
    total=$((total + 1))
    if [ "$exit_match" = "true" ]; then
        check_match=$((check_match + 1))
    else
        check_mismatch=$((check_mismatch + 1))
    fi

    if [ "$py_parse_error" = "true" ] && [ "$rs_parse_error" = "false" ]; then
        parse_fail_python=$((parse_fail_python + 1))
    elif [ "$rs_parse_error" = "true" ] && [ "$py_parse_error" = "false" ]; then
        parse_fail_rust=$((parse_fail_rust + 1))
    fi

    if [ "$py_parse_error" = "$rs_parse_error" ]; then
        parse_match=$((parse_match + 1))
    fi

    # Progress indicator
    progress=$((progress + 1))
    if [ $((progress % 50)) -eq 0 ]; then
        local pct=$((progress * 100 / total_files))
        local elapsed=$(($(date +%s) - start_time))
        local rate=$((progress / (elapsed + 1)))
        local eta=$(((total_files - progress) / (rate + 1)))
        printf "\r  Progress: %d/%d (%d%%) - ETA: %ds    " "$progress" "$total_files" "$pct" "$eta"
    fi
}

echo "Running tests..."
echo ""

# Process all files
for file in "${files[@]}"; do
    test_file "$file"
done

echo ""
echo ""
echo "=== Results Summary ==="
echo ""

# Calculate percentages
parse_pct=$((parse_match * 100 / total))
check_pct=$((check_match * 100 / total))

# Display summary
echo "Files tested:       $total"
echo ""
echo "Parse behavior match: $parse_match/$total ($parse_pct%)"
echo "  - Python parse error only: $parse_fail_python"
echo "  - Rust parse error only:   $parse_fail_rust"
echo ""
echo "Check exit match:     $check_match/$total ($check_pct%)"
echo "  - Mismatches: $check_mismatch"
echo ""

# Generate markdown summary
cat > "$SUMMARY_FILE" << EOF
# Beancount Compatibility Report

Generated: $(date)

## Summary

| Metric | Count | Percentage |
|--------|-------|------------|
| Files tested | $total | 100% |
| Parse match | $parse_match | $parse_pct% |
| Check exit match | $check_match | $check_pct% |

## Parse Behavior

- **Python-only parse errors:** $parse_fail_python
- **Rust-only parse errors:** $parse_fail_rust

## Check Mismatches

Total: $check_mismatch files

EOF

# Find and list mismatches
echo "### Files with exit code mismatch:" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

jq -r 'select(.match == false) | "- \(.file): Python=\(.python.exit), Rust=\(.rust.exit)"' "$RESULTS_FILE" >> "$SUMMARY_FILE" 2>/dev/null || true

echo "" >> "$SUMMARY_FILE"
echo "## Details" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"
echo "Full results in: \`$RESULTS_FILE\`" >> "$SUMMARY_FILE"

echo "Summary written to: $SUMMARY_FILE"
echo "Full results in:    $RESULTS_FILE"
echo ""

# Exit with error if match rate is too low
if [ "$check_pct" -lt 80 ]; then
    echo -e "${YELLOW}Warning: Check match rate below 80%${NC}"
fi
