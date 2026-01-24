//! Document and note validation.

use rustledger_core::{Document, Note};
use std::path::Path;

use crate::LedgerState;
use crate::error::{ErrorCode, ValidationError};

/// Validate a Note directive.
///
/// Checks that the referenced account has been opened.
pub(crate) fn validate_note(state: &LedgerState, note: &Note, errors: &mut Vec<ValidationError>) {
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
pub(crate) fn validate_document(
    state: &LedgerState,
    doc: &Document,
    errors: &mut Vec<ValidationError>,
) {
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
        } else {
            doc_path.to_path_buf()
        };

        if !full_path.exists() {
            errors.push(
                ValidationError::new(
                    ErrorCode::DocumentNotFound,
                    format!("Document file not found: {}", doc.path),
                    doc.date,
                )
                .with_context(format!("resolved path: {}", full_path.display())),
            );
        }
    }
}
