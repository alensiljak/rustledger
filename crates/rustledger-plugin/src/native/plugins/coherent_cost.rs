//! Enforce cost OR price (not both) consistency.

use crate::types::{DirectiveData, PluginError, PluginInput, PluginOutput};

use super::super::NativePlugin;

/// Plugin that ensures currencies use cost OR price consistently, never both.
///
/// If a currency is used with cost notation `{...}`, it should not also be used
/// with price notation `@` in the same ledger, as this can lead to inconsistencies.
pub struct CoherentCostPlugin;

impl NativePlugin for CoherentCostPlugin {
    fn name(&self) -> &'static str {
        "coherent_cost"
    }

    fn description(&self) -> &'static str {
        "Enforce cost OR price (not both) consistency"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        use std::collections::{HashMap, HashSet};

        // Track which currencies are used with cost vs price
        let mut currencies_with_cost: HashSet<String> = HashSet::new();
        let mut currencies_with_price: HashSet<String> = HashSet::new();
        let mut first_use: HashMap<String, (String, String)> = HashMap::new(); // currency -> (type, date)

        for wrapper in &input.directives {
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        let currency = &units.currency;

                        if posting.cost.is_some() && !currencies_with_cost.contains(currency) {
                            currencies_with_cost.insert(currency.clone());
                            first_use
                                .entry(currency.clone())
                                .or_insert(("cost".to_string(), wrapper.date.clone()));
                        }

                        if posting.price.is_some() && !currencies_with_price.contains(currency) {
                            currencies_with_price.insert(currency.clone());
                            first_use
                                .entry(currency.clone())
                                .or_insert(("price".to_string(), wrapper.date.clone()));
                        }
                    }
                }
            }
        }

        // Find currencies used with both
        let mut errors = Vec::new();
        for currency in currencies_with_cost.intersection(&currencies_with_price) {
            errors.push(PluginError::error(format!(
                "Currency '{currency}' is used with both cost and price notation - this may cause inconsistencies"
            )));
        }

        PluginOutput {
            directives: input.directives,
            errors,
        }
    }
}
