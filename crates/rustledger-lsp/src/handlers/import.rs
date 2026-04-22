//! Import review integration for the LSP.
//!
//! Scans transactions for `import-*` metadata (produced by the enriched
//! import pipeline) and provides:
//!
//! - **Diagnostics**: hints/warnings for imported transactions based on confidence
//! - **Code actions**: accept categorization, change account, batch accept
//! - **Code lens**: summary counts of imported/pending/duplicate transactions

use lsp_types::{
    CodeAction, CodeActionKind, CodeLens, Command, Diagnostic, DiagnosticSeverity, Position, Range,
};
use rustledger_core::{Directive, MetaValue};
use rustledger_parser::Spanned;

use super::utils::LineIndex;

/// Convert a byte offset to an LSP Position using a LineIndex.
fn to_position(line_index: &LineIndex, offset: usize) -> Position {
    let (line, col) = line_index.offset_to_position(offset);
    Position {
        line,
        character: col,
    }
}

/// Metadata key for import confidence (set by enriched import pipeline).
const META_CONFIDENCE: &str = "import-confidence";
/// Metadata key for import method (rule, merchant-dict, ml, llm, default).
const META_METHOD: &str = "import-method";

/// Generate diagnostics for imported transactions based on their confidence.
///
/// - confidence > 0.9 → Hint ("Imported via rule: Expenses:Groceries")
/// - confidence 0.5–0.9 → Information ("Review: Expenses:Dining (72%)")
/// - confidence < 0.5 → Warning ("Low confidence: Expenses:Unknown")
pub fn import_diagnostics(directives: &[Spanned<Directive>], source: &str) -> Vec<Diagnostic> {
    let line_index = LineIndex::new(source);
    let mut diagnostics = Vec::new();

    for spanned in directives {
        let Directive::Transaction(txn) = &spanned.value else {
            continue;
        };

        let Some(confidence) = txn.meta.get(META_CONFIDENCE).and_then(meta_to_f64) else {
            continue;
        };

        let method = txn
            .meta
            .get(META_METHOD)
            .and_then(meta_to_str)
            .unwrap_or("unknown");

        // Find the contra-account (the categorized posting, not the bank account)
        let contra_account = txn
            .postings
            .get(1)
            .map(|p| p.account.as_str())
            .unwrap_or("unknown");

        let start = to_position(&line_index, spanned.span.start);
        let end = to_position(&line_index, spanned.span.end.min(source.len()));
        let range = Range { start, end };

        let (severity, message) = if confidence > 0.9 {
            (
                DiagnosticSeverity::HINT,
                format!("Imported ({method}): {contra_account}"),
            )
        } else if confidence >= 0.5 {
            (
                DiagnosticSeverity::INFORMATION,
                format!(
                    "Review ({method}, {:.0}%): {contra_account}",
                    confidence * 100.0
                ),
            )
        } else {
            (
                DiagnosticSeverity::WARNING,
                format!(
                    "Low confidence ({:.0}%): {contra_account} — consider recategorizing",
                    confidence * 100.0
                ),
            )
        };

        diagnostics.push(Diagnostic {
            range,
            severity: Some(severity),
            source: Some("rustledger-import".to_string()),
            message,
            ..Default::default()
        });
    }

    diagnostics
}

/// Generate code actions for imported transactions in the given range.
///
/// Offers:
/// - "Accept categorization" for individual transactions
/// - "Accept all high-confidence imports" for batch operations
pub fn import_code_actions(
    directives: &[Spanned<Directive>],
    source: &str,
    range: Range,
) -> Vec<CodeAction> {
    let line_index = LineIndex::new(source);
    let mut actions = Vec::new();
    let mut high_confidence_count = 0;

    for spanned in directives {
        let Directive::Transaction(txn) = &spanned.value else {
            continue;
        };

        let Some(confidence) = txn.meta.get(META_CONFIDENCE).and_then(meta_to_f64) else {
            continue;
        };

        if confidence > 0.9 {
            high_confidence_count += 1;
        }

        let start = to_position(&line_index, spanned.span.start);
        let end = to_position(&line_index, spanned.span.end.min(source.len()));
        let txn_range = Range { start, end };

        // Only offer actions for transactions that overlap the selection
        if !ranges_overlap(range, txn_range) {
            continue;
        }

        let contra_account = txn
            .postings
            .get(1)
            .map(|p| p.account.to_string())
            .unwrap_or_default();

        actions.push(CodeAction {
            title: format!("Accept import: {contra_account}"),
            kind: Some(CodeActionKind::QUICKFIX),
            command: Some(Command {
                title: "Accept import".to_string(),
                command: "rledger.importAccept".to_string(),
                arguments: Some(vec![serde_json::json!({
                    "line": start.line,
                })]),
            }),
            ..Default::default()
        });
    }

    // Batch accept action
    if high_confidence_count > 1 {
        actions.push(CodeAction {
            title: format!("Accept all {high_confidence_count} high-confidence imports"),
            kind: Some(CodeActionKind::QUICKFIX),
            command: Some(Command {
                title: "Accept all high-confidence".to_string(),
                command: "rledger.importAcceptAll".to_string(),
                arguments: None,
            }),
            ..Default::default()
        });
    }

    actions
}

/// Generate code lens for import summary.
///
/// Shows a summary line above the first imported transaction:
/// "N imported | M need review | K duplicates"
pub fn import_code_lens(directives: &[Spanned<Directive>], source: &str) -> Vec<CodeLens> {
    let line_index = LineIndex::new(source);
    let mut total = 0u32;
    let mut needs_review = 0u32;
    let mut first_import_line: Option<Position> = None;

    for spanned in directives {
        let Directive::Transaction(txn) = &spanned.value else {
            continue;
        };

        let Some(confidence) = txn.meta.get(META_CONFIDENCE).and_then(meta_to_f64) else {
            continue;
        };

        total += 1;
        if confidence < 0.9 {
            needs_review += 1;
        }

        if first_import_line.is_none() {
            first_import_line = Some(to_position(&line_index, spanned.span.start));
        }
    }

    if total == 0 {
        return vec![];
    }

    let Some(pos) = first_import_line else {
        return vec![];
    };

    let title = if needs_review > 0 {
        format!("{total} imported | {needs_review} need review")
    } else {
        format!("{total} imported | all accepted")
    };

    vec![CodeLens {
        range: Range {
            start: Position {
                line: pos.line,
                character: 0,
            },
            end: Position {
                line: pos.line,
                character: 0,
            },
        },
        command: Some(Command {
            title,
            command: String::new(),
            arguments: None,
        }),
        data: None,
    }]
}

/// Check if two ranges overlap.
fn ranges_overlap(a: Range, b: Range) -> bool {
    a.start.line <= b.end.line && b.start.line <= a.end.line
}

/// Extract f64 from MetaValue.
fn meta_to_f64(v: &MetaValue) -> Option<f64> {
    match v {
        MetaValue::Number(d) => {
            use rust_decimal::prelude::ToPrimitive;
            d.to_f64()
        }
        _ => None,
    }
}

/// Extract &str from MetaValue.
fn meta_to_str(v: &MetaValue) -> Option<&str> {
    match v {
        MetaValue::String(s) => Some(s.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rustledger_core::{Metadata, Posting, Transaction, naive_date};
    use rustledger_parser::Span;
    use std::str::FromStr;

    fn make_imported_txn(confidence: f64, method: &str, account: &str) -> Spanned<Directive> {
        let date = naive_date(2024, 1, 15).unwrap();
        let mut meta = Metadata::default();
        meta.insert(
            META_CONFIDENCE.to_string(),
            MetaValue::Number(Decimal::from_str(&confidence.to_string()).unwrap()),
        );
        meta.insert(
            META_METHOD.to_string(),
            MetaValue::String(method.to_string()),
        );

        let txn = Transaction {
            date,
            flag: '*',
            payee: None,
            narration: "Test".into(),
            tags: vec![],
            links: vec![],
            meta,
            postings: vec![
                Posting::new(
                    "Assets:Bank",
                    rustledger_core::Amount::new(Decimal::from_str("-50").unwrap(), "USD"),
                ),
                Posting::new(
                    account,
                    rustledger_core::Amount::new(Decimal::from_str("50").unwrap(), "USD"),
                ),
            ],
            trailing_comments: vec![],
        };

        Spanned {
            value: Directive::Transaction(txn),
            span: Span { start: 0, end: 100 },
            file_id: 0,
        }
    }

    fn make_source() -> String {
        // Source text long enough for the spans
        " ".repeat(200)
    }

    #[test]
    fn diagnostics_high_confidence_is_hint() {
        let source = make_source();
        let directives = vec![make_imported_txn(0.95, "rule", "Expenses:Groceries")];
        let diags = import_diagnostics(&directives, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::HINT));
        assert!(diags[0].message.contains("Expenses:Groceries"));
    }

    #[test]
    fn diagnostics_medium_confidence_is_info() {
        let source = make_source();
        let directives = vec![make_imported_txn(0.72, "ml", "Expenses:Dining")];
        let diags = import_diagnostics(&directives, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::INFORMATION));
        assert!(diags[0].message.contains("72%"));
    }

    #[test]
    fn diagnostics_low_confidence_is_warning() {
        let source = make_source();
        let directives = vec![make_imported_txn(0.3, "default", "Expenses:Unknown")];
        let diags = import_diagnostics(&directives, &source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(diags[0].message.contains("recategorizing"));
    }

    #[test]
    fn diagnostics_skips_non_imported() {
        let source = make_source();
        let date = naive_date(2024, 1, 15).unwrap();
        let txn = Transaction::new(date, "Not imported");
        let directives = vec![Spanned {
            value: Directive::Transaction(txn),
            span: Span { start: 0, end: 50 },
            file_id: 0,
        }];
        let diags = import_diagnostics(&directives, &source);
        assert!(diags.is_empty());
    }

    #[test]
    fn code_lens_shows_summary() {
        let source = make_source();
        let directives = vec![
            make_imported_txn(0.95, "rule", "Expenses:Groceries"),
            make_imported_txn(0.6, "ml", "Expenses:Dining"),
            make_imported_txn(0.3, "default", "Expenses:Unknown"),
        ];
        let lenses = import_code_lens(&directives, &source);
        assert_eq!(lenses.len(), 1);
        let title = &lenses[0].command.as_ref().unwrap().title;
        assert!(title.contains("3 imported"));
        assert!(title.contains("2 need review"));
    }

    #[test]
    fn code_lens_all_accepted() {
        let source = make_source();
        let directives = vec![
            make_imported_txn(0.95, "rule", "Expenses:Groceries"),
            make_imported_txn(0.99, "rule", "Expenses:Dining"),
        ];
        let lenses = import_code_lens(&directives, &source);
        assert_eq!(lenses.len(), 1);
        let title = &lenses[0].command.as_ref().unwrap().title;
        assert!(title.contains("all accepted"));
    }

    #[test]
    fn code_lens_empty_for_no_imports() {
        let source = make_source();
        let directives = vec![];
        let lenses = import_code_lens(&directives, &source);
        assert!(lenses.is_empty());
    }
}
