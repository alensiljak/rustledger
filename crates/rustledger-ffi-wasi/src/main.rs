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

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use rustledger_booking::interpolate;
use rustledger_core::{Cost, Directive, MetaValue, Metadata, NaiveDate, format::FormatConfig};
use rustledger_loader::Loader;
use rustledger_parser::{Spanned, parse as parse_beancount};
use rustledger_plugin::{
    NativePluginRegistry, PluginInput, PluginOptions, directive_to_wrapper, wrapper_to_directive,
};
use rustledger_query::{Executor, parse as parse_query};
use rustledger_validate::{ValidationOptions, validate_spanned_with_options};
use serde::{Deserialize, Serialize};

// =============================================================================
// Constants and Exit Codes
// =============================================================================

/// API version for compatibility detection.
/// Increment minor version for backwards-compatible changes.
/// Increment major version for breaking changes.
const API_VERSION: &str = "1.0";

/// Exit codes for standardized error handling.
mod exit_codes {
    /// Success.
    pub const SUCCESS: i32 = 0;
    /// User error (invalid input, missing arguments, parse errors).
    pub const USER_ERROR: i32 = 1;
    /// Internal error (unexpected failures).
    pub const INTERNAL_ERROR: i32 = 2;
}

/// Write JSON to stdout, handling broken pipe gracefully.
/// Returns the exit code to use.
fn output_json<T: Serialize>(value: &T) -> i32 {
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
fn parse_json_error(e: &serde_json::Error) -> Error {
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
// Output Types (JSON-serializable)
// =============================================================================

/// Metadata includes filename, lineno, hash, plus any user-defined key-value pairs.
#[derive(Serialize, Default)]
struct Meta {
    filename: String,
    lineno: u32,
    /// Entry hash (SHA256 of canonical representation).
    hash: String,
    #[serde(flatten)]
    user: HashMap<String, serde_json::Value>,
}

impl Meta {
    fn new(filename: &str, lineno: u32, hash: String, directive_meta: &Metadata) -> Self {
        let mut user = HashMap::new();
        for (key, value) in directive_meta {
            user.insert(key.clone(), meta_value_to_json(value));
        }
        Self {
            filename: filename.to_string(),
            lineno,
            hash,
            user,
        }
    }
}

/// Convert `MetaValue` to JSON, extracting raw values without extra formatting.
fn meta_value_to_json(value: &MetaValue) -> serde_json::Value {
    match value {
        MetaValue::String(s) => serde_json::Value::String(s.clone()),
        MetaValue::Account(a) => serde_json::Value::String(a.clone()),
        MetaValue::Currency(c) => serde_json::Value::String(c.clone()),
        MetaValue::Tag(t) => serde_json::Value::String(t.clone()),
        MetaValue::Link(l) => serde_json::Value::String(l.clone()),
        MetaValue::Date(d) => serde_json::Value::String(d.to_string()),
        MetaValue::Number(n) => serde_json::json!(n.to_string()),
        MetaValue::Bool(b) => serde_json::Value::Bool(*b),
        MetaValue::Amount(a) => serde_json::json!({
            "number": a.number.to_string(),
            "currency": a.currency.to_string()
        }),
        MetaValue::None => serde_json::Value::Null,
    }
}

#[derive(Serialize, Clone)]
struct Error {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_index: Option<usize>,
    severity: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
            field: None,
            entry_index: None,
            severity: "error".to_string(),
        }
    }

    const fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    #[allow(dead_code)]
    fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    const fn with_entry_index(mut self, index: usize) -> Self {
        self.entry_index = Some(index);
        self
    }

    #[allow(dead_code)]
    fn warning(mut self) -> Self {
        self.severity = "warning".to_string();
        self
    }
}

#[derive(Serialize, Clone)]
struct Amount {
    number: String,
    currency: String,
}

#[derive(Serialize)]
struct PostingCost {
    /// Per-unit cost (e.g., {100 USD})
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<String>,
    /// Total cost (e.g., {{1000 USD}})
    #[serde(skip_serializing_if = "Option::is_none")]
    number_total: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// A typed value preserving the original type from the beancount source.
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
struct TypedValue {
    #[serde(rename = "type")]
    value_type: &'static str,
    value: serde_json::Value,
}

impl TypedValue {
    fn from_meta_value(mv: &MetaValue) -> Self {
        match mv {
            MetaValue::String(s) => Self {
                value_type: "string",
                value: serde_json::Value::String(s.clone()),
            },
            MetaValue::Account(a) => Self {
                value_type: "account",
                value: serde_json::Value::String(a.clone()),
            },
            MetaValue::Currency(c) => Self {
                value_type: "currency",
                value: serde_json::Value::String(c.clone()),
            },
            MetaValue::Tag(t) => Self {
                value_type: "tag",
                value: serde_json::Value::String(t.clone()),
            },
            MetaValue::Link(l) => Self {
                value_type: "link",
                value: serde_json::Value::String(l.clone()),
            },
            MetaValue::Date(d) => Self {
                value_type: "date",
                value: serde_json::Value::String(d.to_string()),
            },
            MetaValue::Number(n) => Self {
                value_type: "number",
                value: serde_json::Value::String(n.to_string()),
            },
            MetaValue::Bool(b) => Self {
                value_type: "bool",
                value: serde_json::Value::Bool(*b),
            },
            MetaValue::Amount(a) => Self {
                value_type: "amount",
                value: serde_json::json!({
                    "number": a.number.to_string(),
                    "currency": a.currency.to_string()
                }),
            },
            MetaValue::None => Self {
                value_type: "null",
                value: serde_json::Value::Null,
            },
        }
    }
}

#[derive(Serialize)]
struct Posting {
    account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    units: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<PostingCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Amount>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    meta: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DirectiveJson {
    Transaction {
        date: String,
        flag: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payee: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
        tags: Vec<String>,
        links: Vec<String>,
        postings: Vec<Posting>,
        meta: Meta,
    },
    Open {
        date: String,
        account: String,
        currencies: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        booking: Option<String>,
        meta: Meta,
    },
    Close {
        date: String,
        account: String,
        meta: Meta,
    },
    Balance {
        date: String,
        account: String,
        amount: Amount,
        meta: Meta,
    },
    Pad {
        date: String,
        account: String,
        source_account: String,
        meta: Meta,
    },
    Commodity {
        date: String,
        currency: String,
        meta: Meta,
    },
    Price {
        date: String,
        currency: String,
        amount: Amount,
        meta: Meta,
    },
    Event {
        date: String,
        event_type: String,
        value: String,
        meta: Meta,
    },
    Note {
        date: String,
        account: String,
        comment: String,
        meta: Meta,
    },
    Document {
        date: String,
        account: String,
        path: String,
        meta: Meta,
    },
    Query {
        date: String,
        name: String,
        query_string: String,
        meta: Meta,
    },
    Custom {
        date: String,
        custom_type: String,
        values: Vec<TypedValue>,
        meta: Meta,
    },
}

#[derive(Serialize)]
struct LedgerOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    operating_currency: Vec<String>,
    name_assets: String,
    name_liabilities: String,
    name_equity: String,
    name_income: String,
    name_expenses: String,
    documents: Vec<String>,
    commodities: Vec<String>,
    booking_method: String,
    display_precision: HashMap<String, u32>,
}

impl Default for LedgerOptions {
    fn default() -> Self {
        Self {
            title: None,
            operating_currency: Vec::new(),
            name_assets: "Assets".to_string(),
            name_liabilities: "Liabilities".to_string(),
            name_equity: "Equity".to_string(),
            name_income: "Income".to_string(),
            name_expenses: "Expenses".to_string(),
            documents: Vec::new(),
            commodities: Vec::new(),
            booking_method: "STRICT".to_string(),
            display_precision: HashMap::new(),
        }
    }
}

/// A plugin directive from the source file.
#[derive(Serialize)]
struct Plugin {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<String>,
}

/// An include directive from the source file.
#[derive(Serialize)]
struct Include {
    path: String,
    lineno: u32,
}

#[derive(Serialize)]
struct LoadOutput {
    api_version: &'static str,
    entries: Vec<DirectiveJson>,
    errors: Vec<Error>,
    options: LedgerOptions,
    plugins: Vec<Plugin>,
    includes: Vec<Include>,
}

#[derive(Serialize)]
struct ValidateOutput {
    api_version: &'static str,
    valid: bool,
    errors: Vec<Error>,
}

#[derive(Serialize)]
struct ColumnInfo {
    name: String,
    datatype: String,
}

#[derive(Serialize)]
struct QueryOutput {
    api_version: &'static str,
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<serde_json::Value>>,
    errors: Vec<Error>,
}

#[derive(Serialize)]
struct VersionOutput {
    api_version: &'static str,
    version: String,
}

/// Output for batch command: load + multiple queries in one parse.
#[derive(Serialize)]
struct BatchOutput {
    api_version: &'static str,
    load: LoadOutput,
    queries: Vec<QueryOutput>,
}

// =============================================================================
// Input Types (JSON-deserializable for create-entry/format-entry)
// =============================================================================

/// Input amount for entry creation.
#[derive(Deserialize, Clone)]
struct InputAmount {
    number: String,
    currency: String,
}

/// Input cost for entry creation.
#[derive(Deserialize, Clone, Default)]
struct InputCost {
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    number_total: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

/// Input posting for entry creation.
#[derive(Deserialize, Clone)]
struct InputPosting {
    account: String,
    #[serde(default)]
    units: Option<InputAmount>,
    #[serde(default)]
    cost: Option<InputCost>,
    #[serde(default)]
    price: Option<InputAmount>,
    #[serde(default)]
    meta: HashMap<String, serde_json::Value>,
}

/// Input entry for create-entry/format-entry commands.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputEntry {
    Transaction {
        date: String,
        #[serde(default = "default_flag")]
        flag: String,
        #[serde(default)]
        payee: Option<String>,
        #[serde(default)]
        narration: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        links: Vec<String>,
        #[serde(default)]
        postings: Vec<InputPosting>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Open {
        date: String,
        account: String,
        #[serde(default)]
        currencies: Vec<String>,
        #[serde(default)]
        booking: Option<String>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Close {
        date: String,
        account: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Balance {
        date: String,
        account: String,
        amount: InputAmount,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Pad {
        date: String,
        account: String,
        source_account: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Commodity {
        date: String,
        currency: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Price {
        date: String,
        currency: String,
        amount: InputAmount,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Event {
        date: String,
        event_type: String,
        value: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Note {
        date: String,
        account: String,
        comment: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Document {
        date: String,
        account: String,
        path: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Query {
        date: String,
        name: String,
        query_string: String,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
    Custom {
        date: String,
        custom_type: String,
        #[serde(default)]
        values: Vec<serde_json::Value>,
        #[serde(default)]
        meta: HashMap<String, serde_json::Value>,
    },
}

fn default_flag() -> String {
    "*".to_string()
}

/// Convert JSON metadata value to core `MetaValue`.
fn json_to_meta_value(value: &serde_json::Value) -> MetaValue {
    match value {
        serde_json::Value::String(s) => MetaValue::String(s.clone()),
        serde_json::Value::Bool(b) => MetaValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                MetaValue::Number(rustledger_core::Decimal::from(i))
            } else if let Some(f) = n.as_f64() {
                MetaValue::Number(
                    rustledger_core::Decimal::from_str_exact(&f.to_string())
                        .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                )
            } else {
                MetaValue::None
            }
        }
        serde_json::Value::Null => MetaValue::None,
        serde_json::Value::Object(obj) => {
            // Handle Amount objects
            if let (Some(number), Some(currency)) = (obj.get("number"), obj.get("currency")) {
                if let (Some(n), Some(c)) = (number.as_str(), currency.as_str()) {
                    return MetaValue::Amount(rustledger_core::Amount {
                        number: rustledger_core::Decimal::from_str_exact(n)
                            .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                        currency: c.into(),
                    });
                }
            }
            MetaValue::None
        }
        serde_json::Value::Array(_) => MetaValue::None,
    }
}

/// Convert `HashMap<String, Value>` to core Metadata.
fn json_map_to_metadata(map: &HashMap<String, serde_json::Value>) -> Metadata {
    map.iter()
        .map(|(k, v)| (k.clone(), json_to_meta_value(v)))
        .collect()
}

/// Convert `InputEntry` to core Directive.
fn input_entry_to_directive(entry: &InputEntry) -> Result<Directive, String> {
    match entry {
        InputEntry::Transaction {
            date,
            flag,
            payee,
            narration,
            tags,
            links,
            postings,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;

            let flag = match flag.as_str() {
                "*" | "txn" => '*',
                "!" => '!',
                other => other.chars().next().unwrap_or('*'),
            };

            let postings: Vec<rustledger_core::Posting> = postings
                .iter()
                .map(|p| {
                    let units = p.units.as_ref().map(|u| {
                        rustledger_core::IncompleteAmount::Complete(rustledger_core::Amount {
                            number: rustledger_core::Decimal::from_str_exact(&u.number)
                                .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                            currency: u.currency.clone().into(),
                        })
                    });

                    let cost = p.cost.as_ref().map(|c| rustledger_core::CostSpec {
                        number_per: c
                            .number
                            .as_ref()
                            .and_then(|n| rustledger_core::Decimal::from_str_exact(n).ok()),
                        number_total: c
                            .number_total
                            .as_ref()
                            .and_then(|n| rustledger_core::Decimal::from_str_exact(n).ok()),
                        currency: c.currency.clone().map(Into::into),
                        date: c
                            .date
                            .as_ref()
                            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
                        label: c.label.clone(),
                        merge: false,
                    });

                    let price = p.price.as_ref().map(|pr| {
                        rustledger_core::PriceAnnotation::Unit(rustledger_core::Amount {
                            number: rustledger_core::Decimal::from_str_exact(&pr.number)
                                .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                            currency: pr.currency.clone().into(),
                        })
                    });

                    rustledger_core::Posting {
                        account: p.account.clone().into(),
                        units,
                        cost,
                        price,
                        flag: None,
                        meta: json_map_to_metadata(&p.meta),
                    }
                })
                .collect();

            Ok(Directive::Transaction(rustledger_core::Transaction {
                date,
                flag,
                payee: payee.clone().map(Into::into),
                narration: narration.clone().unwrap_or_default().into(),
                tags: tags.iter().map(|t| t.clone().into()).collect(),
                links: links.iter().map(|l| l.clone().into()).collect(),
                postings,
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Open {
            date,
            account,
            currencies,
            booking,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Open(rustledger_core::Open {
                date,
                account: account.clone().into(),
                currencies: currencies.iter().map(|c| c.clone().into()).collect(),
                booking: booking.clone(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Close {
            date,
            account,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Close(rustledger_core::Close {
                date,
                account: account.clone().into(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Balance {
            date,
            account,
            amount,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Balance(rustledger_core::Balance {
                date,
                account: account.clone().into(),
                amount: rustledger_core::Amount {
                    number: rustledger_core::Decimal::from_str_exact(&amount.number)
                        .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                    currency: amount.currency.clone().into(),
                },
                tolerance: None,
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Pad {
            date,
            account,
            source_account,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Pad(rustledger_core::Pad {
                date,
                account: account.clone().into(),
                source_account: source_account.clone().into(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Commodity {
            date,
            currency,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Commodity(rustledger_core::Commodity {
                date,
                currency: currency.clone().into(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Price {
            date,
            currency,
            amount,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Price(rustledger_core::Price {
                date,
                currency: currency.clone().into(),
                amount: rustledger_core::Amount {
                    number: rustledger_core::Decimal::from_str_exact(&amount.number)
                        .unwrap_or_else(|_| rustledger_core::Decimal::from(0)),
                    currency: amount.currency.clone().into(),
                },
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Event {
            date,
            event_type,
            value,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Event(rustledger_core::Event {
                date,
                event_type: event_type.clone(),
                value: value.clone(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Note {
            date,
            account,
            comment,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Note(rustledger_core::Note {
                date,
                account: account.clone().into(),
                comment: comment.clone(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Document {
            date,
            account,
            path,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Document(rustledger_core::Document {
                date,
                account: account.clone().into(),
                path: path.clone(),
                tags: Vec::new(),
                links: Vec::new(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Query {
            date,
            name,
            query_string,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Query(rustledger_core::Query {
                date,
                name: name.clone(),
                query: query_string.clone(),
                meta: json_map_to_metadata(meta),
            }))
        }
        InputEntry::Custom {
            date,
            custom_type,
            values,
            meta,
        } => {
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date '{date}': {e}"))?;
            Ok(Directive::Custom(rustledger_core::Custom {
                date,
                custom_type: custom_type.clone(),
                values: values.iter().map(json_to_meta_value).collect(),
                meta: json_map_to_metadata(meta),
            }))
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Simple line lookup for byte offset to line number conversion.
struct LineLookup {
    line_starts: Vec<usize>,
}

impl LineLookup {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    fn byte_to_line(&self, byte_offset: usize) -> u32 {
        match self.line_starts.binary_search(&byte_offset) {
            Ok(line) => line as u32 + 1,
            Err(line) => line as u32,
        }
    }
}

/// Track precision per currency: maps currency -> (`precision_counts` map)
struct PrecisionTracker {
    counts: HashMap<String, HashMap<u32, u32>>,
}

impl PrecisionTracker {
    fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    fn observe(&mut self, currency: &str, number: rustledger_core::Decimal) {
        let precision = number.scale();
        let currency_counts = self.counts.entry(currency.to_string()).or_default();
        *currency_counts.entry(precision).or_insert(0) += 1;
    }

    fn most_common_precision(&self) -> HashMap<String, u32> {
        self.counts
            .iter()
            .map(|(currency, counts)| {
                let precision = counts
                    .iter()
                    .max_by_key(|(_, count)| *count)
                    .map_or(2, |(prec, _)| *prec);
                (currency.clone(), precision)
            })
            .collect()
    }
}

/// Internal load result with all parsed data.
struct LoadResult {
    directives: Vec<Directive>,
    spanned_directives: Vec<Spanned<Directive>>,
    directive_lines: Vec<u32>,
    line_lookup: LineLookup,
    errors: Vec<Error>,
    options: LedgerOptions,
    plugins: Vec<Plugin>,
    includes: Vec<Include>,
}

/// Parse and interpolate source, returning directives with line numbers.
fn load_source(source: &str) -> LoadResult {
    let parse_result = parse_beancount(source);
    let lookup = LineLookup::new(source);

    let mut errors: Vec<Error> = parse_result
        .errors
        .iter()
        .map(|e| Error::new(e.to_string()).with_line(lookup.byte_to_line(e.span().0)))
        .collect();

    // Extract options
    let mut options = LedgerOptions::default();
    for (key, value, _span) in &parse_result.options {
        match key.as_str() {
            "title" => options.title = Some(value.clone()),
            "operating_currency" => options.operating_currency.push(value.clone()),
            "name_assets" => options.name_assets.clone_from(value),
            "name_liabilities" => options.name_liabilities.clone_from(value),
            "name_equity" => options.name_equity.clone_from(value),
            "name_income" => options.name_income.clone_from(value),
            "name_expenses" => options.name_expenses.clone_from(value),
            "documents" => options.documents.push(value.clone()),
            "booking_method" => options.booking_method.clone_from(value),
            _ => {}
        }
    }

    // Collect directive line numbers, commodities, and precision
    let mut directive_lines: Vec<u32> = Vec::new();
    let mut commodities: HashSet<String> = HashSet::new();
    let mut precision_tracker = PrecisionTracker::new();

    let mut directives: Vec<Directive> = Vec::new();
    for spanned in &parse_result.directives {
        let line = lookup.byte_to_line(spanned.span.start);
        directive_lines.push(line);

        // Collect commodities and track precision
        match &spanned.value {
            Directive::Open(o) => {
                for c in &o.currencies {
                    commodities.insert(c.to_string());
                }
            }
            Directive::Commodity(c) => {
                commodities.insert(c.currency.to_string());
            }
            Directive::Transaction(t) => {
                for p in &t.postings {
                    if let Some(units) = &p.units {
                        if let Some(amt) = units.as_amount() {
                            commodities.insert(amt.currency.to_string());
                            precision_tracker.observe(amt.currency.as_ref(), amt.number);
                        }
                    }
                    if let Some(price) = &p.price {
                        if let Some(amt) = price.amount() {
                            commodities.insert(amt.currency.to_string());
                            precision_tracker.observe(amt.currency.as_ref(), amt.number);
                        }
                    }
                }
            }
            Directive::Balance(b) => {
                commodities.insert(b.amount.currency.to_string());
                precision_tracker.observe(b.amount.currency.as_ref(), b.amount.number);
            }
            Directive::Price(p) => {
                commodities.insert(p.currency.to_string());
                commodities.insert(p.amount.currency.to_string());
                precision_tracker.observe(p.amount.currency.as_ref(), p.amount.number);
            }
            _ => {}
        }

        directives.push(spanned.value.clone());
    }

    // Interpolate transactions
    if errors.is_empty() {
        for (i, directive) in directives.iter_mut().enumerate() {
            if let Directive::Transaction(txn) = directive {
                match interpolate(txn) {
                    Ok(result) => {
                        *txn = result.transaction;
                    }
                    Err(e) => {
                        errors.push(Error::new(e.to_string()).with_line(directive_lines[i]));
                    }
                }
            }
        }
    }

    let mut commodity_list: Vec<_> = commodities.into_iter().collect();
    commodity_list.sort();
    options.commodities = commodity_list;
    options.display_precision = precision_tracker.most_common_precision();

    // Extract plugins
    let plugins: Vec<Plugin> = parse_result
        .plugins
        .iter()
        .map(|(name, config, _span)| Plugin {
            name: name.clone(),
            config: config.clone(),
        })
        .collect();

    // Extract includes
    let includes: Vec<Include> = parse_result
        .includes
        .iter()
        .map(|(path, span)| Include {
            path: path.clone(),
            lineno: lookup.byte_to_line(span.start),
        })
        .collect();

    // Clone spanned directives for validation
    let spanned_directives: Vec<Spanned<Directive>> = parse_result.directives.clone();

    LoadResult {
        directives,
        spanned_directives,
        directive_lines,
        line_lookup: lookup,
        errors,
        options,
        plugins,
        includes,
    }
}

/// Compute a SHA256 hash of a directive for unique identification.
fn compute_directive_hash(directive: &Directive) -> String {
    let mut hasher = Sha256::new();

    // Hash the directive type and core content
    match directive {
        Directive::Transaction(t) => {
            hasher.update(b"Transaction");
            hasher.update(t.date.to_string().as_bytes());
            hasher.update(t.flag.to_string().as_bytes());
            if let Some(ref payee) = t.payee {
                hasher.update(payee.as_bytes());
            }
            hasher.update(t.narration.as_bytes());
            for tag in &t.tags {
                hasher.update(tag.as_bytes());
            }
            for link in &t.links {
                hasher.update(link.as_bytes());
            }
            for posting in &t.postings {
                hasher.update(posting.account.as_bytes());
                if let Some(ref units) = posting.units {
                    if let Some(num) = units.number() {
                        write!(&mut hasher, "{num}").ok();
                    }
                    if let Some(cur) = units.currency() {
                        hasher.update(cur.as_bytes());
                    }
                }
            }
        }
        Directive::Open(o) => {
            hasher.update(b"Open");
            hasher.update(o.date.to_string().as_bytes());
            hasher.update(o.account.as_bytes());
            for c in &o.currencies {
                hasher.update(c.as_bytes());
            }
        }
        Directive::Close(c) => {
            hasher.update(b"Close");
            hasher.update(c.date.to_string().as_bytes());
            hasher.update(c.account.as_bytes());
        }
        Directive::Balance(b) => {
            hasher.update(b"Balance");
            hasher.update(b.date.to_string().as_bytes());
            hasher.update(b.account.as_bytes());
            write!(&mut hasher, "{}", b.amount.number).ok();
            hasher.update(b.amount.currency.as_bytes());
        }
        Directive::Pad(p) => {
            hasher.update(b"Pad");
            hasher.update(p.date.to_string().as_bytes());
            hasher.update(p.account.as_bytes());
            hasher.update(p.source_account.as_bytes());
        }
        Directive::Commodity(c) => {
            hasher.update(b"Commodity");
            hasher.update(c.date.to_string().as_bytes());
            hasher.update(c.currency.as_bytes());
        }
        Directive::Price(p) => {
            hasher.update(b"Price");
            hasher.update(p.date.to_string().as_bytes());
            hasher.update(p.currency.as_bytes());
            write!(&mut hasher, "{}", p.amount.number).ok();
            hasher.update(p.amount.currency.as_bytes());
        }
        Directive::Event(e) => {
            hasher.update(b"Event");
            hasher.update(e.date.to_string().as_bytes());
            hasher.update(e.event_type.as_bytes());
            hasher.update(e.value.as_bytes());
        }
        Directive::Note(n) => {
            hasher.update(b"Note");
            hasher.update(n.date.to_string().as_bytes());
            hasher.update(n.account.as_bytes());
            hasher.update(n.comment.as_bytes());
        }
        Directive::Document(d) => {
            hasher.update(b"Document");
            hasher.update(d.date.to_string().as_bytes());
            hasher.update(d.account.as_bytes());
            hasher.update(d.path.as_bytes());
        }
        Directive::Query(q) => {
            hasher.update(b"Query");
            hasher.update(q.date.to_string().as_bytes());
            hasher.update(q.name.as_bytes());
            hasher.update(q.query.as_bytes());
        }
        Directive::Custom(c) => {
            hasher.update(b"Custom");
            hasher.update(c.date.to_string().as_bytes());
            hasher.update(c.custom_type.as_bytes());
        }
    }

    let result = hasher.finalize();
    format!("{result:x}")
}

/// Convert core directive to JSON output format.
fn directive_to_json(directive: &Directive, line: u32, filename: &str) -> DirectiveJson {
    let hash = compute_directive_hash(directive);

    match directive {
        Directive::Transaction(t) => {
            let meta = Meta::new(filename, line, hash, &t.meta);
            DirectiveJson::Transaction {
                date: t.date.to_string(),
                flag: t.flag.to_string(),
                payee: t.payee.as_ref().map(std::string::ToString::to_string),
                narration: if t.narration.is_empty() {
                    None
                } else {
                    Some(t.narration.to_string())
                },
                tags: t
                    .tags
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                links: t
                    .links
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                postings: t
                    .postings
                    .iter()
                    .map(|p| {
                        // Extract amount from IncompleteAmount
                        let units = p.units.as_ref().and_then(|u| {
                            u.as_amount().map(|a| Amount {
                                number: a.number.to_string(),
                                currency: a.currency.to_string(),
                            })
                        });

                        // Extract cost
                        let cost = p.cost.as_ref().map(|c| PostingCost {
                            number: c.number_per.as_ref().map(std::string::ToString::to_string),
                            number_total: c
                                .number_total
                                .as_ref()
                                .map(std::string::ToString::to_string),
                            currency: c.currency.as_ref().map(std::string::ToString::to_string),
                            date: c.date.map(|d| d.to_string()),
                            label: c.label.clone(),
                        });

                        // Extract price from PriceAnnotation
                        let price = p.price.as_ref().and_then(|pr| {
                            pr.amount().map(|a| Amount {
                                number: a.number.to_string(),
                                currency: a.currency.to_string(),
                            })
                        });

                        // Posting metadata
                        let mut posting_meta = HashMap::new();
                        for (key, value) in &p.meta {
                            posting_meta.insert(key.clone(), meta_value_to_json(value));
                        }

                        Posting {
                            account: p.account.to_string(),
                            units,
                            cost,
                            price,
                            meta: posting_meta,
                        }
                    })
                    .collect(),
                meta,
            }
        }
        Directive::Open(o) => {
            let meta = Meta::new(filename, line, hash, &o.meta);
            DirectiveJson::Open {
                date: o.date.to_string(),
                account: o.account.to_string(),
                currencies: o
                    .currencies
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                booking: o.booking.clone(),
                meta,
            }
        }
        Directive::Close(c) => {
            let meta = Meta::new(filename, line, hash, &c.meta);
            DirectiveJson::Close {
                date: c.date.to_string(),
                account: c.account.to_string(),
                meta,
            }
        }
        Directive::Balance(b) => {
            let meta = Meta::new(filename, line, hash, &b.meta);
            DirectiveJson::Balance {
                date: b.date.to_string(),
                account: b.account.to_string(),
                amount: Amount {
                    number: b.amount.number.to_string(),
                    currency: b.amount.currency.to_string(),
                },
                meta,
            }
        }
        Directive::Pad(p) => {
            let meta = Meta::new(filename, line, hash, &p.meta);
            DirectiveJson::Pad {
                date: p.date.to_string(),
                account: p.account.to_string(),
                source_account: p.source_account.to_string(),
                meta,
            }
        }
        Directive::Commodity(c) => {
            let meta = Meta::new(filename, line, hash, &c.meta);
            DirectiveJson::Commodity {
                date: c.date.to_string(),
                currency: c.currency.to_string(),
                meta,
            }
        }
        Directive::Price(p) => {
            let meta = Meta::new(filename, line, hash, &p.meta);
            DirectiveJson::Price {
                date: p.date.to_string(),
                currency: p.currency.to_string(),
                amount: Amount {
                    number: p.amount.number.to_string(),
                    currency: p.amount.currency.to_string(),
                },
                meta,
            }
        }
        Directive::Event(e) => {
            let meta = Meta::new(filename, line, hash, &e.meta);
            DirectiveJson::Event {
                date: e.date.to_string(),
                event_type: e.event_type.clone(),
                value: e.value.clone(),
                meta,
            }
        }
        Directive::Note(n) => {
            let meta = Meta::new(filename, line, hash, &n.meta);
            DirectiveJson::Note {
                date: n.date.to_string(),
                account: n.account.to_string(),
                comment: n.comment.clone(),
                meta,
            }
        }
        Directive::Document(d) => {
            let meta = Meta::new(filename, line, hash, &d.meta);
            DirectiveJson::Document {
                date: d.date.to_string(),
                account: d.account.to_string(),
                path: d.path.clone(),
                meta,
            }
        }
        Directive::Query(q) => {
            let meta = Meta::new(filename, line, hash, &q.meta);
            DirectiveJson::Query {
                date: q.date.to_string(),
                name: q.name.clone(),
                query_string: q.query.clone(),
                meta,
            }
        }
        Directive::Custom(c) => {
            let meta = Meta::new(filename, line, hash, &c.meta);
            DirectiveJson::Custom {
                date: c.date.to_string(),
                custom_type: c.custom_type.clone(),
                values: c.values.iter().map(TypedValue::from_meta_value).collect(),
                meta,
            }
        }
    }
}

/// Convert query Value to JSON.
fn value_to_json(value: &rustledger_query::Value) -> serde_json::Value {
    use rustledger_query::Value;
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Integer(i) => serde_json::json!(i),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Date(d) => serde_json::Value::String(d.to_string()),
        Value::Number(d) => serde_json::json!({"number": d.to_string()}),
        Value::Amount(a) => serde_json::json!({
            "number": a.number.to_string(),
            "currency": a.currency
        }),
        Value::Position(p) => serde_json::json!({
            "units": {
                "number": p.units.number.to_string(),
                "currency": p.units.currency
            }
        }),
        Value::Inventory(inv) => {
            let positions: Vec<_> = inv
                .positions()
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "units": {
                            "number": p.units.number.to_string(),
                            "currency": p.units.currency
                        }
                    })
                })
                .collect();
            serde_json::json!({ "positions": positions })
        }
        Value::StringSet(set) => {
            serde_json::json!(set)
        }
        Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

/// Get datatype string for a Value.
const fn value_datatype(value: &rustledger_query::Value) -> &'static str {
    use rustledger_query::Value;
    match value {
        Value::Null => "null",
        Value::Boolean(_) => "bool",
        Value::Integer(_) => "int",
        Value::String(_) => "str",
        Value::Date(_) => "date",
        Value::Number(_) => "Decimal",
        Value::Amount(_) => "Amount",
        Value::Position(_) => "Position",
        Value::Inventory(_) => "Inventory",
        Value::StringSet(_) => "set",
        Value::Object(_) => "object",
    }
}

// =============================================================================
// Commands
// =============================================================================

fn cmd_load(source: &str, filename: &str) -> i32 {
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

/// Output for load-full command - includes resolved includes and plugin list.
#[derive(Serialize)]
struct LoadFullOutput {
    api_version: &'static str,
    entries: Vec<DirectiveJson>,
    errors: Vec<Error>,
    options: LedgerOptions,
    /// Resolved plugins (from file + `auto_accounts` if enabled).
    plugins: Vec<Plugin>,
    /// Files that were loaded (resolved includes).
    loaded_files: Vec<String>,
}

/// Load a beancount file using the full loader pipeline.
/// This handles:
/// - Include resolution (with cycle detection)
/// - Path security (prevents path traversal)
/// - GPG decryption (for .gpg/.asc files)
/// - Optional plugin execution (`auto_accounts` sorts entries)
fn cmd_load_full(path: &str, run_plugins: &[&str]) -> i32 {
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

    // Run interpolation on transactions
    for (i, directive) in directives.iter_mut().enumerate() {
        if let Directive::Transaction(txn) = directive {
            match interpolate(txn) {
                Ok(result) => {
                    *txn = result.transaction;
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
                        // Preserve the source location through the plugin
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

                // Convert wrappers back to directives, extracting source locations
                // Directives created by plugins will have filename=None, lineno=None
                let mut new_directives = Vec::new();
                let mut new_lines = Vec::new();
                let mut new_files = Vec::new();

                for wrapper in &output.directives {
                    if let Ok(directive) = wrapper_to_directive(wrapper) {
                        new_directives.push(directive);
                        // Use preserved location, or "<plugin>" for plugin-generated entries
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
        display_precision: HashMap::new(), // TODO: could add from display_context
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

fn cmd_validate(source: &str) -> i32 {
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

/// Execute a single query on directives, returning `QueryOutput`.
fn execute_query(directives: &[Directive], query_str: &str) -> QueryOutput {
    // Parse query
    let query = match parse_query(query_str) {
        Ok(q) => q,
        Err(e) => {
            return QueryOutput {
                api_version: API_VERSION,
                columns: vec![],
                rows: vec![],
                errors: vec![Error::new(format!("Query parse error: {e}"))],
            };
        }
    };

    // Execute
    let mut executor = Executor::new(directives);
    match executor.execute(&query) {
        Ok(result) => {
            // Infer column types from first row
            let columns: Vec<ColumnInfo> = if result.rows.is_empty() {
                result
                    .columns
                    .iter()
                    .map(|name| ColumnInfo {
                        name: name.clone(),
                        datatype: "str".to_string(), // Default if no rows
                    })
                    .collect()
            } else {
                result
                    .columns
                    .iter()
                    .zip(result.rows[0].iter())
                    .map(|(name, value)| ColumnInfo {
                        name: name.clone(),
                        datatype: value_datatype(value).to_string(),
                    })
                    .collect()
            };

            let rows: Vec<Vec<_>> = result
                .rows
                .iter()
                .map(|row| row.iter().map(value_to_json).collect())
                .collect();

            QueryOutput {
                api_version: API_VERSION,
                columns,
                rows,
                errors: vec![],
            }
        }
        Err(e) => QueryOutput {
            api_version: API_VERSION,
            columns: vec![],
            rows: vec![],
            errors: vec![Error::new(format!("Query error: {e}"))],
        },
    }
}

fn cmd_query(source: &str, query_str: &str) -> i32 {
    let load = load_source(source);

    if !load.errors.is_empty() {
        let output = QueryOutput {
            api_version: API_VERSION,
            columns: vec![],
            rows: vec![],
            errors: load.errors,
        };
        return output_json(&output);
    }

    let output = execute_query(&load.directives, query_str);
    output_json(&output)
}

/// Batch command: load + multiple queries in one parse.
/// Usage: batch [filename] query1 query2 ...
fn cmd_batch(source: &str, filename: &str, queries: &[String]) -> i32 {
    let load = load_source(source);

    // Build load output
    let entries: Vec<DirectiveJson> = load
        .directives
        .iter()
        .zip(load.directive_lines.iter())
        .map(|(d, &line)| directive_to_json(d, line, filename))
        .collect();

    let load_output = LoadOutput {
        api_version: API_VERSION,
        entries,
        errors: load.errors.clone(),
        options: load.options,
        plugins: load.plugins,
        includes: load.includes,
    };

    // Execute queries (only if no parse errors)
    let query_outputs: Vec<QueryOutput> = if load.errors.is_empty() {
        queries
            .iter()
            .map(|q| execute_query(&load.directives, q))
            .collect()
    } else {
        // Return error for each query
        queries
            .iter()
            .map(|_| QueryOutput {
                api_version: API_VERSION,
                columns: vec![],
                rows: vec![],
                errors: vec![Error::new("Cannot execute query: parse errors exist")],
            })
            .collect()
    };

    let output = BatchOutput {
        api_version: API_VERSION,
        load: load_output,
        queries: query_outputs,
    };
    output_json(&output)
}

// =============================================================================
// Format Command
// =============================================================================

/// Output for format command.
#[derive(Serialize)]
struct FormatOutput {
    api_version: &'static str,
    /// Formatted beancount source text.
    formatted: String,
    /// Any errors encountered.
    errors: Vec<Error>,
}

fn cmd_format(source: &str) -> i32 {
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

// =============================================================================
// Utility Commands
// =============================================================================

/// Output for is-encrypted command.
#[derive(Serialize)]
struct IsEncryptedOutput {
    api_version: &'static str,
    encrypted: bool,
    reason: Option<String>,
}

/// Check if a file is GPG-encrypted.
fn cmd_is_encrypted(path: &str) -> i32 {
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
                        // Check for GPG packet tags
                        if first_byte == 0x85
                            || first_byte == 0x84
                            || (0xC0..=0xCF).contains(&first_byte)
                        {
                            (true, Some("GPG binary header".to_string()))
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
                // If we can't read the file, report error
                eprintln!("Error reading file: {e}");
                return exit_codes::USER_ERROR;
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

/// Output for get-account-type command.
#[derive(Serialize)]
struct AccountTypeOutput {
    api_version: &'static str,
    account: String,
    account_type: Option<String>,
}

/// Extract account type (first component) from an account name.
fn cmd_get_account_type(account: &str) -> i32 {
    let account_type = account.split(':').next().map(String::from);
    let output = AccountTypeOutput {
        api_version: API_VERSION,
        account: account.to_string(),
        account_type,
    };
    output_json(&output)
}

/// Output for types command - exposes type constants.
#[derive(Serialize)]
struct TypesOutput {
    api_version: &'static str,
    /// All directive type names.
    all_directives: Vec<&'static str>,
    /// Booking method names.
    booking_methods: Vec<&'static str>,
    /// The MISSING sentinel description.
    missing: MissingSentinel,
    /// Default account type prefixes.
    account_types: Vec<&'static str>,
}

#[derive(Serialize)]
struct MissingSentinel {
    description: &'static str,
    /// In JSON output, missing amounts appear as null or with `currency_only` field.
    json_representation: &'static str,
}

fn cmd_types() -> i32 {
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

// =============================================================================
// Schema Command
// =============================================================================

/// Output for schema command - JSON Schema documentation for all types.
fn cmd_schema() -> i32 {
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
            "InputEntry": {
                "description": "Input format for create-entry and format-entry commands",
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type", "date"],
                        "properties": {
                            "type": {"const": "transaction"},
                            "date": {"type": "string", "format": "date"},
                            "flag": {"type": "string", "default": "*", "enum": ["*", "!", "txn"]},
                            "payee": {"type": "string"},
                            "narration": {"type": "string"},
                            "tags": {"type": "array", "items": {"type": "string"}},
                            "links": {"type": "array", "items": {"type": "string"}},
                            "postings": {"type": "array", "items": {"$ref": "#/schemas/Posting"}},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account"],
                        "properties": {
                            "type": {"const": "open"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "currencies": {"type": "array", "items": {"type": "string"}},
                            "booking": {"type": "string", "enum": ["STRICT", "FIFO", "LIFO", "HIFO", "AVERAGE", "NONE"]},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account"],
                        "properties": {
                            "type": {"const": "close"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account", "amount"],
                        "properties": {
                            "type": {"const": "balance"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "amount": {"$ref": "#/schemas/Amount"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account", "source_account"],
                        "properties": {
                            "type": {"const": "pad"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "source_account": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "currency"],
                        "properties": {
                            "type": {"const": "commodity"},
                            "date": {"type": "string", "format": "date"},
                            "currency": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "currency", "amount"],
                        "properties": {
                            "type": {"const": "price"},
                            "date": {"type": "string", "format": "date"},
                            "currency": {"type": "string"},
                            "amount": {"$ref": "#/schemas/Amount"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "event_type", "value"],
                        "properties": {
                            "type": {"const": "event"},
                            "date": {"type": "string", "format": "date"},
                            "event_type": {"type": "string"},
                            "value": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account", "comment"],
                        "properties": {
                            "type": {"const": "note"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "comment": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "account", "path"],
                        "properties": {
                            "type": {"const": "document"},
                            "date": {"type": "string", "format": "date"},
                            "account": {"type": "string"},
                            "path": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "name", "query_string"],
                        "properties": {
                            "type": {"const": "query"},
                            "date": {"type": "string", "format": "date"},
                            "name": {"type": "string"},
                            "query_string": {"type": "string"},
                            "meta": {"type": "object"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["type", "date", "custom_type"],
                        "properties": {
                            "type": {"const": "custom"},
                            "date": {"type": "string", "format": "date"},
                            "custom_type": {"type": "string"},
                            "values": {"type": "array"},
                            "meta": {"type": "object"}
                        }
                    }
                ]
            },
            "OutputEntry": {
                "description": "Output format from load command (same as InputEntry but with meta.hash)",
                "allOf": [
                    {"$ref": "#/schemas/InputEntry"},
                    {
                        "properties": {
                            "meta": {
                                "type": "object",
                                "required": ["filename", "lineno", "hash"],
                                "properties": {
                                    "filename": {"type": "string"},
                                    "lineno": {"type": "integer"},
                                    "hash": {"type": "string", "description": "SHA256 hash of entry"}
                                }
                            }
                        }
                    }
                ]
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
        },
        "commands": {
            "load": {
                "description": "Parse beancount source and return entries",
                "input": "Beancount source text (stdin)",
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "entries": {"type": "array", "items": {"$ref": "#/schemas/OutputEntry"}},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}},
                        "options": {"type": "object"},
                        "plugins": {"type": "array"},
                        "includes": {"type": "array"}
                    }
                }
            },
            "validate": {
                "description": "Validate beancount source",
                "input": "Beancount source text (stdin)",
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "valid": {"type": "boolean"},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "query": {
                "description": "Run BQL query on beancount source",
                "input": "Beancount source text (stdin) + BQL query (arg)",
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "columns": {"type": "array", "items": {"type": "object", "properties": {"name": {"type": "string"}, "datatype": {"type": "string"}}}},
                        "rows": {"type": "array", "items": {"type": "array"}},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "format-entry": {
                "description": "Format single entry JSON to beancount text",
                "input": {"$ref": "#/schemas/InputEntry"},
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "formatted": {"type": "string", "description": "Formatted beancount text"},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "format-entries": {
                "description": "Format array of entry JSON to beancount text",
                "input": {"type": "array", "items": {"$ref": "#/schemas/InputEntry"}},
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "formatted": {"type": "string"},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "create-entry": {
                "description": "Create full entry with hash from minimal JSON",
                "input": {"$ref": "#/schemas/InputEntry"},
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "entry": {"$ref": "#/schemas/OutputEntry"},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "create-entries": {
                "description": "Create multiple entries with hashes from JSON array",
                "input": {"type": "array", "items": {"$ref": "#/schemas/InputEntry"}},
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "entries": {"type": "array", "items": {"$ref": "#/schemas/OutputEntry"}},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            },
            "clamp": {
                "description": "Filter entries by date range with opening balances",
                "input": "Beancount source text (stdin) + begin/end dates (args)",
                "output": {
                    "type": "object",
                    "properties": {
                        "api_version": {"type": "string"},
                        "entries": {"type": "array", "items": {"$ref": "#/schemas/OutputEntry"}},
                        "opening_balances": {"type": "array"},
                        "errors": {"type": "array", "items": {"$ref": "#/schemas/Error"}}
                    }
                }
            }
        }
    });

    output_json(&schema)
}

// =============================================================================
// Format Entry Command
// =============================================================================

/// Output for format-entry command.
#[derive(Serialize)]
struct FormatEntryOutput {
    api_version: &'static str,
    formatted: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<Error>,
}

/// Format a single entry from JSON to beancount text.
fn cmd_format_entry(json_str: &str) -> i32 {
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
fn cmd_format_entries(json_str: &str) -> i32 {
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

// =============================================================================
// Create Entry Command
// =============================================================================

/// Output for create-entry command.
#[derive(Serialize)]
struct CreateEntryOutput {
    api_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry: Option<DirectiveJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<Error>,
}

/// Create a full entry with hash from minimal JSON input.
fn cmd_create_entry(json_str: &str) -> i32 {
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
#[derive(Serialize)]
struct CreateEntriesOutput {
    api_version: &'static str,
    entries: Vec<DirectiveJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<Error>,
}

fn cmd_create_entries(json_str: &str) -> i32 {
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

// =============================================================================
// Clamp Command (Date Range Filtering)
// =============================================================================

/// Output for clamp command.
#[derive(Serialize)]
struct ClampOutput {
    api_version: &'static str,
    entries: Vec<DirectiveJson>,
    /// Opening balances synthesized for the begin date.
    opening_balances: Vec<OpeningBalance>,
    errors: Vec<Error>,
}

#[derive(Serialize)]
struct OpeningBalance {
    account: String,
    date: String,
    balance: InventoryJson,
}

#[derive(Serialize)]
struct InventoryJson {
    positions: Vec<PositionJson>,
}

#[derive(Serialize, Clone)]
struct PositionJson {
    units: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<CostJson>,
}

#[derive(Serialize, Clone)]
struct CostJson {
    number: String,
    currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Check if an account is a balance sheet account (Assets, Liabilities, Equity).
fn is_balance_sheet_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Assets" | "Liabilities" | "Equity")
}

/// Check if an account is an income statement account (Income, Expenses).
fn is_income_statement_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Income" | "Expenses")
}

fn cmd_clamp(
    source: &str,
    filename: &str,
    begin_date: Option<&str>,
    end_date: Option<&str>,
) -> i32 {
    let load = load_source(source);

    // Parse date arguments
    let begin: Option<NaiveDate> =
        begin_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let end: Option<NaiveDate> =
        end_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    // Track account balances for opening balances
    let mut account_balances: HashMap<String, rustledger_core::Inventory> = HashMap::new();
    let mut opening_balances: Vec<OpeningBalance> = Vec::new();

    // Track most recent price per commodity before begin_date
    // Key: (base_currency, quote_currency), Value: (date, price_directive, line)
    let mut latest_prices: HashMap<(String, String), (NaiveDate, Directive, u32)> = HashMap::new();

    // Filter directives
    let mut filtered_directives: Vec<(Directive, u32)> = Vec::new();

    for (directive, &line) in load.directives.iter().zip(load.directive_lines.iter()) {
        let directive_date = directive.date();

        // Check if directive is before begin date - accumulate balances and track prices
        if let Some(begin) = begin {
            if directive_date < begin {
                // Accumulate transaction postings for opening balances
                if let Directive::Transaction(txn) = directive {
                    for posting in &txn.postings {
                        if let Some(rustledger_core::IncompleteAmount::Complete(amount)) =
                            &posting.units
                        {
                            let inv = account_balances
                                .entry(posting.account.to_string())
                                .or_default();
                            let position = if let Some(cost_spec) = &posting.cost {
                                // Create position with cost from cost spec
                                let cost = Cost {
                                    number: cost_spec.number_per.unwrap_or(amount.number),
                                    currency: cost_spec
                                        .currency
                                        .clone()
                                        .unwrap_or_else(|| amount.currency.clone()),
                                    date: cost_spec.date.or(Some(txn.date)),
                                    label: cost_spec.label.clone(),
                                };
                                rustledger_core::Position::with_cost(amount.clone(), cost)
                            } else {
                                rustledger_core::Position::simple(amount.clone())
                            };
                            inv.add(position);
                        }
                    }
                }

                // Track most recent price per commodity before begin_date
                if let Directive::Price(price) = directive {
                    let key = (
                        price.currency.to_string(),
                        price.amount.currency.to_string(),
                    );
                    let should_update = latest_prices
                        .get(&key)
                        .map_or(true, |(existing_date, _, _)| {
                            directive_date >= *existing_date
                        });
                    if should_update {
                        latest_prices.insert(key, (directive_date, directive.clone(), line));
                    }
                }

                // Keep Open directives that are before begin date (accounts need to be opened)
                // Note: Commodity entries are NOT included (beancount's clamp_opt excludes them)
                if let Directive::Open(_) = directive {
                    filtered_directives.push((directive.clone(), line));
                }
                continue;
            }
        }

        // Check if directive is after end date - skip
        if let Some(end) = end {
            if directive_date >= end {
                continue;
            }
        }

        // Exclude Commodity entries from output (beancount's clamp_opt doesn't include them)
        if let Directive::Commodity(_) = directive {
            continue;
        }

        // Include directive in filtered output
        filtered_directives.push((directive.clone(), line));
    }

    // Add most recent prices before begin_date to filtered directives
    let mut price_entries: Vec<(Directive, u32)> = latest_prices
        .into_values()
        .map(|(_, directive, line)| (directive, line))
        .collect();
    // Sort prices for deterministic ordering
    price_entries.sort_by(|(a, _), (b, _)| a.date().cmp(&b.date()));

    // Generate opening balance summarization transactions for begin date
    // This matches beancount's clamp_opt behavior:
    // - Balance sheet accounts (Assets, Liabilities, Equity) get individual summarizations
    // - Income/Expense accounts are aggregated into Equity:Earnings:Previous
    let mut summarization_entries: Vec<DirectiveJson> = Vec::new();
    if let Some(begin) = begin {
        // Summarization date is one day before begin date
        let summarize_date = begin.pred_opt().unwrap_or(begin);
        let summarize_date_str = summarize_date.to_string();

        // Separate balance sheet accounts from income statement accounts
        let mut balance_sheet_accounts: Vec<(&String, &rustledger_core::Inventory)> = Vec::new();
        let mut retained_earnings: rustledger_core::Inventory = rustledger_core::Inventory::new();

        for (account, inventory) in &account_balances {
            if inventory.is_empty() {
                continue;
            }

            if is_balance_sheet_account(account) {
                balance_sheet_accounts.push((account, inventory));
            } else if is_income_statement_account(account) {
                // Aggregate Income/Expenses into retained earnings
                for position in inventory.positions() {
                    retained_earnings.add(position.clone());
                }
            }
        }

        // Sort balance sheet accounts for deterministic ordering
        balance_sheet_accounts.sort_by_key(|(account, _)| *account);

        // Create summarization transactions for balance sheet accounts
        for (index, (account, inventory)) in balance_sheet_accounts.iter().enumerate() {
            // Build positions for opening_balances (backward compatibility)
            let positions: Vec<PositionJson> = inventory
                .positions()
                .iter()
                .map(|p| PositionJson {
                    units: Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    },
                    cost: p.cost.as_ref().map(|c| CostJson {
                        number: c.number.to_string(),
                        currency: c.currency.to_string(),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                })
                .collect();

            opening_balances.push(OpeningBalance {
                account: (*account).clone(),
                date: begin.to_string(),
                balance: InventoryJson {
                    positions: positions.clone(),
                },
            });

            // Create summarization transaction (matches beancount behavior)
            // Each position becomes a posting in the transaction
            let postings: Vec<Posting> = inventory
                .positions()
                .iter()
                .map(|p| Posting {
                    account: (*account).clone(),
                    units: Some(Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    }),
                    cost: p.cost.as_ref().map(|c| PostingCost {
                        number: Some(c.number.to_string()),
                        number_total: None,
                        currency: Some(c.currency.to_string()),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                    price: None,
                    meta: HashMap::new(),
                })
                .collect();

            // Create hash for the summarization transaction
            let hash_input = format!(
                "S|{}|Opening balance for '{}' (Summarization)|{}",
                summarize_date_str, account, index
            );
            let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

            summarization_entries.push(DirectiveJson::Transaction {
                date: summarize_date_str.clone(),
                flag: "S".to_string(),
                payee: None,
                narration: Some(format!("Opening balance for '{}' (Summarization)", account)),
                tags: vec![],
                links: vec![],
                postings,
                meta: Meta {
                    filename: "<summarize>".to_string(),
                    lineno: index as u32,
                    hash,
                    user: HashMap::new(),
                },
            });
        }

        // Create aggregated Equity:Earnings:Previous transaction for Income/Expenses
        if !retained_earnings.is_empty() {
            let earnings_account = "Equity:Earnings:Previous";
            let index = balance_sheet_accounts.len();

            // Build positions for opening_balances (backward compatibility)
            let positions: Vec<PositionJson> = retained_earnings
                .positions()
                .iter()
                .map(|p| PositionJson {
                    units: Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    },
                    cost: p.cost.as_ref().map(|c| CostJson {
                        number: c.number.to_string(),
                        currency: c.currency.to_string(),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                })
                .collect();

            opening_balances.push(OpeningBalance {
                account: earnings_account.to_string(),
                date: begin.to_string(),
                balance: InventoryJson {
                    positions: positions.clone(),
                },
            });

            // Create summarization transaction for retained earnings
            let postings: Vec<Posting> = retained_earnings
                .positions()
                .iter()
                .map(|p| Posting {
                    account: earnings_account.to_string(),
                    units: Some(Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    }),
                    cost: p.cost.as_ref().map(|c| PostingCost {
                        number: Some(c.number.to_string()),
                        number_total: None,
                        currency: Some(c.currency.to_string()),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                    price: None,
                    meta: HashMap::new(),
                })
                .collect();

            // Create hash for the retained earnings transaction
            let hash_input = format!(
                "S|{}|Opening balance for '{}' (Summarization)|{}",
                summarize_date_str, earnings_account, index
            );
            let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

            summarization_entries.push(DirectiveJson::Transaction {
                date: summarize_date_str.clone(),
                flag: "S".to_string(),
                payee: None,
                narration: Some(format!(
                    "Opening balance for '{}' (Summarization)",
                    earnings_account
                )),
                tags: vec![],
                links: vec![],
                postings,
                meta: Meta {
                    filename: "<summarize>".to_string(),
                    lineno: index as u32,
                    hash,
                    user: HashMap::new(),
                },
            });
        }
    }

    // Convert filtered directives to JSON
    let mut entries: Vec<DirectiveJson> = filtered_directives
        .iter()
        .map(|(d, line)| directive_to_json(d, *line, filename))
        .collect();

    // Insert price entries at the beginning (before summarization entries)
    let price_json: Vec<DirectiveJson> = price_entries
        .iter()
        .map(|(d, line)| directive_to_json(d, *line, filename))
        .collect();

    // Insert summarization transactions before other entries (they have earlier date)
    entries.splice(0..0, summarization_entries);
    // Insert price entries at the very beginning
    entries.splice(0..0, price_json);

    let output = ClampOutput {
        api_version: API_VERSION,
        entries,
        opening_balances,
        errors: load.errors,
    };
    output_json(&output)
}

// =============================================================================
// Filter Entries Command (operates on already-parsed JSON entries)
// =============================================================================

/// Input for filter-entries command.
#[derive(Deserialize)]
struct FilterEntriesInput {
    /// Array of entry objects (same format as load output).
    entries: Vec<serde_json::Value>,
    /// Begin date (inclusive) in ISO format (YYYY-MM-DD).
    begin_date: String,
    /// End date (exclusive) in ISO format (YYYY-MM-DD).
    end_date: String,
}

/// Output for filter-entries command.
#[derive(Serialize)]
struct FilterEntriesOutput {
    api_version: &'static str,
    entries: Vec<serde_json::Value>,
    errors: Vec<Error>,
}

/// Filter already-parsed entries by date range.
///
/// This avoids re-parsing source text when we already have parsed entries.
///
/// Filtering rules (matching beancount behavior):
/// - Include entries where `begin_date <= entry.date < end_date`
/// - Exclude Commodity directives
/// - Include Open directives where `entry.date < end_date` (still active)
/// - Include Close directives where `entry.date >= begin_date`
fn cmd_filter_entries(json_str: &str) -> i32 {
    // Parse input
    let input: FilterEntriesInput = match serde_json::from_str(json_str) {
        Ok(i) => i,
        Err(e) => {
            let output = FilterEntriesOutput {
                api_version: API_VERSION,
                entries: vec![],
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    // Parse date boundaries
    let Ok(begin) = NaiveDate::parse_from_str(&input.begin_date, "%Y-%m-%d") else {
        let output = FilterEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid begin_date format: {}. Expected YYYY-MM-DD",
                    input.begin_date
                ))
                .with_field("begin_date"),
            ],
        };
        return output_json(&output);
    };

    let Ok(end) = NaiveDate::parse_from_str(&input.end_date, "%Y-%m-%d") else {
        let output = FilterEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid end_date format: {}. Expected YYYY-MM-DD",
                    input.end_date
                ))
                .with_field("end_date"),
            ],
        };
        return output_json(&output);
    };

    // Filter entries
    let mut filtered: Vec<serde_json::Value> = Vec::new();

    for entry in input.entries {
        // Extract entry type and date
        let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let date_str = entry.get("date").and_then(|d| d.as_str()).unwrap_or("");

        let entry_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue, // Skip entries with invalid dates
        };

        // Apply filtering rules based on directive type
        let include = match entry_type {
            // Commodity directives are excluded (beancount behavior)
            "commodity" => false,

            // Open directives: include if date < end_date (still active)
            "open" => entry_date < end,

            // Close directives: include if date >= begin_date
            "close" => entry_date >= begin,

            // All other directives: include if begin_date <= date < end_date
            _ => entry_date >= begin && entry_date < end,
        };

        if include {
            filtered.push(entry);
        }
    }

    let output = FilterEntriesOutput {
        api_version: API_VERSION,
        entries: filtered,
        errors: vec![],
    };
    output_json(&output)
}

fn cmd_version() -> i32 {
    let output = VersionOutput {
        api_version: API_VERSION,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    output_json(&output)
}

fn cmd_help() {
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
        cmd_help();
        std::process::exit(exit_codes::USER_ERROR);
    }

    let command = &args[1];

    let exit_code = match command.as_str() {
        "version" => cmd_version(),
        "help" | "--help" | "-h" => {
            cmd_help();
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
                    Ok(source) => cmd_load(&source, filename),
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
                cmd_load_full(path, &plugins)
            }
        }
        "validate-file" => {
            if args.len() < 3 {
                eprintln!("Error: validate-file command requires file path argument");
                exit_codes::USER_ERROR
            } else {
                match read_source(Some(&args[2])) {
                    Ok(source) => cmd_validate(&source),
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
                    Ok(source) => cmd_query(&source, &args[3]),
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
                    Ok(source) => cmd_batch(&source, filename, &queries),
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
                    Ok(source) => cmd_format(&source),
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
                    Ok(source) => cmd_clamp(&source, filename, begin_date, end_date),
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
                cmd_is_encrypted(&args[2])
            }
        }
        "get-account-type" => {
            if args.len() < 3 {
                eprintln!("Error: get-account-type command requires account name argument");
                exit_codes::USER_ERROR
            } else {
                cmd_get_account_type(&args[2])
            }
        }
        "types" => cmd_types(),
        "schema" => cmd_schema(),
        // Entry manipulation commands (read JSON from stdin)
        "format-entry" | "format-entries" | "create-entry" | "create-entries"
        | "filter-entries" => {
            let json_str = match read_source(None) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(exit_codes::USER_ERROR);
                }
            };
            match command.as_str() {
                "format-entry" => cmd_format_entry(&json_str),
                "format-entries" => cmd_format_entries(&json_str),
                "create-entry" => cmd_create_entry(&json_str),
                "create-entries" => cmd_create_entries(&json_str),
                "filter-entries" => cmd_filter_entries(&json_str),
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
                    cmd_load(&source, filename)
                }
                "validate" => cmd_validate(&source),
                "query" => {
                    if args.len() < 3 {
                        eprintln!("Error: query command requires BQL argument");
                        exit_codes::USER_ERROR
                    } else {
                        cmd_query(&source, &args[2])
                    }
                }
                "batch" => {
                    let filename = args.get(2).map_or("<stdin>", std::string::String::as_str);
                    let queries: Vec<String> = args.iter().skip(3).cloned().collect();
                    cmd_batch(&source, filename, &queries)
                }
                "format" => cmd_format(&source),
                "clamp" => {
                    let filename = args.get(2).map_or("<stdin>", std::string::String::as_str);
                    let begin_date = args.get(3).map(String::as_str);
                    let end_date = args.get(4).map(String::as_str);
                    cmd_clamp(&source, filename, begin_date, end_date)
                }
                _ => unreachable!(),
            }
        }
        _ => {
            eprintln!("Unknown command: {command}");
            cmd_help();
            exit_codes::USER_ERROR
        }
    };

    std::process::exit(exit_code);
}
