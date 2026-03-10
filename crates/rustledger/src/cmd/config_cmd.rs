//! rledger config - Configuration management commands.
//!
//! Provides subcommands for viewing and managing rledger configuration:
//!
//! - `rledger config show` - Show merged configuration
//! - `rledger config path` - Show config file search paths
//! - `rledger config edit` - Open config file in editor
//! - `rledger config init` - Generate a default config file

use crate::config::{self, Config, LoadedConfig};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::process::Command;

/// Configuration management commands.
#[derive(Parser, Debug)]
#[command(name = "config")]
pub struct Args {
    /// Config subcommand to run.
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Show the merged configuration from all sources.
    Show {
        /// Show raw configs without merging (one per source).
        #[arg(long)]
        raw: bool,

        /// Output format (toml, json).
        #[arg(long, short, default_value = "toml")]
        format: String,
    },

    /// Show config file search paths.
    Path,

    /// Open config file in editor.
    Edit {
        /// Edit project config instead of user config.
        #[arg(long)]
        project: bool,

        /// Edit system config instead of user config.
        #[arg(long)]
        system: bool,
    },

    /// Generate a default config file.
    Init {
        /// Create project config (.rledger.toml) instead of user config.
        #[arg(long)]
        project: bool,

        /// Overwrite existing config file.
        #[arg(long, short)]
        force: bool,
    },

    /// List configured aliases.
    Aliases,
}

/// Run the config command.
pub fn run(args: &Args) -> Result<()> {
    match &args.command {
        ConfigCommand::Show { raw, format } => run_show(*raw, format),
        ConfigCommand::Path => run_path(),
        ConfigCommand::Edit { project, system } => run_edit(*project, *system),
        ConfigCommand::Init { project, force } => run_init(*project, *force),
        ConfigCommand::Aliases => run_aliases(),
    }
}

/// Show merged configuration.
fn run_show(raw: bool, format: &str) -> Result<()> {
    let loaded = Config::load()?;

    if raw {
        // Show each config source separately
        println!("# Configuration sources (in order of precedence)\n");

        for source in &loaded.sources {
            match source {
                config::ConfigSource::Project(path)
                | config::ConfigSource::User(path)
                | config::ConfigSource::System(path) => {
                    println!("# === {source} ===");
                    if let Ok(content) = fs::read_to_string(path) {
                        println!("{content}");
                    }
                    println!();
                }
                config::ConfigSource::Environment => {
                    println!("# === Environment Variables ===");
                    if let Ok(file) = std::env::var("RLEDGER_FILE") {
                        println!("RLEDGER_FILE={file}");
                    }
                    if let Ok(format) = std::env::var("RLEDGER_FORMAT") {
                        println!("RLEDGER_FORMAT={format}");
                    }
                    if std::env::var("NO_COLOR").is_ok() {
                        println!("NO_COLOR=1");
                    }
                    if let Ok(profile) = std::env::var("RLEDGER_PROFILE") {
                        println!("RLEDGER_PROFILE={profile}");
                    }
                    println!();
                }
                _ => {}
            }
        }
    } else {
        // Show merged config
        print_config(&loaded, format)?;
    }

    Ok(())
}

/// Print configuration in the specified format.
fn print_config(loaded: &LoadedConfig, format: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();

    match format {
        "toml" => {
            writeln!(stdout, "# Merged configuration (highest priority wins)")?;
            writeln!(stdout, "# Sources: {}", format_sources(&loaded.sources))?;
            writeln!(stdout)?;

            let toml_str = toml::to_string_pretty(&loaded.config)
                .context("Failed to serialize config to TOML")?;
            writeln!(stdout, "{toml_str}")?;
        }
        "json" => {
            let json_str = serde_json::to_string_pretty(&loaded.config)
                .context("Failed to serialize config to JSON")?;
            writeln!(stdout, "{json_str}")?;
        }
        _ => {
            bail!("Unknown format: {format}. Supported: toml, json");
        }
    }

    Ok(())
}

/// Format source list for display.
fn format_sources(sources: &[config::ConfigSource]) -> String {
    if sources.is_empty() {
        "default".to_string()
    } else {
        sources
            .iter()
            .map(|s| match s {
                config::ConfigSource::Cli => "cli".to_string(),
                config::ConfigSource::Environment => "env".to_string(),
                config::ConfigSource::Project(_) => "project".to_string(),
                config::ConfigSource::User(_) => "user".to_string(),
                config::ConfigSource::System(_) => "system".to_string(),
                config::ConfigSource::Default => "default".to_string(),
            })
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// Show config file search paths.
fn run_path() -> Result<()> {
    let paths = config::config_search_paths();

    println!("Configuration file search paths:\n");

    for (level, path, exists) in paths {
        let status = if exists { "(found)" } else { "(not found)" };
        println!("  {level:8} {status:12} {}", path.display());
    }

    println!();
    println!("Environment variables:");
    println!("  RLEDGER_FILE     Default beancount file");
    println!("  RLEDGER_FORMAT   Output format (text, csv, json)");
    println!("  RLEDGER_PROFILE  Active profile name");
    println!("  NO_COLOR         Disable colored output");

    Ok(())
}

/// Open config file in editor.
fn run_edit(project: bool, system: bool) -> Result<()> {
    let path = if system {
        config::system_config_path().context("System config path not available on this platform")?
    } else if project {
        std::env::current_dir()?.join(".rledger.toml")
    } else {
        config::user_config_path().context("User config path not available")?
    };

    // Ensure parent directory exists for user config
    if !project
        && !system
        && let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Create file with default content if it doesn't exist
    if !path.exists() {
        fs::write(&path, Config::default_config_content())
            .with_context(|| format!("Failed to create config file: {}", path.display()))?;
        println!("Created new config file: {}", path.display());
    }

    // Get editor from config, then $EDITOR, then fall back to common editors
    let loaded = Config::load().ok();
    let editor = loaded
        .as_ref()
        .and_then(|l| l.config.default.editor.clone())
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .unwrap_or_else(|| {
            // Try common editors
            for editor in &["nano", "vim", "vi", "notepad"] {
                if which_exists(editor) {
                    return (*editor).to_string();
                }
            }
            "nano".to_string()
        });

    println!("Opening {} with {editor}...", path.display());

    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("Failed to run editor: {editor}"))?;

    if !status.success() {
        bail!("Editor exited with error");
    }

    Ok(())
}

/// Check if a command exists in PATH.
fn which_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(cmd).exists()))
}

/// Generate a default config file.
fn run_init(project: bool, force: bool) -> Result<()> {
    let path = if project {
        std::env::current_dir()?.join(".rledger.toml")
    } else {
        config::user_config_path().context("User config path not available")?
    };

    // Check if file exists
    if path.exists() && !force {
        bail!(
            "Config file already exists: {}\nUse --force to overwrite",
            path.display()
        );
    }

    // Create parent directory if needed
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Write default config
    fs::write(&path, Config::default_config_content())
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    println!("Created config file: {}", path.display());
    println!();
    println!("Edit this file to set your default beancount file:");
    println!(
        "  rledger config edit{}",
        if project { " --project" } else { "" }
    );

    Ok(())
}

/// List configured aliases.
fn run_aliases() -> Result<()> {
    let loaded = Config::load()?;

    if loaded.config.aliases.is_empty() {
        println!("No aliases configured.");
        println!();
        println!("Add aliases to your config file:");
        println!("  [aliases]");
        println!("  bal = \"report balances\"");
        println!("  inc = \"report income\"");
        return Ok(());
    }

    println!("Configured aliases:\n");

    // Sort aliases by name for consistent output
    let mut aliases: Vec<_> = loaded.config.aliases.iter().collect();
    aliases.sort_by_key(|(name, _)| *name);

    for (name, expansion) in aliases {
        println!("  {name} = \"{expansion}\"");
    }

    println!();
    println!("Usage: rledger <alias> [additional args]");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_format_sources() {
        let sources = vec![
            config::ConfigSource::Project("/test/.rledger.toml".into()),
            config::ConfigSource::User("/home/user/.config/rledger/config.toml".into()),
        ];

        let formatted = format_sources(&sources);
        assert_eq!(formatted, "project > user");
    }

    #[test]
    fn test_format_sources_empty() {
        let sources = vec![];
        let formatted = format_sources(&sources);
        assert_eq!(formatted, "default");
    }

    #[test]
    fn test_init_creates_config() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        // Manually create config since run_init uses fixed paths
        fs::write(&config_path, Config::default_config_content()).unwrap();

        assert!(config_path.exists());
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[default]"));
        assert!(content.contains("# file ="));
    }
}
