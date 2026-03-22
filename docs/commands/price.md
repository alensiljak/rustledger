---
title: rledger price
description: Fetch commodity prices
---

# rledger price

Fetch current and historical commodity prices from online sources.

## Usage

```bash
rledger price [OPTIONS] <COMMODITY>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `COMMODITY` | Commodity symbol (e.g., AAPL, BTC, EUR) |

## Options

| Option | Description |
|--------|-------------|
| `-d, --date <DATE>` | Fetch price for specific date |
| `-c, --currency <CURRENCY>` | Quote currency (default: USD) |
| `-s, --source <SOURCE>` | Price source: `yahoo`, `coinbase` |
| `-o, --output <FILE>` | Output file (stdout if not specified) |
| `--from-file <FILE>` | Read commodities from ledger file |

## Examples

### Fetch Single Price

```bash
rledger price AAPL
```

Output:
```
2024-03-15 price AAPL 172.50 USD
```

### Historical Price

```bash
rledger price AAPL -d 2024-01-15
```

### Different Currency

```bash
rledger price EUR -c USD
```

### Cryptocurrency

```bash
rledger price BTC -s coinbase
```

### All Commodities from Ledger

```bash
rledger price --from-file ledger.beancount
```

### Append to Price File

```bash
rledger price AAPL >> prices.beancount
```

### Update Script

Create a daily price update script:

```bash
#!/bin/bash
# update-prices.sh

COMMODITIES="AAPL GOOGL MSFT BTC ETH"
PRICE_FILE="prices.beancount"

for symbol in $COMMODITIES; do
  rledger price "$symbol" >> "$PRICE_FILE"
done
```

Run with cron:

```cron
0 18 * * 1-5 /path/to/update-prices.sh
```

## Supported Sources

| Source | Commodities | Command |
|--------|-------------|---------|
| Yahoo Finance | Stocks, ETFs, forex | `--source yahoo` (default) |
| Coinbase | Cryptocurrencies | `--source coinbase` |

## See Also

- [Common Queries](../guides/common-queries.md) - Querying prices
