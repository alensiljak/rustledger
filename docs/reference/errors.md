---
title: Error Codes Reference
description: Validation errors and how to fix them
---

# Error Codes Reference

This page documents all validation errors that `rledger check` can report.

## Error Format

Errors are displayed as:

```
file.beancount:42: error[E001]: Transaction does not balance
```

Format: `file:line: error[code]: message`

## Syntax Errors

### E001: Syntax Error

**Cause**: Invalid beancount syntax.

**Example**:
```beancount
2024-01-15 * Coffee     ; Missing quotes around payee
  Expenses:Food   5.00
```

**Fix**: Correct the syntax:
```beancount
2024-01-15 * "Coffee"
  Expenses:Food   5.00 USD
  Assets:Cash    -5.00 USD
```

### E002: Invalid Date

**Cause**: Date format is incorrect.

**Example**:
```beancount
15-01-2024 * "Coffee"   ; Wrong format
```

**Fix**: Use YYYY-MM-DD format:
```beancount
2024-01-15 * "Coffee"
```

### E003: Invalid Account Name

**Cause**: Account name doesn't match required format.

**Example**:
```beancount
2024-01-15 * "Coffee"
  expenses:food   5.00 USD    ; Lowercase not allowed
```

**Fix**: Use Title:Case:Accounts:
```beancount
2024-01-15 * "Coffee"
  Expenses:Food   5.00 USD
```

Account names must:
- Start with Assets, Liabilities, Equity, Income, or Expenses
- Use colons as separators
- Have Title Case components

## Balance Errors

### E010: Transaction Does Not Balance

**Cause**: Postings don't sum to zero.

**Example**:
```beancount
2024-01-15 * "Coffee"
  Expenses:Food    5.00 USD
  Assets:Cash     -4.00 USD   ; Doesn't balance
```

**Fix**: Ensure postings sum to zero:
```beancount
2024-01-15 * "Coffee"
  Expenses:Food    5.00 USD
  Assets:Cash     -5.00 USD
```

### E011: Balance Assertion Failed

**Cause**: Account balance doesn't match assertion.

**Example**:
```beancount
2024-01-15 balance Assets:Checking  1000.00 USD
; But actual balance is 950.00 USD
```

**Fix**:
1. Check for missing transactions
2. Verify the expected amount
3. Use `rledger doctor context` to see surrounding transactions

### E012: Mixed Currencies Cannot Balance

**Cause**: Transaction has multiple currencies without cost/price.

**Example**:
```beancount
2024-01-15 * "Exchange"
  Assets:USD    100.00 USD
  Assets:EUR    -85.00 EUR    ; Can't auto-balance different currencies
```

**Fix**: Add cost or price:
```beancount
2024-01-15 * "Exchange"
  Assets:USD    100.00 USD
  Assets:EUR    -85.00 EUR @ 1.18 USD
```

## Account Errors

### E020: Account Not Opened

**Cause**: Transaction uses an account without an `open` directive.

**Example**:
```beancount
; No 'open' for Expenses:Food
2024-01-15 * "Coffee"
  Expenses:Food   5.00 USD
  Assets:Cash    -5.00 USD
```

**Fix**: Add an open directive:
```beancount
2020-01-01 open Expenses:Food
2020-01-01 open Assets:Cash  USD

2024-01-15 * "Coffee"
  Expenses:Food   5.00 USD
  Assets:Cash    -5.00 USD
```

Or use `rledger doctor missing-open` to generate them.

### E021: Account Already Opened

**Cause**: Duplicate `open` directive for same account.

**Fix**: Remove the duplicate.

### E022: Account Closed

**Cause**: Transaction on an account after its `close` date.

**Example**:
```beancount
2024-01-01 close Assets:OldBank

2024-02-15 * "Late transaction"
  Assets:OldBank   100.00 USD   ; Account is closed
```

**Fix**: Use correct account or adjust close date.

### E023: Invalid Account Currency

**Cause**: Posting uses currency not allowed for account.

**Example**:
```beancount
2020-01-01 open Assets:Bank  USD  ; Only USD allowed

2024-01-15 * "Deposit"
  Assets:Bank    100.00 EUR       ; EUR not allowed
```

**Fix**: Use allowed currency or update account declaration.

## Cost and Price Errors

### E030: Invalid Cost Specification

**Cause**: Cost syntax is incorrect.

**Example**:
```beancount
2024-01-15 * "Buy"
  Assets:Brokerage   10 AAPL {USD}  ; Missing price
```

**Fix**:
```beancount
2024-01-15 * "Buy"
  Assets:Brokerage   10 AAPL {150.00 USD}
```

### E031: Booking Error

**Cause**: Can't find lot to reduce (FIFO/LIFO mismatch).

**Example**:
```beancount
2024-01-15 * "Sell AAPL"
  Assets:Brokerage  -10 AAPL {150.00 USD}  ; No matching lot
```

**Fix**: Check cost basis matches existing lot, or use `{}` for automatic matching.

## Plugin Errors

### E040: Plugin Error

**Cause**: A plugin reported an error.

Check the error message for plugin-specific details.

### E041: Unknown Plugin

**Cause**: Plugin specified in file is not available.

**Fix**: Check plugin name spelling or ensure it's installed.

## Duplicate Errors

### E050: Duplicate Transaction

**Cause**: `noduplicates` plugin found matching transaction.

**Fix**: Remove duplicate or add distinguishing metadata.

### E051: Duplicate Price

**Cause**: `unique_prices` plugin found multiple prices for same commodity on same day.

**Fix**: Keep only one price per commodity per day.

## Metadata Errors

### E060: Invalid Metadata

**Cause**: Metadata syntax error.

**Example**:
```beancount
2024-01-15 * "Coffee"
  note "missing colon"        ; Wrong
  Expenses:Food   5.00 USD
```

**Fix**:
```beancount
2024-01-15 * "Coffee"
  note: "with colon"          ; Correct
  Expenses:Food   5.00 USD
```

## Document Errors

### E070: Document Not Found

**Cause**: Document directive references missing file.

**Fix**: Ensure file exists at specified path.

## Using Error Information

### Find Context

```bash
# See what's around an error
rledger doctor context ledger.beancount 42
```

### Check Specific Account

```bash
# View account history
rledger query ledger.beancount \
  "SELECT date, narration, position WHERE account = 'Assets:Bank' ORDER BY date"
```

### Generate Missing Opens

```bash
# Auto-generate open directives
rledger doctor missing-open ledger.beancount
```

## See Also

- [check command](../commands/check.md) - Running validation
- [doctor command](../commands/doctor.md) - Debugging tools
- [Plugins](plugins.md) - Plugin-specific errors
