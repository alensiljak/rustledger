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
pub use api::{
    balances, format, parse, query, validate_source, version,
};

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
}
