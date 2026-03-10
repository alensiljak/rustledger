//! Configuration file support for rledger CLI.
//!
//! Provides layered configuration with the following precedence (highest to lowest):
//! 1. CLI arguments
//! 2. Environment variables (`RLEDGER_FILE`, `RLEDGER_FORMAT`, etc.)
//! 3. Project config (`.rledger.toml` in current directory, searching upward)
//! 4. User config (`~/.config/rledger/config.toml` or platform equivalent)
//! 5. System config (`/etc/rledger/config.toml` or platform equivalent)
//!
//! # Philosophy
//!
//! This config system only handles **CLI convenience options** that don't affect
//! accounting behavior. Options that affect accounting (like `operating_currency`,
//! `booking_method`) should be set via beancount's in-file `option` directives.
//!
//! # Example Configuration
//!
//! ```toml
//! [default]
//! file = "~/finances/main.beancount"
//!
//! [output]
//! format = "text"
//! color = true
//!
//! [profiles.business]
//! file = "~/work/accounting/main.beancount"
//!
//! [aliases]
//! bal = "report balances"
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The main configuration struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default settings.
    #[serde(default)]
    pub default: DefaultConfig,

    /// Output settings.
    #[serde(default)]
    pub output: OutputConfig,

    /// Named profiles for different ledgers.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,

    /// Command-specific settings.
    #[serde(default)]
    pub commands: CommandsConfig,

    /// Command aliases.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

/// Default configuration options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultConfig {
    /// Default beancount file path.
    pub file: Option<String>,

    /// Default editor for interactive commands.
    pub editor: Option<String>,
}

/// Output configuration options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Output format (text, csv, json).
    pub format: Option<String>,

    /// Enable colored output.
    pub color: Option<bool>,

    /// Pager command (e.g., "less -R").
    pub pager: Option<String>,

    /// Sort order for output.
    pub sort: Option<String>,
}

/// Profile configuration (inherits from default and overrides).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    /// Beancount file for this profile.
    pub file: Option<String>,

    /// Output settings for this profile.
    #[serde(default)]
    pub output: OutputConfig,
}

/// Command-specific configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandsConfig {
    /// Query command settings.
    #[serde(default)]
    pub query: CommandConfig,

    /// Check command settings.
    #[serde(default)]
    pub check: CommandConfig,

    /// Report command settings.
    #[serde(default)]
    pub report: CommandConfig,

    /// Format command settings.
    #[serde(default)]
    pub format: FormatCommandConfig,
}

/// Generic command configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandConfig {
    /// Output settings for this command.
    #[serde(default)]
    pub output: OutputConfig,

    /// Verbose output.
    pub verbose: Option<bool>,
}

/// Format command configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatCommandConfig {
    /// Create backup before formatting.
    pub backup: Option<bool>,

    /// Indentation level.
    pub indent: Option<u8>,
}

/// Information about where a config value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// Command-line argument.
    Cli,
    /// Environment variable.
    Environment,
    /// Project config file.
    Project(PathBuf),
    /// User config file.
    User(PathBuf),
    /// System config file.
    System(PathBuf),
    /// Default value.
    Default,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Environment => write!(f, "env"),
            Self::Project(p) => write!(f, "project ({})", p.display()),
            Self::User(p) => write!(f, "user ({})", p.display()),
            Self::System(p) => write!(f, "system ({})", p.display()),
            Self::Default => write!(f, "default"),
        }
    }
}

/// Loaded configuration with source tracking.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// The merged configuration.
    pub config: Config,
    /// Paths that were loaded, from lowest to highest precedence.
    pub sources: Vec<ConfigSource>,
}

impl Config {
    /// Load configuration from all sources.
    ///
    /// Loads configuration layers in the following order (lowest priority first),
    /// with later layers overriding earlier ones:
    /// 1. System config (`/etc/rledger/config.toml`)
    /// 2. User config (`~/.config/rledger/config.toml`)
    /// 3. Project config (`.rledger.toml` searching upward from cwd)
    /// 4. Environment variables (highest priority)
    pub fn load() -> Result<LoadedConfig> {
        let mut merged = Self::default();
        let mut sources = Vec::new();

        // Load system config (lowest priority)
        if let Some(path) = system_config_path()
            && path.exists()
        {
            let config = Self::load_from_file(&path)?;
            merged = merged.merge(config);
            sources.push(ConfigSource::System(path));
        }

        // Load user config
        if let Some(path) = user_config_path()
            && path.exists()
        {
            let config = Self::load_from_file(&path)?;
            merged = merged.merge(config);
            sources.push(ConfigSource::User(path));
        }

        // Load project config (highest file priority)
        if let Some(path) = find_project_config() {
            let config = Self::load_from_file(&path)?;
            merged = merged.merge(config);
            sources.push(ConfigSource::Project(path));
        }

        // Apply environment variables (higher than files)
        merged = merged.apply_env();
        if env::var("RLEDGER_FILE").is_ok()
            || env::var("RLEDGER_FORMAT").is_ok()
            || env::var("NO_COLOR").is_ok()
        {
            sources.push(ConfigSource::Environment);
        }

        Ok(LoadedConfig {
            config: merged,
            sources,
        })
    }

    /// Load configuration from a specific file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        Ok(config)
    }

    /// Merge another config into this one (other takes precedence).
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        // Merge default section
        if other.default.file.is_some() {
            self.default.file = other.default.file;
        }
        if other.default.editor.is_some() {
            self.default.editor = other.default.editor;
        }

        // Merge output section
        if other.output.format.is_some() {
            self.output.format = other.output.format;
        }
        if other.output.color.is_some() {
            self.output.color = other.output.color;
        }
        if other.output.pager.is_some() {
            self.output.pager = other.output.pager;
        }
        if other.output.sort.is_some() {
            self.output.sort = other.output.sort;
        }

        // Merge profiles (other's profiles override)
        for (name, profile) in other.profiles {
            self.profiles.insert(name, profile);
        }

        // Merge aliases (other's aliases override)
        for (name, alias) in other.aliases {
            self.aliases.insert(name, alias);
        }

        // Merge command configs
        self.commands = self.commands.merge(other.commands);

        self
    }

    /// Apply environment variables to the config.
    #[must_use]
    pub fn apply_env(mut self) -> Self {
        if let Ok(file) = env::var("RLEDGER_FILE") {
            self.default.file = Some(file);
        }
        if let Ok(format) = env::var("RLEDGER_FORMAT") {
            self.output.format = Some(format);
        }
        if env::var("NO_COLOR").is_ok() {
            self.output.color = Some(false);
        }
        self
    }

    /// Get the effective file path, optionally applying a profile.
    pub fn effective_file(&self, profile: Option<&str>) -> Option<String> {
        if let Some(profile_name) = profile
            && let Some(profile) = self.profiles.get(profile_name)
            && profile.file.is_some()
        {
            return profile.file.clone();
        }
        self.default.file.clone()
    }

    /// Expand `~` and environment variables in a path.
    pub fn expand_path(path: &str) -> PathBuf {
        let expanded = if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(rest)
            } else {
                PathBuf::from(path)
            }
        } else {
            PathBuf::from(path)
        };

        // Expand environment variables
        let path_str = expanded.to_string_lossy().into_owned();
        match shellexpand::env(&path_str) {
            Ok(expanded) => PathBuf::from(expanded.into_owned()),
            Err(_) => PathBuf::from(path_str),
        }
    }

    /// Get the effective file path as a `PathBuf`, with expansion.
    pub fn effective_file_path(&self, profile: Option<&str>) -> Option<PathBuf> {
        self.effective_file(profile).map(|p| Self::expand_path(&p))
    }

    /// Look up an alias by name.
    pub fn resolve_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(String::as_str)
    }

    /// Generate a default config file content.
    #[must_use]
    pub fn default_config_content() -> String {
        r#"# rledger configuration file
# See: https://github.com/rustledger/rustledger/issues/493

[default]
# Default beancount file (uncomment and edit)
# file = "~/finances/main.beancount"

# Editor for interactive commands (defaults to $EDITOR)
# editor = "nvim"

[output]
# Output format: text, csv, json
# format = "text"

# Enable colored output (set to false or use NO_COLOR env var to disable)
# color = true

# Pager for long output
# pager = "less -R"

# [profiles.business]
# file = "~/work/accounting/main.beancount"

# [profiles.family]
# file = "/shared/family-budget.beancount"

# [commands.query]
# output.format = "csv"

# [aliases]
# bal = "report balances"
# is = "report income-statement"
# bs = "report balance-sheet"
"#
        .to_string()
    }
}

impl OutputConfig {
    /// Merge another output config into this one.
    #[must_use]
    fn merge(mut self, other: Self) -> Self {
        if other.format.is_some() {
            self.format = other.format;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.pager.is_some() {
            self.pager = other.pager;
        }
        if other.sort.is_some() {
            self.sort = other.sort;
        }
        self
    }
}

impl CommandConfig {
    /// Merge another command config into this one.
    #[must_use]
    fn merge(mut self, other: Self) -> Self {
        self.output = self.output.merge(other.output);
        if other.verbose.is_some() {
            self.verbose = other.verbose;
        }
        self
    }
}

impl CommandsConfig {
    /// Merge another commands config into this one.
    #[must_use]
    fn merge(mut self, other: Self) -> Self {
        self.query = self.query.merge(other.query);
        self.check = self.check.merge(other.check);
        self.report = self.report.merge(other.report);

        // Merge format config
        if other.format.backup.is_some() {
            self.format.backup = other.format.backup;
        }
        if other.format.indent.is_some() {
            self.format.indent = other.format.indent;
        }

        self
    }
}

/// Get the user config directory path.
pub fn user_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("rledger"))
}

/// Get the user config file path.
pub fn user_config_path() -> Option<PathBuf> {
    user_config_dir().map(|p| p.join("config.toml"))
}

/// Get the system config file path.
pub fn system_config_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from("/etc/rledger/config.toml"))
    }
    #[cfg(windows)]
    {
        env::var("PROGRAMDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("rledger").join("config.toml"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Find project config by searching upward from current directory.
pub fn find_project_config() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    find_project_config_from(&cwd)
}

/// Find project config by searching upward from a given directory.
pub fn find_project_config_from(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();

    loop {
        let config_path = current.join(".rledger.toml");
        if config_path.exists() {
            return Some(config_path);
        }

        // Stop at home directory
        if let Some(home) = dirs::home_dir()
            && current == home
        {
            break;
        }

        // Move to parent
        if !current.pop() {
            break;
        }
    }

    None
}

/// Get all config file paths that would be searched.
pub fn config_search_paths() -> Vec<(String, PathBuf, bool)> {
    let mut paths = Vec::new();

    // Project config - show actual found path or default location
    if let Ok(cwd) = env::current_dir() {
        if let Some(found_path) = find_project_config_from(&cwd) {
            // Report the actual found project config path
            paths.push(("project".to_string(), found_path, true));
        } else {
            // No project config found; report the default path in the current directory
            let project_path = cwd.join(".rledger.toml");
            paths.push(("project".to_string(), project_path, false));
        }
    }

    // User config
    if let Some(user_path) = user_config_path() {
        let exists = user_path.exists();
        paths.push(("user".to_string(), user_path, exists));
    }

    // System config
    if let Some(system_path) = system_config_path() {
        let exists = system_path.exists();
        paths.push(("system".to_string(), system_path, exists));
    }

    paths
}

// Simple shell expansion for environment variables
mod shellexpand {
    use std::borrow::Cow;
    use std::env;

    pub fn env(input: &str) -> Result<Cow<'_, str>, env::VarError> {
        if !input.contains('$') {
            return Ok(Cow::Borrowed(input));
        }

        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' {
                // Check for ${VAR} or $VAR syntax
                let var_name = if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'
                    let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    name
                } else {
                    chars
                        .by_ref()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect()
                };

                if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else {
                    // Keep the original if var not found
                    result.push('$');
                    result.push_str(&var_name);
                }
            } else {
                result.push(c);
            }
        }

        Ok(Cow::Owned(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let content = r#"
[default]
file = "~/ledger.beancount"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.default.file, Some("~/ledger.beancount".to_string()));
    }

    #[test]
    fn test_parse_full_config() {
        let content = r#"
[default]
file = "~/finances/main.beancount"
editor = "nvim"

[output]
format = "text"
color = true

[profiles.business]
file = "~/work/ledger.beancount"

[aliases]
bal = "report balances"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(
            config.default.file,
            Some("~/finances/main.beancount".to_string())
        );
        assert_eq!(config.default.editor, Some("nvim".to_string()));
        assert_eq!(config.output.format, Some("text".to_string()));
        assert_eq!(config.output.color, Some(true));
        assert!(config.profiles.contains_key("business"));
        assert_eq!(
            config.aliases.get("bal"),
            Some(&"report balances".to_string())
        );
    }

    #[test]
    fn test_merge_configs() {
        let base = Config {
            default: DefaultConfig {
                file: Some("base.beancount".to_string()),
                editor: Some("vim".to_string()),
            },
            output: OutputConfig {
                format: Some("text".to_string()),
                color: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };

        let override_config = Config {
            default: DefaultConfig {
                file: Some("override.beancount".to_string()),
                editor: None,
            },
            output: OutputConfig {
                format: Some("csv".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = base.merge(override_config);

        // Override file should win
        assert_eq!(merged.default.file, Some("override.beancount".to_string()));
        // Base editor should remain (override was None)
        assert_eq!(merged.default.editor, Some("vim".to_string()));
        // Override format should win
        assert_eq!(merged.output.format, Some("csv".to_string()));
        // Base color should remain
        assert_eq!(merged.output.color, Some(true));
    }

    #[test]
    fn test_effective_file_with_profile() {
        let config = Config {
            default: DefaultConfig {
                file: Some("default.beancount".to_string()),
                ..Default::default()
            },
            profiles: {
                let mut profiles = HashMap::new();
                profiles.insert(
                    "business".to_string(),
                    ProfileConfig {
                        file: Some("business.beancount".to_string()),
                        ..Default::default()
                    },
                );
                profiles
            },
            ..Default::default()
        };

        assert_eq!(
            config.effective_file(None),
            Some("default.beancount".to_string())
        );
        assert_eq!(
            config.effective_file(Some("business")),
            Some("business.beancount".to_string())
        );
        // Unknown profile falls back to default
        assert_eq!(
            config.effective_file(Some("unknown")),
            Some("default.beancount".to_string())
        );
    }

    #[test]
    fn test_expand_path_tilde() {
        let path = "~/test/file.beancount";
        let expanded = Config::expand_path(path);

        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("test/file.beancount"));
        }
    }

    #[test]
    fn test_alias_resolution() {
        let config = Config {
            aliases: {
                let mut aliases = HashMap::new();
                aliases.insert("bal".to_string(), "report balances".to_string());
                aliases
            },
            ..Default::default()
        };

        assert_eq!(config.resolve_alias("bal"), Some("report balances"));
        assert_eq!(config.resolve_alias("unknown"), None);
    }

    #[test]
    fn test_command_specific_config() {
        let content = r#"
[commands.query]
output.format = "csv"
verbose = true

[commands.format]
indent = 4
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.commands.query.output.format, Some("csv".to_string()));
        assert_eq!(config.commands.query.verbose, Some(true));
        assert_eq!(config.commands.format.indent, Some(4));
    }

    #[test]
    fn test_merge_command_configs() {
        let base = Config {
            commands: CommandsConfig {
                query: CommandConfig {
                    output: OutputConfig {
                        format: Some("text".to_string()),
                        ..Default::default()
                    },
                    verbose: Some(false),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let override_config = Config {
            commands: CommandsConfig {
                query: CommandConfig {
                    output: OutputConfig {
                        format: Some("csv".to_string()),
                        ..Default::default()
                    },
                    verbose: None,
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = base.merge(override_config);

        // Override format should win
        assert_eq!(merged.commands.query.output.format, Some("csv".to_string()));
        // Base verbose should remain (override was None)
        assert_eq!(merged.commands.query.verbose, Some(false));
    }
}
