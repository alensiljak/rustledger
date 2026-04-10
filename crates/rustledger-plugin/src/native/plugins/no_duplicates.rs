//! Hash-based duplicate transaction detection.
//!
//! Mirrors Python beancount's `beancount.plugins.noduplicates`, which uses
//! `beancount.core.compare.hash_entry` to identify structurally identical
//! transactions. `hash_entry` hashes every field that contributes to a
//! transaction's structural identity: flag, payee, narration, tags, links,
//! and each posting's account, units, cost, price, and flag. Metadata is
//! deliberately excluded (beancount's `hash_entry` passes `exclude_meta=True`).
//!
//! The hash helpers below use exhaustive struct destructuring so that adding
//! a field to `TransactionData`, `PostingData`, `CostData`, `AmountData`, or
//! `PriceAnnotationData` causes a compile error here — forcing whoever adds
//! the field to explicitly decide whether it contributes to structural
//! identity (add to the hash) or not (bind with `_` and document why).

use crate::types::{
    AmountData, CostData, DirectiveData, PluginError, PluginInput, PluginOutput, PostingData,
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

        // Sentinel bytes used to discriminate `None` from `Some` before each
        // optional component. Otherwise `(None, Some(x))` and `(Some(x), None)`
        // could collide for adjacent fields. Python's tuple hash achieves the
        // equivalent via `hash(None)` being a distinct fixed value.
        const ABSENT: u8 = 0;
        const PRESENT: u8 = 1;

        fn hash_amount<H: Hasher>(amount: &AmountData, hasher: &mut H) {
            let AmountData { number, currency } = amount;
            number.hash(hasher);
            currency.hash(hasher);
        }

        fn hash_cost<H: Hasher>(cost: &CostData, hasher: &mut H) {
            let CostData {
                number_per,
                number_total,
                currency,
                date,
                label,
                merge,
            } = cost;
            number_per.hash(hasher);
            number_total.hash(hasher);
            currency.hash(hasher);
            date.hash(hasher);
            label.hash(hasher);
            merge.hash(hasher);
        }

        fn hash_price<H: Hasher>(price: &PriceAnnotationData, hasher: &mut H) {
            let PriceAnnotationData {
                is_total,
                amount,
                number,
                currency,
            } = price;
            is_total.hash(hasher);
            match amount {
                Some(a) => {
                    PRESENT.hash(hasher);
                    hash_amount(a, hasher);
                }
                None => ABSENT.hash(hasher),
            }
            number.hash(hasher);
            currency.hash(hasher);
        }

        fn hash_posting<H: Hasher>(posting: &PostingData, hasher: &mut H) {
            // Destructure so any future field added to `PostingData` causes a
            // compile error here and the maintainer must explicitly decide
            // whether it's part of structural identity.
            let PostingData {
                account,
                units,
                cost,
                price,
                flag,
                // Metadata is intentionally NOT hashed — matches beancount's
                // hash_entry(exclude_meta=True) default. Bind to `_` so adding
                // a new field in the future is still a compile error.
                metadata: _,
            } = posting;

            account.hash(hasher);
            match units {
                Some(u) => {
                    PRESENT.hash(hasher);
                    hash_amount(u, hasher);
                }
                None => ABSENT.hash(hasher),
            }
            match cost {
                Some(c) => {
                    PRESENT.hash(hasher);
                    hash_cost(c, hasher);
                }
                None => ABSENT.hash(hasher),
            }
            match price {
                Some(p) => {
                    PRESENT.hash(hasher);
                    hash_price(p, hasher);
                }
                None => ABSENT.hash(hasher),
            }
            flag.hash(hasher);
        }

        fn hash_transaction(date: &str, txn: &TransactionData) -> u64 {
            // Destructure so any future field added to `TransactionData`
            // causes a compile error here.
            let TransactionData {
                flag,
                payee,
                narration,
                tags,
                links,
                // Metadata is intentionally NOT hashed — matches beancount's
                // hash_entry(exclude_meta=True) default.
                metadata: _,
                postings,
            } = txn;

            let mut hasher = DefaultHasher::new();
            date.hash(&mut hasher);
            flag.hash(&mut hasher);
            payee.hash(&mut hasher);
            narration.hash(&mut hasher);

            // Tags and links are unordered sets in beancount (`frozenset`),
            // so:
            //   1. Sort + dedup so the hash is stable regardless of parser
            //      order and collapses any accidental duplicates the parser
            //      might emit (matching beancount set semantics).
            //   2. Each collection is prefixed with its length so the two
            //      streams can't be merged or swapped without changing the
            //      resulting hash — e.g. `tags={a,b}, links={}` no longer
            //      collides with `tags={a}, links={b}`.
            let mut sorted_tags: Vec<&String> = tags.iter().collect();
            sorted_tags.sort();
            sorted_tags.dedup();
            sorted_tags.len().hash(&mut hasher);
            for tag in sorted_tags {
                tag.hash(&mut hasher);
            }

            let mut sorted_links: Vec<&String> = links.iter().collect();
            sorted_links.sort();
            sorted_links.dedup();
            sorted_links.len().hash(&mut hasher);
            for link in sorted_links {
                link.hash(&mut hasher);
            }

            // Prefix postings with their count so the posting stream can't
            // collide with trailing fields of the set streams above.
            postings.len().hash(&mut hasher);
            for posting in postings {
                hash_posting(posting, &mut hasher);
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
