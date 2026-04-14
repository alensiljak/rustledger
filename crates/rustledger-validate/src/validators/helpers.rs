//! Helper functions for validation.

/// Validate an account name according to beancount rules.
/// Returns None if valid, or Some(reason) if invalid.
///
/// The `account_types` parameter specifies valid account type prefixes (from options
/// like `name_assets`, `name_liabilities`, etc.). Defaults are: Assets, Liabilities,
/// Equity, Income, Expenses.
pub fn validate_account_name(account: &str, account_types: &[String]) -> Option<String> {
    if account.is_empty() {
        return Some("account name is empty".to_string());
    }

    // Iterate components without allocating a Vec
    let mut components = account.split(':');

    // Check root account type (first component)
    let root = components.next()?;
    if root.is_empty() {
        return Some("component 1 is empty".to_string());
    }
    if !account_types.iter().any(|t| t == root) {
        return Some(format!(
            "account must start with one of: {}",
            account_types.join(", ")
        ));
    }

    // Check each component (starting from root)
    // We already validated root's content will be checked below
    for (i, part) in std::iter::once(root).chain(components).enumerate() {
        if part.is_empty() {
            return Some(format!("component {} is empty", i + 1));
        }

        // First character must be uppercase ASCII letter, ASCII digit, or any non-ASCII character.
        // This matches beancount's lexer.l: `([A-Z]|UTF-8-ONLY)` for account type start,
        // and `([A-Z0-9]|UTF-8-ONLY)` for sub-account start.
        // Non-ASCII includes CJK, emojis, symbols, and any other Unicode character.
        // Safety: we just checked part.is_empty() above, so this is guaranteed to succeed
        let Some(first_char) = part.chars().next() else {
            // This branch is unreachable due to the is_empty check above,
            // but we handle it defensively to avoid unwrap
            return Some(format!("component {} is empty", i + 1));
        };
        // Accept: uppercase ASCII letters, ASCII digits, or any non-ASCII character
        let is_valid_start = first_char.is_ascii_uppercase()
            || first_char.is_ascii_digit()
            || !first_char.is_ascii();
        if !is_valid_start {
            return Some(format!(
                "component '{part}' must start with uppercase letter or digit"
            ));
        }

        // Remaining characters: ASCII letters, ASCII digits, hyphens, or any non-ASCII character.
        // This matches beancount's lexer.l: `([A-Za-z0-9-]|UTF-8-ONLY)*`
        for c in part.chars().skip(1) {
            if !c.is_ascii_alphanumeric() && c != '-' && c.is_ascii() {
                return Some(format!(
                    "component '{part}' contains invalid character '{c}'"
                ));
            }
        }
    }

    None // Valid
}
