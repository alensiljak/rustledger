//! Validation command.

use rustledger_validate::{ValidationOptions, validate_spanned_with_options};

use crate::helpers::load_source;
use crate::types::{Error, ValidateOutput};
use crate::{API_VERSION, output_json};

/// Validate beancount source from stdin.
pub fn cmd_validate(source: &str) -> i32 {
    let load = load_source(source);
    let mut errors = load.errors;

    // Run validation if parsing succeeded
    if errors.is_empty() {
        let validation_errors =
            validate_spanned_with_options(&load.spanned_directives, ValidationOptions::default());
        for err in validation_errors {
            // Convert span to line number if available
            let mut e = Error::new(&err.message);
            if let Some(span) = err.span {
                e = e.with_line(load.line_lookup.byte_to_line(span.start));
            }
            errors.push(e);
        }
    }

    let output = ValidateOutput {
        api_version: API_VERSION,
        valid: errors.is_empty(),
        errors,
    };
    output_json(&output)
}
