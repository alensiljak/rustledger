---
on:
  push:
    branches: [main]
    paths:
      - 'crates/**/*.rs'
      - 'Cargo.toml'
      - 'Cargo.lock'
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  create-issue:
    max: 1
  add-comment:
    max: 1

---

# Performance Regression Monitor

Monitor performance benchmarks and alert on regressions.

## Context

rustledger aims to be 10-30x faster than Python beancount. We track performance through:
- Benchmark files in `tests/compat/files/`
- The `benchmarks` branch stores historical data
- Key operations: parsing, validation, booking, queries

Performance-critical crates:
- `rustledger-parser` - Must handle large files efficiently
- `rustledger-booking` - Inventory operations
- `rustledger-query` - Query execution

## Instructions

1. **Check Recent Changes**
   - Identify what changed in the triggering commit
   - Determine if changes affect performance-critical paths

2. **Analyze Performance Impact**
   For changes to critical paths:
   - Look for algorithmic complexity changes
   - Check for new allocations in hot paths
   - Identify potential cache misses
   - Review any new dependencies

3. **Compare Against Benchmarks**
   - Check the `benchmarks` branch for historical data
   - Look at CI benchmark results if available
   - Note any significant deviations

4. **Report Findings**
   If regression detected:
   - Create issue detailing the regression
   - Include specific commit and file changes
   - Suggest potential fixes

   If performance improvement found:
   - Comment on the PR/commit acknowledging improvement

## Performance Baselines

Key metrics to monitor:
- Parse time for 10k transaction file: < 100ms
- Validation time: < 50ms for typical ledger
- Query execution: < 10ms for simple queries
- Memory usage: Should not exceed 2x input file size

## Output Format

Issue title: `[Perf] Regression detected in {component}`

Include:
- Affected component and commit
- Expected vs observed performance
- Specific code changes causing regression
- Suggested investigation or fix