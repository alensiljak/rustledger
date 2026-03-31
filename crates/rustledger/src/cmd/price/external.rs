//! External command price source.
//!
//! This module provides a price source that executes an external command
//! to fetch prices. This allows users to integrate custom price fetchers.

use super::sources::PriceSource;
use super::{PriceRequest, PriceResponse};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

/// A price source that executes an external command.
///
/// The command receives the ticker as the first argument, with optional
/// `--date` and `--currency` flags. The command should output the price
/// in one of these formats:
///
/// 1. Simple: `150.00 USD`
/// 2. JSON: `{"price": 150.00, "currency": "USD", "date": "2024-01-15"}`
/// 3. Beancount: `2024-01-15 price AAPL 150.00 USD`
#[derive(Debug)]
pub struct ExternalCommandSource {
    /// The command and arguments to execute.
    command: Vec<String>,
    /// Timeout for command execution.
    #[allow(dead_code)]
    timeout: Duration,
    /// Additional environment variables.
    env: HashMap<String, String>,
}

impl ExternalCommandSource {
    /// Create a new external command source.
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(command: Vec<String>, timeout: Duration, env: HashMap<String, String>) -> Self {
        Self {
            command,
            timeout,
            env,
        }
    }

    /// Parse output in simple format: `150.00 USD`
    fn parse_simple_format(&self, line: &str) -> Result<(Decimal, String)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let price = Decimal::from_str(parts[0])
                .with_context(|| format!("Invalid price value: {}", parts[0]))?;
            let currency = parts[1].to_string();
            Ok((price, currency))
        } else if parts.len() == 1 {
            let price = Decimal::from_str(parts[0])
                .with_context(|| format!("Invalid price value: {}", parts[0]))?;
            Ok((price, "USD".to_string()))
        } else {
            anyhow::bail!("Invalid simple format output: {line}")
        }
    }

    /// Parse output in JSON format.
    fn parse_json_format(&self, line: &str) -> Result<(Decimal, String, Option<NaiveDate>)> {
        let json: serde_json::Value =
            serde_json::from_str(line).with_context(|| "Invalid JSON output")?;

        let price = json
            .get("price")
            .and_then(|v| {
                if let Some(n) = v.as_number() {
                    Decimal::from_str(&n.to_string()).ok()
                } else if let Some(s) = v.as_str() {
                    Decimal::from_str(s).ok()
                } else {
                    None
                }
            })
            .with_context(|| "Missing or invalid 'price' field in JSON")?;

        let currency = json
            .get("currency")
            .and_then(|v| v.as_str())
            .map_or_else(|| "USD".to_string(), String::from);

        let date = json
            .get("date")
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        Ok((price, currency, date))
    }

    /// Parse output in beancount format: `2024-01-15 price AAPL 150.00 USD`
    fn parse_beancount_format(&self, line: &str) -> Result<(Decimal, String, NaiveDate)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 && parts[1] == "price" {
            let date = NaiveDate::parse_from_str(parts[0], "%Y-%m-%d")
                .with_context(|| format!("Invalid date: {}", parts[0]))?;
            let price = Decimal::from_str(parts[3])
                .with_context(|| format!("Invalid price: {}", parts[3]))?;
            let currency = parts[4].to_string();
            Ok((price, currency, date))
        } else {
            anyhow::bail!("Invalid beancount format output: {line}")
        }
    }
}

impl PriceSource for ExternalCommandSource {
    fn name(&self) -> &'static str {
        "external"
    }

    fn description(&self) -> &'static str {
        "External command price source"
    }

    fn fetch_price(&self, request: &PriceRequest) -> Result<PriceResponse> {
        if self.command.is_empty() {
            anyhow::bail!("External command is empty");
        }

        let program = &self.command[0];
        let args = &self.command[1..];

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.arg(&request.ticker);

        if let Some(date) = request.date {
            cmd.arg("--date");
            cmd.arg(date.format("%Y-%m-%d").to_string());
        }

        cmd.arg("--currency");
        cmd.arg(&request.currency);

        // Set additional environment variables
        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to execute command: {program}"))?;

        let output = child
            .wait_with_output()
            .with_context(|| "Failed to wait for command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Command failed with exit code {:?}: {}",
                output.status.code(),
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let reader = BufReader::new(stdout.as_bytes());

        // Try to parse the first non-empty line
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            // Try JSON format first
            if line.starts_with('{') {
                let (price, currency, date) = self.parse_json_format(line)?;
                return Ok(PriceResponse {
                    price,
                    currency,
                    date: date
                        .unwrap_or_else(|| request.date.unwrap_or_else(|| Utc::now().date_naive())),
                    source: "external".to_string(),
                });
            }

            // Try beancount format
            if line.chars().next().is_some_and(|c| c.is_ascii_digit()) && line.contains("price") {
                let (price, currency, date) = self.parse_beancount_format(line)?;
                return Ok(PriceResponse {
                    price,
                    currency,
                    date,
                    source: "external".to_string(),
                });
            }

            // Try simple format
            let (price, currency) = self.parse_simple_format(line)?;
            return Ok(PriceResponse {
                price,
                currency,
                date: request.date.unwrap_or_else(|| Utc::now().date_naive()),
                source: "external".to_string(),
            });
        }

        anyhow::bail!("Command produced no valid output")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_format() {
        let source = ExternalCommandSource::new(vec![], Duration::from_secs(30), HashMap::new());

        let (price, currency) = source.parse_simple_format("150.00 USD").unwrap();
        assert_eq!(price, Decimal::from_str("150.00").unwrap());
        assert_eq!(currency, "USD");

        let (price, currency) = source.parse_simple_format("  99.99  EUR  ").unwrap();
        assert_eq!(price, Decimal::from_str("99.99").unwrap());
        assert_eq!(currency, "EUR");

        let (price, currency) = source.parse_simple_format("42").unwrap();
        assert_eq!(price, Decimal::from(42));
        assert_eq!(currency, "USD");
    }

    #[test]
    fn test_parse_json_format() {
        let source = ExternalCommandSource::new(vec![], Duration::from_secs(30), HashMap::new());

        let (price, currency, date) = source
            .parse_json_format(r#"{"price": 150.00, "currency": "USD", "date": "2024-01-15"}"#)
            .unwrap();
        assert_eq!(price, Decimal::from_str("150.00").unwrap());
        assert_eq!(currency, "USD");
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()));

        let (price, currency, date) = source.parse_json_format(r#"{"price": "99.99"}"#).unwrap();
        assert_eq!(price, Decimal::from_str("99.99").unwrap());
        assert_eq!(currency, "USD");
        assert!(date.is_none());
    }

    #[test]
    fn test_parse_beancount_format() {
        let source = ExternalCommandSource::new(vec![], Duration::from_secs(30), HashMap::new());

        let (price, currency, date) = source
            .parse_beancount_format("2024-01-15 price AAPL 150.00 USD")
            .unwrap();
        assert_eq!(price, Decimal::from_str("150.00").unwrap());
        assert_eq!(currency, "USD");
        assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    }

    #[test]
    fn test_external_command_echo() {
        let source = ExternalCommandSource::new(
            vec!["echo".to_string(), "150.00 USD".to_string()],
            Duration::from_secs(5),
            HashMap::new(),
        );

        let request = PriceRequest::new("AAPL", "USD");
        let response = source.fetch_price(&request).unwrap();

        assert_eq!(response.price, Decimal::from_str("150.00").unwrap());
        assert_eq!(response.currency, "USD");
        assert_eq!(response.source, "external");
    }
}
