//! Query use-case boundary for finding repository analogues.

use crate::core::model::PatternClassification;
use crate::error::RepoGrammarError;
use crate::ports::index_store::{
    IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
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

fn index_store_error(error: IndexStoreError) -> RepoGrammarError {
    match error {
        IndexStoreError::Unavailable(message)
        | IndexStoreError::InvalidState(message)
        | IndexStoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}
