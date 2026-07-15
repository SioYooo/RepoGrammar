//! Abstention keeps low-confidence analysis from becoming a false claim.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstentionReason {
    LowConfidence,
    MultipleCompetingFamilies,
    DynamicRuntimeBehavior,
    UnsupportedTarget,
}
