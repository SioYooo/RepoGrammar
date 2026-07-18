//! Static-alignment policy vocabulary.
//!
//! These enums name the *static* relationship between a target code unit and a
//! pattern family's source-backed constraint profile. They deliberately never
//! encode a runtime-equivalence verdict: a static alignment certificate proves
//! only that a target's indexed feature profile matches (or violates) the
//! family's derived requirements, and the runtime-equivalence obligation stays an
//! explicit `UNKNOWN` in every certificate. The tokens are stable, source-free,
//! and safe for CLI/MCP protocols and telemetry.

/// Top-level static-alignment outcome. Never `PASS`/`FAIL`/`CONFORMS`: a family
/// membership decision authority can only witness *static* alignment, so the
/// vocabulary is scoped to what indexed structure can prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentStatus {
    /// Every required constraint matched, no deviation, and no blocking unknown.
    StaticallyAligned,
    /// At least one required-feature violation or a prohibited-presence match.
    StaticDeviation,
    /// No violation, but a blocking unknown, an unobserved variation, or degraded
    /// feature extraction prevents a clean static-alignment claim.
    PartialAlignment,
    /// No family or profile could be selected for the target, or the selection
    /// was ambiguous. Nothing was compared.
    InsufficientEvidence,
    /// The target could not be resolved or classified against any authority.
    Unknown,
}

impl AlignmentStatus {
    /// Stable protocol token. Uppercase and source-free.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::StaticallyAligned => "STATICALLY_ALIGNED",
            Self::StaticDeviation => "STATIC_DEVIATION",
            Self::PartialAlignment => "PARTIAL_ALIGNMENT",
            Self::InsufficientEvidence => "INSUFFICIENT_EVIDENCE",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Parse a persisted/serialized alignment token.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "STATICALLY_ALIGNED" => Ok(Self::StaticallyAligned),
            "STATIC_DEVIATION" => Ok(Self::StaticDeviation),
            "PARTIAL_ALIGNMENT" => Ok(Self::PartialAlignment),
            "INSUFFICIENT_EVIDENCE" => Ok(Self::InsufficientEvidence),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(format!("unsupported alignment status {value}")),
        }
    }

    /// Whether a family was actually selected and compared. Abstaining statuses
    /// (`InsufficientEvidence`, `Unknown`) never carry an alignment computation.
    pub fn is_abstaining(self) -> bool {
        matches!(self, Self::InsufficientEvidence | Self::Unknown)
    }

    /// The single authority mapping an alignment status onto its commitment class,
    /// shared by CLI/MCP query-outcome telemetry and the product-eval harness so a
    /// committed certificate is never miscounted as an abstention (and vice versa).
    pub fn outcome_class(self) -> AlignmentOutcomeClass {
        match self {
            // A committed certificate: the target resolved and was compared.
            Self::StaticallyAligned | Self::StaticDeviation => AlignmentOutcomeClass::Committed,
            // A partial certificate: compared, but not cleanly aligned.
            Self::PartialAlignment => AlignmentOutcomeClass::Partial,
            // Abstained: no family was selected/compared.
            Self::InsufficientEvidence | Self::Unknown => AlignmentOutcomeClass::Abstained,
        }
    }
}

/// The commitment class of an alignment status. `Committed` outcomes select and
/// compare a family; `Partial` compares but cannot claim clean alignment;
/// `Abstained` never selects a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentOutcomeClass {
    Committed,
    Partial,
    Abstained,
}

/// How the target relates to the comparison family. This is orthogonal to
/// [`AlignmentStatus`]: it names *membership standing*, not the feature-level
/// outcome. `Exception` is reserved for source-backed negative evidence — a
/// required-feature mismatch against the only ready family of the target's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRelationship {
    /// The target is a member of the comparison family.
    Member,
    /// A non-member that satisfies every required constraint but was not admitted
    /// (e.g. sub-support or a blocked sibling cluster).
    NearMiss,
    /// The target is a member of a competing ready family of the same key.
    CompetingPattern,
    /// A blocking unknown prevented membership.
    BlockedUnknown,
    /// The target's kind or role is not family-eligible for this key.
    OutOfScope,
    /// Source-backed negative evidence: the target violates a required feature of
    /// the only ready family of its key.
    Exception,
}

impl TargetRelationship {
    /// Stable protocol token. Uppercase and source-free.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Member => "MEMBER",
            Self::NearMiss => "NEAR_MISS",
            Self::CompetingPattern => "COMPETING_PATTERN",
            Self::BlockedUnknown => "BLOCKED_UNKNOWN",
            Self::OutOfScope => "OUT_OF_SCOPE",
            Self::Exception => "EXCEPTION",
        }
    }

    /// Parse a persisted/serialized relationship token.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "MEMBER" => Ok(Self::Member),
            "NEAR_MISS" => Ok(Self::NearMiss),
            "COMPETING_PATTERN" => Ok(Self::CompetingPattern),
            "BLOCKED_UNKNOWN" => Ok(Self::BlockedUnknown),
            "OUT_OF_SCOPE" => Ok(Self::OutOfScope),
            "EXCEPTION" => Ok(Self::Exception),
            _ => Err(format!("unsupported target relationship {value}")),
        }
    }
}

/// The kind of a single static deviation. Only the first four kinds are *required*
/// violations that force a [`AlignmentStatus::StaticDeviation`]; the last one,
/// [`Self::UnobservedVariation`], is deliberately *not* called illegal — a value
/// the family has simply never observed is a partial-alignment signal, not a
/// contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticDeviationKind {
    /// An `Equal` constraint's values differ from the target's.
    RequiredMismatch,
    /// An `EqualEmpty` constraint requires no value, but the target carries one.
    MustBeEmptyViolation,
    /// A `MustContain` (subset) core is not fully contained in the target.
    MissingRequiredCore,
    /// A prohibited feature is present on the target.
    ProhibitedPresence,
    /// The target's value on a variation dimension was never observed among the
    /// family's members. Explicitly *unobserved*, never *illegal*.
    UnobservedVariation,
    /// The target's value on a variation dimension is not among the *enumerated*
    /// observed profiles, but that enumeration was truncated at the cap — so
    /// "never observed" cannot be proven. A conservative partial-alignment signal,
    /// never a violation and never an unobserved claim.
    TruncatedObservation,
    /// A required feature is absent, but the target carries a blocking unknown
    /// that plausibly suppressed the feature from the static view. Not a violation:
    /// an incomplete static view must not fabricate a deviation.
    BlockingSuppressedRequirement,
}

impl StaticDeviationKind {
    /// Stable protocol token. Lowercase and source-free.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::RequiredMismatch => "required_mismatch",
            Self::MustBeEmptyViolation => "must_be_empty_violation",
            Self::MissingRequiredCore => "missing_required_core",
            Self::ProhibitedPresence => "prohibited_presence",
            Self::UnobservedVariation => "unobserved_variation",
            Self::TruncatedObservation => "truncated_observation",
            Self::BlockingSuppressedRequirement => "blocking_suppressed_requirement",
        }
    }

    /// Parse a persisted/serialized deviation-kind token.
    pub fn parse_token(value: &str) -> Result<Self, String> {
        match value {
            "required_mismatch" => Ok(Self::RequiredMismatch),
            "must_be_empty_violation" => Ok(Self::MustBeEmptyViolation),
            "missing_required_core" => Ok(Self::MissingRequiredCore),
            "prohibited_presence" => Ok(Self::ProhibitedPresence),
            "unobserved_variation" => Ok(Self::UnobservedVariation),
            "truncated_observation" => Ok(Self::TruncatedObservation),
            "blocking_suppressed_requirement" => Ok(Self::BlockingSuppressedRequirement),
            _ => Err(format!("unsupported static deviation kind {value}")),
        }
    }

    /// Whether this deviation is a *required-feature violation* that forces a
    /// `STATIC_DEVIATION` outcome. Unobserved/truncated variations and
    /// blocking-suppressed requirements are partial-alignment signals, not
    /// violations.
    pub fn is_violation(self) -> bool {
        !matches!(
            self,
            Self::UnobservedVariation
                | Self::TruncatedObservation
                | Self::BlockingSuppressedRequirement
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_status_tokens_round_trip_and_are_source_free() {
        for status in [
            AlignmentStatus::StaticallyAligned,
            AlignmentStatus::StaticDeviation,
            AlignmentStatus::PartialAlignment,
            AlignmentStatus::InsufficientEvidence,
            AlignmentStatus::Unknown,
        ] {
            assert_eq!(AlignmentStatus::parse_token(status.as_token()), Ok(status));
            assert!(status
                .as_token()
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_'));
        }
        // The vocabulary must never leak a runtime-conformance verdict.
        assert!(AlignmentStatus::parse_token("PASS").is_err());
        assert!(AlignmentStatus::parse_token("FAIL").is_err());
        assert!(AlignmentStatus::parse_token("CONFORMS").is_err());
    }

    #[test]
    fn abstaining_statuses_carry_no_computation() {
        assert!(AlignmentStatus::InsufficientEvidence.is_abstaining());
        assert!(AlignmentStatus::Unknown.is_abstaining());
        assert!(!AlignmentStatus::StaticallyAligned.is_abstaining());
        assert!(!AlignmentStatus::StaticDeviation.is_abstaining());
        assert!(!AlignmentStatus::PartialAlignment.is_abstaining());
    }

    #[test]
    fn target_relationship_tokens_round_trip() {
        for relationship in [
            TargetRelationship::Member,
            TargetRelationship::NearMiss,
            TargetRelationship::CompetingPattern,
            TargetRelationship::BlockedUnknown,
            TargetRelationship::OutOfScope,
            TargetRelationship::Exception,
        ] {
            assert_eq!(
                TargetRelationship::parse_token(relationship.as_token()),
                Ok(relationship)
            );
        }
        assert!(TargetRelationship::parse_token("CONFORMS").is_err());
    }

    #[test]
    fn static_deviation_kind_tokens_round_trip_and_flag_violations() {
        for kind in [
            StaticDeviationKind::RequiredMismatch,
            StaticDeviationKind::MustBeEmptyViolation,
            StaticDeviationKind::MissingRequiredCore,
            StaticDeviationKind::ProhibitedPresence,
            StaticDeviationKind::UnobservedVariation,
            StaticDeviationKind::TruncatedObservation,
            StaticDeviationKind::BlockingSuppressedRequirement,
        ] {
            assert_eq!(StaticDeviationKind::parse_token(kind.as_token()), Ok(kind));
        }
        // Unobserved/truncated variation and blocking-suppressed requirements are
        // deliberately not violations.
        assert!(!StaticDeviationKind::UnobservedVariation.is_violation());
        assert!(!StaticDeviationKind::TruncatedObservation.is_violation());
        assert!(!StaticDeviationKind::BlockingSuppressedRequirement.is_violation());
        assert!(StaticDeviationKind::RequiredMismatch.is_violation());
        assert!(StaticDeviationKind::MustBeEmptyViolation.is_violation());
        assert!(StaticDeviationKind::MissingRequiredCore.is_violation());
        assert!(StaticDeviationKind::ProhibitedPresence.is_violation());
        assert!(StaticDeviationKind::parse_token("illegal_variation").is_err());
    }
}
