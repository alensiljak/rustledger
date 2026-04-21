# rustledger

Drop-in replacement for Beancount CLI tools. Pure Rust, 10-30x faster.

## Commands

| Command | Description |
|---------|-------------|
| `rledger check` | Validate ledger files |
| `rledger query` | Run BQL queries |
| `rledger format` | Auto-format beancount files |
| `rledger report` | Generate reports (balances, stats) |
| `rledger doctor` | Debug ledger issues |
| `rledger extract` | Import from CSV/OFX |
| `rledger price` | Fetch commodity prices |

## Compatibility

For Python beancount compatibility (`bean-check`, `bean-query`, etc.), install wrapper scripts:

```bash
rledger compat install
```

## Install

```bash
cargo install rustledger
```

## Example

```bash
rledger check ledger.beancount
rledger query ledger.beancount "SELECT account, SUM(position) GROUP BY account"
rledger format --in-place ledger.beancount
```

## Cargo Features

- `python-plugin-wasm` (default) - Enable Python plugin support via WASM sandbox

## License

GPL-3.0
