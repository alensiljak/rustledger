//! European Central Bank (ECB) price source.
//!
//! Fetches currency exchange rates from the ECB.

use super::{PriceSource, user_agent};
use crate::cmd::price::{PriceRequest, PriceResponse};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::time::Duration;

/// European Central Bank price source.
///
/// Uses the ECB Statistical Data Warehouse API to fetch exchange rates.
/// No API key required.
///
/// # Supported Currencies
///
/// All currencies in the ECB daily reference rates:
/// - EUR (base), USD, GBP, JPY, CHF, CAD, AUD, etc.
///
/// # Notes
///
/// - ECB rates are published once per day around 16:00 CET
/// - Rates are against EUR (EUR is the base currency)
/// - Weekend/holiday rates use the last available rate
#[derive(Debug)]
pub struct EcbSource {
    #[allow(dead_code)]
    timeout: Duration,
}

impl EcbSource {
    /// Create a new ECB source.
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Build the ECB API URL for a currency pair.
    fn build_url(&self, currency: &str) -> String {
        format!(
            "https://data-api.ecb.europa.eu/service/data/EXR/D.{currency}.EUR.SP00.A?lastNObservations=1&format=jsondata"
        )
    }
}

impl PriceSource for EcbSource {
    fn name(&self) -> &'static str {
        "ecb"
    }

    fn description(&self) -> &'static str {
        "European Central Bank - currency exchange rates"
    }

    fn fetch_price(&self, request: &PriceRequest) -> Result<PriceResponse> {
        // ECB provides rates where EUR is the base currency
        // If requesting EUR, return 1.0
        if request.ticker.to_uppercase() == "EUR" {
            let date = request.date.unwrap_or_else(|| Utc::now().date_naive());
            return Ok(PriceResponse {
                price: Decimal::ONE,
                currency: request.currency.clone(),
                date,
                source: self.name().to_string(),
            });
        }

        let url = self.build_url(&request.ticker.to_uppercase());

        let mut response = ureq::get(&url)
            .header("User-Agent", user_agent())
            .header("Accept", "application/json")
            .call()
            .with_context(|| format!("Failed to fetch ECB rate for {}", request.ticker))?;

        let json: serde_json::Value = response
            .body_mut()
            .read_json()
            .with_context(|| format!("Failed to parse ECB response for {}", request.ticker))?;

        // Navigate the SDMX-JSON structure to find the rate
        let datasets = json
            .get("dataSets")
            .and_then(serde_json::Value::as_array)
            .and_then(|a| a.first())
            .with_context(|| "Missing dataSets in ECB response")?;

        let series = datasets
            .get("series")
            .and_then(serde_json::Value::as_object)
            .and_then(|o| o.values().next())
            .with_context(|| "Missing series in ECB response")?;

        let observations = series
            .get("observations")
            .and_then(serde_json::Value::as_object)
            .with_context(|| "Missing observations in ECB response")?;

        // Get the most recent observation
        let (obs_key, obs_value) = observations
            .iter()
            .next_back()
            .with_context(|| "No observations in ECB response")?;

        let rate_value = obs_value
            .as_array()
            .and_then(|a| a.first())
            .and_then(serde_json::Value::as_f64)
            .with_context(|| "Invalid rate value in ECB response")?;

        let rate = Decimal::from_str(&rate_value.to_string())
            .with_context(|| format!("Failed to parse rate: {rate_value}"))?;

        let price = rate;

        // Try to get the date from the structure
        let date = if let Some(structure) = json.get("structure") {
            if let Some(dimensions) = structure.get("dimensions") {
                if let Some(observation) = dimensions.get("observation") {
                    if let Some(time_dim) = observation.as_array().and_then(|a| a.first()) {
                        if let Some(values) = time_dim.get("values").and_then(|v| v.as_array()) {
                            let idx: usize = obs_key.parse().unwrap_or(0);
                            values
                                .get(idx)
                                .and_then(|v| v.get("id"))
                                .and_then(serde_json::Value::as_str)
                                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                                .unwrap_or_else(|| {
                                    request.date.unwrap_or_else(|| Utc::now().date_naive())
                                })
                        } else {
                            request.date.unwrap_or_else(|| Utc::now().date_naive())
                        }
                    } else {
                        request.date.unwrap_or_else(|| Utc::now().date_naive())
                    }
                } else {
                    request.date.unwrap_or_else(|| Utc::now().date_naive())
                }
            } else {
                request.date.unwrap_or_else(|| Utc::now().date_naive())
            }
        } else {
            request.date.unwrap_or_else(|| Utc::now().date_naive())
        };

        Ok(PriceResponse {
            price,
            currency: "EUR".to_string(),
            date,
            source: self.name().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let source = EcbSource::new(Duration::from_secs(30));
        let url = source.build_url("USD");
        assert!(url.contains("USD"));
        assert!(url.contains("data-api.ecb.europa.eu"));
    }

    #[test]
    fn test_source_metadata() {
        let source = EcbSource::new(Duration::from_secs(30));
        assert_eq!(source.name(), "ecb");
        assert!(!source.requires_api_key());
    }

    #[test]
    fn test_eur_returns_one() {
        let source = EcbSource::new(Duration::from_secs(30));
        let request = PriceRequest::new("EUR", "USD");
        let response = source.fetch_price(&request).unwrap();
        assert_eq!(response.price, Decimal::ONE);
    }
}
