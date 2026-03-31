//! Price source implementations.
//!
//! This module contains all built-in price source implementations and the
//! `PriceSource` trait that defines the interface for all sources.

mod alphavantage;
mod coinbase;
mod coincap;
mod coinmarketcap;
mod eastmoneyfund;
mod ecb;
mod oanda;
mod quandl;
mod ratesapi;
mod tsp;
mod yahoo;

pub use alphavantage::AlphaVantageSource;
pub use coinbase::CoinbaseSource;
pub use coincap::CoinCapSource;
pub use coinmarketcap::CoinMarketCapSource;
pub use eastmoneyfund::EastMoneyFundSource;
pub use ecb::EcbSource;
pub use oanda::OandaSource;
pub use quandl::QuandlSource;
pub use ratesapi::RatesApiSource;
pub use tsp::TspSource;
pub use yahoo::YahooFinanceSource;

use super::{PriceRequest, PriceResponse};
use anyhow::Result;

/// Trait for price data sources.
///
/// All price sources must implement this trait. The trait is object-safe
/// to allow dynamic dispatch through `Arc<dyn PriceSource>`.
///
/// # Implementation Notes
///
/// Source implementations store a `timeout` field for future use. Currently,
/// ureq 3.x doesn't support timeout on individual requests (only on the Agent).
/// A future enhancement could use `ureq::Agent` with timeout configuration.
pub trait PriceSource: Send + Sync {
    /// Returns the unique name of this source.
    fn name(&self) -> &'static str;

    /// Returns a human-readable description of this source.
    fn description(&self) -> &'static str;

    /// Indicates if this source requires an API key.
    fn requires_api_key(&self) -> bool {
        false
    }

    /// Returns the environment variable name for the API key, if required.
    fn api_key_env_var(&self) -> Option<&'static str> {
        None
    }

    /// Fetch a price for the given request.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The network request fails
    /// - The response cannot be parsed
    /// - The ticker is not found
    /// - The API key is missing (for sources that require it)
    fn fetch_price(&self, request: &PriceRequest) -> Result<PriceResponse>;
}

/// Helper function to build a User-Agent header for HTTP requests.
pub(crate) const fn user_agent() -> &'static str {
    "Mozilla/5.0 (compatible; rustledger/1.0; +https://github.com/rustledger/rustledger)"
}
