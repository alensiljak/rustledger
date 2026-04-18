# rustledger-ffi-wasi

A WASI module providing JSON-RPC 2.0 API for embedding Rustledger in any language.

## Overview

This crate compiles to a WebAssembly module that can be run via [wasmtime](https://wasmtime.dev/) or any WASI-compatible runtime. It provides a fast, portable way to use Rustledger's Beancount parsing, validation, and querying capabilities from any language.

## Usage

Send JSON-RPC 2.0 requests via stdin:

```bash
# Validate beancount source
echo '{"jsonrpc":"2.0","method":"ledger.validate","params":{"source":"2024-01-01 open Assets:Bank USD"},"id":1}' | \
    wasmtime rustledger-ffi-wasi.wasm

# Execute a BQL query
echo '{"jsonrpc":"2.0","method":"query.execute","params":{"source":"...","query":"SELECT account, sum(position) GROUP BY 1"},"id":1}' | \
    wasmtime rustledger-ffi-wasi.wasm

# Batch requests
echo '[
  {"jsonrpc":"2.0","method":"util.version","id":1},
  {"jsonrpc":"2.0","method":"util.types","id":2}
]' | wasmtime rustledger-ffi-wasi.wasm
```

## Available Methods

### Ledger Operations

| Method | Description |
|--------|-------------|
| `ledger.load` | Parse beancount source and return structured data |
| `ledger.loadFile` | Load and process a beancount file (with includes, booking, plugins) |
| `ledger.validate` | Validate beancount source and return errors |
| `ledger.validateFile` | Validate a beancount file |

### Query Operations

| Method | Description |
|--------|-------------|
| `query.execute` | Execute a BQL query on source |
| `query.executeFile` | Execute a BQL query on a file |
| `query.batch` | Execute multiple queries on source |
| `query.batchFile` | Execute multiple queries on a file |

### Format Operations

| Method | Description |
|--------|-------------|
| `format.source` | Format beancount source code |
| `format.file` | Format a beancount file |
| `format.entry` | Format a single entry from JSON |
| `format.entries` | Format multiple entries from JSON |

### Entry Operations

| Method | Description |
|--------|-------------|
| `entry.create` | Create an entry from JSON |
| `entry.createBatch` | Create multiple entries from JSON |
| `entry.filter` | Filter entries by date range |
| `entry.clamp` | Clamp entries to date range |

### Utility Operations

| Method | Description |
|--------|-------------|
| `util.version` | Get API and package version |
| `util.types` | Get supported directive types, booking methods |
| `util.isEncrypted` | Check if a file path is encrypted |
| `util.getAccountType` | Get account type from account name |

## Error Codes

### Standard JSON-RPC Errors

| Code | Message |
|------|---------|
| -32700 | Parse error (invalid JSON) |
| -32600 | Invalid Request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |

### Beancount-specific Errors

| Code | Message |
|------|---------|
| -32000 | Beancount parse error |
| -32001 | Beancount validation error |
| -32002 | BQL query error |
| -32003 | File I/O error |

## Examples

### Validate a Ledger

```bash
echo '{"jsonrpc":"2.0","method":"ledger.validate","params":{"source":"2024-01-01 open Assets:Bank USD\n2024-01-02 * \"Coffee\"\n  Expenses:Food 5 USD\n  Assets:Bank"},"id":1}' | wasmtime rustledger-ffi-wasi.wasm
```

Response:

```json
{"jsonrpc":"2.0","result":{"api_version":"1.0","valid":true,"errors":[]},"id":1}
```

### Execute a Query

```bash
echo '{"jsonrpc":"2.0","method":"query.execute","params":{"source":"2024-01-01 open Assets:Bank USD\n2024-01-01 open Expenses:Food\n2024-01-02 * \"Coffee\"\n  Expenses:Food 5 USD\n  Assets:Bank -5 USD","query":"SELECT account, sum(position) GROUP BY 1"},"id":1}' | wasmtime rustledger-ffi-wasi.wasm
```

### Create an Entry

```bash
echo '{"jsonrpc":"2.0","method":"entry.create","params":{"entry":{"type":"transaction","date":"2024-01-15","payee":"Grocery Store","narration":"Weekly groceries","postings":[{"account":"Expenses:Food","units":{"number":"50","currency":"USD"}},{"account":"Assets:Bank"}]}},"id":1}' | wasmtime rustledger-ffi-wasi.wasm
```

## OpenRPC Specification

An OpenRPC specification is available at `openrpc.json` for automatic client generation.

## Building

```bash
# Build for WASI target
cargo build -p rustledger-ffi-wasi --target wasm32-wasip1 --release

# The output will be at:
# target/wasm32-wasip1/release/rustledger-ffi-wasi.wasm
```

## License

GPL-3.0
