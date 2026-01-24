//! Auto-generate currency trading account postings.

use crate::types::{DirectiveData, DirectiveWrapper, PluginInput, PluginOutput};

use super::super::NativePlugin;

/// Plugin that auto-generates currency trading account postings.
///
/// For multi-currency transactions, this plugin adds neutralizing postings
/// to equity accounts like `Equity:CurrencyAccounts:USD` to track currency
/// conversion gains/losses. This enables proper reporting of currency
/// trading activity.
pub struct CurrencyAccountsPlugin {
    /// Base account for currency tracking (default: "Equity:CurrencyAccounts").
    base_account: String,
}

impl CurrencyAccountsPlugin {
    /// Create with default base account.
    pub fn new() -> Self {
        Self {
            base_account: "Equity:CurrencyAccounts".to_string(),
        }
    }

    /// Create with custom base account.
    pub const fn with_base_account(base_account: String) -> Self {
        Self { base_account }
    }
}

impl Default for CurrencyAccountsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePlugin for CurrencyAccountsPlugin {
    fn name(&self) -> &'static str {
        "currency_accounts"
    }

    fn description(&self) -> &'static str {
        "Auto-generate currency trading postings"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        use crate::types::{AmountData, PostingData};
        use rust_decimal::Decimal;
        use std::collections::HashMap;
        use std::str::FromStr;

        // Get base account from config if provided
        let base_account = input
            .config
            .as_ref()
            .map_or_else(|| self.base_account.clone(), |c| c.trim().to_string());

        let mut new_directives: Vec<DirectiveWrapper> = Vec::new();

        for wrapper in &input.directives {
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                // Calculate currency totals for this transaction
                // Map from currency -> total amount in that currency
                let mut currency_totals: HashMap<String, Decimal> = HashMap::new();

                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        let amount = Decimal::from_str(&units.number).unwrap_or_default();
                        *currency_totals.entry(units.currency.clone()).or_default() += amount;
                    }
                }

                // If we have multiple currencies with non-zero totals, add balancing postings
                let non_zero_currencies: Vec<_> = currency_totals
                    .iter()
                    .filter(|&(_, total)| *total != Decimal::ZERO)
                    .collect();

                if non_zero_currencies.len() > 1 {
                    // Clone the transaction and add currency account postings
                    let mut modified_txn = txn.clone();

                    for &(currency, total) in &non_zero_currencies {
                        // Add posting to currency account to neutralize
                        modified_txn.postings.push(PostingData {
                            account: format!("{base_account}:{currency}"),
                            units: Some(AmountData {
                                number: (-*total).to_string(),
                                currency: (*currency).clone(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        });
                    }

                    new_directives.push(DirectiveWrapper {
                        directive_type: wrapper.directive_type.clone(),
                        date: wrapper.date.clone(),
                        filename: wrapper.filename.clone(), // Preserve original location
                        lineno: wrapper.lineno,
                        data: DirectiveData::Transaction(modified_txn),
                    });
                } else {
                    // Single currency or balanced - pass through
                    new_directives.push(wrapper.clone());
                }
            } else {
                new_directives.push(wrapper.clone());
            }
        }

        PluginOutput {
            directives: new_directives,
            errors: Vec::new(),
        }
    }
}

#[cfg(test)]
mod currency_accounts_tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_currency_accounts_adds_balancing_postings() {
        let plugin = CurrencyAccountsPlugin::new();

        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2024-01-15".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Transaction(TransactionData {
                    flag: "*".to_string(),
                    payee: None,
                    narration: "Currency exchange".to_string(),
                    tags: vec![],
                    links: vec![],
                    metadata: vec![],
                    postings: vec![
                        PostingData {
                            account: "Assets:Bank:USD".to_string(),
                            units: Some(AmountData {
                                number: "-100".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Assets:Bank:EUR".to_string(),
                            units: Some(AmountData {
                                number: "85".to_string(),
                                currency: "EUR".to_string(),
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
            // Should have original 2 postings + 2 currency account postings
            assert_eq!(txn.postings.len(), 4);

            // Check for currency account postings
            let usd_posting = txn
                .postings
                .iter()
                .find(|p| p.account == "Equity:CurrencyAccounts:USD");
            assert!(usd_posting.is_some());
            let usd_posting = usd_posting.unwrap();
            // Should neutralize the -100 USD
            assert_eq!(usd_posting.units.as_ref().unwrap().number, "100");

            let eur_posting = txn
                .postings
                .iter()
                .find(|p| p.account == "Equity:CurrencyAccounts:EUR");
            assert!(eur_posting.is_some());
            let eur_posting = eur_posting.unwrap();
            // Should neutralize the 85 EUR
            assert_eq!(eur_posting.units.as_ref().unwrap().number, "-85");
        } else {
            panic!("Expected Transaction directive");
        }
    }

    #[test]
    fn test_currency_accounts_single_currency_unchanged() {
        let plugin = CurrencyAccountsPlugin::new();

        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2024-01-15".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Transaction(TransactionData {
                    flag: "*".to_string(),
                    payee: None,
                    narration: "Simple transfer".to_string(),
                    tags: vec![],
                    links: vec![],
                    metadata: vec![],
                    postings: vec![
                        PostingData {
                            account: "Assets:Bank".to_string(),
                            units: Some(AmountData {
                                number: "-100".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Expenses:Food".to_string(),
                            units: Some(AmountData {
                                number: "100".to_string(),
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

        // Single currency balanced - should not add any postings
        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            assert_eq!(txn.postings.len(), 2);
        }
    }

    #[test]
    fn test_currency_accounts_custom_base_account() {
        let plugin = CurrencyAccountsPlugin::new();

        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2024-01-15".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Transaction(TransactionData {
                    flag: "*".to_string(),
                    payee: None,
                    narration: "Exchange".to_string(),
                    tags: vec![],
                    links: vec![],
                    metadata: vec![],
                    postings: vec![
                        PostingData {
                            account: "Assets:USD".to_string(),
                            units: Some(AmountData {
                                number: "-50".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Assets:EUR".to_string(),
                            units: Some(AmountData {
                                number: "42".to_string(),
                                currency: "EUR".to_string(),
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
            config: Some("Income:Trading".to_string()),
        };

        let output = plugin.process(input);
        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            // Check for custom base account
            assert!(
                txn.postings
                    .iter()
                    .any(|p| p.account.starts_with("Income:Trading:"))
            );
        }
    }
}
