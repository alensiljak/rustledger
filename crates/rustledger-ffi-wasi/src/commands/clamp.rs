//! Clamp and filter commands (clamp, filter-entries, clamp-entries).

use std::collections::HashMap;

use rustledger_core::{Cost, Directive, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::convert::directive_to_json;
use crate::helpers::load_source;
use crate::types::{Amount, DirectiveJson, Error, Meta, Posting, PostingCost};
use crate::{API_VERSION, output_json, parse_json_error};

/// Output for clamp command.
#[derive(Serialize)]
pub struct ClampOutput {
    pub api_version: &'static str,
    pub entries: Vec<DirectiveJson>,
    /// Opening balances synthesized for the begin date.
    pub opening_balances: Vec<OpeningBalance>,
    pub errors: Vec<Error>,
}

#[derive(Serialize)]
pub struct OpeningBalance {
    pub account: String,
    pub date: String,
    pub balance: InventoryJson,
}

#[derive(Serialize)]
pub struct InventoryJson {
    pub positions: Vec<PositionJson>,
}

#[derive(Serialize, Clone)]
pub struct PositionJson {
    pub units: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostJson>,
}

#[derive(Serialize, Clone)]
pub struct CostJson {
    pub number: String,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Input for filter-entries command.
#[derive(Deserialize)]
pub struct FilterEntriesInput {
    /// Array of entry objects (same format as load output).
    pub entries: Vec<serde_json::Value>,
    /// Begin date (inclusive) in ISO format (YYYY-MM-DD).
    pub begin_date: String,
    /// End date (exclusive) in ISO format (YYYY-MM-DD).
    pub end_date: String,
}

/// Output for filter-entries command.
#[derive(Serialize)]
pub struct FilterEntriesOutput {
    pub api_version: &'static str,
    pub entries: Vec<serde_json::Value>,
    pub errors: Vec<Error>,
}

/// Input for clamp-entries command.
#[derive(Deserialize)]
pub struct ClampEntriesInput {
    /// Array of entry objects (same format as load output).
    pub entries: Vec<serde_json::Value>,
    /// Begin date (inclusive) in ISO format (YYYY-MM-DD).
    pub begin_date: String,
    /// End date (exclusive) in ISO format (YYYY-MM-DD).
    pub end_date: String,
}

/// Output for clamp-entries command.
#[derive(Serialize)]
pub struct ClampEntriesOutput {
    pub api_version: &'static str,
    pub entries: Vec<serde_json::Value>,
    pub errors: Vec<Error>,
}

/// Check if an account is a balance sheet account (Assets, Liabilities, Equity).
pub fn is_balance_sheet_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Assets" | "Liabilities" | "Equity")
}

/// Check if an account is an income statement account (Income, Expenses).
pub fn is_income_statement_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Income" | "Expenses")
}

/// Clamp beancount source by date range.
pub fn cmd_clamp(
    source: &str,
    filename: &str,
    begin_date: Option<&str>,
    end_date: Option<&str>,
) -> i32 {
    let load = load_source(source);

    // Parse date arguments
    let begin: Option<NaiveDate> =
        begin_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let end: Option<NaiveDate> =
        end_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    // Track account balances for opening balances
    let mut account_balances: HashMap<String, rustledger_core::Inventory> = HashMap::new();
    let mut opening_balances: Vec<OpeningBalance> = Vec::new();

    // Track most recent price per commodity before begin_date
    let mut latest_prices: HashMap<(String, String), (NaiveDate, Directive, u32)> = HashMap::new();

    // Filter directives
    let mut filtered_directives: Vec<(Directive, u32)> = Vec::new();

    for (directive, &line) in load.directives.iter().zip(load.directive_lines.iter()) {
        let directive_date = directive.date();

        // Check if directive is before begin date
        if let Some(begin) = begin
            && directive_date < begin
        {
            // Accumulate transaction postings for opening balances
            if let Directive::Transaction(txn) = directive {
                for posting in &txn.postings {
                    if let Some(rustledger_core::IncompleteAmount::Complete(amount)) =
                        &posting.units
                    {
                        let inv = account_balances
                            .entry(posting.account.to_string())
                            .or_default();
                        let position = if let Some(cost_spec) = &posting.cost {
                            let cost = Cost {
                                number: cost_spec.number_per.unwrap_or(amount.number),
                                currency: cost_spec
                                    .currency
                                    .clone()
                                    .unwrap_or_else(|| amount.currency.clone()),
                                date: cost_spec.date.or(Some(txn.date)),
                                label: cost_spec.label.clone(),
                            };
                            rustledger_core::Position::with_cost(amount.clone(), cost)
                        } else {
                            rustledger_core::Position::simple(amount.clone())
                        };
                        inv.add(position);
                    }
                }
            }

            // Track most recent price per commodity before begin_date
            if let Directive::Price(price) = directive {
                let key = (
                    price.currency.to_string(),
                    price.amount.currency.to_string(),
                );
                let should_update = latest_prices
                    .get(&key)
                    .is_none_or(|(existing_date, _, _)| directive_date >= *existing_date);
                if should_update {
                    latest_prices.insert(key, (directive_date, directive.clone(), line));
                }
            }

            // Keep Open directives before begin date
            if let Directive::Open(_) = directive {
                filtered_directives.push((directive.clone(), line));
            }
            continue;
        }

        // Check if directive is after end date
        if let Some(end) = end
            && directive_date >= end
        {
            continue;
        }

        // Exclude Commodity entries from output
        if let Directive::Commodity(_) = directive {
            continue;
        }

        filtered_directives.push((directive.clone(), line));
    }

    // Add most recent prices before begin_date
    let mut price_entries: Vec<(Directive, u32)> = latest_prices
        .into_values()
        .map(|(_, directive, line)| (directive, line))
        .collect();
    price_entries.sort_by(|(a, _), (b, _)| a.date().cmp(&b.date()));

    // Generate opening balance summarization transactions
    let mut summarization_entries: Vec<DirectiveJson> = Vec::new();
    if let Some(begin) = begin {
        let summarize_date = begin.pred_opt().unwrap_or(begin);
        let summarize_date_str = summarize_date.to_string();

        let mut balance_sheet_accounts: Vec<(&String, &rustledger_core::Inventory)> = Vec::new();
        let mut retained_earnings: rustledger_core::Inventory = rustledger_core::Inventory::new();

        for (account, inventory) in &account_balances {
            if inventory.is_empty() {
                continue;
            }

            if is_balance_sheet_account(account) {
                balance_sheet_accounts.push((account, inventory));
            } else if is_income_statement_account(account) {
                for position in inventory.positions() {
                    retained_earnings.add(position.clone());
                }
            }
        }

        balance_sheet_accounts.sort_by_key(|(account, _)| *account);

        // Create summarization transactions for balance sheet accounts
        for (index, (account, inventory)) in balance_sheet_accounts.iter().enumerate() {
            let positions: Vec<PositionJson> = inventory
                .positions()
                .iter()
                .map(|p| PositionJson {
                    units: Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    },
                    cost: p.cost.as_ref().map(|c| CostJson {
                        number: c.number.to_string(),
                        currency: c.currency.to_string(),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                })
                .collect();

            opening_balances.push(OpeningBalance {
                account: (*account).clone(),
                date: begin.to_string(),
                balance: InventoryJson {
                    positions: positions.clone(),
                },
            });

            let postings: Vec<Posting> = inventory
                .positions()
                .iter()
                .map(|p| Posting {
                    account: (*account).clone(),
                    units: Some(Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    }),
                    cost: p.cost.as_ref().map(|c| PostingCost {
                        number: Some(c.number.to_string()),
                        number_total: None,
                        currency: Some(c.currency.to_string()),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                    price: None,
                    flag: None,
                    meta: HashMap::new(),
                })
                .collect();

            let hash_input = format!(
                "S|{summarize_date_str}|Opening balance for '{account}' (Summarization)|{index}"
            );
            let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

            summarization_entries.push(DirectiveJson::Transaction {
                date: summarize_date_str.clone(),
                flag: "S".to_string(),
                payee: None,
                narration: Some(format!("Opening balance for '{account}' (Summarization)")),
                tags: vec![],
                links: vec![],
                postings,
                meta: Meta {
                    filename: "<summarize>".to_string(),
                    lineno: index as u32,
                    hash,
                    user: HashMap::new(),
                },
            });
        }

        // Create aggregated Equity:Earnings:Previous transaction
        if !retained_earnings.is_empty() {
            let earnings_account = "Equity:Earnings:Previous";
            let index = balance_sheet_accounts.len();

            let positions: Vec<PositionJson> = retained_earnings
                .positions()
                .iter()
                .map(|p| PositionJson {
                    units: Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    },
                    cost: p.cost.as_ref().map(|c| CostJson {
                        number: c.number.to_string(),
                        currency: c.currency.to_string(),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                })
                .collect();

            opening_balances.push(OpeningBalance {
                account: earnings_account.to_string(),
                date: begin.to_string(),
                balance: InventoryJson { positions },
            });

            let postings: Vec<Posting> = retained_earnings
                .positions()
                .iter()
                .map(|p| Posting {
                    account: earnings_account.to_string(),
                    units: Some(Amount {
                        number: p.units.number.to_string(),
                        currency: p.units.currency.to_string(),
                    }),
                    cost: p.cost.as_ref().map(|c| PostingCost {
                        number: Some(c.number.to_string()),
                        number_total: None,
                        currency: Some(c.currency.to_string()),
                        date: c.date.map(|d| d.to_string()),
                        label: c.label.clone(),
                    }),
                    price: None,
                    flag: None,
                    meta: HashMap::new(),
                })
                .collect();

            let hash_input = format!(
                "S|{summarize_date_str}|Opening balance for '{earnings_account}' (Summarization)|{index}"
            );
            let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

            summarization_entries.push(DirectiveJson::Transaction {
                date: summarize_date_str,
                flag: "S".to_string(),
                payee: None,
                narration: Some(format!(
                    "Opening balance for '{earnings_account}' (Summarization)"
                )),
                tags: vec![],
                links: vec![],
                postings,
                meta: Meta {
                    filename: "<summarize>".to_string(),
                    lineno: index as u32,
                    hash,
                    user: HashMap::new(),
                },
            });
        }
    }

    // Convert filtered directives to JSON
    let mut entries: Vec<DirectiveJson> = filtered_directives
        .iter()
        .map(|(d, line)| directive_to_json(d, *line, filename))
        .collect();

    let price_json: Vec<DirectiveJson> = price_entries
        .iter()
        .map(|(d, line)| directive_to_json(d, *line, filename))
        .collect();

    entries.splice(0..0, summarization_entries);
    entries.splice(0..0, price_json);

    let output = ClampOutput {
        api_version: API_VERSION,
        entries,
        opening_balances,
        errors: load.errors,
    };
    output_json(&output)
}

/// Filter already-parsed entries by date range.
pub fn cmd_filter_entries(json_str: &str) -> i32 {
    let input: FilterEntriesInput = match serde_json::from_str(json_str) {
        Ok(i) => i,
        Err(e) => {
            let output = FilterEntriesOutput {
                api_version: API_VERSION,
                entries: vec![],
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    let Ok(begin) = NaiveDate::parse_from_str(&input.begin_date, "%Y-%m-%d") else {
        let output = FilterEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid begin_date format: {}. Expected YYYY-MM-DD",
                    input.begin_date
                ))
                .with_field("begin_date"),
            ],
        };
        return output_json(&output);
    };

    let Ok(end) = NaiveDate::parse_from_str(&input.end_date, "%Y-%m-%d") else {
        let output = FilterEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid end_date format: {}. Expected YYYY-MM-DD",
                    input.end_date
                ))
                .with_field("end_date"),
            ],
        };
        return output_json(&output);
    };

    let mut filtered: Vec<serde_json::Value> = Vec::new();

    for entry in input.entries {
        let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let date_str = entry.get("date").and_then(|d| d.as_str()).unwrap_or("");

        let entry_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        let include = match entry_type {
            "commodity" => false,
            "open" => entry_date < end,
            "close" => entry_date >= begin,
            _ => entry_date >= begin && entry_date < end,
        };

        if include {
            filtered.push(entry);
        }
    }

    let output = FilterEntriesOutput {
        api_version: API_VERSION,
        entries: filtered,
        errors: vec![],
    };
    output_json(&output)
}

/// Clamp already-parsed entries by date range with summarization.
pub fn cmd_clamp_entries(json_str: &str) -> i32 {
    let input: ClampEntriesInput = match serde_json::from_str(json_str) {
        Ok(i) => i,
        Err(e) => {
            let output = ClampEntriesOutput {
                api_version: API_VERSION,
                entries: vec![],
                errors: vec![parse_json_error(&e)],
            };
            return output_json(&output);
        }
    };

    let Ok(begin) = NaiveDate::parse_from_str(&input.begin_date, "%Y-%m-%d") else {
        let output = ClampEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid begin_date format: {}. Expected YYYY-MM-DD",
                    input.begin_date
                ))
                .with_field("begin_date"),
            ],
        };
        return output_json(&output);
    };

    let Ok(end) = NaiveDate::parse_from_str(&input.end_date, "%Y-%m-%d") else {
        let output = ClampEntriesOutput {
            api_version: API_VERSION,
            entries: vec![],
            errors: vec![
                Error::new(format!(
                    "Invalid end_date format: {}. Expected YYYY-MM-DD",
                    input.end_date
                ))
                .with_field("end_date"),
            ],
        };
        return output_json(&output);
    };

    let mut account_balances: HashMap<String, rustledger_core::Inventory> = HashMap::new();
    let mut latest_prices: HashMap<(String, String), (NaiveDate, serde_json::Value)> =
        HashMap::new();
    let mut filtered_entries: Vec<serde_json::Value> = Vec::new();

    for entry in input.entries {
        let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let date_str = entry.get("date").and_then(|d| d.as_str()).unwrap_or("");

        let entry_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        if entry_date < begin {
            if entry_type == "transaction"
                && let Some(postings) = entry.get("postings").and_then(|p| p.as_array())
            {
                for posting in postings {
                    let account = posting
                        .get("account")
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    if account.is_empty() {
                        continue;
                    }

                    if let Some(units) = posting.get("units") {
                        let number_str =
                            units.get("number").and_then(|n| n.as_str()).unwrap_or("0");
                        let currency = units.get("currency").and_then(|c| c.as_str()).unwrap_or("");

                        if let Ok(number) = rustledger_core::Decimal::from_str_exact(number_str) {
                            let amount = rustledger_core::Amount::new(number, currency);
                            let inv = account_balances.entry(account.to_string()).or_default();

                            let position = if let Some(cost) = posting.get("cost") {
                                let cost_number_str =
                                    cost.get("number").and_then(|n| n.as_str()).unwrap_or("0");
                                let cost_currency =
                                    cost.get("currency").and_then(|c| c.as_str()).unwrap_or("");
                                let cost_date_str = cost.get("date").and_then(|d| d.as_str());
                                let cost_label =
                                    cost.get("label").and_then(|l| l.as_str()).map(String::from);

                                if let Ok(cost_number) =
                                    rustledger_core::Decimal::from_str_exact(cost_number_str)
                                {
                                    let cost_date = cost_date_str.and_then(|s| {
                                        NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
                                    });
                                    let cost = Cost {
                                        number: cost_number,
                                        currency: cost_currency.into(),
                                        date: cost_date,
                                        label: cost_label,
                                    };
                                    rustledger_core::Position::with_cost(amount, cost)
                                } else {
                                    rustledger_core::Position::simple(amount)
                                }
                            } else {
                                rustledger_core::Position::simple(amount)
                            };

                            inv.add(position);
                        }
                    }
                }
            }

            if entry_type == "price" {
                let currency = entry.get("currency").and_then(|c| c.as_str()).unwrap_or("");
                let quote_currency = entry
                    .get("amount")
                    .and_then(|a| a.get("currency"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("");

                if !currency.is_empty() && !quote_currency.is_empty() {
                    let key = (currency.to_string(), quote_currency.to_string());
                    let should_update = latest_prices
                        .get(&key)
                        .is_none_or(|(existing_date, _)| entry_date >= *existing_date);
                    if should_update {
                        latest_prices.insert(key, (entry_date, entry.clone()));
                    }
                }
            }

            if entry_type == "open" {
                filtered_entries.push(entry);
            }
            continue;
        }

        if entry_date >= end {
            continue;
        }

        if entry_type == "commodity" {
            continue;
        }

        filtered_entries.push(entry);
    }

    let mut price_entries: Vec<serde_json::Value> = latest_prices
        .into_values()
        .map(|(_, entry)| entry)
        .collect();
    price_entries.sort_by(|a, b| {
        let date_a = a.get("date").and_then(|d| d.as_str()).unwrap_or("");
        let date_b = b.get("date").and_then(|d| d.as_str()).unwrap_or("");
        date_a.cmp(date_b)
    });

    let mut summarization_entries: Vec<serde_json::Value> = Vec::new();
    let summarize_date = begin.pred_opt().unwrap_or(begin);
    let summarize_date_str = summarize_date.to_string();

    let mut balance_sheet_accounts: Vec<(&String, &rustledger_core::Inventory)> = Vec::new();
    let mut retained_earnings: rustledger_core::Inventory = rustledger_core::Inventory::new();

    for (account, inventory) in &account_balances {
        if inventory.is_empty() {
            continue;
        }

        if is_balance_sheet_account(account) {
            balance_sheet_accounts.push((account, inventory));
        } else if is_income_statement_account(account) {
            for position in inventory.positions() {
                retained_earnings.add(position.clone());
            }
        }
    }

    balance_sheet_accounts.sort_by_key(|(account, _)| *account);

    for (index, (account, inventory)) in balance_sheet_accounts.iter().enumerate() {
        let postings: Vec<serde_json::Value> = inventory
            .positions()
            .iter()
            .map(|p| {
                let mut posting = serde_json::json!({
                    "account": account,
                    "units": {
                        "number": p.units.number.to_string(),
                        "currency": p.units.currency.to_string()
                    }
                });
                if let Some(cost) = &p.cost {
                    posting["cost"] = serde_json::json!({
                        "number": cost.number.to_string(),
                        "currency": cost.currency.to_string()
                    });
                    if let Some(date) = cost.date {
                        posting["cost"]["date"] = serde_json::json!(date.to_string());
                    }
                    if let Some(label) = &cost.label {
                        posting["cost"]["label"] = serde_json::json!(label);
                    }
                }
                posting
            })
            .collect();

        let hash_input = format!(
            "S|{summarize_date_str}|Opening balance for '{account}' (Summarization)|{index}"
        );
        let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        summarization_entries.push(serde_json::json!({
            "type": "transaction",
            "date": summarize_date_str,
            "flag": "S",
            "narration": format!("Opening balance for '{}' (Summarization)", account),
            "tags": [],
            "links": [],
            "postings": postings,
            "meta": {
                "filename": "<summarize>",
                "lineno": index,
                "hash": hash
            }
        }));
    }

    if !retained_earnings.is_empty() {
        let earnings_account = "Equity:Earnings:Previous";
        let index = balance_sheet_accounts.len();

        let postings: Vec<serde_json::Value> = retained_earnings
            .positions()
            .iter()
            .map(|p| {
                let mut posting = serde_json::json!({
                    "account": earnings_account,
                    "units": {
                        "number": p.units.number.to_string(),
                        "currency": p.units.currency.to_string()
                    }
                });
                if let Some(cost) = &p.cost {
                    posting["cost"] = serde_json::json!({
                        "number": cost.number.to_string(),
                        "currency": cost.currency.to_string()
                    });
                    if let Some(date) = cost.date {
                        posting["cost"]["date"] = serde_json::json!(date.to_string());
                    }
                    if let Some(label) = &cost.label {
                        posting["cost"]["label"] = serde_json::json!(label);
                    }
                }
                posting
            })
            .collect();

        let hash_input = format!(
            "S|{summarize_date_str}|Opening balance for '{earnings_account}' (Summarization)|{index}"
        );
        let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        summarization_entries.push(serde_json::json!({
            "type": "transaction",
            "date": summarize_date_str,
            "flag": "S",
            "narration": format!("Opening balance for '{}' (Summarization)", earnings_account),
            "tags": [],
            "links": [],
            "postings": postings,
            "meta": {
                "filename": "<summarize>",
                "lineno": index,
                "hash": hash
            }
        }));
    }

    let mut result_entries: Vec<serde_json::Value> = Vec::new();
    result_entries.extend(price_entries);
    result_entries.extend(summarization_entries);
    result_entries.extend(filtered_entries);

    let output = ClampEntriesOutput {
        api_version: API_VERSION,
        entries: result_entries,
        errors: vec![],
    };
    output_json(&output)
}
