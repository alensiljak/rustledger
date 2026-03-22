---
title: rledger report
description: Generate financial reports
---

# rledger report

Generate standard financial reports from your ledger.

## Usage

```bash
rledger report [OPTIONS] [FILE] <SUBCOMMAND>
```

## Subcommands

| Command | Alias | Description |
|---------|-------|-------------|
| `balances` | | All account balances |
| `balsheet` | `bal` | Balance sheet (Assets, Liabilities, Equity) |
| `income` | `is` | Income statement (Income, Expenses) |
| `journal` | `register` | Transaction register |
| `holdings` | | Investment holdings with cost basis |
| `networth` | | Net worth over time |
| `accounts` | | List all accounts |
| `commodities` | | List all currencies/commodities |
| `prices` | | List price entries |
| `stats` | | Ledger statistics |

## Global Options

| Option | Description |
|--------|-------------|
| `-P, --profile <PROFILE>` | Use a profile from config |
| `-f, --format <FORMAT>` | Output: `text`, `csv`, `json` |
| `-v, --verbose` | Show verbose output |

## Examples

### Account Balances

```bash
rledger report balances ledger.beancount
```

Filter by account:

```bash
rledger report balances -a Expenses ledger.beancount
rledger report balances -a Assets:Bank ledger.beancount
```

### Balance Sheet

```bash
rledger report balsheet ledger.beancount
# or
rledger report bal ledger.beancount
```

Output:
```
Assets
  Bank:Checking         5,234.00 USD
  Bank:Savings         12,000.00 USD
  Investments           8,500.00 USD
───────────────────────────────────────
Total Assets           25,734.00 USD

Liabilities
  CreditCard              -450.00 USD
───────────────────────────────────────
Total Liabilities         -450.00 USD

Net Worth              25,284.00 USD
```

### Income Statement

```bash
rledger report income ledger.beancount
# or
rledger report is ledger.beancount
```

### Transaction Journal

```bash
# All transactions
rledger report journal ledger.beancount

# Filter by account
rledger report journal -a Expenses:Food ledger.beancount

# Limit entries
rledger report journal -l 20 ledger.beancount
```

### Holdings

```bash
rledger report holdings ledger.beancount
```

Output:
```
Account                   Units     Cost Basis    Market Value    Gain/Loss
─────────────────────────────────────────────────────────────────────────────
Assets:Brokerage:AAPL     10.00     1,500.00 USD   1,750.00 USD   +250.00 USD
Assets:Brokerage:GOOGL     5.00     2,000.00 USD   2,100.00 USD   +100.00 USD
```

### Net Worth Over Time

```bash
rledger report networth ledger.beancount
```

### Statistics

```bash
rledger report stats ledger.beancount
```

Output:
```
Ledger Statistics
─────────────────
Transactions:     1,234
Accounts:            45
Commodities:          3
Directives:       1,456
Date range:       2020-01-01 to 2024-03-15
```

### Output Formats

```bash
# CSV for spreadsheets
rledger report balances -f csv ledger.beancount > balances.csv

# JSON for scripts
rledger report balances -f json ledger.beancount | jq '.'
```

## See Also

- [query](query.md) - Custom queries with BQL
- [Common Queries](../guides/common-queries.md) - More report examples
