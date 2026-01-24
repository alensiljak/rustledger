//! Native (non-WASM) plugin support.
//!
//! These plugins run as native Rust code for maximum performance.
//! They implement the same interface as WASM plugins.

mod plugins;

pub use plugins::*;

use crate::types::PluginInput;
use crate::types::PluginOutput;

/// Trait for native plugins.
pub trait NativePlugin: Send + Sync {
    /// Plugin name.
    fn name(&self) -> &'static str;

    /// Plugin description.
    fn description(&self) -> &'static str;

    /// Process directives and return modified directives + errors.
    fn process(&self, input: PluginInput) -> PluginOutput;
}

/// Registry of built-in native plugins.
pub struct NativePluginRegistry {
    plugins: Vec<Box<dyn NativePlugin>>,
}

impl NativePluginRegistry {
    /// Create a new registry with all built-in plugins.
    pub fn new() -> Self {
        Self {
            plugins: vec![
                Box::new(ImplicitPricesPlugin),
                Box::new(CheckCommodityPlugin),
                Box::new(AutoTagPlugin::new()),
                Box::new(AutoAccountsPlugin),
                Box::new(LeafOnlyPlugin),
                Box::new(NoDuplicatesPlugin),
                Box::new(OneCommodityPlugin),
                Box::new(UniquePricesPlugin),
                Box::new(CheckClosingPlugin),
                Box::new(CloseTreePlugin),
                Box::new(CoherentCostPlugin),
                Box::new(SellGainsPlugin),
                Box::new(PedanticPlugin),
                Box::new(UnrealizedPlugin::new()),
                Box::new(NoUnusedPlugin),
                Box::new(CheckDrainedPlugin),
                Box::new(CommodityAttrPlugin::new()),
                Box::new(CheckAverageCostPlugin::new()),
                Box::new(CurrencyAccountsPlugin::new()),
            ],
        }
    }

    /// Find a plugin by name.
    pub fn find(&self, name: &str) -> Option<&dyn NativePlugin> {
        // Check for beancount.plugins.* prefix
        let name = name.strip_prefix("beancount.plugins.").unwrap_or(name);

        self.plugins
            .iter()
            .find(|p| p.name() == name)
            .map(std::convert::AsRef::as_ref)
    }

    /// List all available plugins.
    pub fn list(&self) -> Vec<&dyn NativePlugin> {
        self.plugins.iter().map(AsRef::as_ref).collect()
    }

    /// Check if a name refers to a built-in plugin.
    pub fn is_builtin(name: &str) -> bool {
        let name = name.strip_prefix("beancount.plugins.").unwrap_or(name);

        matches!(
            name,
            "implicit_prices"
                | "check_commodity"
                | "auto_tag"
                | "auto_accounts"
                | "leafonly"
                | "noduplicates"
                | "onecommodity"
                | "unique_prices"
                | "check_closing"
                | "close_tree"
                | "coherent_cost"
                | "sellgains"
                | "pedantic"
                | "unrealized"
                | "nounused"
                | "check_drained"
                | "commodity_attr"
                | "check_average_cost"
                | "currency_accounts"
        )
    }
}

impl Default for NativePluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
