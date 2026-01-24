//! Rustledger FFI via WASI - JSON API for embedding in any language.
//!
//! This is a WASI module that can be run via wasmtime (or any WASI runtime):
//!
//! ```bash
//! # Load (full directive output with metadata)
//! cat ledger.beancount | wasmtime rustledger-ffi-wasi.wasm load
//!
//! # Validate
//! cat ledger.beancount | wasmtime rustledger-ffi-wasi.wasm validate
//!
//! # Query
//! cat ledger.beancount | wasmtime rustledger-ffi-wasi.wasm query "SELECT account, sum(position) GROUP BY 1"
//! ```
//!
//! All output is JSON to stdout.

mod commands;
mod convert;
mod helpers;
mod types;

use std::fs;
use std::io::{self, Read, Write};

use serde::Serialize;

use types::Error;

// =============================================================================
// Constants and Exit Codes
// =============================================================================

/// API version for compatibility detection.
/// Increment minor version for backwards-compatible changes.
/// Increment major version for breaking changes.
pub(crate) const API_VERSION: &str = "1.0";

/// Exit codes for standardized error handling.
pub(crate) mod exit_codes {
    /// Success.
    pub const SUCCESS: i32 = 0;
    /// User error (invalid input, missing arguments, parse errors).
    pub const USER_ERROR: i32 = 1;
    /// Internal error (unexpected failures).
    #[allow(dead_code)]
    pub const INTERNAL_ERROR: i32 = 2;
}

/// Write JSON to stdout, handling broken pipe gracefully.
/// Returns the exit code to use.
pub(crate) fn output_json<T: Serialize>(value: &T) -> i32 {
    match serde_json::to_string(value) {
        Ok(json) => {
            // Use write! instead of println! to handle broken pipe
            if writeln!(io::stdout(), "{json}").is_err() {
                // Broken pipe is not an error - consumer closed early
                return exit_codes::SUCCESS;
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error serializing JSON: {e}");
            exit_codes::INTERNAL_ERROR
        }
    }
}

/// Parse JSON with better error messages, extracting line/column info.
pub(crate) fn parse_json_error(e: &serde_json::Error) -> Error {
    let mut err = Error::new(format!("JSON parse error: {e}"));
    // serde_json provides line/column for syntax errors
    if e.line() > 0 {
        err.line = Some(e.line() as u32);
        err.column = Some(e.column() as u32);
    }
    // Try to extract field name from error message
    let msg = e.to_string();
    if msg.contains("missing field") || msg.contains("unknown field") {
        if let Some(start) = msg.find('`') {
            if let Some(end) = msg[start + 1..].find('`') {
                err.field = Some(msg[start + 1..start + 1 + end].to_string());
            }
        }
    }
    err
}

// =============================================================================
// Main
// =============================================================================

/// Read source from stdin or file.
/// If `file_path` is Some, read from file; otherwise read from stdin.
fn read_source(file_path: Option<&str>) -> Result<String, String> {
    if let Some(path) = file_path {
        fs::read_to_string(path).map_err(|e| format!("Error reading file '{path}': {e}"))
    } else {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("Error reading stdin: {e}"))?;
        Ok(source)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        commands::util::cmd_help();
        std::process::exit(exit_codes::USER_ERROR);
    }

    let command = &args[1];

    let exit_code = match command.as_str() {
        "version" => commands::util::cmd_version(),
        "help" | "--help" | "-h" => {
            commands::util::cmd_help();
            exit_codes::SUCCESS
        }
        // File-based commands (for WASI environments where stdin doesn't work)
        "load-file" => {
            if args.len() < 3 {
                eprintln!("Error: load-file command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                let filename = &args[2];
                match read_source(Some(filename)) {
                    Ok(source) => commands::load::cmd_load(&source, filename),
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        "load-full" => {
            if args.len() < 3 {
                eprintln!("Error: load-full command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                let path = &args[2];
                // Remaining args are plugin names
                let plugins: Vec<&str> = args[3..].iter().map(String::as_str).collect();
                commands::load::cmd_load_full(path, &plugins)
            }
        }
        "validate-file" => {
            if args.len() < 3 {
                eprintln!("Error: validate-file command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                match read_source(Some(&args[2])) {
                    Ok(source) => commands::validate::cmd_validate(&source),
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        "query-file" => {
            if args.len() < 4 {
                eprintln!("Error: query-file command requires file path and BQL arguments");
                exit_codes::USER_ERROR
            } else {
                match read_source(Some(&args[2])) {
                    Ok(source) => commands::query::cmd_query(&source, &args[3]),
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        "batch-file" => {
            if args.len() < 4 {
                eprintln!("Error: batch-file command requires file path and at least one query");
                exit_codes::USER_ERROR
            } else {
                let filename = &args[2];
                let queries: Vec<String> = args.iter().skip(3).cloned().collect();
                match read_source(Some(filename)) {
                    Ok(source) => commands::query::cmd_batch(&source, filename, &queries),
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        "format-file" => {
            if args.len() < 3 {
                eprintln!("Error: format-file command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                match read_source(Some(&args[2])) {
                    Ok(source) => commands::format::cmd_format(&source),
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        "clamp-file" => {
            if args.len() < 3 {
                eprintln!("Error: clamp-file command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                let filename = &args[2];
                let begin_date = args.get(3).map(String::as_str);
                let end_date = args.get(4).map(String::as_str);
                match read_source(Some(filename)) {
                    Ok(source) => {
                        commands::clamp::cmd_clamp(&source, filename, begin_date, end_date)
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        exit_codes::USER_ERROR
                    }
                }
            }
        }
        // Utility commands (no stdin required)
        "is-encrypted" => {
            if args.len() < 3 {
                eprintln!("Error: is-encrypted command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                commands::util::cmd_is_encrypted(&args[2])
            }
        }
        "get-account-type" => {
            if args.len() < 3 {
                eprintln!("Error: get-account-type command requires account name argument");
                exit_codes::USER_ERROR
            } else {
                commands::util::cmd_get_account_type(&args[2])
            }
        }
        "types" => commands::util::cmd_types(),
        "schema" => commands::util::cmd_schema(),
        // Entry manipulation commands (read JSON from stdin)
        "format-entry" | "format-entries" | "create-entry" | "create-entries"
        | "filter-entries" | "clamp-entries" => {
            let json_str = match read_source(None) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(exit_codes::USER_ERROR);
                }
            };
            match command.as_str() {
                "format-entry" => commands::format::cmd_format_entry(&json_str),
                "format-entries" => commands::format::cmd_format_entries(&json_str),
                "create-entry" => commands::entry::cmd_create_entry(&json_str),
                "create-entries" => commands::entry::cmd_create_entries(&json_str),
                "filter-entries" => commands::clamp::cmd_filter_entries(&json_str),
                "clamp-entries" => commands::clamp::cmd_clamp_entries(&json_str),
                _ => unreachable!(),
            }
        }
        // Stdin-based commands (original behavior)
        "load" | "validate" | "query" | "batch" | "format" | "clamp" => {
            // Read source from stdin
            let source = match read_source(None) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(exit_codes::USER_ERROR);
                }
            };

            match command.as_str() {
                "load" => {
                    let filename = args.get(2).map_or("<stdin>", std::string::String::as_str);
                    commands::load::cmd_load(&source, filename)
                }
                "validate" => commands::validate::cmd_validate(&source),
                "query" => {
                    if args.len() < 3 {
                        eprintln!("Error: query command requires BQL argument");
                        exit_codes::USER_ERROR
                    } else {
                        commands::query::cmd_query(&source, &args[2])
                    }
                }
                "batch" => {
                    let filename = args.get(2).map_or("<stdin>", std::string::String::as_str);
                    let queries: Vec<String> = args.iter().skip(3).cloned().collect();
                    commands::query::cmd_batch(&source, filename, &queries)
                }
                "format" => commands::format::cmd_format(&source),
                "clamp" => {
                    let filename = args.get(2).map_or("<stdin>", std::string::String::as_str);
                    let begin_date = args.get(3).map(String::as_str);
                    let end_date = args.get(4).map(String::as_str);
                    commands::clamp::cmd_clamp(&source, filename, begin_date, end_date)
                }
                _ => unreachable!(),
            }
        }
        _ => {
            eprintln!("Unknown command: {command}");
            commands::util::cmd_help();
            exit_codes::USER_ERROR
        }
    };

    std::process::exit(exit_code);
}
