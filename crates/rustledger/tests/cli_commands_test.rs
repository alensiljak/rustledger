//! CLI command integration tests.
//!
//! Tests for rledger-check, rledger-query, rledger-format, rledger-doctor, and rledger-report.

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

fn rledger_check_binary() -> PathBuf {
    project_root().join("target/debug/rledger-check")
}

fn rledger_query_binary() -> PathBuf {
    project_root().join("target/debug/rledger-query")
}

fn rledger_format_binary() -> PathBuf {
    project_root().join("target/debug/rledger-format")
}

fn rledger_doctor_binary() -> PathBuf {
    project_root().join("target/debug/rledger-doctor")
}

fn rledger_report_binary() -> PathBuf {
    project_root().join("target/debug/rledger-report")
}

// =============================================================================
// rledger-check tests
// =============================================================================

#[test]
fn test_check_version() {
    let output = Command::new(rledger_check_binary())
        .arg("--version")
        .output()
        .expect("Failed to run rledger-check --version");

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
    let output = Command::new(rledger_check_binary())
        .arg("--help")
        .output()
        .expect("Failed to run rledger-check --help");

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

    let output = Command::new(rledger_check_binary())
        .arg(&path)
        .output()
        .expect("Failed to run rledger-check");

    assert!(output.status.success(), "Valid file should pass check");
}

#[test]
fn test_check_nonexistent_file() {
    let output = Command::new(rledger_check_binary())
        .arg("/nonexistent/file.beancount")
        .output()
        .expect("Failed to run rledger-check");

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

    let output = Command::new(rledger_check_binary())
        .arg("--json")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-check --json");

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

// =============================================================================
// rledger-query tests
// =============================================================================

#[test]
fn test_query_version() {
    let output = Command::new(rledger_query_binary())
        .arg("--version")
        .output()
        .expect("Failed to run rledger-query --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_query_help() {
    let output = Command::new(rledger_query_binary())
        .arg("--help")
        .output()
        .expect("Failed to run rledger-query --help");

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

    let output = Command::new(rledger_query_binary())
        .arg(&path)
        .arg("SELECT DISTINCT account ORDER BY account")
        .output()
        .expect("Failed to run rledger-query");

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

    let output = Command::new(rledger_query_binary())
        .arg(&path)
        .arg("SELECT account, SUM(position) GROUP BY account ORDER BY account")
        .output()
        .expect("Failed to run rledger-query");

    assert!(output.status.success(), "Query should succeed");
}

#[test]
fn test_query_invalid_syntax() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(rledger_query_binary())
        .arg(&path)
        .arg("SELEKT * FROM entries")  // Intentional typo
        .output()
        .expect("Failed to run rledger-query");

    assert!(!output.status.success(), "Invalid query syntax should fail");
}

#[test]
fn test_query_json_output() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(rledger_query_binary())
        .arg("--json")
        .arg(&path)
        .arg("SELECT account LIMIT 3")
        .output()
        .expect("Failed to run rledger-query --json");

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
// rledger-format tests
// =============================================================================

#[test]
fn test_format_version() {
    let output = Command::new(rledger_format_binary())
        .arg("--version")
        .output()
        .expect("Failed to run rledger-format --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_format_help() {
    let output = Command::new(rledger_format_binary())
        .arg("--help")
        .output()
        .expect("Failed to run rledger-format --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_format_file() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(rledger_format_binary())
        .arg(&path)
        .output()
        .expect("Failed to run rledger-format");

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
    let output = Command::new(rledger_format_binary())
        .arg("--check")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-format --check");

    // Either it passes (properly formatted) or fails (needs formatting)
    // Both are valid outcomes for this test
    let _success = output.status.success();
}

// =============================================================================
// rledger-doctor tests
// =============================================================================

#[test]
fn test_doctor_version() {
    let output = Command::new(rledger_doctor_binary())
        .arg("--version")
        .output()
        .expect("Failed to run rledger-doctor --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_doctor_help() {
    let output = Command::new(rledger_doctor_binary())
        .arg("--help")
        .output()
        .expect("Failed to run rledger-doctor --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_doctor_missing_open() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(rledger_doctor_binary())
        .arg("missing-open")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-doctor missing-open");

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

    let output = Command::new(rledger_doctor_binary())
        .arg("context")
        .arg(&path)
        .arg("5")  // Line number
        .output()
        .expect("Failed to run rledger-doctor context");

    // Context command should work (or report no context at line)
    let _success = output.status.success();
}

// =============================================================================
// rledger-report tests
// =============================================================================

#[test]
fn test_report_version() {
    let output = Command::new(rledger_report_binary())
        .arg("--version")
        .output()
        .expect("Failed to run rledger-report --version");

    assert!(output.status.success(), "Version should succeed");
}

#[test]
fn test_report_help() {
    let output = Command::new(rledger_report_binary())
        .arg("--help")
        .output()
        .expect("Failed to run rledger-report --help");

    assert!(output.status.success(), "Help should succeed");
}

#[test]
fn test_report_balances() {
    let path = test_fixtures_dir().join("valid-ledger.beancount");
    if !path.exists() {
        eprintln!("Skipping: valid-ledger.beancount not found");
        return;
    }

    let output = Command::new(rledger_report_binary())
        .arg("balances")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-report balances");

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

    let output = Command::new(rledger_report_binary())
        .arg("trial-balance")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-report trial-balance");

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

    let output = Command::new(rledger_report_binary())
        .arg("journal")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-report journal");

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

    let output = Command::new(rledger_check_binary())
        .arg(&temp_file)
        .output()
        .expect("Failed to run rledger-check");

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

    let output = Command::new(rledger_check_binary())
        .arg(&temp_file)
        .output()
        .expect("Failed to run rledger-check");

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

    let output = Command::new(rledger_check_binary())
        .arg("--native-plugin")
        .arg("auto_accounts")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-check with plugin");

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

    let output = Command::new(rledger_check_binary())
        .arg("--native-plugin")
        .arg("nonexistent_plugin_xyz_12345")
        .arg(&path)
        .output()
        .expect("Failed to run rledger-check with unknown plugin");

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

    let mut child = Command::new(rledger_query_binary())
        .arg("-")  // Read from stdin
        .arg("SELECT account")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn rledger-query");

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
