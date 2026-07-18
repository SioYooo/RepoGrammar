//! SQLite persistence adapter.
//!
//! SQL, migrations, PRAGMAs, and generation filesystem layout stay in this
//! adapter. Application code talks to it through storage ports.

use crate::core::model::{
    ContentHash, FactCertainty, FamilyConstraintProfile, FamilyPrevalence, FamilyPrevalenceClass,
    FeatureConstraint, FeatureConstraintOrigin, FeatureConstraintSemantics, IrEdgeLabel,
    IrNodeKind, SemanticFactKind, TypedUnknown, UnknownClass, UnknownObligation, UnknownReasonCode,
    VariationConstraint,
};
use crate::core::policy::paths::{looks_like_windows_absolute_path, RepoRelativePathError};
use crate::ports::family_store::{
    family_evidence_covered_claim_is_supported, ActiveFamilies, ActiveFamily,
    ActiveFamilyCandidates, ActiveFamilyEvidenceProjection, ActiveFamilySearchSummaries,
    ActiveFamilySummaries, FamilyConstraintProfileStore, FamilyStore, GenerationWriteSession,
    GenerationWriteStore, IndexedFamilyCandidateRecord, IndexedFamilyConstraintProfileRecord,
    IndexedFamilyEvidenceProjectionRecord, IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord,
    IndexedFamilyRecord, IndexedFamilySearchSummaryRecord, IndexedFamilySummaryRecord,
    IndexedVariationSlotRecord, StoreError, WriteSessionStats, FAMILY_SEARCH_PATH_COMPONENT_CAP,
};
use crate::ports::index_store::{
    ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
    ActiveRepoShapeStats, ActiveSemanticFacts, GenerationEngineStampStore, GenerationHandle,
    GenerationPruneReport, GenerationPruneRequest, GenerationRetentionStore, IndexCompactReport,
    IndexCompactRequest, IndexMaintenanceStore, IndexStorageCleanStore, IndexStorageLayout,
    IndexStorageSizeReport, IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedIrEdgeRecord, IndexedIrNodeRecord, IndexedSemanticFactRecord, LegacyLayoutCleanupReport,
    RepoShapeLanguageStats, StorageCleanReport, StorageCleanRequest, StorageInspection,
    STORAGE_SCHEMA_VERSION,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DATABASE_FILE: &str = "repogrammar.sqlite";
const CURRENT_GENERATION_FILE: &str = "current-generation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingDatabase {
    Allowed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenerationDirectoryEntry {
    generation_id: String,
    number: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteIndexLocation {
    pub path: String,
}

/// Test-only aggregate write instrumentation, shared between a store and every
/// write session it opens. It measures the real number of write-session
/// connection opens, committed transactions, and phase checkpoints across a run,
/// so benchmarks and pipeline tests report measured figures rather than values
/// asserted by construction. Production builds compile none of this.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct WriteInstrumentation {
    pub connection_opens: std::sync::atomic::AtomicUsize,
    pub transactions: std::sync::atomic::AtomicUsize,
    pub checkpoints: std::sync::atomic::AtomicUsize,
}

#[derive(Debug, Clone)]
pub struct SqliteIndexStore {
    state_dir: PathBuf,
    #[cfg(test)]
    write_instrumentation: std::sync::Arc<WriteInstrumentation>,
}

impl SqliteIndexStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
            #[cfg(test)]
            write_instrumentation: std::sync::Arc::new(WriteInstrumentation::default()),
        }
    }

    /// A shared handle to this store's test-only write instrumentation.
    #[cfg(test)]
    pub(crate) fn write_instrumentation(&self) -> std::sync::Arc<WriteInstrumentation> {
        std::sync::Arc::clone(&self.write_instrumentation)
    }

    fn database_path(&self) -> PathBuf {
        self.state_dir.join(DATABASE_FILE)
    }

    fn generations_dir(&self) -> PathBuf {
        self.state_dir.join("generations")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.state_dir.join("tmp")
    }

    fn current_generation_path(&self) -> PathBuf {
        self.state_dir.join(CURRENT_GENERATION_FILE)
    }

    fn legacy_generation_layout_present(&self) -> Result<bool, IndexStoreError> {
        match fs::symlink_metadata(self.current_generation_path()) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(unavailable("failed to inspect current-generation pointer")),
        }
        match fs::symlink_metadata(self.generations_dir()) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(unavailable("failed to inspect generations directory")),
        }
    }

    fn mutable_sidecar_sizes(
        &self,
        mutable_database_present: bool,
    ) -> Result<(Option<u64>, Option<u64>), IndexStoreError> {
        if !mutable_database_present {
            return Ok((None, None));
        }
        let wal_bytes = optional_regular_file_size(
            &self.state_dir.join(format!("{DATABASE_FILE}-wal")),
            "repository index WAL sidecar",
        )?;
        let shm_bytes = optional_regular_file_size(
            &self.state_dir.join(format!("{DATABASE_FILE}-shm")),
            "repository index SHM sidecar",
        )?;
        Ok((Some(wal_bytes), Some(shm_bytes)))
    }

    fn generation_dir(&self, generation_id: &str) -> PathBuf {
        self.generations_dir().join(generation_id)
    }

    #[cfg(test)]
    fn mutable_database_path(&self) -> PathBuf {
        self.database_path()
    }

    fn ensure_layout(&self) -> Result<(), IndexStoreError> {
        ensure_real_dir(&self.state_dir, "state directory")?;
        ensure_real_dir(&self.tmp_dir(), "tmp directory")?;
        Ok(())
    }

    /// Recreate the repo-local mutable database when its stored schema version is
    /// older than the current build. Only the mutable SQLite file and its
    /// WAL/SHM sidecars under `.repogrammar` are removed — nothing else. Reads
    /// never call this; only the full-rebuild path does, so the existing active
    /// generation is untouched until a rebuild is actually requested.
    fn recreate_mutable_database_if_outdated(&self) -> Result<(), IndexStoreError> {
        let outdated = match self.try_open_mutable_database_read_only()? {
            Some(connection) => matches!(
                stored_schema_version(&connection)?,
                Some(version) if version < STORAGE_SCHEMA_VERSION
            ),
            None => false,
        };
        if outdated {
            self.remove_mutable_database_files()?;
        }
        Ok(())
    }

    fn remove_mutable_database_files(&self) -> Result<(), IndexStoreError> {
        for suffix in ["", "-wal", "-shm"] {
            let path = self.state_dir.join(format!("{DATABASE_FILE}{suffix}"));
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(unavailable(
                        "failed to remove outdated repository index database",
                    ))
                }
            }
        }
        Ok(())
    }

    fn next_generation_id(&self, connection: &Connection) -> Result<String, IndexStoreError> {
        let mut max_seen = 0u32;

        let mut statement = connection
            .prepare("SELECT generation_id FROM index_generations")
            .map_err(sql_unavailable)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_unavailable)?;
        for row in rows {
            let generation_id = row.map_err(sql_unavailable)?;
            if let Some(number) = parse_generation_number(&generation_id) {
                max_seen = max_seen.max(number);
            }
        }

        if self.generations_dir().exists() {
            for entry in fs::read_dir(self.generations_dir())
                .map_err(|_| unavailable("failed to read generations directory"))?
            {
                let entry = entry.map_err(|_| unavailable("failed to read generation entry"))?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if let Some(number) = parse_generation_number(name) {
                    max_seen = max_seen.max(number);
                }
            }
        }
        if max_seen >= 999_999 {
            return Err(IndexStoreError::InvalidState(
                "generation id space is exhausted".to_string(),
            ));
        }
        Ok(format!("gen-{:06}", max_seen + 1))
    }

    #[cfg(test)]
    fn open_generation(&self, generation_id: &str) -> Result<Connection, IndexStoreError> {
        validate_generation_id(generation_id)?;
        self.open_mutable_database(MissingDatabase::Allowed)
    }

    fn open_existing_generation(&self, generation_id: &str) -> Result<Connection, IndexStoreError> {
        validate_generation_id(generation_id)?;
        self.open_mutable_database(MissingDatabase::Rejected)
    }

    fn open_active_generation_read_only(&self) -> Result<(String, Connection), IndexStoreError> {
        self.require_existing_layout()?;
        if let Some(connection) = self.try_open_mutable_database_read_only()? {
            let generation_id = active_generation_id(&connection)?.ok_or_else(|| {
                IndexStoreError::InvalidState("active generation is missing".to_string())
            })?;
            validate_active_generation_for_read(&connection, &generation_id)?;
            return Ok((generation_id, connection));
        }
        self.open_legacy_active_generation_read_only()
    }

    fn open_active_generation_read_model(&self) -> Result<(String, Connection), IndexStoreError> {
        self.require_existing_layout()?;
        // Read-model queries (stats, family reads) are read-only. Use a
        // read-only connection and do not run migrations here: opening a
        // writable connection and calling apply_migrations on every read took
        // the WAL writer lock and wrote schema_migrations for a pure read.
        if let Some(connection) = self.try_open_mutable_database_read_only()? {
            let generation_id = active_generation_id(&connection)?.ok_or_else(|| {
                IndexStoreError::InvalidState("active generation is missing".to_string())
            })?;
            validate_active_generation_for_read_model(&connection, &generation_id)?;
            return Ok((generation_id, connection));
        }
        self.open_legacy_active_generation_read_only()
    }

    fn open_legacy_active_generation_read_only(
        &self,
    ) -> Result<(String, Connection), IndexStoreError> {
        let current = self.current_generation_path();
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(IndexStoreError::InvalidState(
                    "active generation is missing".to_string(),
                ));
            }
            Err(_) => return Err(unavailable("failed to inspect current-generation pointer")),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(IndexStoreError::InvalidState(
                "current-generation must be a regular file".to_string(),
            ));
        }
        let generation_id = fs::read_to_string(&current)
            .map_err(|_| unavailable("failed to read current-generation pointer"))?
            .trim()
            .to_string();
        validate_generation_id(&generation_id)?;
        let connection = self.open_legacy_existing_generation_read_only(&generation_id)?;
        if generation_status(&connection, &generation_id)?.as_deref() != Some("active") {
            return Err(IndexStoreError::InvalidState(
                "current-generation does not point at an active generation".to_string(),
            ));
        }
        validate_active_generation_for_read(&connection, &generation_id)?;
        Ok((generation_id, connection))
    }

    fn require_existing_layout(&self) -> Result<(), IndexStoreError> {
        ensure_existing_real_dir(&self.state_dir, "state directory")?;
        ensure_existing_real_dir(&self.tmp_dir(), "tmp directory")?;
        Ok(())
    }

    fn open_mutable_database(
        &self,
        missing_database: MissingDatabase,
    ) -> Result<Connection, IndexStoreError> {
        let path = self.database_path_for(missing_database)?;
        let connection = open_connection(path, missing_database)?;
        apply_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn try_open_mutable_database(&self) -> Result<Option<Connection>, IndexStoreError> {
        let path = match self.database_path_for_optional()? {
            Some(path) => path,
            None => return Ok(None),
        };
        let connection = open_connection(path, MissingDatabase::Rejected)?;
        apply_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(Some(connection))
    }

    fn try_open_mutable_database_read_only(&self) -> Result<Option<Connection>, IndexStoreError> {
        let path = match self.database_path_for_optional()? {
            Some(path) => path,
            None => return Ok(None),
        };
        let connection = open_read_only_connection(path)?;
        apply_read_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(Some(connection))
    }

    fn open_mutable_database_read_only(&self) -> Result<Connection, IndexStoreError> {
        let path = self.database_path_for(MissingDatabase::Rejected)?;
        let connection = open_read_only_connection(path)?;
        apply_read_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn compact_mutable_database(
        &self,
        request: IndexCompactRequest,
    ) -> Result<IndexCompactReport, IndexStoreError> {
        if request.dry_run {
            let connection = self.open_mutable_database_read_only()?;
            let active_generation = active_generation_id(&connection)?.ok_or_else(|| {
                IndexStoreError::InvalidState("active generation is missing".to_string())
            })?;
            validate_generation_for_read(&connection, &active_generation)?;
            let before = self.storage_size_report()?;
            return Ok(IndexCompactReport {
                active_generation,
                dry_run: true,
                before: before.clone(),
                after: before,
            });
        }

        let connection = self.open_mutable_database(MissingDatabase::Rejected)?;
        let active_generation = active_generation_id(&connection)?.ok_or_else(|| {
            IndexStoreError::InvalidState("active generation is missing".to_string())
        })?;
        validate_generation_for_read(&connection, &active_generation)?;
        let before = self.storage_size_report()?;
        let _ = run_compaction_maintenance(&connection)?;
        let after = self.storage_size_report()?;
        Ok(IndexCompactReport {
            active_generation,
            dry_run: false,
            before,
            after,
        })
    }

    fn storage_size_report(&self) -> Result<IndexStorageSizeReport, IndexStoreError> {
        let database_path = self.database_path_for(MissingDatabase::Rejected)?;
        let database_bytes =
            required_regular_file_size(&database_path, "repository index database")?;
        let wal_bytes = optional_regular_file_size(
            &self.state_dir.join(format!("{DATABASE_FILE}-wal")),
            "repository index WAL sidecar",
        )?;
        let shm_bytes = optional_regular_file_size(
            &self.state_dir.join(format!("{DATABASE_FILE}-shm")),
            "repository index SHM sidecar",
        )?;
        let total_bytes = checked_size_total(database_bytes, wal_bytes, shm_bytes)?;
        Ok(IndexStorageSizeReport {
            database_bytes,
            wal_bytes,
            shm_bytes,
            total_bytes,
        })
    }

    fn clean_mutable_storage(
        &self,
        request: StorageCleanRequest,
    ) -> Result<StorageCleanReport, IndexStoreError> {
        self.require_existing_layout()?;
        let inspection = self.inspect()?;
        if !inspection.mutable_database_present {
            return Err(IndexStoreError::InvalidState(
                "storage clean requires mutable SQLite storage; run repogrammar resync before cleaning legacy-only layout"
                    .to_string(),
            ));
        }

        let legacy_present_before = inspection.legacy_generation_layout_present;
        let legacy_bytes_before = self.legacy_layout_size()?;
        let compact_probe = self.compact_mutable_database(IndexCompactRequest { dry_run: true })?;
        let database_bytes_before = compact_probe.before.total_bytes;
        let total_bytes_before =
            checked_storage_clean_total(database_bytes_before, legacy_bytes_before)?;

        if request.dry_run {
            let prune = self.prune_mutable_generations(GenerationPruneRequest {
                keep_inactive: 0,
                dry_run: true,
            })?;
            return Ok(StorageCleanReport {
                active_generation: compact_probe.active_generation.clone(),
                dry_run: true,
                legacy_layout: LegacyLayoutCleanupReport {
                    present_before: legacy_present_before,
                    present_after: legacy_present_before,
                    removed: false,
                    bytes_before: legacy_bytes_before,
                    bytes_after: legacy_bytes_before,
                },
                prune,
                compact: compact_probe,
                total_bytes_before,
                total_bytes_after: total_bytes_before,
            });
        }

        let legacy_removed = self.remove_legacy_generation_layout()?;
        let prune = self.prune_mutable_generations(GenerationPruneRequest {
            keep_inactive: 0,
            dry_run: false,
        })?;
        if prune.active_generation != compact_probe.active_generation {
            return Err(IndexStoreError::InvalidState(
                "active generation changed during storage clean; retry repogrammar storage clean"
                    .to_string(),
            ));
        }
        let compact = self.compact_mutable_database(IndexCompactRequest { dry_run: false })?;
        let legacy_present_after = self.legacy_generation_layout_present()?;
        let legacy_bytes_after = self.legacy_layout_size()?;
        let total_bytes_after =
            checked_storage_clean_total(compact.after.total_bytes, legacy_bytes_after)?;

        Ok(StorageCleanReport {
            active_generation: compact.active_generation.clone(),
            dry_run: false,
            legacy_layout: LegacyLayoutCleanupReport {
                present_before: legacy_present_before,
                present_after: legacy_present_after,
                removed: legacy_removed,
                bytes_before: legacy_bytes_before,
                bytes_after: legacy_bytes_after,
            },
            prune,
            compact,
            total_bytes_before,
            total_bytes_after,
        })
    }

    fn legacy_layout_size(&self) -> Result<u64, IndexStoreError> {
        let current_generation_bytes =
            optional_legacy_path_size(&self.current_generation_path(), "current-generation")?;
        let generations_bytes =
            optional_legacy_path_size(&self.generations_dir(), "generations directory")?;
        checked_storage_clean_total(current_generation_bytes, generations_bytes)
    }

    fn remove_legacy_generation_layout(&self) -> Result<bool, IndexStoreError> {
        let mut removed = false;
        let current = self.current_generation_path();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(IndexStoreError::InvalidState(
                    "current-generation must not be a symlink".to_string(),
                ));
            }
            Ok(metadata) if metadata.is_file() => {
                fs::remove_file(&current)
                    .map_err(|_| unavailable("failed to remove current-generation pointer"))?;
                removed = true;
            }
            Ok(_) => {
                return Err(IndexStoreError::InvalidState(
                    "current-generation exists and is not a regular file".to_string(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(unavailable("failed to inspect current-generation pointer")),
        }

        let generations = self.generations_dir();
        match fs::symlink_metadata(&generations) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(IndexStoreError::InvalidState(
                    "generations directory must not be a symlink".to_string(),
                ));
            }
            Ok(metadata) if metadata.is_dir() => {
                fs::remove_dir_all(&generations)
                    .map_err(|_| unavailable("failed to remove generations directory"))?;
                removed = true;
            }
            Ok(_) => {
                return Err(IndexStoreError::InvalidState(
                    "generations exists and is not a directory".to_string(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(unavailable("failed to inspect generations directory")),
        }

        Ok(removed)
    }

    fn database_path_for(
        &self,
        missing_database: MissingDatabase,
    ) -> Result<PathBuf, IndexStoreError> {
        ensure_existing_real_dir(&self.state_dir, "state directory")?;
        let path = self.database_path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(IndexStoreError::InvalidState(
                    "repository index database must not be a symlink".to_string(),
                ))
            }
            Ok(metadata) if metadata.is_file() => canonical_database_path(&self.state_dir),
            Ok(_) => Err(IndexStoreError::InvalidState(
                "repository index database exists and is not a file".to_string(),
            )),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && missing_database == MissingDatabase::Allowed =>
            {
                canonical_database_path(&self.state_dir)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(
                IndexStoreError::InvalidState("repository index database is missing".to_string()),
            ),
            Err(_) => Err(unavailable("failed to inspect repository index database")),
        }
    }

    fn database_path_for_optional(&self) -> Result<Option<PathBuf>, IndexStoreError> {
        ensure_existing_real_dir(&self.state_dir, "state directory")?;
        let path = self.database_path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(IndexStoreError::InvalidState(
                    "repository index database must not be a symlink".to_string(),
                ))
            }
            Ok(metadata) if metadata.is_file() => {
                canonical_database_path(&self.state_dir).map(Some)
            }
            Ok(_) => Err(IndexStoreError::InvalidState(
                "repository index database exists and is not a file".to_string(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(unavailable("failed to inspect repository index database")),
        }
    }

    fn legacy_generation_database_path_for(
        &self,
        generation_id: &str,
        missing_database: MissingDatabase,
    ) -> Result<PathBuf, IndexStoreError> {
        validate_generation_id(generation_id)?;
        let generation_dir = self.generation_dir(generation_id);
        let path = generation_dir.join(DATABASE_FILE);
        ensure_existing_real_dir(&generation_dir, "generation directory")?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(IndexStoreError::InvalidState(
                    "generation database must not be a symlink".to_string(),
                ))
            }
            Ok(metadata) if metadata.is_file() => {
                canonical_generation_database_path(&generation_dir)
            }
            Ok(_) => Err(IndexStoreError::InvalidState(
                "generation database exists and is not a file".to_string(),
            )),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && missing_database == MissingDatabase::Allowed =>
            {
                canonical_generation_database_path(&generation_dir)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(
                IndexStoreError::InvalidState("generation database is missing".to_string()),
            ),
            Err(_) => Err(unavailable("failed to inspect generation database")),
        }
    }

    fn open_legacy_existing_generation_read_only(
        &self,
        generation_id: &str,
    ) -> Result<Connection, IndexStoreError> {
        let path =
            self.legacy_generation_database_path_for(generation_id, MissingDatabase::Rejected)?;
        let connection = open_read_only_connection(path)?;
        apply_read_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn list_generation_directories(
        &self,
    ) -> Result<Vec<GenerationDirectoryEntry>, IndexStoreError> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.generations_dir())
            .map_err(|_| unavailable("failed to read generations directory"))?
        {
            let entry = entry.map_err(|_| unavailable("failed to read generation entry"))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(number) = parse_generation_number(name) else {
                continue;
            };
            validate_generation_id(name)?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| unavailable("failed to inspect generation directory"))?;
            if metadata.file_type().is_symlink() {
                return Err(IndexStoreError::InvalidState(
                    "generation directory must not be a symlink".to_string(),
                ));
            }
            if !metadata.is_dir() {
                return Err(IndexStoreError::InvalidState(
                    "generation directory exists and is not a directory".to_string(),
                ));
            }
            entries.push(GenerationDirectoryEntry {
                generation_id: name.to_string(),
                number,
            });
        }
        entries.sort_by_key(|entry| entry.number);
        Ok(entries)
    }

    fn ensure_active_generation_unchanged(
        &self,
        expected_generation_id: &str,
    ) -> Result<(), IndexStoreError> {
        let (current_generation_id, connection) = self.open_active_generation_read_only()?;
        drop(connection);
        if current_generation_id != expected_generation_id {
            return Err(IndexStoreError::InvalidState(
                "active generation changed during prune; retry repogrammar prune".to_string(),
            ));
        }
        Ok(())
    }
}

fn canonical_generation_database_path(generation_dir: &Path) -> Result<PathBuf, IndexStoreError> {
    generation_dir
        .canonicalize()
        .map(|path| path.join(DATABASE_FILE))
        .map_err(|_| unavailable("failed to canonicalize generation directory"))
}

fn canonical_database_path(state_dir: &Path) -> Result<PathBuf, IndexStoreError> {
    state_dir
        .canonicalize()
        .map(|path| path.join(DATABASE_FILE))
        .map_err(|_| unavailable("failed to canonicalize state directory"))
}

impl IndexStore for SqliteIndexStore {
    fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
        self.ensure_layout()?;
        // A full rebuild against a pre-current schema cannot upgrade in place
        // (CREATE TABLE IF NOT EXISTS never adds columns), so recreate the
        // repo-local mutable database before building the new generation.
        self.recreate_mutable_database_if_outdated()?;
        let connection = self.open_mutable_database(MissingDatabase::Allowed)?;
        apply_migrations(&connection)?;
        let generation_id = self.next_generation_id(&connection)?;
        connection
            .execute(
                "INSERT INTO index_generations \
                 (generation_id, status, created_at, repogrammar_version) \
                 VALUES (?1, 'building', datetime('now'), ?2)",
                params![generation_id, env!("CARGO_PKG_VERSION")],
            )
            .map_err(sql_unavailable)?;

        Ok(GenerationHandle { generation_id })
    }

    fn record_indexed_file(
        &self,
        generation: &GenerationHandle,
        file: &IndexedFileRecord,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.record_indexed_file(file)?;
        session.seal(false)
    }

    fn remove_indexed_file(
        &self,
        generation: &GenerationHandle,
        path: &str,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.remove_indexed_file(path)?;
        session.seal(true)
    }

    fn record_code_unit(
        &self,
        generation: &GenerationHandle,
        unit: &IndexedCodeUnitRecord,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.record_code_unit(unit)?;
        session.seal(false)
    }

    fn record_ir_node(
        &self,
        generation: &GenerationHandle,
        node: &IndexedIrNodeRecord,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.record_ir_node(node)?;
        session.seal(false)
    }

    fn record_ir_edge(
        &self,
        generation: &GenerationHandle,
        edge: &IndexedIrEdgeRecord,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.record_ir_edge(edge)?;
        session.seal(false)
    }

    fn record_semantic_fact(
        &self,
        generation: &GenerationHandle,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError> {
        let mut session = SqliteGenerationWriteSession::open(self, generation)?;
        session.record_semantic_fact(fact)?;
        session.seal(false)
    }

    fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_only()?;
        let files = query_indexed_files(&connection, &generation_id)?;
        Ok(ActiveIndexedFiles {
            generation_id,
            files,
        })
    }

    fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_only()?;
        let units = query_code_units(&connection, &generation_id)?;
        Ok(ActiveCodeUnits {
            generation_id,
            units,
        })
    }

    fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_only()?;
        let facts = query_semantic_facts(&connection, &generation_id)?;
        Ok(ActiveSemanticFacts {
            generation_id,
            facts,
        })
    }

    fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_only()?;
        let (nodes, edges) = query_ir_graph(&connection, &generation_id)?;
        Ok(ActiveIrGraph {
            generation_id,
            nodes,
            edges,
        })
    }

    fn load_active_claim_input_snapshot(
        &self,
    ) -> Result<ActiveClaimInputSnapshot, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_only()?;
        let files = query_indexed_files(&connection, &generation_id)?;
        let units = query_code_units(&connection, &generation_id)?;
        let (ir_nodes, ir_edges) = query_ir_graph(&connection, &generation_id)?;
        let semantic_facts = query_semantic_facts(&connection, &generation_id)?;

        Ok(ActiveClaimInputSnapshot {
            generation_id,
            files,
            units,
            ir_nodes,
            ir_edges,
            semantic_facts,
        })
    }

    fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError> {
        let (generation_id, connection) = self.open_active_generation_read_model()?;
        query_active_repo_shape_stats(&connection, &generation_id)
    }

    fn validate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError> {
        let connection = self.open_existing_generation(&generation.generation_id)?;
        let inspection = inspect_connection(&connection, Some(generation.generation_id.as_str()))?;
        if inspection.schema_version != Some(STORAGE_SCHEMA_VERSION) {
            return Err(IndexStoreError::InvalidState(
                "storage schema version is missing or unsupported".to_string(),
            ));
        }
        if inspection.integrity_check.as_deref() != Some("ok") {
            return Err(IndexStoreError::InvalidState(
                "SQLite integrity check failed".to_string(),
            ));
        }
        if !required_schema_is_present(&connection)? {
            return Err(IndexStoreError::InvalidState(
                "required storage schema is missing or malformed".to_string(),
            ));
        }
        if family_evidence_violation_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "family evidence is inconsistent with indexed code units".to_string(),
            ));
        }
        if semantic_evidence_violation_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "semantic fact evidence is inconsistent with indexed code units".to_string(),
            ));
        }
        if derived_dependency_violation_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "derived record dependencies are inconsistent with indexed files".to_string(),
            ));
        }
        if dirty_record_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "dirty records cannot support an active generation".to_string(),
            ));
        }
        if ir_graph_violation_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "IR graph is inconsistent with indexed code units".to_string(),
            ));
        }
        // Set-wide re-proof of the two per-record `record_code_unit` invariants
        // that the evidence-scoped scans above never revisit for units carrying
        // no evidence: the unit hash must equal its file's hash, and the unit
        // range must stay within the file. This makes the activation gate a
        // strict superset of the historical per-record enforcement.
        if code_unit_file_conformance_violation_count(&connection, &generation.generation_id)? != 0
        {
            return Err(IndexStoreError::InvalidState(
                "code units are inconsistent with indexed files".to_string(),
            ));
        }
        match generation_status(&connection, &generation.generation_id)?.as_deref() {
            Some("building") => {
                let updated = connection
                    .execute(
                        "UPDATE index_generations \
                         SET status = 'validated' \
                         WHERE generation_id = ?1 AND status = 'building'",
                        params![generation.generation_id],
                    )
                    .map_err(sql_unavailable)?;
                if updated != 1 {
                    return Err(IndexStoreError::InvalidState(
                        "generation status changed during validation".to_string(),
                    ));
                }
            }
            Some("validated") => {}
            Some("active") => {
                return Err(IndexStoreError::InvalidState(
                    "active generation cannot be revalidated".to_string(),
                ));
            }
            Some("failed") => {
                return Err(IndexStoreError::InvalidState(
                    "failed generation cannot be validated".to_string(),
                ));
            }
            Some(status) => {
                return Err(IndexStoreError::InvalidState(format!(
                    "unsupported generation status: {status}"
                )));
            }
            None => {
                return Err(IndexStoreError::InvalidState(
                    "generation row is missing".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn activate_generation(&self, generation: &GenerationHandle) -> Result<(), IndexStoreError> {
        // Validate exactly once per activation. The sync pipeline validates the
        // generation immediately before activating it under the held index lock,
        // leaving it `validated`; re-running the whole-database integrity check
        // here would be the second full-database scan per sync. Nothing can write
        // between validate and activate while the lock is held, so a `validated`
        // status is authoritative and its violation scans need not be repeated.
        // Only a not-yet-validated (`building`) generation is validated here —
        // the direct-activate path used to seed an initial active generation
        // hands a freshly prepared `building` row straight to activation.
        let status = {
            let connection = self.open_existing_generation(&generation.generation_id)?;
            generation_status(&connection, &generation.generation_id)?
        };
        match status.as_deref() {
            Some("validated") => {}
            Some("building") => self.validate_generation(generation)?,
            Some("active") => {
                return Err(IndexStoreError::InvalidState(
                    "active generation cannot be reactivated".to_string(),
                ));
            }
            Some("failed") => {
                return Err(IndexStoreError::InvalidState(
                    "failed generation cannot be activated".to_string(),
                ));
            }
            Some(other) => {
                return Err(IndexStoreError::InvalidState(format!(
                    "unsupported generation status: {other}"
                )));
            }
            None => {
                return Err(IndexStoreError::InvalidState(
                    "generation row is missing".to_string(),
                ));
            }
        }
        let mut connection = self.open_existing_generation(&generation.generation_id)?;
        let transaction = connection.transaction().map_err(sql_unavailable)?;
        transaction
            .execute(
                "UPDATE index_generations \
                 SET status = 'validated' \
                 WHERE status = 'active' AND generation_id <> ?1",
                params![generation.generation_id],
            )
            .map_err(sql_unavailable)?;
        let updated = transaction
            .execute(
                "UPDATE index_generations \
                 SET status = 'active', activated_at = datetime('now') \
                 WHERE generation_id = ?1 AND status = 'validated'",
                params![generation.generation_id],
            )
            .map_err(sql_unavailable)?;
        if updated != 1 {
            return Err(IndexStoreError::InvalidState(
                "generation must be validated before activation".to_string(),
            ));
        }
        transaction.commit().map_err(sql_unavailable)?;
        Ok(())
    }

    fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
        self.ensure_layout()?;
        if let Some(connection) = self.try_open_mutable_database()? {
            let legacy_generation_layout_present = self.legacy_generation_layout_present()?;
            let (wal_bytes, shm_bytes) = self.mutable_sidecar_sizes(true)?;
            let active_generation = active_generation_id(&connection)?;
            if let Some(generation_id) = &active_generation {
                if generation_status(&connection, generation_id)?.as_deref() != Some("active") {
                    return Err(IndexStoreError::InvalidState(
                        "active generation row is not marked active".to_string(),
                    ));
                }
            }
            let mut inspection = inspect_connection(&connection, active_generation.as_deref())?;
            inspection.layout = if legacy_generation_layout_present {
                IndexStorageLayout::MutableWithLegacy
            } else {
                IndexStorageLayout::Mutable
            };
            inspection.mutable_database_present = true;
            inspection.legacy_generation_layout_present = legacy_generation_layout_present;
            inspection.wal_bytes = wal_bytes;
            inspection.shm_bytes = shm_bytes;
            return Ok(inspection);
        }

        let legacy_generation_layout_present = self.legacy_generation_layout_present()?;
        let current = self.current_generation_path();
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StorageInspection {
                    layout: if legacy_generation_layout_present {
                        IndexStorageLayout::Legacy
                    } else {
                        IndexStorageLayout::Empty
                    },
                    mutable_database_present: false,
                    legacy_generation_layout_present,
                    wal_bytes: None,
                    shm_bytes: None,
                    active_generation: None,
                    schema_version: None,
                    code_unit_count: None,
                    dependency_record_count: None,
                    dirty_record_count: None,
                    journal_mode: None,
                    foreign_keys_enabled: None,
                    busy_timeout_ms: None,
                    temp_store: None,
                    integrity_check: None,
                });
            }
            Err(_) => return Err(unavailable("failed to inspect current-generation pointer")),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(IndexStoreError::InvalidState(
                "current-generation must be a regular file".to_string(),
            ));
        }
        let generation_id = fs::read_to_string(&current)
            .map_err(|_| unavailable("failed to read current-generation pointer"))?
            .trim()
            .to_string();
        validate_generation_id(&generation_id)?;
        let connection = self.open_legacy_existing_generation_read_only(&generation_id)?;
        if generation_status(&connection, &generation_id)?.as_deref() != Some("active") {
            return Err(IndexStoreError::InvalidState(
                "current-generation does not point at an active generation".to_string(),
            ));
        }
        let mut inspection = inspect_connection(&connection, Some(&generation_id))?;
        inspection.layout = IndexStorageLayout::Legacy;
        inspection.mutable_database_present = false;
        inspection.legacy_generation_layout_present = true;
        inspection.wal_bytes = None;
        inspection.shm_bytes = None;
        Ok(inspection)
    }
}

impl GenerationEngineStampStore for SqliteIndexStore {
    fn active_generation_engine_version(&self) -> Result<Option<String>, IndexStoreError> {
        self.ensure_layout()?;
        let Some(connection) = self.try_open_mutable_database_read_only()? else {
            return Ok(None);
        };
        let Some(generation_id) = active_generation_id(&connection)? else {
            return Ok(None);
        };
        // A missing row or a NULL/absent stamp both map to `None` so the sync
        // preflight treats an unstamped base generation as an engine mismatch
        // rather than hard-erroring the whole sync.
        connection
            .query_row(
                "SELECT repogrammar_version FROM index_generations WHERE generation_id = ?1",
                params![generation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map(Option::flatten)
            .map_err(sql_unavailable)
    }
}

impl GenerationRetentionStore for SqliteIndexStore {
    fn prune_generations(
        &self,
        request: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, IndexStoreError> {
        if self.try_open_mutable_database()?.is_some() {
            return self.prune_mutable_generations(request);
        }
        self.prune_legacy_generation_directories(request)
    }
}

impl SqliteIndexStore {
    fn prune_mutable_generations(
        &self,
        request: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, IndexStoreError> {
        let mut connection = self.open_mutable_database(MissingDatabase::Rejected)?;
        let active_generation = active_generation_id(&connection)?.ok_or_else(|| {
            IndexStoreError::InvalidState("active generation is missing".to_string())
        })?;
        let mut inactive = list_database_generation_entries(&connection)?
            .into_iter()
            .filter(|entry| entry.generation_id != active_generation)
            .collect::<Vec<_>>();
        inactive.sort_by_key(|entry| std::cmp::Reverse(entry.number));

        let retained = inactive
            .iter()
            .take(request.keep_inactive)
            .map(|entry| entry.generation_id.clone())
            .collect::<Vec<_>>();
        let mut candidates = inactive
            .into_iter()
            .skip(request.keep_inactive)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|entry| entry.number);

        let candidate_generations = candidates
            .iter()
            .map(|entry| entry.generation_id.clone())
            .collect::<Vec<_>>();
        let mut deleted_generations = Vec::new();

        if !request.dry_run {
            let transaction = connection.transaction().map_err(sql_unavailable)?;
            if active_generation_id(&transaction)?.as_deref() != Some(active_generation.as_str()) {
                return Err(IndexStoreError::InvalidState(
                    "active generation changed during prune; retry repogrammar prune".to_string(),
                ));
            }
            for candidate in candidates {
                if candidate.generation_id == active_generation {
                    return Err(IndexStoreError::InvalidState(
                        "refusing to prune active generation".to_string(),
                    ));
                }
                transaction
                    .execute(
                        "DELETE FROM index_generations \
                         WHERE generation_id = ?1 AND status <> 'active'",
                        params![candidate.generation_id],
                    )
                    .map_err(sql_unavailable)?;
                deleted_generations.push(candidate.generation_id);
            }
            transaction.commit().map_err(sql_unavailable)?;
            let _ = run_post_commit_maintenance(&connection)?;
        }

        Ok(GenerationPruneReport {
            active_generation,
            keep_inactive: request.keep_inactive,
            retained_inactive_generations: retained,
            candidate_generations,
            deleted_generations,
            dry_run: request.dry_run,
        })
    }

    fn prune_legacy_generation_directories(
        &self,
        request: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, IndexStoreError> {
        let (active_generation, active_connection) =
            self.open_legacy_active_generation_read_only()?;
        drop(active_connection);

        let entries = self.list_generation_directories()?;
        let mut inactive = entries
            .into_iter()
            .filter(|entry| entry.generation_id != active_generation)
            .collect::<Vec<_>>();
        inactive.sort_by_key(|entry| std::cmp::Reverse(entry.number));

        let retained = inactive
            .iter()
            .take(request.keep_inactive)
            .map(|entry| entry.generation_id.clone())
            .collect::<Vec<_>>();
        let mut candidates = inactive
            .into_iter()
            .skip(request.keep_inactive)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|entry| entry.number);

        let candidate_generations = candidates
            .iter()
            .map(|entry| entry.generation_id.clone())
            .collect::<Vec<_>>();
        let mut deleted_generations = Vec::new();

        if !request.dry_run {
            for candidate in candidates {
                self.ensure_active_generation_unchanged(&active_generation)?;
                if candidate.generation_id == active_generation {
                    return Err(IndexStoreError::InvalidState(
                        "refusing to prune active generation".to_string(),
                    ));
                }
                let generation_dir = self.generation_dir(&candidate.generation_id);
                ensure_existing_real_dir(&generation_dir, "generation directory")?;
                fs::remove_dir_all(&generation_dir)
                    .map_err(|_| unavailable("failed to remove generation directory"))?;
                deleted_generations.push(candidate.generation_id);
            }
        }

        Ok(GenerationPruneReport {
            active_generation,
            keep_inactive: request.keep_inactive,
            retained_inactive_generations: retained,
            candidate_generations,
            deleted_generations,
            dry_run: request.dry_run,
        })
    }
}

impl IndexMaintenanceStore for SqliteIndexStore {
    fn compact_storage(
        &self,
        request: IndexCompactRequest,
    ) -> Result<IndexCompactReport, IndexStoreError> {
        self.require_existing_layout()?;
        self.compact_mutable_database(request)
    }
}

impl IndexStorageCleanStore for SqliteIndexStore {
    fn clean_storage(
        &self,
        request: StorageCleanRequest,
    ) -> Result<StorageCleanReport, IndexStoreError> {
        self.clean_mutable_storage(request)
    }
}

impl FamilyStore for SqliteIndexStore {
    fn record_family(
        &self,
        generation: &GenerationHandle,
        family: &IndexedFamilyRecord,
    ) -> Result<(), StoreError> {
        let mut session =
            SqliteGenerationWriteSession::open(self, generation).map_err(family_store_error)?;
        session.record_family(family)?;
        session.seal(false).map_err(family_store_error)
    }

    fn record_family_member(
        &self,
        generation: &GenerationHandle,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError> {
        let mut session =
            SqliteGenerationWriteSession::open(self, generation).map_err(family_store_error)?;
        session.record_family_member(member)?;
        session.seal(false).map_err(family_store_error)
    }

    fn record_variation_slot(
        &self,
        generation: &GenerationHandle,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError> {
        let mut session =
            SqliteGenerationWriteSession::open(self, generation).map_err(family_store_error)?;
        session.record_variation_slot(slot)?;
        session.seal(false).map_err(family_store_error)
    }

    fn record_family_evidence(
        &self,
        generation: &GenerationHandle,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError> {
        let mut session =
            SqliteGenerationWriteSession::open(self, generation).map_err(family_store_error)?;
        session.record_family_evidence(evidence)?;
        session.seal(false).map_err(family_store_error)
    }

    fn list_active_families(&self) -> Result<ActiveFamilies, StoreError> {
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let families = query_families(&connection, &generation_id).map_err(family_store_error)?;
        Ok(ActiveFamilies {
            generation_id,
            families,
        })
    }

    fn list_active_family_summaries(&self) -> Result<ActiveFamilySummaries, StoreError> {
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let families =
            query_family_summaries(&connection, &generation_id).map_err(family_store_error)?;
        Ok(ActiveFamilySummaries {
            generation_id,
            families,
        })
    }

    fn list_active_family_evidence_projection(
        &self,
    ) -> Result<ActiveFamilyEvidenceProjection, StoreError> {
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let rows = query_family_evidence_projection(&connection, &generation_id)
            .map_err(family_store_error)?;
        Ok(ActiveFamilyEvidenceProjection {
            generation_id,
            rows,
        })
    }

    fn list_active_family_search_summaries(
        &self,
    ) -> Result<ActiveFamilySearchSummaries, StoreError> {
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let families = query_family_search_summaries(&connection, &generation_id)
            .map_err(family_store_error)?;
        Ok(ActiveFamilySearchSummaries {
            generation_id,
            families,
        })
    }

    fn find_active_families_by_member(
        &self,
        code_unit_id: &str,
    ) -> Result<ActiveFamilyCandidates, StoreError> {
        validate_index_text_field(code_unit_id, "code unit id").map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let candidates =
            query_family_candidates_by_member(&connection, &generation_id, code_unit_id)
                .map_err(family_store_error)?;
        Ok(ActiveFamilyCandidates {
            generation_id,
            candidates,
            truncated: false,
        })
    }

    fn find_active_families_by_role(
        &self,
        role: &str,
        limit: usize,
    ) -> Result<ActiveFamilyCandidates, StoreError> {
        validate_index_text_field(role, "family member role").map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        query_limited_family_candidates(
            &connection,
            &generation_id,
            limit,
            |connection, generation_id, row_limit| {
                query_family_candidates_by_role(connection, generation_id, role, row_limit)
            },
        )
        .map_err(family_store_error)
    }

    fn find_active_families_by_evidence_path(
        &self,
        path: &str,
        limit: usize,
    ) -> Result<ActiveFamilyCandidates, StoreError> {
        validate_stored_repo_relative_path(path, "query evidence path")
            .map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        query_limited_family_candidates(
            &connection,
            &generation_id,
            limit,
            |connection, generation_id, row_limit| {
                query_family_candidates_by_evidence_path(connection, generation_id, path, row_limit)
            },
        )
        .map_err(family_store_error)
    }

    fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError> {
        validate_index_text_field(family_id, "family id").map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        let Some(family) =
            query_family(&connection, &generation_id, family_id).map_err(family_store_error)?
        else {
            return Ok(None);
        };
        let members = query_family_members(&connection, &generation_id, family_id)
            .map_err(family_store_error)?;
        let variation_slots = query_variation_slots(&connection, &generation_id, family_id)
            .map_err(family_store_error)?;
        let evidence = query_family_evidence(&connection, &generation_id, family_id)
            .map_err(family_store_error)?;
        Ok(Some(ActiveFamily {
            generation_id,
            family,
            members,
            variation_slots,
            evidence,
        }))
    }
}

/// Storage-format version of the constraint-profile JSON column. Bumped only if
/// the serialized shape changes; hydration rejects any other version.
const CONSTRAINT_PROFILE_JSON_VERSION: u64 = 1;

impl FamilyConstraintProfileStore for SqliteIndexStore {
    fn record_family_constraint_profile(
        &self,
        generation: &GenerationHandle,
        record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), StoreError> {
        let mut session =
            SqliteGenerationWriteSession::open(self, generation).map_err(family_store_error)?;
        session.record_family_constraint_profile(record)?;
        session.seal(false).map_err(family_store_error)
    }

    fn show_family_constraint_profile(
        &self,
        family_id: &str,
    ) -> Result<Option<FamilyConstraintProfile>, StoreError> {
        validate_index_text_field(family_id, "family id").map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_model()
            .map_err(family_store_error)?;
        query_family_constraint_profile(&connection, &generation_id, family_id)
            .map_err(family_store_error)
    }
}

/// Rows committed per bounded batch (Phase 5 design §5). Chosen so a worst-case
/// batch (semantic facts write four rows per record) stays a few MB of WAL and a
/// sub-100 ms commit; the session also commits at explicit phase checkpoints. A
/// `building` generation is never read and never resumed, so this bound targets
/// WAL size and lock-hold time, not partial-work durability.
const WRITE_SESSION_BATCH_CAPACITY: usize = 2_000;

/// Deterministic fault-injection points for the write-session fault tests. Only
/// ever set through the `#[cfg(test)]` setter; production builds neither compile
/// the field nor construct a variant.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InjectedWriteFault {
    /// Fail the next record once `rows_written` has reached the threshold.
    AfterRows(usize),
    /// Fail a batch just before it commits.
    BeforeCommit,
    /// In `record_semantic_fact`, fail after the evidence insert and before the
    /// fact insert (the mid-record seam).
    AfterEvidenceBeforeFact,
}

/// Single writer for one `building` generation (Phase 5 design). Owns one
/// connection with pragmas applied once, writes through bounded-batch
/// transactions, and finally either `finish`es (commit + seal) or `abandon`s
/// (rollback + terminal `failed` status). Every record method reproduces the
/// field-level and referential validation of the historical per-record store
/// methods; referential reads run on this session's own connection, which sees
/// both committed batches and the current open batch, so the checks are at least
/// as strong as the previous per-record SELECTs against the committed database.
pub struct SqliteGenerationWriteSession {
    connection: Connection,
    generation: GenerationHandle,
    sealed: bool,
    batch_open: bool,
    rows_in_batch: usize,
    batch_capacity: usize,
    transactions: usize,
    rows_written: usize,
    checkpoints: usize,
    #[cfg(test)]
    injected_fault: Option<InjectedWriteFault>,
    #[cfg(test)]
    instrumentation: std::sync::Arc<WriteInstrumentation>,
}

impl SqliteGenerationWriteSession {
    fn open(
        store: &SqliteIndexStore,
        generation: &GenerationHandle,
    ) -> Result<Self, IndexStoreError> {
        let connection = store.open_existing_generation(&generation.generation_id)?;
        // Require a building generation with a grammatical, record-agnostic
        // message (one session backs every record kind for the whole build).
        match generation_status(&connection, &generation.generation_id)?.as_deref() {
            Some("building") => {}
            Some(status) => {
                return Err(IndexStoreError::InvalidState(format!(
                    "records may only be written for building generations, found {status}"
                )));
            }
            None => {
                return Err(IndexStoreError::InvalidState(
                    "generation row is missing".to_string(),
                ));
            }
        }
        #[cfg(test)]
        store
            .write_instrumentation
            .connection_opens
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Self {
            connection,
            generation: generation.clone(),
            sealed: false,
            batch_open: false,
            rows_in_batch: 0,
            batch_capacity: WRITE_SESSION_BATCH_CAPACITY,
            transactions: 0,
            rows_written: 0,
            checkpoints: 0,
            #[cfg(test)]
            injected_fault: None,
            #[cfg(test)]
            instrumentation: store.write_instrumentation(),
        })
    }

    fn generation_id(&self) -> &str {
        &self.generation.generation_id
    }

    fn ensure_not_sealed(&self) -> Result<(), IndexStoreError> {
        if self.sealed {
            return Err(IndexStoreError::InvalidState(
                "generation write session is already sealed".to_string(),
            ));
        }
        Ok(())
    }

    /// Open a batch if none is active and re-assert the `building` status under
    /// the write lock. `BEGIN IMMEDIATE` takes the write lock up front, so the
    /// status observed here is held for the whole batch; the process-level index
    /// lock additionally excludes any other writer, making the per-batch status
    /// check equivalent to (not weaker than) the historical per-record check.
    fn ensure_batch(&mut self) -> Result<(), IndexStoreError> {
        self.check_after_rows_fault()?;
        if self.batch_open {
            return Ok(());
        }
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(sql_unavailable)?;
        self.batch_open = true;
        self.rows_in_batch = 0;
        // Roll back the just-opened batch on every non-`building` outcome,
        // including a status-read error, so the per-batch status re-assertion is
        // never silently skipped for an open batch.
        let status = match generation_status(&self.connection, self.generation_id()) {
            Ok(status) => status,
            Err(error) => {
                self.rollback_batch();
                return Err(error);
            }
        };
        match status.as_deref() {
            Some("building") => Ok(()),
            Some(status) => {
                self.rollback_batch();
                Err(IndexStoreError::InvalidState(format!(
                    "records may only be written for building generations, found {status}"
                )))
            }
            None => {
                self.rollback_batch();
                Err(IndexStoreError::InvalidState(
                    "generation row is missing".to_string(),
                ))
            }
        }
    }

    fn note_rows(&mut self, rows: usize) -> Result<(), IndexStoreError> {
        self.rows_in_batch = self.rows_in_batch.saturating_add(rows);
        self.rows_written = self.rows_written.saturating_add(rows);
        if self.rows_in_batch >= self.batch_capacity {
            self.commit_batch()?;
        }
        Ok(())
    }

    fn commit_batch(&mut self) -> Result<(), IndexStoreError> {
        if !self.batch_open {
            return Ok(());
        }
        self.check_before_commit_fault()?;
        self.connection
            .execute_batch("COMMIT")
            .map_err(sql_unavailable)?;
        self.batch_open = false;
        self.rows_in_batch = 0;
        self.transactions = self.transactions.saturating_add(1);
        self.instrument_transaction();
        Ok(())
    }

    fn rollback_batch(&mut self) {
        if self.batch_open {
            let _ = self.connection.execute_batch("ROLLBACK");
            self.batch_open = false;
            self.rows_in_batch = 0;
        }
    }

    /// Best-effort terminal status write for an abandoned build that already
    /// persisted rows. Purely additive: the schema already allows `failed`,
    /// `validate_generation` refuses it, and prune deletes any non-active
    /// generation, so a `failed` row is inert. It is applied only when at least
    /// one batch committed (`transactions > 0`); a session that rolled back its
    /// sole open batch persisted nothing, so its generation stays a pristine,
    /// reusable `building` row. This keeps the granular per-record store methods
    /// (each a one-shot session) from stamping `failed` on a mere field- or
    /// referential-validation rejection.
    fn mark_failed_if_persisted(&self) {
        if self.transactions == 0 {
            return;
        }
        let _ = self.connection.execute(
            "UPDATE index_generations SET status = 'failed' \
             WHERE generation_id = ?1 AND status = 'building'",
            params![self.generation.generation_id],
        );
    }

    /// Commit the final batch, seal the session, then optionally run best-effort
    /// post-commit maintenance. Sealing happens **before** maintenance so that a
    /// transient `PRAGMA optimize`/`wal_checkpoint` failure over already-committed
    /// rows cannot leave the session unsealed and let `Drop` stamp a durable
    /// build `failed`. A second seal (after `finish` or `abandon`) is a typed
    /// error rather than a silent success, closing the abandon-then-finish
    /// footgun. `run_maintenance` is true for a whole-build `finish` and for the
    /// granular `remove_indexed_file` (which historically ran maintenance) and
    /// false for the other granular one-shot record paths (which historically ran
    /// none), so per-record maintenance is not newly introduced.
    fn seal(&mut self, run_maintenance: bool) -> Result<(), IndexStoreError> {
        if self.sealed {
            return Err(IndexStoreError::InvalidState(
                "generation write session is already sealed".to_string(),
            ));
        }
        self.commit_batch()?;
        self.sealed = true;
        if run_maintenance {
            let _ = run_post_commit_maintenance(&self.connection);
        }
        Ok(())
    }

    #[cfg(test)]
    fn instrument_transaction(&self) {
        self.instrumentation
            .transactions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(not(test))]
    fn instrument_transaction(&self) {}

    #[cfg(test)]
    fn instrument_checkpoint(&self) {
        self.instrumentation
            .checkpoints
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(not(test))]
    fn instrument_checkpoint(&self) {}

    fn write_indexed_file(&mut self, file: &IndexedFileRecord) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(&file.path)?;
        let size_bytes = i64::try_from(file.size_bytes)
            .map_err(|_| invalid_record("file size exceeds SQLite integer range"))?;
        self.ensure_batch()?;
        let existing_file =
            indexed_file_metadata(&self.connection, self.generation_id(), &file.path)?;
        if existing_file.as_ref().is_some_and(|existing| {
            existing.content_hash == file.content_hash.as_str()
                && existing.size_bytes == size_bytes
                && existing.language == file.language
        }) {
            return Ok(());
        }
        let mut rows = 0usize;
        if existing_file.is_some() {
            mark_dependents_dirty_for_path(
                &self.connection,
                self.generation_id(),
                &file.path,
                "path_replaced",
            )?;
            self.connection
                .execute(
                    "DELETE FROM indexed_files WHERE generation_id = ?1 AND path = ?2",
                    params![self.generation.generation_id, file.path],
                )
                .map_err(sql_unavailable)?;
            rows += 1;
        }
        self.connection
            .execute(
                "INSERT INTO indexed_files \
                 (generation_id, path, content_hash, size_bytes, language) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    self.generation.generation_id,
                    file.path,
                    file.content_hash.as_str(),
                    size_bytes,
                    file.language,
                ],
            )
            .map_err(sql_unavailable)?;
        rows += 1;
        self.note_rows(rows)
    }

    fn write_remove_indexed_file(&mut self, path: &str) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(path)?;
        self.ensure_batch()?;
        if indexed_file_metadata(&self.connection, self.generation_id(), path)?.is_some() {
            mark_dependents_dirty_for_path(
                &self.connection,
                self.generation_id(),
                path,
                "path_removed",
            )?;
            self.connection
                .execute(
                    "DELETE FROM indexed_files WHERE generation_id = ?1 AND path = ?2",
                    params![self.generation.generation_id, path],
                )
                .map_err(sql_unavailable)?;
            self.note_rows(1)?;
        }
        Ok(())
    }

    fn write_code_unit(&mut self, unit: &IndexedCodeUnitRecord) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(&unit.path)?;
        let start_byte = i64::try_from(unit.start_byte)
            .map_err(|_| invalid_record("code unit range exceeds SQLite integer range"))?;
        let end_byte = i64::try_from(unit.end_byte)
            .map_err(|_| invalid_record("code unit range exceeds SQLite integer range"))?;
        self.ensure_batch()?;
        let Some((file_hash, file_size_bytes)) = self
            .connection
            .query_row(
                "SELECT content_hash, size_bytes FROM indexed_files \
                 WHERE generation_id = ?1 AND path = ?2",
                params![self.generation.generation_id, unit.path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(sql_unavailable)?
        else {
            return Err(invalid_record(
                "code unit must reference an indexed file in the same generation",
            ));
        };
        if file_hash != unit.content_hash.as_str() {
            return Err(invalid_record(
                "code unit content hash must match indexed file content hash",
            ));
        }
        if end_byte > file_size_bytes {
            return Err(invalid_record(
                "code unit range must not exceed indexed file size",
            ));
        }
        self.connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    self.generation.generation_id,
                    unit.id,
                    unit.path,
                    unit.language,
                    unit.kind,
                    start_byte,
                    end_byte,
                    unit.content_hash.as_str(),
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_ir_node(&mut self, node: &IndexedIrNodeRecord) -> Result<(), IndexStoreError> {
        validate_index_text_field(&node.id, "IR node id")?;
        validate_index_text_field(&node.code_unit_id, "IR node code unit id")?;
        IrNodeKind::parse_protocol_str(&node.kind).map_err(|_| {
            IndexStoreError::InvalidRecord("IR node kind is unsupported".to_string())
        })?;
        validate_empty_object_payload(&node.payload_json, "IR node payload")?;
        if node.id != format!("ir:{}", node.code_unit_id) {
            return Err(invalid_record(
                "IR node id must be derived from code unit id",
            ));
        }
        self.ensure_batch()?;
        let code_unit_exists = self
            .connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND code_unit_id = ?2",
                params![self.generation.generation_id, node.code_unit_id],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_unavailable)?;
        if code_unit_exists != 1 {
            return Err(invalid_record(
                "IR node must reference an indexed code unit in the same generation",
            ));
        }
        self.connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    self.generation.generation_id,
                    node.id,
                    node.code_unit_id,
                    node.kind,
                    node.payload_json,
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_ir_edge(&mut self, edge: &IndexedIrEdgeRecord) -> Result<(), IndexStoreError> {
        validate_index_text_field(&edge.from_node_id, "IR edge from node id")?;
        validate_index_text_field(&edge.to_node_id, "IR edge to node id")?;
        IrEdgeLabel::parse_protocol_str(&edge.label).map_err(|_| {
            IndexStoreError::InvalidRecord("IR edge label is unsupported".to_string())
        })?;
        if edge.from_node_id == edge.to_node_id {
            return Err(invalid_record("IR edge must not point to itself"));
        }
        self.ensure_batch()?;
        for (node_id, label) in [
            (edge.from_node_id.as_str(), "from"),
            (edge.to_node_id.as_str(), "to"),
        ] {
            let node_exists = self
                .connection
                .query_row(
                    "SELECT count(*) FROM ir_nodes \
                     WHERE generation_id = ?1 AND node_id = ?2",
                    params![self.generation.generation_id, node_id],
                    |row| row.get::<_, u32>(0),
                )
                .map_err(sql_unavailable)?;
            if node_exists != 1 {
                return Err(IndexStoreError::InvalidRecord(format!(
                    "IR edge must reference an indexed {label} node in the same generation"
                )));
            }
        }
        self.connection
            .execute(
                "INSERT INTO ir_edges \
                 (generation_id, from_node_id, to_node_id, label) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    self.generation.generation_id,
                    edge.from_node_id,
                    edge.to_node_id,
                    edge.label,
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_semantic_fact(
        &mut self,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(&fact.path)?;
        let start_byte = i64::try_from(fact.start_byte)
            .map_err(|_| invalid_record("semantic fact range exceeds SQLite integer range"))?;
        let end_byte = i64::try_from(fact.end_byte)
            .map_err(|_| invalid_record("semantic fact range exceeds SQLite integer range"))?;
        let assumptions_json = serde_json::to_string(&fact.assumptions).map_err(|_| {
            IndexStoreError::InvalidRecord("semantic fact assumptions are invalid".to_string())
        })?;
        self.ensure_batch()?;
        let Some((unit_path, unit_hash, unit_start_byte, unit_end_byte, file_hash)) = self
            .connection
            .query_row(
                "SELECT code_units.path, code_units.content_hash, code_units.start_byte, \
                        code_units.end_byte, indexed_files.content_hash \
                 FROM code_units \
                 JOIN indexed_files \
                   ON indexed_files.generation_id = code_units.generation_id \
                  AND indexed_files.path = code_units.path \
                 WHERE code_units.generation_id = ?1 \
                   AND code_units.code_unit_id = ?2",
                params![self.generation.generation_id, fact.code_unit_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_unavailable)?
        else {
            return Err(invalid_record(
                "semantic fact must reference an indexed code unit in the same generation",
            ));
        };
        if unit_path != fact.path {
            return Err(invalid_record(
                "semantic fact evidence path must match code unit path",
            ));
        }
        if unit_hash != fact.content_hash.as_str() || file_hash != fact.content_hash.as_str() {
            return Err(invalid_record(
                "semantic fact content hash must match indexed file and code unit",
            ));
        }
        if start_byte < unit_start_byte || end_byte > unit_end_byte {
            return Err(invalid_record(
                "semantic fact range must stay within code unit range",
            ));
        }
        self.connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    self.generation.generation_id,
                    fact.evidence_id,
                    fact.code_unit_id,
                    fact.path,
                    fact.content_hash.as_str(),
                    start_byte,
                    end_byte,
                    fact.note,
                ],
            )
            .map_err(sql_unavailable)?;
        self.check_after_evidence_before_fact_fault()?;
        self.connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, \
                  origin_engine, origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    self.generation.generation_id,
                    fact.fact_id,
                    fact.kind,
                    fact.subject,
                    fact.target.as_deref(),
                    fact.certainty,
                    fact.origin_engine,
                    fact.origin_engine_version,
                    fact.origin_method,
                    assumptions_json,
                    fact.evidence_id,
                ],
            )
            .map_err(sql_unavailable)?;
        record_derived_dependency(
            &self.connection,
            self.generation_id(),
            "semantic_fact",
            &fact.fact_id,
            &fact.path,
            fact.content_hash.as_str(),
        )?;
        record_derived_dependency(
            &self.connection,
            self.generation_id(),
            "semantic_evidence",
            &fact.evidence_id,
            &fact.path,
            fact.content_hash.as_str(),
        )?;
        self.note_rows(4)
    }

    fn write_family(&mut self, family: &IndexedFamilyRecord) -> Result<(), IndexStoreError> {
        validate_index_text_field(&family.family_id, "family id")?;
        validate_family_classification(&family.classification)?;
        let prevalence = &family.prevalence;
        if prevalence.classification_reason.trim().is_empty() {
            return Err(invalid_record(
                "family classification reason must not be empty",
            ));
        }
        let eligible_peer_count =
            family_prevalence_count_param(prevalence.eligible_peer_count, "eligible peer count")?;
        let supported_member_count = family_prevalence_count_param(
            prevalence.supported_member_count,
            "supported member count",
        )?;
        let competing_ready_family_count = family_prevalence_count_param(
            prevalence.competing_ready_family_count,
            "competing ready family count",
        )?;
        let largest_competing_support = family_prevalence_count_param(
            prevalence.largest_competing_support,
            "largest competing support",
        )?;
        let blocked_peer_count =
            family_prevalence_count_param(prevalence.blocked_peer_count, "blocked peer count")?;
        let unsupported_peer_count = family_prevalence_count_param(
            prevalence.unsupported_peer_count,
            "unsupported peer count",
        )?;
        self.ensure_batch()?;
        self.connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    self.generation.generation_id,
                    family.family_id,
                    family.classification,
                    eligible_peer_count,
                    supported_member_count,
                    prevalence.coverage_ratio,
                    competing_ready_family_count,
                    largest_competing_support,
                    blocked_peer_count,
                    unsupported_peer_count,
                    prevalence.classification_reason,
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_family_member(
        &mut self,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), IndexStoreError> {
        validate_index_text_field(&member.family_id, "family member family id")?;
        validate_index_text_field(&member.code_unit_id, "family member code unit id")?;
        validate_index_text_field(&member.role, "family member role")?;
        self.ensure_batch()?;
        require_family_row(&self.connection, self.generation_id(), &member.family_id)?;
        require_code_unit_row(&self.connection, self.generation_id(), &member.code_unit_id)?;
        self.connection
            .execute(
                "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    self.generation.generation_id,
                    member.family_id,
                    member.code_unit_id,
                    member.role,
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_variation_slot(
        &mut self,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), IndexStoreError> {
        validate_index_text_field(&slot.family_id, "variation slot family id")?;
        validate_index_text_field(&slot.slot_id, "variation slot id")?;
        validate_index_text_field(&slot.description, "variation slot description")?;
        self.ensure_batch()?;
        require_family_row(&self.connection, self.generation_id(), &slot.family_id)?;
        self.connection
            .execute(
                "INSERT INTO variation_slots (generation_id, family_id, slot_id, description) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    self.generation.generation_id,
                    slot.family_id,
                    slot.slot_id,
                    slot.description,
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    fn write_family_evidence(
        &mut self,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), IndexStoreError> {
        validate_index_text_field(&evidence.evidence_id, "family evidence id")?;
        validate_index_text_field(&evidence.family_id, "family evidence family id")?;
        validate_index_text_field(&evidence.code_unit_id, "family evidence code unit id")?;
        validate_family_evidence_covered_claims(&evidence.covered_claims)?;
        validate_repo_relative_path(&evidence.path)?;
        validate_index_text_field(&evidence.note, "family evidence note")?;
        let start_byte = i64::try_from(evidence.start_byte)
            .map_err(|_| invalid_record("family evidence range exceeds SQLite integer range"))?;
        let end_byte = i64::try_from(evidence.end_byte)
            .map_err(|_| invalid_record("family evidence range exceeds SQLite integer range"))?;
        if start_byte > end_byte {
            return Err(invalid_record(
                "family evidence range start must not exceed end",
            ));
        }
        let covered_claims_json = family_evidence_covered_claims_json(&evidence.covered_claims)?;
        self.ensure_batch()?;
        require_family_row(&self.connection, self.generation_id(), &evidence.family_id)?;
        let (unit_path, unit_hash, unit_start_byte, unit_end_byte, file_hash, file_size) =
            code_unit_evidence_bounds(
                &self.connection,
                self.generation_id(),
                &evidence.code_unit_id,
            )?;
        if unit_path != evidence.path {
            return Err(invalid_record(
                "family evidence path must match code unit path",
            ));
        }
        if unit_hash != evidence.content_hash.as_str()
            || file_hash != evidence.content_hash.as_str()
        {
            return Err(invalid_record(
                "family evidence content hash must match indexed file and code unit",
            ));
        }
        if start_byte < unit_start_byte || end_byte > unit_end_byte || end_byte > file_size {
            return Err(invalid_record(
                "family evidence range must stay within code unit range",
            ));
        }
        self.connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    self.generation.generation_id,
                    evidence.evidence_id,
                    evidence.family_id,
                    evidence.code_unit_id,
                    covered_claims_json,
                    evidence.path,
                    evidence.content_hash.as_str(),
                    start_byte,
                    end_byte,
                    evidence.note,
                ],
            )
            .map_err(sql_unavailable)?;
        record_derived_dependency(
            &self.connection,
            self.generation_id(),
            "family",
            &evidence.family_id,
            &evidence.path,
            evidence.content_hash.as_str(),
        )?;
        record_derived_dependency(
            &self.connection,
            self.generation_id(),
            "family_evidence",
            &evidence.evidence_id,
            &evidence.path,
            evidence.content_hash.as_str(),
        )?;
        self.note_rows(3)
    }

    fn write_family_constraint_profile(
        &mut self,
        record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), IndexStoreError> {
        validate_index_text_field(&record.family_id, "family constraint profile family id")?;
        let profile_json = constraint_profile_to_json(&record.profile)?;
        self.ensure_batch()?;
        require_family_row(&self.connection, self.generation_id(), &record.family_id)?;
        self.connection
            .execute(
                "INSERT INTO family_constraint_profiles (generation_id, family_id, profile_json) \
                 VALUES (?1, ?2, ?3)",
                params![
                    self.generation.generation_id,
                    record.family_id,
                    profile_json
                ],
            )
            .map_err(sql_unavailable)?;
        self.note_rows(1)
    }

    #[cfg(test)]
    fn check_after_rows_fault(&mut self) -> Result<(), IndexStoreError> {
        if let Some(InjectedWriteFault::AfterRows(threshold)) = self.injected_fault {
            if self.rows_written >= threshold {
                self.injected_fault = None;
                return Err(IndexStoreError::InvalidState(
                    "injected write-session fault after rows".to_string(),
                ));
            }
        }
        Ok(())
    }

    #[cfg(not(test))]
    fn check_after_rows_fault(&mut self) -> Result<(), IndexStoreError> {
        Ok(())
    }

    #[cfg(test)]
    fn check_before_commit_fault(&mut self) -> Result<(), IndexStoreError> {
        if self.injected_fault == Some(InjectedWriteFault::BeforeCommit) {
            self.injected_fault = None;
            return Err(IndexStoreError::InvalidState(
                "injected write-session fault before commit".to_string(),
            ));
        }
        Ok(())
    }

    #[cfg(not(test))]
    fn check_before_commit_fault(&mut self) -> Result<(), IndexStoreError> {
        Ok(())
    }

    #[cfg(test)]
    fn check_after_evidence_before_fact_fault(&mut self) -> Result<(), IndexStoreError> {
        if self.injected_fault == Some(InjectedWriteFault::AfterEvidenceBeforeFact) {
            self.injected_fault = None;
            return Err(IndexStoreError::InvalidState(
                "injected write-session fault after evidence before fact".to_string(),
            ));
        }
        Ok(())
    }

    #[cfg(not(test))]
    fn check_after_evidence_before_fact_fault(&mut self) -> Result<(), IndexStoreError> {
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn inject_fault(&mut self, fault: InjectedWriteFault) {
        self.injected_fault = Some(fault);
    }

    #[cfg(test)]
    pub(crate) fn batch_is_open(&self) -> bool {
        self.batch_open
    }
}

impl GenerationWriteSession for SqliteGenerationWriteSession {
    fn generation(&self) -> &GenerationHandle {
        &self.generation
    }

    fn record_indexed_file(&mut self, file: &IndexedFileRecord) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_indexed_file(file)
    }

    fn remove_indexed_file(&mut self, path: &str) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_remove_indexed_file(path)
    }

    fn record_code_unit(&mut self, unit: &IndexedCodeUnitRecord) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_code_unit(unit)
    }

    fn record_ir_node(&mut self, node: &IndexedIrNodeRecord) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_ir_node(node)
    }

    fn record_ir_edge(&mut self, edge: &IndexedIrEdgeRecord) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_ir_edge(edge)
    }

    fn record_semantic_fact(
        &mut self,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.write_semantic_fact(fact)
    }

    fn record_family(&mut self, family: &IndexedFamilyRecord) -> Result<(), StoreError> {
        self.ensure_not_sealed().map_err(family_store_error)?;
        self.write_family(family).map_err(family_store_error)
    }

    fn record_family_member(
        &mut self,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError> {
        self.ensure_not_sealed().map_err(family_store_error)?;
        self.write_family_member(member).map_err(family_store_error)
    }

    fn record_variation_slot(
        &mut self,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError> {
        self.ensure_not_sealed().map_err(family_store_error)?;
        self.write_variation_slot(slot).map_err(family_store_error)
    }

    fn record_family_evidence(
        &mut self,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError> {
        self.ensure_not_sealed().map_err(family_store_error)?;
        self.write_family_evidence(evidence)
            .map_err(family_store_error)
    }

    fn record_family_constraint_profile(
        &mut self,
        record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), StoreError> {
        self.ensure_not_sealed().map_err(family_store_error)?;
        self.write_family_constraint_profile(record)
            .map_err(family_store_error)
    }

    fn checkpoint(&mut self) -> Result<(), IndexStoreError> {
        self.ensure_not_sealed()?;
        self.commit_batch()?;
        self.checkpoints = self.checkpoints.saturating_add(1);
        self.instrument_checkpoint();
        // Checkpoint committed frames back into the main database so WAL growth
        // is bounded to roughly one phase rather than the whole build. This is a
        // best-effort optimization over already-committed data, so a transient
        // failure is not fatal.
        let _ = run_passive_wal_checkpoint(&self.connection);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), IndexStoreError> {
        self.seal(true)
    }

    fn abandon(&mut self) -> Result<(), IndexStoreError> {
        if self.sealed {
            return Ok(());
        }
        self.rollback_batch();
        self.mark_failed_if_persisted();
        self.sealed = true;
        Ok(())
    }

    fn stats(&self) -> WriteSessionStats {
        WriteSessionStats {
            transactions: self.transactions,
            rows_written: self.rows_written,
            checkpoints: self.checkpoints,
        }
    }
}

impl Drop for SqliteGenerationWriteSession {
    fn drop(&mut self) {
        if self.sealed {
            return;
        }
        // A build dropped before `finish` (error propagation or panic unwind)
        // reclaims the generation: roll back the open batch and, if any batch
        // already committed, stamp `failed`. Errors are swallowed so drop never
        // panics.
        self.rollback_batch();
        self.mark_failed_if_persisted();
        self.sealed = true;
    }
}

impl GenerationWriteStore for SqliteIndexStore {
    fn open_generation_write_session<'a>(
        &'a self,
        generation: &GenerationHandle,
    ) -> Result<Box<dyn GenerationWriteSession + 'a>, IndexStoreError> {
        Ok(Box::new(SqliteGenerationWriteSession::open(
            self, generation,
        )?))
    }
}

fn query_family_constraint_profile(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<Option<FamilyConstraintProfile>, IndexStoreError> {
    let profile_json = connection
        .query_row(
            "SELECT profile_json FROM family_constraint_profiles \
             WHERE generation_id = ?1 AND family_id = ?2",
            params![generation_id, family_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_unavailable)?;
    let Some(profile_json) = profile_json else {
        return Ok(None);
    };
    Ok(Some(stored_constraint_profile(&profile_json)?))
}

/// Serialize a constraint profile to a deterministic, source-free JSON column.
/// Every value is validated on write; the read path re-validates so a tampered
/// row is rejected on hydration. Object keys serialize in sorted order (the
/// default `serde_json` map is a `BTreeMap`), so the encoding is deterministic.
fn constraint_profile_to_json(
    profile: &FamilyConstraintProfile,
) -> Result<String, IndexStoreError> {
    let required = profile
        .required_equal_features
        .iter()
        .map(|constraint| feature_constraint_to_json(constraint, ConstraintAxis::Required))
        .collect::<Result<Vec<_>, _>>()?;
    let prohibited = profile
        .prohibited_or_blocking_features
        .iter()
        .map(|constraint| feature_constraint_to_json(constraint, ConstraintAxis::Prohibited))
        .collect::<Result<Vec<_>, _>>()?;
    let variations = profile
        .allowed_variations
        .iter()
        .map(variation_constraint_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    let obligations = profile
        .unresolved_obligations
        .iter()
        .map(obligation_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    let value = serde_json::json!({
        "version": CONSTRAINT_PROFILE_JSON_VERSION,
        "required_equal_features": required,
        "allowed_variations": variations,
        "prohibited_or_blocking_features": prohibited,
        "unresolved_obligations": obligations,
    });
    serde_json::to_string(&value)
        .map_err(|_| invalid_record("family constraint profile JSON is invalid"))
}

/// Which array a feature constraint belongs to. The prohibited axis carries only
/// `ProhibitedPresence`; the required axis carries only the equality/subset
/// semantics. Keeping the axis explicit stops a tampered blocker row from
/// hydrating inside `required_equal_features` (or vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstraintAxis {
    Required,
    Prohibited,
}

impl ConstraintAxis {
    fn accepts(self, semantics: FeatureConstraintSemantics) -> bool {
        match self {
            Self::Required => !semantics.is_prohibition(),
            Self::Prohibited => semantics.is_prohibition(),
        }
    }
}

fn feature_constraint_to_json(
    constraint: &FeatureConstraint,
    axis: ConstraintAxis,
) -> Result<serde_json::Value, IndexStoreError> {
    validate_index_text_field(
        &constraint.prefix,
        "family constraint profile feature prefix",
    )?;
    if !axis.accepts(constraint.semantics) {
        return Err(invalid_record(
            "family constraint semantics do not belong to its axis",
        ));
    }
    // Empty-set semantics carry an empty `values` list and vice versa; reject a
    // record that contradicts its own semantics before it reaches storage.
    if constraint.semantics.requires_empty_values() != constraint.values.is_empty() {
        return Err(invalid_record(
            "family constraint feature values are inconsistent with its semantics",
        ));
    }
    for value in &constraint.values {
        validate_index_text_field(value, "family constraint profile feature value")?;
    }
    Ok(serde_json::json!({
        "prefix": constraint.prefix,
        "values": constraint.values,
        "origin": constraint.origin.as_token(),
        "semantics": constraint.semantics.as_token(),
    }))
}

fn variation_constraint_to_json(
    variation: &VariationConstraint,
) -> Result<serde_json::Value, IndexStoreError> {
    validate_index_text_field(
        &variation.dimension,
        "family constraint profile variation dimension",
    )?;
    for profile in &variation.observed_profiles {
        validate_index_text_field(profile, "family constraint profile observed profile")?;
    }
    for member_id in &variation.representative_member_ids {
        validate_index_text_field(
            member_id,
            "family constraint profile representative member id",
        )?;
    }
    Ok(serde_json::json!({
        "dimension": variation.dimension,
        "observed_profiles": variation.observed_profiles,
        "observed_profiles_truncated": variation.observed_profiles_truncated,
        "includes_absent_profile": variation.includes_absent_profile,
        "representative_member_ids": variation.representative_member_ids,
        "observed_only": variation.observed_only,
    }))
}

fn obligation_to_json(
    obligation: &UnknownObligation,
) -> Result<serde_json::Value, IndexStoreError> {
    validate_index_text_field(
        &obligation.affected_claim,
        "family constraint profile obligation affected claim",
    )?;
    if let Some(recovery) = &obligation.recovery {
        validate_index_text_field(recovery, "family constraint profile obligation recovery")?;
    }
    Ok(serde_json::json!({
        "class": obligation.class.as_protocol_str(),
        "reason": obligation.reason.as_protocol_str(),
        "affected_claim": obligation.affected_claim,
        "recovery": obligation.recovery,
    }))
}

fn malformed_constraint_profile() -> IndexStoreError {
    IndexStoreError::InvalidState("stored family constraint profile is malformed".to_string())
}

/// Parse and re-validate a stored constraint-profile JSON column into the typed
/// value. Any missing field, unknown token, wrong version, or source-like value
/// is a malformed-storage error.
fn stored_constraint_profile(json: &str) -> Result<FamilyConstraintProfile, IndexStoreError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| malformed_constraint_profile())?;
    let object = value.as_object().ok_or_else(malformed_constraint_profile)?;
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(CONSTRAINT_PROFILE_JSON_VERSION)
    {
        return Err(malformed_constraint_profile());
    }
    Ok(FamilyConstraintProfile {
        required_equal_features: stored_feature_constraints(
            object.get("required_equal_features"),
            ConstraintAxis::Required,
        )?,
        allowed_variations: stored_variation_constraints(object.get("allowed_variations"))?,
        prohibited_or_blocking_features: stored_feature_constraints(
            object.get("prohibited_or_blocking_features"),
            ConstraintAxis::Prohibited,
        )?,
        unresolved_obligations: stored_obligations(object.get("unresolved_obligations"))?,
    })
}

fn stored_feature_constraints(
    value: Option<&serde_json::Value>,
    axis: ConstraintAxis,
) -> Result<Vec<FeatureConstraint>, IndexStoreError> {
    let array = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed_constraint_profile)?;
    let mut constraints = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry.as_object().ok_or_else(malformed_constraint_profile)?;
        let prefix = constraint_profile_string(object.get("prefix"))?;
        validate_stored_semantic_text_field("stored constraint feature prefix", &prefix)?;
        let values = constraint_profile_string_array(object.get("values"))?;
        for value in &values {
            validate_stored_semantic_text_field("stored constraint feature value", value)?;
        }
        let origin =
            FeatureConstraintOrigin::parse_token(&constraint_profile_string(object.get("origin"))?)
                .map_err(|_| malformed_constraint_profile())?;
        let semantics = FeatureConstraintSemantics::parse_token(&constraint_profile_string(
            object.get("semantics"),
        )?)
        .map_err(|_| malformed_constraint_profile())?;
        // A blocker row must not hydrate inside `required_equal_features`, nor a
        // required binding inside `prohibited_or_blocking_features`.
        if !axis.accepts(semantics) {
            return Err(malformed_constraint_profile());
        }
        if semantics.requires_empty_values() != values.is_empty() {
            return Err(malformed_constraint_profile());
        }
        constraints.push(FeatureConstraint {
            prefix,
            values,
            origin,
            semantics,
        });
    }
    Ok(constraints)
}

fn stored_variation_constraints(
    value: Option<&serde_json::Value>,
) -> Result<Vec<VariationConstraint>, IndexStoreError> {
    let array = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed_constraint_profile)?;
    let mut variations = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry.as_object().ok_or_else(malformed_constraint_profile)?;
        let dimension = constraint_profile_string(object.get("dimension"))?;
        validate_stored_semantic_text_field("stored constraint variation dimension", &dimension)?;
        let observed_profiles = constraint_profile_string_array(object.get("observed_profiles"))?;
        for profile in &observed_profiles {
            validate_stored_semantic_text_field("stored constraint observed profile", profile)?;
        }
        let representative_member_ids =
            constraint_profile_string_array(object.get("representative_member_ids"))?;
        for member_id in &representative_member_ids {
            validate_stored_semantic_text_field(
                "stored constraint representative member id",
                member_id,
            )?;
        }
        let observed_only = constraint_profile_bool(object.get("observed_only"))?;
        if !observed_only {
            return Err(IndexStoreError::InvalidState(
                "stored constraint variation must be observed-only".to_string(),
            ));
        }
        variations.push(VariationConstraint {
            dimension,
            observed_profiles,
            observed_profiles_truncated: constraint_profile_bool(
                object.get("observed_profiles_truncated"),
            )?,
            includes_absent_profile: constraint_profile_bool(
                object.get("includes_absent_profile"),
            )?,
            representative_member_ids,
            observed_only,
        });
    }
    Ok(variations)
}

fn stored_obligations(
    value: Option<&serde_json::Value>,
) -> Result<Vec<UnknownObligation>, IndexStoreError> {
    let array = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed_constraint_profile)?;
    let mut obligations = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry.as_object().ok_or_else(malformed_constraint_profile)?;
        let class =
            UnknownClass::parse_protocol_str(&constraint_profile_string(object.get("class"))?)
                .map_err(|_| malformed_constraint_profile())?;
        let reason = UnknownReasonCode::parse_protocol_str(&constraint_profile_string(
            object.get("reason"),
        )?)
        .map_err(|_| malformed_constraint_profile())?;
        let affected_claim = constraint_profile_string(object.get("affected_claim"))?;
        validate_stored_semantic_text_field(
            "stored constraint obligation affected claim",
            &affected_claim,
        )?;
        let recovery = match object.get("recovery") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(recovery)) => {
                validate_stored_semantic_text_field(
                    "stored constraint obligation recovery",
                    recovery,
                )?;
                Some(recovery.clone())
            }
            Some(_) => return Err(malformed_constraint_profile()),
        };
        obligations.push(TypedUnknown {
            class,
            reason,
            affected_claim,
            recovery,
        });
    }
    Ok(obligations)
}

fn constraint_profile_string(value: Option<&serde_json::Value>) -> Result<String, IndexStoreError> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(malformed_constraint_profile)
}

fn constraint_profile_string_array(
    value: Option<&serde_json::Value>,
) -> Result<Vec<String>, IndexStoreError> {
    let array = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed_constraint_profile)?;
    array
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_string)
                .ok_or_else(malformed_constraint_profile)
        })
        .collect()
}

fn constraint_profile_bool(value: Option<&serde_json::Value>) -> Result<bool, IndexStoreError> {
    value
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(malformed_constraint_profile)
}

struct IndexedFileMetadata {
    content_hash: String,
    size_bytes: i64,
    language: String,
}

fn indexed_file_metadata(
    connection: &Connection,
    generation_id: &str,
    path: &str,
) -> Result<Option<IndexedFileMetadata>, IndexStoreError> {
    connection
        .query_row(
            "SELECT content_hash, size_bytes, language \
             FROM indexed_files \
             WHERE generation_id = ?1 AND path = ?2",
            params![generation_id, path],
            |row| {
                Ok(IndexedFileMetadata {
                    content_hash: row.get(0)?,
                    size_bytes: row.get(1)?,
                    language: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(sql_unavailable)
}

fn mark_dependents_dirty_for_path(
    connection: &Connection,
    generation_id: &str,
    path: &str,
    reason: &str,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(reason, "dirty record reason")?;
    connection
        .execute(
            "INSERT OR IGNORE INTO dirty_records \
             (generation_id, record_kind, record_id, reason, marked_at_generation_id) \
             SELECT generation_id, record_kind, record_id, ?3, ?1 \
             FROM derived_record_dependencies \
             WHERE generation_id = ?1 AND path = ?2",
            params![generation_id, path, reason],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn record_derived_dependency(
    connection: &Connection,
    generation_id: &str,
    record_kind: &str,
    record_id: &str,
    path: &str,
    content_hash: &str,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(record_kind, "derived dependency record kind")?;
    validate_index_text_field(record_id, "derived dependency record id")?;
    validate_repo_relative_path(path)?;
    ContentHash::new(content_hash.to_string())
        .map_err(|_| invalid_record("derived dependency content hash is invalid"))?;
    connection
        .execute(
            "INSERT OR IGNORE INTO derived_record_dependencies \
             (generation_id, record_kind, record_id, path, content_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![generation_id, record_kind, record_id, path, content_hash],
        )
        .map_err(sql_unavailable)?;
    // Re-deriving a record against current indexed content clears its stale
    // marker, so a generation that had a file replaced or removed mid-build can
    // still reach a clean, activatable state instead of being permanently
    // blocked by an unclearable dirty record.
    connection
        .execute(
            "DELETE FROM dirty_records \
             WHERE generation_id = ?1 AND record_kind = ?2 AND record_id = ?3",
            params![generation_id, record_kind, record_id],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn validate_family_classification(classification: &str) -> Result<(), IndexStoreError> {
    match FamilyPrevalenceClass::parse_token(classification) {
        Ok(_) => Ok(()),
        Err(_) => Err(invalid_record("family classification is unsupported")),
    }
}

fn validate_family_evidence_covered_claims(claims: &[String]) -> Result<(), IndexStoreError> {
    if claims.is_empty() {
        return Err(invalid_record(
            "family evidence covered claims must not be empty",
        ));
    }
    let mut seen = Vec::new();
    for claim in claims {
        validate_index_text_field(claim, "family evidence covered claim")?;
        if !family_evidence_covered_claim_is_supported(claim) {
            return Err(invalid_record(
                "family evidence covered claim is unsupported",
            ));
        }
        if seen
            .iter()
            .any(|seen: &&String| seen.as_str() == claim.as_str())
        {
            return Err(invalid_record(
                "family evidence covered claims must be unique",
            ));
        }
        seen.push(claim);
    }
    Ok(())
}

fn family_evidence_covered_claims_json(claims: &[String]) -> Result<String, IndexStoreError> {
    validate_family_evidence_covered_claims(claims)?;
    serde_json::to_string(claims)
        .map_err(|_| invalid_record("family evidence covered claims JSON is invalid"))
}

fn require_family_row(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<(), IndexStoreError> {
    let count = connection
        .query_row(
            "SELECT count(*) FROM families WHERE generation_id = ?1 AND family_id = ?2",
            params![generation_id, family_id],
            |row| row.get::<_, u32>(0),
        )
        .map_err(sql_unavailable)?;
    if count == 1 {
        Ok(())
    } else {
        Err(invalid_record(
            "family-scoped record must reference a family in the same generation",
        ))
    }
}

fn require_code_unit_row(
    connection: &Connection,
    generation_id: &str,
    code_unit_id: &str,
) -> Result<(), IndexStoreError> {
    let count = connection
        .query_row(
            "SELECT count(*) FROM code_units WHERE generation_id = ?1 AND code_unit_id = ?2",
            params![generation_id, code_unit_id],
            |row| row.get::<_, u32>(0),
        )
        .map_err(sql_unavailable)?;
    if count == 1 {
        Ok(())
    } else {
        Err(invalid_record(
            "family member must reference an indexed code unit in the same generation",
        ))
    }
}

fn code_unit_evidence_bounds(
    connection: &Connection,
    generation_id: &str,
    code_unit_id: &str,
) -> Result<(String, String, i64, i64, String, i64), IndexStoreError> {
    connection
        .query_row(
            "SELECT code_units.path, code_units.content_hash, code_units.start_byte, \
                    code_units.end_byte, indexed_files.content_hash, indexed_files.size_bytes \
             FROM code_units \
             JOIN indexed_files \
               ON indexed_files.generation_id = code_units.generation_id \
              AND indexed_files.path = code_units.path \
             WHERE code_units.generation_id = ?1 \
               AND code_units.code_unit_id = ?2",
            params![generation_id, code_unit_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(sql_unavailable)?
        .ok_or_else(|| {
            invalid_record(
                "family evidence must reference an indexed code unit in the same generation",
            )
        })
}

fn query_active_repo_shape_stats(
    connection: &Connection,
    generation_id: &str,
) -> Result<ActiveRepoShapeStats, IndexStoreError> {
    let indexed_file_count = scalar_count(
        connection,
        "SELECT count(*) FROM indexed_files WHERE generation_id = ?1",
        generation_id,
        "indexed file",
    )?;
    let indexed_code_unit_count = scalar_count(
        connection,
        "SELECT count(*) FROM code_units WHERE generation_id = ?1",
        generation_id,
        "indexed code unit",
    )?;
    let semantic_fact_count = scalar_count(
        connection,
        "SELECT count(*) FROM semantic_facts WHERE generation_id = ?1",
        generation_id,
        "semantic fact",
    )?;
    let by_language = REPO_SHAPE_LANGUAGE_SCOPES
        .iter()
        .map(|scope| query_repo_shape_language_stats(connection, generation_id, scope))
        .collect::<Result<Vec<_>, _>>()?;
    let eligible_code_units = by_language
        .iter()
        .find(|stats| stats.language == "python")
        .map(|stats| stats.eligible_code_units)
        .unwrap_or(0);
    let family_member_count = by_language
        .iter()
        .find(|stats| stats.language == "python")
        .map(|stats| stats.family_member_count)
        .unwrap_or(0);
    let covered_code_units = by_language
        .iter()
        .find(|stats| stats.language == "python")
        .map(|stats| stats.covered_code_units)
        .unwrap_or(0);
    let family_count = scalar_count(
        connection,
        "SELECT count(*) FROM families WHERE generation_id = ?1",
        generation_id,
        "family",
    )?;
    Ok(ActiveRepoShapeStats {
        generation_id: generation_id.to_string(),
        indexed_file_count,
        indexed_code_unit_count,
        semantic_fact_count,
        eligible_code_units,
        family_count,
        family_member_count,
        covered_code_units,
        by_language,
    })
}

const REPO_SHAPE_LANGUAGE_SCOPES: &[&str] = &[
    "python",
    "typescript/javascript",
    "rust",
    "java",
    "csharp",
    "c/cpp",
];

fn query_repo_shape_language_stats(
    connection: &Connection,
    generation_id: &str,
    language: &str,
) -> Result<RepoShapeLanguageStats, IndexStoreError> {
    let unit_where = repo_shape_unit_where(language);
    let family_where = repo_shape_family_where(language);
    let indexed_file_where = repo_shape_indexed_file_where(language);
    let indexed_code_unit_where = repo_shape_indexed_code_unit_where(language);
    let indexed_file_count = scalar_count(
        connection,
        &format!(
            "SELECT count(*) FROM indexed_files WHERE generation_id = ?1 AND ({indexed_file_where})"
        ),
        generation_id,
        "indexed file",
    )?;
    let indexed_code_unit_count = scalar_count(
        connection,
        &format!(
            "SELECT count(*) FROM code_units WHERE generation_id = ?1 AND ({indexed_code_unit_where})"
        ),
        generation_id,
        "indexed code unit",
    )?;
    let eligible_code_units = scalar_count(
        connection,
        &format!("SELECT count(*) FROM code_units WHERE generation_id = ?1 AND ({unit_where})"),
        generation_id,
        "eligible code unit",
    )?;
    let family_count = scalar_count(
        connection,
        &format!("SELECT count(*) FROM families WHERE generation_id = ?1 AND ({family_where})"),
        generation_id,
        "family",
    )?;
    let family_member_count = scalar_count(
        connection,
        &format!(
            "SELECT count(*) \
             FROM family_members \
             JOIN code_units \
               ON code_units.generation_id = family_members.generation_id \
              AND code_units.code_unit_id = family_members.code_unit_id \
             WHERE family_members.generation_id = ?1 AND ({unit_where})"
        ),
        generation_id,
        "family member",
    )?;
    let covered_code_units = scalar_count(
        connection,
        &format!(
            "SELECT count(DISTINCT family_members.code_unit_id) \
             FROM family_members \
             JOIN code_units \
               ON code_units.generation_id = family_members.generation_id \
              AND code_units.code_unit_id = family_members.code_unit_id \
             WHERE family_members.generation_id = ?1 AND ({unit_where})"
        ),
        generation_id,
        "covered code unit",
    )?;
    Ok(RepoShapeLanguageStats {
        language: language.to_string(),
        indexed_file_count,
        indexed_code_unit_count,
        eligible_code_units,
        family_count,
        family_member_count,
        covered_code_units,
    })
}

fn repo_shape_unit_where(language: &str) -> &'static str {
    match language {
        "python" => {
            "code_units.language = 'python' AND code_units.kind IN (\
             'fastapi_route', 'pytest_test', 'pytest_fixture', 'pydantic_model', \
             'pydantic_settings', 'sqlalchemy_model', 'sqlalchemy_repository_method', \
             'django_model', 'django_url_pattern', 'django_test', 'flask_route', \
             'unittest_test_method', 'click_command', 'typer_command', 'celery_task')"
        }
        "typescript/javascript" => {
            "code_units.language IN ('typescript', 'typescript-react', 'tsx', 'javascript', \
             'javascript-react', 'jsx') AND code_units.kind IN (\
             'express_route', 'next_app_page', 'next_app_layout', 'next_route_handler', \
             'next_pages_api_route', 'next_pages_page', 'fastify_route', \
             'fastify_plugin_registration', 'prisma_query', 'prisma_transaction', \
             'drizzle_schema_table', 'drizzle_query', 'drizzle_transaction', 'zod_schema', \
             'nest_controller', 'nest_route', 'nest_injectable', 'nest_module', 'hono_route', \
             'test_suite', 'test_case')"
        }
        "rust" => {
            "code_units.language = 'rust' AND code_units.kind IN (\
             'rust_module', 'rust_inline_module', 'rust_external_module', 'rust_struct', \
             'rust_enum', 'rust_trait', 'rust_impl_block', 'rust_function', 'rust_method', \
             'rust_trait_method', 'rust_associated_function', 'rust_test_function', \
             'serde_model', 'thiserror_error_enum', 'tokio_entry', 'tokio_test', \
             'clap_parser', 'axum_route')"
        }
        "java" => {
            "code_units.language = 'java' AND code_units.kind IN (\
             'spring_mvc_route', 'spring_component', 'spring_boot_application', \
             'spring_data_repository', 'junit5_test_method', 'junit4_test_method', \
             'testng_test_method', 'jpa_entity', 'jpa_mapped_superclass', 'jpa_embeddable', \
             'jaxrs_resource_class', 'jaxrs_resource_method')"
        }
        "csharp" => {
            "code_units.language = 'csharp' AND code_units.kind IN (\
             'aspnet_controller', 'aspnet_controller_action', 'aspnet_minimal_api_route', \
             'efcore_db_context', 'efcore_entity_set', 'xunit_test_method', \
             'nunit_test_method', 'mstest_test_method')"
        }
        "c/cpp" => {
            "code_units.language IN ('c', 'cpp') AND code_units.kind IN (\
             'gtest_test_case', 'gtest_test_fixture', 'catch2_test_case', \
             'doctest_test_case', 'boost_test_case', 'boost_test_suite')"
        }
        _ => "0",
    }
}

fn repo_shape_indexed_file_where(language: &str) -> &'static str {
    match language {
        "python" => "indexed_files.language IN ('python', 'python-config')",
        "typescript/javascript" => {
            "indexed_files.language IN ('typescript', 'typescript-react', 'tsx', 'javascript', \
             'javascript-react', 'jsx', 'tsjs-config')"
        }
        "rust" => "indexed_files.language IN ('rust', 'rust-config')",
        "java" => "indexed_files.language = 'java'",
        "csharp" => "indexed_files.language = 'csharp'",
        "c/cpp" => "indexed_files.language IN ('c', 'cpp', 'cpp-config')",
        _ => "0",
    }
}

fn repo_shape_indexed_code_unit_where(language: &str) -> &'static str {
    match language {
        "python" => "code_units.language = 'python'",
        "typescript/javascript" => {
            "code_units.language IN ('typescript', 'typescript-react', 'tsx', 'javascript', \
             'javascript-react', 'jsx')"
        }
        "rust" => "code_units.language = 'rust'",
        "java" => "code_units.language = 'java'",
        "csharp" => "code_units.language = 'csharp'",
        "c/cpp" => "code_units.language IN ('c', 'cpp')",
        _ => "0",
    }
}

fn repo_shape_family_where(language: &str) -> &'static str {
    match language {
        "python" => "families.family_id GLOB 'family:python:*'",
        "typescript/javascript" => {
            "families.family_id GLOB 'family:typescript:*' \
             OR families.family_id GLOB 'family:typescript_react:*' \
             OR families.family_id GLOB 'family:tsx:*' \
             OR families.family_id GLOB 'family:javascript:*' \
             OR families.family_id GLOB 'family:javascript_react:*' \
             OR families.family_id GLOB 'family:jsx:*'"
        }
        "rust" => "families.family_id GLOB 'family:rust:*'",
        "java" => "families.family_id GLOB 'family:java:*'",
        "csharp" => "families.family_id GLOB 'family:csharp:*'",
        "c/cpp" => {
            "(families.family_id GLOB 'family:c:*' OR families.family_id GLOB 'family:cpp:*')"
        }
        _ => "0",
    }
}

fn scalar_count(
    connection: &Connection,
    sql: &str,
    generation_id: &str,
    label: &'static str,
) -> Result<usize, IndexStoreError> {
    let count = connection
        .query_row(sql, params![generation_id], |row| row.get::<_, i64>(0))
        .map_err(sql_unavailable)?;
    usize::try_from(count)
        .map_err(|_| IndexStoreError::InvalidState(format!("{label} count is invalid")))
}

fn query_family_summaries(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFamilySummaryRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT families.family_id, families.classification, count(family_members.code_unit_id), \
                    families.eligible_peer_count, families.supported_member_count, \
                    families.coverage_ratio, families.competing_ready_family_count, \
                    families.largest_competing_support, families.blocked_peer_count, \
                    families.unsupported_peer_count, families.classification_reason \
             FROM families \
             LEFT JOIN family_members \
               ON family_members.generation_id = families.generation_id \
              AND family_members.family_id = families.family_id \
             WHERE families.generation_id = ?1 \
             GROUP BY families.family_id, families.classification \
             ORDER BY families.family_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<f64>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut summaries = Vec::new();
    for row in rows {
        let (
            family_id,
            classification,
            support,
            eligible,
            supported,
            coverage,
            competing,
            largest,
            blocked,
            unsupported,
            reason,
        ) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family id", &family_id)?;
        validate_stored_family_classification(&classification)?;
        let prevalence = stored_family_prevalence(StoredFamilyPrevalenceColumns {
            eligible_peer_count: eligible,
            supported_member_count: supported,
            coverage_ratio: coverage,
            competing_ready_family_count: competing,
            largest_competing_support: largest,
            blocked_peer_count: blocked,
            unsupported_peer_count: unsupported,
            classification_reason: reason,
        })?;
        summaries.push(IndexedFamilySummaryRecord {
            family_id,
            classification,
            support: usize::try_from(support).map_err(|_| {
                IndexStoreError::InvalidState("stored family support is invalid".to_string())
            })?,
            prevalence,
        });
    }
    Ok(summaries)
}

fn query_limited_family_candidates<F>(
    connection: &Connection,
    generation_id: &str,
    limit: usize,
    query: F,
) -> Result<ActiveFamilyCandidates, IndexStoreError>
where
    F: FnOnce(
        &Connection,
        &str,
        usize,
    ) -> Result<Vec<IndexedFamilyCandidateRecord>, IndexStoreError>,
{
    let limit = limit.max(1);
    let mut candidates = query(connection, generation_id, limit.saturating_add(1))?;
    let truncated = candidates.len() > limit;
    if truncated {
        candidates.truncate(limit);
    }
    Ok(ActiveFamilyCandidates {
        generation_id: generation_id.to_string(),
        candidates,
        truncated,
    })
}

fn query_family_candidates_by_member(
    connection: &Connection,
    generation_id: &str,
    code_unit_id: &str,
) -> Result<Vec<IndexedFamilyCandidateRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT family_id \
             FROM family_members \
             WHERE generation_id = ?1 AND code_unit_id = ?2 \
             ORDER BY family_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    query_candidate_rows(&mut statement, params![generation_id, code_unit_id])
}

fn query_family_candidates_by_role(
    connection: &Connection,
    generation_id: &str,
    role: &str,
    row_limit: usize,
) -> Result<Vec<IndexedFamilyCandidateRecord>, IndexStoreError> {
    let row_limit = i64::try_from(row_limit).map_err(|_| {
        IndexStoreError::InvalidRecord("candidate row limit is invalid".to_string())
    })?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT family_id \
             FROM family_members \
             WHERE generation_id = ?1 AND role = ?2 \
             ORDER BY family_id COLLATE BINARY \
             LIMIT ?3",
        )
        .map_err(sql_unavailable)?;
    query_candidate_rows(&mut statement, params![generation_id, role, row_limit])
}

fn query_family_candidates_by_evidence_path(
    connection: &Connection,
    generation_id: &str,
    path: &str,
    row_limit: usize,
) -> Result<Vec<IndexedFamilyCandidateRecord>, IndexStoreError> {
    let row_limit = i64::try_from(row_limit).map_err(|_| {
        IndexStoreError::InvalidRecord("candidate row limit is invalid".to_string())
    })?;
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT family_id \
             FROM evidence \
             WHERE generation_id = ?1 \
               AND family_id IS NOT NULL \
               AND (path = ?2 \
                    OR (length(path) > length(?2) \
                        AND substr(path, length(path) - length(?2), 1) = '/' \
                        AND substr(path, length(path) - length(?2) + 1) = ?2)) \
             ORDER BY family_id COLLATE BINARY \
             LIMIT ?3",
        )
        .map_err(sql_unavailable)?;
    query_candidate_rows(&mut statement, params![generation_id, path, row_limit])
}

fn query_candidate_rows<P>(
    statement: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<IndexedFamilyCandidateRecord>, IndexStoreError>
where
    P: rusqlite::Params,
{
    let rows = statement
        .query_map(params, |row| row.get::<_, String>(0))
        .map_err(sql_unavailable)?;
    let mut candidates = Vec::new();
    for row in rows {
        let family_id = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family id", &family_id)?;
        candidates.push(IndexedFamilyCandidateRecord { family_id });
    }
    Ok(candidates)
}

fn query_families(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFamilyRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT family_id, classification, eligible_peer_count, supported_member_count, \
                    coverage_ratio, competing_ready_family_count, largest_competing_support, \
                    blocked_peer_count, unsupported_peer_count, classification_reason \
             FROM families \
             WHERE generation_id = ?1 \
             ORDER BY family_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut families = Vec::new();
    for row in rows {
        let (
            family_id,
            classification,
            eligible,
            supported,
            coverage,
            competing,
            largest,
            blocked,
            unsupported,
            reason,
        ) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family id", &family_id)?;
        validate_stored_family_classification(&classification)?;
        let prevalence = stored_family_prevalence(StoredFamilyPrevalenceColumns {
            eligible_peer_count: eligible,
            supported_member_count: supported,
            coverage_ratio: coverage,
            competing_ready_family_count: competing,
            largest_competing_support: largest,
            blocked_peer_count: blocked,
            unsupported_peer_count: unsupported,
            classification_reason: reason,
        })?;
        families.push(IndexedFamilyRecord {
            family_id,
            classification,
            prevalence,
        });
    }
    Ok(families)
}

fn query_family(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<Option<IndexedFamilyRecord>, IndexStoreError> {
    let record = connection
        .query_row(
            "SELECT family_id, classification, eligible_peer_count, supported_member_count, \
                    coverage_ratio, competing_ready_family_count, largest_competing_support, \
                    blocked_peer_count, unsupported_peer_count, classification_reason \
             FROM families \
             WHERE generation_id = ?1 AND family_id = ?2",
            params![generation_id, family_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()
        .map_err(sql_unavailable)?;
    let Some((
        family_id,
        classification,
        eligible,
        supported,
        coverage,
        competing,
        largest,
        blocked,
        unsupported,
        reason,
    )) = record
    else {
        return Ok(None);
    };
    validate_stored_semantic_text_field("stored family id", &family_id)?;
    validate_stored_family_classification(&classification)?;
    let prevalence = stored_family_prevalence(StoredFamilyPrevalenceColumns {
        eligible_peer_count: eligible,
        supported_member_count: supported,
        coverage_ratio: coverage,
        competing_ready_family_count: competing,
        largest_competing_support: largest,
        blocked_peer_count: blocked,
        unsupported_peer_count: unsupported,
        classification_reason: reason,
    })?;
    Ok(Some(IndexedFamilyRecord {
        family_id,
        classification,
        prevalence,
    }))
}

fn query_family_members(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<Vec<IndexedFamilyMemberRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT family_members.family_id, family_members.code_unit_id, family_members.role, code_units.path \
             FROM family_members \
             JOIN code_units \
               ON code_units.generation_id = family_members.generation_id \
              AND code_units.code_unit_id = family_members.code_unit_id \
             WHERE family_members.generation_id = ?1 AND family_members.family_id = ?2 \
             ORDER BY code_units.path COLLATE BINARY, code_units.start_byte, \
                      code_units.end_byte, family_members.code_unit_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id, family_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut members = Vec::new();
    for row in rows {
        let (family_id, code_unit_id, role, path) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family member family id", &family_id)?;
        validate_stored_code_unit_id(&code_unit_id, &path)?;
        validate_stored_semantic_text_field("stored family member role", &role)?;
        members.push(IndexedFamilyMemberRecord {
            family_id,
            code_unit_id,
            role,
        });
    }
    Ok(members)
}

fn query_variation_slots(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<Vec<IndexedVariationSlotRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT family_id, slot_id, description \
             FROM variation_slots \
             WHERE generation_id = ?1 AND family_id = ?2 \
             ORDER BY slot_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id, family_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut slots = Vec::new();
    for row in rows {
        let (family_id, slot_id, description) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored variation slot family id", &family_id)?;
        validate_stored_semantic_text_field("stored variation slot id", &slot_id)?;
        validate_stored_semantic_text_field("stored variation slot description", &description)?;
        slots.push(IndexedVariationSlotRecord {
            family_id,
            slot_id,
            description,
        });
    }
    Ok(slots)
}

fn query_family_evidence(
    connection: &Connection,
    generation_id: &str,
    family_id: &str,
) -> Result<Vec<IndexedFamilyEvidenceRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT evidence.evidence_id, evidence.family_id, evidence.code_unit_id, evidence.path, \
                    evidence.content_hash, evidence.start_byte, evidence.end_byte, evidence.note, \
                    evidence.covered_claims_json, code_units.path, code_units.content_hash, \
                    code_units.start_byte, code_units.end_byte, indexed_files.content_hash, \
                    indexed_files.size_bytes \
             FROM evidence \
             JOIN code_units \
               ON code_units.generation_id = evidence.generation_id \
              AND code_units.code_unit_id = evidence.code_unit_id \
             JOIN indexed_files \
               ON indexed_files.generation_id = evidence.generation_id \
              AND indexed_files.path = evidence.path \
             WHERE evidence.generation_id = ?1 AND evidence.family_id = ?2 \
             ORDER BY evidence.path COLLATE BINARY, evidence.start_byte, evidence.end_byte, \
                      evidence.code_unit_id COLLATE BINARY, evidence.evidence_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id, family_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, i64>(14)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut evidence_records = Vec::new();
    for row in rows {
        let (
            evidence_id,
            row_family_id,
            code_unit_id,
            path,
            content_hash,
            start_byte,
            end_byte,
            note,
            covered_claims_json,
            unit_path,
            unit_hash,
            unit_start_byte,
            unit_end_byte,
            file_hash,
            file_size,
        ) = row.map_err(sql_unavailable)?;
        let Some(row_family_id) = row_family_id else {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence is not linked to a family".to_string(),
            ));
        };
        let Some(code_unit_id) = code_unit_id else {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence is not linked to a code unit".to_string(),
            ));
        };
        validate_stored_semantic_text_field("stored family evidence id", &evidence_id)?;
        validate_stored_semantic_text_field("stored family evidence family id", &row_family_id)?;
        validate_stored_semantic_text_field("stored family evidence note", &note)?;
        let covered_claims = stored_family_evidence_covered_claims(&covered_claims_json)?;
        validate_stored_repo_relative_path(&path, "stored family evidence path")?;
        if path != unit_path {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence path does not match code unit".to_string(),
            ));
        }
        validate_stored_code_unit_id(&code_unit_id, &path)?;
        if content_hash != unit_hash || content_hash != file_hash {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence content hash does not match indexed evidence".to_string(),
            ));
        }
        let start_byte = usize::try_from(start_byte).map_err(|_| {
            IndexStoreError::InvalidState(
                "stored family evidence start byte is invalid".to_string(),
            )
        })?;
        let end_byte = usize::try_from(end_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored family evidence end byte is invalid".to_string())
        })?;
        let unit_start_byte = usize::try_from(unit_start_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit start byte is invalid".to_string())
        })?;
        let unit_end_byte = usize::try_from(unit_end_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit end byte is invalid".to_string())
        })?;
        let file_size = usize::try_from(file_size).map_err(|_| {
            IndexStoreError::InvalidState("stored indexed file size is invalid".to_string())
        })?;
        if start_byte > end_byte
            || start_byte < unit_start_byte
            || end_byte > unit_end_byte
            || end_byte > file_size
        {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence range is invalid".to_string(),
            ));
        }
        evidence_records.push(IndexedFamilyEvidenceRecord {
            evidence_id,
            family_id: row_family_id,
            code_unit_id,
            covered_claims,
            path,
            content_hash: ContentHash::new(content_hash).map_err(|_| {
                IndexStoreError::InvalidState(
                    "stored family evidence content hash is invalid".to_string(),
                )
            })?,
            start_byte,
            end_byte,
            note,
        });
    }
    Ok(evidence_records)
}

fn query_family_evidence_projection(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFamilyEvidenceProjectionRecord>, IndexStoreError> {
    // Bounded projection: a single sorted scan of the active generation's family
    // evidence, selecting only the three columns list-level freshness needs. The
    // expected hash is read directly from the evidence row (the same value the
    // single-family freshness check compares against), so no per-family joins or
    // re-reads are required.
    let mut statement = connection
        .prepare(
            "SELECT family_id, path, content_hash \
             FROM evidence \
             WHERE generation_id = ?1 AND family_id IS NOT NULL \
             ORDER BY family_id COLLATE BINARY, path COLLATE BINARY, content_hash COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut projection = Vec::new();
    for row in rows {
        let (family_id, path, content_hash) = row.map_err(sql_unavailable)?;
        let Some(family_id) = family_id else {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence is not linked to a family".to_string(),
            ));
        };
        validate_stored_semantic_text_field("stored family evidence family id", &family_id)?;
        validate_stored_repo_relative_path(&path, "stored family evidence path")?;
        projection.push(IndexedFamilyEvidenceProjectionRecord {
            family_id,
            path,
            content_hash: ContentHash::new(content_hash).map_err(|_| {
                IndexStoreError::InvalidState(
                    "stored family evidence content hash is invalid".to_string(),
                )
            })?,
        });
    }
    Ok(projection)
}

fn query_family_search_summaries(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFamilySearchSummaryRecord>, IndexStoreError> {
    // One bounded, source-free projection of every active family's searchable
    // metadata. Language, code-unit kind, and framework role are uniform across a
    // family's members by construction (they are part of the family key), so
    // aggregating them with MIN yields their single deterministic value while the
    // join stays a single statement. Support counts distinct members; a
    // correlated subquery collects the family's distinct evidence paths joined by
    // newlines (a separator no repo-relative path may contain) so in-memory
    // shaping can split them without ambiguity.
    let mut statement = connection
        .prepare(
            "SELECT families.family_id, families.classification, families.eligible_peer_count, \
                    families.supported_member_count, families.coverage_ratio, \
                    families.competing_ready_family_count, families.largest_competing_support, \
                    families.blocked_peer_count, families.unsupported_peer_count, \
                    families.classification_reason, \
                    MIN(code_units.language), MIN(code_units.kind), MIN(family_members.role), \
                    COUNT(DISTINCT family_members.code_unit_id), \
                    ( \
                        SELECT group_concat(component, char(10)) FROM ( \
                            SELECT DISTINCT evidence.path AS component \
                            FROM evidence \
                            WHERE evidence.generation_id = ?1 \
                              AND evidence.family_id = families.family_id \
                            ORDER BY evidence.path COLLATE BINARY \
                        ) \
                    ) \
             FROM families \
             JOIN family_members \
               ON family_members.generation_id = families.generation_id \
              AND family_members.family_id = families.family_id \
             JOIN code_units \
               ON code_units.generation_id = family_members.generation_id \
              AND code_units.code_unit_id = family_members.code_unit_id \
             WHERE families.generation_id = ?1 \
             GROUP BY families.family_id, families.classification, \
                      families.eligible_peer_count, families.supported_member_count, \
                      families.coverage_ratio, families.competing_ready_family_count, \
                      families.largest_competing_support, families.blocked_peer_count, \
                      families.unsupported_peer_count, families.classification_reason \
             ORDER BY families.family_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, Option<String>>(14)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut summaries = Vec::new();
    for row in rows {
        let (
            family_id,
            classification,
            eligible,
            supported,
            coverage,
            competing,
            largest,
            blocked,
            unsupported,
            reason,
            language,
            code_unit_kind,
            framework_role,
            support,
            evidence_paths,
        ) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family id", &family_id)?;
        validate_stored_family_classification(&classification)?;
        validate_stored_semantic_text_field("stored family language", &language)?;
        validate_stored_semantic_text_field("stored family code unit kind", &code_unit_kind)?;
        validate_stored_semantic_text_field("stored family role", &framework_role)?;
        let prevalence = stored_family_prevalence(StoredFamilyPrevalenceColumns {
            eligible_peer_count: eligible,
            supported_member_count: supported,
            coverage_ratio: coverage,
            competing_ready_family_count: competing,
            largest_competing_support: largest,
            blocked_peer_count: blocked,
            unsupported_peer_count: unsupported,
            classification_reason: reason,
        })?;
        let support = usize::try_from(support).map_err(|_| {
            IndexStoreError::InvalidState("stored family support is invalid".to_string())
        })?;
        let evidence_path_components =
            stored_family_search_path_components(evidence_paths.as_deref())?;
        summaries.push(IndexedFamilySearchSummaryRecord {
            family_id,
            language,
            code_unit_kind,
            framework_role,
            classification,
            support,
            prevalence,
            evidence_path_components,
        });
    }
    Ok(summaries)
}

/// Shape the newline-joined distinct evidence paths of one family into a bounded,
/// deterministically ordered set of repo-relative path segments (ancestor
/// directory components and basenames). Never emits absolute paths or source
/// text: every source path is validated as repo-relative before splitting.
fn stored_family_search_path_components(
    evidence_paths: Option<&str>,
) -> Result<Vec<String>, IndexStoreError> {
    let mut components = std::collections::BTreeSet::new();
    for path in evidence_paths
        .into_iter()
        .flat_map(|joined| joined.split('\n'))
    {
        if path.is_empty() {
            continue;
        }
        validate_stored_repo_relative_path(path, "stored family evidence path")?;
        for segment in path.split('/') {
            if !segment.is_empty() {
                components.insert(segment.to_string());
            }
        }
    }
    Ok(components
        .into_iter()
        .take(FAMILY_SEARCH_PATH_COMPONENT_CAP)
        .collect())
}

fn query_indexed_files(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFileRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT path, content_hash, size_bytes, language \
             FROM indexed_files \
             WHERE generation_id = ?1 \
             ORDER BY path COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            let content_hash = row.get::<_, String>(1)?;
            Ok((
                row.get::<_, String>(0)?,
                content_hash,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut files = Vec::new();
    for row in rows {
        let (path, content_hash, size_bytes, language) = row.map_err(sql_unavailable)?;
        validate_stored_repo_relative_path(&path, "stored indexed file path")?;
        validate_stored_non_empty_text(&language, "stored indexed file language")?;
        files.push(IndexedFileRecord {
            path,
            content_hash: ContentHash::new(content_hash).map_err(|_| {
                IndexStoreError::InvalidState(
                    "stored indexed file content hash is invalid".to_string(),
                )
            })?,
            size_bytes: u64::try_from(size_bytes).map_err(|_| {
                IndexStoreError::InvalidState("stored indexed file size is invalid".to_string())
            })?,
            language,
        });
    }
    Ok(files)
}

fn query_code_units(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedCodeUnitRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT code_units.code_unit_id, code_units.path, code_units.language, \
                    code_units.kind, code_units.start_byte, code_units.end_byte, \
                    code_units.content_hash, indexed_files.content_hash, indexed_files.size_bytes \
             FROM code_units \
             JOIN indexed_files \
               ON indexed_files.generation_id = code_units.generation_id \
              AND indexed_files.path = code_units.path \
             WHERE code_units.generation_id = ?1 \
             ORDER BY code_units.path COLLATE BINARY, code_units.start_byte, \
                      code_units.end_byte, code_units.kind COLLATE BINARY, \
                      code_units.code_unit_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            let content_hash = row.get::<_, String>(6)?;
            let file_hash = row.get::<_, String>(7)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                content_hash,
                file_hash,
                row.get::<_, i64>(8)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut units = Vec::new();
    for row in rows {
        let (id, path, language, kind, start_byte, end_byte, content_hash, file_hash, file_size) =
            row.map_err(sql_unavailable)?;
        validate_stored_repo_relative_path(&path, "stored code unit path")?;
        validate_stored_code_unit_id(&id, &path)?;
        validate_stored_non_empty_text(&language, "stored code unit language")?;
        validate_stored_non_empty_text(&kind, "stored code unit kind")?;
        if content_hash != file_hash {
            return Err(IndexStoreError::InvalidState(
                "stored code unit content hash does not match indexed file".to_string(),
            ));
        }
        let start_byte = usize::try_from(start_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit start byte is invalid".to_string())
        })?;
        let end_byte = usize::try_from(end_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit end byte is invalid".to_string())
        })?;
        let file_size = usize::try_from(file_size).map_err(|_| {
            IndexStoreError::InvalidState("stored indexed file size is invalid".to_string())
        })?;
        if end_byte < start_byte || end_byte > file_size {
            return Err(IndexStoreError::InvalidState(
                "stored code unit range is invalid".to_string(),
            ));
        }
        units.push(IndexedCodeUnitRecord {
            id,
            path,
            language,
            kind,
            start_byte,
            end_byte,
            content_hash: ContentHash::new(content_hash).map_err(|_| {
                IndexStoreError::InvalidState(
                    "stored code unit content hash is invalid".to_string(),
                )
            })?,
        });
    }
    Ok(units)
}

fn query_semantic_facts(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedSemanticFactRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT semantic_facts.fact_id, semantic_facts.kind, semantic_facts.subject, \
                    semantic_facts.target, semantic_facts.certainty, \
                    semantic_facts.origin_engine, semantic_facts.origin_engine_version, \
                    semantic_facts.origin_method, semantic_facts.assumptions_json, \
                    semantic_facts.evidence_id, evidence.code_unit_id, evidence.path, \
                    evidence.content_hash, evidence.start_byte, evidence.end_byte, \
                    evidence.note, code_units.path, code_units.content_hash, \
                    code_units.start_byte, code_units.end_byte, indexed_files.content_hash, \
                    indexed_files.size_bytes \
             FROM semantic_facts \
             JOIN evidence \
               ON evidence.generation_id = semantic_facts.generation_id \
              AND evidence.evidence_id = semantic_facts.evidence_id \
             JOIN code_units \
               ON code_units.generation_id = evidence.generation_id \
              AND code_units.code_unit_id = evidence.code_unit_id \
             JOIN indexed_files \
               ON indexed_files.generation_id = evidence.generation_id \
              AND indexed_files.path = evidence.path \
             WHERE semantic_facts.generation_id = ?1 \
             ORDER BY evidence.path COLLATE BINARY, evidence.start_byte, \
                      evidence.end_byte, evidence.code_unit_id COLLATE BINARY, \
                      semantic_facts.kind COLLATE BINARY, semantic_facts.subject COLLATE BINARY, \
                      semantic_facts.fact_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, String>(17)?,
                row.get::<_, i64>(18)?,
                row.get::<_, i64>(19)?,
                row.get::<_, String>(20)?,
                row.get::<_, i64>(21)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut facts = Vec::new();
    for row in rows {
        let (
            fact_id,
            kind,
            subject,
            target,
            certainty,
            origin_engine,
            origin_engine_version,
            origin_method,
            assumptions_json,
            evidence_id,
            code_unit_id,
            path,
            content_hash,
            start_byte,
            end_byte,
            note,
            unit_path,
            unit_hash,
            unit_start_byte,
            unit_end_byte,
            file_hash,
            file_size,
        ) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored semantic fact id", &fact_id)?;
        validate_stored_semantic_text_field("stored semantic fact subject", &subject)?;
        validate_stored_semantic_text_field("stored semantic fact origin engine", &origin_engine)?;
        validate_stored_semantic_text_field(
            "stored semantic fact origin engine version",
            &origin_engine_version,
        )?;
        validate_stored_semantic_text_field("stored semantic fact origin method", &origin_method)?;
        validate_stored_semantic_text_field("stored semantic fact evidence id", &evidence_id)?;
        validate_stored_semantic_text_field("stored semantic fact note", &note)?;
        if let Some(target) = &target {
            validate_stored_semantic_text_field("stored semantic fact target", target)?;
        }
        SemanticFactKind::parse_protocol_str(&kind).map_err(|_| {
            IndexStoreError::InvalidState("stored semantic fact kind is invalid".to_string())
        })?;
        FactCertainty::parse_protocol_str(&certainty).map_err(|_| {
            IndexStoreError::InvalidState("stored semantic fact certainty is invalid".to_string())
        })?;
        let assumptions: Vec<String> = serde_json::from_str(&assumptions_json).map_err(|_| {
            IndexStoreError::InvalidState(
                "stored semantic fact assumptions JSON is invalid".to_string(),
            )
        })?;
        for assumption in &assumptions {
            validate_stored_semantic_text_field("stored semantic fact assumption", assumption)?;
        }
        validate_stored_repo_relative_path(&path, "stored semantic fact evidence path")?;
        if path != unit_path {
            return Err(IndexStoreError::InvalidState(
                "stored semantic fact evidence path does not match code unit".to_string(),
            ));
        }
        validate_stored_code_unit_id(&code_unit_id, &path)?;
        if content_hash != unit_hash || content_hash != file_hash {
            return Err(IndexStoreError::InvalidState(
                "stored semantic fact content hash does not match indexed evidence".to_string(),
            ));
        }
        let start_byte = usize::try_from(start_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored semantic fact start byte is invalid".to_string())
        })?;
        let end_byte = usize::try_from(end_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored semantic fact end byte is invalid".to_string())
        })?;
        let unit_start_byte = usize::try_from(unit_start_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit start byte is invalid".to_string())
        })?;
        let unit_end_byte = usize::try_from(unit_end_byte).map_err(|_| {
            IndexStoreError::InvalidState("stored code unit end byte is invalid".to_string())
        })?;
        let file_size = usize::try_from(file_size).map_err(|_| {
            IndexStoreError::InvalidState("stored indexed file size is invalid".to_string())
        })?;
        if start_byte > end_byte
            || start_byte < unit_start_byte
            || end_byte > unit_end_byte
            || end_byte > file_size
        {
            return Err(IndexStoreError::InvalidState(
                "stored semantic fact range is invalid".to_string(),
            ));
        }
        facts.push(IndexedSemanticFactRecord {
            fact_id,
            kind,
            subject,
            target,
            certainty,
            origin_engine,
            origin_engine_version,
            origin_method,
            assumptions,
            evidence_id,
            code_unit_id,
            path,
            content_hash: ContentHash::new(content_hash).map_err(|_| {
                IndexStoreError::InvalidState(
                    "stored semantic fact content hash is invalid".to_string(),
                )
            })?,
            start_byte,
            end_byte,
            note,
        });
    }
    Ok(facts)
}

fn query_ir_graph(
    connection: &Connection,
    generation_id: &str,
) -> Result<(Vec<IndexedIrNodeRecord>, Vec<IndexedIrEdgeRecord>), IndexStoreError> {
    let mut node_statement = connection
        .prepare(
            "SELECT ir_nodes.node_id, ir_nodes.code_unit_id, ir_nodes.kind, \
                    ir_nodes.payload_json, code_units.path \
             FROM ir_nodes \
             JOIN code_units \
               ON code_units.generation_id = ir_nodes.generation_id \
              AND code_units.code_unit_id = ir_nodes.code_unit_id \
             WHERE ir_nodes.generation_id = ?1 \
             ORDER BY ir_nodes.node_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let node_rows = node_statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut nodes = Vec::new();
    for row in node_rows {
        let (id, code_unit_id, kind, payload_json, path) = row.map_err(sql_unavailable)?;
        validate_stored_code_unit_id(&code_unit_id, &path)?;
        validate_stored_ir_node_id(&id, &code_unit_id)?;
        IrNodeKind::parse_protocol_str(&kind).map_err(|_| {
            IndexStoreError::InvalidState("stored IR node kind is invalid".to_string())
        })?;
        validate_stored_empty_object_payload(&payload_json, "stored IR node payload")?;
        nodes.push(IndexedIrNodeRecord {
            id,
            code_unit_id,
            kind,
            payload_json,
        });
    }

    let mut edge_statement = connection
        .prepare(
            "SELECT from_node_id, to_node_id, label \
             FROM ir_edges \
             WHERE generation_id = ?1 \
             ORDER BY from_node_id COLLATE BINARY, to_node_id COLLATE BINARY, \
                      label COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let edge_rows = edge_statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut edges = Vec::new();
    for row in edge_rows {
        let (from_node_id, to_node_id, label) = row.map_err(sql_unavailable)?;
        validate_stored_non_empty_text(&from_node_id, "stored IR edge from node id")?;
        validate_stored_non_empty_text(&to_node_id, "stored IR edge to node id")?;
        if from_node_id == to_node_id {
            return Err(IndexStoreError::InvalidState(
                "stored IR edge points to itself".to_string(),
            ));
        }
        IrEdgeLabel::parse_protocol_str(&label).map_err(|_| {
            IndexStoreError::InvalidState("stored IR edge label is invalid".to_string())
        })?;
        edges.push(IndexedIrEdgeRecord {
            from_node_id,
            to_node_id,
            label,
        });
    }
    Ok((nodes, edges))
}

fn apply_pragmas(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(Duration::from_millis(5_000))?;
    let _: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    connection.execute_batch(
        "PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA temp_store=MEMORY;",
    )
}

fn apply_read_pragmas(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(Duration::from_millis(5_000))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA temp_store=MEMORY;",
    )
}

fn run_post_commit_maintenance(
    connection: &Connection,
) -> Result<(u64, u64, u64), IndexStoreError> {
    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(sql_unavailable)?;
    let (busy, log_frames, checkpointed_frames) = connection
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    Ok((
        nonnegative_pragma_count(busy, "WAL checkpoint busy count")?,
        nonnegative_pragma_count(log_frames, "WAL checkpoint log frame count")?,
        nonnegative_pragma_count(checkpointed_frames, "WAL checkpointed frame count")?,
    ))
}

/// Checkpoint committed WAL frames back into the main database without blocking
/// on readers. Used at pipeline phase boundaries to bound WAL growth; it touches
/// only already-committed data, so callers treat a failure as non-fatal.
fn run_passive_wal_checkpoint(connection: &Connection) -> Result<(), IndexStoreError> {
    connection
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(sql_unavailable)?;
    Ok(())
}

fn run_compaction_maintenance(connection: &Connection) -> Result<(u64, u64, u64), IndexStoreError> {
    connection
        .execute_batch("PRAGMA optimize; VACUUM;")
        .map_err(sql_unavailable)?;
    let (busy, log_frames, checkpointed_frames) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let busy = nonnegative_pragma_count(busy, "WAL checkpoint busy count")?;
    if busy != 0 {
        return Err(IndexStoreError::InvalidState(
            "SQLite WAL checkpoint was busy during compact; retry repogrammar compact".to_string(),
        ));
    }
    Ok((
        busy,
        nonnegative_pragma_count(log_frames, "WAL checkpoint log frame count")?,
        nonnegative_pragma_count(checkpointed_frames, "WAL checkpointed frame count")?,
    ))
}

fn nonnegative_pragma_count(value: i64, label: &'static str) -> Result<u64, IndexStoreError> {
    u64::try_from(value).map_err(|_| IndexStoreError::InvalidState(format!("{label} is invalid")))
}

fn open_connection(
    path: PathBuf,
    missing_database: MissingDatabase,
) -> Result<Connection, IndexStoreError> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    if missing_database == MissingDatabase::Allowed {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    Connection::open_with_flags(path, flags).map_err(sql_unavailable)
}

fn open_read_only_connection(path: PathBuf) -> Result<Connection, IndexStoreError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    Connection::open_with_flags(path, flags).map_err(sql_unavailable)
}

fn apply_migrations(connection: &Connection) -> Result<(), IndexStoreError> {
    connection
        .execute_batch(INITIAL_SCHEMA)
        .map_err(sql_unavailable)?;
    // Refuse to write into a database stamped with a newer schema version than
    // this build understands, so an older binary cannot corrupt a newer index.
    // INITIAL_SCHEMA created schema_migrations above, so the version is readable.
    if let Some(version) = stored_schema_version(connection)? {
        if version > STORAGE_SCHEMA_VERSION {
            return Err(IndexStoreError::InvalidState(
                "repository index schema is newer than this repogrammar build supports; upgrade repogrammar"
                    .to_string(),
            ));
        }
    }
    connection
        .execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) \
             VALUES (?1, 'family_constraint_profiles_v9', datetime('now'))",
            params![STORAGE_SCHEMA_VERSION],
        )
        .map_err(sql_unavailable)?;
    connection
        .execute_batch("PRAGMA optimize;")
        .map_err(sql_unavailable)?;
    Ok(())
}

/// Read-path storage schema gate. A stored version older than the current build
/// is an explicit, typed condition whose recovery is a full rebuild via
/// `repogrammar resync`; the recovery guidance is attached in the application
/// layer. A missing or newer version stays a generic unsupported-state error.
fn schema_version_read_gate(stored: Option<u32>) -> Result<(), IndexStoreError> {
    match stored {
        Some(version) if version == STORAGE_SCHEMA_VERSION => Ok(()),
        Some(version) if version < STORAGE_SCHEMA_VERSION => {
            Err(IndexStoreError::SchemaVersionOutdated(
                "repository index schema predates this repogrammar build and must be rebuilt"
                    .to_string(),
            ))
        }
        _ => Err(IndexStoreError::InvalidState(
            "storage schema version is missing or unsupported".to_string(),
        )),
    }
}

fn validate_generation_for_read(
    connection: &Connection,
    generation_id: &str,
) -> Result<(), IndexStoreError> {
    let inspection = inspect_connection(connection, Some(generation_id))?;
    schema_version_read_gate(inspection.schema_version)?;
    if inspection.integrity_check.as_deref() != Some("ok") {
        return Err(IndexStoreError::InvalidState(
            "SQLite integrity check failed".to_string(),
        ));
    }
    if !required_schema_is_present(connection)? {
        return Err(IndexStoreError::InvalidState(
            "required storage schema is missing or malformed".to_string(),
        ));
    }
    if family_evidence_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "family evidence is inconsistent with indexed code units".to_string(),
        ));
    }
    if semantic_evidence_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "semantic fact evidence is inconsistent with indexed code units".to_string(),
        ));
    }
    if derived_dependency_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "derived record dependencies are inconsistent with indexed files".to_string(),
        ));
    }
    if dirty_record_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "dirty records cannot support an active generation".to_string(),
        ));
    }
    if ir_graph_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "IR graph is inconsistent with indexed code units".to_string(),
        ));
    }
    Ok(())
}

fn stored_schema_version(connection: &Connection) -> Result<Option<u32>, IndexStoreError> {
    connection
        .query_row("SELECT max(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<u32>>(0)
        })
        .map_err(sql_unavailable)
}

/// Validation run on every full-snapshot active read. It keeps the per-table
/// structural violation scans (which are load-bearing for tamper/consistency
/// detection) but drops the expensive `PRAGMA integrity_check` — a full-database
/// B-tree verification whose cost scales with the whole database, not the
/// result. The integrity check still runs at generation activation,
/// `inspect`/doctor, and compaction.
fn validate_active_generation_for_read(
    connection: &Connection,
    generation_id: &str,
) -> Result<(), IndexStoreError> {
    schema_version_read_gate(stored_schema_version(connection)?)?;
    if !required_schema_is_present(connection)? {
        return Err(IndexStoreError::InvalidState(
            "required storage schema is missing or malformed".to_string(),
        ));
    }
    if family_evidence_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "family evidence is inconsistent with indexed code units".to_string(),
        ));
    }
    if semantic_evidence_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "semantic fact evidence is inconsistent with indexed code units".to_string(),
        ));
    }
    if derived_dependency_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "derived record dependencies are inconsistent with indexed files".to_string(),
        ));
    }
    if dirty_record_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "dirty records cannot support an active generation".to_string(),
        ));
    }
    if ir_graph_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "IR graph is inconsistent with indexed code units".to_string(),
        ));
    }
    Ok(())
}

/// Validation run on read-model (summary/inventory) reads. It is deliberately
/// lighter than the full-snapshot validator — schema, required tables, and the
/// dirty-record gate — and, like it, no longer runs `PRAGMA integrity_check`.
fn validate_active_generation_for_read_model(
    connection: &Connection,
    generation_id: &str,
) -> Result<(), IndexStoreError> {
    schema_version_read_gate(stored_schema_version(connection)?)?;
    if !required_schema_is_present(connection)? {
        return Err(IndexStoreError::InvalidState(
            "required storage schema is missing or malformed".to_string(),
        ));
    }
    if dirty_record_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "dirty records cannot support an active generation".to_string(),
        ));
    }
    Ok(())
}

fn inspect_connection(
    connection: &Connection,
    active_generation: Option<&str>,
) -> Result<StorageInspection, IndexStoreError> {
    let schema_version = connection
        .query_row("SELECT max(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<u32>>(0)
        })
        .map_err(sql_unavailable)?;
    let journal_mode = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .map_err(sql_unavailable)?;
    let foreign_keys = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, u32>(0))
        .map_err(sql_unavailable)?;
    let busy_timeout_ms = connection
        .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u32>(0))
        .map_err(sql_unavailable)?;
    let temp_store_code = connection
        .query_row("PRAGMA temp_store", [], |row| row.get::<_, u32>(0))
        .map_err(sql_unavailable)?;
    let integrity_check = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(sql_unavailable)?;
    let (code_unit_count, dependency_record_count, dirty_record_count) =
        if let Some(active_generation) = active_generation {
            (
                Some(active_generation_table_count(
                    connection,
                    "code_units",
                    active_generation,
                    "code unit",
                )?),
                Some(active_generation_table_count(
                    connection,
                    "derived_record_dependencies",
                    active_generation,
                    "dependency record",
                )?),
                Some(active_generation_table_count(
                    connection,
                    "dirty_records",
                    active_generation,
                    "dirty record",
                )?),
            )
        } else {
            (None, None, None)
        };

    Ok(StorageInspection {
        layout: IndexStorageLayout::Empty,
        mutable_database_present: false,
        legacy_generation_layout_present: false,
        wal_bytes: None,
        shm_bytes: None,
        active_generation: active_generation.map(str::to_string),
        schema_version,
        code_unit_count,
        dependency_record_count,
        dirty_record_count,
        journal_mode: Some(journal_mode),
        foreign_keys_enabled: Some(foreign_keys == 1),
        busy_timeout_ms: Some(busy_timeout_ms),
        temp_store: Some(match temp_store_code {
            2 => "memory".to_string(),
            1 => "file".to_string(),
            _ => "default".to_string(),
        }),
        integrity_check: Some(integrity_check),
    })
}

fn active_generation_table_count(
    connection: &Connection,
    table: &str,
    generation_id: &str,
    label: &str,
) -> Result<u64, IndexStoreError> {
    let table_exists = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get::<_, u32>(0),
        )
        .map_err(sql_unavailable)?
        == 1;
    if !table_exists {
        return Ok(0);
    }
    let count = connection
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE generation_id = ?1"),
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    u64::try_from(count)
        .map_err(|_| IndexStoreError::InvalidState(format!("{label} count is outside valid range")))
}

fn all_required_tables_exist(connection: &Connection) -> Result<bool, IndexStoreError> {
    for table in REQUIRED_SCHEMA {
        let count = connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                params![table.name],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_unavailable)?;
        if count != 1 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn required_schema_is_present(connection: &Connection) -> Result<bool, IndexStoreError> {
    if !all_required_tables_exist(connection)? {
        return Ok(false);
    }
    for table in REQUIRED_SCHEMA {
        let details = table_details(connection, table.name)?;
        for column in table.columns {
            if !details.columns.iter().any(|candidate| candidate == column) {
                return Ok(false);
            }
        }
        for primary_key_column in table.primary_key_columns {
            if !details
                .primary_key_columns
                .iter()
                .any(|candidate| candidate == primary_key_column)
            {
                return Ok(false);
            }
        }
        if foreign_key_row_count(connection, table.name)? < table.minimum_foreign_key_rows {
            return Ok(false);
        }
        let sql = table_sql(connection, table.name)?;
        for fragment in table.required_sql_fragments {
            if !sql.contains(fragment) {
                return Ok(false);
            }
        }
    }
    if foreign_key_violation_count(connection)? != 0 {
        return Ok(false);
    }
    Ok(true)
}

fn table_details(connection: &Connection, table: &str) -> Result<TableDetails, IndexStoreError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, u32>(5)?))
        })
        .map_err(sql_unavailable)?;
    let mut columns = Vec::new();
    let mut primary_key_columns = Vec::new();
    for row in rows {
        let (name, primary_key_ordinal) = row.map_err(sql_unavailable)?;
        if primary_key_ordinal > 0 {
            primary_key_columns.push(name.clone());
        }
        columns.push(name);
    }
    Ok(TableDetails {
        columns,
        primary_key_columns,
    })
}

fn foreign_key_row_count(connection: &Connection, table: &str) -> Result<usize, IndexStoreError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA foreign_key_list({table})"))
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map([], |_| Ok(()))
        .map_err(sql_unavailable)?;
    let mut count = 0usize;
    for row in rows {
        row.map_err(sql_unavailable)?;
        count += 1;
    }
    Ok(count)
}

fn table_sql(connection: &Connection, table: &str) -> Result<String, IndexStoreError> {
    connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_unavailable)
}

fn foreign_key_violation_count(connection: &Connection) -> Result<usize, IndexStoreError> {
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map([], |_| Ok(()))
        .map_err(sql_unavailable)?;
    let mut count = 0usize;
    for row in rows {
        row.map_err(sql_unavailable)?;
        count += 1;
    }
    Ok(count)
}

fn family_evidence_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let row_violation_count = connection
        .query_row(
            "SELECT count(*) \
             FROM evidence \
             LEFT JOIN families \
               ON families.generation_id = evidence.generation_id \
              AND families.family_id = evidence.family_id \
             LEFT JOIN code_units \
               ON code_units.generation_id = evidence.generation_id \
              AND code_units.code_unit_id = evidence.code_unit_id \
             LEFT JOIN indexed_files \
               ON indexed_files.generation_id = evidence.generation_id \
              AND indexed_files.path = evidence.path \
             WHERE evidence.generation_id = ?1 \
               AND evidence.family_id IS NOT NULL \
               AND (families.family_id IS NULL \
                    OR evidence.code_unit_id IS NULL \
                    OR code_units.code_unit_id IS NULL \
                    OR indexed_files.path IS NULL \
                    OR evidence.path <> code_units.path \
                    OR evidence.content_hash <> code_units.content_hash \
                    OR evidence.content_hash <> indexed_files.content_hash \
                    OR evidence.start_byte < code_units.start_byte \
                    OR evidence.end_byte > code_units.end_byte)",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    // Every emitted family is backed by supported evidence, so evidence is
    // required for all four prevalence classifications.
    let missing_evidence_count = connection
        .query_row(
            "SELECT count(*) \
             FROM families \
             WHERE families.generation_id = ?1 \
               AND NOT EXISTS (\
                   SELECT 1 \
                   FROM evidence \
                   WHERE evidence.generation_id = families.generation_id \
                     AND evidence.family_id = families.family_id\
               )",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    let invalid_classification_count = connection
        .query_row(
            "SELECT count(*) \
             FROM families \
             WHERE generation_id = ?1 \
               AND classification NOT IN \
                   ('DOMINANT_PATTERN', 'SUPPORTED_PATTERN', 'MINORITY_PATTERN', 'UNKNOWN_PREVALENCE')",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    let invalid_covered_claim_count =
        family_evidence_covered_claims_violation_count(connection, generation_id)?;
    let total = row_violation_count
        .checked_add(missing_evidence_count)
        .and_then(|count| count.checked_add(invalid_classification_count))
        .and_then(|count| count.checked_add(invalid_covered_claim_count))
        .ok_or_else(|| {
            IndexStoreError::InvalidState("family evidence violation count overflow".to_string())
        })?;
    usize::try_from(total).map_err(|_| {
        IndexStoreError::InvalidState(
            "family evidence violation count is outside valid range".to_string(),
        )
    })
}

fn family_evidence_covered_claims_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<i64, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT covered_claims_json \
             FROM evidence \
             WHERE generation_id = ?1 AND family_id IS NOT NULL",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| row.get::<_, String>(0))
        .map_err(sql_unavailable)?;
    let mut count = 0i64;
    for row in rows {
        let covered_claims_json = row.map_err(sql_unavailable)?;
        if stored_family_evidence_covered_claims(&covered_claims_json).is_err() {
            count += 1;
        }
    }
    Ok(count)
}

fn semantic_evidence_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let count = connection
        .query_row(
            "SELECT count(*) \
             FROM semantic_facts \
             JOIN evidence \
               ON evidence.generation_id = semantic_facts.generation_id \
              AND evidence.evidence_id = semantic_facts.evidence_id \
             LEFT JOIN code_units \
               ON code_units.generation_id = evidence.generation_id \
              AND code_units.code_unit_id = evidence.code_unit_id \
             LEFT JOIN indexed_files \
               ON indexed_files.generation_id = evidence.generation_id \
              AND indexed_files.path = evidence.path \
             WHERE semantic_facts.generation_id = ?1 \
               AND (evidence.family_id IS NOT NULL \
                    OR evidence.code_unit_id IS NULL \
                    OR code_units.code_unit_id IS NULL \
                    OR indexed_files.path IS NULL \
                    OR evidence.path <> code_units.path \
                    OR evidence.content_hash <> code_units.content_hash \
                    OR evidence.content_hash <> indexed_files.content_hash \
                    OR evidence.start_byte < code_units.start_byte \
                    OR evidence.end_byte > code_units.end_byte)",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    usize::try_from(count).map_err(|_| {
        IndexStoreError::InvalidState(
            "semantic evidence violation count is outside valid range".to_string(),
        )
    })
}

fn derived_dependency_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let row_violation_count = connection
        .query_row(
            "SELECT count(*) \
             FROM derived_record_dependencies \
             LEFT JOIN indexed_files \
               ON indexed_files.generation_id = derived_record_dependencies.generation_id \
              AND indexed_files.path = derived_record_dependencies.path \
             WHERE derived_record_dependencies.generation_id = ?1 \
               AND (indexed_files.path IS NULL \
                    OR derived_record_dependencies.content_hash <> indexed_files.content_hash)",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    let payload_violation_count =
        derived_dependency_payload_violation_count(connection, generation_id)?;
    let total = row_violation_count
        .checked_add(payload_violation_count)
        .ok_or_else(|| {
            IndexStoreError::InvalidState("derived dependency violation count overflow".to_string())
        })?;
    usize::try_from(total).map_err(|_| {
        IndexStoreError::InvalidState(
            "derived dependency violation count is outside valid range".to_string(),
        )
    })
}

fn derived_dependency_payload_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<i64, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT record_kind, record_id, path, content_hash \
             FROM derived_record_dependencies \
             WHERE generation_id = ?1",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut count = 0i64;
    for row in rows {
        let (record_kind, record_id, path, content_hash) = row.map_err(sql_unavailable)?;
        let valid =
            validate_stored_semantic_text_field("stored dependency record kind", &record_kind)
                .and_then(|()| {
                    validate_stored_semantic_text_field("stored dependency record id", &record_id)
                })
                .and_then(|()| validate_stored_repo_relative_path(&path, "stored dependency path"))
                .and_then(|()| {
                    ContentHash::new(content_hash).map(|_| ()).map_err(|_| {
                        IndexStoreError::InvalidState(
                            "stored dependency content hash is invalid".to_string(),
                        )
                    })
                })
                .is_ok();
        if !valid {
            count += 1;
        }
    }
    Ok(count)
}

fn dirty_record_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT record_kind, record_id, reason \
             FROM dirty_records \
             WHERE generation_id = ?1",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_unavailable)?;
    let mut count = 0usize;
    for row in rows {
        let (record_kind, record_id, reason) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored dirty record kind", &record_kind)?;
        validate_stored_semantic_text_field("stored dirty record id", &record_id)?;
        validate_stored_semantic_text_field("stored dirty record reason", &reason)?;
        count += 1;
    }
    Ok(count)
}

fn ir_graph_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let node_count = connection
        .query_row(
            "SELECT count(*) \
             FROM ir_nodes \
             LEFT JOIN code_units \
               ON code_units.generation_id = ir_nodes.generation_id \
              AND code_units.code_unit_id = ir_nodes.code_unit_id \
             WHERE ir_nodes.generation_id = ?1 \
               AND (code_units.code_unit_id IS NULL \
                    OR ir_nodes.node_id <> ('ir:' || ir_nodes.code_unit_id) \
                    OR ir_nodes.kind NOT IN (\
                        'module', 'function', 'arrow_function', 'class', 'method', \
                        'react_component', 'react_hook', 'express_route', \
                        'next_app_page', 'next_app_layout', 'next_route_handler', \
                        'next_pages_api_route', 'next_pages_page', 'fastify_route', \
                        'fastify_plugin_registration', \
                        'prisma_query', 'prisma_transaction', \
                        'drizzle_schema_table', 'drizzle_query', 'drizzle_transaction', \
                        'zod_schema', 'nest_controller', 'nest_route', \
                        'nest_injectable', 'nest_module', 'hono_route', \
                        'test_suite', 'test_case', 'async_function', \
                        'fastapi_route', 'pytest_test', 'pytest_fixture', \
                        'pydantic_model', 'sqlalchemy_model', \
                        'sqlalchemy_repository_method', 'project_config', 'unknown'\
                    ) \
                    OR ir_nodes.payload_json <> '{}')",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    let edge_count = connection
        .query_row(
            "SELECT count(*) \
             FROM ir_edges \
             LEFT JOIN ir_nodes AS from_nodes \
               ON from_nodes.generation_id = ir_edges.generation_id \
              AND from_nodes.node_id = ir_edges.from_node_id \
             LEFT JOIN ir_nodes AS to_nodes \
               ON to_nodes.generation_id = ir_edges.generation_id \
              AND to_nodes.node_id = ir_edges.to_node_id \
             WHERE ir_edges.generation_id = ?1 \
               AND (from_nodes.node_id IS NULL \
                    OR to_nodes.node_id IS NULL \
                    OR ir_edges.from_node_id = ir_edges.to_node_id \
                    OR ir_edges.label <> 'contains')",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    let total = node_count
        .checked_add(edge_count)
        .ok_or_else(|| IndexStoreError::InvalidState("IR violation count overflow".to_string()))?;
    usize::try_from(total).map_err(|_| {
        IndexStoreError::InvalidState("IR violation count is outside valid range".to_string())
    })
}

/// Count code-unit rows whose stored content hash or byte range disagrees with
/// their indexed file. The `record_code_unit` write path proves both invariants
/// per record, but the evidence-scoped validation scans only revisit units that
/// carry evidence; this set-wide check closes that gap so the activation gate is
/// a strict superset of per-record enforcement. Every code-unit row references an
/// existing file by foreign key, so an inner join covers the whole set.
fn code_unit_file_conformance_violation_count(
    connection: &Connection,
    generation_id: &str,
) -> Result<usize, IndexStoreError> {
    let count = connection
        .query_row(
            "SELECT count(*) \
             FROM code_units \
             JOIN indexed_files \
               ON indexed_files.generation_id = code_units.generation_id \
              AND indexed_files.path = code_units.path \
             WHERE code_units.generation_id = ?1 \
               AND (code_units.content_hash <> indexed_files.content_hash \
                    OR code_units.end_byte > indexed_files.size_bytes)",
            params![generation_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_unavailable)?;
    usize::try_from(count).map_err(|_| {
        IndexStoreError::InvalidState(
            "code unit conformance violation count is outside valid range".to_string(),
        )
    })
}

fn generation_status(
    connection: &Connection,
    generation_id: &str,
) -> Result<Option<String>, IndexStoreError> {
    connection
        .query_row(
            "SELECT status FROM index_generations WHERE generation_id = ?1",
            params![generation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_unavailable)
}

fn active_generation_id(connection: &Connection) -> Result<Option<String>, IndexStoreError> {
    let generation_id = connection
        .query_row(
            "SELECT generation_id \
             FROM index_generations \
             WHERE status = 'active' \
             ORDER BY activated_at DESC, generation_id DESC \
             LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_unavailable)?;
    if let Some(generation_id) = &generation_id {
        validate_generation_id(generation_id)?;
    }
    Ok(generation_id)
}

fn list_database_generation_entries(
    connection: &Connection,
) -> Result<Vec<GenerationDirectoryEntry>, IndexStoreError> {
    let mut statement = connection
        .prepare("SELECT generation_id FROM index_generations")
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_unavailable)?;
    let mut entries = Vec::new();
    for row in rows {
        let generation_id = row.map_err(sql_unavailable)?;
        validate_generation_id(&generation_id)?;
        let Some(number) = parse_generation_number(&generation_id) else {
            continue;
        };
        entries.push(GenerationDirectoryEntry {
            generation_id,
            number,
        });
    }
    entries.sort_by_key(|entry| entry.number);
    Ok(entries)
}

fn ensure_real_dir(path: &Path, label: &'static str) -> Result<(), IndexStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IndexStoreError::InvalidState(
            format!("{label} must not be a symlink"),
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(IndexStoreError::InvalidState(format!(
            "{label} exists and is not a directory"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|_| unavailable("failed to create storage directory"))
        }
        Err(_) => Err(unavailable("failed to inspect storage directory")),
    }
}

fn ensure_existing_real_dir(path: &Path, label: &'static str) -> Result<(), IndexStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IndexStoreError::InvalidState(
            format!("{label} must not be a symlink"),
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(IndexStoreError::InvalidState(format!(
            "{label} exists and is not a directory"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(IndexStoreError::InvalidState(format!("{label} is missing")))
        }
        Err(_) => Err(unavailable("failed to inspect storage directory")),
    }
}

fn required_regular_file_size(path: &Path, label: &'static str) -> Result<u64, IndexStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IndexStoreError::InvalidState(
            format!("{label} must not be a symlink"),
        )),
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err(IndexStoreError::InvalidState(format!(
            "{label} exists and is not a file"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(IndexStoreError::InvalidState(format!("{label} is missing")))
        }
        Err(_) => Err(unavailable("failed to inspect storage file")),
    }
}

fn optional_regular_file_size(path: &Path, label: &'static str) -> Result<u64, IndexStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IndexStoreError::InvalidState(
            format!("{label} must not be a symlink"),
        )),
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err(IndexStoreError::InvalidState(format!(
            "{label} exists and is not a file"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(unavailable("failed to inspect storage sidecar")),
    }
}

fn optional_legacy_path_size(path: &Path, label: &'static str) -> Result<u64, IndexStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(IndexStoreError::InvalidState(
            format!("{label} must not be a symlink"),
        )),
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(metadata) if metadata.is_dir() => directory_tree_size(path, metadata.len()),
        Ok(_) => Err(IndexStoreError::InvalidState(format!(
            "{label} exists and is not a regular file or directory"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(unavailable("failed to inspect legacy layout path")),
    }
}

fn directory_tree_size(path: &Path, initial_size: u64) -> Result<u64, IndexStoreError> {
    let mut total = initial_size;
    for entry in fs::read_dir(path).map_err(|_| unavailable("failed to read legacy directory"))? {
        let entry = entry.map_err(|_| unavailable("failed to read legacy directory entry"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| unavailable("failed to inspect legacy directory entry"))?;
        if metadata.file_type().is_symlink() {
            return Err(IndexStoreError::InvalidState(
                "legacy layout entries must not be symlinks".to_string(),
            ));
        }
        let entry_size = if metadata.is_dir() {
            directory_tree_size(&entry.path(), metadata.len())?
        } else if metadata.is_file() {
            metadata.len()
        } else {
            return Err(IndexStoreError::InvalidState(
                "legacy layout entry exists and is not a regular file or directory".to_string(),
            ));
        };
        total = checked_storage_clean_total(total, entry_size)?;
    }
    Ok(total)
}

fn checked_size_total(
    database_bytes: u64,
    wal_bytes: u64,
    shm_bytes: u64,
) -> Result<u64, IndexStoreError> {
    database_bytes
        .checked_add(wal_bytes)
        .and_then(|value| value.checked_add(shm_bytes))
        .ok_or_else(|| {
            IndexStoreError::InvalidState("repository index size is invalid".to_string())
        })
}

fn checked_storage_clean_total(left: u64, right: u64) -> Result<u64, IndexStoreError> {
    left.checked_add(right).ok_or_else(|| {
        IndexStoreError::InvalidState("repository storage size is invalid".to_string())
    })
}

fn validate_generation_id(generation_id: &str) -> Result<(), IndexStoreError> {
    if parse_generation_number(generation_id).is_some_and(|number| number > 0) {
        Ok(())
    } else {
        Err(IndexStoreError::InvalidState(
            "generation id must match gen-000001 format".to_string(),
        ))
    }
}

fn parse_generation_number(value: &str) -> Option<u32> {
    let digits = value.strip_prefix("gen-")?;
    if digits.len() == 6 && digits.chars().all(|character| character.is_ascii_digit()) {
        digits.parse::<u32>().ok()
    } else {
        None
    }
}

fn validate_repo_relative_path(path: &str) -> Result<(), IndexStoreError> {
    crate::core::policy::paths::validate_repo_relative_path(path).map_err(|error| match error {
        RepoRelativePathError::Empty | RepoRelativePathError::ControlCharacter => {
            invalid_record("indexed file path must be non-empty")
        }
        RepoRelativePathError::Absolute
        | RepoRelativePathError::Backslash
        | RepoRelativePathError::UriLike => {
            invalid_record("indexed file path must be repo-relative")
        }
        RepoRelativePathError::Traversal => {
            invalid_record("indexed file path must not contain traversal or prefixes")
        }
    })
}

fn validate_stored_repo_relative_path(
    path: &str,
    label: &'static str,
) -> Result<(), IndexStoreError> {
    validate_repo_relative_path(path)
        .map_err(|_| IndexStoreError::InvalidState(format!("{label} is invalid")))
}

fn validate_stored_non_empty_text(value: &str, label: &'static str) -> Result<(), IndexStoreError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        Err(IndexStoreError::InvalidState(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_stored_code_unit_id(id: &str, path: &str) -> Result<(), IndexStoreError> {
    validate_stored_non_empty_text(id, "stored code unit id")?;
    if id.contains('\\') || id.contains("://") || looks_like_windows_absolute_path(id) {
        return Err(IndexStoreError::InvalidState(
            "stored code unit id is invalid".to_string(),
        ));
    }
    let expected_prefix = format!("unit:{path}#");
    if !id.starts_with(&expected_prefix) {
        return Err(IndexStoreError::InvalidState(
            "stored code unit id does not match code unit path".to_string(),
        ));
    }
    Ok(())
}

fn validate_index_text_field(value: &str, label: &'static str) -> Result<(), IndexStoreError> {
    if value.trim().is_empty()
        || value.chars().any(char::is_control)
        || value.contains('\\')
        || value.contains("://")
        || looks_like_embedded_absolute_path(value)
        || looks_like_source_snippet(value)
    {
        Err(IndexStoreError::InvalidRecord(format!(
            "{label} is invalid"
        )))
    } else {
        Ok(())
    }
}

fn validate_stored_ir_node_id(id: &str, code_unit_id: &str) -> Result<(), IndexStoreError> {
    validate_stored_non_empty_text(id, "stored IR node id")?;
    if id.contains('\\') || id.contains("://") || looks_like_windows_absolute_path(id) {
        return Err(IndexStoreError::InvalidState(
            "stored IR node id is invalid".to_string(),
        ));
    }
    if id != format!("ir:{code_unit_id}") {
        return Err(IndexStoreError::InvalidState(
            "stored IR node id does not match code unit id".to_string(),
        ));
    }
    Ok(())
}

fn validate_empty_object_payload(
    payload_json: &str,
    label: &'static str,
) -> Result<(), IndexStoreError> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_| IndexStoreError::InvalidRecord(format!("{label} is invalid")))?;
    if value == serde_json::json!({}) {
        Ok(())
    } else {
        Err(IndexStoreError::InvalidRecord(format!(
            "{label} is invalid"
        )))
    }
}

fn validate_stored_empty_object_payload(
    payload_json: &str,
    label: &'static str,
) -> Result<(), IndexStoreError> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|_| IndexStoreError::InvalidState(format!("{label} is invalid")))?;
    if value == serde_json::json!({}) {
        Ok(())
    } else {
        Err(IndexStoreError::InvalidState(format!("{label} is invalid")))
    }
}

fn stored_family_evidence_covered_claims(
    claims_json: &str,
) -> Result<Vec<String>, IndexStoreError> {
    let claims: Vec<String> = serde_json::from_str(claims_json).map_err(|_| {
        IndexStoreError::InvalidState(
            "stored family evidence covered claims JSON is invalid".to_string(),
        )
    })?;
    if claims.is_empty() {
        return Err(IndexStoreError::InvalidState(
            "stored family evidence covered claims are empty".to_string(),
        ));
    }
    let mut seen = Vec::new();
    for claim in &claims {
        validate_stored_semantic_text_field("stored family evidence covered claim", claim)?;
        if !family_evidence_covered_claim_is_supported(claim) {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence covered claim is unsupported".to_string(),
            ));
        }
        if seen
            .iter()
            .any(|seen: &&String| seen.as_str() == claim.as_str())
        {
            return Err(IndexStoreError::InvalidState(
                "stored family evidence covered claims contain duplicates".to_string(),
            ));
        }
        seen.push(claim);
    }
    Ok(claims)
}

fn validate_stored_semantic_text_field(
    label: &'static str,
    value: &str,
) -> Result<(), IndexStoreError> {
    validate_stored_non_empty_text(value, label)?;
    if value.contains("://")
        || looks_like_embedded_absolute_path(value)
        || looks_like_source_snippet(value)
    {
        Err(IndexStoreError::InvalidState(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_stored_family_classification(classification: &str) -> Result<(), IndexStoreError> {
    match FamilyPrevalenceClass::parse_token(classification) {
        Ok(_) => Ok(()),
        Err(_) => Err(IndexStoreError::InvalidState(
            "stored family classification is invalid".to_string(),
        )),
    }
}

/// Raw stored `families` prevalence columns, before range validation.
struct StoredFamilyPrevalenceColumns {
    eligible_peer_count: i64,
    supported_member_count: i64,
    coverage_ratio: Option<f64>,
    competing_ready_family_count: i64,
    largest_competing_support: i64,
    blocked_peer_count: i64,
    unsupported_peer_count: i64,
    classification_reason: String,
}

/// Build a validated [`FamilyPrevalence`] from stored SQL column values.
fn stored_family_prevalence(
    columns: StoredFamilyPrevalenceColumns,
) -> Result<FamilyPrevalence, IndexStoreError> {
    let count = |value: i64, label: &str| {
        usize::try_from(value)
            .map_err(|_| IndexStoreError::InvalidState(format!("stored family {label} is invalid")))
    };
    if columns.classification_reason.trim().is_empty() {
        return Err(IndexStoreError::InvalidState(
            "stored family classification reason is empty".to_string(),
        ));
    }
    Ok(FamilyPrevalence {
        eligible_peer_count: count(columns.eligible_peer_count, "eligible peer count")?,
        supported_member_count: count(columns.supported_member_count, "supported member count")?,
        coverage_ratio: columns.coverage_ratio,
        competing_ready_family_count: count(
            columns.competing_ready_family_count,
            "competing ready family count",
        )?,
        largest_competing_support: count(
            columns.largest_competing_support,
            "largest competing support",
        )?,
        blocked_peer_count: count(columns.blocked_peer_count, "blocked peer count")?,
        unsupported_peer_count: count(columns.unsupported_peer_count, "unsupported peer count")?,
        classification_reason: columns.classification_reason,
    })
}

/// Bind the prevalence columns for a family INSERT.
fn family_prevalence_count_param(value: usize, label: &str) -> Result<i64, IndexStoreError> {
    i64::try_from(value).map_err(|_| {
        IndexStoreError::InvalidRecord(format!("family {label} exceeds SQLite integer range"))
    })
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

fn sql_unavailable(error: rusqlite::Error) -> IndexStoreError {
    IndexStoreError::Unavailable(error.to_string())
}

fn unavailable(message: &'static str) -> IndexStoreError {
    IndexStoreError::Unavailable(message.to_string())
}

fn invalid_record(message: &'static str) -> IndexStoreError {
    IndexStoreError::InvalidRecord(message.to_string())
}

fn family_store_error(error: IndexStoreError) -> StoreError {
    match error {
        IndexStoreError::Unavailable(message) => StoreError::Unavailable(message),
        IndexStoreError::InvalidState(message) => StoreError::InvalidState(message),
        IndexStoreError::InvalidRecord(message) => StoreError::InvalidRecord(message),
        IndexStoreError::SchemaVersionOutdated(message) => {
            StoreError::SchemaVersionOutdated(message)
        }
    }
}

struct TableDetails {
    columns: Vec<String>,
    primary_key_columns: Vec<String>,
}

struct RequiredTableSchema {
    name: &'static str,
    columns: &'static [&'static str],
    primary_key_columns: &'static [&'static str],
    minimum_foreign_key_rows: usize,
    required_sql_fragments: &'static [&'static str],
}

const REQUIRED_SCHEMA: &[RequiredTableSchema] = &[
    RequiredTableSchema {
        name: "schema_migrations",
        columns: &["version", "name", "applied_at"],
        primary_key_columns: &["version"],
        minimum_foreign_key_rows: 0,
        required_sql_fragments: &["PRIMARY KEY"],
    },
    RequiredTableSchema {
        name: "index_generations",
        columns: &[
            "generation_id",
            "status",
            "created_at",
            "activated_at",
            "repogrammar_version",
            "repository_revision",
            "worktree_hash",
        ],
        primary_key_columns: &["generation_id"],
        minimum_foreign_key_rows: 0,
        required_sql_fragments: &["PRIMARY KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "indexed_files",
        columns: &[
            "generation_id",
            "path",
            "content_hash",
            "size_bytes",
            "language",
        ],
        primary_key_columns: &["generation_id", "path"],
        minimum_foreign_key_rows: 1,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "code_units",
        columns: &[
            "generation_id",
            "code_unit_id",
            "path",
            "language",
            "kind",
            "start_byte",
            "end_byte",
            "content_hash",
        ],
        primary_key_columns: &["generation_id", "code_unit_id"],
        minimum_foreign_key_rows: 3,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "ir_nodes",
        columns: &[
            "generation_id",
            "node_id",
            "code_unit_id",
            "kind",
            "payload_json",
        ],
        primary_key_columns: &["generation_id", "node_id"],
        minimum_foreign_key_rows: 3,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
    },
    RequiredTableSchema {
        name: "ir_edges",
        columns: &["generation_id", "from_node_id", "to_node_id", "label"],
        primary_key_columns: &["generation_id", "from_node_id", "to_node_id", "label"],
        minimum_foreign_key_rows: 4,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
    },
    RequiredTableSchema {
        name: "semantic_facts",
        columns: &[
            "generation_id",
            "fact_id",
            "kind",
            "subject",
            "target",
            "certainty",
            "origin_engine",
            "origin_engine_version",
            "origin_method",
            "assumptions_json",
            "evidence_id",
        ],
        primary_key_columns: &["generation_id", "fact_id"],
        minimum_foreign_key_rows: 2,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "families",
        columns: &[
            "generation_id",
            "family_id",
            "classification",
            "eligible_peer_count",
            "supported_member_count",
            "coverage_ratio",
            "competing_ready_family_count",
            "largest_competing_support",
            "blocked_peer_count",
            "unsupported_peer_count",
            "classification_reason",
        ],
        primary_key_columns: &["generation_id", "family_id"],
        minimum_foreign_key_rows: 1,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "family_members",
        columns: &["generation_id", "family_id", "code_unit_id", "role"],
        primary_key_columns: &["generation_id", "family_id", "code_unit_id"],
        minimum_foreign_key_rows: 4,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
    },
    RequiredTableSchema {
        name: "variation_slots",
        columns: &["generation_id", "family_id", "slot_id", "description"],
        primary_key_columns: &["generation_id", "family_id", "slot_id"],
        minimum_foreign_key_rows: 2,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
    },
    RequiredTableSchema {
        name: "evidence",
        columns: &[
            "generation_id",
            "evidence_id",
            "family_id",
            "code_unit_id",
            "covered_claims_json",
            "path",
            "content_hash",
            "start_byte",
            "end_byte",
            "note",
        ],
        primary_key_columns: &["generation_id", "evidence_id"],
        minimum_foreign_key_rows: 6,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "family_constraint_profiles",
        columns: &["generation_id", "family_id", "profile_json"],
        primary_key_columns: &["generation_id", "family_id"],
        // One `index_generations` mapping plus the two-column `families` mapping.
        minimum_foreign_key_rows: 3,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "derived_record_dependencies",
        columns: &[
            "generation_id",
            "record_kind",
            "record_id",
            "path",
            "content_hash",
        ],
        primary_key_columns: &["generation_id", "record_kind", "record_id", "path"],
        minimum_foreign_key_rows: 2,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
    RequiredTableSchema {
        name: "dirty_records",
        columns: &[
            "generation_id",
            "record_kind",
            "record_id",
            "reason",
            "marked_at_generation_id",
        ],
        primary_key_columns: &["generation_id", "record_kind", "record_id"],
        minimum_foreign_key_rows: 2,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY", "CHECK"],
    },
];

const INITIAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS index_generations (
    generation_id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('building', 'validated', 'active', 'failed')),
    created_at TEXT NOT NULL,
    activated_at TEXT,
    repogrammar_version TEXT NOT NULL,
    repository_revision TEXT,
    worktree_hash TEXT
);

CREATE TABLE IF NOT EXISTS indexed_files (
    generation_id TEXT NOT NULL,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    language TEXT NOT NULL,
    PRIMARY KEY (generation_id, path),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS code_units (
    generation_id TEXT NOT NULL,
    code_unit_id TEXT NOT NULL,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    content_hash TEXT NOT NULL,
    PRIMARY KEY (generation_id, code_unit_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, path) REFERENCES indexed_files(generation_id, path) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ir_nodes (
    generation_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    code_unit_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (generation_id, node_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, code_unit_id) REFERENCES code_units(generation_id, code_unit_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ir_edges (
    generation_id TEXT NOT NULL,
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (generation_id, from_node_id, to_node_id, label),
    FOREIGN KEY (generation_id, from_node_id) REFERENCES ir_nodes(generation_id, node_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, to_node_id) REFERENCES ir_nodes(generation_id, node_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS semantic_facts (
    generation_id TEXT NOT NULL,
    fact_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    subject TEXT NOT NULL,
    target TEXT,
    certainty TEXT NOT NULL,
    origin_engine TEXT NOT NULL,
    origin_engine_version TEXT NOT NULL,
    origin_method TEXT NOT NULL,
    assumptions_json TEXT NOT NULL DEFAULT '[]' CHECK (assumptions_json <> ''),
    evidence_id TEXT NOT NULL,
    PRIMARY KEY (generation_id, fact_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, evidence_id) REFERENCES evidence(generation_id, evidence_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS families (
    generation_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN ('DOMINANT_PATTERN', 'SUPPORTED_PATTERN', 'MINORITY_PATTERN', 'UNKNOWN_PREVALENCE')),
    eligible_peer_count INTEGER NOT NULL CHECK (eligible_peer_count >= 0),
    supported_member_count INTEGER NOT NULL CHECK (supported_member_count >= 0),
    coverage_ratio REAL,
    competing_ready_family_count INTEGER NOT NULL CHECK (competing_ready_family_count >= 0),
    largest_competing_support INTEGER NOT NULL CHECK (largest_competing_support >= 0),
    blocked_peer_count INTEGER NOT NULL CHECK (blocked_peer_count >= 0),
    unsupported_peer_count INTEGER NOT NULL CHECK (unsupported_peer_count >= 0),
    classification_reason TEXT NOT NULL CHECK (classification_reason <> ''),
    PRIMARY KEY (generation_id, family_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS family_members (
    generation_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    code_unit_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (generation_id, family_id, code_unit_id),
    FOREIGN KEY (generation_id, family_id) REFERENCES families(generation_id, family_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, code_unit_id) REFERENCES code_units(generation_id, code_unit_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_family_members_generation_code_unit
ON family_members(generation_id, code_unit_id, family_id);

CREATE TABLE IF NOT EXISTS variation_slots (
    generation_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    slot_id TEXT NOT NULL,
    description TEXT NOT NULL,
    PRIMARY KEY (generation_id, family_id, slot_id),
    FOREIGN KEY (generation_id, family_id) REFERENCES families(generation_id, family_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS evidence (
    generation_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    family_id TEXT,
    code_unit_id TEXT,
    covered_claims_json TEXT NOT NULL DEFAULT '[]' CHECK (covered_claims_json <> ''),
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    note TEXT NOT NULL,
    PRIMARY KEY (generation_id, evidence_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, family_id) REFERENCES families(generation_id, family_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, path) REFERENCES indexed_files(generation_id, path) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, code_unit_id) REFERENCES code_units(generation_id, code_unit_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_evidence_generation_family_order
ON evidence(generation_id, family_id, path COLLATE BINARY, start_byte, end_byte, code_unit_id COLLATE BINARY, evidence_id COLLATE BINARY);

CREATE INDEX IF NOT EXISTS idx_evidence_generation_path_family
ON evidence(generation_id, path COLLATE BINARY, family_id, start_byte, end_byte, evidence_id COLLATE BINARY);

CREATE TABLE IF NOT EXISTS family_constraint_profiles (
    generation_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    profile_json TEXT NOT NULL CHECK (profile_json <> ''),
    PRIMARY KEY (generation_id, family_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, family_id) REFERENCES families(generation_id, family_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS derived_record_dependencies (
    generation_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind <> ''),
    record_id TEXT NOT NULL CHECK (record_id <> ''),
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    PRIMARY KEY (generation_id, record_kind, record_id, path),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, path) REFERENCES indexed_files(generation_id, path) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_derived_record_dependencies_path
ON derived_record_dependencies(generation_id, path, content_hash);

CREATE TABLE IF NOT EXISTS dirty_records (
    generation_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind <> ''),
    record_id TEXT NOT NULL CHECK (record_id <> ''),
    reason TEXT NOT NULL CHECK (reason <> ''),
    marked_at_generation_id TEXT NOT NULL,
    PRIMARY KEY (generation_id, record_kind, record_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (marked_at_generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dirty_records_generation
ON dirty_records(generation_id, record_kind, record_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::test_support::{create_test_symlink_file, TempWorkspace};

    fn store(workspace: &TempWorkspace) -> SqliteIndexStore {
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        SqliteIndexStore::new(state)
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

    fn code_unit(path: &str) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#module:0-10"),
            path: path.to_string(),
            language: "typescript".to_string(),
            kind: "module".to_string(),
            start_byte: 0,
            end_byte: 10,
            content_hash: file(path).content_hash,
        }
    }

    fn ir_node(unit: &IndexedCodeUnitRecord) -> IndexedIrNodeRecord {
        IndexedIrNodeRecord {
            id: format!("ir:{}", unit.id),
            code_unit_id: unit.id.clone(),
            kind: unit.kind.clone(),
            payload_json: "{}".to_string(),
        }
    }

    fn ir_edge(from: &IndexedIrNodeRecord, to: &IndexedIrNodeRecord) -> IndexedIrEdgeRecord {
        IndexedIrEdgeRecord {
            from_node_id: from.id.clone(),
            to_node_id: to.id.clone(),
            label: "contains".to_string(),
        }
    }

    fn semantic_fact(path: &str) -> IndexedSemanticFactRecord {
        IndexedSemanticFactRecord {
            fact_id: format!("fact:{path}#import:express"),
            kind: "RESOLVED_IMPORT".to_string(),
            subject: format!("{path}#import:express"),
            target: Some("node_modules/@types/express/index.d.ts#Request".to_string()),
            certainty: "SEMANTIC".to_string(),
            origin_engine: "typescript".to_string(),
            origin_engine_version: "6.0.0".to_string(),
            origin_method: "compiler_api".to_string(),
            assumptions: Vec::new(),
            evidence_id: format!("evidence:fact:{path}#import:express"),
            code_unit_id: code_unit(path).id,
            path: path.to_string(),
            content_hash: file(path).content_hash,
            start_byte: 0,
            end_byte: 10,
            note: "compiler resolved import target".to_string(),
        }
    }

    fn family() -> IndexedFamilyRecord {
        IndexedFamilyRecord {
            family_id: "family:routes:read".to_string(),
            classification: "DOMINANT_PATTERN".to_string(),
            prevalence: crate::test_support::sample_family_prevalence(),
        }
    }

    fn family_member(path: &str) -> IndexedFamilyMemberRecord {
        IndexedFamilyMemberRecord {
            family_id: family().family_id,
            code_unit_id: code_unit(path).id,
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

    fn family_evidence(path: &str) -> IndexedFamilyEvidenceRecord {
        IndexedFamilyEvidenceRecord {
            evidence_id: format!("evidence:family:routes:read:{path}"),
            family_id: family().family_id,
            code_unit_id: code_unit(path).id,
            covered_claims: vec!["canonical".to_string(), "support".to_string()],
            path: path.to_string(),
            content_hash: file(path).content_hash,
            start_byte: 0,
            end_byte: 10,
            note: "same framework role and shape".to_string(),
        }
    }

    fn store_with_active_family(name: &str) -> (TempWorkspace, SqliteIndexStore, GenerationHandle) {
        let workspace = TempWorkspace::new(name);
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }
        store
            .record_variation_slot(&generation, &variation_slot())
            .expect("record variation slot");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        (workspace, store, generation)
    }

    #[test]
    fn constraint_profile_round_trips_through_the_store() {
        let workspace = TempWorkspace::new("sqlite-constraint-profile");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }
        let record = IndexedFamilyConstraintProfileRecord {
            family_id: family().family_id,
            profile: crate::test_support::sample_family_constraint_profile(),
        };
        store
            .record_family_constraint_profile(&generation, &record)
            .expect("record constraint profile");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let hydrated = store
            .show_family_constraint_profile(&family().family_id)
            .expect("show constraint profile")
            .expect("profile exists after activation");
        assert_eq!(
            hydrated,
            crate::test_support::sample_family_constraint_profile()
        );
        assert!(store
            .show_family_constraint_profile("family:missing")
            .expect("show missing profile")
            .is_none());

        // Hydrated profiles stay source-free: no URLs or absolute workspace paths.
        let rendered = format!("{hydrated:?}");
        assert!(!rendered.contains("://"));
        assert!(!rendered.contains(&workspace.path().display().to_string()));
    }

    #[test]
    fn constraint_profile_write_rejects_source_like_values() {
        let workspace = TempWorkspace::new("sqlite-constraint-profile-source-free");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_family(&generation, &family())
            .expect("record family");

        let mut profile = crate::test_support::sample_family_constraint_profile();
        profile.required_equal_features[0].values = vec!["const handler = route;".to_string()];
        let record = IndexedFamilyConstraintProfileRecord {
            family_id: family().family_id,
            profile,
        };
        let error = store
            .record_family_constraint_profile(&generation, &record)
            .expect_err("source-like feature value must be rejected");
        assert!(matches!(error, StoreError::InvalidRecord(_)));
    }

    #[test]
    fn stored_constraint_profile_rejects_malformed_rows() {
        let valid =
            constraint_profile_to_json(&crate::test_support::sample_family_constraint_profile())
                .expect("serialize sample profile");
        // The well-formed baseline hydrates cleanly.
        assert!(stored_constraint_profile(&valid).is_ok());

        let base: serde_json::Value = serde_json::from_str(&valid).expect("parse baseline JSON");
        let reject = |mutate: &dyn Fn(&mut serde_json::Value)| {
            let mut value = base.clone();
            mutate(&mut value);
            let json = serde_json::to_string(&value).expect("serialize mutated JSON");
            let error =
                stored_constraint_profile(&json).expect_err("malformed row must be rejected");
            assert!(
                matches!(error, IndexStoreError::InvalidState(_)),
                "expected a typed malformed-storage error, got {error:?}"
            );
        };

        // Wrong stored version.
        reject(&|value| value["version"] = serde_json::json!(999));
        // Unknown origin token.
        reject(&|value| value["required_equal_features"][0]["origin"] = serde_json::json!("notes"));
        // Unknown semantics token.
        reject(&|value| {
            value["required_equal_features"][0]["semantics"] = serde_json::json!("subset")
        });
        // Wrong JSON type for values.
        reject(&|value| value["required_equal_features"][0]["values"] = serde_json::json!("nope"));
        // Source-like value injected post-write.
        reject(&|value| {
            value["required_equal_features"][0]["values"] = serde_json::json!(["const x = 1;"])
        });
        // Semantics/values contradiction (equal-empty with non-empty values).
        reject(&|value| {
            value["required_equal_features"][0]["semantics"] = serde_json::json!("equal_empty")
        });
        // A prohibited-presence blocker tampered into the required axis. Index 3 of
        // the sample is an empty-valued characteristic, so only the axis check
        // fires (values stay empty).
        reject(&|value| {
            value["required_equal_features"][3]["semantics"] =
                serde_json::json!("prohibited_presence")
        });
        // A required binding tampered into the prohibited axis.
        reject(&|value| {
            value["prohibited_or_blocking_features"][0]["semantics"] = serde_json::json!("equal");
            value["prohibited_or_blocking_features"][0]["values"] = serde_json::json!(["svc"]);
        });
        // Unknown obligation class token.
        reject(&|value| value["unresolved_obligations"][0]["class"] = serde_json::json!("bogus"));
    }

    fn query_plan_details<P>(connection: &Connection, sql: &str, params: P) -> Vec<String>
    where
        P: rusqlite::Params,
    {
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("prepare query plan");
        statement
            .query_map(params, |row| row.get::<_, String>(3))
            .expect("query plan rows")
            .map(|row| row.expect("query plan detail"))
            .collect()
    }

    fn plan_uses_index(plan: &[String], index_name: &str) -> bool {
        plan.iter().any(|detail| detail.contains(index_name))
    }

    fn store_with_active_semantic_fact(
        name: &str,
    ) -> (TempWorkspace, SqliteIndexStore, GenerationHandle) {
        let workspace = TempWorkspace::new(name);
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        (workspace, store, generation)
    }

    fn activate_empty_generation(store: &SqliteIndexStore) -> GenerationHandle {
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        generation
    }

    #[test]
    fn active_repo_shape_stats_reports_indexed_inventory_without_supported_family_units() {
        let (_workspace, store, generation) =
            store_with_active_semantic_fact("sqlite-repo-shape-indexed-inventory");

        let stats = store
            .active_repo_shape_stats()
            .expect("active repo shape stats");

        assert_eq!(stats.generation_id, generation.generation_id);
        assert_eq!(stats.indexed_file_count, 1);
        assert_eq!(stats.indexed_code_unit_count, 1);
        assert_eq!(stats.semantic_fact_count, 1);
        assert_eq!(stats.eligible_code_units, 0);
        let tsjs = stats
            .by_language
            .iter()
            .find(|language| language.language == "typescript/javascript")
            .expect("tsjs language stats");
        assert_eq!(tsjs.indexed_file_count, 1);
        assert_eq!(tsjs.indexed_code_unit_count, 1);
        assert_eq!(tsjs.eligible_code_units, 0);
        assert_eq!(tsjs.family_count, 0);
    }

    #[test]
    fn prune_generations_preserves_active_and_removes_old_inactive() {
        let workspace = TempWorkspace::new("sqlite-prune-generations");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        let third = activate_empty_generation(&store);
        let fourth = activate_empty_generation(&store);

        let report = store
            .prune_generations(GenerationPruneRequest {
                keep_inactive: 1,
                dry_run: false,
            })
            .expect("prune generations");

        assert_eq!(report.active_generation, fourth.generation_id);
        assert_eq!(
            report.retained_inactive_generations,
            vec![third.generation_id.clone()]
        );
        assert_eq!(
            report.candidate_generations,
            vec![first.generation_id.clone(), second.generation_id.clone()]
        );
        assert_eq!(report.deleted_generations, report.candidate_generations);
        let connection = store
            .open_existing_generation(&fourth.generation_id)
            .expect("open mutable database");
        assert_eq!(
            generation_status(&connection, &first.generation_id).expect("first status"),
            None
        );
        assert_eq!(
            generation_status(&connection, &second.generation_id).expect("second status"),
            None
        );
        assert_eq!(
            generation_status(&connection, &third.generation_id)
                .expect("third status")
                .as_deref(),
            Some("validated")
        );
        assert_eq!(
            generation_status(&connection, &fourth.generation_id)
                .expect("fourth status")
                .as_deref(),
            Some("active")
        );
        assert!(store.mutable_database_path().is_file());
        assert!(!store.generations_dir().exists());

        let active = store
            .list_active_indexed_files()
            .expect("active generation remains readable");
        assert_eq!(active.generation_id, fourth.generation_id);
    }

    #[test]
    fn prune_generations_dry_run_does_not_remove_candidates() {
        let workspace = TempWorkspace::new("sqlite-prune-dry-run");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        let third = activate_empty_generation(&store);

        let report = store
            .prune_generations(GenerationPruneRequest {
                keep_inactive: 0,
                dry_run: true,
            })
            .expect("dry-run prune generations");

        assert_eq!(report.active_generation, third.generation_id);
        assert!(report.retained_inactive_generations.is_empty());
        assert_eq!(
            report.candidate_generations,
            vec![first.generation_id.clone(), second.generation_id.clone()]
        );
        assert!(report.deleted_generations.is_empty());
        let connection = store
            .open_existing_generation(&third.generation_id)
            .expect("open mutable database");
        for generation in [&first, &second, &third] {
            assert!(generation_status(&connection, &generation.generation_id)
                .expect("generation status")
                .is_some());
        }
    }

    #[test]
    fn storage_clean_removes_legacy_layout_prunes_inactive_and_compacts() {
        let workspace = TempWorkspace::new("sqlite-storage-clean");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        let third = activate_empty_generation(&store);
        fs::write(
            store.current_generation_path(),
            format!("{}\n", third.generation_id),
        )
        .expect("write legacy current pointer");
        fs::create_dir_all(store.generations_dir().join("gen-000100"))
            .expect("create legacy generation dir");
        fs::write(
            store
                .generations_dir()
                .join("gen-000100")
                .join("legacy.txt"),
            vec![b'x'; 1024 * 1024],
        )
        .expect("write legacy payload");

        let report = store
            .clean_storage(StorageCleanRequest { dry_run: false })
            .expect("clean storage");

        assert_eq!(report.active_generation, third.generation_id);
        assert!(!report.dry_run);
        assert!(report.legacy_layout.present_before);
        assert!(!report.legacy_layout.present_after);
        assert!(report.legacy_layout.removed);
        assert!(report.legacy_layout.bytes_before > 0);
        assert_eq!(report.legacy_layout.bytes_after, 0);
        assert_eq!(report.prune.keep_inactive, 0);
        assert_eq!(
            report.prune.candidate_generations,
            vec![first.generation_id.clone(), second.generation_id.clone()]
        );
        assert_eq!(
            report.prune.deleted_generations,
            report.prune.candidate_generations
        );
        assert!(report.total_bytes_before > report.total_bytes_after);
        assert!(!store.current_generation_path().exists());
        assert!(!store.generations_dir().exists());

        let connection = store
            .open_existing_generation(&third.generation_id)
            .expect("open mutable database");
        assert_eq!(
            generation_status(&connection, &third.generation_id)
                .expect("third status")
                .as_deref(),
            Some("active")
        );
        assert_eq!(
            generation_status(&connection, &first.generation_id).expect("first status"),
            None
        );
    }

    #[test]
    fn storage_clean_dry_run_reports_without_mutating() {
        let workspace = TempWorkspace::new("sqlite-storage-clean-dry-run");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        fs::write(
            store.current_generation_path(),
            format!("{}\n", second.generation_id),
        )
        .expect("write legacy current pointer");
        fs::create_dir_all(store.generations_dir().join("gen-000101"))
            .expect("create legacy generation dir");
        fs::write(
            store
                .generations_dir()
                .join("gen-000101")
                .join("legacy.txt"),
            "legacy bytes",
        )
        .expect("write legacy payload");

        let report = store
            .clean_storage(StorageCleanRequest { dry_run: true })
            .expect("dry-run clean storage");

        assert!(report.dry_run);
        assert!(report.legacy_layout.present_before);
        assert!(report.legacy_layout.present_after);
        assert!(!report.legacy_layout.removed);
        assert_eq!(
            report.prune.candidate_generations,
            vec![first.generation_id.clone()]
        );
        assert!(report.prune.deleted_generations.is_empty());
        assert_eq!(report.total_bytes_before, report.total_bytes_after);
        assert!(store.current_generation_path().exists());
        assert!(store.generations_dir().exists());

        let connection = store
            .open_existing_generation(&second.generation_id)
            .expect("open mutable database");
        assert!(generation_status(&connection, &first.generation_id)
            .expect("first status")
            .is_some());
    }

    #[test]
    fn storage_clean_refuses_legacy_only_layout_without_removing_it() {
        let workspace = TempWorkspace::new("sqlite-storage-clean-legacy-only");
        let store = store(&workspace);
        fs::write(store.current_generation_path(), "gen-000001\n")
            .expect("write legacy current pointer");
        fs::create_dir_all(store.generations_dir()).expect("create legacy generations dir");

        let error = store
            .clean_storage(StorageCleanRequest { dry_run: false })
            .expect_err("legacy-only layout must not be cleaned");

        let rendered_error = format!("{error:?}");
        assert!(
            rendered_error.contains("storage clean requires mutable SQLite storage")
                || rendered_error.contains("generation database")
                || rendered_error.contains("generation directory")
        );
        assert!(store.current_generation_path().exists());
        assert!(store.generations_dir().exists());
    }

    #[test]
    fn migrations_are_idempotent_and_create_required_tables() {
        let workspace = TempWorkspace::new("sqlite-migration");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");

        apply_migrations(&connection).expect("reapply migrations");

        assert!(all_required_tables_exist(&connection).expect("required tables"));
        let migration_count: u32 = connection
            .query_row("SELECT count(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count");
        assert_eq!(migration_count, 1);
    }

    #[test]
    fn family_records_round_trip_from_active_generation_without_leaks() {
        let (workspace, store, generation) = store_with_active_family("sqlite-family-round-trip");

        let families = store.list_active_families().expect("list active families");
        let summaries = store
            .list_active_family_summaries()
            .expect("list active family summaries");
        let member_candidates = store
            .find_active_families_by_member(&code_unit("src/a.ts").id)
            .expect("member candidates");
        let role_candidates = store
            .find_active_families_by_role("member", 5)
            .expect("role candidates");
        let path_candidates = store
            .find_active_families_by_evidence_path("src/a.ts", 5)
            .expect("path candidates");
        let detail = store
            .show_family(&family().family_id)
            .expect("show family")
            .expect("family exists");
        let missing = store.show_family("family:missing").expect("show missing");

        assert_eq!(families.generation_id, generation.generation_id);
        assert_eq!(families.families, vec![family()]);
        assert_eq!(summaries.generation_id, generation.generation_id);
        assert_eq!(summaries.families.len(), 1);
        assert_eq!(summaries.families[0].family_id, family().family_id);
        assert_eq!(
            summaries.families[0].classification,
            family().classification
        );
        assert_eq!(summaries.families[0].support, 2);
        // Prevalence metadata round-trips through the summary and detail reads.
        assert_eq!(summaries.families[0].prevalence, family().prevalence);
        assert_eq!(families.families[0].prevalence, family().prevalence);
        assert_eq!(member_candidates.generation_id, generation.generation_id);
        assert_eq!(
            member_candidates.candidates[0].family_id,
            family().family_id
        );
        assert!(!member_candidates.truncated);
        assert_eq!(role_candidates.candidates[0].family_id, family().family_id);
        assert!(!role_candidates.truncated);
        assert_eq!(path_candidates.candidates[0].family_id, family().family_id);
        assert!(!path_candidates.truncated);
        assert_eq!(detail.generation_id, generation.generation_id);
        assert_eq!(detail.family, family());
        assert_eq!(
            detail.members,
            vec![family_member("src/a.ts"), family_member("src/b.ts")]
        );
        assert_eq!(detail.variation_slots, vec![variation_slot()]);
        assert_eq!(
            detail.evidence,
            vec![family_evidence("src/a.ts"), family_evidence("src/b.ts")]
        );
        assert!(missing.is_none());
        assert!(!format!("{detail:?}").contains(&workspace.path().display().to_string()));
        assert!(!format!("{detail:?}").contains("const secret"));
    }

    /// Stamp the mutable database with an older schema version to simulate a
    /// pre-current index. The read gate and rebuild recreate key on this
    /// version, so a version downgrade exercises both migration paths.
    fn stamp_previous_schema_version(store: &SqliteIndexStore) {
        let connection =
            Connection::open(store.mutable_database_path()).expect("open mutable database");
        connection
            .execute(
                "UPDATE schema_migrations SET version = ?1",
                params![STORAGE_SCHEMA_VERSION - 1],
            )
            .expect("downgrade stored schema version");
    }

    #[test]
    fn outdated_schema_read_is_a_typed_resync_error_without_touching_active_generation() {
        let (_workspace, store, _generation) =
            store_with_active_family("sqlite-outdated-read-error");
        stamp_previous_schema_version(&store);

        let error = store
            .list_active_family_summaries()
            .expect_err("an outdated schema read must fail");
        assert!(matches!(error, StoreError::SchemaVersionOutdated(_)));

        // The read must not delete or rebuild: the active generation is intact.
        assert!(store.mutable_database_path().is_file());
        assert_eq!(
            stored_schema_version(
                &Connection::open(store.mutable_database_path()).expect("reopen database")
            )
            .expect("read stored version"),
            Some(STORAGE_SCHEMA_VERSION - 1)
        );

        // The application layer routes the typed error to the resync recovery
        // guidance from the recovery classifier vocabulary.
        let message = match crate::application::query::list_families(&store)
            .expect_err("list_families must surface the outdated schema error")
        {
            crate::error::RepoGrammarError::InvalidInput(message) => message,
            other => panic!("unexpected error: {other:?}"),
        };
        assert!(
            message.contains("resync"),
            "recovery guidance must recommend resync, got: {message}"
        );
    }

    #[test]
    fn outdated_schema_rebuild_recreates_a_working_current_state() {
        let (_workspace, store, _generation) = store_with_active_family("sqlite-outdated-rebuild");
        stamp_previous_schema_version(&store);

        // A full rebuild recreates the mutable database at the current schema.
        let generation = store
            .prepare_next_generation()
            .expect("rebuild recreates the mutable database");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }
        store
            .activate_generation(&generation)
            .expect("activate rebuilt generation");

        let detail = store
            .show_family(&family().family_id)
            .expect("show family")
            .expect("family exists after rebuild");
        assert_eq!(detail.family, family());
        assert_eq!(
            stored_schema_version(
                &Connection::open(store.mutable_database_path()).expect("reopen database")
            )
            .expect("read stored version"),
            Some(STORAGE_SCHEMA_VERSION)
        );
    }

    #[test]
    fn family_evidence_projection_lists_active_generation_rows() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-family-evidence-projection");

        let projection = store
            .list_active_family_evidence_projection()
            .expect("list family evidence projection");

        assert_eq!(projection.generation_id, generation.generation_id);
        assert_eq!(
            projection.rows,
            vec![
                IndexedFamilyEvidenceProjectionRecord {
                    family_id: family().family_id,
                    path: "src/a.ts".to_string(),
                    content_hash: file("src/a.ts").content_hash,
                },
                IndexedFamilyEvidenceProjectionRecord {
                    family_id: family().family_id,
                    path: "src/b.ts".to_string(),
                    content_hash: file("src/b.ts").content_hash,
                },
            ]
        );
    }

    #[test]
    fn family_search_summaries_project_bounded_source_free_metadata() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-family-search-summaries");

        let summaries = store
            .list_active_family_search_summaries()
            .expect("list family search summaries");

        assert_eq!(summaries.generation_id, generation.generation_id);
        assert_eq!(
            summaries.families,
            vec![IndexedFamilySearchSummaryRecord {
                family_id: family().family_id,
                language: "typescript".to_string(),
                code_unit_kind: "module".to_string(),
                framework_role: "member".to_string(),
                classification: family().classification,
                support: 2,
                prevalence: family().prevalence,
                // Distinct segments of "src/a.ts" and "src/b.ts", sorted.
                evidence_path_components: vec![
                    "a.ts".to_string(),
                    "b.ts".to_string(),
                    "src".to_string(),
                ],
            }]
        );

        // Source-freedom: no component may be absolute, a URL, or a source line.
        for family in &summaries.families {
            for component in &family.evidence_path_components {
                assert!(
                    !component.starts_with('/'),
                    "absolute component: {component}"
                );
                assert!(!component.contains("://"), "url component: {component}");
                assert!(
                    !component.contains('/'),
                    "unsplit path component: {component}"
                );
                assert!(
                    !component.contains('{')
                        && !component.contains('}')
                        && !component.contains("=>")
                        && !component.contains(';'),
                    "source-like component: {component}"
                );
            }
        }
        let rendered = format!("{summaries:?}");
        assert!(!rendered.contains("const secret"));
    }

    #[test]
    fn family_read_model_queries_use_read_path_indexes() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-family-read-indexes");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");

        let evidence_plan = query_plan_details(
            &connection,
            "SELECT evidence.evidence_id \
             FROM evidence \
             JOIN code_units \
               ON code_units.generation_id = evidence.generation_id \
              AND code_units.code_unit_id = evidence.code_unit_id \
             JOIN indexed_files \
               ON indexed_files.generation_id = evidence.generation_id \
              AND indexed_files.path = evidence.path \
             WHERE evidence.generation_id = ?1 AND evidence.family_id = ?2 \
             ORDER BY evidence.path COLLATE BINARY, evidence.start_byte, evidence.end_byte, \
                      evidence.code_unit_id COLLATE BINARY, evidence.evidence_id COLLATE BINARY",
            params![generation.generation_id.as_str(), family().family_id],
        );
        assert!(
            plan_uses_index(&evidence_plan, "idx_evidence_generation_family_order"),
            "unexpected evidence plan: {evidence_plan:?}"
        );

        let member_plan = query_plan_details(
            &connection,
            "SELECT family_id \
             FROM family_members \
             WHERE generation_id = ?1 AND code_unit_id = ?2 \
             ORDER BY family_id COLLATE BINARY",
            params![generation.generation_id.as_str(), code_unit("src/a.ts").id],
        );
        assert!(
            plan_uses_index(&member_plan, "idx_family_members_generation_code_unit"),
            "unexpected member plan: {member_plan:?}"
        );

        let path_plan = query_plan_details(
            &connection,
            "SELECT DISTINCT family_id \
             FROM evidence \
             WHERE generation_id = ?1 AND family_id IS NOT NULL AND path = ?2 \
             ORDER BY family_id COLLATE BINARY \
             LIMIT ?3",
            params![generation.generation_id.as_str(), "src/a.ts", 6_i64],
        );
        assert!(
            plan_uses_index(&path_plan, "idx_evidence_generation_path_family"),
            "unexpected path plan: {path_plan:?}"
        );
    }

    #[test]
    fn derived_dependencies_are_recorded_for_semantic_and_family_evidence() {
        let workspace = TempWorkspace::new("sqlite-derived-dependencies");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");
        let mut statement = connection
            .prepare(
                "SELECT record_kind, record_id, path, content_hash \
                 FROM derived_record_dependencies \
                 WHERE generation_id = ?1 \
                 ORDER BY record_kind, record_id, path",
            )
            .expect("prepare dependencies query");
        let rows = statement
            .query_map(params![generation.generation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .expect("query dependencies")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect dependencies");

        assert_eq!(
            rows,
            vec![
                (
                    "family".to_string(),
                    family().family_id,
                    "src/a.ts".to_string(),
                    file("src/a.ts").content_hash.as_str().to_string(),
                ),
                (
                    "family".to_string(),
                    family().family_id,
                    "src/b.ts".to_string(),
                    file("src/b.ts").content_hash.as_str().to_string(),
                ),
                (
                    "family_evidence".to_string(),
                    family_evidence("src/a.ts").evidence_id,
                    "src/a.ts".to_string(),
                    file("src/a.ts").content_hash.as_str().to_string(),
                ),
                (
                    "family_evidence".to_string(),
                    family_evidence("src/b.ts").evidence_id,
                    "src/b.ts".to_string(),
                    file("src/b.ts").content_hash.as_str().to_string(),
                ),
                (
                    "semantic_evidence".to_string(),
                    semantic_fact("src/a.ts").evidence_id,
                    "src/a.ts".to_string(),
                    file("src/a.ts").content_hash.as_str().to_string(),
                ),
                (
                    "semantic_fact".to_string(),
                    semantic_fact("src/a.ts").fact_id,
                    "src/a.ts".to_string(),
                    file("src/a.ts").content_hash.as_str().to_string(),
                ),
            ]
        );
        let inspection = store.inspect().expect("inspect storage");
        assert_eq!(inspection.dependency_record_count, Some(6));
        assert_eq!(inspection.dirty_record_count, Some(0));
    }

    #[test]
    fn unchanged_indexed_file_rewrite_preserves_path_records() {
        let workspace = TempWorkspace::new("sqlite-unchanged-path-rewrite");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");

        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("unchanged file rewrite is idempotent");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let code_unit_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM code_units WHERE generation_id = ?1",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("code unit count");
        let dependency_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM derived_record_dependencies WHERE generation_id = ?1",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("dependency count");
        let dirty_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM dirty_records WHERE generation_id = ?1",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("dirty count");

        assert_eq!(code_unit_count, 1);
        assert_eq!(dependency_count, 2);
        assert_eq!(dirty_count, 0);
    }

    #[test]
    fn replaced_indexed_file_removes_path_records_and_marks_dependents_dirty() {
        let workspace = TempWorkspace::new("sqlite-replaced-path-dirty");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }
        let mut changed_file = file("src/a.ts");
        changed_file.content_hash = ContentHash::new(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("valid hash");
        changed_file.size_bytes = 8;

        store
            .record_indexed_file(&generation, &changed_file)
            .expect("replace indexed file");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let stored_hash: String = connection
            .query_row(
                "SELECT content_hash FROM indexed_files \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("stored changed hash");
        assert_eq!(stored_hash, changed_file.content_hash.as_str());
        let removed_path_units: u32 = connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path code units");
        let retained_path_units: u32 = connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND path = 'src/b.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("retained path code units");
        let removed_path_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path evidence");
        let retained_path_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence \
                 WHERE generation_id = ?1 AND path = 'src/b.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("retained path evidence");
        let removed_path_dependencies: u32 = connection
            .query_row(
                "SELECT count(*) FROM derived_record_dependencies \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path dependencies");
        let dirty_rows = connection
            .prepare(
                "SELECT record_kind, record_id, reason \
                 FROM dirty_records \
                 WHERE generation_id = ?1 \
                 ORDER BY record_kind, record_id",
            )
            .expect("prepare dirty query")
            .query_map(params![generation.generation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("query dirty records")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect dirty rows");

        assert_eq!(removed_path_units, 0);
        assert_eq!(retained_path_units, 1);
        assert_eq!(removed_path_evidence, 0);
        assert_eq!(retained_path_evidence, 1);
        assert_eq!(removed_path_dependencies, 0);
        assert_eq!(
            dirty_rows,
            vec![
                (
                    "family".to_string(),
                    family().family_id,
                    "path_replaced".to_string(),
                ),
                (
                    "family_evidence".to_string(),
                    family_evidence("src/a.ts").evidence_id,
                    "path_replaced".to_string(),
                ),
                (
                    "semantic_evidence".to_string(),
                    semantic_fact("src/a.ts").evidence_id,
                    "path_replaced".to_string(),
                ),
                (
                    "semantic_fact".to_string(),
                    semantic_fact("src/a.ts").fact_id,
                    "path_replaced".to_string(),
                ),
            ]
        );
        let error = store
            .validate_generation(&generation)
            .expect_err("dirty replacement must block activation");
        assert!(format!("{error:?}").contains("dirty records"));
    }

    #[test]
    fn missing_indexed_file_removal_is_idempotent() {
        let workspace = TempWorkspace::new("sqlite-missing-path-removal");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");

        store
            .remove_indexed_file(&generation, "src/missing.ts")
            .expect("missing file removal is idempotent");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let indexed_file_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM indexed_files WHERE generation_id = ?1",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("indexed file count");
        let dirty_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM dirty_records WHERE generation_id = ?1",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("dirty count");

        assert_eq!(indexed_file_count, 1);
        assert_eq!(dirty_count, 0);
    }

    #[test]
    fn removed_indexed_file_cascades_path_records_and_marks_dependents_dirty() {
        let workspace = TempWorkspace::new("sqlite-removed-path-dirty");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }

        store
            .remove_indexed_file(&generation, "src/a.ts")
            .expect("remove indexed file");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let removed_file_rows: u32 = connection
            .query_row(
                "SELECT count(*) FROM indexed_files \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed file rows");
        let retained_file_rows: u32 = connection
            .query_row(
                "SELECT count(*) FROM indexed_files \
                 WHERE generation_id = ?1 AND path = 'src/b.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("retained file rows");
        let removed_path_units: u32 = connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path code units");
        let retained_path_units: u32 = connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND path = 'src/b.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("retained path code units");
        let removed_path_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path evidence");
        let retained_path_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence \
                 WHERE generation_id = ?1 AND path = 'src/b.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("retained path evidence");
        let removed_path_dependencies: u32 = connection
            .query_row(
                "SELECT count(*) FROM derived_record_dependencies \
                 WHERE generation_id = ?1 AND path = 'src/a.ts'",
                params![generation.generation_id],
                |row| row.get(0),
            )
            .expect("removed path dependencies");
        let dirty_rows = connection
            .prepare(
                "SELECT record_kind, record_id, reason \
                 FROM dirty_records \
                 WHERE generation_id = ?1 \
                 ORDER BY record_kind, record_id",
            )
            .expect("prepare dirty query")
            .query_map(params![generation.generation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("query dirty records")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect dirty rows");

        assert_eq!(removed_file_rows, 0);
        assert_eq!(retained_file_rows, 1);
        assert_eq!(removed_path_units, 0);
        assert_eq!(retained_path_units, 1);
        assert_eq!(removed_path_evidence, 0);
        assert_eq!(retained_path_evidence, 1);
        assert_eq!(removed_path_dependencies, 0);
        assert_eq!(
            dirty_rows,
            vec![
                (
                    "family".to_string(),
                    family().family_id,
                    "path_removed".to_string(),
                ),
                (
                    "family_evidence".to_string(),
                    family_evidence("src/a.ts").evidence_id,
                    "path_removed".to_string(),
                ),
                (
                    "semantic_evidence".to_string(),
                    semantic_fact("src/a.ts").evidence_id,
                    "path_removed".to_string(),
                ),
                (
                    "semantic_fact".to_string(),
                    semantic_fact("src/a.ts").fact_id,
                    "path_removed".to_string(),
                ),
            ]
        );
        let error = store
            .validate_generation(&generation)
            .expect_err("dirty removal must block activation");
        assert!(format!("{error:?}").contains("dirty records"));
    }

    #[test]
    fn rerecording_derived_dependencies_clears_dirty_and_allows_activation() {
        let workspace = TempWorkspace::new("sqlite-rerecord-clears-dirty");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let new_hash = ContentHash::new(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("valid hash");

        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_indexed_file(&generation, &file(path))
                .expect("record file");
            store
                .record_code_unit(&generation, &code_unit(path))
                .expect("record code unit");
        }
        store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        store
            .record_family(&generation, &family())
            .expect("record family");
        for path in ["src/a.ts", "src/b.ts"] {
            store
                .record_family_member(&generation, &family_member(path))
                .expect("record family member");
            store
                .record_family_evidence(&generation, &family_evidence(path))
                .expect("record family evidence");
        }

        // Replacing src/a.ts cascades away its derived records and marks the
        // semantic-fact and family-evidence dependencies dirty.
        let mut changed_file = file("src/a.ts");
        changed_file.content_hash = new_hash.clone();
        store
            .record_indexed_file(&generation, &changed_file)
            .expect("replace indexed file");
        // The dirty markers block validation until the records are re-derived.
        let blocked = store
            .validate_generation(&generation)
            .expect_err("dirty records must block validation");
        assert!(format!("{blocked:?}").contains("dirty records"));

        // Re-recording src/a.ts against the new hash clears the dirty markers.
        let mut new_unit = code_unit("src/a.ts");
        new_unit.content_hash = new_hash.clone();
        store
            .record_code_unit(&generation, &new_unit)
            .expect("re-record code unit");
        let mut new_fact = semantic_fact("src/a.ts");
        new_fact.content_hash = new_hash.clone();
        store
            .record_semantic_fact(&generation, &new_fact)
            .expect("re-record semantic fact");
        store
            .record_family_member(&generation, &family_member("src/a.ts"))
            .expect("re-record family member");
        let mut new_evidence = family_evidence("src/a.ts");
        new_evidence.content_hash = new_hash.clone();
        store
            .record_family_evidence(&generation, &new_evidence)
            .expect("re-record family evidence");

        store
            .validate_generation(&generation)
            .expect("re-recorded generation validates");
        store
            .activate_generation(&generation)
            .expect("re-recorded generation activates");
    }

    #[test]
    fn apply_migrations_rejects_a_newer_schema_version() {
        let workspace = TempWorkspace::new("sqlite-future-schema");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name, applied_at) \
                 VALUES (?1, 'future', datetime('now'))",
                params![STORAGE_SCHEMA_VERSION + 1],
            )
            .expect("stamp a newer schema version");
        let error =
            apply_migrations(&connection).expect_err("a newer schema version must be rejected");
        assert!(format!("{error:?}").contains("newer"));
    }

    #[test]
    fn dirty_records_block_active_family_reads() {
        let (_workspace, store, generation) = store_with_active_family("sqlite-dirty-family-read");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "INSERT INTO dirty_records \
                 (generation_id, record_kind, record_id, reason, marked_at_generation_id) \
                 VALUES (?1, 'family', ?2, 'stale_dependency', ?1)",
                params![generation.generation_id, family().family_id],
            )
            .expect("insert dirty family");

        let inspection = store.inspect().expect("inspect storage");
        assert_eq!(inspection.dirty_record_count, Some(1));
        let error = store
            .list_active_families()
            .expect_err("dirty active family must be refused");
        assert!(format!("{error:?}").contains("dirty records"));
    }

    #[test]
    fn dependency_hash_mismatch_blocks_full_snapshot_reads_but_not_summary_inventory() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-stale-family-dependency");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "UPDATE derived_record_dependencies \
                 SET content_hash = ?1 \
                 WHERE generation_id = ?2 AND record_kind = 'family'",
                params![
                    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    generation.generation_id,
                ],
            )
            .expect("tamper family dependency");

        let summaries = store
            .list_active_family_summaries()
            .expect("summary inventory remains source-free");
        assert_eq!(summaries.families[0].family_id, family().family_id);

        let error = store
            .load_active_claim_input_snapshot()
            .expect_err("stale dependency must be refused by full snapshot reads");
        assert!(format!("{error:?}").contains("derived record dependencies"));
    }

    #[test]
    fn family_records_require_building_generation() {
        let (_workspace, store, generation) = store_with_active_family("sqlite-family-immutable");

        for error in [
            store
                .record_family(&generation, &family())
                .expect_err("active family write fails"),
            store
                .record_family_member(&generation, &family_member("src/a.ts"))
                .expect_err("active member write fails"),
            store
                .record_variation_slot(&generation, &variation_slot())
                .expect_err("active slot write fails"),
            store
                .record_family_evidence(&generation, &family_evidence("src/a.ts"))
                .expect_err("active evidence write fails"),
        ] {
            assert!(format!("{error:?}").contains("building generations"));
        }
    }

    #[test]
    fn family_evidence_must_match_same_generation_code_unit() {
        let workspace = TempWorkspace::new("sqlite-family-evidence-validation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_family(&generation, &family())
            .expect("record family");

        let mut wrong_hash = family_evidence("src/a.ts");
        wrong_hash.content_hash = ContentHash::new(
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .expect("valid hash");
        let error = store
            .record_family_evidence(&generation, &wrong_hash)
            .expect_err("wrong hash");
        assert!(format!("{error:?}").contains("content hash"));

        let mut wrong_path = family_evidence("src/a.ts");
        wrong_path.path = "src/other.ts".to_string();
        let error = store
            .record_family_evidence(&generation, &wrong_path)
            .expect_err("wrong path");
        assert!(format!("{error:?}").contains("path"));

        let mut out_of_range = family_evidence("src/a.ts");
        out_of_range.end_byte = 41;
        let error = store
            .record_family_evidence(&generation, &out_of_range)
            .expect_err("out of range");
        assert!(format!("{error:?}").contains("range"));

        let mut reversed_range = family_evidence("src/a.ts");
        reversed_range.start_byte = 6;
        reversed_range.end_byte = 5;
        let error = store
            .record_family_evidence(&generation, &reversed_range)
            .expect_err("reversed range");
        assert!(format!("{error:?}").contains("range"));

        let mut missing_family = family_evidence("src/a.ts");
        missing_family.family_id = "family:missing".to_string();
        let error = store
            .record_family_evidence(&generation, &missing_family)
            .expect_err("missing family");
        assert!(format!("{error:?}").contains("same generation"));

        let mut unsupported_claim = family_evidence("src/a.ts");
        unsupported_claim.covered_claims = vec!["canonical".to_string(), "runtime".to_string()];
        let error = store
            .record_family_evidence(&generation, &unsupported_claim)
            .expect_err("unsupported covered claim");
        assert!(format!("{error:?}").contains("covered claim"));
    }

    #[test]
    fn generation_validation_rejects_malformed_family_evidence_covered_claims() {
        let workspace = TempWorkspace::new("sqlite-family-evidence-coverage-validation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_family(&generation, &family())
            .expect("record family");
        store
            .record_family_evidence(&generation, &family_evidence("src/a.ts"))
            .expect("record family evidence");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open building generation");
        connection
            .execute(
                "UPDATE evidence SET covered_claims_json = '[\"canonical\",\"runtime\"]' \
                 WHERE generation_id = ?1 AND family_id = ?2",
                params![generation.generation_id, family().family_id],
            )
            .expect("tamper family evidence coverage");

        let error = store
            .validate_generation(&generation)
            .expect_err("malformed family coverage must fail validation");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn validation_rejects_non_unknown_family_without_evidence() {
        let workspace = TempWorkspace::new("sqlite-family-no-evidence");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_family(&generation, &family())
            .expect("record family");

        let error = store
            .validate_generation(&generation)
            .expect_err("family claim without evidence fails validation");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn list_active_family_reads_reject_tampered_rows() {
        let (_workspace, store, generation) = store_with_active_family("sqlite-family-tamper");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");

        connection
            .execute(
                "UPDATE evidence SET content_hash = ?1 WHERE generation_id = ?2 AND family_id = ?3",
                params![
                    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    generation.generation_id,
                    family().family_id,
                ],
            )
            .expect("tamper family evidence");

        let error = store
            .show_family(&family().family_id)
            .expect_err("tampered family evidence is rejected");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn show_family_validates_family_evidence_payloads() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-family-list-validates-evidence");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");

        connection
            .execute(
                "UPDATE evidence SET note = 'file:///tmp/secret' \
                 WHERE generation_id = ?1 AND family_id = ?2",
                params![generation.generation_id, family().family_id],
            )
            .expect("tamper family evidence note");

        let error = store
            .show_family(&family().family_id)
            .expect_err("tampered family evidence payload is rejected by detail read");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn show_family_validates_family_evidence_covered_claims() {
        let (_workspace, store, generation) =
            store_with_active_family("sqlite-family-list-validates-evidence-claims");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");

        connection
            .execute(
                "UPDATE evidence SET covered_claims_json = '[\"canonical\",\"runtime\"]' \
                 WHERE generation_id = ?1 AND family_id = ?2",
                params![generation.generation_id, family().family_id],
            )
            .expect("tamper family evidence coverage");

        let error = store
            .show_family(&family().family_id)
            .expect_err("tampered family evidence coverage is rejected by detail read");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn semantic_fact_evidence_must_not_be_family_bound() {
        let (_workspace, store, generation) =
            store_with_active_semantic_fact("sqlite-semantic-family-bound-evidence");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");

        connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, ?2, 'DOMINANT_PATTERN', 2, 2, 1.0, 0, 0, 0, 0, \
                         'coverage 2/2 with no competing ready family')",
                params![generation.generation_id, family().family_id],
            )
            .expect("insert tamper family");
        connection
            .execute(
                "UPDATE evidence SET family_id = ?1 WHERE generation_id = ?2",
                params![family().family_id, generation.generation_id],
            )
            .expect("tamper semantic evidence family link");

        let error = store
            .list_active_semantic_facts()
            .expect_err("family-bound semantic evidence is rejected");

        let error_text = format!("{error:?}");
        assert!(
            error_text.contains("semantic fact evidence") || error_text.contains("family evidence")
        );
    }

    #[test]
    fn generation_id_space_exhaustion_is_reported_without_creating_invalid_ids() {
        let workspace = TempWorkspace::new("sqlite-generation-exhausted");
        let store = store(&workspace);
        let connection = store
            .open_mutable_database(MissingDatabase::Allowed)
            .expect("open mutable database");
        apply_migrations(&connection).expect("apply migrations");
        connection
            .execute(
                "INSERT INTO index_generations \
                 (generation_id, status, created_at, repogrammar_version) \
                 VALUES ('gen-999999', 'validated', datetime('now'), ?1)",
                params![env!("CARGO_PKG_VERSION")],
            )
            .expect("insert max generation");

        let error = store
            .prepare_next_generation()
            .expect_err("exhausted generation ids must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        let count: u32 = connection
            .query_row(
                "SELECT count(*) FROM index_generations WHERE generation_id = 'gen-1000000'",
                [],
                |row| row.get(0),
            )
            .expect("invalid generation count");
        assert_eq!(count, 0);
    }

    #[test]
    fn pragmas_are_applied_to_generation_database() {
        let workspace = TempWorkspace::new("sqlite-pragmas");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let inspection = store.inspect().expect("inspect storage");

        assert_eq!(inspection.active_generation, Some("gen-000001".to_string()));
        assert_eq!(inspection.schema_version, Some(STORAGE_SCHEMA_VERSION));
        assert_eq!(inspection.dependency_record_count, Some(0));
        assert_eq!(inspection.dirty_record_count, Some(0));
        assert_eq!(inspection.journal_mode.as_deref(), Some("wal"));
        assert_eq!(inspection.foreign_keys_enabled, Some(true));
        assert_eq!(inspection.busy_timeout_ms, Some(5_000));
        assert_eq!(inspection.temp_store.as_deref(), Some("memory"));
        assert_eq!(inspection.integrity_check.as_deref(), Some("ok"));
    }

    #[test]
    fn compact_dry_run_reports_sizes_without_mutating_database() {
        let workspace = TempWorkspace::new("sqlite-compact-dry-run");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let database_size = fs::metadata(store.mutable_database_path())
            .expect("database metadata")
            .len();

        let report = store
            .compact_storage(IndexCompactRequest { dry_run: true })
            .expect("dry-run compact");

        assert_eq!(report.active_generation, generation.generation_id);
        assert!(report.dry_run);
        assert_eq!(report.before, report.after);
        assert!(report.before.database_bytes >= database_size);
        assert_eq!(
            fs::metadata(store.mutable_database_path())
                .expect("database metadata after dry-run")
                .len(),
            database_size
        );
    }

    #[test]
    fn compact_yes_reports_size_effects_and_preserves_active_generation() {
        let workspace = TempWorkspace::new("sqlite-compact-yes");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let report = store
            .compact_storage(IndexCompactRequest { dry_run: false })
            .expect("compact storage");

        assert_eq!(report.active_generation, generation.generation_id);
        assert!(!report.dry_run);
        assert!(report.before.total_bytes >= report.before.database_bytes);
        assert!(report.after.total_bytes >= report.after.database_bytes);
        assert_eq!(report.after.wal_bytes, 0);
        let inspection = store.inspect().expect("inspect after compact");
        assert_eq!(inspection.active_generation, Some(report.active_generation));
        assert_eq!(inspection.integrity_check.as_deref(), Some("ok"));
    }

    #[test]
    fn compact_refuses_dirty_active_generation() {
        let workspace = TempWorkspace::new("sqlite-compact-dirty");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "INSERT INTO dirty_records \
                 (generation_id, record_kind, record_id, reason, marked_at_generation_id) \
                 VALUES (?1, 'index', 'compact-test', 'stale_dependency', ?1)",
                params![generation.generation_id],
            )
            .expect("insert dirty record");

        let error = store
            .compact_storage(IndexCompactRequest { dry_run: false })
            .expect_err("dirty generation must block compact");

        assert!(format!("{error:?}").contains("dirty records"));
    }

    #[test]
    fn post_commit_maintenance_runs_optimize_and_passive_wal_checkpoint() {
        let workspace = TempWorkspace::new("sqlite-post-commit-maintenance");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        let (busy, log_frames, checkpointed_frames) =
            run_post_commit_maintenance(&connection).expect("post-commit maintenance");

        assert_eq!(
            active_generation_id(&connection).expect("active generation"),
            Some(generation.generation_id)
        );
        assert!(
            busy > 0 || checkpointed_frames <= log_frames,
            "passive WAL checkpoint reported impossible frame counts: busy={busy}, log={log_frames}, checkpointed={checkpointed_frames}"
        );
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let workspace = TempWorkspace::new("sqlite-fk");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");

        let error = connection
            .execute(
                "INSERT INTO indexed_files \
                 (generation_id, path, content_hash, size_bytes, language) \
                 VALUES ('gen-999999', 'src/a.ts', ?1, 1, 'typescript')",
                params![file("src/a.ts").content_hash.as_str()],
            )
            .expect_err("foreign key must reject missing generation");

        assert!(error.to_string().contains("FOREIGN KEY"));
    }

    #[test]
    fn generation_activation_preserves_indexed_file_metadata() {
        let workspace = TempWorkspace::new("sqlite-activate");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");

        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let inspection = store.inspect().expect("inspect storage");
        assert_eq!(inspection.active_generation, Some(generation.generation_id));

        let connection = store.open_generation("gen-000001").expect("open active");
        let count: u32 = connection
            .query_row("SELECT count(*) FROM indexed_files", [], |row| row.get(0))
            .expect("file count");
        assert_eq!(count, 1);
    }

    #[test]
    fn list_active_indexed_files_returns_sorted_active_metadata_without_leaks() {
        let workspace = TempWorkspace::new("sqlite-list-files");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/b.ts"))
            .expect("record b");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record a");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let report = store
            .list_active_indexed_files()
            .expect("list indexed files");

        assert_eq!(report.generation_id, "gen-000001");
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["src/a.ts", "src/b.ts"]
        );
        for file in report.files {
            assert!(!file
                .path
                .contains(workspace.path().to_string_lossy().as_ref()));
            assert!(!file.path.contains("UNIQUE_SOURCE_SENTINEL"));
            assert!(file.content_hash.as_str().starts_with("sha256:"));
        }
    }

    #[test]
    fn active_reads_ignore_building_generation_until_activation() {
        let workspace = TempWorkspace::new("sqlite-active-read-building-hidden");
        let store = store(&workspace);
        let first = store.prepare_next_generation().expect("prepare first");
        store
            .record_indexed_file(&first, &file("src/old.ts"))
            .expect("record old file");
        store.activate_generation(&first).expect("activate first");

        let second = store.prepare_next_generation().expect("prepare second");
        store
            .record_indexed_file(&second, &file("src/new.ts"))
            .expect("record new file");

        let before_activation = store
            .list_active_indexed_files()
            .expect("list active before activation");
        assert_eq!(before_activation.generation_id, first.generation_id);
        assert_eq!(before_activation.files[0].path, "src/old.ts");

        store
            .activate_generation(&second)
            .expect("activate second generation");

        let after_activation = store
            .list_active_indexed_files()
            .expect("list active after activation");
        assert_eq!(after_activation.generation_id, second.generation_id);
        assert_eq!(after_activation.files[0].path, "src/new.ts");
    }

    #[test]
    fn list_active_code_units_returns_sorted_active_units_without_leaks() {
        let workspace = TempWorkspace::new("sqlite-list-units");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut a_file = file("src/a.ts");
        a_file.size_bytes = 100;
        let mut b_file = file("src/b.ts");
        b_file.size_bytes = 100;
        store
            .record_indexed_file(&generation, &b_file)
            .expect("record b");
        store
            .record_indexed_file(&generation, &a_file)
            .expect("record a");

        let mut b_unit = code_unit("src/b.ts");
        b_unit.id = "unit:src/b.ts#module:0-10".to_string();
        b_unit.start_byte = 0;
        b_unit.end_byte = 10;
        let mut later_a_unit = code_unit("src/a.ts");
        later_a_unit.id = "unit:src/a.ts#function:10-20".to_string();
        later_a_unit.kind = "function".to_string();
        later_a_unit.start_byte = 10;
        later_a_unit.end_byte = 20;
        let mut first_a_unit = code_unit("src/a.ts");
        first_a_unit.id = "unit:src/a.ts#module:0-10".to_string();
        first_a_unit.start_byte = 0;
        first_a_unit.end_byte = 10;
        for unit in [&b_unit, &later_a_unit, &first_a_unit] {
            store
                .record_code_unit(&generation, unit)
                .expect("record unit");
        }
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let report = store.list_active_code_units().expect("list code units");

        assert_eq!(report.generation_id, "gen-000001");
        assert_eq!(
            report
                .units
                .iter()
                .map(|unit| unit.id.as_str())
                .collect::<Vec<_>>(),
            [
                "unit:src/a.ts#module:0-10",
                "unit:src/a.ts#function:10-20",
                "unit:src/b.ts#module:0-10"
            ]
        );
        for unit in report.units {
            let workspace_path = workspace.path().to_string_lossy();
            assert!(!unit.id.contains(workspace_path.as_ref()));
            assert!(!unit.path.contains(workspace_path.as_ref()));
            assert!(!unit.id.contains("UNIQUE_SOURCE_SENTINEL"));
            assert!(unit.content_hash.as_str().starts_with("sha256:"));
        }
    }

    #[test]
    fn list_active_semantic_facts_returns_sorted_active_facts_without_leaks() {
        let workspace = TempWorkspace::new("sqlite-list-semantic-facts");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut a_file = file("src/a.ts");
        a_file.size_bytes = 100;
        let mut b_file = file("src/b.ts");
        b_file.size_bytes = 100;
        store
            .record_indexed_file(&generation, &b_file)
            .expect("record b file");
        store
            .record_indexed_file(&generation, &a_file)
            .expect("record a file");
        store
            .record_code_unit(&generation, &code_unit("src/b.ts"))
            .expect("record b unit");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record a unit");
        let mut b_fact = semantic_fact("src/b.ts");
        b_fact.fact_id = "fact:src/b.ts#import:zod".to_string();
        b_fact.subject = "src/b.ts#import:zod".to_string();
        b_fact.target = Some("node_modules/zod/index.d.ts#z".to_string());
        b_fact.assumptions = vec!["path alias resolved from tsconfig".to_string()];
        store
            .record_semantic_fact(&generation, &b_fact)
            .expect("record b fact");
        let mut a_fact = semantic_fact("src/a.ts");
        a_fact.target = None;
        store
            .record_semantic_fact(&generation, &a_fact)
            .expect("record a fact");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let report = store
            .list_active_semantic_facts()
            .expect("list semantic facts");

        assert_eq!(report.generation_id, "gen-000001");
        assert_eq!(
            report
                .facts
                .iter()
                .map(|fact| fact.fact_id.as_str())
                .collect::<Vec<_>>(),
            ["fact:src/a.ts#import:express", "fact:src/b.ts#import:zod"]
        );
        assert_eq!(report.facts[0].target, None);
        assert_eq!(
            report.facts[1].assumptions,
            vec!["path alias resolved from tsconfig"]
        );
        let workspace_path = workspace.path().to_string_lossy();
        for fact in report.facts {
            for value in [
                fact.fact_id,
                fact.subject,
                fact.target.unwrap_or_default(),
                fact.origin_engine,
                fact.origin_engine_version,
                fact.origin_method,
                fact.evidence_id,
                fact.code_unit_id,
                fact.path,
                fact.note,
            ] {
                assert!(!value.contains(workspace_path.as_ref()));
                assert!(!value.contains("UNIQUE_SOURCE_SENTINEL"));
            }
            for assumption in fact.assumptions {
                assert!(!assumption.contains(workspace_path.as_ref()));
                assert!(!assumption.contains("UNIQUE_SOURCE_SENTINEL"));
            }
        }
    }

    #[test]
    fn load_active_claim_input_snapshot_returns_same_generation_records_without_leaks() {
        let workspace = TempWorkspace::new("sqlite-active-claim-input-snapshot");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut a_file = file("src/a.ts");
        a_file.size_bytes = 100;
        let mut b_file = file("src/b.ts");
        b_file.size_bytes = 100;
        store
            .record_indexed_file(&generation, &b_file)
            .expect("record b file");
        store
            .record_indexed_file(&generation, &a_file)
            .expect("record a file");
        let mut module = code_unit("src/a.ts");
        module.id = "unit:src/a.ts#module:0-100".to_string();
        module.end_byte = 100;
        let mut function = code_unit("src/a.ts");
        function.id = "unit:src/a.ts#function:10-40".to_string();
        function.kind = "function".to_string();
        function.start_byte = 10;
        function.end_byte = 40;
        store
            .record_code_unit(&generation, &function)
            .expect("record function");
        store
            .record_code_unit(&generation, &module)
            .expect("record module");
        let function_node = ir_node(&function);
        let module_node = ir_node(&module);
        store
            .record_ir_node(&generation, &function_node)
            .expect("record function node");
        store
            .record_ir_node(&generation, &module_node)
            .expect("record module node");
        store
            .record_ir_edge(&generation, &ir_edge(&module_node, &function_node))
            .expect("record IR edge");
        let mut fact = semantic_fact("src/a.ts");
        fact.code_unit_id = function.id.clone();
        fact.start_byte = function.start_byte;
        fact.end_byte = function.end_byte;
        fact.evidence_id = "evidence:fact:src/a.ts#function:express".to_string();
        store
            .record_semantic_fact(&generation, &fact)
            .expect("record semantic fact");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let snapshot = store
            .load_active_claim_input_snapshot()
            .expect("load claim input snapshot");

        assert_eq!(snapshot.generation_id, "gen-000001");
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["src/a.ts", "src/b.ts"]
        );
        assert_eq!(
            snapshot
                .units
                .iter()
                .map(|unit| unit.id.as_str())
                .collect::<Vec<_>>(),
            ["unit:src/a.ts#module:0-100", "unit:src/a.ts#function:10-40"]
        );
        assert_eq!(
            snapshot
                .ir_nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [
                "ir:unit:src/a.ts#function:10-40",
                "ir:unit:src/a.ts#module:0-100"
            ]
        );
        assert_eq!(
            snapshot.ir_edges,
            vec![ir_edge(&module_node, &function_node)]
        );
        assert_eq!(snapshot.semantic_facts, vec![fact]);

        let debug = format!("{snapshot:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("UNIQUE_SOURCE_SENTINEL"));
        assert!(!debug.contains("DOMINANT_PATTERN"));
        assert!(!debug.contains("CONFORMS"));
    }

    #[test]
    fn load_active_claim_input_snapshot_returns_empty_file_manifest_only_generation() {
        let workspace = TempWorkspace::new("sqlite-active-claim-input-empty");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("README.md"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let snapshot = store
            .load_active_claim_input_snapshot()
            .expect("load claim input snapshot");

        assert_eq!(snapshot.generation_id, "gen-000001");
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["README.md"]
        );
        assert!(snapshot.units.is_empty());
        assert!(snapshot.ir_nodes.is_empty());
        assert!(snapshot.ir_edges.is_empty());
        assert!(snapshot.semantic_facts.is_empty());
    }

    #[test]
    fn list_active_reads_reject_missing_active_generation() {
        let workspace = TempWorkspace::new("sqlite-list-no-active");
        let store = store(&workspace);
        store.prepare_next_generation().expect("prepare generation");

        let files_error = store
            .list_active_indexed_files()
            .expect_err("missing active generation must fail file reads");
        let units_error = store
            .list_active_code_units()
            .expect_err("missing active generation must fail unit reads");
        let facts_error = store
            .list_active_semantic_facts()
            .expect_err("missing active generation must fail semantic-fact reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("missing active generation must fail snapshot reads");

        assert!(matches!(files_error, IndexStoreError::InvalidState(_)));
        assert!(matches!(units_error, IndexStoreError::InvalidState(_)));
        assert!(matches!(facts_error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn list_active_reads_validate_schema_before_returning_records() {
        let workspace = TempWorkspace::new("sqlite-list-invalid-schema");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        connection
            .execute("DROP TABLE evidence", [])
            .expect("drop required table");

        let error = store
            .list_active_indexed_files()
            .expect_err("malformed active schema must fail reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("malformed active schema must fail snapshot reads");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn list_active_semantic_facts_rejects_tampered_rows() {
        fn assert_rejected(name: &str, tamper: impl FnOnce(&Connection, &GenerationHandle)) {
            let (_workspace, store, generation) = store_with_active_semantic_fact(name);
            let connection = store
                .open_existing_generation(&generation.generation_id)
                .expect("open active generation");
            tamper(&connection, &generation);

            let error = store
                .list_active_semantic_facts()
                .expect_err("tampered semantic fact must fail reads");
            let snapshot_error = store
                .load_active_claim_input_snapshot()
                .expect_err("tampered semantic fact must fail snapshot reads");

            assert!(matches!(error, IndexStoreError::InvalidState(_)));
            assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
        }

        assert_rejected("sqlite-list-semantic-fact-bad-kind", |connection, _| {
            connection
                .execute("UPDATE semantic_facts SET kind = 'CALL'", [])
                .expect("tamper fact kind");
        });
        assert_rejected(
            "sqlite-list-semantic-fact-bad-certainty",
            |connection, _| {
                connection
                    .execute("UPDATE semantic_facts SET certainty = 'LOW_CONFIDENCE'", [])
                    .expect("tamper fact certainty");
            },
        );
        assert_rejected(
            "sqlite-list-semantic-fact-bad-assumptions",
            |connection, _| {
                connection
                    .execute("UPDATE semantic_facts SET assumptions_json = '{}'", [])
                    .expect("tamper assumptions JSON");
            },
        );
        assert_rejected(
            "sqlite-list-semantic-fact-hash-mismatch",
            |connection, _| {
                connection
                    .execute(
                        "UPDATE evidence SET content_hash = ?1",
                        params![
                        "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                    ],
                    )
                    .expect("tamper evidence hash");
            },
        );
        assert_rejected(
            "sqlite-list-semantic-fact-range-mismatch",
            |connection, _| {
                connection
                    .execute("UPDATE evidence SET end_byte = 11", [])
                    .expect("tamper evidence range");
            },
        );
    }

    #[test]
    fn list_active_reads_reject_tampered_file_records() {
        let workspace = TempWorkspace::new("sqlite-list-invalid-file-record");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        connection
            .execute(
                "UPDATE indexed_files SET content_hash = 'sha256:test' WHERE path = 'src/a.ts'",
                [],
            )
            .expect("tamper file hash");

        let error = store
            .list_active_indexed_files()
            .expect_err("tampered file hash must fail reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("tampered file hash must fail snapshot reads");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn list_active_reads_reject_tampered_code_unit_records() {
        let workspace = TempWorkspace::new("sqlite-list-invalid-code-unit-record");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut file = file("src/a.ts");
        file.size_bytes = 20;
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record unit");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        connection
            .execute(
                "UPDATE code_units SET content_hash = ?1, end_byte = 21",
                params!["sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"],
            )
            .expect("tamper unit");

        let error = store
            .list_active_code_units()
            .expect_err("tampered unit hash/range must fail reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("tampered unit hash/range must fail snapshot reads");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn list_active_reads_reject_code_unit_ids_that_do_not_match_paths() {
        let workspace = TempWorkspace::new("sqlite-list-invalid-code-unit-id");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record unit");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        connection
            .execute(
                "UPDATE code_units SET code_unit_id = 'unit:/Users/example/repo/src/a.ts#module:0-10'",
                [],
            )
            .expect("tamper unit id");

        let error = store
            .list_active_code_units()
            .expect_err("tampered unit id must fail reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("tampered unit id must fail snapshot reads");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn code_unit_records_are_persisted_without_source_text_or_absolute_paths() {
        let workspace = TempWorkspace::new("sqlite-code-units");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut file = file("src/a.ts");
        file.size_bytes = 64;
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        let mut unit = code_unit("src/a.ts");
        unit.kind = "function".to_string();
        unit.start_byte = 7;
        unit.end_byte = 24;

        store
            .record_code_unit(&generation, &unit)
            .expect("record code unit");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let row = connection
            .query_row(
                "SELECT code_unit_id, path, language, kind, start_byte, end_byte, content_hash \
                 FROM code_units",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .expect("query code unit");
        assert_eq!(row.0, unit.id);
        assert_eq!(row.1, "src/a.ts");
        assert_eq!(row.2, "typescript");
        assert_eq!(row.3, "function");
        assert_eq!(row.4, 7);
        assert_eq!(row.5, 24);
        assert_eq!(row.6, unit.content_hash.as_str());
        assert!(!row.0.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!row.1.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!row.0.contains("UNIQUE_SOURCE_SENTINEL"));
    }

    #[test]
    fn code_unit_records_must_match_indexed_file_bounds_and_hash() {
        let workspace = TempWorkspace::new("sqlite-code-unit-rejects");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut file = file("src/a.ts");
        file.size_bytes = 10;
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");

        let missing = code_unit("src/missing.ts");
        let error = store
            .record_code_unit(&generation, &missing)
            .expect_err("missing indexed file");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut stale_hash = code_unit("src/a.ts");
        stale_hash.content_hash = ContentHash::new(
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .expect("valid hash");
        let error = store
            .record_code_unit(&generation, &stale_hash)
            .expect_err("content hash mismatch");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut too_large = code_unit("src/a.ts");
        too_large.end_byte = 11;
        let error = store
            .record_code_unit(&generation, &too_large)
            .expect_err("range exceeds file size");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut absolute = code_unit("/tmp/a.ts");
        absolute.end_byte = 1;
        let error = store
            .record_code_unit(&generation, &absolute)
            .expect_err("absolute path");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
    }

    #[test]
    fn ir_graph_records_round_trip_from_active_generation_without_leaks() {
        let workspace = TempWorkspace::new("sqlite-ir-graph");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut file = file("src/a.ts");
        file.size_bytes = 100;
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        let mut module = code_unit("src/a.ts");
        module.id = "unit:src/a.ts#module:0-100".to_string();
        module.end_byte = 100;
        let mut function = code_unit("src/a.ts");
        function.id = "unit:src/a.ts#function:10-40".to_string();
        function.kind = "function".to_string();
        function.start_byte = 10;
        function.end_byte = 40;
        store
            .record_code_unit(&generation, &function)
            .expect("record function");
        store
            .record_code_unit(&generation, &module)
            .expect("record module");
        let module_node = ir_node(&module);
        let function_node = ir_node(&function);
        store
            .record_ir_node(&generation, &function_node)
            .expect("record function node");
        store
            .record_ir_node(&generation, &module_node)
            .expect("record module node");
        store
            .record_ir_edge(&generation, &ir_edge(&module_node, &function_node))
            .expect("record contains edge");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let graph = store.list_active_ir_graph().expect("list IR graph");

        assert_eq!(graph.generation_id, "gen-000001");
        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [
                "ir:unit:src/a.ts#function:10-40",
                "ir:unit:src/a.ts#module:0-100"
            ]
        );
        assert_eq!(graph.edges, vec![ir_edge(&module_node, &function_node)]);
        let workspace_path = workspace.path().to_string_lossy();
        for node in graph.nodes {
            assert_eq!(node.payload_json, "{}");
            assert!(!node.id.contains(workspace_path.as_ref()));
            assert!(!node.code_unit_id.contains(workspace_path.as_ref()));
            assert!(!node.id.contains("UNIQUE_SOURCE_SENTINEL"));
        }
    }

    #[test]
    fn ir_graph_records_must_reference_same_generation_nodes_and_units() {
        let workspace = TempWorkspace::new("sqlite-ir-graph-rejects");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let mut file = file("src/a.ts");
        file.size_bytes = 100;
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        let mut module = code_unit("src/a.ts");
        module.id = "unit:src/a.ts#module:0-100".to_string();
        module.end_byte = 100;
        store
            .record_code_unit(&generation, &module)
            .expect("record module");
        let module_node = ir_node(&module);

        let mut missing_unit = module_node.clone();
        missing_unit.code_unit_id = "unit:src/a.ts#function:10-20".to_string();
        missing_unit.id = format!("ir:{}", missing_unit.code_unit_id);
        let error = store
            .record_ir_node(&generation, &missing_unit)
            .expect_err("missing code unit");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut mismatched_id = module_node.clone();
        mismatched_id.id = "ir:unit:src/a.ts#other:0-100".to_string();
        let error = store
            .record_ir_node(&generation, &mismatched_id)
            .expect_err("mismatched node id");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut non_empty_payload = module_node.clone();
        non_empty_payload.payload_json = r#"{"snippet":"const secret = true;"}"#.to_string();
        let error = store
            .record_ir_node(&generation, &non_empty_payload)
            .expect_err("non-empty payload");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        store
            .record_ir_node(&generation, &module_node)
            .expect("record module node");
        let missing_target = IndexedIrEdgeRecord {
            from_node_id: module_node.id.clone(),
            to_node_id: "ir:unit:src/a.ts#function:10-20".to_string(),
            label: "contains".to_string(),
        };
        let error = store
            .record_ir_edge(&generation, &missing_target)
            .expect_err("missing target node");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let self_edge = IndexedIrEdgeRecord {
            from_node_id: module_node.id.clone(),
            to_node_id: module_node.id,
            label: "contains".to_string(),
        };
        let error = store
            .record_ir_edge(&generation, &self_edge)
            .expect_err("self edge");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
    }

    #[test]
    fn generation_validation_rejects_malformed_ir_graph_rows() {
        let workspace = TempWorkspace::new("sqlite-ir-graph-validation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let file = file("src/a.ts");
        let unit = code_unit("src/a.ts");
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        store
            .record_code_unit(&generation, &unit)
            .expect("record code unit");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:src/a.ts#wrong', ?2, 'module', '{}')",
                params![generation.generation_id, unit.id],
            )
            .expect("insert malformed IR node");

        let error = store
            .validate_generation(&generation)
            .expect_err("malformed IR graph must block activation");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn list_active_ir_graph_rejects_tampered_payloads() {
        let workspace = TempWorkspace::new("sqlite-ir-graph-tamper");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        let unit = code_unit("src/a.ts");
        let node = ir_node(&unit);
        store
            .record_code_unit(&generation, &unit)
            .expect("record code unit");
        store
            .record_ir_node(&generation, &node)
            .expect("record IR node");
        store
            .activate_generation(&generation)
            .expect("activate generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        connection
            .execute(
                "UPDATE ir_nodes SET payload_json = ?1 WHERE node_id = ?2",
                params![r#"{"snippet":"const secret = true;"}"#, node.id],
            )
            .expect("tamper IR payload");

        let error = store
            .list_active_ir_graph()
            .expect_err("tampered IR payload must fail reads");
        let snapshot_error = store
            .load_active_claim_input_snapshot()
            .expect_err("tampered IR payload must fail snapshot reads");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(matches!(snapshot_error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn semantic_fact_records_are_persisted_with_evidence_without_source_text_or_absolute_paths() {
        let workspace = TempWorkspace::new("sqlite-semantic-facts");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let file = file("src/a.ts");
        let unit = code_unit("src/a.ts");
        let mut fact = semantic_fact("src/a.ts");
        fact.assumptions = vec!["path alias resolved from tsconfig".to_string()];
        store
            .record_indexed_file(&generation, &file)
            .expect("record file");
        store
            .record_code_unit(&generation, &unit)
            .expect("record code unit");

        store
            .record_semantic_fact(&generation, &fact)
            .expect("record semantic fact");

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let stored_fact = connection
            .query_row(
                "SELECT fact_id, kind, subject, target, certainty, origin_engine, \
                        origin_engine_version, origin_method, assumptions_json, evidence_id \
                 FROM semantic_facts",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .expect("query semantic fact");
        assert_eq!(stored_fact.0, fact.fact_id);
        assert_eq!(stored_fact.1, "RESOLVED_IMPORT");
        assert_eq!(stored_fact.2, fact.subject);
        assert_eq!(stored_fact.3, fact.target);
        assert_eq!(stored_fact.4, "SEMANTIC");
        assert_eq!(stored_fact.5, "typescript");
        assert_eq!(stored_fact.6, "6.0.0");
        assert_eq!(stored_fact.7, "compiler_api");
        assert_eq!(
            stored_fact.8,
            r#"["path alias resolved from tsconfig"]"#.to_string()
        );
        assert_eq!(stored_fact.9, fact.evidence_id);

        let evidence = connection
            .query_row(
                "SELECT evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note \
                 FROM evidence",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .expect("query evidence");
        assert_eq!(evidence.0, fact.evidence_id);
        assert_eq!(evidence.1, unit.id);
        assert_eq!(evidence.2, "src/a.ts");
        assert_eq!(evidence.3, file.content_hash.as_str());
        assert_eq!(evidence.4, 0);
        assert_eq!(evidence.5, 10);
        assert_eq!(evidence.6, fact.note);
        let workspace_path = workspace.path().to_string_lossy();
        for value in [
            stored_fact.0,
            stored_fact.2,
            stored_fact.8,
            stored_fact.9,
            evidence.0,
            evidence.1,
            evidence.2,
            evidence.6,
        ] {
            assert!(!value.contains(workspace_path.as_ref()));
            assert!(!value.contains("UNIQUE_SOURCE_SENTINEL"));
        }
    }

    #[test]
    fn semantic_fact_records_must_match_indexed_code_unit_evidence() {
        let workspace = TempWorkspace::new("sqlite-semantic-fact-rejects");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_indexed_file(&generation, &file("src/b.ts"))
            .expect("record second file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .record_code_unit(&generation, &code_unit("src/b.ts"))
            .expect("record second code unit");
        let fact = semantic_fact("src/a.ts");

        let mut missing_unit = fact.clone();
        missing_unit.code_unit_id = "unit:missing".to_string();
        let error = store
            .record_semantic_fact(&generation, &missing_unit)
            .expect_err("missing code unit");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut wrong_path = fact.clone();
        wrong_path.path = "src/b.ts".to_string();
        let error = store
            .record_semantic_fact(&generation, &wrong_path)
            .expect_err("path mismatch");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut wrong_hash = fact.clone();
        wrong_hash.content_hash = ContentHash::new(
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .expect("valid hash");
        let error = store
            .record_semantic_fact(&generation, &wrong_hash)
            .expect_err("hash mismatch");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut starts_before_unit = fact.clone();
        starts_before_unit.start_byte = 0;
        starts_before_unit.end_byte = 1;
        starts_before_unit.code_unit_id = {
            let mut offset_unit = code_unit("src/a.ts");
            offset_unit.id = "unit:src/a.ts#module:2-10".to_string();
            offset_unit.start_byte = 2;
            store
                .record_code_unit(&generation, &offset_unit)
                .expect("record offset code unit");
            offset_unit.id
        };
        let error = store
            .record_semantic_fact(&generation, &starts_before_unit)
            .expect_err("range starts before unit");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));

        let mut ends_after_unit = fact;
        ends_after_unit.end_byte = 11;
        let error = store
            .record_semantic_fact(&generation, &ends_after_unit)
            .expect_err("range ends after unit");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
    }

    #[test]
    fn semantic_fact_records_reject_stale_or_wrong_generation_code_units() {
        let workspace = TempWorkspace::new("sqlite-semantic-fact-stale");
        let store = store(&workspace);
        let first = store.prepare_next_generation().expect("prepare first");
        store
            .record_indexed_file(&first, &file("src/a.ts"))
            .expect("record first file");
        store
            .record_code_unit(&first, &code_unit("src/a.ts"))
            .expect("record first code unit");
        store.activate_generation(&first).expect("activate first");

        let second = store.prepare_next_generation().expect("prepare second");
        let mut changed_file = file("src/a.ts");
        changed_file.content_hash = ContentHash::new(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("valid hash");
        changed_file.size_bytes = 42;
        store
            .record_indexed_file(&second, &changed_file)
            .expect("record second file");
        let mut changed_unit = code_unit("src/a.ts");
        changed_unit.content_hash = changed_file.content_hash.clone();
        changed_unit.id = "unit:src/a.ts#module:0-5".to_string();
        changed_unit.end_byte = 5;
        store
            .record_code_unit(&second, &changed_unit)
            .expect("record second code unit");

        let stale_fact = semantic_fact("src/a.ts");
        let error = store
            .record_semantic_fact(&second, &stale_fact)
            .expect_err("stale code unit id must not cross generations");
        assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
        assert_eq!(
            store
                .inspect()
                .expect("inspect after stale fact")
                .active_generation,
            Some(first.generation_id)
        );
    }

    #[test]
    fn semantic_fact_records_require_building_generation() {
        let workspace = TempWorkspace::new("sqlite-semantic-fact-status");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        store
            .validate_generation(&generation)
            .expect("validate generation");

        let error = store
            .record_semantic_fact(&generation, &semantic_fact("src/a.ts"))
            .expect_err("validated generation must reject semantic fact writes");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn file_and_code_unit_records_require_building_generation() {
        fn assert_invalid_state(error: IndexStoreError) {
            assert!(matches!(error, IndexStoreError::InvalidState(_)));
        }

        let workspace = TempWorkspace::new("sqlite-file-unit-status");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file while building");
        store
            .validate_generation(&generation)
            .expect("validate generation");

        let error = store
            .record_indexed_file(&generation, &file("src/b.ts"))
            .expect_err("validated generation must reject indexed-file writes");
        assert_invalid_state(error);
        let error = store
            .remove_indexed_file(&generation, "src/a.ts")
            .expect_err("validated generation must reject indexed-file removal");
        assert_invalid_state(error);
        let error = store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect_err("validated generation must reject code-unit writes");
        assert_invalid_state(error);

        store
            .activate_generation(&generation)
            .expect("activate generation");
        let error = store
            .record_indexed_file(&generation, &file("src/c.ts"))
            .expect_err("active generation must reject indexed-file writes");
        assert_invalid_state(error);
        let error = store
            .remove_indexed_file(&generation, "src/a.ts")
            .expect_err("active generation must reject indexed-file removal");
        assert_invalid_state(error);
        let error = store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect_err("active generation must reject code-unit writes");
        assert_invalid_state(error);

        let failed = store.prepare_next_generation().expect("prepare failed");
        store
            .record_indexed_file(&failed, &file("src/a.ts"))
            .expect("record failed-generation file while building");
        let connection = store
            .open_existing_generation(&failed.generation_id)
            .expect("open failed generation");
        connection
            .execute(
                "UPDATE index_generations SET status = 'failed' WHERE generation_id = ?1",
                params![failed.generation_id.as_str()],
            )
            .expect("mark failed generation");

        let error = store
            .record_indexed_file(&failed, &file("src/b.ts"))
            .expect_err("failed generation must reject indexed-file writes");
        assert_invalid_state(error);
        let error = store
            .remove_indexed_file(&failed, "src/a.ts")
            .expect_err("failed generation must reject indexed-file removal");
        assert_invalid_state(error);
        let error = store
            .record_code_unit(&failed, &code_unit("src/a.ts"))
            .expect_err("failed generation must reject code-unit writes");
        assert_invalid_state(error);
    }

    #[test]
    fn generation_validation_rejects_malformed_semantic_evidence_rows() {
        let workspace = TempWorkspace::new("sqlite-semantic-fact-validation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_indexed_file(&generation, &file("src/b.ts"))
            .expect("record second file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:bad', ?2, 'src/b.ts', ?3, 0, 1, 'bad cross-file evidence')",
                params![
                    generation.generation_id,
                    code_unit("src/a.ts").id,
                    file("src/b.ts").content_hash.as_str(),
                ],
            )
            .expect("insert inconsistent evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'fact:bad', 'RESOLVED_IMPORT', 'src/a.ts#import', NULL, \
                         'SEMANTIC', 'typescript', '6.0.0', 'compiler_api', '[]', 'evidence:bad')",
                params![generation.generation_id],
            )
            .expect("insert semantic fact");

        let error = store
            .validate_generation(&generation)
            .expect_err("malformed semantic evidence must block activation");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn semantic_fact_storage_failure_is_atomic_without_partial_rows() {
        let workspace = TempWorkspace::new("sqlite-semantic-fact-atomic");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .record_indexed_file(&generation, &file("src/a.ts"))
            .expect("record file");
        store
            .record_code_unit(&generation, &code_unit("src/a.ts"))
            .expect("record code unit");
        let fact = semantic_fact("src/a.ts");
        store
            .record_semantic_fact(&generation, &fact)
            .expect("record first semantic fact");
        let duplicate = IndexedSemanticFactRecord {
            fact_id: "fact:src/a.ts#import:duplicate".to_string(),
            ..fact
        };

        let error = store
            .record_semantic_fact(&generation, &duplicate)
            .expect_err("duplicate evidence id must fail without partial rows");
        assert!(matches!(error, IndexStoreError::Unavailable(_)));

        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        let fact_count: u32 = connection
            .query_row("SELECT count(*) FROM semantic_facts", [], |row| row.get(0))
            .expect("semantic fact count");
        let evidence_count: u32 = connection
            .query_row("SELECT count(*) FROM evidence", [], |row| row.get(0))
            .expect("evidence count");
        assert_eq!(fact_count, 1);
        assert_eq!(evidence_count, 1);
    }

    #[test]
    fn failed_generation_validation_preserves_previous_active_generation() {
        let workspace = TempWorkspace::new("sqlite-rollback");
        let store = store(&workspace);
        let first = store.prepare_next_generation().expect("prepare first");
        store.activate_generation(&first).expect("activate first");

        let second = store.prepare_next_generation().expect("prepare second");
        store
            .record_indexed_file(&second, &file("src/a.ts"))
            .expect("record second file");
        store
            .record_code_unit(&second, &code_unit("src/a.ts"))
            .expect("record second code unit");
        let connection = store
            .open_existing_generation(&second.generation_id)
            .expect("open second generation");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:bad', ?2, 'src/a.ts', ?3, 0, 11, 'bad evidence')",
                params![
                    second.generation_id,
                    code_unit("src/a.ts").id,
                    file("src/a.ts").content_hash.as_str()
                ],
            )
            .expect("insert malformed evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'fact:bad', 'RESOLVED_IMPORT', 'src/a.ts#import', NULL, \
                         'SEMANTIC', 'typescript', '6.0.0', 'compiler_api', '[]', 'evidence:bad')",
                params![second.generation_id],
            )
            .expect("insert semantic fact for malformed evidence");

        assert!(store.validate_generation(&second).is_err());
        let inspection = store.inspect().expect("inspect after failed validation");

        assert_eq!(inspection.active_generation, Some(first.generation_id));
    }

    #[test]
    fn validation_rejects_active_generation_without_downgrading_status() {
        let workspace = TempWorkspace::new("sqlite-active-validation-freeze");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .activate_generation(&generation)
            .expect("activate generation");

        let validation_error = store
            .validate_generation(&generation)
            .expect_err("active generation must not be revalidated");
        let activation_error = store
            .activate_generation(&generation)
            .expect_err("active generation must not be reactivated");

        assert!(matches!(validation_error, IndexStoreError::InvalidState(_)));
        assert!(matches!(activation_error, IndexStoreError::InvalidState(_)));
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open active generation");
        let status: String = connection
            .query_row(
                "SELECT status FROM index_generations WHERE generation_id = ?1",
                params![generation.generation_id.as_str()],
                |row| row.get(0),
            )
            .expect("read generation status");
        assert_eq!(status, "active");
        assert_eq!(
            store
                .inspect()
                .expect("inspect after stale validation")
                .active_generation,
            Some(generation.generation_id.clone())
        );
    }

    #[test]
    fn validation_rejects_generation_without_generation_row() {
        let workspace = TempWorkspace::new("sqlite-missing-generation-row");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");

        connection
            .execute(
                "DELETE FROM index_generations WHERE generation_id = ?1",
                params![generation.generation_id],
            )
            .expect("delete generation row");

        let error = store
            .activate_generation(&generation)
            .expect_err("missing generation row must fail activation");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(!store.current_generation_path().exists());
    }

    #[test]
    fn inspect_reports_no_active_generation_when_mutable_database_has_no_active_generation() {
        let workspace = TempWorkspace::new("sqlite-no-active-generation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .validate_generation(&generation)
            .expect("validate generation");

        let inspection = store.inspect().expect("inspect storage");

        assert_eq!(inspection.active_generation, None);
        assert_eq!(inspection.schema_version, Some(STORAGE_SCHEMA_VERSION));
    }

    #[test]
    fn generation_validation_rejects_missing_required_tables() {
        let workspace = TempWorkspace::new("sqlite-missing-table");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let connection = store
            .open_generation(&generation.generation_id)
            .expect("open generation");

        connection
            .execute("DROP TABLE evidence", [])
            .expect("drop required table");

        let error = store
            .validate_generation(&generation)
            .expect_err("missing required table must fail validation");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn generation_validation_rejects_weakened_schema() {
        let workspace = TempWorkspace::new("sqlite-weak-schema");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        fs::remove_file(store.mutable_database_path()).expect("remove generated database");

        let connection =
            Connection::open(store.mutable_database_path()).expect("open weak database");
        connection
            .execute_batch(
                r#"
                CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT, applied_at TEXT);
                INSERT INTO schema_migrations (version, name, applied_at) VALUES (6, 'weak', 'now');
                CREATE TABLE index_generations (generation_id TEXT PRIMARY KEY, status TEXT, created_at TEXT, activated_at TEXT, repogrammar_version TEXT, repository_revision TEXT, worktree_hash TEXT);
                INSERT INTO index_generations (generation_id, status, created_at, repogrammar_version) VALUES ('gen-000001', 'building', 'now', '0.1.0');
                CREATE TABLE indexed_files (generation_id TEXT, path TEXT, content_hash TEXT, size_bytes INTEGER, language TEXT);
                CREATE TABLE code_units (generation_id TEXT, code_unit_id TEXT, path TEXT, language TEXT, kind TEXT, start_byte INTEGER, end_byte INTEGER, content_hash TEXT);
                CREATE TABLE ir_nodes (generation_id TEXT, node_id TEXT, code_unit_id TEXT, kind TEXT, payload_json TEXT);
                CREATE TABLE ir_edges (generation_id TEXT, from_node_id TEXT, to_node_id TEXT, label TEXT);
                CREATE TABLE semantic_facts (generation_id TEXT, fact_id TEXT, kind TEXT, subject TEXT, target TEXT, certainty TEXT);
                CREATE TABLE families (generation_id TEXT, family_id TEXT, classification TEXT);
                CREATE TABLE family_members (generation_id TEXT, family_id TEXT, code_unit_id TEXT, role TEXT);
                CREATE TABLE variation_slots (generation_id TEXT, family_id TEXT, slot_id TEXT, description TEXT);
                CREATE TABLE evidence (generation_id TEXT, evidence_id TEXT, code_unit_id TEXT, path TEXT, content_hash TEXT, start_byte INTEGER, end_byte INTEGER, note TEXT);
                "#,
            )
            .expect("create weak schema");

        let error = store
            .validate_generation(&generation)
            .expect_err("weakened schema must fail validation");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(format!("{error:?}").contains("storage schema version"));
    }

    #[test]
    fn broken_current_generation_pointer_is_reported() {
        let workspace = TempWorkspace::new("sqlite-broken-pointer");
        let store = store(&workspace);
        fs::write(store.current_generation_path(), "gen-999999\n").expect("write broken pointer");

        let error = store.inspect().expect_err("broken pointer must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn invalid_current_generation_id_is_reported() {
        let workspace = TempWorkspace::new("sqlite-invalid-pointer");
        let store = store(&workspace);
        fs::write(store.current_generation_path(), "gen-000000\n").expect("write invalid pointer");

        let error = store.inspect().expect_err("invalid pointer must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn indexed_file_paths_must_be_repo_relative() {
        let workspace = TempWorkspace::new("sqlite-relative-paths");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");

        for path in [
            "/abs.ts",
            "../escape.ts",
            "nested/../escape.ts",
            "C:/escape.ts",
            "C:\\escape.ts",
            "C:escape.ts",
            "",
        ] {
            let error = store
                .record_indexed_file(&generation, &file(path))
                .expect_err("invalid path must fail");
            assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
        }
    }

    #[test]
    fn repository_index_database_must_not_be_symlink() {
        let workspace = TempWorkspace::new("sqlite-db-symlink");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let outside = workspace.path().join("outside.sqlite");
        fs::write(&outside, "").expect("create outside database target");

        if !create_test_symlink_file(&outside, &state.join(DATABASE_FILE)) {
            return;
        }

        let store = SqliteIndexStore::new(state);
        let generation = GenerationHandle {
            generation_id: "gen-000001".to_string(),
        };
        let error = store
            .validate_generation(&generation)
            .expect_err("repository index database symlink must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn mutable_database_must_not_be_symlink_after_prepare() {
        let workspace = TempWorkspace::new("sqlite-prepared-db-symlink");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let outside = workspace.path().join("outside.sqlite");
        fs::write(&outside, "").expect("create outside database target");
        fs::remove_file(store.mutable_database_path()).expect("remove mutable database");

        if !create_test_symlink_file(&outside, &store.mutable_database_path()) {
            return;
        }

        let error = store
            .validate_generation(&generation)
            .expect_err("mutable database symlink must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn activation_ignores_legacy_current_generation_symlink() {
        let workspace = TempWorkspace::new("sqlite-pointer-symlink");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let outside = workspace.path().join("outside-pointer");

        if !create_test_symlink_file(&outside, &store.current_generation_path()) {
            return;
        }

        store
            .activate_generation(&generation)
            .expect("legacy pointer symlink is not used by mutable activation");
        assert_eq!(
            store
                .inspect()
                .expect("inspect active mutable generation")
                .active_generation,
            Some(generation.generation_id)
        );
    }

    #[test]
    fn inspect_rejects_broken_current_generation_symlink() {
        let workspace = TempWorkspace::new("sqlite-inspect-pointer-symlink");
        let store = store(&workspace);
        let outside = workspace.path().join("outside-pointer");

        if !create_test_symlink_file(&outside, &store.current_generation_path()) {
            return;
        }

        let error = store
            .inspect()
            .expect_err("broken symlink pointer must fail");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    // --- Generation write-session fault-injection tests --------------------
    //
    // These exercise the single-connection, bounded-batch write session and its
    // abandonment lifecycle. The invariant across every fault is: the active
    // generation stays readable and unchanged, an abandoned build that already
    // committed rows is stamped `failed`, and a build that persisted nothing
    // leaves a reusable `building` generation.

    fn generation_status_of(
        store: &SqliteIndexStore,
        generation: &GenerationHandle,
    ) -> Option<String> {
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        generation_status(&connection, &generation.generation_id).expect("read status")
    }

    fn generation_table_count(
        store: &SqliteIndexStore,
        generation: &GenerationHandle,
        table: &str,
    ) -> i64 {
        let connection = store
            .open_existing_generation(&generation.generation_id)
            .expect("open generation");
        connection
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE generation_id = ?1"),
                params![generation.generation_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count rows")
    }

    fn seed_active_generation(store: &SqliteIndexStore, path: &str) -> GenerationHandle {
        let generation = store.prepare_next_generation().expect("prepare active");
        {
            let mut session =
                SqliteGenerationWriteSession::open(store, &generation).expect("open session");
            session
                .record_indexed_file(&file(path))
                .expect("record file");
            session.finish().expect("finish session");
        }
        store
            .activate_generation(&generation)
            .expect("activate seed generation");
        generation
    }

    #[test]
    fn write_session_builds_generation_with_one_connection_and_bounded_transactions() {
        use std::sync::atomic::Ordering;
        let workspace = TempWorkspace::new("sqlite-write-session-happy");
        let store = store(&workspace);
        let instrumentation = store.write_instrumentation();
        let generation = store.prepare_next_generation().expect("prepare");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
        session
            .record_indexed_file(&file("src/a.ts"))
            .expect("record file");
        session
            .record_code_unit(&code_unit("src/a.ts"))
            .expect("record code unit");
        session
            .record_semantic_fact(&semantic_fact("src/a.ts"))
            .expect("record semantic fact");
        session.checkpoint().expect("checkpoint");
        session.finish().expect("finish");
        let stats = session.stats();
        assert!(stats.transactions >= 1);
        assert!(stats.transactions < stats.rows_written);
        assert!(stats.rows_written >= 3);
        assert_eq!(stats.checkpoints, 1);
        drop(session);

        // The whole build opened exactly one measured connection.
        assert_eq!(instrumentation.connection_opens.load(Ordering::Relaxed), 1);

        store.validate_generation(&generation).expect("validate");
        store.activate_generation(&generation).expect("activate");
        let files = store.list_active_indexed_files().expect("list files");
        assert_eq!(files.files.len(), 1);
    }

    #[test]
    fn write_session_checkpoint_commits_and_counts() {
        let workspace = TempWorkspace::new("sqlite-write-session-checkpoint");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
        session
            .record_indexed_file(&file("src/a.ts"))
            .expect("record file");
        // A phase checkpoint commits the open batch (so the row is durable and a
        // fresh reader sees it) and increments the checkpoint counter.
        assert_eq!(session.stats().transactions, 0);
        session.checkpoint().expect("checkpoint");
        assert_eq!(session.stats().transactions, 1);
        assert_eq!(session.stats().checkpoints, 1);
        assert!(!session.batch_is_open());
        session
            .checkpoint()
            .expect("second checkpoint is a no-op commit");
        assert_eq!(session.stats().checkpoints, 2);
        assert_eq!(
            generation_table_count(&store, &generation, "indexed_files"),
            1
        );
        session.finish().expect("finish");
    }

    #[test]
    fn write_session_finish_after_abandon_is_a_typed_error() {
        let workspace = TempWorkspace::new("sqlite-write-session-finish-after-abandon");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
        session
            .record_indexed_file(&file("src/a.ts"))
            .expect("record file");
        session.abandon().expect("abandon");
        // A build that gave up on one path must not silently finish on another.
        let error = session
            .finish()
            .expect_err("finish after abandon must error");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        // The rolled-back build persisted nothing and stays a reusable building
        // row (abandon committed no batch), so it can never activate an empty
        // index through the finished path.
        assert_eq!(
            generation_status_of(&store, &generation).as_deref(),
            Some("building")
        );
        assert_eq!(
            generation_table_count(&store, &generation, "indexed_files"),
            0
        );
    }

    #[test]
    fn write_session_double_finish_is_a_typed_error() {
        let workspace = TempWorkspace::new("sqlite-write-session-double-finish");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
        session
            .record_indexed_file(&file("src/a.ts"))
            .expect("record file");
        session.finish().expect("first finish");
        let error = session.finish().expect_err("second finish must error");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn write_session_record_validation_failure_leaves_generation_building() {
        let workspace = TempWorkspace::new("sqlite-write-session-validation");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
            // A code unit with no matching indexed file: a referential rejection
            // that persists nothing.
            let error = session
                .record_code_unit(&code_unit("src/missing.ts"))
                .expect_err("referential failure");
            assert!(matches!(error, IndexStoreError::InvalidRecord(_)));
        }
        assert_eq!(
            generation_status_of(&store, &generation).as_deref(),
            Some("building")
        );
    }

    #[test]
    fn write_session_abandon_after_commit_marks_failed_and_preserves_active() {
        let workspace = TempWorkspace::new("sqlite-write-session-abandon");
        let store = store(&workspace);
        let first = seed_active_generation(&store, "src/a.ts");
        let second = store.prepare_next_generation().expect("prepare second");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &second).expect("open session");
            session
                .record_indexed_file(&file("src/b.ts"))
                .expect("record file");
            session.checkpoint().expect("checkpoint commits a batch");
            session.abandon().expect("abandon");
        }
        assert_eq!(
            generation_status_of(&store, &second).as_deref(),
            Some("failed")
        );
        let files = store.list_active_indexed_files().expect("list active");
        assert_eq!(files.generation_id, first.generation_id);
    }

    #[test]
    fn write_session_drop_without_finish_after_commit_marks_failed() {
        let workspace = TempWorkspace::new("sqlite-write-session-drop");
        let store = store(&workspace);
        let first = seed_active_generation(&store, "src/a.ts");
        let second = store.prepare_next_generation().expect("prepare second");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &second).expect("open session");
            session
                .record_indexed_file(&file("src/b.ts"))
                .expect("record file");
            session.checkpoint().expect("checkpoint commits a batch");
            // Dropped without finish or abandon (the panic-unwind path).
        }
        assert_eq!(
            generation_status_of(&store, &second).as_deref(),
            Some("failed")
        );
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list active")
                .generation_id,
            first.generation_id
        );
    }

    #[test]
    fn write_session_fault_after_rows_aborts_and_reclaims() {
        let workspace = TempWorkspace::new("sqlite-write-session-after-rows");
        let store = store(&workspace);
        let first = seed_active_generation(&store, "src/a.ts");
        let second = store.prepare_next_generation().expect("prepare second");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &second).expect("open session");
            session
                .record_indexed_file(&file("src/b.ts"))
                .expect("record file");
            session.checkpoint().expect("checkpoint");
            session.inject_fault(InjectedWriteFault::AfterRows(1));
            let error = session
                .record_indexed_file(&file("src/c.ts"))
                .expect_err("mid-build fault");
            assert!(matches!(error, IndexStoreError::InvalidState(_)));
        }
        assert_eq!(
            generation_status_of(&store, &second).as_deref(),
            Some("failed")
        );
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list active")
                .generation_id,
            first.generation_id
        );
    }

    #[test]
    fn write_session_fault_after_evidence_before_fact_rolls_back() {
        let workspace = TempWorkspace::new("sqlite-write-session-mid-record");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
            session
                .record_indexed_file(&file("src/a.ts"))
                .expect("record file");
            session
                .record_code_unit(&code_unit("src/a.ts"))
                .expect("record code unit");
            session.checkpoint().expect("commit file and unit");
            session.inject_fault(InjectedWriteFault::AfterEvidenceBeforeFact);
            let error = session
                .record_semantic_fact(&semantic_fact("src/a.ts"))
                .expect_err("mid-record fault");
            assert!(matches!(error, IndexStoreError::InvalidState(_)));
            session.abandon().expect("abandon");
        }
        // File and unit survive (committed), the torn fact and its evidence are
        // rolled back, and the generation is unreachable.
        assert_eq!(
            generation_table_count(&store, &generation, "indexed_files"),
            1
        );
        assert_eq!(generation_table_count(&store, &generation, "code_units"), 1);
        assert_eq!(
            generation_table_count(&store, &generation, "semantic_facts"),
            0
        );
        assert_eq!(generation_table_count(&store, &generation, "evidence"), 0);
        assert_eq!(
            generation_status_of(&store, &generation).as_deref(),
            Some("failed")
        );
    }

    #[test]
    fn write_session_fault_before_commit_discards_batch() {
        let workspace = TempWorkspace::new("sqlite-write-session-before-commit");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        {
            let mut session =
                SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
            session
                .record_indexed_file(&file("src/a.ts"))
                .expect("record file");
            session.inject_fault(InjectedWriteFault::BeforeCommit);
            let error = session.checkpoint().expect_err("commit fault");
            assert!(matches!(error, IndexStoreError::InvalidState(_)));
            assert!(session.batch_is_open());
            session.abandon().expect("abandon");
        }
        // The sole batch never committed, so nothing persisted and the
        // generation stays a reusable `building` row.
        assert_eq!(
            generation_table_count(&store, &generation, "indexed_files"),
            0
        );
        assert_eq!(
            generation_status_of(&store, &generation).as_deref(),
            Some("building")
        );
    }

    #[test]
    fn write_session_reader_sees_old_active_until_activation() {
        let workspace = TempWorkspace::new("sqlite-write-session-reader");
        let store = store(&workspace);
        let first = seed_active_generation(&store, "src/a.ts");
        let second = store.prepare_next_generation().expect("prepare second");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &second).expect("open session");
        session
            .record_indexed_file(&file("src/b.ts"))
            .expect("record file");
        session.checkpoint().expect("checkpoint");
        // Mid-build, reads still resolve the previous active generation.
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list active")
                .generation_id,
            first.generation_id
        );
        session.finish().expect("finish");
        drop(session);
        store.validate_generation(&second).expect("validate");
        store.activate_generation(&second).expect("activate");
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list active after activation")
                .generation_id,
            second.generation_id
        );
    }

    #[test]
    fn write_session_status_flip_between_batches_is_rejected() {
        let workspace = TempWorkspace::new("sqlite-write-session-flip");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare");
        let mut session =
            SqliteGenerationWriteSession::open(&store, &generation).expect("open session");
        session
            .record_indexed_file(&file("src/a.ts"))
            .expect("record file");
        session.checkpoint().expect("checkpoint closes the batch");
        // An external writer flips the generation out of `building` between
        // batches.
        {
            let connection = store
                .open_existing_generation(&generation.generation_id)
                .expect("open generation");
            connection
                .execute(
                    "UPDATE index_generations SET status = 'failed' WHERE generation_id = ?1",
                    params![generation.generation_id],
                )
                .expect("flip status");
        }
        let error = session
            .record_code_unit(&code_unit("src/a.ts"))
            .expect_err("status flip must be caught at the next batch");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }
}
