//! Native plugin implementations, one per file.

pub mod utils;

mod implicit_prices;
mod check_commodity;
mod auto_tag;
mod auto_accounts;
mod leaf_only;
mod no_duplicates;
mod one_commodity;
mod unique_prices;
mod document_discovery;
mod check_closing;
mod close_tree;
mod coherent_cost;
mod sell_gains;
mod pedantic;
mod unrealized;
mod no_unused;
mod check_drained;
mod commodity_attr;
mod check_average_cost;
mod currency_accounts;

pub use implicit_prices::ImplicitPricesPlugin;
pub use check_commodity::CheckCommodityPlugin;
pub use auto_tag::AutoTagPlugin;
pub use auto_accounts::AutoAccountsPlugin;
pub use leaf_only::LeafOnlyPlugin;
pub use no_duplicates::NoDuplicatesPlugin;
pub use one_commodity::OneCommodityPlugin;
pub use unique_prices::UniquePricesPlugin;
pub use document_discovery::DocumentDiscoveryPlugin;
pub use check_closing::CheckClosingPlugin;
pub use close_tree::CloseTreePlugin;
pub use coherent_cost::CoherentCostPlugin;
pub use sell_gains::SellGainsPlugin;
pub use pedantic::PedanticPlugin;
pub use unrealized::UnrealizedPlugin;
pub use no_unused::NoUnusedPlugin;
pub use check_drained::CheckDrainedPlugin;
pub use commodity_attr::CommodityAttrPlugin;
pub use check_average_cost::CheckAverageCostPlugin;
pub use currency_accounts::CurrencyAccountsPlugin;
