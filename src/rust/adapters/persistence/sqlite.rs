//! SQLite persistence adapter.
//!
//! SQL, migrations, PRAGMAs, and generation filesystem layout stay in this
//! adapter. Application code talks to it through `ports::index_store`.

use crate::ports::index_store::{
    GenerationHandle, IndexStore, IndexStoreError, IndexedFileRecord, StorageInspection,
    STORAGE_SCHEMA_VERSION,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const DATABASE_FILE: &str = "repogrammar.sqlite";
const CURRENT_GENERATION_FILE: &str = "current-generation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingDatabase {
    Allowed,
    Rejected,
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
        if generation_status(&connection, &generation.generation_id)?.as_deref() == Some("failed") {
            return Err(IndexStoreError::InvalidState(
                "failed generation cannot be activated".to_string(),
            ));
        }
        let updated = connection
            .execute(
                "UPDATE index_generations SET status = 'validated' WHERE generation_id = ?1",
                params![generation.generation_id],
            )
            .map_err(sql_unavailable)?;
        if updated != 1 {
            return Err(IndexStoreError::InvalidState(
                "generation row is missing".to_string(),
            ));
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
                 WHERE generation_id = ?1",
                params![generation.generation_id],
            )
            .map_err(sql_unavailable)?;
        if updated != 1 {
            return Err(IndexStoreError::InvalidState(
                "generation row is missing".to_string(),
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

    Ok(StorageInspection {
        active_generation: active_generation.map(str::to_string),
        schema_version,
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
    if path.trim().is_empty() || path.chars().any(char::is_control) {
        return Err(invalid_record("indexed file path must be non-empty"));
    }
    if path.contains('\\') || looks_like_windows_absolute_path(path) {
        return Err(invalid_record("indexed file path must be repo-relative"));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(invalid_record("indexed file path must be repo-relative"));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(invalid_record(
                    "indexed file path must not contain traversal or prefixes",
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

fn sql_unavailable(error: rusqlite::Error) -> IndexStoreError {
    IndexStoreError::Unavailable(error.to_string())
}

fn unavailable(message: &'static str) -> IndexStoreError {
    IndexStoreError::Unavailable(message.to_string())
}

fn invalid_record(message: &'static str) -> IndexStoreError {
    IndexStoreError::InvalidRecord(message.to_string())
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
        columns: &["generation_id", "node_id", "kind", "payload_json"],
        primary_key_columns: &["generation_id", "node_id"],
        minimum_foreign_key_rows: 1,
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
        ],
        primary_key_columns: &["generation_id", "fact_id"],
        minimum_foreign_key_rows: 1,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
    },
    RequiredTableSchema {
        name: "families",
        columns: &["generation_id", "family_id", "classification"],
        primary_key_columns: &["generation_id", "family_id"],
        minimum_foreign_key_rows: 1,
        required_sql_fragments: &["PRIMARY KEY", "FOREIGN KEY"],
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
            "code_unit_id",
            "path",
            "content_hash",
            "start_byte",
            "end_byte",
            "note",
        ],
        primary_key_columns: &["generation_id", "evidence_id"],
        minimum_foreign_key_rows: 3,
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
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (generation_id, node_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE
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
    PRIMARY KEY (generation_id, fact_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS families (
    generation_id TEXT NOT NULL,
    family_id TEXT NOT NULL,
    classification TEXT NOT NULL,
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
    code_unit_id TEXT,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
    note TEXT NOT NULL,
    PRIMARY KEY (generation_id, evidence_id),
    FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id) ON DELETE CASCADE,
    FOREIGN KEY (generation_id, path) REFERENCES indexed_files(generation_id, path) ON DELETE CASCADE
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::test_support::TempWorkspace;

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
                INSERT INTO schema_migrations (version, name, applied_at) VALUES (1, 'weak', 'now');
                CREATE TABLE index_generations (generation_id TEXT PRIMARY KEY, status TEXT, created_at TEXT, activated_at TEXT, repogrammar_version TEXT, repository_revision TEXT, worktree_hash TEXT);
                INSERT INTO index_generations (generation_id, status, created_at, repogrammar_version) VALUES ('gen-000001', 'building', 'now', '0.1.0');
                CREATE TABLE indexed_files (generation_id TEXT, path TEXT, content_hash TEXT, size_bytes INTEGER, language TEXT);
                CREATE TABLE code_units (generation_id TEXT, code_unit_id TEXT, path TEXT, language TEXT, kind TEXT, start_byte INTEGER, end_byte INTEGER, content_hash TEXT);
                CREATE TABLE ir_nodes (generation_id TEXT, node_id TEXT, kind TEXT, payload_json TEXT);
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

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, generations.join("gen-000001"))
            .expect("create generation symlink");

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, generations.join("gen-000001"))
            .expect("create generation symlink");

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

        #[cfg(unix)]
        std::os::unix::fs::symlink(
            &outside,
            store.generation_database_path(&generation.generation_id),
        )
        .expect("create database symlink");

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            &outside,
            store.generation_database_path(&generation.generation_id),
        )
        .expect("create database symlink");

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

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, store.current_generation_path())
            .expect("create pointer symlink");

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside, store.current_generation_path())
            .expect("create pointer symlink");

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

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, store.current_generation_path())
            .expect("create pointer symlink");

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside, store.current_generation_path())
            .expect("create pointer symlink");

        let error = store
            .inspect()
            .expect_err("broken symlink pointer must fail");
        assert!(matches!(error, IndexStoreError::InvalidState(_)));
    }
}
