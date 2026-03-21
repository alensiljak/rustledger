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
        use crate::types::{AmountData, OpenData, PostingData};
        use rust_decimal::Decimal;
        use std::collections::{HashMap, HashSet};
        use std::str::FromStr;

        // Get base account from config if provided
        let base_account = input
            .config
            .as_ref()
            .map_or_else(|| self.base_account.clone(), |c| c.trim().to_string());

        // Pre-allocate with expected capacity
        let mut new_directives: Vec<DirectiveWrapper> = Vec::with_capacity(input.directives.len());
        let mut created_accounts: HashSet<String> = HashSet::new();

        // Single pass: collect existing opens AND find earliest date
        let mut existing_opens: HashSet<String> = HashSet::new();
        let mut earliest_date: Option<&str> = None;
        for wrapper in &input.directives {
            // Track earliest date
            match earliest_date {
                None => earliest_date = Some(&wrapper.date),
                Some(current) if wrapper.date.as_str() < current => {
                    earliest_date = Some(&wrapper.date);
                }
                _ => {}
            }
            // Collect existing Open accounts
            if let DirectiveData::Open(open) = &wrapper.data {
                existing_opens.insert(open.account.clone());
            }
        }
        let earliest_date = earliest_date.unwrap_or("1970-01-01").to_string();

        for wrapper in &input.directives {
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                // Calculate currency totals for this transaction
                // Map from currency -> total amount in that currency
                // Like Python beancount, we use the WEIGHT currency (cost currency if present)
                let mut currency_totals: HashMap<String, Decimal> = HashMap::new();

                for posting in &txn.postings {
                    if let Some(units) = &posting.units {
                        let units_amount = Decimal::from_str(&units.number).unwrap_or_default();

                        // Determine the weight currency and amount
                        // If posting has a cost, use cost currency and calculate cost amount
                        // Otherwise use units currency and amount
                        let (currency, amount) = if let Some(cost) = &posting.cost {
                            if let Some(cost_currency) = &cost.currency {
                                // Calculate cost amount
                                let cost_amount = if let Some(num_per) = &cost.number_per {
                                    // Per-unit cost: units * cost_per_unit
                                    let per_unit =
                                        Decimal::from_str(num_per).unwrap_or(Decimal::ONE);
                                    units_amount * per_unit
                                } else if let Some(num_total) = &cost.number_total {
                                    // Total cost specified directly
                                    Decimal::from_str(num_total).unwrap_or_default()
                                } else {
                                    // No cost number, fall back to units
                                    units_amount
                                };
                                (cost_currency.clone(), cost_amount)
                            } else {
                                // Cost exists but no currency - fall back to units
                                (units.currency.clone(), units_amount)
                            }
                        } else if let Some(price) = &posting.price {
                            // Price annotation (@) - use price currency for weight
                            if let Some(price_amount) = &price.amount {
                                let price_currency = price_amount.currency.clone();
                                let price_num =
                                    Decimal::from_str(&price_amount.number).unwrap_or(Decimal::ONE);
                                let weight = if price.is_total {
                                    // Total price (@@): weight is the price amount directly
                                    // But sign follows units
                                    if units_amount < Decimal::ZERO {
                                        -price_num
                                    } else {
                                        price_num
                                    }
                                } else {
                                    // Per-unit price (@): weight = units * price
                                    units_amount * price_num
                                };
                                (price_currency, weight)
                            } else {
                                // Incomplete price - fall back to units
                                (units.currency.clone(), units_amount)
                            }
                        } else {
                            // No cost or price - use units directly
                            (units.currency.clone(), units_amount)
                        };

                        *currency_totals.entry(currency).or_default() += amount;
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
                        let account_name = format!("{base_account}:{currency}");
                        // Track the account for Open directive generation
                        created_accounts.insert(account_name.clone());

                        // Add posting to currency account to neutralize
                        modified_txn.postings.push(PostingData {
                            account: account_name,
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

        // Generate Open directives for created currency accounts (skip existing ones)
        let mut open_directives: Vec<DirectiveWrapper> = created_accounts
            .into_iter()
            .filter(|account| !existing_opens.contains(account))
            .map(|account| DirectiveWrapper {
                directive_type: "open".to_string(),
                date: earliest_date.clone(),
                filename: Some("<currency_accounts>".to_string()),
                lineno: None,
                data: DirectiveData::Open(OpenData {
                    account,
                    currencies: vec![],
                    booking: None,
                    metadata: vec![],
                }),
            })
            .collect();

        // Sort for deterministic output
        open_directives.sort_by(|a, b| {
            if let (DirectiveData::Open(oa), DirectiveData::Open(ob)) = (&a.data, &b.data) {
                oa.account.cmp(&ob.account)
            } else {
                std::cmp::Ordering::Equal
            }
        });

        // Prepend Open directives to the output
        open_directives.extend(new_directives);

        PluginOutput {
            directives: open_directives,
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
        // Should have 2 Open directives + 1 Transaction
        assert_eq!(output.directives.len(), 3);

        // First two should be Open directives (sorted alphabetically)
        if let DirectiveData::Open(open) = &output.directives[0].data {
            assert_eq!(open.account, "Equity:CurrencyAccounts:EUR");
            assert_eq!(output.directives[0].date, "2024-01-15");
        } else {
            panic!("Expected Open directive at index 0");
        }

        if let DirectiveData::Open(open) = &output.directives[1].data {
            assert_eq!(open.account, "Equity:CurrencyAccounts:USD");
            assert_eq!(output.directives[1].date, "2024-01-15");
        } else {
            panic!("Expected Open directive at index 1");
        }

        // Last should be the transaction
        if let DirectiveData::Transaction(txn) = &output.directives[2].data {
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
            panic!("Expected Transaction directive at index 2");
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
        // Should have 2 Open directives + 1 Transaction
        assert_eq!(output.directives.len(), 3);

        // Check Open directives use custom base account
        assert!(output.directives.iter().any(|d| {
            if let DirectiveData::Open(open) = &d.data {
                open.account.starts_with("Income:Trading:")
            } else {
                false
            }
        }));

        // Transaction is at index 2
        if let DirectiveData::Transaction(txn) = &output.directives[2].data {
            // Check for custom base account in postings
            assert!(
                txn.postings
                    .iter()
                    .any(|p| p.account.starts_with("Income:Trading:"))
            );
        } else {
            panic!("Expected Transaction directive at index 2");
        }
    }

    #[test]
    fn test_currency_accounts_open_directives_use_earliest_date() {
        let plugin = CurrencyAccountsPlugin::new();

        let input = PluginInput {
            directives: vec![
                DirectiveWrapper {
                    directive_type: "transaction".to_string(),
                    date: "2024-03-15".to_string(),
                    filename: None,
                    lineno: None,
                    data: DirectiveData::Transaction(TransactionData {
                        flag: "*".to_string(),
                        payee: None,
                        narration: "Later exchange".to_string(),
                        tags: vec![],
                        links: vec![],
                        metadata: vec![],
                        postings: vec![
                            PostingData {
                                account: "Assets:USD".to_string(),
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
                                account: "Assets:EUR".to_string(),
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
                },
                DirectiveWrapper {
                    directive_type: "transaction".to_string(),
                    date: "2024-01-01".to_string(), // Earlier date
                    filename: None,
                    lineno: None,
                    data: DirectiveData::Transaction(TransactionData {
                        flag: "*".to_string(),
                        payee: None,
                        narration: "Earlier exchange".to_string(),
                        tags: vec![],
                        links: vec![],
                        metadata: vec![],
                        postings: vec![
                            PostingData {
                                account: "Assets:GBP".to_string(),
                                units: Some(AmountData {
                                    number: "-50".to_string(),
                                    currency: "GBP".to_string(),
                                }),
                                cost: None,
                                price: None,
                                flag: None,
                                metadata: vec![],
                            },
                            PostingData {
                                account: "Assets:JPY".to_string(),
                                units: Some(AmountData {
                                    number: "7500".to_string(),
                                    currency: "JPY".to_string(),
                                }),
                                cost: None,
                                price: None,
                                flag: None,
                                metadata: vec![],
                            },
                        ],
                    }),
                },
            ],
            options: PluginOptions {
                operating_currencies: vec!["USD".to_string()],
                title: None,
            },
            config: None,
        };

        let output = plugin.process(input);
        // Should have 4 Open directives (EUR, GBP, JPY, USD) + 2 Transactions
        assert_eq!(output.directives.len(), 6);

        // All Open directives should use the earliest date (2024-01-01)
        for wrapper in &output.directives[..4] {
            if let DirectiveData::Open(_) = &wrapper.data {
                assert_eq!(
                    wrapper.date, "2024-01-01",
                    "Open directive should use earliest date"
                );
            }
        }
    }

    #[test]
    fn test_currency_accounts_uses_cost_currency() {
        // Issue #521/#531: When a posting has a cost, use the cost currency
        // for grouping, not the units currency
        let plugin = CurrencyAccountsPlugin::new();

        // Transaction: Buy 9 RING at 68.55 USD each
        // All postings should be grouped under USD (the cost currency)
        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2026-03-21".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Transaction(TransactionData {
                    flag: "*".to_string(),
                    payee: Some("Buy RING".to_string()),
                    narration: String::new(),
                    tags: vec![],
                    links: vec![],
                    metadata: vec![],
                    postings: vec![
                        PostingData {
                            account: "Assets:Shares:RING".to_string(),
                            units: Some(AmountData {
                                number: "9".to_string(),
                                currency: "RING".to_string(),
                            }),
                            cost: Some(CostData {
                                number_per: Some("68.55".to_string()),
                                number_total: None,
                                currency: Some("USD".to_string()),
                                date: None,
                                label: None,
                                merge: false,
                            }),
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Expenses:Financial".to_string(),
                            units: Some(AmountData {
                                number: "0.35".to_string(),
                                currency: "USD".to_string(),
                            }),
                            cost: None,
                            price: None,
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Assets:Cash:USD".to_string(),
                            units: Some(AmountData {
                                number: "-617.30".to_string(),
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

        // All postings have cost/units in USD, so NO currency account postings should be added
        // The transaction should pass through unchanged (just 1 directive)
        assert_eq!(output.directives.len(), 1);

        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            // Should have the original 3 postings only
            assert_eq!(txn.postings.len(), 3);
        } else {
            panic!("Expected Transaction directive");
        }
    }

    #[test]
    fn test_currency_accounts_uses_price_currency() {
        // When a posting has a price (@), use the price currency for grouping
        let plugin = CurrencyAccountsPlugin::new();

        // Transaction: -100 EUR @ 1.10 USD, +110 USD
        // Both should be grouped under USD (price currency for first, units for second)
        let input = PluginInput {
            directives: vec![DirectiveWrapper {
                directive_type: "transaction".to_string(),
                date: "2026-03-17".to_string(),
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
                            account: "Assets:Bank:EUR".to_string(),
                            units: Some(AmountData {
                                number: "-100".to_string(),
                                currency: "EUR".to_string(),
                            }),
                            cost: None,
                            price: Some(PriceAnnotationData {
                                is_total: false,
                                amount: Some(AmountData {
                                    number: "1.10".to_string(),
                                    currency: "USD".to_string(),
                                }),
                                number: None,
                                currency: None,
                            }),
                            flag: None,
                            metadata: vec![],
                        },
                        PostingData {
                            account: "Assets:Bank:USD".to_string(),
                            units: Some(AmountData {
                                number: "110".to_string(),
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

        // Both postings have weight in USD:
        // -100 EUR @ 1.10 USD = -110 USD weight
        // +110 USD = +110 USD weight
        // Total: 0 USD - balanced, NO currency account postings needed
        assert_eq!(output.directives.len(), 1);

        if let DirectiveData::Transaction(txn) = &output.directives[0].data {
            // Should have the original 2 postings only
            assert_eq!(txn.postings.len(), 2);
        } else {
            panic!("Expected Transaction directive");
        }
    }

    #[test]
    fn test_currency_accounts_skips_existing_open() {
        // When user already has Open directive for currency account,
        // plugin should NOT create a duplicate (would cause E1002)
        let plugin = CurrencyAccountsPlugin::new();

        let input = PluginInput {
            directives: vec![
                // Pre-existing Open for the currency account
                DirectiveWrapper {
                    directive_type: "open".to_string(),
                    date: "2024-01-01".to_string(),
                    filename: None,
                    lineno: None,
                    data: DirectiveData::Open(OpenData {
                        account: "Equity:CurrencyAccounts:USD".to_string(),
                        currencies: vec![],
                        booking: None,
                        metadata: vec![],
                    }),
                },
                DirectiveWrapper {
                    directive_type: "open".to_string(),
                    date: "2024-01-01".to_string(),
                    filename: None,
                    lineno: None,
                    data: DirectiveData::Open(OpenData {
                        account: "Assets:Bank:EUR".to_string(),
                        currencies: vec![],
                        booking: None,
                        metadata: vec![],
                    }),
                },
                DirectiveWrapper {
                    directive_type: "open".to_string(),
                    date: "2024-01-01".to_string(),
                    filename: None,
                    lineno: None,
                    data: DirectiveData::Open(OpenData {
                        account: "Assets:Bank:USD".to_string(),
                        currencies: vec![],
                        booking: None,
                        metadata: vec![],
                    }),
                },
                // Multi-currency transaction
                DirectiveWrapper {
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
                },
            ],
            options: PluginOptions {
                operating_currencies: vec!["USD".to_string()],
                title: None,
            },
            config: None,
        };

        let output = plugin.process(input);
        assert_eq!(output.errors.len(), 0);

        // Should have:
        // - 1 new Open for Equity:CurrencyAccounts:EUR (USD already exists)
        // - 3 original Open directives
        // - 1 modified Transaction
        assert_eq!(output.directives.len(), 5);

        // Count Open directives for currency accounts
        let currency_account_opens: Vec<_> = output
            .directives
            .iter()
            .filter_map(|d| {
                if let DirectiveData::Open(open) = &d.data {
                    if open.account.starts_with("Equity:CurrencyAccounts:") {
                        Some(open.account.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Should have exactly 2 currency account Opens (USD from input, EUR generated)
        assert_eq!(currency_account_opens.len(), 2);
        assert!(currency_account_opens.contains(&"Equity:CurrencyAccounts:USD".to_string()));
        assert!(currency_account_opens.contains(&"Equity:CurrencyAccounts:EUR".to_string()));
    }
}
