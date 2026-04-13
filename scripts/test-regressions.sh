#!/usr/bin/env bash
# Run regression tests against rledger
#
# Supports inline assertions in beancount files via ; ASSERT: comments:
#
#   ; ASSERT: no_errors
#   ; ASSERT: error_count == 0
#   ; ASSERT: check_stderr !contains "ambiguous"
#   ; ASSERT: check_stderr contains "warning"
#   ; ASSERT: query "SELECT DISTINCT account" contains "Equity:Currency:USD"
#   ; ASSERT: query "SELECT DISTINCT account" row_count == 4
#
# Files without ASSERT comments fall back to exit-code-only checking (exit 0 = pass).
#
# Usage: ./scripts/test-regressions.sh [rledger-binary]

set -e

RLEDGER="${1:-./target/release/rledger}"
TESTS_DIR="tests/regressions"
PASSED=0
FAILED=0
SKIPPED=0
FAILED_TESTS=""

# Build if binary doesn't exist
if [ ! -f "$RLEDGER" ]; then
    echo "Building rledger..."
    cargo build --release --bin rledger
fi

echo "Running regression tests..."
echo "Binary: $RLEDGER"
echo "Tests:  $TESTS_DIR"
echo ""

# Run a single assertion. Returns 0 on success, 1 on failure.
# Globals: RLEDGER, check_stdout, check_stderr, check_exit
run_assertion() {
    local file="$1"
    local assertion="$2"
    local issue_num="$3"

    # no_errors: exit code must be 0
    if [[ "$assertion" == "no_errors" ]]; then
        if [ "$check_exit" -ne 0 ]; then
            echo "    FAIL: expected no errors, got exit $check_exit"
            echo "    stderr: $(head -5 <<< "$check_stderr")"
            return 1
        fi
        return 0
    fi

    # error_count == N
    if [[ "$assertion" =~ ^error_count\ *==\ *([0-9]+)$ ]]; then
        local expected="${BASH_REMATCH[1]}"
        # Try JSON output first for precise count
        local json_out
        json_out=$("$RLEDGER" check --format json --no-cache "$file" 2>/dev/null || true)
        local actual
        actual=$(echo "$json_out" | grep -o '"error_count":[0-9]*' | cut -d: -f2 || echo "")
        if [ -z "$actual" ]; then
            # Fallback: count error lines in text output
            actual=$(echo "$check_stdout" | grep -c "^error\[" || echo "0")
        fi
        if [ "$actual" != "$expected" ]; then
            echo "    FAIL: error_count == $expected, got $actual"
            return 1
        fi
        return 0
    fi

    # check_stderr contains "TEXT"
    if [[ "$assertion" =~ ^check_stderr\ +contains\ +\"(.+)\"$ ]]; then
        local pattern="${BASH_REMATCH[1]}"
        if ! echo "$check_stderr" | grep -qF "$pattern"; then
            echo "    FAIL: stderr should contain \"$pattern\""
            return 1
        fi
        return 0
    fi

    # check_stderr !contains "TEXT"
    if [[ "$assertion" =~ ^check_stderr\ +\!contains\ +\"(.+)\"$ ]]; then
        local pattern="${BASH_REMATCH[1]}"
        if echo "$check_stderr" | grep -qF "$pattern"; then
            echo "    FAIL: stderr should NOT contain \"$pattern\""
            echo "    found: $(grep -F "$pattern" <<< "$check_stderr" | head -3)"
            return 1
        fi
        return 0
    fi

    # query "BQL" contains "TEXT"
    if [[ "$assertion" =~ ^query\ +\"(.+)\"\ +contains\ +\"(.+)\"$ ]]; then
        local bql="${BASH_REMATCH[1]}"
        local pattern="${BASH_REMATCH[2]}"
        local query_out
        query_out=$("$RLEDGER" query -q "$file" "$bql" 2>/dev/null || true)
        if ! echo "$query_out" | grep -qF "$pattern"; then
            echo "    FAIL: query output should contain \"$pattern\""
            echo "    query: $bql"
            echo "    output: $(head -10 <<< "$query_out")"
            return 1
        fi
        return 0
    fi

    # query "BQL" !contains "TEXT"
    if [[ "$assertion" =~ ^query\ +\"(.+)\"\ +\!contains\ +\"(.+)\"$ ]]; then
        local bql="${BASH_REMATCH[1]}"
        local pattern="${BASH_REMATCH[2]}"
        local query_out
        query_out=$("$RLEDGER" query -q "$file" "$bql" 2>/dev/null || true)
        if echo "$query_out" | grep -qF "$pattern"; then
            echo "    FAIL: query output should NOT contain \"$pattern\""
            echo "    query: $bql"
            echo "    found: $(grep -F "$pattern" <<< "$query_out" | head -3)"
            return 1
        fi
        return 0
    fi

    # query "BQL" row_count == N
    if [[ "$assertion" =~ ^query\ +\"(.+)\"\ +row_count\ *==\ *([0-9]+)$ ]]; then
        local bql="${BASH_REMATCH[1]}"
        local expected="${BASH_REMATCH[2]}"
        local query_out
        query_out=$("$RLEDGER" query -q "$file" "$bql" 2>/dev/null || true)
        # Count non-empty, non-header lines (skip first 2 lines: header + separator)
        local actual
        actual=$(echo "$query_out" | tail -n +3 | grep -c '.' || echo "0")
        if [ "$actual" != "$expected" ]; then
            echo "    FAIL: query row_count == $expected, got $actual"
            echo "    query: $bql"
            echo "    output: $(head -10 <<< "$query_out")"
            return 1
        fi
        return 0
    fi

    echo "    WARN: unknown assertion syntax: $assertion"
    return 0
}

for f in "$TESTS_DIR"/issue-*.beancount; do
    if [ ! -f "$f" ]; then
        echo "No regression tests found in $TESTS_DIR"
        exit 0
    fi

    BASENAME=$(basename "$f")
    ISSUE_NUM=$(echo "$BASENAME" | sed 's/issue-\([0-9]*\).*/\1/')

    # Extract ASSERT lines from the file
    assertions=()
    while IFS= read -r line; do
        assertions+=("$line")
    done < <(grep '^; ASSERT: ' "$f" | sed 's/^; ASSERT: //')

    # Run rledger check and capture output
    check_stdout=""
    check_stderr=""
    check_exit=0
    check_stdout=$("$RLEDGER" check --no-cache "$f" 2>/tmp/rledger-regression-stderr) || check_exit=$?
    check_stderr=$(cat /tmp/rledger-regression-stderr 2>/dev/null || true)

    if [ ${#assertions[@]} -eq 0 ]; then
        # No assertions — fall back to exit-code-only
        if [ "$check_exit" -eq 0 ]; then
            echo "✓ #$ISSUE_NUM passed (exit-code only)"
            PASSED=$((PASSED + 1))
        else
            echo "✗ #$ISSUE_NUM FAILED (exit $check_exit)"
            FAILED=$((FAILED + 1))
            FAILED_TESTS="$FAILED_TESTS #$ISSUE_NUM"
        fi
    else
        # Run each assertion
        test_failed=0
        for assertion in "${assertions[@]}"; do
            if ! run_assertion "$f" "$assertion" "$ISSUE_NUM"; then
                test_failed=1
            fi
        done

        if [ "$test_failed" -eq 0 ]; then
            echo "✓ #$ISSUE_NUM passed (${#assertions[@]} assertions)"
            PASSED=$((PASSED + 1))
        else
            echo "✗ #$ISSUE_NUM FAILED"
            FAILED=$((FAILED + 1))
            FAILED_TESTS="$FAILED_TESTS #$ISSUE_NUM"
        fi
    fi
done

# Cleanup
rm -f /tmp/rledger-regression-stderr

echo ""
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    echo "Failed tests:$FAILED_TESTS"
    exit 1
fi
