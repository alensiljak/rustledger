---
on:
  push:
    branches: [main]
    paths:
      - 'crates/rustledger-plugin/**'
  pull_request:
    paths:
      - 'crates/rustledger-plugin/**'
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

# Plugin Compatibility Checker

Verify native plugin behavior matches Python beancount plugins.

## Context

rustledger implements 20 native plugins in `crates/rustledger-plugin/src/native/`:
- `auto_accounts` - Auto-create accounts
- `check_commodity` - Verify commodities are declared
- `close_tree` - Close account hierarchies
- `coherent_cost` - Validate cost basis
- `fill_account` - Fill missing accounts
- `forecast` - Generate forecast transactions
- `implicit_prices` - Extract prices from transactions
- `noduplicates` - Check for duplicate transactions
- `nounused` - Warn about unused accounts
- `onecommodity` - Enforce single commodity per account
- `pedantic` - Strict validation
- `sellgains` - Calculate capital gains
- `split_expenses` - Split shared expenses
- And more...

## Instructions

1. **Identify Plugin Changes**
   - Determine which plugins were modified
   - Check if it's a bug fix, feature, or refactor

2. **Compare Against Python Behavior**
   For each modified plugin:
   - Review the Python beancount plugin implementation
   - Identify expected inputs and outputs
   - Note any documented edge cases

3. **Verify Compatibility**
   - Check existing plugin tests in `crates/rustledger-plugin/tests/`
   - Look for compatibility test files that use the plugin
   - Identify any behavioral differences

4. **Test Edge Cases**
   Consider:
   - Empty ledgers
   - Ledgers with only the plugin directive
   - Large ledgers with many transactions
   - Unicode in account names/metadata
   - Multiple plugin instances

5. **Report Findings**
   If incompatibility found:
   - Create issue detailing the difference
   - Include minimal reproduction case
   - Reference Python beancount behavior

   For PRs:
   - Comment with compatibility assessment
   - Suggest additional test cases if needed

## Output Format

Issue title: `[Plugin Compat] {plugin_name} behavior differs from Python`

Include:
- Plugin name and version
- Expected behavior (from Python)
- Actual behavior (rustledger)
- Minimal test case demonstrating difference
- Suggested fix or documentation update