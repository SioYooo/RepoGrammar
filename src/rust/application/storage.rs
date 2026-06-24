//! Storage use-case boundary.

use crate::core::model::{FactCertainty, IrEdgeLabel, IrNodeKind, SemanticFactKind};
use crate::error::RepoGrammarError;
use crate::ports::index_store::{
    GenerationHandle, IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedIrEdgeRecord, IndexedIrNodeRecord, IndexedSemanticFactRecord, StorageInspection,
};
use std::path::{Component, Path};

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
    value
        .split_whitespace()
        .any(|token| Path::new(token).is_absolute() || looks_like_windows_absolute_path(token))
}

fn looks_like_source_snippet(value: &str) -> bool {
    value.contains("=>")
        || (value.contains('=') && value.contains(';'))
        || value.contains('{')
        || value.contains('}')
}

fn validate_repo_relative_path(path: &str) -> Result<(), RepoGrammarError> {
    if Path::new(path).is_absolute() {
        return Err(RepoGrammarError::InvalidInput(
            "path must be repository-relative".to_string(),
        ));
    }
    if path.contains('\\') || looks_like_windows_absolute_path(path) {
        return Err(RepoGrammarError::InvalidInput(
            "path must be repository-relative".to_string(),
        ));
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::Prefix(_)
            | Component::RootDir => {
                return Err(RepoGrammarError::InvalidInput(
                    "path must not traverse outside repository".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn index_store_error(error: IndexStoreError) -> RepoGrammarError {
    match error {
        IndexStoreError::Unavailable(message)
        | IndexStoreError::InvalidState(message)
        | IndexStoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::ports::index_store::{
        ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph, ActiveSemanticFacts,
        STORAGE_SCHEMA_VERSION,
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
                active_generation: Some("gen-000001".to_string()),
                schema_version: Some(STORAGE_SCHEMA_VERSION),
                code_unit_count: Some(0),
                journal_mode: Some("wal".to_string()),
                foreign_keys_enabled: Some(true),
                busy_timeout_ms: Some(5_000),
                temp_store: Some("memory".to_string()),
                integrity_check: Some("ok".to_string()),
            })
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

    #[test]
    fn generation_use_cases_delegate_through_storage_port() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        record_indexed_file(&store, &generation, &file("src/a.ts")).expect("record file");
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
        validate_index_generation(&store, &generation).expect("validate generation");
        activate_index_generation(&store, &generation).expect("activate generation");
        let inspection = inspect_index_storage(&store).expect("inspect storage");

        assert_eq!(generation.generation_id, "gen-000001");
        assert_eq!(inspection.schema_version, Some(STORAGE_SCHEMA_VERSION));
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
}
