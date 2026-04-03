# rustledger for VS Code

Thin VS Code client for `rledger-lsp` — all language features are provided by the LSP server.

## Requirements

**`rledger-lsp` is required.** This extension is a thin wrapper that connects VS Code to the LSP server. Without it, no features will work.

```bash
# macOS
brew install rustledger

# Arch Linux
yay -S rustledger-bin

# Cargo
cargo install rustledger-lsp
```

## Installation

Download `rustledger-vscode.vsix` from the [latest release](https://github.com/rustledger/rustledger/releases/latest), then install:

```bash
code --install-extension rustledger-vscode.vsix
```

Or in VS Code: `Ctrl+Shift+P` → "Extensions: Install from VSIX..." → select the downloaded file.

## Features

All features are provided by `rledger-lsp`:

- **Semantic Highlighting** — rich syntax coloring
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

## Extension Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `rustledger.server.path` | `rledger-lsp` | Path to the rledger-lsp binary |
| `rustledger.server.extraArgs` | `[]` | Extra arguments passed to rledger-lsp |
| `rustledger.journalFile` | `""` | Root journal file (auto-discovered if empty) |
| `rustledger.checkForUpdates` | `true` | Check for extension updates on startup |

## Commands

| Command | Description |
|---------|-------------|
| `rustledger: Restart Language Server` | Restart the LSP server (useful after updating `rledger-lsp`) |
| `rustledger: Check for Updates` | Check for a newer version of the extension |

## Auto-Update

The extension automatically checks for updates on startup. When a new version is available, you'll see a notification with an "Update" button that downloads and installs the latest version directly from GitHub Releases.

## Troubleshooting

Check the Output panel (`View > Output`, select "rustledger" from the dropdown) for LSP logs and error messages.

## Multi-File Support

The LSP automatically discovers your root journal file (`main.beancount`, `ledger.beancount`, etc.) and follows `include` directives for cross-file completions and diagnostics. Set `rustledger.journalFile` if auto-discovery doesn't find yours.
