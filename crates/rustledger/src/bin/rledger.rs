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
//! ```

use clap::{Parser, Subcommand};
use std::process::ExitCode;

/// rledger - A pure Rust implementation of Beancount
#[derive(Parser)]
#[command(name = "rledger")]
#[command(author, version, about = "Pure Rust implementation of Beancount, 10-30x faster")]
#[command(propagate_version = true)]
struct Cli {
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { args } => {
            match rustledger::cmd::check::run(&args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(2)
                }
            }
        }
        Commands::Query { args } => {
            match rustledger::cmd::query::run(&args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Format { args } => {
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
            let Some(ref file) = args.file else {
                eprintln!("error: FILE is required");
                return ExitCode::from(2);
            };
            let Some(ref report) = args.report else {
                eprintln!("error: a report subcommand is required");
                return ExitCode::from(2);
            };
            match rustledger::cmd::report_cmd::run(file, report, args.verbose, &args.format) {
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
            let Some(ref file) = args.file else {
                eprintln!("error: FILE is required");
                return ExitCode::from(2);
            };
            match rustledger::cmd::extract_cmd::run(&args, file) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        Commands::Price { args } => {
            match rustledger::cmd::price_cmd::run(&args.price_args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
