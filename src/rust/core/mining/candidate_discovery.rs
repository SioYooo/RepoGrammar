//! Candidate discovery narrows possible analogues before expensive comparison.

use crate::core::mining::fingerprint::StructuralFingerprint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateQuery {
    pub fingerprint: StructuralFingerprint,
    pub max_candidates: usize,
}
