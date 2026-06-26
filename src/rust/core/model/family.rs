//! Pattern-family identifiers and classification results.

use super::Evidence;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FamilyId(String);

impl FamilyId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("family id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    InsufficientEvidence,
    CompetingFamilies,
    DynamicBehavior,
    TargetIncompatible,
    UnsupportedLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownClass {
    Blocking,
    NonBlocking,
    Recoverable,
    Irreducible,
}

impl UnknownClass {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::Blocking => "blocking_unknown",
            Self::NonBlocking => "non_blocking_unknown",
            Self::Recoverable => "recoverable_unknown",
            Self::Irreducible => "irreducible_unknown",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "blocking_unknown" => Ok(Self::Blocking),
            "non_blocking_unknown" => Ok(Self::NonBlocking),
            "recoverable_unknown" => Ok(Self::Recoverable),
            "irreducible_unknown" => Ok(Self::Irreducible),
            _ => Err(format!("unsupported unknown class {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReasonCode {
    DynamicImport,
    MonkeyPatch,
    PytestFixtureInjection,
    RuntimeDependencyInjection,
    UnresolvedImport,
    MissingProjectConfig,
    MissingDependency,
    FrameworkMagic,
    MacroOrPreprocessor,
    BuildVariantAmbiguity,
    ConflictingFacts,
    StaleEvidence,
    InsufficientSupport,
}

impl UnknownReasonCode {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::DynamicImport => "DynamicImport",
            Self::MonkeyPatch => "MonkeyPatch",
            Self::PytestFixtureInjection => "PytestFixtureInjection",
            Self::RuntimeDependencyInjection => "RuntimeDependencyInjection",
            Self::UnresolvedImport => "UnresolvedImport",
            Self::MissingProjectConfig => "MissingProjectConfig",
            Self::MissingDependency => "MissingDependency",
            Self::FrameworkMagic => "FrameworkMagic",
            Self::MacroOrPreprocessor => "MacroOrPreprocessor",
            Self::BuildVariantAmbiguity => "BuildVariantAmbiguity",
            Self::ConflictingFacts => "ConflictingFacts",
            Self::StaleEvidence => "StaleEvidence",
            Self::InsufficientSupport => "InsufficientSupport",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "DynamicImport" => Ok(Self::DynamicImport),
            "MonkeyPatch" => Ok(Self::MonkeyPatch),
            "PytestFixtureInjection" => Ok(Self::PytestFixtureInjection),
            "RuntimeDependencyInjection" => Ok(Self::RuntimeDependencyInjection),
            "UnresolvedImport" => Ok(Self::UnresolvedImport),
            "MissingProjectConfig" => Ok(Self::MissingProjectConfig),
            "MissingDependency" => Ok(Self::MissingDependency),
            "FrameworkMagic" => Ok(Self::FrameworkMagic),
            "MacroOrPreprocessor" => Ok(Self::MacroOrPreprocessor),
            "BuildVariantAmbiguity" => Ok(Self::BuildVariantAmbiguity),
            "ConflictingFacts" => Ok(Self::ConflictingFacts),
            "StaleEvidence" => Ok(Self::StaleEvidence),
            "InsufficientSupport" => Ok(Self::InsufficientSupport),
            _ => Err(format!("unsupported unknown reason code {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedUnknown {
    pub class: UnknownClass,
    pub reason: UnknownReasonCode,
    pub affected_claim: String,
    pub recovery: Option<String>,
}

impl TypedUnknown {
    pub fn new(
        class: UnknownClass,
        reason: UnknownReasonCode,
        affected_claim: impl Into<String>,
        recovery: Option<String>,
    ) -> Result<Self, String> {
        let affected_claim = affected_claim.into();
        if affected_claim.trim().is_empty() {
            return Err("typed UNKNOWN affected claim must not be empty".to_string());
        }
        if recovery
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("typed UNKNOWN recovery must not be empty".to_string());
        }
        Ok(Self {
            class,
            reason,
            affected_claim,
            recovery,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternClassification {
    DominantPattern {
        family_id: FamilyId,
        evidence: Vec<Evidence>,
    },
    Variation {
        family_id: FamilyId,
        slot: String,
        evidence: Vec<Evidence>,
    },
    Exception {
        family_id: FamilyId,
        reason: String,
        evidence: Vec<Evidence>,
    },
    Unknown {
        reason: UnknownReason,
    },
}

impl PatternClassification {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_represents_unknown_without_stringly_status() {
        let classification = PatternClassification::Unknown {
            reason: UnknownReason::CompetingFamilies,
        };

        assert!(classification.is_unknown());
    }

    #[test]
    fn typed_unknown_classes_use_stable_protocol_tokens() {
        let values = [
            (UnknownClass::Blocking, "blocking_unknown"),
            (UnknownClass::NonBlocking, "non_blocking_unknown"),
            (UnknownClass::Recoverable, "recoverable_unknown"),
            (UnknownClass::Irreducible, "irreducible_unknown"),
        ];

        for (class, protocol_value) in values {
            assert_eq!(class.as_protocol_str(), protocol_value);
            assert_eq!(UnknownClass::parse_protocol_str(protocol_value), Ok(class));
        }
        assert!(UnknownClass::parse_protocol_str("unknown").is_err());
    }

    #[test]
    fn typed_unknown_reason_codes_use_stable_protocol_tokens() {
        let values = [
            (UnknownReasonCode::DynamicImport, "DynamicImport"),
            (UnknownReasonCode::MonkeyPatch, "MonkeyPatch"),
            (
                UnknownReasonCode::PytestFixtureInjection,
                "PytestFixtureInjection",
            ),
            (
                UnknownReasonCode::RuntimeDependencyInjection,
                "RuntimeDependencyInjection",
            ),
            (UnknownReasonCode::UnresolvedImport, "UnresolvedImport"),
            (
                UnknownReasonCode::MissingProjectConfig,
                "MissingProjectConfig",
            ),
            (UnknownReasonCode::MissingDependency, "MissingDependency"),
            (UnknownReasonCode::FrameworkMagic, "FrameworkMagic"),
            (
                UnknownReasonCode::MacroOrPreprocessor,
                "MacroOrPreprocessor",
            ),
            (
                UnknownReasonCode::BuildVariantAmbiguity,
                "BuildVariantAmbiguity",
            ),
            (UnknownReasonCode::ConflictingFacts, "ConflictingFacts"),
            (UnknownReasonCode::StaleEvidence, "StaleEvidence"),
            (
                UnknownReasonCode::InsufficientSupport,
                "InsufficientSupport",
            ),
        ];

        for (reason, protocol_value) in values {
            assert_eq!(reason.as_protocol_str(), protocol_value);
            assert_eq!(
                UnknownReasonCode::parse_protocol_str(protocol_value),
                Ok(reason)
            );
        }
        assert!(UnknownReasonCode::parse_protocol_str("LowConfidence").is_err());
    }

    #[test]
    fn typed_unknown_requires_affected_claim() {
        assert!(TypedUnknown::new(
            UnknownClass::Blocking,
            UnknownReasonCode::StaleEvidence,
            "   ",
            None
        )
        .is_err());
    }
}
