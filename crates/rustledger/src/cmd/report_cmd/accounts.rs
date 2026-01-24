//! Accounts report - List all accounts.

use super::{csv_escape, json_escape, OutputFormat};
use anyhow::Result;
use rustledger_core::Directive;
use std::collections::BTreeSet;
use std::io::Write;

/// Generate an accounts list.
pub(super) fn report_accounts<W: Write>(
    directives: &[Directive],
    format: &OutputFormat,
    writer: &mut W,
) -> Result<()> {
    let mut accounts: BTreeSet<&str> = BTreeSet::new();

    for directive in directives {
        if let Directive::Open(open) = directive {
            accounts.insert(&open.account);
        }
    }

    let accounts: Vec<_> = accounts.into_iter().collect();

    match format {
        OutputFormat::Csv => {
            writeln!(writer, "account")?;
            for account in &accounts {
                writeln!(writer, "{}", csv_escape(account))?;
            }
        }
        OutputFormat::Json => {
            writeln!(writer, "[")?;
            for (i, account) in accounts.iter().enumerate() {
                let comma = if i < accounts.len() - 1 { "," } else { "" };
                writeln!(writer, r#"  "{}"{}"#, json_escape(account), comma)?;
            }
            writeln!(writer, "]")?;
        }
        OutputFormat::Text => {
            writeln!(writer, "Accounts ({} total)", accounts.len())?;
            writeln!(writer, "{}", "=".repeat(40))?;
            writeln!(writer)?;
            for account in &accounts {
                writeln!(writer, "{account}")?;
            }
        }
    }

    Ok(())
}
