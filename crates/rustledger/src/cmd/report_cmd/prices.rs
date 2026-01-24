//! Prices report - Show price history.

use super::OutputFormat;
use anyhow::Result;
use rustledger_core::Directive;
use std::io::Write;

/// Generate a prices report.
pub(super) fn report_prices<W: Write>(
    directives: &[Directive],
    commodity_filter: Option<&str>,
    format: &OutputFormat,
    writer: &mut W,
) -> Result<()> {
    let mut prices: Vec<_> = directives
        .iter()
        .filter_map(|d| {
            if let Directive::Price(p) = d {
                if let Some(filter) = commodity_filter {
                    if p.currency != filter {
                        return None;
                    }
                }
                Some(p)
            } else {
                None
            }
        })
        .collect();

    prices.sort_by_key(|p| (p.currency.clone(), p.date));

    match format {
        OutputFormat::Csv => {
            writeln!(writer, "commodity,date,price,currency")?;
            for price in &prices {
                writeln!(
                    writer,
                    "{},{},{},{}",
                    price.currency, price.date, price.amount.number, price.amount.currency
                )?;
            }
        }
        OutputFormat::Json => {
            writeln!(writer, "[")?;
            for (i, price) in prices.iter().enumerate() {
                let comma = if i < prices.len() - 1 { "," } else { "" };
                writeln!(
                    writer,
                    r#"  {{"commodity": "{}", "date": "{}", "price": "{}", "currency": "{}"}}{}"#,
                    price.currency, price.date, price.amount.number, price.amount.currency, comma
                )?;
            }
            writeln!(writer, "]")?;
        }
        OutputFormat::Text => {
            writeln!(writer, "Price History")?;
            writeln!(writer, "{}", "=".repeat(60))?;
            writeln!(writer)?;
            if prices.is_empty() {
                writeln!(writer, "No price entries found.")?;
            } else {
                let mut current_currency = "";
                for price in &prices {
                    if price.currency.as_str() != current_currency {
                        if !current_currency.is_empty() {
                            writeln!(writer)?;
                        }
                        writeln!(writer, "{}:", price.currency)?;
                        current_currency = &price.currency;
                    }
                    writeln!(
                        writer,
                        "  {}  {} {}",
                        price.date, price.amount.number, price.amount.currency
                    )?;
                }
            }
        }
    }

    Ok(())
}
