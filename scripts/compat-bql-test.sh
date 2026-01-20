#!/usr/bin/env bash
set -e

# BQL Compatibility Test (Parallel)
# Compares bean-query (Python) vs rledger-query (Rust) on valid beancount files
# Run inside: nix develop --command ./scripts/compat-bql-test.sh
#
# Environment variables:
#   PARALLEL_JOBS: Number of parallel workers (default: CPU count, max 8)

FIXTURES_DIR="${1:-tests/compat-full}"
RESULTS_DIR="tests/compat-results"

# Fall back to curated files if full suite doesn't exist
if [ ! -d "$FIXTURES_DIR" ] || [ -z "$(ls -A "$FIXTURES_DIR" 2>/dev/null)" ]; then
    FIXTURES_DIR="tests/compat/files"
fi
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_FILE="$RESULTS_DIR/bql_results_$TIMESTAMP.jsonl"

# Parallel settings
PARALLEL_JOBS="${PARALLEL_JOBS:-$(nproc 2>/dev/null || echo 4)}"
[ "$PARALLEL_JOBS" -gt 8 ] && PARALLEL_JOBS=8

echo "=== BQL Compatibility Test (Parallel) ==="
echo ""

mkdir -p "$RESULTS_DIR"
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

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

echo "Using $PARALLEL_JOBS parallel workers"
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
echo "Testing with 4 queries each..."
echo ""

# Standard queries to test
QUERIES_FILE="$TEMP_DIR/queries.txt"
cat > "$QUERIES_FILE" << 'EOF'
SELECT DISTINCT account ORDER BY account LIMIT 20
SELECT COUNT(*) AS total_txns
SELECT currency, COUNT(*) AS count GROUP BY currency ORDER BY count DESC LIMIT 10
SELECT YEAR(date) AS year, COUNT(*) AS count GROUP BY year ORDER BY year
EOF

# Create test script for parallel execution
cat > "$TEMP_DIR/test_bql.sh" << 'SCRIPT'
#!/usr/bin/env bash
file="$1"
query="$2"
FIXTURES_DIR="$3"
RLEDGER_QUERY="$4"

full_path="$FIXTURES_DIR/$file"
[ ! -f "$full_path" ] && exit 0

# Run Python bean-query
py_out=$(bean-query "$full_path" "$query" 2>/dev/null | head -50 || echo "ERROR")

# Run Rust rledger-query
rs_out=$("$RLEDGER_QUERY" "$full_path" "$query" 2>/dev/null | head -50 || echo "ERROR")

# Extract data rows (skip headers, separators, row counts)
py_data=$(echo "$py_out" | grep -v "^-" | grep -v "row(s)" | tail -n +2 | tr -s ' \t' ' ' | sed 's/^ //; s/ $//' | sort)
rs_data=$(echo "$rs_out" | grep -v "^-" | grep -v "row(s)" | tail -n +2 | tr -s ' \t' ' ' | sed 's/^ //; s/ $//' | sort)

if [ "$py_data" = "$rs_data" ]; then
    match="true"
else
    match="false"
fi

# Output JSON result (escape query for JSON)
query_escaped=$(echo "$query" | head -c 50 | sed 's/"/\\"/g')
echo "{\"file\":\"$file\",\"query\":\"$query_escaped\",\"match\":$match}"
SCRIPT
chmod +x "$TEMP_DIR/test_bql.sh"

# Build list of all (file, query) pairs
TEST_CASES_FILE="$TEMP_DIR/test_cases.txt"
> "$TEST_CASES_FILE"
for file in "${valid_files[@]}"; do
    while IFS= read -r query; do
        echo "$file	$query" >> "$TEST_CASES_FILE"
    done < "$QUERIES_FILE"
done

total_cases=$(wc -l < "$TEST_CASES_FILE" | tr -d ' ')
echo "Running $total_cases BQL tests..."
echo ""

start_time=$(date +%s)

# Run tests in parallel
while IFS=$'\t' read -r file query; do
    echo "$file"$'\t'"$query"
done < "$TEST_CASES_FILE" | \
    xargs -P "$PARALLEL_JOBS" -I {} bash -c '
        IFS=$'"'"'\t'"'"' read -r file query <<< "{}"
        '"$TEMP_DIR"'/test_bql.sh "$file" "$query" "'"$FIXTURES_DIR"'" "'"$RLEDGER_QUERY"'"
    ' > "$RESULTS_FILE"

end_time=$(date +%s)
elapsed=$((end_time - start_time))

echo ""
echo "=== BQL Results Summary ==="
echo ""

total_queries=$(wc -l < "$RESULTS_FILE" | tr -d ' ')
matching_queries=$(grep -c '"match":true' "$RESULTS_FILE" || echo 0)
failed_queries=$((total_queries - matching_queries))

if [ "$total_queries" -gt 0 ]; then
    match_pct=$((matching_queries * 100 / total_queries))
else
    match_pct=0
fi

echo "Total queries:    $total_queries"
echo "Matching:         $matching_queries ($match_pct%)"
echo "Different:        $failed_queries"
echo "Execution time:   ${elapsed}s"
echo ""
echo "Results in: $RESULTS_FILE"
