//! rledger extract - Extract transactions from bank files.
//!
//! This is the primary rustledger command for importing transactions from
//! CSV, OFX, and other bank statement formats.
//!
//! # Usage
//!
//! ```bash
//! rledger extract bank.csv --account Assets:Bank:Checking
//! rledger extract statement.csv --importer chase
//! ```
//!
//! # Importers Configuration
//!
//! Create an `importers.toml` file to define reusable import profiles with
//! column mappings and account categorization rules:
//!
//! ```toml
//! [[importers]]
//! name = "chase"
//! account = "Assets:Bank:Chase"
//! date_column = "Transaction Date"
//! amount_column = "Amount"
//! date_format = "%m/%d/%Y"
//!
//! [importers.mappings]
//! "AMAZON" = "Expenses:Shopping"
//! "WHOLE FOODS" = "Expenses:Groceries"
//! ```
//!
//! The file is searched for in the following locations (first found wins):
//! 1. Path specified via `--importers-config`
//! 2. `importers.toml` in the current directory
//! 3. `~/.config/rledger/importers.toml`

use crate::cmd::completions::ShellType;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use format_num_pattern::Locale;
use rustledger_core::{FormatConfig, format_directive};
use rustledger_importer::ImporterConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

/// Extract transactions from bank files.
#[derive(Parser, Debug)]
#[command(name = "extract")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Generate shell completions and exit
    #[arg(long, value_name = "SHELL", hide = true)]
    generate_completions: Option<ShellType>,

    /// The file to extract transactions from
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Use a named importer from importers.toml
    #[arg(long, short = 'i')]
    importer: Option<String>,

    /// Path to importers.toml configuration file
    #[arg(long)]
    importers_config: Option<PathBuf>,

    /// Target account for imported transactions
    #[arg(short, long, default_value = "Assets:Bank:Checking")]
    account: String,

    /// Currency for amounts (default: USD)
    #[arg(short, long, default_value = "USD")]
    currency: String,

    /// Date column name or index
    #[arg(long, default_value = "Date")]
    date_column: String,

    /// Date format (strftime-style)
    #[arg(long, default_value = "%Y-%m-%d")]
    date_format: String,

    /// Narration/description column name or index
    #[arg(long, default_value = "Description")]
    narration_column: String,

    /// Payee column name (optional)
    #[arg(long)]
    payee_column: Option<String>,

    /// Amount column name or index
    #[arg(long, default_value = "Amount")]
    amount_column: String,

    /// Locale used to parse amounts, e.g. `en_US`
    #[arg(long)]
    amount_locale: Option<String>,

    /// Custom formatting for parsing amounts.
    #[arg(long)]
    amount_format: Option<String>,

    /// Debit column (for separate debit/credit columns)
    #[arg(long)]
    debit_column: Option<String>,

    /// Credit column (for separate debit/credit columns)
    #[arg(long)]
    credit_column: Option<String>,

    /// CSV delimiter
    #[arg(long, default_value = ",")]
    delimiter: char,

    /// Number of header rows to skip
    #[arg(long, default_value = "0")]
    skip_rows: usize,

    /// Invert sign of amounts
    #[arg(long)]
    invert_sign: bool,

    /// CSV has no header row
    #[arg(long)]
    no_header: bool,
}

// --- Importers TOML configuration ---

/// Top-level importers configuration file.
#[derive(Debug, Deserialize)]
struct ImportersFile {
    importers: Vec<ImporterEntry>,
}

/// A single importer entry in importers.toml.
#[derive(Debug, Deserialize)]
struct ImporterEntry {
    /// Name used to select this importer via --importer flag.
    name: String,
    /// Target account for imported transactions.
    account: Option<String>,
    /// Currency (default: USD).
    currency: Option<String>,
    /// Date column name or 0-based index.
    date_column: Option<toml::Value>,
    /// Date format (strftime-style).
    date_format: Option<String>,
    /// Narration/description column name or index.
    narration_column: Option<toml::Value>,
    /// Payee column name or index.
    payee_column: Option<toml::Value>,
    /// Amount column name or index.
    amount_column: Option<toml::Value>,
    /// Debit column name or index.
    debit_column: Option<toml::Value>,
    /// Credit column name or index.
    credit_column: Option<toml::Value>,
    /// CSV delimiter character.
    delimiter: Option<String>,
    /// Number of rows to skip.
    skip_rows: Option<usize>,
    /// Whether the CSV has a header row.
    #[serde(default)]
    skip_header: Option<bool>,
    /// Whether to invert amount signs.
    #[serde(default)]
    invert_amounts: Option<bool>,
    /// Default expense account for unmatched transactions.
    default_expense: Option<String>,
    /// Default income account for unmatched negative-amount transactions.
    default_income: Option<String>,
    /// Account mappings: pattern → account.
    #[serde(default)]
    mappings: HashMap<String, String>,
}

/// Parse a TOML value as a column spec string (either a string name or integer index).
fn parse_column_value(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        _ => None,
    }
}

/// Find the importers.toml file, searching in standard locations.
///
/// If an explicit path is provided via `--importers-config`, it must exist
/// or an error is returned. Otherwise, searches the current directory and
/// then `~/.config/rledger/`.
fn find_importers_config(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    // 1. Explicit path from --importers-config — must exist
    if let Some(path) = explicit_path {
        if path.exists() {
            return Ok(Some(path.to_path_buf()));
        }
        return Err(anyhow!("Importers config not found: {}", path.display()));
    }

    // 2. Current directory
    let local = PathBuf::from("importers.toml");
    if local.exists() {
        return Ok(Some(local));
    }

    // 3. User config directory
    if let Some(config_dir) = dirs::config_dir() {
        let user_path = config_dir.join("rledger").join("importers.toml");
        if user_path.exists() {
            return Ok(Some(user_path));
        }
    }

    Ok(None)
}

/// Load and parse an importers.toml file.
fn load_importers_config(path: &Path) -> Result<ImportersFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read importers config: {}", path.display()))?;
    let config: ImportersFile = toml::from_str(&content)
        .with_context(|| format!("Failed to parse importers config: {}", path.display()))?;
    Ok(config)
}

/// Build an `ImporterConfig` from a named importer entry.
fn build_config_from_entry(entry: &ImporterEntry) -> Result<ImporterConfig> {
    let mut builder = ImporterConfig::csv();

    if let Some(ref account) = entry.account {
        builder = builder.account(account);
    }

    if let Some(ref currency) = entry.currency {
        builder = builder.currency(currency);
    }

    if let Some(ref val) = entry.date_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.date_column(&col);
    }

    if let Some(ref fmt) = entry.date_format {
        builder = builder.date_format(fmt);
    }

    if let Some(ref val) = entry.narration_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.narration_column(&col);
    }

    if let Some(ref val) = entry.payee_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.payee_column(&col);
    }

    if let Some(ref val) = entry.amount_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.amount_column(&col);
    }

    if let Some(ref val) = entry.debit_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.debit_column(&col);
    }

    if let Some(ref val) = entry.credit_column
        && let Some(col) = parse_column_value(val)
    {
        builder = builder.credit_column(&col);
    }

    if let Some(ref delim) = entry.delimiter
        && let Some(c) = delim.chars().next()
    {
        builder = builder.delimiter(c);
    }

    if let Some(skip) = entry.skip_rows {
        builder = builder.skip_rows(skip);
    }

    if let Some(skip_header) = entry.skip_header {
        builder = builder.has_header(!skip_header);
    }

    if let Some(invert) = entry.invert_amounts {
        builder = builder.invert_sign(invert);
    }

    if let Some(ref account) = entry.default_expense {
        builder = builder.default_expense(account);
    }

    if let Some(ref account) = entry.default_income {
        builder = builder.default_income(account);
    }

    if !entry.mappings.is_empty() {
        // Sort by pattern length descending so more specific patterns match first
        let mut mappings: Vec<(String, String)> = entry
            .mappings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        builder = builder.mappings(mappings);
    }

    builder.build()
}

/// Main entry point with custom binary name (for bean-extract compatibility).
pub fn main_with_name(bin_name: &str) -> ExitCode {
    let args = Args::parse();

    // Handle shell completion generation
    if let Some(shell) = args.generate_completions {
        crate::cmd::completions::generate_completions::<Args>(shell, bin_name);
        return ExitCode::SUCCESS;
    }

    // File is required when not generating completions
    let Some(ref file) = args.file else {
        eprintln!("error: FILE is required");
        eprintln!("For more information, try '--help'");
        return ExitCode::from(2);
    };

    match run(&args, file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

/// Run the extract command with the given arguments.
pub fn run(args: &Args, file: &PathBuf) -> Result<()> {
    let mut stdout = io::stdout().lock();

    // If --importer is specified, load config from importers.toml
    let config = if let Some(ref importer_name) = args.importer {
        let config_path = find_importers_config(args.importers_config.as_deref())?
            .ok_or_else(|| anyhow!(
                "No importers.toml found. Create one in the current directory or at ~/.config/rledger/importers.toml"
            ))?;

        let importers_file = load_importers_config(&config_path)?;

        let entry = importers_file
            .importers
            .iter()
            .find(|e| e.name == *importer_name)
            .ok_or_else(|| {
                let available: Vec<&str> = importers_file
                    .importers
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect();
                anyhow!(
                    "Importer '{}' not found in {}. Available: {}",
                    importer_name,
                    config_path.display(),
                    available.join(", ")
                )
            })?;

        eprintln!(
            "Using importer '{}' from {}",
            importer_name,
            config_path.display()
        );
        build_config_from_entry(entry)?
    } else {
        // Build from CLI arguments (existing behavior)
        let mut builder = ImporterConfig::csv()
            .account(&args.account)
            .currency(&args.currency)
            .date_column(&args.date_column)
            .date_format(&args.date_format)
            .narration_column(&args.narration_column)
            .amount_column(&args.amount_column)
            .delimiter(args.delimiter)
            .skip_rows(args.skip_rows)
            .invert_sign(args.invert_sign)
            .has_header(!args.no_header);

        if let Some(payee) = &args.payee_column {
            builder = builder.payee_column(payee);
        }

        if let Some(debit) = &args.debit_column {
            builder = builder.debit_column(debit);
        }

        if let Some(credit) = &args.credit_column {
            builder = builder.credit_column(credit);
        }

        if let Some(locale) = &args.amount_locale {
            let Ok(locale) = Locale::from_str(locale) else {
                return Err(anyhow!("{locale} is not a valid locale"));
            };

            builder = builder.amount_locale(locale);
        }

        if let Some(format) = &args.amount_format {
            builder = builder.amount_format(format);
        }

        builder.build()?
    };

    // Extract transactions
    let result = config.extract(file)?;

    // Print warnings
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    // Print extracted directives in beancount format
    let fmt_config = FormatConfig::default();
    for directive in &result.directives {
        writeln!(stdout, "{}", format_directive(directive, &fmt_config))?;
        writeln!(stdout)?;
    }

    eprintln!(
        "Extracted {} transactions from {}",
        result.directives.len(),
        file.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_importer::config::ImporterType;

    fn write_temp_config(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("importers.toml");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn test_load_importers_config_basic() {
        let (_dir, path) = write_temp_config(
            r#"
[[importers]]
name = "chase"
account = "Assets:Bank:Chase"
date_column = "Transaction Date"
amount_column = "Amount"
"#,
        );

        let config = load_importers_config(&path).unwrap();
        assert_eq!(config.importers.len(), 1);
        assert_eq!(config.importers[0].name, "chase");
        assert_eq!(
            config.importers[0].account.as_deref(),
            Some("Assets:Bank:Chase")
        );
    }

    #[test]
    fn test_load_importers_config_with_mappings() {
        let (_dir, path) = write_temp_config(
            r#"
[[importers]]
name = "checking"
account = "Assets:Bank:Checking"

[importers.mappings]
"AMAZON" = "Expenses:Shopping"
"WHOLE FOODS" = "Expenses:Groceries"
"#,
        );

        let config = load_importers_config(&path).unwrap();
        assert_eq!(config.importers[0].mappings.len(), 2);
        assert_eq!(
            config.importers[0].mappings.get("AMAZON"),
            Some(&"Expenses:Shopping".to_string())
        );
    }

    #[test]
    fn test_load_importers_config_multiple_importers() {
        let (_dir, path) = write_temp_config(
            r#"
[[importers]]
name = "checking"
account = "Assets:Bank:Checking"

[[importers]]
name = "credit_card"
account = "Liabilities:CreditCard"
invert_amounts = true
"#,
        );

        let config = load_importers_config(&path).unwrap();
        assert_eq!(config.importers.len(), 2);
        assert_eq!(config.importers[1].name, "credit_card");
        assert_eq!(config.importers[1].invert_amounts, Some(true));
    }

    #[test]
    fn test_load_importers_config_integer_columns() {
        let (_dir, path) = write_temp_config(
            r#"
[[importers]]
name = "noheader"
account = "Assets:Bank"
date_column = 0
amount_column = 3
narration_column = 1
"#,
        );

        let config = load_importers_config(&path).unwrap();
        let entry = &config.importers[0];
        assert_eq!(
            parse_column_value(entry.date_column.as_ref().unwrap()),
            Some("0".to_string())
        );
        assert_eq!(
            parse_column_value(entry.amount_column.as_ref().unwrap()),
            Some("3".to_string())
        );
    }

    #[test]
    fn test_load_importers_config_invalid_toml() {
        let (_dir, path) = write_temp_config("this is not valid toml [[[");
        assert!(load_importers_config(&path).is_err());
    }

    #[test]
    fn test_load_importers_config_missing_file() {
        let path = PathBuf::from("/nonexistent/importers.toml");
        assert!(load_importers_config(&path).is_err());
    }

    #[test]
    fn test_build_config_from_entry_basic() {
        let entry = ImporterEntry {
            name: "test".to_string(),
            account: Some("Assets:Bank:Test".to_string()),
            currency: Some("EUR".to_string()),
            date_column: Some(toml::Value::String("Date".to_string())),
            date_format: Some("%m/%d/%Y".to_string()),
            narration_column: Some(toml::Value::String("Description".to_string())),
            payee_column: None,
            amount_column: Some(toml::Value::String("Amount".to_string())),
            debit_column: None,
            credit_column: None,
            delimiter: None,
            skip_rows: None,
            skip_header: None,
            invert_amounts: None,
            default_expense: None,
            default_income: None,
            mappings: HashMap::new(),
        };

        let config = build_config_from_entry(&entry).unwrap();
        assert_eq!(config.account, "Assets:Bank:Test");
        assert_eq!(config.currency, Some("EUR".to_string()));
    }

    #[test]
    fn test_build_config_from_entry_with_mappings() {
        let mut mappings = HashMap::new();
        mappings.insert("AMAZON".to_string(), "Expenses:Shopping".to_string());
        mappings.insert("WHOLE FOODS".to_string(), "Expenses:Groceries".to_string());

        let entry = ImporterEntry {
            name: "test".to_string(),
            account: Some("Assets:Bank".to_string()),
            currency: None,
            date_column: None,
            date_format: None,
            narration_column: None,
            payee_column: None,
            amount_column: None,
            debit_column: None,
            credit_column: None,
            delimiter: None,
            skip_rows: None,
            skip_header: None,
            invert_amounts: None,
            default_expense: None,
            default_income: None,
            mappings,
        };

        let config = build_config_from_entry(&entry).unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(csv_config.mappings.len(), 2);
        // Patterns should be lowercased and sorted longest-first
        assert_eq!(csv_config.mappings[0].0, "whole foods");
        assert_eq!(csv_config.mappings[1].0, "amazon");
    }

    #[test]
    fn test_build_config_from_entry_with_default_expense() {
        let entry = ImporterEntry {
            name: "test".to_string(),
            account: Some("Assets:Bank".to_string()),
            currency: None,
            date_column: None,
            date_format: None,
            narration_column: None,
            payee_column: None,
            amount_column: None,
            debit_column: None,
            credit_column: None,
            delimiter: None,
            skip_rows: None,
            skip_header: None,
            invert_amounts: None,
            default_expense: Some("Expenses:Uncategorized".to_string()),
            default_income: Some("Income:Other".to_string()),
            mappings: HashMap::new(),
        };

        let config = build_config_from_entry(&entry).unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(
            csv_config.default_expense.as_deref(),
            Some("Expenses:Uncategorized")
        );
        assert_eq!(csv_config.default_income.as_deref(), Some("Income:Other"));
    }

    #[test]
    fn test_build_config_from_entry_all_options() {
        let entry = ImporterEntry {
            name: "full".to_string(),
            account: Some("Assets:Bank".to_string()),
            currency: Some("GBP".to_string()),
            date_column: Some(toml::Value::Integer(0)),
            date_format: Some("%d/%m/%Y".to_string()),
            narration_column: Some(toml::Value::Integer(2)),
            payee_column: Some(toml::Value::String("Payee".to_string())),
            amount_column: None,
            debit_column: Some(toml::Value::String("Debit".to_string())),
            credit_column: Some(toml::Value::String("Credit".to_string())),
            delimiter: Some(";".to_string()),
            skip_rows: Some(2),
            skip_header: Some(true),
            invert_amounts: Some(true),
            default_expense: None,
            default_income: None,
            mappings: HashMap::new(),
        };

        let config = build_config_from_entry(&entry).unwrap();
        assert_eq!(config.currency, Some("GBP".to_string()));
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(csv_config.delimiter, ';');
        assert_eq!(csv_config.skip_rows, 2);
        assert!(!csv_config.has_header); // skip_header=true → has_header=false
        assert!(csv_config.invert_sign);
    }

    #[test]
    fn test_find_importers_config_explicit_missing_returns_error() {
        let result = find_importers_config(Some(Path::new("/nonexistent/importers.toml")));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Importers config not found"));
    }

    #[test]
    fn test_find_importers_config_explicit_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("importers.toml");
        std::fs::write(&path, "[[importers]]\nname = \"test\"\n").unwrap();

        let result = find_importers_config(Some(&path)).unwrap();
        assert_eq!(result, Some(path));
    }

    #[test]
    fn test_find_importers_config_none_returns_ok() {
        // When no explicit path is given, the function should not error
        // (it may or may not find a file depending on the environment)
        let result = find_importers_config(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_end_to_end_extract_with_config() {
        let dir = tempfile::tempdir().unwrap();

        // Write importers.toml
        let config_path = dir.path().join("importers.toml");
        std::fs::write(
            &config_path,
            r#"
[[importers]]
name = "mybank"
account = "Assets:Bank:MyBank"
currency = "USD"
date_column = "Date"
narration_column = "Description"
amount_column = "Amount"
default_expense = "Expenses:Uncategorized"

[importers.mappings]
"GROCERY" = "Expenses:Food"
"#,
        )
        .unwrap();

        // Write CSV (positive amounts → expense side)
        let csv_path = dir.path().join("statement.csv");
        std::fs::write(
            &csv_path,
            "Date,Description,Amount\n\
             2024-01-15,GROCERY STORE,50.00\n\
             2024-01-16,RANDOM PURCHASE,25.00\n",
        )
        .unwrap();

        // Load config and extract
        let importers_file = load_importers_config(&config_path).unwrap();
        let entry = importers_file
            .importers
            .iter()
            .find(|e| e.name == "mybank")
            .unwrap();
        let config = build_config_from_entry(entry).unwrap();
        let result = config.extract(&csv_path).unwrap();

        assert_eq!(result.directives.len(), 2);

        // First should map to Expenses:Food via mapping
        if let rustledger_core::Directive::Transaction(txn) = &result.directives[0] {
            assert_eq!(txn.postings[0].account.as_str(), "Assets:Bank:MyBank");
            assert_eq!(txn.postings[1].account.as_str(), "Expenses:Food");
        } else {
            panic!("Expected transaction");
        }

        // Second should use default_expense since no mapping matches
        if let rustledger_core::Directive::Transaction(txn) = &result.directives[1] {
            assert_eq!(txn.postings[1].account.as_str(), "Expenses:Uncategorized");
        } else {
            panic!("Expected transaction");
        }
    }
}
