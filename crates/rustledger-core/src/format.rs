//! Beancount file formatter.
//!
//! Provides pretty-printing for beancount directives with configurable
//! amount alignment.

use crate::{
    Amount, Balance, Close, Commodity, CostSpec, Custom, Directive, Document, Event,
    IncompleteAmount, MetaValue, Note, Open, Pad, Posting, Price, PriceAnnotation, Query,
    Transaction,
};
use std::fmt::Write;

/// Formatter configuration.
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Column to align amounts to (default: 60).
    pub amount_column: usize,
    /// Indentation for postings.
    pub indent: String,
    /// Indentation for metadata.
    pub meta_indent: String,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            amount_column: 60,
            indent: "  ".to_string(),
            meta_indent: "    ".to_string(),
        }
    }
}

impl FormatConfig {
    /// Create a new config with the specified amount column.
    #[must_use]
    pub fn with_column(column: usize) -> Self {
        Self {
            amount_column: column,
            ..Default::default()
        }
    }

    /// Create a new config with the specified indent width.
    #[must_use]
    pub fn with_indent(indent_width: usize) -> Self {
        let indent = " ".repeat(indent_width);
        let meta_indent = " ".repeat(indent_width * 2);
        Self {
            indent,
            meta_indent,
            ..Default::default()
        }
    }

    /// Create a new config with both column and indent settings.
    #[must_use]
    pub fn new(column: usize, indent_width: usize) -> Self {
        let indent = " ".repeat(indent_width);
        let meta_indent = " ".repeat(indent_width * 2);
        Self {
            amount_column: column,
            indent,
            meta_indent,
        }
    }
}

/// Format a directive to a string.
pub fn format_directive(directive: &Directive, config: &FormatConfig) -> String {
    match directive {
        Directive::Transaction(txn) => format_transaction(txn, config),
        Directive::Balance(bal) => format_balance(bal),
        Directive::Open(open) => format_open(open),
        Directive::Close(close) => format_close(close),
        Directive::Commodity(comm) => format_commodity(comm),
        Directive::Pad(pad) => format_pad(pad),
        Directive::Event(event) => format_event(event),
        Directive::Query(query) => format_query(query),
        Directive::Note(note) => format_note(note),
        Directive::Document(doc) => format_document(doc),
        Directive::Price(price) => format_price(price),
        Directive::Custom(custom) => format_custom(custom),
    }
}

/// Format a transaction.
fn format_transaction(txn: &Transaction, config: &FormatConfig) -> String {
    let mut out = String::new();

    // Date and flag
    write!(out, "{} {}", txn.date, txn.flag).unwrap();

    // Payee and narration
    if let Some(payee) = &txn.payee {
        write!(out, " \"{}\"", escape_string(payee)).unwrap();
    }
    write!(out, " \"{}\"", escape_string(&txn.narration)).unwrap();

    // Tags
    for tag in &txn.tags {
        write!(out, " #{tag}").unwrap();
    }

    // Links
    for link in &txn.links {
        write!(out, " ^{link}").unwrap();
    }

    out.push('\n');

    // Transaction-level metadata
    for (key, value) in &txn.meta {
        writeln!(
            out,
            "{}{}: {}",
            &config.indent,
            key,
            format_meta_value(value)
        )
        .unwrap();
    }

    // Postings
    for posting in &txn.postings {
        out.push_str(&format_posting(posting, config));
        out.push('\n');
    }

    out
}

/// Format a posting with amount alignment.
fn format_posting(posting: &Posting, config: &FormatConfig) -> String {
    let mut line = String::new();
    line.push_str(&config.indent);

    // Flag (if present)
    if let Some(flag) = posting.flag {
        write!(line, "{flag} ").unwrap();
    }

    // Account
    line.push_str(&posting.account);

    // Units, cost, price
    if let Some(incomplete_amount) = &posting.units {
        // Calculate padding to align amount
        let current_len = line.len();
        let amount_str = format_incomplete_amount(incomplete_amount);
        let amount_with_extras =
            format_posting_incomplete_amount(incomplete_amount, &posting.cost, &posting.price);

        // Pad to align the number at the configured column
        let target_col = config.amount_column.saturating_sub(amount_str.len());
        if current_len < target_col {
            let padding = target_col - current_len;
            for _ in 0..padding {
                line.push(' ');
            }
        } else {
            line.push_str("  "); // Minimum 2 spaces
        }

        line.push_str(&amount_with_extras);
    }

    line
}

/// Format an incomplete amount.
fn format_incomplete_amount(amount: &IncompleteAmount) -> String {
    match amount {
        IncompleteAmount::Complete(a) => format!("{} {}", a.number, a.currency),
        IncompleteAmount::NumberOnly(n) => n.to_string(),
        IncompleteAmount::CurrencyOnly(c) => c.to_string(),
    }
}

/// Format the amount part of a posting with incomplete amount support.
fn format_posting_incomplete_amount(
    units: &IncompleteAmount,
    cost: &Option<CostSpec>,
    price: &Option<PriceAnnotation>,
) -> String {
    let mut out = format_incomplete_amount(units);

    // Cost spec
    if let Some(cost_spec) = cost {
        out.push(' ');
        out.push_str(&format_cost_spec(cost_spec));
    }

    // Price annotation
    if let Some(price_ann) = price {
        out.push(' ');
        out.push_str(&format_price_annotation(price_ann));
    }

    out
}

/// Format the amount part of a posting (units + cost + price).
#[allow(dead_code)]
fn format_posting_amount(
    units: &Amount,
    cost: &Option<CostSpec>,
    price: &Option<PriceAnnotation>,
) -> String {
    let mut out = format_amount(units);

    // Cost spec
    if let Some(cost_spec) = cost {
        out.push(' ');
        out.push_str(&format_cost_spec(cost_spec));
    }

    // Price annotation
    if let Some(price_ann) = price {
        out.push(' ');
        out.push_str(&format_price_annotation(price_ann));
    }

    out
}

/// Format an amount.
fn format_amount(amount: &Amount) -> String {
    format!("{} {}", amount.number, amount.currency)
}

/// Format a cost specification.
fn format_cost_spec(spec: &CostSpec) -> String {
    let mut parts = Vec::new();

    // Amount (per-unit or total)
    if let (Some(num), Some(curr)) = (&spec.number_per, &spec.currency) {
        parts.push(format!("{num} {curr}"));
    } else if let (Some(num), Some(curr)) = (&spec.number_total, &spec.currency) {
        // Total cost uses double braces
        return format!("{{{{{num} {curr}}}}}");
    }

    // Date
    if let Some(date) = spec.date {
        parts.push(date.to_string());
    }

    // Label
    if let Some(label) = &spec.label {
        parts.push(format!("\"{}\"", escape_string(label)));
    }

    // Merge marker
    if spec.merge {
        parts.push("*".to_string());
    }

    format!("{{{}}}", parts.join(", "))
}

/// Format a price annotation.
fn format_price_annotation(price: &PriceAnnotation) -> String {
    match price {
        PriceAnnotation::Unit(amount) => format!("@ {}", format_amount(amount)),
        PriceAnnotation::Total(amount) => format!("@@ {}", format_amount(amount)),
        PriceAnnotation::UnitIncomplete(inc) => format!("@ {}", format_incomplete_amount(inc)),
        PriceAnnotation::TotalIncomplete(inc) => format!("@@ {}", format_incomplete_amount(inc)),
        PriceAnnotation::UnitEmpty => "@".to_string(),
        PriceAnnotation::TotalEmpty => "@@".to_string(),
    }
}

/// Format a metadata value.
fn format_meta_value(value: &MetaValue) -> String {
    match value {
        MetaValue::String(s) => format!("\"{}\"", escape_string(s)),
        MetaValue::Account(a) => a.clone(),
        MetaValue::Currency(c) => c.clone(),
        MetaValue::Tag(t) => format!("#{t}"),
        MetaValue::Link(l) => format!("^{l}"),
        MetaValue::Date(d) => d.to_string(),
        MetaValue::Number(n) => n.to_string(),
        MetaValue::Amount(a) => format_amount(a),
        MetaValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        MetaValue::None => String::new(),
    }
}

/// Format a balance directive.
fn format_balance(bal: &Balance) -> String {
    let mut out = format!(
        "{} balance {} {}",
        bal.date,
        bal.account,
        format_amount(&bal.amount)
    );
    if let Some(tol) = &bal.tolerance {
        write!(out, " ~ {tol}").unwrap();
    }
    out.push('\n');
    out
}

/// Format an open directive.
fn format_open(open: &Open) -> String {
    let mut out = format!("{} open {}", open.date, open.account);
    if !open.currencies.is_empty() {
        write!(out, " {}", open.currencies.join(",")).unwrap();
    }
    if let Some(booking) = &open.booking {
        write!(out, " \"{booking}\"").unwrap();
    }
    out.push('\n');
    out
}

/// Format a close directive.
fn format_close(close: &Close) -> String {
    format!("{} close {}\n", close.date, close.account)
}

/// Format a commodity directive.
fn format_commodity(comm: &Commodity) -> String {
    format!("{} commodity {}\n", comm.date, comm.currency)
}

/// Format a pad directive.
fn format_pad(pad: &Pad) -> String {
    format!("{} pad {} {}\n", pad.date, pad.account, pad.source_account)
}

/// Format an event directive.
fn format_event(event: &Event) -> String {
    format!(
        "{} event \"{}\" \"{}\"\n",
        event.date,
        escape_string(&event.event_type),
        escape_string(&event.value)
    )
}

/// Format a query directive.
fn format_query(query: &Query) -> String {
    format!(
        "{} query \"{}\" \"{}\"\n",
        query.date,
        escape_string(&query.name),
        escape_string(&query.query)
    )
}

/// Format a note directive.
fn format_note(note: &Note) -> String {
    format!(
        "{} note {} \"{}\"\n",
        note.date,
        note.account,
        escape_string(&note.comment)
    )
}

/// Format a document directive.
fn format_document(doc: &Document) -> String {
    format!(
        "{} document {} \"{}\"\n",
        doc.date,
        doc.account,
        escape_string(&doc.path)
    )
}

/// Format a price directive.
fn format_price(price: &Price) -> String {
    format!(
        "{} price {} {}\n",
        price.date,
        price.currency,
        format_amount(&price.amount)
    )
}

/// Format a custom directive.
fn format_custom(custom: &Custom) -> String {
    format!(
        "{} custom \"{}\"\n",
        custom.date,
        escape_string(&custom.custom_type)
    )
}

/// Escape a string for output (handle quotes and backslashes).
fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NaiveDate;
    use rust_decimal_macros::dec;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn test_format_simple_transaction() {
        let txn = Transaction::new(date(2024, 1, 15), "Morning coffee")
            .with_flag('*')
            .with_payee("Coffee Shop")
            .with_posting(Posting::new(
                "Expenses:Food:Coffee",
                Amount::new(dec!(5.00), "USD"),
            ))
            .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-5.00), "USD")));

        let config = FormatConfig::with_column(50);
        let formatted = format_transaction(&txn, &config);

        assert!(formatted.contains("2024-01-15 * \"Coffee Shop\" \"Morning coffee\""));
        assert!(formatted.contains("Expenses:Food:Coffee"));
        assert!(formatted.contains("5.00 USD"));
    }

    #[test]
    fn test_format_balance() {
        let bal = Balance::new(
            date(2024, 1, 1),
            "Assets:Bank",
            Amount::new(dec!(1000.00), "USD"),
        );
        let formatted = format_balance(&bal);
        assert_eq!(formatted, "2024-01-01 balance Assets:Bank 1000.00 USD\n");
    }

    #[test]
    fn test_format_open() {
        let open = Open {
            date: date(2024, 1, 1),
            account: "Assets:Bank:Checking".into(),
            currencies: vec!["USD".into(), "EUR".into()],
            booking: None,
            meta: Default::default(),
        };
        let formatted = format_open(&open);
        assert_eq!(formatted, "2024-01-01 open Assets:Bank:Checking USD,EUR\n");
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_string("line1\nline2"), "line1\\nline2");
    }

    // ====================================================================
    // Phase 2: Additional Coverage Tests for Format Functions
    // ====================================================================

    #[test]
    fn test_escape_string_combined() {
        // Test escaping with quotes + backslash + newline combined
        assert_eq!(
            escape_string("path\\to\\file\n\"quoted\""),
            "path\\\\to\\\\file\\n\\\"quoted\\\""
        );
    }

    #[test]
    fn test_escape_string_backslash_quote() {
        // Backslash followed by quote
        assert_eq!(escape_string("\\\""), "\\\\\\\"");
    }

    #[test]
    fn test_escape_string_empty() {
        assert_eq!(escape_string(""), "");
    }

    #[test]
    fn test_escape_string_unicode() {
        assert_eq!(escape_string("café résumé"), "café résumé");
        assert_eq!(escape_string("日本語"), "日本語");
        assert_eq!(escape_string("emoji 🎉"), "emoji 🎉");
    }

    #[test]
    fn test_format_meta_value_string() {
        let val = MetaValue::String("hello world".to_string());
        assert_eq!(format_meta_value(&val), "\"hello world\"");
    }

    #[test]
    fn test_format_meta_value_string_with_quotes() {
        let val = MetaValue::String("say \"hello\"".to_string());
        assert_eq!(format_meta_value(&val), "\"say \\\"hello\\\"\"");
    }

    #[test]
    fn test_format_meta_value_account() {
        let val = MetaValue::Account("Assets:Bank:Checking".to_string());
        assert_eq!(format_meta_value(&val), "Assets:Bank:Checking");
    }

    #[test]
    fn test_format_meta_value_currency() {
        let val = MetaValue::Currency("USD".to_string());
        assert_eq!(format_meta_value(&val), "USD");
    }

    #[test]
    fn test_format_meta_value_tag() {
        let val = MetaValue::Tag("trip-2024".to_string());
        assert_eq!(format_meta_value(&val), "#trip-2024");
    }

    #[test]
    fn test_format_meta_value_link() {
        let val = MetaValue::Link("invoice-123".to_string());
        assert_eq!(format_meta_value(&val), "^invoice-123");
    }

    #[test]
    fn test_format_meta_value_date() {
        let val = MetaValue::Date(date(2024, 6, 15));
        assert_eq!(format_meta_value(&val), "2024-06-15");
    }

    #[test]
    fn test_format_meta_value_number() {
        let val = MetaValue::Number(dec!(123.456));
        assert_eq!(format_meta_value(&val), "123.456");
    }

    #[test]
    fn test_format_meta_value_amount() {
        let val = MetaValue::Amount(Amount::new(dec!(99.99), "USD"));
        assert_eq!(format_meta_value(&val), "99.99 USD");
    }

    #[test]
    fn test_format_meta_value_bool_true() {
        let val = MetaValue::Bool(true);
        assert_eq!(format_meta_value(&val), "TRUE");
    }

    #[test]
    fn test_format_meta_value_bool_false() {
        let val = MetaValue::Bool(false);
        assert_eq!(format_meta_value(&val), "FALSE");
    }

    #[test]
    fn test_format_meta_value_none() {
        let val = MetaValue::None;
        assert_eq!(format_meta_value(&val), "");
    }

    #[test]
    fn test_format_cost_spec_per_unit() {
        let spec = CostSpec {
            number_per: Some(dec!(150.00)),
            number_total: None,
            currency: Some("USD".into()),
            date: None,
            label: None,
            merge: false,
        };
        assert_eq!(format_cost_spec(&spec), "{150.00 USD}");
    }

    #[test]
    fn test_format_cost_spec_total() {
        let spec = CostSpec {
            number_per: None,
            number_total: Some(dec!(1500.00)),
            currency: Some("USD".into()),
            date: None,
            label: None,
            merge: false,
        };
        assert_eq!(format_cost_spec(&spec), "{{1500.00 USD}}");
    }

    #[test]
    fn test_format_cost_spec_with_date() {
        let spec = CostSpec {
            number_per: Some(dec!(150.00)),
            number_total: None,
            currency: Some("USD".into()),
            date: Some(date(2024, 1, 15)),
            label: None,
            merge: false,
        };
        assert_eq!(format_cost_spec(&spec), "{150.00 USD, 2024-01-15}");
    }

    #[test]
    fn test_format_cost_spec_with_label() {
        let spec = CostSpec {
            number_per: Some(dec!(150.00)),
            number_total: None,
            currency: Some("USD".into()),
            date: None,
            label: Some("lot-a".to_string()),
            merge: false,
        };
        assert_eq!(format_cost_spec(&spec), "{150.00 USD, \"lot-a\"}");
    }

    #[test]
    fn test_format_cost_spec_with_merge() {
        let spec = CostSpec {
            number_per: Some(dec!(150.00)),
            number_total: None,
            currency: Some("USD".into()),
            date: None,
            label: None,
            merge: true,
        };
        assert_eq!(format_cost_spec(&spec), "{150.00 USD, *}");
    }

    #[test]
    fn test_format_cost_spec_all_fields() {
        let spec = CostSpec {
            number_per: Some(dec!(150.00)),
            number_total: None,
            currency: Some("USD".into()),
            date: Some(date(2024, 1, 15)),
            label: Some("lot-a".to_string()),
            merge: true,
        };
        assert_eq!(
            format_cost_spec(&spec),
            "{150.00 USD, 2024-01-15, \"lot-a\", *}"
        );
    }

    #[test]
    fn test_format_cost_spec_empty() {
        let spec = CostSpec {
            number_per: None,
            number_total: None,
            currency: None,
            date: None,
            label: None,
            merge: false,
        };
        assert_eq!(format_cost_spec(&spec), "{}");
    }

    #[test]
    fn test_format_price_annotation_unit() {
        let price = PriceAnnotation::Unit(Amount::new(dec!(150.00), "USD"));
        assert_eq!(format_price_annotation(&price), "@ 150.00 USD");
    }

    #[test]
    fn test_format_price_annotation_total() {
        let price = PriceAnnotation::Total(Amount::new(dec!(1500.00), "USD"));
        assert_eq!(format_price_annotation(&price), "@@ 1500.00 USD");
    }

    #[test]
    fn test_format_price_annotation_unit_incomplete() {
        let price = PriceAnnotation::UnitIncomplete(IncompleteAmount::NumberOnly(dec!(150.00)));
        assert_eq!(format_price_annotation(&price), "@ 150.00");
    }

    #[test]
    fn test_format_price_annotation_total_incomplete() {
        let price = PriceAnnotation::TotalIncomplete(IncompleteAmount::CurrencyOnly("USD".into()));
        assert_eq!(format_price_annotation(&price), "@@ USD");
    }

    #[test]
    fn test_format_price_annotation_unit_empty() {
        let price = PriceAnnotation::UnitEmpty;
        assert_eq!(format_price_annotation(&price), "@");
    }

    #[test]
    fn test_format_price_annotation_total_empty() {
        let price = PriceAnnotation::TotalEmpty;
        assert_eq!(format_price_annotation(&price), "@@");
    }

    #[test]
    fn test_format_incomplete_amount_complete() {
        let amount = IncompleteAmount::Complete(Amount::new(dec!(100.50), "EUR"));
        assert_eq!(format_incomplete_amount(&amount), "100.50 EUR");
    }

    #[test]
    fn test_format_incomplete_amount_number_only() {
        let amount = IncompleteAmount::NumberOnly(dec!(42.00));
        assert_eq!(format_incomplete_amount(&amount), "42.00");
    }

    #[test]
    fn test_format_incomplete_amount_currency_only() {
        let amount = IncompleteAmount::CurrencyOnly("BTC".into());
        assert_eq!(format_incomplete_amount(&amount), "BTC");
    }

    #[test]
    fn test_format_close() {
        let close = Close {
            date: date(2024, 12, 31),
            account: "Assets:OldAccount".into(),
            meta: Default::default(),
        };
        let formatted = format_close(&close);
        assert_eq!(formatted, "2024-12-31 close Assets:OldAccount\n");
    }

    #[test]
    fn test_format_commodity() {
        let comm = Commodity {
            date: date(2024, 1, 1),
            currency: "BTC".into(),
            meta: Default::default(),
        };
        let formatted = format_commodity(&comm);
        assert_eq!(formatted, "2024-01-01 commodity BTC\n");
    }

    #[test]
    fn test_format_pad() {
        let pad = Pad {
            date: date(2024, 1, 15),
            account: "Assets:Checking".into(),
            source_account: "Equity:Opening-Balances".into(),
            meta: Default::default(),
        };
        let formatted = format_pad(&pad);
        assert_eq!(
            formatted,
            "2024-01-15 pad Assets:Checking Equity:Opening-Balances\n"
        );
    }

    #[test]
    fn test_format_event() {
        let event = Event {
            date: date(2024, 6, 1),
            event_type: "location".to_string(),
            value: "New York".to_string(),
            meta: Default::default(),
        };
        let formatted = format_event(&event);
        assert_eq!(formatted, "2024-06-01 event \"location\" \"New York\"\n");
    }

    #[test]
    fn test_format_event_with_quotes() {
        let event = Event {
            date: date(2024, 6, 1),
            event_type: "quote".to_string(),
            value: "He said \"hello\"".to_string(),
            meta: Default::default(),
        };
        let formatted = format_event(&event);
        assert_eq!(
            formatted,
            "2024-06-01 event \"quote\" \"He said \\\"hello\\\"\"\n"
        );
    }

    #[test]
    fn test_format_query() {
        let query = Query {
            date: date(2024, 1, 1),
            name: "monthly_expenses".to_string(),
            query: "SELECT account, sum(position) WHERE account ~ 'Expenses'".to_string(),
            meta: Default::default(),
        };
        let formatted = format_query(&query);
        assert!(formatted.contains("query \"monthly_expenses\""));
        assert!(formatted.contains("SELECT account"));
    }

    #[test]
    fn test_format_note() {
        let note = Note {
            date: date(2024, 3, 15),
            account: "Assets:Bank".into(),
            comment: "Called the bank about fee".to_string(),
            meta: Default::default(),
        };
        let formatted = format_note(&note);
        assert_eq!(
            formatted,
            "2024-03-15 note Assets:Bank \"Called the bank about fee\"\n"
        );
    }

    #[test]
    fn test_format_document() {
        let doc = Document {
            date: date(2024, 2, 10),
            account: "Assets:Bank".into(),
            path: "/docs/statement-2024-02.pdf".to_string(),
            tags: vec![],
            links: vec![],
            meta: Default::default(),
        };
        let formatted = format_document(&doc);
        assert_eq!(
            formatted,
            "2024-02-10 document Assets:Bank \"/docs/statement-2024-02.pdf\"\n"
        );
    }

    #[test]
    fn test_format_price() {
        let price = Price {
            date: date(2024, 1, 15),
            currency: "AAPL".into(),
            amount: Amount::new(dec!(185.50), "USD"),
            meta: Default::default(),
        };
        let formatted = format_price(&price);
        assert_eq!(formatted, "2024-01-15 price AAPL 185.50 USD\n");
    }

    #[test]
    fn test_format_custom() {
        let custom = Custom {
            date: date(2024, 1, 1),
            custom_type: "budget".to_string(),
            values: vec![],
            meta: Default::default(),
        };
        let formatted = format_custom(&custom);
        assert_eq!(formatted, "2024-01-01 custom \"budget\"\n");
    }

    #[test]
    fn test_format_open_with_booking() {
        let open = Open {
            date: date(2024, 1, 1),
            account: "Assets:Brokerage".into(),
            currencies: vec!["USD".into()],
            booking: Some("FIFO".to_string()),
            meta: Default::default(),
        };
        let formatted = format_open(&open);
        assert_eq!(formatted, "2024-01-01 open Assets:Brokerage USD \"FIFO\"\n");
    }

    #[test]
    fn test_format_open_no_currencies() {
        let open = Open {
            date: date(2024, 1, 1),
            account: "Assets:Misc".into(),
            currencies: vec![],
            booking: None,
            meta: Default::default(),
        };
        let formatted = format_open(&open);
        assert_eq!(formatted, "2024-01-01 open Assets:Misc\n");
    }

    #[test]
    fn test_format_balance_with_tolerance() {
        let bal = Balance {
            date: date(2024, 1, 1),
            account: "Assets:Bank".into(),
            amount: Amount::new(dec!(1000.00), "USD"),
            tolerance: Some(dec!(0.01)),
            meta: Default::default(),
        };
        let formatted = format_balance(&bal);
        assert_eq!(
            formatted,
            "2024-01-01 balance Assets:Bank 1000.00 USD ~ 0.01\n"
        );
    }

    #[test]
    fn test_format_transaction_with_tags() {
        let txn = Transaction::new(date(2024, 1, 15), "Dinner")
            .with_flag('*')
            .with_tag("trip-2024")
            .with_tag("food")
            .with_posting(Posting::new(
                "Expenses:Food",
                Amount::new(dec!(50.00), "USD"),
            ))
            .with_posting(Posting::new(
                "Assets:Cash",
                Amount::new(dec!(-50.00), "USD"),
            ));

        let config = FormatConfig::default();
        let formatted = format_transaction(&txn, &config);

        assert!(formatted.contains("#trip-2024"));
        assert!(formatted.contains("#food"));
    }

    #[test]
    fn test_format_transaction_with_links() {
        let txn = Transaction::new(date(2024, 1, 15), "Invoice payment")
            .with_flag('*')
            .with_link("invoice-123")
            .with_posting(Posting::new(
                "Income:Freelance",
                Amount::new(dec!(-1000.00), "USD"),
            ))
            .with_posting(Posting::new(
                "Assets:Bank",
                Amount::new(dec!(1000.00), "USD"),
            ));

        let config = FormatConfig::default();
        let formatted = format_transaction(&txn, &config);

        assert!(formatted.contains("^invoice-123"));
    }

    #[test]
    fn test_format_transaction_with_metadata() {
        let mut meta = std::collections::HashMap::new();
        meta.insert(
            "filename".to_string(),
            MetaValue::String("receipt.pdf".to_string()),
        );
        meta.insert("verified".to_string(), MetaValue::Bool(true));

        let txn = Transaction {
            date: date(2024, 1, 15),
            flag: '*',
            payee: None,
            narration: "Purchase".into(),
            tags: vec![],
            links: vec![],
            postings: vec![],
            meta,
        };

        let config = FormatConfig::default();
        let formatted = format_transaction(&txn, &config);

        assert!(formatted.contains("filename: \"receipt.pdf\""));
        assert!(formatted.contains("verified: TRUE"));
    }

    #[test]
    fn test_format_posting_with_flag() {
        let mut posting = Posting::new("Expenses:Unknown", Amount::new(dec!(100.00), "USD"));
        posting.flag = Some('!');

        let config = FormatConfig::default();
        let formatted = format_posting(&posting, &config);

        assert!(formatted.contains("! Expenses:Unknown"));
    }

    #[test]
    fn test_format_posting_no_units() {
        let posting = Posting {
            flag: None,
            account: "Assets:Bank".into(),
            units: None,
            cost: None,
            price: None,
            meta: Default::default(),
        };

        let config = FormatConfig::default();
        let formatted = format_posting(&posting, &config);

        assert!(formatted.contains("Assets:Bank"));
        // No amount should appear
        assert!(!formatted.contains("USD"));
    }

    #[test]
    fn test_format_config_with_column() {
        let config = FormatConfig::with_column(80);
        assert_eq!(config.amount_column, 80);
        assert_eq!(config.indent, "  ");
    }

    #[test]
    fn test_format_config_with_indent() {
        let config = FormatConfig::with_indent(4);
        assert_eq!(config.indent, "    ");
        assert_eq!(config.meta_indent, "        ");
    }

    #[test]
    fn test_format_config_new() {
        let config = FormatConfig::new(70, 3);
        assert_eq!(config.amount_column, 70);
        assert_eq!(config.indent, "   ");
        assert_eq!(config.meta_indent, "      ");
    }

    #[test]
    fn test_format_posting_long_account_name() {
        let posting = Posting::new(
            "Assets:Bank:Checking:Primary:Joint:Savings:Emergency:Fund:Extra:Long",
            Amount::new(dec!(100.00), "USD"),
        );

        let config = FormatConfig::with_column(50);
        let formatted = format_posting(&posting, &config);

        // Should have at least 2 spaces between account and amount
        assert!(formatted.contains("  100.00 USD"));
    }

    #[test]
    fn test_format_posting_with_cost_and_price() {
        let posting = Posting {
            flag: None,
            account: "Assets:Brokerage".into(),
            units: Some(IncompleteAmount::Complete(Amount::new(dec!(10), "AAPL"))),
            cost: Some(CostSpec {
                number_per: Some(dec!(150.00)),
                number_total: None,
                currency: Some("USD".into()),
                date: Some(date(2024, 1, 15)),
                label: None,
                merge: false,
            }),
            price: Some(PriceAnnotation::Unit(Amount::new(dec!(155.00), "USD"))),
            meta: Default::default(),
        };

        let config = FormatConfig::default();
        let formatted = format_posting(&posting, &config);

        assert!(formatted.contains("10 AAPL"));
        assert!(formatted.contains("{150.00 USD, 2024-01-15}"));
        assert!(formatted.contains("@ 155.00 USD"));
    }

    #[test]
    fn test_format_directive_all_types() {
        let config = FormatConfig::default();

        // Transaction
        let txn = Transaction::new(date(2024, 1, 1), "Test")
            .with_flag('*')
            .with_posting(Posting::new("Expenses:Test", Amount::new(dec!(1), "USD")))
            .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-1), "USD")));
        let formatted = format_directive(&Directive::Transaction(txn), &config);
        assert!(formatted.contains("2024-01-01"));

        // Balance
        let bal = Balance::new(
            date(2024, 1, 1),
            "Assets:Bank",
            Amount::new(dec!(100), "USD"),
        );
        let formatted = format_directive(&Directive::Balance(bal), &config);
        assert!(formatted.contains("balance"));

        // Open
        let open = Open {
            date: date(2024, 1, 1),
            account: "Assets:Test".into(),
            currencies: vec![],
            booking: None,
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Open(open), &config);
        assert!(formatted.contains("open"));

        // Close
        let close = Close {
            date: date(2024, 1, 1),
            account: "Assets:Test".into(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Close(close), &config);
        assert!(formatted.contains("close"));

        // Commodity
        let comm = Commodity {
            date: date(2024, 1, 1),
            currency: "BTC".into(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Commodity(comm), &config);
        assert!(formatted.contains("commodity"));

        // Pad
        let pad = Pad {
            date: date(2024, 1, 1),
            account: "Assets:A".into(),
            source_account: "Equity:B".into(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Pad(pad), &config);
        assert!(formatted.contains("pad"));

        // Event
        let event = Event {
            date: date(2024, 1, 1),
            event_type: "test".to_string(),
            value: "value".to_string(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Event(event), &config);
        assert!(formatted.contains("event"));

        // Query
        let query = Query {
            date: date(2024, 1, 1),
            name: "test".to_string(),
            query: "SELECT *".to_string(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Query(query), &config);
        assert!(formatted.contains("query"));

        // Note
        let note = Note {
            date: date(2024, 1, 1),
            account: "Assets:Bank".into(),
            comment: "test".to_string(),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Note(note), &config);
        assert!(formatted.contains("note"));

        // Document
        let doc = Document {
            date: date(2024, 1, 1),
            account: "Assets:Bank".into(),
            path: "/path".to_string(),
            tags: vec![],
            links: vec![],
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Document(doc), &config);
        assert!(formatted.contains("document"));

        // Price
        let price = Price {
            date: date(2024, 1, 1),
            currency: "AAPL".into(),
            amount: Amount::new(dec!(150), "USD"),
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Price(price), &config);
        assert!(formatted.contains("price"));

        // Custom
        let custom = Custom {
            date: date(2024, 1, 1),
            custom_type: "test".to_string(),
            values: vec![],
            meta: Default::default(),
        };
        let formatted = format_directive(&Directive::Custom(custom), &config);
        assert!(formatted.contains("custom"));
    }

    #[test]
    fn test_format_amount_negative() {
        let amount = Amount::new(dec!(-100.50), "USD");
        assert_eq!(format_amount(&amount), "-100.50 USD");
    }

    #[test]
    fn test_format_amount_zero() {
        let amount = Amount::new(dec!(0), "EUR");
        assert_eq!(format_amount(&amount), "0 EUR");
    }

    #[test]
    fn test_format_amount_large_number() {
        let amount = Amount::new(dec!(1234567890.12), "USD");
        assert_eq!(format_amount(&amount), "1234567890.12 USD");
    }

    #[test]
    fn test_format_amount_small_decimal() {
        let amount = Amount::new(dec!(0.00001), "BTC");
        assert_eq!(format_amount(&amount), "0.00001 BTC");
    }
}
