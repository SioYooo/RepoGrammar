//! Storage use-case boundary.

use crate::error::RepoGrammarError;
use crate::ports::index_store::{
    GenerationHandle, IndexStore, IndexStoreError, IndexedFileRecord, StorageInspection,
};

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
    if file.language.trim().is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "indexed file language must not be empty".to_string(),
        ));
    }
    Ok(())
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
    use crate::ports::index_store::STORAGE_SCHEMA_VERSION;

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

    #[test]
    fn generation_use_cases_delegate_through_storage_port() {
        let store = FakeStore;
        let generation = prepare_index_generation(&store).expect("prepare generation");
        record_indexed_file(&store, &generation, &file("src/a.ts")).expect("record file");
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
}
