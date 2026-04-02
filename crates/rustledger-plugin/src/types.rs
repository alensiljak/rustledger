//! Plugin interface types.
//!
//! These types define the contract between the plugin host and plugins.
//! They are serialized via `MessagePack` across the WASM boundary.
//!
//! This module re-exports types from [`rustledger_plugin_types`], which can
//! also be used directly by WASM plugins. Using the types crate directly
//! allows plugins to avoid pulling in the full `rustledger-plugin` dependency.

// Re-export all types from the plugin-types crate
pub use rustledger_plugin_types::*;
