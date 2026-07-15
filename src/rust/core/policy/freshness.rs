//! Freshness policy connects evidence to repository revision and content hashes.

use crate::core::model::{
    ContentHash, FactCertainty, SemanticFactKind, TypedUnknown, UnknownClass, UnknownReasonCode,
};

pub const SEMANTIC_FACT_CLAIM_INPUT: &str = "semantic_fact_claim_input";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceFreshness {
    Fresh,
    Unknown(TypedUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimInputReadiness {
    EligibleInput,
    Blocked { unknown: TypedUnknown },
}

impl ClaimInputReadiness {
    pub fn is_eligible_input(&self) -> bool {
        matches!(self, Self::EligibleInput)
    }
}

pub fn content_hash_freshness(
    stored: &ContentHash,
    current: Option<&ContentHash>,
) -> EvidenceFreshness {
    match current {
        Some(current) if current == stored => EvidenceFreshness::Fresh,
        _ => EvidenceFreshness::Unknown(blocking_unknown(
            UnknownReasonCode::StaleEvidence,
            Some("run repogrammar resync".to_string()),
        )),
    }
}

pub fn semantic_fact_claim_input_readiness(
    kind: SemanticFactKind,
    certainty: FactCertainty,
    freshness: EvidenceFreshness,
) -> ClaimInputReadiness {
    if let EvidenceFreshness::Unknown(unknown) = freshness {
        return ClaimInputReadiness::Blocked { unknown };
    }

    if matches!(
        kind,
        SemanticFactKind::Unknown | SemanticFactKind::ProjectConfig
    ) {
        return ClaimInputReadiness::Blocked {
            unknown: blocking_unknown(UnknownReasonCode::InsufficientSupport, None),
        };
    }

    if certainty.supports_family_membership() {
        return ClaimInputReadiness::EligibleInput;
    }

    let reason = match certainty {
        FactCertainty::Conflicting => UnknownReasonCode::ConflictingFacts,
        FactCertainty::Structural | FactCertainty::FrameworkHeuristic | FactCertainty::Unknown => {
            UnknownReasonCode::InsufficientSupport
        }
        FactCertainty::Semantic | FactCertainty::DataflowDerived => unreachable!(
            "semantic and dataflow certainty are handled by supports_family_membership"
        ),
    };
    ClaimInputReadiness::Blocked {
        unknown: blocking_unknown(reason, None),
    }
}

fn blocking_unknown(reason: UnknownReasonCode, recovery: Option<String>) -> TypedUnknown {
    TypedUnknown::new(
        UnknownClass::Blocking,
        reason,
        SEMANTIC_FACT_CLAIM_INPUT,
        recovery,
    )
    .expect("semantic fact claim input affected claim is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(value: char) -> ContentHash {
        ContentHash::new(format!("sha256:{}", value.to_string().repeat(64))).expect("valid hash")
    }

    #[test]
    fn matching_content_hash_is_fresh() {
        let stored = hash('a');
        let current = hash('a');

        assert_eq!(
            content_hash_freshness(&stored, Some(&current)),
            EvidenceFreshness::Fresh
        );
    }

    #[test]
    fn missing_or_changed_content_hash_is_stale_unknown() {
        let stored = hash('a');
        let changed = hash('b');

        for freshness in [
            content_hash_freshness(&stored, None),
            content_hash_freshness(&stored, Some(&changed)),
        ] {
            let EvidenceFreshness::Unknown(unknown) = freshness else {
                panic!("stale evidence must become typed UNKNOWN");
            };
            assert_eq!(unknown.class, UnknownClass::Blocking);
            assert_eq!(unknown.reason, UnknownReasonCode::StaleEvidence);
            assert_eq!(unknown.affected_claim, SEMANTIC_FACT_CLAIM_INPUT);
            assert_eq!(unknown.recovery.as_deref(), Some("run repogrammar resync"));
        }
    }

    #[test]
    fn fresh_semantic_and_dataflow_facts_are_eligible_inputs_only() {
        for certainty in [FactCertainty::Semantic, FactCertainty::DataflowDerived] {
            let readiness = semantic_fact_claim_input_readiness(
                SemanticFactKind::ResolvedImport,
                certainty,
                EvidenceFreshness::Fresh,
            );

            assert_eq!(readiness, ClaimInputReadiness::EligibleInput);
            assert!(readiness.is_eligible_input());
        }
    }

    #[test]
    fn stale_freshness_blocks_even_semantic_facts() {
        let unknown = blocking_unknown(UnknownReasonCode::StaleEvidence, None);

        let readiness = semantic_fact_claim_input_readiness(
            SemanticFactKind::ResolvedImport,
            FactCertainty::Semantic,
            EvidenceFreshness::Unknown(unknown.clone()),
        );

        assert_eq!(readiness, ClaimInputReadiness::Blocked { unknown });
    }

    #[test]
    fn weak_certainty_cannot_become_claim_input() {
        for certainty in [
            FactCertainty::Structural,
            FactCertainty::FrameworkHeuristic,
            FactCertainty::Unknown,
        ] {
            let readiness = semantic_fact_claim_input_readiness(
                SemanticFactKind::ResolvedImport,
                certainty,
                EvidenceFreshness::Fresh,
            );
            let ClaimInputReadiness::Blocked { unknown } = readiness else {
                panic!("weak certainty must not be eligible claim input");
            };
            assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
        }
    }

    #[test]
    fn unknown_fact_kind_cannot_become_claim_input() {
        for kind in [SemanticFactKind::Unknown, SemanticFactKind::ProjectConfig] {
            let readiness = semantic_fact_claim_input_readiness(
                kind,
                FactCertainty::Semantic,
                EvidenceFreshness::Fresh,
            );
            let ClaimInputReadiness::Blocked { unknown } = readiness else {
                panic!("non-claim fact kind must not become eligible claim input");
            };

            assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
        }
    }

    #[test]
    fn conflicting_certainty_blocks_with_conflicting_facts() {
        let readiness = semantic_fact_claim_input_readiness(
            SemanticFactKind::ResolvedImport,
            FactCertainty::Conflicting,
            EvidenceFreshness::Fresh,
        );
        let ClaimInputReadiness::Blocked { unknown } = readiness else {
            panic!("conflicting facts must be blocked");
        };

        assert_eq!(unknown.reason, UnknownReasonCode::ConflictingFacts);
    }
}
