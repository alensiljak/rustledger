//! Configuration for importers.

use crate::ImportResult;
use crate::csv_importer::CsvImporter;
use anyhow::{Context, Result};
use format_num_pattern::{Locale, NumberFormat, NumberSymbols, core::parse_sym, fmt_to, parse_fmt};
use rust_decimal::Decimal;
use std::{fmt::Display, ops::Neg, path::Path};

/// Configuration for an importer.
#[derive(Debug, Clone)]
pub struct ImporterConfig {
    /// The target account for imported transactions.
    pub account: String,
    /// The currency for amounts (if not specified in the file).
    pub currency: Option<String>,
    /// Amount parser
    pub amount_format: AmountFormat,
    /// The importer type and its specific configuration.
    pub importer_type: ImporterType,
}

/// Type of importer with its specific configuration.
#[derive(Debug, Clone)]
pub enum ImporterType {
    /// CSV file importer.
    Csv(CsvConfig),
}

/// Configuration specific to CSV imports.
#[derive(Debug, Clone)]
pub struct CsvConfig {
    /// The column name or index for the date.
    pub date_column: ColumnSpec,
    /// The date format (strftime-style).
    pub date_format: String,
    /// The column name or index for the narration/description.
    pub narration_column: Option<ColumnSpec>,
    /// The column name or index for the payee.
    pub payee_column: Option<ColumnSpec>,
    /// The column name or index for the amount.
    pub amount_column: Option<ColumnSpec>,
    /// Amount locale, see <https://docs.rs/format_num_pattern/latest/format_num_pattern/index.html>
    pub amount_locale: Option<Locale>,
    /// Amount format, see <https://docs.rs/format_num_pattern/latest/format_num_pattern/index.html>
    pub amount_format: Option<String>,
    /// The column name or index for debit amounts (if separate from credit).
    pub debit_column: Option<ColumnSpec>,
    /// The column name or index for credit amounts (if separate from debit).
    pub credit_column: Option<ColumnSpec>,
    /// Whether the CSV has a header row.
    pub has_header: bool,
    /// The field delimiter.
    pub delimiter: char,
    /// Number of rows to skip at the beginning.
    pub skip_rows: usize,
    /// Whether to invert the sign of amounts.
    pub invert_sign: bool,
}

impl Default for CsvConfig {
    fn default() -> Self {
        Self {
            date_column: ColumnSpec::Name("Date".to_string()),
            date_format: "%Y-%m-%d".to_string(),
            narration_column: Some(ColumnSpec::Name("Description".to_string())),
            payee_column: None,
            amount_column: Some(ColumnSpec::Name("Amount".to_string())),
            amount_locale: Some(Locale::POSIX),
            amount_format: None,
            debit_column: None,
            credit_column: None,
            has_header: true,
            delimiter: ',',
            skip_rows: 0,
            invert_sign: false,
        }
    }
}

/// Specification for a column in the source file.
#[derive(Debug, Clone)]
pub enum ColumnSpec {
    /// Column specified by name (from header).
    Name(String),
    /// Column specified by zero-based index.
    Index(usize),
}

/// Localized/custom amount parsing
#[derive(Debug, Clone)]
pub enum AmountFormat {
    /// Override only symbols.
    Symbols(NumberSymbols),
    /// Override format.
    Format(NumberFormat),
}

impl AmountFormat {
    /// Attempt to parse a string using the given format.
    pub fn parse(&self, amount: &str) -> Result<Decimal> {
        let value: Decimal = match self {
            Self::Symbols(number_symbols) => parse_sym(amount, number_symbols)
                .with_context(|| format!("unable to parse using symbols: {number_symbols:?}")),
            Self::Format(number_format) => parse_fmt(amount, number_format)
                .with_context(|| format!("unable to parse using given format: {number_format}")),
        }?;

        if amount.trim().starts_with('(') && amount.trim().ends_with(')') {
            Ok(value.neg())
        } else {
            Ok(value)
        }
    }

    /// Apply formatting to a decimal amount, making it printable.
    pub const fn apply(&self, amount: Decimal) -> FormattedAmount<'_> {
        FormattedAmount {
            amount,
            formatter: self,
        }
    }

    fn fmt_into<W: core::fmt::Write>(&self, amount: Decimal, writer: &mut W) {
        match self {
            Self::Symbols(number_symbols) => fmt_to(
                amount,
                &NumberFormat::news("###,##0.##", *number_symbols).unwrap(),
                writer,
            ),
            Self::Format(number_format) => fmt_to(amount, number_format, writer),
        }
    }
}

/// Formatted wrapper around a decimal, making it printable.
#[derive(Debug, Clone)]
pub struct FormattedAmount<'a> {
    amount: Decimal,
    formatter: &'a AmountFormat,
}

impl Display for FormattedAmount<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.formatter.fmt_into(self.amount, f);
        Ok(())
    }
}

impl Default for AmountFormat {
    fn default() -> Self {
        Self::Symbols(NumberSymbols::monetary(Locale::POSIX))
    }
}

impl ImporterConfig {
    /// Start building a CSV importer configuration.
    pub fn csv() -> CsvConfigBuilder {
        CsvConfigBuilder::new()
    }

    /// Extract transactions from a file.
    pub fn extract(&self, path: &Path) -> Result<ImportResult> {
        match &self.importer_type {
            ImporterType::Csv(csv_config) => {
                let importer = CsvImporter::new(self.clone());
                importer.extract_file(path, csv_config)
            }
        }
    }

    /// Extract transactions from string content.
    pub fn extract_from_string(&self, content: &str) -> Result<ImportResult> {
        match &self.importer_type {
            ImporterType::Csv(csv_config) => {
                let importer = CsvImporter::new(self.clone());
                importer.extract_string(content, csv_config)
            }
        }
    }
}

/// Builder for CSV importer configuration.
pub struct CsvConfigBuilder {
    account: Option<String>,
    currency: Option<String>,
    config: CsvConfig,
}

impl CsvConfigBuilder {
    /// Create a new CSV config builder.
    pub fn new() -> Self {
        Self {
            account: None,
            currency: None,
            config: CsvConfig::default(),
        }
    }

    /// Set the target account.
    pub fn account(mut self, account: impl Into<String>) -> Self {
        self.account = Some(account.into());
        self
    }

    /// Set the currency for amounts.
    pub fn currency(mut self, currency: impl Into<String>) -> Self {
        self.currency = Some(currency.into());
        self
    }

    /// Set the amount locale
    pub fn amount_locale(mut self, locale: impl Into<Locale>) -> Self {
        self.config.amount_locale = Some(locale.into());
        self
    }

    /// Set the amount format
    pub fn amount_format(mut self, format: impl Into<String>) -> Self {
        self.config.amount_format = Some(format.into());
        self
    }

    /// Set the date column by name.
    pub fn date_column(mut self, name: impl Into<String>) -> Self {
        self.config.date_column = ColumnSpec::Name(name.into());
        self
    }

    /// Set the date column by index.
    pub fn date_column_index(mut self, index: usize) -> Self {
        self.config.date_column = ColumnSpec::Index(index);
        self
    }

    /// Set the date format (strftime-style).
    pub fn date_format(mut self, format: impl Into<String>) -> Self {
        self.config.date_format = format.into();
        self
    }

    /// Set the narration/description column by name.
    pub fn narration_column(mut self, name: impl Into<String>) -> Self {
        self.config.narration_column = Some(ColumnSpec::Name(name.into()));
        self
    }

    /// Set the narration column by index.
    pub fn narration_column_index(mut self, index: usize) -> Self {
        self.config.narration_column = Some(ColumnSpec::Index(index));
        self
    }

    /// Set the payee column by name.
    pub fn payee_column(mut self, name: impl Into<String>) -> Self {
        self.config.payee_column = Some(ColumnSpec::Name(name.into()));
        self
    }

    /// Set the payee column by index.
    pub fn payee_column_index(mut self, index: usize) -> Self {
        self.config.payee_column = Some(ColumnSpec::Index(index));
        self
    }

    /// Set the amount column by name.
    pub fn amount_column(mut self, name: impl Into<String>) -> Self {
        self.config.amount_column = Some(ColumnSpec::Name(name.into()));
        self
    }

    /// Set the amount column by index.
    pub fn amount_column_index(mut self, index: usize) -> Self {
        self.config.amount_column = Some(ColumnSpec::Index(index));
        self
    }

    /// Set separate debit column by name.
    pub fn debit_column(mut self, name: impl Into<String>) -> Self {
        self.config.debit_column = Some(ColumnSpec::Name(name.into()));
        self
    }

    /// Set separate credit column by name.
    pub fn credit_column(mut self, name: impl Into<String>) -> Self {
        self.config.credit_column = Some(ColumnSpec::Name(name.into()));
        self
    }

    /// Set whether the CSV has a header row.
    pub const fn has_header(mut self, has_header: bool) -> Self {
        self.config.has_header = has_header;
        self
    }

    /// Set the field delimiter.
    pub const fn delimiter(mut self, delimiter: char) -> Self {
        self.config.delimiter = delimiter;
        self
    }

    /// Set the number of rows to skip.
    pub const fn skip_rows(mut self, count: usize) -> Self {
        self.config.skip_rows = count;
        self
    }

    /// Set whether to invert the sign of amounts.
    pub const fn invert_sign(mut self, invert: bool) -> Self {
        self.config.invert_sign = invert;
        self
    }

    /// Build the importer configuration.
    pub fn build(self) -> Result<ImporterConfig> {
        Ok(ImporterConfig {
            account: self
                .account
                .unwrap_or_else(|| "Expenses:Unknown".to_string()),
            amount_format: match (&self.config.amount_format, &self.config.amount_locale) {
                (None, None) => AmountFormat::Symbols(NumberSymbols::monetary(Locale::POSIX)),
                (None, Some(locale)) => AmountFormat::Symbols(NumberSymbols::monetary(*locale)),
                (Some(fmt), None) => AmountFormat::Format(
                    NumberFormat::new(fmt).with_context(|| "invalid amount_format")?,
                ),
                (Some(fmt), Some(locale)) => AmountFormat::Format(
                    NumberFormat::news(fmt, NumberSymbols::monetary(*locale))
                        .with_context(|| "invalid number format")?,
                ),
            },
            currency: self.currency,
            importer_type: ImporterType::Csv(self.config),
        })
    }
}

impl Default for CsvConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== CsvConfig Default Tests ==========

    #[test]
    fn test_csv_config_default() {
        let config = CsvConfig::default();
        assert!(matches!(config.date_column, ColumnSpec::Name(ref s) if s == "Date"));
        assert_eq!(config.date_format, "%Y-%m-%d");
        assert!(config.narration_column.is_some());
        assert!(config.payee_column.is_none());
        assert!(config.amount_column.is_some());
        assert!(config.has_header);
        assert_eq!(config.delimiter, ',');
        assert_eq!(config.skip_rows, 0);
        assert!(!config.invert_sign);
    }

    // ========== CsvConfigBuilder Tests ==========

    #[test]
    fn test_csv_config_builder_new() {
        let builder = CsvConfigBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.currency.is_none());
    }

    #[test]
    fn test_csv_config_builder_default() {
        let builder = CsvConfigBuilder::default();
        assert!(builder.account.is_none());
    }

    #[test]
    fn test_csv_config_builder_account() {
        let config = CsvConfigBuilder::new()
            .account("Assets:Bank:Checking")
            .build()
            .unwrap();
        assert_eq!(config.account, "Assets:Bank:Checking");
    }

    #[test]
    fn test_csv_config_builder_default_account() {
        let config = CsvConfigBuilder::new().build().unwrap();
        assert_eq!(config.account, "Expenses:Unknown");
    }

    #[test]
    fn test_csv_config_builder_currency() {
        let config = CsvConfigBuilder::new().currency("EUR").build().unwrap();
        assert_eq!(config.currency, Some("EUR".to_string()));
    }

    #[test]
    fn test_csv_config_builder_date_column() {
        let config = CsvConfigBuilder::new()
            .date_column("TransactionDate")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(
            matches!(csv_config.date_column, ColumnSpec::Name(ref s) if s == "TransactionDate")
        );
    }

    #[test]
    fn test_csv_config_builder_date_column_index() {
        let config = CsvConfigBuilder::new()
            .date_column_index(0)
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(csv_config.date_column, ColumnSpec::Index(0)));
    }

    #[test]
    fn test_csv_config_builder_date_format() {
        let config = CsvConfigBuilder::new()
            .date_format("%m/%d/%Y")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(csv_config.date_format, "%m/%d/%Y");
    }

    #[test]
    fn test_csv_config_builder_narration_column() {
        let config = CsvConfigBuilder::new()
            .narration_column("Memo")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(
            matches!(csv_config.narration_column, Some(ColumnSpec::Name(ref s)) if s == "Memo")
        );
    }

    #[test]
    fn test_csv_config_builder_narration_column_index() {
        let config = CsvConfigBuilder::new()
            .narration_column_index(2)
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(
            csv_config.narration_column,
            Some(ColumnSpec::Index(2))
        ));
    }

    #[test]
    fn test_csv_config_builder_payee_column() {
        let config = CsvConfigBuilder::new()
            .payee_column("Merchant")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(
            matches!(csv_config.payee_column, Some(ColumnSpec::Name(ref s)) if s == "Merchant")
        );
    }

    #[test]
    fn test_csv_config_builder_payee_column_index() {
        let config = CsvConfigBuilder::new()
            .payee_column_index(3)
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(
            csv_config.payee_column,
            Some(ColumnSpec::Index(3))
        ));
    }

    #[test]
    fn test_csv_config_builder_amount_column() {
        let config = CsvConfigBuilder::new()
            .amount_column("Value")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(csv_config.amount_column, Some(ColumnSpec::Name(ref s)) if s == "Value"));
    }

    #[test]
    fn test_csv_config_builder_amount_column_index() {
        let config = CsvConfigBuilder::new()
            .amount_column_index(4)
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(
            csv_config.amount_column,
            Some(ColumnSpec::Index(4))
        ));
    }

    #[test]
    fn test_csv_config_builder_debit_credit_columns() {
        let config = CsvConfigBuilder::new()
            .debit_column("Debit")
            .credit_column("Credit")
            .build()
            .unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(csv_config.debit_column, Some(ColumnSpec::Name(ref s)) if s == "Debit"));
        assert!(matches!(csv_config.credit_column, Some(ColumnSpec::Name(ref s)) if s == "Credit"));
    }

    #[test]
    fn test_csv_config_builder_has_header() {
        let config = CsvConfigBuilder::new().has_header(false).build().unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(!csv_config.has_header);
    }

    #[test]
    fn test_csv_config_builder_delimiter() {
        let config = CsvConfigBuilder::new().delimiter(';').build().unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(csv_config.delimiter, ';');
    }

    #[test]
    fn test_csv_config_builder_skip_rows() {
        let config = CsvConfigBuilder::new().skip_rows(3).build().unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert_eq!(csv_config.skip_rows, 3);
    }

    #[test]
    fn test_csv_config_builder_invert_sign() {
        let config = CsvConfigBuilder::new().invert_sign(true).build().unwrap();
        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(csv_config.invert_sign);
    }

    #[test]
    fn test_csv_config_builder_full_chain() {
        let config = CsvConfigBuilder::new()
            .account("Assets:Bank:Checking")
            .currency("USD")
            .date_column("Date")
            .date_format("%Y/%m/%d")
            .narration_column("Description")
            .payee_column("Payee")
            .amount_column("Amount")
            .has_header(true)
            .delimiter(',')
            .skip_rows(1)
            .invert_sign(false)
            .build()
            .unwrap();

        assert_eq!(config.account, "Assets:Bank:Checking");
        assert_eq!(config.currency, Some("USD".to_string()));

        let ImporterType::Csv(csv_config) = &config.importer_type;
        assert!(matches!(csv_config.date_column, ColumnSpec::Name(ref s) if s == "Date"));
        assert_eq!(csv_config.date_format, "%Y/%m/%d");
        assert!(csv_config.narration_column.is_some());
        assert!(csv_config.payee_column.is_some());
        assert!(csv_config.amount_column.is_some());
        assert!(csv_config.has_header);
        assert_eq!(csv_config.delimiter, ',');
        assert_eq!(csv_config.skip_rows, 1);
        assert!(!csv_config.invert_sign);
    }

    // ========== ImporterConfig Tests ==========

    #[test]
    fn test_importer_config_csv() {
        let builder = ImporterConfig::csv();
        let config = builder.build().unwrap();
        assert!(matches!(config.importer_type, ImporterType::Csv(_)));
    }

    #[test]
    fn test_importer_config_extract_from_string() {
        let config = ImporterConfig::csv()
            .account("Assets:Bank")
            .currency("USD")
            .date_column("Date")
            .narration_column("Description")
            .amount_column("Amount")
            .build()
            .unwrap();

        let csv = "Date,Description,Amount\n2024-01-15,Test,-10.00\n";
        let result = config.extract_from_string(csv).unwrap();
        assert_eq!(result.directives.len(), 1);
    }

    // ========== ColumnSpec Tests ==========

    #[test]
    fn test_column_spec_name() {
        let spec = ColumnSpec::Name("Amount".to_string());
        assert!(matches!(spec, ColumnSpec::Name(ref s) if s == "Amount"));
    }

    #[test]
    fn test_column_spec_index() {
        let spec = ColumnSpec::Index(5);
        assert!(matches!(spec, ColumnSpec::Index(5)));
    }
}
