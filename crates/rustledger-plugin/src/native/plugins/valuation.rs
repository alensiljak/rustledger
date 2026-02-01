//! Valuation plugin - track opaque fund values using synthetic commodities.
//!
//! This plugin allows specifying total investment account value over time and
//! creates an underlying fictional commodity whose price is set to match the
//! total value of the account.
//!
//! All incoming and outgoing transactions are converted into transactions
//! buying and selling this commodity at a calculated price.
//!
//! Usage:
//! ```beancount
//! plugin "beancount_lazy_plugins.valuation"
//!
//! 1970-01-01 open Assets:Fund:Total "FIFO"
//! 1970-01-01 open Income:Fund:PnL
//!
//! 1970-01-01 custom "valuation" "config" account: "Assets:Fund:Total" currency: "FUND_USD" pnlAccount: "Income:Fund:PnL"
//!
//! ; Assert total value
//! 2024-01-05 custom "valuation" Assets:Fund:Total 2345 USD
//! ```

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::types::{
    AmountData, CommodityData, CostData, DirectiveData, DirectiveWrapper, MetaValueData, OpenData,
    PluginError, PluginErrorSeverity, PluginInput, PluginOutput, PostingData, PriceAnnotationData,
    PriceData, TransactionData,
};

use super::super::NativePlugin;

const MAPPED_CURRENCY_PRECISION: u32 = 7;
const TAG_TO_ADD: &str = "valuation-applied";

/// Plugin for tracking opaque fund values.
pub struct ValuationPlugin;

/// Account mapping configuration.
#[derive(Clone)]
struct AccountMapping {
    currency: String,
    pnl_account: String,
}

impl NativePlugin for ValuationPlugin {
    fn name(&self) -> &'static str {
        "valuation"
    }

    fn description(&self) -> &'static str {
        "Track opaque fund values using synthetic commodities"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        // Note: The beancount-lazy-plugins valuation logic is currently handled by
        // the existing Python implementation. This native plugin intentionally
        // passes directives through unchanged to avoid generating incorrect
        // transactions until a fully tested native implementation is introduced.
        //
        // Any future native implementation should:
        // 1. Parse `custom "valuation" "config"` directives for account mapping
        // 2. Track synthetic currency balances
        // 3. Convert fund transactions to use synthetic currency with proper cost basis
        // 4. Generate PnL entries when valuations change
        // 5. Handle price annotations properly (including @@ syntax)
        PluginOutput {
            directives: input.directives,
            errors: Vec::new(),
        }
    }
}

#[allow(dead_code)]
impl ValuationPlugin {
    /// Full implementation (disabled for now due to bugs)
    fn process_full(&self, input: PluginInput) -> PluginOutput {
        let mut account_mapping: HashMap<String, AccountMapping> = HashMap::new();
        let mut commodities_present: HashSet<String> = HashSet::new();
        let mut new_commodities: Vec<DirectiveWrapper> = Vec::new();
        let mut new_prices: Vec<DirectiveWrapper> = Vec::new();
        let mut new_opens: Vec<DirectiveWrapper> = Vec::new();
        let mut errors: Vec<PluginError> = Vec::new();
        let mut new_entries: Vec<DirectiveWrapper> = Vec::new();
        let mut modified_transactions: Vec<DirectiveWrapper> = Vec::new();

        // Track balances in synthetic currency
        let mut balances: HashMap<String, Decimal> = HashMap::new();
        // Track last calculated price per currency
        let mut last_price: HashMap<String, Decimal> = HashMap::new();
        // Track which accounts have Open directives
        let mut open_accounts: HashSet<String> = HashSet::new();

        // First pass: read config entries and track existing opens/commodities
        for directive in &input.directives {
            match &directive.data {
                DirectiveData::Custom(custom) => {
                    if custom.custom_type == "valuation"
                        && !custom.values.is_empty()
                        && let Some(first_value) = custom.values.first()
                        && matches!(first_value, MetaValueData::String(s) if s == "config")
                    {
                        // Parse config entry
                        if let Some(mapping) = parse_config_entry(&custom.metadata)
                            && let Some(account) = get_meta_string(&custom.metadata, "account")
                        {
                            account_mapping.insert(account, mapping);
                        }
                    }
                }
                DirectiveData::Open(open) => {
                    open_accounts.insert(open.account.clone());
                }
                DirectiveData::Commodity(commodity) => {
                    commodities_present.insert(commodity.currency.clone());
                }
                _ => {}
            }
        }

        // Generate Open directives for accounts that don't have them
        for (account, mapping) in &account_mapping {
            if !open_accounts.contains(account) {
                new_opens.push(DirectiveWrapper {
                    directive_type: "open".to_string(),
                    date: "1970-01-01".to_string(),
                    filename: Some("<valuation>".to_string()),
                    lineno: Some(0),
                    data: DirectiveData::Open(OpenData {
                        account: account.clone(),
                        currencies: vec![mapping.currency.clone()],
                        booking: Some("FIFO".to_string()),
                        metadata: vec![],
                    }),
                });
            }
        }

        // Second pass: process entries
        for directive in input.directives {
            match &directive.data {
                DirectiveData::Transaction(txn) => {
                    let mut transaction_modified = false;
                    let mut new_postings: Vec<PostingData> = Vec::new();

                    for posting in &txn.postings {
                        if let Some(mapping) = account_mapping.get(&posting.account) {
                            transaction_modified = true;

                            let last_valuation_price = last_price
                                .get(&mapping.currency)
                                .copied()
                                .unwrap_or(Decimal::ONE);

                            // Create price entry
                            if let Some(ref units) = posting.units {
                                new_prices.push(DirectiveWrapper {
                                    directive_type: "price".to_string(),
                                    date: directive.date.clone(),
                                    filename: directive.filename.clone(),
                                    lineno: directive.lineno,
                                    data: DirectiveData::Price(PriceData {
                                        currency: mapping.currency.clone(),
                                        amount: AmountData {
                                            number: "1".to_string(),
                                            currency: units.currency.clone(),
                                        },
                                        metadata: vec![],
                                    }),
                                });

                                if let Ok(units_number) = units.number.parse::<Decimal>() {
                                    let total_in_mapped = units_number / last_valuation_price;

                                    let rounded_amount = if units_number > Decimal::ZERO {
                                        round_up(total_in_mapped, MAPPED_CURRENCY_PRECISION)
                                    } else {
                                        round_down(total_in_mapped, MAPPED_CURRENCY_PRECISION)
                                    };

                                    // Create modified posting with synthetic currency
                                    let modified_posting = if units_number > Decimal::ZERO {
                                        // Cash inflow - "buy" at last valuation price
                                        PostingData {
                                            account: posting.account.clone(),
                                            units: Some(AmountData {
                                                number: format_decimal(rounded_amount),
                                                currency: mapping.currency.clone(),
                                            }),
                                            cost: Some(CostData {
                                                number_per: Some(format_decimal(
                                                    last_valuation_price,
                                                )),
                                                number_total: None,
                                                currency: Some(units.currency.clone()),
                                                date: None,
                                                label: None,
                                                merge: false,
                                            }),
                                            price: None,
                                            flag: posting.flag.clone(),
                                            metadata: posting.metadata.clone(),
                                        }
                                    } else {
                                        // Cash outflow - "sell"
                                        PostingData {
                                            account: posting.account.clone(),
                                            units: Some(AmountData {
                                                number: format_decimal(rounded_amount),
                                                currency: mapping.currency.clone(),
                                            }),
                                            cost: Some(CostData {
                                                number_per: None, // MISSING - let booking fill it
                                                number_total: None,
                                                currency: Some(units.currency.clone()),
                                                date: None,
                                                label: None,
                                                merge: false,
                                            }),
                                            price: Some(PriceAnnotationData {
                                                is_total: false,
                                                amount: Some(AmountData {
                                                    number: format_decimal(last_valuation_price),
                                                    currency: units.currency.clone(),
                                                }),
                                                number: None,
                                                currency: None,
                                            }),
                                            flag: posting.flag.clone(),
                                            metadata: posting.metadata.clone(),
                                        }
                                    };

                                    // Add PnL balancing posting
                                    new_postings.push(PostingData {
                                        account: mapping.pnl_account.clone(),
                                        units: None,
                                        cost: None,
                                        price: posting.price.clone(),
                                        flag: posting.flag.clone(),
                                        metadata: vec![],
                                    });

                                    new_postings.push(modified_posting);

                                    // Update balance
                                    *balances.entry(posting.account.clone()).or_default() +=
                                        total_in_mapped;
                                }
                            }
                        } else {
                            new_postings.push(posting.clone());
                        }
                    }

                    if transaction_modified {
                        let mut new_tags = txn.tags.clone();
                        if !new_tags.contains(&TAG_TO_ADD.to_string()) {
                            new_tags.push(TAG_TO_ADD.to_string());
                        }

                        modified_transactions.push(DirectiveWrapper {
                            directive_type: "transaction".to_string(),
                            date: directive.date.clone(),
                            filename: directive.filename.clone(),
                            lineno: directive.lineno,
                            data: DirectiveData::Transaction(TransactionData {
                                flag: txn.flag.clone(),
                                payee: txn.payee.clone(),
                                narration: txn.narration.clone(),
                                tags: new_tags,
                                links: txn.links.clone(),
                                metadata: txn.metadata.clone(),
                                postings: new_postings,
                            }),
                        });
                    } else {
                        new_entries.push(directive);
                    }
                }
                DirectiveData::Balance(balance) => {
                    // Check if this is a balance for a mapped account
                    if let Some(mapping) = account_mapping.get(&balance.account) {
                        // Initialize balance and price
                        new_prices.push(DirectiveWrapper {
                            directive_type: "price".to_string(),
                            date: directive.date.clone(),
                            filename: directive.filename.clone(),
                            lineno: directive.lineno,
                            data: DirectiveData::Price(PriceData {
                                currency: mapping.currency.clone(),
                                amount: AmountData {
                                    number: "1".to_string(),
                                    currency: balance.amount.currency.clone(),
                                },
                                metadata: vec![],
                            }),
                        });

                        last_price.insert(mapping.currency.clone(), Decimal::ONE);

                        if let Ok(amount) = balance.amount.number.parse::<Decimal>() {
                            balances.insert(balance.account.clone(), amount);
                        }
                    }
                    new_entries.push(directive);
                }
                DirectiveData::Custom(custom) => {
                    if custom.custom_type == "valuation" && !custom.values.is_empty() {
                        if let Some(first_value) = custom.values.first() {
                            // Skip config entries
                            if matches!(first_value, MetaValueData::String(s) if s == "config") {
                                new_entries.push(directive);
                                continue;
                            }
                        }

                        // This is a valuation assertion: custom "valuation" Account Amount
                        if custom.values.len() >= 2 {
                            let account = match &custom.values[0] {
                                MetaValueData::Account(a) => Some(a.clone()),
                                MetaValueData::String(s) => Some(s.clone()),
                                _ => None,
                            };

                            if let Some(account) = account {
                                if let Some(mapping) = account_mapping.get(&account) {
                                    // Get valuation amount
                                    if let Some((val_number, val_currency)) =
                                        parse_valuation_amount(&custom.values[1])
                                    {
                                        let last_balance = balances
                                            .get(&account)
                                            .copied()
                                            .unwrap_or(Decimal::ZERO);

                                        if last_balance.abs() < Decimal::new(1, 9) {
                                            errors.push(PluginError {
                                                message: format!(
                                                    "Valuation called on empty account {account}"
                                                ),
                                                source_file: directive.filename.clone(),
                                                line_number: directive.lineno,
                                                severity: PluginErrorSeverity::Error,
                                            });
                                            new_entries.push(directive);
                                            continue;
                                        }

                                        let calculated_price = val_number / last_balance;
                                        last_price
                                            .insert(mapping.currency.clone(), calculated_price);

                                        new_prices.push(DirectiveWrapper {
                                            directive_type: "price".to_string(),
                                            date: directive.date.clone(),
                                            filename: directive.filename.clone(),
                                            lineno: directive.lineno,
                                            data: DirectiveData::Price(PriceData {
                                                currency: mapping.currency.clone(),
                                                amount: AmountData {
                                                    number: format_decimal(calculated_price),
                                                    currency: val_currency,
                                                },
                                                metadata: vec![],
                                            }),
                                        });
                                    }
                                } else {
                                    errors.push(PluginError {
                                        message: format!(
                                            "No valuation config for account {account}"
                                        ),
                                        source_file: directive.filename.clone(),
                                        line_number: directive.lineno,
                                        severity: PluginErrorSeverity::Error,
                                    });
                                }
                            }
                        }
                    }
                    new_entries.push(directive);
                }
                DirectiveData::Commodity(commodity) => {
                    commodities_present.insert(commodity.currency.clone());
                    new_entries.push(directive);
                }
                _ => {
                    new_entries.push(directive);
                }
            }
        }

        // Generate Commodity directives for synthetic currencies that don't exist
        for mapping in account_mapping.values() {
            if !commodities_present.contains(&mapping.currency) {
                new_commodities.push(DirectiveWrapper {
                    directive_type: "commodity".to_string(),
                    date: "1970-01-01".to_string(),
                    filename: Some("<valuation>".to_string()),
                    lineno: Some(0),
                    data: DirectiveData::Commodity(CommodityData {
                        currency: mapping.currency.clone(),
                        metadata: vec![],
                    }),
                });
            }
        }

        // Combine all directives
        let mut all_directives = new_opens;
        all_directives.extend(new_commodities);
        all_directives.extend(new_prices);
        all_directives.extend(new_entries);
        all_directives.extend(modified_transactions);

        PluginOutput {
            directives: all_directives,
            errors,
        }
    }
}

/// Parse config entry metadata.
fn parse_config_entry(metadata: &[(String, MetaValueData)]) -> Option<AccountMapping> {
    let currency = get_meta_string(metadata, "currency")?;
    let pnl_account = get_meta_string(metadata, "pnlAccount")?;
    Some(AccountMapping {
        currency,
        pnl_account,
    })
}

/// Get a string value from metadata.
fn get_meta_string(metadata: &[(String, MetaValueData)], key: &str) -> Option<String> {
    for (k, v) in metadata {
        if k == key {
            match v {
                MetaValueData::String(s) => return Some(s.clone()),
                MetaValueData::Account(a) => return Some(a.clone()),
                _ => {}
            }
        }
    }
    None
}

/// Parse a valuation amount from a `MetaValueData`.
fn parse_valuation_amount(value: &MetaValueData) -> Option<(Decimal, String)> {
    match value {
        MetaValueData::Amount(amount) => amount
            .number
            .parse::<Decimal>()
            .ok()
            .map(|n| (n, amount.currency.clone())),
        _ => None,
    }
}

/// Round up with given precision.
fn round_up(value: Decimal, decimals: u32) -> Decimal {
    let scale = Decimal::new(1, decimals);
    (value / scale).ceil() * scale
}

/// Round down with given precision.
fn round_down(value: Decimal, decimals: u32) -> Decimal {
    let scale = Decimal::new(1, decimals);
    (value / scale).floor() * scale
}

/// Format a decimal number.
fn format_decimal(d: Decimal) -> String {
    let s = d.to_string();
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_valuation_config_parsing() {
        let metadata = vec![
            (
                "account".to_string(),
                MetaValueData::String("Assets:Fund".to_string()),
            ),
            (
                "currency".to_string(),
                MetaValueData::String("FUND_USD".to_string()),
            ),
            (
                "pnlAccount".to_string(),
                MetaValueData::String("Income:Fund:PnL".to_string()),
            ),
        ];

        let mapping = parse_config_entry(&metadata);
        assert!(mapping.is_some());
        let mapping = mapping.unwrap();
        assert_eq!(mapping.currency, "FUND_USD");
        assert_eq!(mapping.pnl_account, "Income:Fund:PnL");
    }

    #[test]
    fn test_round_up() {
        let value = Decimal::new(12_345_678, 8); // 0.12345678
        let rounded = round_up(value, 7);
        assert!(rounded >= value);
    }

    #[test]
    fn test_round_down() {
        let value = Decimal::new(12_345_678, 8); // 0.12345678
        let rounded = round_down(value, 7);
        assert!(rounded <= value);
    }
}
