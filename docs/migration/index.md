______________________________________________________________________

## title: Migration Guides description: Migrating from other accounting tools

# Migration Guides

Guides for migrating to rustledger from other plain-text accounting tools.

## Available Guides

| Tool | Description |
|------|-------------|
| [From Beancount](from-beancount.md) | Python beancount to rustledger |
| [From Ledger](from-ledger.md) | ledger-cli to rustledger |
| [From hledger](from-hledger.md) | hledger to rustledger |

## Quick Comparison

| Feature | rustledger | beancount | ledger-cli | hledger |
|---------|------------|-----------|------------|---------|
| Language | Rust | Python | C++ | Haskell |
| Syntax | Beancount | Beancount | Ledger | Ledger-compatible |
| Speed | Fast (10-30x) | Baseline | Fast | Medium |
| Double-entry | Required | Required | Optional | Optional |
| Plugins | Native + WASM | Python | None | None |
| LSP | Yes | No | No | Yes |
| Query Language | BQL | BQL | Custom | Custom |

## Which Migration Path?

### From Beancount

If you're using Python beancount, rustledger is a drop-in replacement:

- Same file format
- Same syntax
- Same plugins (most)
- 10-30x faster

[Start migration →](from-beancount.md)

### From Ledger/hledger

If you're using ledger-cli or hledger:

- Different file format (requires conversion)
- Different syntax
- Different query language
- Strict double-entry enforcement

Consider whether strict double-entry is right for you before migrating.

[From Ledger →](from-ledger.md) | [From hledger →](from-hledger.md)
