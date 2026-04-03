# rustledger for VS Code

Beancount language support powered by [rustledger](https://github.com/rustledger/rustledger) — a fast, Rust-based Beancount implementation.

## Features

- **Diagnostics** — real-time syntax and validation errors
- **Completion** — accounts, currencies, payees, tags, links
- **Hover** — account balances, metadata, directive info
- **Go to Definition** — jump to account and commodity declarations
- **Find References** — find all uses of an account or commodity
- **Rename** — rename accounts across files
- **Formatting** — format documents and selections
- **Document Symbols** — outline view
- **Code Actions** — quick fixes for common issues
- **Inlay Hints** — inline balance and cost annotations
- **Semantic Highlighting** — rich syntax coloring

## Requirements

Install `rledger-lsp` (included with rustledger):

```bash
# macOS
brew install rustledger

# Arch Linux
yay -S rustledger-bin

# Cargo
cargo install rustledger-lsp
```

## Extension Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `rustledger.server.path` | `rledger-lsp` | Path to the rledger-lsp binary |
| `rustledger.server.extraArgs` | `[]` | Extra arguments passed to rledger-lsp |
| `rustledger.journalFile` | `""` | Root journal file (auto-discovered if empty) |

## Commands

| Command | Description |
|---------|-------------|
| `rustledger: Restart Language Server` | Restart the LSP server (useful after updating `rledger-lsp`) |

## Troubleshooting

Check the Output panel (`View > Output`, select "rustledger" from the dropdown) for LSP logs and error messages.

## Multi-File Support

The LSP automatically discovers your root journal file (`main.beancount`, `ledger.beancount`, etc.) and follows `include` directives for cross-file completions and diagnostics. Set `rustledger.journalFile` if auto-discovery doesn't find yours.
