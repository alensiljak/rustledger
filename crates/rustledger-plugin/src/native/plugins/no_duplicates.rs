//! Hash-based duplicate transaction detection.
//!
//! Mirrors Python beancount's `beancount.plugins.noduplicates`, which uses
//! `beancount.core.compare.hash_entry` to identify structurally identical
//! transactions. `hash_entry` hashes every field that contributes to a
//! transaction's structural identity: flag, payee, narration, tags, links,
//! and each posting's account, units, cost, price, and flag. Metadata is
//! deliberately excluded (beancount's `hash_entry` passes `exclude_meta=True`).

use crate::types::{
    CostData, DirectiveData, PluginError, PluginInput, PluginOutput, PostingData,
    PriceAnnotationData, TransactionData,
};

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

        // Hash a cost spec field-by-field so two cost specs with identical
        // contents but independently allocated `Option`s hash to the same
        // value.
        fn hash_cost<H: Hasher>(cost: &CostData, hasher: &mut H) {
            cost.number_per.hash(hasher);
            cost.number_total.hash(hasher);
            cost.currency.hash(hasher);
            cost.date.hash(hasher);
            cost.label.hash(hasher);
            cost.merge.hash(hasher);
        }

        fn hash_price<H: Hasher>(price: &PriceAnnotationData, hasher: &mut H) {
            price.is_total.hash(hasher);
            if let Some(amount) = &price.amount {
                amount.number.hash(hasher);
                amount.currency.hash(hasher);
            }
            price.number.hash(hasher);
            price.currency.hash(hasher);
        }

        fn hash_posting<H: Hasher>(posting: &PostingData, hasher: &mut H) {
            posting.account.hash(hasher);
            // Discriminate None from Some by hashing a sentinel before each
            // optional component — otherwise `(None, Some(x))` and
            // `(Some(x), None)` could collide for adjacent fields. Python's
            // tuple hash naturally does the equivalent via `hash(None)`.
            match &posting.units {
                Some(units) => {
                    1u8.hash(hasher);
                    units.number.hash(hasher);
                    units.currency.hash(hasher);
                }
                None => 0u8.hash(hasher),
            }
            match &posting.cost {
                Some(cost) => {
                    1u8.hash(hasher);
                    hash_cost(cost, hasher);
                }
                None => 0u8.hash(hasher),
            }
            match &posting.price {
                Some(price) => {
                    1u8.hash(hasher);
                    hash_price(price, hasher);
                }
                None => 0u8.hash(hasher),
            }
            posting.flag.hash(hasher);
        }

        fn hash_transaction(date: &str, txn: &TransactionData) -> u64 {
            let mut hasher = DefaultHasher::new();
            date.hash(&mut hasher);
            txn.flag.hash(&mut hasher);
            txn.payee.hash(&mut hasher);
            txn.narration.hash(&mut hasher);

            // Tags and links are unordered sets in beancount; sort so the
            // hash is stable regardless of the order the parser emitted them.
            let mut tags: Vec<&String> = txn.tags.iter().collect();
            tags.sort();
            for tag in tags {
                tag.hash(&mut hasher);
            }
            let mut links: Vec<&String> = txn.links.iter().collect();
            links.sort();
            for link in links {
                link.hash(&mut hasher);
            }

            for posting in &txn.postings {
                hash_posting(posting, &mut hasher);
            }

            // Metadata is intentionally NOT hashed: beancount's hash_entry
            // defaults to exclude_meta=True for the noduplicates plugin.
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
