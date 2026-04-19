______________________________________________________________________

## title: Importing Data description: Import transactions from bank statements

# Importing Data

Import transactions from CSV and OFX bank statements into beancount format.

## Quick Start

```bash
# Basic CSV import
rledger extract bank-statement.csv -a Assets:Bank:Checking

# With duplicate detection
rledger extract statement.csv -a Assets:Bank --existing ledger.beancount

# Append to ledger
rledger extract statement.csv -a Assets:Bank >> ledger.beancount
```

## CSV Import

### Basic Usage

Most bank CSV exports work with minimal configuration:

```bash
rledger extract chase-statement.csv -a Assets:Bank:Chase
```

The importer auto-detects common CSV formats and column layouts.

### Custom Configuration

For non-standard formats, create `importers.toml`:

```toml
[[importers]]
name = "chase"
account = "Assets:Bank:Chase"

# Column mapping (0-indexed or by header name)
date_column = 0
payee_column = 1
narration_column = 2
amount_column = 3

# Date format
date_format = "%m/%d/%Y"

# Options
skip_header = true
invert_amounts = false  # Set true for credit cards

# Default for unmatched transactions
default_expense = "Expenses:Unknown"
```

Use with:

```bash
rledger extract --importer chase chase-statement.csv
```

The `importers.toml` file is searched for automatically in these locations (first found wins):

1. Path specified via `--importers-config path/to/importers.toml`
1. `importers.toml` in the current directory
1. `~/.config/rledger/importers.toml`

### Account Mapping

Map transaction descriptions to accounts automatically:

```toml
[[importers]]
name = "checking"
account = "Assets:Bank:Checking"
# ... other settings ...

[importers.mappings]
"AMAZON" = "Expenses:Shopping"
"WHOLE FOODS" = "Expenses:Food:Groceries"
"SHELL" = "Expenses:Transport:Gas"
"NETFLIX" = "Expenses:Entertainment:Streaming"
"PAYROLL" = "Income:Salary"
"INTEREST" = "Income:Interest"
```

Patterns are matched case-insensitively against the payee field first, then the
narration. Longer patterns are matched first, so more specific patterns take
priority over shorter ones. The first match wins.

## OFX Import

OFX (Open Financial Exchange) files from banks import directly:

```bash
rledger extract statement.ofx -a Assets:Bank:Checking
```

OFX files contain structured data, so no column mapping is needed.

## Multiple Accounts

Configure multiple importers for different accounts:

```toml
[[importers]]
name = "checking"
account = "Assets:Bank:Checking"
date_column = "Date"
amount_column = "Amount"
narration_column = "Description"

[[importers]]
name = "credit_card"
account = "Liabilities:CreditCard:Chase"
date_column = "Trans Date"
amount_column = "Amount"
narration_column = "Description"
invert_amounts = true  # Credit card amounts need inverting

[[importers]]
name = "savings"
account = "Assets:Bank:Savings"
date_column = 0
amount_column = 3
narration_column = 1
```

Select which importer to use:

```bash
rledger extract --importer credit_card chase-card.csv
```

Or specify a custom config path:

```bash
rledger extract --importers-config path/to/importers.toml --importer credit_card chase-card.csv
```

## Duplicate Detection

Avoid importing the same transactions twice:

```bash
# Check against existing ledger
rledger extract statement.csv -a Assets:Bank --existing ledger.beancount
```

Duplicates are detected by matching:

- Date
- Amount
- Payee/narration (fuzzy match)

## Workflow

### Initial Import

```bash
# 1. Test import (preview output)
rledger extract statement.csv -a Assets:Bank

# 2. Review and append
rledger extract statement.csv -a Assets:Bank >> ledger.beancount

# 3. Validate
rledger check ledger.beancount
```

### Monthly Routine

```bash
# Download statements, then:
rledger extract march-statement.csv \
  --importer checking \
  --existing ledger.beancount \
  >> ledger.beancount

# Fix any unmatched accounts
rledger check ledger.beancount
```

### Categorization Tips

1. **Start broad**: Use `Expenses:Unknown` for unmatched
1. **Add patterns**: When you see repeated merchants, add mappings
1. **Refine over time**: Your mappings improve with each import

## Troubleshooting

### Wrong Date Format

If dates parse incorrectly, specify the format:

```toml
date_format = "%m/%d/%Y"   # US: 03/15/2024
date_format = "%d/%m/%Y"   # EU: 15/03/2024
date_format = "%Y-%m-%d"   # ISO: 2024-03-15
date_format = "%d.%m.%Y"   # German: 15.03.2024
```

### Wrong Amount Signs

Credit card statements often show purchases as positive. Invert them:

```toml
invert_amounts = true
```

### Encoding Issues

If you see garbled characters:

```bash
# Convert to UTF-8 first
iconv -f ISO-8859-1 -t UTF-8 statement.csv > statement-utf8.csv
rledger extract statement-utf8.csv -a Assets:Bank
```

### Column Detection Failed

Explicitly specify columns by index (0-based):

```toml
date_column = 0
amount_column = 3
narration_column = 2
```

Or by header name:

```toml
date_column = "Transaction Date"
amount_column = "Amount"
narration_column = "Description"
```

## See Also

- [extract command](../commands/extract.md) - Full command reference
- [doctor missing-open](../commands/doctor.md) - Generate missing account Open directives
