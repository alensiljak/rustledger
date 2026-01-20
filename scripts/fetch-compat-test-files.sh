#!/usr/bin/env bash
set -e
# Fetch beancount test files from multiple sources for compatibility testing
# Run inside: nix develop --command ./scripts/fetch-compat-test-files.sh

DEST="tests/compat-full"
TMPDIR="/tmp/beancount-fetch-$$"

echo "=== Beancount Compatibility Test File Fetcher ==="
echo "Target: 1000+ real beancount files from diverse sources"
echo ""

# Cleanup function
cleanup() {
    echo "Cleaning up temporary files..."
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

mkdir -p "$TMPDIR"
mkdir -p "$DEST"/{beancount-v2,beancount-v3,parser-lima,fava,beangulp,ledger2beancount,beancount-import,launchplatform,smart-importer,reds-importers,community}

# Helper function to clone and extract
fetch_repo() {
    local name="$1"
    local url="$2"
    local branch="${3:-}"
    local dest_subdir="$4"

    echo "[$name] Cloning..."
    if [ -n "$branch" ]; then
        git clone --depth=1 --branch="$branch" "$url" "$TMPDIR/$name" 2>/dev/null || {
            echo "[$name] Failed to clone (branch: $branch), skipping..."
            return 0
        }
    else
        git clone --depth=1 "$url" "$TMPDIR/$name" 2>/dev/null || {
            echo "[$name] Failed to clone, skipping..."
            return 0
        }
    fi

    local count=$(find "$TMPDIR/$name" -name "*.beancount" -type f | wc -l)
    echo "[$name] Found $count .beancount files"

    # Copy files, preserving some path info in filename to avoid collisions
    find "$TMPDIR/$name" -name "*.beancount" -type f | while read -r f; do
        # Create unique filename from path
        relpath="${f#$TMPDIR/$name/}"
        safename=$(echo "$relpath" | tr '/' '_')
        cp "$f" "$DEST/$dest_subdir/$safename"
    done

    rm -rf "$TMPDIR/$name"
}

echo ""
echo "=== Phase 1: Official Beancount Repositories ==="

# 1. beancount v2 (most comprehensive test data)
fetch_repo "beancount-v2" "https://github.com/beancount/beancount" "v2" "beancount-v2"

# 2. beancount v3
fetch_repo "beancount-v3" "https://github.com/beancount/beancount" "v3" "beancount-v3"

echo ""
echo "=== Phase 2: Parser Test Suites ==="

# 3. beancount-parser-lima (language-independent test suite)
fetch_repo "parser-lima" "https://github.com/tesujimath/beancount-parser-lima" "" "parser-lima"

# 4. LaunchPlatform/beancount-parser (Lark-based parser)
fetch_repo "launchplatform" "https://github.com/LaunchPlatform/beancount-parser" "" "launchplatform"

echo ""
echo "=== Phase 3: Tooling Repositories ==="

# 5. fava (web interface)
fetch_repo "fava" "https://github.com/beancount/fava" "" "fava"

# 6. beangulp (importer framework)
fetch_repo "beangulp" "https://github.com/beancount/beangulp" "" "beangulp"

# 7. ledger2beancount (converter)
fetch_repo "ledger2beancount" "https://github.com/beancount/ledger2beancount" "" "ledger2beancount"

# 8. beancount-import (web UI for importing)
fetch_repo "beancount-import" "https://github.com/jbms/beancount-import" "" "beancount-import"

# 9. smart_importer (ML importers)
fetch_repo "smart-importer" "https://github.com/beancount/smart_importer" "" "smart-importer"

# 10. beancount_reds_importers
fetch_repo "reds-importers" "https://github.com/redstreet/beancount_reds_importers" "" "reds-importers"

echo ""
echo "=== Phase 4: Community Examples ==="

# 11. wileykestner/beancount-example
fetch_repo "wileykestner" "https://github.com/wileykestner/beancount-example" "" "community"

# 12. Donearm/ledger (personal ledger)
fetch_repo "donearm" "https://github.com/Donearm/ledger" "" "community"

# 13. seltzered/fava-classy-portfolio-demo
fetch_repo "fava-portfolio-demo" "https://github.com/seltzered/fava-classy-portfolio-demo" "" "community"

# 14. andreasgerstmayr/fava-dashboards
fetch_repo "fava-dashboards" "https://github.com/andreasgerstmayr/fava-dashboards" "" "community"

# 15. tarioch/beancounttools
fetch_repo "beancounttools" "https://github.com/tarioch/beancounttools" "" "community"

# 16. beancount/beanquery (examples)
fetch_repo "beanquery" "https://github.com/beancount/beanquery" "" "community"

# 17. beancount/beangrow
fetch_repo "beangrow" "https://github.com/beancount/beangrow" "" "community"

# 18. redstreet/fava_investor
fetch_repo "fava-investor" "https://github.com/redstreet/fava_investor" "" "community"

# 19. beancount/beancount-mode (emacs examples)
fetch_repo "beancount-mode" "https://github.com/beancount/beancount-mode" "" "community"

# 20. LaunchPlatform/beancount-black (formatter examples)
fetch_repo "beancount-black" "https://github.com/LaunchPlatform/beancount-black" "" "community"

# 21. henriquebastos/gnucash-to-beancount
fetch_repo "gnucash-to-beancount" "https://github.com/henriquebastos/gnucash-to-beancount" "" "community"

# 22. beancount/beancount2ledger
fetch_repo "beancount2ledger" "https://github.com/beancount/beancount2ledger" "" "community"

# 23. deb-sig/double-entry-generator
fetch_repo "double-entry-generator" "https://github.com/deb-sig/double-entry-generator" "" "community"

# 24. davidastephens/beancount-plugins
fetch_repo "davidastephens-plugins" "https://github.com/davidastephens/beancount-plugins" "" "community"

# 25. redstreet/beancount_reds_plugins
fetch_repo "reds-plugins" "https://github.com/redstreet/beancount_reds_plugins" "" "community"

# 26. fkarg/beancount-plugins
fetch_repo "fkarg-plugins" "https://github.com/fkarg/beancount-plugins" "" "community"

# 27. beancount/fava-plugins
fetch_repo "fava-plugins" "https://github.com/beancount/fava-plugins" "" "community"

# 28. jamatute/beancount-importer
fetch_repo "jamatute-importer" "https://github.com/jamatute/beancount-importer" "" "community"

# 29. gpaulissen/beancount-import-copy
fetch_repo "beancount-import-copy" "https://github.com/gpaulissen/beancount-import-copy" "" "beancount-import"

# 30. beancount/beanprice
fetch_repo "beanprice" "https://github.com/beancount/beanprice" "" "community"

# 31. LaunchPlatform/beanhub-import
fetch_repo "beanhub-import" "https://github.com/LaunchPlatform/beanhub-import" "" "community"

# 32. LaunchPlatform/beanhub-extract
fetch_repo "beanhub-extract" "https://github.com/LaunchPlatform/beanhub-extract" "" "community"

# 33. simonmichael/hledger (beancount examples)
fetch_repo "hledger" "https://github.com/simonmichael/hledger" "" "community"

# 34. cantino/mcfly (if it has beancount)
fetch_repo "autobean" "https://github.com/SEIAROTg/autobean" "" "community"

echo ""
echo "=== Summary ==="
echo ""
echo "Files collected by source:"
total=0
for dir in "$DEST"/*/; do
    count=$(find "$dir" -name "*.beancount" -type f 2>/dev/null | wc -l)
    dirname=$(basename "$dir")
    printf "  %-20s %4d files\n" "$dirname:" "$count"
    total=$((total + count))
done
echo ""
echo "Total new files: $total"

# Count curated files
curated=$(find tests/compat/files -name "*.beancount" -type f 2>/dev/null | wc -l)
echo "Curated files:   $curated (committed)"
echo "Grand total:     $((total + curated))"
echo ""
echo "Full test suite stored in: $DEST"
echo "Curated test suite in:     tests/compat/files/"
