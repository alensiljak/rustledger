//! LLM prompt building for transaction categorization.
//!
//! Provides utilities for building prompts that ask an LLM to categorize
//! transactions. Designed to be used via the MCP server, where the LLM
//! acts as the third tier of categorization (after rules and ML).

use rustledger_plugin_types::{DirectiveData, DirectiveWrapper};

/// A request to categorize a transaction via LLM.
#[derive(Debug, Clone)]
pub struct CategorizationRequest {
    /// The payee (if available).
    pub payee: Option<String>,
    /// The narration/description.
    pub narration: String,
    /// The amount (as a string).
    pub amount: Option<String>,
    /// The currency.
    pub currency: Option<String>,
    /// The date.
    pub date: String,
    /// List of known accounts in the ledger (for constrained prediction).
    pub known_accounts: Vec<String>,
}

/// A parsed categorization response from the LLM.
#[derive(Debug, Clone)]
pub struct CategorizationResponse {
    /// The predicted account.
    pub account: String,
    /// Brief reasoning for the prediction.
    pub reasoning: String,
}

/// Build a prompt for transaction categorization.
///
/// The prompt includes the transaction details and a list of known accounts,
/// asking the LLM to select the most appropriate account and explain why.
#[must_use]
pub fn build_categorization_prompt(request: &CategorizationRequest) -> String {
    let mut prompt = String::new();

    prompt.push_str("Categorize this financial transaction into the most appropriate account.\n\n");
    prompt.push_str("Transaction:\n");
    prompt.push_str(&format!("  Date: {}\n", request.date));
    if let Some(ref payee) = request.payee {
        prompt.push_str(&format!("  Payee: {payee}\n"));
    }
    prompt.push_str(&format!("  Description: {}\n", request.narration));
    if let Some(ref amount) = request.amount {
        let currency = request.currency.as_deref().unwrap_or("USD");
        prompt.push_str(&format!("  Amount: {amount} {currency}\n"));
    }

    prompt.push_str("\nAvailable accounts:\n");
    for account in &request.known_accounts {
        prompt.push_str(&format!("  - {account}\n"));
    }

    prompt.push_str("\nRespond with ONLY the account name on the first line, ");
    prompt.push_str("followed by a brief reason on the second line.\n");
    prompt.push_str("Example:\n");
    prompt.push_str("Expenses:Groceries\n");
    prompt.push_str("Whole Foods is a grocery store\n");

    prompt
}

/// Parse an LLM response into a structured categorization.
///
/// Expects the account name on the first line and reasoning on the second.
/// Returns `None` if the response can't be parsed.
#[must_use]
pub fn parse_categorization_response(response: &str) -> Option<CategorizationResponse> {
    let mut lines = response.trim().lines();
    let account = lines.next()?.trim().to_string();

    // Validate it looks like an account (contains ':')
    if !account.contains(':') {
        return None;
    }

    let reasoning = lines.next().unwrap_or("").trim().to_string();

    Some(CategorizationResponse { account, reasoning })
}

/// Extract known expense/income accounts from directives for prompt building.
#[must_use]
pub fn extract_known_accounts(directives: &[DirectiveWrapper]) -> Vec<String> {
    let mut accounts = std::collections::BTreeSet::new();

    for d in directives {
        match &d.data {
            DirectiveData::Transaction(txn) => {
                for posting in &txn.postings {
                    if posting.account.starts_with("Expenses:")
                        || posting.account.starts_with("Income:")
                    {
                        accounts.insert(posting.account.clone());
                    }
                }
            }
            DirectiveData::Open(open)
                if (open.account.starts_with("Expenses:")
                    || open.account.starts_with("Income:")) =>
            {
                accounts.insert(open.account.clone());
            }
            _ => {}
        }
    }

    accounts.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_basic() {
        let request = CategorizationRequest {
            payee: Some("WHOLE FOODS MARKET".to_string()),
            narration: "Groceries".to_string(),
            amount: Some("-85.23".to_string()),
            currency: Some("USD".to_string()),
            date: "2024-01-15".to_string(),
            known_accounts: vec![
                "Expenses:Groceries".to_string(),
                "Expenses:Dining".to_string(),
                "Expenses:Transport".to_string(),
            ],
        };
        let prompt = build_categorization_prompt(&request);
        assert!(prompt.contains("WHOLE FOODS MARKET"));
        assert!(prompt.contains("-85.23 USD"));
        assert!(prompt.contains("Expenses:Groceries"));
        assert!(prompt.contains("Expenses:Dining"));
    }

    #[test]
    fn parse_response_valid() {
        let response = "Expenses:Groceries\nWhole Foods is a grocery store";
        let parsed = parse_categorization_response(response).unwrap();
        assert_eq!(parsed.account, "Expenses:Groceries");
        assert_eq!(parsed.reasoning, "Whole Foods is a grocery store");
    }

    #[test]
    fn parse_response_no_reasoning() {
        let response = "Expenses:Dining\n";
        let parsed = parse_categorization_response(response).unwrap();
        assert_eq!(parsed.account, "Expenses:Dining");
        assert_eq!(parsed.reasoning, "");
    }

    #[test]
    fn parse_response_invalid() {
        let response = "This is not an account";
        assert!(parse_categorization_response(response).is_none());
    }

    #[test]
    fn extract_accounts() {
        use rustledger_plugin_types::{AmountData, OpenData, PostingData, TransactionData};

        let directives = vec![
            DirectiveWrapper {
                directive_type: "open".to_string(),
                date: "2024-01-01".to_string(),
                filename: None,
                lineno: None,
                data: DirectiveData::Open(OpenData {
                    account: "Expenses:Groceries".to_string(),
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
                    account: "Assets:Bank".to_string(),
                    currencies: vec![],
                    booking: None,
                    metadata: vec![],
                }),
            },
        ];
        let accounts = extract_known_accounts(&directives);
        assert_eq!(accounts, vec!["Expenses:Groceries"]);
        // Assets:Bank is excluded (not Expenses: or Income:)
    }
}
