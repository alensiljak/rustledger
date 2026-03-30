---
title: rledger extract
description: Import transactions from bank statements
---

# rledger extract

Import transactions from CSV and OFX bank statements.

## Usage

```bash
rledger extract [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | CSV or OFX file to import |

## Options

| Option | Description |
|--------|-------------|
| `-i, --importer <NAME>` | Use a named importer from `importers.toml` |
| `--importers-config <FILE>` | Path to `importers.toml` (auto-discovered by default) |
| `-a, --account <ACCOUNT>` | Target account |
| `-c, --currency <CURRENCY>` | Currency for amounts (default: USD) |
| `--existing <FILE>` | Existing ledger (for duplicate detection) |

## Examples

### Basic CSV Import

```bash
rledger extract bank-statement.csv -a Assets:Bank:Checking
```

### With Configuration

Create `importers.toml`:

```toml
[[importers]]
name = "chase"
account = "Assets:Bank:Chase"
date_column = 0
narration_column = 2
amount_column = 3
date_format = "%m/%d/%Y"
skip_header = true

[importers.mappings]
"AMAZON" = "Expenses:Shopping"
"WHOLE FOODS" = "Expenses:Food:Groceries"
"SHELL" = "Expenses:Transport:Gas"
```

```bash
rledger extract --importer chase chase-statement.csv
```

### OFX Import

```bash
rledger extract statement.ofx -a Assets:Bank:Checking
```

### Append to Ledger

```bash
rledger extract statement.csv -a Assets:Bank >> ledger.beancount
```

### Duplicate Detection

```bash
# Skip transactions already in ledger
rledger extract statement.csv -a Assets:Bank --existing ledger.beancount
```

## Importer Configuration

### CSV Options

```toml
[[importers]]
name = "my_bank"
account = "Assets:Bank:MyBank"

# Column mapping (0-indexed)
date_column = 0
payee_column = 1
narration_column = 2
amount_column = 3

# Or use column names (if CSV has header)
date_column = "Date"
amount_column = "Amount"

# Date parsing
date_format = "%Y-%m-%d"  # or "%m/%d/%Y", "%d.%m.%Y"

# Skip header row
skip_header = true

# Invert amounts (for credit card statements)
invert_amounts = true

# Default expense account
default_expense = "Expenses:Unknown"

# Pattern-based account mapping
[importers.mappings]
"GROCERY" = "Expenses:Food:Groceries"
"GAS STATION" = "Expenses:Transport:Gas"
"PAYROLL" = "Income:Salary"
```

### Multiple Importers

```toml
[[importers]]
name = "checking"
account = "Assets:Bank:Checking"
# ...

[[importers]]
name = "credit_card"
account = "Liabilities:CreditCard"
invert_amounts = true
# ...
```

Use with:

```bash
rledger extract --importer checking statement.csv
```

The `importers.toml` file is auto-discovered from the current directory or `~/.config/rledger/`. To specify a custom path:

```bash
rledger extract --importers-config path/to/importers.toml --importer checking statement.csv
```

## See Also

- [Importing Guide](../guides/importing.md) - Detailed import tutorial
