//! Balance and pad validation.

use rust_decimal::Decimal;
use rustledger_core::{Amount, Balance, Pad, Position};

use crate::error::{ErrorCode, ValidationError};
use crate::{LedgerState, PendingPad};

/// Validate a Pad directive.
pub(crate) fn validate_pad(state: &mut LedgerState, pad: &Pad, errors: &mut Vec<ValidationError>) {
    // Check that the target account exists
    if !state.accounts.contains_key(&pad.account) {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!("Pad target account {} was never opened", pad.account),
            pad.date,
        ));
        return;
    }

    // Check that the source account exists
    if !state.accounts.contains_key(&pad.source_account) {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!("Pad source account {} was never opened", pad.source_account),
            pad.date,
        ));
        return;
    }

    // Add to pending pads list for this account
    let pending_pad = PendingPad {
        source_account: pad.source_account.clone(),
        date: pad.date,
        used: false,
    };
    state
        .pending_pads
        .entry(pad.account.clone())
        .or_default()
        .push(pending_pad);
}

/// Validate a Balance directive.
pub(crate) fn validate_balance(
    state: &mut LedgerState,
    bal: &Balance,
    errors: &mut Vec<ValidationError>,
) {
    // Check account exists
    if !state.accounts.contains_key(&bal.account) {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!("Account {} was never opened", bal.account),
            bal.date,
        ));
        return;
    }

    // Check if there are pending pads for this account
    // Use get_mut instead of remove - a pad can apply to multiple currencies
    if let Some(pending_pads) = state.pending_pads.get_mut(&bal.account) {
        // Check for multiple pads (E2004) - only warn if none have been used yet
        if pending_pads.len() > 1 && !pending_pads.iter().any(|p| p.used) {
            errors.push(
                ValidationError::new(
                    ErrorCode::MultiplePadForBalance,
                    format!(
                        "Multiple pad directives for {} {} before balance assertion",
                        bal.account, bal.amount.currency
                    ),
                    bal.date,
                )
                .with_context(format!(
                    "pad dates: {}",
                    pending_pads
                        .iter()
                        .map(|p| p.date.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
        }

        // Use the most recent pad
        if let Some(pending_pad) = pending_pads.last_mut() {
            // Apply padding: calculate difference and add to both accounts
            // Balance assertions include sub-accounts, so sum them all up
            let mut actual = Decimal::ZERO;
            let account_prefix = format!("{}:", bal.account);
            for (account, inv) in &state.inventories {
                if account == &bal.account || account.starts_with(&account_prefix) {
                    actual += inv.units(&bal.amount.currency);
                }
            }
            {
                let expected = bal.amount.number;
                let difference = expected - actual;

                if difference != Decimal::ZERO {
                    // Add padding amount to target account
                    if let Some(target_inv) = state.inventories.get_mut(&bal.account) {
                        target_inv.add(Position::simple(Amount::new(
                            difference,
                            &bal.amount.currency,
                        )));
                    }

                    // Subtract padding amount from source account
                    if let Some(source_inv) = state.inventories.get_mut(&pending_pad.source_account)
                    {
                        source_inv.add(Position::simple(Amount::new(
                            -difference,
                            &bal.amount.currency,
                        )));
                    }

                    // Mark pad as used only if padding was actually needed
                    pending_pad.used = true;
                }
            }
        }
        // After padding, the balance should match (no error needed)
        return;
    }

    // Get inventory and check balance (no padding case)
    // In beancount, balance assertions include sub-accounts
    // e.g., balance Assets:Checking includes Assets:Checking:Sub1, Assets:Checking:Sub2, etc.
    let mut actual = Decimal::ZERO;
    let account_prefix = format!("{}:", bal.account);
    for (account, inv) in &state.inventories {
        // Include exact match or sub-accounts (account:*)
        if account == &bal.account || account.starts_with(&account_prefix) {
            actual += inv.units(&bal.amount.currency);
        }
    }

    if actual != Decimal::ZERO || state.inventories.contains_key(&bal.account) {
        let expected = bal.amount.number;
        let difference = (actual - expected).abs();

        // Determine tolerance and whether it was explicitly specified
        let (tolerance, is_explicit) = if let Some(t) = bal.tolerance {
            (t, true)
        } else {
            (bal.amount.inferred_tolerance(), false)
        };

        if difference > tolerance {
            // Use E2002 for explicit tolerance, E2001 for inferred
            let error_code = if is_explicit {
                ErrorCode::BalanceToleranceExceeded
            } else {
                ErrorCode::BalanceAssertionFailed
            };

            let message = if is_explicit {
                format!(
                    "Balance exceeds explicit tolerance for {}: expected {} {} ~ {}, got {} {} (difference: {})",
                    bal.account,
                    expected,
                    bal.amount.currency,
                    tolerance,
                    actual,
                    bal.amount.currency,
                    difference
                )
            } else {
                format!(
                    "Balance assertion failed for {}: expected {} {}, got {} {}",
                    bal.account, expected, bal.amount.currency, actual, bal.amount.currency
                )
            };

            errors.push(
                ValidationError::new(error_code, message, bal.date)
                    .with_context(format!("difference: {difference}, tolerance: {tolerance}")),
            );
        }
    }
}
