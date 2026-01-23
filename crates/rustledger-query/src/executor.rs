//! BQL Query Executor.
//!
//! Executes parsed BQL queries against a set of Beancount directives.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use chrono::Datelike;
use regex::Regex;
use rust_decimal::Decimal;
use rustledger_core::{
    Amount, Directive, InternedStr, Inventory, MetaValue, Metadata, NaiveDate, Position,
    Transaction,
};
use rustledger_loader::SourceMap;
use rustledger_parser::Spanned;

use crate::ast::{
    BalancesQuery, BinaryOp, BinaryOperator, CreateTableStmt, Expr, FromClause, FunctionCall,
    InsertSource, InsertStmt, JournalQuery, Literal, OrderSpec, PrintQuery, Query, SelectQuery,
    SortDirection, Target, UnaryOp, UnaryOperator, WindowFunction,
};
use crate::error::QueryError;

/// Source location information for a directive.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// File path.
    pub filename: String,
    /// Line number (1-based).
    pub lineno: usize,
}

/// An interval unit for date arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntervalUnit {
    /// Days.
    Day,
    /// Weeks.
    Week,
    /// Months.
    Month,
    /// Quarters.
    Quarter,
    /// Years.
    Year,
}

impl IntervalUnit {
    /// Parse an interval unit from a string.
    pub fn parse_unit(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "DAY" | "DAYS" | "D" => Some(Self::Day),
            "WEEK" | "WEEKS" | "W" => Some(Self::Week),
            "MONTH" | "MONTHS" | "M" => Some(Self::Month),
            "QUARTER" | "QUARTERS" | "Q" => Some(Self::Quarter),
            "YEAR" | "YEARS" | "Y" => Some(Self::Year),
            _ => None,
        }
    }
}

/// An interval value for date arithmetic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Interval {
    /// The count (can be negative).
    pub count: i64,
    /// The unit.
    pub unit: IntervalUnit,
}

impl Interval {
    /// Create a new interval.
    pub const fn new(count: i64, unit: IntervalUnit) -> Self {
        Self { count, unit }
    }

    /// Convert interval to an approximate number of days for comparison.
    /// Uses: Day=1, Week=7, Month=30, Quarter=91, Year=365.
    const fn to_approx_days(&self) -> i64 {
        let days_per_unit = match self.unit {
            IntervalUnit::Day => 1,
            IntervalUnit::Week => 7,
            IntervalUnit::Month => 30,
            IntervalUnit::Quarter => 91,
            IntervalUnit::Year => 365,
        };
        self.count.saturating_mul(days_per_unit)
    }

    /// Add this interval to a date.
    #[allow(clippy::missing_const_for_fn)] // chrono methods aren't const
    pub fn add_to_date(&self, date: NaiveDate) -> Option<NaiveDate> {
        use chrono::Months;

        match self.unit {
            IntervalUnit::Day => date.checked_add_signed(chrono::Duration::days(self.count)),
            IntervalUnit::Week => date.checked_add_signed(chrono::Duration::weeks(self.count)),
            IntervalUnit::Month => {
                if self.count >= 0 {
                    date.checked_add_months(Months::new(self.count as u32))
                } else {
                    date.checked_sub_months(Months::new((-self.count) as u32))
                }
            }
            IntervalUnit::Quarter => {
                let months = self.count * 3;
                if months >= 0 {
                    date.checked_add_months(Months::new(months as u32))
                } else {
                    date.checked_sub_months(Months::new((-months) as u32))
                }
            }
            IntervalUnit::Year => {
                let months = self.count * 12;
                if months >= 0 {
                    date.checked_add_months(Months::new(months as u32))
                } else {
                    date.checked_sub_months(Months::new((-months) as u32))
                }
            }
        }
    }
}

/// A value that can result from evaluating a BQL expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// String value.
    String(String),
    /// Numeric value.
    Number(Decimal),
    /// Integer value.
    Integer(i64),
    /// Date value.
    Date(NaiveDate),
    /// Boolean value.
    Boolean(bool),
    /// Amount (number + currency).
    Amount(Amount),
    /// Position (amount + optional cost).
    Position(Position),
    /// Inventory (aggregated positions).
    Inventory(Inventory),
    /// Set of strings (tags, links).
    StringSet(Vec<String>),
    /// Metadata dictionary.
    Metadata(Metadata),
    /// Interval for date arithmetic.
    Interval(Interval),
    /// Structured object (for entry, meta columns).
    Object(BTreeMap<String, Self>),
    /// NULL value.
    Null,
}

impl Value {
    /// Compute a hash for this value.
    ///
    /// Note: This is not the standard Hash trait because some contained types
    /// (Decimal, Inventory) don't implement Hash. We use byte representations
    /// for those types.
    fn hash_value<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::String(s) => s.hash(state),
            Self::Number(d) => d.serialize().hash(state),
            Self::Integer(i) => i.hash(state),
            Self::Date(d) => {
                d.year().hash(state);
                d.month().hash(state);
                d.day().hash(state);
            }
            Self::Boolean(b) => b.hash(state),
            Self::Amount(a) => {
                a.number.serialize().hash(state);
                a.currency.as_str().hash(state);
            }
            Self::Position(p) => {
                p.units.number.serialize().hash(state);
                p.units.currency.as_str().hash(state);
                if let Some(cost) = &p.cost {
                    cost.number.serialize().hash(state);
                    cost.currency.as_str().hash(state);
                }
            }
            Self::Inventory(inv) => {
                for pos in inv.positions() {
                    pos.units.number.serialize().hash(state);
                    pos.units.currency.as_str().hash(state);
                    if let Some(cost) = &pos.cost {
                        cost.number.serialize().hash(state);
                        cost.currency.as_str().hash(state);
                    }
                }
            }
            Self::StringSet(ss) => {
                // Hash StringSet in a canonical, order-independent way by sorting first.
                let mut sorted = ss.clone();
                sorted.sort();
                for s in &sorted {
                    s.hash(state);
                }
            }
            Self::Metadata(meta) => {
                // Hash metadata in canonical order by sorting keys
                let mut keys: Vec<_> = meta.keys().collect();
                keys.sort();
                for key in keys {
                    key.hash(state);
                    // Hash the debug representation of the value
                    format!("{:?}", meta.get(key)).hash(state);
                }
            }
            Self::Interval(interval) => {
                interval.count.hash(state);
                interval.unit.hash(state);
            }
            Self::Object(obj) => {
                // BTreeMap is already sorted by key, so iteration order is deterministic
                for (k, v) in obj {
                    k.hash(state);
                    v.hash_value(state);
                }
            }
            Self::Null => {}
        }
    }
}

/// Compute a hash for a row (for DISTINCT deduplication).
fn hash_row(row: &Row) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    for value in row {
        value.hash_value(&mut hasher);
    }
    hasher.finish()
}

/// Compute a hash for a single value (for PIVOT lookups).
fn hash_single_value(value: &Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    value.hash_value(&mut hasher);
    hasher.finish()
}

/// A row of query results.
pub type Row = Vec<Value>;

/// Query result containing column names and rows.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Column names.
    pub columns: Vec<String>,
    /// Result rows.
    pub rows: Vec<Row>,
}

impl QueryResult {
    /// Create a new empty result.
    pub const fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    /// Add a row to the result.
    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the result is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Context for a single posting being evaluated.
#[derive(Debug)]
pub struct PostingContext<'a> {
    /// The transaction this posting belongs to.
    pub transaction: &'a Transaction,
    /// The posting index within the transaction.
    pub posting_index: usize,
    /// Running balance after this posting (optional).
    pub balance: Option<Inventory>,
    /// The directive index (for source location lookup).
    pub directive_index: Option<usize>,
}

/// Context for window function evaluation.
#[derive(Debug, Clone)]
pub struct WindowContext {
    /// Row number within the partition (1-based).
    pub row_number: usize,
    /// Rank within the partition (1-based, ties get same rank).
    pub rank: usize,
    /// Dense rank within the partition (1-based, no gaps after ties).
    pub dense_rank: usize,
}

/// Account information cached from Open/Close directives.
#[derive(Debug, Clone)]
struct AccountInfo {
    /// Date the account was opened.
    open_date: Option<NaiveDate>,
    /// Date the account was closed (if any).
    close_date: Option<NaiveDate>,
    /// Metadata from the Open directive.
    open_meta: Metadata,
}

/// An in-memory table created by CREATE TABLE.
#[derive(Debug, Clone)]
pub struct Table {
    /// Column names.
    pub columns: Vec<String>,
    /// Rows of data.
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    /// Create a new empty table with the given column names.
    #[allow(clippy::missing_const_for_fn)] // Vec::new() isn't const with owned columns
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    /// Add a row to the table.
    pub fn add_row(&mut self, row: Vec<Value>) {
        self.rows.push(row);
    }
}

/// Query executor.
pub struct Executor<'a> {
    /// All directives to query over.
    directives: &'a [Directive],
    /// Spanned directives (optional, for source location support).
    spanned_directives: Option<&'a [Spanned<Directive>]>,
    /// Account balances (built up during query).
    balances: HashMap<InternedStr, Inventory>,
    /// Price database for `VALUE()` conversions.
    price_db: crate::price::PriceDatabase,
    /// Target currency for `VALUE()` conversions.
    target_currency: Option<String>,
    /// Cache for compiled regex patterns.
    regex_cache: RefCell<HashMap<String, Option<Regex>>>,
    /// Account info cache from Open/Close directives.
    account_info: HashMap<String, AccountInfo>,
    /// Source locations for directives (indexed by directive index).
    source_locations: Option<Vec<SourceLocation>>,
    /// In-memory tables created by CREATE TABLE.
    tables: HashMap<String, Table>,
}

impl<'a> Executor<'a> {
    /// Create a new executor with the given directives.
    pub fn new(directives: &'a [Directive]) -> Self {
        let price_db = crate::price::PriceDatabase::from_directives(directives);

        // Build account info cache from Open/Close directives
        let mut account_info: HashMap<String, AccountInfo> = HashMap::new();
        for directive in directives {
            match directive {
                Directive::Open(open) => {
                    let account = open.account.to_string();
                    let info = account_info.entry(account).or_insert_with(|| AccountInfo {
                        open_date: None,
                        close_date: None,
                        open_meta: Metadata::new(),
                    });
                    info.open_date = Some(open.date);
                    info.open_meta.clone_from(&open.meta);
                }
                Directive::Close(close) => {
                    let account = close.account.to_string();
                    let info = account_info.entry(account).or_insert_with(|| AccountInfo {
                        open_date: None,
                        close_date: None,
                        open_meta: Metadata::new(),
                    });
                    info.close_date = Some(close.date);
                }
                _ => {}
            }
        }

        Self {
            directives,
            spanned_directives: None,
            balances: HashMap::new(),
            price_db,
            target_currency: None,
            regex_cache: RefCell::new(HashMap::new()),
            account_info,
            source_locations: None,
            tables: HashMap::new(),
        }
    }

    /// Create a new executor with source location support.
    ///
    /// This constructor accepts spanned directives and a source map, enabling
    /// the `filename`, `lineno`, and `location` columns in queries.
    pub fn new_with_sources(
        spanned_directives: &'a [Spanned<Directive>],
        source_map: &SourceMap,
    ) -> Self {
        // Build price database from spanned directives
        let mut price_db = crate::price::PriceDatabase::new();
        for spanned in spanned_directives {
            if let Directive::Price(p) = &spanned.value {
                price_db.add_price(p);
            }
        }

        // Build source locations
        let source_locations: Vec<SourceLocation> = spanned_directives
            .iter()
            .map(|spanned| {
                let file = source_map.get(spanned.file_id as usize);
                let (line, _col) = file.map_or((0, 0), |f| f.line_col(spanned.span.start));
                SourceLocation {
                    filename: file.map_or_else(String::new, |f| f.path.display().to_string()),
                    lineno: line,
                }
            })
            .collect();

        // Build account info cache from Open/Close directives
        let mut account_info: HashMap<String, AccountInfo> = HashMap::new();
        for spanned in spanned_directives {
            match &spanned.value {
                Directive::Open(open) => {
                    let account = open.account.to_string();
                    let info = account_info.entry(account).or_insert_with(|| AccountInfo {
                        open_date: None,
                        close_date: None,
                        open_meta: Metadata::new(),
                    });
                    info.open_date = Some(open.date);
                    info.open_meta.clone_from(&open.meta);
                }
                Directive::Close(close) => {
                    let account = close.account.to_string();
                    let info = account_info.entry(account).or_insert_with(|| AccountInfo {
                        open_date: None,
                        close_date: None,
                        open_meta: Metadata::new(),
                    });
                    info.close_date = Some(close.date);
                }
                _ => {}
            }
        }

        Self {
            directives: &[], // Empty - we use spanned_directives instead
            spanned_directives: Some(spanned_directives),
            balances: HashMap::new(),
            price_db,
            target_currency: None,
            regex_cache: RefCell::new(HashMap::new()),
            account_info,
            source_locations: Some(source_locations),
            tables: HashMap::new(),
        }
    }

    /// Get the source location for a directive by index.
    fn get_source_location(&self, directive_index: usize) -> Option<&SourceLocation> {
        self.source_locations
            .as_ref()
            .and_then(|locs| locs.get(directive_index))
    }

    /// Get or compile a regex pattern from the cache.
    ///
    /// Returns `Some(Regex)` if the pattern is valid, `None` if it's invalid.
    /// Invalid patterns are cached as `None` to avoid repeated compilation attempts.
    fn get_or_compile_regex(&self, pattern: &str) -> Option<Regex> {
        let mut cache = self.regex_cache.borrow_mut();
        if let Some(cached) = cache.get(pattern) {
            return cached.clone();
        }
        let compiled = Regex::new(pattern).ok();
        cache.insert(pattern.to_string(), compiled.clone());
        compiled
    }

    /// Get or compile a regex pattern, returning an error if invalid.
    fn require_regex(&self, pattern: &str) -> Result<Regex, QueryError> {
        self.get_or_compile_regex(pattern)
            .ok_or_else(|| QueryError::Type(format!("invalid regex: {pattern}")))
    }

    /// Set the target currency for `VALUE()` conversions.
    pub fn set_target_currency(&mut self, currency: impl Into<String>) {
        self.target_currency = Some(currency.into());
    }

    /// Execute a query and return the results.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] in the following cases:
    ///
    /// - [`QueryError::UnknownColumn`] - A referenced column name doesn't exist
    /// - [`QueryError::UnknownFunction`] - An unknown function is called
    /// - [`QueryError::InvalidArguments`] - Function called with wrong arguments
    /// - [`QueryError::Type`] - Type mismatch in expression (e.g., comparing string to number)
    /// - [`QueryError::Aggregation`] - Error in aggregate function (SUM, COUNT, etc.)
    /// - [`QueryError::Evaluation`] - General expression evaluation error
    pub fn execute(&mut self, query: &Query) -> Result<QueryResult, QueryError> {
        match query {
            Query::Select(select) => self.execute_select(select),
            Query::Journal(journal) => self.execute_journal(journal),
            Query::Balances(balances) => self.execute_balances(balances),
            Query::Print(print) => self.execute_print(print),
            Query::CreateTable(create) => self.execute_create_table(create),
            Query::Insert(insert) => self.execute_insert(insert),
        }
    }

    /// Execute a SELECT query.
    fn execute_select(&self, query: &SelectQuery) -> Result<QueryResult, QueryError> {
        // Check if we have a subquery
        if let Some(from) = &query.from {
            if let Some(subquery) = &from.subquery {
                return self.execute_select_from_subquery(query, subquery);
            }
            // Check if we're selecting from a user-created table
            if let Some(table_name) = &from.table_name {
                return self.execute_select_from_table(query, table_name);
            }
        }

        // Determine column names
        let column_names = self.resolve_column_names(&query.targets)?;
        let mut result = QueryResult::new(column_names.clone());

        // Collect matching postings
        let postings = self.collect_postings(query.from.as_ref(), query.where_clause.as_ref())?;

        // Check if this is an aggregate query
        let is_aggregate = query
            .targets
            .iter()
            .any(|t| Self::is_aggregate_expr(&t.expr));

        if is_aggregate {
            // Group and aggregate
            let grouped = self.group_postings(&postings, query.group_by.as_ref())?;
            for (_, group) in grouped {
                let row = self.evaluate_aggregate_row(&query.targets, &group)?;

                // Apply HAVING filter on aggregated row
                if let Some(having_expr) = &query.having {
                    if !self.evaluate_having_filter(
                        having_expr,
                        &row,
                        &column_names,
                        &query.targets,
                        &group,
                    )? {
                        continue;
                    }
                }

                result.add_row(row);
            }
        } else {
            // Check if query has window functions
            let has_windows = Self::has_window_functions(&query.targets);
            let window_contexts = if has_windows {
                if let Some(wf) = Self::find_window_function(&query.targets) {
                    Some(self.compute_window_contexts(&postings, wf)?)
                } else {
                    None
                }
            } else {
                None
            };

            // Simple query - one row per posting
            // Use HashSet for O(1) DISTINCT deduplication instead of O(n) contains()
            let mut seen_hashes: HashSet<u64> = if query.distinct {
                HashSet::with_capacity(postings.len())
            } else {
                HashSet::new()
            };

            for (i, ctx) in postings.iter().enumerate() {
                let row = if let Some(ref wctxs) = window_contexts {
                    self.evaluate_row_with_window(&query.targets, ctx, Some(&wctxs[i]))?
                } else {
                    self.evaluate_row(&query.targets, ctx)?
                };
                if query.distinct {
                    // O(1) hash-based deduplication
                    let row_hash = hash_row(&row);
                    if seen_hashes.insert(row_hash) {
                        result.add_row(row);
                    }
                } else {
                    result.add_row(row);
                }
            }
        }

        // Apply PIVOT BY transformation
        if let Some(pivot_exprs) = &query.pivot_by {
            result = self.apply_pivot(&result, pivot_exprs, &query.targets)?;
        }

        // Apply ORDER BY
        if let Some(order_by) = &query.order_by {
            self.sort_results(&mut result, order_by)?;
        } else if query.group_by.is_some() && !result.rows.is_empty() && !result.columns.is_empty()
        {
            // When there's GROUP BY but no ORDER BY, sort by the first column
            // for deterministic output (matches Python beancount behavior)
            let first_col = result.columns[0].clone();
            let default_order = vec![OrderSpec {
                expr: Expr::Column(first_col),
                direction: SortDirection::Asc,
            }];
            self.sort_results(&mut result, &default_order)?;
        }

        // Apply LIMIT
        if let Some(limit) = query.limit {
            result.rows.truncate(limit as usize);
        }

        Ok(result)
    }

    /// Execute a SELECT query that sources from a subquery.
    fn execute_select_from_subquery(
        &self,
        outer_query: &SelectQuery,
        inner_query: &SelectQuery,
    ) -> Result<QueryResult, QueryError> {
        // Execute the inner query first
        let inner_result = self.execute_select(inner_query)?;

        // Build a column name -> index mapping for the inner result
        let inner_column_map: HashMap<String, usize> = inner_result
            .columns
            .iter()
            .enumerate()
            .map(|(i, name)| (name.to_lowercase(), i))
            .collect();

        // Determine outer column names
        let outer_column_names =
            self.resolve_subquery_column_names(&outer_query.targets, &inner_result.columns)?;
        let mut result = QueryResult::new(outer_column_names);

        // Use HashSet for O(1) DISTINCT deduplication
        let mut seen_hashes: HashSet<u64> = if outer_query.distinct {
            HashSet::with_capacity(inner_result.rows.len())
        } else {
            HashSet::new()
        };

        // Process each row from the inner result
        for inner_row in &inner_result.rows {
            // Apply outer WHERE clause if present
            if let Some(where_expr) = &outer_query.where_clause {
                if !self.evaluate_subquery_filter(where_expr, inner_row, &inner_column_map)? {
                    continue;
                }
            }

            // Evaluate outer targets
            let outer_row =
                self.evaluate_subquery_row(&outer_query.targets, inner_row, &inner_column_map)?;

            if outer_query.distinct {
                // O(1) hash-based deduplication
                let row_hash = hash_row(&outer_row);
                if seen_hashes.insert(row_hash) {
                    result.add_row(outer_row);
                }
            } else {
                result.add_row(outer_row);
            }
        }

        // Apply ORDER BY
        if let Some(order_by) = &outer_query.order_by {
            self.sort_results(&mut result, order_by)?;
        }

        // Apply LIMIT
        if let Some(limit) = outer_query.limit {
            result.rows.truncate(limit as usize);
        }

        Ok(result)
    }

    /// Execute a SELECT query that sources from a user-created table.
    fn execute_select_from_table(
        &self,
        query: &SelectQuery,
        table_name: &str,
    ) -> Result<QueryResult, QueryError> {
        let table_name_upper = table_name.to_uppercase();

        // Look up the table
        let table = self.tables.get(&table_name_upper).ok_or_else(|| {
            QueryError::Evaluation(format!("table '{table_name}' does not exist"))
        })?;

        // Build a column name -> index mapping for the table
        let column_map: HashMap<String, usize> = table
            .columns
            .iter()
            .enumerate()
            .map(|(i, name)| (name.to_lowercase(), i))
            .collect();

        // Determine column names for the result
        let column_names = self.resolve_subquery_column_names(&query.targets, &table.columns)?;
        let mut result = QueryResult::new(column_names);

        // Use HashSet for O(1) DISTINCT deduplication
        let mut seen_hashes: HashSet<u64> = if query.distinct {
            HashSet::with_capacity(table.rows.len())
        } else {
            HashSet::new()
        };

        // Process each row from the table
        for row in &table.rows {
            // Apply WHERE clause if present
            if let Some(where_expr) = &query.where_clause {
                if !self.evaluate_subquery_filter(where_expr, row, &column_map)? {
                    continue;
                }
            }

            // Evaluate targets
            let result_row = self.evaluate_subquery_row(&query.targets, row, &column_map)?;

            if query.distinct {
                // O(1) hash-based deduplication
                let row_hash = hash_row(&result_row);
                if seen_hashes.insert(row_hash) {
                    result.add_row(result_row);
                }
            } else {
                result.add_row(result_row);
            }
        }

        // Apply ORDER BY
        if let Some(order_by) = &query.order_by {
            self.sort_results(&mut result, order_by)?;
        }

        // Apply LIMIT
        if let Some(limit) = query.limit {
            result.rows.truncate(limit as usize);
        }

        Ok(result)
    }

    /// Resolve column names for a query from a subquery.
    fn resolve_subquery_column_names(
        &self,
        targets: &[Target],
        inner_columns: &[String],
    ) -> Result<Vec<String>, QueryError> {
        let mut names = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            if let Some(alias) = &target.alias {
                names.push(alias.clone());
            } else if matches!(target.expr, Expr::Wildcard) {
                // Expand wildcard to all inner columns
                names.extend(inner_columns.iter().cloned());
            } else {
                names.push(self.expr_to_name(&target.expr, i));
            }
        }
        Ok(names)
    }

    /// Evaluate a filter expression against a subquery row.
    fn evaluate_subquery_filter(
        &self,
        expr: &Expr,
        row: &[Value],
        column_map: &HashMap<String, usize>,
    ) -> Result<bool, QueryError> {
        let val = self.evaluate_subquery_expr(expr, row, column_map)?;
        self.to_bool(&val)
    }

    /// Evaluate an expression against a subquery row.
    fn evaluate_subquery_expr(
        &self,
        expr: &Expr,
        row: &[Value],
        column_map: &HashMap<String, usize>,
    ) -> Result<Value, QueryError> {
        match expr {
            Expr::Wildcard => Err(QueryError::Evaluation(
                "Wildcard not allowed in expression context".to_string(),
            )),
            Expr::Column(name) => {
                let lower = name.to_lowercase();
                if let Some(&idx) = column_map.get(&lower) {
                    Ok(row.get(idx).cloned().unwrap_or(Value::Null))
                } else {
                    Err(QueryError::Evaluation(format!(
                        "Unknown column '{name}' in subquery result"
                    )))
                }
            }
            Expr::Literal(lit) => self.evaluate_literal(lit),
            Expr::Function(func) => {
                // Evaluate function arguments
                let args: Vec<Value> = func
                    .args
                    .iter()
                    .map(|a| self.evaluate_subquery_expr(a, row, column_map))
                    .collect::<Result<Vec<_>, _>>()?;
                self.evaluate_function_on_values(&func.name, &args)
            }
            Expr::BinaryOp(op) => {
                let left = self.evaluate_subquery_expr(&op.left, row, column_map)?;
                let right = self.evaluate_subquery_expr(&op.right, row, column_map)?;
                self.binary_op_on_values(op.op, &left, &right)
            }
            Expr::UnaryOp(op) => {
                let val = self.evaluate_subquery_expr(&op.operand, row, column_map)?;
                self.unary_op_on_value(op.op, &val)
            }
            Expr::Paren(inner) => self.evaluate_subquery_expr(inner, row, column_map),
            Expr::Window(_) => Err(QueryError::Evaluation(
                "Window functions not supported in subquery expressions".to_string(),
            )),
            Expr::Between { value, low, high } => {
                let val = self.evaluate_subquery_expr(value, row, column_map)?;
                let low_val = self.evaluate_subquery_expr(low, row, column_map)?;
                let high_val = self.evaluate_subquery_expr(high, row, column_map)?;

                let ge = self.compare_values(&val, &low_val, std::cmp::Ordering::is_ge)?;
                let le = self.compare_values(&val, &high_val, std::cmp::Ordering::is_le)?;

                match (ge, le) {
                    (Value::Boolean(g), Value::Boolean(l)) => Ok(Value::Boolean(g && l)),
                    _ => Err(QueryError::Type(
                        "BETWEEN requires comparable values".to_string(),
                    )),
                }
            }
        }
    }

    /// Evaluate a row of targets against a subquery row.
    fn evaluate_subquery_row(
        &self,
        targets: &[Target],
        inner_row: &[Value],
        column_map: &HashMap<String, usize>,
    ) -> Result<Row, QueryError> {
        let mut row = Vec::new();
        for target in targets {
            if matches!(target.expr, Expr::Wildcard) {
                // Expand wildcard to all values from inner row
                row.extend(inner_row.iter().cloned());
            } else {
                row.push(self.evaluate_subquery_expr(&target.expr, inner_row, column_map)?);
            }
        }
        Ok(row)
    }

    /// Execute a JOURNAL query.
    fn execute_journal(&mut self, query: &JournalQuery) -> Result<QueryResult, QueryError> {
        // JOURNAL is a shorthand for SELECT with specific columns
        let account_pattern = &query.account_pattern;

        // Try to compile as regex (using cache)
        let account_regex = self.get_or_compile_regex(account_pattern);

        let columns = vec![
            "date".to_string(),
            "flag".to_string(),
            "payee".to_string(),
            "narration".to_string(),
            "account".to_string(),
            "position".to_string(),
            "balance".to_string(),
        ];
        let mut result = QueryResult::new(columns);

        // Filter transactions that touch the account
        for directive in self.directives {
            if let Directive::Transaction(txn) = directive {
                // Apply FROM clause filter if present
                if let Some(from) = &query.from {
                    if let Some(filter) = &from.filter {
                        if !self.evaluate_from_filter(filter, txn)? {
                            continue;
                        }
                    }
                }

                for posting in &txn.postings {
                    // Match account using regex or substring
                    let matches = if let Some(ref regex) = account_regex {
                        regex.is_match(&posting.account)
                    } else {
                        posting.account.contains(account_pattern)
                    };

                    if matches {
                        // Build the row
                        let balance = self.balances.entry(posting.account.clone()).or_default();

                        // Only process complete amounts
                        if let Some(units) = posting.amount() {
                            let pos = if let Some(cost_spec) = &posting.cost {
                                if let Some(cost) = cost_spec.resolve(units.number, txn.date) {
                                    Position::with_cost(units.clone(), cost)
                                } else {
                                    Position::simple(units.clone())
                                }
                            } else {
                                Position::simple(units.clone())
                            };
                            balance.add(pos.clone());
                        }

                        // Apply AT function if specified
                        let position_value = if let Some(at_func) = &query.at_function {
                            match at_func.to_uppercase().as_str() {
                                "COST" => {
                                    if let Some(units) = posting.amount() {
                                        if let Some(cost_spec) = &posting.cost {
                                            if let Some(cost) =
                                                cost_spec.resolve(units.number, txn.date)
                                            {
                                                let total = units.number * cost.number;
                                                Value::Amount(Amount::new(total, &cost.currency))
                                            } else {
                                                Value::Amount(units.clone())
                                            }
                                        } else {
                                            Value::Amount(units.clone())
                                        }
                                    } else {
                                        Value::Null
                                    }
                                }
                                "UNITS" => posting
                                    .amount()
                                    .map_or(Value::Null, |u| Value::Amount(u.clone())),
                                _ => posting
                                    .amount()
                                    .map_or(Value::Null, |u| Value::Amount(u.clone())),
                            }
                        } else {
                            posting
                                .amount()
                                .map_or(Value::Null, |u| Value::Amount(u.clone()))
                        };

                        let row = vec![
                            Value::Date(txn.date),
                            Value::String(txn.flag.to_string()),
                            Value::String(
                                txn.payee
                                    .as_ref()
                                    .map_or_else(String::new, ToString::to_string),
                            ),
                            Value::String(txn.narration.to_string()),
                            Value::String(posting.account.to_string()),
                            position_value,
                            Value::Inventory(balance.clone()),
                        ];
                        result.add_row(row);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Execute a BALANCES query.
    fn execute_balances(&mut self, query: &BalancesQuery) -> Result<QueryResult, QueryError> {
        // Build up balances by processing all transactions (with FROM filtering)
        self.build_balances_with_filter(query.from.as_ref())?;

        let columns = vec!["account".to_string(), "balance".to_string()];
        let mut result = QueryResult::new(columns);

        // Sort accounts for consistent output
        let mut accounts: Vec<_> = self.balances.keys().collect();
        accounts.sort();

        for account in accounts {
            // Safety: account comes from self.balances.keys(), so it's guaranteed to exist
            let Some(balance) = self.balances.get(account) else {
                continue; // Defensive: skip if somehow the key disappeared
            };

            // Apply AT function if specified
            let balance_value = if let Some(at_func) = &query.at_function {
                match at_func.to_uppercase().as_str() {
                    "COST" => {
                        // Sum up cost basis
                        let cost_inventory = balance.at_cost();
                        Value::Inventory(cost_inventory)
                    }
                    "UNITS" => {
                        // Just the units (remove cost info)
                        let units_inventory = balance.at_units();
                        Value::Inventory(units_inventory)
                    }
                    _ => Value::Inventory(balance.clone()),
                }
            } else {
                Value::Inventory(balance.clone())
            };

            let row = vec![Value::String(account.to_string()), balance_value];
            result.add_row(row);
        }

        Ok(result)
    }

    /// Execute a PRINT query.
    fn execute_print(&self, query: &PrintQuery) -> Result<QueryResult, QueryError> {
        // PRINT outputs directives in Beancount format
        let columns = vec!["directive".to_string()];
        let mut result = QueryResult::new(columns);

        for directive in self.directives {
            // Apply FROM clause filter if present
            if let Some(from) = &query.from {
                if let Some(filter) = &from.filter {
                    // PRINT filters at transaction level
                    if let Directive::Transaction(txn) = directive {
                        if !self.evaluate_from_filter(filter, txn)? {
                            continue;
                        }
                    }
                }
            }

            // Format the directive as a string
            let formatted = self.format_directive(directive);
            result.add_row(vec![Value::String(formatted)]);
        }

        Ok(result)
    }

    /// Format a directive for PRINT output.
    fn format_directive(&self, directive: &Directive) -> String {
        match directive {
            Directive::Transaction(txn) => {
                let mut out = format!("{} {} ", txn.date, txn.flag);
                if let Some(payee) = &txn.payee {
                    out.push_str(&format!("\"{payee}\" "));
                }
                out.push_str(&format!("\"{}\"", txn.narration));

                for tag in &txn.tags {
                    out.push_str(&format!(" #{tag}"));
                }
                for link in &txn.links {
                    out.push_str(&format!(" ^{link}"));
                }
                out.push('\n');

                for posting in &txn.postings {
                    out.push_str(&format!("  {}", posting.account));
                    if let Some(units) = posting.amount() {
                        out.push_str(&format!("  {} {}", units.number, units.currency));
                    }
                    out.push('\n');
                }
                out
            }
            Directive::Balance(bal) => {
                format!(
                    "{} balance {} {} {}\n",
                    bal.date, bal.account, bal.amount.number, bal.amount.currency
                )
            }
            Directive::Open(open) => {
                let mut out = format!("{} open {}", open.date, open.account);
                if !open.currencies.is_empty() {
                    out.push_str(&format!(" {}", open.currencies.join(",")));
                }
                out.push('\n');
                out
            }
            Directive::Close(close) => {
                format!("{} close {}\n", close.date, close.account)
            }
            Directive::Commodity(comm) => {
                format!("{} commodity {}\n", comm.date, comm.currency)
            }
            Directive::Pad(pad) => {
                format!("{} pad {} {}\n", pad.date, pad.account, pad.source_account)
            }
            Directive::Event(event) => {
                format!(
                    "{} event \"{}\" \"{}\"\n",
                    event.date, event.event_type, event.value
                )
            }
            Directive::Query(query) => {
                format!(
                    "{} query \"{}\" \"{}\"\n",
                    query.date, query.name, query.query
                )
            }
            Directive::Note(note) => {
                format!("{} note {} \"{}\"\n", note.date, note.account, note.comment)
            }
            Directive::Document(doc) => {
                format!("{} document {} \"{}\"\n", doc.date, doc.account, doc.path)
            }
            Directive::Price(price) => {
                format!(
                    "{} price {} {} {}\n",
                    price.date, price.currency, price.amount.number, price.amount.currency
                )
            }
            Directive::Custom(custom) => {
                format!("{} custom \"{}\"\n", custom.date, custom.custom_type)
            }
        }
    }

    /// Execute a CREATE TABLE statement.
    fn execute_create_table(
        &mut self,
        create: &CreateTableStmt,
    ) -> Result<QueryResult, QueryError> {
        let table_name = create.table_name.to_uppercase();

        // Check if table already exists
        if self.tables.contains_key(&table_name) {
            return Err(QueryError::Evaluation(format!(
                "table '{}' already exists",
                create.table_name
            )));
        }

        let table = if let Some(select) = &create.as_select {
            // CREATE TABLE ... AS SELECT ...
            let result = self.execute_select(select)?;
            Table {
                columns: result.columns,
                rows: result.rows,
            }
        } else {
            // CREATE TABLE ... (col1, col2, ...)
            let columns = create.columns.iter().map(|c| c.name.clone()).collect();
            Table::new(columns)
        };

        self.tables.insert(table_name, table);

        // Return empty result with a message
        let mut result = QueryResult::new(vec!["result".to_string()]);
        result.add_row(vec![Value::String(format!(
            "Created table '{}'",
            create.table_name
        ))]);
        Ok(result)
    }

    /// Execute an INSERT statement.
    fn execute_insert(&mut self, insert: &InsertStmt) -> Result<QueryResult, QueryError> {
        let table_name = insert.table_name.to_uppercase();

        // Check if table exists
        if !self.tables.contains_key(&table_name) {
            return Err(QueryError::Evaluation(format!(
                "table '{}' does not exist",
                insert.table_name
            )));
        }

        // Get the table's column count for validation
        let table_column_count = self
            .tables
            .get(&table_name)
            .expect("table existence verified above")
            .columns
            .len();

        let rows_to_insert: Vec<Vec<Value>> = match &insert.source {
            InsertSource::Values(value_rows) => {
                // Evaluate each row of expressions
                let mut rows = Vec::with_capacity(value_rows.len());
                for value_row in value_rows {
                    // Validate column count
                    if let Some(ref cols) = insert.columns {
                        if value_row.len() != cols.len() {
                            return Err(QueryError::Evaluation(format!(
                                "INSERT has {} columns but VALUES has {} values",
                                cols.len(),
                                value_row.len()
                            )));
                        }
                    } else if value_row.len() != table_column_count {
                        return Err(QueryError::Evaluation(format!(
                            "table has {} columns but VALUES has {} values",
                            table_column_count,
                            value_row.len()
                        )));
                    }

                    // Evaluate each expression in the row
                    let mut row = Vec::with_capacity(value_row.len());
                    for expr in value_row {
                        let value = self.evaluate_literal_expr(expr)?;
                        row.push(value);
                    }
                    rows.push(row);
                }
                rows
            }
            InsertSource::Select(select) => {
                // Execute the SELECT and use its results
                let result = self.execute_select(select)?;

                // Validate column count
                if let Some(ref cols) = insert.columns {
                    if result.columns.len() != cols.len() {
                        return Err(QueryError::Evaluation(format!(
                            "INSERT has {} columns but SELECT returns {} columns",
                            cols.len(),
                            result.columns.len()
                        )));
                    }
                } else if result.columns.len() != table_column_count {
                    return Err(QueryError::Evaluation(format!(
                        "table has {} columns but SELECT returns {} columns",
                        table_column_count,
                        result.columns.len()
                    )));
                }

                result.rows
            }
        };

        let rows_inserted = rows_to_insert.len();

        // Insert rows into the table
        if let Some(ref cols) = insert.columns {
            // Insert with specific columns - need to map to table column positions
            let table = self
                .tables
                .get(&table_name)
                .expect("table existence verified above");
            let col_indices: Vec<Option<usize>> = cols
                .iter()
                .map(|c| {
                    table
                        .columns
                        .iter()
                        .position(|tc| tc.eq_ignore_ascii_case(c))
                })
                .collect();

            // Validate all column names exist
            for (i, idx) in col_indices.iter().enumerate() {
                if idx.is_none() {
                    return Err(QueryError::Evaluation(format!(
                        "column '{}' does not exist in table '{}'",
                        cols[i], insert.table_name
                    )));
                }
            }

            // Build full rows with NULLs for missing columns
            let table = self
                .tables
                .get_mut(&table_name)
                .expect("table existence verified above");
            for value_row in rows_to_insert {
                let mut full_row = vec![Value::Null; table_column_count];
                for (i, value) in value_row.into_iter().enumerate() {
                    // Use .get() for defensive bounds checking even though validation
                    // should guarantee lengths match
                    if let Some(idx) = col_indices.get(i).copied().flatten() {
                        full_row[idx] = value;
                    }
                }
                table.add_row(full_row);
            }
        } else {
            // Insert all columns in order
            let table = self
                .tables
                .get_mut(&table_name)
                .expect("table existence verified above");
            for row in rows_to_insert {
                table.add_row(row);
            }
        }

        // Return result with row count
        let mut result = QueryResult::new(vec!["result".to_string()]);
        result.add_row(vec![Value::String(format!(
            "Inserted {} row(s) into '{}'",
            rows_inserted, insert.table_name
        ))]);
        Ok(result)
    }

    /// Evaluate a literal expression (for INSERT VALUES).
    fn evaluate_literal_expr(&self, expr: &Expr) -> Result<Value, QueryError> {
        match expr {
            Expr::Literal(lit) => self.evaluate_literal(lit),
            Expr::UnaryOp(unary) => {
                let value = self.evaluate_literal_expr(&unary.operand)?;
                match unary.op {
                    UnaryOperator::Neg => match value {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        Value::Integer(i) => Ok(Value::Integer(-i)),
                        _ => Err(QueryError::Type(
                            "cannot negate non-numeric value".to_string(),
                        )),
                    },
                    UnaryOperator::Not => match value {
                        Value::Boolean(b) => Ok(Value::Boolean(!b)),
                        _ => Err(QueryError::Type(
                            "cannot negate non-boolean value".to_string(),
                        )),
                    },
                    _ => Err(QueryError::Evaluation(
                        "unsupported operator in INSERT VALUES".to_string(),
                    )),
                }
            }
            Expr::Paren(inner) => self.evaluate_literal_expr(inner),
            Expr::Function(func) => {
                // Allow some simple functions in VALUES
                let name = func.name.to_uppercase();
                match name.as_str() {
                    "DATE" => {
                        // DATE(year, month, day) or DATE('YYYY-MM-DD')
                        if func.args.len() == 1 {
                            let arg = self.evaluate_literal_expr(&func.args[0])?;
                            if let Value::String(s) = arg {
                                if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                    return Ok(Value::Date(date));
                                }
                            }
                            Err(QueryError::Type("invalid date string".to_string()))
                        } else if func.args.len() == 3 {
                            let year = self.evaluate_literal_expr(&func.args[0])?;
                            let month = self.evaluate_literal_expr(&func.args[1])?;
                            let day = self.evaluate_literal_expr(&func.args[2])?;
                            match (year, month, day) {
                                (Value::Integer(y), Value::Integer(m), Value::Integer(d)) => {
                                    if let Some(date) =
                                        NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32)
                                    {
                                        Ok(Value::Date(date))
                                    } else {
                                        Err(QueryError::Type("invalid date components".to_string()))
                                    }
                                }
                                _ => Err(QueryError::Type(
                                    "DATE() requires integer arguments".to_string(),
                                )),
                            }
                        } else {
                            Err(QueryError::Evaluation(
                                "DATE() requires 1 or 3 arguments".to_string(),
                            ))
                        }
                    }
                    _ => Err(QueryError::Evaluation(format!(
                        "function '{}' not supported in INSERT VALUES",
                        func.name
                    ))),
                }
            }
            _ => Err(QueryError::Evaluation(
                "only literal values are allowed in INSERT VALUES".to_string(),
            )),
        }
    }

    /// Build up account balances with optional FROM filtering.
    fn build_balances_with_filter(&mut self, from: Option<&FromClause>) -> Result<(), QueryError> {
        for directive in self.directives {
            if let Directive::Transaction(txn) = directive {
                // Apply FROM filter if present
                if let Some(from_clause) = from {
                    if let Some(filter) = &from_clause.filter {
                        if !self.evaluate_from_filter(filter, txn)? {
                            continue;
                        }
                    }
                }

                for posting in &txn.postings {
                    if let Some(units) = posting.amount() {
                        let balance = self.balances.entry(posting.account.clone()).or_default();

                        let pos = if let Some(cost_spec) = &posting.cost {
                            if let Some(cost) = cost_spec.resolve(units.number, txn.date) {
                                Position::with_cost(units.clone(), cost)
                            } else {
                                Position::simple(units.clone())
                            }
                        } else {
                            Position::simple(units.clone())
                        };
                        balance.add(pos);
                    }
                }
            }
        }
        Ok(())
    }

    /// Collect postings matching the FROM and WHERE clauses.
    fn collect_postings(
        &self,
        from: Option<&FromClause>,
        where_clause: Option<&Expr>,
    ) -> Result<Vec<PostingContext<'a>>, QueryError> {
        let mut postings = Vec::new();
        // Track running balance per account
        let mut running_balances: HashMap<InternedStr, Inventory> = HashMap::new();

        // Create an iterator over (directive_index, directive) pairs
        // Handle both spanned and unspanned directives
        let directive_iter: Vec<(usize, &Directive)> =
            if let Some(spanned) = self.spanned_directives {
                spanned
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i, &s.value))
                    .collect()
            } else {
                self.directives.iter().enumerate().collect()
            };

        for (directive_index, directive) in directive_iter {
            if let Directive::Transaction(txn) = directive {
                // Check FROM clause (transaction-level filter)
                if let Some(from) = from {
                    // Apply date filters
                    if let Some(open_date) = from.open_on {
                        if txn.date < open_date {
                            // Update balances but don't include in results
                            for posting in &txn.postings {
                                if let Some(units) = posting.amount() {
                                    let balance = running_balances
                                        .entry(posting.account.clone())
                                        .or_default();
                                    balance.add(Position::simple(units.clone()));
                                }
                            }
                            continue;
                        }
                    }
                    if let Some(close_date) = from.close_on {
                        if txn.date > close_date {
                            continue;
                        }
                    }
                    // Apply filter expression
                    if let Some(filter) = &from.filter {
                        if !self.evaluate_from_filter(filter, txn)? {
                            continue;
                        }
                    }
                }

                // Add postings with running balance
                for (i, posting) in txn.postings.iter().enumerate() {
                    // Update running balance for this account
                    if let Some(units) = posting.amount() {
                        let balance = running_balances.entry(posting.account.clone()).or_default();
                        balance.add(Position::simple(units.clone()));
                    }

                    let ctx = PostingContext {
                        transaction: txn,
                        posting_index: i,
                        balance: running_balances.get(&posting.account).cloned(),
                        directive_index: Some(directive_index),
                    };

                    // Check WHERE clause (posting-level filter)
                    if let Some(where_expr) = where_clause {
                        if self.evaluate_predicate(where_expr, &ctx)? {
                            postings.push(ctx);
                        }
                    } else {
                        postings.push(ctx);
                    }
                }
            }
        }

        Ok(postings)
    }

    /// Evaluate a FROM filter on a transaction.
    fn evaluate_from_filter(&self, filter: &Expr, txn: &Transaction) -> Result<bool, QueryError> {
        // Handle special FROM predicates
        match filter {
            Expr::Function(func) => {
                if func.name.to_uppercase().as_str() == "HAS_ACCOUNT" {
                    if func.args.len() != 1 {
                        return Err(QueryError::InvalidArguments(
                            "has_account".to_string(),
                            "expected 1 argument".to_string(),
                        ));
                    }
                    let pattern = match &func.args[0] {
                        Expr::Literal(Literal::String(s)) => s.clone(),
                        Expr::Column(s) => s.clone(),
                        _ => {
                            return Err(QueryError::Type(
                                "has_account expects a string pattern".to_string(),
                            ));
                        }
                    };
                    // Check if any posting matches the account pattern (using cache)
                    let regex = self.require_regex(&pattern)?;
                    for posting in &txn.postings {
                        if regex.is_match(&posting.account) {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                } else {
                    // For other functions, create a dummy context and evaluate
                    let dummy_ctx = PostingContext {
                        transaction: txn,
                        posting_index: 0,
                        balance: None,
                        directive_index: None,
                    };
                    self.evaluate_predicate(filter, &dummy_ctx)
                }
            }
            Expr::BinaryOp(op) => {
                // Handle YEAR = N, MONTH = N, etc.
                match (&op.left, &op.right) {
                    (Expr::Column(col), Expr::Literal(lit)) if col.to_uppercase() == "YEAR" => {
                        // Handle both Integer and Number for year comparison
                        let year_val = match lit {
                            Literal::Integer(n) => Some(*n as i32),
                            Literal::Number(n) => n.to_string().parse::<i32>().ok(),
                            _ => None,
                        };
                        if let Some(n) = year_val {
                            let matches = txn.date.year() == n;
                            Ok(if op.op == BinaryOperator::Eq {
                                matches
                            } else {
                                !matches
                            })
                        } else {
                            Ok(false)
                        }
                    }
                    (Expr::Column(col), Expr::Literal(lit)) if col.to_uppercase() == "MONTH" => {
                        // Handle both Integer and Number for month comparison
                        let month_val = match lit {
                            Literal::Integer(n) => Some(*n as u32),
                            Literal::Number(n) => n.to_string().parse::<u32>().ok(),
                            _ => None,
                        };
                        if let Some(n) = month_val {
                            let matches = txn.date.month() == n;
                            Ok(if op.op == BinaryOperator::Eq {
                                matches
                            } else {
                                !matches
                            })
                        } else {
                            Ok(false)
                        }
                    }
                    (Expr::Column(col), Expr::Literal(Literal::Date(d)))
                        if col.to_uppercase() == "DATE" =>
                    {
                        let matches = match op.op {
                            BinaryOperator::Eq => txn.date == *d,
                            BinaryOperator::Ne => txn.date != *d,
                            BinaryOperator::Lt => txn.date < *d,
                            BinaryOperator::Le => txn.date <= *d,
                            BinaryOperator::Gt => txn.date > *d,
                            BinaryOperator::Ge => txn.date >= *d,
                            _ => false,
                        };
                        Ok(matches)
                    }
                    _ => {
                        // Fall back to posting-level evaluation
                        let dummy_ctx = PostingContext {
                            transaction: txn,
                            posting_index: 0,
                            balance: None,
                            directive_index: None,
                        };
                        self.evaluate_predicate(filter, &dummy_ctx)
                    }
                }
            }
            _ => {
                // For other expressions, create a dummy context
                let dummy_ctx = PostingContext {
                    transaction: txn,
                    posting_index: 0,
                    balance: None,
                    directive_index: None,
                };
                self.evaluate_predicate(filter, &dummy_ctx)
            }
        }
    }

    /// Evaluate a predicate expression in the context of a posting.
    fn evaluate_predicate(&self, expr: &Expr, ctx: &PostingContext) -> Result<bool, QueryError> {
        let value = self.evaluate_expr(expr, ctx)?;
        match value {
            Value::Boolean(b) => Ok(b),
            Value::Null => Ok(false),
            _ => Err(QueryError::Type("expected boolean expression".to_string())),
        }
    }

    /// Evaluate an expression in the context of a posting.
    fn evaluate_expr(&self, expr: &Expr, ctx: &PostingContext) -> Result<Value, QueryError> {
        match expr {
            Expr::Wildcard => Ok(Value::Null), // Wildcard isn't really an expression
            Expr::Column(name) => self.evaluate_column(name, ctx),
            Expr::Literal(lit) => self.evaluate_literal(lit),
            Expr::Function(func) => self.evaluate_function(func, ctx),
            Expr::Window(_) => {
                // Window functions are evaluated at the query level, not per-posting
                // This case should not be reached; window values are pre-computed
                Err(QueryError::Evaluation(
                    "Window function cannot be evaluated in posting context".to_string(),
                ))
            }
            Expr::BinaryOp(op) => self.evaluate_binary_op(op, ctx),
            Expr::UnaryOp(op) => self.evaluate_unary_op(op, ctx),
            Expr::Paren(inner) => self.evaluate_expr(inner, ctx),
            Expr::Between { value, low, high } => {
                let val = self.evaluate_expr(value, ctx)?;
                let low_val = self.evaluate_expr(low, ctx)?;
                let high_val = self.evaluate_expr(high, ctx)?;

                let ge = self.compare_values(&val, &low_val, std::cmp::Ordering::is_ge)?;
                let le = self.compare_values(&val, &high_val, std::cmp::Ordering::is_le)?;

                match (ge, le) {
                    (Value::Boolean(g), Value::Boolean(l)) => Ok(Value::Boolean(g && l)),
                    _ => Err(QueryError::Type(
                        "BETWEEN requires comparable values".to_string(),
                    )),
                }
            }
        }
    }

    /// Evaluate a column reference.
    fn evaluate_column(&self, name: &str, ctx: &PostingContext) -> Result<Value, QueryError> {
        let posting = &ctx.transaction.postings[ctx.posting_index];

        match name {
            "date" => Ok(Value::Date(ctx.transaction.date)),
            "account" => Ok(Value::String(posting.account.to_string())),
            "narration" => Ok(Value::String(ctx.transaction.narration.to_string())),
            "payee" => Ok(ctx
                .transaction
                .payee
                .as_ref()
                .map_or(Value::Null, |p| Value::String(p.to_string()))),
            "flag" => Ok(Value::String(ctx.transaction.flag.to_string())),
            "tags" => Ok(Value::StringSet(
                ctx.transaction
                    .tags
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            )),
            "links" => Ok(Value::StringSet(
                ctx.transaction
                    .links
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            )),
            "position" => {
                // Position includes both units and cost
                if let Some(units) = posting.amount() {
                    if let Some(cost_spec) = &posting.cost {
                        if let (Some(number_per), Some(currency)) =
                            (&cost_spec.number_per, &cost_spec.currency)
                        {
                            // Use Cost::new() to auto-quantize the cost number
                            let cost = rustledger_core::Cost::new(*number_per, currency.clone())
                                .with_date_opt(cost_spec.date)
                                .with_label_opt(cost_spec.label.clone());
                            return Ok(Value::Position(Position::with_cost(units.clone(), cost)));
                        }
                    }
                    Ok(Value::Position(Position::simple(units.clone())))
                } else {
                    Ok(Value::Null)
                }
            }
            "units" => Ok(posting
                .amount()
                .map_or(Value::Null, |u| Value::Amount(u.clone()))),
            "cost" => {
                // Get the cost of the posting
                if let Some(units) = posting.amount() {
                    if let Some(cost) = &posting.cost {
                        if let Some(number_per) = &cost.number_per {
                            if let Some(currency) = &cost.currency {
                                let total = units.number.abs() * number_per;
                                return Ok(Value::Amount(Amount::new(total, currency.clone())));
                            }
                        }
                    }
                }
                Ok(Value::Null)
            }
            "weight" => {
                // Weight is the amount used for transaction balancing
                // With cost: units × cost currency
                // Without cost: units amount
                if let Some(units) = posting.amount() {
                    if let Some(cost) = &posting.cost {
                        if let Some(number_per) = &cost.number_per {
                            if let Some(currency) = &cost.currency {
                                let total = units.number * number_per;
                                return Ok(Value::Amount(Amount::new(total, currency.clone())));
                            }
                        }
                    }
                    // No cost, use units
                    Ok(Value::Amount(units.clone()))
                } else {
                    Ok(Value::Null)
                }
            }
            "balance" => {
                // Running balance for this account
                if let Some(ref balance) = ctx.balance {
                    Ok(Value::Inventory(balance.clone()))
                } else {
                    Ok(Value::Null)
                }
            }
            "year" => Ok(Value::Integer(ctx.transaction.date.year().into())),
            "month" => Ok(Value::Integer(ctx.transaction.date.month().into())),
            "day" => Ok(Value::Integer(ctx.transaction.date.day().into())),
            "currency" => Ok(posting
                .amount()
                .map_or(Value::Null, |u| Value::String(u.currency.to_string()))),
            "number" => Ok(posting
                .amount()
                .map_or(Value::Null, |u| Value::Number(u.number))),
            // Posting flag (separate from transaction flag)
            "posting_flag" => Ok(posting
                .flag
                .map_or(Value::Null, |f| Value::String(f.to_string()))),
            // Description: "payee narration" or just narration
            "description" => {
                let desc = match &ctx.transaction.payee {
                    Some(payee) => format!("{} {}", payee, ctx.transaction.narration),
                    None => ctx.transaction.narration.to_string(),
                };
                Ok(Value::String(desc))
            }
            // Cost number (per-unit cost)
            "cost_number" => Ok(posting
                .cost
                .as_ref()
                .and_then(|c| c.number_per)
                .map_or(Value::Null, Value::Number)),
            // Cost currency
            "cost_currency" => Ok(posting
                .cost
                .as_ref()
                .and_then(|c| c.currency.as_ref())
                .map_or(Value::Null, |c| Value::String(c.to_string()))),
            // Cost date
            "cost_date" => Ok(posting
                .cost
                .as_ref()
                .and_then(|c| c.date)
                .map_or(Value::Null, Value::Date)),
            // Cost label
            "cost_label" => Ok(posting
                .cost
                .as_ref()
                .and_then(|c| c.label.as_ref())
                .map_or(Value::Null, |l| Value::String(l.clone()))),
            // Price annotation
            "price" => {
                use rustledger_core::PriceAnnotation;
                if let Some(price) = &posting.price {
                    match price {
                        PriceAnnotation::Unit(amount) | PriceAnnotation::Total(amount) => {
                            Ok(Value::Amount(amount.clone()))
                        }
                        PriceAnnotation::UnitIncomplete(inc)
                        | PriceAnnotation::TotalIncomplete(inc) => {
                            // Try to get complete amount from incomplete
                            if let Some(amount) = inc.as_amount().cloned() {
                                Ok(Value::Amount(amount))
                            } else {
                                Ok(Value::Null)
                            }
                        }
                        PriceAnnotation::UnitEmpty | PriceAnnotation::TotalEmpty => Ok(Value::Null),
                    }
                } else {
                    Ok(Value::Null)
                }
            }
            // All accounts in the transaction
            "accounts" => Ok(Value::StringSet(
                ctx.transaction
                    .postings
                    .iter()
                    .map(|p| p.account.to_string())
                    .collect(),
            )),
            // All accounts except the current posting's account
            "other_accounts" => {
                let current = &posting.account;
                Ok(Value::StringSet(
                    ctx.transaction
                        .postings
                        .iter()
                        .filter(|p| &p.account != current)
                        .map(|p| p.account.to_string())
                        .collect(),
                ))
            }
            // Posting metadata as dictionary
            "meta" => Ok(Value::Metadata(posting.meta.clone())),
            // Source location columns
            "filename" => {
                if let Some(idx) = ctx.directive_index {
                    if let Some(loc) = self.get_source_location(idx) {
                        Ok(Value::String(loc.filename.clone()))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    Ok(Value::Null)
                }
            }
            "lineno" => {
                if let Some(idx) = ctx.directive_index {
                    if let Some(loc) = self.get_source_location(idx) {
                        Ok(Value::Integer(loc.lineno as i64))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    Ok(Value::Null)
                }
            }
            "location" => {
                if let Some(idx) = ctx.directive_index {
                    if let Some(loc) = self.get_source_location(idx) {
                        Ok(Value::String(format!("{}:{}", loc.filename, loc.lineno)))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    Ok(Value::Null)
                }
            }
            // has_cost - check if posting has cost specification
            "has_cost" => Ok(Value::Boolean(posting.cost.is_some())),
            // entry - parent transaction as structured object
            "entry" => {
                let txn = ctx.transaction;
                let mut obj = BTreeMap::new();
                obj.insert("date".to_string(), Value::Date(txn.date));
                obj.insert("flag".to_string(), Value::String(txn.flag.to_string()));
                if let Some(ref payee) = txn.payee {
                    obj.insert("payee".to_string(), Value::String(payee.to_string()));
                }
                obj.insert(
                    "narration".to_string(),
                    Value::String(txn.narration.to_string()),
                );
                obj.insert(
                    "tags".to_string(),
                    Value::StringSet(txn.tags.iter().map(ToString::to_string).collect()),
                );
                obj.insert(
                    "links".to_string(),
                    Value::StringSet(txn.links.iter().map(ToString::to_string).collect()),
                );
                // Include transaction metadata
                let mut meta_obj = BTreeMap::new();
                for (k, v) in &txn.meta {
                    meta_obj.insert(k.clone(), Self::meta_value_to_value(v));
                }
                obj.insert("meta".to_string(), Value::Object(meta_obj));
                Ok(Value::Object(obj))
            }
            _ => Err(QueryError::UnknownColumn(name.to_string())),
        }
    }

    /// Convert a `MetaValue` to a `Value`.
    fn meta_value_to_value(mv: &rustledger_core::MetaValue) -> Value {
        use rustledger_core::MetaValue;
        match mv {
            MetaValue::String(s) => Value::String(s.clone()),
            MetaValue::Number(n) => Value::Number(*n),
            MetaValue::Bool(b) => Value::Boolean(*b),
            MetaValue::Date(d) => Value::Date(*d),
            MetaValue::Currency(c) => Value::String(c.clone()),
            MetaValue::Amount(a) => Value::Amount(a.clone()),
            MetaValue::Account(a) => Value::String(a.clone()),
            MetaValue::Tag(t) => Value::String(t.clone()),
            MetaValue::Link(l) => Value::String(l.clone()),
            MetaValue::None => Value::Null,
        }
    }

    /// Evaluate a literal.
    fn evaluate_literal(&self, lit: &Literal) -> Result<Value, QueryError> {
        Ok(match lit {
            Literal::String(s) => Value::String(s.clone()),
            Literal::Number(n) => Value::Number(*n),
            Literal::Integer(i) => Value::Integer(*i),
            Literal::Date(d) => Value::Date(*d),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
        })
    }

    /// Evaluate a function call.
    ///
    /// Dispatches to specialized helper methods based on function category.
    fn evaluate_function(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        let name = func.name.to_uppercase();
        match name.as_str() {
            // Date functions
            "YEAR" | "MONTH" | "DAY" | "WEEKDAY" | "QUARTER" | "YMONTH" | "TODAY" => {
                self.eval_date_function(&name, func, ctx)
            }
            // Extended date functions
            "DATE" | "DATE_DIFF" | "DATE_ADD" | "DATE_TRUNC" | "DATE_PART" | "PARSE_DATE"
            | "DATE_BIN" | "INTERVAL" => self.eval_extended_date_function(&name, func, ctx),
            // String functions
            "LENGTH" | "UPPER" | "LOWER" | "SUBSTR" | "SUBSTRING" | "TRIM" | "STARTSWITH"
            | "ENDSWITH" | "GREP" | "GREPN" | "SUBST" | "SPLITCOMP" | "JOINSTR" | "MAXWIDTH" => {
                self.eval_string_function(&name, func, ctx)
            }
            // Account functions
            "PARENT" | "LEAF" | "ROOT" | "ACCOUNT_DEPTH" | "ACCOUNT_SORTKEY" => {
                self.eval_account_function(&name, func, ctx)
            }
            // Account metadata functions
            "OPEN_DATE" | "CLOSE_DATE" | "OPEN_META" => {
                self.eval_account_meta_function(&name, func, ctx)
            }
            // Math functions
            "ABS" | "NEG" | "ROUND" | "SAFEDIV" => self.eval_math_function(&name, func, ctx),
            // Amount/Position functions
            "NUMBER" | "CURRENCY" | "GETITEM" | "GET" | "UNITS" | "COST" | "WEIGHT" | "VALUE" => {
                self.eval_position_function(&name, func, ctx)
            }
            // Inventory functions
            "EMPTY" | "FILTER_CURRENCY" | "POSSIGN" => {
                self.eval_inventory_function(&name, func, ctx)
            }
            // Price functions
            "GETPRICE" => self.eval_getprice(func, ctx),
            // Utility functions
            "COALESCE" => self.eval_coalesce(func, ctx),
            "ONLY" => self.eval_only(func, ctx),
            // Metadata functions
            "META" | "ENTRY_META" | "ANY_META" | "POSTING_META" => {
                self.eval_meta_function(&name, func, ctx)
            }
            // Currency conversion
            "CONVERT" => self.eval_convert(func, ctx),
            // Type casting functions
            "INT" => self.eval_int(func, ctx),
            "DECIMAL" => self.eval_decimal(func, ctx),
            "STR" => self.eval_str(func, ctx),
            "BOOL" => self.eval_bool(func, ctx),
            // Aggregate functions return Null when evaluated on a single row
            // They're handled specially in aggregate evaluation
            "SUM" | "COUNT" | "MIN" | "MAX" | "FIRST" | "LAST" | "AVG" => Ok(Value::Null),
            _ => Err(QueryError::UnknownFunction(func.name.clone())),
        }
    }

    /// Evaluate date functions: `YEAR`, `MONTH`, `DAY`, `WEEKDAY`, `QUARTER`, `YMONTH`, `TODAY`.
    fn eval_date_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        if name == "TODAY" {
            if !func.args.is_empty() {
                return Err(QueryError::InvalidArguments(
                    "TODAY".to_string(),
                    "expected 0 arguments".to_string(),
                ));
            }
            return Ok(Value::Date(chrono::Local::now().date_naive()));
        }

        // All other date functions expect exactly 1 argument
        if func.args.len() != 1 {
            return Err(QueryError::InvalidArguments(
                name.to_string(),
                "expected 1 argument".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let date = match val {
            Value::Date(d) => d,
            _ => return Err(QueryError::Type(format!("{name} expects a date"))),
        };

        match name {
            "YEAR" => Ok(Value::Integer(date.year().into())),
            "MONTH" => Ok(Value::Integer(date.month().into())),
            "DAY" => Ok(Value::Integer(date.day().into())),
            "WEEKDAY" => Ok(Value::Integer(date.weekday().num_days_from_monday().into())),
            "QUARTER" => {
                let quarter = (date.month() - 1) / 3 + 1;
                Ok(Value::Integer(quarter.into()))
            }
            "YMONTH" => Ok(Value::String(format!(
                "{:04}-{:02}",
                date.year(),
                date.month()
            ))),
            _ => unreachable!(),
        }
    }

    /// Evaluate extended date functions.
    fn eval_extended_date_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "DATE" => self.eval_date_construct(func, ctx),
            "DATE_DIFF" => self.eval_date_diff(func, ctx),
            "DATE_ADD" => self.eval_date_add(func, ctx),
            "DATE_TRUNC" => self.eval_date_trunc(func, ctx),
            "DATE_PART" => self.eval_date_part(func, ctx),
            "PARSE_DATE" => self.eval_parse_date(func, ctx),
            "DATE_BIN" => self.eval_date_bin(func, ctx),
            "INTERVAL" => self.eval_interval(func, ctx),
            _ => unreachable!(),
        }
    }

    /// Evaluate INTERVAL function (construct an interval).
    fn eval_interval(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        // interval(unit) - creates an interval of 1 unit
        // interval(count, unit) - creates an interval of count units
        match func.args.len() {
            1 => {
                let unit_str = match self.evaluate_expr(&func.args[0], ctx)? {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "interval() unit must be a string".to_string(),
                        ));
                    }
                };
                let unit = IntervalUnit::parse_unit(&unit_str).ok_or_else(|| {
                    QueryError::InvalidArguments(
                        "INTERVAL".to_string(),
                        format!("invalid interval unit: {unit_str}"),
                    )
                })?;
                Ok(Value::Interval(Interval::new(1, unit)))
            }
            2 => {
                let count = match self.evaluate_expr(&func.args[0], ctx)? {
                    Value::Integer(n) => n,
                    Value::Number(d) => {
                        use rust_decimal::prelude::ToPrimitive;
                        // Reject decimals with fractional parts
                        if !d.fract().is_zero() {
                            return Err(QueryError::Type(
                                "interval() count must be an integer".to_string(),
                            ));
                        }
                        d.to_i64().ok_or_else(|| {
                            QueryError::Type("interval() count must be an integer".to_string())
                        })?
                    }
                    _ => {
                        return Err(QueryError::Type(
                            "interval() count must be a number".to_string(),
                        ));
                    }
                };
                let unit_str = match self.evaluate_expr(&func.args[1], ctx)? {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "interval() unit must be a string".to_string(),
                        ));
                    }
                };
                let unit = IntervalUnit::parse_unit(&unit_str).ok_or_else(|| {
                    QueryError::InvalidArguments(
                        "INTERVAL".to_string(),
                        format!("invalid interval unit: {unit_str}"),
                    )
                })?;
                Ok(Value::Interval(Interval::new(count, unit)))
            }
            _ => Err(QueryError::InvalidArguments(
                "INTERVAL".to_string(),
                "expected 1 or 2 arguments".to_string(),
            )),
        }
    }

    /// Evaluate DATE function (construct a date).
    ///
    /// `DATE(year, month, day)` - construct from components
    /// `DATE(string)` - parse ISO date string
    fn eval_date_construct(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match func.args.len() {
            1 => {
                // DATE(string) - parse ISO date
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                        .map(Value::Date)
                        .map_err(|_| QueryError::Type(format!("DATE: cannot parse '{s}' as date"))),
                    Value::Date(d) => Ok(Value::Date(d)),
                    _ => Err(QueryError::Type(
                        "DATE: argument must be a string or date".to_string(),
                    )),
                }
            }
            3 => {
                // DATE(year, month, day)
                let year = match self.evaluate_expr(&func.args[0], ctx)? {
                    Value::Integer(i) => i as i32,
                    Value::Number(n) => {
                        use rust_decimal::prelude::ToPrimitive;
                        n.to_i32().ok_or_else(|| {
                            QueryError::Type("DATE: year must be an integer".to_string())
                        })?
                    }
                    _ => {
                        return Err(QueryError::Type(
                            "DATE: year must be an integer".to_string(),
                        ));
                    }
                };
                let month = match self.evaluate_expr(&func.args[1], ctx)? {
                    Value::Integer(i) => i as u32,
                    Value::Number(n) => {
                        use rust_decimal::prelude::ToPrimitive;
                        n.to_u32().ok_or_else(|| {
                            QueryError::Type("DATE: month must be an integer".to_string())
                        })?
                    }
                    _ => {
                        return Err(QueryError::Type(
                            "DATE: month must be an integer".to_string(),
                        ));
                    }
                };
                let day = match self.evaluate_expr(&func.args[2], ctx)? {
                    Value::Integer(i) => i as u32,
                    Value::Number(n) => {
                        use rust_decimal::prelude::ToPrimitive;
                        n.to_u32().ok_or_else(|| {
                            QueryError::Type("DATE: day must be an integer".to_string())
                        })?
                    }
                    _ => return Err(QueryError::Type("DATE: day must be an integer".to_string())),
                };
                NaiveDate::from_ymd_opt(year, month, day)
                    .map(Value::Date)
                    .ok_or_else(|| {
                        QueryError::Type(format!("DATE: invalid date {year}-{month}-{day}"))
                    })
            }
            _ => Err(QueryError::InvalidArguments(
                "DATE".to_string(),
                "expected 1 or 3 arguments".to_string(),
            )),
        }
    }

    /// Evaluate `DATE_DIFF` function (difference in days).
    ///
    /// `DATE_DIFF(date1, date2)` - returns date1 - date2 in days
    fn eval_date_diff(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("DATE_DIFF", func, 2)?;

        let date1 = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_DIFF: first argument must be a date".to_string(),
                ));
            }
        };
        let date2 = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_DIFF: second argument must be a date".to_string(),
                ));
            }
        };

        let diff = date1.signed_duration_since(date2).num_days();
        Ok(Value::Integer(diff))
    }

    /// Evaluate `DATE_ADD` function (add days or interval to a date).
    ///
    /// `DATE_ADD(date, days)` - returns date + days
    /// `DATE_ADD(date, interval)` - returns date + interval
    fn eval_date_add(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("DATE_ADD", func, 2)?;

        let date = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_ADD: first argument must be a date".to_string(),
                ));
            }
        };

        let second_arg = self.evaluate_expr(&func.args[1], ctx)?;
        let result = match second_arg {
            Value::Integer(days) => date + chrono::Duration::days(days),
            Value::Number(n) => {
                use rust_decimal::prelude::ToPrimitive;
                let days = n.to_i64().ok_or_else(|| {
                    QueryError::Type("DATE_ADD: days must be an integer".to_string())
                })?;
                date + chrono::Duration::days(days)
            }
            Value::Interval(interval) => interval
                .add_to_date(date)
                .ok_or_else(|| QueryError::Evaluation("DATE_ADD: interval overflow".to_string()))?,
            _ => {
                return Err(QueryError::Type(
                    "DATE_ADD: second argument must be an integer or interval".to_string(),
                ));
            }
        };

        Ok(Value::Date(result))
    }

    /// Evaluate `DATE_TRUNC` function (truncate date to field).
    ///
    /// `DATE_TRUNC(field, date)` - truncate to year/month
    fn eval_date_trunc(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("DATE_TRUNC", func, 2)?;

        let field = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s.to_uppercase(),
            _ => {
                return Err(QueryError::Type(
                    "DATE_TRUNC: first argument must be a string".to_string(),
                ));
            }
        };
        let date = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_TRUNC: second argument must be a date".to_string(),
                ));
            }
        };

        let result = match field.as_str() {
            "YEAR" => NaiveDate::from_ymd_opt(date.year(), 1, 1),
            "QUARTER" => {
                let quarter = (date.month() - 1) / 3;
                NaiveDate::from_ymd_opt(date.year(), quarter * 3 + 1, 1)
            }
            "MONTH" => NaiveDate::from_ymd_opt(date.year(), date.month(), 1),
            "WEEK" => {
                // Start of week (Monday)
                let days_from_monday = i64::from(date.weekday().num_days_from_monday());
                Some(date - chrono::Duration::days(days_from_monday))
            }
            "DAY" => Some(date),
            _ => {
                return Err(QueryError::Type(format!(
                    "DATE_TRUNC: unknown field '{field}', expected YEAR, QUARTER, MONTH, WEEK, or DAY"
                )));
            }
        };

        result
            .map(Value::Date)
            .ok_or_else(|| QueryError::Type("DATE_TRUNC: invalid date result".to_string()))
    }

    /// Evaluate `DATE_PART` function (extract date component).
    ///
    /// `DATE_PART(field, date)` - extract component
    fn eval_date_part(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("DATE_PART", func, 2)?;

        let field = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s.to_uppercase(),
            _ => {
                return Err(QueryError::Type(
                    "DATE_PART: first argument must be a string".to_string(),
                ));
            }
        };
        let date = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_PART: second argument must be a date".to_string(),
                ));
            }
        };

        let result = match field.as_str() {
            "YEAR" => i64::from(date.year()),
            "MONTH" => i64::from(date.month()),
            "DAY" => i64::from(date.day()),
            "QUARTER" => i64::from((date.month() - 1) / 3 + 1),
            "WEEK" => i64::from(date.iso_week().week()),
            "WEEKDAY" | "DOW" => i64::from(date.weekday().num_days_from_monday()),
            "DOY" => i64::from(date.ordinal()),
            _ => {
                return Err(QueryError::Type(format!(
                    "DATE_PART: unknown field '{field}', expected YEAR, MONTH, DAY, QUARTER, WEEK, WEEKDAY, DOW, or DOY"
                )));
            }
        };

        Ok(Value::Integer(result))
    }

    /// Evaluate `PARSE_DATE` function (parse date with format).
    ///
    /// `PARSE_DATE(string, format)` - parse with chrono format
    fn eval_parse_date(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("PARSE_DATE", func, 2)?;

        let string = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "PARSE_DATE: first argument must be a string".to_string(),
                ));
            }
        };
        let format = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "PARSE_DATE: second argument must be a format string".to_string(),
                ));
            }
        };

        NaiveDate::parse_from_str(&string, &format)
            .map(Value::Date)
            .map_err(|e| {
                QueryError::Type(format!(
                    "PARSE_DATE: cannot parse '{string}' with format '{format}': {e}"
                ))
            })
    }

    /// Evaluate `DATE_BIN` function (bin dates into buckets).
    ///
    /// `DATE_BIN(stride, source, origin)` - bins source date into buckets of stride size
    /// starting from origin.
    ///
    /// Stride is a string like "1 day", "7 days", "1 week", "1 month", "3 months", "1 year".
    fn eval_date_bin(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("DATE_BIN", func, 3)?;

        let stride = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            Value::Integer(days) => format!("{days} days"),
            _ => {
                return Err(QueryError::Type(
                    "DATE_BIN: first argument must be a stride string or integer days".to_string(),
                ));
            }
        };

        let source = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_BIN: second argument must be a date".to_string(),
                ));
            }
        };

        let origin = match self.evaluate_expr(&func.args[2], ctx)? {
            Value::Date(d) => d,
            _ => {
                return Err(QueryError::Type(
                    "DATE_BIN: third argument must be a date".to_string(),
                ));
            }
        };

        // Parse stride string
        let stride_lower = stride.to_lowercase();
        let parts: Vec<&str> = stride_lower.split_whitespace().collect();

        let (amount, unit) = match parts.as_slice() {
            [num, unit] => {
                let n: i64 = num.parse().map_err(|_| {
                    QueryError::Type(format!("DATE_BIN: invalid stride number '{num}'"))
                })?;
                (n, *unit)
            }
            [unit] => (1, *unit),
            _ => {
                return Err(QueryError::Type(format!(
                    "DATE_BIN: invalid stride format '{stride}'"
                )));
            }
        };

        // Calculate days from origin to source
        let days_diff = (source - origin).num_days();

        // Calculate binned date based on unit
        let binned = match unit.trim_end_matches('s') {
            "day" => {
                let bucket = days_diff / amount;
                origin + chrono::Duration::days(bucket * amount)
            }
            "week" => {
                let days_per_stride = amount * 7;
                let bucket = days_diff / days_per_stride;
                origin + chrono::Duration::days(bucket * days_per_stride)
            }
            "month" => {
                // For months, we need to work with calendar months
                let months_diff = (source.year() - origin.year()) * 12 + (source.month() as i32)
                    - (origin.month() as i32);
                let bucket = months_diff / (amount as i32);
                let total_months = (origin.month() as i32) - 1 + bucket * (amount as i32);
                let year = origin.year() + total_months / 12;
                let month = (total_months % 12 + 1) as u32;
                NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(origin)
            }
            "quarter" => {
                // 3-month buckets
                let months_diff = (source.year() - origin.year()) * 12 + (source.month() as i32)
                    - (origin.month() as i32);
                let quarters = months_diff / (3 * amount as i32);
                let total_months = (origin.month() as i32) - 1 + quarters * 3 * (amount as i32);
                let year = origin.year() + total_months / 12;
                let month = (total_months % 12 + 1) as u32;
                NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(origin)
            }
            "year" => {
                let years_diff = source.year() - origin.year();
                let bucket = years_diff / (amount as i32);
                let year = origin.year() + bucket * (amount as i32);
                NaiveDate::from_ymd_opt(year, origin.month(), origin.day()).unwrap_or(origin)
            }
            _ => {
                return Err(QueryError::Type(format!(
                    "DATE_BIN: unknown unit '{unit}', expected day(s), week(s), month(s), quarter(s), or year(s)"
                )));
            }
        };

        Ok(Value::Date(binned))
    }

    /// Evaluate string functions: `LENGTH`, `UPPER`, `LOWER`, `SUBSTR`, `TRIM`, `STARTSWITH`, `ENDSWITH`.
    fn eval_string_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "LENGTH" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::Integer(s.len() as i64)),
                    Value::StringSet(s) => Ok(Value::Integer(s.len() as i64)),
                    _ => Err(QueryError::Type(
                        "LENGTH expects a string or set".to_string(),
                    )),
                }
            }
            "UPPER" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::String(s.to_uppercase())),
                    _ => Err(QueryError::Type("UPPER expects a string".to_string())),
                }
            }
            "LOWER" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::String(s.to_lowercase())),
                    _ => Err(QueryError::Type("LOWER expects a string".to_string())),
                }
            }
            "TRIM" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::String(s.trim().to_string())),
                    _ => Err(QueryError::Type("TRIM expects a string".to_string())),
                }
            }
            "SUBSTR" | "SUBSTRING" => self.eval_substr(func, ctx),
            "STARTSWITH" => {
                Self::require_args(name, func, 2)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                let prefix = self.evaluate_expr(&func.args[1], ctx)?;
                match (val, prefix) {
                    (Value::String(s), Value::String(p)) => Ok(Value::Boolean(s.starts_with(&p))),
                    _ => Err(QueryError::Type(
                        "STARTSWITH expects two strings".to_string(),
                    )),
                }
            }
            "ENDSWITH" => {
                Self::require_args(name, func, 2)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                let suffix = self.evaluate_expr(&func.args[1], ctx)?;
                match (val, suffix) {
                    (Value::String(s), Value::String(p)) => Ok(Value::Boolean(s.ends_with(&p))),
                    _ => Err(QueryError::Type("ENDSWITH expects two strings".to_string())),
                }
            }
            "GREP" => self.eval_grep(func, ctx),
            "GREPN" => self.eval_grepn(func, ctx),
            "SUBST" => self.eval_subst(func, ctx),
            "SPLITCOMP" => self.eval_splitcomp(func, ctx),
            "JOINSTR" => self.eval_joinstr(func, ctx),
            "MAXWIDTH" => self.eval_maxwidth(func, ctx),
            _ => unreachable!(),
        }
    }

    /// Evaluate GREP function (regex match).
    ///
    /// `GREP(pattern, string)` - Return matched portion or null
    fn eval_grep(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("GREP", func, 2)?;

        let pattern = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GREP: first argument must be a pattern string".to_string(),
                ));
            }
        };
        let string = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GREP: second argument must be a string".to_string(),
                ));
            }
        };

        let re = Regex::new(&pattern)
            .map_err(|e| QueryError::Type(format!("GREP: invalid regex '{pattern}': {e}")))?;

        match re.find(&string) {
            Some(m) => Ok(Value::String(m.as_str().to_string())),
            None => Ok(Value::Null),
        }
    }

    /// Evaluate GREPN function (regex capture group).
    ///
    /// `GREPN(pattern, string, n)` - Return nth capture group
    fn eval_grepn(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("GREPN", func, 3)?;

        let pattern = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GREPN: first argument must be a pattern string".to_string(),
                ));
            }
        };
        let string = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GREPN: second argument must be a string".to_string(),
                ));
            }
        };
        let n = match self.evaluate_expr(&func.args[2], ctx)? {
            Value::Integer(i) => i as usize,
            Value::Number(n) => {
                use rust_decimal::prelude::ToPrimitive;
                n.to_usize().ok_or_else(|| {
                    QueryError::Type("GREPN: third argument must be a non-negative integer".into())
                })?
            }
            _ => {
                return Err(QueryError::Type(
                    "GREPN: third argument must be an integer".to_string(),
                ));
            }
        };

        let re = Regex::new(&pattern)
            .map_err(|e| QueryError::Type(format!("GREPN: invalid regex '{pattern}': {e}")))?;

        match re.captures(&string) {
            Some(caps) => match caps.get(n) {
                Some(m) => Ok(Value::String(m.as_str().to_string())),
                None => Ok(Value::Null),
            },
            None => Ok(Value::Null),
        }
    }

    /// Evaluate SUBST function (regex substitution).
    ///
    /// `SUBST(pattern, replacement, string)` - Replace matches with replacement
    fn eval_subst(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("SUBST", func, 3)?;

        let pattern = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "SUBST: first argument must be a pattern string".to_string(),
                ));
            }
        };
        let replacement = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "SUBST: second argument must be a replacement string".to_string(),
                ));
            }
        };
        let string = match self.evaluate_expr(&func.args[2], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "SUBST: third argument must be a string".to_string(),
                ));
            }
        };

        let re = Regex::new(&pattern)
            .map_err(|e| QueryError::Type(format!("SUBST: invalid regex '{pattern}': {e}")))?;

        Ok(Value::String(
            re.replace_all(&string, &replacement).to_string(),
        ))
    }

    /// Evaluate SPLITCOMP function (split and get component).
    ///
    /// `SPLITCOMP(string, delimiter, n)` - Split and return nth component (0-based)
    fn eval_splitcomp(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("SPLITCOMP", func, 3)?;

        let string = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "SPLITCOMP: first argument must be a string".to_string(),
                ));
            }
        };
        let delimiter = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "SPLITCOMP: second argument must be a delimiter string".to_string(),
                ));
            }
        };
        let n = match self.evaluate_expr(&func.args[2], ctx)? {
            Value::Integer(i) => i as usize,
            Value::Number(n) => {
                use rust_decimal::prelude::ToPrimitive;
                n.to_usize().ok_or_else(|| {
                    QueryError::Type(
                        "SPLITCOMP: third argument must be a non-negative integer".into(),
                    )
                })?
            }
            _ => {
                return Err(QueryError::Type(
                    "SPLITCOMP: third argument must be an integer".to_string(),
                ));
            }
        };

        let parts: Vec<&str> = string.split(&delimiter).collect();
        match parts.get(n) {
            Some(part) => Ok(Value::String((*part).to_string())),
            None => Ok(Value::Null),
        }
    }

    /// Evaluate JOINSTR function (join values with separator).
    ///
    /// `JOINSTR(value, ...)` - Join multiple values with comma separator
    fn eval_joinstr(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.is_empty() {
            return Err(QueryError::InvalidArguments(
                "JOINSTR".to_string(),
                "expected at least 1 argument".to_string(),
            ));
        }

        let mut parts = Vec::new();
        for arg in &func.args {
            let val = self.evaluate_expr(arg, ctx)?;
            match val {
                Value::String(s) => parts.push(s),
                Value::StringSet(ss) => parts.extend(ss),
                Value::Null => {} // Skip nulls
                other => parts.push(self.value_to_string(&other)),
            }
        }

        Ok(Value::String(parts.join(", ")))
    }

    /// Evaluate MAXWIDTH function (truncate with ellipsis).
    ///
    /// `MAXWIDTH(string, n)` - Truncate string to n characters with ellipsis
    fn eval_maxwidth(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args("MAXWIDTH", func, 2)?;

        let string = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "MAXWIDTH: first argument must be a string".to_string(),
                ));
            }
        };
        let n = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Integer(i) => i as usize,
            Value::Number(n) => {
                use rust_decimal::prelude::ToPrimitive;
                n.to_usize().ok_or_else(|| {
                    QueryError::Type("MAXWIDTH: second argument must be a positive integer".into())
                })?
            }
            _ => {
                return Err(QueryError::Type(
                    "MAXWIDTH: second argument must be an integer".to_string(),
                ));
            }
        };

        if string.chars().count() <= n {
            Ok(Value::String(string))
        } else if n <= 3 {
            Ok(Value::String(string.chars().take(n).collect()))
        } else {
            let truncated: String = string.chars().take(n - 3).collect();
            Ok(Value::String(format!("{truncated}...")))
        }
    }

    /// Evaluate SUBSTR/SUBSTRING function.
    fn eval_substr(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.len() < 2 || func.args.len() > 3 {
            return Err(QueryError::InvalidArguments(
                "SUBSTR".to_string(),
                "expected 2 or 3 arguments".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let start = self.evaluate_expr(&func.args[1], ctx)?;
        let len = if func.args.len() == 3 {
            Some(self.evaluate_expr(&func.args[2], ctx)?)
        } else {
            None
        };

        match (val, start, len) {
            (Value::String(s), Value::Integer(start), None) => {
                let start = start.max(0) as usize;
                if start >= s.len() {
                    Ok(Value::String(String::new()))
                } else {
                    Ok(Value::String(s[start..].to_string()))
                }
            }
            (Value::String(s), Value::Integer(start), Some(Value::Integer(len))) => {
                let start = start.max(0) as usize;
                let len = len.max(0) as usize;
                if start >= s.len() {
                    Ok(Value::String(String::new()))
                } else {
                    let end = (start + len).min(s.len());
                    Ok(Value::String(s[start..end].to_string()))
                }
            }
            _ => Err(QueryError::Type(
                "SUBSTR expects (string, int, [int])".to_string(),
            )),
        }
    }

    /// Evaluate account functions: `PARENT`, `LEAF`, `ROOT`, `ACCOUNT_DEPTH`, `ACCOUNT_SORTKEY`.
    fn eval_account_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "PARENT" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        if let Some(idx) = s.rfind(':') {
                            Ok(Value::String(s[..idx].to_string()))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "PARENT expects an account string".to_string(),
                    )),
                }
            }
            "LEAF" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        if let Some(idx) = s.rfind(':') {
                            Ok(Value::String(s[idx + 1..].to_string()))
                        } else {
                            Ok(Value::String(s))
                        }
                    }
                    _ => Err(QueryError::Type(
                        "LEAF expects an account string".to_string(),
                    )),
                }
            }
            "ROOT" => self.eval_root(func, ctx),
            "ACCOUNT_DEPTH" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        let depth = s.chars().filter(|c| *c == ':').count() + 1;
                        Ok(Value::Integer(depth as i64))
                    }
                    _ => Err(QueryError::Type(
                        "ACCOUNT_DEPTH expects an account string".to_string(),
                    )),
                }
            }
            "ACCOUNT_SORTKEY" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::String(s)),
                    _ => Err(QueryError::Type(
                        "ACCOUNT_SORTKEY expects an account string".to_string(),
                    )),
                }
            }
            _ => unreachable!(),
        }
    }

    /// Evaluate account metadata functions: `OPEN_DATE`, `CLOSE_DATE`, `OPEN_META`.
    fn eval_account_meta_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "OPEN_DATE" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(account) => {
                        if let Some(info) = self.account_info.get(&account) {
                            Ok(info.open_date.map_or(Value::Null, Value::Date))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "OPEN_DATE expects an account string".to_string(),
                    )),
                }
            }
            "CLOSE_DATE" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(account) => {
                        if let Some(info) = self.account_info.get(&account) {
                            Ok(info.close_date.map_or(Value::Null, Value::Date))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "CLOSE_DATE expects an account string".to_string(),
                    )),
                }
            }
            "OPEN_META" => {
                Self::require_args(name, func, 2)?;
                let account_val = self.evaluate_expr(&func.args[0], ctx)?;
                let key_val = self.evaluate_expr(&func.args[1], ctx)?;

                let (account, key) = match (account_val, key_val) {
                    (Value::String(a), Value::String(k)) => (a, k),
                    _ => {
                        return Err(QueryError::Type(
                            "OPEN_META expects (account_string, key_string)".to_string(),
                        ));
                    }
                };

                if let Some(info) = self.account_info.get(&account) {
                    let meta_value = info.open_meta.get(&key);
                    Ok(Self::meta_value_to_value(meta_value))
                } else {
                    Ok(Value::Null)
                }
            }
            _ => unreachable!(),
        }
    }

    /// Evaluate ROOT function (takes 1-2 arguments).
    fn eval_root(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.is_empty() || func.args.len() > 2 {
            return Err(QueryError::InvalidArguments(
                "ROOT".to_string(),
                "expected 1 or 2 arguments".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let n = if func.args.len() == 2 {
            match self.evaluate_expr(&func.args[1], ctx)? {
                Value::Integer(i) => i as usize,
                _ => {
                    return Err(QueryError::Type(
                        "ROOT second arg must be integer".to_string(),
                    ));
                }
            }
        } else {
            1
        };

        match val {
            Value::String(s) => {
                let parts: Vec<&str> = s.split(':').collect();
                if n >= parts.len() {
                    Ok(Value::String(s))
                } else {
                    Ok(Value::String(parts[..n].join(":")))
                }
            }
            _ => Err(QueryError::Type(
                "ROOT expects an account string".to_string(),
            )),
        }
    }

    /// Evaluate math functions: `ABS`, `NEG`, `ROUND`, `SAFEDIV`.
    fn eval_math_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "ABS" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::Number(n) => Ok(Value::Number(n.abs())),
                    Value::Integer(i) => Ok(Value::Integer(i.abs())),
                    _ => Err(QueryError::Type("ABS expects a number".to_string())),
                }
            }
            "NEG" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::Number(n) => Ok(Value::Number(-n)),
                    Value::Integer(i) => Ok(Value::Integer(-i)),
                    _ => Err(QueryError::Type("NEG expects a number".to_string())),
                }
            }
            "ROUND" => self.eval_round(func, ctx),
            "SAFEDIV" => self.eval_safediv(func, ctx),
            _ => unreachable!(),
        }
    }

    /// Evaluate ROUND function (takes 1-2 arguments).
    fn eval_round(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.is_empty() || func.args.len() > 2 {
            return Err(QueryError::InvalidArguments(
                "ROUND".to_string(),
                "expected 1 or 2 arguments".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let decimals = if func.args.len() == 2 {
            match self.evaluate_expr(&func.args[1], ctx)? {
                Value::Integer(i) => i as u32,
                _ => {
                    return Err(QueryError::Type(
                        "ROUND second arg must be integer".to_string(),
                    ));
                }
            }
        } else {
            0
        };

        match val {
            Value::Number(n) => Ok(Value::Number(n.round_dp(decimals))),
            Value::Integer(i) => Ok(Value::Integer(i)),
            _ => Err(QueryError::Type("ROUND expects a number".to_string())),
        }
    }

    /// Evaluate SAFEDIV function.
    fn eval_safediv(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("SAFEDIV", func, 2)?;
        let num = self.evaluate_expr(&func.args[0], ctx)?;
        let den = self.evaluate_expr(&func.args[1], ctx)?;

        match (num, den) {
            (Value::Number(n), Value::Number(d)) => {
                if d.is_zero() {
                    Ok(Value::Number(Decimal::ZERO))
                } else {
                    Ok(Value::Number(n / d))
                }
            }
            (Value::Integer(n), Value::Integer(d)) => {
                if d == 0 {
                    Ok(Value::Integer(0))
                } else {
                    Ok(Value::Integer(n / d))
                }
            }
            _ => Err(QueryError::Type("SAFEDIV expects two numbers".to_string())),
        }
    }

    /// Evaluate position/amount functions: `NUMBER`, `CURRENCY`, `GETITEM`, `UNITS`, `COST`, `WEIGHT`, `VALUE`.
    fn eval_position_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "NUMBER" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::Amount(a) => Ok(Value::Number(a.number)),
                    Value::Position(p) => Ok(Value::Number(p.units.number)),
                    Value::Number(n) => Ok(Value::Number(n)),
                    Value::Integer(i) => Ok(Value::Number(Decimal::from(i))),
                    _ => Err(QueryError::Type(
                        "NUMBER expects an amount or position".to_string(),
                    )),
                }
            }
            "CURRENCY" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::Amount(a) => Ok(Value::String(a.currency.to_string())),
                    Value::Position(p) => Ok(Value::String(p.units.currency.to_string())),
                    _ => Err(QueryError::Type(
                        "CURRENCY expects an amount or position".to_string(),
                    )),
                }
            }
            "GETITEM" | "GET" => self.eval_getitem(func, ctx),
            "UNITS" => self.eval_units(func, ctx),
            "COST" => self.eval_cost(func, ctx),
            "WEIGHT" => self.eval_weight(func, ctx),
            "VALUE" => self.eval_value(func, ctx),
            _ => unreachable!(),
        }
    }

    /// Evaluate inventory functions: `EMPTY`, `FILTER_CURRENCY`, `POSSIGN`.
    fn eval_inventory_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "EMPTY" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::Inventory(inv) => Ok(Value::Boolean(inv.is_empty())),
                    Value::Null => Ok(Value::Boolean(true)),
                    _ => Err(QueryError::Type("EMPTY expects an inventory".to_string())),
                }
            }
            "FILTER_CURRENCY" => {
                Self::require_args(name, func, 2)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                let currency = self.evaluate_expr(&func.args[1], ctx)?;

                match (val, currency) {
                    (Value::Inventory(inv), Value::String(curr)) => {
                        let filtered: Vec<Position> = inv
                            .positions()
                            .iter()
                            .filter(|p| p.units.currency.as_str() == curr)
                            .cloned()
                            .collect();
                        let mut new_inv = Inventory::new();
                        for pos in filtered {
                            new_inv.add(pos);
                        }
                        Ok(Value::Inventory(new_inv))
                    }
                    (Value::Null, _) => Ok(Value::Null),
                    _ => Err(QueryError::Type(
                        "FILTER_CURRENCY expects (inventory, string)".to_string(),
                    )),
                }
            }
            "POSSIGN" => {
                Self::require_args(name, func, 2)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                let account = self.evaluate_expr(&func.args[1], ctx)?;

                let account_str = match account {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "POSSIGN expects (amount, account_string)".to_string(),
                        ));
                    }
                };

                // Determine if account is credit-normal (Liabilities, Equity, Income)
                // These need their signs inverted; Assets/Expenses are debit-normal
                let first_component = account_str.split(':').next().unwrap_or("");
                let is_credit_normal =
                    matches!(first_component, "Liabilities" | "Equity" | "Income");

                match val {
                    Value::Amount(mut a) => {
                        if is_credit_normal {
                            a.number = -a.number;
                        }
                        Ok(Value::Amount(a))
                    }
                    Value::Number(n) => {
                        let adjusted = if is_credit_normal { -n } else { n };
                        Ok(Value::Number(adjusted))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(QueryError::Type(
                        "POSSIGN expects (amount, account_string)".to_string(),
                    )),
                }
            }
            _ => unreachable!(),
        }
    }

    /// Evaluate GETITEM/GET function.
    fn eval_getitem(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("GETITEM", func, 2)?;
        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let key = self.evaluate_expr(&func.args[1], ctx)?;

        match (val, key) {
            (Value::Inventory(inv), Value::String(currency)) => {
                let total = inv.units(&currency);
                if total.is_zero() {
                    Ok(Value::Null)
                } else {
                    Ok(Value::Amount(Amount::new(total, currency)))
                }
            }
            _ => Err(QueryError::Type(
                "GETITEM expects (inventory, string)".to_string(),
            )),
        }
    }

    /// Evaluate UNITS function.
    fn eval_units(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("UNITS", func, 1)?;
        let val = self.evaluate_expr(&func.args[0], ctx)?;

        match val {
            Value::Position(p) => Ok(Value::Amount(p.units)),
            Value::Amount(a) => Ok(Value::Amount(a)),
            Value::Inventory(inv) => {
                let positions: Vec<String> = inv
                    .positions()
                    .iter()
                    .map(|p| format!("{} {}", p.units.number, p.units.currency))
                    .collect();
                Ok(Value::String(positions.join(", ")))
            }
            _ => Err(QueryError::Type(
                "UNITS expects a position or inventory".to_string(),
            )),
        }
    }

    /// Evaluate COST function.
    fn eval_cost(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("COST", func, 1)?;
        let val = self.evaluate_expr(&func.args[0], ctx)?;

        match val {
            Value::Position(p) => {
                if let Some(cost) = &p.cost {
                    let total = p.units.number.abs() * cost.number;
                    Ok(Value::Amount(Amount::new(total, cost.currency.clone())))
                } else {
                    Ok(Value::Null)
                }
            }
            Value::Amount(a) => Ok(Value::Amount(a)),
            Value::Inventory(inv) => {
                let mut total = Decimal::ZERO;
                let mut currency: Option<InternedStr> = None;
                for pos in inv.positions() {
                    if let Some(cost) = &pos.cost {
                        total += pos.units.number.abs() * cost.number;
                        if currency.is_none() {
                            currency = Some(cost.currency.clone());
                        }
                    }
                }
                if let Some(curr) = currency {
                    Ok(Value::Amount(Amount::new(total, curr)))
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Err(QueryError::Type(
                "COST expects a position or inventory".to_string(),
            )),
        }
    }

    /// Evaluate WEIGHT function.
    fn eval_weight(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("WEIGHT", func, 1)?;
        let val = self.evaluate_expr(&func.args[0], ctx)?;

        match val {
            Value::Position(p) => {
                if let Some(cost) = &p.cost {
                    let total = p.units.number * cost.number;
                    Ok(Value::Amount(Amount::new(total, cost.currency.clone())))
                } else {
                    Ok(Value::Amount(p.units))
                }
            }
            Value::Amount(a) => Ok(Value::Amount(a)),
            _ => Err(QueryError::Type(
                "WEIGHT expects a position or amount".to_string(),
            )),
        }
    }

    /// Evaluate VALUE function (market value conversion).
    fn eval_value(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.is_empty() || func.args.len() > 2 {
            return Err(QueryError::InvalidArguments(
                "VALUE".to_string(),
                "expected 1-2 arguments".to_string(),
            ));
        }

        let target_currency = if func.args.len() == 2 {
            match self.evaluate_expr(&func.args[1], ctx)? {
                Value::String(s) => s,
                _ => {
                    return Err(QueryError::Type(
                        "VALUE second argument must be a currency string".to_string(),
                    ));
                }
            }
        } else {
            self.target_currency.clone().ok_or_else(|| {
                QueryError::InvalidArguments(
                    "VALUE".to_string(),
                    "no target currency set; either call set_target_currency() on the executor \
                     or pass the currency as VALUE(amount, 'USD')"
                        .to_string(),
                )
            })?
        };

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let date = ctx.transaction.date;

        match val {
            Value::Position(p) => {
                if p.units.currency == target_currency {
                    Ok(Value::Amount(p.units))
                } else if let Some(converted) =
                    self.price_db.convert(&p.units, &target_currency, date)
                {
                    Ok(Value::Amount(converted))
                } else {
                    Ok(Value::Amount(p.units))
                }
            }
            Value::Amount(a) => {
                if a.currency == target_currency {
                    Ok(Value::Amount(a))
                } else if let Some(converted) = self.price_db.convert(&a, &target_currency, date) {
                    Ok(Value::Amount(converted))
                } else {
                    Ok(Value::Amount(a))
                }
            }
            Value::Inventory(inv) => {
                let mut total = Decimal::ZERO;
                for pos in inv.positions() {
                    if pos.units.currency == target_currency {
                        total += pos.units.number;
                    } else if let Some(converted) =
                        self.price_db.convert(&pos.units, &target_currency, date)
                    {
                        total += converted.number;
                    }
                }
                Ok(Value::Amount(Amount::new(total, &target_currency)))
            }
            _ => Err(QueryError::Type(
                "VALUE expects a position or inventory".to_string(),
            )),
        }
    }

    /// Evaluate GETPRICE function.
    ///
    /// `GETPRICE(base_currency, quote_currency)` - Get price using context date
    /// `GETPRICE(base_currency, quote_currency, date)` - Get price at specific date
    fn eval_getprice(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        if func.args.len() < 2 || func.args.len() > 3 {
            return Err(QueryError::InvalidArguments(
                "GETPRICE".to_string(),
                "expected 2 or 3 arguments: (base_currency, quote_currency[, date])".to_string(),
            ));
        }

        // Get base currency
        let base = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GETPRICE: first argument must be a currency string".to_string(),
                ));
            }
        };

        // Get quote currency
        let quote = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "GETPRICE: second argument must be a currency string".to_string(),
                ));
            }
        };

        // Get date (optional, defaults to context date)
        let date = if func.args.len() == 3 {
            match self.evaluate_expr(&func.args[2], ctx)? {
                Value::Date(d) => d,
                _ => {
                    return Err(QueryError::Type(
                        "GETPRICE: third argument must be a date".to_string(),
                    ));
                }
            }
        } else {
            ctx.transaction.date
        };

        // Look up the price
        match self.price_db.get_price(&base, &quote, date) {
            Some(price) => Ok(Value::Number(price)),
            None => Ok(Value::Null),
        }
    }

    /// Evaluate metadata functions: `META`, `ENTRY_META`, `ANY_META`.
    ///
    /// - `META(key)` - Get metadata value from the posting
    /// - `ENTRY_META(key)` - Get metadata value from the transaction
    /// - `ANY_META(key)` - Get metadata value from posting, falling back to transaction
    fn eval_meta_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        Self::require_args(name, func, 1)?;

        let key = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(format!(
                    "{name}: argument must be a string key"
                )));
            }
        };

        let posting = &ctx.transaction.postings[ctx.posting_index];

        let meta_value = match name {
            "META" | "POSTING_META" => posting.meta.get(&key),
            "ENTRY_META" => ctx.transaction.meta.get(&key),
            "ANY_META" => posting
                .meta
                .get(&key)
                .or_else(|| ctx.transaction.meta.get(&key)),
            _ => unreachable!(),
        };

        Ok(Self::meta_value_to_value(meta_value))
    }

    /// Convert a `MetaValue` to a `Value`.
    fn meta_value_to_value(mv: Option<&MetaValue>) -> Value {
        match mv {
            None => Value::Null,
            Some(MetaValue::String(s)) => Value::String(s.clone()),
            Some(MetaValue::Number(n)) => Value::Number(*n),
            Some(MetaValue::Date(d)) => Value::Date(*d),
            Some(MetaValue::Bool(b)) => Value::Boolean(*b),
            Some(MetaValue::Amount(a)) => Value::Amount(a.clone()),
            Some(MetaValue::Account(s)) => Value::String(s.clone()),
            Some(MetaValue::Currency(s)) => Value::String(s.clone()),
            Some(MetaValue::Tag(s)) => Value::String(s.clone()),
            Some(MetaValue::Link(s)) => Value::String(s.clone()),
            Some(MetaValue::None) => Value::Null,
        }
    }

    /// Evaluate CONVERT function (currency conversion).
    ///
    /// `CONVERT(position, currency)` - Convert position/amount to target currency.
    /// `CONVERT(position, currency, date)` - Convert using price at specific date.
    fn eval_convert(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        if func.args.len() < 2 || func.args.len() > 3 {
            return Err(QueryError::InvalidArguments(
                "CONVERT".to_string(),
                "expected 2 or 3 arguments: (value, currency[, date])".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;

        let target_currency = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "CONVERT: second argument must be a currency string".to_string(),
                ));
            }
        };

        let date = if func.args.len() == 3 {
            match self.evaluate_expr(&func.args[2], ctx)? {
                Value::Date(d) => d,
                _ => {
                    return Err(QueryError::Type(
                        "CONVERT: third argument must be a date".to_string(),
                    ));
                }
            }
        } else {
            ctx.transaction.date
        };

        match val {
            Value::Position(p) => {
                if p.units.currency == target_currency {
                    Ok(Value::Amount(p.units))
                } else if let Some(converted) =
                    self.price_db.convert(&p.units, &target_currency, date)
                {
                    Ok(Value::Amount(converted))
                } else {
                    // Return original units if no conversion available
                    Ok(Value::Amount(p.units))
                }
            }
            Value::Amount(a) => {
                if a.currency == target_currency {
                    Ok(Value::Amount(a))
                } else if let Some(converted) = self.price_db.convert(&a, &target_currency, date) {
                    Ok(Value::Amount(converted))
                } else {
                    Ok(Value::Amount(a))
                }
            }
            Value::Inventory(inv) => {
                let mut total = Decimal::ZERO;
                for pos in inv.positions() {
                    if pos.units.currency == target_currency {
                        total += pos.units.number;
                    } else if let Some(converted) =
                        self.price_db.convert(&pos.units, &target_currency, date)
                    {
                        total += converted.number;
                    }
                }
                Ok(Value::Amount(Amount::new(total, &target_currency)))
            }
            Value::Number(n) => {
                // Just wrap the number as an amount with the target currency
                Ok(Value::Amount(Amount::new(n, &target_currency)))
            }
            _ => Err(QueryError::Type(
                "CONVERT expects a position, amount, inventory, or number".to_string(),
            )),
        }
    }

    /// Evaluate INT function (convert to integer).
    fn eval_int(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        use rust_decimal::prelude::ToPrimitive;

        Self::require_args("INT", func, 1)?;

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        match val {
            Value::Integer(i) => Ok(Value::Integer(i)),
            Value::Number(n) => {
                // Truncate decimal to integer
                n.trunc()
                    .to_i64()
                    .map(Value::Integer)
                    .ok_or_else(|| QueryError::Type("INT: number too large for integer".into()))
            }
            Value::Boolean(b) => Ok(Value::Integer(i64::from(b))),
            Value::String(s) => s
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| QueryError::Type(format!("INT: cannot parse '{s}' as integer"))),
            Value::Null => Ok(Value::Null),
            _ => Err(QueryError::Type(
                "INT expects a number, integer, boolean, or string".to_string(),
            )),
        }
    }

    /// Evaluate DECIMAL function (convert to decimal).
    fn eval_decimal(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("DECIMAL", func, 1)?;

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        match val {
            Value::Number(n) => Ok(Value::Number(n)),
            Value::Integer(i) => Ok(Value::Number(Decimal::from(i))),
            Value::Boolean(b) => Ok(Value::Number(if b { Decimal::ONE } else { Decimal::ZERO })),
            Value::String(s) => s
                .parse::<Decimal>()
                .map(Value::Number)
                .map_err(|_| QueryError::Type(format!("DECIMAL: cannot parse '{s}' as decimal"))),
            Value::Null => Ok(Value::Null),
            _ => Err(QueryError::Type(
                "DECIMAL expects a number, integer, boolean, or string".to_string(),
            )),
        }
    }

    /// Evaluate STR function (convert to string).
    fn eval_str(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("STR", func, 1)?;

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        match val {
            Value::String(s) => Ok(Value::String(s)),
            Value::Integer(i) => Ok(Value::String(i.to_string())),
            Value::Number(n) => Ok(Value::String(n.to_string())),
            Value::Boolean(b) => Ok(Value::String(if b { "TRUE" } else { "FALSE" }.to_string())),
            Value::Date(d) => Ok(Value::String(d.to_string())),
            Value::Amount(a) => Ok(Value::String(format!("{} {}", a.number, a.currency))),
            Value::Null => Ok(Value::Null),
            _ => Err(QueryError::Type("STR expects a scalar value".to_string())),
        }
    }

    /// Evaluate BOOL function (convert to boolean).
    fn eval_bool(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("BOOL", func, 1)?;

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        match val {
            Value::Boolean(b) => Ok(Value::Boolean(b)),
            Value::Integer(i) => Ok(Value::Boolean(i != 0)),
            Value::Number(n) => Ok(Value::Boolean(!n.is_zero())),
            Value::String(s) => {
                let s_upper = s.to_uppercase();
                match s_upper.as_str() {
                    "TRUE" | "YES" | "1" | "T" | "Y" => Ok(Value::Boolean(true)),
                    "FALSE" | "NO" | "0" | "F" | "N" | "" => Ok(Value::Boolean(false)),
                    _ => Err(QueryError::Type(format!(
                        "BOOL: cannot parse '{s}' as boolean"
                    ))),
                }
            }
            Value::Null => Ok(Value::Null),
            _ => Err(QueryError::Type(
                "BOOL expects an integer, number, boolean, or string".to_string(),
            )),
        }
    }

    /// Evaluate COALESCE function.
    fn eval_coalesce(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        for arg in &func.args {
            let val = self.evaluate_expr(arg, ctx)?;
            if !matches!(val, Value::Null) {
                return Ok(val);
            }
        }
        Ok(Value::Null)
    }

    /// Evaluate ONLY function.
    ///
    /// `ONLY(key, inventory)` - Extract amount with given currency from inventory.
    /// Returns the amount if exactly one position matches, NULL otherwise.
    fn eval_only(&self, func: &FunctionCall, ctx: &PostingContext) -> Result<Value, QueryError> {
        Self::require_args("ONLY", func, 2)?;

        // Get the currency key
        let key = match self.evaluate_expr(&func.args[0], ctx)? {
            Value::String(s) => s,
            _ => {
                return Err(QueryError::Type(
                    "ONLY: first argument must be a currency string".to_string(),
                ));
            }
        };

        // Get the inventory
        let inv = match self.evaluate_expr(&func.args[1], ctx)? {
            Value::Inventory(inv) => inv,
            Value::Position(pos) => {
                // If it's a single position, check if it matches
                if pos.units.currency == key {
                    return Ok(Value::Amount(pos.units));
                }
                return Ok(Value::Null);
            }
            Value::Amount(amt) => {
                // If it's a single amount, check if it matches
                if amt.currency == key {
                    return Ok(Value::Amount(amt));
                }
                return Ok(Value::Null);
            }
            Value::Null => return Ok(Value::Null),
            _ => {
                return Err(QueryError::Type(
                    "ONLY: second argument must be an inventory, position, or amount".to_string(),
                ));
            }
        };

        // Find positions matching the currency
        let matching: Vec<_> = inv
            .positions()
            .iter()
            .filter(|p| p.units.currency == key)
            .collect();

        match matching.len() {
            0 => Ok(Value::Null),
            1 => Ok(Value::Amount(matching[0].units.clone())),
            _ => Ok(Value::Null), // Multiple matches, return NULL
        }
    }

    /// Evaluate a function with pre-evaluated arguments (for subquery context).
    fn evaluate_function_on_values(&self, name: &str, args: &[Value]) -> Result<Value, QueryError> {
        let name_upper = name.to_uppercase();
        match name_upper.as_str() {
            // Date functions
            "TODAY" => Ok(Value::Date(chrono::Local::now().date_naive())),
            "YEAR" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::Date(d) => Ok(Value::Integer(d.year().into())),
                    _ => Err(QueryError::Type("YEAR expects a date".to_string())),
                }
            }
            "MONTH" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::Date(d) => Ok(Value::Integer(d.month().into())),
                    _ => Err(QueryError::Type("MONTH expects a date".to_string())),
                }
            }
            "DAY" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::Date(d) => Ok(Value::Integer(d.day().into())),
                    _ => Err(QueryError::Type("DAY expects a date".to_string())),
                }
            }
            // String functions
            "LENGTH" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::Integer(s.len() as i64)),
                    _ => Err(QueryError::Type("LENGTH expects a string".to_string())),
                }
            }
            "UPPER" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::String(s.to_uppercase())),
                    _ => Err(QueryError::Type("UPPER expects a string".to_string())),
                }
            }
            "LOWER" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::String(s.to_lowercase())),
                    _ => Err(QueryError::Type("LOWER expects a string".to_string())),
                }
            }
            "TRIM" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::String(s.trim().to_string())),
                    _ => Err(QueryError::Type("TRIM expects a string".to_string())),
                }
            }
            // Math functions
            "ABS" => {
                Self::require_args_count(&name_upper, args, 1)?;
                match &args[0] {
                    Value::Number(n) => Ok(Value::Number(n.abs())),
                    Value::Integer(i) => Ok(Value::Integer(i.abs())),
                    _ => Err(QueryError::Type("ABS expects a number".to_string())),
                }
            }
            "ROUND" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(QueryError::InvalidArguments(
                        "ROUND".to_string(),
                        "expected 1 or 2 arguments".to_string(),
                    ));
                }
                match &args[0] {
                    Value::Number(n) => {
                        let scale = if args.len() == 2 {
                            match &args[1] {
                                Value::Integer(i) => *i as u32,
                                _ => 0,
                            }
                        } else {
                            0
                        };
                        Ok(Value::Number(n.round_dp(scale)))
                    }
                    Value::Integer(i) => Ok(Value::Integer(*i)),
                    _ => Err(QueryError::Type("ROUND expects a number".to_string())),
                }
            }
            // Utility functions
            "COALESCE" => {
                for arg in args {
                    if !matches!(arg, Value::Null) {
                        return Ok(arg.clone());
                    }
                }
                Ok(Value::Null)
            }
            // Aggregate functions return Null when evaluated on a single row
            "SUM" | "COUNT" | "MIN" | "MAX" | "FIRST" | "LAST" | "AVG" => Ok(Value::Null),
            _ => Err(QueryError::UnknownFunction(name.to_string())),
        }
    }

    /// Helper to require a specific number of arguments (for pre-evaluated args).
    fn require_args_count(name: &str, args: &[Value], expected: usize) -> Result<(), QueryError> {
        if args.len() != expected {
            return Err(QueryError::InvalidArguments(
                name.to_string(),
                format!("expected {} argument(s), got {}", expected, args.len()),
            ));
        }
        Ok(())
    }

    /// Helper to require a specific number of arguments.
    fn require_args(name: &str, func: &FunctionCall, expected: usize) -> Result<(), QueryError> {
        if func.args.len() != expected {
            return Err(QueryError::InvalidArguments(
                name.to_string(),
                format!("expected {expected} argument(s)"),
            ));
        }
        Ok(())
    }

    /// Evaluate a binary operation.
    fn evaluate_binary_op(&self, op: &BinaryOp, ctx: &PostingContext) -> Result<Value, QueryError> {
        let left = self.evaluate_expr(&op.left, ctx)?;
        let right = self.evaluate_expr(&op.right, ctx)?;

        match op.op {
            BinaryOperator::Eq => Ok(Value::Boolean(self.values_equal(&left, &right))),
            BinaryOperator::Ne => Ok(Value::Boolean(!self.values_equal(&left, &right))),
            BinaryOperator::Lt => self.compare_values(&left, &right, std::cmp::Ordering::is_lt),
            BinaryOperator::Le => self.compare_values(&left, &right, std::cmp::Ordering::is_le),
            BinaryOperator::Gt => self.compare_values(&left, &right, std::cmp::Ordering::is_gt),
            BinaryOperator::Ge => self.compare_values(&left, &right, std::cmp::Ordering::is_ge),
            BinaryOperator::And => {
                let l = self.to_bool(&left)?;
                let r = self.to_bool(&right)?;
                Ok(Value::Boolean(l && r))
            }
            BinaryOperator::Or => {
                let l = self.to_bool(&left)?;
                let r = self.to_bool(&right)?;
                Ok(Value::Boolean(l || r))
            }
            BinaryOperator::Regex => {
                // ~ operator: string matches regex pattern
                let s = match left {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "regex requires string left operand".to_string(),
                        ));
                    }
                };
                let pattern = match right {
                    Value::String(p) => p,
                    _ => {
                        return Err(QueryError::Type(
                            "regex requires string pattern".to_string(),
                        ));
                    }
                };
                // Use cached regex matching
                let re = self.require_regex(&pattern)?;
                Ok(Value::Boolean(re.is_match(&s)))
            }
            BinaryOperator::In => {
                // Check if left value is in right set
                match right {
                    Value::StringSet(set) => {
                        let needle = match left {
                            Value::String(s) => s,
                            _ => {
                                return Err(QueryError::Type(
                                    "IN requires string left operand".to_string(),
                                ));
                            }
                        };
                        Ok(Value::Boolean(set.contains(&needle)))
                    }
                    _ => Err(QueryError::Type(
                        "IN requires set right operand".to_string(),
                    )),
                }
            }
            BinaryOperator::NotRegex => {
                // !~ operator: string does not match regex pattern
                let s = match left {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "!~ requires string left operand".to_string(),
                        ));
                    }
                };
                let pattern = match right {
                    Value::String(p) => p,
                    _ => {
                        return Err(QueryError::Type("!~ requires string pattern".to_string()));
                    }
                };
                let re = self.require_regex(&pattern)?;
                Ok(Value::Boolean(!re.is_match(&s)))
            }
            BinaryOperator::NotIn => {
                // NOT IN: check if left value is not in right set
                match right {
                    Value::StringSet(set) => {
                        let needle = match left {
                            Value::String(s) => s,
                            _ => {
                                return Err(QueryError::Type(
                                    "NOT IN requires string left operand".to_string(),
                                ));
                            }
                        };
                        Ok(Value::Boolean(!set.contains(&needle)))
                    }
                    _ => Err(QueryError::Type(
                        "NOT IN requires set right operand".to_string(),
                    )),
                }
            }
            BinaryOperator::Add => {
                // Handle date + interval
                match (&left, &right) {
                    (Value::Date(d), Value::Interval(i)) | (Value::Interval(i), Value::Date(d)) => {
                        i.add_to_date(*d)
                            .map(Value::Date)
                            .ok_or_else(|| QueryError::Evaluation("date overflow".to_string()))
                    }
                    _ => self.arithmetic_op(&left, &right, |a, b| a + b),
                }
            }
            BinaryOperator::Sub => {
                // Handle date - interval
                match (&left, &right) {
                    (Value::Date(d), Value::Interval(i)) => {
                        let neg_count = i.count.checked_neg().ok_or_else(|| {
                            QueryError::Evaluation("interval count overflow".to_string())
                        })?;
                        let neg_interval = Interval::new(neg_count, i.unit);
                        neg_interval
                            .add_to_date(*d)
                            .map(Value::Date)
                            .ok_or_else(|| QueryError::Evaluation("date overflow".to_string()))
                    }
                    _ => self.arithmetic_op(&left, &right, |a, b| a - b),
                }
            }
            BinaryOperator::Mul => self.arithmetic_op(&left, &right, |a, b| a * b),
            BinaryOperator::Div => self.arithmetic_op(&left, &right, |a, b| a / b),
            BinaryOperator::Mod => self.arithmetic_op(&left, &right, |a, b| a % b),
        }
    }

    /// Evaluate a unary operation.
    fn evaluate_unary_op(&self, op: &UnaryOp, ctx: &PostingContext) -> Result<Value, QueryError> {
        let val = self.evaluate_expr(&op.operand, ctx)?;
        self.unary_op_on_value(op.op, &val)
    }

    /// Apply a unary operator to a value.
    fn unary_op_on_value(&self, op: UnaryOperator, val: &Value) -> Result<Value, QueryError> {
        match op {
            UnaryOperator::Not => {
                let b = self.to_bool(val)?;
                Ok(Value::Boolean(!b))
            }
            UnaryOperator::Neg => match val {
                Value::Number(n) => Ok(Value::Number(-*n)),
                Value::Integer(i) => Ok(Value::Integer(-*i)),
                _ => Err(QueryError::Type(
                    "negation requires numeric value".to_string(),
                )),
            },
            UnaryOperator::IsNull => Ok(Value::Boolean(matches!(val, Value::Null))),
            UnaryOperator::IsNotNull => Ok(Value::Boolean(!matches!(val, Value::Null))),
        }
    }

    /// Check if two values are equal.
    fn values_equal(&self, left: &Value, right: &Value) -> bool {
        // BQL treats NULL = NULL as TRUE
        match (left, right) {
            (Value::Null, Value::Null) => true,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Number(a), Value::Integer(b)) => *a == Decimal::from(*b),
            (Value::Integer(a), Value::Number(b)) => Decimal::from(*a) == *b,
            (Value::Date(a), Value::Date(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            _ => false,
        }
    }

    /// Compare two values.
    fn compare_values<F>(&self, left: &Value, right: &Value, pred: F) -> Result<Value, QueryError>
    where
        F: FnOnce(std::cmp::Ordering) -> bool,
    {
        let ord = match (left, right) {
            (Value::Number(a), Value::Number(b)) => a.cmp(b),
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::Number(a), Value::Integer(b)) => a.cmp(&Decimal::from(*b)),
            (Value::Integer(a), Value::Number(b)) => Decimal::from(*a).cmp(b),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Date(a), Value::Date(b)) => a.cmp(b),
            _ => return Err(QueryError::Type("cannot compare values".to_string())),
        };
        Ok(Value::Boolean(pred(ord)))
    }

    /// Check if left value is less than right value.
    fn value_less_than(&self, left: &Value, right: &Value) -> Result<bool, QueryError> {
        let ord = match (left, right) {
            (Value::Number(a), Value::Number(b)) => a.cmp(b),
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::Number(a), Value::Integer(b)) => a.cmp(&Decimal::from(*b)),
            (Value::Integer(a), Value::Number(b)) => Decimal::from(*a).cmp(b),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Date(a), Value::Date(b)) => a.cmp(b),
            _ => return Err(QueryError::Type("cannot compare values".to_string())),
        };
        Ok(ord.is_lt())
    }

    /// Perform arithmetic operation.
    fn arithmetic_op<F>(&self, left: &Value, right: &Value, op: F) -> Result<Value, QueryError>
    where
        F: FnOnce(Decimal, Decimal) -> Decimal,
    {
        let (a, b) = match (left, right) {
            (Value::Number(a), Value::Number(b)) => (*a, *b),
            (Value::Integer(a), Value::Integer(b)) => (Decimal::from(*a), Decimal::from(*b)),
            (Value::Number(a), Value::Integer(b)) => (*a, Decimal::from(*b)),
            (Value::Integer(a), Value::Number(b)) => (Decimal::from(*a), *b),
            _ => {
                return Err(QueryError::Type(
                    "arithmetic requires numeric values".to_string(),
                ));
            }
        };
        Ok(Value::Number(op(a, b)))
    }

    /// Convert a value to boolean.
    fn to_bool(&self, val: &Value) -> Result<bool, QueryError> {
        match val {
            Value::Boolean(b) => Ok(*b),
            Value::Null => Ok(false),
            _ => Err(QueryError::Type("expected boolean".to_string())),
        }
    }

    /// Check if an expression contains aggregate functions.
    fn is_aggregate_expr(expr: &Expr) -> bool {
        match expr {
            Expr::Function(func) => {
                matches!(
                    func.name.to_uppercase().as_str(),
                    "SUM" | "COUNT" | "MIN" | "MAX" | "FIRST" | "LAST" | "AVG"
                )
            }
            Expr::BinaryOp(op) => {
                Self::is_aggregate_expr(&op.left) || Self::is_aggregate_expr(&op.right)
            }
            Expr::UnaryOp(op) => Self::is_aggregate_expr(&op.operand),
            Expr::Paren(inner) => Self::is_aggregate_expr(inner),
            _ => false,
        }
    }

    /// Check if an expression is a window function.
    const fn is_window_expr(expr: &Expr) -> bool {
        matches!(expr, Expr::Window(_))
    }

    /// Check if any target contains a window function.
    fn has_window_functions(targets: &[Target]) -> bool {
        targets.iter().any(|t| Self::is_window_expr(&t.expr))
    }

    /// Resolve column names from targets.
    fn resolve_column_names(&self, targets: &[Target]) -> Result<Vec<String>, QueryError> {
        let mut names = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            if let Some(alias) = &target.alias {
                names.push(alias.clone());
            } else {
                names.push(self.expr_to_name(&target.expr, i));
            }
        }
        Ok(names)
    }

    /// Convert an expression to a column name.
    fn expr_to_name(&self, expr: &Expr, index: usize) -> String {
        match expr {
            Expr::Wildcard => "*".to_string(),
            Expr::Column(name) => name.clone(),
            Expr::Function(func) => func.name.clone(),
            Expr::Window(wf) => wf.name.clone(),
            _ => format!("col{index}"),
        }
    }

    /// Evaluate a row of results for non-aggregate query.
    fn evaluate_row(&self, targets: &[Target], ctx: &PostingContext) -> Result<Row, QueryError> {
        self.evaluate_row_with_window(targets, ctx, None)
    }

    /// Evaluate a row with optional window context.
    fn evaluate_row_with_window(
        &self,
        targets: &[Target],
        ctx: &PostingContext,
        window_ctx: Option<&WindowContext>,
    ) -> Result<Row, QueryError> {
        let mut row = Vec::new();
        for target in targets {
            if matches!(target.expr, Expr::Wildcard) {
                // Expand wildcard to default columns
                row.push(Value::Date(ctx.transaction.date));
                row.push(Value::String(ctx.transaction.flag.to_string()));
                row.push(
                    ctx.transaction
                        .payee
                        .as_ref()
                        .map_or(Value::Null, |p| Value::String(p.to_string())),
                );
                row.push(Value::String(ctx.transaction.narration.to_string()));
                let posting = &ctx.transaction.postings[ctx.posting_index];
                row.push(Value::String(posting.account.to_string()));
                row.push(
                    posting
                        .amount()
                        .map_or(Value::Null, |u| Value::Amount(u.clone())),
                );
            } else if let Expr::Window(wf) = &target.expr {
                // Handle window function
                row.push(self.evaluate_window_function(wf, window_ctx)?);
            } else {
                row.push(self.evaluate_expr(&target.expr, ctx)?);
            }
        }
        Ok(row)
    }

    /// Evaluate a window function.
    fn evaluate_window_function(
        &self,
        wf: &WindowFunction,
        window_ctx: Option<&WindowContext>,
    ) -> Result<Value, QueryError> {
        let ctx = window_ctx.ok_or_else(|| {
            QueryError::Evaluation("Window function requires window context".to_string())
        })?;

        match wf.name.to_uppercase().as_str() {
            "ROW_NUMBER" => Ok(Value::Integer(ctx.row_number as i64)),
            "RANK" => Ok(Value::Integer(ctx.rank as i64)),
            "DENSE_RANK" => Ok(Value::Integer(ctx.dense_rank as i64)),
            _ => Err(QueryError::Evaluation(format!(
                "Window function '{}' not yet implemented",
                wf.name
            ))),
        }
    }

    /// Compute window contexts for all postings based on the window spec.
    fn compute_window_contexts(
        &self,
        postings: &[PostingContext],
        wf: &WindowFunction,
    ) -> Result<Vec<WindowContext>, QueryError> {
        let spec = &wf.over;

        // Compute partition keys for each posting
        let mut partition_keys: Vec<String> = Vec::with_capacity(postings.len());
        for ctx in postings {
            if let Some(partition_exprs) = &spec.partition_by {
                let mut key_values = Vec::new();
                for expr in partition_exprs {
                    key_values.push(self.evaluate_expr(expr, ctx)?);
                }
                partition_keys.push(Self::make_group_key(&key_values));
            } else {
                // No partition - all rows in one partition
                partition_keys.push(String::new());
            }
        }

        // Group posting indices by partition key
        let mut partitions: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, key) in partition_keys.iter().enumerate() {
            partitions.entry(key.clone()).or_default().push(idx);
        }

        // Compute order values for sorting within partitions
        let mut order_values: Vec<Vec<Value>> = Vec::with_capacity(postings.len());
        for ctx in postings {
            if let Some(order_specs) = &spec.order_by {
                let mut values = Vec::new();
                for order_spec in order_specs {
                    values.push(self.evaluate_expr(&order_spec.expr, ctx)?);
                }
                order_values.push(values);
            } else {
                order_values.push(Vec::new());
            }
        }

        // Initialize window contexts
        let mut window_contexts: Vec<WindowContext> = vec![
            WindowContext {
                row_number: 0,
                rank: 0,
                dense_rank: 0,
            };
            postings.len()
        ];

        // Process each partition
        for indices in partitions.values() {
            // Sort indices within partition by order values
            let mut sorted_indices: Vec<usize> = indices.clone();
            if let Some(order_specs) = &spec.order_by {
                sorted_indices.sort_by(|&a, &b| {
                    let vals_a = &order_values[a];
                    let vals_b = &order_values[b];
                    for (i, (va, vb)) in vals_a.iter().zip(vals_b.iter()).enumerate() {
                        let cmp = self.compare_values_for_sort(va, vb);
                        if cmp != std::cmp::Ordering::Equal {
                            return if order_specs
                                .get(i)
                                .is_some_and(|s| s.direction == SortDirection::Desc)
                            {
                                cmp.reverse()
                            } else {
                                cmp
                            };
                        }
                    }
                    std::cmp::Ordering::Equal
                });
            }

            // Assign ranks within the partition
            let mut row_num = 1;
            let mut rank = 1;
            let mut dense_rank = 1;
            let mut prev_values: Option<&Vec<Value>> = None;

            for (position, &original_idx) in sorted_indices.iter().enumerate() {
                let current_values = &order_values[original_idx];

                // Check if this row has the same order values as the previous row
                let is_tie = if let Some(prev) = prev_values {
                    current_values == prev
                } else {
                    false
                };

                if !is_tie && position > 0 {
                    // New value - update ranks
                    rank = position + 1;
                    dense_rank += 1;
                }
                window_contexts[original_idx] = WindowContext {
                    row_number: row_num,
                    rank,
                    dense_rank,
                };

                row_num += 1;
                prev_values = Some(current_values);
            }
        }

        Ok(window_contexts)
    }

    /// Extract the first window function from targets (for getting the window spec).
    fn find_window_function(targets: &[Target]) -> Option<&WindowFunction> {
        for target in targets {
            if let Expr::Window(wf) = &target.expr {
                return Some(wf);
            }
        }
        None
    }

    /// Generate a hashable key from a vector of Values.
    /// Used for O(1) grouping instead of O(n) linear search.
    fn make_group_key(values: &[Value]) -> String {
        use std::fmt::Write;
        let mut key = String::new();
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                key.push('\x00'); // Null separator between values
            }
            match v {
                Value::String(s) => {
                    key.push('S');
                    key.push_str(s);
                }
                Value::Number(n) => {
                    key.push('N');
                    let _ = write!(key, "{n}");
                }
                Value::Integer(n) => {
                    key.push('I');
                    let _ = write!(key, "{n}");
                }
                Value::Date(d) => {
                    key.push('D');
                    let _ = write!(key, "{d}");
                }
                Value::Boolean(b) => {
                    key.push(if *b { 'T' } else { 'F' });
                }
                Value::Amount(a) => {
                    key.push('A');
                    let _ = write!(key, "{} {}", a.number, a.currency);
                }
                Value::Position(p) => {
                    key.push('P');
                    let _ = write!(key, "{} {}", p.units.number, p.units.currency);
                }
                Value::Inventory(_) => {
                    // Inventories are complex; use a placeholder
                    // (unlikely to be used as GROUP BY key)
                    key.push('V');
                }
                Value::StringSet(ss) => {
                    key.push('Z');
                    for s in ss {
                        key.push_str(s);
                        key.push(',');
                    }
                }
                Value::Metadata(meta) => {
                    // Metadata as GROUP BY key - use debug representation
                    key.push('M');
                    let _ = write!(key, "{meta:?}");
                }
                Value::Interval(i) => {
                    key.push('R');
                    let _ = write!(key, "{} {:?}", i.count, i.unit);
                }
                Value::Object(obj) => {
                    // Objects are complex; serialize keys/values
                    key.push('O');
                    for (k, v) in obj {
                        key.push_str(k);
                        key.push(':');
                        let _ = write!(key, "{v:?}");
                        key.push(';');
                    }
                }
                Value::Null => {
                    key.push('0');
                }
            }
        }
        key
    }

    /// Group postings by the GROUP BY expressions.
    /// Uses `HashMap` for O(1) key lookup instead of O(n) linear search.
    fn group_postings<'b>(
        &self,
        postings: &'b [PostingContext<'a>],
        group_by: Option<&Vec<Expr>>,
    ) -> Result<Vec<(Vec<Value>, Vec<&'b PostingContext<'a>>)>, QueryError> {
        if let Some(group_exprs) = group_by {
            // Use HashMap for O(1) grouping
            let mut group_map: HashMap<String, (Vec<Value>, Vec<&PostingContext<'a>>)> =
                HashMap::new();

            for ctx in postings {
                let mut key_values = Vec::with_capacity(group_exprs.len());
                for expr in group_exprs {
                    key_values.push(self.evaluate_expr(expr, ctx)?);
                }
                let hash_key = Self::make_group_key(&key_values);

                group_map
                    .entry(hash_key)
                    .or_insert_with(|| (key_values, Vec::new()))
                    .1
                    .push(ctx);
            }

            Ok(group_map.into_values().collect())
        } else {
            // No GROUP BY - all postings in one group
            // But if there are no postings, return no groups (matching Python beancount)
            if postings.is_empty() {
                Ok(vec![])
            } else {
                Ok(vec![(Vec::new(), postings.iter().collect())])
            }
        }
    }

    /// Evaluate a row of aggregate results.
    fn evaluate_aggregate_row(
        &self,
        targets: &[Target],
        group: &[&PostingContext],
    ) -> Result<Row, QueryError> {
        let mut row = Vec::new();
        for target in targets {
            row.push(self.evaluate_aggregate_expr(&target.expr, group)?);
        }
        Ok(row)
    }

    /// Evaluate an expression in an aggregate context.
    fn evaluate_aggregate_expr(
        &self,
        expr: &Expr,
        group: &[&PostingContext],
    ) -> Result<Value, QueryError> {
        match expr {
            Expr::Function(func) => {
                match func.name.to_uppercase().as_str() {
                    "COUNT" => {
                        // COUNT(*) counts all rows
                        Ok(Value::Integer(group.len() as i64))
                    }
                    "SUM" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "SUM".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        let mut total = Inventory::new();
                        for ctx in group {
                            let val = self.evaluate_expr(&func.args[0], ctx)?;
                            match val {
                                Value::Amount(amt) => {
                                    let pos = Position::simple(amt);
                                    total.add(pos);
                                }
                                Value::Position(pos) => {
                                    total.add(pos);
                                }
                                Value::Number(n) => {
                                    // Sum as raw number
                                    let pos =
                                        Position::simple(Amount::new(n, "__NUMBER__".to_string()));
                                    total.add(pos);
                                }
                                Value::Null => {}
                                _ => {
                                    return Err(QueryError::Type(
                                        "SUM requires numeric or position value".to_string(),
                                    ));
                                }
                            }
                        }
                        Ok(Value::Inventory(total))
                    }
                    "FIRST" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "FIRST".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        // Find chronologically first posting (by transaction date)
                        if let Some(ctx) = group.iter().min_by_key(|c| c.transaction.date) {
                            self.evaluate_expr(&func.args[0], ctx)
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "LAST" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "LAST".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        // Find chronologically last posting (by transaction date)
                        if let Some(ctx) = group.iter().max_by_key(|c| c.transaction.date) {
                            self.evaluate_expr(&func.args[0], ctx)
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "MIN" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "MIN".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        let mut min_val: Option<Value> = None;
                        for ctx in group {
                            let val = self.evaluate_expr(&func.args[0], ctx)?;
                            if matches!(val, Value::Null) {
                                continue;
                            }
                            min_val = Some(match min_val {
                                None => val,
                                Some(current) => {
                                    if self.value_less_than(&val, &current)? {
                                        val
                                    } else {
                                        current
                                    }
                                }
                            });
                        }
                        Ok(min_val.unwrap_or(Value::Null))
                    }
                    "MAX" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "MAX".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        let mut max_val: Option<Value> = None;
                        for ctx in group {
                            let val = self.evaluate_expr(&func.args[0], ctx)?;
                            if matches!(val, Value::Null) {
                                continue;
                            }
                            max_val = Some(match max_val {
                                None => val,
                                Some(current) => {
                                    if self.value_less_than(&current, &val)? {
                                        val
                                    } else {
                                        current
                                    }
                                }
                            });
                        }
                        Ok(max_val.unwrap_or(Value::Null))
                    }
                    "AVG" => {
                        if func.args.len() != 1 {
                            return Err(QueryError::InvalidArguments(
                                "AVG".to_string(),
                                "expected 1 argument".to_string(),
                            ));
                        }
                        let mut sum = Decimal::ZERO;
                        let mut count = 0i64;
                        for ctx in group {
                            let val = self.evaluate_expr(&func.args[0], ctx)?;
                            match val {
                                Value::Number(n) => {
                                    sum += n;
                                    count += 1;
                                }
                                Value::Integer(i) => {
                                    sum += Decimal::from(i);
                                    count += 1;
                                }
                                Value::Null => {}
                                _ => {
                                    return Err(QueryError::Type(
                                        "AVG expects numeric values".to_string(),
                                    ));
                                }
                            }
                        }
                        if count == 0 {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Number(sum / Decimal::from(count)))
                        }
                    }
                    _ => {
                        // Non-aggregate function
                        if let Some(ctx) = group.first() {
                            self.evaluate_function(func, ctx)
                        } else {
                            Ok(Value::Null)
                        }
                    }
                }
            }
            Expr::Column(_) => {
                // For non-aggregate columns in aggregate query, take first value
                if let Some(ctx) = group.first() {
                    self.evaluate_expr(expr, ctx)
                } else {
                    Ok(Value::Null)
                }
            }
            Expr::BinaryOp(op) => {
                let left = self.evaluate_aggregate_expr(&op.left, group)?;
                let right = self.evaluate_aggregate_expr(&op.right, group)?;
                // Re-evaluate with computed values
                self.binary_op_on_values(op.op, &left, &right)
            }
            _ => {
                // For other expressions, evaluate on first row
                if let Some(ctx) = group.first() {
                    self.evaluate_expr(expr, ctx)
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    /// Apply binary operator to already-evaluated values.
    fn binary_op_on_values(
        &self,
        op: BinaryOperator,
        left: &Value,
        right: &Value,
    ) -> Result<Value, QueryError> {
        match op {
            BinaryOperator::Eq => Ok(Value::Boolean(self.values_equal(left, right))),
            BinaryOperator::Ne => Ok(Value::Boolean(!self.values_equal(left, right))),
            BinaryOperator::Lt => self.compare_values(left, right, std::cmp::Ordering::is_lt),
            BinaryOperator::Le => self.compare_values(left, right, std::cmp::Ordering::is_le),
            BinaryOperator::Gt => self.compare_values(left, right, std::cmp::Ordering::is_gt),
            BinaryOperator::Ge => self.compare_values(left, right, std::cmp::Ordering::is_ge),
            BinaryOperator::And => {
                let l = self.to_bool(left)?;
                let r = self.to_bool(right)?;
                Ok(Value::Boolean(l && r))
            }
            BinaryOperator::Or => {
                let l = self.to_bool(left)?;
                let r = self.to_bool(right)?;
                Ok(Value::Boolean(l || r))
            }
            BinaryOperator::Regex => {
                // ~ operator: string matches regex pattern
                let s = match left {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "regex requires string left operand".to_string(),
                        ));
                    }
                };
                let pattern = match right {
                    Value::String(p) => p,
                    _ => {
                        return Err(QueryError::Type(
                            "regex requires string pattern".to_string(),
                        ));
                    }
                };
                // Use cached regex matching
                let re = self.require_regex(pattern)?;
                Ok(Value::Boolean(re.is_match(s)))
            }
            BinaryOperator::In => {
                // Check if left value is in right set
                match right {
                    Value::StringSet(set) => {
                        let needle = match left {
                            Value::String(s) => s,
                            _ => {
                                return Err(QueryError::Type(
                                    "IN requires string left operand".to_string(),
                                ));
                            }
                        };
                        Ok(Value::Boolean(set.contains(needle)))
                    }
                    _ => Err(QueryError::Type(
                        "IN requires set right operand".to_string(),
                    )),
                }
            }
            BinaryOperator::NotRegex => {
                // !~ operator: string does not match regex pattern
                let s = match left {
                    Value::String(s) => s,
                    _ => {
                        return Err(QueryError::Type(
                            "!~ requires string left operand".to_string(),
                        ));
                    }
                };
                let pattern = match right {
                    Value::String(p) => p,
                    _ => {
                        return Err(QueryError::Type("!~ requires string pattern".to_string()));
                    }
                };
                let re = self.require_regex(pattern)?;
                Ok(Value::Boolean(!re.is_match(s)))
            }
            BinaryOperator::NotIn => {
                // NOT IN: check if left value is not in right set
                match right {
                    Value::StringSet(set) => {
                        let needle = match left {
                            Value::String(s) => s,
                            _ => {
                                return Err(QueryError::Type(
                                    "NOT IN requires string left operand".to_string(),
                                ));
                            }
                        };
                        Ok(Value::Boolean(!set.contains(needle)))
                    }
                    _ => Err(QueryError::Type(
                        "NOT IN requires set right operand".to_string(),
                    )),
                }
            }
            BinaryOperator::Add => {
                // Handle date + interval
                match (left, right) {
                    (Value::Date(d), Value::Interval(i)) | (Value::Interval(i), Value::Date(d)) => {
                        i.add_to_date(*d)
                            .map(Value::Date)
                            .ok_or_else(|| QueryError::Evaluation("date overflow".to_string()))
                    }
                    _ => self.arithmetic_op(left, right, |a, b| a + b),
                }
            }
            BinaryOperator::Sub => {
                // Handle date - interval
                match (left, right) {
                    (Value::Date(d), Value::Interval(i)) => {
                        let neg_count = i.count.checked_neg().ok_or_else(|| {
                            QueryError::Evaluation("interval count overflow".to_string())
                        })?;
                        let neg_interval = Interval::new(neg_count, i.unit);
                        neg_interval
                            .add_to_date(*d)
                            .map(Value::Date)
                            .ok_or_else(|| QueryError::Evaluation("date overflow".to_string()))
                    }
                    _ => self.arithmetic_op(left, right, |a, b| a - b),
                }
            }
            BinaryOperator::Mul => self.arithmetic_op(left, right, |a, b| a * b),
            BinaryOperator::Div => self.arithmetic_op(left, right, |a, b| a / b),
            BinaryOperator::Mod => self.arithmetic_op(left, right, |a, b| a % b),
        }
    }

    /// Sort results by ORDER BY clauses.
    fn sort_results(
        &self,
        result: &mut QueryResult,
        order_by: &[OrderSpec],
    ) -> Result<(), QueryError> {
        if order_by.is_empty() {
            return Ok(());
        }

        // Build a map from column names to indices
        let column_indices: std::collections::HashMap<&str, usize> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, name)| (name.as_str(), i))
            .collect();

        // Resolve ORDER BY expressions to column indices
        let mut sort_specs: Vec<(usize, bool)> = Vec::new();
        for spec in order_by {
            // Try to resolve the expression to a column index
            let idx = match &spec.expr {
                Expr::Column(name) => column_indices
                    .get(name.as_str())
                    .copied()
                    .ok_or_else(|| QueryError::UnknownColumn(name.clone()))?,
                Expr::Function(func) => {
                    // Try to find a column with the function name
                    column_indices
                        .get(func.name.as_str())
                        .copied()
                        .ok_or_else(|| {
                            QueryError::Evaluation(format!(
                                "ORDER BY expression not found in SELECT: {}",
                                func.name
                            ))
                        })?
                }
                _ => {
                    return Err(QueryError::Evaluation(
                        "ORDER BY expression must reference a selected column".to_string(),
                    ));
                }
            };
            let ascending = spec.direction != SortDirection::Desc;
            sort_specs.push((idx, ascending));
        }

        // Sort the rows
        result.rows.sort_by(|a, b| {
            for (idx, ascending) in &sort_specs {
                if *idx >= a.len() || *idx >= b.len() {
                    continue;
                }
                let ord = self.compare_values_for_sort(&a[*idx], &b[*idx]);
                if ord != std::cmp::Ordering::Equal {
                    return if *ascending { ord } else { ord.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(())
    }

    /// Compare two values for sorting purposes.
    fn compare_values_for_sort(&self, left: &Value, right: &Value) -> std::cmp::Ordering {
        match (left, right) {
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Greater, // Nulls sort last
            (_, Value::Null) => std::cmp::Ordering::Less,
            (Value::Number(a), Value::Number(b)) => a.cmp(b),
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::Number(a), Value::Integer(b)) => a.cmp(&Decimal::from(*b)),
            (Value::Integer(a), Value::Number(b)) => Decimal::from(*a).cmp(b),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Date(a), Value::Date(b)) => a.cmp(b),
            (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
            // Compare amounts by their numeric value (same currency assumed)
            (Value::Amount(a), Value::Amount(b)) => a.number.cmp(&b.number),
            // Compare positions by their units' numeric value
            (Value::Position(a), Value::Position(b)) => a.units.number.cmp(&b.units.number),
            // Compare inventories by first position's value (for single-currency)
            (Value::Inventory(a), Value::Inventory(b)) => {
                let a_val = a.positions().first().map(|p| &p.units.number);
                let b_val = b.positions().first().map(|p| &p.units.number);
                match (a_val, b_val) {
                    (Some(av), Some(bv)) => av.cmp(bv),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            }
            // Compare intervals by approximate days
            (Value::Interval(a), Value::Interval(b)) => a.to_approx_days().cmp(&b.to_approx_days()),
            _ => std::cmp::Ordering::Equal, // Can't compare other types
        }
    }

    /// Evaluate a HAVING clause filter on an aggregated row.
    ///
    /// The HAVING clause can reference:
    /// - Column names/aliases from the SELECT clause
    /// - Aggregate functions (evaluated on the group)
    fn evaluate_having_filter(
        &self,
        having_expr: &Expr,
        row: &[Value],
        column_names: &[String],
        targets: &[Target],
        group: &[&PostingContext],
    ) -> Result<bool, QueryError> {
        // Build a map of column name -> index for quick lookup
        let col_map: HashMap<String, usize> = column_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.to_uppercase(), i))
            .collect();

        // Also map aliases
        let alias_map: HashMap<String, usize> = targets
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.alias.as_ref().map(|a| (a.to_uppercase(), i)))
            .collect();

        let val = self.evaluate_having_expr(having_expr, row, &col_map, &alias_map, group)?;

        match val {
            Value::Boolean(b) => Ok(b),
            Value::Null => Ok(false), // NULL is treated as false in HAVING
            _ => Err(QueryError::Type(
                "HAVING clause must evaluate to boolean".to_string(),
            )),
        }
    }

    /// Evaluate an expression in HAVING context (can reference aggregated values).
    fn evaluate_having_expr(
        &self,
        expr: &Expr,
        row: &[Value],
        col_map: &HashMap<String, usize>,
        alias_map: &HashMap<String, usize>,
        group: &[&PostingContext],
    ) -> Result<Value, QueryError> {
        match expr {
            Expr::Column(name) => {
                let upper_name = name.to_uppercase();
                // Try alias first, then column name
                if let Some(&idx) = alias_map.get(&upper_name) {
                    Ok(row.get(idx).cloned().unwrap_or(Value::Null))
                } else if let Some(&idx) = col_map.get(&upper_name) {
                    Ok(row.get(idx).cloned().unwrap_or(Value::Null))
                } else {
                    Err(QueryError::Evaluation(format!(
                        "Column '{name}' not found in SELECT clause for HAVING"
                    )))
                }
            }
            Expr::Literal(lit) => self.evaluate_literal(lit),
            Expr::Function(_) => {
                // Re-evaluate aggregate function on group
                self.evaluate_aggregate_expr(expr, group)
            }
            Expr::BinaryOp(op) => {
                let left = self.evaluate_having_expr(&op.left, row, col_map, alias_map, group)?;
                let right = self.evaluate_having_expr(&op.right, row, col_map, alias_map, group)?;
                self.binary_op_on_values(op.op, &left, &right)
            }
            Expr::UnaryOp(op) => {
                let val = self.evaluate_having_expr(&op.operand, row, col_map, alias_map, group)?;
                match op.op {
                    UnaryOperator::Not => {
                        let b = self.to_bool(&val)?;
                        Ok(Value::Boolean(!b))
                    }
                    UnaryOperator::Neg => match val {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        Value::Integer(i) => Ok(Value::Integer(-i)),
                        _ => Err(QueryError::Type(
                            "Cannot negate non-numeric value".to_string(),
                        )),
                    },
                    UnaryOperator::IsNull => Ok(Value::Boolean(matches!(val, Value::Null))),
                    UnaryOperator::IsNotNull => Ok(Value::Boolean(!matches!(val, Value::Null))),
                }
            }
            Expr::Paren(inner) => self.evaluate_having_expr(inner, row, col_map, alias_map, group),
            Expr::Wildcard => Err(QueryError::Evaluation(
                "Wildcard not allowed in HAVING clause".to_string(),
            )),
            Expr::Window(_) => Err(QueryError::Evaluation(
                "Window functions not allowed in HAVING clause".to_string(),
            )),
            Expr::Between { value, low, high } => {
                let val = self.evaluate_having_expr(value, row, col_map, alias_map, group)?;
                let low_val = self.evaluate_having_expr(low, row, col_map, alias_map, group)?;
                let high_val = self.evaluate_having_expr(high, row, col_map, alias_map, group)?;

                let ge = self.compare_values(&val, &low_val, std::cmp::Ordering::is_ge)?;
                let le = self.compare_values(&val, &high_val, std::cmp::Ordering::is_le)?;

                match (ge, le) {
                    (Value::Boolean(g), Value::Boolean(l)) => Ok(Value::Boolean(g && l)),
                    _ => Err(QueryError::Type(
                        "BETWEEN requires comparable values".to_string(),
                    )),
                }
            }
        }
    }

    /// Apply PIVOT BY transformation to results.
    ///
    /// PIVOT BY transforms rows into columns based on pivot column values.
    /// For example: `SELECT account, YEAR(date), SUM(amount) GROUP BY 1, 2 PIVOT BY YEAR(date)`
    /// would create columns for each year.
    fn apply_pivot(
        &self,
        result: &QueryResult,
        pivot_exprs: &[Expr],
        _targets: &[Target],
    ) -> Result<QueryResult, QueryError> {
        if pivot_exprs.is_empty() {
            return Ok(result.clone());
        }

        // For simplicity, we'll pivot on the first expression only
        // A full implementation would support multiple pivot columns
        let pivot_expr = &pivot_exprs[0];

        // Find which column in the result matches the pivot expression
        let pivot_col_idx = self.find_pivot_column(result, pivot_expr)?;

        // Collect unique pivot values
        let mut pivot_values: Vec<Value> = result
            .rows
            .iter()
            .map(|row| row.get(pivot_col_idx).cloned().unwrap_or(Value::Null))
            .collect();
        pivot_values.sort_by(|a, b| self.compare_values_for_sort(a, b));
        pivot_values.dedup();

        // Build new column names: original columns (except pivot) + pivot values
        let mut new_columns: Vec<String> = result
            .columns
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != pivot_col_idx)
            .map(|(_, c)| c.clone())
            .collect();

        // Identify the "value" column (usually the last one, or the one with aggregate)
        let value_col_idx = result.columns.len() - 1;

        // Add pivot value columns
        for pv in &pivot_values {
            new_columns.push(Self::value_to_string(pv));
        }

        let mut new_result = QueryResult::new(new_columns);

        // Group rows by non-pivot, non-value columns
        let group_cols: Vec<usize> = (0..result.columns.len())
            .filter(|i| *i != pivot_col_idx && *i != value_col_idx)
            .collect();

        let mut groups: HashMap<String, Vec<&Row>> = HashMap::new();
        for row in &result.rows {
            let key: String = group_cols
                .iter()
                .map(|&i| Self::value_to_string(&row[i]))
                .collect::<Vec<_>>()
                .join("|");
            groups.entry(key).or_default().push(row);
        }

        // Build pivoted rows
        for (_key, group_rows) in groups {
            let mut new_row: Vec<Value> = group_cols
                .iter()
                .map(|&i| group_rows[0][i].clone())
                .collect();

            // Build O(1) pivot value -> row index for this group
            let pivot_index: HashMap<u64, usize> = group_rows
                .iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    row.get(pivot_col_idx).map(|v| (hash_single_value(v), idx))
                })
                .collect();

            // Add pivot values with O(1) lookup
            for pv in &pivot_values {
                let pv_hash = hash_single_value(pv);
                if let Some(&row_idx) = pivot_index.get(&pv_hash) {
                    new_row.push(
                        group_rows[row_idx]
                            .get(value_col_idx)
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                } else {
                    new_row.push(Value::Null);
                }
            }

            new_result.add_row(new_row);
        }

        Ok(new_result)
    }

    /// Find the column index matching the pivot expression.
    fn find_pivot_column(
        &self,
        result: &QueryResult,
        pivot_expr: &Expr,
    ) -> Result<usize, QueryError> {
        match pivot_expr {
            Expr::Column(name) => {
                let upper_name = name.to_uppercase();
                result
                    .columns
                    .iter()
                    .position(|c| c.to_uppercase() == upper_name)
                    .ok_or_else(|| {
                        QueryError::Evaluation(format!(
                            "PIVOT BY column '{name}' not found in SELECT"
                        ))
                    })
            }
            Expr::Literal(Literal::Integer(n)) => {
                let idx = (*n as usize).saturating_sub(1);
                if idx < result.columns.len() {
                    Ok(idx)
                } else {
                    Err(QueryError::Evaluation(format!(
                        "PIVOT BY column index {n} out of range"
                    )))
                }
            }
            Expr::Literal(Literal::Number(n)) => {
                // Numbers are parsed as Decimal - convert to integer index
                use rust_decimal::prelude::ToPrimitive;
                let idx = n.to_usize().unwrap_or(0).saturating_sub(1);
                if idx < result.columns.len() {
                    Ok(idx)
                } else {
                    Err(QueryError::Evaluation(format!(
                        "PIVOT BY column index {n} out of range"
                    )))
                }
            }
            _ => {
                // For complex expressions, try to find a matching column by string representation
                // This is a simplified approach
                Err(QueryError::Evaluation(
                    "PIVOT BY must reference a column name or index".to_string(),
                ))
            }
        }
    }

    /// Convert a value to string for display/grouping.
    fn value_to_string(val: &Value) -> String {
        match val {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Date(d) => d.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Amount(a) => format!("{} {}", a.number, a.currency),
            Value::Position(p) => p.to_string(),
            Value::Inventory(inv) => inv.to_string(),
            Value::StringSet(ss) => ss.join(", "),
            Value::Metadata(meta) => {
                // Format metadata as key=value pairs
                let pairs: Vec<String> = meta.iter().map(|(k, v)| format!("{k}: {v:?}")).collect();
                format!("{{{}}}", pairs.join(", "))
            }
            Value::Interval(i) => format!("{} {:?}", i.count, i.unit),
            Value::Object(obj) => {
                // Format object as {key: value, ...}
                let pairs: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", Self::value_to_string(v)))
                    .collect();
                format!("{{{}}}", pairs.join(", "))
            }
            Value::Null => "NULL".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rust_decimal_macros::dec;
    use rustledger_core::Posting;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn sample_directives() -> Vec<Directive> {
        vec![
            Directive::Transaction(
                Transaction::new(date(2024, 1, 15), "Coffee")
                    .with_flag('*')
                    .with_payee("Coffee Shop")
                    .with_posting(Posting::new(
                        "Expenses:Food:Coffee",
                        Amount::new(dec!(5.00), "USD"),
                    ))
                    .with_posting(Posting::new(
                        "Assets:Bank:Checking",
                        Amount::new(dec!(-5.00), "USD"),
                    )),
            ),
            Directive::Transaction(
                Transaction::new(date(2024, 1, 16), "Groceries")
                    .with_flag('*')
                    .with_payee("Supermarket")
                    .with_posting(Posting::new(
                        "Expenses:Food:Groceries",
                        Amount::new(dec!(50.00), "USD"),
                    ))
                    .with_posting(Posting::new(
                        "Assets:Bank:Checking",
                        Amount::new(dec!(-50.00), "USD"),
                    )),
            ),
        ]
    }

    #[test]
    fn test_simple_select() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        let query = parse("SELECT date, account").unwrap();
        let result = executor.execute(&query).unwrap();

        assert_eq!(result.columns, vec!["date", "account"]);
        assert_eq!(result.len(), 4); // 2 transactions × 2 postings
    }

    #[test]
    fn test_where_clause() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        let query = parse("SELECT account WHERE account ~ \"Expenses:\"").unwrap();
        let result = executor.execute(&query).unwrap();

        assert_eq!(result.len(), 2); // Only expense postings
    }

    #[test]
    fn test_balances() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        let query = parse("BALANCES").unwrap();
        let result = executor.execute(&query).unwrap();

        assert_eq!(result.columns, vec!["account", "balance"]);
        assert!(result.len() >= 3); // At least 3 accounts
    }

    #[test]
    fn test_account_functions() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test LEAF function
        let query = parse("SELECT DISTINCT LEAF(account) WHERE account ~ \"Expenses:\"").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // Coffee, Groceries

        // Test ROOT function
        let query = parse("SELECT DISTINCT ROOT(account)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // Expenses, Assets

        // Test PARENT function
        let query = parse("SELECT DISTINCT PARENT(account) WHERE account ~ \"Expenses:\"").unwrap();
        let result = executor.execute(&query).unwrap();
        assert!(!result.is_empty()); // At least "Expenses:Food"
    }

    #[test]
    fn test_min_max_aggregate() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test MIN(date)
        let query = parse("SELECT MIN(date)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15)));

        // Test MAX(date)
        let query = parse("SELECT MAX(date)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 16)));
    }

    #[test]
    fn test_order_by() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        let query = parse("SELECT date, account ORDER BY date DESC").unwrap();
        let result = executor.execute(&query).unwrap();

        // Should have all postings, ordered by date descending
        assert_eq!(result.len(), 4);
        // First row should be from 2024-01-16 (later date)
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 16)));
    }

    #[test]
    fn test_hash_value_all_variants() {
        use rustledger_core::{Cost, Inventory, Position};

        // Test that all Value variants can be hashed without panic
        let values = vec![
            Value::String("test".to_string()),
            Value::Number(dec!(123.45)),
            Value::Integer(42),
            Value::Date(date(2024, 1, 15)),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::Amount(Amount::new(dec!(100), "USD")),
            Value::Position(Position::simple(Amount::new(dec!(10), "AAPL"))),
            Value::Position(Position::with_cost(
                Amount::new(dec!(10), "AAPL"),
                Cost::new(dec!(150), "USD"),
            )),
            Value::Inventory(Inventory::new()),
            Value::StringSet(vec!["tag1".to_string(), "tag2".to_string()]),
            Value::Null,
        ];

        // Hash each value and verify no panic
        for value in &values {
            let hash = hash_single_value(value);
            assert!(hash != 0 || matches!(value, Value::Null));
        }

        // Test that different values produce different hashes (usually)
        let hash1 = hash_single_value(&Value::String("a".to_string()));
        let hash2 = hash_single_value(&Value::String("b".to_string()));
        assert_ne!(hash1, hash2);

        // Test that same values produce same hashes
        let hash3 = hash_single_value(&Value::Integer(42));
        let hash4 = hash_single_value(&Value::Integer(42));
        assert_eq!(hash3, hash4);
    }

    #[test]
    fn test_hash_row_distinct() {
        // Test hash_row for DISTINCT deduplication
        let row1 = vec![Value::String("a".to_string()), Value::Integer(1)];
        let row2 = vec![Value::String("a".to_string()), Value::Integer(1)];
        let row3 = vec![Value::String("b".to_string()), Value::Integer(1)];

        assert_eq!(hash_row(&row1), hash_row(&row2));
        assert_ne!(hash_row(&row1), hash_row(&row3));
    }

    #[test]
    fn test_string_set_hash_order_independent() {
        // StringSet hash should be order-independent
        let set1 = Value::StringSet(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let set2 = Value::StringSet(vec!["c".to_string(), "a".to_string(), "b".to_string()]);
        let set3 = Value::StringSet(vec!["b".to_string(), "c".to_string(), "a".to_string()]);

        let hash1 = hash_single_value(&set1);
        let hash2 = hash_single_value(&set2);
        let hash3 = hash_single_value(&set3);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_inventory_hash_includes_cost() {
        use rustledger_core::{Cost, Inventory, Position};

        // Two inventories with same units but different costs should hash differently
        let mut inv1 = Inventory::new();
        inv1.add(Position::with_cost(
            Amount::new(dec!(10), "AAPL"),
            Cost::new(dec!(100), "USD"),
        ));

        let mut inv2 = Inventory::new();
        inv2.add(Position::with_cost(
            Amount::new(dec!(10), "AAPL"),
            Cost::new(dec!(200), "USD"),
        ));

        let hash1 = hash_single_value(&Value::Inventory(inv1));
        let hash2 = hash_single_value(&Value::Inventory(inv2));

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_distinct_deduplication() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Without DISTINCT - should have duplicates (same flag '*' for all)
        let query = parse("SELECT flag").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 4); // One per posting, all have flag '*'

        // With DISTINCT - should deduplicate
        let query = parse("SELECT DISTINCT flag").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 1); // Deduplicated to 1 (all '*')
    }

    #[test]
    fn test_limit_clause() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test LIMIT restricts result count
        let query = parse("SELECT date, account LIMIT 2").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2);

        // Test LIMIT 0 returns empty
        let query = parse("SELECT date LIMIT 0").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 0);

        // Test LIMIT larger than result set returns all
        let query = parse("SELECT date LIMIT 100").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_group_by_with_count() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Group by account root and count postings
        let query = parse("SELECT ROOT(account), COUNT(account) GROUP BY ROOT(account)").unwrap();
        let result = executor.execute(&query).unwrap();

        assert_eq!(result.columns.len(), 2);
        // Should have 2 groups: Assets and Expenses
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_count_aggregate() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Count all postings
        let query = parse("SELECT COUNT(account)").unwrap();
        let result = executor.execute(&query).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result.rows[0][0], Value::Integer(4));

        // Count with GROUP BY
        let query = parse("SELECT ROOT(account), COUNT(account) GROUP BY ROOT(account)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // Assets, Expenses
    }

    #[test]
    fn test_journal_query() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // JOURNAL for Expenses account
        let query = parse("JOURNAL \"Expenses\"").unwrap();
        let result = executor.execute(&query).unwrap();

        // Should have columns for journal output
        assert!(result.columns.contains(&"account".to_string()));
        // Should only show expense account entries
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_print_query() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // PRINT outputs formatted directives
        let query = parse("PRINT").unwrap();
        let result = executor.execute(&query).unwrap();

        // PRINT returns single column "directive" with formatted output
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0], "directive");
        // Should have one row per directive (2 transactions)
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_empty_directives() {
        let directives: Vec<Directive> = vec![];
        let mut executor = Executor::new(&directives);

        // SELECT on empty directives
        let query = parse("SELECT date, account").unwrap();
        let result = executor.execute(&query).unwrap();
        assert!(result.is_empty());

        // BALANCES on empty directives
        let query = parse("BALANCES").unwrap();
        let result = executor.execute(&query).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_comparison_operators() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Less than comparison on dates
        let query = parse("SELECT date WHERE date < 2024-01-16").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // First transaction postings

        // Greater than comparison on year
        let query = parse("SELECT date WHERE year > 2023").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 4); // All 2024 postings

        // Equality comparison on day
        let query = parse("SELECT account WHERE day = 15").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // First transaction postings (Jan 15)
    }

    #[test]
    fn test_logical_operators() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // AND operator
        let query = parse("SELECT account WHERE account ~ \"Expenses\" AND day > 14").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2); // Expense postings on Jan 15 and 16

        // OR operator
        let query = parse("SELECT account WHERE day = 15 OR day = 16").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 4); // All postings (both days)
    }

    #[test]
    fn test_arithmetic_expressions() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Negation on integer
        let query = parse("SELECT -day WHERE day = 15").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 2);
        // Day 15 negated should be -15
        for row in &result.rows {
            if let Value::Integer(n) = &row[0] {
                assert_eq!(*n, -15);
            }
        }
    }

    #[test]
    fn test_first_last_aggregates() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // FIRST aggregate
        let query = parse("SELECT FIRST(date)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15)));

        // LAST aggregate
        let query = parse("SELECT LAST(date)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 16)));
    }

    #[test]
    fn test_wildcard_select() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // SELECT * returns all postings with wildcard column name
        let query = parse("SELECT *").unwrap();
        let result = executor.execute(&query).unwrap();

        // Wildcard produces column name "*"
        assert_eq!(result.columns, vec!["*"]);
        // But each row has expanded values (date, flag, payee, narration, account, position)
        assert_eq!(result.len(), 4);
        assert_eq!(result.rows[0].len(), 6); // 6 expanded values
    }

    #[test]
    fn test_query_result_methods() {
        let mut result = QueryResult::new(vec!["col1".to_string(), "col2".to_string()]);

        // Initially empty
        assert!(result.is_empty());
        assert_eq!(result.len(), 0);

        // Add rows
        result.add_row(vec![Value::Integer(1), Value::String("a".to_string())]);
        assert!(!result.is_empty());
        assert_eq!(result.len(), 1);

        result.add_row(vec![Value::Integer(2), Value::String("b".to_string())]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_type_cast_functions() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test INT function
        let query = parse("SELECT int(5.7)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Integer(5));

        // Test DECIMAL function
        let query = parse("SELECT decimal(42)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Number(dec!(42)));

        // Test STR function
        let query = parse("SELECT str(123)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("123".to_string()));

        // Test BOOL function
        let query = parse("SELECT bool(1)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Boolean(true));

        let query = parse("SELECT bool(0)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Boolean(false));
    }

    #[test]
    fn test_meta_functions() {
        use std::collections::HashMap;

        // Create directives with metadata
        let mut txn_meta: HashMap<String, MetaValue> = HashMap::new();
        txn_meta.insert(
            "source".to_string(),
            MetaValue::String("bank_import".to_string()),
        );

        let mut posting_meta: HashMap<String, MetaValue> = HashMap::new();
        posting_meta.insert(
            "category".to_string(),
            MetaValue::String("food".to_string()),
        );

        let txn = Transaction {
            date: date(2024, 1, 15),
            flag: '*',
            payee: Some("Coffee Shop".into()),
            narration: "Coffee".into(),
            tags: vec![],
            links: vec![],
            meta: txn_meta,
            postings: vec![
                Posting {
                    account: "Expenses:Food".into(),
                    units: Some(rustledger_core::IncompleteAmount::Complete(Amount::new(
                        dec!(5),
                        "USD",
                    ))),
                    cost: None,
                    price: None,
                    flag: None,
                    meta: posting_meta,
                },
                Posting::new("Assets:Cash", Amount::new(dec!(-5), "USD")),
            ],
        };

        let directives = vec![Directive::Transaction(txn)];
        let mut executor = Executor::new(&directives);

        // Test META (posting metadata)
        let query = parse("SELECT meta('category') WHERE account ~ 'Expenses'").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("food".to_string()));

        // Test ENTRY_META (transaction metadata)
        let query = parse("SELECT entry_meta('source') WHERE account ~ 'Expenses'").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("bank_import".to_string()));

        // Test ANY_META (falls back to txn meta when posting meta missing)
        let query = parse("SELECT any_meta('source') WHERE account ~ 'Expenses'").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("bank_import".to_string()));

        // Test ANY_META (uses posting meta when available)
        let query = parse("SELECT any_meta('category') WHERE account ~ 'Expenses'").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("food".to_string()));

        // Test missing meta returns NULL
        let query = parse("SELECT meta('nonexistent') WHERE account ~ 'Expenses'").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);
    }

    #[test]
    fn test_convert_function() {
        // Create directives with price information
        let price = rustledger_core::Price {
            date: date(2024, 1, 1),
            currency: "EUR".into(),
            amount: Amount::new(dec!(1.10), "USD"),
            meta: HashMap::new(),
        };

        let txn = Transaction::new(date(2024, 1, 15), "Test")
            .with_flag('*')
            .with_posting(Posting::new("Assets:Euro", Amount::new(dec!(100), "EUR")))
            .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-110), "USD")));

        let directives = vec![Directive::Price(price), Directive::Transaction(txn)];
        let mut executor = Executor::new(&directives);

        // Test CONVERT with amount
        let query = parse("SELECT convert(position, 'USD') WHERE account ~ 'Euro'").unwrap();
        let result = executor.execute(&query).unwrap();
        // 100 EUR × 1.10 = 110 USD
        match &result.rows[0][0] {
            Value::Amount(a) => {
                assert_eq!(a.number, dec!(110));
                assert_eq!(a.currency.as_ref(), "USD");
            }
            _ => panic!("Expected Amount, got {:?}", result.rows[0][0]),
        }
    }

    #[test]
    fn test_date_functions() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test DATE construction from string
        let query = parse("SELECT date('2024-06-15')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 6, 15)));

        // Test DATE construction from components
        let query = parse("SELECT date(2024, 6, 15)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 6, 15)));

        // Test DATE_DIFF
        let query = parse("SELECT date_diff(date('2024-01-20'), date('2024-01-15'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Integer(5));

        // Test DATE_ADD
        let query = parse("SELECT date_add(date('2024-01-15'), 10)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 25)));

        // Test DATE_TRUNC year
        let query = parse("SELECT date_trunc('year', date('2024-06-15'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 1)));

        // Test DATE_TRUNC month
        let query = parse("SELECT date_trunc('month', date('2024-06-15'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 6, 1)));

        // Test DATE_PART
        let query = parse("SELECT date_part('month', date('2024-06-15'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Integer(6));

        // Test PARSE_DATE with custom format
        let query = parse("SELECT parse_date('15/06/2024', '%d/%m/%Y')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 6, 15)));

        // Test DATE_BIN with day stride
        let query =
            parse("SELECT date_bin('7 days', date('2024-01-15'), date('2024-01-01'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15))); // 15 is 14 days from 1, so bucket starts at 15

        // Test DATE_BIN with week stride
        let query =
            parse("SELECT date_bin('1 week', date('2024-01-20'), date('2024-01-01'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15))); // Week 3 starts at day 15

        // Test DATE_BIN with month stride
        let query =
            parse("SELECT date_bin('1 month', date('2024-06-15'), date('2024-01-01'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 6, 1))); // June bucket

        // Test DATE_BIN with year stride
        let query =
            parse("SELECT date_bin('1 year', date('2024-06-15'), date('2020-01-01'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 1))); // 2024 bucket
    }

    #[test]
    fn test_string_functions_extended() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test GREP - returns matched portion
        let query = parse("SELECT grep('Ex[a-z]+', 'Hello Expenses World')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("Expenses".to_string()));

        // Test GREP - no match returns NULL
        let query = parse("SELECT grep('xyz', 'Hello World')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);

        // Test GREPN - capture group (using [0-9] since \d is not escaped in BQL strings)
        let query = parse("SELECT grepn('([0-9]+)-([0-9]+)', '2024-01', 1)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("2024".to_string()));

        // Test SUBST - substitution
        let query = parse("SELECT subst('-', '/', '2024-01-15')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("2024/01/15".to_string()));

        // Test SPLITCOMP
        let query = parse("SELECT splitcomp('a:b:c', ':', 1)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("b".to_string()));

        // Test JOINSTR
        let query = parse("SELECT joinstr('hello', 'world')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("hello, world".to_string()));

        // Test MAXWIDTH - no truncation needed
        let query = parse("SELECT maxwidth('hello', 10)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("hello".to_string()));

        // Test MAXWIDTH - truncation with ellipsis
        let query = parse("SELECT maxwidth('hello world', 8)").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("hello...".to_string()));
    }

    #[test]
    fn test_inventory_functions() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test EMPTY on sum of position (sum across all postings may cancel out)
        // Use a filter to get non-canceling positions
        let query = parse("SELECT empty(sum(position)) WHERE account ~ 'Assets'").unwrap();
        let result = executor.execute(&query).unwrap();
        // Should be a boolean (the actual value depends on sample data)
        assert!(matches!(result.rows[0][0], Value::Boolean(_)));

        // Test EMPTY with null returns true
        // (null handling is already tested in the function)

        // Test POSSIGN with debit account (Assets) - no sign change
        let query = parse("SELECT possign(100, 'Assets:Bank')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Number(rust_decimal::Decimal::from(100))
        );

        // Test POSSIGN with credit account (Income) - sign is negated
        let query = parse("SELECT possign(100, 'Income:Salary')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Number(rust_decimal::Decimal::from(-100))
        );

        // Test POSSIGN with Expenses (debit normal) - no sign change
        let query = parse("SELECT possign(50, 'Expenses:Food')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Number(rust_decimal::Decimal::from(50))
        );

        // Test POSSIGN with Liabilities (credit normal) - sign is negated
        let query = parse("SELECT possign(200, 'Liabilities:CreditCard')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Number(rust_decimal::Decimal::from(-200))
        );

        // Test POSSIGN with Equity (credit normal) - sign is negated
        let query = parse("SELECT possign(300, 'Equity:OpeningBalances')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Number(rust_decimal::Decimal::from(-300))
        );
    }

    #[test]
    fn test_account_meta_functions() {
        use rustledger_core::{Close, Metadata, Open};

        // Create directives with Open/Close
        let mut open_meta = Metadata::new();
        open_meta.insert(
            "category".to_string(),
            MetaValue::String("checking".to_string()),
        );

        let directives = vec![
            Directive::Open(Open {
                date: date(2020, 1, 1),
                account: "Assets:Bank:Checking".into(),
                currencies: vec![],
                booking: None,
                meta: open_meta,
            }),
            Directive::Open(Open::new(date(2020, 2, 15), "Expenses:Food")),
            Directive::Close(Close::new(date(2024, 12, 31), "Assets:Bank:Checking")),
            // A transaction to have postings for the query context
            Directive::Transaction(
                Transaction::new(date(2024, 1, 15), "Coffee")
                    .with_posting(Posting::new(
                        "Expenses:Food",
                        Amount::new(dec!(5.00), "USD"),
                    ))
                    .with_posting(Posting::new(
                        "Assets:Bank:Checking",
                        Amount::new(dec!(-5.00), "USD"),
                    )),
            ),
        ];

        let mut executor = Executor::new(&directives);

        // Test OPEN_DATE - account with open directive
        let query = parse("SELECT open_date('Assets:Bank:Checking')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2020, 1, 1)));

        // Test CLOSE_DATE - account with close directive
        let query = parse("SELECT close_date('Assets:Bank:Checking')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Date(date(2024, 12, 31)));

        // Test OPEN_DATE - account without close directive
        let query = parse("SELECT close_date('Expenses:Food')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);

        // Test OPEN_META - get metadata from open directive
        let query = parse("SELECT open_meta('Assets:Bank:Checking', 'category')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::String("checking".to_string()));

        // Test OPEN_META - non-existent key
        let query = parse("SELECT open_meta('Assets:Bank:Checking', 'nonexistent')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);

        // Test with non-existent account
        let query = parse("SELECT open_date('NonExistent:Account')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);
    }

    #[test]
    fn test_source_location_columns_return_null_without_sources() {
        // When using the regular constructor (without source location support),
        // the filename, lineno, and location columns should return Null
        let directives = vec![Directive::Transaction(Transaction {
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            flag: '*',
            payee: Some("Test".into()),
            narration: "Test transaction".into(),
            tags: vec![],
            links: vec![],
            meta: Metadata::new(),
            postings: vec![
                Posting::new("Assets:Bank", Amount::new(dec!(100), "USD")),
                Posting::new("Expenses:Food", Amount::new(dec!(-100), "USD")),
            ],
        })];

        let mut executor = Executor::new(&directives);

        // Test filename column returns Null
        let query = parse("SELECT filename").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);

        // Test lineno column returns Null
        let query = parse("SELECT lineno").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);

        // Test location column returns Null
        let query = parse("SELECT location").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Null);
    }

    #[test]
    fn test_source_location_columns_with_sources() {
        use rustledger_loader::SourceMap;
        use rustledger_parser::Spanned;
        use std::sync::Arc;

        // Create a source map with a test file
        let mut source_map = SourceMap::new();
        let source: Arc<str> =
            "2024-01-15 * \"Test\"\n  Assets:Bank  100 USD\n  Expenses:Food".into();
        let file_id = source_map.add_file("test.beancount".into(), source);

        // Create a spanned directive
        let txn = Transaction {
            date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            flag: '*',
            payee: Some("Test".into()),
            narration: "Test transaction".into(),
            tags: vec![],
            links: vec![],
            meta: Metadata::new(),
            postings: vec![
                Posting::new("Assets:Bank", Amount::new(dec!(100), "USD")),
                Posting::new("Expenses:Food", Amount::new(dec!(-100), "USD")),
            ],
        };

        let spanned_directives = vec![Spanned {
            value: Directive::Transaction(txn),
            span: rustledger_parser::Span { start: 0, end: 50 },
            file_id: file_id as u16,
        }];

        let mut executor = Executor::new_with_sources(&spanned_directives, &source_map);

        // Test filename column returns the file path
        let query = parse("SELECT filename").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::String("test.beancount".to_string())
        );

        // Test lineno column returns line number
        let query = parse("SELECT lineno").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(result.rows[0][0], Value::Integer(1));

        // Test location column returns formatted location
        let query = parse("SELECT location").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::String("test.beancount:1".to_string())
        );
    }

    #[test]
    fn test_interval_function() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test interval with single argument (unit only, count=1)
        let query = parse("SELECT interval('month')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Interval(Interval::new(1, IntervalUnit::Month))
        );

        // Test interval with two arguments (count, unit)
        let query = parse("SELECT interval(3, 'day')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Interval(Interval::new(3, IntervalUnit::Day))
        );

        // Test interval with negative count
        let query = parse("SELECT interval(-2, 'week')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Interval(Interval::new(-2, IntervalUnit::Week))
        );
    }

    #[test]
    fn test_date_add_with_interval() {
        let directives = sample_directives();
        let mut executor = Executor::new(&directives);

        // Test date_add with interval
        let query = parse("SELECT date_add(date(2024, 1, 15), interval(1, 'month'))").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Date(NaiveDate::from_ymd_opt(2024, 2, 15).unwrap())
        );

        // Test date + interval using binary operator
        let query = parse("SELECT date(2024, 1, 15) + interval(1, 'year')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Date(NaiveDate::from_ymd_opt(2025, 1, 15).unwrap())
        );

        // Test date - interval
        let query = parse("SELECT date(2024, 3, 15) - interval(2, 'month')").unwrap();
        let result = executor.execute(&query).unwrap();
        assert_eq!(
            result.rows[0][0],
            Value::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
    }
}
