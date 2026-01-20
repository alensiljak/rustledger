#!/usr/bin/env bash
set -e

# BQL Compatibility Test
# Compares bean-query (Python) vs rledger-query (Rust) on valid beancount files
# Run inside: nix develop --command ./scripts/compat-bql-test.sh

FIXTURES_DIR="spec/fixtures"
RESULTS_DIR="spec/fixtures/compat-results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_FILE="$RESULTS_DIR/bql_results_$TIMESTAMP.jsonl"

# Standard queries to test
QUERIES=(
    "SELECT DISTINCT account ORDER BY account LIMIT 20"
    "SELECT COUNT(*) AS total_txns"
    "SELECT currency, COUNT(*) AS count GROUP BY currency ORDER BY count DESC LIMIT 10"
    "SELECT YEAR(date) AS year, COUNT(*) AS count GROUP BY year ORDER BY year"
)

echo "=== BQL Compatibility Test ==="
echo ""

mkdir -p "$RESULTS_DIR"

# Build rustledger
echo "Building rustledger..."
cargo build --release --quiet
RLEDGER_QUERY="./target/release/rledger-query"

if [ ! -x "$RLEDGER_QUERY" ]; then
    echo "Error: rledger-query not found"
    exit 1
fi

# Check bean-query is available
if ! command -v bean-query &> /dev/null; then
    echo "Error: bean-query not found. Run inside nix develop."
    exit 1
fi

echo "Testing BQL queries on valid files..."
echo ""

# Find files that passed both check tests (use most recent results)
COMPAT_RESULTS=$(ls -t "$RESULTS_DIR"/results_*.jsonl 2>/dev/null | head -1)
if [ -z "$COMPAT_RESULTS" ]; then
    echo "No compat-test results found. Run ./scripts/compat-test.sh first."
    exit 1
fi

# Get files where both implementations passed (match=true and both exit=0)
mapfile -t valid_files < <(jq -r 'select(.match == true and .python.exit == 0 and .rust.exit == 0) | .file' "$COMPAT_RESULTS" | head -50)

echo "Found ${#valid_files[@]} files that passed both implementations"
echo "Testing first 50 with ${#QUERIES[@]} queries each..."
echo ""

total_queries=0
matching_queries=0
failed_queries=0

for file in "${valid_files[@]}"; do
    full_path="$FIXTURES_DIR/$file"
    if [ ! -f "$full_path" ]; then
        continue
    fi

    for query in "${QUERIES[@]}"; do
        total_queries=$((total_queries + 1))

        # Run Python bean-query
        py_out=$(bean-query "$full_path" "$query" 2>/dev/null | head -50 || echo "ERROR")

        # Run Rust rledger-query
        rs_out=$("$RLEDGER_QUERY" "$full_path" "$query" 2>/dev/null | head -50 || echo "ERROR")

        # Extract data rows only (skip headers, separators, row counts)
        # Python format: header, separator (dashes), data rows
        # Rust format: header, separator (dashes), data rows, "N row(s)"
        py_data=$(echo "$py_out" | grep -v "^-" | grep -v "row(s)" | tail -n +2 | tr -s ' \t' ' ' | sed 's/^ //; s/ $//' | sort)
        rs_data=$(echo "$rs_out" | grep -v "^-" | grep -v "row(s)" | tail -n +2 | tr -s ' \t' ' ' | sed 's/^ //; s/ $//' | sort)

        if [ "$py_data" = "$rs_data" ]; then
            matching_queries=$((matching_queries + 1))
            match="true"
        else
            failed_queries=$((failed_queries + 1))
            match="false"
        fi

        # Log result
        echo "{\"file\":\"$file\",\"query\":\"${query:0:50}\",\"match\":$match}" >> "$RESULTS_FILE"
    done

    # Progress
    printf "\r  Tested: %d queries (%d match, %d differ)    " "$total_queries" "$matching_queries" "$failed_queries"
done

echo ""
echo ""
echo "=== BQL Results Summary ==="
echo ""
echo "Total queries:    $total_queries"
echo "Matching:         $matching_queries ($((matching_queries * 100 / total_queries))%)"
echo "Different:        $failed_queries"
echo ""
echo "Results in: $RESULTS_FILE"
