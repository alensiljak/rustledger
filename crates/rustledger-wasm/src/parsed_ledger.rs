//! Stateful ledger class for WASM.
//!
//! Provides a cached, parsed representation of a Beancount ledger for efficient
//! multiple operations without re-parsing.

use std::collections::HashMap;
use std::path::Path;
use wasm_bindgen::prelude::*;

use rustledger_core::Directive;
use rustledger_parser::ParseResult as ParserResult;
use rustledger_query::{Executor, parse as parse_query};

use crate::convert::{directive_to_json, value_to_cell};
use crate::editor;
use crate::helpers::{load_and_book, run_validation, to_js};
#[cfg(feature = "plugins")]
use crate::types::PluginResult;
use crate::types::{Error, FormatResult, LedgerOptions, PadResult, QueryResult};

/// A parsed and validated ledger that caches the parse result.
///
/// Use this class when you need to perform multiple operations on the same
/// source without re-parsing each time. Supports both single-file and
/// multi-file ledgers.
///
/// # Example (JavaScript)
///
/// ```javascript
/// // Single file
/// const ledger = new ParsedLedger(source);
///
/// // Multi-file (with include resolution)
/// const ledger = ParsedLedger.fromFiles({
///     "main.beancount": 'include "accounts.beancount"\n...',
///     "accounts.beancount": "2024-01-01 open Assets:Bank USD\n..."
/// }, "main.beancount");
///
/// if (ledger.isValid()) {
///     const balances = ledger.query("BALANCES");
/// }
/// ```
#[wasm_bindgen(skip_typescript)]
pub struct ParsedLedger {
    /// The original source text (single-file only).
    source: Option<String>,
    /// The raw parse result (single-file only, for editor features).
    parse_result: Option<ParserResult>,
    /// The booked directives.
    directives: Vec<Directive>,
    /// Ledger options.
    options: LedgerOptions,
    /// Parse/processing errors.
    parse_errors: Vec<Error>,
    /// Validation errors.
    validation_errors: Vec<Error>,
    /// Cached editor data (single-file only).
    editor_cache: Option<editor::EditorCache>,
}

#[wasm_bindgen]
impl ParsedLedger {
    /// Create a new `ParsedLedger` from a single source string.
    ///
    /// Parses, books, and validates the source. Call `isValid()` to check for errors.
    /// Editor features (completions, hover, etc.) are available with this constructor.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Self {
        let load = load_and_book(source);
        let validation_errors = run_validation(&load);

        // Build editor cache once for efficient editor operations
        let editor_cache = editor::EditorCache::new(source, &load.parse_result);

        Self {
            source: Some(source.to_string()),
            parse_result: Some(load.parse_result),
            directives: load.directives,
            options: load.options,
            parse_errors: load.errors,
            validation_errors,
            editor_cache: Some(editor_cache),
        }
    }

    /// Create a `ParsedLedger` from multiple files with include resolution.
    ///
    /// This is the multi-file equivalent of the constructor, useful for ledgers
    /// that span multiple files with `include` directives.
    ///
    /// Editor features (completions, hover, definition, etc.) are not available
    /// on multi-file ledgers — use the single-file constructor for editor integration.
    ///
    /// # Arguments
    ///
    /// * `files` - A JavaScript object mapping file paths to their contents.
    /// * `entry_point` - The main file to start loading from (must exist in `files`).
    #[wasm_bindgen(js_name = "fromFiles")]
    pub fn from_files(files: JsValue, entry_point: &str) -> Result<Self, JsError> {
        use rustledger_loader::{FileSystem, LoadOptions, Loader, VirtualFileSystem, process};

        let file_map: HashMap<String, String> = serde_wasm_bindgen::from_value(files)
            .map_err(|e| JsError::new(&format!("Invalid files object: {e}")))?;

        if file_map.is_empty() {
            return Err(JsError::new("Files map cannot be empty"));
        }

        let vfs = VirtualFileSystem::from_files(file_map);

        if !vfs.exists(Path::new(entry_point)) {
            return Err(JsError::new(&format!(
                "Entry point '{entry_point}' not found in files map"
            )));
        }

        let mut loader = Loader::new().with_filesystem(Box::new(vfs));

        let load_result = match loader.load(Path::new(entry_point)) {
            Ok(result) => result,
            Err(e) => {
                return Ok(Self {
                    source: None,
                    parse_result: None,
                    directives: Vec::new(),
                    options: LedgerOptions::default(),
                    parse_errors: vec![Error::new(format!("Load error: {e}"))],
                    validation_errors: Vec::new(),
                    editor_cache: None,
                });
            }
        };

        let options = LedgerOptions {
            title: load_result.options.title.clone(),
            operating_currencies: load_result.options.operating_currency.clone(),
        };

        let load_options = LoadOptions {
            validate: true,
            ..Default::default()
        };

        match process(load_result, &load_options) {
            Ok(ledger) => {
                let directives: Vec<Directive> =
                    ledger.directives.into_iter().map(|s| s.value).collect();
                let errors: Vec<Error> = ledger.errors.into_iter().map(Error::from).collect();

                // Build completions cache from booked directives (accounts,
                // currencies, payees across all files)
                let editor_cache = editor::EditorCache::from_directives(&directives);

                // Store as validation_errors (not parse_errors) so queries still
                // work on multi-file ledgers with validation issues, matching
                // single-file behavior where parse_errors blocks queries but
                // validation_errors don't.
                Ok(Self {
                    source: None,
                    parse_result: None,
                    directives,
                    options,
                    parse_errors: Vec::new(),
                    validation_errors: errors,
                    editor_cache: Some(editor_cache),
                })
            }
            Err(e) => Ok(Self {
                source: None,
                parse_result: None,
                directives: Vec::new(),
                options,
                parse_errors: vec![Error::new(format!("Processing error: {e}"))],
                validation_errors: Vec::new(),
                editor_cache: None,
            }),
        }
    }

    /// Check if the ledger is valid (no parse or validation errors).
    #[wasm_bindgen(js_name = "isValid")]
    pub fn is_valid(&self) -> bool {
        self.parse_errors.is_empty() && self.validation_errors.is_empty()
    }

    /// Get all errors (parse + validation).
    #[wasm_bindgen(js_name = "getErrors")]
    pub fn get_errors(&self) -> Result<JsValue, JsError> {
        let mut all_errors = self.parse_errors.clone();
        all_errors.extend(self.validation_errors.clone());
        to_js(&all_errors)
    }

    /// Get parse errors only.
    #[wasm_bindgen(js_name = "getParseErrors")]
    pub fn get_parse_errors(&self) -> Result<JsValue, JsError> {
        to_js(&self.parse_errors)
    }

    /// Get validation errors only.
    #[wasm_bindgen(js_name = "getValidationErrors")]
    pub fn get_validation_errors(&self) -> Result<JsValue, JsError> {
        to_js(&self.validation_errors)
    }

    /// Get the parsed directives.
    #[wasm_bindgen(js_name = "getDirectives")]
    pub fn get_directives(&self) -> Result<JsValue, JsError> {
        let directives: Vec<_> = self.directives.iter().map(directive_to_json).collect();
        to_js(&directives)
    }

    /// Get the ledger options.
    #[wasm_bindgen(js_name = "getOptions")]
    pub fn get_options(&self) -> Result<JsValue, JsError> {
        to_js(&self.options)
    }

    /// Get the number of directives.
    #[wasm_bindgen(js_name = "directiveCount")]
    pub fn directive_count(&self) -> usize {
        self.directives.len()
    }

    /// Run a BQL query on this ledger.
    #[wasm_bindgen]
    pub fn query(&self, query_str: &str) -> Result<JsValue, JsError> {
        if !self.parse_errors.is_empty() {
            let result = QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                errors: self.parse_errors.clone(),
            };
            return to_js(&result);
        }

        let query = match parse_query(query_str) {
            Ok(q) => q,
            Err(e) => {
                let result = QueryResult {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    errors: vec![Error::new(e.to_string())],
                };
                return to_js(&result);
            }
        };

        let mut executor = Executor::new(&self.directives);
        match executor.execute(&query) {
            Ok(result) => {
                let rows: Vec<Vec<_>> = result
                    .rows
                    .iter()
                    .map(|row| row.iter().map(value_to_cell).collect())
                    .collect();

                let query_result = QueryResult {
                    columns: result.columns,
                    rows,
                    errors: Vec::new(),
                };
                to_js(&query_result)
            }
            Err(e) => {
                let result = QueryResult {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    errors: vec![Error::new(format!("Query execution error: {e}"))],
                };
                to_js(&result)
            }
        }
    }

    /// Get account balances (shorthand for query("BALANCES")).
    #[wasm_bindgen]
    pub fn balances(&self) -> Result<JsValue, JsError> {
        self.query("BALANCES")
    }

    /// Format the ledger source.
    ///
    /// Only available for single-file ledgers.
    #[wasm_bindgen]
    pub fn format(&self) -> Result<JsValue, JsError> {
        use rustledger_core::{FormatConfig, format_directive};

        if self.source.is_none() {
            let result = FormatResult {
                formatted: None,
                errors: vec![Error::new(
                    "format() is only available for single-file ledgers",
                )],
            };
            return to_js(&result);
        }

        if !self.parse_errors.is_empty() {
            let result = FormatResult {
                formatted: None,
                errors: self.parse_errors.clone(),
            };
            return to_js(&result);
        }

        let config = FormatConfig::default();
        let mut formatted = String::new();

        for directive in &self.directives {
            formatted.push_str(&format_directive(directive, &config));
            formatted.push('\n');
        }

        let result = FormatResult {
            formatted: Some(formatted),
            errors: Vec::new(),
        };
        to_js(&result)
    }

    /// Expand pad directives.
    #[wasm_bindgen(js_name = "expandPads")]
    pub fn expand_pads(&self) -> Result<JsValue, JsError> {
        use rustledger_booking::process_pads;

        if !self.parse_errors.is_empty() {
            let result = PadResult {
                directives: Vec::new(),
                padding_transactions: Vec::new(),
                errors: self.parse_errors.clone(),
            };
            return to_js(&result);
        }

        let pad_result = process_pads(&self.directives);

        let result = PadResult {
            directives: pad_result
                .directives
                .iter()
                .map(directive_to_json)
                .collect(),
            padding_transactions: pad_result
                .padding_transactions
                .iter()
                .map(|txn| directive_to_json(&Directive::Transaction(txn.clone())))
                .collect(),
            errors: pad_result
                .errors
                .iter()
                .map(|e| Error::new(e.message.clone()))
                .collect(),
        };
        to_js(&result)
    }

    /// Run a native plugin on this ledger.
    #[cfg(feature = "plugins")]
    #[wasm_bindgen(js_name = "runPlugin")]
    pub fn run_plugin(&self, plugin_name: &str) -> Result<JsValue, JsError> {
        use rustledger_plugin::{
            NativePluginRegistry, PluginInput, PluginOptions, directives_to_wrappers,
            wrappers_to_directives,
        };

        if !self.parse_errors.is_empty() {
            let result = PluginResult {
                directives: Vec::new(),
                errors: self.parse_errors.clone(),
            };
            return to_js(&result);
        }

        let registry = NativePluginRegistry::new();
        let Some(plugin) = registry.find(plugin_name) else {
            let result = PluginResult {
                directives: Vec::new(),
                errors: vec![Error::new(format!("Unknown plugin: {plugin_name}"))],
            };
            return to_js(&result);
        };

        let wrappers = directives_to_wrappers(&self.directives);
        let input = PluginInput {
            directives: wrappers,
            options: PluginOptions::default(),
            config: None,
        };

        let output = plugin.process(input);

        let output_directives = match wrappers_to_directives(&output.directives) {
            Ok(dirs) => dirs,
            Err(e) => {
                let result = PluginResult {
                    directives: Vec::new(),
                    errors: vec![Error::new(format!("Conversion error: {e}"))],
                };
                return to_js(&result);
            }
        };

        let result = PluginResult {
            directives: output_directives.iter().map(directive_to_json).collect(),
            errors: output
                .errors
                .iter()
                .map(|e| match e.severity {
                    rustledger_plugin::PluginErrorSeverity::Warning => {
                        Error::warning(e.message.clone())
                    }
                    rustledger_plugin::PluginErrorSeverity::Error => Error::new(e.message.clone()),
                })
                .collect(),
        };
        to_js(&result)
    }

    // =========================================================================
    // Editor Integration (LSP-like features)
    // =========================================================================

    /// Check if this is a multi-file ledger.
    ///
    /// Multi-file ledgers do not support editor features (completions, hover, etc.).
    #[wasm_bindgen(js_name = "isMultiFile")]
    pub fn is_multi_file(&self) -> bool {
        self.source.is_none()
    }

    /// Get completions at the given position.
    ///
    /// Returns context-aware completions for accounts, currencies, directives, etc.
    /// For single-file ledgers, uses the stored source. For multi-file, use
    /// `getCompletionsForSource()` instead.
    #[wasm_bindgen(js_name = "getCompletions")]
    pub fn get_completions(&self, line: u32, character: u32) -> Result<JsValue, JsError> {
        let (Some(source), Some(cache)) = (&self.source, &self.editor_cache) else {
            return Err(JsError::new(
                "getCompletions() requires single-file ledger; use getCompletionsForSource() for multi-file",
            ));
        };
        let result = editor::get_completions_cached(source, line, character, cache);
        to_js(&result)
    }

    /// Get completions for a specific source string.
    ///
    /// Use this for multi-file ledgers where you're editing one file but want
    /// completions from all files (accounts, currencies, payees).
    #[wasm_bindgen(js_name = "getCompletionsForSource")]
    pub fn get_completions_for_source(
        &self,
        source: &str,
        line: u32,
        character: u32,
    ) -> Result<JsValue, JsError> {
        let Some(cache) = &self.editor_cache else {
            return Err(JsError::new("Editor cache not available"));
        };
        let result = editor::get_completions_cached(source, line, character, cache);
        to_js(&result)
    }

    /// Get hover information at the given position.
    ///
    /// Returns documentation for accounts, currencies, and directive keywords.
    /// Only available for single-file ledgers.
    #[wasm_bindgen(js_name = "getHoverInfo")]
    pub fn get_hover_info(&self, line: u32, character: u32) -> Result<JsValue, JsError> {
        let (Some(source), Some(parse_result), Some(cache)) =
            (&self.source, &self.parse_result, &self.editor_cache)
        else {
            return Err(JsError::new(
                "getHoverInfo() is only available for single-file ledgers",
            ));
        };
        let result = editor::get_hover_info_cached(source, line, character, parse_result, cache);
        to_js(&result)
    }

    /// Get the definition location for the symbol at the given position.
    ///
    /// Returns the location of the `open` or `commodity` directive for accounts/currencies.
    /// Only available for single-file ledgers.
    #[wasm_bindgen(js_name = "getDefinition")]
    pub fn get_definition(&self, line: u32, character: u32) -> Result<JsValue, JsError> {
        let (Some(source), Some(parse_result), Some(cache)) =
            (&self.source, &self.parse_result, &self.editor_cache)
        else {
            return Err(JsError::new(
                "getDefinition() is only available for single-file ledgers",
            ));
        };
        let result = editor::get_definition_cached(source, line, character, parse_result, cache);
        to_js(&result)
    }

    /// Get all document symbols for the outline view.
    ///
    /// Returns a hierarchical list of all directives with their positions.
    /// Only available for single-file ledgers.
    #[wasm_bindgen(js_name = "getDocumentSymbols")]
    pub fn get_document_symbols(&self) -> Result<JsValue, JsError> {
        let (Some(parse_result), Some(cache)) = (&self.parse_result, &self.editor_cache) else {
            return Err(JsError::new(
                "getDocumentSymbols() is only available for single-file ledgers",
            ));
        };
        let result = editor::get_document_symbols_cached(parse_result, cache);
        to_js(&result)
    }

    /// Find all references to the symbol at the given position.
    ///
    /// Returns all occurrences of accounts, currencies, or payees in the document.
    /// Only available for single-file ledgers.
    #[wasm_bindgen(js_name = "getReferences")]
    pub fn get_references(&self, line: u32, character: u32) -> Result<JsValue, JsError> {
        let (Some(source), Some(parse_result), Some(cache)) =
            (&self.source, &self.parse_result, &self.editor_cache)
        else {
            return Err(JsError::new(
                "getReferences() is only available for single-file ledgers",
            ));
        };
        let result = editor::get_references_cached(source, line, character, parse_result, cache);
        to_js(&result)
    }
}
