#!/usr/bin/env python3
"""
Analyze beancount compatibility test results.

Usage:
    python scripts/analyze-compat-results.py [results.jsonl]
"""

import json
import sys
from pathlib import Path
from collections import defaultdict

def load_results(path: Path) -> list[dict]:
    """Load JSONL results file."""
    results = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    results.append(json.loads(line))
                except json.JSONDecodeError:
                    continue
    return results


def categorize_mismatches(results: list[dict]) -> dict:
    """Categorize mismatches by type."""
    categories = defaultdict(list)

    for r in results:
        if r.get("match", True):
            continue

        py_exit = r["python"]["exit"]
        rs_exit = r["rust"]["exit"]
        file = r["file"]

        if py_exit == 0 and rs_exit != 0:
            # Python passes, Rust fails
            rs_stderr = r["rust"].get("stderr", "")
            if "balance" in rs_stderr.lower():
                categories["rust_balance_error"].append(file)
            elif "validation" in rs_stderr.lower():
                categories["rust_validation_error"].append(file)
            elif "parse" in rs_stderr.lower() or "syntax" in rs_stderr.lower():
                categories["rust_parse_error"].append(file)
            else:
                categories["rust_other_error"].append(file)
        elif py_exit != 0 and rs_exit == 0:
            # Python fails, Rust passes
            py_stderr = r["python"].get("stderr", "")
            if "plugin" in py_stderr.lower() or "Plugin" in file:
                categories["python_plugin_error"].append(file)
            elif "option" in py_stderr.lower() or "Option" in file:
                categories["python_option_error"].append(file)
            elif "deprecated" in py_stderr.lower() or "Deprecated" in file:
                categories["python_deprecated_error"].append(file)
            elif "push" in py_stderr.lower() or "pop" in py_stderr.lower():
                categories["python_pushpop_error"].append(file)
            else:
                categories["python_other_error"].append(file)
        else:
            # Both fail with different errors
            categories["both_fail_different"].append(file)

    return categories


def analyze_by_source(results: list[dict]) -> dict:
    """Analyze results by source directory."""
    sources = defaultdict(lambda: {"total": 0, "match": 0, "mismatch": 0})

    for r in results:
        file = r["file"]
        # Extract source from path
        parts = file.split("/")
        if len(parts) > 1:
            source = parts[0]
        else:
            source = "root"

        sources[source]["total"] += 1
        if r.get("match", True):
            sources[source]["match"] += 1
        else:
            sources[source]["mismatch"] += 1

    return sources


def main():
    # Find results file
    if len(sys.argv) > 1:
        results_path = Path(sys.argv[1])
    else:
        results_dir = Path("spec/fixtures/compat-results")
        results_files = sorted(results_dir.glob("results_*.jsonl"), reverse=True)
        if not results_files:
            print("No results files found")
            sys.exit(1)
        results_path = results_files[0]

    print(f"Analyzing: {results_path}")
    print()

    results = load_results(results_path)

    # Basic stats
    total = len(results)
    matches = sum(1 for r in results if r.get("match", True))
    mismatches = total - matches

    print(f"## Summary")
    print(f"- Total files: {total}")
    print(f"- Matches: {matches} ({100*matches//total}%)")
    print(f"- Mismatches: {mismatches} ({100*mismatches//total}%)")
    print()

    # Categorize mismatches
    categories = categorize_mismatches(results)

    print("## Mismatch Categories")
    print()
    print("### Rust fails where Python passes:")
    for cat in ["rust_balance_error", "rust_validation_error", "rust_parse_error", "rust_other_error"]:
        if categories[cat]:
            print(f"- **{cat}**: {len(categories[cat])} files")

    print()
    print("### Python fails where Rust passes:")
    for cat in ["python_plugin_error", "python_option_error", "python_deprecated_error", "python_pushpop_error", "python_other_error"]:
        if categories[cat]:
            print(f"- **{cat}**: {len(categories[cat])} files")

    if categories["both_fail_different"]:
        print()
        print(f"### Both fail differently: {len(categories['both_fail_different'])} files")

    print()
    print("## By Source Directory")
    print()
    sources = analyze_by_source(results)
    print("| Source | Total | Match | Mismatch | Rate |")
    print("|--------|-------|-------|----------|------|")
    for source, stats in sorted(sources.items(), key=lambda x: -x[1]["total"]):
        rate = 100 * stats["match"] // stats["total"] if stats["total"] > 0 else 0
        print(f"| {source} | {stats['total']} | {stats['match']} | {stats['mismatch']} | {rate}% |")

    print()
    print("## Sample Mismatches")
    print()

    # Show a few examples from each category
    for cat, files in categories.items():
        if files:
            print(f"### {cat}")
            for f in files[:3]:
                print(f"- `{f}`")
            if len(files) > 3:
                print(f"- ... and {len(files) - 3} more")
            print()


if __name__ == "__main__":
    main()
