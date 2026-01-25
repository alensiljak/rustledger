use anyhow::{Context, Result};
use rustledger_loader::Loader;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Convert a byte offset to a 1-based line number.
fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

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
        // Convert byte offsets to line numbers for comparison
        let span_start_line = byte_offset_to_line(&source, span.start);
        let span_end_line = byte_offset_to_line(&source, span.end);

        if span_start_line <= line && span_end_line >= line {
            writeln!(writer, "Directive at this location:")?;
            writeln!(writer, "{:?}", spanned.value)?;
            break;
        }
    }

    Ok(())
}
