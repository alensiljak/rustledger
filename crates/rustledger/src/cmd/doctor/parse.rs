use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn cmd_parse<W: Write>(file: &PathBuf, verbose: bool, writer: &mut W) -> Result<()> {
    let source =
        fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;

    let result = rustledger_parser::parse(&source);

    writeln!(writer, "Parse result for {}:", file.display())?;
    writeln!(writer, "{}", "=".repeat(60))?;
    writeln!(writer)?;

    if verbose {
        for (i, spanned) in result.directives.iter().enumerate() {
            writeln!(writer, "[{}] {:?}", i, spanned.value)?;
        }
    } else {
        writeln!(writer, "Directives: {}", result.directives.len())?;
        writeln!(writer, "Errors: {}", result.errors.len())?;
        writeln!(writer, "Options: {}", result.options.len())?;
        writeln!(writer, "Plugins: {}", result.plugins.len())?;
        writeln!(writer, "Includes: {}", result.includes.len())?;
    }

    if !result.errors.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Errors:")?;
        for err in &result.errors {
            writeln!(writer, "  {}", err.message())?;
        }
    }

    Ok(())
}
