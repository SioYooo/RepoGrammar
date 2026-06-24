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
}
