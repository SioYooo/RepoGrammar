//! Indexing use-case boundary.

use crate::core::model::{CodeUnit, Language, RepositoryRevision};
use crate::error::RepoGrammarError;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::index_store::{IndexStore, IndexedCodeUnitRecord, IndexedFileRecord};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument, SourceParser};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError};

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

pub fn index_repository_with_discovery_parser_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let report = discover_repository_files(request.clone(), discovery)?;
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

    let mut indexed_units = 0usize;
    let mut warnings = report.warnings;
    for file in &report.files {
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        let parse_report = match parser.parse(SourceDocument {
            path: &source.path,
            language: language_from_discovered(file.language),
            content_hash: source.content_hash.clone(),
            repository_revision: RepositoryRevision::new("UNKNOWN")
                .expect("UNKNOWN is a non-empty repository revision marker"),
            text: &source.text,
        }) {
            Ok(report) => report,
            Err(ParseError::UnsupportedLanguage) => {
                warnings.push(format!(
                    "parser skipped unsupported language: {}",
                    file.path
                ));
                continue;
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {}: internal parser error",
                    file.path
                )));
            }
        };
        indexed_units += record_parse_report(
            store,
            &generation,
            file,
            &source.text,
            parse_report,
            &mut warnings,
        )?;
    }

    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexed_units,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        warnings,
    })
}

fn record_parse_report(
    store: &impl IndexStore,
    generation: &crate::ports::index_store::GenerationHandle,
    file: &DiscoveredFile,
    text: &str,
    mut parse_report: ParseReport,
    warnings: &mut Vec<String>,
) -> Result<usize, RepoGrammarError> {
    for _diagnostic in parse_report.diagnostics {
        warnings.push(format!(
            "parse diagnostic for {}: syntax-only parser reported a diagnostic",
            file.path
        ));
    }
    parse_report.units.sort_by(|left, right| {
        (
            left.provenance.path.as_str(),
            left.range.start_byte,
            left.range.end_byte,
            left.kind.as_str(),
            left.id.as_str(),
        )
            .cmp(&(
                right.provenance.path.as_str(),
                right.range.start_byte,
                right.range.end_byte,
                right.kind.as_str(),
                right.id.as_str(),
            ))
    });
    let mut count = 0usize;
    for unit in &parse_report.units {
        validate_parser_unit(file, text, unit)?;
        crate::application::storage::record_code_unit(
            store,
            generation,
            &IndexedCodeUnitRecord {
                id: unit.id.as_str().to_string(),
                path: unit.provenance.path.clone(),
                language: unit.language.as_str().to_string(),
                kind: unit.kind.as_str().to_string(),
                start_byte: unit.range.start_byte,
                end_byte: unit.range.end_byte,
                content_hash: unit.provenance.content_hash.clone(),
            },
        )?;
        count += 1;
    }
    Ok(count)
}

fn validate_parser_unit(
    file: &DiscoveredFile,
    text: &str,
    unit: &CodeUnit,
) -> Result<(), RepoGrammarError> {
    if unit.provenance.path != file.path {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a code unit for a different path".to_string(),
        ));
    }
    if unit.provenance.content_hash != file.content_hash {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a code unit with mismatched content hash".to_string(),
        ));
    }
    if unit.range.end_byte > text.len() {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a code unit range outside source bounds".to_string(),
        ));
    }
    Ok(())
}

fn language_from_discovered(language: DiscoveredLanguage) -> Language {
    match language {
        DiscoveredLanguage::TypeScript | DiscoveredLanguage::TypeScriptReact => {
            Language::TypeScript
        }
        DiscoveredLanguage::JavaScript | DiscoveredLanguage::JavaScriptReact => {
            Language::JavaScript
        }
    }
}

fn discovery_error(error: FileDiscoveryError) -> RepoGrammarError {
    match error {
        FileDiscoveryError::InvalidRoot(message) | FileDiscoveryError::Unavailable(message) => {
            RepoGrammarError::InvalidInput(message)
        }
    }
}

fn source_store_error(error: SourceStoreError) -> RepoGrammarError {
    let message = match error {
        SourceStoreError::InvalidRequest(_) => "source read request is invalid",
        SourceStoreError::Missing(_) => "source is missing",
        SourceStoreError::HashMismatch(_) => "source content changed after discovery",
        SourceStoreError::TooLarge(_) => "source exceeds configured size limit",
        SourceStoreError::NonUtf8(_) => "source is not UTF-8",
        SourceStoreError::Unavailable(_) => "source is unavailable",
    };
    RepoGrammarError::InvalidInput(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::adapters::filesystem::source_store::FilesystemSourceStore;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::core::model::{CodeUnitId, CodeUnitKind, ContentHash, Provenance, SourceRange};
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::ports::index_store::{
        GenerationHandle, IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
        StorageInspection, STORAGE_SCHEMA_VERSION,
    };
    use crate::ports::parser::{ParseDiagnostic, ParseDiagnosticSeverity};
    use crate::ports::source_store::{SourceStore, SourceText};
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use std::fs;

    fn strict_hash(value: &str) -> ContentHash {
        ContentHash::new(value).expect("valid strict hash")
    }

    fn parser_unit(
        document: &SourceDocument<'_>,
        id: &str,
        path: &str,
        content_hash: ContentHash,
        start_byte: usize,
        end_byte: usize,
    ) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(id).expect("valid unit id"),
            language: document.language.clone(),
            kind: CodeUnitKind::Module,
            range: SourceRange::new(start_byte, end_byte).expect("valid range"),
            provenance: Provenance::new(path, content_hash, document.repository_revision.clone())
                .expect("valid provenance"),
        }
    }

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
    fn syntax_only_parser_output_is_stored_in_active_generation() {
        let workspace = TempWorkspace::new("indexing-code-units");
        let sentinel = "UNIQUE_SOURCE_SENTINEL_DO_NOT_STORE";
        fs::write(
            workspace.path().join("component.tsx"),
            format!(
                "export function UserCard() {{ return <section>{sentinel}</section>; }}\n\
                 export const useUsers = () => {{ return []; }};\n"
            ),
        )
        .expect("write component");
        fs::write(
            workspace.path().join("routes.js"),
            "app.get('/users', (req, res) => { res.json([]); });\n\
             describe('users', () => { it('loads', () => {}); });\n",
        )
        .expect("write routes");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &store,
        )
        .expect("index syntax units");

        assert_eq!(outcome.discovered_files, 2);
        assert_eq!(outcome.indexed_units, 7);
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        let database = state
            .join("generations")
            .join("gen-000001")
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let rows = connection
            .prepare(
                "SELECT path, kind, start_byte, end_byte, content_hash \
                 FROM code_units ORDER BY path, start_byte, end_byte, code_unit_id",
            )
            .expect("prepare query")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .expect("query code units")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect code units");
        let kinds = rows
            .iter()
            .map(|(_, kind, _, _, _)| kind.as_str())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"module"));
        assert!(kinds.contains(&"react_component"));
        assert!(kinds.contains(&"react_hook"));
        assert!(kinds.contains(&"express_route"));
        assert!(kinds.contains(&"test_suite"));
        assert!(kinds.contains(&"test_case"));
        assert!(rows.iter().all(|(path, _kind, start, end, hash)| {
            !path.contains(workspace.path().to_string_lossy().as_ref())
                && !path.contains(sentinel)
                && hash.starts_with("sha256:")
                && start <= end
        }));
    }

    #[test]
    fn syntax_errors_store_partial_units_with_repo_relative_warning() {
        let workspace = TempWorkspace::new("indexing-syntax-diagnostic");
        fs::write(
            workspace.path().join("broken.ts"),
            "export function broken() {\n  return 1;\n",
        )
        .expect("write broken source");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &store,
        )
        .expect("index partial syntax units");

        assert!(outcome.indexed_units >= 2);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("parse diagnostic for broken.ts")));
        assert!(!outcome
            .warnings
            .iter()
            .any(|warning| warning.contains(workspace.path().to_string_lossy().as_ref())));
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
    }

    #[test]
    fn parser_diagnostic_messages_are_not_exposed_in_indexing_warnings() {
        struct LeakyDiagnosticParser;

        impl SourceParser for LeakyDiagnosticParser {
            fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
                Ok(ParseReport {
                    units: vec![parser_unit(
                        &document,
                        "unit:src/a.ts#module:0-1",
                        document.path,
                        document.content_hash.clone(),
                        0,
                        1,
                    )],
                    diagnostics: vec![ParseDiagnostic {
                        path: "/tmp/absolute/source.ts".to_string(),
                        range: None,
                        severity: ParseDiagnosticSeverity::Warning,
                        message: format!(
                            "UNIQUE_SOURCE_SENTINEL_DO_NOT_LEAK at {}",
                            "/tmp/absolute/source.ts"
                        ),
                    }],
                })
            }
        }

        let workspace = TempWorkspace::new("indexing-diagnostic-redaction");
        fs::write(workspace.path().join("a.ts"), "x").expect("write source");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &LeakyDiagnosticParser,
            &store,
        )
        .expect("index with diagnostic");

        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("parse diagnostic for a.ts"));
        assert!(!outcome.warnings[0].contains("UNIQUE_SOURCE_SENTINEL"));
        assert!(!outcome.warnings[0].contains("/tmp/absolute"));
        assert!(!outcome.warnings[0].contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn source_read_failure_preserves_previous_active_generation() {
        struct FailingSourceStore;

        impl SourceStore for FailingSourceStore {
            fn read_source(
                &self,
                request: SourceReadRequest,
            ) -> Result<SourceText, SourceStoreError> {
                Err(SourceStoreError::HashMismatch(format!(
                    "source content changed after discovery: {}",
                    request.path
                )))
            }
        }

        let workspace = TempWorkspace::new("indexing-source-fail");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);
        let first = store.prepare_next_generation().expect("prepare first");
        store.activate_generation(&first).expect("activate first");

        let error = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FailingSourceStore,
            &SyntaxCodeUnitParser,
            &store,
        )
        .expect_err("source failure must abort new generation");

        assert!(error.to_string().contains("source content changed"));
        assert_eq!(
            fs::read_to_string(state.join("current-generation"))
                .expect("read active generation")
                .trim(),
            "gen-000001"
        );
    }

    #[test]
    fn malformed_parser_units_abort_without_activating_new_generation() {
        #[derive(Clone, Copy)]
        enum BadUnitMode {
            DifferentPath,
            MismatchedHash,
            OutOfBoundsRange,
        }

        struct BadUnitParser(BadUnitMode);

        impl SourceParser for BadUnitParser {
            fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
                let (path, hash, end_byte) = match self.0 {
                    BadUnitMode::DifferentPath => {
                        ("src/other.ts", document.content_hash.clone(), document.text.len())
                    }
                    BadUnitMode::MismatchedHash => (
                        document.path,
                        strict_hash(
                            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                        ),
                        document.text.len(),
                    ),
                    BadUnitMode::OutOfBoundsRange => {
                        (document.path, document.content_hash.clone(), document.text.len() + 1)
                    }
                };
                Ok(ParseReport {
                    units: vec![parser_unit(
                        &document,
                        "unit:src/a.ts#module:0-1",
                        path,
                        hash,
                        0,
                        end_byte,
                    )],
                    diagnostics: Vec::new(),
                })
            }
        }

        for mode in [
            BadUnitMode::DifferentPath,
            BadUnitMode::MismatchedHash,
            BadUnitMode::OutOfBoundsRange,
        ] {
            let workspace = TempWorkspace::new("indexing-bad-parser-unit");
            fs::write(workspace.path().join("a.ts"), "export const a = 1;\n")
                .expect("write source");
            let state = workspace.path().join(".repogrammar");
            fs::create_dir_all(state.join("generations")).expect("create generations");
            fs::create_dir_all(state.join("tmp")).expect("create tmp");
            let store = SqliteIndexStore::new(&state);
            let first = store.prepare_next_generation().expect("prepare first");
            store.activate_generation(&first).expect("activate first");

            let error = index_repository_with_discovery_parser_and_store(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &BadUnitParser(mode),
                &store,
            )
            .expect_err("bad parser unit must abort new generation");

            assert!(
                error.to_string().contains("parser returned a code unit"),
                "unexpected error: {error}"
            );
            assert_eq!(
                fs::read_to_string(state.join("current-generation"))
                    .expect("read active generation")
                    .trim(),
                "gen-000001"
            );
        }
    }

    #[test]
    fn code_unit_record_failure_preserves_previous_active_generation() {
        struct DuplicateIdParser;

        impl SourceParser for DuplicateIdParser {
            fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
                Ok(ParseReport {
                    units: vec![
                        parser_unit(
                            &document,
                            "unit:src/a.ts#duplicate",
                            document.path,
                            document.content_hash.clone(),
                            0,
                            1,
                        ),
                        parser_unit(
                            &document,
                            "unit:src/a.ts#duplicate",
                            document.path,
                            document.content_hash.clone(),
                            1,
                            2,
                        ),
                    ],
                    diagnostics: Vec::new(),
                })
            }
        }

        let workspace = TempWorkspace::new("indexing-code-unit-record-fail");
        fs::write(workspace.path().join("a.ts"), "xy").expect("write source");
        let state = workspace.path().join(".repogrammar");
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        let store = SqliteIndexStore::new(&state);
        let first = store.prepare_next_generation().expect("prepare first");
        store.activate_generation(&first).expect("activate first");

        let _error = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &DuplicateIdParser,
            &store,
        )
        .expect_err("duplicate code unit id must abort new generation");

        assert_eq!(
            fs::read_to_string(state.join("current-generation"))
                .expect("read active generation")
                .trim(),
            "gen-000001"
        );
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

            fn record_code_unit(
                &self,
                _generation: &GenerationHandle,
                _unit: &IndexedCodeUnitRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("code unit recording must not run after file record failure")
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

            fn record_code_unit(
                &self,
                generation: &GenerationHandle,
                _unit: &IndexedCodeUnitRecord,
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
                    code_unit_count: Some(0),
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
