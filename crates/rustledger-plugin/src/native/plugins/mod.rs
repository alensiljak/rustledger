//! Native plugin implementations, one per file.

pub mod utils;

mod auto_accounts;
mod auto_tag;
mod check_average_cost;
mod check_closing;
mod check_commodity;
mod check_drained;
mod close_tree;
mod coherent_cost;
mod commodity_attr;
mod currency_accounts;
mod document_discovery;
mod implicit_prices;
mod leaf_only;
mod no_duplicates;
mod no_unused;
mod one_commodity;
mod pedantic;
mod sell_gains;
mod unique_prices;
mod unrealized;

pub use auto_accounts::AutoAccountsPlugin;
pub use auto_tag::AutoTagPlugin;
pub use check_average_cost::CheckAverageCostPlugin;
pub use check_closing::CheckClosingPlugin;
pub use check_commodity::CheckCommodityPlugin;
pub use check_drained::CheckDrainedPlugin;
pub use close_tree::CloseTreePlugin;
pub use coherent_cost::CoherentCostPlugin;
pub use commodity_attr::CommodityAttrPlugin;
pub use currency_accounts::CurrencyAccountsPlugin;
pub use document_discovery::DocumentDiscoveryPlugin;
pub use implicit_prices::ImplicitPricesPlugin;
pub use leaf_only::LeafOnlyPlugin;
pub use no_duplicates::NoDuplicatesPlugin;
pub use no_unused::NoUnusedPlugin;
pub use one_commodity::OneCommodityPlugin;
pub use pedantic::PedanticPlugin;
pub use sell_gains::SellGainsPlugin;
pub use unique_prices::UniquePricesPlugin;
pub use unrealized::UnrealizedPlugin;
