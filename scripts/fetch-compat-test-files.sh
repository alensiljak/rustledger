#!/usr/bin/env bash
set -e

# Fetch Beancount Test Files from Multiple Sources
# Run inside: nix develop --command ./scripts/fetch-compat-test-files.sh
#
# This script downloads .beancount files from various open source projects
# to create a comprehensive compatibility test suite.
#
# Target: ~800+ real beancount files from diverse sources

DEST="tests/compat/files"
TMPDIR="/tmp/beancount-fetch-$$"

echo "=== Fetching Beancount Test Files ==="
echo ""

mkdir -p "$TMPDIR"
mkdir -p "$DEST"

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

# Helper function to fetch and extract beancount files
fetch_repo() {
    local name="$1"
    local repo="$2"  # GitHub repo in "owner/repo" format
    local branch="${3:-}"
    local subdir="$DEST/$name"

    mkdir -p "$subdir"
    echo "Fetching $name..."

    local clone_args=(-- --depth=1)
    if [ -n "$branch" ]; then
        clone_args+=(--branch="$branch")
    fi

    gh repo clone "$repo" "$TMPDIR/$name" "${clone_args[@]}" 2>/dev/null || {
        echo "  Warning: Failed to clone $name"
        return 0
    }
    
    # Find and copy all .beancount files, preserving some path info in filename
    find "$TMPDIR/$name" -name "*.beancount" -type f | while read -r f; do
        # Create a unique filename based on relative path
        relpath="${f#$TMPDIR/$name/}"
        safename=$(echo "$relpath" | tr '/' '_')
        cp "$f" "$subdir/$safename" 2>/dev/null || true
    done
    
    count=$(find "$subdir" -name "*.beancount" | wc -l | tr -d ' ')
    echo "  Found $count files"
}

# 1. beancount v2 - most comprehensive test data
fetch_repo "beancount-v2" "beancount/beancount" "v2"

# 2. beancount v3
fetch_repo "beancount-v3" "beancount/beancount" "v3"

# 3. fava - web interface with test data
fetch_repo "fava" "beancount/fava"

# 4. beangulp - importer framework
fetch_repo "beangulp" "beancount/beangulp"

# 5. ledger2beancount - conversion tool tests
fetch_repo "ledger2beancount" "beancount/ledger2beancount"

# 6. beancount-import - import web UI
fetch_repo "beancount-import" "jbms/beancount-import"

# 7. LaunchPlatform parser - standalone parser tests
fetch_repo "launchplatform" "LaunchPlatform/beancount-parser"

# 8. smart_importer - ML importers
fetch_repo "smart-importer" "beancount/smart_importer"

# 9. beancount_reds_importers
fetch_repo "reds-importers" "redstreet/beancount_reds_importers"

# 10. Community examples
fetch_repo "community-wileykestner" "wileykestner/beancount-example"

# 11. Parser Lima - comprehensive parser test suite (246 files)
fetch_repo "parser-lima" "tesujimath/beancount-parser-lima"

# 12. Fava Investor - investment tracking plugin
fetch_repo "fava-investor" "redstreet/fava_investor"

# 13. Fava Dashboards
fetch_repo "fava-dashboards" "andreasgerstmayr/fava-dashboards"

# 14. Beancern (tariochbctools)
fetch_repo "beancern" "tarioch/beancern"

# Summary
echo ""
echo "=== Summary ==="
echo ""

total=0
for dir in "$DEST"/*/; do
    if [ -d "$dir" ]; then
        count=$(find "$dir" -name "*.beancount" | wc -l | tr -d ' ')
        dirname=$(basename "$dir")
        printf "  %-25s %4d files\n" "$dirname:" "$count"
        total=$((total + count))
    fi
done

# Also count top-level files
top_count=$(find "$DEST" -maxdepth 1 -name "*.beancount" | wc -l | tr -d ' ')
if [ "$top_count" -gt 0 ]; then
    printf "  %-25s %4d files\n" "(top-level):" "$top_count"
    total=$((total + top_count))
fi

echo ""
echo "Total: $total beancount files"
echo ""
echo "Files saved to: $DEST"
