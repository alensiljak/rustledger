# rustledger-plugin-types

WASM plugin interface types for [rustledger](https://github.com/rustledger/rustledger).

This crate provides the canonical type definitions that plugins must use to communicate with the rustledger host. Using this crate ensures your plugin's types are always compatible with the host.

## Installation

Add to your plugin's `Cargo.toml`:

```toml
[package]
name = "my-plugin"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
rustledger-plugin-types = "0.10"
rmp-serde = "1"
```

## Quick Start

```rust
use rustledger_plugin_types::*;

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[no_mangle]
pub extern "C" fn process(input_ptr: u32, input_len: u32) -> u64 {
    // Read input
    let input_bytes = unsafe {
        std::slice::from_raw_parts(input_ptr as *const u8, input_len as usize)
    };
    let input: PluginInput = rmp_serde::from_slice(input_bytes).unwrap();

    // Process directives (example: add a tag to all transactions)
    let mut directives = input.directives;
    for wrapper in &mut directives {
        if let DirectiveData::Transaction(ref mut txn) = wrapper.data {
            txn.tags.push("processed".to_string());
        }
    }

    // Return output
    let output = PluginOutput { directives, errors: vec![] };
    let output_bytes = rmp_serde::to_vec(&output).unwrap();

    let output_ptr = alloc(output_bytes.len() as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(
            output_bytes.as_ptr(),
            output_ptr,
            output_bytes.len(),
        );
    }

    ((output_ptr as u64) << 32) | (output_bytes.len() as u64)
}
```

## Building

```sh
# Install WASM target
rustup target add wasm32-unknown-unknown

# Build your plugin
cargo build --target wasm32-unknown-unknown --release
```

The plugin will be at `target/wasm32-unknown-unknown/release/my_plugin.wasm`.

## Using Your Plugin

In your beancount file:

```beancount
plugin "path/to/my_plugin.wasm" "optional-config-string"

2024-01-01 open Assets:Bank USD
```

## Types Overview

| Type | Description |
|------|-------------|
| `PluginInput` | Input from host: directives, options, config |
| `PluginOutput` | Output to host: processed directives, errors |
| `DirectiveWrapper` | Wrapper with date, source location, and data |
| `DirectiveData` | Enum of all directive types |
| `PluginError` | Error/warning with optional source location |

## Creating Errors

```rust
use rustledger_plugin_types::{PluginError, PluginErrorSeverity};

// Simple error
let error = PluginError::error("Something went wrong");

// Warning with source location
let warning = PluginError::warning("Duplicate entry")
    .at("ledger.beancount", 42);
```

## License

GPL-3.0-only
