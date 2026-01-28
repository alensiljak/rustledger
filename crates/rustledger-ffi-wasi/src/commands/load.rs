//! Load commands (load, load-full).

use std::collections::HashMap;
use std::path::Path;

use rustledger_booking::BookingEngine;
use rustledger_core::Directive;
use rustledger_loader::Loader;
use rustledger_plugin::{
    NativePluginRegistry, PluginInput, PluginOptions, directive_to_wrapper, wrapper_to_directive,
};
use serde::Serialize;

use crate::convert::directive_to_json;
use crate::helpers::load_source;
use crate::types::{DirectiveJson, Error, LedgerOptions, LoadOutput, Plugin};
use crate::{API_VERSION, output_json};

/// Output for load-full command - includes resolved includes and plugin list.
#[derive(Serialize)]
pub struct LoadFullOutput {
    pub api_version: &'static str,
    pub entries: Vec<DirectiveJson>,
    pub errors: Vec<Error>,
    pub options: LedgerOptions,
    /// Resolved plugins (from file + `auto_accounts` if enabled).
    pub plugins: Vec<Plugin>,
    /// Files that were loaded (resolved includes).
    pub loaded_files: Vec<String>,
}

/// Load beancount source from stdin.
pub fn cmd_load(source: &str, filename: &str) -> i32 {
    let load = load_source(source);

    let entries: Vec<DirectiveJson> = load
        .directives
        .iter()
        .zip(load.directive_lines.iter())
        .map(|(d, &line)| directive_to_json(d, line, filename))
        .collect();

    let output = LoadOutput {
        api_version: API_VERSION,
        entries,
        errors: load.errors,
        options: load.options,
        plugins: load.plugins,
        includes: load.includes,
    };
    output_json(&output)
}

/// Load a beancount file using the full loader pipeline.
/// This handles:
/// - Include resolution (with cycle detection)
/// - Path security (prevents path traversal)
/// - GPG decryption (for .gpg/.asc files)
/// - Optional plugin execution (`auto_accounts` sorts entries)
pub fn cmd_load_full(path: &str, run_plugins: &[&str]) -> i32 {
    let path = Path::new(path);

    // Load using the full loader
    let mut loader = Loader::new();
    let load_result = match loader.load(path) {
        Ok(result) => result,
        Err(e) => {
            let output = LoadFullOutput {
                api_version: API_VERSION,
                entries: vec![],
                errors: vec![Error::new(format!("Failed to load file: {e}"))],
                options: LedgerOptions::default(),
                plugins: vec![],
                loaded_files: vec![],
            };
            return output_json(&output);
        }
    };

    // Collect errors from loader (these are non-fatal errors)
    let mut errors: Vec<Error> = load_result
        .errors
        .iter()
        .map(|e| Error::new(e.to_string()))
        .collect();

    // Convert directives and get line numbers/filenames
    let mut directives: Vec<Directive> = Vec::new();
    let mut directive_lines: Vec<u32> = Vec::new();
    let mut directive_files: Vec<String> = Vec::new();

    for spanned in &load_result.directives {
        directives.push(spanned.value.clone());

        // Get line number and filename from source map
        let file_id = spanned.file_id as usize;
        if let Some(source_file) = load_result.source_map.get(file_id) {
            let (line, _col) = source_file.line_col(spanned.span.start);
            directive_lines.push(line as u32);
            directive_files.push(source_file.path.display().to_string());
        } else {
            directive_lines.push(0);
            directive_files.push("<unknown>".to_string());
        }
    }

    // Run booking and interpolation on transactions (sequential)
    let booking_method = load_result
        .options
        .booking_method
        .parse()
        .unwrap_or(rustledger_core::BookingMethod::Strict);
    let mut booking_engine = BookingEngine::with_method(booking_method);

    for (i, directive) in directives.iter_mut().enumerate() {
        if let Directive::Transaction(txn) = directive {
            match booking_engine.book_and_interpolate(txn) {
                Ok(result) => {
                    booking_engine.apply(&result.transaction);
                    *txn = result.transaction;
                    // Normalize total prices (@@→@) for downstream consumers
                    rustledger_booking::normalize_prices(txn);
                }
                Err(e) => {
                    errors.push(Error::new(e.to_string()).with_line(directive_lines[i]));
                }
            }
        }
    }

    // Run plugins if requested
    if !run_plugins.is_empty() && errors.is_empty() {
        let registry = NativePluginRegistry::new();

        for plugin_name in run_plugins {
            if let Some(plugin) = registry.find(plugin_name) {
                // Convert directives to wrappers for plugin, preserving source locations
                let wrappers: Vec<_> = directives
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let mut wrapper = directive_to_wrapper(d);
                        wrapper.filename = Some(
                            directive_files
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| "<unknown>".to_string()),
                        );
                        wrapper.lineno = Some(directive_lines.get(i).copied().unwrap_or(0));
                        wrapper
                    })
                    .collect();

                let input = PluginInput {
                    directives: wrappers,
                    options: PluginOptions {
                        operating_currencies: load_result.options.operating_currency.clone(),
                        title: load_result.options.title.clone(),
                    },
                    config: None,
                };

                let output = plugin.process(input);

                // Convert errors
                for err in output.errors {
                    errors.push(Error::new(err.message));
                }

                // Convert wrappers back to directives
                let mut new_directives = Vec::new();
                let mut new_lines = Vec::new();
                let mut new_files = Vec::new();

                for wrapper in &output.directives {
                    if let Ok(directive) = wrapper_to_directive(wrapper) {
                        new_directives.push(directive);
                        new_lines.push(wrapper.lineno.unwrap_or(0));
                        new_files.push(
                            wrapper
                                .filename
                                .clone()
                                .unwrap_or_else(|| "<plugin>".to_string()),
                        );
                    }
                }

                directives = new_directives;
                directive_lines = new_lines;
                directive_files = new_files;
            } else {
                errors.push(Error::new(format!("Unknown plugin: {plugin_name}")));
            }
        }
    }

    // Convert options
    let options = LedgerOptions {
        title: load_result.options.title.clone(),
        operating_currency: load_result.options.operating_currency.clone(),
        name_assets: load_result.options.name_assets.clone(),
        name_liabilities: load_result.options.name_liabilities.clone(),
        name_equity: load_result.options.name_equity.clone(),
        name_income: load_result.options.name_income.clone(),
        name_expenses: load_result.options.name_expenses.clone(),
        documents: load_result.options.documents.clone(),
        commodities: Vec::new(),
        booking_method: load_result.options.booking_method.clone(),
        display_precision: HashMap::new(),
    };

    // Convert plugins from loader result
    let plugins: Vec<Plugin> = load_result
        .plugins
        .iter()
        .map(|p| Plugin {
            name: p.name.clone(),
            config: p.config.clone(),
        })
        .collect();

    // Get list of loaded files
    let loaded_files: Vec<String> = load_result
        .source_map
        .files()
        .iter()
        .map(|sf| sf.path.display().to_string())
        .collect();

    // Build entries
    let entries: Vec<DirectiveJson> = directives
        .iter()
        .enumerate()
        .map(|(i, d)| {
            directive_to_json(
                d,
                directive_lines.get(i).copied().unwrap_or(0),
                directive_files.get(i).map_or("<unknown>", String::as_str),
            )
        })
        .collect();

    let output = LoadFullOutput {
        api_version: API_VERSION,
        entries,
        errors,
        options,
        plugins,
        loaded_files,
    };
    output_json(&output)
}
