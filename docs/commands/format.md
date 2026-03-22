---
title: rledger format
description: Auto-format beancount files
---

# rledger format

Automatically format beancount files for consistent style.

## Usage

```bash
rledger format [OPTIONS] [FILE]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | The beancount file to format |

## Options

| Option | Description |
|--------|-------------|
| `-i, --in-place` | Modify file in place |
| `-P, --profile <PROFILE>` | Use a profile from config |

## Examples

### Preview Formatting

```bash
# Print formatted output (doesn't modify file)
rledger format ledger.beancount
```

### Format In Place

```bash
rledger format --in-place ledger.beancount
```

### Format Multiple Files

```bash
for f in *.beancount; do
  rledger format --in-place "$f"
done
```

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
for file in $(git diff --cached --name-only | grep '\.beancount$'); do
  rledger format --in-place "$file"
  git add "$file"
done
```

## Formatting Rules

The formatter applies these rules:

- **Alignment**: Numbers aligned at the decimal point
- **Spacing**: Consistent spacing around operators
- **Indentation**: Standard 2-space indentation for postings
- **Dates**: ISO format (YYYY-MM-DD)
- **Metadata**: Aligned key-value pairs

### Before

```beancount
2024-01-15 * "Coffee shop"
  Expenses:Food     5.00USD
  Assets:Cash    -5.00 USD
    note: "morning coffee"
```

### After

```beancount
2024-01-15 * "Coffee shop"
  Expenses:Food             5.00 USD
  Assets:Cash              -5.00 USD
    note: "morning coffee"
```

## See Also

- [check](check.md) - Validate your ledger
