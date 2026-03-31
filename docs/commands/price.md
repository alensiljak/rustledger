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
| `-P, --profile <PROFILE>` | Use a profile from config |
| `-f, --file <FILE>` | Beancount file to read commodities from |
| `-c, --currency <CURRENCY>` | Base currency for price quotes [default: USD] |
| `-d, --date <DATE>` | Date for prices (YYYY-MM-DD, defaults to today) |
| `-b, --beancount` | Output as beancount price directives |
| `-m, --mapping <MAPPING>` | Yahoo Finance symbol mapping (e.g., `VTI:VTI,BTC:BTC-USD`) |
| `-v, --verbose` | Show verbose output |

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
# Use mapping to specify Yahoo Finance symbol for crypto
rledger price BTC -m "BTC:BTC-USD"
```

### All Commodities from Ledger

```bash
rledger price -f ledger.beancount
```

### Output as Beancount Directives

```bash
rledger price AAPL -b
```

### Symbol Mapping for Yahoo Finance

```bash
# Map BTC to Yahoo's BTC-USD symbol
rledger price BTC -m "BTC:BTC-USD"
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

## Price Sources

Prices are fetched from Yahoo Finance. For commodities with non-standard symbols, use the `-m` mapping option:

```bash
# Standard stock symbols work directly
rledger price AAPL GOOGL MSFT

# Cryptocurrencies need mapping to Yahoo symbols
rledger price BTC ETH -m "BTC:BTC-USD,ETH:ETH-USD"

# Mutual funds and ETFs
rledger price VTI VXUS
```

## See Also

- [Common Queries](../guides/common-queries.md) - Querying prices
