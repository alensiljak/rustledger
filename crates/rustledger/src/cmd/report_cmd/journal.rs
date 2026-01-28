//! Journal report - Transaction journal/register.

use super::{OutputFormat, csv_escape, json_escape};
use anyhow::Result;
use rustledger_core::Directive;
use std::io::Write;

/// Generate a journal/register report.
pub(super) fn report_journal<W: Write>(
    directives: &[Directive],
    account_filter: Option<&str>,
    limit: Option<usize>,
    format: &OutputFormat,
    writer: &mut W,
) -> Result<()> {
    let mut entries: Vec<_> = directives
        .iter()
        .filter_map(|d| {
            if let Directive::Transaction(txn) = d {
                if let Some(filter) = account_filter
                    && !txn.postings.iter().any(|p| p.account.starts_with(filter))
                {
                    return None;
                }
                Some(txn)
            } else {
                None
            }
        })
        .collect();

    entries.sort_by_key(|t| t.date);

    let entries_to_show = if let Some(n) = limit {
        entries.into_iter().rev().take(n).collect::<Vec<_>>()
    } else {
        entries
    };

    match format {
        OutputFormat::Csv => {
            writeln!(writer, "date,flag,payee,narration,account,amount,currency")?;
            for txn in &entries_to_show {
                let payee = txn.payee.as_deref().unwrap_or("");
                for posting in &txn.postings {
                    let (amount, currency) = if let Some(amt) = posting.amount() {
                        (amt.number.to_string(), amt.currency.to_string())
                    } else {
                        (String::new(), String::new())
                    };
                    writeln!(
                        writer,
                        "{},{},{},{},{},{},{}",
                        txn.date,
                        txn.flag,
                        csv_escape(payee),
                        csv_escape(&txn.narration),
                        csv_escape(&posting.account),
                        amount,
                        currency
                    )?;
                }
            }
        }
        OutputFormat::Json => {
            writeln!(writer, "[")?;
            for (i, txn) in entries_to_show.iter().enumerate() {
                let payee = txn.payee.as_deref().unwrap_or("");
                let comma = if i < entries_to_show.len() - 1 {
                    ","
                } else {
                    ""
                };
                writeln!(writer, "  {{")?;
                writeln!(writer, r#"    "date": "{}","#, txn.date)?;
                writeln!(writer, r#"    "flag": "{}","#, txn.flag)?;
                writeln!(writer, r#"    "payee": "{}","#, json_escape(payee))?;
                writeln!(
                    writer,
                    r#"    "narration": "{}","#,
                    json_escape(&txn.narration)
                )?;
                writeln!(writer, r#"    "postings": ["#)?;
                for (j, posting) in txn.postings.iter().enumerate() {
                    let pcomma = if j < txn.postings.len() - 1 { "," } else { "" };
                    let (amount, currency) = if let Some(amt) = posting.amount() {
                        (amt.number.to_string(), amt.currency.to_string())
                    } else {
                        (String::new(), String::new())
                    };
                    writeln!(
                        writer,
                        r#"      {{"account": "{}", "amount": "{}", "currency": "{}"}}{}"#,
                        json_escape(&posting.account),
                        amount,
                        currency,
                        pcomma
                    )?;
                }
                writeln!(writer, "    ]")?;
                writeln!(writer, "  }}{comma}")?;
            }
            writeln!(writer, "]")?;
        }
        OutputFormat::Text => {
            writeln!(writer, "Transaction Journal")?;
            writeln!(writer, "{}", "=".repeat(80))?;
            writeln!(writer)?;

            for txn in &entries_to_show {
                let payee = txn.payee.as_ref().map_or("", |p| p.as_str());
                let narration = txn.narration.as_str();
                let desc = if payee.is_empty() {
                    narration.to_string()
                } else {
                    format!("{payee} | {narration}")
                };
                writeln!(writer, "{} {} {}", txn.date, txn.flag, desc)?;

                for posting in &txn.postings {
                    if let Some(amount) = posting.amount() {
                        writeln!(
                            writer,
                            "  {:50} {:>12} {}",
                            posting.account.as_str(),
                            amount.number,
                            amount.currency
                        )?;
                    } else {
                        writeln!(writer, "  {:50}", posting.account.as_str())?;
                    }
                }
                writeln!(writer)?;
            }
        }
    }

    Ok(())
}
