//! Query execution for the JSON-RPC API.

use rustledger_core::Directive;
use rustledger_query::{Executor, parse as parse_query};

use crate::API_VERSION;
use crate::convert::{value_datatype, value_to_json};
use crate::types::{ColumnInfo, Error, QueryOutput};

/// Execute a single query on directives, returning `QueryOutput`.
pub fn execute_query(directives: &[Directive], query_str: &str) -> QueryOutput {
    // Parse query
    let query = match parse_query(query_str) {
        Ok(q) => q,
        Err(e) => {
            return QueryOutput {
                api_version: API_VERSION,
                columns: vec![],
                rows: vec![],
                errors: vec![Error::new(e.to_string())],
            };
        }
    };

    // Execute
    let mut executor = Executor::new(directives);
    match executor.execute(&query) {
        Ok(result) => {
            // Infer column types from first row
            let columns: Vec<ColumnInfo> = if result.rows.is_empty() {
                result
                    .columns
                    .iter()
                    .map(|name| ColumnInfo {
                        name: name.clone(),
                        datatype: "str".to_string(),
                    })
                    .collect()
            } else {
                result
                    .columns
                    .iter()
                    .zip(result.rows[0].iter())
                    .map(|(name, value)| ColumnInfo {
                        name: name.clone(),
                        datatype: value_datatype(value).to_string(),
                    })
                    .collect()
            };

            let rows: Vec<Vec<_>> = result
                .rows
                .iter()
                .map(|row| row.iter().map(value_to_json).collect())
                .collect();

            QueryOutput {
                api_version: API_VERSION,
                columns,
                rows,
                errors: vec![],
            }
        }
        Err(e) => QueryOutput {
            api_version: API_VERSION,
            columns: vec![],
            rows: vec![],
            errors: vec![Error::new(format!("Query error: {e}"))],
        },
    }
}
