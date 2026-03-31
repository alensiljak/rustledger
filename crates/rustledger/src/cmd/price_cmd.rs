//! Price fetching command for rustledger.
//!
//! Fetches current prices for commodities from configurable online sources.

use crate::cmd::completions::ShellType;
use crate::cmd::price::sources::PriceSource;
use crate::cmd::price::{PriceRequest, PriceSourceRegistry};
use crate::config::{CommodityMapping, Config, PriceConfig};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::Parser;
use rustledger_loader::Loader;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

/// Fetch current prices for commodities.
#[derive(Parser, Debug)]
#[command(name = "price", about = "Fetch current prices for commodities")]
pub struct Args {
    /// Generate shell completions for the specified shell.
    #[arg(long, value_name = "SHELL")]
    generate_completions: Option<ShellType>,

    /// Price command arguments.
    #[command(flatten)]
    pub price_args: PriceArgs,
}

/// Price-specific arguments.
#[derive(Parser, Debug)]
pub struct PriceArgs {
    /// Beancount file to read commodities from (optional).
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Specific commodity symbols to fetch (e.g., AAPL, MSFT).
    #[arg(value_name = "SYMBOL")]
    symbols: Vec<String>,

    /// Base currency for price quotes.
    #[arg(short = 'c', long, default_value = "USD")]
    currency: String,

    /// Date for prices (YYYY-MM-DD, defaults to today).
    #[arg(short, long)]
    date: Option<String>,

    /// Output as beancount price directives.
    #[arg(short = 'b', long)]
    beancount: bool,

    /// Show verbose output.
    #[arg(short, long)]
    verbose: bool,

    /// Symbol mapping (e.g., VTI:VTI,BTC:BTC-USD).
    /// Maps commodity names to ticker symbols.
    #[arg(short = 'm', long, value_delimiter = ',')]
    mapping: Vec<String>,

    /// Use specific source (overrides mapping).
    #[arg(short = 's', long)]
    source: Option<String>,

    /// Use ad-hoc external command as source.
    /// The command receives the ticker as the first argument.
    #[arg(long, value_name = "CMD")]
    source_cmd: Option<String>,

    /// List configured sources and exit.
    #[arg(long)]
    list_sources: bool,
}

/// Main entry point with custom binary name (for bean-price compatibility).
pub fn main_with_name(bin_name: &str) -> ExitCode {
    let mut args = Args::parse();

    // Handle shell completion generation
    if let Some(shell) = args.generate_completions {
        crate::cmd::completions::generate_completions::<Args>(shell, bin_name);
        return ExitCode::SUCCESS;
    }

    // Load configuration
    let config = Config::load().map(|l| l.config).unwrap_or_default();

    // If no file or symbols specified, try to get file from config
    if args.price_args.file.is_none() && args.price_args.symbols.is_empty() {
        let profile = std::env::var("RLEDGER_PROFILE").ok();
        args.price_args.file = config.effective_file_path(profile.as_deref());
    }

    match run(&args.price_args, &config.price) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// Run the price command.
pub fn run(args: &PriceArgs, price_config: &PriceConfig) -> Result<()> {
    // Create the registry with config
    let registry = PriceSourceRegistry::new(price_config);

    // Handle --list-sources
    if args.list_sources {
        return list_sources(&registry);
    }

    // Handle --source-cmd (ad-hoc external command)
    if let Some(cmd) = &args.source_cmd {
        return run_with_external_command(args, cmd);
    }

    let mut symbols_to_fetch: Vec<String> = args.symbols.clone();

    // Build symbol mapping from CLI args
    let mut cli_mapping: HashMap<String, CommodityMapping> = HashMap::new();
    for mapping in &args.mapping {
        if let Some((from, to)) = mapping.split_once(':') {
            cli_mapping.insert(from.to_string(), CommodityMapping::Simple(to.to_string()));
        }
    }

    // If a file is provided, extract commodity symbols
    if let Some(ref file) = args.file {
        let mut loader = Loader::new();
        let ledger = loader.load(file)?;

        // Get commodities that might have ticker symbols
        for spanned in &ledger.directives {
            if let rustledger_core::Directive::Commodity(comm) = &spanned.value {
                let symbol = comm.currency.as_str();
                // Check if it looks like a ticker symbol (uppercase letters)
                if symbol
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                    && symbol.len() <= 10
                    && !symbols_to_fetch.contains(&symbol.to_string())
                {
                    symbols_to_fetch.push(symbol.to_string());
                }
            }
        }
    }

    if symbols_to_fetch.is_empty() {
        eprintln!(
            "No symbols to fetch. Provide symbols as arguments or use -f with a beancount file."
        );
        return Ok(());
    }

    if args.verbose {
        eprintln!("Fetching prices for: {symbols_to_fetch:?}");
    }

    // Parse target date
    let date = if let Some(ref d) = args.date {
        Some(
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .with_context(|| format!("Invalid date: {d}"))?,
        )
    } else {
        None
    };

    // Merge CLI mapping with config mapping (CLI takes precedence)
    let mut combined_mapping = price_config.mapping.clone();
    for (k, v) in cli_mapping {
        combined_mapping.insert(k, v);
    }

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Fetch prices
    for symbol in &symbols_to_fetch {
        let result = if let Some(source_name) = &args.source {
            // Use explicit source
            fetch_with_source(&registry, source_name, symbol, &args.currency, date)
        } else {
            // Use mapping/default
            registry.fetch_price(symbol, &args.currency, date, &combined_mapping)
        };

        match result {
            Ok(response) => {
                if args.beancount {
                    // Output as beancount price directive
                    let date_str = response.date.format("%Y-%m-%d");
                    writeln!(
                        handle,
                        "{date_str} price {symbol} {} {}",
                        response.price, response.currency
                    )?;
                } else {
                    writeln!(handle, "{symbol}: {} {}", response.price, response.currency)?;
                }
            }
            Err(e) => {
                if args.verbose {
                    eprintln!("Error fetching {symbol}: {e}");
                } else {
                    eprintln!("; Failed to fetch {symbol}: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Fetch a price using a specific source.
fn fetch_with_source(
    registry: &PriceSourceRegistry,
    source_name: &str,
    ticker: &str,
    currency: &str,
    date: Option<NaiveDate>,
) -> Result<crate::cmd::price::PriceResponse> {
    let source = registry
        .get(source_name)
        .with_context(|| format!("Unknown source: {source_name}"))?;

    let request = PriceRequest {
        ticker: ticker.to_string(),
        currency: currency.to_string(),
        date,
    };

    source.fetch_price(&request)
}

/// Run with an ad-hoc external command.
fn run_with_external_command(args: &PriceArgs, cmd: &str) -> Result<()> {
    use crate::cmd::price::external::ExternalCommandSource;

    // Parse the command string into parts
    let command_parts: Vec<String> =
        shell_words::split(cmd).with_context(|| format!("Failed to parse command: {cmd}"))?;

    if command_parts.is_empty() {
        anyhow::bail!("Empty command provided");
    }

    let source = ExternalCommandSource::new(command_parts, Duration::from_secs(30), HashMap::new());

    let date = if let Some(ref d) = args.date {
        Some(
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .with_context(|| format!("Invalid date: {d}"))?,
        )
    } else {
        None
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for symbol in &args.symbols {
        let request = PriceRequest {
            ticker: symbol.clone(),
            currency: args.currency.clone(),
            date,
        };

        match source.fetch_price(&request) {
            Ok(response) => {
                if args.beancount {
                    let date_str = response.date.format("%Y-%m-%d");
                    writeln!(
                        handle,
                        "{date_str} price {symbol} {} {}",
                        response.price, response.currency
                    )?;
                } else {
                    writeln!(handle, "{symbol}: {} {}", response.price, response.currency)?;
                }
            }
            Err(e) => {
                if args.verbose {
                    eprintln!("Error fetching {symbol}: {e}");
                } else {
                    eprintln!("; Failed to fetch {symbol}: {e}");
                }
            }
        }
    }

    Ok(())
}

/// List all configured sources.
fn list_sources(registry: &PriceSourceRegistry) -> Result<()> {
    println!("Available price sources:");
    println!();

    let sources = registry.list_sources();
    let default_source = registry.default_source_name();

    for name in sources {
        if let Some(source) = registry.get(name) {
            let default_marker = if name == default_source {
                " (default)"
            } else {
                ""
            };
            let api_key_note = if source.requires_api_key() {
                if let Some(env_var) = source.api_key_env_var() {
                    if std::env::var(env_var).is_ok() {
                        " [API key set]"
                    } else {
                        " [API key required]"
                    }
                } else {
                    " [API key required]"
                }
            } else {
                ""
            };
            println!("  {name}{default_marker}{api_key_note}");
            println!("    {}", source.description());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_args_parsing() {
        let args = Args::parse_from(["price", "AAPL", "MSFT"]);
        assert_eq!(args.price_args.symbols, vec!["AAPL", "MSFT"]);
        assert_eq!(args.price_args.currency, "USD");
        assert!(!args.price_args.beancount);
    }

    #[test]
    fn test_price_args_with_options() {
        let args = Args::parse_from([
            "price",
            "-c",
            "EUR",
            "-b",
            "-m",
            "BTC:BTC-USD,ETH:ETH-USD",
            "BTC",
            "ETH",
        ]);
        assert_eq!(args.price_args.symbols, vec!["BTC", "ETH"]);
        assert_eq!(args.price_args.currency, "EUR");
        assert!(args.price_args.beancount);
        assert_eq!(args.price_args.mapping.len(), 2);
    }

    #[test]
    fn test_price_args_with_source() {
        let args = Args::parse_from(["price", "-s", "coinbase", "BTC"]);
        assert_eq!(args.price_args.source, Some("coinbase".to_string()));
        assert_eq!(args.price_args.symbols, vec!["BTC"]);
    }

    #[test]
    fn test_price_args_with_source_cmd() {
        let args = Args::parse_from(["price", "--source-cmd", "echo 150.00 USD", "AAPL"]);
        assert_eq!(
            args.price_args.source_cmd,
            Some("echo 150.00 USD".to_string())
        );
    }

    #[test]
    fn test_price_args_list_sources() {
        let args = Args::parse_from(["price", "--list-sources"]);
        assert!(args.price_args.list_sources);
    }
}
