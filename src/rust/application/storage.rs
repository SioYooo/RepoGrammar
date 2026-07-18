//! Storage use-case boundary.

use crate::application::recovery::{recovery_guidance, RecoveryAction};
use crate::core::model::{
    FactCertainty, FamilyConstraintProfile, FamilyPrevalenceClass, FeatureConstraint, IrEdgeLabel,
    IrNodeKind, SemanticFactKind, VariationConstraint,
};
use crate::core::policy::paths::{looks_like_absolute_path, RepoRelativePathError};
use crate::error::RepoGrammarError;
use crate::ports::family_store::{
    family_evidence_covered_claim_is_supported, ActiveFamilies, ActiveFamily,
    ActiveFamilySearchSummaries, FamilyConstraintProfileStore, FamilyStore,
    IndexedFamilyConstraintProfileRecord, IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord,
    IndexedFamilyRecord, IndexedVariationSlotRecord, StoreError,
};
use crate::ports::index_store::{
    GenerationHandle, GenerationPruneReport, GenerationPruneRequest, GenerationRetentionStore,
    IndexCompactReport, IndexCompactRequest, IndexMaintenanceStore, IndexStorageCleanStore,
    IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord, IndexedIrEdgeRecord,
    IndexedIrNodeRecord, IndexedSemanticFactRecord, StorageCleanReport, StorageCleanRequest,
    StorageInspection,
};

pub const DEFAULT_RETAINED_INACTIVE_GENERATIONS: usize = 2;

pub fn prepare_index_generation(
    store: &impl IndexStore,
) -> Result<GenerationHandle, RepoGrammarError> {
    store.prepare_next_generation().map_err(index_store_error)
}

pub fn record_indexed_file(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    file: &IndexedFileRecord,
) -> Result<(), RepoGrammarError> {
    validate_indexed_file(file)?;
    store
        .record_indexed_file(generation, file)
        .map_err(index_store_error)
}

pub fn remove_indexed_file(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    path: &str,
) -> Result<(), RepoGrammarError> {
    validate_repo_relative_path(path)?;
    store
        .remove_indexed_file(generation, path)
        .map_err(index_store_error)
}

pub fn record_code_unit(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    unit: &IndexedCodeUnitRecord,
) -> Result<(), RepoGrammarError> {
    validate_code_unit(unit)?;
    store
        .record_code_unit(generation, unit)
        .map_err(index_store_error)
}

pub fn record_ir_node(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    node: &IndexedIrNodeRecord,
) -> Result<(), RepoGrammarError> {
    validate_ir_node(node)?;
    store
        .record_ir_node(generation, node)
        .map_err(index_store_error)
}

pub fn record_ir_edge(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    edge: &IndexedIrEdgeRecord,
) -> Result<(), RepoGrammarError> {
    validate_ir_edge(edge)?;
    store
        .record_ir_edge(generation, edge)
        .map_err(index_store_error)
}

pub fn record_semantic_fact(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    fact: &IndexedSemanticFactRecord,
) -> Result<(), RepoGrammarError> {
    validate_semantic_fact(fact)?;
    store
        .record_semantic_fact(generation, fact)
        .map_err(index_store_error)
}

pub fn record_family(
    store: &(impl FamilyStore + ?Sized),
    generation: &GenerationHandle,
    family: &IndexedFamilyRecord,
) -> Result<(), RepoGrammarError> {
    validate_family(family)?;
    store
        .record_family(generation, family)
        .map_err(family_store_error)
}

pub fn record_family_member(
    store: &(impl FamilyStore + ?Sized),
    generation: &GenerationHandle,
    member: &IndexedFamilyMemberRecord,
) -> Result<(), RepoGrammarError> {
    validate_family_member(member)?;
    store
        .record_family_member(generation, member)
        .map_err(family_store_error)
}

pub fn record_variation_slot(
    store: &(impl FamilyStore + ?Sized),
    generation: &GenerationHandle,
    slot: &IndexedVariationSlotRecord,
) -> Result<(), RepoGrammarError> {
    validate_variation_slot(slot)?;
    store
        .record_variation_slot(generation, slot)
        .map_err(family_store_error)
}

pub fn record_family_evidence(
    store: &(impl FamilyStore + ?Sized),
    generation: &GenerationHandle,
    evidence: &IndexedFamilyEvidenceRecord,
) -> Result<(), RepoGrammarError> {
    validate_family_evidence(evidence)?;
    store
        .record_family_evidence(generation, evidence)
        .map_err(family_store_error)
}

pub fn list_active_families(
    store: &(impl FamilyStore + ?Sized),
) -> Result<ActiveFamilies, RepoGrammarError> {
    store.list_active_families().map_err(family_store_error)
}

/// Read the bounded, source-free searchable-metadata projection of every active
/// family. Substrate for deterministic term-based retrieval; not yet routed into
/// the production fuzzy lookup path.
pub fn list_active_family_search_summaries(
    store: &(impl FamilyStore + ?Sized),
) -> Result<ActiveFamilySearchSummaries, RepoGrammarError> {
    store
        .list_active_family_search_summaries()
        .map_err(family_store_error)
}

pub fn show_family(
    store: &impl FamilyStore,
    family_id: &str,
) -> Result<Option<ActiveFamily>, RepoGrammarError> {
    validate_semantic_text_field("family id", family_id)?;
    store.show_family(family_id).map_err(family_store_error)
}

pub fn record_family_constraint_profile(
    store: &(impl FamilyConstraintProfileStore + ?Sized),
    generation: &GenerationHandle,
    record: &IndexedFamilyConstraintProfileRecord,
) -> Result<(), RepoGrammarError> {
    validate_family_constraint_profile(record)?;
    store
        .record_family_constraint_profile(generation, record)
        .map_err(family_store_error)
}

pub fn show_family_constraint_profile(
    store: &impl FamilyConstraintProfileStore,
    family_id: &str,
) -> Result<Option<FamilyConstraintProfile>, RepoGrammarError> {
    validate_semantic_text_field("family id", family_id)?;
    store
        .show_family_constraint_profile(family_id)
        .map_err(family_store_error)
}

pub fn validate_index_generation(
    store: &impl IndexStore,
    generation: &GenerationHandle,
) -> Result<(), RepoGrammarError> {
    store
        .validate_generation(generation)
        .map_err(index_store_error)
}

pub fn activate_index_generation(
    store: &impl IndexStore,
    generation: &GenerationHandle,
) -> Result<(), RepoGrammarError> {
    store
        .activate_generation(generation)
        .map_err(index_store_error)
}

pub fn inspect_index_storage(
    store: &impl IndexStore,
) -> Result<StorageInspection, RepoGrammarError> {
    store.inspect().map_err(index_store_error)
}

pub fn prune_index_generations(
    store: &impl GenerationRetentionStore,
    repository_root: &str,
    state_dir_override: Option<&str>,
    request: GenerationPruneRequest,
) -> Result<GenerationPruneReport, RepoGrammarError> {
    let _index_lock =
        crate::application::repository::acquire_index_lock(repository_root, state_dir_override)?;
    store.prune_generations(request).map_err(index_store_error)
}

pub fn compact_index_storage(
    store: &impl IndexMaintenanceStore,
    repository_root: &str,
    state_dir_override: Option<&str>,
    request: IndexCompactRequest,
) -> Result<IndexCompactReport, RepoGrammarError> {
    let _index_lock =
        crate::application::repository::acquire_index_lock(repository_root, state_dir_override)?;
    store.compact_storage(request).map_err(index_store_error)
}

pub fn clean_index_storage(
    store: &impl IndexStorageCleanStore,
    repository_root: &str,
    state_dir_override: Option<&str>,
    request: StorageCleanRequest,
) -> Result<StorageCleanReport, RepoGrammarError> {
    let _index_lock =
        crate::application::repository::acquire_index_lock(repository_root, state_dir_override)?;
    store.clean_storage(request).map_err(index_store_error)
}

fn validate_indexed_file(file: &IndexedFileRecord) -> Result<(), RepoGrammarError> {
    if file.path.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "indexed file path must not be empty".to_string(),
        ));
    }
    validate_repo_relative_path(&file.path)?;
    if file.language.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "indexed file language must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn validate_code_unit(unit: &IndexedCodeUnitRecord) -> Result<(), RepoGrammarError> {
    if unit.id.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "code unit id must not be empty".to_string(),
        ));
    }
    if unit.path.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "code unit path must not be empty".to_string(),
        ));
    }
    validate_repo_relative_path(&unit.path)?;
    if unit.language.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "code unit language must not be empty".to_string(),
        ));
    }
    if unit.kind.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "code unit kind must not be empty".to_string(),
        ));
    }
    if unit.start_byte > unit.end_byte {
        return Err(RepoGrammarError::InvalidInput(
            "code unit source range start must not exceed end".to_string(),
        ));
    }
    Ok(())
}

fn validate_ir_node(node: &IndexedIrNodeRecord) -> Result<(), RepoGrammarError> {
    for (field_name, value) in [
        ("IR node id", node.id.as_str()),
        ("IR node code unit id", node.code_unit_id.as_str()),
        ("IR node kind", node.kind.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RepoGrammarError::InvalidInput(format!(
                "{field_name} must not be empty"
            )));
        }
        validate_semantic_text_field(field_name, value)?;
    }
    if node.id != format!("ir:{}", node.code_unit_id) {
        return Err(RepoGrammarError::InvalidInput(
            "IR node id must be derived from code unit id".to_string(),
        ));
    }
    if node.payload_json.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "IR node payload must not be empty".to_string(),
        ));
    }
    IrNodeKind::parse_protocol_str(&node.kind)
        .map_err(|error| RepoGrammarError::InvalidInput(error.to_string()))?;
    validate_empty_object_payload("IR node payload", &node.payload_json)?;
    Ok(())
}

fn validate_ir_edge(edge: &IndexedIrEdgeRecord) -> Result<(), RepoGrammarError> {
    for (field_name, value) in [
        ("IR edge from node id", edge.from_node_id.as_str()),
        ("IR edge to node id", edge.to_node_id.as_str()),
        ("IR edge label", edge.label.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RepoGrammarError::InvalidInput(format!(
                "{field_name} must not be empty"
            )));
        }
        validate_semantic_text_field(field_name, value)?;
    }
    if edge.from_node_id == edge.to_node_id {
        return Err(RepoGrammarError::InvalidInput(
            "IR edge must not point to itself".to_string(),
        ));
    }
    IrEdgeLabel::parse_protocol_str(&edge.label)
        .map_err(|error| RepoGrammarError::InvalidInput(error.to_string()))?;
    Ok(())
}

fn validate_semantic_fact(fact: &IndexedSemanticFactRecord) -> Result<(), RepoGrammarError> {
    for (field_name, value) in [
        ("semantic fact id", fact.fact_id.as_str()),
        ("semantic fact kind", fact.kind.as_str()),
        ("semantic fact subject", fact.subject.as_str()),
        ("semantic fact certainty", fact.certainty.as_str()),
        ("semantic fact origin engine", fact.origin_engine.as_str()),
        (
            "semantic fact origin engine version",
            fact.origin_engine_version.as_str(),
        ),
        ("semantic fact origin method", fact.origin_method.as_str()),
        ("semantic fact evidence id", fact.evidence_id.as_str()),
        ("semantic fact code unit id", fact.code_unit_id.as_str()),
        ("semantic fact path", fact.path.as_str()),
        ("semantic fact note", fact.note.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RepoGrammarError::InvalidInput(format!(
                "{field_name} must not be empty"
            )));
        }
    }
    if fact
        .target
        .as_ref()
        .is_some_and(|target| target.trim().is_empty())
    {
        return Err(RepoGrammarError::InvalidInput(
            "semantic fact target must not be empty when present".to_string(),
        ));
    }
    SemanticFactKind::parse_protocol_str(&fact.kind)
        .map_err(|error| RepoGrammarError::InvalidInput(error.to_string()))?;
    FactCertainty::parse_protocol_str(&fact.certainty)
        .map_err(|error| RepoGrammarError::InvalidInput(error.to_string()))?;
    for (field_name, value) in [
        ("semantic fact id", fact.fact_id.as_str()),
        ("semantic fact subject", fact.subject.as_str()),
        ("semantic fact origin engine", fact.origin_engine.as_str()),
        (
            "semantic fact origin engine version",
            fact.origin_engine_version.as_str(),
        ),
        ("semantic fact origin method", fact.origin_method.as_str()),
        ("semantic fact evidence id", fact.evidence_id.as_str()),
        ("semantic fact code unit id", fact.code_unit_id.as_str()),
        ("semantic fact note", fact.note.as_str()),
    ] {
        validate_semantic_text_field(field_name, value)?;
    }
    if let Some(target) = &fact.target {
        validate_semantic_text_field("semantic fact target", target)?;
    }
    for assumption in &fact.assumptions {
        if assumption.trim().is_empty() {
            return Err(RepoGrammarError::InvalidInput(
                "semantic fact assumptions must not contain empty values".to_string(),
            ));
        }
        validate_semantic_text_field("semantic fact assumption", assumption)?;
    }
    validate_repo_relative_path(&fact.path)?;
    if fact.start_byte > fact.end_byte {
        return Err(RepoGrammarError::InvalidInput(
            "semantic fact source range start must not exceed end".to_string(),
        ));
    }
    Ok(())
}

fn validate_family(family: &IndexedFamilyRecord) -> Result<(), RepoGrammarError> {
    validate_family_text_field("family id", &family.family_id)?;
    if FamilyPrevalenceClass::parse_token(&family.classification).is_err() {
        return Err(RepoGrammarError::InvalidInput(
            "family classification is unsupported".to_string(),
        ));
    }
    if family.prevalence.classification_reason.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "family classification reason must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn validate_family_constraint_profile(
    record: &IndexedFamilyConstraintProfileRecord,
) -> Result<(), RepoGrammarError> {
    validate_family_text_field("family constraint profile family id", &record.family_id)?;
    let profile = &record.profile;
    for constraint in &profile.required_equal_features {
        validate_feature_constraint("required-equal feature", constraint, false)?;
    }
    for constraint in &profile.prohibited_or_blocking_features {
        validate_feature_constraint("prohibited feature", constraint, true)?;
    }
    for variation in &profile.allowed_variations {
        validate_variation_constraint(variation)?;
    }
    for obligation in &profile.unresolved_obligations {
        validate_family_text_field(
            "constraint profile obligation affected claim",
            &obligation.affected_claim,
        )?;
        if let Some(recovery) = &obligation.recovery {
            validate_family_text_field("constraint profile obligation recovery", recovery)?;
        }
    }
    Ok(())
}

fn validate_feature_constraint(
    label: &str,
    constraint: &FeatureConstraint,
    prohibited_axis: bool,
) -> Result<(), RepoGrammarError> {
    validate_family_text_field(&format!("{label} prefix"), &constraint.prefix)?;
    // The prohibited axis carries only prohibited-presence blockers; the required
    // axis carries only equality/subset bindings. Reject a cross-axis semantics.
    if constraint.semantics.is_prohibition() != prohibited_axis {
        return Err(RepoGrammarError::InvalidInput(format!(
            "{label} semantics do not belong to its axis"
        )));
    }
    // Empty-set semantics (equal-empty, prohibited-presence) bind an empty value
    // list; equality and must-contain bind a non-empty one.
    if constraint.semantics.requires_empty_values() != constraint.values.is_empty() {
        return Err(RepoGrammarError::InvalidInput(format!(
            "{label} values are inconsistent with its semantics"
        )));
    }
    for value in &constraint.values {
        validate_family_text_field(&format!("{label} value"), value)?;
    }
    Ok(())
}

fn validate_variation_constraint(variation: &VariationConstraint) -> Result<(), RepoGrammarError> {
    if !variation.observed_only {
        return Err(RepoGrammarError::InvalidInput(
            "variation constraint must be observed-only".to_string(),
        ));
    }
    validate_family_text_field("variation dimension", &variation.dimension)?;
    for profile in &variation.observed_profiles {
        validate_family_text_field("variation observed profile", profile)?;
    }
    for member_id in &variation.representative_member_ids {
        validate_family_text_field("variation representative member id", member_id)?;
    }
    Ok(())
}

fn validate_family_member(member: &IndexedFamilyMemberRecord) -> Result<(), RepoGrammarError> {
    validate_family_text_field("family member family id", &member.family_id)?;
    validate_family_text_field("family member code unit id", &member.code_unit_id)?;
    validate_family_text_field("family member role", &member.role)?;
    Ok(())
}

fn validate_variation_slot(slot: &IndexedVariationSlotRecord) -> Result<(), RepoGrammarError> {
    validate_family_text_field("variation slot family id", &slot.family_id)?;
    validate_family_text_field("variation slot id", &slot.slot_id)?;
    validate_family_text_field("variation slot description", &slot.description)?;
    Ok(())
}

fn validate_family_evidence(
    evidence: &IndexedFamilyEvidenceRecord,
) -> Result<(), RepoGrammarError> {
    validate_family_text_field("family evidence id", &evidence.evidence_id)?;
    validate_family_text_field("family evidence family id", &evidence.family_id)?;
    validate_family_text_field("family evidence code unit id", &evidence.code_unit_id)?;
    validate_family_evidence_covered_claims(&evidence.covered_claims)?;
    validate_repo_relative_path(&evidence.path)?;
    validate_family_text_field("family evidence note", &evidence.note)?;
    if evidence.start_byte > evidence.end_byte {
        return Err(RepoGrammarError::InvalidInput(
            "family evidence source range start must not exceed end".to_string(),
        ));
    }
    Ok(())
}

fn validate_family_evidence_covered_claims(claims: &[String]) -> Result<(), RepoGrammarError> {
    if claims.is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "family evidence covered claims must not be empty".to_string(),
        ));
    }
    let mut seen = Vec::new();
    for claim in claims {
        if claim.trim().is_empty() {
            return Err(RepoGrammarError::InvalidInput(
                "family evidence covered claims must not contain empty values".to_string(),
            ));
        }
        validate_family_text_field("family evidence covered claim", claim)?;
        if !family_evidence_covered_claim_is_supported(claim) {
            return Err(RepoGrammarError::InvalidInput(
                "family evidence covered claim is unsupported".to_string(),
            ));
        }
        if seen
            .iter()
            .any(|seen: &&String| seen.as_str() == claim.as_str())
        {
            return Err(RepoGrammarError::InvalidInput(
                "family evidence covered claims must be unique".to_string(),
            ));
        }
        seen.push(claim);
    }
    Ok(())
}

fn validate_family_text_field(field_name: &str, value: &str) -> Result<(), RepoGrammarError> {
    if value.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(format!(
            "{field_name} must not be empty"
        )));
    }
    validate_semantic_text_field(field_name, value)
}

fn validate_empty_object_payload(
    field_name: &str,
    payload_json: &str,
) -> Result<(), RepoGrammarError> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_| RepoGrammarError::InvalidInput(format!("{field_name} must be valid JSON")))?;
    if value == serde_json::json!({}) {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(format!(
            "{field_name} must be an empty JSON object until typed IR attributes are implemented"
        )))
    }
}

fn validate_semantic_text_field(field_name: &str, value: &str) -> Result<(), RepoGrammarError> {
    if value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains("://")
        || looks_like_embedded_absolute_path(value)
        || looks_like_source_snippet(value)
    {
        Err(RepoGrammarError::InvalidInput(format!(
            "{field_name} contains unsupported content"
        )))
    } else {
        Ok(())
    }
}

fn looks_like_embedded_absolute_path(value: &str) -> bool {
    value.split_whitespace().any(looks_like_absolute_path)
}

fn looks_like_source_snippet(value: &str) -> bool {
    let trimmed = value.trim_start();
    value.contains("=>")
        || (value.contains('=') && value.contains(';'))
        || value.contains('{')
        || value.contains('}')
        || trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("export ")
}

fn validate_repo_relative_path(path: &str) -> Result<(), RepoGrammarError> {
    crate::core::policy::paths::validate_repo_relative_path(path).map_err(|error| {
        let message = match error {
            RepoRelativePathError::Traversal => "path must not traverse outside repository",
            RepoRelativePathError::Empty
            | RepoRelativePathError::Absolute
            | RepoRelativePathError::Backslash
            | RepoRelativePathError::ControlCharacter
            | RepoRelativePathError::UriLike => "path must be repository-relative",
        };
        RepoGrammarError::InvalidInput(message.to_string())
    })
}

/// Combine an adapter schema-outdated message with the authoritative resync
/// recovery guidance from the recovery classifier vocabulary.
fn schema_outdated_message(message: &str) -> String {
    format!("{message}; {}", recovery_guidance(RecoveryAction::Resync))
}

fn index_store_error(error: IndexStoreError) -> RepoGrammarError {
    match error {
        IndexStoreError::SchemaVersionOutdated(message) => {
            RepoGrammarError::InvalidInput(schema_outdated_message(&message))
        }
        IndexStoreError::Unavailable(message)
        | IndexStoreError::InvalidState(message)
        | IndexStoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}

fn family_store_error(error: StoreError) -> RepoGrammarError {
    match error {
        StoreError::SchemaVersionOutdated(message) => {
            RepoGrammarError::InvalidInput(schema_outdated_message(&message))
        }
        StoreError::Unavailable(message)
        | StoreError::InvalidState(message)
        | StoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::ports::family_store::{
        ActiveFamilies, ActiveFamily, ActiveFamilyCandidates, ActiveFamilyEvidenceProjection,
        ActiveFamilySearchSummaries, ActiveFamilySummaries, IndexedFamilyCandidateRecord,
        IndexedFamilyConstraintProfileRecord, IndexedFamilyEvidenceProjectionRecord,
        IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedFamilyRecord,
        IndexedFamilySearchSummaryRecord, IndexedFamilySummaryRecord, IndexedVariationSlotRecord,
    };
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveRepoShapeStats, ActiveSemanticFacts, IndexStorageLayout, STORAGE_SCHEMA_VERSION,
    };

    struct FakeStore;

    impl IndexStore for FakeStore {
        fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
            Ok(GenerationHandle {
                generation_id: "gen-000001".to_string(),
            })
        }

        fn record_indexed_file(
            &self,
            _generation: &GenerationHandle,
            _file: &IndexedFileRecord,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn remove_indexed_file(
            &self,
            _generation: &GenerationHandle,
            _path: &str,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn record_code_unit(
            &self,
            _generation: &GenerationHandle,
            _unit: &IndexedCodeUnitRecord,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn record_ir_node(
            &self,
            _generation: &GenerationHandle,
            _node: &IndexedIrNodeRecord,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn record_ir_edge(
            &self,
            _generation: &GenerationHandle,
            _edge: &IndexedIrEdgeRecord,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn record_semantic_fact(
            &self,
            _generation: &GenerationHandle,
            _fact: &IndexedSemanticFactRecord,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError> {
            Ok(ActiveIndexedFiles {
                generation_id: "gen-000001".to_string(),
                files: Vec::new(),
            })
        }

        fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
            Ok(ActiveCodeUnits {
                generation_id: "gen-000001".to_string(),
                units: Vec::new(),
            })
        }

        fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError> {
            Ok(ActiveSemanticFacts {
                generation_id: "gen-000001".to_string(),
                facts: Vec::new(),
            })
        }

        fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError> {
            Ok(ActiveIrGraph {
                generation_id: "gen-000001".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }

        fn load_active_claim_input_snapshot(
            &self,
        ) -> Result<ActiveClaimInputSnapshot, IndexStoreError> {
            Ok(ActiveClaimInputSnapshot {
                generation_id: "gen-000001".to_string(),
                files: Vec::new(),
                units: Vec::new(),
                ir_nodes: Vec::new(),
                ir_edges: Vec::new(),
                semantic_facts: Vec::new(),
            })
        }

        fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError> {
            Ok(ActiveRepoShapeStats {
                generation_id: "gen-000001".to_string(),
                indexed_file_count: 0,
                indexed_code_unit_count: 0,
                semantic_fact_count: 0,
                eligible_code_units: 0,
                family_count: 1,
                family_member_count: 1,
                covered_code_units: 1,
                by_language: Vec::new(),
            })
        }

        fn validate_generation(
            &self,
            _generation: &GenerationHandle,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn activate_generation(
            &self,
            _generation: &GenerationHandle,
        ) -> Result<(), IndexStoreError> {
            Ok(())
        }

        fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
            Ok(StorageInspection {
                layout: IndexStorageLayout::Mutable,
                mutable_database_present: true,
                legacy_generation_layout_present: false,
                wal_bytes: Some(0),
                shm_bytes: Some(0),
                active_generation: Some("gen-000001".to_string()),
                schema_version: Some(STORAGE_SCHEMA_VERSION),
                code_unit_count: Some(0),
                dependency_record_count: Some(0),
                dirty_record_count: Some(0),
                journal_mode: Some("wal".to_string()),
                foreign_keys_enabled: Some(true),
                busy_timeout_ms: Some(5_000),
                temp_store: Some("memory".to_string()),
                integrity_check: Some("ok".to_string()),
            })
        }
    }

    impl FamilyStore for FakeStore {
        fn record_family(
            &self,
            _generation: &GenerationHandle,
            _family: &IndexedFamilyRecord,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        fn record_family_member(
            &self,
            _generation: &GenerationHandle,
            _member: &IndexedFamilyMemberRecord,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        fn record_variation_slot(
            &self,
            _generation: &GenerationHandle,
            _slot: &IndexedVariationSlotRecord,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        fn record_family_evidence(
            &self,
            _generation: &GenerationHandle,
            _evidence: &IndexedFamilyEvidenceRecord,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        fn list_active_families(&self) -> Result<ActiveFamilies, StoreError> {
            Ok(ActiveFamilies {
                generation_id: "gen-000001".to_string(),
                families: vec![family()],
            })
        }

        fn list_active_family_summaries(&self) -> Result<ActiveFamilySummaries, StoreError> {
            Ok(ActiveFamilySummaries {
                generation_id: "gen-000001".to_string(),
                families: vec![IndexedFamilySummaryRecord {
                    family_id: family().family_id,
                    classification: family().classification,
                    support: 1,
                    prevalence: family().prevalence,
                }],
            })
        }

        fn list_active_family_evidence_projection(
            &self,
        ) -> Result<ActiveFamilyEvidenceProjection, StoreError> {
            let evidence = family_evidence();
            Ok(ActiveFamilyEvidenceProjection {
                generation_id: "gen-000001".to_string(),
                rows: vec![IndexedFamilyEvidenceProjectionRecord {
                    family_id: evidence.family_id,
                    path: evidence.path,
                    content_hash: evidence.content_hash,
                }],
            })
        }

        fn list_active_family_search_summaries(
            &self,
        ) -> Result<ActiveFamilySearchSummaries, StoreError> {
            Ok(ActiveFamilySearchSummaries {
                generation_id: "gen-000001".to_string(),
                families: vec![IndexedFamilySearchSummaryRecord {
                    family_id: family().family_id,
                    language: "typescript".to_string(),
                    code_unit_kind: "module".to_string(),
                    framework_role: family_member().role,
                    classification: family().classification,
                    support: 1,
                    prevalence: family().prevalence,
                    evidence_path_components: vec!["a.ts".to_string(), "src".to_string()],
                }],
            })
        }

        fn find_active_families_by_member(
            &self,
            code_unit_id: &str,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let candidates = if code_unit_id == family_member().code_unit_id {
                vec![IndexedFamilyCandidateRecord {
                    family_id: family().family_id,
                }]
            } else {
                Vec::new()
            };
            Ok(ActiveFamilyCandidates {
                generation_id: "gen-000001".to_string(),
                candidates,
                truncated: false,
            })
        }

        fn find_active_families_by_role(
            &self,
            role: &str,
            _limit: usize,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let candidates = if role == family_member().role {
                vec![IndexedFamilyCandidateRecord {
                    family_id: family().family_id,
                }]
            } else {
                Vec::new()
            };
            Ok(ActiveFamilyCandidates {
                generation_id: "gen-000001".to_string(),
                candidates,
                truncated: false,
            })
        }

        fn find_active_families_by_evidence_path(
            &self,
            path: &str,
            _limit: usize,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let candidates = if path == family_evidence().path {
                vec![IndexedFamilyCandidateRecord {
                    family_id: family().family_id,
                }]
            } else {
                Vec::new()
            };
            Ok(ActiveFamilyCandidates {
                generation_id: "gen-000001".to_string(),
                candidates,
                truncated: false,
            })
        }

        fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError> {
            if family_id == family().family_id {
                Ok(Some(ActiveFamily {
                    generation_id: "gen-000001".to_string(),
                    family: family(),
                    members: vec![family_member()],
                    variation_slots: vec![variation_slot()],
                    evidence: vec![family_evidence()],
                }))
            } else {
                Ok(None)
            }
        }
    }

    impl FamilyConstraintProfileStore for FakeStore {
        fn record_family_constraint_profile(
            &self,
            _generation: &GenerationHandle,
            _record: &IndexedFamilyConstraintProfileRecord,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        fn show_family_constraint_profile(
            &self,
            family_id: &str,
        ) -> Result<Option<FamilyConstraintProfile>, StoreError> {
            if family_id == family().family_id {
                Ok(Some(constraint_profile()))
            } else {
                Ok(None)
            }
        }
    }

    fn constraint_profile() -> FamilyConstraintProfile {
        crate::test_support::sample_family_constraint_profile()
    }

    fn constraint_profile_record() -> IndexedFamilyConstraintProfileRecord {
        IndexedFamilyConstraintProfileRecord {
            family_id: family().family_id,
            profile: constraint_profile(),
        }
    }

    fn file(path: &str) -> IndexedFileRecord {
        IndexedFileRecord {
            path: path.to_string(),
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            size_bytes: 42,
            language: "typescript".to_string(),
        }
    }

    fn semantic_fact() -> IndexedSemanticFactRecord {
        IndexedSemanticFactRecord {
            fact_id: "fact:src/a.ts#import:express".to_string(),
            kind: "RESOLVED_IMPORT".to_string(),
            subject: "src/a.ts#import:express".to_string(),
            target: Some("node_modules/@types/express/index.d.ts#Request".to_string()),
            certainty: "SEMANTIC".to_string(),
            origin_engine: "typescript".to_string(),
            origin_engine_version: "6.0.0".to_string(),
            origin_method: "compiler_api".to_string(),
            assumptions: Vec::new(),
            evidence_id: "evidence:fact:src/a.ts#import:express".to_string(),
            code_unit_id: "unit:src/a.ts#module:0-1".to_string(),
            path: "src/a.ts".to_string(),
            content_hash: file("src/a.ts").content_hash,
            start_byte: 0,
            end_byte: 1,
            note: "compiler resolved import target".to_string(),
        }
    }

    fn ir_node() -> IndexedIrNodeRecord {
        IndexedIrNodeRecord {
            id: "ir:unit:src/a.ts#module:0-1".to_string(),
            code_unit_id: "unit:src/a.ts#module:0-1".to_string(),
            kind: "module".to_string(),
            payload_json: "{}".to_string(),
        }
    }

    fn ir_edge() -> IndexedIrEdgeRecord {
        IndexedIrEdgeRecord {
            from_node_id: "ir:unit:src/a.ts#module:0-10".to_string(),
            to_node_id: "ir:unit:src/a.ts#function:1-9".to_string(),
            label: "contains".to_string(),
        }
    }

    fn family() -> IndexedFamilyRecord {
        IndexedFamilyRecord {
            family_id: "family:routes:read".to_string(),
            classification: "DOMINANT_PATTERN".to_string(),
            prevalence: crate::test_support::sample_family_prevalence(),
        }
    }

    fn family_member() -> IndexedFamilyMemberRecord {
        IndexedFamilyMemberRecord {
            family_id: family().family_id,
            code_unit_id: "unit:src/a.ts#module:0-1".to_string(),
            role: "member".to_string(),
        }
    }

    fn variation_slot() -> IndexedVariationSlotRecord {
        IndexedVariationSlotRecord {
            family_id: family().family_id,
            slot_id: "slot:handler".to_string(),
            description: "handler choice".to_string(),
        }
    }

    fn family_evidence() -> IndexedFamilyEvidenceRecord {
        IndexedFamilyEvidenceRecord {
            evidence_id: "evidence:family:routes:read:src/a.ts".to_string(),
            family_id: family().family_id,
            code_unit_id: "unit:src/a.ts#module:0-1".to_string(),
            covered_claims: vec!["canonical".to_string(), "support".to_string()],
            path: "src/a.ts".to_string(),
            content_hash: file("src/a.ts").content_hash,
            start_byte: 0,
            end_byte: 1,
            note: "same framework role and shape".to_string(),
        }
    }

    #[test]
    fn generation_use_cases_delegate_through_storage_port() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        record_indexed_file(&store, &generation, &file("src/a.ts")).expect("record file");
        remove_indexed_file(&store, &generation, "src/removed.ts").expect("remove file");
        record_code_unit(
            &store,
            &generation,
            &IndexedCodeUnitRecord {
                id: "unit:src/a.ts#module:0-1".to_string(),
                path: "src/a.ts".to_string(),
                language: "typescript".to_string(),
                kind: "module".to_string(),
                start_byte: 0,
                end_byte: 1,
                content_hash: file("src/a.ts").content_hash,
            },
        )
        .expect("record unit");
        record_ir_node(&store, &generation, &ir_node()).expect("record IR node");
        record_ir_edge(&store, &generation, &ir_edge()).expect("record IR edge");
        record_semantic_fact(&store, &generation, &semantic_fact()).expect("record semantic fact");
        record_family(&store, &generation, &family()).expect("record family");
        record_family_member(&store, &generation, &family_member()).expect("record family member");
        record_variation_slot(&store, &generation, &variation_slot())
            .expect("record variation slot");
        record_family_evidence(&store, &generation, &family_evidence())
            .expect("record family evidence");
        validate_index_generation(&store, &generation).expect("validate generation");
        activate_index_generation(&store, &generation).expect("activate generation");
        let inspection = inspect_index_storage(&store).expect("inspect storage");
        let active_families = list_active_families(&store).expect("list families");
        let active_family = show_family(&store, &family().family_id)
            .expect("show family")
            .expect("family exists");

        assert_eq!(generation.generation_id, "gen-000001");
        assert_eq!(inspection.schema_version, Some(STORAGE_SCHEMA_VERSION));
        assert_eq!(active_families.families, vec![family()]);
        assert_eq!(active_family.members, vec![family_member()]);
    }

    #[test]
    fn family_search_summaries_delegate_through_storage_port() {
        let store = FakeStore;
        let summaries =
            list_active_family_search_summaries(&store).expect("list family search summaries");

        assert_eq!(summaries.generation_id, "gen-000001");
        assert_eq!(summaries.families.len(), 1);
        let summary = &summaries.families[0];
        assert_eq!(summary.family_id, family().family_id);
        assert_eq!(summary.language, "typescript");
        assert_eq!(summary.framework_role, family_member().role);
        assert_eq!(summary.support, 1);
        assert_eq!(
            summary.evidence_path_components,
            vec!["a.ts".to_string(), "src".to_string()]
        );
    }

    #[test]
    fn indexed_file_validation_rejects_empty_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");

        let error =
            record_indexed_file(&store, &generation, &file(" ")).expect_err("empty path must fail");
        assert!(error.to_string().contains("path"));

        let mut missing_language = file("src/a.ts");
        missing_language.language = " ".to_string();
        let error = record_indexed_file(&store, &generation, &missing_language)
            .expect_err("empty language must fail");
        assert!(error.to_string().contains("language"));

        let error = remove_indexed_file(&store, &generation, "../escape.ts")
            .expect_err("unsafe removal path must fail");
        assert!(error.to_string().contains("path"));
    }

    #[test]
    fn code_unit_validation_rejects_invalid_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        let hash = file("src/a.ts").content_hash;
        let mut unit = IndexedCodeUnitRecord {
            id: "unit:src/a.ts#module:0-1".to_string(),
            path: "src/a.ts".to_string(),
            language: "typescript".to_string(),
            kind: "module".to_string(),
            start_byte: 0,
            end_byte: 1,
            content_hash: hash,
        };

        let mut missing_id = unit.clone();
        missing_id.id = " ".to_string();
        assert!(record_code_unit(&store, &generation, &missing_id)
            .expect_err("missing id")
            .to_string()
            .contains("id"));

        let mut absolute_path = unit.clone();
        absolute_path.path = "/tmp/a.ts".to_string();
        assert!(record_code_unit(&store, &generation, &absolute_path)
            .expect_err("absolute path")
            .to_string()
            .contains("repository-relative"));

        let mut windows_absolute = unit.clone();
        windows_absolute.path = "C:\\tmp\\a.ts".to_string();
        assert!(record_code_unit(&store, &generation, &windows_absolute)
            .expect_err("windows absolute path")
            .to_string()
            .contains("repository-relative"));

        let mut traversal = unit.clone();
        traversal.path = "../a.ts".to_string();
        assert!(record_code_unit(&store, &generation, &traversal)
            .expect_err("traversal path")
            .to_string()
            .contains("outside repository"));

        let mut missing_language = unit.clone();
        missing_language.language = " ".to_string();
        assert!(record_code_unit(&store, &generation, &missing_language)
            .expect_err("missing language")
            .to_string()
            .contains("language"));

        let mut missing_kind = unit.clone();
        missing_kind.kind = " ".to_string();
        assert!(record_code_unit(&store, &generation, &missing_kind)
            .expect_err("missing kind")
            .to_string()
            .contains("kind"));

        unit.start_byte = 2;
        unit.end_byte = 1;
        assert!(record_code_unit(&store, &generation, &unit)
            .expect_err("reversed range")
            .to_string()
            .contains("range"));
    }

    #[test]
    fn ir_validation_rejects_invalid_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        let node = ir_node();

        let mut missing_id = node.clone();
        missing_id.id = " ".to_string();
        assert!(record_ir_node(&store, &generation, &missing_id)
            .expect_err("missing node id")
            .to_string()
            .contains("id"));

        let mut mismatched_id = node.clone();
        mismatched_id.id = "ir:unit:src/other.ts#module:0-1".to_string();
        assert!(record_ir_node(&store, &generation, &mismatched_id)
            .expect_err("mismatched node id")
            .to_string()
            .contains("derived"));

        let mut invalid_kind = node.clone();
        invalid_kind.kind = "tree_sitter_node".to_string();
        assert!(record_ir_node(&store, &generation, &invalid_kind)
            .expect_err("invalid kind")
            .to_string()
            .contains("unsupported IR node kind"));

        let mut non_empty_payload = node;
        non_empty_payload.payload_json = r#"{"snippet":"const x = 1;"}"#.to_string();
        assert!(record_ir_node(&store, &generation, &non_empty_payload)
            .expect_err("non-empty payload")
            .to_string()
            .contains("empty JSON object"));

        let edge = ir_edge();
        let mut self_edge = edge.clone();
        self_edge.to_node_id = self_edge.from_node_id.clone();
        assert!(record_ir_edge(&store, &generation, &self_edge)
            .expect_err("self edge")
            .to_string()
            .contains("itself"));

        let mut invalid_label = edge;
        invalid_label.label = "calls".to_string();
        assert!(record_ir_edge(&store, &generation, &invalid_label)
            .expect_err("invalid edge label")
            .to_string()
            .contains("unsupported IR edge label"));
    }

    #[test]
    fn semantic_fact_validation_rejects_invalid_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        let fact = semantic_fact();

        let mut missing_id = fact.clone();
        missing_id.fact_id = " ".to_string();
        assert!(record_semantic_fact(&store, &generation, &missing_id)
            .expect_err("missing id")
            .to_string()
            .contains("id"));

        let mut blank_target = fact.clone();
        blank_target.target = Some(" ".to_string());
        assert!(record_semantic_fact(&store, &generation, &blank_target)
            .expect_err("blank target")
            .to_string()
            .contains("target"));

        let mut absolute_path = fact.clone();
        absolute_path.path = "/tmp/a.ts".to_string();
        assert!(record_semantic_fact(&store, &generation, &absolute_path)
            .expect_err("absolute path")
            .to_string()
            .contains("repository-relative"));

        let mut traversal = fact.clone();
        traversal.path = "../a.ts".to_string();
        assert!(record_semantic_fact(&store, &generation, &traversal)
            .expect_err("traversal path")
            .to_string()
            .contains("outside repository"));

        let mut missing_origin = fact.clone();
        missing_origin.origin_engine = " ".to_string();
        assert!(record_semantic_fact(&store, &generation, &missing_origin)
            .expect_err("missing origin")
            .to_string()
            .contains("origin"));

        let mut invalid_kind = fact.clone();
        invalid_kind.kind = "CALL".to_string();
        assert!(record_semantic_fact(&store, &generation, &invalid_kind)
            .expect_err("invalid kind")
            .to_string()
            .contains("unsupported semantic fact kind"));

        let mut invalid_certainty = fact.clone();
        invalid_certainty.certainty = "LOW_CONFIDENCE".to_string();
        assert!(
            record_semantic_fact(&store, &generation, &invalid_certainty)
                .expect_err("invalid certainty")
                .to_string()
                .contains("unsupported fact certainty")
        );

        let mut leaky_target = fact.clone();
        leaky_target.target = Some("file:///tmp/secret".to_string());
        assert!(record_semantic_fact(&store, &generation, &leaky_target)
            .expect_err("leaky target")
            .to_string()
            .contains("unsupported content"));

        let mut leaky_assumption = fact.clone();
        leaky_assumption.assumptions = vec!["read /tmp/secret".to_string()];
        assert!(record_semantic_fact(&store, &generation, &leaky_assumption)
            .expect_err("leaky assumption")
            .to_string()
            .contains("unsupported content"));

        let mut source_like_note = fact.clone();
        source_like_note.note = "const secret = true;".to_string();
        assert!(record_semantic_fact(&store, &generation, &source_like_note)
            .expect_err("source-like note")
            .to_string()
            .contains("unsupported content"));

        let mut reversed = fact.clone();
        reversed.start_byte = 2;
        reversed.end_byte = 1;
        assert!(record_semantic_fact(&store, &generation, &reversed)
            .expect_err("reversed range")
            .to_string()
            .contains("range"));
    }

    #[test]
    fn semantic_fact_validation_accepts_null_target_and_empty_assumptions() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        let mut fact = semantic_fact();
        fact.target = None;
        fact.assumptions = Vec::new();

        record_semantic_fact(&store, &generation, &fact)
            .expect("null target and empty assumptions remain valid");
    }

    #[test]
    fn family_validation_rejects_invalid_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");

        let mut missing_id = family();
        missing_id.family_id = " ".to_string();
        assert!(record_family(&store, &generation, &missing_id)
            .expect_err("missing family id")
            .to_string()
            .contains("family id"));

        let mut invalid_classification = family();
        invalid_classification.classification = "SIMILAR".to_string();
        assert!(record_family(&store, &generation, &invalid_classification)
            .expect_err("invalid classification")
            .to_string()
            .contains("classification"));

        let mut leaky_member = family_member();
        leaky_member.role = "see /tmp/secret".to_string();
        assert!(record_family_member(&store, &generation, &leaky_member)
            .expect_err("leaky role")
            .to_string()
            .contains("unsupported content"));

        let mut source_like_slot = variation_slot();
        source_like_slot.description = "const handler = route;".to_string();
        assert!(
            record_variation_slot(&store, &generation, &source_like_slot)
                .expect_err("source-like variation")
                .to_string()
                .contains("unsupported content")
        );

        let mut absolute_path = family_evidence();
        absolute_path.path = "/tmp/a.ts".to_string();
        assert!(record_family_evidence(&store, &generation, &absolute_path)
            .expect_err("absolute path")
            .to_string()
            .contains("repository-relative"));

        let mut traversal = family_evidence();
        traversal.path = "../a.ts".to_string();
        assert!(record_family_evidence(&store, &generation, &traversal)
            .expect_err("traversal path")
            .to_string()
            .contains("outside repository"));

        let mut uri_note = family_evidence();
        uri_note.note = "file:///tmp/secret".to_string();
        assert!(record_family_evidence(&store, &generation, &uri_note)
            .expect_err("URI note")
            .to_string()
            .contains("unsupported content"));

        let mut empty_claims = family_evidence();
        empty_claims.covered_claims = Vec::new();
        assert!(record_family_evidence(&store, &generation, &empty_claims)
            .expect_err("empty covered claims")
            .to_string()
            .contains("covered claims"));

        let mut unsupported_claim = family_evidence();
        unsupported_claim.covered_claims = vec!["canonical".to_string(), "runtime".to_string()];
        assert!(
            record_family_evidence(&store, &generation, &unsupported_claim)
                .expect_err("unsupported covered claim")
                .to_string()
                .contains("unsupported")
        );

        let mut duplicate_claim = family_evidence();
        duplicate_claim.covered_claims = vec!["support".to_string(), "support".to_string()];
        assert!(
            record_family_evidence(&store, &generation, &duplicate_claim)
                .expect_err("duplicate covered claim")
                .to_string()
                .contains("unique")
        );

        let mut reversed = family_evidence();
        reversed.start_byte = 2;
        reversed.end_byte = 1;
        assert!(record_family_evidence(&store, &generation, &reversed)
            .expect_err("reversed range")
            .to_string()
            .contains("range"));
    }

    #[test]
    fn constraint_profile_use_cases_delegate_through_storage_port() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");

        record_family_constraint_profile(&store, &generation, &constraint_profile_record())
            .expect("record constraint profile");
        let hydrated = show_family_constraint_profile(&store, &family().family_id)
            .expect("show constraint profile")
            .expect("profile exists");
        assert_eq!(hydrated, constraint_profile());
        assert!(show_family_constraint_profile(&store, "family:missing")
            .expect("show missing")
            .is_none());
    }

    #[test]
    fn constraint_profile_validation_rejects_invalid_fields_before_store_call() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");

        let mut missing_id = constraint_profile_record();
        missing_id.family_id = " ".to_string();
        assert!(
            record_family_constraint_profile(&store, &generation, &missing_id)
                .expect_err("missing family id")
                .to_string()
                .contains("family id")
        );

        let mut leaky_value = constraint_profile_record();
        leaky_value.profile.required_equal_features[0].values = vec!["see /tmp/secret".to_string()];
        assert!(
            record_family_constraint_profile(&store, &generation, &leaky_value)
                .expect_err("leaky feature value")
                .to_string()
                .contains("unsupported content")
        );

        let mut not_observed_only = constraint_profile_record();
        not_observed_only.profile.allowed_variations[0].observed_only = false;
        assert!(
            record_family_constraint_profile(&store, &generation, &not_observed_only)
                .expect_err("non observed-only variation")
                .to_string()
                .contains("observed-only")
        );

        // A prohibited-presence blocker tampered into the required axis. Index 3
        // is an empty-valued characteristic, so only the axis check fires.
        let mut cross_axis = constraint_profile_record();
        cross_axis.profile.required_equal_features[3].semantics =
            crate::core::model::FeatureConstraintSemantics::ProhibitedPresence;
        assert!(
            record_family_constraint_profile(&store, &generation, &cross_axis)
                .expect_err("cross-axis semantics")
                .to_string()
                .contains("axis")
        );
    }
}
