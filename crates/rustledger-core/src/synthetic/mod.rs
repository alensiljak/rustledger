//! Synthetic beancount file generation for testing.
//!
//! This module provides utilities for generating synthetic beancount files
//! to test parser and validator compatibility with Python beancount.
//!
//! # Features
//!
//! - Edge case generators for stress-testing parsers
//! - Manifest tracking for reproducible test fixtures
//! - Integration with proptest for property-based testing

pub mod edge_cases;
pub mod manifest;

pub use edge_cases::*;
pub use manifest::*;
