---
on:
  schedule: weekly on friday
  workflow_dispatch:

permissions:
  contents: read
  issues: read
  pull-requests: read

safe-outputs:
  create-issue:
    max: 1

---

# Weekly Project Status Report

Generate a comprehensive weekly status report for the rustledger project.

## Context

rustledger is a Rust implementation of Beancount with:
- 9 crates in a cargo workspace
- CLI tool (`rledger`) with multiple commands
- WASM library target
- 694 compatibility tests against Python beancount
- Active development with regular releases

## Instructions

1. **Gather Activity Data**
   - Count commits to main branch this week
   - List merged pull requests
   - Count new/closed issues
   - Check CI status and test results

2. **Analyze Development Velocity**
   - Compare to previous weeks if data available
   - Note any blocked PRs or stale issues
   - Identify active contributors

3. **Check Project Health**
   - Compatibility test pass rate
   - Any new test failures?
   - Dependency update status (Dependabot PRs)
   - Security advisory status

4. **Identify Trends**
   - Areas of active development
   - Recurring issues or bug patterns
   - Documentation gaps noted

5. **Generate Report**
   Create a well-formatted issue with all findings

## Output Format

Title: `[Weekly Status] {date range}`

Sections:
- **Summary**: High-level overview
- **Merged PRs**: List with brief descriptions
- **Open PRs**: Pending reviews or blocked
- **Issues**: New, closed, and trending
- **CI Health**: Test pass rates, build status
- **Dependencies**: Any updates pending
- **Next Week**: Suggested focus areas

Use tables and bullet points for clarity.
Add labels: `status-report`, `automated`