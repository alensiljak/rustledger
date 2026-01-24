//! Integration tests for the BQL query engine.
//!
//! Tests cover parsing, execution, aggregation, filtering, and real-world query scenarios.

use rust_decimal_macros::dec;
use rustledger_core::{Amount, Directive, NaiveDate, Open, Posting, Transaction};
use rustledger_query::{Executor, QueryResult, Value, parse};

// ============================================================================
// Helper Functions
// ============================================================================

#[allow(clippy::missing_const_for_fn)]
fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn make_test_directives() -> Vec<Directive> {
    vec![
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Bank:Checking")),
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Bank:Savings")),
        Directive::Open(Open::new(date(2024, 1, 1), "Expenses:Food")),
        Directive::Open(Open::new(date(2024, 1, 1), "Expenses:Transport")),
        Directive::Open(Open::new(date(2024, 1, 1), "Income:Salary")),
        // Transaction 1: Salary
        Directive::Transaction(
            Transaction::new(date(2024, 1, 15), "Monthly salary")
                .with_payee("Employer")
                .with_posting(Posting::new(
                    "Income:Salary",
                    Amount::new(dec!(-5000), "USD"),
                ))
                .with_posting(Posting::new(
                    "Assets:Bank:Checking",
                    Amount::new(dec!(5000), "USD"),
                )),
        ),
        // Transaction 2: Groceries
        Directive::Transaction(
            Transaction::new(date(2024, 1, 20), "Weekly groceries")
                .with_payee("Grocery Store")
                .with_tag("food")
                .with_posting(Posting::new("Expenses:Food", Amount::new(dec!(150), "USD")))
                .with_posting(Posting::new(
                    "Assets:Bank:Checking",
                    Amount::new(dec!(-150), "USD"),
                )),
        ),
        // Transaction 3: Gas
        Directive::Transaction(
            Transaction::new(date(2024, 1, 22), "Fill up")
                .with_payee("Gas Station")
                .with_posting(Posting::new(
                    "Expenses:Transport",
                    Amount::new(dec!(45), "USD"),
                ))
                .with_posting(Posting::new(
                    "Assets:Bank:Checking",
                    Amount::new(dec!(-45), "USD"),
                )),
        ),
        // Transaction 4: Transfer to savings
        Directive::Transaction(
            Transaction::new(date(2024, 1, 25), "Transfer to savings")
                .with_posting(Posting::new(
                    "Assets:Bank:Savings",
                    Amount::new(dec!(1000), "USD"),
                ))
                .with_posting(Posting::new(
                    "Assets:Bank:Checking",
                    Amount::new(dec!(-1000), "USD"),
                )),
        ),
        // Transaction 5: More groceries
        Directive::Transaction(
            Transaction::new(date(2024, 1, 27), "More groceries")
                .with_payee("Grocery Store")
                .with_tag("food")
                .with_posting(Posting::new("Expenses:Food", Amount::new(dec!(80), "USD")))
                .with_posting(Posting::new(
                    "Assets:Bank:Checking",
                    Amount::new(dec!(-80), "USD"),
                )),
        ),
    ]
}

fn execute_query(query_str: &str, directives: &[Directive]) -> QueryResult {
    let query = parse(query_str).expect("query should parse");
    let mut executor = Executor::new(directives);
    executor.execute(&query).expect("query should execute")
}

// ============================================================================
// Query Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_select() {
    let query = parse("SELECT account, number").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_parse_select_with_where() {
    let query = parse(r#"SELECT account WHERE account ~ "Expenses""#).expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_parse_select_with_group_by() {
    let query = parse("SELECT account, SUM(number) GROUP BY account").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_parse_select_with_order_by() {
    let query = parse("SELECT account, number ORDER BY number DESC").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_parse_journal_query() {
    let query = parse(r#"JOURNAL "Assets:Bank""#).expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Journal(_)));
}

#[test]
fn test_parse_balances_query() {
    let query = parse("BALANCES").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Balances(_)));
}

#[test]
fn test_parse_print_query() {
    let query = parse("PRINT").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Print(_)));
}

#[test]
fn test_parse_error_invalid_query() {
    let result = parse("INVALID QUERY SYNTAX");
    assert!(result.is_err());
}

// ============================================================================
// Query Execution Tests
// ============================================================================

#[test]
fn test_execute_select_account() {
    let directives = make_test_directives();
    let result = execute_query("SELECT account", &directives);

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "account");
}

#[test]
fn test_execute_select_multiple_columns() {
    let directives = make_test_directives();
    let result = execute_query("SELECT account, position", &directives);

    assert_eq!(result.columns.len(), 2);
    assert!(result.columns.contains(&"account".to_string()));
    assert!(result.columns.contains(&"position".to_string()));
}

#[test]
fn test_execute_select_with_filter() {
    let directives = make_test_directives();
    let result = execute_query(r#"SELECT account WHERE account ~ "Expenses""#, &directives);

    // All results should be expense accounts
    for row in &result.rows {
        if let Value::String(account) = &row[0] {
            assert!(
                account.starts_with("Expenses"),
                "expected Expenses account, got {account}"
            );
        }
    }
}

#[test]
fn test_execute_select_with_date_filter() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT date, narration WHERE date >= 2024-01-20",
        &directives,
    );

    // All results should be on or after Jan 20
    for row in &result.rows {
        if let Value::Date(d) = &row[0] {
            assert!(
                *d >= date(2024, 1, 20),
                "expected date >= 2024-01-20, got {d}"
            );
        }
    }
}

// ============================================================================
// Aggregation Tests
// ============================================================================

#[test]
fn test_execute_sum_aggregation() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account, SUM(position) WHERE account ~ "Expenses:Food" GROUP BY account"#,
        &directives,
    );

    // Should have one row for Expenses:Food
    assert!(!result.is_empty());

    // Find the Expenses:Food row
    let food_row = result.rows.iter().find(|row| {
        if let Value::String(account) = &row[0] {
            account == "Expenses:Food"
        } else {
            false
        }
    });

    assert!(food_row.is_some(), "should have Expenses:Food row");
}

#[test]
fn test_execute_count_aggregation() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account, COUNT(*) WHERE account ~ "Expenses" GROUP BY account"#,
        &directives,
    );

    assert!(!result.is_empty());
}

#[test]
fn test_execute_group_by_account() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT account, SUM(position) GROUP BY account",
        &directives,
    );

    // Should have grouped results
    assert!(!result.is_empty());

    // Check that we have unique accounts
    let accounts: Vec<&String> = result
        .rows
        .iter()
        .filter_map(|row| {
            if let Value::String(s) = &row[0] {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    // Each account should appear at most once
    let unique_accounts: std::collections::HashSet<_> = accounts.iter().collect();
    assert_eq!(accounts.len(), unique_accounts.len());
}

// ============================================================================
// Ordering Tests
// ============================================================================

#[test]
fn test_execute_order_by_date() {
    let directives = make_test_directives();
    let result = execute_query("SELECT date, narration ORDER BY date ASC", &directives);

    // Verify dates are in ascending order
    let dates: Vec<NaiveDate> = result
        .rows
        .iter()
        .filter_map(|row| {
            if let Value::Date(d) = &row[0] {
                Some(*d)
            } else {
                None
            }
        })
        .collect();

    for i in 1..dates.len() {
        assert!(
            dates[i] >= dates[i - 1],
            "dates should be in ascending order"
        );
    }
}

#[test]
fn test_execute_order_by_desc() {
    let directives = make_test_directives();
    let result = execute_query("SELECT date, narration ORDER BY date DESC", &directives);

    let dates: Vec<NaiveDate> = result
        .rows
        .iter()
        .filter_map(|row| {
            if let Value::Date(d) = &row[0] {
                Some(*d)
            } else {
                None
            }
        })
        .collect();

    for i in 1..dates.len() {
        assert!(
            dates[i] <= dates[i - 1],
            "dates should be in descending order"
        );
    }
}

// ============================================================================
// Function Tests
// ============================================================================

#[test]
fn test_execute_year_function() {
    let directives = make_test_directives();
    let result = execute_query("SELECT YEAR(date), narration", &directives);

    assert!(!result.is_empty());

    // All years should be 2024
    for row in &result.rows {
        if let Value::Integer(year) = &row[0] {
            assert_eq!(*year, 2024);
        }
    }
}

#[test]
fn test_execute_month_function() {
    let directives = make_test_directives();
    let result = execute_query("SELECT MONTH(date), narration", &directives);

    assert!(!result.is_empty());

    // All months should be 1 (January)
    for row in &result.rows {
        if let Value::Integer(month) = &row[0] {
            assert_eq!(*month, 1);
        }
    }
}

#[test]
fn test_execute_account_functions() {
    let directives = make_test_directives();
    let result = execute_query("SELECT account, ROOT(account), LEAF(account)", &directives);

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 3);
}

// ============================================================================
// JOURNAL Query Tests
// ============================================================================

#[test]
fn test_execute_journal_query() {
    let directives = make_test_directives();
    let query = parse(r#"JOURNAL "Assets:Bank:Checking""#).expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&query).expect("should execute");

    // Journal should show postings to Assets:Bank:Checking
    assert!(!result.is_empty());
}

// ============================================================================
// BALANCES Query Tests
// ============================================================================

#[test]
fn test_execute_balances_query() {
    let directives = make_test_directives();
    let query = parse("BALANCES").expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&query).expect("should execute");

    // Should have balances for all accounts
    assert!(!result.is_empty());
}

#[test]
fn test_execute_balances_with_from() {
    let directives = make_test_directives();
    let query = parse(r"BALANCES FROM OPEN ON 2024-01-01").expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&query).expect("should execute");

    // Should have balances
    assert!(!result.is_empty());
}

// ============================================================================
// Expression Tests
// ============================================================================

#[test]
fn test_execute_arithmetic_expression() {
    let directives = make_test_directives();
    let result = execute_query("SELECT NUMBER(position), NUMBER(position) * 2", &directives);

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 2);
}

#[test]
fn test_execute_comparison_in_where() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT account, NUMBER(position) WHERE NUMBER(position) > 100",
        &directives,
    );

    // All numbers should be > 100
    for row in &result.rows {
        if let Value::Number(n) = &row[1] {
            assert!(*n > dec!(100), "expected number > 100, got {n}");
        }
    }
}

#[test]
fn test_execute_and_condition() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account, NUMBER(position) WHERE account ~ "Expenses" AND NUMBER(position) > 50"#,
        &directives,
    );

    for row in &result.rows {
        if let (Value::String(account), Value::Number(n)) = (&row[0], &row[1]) {
            assert!(account.starts_with("Expenses"));
            assert!(*n > dec!(50));
        }
    }
}

#[test]
fn test_execute_or_condition() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account WHERE account ~ "Food" OR account ~ "Transport""#,
        &directives,
    );

    for row in &result.rows {
        if let Value::String(account) = &row[0] {
            assert!(
                account.contains("Food") || account.contains("Transport"),
                "expected Food or Transport account, got {account}"
            );
        }
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_execute_empty_result() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account WHERE account ~ "NonExistent""#,
        &directives,
    );

    assert!(result.is_empty());
}

#[test]
fn test_execute_with_no_directives() {
    let directives: Vec<Directive> = vec![];
    let result = execute_query("SELECT account", &directives);

    assert!(result.is_empty());
}

#[test]
fn test_execute_distinct() {
    let directives = make_test_directives();
    let result = execute_query("SELECT DISTINCT payee", &directives);

    // Should have unique payees
    let payees: Vec<&String> = result
        .rows
        .iter()
        .filter_map(|row| {
            if let Value::String(s) = &row[0] {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    let unique_payees: std::collections::HashSet<_> = payees.iter().collect();
    assert_eq!(payees.len(), unique_payees.len());
}

// ============================================================================
// Real-World Query Scenarios
// ============================================================================

#[test]
fn test_expense_summary_by_category() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account, SUM(position) WHERE account ~ "Expenses" GROUP BY account ORDER BY account"#,
        &directives,
    );

    assert!(!result.is_empty());
}

#[test]
fn test_monthly_spending() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT YEAR(date), MONTH(date), SUM(position) WHERE account ~ "Expenses" GROUP BY YEAR(date), MONTH(date)"#,
        &directives,
    );

    assert!(!result.is_empty());
}

#[test]
fn test_payee_analysis() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT payee, COUNT(*), SUM(position) GROUP BY payee",
        &directives,
    );

    assert!(!result.is_empty());
}

// ============================================================================
// Subquery Tests
// ============================================================================

#[test]
fn test_subquery_basic() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT * FROM (SELECT account, position WHERE account ~ \"Expenses:\")",
        &directives,
    );

    // Should return expenses postings from subquery
    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 2); // account, position
}

#[test]
fn test_subquery_with_aggregation() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT account, total FROM (SELECT account, SUM(position) AS total GROUP BY account)",
        &directives,
    );

    // Should have aggregated results from subquery
    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 2);
}

#[test]
fn test_subquery_with_inner_filter() {
    let directives = make_test_directives();
    // Get expense totals with filtering inside subquery
    let result = execute_query(
        "SELECT * FROM (SELECT account, SUM(position) AS total WHERE account ~ \"Expenses:\" GROUP BY account)",
        &directives,
    );

    assert!(!result.is_empty());
}

// ============================================================================
// HAVING Clause Tests
// ============================================================================

#[test]
fn test_having_basic() {
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT account, COUNT(*) AS cnt GROUP BY account HAVING cnt >= 2",
        &directives,
    );

    // Should only return accounts with count >= 2
    assert!(!result.is_empty());
    for row in &result.rows {
        if let Value::Integer(cnt) = &row[1] {
            assert!(*cnt >= 2, "expected count >= 2, got {cnt}");
        }
    }
}

#[test]
fn test_having_with_count() {
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT account, COUNT(*) AS cnt GROUP BY account HAVING cnt > 1",
        &directives,
    );

    // Should only return accounts with more than 1 posting
    for row in &result.rows {
        if let Value::Integer(cnt) = &row[1] {
            assert!(*cnt > 1, "expected count > 1, got {cnt}");
        }
    }
}

#[test]
fn test_having_filters_all() {
    let directives = make_test_directives();
    // Very high threshold that no account should meet
    let result = execute_query(
        r"SELECT account, COUNT(*) AS cnt GROUP BY account HAVING cnt > 999999",
        &directives,
    );

    assert!(
        result.is_empty(),
        "expected no results with very high threshold"
    );
}

// ============================================================================
// PIVOT BY Tests
// ============================================================================

#[test]
fn test_parse_pivot_by() {
    let query =
        parse("SELECT account, YEAR(date), SUM(position) GROUP BY 1, 2 PIVOT BY YEAR(date)")
            .expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

// ============================================================================
// Window Function Tests
// ============================================================================

#[test]
fn test_parse_window_function_row_number() {
    let query = parse("SELECT account, ROW_NUMBER() OVER (ORDER BY date)").expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_parse_window_function_with_partition() {
    let query = parse("SELECT account, ROW_NUMBER() OVER (PARTITION BY account ORDER BY date)")
        .expect("should parse");
    assert!(matches!(query, rustledger_query::Query::Select(_)));
}

#[test]
fn test_execute_window_row_number() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT date, narration, ROW_NUMBER() OVER (ORDER BY date) AS rn",
        &directives,
    );

    assert!(!result.is_empty());

    // Row numbers should be sequential
    let row_nums: Vec<i64> = result
        .rows
        .iter()
        .filter_map(|row| {
            if let Value::Integer(n) = &row[2] {
                Some(*n)
            } else {
                None
            }
        })
        .collect();

    for (i, &rn) in row_nums.iter().enumerate() {
        assert_eq!(
            rn,
            (i + 1) as i64,
            "expected row_number {}, got {rn}",
            i + 1
        );
    }
}

#[test]
fn test_execute_window_rank() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT account, RANK() OVER (ORDER BY account)",
        &directives,
    );

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 2);
}

#[test]
fn test_execute_window_dense_rank() {
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT account, DENSE_RANK() OVER (ORDER BY account)",
        &directives,
    );

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 2);
}

#[test]
fn test_execute_window_with_partition_by() {
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT account, date, ROW_NUMBER() OVER (PARTITION BY account ORDER BY date) AS rn",
        &directives,
    );

    assert!(!result.is_empty());
    // Each partition should have its own row numbering starting from 1
}

// ============================================================================
// Tags and Links Tests
// ============================================================================

#[test]
fn test_select_tags() {
    let directives = make_test_directives();
    // Transaction 2 has tag "food"
    let result = execute_query(
        r#"SELECT date, narration, tags WHERE "food" IN tags"#,
        &directives,
    );

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 3);
    // Should find the groceries transaction
    for row in &result.rows {
        if let Value::StringSet(tags) = &row[2] {
            assert!(
                tags.contains(&"food".to_string()),
                "expected 'food' in tags"
            );
        }
    }
}

#[test]
fn test_select_links() {
    // Create directives with a linked transaction
    let directives = vec![
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Bank")),
        Directive::Open(Open::new(date(2024, 1, 1), "Expenses:Food")),
        Directive::Transaction(
            Transaction::new(date(2024, 1, 15), "Linked transaction")
                .with_link("invoice-123")
                .with_posting(Posting::new("Expenses:Food", Amount::new(dec!(100), "USD")))
                .with_posting(Posting::new("Assets:Bank", Amount::new(dec!(-100), "USD"))),
        ),
    ];

    let result = execute_query(
        r#"SELECT date, narration, links WHERE "invoice-123" IN links"#,
        &directives,
    );

    assert!(!result.is_empty());
    assert_eq!(result.columns.len(), 3);
    for row in &result.rows {
        if let Value::StringSet(links) = &row[2] {
            assert!(
                links.contains(&"invoice-123".to_string()),
                "expected 'invoice-123' in links"
            );
        }
    }
}

#[test]
fn test_select_payee_and_narration() {
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT date, payee, narration WHERE payee = "Grocery Store""#,
        &directives,
    );

    assert!(!result.is_empty());
    for row in &result.rows {
        if let Value::String(payee) = &row[1] {
            assert_eq!(payee, "Grocery Store");
        }
        // Just verify narration is a non-empty string
        if let Value::String(narration) = &row[2] {
            assert!(!narration.is_empty(), "narration should not be empty");
        }
    }
}

// ============================================================================
// CREATE TABLE and INSERT Tests
// ============================================================================

#[test]
fn test_create_table_simple() {
    let directives = make_test_directives();
    let create_query = parse("CREATE TABLE test_table (col1, col2, col3)").expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&create_query).expect("should execute");

    assert_eq!(result.columns, vec!["result"]);
    assert_eq!(result.rows.len(), 1);
    if let Value::String(msg) = &result.rows[0][0] {
        assert!(msg.contains("Created table"));
    }
}

#[test]
fn test_create_table_as_select() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create a table from a SELECT query (using GROUP BY account which is simpler)
    let create_query =
        parse("CREATE TABLE balances AS SELECT account, sum(number) GROUP BY account")
            .expect("should parse");
    let result = executor.execute(&create_query).expect("should execute");

    assert_eq!(result.columns, vec!["result"]);
    if let Value::String(msg) = &result.rows[0][0] {
        assert!(msg.contains("Created table 'balances'"));
    }

    // Now select from the created table
    let select_query = parse("SELECT * FROM balances").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert!(!result.is_empty());
    assert_eq!(result.columns, vec!["account", "sum"]);
}

#[test]
fn test_insert_values() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create a table
    let create_query = parse("CREATE TABLE accounts (name, balance)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert values
    let insert_query = parse("INSERT INTO accounts VALUES ('Checking', 100), ('Savings', 500)")
        .expect("should parse");
    let result = executor.execute(&insert_query).expect("should execute");

    if let Value::String(msg) = &result.rows[0][0] {
        assert!(msg.contains("Inserted 2 row(s)"));
    }

    // Select from the table
    let select_query = parse("SELECT * FROM accounts").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0][0], Value::String("Checking".to_string()));
    // Numbers are parsed as Decimal (Number type)
    assert_eq!(result.rows[0][1], Value::Number(dec!(100)));
    assert_eq!(result.rows[1][0], Value::String("Savings".to_string()));
    assert_eq!(result.rows[1][1], Value::Number(dec!(500)));
}

#[test]
fn test_insert_select() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create a table from SELECT
    let create_query = parse("CREATE TABLE expenses (account)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert from a SELECT query
    let insert_query =
        parse("INSERT INTO expenses SELECT DISTINCT account WHERE account ~ 'Expenses:'")
            .expect("should parse");
    let result = executor.execute(&insert_query).expect("should execute");

    if let Value::String(msg) = &result.rows[0][0] {
        assert!(msg.contains("Inserted"));
    }

    // Select from the table
    let select_query = parse("SELECT * FROM expenses").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert!(!result.is_empty());
    for row in &result.rows {
        if let Value::String(acct) = &row[0] {
            assert!(acct.starts_with("Expenses:"));
        }
    }
}

#[test]
fn test_select_from_table_with_where() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate a table
    let create_query = parse("CREATE TABLE items (name, price)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse("INSERT INTO items VALUES ('Apple', 1), ('Banana', 2), ('Cherry', 5)")
        .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Select with a WHERE clause
    let select_query = parse("SELECT name FROM items WHERE price > 1").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.rows.len(), 2);
    let names: Vec<_> = result.rows.iter().map(|r| &r[0]).collect();
    assert!(names.contains(&&Value::String("Banana".to_string())));
    assert!(names.contains(&&Value::String("Cherry".to_string())));
}

#[test]
fn test_select_from_table_with_order_limit() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate a table
    let create_query = parse("CREATE TABLE nums (value)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO nums VALUES (3), (1), (4), (1), (5)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Select with ORDER BY and LIMIT
    let select_query =
        parse("SELECT value FROM nums ORDER BY value DESC LIMIT 3").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.rows.len(), 3);
    // Numbers are parsed as Decimal (Number type)
    assert_eq!(result.rows[0][0], Value::Number(dec!(5)));
    assert_eq!(result.rows[1][0], Value::Number(dec!(4)));
    assert_eq!(result.rows[2][0], Value::Number(dec!(3)));
}

#[test]
fn test_create_table_duplicate_error() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let create_query = parse("CREATE TABLE mytable (col1)").expect("should parse");
    executor
        .execute(&create_query)
        .expect("should execute first time");

    // Try to create the same table again - should error
    let result = executor.execute(&create_query);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("already exists"));
    }
}

#[test]
fn test_insert_table_not_exists_error() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let insert_query = parse("INSERT INTO nonexistent VALUES (1)").expect("should parse");
    let result = executor.execute(&insert_query);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("does not exist"));
    }
}

#[test]
fn test_select_table_not_exists_error() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let select_query = parse("SELECT * FROM nonexistent").expect("should parse");
    let result = executor.execute(&select_query);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("does not exist"));
    }
}

// ============================================================================
// Interval Function Tests
// ============================================================================

#[test]
fn test_interval_basic_construction() {
    use rustledger_query::{Interval, IntervalUnit};

    let directives = make_test_directives();
    let result = execute_query("SELECT interval(1, 'day') LIMIT 1", &directives);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(1, IntervalUnit::Day))
    );
}

#[test]
fn test_interval_all_units() {
    use rustledger_query::{Interval, IntervalUnit};

    let directives = make_test_directives();

    // Day
    let result = execute_query("SELECT interval(5, 'day') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(5, IntervalUnit::Day))
    );

    // Week
    let result = execute_query("SELECT interval(2, 'week') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(2, IntervalUnit::Week))
    );

    // Month
    let result = execute_query("SELECT interval(3, 'month') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(3, IntervalUnit::Month))
    );

    // Quarter
    let result = execute_query("SELECT interval(4, 'quarter') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(4, IntervalUnit::Quarter))
    );

    // Year
    let result = execute_query("SELECT interval(1, 'year') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(1, IntervalUnit::Year))
    );
}

#[test]
fn test_interval_negative() {
    use rustledger_query::{Interval, IntervalUnit};

    let directives = make_test_directives();

    let result = execute_query("SELECT interval(-7, 'day') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(-7, IntervalUnit::Day))
    );
}

#[test]
fn test_interval_invalid_unit() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let query = parse("SELECT interval(1, 'invalid_unit')").expect("should parse");
    let result = executor.execute(&query);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("invalid interval unit"));
    }
}

#[test]
fn test_interval_date_arithmetic() {
    let directives = make_test_directives();

    // Date + interval (days)
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(10, 'day') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 25)));

    // Date + interval (months)
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(2, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 3, 15)));

    // Date - interval (days)
    let result = execute_query(
        "SELECT date('2024-01-15') - interval(5, 'day') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 10)));

    // Date - interval (months)
    let result = execute_query(
        "SELECT date('2024-03-15') - interval(1, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 2, 15)));
}

#[test]
fn test_interval_decimal_count_error() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Decimal count should fail - must be an integer
    let query = parse("SELECT interval(3.5, 'day')").expect("should parse");
    let result = executor.execute(&query);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("must be an integer"));
    }
}

// ============================================================================
// INSERT Column Mapping Tests
// ============================================================================

#[test]
fn test_insert_with_reordered_columns() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table with col1, col2
    let create_query = parse("CREATE TABLE test_reorder (col1, col2)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert with columns in reverse order
    let insert_query = parse("INSERT INTO test_reorder (col2, col1) VALUES ('second', 'first')")
        .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Verify the values are in the correct positions
    let select_query = parse("SELECT col1, col2 FROM test_reorder").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::String("first".to_string()));
    assert_eq!(result.rows[0][1], Value::String("second".to_string()));
}

#[test]
fn test_insert_with_column_subset() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table with 3 columns
    let create_query = parse("CREATE TABLE test_subset (a, b, c)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert only into column 'b' - others should be NULL
    let insert_query =
        parse("INSERT INTO test_subset (b) VALUES ('middle')").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Verify the values
    let select_query = parse("SELECT a, b, c FROM test_subset").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::Null);
    assert_eq!(result.rows[0][1], Value::String("middle".to_string()));
    assert_eq!(result.rows[0][2], Value::Null);
}

#[test]
fn test_insert_invalid_column_error() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table
    let create_query = parse("CREATE TABLE test_invalid (col1, col2)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert with non-existent column
    let insert_query =
        parse("INSERT INTO test_invalid (nonexistent) VALUES ('value')").expect("should parse");
    let result = executor.execute(&insert_query);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("does not exist"));
    }
}

// ============================================================================
// SELECT FROM Table Aggregation Tests
// ============================================================================

#[test]
fn test_select_from_table_all_rows() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query = parse("CREATE TABLE numbers (value)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO numbers VALUES (1), (2), (3), (4), (5)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Select all values and verify row count
    let result = executor
        .execute(&parse("SELECT value FROM numbers").expect("should parse"))
        .expect("should execute");
    assert_eq!(result.len(), 5);
}

#[test]
fn test_select_from_table_filter() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query = parse("CREATE TABLE prices (category, price)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse(
        "INSERT INTO prices VALUES ('food', 10), ('food', 20), ('transport', 15), ('transport', 25)",
    )
    .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Filter by category
    let result = executor
        .execute(
            &parse("SELECT price FROM prices WHERE category = 'food' ORDER BY price")
                .expect("should parse"),
        )
        .expect("should execute");

    assert_eq!(result.len(), 2);
    assert_eq!(result.rows[0][0], Value::Number(dec!(10)));
    assert_eq!(result.rows[1][0], Value::Number(dec!(20)));
}

#[test]
fn test_select_from_table_distinct() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table with duplicates
    let create_query = parse("CREATE TABLE items (name)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO items VALUES ('apple'), ('banana'), ('apple'), ('cherry'), ('banana')")
            .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // SELECT DISTINCT
    let result = executor
        .execute(&parse("SELECT DISTINCT name FROM items ORDER BY name").expect("should parse"))
        .expect("should execute");

    assert_eq!(result.len(), 3);
    assert_eq!(result.rows[0][0], Value::String("apple".to_string()));
    assert_eq!(result.rows[1][0], Value::String("banana".to_string()));
    assert_eq!(result.rows[2][0], Value::String("cherry".to_string()));
}

// ============================================================================
// Interval Edge Case Tests
// ============================================================================

#[test]
fn test_interval_zero() {
    use rustledger_query::{Interval, IntervalUnit};

    let directives = make_test_directives();

    // Zero interval should work
    let result = execute_query("SELECT interval(0, 'day') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(0, IntervalUnit::Day))
    );

    // Date + zero interval should return same date
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(0, 'day') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15)));

    // Zero months
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(0, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 1, 15)));
}

#[test]
fn test_interval_month_end_arithmetic() {
    let directives = make_test_directives();

    // Jan 31 + 1 month = Feb 29 (2024 is leap year)
    let result = execute_query(
        "SELECT date('2024-01-31') + interval(1, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 2, 29)));

    // Mar 31 - 1 month = Feb 29 (2024 is leap year)
    let result = execute_query(
        "SELECT date('2024-03-31') - interval(1, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 2, 29)));

    // Jan 31 + 1 month in non-leap year = Feb 28
    let result = execute_query(
        "SELECT date('2023-01-31') + interval(1, 'month') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2023, 2, 28)));
}

#[test]
fn test_interval_quarter_arithmetic() {
    let directives = make_test_directives();

    // Jan 15 + 1 quarter = Apr 15
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(1, 'quarter') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 4, 15)));

    // Jan 15 + 2 quarters = Jul 15
    let result = execute_query(
        "SELECT date('2024-01-15') + interval(2, 'quarter') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 7, 15)));

    // Oct 15 - 2 quarters = Apr 15
    let result = execute_query(
        "SELECT date('2024-10-15') - interval(2, 'quarter') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2024, 4, 15)));
}

#[test]
fn test_interval_year_arithmetic() {
    let directives = make_test_directives();

    // Regular year arithmetic
    let result = execute_query(
        "SELECT date('2024-06-15') + interval(1, 'year') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2025, 6, 15)));

    // Feb 29 + 1 year = Feb 28 (2025 is not a leap year)
    let result = execute_query(
        "SELECT date('2024-02-29') + interval(1, 'year') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2025, 2, 28)));

    // Year subtraction
    let result = execute_query(
        "SELECT date('2024-06-15') - interval(2, 'year') LIMIT 1",
        &directives,
    );
    assert_eq!(result.rows[0][0], Value::Date(date(2022, 6, 15)));
}

#[test]
fn test_interval_case_insensitive_unit() {
    use rustledger_query::{Interval, IntervalUnit};

    let directives = make_test_directives();

    // Uppercase
    let result = execute_query("SELECT interval(1, 'DAY') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(1, IntervalUnit::Day))
    );

    // Mixed case
    let result = execute_query("SELECT interval(1, 'Month') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(1, IntervalUnit::Month))
    );

    // Short form
    let result = execute_query("SELECT interval(1, 'd') LIMIT 1", &directives);
    assert_eq!(
        result.rows[0][0],
        Value::Interval(Interval::new(1, IntervalUnit::Day))
    );
}

// ============================================================================
// INSERT Column Mapping Extended Tests
// ============================================================================

#[test]
fn test_insert_multiple_rows_with_columns() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table
    let create_query = parse("CREATE TABLE multi_insert (col1, col2)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert multiple rows with reordered columns
    let insert_query =
        parse("INSERT INTO multi_insert (col2, col1) VALUES ('a', 'b'), ('c', 'd'), ('e', 'f')")
            .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Verify all rows are correctly mapped
    let select_query =
        parse("SELECT col1, col2 FROM multi_insert ORDER BY col1").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 3);
    assert_eq!(result.rows[0][0], Value::String("b".to_string()));
    assert_eq!(result.rows[0][1], Value::String("a".to_string()));
    assert_eq!(result.rows[1][0], Value::String("d".to_string()));
    assert_eq!(result.rows[1][1], Value::String("c".to_string()));
    assert_eq!(result.rows[2][0], Value::String("f".to_string()));
    assert_eq!(result.rows[2][1], Value::String("e".to_string()));
}

#[test]
fn test_insert_column_case_insensitive() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table with lowercase column names
    let create_query = parse("CREATE TABLE case_test (name, value)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert with uppercase column names
    let insert_query =
        parse("INSERT INTO case_test (NAME, VALUE) VALUES ('test', 123)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Verify insert worked
    let select_query = parse("SELECT name, value FROM case_test").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::String("test".to_string()));
}

#[test]
fn test_insert_natural_column_order() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table
    let create_query = parse("CREATE TABLE natural_order (a, b, c)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert in natural order (same as table definition)
    let insert_query =
        parse("INSERT INTO natural_order (a, b, c) VALUES ('x', 'y', 'z')").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Verify correct ordering
    let select_query = parse("SELECT a, b, c FROM natural_order").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::String("x".to_string()));
    assert_eq!(result.rows[0][1], Value::String("y".to_string()));
    assert_eq!(result.rows[0][2], Value::String("z".to_string()));
}

// ============================================================================
// SELECT FROM Table Extended Tests
// ============================================================================

#[test]
fn test_select_from_empty_table() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create empty table
    let create_query = parse("CREATE TABLE empty_table (col)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Select from empty table should return 0 rows
    let select_query = parse("SELECT col FROM empty_table").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 0);
}

#[test]
fn test_select_multi_column() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query =
        parse("CREATE TABLE products (name, price, category)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO products VALUES ('Apple', 1.50, 'fruit'), ('Bread', 2.00, 'bakery')")
            .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Select multiple columns
    let select_query =
        parse("SELECT name, price, category FROM products ORDER BY name").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 2);
    assert_eq!(result.columns, vec!["name", "price", "category"]);
    assert_eq!(result.rows[0][0], Value::String("Apple".to_string()));
    assert_eq!(result.rows[0][1], Value::Number(dec!(1.50)));
    assert_eq!(result.rows[0][2], Value::String("fruit".to_string()));
}

#[test]
fn test_select_order_by_desc() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query = parse("CREATE TABLE scores (name, score)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse("INSERT INTO scores VALUES ('Alice', 85), ('Bob', 92), ('Carol', 78)")
        .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Order by score descending
    let select_query =
        parse("SELECT name, score FROM scores ORDER BY score DESC").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 3);
    assert_eq!(result.rows[0][0], Value::String("Bob".to_string())); // 92
    assert_eq!(result.rows[1][0], Value::String("Alice".to_string())); // 85
    assert_eq!(result.rows[2][0], Value::String("Carol".to_string())); // 78
}

#[test]
fn test_select_with_limit() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query = parse("CREATE TABLE many_rows (val)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO many_rows VALUES (1), (2), (3), (4), (5), (6), (7), (8), (9), (10)")
            .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Select with limit
    let select_query = parse("SELECT val FROM many_rows LIMIT 3").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 3);
}

#[test]
fn test_select_distinct_with_nulls() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table with 3 columns (we'll only populate first column)
    let create_query = parse("CREATE TABLE nulls_test (a, b)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert using column subset - 'b' will be NULL for all rows
    let insert_query =
        parse("INSERT INTO nulls_test (a) VALUES ('x'), ('y'), ('x')").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // DISTINCT on column 'b' should return single NULL
    let select_query = parse("SELECT DISTINCT b FROM nulls_test").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::Null);
}

#[test]
fn test_select_where_is_null() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table
    let create_query = parse("CREATE TABLE null_filter (name, value)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    // Insert some rows with NULL values (using column subset)
    let insert_query =
        parse("INSERT INTO null_filter (name) VALUES ('has_null')").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    let insert_query2 =
        parse("INSERT INTO null_filter VALUES ('has_value', 42)").expect("should parse");
    executor.execute(&insert_query2).expect("should execute");

    // Filter for NULL values
    let select_query =
        parse("SELECT name FROM null_filter WHERE value IS NULL").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::String("has_null".to_string()));
}

#[test]
fn test_select_complex_where() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create and populate table
    let create_query =
        parse("CREATE TABLE inventory (item, price, category)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse(
        "INSERT INTO inventory VALUES ('Apple', 1, 'fruit'), ('Steak', 15, 'meat'), ('Banana', 2, 'fruit'), ('Chicken', 8, 'meat')",
    )
    .expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Complex WHERE with AND
    let select_query =
        parse("SELECT item FROM inventory WHERE price > 5 AND category = 'meat' ORDER BY item")
            .expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 2);
    assert_eq!(result.rows[0][0], Value::String("Chicken".to_string()));
    assert_eq!(result.rows[1][0], Value::String("Steak".to_string()));
}
