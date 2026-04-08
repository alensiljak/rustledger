//! Display context for formatting numbers with consistent precision.
//!
//! This module provides the [`DisplayContext`] type which tracks the precision
//! (number of decimal places) seen for each currency during parsing. This allows
//! numbers to be formatted consistently - for example, if a file contains both
//! `100 USD` and `50.25 USD`, both should display with 2 decimal places.
//!
//! This matches Python beancount's `display_context` behavior.
//!
//! # Example
//!
//! ```
//! use rustledger_core::DisplayContext;
//! use rust_decimal_macros::dec;
//!
//! let mut ctx = DisplayContext::new();
//!
//! // Track precision from parsed numbers
//! ctx.update(dec!(100), "USD");      // 0 decimal places
//! ctx.update(dec!(50.25), "USD");    // 2 decimal places
//! ctx.update(dec!(1.5), "EUR");      // 1 decimal place
//!
//! // Get the precision to use (maximum seen)
//! assert_eq!(ctx.get_precision("USD"), Some(2));
//! assert_eq!(ctx.get_precision("EUR"), Some(1));
//! assert_eq!(ctx.get_precision("GBP"), None);  // Never seen
//!
//! // Format a number with the tracked precision
//! assert_eq!(ctx.format(dec!(100), "USD"), "100.00");
//! assert_eq!(ctx.format(dec!(50.25), "USD"), "50.25");
//! assert_eq!(ctx.format(dec!(1.5), "EUR"), "1.5");
//! ```

use rust_decimal::Decimal;
use std::collections::HashMap;

/// Display context for formatting numbers with consistent precision per currency.
///
/// Tracks the maximum number of decimal places seen for each currency during parsing,
/// and provides methods to format numbers with that precision.
#[derive(Debug, Clone, Default)]
pub struct DisplayContext {
    /// Maximum decimal places seen per currency.
    precisions: HashMap<String, u32>,

    /// Whether to render commas in numbers (from `option "render_commas"`).
    render_commas: bool,

    /// Fixed precision overrides (from `option "display_precision"`).
    /// These take precedence over inferred precision.
    fixed_precisions: HashMap<String, u32>,
}

impl DisplayContext {
    /// Create a new empty display context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the display context with a number for a currency.
    ///
    /// This records the decimal precision of the number (number of digits after
    /// the decimal point) and updates the maximum precision seen for that currency.
    /// Update the display context with a number for a currency.
    ///
    /// Records the decimal precision and updates the maximum precision seen
    /// for that currency. Uses the normalized number's scale to avoid
    /// inflated precision from booking computations (e.g., `2.940000...`
    /// normalizes to scale 2, not 28).
    pub fn update(&mut self, number: Decimal, currency: &str) {
        // Normalize strips trailing zeros from computed values while
        // preserving meaningful precision (e.g., 111.11 stays scale 2,
        // but 2.9400000000 becomes scale 2). Numbers like 10.00 normalize
        // to scale 0, but that's fine — they contribute 0 dp which won't
        // reduce the tracked max from other numbers with real fractional parts.
        let precision = Self::decimal_precision(number.normalize());
        let entry = self.precisions.entry(currency.to_string()).or_insert(0);
        *entry = (*entry).max(precision);
    }

    /// Update the display context from another display context.
    ///
    /// Takes the maximum precision for each currency from both contexts.
    pub fn update_from(&mut self, other: &Self) {
        for (currency, precision) in &other.precisions {
            let entry = self.precisions.entry(currency.clone()).or_insert(0);
            *entry = (*entry).max(*precision);
        }
    }

    /// Set the `render_commas` flag.
    pub const fn set_render_commas(&mut self, render_commas: bool) {
        self.render_commas = render_commas;
    }

    /// Get the `render_commas` flag.
    #[must_use]
    pub const fn render_commas(&self) -> bool {
        self.render_commas
    }

    /// Set a fixed precision for a currency (from `option "display_precision"`).
    ///
    /// Fixed precision takes precedence over inferred precision.
    pub fn set_fixed_precision(&mut self, currency: &str, precision: u32) {
        self.fixed_precisions
            .insert(currency.to_string(), precision);
    }

    /// Get the precision for a currency.
    ///
    /// Returns the fixed precision if set, otherwise the maximum precision seen,
    /// or None if the currency has never been seen.
    #[must_use]
    pub fn get_precision(&self, currency: &str) -> Option<u32> {
        // Fixed precision takes precedence
        if let Some(&precision) = self.fixed_precisions.get(currency) {
            return Some(precision);
        }
        self.precisions.get(currency).copied()
    }

    /// Format a decimal number for a currency using the tracked precision.
    ///
    /// If the currency has been seen, formats with the maximum precision.
    /// Otherwise, formats with the number's natural precision (no trailing zeros).
    /// Uses half-up rounding to match Python beancount behavior.
    #[must_use]
    pub fn format(&self, number: Decimal, currency: &str) -> String {
        let precision = self.get_precision(currency);

        if let Some(dp) = precision {
            // Round with half-up (MidpointAwayFromZero) to match Python behavior
            // Note: format!("{:.N}", decimal) uses truncation which gives wrong results
            // for values like -1202.00896 (would give -1202.00 instead of -1202.01)
            let rounded = number.round_dp(dp);
            let formatted = format!("{rounded}");
            // Ensure we have the right number of decimal places (add trailing zeros if needed)
            let formatted = Self::ensure_decimal_places(&formatted, dp);
            if self.render_commas {
                Self::add_commas(&formatted)
            } else {
                formatted
            }
        } else {
            // No tracked precision - use natural formatting
            let formatted = number.normalize().to_string();
            if self.render_commas {
                Self::add_commas(&formatted)
            } else {
                formatted
            }
        }
    }

    /// Format an amount (number + currency) using the tracked precision.
    #[must_use]
    pub fn format_amount(&self, number: Decimal, currency: &str) -> String {
        format!("{} {}", self.format(number, currency), currency)
    }

    /// Get the decimal precision (number of digits after decimal point) of a number.
    const fn decimal_precision(number: Decimal) -> u32 {
        // scale() returns the number of decimal digits
        number.scale()
    }

    /// Ensure a formatted number has exactly `dp` decimal places.
    /// Adds trailing zeros if needed, or adds ".00..." if no decimal point.
    fn ensure_decimal_places(s: &str, dp: u32) -> String {
        if dp == 0 {
            // No decimal places needed - remove any decimal point
            return s.split('.').next().unwrap_or(s).to_string();
        }

        let dp = dp as usize;
        if let Some(dot_pos) = s.find('.') {
            let current_decimals = s.len() - dot_pos - 1;
            if current_decimals >= dp {
                // Already has enough or more decimals
                s.to_string()
            } else {
                // Need to add trailing zeros
                let zeros_needed = dp - current_decimals;
                format!("{s}{}", "0".repeat(zeros_needed))
            }
        } else {
            // No decimal point - add one with zeros
            format!("{s}.{}", "0".repeat(dp))
        }
    }

    /// Add thousand separators (commas) to a formatted number string.
    fn add_commas(s: &str) -> String {
        // Split on decimal point
        let (integer_part, decimal_part) = match s.find('.') {
            Some(pos) => (&s[..pos], Some(&s[pos..])),
            None => (s, None),
        };

        // Handle negative sign
        let (sign, digits) = if let Some(stripped) = integer_part.strip_prefix('-') {
            ("-", stripped)
        } else {
            ("", integer_part)
        };

        // Add commas to integer part (from right to left)
        let mut result = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, c) in digits.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        let integer_with_commas: String = result.chars().rev().collect();

        // Combine parts
        match decimal_part {
            Some(dec) => format!("{sign}{integer_with_commas}{dec}"),
            None => format!("{sign}{integer_with_commas}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_update_and_get_precision() {
        let mut ctx = DisplayContext::new();

        ctx.update(dec!(100), "USD");
        assert_eq!(ctx.get_precision("USD"), Some(0));

        ctx.update(dec!(50.25), "USD");
        assert_eq!(ctx.get_precision("USD"), Some(2));

        // Maximum is kept
        ctx.update(dec!(1), "USD");
        assert_eq!(ctx.get_precision("USD"), Some(2));

        // Unknown currency
        assert_eq!(ctx.get_precision("EUR"), None);
    }

    #[test]
    fn test_format_with_precision() {
        let mut ctx = DisplayContext::new();
        ctx.update(dec!(100), "USD");
        ctx.update(dec!(50.25), "USD");

        // Formats to max precision (2)
        assert_eq!(ctx.format(dec!(100), "USD"), "100.00");
        assert_eq!(ctx.format(dec!(50.25), "USD"), "50.25");
        assert_eq!(ctx.format(dec!(7.5), "USD"), "7.50");
    }

    #[test]
    fn test_format_unknown_currency() {
        let ctx = DisplayContext::new();

        // Unknown currency uses natural formatting
        assert_eq!(ctx.format(dec!(100), "EUR"), "100");
        assert_eq!(ctx.format(dec!(50.25), "EUR"), "50.25");
    }

    #[test]
    fn test_fixed_precision_override() {
        let mut ctx = DisplayContext::new();
        ctx.update(dec!(100), "USD");
        ctx.update(dec!(50.25), "USD");

        // Inferred precision is 2
        assert_eq!(ctx.get_precision("USD"), Some(2));

        // Set fixed precision to 4
        ctx.set_fixed_precision("USD", 4);
        assert_eq!(ctx.get_precision("USD"), Some(4));

        // Formatting uses fixed precision
        assert_eq!(ctx.format(dec!(100), "USD"), "100.0000");
    }

    #[test]
    fn test_render_commas() {
        let mut ctx = DisplayContext::new();
        ctx.set_render_commas(true);
        ctx.update(dec!(1234567.89), "USD");

        assert_eq!(ctx.format(dec!(1234567.89), "USD"), "1,234,567.89");
        assert_eq!(ctx.format(dec!(1000), "USD"), "1,000.00");
    }

    #[test]
    fn test_add_commas() {
        assert_eq!(DisplayContext::add_commas("1234567"), "1,234,567");
        assert_eq!(DisplayContext::add_commas("1234567.89"), "1,234,567.89");
        assert_eq!(DisplayContext::add_commas("-1234567.89"), "-1,234,567.89");
        assert_eq!(DisplayContext::add_commas("123"), "123");
        assert_eq!(DisplayContext::add_commas("1"), "1");
    }

    #[test]
    fn test_update_from() {
        let mut ctx1 = DisplayContext::new();
        ctx1.update(dec!(100), "USD");

        let mut ctx2 = DisplayContext::new();
        ctx2.update(dec!(50.25), "USD");
        ctx2.update(dec!(1.5), "EUR");

        ctx1.update_from(&ctx2);

        assert_eq!(ctx1.get_precision("USD"), Some(2));
        assert_eq!(ctx1.get_precision("EUR"), Some(1));
    }

    #[test]
    fn test_format_amount() {
        let mut ctx = DisplayContext::new();
        ctx.update(dec!(50.25), "USD");

        assert_eq!(ctx.format_amount(dec!(100), "USD"), "100.00 USD");
    }
}
