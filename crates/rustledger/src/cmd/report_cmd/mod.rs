//! rledger report - Generate financial reports from beancount files.
//!
//! This is the primary rustledger command for generating reports.
//! For backwards compatibility with Python beancount, `bean-report` is also available.
//!
//! # Usage
//!
//! ```bash
//! rledger report ledger.beancount balances
//! rledger report ledger.beancount income
//! rledger report ledger.beancount holdings
//! ```
//!
//! # Reports
//!
//! - `balances` - Show account balances
//! - `accounts` - List all accounts
//! - `commodities` - List all commodities
//! - `prices` - Show price history
//! - `stats` - Show ledger statistics

// Allow inner helper functions after statements for cleaner report code organization
#![allow(clippy::items_after_statements)]

mod accounts;
mod balances;
mod balsheet;
mod commodities;
mod holdings;
mod income;
mod journal;
mod networth;
mod prices;
mod stats;

use crate::cmd::completions::ShellType;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rustledger_booking::interpolate;
use rustledger_core::{Directive, NaiveDate};
use rustledger_loader::Loader;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

/// Generate reports from beancount files.
#[derive(Parser, Debug)]
#[command(name = "report")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Generate shell completions and exit
    #[arg(long, value_name = "SHELL", hide = true)]
    generate_completions: Option<ShellType>,

    /// The beancount file to process
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// The report to generate
    #[command(subcommand)]
    pub report: Option<Report>,

    /// Show verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format (text, csv, json)
    #[arg(short = 'f', long, global = true, default_value = "text")]
    pub format: OutputFormat,
}

/// Output format for reports.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// CSV output.
    Csv,
    /// JSON output.
    Json,
}

/// Available report types.
#[derive(Subcommand, Debug)]
pub enum Report {
    /// Show account balances
    Balances {
        /// Filter to accounts matching this prefix
        #[arg(short, long)]
        account: Option<String>,
    },
    /// Balance sheet (Assets, Liabilities, Equity)
    #[command(alias = "bal")]
    Balsheet,
    /// Income statement (Income and Expenses)
    #[command(alias = "is")]
    Income,
    /// Transaction journal/register
    #[command(alias = "register")]
    Journal {
        /// Filter to accounts matching this prefix
        #[arg(short, long)]
        account: Option<String>,
        /// Limit number of entries
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Investment holdings with cost basis
    Holdings {
        /// Filter to accounts matching this prefix
        #[arg(short, long)]
        account: Option<String>,
    },
    /// Net worth over time
    Networth {
        /// Group by period (daily, weekly, monthly, yearly)
        #[arg(short, long, default_value = "monthly")]
        period: String,
    },
    /// List all accounts
    Accounts,
    /// List all commodities/currencies
    Commodities,
    /// Show ledger statistics
    Stats,
    /// Show price entries
    Prices {
        /// Filter to specific commodity
        #[arg(short, long)]
        commodity: Option<String>,
    },
}

/// Main entry point with custom binary name (for bean-report compatibility).
pub fn main_with_name(bin_name: &str) -> ExitCode {
    let args = Args::parse();

    // Handle shell completion generation
    if let Some(shell) = args.generate_completions {
        crate::cmd::completions::generate_completions::<Args>(shell, bin_name);
        return ExitCode::SUCCESS;
    }

    // File and report are required when not generating completions
    let Some(file) = args.file else {
        eprintln!("error: FILE is required");
        eprintln!("For more information, try '--help'");
        return ExitCode::from(2);
    };

    let Some(report) = args.report else {
        eprintln!("error: a report subcommand is required");
        eprintln!("For more information, try '--help'");
        return ExitCode::from(2);
    };

    match run(&file, &report, args.verbose, &args.format) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// Run the report command with the given arguments.
pub fn run(file: &PathBuf, report: &Report, verbose: bool, format: &OutputFormat) -> Result<()> {
    let mut stdout = io::stdout().lock();

    // Check if file exists
    if !file.exists() {
        anyhow::bail!("file not found: {}", file.display());
    }

    // Load the file
    if verbose {
        eprintln!("Loading {}...", file.display());
    }

    let mut loader = Loader::new();
    let load_result = loader
        .load(file)
        .with_context(|| format!("failed to load {}", file.display()))?;

    // Extract directives (move, not clone)
    let mut directives: Vec<_> = load_result
        .directives
        .into_iter()
        .map(|s| s.value)
        .collect();

    // Interpolate transactions
    for directive in &mut directives {
        if let Directive::Transaction(txn) = directive {
            if let Ok(result) = interpolate(txn) {
                *txn = result.transaction;
            }
        }
    }

    // Generate the requested report
    match report {
        Report::Balances { account } => {
            balances::report_balances(&directives, account.as_deref(), format, &mut stdout)?;
        }
        Report::Balsheet => {
            balsheet::report_balsheet(&directives, format, &mut stdout)?;
        }
        Report::Income => {
            income::report_income(&directives, format, &mut stdout)?;
        }
        Report::Journal { account, limit } => {
            journal::report_journal(&directives, account.as_deref(), *limit, format, &mut stdout)?;
        }
        Report::Holdings { account } => {
            holdings::report_holdings(&directives, account.as_deref(), format, &mut stdout)?;
        }
        Report::Networth { period } => {
            networth::report_networth(&directives, period, format, &mut stdout)?;
        }
        Report::Accounts => {
            accounts::report_accounts(&directives, format, &mut stdout)?;
        }
        Report::Commodities => {
            commodities::report_commodities(&directives, format, &mut stdout)?;
        }
        Report::Stats => {
            stats::report_stats(&directives, file, &mut stdout)?;
        }
        Report::Prices { commodity } => {
            prices::report_prices(&directives, commodity.as_deref(), format, &mut stdout)?;
        }
    }

    Ok(())
}

/// Escape a string for CSV output.
pub(super) fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Escape a string for JSON output.
pub(super) fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[derive(Default)]
pub(super) struct LedgerStats {
    pub transactions: usize,
    pub postings: usize,
    pub accounts: usize,
    pub commodities: usize,
    pub balance_assertions: usize,
    pub prices: usize,
    pub pads: usize,
    pub events: usize,
    pub notes: usize,
    pub documents: usize,
    pub queries: usize,
    pub custom: usize,
    pub first_date: Option<NaiveDate>,
    pub last_date: Option<NaiveDate>,
}
