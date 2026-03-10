//! rledger - Unified CLI for rustledger.
//!
//! A pure Rust implementation of Beancount with 10-30x faster performance.
//!
//! # Usage
//!
//! ```bash
//! rledger check ledger.beancount
//! rledger query ledger.beancount "SELECT account, sum(position)"
//! rledger format ledger.beancount
//! rledger report ledger.beancount balances
//! rledger doctor lex ledger.beancount
//! rledger config show  # Show configuration
//! rledger completions bash  # Generate shell completions
//! ```
//!
//! # Configuration
//!
//! If no file is specified, rledger will use the default file from config:
//! ```bash
//! rledger check  # Uses file from ~/.config/rledger/config.toml
//! ```

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use rustledger::config::Config;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

/// rledger - A pure Rust implementation of Beancount
#[derive(Parser)]
#[command(name = "rledger")]
#[command(
    author,
    version,
    about = "Pure Rust implementation of Beancount, 10-30x faster"
)]
#[command(propagate_version = true)]
struct Cli {
    /// Use a specific profile from config
    #[arg(long, short = 'P', global = true)]
    profile: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate beancount files
    #[command(alias = "c")]
    Check {
        #[command(flatten)]
        args: rustledger::cmd::check::Args,
    },

    /// Query beancount files with BQL
    #[command(alias = "q")]
    Query {
        #[command(flatten)]
        args: rustledger::cmd::query::Args,
    },

    /// Format beancount files
    #[command(alias = "fmt")]
    Format {
        #[command(flatten)]
        args: rustledger::cmd::format::Args,
    },

    /// Generate financial reports
    #[command(alias = "r")]
    Report {
        #[command(flatten)]
        args: rustledger::cmd::report_cmd::Args,
    },

    /// Debugging and diagnostic tools
    #[command(alias = "d")]
    Doctor {
        #[command(flatten)]
        args: rustledger::cmd::doctor::Args,
    },

    /// Extract transactions from bank files
    #[command(alias = "x")]
    Extract {
        #[command(flatten)]
        args: rustledger::cmd::extract_cmd::Args,
    },

    /// Fetch commodity prices
    #[command(alias = "p")]
    Price {
        #[command(flatten)]
        args: rustledger::cmd::price_cmd::Args,
    },

    /// Manage configuration
    #[command(alias = "cfg")]
    Config {
        #[command(flatten)]
        args: rustledger::cmd::config_cmd::Args,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Get the effective file path from CLI arg or config.
fn get_file(file: Option<&PathBuf>, config: &Config, profile: Option<&str>) -> Option<PathBuf> {
    file.cloned()
        .or_else(|| config.effective_file_path(profile))
}

/// Helper to resolve file and return error if not found.
fn require_file(
    file: Option<&PathBuf>,
    config: &Config,
    profile: Option<&str>,
) -> Result<PathBuf, ExitCode> {
    get_file(file, config, profile).ok_or_else(|| {
        eprintln!("error: FILE is required (or set default.file in config)");
        eprintln!("  hint: run 'rledger config init' to create a config file");
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Load config (ignore errors - we'll use defaults)
    let config = Config::load().map(|l| l.config).unwrap_or_default();

    match cli.command {
        Commands::Check { mut args } => {
            // If no file specified, try to get from config
            if args.file.is_none() && args.generate_completions.is_none() {
                args.file = config.effective_file_path(cli.profile.as_deref());
            }
            match rustledger::cmd::check::run(&args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(2)
                }
            }
        }
        Commands::Query { mut args } => {
            // If no file specified, try to get from config
            if args.file.is_none() && args.generate_completions.is_none() {
                args.file = config.effective_file_path(cli.profile.as_deref());
            }
            match rustledger::cmd::query::run(&args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Format { mut args } => {
            // If no files specified, try to get from config
            if args.files.is_empty()
                && args.generate_completions.is_none()
                && let Some(file) = config.effective_file_path(cli.profile.as_deref())
            {
                args.files.push(file);
            }
            match rustledger::cmd::format::run(&args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(2)
                }
            }
        }
        Commands::Report { args } => {
            // Report requires file and report subcommand
            let file = match require_file(args.file.as_ref(), &config, cli.profile.as_deref()) {
                Ok(f) => f,
                Err(code) => return code,
            };
            let Some(ref report) = args.report else {
                eprintln!("error: a report subcommand is required");
                return ExitCode::from(2);
            };
            match rustledger::cmd::report_cmd::run(&file, report, args.verbose, &args.format) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Doctor { args } => {
            let Some(command) = args.command else {
                eprintln!("error: a doctor subcommand is required");
                return ExitCode::from(2);
            };
            match rustledger::cmd::doctor::run(command) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Extract { args } => {
            let file = match require_file(args.file.as_ref(), &config, cli.profile.as_deref()) {
                Ok(f) => f,
                Err(code) => return code,
            };
            match rustledger::cmd::extract_cmd::run(&args, &file) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Price { args } => match rustledger::cmd::price_cmd::run(&args.price_args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
        Commands::Config { args } => match rustledger::cmd::config_cmd::run(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(1)
            }
        },
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "rledger", &mut io::stdout());
            ExitCode::SUCCESS
        }
    }
}
