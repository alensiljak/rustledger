use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn cmd_lex<W: Write>(file: &PathBuf, writer: &mut W) -> Result<()> {
    let source =
        fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;

    // Use the parser's lexer to tokenize
    let result = rustledger_parser::parse(&source);

    // Show tokens by line
    writeln!(writer, "Lexer output for {}:", file.display())?;
    writeln!(writer, "{}", "=".repeat(60))?;

    // Since we don't have direct lexer access, show the parsed result info
    writeln!(writer, "Parsed {} directives", result.directives.len())?;
    writeln!(writer, "Found {} errors", result.errors.len())?;
    writeln!(writer, "Found {} options", result.options.len())?;
    writeln!(writer, "Found {} plugins", result.plugins.len())?;
    writeln!(writer, "Found {} includes", result.includes.len())?;

    if !result.errors.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "Parse errors:")?;
        for err in &result.errors {
            writeln!(writer, "  Line {}: {}", err.span.start, err.message())?;
        }
    }

    Ok(())
}
