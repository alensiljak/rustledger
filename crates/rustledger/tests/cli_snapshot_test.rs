//! Snapshot tests for CLI output formatting.
//!
//! These tests capture the exact output of `rledger check` and `rledger query`
//! to detect unintentional formatting regressions. When formatting changes
//! intentionally, update snapshots with `cargo insta review`.
//!
//! See issue #786: <https://github.com/rustledger/rustledger/issues/786>

mod common;

use std::process::Command;

use common::test_fixtures_dir;

/// Normalize output for snapshot stability: strip the file path prefix
/// so snapshots don't depend on the absolute path of the test machine.
fn normalize_output(output: &str, fixture_dir: &str) -> String {
    output.replace(fixture_dir, "<fixtures>")
}

// ============================================================================
// rledger check — text output
// ============================================================================

/// Snapshot: `rledger check` on a valid file (text mode).
/// The file triggers a warning (E1004: close with non-zero balance) but no errors.
#[test]
fn test_snapshot_check_clean_file_text() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("valid-ledger.beancount");

    let output = Command::new(&rledger)
        .args(["check", "--no-cache"])
        .arg(&fixture)
        .output()
        .expect("failed to run rledger check");

    assert!(
        output.status.success(),
        "rledger check should succeed on valid file: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let normalized = normalize_output(&stdout, &test_fixtures_dir().display().to_string());

    insta::assert_snapshot!("check_clean_text", normalized.trim());
}

/// Snapshot: `rledger check` on a file with parse errors (text mode).
#[test]
fn test_snapshot_check_parse_errors_text() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("parse-errors.beancount");

    let output = Command::new(&rledger)
        .args(["check", "--no-cache"])
        .arg(&fixture)
        .output()
        .expect("failed to run rledger check");

    assert!(
        !output.status.success(),
        "rledger check should fail on file with parse errors"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let normalized = normalize_output(&stdout, &test_fixtures_dir().display().to_string());

    insta::assert_snapshot!("check_parse_errors_text", normalized.trim());
}

/// Snapshot: `rledger check` on a file with validation errors (text mode).
#[test]
fn test_snapshot_check_validation_errors_text() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("validation-errors.beancount");

    let output = Command::new(&rledger)
        .args(["check", "--no-cache"])
        .arg(&fixture)
        .output()
        .expect("failed to run rledger check");

    assert!(
        !output.status.success(),
        "rledger check should fail on file with validation errors"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let normalized = normalize_output(&stdout, &test_fixtures_dir().display().to_string());

    insta::assert_snapshot!("check_validation_errors_text", normalized.trim());
}

// ============================================================================
// rledger check — JSON output
// ============================================================================

/// Snapshot: `rledger check --format json` on a clean file.
#[test]
fn test_snapshot_check_clean_file_json() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("valid-ledger.beancount");

    let output = Command::new(&rledger)
        .args(["check", "--format", "json", "--no-cache"])
        .arg(&fixture)
        .output()
        .expect("failed to run rledger check");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("--format") {
            eprintln!("Skipping: --format json not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse and re-serialize to normalize field ordering, then snapshot
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should produce valid JSON");

    // Snapshot the structure — redact file paths for portability
    let normalized_json = normalize_json_paths(&json, &test_fixtures_dir().display().to_string());
    let pretty = serde_json::to_string_pretty(&normalized_json).unwrap();

    insta::assert_snapshot!("check_clean_json", pretty);
}

/// Snapshot: `rledger check --format json` on a file with validation errors.
#[test]
fn test_snapshot_check_validation_errors_json() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("validation-errors.beancount");

    let output = Command::new(&rledger)
        .args(["check", "--format", "json", "--no-cache"])
        .arg(&fixture)
        .output()
        .expect("failed to run rledger check");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("--format") {
            eprintln!("Skipping: --format json not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should produce valid JSON");
    let normalized_json = normalize_json_paths(&json, &test_fixtures_dir().display().to_string());
    let pretty = serde_json::to_string_pretty(&normalized_json).unwrap();

    insta::assert_snapshot!("check_validation_errors_json", pretty);
}

// ============================================================================
// rledger query — table output
// ============================================================================

/// Snapshot: `rledger query` table output for a simple SELECT.
#[test]
fn test_snapshot_query_table_output() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("query-test.beancount");

    let output = Command::new(&rledger)
        .args(["query", "-q"])
        .arg(&fixture)
        .arg("SELECT date, narration, account, position WHERE account ~ 'Expenses' ORDER BY date")
        .output()
        .expect("failed to run rledger query");

    assert!(
        output.status.success(),
        "rledger query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    insta::assert_snapshot!("query_table_output", stdout.trim());
}

/// Snapshot: `rledger query` aggregate output.
#[test]
fn test_snapshot_query_aggregate_output() {
    let rledger = require_rledger!();
    let fixture = test_fixtures_dir().join("query-test.beancount");

    let output = Command::new(&rledger)
        .args(["query", "-q"])
        .arg(&fixture)
        .arg("SELECT account, SUM(position) GROUP BY account ORDER BY account")
        .output()
        .expect("failed to run rledger query");

    assert!(
        output.status.success(),
        "rledger query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    insta::assert_snapshot!("query_aggregate_output", stdout.trim());
}

// ============================================================================
// Helpers
// ============================================================================

/// Replace absolute file paths in JSON values with `<fixtures>/filename`.
fn normalize_json_paths(value: &serde_json::Value, fixture_dir: &str) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            serde_json::Value::String(s.replace(fixture_dir, "<fixtures>"))
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.iter()
                .map(|v| normalize_json_paths(v, fixture_dir))
                .collect(),
        ),
        serde_json::Value::Object(obj) => serde_json::Value::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), normalize_json_paths(v, fixture_dir)))
                .collect(),
        ),
        other => other.clone(),
    }
}
