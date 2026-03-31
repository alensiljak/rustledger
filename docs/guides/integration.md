---
title: Integration Guide
description: Embedding rustledger in applications and other languages
---

# Integration Guide

This guide covers all the ways to integrate rustledger into your applications, from direct Rust usage to embedding in Python, Node.js, or any other language.

## Overview

rustledger provides multiple integration paths:

| Approach | Best For | Languages |
|----------|----------|-----------|
| [CLI](#command-line-interface) | Shell scripts, CI/CD pipelines | Any (subprocess) |
| [Rust Crates](#rust-crates) | Rust applications | Rust |
| [WASM Library](#webassembly-library) | Browsers, Node.js | JavaScript, TypeScript |
| [WASI FFI](#wasi-ffi-json-rpc) | Embedding in any language | Python, Ruby, Go, etc. |
| [LSP](#language-server-protocol) | Editor integrations | Any LSP client |

## Command-Line Interface

The simplest integration is calling `rledger` as a subprocess.

### JSON Output

Most commands support `--format json` for machine-readable output:

```bash
# Get balances as JSON
rledger query ledger.beancount "BALANCES" --format json

# Check for errors
rledger check ledger.beancount --format json

# Format and compare
rledger format ledger.beancount --check
```

### Example: Python Subprocess

```python
import subprocess
import json

def get_balances(ledger_path):
    result = subprocess.run(
        ["rledger", "query", ledger_path, "BALANCES", "--format", "json"],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)

balances = get_balances("ledger.beancount")
for row in balances["rows"]:
    print(f"{row['account']}: {row['balance']}")
```

## Rust Crates

For Rust applications, use the crates directly:

```toml
[dependencies]
rustledger-core = "0.x"
rustledger-parser = "0.x"
rustledger-loader = "0.x"
rustledger-query = "0.x"
```

### Parsing

```rust
use rustledger_parser::parse;

let source = r#"
2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Expenses:Food  5 USD
  Assets:Bank   -5 USD
"#;

let result = parse(source);
for directive in result.directives {
    println!("{:?}", directive);
}
```

### Loading with Includes

```rust
use std::path::Path;
use rustledger_loader::Loader;

let mut loader = Loader::new();
let result = loader.load(Path::new("ledger.beancount"))?;

// Access loaded directives
for directive in &result.directives {
    println!("{:?}", directive);
}
```

### Running Queries

```rust
use std::path::Path;
use rustledger_loader::Loader;
use rustledger_query::{parse as parse_query, Executor};

let mut loader = Loader::new();
let result = loader.load(Path::new("ledger.beancount"))?;

let query = parse_query("SELECT account, sum(position) GROUP BY account")?;
let executor = Executor::new(&result.directives);
let query_result = executor.execute(&query)?;

for row in &query_result.rows {
    println!("{:?}", row);
}
```

## WebAssembly Library

For browser or Node.js applications, use the WASM package:

```bash
npm install @rustledger/wasm
```

### Browser Usage

```javascript
import init, { parse, validateSource, query } from '@rustledger/wasm';

await init();

const source = `
2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
`;

const result = parse(source);
if (result.errors.length === 0) {
    const validation = validateSource(source);
    console.log('Valid:', validation.valid);

    const balances = query(source, 'BALANCES');
    console.log('Balances:', balances.rows);
}
```

### Stateful API

For multiple operations on the same ledger, use `ParsedLedger`:

```javascript
import { ParsedLedger } from '@rustledger/wasm';

const ledger = new ParsedLedger(source);

if (ledger.isValid()) {
    const balances = ledger.balances();
    const formatted = ledger.format();

    // Editor features
    const completions = ledger.getCompletions(5, 10);
    const hover = ledger.getHoverInfo(3, 5);
    const definition = ledger.getDefinition(4, 3);
}

ledger.free(); // Release WASM memory
```

### Available Functions

| Function | Description |
|----------|-------------|
| `parse()` | Parse Beancount source to JSON |
| `validateSource()` | Validate ledger with error reporting |
| `query()` | Run BQL queries |
| `format()` | Format source with consistent alignment |
| `expandPads()` | Expand pad directives |
| `runPlugin()` | Run native plugins |
| `bqlCompletions()` | BQL query completions |

## WASI FFI (JSON-RPC)

The WASI FFI module exposes a JSON-RPC 2.0 API that can be embedded in any language with a WASI runtime. This is ideal for building an API server or embedding in Python, Ruby, Go, etc.

### Quick Start

Build or download the WASM module, then run with wasmtime:

```bash
# Build from source
cargo build -p rustledger-ffi-wasi --target wasm32-wasip1 --release

# Run with wasmtime
echo '{"jsonrpc":"2.0","method":"ledger.validate","params":{"source":"2024-01-01 open Assets:Bank USD"},"id":1}' | \
    wasmtime target/wasm32-wasip1/release/rustledger-ffi-wasi.wasm
```

### Available Methods

#### Ledger Operations

| Method | Description |
|--------|-------------|
| `ledger.load` | Parse beancount source and return structured data |
| `ledger.loadFile` | Load and process a beancount file (with includes, booking, plugins) |
| `ledger.validate` | Validate beancount source and return errors |
| `ledger.validateFile` | Validate a beancount file |

#### Query Operations

| Method | Description |
|--------|-------------|
| `query.execute` | Execute a BQL query on source |
| `query.executeFile` | Execute a BQL query on a file |
| `query.batch` | Execute multiple queries on source |
| `query.batchFile` | Execute multiple queries on a file |

#### Format Operations

| Method | Description |
|--------|-------------|
| `format.source` | Format beancount source code |
| `format.file` | Format a beancount file |
| `format.entry` | Format a single entry from JSON |
| `format.entries` | Format multiple entries from JSON |

#### Entry Operations

| Method | Description |
|--------|-------------|
| `entry.create` | Create an entry from JSON |
| `entry.createBatch` | Create multiple entries from JSON |
| `entry.filter` | Filter entries by date range |
| `entry.clamp` | Clamp entries to date range |

#### Utility Operations

| Method | Description |
|--------|-------------|
| `util.version` | Get API and package version |
| `util.types` | Get supported directive types, booking methods |
| `util.schema` | Get JSON Schema for API types |

### Example: Python with wasmtime-py

```python
from wasmtime import Store, Module, Instance, Linker, WasiConfig
import json

# Load the WASI module
store = Store()
wasi_config = WasiConfig()
wasi_config.inherit_stdin()
wasi_config.inherit_stdout()
store.set_wasi(wasi_config)

module = Module.from_file(store.engine, "rustledger-ffi-wasi.wasm")
linker = Linker(store.engine)
linker.define_wasi()
instance = linker.instantiate(store, module)

# Helper to call JSON-RPC methods
def call_rpc(method, params):
    request = json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    })
    # Write to stdin, read from stdout
    # ... implementation depends on your WASI binding approach
    pass

# Validate a ledger
result = call_rpc("ledger.validate", {
    "source": "2024-01-01 open Assets:Bank USD"
})
print(result)
```

### Error Codes

Standard JSON-RPC errors:

| Code | Message |
|------|---------|
| -32700 | Parse error (invalid JSON) |
| -32600 | Invalid Request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |

Beancount-specific errors:

| Code | Message |
|------|---------|
| -32000 | Beancount parse error |
| -32001 | Beancount validation error |
| -32002 | BQL query error |
| -32003 | File I/O error |

### Batch Requests

Send multiple requests in a single call:

```bash
echo '[
  {"jsonrpc":"2.0","method":"util.version","id":1},
  {"jsonrpc":"2.0","method":"ledger.validate","params":{"source":"..."},"id":2}
]' | wasmtime rustledger-ffi-wasi.wasm
```

## Language Server Protocol

For editor integrations, rustledger provides an LSP server:

```bash
rledger lsp
```

The LSP supports:
- Diagnostics (validation errors)
- Go to definition (accounts, commodities)
- Hover information (account details, balances)
- Completions (accounts, currencies, payees)
- Document formatting
- Code actions

See [Editor Integration](editor-integration.md) for setup instructions.

## Comparison

| Feature | CLI | Rust | WASM | WASI FFI | LSP |
|---------|-----|------|------|----------|-----|
| Parse ledger | Y | Y | Y | Y | - |
| Validate | Y | Y | Y | Y | Y |
| BQL queries | Y | Y | Y | Y | - |
| Format | Y | Y | Y | Y | Y |
| File access | Y | Y | - | Y | Y |
| Plugins | Y | Y | Y | Y | Y |
| Editor features | - | - | Y | - | Y |
| Streaming | - | Y | - | - | - |

## Which Should I Use?

- **Building a web app?** Use WASM
- **Building a desktop app in Rust?** Use the crates directly
- **Building a Python/Ruby/Go service?** Use WASI FFI with JSON-RPC
- **Writing shell scripts?** Use CLI with `--format json`
- **Building an editor plugin?** Use LSP
- **Need maximum performance?** Use Rust crates or WASI FFI
