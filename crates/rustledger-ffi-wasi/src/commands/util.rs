//! Utility commands (version, help, is-encrypted, get-account-type, types, schema).

use std::fs;

use serde::Serialize;

use crate::types::VersionOutput;
use crate::{API_VERSION, output_json};

/// Output for is-encrypted command.
#[derive(Serialize)]
pub struct IsEncryptedOutput {
    pub api_version: &'static str,
    pub encrypted: bool,
    pub reason: Option<String>,
}

/// Output for get-account-type command.
#[derive(Serialize)]
pub struct AccountTypeOutput {
    pub api_version: &'static str,
    pub account: String,
    pub account_type: Option<String>,
}

/// Output for types command - exposes type constants.
#[derive(Serialize)]
pub struct TypesOutput {
    pub api_version: &'static str,
    /// All directive type names.
    pub all_directives: Vec<&'static str>,
    /// Booking method names.
    pub booking_methods: Vec<&'static str>,
    /// The MISSING sentinel description.
    pub missing: MissingSentinel,
    /// Default account type prefixes.
    pub account_types: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct MissingSentinel {
    pub description: &'static str,
    /// In JSON output, missing amounts appear as null or with `currency_only` field.
    pub json_representation: &'static str,
}

/// Show version.
pub fn cmd_version() -> i32 {
    let output = VersionOutput {
        api_version: API_VERSION,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    output_json(&output)
}

/// Show help.
pub fn cmd_help() {
    eprintln!("rustledger-ffi-wasi - Rustledger FFI via WASI (JSON API for embedding)");
    eprintln!();
    eprintln!("Usage: rustledger-ffi-wasi <command> [args...]");
    eprintln!();
    eprintln!("Commands (stdin-based):");
    eprintln!("  load [filename]      Load source from stdin, output entries + options + errors");
    eprintln!("  validate             Validate source from stdin");
    eprintln!("  query <bql>          Run BQL query on source from stdin");
    eprintln!("  batch [file] <bql>.. Load + run multiple queries in one parse (efficient)");
    eprintln!("  format               Format source from stdin back to beancount syntax");
    eprintln!("  clamp [file] [begin] [end]  Filter entries by date range");
    eprintln!();
    eprintln!("Commands (file-based, for WASI environments):");
    eprintln!("  load-file <path>          Load from file path");
    eprintln!("  load-full <path> [plugins..]  Full load: resolves includes, runs plugins");
    eprintln!("  validate-file <path>      Validate from file path");
    eprintln!("  query-file <path> <bql>   Query from file path");
    eprintln!("  batch-file <path> <bql>.. Batch queries from file path");
    eprintln!("  format-file <path>        Format file back to beancount syntax");
    eprintln!("  clamp-file <path> [begin] [end]  Filter entries by date range");
    eprintln!();
    eprintln!("Entry manipulation (stdin JSON):");
    eprintln!("  format-entry             Format single entry JSON to beancount text");
    eprintln!("  format-entries           Format array of entry JSON to beancount text");
    eprintln!("  create-entry             Create full entry with hash from minimal JSON");
    eprintln!("  create-entries           Create multiple entries from JSON array");
    eprintln!("  filter-entries           Filter entries by date range (avoids re-parsing)");
    eprintln!("  clamp-entries            Clamp entries with summarizations (avoids re-parsing)");
    eprintln!();
    eprintln!("Utility commands:");
    eprintln!("  is-encrypted <path>       Check if file is GPG-encrypted");
    eprintln!("  get-account-type <acct>   Extract account type from account name");
    eprintln!("  types                     Get type constants (ALL_DIRECTIVES, Booking, etc.)");
    eprintln!("  schema                    Get JSON Schema documentation for all types/commands");
    eprintln!();
    eprintln!("Other:");
    eprintln!("  version              Show version");
    eprintln!("  help                 Show this help");
    eprintln!();
    eprintln!("All output is JSON to stdout.");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  # Stdin-based (if stdin works in your environment):");
    eprintln!("  cat ledger.beancount | wasmtime rustledger-ffi-wasi.wasm load ledger.beancount");
    eprintln!("  cat ledger.beancount | wasmtime rustledger-ffi-wasi.wasm query \"BALANCES\"");
    eprintln!();
    eprintln!("  # File-based (recommended for WASI/wasmtime):");
    eprintln!("  wasmtime --dir=. rustledger-ffi-wasi.wasm load-file ledger.beancount");
    eprintln!(
        "  wasmtime --dir=. rustledger-ffi-wasi.wasm load-full ledger.beancount  # with includes"
    );
    eprintln!(
        "  wasmtime --dir=. rustledger-ffi-wasi.wasm load-full ledger.beancount auto_accounts"
    );
    eprintln!(
        "  wasmtime --dir=. rustledger-ffi-wasi.wasm query-file ledger.beancount \"JOURNAL\""
    );
    eprintln!(
        "  wasmtime --dir=. rustledger-ffi-wasi.wasm clamp-file ledger.beancount 2024-01-01 2024-12-31"
    );
    eprintln!();
    eprintln!("  # Utility commands:");
    eprintln!("  wasmtime --dir=. rustledger-ffi-wasi.wasm is-encrypted ledger.beancount.gpg");
    eprintln!("  rustledger-ffi-wasi get-account-type \"Assets:Bank:Checking\"");
    eprintln!("  rustledger-ffi-wasi types");
    eprintln!("  rustledger-ffi-wasi schema    # Get JSON Schema for all types");
}

/// Check if a file is GPG-encrypted.
pub fn cmd_is_encrypted(path: &str) -> i32 {
    // Check extension first (case-insensitive)
    let path_obj = std::path::Path::new(path);
    let has_gpg_ext = path_obj
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gpg") || ext.eq_ignore_ascii_case("asc"));

    let (encrypted, reason) = if has_gpg_ext {
        (true, Some("file extension".to_string()))
    } else {
        // Check for GPG header by reading first few bytes
        match fs::read(path) {
            Ok(bytes) => {
                // GPG binary format starts with 0x85 or 0x84 (old format) or 0xC0-0xCF (new format)
                // ASCII armored starts with "-----BEGIN PGP"
                if bytes.len() >= 15 {
                    let ascii_header = String::from_utf8_lossy(&bytes[..15]);
                    if ascii_header.starts_with("-----BEGIN PGP") {
                        (true, Some("ASCII armor header".to_string()))
                    } else if !bytes.is_empty() {
                        let first_byte = bytes[0];
                        if first_byte == 0x85 || first_byte == 0x84 || (0xC0..=0xCF).contains(&first_byte) {
                            (true, Some("binary GPG header".to_string()))
                        } else {
                            (false, None)
                        }
                    } else {
                        (false, None)
                    }
                } else {
                    (false, None)
                }
            }
            Err(e) => {
                let output = IsEncryptedOutput {
                    api_version: API_VERSION,
                    encrypted: false,
                    reason: Some(format!("Error reading file: {e}")),
                };
                return output_json(&output);
            }
        }
    };

    let output = IsEncryptedOutput {
        api_version: API_VERSION,
        encrypted,
        reason,
    };
    output_json(&output)
}

/// Extract account type (first component) from an account name.
pub fn cmd_get_account_type(account: &str) -> i32 {
    let account_type = account.split(':').next().map(String::from);
    let output = AccountTypeOutput {
        api_version: API_VERSION,
        account: account.to_string(),
        account_type,
    };
    output_json(&output)
}

/// Get type constants.
pub fn cmd_types() -> i32 {
    let output = TypesOutput {
        api_version: API_VERSION,
        all_directives: vec![
            "transaction",
            "balance",
            "open",
            "close",
            "commodity",
            "pad",
            "event",
            "query",
            "note",
            "document",
            "price",
            "custom",
        ],
        booking_methods: vec![
            "STRICT",
            "STRICT_WITH_SIZE",
            "FIFO",
            "LIFO",
            "HIFO",
            "AVERAGE",
            "NONE",
        ],
        missing: MissingSentinel {
            description: "MISSING represents an incomplete posting amount that will be interpolated",
            json_representation: "null or {\"currency_only\": \"USD\"}",
        },
        account_types: vec!["Assets", "Liabilities", "Equity", "Income", "Expenses"],
    };
    output_json(&output)
}

/// Get JSON Schema documentation for all types.
pub fn cmd_schema() -> i32 {
    let schema = serde_json::json!({
        "api_version": API_VERSION,
        "description": "JSON Schema documentation for rustledger-ffi-wasi commands",
        "schemas": {
            "Amount": {
                "type": "object",
                "required": ["number", "currency"],
                "properties": {
                    "number": {"type": "string", "description": "Decimal number as string (e.g., \"100.00\")"},
                    "currency": {"type": "string", "description": "Currency code (e.g., \"USD\")"}
                }
            },
            "Cost": {
                "type": "object",
                "properties": {
                    "number": {"type": "string", "description": "Per-unit cost number"},
                    "number_total": {"type": "string", "description": "Total cost number"},
                    "currency": {"type": "string", "description": "Cost currency"},
                    "date": {"type": "string", "format": "date", "description": "Lot date (YYYY-MM-DD)"},
                    "label": {"type": "string", "description": "Lot label"}
                }
            },
            "Posting": {
                "type": "object",
                "required": ["account"],
                "properties": {
                    "account": {"type": "string", "description": "Account name (e.g., \"Assets:Bank:Checking\")"},
                    "units": {"$ref": "#/schemas/Amount", "description": "Posted amount (optional for auto-balance)"},
                    "cost": {"$ref": "#/schemas/Cost", "description": "Cost basis"},
                    "price": {"$ref": "#/schemas/Amount", "description": "Price annotation"},
                    "meta": {"type": "object", "description": "Posting metadata"}
                }
            },
            "Error": {
                "type": "object",
                "required": ["message", "severity"],
                "properties": {
                    "message": {"type": "string"},
                    "line": {"type": "integer", "description": "Line number (1-based)"},
                    "column": {"type": "integer", "description": "Column number (1-based)"},
                    "field": {"type": "string", "description": "Field that caused the error"},
                    "entry_index": {"type": "integer", "description": "Index of entry in array (0-based)"},
                    "severity": {"type": "string", "enum": ["error", "warning"]}
                }
            }
        }
    });

    output_json(&schema)
}
