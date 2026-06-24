//! Query use-case boundary for finding repository analogues.

use crate::core::model::PatternClassification;
use crate::error::RepoGrammarError;
use crate::ports::index_store::{
    IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedSemanticFactRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueQuery {
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueResult {
    pub classification: PatternClassification,
}

pub fn find_analogues(_query: AnalogueQuery) -> Result<Vec<AnalogueResult>, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("find_analogues"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFilesReport {
    pub active_generation: String,
    pub files: Vec<IndexedFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedCodeUnitsReport {
    pub active_generation: String,
    pub units: Vec<IndexedCodeUnitRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSemanticFactsReport {
    pub active_generation: String,
    pub facts: Vec<IndexedSemanticFactRecord>,
}

pub fn list_indexed_files(store: &impl IndexStore) -> Result<IndexedFilesReport, RepoGrammarError> {
    let snapshot = store
        .list_active_indexed_files()
        .map_err(index_store_error)?;
    Ok(IndexedFilesReport {
        active_generation: snapshot.generation_id,
        files: snapshot.files,
    })
}

pub fn list_code_units(
    store: &impl IndexStore,
) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
    let snapshot = store.list_active_code_units().map_err(index_store_error)?;
    Ok(IndexedCodeUnitsReport {
        active_generation: snapshot.generation_id,
        units: snapshot.units,
    })
}

pub fn list_semantic_facts(
    store: &impl IndexStore,
) -> Result<IndexedSemanticFactsReport, RepoGrammarError> {
    let snapshot = store
        .list_active_semantic_facts()
        .map_err(index_store_error)?;
    Ok(IndexedSemanticFactsReport {
        active_generation: snapshot.generation_id,
        facts: snapshot.facts,
    })
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
        ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph, ActiveSemanticFacts, GenerationHandle,
        IndexedIrEdgeRecord, IndexedIrNodeRecord, StorageInspection,
    };

    struct FakeStore;

    impl IndexStore for FakeStore {
        fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
            panic!("query read tests must not prepare generations")
        }

        fn record_indexed_file(
            &self,
            _generation: &GenerationHandle,
            _file: &IndexedFileRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write indexed files")
        }

        fn record_code_unit(
            &self,
            _generation: &GenerationHandle,
            _unit: &IndexedCodeUnitRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write code units")
        }

        fn record_ir_node(
            &self,
            _generation: &GenerationHandle,
            _node: &IndexedIrNodeRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write IR nodes")
        }

        fn record_ir_edge(
            &self,
            _generation: &GenerationHandle,
            _edge: &IndexedIrEdgeRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write IR edges")
        }

        fn record_semantic_fact(
            &self,
            _generation: &GenerationHandle,
            _fact: &IndexedSemanticFactRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write semantic facts")
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
                facts: vec![semantic_fact()],
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
            panic!("query read tests must not validate generations")
        }

        fn activate_generation(
            &self,
            _generation: &GenerationHandle,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not activate generations")
        }

        fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
            panic!("query read tests must not inspect storage")
        }
    }

    fn semantic_fact() -> IndexedSemanticFactRecord {
        IndexedSemanticFactRecord {
            fact_id: "semantic-fact:000000".to_string(),
            kind: "RESOLVED_IMPORT".to_string(),
            subject: "src/a.ts#import:express".to_string(),
            target: None,
            certainty: "SEMANTIC".to_string(),
            origin_engine: "typescript".to_string(),
            origin_engine_version: "6.0.0".to_string(),
            origin_method: "compiler_api".to_string(),
            assumptions: Vec::new(),
            evidence_id: "semantic-evidence:000000".to_string(),
            code_unit_id: "unit:src/a.ts#module:0-10".to_string(),
            path: "src/a.ts".to_string(),
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid content hash"),
            start_byte: 0,
            end_byte: 10,
            note: "compiler resolved import target".to_string(),
        }
    }

    #[test]
    fn list_semantic_facts_delegates_through_index_store() {
        let report = list_semantic_facts(&FakeStore).expect("list semantic facts");

        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.facts, vec![semantic_fact()]);
    }
}
