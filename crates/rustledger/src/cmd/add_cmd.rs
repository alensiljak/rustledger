//! Add transactions to beancount files.
//!
//! Provides interactive and quick modes for adding transactions:
//!
//! - Interactive mode: Prompts for date, payee, accounts, amounts
//! - Quick mode: One-liner for scripting
//!
//! # Usage
//!
//! ```bash
//! # Interactive mode
//! rledger add ledger.beancount
//!
//! # Quick mode
//! rledger add -q "Coffee Shop" "Morning coffee" Expenses:Food 4.50USD Assets:Checking
//! ```

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};
use clap::Parser;
use rust_decimal::Decimal;
use rustledger_core::format::{FormatConfig, format_directive};
use rustledger_core::{Amount, Directive, Posting, Transaction};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

/// Add transactions to beancount files.
#[derive(Parser, Debug)]
#[command(name = "add")]
pub struct Args {
    /// File to append transaction to.
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Transaction date (YYYY-MM-DD, "today", "yesterday", "+1", "-1").
    #[arg(short, long)]
    pub date: Option<String>,

    /// Print transaction without appending.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Skip confirmation prompt.
    #[arg(short, long)]
    pub yes: bool,

    /// Quick mode: payee narration account amount \[account \[amount\]\]...
    #[arg(short, long, num_args = 4.., value_name = "ARGS")]
    pub quick: Option<Vec<String>>,
}

/// Parse a flexible date string.
///
/// Supports:
/// - "today" or empty → current date
/// - "yesterday" → previous day
/// - "+N" / "-N" → relative days from today
/// - "YYYY-MM-DD" → explicit date
pub fn parse_date(input: &str) -> Result<NaiveDate> {
    let trimmed = input.trim().to_lowercase();
    let today = Local::now().date_naive();

    if trimmed.is_empty() || trimmed == "today" {
        return Ok(today);
    }

    if trimmed == "yesterday" {
        return today.pred_opt().context("Cannot compute yesterday's date");
    }

    // Relative days: +N or -N
    if let Some(stripped) = trimmed.strip_prefix('+') {
        let days: i64 = stripped
            .parse()
            .with_context(|| format!("Invalid relative date: {input}"))?;
        return today
            .checked_add_signed(chrono::Duration::days(days))
            .context("Date out of range");
    }

    if let Some(stripped) = trimmed.strip_prefix('-') {
        let days: i64 = stripped
            .parse()
            .with_context(|| format!("Invalid relative date: {input}"))?;
        return today
            .checked_sub_signed(chrono::Duration::days(days))
            .context("Date out of range");
    }

    // Explicit date: YYYY-MM-DD
    NaiveDate::parse_from_str(&trimmed, "%Y-%m-%d")
        .with_context(|| format!("Invalid date format: {input}. Use YYYY-MM-DD."))
}

/// Parse an amount string like "123.45 USD" or "123.45USD".
pub fn parse_amount(input: &str) -> Result<Amount> {
    let trimmed = input.trim();

    // Find where the number ends and currency begins
    // Currency is uppercase letters at the end
    let mut split_pos = trimmed.len();
    for (i, c) in trimmed.char_indices().rev() {
        if c.is_ascii_uppercase() || c == '-' {
            split_pos = i;
        } else {
            break;
        }
    }

    // Handle case where there's a space between number and currency
    let (number_part, currency_part) = if split_pos == trimmed.len() {
        // Try splitting by whitespace
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            bail!("Invalid amount format: {input}. Expected '123.45 USD' or '123.45USD'.");
        }
    } else {
        let num = trimmed[..split_pos].trim();
        let cur = trimmed[split_pos..].trim();
        (num, cur)
    };

    let number = Decimal::from_str(number_part)
        .with_context(|| format!("Invalid number in amount: {number_part}"))?;

    if currency_part.is_empty() {
        bail!("Missing currency in amount: {input}");
    }

    Ok(Amount::new(number, currency_part))
}

/// Calculate the balancing amount for a transaction.
///
/// Returns the negative sum of all provided amounts.
/// Only works when all amounts have the same currency.
pub fn calculate_balance(amounts: &[Amount]) -> Result<Amount> {
    if amounts.is_empty() {
        bail!("Cannot calculate balance with no amounts");
    }

    let currency = &amounts[0].currency;

    // Verify all amounts have the same currency
    for amt in amounts.iter().skip(1) {
        if amt.currency != *currency {
            bail!(
                "Cannot auto-balance: mixed currencies ({} and {})",
                currency,
                amt.currency
            );
        }
    }

    let sum: Decimal = amounts.iter().map(|a| a.number).sum();
    Ok(Amount::new(-sum, currency.as_str()))
}

/// Run the add command in quick mode.
fn run_quick_mode(args: &Args, file: &PathBuf, date: NaiveDate) -> Result<()> {
    let quick_args = args.quick.as_ref().expect("quick mode args");

    if quick_args.len() < 4 {
        bail!("Quick mode requires at least: payee narration account amount");
    }

    let payee = &quick_args[0];
    let narration = &quick_args[1];

    // Parse account/amount pairs
    let mut postings: Vec<Posting> = Vec::new();
    let mut amounts: Vec<Amount> = Vec::new();
    let mut i = 2;

    while i < quick_args.len() {
        let account = &quick_args[i];
        i += 1;

        if i < quick_args.len() {
            // Try to parse as amount
            if let Ok(amount) = parse_amount(&quick_args[i]) {
                postings.push(Posting::new(account.as_str(), amount.clone()));
                amounts.push(amount);
                i += 1;
            } else {
                // No amount for this account - will be auto-balanced
                postings.push(Posting::auto(account.as_str()));
            }
        } else {
            // Last account with no amount - auto-balance
            postings.push(Posting::auto(account.as_str()));
        }
    }

    // If the last posting has no amount, calculate and set it
    if let Some(last) = postings.last_mut()
        && !last.has_units()
        && !amounts.is_empty()
    {
        let balance = calculate_balance(&amounts)?;
        *last = Posting::new(last.account.as_str(), balance);
    }

    // Build the transaction
    let mut txn = Transaction::new(date, narration.as_str()).with_flag('*');

    if !payee.is_empty() {
        txn = txn.with_payee(payee.as_str());
    }

    for posting in postings {
        txn = txn.with_posting(posting);
    }

    // Format and display
    let config = FormatConfig::default();
    let directive = Directive::Transaction(txn);
    let formatted = format_directive(&directive, &config);

    if args.dry_run {
        println!("{formatted}");
        return Ok(());
    }

    // Show preview and confirm
    if !args.yes {
        println!("Preview:");
        println!("{formatted}");
        print!("Append to {}? [Y/n] ", file.display());
        std::io::stdout().flush()?;

        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        let response = response.trim().to_lowercase();

        if !response.is_empty() && response != "y" && response != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Append to file
    append_transaction(file, &formatted)?;

    println!("Transaction appended to {}", file.display());
    Ok(())
}

/// Append a formatted transaction to a file.
fn append_transaction(file: &PathBuf, formatted: &str) -> Result<()> {
    // Check if file exists
    let file_exists = file.exists();

    if !file_exists {
        // Create the file with just the transaction
        fs::write(file, formatted)
            .with_context(|| format!("Failed to create {}", file.display()))?;
        return Ok(());
    }

    // Read the file to check if it ends with newlines
    let content =
        fs::read_to_string(file).with_context(|| format!("Failed to read {}", file.display()))?;

    let mut f = OpenOptions::new()
        .append(true)
        .open(file)
        .with_context(|| format!("Failed to open {} for appending", file.display()))?;

    // Add blank line separator if file doesn't end with double newline
    if !content.ends_with("\n\n") {
        writeln!(f)?;
        if !content.ends_with('\n') {
            writeln!(f)?;
        }
    }

    write!(f, "{formatted}")?;

    Ok(())
}

/// Run the add command.
pub fn run(args: &Args, file: &PathBuf) -> Result<()> {
    // Parse the date
    let date = if let Some(ref d) = args.date {
        parse_date(d)?
    } else {
        Local::now().date_naive()
    };

    // Check if file exists, offer to create if not
    if !file.exists() && !args.dry_run {
        if !args.yes {
            print!("File {} does not exist. Create it? [y/N] ", file.display());
            std::io::stdout().flush()?;

            let mut response = String::new();
            std::io::stdin().read_line(&mut response)?;
            let response = response.trim().to_lowercase();

            if response != "y" && response != "yes" {
                bail!("File does not exist: {}", file.display());
            }
        }
        // Create parent directories if needed
        if let Some(parent) = file.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    // Dispatch to appropriate mode
    if args.quick.is_some() {
        run_quick_mode(args, file, date)
    } else {
        // Interactive mode - Phase 2
        bail!("Interactive mode not yet implemented. Use --quick (-q) for now.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date_today() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("today").unwrap(), today);
        assert_eq!(parse_date("").unwrap(), today);
        assert_eq!(parse_date("TODAY").unwrap(), today);
    }

    #[test]
    fn test_parse_date_yesterday() {
        let yesterday = Local::now().date_naive().pred_opt().unwrap();
        assert_eq!(parse_date("yesterday").unwrap(), yesterday);
        assert_eq!(parse_date("YESTERDAY").unwrap(), yesterday);
    }

    #[test]
    fn test_parse_date_relative() {
        let today = Local::now().date_naive();
        let tomorrow = today.succ_opt().unwrap();
        let yesterday = today.pred_opt().unwrap();

        assert_eq!(parse_date("+1").unwrap(), tomorrow);
        assert_eq!(parse_date("-1").unwrap(), yesterday);
        assert_eq!(
            parse_date("+7").unwrap(),
            today.checked_add_signed(chrono::Duration::days(7)).unwrap()
        );
    }

    #[test]
    fn test_parse_date_explicit() {
        assert_eq!(
            parse_date("2026-03-21").unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 21).unwrap()
        );
        assert_eq!(
            parse_date("2024-01-01").unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
        );
    }

    #[test]
    fn test_parse_date_invalid() {
        assert!(parse_date("not-a-date").is_err());
        assert!(parse_date("2026/03/21").is_err());
        assert!(parse_date("03-21-2026").is_err());
    }

    #[test]
    fn test_parse_amount_with_space() {
        let amt = parse_amount("123.45 USD").unwrap();
        assert_eq!(amt.number, Decimal::from_str("123.45").unwrap());
        assert_eq!(amt.currency.as_str(), "USD");
    }

    #[test]
    fn test_parse_amount_no_space() {
        let amt = parse_amount("123.45USD").unwrap();
        assert_eq!(amt.number, Decimal::from_str("123.45").unwrap());
        assert_eq!(amt.currency.as_str(), "USD");
    }

    #[test]
    fn test_parse_amount_negative() {
        let amt = parse_amount("-50.00 EUR").unwrap();
        assert_eq!(amt.number, Decimal::from_str("-50.00").unwrap());
        assert_eq!(amt.currency.as_str(), "EUR");
    }

    #[test]
    fn test_parse_amount_integer() {
        let amt = parse_amount("100 BTC").unwrap();
        assert_eq!(amt.number, Decimal::from_str("100").unwrap());
        assert_eq!(amt.currency.as_str(), "BTC");
    }

    #[test]
    fn test_parse_amount_invalid() {
        assert!(parse_amount("123.45").is_err()); // No currency
        assert!(parse_amount("USD").is_err()); // No number
        assert!(parse_amount("abc USD").is_err()); // Invalid number
    }

    #[test]
    fn test_calculate_balance_simple() {
        let amounts = vec![Amount::new(Decimal::from_str("100.00").unwrap(), "USD")];
        let balance = calculate_balance(&amounts).unwrap();
        assert_eq!(balance.number, Decimal::from_str("-100.00").unwrap());
        assert_eq!(balance.currency.as_str(), "USD");
    }

    #[test]
    fn test_calculate_balance_multiple() {
        let amounts = vec![
            Amount::new(Decimal::from_str("50.00").unwrap(), "USD"),
            Amount::new(Decimal::from_str("25.00").unwrap(), "USD"),
        ];
        let balance = calculate_balance(&amounts).unwrap();
        assert_eq!(balance.number, Decimal::from_str("-75.00").unwrap());
    }

    #[test]
    fn test_calculate_balance_mixed_currencies() {
        let amounts = vec![
            Amount::new(Decimal::from_str("100.00").unwrap(), "USD"),
            Amount::new(Decimal::from_str("50.00").unwrap(), "EUR"),
        ];
        assert!(calculate_balance(&amounts).is_err());
    }

    #[test]
    fn test_calculate_balance_empty() {
        let amounts: Vec<Amount> = vec![];
        assert!(calculate_balance(&amounts).is_err());
    }
}
