//! Transaction validation.

use rust_decimal::Decimal;
use rustledger_core::{Amount, BookingMethod, Inventory, Posting, Transaction};
use std::collections::HashMap;

use crate::error::{ErrorCode, ValidationError};
use crate::{AccountState, LedgerState, ValidationOptions};

/// Validate a Transaction directive.
pub(crate) fn validate_transaction(
    state: &mut LedgerState,
    txn: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    // Check transaction structure
    if !validate_transaction_structure(txn, errors) {
        return; // No point checking further if no postings
    }

    // Check each posting's account lifecycle and currency constraints
    validate_posting_accounts(state, txn, errors);

    // Check transaction balance
    validate_transaction_balance(txn, &state.options, errors);

    // Update inventories with booking validation
    update_inventories(state, txn, errors);
}

/// Validate transaction structure.
/// Returns false if validation should stop (no postings to validate).
///
/// Note: Python beancount allows transactions with zero postings (metadata-only transactions).
/// Single-posting transactions are allowed structurally but will fail balance checking.
pub(crate) fn validate_transaction_structure(
    txn: &Transaction,
    errors: &mut Vec<ValidationError>,
) -> bool {
    if txn.postings.is_empty() {
        // Python beancount allows transactions with no postings (metadata-only).
        // No error, but skip further validation since there's nothing to validate.
        return false;
    }

    // Warn about single posting (structurally valid but will fail balance check)
    if txn.postings.len() == 1 {
        errors.push(ValidationError::new(
            ErrorCode::SinglePosting,
            "Transaction has only one posting".to_string(),
            txn.date,
        ));
    }

    true
}

/// Validate account lifecycle and currency constraints for each posting.
pub(crate) fn validate_posting_accounts(
    state: &LedgerState,
    txn: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    for posting in &txn.postings {
        match state.accounts.get(&posting.account) {
            Some(account_state) => {
                validate_account_lifecycle(txn, posting, account_state, errors);
                validate_posting_currency(state, txn, posting, account_state, errors);
            }
            None => {
                errors.push(ValidationError::new(
                    ErrorCode::AccountNotOpen,
                    format!("Account {} was never opened", posting.account),
                    txn.date,
                ));
            }
        }
    }
}

/// Validate that an account is open at transaction time and not closed.
pub(crate) fn validate_account_lifecycle(
    txn: &Transaction,
    posting: &Posting,
    account_state: &AccountState,
    errors: &mut Vec<ValidationError>,
) {
    if txn.date < account_state.opened {
        errors.push(ValidationError::new(
            ErrorCode::AccountNotOpen,
            format!(
                "Account {} used on {} but not opened until {}",
                posting.account, txn.date, account_state.opened
            ),
            txn.date,
        ));
    }

    if let Some(closed) = account_state.closed {
        if txn.date >= closed {
            errors.push(ValidationError::new(
                ErrorCode::AccountClosed,
                format!(
                    "Account {} used on {} but was closed on {}",
                    posting.account, txn.date, closed
                ),
                txn.date,
            ));
        }
    }
}

/// Validate currency constraints and commodity declarations for a posting.
pub(crate) fn validate_posting_currency(
    state: &LedgerState,
    txn: &Transaction,
    posting: &Posting,
    account_state: &AccountState,
    errors: &mut Vec<ValidationError>,
) {
    let Some(units) = posting.amount() else {
        return;
    };

    // Check currency constraints
    if !account_state.currencies.is_empty() && !account_state.currencies.contains(&units.currency) {
        errors.push(ValidationError::new(
            ErrorCode::CurrencyNotAllowed,
            format!(
                "Currency {} not allowed in account {}",
                units.currency, posting.account
            ),
            txn.date,
        ));
    }

    // Check commodity declaration
    if state.options.require_commodities && !state.commodities.contains(&units.currency) {
        errors.push(ValidationError::new(
            ErrorCode::UndeclaredCurrency,
            format!("Currency {} not declared", units.currency),
            txn.date,
        ));
    }
}

/// Validate that the transaction balances within tolerance.
///
/// Tolerance is calculated per-currency based on:
/// 1. The quantum (precision) of amounts in postings
/// 2. Cost-based tolerance when `infer_tolerance_from_cost` is enabled:
///    `tolerance = units_quantum * cost_per_unit * tolerance_multiplier`
pub(crate) fn validate_transaction_balance(
    txn: &Transaction,
    options: &ValidationOptions,
    errors: &mut Vec<ValidationError>,
) {
    // Skip balance checking if there are any empty cost specs (e.g., `{}`).
    // Empty cost specs will have their cost filled in by lot matching during booking,
    // and if there's no matching lot, that error will be reported separately.
    // This matches Python beancount behavior where booking runs before balance checking.
    let has_empty_cost_spec = txn.postings.iter().any(|p| {
        if let Some(cost) = &p.cost {
            // Empty cost spec: no per-unit cost, no total cost
            cost.number_per.is_none() && cost.number_total.is_none()
        } else {
            false
        }
    });
    if has_empty_cost_spec {
        return; // Lot matching will validate this transaction
    }

    let residuals = rustledger_booking::calculate_residual(txn);

    // Calculate per-currency tolerance based on postings
    let tolerances = calculate_tolerances(txn, options);

    for (currency, residual) in residuals {
        // Get the tolerance for this currency, defaulting to 0.005
        let tolerance = tolerances
            .get(currency.as_str())
            .copied()
            .unwrap_or_else(|| Decimal::new(5, 3));

        if residual.abs() > tolerance {
            errors.push(ValidationError::new(
                ErrorCode::TransactionUnbalanced,
                format!("Transaction does not balance: residual {residual} {currency}"),
                txn.date,
            ));
        }
    }
}

/// Calculate the quantum (smallest unit) of a decimal number based on its precision.
/// For example: 10.436 has quantum 0.001, 100.00 has quantum 0.01
pub(crate) fn decimal_quantum(value: Decimal) -> Decimal {
    let scale = value.scale();
    if scale == 0 {
        Decimal::ONE
    } else {
        Decimal::new(1, scale)
    }
}

/// Calculate per-currency tolerances for a transaction.
///
/// When `infer_tolerance_from_cost` is enabled, for each posting with a cost:
///   `tolerance = units_quantum * cost_per_unit * tolerance_multiplier`
///
/// The tolerance for each cost currency is the maximum of all such values
/// computed from postings with costs in that currency.
pub(crate) fn calculate_tolerances(
    txn: &Transaction,
    options: &ValidationOptions,
) -> HashMap<String, Decimal> {
    let mut tolerances: HashMap<String, Decimal> = HashMap::new();

    // Default tolerance based on quantum of amounts in postings
    for posting in &txn.postings {
        if let Some(units) = posting.amount() {
            let quantum = decimal_quantum(units.number);
            // Use half the quantum as base tolerance (like Python beancount)
            let base_tolerance = quantum * options.tolerance_multiplier;

            tolerances
                .entry(units.currency.to_string())
                .and_modify(|t| *t = (*t).max(base_tolerance))
                .or_insert(base_tolerance);
        }
    }

    // Calculate cost-inferred tolerance if enabled
    if options.infer_tolerance_from_cost {
        for posting in &txn.postings {
            if let (Some(units), Some(cost_spec)) = (posting.amount(), &posting.cost) {
                // Get the cost per unit
                if let Some(cost_per_unit) = cost_spec.number_per {
                    // Get the cost currency
                    if let Some(cost_currency) = &cost_spec.currency {
                        // Calculate: units_quantum * cost_per_unit * multiplier
                        let units_quantum = decimal_quantum(units.number);
                        let cost_tolerance =
                            units_quantum * cost_per_unit * options.tolerance_multiplier;

                        // Update tolerance for the cost currency (take max)
                        tolerances
                            .entry(cost_currency.to_string())
                            .and_modify(|t| *t = (*t).max(cost_tolerance))
                            .or_insert(cost_tolerance);
                    }
                }
            }
        }
    }

    tolerances
}

/// Update inventories with booking validation for each posting.
pub(crate) fn update_inventories(
    state: &mut LedgerState,
    txn: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    for posting in &txn.postings {
        let Some(units) = posting.amount() else {
            continue;
        };
        let Some(inv) = state.inventories.get_mut(&posting.account) else {
            continue;
        };

        let booking_method = state
            .accounts
            .get(&posting.account)
            .map(|a| a.booking)
            .unwrap_or_default();

        let is_reduction = units.number.is_sign_negative() && posting.cost.is_some();

        if is_reduction {
            process_inventory_reduction(inv, posting, units, booking_method, txn, errors);
        } else {
            process_inventory_addition(inv, posting, units, txn);
        }
    }
}

/// Process an inventory reduction (selling/removing units).
pub(crate) fn process_inventory_reduction(
    inv: &mut Inventory,
    posting: &Posting,
    units: &Amount,
    booking_method: BookingMethod,
    txn: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    match inv.reduce(units, posting.cost.as_ref(), booking_method) {
        Ok(_) => {}
        Err(rustledger_core::BookingError::InsufficientUnits {
            requested,
            available,
            ..
        }) => {
            errors.push(
                ValidationError::new(
                    ErrorCode::InsufficientUnits,
                    format!(
                        "Insufficient units in {}: requested {}, available {}",
                        posting.account, requested, available
                    ),
                    txn.date,
                )
                .with_context(format!("currency: {}", units.currency)),
            );
        }
        Err(rustledger_core::BookingError::NoMatchingLot { currency, .. }) => {
            // In STRICT mode, when no lot matches AND the inventory has no POSITIVE
            // positions for this commodity, Python beancount allows "sell to open"
            // by creating a new lot with negative units. This is common in options trading.
            // However, if there ARE positive lots that just don't match the cost spec,
            // that's an error (you're trying to sell from a lot that doesn't exist).
            // We only check for positive lots because negative lots are short positions
            // from previous sell-to-open operations.
            let has_positive_lots = inv
                .positions()
                .iter()
                .any(|p| p.units.currency == units.currency && p.units.number > Decimal::ZERO);

            if booking_method == BookingMethod::Strict && !has_positive_lots {
                if let Some(cost_spec) = &posting.cost {
                    // Need cost per unit (or total) and currency to create a new lot
                    let cost_number = cost_spec
                        .number_per
                        .or_else(|| cost_spec.number_total.map(|t| t / units.number.abs()));

                    // Infer currency from cost spec, price annotation, or fall back
                    let cost_currency = cost_spec.currency.clone().or_else(|| {
                        // Try to get currency from price annotation
                        posting.price.as_ref().and_then(|p| match p {
                            rustledger_core::PriceAnnotation::Unit(a)
                            | rustledger_core::PriceAnnotation::Total(a) => {
                                Some(a.currency.clone())
                            }
                            rustledger_core::PriceAnnotation::UnitIncomplete(inc)
                            | rustledger_core::PriceAnnotation::TotalIncomplete(inc) => {
                                inc.as_amount().map(|a| a.currency.clone())
                            }
                            _ => None,
                        })
                    });

                    if let (Some(number), Some(curr)) = (cost_number, cost_currency) {
                        // Create a new position with negative units (sell to open)
                        let cost = rustledger_core::Cost::new(number, curr)
                            .with_date(cost_spec.date.unwrap_or(txn.date));
                        let cost = if let Some(label) = &cost_spec.label {
                            cost.with_label(label.clone())
                        } else {
                            cost
                        };
                        let position = rustledger_core::Position::with_cost(units.clone(), cost);
                        inv.add(position);
                        return; // Successfully created sell-to-open position
                    }
                }
            }
            // Couldn't create sell-to-open (or has existing lots that don't match), report error
            errors.push(
                ValidationError::new(
                    ErrorCode::NoMatchingLot,
                    format!("No matching lot for {} in {}", currency, posting.account),
                    txn.date,
                )
                .with_context(format!("cost spec: {:?}", posting.cost)),
            );
        }
        Err(rustledger_core::BookingError::AmbiguousMatch {
            currency,
            num_matches,
        }) => {
            errors.push(
                ValidationError::new(
                    ErrorCode::AmbiguousLotMatch,
                    format!(
                        "Ambiguous lot match for {}: {} lots match in {}",
                        currency, num_matches, posting.account
                    ),
                    txn.date,
                )
                .with_context("Specify cost, date, or label to disambiguate".to_string()),
            );
        }
        Err(rustledger_core::BookingError::CurrencyMismatch { .. }) => {
            // This shouldn't happen in normal validation
        }
    }
}

/// Process an inventory addition (buying/adding units).
pub(crate) fn process_inventory_addition(
    inv: &mut Inventory,
    posting: &Posting,
    units: &Amount,
    txn: &Transaction,
) {
    let position = if let Some(cost_spec) = &posting.cost {
        if let Some(cost) = cost_spec.resolve(units.number, txn.date) {
            rustledger_core::Position::with_cost(units.clone(), cost)
        } else {
            rustledger_core::Position::simple(units.clone())
        }
    } else {
        rustledger_core::Position::simple(units.clone())
    };

    inv.add(position);
}
