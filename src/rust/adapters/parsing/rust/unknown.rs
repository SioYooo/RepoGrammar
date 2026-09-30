use super::{RUST_ANCHOR_ENGINE, RUST_ANCHOR_METHOD};
use crate::core::model::{
    CodeUnit, CodeUnitId, Evidence, FactCertainty, FactOrigin, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId,
};
use crate::ports::parser::{ParseError, SourceDocument};

#[derive(Debug, Clone, Copy)]
pub(super) struct RustUnknownSpec<'a> {
    pub(super) reason: &'static str,
    pub(super) affected_claim: &'static str,
    pub(super) kind: &'a str,
    pub(super) note: &'static str,
}

pub(super) fn fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: RustUnknownSpec<'_>,
) -> Result<SemanticFact, ParseError> {
    fact_with_assumptions(document, unit, start_byte, end_byte, spec, Vec::new())
}

pub(super) fn fact_with_assumptions(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: RustUnknownSpec<'_>,
    extra_assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    let mut assumptions = vec![
        format!("affected_claim={}", spec.affected_claim),
        format!("rust_unknown_kind={}", spec.kind),
    ];
    assumptions.extend(extra_assumptions);
    assumptions.sort();
    assumptions.dedup();
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
        assumptions,
    })
}

pub(super) fn project_config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: RustUnknownSpec<'_>,
) -> Result<SemanticFact, ParseError> {
    fact(document, unit, start_byte, end_byte, spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::sha256_hex;
    use crate::core::model::{ContentHash, Language, RepositoryRevision};
    use crate::ports::parser::SourceParser;

    #[test]
    fn borrowed_syntax_kind_is_copied_into_owned_unknown_fact() {
        let text = "fn run() { example!(); }";
        let document = || SourceDocument {
            path: "fixture.rs",
            language: Language::Rust,
            text,
            content_hash: ContentHash::new(format!("sha256:{}", sha256_hex(text.as_bytes())))
                .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
        };
        let report = super::super::RustSyntaxParser
            .parse(document())
            .expect("real grammar parses macro");
        let kind = String::from("macro_invocation");
        let result = fact(
            &document(),
            &report.units[0],
            0,
            text.len(),
            RustUnknownSpec {
                reason: "MacroOrPreprocessor",
                affected_claim: "rust_macro_expansion",
                kind: &kind,
                note: "Rust macro syntax is not expanded",
            },
        )
        .expect("borrowed parser kind accepted");
        drop(kind);
        assert_eq!(result.kind, SemanticFactKind::Unknown);
        assert_eq!(result.certainty, FactCertainty::Unknown);
        assert!(result
            .assumptions
            .contains(&"rust_unknown_kind=macro_invocation".to_string()));
    }
}
