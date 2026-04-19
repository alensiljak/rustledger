//! Document symbols support for the editor.

use rustledger_core::Directive;
use rustledger_parser::ParseResult;

use crate::types::{EditorDocumentSymbol, EditorRange, SymbolKind};

use super::line_index::{EditorCache, LineIndex};

/// Get all document symbols (for outline view) - cached version using `LineIndex`.
pub fn get_document_symbols_cached(
    parse_result: &ParseResult,
    cache: &EditorCache,
) -> Vec<EditorDocumentSymbol> {
    parse_result
        .directives
        .iter()
        .filter_map(|spanned| {
            directive_to_symbol_cached(
                &spanned.value,
                spanned.span.start,
                spanned.span.end,
                &cache.line_index,
            )
        })
        .collect()
}

/// Get all document symbols (for outline view, non-cached, used by tests).
pub fn get_document_symbols(source: &str, parse_result: &ParseResult) -> Vec<EditorDocumentSymbol> {
    let line_index = LineIndex::new(source);
    parse_result
        .directives
        .iter()
        .filter_map(|spanned| {
            directive_to_symbol_cached(
                &spanned.value,
                spanned.span.start,
                spanned.span.end,
                &line_index,
            )
        })
        .collect()
}

/// Convert a directive to a document symbol - cached version using `LineIndex`.
fn directive_to_symbol_cached(
    directive: &Directive,
    start_offset: usize,
    end_offset: usize,
    line_index: &LineIndex,
) -> Option<EditorDocumentSymbol> {
    let (start_line, start_col) = line_index.offset_to_position(start_offset);
    let (end_line, end_col) = line_index.offset_to_position(end_offset);

    let range = EditorRange {
        start_line,
        start_character: start_col,
        end_line,
        end_character: end_col,
    };

    match directive {
        Directive::Transaction(txn) => {
            let date = txn.date;
            let name = if let Some(ref payee) = txn.payee {
                format!("{date} {payee}")
            } else if !txn.narration.is_empty() {
                let narration = &txn.narration;
                format!("{date} {narration}")
            } else {
                format!("{date} Transaction")
            };

            let detail = if txn.narration.is_empty() {
                None
            } else {
                Some(txn.narration.to_string())
            };

            let children: Vec<EditorDocumentSymbol> = txn
                .postings
                .iter()
                .enumerate()
                .map(|(i, posting)| {
                    let posting_name = posting.account.to_string();
                    let posting_detail = posting.units.as_ref().map(|u| {
                        if let (Some(num), Some(curr)) = (u.number(), u.currency()) {
                            format!("{num} {curr}")
                        } else if let Some(num) = u.number() {
                            num.to_string()
                        } else {
                            String::new()
                        }
                    });

                    let posting_line = start_line + 1 + i as u32;
                    let posting_range = EditorRange {
                        start_line: posting_line,
                        start_character: 2,
                        end_line: posting_line,
                        end_character: 50,
                    };

                    EditorDocumentSymbol {
                        name: posting_name,
                        detail: posting_detail,
                        kind: SymbolKind::Posting,
                        range: posting_range,
                        children: None,
                        deprecated: None,
                    }
                })
                .collect();

            Some(EditorDocumentSymbol {
                name,
                detail,
                kind: SymbolKind::Transaction,
                range,
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
                deprecated: None,
            })
        }

        Directive::Open(open) => {
            let account = &open.account;
            Some(EditorDocumentSymbol {
                name: format!("open {account}"),
                detail: if open.currencies.is_empty() {
                    None
                } else {
                    Some(
                        open.currencies
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                },
                kind: SymbolKind::Account,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Close(close) => {
            let account = &close.account;
            Some(EditorDocumentSymbol {
                name: format!("close {account}"),
                detail: None,
                kind: SymbolKind::Account,
                range,
                children: None,
                deprecated: Some(true),
            })
        }

        Directive::Balance(bal) => {
            let account = &bal.account;
            let number = &bal.amount.number;
            let currency = &bal.amount.currency;
            Some(EditorDocumentSymbol {
                name: format!("balance {account}"),
                detail: Some(format!("{number} {currency}")),
                kind: SymbolKind::Balance,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Pad(pad) => {
            let account = &pad.account;
            let source_account = &pad.source_account;
            Some(EditorDocumentSymbol {
                name: format!("pad {account}"),
                detail: Some(format!("from {source_account}")),
                kind: SymbolKind::Pad,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Commodity(comm) => {
            let currency = &comm.currency;
            Some(EditorDocumentSymbol {
                name: format!("commodity {currency}"),
                detail: None,
                kind: SymbolKind::Commodity,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Event(event) => {
            let event_type = &event.event_type;
            Some(EditorDocumentSymbol {
                name: format!("event \"{event_type}\""),
                detail: Some(event.value.clone()),
                kind: SymbolKind::Event,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Note(note) => {
            let account = &note.account;
            Some(EditorDocumentSymbol {
                name: format!("note {account}"),
                detail: Some(note.comment.clone()),
                kind: SymbolKind::Note,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Document(doc) => {
            let account = &doc.account;
            Some(EditorDocumentSymbol {
                name: format!("document {account}"),
                detail: Some(doc.path.clone()),
                kind: SymbolKind::Document,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Price(price) => {
            let currency = &price.currency;
            let number = &price.amount.number;
            let amount_currency = &price.amount.currency;
            Some(EditorDocumentSymbol {
                name: format!("price {currency}"),
                detail: Some(format!("{number} {amount_currency}")),
                kind: SymbolKind::Price,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Query(query) => {
            let name = &query.name;
            Some(EditorDocumentSymbol {
                name: format!("query \"{name}\""),
                detail: None,
                kind: SymbolKind::Query,
                range,
                children: None,
                deprecated: None,
            })
        }

        Directive::Custom(custom) => {
            let custom_type = &custom.custom_type;
            Some(EditorDocumentSymbol {
                name: format!("custom \"{custom_type}\""),
                detail: None,
                kind: SymbolKind::Custom,
                range,
                children: None,
                deprecated: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_get_document_symbols() {
        let source = r#"2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee Shop" "Morning coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let symbols = get_document_symbols(source, &result);

        assert_eq!(symbols.len(), 2); // open + transaction

        // First is the open directive
        assert!(symbols[0].name.contains("open"));
        assert_eq!(symbols[0].kind, SymbolKind::Account);

        // Second is the transaction with children (postings)
        assert!(symbols[1].name.contains("Coffee"));
        assert_eq!(symbols[1].kind, SymbolKind::Transaction);
        assert!(symbols[1].children.is_some());
        assert_eq!(symbols[1].children.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_document_symbols_all_directive_types() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-01 close Assets:OldBank
2024-01-01 commodity BTC
2024-01-01 balance Assets:Bank 100.00 USD
2024-01-01 pad Assets:Bank Equity:Opening
2024-01-01 event "location" "New York"
2024-01-01 note Assets:Bank "Test note"
2024-01-01 document Assets:Bank "/path/to/doc.pdf"
2024-01-01 price BTC 50000.00 USD
2024-01-01 query "test_query" "SELECT *"
2024-01-01 custom "budget" Expenses 500.00 USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let symbols = get_document_symbols(source, &result);

        // Should have symbols for all directive types
        assert!(symbols.len() >= 12);

        // Verify different symbol kinds
        let kinds: Vec<_> = symbols.iter().map(|s| &s.kind).collect();
        assert!(kinds.contains(&&SymbolKind::Account)); // open/close
        assert!(kinds.contains(&&SymbolKind::Commodity));
        assert!(kinds.contains(&&SymbolKind::Balance));
        assert!(kinds.contains(&&SymbolKind::Pad));
        assert!(kinds.contains(&&SymbolKind::Event));
        assert!(kinds.contains(&&SymbolKind::Note));
        assert!(kinds.contains(&&SymbolKind::Document));
        assert!(kinds.contains(&&SymbolKind::Price));
        assert!(kinds.contains(&&SymbolKind::Query));
        assert!(kinds.contains(&&SymbolKind::Custom));
        assert!(kinds.contains(&&SymbolKind::Transaction));
    }
}
