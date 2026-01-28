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

## Running Tests

These files are tested by:
1. `rledger check` - should exit 0 (no errors)
2. Compatibility test suite - compares output with Python beancount

```bash
# Run all regression tests
for f in tests/regressions/issue-*.beancount; do
  echo "Testing $f..."
  cargo run --release -p rustledger -- check "$f"
done
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
