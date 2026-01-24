use anyhow::{Context, Result};
use rustledger_loader::Loader;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn cmd_list_options<W: Write>(writer: &mut W) -> Result<()> {
    writeln!(writer, "Available beancount options:")?;
    writeln!(writer, "{}", "=".repeat(60))?;
    writeln!(writer)?;

    let options = [
        ("title", "string", "The title of the ledger"),
        (
            "operating_currency",
            "string",
            "Operating currencies (can be specified multiple times)",
        ),
        ("render_commas", "bool", "Render commas in numbers"),
        (
            "name_assets",
            "string",
            "Name for Assets accounts (default: Assets)",
        ),
        (
            "name_liabilities",
            "string",
            "Name for Liabilities accounts (default: Liabilities)",
        ),
        (
            "name_equity",
            "string",
            "Name for Equity accounts (default: Equity)",
        ),
        (
            "name_income",
            "string",
            "Name for Income accounts (default: Income)",
        ),
        (
            "name_expenses",
            "string",
            "Name for Expenses accounts (default: Expenses)",
        ),
        (
            "account_previous_balances",
            "string",
            "Account for opening balances",
        ),
        (
            "account_previous_earnings",
            "string",
            "Account for previous earnings",
        ),
        (
            "account_previous_conversions",
            "string",
            "Account for previous conversions",
        ),
        (
            "account_current_earnings",
            "string",
            "Account for current earnings",
        ),
        (
            "account_current_conversions",
            "string",
            "Account for current conversions",
        ),
        (
            "account_unrealized_gains",
            "string",
            "Account for unrealized gains",
        ),
        ("account_rounding", "string", "Account for rounding errors"),
        ("conversion_currency", "string", "Currency for conversions"),
        (
            "inferred_tolerance_default",
            "string",
            "Default tolerance for balance checks",
        ),
        (
            "inferred_tolerance_multiplier",
            "decimal",
            "Multiplier for inferred tolerances",
        ),
        (
            "infer_tolerance_from_cost",
            "bool",
            "Infer tolerance from cost",
        ),
        ("documents", "string", "Directories to search for documents"),
        (
            "booking_method",
            "string",
            "Default booking method (STRICT, FIFO, LIFO, etc.)",
        ),
        ("plugin_processing_mode", "string", "Plugin processing mode"),
        (
            "long_string_maxlines",
            "int",
            "Maximum lines for long strings",
        ),
        ("insert_pythonpath", "string", "Python paths for plugins"),
    ];

    for (name, type_name, description) in options {
        writeln!(writer, "option \"{name}\" <{type_name}>")?;
        writeln!(writer, "  {description}")?;
        writeln!(writer)?;
    }

    Ok(())
}

pub(super) fn cmd_print_options<W: Write>(file: &PathBuf, writer: &mut W) -> Result<()> {
    let mut loader = Loader::new();
    let load_result = loader
        .load(file)
        .with_context(|| format!("failed to load {}", file.display()))?;

    writeln!(writer, "Options from {}:", file.display())?;
    writeln!(writer, "{}", "=".repeat(60))?;
    writeln!(writer)?;

    let options = &load_result.options;

    if let Some(title) = &options.title {
        writeln!(writer, "title: {title:?}")?;
    }
    if !options.operating_currency.is_empty() {
        writeln!(
            writer,
            "operating_currency: {:?}",
            options.operating_currency
        )?;
    }
    writeln!(writer, "name_assets: {:?}", options.name_assets)?;
    writeln!(writer, "name_liabilities: {:?}", options.name_liabilities)?;
    writeln!(writer, "name_equity: {:?}", options.name_equity)?;
    writeln!(writer, "name_income: {:?}", options.name_income)?;
    writeln!(writer, "name_expenses: {:?}", options.name_expenses)?;

    writeln!(
        writer,
        "account_previous_balances: {:?}",
        options.account_previous_balances
    )?;
    writeln!(
        writer,
        "account_previous_earnings: {:?}",
        options.account_previous_earnings
    )?;
    writeln!(
        writer,
        "account_current_earnings: {:?}",
        options.account_current_earnings
    )?;
    if let Some(acct) = &options.account_unrealized_gains {
        writeln!(writer, "account_unrealized_gains: {acct:?}")?;
    }

    writeln!(writer, "booking_method: {:?}", options.booking_method)?;

    if !options.documents.is_empty() {
        writeln!(writer, "documents: {:?}", options.documents)?;
    }

    Ok(())
}
