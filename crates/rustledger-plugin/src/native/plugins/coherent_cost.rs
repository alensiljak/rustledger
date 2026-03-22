//! Enforce cost OR price (not both) consistency.

use crate::types::{DirectiveData, PluginError, PluginInput, PluginOutput};

use super::super::NativePlugin;

/// Plugin that ensures currencies use cost OR price consistently, never both.
///
/// If a currency is used with cost notation `{...}` in some postings, it should
/// not be used with price-only notation `@` (without cost) in other postings,
/// as this indicates inconsistent tracking.
///
/// Note: Having BOTH cost AND price on the same posting is valid and common
/// when selling positions (cost = acquisition price, price = sale price).
pub struct CoherentCostPlugin;

impl NativePlugin for CoherentCostPlugin {
    fn name(&self) -> &'static str {
        "coherent_cost"
    }

    fn description(&self) -> &'static str {
        "Enforce cost OR price (not both) consistency"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        use std::collections::HashSet;

        // Track currencies used with cost (with or without price)
        let mut currencies_with_cost: HashSet<String> = HashSet::new();
        // Track currencies used with price-only (no cost)
        let mut currencies_with_price_only: HashSet<String> = HashSet::new();

        for wrapper in &input.directives {
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        let currency = &units.currency;

                        // Check if this posting has cost
                        if posting.cost.is_some() {
                            currencies_with_cost.insert(currency.clone());
                        } else if posting.price.is_some() {
                            // Price-only (no cost) - this is the problematic case
                            currencies_with_price_only.insert(currency.clone());
                        }
                    }
                }
            }
        }

        // Find currencies used with cost in some places and price-only in others
        let mut errors = Vec::new();
        for currency in currencies_with_cost.intersection(&currencies_with_price_only) {
            errors.push(PluginError::error(format!(
                "Currency '{currency}' is used with both cost and price-only notation - this may cause inconsistencies"
            )));
        }

        PluginOutput {
            directives: input.directives,
            errors,
        }
    }
}
