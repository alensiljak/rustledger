---
on:
  schedule: weekly on monday
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  create-issue:
    max: 1

---

# Compatibility Test Analyst

Analyze rustledger's compatibility test results against Python beancount and provide insights on test failures, patterns, and potential fixes.

## Context

rustledger is a Rust implementation of Beancount (double-entry bookkeeping). We maintain compatibility tests that compare our output against Python beancount. Currently we have 694 tests with a 99.86% pass rate.

Key files:
- `tests/compat/` - Compatibility test infrastructure
- `scripts/compat-test.sh` - Test runner script
- `crates/rustledger-validate/` - Validation logic
- `crates/rustledger-booking/` - Booking engine

## Instructions

1. **Gather Test Results**
   - Check recent CI runs for compatibility test results
   - Look at the `compatibility` branch for historical test data
   - Identify any failing tests from recent runs

2. **Analyze Failures**
   - For each failing test, identify:
     - The specific beancount file causing the failure
     - Whether it's a parsing, validation, or booking difference
     - If it's a known limitation (like decimal precision)
   - Look for patterns across failures

3. **Check Known Limitations**
   - Review `CLAUDE.md` for documented limitations (e.g., decimal precision)
   - Determine if failures are expected or regressions

4. **Generate Report**
   Create an issue with:
   - Summary of test pass rate
   - List of failing tests with categories
   - Pattern analysis (are failures related?)
   - Suggested fixes or investigations
   - Priority recommendations

## Output Format

Title: `[Compat Analysis] Weekly Report - {date}`

Include sections:
- **Summary**: Overall pass rate and trend
- **New Failures**: Any tests that started failing recently
- **Known Limitations**: Failures due to documented differences
- **Actionable Items**: Specific fixes or investigations needed
- **Trends**: Changes over time if data available