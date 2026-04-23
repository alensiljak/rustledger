//! Document and note validation.

use rustledger_core::{Document, Note};
use std::path::Path;

use crate::LedgerState;
use crate::error::{ErrorCode, ValidationError};

/// Validate a Note directive.
///
/// Checks that the referenced account has been opened.
pub fn validate_note(state: &LedgerState, note: &Note, errors: &mut Vec<ValidationError>) {
    // Check account exists
    if !state.accounts.contains_key(&note.account) {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!("Invalid reference to unknown account '{}'", note.account),
            note.date,
        ));
    }
}

/// Validate a Document directive.
pub fn validate_document(state: &LedgerState, doc: &Document, errors: &mut Vec<ValidationError>) {
    // Check account exists
    if !state.accounts.contains_key(&doc.account) {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!("Invalid reference to unknown account '{}'", doc.account),
            doc.date,
        ));
    }

    // Check if document file exists (if enabled)
    if state.options.check_documents {
        let doc_path = Path::new(&doc.path);

        let full_path = if doc_path.is_absolute() {
            doc_path.to_path_buf()
        } else if let Some(base) = &state.options.document_base {
            base.join(doc_path)
        } else if !state.options.document_dirs.is_empty() {
            // Try resolving relative path against each document directory
            let mut found = None;
            for dir in &state.options.document_dirs {
                let candidate = Path::new(dir).join(doc_path);
                if candidate.exists() {
                    found = Some(candidate);
                    break;
                }
            }
            match found {
                Some(p) => p,
                None => doc_path.to_path_buf(),
            }
        } else {
            doc_path.to_path_buf()
        };

        if !full_path.exists() {
            let mut error = ValidationError::new(
                ErrorCode::DocumentNotFound,
                format!("Document file not found: {}", doc.path),
                doc.date,
            );

            if doc_path.is_relative()
                && state.options.document_base.is_none()
                && !state.options.document_dirs.is_empty()
            {
                let searched: Vec<String> = state
                    .options
                    .document_dirs
                    .iter()
                    .map(|d| format!("{}/{}", d, doc.path))
                    .collect();
                error = error.with_context(format!("searched: {}", searched.join(", ")));
            } else {
                error = error.with_context(format!("resolved path: {}", full_path.display()));
            }

            errors.push(error);
        }
    }
}
