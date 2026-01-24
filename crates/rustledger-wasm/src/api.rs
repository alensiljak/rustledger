//! Public WASM API functions.
//!
//! These functions are exposed to JavaScript via wasm-bindgen.

use wasm_bindgen::prelude::*;

use rustledger_core::Directive;
use rustledger_parser::parse as parse_beancount;

use crate::convert::{directive_to_json, value_to_cell};
use crate::helpers::{extract_options, load_and_interpolate, run_validation, to_js};
#[cfg(feature = "completions")]
use crate::types::{CompletionJson, CompletionResultJson};
use crate::types::{
    Error, FormatResult, Ledger, PadResult, ParseResult, QueryResult, ValidationResult,
};
#[cfg(feature = "plugins")]
use crate::types::{PluginInfo, PluginResult};
use crate::utils::LineLookup;

/// Parse a Beancount source string.
///
/// Returns a `ParseResult` with the parsed ledger and any errors.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<JsValue, JsError> {
    let result = parse_beancount(source);
    let lookup = LineLookup::new(source);

    let errors: Vec<Error> = result
        .errors
        .iter()
        .map(|e| Error::with_line(e.to_string(), lookup.byte_to_line(e.span().0)))
        .collect();

    // Extract options from parsed result
    let options = extract_options(&result.options);

    let ledger = Some(Ledger {
        directives: result
            .directives
            .iter()
            .map(|spanned| directive_to_json(&spanned.value))
            .collect(),
        options,
    });

    let parse_result = ParseResult { ledger, errors };
    to_js(&parse_result)
}

/// Validate a Beancount source string.
///
/// Parses, interpolates, and validates in one step.
/// Returns a `ValidationResult` indicating whether the ledger is valid.
#[wasm_bindgen(js_name = "validateSource")]
pub fn validate_source(source: &str) -> Result<JsValue, JsError> {
    let load = load_and_interpolate(source);
    let validation_errors = run_validation(&load);
    let mut errors = load.errors;
    errors.extend(validation_errors);

    let result = ValidationResult {
        valid: errors.is_empty(),
        errors,
    };
    to_js(&result)
}

/// Run a BQL query on a Beancount source string.
///
/// Parses the source, interpolates, then executes the query.
/// Returns a `QueryResult` with columns, rows, and any errors.
#[wasm_bindgen]
pub fn query(source: &str, query_str: &str) -> Result<JsValue, JsError> {
    use rustledger_query::{Executor, parse as parse_query};

    let load = load_and_interpolate(source);

    // Return early if there were parse/interpolation errors
    if !load.errors.is_empty() {
        let result = QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            errors: load.errors,
        };
        return to_js(&result);
    }

    // Parse the query
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

    let mut executor = Executor::new(&load.directives);
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

/// Get version information.
///
/// Returns the version string of the rustledger-wasm package.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Format a Beancount source string.
///
/// Parses and reformats with consistent alignment.
/// Returns a `FormatResult` with the formatted source or errors.
#[wasm_bindgen]
pub fn format(source: &str) -> Result<JsValue, JsError> {
    use rustledger_core::{FormatConfig, format_directive};

    let parse_result = parse_beancount(source);
    let lookup = LineLookup::new(source);

    if !parse_result.errors.is_empty() {
        let result = FormatResult {
            formatted: None,
            errors: parse_result
                .errors
                .iter()
                .map(|e| Error::with_line(e.to_string(), lookup.byte_to_line(e.span().0)))
                .collect(),
        };
        return to_js(&result);
    }

    let config = FormatConfig::default();
    let mut formatted = String::new();

    for spanned in &parse_result.directives {
        formatted.push_str(&format_directive(&spanned.value, &config));
        formatted.push('\n');
    }

    let result = FormatResult {
        formatted: Some(formatted),
        errors: Vec::new(),
    };
    to_js(&result)
}

/// Process pad directives and expand them.
///
/// Returns directives with pad-generated transactions included.
#[wasm_bindgen(js_name = "expandPads")]
pub fn expand_pads(source: &str) -> Result<JsValue, JsError> {
    use rustledger_booking::process_pads;

    let load = load_and_interpolate(source);

    // Return early if there were parse/interpolation errors
    if !load.errors.is_empty() {
        let result = PadResult {
            directives: Vec::new(),
            padding_transactions: Vec::new(),
            errors: load.errors,
        };
        return to_js(&result);
    }

    // Process pads
    let pad_result = process_pads(&load.directives);

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

/// Run a native plugin on the source.
///
/// Available plugins can be listed with `listPlugins()`.
#[cfg(feature = "plugins")]
#[wasm_bindgen(js_name = "runPlugin")]
pub fn run_plugin(source: &str, plugin_name: &str) -> Result<JsValue, JsError> {
    use rustledger_plugin::{
        NativePluginRegistry, PluginInput, PluginOptions, directives_to_wrappers,
        wrappers_to_directives,
    };

    let load = load_and_interpolate(source);

    // Return early if there were parse/interpolation errors
    if !load.errors.is_empty() {
        let result = PluginResult {
            directives: Vec::new(),
            errors: load.errors,
        };
        return to_js(&result);
    }

    // Find and run the plugin
    let registry = NativePluginRegistry::new();
    let Some(plugin) = registry.find(plugin_name) else {
        let result = PluginResult {
            directives: Vec::new(),
            errors: vec![Error::new(format!("Unknown plugin: {plugin_name}"))],
        };
        return to_js(&result);
    };

    // Convert directives to plugin format and run
    let wrappers = directives_to_wrappers(&load.directives);
    let input = PluginInput {
        directives: wrappers,
        options: PluginOptions::default(),
        config: None,
    };

    let output = plugin.process(input);

    // Convert back
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

/// List available native plugins.
///
/// Returns an array of `PluginInfo` objects with name and description.
#[cfg(feature = "plugins")]
#[wasm_bindgen(js_name = "listPlugins")]
pub fn list_plugins() -> Result<JsValue, JsError> {
    use rustledger_plugin::NativePluginRegistry;

    let registry = NativePluginRegistry::new();
    let plugins: Vec<PluginInfo> = registry
        .list()
        .iter()
        .map(|p| PluginInfo {
            name: p.name().to_string(),
            description: p.description().to_string(),
        })
        .collect();

    to_js(&plugins)
}

/// Calculate account balances.
///
/// Shorthand for `query(source, "BALANCES")`.
#[wasm_bindgen]
pub fn balances(source: &str) -> Result<JsValue, JsError> {
    query(source, "BALANCES")
}

/// Get BQL query completions at cursor position.
///
/// Returns context-aware completions for the BQL query language.
#[cfg(feature = "completions")]
#[wasm_bindgen(js_name = "bqlCompletions")]
pub fn bql_completions(partial_query: &str, cursor_pos: usize) -> Result<JsValue, JsError> {
    use rustledger_query::completions;

    let result = completions::complete(partial_query, cursor_pos);

    let json_result = CompletionResultJson {
        completions: result
            .completions
            .into_iter()
            .map(|c| CompletionJson {
                text: c.text,
                category: c.category.as_str().to_string(),
                description: c.description,
            })
            .collect(),
        context: format!("{:?}", result.context),
    };

    to_js(&json_result)
}
