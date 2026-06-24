//! Semantic facts produced by language-native workers and framework adapters.

use super::Evidence;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolId(String);

impl SymbolId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("symbol id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticFactKind {
    ResolvedCall,
    ResolvedImport,
    Symbol,
    Type,
    FrameworkRole,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactOrigin {
    pub engine: String,
    pub engine_version: String,
    pub method: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactCertainty {
    Semantic,
    DataflowDerived,
    Structural,
    FrameworkHeuristic,
    Conflicting,
    Unknown,
}

impl FactCertainty {
    pub fn supports_family_membership(self) -> bool {
        matches!(self, Self::Semantic | Self::DataflowDerived)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFact {
    pub kind: SemanticFactKind,
    pub subject: String,
    pub target: Option<SymbolId>,
    pub origin: FactOrigin,
    pub certainty: FactCertainty,
    pub evidence: Evidence,
    pub assumptions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_certainty_does_not_prove_semantic_family_membership() {
        assert!(FactCertainty::Semantic.supports_family_membership());
        assert!(!FactCertainty::Structural.supports_family_membership());
        assert!(!FactCertainty::Conflicting.supports_family_membership());
        assert!(!FactCertainty::Unknown.supports_family_membership());
    }
}
