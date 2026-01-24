//! Commodities report - List all commodities/currencies.

use super::OutputFormat;
use anyhow::Result;
use rustledger_core::Directive;
use std::collections::BTreeSet;
use std::io::Write;

/// Generate a commodities list.
pub(super) fn report_commodities<W: Write>(
    directives: &[Directive],
    format: &OutputFormat,
    writer: &mut W,
) -> Result<()> {
    let mut commodities: BTreeSet<&str> = BTreeSet::new();

    for directive in directives {
        match directive {
            Directive::Commodity(comm) => {
                commodities.insert(&comm.currency);
            }
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    if let Some(amount) = posting.amount() {
                        commodities.insert(&amount.currency);
                    }
                }
            }
            Directive::Balance(bal) => {
                commodities.insert(&bal.amount.currency);
            }
            Directive::Price(price) => {
                commodities.insert(&price.currency);
                commodities.insert(&price.amount.currency);
            }
            _ => {}
        }
    }

    let commodities: Vec<_> = commodities.into_iter().collect();

    match format {
        OutputFormat::Csv => {
            writeln!(writer, "commodity")?;
            for commodity in &commodities {
                writeln!(writer, "{commodity}")?;
            }
        }
        OutputFormat::Json => {
            writeln!(writer, "[")?;
            for (i, commodity) in commodities.iter().enumerate() {
                let comma = if i < commodities.len() - 1 { "," } else { "" };
                writeln!(writer, r#"  "{commodity}"{comma}"#)?;
            }
            writeln!(writer, "]")?;
        }
        OutputFormat::Text => {
            writeln!(writer, "Commodities ({} total)", commodities.len())?;
            writeln!(writer, "{}", "=".repeat(40))?;
            writeln!(writer)?;
            for commodity in &commodities {
                writeln!(writer, "{commodity}")?;
            }
        }
    }

    Ok(())
}
