//! Source evidence attached to a classification or query result.

use super::{CodeUnitId, Provenance, SourceRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub code_unit_id: CodeUnitId,
    pub range: SourceRange,
    pub provenance: Provenance,
    pub note: String,
}

impl Evidence {
    pub fn new(
        code_unit_id: CodeUnitId,
        range: SourceRange,
        provenance: Provenance,
        note: impl Into<String>,
    ) -> Result<Self, String> {
        let note = note.into();
        if note.trim().is_empty() {
            Err("evidence note must not be empty".to_string())
        } else {
            Ok(Self {
                code_unit_id,
                range,
                provenance,
                note,
            })
        }
    }
}
