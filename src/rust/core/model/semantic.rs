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

    pub fn as_str(&self) -> &str {
        &self.0
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

impl SemanticFactKind {
    pub fn as_protocol_str(&self) -> &'static str {
        match self {
            Self::ResolvedCall => "RESOLVED_CALL",
            Self::ResolvedImport => "RESOLVED_IMPORT",
            Self::Symbol => "SYMBOL",
            Self::Type => "TYPE",
            Self::FrameworkRole => "FRAMEWORK_ROLE",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "RESOLVED_CALL" => Ok(Self::ResolvedCall),
            "RESOLVED_IMPORT" => Ok(Self::ResolvedImport),
            "SYMBOL" => Ok(Self::Symbol),
            "TYPE" => Ok(Self::Type),
            "FRAMEWORK_ROLE" => Ok(Self::FrameworkRole),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(format!("unsupported semantic fact kind {value}")),
        }
    }
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

    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::Semantic => "SEMANTIC",
            Self::DataflowDerived => "DATAFLOW_DERIVED",
            Self::Structural => "STRUCTURAL",
            Self::FrameworkHeuristic => "FRAMEWORK_HEURISTIC",
            Self::Conflicting => "CONFLICTING",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "SEMANTIC" => Ok(Self::Semantic),
            "DATAFLOW_DERIVED" => Ok(Self::DataflowDerived),
            "STRUCTURAL" => Ok(Self::Structural),
            "FRAMEWORK_HEURISTIC" => Ok(Self::FrameworkHeuristic),
            "CONFLICTING" => Ok(Self::Conflicting),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(format!("unsupported fact certainty {value}")),
        }
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

    #[test]
    fn semantic_fact_kinds_use_stable_protocol_tokens() {
        let values = [
            (SemanticFactKind::ResolvedCall, "RESOLVED_CALL"),
            (SemanticFactKind::ResolvedImport, "RESOLVED_IMPORT"),
            (SemanticFactKind::Symbol, "SYMBOL"),
            (SemanticFactKind::Type, "TYPE"),
            (SemanticFactKind::FrameworkRole, "FRAMEWORK_ROLE"),
            (SemanticFactKind::Unknown, "UNKNOWN"),
        ];

        for (kind, protocol_value) in values {
            assert_eq!(kind.as_protocol_str(), protocol_value);
            assert_eq!(
                SemanticFactKind::parse_protocol_str(protocol_value),
                Ok(kind)
            );
        }
        assert!(SemanticFactKind::parse_protocol_str("CALL").is_err());
    }

    #[test]
    fn certainty_values_use_stable_protocol_tokens() {
        let values = [
            (FactCertainty::Semantic, "SEMANTIC"),
            (FactCertainty::DataflowDerived, "DATAFLOW_DERIVED"),
            (FactCertainty::Structural, "STRUCTURAL"),
            (FactCertainty::FrameworkHeuristic, "FRAMEWORK_HEURISTIC"),
            (FactCertainty::Conflicting, "CONFLICTING"),
            (FactCertainty::Unknown, "UNKNOWN"),
        ];

        for (certainty, protocol_value) in values {
            assert_eq!(certainty.as_protocol_str(), protocol_value);
            assert_eq!(
                FactCertainty::parse_protocol_str(protocol_value),
                Ok(certainty)
            );
        }
        assert!(FactCertainty::parse_protocol_str("LOW_CONFIDENCE").is_err());
    }
}
