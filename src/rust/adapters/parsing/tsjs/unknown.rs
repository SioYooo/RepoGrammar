use super::{TSJS_ANCHOR_ENGINE, TSJS_ANCHOR_METHOD};
use crate::core::model::{
    CodeUnit, CodeUnitId, Evidence, FactCertainty, FactOrigin, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, SourceDocument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnknownAnchor {
    pub(super) reason: UnknownReasonCode,
    pub(super) affected_claim: &'static str,
    pub(super) kind: &'static str,
    pub(super) note: &'static str,
}

pub(super) fn fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    unknown: UnknownAnchor,
) -> Result<SemanticFact, ParseError> {
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let evidence = Evidence::new(
        CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
        SourceRange::new(unit.range.start_byte, unit.range.end_byte)
            .map_err(ParseError::Internal)?,
        provenance,
        unknown.note,
    )
    .map_err(ParseError::Internal)?;
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(
            SymbolId::new(unknown.reason.as_protocol_str()).map_err(ParseError::Internal)?,
        ),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: TSJS_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence,
        assumptions: vec![
            format!("affected_claim={}", unknown.affected_claim),
            format!("tsjs_unknown_kind={}", unknown.kind),
        ],
    })
}
