---
title: rledger config
description: Manage rustledger configuration
---

# rledger config

Manage rustledger configuration files and settings.

## Usage

```bash
rledger config <COMMAND>
```

## Commands

| Command | Description |
|---------|-------------|
| `show` | Show the merged configuration from all sources |
| `path` | Show config file search paths |
| `edit` | Open config file in editor |
| `init` | Generate a default config file |
| `aliases` | List configured aliases |

## Options

| Option | Description |
|--------|-------------|
| `-P, --profile <PROFILE>` | Use a specific profile from config |

## Examples

### Show Current Config

```bash
rledger config show
```

Output:
```toml
[default]
file = "/home/user/finances/main.beancount"

[check]
auto = true

[query]
format = "text"
```

### Show Config Paths

```bash
rledger config path
```

Output:
```
Searching for config in:
  1. ./rledger.toml
  2. ./.rledger.toml
  3. /home/user/.config/rledger/config.toml
  4. /home/user/.rledger.toml

Found: /home/user/.config/rledger/config.toml
```

### Create Default Config

```bash
rledger config init
```

Creates `~/.config/rledger/config.toml` with default settings.

### Edit Config

```bash
rledger config edit
```

Opens config file in `$EDITOR` (or `vi` if not set).

### List Aliases

```bash
rledger config aliases
```

Output:
```
Configured aliases:
  bal     -> query "BALANCES"
  recent  -> query "SELECT date, payee, narration ORDER BY date DESC LIMIT 20"
```

## Config File Format

See [Configuration](../getting-started/configuration.md) for the full config file reference.

## See Also

- [Configuration](../getting-started/configuration.md) - Config file reference
- [Quick Start](../getting-started/quick-start.md) - Getting started guide
