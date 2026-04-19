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
FAILED_TESTS=""

# Create temp files for output capture; clean up on exit
STDERR_TMP=$(mktemp)
QUERY_STDERR_TMP=$(mktemp)
trap 'rm -f "$STDERR_TMP" "$QUERY_STDERR_TMP"' EXIT

# Build if binary doesn't exist
if [ ! -f "$RLEDGER" ]; then
  echo "Building rledger..."
  cargo build --release --bin rledger
fi

echo "Running regression tests..."
echo "Binary: $RLEDGER"
echo "Tests:  $TESTS_DIR"
echo ""

# Run a query and validate it succeeded. Sets query_out on success.
# Returns 1 if the query command itself failed.
run_query() {
  local file="$1"
  local bql="$2"
  local query_exit=0
  query_out=$("$RLEDGER" query "$file" "$bql" 2>"$QUERY_STDERR_TMP") || query_exit=$?
  local query_stderr
  query_stderr=$(cat "$QUERY_STDERR_TMP" 2>/dev/null || true)
  if [ "$query_exit" -ne 0 ]; then
    echo "    FAIL: query command failed (exit $query_exit)"
    echo "    query: $bql"
    if [ -n "$query_stderr" ]; then
      echo "    stderr: $(head -3 <<<"$query_stderr")"
    fi
    return 1
  fi
  return 0
}

# Run a single assertion. Returns 0 on success, 1 on failure.
# Globals: RLEDGER, check_stdout, check_stderr, check_exit, check_json
run_assertion() {
  local file="$1"
  local assertion="$2"

  # no_errors: exit code must be 0
  if [[ $assertion == "no_errors" ]]; then
    if [ "$check_exit" -ne 0 ]; then
      echo "    FAIL: expected no errors, got exit $check_exit"
      echo "    stderr: $(head -5 <<<"$check_stderr")"
      return 1
    fi
    return 0
  fi

  # error_count == N (uses cached JSON output from the main check run)
  if [[ $assertion =~ ^error_count\ *==\ *([0-9]+)$ ]]; then
    local expected="${BASH_REMATCH[1]}"
    local actual
    actual=$(echo "$check_json" | grep -oE '"error_count"[[:space:]]*:[[:space:]]*[0-9]+' | grep -oE '[0-9]+' | head -n1 || echo "")
    if [ -z "$actual" ]; then
      # Fallback: count error lines in text output
      actual=$(echo "$check_stdout" | grep -cE '(^error\[|: error\[)' || echo "0")
    fi
    if [ "$actual" != "$expected" ]; then
      echo "    FAIL: error_count == $expected, got $actual"
      return 1
    fi
    return 0
  fi

  # check_stderr contains "TEXT"
  if [[ $assertion =~ ^check_stderr\ +contains\ +\"(.+)\"$ ]]; then
    local pattern="${BASH_REMATCH[1]}"
    if ! echo "$check_stderr" | grep -qF -- "$pattern"; then
      echo "    FAIL: stderr should contain \"$pattern\""
      return 1
    fi
    return 0
  fi

  # check_stderr !contains "TEXT"
  if [[ $assertion =~ ^check_stderr\ +\!contains\ +\"(.+)\"$ ]]; then
    local pattern="${BASH_REMATCH[1]}"
    if echo "$check_stderr" | grep -qF -- "$pattern"; then
      echo "    FAIL: stderr should NOT contain \"$pattern\""
      echo "    found: $(grep -F -- "$pattern" <<<"$check_stderr" | head -3)"
      return 1
    fi
    return 0
  fi

  # query "BQL" contains "TEXT"
  if [[ $assertion =~ ^query\ +\"(.+)\"\ +contains\ +\"(.+)\"$ ]]; then
    local bql="${BASH_REMATCH[1]}"
    local pattern="${BASH_REMATCH[2]}"
    if ! run_query "$file" "$bql"; then
      return 1
    fi
    if ! echo "$query_out" | grep -qF -- "$pattern"; then
      echo "    FAIL: query output should contain \"$pattern\""
      echo "    query: $bql"
      echo "    output: $(head -10 <<<"$query_out")"
      return 1
    fi
    return 0
  fi

  # query "BQL" !contains "TEXT"
  if [[ $assertion =~ ^query\ +\"(.+)\"\ +\!contains\ +\"(.+)\"$ ]]; then
    local bql="${BASH_REMATCH[1]}"
    local pattern="${BASH_REMATCH[2]}"
    if ! run_query "$file" "$bql"; then
      return 1
    fi
    if echo "$query_out" | grep -qF -- "$pattern"; then
      echo "    FAIL: query output should NOT contain \"$pattern\""
      echo "    query: $bql"
      echo "    found: $(grep -F -- "$pattern" <<<"$query_out" | head -3)"
      return 1
    fi
    return 0
  fi

  # query "BQL" row_count == N
  if [[ $assertion =~ ^query\ +\"(.+)\"\ +row_count\ *==\ *([0-9]+)$ ]]; then
    local bql="${BASH_REMATCH[1]}"
    local expected="${BASH_REMATCH[2]}"
    if ! run_query "$file" "$bql"; then
      return 1
    fi
    # Parse the trailing "N row(s)" summary line for the actual count
    local actual
    actual=$(echo "$query_out" | grep -oE '^[0-9]+ row' | grep -oE '^[0-9]+' | tail -1 || echo "")
    if [ -z "$actual" ]; then
      # Fallback: count non-empty lines after header+separator (skip first 2)
      actual=$(echo "$query_out" | tail -n +3 | grep -cE '.+' || echo "0")
    fi
    if [ "$actual" != "$expected" ]; then
      echo "    FAIL: query row_count == $expected, got $actual"
      echo "    query: $bql"
      echo "    output: $(head -10 <<<"$query_out")"
      return 1
    fi
    return 0
  fi

  echo "    FAIL: unknown assertion syntax: $assertion"
  return 1
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

  # Run rledger check and capture output (text mode)
  check_stdout=""
  check_stderr=""
  check_exit=0
  check_stdout=$("$RLEDGER" check --no-cache "$f" 2>"$STDERR_TMP") || check_exit=$?
  check_stderr=$(cat "$STDERR_TMP" 2>/dev/null || true)

  # Also run in JSON mode once (cached for error_count assertions)
  check_json=$("$RLEDGER" check --format json --no-cache "$f" 2>/dev/null || true)

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
      if ! run_assertion "$f" "$assertion"; then
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

echo ""
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
  echo "Failed tests:$FAILED_TESTS"
  exit 1
fi
