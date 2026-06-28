use super::{RUST_ANCHOR_ENGINE, RUST_ANCHOR_METHOD};
use crate::core::model::{
    CodeUnit, CodeUnitId, Evidence, FactCertainty, FactOrigin, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId,
};
use crate::ports::parser::{ParseError, SourceDocument};

#[derive(Debug, Clone, Copy)]
pub(super) struct RustUnknownSpec {
    pub(super) reason: &'static str,
    pub(super) affected_claim: &'static str,
    pub(super) kind: &'static str,
    pub(super) note: &'static str,
}

pub(super) fn fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: RustUnknownSpec,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(spec.reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: RUST_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: RUST_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?,
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            spec.note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={}", spec.affected_claim),
            format!("rust_unknown_kind={}", spec.kind),
        ],
    })
}

pub(super) fn project_config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: RustUnknownSpec,
) -> Result<SemanticFact, ParseError> {
    fact(document, unit, start_byte, end_byte, spec)
}
