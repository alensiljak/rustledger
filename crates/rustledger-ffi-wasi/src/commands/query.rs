//! Query commands (query, batch).

use rustledger_core::Directive;
use rustledger_query::{Executor, parse as parse_query};

use crate::convert::{directive_to_json, value_datatype, value_to_json};
use crate::helpers::load_source;
use crate::types::{BatchOutput, ColumnInfo, DirectiveJson, Error, LoadOutput, QueryOutput};
use crate::{API_VERSION, output_json};

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
                        datatype: "str".to_string(), // Default if no rows
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

/// Execute a single query on source from stdin.
pub fn cmd_query(source: &str, query_str: &str) -> i32 {
    let load = load_source(source);

    if !load.errors.is_empty() {
        let output = QueryOutput {
            api_version: API_VERSION,
            columns: vec![],
            rows: vec![],
            errors: load.errors,
        };
        return output_json(&output);
    }

    let output = execute_query(&load.directives, query_str);
    output_json(&output)
}

/// Batch command: load + multiple queries in one parse.
pub fn cmd_batch(source: &str, filename: &str, queries: &[String]) -> i32 {
    let load = load_source(source);

    // Build load output
    let entries: Vec<DirectiveJson> = load
        .directives
        .iter()
        .zip(load.directive_lines.iter())
        .map(|(d, &line)| directive_to_json(d, line, filename))
        .collect();

    let load_output = LoadOutput {
        api_version: API_VERSION,
        entries,
        errors: load.errors.clone(),
        options: load.options,
        plugins: load.plugins,
        includes: load.includes,
    };

    // Execute queries (only if no parse errors)
    let query_outputs: Vec<QueryOutput> = if load.errors.is_empty() {
        queries
            .iter()
            .map(|q| execute_query(&load.directives, q))
            .collect()
    } else {
        // Return error for each query
        queries
            .iter()
            .map(|_| QueryOutput {
                api_version: API_VERSION,
                columns: vec![],
                rows: vec![],
                errors: vec![Error::new("Cannot execute query: parse errors exist")],
            })
            .collect()
    };

    let output = BatchOutput {
        api_version: API_VERSION,
        load: load_output,
        queries: query_outputs,
    };
    output_json(&output)
}
