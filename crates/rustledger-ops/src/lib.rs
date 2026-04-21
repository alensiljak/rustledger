//! Pure operations on beancount directives.
//!
//! This crate provides reusable functions for transforming and analyzing
//! collections of beancount directives. All operations are pure — they take
//! directives in and return results out, with no I/O or framework coupling.
//!
//! Analogous to Python beancount's `ops/` module.
//!
//! # Modules
//!
//! - [`fingerprint`] — structural hashing and stable fingerprinting of transactions
//! - [`dedup`] — duplicate detection (structural, fuzzy, and fingerprint-based)
//! - [`enrichment`] — shared types for operation results (confidence, method, alternatives)

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dedup;
pub mod enrichment;
pub mod fingerprint;
