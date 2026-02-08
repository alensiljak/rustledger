---
on:
  push:
    branches: [main]
    paths:
      - 'crates/**/*.rs'
      - '!crates/**/tests/**'
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  create-pull-request:

---

# Documentation-Code Sync Checker

Verify that documentation matches implementation and propose updates when they diverge.

## Context

rustledger has documentation in several places:
- `CLAUDE.md` - Development context and architecture
- `README.md` - User-facing documentation
- `docs/` - Additional documentation
- Rustdoc comments in `crates/*/src/**/*.rs`

Key public APIs to monitor:
- `rustledger-core` - Core types (Amount, Position, Inventory)
- `rustledger-parser` - Lexer and parser
- `rustledger-validate` - Validation with 27 error codes
- `rustledger-booking` - 7 booking methods
- `rustledger-query` - BQL query engine
- `rustledger-plugin` - 20 native plugins

## Instructions

1. **Identify Changed Code**
   - Look at the files changed in the triggering commit
   - Focus on public API changes (pub fn, pub struct, pub enum)

2. **Check Documentation Alignment**
   - For changed public items, verify rustdoc comments are accurate
   - Check if README.md mentions affected features
   - Verify CLAUDE.md architecture descriptions match

3. **Specific Checks**
   - If validation error codes changed, verify they're documented
   - If booking methods changed, check booking.md
   - If CLI commands changed, check README usage examples
   - If plugin behavior changed, check plugin documentation

4. **Propose Updates**
   If documentation is outdated:
   - Create a PR with documentation fixes
   - Include clear explanation of what changed
   - Reference the commit that changed the implementation

## Output Format

PR title: `docs: sync documentation with {component} changes`

PR body should include:
- What implementation changed
- What documentation was outdated
- Summary of documentation updates made