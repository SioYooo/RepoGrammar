//! Normalization converts unified IR into comparison-ready forms.

use crate::core::model::CodeUnitId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUnit {
    pub code_unit_id: CodeUnitId,
    pub structural_terms: Vec<String>,
}
