//! Example WASM Plugin for rustledger
//!
//! This plugin demonstrates how to create a WASM plugin that:
//! 1. Receives directives from the host
//! 2. Processes them (adds tags, validates, generates new directives)
//! 3. Returns modified directives and any errors
//!
//! # Building
//!
//! ```bash
//! rustup target add wasm32-unknown-unknown
//! cargo build --target wasm32-unknown-unknown --release
//! ```
//!
//! The plugin will be at: `target/wasm32-unknown-unknown/release/example_plugin.wasm`
//!
//! # Using with rustledger
//!
//! ```beancount
//! plugin "path/to/example_plugin.wasm" "threshold=1000"
//!
//! 2024-01-01 open Assets:Bank USD
//! ```

use rustledger_plugin_types::*;

// ============================================================================
// Required Exports
// ============================================================================

/// Allocate memory in WASM linear memory.
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Free memory in WASM linear memory.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

/// Plugin entry point.
#[no_mangle]
pub extern "C" fn process(input_ptr: u32, input_len: u32) -> u64 {
    let input_bytes =
        unsafe { std::slice::from_raw_parts(input_ptr as *const u8, input_len as usize) };

    let input: PluginInput = match rmp_serde::from_slice(input_bytes) {
        Ok(i) => i,
        Err(e) => return pack_error(&format!("Failed to deserialize: {}", e)),
    };

    let output = process_directives(input);

    let output_bytes = match rmp_serde::to_vec(&output) {
        Ok(b) => b,
        Err(e) => return pack_error(&format!("Failed to serialize: {}", e)),
    };

    let output_ptr = alloc(output_bytes.len() as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(output_bytes.as_ptr(), output_ptr, output_bytes.len());
    }

    ((output_ptr as u64) << 32) | (output_bytes.len() as u64)
}

fn pack_error(message: &str) -> u64 {
    let output = PluginOutput {
        directives: Vec::new(),
        errors: vec![PluginError::error(message)],
    };
    let bytes = rmp_serde::to_vec(&output).unwrap_or_default();
    let ptr = alloc(bytes.len() as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ((ptr as u64) << 32) | (bytes.len() as u64)
}

// ============================================================================
// Plugin Logic
// ============================================================================

/// Main plugin processing logic.
///
/// This example plugin:
/// 1. Adds a "processed" tag to all transactions
/// 2. Validates that expense accounts have expense tags
/// 3. Generates warnings for large transactions
fn process_directives(input: PluginInput) -> PluginOutput {
    let mut directives = Vec::new();
    let mut errors = Vec::new();

    // Parse config (example: "threshold=1000")
    let threshold: f64 = input
        .config
        .as_ref()
        .and_then(|c| c.strip_prefix("threshold=").and_then(|s| s.parse().ok()))
        .unwrap_or(1000.0);

    for mut wrapper in input.directives {
        if let DirectiveData::Transaction(ref mut txn) = wrapper.data {
            // Add "processed" tag
            if !txn.tags.contains(&"processed".to_string()) {
                txn.tags.push("processed".to_string());
            }

            // Check for large transactions
            for posting in &txn.postings {
                if let Some(ref units) = posting.units {
                    if let Ok(amount) = units.number.parse::<f64>() {
                        if amount.abs() > threshold {
                            errors.push(PluginError::warning(format!(
                                "Large transaction: {} {} in {} (threshold: {})",
                                units.number, units.currency, posting.account, threshold
                            )));
                        }
                    }
                }

                // Check expense accounts have expense-related tags
                if posting.account.starts_with("Expenses:") {
                    let has_expense_tag = txn
                        .tags
                        .iter()
                        .any(|t| t == "expense" || t == "deductible" || t == "business");
                    if !has_expense_tag && txn.tags.len() <= 1 {
                        errors.push(PluginError::warning(format!(
                            "Expense transaction without category tag: {}",
                            txn.narration
                        )));
                    }
                }
            }
        }

        directives.push(wrapper);
    }

    PluginOutput { directives, errors }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transaction(
        date: &str,
        payee: Option<&str>,
        narration: &str,
        account: &str,
        amount: &str,
    ) -> DirectiveWrapper {
        DirectiveWrapper {
            directive_type: String::new(),
            date: date.to_string(),
            filename: None,
            lineno: None,
            data: DirectiveData::Transaction(TransactionData {
                flag: "*".to_string(),
                payee: payee.map(String::from),
                narration: narration.to_string(),
                tags: vec![],
                links: vec![],
                metadata: vec![],
                postings: vec![PostingData {
                    account: account.to_string(),
                    units: Some(AmountData {
                        number: amount.to_string(),
                        currency: "USD".to_string(),
                    }),
                    cost: None,
                    price: None,
                    flag: None,
                    metadata: vec![],
                }],
            }),
        }
    }

    #[test]
    fn test_process_adds_tag() {
        let input = PluginInput {
            directives: vec![make_transaction(
                "2024-01-15",
                Some("Coffee Shop"),
                "Morning coffee",
                "Expenses:Food:Coffee",
                "5.00",
            )],
            options: PluginOptions {
                operating_currencies: vec!["USD".to_string()],
                title: None,
            },
            config: None,
        };

        let output = process_directives(input);

        assert_eq!(output.directives.len(), 1);
        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            assert!(txn.tags.contains(&"processed".to_string()));
        } else {
            panic!("Expected transaction");
        }
    }

    #[test]
    fn test_large_transaction_warning() {
        let input = PluginInput {
            directives: vec![make_transaction(
                "2024-01-15",
                None,
                "Big purchase",
                "Expenses:Shopping",
                "5000.00",
            )],
            options: PluginOptions {
                operating_currencies: vec!["USD".to_string()],
                title: None,
            },
            config: Some("threshold=1000".to_string()),
        };

        let output = process_directives(input);

        assert!(output
            .errors
            .iter()
            .any(|e| e.message.contains("Large transaction")));
    }
}
