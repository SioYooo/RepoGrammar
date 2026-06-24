//! Compatibility policy decides whether a target can be compared to a family.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityResult {
    Compatible,
    Incompatible { reason: String },
    Unknown { reason: String },
}
