#!/usr/bin/env bash
# Run regression tests against rledger
# Usage: ./scripts/test-regressions.sh [rledger-binary]

set -e

RLEDGER="${1:-./target/release/rledger}"
TESTS_DIR="tests/regressions"
PASSED=0
FAILED=0
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

for f in "$TESTS_DIR"/issue-*.beancount; do
    if [ ! -f "$f" ]; then
        echo "No regression tests found in $TESTS_DIR"
        exit 0
    fi

    BASENAME=$(basename "$f")
    ISSUE_NUM=$(echo "$BASENAME" | sed 's/issue-\([0-9]*\).*/\1/')

    if "$RLEDGER" check "$f" > /dev/null 2>&1; then
        echo "✓ #$ISSUE_NUM passed"
        PASSED=$((PASSED + 1))
    else
        echo "✗ #$ISSUE_NUM FAILED"
        FAILED=$((FAILED + 1))
        FAILED_TESTS="$FAILED_TESTS #$ISSUE_NUM"
    fi
done

echo ""
echo "Results: $PASSED passed, $FAILED failed"

if [ $FAILED -gt 0 ]; then
    echo "Failed tests:$FAILED_TESTS"
    exit 1
fi
