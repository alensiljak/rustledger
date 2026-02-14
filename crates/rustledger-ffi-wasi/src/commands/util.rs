//! Utility types for the JSON-RPC API.

use serde::Serialize;

/// Output for util.types method.
#[derive(Serialize)]
pub struct TypesOutput {
    pub api_version: &'static str,
    pub all_directives: Vec<&'static str>,
    pub booking_methods: Vec<&'static str>,
    pub missing: MissingSentinel,
    pub account_types: Vec<&'static str>,
}

/// Description of the MISSING sentinel value.
#[derive(Serialize)]
pub struct MissingSentinel {
    pub description: &'static str,
    pub json_representation: &'static str,
}
