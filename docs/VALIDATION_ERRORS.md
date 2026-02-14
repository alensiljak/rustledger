# Validation Errors Reference

rustledger validates ledgers and reports **validation errors** (codes `Exxxx`) and **parse errors** (codes `Pxxxx`).

> **Authoritative Sources:**
> - Specification: `spec/core/validation.md`
> - Implementation: `crates/rustledger-validate/src/error.rs`

## Account Errors (E1xxx)

| Code | Error | Description |
|------|-------|-------------|
| E1001 | AccountNotOpen | Account used before it was opened |
| E1002 | AccountAlreadyOpen | Duplicate open directive for same account |
| E1003 | AccountClosed | Account used after it was closed |
| E1004 | AccountCloseNonZero | Account closed with non-zero balance |
| E1005 | InvalidAccountName | Invalid account name format |

## Balance Errors (E2xxx)

| Code | Error | Description |
|------|-------|-------------|
| E2001 | BalanceAssertionFailed | Balance assertion does not match computed balance |
| E2002 | BalanceExceedsTolerance | Balance exceeds explicit tolerance |
| E2003 | PadWithoutBalance | Pad directive without subsequent balance assertion |
| E2004 | MultiplePads | Multiple pads for same balance assertion |

## Transaction Errors (E3xxx)

| Code | Error | Description |
|------|-------|-------------|
| E3001 | TransactionUnbalanced | Transaction does not balance |
| E3002 | MultipleEmptyPostings | Multiple postings missing amounts for same currency |
| E3003 | NoPostings | Transaction has no postings |
| E3004 | SinglePosting | Transaction has single posting (warning) |

## Lot/Booking Errors (E4xxx)

| Code | Error | Description |
|------|-------|-------------|
| E4001 | NoMatchingLot | No matching lot for reduction |
| E4002 | InsufficientLotUnits | Insufficient units in lot for reduction |
| E4003 | AmbiguousLotMatch | Ambiguous lot match in STRICT mode |
| E4004 | NegativeInventory | Reduction would create negative inventory |

## Currency Errors (E5xxx)

| Code | Error | Description |
|------|-------|-------------|
| E5001 | CurrencyNotDeclared | Currency not declared (when strict mode enabled) |
| E5002 | CurrencyNotAllowed | Currency not allowed in account |

## Metadata Errors (E6xxx)

| Code | Error | Description |
|------|-------|-------------|
| E6001 | DuplicateMetadataKey | Duplicate metadata key |
| E6002 | InvalidMetadataValue | Invalid metadata value type |

## Option Errors (E7xxx)

| Code | Error | Description |
|------|-------|-------------|
| E7001 | UnknownOption | Unknown option name |
| E7002 | InvalidOptionValue | Invalid option value |
| E7003 | DuplicateOption | Duplicate non-repeatable option |

## Document Errors (E8xxx)

| Code | Error | Description |
|------|-------|-------------|
| E8001 | DocumentNotFound | Document file not found |

## Info/Warnings (E10xxx)

| Code | Error | Description |
|------|-------|-------------|
| E10001 | DateOutOfOrder | Entry date is before previous entry (info only) |
| E10002 | FutureDate | Entry dated in the future (warning) |

## Parse Errors (Pxxxx)

Parse errors are reported separately from validation errors and indicate syntax issues, invalid directives, or malformed input. They are handled by the parser, not the validator.

## See Also

- [Migration Guide](MIGRATION.md) - Error message differences from Python beancount
- [BQL Reference](BQL_REFERENCE.md) - Query language for analyzing ledgers
