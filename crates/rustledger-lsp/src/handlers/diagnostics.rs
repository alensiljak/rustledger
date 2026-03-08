//! Diagnostics handler for publishing parse and validation errors.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rustledger_core::Directive;
use rustledger_parser::{ParseError, ParseResult, Spanned};
use rustledger_validate::{
    Severity, ValidationError, ValidationOptions, validate_spanned_with_options,
};

use super::utils::LineIndex;

/// Convert parse errors to LSP diagnostics.
pub fn parse_errors_to_diagnostics(result: &ParseResult, source: &str) -> Vec<Diagnostic> {
    let line_index = LineIndex::new(source);
    result
        .errors
        .iter()
        .map(|e| parse_error_to_diagnostic(e, &line_index))
        .collect()
}

/// Convert a single parse error to an LSP diagnostic.
pub fn parse_error_to_diagnostic(error: &ParseError, line_index: &LineIndex) -> Diagnostic {
    let (start_line, start_col) = line_index.offset_to_position(error.span.start);
    let (end_line, end_col) = line_index.offset_to_position(error.span.end);

    Diagnostic {
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::String(format!(
            "P{:04}",
            error.kind_code()
        ))),
        source: Some("rustledger".to_string()),
        message: error.message(),
        related_information: None,
        tags: None,
        code_description: None,
        data: None,
    }
}

/// Run validation on parsed directives and convert errors to LSP diagnostics.
pub fn validation_errors_to_diagnostics(
    directives: &[Spanned<Directive>],
    source: &str,
) -> Vec<Diagnostic> {
    let line_index = LineIndex::new(source);
    let validation_errors = validate_spanned_with_options(directives, ValidationOptions::default());

    validation_errors
        .iter()
        .map(|e| validation_error_to_diagnostic(e, &line_index))
        .collect()
}

/// Convert a single validation error to an LSP diagnostic.
pub fn validation_error_to_diagnostic(
    error: &ValidationError,
    line_index: &LineIndex,
) -> Diagnostic {
    // Get position from span if available, otherwise use start of file
    let (start_line, start_col, end_line, end_col) = if let Some(span) = &error.span {
        let (sl, sc) = line_index.offset_to_position(span.start);
        let (el, ec) = line_index.offset_to_position(span.end);
        (sl, sc, el, ec)
    } else {
        // No span available - put at start of file
        (0, 0, 0, 0)
    };

    // Map severity to LSP severity
    let severity = match error.code.severity() {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    // Build message with context if available
    let message = if let Some(ctx) = &error.context {
        format!("{} ({})\n  context: {}", error.message, error.date, ctx)
    } else {
        format!("{} ({})", error.message, error.date)
    };

    Diagnostic {
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
        },
        severity: Some(severity),
        code: Some(lsp_types::NumberOrString::String(
            error.code.code().to_string(),
        )),
        source: Some("rustledger".to_string()),
        message,
        related_information: None,
        tags: None,
        code_description: None,
        data: None,
    }
}

/// Get all diagnostics (parse errors + validation errors) for a parse result.
pub fn all_diagnostics(result: &ParseResult, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = parse_errors_to_diagnostics(result, source);

    // Only run validation if there are no parse errors
    // (validation on partially-parsed files may produce confusing results)
    if result.errors.is_empty() {
        let validation_diagnostics = validation_errors_to_diagnostics(&result.directives, source);
        diagnostics.extend(validation_diagnostics);
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_line_index_offset_to_position() {
        let source = "line1\nline2\nline3";
        let line_index = LineIndex::new(source);

        assert_eq!(line_index.offset_to_position(0), (0, 0));
        assert_eq!(line_index.offset_to_position(5), (0, 5));
        assert_eq!(line_index.offset_to_position(6), (1, 0));
        assert_eq!(line_index.offset_to_position(12), (2, 0));
    }

    #[test]
    fn test_validation_errors_shown_as_diagnostics() {
        // Minimal test case from issue #475
        let source = r#"2024-01-01 open Assets:Bank:Checking USD
2024-01-01 open Income:Salary

2024-01-15 * "Paycheck"
  Assets:Bank:Checking                    5000 USD
  Income:Typo

2024-01-15 * "Paycheck"
  Assets:Bank:Checking                    5000 USD
  Income:Salary                          -3000 USD

2024-01-16 balance Assets:Bank:Checking 2000 USD
"#;

        let result = parse(source);
        assert!(result.errors.is_empty(), "Should have no parse errors");

        let diagnostics = all_diagnostics(&result, source);

        // Should have 4 validation errors:
        // 1. E1001: Account Income:Typo was never opened
        // 2. E3001: First transaction residual 5000 USD (only one posting has amount)
        // 3. E3001: Second transaction residual 2000 USD (5000 - 3000)
        // 4. E2001: Balance assertion failed
        assert_eq!(diagnostics.len(), 4, "Expected 4 validation errors");

        // Check error codes
        let codes: Vec<_> = diagnostics
            .iter()
            .filter_map(|d| d.code.as_ref())
            .map(|c| match c {
                lsp_types::NumberOrString::String(s) => s.as_str(),
                lsp_types::NumberOrString::Number(n) => panic!("Unexpected number code: {}", n),
            })
            .collect();

        assert!(
            codes.contains(&"E1001"),
            "Should have E1001 (account not opened)"
        );
        assert!(
            codes.contains(&"E3001"),
            "Should have E3001 (unbalanced transaction)"
        );
        assert!(
            codes.contains(&"E2001"),
            "Should have E2001 (balance assertion failed)"
        );

        // Check severities - all should be ERROR
        for diag in &diagnostics {
            assert_eq!(
                diag.severity,
                Some(DiagnosticSeverity::ERROR),
                "All validation errors should have ERROR severity"
            );
        }
    }
}
