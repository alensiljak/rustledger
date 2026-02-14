//! Load types for the JSON-RPC API.

use serde::Serialize;

use crate::types::{DirectiveJson, Error, LedgerOptions, Plugin};

/// Output for ledger.loadFile method.
#[derive(Serialize)]
pub struct LoadFullOutput {
    pub api_version: &'static str,
    pub entries: Vec<DirectiveJson>,
    pub errors: Vec<Error>,
    pub options: LedgerOptions,
    pub plugins: Vec<Plugin>,
    pub loaded_files: Vec<String>,
}
