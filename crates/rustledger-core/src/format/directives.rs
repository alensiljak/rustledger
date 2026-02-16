//! Directive formatting for balance, open, close, etc.

use super::{escape_string, format_amount, format_meta_value};
use crate::{
    Balance, Close, Commodity, Custom, Document, Event, Metadata, Note, Open, Pad, Price, Query,
};
use std::fmt::Write;

/// Default indent for metadata (2 spaces).
const META_INDENT: &str = "  ";

/// Format metadata entries in deterministic (sorted) order.
fn format_metadata(meta: &Metadata, out: &mut String) {
    // Sort keys for deterministic output order
    let mut keys: Vec<_> = meta.keys().collect();
    keys.sort();

    for key in keys {
        let value = &meta[key];
        writeln!(out, "{META_INDENT}{}: {}", key, format_meta_value(value)).unwrap();
    }
}

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
    format_metadata(&bal.meta, &mut out);
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
    format_metadata(&open.meta, &mut out);
    out
}

/// Format a close directive.
pub fn format_close(close: &Close) -> String {
    let mut out = format!("{} close {}\n", close.date, close.account);
    format_metadata(&close.meta, &mut out);
    out
}

/// Format a commodity directive.
pub fn format_commodity(comm: &Commodity) -> String {
    let mut out = format!("{} commodity {}\n", comm.date, comm.currency);
    format_metadata(&comm.meta, &mut out);
    out
}

/// Format a pad directive.
pub fn format_pad(pad: &Pad) -> String {
    let mut out = format!("{} pad {} {}\n", pad.date, pad.account, pad.source_account);
    format_metadata(&pad.meta, &mut out);
    out
}

/// Format an event directive.
pub fn format_event(event: &Event) -> String {
    let mut out = format!(
        "{} event \"{}\" \"{}\"\n",
        event.date,
        escape_string(&event.event_type),
        escape_string(&event.value)
    );
    format_metadata(&event.meta, &mut out);
    out
}

/// Format a query directive.
pub fn format_query(query: &Query) -> String {
    let mut out = format!(
        "{} query \"{}\" \"{}\"\n",
        query.date,
        escape_string(&query.name),
        escape_string(&query.query)
    );
    format_metadata(&query.meta, &mut out);
    out
}

/// Format a note directive.
pub fn format_note(note: &Note) -> String {
    let mut out = format!(
        "{} note {} \"{}\"\n",
        note.date,
        note.account,
        escape_string(&note.comment)
    );
    format_metadata(&note.meta, &mut out);
    out
}

/// Format a document directive.
pub fn format_document(doc: &Document) -> String {
    let mut out = format!(
        "{} document {} \"{}\"\n",
        doc.date,
        doc.account,
        escape_string(&doc.path)
    );
    format_metadata(&doc.meta, &mut out);
    out
}

/// Format a price directive.
pub fn format_price(price: &Price) -> String {
    let mut out = format!(
        "{} price {} {}\n",
        price.date,
        price.currency,
        format_amount(&price.amount)
    );
    format_metadata(&price.meta, &mut out);
    out
}

/// Format a custom directive.
pub fn format_custom(custom: &Custom) -> String {
    let mut out = format!(
        "{} custom \"{}\"\n",
        custom.date,
        escape_string(&custom.custom_type)
    );
    format_metadata(&custom.meta, &mut out);
    out
}
