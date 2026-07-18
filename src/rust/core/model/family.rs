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

/// Maximum distinct observed profiles enumerated for one variation dimension.
/// A dimension with more than this many observed profiles is truncated and its
/// [`VariationConstraint::observed_profiles_truncated`] flag is set. A profile is
/// only ever an *observed* member value, never the full legal space.
pub const CONSTRAINT_OBSERVED_PROFILE_CAP: usize = 8;

/// Maximum representative member ids retained per variation dimension. Bounds the
/// example set so a large family cannot contribute an unbounded id list.
pub const CONSTRAINT_REPRESENTATIVE_MEMBER_CAP: usize = 8;

/// Where a required-or-prohibited feature constraint was derived from. Every
/// origin names a family-membership decision authority; a constraint is never
/// derived from notes, storage order, or free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureConstraintOrigin {
    /// A characteristic-profile prefix the role's compatibility rule forces to be
    /// equal across every member (the `characteristic_profile_prefixes` authority).
    CharacteristicProfile,
    /// The universal framework-role identity every member shares.
    FrameworkRoleIdentity,
    /// The universal non-empty support-family intersection every member shares.
    SupportFamilyIntersection,
    /// A feature whose mere presence the per-language compatibility rule rejects,
    /// excluding membership (the `unknown_blocker:` incompatibility rule).
    IncompatibilityBlocker,
}

impl FeatureConstraintOrigin {
    /// Stable persisted/serialized token for this origin.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::CharacteristicProfile => "characteristic_profile",
            Self::FrameworkRoleIdentity => "framework_role_identity",
            Self::SupportFamilyIntersection => "support_family_intersection",
            Self::IncompatibilityBlocker => "incompatibility_blocker",
        }
    }

    /// Parse a persisted origin token back into the typed vocabulary.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "characteristic_profile" => Ok(Self::CharacteristicProfile),
            "framework_role_identity" => Ok(Self::FrameworkRoleIdentity),
            "support_family_intersection" => Ok(Self::SupportFamilyIntersection),
            "incompatibility_blocker" => Ok(Self::IncompatibilityBlocker),
            _ => Err(format!("unsupported feature constraint origin {value}")),
        }
    }
}

/// How a [`FeatureConstraint`]'s `values` bind a candidate member. The membership
/// rules are not uniform: some prefixes demand exact equality (including the
/// empty case), some demand only that a shared core be present, and the blocker
/// rule forbids any value. The semantics field makes each constraint
/// self-describing so a required-but-empty constraint is never confused with the
/// prohibited-presence wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureConstraintSemantics {
    /// Every member carries exactly these (non-empty) values under the prefix.
    Equal,
    /// Every member carries no value under the prefix — equality against the
    /// empty set. `values` is always empty and this is distinct from
    /// [`Self::ProhibitedPresence`], which lives on the prohibited axis.
    EqualEmpty,
    /// Every member's values under the prefix are a superset of these — the
    /// values shared by the whole cluster. Members may carry additional values,
    /// because membership requires only pairwise overlap, not equality.
    MustContain,
    /// No member may carry any value under the prefix; presence excludes
    /// membership. `values` is always empty (a wildcard over the prefix).
    ProhibitedPresence,
}

impl FeatureConstraintSemantics {
    /// Stable persisted/serialized token for this semantics.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::EqualEmpty => "equal_empty",
            Self::MustContain => "must_contain",
            Self::ProhibitedPresence => "prohibited_presence",
        }
    }

    /// Parse a persisted semantics token back into the typed vocabulary.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "equal" => Ok(Self::Equal),
            "equal_empty" => Ok(Self::EqualEmpty),
            "must_contain" => Ok(Self::MustContain),
            "prohibited_presence" => Ok(Self::ProhibitedPresence),
            _ => Err(format!("unsupported feature constraint semantics {value}")),
        }
    }

    /// Whether this semantics binds against an empty `values` list.
    pub fn requires_empty_values(self) -> bool {
        matches!(self, Self::EqualEmpty | Self::ProhibitedPresence)
    }

    /// Whether this semantics belongs on the prohibited axis (a membership
    /// blocker) rather than the required-feature axis. Only
    /// [`Self::ProhibitedPresence`] is a prohibition; the equality and subset
    /// semantics are required-feature bindings. The two axes never share a
    /// semantics, so a stored constraint can be checked against the array it was
    /// read from.
    pub fn is_prohibition(self) -> bool {
        matches!(self, Self::ProhibitedPresence)
    }
}

/// A typed feature constraint drawn from a family-membership decision authority.
///
/// `prefix` is a feature namespace prefix such as `decorator_shape:`. `values`
/// are the values under that prefix that the authority binds, and `semantics`
/// says how they bind (equality, equality against empty, subset/must-contain, or
/// prohibited presence). Only [`FeatureConstraintSemantics::Equal`] and
/// [`FeatureConstraintSemantics::MustContain`] carry non-empty `values`; the
/// empty-set semantics carry an empty list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureConstraint {
    pub prefix: String,
    pub values: Vec<String>,
    pub origin: FeatureConstraintOrigin,
    pub semantics: FeatureConstraintSemantics,
}

/// A dimension along which members of a family legally differ.
///
/// `observed_profiles` enumerates only the non-empty profiles actually seen among
/// the current members — never the full legal space — which is why `observed_only`
/// is always `true`. `includes_absent_profile` records that at least one member
/// carried no value under the dimension (an observed "absent" profile), so the
/// variation decision stays consistent with the co-persisted variation slots.
/// When the distinct profiles exceed [`CONSTRAINT_OBSERVED_PROFILE_CAP`] the
/// enumeration is truncated and `observed_profiles_truncated` is set.
/// `representative_member_ids` names bounded example members, aligned by index
/// with `observed_profiles`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariationConstraint {
    pub dimension: String,
    pub observed_profiles: Vec<String>,
    pub observed_profiles_truncated: bool,
    pub includes_absent_profile: bool,
    pub representative_member_ids: Vec<String>,
    /// Always `true`: only observed values are claimed legal.
    pub observed_only: bool,
}

/// An unresolved obligation carried by a constraint profile. It reuses the typed
/// `UNKNOWN` vocabulary verbatim (`class`/`reason`/`affected_claim`/`recovery`)
/// so a constraint profile never opens a second, free-text obligation channel.
pub type UnknownObligation = TypedUnknown;

/// A source-backed implementation specification for a pattern family.
///
/// Every field is derived only from the family-membership decision authorities
/// (per-language compatibility rules, characteristic-profile prefixes, variation
/// slots, and the typed `UNKNOWN` vocabulary); none of it is ever read from
/// notes, storage order, or free text. Variations are observed-only: a value that
/// was never observed among the members is never claimed legal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyConstraintProfile {
    /// Features every member is bound to under its `semantics`: the framework-role
    /// identity and the role's characteristic prefixes (`Equal`/`EqualEmpty`), and
    /// the shared support-family core (`MustContain`, a subset rule — members may
    /// carry additional support families since membership needs only pairwise
    /// overlap). The support-family entry is omitted when the shared core is empty
    /// (complete-link clustering permits an empty global core) or when the role's
    /// characteristic prefixes already bind `support_family:` by equality.
    pub required_equal_features: Vec<FeatureConstraint>,
    /// Dimensions along which members legally differ, enumerated observed-only.
    pub allowed_variations: Vec<VariationConstraint>,
    /// Feature values whose presence excludes membership for this family key.
    pub prohibited_or_blocking_features: Vec<FeatureConstraint>,
    /// The claim's non-blocking unknowns plus the always-present runtime
    /// equivalence obligation, in claim order.
    pub unresolved_obligations: Vec<UnknownObligation>,
}

impl FamilyConstraintProfile {
    /// An empty profile with no derived constraints or obligations. Used by
    /// hydration and test fixtures; a real emitted family always carries at least
    /// the runtime-equivalence obligation and its universal requirements.
    pub fn empty() -> Self {
        Self {
            required_equal_features: Vec::new(),
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: Vec::new(),
        }
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

    #[test]
    fn feature_constraint_origins_round_trip_through_stable_tokens() {
        for origin in [
            FeatureConstraintOrigin::CharacteristicProfile,
            FeatureConstraintOrigin::FrameworkRoleIdentity,
            FeatureConstraintOrigin::SupportFamilyIntersection,
            FeatureConstraintOrigin::IncompatibilityBlocker,
        ] {
            assert_eq!(
                FeatureConstraintOrigin::parse_token(origin.as_token()),
                Ok(origin)
            );
            // Origin tokens carry no path, symbol, or repository-specific text.
            assert!(origin
                .as_token()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'));
        }
        assert!(FeatureConstraintOrigin::parse_token("notes").is_err());
    }

    #[test]
    fn feature_constraint_semantics_round_trip_and_declare_empty_binding() {
        for semantics in [
            FeatureConstraintSemantics::Equal,
            FeatureConstraintSemantics::EqualEmpty,
            FeatureConstraintSemantics::MustContain,
            FeatureConstraintSemantics::ProhibitedPresence,
        ] {
            assert_eq!(
                FeatureConstraintSemantics::parse_token(semantics.as_token()),
                Ok(semantics)
            );
            assert!(semantics
                .as_token()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'));
        }
        assert!(FeatureConstraintSemantics::parse_token("subset").is_err());
        // Only the empty-set semantics bind against an empty `values` list.
        assert!(FeatureConstraintSemantics::EqualEmpty.requires_empty_values());
        assert!(FeatureConstraintSemantics::ProhibitedPresence.requires_empty_values());
        assert!(!FeatureConstraintSemantics::Equal.requires_empty_values());
        assert!(!FeatureConstraintSemantics::MustContain.requires_empty_values());
    }

    #[test]
    fn unknown_obligation_reuses_the_typed_unknown_vocabulary() {
        // The obligation type is exactly the typed UNKNOWN vocabulary, so a
        // constraint profile can never introduce a second, free-text channel.
        let obligation: UnknownObligation = TypedUnknown::new(
            UnknownClass::NonBlocking,
            UnknownReasonCode::FrameworkMagic,
            "family:example:runtime_equivalence",
            Some("add semantic-worker evidence".to_string()),
        )
        .expect("valid obligation");
        assert_eq!(obligation.class, UnknownClass::NonBlocking);
        assert_eq!(obligation.reason, UnknownReasonCode::FrameworkMagic);
    }

    #[test]
    fn empty_constraint_profile_has_no_derived_constraints() {
        let profile = FamilyConstraintProfile::empty();
        assert!(profile.required_equal_features.is_empty());
        assert!(profile.allowed_variations.is_empty());
        assert!(profile.prohibited_or_blocking_features.is_empty());
        assert!(profile.unresolved_obligations.is_empty());
        // Named caps are a stable, documented part of the contract.
        assert_eq!(CONSTRAINT_OBSERVED_PROFILE_CAP, 8);
        assert_eq!(CONSTRAINT_REPRESENTATIVE_MEMBER_CAP, 8);
    }
}
