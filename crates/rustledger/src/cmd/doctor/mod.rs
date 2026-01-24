//! bean-doctor - Debugging tool for beancount files.
//!
//! This is the Rust equivalent of Python beancount's `bean-doctor` command.
//!
//! # Usage
//!
//! ```bash
//! bean-doctor lex ledger.beancount         # Dump lexer tokens
//! bean-doctor context ledger.beancount 42  # Show context at line 42
//! bean-doctor linked ledger.beancount ^trip-2024  # Find linked transactions
//! bean-doctor missing-open ledger.beancount  # Generate missing Open directives
//! bean-doctor list-options                 # List available options
//! ```

use crate::cmd::completions::ShellType;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

mod context;
mod directories;
mod display_context;
mod generate_synthetic;
mod lex;
mod linked;
mod missing_open;
mod options;
mod parse;
mod region;
mod roundtrip;
mod stats;

/// Debugging tool for beancount files.
#[derive(Parser, Debug)]
#[command(name = "bean-doctor")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Generate shell completions and exit
    #[arg(long, value_name = "SHELL", hide = true)]
    generate_completions: Option<ShellType>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Dump the lexer output for a beancount file
    #[command(alias = "dump-lexer")]
    Lex {
        /// The beancount file to lex
        file: PathBuf,
    },

    /// Parse a ledger and show parsed directives
    Parse {
        /// The beancount file to parse
        file: PathBuf,
        /// Show detailed output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show transaction context at a location
    Context {
        /// The beancount file
        file: PathBuf,
        /// Line number to show context for
        line: usize,
    },

    /// Find transactions linked by a link or at a location
    Linked {
        /// The beancount file
        file: PathBuf,
        /// Link name (^link), tag name (#tag), or line number
        location: String,
    },

    /// Print Open directives missing in a file
    MissingOpen {
        /// The beancount file
        file: PathBuf,
    },

    /// List available beancount options
    ListOptions,

    /// Print options parsed from a ledger
    PrintOptions {
        /// The beancount file
        file: PathBuf,
    },

    /// Display statistics about a ledger
    Stats {
        /// The beancount file
        file: PathBuf,
    },

    /// Display the decimal precision context inferred from the file
    DisplayContext {
        /// The beancount file
        file: PathBuf,
    },

    /// Round-trip test on arbitrary ledger
    Roundtrip {
        /// The beancount file
        file: PathBuf,
    },

    /// Validate a directory hierarchy against the ledger's account names
    Directories {
        /// The beancount file
        file: PathBuf,
        /// Directory roots to validate
        #[arg(value_name = "DIR")]
        dirs: Vec<PathBuf>,
    },

    /// Print transactions in a line range with balances
    Region {
        /// The beancount file
        file: PathBuf,
        /// Start line number
        start_line: usize,
        /// End line number
        end_line: usize,
        /// Convert balances to market value or cost
        #[arg(long, value_enum)]
        conversion: Option<Conversion>,
    },

    /// Generate synthetic beancount files for testing
    GenerateSynthetic {
        /// Output directory for generated files
        #[arg(short, long, default_value = "tests/compat/synthetic")]
        output: PathBuf,

        /// Number of files to generate (for proptest-style generation)
        #[arg(short, long, default_value = "50")]
        count: usize,

        /// Random seed for reproducibility
        #[arg(short, long)]
        seed: Option<u64>,

        /// Skip bean-check validation (faster but may produce invalid files)
        #[arg(long)]
        skip_validation: bool,

        /// Write manifest file tracking generated files
        #[arg(long)]
        manifest: bool,

        /// Generate edge case files only
        #[arg(long)]
        edge_cases_only: bool,
    },
}

/// Conversion type for region balances
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Conversion {
    /// Convert to market value using price database
    Value,
    /// Convert to cost basis
    Cost,
}

/// Main entry point for the doctor command.
pub fn main() -> ExitCode {
    main_with_name("rledger-doctor")
}

/// Main entry point with custom binary name (for bean-doctor compatibility).
pub fn main_with_name(bin_name: &str) -> ExitCode {
    let args = Args::parse();

    // Handle shell completion generation
    if let Some(shell) = args.generate_completions {
        crate::cmd::completions::generate_completions::<Args>(shell, bin_name);
        return ExitCode::SUCCESS;
    }

    // Command is required when not generating completions
    let Some(command) = args.command else {
        eprintln!("error: a subcommand is required");
        eprintln!("For more information, try '--help'");
        return ExitCode::from(2);
    };

    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(command: Command) -> Result<()> {
    let mut stdout = io::stdout().lock();

    match command {
        Command::Lex { file } => lex::cmd_lex(&file, &mut stdout),
        Command::Parse { file, verbose } => parse::cmd_parse(&file, verbose, &mut stdout),
        Command::Context { file, line } => context::cmd_context(&file, line, &mut stdout),
        Command::Linked { file, location } => linked::cmd_linked(&file, &location, &mut stdout),
        Command::MissingOpen { file } => missing_open::cmd_missing_open(&file, &mut stdout),
        Command::ListOptions => options::cmd_list_options(&mut stdout),
        Command::PrintOptions { file } => options::cmd_print_options(&file, &mut stdout),
        Command::Stats { file } => stats::cmd_stats(&file, &mut stdout),
        Command::DisplayContext { file } => {
            display_context::cmd_display_context(&file, &mut stdout)
        }
        Command::Roundtrip { file } => roundtrip::cmd_roundtrip(&file, &mut stdout),
        Command::Directories { file, dirs } => {
            directories::cmd_directories(&file, &dirs, &mut stdout)
        }
        Command::Region {
            file,
            start_line,
            end_line,
            conversion,
        } => region::cmd_region(&file, start_line, end_line, conversion, &mut stdout),
        Command::GenerateSynthetic {
            output,
            count,
            seed,
            skip_validation,
            manifest,
            edge_cases_only,
        } => generate_synthetic::cmd_generate_synthetic(
            &output,
            count,
            seed,
            skip_validation,
            manifest,
            edge_cases_only,
            &mut stdout,
        ),
    }
}
