---
on:
  schedule: weekly on wednesday
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  create-pull-request:

---

# BQL Test Generator

Generate new test cases for the Beancount Query Language (BQL) engine to improve coverage.

## Context

rustledger implements BQL (Beancount Query Language) in `crates/rustledger-query/`. The query engine supports:
- SELECT statements with various columns
- WHERE clauses with expressions
- GROUP BY and ORDER BY
- Aggregate functions (SUM, COUNT, FIRST, LAST, etc.)
- Date functions and arithmetic

Key files:
- `crates/rustledger-query/src/executor.rs` - Query execution
- `crates/rustledger-query/src/parser.rs` - BQL parser
- `crates/rustledger-query/src/completions.rs` - Completions
- `crates/rustledger-query/tests/` - Existing tests

## Instructions

1. **Analyze Current Coverage**
   - Read existing BQL tests in `crates/rustledger-query/tests/`
   - Identify which functions and features have tests
   - Note any gaps in coverage

2. **Identify Test Gaps**
   Look for missing tests in:
   - Edge cases (empty results, single row, large datasets)
   - All aggregate functions
   - Complex WHERE expressions
   - Date range queries
   - Unicode in account names and metadata
   - Error cases (invalid queries)

3. **Generate Test Cases**
   Create new test cases that:
   - Cover identified gaps
   - Include both positive and negative tests
   - Use realistic beancount data
   - Are well-documented with comments

4. **Create PR**
   - Add tests to appropriate test files
   - Group related tests together
   - Include comment explaining what each test covers

## Output Format

PR title: `test(query): add BQL test cases for {feature}`

Include:
- New test file or additions to existing tests
- Comment in PR explaining coverage improvement
- Any edge cases discovered during analysis