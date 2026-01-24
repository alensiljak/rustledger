//! Net worth report - Net worth over time.

use super::OutputFormat;
use anyhow::Result;
use chrono::Datelike;
use rust_decimal::Decimal;
use rustledger_core::{Directive, InternedStr};
use std::collections::BTreeMap;
use std::io::Write;

/// Generate a net worth over time report.
pub(super) fn report_networth<W: Write>(
    directives: &[Directive],
    period: &str,
    format: &OutputFormat,
    writer: &mut W,
) -> Result<()> {
    let mut transactions: Vec<_> = directives
        .iter()
        .filter_map(|d| {
            if let Directive::Transaction(txn) = d {
                Some(txn)
            } else {
                None
            }
        })
        .collect();

    transactions.sort_by_key(|t| t.date);

    if transactions.is_empty() {
        match format {
            OutputFormat::Csv => writeln!(writer, "period,currency,amount")?,
            OutputFormat::Json => writeln!(writer, "[]")?,
            OutputFormat::Text => writeln!(writer, "No transactions found.")?,
        }
        return Ok(());
    }

    let mut asset_balance: BTreeMap<InternedStr, Decimal> = BTreeMap::new();
    let mut liability_balance: BTreeMap<InternedStr, Decimal> = BTreeMap::new();
    let mut period_results: Vec<(String, BTreeMap<InternedStr, Decimal>)> = Vec::new();

    let format_period = |date: rustledger_core::NaiveDate, period: &str| -> String {
        match period {
            "daily" => date.to_string(),
            "weekly" => format!("{}-W{:02}", date.year(), date.iso_week().week()),
            "yearly" => format!("{}", date.year()),
            _ => format!("{}-{:02}", date.year(), date.month()),
        }
    };

    let mut current_period = String::new();

    for txn in transactions {
        let txn_period = format_period(txn.date, period);

        if txn_period != current_period && !current_period.is_empty() {
            let mut net_worth: BTreeMap<InternedStr, Decimal> = asset_balance.clone();
            for (currency, amount) in &liability_balance {
                *net_worth.entry(currency.clone()).or_default() += amount;
            }
            period_results.push((current_period.clone(), net_worth));
        }
        current_period = txn_period;

        for posting in &txn.postings {
            if let Some(amount) = posting.amount() {
                let account_str: &str = &posting.account;
                if account_str.starts_with("Assets:") {
                    *asset_balance.entry(amount.currency.clone()).or_default() += amount.number;
                } else if account_str.starts_with("Liabilities:") {
                    *liability_balance
                        .entry(amount.currency.clone())
                        .or_default() += amount.number;
                }
            }
        }
    }

    if !current_period.is_empty() {
        let mut net_worth: BTreeMap<InternedStr, Decimal> = asset_balance.clone();
        for (currency, amount) in &liability_balance {
            *net_worth.entry(currency.clone()).or_default() += amount;
        }
        period_results.push((current_period, net_worth));
    }

    match format {
        OutputFormat::Csv => {
            writeln!(writer, "period,currency,amount")?;
            for (period_label, net_worth) in &period_results {
                for (currency, amount) in net_worth {
                    writeln!(writer, "{period_label},{currency},{amount}")?;
                }
            }
        }
        OutputFormat::Json => {
            writeln!(writer, "[")?;
            let total_entries: usize = period_results.iter().map(|(_, nw)| nw.len()).sum();
            let mut entry_idx = 0;
            for (period_label, net_worth) in &period_results {
                for (currency, amount) in net_worth {
                    entry_idx += 1;
                    let comma = if entry_idx < total_entries { "," } else { "" };
                    writeln!(
                        writer,
                        r#"  {{"period": "{period_label}", "currency": "{currency}", "amount": "{amount}"}}{comma}"#
                    )?;
                }
            }
            writeln!(writer, "]")?;
        }
        OutputFormat::Text => {
            writeln!(writer, "Net Worth Over Time ({period})")?;
            writeln!(writer, "{}", "=".repeat(60))?;
            writeln!(writer)?;

            for (period_label, net_worth) in &period_results {
                write!(writer, "{period_label:12}")?;
                for (currency, amount) in net_worth {
                    write!(writer, "  {amount:>12} {currency}")?;
                }
                writeln!(writer)?;
            }
        }
    }

    Ok(())
}
