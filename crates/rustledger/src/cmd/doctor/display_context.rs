use anyhow::{Context, Result};
use rustledger_core::{Directive, InternedStr};
use rustledger_loader::Loader;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn cmd_display_context<W: Write>(file: &PathBuf, writer: &mut W) -> Result<()> {
    let mut loader = Loader::new();
    let load_result = loader
        .load(file)
        .with_context(|| format!("failed to load {}", file.display()))?;

    // Collect decimal precision from numbers in the file
    let mut currency_scales: BTreeMap<InternedStr, i32> = BTreeMap::new();

    for spanned in &load_result.directives {
        match &spanned.value {
            Directive::Transaction(txn) => {
                for posting in &txn.postings {
                    if let Some(amount) = posting.amount() {
                        let scale = amount.number.scale() as i32;
                        let entry = currency_scales.entry(amount.currency.clone()).or_insert(0);
                        if scale > *entry {
                            *entry = scale;
                        }
                    }
                }
            }
            Directive::Balance(bal) => {
                let scale = bal.amount.number.scale() as i32;
                let entry = currency_scales
                    .entry(bal.amount.currency.clone())
                    .or_insert(0);
                if scale > *entry {
                    *entry = scale;
                }
            }
            Directive::Price(price) => {
                let scale = price.amount.number.scale() as i32;
                let entry = currency_scales
                    .entry(price.amount.currency.clone())
                    .or_insert(0);
                if scale > *entry {
                    *entry = scale;
                }
            }
            _ => {}
        }
    }

    writeln!(writer, "Display Context (decimal precision by currency)")?;
    writeln!(writer, "{}", "=".repeat(60))?;
    writeln!(writer)?;

    if currency_scales.is_empty() {
        writeln!(writer, "No currencies found in file.")?;
    } else {
        for (currency, scale) in &currency_scales {
            writeln!(writer, "{currency}: {scale} decimal places")?;
        }
    }

    Ok(())
}
