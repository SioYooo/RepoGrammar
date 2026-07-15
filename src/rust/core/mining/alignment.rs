//! Structural alignment compares normalized units without transport concerns.

use crate::core::model::CodeUnitId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentInput {
    pub left: CodeUnitId,
    pub right: CodeUnitId,
}
