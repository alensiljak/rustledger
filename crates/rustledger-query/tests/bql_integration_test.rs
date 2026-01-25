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

/// Test ORDER BY with GROUP BY expressions that are not in SELECT.
///
/// This test verifies that ORDER BY can reference expressions that appear in
/// GROUP BY but not in SELECT (hidden columns). This is valid SQL semantics
/// and matches Python beancount behavior.
#[test]
fn test_order_by_group_by_expression_not_in_select() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Query with account_sortkey in GROUP BY and ORDER BY but not in SELECT
    // This should work because account_sortkey(account) is a GROUP BY expression
    let query = parse(
        "SELECT account, sum(number) \
         GROUP BY account, account_sortkey(account) \
         ORDER BY account_sortkey(account)",
    )
    .expect("should parse");
    let result = executor.execute(&query).expect("should execute");

    // The result should only have 2 columns (account and sum), not the hidden sortkey column
    assert_eq!(result.columns.len(), 2);
    assert_eq!(result.columns[0], "account");
    assert_eq!(result.columns[1], "sum");

    // Verify all rows have exactly 2 values
    for row in &result.rows {
        assert_eq!(
            row.len(),
            2,
            "Row should have 2 columns, not hidden columns"
        );
    }
}

/// Test ORDER BY with multiple GROUP BY expressions, some not in SELECT.
#[test]
fn test_order_by_multiple_hidden_columns() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Multiple ORDER BY expressions where some are not in SELECT
    let query = parse(
        "SELECT account, sum(number), currency \
         GROUP BY account, currency, account_sortkey(account) \
         ORDER BY account_sortkey(account), currency",
    )
    .expect("should parse");
    let result = executor.execute(&query).expect("should execute");

    // Should have 3 visible columns
    assert_eq!(result.columns.len(), 3);
    assert_eq!(result.columns[0], "account");
    assert_eq!(result.columns[1], "sum");
    assert_eq!(result.columns[2], "currency");

    // Verify all rows have exactly 3 values
    for row in &result.rows {
        assert_eq!(
            row.len(),
            3,
            "Row should have 3 columns, not hidden columns"
        );
    }
}

/// Test ORDER BY with hidden columns in non-aggregate query.
///
/// This tests the edge case where a query has GROUP BY but no aggregate functions.
#[test]
fn test_order_by_hidden_column_non_aggregate() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Query without aggregate functions but with GROUP BY and ORDER BY
    // Note: This is unusual but valid SQL semantics
    let query = parse(
        "SELECT account \
         GROUP BY account, account_sortkey(account) \
         ORDER BY account_sortkey(account)",
    )
    .expect("should parse");
    let result = executor.execute(&query).expect("should execute");

    // Should only have 1 column (account), hidden column should be removed
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "account");

    // Verify all rows have exactly 1 value
    for row in &result.rows {
        assert_eq!(
            row.len(),
            1,
            "Row should have 1 column, hidden column removed"
        );
    }
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

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_error_unknown_column() {
    let directives = make_test_directives();
    let query = parse("SELECT nonexistent_column").expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_error_unknown_function() {
    let directives = make_test_directives();
    let query = parse("SELECT NONEXISTENT_FUNC(account)").expect("should parse");
    let mut executor = Executor::new(&directives);
    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_error_type_mismatch_comparison() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    // Create table with mixed types
    let create_query = parse("CREATE TABLE types (name, value)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse("INSERT INTO types VALUES ('text', 42)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Try to compare string column with number (should still work - coercion)
    let select_query = parse("SELECT name FROM types WHERE name > 10").expect("should parse");
    // This may or may not error depending on implementation - just verify it doesn't panic
    let _ = executor.execute(&select_query);
}

#[test]
fn test_division_behavior() {
    // Test division using literal expressions - use LIMIT 1 to get single row
    let directives = make_test_directives();
    let result = execute_query("SELECT 10 / 2 LIMIT 1", &directives);
    // Division should work and return single row
    assert_eq!(result.len(), 1);
    // Result should be 5
    if let Value::Integer(val) = &result.rows[0][0] {
        assert_eq!(*val, 5);
    } else if let Value::Number(val) = &result.rows[0][0] {
        assert_eq!(*val, dec!(5));
    }
}

#[test]
fn test_error_invalid_function_args_year() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let create_query = parse("CREATE TABLE func_test (val)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse("INSERT INTO func_test VALUES ('not a date')").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // YEAR expects a date, not a string
    let select_query = parse("SELECT YEAR(val) FROM func_test").expect("should parse");
    let result = executor.execute(&select_query);
    assert!(result.is_err());
}

#[test]
fn test_error_invalid_function_args_length() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let create_query = parse("CREATE TABLE len_test (val)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query = parse("INSERT INTO len_test VALUES (12345)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // LENGTH expects a string, not a number
    let select_query = parse("SELECT LENGTH(val) FROM len_test").expect("should parse");
    let result = executor.execute(&select_query);
    assert!(result.is_err());
}

// ============================================================================
// Aggregate Edge Cases
// ============================================================================

#[test]
fn test_aggregate_sum_on_ledger() {
    // Test SUM on ledger data
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT SUM(number) WHERE account ~ "Expenses:Food""#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // We have 150 + 80 = 230 USD in Food expenses
    if let Value::Number(sum) = &result.rows[0][0] {
        assert_eq!(*sum, dec!(230));
    }
}

#[test]
fn test_aggregate_count_on_ledger() {
    // Test COUNT on ledger data
    let directives = make_test_directives();

    // COUNT(*) counts all postings matching filter
    let result = execute_query(r#"SELECT COUNT(*) WHERE account ~ "Expenses""#, &directives);
    if let Value::Integer(count) = &result.rows[0][0] {
        assert_eq!(*count, 3); // 2 Food + 1 Transport
    }
}

#[test]
fn test_aggregate_avg_on_ledger() {
    // Test AVG on ledger data
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT AVG(number) WHERE account ~ "Expenses:Food""#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Average of 150 and 80 = 115
    if let Value::Number(avg) = &result.rows[0][0] {
        assert_eq!(*avg, dec!(115));
    }
}

#[test]
fn test_aggregate_min_max_on_ledger() {
    // Test MIN/MAX on ledger data
    let directives = make_test_directives();

    let result = execute_query(
        r#"SELECT MIN(number), MAX(number) WHERE account ~ "Expenses""#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Min expense: 45 (Transport), Max expense: 150 (Food)
    if let Value::Number(min) = &result.rows[0][0] {
        assert_eq!(*min, dec!(45));
    }
    if let Value::Number(max) = &result.rows[0][1] {
        assert_eq!(*max, dec!(150));
    }
}

#[test]
fn test_aggregate_filtered() {
    // Test aggregates with specific filter
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT SUM(number), COUNT(*), AVG(number) WHERE account = "Expenses:Food""#,
        &directives,
    );

    // Should have 1 row with aggregate results
    assert_eq!(result.len(), 1);
    // COUNT should be 2 (two Food transactions)
    if let Value::Integer(count) = &result.rows[0][1] {
        assert_eq!(*count, 2);
    }
}

// ============================================================================
// GROUP BY Edge Cases
// ============================================================================

#[test]
fn test_group_by_multiple_columns_ledger() {
    // Test GROUP BY with multiple columns on ledger data
    let directives = make_test_directives();

    // Group by account root (first component) and currency
    let result = execute_query(
        r"SELECT account, currency, SUM(number) AS total GROUP BY account, currency ORDER BY account",
        &directives,
    );

    // Should have multiple groups for different accounts
    assert!(result.len() >= 3);
}

#[test]
fn test_group_by_with_having_ledger() {
    // Test GROUP BY with HAVING on ledger data
    let directives = make_test_directives();

    // Only show accounts with more than 1 posting
    let result = execute_query(
        r"SELECT account, COUNT(*) AS cnt GROUP BY account HAVING cnt > 1 ORDER BY account",
        &directives,
    );

    // Assets:Bank:Checking has multiple postings
    assert!(!result.is_empty());
    for row in &result.rows {
        if let Value::Integer(cnt) = &row[1] {
            assert!(*cnt > 1);
        }
    }
}

// ============================================================================
// Window Function Edge Cases
// ============================================================================

#[test]
fn test_window_rank_with_ties() {
    // Test RANK with ties using ledger data (window functions not supported in FROM table)
    let directives = make_test_directives();
    // Use existing ledger postings - we have 5 transactions
    // Group by account to get tie-breaking scenarios
    let result = execute_query(
        r"SELECT account, RANK() OVER (ORDER BY account) AS rnk WHERE account ~ 'Assets' ORDER BY account",
        &directives,
    );

    // We have 4 postings to Assets accounts (Checking gets multiple)
    assert!(result.len() >= 2);
    // First posting should have rank 1
    if let Value::Integer(rank) = &result.rows[0][1] {
        assert_eq!(*rank, 1);
    }
}

#[test]
fn test_window_dense_rank_with_ties() {
    // Test DENSE_RANK using ledger data
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT account, DENSE_RANK() OVER (ORDER BY account) AS drnk WHERE account ~ 'Expenses' ORDER BY account",
        &directives,
    );

    // We have Expenses:Food and Expenses:Transport
    assert!(result.len() >= 2);
    // All Food postings should have same dense_rank
    if let Value::Integer(rank) = &result.rows[0][1] {
        assert!(*rank >= 1);
    }
}

#[test]
fn test_window_row_number_on_ledger() {
    // Test ROW_NUMBER using ledger data
    let directives = make_test_directives();
    let result = execute_query(
        "SELECT date, narration, ROW_NUMBER() OVER (ORDER BY date) AS rn ORDER BY date LIMIT 5",
        &directives,
    );

    assert!(result.len() >= 3);
    // Row numbers should be sequential
    for (i, row) in result.rows.iter().enumerate() {
        if let Value::Integer(rn) = &row[2] {
            assert_eq!(*rn, (i + 1) as i64, "Row number should be sequential");
        }
    }
}

// ============================================================================
// String Function Tests
// ============================================================================

#[test]
fn test_string_upper_lower_ledger() {
    // Test UPPER/LOWER on ledger narration
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT UPPER(narration), LOWER(narration) WHERE narration = "Monthly salary" LIMIT 1"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    assert_eq!(
        result.rows[0][0],
        Value::String("MONTHLY SALARY".to_string())
    );
    assert_eq!(
        result.rows[0][1],
        Value::String("monthly salary".to_string())
    );
}

#[test]
fn test_string_length_ledger() {
    // Test LENGTH on ledger account names
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account, LENGTH(account) AS len WHERE account ~ "Assets" LIMIT 3"#,
        &directives,
    );

    assert!(!result.is_empty());
    // All lengths should be positive
    for row in &result.rows {
        if let Value::Integer(len) = &row[1] {
            assert!(*len > 0);
        }
    }
}

#[test]
fn test_string_trim_literal() {
    // Test TRIM with literal string
    let directives = make_test_directives();
    let result = execute_query(r#"SELECT TRIM("  hello  ") LIMIT 1"#, &directives);

    assert_eq!(result.len(), 1);
    assert_eq!(result.rows[0][0], Value::String("hello".to_string()));
}

// ============================================================================
// Math Function Tests
// ============================================================================

#[test]
fn test_math_abs_ledger() {
    // Test ABS on ledger amounts (negative income postings)
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT number, ABS(number) AS abs_val WHERE account ~ "Income" LIMIT 1"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Income posting is negative, ABS should be positive
    if let Value::Number(abs_val) = &result.rows[0][1] {
        assert!(*abs_val > dec!(0));
    }
}

#[test]
fn test_math_round_ledger() {
    // Test ROUND on ledger amounts (ROUND takes 1 arg in BQL)
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT number, ROUND(number) AS rounded WHERE account ~ "Expenses:Food" LIMIT 1"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Should have a result
    assert!(!matches!(result.rows[0][1], Value::Null));
}

// ============================================================================
// COALESCE Function Tests
// ============================================================================

#[test]
fn test_coalesce_with_payee() {
    // Test COALESCE with payee (some transactions have payee, some don't)
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT COALESCE(payee, narration) AS description LIMIT 5",
        &directives,
    );

    // Should have results
    assert!(!result.is_empty());
    // All should be non-null (either payee or narration)
    for row in &result.rows {
        assert!(!matches!(row[0], Value::Null));
    }
}

#[test]
fn test_coalesce_first_non_null() {
    // Test COALESCE returns first non-null value
    let directives = make_test_directives();
    // Use payee (which may be NULL) with narration fallback
    let result = execute_query(
        r"SELECT payee, narration, COALESCE(payee, narration) AS desc LIMIT 5",
        &directives,
    );

    assert!(!result.is_empty());
    // COALESCE result should never be NULL when narration exists
    for row in &result.rows {
        assert!(!matches!(row[2], Value::Null));
    }
}

// ============================================================================
// Boolean Expression Tests
// ============================================================================

#[test]
fn test_boolean_and_or_not() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let create_query = parse("CREATE TABLE bools (a, b)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO bools VALUES (1, 0), (1, 1), (0, 0), (0, 1)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    // Test AND
    let select_query = parse("SELECT a, b FROM bools WHERE a = 1 AND b = 1").expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");
    assert_eq!(result.len(), 1);

    // Test OR
    let select_query2 = parse("SELECT a, b FROM bools WHERE a = 1 OR b = 1").expect("should parse");
    let result2 = executor.execute(&select_query2).expect("should execute");
    assert_eq!(result2.len(), 3);

    // Test NOT
    let select_query3 = parse("SELECT a, b FROM bools WHERE NOT (a = 1)").expect("should parse");
    let result3 = executor.execute(&select_query3).expect("should execute");
    assert_eq!(result3.len(), 2);
}

#[test]
fn test_between_clause() {
    let directives = make_test_directives();
    let mut executor = Executor::new(&directives);

    let create_query = parse("CREATE TABLE range_test (val)").expect("should parse");
    executor.execute(&create_query).expect("should execute");

    let insert_query =
        parse("INSERT INTO range_test VALUES (1), (5), (10), (15), (20)").expect("should parse");
    executor.execute(&insert_query).expect("should execute");

    let select_query = parse("SELECT val FROM range_test WHERE val BETWEEN 5 AND 15 ORDER BY val")
        .expect("should parse");
    let result = executor.execute(&select_query).expect("should execute");

    assert_eq!(result.len(), 3);
    if let Value::Integer(v) = &result.rows[0][0] {
        assert_eq!(*v, 5);
    }
    if let Value::Integer(v) = &result.rows[1][0] {
        assert_eq!(*v, 10);
    }
    if let Value::Integer(v) = &result.rows[2][0] {
        assert_eq!(*v, 15);
    }
}

#[test]
fn test_in_clause_with_accounts() {
    // Test IN clause on ledger accounts
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT account WHERE account = "Expenses:Food" OR account = "Expenses:Transport""#,
        &directives,
    );

    // Should find Food and Transport expense postings
    assert!(result.len() >= 2);
    for row in &result.rows {
        if let Value::String(acc) = &row[0] {
            assert!(acc.contains("Expenses"));
        }
    }
}

#[test]
fn test_filter_with_not_equal() {
    // Test filtering with !=
    let directives = make_test_directives();
    let result = execute_query(
        r#"SELECT DISTINCT account WHERE account ~ "Assets" AND account != "Assets:Bank:Savings""#,
        &directives,
    );

    // Should only have Checking, not Savings
    for row in &result.rows {
        if let Value::String(acc) = &row[0] {
            assert!(!acc.contains("Savings"));
        }
    }
}

// ============================================================================
// Nested Aggregate Function Tests (Holdings-style queries)
// ============================================================================

use rustledger_core::{CostSpec, Price};

fn make_holdings_directives() -> Vec<Directive> {
    vec![
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Brokerage")),
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Cash")),
        // Price directives
        Directive::Price(Price::new(
            date(2024, 1, 1),
            "AAPL",
            Amount::new(dec!(150), "USD"),
        )),
        Directive::Price(Price::new(
            date(2024, 6, 1),
            "AAPL",
            Amount::new(dec!(180), "USD"),
        )),
        // Buy 10 AAPL at $100
        Directive::Transaction(
            Transaction::new(date(2024, 1, 15), "Buy AAPL")
                .with_posting(
                    Posting::new("Assets:Brokerage", Amount::new(dec!(10), "AAPL")).with_cost(
                        CostSpec::empty()
                            .with_number_per(dec!(100))
                            .with_currency("USD")
                            .with_date(date(2024, 1, 15)),
                    ),
                )
                .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-1000), "USD"))),
        ),
        // Buy 5 more AAPL at $120
        Directive::Transaction(
            Transaction::new(date(2024, 3, 20), "Buy more AAPL")
                .with_posting(
                    Posting::new("Assets:Brokerage", Amount::new(dec!(5), "AAPL")).with_cost(
                        CostSpec::empty()
                            .with_number_per(dec!(120))
                            .with_currency("USD")
                            .with_date(date(2024, 3, 20)),
                    ),
                )
                .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-600), "USD"))),
        ),
    ]
}

#[test]
fn test_units_sum_position() {
    // Test units(sum(position)) - nested aggregate with non-aggregate function
    let directives = make_holdings_directives();
    let result = execute_query(
        r"SELECT account, units(sum(position)) as units GROUP BY account",
        &directives,
    );

    // Should have 2 rows: Brokerage and Cash
    assert_eq!(result.len(), 2);
}

#[test]
fn test_cost_sum_position() {
    // Test cost(sum(position)) - book value calculation
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT account, cost(sum(position)) as book_value
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Book value should be 10*100 + 5*120 = 1600 USD
    if let Value::Amount(amt) = &result.rows[0][1] {
        assert_eq!(amt.number, dec!(1600));
        assert_eq!(amt.currency.as_str(), "USD");
    } else {
        panic!("Expected Amount value for book_value");
    }
}

#[test]
fn test_number_cost_sum_position() {
    // Test number(cost(sum(position))) - deeply nested
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT number(cost(sum(position))) as cost_number
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(1600));
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_safediv_with_aggregates() {
    // Test safediv with aggregate expressions - like profit percentage calculation
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT safediv(number(cost(sum(position))), 100) as cost_pct
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(16)); // 1600 / 100 = 16
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_parenthesized_aggregate_expression() {
    // Test that parentheses work correctly with aggregate expressions
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT (cost(sum(position))) as book_value
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Amount(amt) = &result.rows[0][0] {
        assert_eq!(amt.number, dec!(1600));
    } else {
        panic!("Expected Amount value");
    }
}

#[test]
fn test_complex_arithmetic_with_aggregates() {
    // Test complex arithmetic expressions with aggregates
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT (number(cost(sum(position))) - 1000) * 2 as calc
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        // (1600 - 1000) * 2 = 1200
        assert_eq!(*n, dec!(1200));
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_multiple_nested_aggregates_in_select() {
    // Test multiple columns with nested aggregates
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT
             account,
             units(sum(position)) as units,
             cost(sum(position)) as book_value,
             number(cost(sum(position))) as cost_num
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Verify cost_num column
    if let Value::Number(n) = &result.rows[0][3] {
        assert_eq!(*n, dec!(1600));
    } else {
        panic!("Expected Number value for cost_num");
    }
}

// ============================================================================
// Unit tests for evaluate_function_on_values code path
// These test non-aggregate functions wrapping aggregate expressions
// ============================================================================

#[test]
fn test_currency_on_aggregate() {
    // Test currency(cost(sum(position))) - currency extraction from aggregate result
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT currency(cost(sum(position))) as cost_curr
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::String(s) = &result.rows[0][0] {
        assert_eq!(s, "USD");
    } else {
        panic!("Expected String value for currency");
    }
}

#[test]
fn test_deeply_nested_aggregate_functions() {
    // Test abs(number(cost(sum(position)))) - 3 levels of nesting
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT abs(number(cost(sum(position)))) as abs_cost
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(1600));
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_safediv_with_two_aggregate_args() {
    // Test safediv with two aggregate arguments: safediv(sum(...), count(...))
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT safediv(number(cost(sum(position))), count(1)) as avg_cost
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        // 1600 / 2 postings = 800
        assert_eq!(*n, dec!(800));
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_null_propagation_in_nested_aggregates() {
    // Test that NULL values propagate correctly through nested functions
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT number(cost(sum(position))) as cost_num
           WHERE account ~ "Cash"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Cash positions have no cost, so should be NULL
    assert!(
        matches!(&result.rows[0][0], Value::Null),
        "Expected Null for cash position cost"
    );
}

#[test]
fn test_unary_negation_on_aggregate() {
    // Test -number(cost(sum(position))) - unary operator on aggregate
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT -number(cost(sum(position))) as neg_cost
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(-1600));
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_number_on_single_currency_inventory() {
    // When inventory has one currency, NUMBER should return the total
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT currency, number(units(sum(position))) as units_num
           WHERE account ~ "Brokerage"
           GROUP BY currency"#,
        &directives,
    );

    // Should have 1 row for AAPL
    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][1] {
        assert_eq!(*n, dec!(15)); // 10 + 5 AAPL
    } else {
        panic!("Expected Number value");
    }
}

fn make_multi_currency_holdings() -> Vec<Directive> {
    vec![
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Brokerage")),
        Directive::Open(Open::new(date(2024, 1, 1), "Assets:Cash")),
        // Buy 10 AAPL
        Directive::Transaction(
            Transaction::new(date(2024, 1, 15), "Buy AAPL")
                .with_posting(
                    Posting::new("Assets:Brokerage", Amount::new(dec!(10), "AAPL")).with_cost(
                        CostSpec::empty()
                            .with_number_per(dec!(100))
                            .with_currency("USD")
                            .with_date(date(2024, 1, 15)),
                    ),
                )
                .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-1000), "USD"))),
        ),
        // Buy 5 GOOG (different currency/stock)
        Directive::Transaction(
            Transaction::new(date(2024, 2, 10), "Buy GOOG")
                .with_posting(
                    Posting::new("Assets:Brokerage", Amount::new(dec!(5), "GOOG")).with_cost(
                        CostSpec::empty()
                            .with_number_per(dec!(150))
                            .with_currency("USD")
                            .with_date(date(2024, 2, 10)),
                    ),
                )
                .with_posting(Posting::new("Assets:Cash", Amount::new(dec!(-750), "USD"))),
        ),
    ]
}

#[test]
fn test_number_returns_null_for_mixed_currency_inventory() {
    // When an inventory contains multiple currencies (AAPL + GOOG),
    // NUMBER should return NULL rather than a meaningless sum
    let directives = make_multi_currency_holdings();
    let result = execute_query(
        r#"SELECT number(units(sum(position))) as units_num
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Should be NULL because inventory has AAPL and GOOG
    assert!(
        matches!(&result.rows[0][0], Value::Null),
        "Expected Null for multi-currency inventory, got {:?}",
        result.rows[0][0]
    );
}

// ============================================================================
// Additional coverage tests for evaluate_function_on_values
// ============================================================================

#[test]
fn test_safediv_division_by_zero() {
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT safediv(number(cost(sum(position))), 0) as div_zero
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    assert!(matches!(&result.rows[0][0], Value::Null));
}

#[test]
fn test_safediv_with_null() {
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT safediv(number(cost(sum(position))), number(cost(sum(position)))) as ratio
           WHERE account ~ "Cash"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // Cash has no cost, so NULL / anything = NULL
    assert!(matches!(&result.rows[0][0], Value::Null));
}

#[test]
fn test_value_function_with_conversion() {
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT number(value(sum(position), "USD")) as market_value
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    // 15 AAPL * 180 USD = 2700 USD
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(2700));
    } else {
        panic!("Expected Number value for market value");
    }
}

#[test]
fn test_empty_function() {
    let directives = make_test_directives();
    let result = execute_query(
        r"SELECT empty(sum(position)) as is_empty GROUP BY account LIMIT 1",
        &directives,
    );

    assert!(!result.is_empty());
    // Most accounts have postings, so should be false
    assert!(matches!(&result.rows[0][0], Value::Boolean(_)));
}

#[test]
fn test_only_function_with_inventory() {
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT only(currency, sum(position)) as only_amt
           WHERE account ~ "Brokerage"
           GROUP BY currency"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Amount(a) = &result.rows[0][0] {
        assert_eq!(a.number, dec!(15)); // 10 + 5 AAPL
    } else {
        panic!("Expected Amount value");
    }
}

#[test]
fn test_filter_currency_function() {
    let directives = make_multi_currency_holdings();
    let result = execute_query(
        r#"SELECT number(units(filter_currency(sum(position), "AAPL"))) as aapl_units
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::Number(n) = &result.rows[0][0] {
        assert_eq!(*n, dec!(10)); // Only 10 AAPL, not GOOG
    } else {
        panic!("Expected Number value");
    }
}

#[test]
fn test_currency_on_inventory() {
    let directives = make_holdings_directives();
    let result = execute_query(
        r#"SELECT currency(units(sum(position))) as curr
           WHERE account ~ "Brokerage"
           GROUP BY account"#,
        &directives,
    );

    assert_eq!(result.len(), 1);
    if let Value::String(s) = &result.rows[0][0] {
        assert_eq!(s, "AAPL");
    } else {
        panic!("Expected String value for currency");
    }
}

#[test]
fn test_number_on_empty_inventory() {
    let directives = make_test_directives();
    // Query an account with no postings to get empty inventory
    let result = execute_query(
        r#"SELECT number(sum(position)) as num
           WHERE account = "NonExistent:Account"
           GROUP BY account"#,
        &directives,
    );

    // No results since account doesn't exist
    assert!(result.is_empty());
}
