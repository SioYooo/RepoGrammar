//! Indexing use-case boundary.

use crate::adapters::frameworks::{java, tsjs};
use crate::adapters::parsing::java::{JAVA_ANCHOR_ENGINE, JAVA_ANCHOR_METHOD};
use crate::adapters::parsing::rust::{RUST_ANCHOR_ENGINE, RUST_ANCHOR_METHOD};
use crate::adapters::parsing::tsjs::{TSJS_ANCHOR_ENGINE, TSJS_ANCHOR_METHOD};
use crate::application::family::{
    build_family_claims, family_eligible_kind, family_storage_records,
    java_support_target_is_role_compatible, min_family_support, python_family_unknown_blocks_claim,
    python_support_target_is_role_compatible, tsjs_support_target_is_role_compatible,
    JAVA_DERIVED_SUPPORT_ENGINE, JAVA_DERIVED_SUPPORT_METHOD, RUST_DERIVED_SUPPORT_ENGINE,
    RUST_DERIVED_SUPPORT_METHOD, TSJS_DERIVED_SUPPORT_ENGINE, TSJS_DERIVED_SUPPORT_METHOD,
};
use crate::application::progress::{ProgressEvent, ProgressStage, WorkUnits};
use crate::application::proof_lattice::{derived_support_fact, DerivedSupportSpec};
use crate::core::model::{
    CodeUnit, CodeUnitId, ContentHash, Evidence, FactCertainty, FactOrigin, IrEdge, IrNode,
    Language, Provenance, RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange,
    SymbolId,
};
use crate::core::policy::paths::validate_repo_relative_path;
use crate::core::policy::rust_self_dogfood::rust_support_target_is_role_compatible;
use crate::error::RepoGrammarError;
use crate::ports::family_store::FamilyStore;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};
use crate::ports::index_store::{
    ActiveClaimInputSnapshot, GenerationHandle, IndexStorageLayout, IndexStore, IndexStoreError,
    IndexedCodeUnitRecord, IndexedFileRecord, IndexedIrEdgeRecord, IndexedIrNodeRecord,
    IndexedSemanticFactRecord, STORAGE_SCHEMA_VERSION,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, ParserProjectFileContext, ParserTsJsPathAlias,
    SourceDocument, SourceParser,
};
use crate::ports::python_provider::{
    PythonProviderCandidate, PythonProviderKind, PythonProviderOperation, PythonProviderRequest,
};
use crate::ports::rust_provider::{
    RustProviderCandidate, RustProviderError, RustProviderKind, RustProviderOperation,
    RustProviderOutput, RustProviderProvenance, RustProviderRequest, RustSemanticProvider,
};
use crate::ports::semantic_worker::{
    SemanticWorker, SemanticWorkerError, SemanticWorkerOperation, SemanticWorkerOperationKind,
    SemanticWorkerRequest,
};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingRequest {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
    pub max_file_bytes: u64,
    pub strict_gitignore: bool,
}

impl IndexingRequest {
    pub fn new(repository_root: impl Into<String>) -> Self {
        Self {
            repository_root: repository_root.into(),
            state_dir_override: None,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            strict_gitignore: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingOutcome {
    pub indexed_units: usize,
    pub semantic_facts: usize,
    pub discovered_files: usize,
    pub skipped_paths: usize,
    pub active_generation: Option<String>,
    pub semantic_worker: SemanticWorkerRunStatus,
    pub sync_report: Option<IndexingSyncReport>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexingSyncMode {
    Incremental,
    FullRebuildFallback,
}

impl IndexingSyncMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incremental => "incremental",
            Self::FullRebuildFallback => "full_rebuild_fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingSyncReport {
    pub base_generation: Option<String>,
    pub sync_mode: IndexingSyncMode,
    pub fallback_reason: Option<String>,
    pub added_files: usize,
    pub modified_files: usize,
    pub removed_files: usize,
    pub unchanged_files: usize,
    pub copied_forward_files: usize,
    pub reparsed_files: usize,
    pub families_recomputed: usize,
    pub dirty_records_cleared: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticWorkerRunStatus {
    Deferred,
    Complete,
    FallbackUnavailable,
    FallbackUnsupportedVersion,
    FallbackTimeout,
    FallbackWorkerCrashed,
    FallbackProtocolViolation,
}

impl SemanticWorkerRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deferred => "deferred",
            Self::Complete => "complete",
            Self::FallbackUnavailable => "fallback_unavailable",
            Self::FallbackUnsupportedVersion => "fallback_unsupported_version",
            Self::FallbackTimeout => "fallback_timeout",
            Self::FallbackWorkerCrashed => "fallback_worker_crashed",
            Self::FallbackProtocolViolation => "fallback_protocol_violation",
        }
    }

    fn warning_token(self) -> Option<&'static str> {
        match self {
            Self::Deferred | Self::Complete => None,
            Self::FallbackUnavailable => Some("unavailable"),
            Self::FallbackUnsupportedVersion => Some("unsupported_version"),
            Self::FallbackTimeout => Some("timeout"),
            Self::FallbackWorkerCrashed => Some("worker_crashed"),
            Self::FallbackProtocolViolation => Some("protocol_violation"),
        }
    }
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
            strict_gitignore: request.strict_gitignore,
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
        semantic_facts: 0,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: None,
        semantic_worker: SemanticWorkerRunStatus::Deferred,
        sync_report: None,
        warnings: report.warnings,
    })
}

pub fn index_repository_with_discovery_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let _index_lock = crate::application::repository::acquire_index_lock(
        &request.repository_root,
        request.state_dir_override.as_deref(),
    )?;
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
    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexed_units: 0,
        semantic_facts: 0,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        semantic_worker: SemanticWorkerRunStatus::Deferred,
        sync_report: None,
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
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: None,
            rust_provider: None,
            semantic_worker: None,
            family_store: None,
        },
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: None,
            family_store: None,
        },
        store,
        &mut progress,
    )
}

pub fn sync_repository_with_discovery_parser_frameworks_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    sync_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: None,
            family_store: None,
        },
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_families_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    store: &(impl IndexStore + FamilyStore),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: None,
            family_store: Some(store),
        },
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: None,
            family_store: Some(store),
        },
        store,
        progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_and_rust_provider: (&dyn FrameworkRoleDetector, &dyn RustSemanticProvider),
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let (framework_roles, rust_provider) = framework_and_rust_provider;
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: Some(rust_provider),
            semantic_worker: None,
            family_store: Some(store),
        },
        store,
        progress,
    )
}

pub fn index_repository_with_discovery_parser_semantic_worker_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    semantic_worker: &dyn SemanticWorker,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: None,
            rust_provider: None,
            semantic_worker: Some(semantic_worker),
            family_store: None,
        },
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_semantic_worker_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    semantic_worker: &dyn SemanticWorker,
    store: &impl IndexStore,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: Some(semantic_worker),
            family_store: None,
        },
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: &dyn FrameworkRoleDetector,
    semantic_worker: &dyn SemanticWorker,
    store: &(impl IndexStore + FamilyStore),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let mut progress = |_event: ProgressEvent| {};
    index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store_with_progress(
        request,
        discovery,
        source_store,
        parser,
        (framework_roles, semantic_worker),
        store,
        &mut progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_and_worker: (&dyn FrameworkRoleDetector, &dyn SemanticWorker),
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let (framework_roles, semantic_worker) = framework_and_worker;
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: None,
            semantic_worker: Some(semantic_worker),
            family_store: Some(store),
        },
        store,
        progress,
    )
}

pub fn index_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_worker_and_rust_provider: (
        &dyn FrameworkRoleDetector,
        &dyn SemanticWorker,
        &dyn RustSemanticProvider,
    ),
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let (framework_roles, semantic_worker, rust_provider) = framework_worker_and_rust_provider;
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: Some(rust_provider),
            semantic_worker: Some(semantic_worker),
            family_store: Some(store),
        },
        store,
        progress,
    )
}

pub fn sync_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_and_rust_provider: (&dyn FrameworkRoleDetector, &dyn RustSemanticProvider),
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let (framework_roles, rust_provider) = framework_and_rust_provider;
    sync_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: Some(rust_provider),
            semantic_worker: None,
            family_store: Some(store),
        },
        store,
        progress,
    )
}

pub fn sync_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_worker_and_rust_provider: (
        &dyn FrameworkRoleDetector,
        &dyn SemanticWorker,
        &dyn RustSemanticProvider,
    ),
    store: &(impl IndexStore + FamilyStore),
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let (framework_roles, semantic_worker, rust_provider) = framework_worker_and_rust_provider;
    sync_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            rust_provider: Some(rust_provider),
            semantic_worker: Some(semantic_worker),
            family_store: Some(store),
        },
        store,
        progress,
    )
}

fn index_repository_with_optional_semantic_worker(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    options: IndexingPipelineOptions<'_>,
    store: &impl IndexStore,
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "acquiring index lock",
        WorkUnits::Unknown,
    );
    let _index_lock = crate::application::repository::acquire_index_lock(
        &request.repository_root,
        request.state_dir_override.as_deref(),
    )?;
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "discovering repository files",
        WorkUnits::Unknown,
    );
    let report = discover_repository_files(request.clone(), discovery)?;
    let mut runtime = IndexingRuntime {
        source_store,
        parser,
        options,
        store,
        progress,
    };
    index_repository_full_after_discovery(request, report, &mut runtime, None)
}

fn index_repository_full_after_discovery<SourceStoreImpl, SourceParserImpl, IndexStoreImpl>(
    request: IndexingRequest,
    report: FileDiscoveryReport,
    runtime: &mut IndexingRuntime<'_, SourceStoreImpl, SourceParserImpl, IndexStoreImpl>,
    mut sync_report: Option<IndexingSyncReport>,
) -> Result<IndexingOutcome, RepoGrammarError>
where
    SourceStoreImpl: SourceStore,
    SourceParserImpl: SourceParser,
    IndexStoreImpl: IndexStore,
{
    let source_store = runtime.source_store;
    let parser = runtime.parser;
    let options = &runtime.options;
    let store = runtime.store;
    let progress = &mut *runtime.progress;
    emit_progress(
        progress,
        ProgressStage::FileScanning,
        "discovered files",
        known_work_units(report.files.len(), report.files.len()),
    );
    let generation = crate::application::storage::prepare_index_generation(store)?;
    for (index, file) in report.files.iter().enumerate() {
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
        emit_progress(
            progress,
            ProgressStage::FileScanning,
            "stored file metadata",
            known_work_units(index + 1, report.files.len()),
        );
    }

    let mut indexed_units = 0usize;
    let mut indexed_code_units = Vec::new();
    let mut parser_semantic_facts = Vec::new();
    let mut framework_role_facts = Vec::new();
    let mut warnings = report.warnings.clone();
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "building parser project context",
        WorkUnits::Unknown,
    );
    let parser_context = parser_project_context(&request, &report, source_store, parser)?;
    for (index, file) in report.files.iter().enumerate() {
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        let parse_report = match parser.parse_with_context(
            SourceDocument {
                path: &source.path,
                language: language_from_discovered(file.language),
                content_hash: source.content_hash.clone(),
                repository_revision: RepositoryRevision::new("UNKNOWN")
                    .expect("UNKNOWN is a non-empty repository revision marker"),
                text: &source.text,
            },
            &parser_context,
        ) {
            Ok(report) => report,
            Err(ParseError::UnsupportedLanguage) => {
                warnings.push(format!(
                    "parser skipped unsupported language: {}",
                    file.path
                ));
                emit_progress(
                    progress,
                    ProgressStage::SyntaxParsing,
                    "parsed source files",
                    known_work_units(index + 1, report.files.len()),
                );
                continue;
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {}: internal parser error",
                    file.path
                )));
            }
        };
        let parse_outcome = record_parse_report(
            store,
            &generation,
            file,
            &source.text,
            parse_report,
            options.framework_roles,
            &mut warnings,
        )?;
        indexed_units += parse_outcome.indexed_units;
        indexed_code_units.extend(parse_outcome.code_units);
        parser_semantic_facts.extend(parse_outcome.semantic_facts);
        framework_role_facts.extend(parse_outcome.framework_role_facts);
        emit_progress(
            progress,
            ProgressStage::SyntaxParsing,
            "parsed source files",
            known_work_units(index + 1, report.files.len()),
        );
    }
    emit_progress(
        progress,
        ProgressStage::CodeUnitExtractionNormalization,
        "stored code units",
        known_work_units(indexed_units, indexed_units),
    );

    sort_semantic_facts(&mut parser_semantic_facts);
    let parser_fact_count = record_semantic_facts(store, &generation, 0, &parser_semantic_facts)?;
    sort_semantic_facts(&mut framework_role_facts);
    let framework_fact_count =
        record_semantic_facts(store, &generation, parser_fact_count, &framework_role_facts)?;
    let mut derived_python_support_facts = derive_python_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_python_support_facts);
    let derived_python_support_fact_count = record_semantic_facts(
        store,
        &generation,
        parser_fact_count + framework_fact_count,
        &derived_python_support_facts,
    )?;
    let mut derived_tsjs_support_facts = derive_tsjs_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_tsjs_support_facts);
    let derived_tsjs_support_fact_count = record_semantic_facts(
        store,
        &generation,
        parser_fact_count + framework_fact_count + derived_python_support_fact_count,
        &derived_tsjs_support_facts,
    )?;
    let mut derived_java_support_facts = derive_java_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_java_support_facts);
    let derived_java_support_fact_count = record_semantic_facts(
        store,
        &generation,
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count,
        &derived_java_support_facts,
    )?;
    let mut derived_rust_support_facts = derive_rust_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_rust_support_facts);
    let derived_rust_support_fact_count = record_semantic_facts(
        store,
        &generation,
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count
            + derived_java_support_fact_count,
        &derived_rust_support_facts,
    )?;
    let local_support_fact_count = parser_fact_count
        + framework_fact_count
        + derived_python_support_fact_count
        + derived_tsjs_support_fact_count
        + derived_java_support_fact_count
        + derived_rust_support_fact_count;
    emit_progress(
        progress,
        ProgressStage::SemanticResolution,
        "recorded local support facts",
        known_work_units(local_support_fact_count, local_support_fact_count),
    );

    let rust_provider_facts = record_rust_provider_facts(
        &request,
        &indexed_code_units,
        &generation,
        options.rust_provider,
        store,
        &mut warnings,
        local_support_fact_count,
    )?;
    let rust_provider_fact_count = rust_provider_facts.len();
    if rust_provider_fact_count > 0 {
        emit_progress(
            progress,
            ProgressStage::SemanticResolution,
            "recorded rust provider facts",
            known_work_units(rust_provider_fact_count, rust_provider_fact_count),
        );
    }

    if options.semantic_worker.is_some() {
        emit_progress(
            progress,
            ProgressStage::SemanticResolution,
            "running semantic worker",
            WorkUnits::Unknown,
        );
    } else {
        emit_progress(
            progress,
            ProgressStage::SemanticResolution,
            "semantic worker deferred",
            WorkUnits::Unknown,
        );
    }
    let (semantic_worker, worker_facts) = record_semantic_worker_facts(
        SemanticWorkerFactRecording {
            request: &request,
            discovery_report: &report,
            parser_semantic_facts: &parser_semantic_facts,
            generation: &generation,
            semantic_worker: options.semantic_worker,
            fact_id_offset: local_support_fact_count + rust_provider_fact_count,
        },
        store,
        &mut warnings,
    )?;
    let worker_semantic_facts = worker_facts.len();
    if worker_semantic_facts > 0 {
        emit_progress(
            progress,
            ProgressStage::SemanticResolution,
            "recorded worker facts",
            known_work_units(worker_semantic_facts, worker_semantic_facts),
        );
    }
    let mut derived_tsjs_provider_support_facts =
        derive_tsjs_provider_resolved_framework_support_facts(
            &indexed_code_units,
            &parser_semantic_facts,
            &framework_role_facts,
            &worker_facts,
        )?;
    sort_semantic_facts(&mut derived_tsjs_provider_support_facts);
    let derived_tsjs_provider_support_fact_count = record_semantic_facts(
        store,
        &generation,
        local_support_fact_count + rust_provider_fact_count + worker_semantic_facts,
        &derived_tsjs_provider_support_facts,
    )?;
    if derived_tsjs_provider_support_fact_count > 0 {
        emit_progress(
            progress,
            ProgressStage::SemanticResolution,
            "recorded provider-resolved TS/JS support facts",
            known_work_units(
                derived_tsjs_provider_support_fact_count,
                derived_tsjs_provider_support_fact_count,
            ),
        );
    }

    if let Some(family_store) = options.family_store {
        emit_progress(
            progress,
            ProgressStage::CandidateDiscovery,
            "checking family candidates",
            WorkUnits::Unknown,
        );
        let mut family_facts = Vec::with_capacity(
            parser_semantic_facts.len()
                + framework_role_facts.len()
                + derived_python_support_facts.len()
                + derived_tsjs_support_facts.len()
                + derived_java_support_facts.len()
                + derived_rust_support_facts.len()
                + rust_provider_facts.len()
                + worker_facts.len()
                + derived_tsjs_provider_support_facts.len(),
        );
        family_facts.extend(parser_semantic_facts.iter().cloned());
        family_facts.extend(framework_role_facts.iter().cloned());
        family_facts.extend(derived_python_support_facts);
        family_facts.extend(derived_tsjs_support_facts);
        family_facts.extend(derived_java_support_facts);
        family_facts.extend(derived_rust_support_facts);
        family_facts.extend(rust_provider_facts.iter().cloned());
        family_facts.extend(worker_facts);
        family_facts.extend(derived_tsjs_provider_support_facts);
        let family_count = record_family_claims(
            family_store,
            &generation,
            &indexed_code_units,
            &family_facts,
        )?;
        if let Some(sync_report) = sync_report.as_mut() {
            sync_report.families_recomputed = family_count;
        }
        emit_progress(
            progress,
            ProgressStage::FamilyConstruction,
            "stored eligible family claims",
            WorkUnits::Unknown,
        );
    }

    emit_progress(
        progress,
        ProgressStage::PersistenceValidation,
        "validating generation",
        WorkUnits::Unknown,
    );
    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;
    emit_progress(
        progress,
        ProgressStage::PersistenceValidation,
        "activated generation",
        WorkUnits::Unknown,
    );

    Ok(IndexingOutcome {
        indexed_units,
        // `local_support_fact_count` sums local parser/framework/derived facts;
        // add provider and worker facts so the reported total matches storage.
        semantic_facts: local_support_fact_count
            + rust_provider_fact_count
            + worker_semantic_facts
            + derived_tsjs_provider_support_fact_count,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        semantic_worker,
        sync_report,
        warnings,
    })
}

fn sync_repository_with_optional_semantic_worker(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    options: IndexingPipelineOptions<'_>,
    store: &impl IndexStore,
    progress: &mut dyn FnMut(ProgressEvent),
) -> Result<IndexingOutcome, RepoGrammarError> {
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "acquiring index lock",
        WorkUnits::Unknown,
    );
    let _index_lock = crate::application::repository::acquire_index_lock(
        &request.repository_root,
        request.state_dir_override.as_deref(),
    )?;
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "discovering repository files",
        WorkUnits::Unknown,
    );
    let report = discover_repository_files(request.clone(), discovery)?;
    let semantic_worker_configured = options.semantic_worker.is_some();
    let mut runtime = IndexingRuntime {
        source_store,
        parser,
        options,
        store,
        progress,
    };
    let preflight = incremental_sync_preflight(runtime.store, semantic_worker_configured)?;
    let Some(base_generation) = preflight.base_generation.clone() else {
        let sync_report = sync_fallback_report(None, &report, None, "missing_active_generation");
        return index_repository_full_after_discovery(
            request,
            report,
            &mut runtime,
            Some(sync_report),
        );
    };
    if let Some(reason) = preflight.fallback_reason {
        let delta = if reason == "semantic_worker_requires_full_rebuild" {
            let snapshot = runtime
                .store
                .load_active_claim_input_snapshot()
                .map_err(index_store_error)?;
            Some(compute_sync_delta(&snapshot, &report))
        } else {
            None
        };
        let sync_report =
            sync_fallback_report(Some(base_generation), &report, delta.as_ref(), &reason);
        return index_repository_full_after_discovery(
            request,
            report,
            &mut runtime,
            Some(sync_report),
        );
    }

    let snapshot = runtime
        .store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    let delta = compute_sync_delta(&snapshot, &report);
    if sync_delta_touches_project_context(&delta) {
        let sync_report = sync_fallback_report(
            Some(snapshot.generation_id.clone()),
            &report,
            Some(&delta),
            "project_context_changed",
        );
        return index_repository_full_after_discovery(
            request,
            report,
            &mut runtime,
            Some(sync_report),
        );
    }

    index_repository_incremental_after_discovery(request, report, snapshot, delta, &mut runtime)
}

struct IncrementalSyncPreflight {
    base_generation: Option<String>,
    fallback_reason: Option<String>,
}

fn incremental_sync_preflight(
    store: &impl IndexStore,
    semantic_worker_configured: bool,
) -> Result<IncrementalSyncPreflight, RepoGrammarError> {
    let inspection = store.inspect().map_err(index_store_error)?;
    let mut fallback_reason = None;
    if !matches!(
        inspection.layout,
        IndexStorageLayout::Mutable | IndexStorageLayout::MutableWithLegacy
    ) {
        fallback_reason = Some("legacy_or_empty_storage_layout".to_string());
    } else if inspection.schema_version != Some(STORAGE_SCHEMA_VERSION) {
        fallback_reason = Some("unsupported_storage_schema".to_string());
    } else if inspection.active_generation.is_none() {
        fallback_reason = Some("missing_active_generation".to_string());
    } else if inspection.dirty_record_count.unwrap_or(0) != 0 {
        fallback_reason = Some("active_dirty_records".to_string());
    } else if semantic_worker_configured {
        fallback_reason = Some("semantic_worker_requires_full_rebuild".to_string());
    }
    Ok(IncrementalSyncPreflight {
        base_generation: inspection.active_generation,
        fallback_reason,
    })
}

struct SyncDelta {
    added_files: Vec<DiscoveredFile>,
    modified_files: Vec<DiscoveredFile>,
    removed_files: Vec<IndexedFileRecord>,
    unchanged_files: Vec<IndexedFileRecord>,
}

impl SyncDelta {
    fn changed_files(&self) -> Vec<DiscoveredFile> {
        let mut files = Vec::with_capacity(self.added_files.len() + self.modified_files.len());
        files.extend(self.added_files.iter().cloned());
        files.extend(self.modified_files.iter().cloned());
        files.sort_by(|left, right| left.path.cmp(&right.path));
        files
    }
}

fn compute_sync_delta(
    snapshot: &ActiveClaimInputSnapshot,
    report: &FileDiscoveryReport,
) -> SyncDelta {
    let active_by_path = snapshot
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let current_by_path = report
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut added_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut unchanged_files = Vec::new();
    let mut removed_files = Vec::new();

    for file in &report.files {
        let Some(active) = active_by_path.get(file.path.as_str()) else {
            added_files.push(file.clone());
            continue;
        };
        if active.content_hash == file.content_hash
            && active.size_bytes == file.size_bytes
            && active.language == file.language.as_str()
        {
            unchanged_files.push((*active).clone());
        } else {
            modified_files.push(file.clone());
        }
    }
    for active in &snapshot.files {
        if !current_by_path.contains_key(active.path.as_str()) {
            removed_files.push(active.clone());
        }
    }

    SyncDelta {
        added_files,
        modified_files,
        removed_files,
        unchanged_files,
    }
}

fn sync_fallback_report(
    base_generation: Option<String>,
    report: &FileDiscoveryReport,
    delta: Option<&SyncDelta>,
    reason: &str,
) -> IndexingSyncReport {
    let (added_files, modified_files, removed_files, unchanged_files) = match delta {
        Some(delta) => (
            delta.added_files.len(),
            delta.modified_files.len(),
            delta.removed_files.len(),
            delta.unchanged_files.len(),
        ),
        None => (report.files.len(), 0, 0, 0),
    };
    IndexingSyncReport {
        base_generation,
        sync_mode: IndexingSyncMode::FullRebuildFallback,
        fallback_reason: Some(reason.to_string()),
        added_files,
        modified_files,
        removed_files,
        unchanged_files,
        copied_forward_files: 0,
        reparsed_files: report.files.len(),
        families_recomputed: 0,
        dirty_records_cleared: 0,
    }
}

fn index_repository_incremental_after_discovery<SourceStoreImpl, SourceParserImpl, IndexStoreImpl>(
    request: IndexingRequest,
    report: FileDiscoveryReport,
    snapshot: ActiveClaimInputSnapshot,
    delta: SyncDelta,
    runtime: &mut IndexingRuntime<'_, SourceStoreImpl, SourceParserImpl, IndexStoreImpl>,
) -> Result<IndexingOutcome, RepoGrammarError>
where
    SourceStoreImpl: SourceStore,
    SourceParserImpl: SourceParser,
    IndexStoreImpl: IndexStore,
{
    let source_store = runtime.source_store;
    let parser = runtime.parser;
    let options = &runtime.options;
    let store = runtime.store;
    let progress = &mut *runtime.progress;
    emit_progress(
        progress,
        ProgressStage::FileScanning,
        "discovered files",
        known_work_units(report.files.len(), report.files.len()),
    );
    let generation = crate::application::storage::prepare_index_generation(store)?;
    let changed_files = delta.changed_files();
    let unchanged_paths = delta
        .unchanged_files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();

    let mut stored_files = 0usize;
    for file in &delta.unchanged_files {
        crate::application::storage::record_indexed_file(store, &generation, file)?;
        stored_files += 1;
        emit_progress(
            progress,
            ProgressStage::FileScanning,
            "stored file metadata",
            known_work_units(stored_files, report.files.len()),
        );
    }
    for file in &changed_files {
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
        stored_files += 1;
        emit_progress(
            progress,
            ProgressStage::FileScanning,
            "stored file metadata",
            known_work_units(stored_files, report.files.len()),
        );
    }

    let mut indexed_code_units = Vec::new();
    let mut copied_unit_ids = BTreeSet::new();
    for unit in &snapshot.units {
        if !unchanged_paths.contains(&unit.path) {
            continue;
        }
        crate::application::storage::record_code_unit(store, &generation, unit)?;
        copied_unit_ids.insert(unit.id.clone());
        indexed_code_units.push(unit.clone());
    }

    let mut copied_node_ids = BTreeSet::new();
    for node in &snapshot.ir_nodes {
        if !copied_unit_ids.contains(&node.code_unit_id) {
            continue;
        }
        crate::application::storage::record_ir_node(store, &generation, node)?;
        copied_node_ids.insert(node.id.clone());
    }
    for edge in &snapshot.ir_edges {
        if copied_node_ids.contains(&edge.from_node_id)
            && copied_node_ids.contains(&edge.to_node_id)
        {
            crate::application::storage::record_ir_edge(store, &generation, edge)?;
        }
    }

    let mut copied_semantic_records = Vec::new();
    let mut copied_parser_facts = Vec::new();
    let mut copied_framework_role_facts = Vec::new();
    for record in &snapshot.semantic_facts {
        if !unchanged_paths.contains(&record.path) || is_local_derived_support_record(record) {
            continue;
        }
        crate::application::storage::record_semantic_fact(store, &generation, record)?;
        let fact = semantic_fact_from_index_record(record)?;
        if fact.kind == SemanticFactKind::FrameworkRole
            && fact.certainty == FactCertainty::FrameworkHeuristic
        {
            copied_framework_role_facts.push(fact);
        } else {
            copied_parser_facts.push(fact);
        }
        copied_semantic_records.push(record.clone());
    }

    let mut warnings = report.warnings.clone();
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "building parser project context",
        WorkUnits::Unknown,
    );
    let parser_context = parser_project_context(&request, &report, source_store, parser)?;
    let mut parser_semantic_facts = Vec::new();
    let mut framework_role_facts = Vec::new();
    for (index, file) in changed_files.iter().enumerate() {
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        let parse_report = match parser.parse_with_context(
            SourceDocument {
                path: &source.path,
                language: language_from_discovered(file.language),
                content_hash: source.content_hash.clone(),
                repository_revision: RepositoryRevision::new("UNKNOWN")
                    .expect("UNKNOWN is a non-empty repository revision marker"),
                text: &source.text,
            },
            &parser_context,
        ) {
            Ok(report) => report,
            Err(ParseError::UnsupportedLanguage) => {
                warnings.push(format!(
                    "parser skipped unsupported language: {}",
                    file.path
                ));
                emit_progress(
                    progress,
                    ProgressStage::SyntaxParsing,
                    "parsed source files",
                    known_work_units(index + 1, changed_files.len()),
                );
                continue;
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {}: internal parser error",
                    file.path
                )));
            }
        };
        let parse_outcome = record_parse_report(
            store,
            &generation,
            file,
            &source.text,
            parse_report,
            options.framework_roles,
            &mut warnings,
        )?;
        indexed_code_units.extend(parse_outcome.code_units);
        parser_semantic_facts.extend(parse_outcome.semantic_facts);
        framework_role_facts.extend(parse_outcome.framework_role_facts);
        emit_progress(
            progress,
            ProgressStage::SyntaxParsing,
            "parsed source files",
            known_work_units(index + 1, changed_files.len()),
        );
    }
    emit_progress(
        progress,
        ProgressStage::CodeUnitExtractionNormalization,
        "stored code units",
        known_work_units(indexed_code_units.len(), indexed_code_units.len()),
    );

    let mut next_fact_offset = next_semantic_fact_offset(&copied_semantic_records);
    sort_semantic_facts(&mut parser_semantic_facts);
    let parser_fact_count =
        record_semantic_facts(store, &generation, next_fact_offset, &parser_semantic_facts)?;
    next_fact_offset += parser_fact_count;
    sort_semantic_facts(&mut framework_role_facts);
    let framework_fact_count =
        record_semantic_facts(store, &generation, next_fact_offset, &framework_role_facts)?;
    next_fact_offset += framework_fact_count;

    let mut all_parser_facts = copied_parser_facts;
    all_parser_facts.extend(parser_semantic_facts.iter().cloned());
    let mut all_framework_role_facts = copied_framework_role_facts;
    all_framework_role_facts.extend(framework_role_facts.iter().cloned());

    let mut derived_python_support_facts = derive_python_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_python_support_facts);
    let derived_python_support_fact_count = record_semantic_facts(
        store,
        &generation,
        next_fact_offset,
        &derived_python_support_facts,
    )?;
    next_fact_offset += derived_python_support_fact_count;
    let mut derived_tsjs_support_facts = derive_tsjs_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_tsjs_support_facts);
    let derived_tsjs_support_fact_count = record_semantic_facts(
        store,
        &generation,
        next_fact_offset,
        &derived_tsjs_support_facts,
    )?;
    next_fact_offset += derived_tsjs_support_fact_count;
    let mut derived_java_support_facts = derive_java_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_java_support_facts);
    let derived_java_support_fact_count = record_semantic_facts(
        store,
        &generation,
        next_fact_offset,
        &derived_java_support_facts,
    )?;
    next_fact_offset += derived_java_support_fact_count;
    let mut derived_rust_support_facts = derive_rust_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_rust_support_facts);
    let derived_rust_support_fact_count = record_semantic_facts(
        store,
        &generation,
        next_fact_offset,
        &derived_rust_support_facts,
    )?;
    let local_support_fact_count = copied_semantic_records.len()
        + parser_fact_count
        + framework_fact_count
        + derived_python_support_fact_count
        + derived_tsjs_support_fact_count
        + derived_java_support_fact_count
        + derived_rust_support_fact_count;
    emit_progress(
        progress,
        ProgressStage::SemanticResolution,
        "recorded local support facts",
        known_work_units(local_support_fact_count, local_support_fact_count),
    );
    emit_progress(
        progress,
        ProgressStage::SemanticResolution,
        "semantic worker deferred",
        WorkUnits::Unknown,
    );

    let mut sync_report = IndexingSyncReport {
        base_generation: Some(snapshot.generation_id),
        sync_mode: IndexingSyncMode::Incremental,
        fallback_reason: None,
        added_files: delta.added_files.len(),
        modified_files: delta.modified_files.len(),
        removed_files: delta.removed_files.len(),
        unchanged_files: delta.unchanged_files.len(),
        copied_forward_files: delta.unchanged_files.len(),
        reparsed_files: changed_files.len(),
        families_recomputed: 0,
        dirty_records_cleared: 0,
    };

    if let Some(family_store) = options.family_store {
        emit_progress(
            progress,
            ProgressStage::CandidateDiscovery,
            "checking family candidates",
            WorkUnits::Unknown,
        );
        let mut family_facts = Vec::with_capacity(
            all_parser_facts.len()
                + all_framework_role_facts.len()
                + derived_python_support_facts.len()
                + derived_tsjs_support_facts.len()
                + derived_java_support_facts.len()
                + derived_rust_support_facts.len(),
        );
        family_facts.extend(all_parser_facts);
        family_facts.extend(all_framework_role_facts);
        family_facts.extend(derived_python_support_facts);
        family_facts.extend(derived_tsjs_support_facts);
        family_facts.extend(derived_java_support_facts);
        family_facts.extend(derived_rust_support_facts);
        sync_report.families_recomputed = record_family_claims(
            family_store,
            &generation,
            &indexed_code_units,
            &family_facts,
        )?;
        emit_progress(
            progress,
            ProgressStage::FamilyConstruction,
            "stored eligible family claims",
            WorkUnits::Unknown,
        );
    }

    emit_progress(
        progress,
        ProgressStage::PersistenceValidation,
        "validating generation",
        WorkUnits::Unknown,
    );
    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;
    emit_progress(
        progress,
        ProgressStage::PersistenceValidation,
        "activated generation",
        WorkUnits::Unknown,
    );

    Ok(IndexingOutcome {
        indexed_units: indexed_code_units.len(),
        semantic_facts: local_support_fact_count,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        semantic_worker: SemanticWorkerRunStatus::Deferred,
        sync_report: Some(sync_report),
        warnings,
    })
}

fn is_local_derived_support_record(record: &IndexedSemanticFactRecord) -> bool {
    matches!(
        record.origin_engine.as_str(),
        "repogrammar-python-derived"
            | TSJS_DERIVED_SUPPORT_ENGINE
            | JAVA_DERIVED_SUPPORT_ENGINE
            | RUST_DERIVED_SUPPORT_ENGINE
    )
}

fn next_semantic_fact_offset(records: &[IndexedSemanticFactRecord]) -> usize {
    records
        .iter()
        .filter_map(|record| {
            record
                .fact_id
                .strip_prefix("semantic-fact:")
                .and_then(|suffix| suffix.parse::<usize>().ok())
        })
        .max()
        .map_or(records.len(), |index| index.saturating_add(1))
}

fn sync_delta_touches_project_context(delta: &SyncDelta) -> bool {
    delta
        .added_files
        .iter()
        .map(|file| file.path.as_str())
        .chain(delta.modified_files.iter().map(|file| file.path.as_str()))
        .chain(delta.removed_files.iter().map(|file| file.path.as_str()))
        .any(sync_path_requires_full_project_context)
}

fn sync_path_requires_full_project_context(path: &str) -> bool {
    if path.ends_with(".py") {
        return true;
    }
    matches!(
        path,
        "package.json"
            | "tsconfig.json"
            | "jsconfig.json"
            | "jest.config.json"
            | "vitest.config.json"
            | "pyproject.toml"
            | "Cargo.toml"
            | "Cargo.lock"
    ) || path == "conftest.py"
        || path.ends_with("/conftest.py")
        || path.ends_with("/Cargo.toml")
        || path.ends_with("/Cargo.lock")
}

fn emit_progress(
    progress: &mut dyn FnMut(ProgressEvent),
    stage: ProgressStage,
    message: &'static str,
    work: WorkUnits,
) {
    progress(ProgressEvent::new(stage, message, work));
}

fn known_work_units(completed: usize, total: usize) -> WorkUnits {
    let Ok(completed) = u64::try_from(completed) else {
        return WorkUnits::Unknown;
    };
    let Ok(total) = u64::try_from(total) else {
        return WorkUnits::Unknown;
    };
    WorkUnits::known(completed, total).unwrap_or(WorkUnits::Unknown)
}

#[derive(Clone, Copy)]
struct IndexingPipelineOptions<'a> {
    framework_roles: Option<&'a dyn FrameworkRoleDetector>,
    rust_provider: Option<&'a dyn RustSemanticProvider>,
    semantic_worker: Option<&'a dyn SemanticWorker>,
    family_store: Option<&'a dyn FamilyStore>,
}

struct IndexingRuntime<'a, SourceStoreImpl, SourceParserImpl, IndexStoreImpl>
where
    SourceStoreImpl: SourceStore,
    SourceParserImpl: SourceParser,
    IndexStoreImpl: IndexStore,
{
    source_store: &'a SourceStoreImpl,
    parser: &'a SourceParserImpl,
    options: IndexingPipelineOptions<'a>,
    store: &'a IndexStoreImpl,
    progress: &'a mut dyn FnMut(ProgressEvent),
}

struct ParseStorageOutcome {
    indexed_units: usize,
    code_units: Vec<IndexedCodeUnitRecord>,
    semantic_facts: Vec<SemanticFact>,
    framework_role_facts: Vec<SemanticFact>,
}

fn parser_project_context(
    request: &IndexingRequest,
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
) -> Result<ParserProjectContext, RepoGrammarError> {
    let python_module_paths = report
        .files
        .iter()
        .filter(|file| file.language == DiscoveredLanguage::Python)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let python_source_roots =
        python_source_roots_from_project_config(request, report, source_store, parser)?;
    let mut python_module_files = Vec::new();
    for file in &report.files {
        if file.language != DiscoveredLanguage::Python {
            continue;
        }
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        python_module_files.push(ParserProjectFileContext {
            path: source.path,
            text: source.text,
        });
    }
    python_module_files.sort_by(|left, right| left.path.cmp(&right.path));
    let tsjs_module_paths = tsjs_module_paths(report);
    let tsjs_path_aliases = tsjs_path_aliases_from_project_config(request, report, source_store)?;
    let tsjs_root_dirs = tsjs_root_dirs_from_project_config(request, report, source_store)?;
    let tsjs_package_dependencies =
        tsjs_package_dependencies_from_project_config(request, report, source_store)?;
    let tsjs_has_test_runner_context = tsjs_has_test_runner_context(report, source_store, request)?;
    let rust_module_paths = rust_module_paths(report);
    let mut rust_cargo_files = Vec::new();
    for file in &report.files {
        if file.language != DiscoveredLanguage::RustConfig {
            continue;
        }
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        rust_cargo_files.push(ParserProjectFileContext {
            path: source.path,
            text: source.text,
        });
    }
    rust_cargo_files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut python_conftest_files = Vec::new();
    for file in &report.files {
        if file.language != DiscoveredLanguage::Python || !is_python_conftest_path(&file.path) {
            continue;
        }
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        python_conftest_files.push(ParserProjectFileContext {
            path: source.path,
            text: source.text,
        });
    }
    python_conftest_files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(ParserProjectContext {
        python_module_paths,
        python_module_files,
        python_source_roots,
        python_conftest_files,
        tsjs_module_paths,
        tsjs_path_aliases,
        tsjs_root_dirs,
        tsjs_package_dependencies,
        tsjs_has_test_runner_context,
        rust_module_paths,
        rust_cargo_files,
    })
}

fn tsjs_package_dependencies_from_project_config(
    request: &IndexingRequest,
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
) -> Result<Vec<String>, RepoGrammarError> {
    let Some(package_file) = report.files.iter().find(|file| {
        file.language == DiscoveredLanguage::TsJsConfig && file.path == "package.json"
    }) else {
        return Ok(Vec::new());
    };
    let source = source_store
        .read_source(SourceReadRequest {
            repository_root: request.repository_root.clone(),
            path: package_file.path.clone(),
            expected_content_hash: package_file.content_hash.clone(),
            max_file_bytes: request.max_file_bytes,
        })
        .map_err(source_store_error)?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&source.text) else {
        return Ok(Vec::new());
    };
    let mut dependencies = BTreeSet::new();
    for field in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(object) = value.get(field).and_then(serde_json::Value::as_object) else {
            continue;
        };
        dependencies.extend(object.keys().cloned());
    }
    Ok(dependencies.into_iter().collect())
}

fn tsjs_module_paths(report: &FileDiscoveryReport) -> Vec<String> {
    report
        .files
        .iter()
        .filter(|file| is_tsjs_source_language(file.language))
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn rust_module_paths(report: &FileDiscoveryReport) -> Vec<String> {
    report
        .files
        .iter()
        .filter(|file| file.language == DiscoveredLanguage::Rust)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn is_tsjs_source_language(language: DiscoveredLanguage) -> bool {
    matches!(
        language,
        DiscoveredLanguage::TypeScript
            | DiscoveredLanguage::TypeScriptReact
            | DiscoveredLanguage::JavaScript
            | DiscoveredLanguage::JavaScriptReact
    )
}

fn tsjs_path_aliases_from_project_config(
    request: &IndexingRequest,
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
) -> Result<Vec<ParserTsJsPathAlias>, RepoGrammarError> {
    let mut aliases = Vec::new();
    for config_path in ["tsconfig.json", "jsconfig.json"] {
        let Some(file) = report.files.iter().find(|file| {
            file.language == DiscoveredLanguage::TsJsConfig && file.path == config_path
        }) else {
            continue;
        };
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&source.text) else {
            continue;
        };
        let Some(compiler_options) = value
            .get("compilerOptions")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        let base_url = tsjs_project_config_base_url(compiler_options.get("baseUrl"));
        let Some(paths) = compiler_options
            .get("paths")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (alias_pattern, target_patterns) in paths {
            let targets = target_patterns
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .filter_map(|target| {
                    tsjs_project_config_target_pattern(base_url.as_deref(), target)
                })
                .collect::<Vec<_>>();
            if !targets.is_empty() {
                aliases.push(ParserTsJsPathAlias {
                    alias_pattern: alias_pattern.clone(),
                    target_patterns: targets,
                });
            }
        }
    }
    aliases.sort_by(|left, right| {
        left.alias_pattern
            .cmp(&right.alias_pattern)
            .then_with(|| left.target_patterns.cmp(&right.target_patterns))
    });
    aliases.dedup();
    Ok(aliases)
}

fn tsjs_project_config_base_url(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value.and_then(serde_json::Value::as_str)?.trim();
    if value.is_empty() || value == "." || value == "./" {
        return None;
    }
    let normalized = value.trim_start_matches("./").trim_end_matches('/');
    if normalized.is_empty() || validate_repo_relative_path(normalized).is_err() {
        return None;
    }
    Some(normalized.to_string())
}

fn tsjs_project_config_target_pattern(base_url: Option<&str>, target: &str) -> Option<String> {
    let normalized = target.trim().trim_start_matches("./").trim_end_matches('/');
    if normalized.is_empty() {
        return None;
    }
    let candidate = match base_url {
        Some(base_url) => format!("{base_url}/{normalized}"),
        None => normalized.to_string(),
    };
    if validate_repo_relative_path(&candidate).is_err() {
        return None;
    }
    Some(candidate)
}

fn tsjs_root_dirs_from_project_config(
    request: &IndexingRequest,
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
) -> Result<Vec<String>, RepoGrammarError> {
    let mut root_dirs = BTreeSet::new();
    for config_path in ["tsconfig.json", "jsconfig.json"] {
        let Some(file) = report.files.iter().find(|file| {
            file.language == DiscoveredLanguage::TsJsConfig && file.path == config_path
        }) else {
            continue;
        };
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&source.text) else {
            continue;
        };
        let Some(root_dirs_value) = value
            .get("compilerOptions")
            .and_then(serde_json::Value::as_object)
            .and_then(|compiler_options| compiler_options.get("rootDirs"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        root_dirs.extend(
            root_dirs_value
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(tsjs_project_config_root_dir),
        );
    }
    Ok(root_dirs.into_iter().collect())
}

fn tsjs_project_config_root_dir(root_dir: &str) -> Option<String> {
    let normalized = root_dir
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/');
    if normalized.is_empty() || normalized.contains('*') || normalized.contains('?') {
        return None;
    }
    if validate_repo_relative_path(normalized).is_err() {
        return None;
    }
    Some(normalized.to_string())
}

fn tsjs_has_test_runner_context(
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
    request: &IndexingRequest,
) -> Result<bool, RepoGrammarError> {
    if report.files.iter().any(|file| {
        file.language == DiscoveredLanguage::TsJsConfig
            && matches!(
                file.path.as_str(),
                "jest.config.json" | "vitest.config.json"
            )
    }) {
        return Ok(true);
    }
    let Some(package_file) = report.files.iter().find(|file| {
        file.language == DiscoveredLanguage::TsJsConfig && file.path == "package.json"
    }) else {
        return Ok(false);
    };
    let source = source_store
        .read_source(SourceReadRequest {
            repository_root: request.repository_root.clone(),
            path: package_file.path.clone(),
            expected_content_hash: package_file.content_hash.clone(),
            max_file_bytes: request.max_file_bytes,
        })
        .map_err(source_store_error)?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&source.text) else {
        return Ok(false);
    };
    for field in ["dependencies", "devDependencies", "peerDependencies"] {
        if value
            .get(field)
            .and_then(serde_json::Value::as_object)
            .is_some_and(|dependencies| {
                dependencies.contains_key("vitest")
                    || dependencies.contains_key("@jest/globals")
                    || dependencies.contains_key("jest")
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_python_conftest_path(path: &str) -> bool {
    path == "conftest.py" || path.ends_with("/conftest.py")
}

fn python_source_roots_from_project_config(
    request: &IndexingRequest,
    report: &FileDiscoveryReport,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
) -> Result<Vec<String>, RepoGrammarError> {
    let Some(file) = report.files.iter().find(|file| {
        file.language == DiscoveredLanguage::PythonConfig && file.path == "pyproject.toml"
    }) else {
        return Ok(Vec::new());
    };
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
        language: Language::PythonConfig,
        content_hash: source.content_hash.clone(),
        repository_revision: RepositoryRevision::new("UNKNOWN")
            .expect("UNKNOWN is a non-empty repository revision marker"),
        text: &source.text,
    }) {
        Ok(report) => report,
        Err(ParseError::UnsupportedLanguage) => return Ok(Vec::new()),
        Err(ParseError::Internal(_)) => {
            return Err(RepoGrammarError::InvalidInput(
                "parser failed for pyproject.toml source-root context: internal parser error"
                    .to_string(),
            ));
        }
    };
    Ok(extract_python_source_roots_from_project_config_facts(
        &parse_report.semantic_facts,
    ))
}

fn extract_python_source_roots_from_project_config_facts(facts: &[SemanticFact]) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for fact in facts {
        if fact.kind != SemanticFactKind::ProjectConfig
            || fact.certainty != FactCertainty::Structural
            || fact.origin.engine != "python"
            || fact.origin.method != "tomllib"
            || !fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "python_config_field=source_roots")
        {
            continue;
        }
        for assumption in &fact.assumptions {
            let Some(root) = assumption.strip_prefix("python_config_source_root=") else {
                continue;
            };
            if validate_repo_relative_path(root).is_ok() {
                roots.insert(root.to_string());
            }
        }
    }
    roots.into_iter().collect()
}

fn record_parse_report(
    store: &impl IndexStore,
    generation: &crate::ports::index_store::GenerationHandle,
    file: &DiscoveredFile,
    text: &str,
    mut parse_report: ParseReport,
    framework_roles: Option<&dyn FrameworkRoleDetector>,
    warnings: &mut Vec<String>,
) -> Result<ParseStorageOutcome, RepoGrammarError> {
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
    for unit in &parse_report.units {
        validate_parser_unit(file, text, unit)?;
    }
    sort_ir_nodes(&mut parse_report.ir_nodes);
    sort_ir_edges(&mut parse_report.ir_edges);
    validate_parser_ir_coverage(&parse_report.units, &parse_report.ir_nodes)?;
    for node in &parse_report.ir_nodes {
        validate_parser_ir_node(file, text, &parse_report.units, node)?;
    }
    for edge in &parse_report.ir_edges {
        validate_parser_ir_edge(&parse_report.ir_nodes, edge)?;
    }
    sort_semantic_facts(&mut parse_report.semantic_facts);
    for fact in &parse_report.semantic_facts {
        validate_parser_semantic_fact(file, text, &parse_report.units, fact)?;
    }
    let framework_role_facts = match framework_roles {
        Some(detector) => detector
            .detect_roles(&parse_report.units)
            .map_err(framework_role_error)?,
        None => Vec::new(),
    };

    let mut count = 0usize;
    let mut code_units = Vec::with_capacity(parse_report.units.len());
    for unit in &parse_report.units {
        let record = IndexedCodeUnitRecord {
            id: unit.id.as_str().to_string(),
            path: unit.provenance.path.clone(),
            language: unit.language.as_str().to_string(),
            kind: unit.kind.as_str().to_string(),
            start_byte: unit.range.start_byte,
            end_byte: unit.range.end_byte,
            content_hash: unit.provenance.content_hash.clone(),
        };
        crate::application::storage::record_code_unit(store, generation, &record)?;
        code_units.push(record);
        count += 1;
    }
    for node in &parse_report.ir_nodes {
        crate::application::storage::record_ir_node(
            store,
            generation,
            &IndexedIrNodeRecord {
                id: node.id.as_str().to_string(),
                code_unit_id: node.code_unit_id.as_str().to_string(),
                kind: node.kind.as_str().to_string(),
                payload_json: "{}".to_string(),
            },
        )?;
    }
    for edge in &parse_report.ir_edges {
        crate::application::storage::record_ir_edge(
            store,
            generation,
            &IndexedIrEdgeRecord {
                from_node_id: edge.from_node_id.as_str().to_string(),
                to_node_id: edge.to_node_id.as_str().to_string(),
                label: edge.label.as_str().to_string(),
            },
        )?;
    }
    Ok(ParseStorageOutcome {
        indexed_units: count,
        code_units,
        semantic_facts: parse_report.semantic_facts,
        framework_role_facts,
    })
}

fn record_family_claims(
    store: &dyn FamilyStore,
    generation: &GenerationHandle,
    code_units: &[IndexedCodeUnitRecord],
    framework_role_facts: &[SemanticFact],
) -> Result<usize, RepoGrammarError> {
    let report = build_family_claims(code_units, framework_role_facts);
    for claim in &report.claims {
        let records = family_storage_records(claim);
        crate::application::storage::record_family(store, generation, &records.family)?;
        for member in &records.members {
            crate::application::storage::record_family_member(store, generation, member)?;
        }
        for slot in &records.variation_slots {
            crate::application::storage::record_variation_slot(store, generation, slot)?;
        }
        for evidence in &records.evidence {
            crate::application::storage::record_family_evidence(store, generation, evidence)?;
        }
    }
    Ok(report.claims.len())
}

fn record_rust_provider_facts(
    request: &IndexingRequest,
    code_units: &[IndexedCodeUnitRecord],
    generation: &GenerationHandle,
    rust_provider: Option<&dyn RustSemanticProvider>,
    store: &impl IndexStore,
    warnings: &mut Vec<String>,
    fact_id_offset: usize,
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let Some(rust_provider) = rust_provider else {
        return Ok(Vec::new());
    };
    let candidates = rust_provider_manifest_candidates(code_units)?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let project_root = Path::new(&request.repository_root);
    if !project_root.is_absolute() || !project_root.is_dir() {
        warnings.push("rust provider fallback: invalid_project_root".to_string());
        return Ok(Vec::new());
    }
    let provider_request = rust_cargo_metadata_provider_request(candidates)?;
    let output = match rust_provider.analyze_project(project_root, provider_request.clone()) {
        Ok(output) => output,
        Err(error) => {
            warnings.push(format!(
                "rust provider fallback: {}",
                rust_provider_error_token(&error)
            ));
            return Ok(Vec::new());
        }
    };
    let unknown_count = output.unknowns.len();
    let unknown_facts = rust_provider_unknown_facts(&provider_request, &output)?;
    let mut facts = output.facts;
    facts.extend(unknown_facts);
    if unknown_count > 0 {
        warnings.push(format!(
            "rust provider reported {unknown_count} typed UNKNOWN(s)"
        ));
    }
    sort_semantic_facts(&mut facts);
    record_semantic_facts(store, generation, fact_id_offset, &facts)?;
    Ok(facts)
}

fn rust_provider_manifest_candidates(
    code_units: &[IndexedCodeUnitRecord],
) -> Result<Vec<RustProviderCandidate>, RepoGrammarError> {
    let mut candidates = Vec::new();
    for unit in code_units {
        if unit.language != DiscoveredLanguage::RustConfig.as_str()
            || unit.kind != "project_config"
            || !(unit.path == "Cargo.toml" || unit.path.ends_with("/Cargo.toml"))
        {
            continue;
        }
        candidates.push(
            RustProviderCandidate::new(
                CodeUnitId::new(unit.id.clone()).map_err(RepoGrammarError::InvalidInput)?,
                unit.path.clone(),
                unit.content_hash.clone(),
                SourceRange::new(unit.start_byte, unit.end_byte)
                    .map_err(RepoGrammarError::InvalidInput)?,
                Some(unit.path.clone()),
                None,
            )
            .map_err(RepoGrammarError::InvalidInput)?,
        );
    }
    Ok(candidates)
}

fn rust_cargo_metadata_provider_request(
    mut candidates: Vec<RustProviderCandidate>,
) -> Result<RustProviderRequest, RepoGrammarError> {
    candidates.sort_by(|left, right| {
        rust_provider_candidate_sort_key(left).cmp(&rust_provider_candidate_sort_key(right))
    });
    let cargo_metadata_hash =
        rust_provider_dimension_hash("cargo_metadata_manifest_candidates_v1", &candidates);
    let cfg_profile_hash = rust_provider_dimension_hash(
        "cfg_profile_default_no_build_scripts_no_proc_macros_v1",
        &candidates,
    );
    RustProviderRequest::new(
        RustProviderKind::CargoMetadata,
        RustProviderOperation::CargoProjectModel,
        candidates,
        "unknown",
        cargo_metadata_hash,
        cfg_profile_hash,
        "default-no-build-scripts-no-proc-macros",
        false,
        false,
    )
    .map_err(RepoGrammarError::InvalidInput)
}

fn rust_provider_candidate_sort_key(
    candidate: &RustProviderCandidate,
) -> (&str, usize, usize, &str, Option<&str>, Option<&str>) {
    (
        candidate.path.as_str(),
        candidate.range.start_byte,
        candidate.range.end_byte,
        candidate.code_unit_id.as_str(),
        candidate.manifest_path.as_deref(),
        candidate.crate_root_path.as_deref(),
    )
}

fn rust_provider_dimension_hash(
    label: &'static str,
    candidates: &[RustProviderCandidate],
) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(label.as_bytes());
    for candidate in candidates {
        hasher.update(b"\0");
        hasher.update(candidate.path.as_bytes());
        hasher.update(b"\0");
        hasher.update(candidate.code_unit_id.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update(candidate.content_hash.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update(candidate.range.start_byte.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(candidate.range.end_byte.to_string().as_bytes());
    }
    let digest = hasher.finalize();
    ContentHash::new(format!("sha256:{}", bytes_to_lower_hex(digest.as_ref())))
        .expect("sha2 digest formats as strict sha256 hash")
}

fn bytes_to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn rust_provider_unknown_facts(
    request: &RustProviderRequest,
    output: &RustProviderOutput,
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let Some(candidate) = request.candidates.first() else {
        return Ok(Vec::new());
    };
    let mut facts = Vec::new();
    for (index, unknown) in output.unknowns.iter().enumerate() {
        let mut assumptions = output
            .provenance
            .as_ref()
            .map(RustProviderProvenance::assumptions)
            .unwrap_or_else(|| {
                vec![
                    "provider_resolved=false".to_string(),
                    format!("provider={}", request.provider.as_str()),
                    format!("query_operation={}", request.operation.as_str()),
                    format!("rust_toolchain={}", request.rust_toolchain),
                    format!(
                        "cargo_metadata_hash={}",
                        request.cargo_metadata_hash.as_str()
                    ),
                    format!("cfg_profile_hash={}", request.cfg_profile_hash.as_str()),
                    format!(
                        "environment_fingerprint={}",
                        request.environment_fingerprint
                    ),
                    format!("build_scripts_executed={}", request.build_scripts_executed),
                    format!("proc_macros_executed={}", request.proc_macros_executed),
                ]
            });
        assumptions.push("rust_provider_unknown=true".to_string());
        assumptions.push(format!("unknown_class={}", unknown.class.as_protocol_str()));
        assumptions.push(format!("affected_claim={}", unknown.affected_claim));
        if let Some(recovery) = &unknown.recovery {
            assumptions.push(format!(
                "recovery={}",
                sanitize_semantic_assumption(recovery)
            ));
        }
        facts.push(SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!(
                "{}#rust_provider_unknown:{index}",
                candidate.code_unit_id.as_str()
            ),
            target: Some(
                SymbolId::new(unknown.reason.as_protocol_str())
                    .map_err(RepoGrammarError::InvalidInput)?,
            ),
            origin: FactOrigin {
                engine: output
                    .provenance
                    .as_ref()
                    .map(|provenance| provenance.provider.as_str().to_string())
                    .unwrap_or_else(|| request.provider.as_str().to_string()),
                engine_version: output
                    .provenance
                    .as_ref()
                    .map(|provenance| provenance.provider_version.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                method: format!("{}_unknown", request.operation.as_str()),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(candidate.code_unit_id.as_str())
                    .map_err(RepoGrammarError::InvalidInput)?,
                SourceRange::new(candidate.range.start_byte, candidate.range.end_byte)
                    .map_err(RepoGrammarError::InvalidInput)?,
                Provenance::new(
                    &candidate.path,
                    candidate.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("UNKNOWN is a valid revision marker"),
                )
                .map_err(RepoGrammarError::InvalidInput)?,
                "Rust provider typed UNKNOWN",
            )
            .map_err(RepoGrammarError::InvalidInput)?,
            assumptions,
        });
    }
    Ok(facts)
}

fn rust_provider_error_token(error: &RustProviderError) -> &'static str {
    match error {
        RustProviderError::InvalidRequest(_) => "invalid_request",
        RustProviderError::ProtocolViolation(_) => "protocol_violation",
    }
}

fn sanitize_semantic_assumption(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn derive_python_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let blocked_units =
        python_framework_support_blocked_units(code_units, parser_facts, &role_by_unit);
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_python_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language != "python" || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if blocked_units.contains(code_unit_id) {
            continue;
        }
        let Some(target) = fact.target.as_ref().map(SymbolId::as_str) else {
            continue;
        };
        if python_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_python_framework_support_fact(
            unit,
            fact.kind.clone(),
            target,
            framework_role,
            &fact.evidence.provenance.repository_revision,
        )?);
    }

    Ok(derived)
}

fn python_framework_support_blocked_units(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut blocked = BTreeSet::new();
    for fact in parser_facts {
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language != "python" || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if python_framework_support_blocking_unknown(fact, framework_role) {
            blocked.insert(code_unit_id.to_string());
        }
    }
    blocked
}

fn python_framework_support_blocking_unknown(fact: &SemanticFact, framework_role: &str) -> bool {
    fact.origin.engine == "python"
        && fact.origin.method == "cpython_ast"
        && python_family_unknown_blocks_claim(fact, framework_role)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PythonProviderPlanKey {
    code_unit_kind: String,
    framework_role: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPythonProviderRequest {
    pub code_unit_kind: String,
    pub framework_role: String,
    pub request: PythonProviderRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePythonProviderPlanningReport {
    pub active_generation: String,
    pub requests: Vec<PlannedPythonProviderRequest>,
}

pub fn plan_active_pyrefly_framework_identity_requests(
    store: &impl IndexStore,
    python_version: impl Into<String>,
    provider_config_hash: ContentHash,
    environment_fingerprint: impl Into<String>,
) -> Result<ActivePythonProviderPlanningReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    let facts = snapshot
        .semantic_facts
        .iter()
        .map(semantic_fact_from_index_record)
        .collect::<Result<Vec<_>, _>>()?;
    let requests = plan_pyrefly_framework_identity_requests(
        &snapshot.units,
        &facts,
        python_version,
        provider_config_hash,
        environment_fingerprint,
    )?;

    Ok(ActivePythonProviderPlanningReport {
        active_generation: snapshot.generation_id,
        requests,
    })
}

pub fn plan_pyrefly_framework_identity_requests(
    code_units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
    python_version: impl Into<String>,
    provider_config_hash: ContentHash,
    environment_fingerprint: impl Into<String>,
) -> Result<Vec<PlannedPythonProviderRequest>, RepoGrammarError> {
    let python_version = python_version.into();
    let environment_fingerprint = environment_fingerprint.into();
    let role_by_unit = framework_role_targets_by_unit(facts);
    let blocked_units = python_provider_planner_blocked_units(code_units, facts);
    let mut groups: BTreeMap<PythonProviderPlanKey, Vec<&IndexedCodeUnitRecord>> = BTreeMap::new();

    for unit in code_units {
        if unit.language != "python" || !family_eligible_kind(&unit.kind) {
            continue;
        }
        if blocked_units.contains(unit.id.as_str()) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(unit.id.as_str())
            .and_then(single_framework_role)
        else {
            continue;
        };
        if !python_provider_kind_role_pair_is_supported(&unit.kind, framework_role) {
            continue;
        }
        groups
            .entry(PythonProviderPlanKey {
                code_unit_kind: unit.kind.clone(),
                framework_role: framework_role.to_string(),
            })
            .or_default()
            .push(unit);
    }

    let mut requests = Vec::new();
    for (key, units) in groups {
        if units.len() < min_family_support("python") {
            continue;
        }
        let candidates = units
            .iter()
            .map(|unit| {
                let code_unit_id =
                    CodeUnitId::new(unit.id.clone()).map_err(RepoGrammarError::InvalidInput)?;
                let range = SourceRange::new(unit.start_byte, unit.end_byte)
                    .map_err(RepoGrammarError::InvalidInput)?;
                PythonProviderCandidate::new(
                    code_unit_id,
                    unit.path.clone(),
                    unit.content_hash.clone(),
                    range,
                )
                .map_err(RepoGrammarError::InvalidInput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let request = PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
            candidates,
            python_version.clone(),
            provider_config_hash.clone(),
            environment_fingerprint.clone(),
        )
        .map_err(RepoGrammarError::InvalidInput)?;
        requests.push(PlannedPythonProviderRequest {
            code_unit_kind: key.code_unit_kind,
            framework_role: key.framework_role,
            request,
        });
    }

    Ok(requests)
}

fn python_provider_kind_role_pair_is_supported(kind: &str, framework_role: &str) -> bool {
    matches!(
        (kind, framework_role),
        ("fastapi_route", "framework:fastapi.route")
            | ("pytest_test", "framework:pytest.test")
            | ("pytest_fixture", "framework:pytest.fixture")
            | ("pydantic_model", "framework:pydantic.model")
            | ("sqlalchemy_model", "framework:sqlalchemy.model")
            | (
                "sqlalchemy_repository_method",
                "framework:sqlalchemy.repository_method"
            )
    )
}

fn semantic_fact_from_index_record(
    record: &IndexedSemanticFactRecord,
) -> Result<SemanticFact, RepoGrammarError> {
    let kind = SemanticFactKind::parse_protocol_str(&record.kind).map_err(|_| {
        RepoGrammarError::InvalidInput("stored semantic fact kind is invalid".to_string())
    })?;
    let certainty = FactCertainty::parse_protocol_str(&record.certainty).map_err(|_| {
        RepoGrammarError::InvalidInput("stored semantic fact certainty is invalid".to_string())
    })?;
    Ok(SemanticFact {
        kind,
        subject: record.subject.clone(),
        target: record
            .target
            .as_ref()
            .map(SymbolId::new)
            .transpose()
            .map_err(RepoGrammarError::InvalidInput)?,
        origin: FactOrigin {
            engine: record.origin_engine.clone(),
            engine_version: record.origin_engine_version.clone(),
            method: record.origin_method.clone(),
        },
        certainty,
        evidence: Evidence::new(
            CodeUnitId::new(record.code_unit_id.clone()).map_err(RepoGrammarError::InvalidInput)?,
            SourceRange::new(record.start_byte, record.end_byte)
                .map_err(RepoGrammarError::InvalidInput)?,
            Provenance::new(
                &record.path,
                record.content_hash.clone(),
                RepositoryRevision::new("UNKNOWN").expect("UNKNOWN is a valid revision marker"),
            )
            .map_err(RepoGrammarError::InvalidInput)?,
            &record.note,
        )
        .map_err(RepoGrammarError::InvalidInput)?,
        assumptions: record.assumptions.clone(),
    })
}

fn python_provider_planner_blocked_units(
    code_units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
) -> BTreeSet<String> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut blocked = BTreeSet::new();

    for fact in facts {
        if !python_provider_planner_blocking_unknown(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language == "python" && parser_fact_evidence_is_within_unit(fact, unit) {
            blocked.insert(unit.id.clone());
        }
    }

    blocked
}

fn python_provider_planner_blocking_unknown(fact: &SemanticFact) -> bool {
    if fact.kind != SemanticFactKind::Unknown
        || fact.certainty != FactCertainty::Unknown
        || fact.origin.engine != "python"
        || fact.origin.method != "cpython_ast"
    {
        return false;
    }

    let Some(reason) = fact.target.as_ref().map(SymbolId::as_str) else {
        return false;
    };
    match reason {
        "DynamicImport" | "RuntimeDependencyInjection" | "UnresolvedImport" => fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "affected_claim=python_import_resolution"),
        "FrameworkMagic" => fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "affected_claim=python_framework_identity"),
        "PytestFixtureInjection" | "ConflictingFacts" => fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "affected_claim=pytest_fixture_binding"),
        _ => false,
    }
}

fn framework_role_targets_by_unit(facts: &[SemanticFact]) -> BTreeMap<String, BTreeSet<String>> {
    let mut roles = BTreeMap::new();
    for fact in facts {
        if fact.kind != SemanticFactKind::FrameworkRole
            || fact.certainty != FactCertainty::FrameworkHeuristic
        {
            continue;
        }
        let Some(target) = fact.target.as_ref() else {
            continue;
        };
        roles
            .entry(fact.subject.clone())
            .or_insert_with(BTreeSet::new)
            .insert(target.as_str().to_string());
    }
    roles
}

fn single_framework_role(roles: &BTreeSet<String>) -> Option<&str> {
    if roles.len() == 1 {
        roles.iter().next().map(String::as_str)
    } else {
        None
    }
}

fn is_python_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == "python"
        && fact.origin.method == "cpython_ast"
        && fact.target.is_some()
}

fn parser_fact_evidence_is_within_unit(fact: &SemanticFact, unit: &IndexedCodeUnitRecord) -> bool {
    fact.evidence.provenance.path == unit.path
        && fact.evidence.provenance.content_hash == unit.content_hash
        && fact.evidence.range.start_byte >= unit.start_byte
        && fact.evidence.range.end_byte <= unit.end_byte
}

fn derived_python_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
) -> Result<SemanticFact, RepoGrammarError> {
    derived_support_fact(
        unit,
        kind,
        target,
        repository_revision,
        DerivedSupportSpec {
            engine: "repogrammar-python-derived",
            method: "bounded_ast_anchor_v1",
            note: "bounded Python framework anchor support",
            assumptions: vec![
                "provider_resolved=false".to_string(),
                "derived_from=cpython_ast_structural_anchors".to_string(),
                format!("framework_role={framework_role}"),
            ],
        },
    )
}

fn derive_tsjs_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_tsjs_structural_anchor_fact(fact) {
            continue;
        }
        if tsjs_provider_required_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !is_tsjs_language(&unit.language) || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        let Some(target) = fact.target.as_ref().map(SymbolId::as_str) else {
            continue;
        };
        if tsjs_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_tsjs_framework_support_fact(
            unit,
            fact.kind.clone(),
            target,
            framework_role,
            &fact.evidence.provenance.repository_revision,
            fact,
        )?);
    }

    Ok(derived)
}

fn derive_tsjs_provider_resolved_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
    worker_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let export_proofs = tsjs_provider_resolved_export_proofs(worker_facts);
    let binding_proofs = tsjs_provider_resolved_binding_proofs(worker_facts);
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_tsjs_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !is_tsjs_language(&unit.language) || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        let Some(target) = fact.target.as_ref().map(SymbolId::as_str) else {
            continue;
        };
        if tsjs_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        let Some((proof_kind, worker_fact)) =
            tsjs_provider_resolved_proof_for_framework_fact(fact, &export_proofs, &binding_proofs)
        else {
            continue;
        };
        if !seen.insert((unit.id.clone(), target.to_string(), proof_kind)) {
            continue;
        }
        derived.push(derived_tsjs_provider_resolved_framework_support_fact(
            unit,
            fact.kind.clone(),
            target,
            framework_role,
            &fact.evidence.provenance.repository_revision,
            fact,
            worker_fact,
        )?);
    }

    Ok(derived)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TsJsProviderExportProofKey {
    path: String,
    content_hash: String,
    code_unit_id: String,
    start_byte: usize,
    end_byte: usize,
    export_name: String,
}

impl TsJsProviderExportProofKey {
    fn from_fact(fact: &SemanticFact, export_name: &str) -> Self {
        Self {
            path: fact.evidence.provenance.path.clone(),
            content_hash: fact.evidence.provenance.content_hash.as_str().to_string(),
            code_unit_id: fact.evidence.code_unit_id.as_str().to_string(),
            start_byte: fact.evidence.range.start_byte,
            end_byte: fact.evidence.range.end_byte,
            export_name: export_name.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TsJsProviderBindingProofKey {
    path: String,
    content_hash: String,
    code_unit_id: String,
    start_byte: usize,
    end_byte: usize,
    literal_specifier: String,
}

impl TsJsProviderBindingProofKey {
    fn from_fact(fact: &SemanticFact, literal_specifier: &str) -> Self {
        Self {
            path: fact.evidence.provenance.path.clone(),
            content_hash: fact.evidence.provenance.content_hash.as_str().to_string(),
            code_unit_id: fact.evidence.code_unit_id.as_str().to_string(),
            start_byte: fact.evidence.range.start_byte,
            end_byte: fact.evidence.range.end_byte,
            literal_specifier: literal_specifier.to_string(),
        }
    }
}

fn tsjs_provider_resolved_proof_for_framework_fact<'a>(
    fact: &SemanticFact,
    export_proofs: &'a BTreeMap<TsJsProviderExportProofKey, SemanticFact>,
    binding_proofs: &'a BTreeMap<TsJsProviderBindingProofKey, SemanticFact>,
) -> Option<(String, &'a SemanticFact)> {
    if let Some(export_name) = tsjs_framework_export_operation_literal(fact) {
        let proof_key = TsJsProviderExportProofKey::from_fact(fact, &export_name);
        if let Some(worker_fact) = export_proofs.get(&proof_key) {
            return Some((format!("export:{export_name}"), worker_fact));
        }
    }
    if let Some(literal_specifier) = tsjs_provider_binding_operation_literal(fact) {
        let proof_key = TsJsProviderBindingProofKey::from_fact(fact, &literal_specifier);
        if let Some(worker_fact) = binding_proofs.get(&proof_key) {
            if tsjs_provider_resolved_binding_kind_matches(fact, worker_fact) {
                return Some((format!("binding:{literal_specifier}"), worker_fact));
            }
        }
    }
    None
}

fn tsjs_provider_resolved_export_proofs(
    worker_facts: &[SemanticFact],
) -> BTreeMap<TsJsProviderExportProofKey, SemanticFact> {
    let mut proofs = BTreeMap::new();
    for fact in worker_facts {
        if !tsjs_provider_resolved_export_fact(fact) {
            continue;
        }
        let Some(export_name) = fact_assumption_value(fact, "tsjs_export_name=") else {
            continue;
        };
        proofs.insert(
            TsJsProviderExportProofKey::from_fact(fact, export_name),
            fact.clone(),
        );
    }
    proofs
}

fn tsjs_provider_resolved_binding_proofs(
    worker_facts: &[SemanticFact],
) -> BTreeMap<TsJsProviderBindingProofKey, SemanticFact> {
    let mut proofs = BTreeMap::new();
    for fact in worker_facts {
        if !tsjs_provider_resolved_binding_fact(fact) {
            continue;
        }
        let Some(export_name) = fact_assumption_value(fact, "tsjs_export_name=") else {
            continue;
        };
        let Some(import_specifier) = fact_assumption_value(fact, "tsjs_import_specifier=") else {
            continue;
        };
        proofs.insert(
            TsJsProviderBindingProofKey::from_fact(
                fact,
                &format!("{import_specifier}#{export_name}"),
            ),
            fact.clone(),
        );
    }
    proofs
}

fn tsjs_provider_resolved_export_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall | SemanticFactKind::Symbol | SemanticFactKind::Type
    ) && tsjs_provider_resolved_compiler_fact(fact)
        && fact_assumption_value(fact, "query_operation=") == Some("resolve_export")
}

fn tsjs_provider_resolved_binding_fact(fact: &SemanticFact) -> bool {
    matches!(fact.kind, SemanticFactKind::Symbol | SemanticFactKind::Type)
        && tsjs_provider_resolved_compiler_fact(fact)
        && fact_assumption_value(fact, "query_operation=") == Some("resolve_reexport")
}

fn tsjs_provider_resolved_binding_kind_matches(
    anchor_fact: &SemanticFact,
    worker_fact: &SemanticFact,
) -> bool {
    match fact_assumption_value(anchor_fact, "binding_kind=") {
        Some("prisma_client") => matches!(
            worker_fact.kind,
            SemanticFactKind::Symbol | SemanticFactKind::Type
        ),
        Some("tsjs_route_handler") => matches!(worker_fact.kind, SemanticFactKind::Symbol),
        _ => false,
    }
}

fn tsjs_provider_resolved_compiler_fact(fact: &SemanticFact) -> bool {
    fact.certainty == FactCertainty::Semantic
        && fact.origin.engine == "typescript"
        && fact.origin.method == "compiler_api_module_resolver_v1"
        && fact_assumption_value(fact, "provider=") == Some("typescript")
        && fact_assumption_value(fact, "provider_resolved=") == Some("true")
}

fn derived_tsjs_provider_resolved_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
    worker_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider=typescript".to_string(),
        "provider_resolved=true".to_string(),
        "derived_from=tsjs_structural_anchors".to_string(),
    ];
    if let Some(derived_from) = tsjs::derived_from_for_target(target) {
        assumptions.push(format!("derived_from={derived_from}"));
    }
    assumptions.push(format!("framework_role={framework_role}"));
    for assumption in &source_fact.assumptions {
        if !assumptions.iter().any(|existing| existing == assumption)
            && !assumption.starts_with("provider=")
            && !assumption.starts_with("provider_resolved=")
            && !assumption.starts_with("framework_role=")
        {
            assumptions.push(assumption.clone());
        }
    }
    for assumption in &worker_fact.assumptions {
        if tsjs_worker_support_assumption_is_safe(assumption)
            && !assumptions.iter().any(|existing| existing == assumption)
        {
            assumptions.push(assumption.clone());
        }
    }
    if !assumptions
        .iter()
        .any(|assumption| assumption.starts_with("tsjs_anchor_kind="))
    {
        assumptions.push(format!("tsjs_anchor_kind={}", unit.kind));
    }
    assumptions.sort();
    assumptions.dedup();
    derived_support_fact(
        unit,
        kind,
        target,
        repository_revision,
        DerivedSupportSpec {
            engine: TSJS_DERIVED_SUPPORT_ENGINE,
            method: TSJS_DERIVED_SUPPORT_METHOD,
            note: "provider-resolved TS/JS framework binding support",
            assumptions,
        },
    )
}

fn tsjs_worker_support_assumption_is_safe(assumption: &str) -> bool {
    assumption == "provider=typescript"
        || assumption == "provider_resolved=true"
        || assumption.starts_with("operation_id=")
        || assumption.starts_with("query_operation=")
        || assumption.starts_with("tsconfig_hash=")
        || assumption.starts_with("package_json_hash=")
        || assumption.starts_with("environment_fingerprint=")
        || assumption.starts_with("tsjs_export_name=")
        || assumption.starts_with("tsjs_import_specifier=")
        || assumption.starts_with("tsjs_import_resolution=")
}

fn tsjs_provider_required_anchor_fact(fact: &SemanticFact) -> bool {
    fact.assumptions
        .iter()
        .any(|assumption| assumption == "provider_required=typescript")
}

fn is_tsjs_language(language: &str) -> bool {
    language == "typescript" || language == "javascript"
}

fn is_tsjs_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == TSJS_ANCHOR_ENGINE
        && fact.origin.method == TSJS_ANCHOR_METHOD
        && fact.target.is_some()
}

fn derived_tsjs_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        "derived_from=tsjs_structural_anchors".to_string(),
    ];
    if let Some(derived_from) = tsjs::derived_from_for_target(target) {
        assumptions.push(format!("derived_from={derived_from}"));
    }
    assumptions.push(format!("framework_role={framework_role}"));
    for assumption in &source_fact.assumptions {
        if !assumptions.iter().any(|existing| existing == assumption)
            && !assumption.starts_with("provider_resolved=")
            && !assumption.starts_with("framework_role=")
        {
            assumptions.push(assumption.clone());
        }
    }
    if !assumptions
        .iter()
        .any(|assumption| assumption.starts_with("tsjs_anchor_kind="))
    {
        assumptions.push(format!("tsjs_anchor_kind={}", unit.kind));
    }
    derived_support_fact(
        unit,
        kind,
        target,
        repository_revision,
        DerivedSupportSpec {
            engine: TSJS_DERIVED_SUPPORT_ENGINE,
            method: TSJS_DERIVED_SUPPORT_METHOD,
            note: "bounded TS/JS framework anchor support",
            assumptions,
        },
    )
}

fn derive_java_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let blocked_units = java_framework_support_blocked_units(code_units, parser_facts);
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_java_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language != "java" || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if blocked_units.contains(code_unit_id) {
            continue;
        }
        let Some(target) = fact.target.as_ref().map(SymbolId::as_str) else {
            continue;
        };
        if java_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_java_framework_support_fact(
            unit,
            fact.kind.clone(),
            target,
            framework_role,
            &fact.evidence.provenance.repository_revision,
            fact,
        )?);
    }

    Ok(derived)
}

fn java_framework_support_blocked_units(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
) -> BTreeSet<String> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut blocked = BTreeSet::new();
    for fact in parser_facts {
        if !java_framework_support_blocking_unknown(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language == "java" && parser_fact_evidence_is_within_unit(fact, unit) {
            blocked.insert(code_unit_id.to_string());
        }
    }
    blocked
}

fn java_framework_support_blocking_unknown(fact: &SemanticFact) -> bool {
    if fact.kind != SemanticFactKind::Unknown
        || fact.certainty != FactCertainty::Unknown
        || fact.origin.engine != JAVA_ANCHOR_ENGINE
        || fact.origin.method != JAVA_ANCHOR_METHOD
    {
        return false;
    }
    let Some(reason) = fact.target.as_ref().map(SymbolId::as_str) else {
        return false;
    };
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("java_family_membership");
    match reason {
        "UnresolvedImport"
        | "MissingProjectConfig"
        | "MissingDependency"
        | "FrameworkMagic"
        | "ConflictingFacts"
        | "StaleEvidence" => {
            matches!(
                affected_claim,
                "java_family_membership"
                    | "java_spring_annotation_binding"
                    | "java_spring_controller_identity"
                    | "java_spring_framework_identity"
                    | "java_spring_repository_identity"
            ) || affected_claim.starts_with("family:")
        }
        _ => false,
    }
}

fn is_java_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == JAVA_ANCHOR_ENGINE
        && fact.origin.method == JAVA_ANCHOR_METHOD
        && fact.target.is_some()
}

fn derived_java_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        "derived_from=tree_sitter_java_structural_anchors".to_string(),
        format!("framework_role={framework_role}"),
        format!(
            "derived_from={}",
            java::support_family(target, framework_role)
        ),
    ];
    assumptions.extend(
        source_fact
            .assumptions
            .iter()
            .filter(|assumption| {
                assumption.starts_with("java_anchor_kind=")
                    || assumption.starts_with("spring_annotation=")
                    || assumption.starts_with("http_method=")
                    || assumption.starts_with("route_path_shape=")
                    || assumption.starts_with("class_route_path_shape=")
                    || assumption.starts_with("java_visibility_shape=")
                    || assumption.starts_with("java_return_shape=")
                    || assumption.starts_with("java_parameter_shape=")
                    || assumption.starts_with("java_class_shape=")
            })
            .cloned(),
    );
    assumptions.sort();
    assumptions.dedup();

    derived_support_fact(
        unit,
        kind,
        target,
        repository_revision,
        DerivedSupportSpec {
            engine: JAVA_DERIVED_SUPPORT_ENGINE,
            method: JAVA_DERIVED_SUPPORT_METHOD,
            note: "bounded Java Spring structural role support",
            assumptions,
        },
    )
}

fn derive_rust_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let blocked_units = rust_framework_support_blocked_units(code_units, parser_facts);
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_rust_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language != "rust" || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if blocked_units.contains(code_unit_id) {
            continue;
        }
        let Some(target) = fact.target.as_ref().map(SymbolId::as_str) else {
            continue;
        };
        if rust_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_rust_framework_support_fact(
            unit,
            fact.kind.clone(),
            target,
            framework_role,
            &fact.evidence.provenance.repository_revision,
            fact,
        )?);
    }

    Ok(derived)
}

fn rust_framework_support_blocked_units(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
) -> BTreeSet<String> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut blocked = BTreeSet::new();
    for fact in parser_facts {
        if !rust_framework_support_blocking_unknown(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language == "rust" && parser_fact_evidence_is_within_unit(fact, unit) {
            blocked.insert(code_unit_id.to_string());
        }
    }
    blocked
}

fn rust_framework_support_blocking_unknown(fact: &SemanticFact) -> bool {
    if fact.kind != SemanticFactKind::Unknown
        || fact.certainty != FactCertainty::Unknown
        || fact.origin.engine != RUST_ANCHOR_ENGINE
        || fact.origin.method != RUST_ANCHOR_METHOD
    {
        return false;
    }
    let Some(reason) = fact.target.as_ref().map(SymbolId::as_str) else {
        return false;
    };
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("rust_family_membership");
    match reason {
        "MacroOrPreprocessor"
        | "BuildVariantAmbiguity"
        | "ConflictingFacts"
        | "StaleEvidence"
        | "UnresolvedImport"
        | "FrameworkMagic" => matches!(
            affected_claim,
            "rust_family_membership"
                | "rust_macro_expansion"
                | "rust_build_variant"
                | "rust_trait_dispatch"
                | "rust_module_resolution"
        ),
        _ => false,
    }
}

fn is_rust_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == RUST_ANCHOR_ENGINE
        && fact.origin.method == RUST_ANCHOR_METHOD
        && fact.target.is_some()
}

fn derived_rust_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        "derived_from=tree_sitter_rust_structural_anchors".to_string(),
        format!("framework_role={framework_role}"),
    ];
    assumptions.extend(
        source_fact
            .assumptions
            .iter()
            .filter(|assumption| {
                assumption.starts_with("rust_anchor_kind=")
                    || assumption.starts_with("rust_signature_shape=")
                    || assumption.starts_with("rust_error_shape=")
                    || assumption.starts_with("rust_call_shape=")
                    || assumption.starts_with("rust_control_shape=")
                    || assumption.starts_with("rust_path_context=")
            })
            .cloned(),
    );
    assumptions.sort();
    assumptions.dedup();

    derived_support_fact(
        unit,
        kind,
        target,
        repository_revision,
        DerivedSupportSpec {
            engine: RUST_DERIVED_SUPPORT_ENGINE,
            method: RUST_DERIVED_SUPPORT_METHOD,
            note: "bounded Rust structural role support",
            assumptions,
        },
    )
}

struct SemanticWorkerFactRecording<'a> {
    request: &'a IndexingRequest,
    discovery_report: &'a FileDiscoveryReport,
    parser_semantic_facts: &'a [SemanticFact],
    generation: &'a GenerationHandle,
    semantic_worker: Option<&'a dyn SemanticWorker>,
    fact_id_offset: usize,
}

fn record_semantic_worker_facts(
    input: SemanticWorkerFactRecording<'_>,
    store: &impl IndexStore,
    warnings: &mut Vec<String>,
) -> Result<(SemanticWorkerRunStatus, Vec<SemanticFact>), RepoGrammarError> {
    let Some(semantic_worker) = input.semantic_worker else {
        return Ok((SemanticWorkerRunStatus::Deferred, Vec::new()));
    };

    if input.discovery_report.files.is_empty() {
        return Ok((SemanticWorkerRunStatus::Deferred, Vec::new()));
    }

    let changed_files = input
        .discovery_report
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let operations = tsjs_semantic_worker_operations(
        input.request,
        input.discovery_report,
        input.parser_semantic_facts,
    );
    let mut facts = match semantic_worker.analyze_project(SemanticWorkerRequest {
        project_root: input.request.repository_root.clone(),
        changed_files,
        operations,
    }) {
        Ok(facts) => facts,
        Err(error) => {
            let status = semantic_worker_error_status(error);
            if let Some(token) = status.warning_token() {
                warnings.push(format!("semantic worker fallback: {token}"));
            }
            return Ok((status, Vec::new()));
        }
    };

    sort_semantic_facts(&mut facts);
    record_semantic_facts(store, input.generation, input.fact_id_offset, &facts)?;
    Ok((SemanticWorkerRunStatus::Complete, facts))
}

fn tsjs_semantic_worker_operations(
    request: &IndexingRequest,
    discovery_report: &FileDiscoveryReport,
    parser_semantic_facts: &[SemanticFact],
) -> Vec<SemanticWorkerOperation> {
    let file_hash_by_path = discovery_report
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.content_hash.as_str().to_string()))
        .collect::<BTreeMap<_, _>>();
    let project_config_hash = file_hash_by_path
        .get("tsconfig.json")
        .or_else(|| file_hash_by_path.get("jsconfig.json"))
        .cloned()
        .unwrap_or_else(zero_content_hash);
    let package_json_hash = file_hash_by_path
        .get("package.json")
        .cloned()
        .unwrap_or_else(zero_content_hash);
    let operation_context = TsJsSemanticWorkerOperationContext {
        project_config_hash,
        package_json_hash,
        max_files: discovery_report.files.len().max(1),
        max_bytes: request.max_file_bytes,
    };
    let mut operations = Vec::new();

    for fact in parser_semantic_facts {
        if !tsjs_parser_import_resolution_fact(fact) {
            continue;
        }
        let Some(literal_specifier) = fact_assumption_value(fact, "literal_specifier=") else {
            continue;
        };
        let Some(operation_kind) = tsjs_semantic_worker_operation_kind(fact) else {
            continue;
        };
        push_tsjs_semantic_worker_operation(
            &mut operations,
            fact,
            operation_kind,
            literal_specifier,
            &operation_context,
        );
    }
    for fact in parser_semantic_facts {
        let Some(literal_specifier) = tsjs_framework_export_operation_literal(fact) else {
            continue;
        };
        push_tsjs_semantic_worker_operation(
            &mut operations,
            fact,
            SemanticWorkerOperationKind::ResolveExport,
            &literal_specifier,
            &operation_context,
        );
    }
    for fact in parser_semantic_facts {
        let Some(literal_specifier) = tsjs_provider_binding_operation_literal(fact) else {
            continue;
        };
        push_tsjs_semantic_worker_operation(
            &mut operations,
            fact,
            SemanticWorkerOperationKind::ResolveReexport,
            &literal_specifier,
            &operation_context,
        );
    }

    operations
}

struct TsJsSemanticWorkerOperationContext {
    project_config_hash: String,
    package_json_hash: String,
    max_files: usize,
    max_bytes: u64,
}

fn push_tsjs_semantic_worker_operation(
    operations: &mut Vec<SemanticWorkerOperation>,
    fact: &SemanticFact,
    operation: SemanticWorkerOperationKind,
    literal_specifier: &str,
    context: &TsJsSemanticWorkerOperationContext,
) {
    operations.push(SemanticWorkerOperation {
        operation_id: format!("tsjs-op-{:06}", operations.len()),
        operation,
        path: fact.evidence.provenance.path.clone(),
        content_hash: fact.evidence.provenance.content_hash.as_str().to_string(),
        code_unit_id: fact.evidence.code_unit_id.as_str().to_string(),
        start_byte: fact.evidence.range.start_byte,
        end_byte: fact.evidence.range.end_byte,
        literal_specifier: literal_specifier.to_string(),
        project_config_hash: context.project_config_hash.clone(),
        package_json_hash: context.package_json_hash.clone(),
        max_files: context.max_files,
        max_bytes: context.max_bytes,
    });
}

fn tsjs_semantic_worker_operation_kind(fact: &SemanticFact) -> Option<SemanticWorkerOperationKind> {
    match fact_assumption_value(fact, "affected_claim=") {
        Some("tsjs_reexport_resolution") => Some(SemanticWorkerOperationKind::ResolveReexport),
        Some("tsjs_export_resolution") => Some(SemanticWorkerOperationKind::ResolveExport),
        Some("tsjs_package_entry") => Some(SemanticWorkerOperationKind::ResolvePackageEntry),
        Some("tsjs_import_resolution") | None => {
            Some(SemanticWorkerOperationKind::ResolveModuleSpecifier)
        }
        Some(_) => None,
    }
}

fn tsjs_parser_import_resolution_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedImport | SemanticFactKind::Unknown
    ) && fact.origin.engine == crate::adapters::parsing::tsjs::TSJS_ANCHOR_ENGINE
        && fact.origin.method == "bounded_import_resolver_v1"
        && fact_assumption_value(fact, "literal_specifier=").is_some()
}

fn tsjs_framework_export_operation_literal(fact: &SemanticFact) -> Option<String> {
    if !is_tsjs_structural_anchor_fact(fact) {
        return None;
    }
    match fact_assumption_value(fact, "tsjs_anchor_kind=") {
        Some("next_route_handler") => {
            fact_assumption_value(fact, "http_method=").map(ToString::to_string)
        }
        Some("next_app_page" | "next_app_layout" | "next_pages_api_route" | "next_pages_page") => {
            Some("default".to_string())
        }
        _ => None,
    }
}

fn tsjs_provider_binding_operation_literal(fact: &SemanticFact) -> Option<String> {
    if !is_tsjs_structural_anchor_fact(fact) || !tsjs_provider_required_anchor_fact(fact) {
        return None;
    }
    let import_specifier = fact_assumption_value(fact, "binding_import_specifier=")?;
    let export_name = fact_assumption_value(fact, "binding_export_name=")?;
    Some(format!("{import_specifier}#{export_name}"))
}

fn zero_content_hash() -> String {
    "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()
}

fn fact_assumption_value<'a>(fact: &'a SemanticFact, prefix: &str) -> Option<&'a str> {
    fact.assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix(prefix))
}

fn record_semantic_facts(
    store: &impl IndexStore,
    generation: &GenerationHandle,
    fact_id_offset: usize,
    facts: &[SemanticFact],
) -> Result<usize, RepoGrammarError> {
    for (index, fact) in facts.iter().enumerate() {
        crate::application::storage::record_semantic_fact(
            store,
            generation,
            &indexed_semantic_fact_record(fact_id_offset + index, fact),
        )?;
    }
    Ok(facts.len())
}

fn framework_role_error(error: FrameworkRoleError) -> RepoGrammarError {
    match error {
        FrameworkRoleError::InvalidFact(message) => RepoGrammarError::InvalidInput(message),
    }
}

fn semantic_worker_error_status(error: SemanticWorkerError) -> SemanticWorkerRunStatus {
    match error {
        SemanticWorkerError::Unavailable(_) => SemanticWorkerRunStatus::FallbackUnavailable,
        SemanticWorkerError::UnsupportedVersion(_) => {
            SemanticWorkerRunStatus::FallbackUnsupportedVersion
        }
        SemanticWorkerError::Timeout(_) => SemanticWorkerRunStatus::FallbackTimeout,
        SemanticWorkerError::WorkerCrashed(_) => SemanticWorkerRunStatus::FallbackWorkerCrashed,
        SemanticWorkerError::ProtocolViolation(_) => {
            SemanticWorkerRunStatus::FallbackProtocolViolation
        }
    }
}

fn sort_semantic_facts(facts: &mut [SemanticFact]) {
    facts.sort_by(|left, right| {
        (
            left.evidence.provenance.path.as_str(),
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
            left.evidence.code_unit_id.as_str(),
            left.kind.as_protocol_str(),
            left.subject.as_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.certainty.as_protocol_str(),
            left.origin.engine.as_str(),
            left.origin.engine_version.as_str(),
            left.origin.method.as_str(),
        )
            .cmp(&(
                right.evidence.provenance.path.as_str(),
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.evidence.code_unit_id.as_str(),
                right.kind.as_protocol_str(),
                right.subject.as_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.certainty.as_protocol_str(),
                right.origin.engine.as_str(),
                right.origin.engine_version.as_str(),
                right.origin.method.as_str(),
            ))
    });
}

fn indexed_semantic_fact_record(index: usize, fact: &SemanticFact) -> IndexedSemanticFactRecord {
    IndexedSemanticFactRecord {
        fact_id: format!("semantic-fact:{index:06}"),
        kind: fact.kind.as_protocol_str().to_string(),
        subject: fact.subject.clone(),
        target: fact
            .target
            .as_ref()
            .map(|target| target.as_str().to_string()),
        certainty: fact.certainty.as_protocol_str().to_string(),
        origin_engine: fact.origin.engine.clone(),
        origin_engine_version: fact.origin.engine_version.clone(),
        origin_method: fact.origin.method.clone(),
        assumptions: fact.assumptions.clone(),
        evidence_id: format!("semantic-evidence:{index:06}"),
        code_unit_id: fact.evidence.code_unit_id.as_str().to_string(),
        path: fact.evidence.provenance.path.clone(),
        content_hash: fact.evidence.provenance.content_hash.clone(),
        start_byte: fact.evidence.range.start_byte,
        end_byte: fact.evidence.range.end_byte,
        note: fact.evidence.note.clone(),
    }
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

fn validate_parser_ir_node(
    file: &DiscoveredFile,
    text: &str,
    units: &[CodeUnit],
    node: &IrNode,
) -> Result<(), RepoGrammarError> {
    if node.provenance.path != file.path {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node for a different path".to_string(),
        ));
    }
    if node.provenance.content_hash != file.content_hash {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node with mismatched content hash".to_string(),
        ));
    }
    if node.range.end_byte > text.len() {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node range outside source bounds".to_string(),
        ));
    }
    let Some(unit) = units
        .iter()
        .find(|unit| unit.id.as_str() == node.code_unit_id.as_str())
    else {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node for an unknown code unit".to_string(),
        ));
    };
    if node.range != unit.range || node.provenance != unit.provenance {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node that does not match its code unit evidence".to_string(),
        ));
    }
    if node.id.as_str() != format!("ir:{}", unit.id.as_str()) {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR node id that does not match its code unit".to_string(),
        ));
    }
    Ok(())
}

fn validate_parser_ir_coverage(
    units: &[CodeUnit],
    nodes: &[IrNode],
) -> Result<(), RepoGrammarError> {
    let mut node_code_unit_ids = BTreeSet::new();
    for node in nodes {
        if !node_code_unit_ids.insert(node.code_unit_id.as_str()) {
            return Err(RepoGrammarError::InvalidInput(
                "parser returned duplicate IR nodes for a code unit".to_string(),
            ));
        }
    }
    for unit in units {
        if !node_code_unit_ids.contains(unit.id.as_str()) {
            return Err(RepoGrammarError::InvalidInput(
                "parser did not return IR for every code unit".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_parser_ir_edge(nodes: &[IrNode], edge: &IrEdge) -> Result<(), RepoGrammarError> {
    if edge.from_node_id == edge.to_node_id {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned an IR self-edge".to_string(),
        ));
    }
    for (label, node_id) in [
        ("from", edge.from_node_id.as_str()),
        ("to", edge.to_node_id.as_str()),
    ] {
        if !nodes.iter().any(|node| node.id.as_str() == node_id) {
            return Err(RepoGrammarError::InvalidInput(format!(
                "parser returned an IR edge with unknown {label} node"
            )));
        }
    }
    Ok(())
}

fn validate_parser_semantic_fact(
    file: &DiscoveredFile,
    text: &str,
    units: &[CodeUnit],
    fact: &SemanticFact,
) -> Result<(), RepoGrammarError> {
    if !matches!(
        fact.certainty,
        FactCertainty::Structural | FactCertainty::Unknown
    ) && !is_python_parser_graph_derived_fact(fact)
    {
        return Err(RepoGrammarError::InvalidInput(
            "parser semantic facts must stay structural or unknown unless explicitly graph-derived"
                .to_string(),
        ));
    }
    if fact.kind == SemanticFactKind::Unknown && fact.certainty != FactCertainty::Unknown {
        return Err(RepoGrammarError::InvalidInput(
            "parser UNKNOWN facts must use UNKNOWN certainty".to_string(),
        ));
    }
    if fact.evidence.provenance.path != file.path {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a semantic fact for a different path".to_string(),
        ));
    }
    if fact.evidence.provenance.content_hash != file.content_hash {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a semantic fact with mismatched content hash".to_string(),
        ));
    }
    if fact.evidence.range.end_byte > text.len() {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a semantic fact range outside source bounds".to_string(),
        ));
    }
    let Some(unit) = units
        .iter()
        .find(|unit| unit.id.as_str() == fact.evidence.code_unit_id.as_str())
    else {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a semantic fact for an unknown code unit".to_string(),
        ));
    };
    if fact.evidence.range.start_byte < unit.range.start_byte
        || fact.evidence.range.end_byte > unit.range.end_byte
        || fact.evidence.provenance != unit.provenance
    {
        return Err(RepoGrammarError::InvalidInput(
            "parser returned a semantic fact that does not match its code unit evidence"
                .to_string(),
        ));
    }
    Ok(())
}

fn is_python_parser_graph_derived_fact(fact: &SemanticFact) -> bool {
    if fact.certainty != FactCertainty::DataflowDerived
        || fact.origin.engine != "python"
        || fact.origin.method != "cpython_ast"
        || fact.assumptions.len() != 3
        || !fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "provider_resolved=false")
    {
        return false;
    }
    let anchor = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("python_anchor_kind="));
    let derived_from = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("derived_from="));
    matches!(
        (fact.kind.clone(), anchor, derived_from),
        (
            SemanticFactKind::ResolvedImport,
            Some("repo_local_import_binding"),
            Some("repo_local_python_import_graph")
        ) | (
            SemanticFactKind::Symbol | SemanticFactKind::Type,
            Some("repo_local_import_symbol"),
            Some("repo_local_python_import_graph")
        ) | (
            SemanticFactKind::Symbol,
            Some("pytest_fixture_edge" | "pytest_conftest_fixture_edge"),
            Some("repo_local_pytest_fixture_graph")
        )
    )
}

fn sort_ir_nodes(nodes: &mut [IrNode]) {
    nodes.sort_by(|left, right| {
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
}

fn sort_ir_edges(edges: &mut [IrEdge]) {
    edges.sort_by(|left, right| {
        (
            left.from_node_id.as_str(),
            left.to_node_id.as_str(),
            left.label.as_str(),
        )
            .cmp(&(
                right.from_node_id.as_str(),
                right.to_node_id.as_str(),
                right.label.as_str(),
            ))
    });
}

fn language_from_discovered(language: DiscoveredLanguage) -> Language {
    match language {
        DiscoveredLanguage::TypeScript | DiscoveredLanguage::TypeScriptReact => {
            Language::TypeScript
        }
        DiscoveredLanguage::JavaScript | DiscoveredLanguage::JavaScriptReact => {
            Language::JavaScript
        }
        DiscoveredLanguage::Python => Language::Python,
        DiscoveredLanguage::PythonConfig => Language::PythonConfig,
        DiscoveredLanguage::TsJsConfig => Language::TsJsConfig,
        DiscoveredLanguage::Java => Language::Java,
        DiscoveredLanguage::Rust => Language::Rust,
        DiscoveredLanguage::RustConfig => Language::RustConfig,
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
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::adapters::filesystem::source_store::FilesystemSourceStore;
    use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
    use crate::adapters::parsing::python::PythonAstParser;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::adapters::parsing::RepoGrammarSourceParser;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::application::query::{assess_semantic_fact_readiness, SemanticFactReadinessRequest};
    use crate::core::model::{
        CodeUnitId, CodeUnitKind, ContentHash, Evidence, FactCertainty, FactOrigin, IrEdgeLabel,
        IrNodeId, Provenance, RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange,
        UnknownClass, UnknownReasonCode,
    };
    use crate::core::policy::freshness::ClaimInputReadiness;
    use crate::ports::family_store::FamilyStore;
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveSemanticFacts, GenerationHandle, IndexStorageLayout, IndexStore, IndexStoreError,
        IndexedCodeUnitRecord, IndexedFileRecord, IndexedSemanticFactRecord, StorageInspection,
        STORAGE_SCHEMA_VERSION,
    };
    use crate::ports::parser::{ParseDiagnostic, ParseDiagnosticSeverity, ParserProjectContext};
    use crate::ports::semantic_worker::{
        SemanticWorker, SemanticWorkerError, SemanticWorkerRequest,
    };
    use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceText};
    use crate::test_support::TempWorkspace;
    use rusqlite::{params, Connection, OptionalExtension};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn strict_hash(value: &str) -> ContentHash {
        ContentHash::new(value).expect("valid strict hash")
    }

    #[test]
    fn sync_project_context_gate_covers_root_and_nested_config_paths() {
        for path in [
            "package.json",
            "tsconfig.json",
            "jsconfig.json",
            "jest.config.json",
            "vitest.config.json",
            "pyproject.toml",
            "src/app.py",
            "src/conftest_helper.py",
            "Cargo.toml",
            "Cargo.lock",
            "conftest.py",
            "tests/conftest.py",
            "crates/demo/Cargo.toml",
            "crates/demo/Cargo.lock",
        ] {
            assert!(sync_path_requires_full_project_context(path), "{path}");
        }
        for path in ["src/app.ts", "Cargo.locked", "docs/Cargo.toml.md"] {
            assert!(!sync_path_requires_full_project_context(path), "{path}");
        }
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

    fn project_config_source_root_fact(
        document: &SourceDocument<'_>,
        unit: &CodeUnit,
        root: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ProjectConfig,
            subject: unit.id.as_str().to_string(),
            target: Some(
                SymbolId::new(format!(
                    "python.project_config.source_root.{}",
                    root.replace('/', ".")
                ))
                .expect("valid source-root target"),
            ),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "UNKNOWN".to_string(),
                method: "tomllib".to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                unit.id.clone(),
                unit.range.clone(),
                Provenance::new(
                    document.path,
                    document.content_hash.clone(),
                    document.repository_revision.clone(),
                )
                .expect("valid provenance"),
                "Python project config structural fact",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "python_config_field=source_roots".to_string(),
                format!("python_config_source_root={root}"),
                "parsed_with=tomllib".to_string(),
                "not_family_claim_input".to_string(),
            ],
        }
    }

    struct SingleUnitParser;

    impl SourceParser for SingleUnitParser {
        fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            let unit = parser_unit(
                &document,
                "unit:a.ts#module:0-all",
                document.path,
                document.content_hash.clone(),
                0,
                document.text.len(),
            );
            let ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
            Ok(ParseReport {
                units: vec![unit],
                ir_nodes: vec![ir_node],
                ir_edges: Vec::new(),
                semantic_facts: Vec::new(),
                diagnostics: Vec::new(),
            })
        }
    }

    struct FakeRustProjectModelProvider;

    impl RustSemanticProvider for FakeRustProjectModelProvider {
        fn analyze_project(
            &self,
            _project_root: &Path,
            request: RustProviderRequest,
        ) -> Result<RustProviderOutput, RustProviderError> {
            let candidate = request
                .candidates
                .first()
                .expect("test request has a Cargo.toml candidate");
            let provenance = RustProviderProvenance::new(
                request.provider,
                "test-provider",
                request.rust_toolchain.clone(),
                request.cargo_metadata_hash.clone(),
                request.cfg_profile_hash.clone(),
                request.environment_fingerprint.clone(),
                request.operation,
                request.build_scripts_executed,
                request.proc_macros_executed,
            )
            .expect("valid fake Rust provenance");
            let mut assumptions = provenance.assumptions();
            assumptions.push("cargo_fact=workspace".to_string());
            Ok(RustProviderOutput::facts(
                provenance,
                vec![SemanticFact {
                    kind: SemanticFactKind::ProjectConfig,
                    subject: format!("{}#fake_cargo_metadata", candidate.code_unit_id.as_str()),
                    target: Some(SymbolId::new("cargo.workspace").expect("valid target")),
                    origin: FactOrigin {
                        engine: "cargo_metadata".to_string(),
                        engine_version: "test-provider".to_string(),
                        method: "cargo_metadata_no_deps_v1".to_string(),
                    },
                    certainty: FactCertainty::Semantic,
                    evidence: Evidence::new(
                        CodeUnitId::new(candidate.code_unit_id.as_str()).expect("valid unit id"),
                        SourceRange::new(candidate.range.start_byte, candidate.range.end_byte)
                            .expect("valid range"),
                        Provenance::new(
                            &candidate.path,
                            candidate.content_hash.clone(),
                            RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                        )
                        .expect("valid provenance"),
                        "fake Cargo metadata workspace model",
                    )
                    .expect("valid evidence"),
                    assumptions,
                }],
                Vec::new(),
            ))
        }
    }

    struct ParserSemanticFactParser;

    impl SourceParser for ParserSemanticFactParser {
        fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            let unit = parser_unit(
                &document,
                "unit:a.ts#module:0-all",
                document.path,
                document.content_hash.clone(),
                0,
                document.text.len(),
            );
            let ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
            Ok(ParseReport {
                units: vec![unit],
                ir_nodes: vec![ir_node],
                ir_edges: Vec::new(),
                semantic_facts: vec![SemanticFact {
                    kind: SemanticFactKind::ResolvedImport,
                    subject: format!("{}#import:express", document.path),
                    target: Some(SymbolId::new("fastapi.APIRouter").expect("valid target")),
                    origin: FactOrigin {
                        engine: "python".to_string(),
                        engine_version: "3.13.0".to_string(),
                        method: "cpython_ast".to_string(),
                    },
                    certainty: FactCertainty::Semantic,
                    evidence: Evidence::new(
                        CodeUnitId::new("unit:a.ts#module:0-all").expect("valid unit id"),
                        SourceRange::new(0, document.text.len()).expect("valid range"),
                        Provenance::new(
                            document.path,
                            document.content_hash.clone(),
                            document.repository_revision.clone(),
                        )
                        .expect("valid provenance"),
                        "parser-origin semantic import must be rejected",
                    )
                    .expect("valid evidence"),
                    assumptions: Vec::new(),
                }],
                diagnostics: Vec::new(),
            })
        }
    }

    struct ExpressRouteParser;

    impl SourceParser for ExpressRouteParser {
        fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            let mut unit = parser_unit(
                &document,
                "unit:a.ts#express:0-all",
                document.path,
                document.content_hash.clone(),
                0,
                document.text.len(),
            );
            unit.kind = CodeUnitKind::ExpressRoute;
            let ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
            Ok(ParseReport {
                units: vec![unit],
                ir_nodes: vec![ir_node],
                ir_edges: Vec::new(),
                semantic_facts: Vec::new(),
                diagnostics: Vec::new(),
            })
        }
    }

    struct RecordingContextParser {
        contexts: Mutex<Vec<ParserProjectContext>>,
    }

    impl RecordingContextParser {
        fn new() -> Self {
            Self {
                contexts: Mutex::new(Vec::new()),
            }
        }
    }

    impl SourceParser for RecordingContextParser {
        fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            let unit_id = format!("unit:{}#module:0-all", document.path);
            let unit = parser_unit(
                &document,
                &unit_id,
                document.path,
                document.content_hash.clone(),
                0,
                document.text.len(),
            );
            let ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
            let mut semantic_facts = Vec::new();
            if document.language == Language::PythonConfig && document.path == "pyproject.toml" {
                semantic_facts.extend(
                    ["src", "src/lib", "tests"]
                        .into_iter()
                        .map(|root| project_config_source_root_fact(&document, &unit, root)),
                );
            }
            Ok(ParseReport {
                units: vec![unit],
                ir_nodes: vec![ir_node],
                ir_edges: Vec::new(),
                semantic_facts,
                diagnostics: Vec::new(),
            })
        }

        fn parse_with_context(
            &self,
            document: SourceDocument<'_>,
            context: &ParserProjectContext,
        ) -> Result<ParseReport, ParseError> {
            self.contexts
                .lock()
                .expect("record contexts")
                .push(context.clone());
            self.parse(document)
        }
    }

    struct StaticSemanticWorker {
        expected_files: Vec<String>,
        result: Result<Vec<SemanticFact>, SemanticWorkerError>,
    }

    impl SemanticWorker for StaticSemanticWorker {
        fn analyze_project(
            &self,
            request: SemanticWorkerRequest,
        ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
            assert!(Path::new(&request.project_root).is_absolute());
            assert_eq!(request.changed_files, self.expected_files);
            self.result.clone()
        }
    }

    struct RecordingSemanticWorker {
        requests: Mutex<Vec<SemanticWorkerRequest>>,
    }

    impl RecordingSemanticWorker {
        fn new() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl SemanticWorker for RecordingSemanticWorker {
        fn analyze_project(
            &self,
            request: SemanticWorkerRequest,
        ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
            self.requests
                .lock()
                .expect("record semantic worker request")
                .push(request);
            Ok(Vec::new())
        }
    }

    struct PanickingSemanticWorker;

    impl SemanticWorker for PanickingSemanticWorker {
        fn analyze_project(
            &self,
            _request: SemanticWorkerRequest,
        ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
            panic!("semantic worker must not run when no files are discovered")
        }
    }

    struct CountingDiscovery<'a> {
        calls: &'a AtomicUsize,
    }

    impl FileDiscovery for CountingDiscovery<'_> {
        fn discover(
            &self,
            _request: FileDiscoveryRequest,
        ) -> Result<FileDiscoveryReport, FileDiscoveryError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(FileDiscoveryReport {
                files: Vec::new(),
                skipped: Vec::new(),
                warnings: Vec::new(),
                git_ignore_status: GitIgnoreStatus::NotRepository,
            })
        }
    }

    fn semantic_fact_for_a_ts(content_hash: ContentHash) -> SemanticFact {
        semantic_fact_for_unit(
            content_hash,
            "unit:a.ts#module:0-all",
            "a.ts",
            "import express from 'express';\n".len(),
        )
    }

    fn semantic_fact_for_unit(
        content_hash: ContentHash,
        code_unit_id: &str,
        path: &str,
        end_byte: usize,
    ) -> SemanticFact {
        semantic_fact_for_unit_with_target(content_hash, code_unit_id, path, 0, end_byte, None)
    }

    fn semantic_fact_for_unit_with_target(
        content_hash: ContentHash,
        code_unit_id: &str,
        path: &str,
        start_byte: usize,
        end_byte: usize,
        target: Option<&str>,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedImport,
            subject: format!("{path}#import:express"),
            target: target.map(|target| SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                CodeUnitId::new(code_unit_id).expect("valid code unit id"),
                SourceRange::new(start_byte, end_byte).expect("valid range"),
                Provenance::new(
                    path,
                    content_hash,
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "compiler resolved import target",
            )
            .expect("valid evidence"),
            assumptions: Vec::new(),
        }
    }

    fn indexed_python_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: "python".to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn parser_structural_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        kind: SemanticFactKind,
        target: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "3.13.0".to_string(),
                method: "cpython_ast".to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte + 1, unit.end_byte - 1).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "CPython ast structural decorator_binding",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "python_anchor_kind=decorator_binding".to_string(),
                "binding unresolved without provider".to_string(),
            ],
        }
    }

    fn parser_project_config_anchor_fact(unit: &IndexedCodeUnitRecord) -> SemanticFact {
        let mut fact = parser_structural_anchor_fact(
            unit,
            SemanticFactKind::ResolvedImport,
            "fastapi.APIRouter.get",
        );
        fact.kind = SemanticFactKind::ProjectConfig;
        fact.origin.method = "tomllib".to_string();
        fact
    }

    fn framework_role_fact_for_unit(unit: &IndexedCodeUnitRecord, role: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::FrameworkRole,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(role).expect("valid role")),
            origin: FactOrigin {
                engine: "repogrammar-frameworks".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: "syntax_code_unit_kind".to_string(),
            },
            certainty: FactCertainty::FrameworkHeuristic,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "CPython ast code unit indicates framework role",
            )
            .expect("valid evidence"),
            assumptions: vec!["binding unresolved without provider".to_string()],
        }
    }

    fn indexed_tsjs_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: "typescript".to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn indexed_java_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: "java".to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn tsjs_structural_anchor_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: TSJS_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: TSJS_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded TS/JS exact framework anchor",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("tsjs_anchor_kind={}", unit.kind)],
        }
    }

    fn tsjs_next_route_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        method: &str,
    ) -> SemanticFact {
        let mut fact = tsjs_structural_anchor_fact(unit, target);
        fact.assumptions.push(format!("http_method={method}"));
        fact
    }

    fn tsjs_provider_required_prisma_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
    ) -> SemanticFact {
        let mut fact = tsjs_structural_anchor_fact(unit, target);
        fact.assumptions.extend([
            "provider_required=typescript".to_string(),
            "binding_kind=prisma_client".to_string(),
            "binding_local_name=prisma".to_string(),
            "binding_import_specifier=./db".to_string(),
            "binding_export_name=prisma".to_string(),
            "required_mechanism=typescript_export_graph".to_string(),
        ]);
        fact
    }

    fn tsjs_provider_required_route_handler_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
    ) -> SemanticFact {
        let mut fact = tsjs_structural_anchor_fact(unit, target);
        fact.assumptions.extend([
            "provider_required=typescript".to_string(),
            "binding_kind=tsjs_route_handler".to_string(),
            "binding_local_name=listUsers".to_string(),
            "binding_import_specifier=./handlers".to_string(),
            "binding_export_name=listUsers".to_string(),
            "required_mechanism=typescript_export_graph".to_string(),
        ]);
        fact
    }

    fn tsjs_provider_export_fact(unit: &IndexedCodeUnitRecord, export_name: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Symbol,
            subject: format!("{}#resolve_export:{export_name}", unit.path),
            target: Some(
                SymbolId::new(format!("symbol:{}#export:{export_name}", unit.path))
                    .expect("valid target"),
            ),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api_module_resolver_v1".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "TypeScript compiler resolved TS/JS export symbol",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "provider=typescript".to_string(),
                "provider_resolved=true".to_string(),
                "environment_fingerprint=node_typescript_compiler_api_v1".to_string(),
                format!("operation_id=op-{export_name}"),
                "query_operation=resolve_export".to_string(),
                "tsconfig_hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                "package_json_hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
                format!("tsjs_export_name={export_name}"),
            ],
        }
    }

    fn tsjs_provider_binding_fact(
        unit: &IndexedCodeUnitRecord,
        import_specifier: &str,
        export_name: &str,
    ) -> SemanticFact {
        tsjs_provider_binding_fact_with_kind(
            unit,
            import_specifier,
            export_name,
            SemanticFactKind::Symbol,
        )
    }

    fn tsjs_provider_binding_fact_with_kind(
        unit: &IndexedCodeUnitRecord,
        import_specifier: &str,
        export_name: &str,
        kind: SemanticFactKind,
    ) -> SemanticFact {
        SemanticFact {
            kind,
            subject: format!("{}#resolve_reexport:{import_specifier}#{export_name}", unit.path),
            target: Some(
                SymbolId::new(format!("symbol:src/db.ts#export:{export_name}"))
                    .expect("valid target"),
            ),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api_module_resolver_v1".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "TypeScript compiler resolved TS/JS re-export symbol",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "provider=typescript".to_string(),
                "provider_resolved=true".to_string(),
                "environment_fingerprint=node_typescript_compiler_api_v1".to_string(),
                format!("operation_id=op-{export_name}"),
                "query_operation=resolve_reexport".to_string(),
                "tsconfig_hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                "package_json_hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
                format!("tsjs_export_name={export_name}"),
                format!("tsjs_import_specifier={import_specifier}"),
                "tsjs_import_resolution=compiler_api".to_string(),
            ],
        }
    }

    fn java_structural_anchor_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: JAVA_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: JAVA_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded Java Spring structural role anchor",
            )
            .expect("valid evidence"),
            assumptions: vec![
                format!("java_anchor_kind={}", unit.kind),
                "spring_annotation=GetMapping".to_string(),
                "http_method=GET".to_string(),
                "route_path_shape=literal".to_string(),
                "class_route_path_shape=none".to_string(),
            ],
        }
    }

    fn java_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid reason")),
            origin: FactOrigin {
                engine: JAVA_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: JAVA_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "Java parser typed UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn exact_express_route_anchors_derive_family_support() {
        let first = indexed_tsjs_unit("src/a.ts", "express_route", 0);
        let second = indexed_tsjs_unit("src/b.ts", "express_route", 1);
        let third = indexed_tsjs_unit("src/c.ts", "express_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            tsjs_structural_anchor_fact(&first, "express.route.get"),
            tsjs_structural_anchor_fact(&second, "express.route.post"),
            tsjs_structural_anchor_fact(&third, "express.route.delete"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:express.route_handler"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact express support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == TSJS_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == TSJS_DERIVED_SUPPORT_METHOD
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=tsjs_structural_anchors")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "typescript");
        assert_eq!(
            report.claims[0].framework_role,
            "framework:express.route_handler"
        );
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn exact_java_spring_anchors_derive_family_support_without_unresolved_imports() {
        let first = indexed_java_unit("src/main/java/AController.java", "spring_mvc_route", 0);
        let second = indexed_java_unit("src/main/java/BController.java", "spring_mvc_route", 1);
        let third = indexed_java_unit("src/main/java/CController.java", "spring_mvc_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            java_structural_anchor_fact(&first, "spring.web.bind.annotation.GetMapping"),
            java_structural_anchor_fact(&second, "spring.web.bind.annotation.GetMapping"),
            java_structural_anchor_fact(&third, "spring.web.bind.annotation.GetMapping"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:spring.mvc_route"))
            .collect::<Vec<_>>();

        let derived = derive_java_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact java spring support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == JAVA_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == JAVA_DERIVED_SUPPORT_METHOD
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "derived_from=tree_sitter_java_structural_anchors"
                })
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "java");
        assert_eq!(
            report.claims[0].framework_role,
            "framework:spring.mvc_route"
        );
        assert_eq!(report.claims[0].support, 3);

        let blocked = derive_java_framework_support_facts(
            std::slice::from_ref(&first),
            &[
                java_structural_anchor_fact(&first, "spring.web.bind.annotation.GetMapping"),
                java_unknown_fact(
                    &first,
                    UnknownReasonCode::UnresolvedImport,
                    "java_spring_annotation_binding",
                ),
            ],
            &[framework_role_fact_for_unit(
                &first,
                "framework:spring.mvc_route",
            )],
        )
        .expect("derive with blocking unknown");
        assert!(blocked.is_empty());
    }

    #[test]
    fn tsjs_framework_adapter_exact_anchors_derive_role_compatible_support() {
        for (kind, role, target, derived_from) in [
            (
                "next_app_page",
                "framework:next.app.page",
                "next.app.page",
                "tsjs_next_structural_anchors",
            ),
            (
                "next_route_handler",
                "framework:next.route.handler",
                "next.route.GET",
                "tsjs_next_structural_anchors",
            ),
            (
                "fastify_route",
                "framework:fastify.route_handler",
                "fastify.route.get",
                "tsjs_fastify_structural_anchors",
            ),
            (
                "fastify_route",
                "framework:fastify.route_handler",
                "fastify.route.route",
                "tsjs_fastify_structural_anchors",
            ),
            (
                "prisma_query",
                "framework:prisma.query",
                "prisma.query.findMany",
                "tsjs_prisma_structural_anchors",
            ),
            (
                "drizzle_query",
                "framework:drizzle.query",
                "drizzle.query.select",
                "tsjs_drizzle_structural_anchors",
            ),
        ] {
            let first = indexed_tsjs_unit("src/a.ts", kind, 0);
            let second = indexed_tsjs_unit("src/b.ts", kind, 1);
            let third = indexed_tsjs_unit("src/c.ts", kind, 2);
            let units = vec![first.clone(), second.clone(), third.clone()];
            let parser_facts = vec![
                tsjs_structural_anchor_fact(&first, target),
                tsjs_structural_anchor_fact(&second, target),
                tsjs_structural_anchor_fact(&third, target),
            ];
            let role_facts = units
                .iter()
                .map(|unit| framework_role_fact_for_unit(unit, role))
                .collect::<Vec<_>>();

            let derived = derive_tsjs_framework_support_facts(&units, &parser_facts, &role_facts)
                .expect("derive exact adapter support");

            assert_eq!(derived.len(), 3, "{role} should derive support");
            assert!(derived.iter().all(|fact| fact
                .assumptions
                .iter()
                .any(|assumption| assumption == &format!("derived_from={derived_from}"))));
            let mut family_facts = role_facts;
            family_facts.extend(derived);
            let report = build_family_claims(&units, &family_facts);
            assert_eq!(report.claims.len(), 1, "{role} should form a family");
            assert_eq!(report.claims[0].framework_role, role);
            assert_eq!(report.claims[0].support, 3);
        }
    }

    #[test]
    fn provider_resolved_tsjs_export_facts_derive_next_support() {
        let first = indexed_tsjs_unit("app/users/route.ts", "next_route_handler", 0);
        let second = indexed_tsjs_unit("app/accounts/route.ts", "next_route_handler", 1);
        let third = indexed_tsjs_unit("app/orders/route.ts", "next_route_handler", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            tsjs_next_route_anchor_fact(&first, "next.route.GET", "GET"),
            tsjs_next_route_anchor_fact(&second, "next.route.GET", "GET"),
            tsjs_next_route_anchor_fact(&third, "next.route.GET", "GET"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:next.route.handler"))
            .collect::<Vec<_>>();
        let worker_facts = units
            .iter()
            .map(|unit| tsjs_provider_export_fact(unit, "GET"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_provider_resolved_framework_support_facts(
            &units,
            &parser_facts,
            &role_facts,
            &worker_facts,
        )
        .expect("derive provider-resolved next support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == TSJS_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == TSJS_DERIVED_SUPPORT_METHOD
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider=typescript")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=true")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "query_operation=resolve_export")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=tsjs_next_structural_anchors")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(
            report.claims[0].framework_role,
            "framework:next.route.handler"
        );
    }

    #[test]
    fn provider_resolved_tsjs_export_support_rejects_fallback_or_mismatched_worker_facts() {
        let unit = indexed_tsjs_unit("app/users/route.ts", "next_route_handler", 0);
        let parser_fact = tsjs_next_route_anchor_fact(&unit, "next.route.GET", "GET");
        let role_fact = framework_role_fact_for_unit(&unit, "framework:next.route.handler");

        let mut fallback_fact = tsjs_provider_export_fact(&unit, "GET");
        fallback_fact.certainty = FactCertainty::Structural;
        fallback_fact.origin.engine = "repogrammar-tsjs-static-worker".to_string();
        fallback_fact.origin.method = "bounded_project_model_resolver_v1".to_string();
        fallback_fact
            .assumptions
            .retain(|assumption| assumption != "provider=typescript");
        fallback_fact
            .assumptions
            .retain(|assumption| assumption != "provider_resolved=true");
        fallback_fact
            .assumptions
            .push("provider=repogrammar_static_tsjs".to_string());
        fallback_fact
            .assumptions
            .push("provider_resolved=false".to_string());

        let fallback = derive_tsjs_provider_resolved_framework_support_facts(
            std::slice::from_ref(&unit),
            std::slice::from_ref(&parser_fact),
            std::slice::from_ref(&role_fact),
            std::slice::from_ref(&fallback_fact),
        )
        .expect("fallback worker fact is valid input");
        assert!(fallback.is_empty());

        let mut mismatched = tsjs_provider_export_fact(&unit, "default");
        mismatched
            .assumptions
            .retain(|assumption| !assumption.starts_with("tsjs_export_name="));
        mismatched
            .assumptions
            .push("tsjs_export_name=default".to_string());
        let mismatch = derive_tsjs_provider_resolved_framework_support_facts(
            &[unit],
            &[parser_fact],
            &[role_fact],
            &[mismatched],
        )
        .expect("mismatched worker fact is valid input");
        assert!(mismatch.is_empty());
    }

    #[test]
    fn provider_required_tsjs_anchors_do_not_derive_structural_support() {
        let unit = indexed_tsjs_unit("src/repository.ts", "prisma_query", 0);
        let parser_fact = tsjs_provider_required_prisma_anchor_fact(&unit, "prisma.query.findMany");
        let role_fact = framework_role_fact_for_unit(&unit, "framework:prisma.query");

        let derived = derive_tsjs_framework_support_facts(
            std::slice::from_ref(&unit),
            std::slice::from_ref(&parser_fact),
            std::slice::from_ref(&role_fact),
        )
        .expect("derive structural tsjs support");

        assert!(derived.is_empty());
    }

    #[test]
    fn provider_resolved_tsjs_binding_facts_derive_prisma_support() {
        let first = indexed_tsjs_unit("src/users.ts", "prisma_query", 0);
        let second = indexed_tsjs_unit("src/accounts.ts", "prisma_query", 1);
        let third = indexed_tsjs_unit("src/orders.ts", "prisma_query", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            tsjs_provider_required_prisma_anchor_fact(&first, "prisma.query.findMany"),
            tsjs_provider_required_prisma_anchor_fact(&second, "prisma.query.findMany"),
            tsjs_provider_required_prisma_anchor_fact(&third, "prisma.query.findMany"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:prisma.query"))
            .collect::<Vec<_>>();
        let worker_facts = units
            .iter()
            .map(|unit| tsjs_provider_binding_fact(unit, "./db", "prisma"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_provider_resolved_framework_support_facts(
            &units,
            &parser_facts,
            &role_facts,
            &worker_facts,
        )
        .expect("derive provider-resolved prisma support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider=typescript")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=true")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "query_operation=resolve_reexport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "binding_kind=prisma_client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=tsjs_prisma_structural_anchors")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].framework_role, "framework:prisma.query");
    }

    #[test]
    fn provider_resolved_tsjs_route_handler_bindings_derive_express_support() {
        let first = indexed_tsjs_unit("src/users.ts", "express_route", 0);
        let second = indexed_tsjs_unit("src/accounts.ts", "express_route", 1);
        let third = indexed_tsjs_unit("src/orders.ts", "express_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            tsjs_provider_required_route_handler_anchor_fact(&first, "express.route.get"),
            tsjs_provider_required_route_handler_anchor_fact(&second, "express.route.get"),
            tsjs_provider_required_route_handler_anchor_fact(&third, "express.route.get"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:express.route_handler"))
            .collect::<Vec<_>>();
        let worker_facts = units
            .iter()
            .map(|unit| tsjs_provider_binding_fact(unit, "./handlers", "listUsers"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_provider_resolved_framework_support_facts(
            &units,
            &parser_facts,
            &role_facts,
            &worker_facts,
        )
        .expect("derive provider-resolved express handler support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "binding_kind=tsjs_route_handler")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "query_operation=resolve_reexport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=tsjs_express_structural_anchors")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(
            report.claims[0].framework_role,
            "framework:express.route_handler"
        );
    }

    #[test]
    fn provider_resolved_tsjs_binding_support_rejects_fallback_or_mismatched_worker_facts() {
        let unit = indexed_tsjs_unit("src/repository.ts", "prisma_query", 0);
        let parser_fact = tsjs_provider_required_prisma_anchor_fact(&unit, "prisma.query.findMany");
        let role_fact = framework_role_fact_for_unit(&unit, "framework:prisma.query");

        let mut fallback_fact = tsjs_provider_binding_fact(&unit, "./db", "prisma");
        fallback_fact.certainty = FactCertainty::Structural;
        fallback_fact.origin.engine = "repogrammar-tsjs-static-worker".to_string();
        fallback_fact.origin.method = "bounded_project_model_resolver_v1".to_string();
        fallback_fact
            .assumptions
            .retain(|assumption| assumption != "provider=typescript");
        fallback_fact
            .assumptions
            .retain(|assumption| assumption != "provider_resolved=true");
        fallback_fact
            .assumptions
            .push("provider=repogrammar_static_tsjs".to_string());
        fallback_fact
            .assumptions
            .push("provider_resolved=false".to_string());
        let fallback = derive_tsjs_provider_resolved_framework_support_facts(
            std::slice::from_ref(&unit),
            std::slice::from_ref(&parser_fact),
            std::slice::from_ref(&role_fact),
            std::slice::from_ref(&fallback_fact),
        )
        .expect("fallback worker fact is valid input");
        assert!(fallback.is_empty());

        let mismatched = tsjs_provider_binding_fact(&unit, "./other", "prisma");
        let mismatch = derive_tsjs_provider_resolved_framework_support_facts(
            &[unit],
            &[parser_fact],
            &[role_fact],
            &[mismatched],
        )
        .expect("mismatched worker fact is valid input");
        assert!(mismatch.is_empty());
    }

    #[test]
    fn tsjs_package_config_facts_do_not_derive_framework_support() {
        let unit = indexed_tsjs_unit("src/a.ts", "next_app_page", 0);
        let package_fact = SemanticFact {
            kind: SemanticFactKind::ProjectConfig,
            subject: "unit:package.json#project_config".to_string(),
            target: Some(SymbolId::new("package:next").expect("valid target")),
            origin: FactOrigin {
                engine: TSJS_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: "bounded_project_inventory_v1".to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded package dependency metadata",
            )
            .expect("valid evidence"),
            assumptions: vec!["tsjs_project_config=package_json".to_string()],
        };
        let role_fact = framework_role_fact_for_unit(&unit, "framework:next.app.page");

        let derived = derive_tsjs_framework_support_facts(&[unit], &[package_fact], &[role_fact])
            .expect("derive from package config");

        assert!(derived.is_empty());
    }

    #[test]
    fn frameworkheuristic_only_tsjs_facts_do_not_derive_support() {
        let first = indexed_tsjs_unit("src/a.ts", "express_route", 0);
        let second = indexed_tsjs_unit("src/b.ts", "express_route", 1);
        let units = vec![first.clone(), second.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:express.route_handler"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_framework_support_facts(&units, &role_facts, &role_facts)
            .expect("derive from heuristic-only facts");

        assert!(derived.is_empty());
        let report = build_family_claims(&units, &role_facts);
        assert!(report.claims.is_empty());
    }

    #[test]
    fn tsjs_lookalike_anchor_target_does_not_derive_support() {
        let first = indexed_tsjs_unit("src/a.ts", "express_route", 0);
        let second = indexed_tsjs_unit("src/b.ts", "express_route", 1);
        let units = vec![first.clone(), second.clone()];
        let parser_facts = vec![
            tsjs_structural_anchor_fact(&first, "express.lookalike.get"),
            tsjs_structural_anchor_fact(&second, "express.lookalike.post"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:express.route_handler"))
            .collect::<Vec<_>>();

        let derived = derive_tsjs_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive from lookalike target");

        assert!(derived.is_empty());
    }

    fn family_claim_facts(
        parser_facts: &[SemanticFact],
        role_facts: Vec<SemanticFact>,
        derived_facts: Vec<SemanticFact>,
    ) -> Vec<SemanticFact> {
        let mut facts =
            Vec::with_capacity(parser_facts.len() + role_facts.len() + derived_facts.len());
        facts.extend(parser_facts.iter().cloned());
        facts.extend(role_facts);
        facts.extend(derived_facts);
        facts
    }

    fn parser_unknown_fact_for_unit(
        unit: &IndexedCodeUnitRecord,
        reason_code: &str,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(reason_code).expect("valid reason code")),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "3.13.0".to_string(),
                method: "cpython_ast".to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte + 1, unit.end_byte - 1).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed Python UNKNOWN for provider planning",
            )
            .expect("valid evidence"),
            assumptions: vec![
                format!("reason_code={reason_code}"),
                format!("affected_claim={affected_claim}"),
            ],
        }
    }

    fn semantic_support_facts_for_express_routes(
        workspace: &TempWorkspace,
    ) -> (Vec<String>, Vec<SemanticFact>) {
        let request = IndexingRequest::new(workspace.path().display().to_string());
        let report = discover_repository_files(request.clone(), &FilesystemFileDiscovery)
            .expect("discover files for semantic support");
        let parser = SyntaxCodeUnitParser;
        let mut facts = Vec::new();
        for file in &report.files {
            let source = FilesystemSourceStore
                .read_source(SourceReadRequest {
                    repository_root: request.repository_root.clone(),
                    path: file.path.clone(),
                    expected_content_hash: file.content_hash.clone(),
                    max_file_bytes: request.max_file_bytes,
                })
                .expect("read source for semantic support");
            let parse_report = parser
                .parse(SourceDocument {
                    path: &source.path,
                    language: language_from_discovered(file.language),
                    content_hash: source.content_hash.clone(),
                    repository_revision: RepositoryRevision::new("UNKNOWN")
                        .expect("valid revision"),
                    text: &source.text,
                })
                .expect("parse source for semantic support");
            for unit in parse_report
                .units
                .into_iter()
                .filter(|unit| unit.kind == CodeUnitKind::ExpressRoute)
            {
                facts.push(semantic_fact_for_unit_with_target(
                    unit.provenance.content_hash,
                    unit.id.as_str(),
                    &unit.provenance.path,
                    unit.range.start_byte,
                    unit.range.end_byte,
                    Some("package:express"),
                ));
            }
        }
        (
            report.files.iter().map(|file| file.path.clone()).collect(),
            facts,
        )
    }

    fn provider_export_facts_for_next_routes(
        workspace: &TempWorkspace,
    ) -> (Vec<String>, Vec<SemanticFact>) {
        let request = IndexingRequest::new(workspace.path().display().to_string());
        let report = discover_repository_files(request.clone(), &FilesystemFileDiscovery)
            .expect("discover files for next provider support");
        let parser = SyntaxCodeUnitParser;
        let parser_context =
            parser_project_context(&request, &report, &FilesystemSourceStore, &parser)
                .expect("build parser context");
        let mut facts = Vec::new();
        for file in &report.files {
            let source = FilesystemSourceStore
                .read_source(SourceReadRequest {
                    repository_root: request.repository_root.clone(),
                    path: file.path.clone(),
                    expected_content_hash: file.content_hash.clone(),
                    max_file_bytes: request.max_file_bytes,
                })
                .expect("read source for next provider support");
            let parse_report = parser
                .parse_with_context(
                    SourceDocument {
                        path: &source.path,
                        language: language_from_discovered(file.language),
                        content_hash: source.content_hash.clone(),
                        repository_revision: RepositoryRevision::new("UNKNOWN")
                            .expect("valid revision"),
                        text: &source.text,
                    },
                    &parser_context,
                )
                .expect("parse source for next provider support");
            for unit in parse_report
                .units
                .into_iter()
                .filter(|unit| unit.kind == CodeUnitKind::NextRouteHandler)
            {
                facts.push(tsjs_provider_export_fact_for_code_unit(&unit, "GET"));
            }
        }
        (
            report.files.iter().map(|file| file.path.clone()).collect(),
            facts,
        )
    }

    fn provider_binding_facts_for_prisma_queries(
        workspace: &TempWorkspace,
    ) -> (Vec<String>, Vec<SemanticFact>) {
        let request = IndexingRequest::new(workspace.path().display().to_string());
        let report = discover_repository_files(request.clone(), &FilesystemFileDiscovery)
            .expect("discover files for prisma provider support");
        let parser = SyntaxCodeUnitParser;
        let parser_context =
            parser_project_context(&request, &report, &FilesystemSourceStore, &parser)
                .expect("build parser context");
        let mut facts = Vec::new();
        for file in &report.files {
            let source = FilesystemSourceStore
                .read_source(SourceReadRequest {
                    repository_root: request.repository_root.clone(),
                    path: file.path.clone(),
                    expected_content_hash: file.content_hash.clone(),
                    max_file_bytes: request.max_file_bytes,
                })
                .expect("read source for prisma provider support");
            let parse_report = parser
                .parse_with_context(
                    SourceDocument {
                        path: &source.path,
                        language: language_from_discovered(file.language),
                        content_hash: source.content_hash.clone(),
                        repository_revision: RepositoryRevision::new("UNKNOWN")
                            .expect("valid revision"),
                        text: &source.text,
                    },
                    &parser_context,
                )
                .expect("parse source for prisma provider support");
            for unit in parse_report
                .units
                .into_iter()
                .filter(|unit| unit.kind == CodeUnitKind::PrismaQuery)
            {
                facts.push(tsjs_provider_binding_fact_for_code_unit(
                    &unit, "./db", "prisma",
                ));
            }
        }
        (
            report.files.iter().map(|file| file.path.clone()).collect(),
            facts,
        )
    }

    fn tsjs_provider_export_fact_for_code_unit(unit: &CodeUnit, export_name: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Symbol,
            subject: format!(
                "{}#resolve_export:{}-{}",
                unit.provenance.path, unit.range.start_byte, unit.range.end_byte
            ),
            target: Some(
                SymbolId::new(format!(
                    "symbol:{}#export:{export_name}",
                    unit.provenance.path
                ))
                .expect("valid target"),
            ),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api_module_resolver_v1".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                unit.id.clone(),
                unit.range.clone(),
                unit.provenance.clone(),
                "TypeScript compiler resolved TS/JS export symbol",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "provider=typescript".to_string(),
                "provider_resolved=true".to_string(),
                "environment_fingerprint=node_typescript_compiler_api_v1".to_string(),
                format!("operation_id=op-{export_name}"),
                "query_operation=resolve_export".to_string(),
                "tsconfig_hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                "package_json_hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
                format!("tsjs_export_name={export_name}"),
            ],
        }
    }

    fn tsjs_provider_binding_fact_for_code_unit(
        unit: &CodeUnit,
        import_specifier: &str,
        export_name: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Symbol,
            subject: format!(
                "{}#resolve_reexport:{}-{}",
                unit.provenance.path, unit.range.start_byte, unit.range.end_byte
            ),
            target: Some(
                SymbolId::new(format!("symbol:src/db.ts#export:{export_name}"))
                    .expect("valid target"),
            ),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api_module_resolver_v1".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                unit.id.clone(),
                unit.range.clone(),
                unit.provenance.clone(),
                "TypeScript compiler resolved TS/JS re-export symbol",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "provider=typescript".to_string(),
                "provider_resolved=true".to_string(),
                "environment_fingerprint=node_typescript_compiler_api_v1".to_string(),
                format!("operation_id=op-{export_name}"),
                "query_operation=resolve_reexport".to_string(),
                "tsconfig_hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                "package_json_hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .to_string(),
                format!("tsjs_export_name={export_name}"),
                format!("tsjs_import_specifier={import_specifier}"),
                "tsjs_import_resolution=compiler_api".to_string(),
            ],
        }
    }

    fn semantic_fact_count(state: &Path, generation_id: &str) -> u32 {
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .query_row(
                "SELECT count(*) FROM semantic_facts WHERE generation_id = ?1",
                params![generation_id],
                |row| row.get(0),
            )
            .expect("count semantic facts")
    }

    fn provider_resolved_tsjs_support_fact_count(state: &Path, generation_id: &str) -> u32 {
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .query_row(
                "SELECT count(*) FROM semantic_facts \
                 WHERE generation_id = ?1 \
                 AND origin_engine = ?2 \
                 AND origin_method = ?3 \
                 AND certainty = 'DATAFLOW_DERIVED' \
                 AND assumptions_json LIKE '%provider_resolved=true%'",
                params![
                    generation_id,
                    TSJS_DERIVED_SUPPORT_ENGINE,
                    TSJS_DERIVED_SUPPORT_METHOD
                ],
                |row| row.get(0),
            )
            .expect("count provider-resolved tsjs support facts")
    }

    fn semantic_fact_ids(state: &Path, generation_id: &str) -> Vec<(String, String)> {
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        let mut statement = connection
            .prepare(
                "SELECT fact_id, evidence_id \
                 FROM semantic_facts \
                 WHERE generation_id = ?1 \
                 ORDER BY fact_id",
            )
            .expect("prepare fact id query");
        statement
            .query_map(params![generation_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query semantic fact ids")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect semantic fact ids")
    }

    fn create_index_state(state: &Path) {
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        fs::create_dir_all(state.join("locks")).expect("create locks");
    }

    fn active_generation_id(state: &Path) -> Option<String> {
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
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
            .expect("read active generation")
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
    fn index_lock_is_acquired_before_expensive_discovery() {
        let workspace = TempWorkspace::new("indexing-lock-before-discovery");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let _guard = crate::application::repository::acquire_index_lock(
            &workspace.path().display().to_string(),
            None,
        )
        .expect("hold index lock");
        let discovery_calls = AtomicUsize::new(0);
        let discovery = CountingDiscovery {
            calls: &discovery_calls,
        };

        let error = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &discovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &store,
        )
        .expect_err("live lock must fail before discovery");

        assert!(error.to_string().contains("index lock is held"));
        assert_eq!(discovery_calls.load(Ordering::SeqCst), 0);
        assert!(!state.join("current-generation").exists());
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
        create_index_state(&state);
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

        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        let rows = connection
            .prepare(
                "SELECT path, content_hash, size_bytes, language \
                 FROM indexed_files \
                 WHERE generation_id = 'gen-000001' \
                 ORDER BY rowid",
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
    fn parser_indexing_emits_stage_progress_without_source_paths() {
        let workspace = TempWorkspace::new("indexing-progress-events");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        fs::write(workspace.path().join("b.ts"), "export const b = 2;\n").expect("write b");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let mut events = Vec::new();

        let outcome =
            index_repository_with_discovery_parser_frameworks_families_and_store_with_progress(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &detector,
                &store,
                &mut |event| events.push(event),
            )
            .expect("index with progress events");

        assert_eq!(outcome.discovered_files, 2);
        assert!(events.iter().any(|event| matches!(
            (event.stage, event.work),
            (ProgressStage::ProjectDiscovery, WorkUnits::Unknown)
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event.stage, ProgressStage::FileScanning)));
        assert!(events.iter().any(|event| matches!(
            (event.stage, event.work),
            (ProgressStage::SyntaxParsing, WorkUnits::Known(work))
                if work.completed() == 2 && work.total() == 2
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event.stage, ProgressStage::PersistenceValidation)));
        let rendered = events
            .iter()
            .map(ProgressEvent::render_plain)
            .collect::<String>();
        assert!(!rendered.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!rendered.contains("a.ts"));
        assert!(!rendered.contains("b.ts"));
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
        create_index_state(&state);
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
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        let rows = connection
            .prepare(
                "SELECT path, kind, start_byte, end_byte, content_hash \
                 FROM code_units \
                 WHERE generation_id = 'gen-000001' \
                 ORDER BY path, start_byte, end_byte, code_unit_id",
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
        let ir_nodes = connection
            .prepare(
                "SELECT node_id, code_unit_id, kind, payload_json \
                 FROM ir_nodes ORDER BY node_id",
            )
            .expect("prepare IR node query")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .expect("query IR nodes")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect IR nodes");
        assert_eq!(ir_nodes.len(), rows.len());
        assert!(ir_nodes
            .iter()
            .all(|(node_id, code_unit_id, kind, payload)| {
                node_id == &format!("ir:{code_unit_id}")
                    && !node_id.contains(workspace.path().to_string_lossy().as_ref())
                    && !node_id.contains(sentinel)
                    && !kind.trim().is_empty()
                    && payload == "{}"
            }));
        let ir_edges = connection
            .prepare(
                "SELECT from_node_id, to_node_id, label \
                 FROM ir_edges ORDER BY from_node_id, to_node_id, label",
            )
            .expect("prepare IR edge query")
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("query IR edges")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect IR edges");
        assert!(!ir_edges.is_empty());
        assert!(ir_edges.iter().all(|(from, to, label)| {
            from != to
                && from.starts_with("ir:unit:")
                && to.starts_with("ir:unit:")
                && label == "contains"
        }));
    }

    #[test]
    fn parser_semantic_facts_cannot_claim_semantic_certainty() {
        let workspace = TempWorkspace::new("indexing-parser-semantic-fact");
        fs::write(
            workspace.path().join("a.ts"),
            "import express from 'express';\n",
        )
        .expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let error = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &ParserSemanticFactParser,
            &store,
        )
        .expect_err("parser-origin SEMANTIC fact must fail");

        assert!(
            error
                .to_string()
                .contains("parser semantic facts must stay structural or unknown"),
            "unexpected error: {error}"
        );
        assert!(!state.join("current-generation").exists());
    }

    #[test]
    fn parser_graph_derived_python_facts_require_safe_origin_assumptions() {
        let content_hash =
            strict_hash("sha256:1111111111111111111111111111111111111111111111111111111111111111");
        let revision = RepositoryRevision::new("test-revision").expect("valid revision");
        let source = "from acme import models\n";
        let document = SourceDocument {
            path: "app.py",
            language: Language::Python,
            content_hash: content_hash.clone(),
            repository_revision: revision.clone(),
            text: source,
        };
        let file = DiscoveredFile {
            path: "app.py".to_string(),
            language: DiscoveredLanguage::Python,
            content_hash: content_hash.clone(),
            size_bytes: source.len() as u64,
        };
        let unit = parser_unit(
            &document,
            "unit:app.py#module:module:0-22:0",
            "app.py",
            content_hash,
            0,
            source.len(),
        );
        let fact = SemanticFact {
            kind: SemanticFactKind::ResolvedImport,
            subject: unit.id.as_str().to_string(),
            target: Some(SymbolId::new("acme.models").expect("valid target")),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "3.13.0".to_string(),
                method: "cpython_ast".to_string(),
            },
            certainty: FactCertainty::DataflowDerived,
            evidence: Evidence::new(
                unit.id.clone(),
                SourceRange::new(0, source.len()).expect("valid range"),
                Provenance::new("app.py", document.content_hash.clone(), revision)
                    .expect("valid provenance"),
                "CPython ast repo_local_python_import_graph repo_local_import_binding",
            )
            .expect("valid evidence"),
            assumptions: vec![
                "python_anchor_kind=repo_local_import_binding".to_string(),
                "provider_resolved=false".to_string(),
                "derived_from=repo_local_python_import_graph".to_string(),
            ],
        };

        validate_parser_semantic_fact(&file, source, std::slice::from_ref(&unit), &fact)
            .expect("safe graph-derived fact is valid");

        let mut unsafe_fact = fact;
        unsafe_fact.assumptions = vec![
            "python_anchor_kind=repo_local_import_binding".to_string(),
            "derived_from=repo_local_python_import_graph".to_string(),
        ];
        let error =
            validate_parser_semantic_fact(&file, source, std::slice::from_ref(&unit), &unsafe_fact)
                .expect_err("missing provider_resolved boundary must fail");
        assert!(
            error
                .to_string()
                .contains("parser semantic facts must stay structural or unknown"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn exact_python_parser_anchors_derive_family_support_without_promoting_raw_facts() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.get",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.FastAPI.post",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.delete",
            ),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 3);
        assert!(parser_facts
            .iter()
            .all(|fact| fact.certainty == FactCertainty::Structural));
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false")
                && fact.evidence.range.start_byte == 0
                && fact.evidence.range.end_byte == 20
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:fastapi.route");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn blocking_parser_unknowns_prevent_python_family_support_derivation() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.get",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.FastAPI.post",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.delete",
            ),
        ];
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "DynamicImport",
            "python_import_resolution",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 2);
        assert!(derived
            .iter()
            .all(|fact| { fact.evidence.code_unit_id.as_str() != second.id }));
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::DynamicImport));
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn fastapi_dependency_target_unknown_does_not_block_route_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.get",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.FastAPI.post",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.delete",
            ),
        ];
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "RuntimeDependencyInjection",
            "fastapi_dependency_target",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 3);
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.framework_role, "framework:fastapi.route");
        assert!(claim.unknowns.iter().any(|unknown| {
            unknown.class == UnknownClass::NonBlocking
                && unknown.reason == UnknownReasonCode::RuntimeDependencyInjection
                && unknown.affected_claim
                    == format!("{}:fastapi_dependency_target", claim.family_id)
        }));
        let records = family_storage_records(claim);
        assert!(records.variation_slots.iter().any(|slot| {
            slot.description == format!(
                "unknown|non_blocking_unknown|RuntimeDependencyInjection|{}:fastapi_dependency_target|resolve this Python subclaim before relying on it",
                claim.family_id
            )
        }));
    }

    #[test]
    fn python_import_resolution_unknown_blocks_fastapi_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(
                    unit,
                    SemanticFactKind::Symbol,
                    "fastapi.APIRouter.get",
                )
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "RuntimeDependencyInjection",
            "python_import_resolution",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 2);
        assert!(derived
            .iter()
            .all(|fact| fact.evidence.code_unit_id.as_str() != second.id));
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::RuntimeDependencyInjection));
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_monkey_patch_unknown_blocks_derived_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(
                    unit,
                    SemanticFactKind::Symbol,
                    "fastapi.APIRouter.get",
                )
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "MonkeyPatch",
            "python_call_target",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 2);
        assert!(derived
            .iter()
            .all(|fact| fact.evidence.code_unit_id.as_str() != second.id));
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::MonkeyPatch));
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_framework_identity_unknown_blocks_derived_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(
                    unit,
                    SemanticFactKind::Symbol,
                    "fastapi.APIRouter.get",
                )
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "FrameworkMagic",
            "python_framework_identity",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert_eq!(derived.len(), 2);
        assert!(derived
            .iter()
            .all(|fact| fact.evidence.code_unit_id.as_str() != second.id));
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::FrameworkMagic));
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn pytest_fixture_binding_unknown_blocks_pytest_family_support() {
        let first = indexed_python_unit("tests/test_a.py", "pytest_test", 0);
        let second = indexed_python_unit("tests/test_b.py", "pytest_test", 1);
        let third = indexed_python_unit("tests/test_c.py", "pytest_test", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Symbol, "pytest.test")
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_unknown_fact_for_unit(
            &third,
            "ConflictingFacts",
            "pytest_fixture_binding",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pytest.test"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pytest support");

        assert_eq!(derived.len(), 2);
        assert!(derived
            .iter()
            .all(|fact| fact.evidence.code_unit_id.as_str() != third.id));
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::ConflictingFacts));
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn exact_fastapi_route_method_matrix_derives_family_support() {
        let targets = [
            "fastapi.FastAPI.delete",
            "fastapi.FastAPI.get",
            "fastapi.FastAPI.head",
            "fastapi.FastAPI.options",
            "fastapi.FastAPI.patch",
            "fastapi.FastAPI.post",
            "fastapi.FastAPI.put",
            "fastapi.APIRouter.delete",
            "fastapi.APIRouter.get",
            "fastapi.APIRouter.head",
            "fastapi.APIRouter.options",
            "fastapi.APIRouter.patch",
            "fastapi.APIRouter.post",
            "fastapi.APIRouter.put",
        ];
        let units = targets
            .iter()
            .enumerate()
            .map(|(index, _target)| {
                indexed_python_unit(&format!("app/route_{index}.py"), "fastapi_route", index)
            })
            .collect::<Vec<_>>();
        let mut parser_facts = units
            .iter()
            .zip(targets)
            .map(|(unit, target)| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Symbol, target)
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_structural_anchor_fact(
            &units[0],
            SemanticFactKind::Symbol,
            "fastapi.APIRouter.api_route",
        ));
        parser_facts.push(parser_structural_anchor_fact(
            &units[1],
            SemanticFactKind::Symbol,
            "fastapi.FastAPI.websocket",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let mut derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact FastAPI support");
        derived.sort_by(|left, right| {
            left.target
                .as_ref()
                .map(SymbolId::as_str)
                .cmp(&right.target.as_ref().map(SymbolId::as_str))
        });
        let mut derived_targets = derived
            .iter()
            .map(|fact| fact.target.as_ref().map(SymbolId::as_str).expect("target"))
            .collect::<Vec<_>>();
        let mut expected_targets = targets.to_vec();
        derived_targets.sort_unstable();
        expected_targets.sort_unstable();

        assert_eq!(derived_targets, expected_targets);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
        }));
        assert!(!derived.iter().any(|fact| {
            matches!(
                fact.target.as_ref().map(SymbolId::as_str),
                Some("fastapi.APIRouter.api_route") | Some("fastapi.FastAPI.websocket")
            )
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:fastapi.route");
        assert_eq!(report.claims[0].support, targets.len());
    }

    #[test]
    fn plans_provider_identity_request_for_same_role_python_candidates() {
        let first = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let second = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let planned = plan_pyrefly_framework_identity_requests(
            &units,
            &role_facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-candidate",
        )
        .expect("plan provider requests");

        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].code_unit_kind, "fastapi_route");
        assert_eq!(planned[0].framework_role, "framework:fastapi.route");
        assert_eq!(planned[0].request.provider, PythonProviderKind::Pyrefly);
        assert_eq!(
            planned[0].request.operation,
            PythonProviderOperation::ResolveFrameworkIdentity
        );
        assert_eq!(
            planned[0]
                .request
                .candidates
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect::<Vec<_>>(),
            vec!["app/a.py", "app/b.py", "app/c.py"]
        );
        assert!(planned[0].request.candidates.iter().all(|candidate| {
            candidate.range.start_byte == 0 && candidate.range.end_byte == 20
        }));
        let family_report = build_family_claims(&units, &role_facts);
        assert!(family_report.claims.is_empty());
    }

    #[test]
    fn provider_identity_planner_skips_low_support_mixed_and_ambiguous_candidates() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let pydantic = indexed_python_unit("app/schema.py", "pydantic_model", 2);
        let units = vec![first.clone(), second.clone(), pydantic.clone()];
        let role_facts = vec![
            framework_role_fact_for_unit(&first, "framework:fastapi.route"),
            framework_role_fact_for_unit(&second, "framework:fastapi.route"),
            framework_role_fact_for_unit(&pydantic, "framework:pydantic.model"),
            framework_role_fact_for_unit(&pydantic, "framework:pytest.test"),
        ];

        let planned = plan_pyrefly_framework_identity_requests(
            &units,
            &role_facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-context",
        )
        .expect("plan provider requests");

        assert!(planned.is_empty());
        let family_report = build_family_claims(&units, &role_facts);
        assert!(family_report.claims.is_empty());
    }

    #[test]
    fn provider_identity_planner_groups_multiple_supported_python_roles() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let fourth = indexed_python_unit("app/models.py", "pydantic_model", 3);
        let fifth = indexed_python_unit("app/schemas.py", "pydantic_model", 4);
        let sixth = indexed_python_unit("app/settings.py", "pydantic_model", 5);
        let units = vec![
            first.clone(),
            second.clone(),
            third.clone(),
            fourth.clone(),
            fifth.clone(),
            sixth.clone(),
        ];
        let role_facts = units
            .iter()
            .map(|unit| {
                let role = if unit.kind == "pydantic_model" {
                    "framework:pydantic.model"
                } else {
                    "framework:fastapi.route"
                };
                framework_role_fact_for_unit(unit, role)
            })
            .collect::<Vec<_>>();

        let planned = plan_pyrefly_framework_identity_requests(
            &units,
            &role_facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-supported",
        )
        .expect("plan provider requests");

        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].code_unit_kind, "fastapi_route");
        assert_eq!(planned[1].code_unit_kind, "pydantic_model");
        assert!(planned.iter().all(|plan| {
            plan.request.provider == PythonProviderKind::Pyrefly
                && plan.request.operation == PythonProviderOperation::ResolveFrameworkIdentity
        }));
    }

    #[test]
    fn provider_identity_planner_skips_only_claim_blocking_unknown_units() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let fourth = indexed_python_unit("app/d.py", "fastapi_route", 3);
        let fifth = indexed_python_unit("app/e.py", "fastapi_route", 4);
        let sixth = indexed_python_unit("app/f.py", "fastapi_route", 5);
        let seventh = indexed_python_unit("app/g.py", "fastapi_route", 6);
        let eighth = indexed_python_unit("app/h.py", "fastapi_route", 7);
        let pytest_first = indexed_python_unit("tests/test_a.py", "pytest_test", 8);
        let pytest_second = indexed_python_unit("tests/test_b.py", "pytest_test", 9);
        let pytest_third = indexed_python_unit("tests/test_c.py", "pytest_test", 10);
        let pytest_fourth = indexed_python_unit("tests/test_d.py", "pytest_test", 11);
        let pytest_fifth = indexed_python_unit("tests/test_e.py", "pytest_test", 12);
        let units = vec![
            first.clone(),
            second.clone(),
            third.clone(),
            fourth.clone(),
            fifth.clone(),
            sixth.clone(),
            seventh.clone(),
            eighth.clone(),
            pytest_first.clone(),
            pytest_second.clone(),
            pytest_third.clone(),
            pytest_fourth.clone(),
            pytest_fifth.clone(),
        ];
        let mut facts = units
            .iter()
            .map(|unit| {
                let role = if unit.kind == "pytest_test" {
                    "framework:pytest.test"
                } else {
                    "framework:fastapi.route"
                };
                framework_role_fact_for_unit(unit, role)
            })
            .collect::<Vec<_>>();
        facts.push(parser_unknown_fact_for_unit(
            &second,
            "DynamicImport",
            "python_import_resolution",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &third,
            "RuntimeDependencyInjection",
            "python_import_resolution",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &fourth,
            "UnresolvedImport",
            "python_import_resolution",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &fifth,
            "FrameworkMagic",
            "python_framework_identity",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &sixth,
            "MonkeyPatch",
            "python_call_target",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &seventh,
            "FrameworkMagic",
            "python_call_target",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &eighth,
            "RuntimeDependencyInjection",
            "fastapi_dependency_target",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &pytest_second,
            "PytestFixtureInjection",
            "pytest_fixture_binding",
        ));
        facts.push(parser_unknown_fact_for_unit(
            &pytest_third,
            "ConflictingFacts",
            "pytest_fixture_binding",
        ));

        let planned = plan_pyrefly_framework_identity_requests(
            &units,
            &facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-unknowns",
        )
        .expect("plan provider requests");

        assert_eq!(planned.len(), 2);
        let fastapi_plan = planned
            .iter()
            .find(|plan| plan.code_unit_kind == "fastapi_route")
            .expect("fastapi plan");
        assert_eq!(
            fastapi_plan
                .request
                .candidates
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect::<Vec<_>>(),
            vec!["app/a.py", "app/f.py", "app/g.py", "app/h.py"]
        );
        let pytest_plan = planned
            .iter()
            .find(|plan| plan.code_unit_kind == "pytest_test")
            .expect("pytest plan");
        assert_eq!(
            pytest_plan
                .request
                .candidates
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect::<Vec<_>>(),
            vec!["tests/test_a.py", "tests/test_d.py", "tests/test_e.py"]
        );
    }

    #[test]
    fn active_provider_identity_planner_uses_persisted_snapshot_without_mutation() {
        let workspace = TempWorkspace::new("indexing-active-python-provider-plan");
        fs::create_dir_all(workspace.path().join("app")).expect("create app dir");
        fs::write(
            workspace.path().join("app/a.py"),
            concat!(
                "from fastapi import APIRouter\n",
                "router = APIRouter()\n",
                "@router.get('/a')\n",
                "def a():\n",
                "    return {'ok': True}\n",
            ),
        )
        .expect("write route a");
        fs::write(
            workspace.path().join("app/b.py"),
            concat!(
                "from fastapi import APIRouter\n",
                "import importlib\n",
                "router = APIRouter()\n",
                "@router.get('/b')\n",
                "def b(name: str):\n",
                "    return importlib.import_module(name)\n",
            ),
        )
        .expect("write route b");
        fs::write(
            workspace.path().join("app/c.py"),
            concat!(
                "from fastapi import APIRouter\n",
                "router = APIRouter()\n",
                "@router.get('/c')\n",
                "def c(obj, name):\n",
                "    setattr(obj, name, object())\n",
                "    return {'ok': True}\n",
            ),
        )
        .expect("write route c");
        fs::write(
            workspace.path().join("app/d.py"),
            concat!(
                "from fastapi import APIRouter\n",
                "router = APIRouter()\n",
                "@router.get('/d')\n",
                "def d():\n",
                "    return {'ok': True}\n",
            ),
        )
        .expect("write route d");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &PythonAstParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index Python routes");
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        let facts_before = store
            .list_active_semantic_facts()
            .expect("list active facts before planning")
            .facts;
        assert!(facts_before.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("DynamicImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        assert!(facts_before.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("MonkeyPatch")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_call_target")
        }));

        let report = plan_active_pyrefly_framework_identity_requests(
            &store,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-active",
        )
        .expect("plan from active snapshot");

        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.requests.len(), 1);
        assert_eq!(
            report.requests[0]
                .request
                .candidates
                .iter()
                .map(|candidate| candidate.path.as_str())
                .collect::<Vec<_>>(),
            vec!["app/a.py", "app/c.py", "app/d.py"]
        );
        let facts_after = store
            .list_active_semantic_facts()
            .expect("list active facts after planning")
            .facts;
        assert_eq!(facts_after.len(), facts_before.len());
        assert!(store
            .list_active_families()
            .expect("list active families")
            .families
            .is_empty());
    }

    #[test]
    fn provider_identity_planner_rejects_unsafe_candidate_paths_without_panic() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let safe_units = [first.clone(), second.clone(), third.clone()];
        let role_facts = safe_units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();
        let mut first = first;
        first.path = "../escape.py".to_string();
        let units = vec![first, second, third];

        let error = plan_pyrefly_framework_identity_requests(
            &units,
            &role_facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "env-sha256-unsafe",
        )
        .expect_err("unsafe path should reject provider request construction");

        assert!(error.to_string().contains("repo-relative"));
    }

    #[test]
    fn provider_identity_planner_rejects_invalid_planning_metadata() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();

        let error = plan_pyrefly_framework_identity_requests(
            &units,
            &role_facts,
            "3.12.6",
            strict_hash("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "file://env",
        )
        .expect_err("invalid metadata should bubble");

        assert!(error.to_string().contains("path-like text"));
    }

    #[test]
    fn exact_pytest_parser_anchors_derive_family_support() {
        let first = indexed_python_unit("tests/test_api.py", "pytest_test", 0);
        let second = indexed_python_unit("tests/test_api.py", "pytest_test", 1);
        let third = indexed_python_unit("tests/test_api.py", "pytest_test", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Symbol, "pytest.test")
            })
            .collect::<Vec<_>>();
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pytest.test"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pytest support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.test")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:pytest.test");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn exact_pytest_parametrize_decorator_anchors_derive_family_support() {
        let first = indexed_python_unit("tests/test_api.py", "pytest_test", 0);
        let second = indexed_python_unit("tests/test_api.py", "pytest_test", 1);
        let third = indexed_python_unit("tests/test_api.py", "pytest_test", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(
                    unit,
                    SemanticFactKind::Symbol,
                    "pytest.mark.parametrize",
                )
            })
            .collect::<Vec<_>>();
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pytest.test"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pytest parametrize support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.mark.parametrize")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:pytest.test");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn exact_pytest_fixture_parser_anchors_derive_family_support() {
        let first = indexed_python_unit("tests/conftest.py", "pytest_fixture", 0);
        let second = indexed_python_unit("tests/conftest.py", "pytest_fixture", 1);
        let third = indexed_python_unit("tests/conftest.py", "pytest_fixture", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Symbol, "pytest.fixture")
            })
            .collect::<Vec<_>>();
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pytest.fixture"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pytest fixture support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:pytest.fixture");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn pytest_fixture_edges_and_parametrize_args_do_not_derive_family_support() {
        let first = indexed_python_unit("tests/test_api.py", "pytest_test", 0);
        let second = indexed_python_unit("tests/test_api.py", "pytest_test", 1);
        let third = indexed_python_unit("tests/test_api.py", "pytest_test", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "pytest.fixture.client",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "pytest.parametrize.client",
            ),
            parser_structural_anchor_fact(&third, SemanticFactKind::Symbol, "pytest.fixture.db"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pytest.test"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pytest support");

        assert!(derived.is_empty());
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn exact_pydantic_parser_anchors_derive_family_support() {
        let first = indexed_python_unit("schemas.py", "pydantic_model", 0);
        let second = indexed_python_unit("schemas.py", "pydantic_model", 1);
        let third = indexed_python_unit("schemas.py", "pydantic_model", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Type, "pydantic.BaseModel")
            })
            .collect::<Vec<_>>();
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pydantic.model"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pydantic support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.BaseModel")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].framework_role, "framework:pydantic.model");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn exact_pydantic_settings_parser_anchors_derive_family_support() {
        for target in ["pydantic.BaseSettings", "pydantic_settings.BaseSettings"] {
            let first = indexed_python_unit("settings.py", "pydantic_model", 0);
            let second = indexed_python_unit("settings.py", "pydantic_model", 1);
            let third = indexed_python_unit("settings.py", "pydantic_model", 2);
            let units = vec![first.clone(), second.clone(), third.clone()];
            let parser_facts = units
                .iter()
                .map(|unit| parser_structural_anchor_fact(unit, SemanticFactKind::Type, target))
                .collect::<Vec<_>>();
            let role_facts = units
                .iter()
                .map(|unit| framework_role_fact_for_unit(unit, "framework:pydantic.model"))
                .collect::<Vec<_>>();

            let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
                .expect("derive exact pydantic settings support");

            assert_eq!(derived.len(), 3);
            assert!(derived.iter().all(|fact| {
                fact.certainty == FactCertainty::DataflowDerived
                    && fact.origin.engine == "repogrammar-python-derived"
                    && fact.origin.method == "bounded_ast_anchor_v1"
                    && fact.target.as_ref().map(SymbolId::as_str) == Some(target)
            }));
            let mut family_facts = role_facts;
            family_facts.extend(derived);
            let report = build_family_claims(&units, &family_facts);
            assert_eq!(report.claims.len(), 1);
            assert_eq!(report.claims[0].language, "python");
            assert_eq!(report.claims[0].framework_role, "framework:pydantic.model");
            assert_eq!(report.claims[0].support, 3);
        }
    }

    #[test]
    fn pydantic_member_anchors_do_not_derive_family_support() {
        let first = indexed_python_unit("schemas.py", "pydantic_model", 0);
        let second = indexed_python_unit("schemas.py", "pydantic_model", 1);
        let third = indexed_python_unit("schemas.py", "pydantic_model", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pydantic.model"))
            .collect::<Vec<_>>();
        let parser_facts = vec![
            parser_structural_anchor_fact(&first, SemanticFactKind::Symbol, "pydantic.field.id"),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Type,
                "pydantic.field_type.int",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "pydantic.model_config",
            ),
            parser_structural_anchor_fact(&second, SemanticFactKind::Symbol, "pydantic.Config"),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "pydantic.computed_field",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "pydantic.field_validator",
            ),
            parser_structural_anchor_fact(&third, SemanticFactKind::Symbol, "pydantic.validator"),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "pydantic.model_validator",
            ),
        ];

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pydantic support");

        assert!(derived.is_empty());
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn exact_sqlalchemy_repository_parser_anchors_derive_family_support() {
        for target in [
            "sqlalchemy.select",
            "sqlalchemy.orm.Session.execute",
            "sqlalchemy.orm.Session.commit",
            "sqlalchemy.orm.Session.rollback",
            "sqlalchemy.orm.Session.scalar",
            "sqlalchemy.orm.Session.scalars",
            "sqlalchemy.ext.asyncio.AsyncSession.execute",
            "sqlalchemy.ext.asyncio.AsyncSession.commit",
            "sqlalchemy.ext.asyncio.AsyncSession.rollback",
            "sqlalchemy.ext.asyncio.AsyncSession.scalar",
            "sqlalchemy.ext.asyncio.AsyncSession.scalars",
        ] {
            let first = indexed_python_unit("repository.py", "sqlalchemy_repository_method", 0);
            let second = indexed_python_unit("repository.py", "sqlalchemy_repository_method", 1);
            let third = indexed_python_unit("repository.py", "sqlalchemy_repository_method", 2);
            let units = vec![first.clone(), second.clone(), third.clone()];
            let parser_facts = units
                .iter()
                .map(|unit| {
                    parser_structural_anchor_fact(unit, SemanticFactKind::ResolvedCall, target)
                })
                .collect::<Vec<_>>();
            let role_facts = units
                .iter()
                .map(|unit| {
                    framework_role_fact_for_unit(unit, "framework:sqlalchemy.repository_method")
                })
                .collect::<Vec<_>>();

            let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
                .expect("derive exact SQLAlchemy repository support");

            assert_eq!(derived.len(), 3);
            assert!(derived.iter().all(|fact| {
                fact.certainty == FactCertainty::DataflowDerived
                    && fact.origin.engine == "repogrammar-python-derived"
                    && fact.origin.method == "bounded_ast_anchor_v1"
                    && fact.target.as_ref().map(SymbolId::as_str) == Some(target)
            }));
            let mut family_facts = role_facts;
            family_facts.extend(derived);
            let report = build_family_claims(&units, &family_facts);
            assert_eq!(report.claims.len(), 1);
            assert_eq!(report.claims[0].language, "python");
            assert_eq!(
                report.claims[0].framework_role,
                "framework:sqlalchemy.repository_method"
            );
            assert_eq!(report.claims[0].support, 3);
        }
    }

    #[test]
    fn exact_sqlalchemy_model_parser_anchors_derive_family_support() {
        for (kind, target) in [
            (SemanticFactKind::Type, "sqlalchemy.orm.Mapped"),
            (
                SemanticFactKind::ResolvedCall,
                "sqlalchemy.orm.mapped_column",
            ),
        ] {
            let first = indexed_python_unit("models.py", "sqlalchemy_model", 0);
            let second = indexed_python_unit("models.py", "sqlalchemy_model", 1);
            let third = indexed_python_unit("models.py", "sqlalchemy_model", 2);
            let units = vec![first.clone(), second.clone(), third.clone()];
            let parser_facts = units
                .iter()
                .map(|unit| parser_structural_anchor_fact(unit, kind.clone(), target))
                .collect::<Vec<_>>();
            let role_facts = units
                .iter()
                .map(|unit| framework_role_fact_for_unit(unit, "framework:sqlalchemy.model"))
                .collect::<Vec<_>>();

            let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
                .expect("derive exact SQLAlchemy model support");

            assert_eq!(derived.len(), 3);
            assert!(derived.iter().all(|fact| {
                fact.certainty == FactCertainty::DataflowDerived
                    && fact.origin.engine == "repogrammar-python-derived"
                    && fact.origin.method == "bounded_ast_anchor_v1"
                    && fact.target.as_ref().map(SymbolId::as_str) == Some(target)
            }));
            let mut family_facts = role_facts;
            family_facts.extend(derived);
            let report = build_family_claims(&units, &family_facts);
            assert_eq!(report.claims.len(), 1);
            assert_eq!(report.claims[0].language, "python");
            assert_eq!(
                report.claims[0].framework_role,
                "framework:sqlalchemy.model"
            );
            assert_eq!(report.claims[0].support, 3);
        }
    }

    #[test]
    fn python_parser_anchor_derivation_rejects_substrings_and_non_claim_inputs() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "myproject.fastapi.APIRouter.get",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.get_extra",
            ),
            parser_project_config_anchor_fact(&third),
        ];

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert!(derived.is_empty());
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn fastapi_context_effect_anchors_do_not_derive_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::ResolvedCall,
                "fastapi.Depends",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.dependency.get_db",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::ResolvedCall,
                "fastapi.HTTPException",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.http_exception.status_code.404",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Type,
                "fastapi.response_model.UserOut",
            ),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Type,
                "fastapi.request_body.UserIn",
            ),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.request_param.path.user_id",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.request_param.query.query",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.request_param.header.request_id",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.request_param.cookie.session_id",
            ),
        ];

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert!(derived.is_empty());
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn local_framework_like_names_do_not_derive_python_family_support() {
        for (unit_kind, role, target_kind, targets) in [
            (
                "fastapi_route",
                "framework:fastapi.route",
                SemanticFactKind::Symbol,
                ["client.get", "client.post", "client.put"],
            ),
            (
                "pydantic_model",
                "framework:pydantic.model",
                SemanticFactKind::Type,
                ["BaseModel", "app.models.BaseModel", "tests.BaseModel"],
            ),
            (
                "sqlalchemy_model",
                "framework:sqlalchemy.model",
                SemanticFactKind::Type,
                ["Base", "app.db.Base", "tests.Base"],
            ),
        ] {
            let first = indexed_python_unit("app/a.py", unit_kind, 0);
            let second = indexed_python_unit("app/b.py", unit_kind, 1);
            let third = indexed_python_unit("app/c.py", unit_kind, 2);
            let units = vec![first.clone(), second.clone(), third.clone()];
            let role_facts = units
                .iter()
                .map(|unit| framework_role_fact_for_unit(unit, role))
                .collect::<Vec<_>>();
            let parser_facts = units
                .iter()
                .zip(targets)
                .map(|(unit, target)| {
                    parser_structural_anchor_fact(unit, target_kind.clone(), target)
                })
                .collect::<Vec<_>>();

            let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
                .expect("derive exact Python support");

            assert!(
                derived.is_empty(),
                "{unit_kind} local framework-like names must not derive support"
            );
            let mut family_facts = role_facts;
            family_facts.extend(derived);
            let report = build_family_claims(&units, &family_facts);
            assert!(
                report.claims.is_empty(),
                "{unit_kind} local framework-like names must not produce a family"
            );
            assert!(report
                .unknowns
                .iter()
                .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
        }
    }

    #[test]
    fn fastapi_service_call_anchors_do_not_derive_family_support() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::ResolvedCall,
                "app.services.UserService.list_users",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::ResolvedCall,
                "app.services.UserService.create_user",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::ResolvedCall,
                "app.repositories.UserRepository.list_users",
            ),
        ];

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");

        assert!(derived.is_empty());
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn fastapi_context_effect_anchors_do_not_change_support_targets() {
        let first = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let second = indexed_python_unit("app/b.py", "fastapi_route", 1);
        let third = indexed_python_unit("app/c.py", "fastapi_route", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:fastapi.route"))
            .collect::<Vec<_>>();
        let parser_facts = vec![
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.get",
            ),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::ResolvedCall,
                "fastapi.Depends",
            ),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Symbol,
                "fastapi.dependency.get_db",
            ),
            parser_structural_anchor_fact(
                &first,
                SemanticFactKind::Type,
                "fastapi.response_model.UserList",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.FastAPI.post",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::ResolvedCall,
                "fastapi.HTTPException",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Symbol,
                "fastapi.http_exception.status_code.400",
            ),
            parser_structural_anchor_fact(
                &second,
                SemanticFactKind::Type,
                "fastapi.response_model.UserOut",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.APIRouter.delete",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::ResolvedCall,
                "fastapi.Depends",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Symbol,
                "fastapi.dependency.delete_db",
            ),
            parser_structural_anchor_fact(
                &third,
                SemanticFactKind::Type,
                "fastapi.response_model.DeleteResult",
            ),
        ];

        let mut derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact Python support");
        derived.sort_by(|left, right| {
            left.target
                .as_ref()
                .map(SymbolId::as_str)
                .cmp(&right.target.as_ref().map(SymbolId::as_str))
        });
        let targets = derived
            .iter()
            .map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec![
                Some("fastapi.APIRouter.delete"),
                Some("fastapi.APIRouter.get"),
                Some("fastapi.FastAPI.post")
            ]
        );
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "repogrammar-python-derived"
                && fact.origin.method == "bounded_ast_anchor_v1"
        }));
    }

    #[test]
    fn python_parser_anchor_derivation_requires_single_framework_role() {
        let unit = indexed_python_unit("app/a.py", "fastapi_route", 0);
        let parser_facts = vec![parser_structural_anchor_fact(
            &unit,
            SemanticFactKind::Symbol,
            "fastapi.APIRouter.get",
        )];

        let no_role =
            derive_python_framework_support_facts(std::slice::from_ref(&unit), &parser_facts, &[])
                .expect("derive exact Python support");
        assert!(no_role.is_empty());

        let multi_role = vec![
            framework_role_fact_for_unit(&unit, "framework:fastapi.route"),
            framework_role_fact_for_unit(&unit, "framework:pytest.test"),
        ];
        let derived = derive_python_framework_support_facts(
            std::slice::from_ref(&unit),
            &parser_facts,
            &multi_role,
        )
        .expect("derive exact Python support");
        assert!(derived.is_empty());
    }

    #[test]
    fn syntax_framework_role_facts_are_stored_but_not_family_claim_inputs() {
        let workspace = TempWorkspace::new("indexing-framework-role-facts");
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
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;

        let outcome = index_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &detector,
            &store,
        )
        .expect("index syntax framework role facts");

        assert_eq!(outcome.discovered_files, 2);
        assert_eq!(outcome.indexed_units, 7);
        assert_eq!(outcome.semantic_facts, 8);
        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Deferred);
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));

        let facts = store
            .list_active_semantic_facts()
            .expect("list semantic facts");
        assert_eq!(facts.generation_id, "gen-000001");
        assert_eq!(facts.facts.len(), 8);
        let role_facts = facts
            .facts
            .iter()
            .filter(|fact| fact.kind == "FRAMEWORK_ROLE")
            .collect::<Vec<_>>();
        let unknown_facts = facts
            .facts
            .iter()
            .filter(|fact| fact.kind == "UNKNOWN")
            .collect::<Vec<_>>();
        assert_eq!(role_facts.len(), 5);
        assert_eq!(unknown_facts.len(), 3);
        assert!(role_facts.iter().all(|fact| {
            fact.kind == "FRAMEWORK_ROLE"
                && fact.certainty == "FRAMEWORK_HEURISTIC"
                && fact.origin_engine == "repogrammar-frameworks"
                && fact.origin_method == "syntax_code_unit_kind"
                && fact.path != sentinel
                && !fact
                    .path
                    .contains(workspace.path().to_string_lossy().as_ref())
                && !fact.note.contains(sentinel)
                && !fact
                    .note
                    .contains(workspace.path().to_string_lossy().as_ref())
                && fact.content_hash.as_str().starts_with("sha256:")
        }));
        assert!(unknown_facts.iter().all(|fact| {
            fact.certainty == "UNKNOWN"
                && fact.origin_engine == "repogrammar-tsjs-syntax"
                && fact.origin_method == "exact_anchor_v1"
                && fact.path != sentinel
                && !fact
                    .path
                    .contains(workspace.path().to_string_lossy().as_ref())
                && !fact.note.contains(sentinel)
                && !fact
                    .note
                    .contains(workspace.path().to_string_lossy().as_ref())
                && fact.content_hash.as_str().starts_with("sha256:")
        }));
        let targets = role_facts
            .iter()
            .map(|fact| fact.target.as_deref().expect("framework role target"))
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            [
                "framework:react.component",
                "framework:react.hook",
                "framework:express.route_handler",
                "framework:jest_vitest.suite",
                "framework:jest_vitest.test"
            ]
        );
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        let family_bound_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence \
                 WHERE generation_id = 'gen-000001' AND family_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("count family-bound evidence");
        let families: u32 = connection
            .query_row(
                "SELECT count(*) FROM families WHERE generation_id = 'gen-000001'",
                [],
                |row| row.get(0),
            )
            .expect("count families");
        let family_members: u32 = connection
            .query_row(
                "SELECT count(*) FROM family_members WHERE generation_id = 'gen-000001'",
                [],
                |row| row.get(0),
            )
            .expect("count family members");
        assert_eq!(family_bound_evidence, 0);
        assert_eq!(families, 0);
        assert_eq!(family_members, 0);
        let workspace_path = workspace.path().to_string_lossy().to_string();
        let debug = format!("{:?}", facts.facts);
        for forbidden in [
            sentinel,
            workspace_path.as_str(),
            "app.get",
            "res.json",
            "return <",
            "describe(",
            "it(",
        ] {
            assert!(
                !debug.contains(forbidden),
                "framework facts leaked forbidden text {forbidden}"
            );
        }

        let readiness = assess_semantic_fact_readiness(
            SemanticFactReadinessRequest {
                repository_root: workspace.path().display().to_string(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
        )
        .expect("assess framework fact readiness");
        assert_eq!(readiness.facts.len(), 8);
        for fact in readiness.facts {
            let ClaimInputReadiness::Blocked { unknown } = fact.readiness else {
                panic!("framework heuristic facts must not become family claim input");
            };
            assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
        }
    }

    #[test]
    fn semantic_supported_framework_groups_are_stored_as_family_records() {
        let workspace = TempWorkspace::new("indexing-family-builder");
        fs::write(
            workspace.path().join("users.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n",
        )
        .expect("write users route");
        fs::write(
            workspace.path().join("accounts.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/accounts', (req, res) => { res.json([]); });\n",
        )
        .expect("write accounts route");
        fs::write(
            workspace.path().join("orders.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/orders', (req, res) => { res.json([]); });\n",
        )
        .expect("write orders route");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let (expected_files, facts) = semantic_support_facts_for_express_routes(&workspace);
        assert_eq!(facts.len(), 3);
        let worker = StaticSemanticWorker {
            expected_files,
            result: Ok(facts),
        };

        let outcome =
            index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &detector,
                &worker,
                &store,
            )
            .expect("index semantic-supported family");

        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Complete);
        assert_eq!(
            outcome.semantic_facts as u32,
            semantic_fact_count(&state, "gen-000001")
        );
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.generation_id, "gen-000001");
        assert_eq!(families.families.len(), 1);
        assert_eq!(families.families[0].classification, "DOMINANT_PATTERN");
        let family = store
            .show_family(&families.families[0].family_id)
            .expect("show family")
            .expect("family exists");
        assert_eq!(family.members.len(), 3);
        assert_eq!(family.evidence.len(), 3);
        assert!(family
            .members
            .iter()
            .all(|member| member.role == "framework:express.route_handler"));
        let debug = format!("{family:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("res.json"));
    }

    #[test]
    fn provider_resolved_next_export_support_is_recorded_after_worker() {
        let workspace = TempWorkspace::new("indexing-provider-next-export-support");
        for route in ["users", "accounts", "orders"] {
            let path = workspace.path().join(format!("app/{route}/route.ts"));
            fs::create_dir_all(path.parent().expect("route parent")).expect("create route dir");
            fs::write(
                path,
                "export async function GET() { return Response.json([]); }\n",
            )
            .expect("write next route");
        }
        fs::write(
            workspace.path().join("package.json"),
            r#"{"dependencies":{"next":"latest"}}"#,
        )
        .expect("write package json");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let (expected_files, facts) = provider_export_facts_for_next_routes(&workspace);
        assert_eq!(facts.len(), 3);
        let worker = StaticSemanticWorker {
            expected_files,
            result: Ok(facts),
        };

        let outcome =
            index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &detector,
                &worker,
                &store,
            )
            .expect("index provider-resolved next support");

        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Complete);
        assert_eq!(
            provider_resolved_tsjs_support_fact_count(&state, "gen-000001"),
            3
        );
        assert_eq!(
            outcome.semantic_facts as u32,
            semantic_fact_count(&state, "gen-000001")
        );
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.families.len(), 1);
        let family = store
            .show_family(&families.families[0].family_id)
            .expect("show family")
            .expect("family exists");
        assert_eq!(family.members.len(), 3);
        assert!(family
            .members
            .iter()
            .all(|member| member.role == "framework:next.route.handler"));
    }

    #[test]
    fn provider_resolved_prisma_binding_support_is_recorded_after_worker() {
        let workspace = TempWorkspace::new("indexing-provider-prisma-binding-support");
        fs::write(
            workspace.path().join("db.ts"),
            "export const prisma = {};\n",
        )
        .expect("write shared client");
        for name in ["users", "accounts", "orders"] {
            let path = workspace.path().join(format!("{name}.ts"));
            fs::write(
                path,
                "import { prisma } from './db';\n\
                 export function list() { return prisma.user.findMany(); }\n",
            )
            .expect("write repository");
        }
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let (expected_files, facts) = provider_binding_facts_for_prisma_queries(&workspace);
        assert_eq!(facts.len(), 3);
        let worker = StaticSemanticWorker {
            expected_files,
            result: Ok(facts),
        };

        let outcome =
            index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &detector,
                &worker,
                &store,
            )
            .expect("index provider-resolved prisma support");

        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Complete);
        assert_eq!(
            provider_resolved_tsjs_support_fact_count(&state, "gen-000001"),
            3
        );
        assert_eq!(
            outcome.semantic_facts as u32,
            semantic_fact_count(&state, "gen-000001")
        );
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.families.len(), 1);
        let family = store
            .show_family(&families.families[0].family_id)
            .expect("show family")
            .expect("family exists");
        assert_eq!(family.members.len(), 3);
        assert!(family
            .members
            .iter()
            .all(|member| member.role == "framework:prisma.query"));
        let debug = format!("{family:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("prisma.user.findMany"));
    }

    #[test]
    fn reported_semantic_facts_count_includes_derived_tsjs_support_facts() {
        // A resolved Express server produces exact TS/JS anchors that are promoted
        // to bounded TS/JS-derived support facts and recorded in the generation.
        // Regression guard: `IndexingOutcome::semantic_facts` must count those
        // derived facts, so the reported total equals what was actually stored.
        let workspace = TempWorkspace::new("indexing-derived-tsjs-count");
        fs::write(
            workspace.path().join("server.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n\
             app.post('/users', (req, res) => { res.json({}); });\n\
             app.delete('/users/:id', (req, res) => { res.json({}); });\n",
        )
        .expect("write resolved express server");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index resolved express server");

        // No semantic worker ran, so a family here can only come from the
        // TS/JS-derived support facts the count must include.
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.families.len(), 1);
        assert_eq!(families.families[0].classification, "DOMINANT_PATTERN");

        let stored = semantic_fact_count(&state, "gen-000001");
        assert!(stored >= 2, "derived TS/JS support facts must be recorded");
        assert_eq!(
            outcome.semantic_facts as u32, stored,
            "reported semantic_facts must include TS/JS-derived support facts, not drop them"
        );
    }

    #[test]
    fn optional_semantic_worker_records_valid_same_generation_facts() {
        let workspace = TempWorkspace::new("indexing-semantic-worker");
        let source = "import express from 'express';\n";
        fs::write(workspace.path().join("a.ts"), source).expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let discovered = discover_repository_files(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("discover files");
        let fact = semantic_fact_for_a_ts(discovered.files[0].content_hash.clone());
        let worker = StaticSemanticWorker {
            expected_files: vec!["a.ts".to_string()],
            result: Ok(vec![fact]),
        };

        let outcome = index_repository_with_discovery_parser_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SingleUnitParser,
            &worker,
            &store,
        )
        .expect("index semantic worker facts");

        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 1);
        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Complete);
        assert_eq!(outcome.warnings, Vec::<String>::new());
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(semantic_fact_count(&state, "gen-000001"), 1);
    }

    #[test]
    fn optional_semantic_worker_request_includes_tsjs_resolver_operations() {
        let workspace = TempWorkspace::new("indexing-tsjs-worker-operations");
        fs::create_dir_all(workspace.path().join("src/app")).expect("create source dirs");
        fs::create_dir_all(workspace.path().join("app/users")).expect("create next dirs");
        let source = "import service from '@app/service';\nexport * from './barrel';\n";
        let repository = "import { prisma } from './db';\nprisma.user.findMany();\n";
        let target = "export const service = true;\n";
        let barrel = "export const value = true;\n";
        let db = "export const prisma = {};\n";
        let next_route = "export async function GET() { return Response.json([]); }\n";
        let config = r#"{"compilerOptions":{"baseUrl":"src","paths":{"@app/*":["app/*"]}}}"#;
        let package_json =
            r#"{"dependencies":{"express":"latest","next":"latest","@prisma/client":"latest"}}"#;
        fs::write(workspace.path().join("src/route.ts"), source).expect("write route");
        fs::write(workspace.path().join("src/repository.ts"), repository)
            .expect("write repository");
        fs::write(workspace.path().join("src/app/service.ts"), target).expect("write target");
        fs::write(workspace.path().join("src/barrel.ts"), barrel).expect("write barrel");
        fs::write(workspace.path().join("src/db.ts"), db).expect("write db");
        fs::write(workspace.path().join("app/users/route.ts"), next_route)
            .expect("write next route");
        fs::write(workspace.path().join("tsconfig.json"), config).expect("write tsconfig");
        fs::write(workspace.path().join("package.json"), package_json).expect("write package");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let discovered = discover_repository_files(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("discover files");
        let hash_for = |path: &str| {
            discovered
                .files
                .iter()
                .find(|file| file.path == path)
                .expect("discovered file")
                .content_hash
                .as_str()
                .to_string()
        };
        let worker = RecordingSemanticWorker::new();

        index_repository_with_discovery_parser_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &worker,
            &store,
        )
        .expect("index with recording semantic worker");

        let requests = worker.requests.lock().expect("recorded requests");
        assert_eq!(requests.len(), 1);
        let operations = &requests[0].operations;
        assert_eq!(operations.len(), 5);
        let find_operation =
            |kind: SemanticWorkerOperationKind, path: &str, literal_specifier: &str| {
                operations
                    .iter()
                    .find(|operation| {
                        operation.operation == kind
                            && operation.path == path
                            && operation.literal_specifier == literal_specifier
                    })
                    .expect("semantic worker operation")
            };
        let operation = find_operation(
            SemanticWorkerOperationKind::ResolveModuleSpecifier,
            "src/route.ts",
            "@app/service",
        );
        assert_eq!(
            operation.operation,
            SemanticWorkerOperationKind::ResolveModuleSpecifier
        );
        assert_eq!(operation.path, "src/route.ts");
        assert_eq!(operation.content_hash, hash_for("src/route.ts"));
        assert_eq!(operation.literal_specifier, "@app/service");
        assert_eq!(operation.project_config_hash, hash_for("tsconfig.json"));
        assert_eq!(operation.package_json_hash, hash_for("package.json"));
        assert_eq!(operation.max_files, discovered.files.len());
        assert_eq!(operation.max_bytes, DEFAULT_MAX_FILE_BYTES);
        assert!(operation.operation_id.starts_with("tsjs-op-"));
        assert!(operation
            .code_unit_id
            .starts_with("unit:src/route.ts#module:"));
        let import_operation = find_operation(
            SemanticWorkerOperationKind::ResolveModuleSpecifier,
            "src/repository.ts",
            "./db",
        );
        assert_eq!(import_operation.content_hash, hash_for("src/repository.ts"));
        assert!(import_operation
            .code_unit_id
            .starts_with("unit:src/repository.ts#module:"));
        let reexport_operation = find_operation(
            SemanticWorkerOperationKind::ResolveReexport,
            "src/route.ts",
            "./barrel#*",
        );
        assert_eq!(
            reexport_operation.operation,
            SemanticWorkerOperationKind::ResolveReexport
        );
        assert_eq!(reexport_operation.path, "src/route.ts");
        assert_eq!(reexport_operation.content_hash, hash_for("src/route.ts"));
        assert_eq!(reexport_operation.literal_specifier, "./barrel#*");
        assert_eq!(
            reexport_operation.project_config_hash,
            hash_for("tsconfig.json")
        );
        assert_eq!(
            reexport_operation.package_json_hash,
            hash_for("package.json")
        );
        assert!(reexport_operation
            .code_unit_id
            .starts_with("unit:src/route.ts#module:"));
        let export_operation = find_operation(
            SemanticWorkerOperationKind::ResolveExport,
            "app/users/route.ts",
            "GET",
        );
        assert_eq!(
            export_operation.operation,
            SemanticWorkerOperationKind::ResolveExport
        );
        assert_eq!(export_operation.path, "app/users/route.ts");
        assert_eq!(
            export_operation.content_hash,
            hash_for("app/users/route.ts")
        );
        assert_eq!(export_operation.literal_specifier, "GET");
        assert_eq!(
            export_operation.project_config_hash,
            hash_for("tsconfig.json")
        );
        assert_eq!(export_operation.package_json_hash, hash_for("package.json"));
        assert!(export_operation
            .code_unit_id
            .starts_with("unit:app/users/route.ts#next_route_handler:"));
        let binding_operation = find_operation(
            SemanticWorkerOperationKind::ResolveReexport,
            "src/repository.ts",
            "./db#prisma",
        );
        assert_eq!(
            binding_operation.content_hash,
            hash_for("src/repository.ts")
        );
        assert_eq!(
            binding_operation.project_config_hash,
            hash_for("tsconfig.json")
        );
        assert_eq!(
            binding_operation.package_json_hash,
            hash_for("package.json")
        );
        assert!(binding_operation
            .code_unit_id
            .starts_with("unit:src/repository.ts#prisma_query:"));
    }

    #[test]
    fn framework_and_worker_semantic_fact_ids_do_not_collide() {
        let workspace = TempWorkspace::new("indexing-framework-worker-fact-ids");
        let source = "app.get('/users', (req, res) => res.json([]));\n";
        fs::write(workspace.path().join("a.ts"), source).expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let discovered = discover_repository_files(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("discover files");
        let fact = semantic_fact_for_unit(
            discovered.files[0].content_hash.clone(),
            "unit:a.ts#express:0-all",
            "a.ts",
            source.len(),
        );
        let worker = StaticSemanticWorker {
            expected_files: vec!["a.ts".to_string()],
            result: Ok(vec![fact]),
        };
        let detector = SyntaxFrameworkRoleDetector;

        let outcome = index_repository_with_discovery_parser_frameworks_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &ExpressRouteParser,
            &detector,
            &worker,
            &store,
        )
        .expect("index framework and worker facts");

        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 2);
        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Complete);
        assert_eq!(
            semantic_fact_ids(&state, "gen-000001"),
            vec![
                (
                    "semantic-fact:000000".to_string(),
                    "semantic-evidence:000000".to_string()
                ),
                (
                    "semantic-fact:000001".to_string(),
                    "semantic-evidence:000001".to_string()
                )
            ]
        );
    }

    #[test]
    fn rust_provider_project_config_facts_are_recorded_in_active_generation() {
        let workspace = TempWorkspace::new("indexing-rust-provider-cargo-metadata");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write manifest");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let provider = FakeRustProjectModelProvider;

        let outcome =
            index_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &RepoGrammarSourceParser::default(),
                (&detector, &provider),
                &store,
                &mut |_event| {},
            )
            .expect("index with Rust project-model provider");

        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert!(outcome.semantic_facts >= 1);
        assert!(outcome.warnings.is_empty());
        let facts = store
            .list_active_semantic_facts()
            .expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.certainty == "SEMANTIC"
                && fact.origin_engine == "cargo_metadata"
                && fact.origin_method == "cargo_metadata_no_deps_v1"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("cargo.workspace")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "build_scripts_executed=false")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "proc_macros_executed=false")
        }));
    }

    #[test]
    fn optional_semantic_worker_fallback_keeps_syntax_only_generation_sanitized() {
        let workspace = TempWorkspace::new("indexing-semantic-worker-fallback");
        fs::write(
            workspace.path().join("a.ts"),
            "import express from 'express';\n",
        )
        .expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let worker = StaticSemanticWorker {
            expected_files: vec!["a.ts".to_string()],
            result: Err(SemanticWorkerError::Unavailable(
                "raw worker path /tmp/secret.ts must not leak".to_string(),
            )),
        };

        let outcome = index_repository_with_discovery_parser_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SingleUnitParser,
            &worker,
            &store,
        )
        .expect("fallback should still activate syntax-only generation");

        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(
            outcome.semantic_worker,
            SemanticWorkerRunStatus::FallbackUnavailable
        );
        assert_eq!(
            outcome.warnings,
            vec!["semantic worker fallback: unavailable".to_string()]
        );
        assert!(!outcome.warnings.iter().any(|warning| {
            warning.contains("/tmp/secret")
                || warning.contains(workspace.path().to_string_lossy().as_ref())
        }));
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(semantic_fact_count(&state, "gen-000001"), 0);
    }

    #[test]
    fn optional_semantic_worker_stale_hash_fact_preserves_previous_active_generation() {
        let workspace = TempWorkspace::new("indexing-semantic-worker-stale-hash");
        fs::write(
            workspace.path().join("a.ts"),
            "import express from 'express';\n",
        )
        .expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let first = store.prepare_next_generation().expect("prepare first");
        store.activate_generation(&first).expect("activate first");
        let worker = StaticSemanticWorker {
            expected_files: vec!["a.ts".to_string()],
            result: Ok(vec![semantic_fact_for_a_ts(
                ContentHash::new(
                    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                )
                .expect("valid stale hash"),
            )]),
        };

        let error = index_repository_with_discovery_parser_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SingleUnitParser,
            &worker,
            &store,
        )
        .expect_err("stale semantic evidence must abort indexing");

        assert!(error.to_string().contains("semantic fact content hash"));
        assert_eq!(active_generation_id(&state).as_deref(), Some("gen-000001"));
        assert_eq!(semantic_fact_count(&state, "gen-000002"), 0);
    }

    #[test]
    fn optional_semantic_worker_is_deferred_when_no_files_are_discovered() {
        let workspace = TempWorkspace::new("indexing-semantic-worker-empty");
        fs::write(workspace.path().join("README.md"), "not a TS/JS source\n")
            .expect("write ignored source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let outcome = index_repository_with_discovery_parser_semantic_worker_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SingleUnitParser,
            &PanickingSemanticWorker,
            &store,
        )
        .expect("empty discovery still activates syntax-only generation");

        assert_eq!(outcome.discovered_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Deferred);
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
    }

    #[test]
    fn parser_context_receives_deterministic_python_inventory() {
        let workspace = TempWorkspace::new("indexing-python-parser-context");
        fs::create_dir_all(workspace.path().join("src/acme/services")).expect("create package");
        fs::create_dir_all(workspace.path().join("tests")).expect("create tests");
        fs::write(
            workspace.path().join("src/acme/services/users.py"),
            "def list_users():\n    return []\n",
        )
        .expect("write users module");
        fs::write(
            workspace.path().join("src/acme/api.py"),
            "from acme.services import users\n",
        )
        .expect("write api module");
        fs::write(workspace.path().join("src/acme/__init__.py"), "").expect("write init");
        fs::write(
            workspace.path().join("pyproject.toml"),
            r#"
[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]
"#,
        )
        .expect("write pyproject");
        fs::write(
            workspace.path().join("tests/conftest.py"),
            "import pytest\n\n@pytest.fixture\ndef client():\n    return object()\n",
        )
        .expect("write conftest");
        fs::write(workspace.path().join("README.md"), "not source\n").expect("write readme");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RecordingContextParser::new();

        let outcome = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &store,
        )
        .expect("index with recording parser");

        assert_eq!(outcome.discovered_files, 5);
        let contexts = parser.contexts.lock().expect("recorded contexts");
        assert_eq!(contexts.len(), 5);
        let expected = vec![
            "src/acme/__init__.py".to_string(),
            "src/acme/api.py".to_string(),
            "src/acme/services/users.py".to_string(),
            "tests/conftest.py".to_string(),
        ];
        for context in contexts.iter() {
            assert_eq!(context.python_module_paths, expected);
            assert_eq!(
                context
                    .python_module_files
                    .iter()
                    .map(|file| file.path.as_str())
                    .collect::<Vec<_>>(),
                expected.iter().map(String::as_str).collect::<Vec<_>>()
            );
            assert_eq!(context.python_source_roots, ["src", "src/lib", "tests"]);
            assert_eq!(context.python_conftest_files.len(), 1);
            assert_eq!(context.python_conftest_files[0].path, "tests/conftest.py");
            assert!(context.python_conftest_files[0]
                .text
                .contains("@pytest.fixture"));
            assert!(!context.python_conftest_files[0]
                .path
                .contains(workspace.path().to_string_lossy().as_ref()));
            assert!(context
                .python_module_paths
                .iter()
                .all(|path| path.ends_with(".py")
                    && !Path::new(path).is_absolute()
                    && !path.contains("..")
                    && !path.contains(workspace.path().to_string_lossy().as_ref())));
            assert!(context.python_module_files.iter().all(|file| {
                file.path.ends_with(".py")
                    && !Path::new(&file.path).is_absolute()
                    && !file.path.contains("..")
                    && !file
                        .path
                        .contains(workspace.path().to_string_lossy().as_ref())
                    && !file
                        .text
                        .contains(workspace.path().to_string_lossy().as_ref())
            }));
        }
    }

    #[test]
    fn parser_context_receives_deterministic_tsjs_inventory_aliases_and_test_runner_context() {
        let workspace = TempWorkspace::new("indexing-tsjs-parser-context");
        fs::create_dir_all(workspace.path().join("src/lib")).expect("create src/lib");
        fs::create_dir_all(workspace.path().join("tests")).expect("create tests");
        fs::write(
            workspace.path().join("package.json"),
            r#"{"devDependencies":{"vitest":"^1.0.0"}}"#,
        )
        .expect("write package");
        fs::write(
            workspace.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":"src","rootDirs":["src","generated","../outside","src/*"],"paths":{"@app":["app.ts"],"@lib/*":["lib/*"],"@unsafe/*":["../outside/*"]}}}"#,
        )
        .expect("write tsconfig");
        fs::write(
            workspace.path().join("jsconfig.json"),
            r#"{"compilerOptions":{"paths":{"@shared/*":["src/shared/*"],"@test/*":["tests/*"]}}}"#,
        )
        .expect("write jsconfig");
        fs::write(
            workspace.path().join("src/app.ts"),
            "export const app = 1;\n",
        )
        .expect("write app");
        fs::write(
            workspace.path().join("src/lib/client.js"),
            "export const client = 1;\n",
        )
        .expect("write client");
        fs::write(
            workspace.path().join("tests/app.test.ts"),
            "describe('app', () => { it('works', () => {}); });\n",
        )
        .expect("write test");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RecordingContextParser::new();

        let outcome = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &store,
        )
        .expect("index with TS/JS recording parser");

        assert_eq!(outcome.discovered_files, 6);
        let contexts = parser.contexts.lock().expect("recorded contexts");
        assert_eq!(contexts.len(), 6);
        for context in contexts.iter() {
            assert_eq!(
                context.tsjs_module_paths,
                ["src/app.ts", "src/lib/client.js", "tests/app.test.ts"]
            );
            assert_eq!(context.tsjs_path_aliases.len(), 4);
            assert_eq!(context.tsjs_path_aliases[0].alias_pattern, "@app");
            assert_eq!(context.tsjs_path_aliases[0].target_patterns, ["src/app.ts"]);
            assert_eq!(context.tsjs_path_aliases[1].alias_pattern, "@lib/*");
            assert_eq!(context.tsjs_path_aliases[1].target_patterns, ["src/lib/*"]);
            assert_eq!(context.tsjs_path_aliases[2].alias_pattern, "@shared/*");
            assert_eq!(
                context.tsjs_path_aliases[2].target_patterns,
                ["src/shared/*"]
            );
            assert_eq!(context.tsjs_path_aliases[3].alias_pattern, "@test/*");
            assert_eq!(context.tsjs_path_aliases[3].target_patterns, ["tests/*"]);
            assert_eq!(context.tsjs_root_dirs, ["generated", "src"]);
            assert!(context.tsjs_has_test_runner_context);
            assert!(context.tsjs_module_paths.iter().all(|path| {
                (path.ends_with(".ts") || path.ends_with(".js"))
                    && !Path::new(path).is_absolute()
                    && !path.contains("..")
                    && !path.contains(workspace.path().to_string_lossy().as_ref())
            }));
        }
    }

    #[test]
    fn python_source_root_context_uses_only_safe_project_config_facts() {
        let content_hash =
            strict_hash("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let revision = RepositoryRevision::new("UNKNOWN").expect("valid revision");
        let document = SourceDocument {
            path: "pyproject.toml",
            language: Language::PythonConfig,
            content_hash,
            repository_revision: revision,
            text: "[tool.pyright]\ninclude = ['src']\n",
        };
        let unit = parser_unit(
            &document,
            "unit:pyproject.toml#project_config:0-all",
            document.path,
            document.content_hash.clone(),
            0,
            document.text.len(),
        );
        let safe = project_config_source_root_fact(&document, &unit, "src");
        let mut unsafe_root = project_config_source_root_fact(&document, &unit, "tests");
        unsafe_root.assumptions = vec![
            "python_config_field=source_roots".to_string(),
            "python_config_source_root=../secret".to_string(),
            "parsed_with=tomllib".to_string(),
        ];
        let mut wrong_field = project_config_source_root_fact(&document, &unit, "src/lib");
        wrong_field.assumptions = vec![
            "python_config_field=tool_sections".to_string(),
            "python_config_source_root=src/lib".to_string(),
            "parsed_with=tomllib".to_string(),
        ];

        assert_eq!(
            extract_python_source_roots_from_project_config_facts(&[
                unsafe_root,
                wrong_field,
                safe
            ]),
            ["src"]
        );
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
        create_index_state(&state);
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
                let unit = parser_unit(
                    &document,
                    "unit:src/a.ts#module:0-1",
                    document.path,
                    document.content_hash.clone(),
                    0,
                    1,
                );
                let ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
                Ok(ParseReport {
                    units: vec![unit],
                    ir_nodes: vec![ir_node],
                    ir_edges: Vec::new(),
                    semantic_facts: Vec::new(),
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
        create_index_state(&state);
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
        create_index_state(&state);
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
        assert_eq!(active_generation_id(&state).as_deref(), Some("gen-000001"));
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
                    ir_nodes: Vec::new(),
                    ir_edges: Vec::new(),
                    semantic_facts: Vec::new(),
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
            create_index_state(&state);
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
            assert_eq!(active_generation_id(&state).as_deref(), Some("gen-000001"));
        }
    }

    #[test]
    fn malformed_parser_ir_aborts_without_activating_new_generation() {
        #[derive(Clone, Copy)]
        enum BadIrMode {
            MissingNode,
            DifferentPath,
            UnknownEdgeTarget,
        }

        struct BadIrParser(BadIrMode);

        impl SourceParser for BadIrParser {
            fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
                let unit = parser_unit(
                    &document,
                    "unit:src/a.ts#module:0-1",
                    document.path,
                    document.content_hash.clone(),
                    0,
                    1,
                );
                let mut ir_node = IrNode::from_code_unit(&unit).map_err(ParseError::Internal)?;
                match self.0 {
                    BadIrMode::MissingNode => Ok(ParseReport {
                        units: vec![unit],
                        ir_nodes: Vec::new(),
                        ir_edges: Vec::new(),
                        semantic_facts: Vec::new(),
                        diagnostics: Vec::new(),
                    }),
                    BadIrMode::DifferentPath => {
                        ir_node.provenance = Provenance::new(
                            "src/other.ts",
                            document.content_hash.clone(),
                            document.repository_revision.clone(),
                        )
                        .map_err(ParseError::Internal)?;
                        Ok(ParseReport {
                            units: vec![unit],
                            ir_nodes: vec![ir_node],
                            ir_edges: Vec::new(),
                            semantic_facts: Vec::new(),
                            diagnostics: Vec::new(),
                        })
                    }
                    BadIrMode::UnknownEdgeTarget => {
                        let edge = IrEdge {
                            from_node_id: ir_node.id.clone(),
                            to_node_id: IrNodeId::new("ir:unit:src/a.ts#missing")
                                .map_err(ParseError::Internal)?,
                            label: IrEdgeLabel::Contains,
                        };
                        Ok(ParseReport {
                            units: vec![unit],
                            ir_nodes: vec![ir_node],
                            ir_edges: vec![edge],
                            semantic_facts: Vec::new(),
                            diagnostics: Vec::new(),
                        })
                    }
                }
            }
        }

        for mode in [
            BadIrMode::MissingNode,
            BadIrMode::DifferentPath,
            BadIrMode::UnknownEdgeTarget,
        ] {
            let workspace = TempWorkspace::new("indexing-bad-parser-ir");
            fs::write(workspace.path().join("a.ts"), "x").expect("write source");
            let state = workspace.path().join(".repogrammar");
            create_index_state(&state);
            let store = SqliteIndexStore::new(&state);
            let first = store.prepare_next_generation().expect("prepare first");
            store.activate_generation(&first).expect("activate first");

            let error = index_repository_with_discovery_parser_and_store(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &BadIrParser(mode),
                &store,
            )
            .expect_err("bad parser IR must abort new generation");

            assert!(
                error.to_string().contains("parser"),
                "unexpected error: {error}"
            );
            assert_eq!(active_generation_id(&state).as_deref(), Some("gen-000001"));
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
                    ir_nodes: Vec::new(),
                    ir_edges: Vec::new(),
                    semantic_facts: Vec::new(),
                    diagnostics: Vec::new(),
                })
            }
        }

        let workspace = TempWorkspace::new("indexing-code-unit-record-fail");
        fs::write(workspace.path().join("a.ts"), "xy").expect("write source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
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

        assert_eq!(active_generation_id(&state).as_deref(), Some("gen-000001"));
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

            fn remove_indexed_file(
                &self,
                _generation: &GenerationHandle,
                _path: &str,
            ) -> Result<(), IndexStoreError> {
                panic!("file removal must not run after file record failure")
            }

            fn record_code_unit(
                &self,
                _generation: &GenerationHandle,
                _unit: &IndexedCodeUnitRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("code unit recording must not run after file record failure")
            }

            fn record_ir_node(
                &self,
                _generation: &GenerationHandle,
                _node: &IndexedIrNodeRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("IR node recording must not run after file record failure")
            }

            fn record_ir_edge(
                &self,
                _generation: &GenerationHandle,
                _edge: &IndexedIrEdgeRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("IR edge recording must not run after file record failure")
            }

            fn record_semantic_fact(
                &self,
                _generation: &GenerationHandle,
                _fact: &IndexedSemanticFactRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("semantic fact recording must not run during syntax-only indexing")
            }

            fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError> {
                panic!("active indexed file reads must not run during indexing")
            }

            fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
                panic!("active code-unit reads must not run during indexing")
            }

            fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError> {
                panic!("active semantic-fact reads must not run during indexing")
            }

            fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError> {
                panic!("active IR graph reads must not run during indexing")
            }

            fn load_active_claim_input_snapshot(
                &self,
            ) -> Result<ActiveClaimInputSnapshot, IndexStoreError> {
                panic!("active claim-input snapshot reads must not run during indexing")
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
        create_index_state(&workspace.path().join(".repogrammar"));

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

            fn remove_indexed_file(
                &self,
                generation: &GenerationHandle,
                _path: &str,
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

            fn record_ir_node(
                &self,
                generation: &GenerationHandle,
                _node: &IndexedIrNodeRecord,
            ) -> Result<(), IndexStoreError> {
                self.recorded_generations
                    .borrow_mut()
                    .push(generation.generation_id.clone());
                Ok(())
            }

            fn record_ir_edge(
                &self,
                generation: &GenerationHandle,
                _edge: &IndexedIrEdgeRecord,
            ) -> Result<(), IndexStoreError> {
                self.recorded_generations
                    .borrow_mut()
                    .push(generation.generation_id.clone());
                Ok(())
            }

            fn record_semantic_fact(
                &self,
                _generation: &GenerationHandle,
                _fact: &IndexedSemanticFactRecord,
            ) -> Result<(), IndexStoreError> {
                panic!("semantic fact recording must not run during syntax-only indexing")
            }

            fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError> {
                panic!("active indexed file reads must not run during indexing")
            }

            fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
                panic!("active code-unit reads must not run during indexing")
            }

            fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError> {
                panic!("active semantic-fact reads must not run during indexing")
            }

            fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError> {
                panic!("active IR graph reads must not run during indexing")
            }

            fn load_active_claim_input_snapshot(
                &self,
            ) -> Result<ActiveClaimInputSnapshot, IndexStoreError> {
                panic!("active claim-input snapshot reads must not run during indexing")
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
                    layout: IndexStorageLayout::Mutable,
                    mutable_database_present: true,
                    legacy_generation_layout_present: false,
                    wal_bytes: Some(0),
                    shm_bytes: Some(0),
                    active_generation: Some(self.active_generation.borrow().clone()),
                    schema_version: Some(STORAGE_SCHEMA_VERSION),
                    code_unit_count: Some(0),
                    dependency_record_count: Some(0),
                    dirty_record_count: Some(0),
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
        create_index_state(&workspace.path().join(".repogrammar"));
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
