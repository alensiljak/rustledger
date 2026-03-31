---
title: Contributing Plugins
description: How to add native Rust plugins to rustledger
---

# Contributing Native Plugins

This guide explains how to add new native Rust plugins to rustledger itself.

::: tip For Custom Plugins
If you want to write a plugin for your own use without modifying rustledger, see the [Custom Plugins Guide](../guides/custom-plugins.md) for WASM plugins.
:::

## When to Add a Native Plugin

Add a native plugin when:
- The functionality is generally useful to many users
- You want to contribute to rustledger
- Maximum performance is critical
- The plugin implements a Python beancount plugin for compatibility

## Plugin Architecture

Native plugins live in `crates/rustledger-plugin/src/native/plugins/`:

```
crates/rustledger-plugin/
├── src/
│   ├── lib.rs
│   ├── types.rs           # PluginInput, PluginOutput, etc.
│   └── native/
│       ├── mod.rs         # NativePlugin trait
│       ├── registry.rs    # Plugin registration
│       └── plugins/
│           ├── mod.rs     # Plugin exports
│           ├── auto_accounts.rs
│           ├── implicit_prices.rs
│           ├── noduplicates.rs
│           └── your_plugin.rs  # Your new plugin
```

## Step-by-Step Guide

### 1. Create the Plugin File

Create a new file in `crates/rustledger-plugin/src/native/plugins/`:

```rust
// crates/rustledger-plugin/src/native/plugins/my_plugin.rs

use crate::native::NativePlugin;
use crate::types::{PluginError, PluginErrorSeverity, PluginInput, PluginOutput};

/// My Plugin - brief description.
///
/// Longer description explaining what the plugin does,
/// when to use it, and any configuration options.
///
/// # Example
///
/// ```beancount
/// plugin "beancount.plugins.my_plugin" "optional_config"
/// ```
pub struct MyPlugin;

impl NativePlugin for MyPlugin {
    fn name(&self) -> &'static str {
        "my_plugin"
    }

    fn description(&self) -> &'static str {
        "Brief description of what the plugin does"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        let mut directives = input.directives;
        let mut errors = Vec::new();

        // Your plugin logic here
        // Access config via: input.config
        // Access options via: input.options

        PluginOutput { directives, errors }
    }
}
```

### 2. Register the Plugin

Add your plugin to `crates/rustledger-plugin/src/native/plugins/mod.rs`:

```rust
// Add the module
mod my_plugin;

// Re-export
pub use my_plugin::MyPlugin;
```

Then register it in `crates/rustledger-plugin/src/native/registry.rs`:

```rust
impl NativePluginRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: HashMap::new(),
        };

        // ... existing plugins ...

        // Add your plugin
        registry.register(Box::new(plugins::MyPlugin));

        registry
    }
}
```

### 3. Add Aliases (Optional)

For Python beancount compatibility, add aliases in `registry.rs`:

```rust
// Allow both "my_plugin" and "beancount.plugins.my_plugin"
registry.add_alias("beancount.plugins.my_plugin", "my_plugin");
```

### 4. Write Tests

Add tests in your plugin file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use rustledger_core::{Directive, Transaction, Posting, Amount};

    fn make_transaction(narration: &str) -> Directive {
        Directive::Transaction(Transaction {
            date: chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            flag: rustledger_core::TxnFlag::Okay,
            payee: None,
            narration: narration.to_string(),
            tags: vec![],
            links: vec![],
            meta: Default::default(),
            postings: vec![],
            source: None,
        })
    }

    #[test]
    fn test_basic_functionality() {
        let plugin = MyPlugin;
        let input = PluginInput {
            directives: vec![make_transaction("Test")],
            options: Default::default(),
            config: None,
        };

        let output = plugin.process(input);

        assert!(output.errors.is_empty());
        assert_eq!(output.directives.len(), 1);
    }

    #[test]
    fn test_with_config() {
        let plugin = MyPlugin;
        let input = PluginInput {
            directives: vec![],
            options: Default::default(),
            config: Some("threshold=100".to_string()),
        };

        let output = plugin.process(input);
        // Assert based on config
    }

    #[test]
    fn test_error_case() {
        let plugin = MyPlugin;
        // Create input that should trigger an error
        let input = PluginInput {
            directives: vec![/* problematic directive */],
            options: Default::default(),
            config: None,
        };

        let output = plugin.process(input);

        assert!(!output.errors.is_empty());
        assert_eq!(output.errors[0].severity, PluginErrorSeverity::Error);
    }
}
```

### 5. Add Integration Tests

Add integration tests in `crates/rustledger-plugin/tests/`:

```rust
// crates/rustledger-plugin/tests/my_plugin_test.rs

use rustledger_plugin::{NativePluginRegistry, run_plugin};

#[test]
fn test_my_plugin_integration() {
    let registry = NativePluginRegistry::new();
    let plugin = registry.get("my_plugin").unwrap();

    let ledger = r#"
        2024-01-01 open Assets:Bank USD

        2024-01-15 * "Test"
          Assets:Bank  100.00 USD
          Income:Test -100.00 USD
    "#;

    // Parse and run plugin
    // Assert expected behavior
}
```

## The NativePlugin Trait

```rust
pub trait NativePlugin: Send + Sync {
    /// Plugin identifier (used in `plugin "name"`)
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Process directives and return results
    fn process(&self, input: PluginInput) -> PluginOutput;
}
```

## Working with Directives

### Common Directive Types

```rust
use rustledger_core::{Directive, Transaction, Open, Close, Balance, Price};

for directive in &input.directives {
    match directive {
        Directive::Transaction(txn) => {
            // Access txn.date, txn.payee, txn.narration, txn.postings, etc.
        }
        Directive::Open(open) => {
            // Access open.account, open.currencies, open.booking
        }
        Directive::Close(close) => {
            // Access close.account
        }
        Directive::Balance(bal) => {
            // Access bal.account, bal.amount
        }
        Directive::Price(price) => {
            // Access price.currency, price.amount
        }
        // ... other types
    }
}
```

### Modifying Directives

```rust
fn process(&self, input: PluginInput) -> PluginOutput {
    let directives: Vec<_> = input.directives
        .into_iter()
        .map(|d| {
            if let Directive::Transaction(mut txn) = d {
                // Modify transaction
                txn.tags.push("processed".to_string());
                Directive::Transaction(txn)
            } else {
                d
            }
        })
        .collect();

    PluginOutput {
        directives,
        errors: vec![],
    }
}
```

### Adding New Directives

```rust
fn process(&self, input: PluginInput) -> PluginOutput {
    let mut directives = input.directives;

    // Generate new directives
    let new_price = Directive::Price(Price {
        date: chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
        currency: "AAPL".to_string(),
        amount: Amount::new(Decimal::new(15000, 2), "USD"),
        meta: Default::default(),
        source: None,
    });

    directives.push(new_price);

    PluginOutput {
        directives,
        errors: vec![],
    }
}
```

### Reporting Errors

```rust
use crate::types::{PluginError, PluginErrorSeverity};

fn process(&self, input: PluginInput) -> PluginOutput {
    let mut errors = Vec::new();

    for directive in &input.directives {
        if let Directive::Transaction(txn) = directive {
            if txn.postings.is_empty() {
                errors.push(PluginError {
                    message: "Transaction has no postings".to_string(),
                    severity: PluginErrorSeverity::Error,
                    source: txn.source.clone(),
                });
            }
        }
    }

    PluginOutput {
        directives: input.directives,
        errors,
    }
}
```

## Best Practices

### Performance

1. **Avoid cloning when possible**: Use references and iterators
2. **Early returns**: Skip directives that don't need processing
3. **Batch operations**: Collect changes before applying

```rust
// Good: Filter first, then process
let transactions: Vec<_> = input.directives
    .iter()
    .filter_map(|d| match d {
        Directive::Transaction(t) => Some(t),
        _ => None,
    })
    .collect();

// Process only relevant directives
for txn in &transactions {
    // ...
}
```

### Error Messages

Write clear, actionable error messages:

```rust
// Good
PluginError {
    message: format!(
        "Account '{}' uses currency {} but was opened with {:?}",
        posting.account, currency, allowed_currencies
    ),
    severity: PluginErrorSeverity::Error,
    source: txn.source.clone(),
}

// Bad
PluginError {
    message: "Invalid currency".to_string(),
    severity: PluginErrorSeverity::Error,
    source: None,
}
```

### Configuration Parsing

Handle configuration gracefully:

```rust
fn process(&self, input: PluginInput) -> PluginOutput {
    // Parse config with defaults
    let threshold: f64 = input.config
        .as_ref()
        .and_then(|c| c.strip_prefix("threshold="))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000.0);

    // Or for more complex config:
    let config = parse_config(&input.config);

    // ...
}

fn parse_config(config: &Option<String>) -> PluginConfig {
    let Some(config_str) = config else {
        return PluginConfig::default();
    };

    // Parse key=value pairs
    let mut result = PluginConfig::default();
    for part in config_str.split(',') {
        if let Some((key, value)) = part.split_once('=') {
            match key.trim() {
                "threshold" => result.threshold = value.parse().unwrap_or(1000.0),
                "strict" => result.strict = value == "true",
                _ => {} // Ignore unknown keys
            }
        }
    }
    result
}
```

## Documentation Requirements

1. **Module docs**: Explain what the plugin does
2. **Example usage**: Show beancount syntax
3. **Configuration**: Document all options
4. **Error codes**: List possible errors

```rust
//! Check that all accounts have a specific prefix.
//!
//! This plugin validates that all account names start with one of the
//! standard prefixes (Assets, Liabilities, Equity, Income, Expenses).
//!
//! # Usage
//!
//! ```beancount
//! plugin "beancount.plugins.check_prefix"
//! ```
//!
//! # Configuration
//!
//! Optional: specify custom allowed prefixes:
//!
//! ```beancount
//! plugin "beancount.plugins.check_prefix" "Assets,Liabilities,Equity"
//! ```
//!
//! # Errors
//!
//! - `E1001`: Account name does not start with an allowed prefix
```

## Submitting Your Plugin

1. **Fork rustledger** and create a feature branch
2. **Add your plugin** following this guide
3. **Write tests** for all functionality
4. **Update documentation** in `docs/reference/plugins.md`
5. **Run the test suite**: `cargo test -p rustledger-plugin`
6. **Submit a PR** with a clear description

## See Also

- [Custom Plugins Guide](../guides/custom-plugins.md) - WASM plugins for personal use
- [Plugins Reference](../reference/plugins.md) - All available plugins
- [Contributing Guide](./contributing.md) - General contribution guidelines
