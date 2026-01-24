use anyhow::{Context, Result};
use rustledger_loader::Loader;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn cmd_context<W: Write>(file: &PathBuf, line: usize, writer: &mut W) -> Result<()> {
    let mut loader = Loader::new();
    let load_result = loader
        .load(file)
        .with_context(|| format!("failed to load {}", file.display()))?;

    // Find the directive at or near the specified line
    let source = fs::read_to_string(file)?;
    let lines: Vec<&str> = source.lines().collect();

    writeln!(writer, "Context at {}:{}", file.display(), line)?;
    writeln!(writer, "{}", "=".repeat(60))?;
    writeln!(writer)?;

    // Show surrounding lines
    let start = line.saturating_sub(3);
    let end = (line + 3).min(lines.len());

    for (i, src_line) in lines.iter().enumerate().skip(start).take(end - start) {
        let line_num = i + 1;
        let marker = if line_num == line { ">>>" } else { "   " };
        writeln!(writer, "{marker} {line_num:4} | {src_line}")?;
    }

    // Find which directive contains this line
    writeln!(writer)?;
    for spanned in &load_result.directives {
        let span = &spanned.span;
        // Check if line falls within directive's span (approximate)
        if span.start <= line && span.end >= line {
            writeln!(writer, "Directive at this location:")?;
            writeln!(writer, "{:?}", spanned.value)?;
            break;
        }
    }

    Ok(())
}
