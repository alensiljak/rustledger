//! Entry creation commands (create-entry, create-entries).

use serde::Serialize;

use crate::convert::directive_to_json;
use crate::types::{DirectiveJson, Error, InputEntry, input_entry_to_directive};
use crate::{API_VERSION, output_json, parse_json_error};

/// Output for create-entry command.
#[derive(Serialize)]
pub struct CreateEntryOutput {
    pub api_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<DirectiveJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<Error>,
}

/// Output for create-entries command.
#[derive(Serialize)]
pub struct CreateEntriesOutput {
    pub api_version: &'static str,
    pub entries: Vec<DirectiveJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<Error>,
}

/// Create a full entry with hash from minimal JSON input.
pub fn cmd_create_entry(json_str: &str) -> i32 {
    // Parse JSON into InputEntry
    let input_entry: InputEntry = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => {
            let output = CreateEntryOutput {
                api_version: API_VERSION,
                entry: None,
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    // Convert to Directive
    let directive = match input_entry_to_directive(&input_entry) {
        Ok(d) => d,
        Err(e) => {
            let output = CreateEntryOutput {
                api_version: API_VERSION,
                entry: None,
                errors: vec![Error::new(e)],
            };
            return output_json(&output);
        }
    };

    // Convert to full DirectiveJson with hash
    let entry_json = directive_to_json(&directive, 0, "<created>");

    let output = CreateEntryOutput {
        api_version: API_VERSION,
        entry: Some(entry_json),
        errors: vec![],
    };
    output_json(&output)
}

/// Create multiple entries from JSON array.
pub fn cmd_create_entries(json_str: &str) -> i32 {
    // Parse JSON array of entries
    let input_entries: Vec<InputEntry> = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => {
            let output = CreateEntriesOutput {
                api_version: API_VERSION,
                entries: vec![],
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for (i, input_entry) in input_entries.iter().enumerate() {
        match input_entry_to_directive(input_entry) {
            Ok(directive) => {
                entries.push(directive_to_json(&directive, i as u32, "<created>"));
            }
            Err(e) => {
                errors.push(Error::new(format!("Entry {i}: {e}")).with_entry_index(i));
            }
        }
    }

    let output = CreateEntriesOutput {
        api_version: API_VERSION,
        entries,
        errors,
    };
    output_json(&output)
}
