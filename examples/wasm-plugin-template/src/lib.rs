//! Template WASM Plugin for rustledger
//!
//! This is a minimal template for creating rustledger WASM plugins.
//! It demonstrates the required exports and basic structure.
//!
//! # Building
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! cargo build --target wasm32-unknown-unknown --release
//! ```
//!
//! The plugin will be at: `target/wasm32-unknown-unknown/release/example_plugin.wasm`
//!
//! # Using in a Beancount File
//!
//! ```beancount
//! plugin "path/to/example_plugin.wasm"
//!
//! 2024-01-01 open Assets:Bank USD
//! ```

use rustledger_plugin_types::*;

// ============================================================================
// Required Exports
// ============================================================================

/// Memory allocator - required by the host to pass data to the plugin.
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Plugin entry point - called by the host with MessagePack-encoded input.
///
/// Returns a packed u64: (output_ptr << 32) | output_len
#[no_mangle]
pub extern "C" fn process(input_ptr: u32, input_len: u32) -> u64 {
    // Read input from WASM memory
    let input_bytes = unsafe {
        std::slice::from_raw_parts(input_ptr as *const u8, input_len as usize)
    };

    // Deserialize input
    let input: PluginInput = match rmp_serde::from_slice(input_bytes) {
        Ok(i) => i,
        Err(e) => return pack_error(&format!("Failed to deserialize: {}", e)),
    };

    // Process directives (customize this!)
    let output = process_directives(input);

    // Serialize and return output
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

/// Helper to return an error result.
fn pack_error(message: &str) -> u64 {
    let output = PluginOutput {
        directives: vec![],
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
// Plugin Logic - Customize This!
// ============================================================================

/// Main processing function.
///
/// This example adds a "processed" tag to all transactions.
fn process_directives(input: PluginInput) -> PluginOutput {
    let mut directives = Vec::new();

    for mut wrapper in input.directives {
        // Example: add a tag to all transactions
        if let DirectiveData::Transaction(ref mut txn) = wrapper.data {
            if !txn.tags.contains(&"processed".to_string()) {
                txn.tags.push("processed".to_string());
            }
        }
        directives.push(wrapper);
    }

    PluginOutput {
        directives,
        errors: vec![],
    }
}
