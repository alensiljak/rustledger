---
title: rledger query
description: Run BQL queries on your ledger
---

# rledger query

Run BQL (Beancount Query Language) queries against your ledger.

## Usage

```bash
rledger query [OPTIONS] [FILE] [QUERY]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | The beancount file (uses `$RLEDGER_FILE` or config if not specified) |
| `QUERY` | BQL query string (interactive mode if not specified) |

## Options

| Option | Description |
|--------|-------------|
| `-P, --profile <PROFILE>` | Use a profile from config |
| `-f, --format <FORMAT>` | Output format: `text`, `csv`, `json` |
| `-v, --verbose` | Show verbose output |

## Examples

### One-shot Query

```bash
rledger query ledger.beancount "SELECT account, sum(position) GROUP BY account"
```

### Interactive Mode

```bash
rledger query ledger.beancount
```

Opens an interactive shell with tab completion:

```
rledger> SELECT date, narration WHERE account ~ 'Expenses'
rledger> BALANCES
rledger> .exit
```

### Output Formats

```bash
# Text (default)
rledger query ledger.beancount "BALANCES"

# CSV (for spreadsheets)
rledger query ledger.beancount "BALANCES" -f csv > balances.csv

# JSON (for scripts)
rledger query ledger.beancount "BALANCES" -f json | jq '.rows[]'
```

### Common Queries

```bash
# Account balances
rledger query ledger.beancount "BALANCES"

# Expenses by category
rledger query ledger.beancount "
  SELECT account, sum(position)
  WHERE account ~ 'Expenses'
  GROUP BY account
  ORDER BY sum(position) DESC
"

# Recent transactions
rledger query ledger.beancount "
  SELECT date, payee, narration, account, position
  ORDER BY date DESC
  LIMIT 20
"

# Transactions matching pattern
rledger query ledger.beancount "
  SELECT date, narration, position
  WHERE narration ~ 'Coffee'
"

# Monthly expenses
rledger query ledger.beancount "
  SELECT month(date) as month, sum(number) as total
  WHERE account ~ 'Expenses'
  GROUP BY month
  ORDER BY month
"
```

### Using with Pipes

```bash
# Filter output with grep
rledger query ledger.beancount "BALANCES" | grep Expenses

# Process JSON with jq
rledger query ledger.beancount "BALANCES" -f json | jq '.rows[] | select(.balance > 100)'

# Export to file
rledger query ledger.beancount "SELECT *" -f csv > transactions.csv
```

## BQL Reference

See [BQL Reference](../reference/bql.md) for complete query language documentation.

### Quick Reference

```sql
-- Select columns
SELECT date, narration, account, position

-- Filter rows
WHERE account ~ 'Expenses' AND year(date) = 2024

-- Aggregate
GROUP BY account
HAVING sum(number) > 100

-- Sort and limit
ORDER BY date DESC
LIMIT 10

-- Built-in reports
BALANCES [FROM ...]
JOURNAL 'Account:Name'
```

## See Also

- [BQL Reference](../reference/bql.md) - Full query language docs
- [Common Queries](../guides/common-queries.md) - Useful query examples
