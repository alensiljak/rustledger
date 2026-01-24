//! Plugin that automatically adds tags based on account patterns.

use crate::types::{DirectiveData, PluginInput, PluginOutput};

use super::super::NativePlugin;

/// Plugin that automatically adds tags based on account patterns.
///
/// This is an example plugin showing how to implement custom tagging logic.
/// It can be configured with rules like:
/// - "Expenses:Food" -> #food
/// - "Expenses:Travel" -> #travel
/// - "Assets:Bank" -> #banking
pub struct AutoTagPlugin {
    /// Rules mapping account prefixes to tags.
    rules: Vec<(String, String)>,
}

impl AutoTagPlugin {
    /// Create with default rules.
    pub fn new() -> Self {
        Self {
            rules: vec![
                ("Expenses:Food".to_string(), "food".to_string()),
                ("Expenses:Travel".to_string(), "travel".to_string()),
                ("Expenses:Transport".to_string(), "transport".to_string()),
                ("Income:Salary".to_string(), "income".to_string()),
            ],
        }
    }

    /// Create with custom rules.
    pub const fn with_rules(rules: Vec<(String, String)>) -> Self {
        Self { rules }
    }
}

impl Default for AutoTagPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePlugin for AutoTagPlugin {
    fn name(&self) -> &'static str {
        "auto_tag"
    }

    fn description(&self) -> &'static str {
        "Auto-tag transactions by account patterns"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        let directives: Vec<_> = input
            .directives
            .into_iter()
            .map(|mut wrapper| {
                if wrapper.directive_type == "transaction" {
                    if let DirectiveData::Transaction(ref mut txn) = wrapper.data {
                        // Check each posting against rules
                        for posting in &txn.postings {
                            for (prefix, tag) in &self.rules {
                                if posting.account.starts_with(prefix) {
                                    // Add tag if not already present
                                    if !txn.tags.contains(tag) {
                                        txn.tags.push(tag.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                wrapper
            })
            .collect();

        PluginOutput {
            directives,
            errors: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_auto_tag_adds_tag() {
        let plugin = AutoTagPlugin::new();

        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2024-01-15".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Transaction(TransactionData {
                    flag: "*".to_string(),
                    payee: None,
                    narration: "Lunch".to_string(),
                    tags: vec![],
                    links: vec![],
                    metadata: vec![],
                    postings: vec![
                        PostingData {
                            account: "Expenses:Food:Restaurants".to_string(),
                            units: Some(AmountData {
                                number: "25.00".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Assets:Cash".to_string(),
                            units: Some(AmountData {
                                number: "-25.00".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                    ],
                }),
            }],
            options: PluginOptions {
                operating_currencies: vec!["USD".to_string()],
                title: None,
            },
            config: None,
        };

        let output = plugin.process(input);
        assert_eq!(output.errors.len(), 0);
        assert_eq!(output.directives.len(), 1);

        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            assert!(txn.tags.contains(&"food".to_string()));
        } else {
            panic!("Expected transaction");
        }
    }
}
