//! Editor integration helpers for LSP-like functionality.
//!
//! This module provides completion, hover, go-to-definition, and document symbols
//! functionality adapted from rustledger-lsp for use in web editors like `CodeMirror`.

mod completions;
mod definitions;
mod helpers;
mod hover;
mod line_index;
mod references;
mod symbols;

// Re-export only what's used by lib.rs
pub use completions::get_completions_cached;
pub use definitions::get_definition_cached;
pub use hover::get_hover_info_cached;
pub use line_index::EditorCache;
pub use references::get_references_cached;
pub use symbols::get_document_symbols_cached;

#[cfg(test)]
mod tests {
    use super::*;
    use completions::get_completions;

    use rustledger_parser::parse;

    #[test]
    fn test_editor_cache_new() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee Shop" "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;
        let result = parse(source);
        let cache = EditorCache::new(source, &result);

        assert!(!cache.accounts.is_empty());
        assert!(!cache.currencies.is_empty());
        assert!(!cache.payees.is_empty());
    }

    #[test]
    #[ignore = "Manual benchmark - run with: cargo test -p rustledger-wasm --release -- --ignored --nocapture"]
    fn bench_editor_cache_performance() {
        use std::time::Instant;

        // Load test file
        let source = std::fs::read_to_string("../../tests/fixtures/examples/example.beancount")
            .expect("Failed to read example.beancount");

        let parse_result = parse(&source);
        let directive_count = parse_result.directives.len();

        println!("\n=== Editor Performance Benchmark ===");
        println!("File: example.beancount");
        println!("Lines: {}", source.lines().count());
        println!("Directives: {directive_count}");

        // Measure cache build time (one-time cost)
        let start = Instant::now();
        let cache = EditorCache::new(&source, &parse_result);
        let cache_build_time = start.elapsed();
        println!("\nCache build time: {cache_build_time:?}");
        println!("  Accounts cached: {}", cache.accounts.len());
        println!("  Currencies cached: {}", cache.currencies.len());
        println!("  Payees cached: {}", cache.payees.len());

        // Measure cached operations (multiple calls)
        let iterations = 1000;

        // Completions
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_completions_cached(&source, 100, 2, &cache);
        }
        let completions_time = start.elapsed();
        println!(
            "\nCompletions ({iterations}x): {:?} ({:?}/call)",
            completions_time,
            completions_time / iterations
        );

        // Document symbols
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_document_symbols_cached(&parse_result, &cache);
        }
        let symbols_time = start.elapsed();
        println!(
            "Document symbols ({iterations}x): {:?} ({:?}/call)",
            symbols_time,
            symbols_time / iterations
        );

        // Definition lookup
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_definition_cached(&source, 500, 5, &parse_result, &cache);
        }
        let definition_time = start.elapsed();
        println!(
            "Definition lookup ({iterations}x): {:?} ({:?}/call)",
            definition_time,
            definition_time / iterations
        );

        // Compare with legacy (non-cached) approach
        println!("\n--- Legacy (non-cached) comparison ---");

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = get_completions(&source, 100, 2, &parse_result);
        }
        let legacy_completions_time = start.elapsed();
        println!(
            "Legacy completions ({iterations}x): {:?} ({:?}/call)",
            legacy_completions_time,
            legacy_completions_time / iterations
        );

        let speedup =
            legacy_completions_time.as_nanos() as f64 / completions_time.as_nanos() as f64;
        println!("\nSpeedup (cached vs legacy): {speedup:.1}x faster");
    }
}
