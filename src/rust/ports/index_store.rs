//! Repository-local index storage port.
//!
//! Storage adapters own SQL, database handles, and filesystem details. This
//! port exposes only RepoGrammar-owned values.

use crate::core::model::ContentHash;

pub const STORAGE_SCHEMA_VERSION: u32 = 1;

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
pub struct StorageInspection {
    pub active_generation: Option<String>,
    pub schema_version: Option<u32>,
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

    fn validate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn activate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError>;

    fn inspect(&self) -> Result<StorageInspection, IndexStoreError>;
}
