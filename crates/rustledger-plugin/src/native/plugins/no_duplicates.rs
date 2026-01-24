//! Hash-based duplicate transaction detection.

use crate::types::{DirectiveData, PluginError, PluginInput, PluginOutput, TransactionData};

use super::super::NativePlugin;

/// Plugin that detects duplicate transactions based on hash.
pub struct NoDuplicatesPlugin;

impl NativePlugin for NoDuplicatesPlugin {
    fn name(&self) -> &'static str {
        "noduplicates"
    }

    fn description(&self) -> &'static str {
        "Hash-based duplicate transaction detection"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        use std::collections::HashSet;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_transaction(date: &str, txn: &TransactionData) -> u64 {
            let mut hasher = DefaultHasher::new();
            date.hash(&mut hasher);
            txn.narration.hash(&mut hasher);
            txn.payee.hash(&mut hasher);
            for posting in &txn.postings {
                posting.account.hash(&mut hasher);
                if let Some(units) = &posting.units {
                    units.number.hash(&mut hasher);
                    units.currency.hash(&mut hasher);
                }
            }
            hasher.finish()
        }

        let mut seen: HashSet<u64> = HashSet::new();
        let mut errors = Vec::new();

        for wrapper in &input.directives {
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                let hash = hash_transaction(&wrapper.date, txn);
                if !seen.insert(hash) {
                    errors.push(PluginError::error(format!(
                        "Duplicate transaction: {} \"{}\"",
                        wrapper.date, txn.narration
                    )));
                }
            }
        }

        PluginOutput {
            directives: input.directives,
            errors,
        }
    }
}
