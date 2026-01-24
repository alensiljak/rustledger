//! Format commands (format, format-entry, format-entries).

use rustledger_core::format::FormatConfig;
use rustledger_parser::parse as parse_beancount;
use serde::Serialize;

use crate::helpers::LineLookup;
use crate::types::{Error, InputEntry, input_entry_to_directive};
use crate::{API_VERSION, output_json, parse_json_error};

/// Output for format command.
#[derive(Serialize)]
pub struct FormatOutput {
    pub api_version: &'static str,
    /// Formatted beancount source text.
    pub formatted: String,
    /// Any errors encountered.
    pub errors: Vec<Error>,
}

/// Output for format-entry command.
#[derive(Serialize)]
pub struct FormatEntryOutput {
    pub api_version: &'static str,
    pub formatted: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<Error>,
}

/// Format beancount source from stdin.
pub fn cmd_format(source: &str) -> i32 {
    let parse_result = parse_beancount(source);
    let lookup = LineLookup::new(source);

    let errors: Vec<Error> = parse_result
        .errors
        .iter()
        .map(|e| Error::new(e.to_string()).with_line(lookup.byte_to_line(e.span().0)))
        .collect();

    // Format all directives
    let config = FormatConfig::default();
    let mut formatted = String::new();

    // Add options first
    for (key, value, _span) in &parse_result.options {
        formatted.push_str(&format!("option \"{key}\" \"{value}\"\n"));
    }
    if !parse_result.options.is_empty() {
        formatted.push('\n');
    }

    // Add plugins
    for (plugin, config_opt, _span) in &parse_result.plugins {
        if let Some(cfg) = config_opt {
            formatted.push_str(&format!("plugin \"{plugin}\" \"{cfg}\"\n"));
        } else {
            formatted.push_str(&format!("plugin \"{plugin}\"\n"));
        }
    }
    if !parse_result.plugins.is_empty() {
        formatted.push('\n');
    }

    // Format directives
    for spanned in &parse_result.directives {
        formatted.push_str(&rustledger_core::format::format_directive(
            &spanned.value,
            &config,
        ));
    }

    let output = FormatOutput {
        api_version: API_VERSION,
        formatted,
        errors,
    };
    output_json(&output)
}

/// Format a single entry from JSON to beancount text.
pub fn cmd_format_entry(json_str: &str) -> i32 {
    // Parse JSON into InputEntry
    let entry: InputEntry = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => {
            let output = FormatEntryOutput {
                api_version: API_VERSION,
                formatted: String::new(),
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    // Convert to Directive
    let directive = match input_entry_to_directive(&entry) {
        Ok(d) => d,
        Err(e) => {
            let output = FormatEntryOutput {
                api_version: API_VERSION,
                formatted: String::new(),
                errors: vec![Error::new(e)],
            };
            return output_json(&output);
        }
    };

    // Format directive
    let config = FormatConfig::default();
    let formatted = rustledger_core::format::format_directive(&directive, &config);

    let output = FormatEntryOutput {
        api_version: API_VERSION,
        formatted,
        errors: vec![],
    };
    output_json(&output)
}

/// Format multiple entries from JSON array to beancount text.
pub fn cmd_format_entries(json_str: &str) -> i32 {
    // Parse JSON array of entries
    let entries: Vec<InputEntry> = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => {
            let output = FormatEntryOutput {
                api_version: API_VERSION,
                formatted: String::new(),
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    let config = FormatConfig::default();
    let mut formatted = String::new();
    let mut errors = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        match input_entry_to_directive(entry) {
            Ok(directive) => {
                formatted.push_str(&rustledger_core::format::format_directive(
                    &directive, &config,
                ));
            }
            Err(e) => {
                errors.push(Error::new(format!("Entry {i}: {e}")).with_entry_index(i));
            }
        }
    }

    let output = FormatEntryOutput {
        api_version: API_VERSION,
        formatted,
        errors,
    };
    output_json(&output)
}
