//! Repository-local index storage port.
//!
//! Storage adapters own SQL, database handles, and filesystem details. This
//! port exposes only RepoGrammar-owned values.

use crate::core::model::ContentHash;

pub const STORAGE_SCHEMA_VERSION: u32 = 9;

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
pub struct RepoShapeLanguageStats {
    pub language: String,
    pub indexed_file_count: usize,
    pub indexed_code_unit_count: usize,
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_member_count: usize,
    pub covered_code_units: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRepoShapeStats {
    pub generation_id: String,
    pub indexed_file_count: usize,
    pub indexed_code_unit_count: usize,
    pub semantic_fact_count: usize,
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_member_count: usize,
    pub covered_code_units: usize,
    pub by_language: Vec<RepoShapeLanguageStats>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStorageLayout {
    Empty,
    Mutable,
    Legacy,
    MutableWithLegacy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageInspection {
    pub layout: IndexStorageLayout,
    pub mutable_database_present: bool,
    pub legacy_generation_layout_present: bool,
    pub wal_bytes: Option<u64>,
    pub shm_bytes: Option<u64>,
    pub active_generation: Option<String>,
    pub schema_version: Option<u32>,
    pub code_unit_count: Option<u64>,
    pub dependency_record_count: Option<u64>,
    pub dirty_record_count: Option<u64>,
    pub journal_mode: Option<String>,
    pub foreign_keys_enabled: Option<bool>,
    pub busy_timeout_ms: Option<u32>,
    pub temp_store: Option<String>,
    pub integrity_check: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationPruneRequest {
    pub keep_inactive: usize,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationPruneReport {
    pub active_generation: String,
    pub keep_inactive: usize,
    pub retained_inactive_generations: Vec<String>,
    pub candidate_generations: Vec<String>,
    pub deleted_generations: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexCompactRequest {
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStorageSizeReport {
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexCompactReport {
    pub active_generation: String,
    pub dry_run: bool,
    pub before: IndexStorageSizeReport,
    pub after: IndexStorageSizeReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageCleanRequest {
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyLayoutCleanupReport {
    pub present_before: bool,
    pub present_after: bool,
    pub removed: bool,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCleanReport {
    pub active_generation: String,
    pub dry_run: bool,
    pub legacy_layout: LegacyLayoutCleanupReport,
    pub prune: GenerationPruneReport,
    pub compact: IndexCompactReport,
    pub total_bytes_before: u64,
    pub total_bytes_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexStoreError {
    Unavailable(String),
    InvalidState(String),
    InvalidRecord(String),
    /// The stored index was written by an older storage schema version than this
    /// build understands. Read paths return this typed error; the recovery is a
    /// full rebuild via `repogrammar resync`.
    SchemaVersionOutdated(String),
}

pub trait IndexStore {
    fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError>;

    fn record_indexed_file(
        &self,
        generation: &GenerationHandle,
        file: &IndexedFileRecord,
    ) -> Result<(), IndexStoreError>;

    fn remove_indexed_file(
        &self,
        generation: &GenerationHandle,
        path: &str,
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

    fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError>;

    fn validate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn activate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn inspect(&self) -> Result<StorageInspection, IndexStoreError>;
}

pub trait GenerationRetentionStore {
    fn prune_generations(
        &self,
        request: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, IndexStoreError>;
}

pub trait IndexMaintenanceStore {
    fn compact_storage(
        &self,
        request: IndexCompactRequest,
    ) -> Result<IndexCompactReport, IndexStoreError>;
}

pub trait IndexStorageCleanStore {
    fn clean_storage(
        &self,
        request: StorageCleanRequest,
    ) -> Result<StorageCleanReport, IndexStoreError>;
}
