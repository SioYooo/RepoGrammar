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

/// Evidence-backed prevalence of an emitted pattern family within its
/// `FamilyKey` peer group.
///
/// Minimum support only qualifies a cluster for emission; prevalence decides
/// whether that emitted family actually *dominates* its eligible peers. Every
/// counter is metadata derived from indexed structure — never repository source
/// text.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyPrevalence {
    /// Peers that could in principle claim membership: units of the same key
    /// whose supported evidence survived the blocking filter. This is the
    /// classification denominator.
    pub eligible_peer_count: usize,
    /// Support of the emitted cluster (its member count).
    pub supported_member_count: usize,
    /// `supported_member_count / eligible_peer_count`. `None` only when the
    /// denominator is zero, which cannot happen for an emitted claim; the
    /// `Option` is kept for schema honesty.
    pub coverage_ratio: Option<f64>,
    /// Other ready clusters of the same key competing for dominance.
    pub competing_ready_family_count: usize,
    /// Largest support among the competing ready clusters, `0` if none.
    pub largest_competing_support: usize,
    /// Peers whose support was emptied by a blocking `UNKNOWN`. Excluded from the
    /// denominator but recorded for reliability assessment.
    pub blocked_peer_count: usize,
    /// Peers that had no role-compatible support facts at all. Excluded from the
    /// denominator but recorded for reliability assessment.
    pub unsupported_peer_count: usize,
    /// One deterministic sentence, drawn from a fixed template set, explaining
    /// the classification. Never free text from repository content.
    pub classification_reason: String,
}

/// The four-token family prevalence vocabulary. Minimum support no longer
/// implies dominance: a supported cluster is classified by how it compares to
/// its eligible peers and competing ready families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyPrevalenceClass {
    /// Coverage is high and the cluster clearly outnumbers any competitor.
    DominantPattern,
    /// Meets minimum support but does not dominate its eligible peers.
    SupportedPattern,
    /// Covers less than a third of eligible peers, or is smaller than a
    /// competing ready family.
    MinorityPattern,
    /// The denominator is unreliable because blocking unknowns dominate the peer
    /// group.
    UnknownPrevalence,
}

impl FamilyPrevalenceClass {
    /// Stable persisted/serialized token for this classification.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::DominantPattern => "DOMINANT_PATTERN",
            Self::SupportedPattern => "SUPPORTED_PATTERN",
            Self::MinorityPattern => "MINORITY_PATTERN",
            Self::UnknownPrevalence => "UNKNOWN_PREVALENCE",
        }
    }

    /// Parse a persisted classification token back into the typed vocabulary.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "DOMINANT_PATTERN" => Ok(Self::DominantPattern),
            "SUPPORTED_PATTERN" => Ok(Self::SupportedPattern),
            "MINORITY_PATTERN" => Ok(Self::MinorityPattern),
            "UNKNOWN_PREVALENCE" => Ok(Self::UnknownPrevalence),
            _ => Err(format!(
                "unsupported family prevalence classification {value}"
            )),
        }
    }
}

/// Integer inputs to [`assess_family_prevalence`]. Classification is decided on
/// exact integers to avoid float-edge flakiness; the reported `coverage_ratio`
/// float is derived separately by [`coverage_ratio`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrevalenceInputs {
    pub eligible_peer_count: usize,
    pub supported_member_count: usize,
    pub competing_ready_family_count: usize,
    pub largest_competing_support: usize,
    pub blocked_peer_count: usize,
    pub unsupported_peer_count: usize,
}

/// The classification decision plus its deterministic reason sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyPrevalenceAssessment {
    pub class: FamilyPrevalenceClass,
    pub reason: String,
}

// Coverage strictly below `1/3` of eligible peers is a `MINORITY_PATTERN`.
const MINORITY_COVERAGE_NUMERATOR: usize = 1;
const MINORITY_COVERAGE_DENOMINATOR: usize = 3;
// Coverage at or above `3/5` (0.6) of eligible peers is required for
// `DOMINANT_PATTERN`.
const DOMINANT_COVERAGE_NUMERATOR: usize = 3;
const DOMINANT_COVERAGE_DENOMINATOR: usize = 5;
// `DOMINANT_PATTERN` requires support of at least this multiple of the largest
// competing ready family.
const DOMINANT_COMPETING_SUPPORT_MULTIPLE: usize = 2;
// `DOMINANT_PATTERN` requires at least this absolute support. Emitted clusters
// always meet minimum support (>= 2), so this guard is an invariant check.
const DOMINANT_MINIMUM_SUPPORT: usize = 2;

/// Authoritative family prevalence classifier.
///
/// This is the single decision entrypoint; callers route, persist, and format
/// the result but must not re-derive the classification from raw counters. The
/// comparisons are exact integer cross-multiplications so threshold edges are
/// deterministic and float-free.
pub fn assess_family_prevalence(inputs: &PrevalenceInputs) -> FamilyPrevalenceAssessment {
    let eligible = inputs.eligible_peer_count;
    let support = inputs.supported_member_count;
    let competitor = inputs.largest_competing_support;

    // A denominator dominated by blocking unknowns cannot support a prevalence
    // judgement, so refuse to claim any prevalence at all.
    if inputs.blocked_peer_count > eligible {
        return FamilyPrevalenceAssessment {
            class: FamilyPrevalenceClass::UnknownPrevalence,
            reason: format!(
                "blocked peers {} exceed eligible peers {eligible}",
                inputs.blocked_peer_count
            ),
        };
    }

    // coverage_ratio < 1/3  <=>  3 * support < eligible.
    let below_minority_coverage = MINORITY_COVERAGE_DENOMINATOR.saturating_mul(support)
        < MINORITY_COVERAGE_NUMERATOR.saturating_mul(eligible);
    if below_minority_coverage {
        return FamilyPrevalenceAssessment {
            class: FamilyPrevalenceClass::MinorityPattern,
            reason: format!("coverage {support}/{eligible} below one-third of eligible peers"),
        };
    }
    if support < competitor {
        return FamilyPrevalenceAssessment {
            class: FamilyPrevalenceClass::MinorityPattern,
            reason: format!("support {support} below competing ready support {competitor}"),
        };
    }

    // coverage_ratio >= 3/5  <=>  5 * support >= 3 * eligible.
    let meets_dominant_coverage = DOMINANT_COVERAGE_DENOMINATOR.saturating_mul(support)
        >= DOMINANT_COVERAGE_NUMERATOR.saturating_mul(eligible);
    let dominates_competitor =
        support >= DOMINANT_COMPETING_SUPPORT_MULTIPLE.saturating_mul(competitor);
    let meets_minimum_support = support >= DOMINANT_MINIMUM_SUPPORT;
    if meets_dominant_coverage && dominates_competitor && meets_minimum_support {
        let reason = if competitor == 0 {
            format!("coverage {support}/{eligible} with no competing ready family")
        } else {
            format!(
                "coverage {support}/{eligible} dominates largest competing support {competitor}"
            )
        };
        return FamilyPrevalenceAssessment {
            class: FamilyPrevalenceClass::DominantPattern,
            reason,
        };
    }

    FamilyPrevalenceAssessment {
        class: FamilyPrevalenceClass::SupportedPattern,
        reason: format!("coverage {support}/{eligible} without dominant margin"),
    }
}

/// Reported coverage ratio for an emitted family. `None` only when the
/// denominator is zero, which cannot happen for an emitted claim.
pub fn coverage_ratio(eligible_peer_count: usize, supported_member_count: usize) -> Option<f64> {
    if eligible_peer_count == 0 {
        None
    } else {
        Some(supported_member_count as f64 / eligible_peer_count as f64)
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

    fn inputs(
        eligible: usize,
        support: usize,
        competitors: usize,
        largest_competitor: usize,
        blocked: usize,
        unsupported: usize,
    ) -> PrevalenceInputs {
        PrevalenceInputs {
            eligible_peer_count: eligible,
            supported_member_count: support,
            competing_ready_family_count: competitors,
            largest_competing_support: largest_competitor,
            blocked_peer_count: blocked,
            unsupported_peer_count: unsupported,
        }
    }

    #[test]
    fn family_prevalence_tokens_round_trip() {
        for class in [
            FamilyPrevalenceClass::DominantPattern,
            FamilyPrevalenceClass::SupportedPattern,
            FamilyPrevalenceClass::MinorityPattern,
            FamilyPrevalenceClass::UnknownPrevalence,
        ] {
            assert_eq!(
                FamilyPrevalenceClass::parse_token(class.as_token()),
                Ok(class)
            );
        }
        assert!(FamilyPrevalenceClass::parse_token("VARIATION").is_err());
    }

    #[test]
    fn full_coverage_with_no_competitor_is_dominant() {
        // Sanity anchor: fastapi_route-shaped input (support 30, sole cluster).
        let assessment = assess_family_prevalence(&inputs(30, 30, 0, 0, 0, 0));
        assert_eq!(assessment.class, FamilyPrevalenceClass::DominantPattern);
        assert_eq!(
            assessment.reason,
            "coverage 30/30 with no competing ready family"
        );
    }

    #[test]
    fn three_members_with_nine_eligible_peers_are_not_dominant() {
        // Sanity anchor: a small cluster inside a much larger peer group.
        // Competing peers split below support -> no competing ready family.
        let split = assess_family_prevalence(&inputs(9, 3, 0, 0, 0, 0));
        assert_eq!(split.class, FamilyPrevalenceClass::SupportedPattern);
        // A single competing ready family larger than us -> minority.
        let outnumbered = assess_family_prevalence(&inputs(9, 3, 1, 6, 0, 0));
        assert_eq!(outnumbered.class, FamilyPrevalenceClass::MinorityPattern);
    }

    #[test]
    fn coverage_below_one_third_is_minority() {
        // 2/9 < 1/3.
        assert_eq!(
            assess_family_prevalence(&inputs(9, 2, 0, 0, 0, 0)).class,
            FamilyPrevalenceClass::MinorityPattern
        );
        // Exactly 1/3 (3/9) is not minority by coverage.
        assert_ne!(
            assess_family_prevalence(&inputs(9, 3, 0, 0, 0, 0)).class,
            FamilyPrevalenceClass::MinorityPattern
        );
    }

    #[test]
    fn support_below_a_competing_ready_family_is_minority() {
        // Coverage alone (4/10) is above one-third, but a larger competitor wins.
        assert_eq!(
            assess_family_prevalence(&inputs(10, 4, 1, 6, 0, 0)).class,
            FamilyPrevalenceClass::MinorityPattern
        );
    }

    #[test]
    fn dominant_coverage_edge_is_exact() {
        // coverage_ratio == 3/5 is dominant; just below is not.
        assert_eq!(
            assess_family_prevalence(&inputs(5, 3, 0, 0, 0, 0)).class,
            FamilyPrevalenceClass::DominantPattern
        );
        assert_eq!(
            assess_family_prevalence(&inputs(6, 3, 0, 0, 0, 0)).class,
            FamilyPrevalenceClass::SupportedPattern
        );
    }

    #[test]
    fn dominant_competing_support_edge_is_exact() {
        // support == 2 * largest competitor dominates; one less does not.
        assert_eq!(
            assess_family_prevalence(&inputs(6, 4, 1, 2, 0, 0)).class,
            FamilyPrevalenceClass::DominantPattern
        );
        assert_eq!(
            assess_family_prevalence(&inputs(5, 3, 1, 2, 0, 0)).class,
            FamilyPrevalenceClass::SupportedPattern
        );
    }

    #[test]
    fn unreliable_denominator_is_unknown_prevalence() {
        // blocked_peer_count > eligible_peer_count.
        let assessment = assess_family_prevalence(&inputs(3, 3, 0, 0, 4, 0));
        assert_eq!(assessment.class, FamilyPrevalenceClass::UnknownPrevalence);
        assert_eq!(assessment.reason, "blocked peers 4 exceed eligible peers 3");
    }

    #[test]
    fn coverage_ratio_is_none_only_for_zero_denominator() {
        assert_eq!(coverage_ratio(0, 0), None);
        assert_eq!(coverage_ratio(30, 30), Some(1.0));
        assert_eq!(coverage_ratio(4, 1), Some(0.25));
    }
}
