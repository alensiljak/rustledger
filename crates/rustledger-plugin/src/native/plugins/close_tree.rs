//! Close descendant accounts automatically.

use crate::types::{
    CloseData, DirectiveData, DirectiveWrapper, PluginInput, PluginOutput, sort_directives,
};

use super::super::NativePlugin;

/// Plugin that closes all descendant accounts when a parent account closes.
///
/// When an account like `Assets:Bank` is closed, this plugin also generates
/// close directives for all sub-accounts like `Assets:Bank:Checking`.
pub struct CloseTreePlugin;

impl NativePlugin for CloseTreePlugin {
    fn name(&self) -> &'static str {
        "close_tree"
    }

    fn description(&self) -> &'static str {
        "Close descendant accounts automatically"
    }

    fn process(&self, input: PluginInput) -> PluginOutput {
        use std::collections::HashSet;

        // Collect all accounts that are used
        let mut all_accounts: HashSet<String> = HashSet::new();
        for wrapper in &input.directives {
            if let DirectiveData::Open(data) = &wrapper.data {
                all_accounts.insert(data.account.clone());
            }
            if let DirectiveData::Transaction(txn) = &wrapper.data {
                for posting in &txn.postings {
                    all_accounts.insert(posting.account.clone());
                }
            }
        }

        // Collect accounts that are explicitly closed
        let mut closed_parents: Vec<(String, String)> = Vec::new(); // (account, date)
        for wrapper in &input.directives {
            if let DirectiveData::Close(data) = &wrapper.data {
                closed_parents.push((data.account.clone(), wrapper.date.clone()));
            }
        }

        // Find child accounts for each closed parent
        let mut new_directives = input.directives;

        for (parent, close_date) in &closed_parents {
            let prefix = format!("{parent}:");
            for account in &all_accounts {
                if account.starts_with(&prefix) {
                    // Check if already closed
                    let already_closed = new_directives.iter().any(|w| {
                        if let DirectiveData::Close(data) = &w.data {
                            &data.account == account
                        } else {
                            false
                        }
                    });

                    if !already_closed {
                        new_directives.push(DirectiveWrapper {
                            directive_type: "close".to_string(),
                            date: close_date.clone(),
                            filename: None, // Plugin-generated
                            lineno: None,
                            data: DirectiveData::Close(CloseData {
                                account: account.clone(),
                                metadata: vec![],
                            }),
                        });
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
