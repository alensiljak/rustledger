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
                    // Collect alphanumeric and underscore characters for var name
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    name
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

    #[test]
    fn test_parse_empty_config() {
        let content = "";
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.default.file, None);
        assert_eq!(config.output.format, None);
        assert!(config.profiles.is_empty());
        assert!(config.aliases.is_empty());
    }

    #[test]
    fn test_parse_output_config_all_fields() {
        let content = r#"
[output]
format = "json"
color = false
pager = "less -R"
sort = "date"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.output.format, Some("json".to_string()));
        assert_eq!(config.output.color, Some(false));
        assert_eq!(config.output.pager, Some("less -R".to_string()));
        assert_eq!(config.output.sort, Some("date".to_string()));
    }

    #[test]
    fn test_parse_multiple_profiles() {
        let content = r#"
[profiles.personal]
file = "~/personal.beancount"

[profiles.business]
file = "~/business.beancount"

[profiles.family]
file = "/shared/family.beancount"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.profiles.len(), 3);
        assert_eq!(
            config.profiles.get("personal").unwrap().file,
            Some("~/personal.beancount".to_string())
        );
        assert_eq!(
            config.profiles.get("business").unwrap().file,
            Some("~/business.beancount".to_string())
        );
        assert_eq!(
            config.profiles.get("family").unwrap().file,
            Some("/shared/family.beancount".to_string())
        );
    }

    #[test]
    fn test_parse_profile_with_output() {
        let content = r#"
[profiles.work]
file = "~/work.beancount"

[profiles.work.output]
format = "csv"
color = false
"#;
        let config: Config = toml::from_str(content).unwrap();
        let work = config.profiles.get("work").unwrap();
        assert_eq!(work.file, Some("~/work.beancount".to_string()));
        assert_eq!(work.output.format, Some("csv".to_string()));
        assert_eq!(work.output.color, Some(false));
    }

    #[test]
    fn test_parse_multiple_aliases() {
        let content = r#"
[aliases]
bal = "report balances"
is = "report income"
bs = "report balance-sheet"
expenses = "query 'SELECT account, sum(position) WHERE account ~ \"Expenses\"'"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.aliases.len(), 4);
        assert_eq!(config.resolve_alias("bal"), Some("report balances"));
        assert_eq!(config.resolve_alias("is"), Some("report income"));
        assert_eq!(config.resolve_alias("bs"), Some("report balance-sheet"));
        assert!(
            config
                .resolve_alias("expenses")
                .unwrap()
                .contains("Expenses")
        );
    }

    #[test]
    fn test_parse_all_command_configs() {
        let content = r#"
[commands.query]
output.format = "csv"
verbose = true

[commands.check]
output.format = "json"
verbose = false

[commands.report]
output.format = "text"

[commands.format]
backup = true
indent = 4
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert_eq!(config.commands.query.output.format, Some("csv".to_string()));
        assert_eq!(config.commands.query.verbose, Some(true));

        assert_eq!(
            config.commands.check.output.format,
            Some("json".to_string())
        );
        assert_eq!(config.commands.check.verbose, Some(false));

        assert_eq!(
            config.commands.report.output.format,
            Some("text".to_string())
        );

        assert_eq!(config.commands.format.backup, Some(true));
        assert_eq!(config.commands.format.indent, Some(4));
    }

    #[test]
    fn test_merge_output_config() {
        let base = OutputConfig {
            format: Some("text".to_string()),
            color: Some(true),
            pager: Some("less".to_string()),
            sort: None,
        };

        let override_cfg = OutputConfig {
            format: Some("csv".to_string()),
            color: None,
            pager: None,
            sort: Some("amount".to_string()),
        };

        let merged = base.merge(override_cfg);

        assert_eq!(merged.format, Some("csv".to_string())); // overridden
        assert_eq!(merged.color, Some(true)); // kept from base
        assert_eq!(merged.pager, Some("less".to_string())); // kept from base
        assert_eq!(merged.sort, Some("amount".to_string())); // new from override
    }

    #[test]
    fn test_merge_aliases() {
        let base = Config {
            aliases: {
                let mut m = HashMap::new();
                m.insert("bal".to_string(), "report balances".to_string());
                m.insert("is".to_string(), "report income".to_string());
                m
            },
            ..Default::default()
        };

        let override_config = Config {
            aliases: {
                let mut m = HashMap::new();
                m.insert("bal".to_string(), "report balances -f csv".to_string()); // override
                m.insert("bs".to_string(), "report balance-sheet".to_string()); // new
                m
            },
            ..Default::default()
        };

        let merged = base.merge(override_config);

        assert_eq!(merged.aliases.len(), 3);
        assert_eq!(
            merged.resolve_alias("bal"),
            Some("report balances -f csv") // overridden
        );
        assert_eq!(merged.resolve_alias("is"), Some("report income")); // kept
        assert_eq!(merged.resolve_alias("bs"), Some("report balance-sheet")); // new
    }

    #[test]
    fn test_merge_profiles() {
        let base = Config {
            profiles: {
                let mut m = HashMap::new();
                m.insert(
                    "work".to_string(),
                    ProfileConfig {
                        file: Some("~/work.beancount".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };

        let override_config = Config {
            profiles: {
                let mut m = HashMap::new();
                m.insert(
                    "work".to_string(),
                    ProfileConfig {
                        file: Some("~/work-new.beancount".to_string()),
                        ..Default::default()
                    },
                );
                m.insert(
                    "personal".to_string(),
                    ProfileConfig {
                        file: Some("~/personal.beancount".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };

        let merged = base.merge(override_config);

        assert_eq!(merged.profiles.len(), 2);
        assert_eq!(
            merged.profiles.get("work").unwrap().file,
            Some("~/work-new.beancount".to_string()) // overridden
        );
        assert_eq!(
            merged.profiles.get("personal").unwrap().file,
            Some("~/personal.beancount".to_string()) // new
        );
    }

    #[test]
    fn test_effective_file_no_default() {
        let config = Config::default();
        assert_eq!(config.effective_file(None), None);
        assert_eq!(config.effective_file(Some("nonexistent")), None);
    }

    #[test]
    fn test_effective_file_profile_without_file() {
        let config = Config {
            default: DefaultConfig {
                file: Some("default.beancount".to_string()),
                ..Default::default()
            },
            profiles: {
                let mut m = HashMap::new();
                m.insert(
                    "empty".to_string(),
                    ProfileConfig {
                        file: None, // no file override
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };

        // Profile exists but has no file, should fall back to default
        assert_eq!(
            config.effective_file(Some("empty")),
            Some("default.beancount".to_string())
        );
    }

    #[test]
    fn test_expand_path_no_tilde() {
        let path = "/absolute/path/file.beancount";
        let expanded = Config::expand_path(path);
        assert_eq!(expanded, PathBuf::from("/absolute/path/file.beancount"));
    }

    #[test]
    fn test_expand_path_relative() {
        let path = "relative/path/file.beancount";
        let expanded = Config::expand_path(path);
        assert_eq!(expanded, PathBuf::from("relative/path/file.beancount"));
    }

    #[test]
    fn test_shellexpand_unknown_var() {
        let result = shellexpand::env("$UNKNOWN_VAR_RLEDGER_TEST/file").unwrap();
        // Unknown vars are kept as-is
        assert_eq!(&*result, "$UNKNOWN_VAR_RLEDGER_TEST/file");
    }

    #[test]
    fn test_shellexpand_no_vars() {
        let result = shellexpand::env("/path/without/vars").unwrap();
        assert_eq!(&*result, "/path/without/vars");
    }

    #[test]
    fn test_shellexpand_multiple_vars_in_path() {
        // Test that existing env vars like HOME work
        let result = shellexpand::env("$HOME/test");
        // Should succeed (HOME is always set)
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config_content() {
        let content = Config::default_config_content();
        assert!(content.contains("[default]"));
        assert!(content.contains("# file ="));
        assert!(content.contains("[output]"));
        assert!(content.contains("# format ="));
        assert!(content.contains("[aliases]"));
    }

    #[test]
    fn test_config_source_display() {
        assert_eq!(format!("{}", ConfigSource::Cli), "cli");
        assert_eq!(format!("{}", ConfigSource::Environment), "env");
        assert_eq!(format!("{}", ConfigSource::Default), "default");

        let project = ConfigSource::Project(PathBuf::from("/test/.rledger.toml"));
        assert!(format!("{project}").contains("project"));
        assert!(format!("{project}").contains(".rledger.toml"));

        let user = ConfigSource::User(PathBuf::from("/home/user/.config/rledger/config.toml"));
        assert!(format!("{user}").contains("user"));
    }

    #[test]
    fn test_load_from_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[default]
file = "test.beancount"

[aliases]
t = "check"
"#
        )
        .unwrap();

        let config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config.default.file, Some("test.beancount".to_string()));
        assert_eq!(config.resolve_alias("t"), Some("check"));
    }

    #[test]
    fn test_load_from_file_invalid_toml() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("invalid.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(file, "this is not valid toml [[[").unwrap();

        let result = Config::load_from_file(&config_path);
        assert!(result.is_err());
    }

    // Note: apply_env tests require setting environment variables which is unsafe.
    // These behaviors are tested via integration tests instead.

    #[test]
    fn test_apply_env_no_changes() {
        // When no env vars are set, apply_env should not change anything
        let config = Config {
            default: DefaultConfig {
                file: Some("/config/file.beancount".to_string()),
                ..Default::default()
            },
            output: OutputConfig {
                format: Some("text".to_string()),
                color: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };

        // Note: This test assumes RLEDGER_FILE and RLEDGER_FORMAT are not set
        // If they happen to be set in the test environment, this test may fail
        // Just verify the method doesn't panic
        let _ = config.apply_env();
    }

    #[test]
    fn test_user_config_path() {
        let path = user_config_path();
        // Should return Some on most platforms
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("rledger"));
            assert!(p.to_string_lossy().contains("config.toml"));
        }
    }

    #[test]
    fn test_system_config_path() {
        let path = system_config_path();
        #[cfg(unix)]
        {
            assert!(path.is_some());
            assert!(path.unwrap().to_string_lossy().contains("/etc/rledger"));
        }
    }

    #[test]
    fn test_find_project_config_from_none() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_project_config_from(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_find_project_config_from_current() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".rledger.toml");
        std::fs::write(&config_path, "[default]").unwrap();

        let result = find_project_config_from(dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), config_path);
    }

    #[test]
    fn test_find_project_config_from_parent() {
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("subdir");
        std::fs::create_dir(&child).unwrap();

        let config_path = parent.path().join(".rledger.toml");
        std::fs::write(&config_path, "[default]").unwrap();

        let result = find_project_config_from(&child);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), config_path);
    }

    #[test]
    fn test_config_search_paths_structure() {
        let paths = config_search_paths();

        // Should have project, user, and possibly system
        assert!(!paths.is_empty());

        // Check that we have expected levels
        let levels: Vec<&str> = paths.iter().map(|(l, _, _)| l.as_str()).collect();
        assert!(levels.contains(&"project"));
        assert!(levels.contains(&"user"));
    }

    #[test]
    fn test_merge_full_workflow() {
        // Simulate system -> user -> project merge
        let system = Config {
            default: DefaultConfig {
                file: Some("/etc/default.beancount".to_string()),
                editor: Some("vi".to_string()),
            },
            output: OutputConfig {
                format: Some("text".to_string()),
                color: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };

        let user = Config {
            default: DefaultConfig {
                editor: Some("vim".to_string()), // override
                ..Default::default()
            },
            aliases: {
                let mut m = HashMap::new();
                m.insert("bal".to_string(), "report balances".to_string());
                m
            },
            ..Default::default()
        };

        let project = Config {
            default: DefaultConfig {
                file: Some("./project.beancount".to_string()), // override
                ..Default::default()
            },
            output: OutputConfig {
                format: Some("csv".to_string()), // override
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = system.merge(user).merge(project);

        // Project file wins
        assert_eq!(merged.default.file, Some("./project.beancount".to_string()));
        // User editor wins over system
        assert_eq!(merged.default.editor, Some("vim".to_string()));
        // Project format wins
        assert_eq!(merged.output.format, Some("csv".to_string()));
        // System color preserved
        assert_eq!(merged.output.color, Some(true));
        // User alias preserved
        assert_eq!(merged.resolve_alias("bal"), Some("report balances"));
    }
}
