//! Repository-local index storage port.
//!
//! Storage adapters own SQL, database handles, and filesystem details. This
//! port exposes only RepoGrammar-owned values.

use crate::core::model::ContentHash;

pub const STORAGE_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationHandle {
    pub generation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFileRecord {
    pub path: String,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedCodeUnitRecord {
    pub id: String,
    pub path: String,
    pub language: String,
    pub kind: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSemanticFactRecord {
    pub fact_id: String,
    pub kind: String,
    pub subject: String,
    pub target: Option<String>,
    pub certainty: String,
    pub origin_engine: String,
    pub origin_engine_version: String,
    pub origin_method: String,
    pub assumptions: Vec<String>,
    pub evidence_id: String,
    pub code_unit_id: String,
    pub path: String,
    pub content_hash: ContentHash,
    pub start_byte: usize,
    pub end_byte: usize,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedIrNodeRecord {
    pub id: String,
    pub code_unit_id: String,
    pub kind: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedIrEdgeRecord {
    pub from_node_id: String,
    pub to_node_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveIndexedFiles {
    pub generation_id: String,
    pub files: Vec<IndexedFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCodeUnits {
    pub generation_id: String,
    pub units: Vec<IndexedCodeUnitRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSemanticFacts {
    pub generation_id: String,
    pub facts: Vec<IndexedSemanticFactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveIrGraph {
    pub generation_id: String,
    pub nodes: Vec<IndexedIrNodeRecord>,
    pub edges: Vec<IndexedIrEdgeRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveClaimInputSnapshot {
    pub generation_id: String,
    pub files: Vec<IndexedFileRecord>,
    pub units: Vec<IndexedCodeUnitRecord>,
    pub ir_nodes: Vec<IndexedIrNodeRecord>,
    pub ir_edges: Vec<IndexedIrEdgeRecord>,
    pub semantic_facts: Vec<IndexedSemanticFactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageInspection {
    pub active_generation: Option<String>,
    pub schema_version: Option<u32>,
    pub code_unit_count: Option<u64>,
    pub journal_mode: Option<String>,
    pub foreign_keys_enabled: Option<bool>,
    pub busy_timeout_ms: Option<u32>,
    pub temp_store: Option<String>,
    pub integrity_check: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexStoreError {
    Unavailable(String),
    InvalidState(String),
    InvalidRecord(String),
}

pub trait IndexStore {
    fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError>;

    fn record_indexed_file(
        &self,
        generation: &GenerationHandle,
        file: &IndexedFileRecord,
    ) -> Result<(), IndexStoreError>;

    fn record_code_unit(
        &self,
        generation: &GenerationHandle,
        unit: &IndexedCodeUnitRecord,
    ) -> Result<(), IndexStoreError>;

    fn record_ir_node(
        &self,
        generation: &GenerationHandle,
        node: &IndexedIrNodeRecord,
    ) -> Result<(), IndexStoreError>;

    fn record_ir_edge(
        &self,
        generation: &GenerationHandle,
        edge: &IndexedIrEdgeRecord,
    ) -> Result<(), IndexStoreError>;

    fn record_semantic_fact(
        &self,
        generation: &GenerationHandle,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError>;

    fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError>;

    fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError>;

    fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError>;

    fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError>;

    fn load_active_claim_input_snapshot(&self)
        -> Result<ActiveClaimInputSnapshot, IndexStoreError>;

    fn validate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn activate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn inspect(&self) -> Result<StorageInspection, IndexStoreError>;
}
