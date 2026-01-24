//! Internal helper functions for WASM bindings.

use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use rustledger_booking::interpolate;
use rustledger_core::Directive;
use rustledger_parser::{ParseResult as ParserResult, parse as parse_beancount};
use rustledger_validate::validate as validate_ledger;

use crate::types::{Error, LedgerOptions, Severity};
use crate::utils::LineLookup;

/// Result of loading and interpolating a source file.
pub(crate) struct LoadResult {
    pub directives: Vec<Directive>,
    pub options: LedgerOptions,
    pub errors: Vec<Error>,
    pub lookup: LineLookup,
    pub parse_result: ParserResult,
}

/// Parse and interpolate a Beancount source string.
///
/// This is the common entry point for all processing functions.
pub(crate) fn load_and_interpolate(source: &str) -> LoadResult {
    let parse_result = parse_beancount(source);
    let lookup = LineLookup::new(source);

    // Collect parse errors
    let mut errors: Vec<Error> = parse_result
        .errors
        .iter()
        .map(|e| Error::with_line(e.to_string(), lookup.byte_to_line(e.span().0)))
        .collect();

    // Extract options
    let options = extract_options(&parse_result.options);

    // Extract directives
    let mut directives: Vec<_> = parse_result
        .directives
        .iter()
        .map(|s| s.value.clone())
        .collect();

    // Interpolate transactions (fill in missing amounts)
    if errors.is_empty() {
        for (i, directive) in directives.iter_mut().enumerate() {
            if let Directive::Transaction(txn) = directive {
                match interpolate(txn) {
                    Ok(result) => {
                        *txn = result.transaction;
                    }
                    Err(e) => {
                        let line = lookup.byte_to_line(parse_result.directives[i].span.start);
                        errors.push(Error::with_line(e.to_string(), line));
                    }
                }
            }
        }
    }

    LoadResult {
        directives,
        options,
        errors,
        lookup,
        parse_result,
    }
}

/// Run validation on a loaded ledger and return validation errors.
pub(crate) fn run_validation(load: &LoadResult) -> Vec<Error> {
    if !load.errors.is_empty() {
        return Vec::new();
    }

    let mut date_to_line: HashMap<String, u32> = HashMap::new();
    for spanned in &load.parse_result.directives {
        let line = load.lookup.byte_to_line(spanned.span.start);
        let date = spanned.value.date().to_string();
        date_to_line.entry(date).or_insert(line);
    }

    validate_ledger(&load.directives)
        .into_iter()
        .map(|err| {
            let line = date_to_line.get(&err.date.to_string()).copied();
            Error {
                message: err.message,
                line,
                column: None,
                severity: Severity::Error,
            }
        })
        .collect()
}

/// Serialize a value to `JsValue` using JSON-compatible settings.
///
/// This ensures:
/// - `None` serializes as `null` (not `undefined`)
/// - Maps serialize as plain objects (not ES2015 `Map`)
pub(crate) fn to_js<T: serde::Serialize>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    value
        .serialize(&serializer)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Extract [`LedgerOptions`] from parsed option directives.
pub(crate) fn extract_options(
    options: &[(String, String, rustledger_parser::Span)],
) -> LedgerOptions {
    let mut ledger_options = LedgerOptions::default();

    for (key, value, _span) in options {
        match key.as_str() {
            "title" => ledger_options.title = Some(value.clone()),
            "operating_currency" => {
                ledger_options.operating_currencies.push(value.clone());
            }
            _ => {} // Ignore other options for now
        }
    }

    ledger_options
}
