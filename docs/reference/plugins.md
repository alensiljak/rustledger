---
title: Plugins Reference
description: Available plugins and configuration
---

# Plugins Reference

rustledger supports native Rust plugins for validation and transformation.

## Using Plugins

Enable plugins in your beancount file:

```beancount
plugin "beancount.plugins.auto_accounts"
plugin "beancount.plugins.implicit_prices"
```


## Built-in Plugins

### Validation Plugins

| Plugin | Description |
|--------|-------------|
| `check_commodity` | Ensure commodities are declared |
| `coherent_cost` | Validate cost specifications |
| `leafonly` | Restrict postings to leaf accounts |
| `noduplicates` | Detect duplicate transactions |
| `onecommodity` | One commodity per account |
| `pedantic` | Strict validation rules |
| `unique_prices` | One price per commodity per day |

### Transformation Plugins

| Plugin | Description |
|--------|-------------|
| `auto_accounts` | Auto-generate Open directives |
| `close_tree` | Recursively close account trees |
| `implicit_prices` | Generate prices from transactions |
| `sellgains` | Auto-generate capital gains postings |
| `split_expenses` | Split shared expenses |

## Plugin Details

### auto_accounts

Automatically generates `Open` directives for any accounts used in transactions.

```beancount
plugin "beancount.plugins.auto_accounts"

; No need for explicit Open directives
2024-01-15 * "Coffee"
  Expenses:Food:Coffee   5.00 USD
  Assets:Cash           -5.00 USD
```

Options: None

### check_commodity

Ensures all commodities are explicitly declared.

```beancount
plugin "beancount.plugins.check_commodity"

2020-01-01 commodity USD
2020-01-01 commodity AAPL
```

Options: None

### coherent_cost

Validates that cost specifications are consistent with prices.

```beancount
plugin "beancount.plugins.coherent_cost"
```

This plugin checks that when both cost and price are specified, they're coherent (e.g., for capital gains transactions).

Options: None

### implicit_prices

Generates price directives from transaction costs.

```beancount
plugin "beancount.plugins.implicit_prices"

2024-01-15 * "Buy stock"
  Assets:Brokerage   10 AAPL {150.00 USD}
  Assets:Cash       -1500.00 USD

; Generates:
; 2024-01-15 price AAPL 150.00 USD
```

Options: None

### leafonly

Restricts postings to leaf accounts only. Reports an error if you post to an account that has sub-accounts.

```beancount
plugin "beancount.plugins.leafonly"
```

```beancount
; ERROR: Cannot post to parent account
2024-01-15 * "Coffee"
  Expenses   5.00 USD    ; Error if Expenses:Food exists
  Assets:Cash

; OK: Posting to leaf account
2024-01-15 * "Coffee"
  Expenses:Food:Coffee   5.00 USD
  Assets:Cash
```

Options: None

### noduplicates

Detects duplicate transactions based on date, payee, narration, and amounts.

```beancount
plugin "beancount.plugins.noduplicates"
```

Options: None

### onecommodity

Ensures each account only holds one commodity.

```beancount
plugin "beancount.plugins.onecommodity"

2020-01-01 open Assets:Bank:USD   USD
2020-01-01 open Assets:Bank:EUR   EUR
```

Options: None

### pedantic

Enables strict validation:
- Requires explicit tags on all transactions
- Requires payees on all transactions
- Other strict checks

```beancount
plugin "beancount.plugins.pedantic"
```

Options: None

### sellgains

Automatically generates capital gains postings for sales.

```beancount
plugin "beancount.plugins.sellgains"

2024-01-15 * "Sell AAPL"
  Assets:Brokerage  -10 AAPL {} @ 175.00 USD
  Assets:Cash       1750.00 USD
  ; Auto-generates:
  ; Income:CapitalGains   -250.00 USD
```

Options: None

### split_expenses

Splits expenses among multiple people.

```beancount
plugin "beancount.plugins.split_expenses" "mark,john 50,50"

2024-01-15 * "Dinner" #shared
  Expenses:Food      100.00 USD
  Assets:Cash
```

Options: `"person1,person2 share1,share2"`

### unique_prices

Ensures only one price directive per commodity per day.

```beancount
plugin "beancount.plugins.unique_prices"
```

Options: None

## Plugin Order

Plugins run in the order specified:

```beancount
; auto_accounts runs first, creating Opens
plugin "beancount.plugins.auto_accounts"

; Then implicit_prices generates price directives
plugin "beancount.plugins.implicit_prices"

; Finally, validation plugins check the result
plugin "beancount.plugins.check_commodity"
```

## Writing Custom Plugins

### Native Rust Plugin

Add to `crates/rustledger-plugin/src/native/plugins/`:

```rust
// my_plugin.rs
use crate::{NativePlugin, PluginInput, PluginOutput};

pub struct MyPlugin;

impl NativePlugin for MyPlugin {
    fn name(&self) -> &'static str {
        "my_plugin"
    }

    fn description(&self) -> &'static str {
        "Description of what the plugin does"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        // input.directives - the directives to process
        // input.config - optional config string from plugin directive
        // Return PluginOutput with modified directives and/or errors
        PluginOutput {
            directives: input.directives,
            errors: vec![],
        }
    }
}
```

Register in `mod.rs`:

```rust
registry.register(Box::new(MyPlugin));
```

## Compatibility with Python Beancount

Most Python beancount plugins have native equivalents:

| Python Plugin | rustledger Equivalent |
|---------------|----------------------|
| `auto_accounts` | ✅ `auto_accounts` |
| `implicit_prices` | ✅ `implicit_prices` |
| `check_commodity` | ✅ `check_commodity` |
| `leafonly` | ✅ `leafonly` |
| `noduplicates` | ✅ `noduplicates` |
| `onecommodity` | ✅ `onecommodity` |
| `sellgains` | ✅ `sellgains` |
| Custom Python | Rewrite in Rust |

## See Also

- [check command](../commands/check.md) - Running validation
- [Migration](../migration/from-beancount.md) - Python plugin migration
