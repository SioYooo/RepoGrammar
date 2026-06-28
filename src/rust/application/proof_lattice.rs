//! Shared helpers for promoting structural evidence into bounded support facts.

use crate::core::model::{
    CodeUnitId, Evidence, FactCertainty, FactOrigin, Provenance, RepositoryRevision, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId,
};
use crate::error::RepoGrammarError;
use crate::ports::index_store::IndexedCodeUnitRecord;
use std::collections::BTreeSet;

pub(crate) struct DerivedSupportSpec<'a> {
    pub(crate) engine: &'a str,
    pub(crate) method: &'a str,
    pub(crate) note: &'a str,
    pub(crate) assumptions: Vec<String>,
}

pub(crate) fn derived_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    repository_revision: &RepositoryRevision,
    spec: DerivedSupportSpec<'_>,
) -> Result<SemanticFact, RepoGrammarError> {
    Ok(SemanticFact {
        kind,
        subject: unit.id.clone(),
        target: Some(SymbolId::new(target).map_err(RepoGrammarError::InvalidInput)?),
        origin: FactOrigin {
            engine: spec.engine.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: spec.method.to_string(),
        },
        certainty: FactCertainty::DataflowDerived,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.clone()).map_err(RepoGrammarError::InvalidInput)?,
            SourceRange::new(unit.start_byte, unit.end_byte)
                .map_err(RepoGrammarError::InvalidInput)?,
            Provenance::new(
                &unit.path,
                unit.content_hash.clone(),
                repository_revision.clone(),
            )
            .map_err(RepoGrammarError::InvalidInput)?,
            spec.note,
        )
        .map_err(RepoGrammarError::InvalidInput)?,
        assumptions: spec.assumptions,
    })
}

pub(crate) fn derived_support_has_safe_origin(
    fact: &SemanticFact,
    engine: &str,
    method: &str,
    framework_role: &str,
    required_assumptions: &[String],
) -> bool {
    fact.certainty == FactCertainty::DataflowDerived
        && fact.origin.engine == engine
        && fact.origin.method == method
        && fact_has_assumption(fact, "provider_resolved=false")
        && fact_has_assumption(fact, &format!("framework_role={framework_role}"))
        && required_assumptions
            .iter()
            .all(|assumption| fact_has_assumption(fact, assumption))
}

pub(crate) fn add_variation_features_from_assumptions(
    entry: &mut BTreeSet<String>,
    assumptions: &[String],
    mappings: &[(&str, &str)],
    stable_token: impl Fn(&str) -> String,
) {
    for assumption in assumptions {
        for (prefix, feature_prefix) in mappings {
            if let Some(value) = assumption.strip_prefix(prefix) {
                entry.insert(format!("{feature_prefix}{}", stable_token(value)));
            }
        }
    }
}

fn fact_has_assumption(fact: &SemanticFact, expected: &str) -> bool {
    fact.assumptions
        .iter()
        .any(|assumption| assumption == expected)
}
