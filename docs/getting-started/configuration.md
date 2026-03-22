---
title: Configuration
description: Configure rustledger with profiles and options
---

# Configuration

rustledger can be configured via environment variables, config files, and command-line options.

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `LEDGER_FILE` | Default beancount file | `~/ledger.beancount` |
| `RLEDGER_CONFIG` | Config file location | `~/.config/rledger/config.toml` |

Set in your shell profile (`~/.bashrc`, `~/.zshrc`):

```bash
export LEDGER_FILE="$HOME/finances/main.beancount"
```

## Config File

rustledger looks for configuration in:

1. `$RLEDGER_CONFIG` (if set)
2. `~/.config/rledger/config.toml`
3. `.rledger.toml` in the current directory

### Example Config

```toml
# ~/.config/rledger/config.toml

# Default ledger file
ledger_file = "~/finances/main.beancount"

# Default output format
format = "text"

# Enable plugins by default
plugins = ["auto_accounts", "implicit_prices"]

# Profiles for different ledgers
[profiles.personal]
ledger_file = "~/finances/personal.beancount"
plugins = ["auto_accounts"]

[profiles.business]
ledger_file = "~/finances/business.beancount"
plugins = ["auto_accounts", "check_commodity"]
```

### Using Profiles

```bash
# Use a profile
rledger check -P personal
rledger report balances -P business

# Override ledger file
rledger check ~/other/ledger.beancount
```

## Beancount Options

Set options in your beancount file:

```beancount
option "title" "My Personal Finances"
option "operating_currency" "USD"
option "booking_method" "FIFO"
```

### Common Options

| Option | Description | Default |
|--------|-------------|---------|
| `title` | Ledger title | (none) |
| `operating_currency` | Main currency | (none) |
| `booking_method` | FIFO, LIFO, AVERAGE, etc. | STRICT |
| `account_previous_balances` | Retained earnings account | `Equity:Opening-Balances` |
| `account_current_earnings` | Current earnings account | `Equity:Earnings:Current` |
| `inferred_tolerance_default` | Balance tolerance | `0.005` |

### Booking Methods

```beancount
; First-in, first-out
option "booking_method" "FIFO"

; Last-in, first-out
option "booking_method" "LIFO"

; Average cost
option "booking_method" "AVERAGE"

; Strict matching (default)
option "booking_method" "STRICT"
```

## Shell Aliases

For quick commands, add aliases to your shell:

```bash
# ~/.bashrc or ~/.zshrc

export LEDGER_FILE="$HOME/finances/main.beancount"

# Quick commands
alias rc='rledger check'
alias rq='rledger query'
alias rb='rledger report balances -a'
alias rr='rledger report journal -a'
alias rbal='rledger report balsheet'
alias ris='rledger report income'

# Usage:
# rc                    - check ledger
# rb Expenses:Food      - balance for food expenses
# rr Assets:Bank        - register for bank accounts
```

## Editor Integration

### VS Code

Install the [Beancount extension](https://marketplace.visualstudio.com/items?itemName=Lencerf.beancount) and configure it to use rustledger:

```json
{
  "beancount.lspPath": "rledger-lsp"
}
```

### Neovim

Using nvim-lspconfig:

```lua
require('lspconfig').rledger_lsp.setup{}
```

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "beancount"
language-server = { command = "rledger-lsp" }
```

See [Editor Integration](../guides/editor-integration.md) for detailed setup instructions.

## Next Steps

- [Commands Reference](../commands/index.md) - All CLI commands
- [Shell Aliases](../guides/shell-aliases.md) - More alias examples
- [Editor Integration](../guides/editor-integration.md) - Full editor setup
