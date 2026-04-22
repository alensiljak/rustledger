//! Transfer matching across accounts.
//!
//! Detects transfer pairs — transactions that represent the same real-world
//! money movement appearing in two different account imports (e.g., a $500
//! debit in checking and a $500 credit in savings on the same day).
//!
//! The matcher finds pairs based on:
//! - Opposite-sign amounts (within tolerance)
//! - Same currency
//! - Dates within a configurable window
//! - Optional narration keyword boosting (TRANSFER, XFER, ACH, etc.)

use rust_decimal::Decimal;
use rustledger_plugin_types::{DirectiveData, DirectiveWrapper};
use std::str::FromStr;

/// Configuration for transfer matching.
#[derive(Debug, Clone)]
pub struct TransferConfig {
    /// Maximum number of days between matched transactions (default: 3).
    pub date_window_days: i64,
    /// Amount tolerance for matching (default: 0.01).
    pub amount_tolerance: Decimal,
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            date_window_days: 3,
            amount_tolerance: Decimal::new(1, 2), // 0.01
        }
    }
}

/// A detected transfer pair.
#[derive(Debug, Clone)]
pub struct TransferMatch {
    /// Index of the source transaction (debit side) in the first group.
    pub from_group: usize,
    /// Index within that group's directives.
    pub from_index: usize,
    /// Index of the destination transaction (credit side) in the second group.
    pub to_group: usize,
    /// Index within that group's directives.
    pub to_index: usize,
    /// The matched amount (absolute value).
    pub amount: Decimal,
    /// The matched currency.
    pub currency: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
}

/// Find transfer pairs across multiple account import groups.
///
/// Each group is a `(account_name, directives)` pair from a separate import.
/// Returns matches between groups (never within a single group).
#[must_use]
pub fn find_transfers(
    groups: &[(String, Vec<DirectiveWrapper>)],
    config: &TransferConfig,
) -> Vec<TransferMatch> {
    let mut matches = Vec::new();

    // Compare each pair of groups
    for (g1, (_, directives1)) in groups.iter().enumerate() {
        for (g2, (_, directives2)) in groups.iter().enumerate() {
            if g2 <= g1 {
                continue; // Avoid duplicate comparisons
            }

            find_matches_between(g1, directives1, g2, directives2, config, &mut matches);
        }
    }

    matches
}

/// Find matching transactions between two directive lists.
fn find_matches_between(
    g1: usize,
    directives1: &[DirectiveWrapper],
    g2: usize,
    directives2: &[DirectiveWrapper],
    config: &TransferConfig,
    matches: &mut Vec<TransferMatch>,
) {
    // Track which directives have already been matched
    let mut matched_in_g2: Vec<bool> = vec![false; directives2.len()];

    for (i, d1) in directives1.iter().enumerate() {
        let Some((amount1, currency1)) = first_posting_amount_currency(d1) else {
            continue;
        };

        for (j, d2) in directives2.iter().enumerate() {
            if matched_in_g2[j] {
                continue;
            }

            let Some((amount2, currency2)) = first_posting_amount_currency(d2) else {
                continue;
            };

            // Must be same currency
            if currency1 != currency2 {
                continue;
            }

            // Must be opposite signs and similar absolute amounts
            let sum = (amount1 + amount2).abs();
            if sum > config.amount_tolerance {
                continue;
            }

            // Must be within date window
            if !within_date_window(&d1.date, &d2.date, config.date_window_days) {
                continue;
            }

            // Compute confidence
            let mut confidence: f64 = 0.7; // Base confidence for amount + date match

            // Boost for transfer-related keywords in narration
            if has_transfer_keywords(d1) || has_transfer_keywords(d2) {
                confidence += 0.2;
            }

            // Boost for exact date match
            if d1.date == d2.date {
                confidence += 0.1;
            }

            let confidence = confidence.min(1.0);

            // Determine from/to based on sign
            let (from_group, from_index, to_group, to_index) = if amount1.is_sign_negative() {
                (g1, i, g2, j)
            } else {
                (g2, j, g1, i)
            };

            matches.push(TransferMatch {
                from_group,
                from_index,
                to_group,
                to_index,
                amount: amount1.abs(),
                currency: currency1.to_string(),
                confidence,
            });

            matched_in_g2[j] = true;
            break; // One match per source transaction
        }
    }
}

/// Extract the first posting's amount and currency from a directive.
fn first_posting_amount_currency(d: &DirectiveWrapper) -> Option<(Decimal, &str)> {
    if let DirectiveData::Transaction(txn) = &d.data
        && let Some(posting) = txn.postings.first()
        && let Some(units) = &posting.units
    {
        let amount = Decimal::from_str(&units.number).ok()?;
        return Some((amount, &units.currency));
    }
    None
}

/// Check if two date strings are within a given window (in days).
fn within_date_window(date1: &str, date2: &str, days: i64) -> bool {
    use rustledger_plugin_types as _; // Dates are YYYY-MM-DD strings
    // Simple date comparison for YYYY-MM-DD format
    let d1: jiff::civil::Date = match date1.parse() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let d2: jiff::civil::Date = match date2.parse() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let diff = d2.since(d1).unwrap_or_default().get_days().abs();
    i64::from(diff) <= days
}

/// Transfer-related keywords that boost matching confidence.
const TRANSFER_KEYWORDS: &[&str] = &[
    "transfer", "xfer", "ach", "wire", "payment", "internal", "move", "sweep",
];

/// Check if a directive's narration contains transfer-related keywords.
fn has_transfer_keywords(d: &DirectiveWrapper) -> bool {
    if let DirectiveData::Transaction(txn) = &d.data {
        let narration_lower = txn.narration.to_lowercase();
        if TRANSFER_KEYWORDS
            .iter()
            .any(|kw| narration_lower.contains(kw))
        {
            return true;
        }
        if let Some(ref payee) = txn.payee {
            let payee_lower = payee.to_lowercase();
            if TRANSFER_KEYWORDS.iter().any(|kw| payee_lower.contains(kw)) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_plugin_types::{AmountData, PostingData, TransactionData};

    fn make_txn(date: &str, narration: &str, amount: &str, currency: &str) -> DirectiveWrapper {
        DirectiveWrapper {
            directive_type: "transaction".to_string(),
            date: date.to_string(),
            filename: None,
            lineno: None,
            data: DirectiveData::Transaction(TransactionData {
                flag: "*".to_string(),
                payee: None,
                narration: narration.to_string(),
                tags: vec![],
                links: vec![],
                metadata: vec![],
                postings: vec![PostingData {
                    account: "Assets:Bank".to_string(),
                    units: Some(AmountData {
                        number: amount.to_string(),
                        currency: currency.to_string(),
                    }),
                    cost: None,
                    price: None,
                    flag: None,
                    metadata: vec![],
                }],
            }),
        }
    }

    #[test]
    fn matches_opposite_amounts_same_date() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn(
                    "2024-01-15",
                    "Transfer to savings",
                    "-500.00",
                    "USD",
                )],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn(
                    "2024-01-15",
                    "Transfer from checking",
                    "500.00",
                    "USD",
                )],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].amount, Decimal::new(50000, 2));
        assert!(matches[0].confidence > 0.8); // Transfer keywords + exact date
    }

    #[test]
    fn matches_within_date_window() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "ACH payment", "-200.00", "USD")],
            ),
            (
                "Assets:CreditCard".to_string(),
                vec![make_txn("2024-01-17", "Payment received", "200.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn no_match_outside_date_window() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "-500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-25", "Transfer", "500.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert!(matches.is_empty());
    }

    #[test]
    fn no_match_different_currency() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "-500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "500.00", "EUR")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert!(matches.is_empty());
    }

    #[test]
    fn no_match_same_sign() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Deposit", "500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-15", "Deposit", "500.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert!(matches.is_empty());
    }

    #[test]
    fn no_match_different_amounts() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "-500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "499.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert!(matches.is_empty());
    }

    #[test]
    fn transfer_keywords_boost_confidence() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn(
                    "2024-01-15",
                    "TRANSFER TO SAVINGS",
                    "-500.00",
                    "USD",
                )],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn(
                    "2024-01-15",
                    "TRANSFER FROM CHECKING",
                    "500.00",
                    "USD",
                )],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert_eq!(matches.len(), 1);
        // Both sides have keywords + exact date = max confidence
        assert!(matches[0].confidence >= 0.9);
    }

    #[test]
    fn no_keywords_lower_confidence() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Something", "-500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-17", "Something else", "500.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert_eq!(matches.len(), 1);
        // No keywords, different dates = base confidence only
        assert!(matches[0].confidence < 0.8);
    }

    #[test]
    fn multiple_transfers() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![
                    make_txn("2024-01-15", "Transfer 1", "-500.00", "USD"),
                    make_txn("2024-01-20", "Transfer 2", "-300.00", "USD"),
                ],
            ),
            (
                "Assets:Savings".to_string(),
                vec![
                    make_txn("2024-01-15", "Transfer 1", "500.00", "USD"),
                    make_txn("2024-01-20", "Transfer 2", "300.00", "USD"),
                ],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn one_to_one_matching() {
        // Same amount appears twice — should not double-match
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![
                    make_txn("2024-01-15", "Transfer", "-500.00", "USD"),
                    make_txn("2024-01-15", "Transfer", "-500.00", "USD"),
                ],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "500.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        // Only one match — the single savings entry can only match one checking entry
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn three_groups() {
        let groups = vec![
            (
                "Assets:Checking".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "-500.00", "USD")],
            ),
            (
                "Assets:Savings".to_string(),
                vec![make_txn("2024-01-15", "Transfer", "500.00", "USD")],
            ),
            (
                "Assets:CreditCard".to_string(),
                vec![make_txn("2024-01-15", "Payment", "200.00", "USD")],
            ),
        ];
        let matches = find_transfers(&groups, &TransferConfig::default());
        // Checking→Savings matches; CreditCard has no opposite-sign match
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn empty_groups() {
        let groups: Vec<(String, Vec<DirectiveWrapper>)> = vec![];
        let matches = find_transfers(&groups, &TransferConfig::default());
        assert!(matches.is_empty());
    }
}
