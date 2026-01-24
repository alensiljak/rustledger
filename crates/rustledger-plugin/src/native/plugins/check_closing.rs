//! Zero balance assertion on account closing.

use crate::types::{
    AmountData, BalanceData, DirectiveData, DirectiveWrapper, MetaValueData, PluginInput,
    PluginOutput, sort_directives,
};

use super::super::NativePlugin;
use super::utils::increment_date;

/// Plugin that inserts zero balance assertion when posting has `closing: TRUE` metadata.
///
/// When a posting has metadata `closing: TRUE`, this plugin adds a balance assertion
/// for that account with zero balance on the next day.
pub struct CheckClosingPlugin;

impl NativePlugin for CheckClosingPlugin {
    fn name(&self) -> &'static str {
        "check_closing"
    }

    fn description(&self) -> &'static str {
        "Zero balance assertion on account closing"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        let mut new_directives: Vec<DirectiveWrapper> = Vec::new();

        for wrapper in &input.directives {
            new_directives.push(wrapper.clone());

            if let DirectiveData::Transaction(txn) = &wrapper.data {
                for posting in &txn.postings {
                    // Check for closing: TRUE metadata
                    let has_closing = posting.metadata.iter().any(|(key, val)| {
                        key == "closing" && matches!(val, MetaValueData::Bool(true))
                    });

                    if has_closing {
                        // Parse the date and add one day
                        if let Some(next_date) = increment_date(&wrapper.date) {
                            // Get the currency from the posting
                            let currency = posting
                                .units
                                .as_ref()
                                .map_or_else(|| "USD".to_string(), |u| u.currency.clone());

                            // Add zero balance assertion
                            new_directives.push(DirectiveWrapper {
                                directive_type: "balance".to_string(),
                                date: next_date,
                                filename: None, // Plugin-generated
                                lineno: None,
                                data: DirectiveData::Balance(BalanceData {
                                    account: posting.account.clone(),
                                    amount: AmountData {
                                        number: "0".to_string(),
                                        currency,
                                    },
                                    tolerance: None,
                                    metadata: vec![],
                                }),
                            });
                        }
                    }
                }
            }
        }

        // Sort using beancount's standard ordering
        sort_directives(&mut new_directives);

        PluginOutput {
            directives: new_directives,
            errors: Vec::new(),
        }
    }
}
