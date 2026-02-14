//! Clamp and filter functions for entries.

use std::collections::HashMap;

use rustledger_core::{Cost, Inventory, NaiveDate, Position};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::types::{Amount, Posting, PostingCost};

/// Result of a clamp operation.
#[derive(Serialize)]
pub struct ClampResult {
    pub entries: Vec<serde_json::Value>,
}

/// Result of a filter operation.
#[derive(Serialize)]
pub struct FilterResult {
    pub entries: Vec<serde_json::Value>,
}

/// Filter entries by date range (simple filtering, no summarization).
///
/// Rules:
/// - `commodity`: Always excluded
/// - `open`: Included if date < `end` (still active)
/// - `close`: Included if date >= `begin`
/// - Others: Included if `begin` <= date < `end`
pub fn filter_entries(
    entries: Vec<serde_json::Value>,
    begin: NaiveDate,
    end: NaiveDate,
) -> FilterResult {
    let filtered = entries
        .into_iter()
        .filter(|entry| {
            let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let date_str = entry.get("date").and_then(|d| d.as_str()).unwrap_or("");

            let Ok(entry_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
                // Drop entries without valid dates (consistent with clamp_entries)
                return false;
            };

            match entry_type {
                "commodity" => false,
                "open" => entry_date < end,
                "close" => entry_date >= begin,
                _ => entry_date >= begin && entry_date < end,
            }
        })
        .collect();

    FilterResult { entries: filtered }
}

/// Clamp entries by date range with opening balance summarization.
///
/// This function:
/// 1. Accumulates balances from transactions before `begin`
/// 2. Creates summarization transactions for opening balances
/// 3. Filters entries to the date range
/// 4. Includes relevant prices
pub fn clamp_entries(
    entries: Vec<serde_json::Value>,
    begin: NaiveDate,
    end: NaiveDate,
) -> ClampResult {
    let mut account_balances: HashMap<String, Inventory> = HashMap::new();
    let mut latest_prices: HashMap<(String, String), (NaiveDate, serde_json::Value)> =
        HashMap::new();
    let mut filtered_entries: Vec<serde_json::Value> = Vec::new();

    // First pass: accumulate balances and find relevant entries
    for entry in entries {
        let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let date_str = entry.get("date").and_then(|d| d.as_str()).unwrap_or("");

        let Ok(entry_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };

        if entry_date < begin {
            // Entries before begin date

            // Accumulate transaction balances for opening balance calculation
            if entry_type == "transaction"
                && let Some(postings) = entry.get("postings").and_then(|p| p.as_array())
            {
                for posting in postings {
                    accumulate_posting_balance(posting, &mut account_balances);
                }
            }

            // Track most recent price before begin_date (only keep latest per pair)
            if entry_type == "price"
                && let (Some(currency), Some(amount)) = (
                    entry.get("currency").and_then(|c| c.as_str()),
                    entry.get("amount"),
                )
                && let Some(amt_currency) = amount.get("currency").and_then(|c| c.as_str())
            {
                let key = (currency.to_string(), amt_currency.to_string());
                // Only update if this price is more recent (or same day - last wins)
                let should_update = latest_prices
                    .get(&key)
                    .is_none_or(|(d, _)| entry_date >= *d);
                if should_update {
                    latest_prices.insert(key, (entry_date, entry.clone()));
                }
            }

            // Include Open directives before begin (they're pre-existing accounts)
            if entry_type == "open" {
                filtered_entries.push(entry);
            }
        } else if entry_date < end {
            // Entry is within range - include all except commodity
            if entry_type != "commodity" {
                filtered_entries.push(entry);
            }
        }
        // Entries with date >= end are skipped
    }

    // Create summarization transactions for opening balances
    // One transaction per account with all positions as postings
    let mut summary_entries: Vec<serde_json::Value> = Vec::new();

    // Collect balance sheet accounts with non-empty inventories
    let mut balance_sheet_accounts: Vec<(&String, &Inventory)> = account_balances
        .iter()
        .filter(|(account, inv)| is_balance_sheet_account(account) && !inv.is_empty())
        .collect();

    // Sort by account name for deterministic output
    balance_sheet_accounts.sort_by_key(|(account, _)| *account);

    for (account, inventory) in balance_sheet_accounts {
        let entry = create_summary_transaction(account, inventory, begin);
        summary_entries.push(entry);
    }

    // Create Equity:Earnings:Previous transaction to close out Income/Expenses
    // This transfers the P&L balance to retained earnings
    let mut pnl_totals: HashMap<String, rustledger_core::Decimal> = HashMap::new();
    for (account, inv) in &account_balances {
        if is_income_statement_account(account) {
            for position in inv.positions() {
                let currency = position.units.currency.to_string();
                *pnl_totals.entry(currency).or_default() += position.units.number;
            }
        }
    }

    // Only create the transaction if there are P&L balances
    if !pnl_totals.is_empty() {
        let entry = create_earnings_transaction(&pnl_totals, begin);
        summary_entries.push(entry);
    }

    // Include latest prices before begin_date
    let mut price_entries: Vec<serde_json::Value> = latest_prices
        .into_values()
        .map(|(_, entry)| entry)
        .collect();

    // Combine all entries and sort by date
    let mut all_entries = Vec::new();
    all_entries.append(&mut price_entries);
    all_entries.append(&mut summary_entries);
    all_entries.append(&mut filtered_entries);

    // Sort by date with deterministic tiebreakers (type priority, then hash)
    all_entries.sort_by(|a, b| {
        let date_a = a.get("date").and_then(|d| d.as_str()).unwrap_or("");
        let date_b = b.get("date").and_then(|d| d.as_str()).unwrap_or("");

        date_a
            .cmp(date_b)
            .then_with(|| {
                // Type priority: open < balance < transaction < others
                fn type_priority(entry: &serde_json::Value) -> u8 {
                    match entry.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                        "open" => 0,
                        "balance" => 1,
                        "transaction" => 2,
                        "close" => 10,
                        _ => 5,
                    }
                }
                type_priority(a).cmp(&type_priority(b))
            })
            .then_with(|| {
                // Finally, compare by hash for full determinism
                let hash_a = a
                    .get("meta")
                    .and_then(|m| m.get("hash"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("");
                let hash_b = b
                    .get("meta")
                    .and_then(|m| m.get("hash"))
                    .and_then(|h| h.as_str())
                    .unwrap_or("");
                hash_a.cmp(hash_b)
            })
    });

    ClampResult {
        entries: all_entries,
    }
}

/// Check if an account is a balance sheet account.
fn is_balance_sheet_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Assets" | "Liabilities" | "Equity")
}

/// Check if an account is an income statement account (Income or Expenses).
fn is_income_statement_account(account: &str) -> bool {
    let account_type = account.split(':').next().unwrap_or("");
    matches!(account_type, "Income" | "Expenses")
}

/// Create a transaction to close out Income/Expenses to Equity:Earnings:Previous.
fn create_earnings_transaction(
    pnl_totals: &HashMap<String, rustledger_core::Decimal>,
    date: NaiveDate,
) -> serde_json::Value {
    let date_str = date.format("%Y-%m-%d").to_string();

    // Create postings for each currency
    let mut postings: Vec<Posting> = Vec::new();

    // Sort currencies for deterministic output
    let mut currencies: Vec<_> = pnl_totals.keys().collect();
    currencies.sort();

    for currency in currencies {
        let number = pnl_totals[currency];
        if number.is_zero() {
            continue;
        }

        // Post balance to Equity:Earnings:Previous
        postings.push(Posting {
            account: "Equity:Earnings:Previous".to_string(),
            units: Some(Amount {
                number: number.to_string(),
                currency: currency.clone(),
            }),
            cost: None,
            price: None,
            flag: None,
            meta: HashMap::new(),
        });

        // Post opposite to Equity:Opening-Balances
        postings.push(Posting {
            account: "Equity:Opening-Balances".to_string(),
            units: Some(Amount {
                number: (-number).to_string(),
                currency: currency.clone(),
            }),
            cost: None,
            price: None,
            flag: None,
            meta: HashMap::new(),
        });
    }

    // Create unique hash for the earnings transaction
    let hash_input = format!("earnings:{date_str}");
    let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

    serde_json::json!({
        "type": "transaction",
        "date": date_str,
        "flag": "S",
        "payee": null,
        "narration": "Opening balance",
        "tags": [],
        "links": [],
        "postings": postings,
        "meta": {
            "filename": "<summarization>",
            "lineno": 0,
            "hash": hash
        }
    })
}

/// Accumulate a posting's balance into the account balances map.
fn accumulate_posting_balance(
    posting: &serde_json::Value,
    account_balances: &mut HashMap<String, Inventory>,
) {
    let account = posting
        .get("account")
        .and_then(|a| a.as_str())
        .unwrap_or("");
    if account.is_empty() {
        return;
    }

    let Some(units) = posting.get("units") else {
        return;
    };

    let number_str = units.get("number").and_then(|n| n.as_str()).unwrap_or("0");
    let currency = units.get("currency").and_then(|c| c.as_str()).unwrap_or("");

    let Ok(number) = rustledger_core::Decimal::from_str_exact(number_str) else {
        return;
    };

    let amount = rustledger_core::Amount::new(number, currency);
    let inv = account_balances.entry(account.to_string()).or_default();

    let position = if let Some(cost) = posting.get("cost") {
        parse_cost_and_create_position(amount, cost)
    } else {
        Position::simple(amount)
    };

    inv.add(position);
}

/// Parse cost from JSON and create a Position.
/// Falls back to `Position::simple` if cost data is missing or invalid.
fn parse_cost_and_create_position(
    amount: rustledger_core::Amount,
    cost: &serde_json::Value,
) -> Position {
    // Get cost number - if missing or invalid, fall back to simple position
    let Some(cost_number_str) = cost.get("number").and_then(|n| n.as_str()) else {
        return Position::simple(amount);
    };
    let Ok(cost_number) = rustledger_core::Decimal::from_str_exact(cost_number_str) else {
        return Position::simple(amount);
    };

    // Get cost currency - if missing or empty, fall back to simple position
    let Some(cost_currency) = cost.get("currency").and_then(|c| c.as_str()) else {
        return Position::simple(amount);
    };
    if cost_currency.is_empty() {
        return Position::simple(amount);
    }

    let cost_date_str = cost.get("date").and_then(|d| d.as_str());
    let cost_label = cost.get("label").and_then(|l| l.as_str()).map(String::from);
    let cost_date = cost_date_str.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let cost = Cost {
        number: cost_number,
        currency: cost_currency.into(),
        date: cost_date,
        label: cost_label,
    };
    Position::with_cost(amount, cost)
}

/// Create a summary transaction for an account's opening balance.
/// Takes all positions in the inventory and creates one transaction.
fn create_summary_transaction(
    account: &str,
    inventory: &Inventory,
    date: NaiveDate,
) -> serde_json::Value {
    let date_str = date.format("%Y-%m-%d").to_string();

    // Create postings for each position in the inventory
    let mut postings: Vec<Posting> = Vec::new();

    for position in inventory.positions() {
        let units = Amount {
            number: position.units.number.to_string(),
            currency: position.units.currency.to_string(),
        };

        let cost = position.cost.as_ref().map(|c| PostingCost {
            number: Some(c.number.to_string()),
            number_total: None,
            currency: Some(c.currency.to_string()),
            date: c.date.map(|d| d.format("%Y-%m-%d").to_string()),
            label: c.label.clone(),
        });

        postings.push(Posting {
            account: account.to_string(),
            units: Some(units),
            cost,
            price: None,
            flag: None,
            meta: HashMap::new(),
        });
    }

    // Create equity posting (one per position to balance)
    for position in inventory.positions() {
        let neg_number = -position.units.number;
        postings.push(Posting {
            account: "Equity:Opening-Balances".to_string(),
            units: Some(Amount {
                number: neg_number.to_string(),
                currency: position.units.currency.to_string(),
            }),
            cost: None,
            price: None,
            flag: None,
            meta: HashMap::new(),
        });
    }

    // Create unique hash for the summary transaction
    let hash_input = format!("summary:{date_str}:{account}");
    let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

    serde_json::json!({
        "type": "transaction",
        "date": date_str,
        "flag": "S",
        "payee": null,
        "narration": "Opening balance",
        "tags": [],
        "links": [],
        "postings": postings,
        "meta": {
            "filename": "<summarization>",
            "lineno": 0,
            "hash": hash
        }
    })
}
