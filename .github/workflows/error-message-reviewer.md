---
on:
  pull_request:
    types: [opened, synchronize]
    paths:
      - 'crates/**/src/**/*.rs'
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  add-comment:
    max: 1

---

# Error Message Quality Reviewer

Review error messages in PRs for clarity, consistency, and helpfulness.

## Context

rustledger has validation errors defined in `crates/rustledger-validate/src/lib.rs` with 27 error codes. Good error messages should:
- Clearly state what went wrong
- Include relevant context (line numbers, account names, amounts)
- Suggest how to fix the issue when possible
- Be consistent in style across the codebase

Error message locations:
- `rustledger-validate` - Validation errors
- `rustledger-parser` - Parse errors with spans
- `rustledger-booking` - Booking errors
- `rustledger-loader` - File loading errors

## Instructions

1. **Identify Error Messages in PR**
   - Look for new or modified error messages
   - Check `Error`, `anyhow::bail!`, `thiserror` usage
   - Find format strings in error contexts

2. **Review Each Error Message**
   Check for:
   - **Clarity**: Is it clear what went wrong?
   - **Context**: Does it include relevant details (file, line, values)?
   - **Actionability**: Does it hint at how to fix the issue?
   - **Consistency**: Does it match the style of other errors?
   - **Grammar**: Is it grammatically correct?

3. **Style Guidelines**
   - Use sentence case (capitalize first word only)
   - Include specific values when available
   - Avoid jargon when user-facing
   - Keep messages concise but complete
   - Use consistent terminology

4. **Provide Feedback**
   If issues found:
   - Comment on the PR with specific suggestions
   - Reference similar error messages for consistency
   - Provide improved wording examples

## Output Format

Comment with:
- Summary of error messages reviewed
- Specific feedback for each message needing improvement
- Suggested rewording with explanation
- Praise for well-crafted messages

Use diff blocks to show suggested changes:
```diff
- "invalid account"
+ "invalid account name '{}': account names must start with a valid root (Assets, Liabilities, Equity, Income, Expenses)"
```