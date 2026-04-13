//! Shared test utilities for CLI integration tests.

#![allow(dead_code)]

use std::path::PathBuf;

/// Get the workspace root directory.
pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Get the test fixtures directory.
pub fn test_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Find the rledger binary, checking `CARGO_BIN_EXE`, release, and debug paths.
pub fn rledger_binary() -> Option<PathBuf> {
    // Use CARGO_BIN_EXE_rledger if available (set by cargo test)
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rledger") {
        return Some(PathBuf::from(path));
    }

    // Check target/release first (for --release builds)
    let release_path = project_root().join("target/release/rledger");
    if release_path.exists() {
        return Some(release_path);
    }

    // Fall back to target/debug
    let debug_path = project_root().join("target/debug/rledger");
    if debug_path.exists() {
        return Some(debug_path);
    }

    // Binary not found (Nix builds, not yet built, etc.)
    None
}

/// Skip tests when rledger binary is not available.
#[macro_export]
macro_rules! require_rledger {
    () => {
        match common::rledger_binary() {
            Some(path) => path,
            None => {
                eprintln!("Skipping: rledger binary not found");
                return;
            }
        }
    };
}
