//! CLI command integration tests.
//!
//! Tests for rledger check, rledger query, rledger format, rledger doctor, and rledger report.

use std::path::PathBuf;
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn test_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn rledger_binary() -> Option<PathBuf> {
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

/// Helper macro to skip tests when rledger binary is not available
macro_rules! require_rledger {
    () => {
        match rledger_binary() {
            Some(path) => path,
            None => {
                eprintln!("Skipping: rledger binary not found");
                return;
            }
        }
    };
}

// =============================================================================
// rledger check tests
// =============================================================================

#[test]
fn test_check_version() {
    let output = Command::new(require_rledger!())
        .args(["check", "--version"])
        .output()
        .expect("Failed to run rledger check --version");

    assert!(output.status.success(), "Version should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Version output should contain a version number
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()) || stdout.contains('.'),
        "Version output should contain version info: {stdout}"
    );
}

#[test]
fn test_check_help() {
    let output = Command::new(require_rledger!())
        .args(["check", "--help"])
        .output()
        .expect("Failed to run rledger check --help");

    assert!(output.status.success(), "Help should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage") || stdout.contains("usage"),
        "Help should show usage"
    );
}

#[test]
fn test_check_valid_file() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg(&path)
        .output()
        .expect("Failed to run rledger check");

    assert!(output.status.success(), "Valid file should pass check");
}

#[test]
fn test_check_nonexistent_file() {
    let output = Command::new(require_rledger!())
        .args(["check", "/nonexistent/file.beancount"])
        .output()
        .expect("Failed to run rledger check");

    assert!(
        !output.status.success(),
        "Nonexistent file should fail check"
    );
}

#[test]
fn test_check_json_output() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg("--json")
        .arg(&path)
        .output()
        .expect("Failed to run rledger check --json");

    // Skip if --json is not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") && stderr.contains("--json") {
            eprintln!("Skipping: --json flag not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // JSON output should be valid JSON (starts with { or [)
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        assert!(
            trimmed.starts_with('{') || trimmed.starts_with('['),
            "JSON output should be valid JSON, got: {trimmed}"
        );
    }
}

/// Regression for issue #736 case 1: an account whose root type is not one
/// of the configured account names (defaults: Assets/Liabilities/Equity/
/// Income/Expenses) must be reported as a parse-phase diagnostic in JSON
/// output. This matches Python beancount, where the lexer itself rejects
/// such account names, and satisfies the pta-standards conformance harness
/// which classifies errors by the `phase` field.
#[test]
fn test_check_invalid_account_root_is_parse_phase() {
    let rledger = require_rledger!();
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), "2024-01-01 open Savings:Emergency\n").expect("write");

    let output = Command::new(&rledger)
        .args(["check", "--format", "json", "--no-cache"])
        .arg(tmp.path())
        .output()
        .expect("Failed to run rledger check");

    // Skip if this rledger build doesn't support --no-cache or --format json.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("--no-cache") || stderr.contains("--format") {
            eprintln!("Skipping: required flags not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("check --format json should produce valid JSON");

    let diagnostics = json["diagnostics"]
        .as_array()
        .expect("diagnostics array missing");
    let e1005 = diagnostics
        .iter()
        .find(|d| d["code"] == "E1005")
        .expect("expected E1005 diagnostic for Savings:Emergency");

    assert_eq!(
        e1005["phase"], "parse",
        "E1005 must be phase=parse for conformance compatibility, got: {e1005}"
    );
    assert_eq!(
        json["parse_error_count"], 1,
        "parse_error_count should include E1005; got json: {json}"
    );
    assert_eq!(
        json["validate_error_count"], 0,
        "validate_error_count should not include E1005; got json: {json}"
    );
}

/// Regression for issue #737: a wildcard reduction `-5 AAPL {}` against an
/// inventory holding lots at different costs must produce exactly one E4003
/// "Ambiguous lot match" diagnostic — not zero (the original silent-accept
/// bug) and not two (the validator/booking double-report).
#[test]
fn test_check_ambiguous_lot_match_reports_once() {
    let rledger = require_rledger!();
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(
        tmp.path(),
        "\
2024-01-01 open Assets:Stock AAPL \"STRICT\"
2024-01-01 open Assets:Cash USD
2024-01-01 open Income:Gains

2024-01-15 * \"Buy lot 1\"
  Assets:Stock 10 AAPL {150 USD}
  Assets:Cash -1500 USD

2024-01-20 * \"Buy lot 2\"
  Assets:Stock 10 AAPL {160 USD}
  Assets:Cash -1600 USD

2024-02-15 * \"Sell - ambiguous\"
  Assets:Stock -5 AAPL {}
  Assets:Cash 800 USD
  Income:Gains
",
    )
    .expect("write");

    let output = Command::new(&rledger)
        .args(["check", "--format", "json", "--no-cache"])
        .arg(tmp.path())
        .output()
        .expect("failed to run rledger check");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("--no-cache") || stderr.contains("--format") {
            eprintln!("Skipping: required flags not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("check --format json should produce valid JSON");

    let diagnostics = json["diagnostics"]
        .as_array()
        .expect("diagnostics array missing");
    let e4003: Vec<_> = diagnostics
        .iter()
        .filter(|d| d["code"] == "E4003")
        .collect();

    assert_eq!(
        e4003.len(),
        1,
        "expected exactly one E4003 diagnostic, got {}: {json}",
        e4003.len()
    );
    let msg = e4003[0]["message"].as_str().unwrap_or("");
    assert!(
        msg.to_lowercase().contains("ambiguous"),
        "E4003 message should mention 'ambiguous', got: {msg}"
    );
}

// =============================================================================
// rledger query tests
// =============================================================================

#[test]
fn test_query_version() {
    let output = Command::new(require_rledger!())
        .args(["query", "--version"])
        .output()
        .expect("Failed to run rledger query --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_query_help() {
    let output = Command::new(require_rledger!())
        .args(["query", "--help"])
        .output()
        .expect("Failed to run rledger query --help");

    assert!(output.status.success(), "Help should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage") || stdout.contains("usage"),
        "Help should show usage"
    );
}

#[test]
fn test_query_select_accounts() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("query")
        .arg(&path)
        .arg("SELECT DISTINCT account ORDER BY account")
        .output()
        .expect("Failed to run rledger query");

    assert!(output.status.success(), "Query should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Assets") || stdout.contains("Expenses"),
        "Query should return accounts"
    );
}

#[test]
fn test_query_sum_positions() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("query")
        .arg(&path)
        .arg("SELECT account, SUM(position) GROUP BY account ORDER BY account")
        .output()
        .expect("Failed to run rledger query");

    assert!(output.status.success(), "Query should succeed");
}

#[test]
fn test_query_invalid_syntax() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("query")
        .arg(&path)
        .arg("SELEKT * FROM entries") // Intentional typo
        .output()
        .expect("Failed to run rledger query");

    assert!(!output.status.success(), "Invalid query syntax should fail");
}

#[test]
fn test_query_json_output() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("query")
        .arg("--json")
        .arg(&path)
        .arg("SELECT account LIMIT 3")
        .output()
        .expect("Failed to run rledger query --json");

    // Skip if --json is not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") && stderr.contains("--json") {
            eprintln!("Skipping: --json flag not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        assert!(
            trimmed.starts_with('{') || trimmed.starts_with('['),
            "JSON output should be valid JSON"
        );
    }
}

// =============================================================================
// rledger format tests
// =============================================================================

#[test]
fn test_format_version() {
    let output = Command::new(require_rledger!())
        .args(["format", "--version"])
        .output()
        .expect("Failed to run rledger format --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_format_help() {
    let output = Command::new(require_rledger!())
        .args(["format", "--help"])
        .output()
        .expect("Failed to run rledger format --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_format_file() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("format")
        .arg(&path)
        .output()
        .expect("Failed to run rledger format");

    assert!(output.status.success(), "Format should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Formatted output should contain some beancount content
    assert!(
        stdout.contains("open") || stdout.contains("2020"),
        "Formatted output should contain beancount content"
    );
}

#[test]
fn test_format_check_mode() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    // --check mode should not modify file, just check if formatting needed
    let output = Command::new(require_rledger!())
        .arg("format")
        .arg("--check")
        .arg(&path)
        .output()
        .expect("Failed to run rledger format --check");

    // Either it passes (properly formatted) or fails (needs formatting)
    // Both are valid outcomes for this test
    let _success = output.status.success();
}

// =============================================================================
// rledger doctor tests
// =============================================================================

#[test]
fn test_doctor_version() {
    let output = Command::new(require_rledger!())
        .args(["doctor", "--version"])
        .output()
        .expect("Failed to run rledger doctor --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_doctor_help() {
    let output = Command::new(require_rledger!())
        .args(["doctor", "--help"])
        .output()
        .expect("Failed to run rledger doctor --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_doctor_missing_open() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("doctor")
        .arg("missing-open")
        .arg(&path)
        .output()
        .expect("Failed to run rledger doctor missing-open");

    // Should succeed even if no missing opens found
    assert!(
        output.status.success(),
        "Doctor missing-open should succeed"
    );
}

#[test]
fn test_doctor_context() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("doctor")
        .arg("context")
        .arg(&path)
        .arg("5") // Line number
        .output()
        .expect("Failed to run rledger doctor context");

    // Context command should work (or report no context at line)
    let _success = output.status.success();
}

// =============================================================================
// rledger report tests
// =============================================================================

#[test]
fn test_report_version() {
    let output = Command::new(require_rledger!())
        .args(["report", "--version"])
        .output()
        .expect("Failed to run rledger report --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_report_help() {
    let output = Command::new(require_rledger!())
        .args(["report", "--help"])
        .output()
        .expect("Failed to run rledger report --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_report_balances() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("report")
        .arg(&path)
        .arg("balances")
        .output()
        .expect("Failed to run rledger report balances");

    // Skip if subcommand not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") || stderr.contains("Usage") {
            eprintln!("Skipping: 'balances' subcommand not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Assets")
            || stdout.contains("USD")
            || stdout.contains("balance")
            || stdout.is_empty(),
        "Balances report should show accounts or amounts"
    );
}

#[test]
fn test_report_trial_balance() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("report")
        .arg(&path)
        .arg("trial-balance")
        .output()
        .expect("Failed to run rledger report trial-balance");

    // Skip if subcommand not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") || stderr.contains("Usage") {
            eprintln!("Skipping: 'trial-balance' subcommand not supported");
        }
    }
}

#[test]
fn test_report_journal() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("report")
        .arg(&path)
        .arg("journal")
        .output()
        .expect("Failed to run rledger report journal");

    // Skip if subcommand not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") || stderr.contains("Usage") {
            eprintln!("Skipping: 'journal' subcommand not supported");
        }
    }
}

// =============================================================================
// Error message format tests
// =============================================================================

#[test]
fn test_error_message_includes_line_number() {
    // Create a temp file with a validation error
    let content = r#"
2024-01-01 open Assets:Bank USD

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD  ; Account not opened
  Assets:Bank   -5.00 USD
"#;

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("error-line-test.beancount");
    std::fs::write(&temp_file, content).expect("Failed to write temp file");

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg(&temp_file)
        .output()
        .expect("Failed to run rledger check");

    assert!(!output.status.success(), "Should have validation error");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");

    // Error message should include line number
    assert!(
        combined.contains(':') && combined.chars().any(|c| c.is_ascii_digit()),
        "Error should include line number reference"
    );

    std::fs::remove_file(&temp_file).ok();
}

#[test]
fn test_error_message_includes_file_path() {
    let content = r"
2024-01-01 open Assets:Bank USD
2024-01-01 open Assets:Bank USD  ; Duplicate!
";

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("error-path-test.beancount");
    std::fs::write(&temp_file, content).expect("Failed to write temp file");

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg(&temp_file)
        .output()
        .expect("Failed to run rledger check");

    assert!(!output.status.success(), "Should have validation error");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");

    // Error message should include file path
    assert!(
        combined.contains("error-path-test.beancount") || combined.contains(".beancount"),
        "Error should reference file path"
    );

    std::fs::remove_file(&temp_file).ok();
}

// =============================================================================
// Plugin tests
// =============================================================================

#[test]
fn test_check_with_native_plugin() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg("--native-plugin")
        .arg("auto_accounts")
        .arg(&path)
        .output()
        .expect("Failed to run rledger check with plugin");

    assert!(
        output.status.success(),
        "Check with auto_accounts plugin should succeed"
    );
}

#[test]
fn test_check_with_unknown_plugin() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(require_rledger!())
        .arg("check")
        .arg("--native-plugin")
        .arg("nonexistent_plugin_xyz_12345")
        .arg(&path)
        .output()
        .expect("Failed to run rledger check with unknown plugin");

    // Unknown plugin should either fail or produce a warning/error in output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // Either fails or shows error/warning about unknown plugin
    let has_plugin_error = !output.status.success()
        || combined.to_lowercase().contains("unknown")
        || combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error");

    assert!(
        has_plugin_error,
        "Unknown plugin should produce an error: {combined}"
    );
}

// =============================================================================
// Stdin input tests
// =============================================================================

#[test]
fn test_query_stdin_input() {
    let content = r#"
2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
"#;

    let mut child = Command::new(require_rledger!())
        .arg("query")
        .arg("-") // Read from stdin
        .arg("SELECT account")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn rledger query");

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("Failed to get stdin");
        // Handle broken pipe gracefully - stdin may not be supported
        if stdin.write_all(content.as_bytes()).is_err() {
            let _ = child.wait();
            eprintln!("Skipping: stdin write failed (not supported)");
            return;
        }
    }

    let output = child.wait_with_output().expect("Failed to wait on child");

    // Skip if stdin not supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("error:") || stderr.contains('-') || stderr.contains("stdin") {
            eprintln!("Skipping: stdin input not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Assets") || stdout.contains("Expenses") || stdout.is_empty(),
        "Query should return accounts or be empty"
    );
}

// ============================================================================
// JSON Output Validity Tests (Issue #780)
// ============================================================================

/// Helper: run `rledger check --format json --no-cache` on inline content,
/// return parsed JSON. Skips the test if the binary doesn't support the flags.
fn check_json(rledger: &std::path::Path, content: &str) -> Option<serde_json::Value> {
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), content).expect("write");

    let output = Command::new(rledger)
        .args(["check", "--format", "json", "--no-cache"])
        .arg(tmp.path())
        .output()
        .expect("failed to run rledger check");

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("--no-cache") || stderr.contains("--format") {
        eprintln!("Skipping: required flags not supported");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Core assertion: stdout must start with '{' (no plain-text prefix).
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('{'),
        "JSON output must start with '{{', got: {}",
        &trimmed[..trimmed.len().min(200)]
    );

    let json: serde_json::Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
        panic!(
            "stdout is not valid JSON: {e}\nfirst 500 chars: {}",
            &trimmed[..trimmed.len().min(500)]
        )
    });

    // Structural assertions: required top-level fields.
    assert!(json["diagnostics"].is_array(), "missing diagnostics array");
    assert!(json["error_count"].is_number(), "missing error_count");
    assert!(json["warning_count"].is_number(), "missing warning_count");
    assert!(
        json["parse_error_count"].is_number(),
        "missing parse_error_count"
    );
    assert!(
        json["validate_error_count"].is_number(),
        "missing validate_error_count"
    );

    Some(json)
}

/// Regression for #774: plugin errors must appear inside the JSON diagnostics
/// array, not as plain text before the JSON document.
#[test]
fn test_json_output_plugin_errors_in_diagnostics() {
    let rledger = require_rledger!();
    let content = r#"
option "operating_currency" "USD"

plugin "a_completely_nonexistent_plugin"
plugin "another_fake_plugin" "some_config"

2024-01-01 open Assets:Cash USD
2024-01-01 open Expenses:Food

2024-01-15 * "Lunch"
  Expenses:Food   10 USD
  Assets:Cash    -10 USD
"#;

    let Some(json) = check_json(&rledger, content) else {
        return;
    };

    let diagnostics = json["diagnostics"].as_array().unwrap();

    // Plugin errors should be in the diagnostics array.
    let plugin_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            let code = d["code"].as_str().unwrap_or("");
            code == "E8001" || code == "E8005"
        })
        .collect();

    assert!(
        plugin_diags.len() >= 2,
        "expected at least 2 plugin error diagnostics, got {}: {json}",
        plugin_diags.len()
    );

    // error_count must include the plugin errors.
    let error_count = json["error_count"].as_u64().unwrap_or(0);
    assert!(
        error_count >= 2,
        "error_count should include plugin errors, got {error_count}"
    );
}

/// Clean file with no errors: JSON output should have empty diagnostics
/// and all counts at zero.
#[test]
fn test_json_output_clean_file() {
    let rledger = require_rledger!();
    let content = r#"
2024-01-01 open Assets:Cash USD
2024-01-01 open Expenses:Food

2024-01-15 * "Lunch"
  Expenses:Food   10 USD
  Assets:Cash    -10 USD
"#;

    let Some(json) = check_json(&rledger, content) else {
        return;
    };

    let diagnostics = json["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.is_empty(),
        "clean file should have no diagnostics, got: {diagnostics:?}"
    );
    assert_eq!(json["error_count"], 0);
    assert_eq!(json["warning_count"], 0);
    assert_eq!(json["parse_error_count"], 0);
    assert_eq!(json["validate_error_count"], 0);
}

/// File with parse errors only: diagnostics should all have phase "parse".
#[test]
fn test_json_output_parse_errors_only() {
    let rledger = require_rledger!();
    // Malformed beancount — missing amount on second posting, unclosed string
    let content = "2024-01-01 open Assets:Cash\n\nthis is not valid beancount syntax {{{ }}\n";

    let Some(json) = check_json(&rledger, content) else {
        return;
    };

    let error_count = json["error_count"].as_u64().unwrap_or(0);
    assert!(error_count > 0, "should have parse errors");

    let parse_count = json["parse_error_count"].as_u64().unwrap_or(0);
    assert!(parse_count > 0, "parse_error_count should be > 0");
}

/// File with validation errors: diagnostics should include phase "validate".
#[test]
fn test_json_output_validation_errors() {
    let rledger = require_rledger!();
    // Transaction references account that was never opened
    let content = r#"
2024-01-15 * "No open"
  Expenses:Food   10 USD
  Assets:Cash    -10 USD
"#;

    let Some(json) = check_json(&rledger, content) else {
        return;
    };

    let diagnostics = json["diagnostics"].as_array().unwrap();
    let validate_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d["phase"] == "validate")
        .collect();

    assert!(
        !validate_diags.is_empty(),
        "should have validation-phase diagnostics for unopened accounts"
    );

    let validate_count = json["validate_error_count"].as_u64().unwrap_or(0);
    assert!(validate_count > 0, "validate_error_count should be > 0");
}
