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

/// Internal claim-scoped effect of a typed `UNKNOWN`.
///
/// This axis is intentionally separate from [`ResolutionClass`]. Public
/// protocols continue to serialize the legacy [`UnknownClass`] vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimImpact {
    Blocking,
    NonBlocking,
}

impl ClaimImpact {
    /// Converts only the legacy values that actually encode family claim
    /// impact. Recovery-oriented legacy classes are deliberately rejected.
    pub(crate) fn from_legacy_family_class(class: UnknownClass) -> Option<Self> {
        match class {
            UnknownClass::Blocking => Some(Self::Blocking),
            UnknownClass::NonBlocking => Some(Self::NonBlocking),
            UnknownClass::Recoverable | UnknownClass::Irreducible => None,
        }
    }

    pub(crate) fn as_legacy_unknown_class(self) -> UnknownClass {
        match self {
            Self::Blocking => UnknownClass::Blocking,
            Self::NonBlocking => UnknownClass::NonBlocking,
        }
    }
}

/// Internal recoverability of a typed `UNKNOWN` under the current analyzer
/// configuration and registered recovery mechanisms.
///
/// This axis must not be used to decide whether a claim is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolutionClass {
    Recoverable,
    Irreducible,
}

impl ResolutionClass {
    pub(crate) fn as_legacy_unknown_class(self) -> UnknownClass {
        match self {
            Self::Recoverable => UnknownClass::Recoverable,
            Self::Irreducible => UnknownClass::Irreducible,
        }
    }
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

/// The kind of semantic question a typed `UNKNOWN` poses — its *obligation*.
///
/// This is a first-class refinement layered on top of a typed `UNKNOWN`; it never
/// replaces or weakens the `UNKNOWN` contract. Every semantic `UNKNOWN` still
/// blocks or abstains exactly as before; the obligation only names *what would
/// have to be proven* to discharge it, so an agent can see whether an `UNKNOWN`
/// is a provider-resolvable question (type identity, symbol binding, dispatch
/// target, framework identity, build variant, macro expansion, external
/// dependency), a runtime-defined residual that stays `UNKNOWN` by design
/// (`RuntimeIrreducible`, ADR-0015 class c), or a governance state that is not a
/// semantic obligation at all (`Governance`: stale, conflicting, or insufficient
/// support). The recoverable-vs-irreducible axis remains [`UnknownClass`]; this
/// enum is orthogonal and adds the obligation *kind*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticObligation {
    TypeIdentity,
    SymbolBinding,
    DispatchTarget,
    FrameworkIdentity,
    BuildVariant,
    MacroExpansion,
    ExternalDependency,
    RuntimeIrreducible,
    Governance,
}

impl SemanticObligation {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::TypeIdentity => "type_identity",
            Self::SymbolBinding => "symbol_binding",
            Self::DispatchTarget => "dispatch_target",
            Self::FrameworkIdentity => "framework_identity",
            Self::BuildVariant => "build_variant",
            Self::MacroExpansion => "macro_expansion",
            Self::ExternalDependency => "external_dependency",
            Self::RuntimeIrreducible => "runtime_irreducible",
            Self::Governance => "governance",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "type_identity" => Ok(Self::TypeIdentity),
            "symbol_binding" => Ok(Self::SymbolBinding),
            "dispatch_target" => Ok(Self::DispatchTarget),
            "framework_identity" => Ok(Self::FrameworkIdentity),
            "build_variant" => Ok(Self::BuildVariant),
            "macro_expansion" => Ok(Self::MacroExpansion),
            "external_dependency" => Ok(Self::ExternalDependency),
            "runtime_irreducible" => Ok(Self::RuntimeIrreducible),
            "governance" => Ok(Self::Governance),
            _ => Err(format!("unsupported semantic obligation {value}")),
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
    fn claim_impact_accepts_only_legacy_family_effect_classes() {
        assert_eq!(
            ClaimImpact::from_legacy_family_class(UnknownClass::Blocking),
            Some(ClaimImpact::Blocking)
        );
        assert_eq!(
            ClaimImpact::from_legacy_family_class(UnknownClass::NonBlocking),
            Some(ClaimImpact::NonBlocking)
        );
        assert_eq!(
            ClaimImpact::from_legacy_family_class(UnknownClass::Recoverable),
            None
        );
        assert_eq!(
            ClaimImpact::from_legacy_family_class(UnknownClass::Irreducible),
            None
        );
    }

    #[test]
    fn semantic_obligations_use_stable_source_free_protocol_tokens() {
        let values = [
            (SemanticObligation::TypeIdentity, "type_identity"),
            (SemanticObligation::SymbolBinding, "symbol_binding"),
            (SemanticObligation::DispatchTarget, "dispatch_target"),
            (SemanticObligation::FrameworkIdentity, "framework_identity"),
            (SemanticObligation::BuildVariant, "build_variant"),
            (SemanticObligation::MacroExpansion, "macro_expansion"),
            (
                SemanticObligation::ExternalDependency,
                "external_dependency",
            ),
            (
                SemanticObligation::RuntimeIrreducible,
                "runtime_irreducible",
            ),
            (SemanticObligation::Governance, "governance"),
        ];

        for (obligation, protocol_value) in values {
            assert_eq!(obligation.as_protocol_str(), protocol_value);
            assert_eq!(
                SemanticObligation::parse_protocol_str(protocol_value),
                Ok(obligation)
            );
            // The obligation vocabulary is a fixed, source-free enum: tokens carry
            // no path, symbol, or repository-specific text.
            assert!(protocol_value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'));
        }
        assert!(SemanticObligation::parse_protocol_str("nope").is_err());
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
