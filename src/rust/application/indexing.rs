//! Indexing use-case boundary.

use crate::error::RepoGrammarError;
use crate::ports::file_discovery::{
    FileDiscovery, FileDiscoveryError, FileDiscoveryReport, FileDiscoveryRequest,
    DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::index_store::{IndexStore, IndexedFileRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
}

impl IndexingRequest {
    pub fn new(repository_root: impl Into<String>) -> Self {
        Self {
            repository_root: repository_root.into(),
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingOutcome {
    pub indexed_units: usize,
    pub discovered_files: usize,
    pub skipped_paths: usize,
    pub active_generation: Option<String>,
    pub warnings: Vec<String>,
}

pub fn index_repository(_request: IndexingRequest) -> Result<IndexingOutcome, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("index"))
}

pub fn discover_repository_files(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
) -> Result<FileDiscoveryReport, RepoGrammarError> {
    discovery
        .discover(FileDiscoveryRequest {
            repository_root: request.repository_root,
            max_file_bytes: request.max_file_bytes,
        })
        .map_err(discovery_error)
}

pub fn index_repository_with_discovery(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let report = discover_repository_files(request, discovery)?;
    Ok(IndexingOutcome {
        indexed_units: 0,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: None,
        warnings: report.warnings,
    })
}

pub fn index_repository_with_discovery_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let report = discover_repository_files(request, discovery)?;
    let generation = crate::application::storage::prepare_index_generation(store)?;
    for file in &report.files {
        crate::application::storage::record_indexed_file(
            store,
            &generation,
            &IndexedFileRecord {
                path: file.path.clone(),
                content_hash: file.content_hash.clone(),
                size_bytes: file.size_bytes,
                language: file.language.as_str().to_string(),
            },
        )?;
    }
    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexed_units: 0,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        warnings: report.warnings,
    })
}

fn discovery_error(error: FileDiscoveryError) -> RepoGrammarError {
    match error {
        FileDiscoveryError::InvalidRoot(message) | FileDiscoveryError::Unavailable(message) => {
            RepoGrammarError::InvalidInput(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::ports::index_store::{
        GenerationHandle, IndexStore, IndexStoreError, IndexedFileRecord, StorageInspection,
        STORAGE_SCHEMA_VERSION,
    };
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use std::fs;

    #[test]
    fn discovery_use_case_returns_files_without_claiming_indexed_units() {
        let workspace = TempWorkspace::new("indexing-discovery");
        fs::write(
            workspace.path().join("handler.ts"),
            "export const handler = () => 1;\n",
        )
        .expect("write source");

        let report = discover_repository_files(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("discover files");

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, "handler.ts");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::NotRepository);

        let outcome = index_repository_with_discovery(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("scan repository");
        assert_eq!(outcome.discovered_files, 1);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.active_generation, None);
    }

    #[test]
    fn discovery_use_case_rejects_invalid_roots() {
        let error = discover_repository_files(IndexingRequest::new(""), &FilesystemFileDiscovery)
            .expect_err("empty root must fail");

        assert!(error.to_string().contains("repository root"));
    }

    #[test]
    fn discovery_output_is_stored_in_active_sqlite_generation_without_code_units() {
        let workspace = TempWorkspace::new("indexing-store");
        let sentinel = "UNIQUE_SOURCE_SENTINEL_DO_NOT_STORE";
        fs::write(workspace.path().join("b.ts"), "export const b = 1;\n").expect("write b");
        fs::write(
            workspace.path().join("a.ts"),
            format!("export const a = '{sentinel}';\n"),
        )
        .expect("write a");
        fs::create_dir(workspace.path().join("node_modules")).expect("create node_modules");
        fs::write(
            workspace.path().join("node_modules/ignored.ts"),
            "ignored\n",
        )
        .expect("write ignored");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &store,
        )
        .expect("index file manifest");

        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.discovered_files, 2);
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        let inspection = store.inspect().expect("inspect storage");
        assert_eq!(inspection.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(inspection.schema_version, Some(STORAGE_SCHEMA_VERSION));

        let database = state
            .join("generations")
            .join("gen-000001")
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let rows = connection
            .prepare(
                "SELECT path, content_hash, size_bytes, language FROM indexed_files ORDER BY rowid",
            )
            .expect("prepare query")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .expect("query indexed files")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect indexed files");
        assert_eq!(rows[0].0, "a.ts");
        assert_eq!(rows[1].0, "b.ts");
        assert!(rows
            .iter()
            .all(|(path, hash, size, language)| path != sentinel
                && !path.contains(workspace.path().to_string_lossy().as_ref())
                && !hash.contains(sentinel)
                && *size >= 0
                && matches!(language.as_str(), "typescript" | "javascript")));
        let code_units: u32 = connection
            .query_row("SELECT count(*) FROM code_units", [], |row| row.get(0))
            .expect("count code units");
        assert_eq!(code_units, 0);
    }

    #[test]
    fn failed_file_recording_does_not_activate_generation() {
        struct FailingStore;

        impl IndexStore for FailingStore {
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
                Err(IndexStoreError::InvalidRecord(
                    "record rejected".to_string(),
                ))
            }

            fn validate_generation(
                &self,
                _generation: &GenerationHandle,
            ) -> Result<(), IndexStoreError> {
                panic!("validation must not run after record failure")
            }

            fn activate_generation(
                &self,
                _generation: &GenerationHandle,
            ) -> Result<(), IndexStoreError> {
                panic!("activation must not run after record failure")
            }

            fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
                unreachable!("not used")
            }
        }

        let workspace = TempWorkspace::new("indexing-store-fail");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");

        let error = index_repository_with_discovery_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FailingStore,
        )
        .expect_err("record failure must abort indexing");

        assert!(error.to_string().contains("record rejected"));
    }

    #[test]
    fn failed_generation_validation_preserves_previous_active_generation() {
        use std::cell::RefCell;

        struct ValidationFailingStore {
            active_generation: RefCell<String>,
            recorded_generations: RefCell<Vec<String>>,
        }

        impl IndexStore for ValidationFailingStore {
            fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
                Ok(GenerationHandle {
                    generation_id: "gen-000002".to_string(),
                })
            }

            fn record_indexed_file(
                &self,
                generation: &GenerationHandle,
                _file: &IndexedFileRecord,
            ) -> Result<(), IndexStoreError> {
                self.recorded_generations
                    .borrow_mut()
                    .push(generation.generation_id.clone());
                Ok(())
            }

            fn validate_generation(
                &self,
                generation: &GenerationHandle,
            ) -> Result<(), IndexStoreError> {
                assert_eq!(generation.generation_id, "gen-000002");
                Err(IndexStoreError::InvalidState(
                    "validation rejected generation".to_string(),
                ))
            }

            fn activate_generation(
                &self,
                _generation: &GenerationHandle,
            ) -> Result<(), IndexStoreError> {
                panic!("activation must not run after validation failure")
            }

            fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
                Ok(StorageInspection {
                    active_generation: Some(self.active_generation.borrow().clone()),
                    schema_version: Some(STORAGE_SCHEMA_VERSION),
                    journal_mode: Some("wal".to_string()),
                    foreign_keys_enabled: Some(true),
                    busy_timeout_ms: Some(5_000),
                    temp_store: Some("memory".to_string()),
                    integrity_check: Some("ok".to_string()),
                })
            }
        }

        let workspace = TempWorkspace::new("indexing-validation-fail");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        let store = ValidationFailingStore {
            active_generation: RefCell::new("gen-000001".to_string()),
            recorded_generations: RefCell::new(Vec::new()),
        };

        let error = index_repository_with_discovery_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &store,
        )
        .expect_err("validation failure must abort indexing");

        assert!(error.to_string().contains("validation rejected"));
        assert_eq!(
            store.inspect().expect("inspect fake").active_generation,
            Some("gen-000001".to_string())
        );
        assert_eq!(
            store.recorded_generations.borrow().as_slice(),
            ["gen-000002"]
        );
    }
}
