//! Directive formatting for balance, open, close, etc.

use super::{escape_string, format_amount};
use crate::{Balance, Close, Commodity, Custom, Document, Event, Note, Open, Pad, Price, Query};
use std::fmt::Write;

/// Format a balance directive.
pub fn format_balance(bal: &Balance) -> String {
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
pub fn format_open(open: &Open) -> String {
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
pub fn format_close(close: &Close) -> String {
    format!("{} close {}\n", close.date, close.account)
}

/// Format a commodity directive.
pub fn format_commodity(comm: &Commodity) -> String {
    format!("{} commodity {}\n", comm.date, comm.currency)
}

/// Format a pad directive.
pub fn format_pad(pad: &Pad) -> String {
    format!("{} pad {} {}\n", pad.date, pad.account, pad.source_account)
}

/// Format an event directive.
pub fn format_event(event: &Event) -> String {
    format!(
        "{} event \"{}\" \"{}\"\n",
        event.date,
        escape_string(&event.event_type),
        escape_string(&event.value)
    )
}

/// Format a query directive.
pub fn format_query(query: &Query) -> String {
    format!(
        "{} query \"{}\" \"{}\"\n",
        query.date,
        escape_string(&query.name),
        escape_string(&query.query)
    )
}

/// Format a note directive.
pub fn format_note(note: &Note) -> String {
    format!(
        "{} note {} \"{}\"\n",
        note.date,
        note.account,
        escape_string(&note.comment)
    )
}

/// Format a document directive.
pub fn format_document(doc: &Document) -> String {
    format!(
        "{} document {} \"{}\"\n",
        doc.date,
        doc.account,
        escape_string(&doc.path)
    )
}

/// Format a price directive.
pub fn format_price(price: &Price) -> String {
    format!(
        "{} price {} {}\n",
        price.date,
        price.currency,
        format_amount(&price.amount)
    )
}

/// Format a custom directive.
pub fn format_custom(custom: &Custom) -> String {
    format!(
        "{} custom \"{}\"\n",
        custom.date,
        escape_string(&custom.custom_type)
    )
}
