//! Diagnostics handler for publishing parse and validation errors.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rustledger_booking::BookingEngine;
use rustledger_core::{BookingMethod, Directive};
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
///
/// This function runs booking/interpolation before validation to mirror the
/// ordering used by `rledger check`. Without booking, transactions with
/// auto-filled postings (e.g., a posting with no amount) would be incorrectly
/// flagged as unbalanced.
pub fn validation_errors_to_diagnostics(
    directives: &[Spanned<Directive>],
    source: &str,
) -> Vec<Diagnostic> {
    let line_index = LineIndex::new(source);

    // Clone and sort directives by date (required for correct lot matching during booking)
    let mut booked_directives: Vec<Spanned<Directive>> = directives.to_vec();
    booked_directives.sort_by(|a, b| {
        a.value
            .date()
            .cmp(&b.value.date())
            .then_with(|| a.value.priority().cmp(&b.value.priority()))
    });

    // Run booking/interpolation on transactions before validation.
    // This fills in missing amounts (auto-balancing) so validation sees the complete picture.
    // Use Strict booking method to match rledger check's default behavior.
    let mut booking_engine = BookingEngine::with_method(BookingMethod::Strict);
    for spanned in &mut booked_directives {
        if let Directive::Transaction(txn) = &mut spanned.value
            && let Ok(result) = booking_engine.book_and_interpolate(txn)
        {
            booking_engine.apply(&result.transaction);
            *txn = result.transaction;
        }
        // If booking fails, we leave the transaction as-is and let validation catch it
    }

    let validation_errors =
        validate_spanned_with_options(&booked_directives, ValidationOptions::default());

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
    let (start_line, start_col, end_line, end_col, has_location) = if let Some(span) = &error.span {
        let (sl, sc) = line_index.offset_to_position(span.start);
        let (el, ec) = line_index.offset_to_position(span.end);
        (sl, sc, el, ec, true)
    } else {
        // No span available - put at start of file and note in message
        (0, 0, 0, 0, false)
    };

    // Map severity to LSP severity
    let severity = match error.code.severity() {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    // Build message with context if available
    let mut message = if let Some(ctx) = &error.context {
        format!("{} ({})\n  context: {}", error.message, error.date, ctx)
    } else {
        format!("{} ({})", error.message, error.date)
    };

    // Add note if location is unknown
    if !has_location {
        message.push_str("\n  (source location unknown)");
    }

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

/// Maximum file size (in bytes) for which validation will be run.
/// For larger files, only parse errors are reported to keep the LSP responsive.
/// 500KB is a generous limit - most beancount files are much smaller.
const MAX_VALIDATION_FILE_SIZE: usize = 500 * 1024;

/// Get all diagnostics (parse errors + validation errors) for a parse result.
///
/// Validation is skipped for files larger than `MAX_VALIDATION_FILE_SIZE` to
/// avoid blocking the LSP main loop on very large files.
pub fn all_diagnostics(result: &ParseResult, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = parse_errors_to_diagnostics(result, source);

    // Only run validation if:
    // 1. There are no parse errors (validation on partial parses is confusing)
    // 2. File is not too large (to keep LSP responsive)
    if result.errors.is_empty() {
        if source.len() <= MAX_VALIDATION_FILE_SIZE {
            let validation_diagnostics =
                validation_errors_to_diagnostics(&result.directives, source);
            diagnostics.extend(validation_diagnostics);
        } else {
            tracing::debug!(
                "Skipping validation for large file ({} bytes > {} limit)",
                source.len(),
                MAX_VALIDATION_FILE_SIZE
            );
        }
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

        // Should have at least these validation errors:
        // - E1001: Account Income:Typo was never opened
        // - E3001: Transaction(s) do not balance
        // - E2001: Balance assertion failed
        // Note: We check for presence rather than exact count to avoid brittleness
        // if the validator adds new checks in the future.
        assert!(
            !diagnostics.is_empty(),
            "Should have at least one validation error"
        );

        // Helper to get code string from a diagnostic
        fn get_code(d: &Diagnostic) -> String {
            match d.code.as_ref().unwrap() {
                lsp_types::NumberOrString::String(s) => s.clone(),
                lsp_types::NumberOrString::Number(n) => panic!("Unexpected number code: {}", n),
            }
        }

        // Check expected error codes are present
        let codes: Vec<_> = diagnostics.iter().map(get_code).collect();

        assert!(
            codes.iter().any(|c| c == "E1001"),
            "Should have E1001 (account not opened)"
        );
        assert!(
            codes.iter().any(|c| c == "E3001"),
            "Should have E3001 (unbalanced transaction)"
        );
        assert!(
            codes.iter().any(|c| c == "E2001"),
            "Should have E2001 (balance assertion failed)"
        );

        // Check that severity matches the expected severity for each error code
        // (rather than asserting all are ERROR, which would break if warnings are added)
        for diag in &diagnostics {
            let code = get_code(diag);
            let expected_severity = match code.as_str() {
                "E1001" | "E2001" | "E3001" => Some(DiagnosticSeverity::ERROR),
                // Add other known codes here as needed
                _ => continue, // Don't assert on unknown codes
            };
            assert_eq!(
                diag.severity, expected_severity,
                "Diagnostic {} should have correct severity",
                code
            );
        }
    }

    #[test]
    fn test_auto_filled_postings_do_not_trigger_false_positive() {
        // Regression test for issue #475 follow-up comment:
        // A valid file with auto-filled postings should NOT have E3001 errors.
        // The second posting has no amount, which should be auto-filled to -5000 USD.
        let source = r#"2024-01-01 open Assets:Bank:Checking USD
2024-01-01 open Income:Salary

2024-01-15 * "Paycheck"
  Assets:Bank:Checking                    5000 USD
  Income:Salary

2024-01-16 balance Assets:Bank:Checking 5000 USD
"#;

        let result = parse(source);
        assert!(result.errors.is_empty(), "Should have no parse errors");

        let diagnostics = all_diagnostics(&result, source);

        // Helper to get code string from a diagnostic
        fn get_code(d: &Diagnostic) -> String {
            match d.code.as_ref().unwrap() {
                lsp_types::NumberOrString::String(s) => s.clone(),
                lsp_types::NumberOrString::Number(n) => panic!("Unexpected number code: {}", n),
            }
        }

        // Filter to only ERROR severity diagnostics (allow warnings/info)
        let error_diagnostics: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| matches!(d.severity, Some(DiagnosticSeverity::ERROR)))
            .collect();

        let error_codes: Vec<_> = error_diagnostics.iter().map(|d| get_code(d)).collect();

        // Specifically, there should be NO E3001 (unbalanced transaction) error
        // because the booking step should auto-fill the missing amount
        assert!(
            !error_codes.iter().any(|c| c == "E3001"),
            "Should NOT have E3001 - the transaction is balanced after booking fills in the missing amount. Got codes: {:?}",
            error_codes
        );

        // The file should have no ERROR-severity diagnostics (but may have warnings/info)
        assert!(
            error_diagnostics.is_empty(),
            "Valid file should have no ERROR diagnostics, but got: {:?}",
            error_codes
        );
    }
}
