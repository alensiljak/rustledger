#![no_main]
//! Fuzz target for the BQL query parser.
//!
//! This fuzzer tests the query parser's robustness against arbitrary input.
//! It ensures the parser doesn't panic, crash, or exhibit undefined behavior
//! when processing malformed or malicious query strings.

use libfuzzer_sys::fuzz_target;
use rustledger_query::parse;

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8 strings
    if let Ok(input) = std::str::from_utf8(data) {
        // The parser should never panic, regardless of input
        // It may return errors, but should handle them gracefully
        let _ = parse(input);
    }
});
