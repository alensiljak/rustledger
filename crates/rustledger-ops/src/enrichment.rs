//! Shared types for operation results.
//!
//! These types describe the outcome of directive operations — how a
//! categorization was determined, what alternatives were considered,
//! and how confident the system is in its result.

/// How a categorization was determined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategorizationMethod {
    /// Matched a user-defined rule (substring or regex).
    Rule,
    /// Matched a built-in merchant dictionary entry.
    MerchantDict,
    /// ML model prediction.
    Ml,
    /// LLM suggestion via MCP.
    Llm,
    /// Fell back to default account (no match found).
    Default,
    /// User manually assigned.
    Manual,
}

impl CategorizationMethod {
    /// Returns the string representation used in directive metadata.
    #[must_use]
    pub const fn as_meta_value(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::MerchantDict => "merchant-dict",
            Self::Ml => "ml",
            Self::Llm => "llm",
            Self::Default => "default",
            Self::Manual => "manual",
        }
    }
}

/// An alternative categorization with its confidence score.
#[derive(Debug, Clone)]
pub struct Alternative {
    /// The account that could have been chosen.
    pub account: String,
    /// Confidence score for this alternative (0.0 to 1.0).
    pub confidence: f64,
    /// How this alternative was determined.
    pub method: CategorizationMethod,
}

/// Enrichment metadata for a single directive, produced by operations.
#[derive(Debug, Clone)]
pub struct Enrichment {
    /// Index of the directive this enrichment applies to.
    pub directive_index: usize,
    /// Confidence score for the primary categorization (0.0 to 1.0).
    pub confidence: f64,
    /// How the primary categorization was determined.
    pub method: CategorizationMethod,
    /// Other possible categorizations, sorted by confidence descending.
    pub alternatives: Vec<Alternative>,
    /// Stable fingerprint for deduplication (if computed).
    pub fingerprint: Option<crate::fingerprint::Fingerprint>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorization_method_meta_values() {
        assert_eq!(CategorizationMethod::Rule.as_meta_value(), "rule");
        assert_eq!(
            CategorizationMethod::MerchantDict.as_meta_value(),
            "merchant-dict"
        );
        assert_eq!(CategorizationMethod::Ml.as_meta_value(), "ml");
        assert_eq!(CategorizationMethod::Llm.as_meta_value(), "llm");
        assert_eq!(CategorizationMethod::Default.as_meta_value(), "default");
        assert_eq!(CategorizationMethod::Manual.as_meta_value(), "manual");
    }
}
