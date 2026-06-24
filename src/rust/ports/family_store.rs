//! Persistence port for pattern-family records.

use crate::core::model::{FamilyId, PatternClassification};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Unavailable(String),
    InvalidRecord(String),
}

pub trait FamilyStore {
    fn load_classification(
        &self,
        family_id: &FamilyId,
    ) -> Result<Option<PatternClassification>, StoreError>;
}
