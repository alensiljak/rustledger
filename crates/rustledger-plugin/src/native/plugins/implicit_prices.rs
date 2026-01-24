//! Plugin that generates price entries from transaction costs and prices.

use crate::types::{DirectiveWrapper, PluginInput, PluginOutput};

use super::super::NativePlugin;

/// Plugin that generates price entries from transaction costs and prices.
///
/// When a transaction has a posting with a cost or price annotation,
/// this plugin generates a corresponding Price directive.
pub struct ImplicitPricesPlugin;

impl NativePlugin for ImplicitPricesPlugin {
    fn name(&self) -> &'static str {
        "implicit_prices"
    }

    fn description(&self) -> &'static str {
        "Generate price entries from transaction costs/prices"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        let mut new_directives = Vec::new();
        let mut generated_prices = Vec::new();

        for wrapper in &input.directives {
            new_directives.push(wrapper.clone());

            // Only process transactions
            if wrapper.directive_type != "transaction" {
                continue;
            }

            // Extract prices from transaction data
            if let crate::types::DirectiveData::Transaction(ref txn) = wrapper.data {
                for posting in &txn.postings {
                    // Check for price annotation
                    if let Some(ref units) = posting.units {
                        if let Some(ref price) = posting.price {
                            // Generate a price directive only if we have a complete amount
                            if let Some(ref price_amount) = price.amount {
                                let price_wrapper = DirectiveWrapper {
                                    directive_type: "price".to_string(),
                                    date: wrapper.date.clone(),
                                    filename: None, // Plugin-generated
                                    lineno: None,
                                    data: crate::types::DirectiveData::Price(
                                        crate::types::PriceData {
                                            currency: units.currency.clone(),
                                            amount: price_amount.clone(),
                                            metadata: vec![],
                                        },
                                    ),
                                };
                                generated_prices.push(price_wrapper);
                            }
                        }

                        // Check for cost with price info
                        if let Some(ref cost) = posting.cost {
                            if let (Some(number), Some(currency)) =
                                (&cost.number_per, &cost.currency)
                            {
                                let price_wrapper = DirectiveWrapper {
                                    directive_type: "price".to_string(),
                                    date: wrapper.date.clone(),
                                    filename: None, // Plugin-generated
                                    lineno: None,
                                    data: crate::types::DirectiveData::Price(
                                        crate::types::PriceData {
                                            currency: units.currency.clone(),
                                            amount: crate::types::AmountData {
                                                number: number.clone(),
                                                currency: currency.clone(),
                                            },
                                            metadata: vec![],
                                        },
                                    ),
                                };
                                generated_prices.push(price_wrapper);
                            }
                        }
                    }
                }
            }
        }

        // Add generated prices
        new_directives.extend(generated_prices);

        PluginOutput {
            directives: new_directives,
            errors: Vec::new(),
        }
    }
}
