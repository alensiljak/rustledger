//! Beancount WASM Bindings.
//!
//! This crate provides WebAssembly bindings for using Beancount from JavaScript/TypeScript.
//!
//! # Features
//!
//! - Parse Beancount files
//! - Validate ledgers
//! - Run BQL queries
//! - Format directives
//!
//! # Example (JavaScript)
//!
//! ```javascript
//! import init, { parse, validateSource, query } from '@rustledger/wasm';
//!
//! await init();
//!
//! const source = `
//! 2024-01-01 open Assets:Bank USD
//! 2024-01-15 * "Coffee"
//!   Expenses:Food  5.00 USD
//!   Assets:Bank   -5.00 USD
//! `;
//!
//! const result = parse(source);
//! if (result.errors.length === 0) {
//!     const validation = validateSource(source);
//!     console.log('Validation errors:', validation.errors);
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// wasm_bindgen doesn't support const fn on exported methods
#![allow(clippy::missing_const_for_fn)]

// Internal modules
mod convert;
mod editor;
mod helpers;
mod utils;

// Public modules
pub mod types;

// Public API modules
mod api;
mod parsed_ledger;

// Re-export public API
pub use api::{balances, format, parse, query, validate_source, version};
pub use api::{parse_multi_file, query_multi_file, validate_multi_file};

#[cfg(feature = "completions")]
pub use api::bql_completions;

#[cfg(feature = "plugins")]
pub use api::{list_plugins, run_plugin};

pub use api::expand_pads;
pub use parsed_ledger::ParsedLedger;

use wasm_bindgen::prelude::*;

// =============================================================================
// TypeScript Type Definitions
// =============================================================================

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &'static str = r#"
/** Error severity level. */
export type Severity = 'error' | 'warning';

/** Error with source location information. */
export interface BeancountError {
    message: string;
    line?: number;
    column?: number;
    severity: Severity;
}

/** Amount with number and currency. */
export interface Amount {
    number: string;
    currency: string;
}

/** Posting cost specification. */
export interface PostingCost {
    number_per?: string;
    currency?: string;
    date?: string;
    label?: string;
}

/** A posting within a transaction. */
export interface Posting {
    account: string;
    units?: Amount;
    cost?: PostingCost;
    price?: Amount;
}

/** Base directive with date. */
interface BaseDirective {
    date: string;
}

/** Transaction directive. */
export interface TransactionDirective extends BaseDirective {
    type: 'transaction';
    flag: string;
    payee?: string;
    narration?: string;
    tags: string[];
    links: string[];
    postings: Posting[];
}

/** Balance assertion directive. */
export interface BalanceDirective extends BaseDirective {
    type: 'balance';
    account: string;
    amount: Amount;
}

/** Open account directive. */
export interface OpenDirective extends BaseDirective {
    type: 'open';
    account: string;
    currencies: string[];
    booking?: string;
}

/** Close account directive. */
export interface CloseDirective extends BaseDirective {
    type: 'close';
    account: string;
}

/** All directive types. */
export type Directive =
    | TransactionDirective
    | BalanceDirective
    | OpenDirective
    | CloseDirective
    | { type: 'commodity'; date: string; currency: string }
    | { type: 'pad'; date: string; account: string; source_account: string }
    | { type: 'event'; date: string; event_type: string; value: string }
    | { type: 'note'; date: string; account: string; comment: string }
    | { type: 'document'; date: string; account: string; path: string }
    | { type: 'price'; date: string; currency: string; amount: Amount }
    | { type: 'query'; date: string; name: string; query_string: string }
    | { type: 'custom'; date: string; custom_type: string };

/** Ledger options. */
export interface LedgerOptions {
    operating_currencies: string[];
    title?: string;
}

/** Parsed ledger. */
export interface Ledger {
    directives: Directive[];
    options: LedgerOptions;
}

/** Result of parsing a Beancount file. */
export interface ParseResult {
    ledger?: Ledger;
    errors: BeancountError[];
}

/** Result of validation. */
export interface ValidationResult {
    valid: boolean;
    errors: BeancountError[];
}

/** Cell value in query results. */
export type CellValue =
    | null
    | string
    | number
    | boolean
    | Amount
    | { units: Amount; cost?: { number: string; currency: string; date?: string; label?: string } }
    | { positions: Array<{ units: Amount }> }
    | string[];

/** Result of a BQL query. */
export interface QueryResult {
    columns: string[];
    rows: CellValue[][];
    errors: BeancountError[];
}

/** Result of formatting. */
export interface FormatResult {
    formatted?: string;
    errors: BeancountError[];
}

/** Result of pad expansion. */
export interface PadResult {
    directives: Directive[];
    padding_transactions: Directive[];
    errors: BeancountError[];
}

/** Result of running a plugin. */
export interface PluginResult {
    directives: Directive[];
    errors: BeancountError[];
}

/** Plugin information. */
export interface PluginInfo {
    name: string;
    description: string;
}

/** BQL completion suggestion. */
export interface Completion {
    text: string;
    category: string;
    description?: string;
}

/** Result of BQL completion request. */
export interface CompletionResult {
    completions: Completion[];
    context: string;
}

// =============================================================================
// Editor Integration Types (LSP-like features)
// =============================================================================

/** The kind of a completion item. */
export type EditorCompletionKind = 'keyword' | 'account' | 'accountsegment' | 'currency' | 'payee' | 'date' | 'text';

/** A completion item for Beancount source editing. */
export interface EditorCompletion {
    label: string;
    kind: EditorCompletionKind;
    detail?: string;
    insertText?: string;
}

/** Result of an editor completion request. */
export interface EditorCompletionResult {
    completions: EditorCompletion[];
    context: string;
}

/** A range in the document. */
export interface EditorRange {
    start_line: number;
    start_character: number;
    end_line: number;
    end_character: number;
}

/** Hover information for a symbol. */
export interface EditorHoverInfo {
    contents: string;
    range?: EditorRange;
}

/** A location in the document. */
export interface EditorLocation {
    line: number;
    character: number;
}

/** The kind of a symbol. */
export type SymbolKind = 'transaction' | 'account' | 'balance' | 'commodity' | 'posting' | 'pad' | 'event' | 'note' | 'document' | 'price' | 'query' | 'custom';

/** A document symbol for the outline view. */
export interface EditorDocumentSymbol {
    name: string;
    detail?: string;
    kind: SymbolKind;
    range: EditorRange;
    children?: EditorDocumentSymbol[];
    deprecated?: boolean;
}

/** The kind of reference. */
export type ReferenceKind = 'account' | 'currency' | 'payee';

/** A reference to a symbol in the document. */
export interface EditorReference {
    range: EditorRange;
    kind: ReferenceKind;
    is_definition: boolean;
    context?: string;
}

/** Result of a find-references request. */
export interface EditorReferencesResult {
    symbol: string;
    kind: ReferenceKind;
    references: EditorReference[];
}

/**
 * A parsed and validated ledger that caches the parse result.
 * Use this class when you need to perform multiple operations on the same
 * source without re-parsing each time.
 */
export class ParsedLedger {
    constructor(source: string);
    free(): void;

    /** Check if the ledger is valid (no parse or validation errors). */
    isValid(): boolean;

    /** Get all errors (parse + validation). */
    getErrors(): BeancountError[];

    /** Get parse errors only. */
    getParseErrors(): BeancountError[];

    /** Get validation errors only. */
    getValidationErrors(): BeancountError[];

    /** Get the parsed directives. */
    getDirectives(): Directive[];

    /** Get the ledger options. */
    getOptions(): LedgerOptions;

    /** Get the number of directives. */
    directiveCount(): number;

    /** Run a BQL query on this ledger. */
    query(queryStr: string): QueryResult;

    /** Get account balances (shorthand for query("BALANCES")). */
    balances(): QueryResult;

    /** Format the ledger source. */
    format(): FormatResult;

    /** Expand pad directives. */
    expandPads(): PadResult;

    /** Run a native plugin on this ledger. */
    runPlugin(pluginName: string): PluginResult;

    // =========================================================================
    // Editor Integration (LSP-like features)
    // =========================================================================

    /** Get completions at the given position. */
    getCompletions(line: number, character: number): EditorCompletionResult;

    /** Get hover information at the given position. */
    getHoverInfo(line: number, character: number): EditorHoverInfo | null;

    /** Get the definition location for the symbol at the given position. */
    getDefinition(line: number, character: number): EditorLocation | null;

    /** Get all document symbols for the outline view. */
    getDocumentSymbols(): EditorDocumentSymbol[];

    /** Find all references to the symbol at the given position. */
    getReferences(line: number, character: number): EditorReferencesResult | null;
}

// =============================================================================
// Multi-File API (for WASM environments without filesystem access)
// =============================================================================

/** Map of file paths to their contents. */
export type FileMap = Record<string, string>;

/**
 * Parse multiple Beancount files with include resolution.
 *
 * @param files - Object mapping file paths to their contents
 * @param entryPoint - The main file to start loading from (must exist in files)
 * @returns ParseResult with the combined ledger from all files
 *
 * @example
 * const result = parseMultiFile({
 *   "main.beancount": 'include "accounts.beancount"',
 *   "accounts.beancount": "2024-01-01 open Assets:Bank USD"
 * }, "main.beancount");
 */
export function parseMultiFile(files: FileMap, entryPoint: string): ParseResult;

/**
 * Validate multiple Beancount files with include resolution.
 *
 * @param files - Object mapping file paths to their contents
 * @param entryPoint - The main file to start loading from (must exist in files)
 * @returns ValidationResult indicating whether the combined ledger is valid
 */
export function validateMultiFile(files: FileMap, entryPoint: string): ValidationResult;

/**
 * Run a BQL query on multiple Beancount files.
 *
 * @param files - Object mapping file paths to their contents
 * @param entryPoint - The main file to start loading from (must exist in files)
 * @param query - The BQL query string to execute
 * @returns QueryResult with columns, rows, and any errors
 */
export function queryMultiFile(files: FileMap, entryPoint: string, query: string): QueryResult;
"#;

// =============================================================================
// Initialization
// =============================================================================

/// Initialize the WASM module.
///
/// This sets up panic hooks for better error messages in the browser console.
/// Call this once before using any other functions.
#[wasm_bindgen(start)]
pub fn init() {
    // Set up panic hook for better error messages
    console_error_panic_hook::set_once();
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse as parse_beancount;
    use rustledger_validate::validate as validate_ledger;

    #[test]
    fn test_parse_simple() {
        let source = r#"
2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee Shop" "Morning coffee"
  Expenses:Food:Coffee  5.00 USD
  Assets:Bank          -5.00 USD
"#;

        let result = parse_beancount(source);
        assert!(result.errors.is_empty());
        assert_eq!(result.directives.len(), 2);
    }

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }

    #[test]
    fn test_load_and_interpolate() {
        use helpers::load_and_interpolate;

        // Valid ledger
        let source = r#"
2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
"#;
        let load = load_and_interpolate(source);
        assert!(load.errors.is_empty());
        assert_eq!(load.directives.len(), 3);

        // Invalid ledger (unopened account)
        let source = r#"
2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
"#;
        let load = load_and_interpolate(source);
        assert!(load.errors.is_empty()); // Parse succeeds
        let validation_errors = validate_ledger(&load.directives);
        assert!(
            !validation_errors.is_empty(),
            "should detect Expenses:Food not opened"
        );
    }

    // =========================================================================
    // Multi-file API tests
    // =========================================================================

    #[test]
    fn test_multi_file_include_resolution() {
        use rustledger_loader::{Loader, VirtualFileSystem};
        use std::path::Path;

        let mut vfs = VirtualFileSystem::new();
        vfs.add_file(
            "main.beancount",
            r#"
include "accounts.beancount"

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
"#,
        );
        vfs.add_file(
            "accounts.beancount",
            r#"
2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD
"#,
        );

        let mut loader = Loader::new().with_filesystem(Box::new(vfs));
        let result = loader.load(Path::new("main.beancount")).unwrap();

        assert!(result.errors.is_empty(), "should have no errors");
        // 2 opens + 1 transaction = 3 directives
        assert_eq!(result.directives.len(), 3);
    }

    #[test]
    fn test_multi_file_nested_includes() {
        use rustledger_loader::{Loader, VirtualFileSystem};
        use std::path::Path;

        let mut vfs = VirtualFileSystem::new();
        vfs.add_file("main.beancount", r#"include "accounts/index.beancount""#);
        vfs.add_file(
            "accounts/index.beancount",
            r#"
include "assets.beancount"
include "expenses.beancount"
"#,
        );
        vfs.add_file(
            "accounts/assets.beancount",
            "2024-01-01 open Assets:Bank USD",
        );
        vfs.add_file(
            "accounts/expenses.beancount",
            "2024-01-01 open Expenses:Food USD",
        );

        let mut loader = Loader::new().with_filesystem(Box::new(vfs));
        let result = loader.load(Path::new("main.beancount")).unwrap();

        assert!(result.errors.is_empty(), "should have no errors");
        assert_eq!(result.directives.len(), 2); // 2 open directives
    }

    #[test]
    fn test_multi_file_validation() {
        use rustledger_booking::BookingEngine;
        use rustledger_core::Directive;
        use rustledger_loader::{Loader, VirtualFileSystem};
        use std::path::Path;

        let mut vfs = VirtualFileSystem::new();
        vfs.add_file(
            "main.beancount",
            r#"
include "accounts.beancount"

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank
"#,
        );
        vfs.add_file(
            "accounts.beancount",
            r#"
2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD
"#,
        );

        let mut loader = Loader::new().with_filesystem(Box::new(vfs));
        let result = loader.load(Path::new("main.beancount")).unwrap();

        assert!(result.errors.is_empty());

        // Extract directives and book transactions
        let mut directives: Vec<_> = result.directives.into_iter().map(|s| s.value).collect();
        let mut engine = BookingEngine::new();
        for directive in &mut directives {
            if let Directive::Transaction(txn) = directive {
                if let Ok(result) = engine.book_and_interpolate(txn) {
                    engine.apply(&result.transaction);
                    *txn = result.transaction;
                }
            }
        }
        // Sort by date for proper validation
        directives.sort_by_key(|d| d.date());
        let validation_errors = validate_ledger(&directives);
        assert!(
            validation_errors.is_empty(),
            "ledger should be valid, but got: {:?}",
            validation_errors
        );
    }

    /// Regression test for #659: total cost `{{ }}` syntax must produce per-unit cost.
    #[test]
    fn test_total_cost_produces_per_unit_cost() {
        use helpers::load_and_interpolate;
        use rustledger_core::Directive;

        let source = r#"
2020-01-01 open Assets:Investments:PROP PROP
2020-01-01 open Assets:Bank AUD

2020-01-16 * "Buy PROP"
  Assets:Investments:PROP  273.2200 PROP {{150.00 AUD}}
  Assets:Bank              -150.00 AUD
"#;
        let load = load_and_interpolate(source);
        assert!(load.errors.is_empty(), "errors: {:?}", load.errors);

        // Find the transaction and check that cost has number_per set
        for directive in &load.directives {
            if let Directive::Transaction(txn) = directive {
                let prop_posting = txn
                    .postings
                    .iter()
                    .find(|p| {
                        p.units
                            .as_ref()
                            .map_or(false, |u| u.currency() == Some("PROP"))
                    })
                    .expect("should have PROP posting");

                let cost = prop_posting.cost.as_ref().expect("should have cost");
                assert!(
                    cost.number_per.is_some(),
                    "total cost {{}} should be converted to per-unit cost, but number_per is None"
                );

                let per_unit = cost.number_per.unwrap();
                // 150.00 / 273.22 ≈ 0.5490
                assert!(
                    per_unit > rustledger_core::Decimal::ZERO,
                    "per-unit cost should be positive, got {per_unit}"
                );
            }
        }
    }
}
