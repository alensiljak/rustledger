# Regression Tests

This directory contains beancount files from user-reported issues to prevent regressions.

## Purpose

When a user reports a bug with a reproducible beancount file, we:
1. Add the file here (named after the issue number)
2. Fix the bug
3. Keep the test to prevent regressions

## File Format

Each file should:
- Be named `issue-NNN.beancount` where NNN is the GitHub issue number
- Include a header comment with:
  - Link to the original issue
  - Brief description of the bug
  - Expected behavior

Example:
```beancount
; Issue: https://github.com/rustledger/rustledger/issues/123
; Description: Cost without currency should infer from context
; Expected: No errors (matches Python beancount behavior)

2026-01-01 open Assets:Stock STOCK
...
```

## Inline Assertions

Files can include `; ASSERT:` comments for content-based verification (not just exit codes):

```beancount
; ASSERT: no_errors
; ASSERT: error_count == 0
; ASSERT: check_stderr !contains "ambiguous"
; ASSERT: check_stderr contains "warning"
; ASSERT: query "SELECT DISTINCT account" contains "Equity:Currency:USD"
; ASSERT: query "SELECT DISTINCT account" row_count == 4
```

Files without assertions fall back to exit-code-only checking (exit 0 = pass).

## Running Tests

```bash
# Run all regression tests (with assertions)
./scripts/test-regressions.sh

# Run with a specific binary
./scripts/test-regressions.sh ./target/debug/rledger
```

## Adding New Tests

When adding a test from a new issue:

1. Create `issue-NNN.beancount` with the minimal reproducer
2. Verify it fails before fixing (if not yet fixed)
3. Fix the bug
4. Verify the test passes
5. Commit both the fix and the test

## Index

| Issue | Description | Status |
|-------|-------------|--------|
| [#178](https://github.com/rustledger/rustledger/issues/178) | Posting amount inference with costs | Fixed |
| [#179](https://github.com/rustledger/rustledger/issues/179) | Indented comments in metadata | Fixed |
| [#203](https://github.com/rustledger/rustledger/issues/203) | Cost currency inference | Fixed |
| [#223](https://github.com/rustledger/rustledger/issues/223) | Lot matching with reverse-chronological order | Fixed |
| [#230](https://github.com/rustledger/rustledger/issues/230) | Cost without currency lot matching | Fixed |
| [#251](https://github.com/rustledger/rustledger/issues/251) | Tolerance handling | Fixed |
| [#273](https://github.com/rustledger/rustledger/issues/273) | Inventory lots with same price but different dates | Fixed |
| [#274](https://github.com/rustledger/rustledger/issues/274) | Transaction interpolation rounding issue | Fixed |
| [#276](https://github.com/rustledger/rustledger/issues/276) | Multi-posting lot boundary crossing | Fixed |
| [#277](https://github.com/rustledger/rustledger/issues/277) | CostSpec serialization in Python compat layer | Fixed |
| [#278](https://github.com/rustledger/rustledger/issues/278) | Zerosum plugin duplicate Open directives | Fixed |
| [#279](https://github.com/rustledger/rustledger/issues/279) | Crypto FIFO with multi-lot sales | Fixed |
| [#283](https://github.com/rustledger/rustledger/issues/283) | BQL convert() should use latest price | Fixed |
| [#333](https://github.com/rustledger/rustledger/issues/333) | Cost spec scale in interpolation | Fixed |
| [#365](https://github.com/rustledger/rustledger/issues/365) | Glob patterns in include directives | Fixed |
| [#516](https://github.com/rustledger/rustledger/issues/516) | coherent_cost plugin false positive on cost+price | Fixed |
| [#520](https://github.com/rustledger/rustledger/issues/520) | currency_accounts missing Open directives | Fixed |
| [#521](https://github.com/rustledger/rustledger/issues/521) | currency_accounts should use cost/price currency | Fixed |
| [#532](https://github.com/rustledger/rustledger/issues/532) | BQL regex case-insensitive matching | Fixed |
| [#593](https://github.com/rustledger/rustledger/issues/593) | BQL cost() and value() functions | Fixed |
