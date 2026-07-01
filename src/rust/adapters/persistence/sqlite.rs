//! SQLite persistence adapter.
//!
//! SQL, migrations, PRAGMAs, and generation filesystem layout stay in this
//! adapter. Application code talks to it through storage ports.

use crate::core::model::{ContentHash, FactCertainty, IrEdgeLabel, IrNodeKind, SemanticFactKind};
use crate::core::policy::paths::{looks_like_windows_absolute_path, RepoRelativePathError};
use crate::ports::family_store::{
    family_evidence_covered_claim_is_supported, ActiveFamilies, ActiveFamily, FamilyStore,
    IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedFamilyRecord,
    IndexedVariationSlotRecord, StoreError,
};
use crate::ports::index_store::{
    ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
    ActiveSemanticFacts, GenerationHandle, GenerationPruneReport, GenerationPruneRequest,
    GenerationRetentionStore, IndexStore, IndexStoreError, IndexedCodeUnitRecord,
    IndexedFileRecord, IndexedIrEdgeRecord, IndexedIrNodeRecord, IndexedSemanticFactRecord,
    StorageInspection, STORAGE_SCHEMA_VERSION,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteIndexStore {
    state_dir: PathBuf,
}

impl SqliteIndexStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
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

    fn generation_dir(&self, generation_id: &str) -> PathBuf {
        self.generations_dir().join(generation_id)
    }

    #[cfg(test)]
    fn generation_database_path(&self, generation_id: &str) -> PathBuf {
        self.generation_dir(generation_id).join(DATABASE_FILE)
    }

    fn ensure_layout(&self) -> Result<(), IndexStoreError> {
        ensure_real_dir(&self.state_dir, "state directory")?;
        ensure_real_dir(&self.generations_dir(), "generations directory")?;
        ensure_real_dir(&self.tmp_dir(), "tmp directory")?;
        Ok(())
    }

    fn next_generation_id(&self) -> Result<String, IndexStoreError> {
        let mut max_seen = 0u32;
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
        if max_seen >= 999_999 {
            return Err(IndexStoreError::InvalidState(
                "generation id space is exhausted".to_string(),
            ));
        }
        Ok(format!("gen-{:06}", max_seen + 1))
    }

    fn open_generation(&self, generation_id: &str) -> Result<Connection, IndexStoreError> {
        let path = self.generation_database_path_for(generation_id, MissingDatabase::Allowed)?;
        let connection = open_connection(path, MissingDatabase::Allowed)?;
        apply_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn open_existing_generation(&self, generation_id: &str) -> Result<Connection, IndexStoreError> {
        let path = self.generation_database_path_for(generation_id, MissingDatabase::Rejected)?;
        let connection = open_connection(path, MissingDatabase::Rejected)?;
        apply_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn open_existing_generation_read_only(
        &self,
        generation_id: &str,
    ) -> Result<Connection, IndexStoreError> {
        let path = self.generation_database_path_for(generation_id, MissingDatabase::Rejected)?;
        let connection = open_read_only_connection(path)?;
        apply_read_pragmas(&connection).map_err(sql_unavailable)?;
        Ok(connection)
    }

    fn open_active_generation_read_only(&self) -> Result<(String, Connection), IndexStoreError> {
        self.require_existing_layout()?;
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
        let connection = self.open_existing_generation_read_only(&generation_id)?;
        if generation_status(&connection, &generation_id)?.as_deref() != Some("active") {
            return Err(IndexStoreError::InvalidState(
                "current-generation does not point at an active generation".to_string(),
            ));
        }
        validate_generation_for_read(&connection, &generation_id)?;
        Ok((generation_id, connection))
    }

    fn require_existing_layout(&self) -> Result<(), IndexStoreError> {
        ensure_existing_real_dir(&self.state_dir, "state directory")?;
        ensure_existing_real_dir(&self.generations_dir(), "generations directory")?;
        ensure_existing_real_dir(&self.tmp_dir(), "tmp directory")?;
        Ok(())
    }

    fn generation_database_path_for(
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

impl IndexStore for SqliteIndexStore {
    fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
        self.ensure_layout()?;
        let generation_id = self.next_generation_id()?;
        fs::create_dir(self.generation_dir(&generation_id))
            .map_err(|_| unavailable("failed to create generation directory"))?;
        let connection = self.open_generation(&generation_id)?;
        apply_migrations(&connection)?;
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
        validate_repo_relative_path(&file.path)?;
        let connection = self.open_existing_generation(&generation.generation_id)?;
        require_building_generation(&connection, &generation.generation_id, "indexed files")?;
        connection
            .execute(
                "INSERT INTO indexed_files \
                 (generation_id, path, content_hash, size_bytes, language) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    generation.generation_id,
                    file.path,
                    file.content_hash.as_str(),
                    i64::try_from(file.size_bytes)
                        .map_err(|_| invalid_record("file size exceeds SQLite integer range"))?,
                    file.language,
                ],
            )
            .map_err(sql_unavailable)?;
        Ok(())
    }

    fn record_code_unit(
        &self,
        generation: &GenerationHandle,
        unit: &IndexedCodeUnitRecord,
    ) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(&unit.path)?;
        let connection = self.open_existing_generation(&generation.generation_id)?;
        require_building_generation(&connection, &generation.generation_id, "code units")?;
        let Some((file_hash, file_size_bytes)) = connection
            .query_row(
                "SELECT content_hash, size_bytes FROM indexed_files \
                 WHERE generation_id = ?1 AND path = ?2",
                params![generation.generation_id, unit.path],
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
        let start_byte = i64::try_from(unit.start_byte)
            .map_err(|_| invalid_record("code unit range exceeds SQLite integer range"))?;
        let end_byte = i64::try_from(unit.end_byte)
            .map_err(|_| invalid_record("code unit range exceeds SQLite integer range"))?;
        if end_byte > file_size_bytes {
            return Err(invalid_record(
                "code unit range must not exceed indexed file size",
            ));
        }
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    generation.generation_id,
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
        Ok(())
    }

    fn record_ir_node(
        &self,
        generation: &GenerationHandle,
        node: &IndexedIrNodeRecord,
    ) -> Result<(), IndexStoreError> {
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
        let connection = self.open_existing_generation(&generation.generation_id)?;
        require_building_generation(&connection, &generation.generation_id, "IR nodes")?;
        let code_unit_exists = connection
            .query_row(
                "SELECT count(*) FROM code_units \
                 WHERE generation_id = ?1 AND code_unit_id = ?2",
                params![generation.generation_id, node.code_unit_id],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_unavailable)?;
        if code_unit_exists != 1 {
            return Err(invalid_record(
                "IR node must reference an indexed code unit in the same generation",
            ));
        }
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    generation.generation_id,
                    node.id,
                    node.code_unit_id,
                    node.kind,
                    node.payload_json,
                ],
            )
            .map_err(sql_unavailable)?;
        Ok(())
    }

    fn record_ir_edge(
        &self,
        generation: &GenerationHandle,
        edge: &IndexedIrEdgeRecord,
    ) -> Result<(), IndexStoreError> {
        validate_index_text_field(&edge.from_node_id, "IR edge from node id")?;
        validate_index_text_field(&edge.to_node_id, "IR edge to node id")?;
        IrEdgeLabel::parse_protocol_str(&edge.label).map_err(|_| {
            IndexStoreError::InvalidRecord("IR edge label is unsupported".to_string())
        })?;
        if edge.from_node_id == edge.to_node_id {
            return Err(invalid_record("IR edge must not point to itself"));
        }
        let connection = self.open_existing_generation(&generation.generation_id)?;
        require_building_generation(&connection, &generation.generation_id, "IR edges")?;
        for (node_id, label) in [
            (edge.from_node_id.as_str(), "from"),
            (edge.to_node_id.as_str(), "to"),
        ] {
            let node_exists = connection
                .query_row(
                    "SELECT count(*) FROM ir_nodes \
                     WHERE generation_id = ?1 AND node_id = ?2",
                    params![generation.generation_id, node_id],
                    |row| row.get::<_, u32>(0),
                )
                .map_err(sql_unavailable)?;
            if node_exists != 1 {
                return Err(IndexStoreError::InvalidRecord(format!(
                    "IR edge must reference an indexed {label} node in the same generation"
                )));
            }
        }
        connection
            .execute(
                "INSERT INTO ir_edges \
                 (generation_id, from_node_id, to_node_id, label) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    generation.generation_id,
                    edge.from_node_id,
                    edge.to_node_id,
                    edge.label,
                ],
            )
            .map_err(sql_unavailable)?;
        Ok(())
    }

    fn record_semantic_fact(
        &self,
        generation: &GenerationHandle,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError> {
        validate_repo_relative_path(&fact.path)?;
        let mut connection = self.open_existing_generation(&generation.generation_id)?;
        require_building_generation(&connection, &generation.generation_id, "semantic facts")?;
        let Some((unit_path, unit_hash, unit_start_byte, unit_end_byte, file_hash)) = connection
            .query_row(
                "SELECT code_units.path, code_units.content_hash, code_units.start_byte, \
                        code_units.end_byte, indexed_files.content_hash \
                 FROM code_units \
                 JOIN indexed_files \
                   ON indexed_files.generation_id = code_units.generation_id \
                  AND indexed_files.path = code_units.path \
                 WHERE code_units.generation_id = ?1 \
                   AND code_units.code_unit_id = ?2",
                params![generation.generation_id, fact.code_unit_id],
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
        let start_byte = i64::try_from(fact.start_byte)
            .map_err(|_| invalid_record("semantic fact range exceeds SQLite integer range"))?;
        let end_byte = i64::try_from(fact.end_byte)
            .map_err(|_| invalid_record("semantic fact range exceeds SQLite integer range"))?;
        if start_byte < unit_start_byte || end_byte > unit_end_byte {
            return Err(invalid_record(
                "semantic fact range must stay within code unit range",
            ));
        }
        let assumptions_json = serde_json::to_string(&fact.assumptions).map_err(|_| {
            IndexStoreError::InvalidRecord("semantic fact assumptions are invalid".to_string())
        })?;

        let transaction = connection.transaction().map_err(sql_unavailable)?;
        transaction
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    generation.generation_id,
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
        transaction
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, \
                  origin_engine, origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    generation.generation_id,
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
        transaction.commit().map_err(sql_unavailable)?;
        Ok(())
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
        if ir_graph_violation_count(&connection, &generation.generation_id)? != 0 {
            return Err(IndexStoreError::InvalidState(
                "IR graph is inconsistent with indexed code units".to_string(),
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
        self.validate_generation(generation)?;
        let connection = self.open_existing_generation(&generation.generation_id)?;
        let updated = connection
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
        atomic_write_current_generation(
            &self.current_generation_path(),
            &self.tmp_dir(),
            &generation.generation_id,
        )?;
        Ok(())
    }

    fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
        self.ensure_layout()?;
        let current = self.current_generation_path();
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StorageInspection {
                    active_generation: None,
                    schema_version: None,
                    code_unit_count: None,
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
        let connection = self.open_existing_generation(&generation_id)?;
        if generation_status(&connection, &generation_id)?.as_deref() != Some("active") {
            return Err(IndexStoreError::InvalidState(
                "current-generation does not point at an active generation".to_string(),
            ));
        }
        inspect_connection(&connection, Some(&generation_id))
    }
}

impl GenerationRetentionStore for SqliteIndexStore {
    fn prune_generations(
        &self,
        request: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, IndexStoreError> {
        let (active_generation, active_connection) = self.open_active_generation_read_only()?;
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

impl FamilyStore for SqliteIndexStore {
    fn record_family(
        &self,
        generation: &GenerationHandle,
        family: &IndexedFamilyRecord,
    ) -> Result<(), StoreError> {
        record_family_sqlite(self, generation, family).map_err(family_store_error)
    }

    fn record_family_member(
        &self,
        generation: &GenerationHandle,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError> {
        record_family_member_sqlite(self, generation, member).map_err(family_store_error)
    }

    fn record_variation_slot(
        &self,
        generation: &GenerationHandle,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError> {
        record_variation_slot_sqlite(self, generation, slot).map_err(family_store_error)
    }

    fn record_family_evidence(
        &self,
        generation: &GenerationHandle,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError> {
        record_family_evidence_sqlite(self, generation, evidence).map_err(family_store_error)
    }

    fn list_active_families(&self) -> Result<ActiveFamilies, StoreError> {
        let (generation_id, connection) = self
            .open_active_generation_read_only()
            .map_err(family_store_error)?;
        let families = query_families(&connection, &generation_id).map_err(family_store_error)?;
        for family in &families {
            query_family_evidence(&connection, &generation_id, &family.family_id)
                .map_err(family_store_error)?;
        }
        Ok(ActiveFamilies {
            generation_id,
            families,
        })
    }

    fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError> {
        validate_index_text_field(family_id, "family id").map_err(family_store_error)?;
        let (generation_id, connection) = self
            .open_active_generation_read_only()
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

fn record_family_sqlite(
    store: &SqliteIndexStore,
    generation: &GenerationHandle,
    family: &IndexedFamilyRecord,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(&family.family_id, "family id")?;
    validate_family_classification(&family.classification)?;
    let connection = store.open_existing_generation(&generation.generation_id)?;
    require_building_generation(&connection, &generation.generation_id, "families")?;
    connection
        .execute(
            "INSERT INTO families (generation_id, family_id, classification) VALUES (?1, ?2, ?3)",
            params![
                generation.generation_id,
                family.family_id,
                family.classification,
            ],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn record_family_member_sqlite(
    store: &SqliteIndexStore,
    generation: &GenerationHandle,
    member: &IndexedFamilyMemberRecord,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(&member.family_id, "family member family id")?;
    validate_index_text_field(&member.code_unit_id, "family member code unit id")?;
    validate_index_text_field(&member.role, "family member role")?;
    let connection = store.open_existing_generation(&generation.generation_id)?;
    require_building_generation(&connection, &generation.generation_id, "family members")?;
    require_family_row(&connection, &generation.generation_id, &member.family_id)?;
    require_code_unit_row(&connection, &generation.generation_id, &member.code_unit_id)?;
    connection
        .execute(
            "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                generation.generation_id,
                member.family_id,
                member.code_unit_id,
                member.role,
            ],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn record_variation_slot_sqlite(
    store: &SqliteIndexStore,
    generation: &GenerationHandle,
    slot: &IndexedVariationSlotRecord,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(&slot.family_id, "variation slot family id")?;
    validate_index_text_field(&slot.slot_id, "variation slot id")?;
    validate_index_text_field(&slot.description, "variation slot description")?;
    let connection = store.open_existing_generation(&generation.generation_id)?;
    require_building_generation(&connection, &generation.generation_id, "variation slots")?;
    require_family_row(&connection, &generation.generation_id, &slot.family_id)?;
    connection
        .execute(
            "INSERT INTO variation_slots (generation_id, family_id, slot_id, description) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                generation.generation_id,
                slot.family_id,
                slot.slot_id,
                slot.description,
            ],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn record_family_evidence_sqlite(
    store: &SqliteIndexStore,
    generation: &GenerationHandle,
    evidence: &IndexedFamilyEvidenceRecord,
) -> Result<(), IndexStoreError> {
    validate_index_text_field(&evidence.evidence_id, "family evidence id")?;
    validate_index_text_field(&evidence.family_id, "family evidence family id")?;
    validate_index_text_field(&evidence.code_unit_id, "family evidence code unit id")?;
    validate_family_evidence_covered_claims(&evidence.covered_claims)?;
    validate_repo_relative_path(&evidence.path)?;
    validate_index_text_field(&evidence.note, "family evidence note")?;
    let connection = store.open_existing_generation(&generation.generation_id)?;
    require_building_generation(&connection, &generation.generation_id, "family evidence")?;
    require_family_row(&connection, &generation.generation_id, &evidence.family_id)?;
    let (unit_path, unit_hash, unit_start_byte, unit_end_byte, file_hash, file_size) =
        code_unit_evidence_bounds(
            &connection,
            &generation.generation_id,
            &evidence.code_unit_id,
        )?;
    if unit_path != evidence.path {
        return Err(invalid_record(
            "family evidence path must match code unit path",
        ));
    }
    if unit_hash != evidence.content_hash.as_str() || file_hash != evidence.content_hash.as_str() {
        return Err(invalid_record(
            "family evidence content hash must match indexed file and code unit",
        ));
    }
    let start_byte = i64::try_from(evidence.start_byte)
        .map_err(|_| invalid_record("family evidence range exceeds SQLite integer range"))?;
    let end_byte = i64::try_from(evidence.end_byte)
        .map_err(|_| invalid_record("family evidence range exceeds SQLite integer range"))?;
    if start_byte > end_byte {
        return Err(invalid_record(
            "family evidence range start must not exceed end",
        ));
    }
    if start_byte < unit_start_byte || end_byte > unit_end_byte || end_byte > file_size {
        return Err(invalid_record(
            "family evidence range must stay within code unit range",
        ));
    }
    let covered_claims_json = family_evidence_covered_claims_json(&evidence.covered_claims)?;
    connection
        .execute(
            "INSERT INTO evidence \
             (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, path, content_hash, start_byte, end_byte, note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                generation.generation_id,
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
    Ok(())
}

fn validate_family_classification(classification: &str) -> Result<(), IndexStoreError> {
    match classification {
        "DOMINANT_PATTERN" | "VARIATION" | "EXCEPTION" | "UNKNOWN" => Ok(()),
        _ => Err(invalid_record("family classification is unsupported")),
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

fn query_families(
    connection: &Connection,
    generation_id: &str,
) -> Result<Vec<IndexedFamilyRecord>, IndexStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT family_id, classification \
             FROM families \
             WHERE generation_id = ?1 \
             ORDER BY family_id COLLATE BINARY",
        )
        .map_err(sql_unavailable)?;
    let rows = statement
        .query_map(params![generation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_unavailable)?;
    let mut families = Vec::new();
    for row in rows {
        let (family_id, classification) = row.map_err(sql_unavailable)?;
        validate_stored_semantic_text_field("stored family id", &family_id)?;
        validate_stored_family_classification(&classification)?;
        families.push(IndexedFamilyRecord {
            family_id,
            classification,
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
            "SELECT family_id, classification \
             FROM families \
             WHERE generation_id = ?1 AND family_id = ?2",
            params![generation_id, family_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(sql_unavailable)?;
    let Some((family_id, classification)) = record else {
        return Ok(None);
    };
    validate_stored_semantic_text_field("stored family id", &family_id)?;
    validate_stored_family_classification(&classification)?;
    Ok(Some(IndexedFamilyRecord {
        family_id,
        classification,
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
    connection
        .execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) \
             VALUES (?1, 'initial_schema', datetime('now'))",
            params![STORAGE_SCHEMA_VERSION],
        )
        .map_err(sql_unavailable)?;
    Ok(())
}

fn validate_generation_for_read(
    connection: &Connection,
    generation_id: &str,
) -> Result<(), IndexStoreError> {
    let inspection = inspect_connection(connection, Some(generation_id))?;
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
    if ir_graph_violation_count(connection, generation_id)? != 0 {
        return Err(IndexStoreError::InvalidState(
            "IR graph is inconsistent with indexed code units".to_string(),
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
    let code_unit_count = if active_generation.is_some() {
        let count = connection
            .query_row("SELECT count(*) FROM code_units", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(sql_unavailable)?;
        Some(u64::try_from(count).map_err(|_| {
            IndexStoreError::InvalidState("code unit count is outside valid range".to_string())
        })?)
    } else {
        None
    };

    Ok(StorageInspection {
        active_generation: active_generation.map(str::to_string),
        schema_version,
        code_unit_count,
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
    let missing_evidence_count = connection
        .query_row(
            "SELECT count(*) \
             FROM families \
             WHERE families.generation_id = ?1 \
               AND families.classification <> 'UNKNOWN' \
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
               AND classification NOT IN ('DOMINANT_PATTERN', 'VARIATION', 'EXCEPTION', 'UNKNOWN')",
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
                        'prisma_query', 'prisma_transaction', \
                        'drizzle_schema_table', 'drizzle_query', 'drizzle_transaction', \
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

fn require_building_generation(
    connection: &Connection,
    generation_id: &str,
    record_type: &str,
) -> Result<(), IndexStoreError> {
    match generation_status(connection, generation_id)?.as_deref() {
        Some("building") => Ok(()),
        Some(_) => Err(IndexStoreError::InvalidState(format!(
            "{record_type} may only be recorded for building generations"
        ))),
        None => Err(IndexStoreError::InvalidState(
            "generation row is missing".to_string(),
        )),
    }
}

fn atomic_write_current_generation(
    current_path: &Path,
    tmp_dir: &Path,
    generation_id: &str,
) -> Result<(), IndexStoreError> {
    validate_generation_id(generation_id)?;
    ensure_real_dir(tmp_dir, "tmp directory")?;
    if let Ok(metadata) = fs::symlink_metadata(current_path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(IndexStoreError::InvalidState(
                "current-generation must be a regular file".to_string(),
            ));
        }
    }

    let tmp_path = tmp_dir.join(format!("{CURRENT_GENERATION_FILE}.tmp"));
    fs::write(&tmp_path, format!("{generation_id}\n"))
        .map_err(|_| unavailable("failed to write current-generation pointer"))?;
    fs::rename(&tmp_path, current_path).map_err(|_| {
        let _ = fs::remove_file(&tmp_path);
        unavailable("failed to atomically activate current generation")
    })?;
    Ok(())
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
    match classification {
        "DOMINANT_PATTERN" | "VARIATION" | "EXCEPTION" | "UNKNOWN" => Ok(()),
        _ => Err(IndexStoreError::InvalidState(
            "stored family classification is invalid".to_string(),
        )),
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
        columns: &["generation_id", "family_id", "classification"],
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
    classification TEXT NOT NULL CHECK (classification IN ('DOMINANT_PATTERN', 'VARIATION', 'EXCEPTION', 'UNKNOWN')),
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
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::test_support::{create_test_symlink_dir, create_test_symlink_file, TempWorkspace};

    fn store(workspace: &TempWorkspace) -> SqliteIndexStore {
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
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
    fn prune_generations_preserves_active_and_removes_old_inactive() {
        let workspace = TempWorkspace::new("sqlite-prune-generations");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        let third = activate_empty_generation(&store);
        let fourth = activate_empty_generation(&store);
        fs::write(
            store
                .generation_dir(&first.generation_id)
                .join("repogrammar.sqlite-wal"),
            "wal",
        )
        .expect("write wal sidecar");
        fs::write(
            store
                .generation_dir(&first.generation_id)
                .join("repogrammar.sqlite-shm"),
            "shm",
        )
        .expect("write shm sidecar");

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
        assert!(!store.generation_dir(&first.generation_id).exists());
        assert!(!store.generation_dir(&second.generation_id).exists());
        assert!(store.generation_dir(&third.generation_id).is_dir());
        assert!(store.generation_dir(&fourth.generation_id).is_dir());

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
        assert!(store.generation_dir(&first.generation_id).is_dir());
        assert!(store.generation_dir(&second.generation_id).is_dir());
        assert!(store.generation_dir(&third.generation_id).is_dir());
    }

    #[test]
    fn prune_generations_refuses_generation_directory_symlink() {
        let workspace = TempWorkspace::new("sqlite-prune-generation-symlink");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        fs::remove_dir_all(store.generation_dir(&first.generation_id))
            .expect("remove first generation");
        let outside = workspace.path().join("outside-generation");
        fs::create_dir_all(&outside).expect("create outside generation");
        if !create_test_symlink_dir(&outside, &store.generation_dir(&first.generation_id)) {
            return;
        }

        let error = store
            .prune_generations(GenerationPruneRequest {
                keep_inactive: 0,
                dry_run: false,
            })
            .expect_err("symlink generation directory must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(store.generation_dir(&second.generation_id).is_dir());
    }

    #[test]
    fn prune_generations_refuses_generation_file() {
        let workspace = TempWorkspace::new("sqlite-prune-generation-file");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        fs::remove_dir_all(store.generation_dir(&first.generation_id))
            .expect("remove first generation");
        fs::write(
            store.generation_dir(&first.generation_id),
            "not a directory",
        )
        .expect("write generation file");

        let error = store
            .prune_generations(GenerationPruneRequest {
                keep_inactive: 0,
                dry_run: false,
            })
            .expect_err("generation file must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(store.generation_dir(&second.generation_id).is_dir());
    }

    #[test]
    fn prune_generations_refuses_corrupt_current_generation_pointer() {
        let workspace = TempWorkspace::new("sqlite-prune-corrupt-pointer");
        let store = store(&workspace);
        let first = activate_empty_generation(&store);
        let second = activate_empty_generation(&store);
        fs::write(store.current_generation_path(), "not-a-generation\n")
            .expect("corrupt current generation pointer");

        let error = store
            .prune_generations(GenerationPruneRequest {
                keep_inactive: 0,
                dry_run: false,
            })
            .expect_err("corrupt pointer must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(store.generation_dir(&first.generation_id).is_dir());
        assert!(store.generation_dir(&second.generation_id).is_dir());
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
        let detail = store
            .show_family(&family().family_id)
            .expect("show family")
            .expect("family exists");
        let missing = store.show_family("family:missing").expect("show missing");

        assert_eq!(families.generation_id, generation.generation_id);
        assert_eq!(families.families, vec![family()]);
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
    fn list_active_families_validates_family_evidence_payloads() {
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
            .list_active_families()
            .expect_err("tampered family evidence payload is rejected by list");

        assert!(format!("{error:?}").contains("family evidence"));
    }

    #[test]
    fn list_active_families_validates_family_evidence_covered_claims() {
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
            .list_active_families()
            .expect_err("tampered family evidence coverage is rejected by list");

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
                "INSERT INTO families (generation_id, family_id, classification) \
                 VALUES (?1, ?2, 'UNKNOWN')",
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
        fs::create_dir(store.generations_dir().join("gen-999999")).expect("create max generation");

        let error = store
            .prepare_next_generation()
            .expect_err("exhausted generation ids must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
        assert!(!store.generations_dir().join("gen-1000000").exists());
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
        assert_eq!(inspection.journal_mode.as_deref(), Some("wal"));
        assert_eq!(inspection.foreign_keys_enabled, Some(true));
        assert_eq!(inspection.busy_timeout_ms, Some(5_000));
        assert_eq!(inspection.temp_store.as_deref(), Some("memory"));
        assert_eq!(inspection.integrity_check.as_deref(), Some("ok"));
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
        fs::write(
            store.generation_database_path(&second.generation_id),
            "not a sqlite database",
        )
        .expect("corrupt second db");

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
    fn inspect_rejects_pointer_to_non_active_generation() {
        let workspace = TempWorkspace::new("sqlite-pointer-not-active");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        store
            .validate_generation(&generation)
            .expect("validate generation");
        fs::write(store.current_generation_path(), "gen-000001\n").expect("write pointer");

        let error = store
            .inspect()
            .expect_err("validated but inactive pointer must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
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
        fs::remove_file(store.generation_database_path(&generation.generation_id))
            .expect("remove generated database");

        let connection =
            Connection::open(store.generation_database_path(&generation.generation_id))
                .expect("open weak database");
        connection
            .execute_batch(
                r#"
                CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT, applied_at TEXT);
                INSERT INTO schema_migrations (version, name, applied_at) VALUES (5, 'weak', 'now');
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
        assert!(format!("{error:?}").contains("required storage schema"));
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
    fn generation_directory_must_not_be_symlink() {
        let workspace = TempWorkspace::new("sqlite-generation-dir-symlink");
        let state = workspace.path().join(".repogrammar");
        let generations = state.join("generations");
        fs::create_dir_all(&generations).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let outside = workspace.path().join("outside-generation");
        fs::create_dir_all(&outside).expect("create outside generation");

        if !create_test_symlink_dir(&outside, &generations.join("gen-000001")) {
            return;
        }

        let store = SqliteIndexStore::new(state);
        let generation = GenerationHandle {
            generation_id: "gen-000001".to_string(),
        };
        let error = store
            .validate_generation(&generation)
            .expect_err("generation directory symlink must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn generation_database_must_not_be_symlink() {
        let workspace = TempWorkspace::new("sqlite-generation-db-symlink");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let outside = workspace.path().join("outside.sqlite");
        fs::write(&outside, "").expect("create outside database target");
        fs::remove_file(store.generation_database_path(&generation.generation_id))
            .expect("remove generated database");

        if !create_test_symlink_file(
            &outside,
            &store.generation_database_path(&generation.generation_id),
        ) {
            return;
        }

        let error = store
            .validate_generation(&generation)
            .expect_err("generation database symlink must fail");

        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }

    #[test]
    fn activation_rejects_current_generation_symlink() {
        let workspace = TempWorkspace::new("sqlite-pointer-symlink");
        let store = store(&workspace);
        let generation = store.prepare_next_generation().expect("prepare generation");
        let outside = workspace.path().join("outside-pointer");

        if !create_test_symlink_file(&outside, &store.current_generation_path()) {
            return;
        }

        let error = store
            .activate_generation(&generation)
            .expect_err("symlink pointer must fail");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
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
}
