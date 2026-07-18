//! Indexing use-case boundary.

use crate::adapters::frameworks::rust_general::rust_support_target_is_role_compatible;
use crate::adapters::frameworks::{cpp, csharp, java, tsjs};
use crate::adapters::parsing::cpp::{CPP_ANCHOR_ENGINE, CPP_ANCHOR_METHOD};
use crate::adapters::parsing::csharp::{CSHARP_ANCHOR_ENGINE, CSHARP_ANCHOR_METHOD};
use crate::adapters::parsing::java::{JAVA_ANCHOR_ENGINE, JAVA_ANCHOR_METHOD};
use crate::adapters::parsing::python::{
    python_project_config_parser_method, MAX_PYTHON_FRONTEND_INPUT_BYTES,
};
use crate::adapters::parsing::rust::{RUST_ANCHOR_ENGINE, RUST_ANCHOR_METHOD};
use crate::adapters::parsing::tsjs::{TSJS_ANCHOR_ENGINE, TSJS_ANCHOR_METHOD};
use crate::application::family::{
    build_family_claims, cpp_support_target_is_role_compatible,
    csharp_support_target_is_role_compatible, family_constraint_profile_record,
    family_eligible_kind, family_storage_records, family_unknown_blocks_claim,
    java_support_target_is_role_compatible, min_family_support,
    python_support_target_is_role_compatible, tsjs_support_target_is_role_compatible,
    CPP_DERIVED_SUPPORT_ENGINE, CPP_DERIVED_SUPPORT_METHOD, CSHARP_DERIVED_SUPPORT_ENGINE,
    CSHARP_DERIVED_SUPPORT_METHOD, JAVA_DERIVED_SUPPORT_ENGINE, JAVA_DERIVED_SUPPORT_METHOD,
    RUST_DERIVED_SUPPORT_ENGINE, RUST_DERIVED_SUPPORT_METHOD, TSJS_DERIVED_SUPPORT_ENGINE,
    TSJS_DERIVED_SUPPORT_METHOD,
};
use crate::application::progress::{ProgressEvent, ProgressStage, WorkUnits};
use crate::application::proof_lattice::{derived_support_fact, DerivedSupportSpec};
use crate::core::model::{
    CodeUnit, CodeUnitId, ContentHash, Evidence, FactCertainty, FactOrigin, IrEdge, IrNode,
    Language, Provenance, RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange,
    SymbolId,
};
use crate::core::policy::paths::validate_repo_relative_path;
use crate::error::RepoGrammarError;
use crate::ports::family_store::{
    FamilyConstraintProfileStore, FamilyStore, FamilyStoreWithProfiles, GenerationWriteSession,
    GenerationWriteStore,
};
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};
use crate::ports::index_store::{
    ActiveClaimInputSnapshot, GenerationEngineStampStore, IndexStorageLayout, IndexStore,
    IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord, IndexedIrEdgeRecord,
    IndexedIrNodeRecord, IndexedSemanticFactRecord, PythonModuleInterfaceStore,
    STORAGE_SCHEMA_VERSION,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, ParserProjectFileContext, ParserTsJsPathAlias,
    PythonInterfaceProbe, SourceDocument, SourceParseOutput, SourceParser,
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
    pub indexing_mode: IndexingGenerationMode,
    pub parser_attempted_files: usize,
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
pub enum IndexingGenerationMode {
    FileManifestOnly,
    SyntaxOnlyCodeUnits,
}

impl IndexingGenerationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "file_manifest_only",
            Self::SyntaxOnlyCodeUnits => "syntax_only_code_units",
        }
    }

    pub fn parser_status(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "deferred",
            Self::SyntaxOnlyCodeUnits => "syntax_only",
        }
    }

    pub fn human_summary(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "file manifest stored",
            Self::SyntaxOnlyCodeUnits => "syntax-only code units stored",
        }
    }
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
    /// Cross-generation family-identity change versus the base generation, or
    /// `None` when there is no base generation to diff against (first build).
    pub family_identity_delta: Option<FamilyIdentityDelta>,
}

/// Family-id set difference between the base generation and the newly recorded
/// generation. Ids are deterministic follow-up handles: a family that is
/// re-clustered under a different characteristic profile appears as one removed
/// and one added id, not as an in-place rename. Samples are sorted and bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyIdentityDelta {
    pub added_count: usize,
    pub removed_count: usize,
    pub added_sample: Vec<String>,
    pub removed_sample: Vec<String>,
}

/// Maximum family ids listed in each bounded `FamilyIdentityDelta` sample.
const FAMILY_IDENTITY_DELTA_SAMPLE_CAP: usize = 20;

impl FamilyIdentityDelta {
    fn from_id_sets(base_ids: &BTreeSet<String>, new_ids: &BTreeSet<String>) -> Self {
        let added: Vec<String> = new_ids.difference(base_ids).cloned().collect();
        let removed: Vec<String> = base_ids.difference(new_ids).cloned().collect();
        FamilyIdentityDelta {
            added_count: added.len(),
            removed_count: removed.len(),
            added_sample: added
                .into_iter()
                .take(FAMILY_IDENTITY_DELTA_SAMPLE_CAP)
                .collect(),
            removed_sample: removed
                .into_iter()
                .take(FAMILY_IDENTITY_DELTA_SAMPLE_CAP)
                .collect(),
        }
    }
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
        indexing_mode: IndexingGenerationMode::FileManifestOnly,
        parser_attempted_files: 0,
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
    store: &(impl IndexStore + GenerationWriteStore),
) -> Result<IndexingOutcome, RepoGrammarError> {
    let _index_lock = crate::application::repository::acquire_index_lock(
        &request.repository_root,
        request.state_dir_override.as_deref(),
    )?;
    let report = discover_repository_files(request.clone(), discovery)?;
    let generation = crate::application::storage::prepare_index_generation(store)?;
    {
        let mut session =
            crate::application::storage::open_index_write_session(store, &generation)?;
        for file in &report.files {
            crate::application::storage::record_indexed_file(
                session.as_mut(),
                &IndexedFileRecord {
                    path: file.path.clone(),
                    content_hash: file.content_hash.clone(),
                    size_bytes: file.size_bytes,
                    language: file.language.as_str().to_string(),
                },
            )?;
        }
        crate::application::storage::finish_index_write_session(session.as_mut())?;
    }
    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexing_mode: IndexingGenerationMode::FileManifestOnly,
        parser_attempted_files: 0,
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
    store: &(impl IndexStore + GenerationWriteStore),
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
    store: &(impl IndexStore + GenerationWriteStore),
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
    store: &(impl IndexStore
          + GenerationEngineStampStore
          + PythonModuleInterfaceStore
          + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore + GenerationWriteStore),
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
    store: &(impl IndexStore + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore + FamilyStore + FamilyConstraintProfileStore + GenerationWriteStore),
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
    store: &(impl IndexStore
          + FamilyStore
          + FamilyConstraintProfileStore
          + GenerationEngineStampStore
          + PythonModuleInterfaceStore
          + GenerationWriteStore),
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
    store: &(impl IndexStore
          + FamilyStore
          + FamilyConstraintProfileStore
          + GenerationEngineStampStore
          + PythonModuleInterfaceStore
          + GenerationWriteStore),
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
    store: &(impl IndexStore + GenerationWriteStore),
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
    IndexStoreImpl: IndexStore + GenerationWriteStore,
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
    // One write session serves the whole build: a single connection with pragmas
    // applied once and bounded-batch transactions replaces the historical
    // per-record connection opens. It is finished (committed and sealed) before
    // validation; an early return drops it, which rolls back the open batch and
    // stamps the terminal `failed` status.
    let mut session = crate::application::storage::open_index_write_session(store, &generation)?;
    for (index, file) in report.files.iter().enumerate() {
        crate::application::storage::record_indexed_file(
            session.as_mut(),
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
    let mut parser_attempted_files = 0usize;
    let mut indexed_code_units = Vec::new();
    let mut parser_semantic_facts = Vec::new();
    let mut framework_role_facts = Vec::new();
    let mut warnings = report.warnings.clone();
    extend_inventory_only_language_warnings(&mut warnings, &report);
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "building parser project context",
        WorkUnits::Unknown,
    );
    let parser_context = parser_project_context(&request, &report, source_store, parser)?;
    for (index, file) in report.files.iter().enumerate() {
        if discovered_language_is_inventory_only(file.language) {
            emit_progress(
                progress,
                ProgressStage::SyntaxParsing,
                "deferred inventory-only files",
                known_work_units(index + 1, report.files.len()),
            );
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
        parser_attempted_files += 1;
        let SourceParseOutput {
            report: parse_report,
            python_interface_hash,
        } = match parser.parse_with_context_output(
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
            Ok(output) => output,
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
            Err(ParseError::Timeout) => {
                return Err(RepoGrammarError::InvalidInput(
                    "parser timed out while analyzing a source file".to_string(),
                ));
            }
            Err(ParseError::PythonFrontendContractMismatch) => {
                return Err(python_frontend_contract_mismatch_error());
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {}: internal parser error",
                    file.path
                )));
            }
        };
        let parse_outcome = record_parse_report(
            session.as_mut(),
            file,
            &source.text,
            parse_report,
            options.framework_roles,
            &mut warnings,
        )?;
        record_python_module_interface_if_python(
            session.as_mut(),
            file,
            python_interface_hash.as_deref(),
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
    // Phase boundary: the file, code-unit, and IR write phase is complete.
    crate::application::storage::checkpoint_index_write_session(session.as_mut())?;

    sort_semantic_facts(&mut parser_semantic_facts);
    let parser_fact_count = record_semantic_facts(session.as_mut(), 0, &parser_semantic_facts)?;
    sort_semantic_facts(&mut framework_role_facts);
    let framework_fact_count =
        record_semantic_facts(session.as_mut(), parser_fact_count, &framework_role_facts)?;
    let mut derived_python_support_facts = derive_python_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_python_support_facts);
    let derived_python_support_fact_count = record_semantic_facts(
        session.as_mut(),
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
        session.as_mut(),
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
        session.as_mut(),
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count,
        &derived_java_support_facts,
    )?;
    let mut derived_csharp_support_facts = derive_csharp_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_csharp_support_facts);
    let derived_csharp_support_fact_count = record_semantic_facts(
        session.as_mut(),
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count
            + derived_java_support_fact_count,
        &derived_csharp_support_facts,
    )?;
    let mut derived_cpp_support_facts = derive_cpp_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_cpp_support_facts);
    let derived_cpp_support_fact_count = record_semantic_facts(
        session.as_mut(),
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count
            + derived_java_support_fact_count
            + derived_csharp_support_fact_count,
        &derived_cpp_support_facts,
    )?;
    let mut derived_rust_support_facts = derive_rust_framework_support_facts(
        &indexed_code_units,
        &parser_semantic_facts,
        &framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_rust_support_facts);
    let derived_rust_support_fact_count = record_semantic_facts(
        session.as_mut(),
        parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + derived_tsjs_support_fact_count
            + derived_java_support_fact_count
            + derived_csharp_support_fact_count
            + derived_cpp_support_fact_count,
        &derived_rust_support_facts,
    )?;
    let local_support_fact_count = parser_fact_count
        + framework_fact_count
        + derived_python_support_fact_count
        + derived_tsjs_support_fact_count
        + derived_java_support_fact_count
        + derived_csharp_support_fact_count
        + derived_cpp_support_fact_count
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
        options.rust_provider,
        session.as_mut(),
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
            semantic_worker: options.semantic_worker,
            fact_id_offset: local_support_fact_count + rust_provider_fact_count,
        },
        session.as_mut(),
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
        session.as_mut(),
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

    // Phase boundary: the semantic-fact write phase is complete.
    crate::application::storage::checkpoint_index_write_session(session.as_mut())?;
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
                + derived_csharp_support_facts.len()
                + derived_cpp_support_facts.len()
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
        family_facts.extend(derived_csharp_support_facts);
        family_facts.extend(derived_cpp_support_facts);
        family_facts.extend(derived_rust_support_facts);
        family_facts.extend(rust_provider_facts.iter().cloned());
        family_facts.extend(worker_facts);
        family_facts.extend(derived_tsjs_provider_support_facts);
        // Capture the base generation's family ids before the new generation is
        // activated, but only when there is a base generation to diff against.
        let base_family_ids = sync_report
            .as_ref()
            .filter(|report| report.base_generation.is_some())
            .map(|_| base_generation_family_ids(family_store))
            .transpose()?;
        let (family_count, new_family_ids) =
            record_family_claims(session.as_mut(), &indexed_code_units, &family_facts)?;
        if let Some(sync_report) = sync_report.as_mut() {
            sync_report.families_recomputed = family_count;
            if let Some(base_family_ids) = &base_family_ids {
                sync_report.family_identity_delta = Some(FamilyIdentityDelta::from_id_sets(
                    base_family_ids,
                    &new_family_ids,
                ));
            }
        }
        emit_progress(
            progress,
            ProgressStage::FamilyConstruction,
            "stored eligible family claims",
            WorkUnits::Unknown,
        );
        // Phase boundary: the family recomputation and write phase is complete.
        crate::application::storage::checkpoint_index_write_session(session.as_mut())?;
    }

    // Commit and seal the write session so validation and activation observe the
    // fully committed generation on their own connections.
    crate::application::storage::finish_index_write_session(session.as_mut())?;
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

    if let Some(sync_report) = sync_report.as_mut() {
        sync_report.reparsed_files = parser_attempted_files;
    }

    Ok(IndexingOutcome {
        indexing_mode: indexing_generation_mode(&report),
        parser_attempted_files,
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
    store: &(impl IndexStore
          + GenerationEngineStampStore
          + PythonModuleInterfaceStore
          + GenerationWriteStore),
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
    let base_engine_version = runtime
        .store
        .active_generation_engine_version()
        .map_err(index_store_error)?;
    let preflight = incremental_sync_preflight(
        runtime.store,
        semantic_worker_configured,
        base_engine_version.as_deref(),
    )?;
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

    let active_files = runtime
        .store
        .list_active_indexed_files()
        .map_err(index_store_error)?;
    let delta = compute_sync_delta_from_files(&active_files.files, &report);
    if delta.is_empty() {
        let stats = runtime
            .store
            .active_repo_shape_stats()
            .map_err(index_store_error)?;
        if active_files.generation_id != base_generation || stats.generation_id != base_generation {
            return Err(RepoGrammarError::InvalidInput(
                "active generation changed during unchanged sync".to_string(),
            ));
        }
        emit_progress(
            runtime.progress,
            ProgressStage::FileScanning,
            "no indexed file changes",
            known_work_units(report.files.len(), report.files.len()),
        );
        emit_progress(
            runtime.progress,
            ProgressStage::PersistenceValidation,
            "retained current active generation",
            WorkUnits::Unknown,
        );
        let mut warnings = report.warnings.clone();
        extend_inventory_only_language_warnings(&mut warnings, &report);
        return Ok(IndexingOutcome {
            indexing_mode: indexing_generation_mode(&report),
            parser_attempted_files: 0,
            indexed_units: stats.indexed_code_unit_count,
            semantic_facts: stats.semantic_fact_count,
            discovered_files: report.files.len(),
            skipped_paths: report.skipped.len(),
            active_generation: Some(base_generation.clone()),
            semantic_worker: SemanticWorkerRunStatus::Deferred,
            sync_report: Some(IndexingSyncReport {
                base_generation: Some(base_generation),
                sync_mode: IndexingSyncMode::Incremental,
                fallback_reason: None,
                added_files: 0,
                modified_files: 0,
                removed_files: 0,
                unchanged_files: delta.unchanged_files.len(),
                copied_forward_files: 0,
                reparsed_files: 0,
                families_recomputed: 0,
                dirty_records_cleared: 0,
                family_identity_delta: None,
            }),
            warnings,
        });
    }

    let snapshot = runtime
        .store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    if snapshot.generation_id != base_generation {
        return Err(RepoGrammarError::InvalidInput(
            "active generation changed during incremental sync".to_string(),
        ));
    }
    let decision = classify_sync_context_gate(
        &request,
        &delta,
        &snapshot.files,
        &report.files,
        runtime.source_store,
        runtime.parser,
        runtime.store,
    )?;
    match decision {
        SyncContextDecision::FullRebuild(reason) => {
            let sync_report = sync_fallback_report(
                Some(snapshot.generation_id.clone()),
                &report,
                Some(&delta),
                reason,
            );
            index_repository_full_after_discovery(request, report, &mut runtime, Some(sync_report))
        }
        SyncContextDecision::Incremental(base_interfaces) => {
            index_repository_incremental_after_discovery(
                request,
                report,
                snapshot,
                delta,
                base_interfaces,
                &mut runtime,
            )
        }
    }
}

struct IncrementalSyncPreflight {
    base_generation: Option<String>,
    fallback_reason: Option<String>,
}

fn incremental_sync_preflight(
    store: &impl IndexStore,
    semantic_worker_configured: bool,
    base_engine_version: Option<&str>,
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
    } else if base_engine_version != Some(env!("CARGO_PKG_VERSION")) {
        // The active generation was produced by a different RepoGrammar engine
        // version than the running binary. Copy-forward would relabel its facts
        // with the new version without reparsing, so rebuild instead. A missing
        // stamp (`None`) is treated as a mismatch: never copy forward facts of
        // unknown provenance.
        fallback_reason = Some("engine_version_changed".to_string());
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

    fn is_empty(&self) -> bool {
        self.added_files.is_empty()
            && self.modified_files.is_empty()
            && self.removed_files.is_empty()
    }
}

fn compute_sync_delta(
    snapshot: &ActiveClaimInputSnapshot,
    report: &FileDiscoveryReport,
) -> SyncDelta {
    compute_sync_delta_from_files(&snapshot.files, report)
}

fn compute_sync_delta_from_files(
    active_files: &[IndexedFileRecord],
    report: &FileDiscoveryReport,
) -> SyncDelta {
    let active_by_path = active_files
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
    for active in active_files {
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
        reparsed_files: 0,
        families_recomputed: 0,
        dirty_records_cleared: 0,
        family_identity_delta: None,
    }
}

fn index_repository_incremental_after_discovery<SourceStoreImpl, SourceParserImpl, IndexStoreImpl>(
    request: IndexingRequest,
    report: FileDiscoveryReport,
    snapshot: ActiveClaimInputSnapshot,
    delta: SyncDelta,
    base_python_interfaces: BTreeMap<String, String>,
    runtime: &mut IndexingRuntime<'_, SourceStoreImpl, SourceParserImpl, IndexStoreImpl>,
) -> Result<IndexingOutcome, RepoGrammarError>
where
    SourceStoreImpl: SourceStore,
    SourceParserImpl: SourceParser,
    IndexStoreImpl: IndexStore + GenerationWriteStore,
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
    // One write session serves the whole incremental build, including the
    // copy-forward of unchanged rows; see the full-build path for the lifecycle.
    let mut session = crate::application::storage::open_index_write_session(store, &generation)?;
    let changed_files = delta.changed_files();
    let inventory_only_paths = inventory_only_paths(&report);
    let unchanged_paths = delta
        .unchanged_files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();

    let mut stored_files = 0usize;
    for file in &delta.unchanged_files {
        crate::application::storage::record_indexed_file(session.as_mut(), file)?;
        // Copy the Python interface hash forward for every unchanged `.py` module.
        // Reaching this path guaranteed every modified module's interface was
        // unchanged and no `.py` was added or removed, so an unchanged module's
        // stored hash equals what a full rebuild would recompute (identical
        // content, identical engine). A module with no stored hash (its build-time
        // probe failed) is simply skipped; the next sync treats the gap as
        // `python_interface_unverified` and rebuilds.
        if file.language == DiscoveredLanguage::Python.as_str() {
            if let Some(interface_hash) = base_python_interfaces.get(&file.path) {
                session
                    .as_mut()
                    .record_python_module_interface(&file.path, interface_hash)
                    .map_err(index_store_error)?;
            }
        }
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
            session.as_mut(),
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
        if !unchanged_paths.contains(&unit.path) || inventory_only_paths.contains(&unit.path) {
            continue;
        }
        crate::application::storage::record_code_unit(session.as_mut(), unit)?;
        copied_unit_ids.insert(unit.id.clone());
        indexed_code_units.push(unit.clone());
    }

    let mut copied_node_ids = BTreeSet::new();
    for node in &snapshot.ir_nodes {
        if !copied_unit_ids.contains(&node.code_unit_id) {
            continue;
        }
        crate::application::storage::record_ir_node(session.as_mut(), node)?;
        copied_node_ids.insert(node.id.clone());
    }
    for edge in &snapshot.ir_edges {
        if copied_node_ids.contains(&edge.from_node_id)
            && copied_node_ids.contains(&edge.to_node_id)
        {
            crate::application::storage::record_ir_edge(session.as_mut(), edge)?;
        }
    }

    let mut copied_semantic_records = Vec::new();
    let mut copied_parser_facts = Vec::new();
    let mut copied_framework_role_facts = Vec::new();
    // Provider-resolved TS/JS support facts are excluded from copy-forward (they
    // are a derived-support engine) but the raw worker facts they are derived
    // from survive, so capture them here to recompute provider support below.
    // Without this, a worker-less incremental sync would silently drop the base
    // generation's provider-resolved support and diverge from a full rebuild.
    let mut copied_worker_facts = Vec::new();
    for record in &snapshot.semantic_facts {
        if !unchanged_paths.contains(&record.path)
            || inventory_only_paths.contains(&record.path)
            || is_local_derived_support_record(record)
        {
            continue;
        }
        crate::application::storage::record_semantic_fact(session.as_mut(), record)?;
        let fact = semantic_fact_from_index_record(record)?;
        if fact.kind == SemanticFactKind::FrameworkRole
            && fact.certainty == FactCertainty::FrameworkHeuristic
        {
            copied_framework_role_facts.push(fact);
        } else {
            if tsjs_provider_resolved_compiler_fact(&fact) {
                copied_worker_facts.push(fact.clone());
            }
            copied_parser_facts.push(fact);
        }
        copied_semantic_records.push(record.clone());
    }

    let mut warnings = report.warnings.clone();
    extend_inventory_only_language_warnings(&mut warnings, &report);
    emit_progress(
        progress,
        ProgressStage::ProjectDiscovery,
        "building parser project context",
        WorkUnits::Unknown,
    );
    let parser_context = parser_project_context(&request, &report, source_store, parser)?;
    let mut parser_attempted_files = 0usize;
    let mut parser_semantic_facts = Vec::new();
    let mut framework_role_facts = Vec::new();
    for (index, file) in changed_files.iter().enumerate() {
        if discovered_language_is_inventory_only(file.language) {
            emit_progress(
                progress,
                ProgressStage::SyntaxParsing,
                "deferred changed inventory-only files",
                known_work_units(index + 1, changed_files.len()),
            );
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
        parser_attempted_files += 1;
        let SourceParseOutput {
            report: parse_report,
            python_interface_hash,
        } = match parser.parse_with_context_output(
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
            Ok(output) => output,
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
            Err(ParseError::Timeout) => {
                return Err(RepoGrammarError::InvalidInput(
                    "parser timed out while analyzing a source file".to_string(),
                ));
            }
            Err(ParseError::PythonFrontendContractMismatch) => {
                return Err(python_frontend_contract_mismatch_error());
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {}: internal parser error",
                    file.path
                )));
            }
        };
        let parse_outcome = record_parse_report(
            session.as_mut(),
            file,
            &source.text,
            parse_report,
            options.framework_roles,
            &mut warnings,
        )?;
        // A reparsed `.py` module stores the interface hash returned by that same
        // parse request. Modified modules reached here only with an unchanged
        // interface, so the fresh hash equals the copied-forward base hash of an
        // unchanged module — both paths converge on the full-rebuild table.
        record_python_module_interface_if_python(
            session.as_mut(),
            file,
            python_interface_hash.as_deref(),
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
    // Phase boundary: the file, code-unit, and IR write phase is complete.
    crate::application::storage::checkpoint_index_write_session(session.as_mut())?;

    let mut next_fact_offset = next_semantic_fact_offset(&copied_semantic_records);
    sort_semantic_facts(&mut parser_semantic_facts);
    let parser_fact_count =
        record_semantic_facts(session.as_mut(), next_fact_offset, &parser_semantic_facts)?;
    next_fact_offset += parser_fact_count;
    sort_semantic_facts(&mut framework_role_facts);
    let framework_fact_count =
        record_semantic_facts(session.as_mut(), next_fact_offset, &framework_role_facts)?;
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
        session.as_mut(),
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
        session.as_mut(),
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
        session.as_mut(),
        next_fact_offset,
        &derived_java_support_facts,
    )?;
    next_fact_offset += derived_java_support_fact_count;
    let mut derived_csharp_support_facts = derive_csharp_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_csharp_support_facts);
    let derived_csharp_support_fact_count = record_semantic_facts(
        session.as_mut(),
        next_fact_offset,
        &derived_csharp_support_facts,
    )?;
    next_fact_offset += derived_csharp_support_fact_count;
    let mut derived_cpp_support_facts = derive_cpp_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_cpp_support_facts);
    let derived_cpp_support_fact_count = record_semantic_facts(
        session.as_mut(),
        next_fact_offset,
        &derived_cpp_support_facts,
    )?;
    next_fact_offset += derived_cpp_support_fact_count;
    let mut derived_rust_support_facts = derive_rust_framework_support_facts(
        &indexed_code_units,
        &all_parser_facts,
        &all_framework_role_facts,
    )?;
    sort_semantic_facts(&mut derived_rust_support_facts);
    let derived_rust_support_fact_count = record_semantic_facts(
        session.as_mut(),
        next_fact_offset,
        &derived_rust_support_facts,
    )?;
    next_fact_offset += derived_rust_support_fact_count;
    // Recompute provider-resolved TS/JS support from the copied-forward worker
    // facts so incremental-sync family support matches a full rebuild for
    // unchanged files instead of silently dropping it.
    let mut derived_tsjs_provider_support_facts =
        derive_tsjs_provider_resolved_framework_support_facts(
            &indexed_code_units,
            &all_parser_facts,
            &all_framework_role_facts,
            &copied_worker_facts,
        )?;
    sort_semantic_facts(&mut derived_tsjs_provider_support_facts);
    let derived_tsjs_provider_support_fact_count = record_semantic_facts(
        session.as_mut(),
        next_fact_offset,
        &derived_tsjs_provider_support_facts,
    )?;
    let local_support_fact_count = copied_semantic_records.len()
        + parser_fact_count
        + framework_fact_count
        + derived_python_support_fact_count
        + derived_tsjs_support_fact_count
        + derived_java_support_fact_count
        + derived_csharp_support_fact_count
        + derived_cpp_support_fact_count
        + derived_rust_support_fact_count
        + derived_tsjs_provider_support_fact_count;
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
        reparsed_files: parser_attempted_files,
        families_recomputed: 0,
        dirty_records_cleared: 0,
        family_identity_delta: None,
    };

    // Phase boundary: the semantic-fact write phase is complete.
    crate::application::storage::checkpoint_index_write_session(session.as_mut())?;
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
                + derived_csharp_support_facts.len()
                + derived_cpp_support_facts.len()
                + derived_rust_support_facts.len()
                + derived_tsjs_provider_support_facts.len(),
        );
        family_facts.extend(all_parser_facts);
        family_facts.extend(all_framework_role_facts);
        family_facts.extend(derived_python_support_facts);
        family_facts.extend(derived_tsjs_support_facts);
        family_facts.extend(derived_java_support_facts);
        family_facts.extend(derived_csharp_support_facts);
        family_facts.extend(derived_cpp_support_facts);
        family_facts.extend(derived_rust_support_facts);
        family_facts.extend(derived_tsjs_provider_support_facts);
        // The incremental path always resyncs from an active base generation, so
        // its family ids are always available to diff against.
        let base_family_ids = base_generation_family_ids(family_store)?;
        let (family_count, new_family_ids) =
            record_family_claims(session.as_mut(), &indexed_code_units, &family_facts)?;
        sync_report.families_recomputed = family_count;
        sync_report.family_identity_delta = Some(FamilyIdentityDelta::from_id_sets(
            &base_family_ids,
            &new_family_ids,
        ));
        emit_progress(
            progress,
            ProgressStage::FamilyConstruction,
            "stored eligible family claims",
            WorkUnits::Unknown,
        );
        // Phase boundary: the family recomputation and write phase is complete.
        crate::application::storage::checkpoint_index_write_session(session.as_mut())?;
    }

    // Commit and seal the write session so validation and activation observe the
    // fully committed generation on their own connections.
    crate::application::storage::finish_index_write_session(session.as_mut())?;
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
        indexing_mode: indexing_generation_mode(&report),
        parser_attempted_files,
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
            | CSHARP_DERIVED_SUPPORT_ENGINE
            | CPP_DERIVED_SUPPORT_ENGINE
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

/// The incremental-sync context gate decision. Either the delta only touches
/// file-local and interface-stable inputs and the incremental path is safe
/// (carrying the base generation's Python interface hashes for copy-forward), or
/// some change forces a full rebuild with a specific, observable reason.
enum SyncContextDecision {
    /// Proceed incrementally. Holds the base generation's `path -> interface_hash`
    /// map so unchanged Python modules copy their hash forward unchanged.
    Incremental(BTreeMap<String, String>),
    /// Rebuild fully. The reason string flows verbatim into
    /// `IndexingSyncReport.fallback_reason`.
    FullRebuild(&'static str),
}

/// Decide whether a computed delta can take the incremental path.
///
/// Ordering (first hit wins):
/// 1. Any add/remove of a project-context path, or any modified config /
///    `conftest.py` / non-file-local change other than an interface-eligible
///    Python module edit, forces a full rebuild (`project_context_changed`) —
///    unchanged from the pre-existing gate.
/// 2. Otherwise, when the delta modifies any interface-eligible Python module,
///    the Python context-payload regime is checked: if the whole-project
///    `parse_document` context could cross the worker's per-request cap (and be
///    silently dropped) on either the base or the current manifest, a full
///    rebuild is forced (`python_context_budget`). See
///    `python_context_budget_is_safe`.
/// 3. Then every modified interface-eligible Python module is probed: its current
///    interface hash is compared against the base generation's stored hash. A
///    missing stored hash or an unverifiable probe yields
///    `python_interface_unverified`; a differing hash yields
///    `python_interface_changed`; all-unchanged proceeds incrementally.
///
/// `python_interface_unverified` is checked before `python_interface_changed` so
/// an unprovable module dominates a provably-changed one; both force a full
/// rebuild, so the precedence is diagnostic only. The probe loop short-circuits
/// on the first unverified module, since that outcome is already fixed.
fn classify_sync_context_gate(
    request: &IndexingRequest,
    delta: &SyncDelta,
    base_files: &[IndexedFileRecord],
    current_files: &[DiscoveredFile],
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    store: &impl PythonModuleInterfaceStore,
) -> Result<SyncContextDecision, RepoGrammarError> {
    if sync_delta_forces_full_context_excluding_python_modules(delta) {
        return Ok(SyncContextDecision::FullRebuild("project_context_changed"));
    }
    // The base generation's stored interface hashes are both the comparison
    // baseline for modified modules and the copy-forward source for unchanged
    // modules on the incremental path, so load them once here.
    let base_interfaces = store
        .active_python_module_interfaces()
        .map_err(index_store_error)?
        .into_iter()
        .map(|record| (record.path, record.interface_hash))
        .collect::<BTreeMap<_, _>>();
    let eligible_modules = delta
        .modified_files
        .iter()
        .filter(|file| is_interface_eligible_python_module(file))
        .collect::<Vec<_>>();
    // Only a Python *module* modification changes the Python context-payload
    // sizes; adds/removes and conftest/config edits already fell back above, and a
    // delta with no modified Python module leaves the sizes byte-identical, so the
    // budget need only be checked when there is at least one eligible module.
    if !eligible_modules.is_empty()
        && !python_context_budget_is_safe(
            base_files,
            current_files,
            MAX_PYTHON_FRONTEND_INPUT_BYTES,
        )
    {
        return Ok(SyncContextDecision::FullRebuild("python_context_budget"));
    }
    let mut saw_unverified = false;
    let mut saw_changed = false;
    for file in eligible_modules {
        let Some(base_hash) = base_interfaces.get(&file.path) else {
            // No stored interface: a build-time probe failed for this module (a
            // base generation on an older schema is rejected earlier by the
            // preflight `unsupported_storage_schema` gate, so it never reaches
            // here). The edit cannot be proven file-local.
            saw_unverified = true;
            break;
        };
        let source = source_store
            .read_source(SourceReadRequest {
                repository_root: request.repository_root.clone(),
                path: file.path.clone(),
                expected_content_hash: file.content_hash.clone(),
                max_file_bytes: request.max_file_bytes,
            })
            .map_err(source_store_error)?;
        match parser.extract_python_interface(&file.path, &source.text) {
            PythonInterfaceProbe::Computed(current_hash) => {
                if &current_hash != base_hash {
                    saw_changed = true;
                }
            }
            PythonInterfaceProbe::Unverified => {
                // Unverified dominates; the decision is fixed, so stop probing.
                saw_unverified = true;
                break;
            }
        }
    }
    if saw_unverified {
        return Ok(SyncContextDecision::FullRebuild(
            "python_interface_unverified",
        ));
    }
    if saw_changed {
        return Ok(SyncContextDecision::FullRebuild("python_interface_changed"));
    }
    Ok(SyncContextDecision::Incremental(base_interfaces))
}

/// Whether the Python whole-project `parse_document` context stays safely under
/// the worker's per-request byte cap on *both* the base and the current manifest.
///
/// The worker ships every `.py` module's text (`module_files`, and every
/// `conftest.py` again in `conftest_files`) alongside each target document.
/// `serialize_parse_request` silently drops that context when the serialized
/// request exceeds [`MAX_PYTHON_FRONTEND_INPUT_BYTES`], so a target parsed near
/// the cap loses cross-module resolution. A size-changing `.py` edit — even an
/// interface-stable one — can flip that regime, which would make an incremental
/// sync's copied-forward sibling facts (parsed under the base regime) diverge
/// from a clean rebuild's (parsed under the current regime). Requiring both
/// manifests safely under the cap keeps every module's parse context-complete in
/// both regimes, so copy-forward matches a clean rebuild.
///
/// The estimate is a conservative upper bound from manifest sizes only (no file
/// reads): raw byte sizes plus fixed per-entry and envelope overhead, then a
/// `PYTHON_CONTEXT_ESCAPE_HEADROOM`x factor bounding JSON string escaping. Because
/// the gate never reads file contents it cannot assume the bytes are valid Python,
/// so the multiplier must bound the *worst-case* escaping of any byte. `serde_json`
/// (the frontend request serializer) escapes each control character
/// `U+0000..=U+001F` that lacks a short escape as `\uXXXX` — six output bytes for
/// one source byte, the largest per-byte expansion any input can incur (`"`, `\`,
/// and `\b\t\n\f\r` cost two bytes; every byte `>= 0x20` other than `"`/`\`,
/// including all UTF-8 continuation bytes, is emitted verbatim). A `.py` file dense
/// in control characters therefore escapes up to 6x, not the ~2x of ordinary
/// source, so a 6x headroom is the smallest provable bound and `raw * 6 < cap`
/// implies the real serialized request is under the cap on *every* input. A tighter
/// 2x factor was unsound: a control-char-dense module with `raw` in `(cap/6, cap/2)`
/// passes a 2x gate yet can serialize past the cap, reopening the silent
/// context-drop divergence channel this gate exists to close.
fn python_context_budget_is_safe(
    base_files: &[IndexedFileRecord],
    current_files: &[DiscoveredFile],
    max_request_bytes: usize,
) -> bool {
    let cap = max_request_bytes as u128;
    let base = python_context_request_raw_estimate(
        base_files
            .iter()
            .filter(|file| file.language == DiscoveredLanguage::Python.as_str())
            .map(|file| {
                (
                    file.path.as_str(),
                    file.size_bytes,
                    is_python_conftest_path(&file.path),
                )
            }),
    );
    let current = python_context_request_raw_estimate(
        current_files
            .iter()
            .filter(|file| file.language == DiscoveredLanguage::Python)
            .map(|file| {
                (
                    file.path.as_str(),
                    file.size_bytes,
                    is_python_conftest_path(&file.path),
                )
            }),
    );
    base.saturating_mul(PYTHON_CONTEXT_ESCAPE_HEADROOM) < cap
        && current.saturating_mul(PYTHON_CONTEXT_ESCAPE_HEADROOM) < cap
}

/// Fixed overhead (bytes) of a `parse_document` request envelope minus the
/// Python context payload: protocol/contract/mode/path/content-hash/revision keys
/// and the surrounding JSON structure. Generously over-estimated.
const PYTHON_CONTEXT_ENVELOPE_OVERHEAD: u128 = 4096;
/// Fixed JSON overhead (bytes) charged per `.py` module for its `module_files`
/// object braces, keys, and quotes.
const PYTHON_CONTEXT_PER_FILE_OVERHEAD: u128 = 64;
/// Worst-case JSON string-escape expansion for any source byte, applied to the raw
/// estimate before comparing against the request cap. Six bytes (`\uXXXX`) is the
/// largest expansion `serde_json` produces for a single byte — a short-escape-less
/// control character `U+0000..=U+001F` — so this is a provable upper bound that
/// holds even for content the size-only gate cannot inspect. It supersedes an
/// earlier 2x factor that under-counted control-char-dense modules and could admit
/// a request that serialized past the cap.
const PYTHON_CONTEXT_ESCAPE_HEADROOM: u128 = 6;

/// Conservative raw byte estimate of the largest `parse_document` request over a
/// Python file set (each item is `(path, size_bytes, is_conftest)`). Sums the
/// `module_files` payload (every text once), the `conftest_files` payload
/// (conftest text a second time), the `module_paths` list, one target document's
/// text, and fixed overhead. Escaping headroom is applied by the caller.
fn python_context_request_raw_estimate<'a>(
    files: impl Iterator<Item = (&'a str, u64, bool)>,
) -> u128 {
    let mut total = PYTHON_CONTEXT_ENVELOPE_OVERHEAD;
    let mut max_file = 0u128;
    for (path, size, is_conftest) in files {
        let size = u128::from(size);
        let path_len = path.len() as u128;
        // `module_files` entry {"path":...,"text":...} plus the `module_paths`
        // entry: the text once, the path twice, and per-file structural overhead.
        total = total.saturating_add(size + 2 * path_len + 2 * PYTHON_CONTEXT_PER_FILE_OVERHEAD);
        if is_conftest {
            // conftest text is shipped a second time in `conftest_files`.
            total = total.saturating_add(size + path_len + PYTHON_CONTEXT_PER_FILE_OVERHEAD);
        }
        max_file = max_file.max(size);
    }
    // The target document's own text is shipped once more in the envelope.
    total.saturating_add(max_file)
}

/// A modified file is interface-eligible only when it is a discovered Python
/// module (`DiscoveredLanguage::Python`, so not a `*Config` classification such
/// as root `setup.py`/`setup.cfg`/`pyproject.toml`) and not a `conftest.py`.
/// `conftest.py` alters ancestor fixture context for a whole subtree, and root
/// configs alter source roots — neither is captured by a module's interface
/// projection, so both keep full-rebuild behavior regardless of interface.
fn is_interface_eligible_python_module(file: &DiscoveredFile) -> bool {
    file.language == DiscoveredLanguage::Python && !is_python_conftest_path(&file.path)
}

/// The syntactic half of the context gate: every project-context change that
/// forces a full rebuild *except* an interface-eligible Python module edit
/// (which the caller resolves with an interface probe). This is the pre-existing
/// `sync_delta_touches_project_context` behavior with modified Python modules
/// carved out for the interface check.
fn sync_delta_forces_full_context_excluding_python_modules(delta: &SyncDelta) -> bool {
    // Adds and removes of a project-context path change that language's discovered
    // path set (Python module index, Rust `mod` candidates, TS/JS import
    // resolution), which can alter how *other* files parse, so they always force a
    // full rebuild.
    let added_or_removed = delta
        .added_files
        .iter()
        .filter(|file| !discovered_language_is_inventory_only(file.language))
        .map(|file| file.path.as_str())
        .chain(
            delta
                .removed_files
                .iter()
                .filter(|file| !indexed_language_is_inventory_only(&file.language))
                .map(|file| file.path.as_str()),
        )
        .any(sync_path_requires_full_project_context);
    // A content-only modification (same path present in both manifests, changed
    // hash) forces a full rebuild when the edit could still change another file's
    // parse (see `modified_file_requires_full_project_context`), unless it is an
    // interface-eligible Python module — those defer to the interface probe.
    let modified = delta
        .modified_files
        .iter()
        .filter(|file| !discovered_language_is_inventory_only(file.language))
        .any(|file| {
            modified_file_requires_full_project_context(file)
                && !is_interface_eligible_python_module(file)
        });
    added_or_removed || modified
}

/// Whether a content-only modification of `file` must force a full rebuild.
///
/// Rust and TS/JS parsers consume only their own discovered path set plus root
/// configuration (Rust: `rust_module_paths` + the nearest `Cargo.toml`'s feature
/// names; TS/JS: `tsjs_module_paths` + the root tsconfig/jsconfig/package.json
/// projections and the test-runner flag) — never another file's source text
/// (`docs/specifications/indexing-pipeline.md`). A content-only edit (this is the
/// modified bucket: the path exists in both the base and the current manifest,
/// only its hash changed) leaves every language path set and every root
/// configuration byte-identical, so it cannot change how any *other* file parses.
/// Exactly the edited file must be reparsed — the file-local fast path.
///
/// Python is deliberately excluded: its parser consumes `python_module_files`
/// and `python_conftest_files` (the text of every module and `conftest.py`), so a
/// Python content edit can change another file's parse. Configuration files are
/// excluded because discovery classifies them as `*Config` languages, not as
/// `language_is_file_local_source`, and they feed project-wide context. Adds and
/// removes are handled separately above because they change the path set.
fn modified_file_requires_full_project_context(file: &DiscoveredFile) -> bool {
    if language_is_file_local_source(file.language) {
        return false;
    }
    sync_path_requires_full_project_context(file.path.as_str())
}

/// Languages whose parser output for one file is independent of every other
/// file's source text, so a content-only edit is provably file-local. Rust and
/// TS/JS join the already-incremental Java/C#/C/C++ family, which is excluded
/// from the project-context gate entirely because those parsers ignore context.
/// Python and all `*Config` classifications are intentionally absent.
fn language_is_file_local_source(language: DiscoveredLanguage) -> bool {
    matches!(
        language,
        DiscoveredLanguage::Rust
            | DiscoveredLanguage::TypeScript
            | DiscoveredLanguage::TypeScriptReact
            | DiscoveredLanguage::JavaScript
            | DiscoveredLanguage::JavaScriptReact
    )
}

fn sync_path_requires_full_project_context(path: &str) -> bool {
    if path.ends_with(".py")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".rs")
    {
        return true;
    }
    matches!(
        path,
        "package.json"
            | "tsconfig.json"
            | "jsconfig.json"
            | "jest.config.json"
            | "jest.config.cjs"
            | "jest.config.mjs"
            | "vitest.config.json"
            | "vitest.config.cjs"
            | "vitest.config.mjs"
            | "next.config.cjs"
            | "next.config.mjs"
            // Mocha runner configs are discovered as TsJsConfig at the repository
            // root (discovery.rs) and flip the global TS/JS test-runner flag in
            // the parser project context (tsjs_has_test_runner_context). Adding,
            // removing, or editing one changes how every TS/JS file is parsed, so
            // it must force a full rebuild. `.mocharc.js` is already covered by
            // the `.js` extension branch above; the remaining names are listed
            // here explicitly so the gate mirrors the runner-flag consumption.
            | ".mocharc.json"
            | ".mocharc.jsonc"
            | ".mocharc.cjs"
            | ".mocharc.yml"
            | ".mocharc.yaml"
            | "pyproject.toml"
            | "setup.cfg"
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
    family_store: Option<&'a dyn FamilyStoreWithProfiles>,
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
            && (matches!(
                file.path.as_str(),
                "jest.config.json" | "vitest.config.json"
            ) || file.path.starts_with(".mocharc."))
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
                    || dependencies.contains_key("mocha")
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
    // Union and deduplicate candidate source roots from every discovered root
    // Python config file. This is structural parser context, not a model of
    // setuptools precedence or evidence that can prove/suppress a family claim.
    // `pyproject.toml` (TOML), `setup.cfg` (INI), and `setup.py` (parsed with
    // `ast`, never executed) are parsed through the same sanitized
    // project-config parser; the worker dispatches on the file name.
    let mut roots = BTreeSet::new();
    for file in report.files.iter().filter(|file| {
        file.language == DiscoveredLanguage::PythonConfig
            && (file.path == "pyproject.toml"
                || file.path == "setup.cfg"
                || file.path == "setup.py")
    }) {
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
            Err(ParseError::UnsupportedLanguage) => continue,
            Err(ParseError::Timeout) => {
                return Err(RepoGrammarError::InvalidInput(
                    "parser timed out while reading Python project context".to_string(),
                ));
            }
            Err(ParseError::PythonFrontendContractMismatch) => {
                return Err(python_frontend_contract_mismatch_error());
            }
            Err(ParseError::Internal(_)) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "parser failed for {} source-root context: internal parser error",
                    file.path
                )));
            }
        };
        roots.extend(extract_python_source_roots_from_project_config_facts(
            &parse_report.semantic_facts,
        ));
    }
    Ok(roots.into_iter().collect())
}

fn extract_python_source_roots_from_project_config_facts(facts: &[SemanticFact]) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for fact in facts {
        let expected_method =
            python_project_config_parser_method(fact.evidence.provenance.path.as_str());
        if fact.kind != SemanticFactKind::ProjectConfig
            || fact.certainty != FactCertainty::Structural
            || fact.origin.engine != "python"
            || expected_method != Some(fact.origin.method.as_str())
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

/// Record the interface hash returned by the same parse request that produced a
/// just-parsed `.py` module (schema v10 `python_module_interfaces`), so a later
/// sync can decide whether an edit to it is file-local. Non-Python files and
/// parsers that return no verified hash store nothing; the next Python sync then
/// conservatively falls back through `python_interface_unverified`.
fn record_python_module_interface_if_python(
    session: &mut dyn GenerationWriteSession,
    file: &DiscoveredFile,
    interface_hash: Option<&str>,
) -> Result<(), RepoGrammarError> {
    if file.language != DiscoveredLanguage::Python {
        return Ok(());
    }
    if let Some(interface_hash) = interface_hash {
        session
            .record_python_module_interface(&file.path, interface_hash)
            .map_err(index_store_error)?;
    }
    Ok(())
}

fn record_parse_report(
    session: &mut dyn GenerationWriteSession,
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
        crate::application::storage::record_code_unit(session, &record)?;
        code_units.push(record);
        count += 1;
    }
    for node in &parse_report.ir_nodes {
        crate::application::storage::record_ir_node(
            session,
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
            session,
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

/// Records the recomputed family claims and returns their count plus the sorted
/// set of family ids recorded into `generation`, so callers can diff it against
/// the base generation for cross-generation identity reporting.
fn record_family_claims(
    session: &mut dyn GenerationWriteSession,
    code_units: &[IndexedCodeUnitRecord],
    framework_role_facts: &[SemanticFact],
) -> Result<(usize, BTreeSet<String>), RepoGrammarError> {
    let report = build_family_claims(code_units, framework_role_facts);
    let mut family_ids = BTreeSet::new();
    for claim in &report.claims {
        let records = family_storage_records(claim);
        crate::application::storage::record_family(session, &records.family)?;
        for member in &records.members {
            crate::application::storage::record_family_member(session, member)?;
        }
        for slot in &records.variation_slots {
            crate::application::storage::record_variation_slot(session, slot)?;
        }
        for evidence in &records.evidence {
            crate::application::storage::record_family_evidence(session, evidence)?;
        }
        // Persist the co-derived constraint profile alongside the family it
        // specifies, in the same generation, so query surfaces can hydrate the
        // source-backed implementation specification the claim already carries.
        crate::application::storage::record_family_constraint_profile(
            session,
            &family_constraint_profile_record(claim),
        )?;
        family_ids.insert(claim.family_id.clone());
    }
    Ok((report.claims.len(), family_ids))
}

/// The active generation's family-id set, captured before a new generation is
/// activated so it represents the base generation for identity diffing.
fn base_generation_family_ids(
    family_store: &dyn FamilyStoreWithProfiles,
) -> Result<BTreeSet<String>, RepoGrammarError> {
    Ok(
        crate::application::storage::list_active_families(family_store)?
            .families
            .into_iter()
            .map(|family| family.family_id)
            .collect(),
    )
}

fn record_rust_provider_facts(
    request: &IndexingRequest,
    code_units: &[IndexedCodeUnitRecord],
    rust_provider: Option<&dyn RustSemanticProvider>,
    session: &mut dyn GenerationWriteSession,
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
    record_semantic_facts(session, fact_id_offset, &facts)?;
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
        framework_support_blocked_units(code_units, parser_facts, &role_by_unit, |language| {
            language == "python"
        });
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

fn framework_support_blocked_units(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    language_matches: impl Fn(&str) -> bool,
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
        if !language_matches(&unit.language) || !parser_fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if family_unknown_blocks_claim(&unit.language, fact, framework_role) {
            blocked.insert(code_unit_id.to_string());
        }
    }
    blocked
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
            | ("django_model", "framework:django.model")
            | ("django_url_pattern", "framework:django.url_pattern")
            | ("django_test", "framework:django.test")
            | ("flask_route", "framework:flask.route")
            | ("unittest_test_method", "framework:unittest.test")
            | ("click_command", "framework:click.command")
            | ("typer_command", "framework:typer.command")
            | ("celery_task", "framework:celery.task")
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
    let blocked_units =
        framework_support_blocked_units(code_units, parser_facts, &role_by_unit, is_tsjs_language);
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
        if blocked_units.contains(code_unit_id) {
            continue;
        }
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TsJsProviderRequiredBinding {
    kind: String,
    import_specifier: String,
    export_name: String,
}

impl TsJsProviderRequiredBinding {
    fn literal_specifier(&self) -> String {
        format!("{}#{}", self.import_specifier, self.export_name)
    }
}

#[derive(Debug, Default)]
struct TsJsProviderRequiredBindingFields {
    kind: Option<String>,
    local_name: Option<String>,
    import_specifier: Option<String>,
    export_name: Option<String>,
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
    let required_bindings = tsjs_provider_required_bindings(fact);
    if !required_bindings.is_empty() {
        let mut first_worker_fact = None;
        let mut proof_literals = Vec::new();
        for binding in required_bindings {
            let literal_specifier = binding.literal_specifier();
            let proof_key = TsJsProviderBindingProofKey::from_fact(fact, &literal_specifier);
            let worker_fact = binding_proofs.get(&proof_key)?;
            if !tsjs_provider_resolved_binding_kind_matches(&binding.kind, worker_fact) {
                return None;
            }
            if first_worker_fact.is_none() {
                first_worker_fact = Some(worker_fact);
            }
            proof_literals.push(literal_specifier);
        }
        return Some((
            format!("bindings:{}", proof_literals.join(",")),
            first_worker_fact?,
        ));
    }
    None
}

fn tsjs_provider_required_bindings(fact: &SemanticFact) -> Vec<TsJsProviderRequiredBinding> {
    if !is_tsjs_structural_anchor_fact(fact) || !tsjs_provider_required_anchor_fact(fact) {
        return Vec::new();
    }
    let mut bindings = Vec::new();
    if let (Some(kind), Some(_local_name), Some(import_specifier), Some(export_name)) = (
        fact_assumption_value(fact, "binding_kind="),
        fact_assumption_value(fact, "binding_local_name="),
        fact_assumption_value(fact, "binding_import_specifier="),
        fact_assumption_value(fact, "binding_export_name="),
    ) {
        bindings.push(TsJsProviderRequiredBinding {
            kind: kind.to_string(),
            import_specifier: import_specifier.to_string(),
            export_name: export_name.to_string(),
        });
    }

    let mut fields_by_id: BTreeMap<String, TsJsProviderRequiredBindingFields> = BTreeMap::new();
    for assumption in &fact.assumptions {
        let Some(rest) = assumption.strip_prefix("binding:") else {
            continue;
        };
        let Some((binding_id, field_assignment)) = rest.split_once(':') else {
            continue;
        };
        if binding_id.is_empty() {
            continue;
        }
        let Some((field, value)) = field_assignment.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        let fields = fields_by_id.entry(binding_id.to_string()).or_default();
        match field {
            "kind" => fields.kind = Some(value.to_string()),
            "local_name" => fields.local_name = Some(value.to_string()),
            "import_specifier" => fields.import_specifier = Some(value.to_string()),
            "export_name" => fields.export_name = Some(value.to_string()),
            _ => {}
        }
    }
    for (_binding_id, fields) in fields_by_id {
        if let (Some(kind), Some(_local_name), Some(import_specifier), Some(export_name)) = (
            fields.kind,
            fields.local_name,
            fields.import_specifier,
            fields.export_name,
        ) {
            bindings.push(TsJsProviderRequiredBinding {
                kind,
                import_specifier,
                export_name,
            });
        }
    }
    bindings.sort();
    bindings.dedup();
    bindings
}

fn tsjs_provider_binding_operation_literals(fact: &SemanticFact) -> Vec<String> {
    tsjs_provider_required_bindings(fact)
        .iter()
        .map(TsJsProviderRequiredBinding::literal_specifier)
        .collect()
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
    binding_kind: &str,
    worker_fact: &SemanticFact,
) -> bool {
    match binding_kind {
        "prisma_client" => matches!(
            worker_fact.kind,
            SemanticFactKind::Symbol | SemanticFactKind::Type
        ),
        "tsjs_route_handler" | "drizzle_db" | "drizzle_table" => {
            matches!(worker_fact.kind, SemanticFactKind::Symbol)
        }
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
    let blocked_units =
        framework_support_blocked_units(code_units, parser_facts, &role_by_unit, |language| {
            language == "java"
        });
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
            .filter(|assumption| java::assumption_is_copied_to_support(assumption))
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

fn derive_csharp_framework_support_facts(
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
        framework_support_blocked_units(code_units, parser_facts, &role_by_unit, |language| {
            language == "csharp"
        });
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_csharp_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if unit.language != "csharp" || !parser_fact_evidence_is_within_unit(fact, unit) {
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
        if csharp_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_csharp_framework_support_fact(
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

fn is_csharp_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == CSHARP_ANCHOR_ENGINE
        && fact.origin.method == CSHARP_ANCHOR_METHOD
        && fact.target.is_some()
}

fn derived_csharp_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        "derived_from=tree_sitter_csharp_structural_anchors".to_string(),
        format!("framework_role={framework_role}"),
        format!(
            "derived_from={}",
            csharp::support_family(target, framework_role)
        ),
    ];
    assumptions.extend(
        source_fact
            .assumptions
            .iter()
            .filter(|assumption| {
                assumption.starts_with("csharp_anchor_kind=")
                    || assumption.starts_with("aspnet_attribute=")
                    || assumption.starts_with("http_method=")
                    || assumption.starts_with("route_template_shape=")
                    || assumption.starts_with("class_route_template_shape=")
                    || assumption.starts_with("test_attribute=")
                    || assumption.starts_with("test_data_shape=")
                    || assumption.starts_with("csharp_visibility_shape=")
                    || assumption.starts_with("csharp_class_shape=")
                    || assumption.starts_with("csharp_return_shape=")
                    || assumption.starts_with("csharp_parameter_shape=")
                    || assumption.starts_with("efcore_entity_type_shape=")
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
            engine: CSHARP_DERIVED_SUPPORT_ENGINE,
            method: CSHARP_DERIVED_SUPPORT_METHOD,
            note: "bounded C# framework structural role support",
            assumptions,
        },
    )
}

fn derive_cpp_framework_support_facts(
    code_units: &[IndexedCodeUnitRecord],
    parser_facts: &[SemanticFact],
    framework_role_facts: &[SemanticFact],
) -> Result<Vec<SemanticFact>, RepoGrammarError> {
    let unit_by_id = code_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let role_by_unit = framework_role_targets_by_unit(framework_role_facts);
    let blocked_units = framework_support_blocked_units(
        code_units,
        parser_facts,
        &role_by_unit,
        is_c_cpp_unit_language,
    );
    let mut seen = BTreeSet::new();
    let mut derived = Vec::new();

    for fact in parser_facts {
        if !is_cpp_structural_anchor_fact(fact) {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !is_c_cpp_unit_language(&unit.language)
            || !parser_fact_evidence_is_within_unit(fact, unit)
        {
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
        if cpp_support_target_is_role_compatible(target, framework_role) != Some(true) {
            continue;
        }
        if !seen.insert((unit.id.clone(), target.to_string())) {
            continue;
        }
        derived.push(derived_cpp_framework_support_fact(
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

fn is_c_cpp_unit_language(language: &str) -> bool {
    language == "c" || language == "cpp"
}

fn is_cpp_structural_anchor_fact(fact: &SemanticFact) -> bool {
    matches!(
        fact.kind,
        SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type
    ) && fact.certainty == FactCertainty::Structural
        && fact.origin.engine == CPP_ANCHOR_ENGINE
        && fact.origin.method == CPP_ANCHOR_METHOD
        && fact.target.is_some()
}

fn derived_cpp_framework_support_fact(
    unit: &IndexedCodeUnitRecord,
    kind: SemanticFactKind,
    target: &str,
    framework_role: &str,
    repository_revision: &RepositoryRevision,
    source_fact: &SemanticFact,
) -> Result<SemanticFact, RepoGrammarError> {
    let mut assumptions = vec![
        "provider_resolved=false".to_string(),
        "derived_from=tree_sitter_c_cpp_structural_anchors".to_string(),
        format!("framework_role={framework_role}"),
        format!(
            "derived_from={}",
            cpp::support_family(target, framework_role)
        ),
    ];
    assumptions.extend(
        source_fact
            .assumptions
            .iter()
            .filter(|assumption| {
                assumption.starts_with("cpp_anchor_kind=")
                    || assumption.starts_with("test_framework=")
                    || assumption.starts_with("test_macro=")
                    || assumption.starts_with("test_name_shape=")
                    || assumption.starts_with("fixture_shape=")
                    || assumption.starts_with("suite_shape=")
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
            engine: CPP_DERIVED_SUPPORT_ENGINE,
            method: CPP_DERIVED_SUPPORT_METHOD,
            note: "bounded C/C++ framework structural role support",
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
    let blocked_units =
        framework_support_blocked_units(code_units, parser_facts, &role_by_unit, |language| {
            language == "rust"
        });
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
                    || assumption.starts_with("serde_attr_shape=")
                    || assumption.starts_with("error_message_shape=")
                    || assumption.starts_with("clap_attr_shape=")
                    || assumption.starts_with("http_method=")
                    || assumption.starts_with("route_path_shape=")
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
    semantic_worker: Option<&'a dyn SemanticWorker>,
    fact_id_offset: usize,
}

fn record_semantic_worker_facts(
    input: SemanticWorkerFactRecording<'_>,
    session: &mut dyn GenerationWriteSession,
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
    record_semantic_facts(session, input.fact_id_offset, &facts)?;
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
        for literal_specifier in tsjs_provider_binding_operation_literals(fact) {
            push_tsjs_semantic_worker_operation(
                &mut operations,
                fact,
                SemanticWorkerOperationKind::ResolveReexport,
                &literal_specifier,
                &operation_context,
            );
        }
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

fn zero_content_hash() -> String {
    "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()
}

fn fact_assumption_value<'a>(fact: &'a SemanticFact, prefix: &str) -> Option<&'a str> {
    fact.assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix(prefix))
}

fn record_semantic_facts(
    session: &mut dyn GenerationWriteSession,
    fact_id_offset: usize,
    facts: &[SemanticFact],
) -> Result<usize, RepoGrammarError> {
    for (index, fact) in facts.iter().enumerate() {
        crate::application::storage::record_semantic_fact(
            session,
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
        DiscoveredLanguage::CSharp => Language::CSharp,
        DiscoveredLanguage::C => Language::C,
        DiscoveredLanguage::Cpp => Language::Cpp,
        DiscoveredLanguage::CppConfig => Language::CppConfig,
        DiscoveredLanguage::Go => Language::Go,
        DiscoveredLanguage::GoConfig => Language::GoConfig,
        DiscoveredLanguage::Php => Language::Php,
        DiscoveredLanguage::PhpConfig => Language::PhpConfig,
        DiscoveredLanguage::Ruby => Language::Ruby,
        DiscoveredLanguage::RubyConfig => Language::RubyConfig,
        DiscoveredLanguage::Swift => Language::Swift,
        DiscoveredLanguage::SwiftConfig => Language::SwiftConfig,
        DiscoveredLanguage::Rust => Language::Rust,
        DiscoveredLanguage::RustConfig => Language::RustConfig,
    }
}

fn discovered_language_is_inventory_only(language: DiscoveredLanguage) -> bool {
    language_token_is_inventory_only(language.as_str())
}

fn indexed_language_is_inventory_only(language: &str) -> bool {
    language_token_is_inventory_only(language)
}

fn language_token_is_inventory_only(language: &str) -> bool {
    matches!(
        language,
        "go" | "go-config"
            | "php"
            | "php-config"
            | "ruby"
            | "ruby-config"
            | "swift"
            | "swift-config"
    )
}

fn inventory_only_paths(report: &FileDiscoveryReport) -> BTreeSet<String> {
    report
        .files
        .iter()
        .filter(|file| discovered_language_is_inventory_only(file.language))
        .map(|file| file.path.clone())
        .collect()
}

fn indexing_generation_mode(report: &FileDiscoveryReport) -> IndexingGenerationMode {
    if report
        .files
        .iter()
        .all(|file| discovered_language_is_inventory_only(file.language))
    {
        IndexingGenerationMode::FileManifestOnly
    } else {
        IndexingGenerationMode::SyntaxOnlyCodeUnits
    }
}

fn extend_inventory_only_language_warnings(
    warnings: &mut Vec<String>,
    report: &FileDiscoveryReport,
) {
    let mut tokens = BTreeSet::new();
    for file in &report.files {
        if discovered_language_is_inventory_only(file.language) {
            tokens.insert(file.language.as_str());
        }
    }
    for token in tokens {
        let warning = format!("parser skipped unsupported language token: {token}");
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
}

fn discovery_error(error: FileDiscoveryError) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(error.to_string())
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

fn python_frontend_contract_mismatch_error() -> RepoGrammarError {
    RepoGrammarError::InvalidInput(
        "PythonFrontendContractMismatch: rebuild or reinstall RepoGrammar so the product binary and bundled Python worker come from the same release"
            .to_string(),
    )
}

fn index_store_error(error: IndexStoreError) -> RepoGrammarError {
    match error {
        IndexStoreError::Unavailable(message)
        | IndexStoreError::InvalidState(message)
        | IndexStoreError::InvalidRecord(message)
        | IndexStoreError::SchemaVersionOutdated(message) => {
            RepoGrammarError::InvalidInput(message)
        }
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
    use crate::ports::file_discovery::{
        FileDiscoveryLimitExceeded, FileDiscoveryLimitKind, GitIgnoreStatus,
    };
    use crate::ports::index_store::ActiveRepoShapeStats;
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

    struct RejectingSourceStore {
        calls: AtomicUsize,
    }

    impl RejectingSourceStore {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl SourceStore for RejectingSourceStore {
        fn read_source(&self, _request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(SourceStoreError::Unavailable(
                "inventory-only source read attempted".to_string(),
            ))
        }
    }

    struct RecordingSourceStore {
        paths: Mutex<Vec<String>>,
    }

    impl RecordingSourceStore {
        fn new() -> Self {
            Self {
                paths: Mutex::new(Vec::new()),
            }
        }

        fn paths(&self) -> Vec<String> {
            self.paths.lock().expect("read source paths").clone()
        }
    }

    impl SourceStore for RecordingSourceStore {
        fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
            self.paths
                .lock()
                .expect("record source path")
                .push(request.path.clone());
            FilesystemSourceStore.read_source(request)
        }
    }

    struct RecordingParser {
        paths: Mutex<Vec<String>>,
    }

    impl RecordingParser {
        fn new() -> Self {
            Self {
                paths: Mutex::new(Vec::new()),
            }
        }

        fn paths(&self) -> Vec<String> {
            self.paths.lock().expect("read parser paths").clone()
        }
    }

    impl SourceParser for RecordingParser {
        fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            self.paths
                .lock()
                .expect("record parser path")
                .push(document.path.to_string());
            RepoGrammarSourceParser::default().parse(document)
        }

        fn parse_with_context(
            &self,
            document: SourceDocument<'_>,
            context: &ParserProjectContext,
        ) -> Result<ParseReport, ParseError> {
            self.paths
                .lock()
                .expect("record parser path")
                .push(document.path.to_string());
            RepoGrammarSourceParser::default().parse_with_context(document, context)
        }
    }

    struct PythonContractMismatchParser;

    impl SourceParser for PythonContractMismatchParser {
        fn parse(&self, _document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            Err(ParseError::PythonFrontendContractMismatch)
        }
    }

    fn strict_hash(value: &str) -> ContentHash {
        ContentHash::new(value).expect("valid strict hash")
    }

    fn assert_active_pydantic_validator_evidence(store: &impl IndexStore) {
        let files = store
            .list_active_indexed_files()
            .expect("read active Pydantic fixture files");
        assert_eq!(files.files.len(), 1);
        let active_hash = files.files[0].content_hash.clone();
        let facts = store
            .list_active_semantic_facts()
            .expect("read active Pydantic fixture facts")
            .facts;
        assert!(facts.iter().any(|fact| {
            fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pydantic.field_validator")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_validator")
        }));
        assert!(facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("FrameworkMagic")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "affected_claim=pydantic_validator_side_effects"
                })
        }));
        assert!(facts.iter().all(|fact| {
            fact.path == "schemas.py"
                && fact.content_hash == active_hash
                && !fact.note.contains("return value.lower()")
        }));
    }

    #[test]
    fn python_frontend_contract_mismatch_has_sanitized_reinstall_recovery() {
        let workspace = TempWorkspace::new("indexing-python-contract-mismatch");
        let source = "def secret_value():\n    return 'must-not-leak'\n";
        fs::write(workspace.path().join("app.py"), source).expect("write Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let error = index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &PythonContractMismatchParser,
            &store,
        )
        .expect_err("mixed host/worker contract must abort indexing");

        assert_eq!(
            error.to_string(),
            "PythonFrontendContractMismatch: rebuild or reinstall RepoGrammar so the product binary and bundled Python worker come from the same release"
        );
        let rendered = error.to_string();
        assert!(!rendered.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!rendered.contains("must-not-leak"));
        assert!(!rendered.contains("worker.py"));
        assert!(!state.join("current-generation").exists());
    }

    #[test]
    fn python_indexing_persists_parse_hash_without_a_second_interface_probe() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingPythonParser {
            parse_requests: AtomicUsize,
            interface_probes: AtomicUsize,
        }

        impl SourceParser for CountingPythonParser {
            fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
                RepoGrammarSourceParser::default().parse(document)
            }

            fn parse_with_context_output(
                &self,
                document: SourceDocument<'_>,
                context: &ParserProjectContext,
            ) -> Result<SourceParseOutput, ParseError> {
                self.parse_requests.fetch_add(1, Ordering::SeqCst);
                RepoGrammarSourceParser::default().parse_with_context_output(document, context)
            }

            fn extract_python_interface(&self, path: &str, text: &str) -> PythonInterfaceProbe {
                self.interface_probes.fetch_add(1, Ordering::SeqCst);
                RepoGrammarSourceParser::default().extract_python_interface(path, text)
            }
        }

        let workspace = TempWorkspace::new("indexing-python-single-worker-request");
        fs::write(
            workspace.path().join("app.py"),
            "def current_tenant() -> str:\n    return \"default\"\n",
        )
        .expect("write Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = CountingPythonParser {
            parse_requests: AtomicUsize::new(0),
            interface_probes: AtomicUsize::new(0),
        };
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python source");
        assert_eq!(parser.parse_requests.load(Ordering::SeqCst), 1);
        assert_eq!(parser.interface_probes.load(Ordering::SeqCst), 0);
        let interfaces = store
            .active_python_module_interfaces()
            .expect("read stored Python interface hashes");
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].path, "app.py");
        assert!(interfaces[0].interface_hash.starts_with("sha256:"));

        fs::write(
            workspace.path().join("app.py"),
            "def current_tenant() -> str:\n    return \"primary\"\n",
        )
        .expect("edit Python function body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("incrementally sync Python body edit");
        let report = synced.sync_report.expect("Python sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.reparsed_files, 1);
        assert_eq!(parser.parse_requests.load(Ordering::SeqCst), 2);
        assert_eq!(
            parser.interface_probes.load(Ordering::SeqCst),
            1,
            "only the incremental preflight may launch extract_interface"
        );
    }

    #[test]
    fn committed_pydantic_fixture_survives_unchanged_sync_without_copy_forward() {
        let workspace = TempWorkspace::new("indexing-pydantic-contract-lifecycle");
        let source = include_str!("../../fixtures/python/release/v0_1/pydantic-basic/schemas.py");
        fs::write(workspace.path().join("schemas.py"), source)
            .expect("write committed Pydantic fixture");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let full = index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index committed Pydantic fixture");
        assert_eq!(full.discovered_files, 1);
        assert_eq!(full.parser_attempted_files, 1);
        let full_generation = full.active_generation.expect("full generation");
        assert_active_pydantic_validator_evidence(&store);

        let incremental = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync unchanged committed Pydantic fixture");
        assert_eq!(
            incremental.active_generation.as_deref(),
            Some(full_generation.as_str())
        );
        let report = incremental.sync_report.expect("incremental sync report");
        assert_eq!(
            report.base_generation.as_deref(),
            Some(full_generation.as_str())
        );
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.modified_files, 0);
        assert_eq!(report.unchanged_files, 1);
        assert_eq!(report.reparsed_files, 0);
        assert_eq!(report.copied_forward_files, 0);
        assert_eq!(report.families_recomputed, 0);
        assert_active_pydantic_validator_evidence(&store);
    }

    #[test]
    fn indexing_pipelines_checkpoint_at_phase_boundaries() {
        use std::sync::atomic::Ordering;
        // Both production build pipelines route every record through one write
        // session and commit + WAL-checkpoint at phase boundaries. This asserts,
        // through the store's real write instrumentation, that the full and
        // incremental pipelines each open exactly one connection and checkpoint
        // at the file/unit/IR and semantic-fact phase boundaries (the family
        // phase adds a third checkpoint when families recompute).
        let workspace = TempWorkspace::new("indexing-pipeline-checkpoints");
        let source = include_str!("../../fixtures/python/release/v0_1/pydantic-basic/schemas.py");
        fs::write(workspace.path().join("schemas.py"), source).expect("write fixture source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let instrumentation = store.write_instrumentation();
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("full index");
        let after_full = instrumentation.checkpoints.load(Ordering::Relaxed);
        assert!(
            after_full >= 2,
            "full pipeline must checkpoint at phase boundaries, saw {after_full}"
        );
        assert_eq!(instrumentation.connection_opens.load(Ordering::Relaxed), 1);

        fs::write(
            workspace.path().join("schemas.py"),
            format!("{source}\n# body-only change keeps the interface stable\n"),
        )
        .expect("edit fixture without changing its interface");

        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("incremental sync");
        assert_eq!(
            synced.sync_report.expect("sync report").sync_mode,
            IndexingSyncMode::Incremental
        );
        let after_sync = instrumentation.checkpoints.load(Ordering::Relaxed);
        assert!(
            after_sync >= after_full + 2,
            "incremental pipeline must checkpoint at phase boundaries, saw {} more",
            after_sync - after_full
        );
        assert_eq!(instrumentation.connection_opens.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn mocharc_edit_forces_full_rebuild_fallback() {
        // Regression: `.mocharc.*` runner configs are discovered as TsJsConfig
        // and flip the global TS/JS test-runner flag, but they were absent from
        // the incremental project-context gate. Editing one used to take the
        // incremental path, copying forward facts parsed under the old runner
        // flag while the changed file parsed under the new one. The edit must
        // now force a full rebuild.
        let workspace = TempWorkspace::new("indexing-mocharc-gate-sync");
        fs::write(workspace.path().join("app.ts"), "export const value = 1;\n")
            .expect("write TS source");
        fs::write(
            workspace.path().join(".mocharc.json"),
            "{\"spec\": \"test/**/*.spec.ts\"}\n",
        )
        .expect("write mocharc runner config");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index TS + mocharc fixture");

        fs::write(
            workspace.path().join(".mocharc.json"),
            "{\"spec\": \"spec/**/*.spec.ts\"}\n",
        )
        .expect("edit mocharc runner config");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after mocharc edit");
        let report = synced.sync_report.expect("mocharc sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("project_context_changed")
        );
        assert_eq!(report.modified_files, 1);
    }

    fn gate_discovered_file(path: &str, language: DiscoveredLanguage) -> DiscoveredFile {
        DiscoveredFile {
            path: path.to_string(),
            language,
            content_hash: crate::core::model::ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            size_bytes: 1,
        }
    }

    struct GateStubSourceStore;
    impl SourceStore for GateStubSourceStore {
        fn read_source(
            &self,
            request: SourceReadRequest,
        ) -> Result<crate::ports::source_store::SourceText, SourceStoreError> {
            Ok(crate::ports::source_store::SourceText {
                path: request.path,
                content_hash: request.expected_content_hash,
                text: String::new(),
            })
        }
    }

    struct GateStubParser {
        probes: BTreeMap<String, PythonInterfaceProbe>,
    }
    impl SourceParser for GateStubParser {
        fn parse(&self, _document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
            unreachable!("the context gate never calls parse")
        }
        fn extract_python_interface(&self, path: &str, _text: &str) -> PythonInterfaceProbe {
            self.probes
                .get(path)
                .cloned()
                .unwrap_or(PythonInterfaceProbe::Unverified)
        }
    }

    struct GateStubInterfaceStore {
        records: Vec<crate::ports::index_store::PythonModuleInterfaceRecord>,
    }
    impl PythonModuleInterfaceStore for GateStubInterfaceStore {
        fn active_python_module_interfaces(
            &self,
        ) -> Result<Vec<crate::ports::index_store::PythonModuleInterfaceRecord>, IndexStoreError>
        {
            Ok(self.records.clone())
        }
    }

    fn gate_interface_record(
        path: &str,
        hash: &str,
    ) -> crate::ports::index_store::PythonModuleInterfaceRecord {
        crate::ports::index_store::PythonModuleInterfaceRecord {
            path: path.to_string(),
            interface_hash: hash.to_string(),
        }
    }

    fn indexed_from_discovered(file: &DiscoveredFile) -> IndexedFileRecord {
        IndexedFileRecord {
            path: file.path.clone(),
            content_hash: file.content_hash.clone(),
            size_bytes: file.size_bytes,
            language: file.language.as_str().to_string(),
        }
    }

    fn run_gate(
        modified: Vec<DiscoveredFile>,
        added: Vec<DiscoveredFile>,
        base: Vec<crate::ports::index_store::PythonModuleInterfaceRecord>,
        probes: BTreeMap<String, PythonInterfaceProbe>,
    ) -> SyncContextDecision {
        // The synthetic manifests keep the Python context payload tiny (the stub
        // files default to 1 byte), so the budget gate is trivially safe and these
        // cases exercise the interface logic only. The near-cap regime is covered
        // separately by the `python_context_budget_*` tests.
        let current_files: Vec<DiscoveredFile> =
            modified.iter().chain(added.iter()).cloned().collect();
        let base_files: Vec<IndexedFileRecord> =
            modified.iter().map(indexed_from_discovered).collect();
        let delta = SyncDelta {
            added_files: added,
            modified_files: modified,
            removed_files: Vec::new(),
            unchanged_files: Vec::new(),
        };
        classify_sync_context_gate(
            &IndexingRequest::new("/repo"),
            &delta,
            &base_files,
            &current_files,
            &GateStubSourceStore,
            &GateStubParser { probes },
            &GateStubInterfaceStore { records: base },
        )
        .expect("gate classification")
    }

    fn probe_map(
        entries: &[(&str, PythonInterfaceProbe)],
    ) -> BTreeMap<String, PythonInterfaceProbe> {
        entries
            .iter()
            .map(|(path, probe)| ((*path).to_string(), probe.clone()))
            .collect()
    }

    #[test]
    fn gate_python_body_edit_stable_interface_is_incremental() {
        // A modified `.py` whose probed interface equals its stored base hash is a
        // body-only edit: file-local, incremental.
        let decision = run_gate(
            vec![gate_discovered_file("app.py", DiscoveredLanguage::Python)],
            Vec::new(),
            vec![gate_interface_record("app.py", "sha256:abc")],
            probe_map(&[(
                "app.py",
                PythonInterfaceProbe::Computed("sha256:abc".into()),
            )]),
        );
        assert!(matches!(decision, SyncContextDecision::Incremental(_)));
    }

    #[test]
    fn gate_python_interface_edit_falls_back_changed() {
        let decision = run_gate(
            vec![gate_discovered_file("app.py", DiscoveredLanguage::Python)],
            Vec::new(),
            vec![gate_interface_record("app.py", "sha256:abc")],
            probe_map(&[(
                "app.py",
                PythonInterfaceProbe::Computed("sha256:def".into()),
            )]),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("python_interface_changed")
        ));
    }

    #[test]
    fn gate_python_unverifiable_probe_falls_back_unverified() {
        let decision = run_gate(
            vec![gate_discovered_file("app.py", DiscoveredLanguage::Python)],
            Vec::new(),
            vec![gate_interface_record("app.py", "sha256:abc")],
            probe_map(&[("app.py", PythonInterfaceProbe::Unverified)]),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("python_interface_unverified")
        ));
    }

    #[test]
    fn gate_python_missing_base_hash_falls_back_unverified() {
        // No stored interface (base predates schema v10): cannot prove file-local.
        let decision = run_gate(
            vec![gate_discovered_file("app.py", DiscoveredLanguage::Python)],
            Vec::new(),
            Vec::new(),
            probe_map(&[(
                "app.py",
                PythonInterfaceProbe::Computed("sha256:abc".into()),
            )]),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("python_interface_unverified")
        ));
    }

    #[test]
    fn gate_modified_conftest_falls_back_without_probing_interface() {
        // A conftest edit is excluded from the interface fast path and forces a
        // full rebuild even though its interface hash would be probed as stable.
        // The stub parser would panic-return `Unverified` for an unmapped path, so
        // an `Incremental`/`unverified` outcome here would prove the conftest was
        // wrongly interface-probed; `project_context_changed` proves it was not.
        let decision = run_gate(
            vec![gate_discovered_file(
                "pkg/conftest.py",
                DiscoveredLanguage::Python,
            )],
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("project_context_changed")
        ));
    }

    #[test]
    fn gate_added_python_module_falls_back_project_context() {
        let decision = run_gate(
            Vec::new(),
            vec![gate_discovered_file(
                "new_module.py",
                DiscoveredLanguage::Python,
            )],
            Vec::new(),
            BTreeMap::new(),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("project_context_changed")
        ));
    }

    #[test]
    fn gate_mixed_python_body_and_rust_content_edit_is_incremental() {
        // A stable-interface `.py` edit plus a file-local Rust content edit are
        // both incremental; the Rust file never reaches the interface probe.
        let decision = run_gate(
            vec![
                gate_discovered_file("app.py", DiscoveredLanguage::Python),
                gate_discovered_file("lib.rs", DiscoveredLanguage::Rust),
            ],
            Vec::new(),
            vec![gate_interface_record("app.py", "sha256:abc")],
            probe_map(&[(
                "app.py",
                PythonInterfaceProbe::Computed("sha256:abc".into()),
            )]),
        );
        assert!(matches!(decision, SyncContextDecision::Incremental(_)));
    }

    #[test]
    fn gate_mixed_unverified_dominates_changed() {
        // With one unverifiable module and one provably-changed module, the
        // unverified reason wins (documented precedence); both force a full
        // rebuild.
        let decision = run_gate(
            vec![
                gate_discovered_file("a.py", DiscoveredLanguage::Python),
                gate_discovered_file("b.py", DiscoveredLanguage::Python),
            ],
            Vec::new(),
            vec![
                gate_interface_record("a.py", "sha256:aaa"),
                gate_interface_record("b.py", "sha256:bbb"),
            ],
            probe_map(&[
                ("a.py", PythonInterfaceProbe::Unverified),
                (
                    "b.py",
                    PythonInterfaceProbe::Computed("sha256:changed".into()),
                ),
            ]),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("python_interface_unverified")
        ));
    }

    fn gate_discovered_file_sized(
        path: &str,
        language: DiscoveredLanguage,
        size_bytes: u64,
    ) -> DiscoveredFile {
        DiscoveredFile {
            size_bytes,
            ..gate_discovered_file(path, language)
        }
    }

    #[test]
    fn gate_python_modification_over_context_budget_falls_back_before_probing() {
        // A Python module large enough to push the whole-project `parse_document`
        // context across the real 1 MiB cap forces a full rebuild on the budget
        // gate, before the interface is even probed (the stub parser would report a
        // matching hash, which must not win). This closes the context-omission
        // channel where a size change flips how sibling modules parse.
        let decision = run_gate(
            vec![gate_discovered_file_sized(
                "app.py",
                DiscoveredLanguage::Python,
                2 * 1024 * 1024,
            )],
            Vec::new(),
            vec![gate_interface_record("app.py", "sha256:abc")],
            probe_map(&[(
                "app.py",
                PythonInterfaceProbe::Computed("sha256:abc".into()),
            )]),
        );
        assert!(matches!(
            decision,
            SyncContextDecision::FullRebuild("python_context_budget")
        ));
    }

    fn budget_base(path: &str, size: u64) -> IndexedFileRecord {
        IndexedFileRecord {
            path: path.to_string(),
            content_hash: crate::core::model::ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            size_bytes: size,
            language: DiscoveredLanguage::Python.as_str().to_string(),
        }
    }

    #[test]
    fn python_context_budget_is_safe_under_and_over_an_injected_cap() {
        let cap = 300_000usize;
        // Both manifests under the cap after the 6x worst-case escaping headroom.
        assert!(python_context_budget_is_safe(
            &[budget_base("app.py", 20_000)],
            &[gate_discovered_file_sized(
                "app.py",
                DiscoveredLanguage::Python,
                20_000
            )],
            cap,
        ));
        // The current manifest grew past the cap: the current-regime parses could
        // drop context, so a size-stable copy-forward would diverge — unsafe.
        assert!(!python_context_budget_is_safe(
            &[budget_base("app.py", 20_000)],
            &[
                gate_discovered_file_sized("app.py", DiscoveredLanguage::Python, 20_000),
                gate_discovered_file_sized("extra.py", DiscoveredLanguage::Python, 20_000),
            ],
            cap,
        ));
        // Regime flip the other way: the base manifest was over the cap while the
        // current one is under — also unsafe (the copied base facts were parsed
        // context-omitted).
        assert!(!python_context_budget_is_safe(
            &[
                budget_base("app.py", 20_000),
                budget_base("extra.py", 20_000),
            ],
            &[gate_discovered_file_sized(
                "app.py",
                DiscoveredLanguage::Python,
                20_000
            )],
            cap,
        ));
    }

    #[test]
    fn python_context_budget_ignores_non_python_manifest_bytes() {
        // A large Rust or config file does not enter the Python context payload, so
        // it must not trip the Python budget.
        let cap = 300_000usize;
        assert!(python_context_budget_is_safe(
            &[budget_base("app.py", 10_000)],
            &[
                gate_discovered_file_sized("app.py", DiscoveredLanguage::Python, 10_000),
                gate_discovered_file_sized("huge.rs", DiscoveredLanguage::Rust, 10_000_000),
            ],
            cap,
        ));
    }

    #[test]
    fn python_context_budget_counts_conftest_text_twice() {
        // conftest text ships in both `module_files` and `conftest_files`, so a
        // conftest of a given size consumes more budget than a plain module of the
        // same size. At a cap tuned between the two (after the 6x escaping
        // headroom), the module is safe and the conftest is not.
        let cap = 700_000usize;
        assert!(python_context_budget_is_safe(
            &[budget_base("pkg/app.py", 50_000)],
            &[gate_discovered_file_sized(
                "pkg/app.py",
                DiscoveredLanguage::Python,
                50_000
            )],
            cap,
        ));
        assert!(!python_context_budget_is_safe(
            &[budget_base("pkg/conftest.py", 50_000)],
            &[gate_discovered_file_sized(
                "pkg/conftest.py",
                DiscoveredLanguage::Python,
                50_000
            )],
            cap,
        ));
    }

    #[test]
    fn python_context_budget_rejects_control_char_escape_worst_case() {
        // The size-only gate cannot read the bytes, so the headroom must bound the
        // worst-case escape (6x for a `\uXXXX` control character), not the ~2x of
        // ordinary source. A 150 KB module of control characters escapes to ~900 KB;
        // its raw estimate sits in `(cap/6, cap/2)`, so the retired 2x factor would
        // pass a 1.2 MB cap while the real request could approach 6x and cross it —
        // the exact silent-truncation hole the gate must fail closed on. The 6x
        // factor rejects it.
        let cap = 1_200_000usize;
        assert!(!python_context_budget_is_safe(
            &[budget_base("app.py", 150_000)],
            &[gate_discovered_file_sized(
                "app.py",
                DiscoveredLanguage::Python,
                150_000
            )],
            cap,
        ));
        // The same manifest under a cap generous enough to absorb the 6x headroom is
        // still admitted: the fix tightens the bound, it does not blanket-reject.
        assert!(python_context_budget_is_safe(
            &[budget_base("app.py", 150_000)],
            &[gate_discovered_file_sized(
                "app.py",
                DiscoveredLanguage::Python,
                150_000
            )],
            2_000_000,
        ));
    }

    #[test]
    fn language_is_file_local_source_covers_rust_and_tsjs_only() {
        for language in [
            DiscoveredLanguage::Rust,
            DiscoveredLanguage::TypeScript,
            DiscoveredLanguage::TypeScriptReact,
            DiscoveredLanguage::JavaScript,
            DiscoveredLanguage::JavaScriptReact,
        ] {
            assert!(
                language_is_file_local_source(language),
                "{}",
                language.as_str()
            );
        }
        // Python (cross-file module/conftest text) and every `*Config`
        // classification feed project-wide context and must never be treated as
        // file-local by the modified-file fast path.
        for language in [
            DiscoveredLanguage::Python,
            DiscoveredLanguage::PythonConfig,
            DiscoveredLanguage::TsJsConfig,
            DiscoveredLanguage::RustConfig,
            DiscoveredLanguage::Java,
            DiscoveredLanguage::CSharp,
        ] {
            assert!(
                !language_is_file_local_source(language),
                "{}",
                language.as_str()
            );
        }
    }

    #[test]
    fn content_only_rust_edit_takes_incremental_fast_path() {
        // A content-only edit of a Rust source file leaves `rust_module_paths`
        // and the nearest `Cargo.toml` byte-identical, so exactly the edited file
        // is reparsed on the incremental path instead of forcing a full rebuild.
        let workspace = TempWorkspace::new("indexing-rust-content-edit-fast-path");
        fs::write(
            workspace.path().join("compute.rs"),
            "fn compute() -> i32 {\n    1\n}\n",
        )
        .expect("write Rust source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Rust fixture");

        fs::write(
            workspace.path().join("compute.rs"),
            "fn compute() -> i32 {\n    2\n}\n",
        )
        .expect("edit Rust body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Rust content edit");
        let report = synced.sync_report.expect("Rust edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(report.modified_files, 1);
        assert_eq!(report.reparsed_files, 1);
    }

    #[test]
    fn content_only_typescript_edit_takes_incremental_fast_path() {
        // TS/JS parsing consumes only the discovered path set and root config, not
        // other files' text, so a content-only TS edit is file-local: one file is
        // reparsed and the sync stays incremental.
        let workspace = TempWorkspace::new("indexing-ts-content-edit-fast-path");
        fs::write(
            workspace.path().join("service.ts"),
            "export const value = 1;\n",
        )
        .expect("write TS source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index TS fixture");

        fs::write(
            workspace.path().join("service.ts"),
            "export const value = 2;\n",
        )
        .expect("edit TS body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after TS content edit");
        let report = synced.sync_report.expect("TS edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(report.modified_files, 1);
        assert_eq!(report.reparsed_files, 1);
    }

    const PY_APP_BODY_DEFAULT: &str =
        "def current_tenant() -> str:\n    return \"default\"\n\n\ndef list_ids() -> list[int]:\n    return []\n";

    #[test]
    fn python_body_edit_stable_interface_takes_incremental_fast_path() {
        // A Python function-body edit that leaves the module's top-level symbol
        // surface unchanged has a stable interface hash, so the preflight keeps it
        // on the file-local incremental path: only the edited module is reparsed
        // and its sibling module copies forward.
        let workspace = TempWorkspace::new("indexing-python-body-edit-fast-path");
        fs::write(workspace.path().join("app.py"), PY_APP_BODY_DEFAULT)
            .expect("write Python source");
        fs::write(
            workspace.path().join("helpers.py"),
            "def helper() -> int:\n    return 1\n",
        )
        .expect("write sibling Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python fixture");

        fs::write(
            workspace.path().join("app.py"),
            "def current_tenant() -> str:\n    return \"primary\"\n\n\ndef list_ids() -> list[int]:\n    return []\n",
        )
        .expect("edit Python body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Python body edit");
        let report = synced.sync_report.expect("Python body edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(report.modified_files, 1);
        assert_eq!(report.reparsed_files, 1);
    }

    #[test]
    fn python_interface_edit_forces_full_rebuild_fallback() {
        // Adding a top-level function changes the module's exported symbol surface,
        // so its interface hash changes and the preflight falls back to a full
        // rebuild with the specific `python_interface_changed` reason.
        let workspace = TempWorkspace::new("indexing-python-interface-edit-fallback");
        fs::write(workspace.path().join("app.py"), PY_APP_BODY_DEFAULT)
            .expect("write Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python fixture");

        fs::write(
            workspace.path().join("app.py"),
            "def current_tenant() -> str:\n    return \"default\"\n\n\ndef list_ids() -> list[int]:\n    return []\n\n\ndef added_public() -> int:\n    return 2\n",
        )
        .expect("add a top-level Python function");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Python interface edit");
        let report = synced
            .sync_report
            .expect("Python interface edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("python_interface_changed")
        );
    }

    #[test]
    fn python_conftest_edit_forces_full_rebuild_fallback() {
        // A `conftest.py` edit alters ancestor fixture context for its subtree,
        // which the module interface projection does not model, so it always forces
        // a full rebuild regardless of interface hash.
        let workspace = TempWorkspace::new("indexing-python-conftest-edit-fallback");
        fs::write(workspace.path().join("app.py"), PY_APP_BODY_DEFAULT)
            .expect("write Python source");
        fs::write(
            workspace.path().join("conftest.py"),
            "import pytest\n\n\n@pytest.fixture\ndef tenant() -> str:\n    return \"default\"\n",
        )
        .expect("write conftest");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python fixture with conftest");

        fs::write(
            workspace.path().join("conftest.py"),
            "import pytest\n\n\n@pytest.fixture\ndef tenant() -> str:\n    return \"primary\"\n",
        )
        .expect("edit conftest body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after conftest edit");
        let report = synced.sync_report.expect("conftest edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("project_context_changed")
        );
    }

    #[test]
    fn repeated_python_body_edits_stay_incremental() {
        // The interface hash is copied forward for unchanged modules and rewritten
        // for reparsed modules, so a second body edit still finds a stored base
        // hash and stays incremental — the fast path does not decay after one sync.
        let workspace = TempWorkspace::new("indexing-python-repeated-body-edit");
        fs::write(workspace.path().join("app.py"), PY_APP_BODY_DEFAULT)
            .expect("write Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python fixture");

        for body in ["one", "two"] {
            fs::write(
                workspace.path().join("app.py"),
                format!(
                    "def current_tenant() -> str:\n    return \"{body}\"\n\n\ndef list_ids() -> list[int]:\n    return []\n"
                ),
            )
            .expect("edit Python body");
            let synced = sync_repository_with_discovery_parser_frameworks_and_store(
                request(),
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &parser,
                &detector,
                &store,
            )
            .expect("sync after Python body edit");
            let report = synced.sync_report.expect("Python body edit sync report");
            assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
            assert_eq!(report.fallback_reason, None);
            assert_eq!(report.reparsed_files, 1);
        }
    }

    #[test]
    fn added_rust_source_forces_full_rebuild() {
        // Adding a Rust source file grows `rust_module_paths`, which can change how
        // other files' `mod` candidates resolve, so an add still forces a full
        // rebuild even though a content-only edit would not.
        let workspace = TempWorkspace::new("indexing-rust-add-fallback");
        fs::write(
            workspace.path().join("compute.rs"),
            "fn compute() -> i32 {\n    1\n}\n",
        )
        .expect("write Rust source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Rust fixture");

        fs::write(
            workspace.path().join("helper.rs"),
            "fn helper() -> i32 {\n    3\n}\n",
        )
        .expect("add Rust source");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Rust add");
        let report = synced.sync_report.expect("Rust add sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("project_context_changed")
        );
        assert_eq!(report.added_files, 1);
    }

    #[test]
    fn removed_rust_source_forces_full_rebuild() {
        // Removing a Rust source file shrinks `rust_module_paths`, so it forces a
        // full rebuild for the same reason an add does.
        let workspace = TempWorkspace::new("indexing-rust-remove-fallback");
        fs::write(
            workspace.path().join("compute.rs"),
            "fn compute() -> i32 {\n    1\n}\n",
        )
        .expect("write Rust source");
        fs::write(
            workspace.path().join("helper.rs"),
            "fn helper() -> i32 {\n    3\n}\n",
        )
        .expect("write second Rust source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Rust fixture");

        fs::remove_file(workspace.path().join("helper.rs")).expect("remove Rust source");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Rust remove");
        let report = synced.sync_report.expect("Rust remove sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("project_context_changed")
        );
        assert_eq!(report.removed_files, 1);
    }

    #[test]
    fn python_stable_interface_content_edit_takes_incremental_fast_path() {
        // S2b narrows the S2 rule that any Python edit forces a full rebuild: a
        // content-only edit whose module interface projection (top-level symbols,
        // `__all__`, `__init__` re-exports) is unchanged is provably file-local,
        // because the interface is the only channel by which the module's text
        // reaches another file's parse. The same edit that S2 rebuilt fully now
        // stays incremental under the interface gate.
        let workspace = TempWorkspace::new("indexing-python-content-edit-incremental");
        fs::write(
            workspace.path().join("app.py"),
            "def value():\n    return \"default\"\n",
        )
        .expect("write Python source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Python fixture");

        fs::write(
            workspace.path().join("app.py"),
            "def value():\n    return \"primary\"\n",
        )
        .expect("edit Python body");
        let synced = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("sync after Python content edit");
        let report = synced.sync_report.expect("Python edit sync report");
        assert_eq!(report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(report.fallback_reason, None);
        assert_eq!(report.modified_files, 1);
        assert_eq!(report.reparsed_files, 1);
    }

    #[test]
    fn sync_after_engine_version_change_forces_full_rebuild_fallback() {
        // Regression: preflight never compared the base generation's producing
        // engine version against the running binary, so after an upgrade a delta
        // that avoids the project-context gate (here: a no-op) copied forward
        // facts produced by the older engine and relabeled them with the new
        // version. A matching version must stay incremental; a mismatch must
        // force a full rebuild.
        let workspace = TempWorkspace::new("indexing-engine-version-gate");
        fs::write(
            workspace.path().join("App.java"),
            "class App { void run() {} }\n",
        )
        .expect("write Java source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RepoGrammarSourceParser::default();
        let detector = SyntaxFrameworkRoleDetector;
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("fully index Java fixture");

        // Same engine version: an unchanged repository stays on the incremental
        // path.
        let same_version = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("no-op sync with matching engine version");
        let same_report = same_version.sync_report.expect("same-version sync report");
        assert_eq!(same_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(same_report.fallback_reason, None);

        // Simulate an engine upgrade by rewriting the stored producing version on
        // the now-active generation.
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .execute(
                "UPDATE index_generations SET repogrammar_version = ?1 WHERE status = 'active'",
                params!["0.0.0-older-engine"],
            )
            .expect("rewrite stored engine version");
        drop(connection);

        let after_upgrade = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &detector,
            &store,
        )
        .expect("no-op sync after simulated engine upgrade");
        let upgrade_report = after_upgrade.sync_report.expect("post-upgrade sync report");
        assert_eq!(
            upgrade_report.sync_mode,
            IndexingSyncMode::FullRebuildFallback
        );
        assert_eq!(
            upgrade_report.fallback_reason.as_deref(),
            Some("engine_version_changed")
        );
    }

    #[test]
    fn sync_project_context_gate_covers_root_and_nested_config_paths() {
        for path in [
            "package.json",
            "tsconfig.json",
            "jsconfig.json",
            "jest.config.json",
            "jest.config.cjs",
            "jest.config.mjs",
            "vitest.config.json",
            "vitest.config.cjs",
            "vitest.config.mjs",
            "next.config.cjs",
            "next.config.mjs",
            ".mocharc.json",
            ".mocharc.jsonc",
            ".mocharc.js",
            ".mocharc.cjs",
            ".mocharc.yml",
            ".mocharc.yaml",
            "pyproject.toml",
            "src/app.py",
            "src/conftest_helper.py",
            "src/app.ts",
            "src/app.tsx",
            "src/app.js",
            "src/app.jsx",
            "src/lib.rs",
            "Cargo.toml",
            "Cargo.lock",
            "conftest.py",
            "tests/conftest.py",
            "crates/demo/Cargo.toml",
            "crates/demo/Cargo.lock",
        ] {
            assert!(sync_path_requires_full_project_context(path), "{path}");
        }
        for path in [
            "Cargo.locked",
            "docs/Cargo.toml.md",
            // Mocha runner configs are discovered only at the repository root, so
            // a nested lookalike is an ordinary undiscovered file, not a gate hit.
            "packages/app/.mocharc.json",
            "docs/.mocharc.yaml.md",
            "src/main.go",
            "go.mod",
            "go.work",
            "modules/demo/go.mod",
            "workspaces/demo/go.work",
            "go.mod.bak",
            "docs/go.work.md",
            "main.rb",
            "Gemfile",
            "Gemfile.lock",
            "gems.rb",
            "gems.locked",
            ".ruby-version",
            "nested/example.gemspec",
            "main.swift",
            "Package.swift",
            "Package.resolved",
            ".swift-version",
            "nested/Package@swift-6.3.3.swift",
        ] {
            assert!(!sync_path_requires_full_project_context(path), "{path}");
        }
    }

    #[test]
    fn default_index_persists_source_free_go_inventory_without_claim_inputs() {
        let workspace = TempWorkspace::new("indexing-go-discovery-only");
        fs::create_dir_all(workspace.path().join("pkg")).expect("create Go package");
        fs::write(
            workspace.path().join("go.mod"),
            "module example.test/secret-module\n",
        )
        .expect("write go.mod");
        fs::write(workspace.path().join("go.work"), "go 1.25\n").expect("write go.work");
        fs::write(workspace.path().join("pkg/main.go"), [0xff, 0xfe, 0xfd])
            .expect("write Go source");
        fs::write(
            workspace.path().join("pkg/main_test.go"),
            "package pkg\nfunc TestMainShape() {}\n",
        )
        .expect("write Go test");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RejectingSourceStore::new();
        let mut progress_events = Vec::new();
        let mut progress = |event: ProgressEvent| progress_events.push(event);

        let outcome =
            index_repository_with_discovery_parser_frameworks_families_and_store_with_progress(
                IndexingRequest::new(workspace.path().display().to_string()),
                &FilesystemFileDiscovery,
                &source_store,
                &RepoGrammarSourceParser::default(),
                &detector,
                &store,
                &mut progress,
            )
            .expect("index Go inventory");

        assert_eq!(outcome.discovered_files, 4);
        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            outcome.warnings,
            vec![
                "parser skipped unsupported language token: go".to_string(),
                "parser skipped unsupported language token: go-config".to_string(),
            ]
        );
        assert!(progress_events
            .iter()
            .any(|event| event.message == "deferred inventory-only files"));
        assert!(!progress_events
            .iter()
            .any(|event| event.message == "parsed source files"));
        assert!(!format!("{outcome:?}").contains("secret-module"));
        assert!(!format!("{outcome:?}").contains(workspace.path().to_string_lossy().as_ref()));

        let files = store
            .list_active_indexed_files()
            .expect("read persisted Go inventory");
        assert_eq!(
            files
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("go.mod", "go-config"),
                ("go.work", "go-config"),
                ("pkg/main.go", "go"),
                ("pkg/main_test.go", "go"),
            ]
        );
        let files_debug = format!("{files:?}");
        assert!(!files_debug.contains("secret-module"));
        assert!(!files_debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(store
            .list_active_code_units()
            .expect("read Go units")
            .units
            .is_empty());
        assert!(store
            .list_active_semantic_facts()
            .expect("read Go facts")
            .facts
            .is_empty());
        assert!(store
            .list_active_families()
            .expect("read Go families")
            .families
            .is_empty());
    }

    #[test]
    fn default_index_persists_source_free_ruby_inventory_without_claim_inputs() {
        let workspace = TempWorkspace::new("indexing-ruby-discovery-only");
        fs::create_dir_all(workspace.path().join("gems")).expect("create Ruby config dir");
        fs::create_dir_all(workspace.path().join("lib")).expect("create Ruby source dir");
        fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://must-not-be-read.invalid'\n",
        )
        .expect("write Gemfile");
        fs::write(
            workspace.path().join("gems.rb"),
            "raise 'must not execute'\n",
        )
        .expect("write gems.rb");
        fs::write(
            workspace.path().join("gems/example.gemspec"),
            "raise 'must not execute'\n",
        )
        .expect("write gemspec");
        fs::write(workspace.path().join("lib/main.rb"), [0xff, 0xfe, 0xfd])
            .expect("write binary Ruby source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index Ruby inventory");

        assert_eq!(outcome.discovered_files, 4);
        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            outcome.warnings,
            vec![
                "parser skipped unsupported language token: ruby".to_string(),
                "parser skipped unsupported language token: ruby-config".to_string(),
            ]
        );

        let files = store
            .list_active_indexed_files()
            .expect("read persisted Ruby inventory");
        assert_eq!(
            files
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Gemfile", "ruby-config"),
                ("gems.rb", "ruby-config"),
                ("gems/example.gemspec", "ruby-config"),
                ("lib/main.rb", "ruby"),
            ]
        );
        assert!(store
            .list_active_code_units()
            .expect("read Ruby units")
            .units
            .is_empty());
        assert!(store
            .list_active_ir_graph()
            .expect("read Ruby IR")
            .nodes
            .is_empty());
        assert!(store
            .list_active_semantic_facts()
            .expect("read Ruby facts")
            .facts
            .is_empty());
        assert!(store
            .list_active_families()
            .expect("read Ruby families")
            .families
            .is_empty());
        let debug = format!("{outcome:?}{files:?}");
        assert!(!debug.contains("must-not-be-read"));
        assert!(!debug.contains("must not execute"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn default_index_persists_source_free_php_inventory_without_claim_inputs() {
        let workspace = TempWorkspace::new("indexing-php-discovery-only");
        fs::create_dir_all(workspace.path().join("src")).expect("create PHP source dir");
        fs::write(
            workspace.path().join("composer.json"),
            "must-not-be-decoded-or-evaluated",
        )
        .expect("write Composer manifest");
        fs::write(
            workspace.path().join("composer.lock"),
            "must-not-be-decoded-or-evaluated",
        )
        .expect("write Composer lock");
        fs::write(
            workspace.path().join("phpunit.xml"),
            "must-not-be-decoded-or-evaluated",
        )
        .expect("write PHPUnit config");
        fs::write(workspace.path().join("src/main.php"), [0xff, 0xfe, 0xfd])
            .expect("write binary PHP source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index PHP inventory");

        assert_eq!(outcome.discovered_files, 4);
        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            outcome.warnings,
            vec![
                "parser skipped unsupported language token: php".to_string(),
                "parser skipped unsupported language token: php-config".to_string(),
            ]
        );

        let files = store
            .list_active_indexed_files()
            .expect("read persisted PHP inventory");
        assert_eq!(
            files
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("composer.json", "php-config"),
                ("composer.lock", "php-config"),
                ("phpunit.xml", "php-config"),
                ("src/main.php", "php"),
            ]
        );
        assert!(store
            .list_active_code_units()
            .expect("read PHP units")
            .units
            .is_empty());
        assert!(store
            .list_active_ir_graph()
            .expect("read PHP IR")
            .nodes
            .is_empty());
        assert!(store
            .list_active_semantic_facts()
            .expect("read PHP facts")
            .facts
            .is_empty());
        assert!(store
            .list_active_families()
            .expect("read PHP families")
            .families
            .is_empty());
        let debug = format!("{outcome:?}{files:?}");
        assert!(!debug.contains("must-not-be-decoded-or-evaluated"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn default_index_persists_source_free_swift_inventory_without_claim_inputs() {
        let workspace = TempWorkspace::new("indexing-swift-discovery-only");
        fs::create_dir_all(workspace.path().join("Sources/App")).expect("create Swift source dir");
        fs::create_dir_all(workspace.path().join("nested"))
            .expect("create nested Swift package dir");
        fs::write(
            workspace.path().join("Package.swift"),
            "must-not-be-decoded-or-evaluated",
        )
        .expect("write Swift package manifest");
        fs::write(
            workspace.path().join("Package.resolved"),
            "must-not-be-decoded-or-evaluated",
        )
        .expect("write Swift package lock");
        fs::write(
            workspace.path().join(".swift-version"),
            "must-not-select-a-toolchain",
        )
        .expect("write Swift version selector");
        fs::write(
            workspace.path().join("nested/Package@swift-6.3.3.swift"),
            "must-not-select-a-manifest",
        )
        .expect("write version-specific Swift manifest");
        fs::write(
            workspace.path().join("Sources/App/main.swift"),
            [0xff, 0xfe, 0xfd],
        )
        .expect("write binary Swift source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index Swift inventory");

        assert_eq!(outcome.discovered_files, 5);
        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            outcome.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );

        let files = store
            .list_active_indexed_files()
            .expect("read persisted Swift inventory");
        assert_eq!(
            files
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (".swift-version", "swift-config"),
                ("Package.resolved", "swift-config"),
                ("Package.swift", "swift-config"),
                ("Sources/App/main.swift", "swift"),
                ("nested/Package@swift-6.3.3.swift", "swift-config"),
            ]
        );
        assert!(store
            .list_active_code_units()
            .expect("read Swift units")
            .units
            .is_empty());
        assert!(store
            .list_active_ir_graph()
            .expect("read Swift IR")
            .nodes
            .is_empty());
        assert!(store
            .list_active_semantic_facts()
            .expect("read Swift facts")
            .facts
            .is_empty());
        assert!(store
            .list_active_families()
            .expect("read Swift families")
            .families
            .is_empty());
        let debug = format!("{outcome:?}{files:?}");
        assert!(!debug.contains("must-not-be-decoded-or-evaluated"));
        assert!(!debug.contains("must-not-select-a-toolchain"));
        assert!(!debug.contains("must-not-select-a-manifest"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn empty_repository_generation_is_file_manifest_only_without_parser_attempts() {
        let workspace = TempWorkspace::new("indexing-empty-file-manifest");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("index empty repository");

        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(outcome.indexed_units, 0);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn go_only_first_sync_reports_file_manifest_and_zero_reparsed_files() {
        let workspace = TempWorkspace::new("indexing-go-first-sync");
        fs::write(workspace.path().join("main.go"), "package demo\n").expect("write Go source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RejectingSourceStore::new();

        let outcome = sync_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &detector,
            &store,
        )
        .expect("first Go sync");

        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        let report = outcome.sync_report.expect("sync fallback report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("missing_active_generation")
        );
        assert_eq!(report.added_files, 1);
        assert_eq!(report.reparsed_files, 0);
    }

    #[test]
    fn ruby_only_first_sync_reports_file_manifest_and_zero_reparsed_files() {
        let workspace = TempWorkspace::new("indexing-ruby-first-sync");
        fs::write(workspace.path().join("main.rb"), "puts :inventory\n")
            .expect("write Ruby source");
        fs::write(workspace.path().join("Gemfile"), "source 'unused'\n")
            .expect("write Ruby config");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = sync_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("first Ruby sync");

        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        let report = outcome.sync_report.expect("sync fallback report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("missing_active_generation")
        );
        assert_eq!(report.added_files, 2);
        assert_eq!(report.reparsed_files, 0);
    }

    #[test]
    fn php_only_first_sync_reports_file_manifest_and_zero_reparsed_files() {
        let workspace = TempWorkspace::new("indexing-php-first-sync");
        fs::write(workspace.path().join("main.php"), "<?php\n").expect("write PHP source");
        fs::write(workspace.path().join("composer.json"), "not-json\n").expect("write PHP config");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();

        let outcome = sync_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("first PHP sync");

        assert_eq!(
            outcome.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(outcome.parser_attempted_files, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        let report = outcome.sync_report.expect("sync fallback report");
        assert_eq!(report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("missing_active_generation")
        );
        assert_eq!(report.added_files, 2);
        assert_eq!(report.reparsed_files, 0);
    }

    #[test]
    fn swift_only_first_and_unchanged_sync_stay_file_manifest_only() {
        let workspace = TempWorkspace::new("indexing-swift-first-unchanged-sync");
        fs::write(workspace.path().join("main.swift"), "fatalError()\n")
            .expect("write Swift source");
        fs::write(workspace.path().join("Package.swift"), "not-evaluated\n")
            .expect("write Swift config");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let source_store = RejectingSourceStore::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let first = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("first Swift sync");

        assert_eq!(
            first.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(first.parser_attempted_files, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            first.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );
        let first_report = first.sync_report.expect("first Swift sync report");
        assert_eq!(
            first_report.sync_mode,
            IndexingSyncMode::FullRebuildFallback
        );
        assert_eq!(
            first_report.fallback_reason.as_deref(),
            Some("missing_active_generation")
        );
        assert_eq!(first_report.added_files, 2);
        assert_eq!(first_report.reparsed_files, 0);

        let unchanged = sync_repository_with_discovery_parser_frameworks_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &RepoGrammarSourceParser::default(),
            &SyntaxFrameworkRoleDetector,
            &store,
        )
        .expect("unchanged Swift sync");

        assert_eq!(
            unchanged.indexing_mode,
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(unchanged.parser_attempted_files, 0);
        assert_eq!(source_store.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            unchanged.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );
        let unchanged_report = unchanged.sync_report.expect("unchanged Swift sync report");
        assert_eq!(unchanged_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(unchanged_report.added_files, 0);
        assert_eq!(unchanged_report.modified_files, 0);
        assert_eq!(unchanged_report.removed_files, 0);
        assert_eq!(unchanged_report.reparsed_files, 0);
    }

    #[test]
    fn mixed_repo_go_inventory_deltas_stay_incremental_and_preserve_non_go_claims() {
        let workspace = TempWorkspace::new("indexing-go-incremental-inventory");
        fs::write(
            workspace.path().join("server.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n\
             app.post('/users', (req, res) => { res.json({}); });\n\
             app.delete('/users/:id', (req, res) => { res.json({}); });\n",
        )
        .expect("write stable TypeScript family source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let initial = index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index non-Go family");
        assert_eq!(
            initial.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(initial.parser_attempted_files, 1);
        let expected_facts = store
            .list_active_semantic_facts()
            .expect("list initial facts")
            .facts;
        let expected_families = store
            .list_active_families()
            .expect("list initial families")
            .families;
        assert_eq!(expected_families.len(), 1);

        fs::write(workspace.path().join("main.go"), "package demo\n").expect("add Go source");
        fs::write(
            workspace.path().join("go.mod"),
            "module example.test/demo\n",
        )
        .expect("add go.mod");
        fs::write(workspace.path().join("go.work"), "go 1.26\n").expect("add go.work");
        let added = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync added Go inventory");
        assert_eq!(
            added.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(added.parser_attempted_files, 0);
        let added_report = added.sync_report.expect("added sync report");
        assert_eq!(added_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(added_report.added_files, 3);
        assert_eq!(added_report.modified_files, 0);
        assert_eq!(added_report.removed_files, 0);
        assert_eq!(added_report.reparsed_files, 0);
        assert_eq!(
            added.warnings,
            vec![
                "parser skipped unsupported language token: go".to_string(),
                "parser skipped unsupported language token: go-config".to_string(),
            ]
        );
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list added files")
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["go.mod", "go.work", "main.go", "server.ts"]
        );
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after add")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after add")
                .families,
            expected_families
        );

        let unchanged = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync unchanged Go inventory");
        assert_eq!(
            unchanged.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(unchanged.parser_attempted_files, 0);
        let unchanged_report = unchanged
            .sync_report
            .as_ref()
            .expect("unchanged sync report");
        assert_eq!(unchanged_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(unchanged_report.reparsed_files, 0);
        assert_eq!(
            unchanged.warnings,
            vec![
                "parser skipped unsupported language token: go".to_string(),
                "parser skipped unsupported language token: go-config".to_string(),
            ]
        );

        fs::write(
            workspace.path().join("main.go"),
            "package demo\n// changed\n",
        )
        .expect("modify Go source");
        fs::write(
            workspace.path().join("go.mod"),
            "module example.test/demo\n\ngo 1.26\n",
        )
        .expect("modify go.mod");
        fs::write(workspace.path().join("go.work"), "go 1.26\nuse ./module\n")
            .expect("modify go.work");
        let modified = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync modified Go inventory");
        assert_eq!(
            modified.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(modified.parser_attempted_files, 0);
        let modified_report = modified.sync_report.expect("modified sync report");
        assert_eq!(modified_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(modified_report.added_files, 0);
        assert_eq!(modified_report.modified_files, 3);
        assert_eq!(modified_report.removed_files, 0);
        assert_eq!(modified_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after modify")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after modify")
                .families,
            expected_families
        );

        fs::remove_file(workspace.path().join("main.go")).expect("remove Go source");
        fs::remove_file(workspace.path().join("go.mod")).expect("remove go.mod");
        fs::remove_file(workspace.path().join("go.work")).expect("remove go.work");
        let removed = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync removed Go inventory");
        assert_eq!(
            removed.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(removed.parser_attempted_files, 0);
        assert!(removed.warnings.is_empty());
        let removed_report = removed.sync_report.expect("removed sync report");
        assert_eq!(removed_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(removed_report.added_files, 0);
        assert_eq!(removed_report.modified_files, 0);
        assert_eq!(removed_report.removed_files, 3);
        assert_eq!(removed_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list remaining files")
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["server.ts"]
        );
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after remove")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after remove")
                .families,
            expected_families
        );

        for path in source_store.paths() {
            assert!(
                !indexed_language_path_is_go(&path),
                "source read Go path {path}"
            );
        }
        for path in parser.paths() {
            assert!(
                !indexed_language_path_is_go(&path),
                "parser saw Go path {path}"
            );
        }
    }

    #[test]
    fn mixed_repo_ruby_inventory_deltas_stay_incremental_and_preserve_non_ruby_claims() {
        let workspace = TempWorkspace::new("indexing-ruby-incremental-inventory");
        fs::write(
            workspace.path().join("server.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n\
             app.post('/users', (req, res) => { res.json({}); });\n\
             app.delete('/users/:id', (req, res) => { res.json({}); });\n",
        )
        .expect("write stable TypeScript family source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let initial = index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index non-Ruby family");
        assert_eq!(
            initial.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(initial.parser_attempted_files, 1);
        let expected_facts = store
            .list_active_semantic_facts()
            .expect("list initial facts")
            .facts;
        let expected_families = store
            .list_active_families()
            .expect("list initial families")
            .families;
        assert_eq!(expected_families.len(), 1);

        fs::write(workspace.path().join("main.rb"), "puts :inventory\n").expect("add Ruby source");
        fs::write(workspace.path().join("Gemfile"), "source 'unused'\n").expect("add Gemfile");
        fs::write(workspace.path().join("gems.rb"), "source 'unused'\n").expect("add gems.rb");
        let added = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync added Ruby inventory");
        assert_eq!(
            added.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(added.parser_attempted_files, 0);
        let added_report = added.sync_report.expect("added sync report");
        assert_eq!(added_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(added_report.added_files, 3);
        assert_eq!(added_report.modified_files, 0);
        assert_eq!(added_report.removed_files, 0);
        assert_eq!(added_report.reparsed_files, 0);
        assert_eq!(
            added.warnings,
            vec![
                "parser skipped unsupported language token: ruby".to_string(),
                "parser skipped unsupported language token: ruby-config".to_string(),
            ]
        );
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after Ruby add")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after Ruby add")
                .families,
            expected_families
        );

        let unchanged = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync unchanged Ruby inventory");
        assert_eq!(unchanged.parser_attempted_files, 0);
        let unchanged_report = unchanged.sync_report.expect("unchanged sync report");
        assert_eq!(unchanged_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(unchanged_report.added_files, 0);
        assert_eq!(unchanged_report.modified_files, 0);
        assert_eq!(unchanged_report.removed_files, 0);
        assert_eq!(unchanged_report.reparsed_files, 0);

        fs::write(workspace.path().join("main.rb"), "puts :changed\n").expect("modify Ruby source");
        fs::write(workspace.path().join("Gemfile"), "source 'changed'\n").expect("modify Gemfile");
        fs::write(workspace.path().join("gems.rb"), "source 'changed'\n").expect("modify gems.rb");
        let modified = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync modified Ruby inventory");
        assert_eq!(modified.parser_attempted_files, 0);
        let modified_report = modified.sync_report.expect("modified sync report");
        assert_eq!(modified_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(modified_report.added_files, 0);
        assert_eq!(modified_report.modified_files, 3);
        assert_eq!(modified_report.removed_files, 0);
        assert_eq!(modified_report.reparsed_files, 0);

        fs::remove_file(workspace.path().join("main.rb")).expect("remove Ruby source inventory");
        let source_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed Ruby source inventory");
        assert_eq!(source_removed.parser_attempted_files, 0);
        assert_eq!(
            source_removed.warnings,
            vec!["parser skipped unsupported language token: ruby-config".to_string()]
        );
        let source_removed_report = source_removed
            .sync_report
            .expect("source removal sync report");
        assert_eq!(
            source_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(source_removed_report.added_files, 0);
        assert_eq!(source_removed_report.modified_files, 0);
        assert_eq!(source_removed_report.removed_files, 1);
        assert_eq!(source_removed_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_indexed_files()
                .expect("list files after Ruby source removal")
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["Gemfile", "gems.rb", "server.ts"]
        );

        for path in ["Gemfile", "gems.rb"] {
            fs::remove_file(workspace.path().join(path)).expect("remove Ruby config inventory");
        }
        let configs_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed Ruby config inventory");
        assert_eq!(configs_removed.parser_attempted_files, 0);
        assert!(configs_removed.warnings.is_empty());
        let configs_removed_report = configs_removed
            .sync_report
            .expect("config removal sync report");
        assert_eq!(
            configs_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(configs_removed_report.added_files, 0);
        assert_eq!(configs_removed_report.modified_files, 0);
        assert_eq!(configs_removed_report.removed_files, 2);
        assert_eq!(configs_removed_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after Ruby removal")
                .families,
            expected_families
        );
        assert_eq!(source_store.paths(), vec!["server.ts".to_string()]);
        assert_eq!(parser.paths(), vec!["server.ts".to_string()]);
    }

    #[test]
    fn mixed_repo_php_inventory_deltas_stay_incremental_and_preserve_non_php_claims() {
        let workspace = TempWorkspace::new("indexing-php-incremental-inventory");
        fs::write(
            workspace.path().join("server.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n\
             app.post('/users', (req, res) => { res.json({}); });\n\
             app.delete('/users/:id', (req, res) => { res.json({}); });\n",
        )
        .expect("write stable TypeScript family source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let initial = index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index non-PHP family");
        assert_eq!(
            initial.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(initial.parser_attempted_files, 1);
        let expected_facts = store
            .list_active_semantic_facts()
            .expect("list initial facts")
            .facts;
        let expected_families = store
            .list_active_families()
            .expect("list initial families")
            .families;
        assert_eq!(expected_families.len(), 1);

        fs::write(workspace.path().join("main.php"), "<?php\n").expect("add PHP source");
        fs::write(workspace.path().join("composer.json"), "not-json\n")
            .expect("add Composer config");
        fs::write(workspace.path().join("phpunit.xml"), "not-xml\n").expect("add PHPUnit config");
        let added = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync added PHP inventory");
        assert_eq!(
            added.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(added.parser_attempted_files, 0);
        let added_report = added.sync_report.expect("added sync report");
        assert_eq!(added_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(added_report.added_files, 3);
        assert_eq!(added_report.modified_files, 0);
        assert_eq!(added_report.removed_files, 0);
        assert_eq!(added_report.reparsed_files, 0);
        assert_eq!(
            added.warnings,
            vec![
                "parser skipped unsupported language token: php".to_string(),
                "parser skipped unsupported language token: php-config".to_string(),
            ]
        );
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after PHP add")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after PHP add")
                .families,
            expected_families
        );

        let unchanged = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync unchanged PHP inventory");
        assert_eq!(
            unchanged.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(unchanged.parser_attempted_files, 0);
        assert_eq!(
            unchanged.warnings,
            vec![
                "parser skipped unsupported language token: php".to_string(),
                "parser skipped unsupported language token: php-config".to_string(),
            ]
        );
        let unchanged_report = unchanged.sync_report.expect("unchanged sync report");
        assert_eq!(unchanged_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(unchanged_report.added_files, 0);
        assert_eq!(unchanged_report.modified_files, 0);
        assert_eq!(unchanged_report.removed_files, 0);
        assert_eq!(unchanged_report.reparsed_files, 0);

        fs::write(workspace.path().join("main.php"), "<?php // changed\n")
            .expect("modify PHP source");
        fs::write(workspace.path().join("composer.json"), "changed\n")
            .expect("modify Composer config");
        fs::write(workspace.path().join("phpunit.xml"), "changed\n")
            .expect("modify PHPUnit config");
        let modified = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync modified PHP inventory");
        assert_eq!(
            modified.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(modified.parser_attempted_files, 0);
        assert_eq!(
            modified.warnings,
            vec![
                "parser skipped unsupported language token: php".to_string(),
                "parser skipped unsupported language token: php-config".to_string(),
            ]
        );
        let modified_report = modified.sync_report.expect("modified sync report");
        assert_eq!(modified_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(modified_report.added_files, 0);
        assert_eq!(modified_report.modified_files, 3);
        assert_eq!(modified_report.removed_files, 0);
        assert_eq!(modified_report.reparsed_files, 0);

        fs::remove_file(workspace.path().join("main.php")).expect("remove PHP source inventory");
        let source_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed PHP source inventory");
        assert_eq!(source_removed.parser_attempted_files, 0);
        assert_eq!(
            source_removed.warnings,
            vec!["parser skipped unsupported language token: php-config".to_string()]
        );
        let source_removed_report = source_removed
            .sync_report
            .expect("source removal sync report");
        assert_eq!(
            source_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(source_removed_report.added_files, 0);
        assert_eq!(source_removed_report.modified_files, 0);
        assert_eq!(source_removed_report.removed_files, 1);
        assert_eq!(source_removed_report.reparsed_files, 0);

        for path in ["composer.json", "phpunit.xml"] {
            fs::remove_file(workspace.path().join(path)).expect("remove PHP config inventory");
        }
        let configs_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed PHP config inventory");
        assert_eq!(configs_removed.parser_attempted_files, 0);
        assert!(configs_removed.warnings.is_empty());
        let configs_removed_report = configs_removed
            .sync_report
            .expect("config removal sync report");
        assert_eq!(
            configs_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(configs_removed_report.added_files, 0);
        assert_eq!(configs_removed_report.modified_files, 0);
        assert_eq!(configs_removed_report.removed_files, 2);
        assert_eq!(configs_removed_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after PHP removal")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after PHP removal")
                .families,
            expected_families
        );
        assert_eq!(source_store.paths(), vec!["server.ts".to_string()]);
        assert_eq!(parser.paths(), vec!["server.ts".to_string()]);
    }

    #[test]
    fn mixed_repo_swift_inventory_deltas_stay_incremental_and_preserve_non_swift_claims() {
        let workspace = TempWorkspace::new("indexing-swift-incremental-inventory");
        fs::write(
            workspace.path().join("server.ts"),
            "import express from 'express';\n\
             const app = express();\n\
             app.get('/users', (req, res) => { res.json([]); });\n\
             app.post('/users', (req, res) => { res.json({}); });\n\
             app.delete('/users/:id', (req, res) => { res.json({}); });\n",
        )
        .expect("write stable TypeScript family source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        let initial = index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index non-Swift family");
        assert_eq!(
            initial.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(initial.parser_attempted_files, 1);
        let expected_facts = store
            .list_active_semantic_facts()
            .expect("list initial facts")
            .facts;
        let expected_families = store
            .list_active_families()
            .expect("list initial families")
            .families;
        assert_eq!(expected_families.len(), 1);

        fs::write(workspace.path().join("main.swift"), "fatalError()\n").expect("add Swift source");
        fs::write(workspace.path().join("Package.swift"), "not-evaluated\n")
            .expect("add Swift package manifest");
        fs::write(workspace.path().join("Package.resolved"), "not-decoded\n")
            .expect("add Swift package lock");
        let added = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync added Swift inventory");
        assert_eq!(
            added.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(added.parser_attempted_files, 0);
        assert_eq!(
            added.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );
        let added_report = added.sync_report.expect("added sync report");
        assert_eq!(added_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(added_report.added_files, 3);
        assert_eq!(added_report.modified_files, 0);
        assert_eq!(added_report.removed_files, 0);
        assert_eq!(added_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after Swift add")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after Swift add")
                .families,
            expected_families
        );

        let unchanged = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync unchanged Swift inventory");
        assert_eq!(
            unchanged.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(unchanged.parser_attempted_files, 0);
        assert_eq!(
            unchanged.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );
        let unchanged_report = unchanged.sync_report.expect("unchanged sync report");
        assert_eq!(unchanged_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(unchanged_report.added_files, 0);
        assert_eq!(unchanged_report.modified_files, 0);
        assert_eq!(unchanged_report.removed_files, 0);
        assert_eq!(unchanged_report.reparsed_files, 0);

        fs::write(
            workspace.path().join("main.swift"),
            "fatalError(\"changed\")\n",
        )
        .expect("modify Swift source");
        fs::write(workspace.path().join("Package.swift"), "changed\n")
            .expect("modify Swift package manifest");
        fs::write(workspace.path().join("Package.resolved"), "changed\n")
            .expect("modify Swift package lock");
        let modified = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("sync modified Swift inventory");
        assert_eq!(
            modified.indexing_mode,
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(modified.parser_attempted_files, 0);
        assert_eq!(
            modified.warnings,
            vec![
                "parser skipped unsupported language token: swift".to_string(),
                "parser skipped unsupported language token: swift-config".to_string(),
            ]
        );
        let modified_report = modified.sync_report.expect("modified sync report");
        assert_eq!(modified_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(modified_report.added_files, 0);
        assert_eq!(modified_report.modified_files, 3);
        assert_eq!(modified_report.removed_files, 0);
        assert_eq!(modified_report.reparsed_files, 0);

        fs::remove_file(workspace.path().join("main.swift"))
            .expect("remove Swift source inventory");
        let source_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed Swift source inventory");
        assert_eq!(source_removed.parser_attempted_files, 0);
        assert_eq!(
            source_removed.warnings,
            vec!["parser skipped unsupported language token: swift-config".to_string()]
        );
        let source_removed_report = source_removed
            .sync_report
            .expect("source removal sync report");
        assert_eq!(
            source_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(source_removed_report.added_files, 0);
        assert_eq!(source_removed_report.modified_files, 0);
        assert_eq!(source_removed_report.removed_files, 1);
        assert_eq!(source_removed_report.reparsed_files, 0);

        for path in ["Package.swift", "Package.resolved"] {
            fs::remove_file(workspace.path().join(path)).expect("remove Swift config inventory");
        }
        let configs_removed =
            sync_with_families(request(), &source_store, &parser, &detector, &store)
                .expect("sync removed Swift config inventory");
        assert_eq!(configs_removed.parser_attempted_files, 0);
        assert!(configs_removed.warnings.is_empty());
        let configs_removed_report = configs_removed
            .sync_report
            .expect("config removal sync report");
        assert_eq!(
            configs_removed_report.sync_mode,
            IndexingSyncMode::Incremental
        );
        assert_eq!(configs_removed_report.added_files, 0);
        assert_eq!(configs_removed_report.modified_files, 0);
        assert_eq!(configs_removed_report.removed_files, 2);
        assert_eq!(configs_removed_report.reparsed_files, 0);
        assert_eq!(
            store
                .list_active_semantic_facts()
                .expect("list facts after Swift removal")
                .facts,
            expected_facts
        );
        assert_eq!(
            store
                .list_active_families()
                .expect("list families after Swift removal")
                .families,
            expected_families
        );
        assert_eq!(source_store.paths(), vec!["server.ts".to_string()]);
        assert_eq!(parser.paths(), vec!["server.ts".to_string()]);
    }

    #[test]
    fn incremental_sync_purges_legacy_claim_records_for_inventory_only_go_paths() {
        let workspace = TempWorkspace::new("indexing-go-purge-legacy-claims");
        fs::write(workspace.path().join("main.go"), "package demo\n").expect("write Go inventory");
        fs::write(workspace.path().join("Stable.java"), "class Stable {}\n")
            .expect("write unrelated Java source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index base inventory");

        let active = store
            .list_active_indexed_files()
            .expect("list active files before tamper");
        let go_file = active
            .files
            .iter()
            .find(|file| file.path == "main.go")
            .expect("Go file metadata");
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:main.go#module:0-1:legacy', 'main.go', 'go', 'module', 0, 1, ?2)",
                params![active.generation_id, go_file.content_hash.as_str()],
            )
            .expect("seed legacy Go unit");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:main.go#module:0-1:legacy', \
                         'unit:main.go#module:0-1:legacy', 'module', '{}')",
                params![active.generation_id],
            )
            .expect("seed legacy Go IR");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-go', 'unit:main.go#module:0-1:legacy', \
                         'main.go', ?2, 0, 1, 'legacy tampered Go evidence')",
                params![active.generation_id, go_file.content_hash.as_str()],
            )
            .expect("seed legacy Go evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-go', 'SYMBOL', \
                         'unit:main.go#module:0-1:legacy', \
                         'go.testing.test_function', 'STRUCTURAL', 'legacy-go', '0', \
                         'tampered', '[]', 'evidence:legacy-go')",
                params![active.generation_id],
            )
            .expect("seed legacy Go fact");
        connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, 'family:go:legacy', 'DOMINANT_PATTERN', 2, 2, 1.0, 0, 0, 0, 0, \
                         'coverage 2/2 with no competing ready family')",
                params![active.generation_id],
            )
            .expect("seed legacy Go family");
        connection
            .execute(
                "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
                 VALUES (?1, 'family:go:legacy', 'unit:main.go#module:0-1:legacy', \
                         'framework:go.testing')",
                params![active.generation_id],
            )
            .expect("seed legacy Go family member");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, \
                  path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-go-family', 'family:go:legacy', \
                         'unit:main.go#module:0-1:legacy', '[\"support\"]', 'main.go', ?2, \
                         0, 1, 'legacy tampered Go family evidence')",
                params![active.generation_id, go_file.content_hash.as_str()],
            )
            .expect("seed legacy Go family evidence");
        drop(connection);

        assert!(store
            .list_active_code_units()
            .expect("read tampered units")
            .units
            .iter()
            .any(|unit| unit.path == "main.go"));
        assert!(store
            .list_active_families()
            .expect("read tampered families")
            .families
            .iter()
            .any(|family| family.family_id == "family:go:legacy"));

        fs::write(
            workspace.path().join("Stable.java"),
            "class Stable { int changed; }\n",
        )
        .expect("modify unrelated Java source");
        let outcome = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("incremental sync after unrelated change");

        let sync_report = outcome.sync_report.expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(sync_report.modified_files, 1);
        assert_eq!(sync_report.reparsed_files, 1);
        assert_eq!(
            sync_report.dirty_records_cleared, 0,
            "generation-by-replacement omission does not create or clear dirty markers"
        );
        let files = store
            .list_active_indexed_files()
            .expect("read active files after purge");
        assert!(files
            .files
            .iter()
            .any(|file| file.path == "main.go" && file.language == "go"));
        assert!(!store
            .list_active_code_units()
            .expect("read units after purge")
            .units
            .iter()
            .any(|unit| unit.path == "main.go"));
        assert!(!store
            .list_active_ir_graph()
            .expect("read IR after purge")
            .nodes
            .iter()
            .any(|node| node.code_unit_id == "unit:main.go#module:0-1:legacy"));
        assert!(!store
            .list_active_semantic_facts()
            .expect("read facts after purge")
            .facts
            .iter()
            .any(|fact| fact.path == "main.go"));
        assert!(!store
            .list_active_families()
            .expect("read families after purge")
            .families
            .iter()
            .any(|family| family.family_id == "family:go:legacy"));
        for path in source_store.paths() {
            assert!(
                !indexed_language_path_is_go(&path),
                "source read Go path {path}"
            );
        }
        for path in parser.paths() {
            assert!(
                !indexed_language_path_is_go(&path),
                "parser saw Go path {path}"
            );
        }
    }

    #[test]
    fn incremental_sync_purges_legacy_claim_records_for_inventory_only_ruby_paths() {
        let workspace = TempWorkspace::new("indexing-ruby-purge-legacy-claims");
        fs::write(workspace.path().join("main.rb"), "puts :inventory\n")
            .expect("write Ruby inventory");
        fs::write(workspace.path().join("Stable.java"), "class Stable {}\n")
            .expect("write unrelated Java source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index base inventory");

        let active = store
            .list_active_indexed_files()
            .expect("list active files before tamper");
        let ruby_file = active
            .files
            .iter()
            .find(|file| file.path == "main.rb")
            .expect("Ruby file metadata");
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:main.rb#module:0-1:legacy', 'main.rb', 'ruby', 'module', 0, 1, ?2)",
                params![active.generation_id, ruby_file.content_hash.as_str()],
            )
            .expect("seed legacy Ruby unit");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:main.rb#module:0-1:legacy', \
                         'unit:main.rb#module:0-1:legacy', 'module', '{}')",
                params![active.generation_id],
            )
            .expect("seed legacy Ruby IR");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-ruby', 'unit:main.rb#module:0-1:legacy', \
                         'main.rb', ?2, 0, 1, 'legacy tampered Ruby evidence')",
                params![active.generation_id, ruby_file.content_hash.as_str()],
            )
            .expect("seed legacy Ruby evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-ruby', 'SYMBOL', \
                         'unit:main.rb#module:0-1:legacy', \
                         'ruby.minitest.test_method', 'STRUCTURAL', 'legacy-ruby', '0', \
                         'tampered', '[]', 'evidence:legacy-ruby')",
                params![active.generation_id],
            )
            .expect("seed legacy Ruby fact");
        connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, 'family:ruby:legacy', 'DOMINANT_PATTERN', 2, 2, 1.0, 0, 0, 0, 0, \
                         'coverage 2/2 with no competing ready family')",
                params![active.generation_id],
            )
            .expect("seed legacy Ruby family");
        connection
            .execute(
                "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
                 VALUES (?1, 'family:ruby:legacy', 'unit:main.rb#module:0-1:legacy', \
                         'framework:ruby.minitest')",
                params![active.generation_id],
            )
            .expect("seed legacy Ruby family member");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, \
                  path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-ruby-family', 'family:ruby:legacy', \
                         'unit:main.rb#module:0-1:legacy', '[\"support\"]', 'main.rb', ?2, \
                         0, 1, 'legacy tampered Ruby family evidence')",
                params![active.generation_id, ruby_file.content_hash.as_str()],
            )
            .expect("seed legacy Ruby family evidence");
        drop(connection);

        fs::write(
            workspace.path().join("Stable.java"),
            "class Stable { int changed; }\n",
        )
        .expect("modify unrelated Java source");
        let outcome = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("incremental sync after unrelated change");

        let sync_report = outcome.sync_report.expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(sync_report.modified_files, 1);
        assert_eq!(sync_report.reparsed_files, 1);
        let active_after_purge = store
            .list_active_indexed_files()
            .expect("read active files after purge");
        assert!(active_after_purge
            .files
            .iter()
            .any(|file| file.path == "main.rb" && file.language == "ruby"));
        assert!(!store
            .list_active_code_units()
            .expect("read units after purge")
            .units
            .iter()
            .any(|unit| unit.path == "main.rb"));
        assert!(!store
            .list_active_ir_graph()
            .expect("read IR after purge")
            .nodes
            .iter()
            .any(|node| node.code_unit_id == "unit:main.rb#module:0-1:legacy"));
        assert!(!store
            .list_active_semantic_facts()
            .expect("read facts after purge")
            .facts
            .iter()
            .any(|fact| fact.path == "main.rb"));
        assert!(!store
            .list_active_families()
            .expect("read families after purge")
            .families
            .iter()
            .any(|family| family.family_id == "family:ruby:legacy"));
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("reopen repository database");
        let remaining_ruby_evidence: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM evidence \
                 WHERE generation_id = ?1 \
                   AND (path = 'main.rb' \
                        OR evidence_id IN ('evidence:legacy-ruby', \
                                           'evidence:legacy-ruby-family'))",
                params![active_after_purge.generation_id],
                |row| row.get(0),
            )
            .expect("count active Ruby legacy evidence");
        assert_eq!(remaining_ruby_evidence, 0);
        assert!(source_store.paths().iter().all(|path| path != "main.rb"));
        assert!(parser.paths().iter().all(|path| path != "main.rb"));
    }

    #[test]
    fn incremental_sync_purges_legacy_claim_records_for_inventory_only_php_paths() {
        let workspace = TempWorkspace::new("indexing-php-purge-legacy-claims");
        fs::write(workspace.path().join("main.php"), "<?php\n").expect("write PHP inventory");
        fs::write(workspace.path().join("composer.json"), "not-json\n")
            .expect("write PHP config inventory");
        fs::write(workspace.path().join("Stable.java"), "class Stable {}\n")
            .expect("write unrelated Java source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index base inventory");

        let active = store
            .list_active_indexed_files()
            .expect("list active files before tamper");
        let php_file = active
            .files
            .iter()
            .find(|file| file.path == "main.php")
            .expect("PHP file metadata");
        let php_config = active
            .files
            .iter()
            .find(|file| file.path == "composer.json")
            .expect("PHP config metadata");
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:main.php#module:0-1:legacy', 'main.php', 'php', 'module', 0, 1, ?2)",
                params![active.generation_id, php_file.content_hash.as_str()],
            )
            .expect("seed legacy PHP unit");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:main.php#module:0-1:legacy', \
                         'unit:main.php#module:0-1:legacy', 'module', '{}')",
                params![active.generation_id],
            )
            .expect("seed legacy PHP IR");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-php', 'unit:main.php#module:0-1:legacy', \
                         'main.php', ?2, 0, 1, 'legacy tampered PHP evidence')",
                params![active.generation_id, php_file.content_hash.as_str()],
            )
            .expect("seed legacy PHP evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-php', 'SYMBOL', \
                         'unit:main.php#module:0-1:legacy', \
                         'php.phpunit.test_method', 'STRUCTURAL', 'legacy-php', '0', \
                         'tampered', '[]', 'evidence:legacy-php')",
                params![active.generation_id],
            )
            .expect("seed legacy PHP fact");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:composer.json#module:0-1:legacy', 'composer.json', \
                         'php-config', 'module', 0, 1, ?2)",
                params![active.generation_id, php_config.content_hash.as_str()],
            )
            .expect("seed legacy PHP config unit");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-php-config', \
                         'unit:composer.json#module:0-1:legacy', 'composer.json', ?2, 0, 1, \
                         'legacy tampered PHP config evidence')",
                params![active.generation_id, php_config.content_hash.as_str()],
            )
            .expect("seed legacy PHP config evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-php-config', 'PROJECT_CONFIG', \
                         'unit:composer.json#module:0-1:legacy', \
                         'php.composer.project_scope', 'STRUCTURAL', 'legacy-php-config', '0', \
                         'tampered', '[]', 'evidence:legacy-php-config')",
                params![active.generation_id],
            )
            .expect("seed legacy PHP config fact");
        connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, 'family:php:legacy', 'DOMINANT_PATTERN', 2, 2, 1.0, 0, 0, 0, 0, \
                         'coverage 2/2 with no competing ready family')",
                params![active.generation_id],
            )
            .expect("seed legacy PHP family");
        connection
            .execute(
                "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
                 VALUES (?1, 'family:php:legacy', 'unit:main.php#module:0-1:legacy', \
                         'framework:php.phpunit')",
                params![active.generation_id],
            )
            .expect("seed legacy PHP family member");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, \
                  path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-php-family', 'family:php:legacy', \
                         'unit:main.php#module:0-1:legacy', '[\"support\"]', 'main.php', ?2, \
                         0, 1, 'legacy tampered PHP family evidence')",
                params![active.generation_id, php_file.content_hash.as_str()],
            )
            .expect("seed legacy PHP family evidence");
        drop(connection);

        fs::write(
            workspace.path().join("Stable.java"),
            "class Stable { int changed; }\n",
        )
        .expect("modify unrelated Java source");
        let outcome = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("incremental sync after unrelated change");

        let sync_report = outcome.sync_report.expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(sync_report.modified_files, 1);
        assert_eq!(sync_report.reparsed_files, 1);
        let active_after_purge = store
            .list_active_indexed_files()
            .expect("read active files after purge");
        assert!(active_after_purge
            .files
            .iter()
            .any(|file| file.path == "main.php" && file.language == "php"));
        assert!(active_after_purge
            .files
            .iter()
            .any(|file| file.path == "composer.json" && file.language == "php-config"));
        assert!(!store
            .list_active_code_units()
            .expect("read units after purge")
            .units
            .iter()
            .any(|unit| matches!(unit.path.as_str(), "main.php" | "composer.json")));
        assert!(!store
            .list_active_ir_graph()
            .expect("read IR after purge")
            .nodes
            .iter()
            .any(|node| node.code_unit_id == "unit:main.php#module:0-1:legacy"));
        assert!(!store
            .list_active_semantic_facts()
            .expect("read facts after purge")
            .facts
            .iter()
            .any(|fact| matches!(fact.path.as_str(), "main.php" | "composer.json")));
        assert!(!store
            .list_active_families()
            .expect("read families after purge")
            .families
            .iter()
            .any(|family| family.family_id == "family:php:legacy"));
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("reopen repository database");
        let remaining_php_evidence: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM evidence \
                 WHERE generation_id = ?1 \
                   AND (path IN ('main.php', 'composer.json') \
                        OR evidence_id IN ('evidence:legacy-php', \
                                           'evidence:legacy-php-config', \
                                           'evidence:legacy-php-family'))",
                params![active_after_purge.generation_id],
                |row| row.get(0),
            )
            .expect("count active PHP legacy evidence");
        assert_eq!(remaining_php_evidence, 0);
        assert!(source_store
            .paths()
            .iter()
            .all(|path| !matches!(path.as_str(), "main.php" | "composer.json")));
        assert!(parser
            .paths()
            .iter()
            .all(|path| !matches!(path.as_str(), "main.php" | "composer.json")));
    }

    #[test]
    fn incremental_sync_purges_legacy_claim_records_for_inventory_only_swift_paths() {
        let workspace = TempWorkspace::new("indexing-swift-purge-legacy-claims");
        fs::write(workspace.path().join("main.swift"), "fatalError()\n")
            .expect("write Swift inventory");
        fs::write(workspace.path().join("Package.swift"), "not-evaluated\n")
            .expect("write Swift config inventory");
        fs::write(workspace.path().join("Stable.java"), "class Stable {}\n")
            .expect("write unrelated Java source");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let source_store = RecordingSourceStore::new();
        let parser = RecordingParser::new();
        let request = || IndexingRequest::new(workspace.path().display().to_string());

        index_repository_with_discovery_parser_frameworks_families_and_store(
            request(),
            &FilesystemFileDiscovery,
            &source_store,
            &parser,
            &detector,
            &store,
        )
        .expect("index base inventory");

        let active = store
            .list_active_indexed_files()
            .expect("list active files before tamper");
        let swift_file = active
            .files
            .iter()
            .find(|file| file.path == "main.swift")
            .expect("Swift file metadata");
        let swift_config = active
            .files
            .iter()
            .find(|file| file.path == "Package.swift")
            .expect("Swift config metadata");
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("open repository database");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:main.swift#module:0-1:legacy', 'main.swift', \
                         'swift', 'module', 0, 1, ?2)",
                params![active.generation_id, swift_file.content_hash.as_str()],
            )
            .expect("seed legacy Swift unit");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:main.swift#module:0-1:legacy', \
                         'unit:main.swift#module:0-1:legacy', 'module', '{}')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift IR");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-swift', \
                         'unit:main.swift#module:0-1:legacy', 'main.swift', ?2, 0, 1, \
                         'legacy tampered Swift evidence')",
                params![active.generation_id, swift_file.content_hash.as_str()],
            )
            .expect("seed legacy Swift evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-swift', 'SYMBOL', \
                         'unit:main.swift#module:0-1:legacy', \
                         'swift.xctest.test_method', 'STRUCTURAL', 'legacy-swift', '0', \
                         'tampered', '[]', 'evidence:legacy-swift')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift fact");
        connection
            .execute(
                "INSERT INTO code_units \
                 (generation_id, code_unit_id, path, language, kind, start_byte, end_byte, content_hash) \
                 VALUES (?1, 'unit:Package.swift#module:0-1:legacy', 'Package.swift', \
                         'swift-config', 'module', 0, 1, ?2)",
                params![active.generation_id, swift_config.content_hash.as_str()],
            )
            .expect("seed legacy Swift config unit");
        connection
            .execute(
                "INSERT INTO ir_nodes \
                 (generation_id, node_id, code_unit_id, kind, payload_json) \
                 VALUES (?1, 'ir:unit:Package.swift#module:0-1:legacy', \
                         'unit:Package.swift#module:0-1:legacy', 'module', '{}')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift config IR");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, code_unit_id, path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-swift-config', \
                         'unit:Package.swift#module:0-1:legacy', 'Package.swift', ?2, 0, 1, \
                         'legacy tampered Swift config evidence')",
                params![active.generation_id, swift_config.content_hash.as_str()],
            )
            .expect("seed legacy Swift config evidence");
        connection
            .execute(
                "INSERT INTO semantic_facts \
                 (generation_id, fact_id, kind, subject, target, certainty, origin_engine, \
                  origin_engine_version, origin_method, assumptions_json, evidence_id) \
                 VALUES (?1, 'semantic-fact:legacy-swift-config', 'PROJECT_CONFIG', \
                         'unit:Package.swift#module:0-1:legacy', \
                         'swift.package.project_scope', 'STRUCTURAL', 'legacy-swift-config', '0', \
                         'tampered', '[]', 'evidence:legacy-swift-config')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift config fact");
        connection
            .execute(
                "INSERT INTO families (\
                     generation_id, family_id, classification, eligible_peer_count, \
                     supported_member_count, coverage_ratio, competing_ready_family_count, \
                     largest_competing_support, blocked_peer_count, unsupported_peer_count, \
                     classification_reason) \
                 VALUES (?1, 'family:swift:legacy', 'DOMINANT_PATTERN', 2, 2, 1.0, 0, 0, 0, 0, \
                         'coverage 2/2 with no competing ready family')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift family");
        connection
            .execute(
                "INSERT INTO family_members (generation_id, family_id, code_unit_id, role) \
                 VALUES (?1, 'family:swift:legacy', \
                         'unit:main.swift#module:0-1:legacy', 'framework:swift.xctest')",
                params![active.generation_id],
            )
            .expect("seed legacy Swift family member");
        connection
            .execute(
                "INSERT INTO evidence \
                 (generation_id, evidence_id, family_id, code_unit_id, covered_claims_json, \
                  path, content_hash, start_byte, end_byte, note) \
                 VALUES (?1, 'evidence:legacy-swift-family', 'family:swift:legacy', \
                         'unit:main.swift#module:0-1:legacy', '[\"support\"]', \
                         'main.swift', ?2, 0, 1, \
                         'legacy tampered Swift family evidence')",
                params![active.generation_id, swift_file.content_hash.as_str()],
            )
            .expect("seed legacy Swift family evidence");
        drop(connection);

        fs::write(
            workspace.path().join("Stable.java"),
            "class Stable { int changed; }\n",
        )
        .expect("modify unrelated Java source");
        let outcome = sync_with_families(request(), &source_store, &parser, &detector, &store)
            .expect("incremental sync after unrelated change");

        let sync_report = outcome.sync_report.expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(sync_report.modified_files, 1);
        assert_eq!(sync_report.reparsed_files, 1);
        let active_after_purge = store
            .list_active_indexed_files()
            .expect("read active files after purge");
        assert!(active_after_purge
            .files
            .iter()
            .any(|file| file.path == "main.swift" && file.language == "swift"));
        assert!(active_after_purge
            .files
            .iter()
            .any(|file| file.path == "Package.swift" && file.language == "swift-config"));
        assert!(!store
            .list_active_code_units()
            .expect("read units after purge")
            .units
            .iter()
            .any(|unit| matches!(unit.path.as_str(), "main.swift" | "Package.swift")));
        assert!(!store
            .list_active_ir_graph()
            .expect("read IR after purge")
            .nodes
            .iter()
            .any(|node| matches!(
                node.code_unit_id.as_str(),
                "unit:main.swift#module:0-1:legacy" | "unit:Package.swift#module:0-1:legacy"
            )));
        assert!(!store
            .list_active_semantic_facts()
            .expect("read facts after purge")
            .facts
            .iter()
            .any(|fact| matches!(fact.path.as_str(), "main.swift" | "Package.swift")));
        assert!(!store
            .list_active_families()
            .expect("read families after purge")
            .families
            .iter()
            .any(|family| family.family_id == "family:swift:legacy"));
        let connection =
            Connection::open(state.join("repogrammar.sqlite")).expect("reopen repository database");
        let remaining_swift_evidence: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM evidence \
                 WHERE generation_id = ?1 \
                   AND (path IN ('main.swift', 'Package.swift') \
                        OR evidence_id IN ('evidence:legacy-swift', \
                                           'evidence:legacy-swift-config', \
                                           'evidence:legacy-swift-family'))",
                params![active_after_purge.generation_id],
                |row| row.get(0),
            )
            .expect("count active Swift legacy evidence");
        assert_eq!(remaining_swift_evidence, 0);
        assert!(source_store
            .paths()
            .iter()
            .all(|path| !matches!(path.as_str(), "main.swift" | "Package.swift")));
        assert!(parser
            .paths()
            .iter()
            .all(|path| !matches!(path.as_str(), "main.swift" | "Package.swift")));
    }

    fn indexed_language_path_is_go(path: &str) -> bool {
        path.ends_with(".go")
            || path == "go.mod"
            || path == "go.work"
            || path.ends_with("/go.mod")
            || path.ends_with("/go.work")
    }

    fn sync_with_families(
        request: IndexingRequest,
        source_store: &impl SourceStore,
        parser: &impl SourceParser,
        detector: &dyn FrameworkRoleDetector,
        store: &SqliteIndexStore,
    ) -> Result<IndexingOutcome, RepoGrammarError> {
        let mut progress = |_event: ProgressEvent| {};
        sync_repository_with_optional_semantic_worker(
            request,
            &FilesystemFileDiscovery,
            source_store,
            parser,
            IndexingPipelineOptions {
                framework_roles: Some(detector),
                rust_provider: None,
                semantic_worker: None,
                family_store: Some(store),
            },
            store,
            &mut progress,
        )
    }

    #[test]
    fn sync_falls_back_when_tsjs_module_inventory_changes() {
        let workspace = TempWorkspace::new("indexing-tsjs-module-inventory-fallback");
        fs::write(
            workspace.path().join("app.ts"),
            "import { x } from './missing';\nexport const y = x;\n",
        )
        .expect("write unresolved import");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;

        index_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &detector,
            &store,
        )
        .expect("index unresolved import");
        assert_eq!(unresolved_import_unknown_count(&store), 1);

        fs::write(workspace.path().join("missing.ts"), "export const x = 1;\n")
            .expect("write import target");
        let outcome = sync_repository_with_discovery_parser_frameworks_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &detector,
            &store,
        )
        .expect("sync after module inventory change");

        let sync_report = outcome.sync_report.expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::FullRebuildFallback);
        assert_eq!(
            sync_report.fallback_reason.as_deref(),
            Some("project_context_changed")
        );
        assert_eq!(sync_report.added_files, 1);
        assert_eq!(sync_report.reparsed_files, 2);
        assert_eq!(unresolved_import_unknown_count(&store), 0);
    }

    fn unresolved_import_unknown_count(store: &impl IndexStore) -> usize {
        store
            .list_active_semantic_facts()
            .expect("list semantic facts")
            .facts
            .into_iter()
            .filter(|fact| {
                fact.kind == "UNKNOWN"
                    && fact
                        .target
                        .as_ref()
                        .is_some_and(|target| target.as_str() == "UnresolvedImport")
            })
            .count()
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
        let parser_method = python_project_config_parser_method(document.path)
            .expect("test project-config path has a parser method");
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
                method: parser_method.to_string(),
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
                format!("parsed_with={parser_method}"),
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
            if document.language == Language::PythonConfig && document.path == "setup.cfg" {
                semantic_facts.extend(
                    ["pkg", "tests"]
                        .into_iter()
                        .map(|root| project_config_source_root_fact(&document, &unit, root)),
                );
            }
            if document.language == Language::PythonConfig && document.path == "setup.py" {
                semantic_facts.extend(
                    ["app"]
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

    fn indexed_csharp_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: "csharp".to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn indexed_unit_for_language(
        path: &str,
        language: &str,
        kind: &str,
        index: usize,
    ) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: language.to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn family_unknown_fact_for_unit(
        unit: &IndexedCodeUnitRecord,
        engine: &str,
        method: &str,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid reason")),
            origin: FactOrigin {
                engine: engine.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: method.to_string(),
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
                "parser typed UNKNOWN for family support",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    fn rust_structural_anchor_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: RUST_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: RUST_ANCHOR_METHOD.to_string(),
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
                "bounded Rust framework structural role anchor",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("rust_anchor_kind={}", unit.kind)],
        }
    }

    #[test]
    fn support_blocking_matches_authoritative_classifier_for_all_languages() {
        let cases = [
            (
                "python",
                "python",
                "cpython_ast",
                "framework:fastapi.route",
                UnknownReasonCode::UnresolvedImport,
                "python_import_resolution",
                true,
            ),
            (
                "python",
                "python",
                "cpython_ast",
                "framework:fastapi.route",
                UnknownReasonCode::RuntimeDependencyInjection,
                "fastapi_dependency_target",
                false,
            ),
            (
                "typescript",
                TSJS_ANCHOR_ENGINE,
                TSJS_ANCHOR_METHOD,
                "framework:express.route_handler",
                UnknownReasonCode::UnresolvedImport,
                "tsjs_receiver_binding",
                true,
            ),
            (
                "typescript",
                TSJS_ANCHOR_ENGINE,
                TSJS_ANCHOR_METHOD,
                "framework:express.route_handler",
                UnknownReasonCode::FrameworkMagic,
                "tsjs_handler_shape",
                false,
            ),
            (
                "java",
                JAVA_ANCHOR_ENGINE,
                JAVA_ANCHOR_METHOD,
                "framework:spring.mvc.route",
                UnknownReasonCode::UnresolvedImport,
                "java_spring_controller_identity",
                true,
            ),
            (
                "java",
                JAVA_ANCHOR_ENGINE,
                JAVA_ANCHOR_METHOD,
                "framework:spring.mvc.route",
                UnknownReasonCode::FrameworkMagic,
                "java_spring_route_path",
                false,
            ),
            (
                "csharp",
                CSHARP_ANCHOR_ENGINE,
                CSHARP_ANCHOR_METHOD,
                "framework:aspnetcore.controller_action",
                UnknownReasonCode::UnresolvedImport,
                "csharp_attribute_binding",
                true,
            ),
            (
                "csharp",
                CSHARP_ANCHOR_ENGINE,
                CSHARP_ANCHOR_METHOD,
                "framework:aspnetcore.controller_action",
                UnknownReasonCode::FrameworkMagic,
                "csharp_aspnet_route_template",
                false,
            ),
            (
                "cpp",
                CPP_ANCHOR_ENGINE,
                CPP_ANCHOR_METHOD,
                "framework:cpp.gtest.test_case",
                UnknownReasonCode::UnresolvedImport,
                "cpp_test_framework_identity",
                true,
            ),
            (
                "cpp",
                CPP_ANCHOR_ENGINE,
                CPP_ANCHOR_METHOD,
                "framework:cpp.gtest.test_case",
                UnknownReasonCode::MissingProjectConfig,
                "cpp_project_config",
                false,
            ),
            (
                "rust",
                RUST_ANCHOR_ENGINE,
                RUST_ANCHOR_METHOD,
                "framework:serde.model",
                UnknownReasonCode::UnresolvedImport,
                "rust_framework_attribute_binding",
                true,
            ),
            (
                "rust",
                RUST_ANCHOR_ENGINE,
                RUST_ANCHOR_METHOD,
                "framework:serde.model",
                UnknownReasonCode::MacroOrPreprocessor,
                "rust_derive_expansion",
                false,
            ),
        ];

        for (index, (language, engine, method, role, reason, claim, expected_blocks)) in
            cases.into_iter().enumerate()
        {
            let unit = indexed_unit_for_language(
                &format!("src/classifier_case_{index}"),
                language,
                "framework_case",
                index,
            );
            let fact = family_unknown_fact_for_unit(&unit, engine, method, reason, claim);
            let role_facts = [framework_role_fact_for_unit(&unit, role)];
            let role_by_unit = framework_role_targets_by_unit(&role_facts);
            let blocked = framework_support_blocked_units(
                std::slice::from_ref(&unit),
                std::slice::from_ref(&fact),
                &role_by_unit,
                |_| true,
            );

            assert_eq!(
                family_unknown_blocks_claim(language, &fact, role),
                expected_blocks,
                "authoritative classifier mismatch for {language}:{claim}"
            );
            assert_eq!(
                blocked.contains(unit.id.as_str()),
                expected_blocks,
                "support blocker mismatch for {language}:{claim}"
            );
        }
    }

    #[test]
    fn blocking_tsjs_unknown_prevents_structural_support_derivation() {
        let unit = indexed_tsjs_unit("src/blocked_route.ts", "express_route", 0);
        let parser_facts = [
            tsjs_structural_anchor_fact(&unit, "express.route.get"),
            family_unknown_fact_for_unit(
                &unit,
                TSJS_ANCHOR_ENGINE,
                TSJS_ANCHOR_METHOD,
                UnknownReasonCode::UnresolvedImport,
                "tsjs_receiver_binding",
            ),
        ];
        let role_facts = [framework_role_fact_for_unit(
            &unit,
            "framework:express.route_handler",
        )];

        let derived = derive_tsjs_framework_support_facts(
            std::slice::from_ref(&unit),
            &parser_facts,
            &role_facts,
        )
        .expect("derive TS/JS support with blocking UNKNOWN");

        assert!(derived.is_empty());
    }

    #[test]
    fn rust_framework_specific_unknowns_prevent_support_derivation() {
        for (index, kind, target, role, reason, claim) in [
            (
                0,
                "serde_model",
                "serde.Serialize",
                "framework:serde.model",
                UnknownReasonCode::UnresolvedImport,
                "rust_framework_attribute_binding",
            ),
            (
                1,
                "axum_route",
                "axum.routing.route",
                "framework:axum.route",
                UnknownReasonCode::FrameworkMagic,
                "rust_axum_route_identity",
            ),
        ] {
            let unit = indexed_unit_for_language(
                &format!("src/rust_case_{index}.rs"),
                "rust",
                kind,
                index,
            );
            let parser_facts = [
                rust_structural_anchor_fact(&unit, target),
                family_unknown_fact_for_unit(
                    &unit,
                    RUST_ANCHOR_ENGINE,
                    RUST_ANCHOR_METHOD,
                    reason,
                    claim,
                ),
            ];
            let role_facts = [framework_role_fact_for_unit(&unit, role)];

            let derived = derive_rust_framework_support_facts(
                std::slice::from_ref(&unit),
                &parser_facts,
                &role_facts,
            )
            .expect("derive Rust support with blocking UNKNOWN");

            assert!(derived.is_empty(), "blocking Rust claim {claim}");
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

    fn tsjs_provider_required_drizzle_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
    ) -> SemanticFact {
        let mut fact = tsjs_structural_anchor_fact(unit, target);
        fact.assumptions.extend([
            "provider_required=typescript".to_string(),
            "binding:db:kind=drizzle_db".to_string(),
            "binding:db:local_name=db".to_string(),
            "binding:db:import_specifier=./db".to_string(),
            "binding:db:export_name=db".to_string(),
            "binding:table:kind=drizzle_table".to_string(),
            "binding:table:local_name=users".to_string(),
            "binding:table:import_specifier=./schema".to_string(),
            "binding:table:export_name=users".to_string(),
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

    fn csharp_structural_anchor_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: CSHARP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CSHARP_ANCHOR_METHOD.to_string(),
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
                "bounded C# structural role anchor",
            )
            .expect("valid evidence"),
            assumptions: vec![
                format!("csharp_anchor_kind={}", unit.kind),
                "aspnet_attribute=HttpGet".to_string(),
                "http_method=GET".to_string(),
                "route_template_shape=literal".to_string(),
                "class_route_template_shape=literal".to_string(),
            ],
        }
    }

    fn csharp_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid reason")),
            origin: FactOrigin {
                engine: CSHARP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CSHARP_ANCHOR_METHOD.to_string(),
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
                "C# parser typed UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn exact_csharp_aspnet_anchors_derive_family_support_and_blocked_units_derive_none() {
        let first = indexed_csharp_unit(
            "src/Controllers/AController.cs",
            "aspnet_controller_action",
            0,
        );
        let second = indexed_csharp_unit(
            "src/Controllers/BController.cs",
            "aspnet_controller_action",
            1,
        );
        let third = indexed_csharp_unit(
            "src/Controllers/CController.cs",
            "aspnet_controller_action",
            2,
        );
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            csharp_structural_anchor_fact(&first, "aspnetcore.mvc.HttpGet"),
            csharp_structural_anchor_fact(&second, "aspnetcore.mvc.HttpGet"),
            csharp_structural_anchor_fact(&third, "aspnetcore.mvc.HttpGet"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| {
                framework_role_fact_for_unit(unit, "framework:aspnetcore.controller_action")
            })
            .collect::<Vec<_>>();

        let derived = derive_csharp_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact csharp aspnet support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == CSHARP_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == CSHARP_DERIVED_SUPPORT_METHOD
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "derived_from=tree_sitter_csharp_structural_anchors"
                })
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "csharp");
        assert_eq!(
            report.claims[0].framework_role,
            "framework:aspnetcore.controller_action"
        );
        assert_eq!(report.claims[0].support, 3);

        let blocked = derive_csharp_framework_support_facts(
            std::slice::from_ref(&first),
            &[
                csharp_structural_anchor_fact(&first, "aspnetcore.mvc.HttpGet"),
                csharp_unknown_fact(
                    &first,
                    UnknownReasonCode::UnresolvedImport,
                    "csharp_attribute_binding",
                ),
            ],
            &[framework_role_fact_for_unit(
                &first,
                "framework:aspnetcore.controller_action",
            )],
        )
        .expect("derive with blocking unknown");
        assert!(blocked.is_empty());
    }

    fn indexed_cpp_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-20:{index}"),
            path: path.to_string(),
            language: "cpp".to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 20,
            content_hash: strict_hash(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        }
    }

    fn cpp_structural_anchor_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: CPP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CPP_ANCHOR_METHOD.to_string(),
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
                "bounded C/C++ structural role anchor",
            )
            .expect("valid evidence"),
            assumptions: vec![
                format!("cpp_anchor_kind={}", unit.kind),
                "test_framework=gtest".to_string(),
                "test_macro=TEST".to_string(),
                "test_name_shape=identifier_pair".to_string(),
                "fixture_shape=free".to_string(),
            ],
        }
    }

    fn cpp_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid reason")),
            origin: FactOrigin {
                engine: CPP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CPP_ANCHOR_METHOD.to_string(),
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
                "C/C++ parser typed UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn exact_cpp_gtest_anchors_derive_family_support_and_blocked_units_derive_none() {
        let first = indexed_cpp_unit("tests/a_test.cc", "gtest_test_case", 0);
        let second = indexed_cpp_unit("tests/b_test.cc", "gtest_test_case", 1);
        let third = indexed_cpp_unit("tests/c_test.cc", "gtest_test_case", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            cpp_structural_anchor_fact(&first, "gtest.TEST"),
            cpp_structural_anchor_fact(&second, "gtest.TEST"),
            cpp_structural_anchor_fact(&third, "gtest.TEST"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:gtest.test"))
            .collect::<Vec<_>>();

        let derived = derive_cpp_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact cpp gtest support");

        assert_eq!(derived.len(), 3);
        assert!(derived.iter().all(|fact| {
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == CPP_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == CPP_DERIVED_SUPPORT_METHOD
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "derived_from=tree_sitter_c_cpp_structural_anchors"
                })
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "cpp");
        assert_eq!(report.claims[0].framework_role, "framework:gtest.test");
        assert_eq!(report.claims[0].support, 3);

        let blocked = derive_cpp_framework_support_facts(
            std::slice::from_ref(&first),
            &[
                cpp_structural_anchor_fact(&first, "gtest.TEST"),
                cpp_unknown_fact(
                    &first,
                    UnknownReasonCode::BuildVariantAmbiguity,
                    "cpp_build_variant",
                ),
            ],
            &[framework_role_fact_for_unit(&first, "framework:gtest.test")],
        )
        .expect("derive with blocking unknown");
        assert!(blocked.is_empty());
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
    fn exact_java_junit5_anchors_derive_support_and_test_lookalike_blocks() {
        fn junit5_anchor(unit: &IndexedCodeUnitRecord) -> SemanticFact {
            let mut fact = java_structural_anchor_fact(unit, "junit.jupiter.Test");
            fact.assumptions = vec![
                format!("java_anchor_kind={}", unit.kind),
                "test_annotation=Test".to_string(),
                "test_data_shape=none".to_string(),
            ];
            fact
        }

        let first = indexed_java_unit("src/test/java/ATest.java", "junit5_test_method", 0);
        let second = indexed_java_unit("src/test/java/BTest.java", "junit5_test_method", 1);
        let third = indexed_java_unit("src/test/java/CTest.java", "junit5_test_method", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            junit5_anchor(&first),
            junit5_anchor(&second),
            junit5_anchor(&third),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:junit5.test"))
            .collect::<Vec<_>>();

        let derived = derive_java_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact junit5 support");
        assert_eq!(derived.len(), 3);
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].framework_role, "framework:junit5.test");
        assert_eq!(report.claims[0].support, 3);

        let blocked = derive_java_framework_support_facts(
            std::slice::from_ref(&first),
            &[
                junit5_anchor(&first),
                java_unknown_fact(
                    &first,
                    UnknownReasonCode::UnresolvedImport,
                    "java_test_annotation_binding",
                ),
            ],
            &[framework_role_fact_for_unit(
                &first,
                "framework:junit5.test",
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
    fn fastify_plugin_registration_context_does_not_derive_route_support() {
        let unit = indexed_tsjs_unit("src/plugins.ts", "fastify_plugin_registration", 0);
        let parser_fact = tsjs_structural_anchor_fact(&unit, "fastify.plugin.register");
        let role_fact = framework_role_fact_for_unit(&unit, "framework:fastify.route_handler");

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
    fn provider_resolved_tsjs_drizzle_bindings_require_db_and_table_proofs() {
        let first = indexed_tsjs_unit("src/users.ts", "drizzle_query", 0);
        let second = indexed_tsjs_unit("src/accounts.ts", "drizzle_query", 1);
        let third = indexed_tsjs_unit("src/orders.ts", "drizzle_query", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let parser_facts = vec![
            tsjs_provider_required_drizzle_anchor_fact(&first, "drizzle.query.select"),
            tsjs_provider_required_drizzle_anchor_fact(&second, "drizzle.query.insert"),
            tsjs_provider_required_drizzle_anchor_fact(&third, "drizzle.query.query_findMany"),
        ];
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:drizzle.query"))
            .collect::<Vec<_>>();
        let worker_facts = units
            .iter()
            .flat_map(|unit| {
                [
                    tsjs_provider_binding_fact(unit, "./db", "db"),
                    tsjs_provider_binding_fact(unit, "./schema", "users"),
                ]
            })
            .collect::<Vec<_>>();

        let derived = derive_tsjs_provider_resolved_framework_support_facts(
            &units,
            &parser_facts,
            &role_facts,
            &worker_facts,
        )
        .expect("derive provider-resolved drizzle support");

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
                    .any(|assumption| assumption == "binding:db:kind=drizzle_db")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "binding:table:kind=drizzle_table")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "query_operation=resolve_reexport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=tsjs_drizzle_structural_anchors")
        }));
        let mut family_facts = role_facts;
        family_facts.extend(derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].framework_role, "framework:drizzle.query");

        let partial = derive_tsjs_provider_resolved_framework_support_facts(
            std::slice::from_ref(&first),
            std::slice::from_ref(&parser_facts[0]),
            std::slice::from_ref(&family_facts[0]),
            &[tsjs_provider_binding_fact(&first, "./db", "db")],
        )
        .expect("partial drizzle proof is valid input");
        assert!(partial.is_empty());

        let mut fallback_table = tsjs_provider_binding_fact(&first, "./schema", "users");
        fallback_table.certainty = FactCertainty::Structural;
        fallback_table.origin.engine = "repogrammar-tsjs-static-worker".to_string();
        fallback_table.origin.method = "bounded_project_model_resolver_v1".to_string();
        fallback_table
            .assumptions
            .retain(|assumption| assumption != "provider=typescript");
        fallback_table
            .assumptions
            .retain(|assumption| assumption != "provider_resolved=true");
        fallback_table
            .assumptions
            .push("provider=repogrammar_static_tsjs".to_string());
        fallback_table
            .assumptions
            .push("provider_resolved=false".to_string());
        let fallback = derive_tsjs_provider_resolved_framework_support_facts(
            std::slice::from_ref(&first),
            std::slice::from_ref(&parser_facts[0]),
            std::slice::from_ref(&family_facts[0]),
            &[
                tsjs_provider_binding_fact(&first, "./db", "db"),
                fallback_table,
            ],
        )
        .expect("fallback drizzle proof is valid input");
        assert!(fallback.is_empty());
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

    fn provider_binding_facts_for_drizzle_queries(
        workspace: &TempWorkspace,
    ) -> (Vec<String>, Vec<SemanticFact>) {
        let request = IndexingRequest::new(workspace.path().display().to_string());
        let report = discover_repository_files(request.clone(), &FilesystemFileDiscovery)
            .expect("discover files for drizzle provider support");
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
                .expect("read source for drizzle provider support");
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
                .expect("parse source for drizzle provider support");
            for unit in parse_report
                .units
                .into_iter()
                .filter(|unit| unit.kind == CodeUnitKind::DrizzleQuery)
            {
                facts.extend([
                    tsjs_provider_binding_fact_for_code_unit(&unit, "./db", "db"),
                    tsjs_provider_binding_fact_for_code_unit(&unit, "./schema", "users"),
                ]);
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
    fn discovery_resource_limit_maps_to_invalid_input_before_generation_preparation() {
        struct ResourceLimitedDiscovery;

        impl FileDiscovery for ResourceLimitedDiscovery {
            fn discover(
                &self,
                _request: FileDiscoveryRequest,
            ) -> Result<FileDiscoveryReport, FileDiscoveryError> {
                Err(FileDiscoveryError::ResourceLimitExceeded(
                    FileDiscoveryLimitExceeded {
                        kind: FileDiscoveryLimitKind::VisitedEntries,
                        limit: 1,
                        observed: 2,
                    },
                ))
            }
        }

        let workspace = TempWorkspace::new("indexing-discovery-resource-limit");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);

        let error = index_repository_with_discovery_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &ResourceLimitedDiscovery,
            &store,
        )
        .expect_err("resource limit must abort before generation preparation");

        assert!(matches!(error, RepoGrammarError::InvalidInput(_)));
        let rendered = error.to_string();
        assert!(rendered.contains("resource=visited_entries"));
        assert!(rendered.contains("narrow the repository scope"));
        assert!(!state.join("repogrammar.sqlite").exists());
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
    fn pydantic_validator_side_effect_unknown_does_not_block_derived_support() {
        let first = indexed_python_unit("schemas.py", "pydantic_model", 0);
        let second = indexed_python_unit("schemas.py", "pydantic_model", 1);
        let third = indexed_python_unit("schemas.py", "pydantic_model", 2);
        let units = vec![first.clone(), second.clone(), third.clone()];
        let mut parser_facts = units
            .iter()
            .map(|unit| {
                parser_structural_anchor_fact(unit, SemanticFactKind::Type, "pydantic.BaseModel")
            })
            .collect::<Vec<_>>();
        parser_facts.push(parser_unknown_fact_for_unit(
            &second,
            "FrameworkMagic",
            "pydantic_validator_side_effects",
        ));
        let role_facts = units
            .iter()
            .map(|unit| framework_role_fact_for_unit(unit, "framework:pydantic.model"))
            .collect::<Vec<_>>();

        let derived = derive_python_framework_support_facts(&units, &parser_facts, &role_facts)
            .expect("derive exact pydantic support");

        assert_eq!(derived.len(), 3);
        let family_facts = family_claim_facts(&parser_facts, role_facts, derived);
        let report = build_family_claims(&units, &family_facts);
        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.framework_role, "framework:pydantic.model");
        assert_eq!(claim.support, 3);
        assert!(claim.unknowns.iter().any(|unknown| {
            unknown.class == UnknownClass::NonBlocking
                && unknown.reason == UnknownReasonCode::FrameworkMagic
                && unknown.affected_claim
                    == format!("{}:pydantic_validator_side_effects", claim.family_id)
        }));
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
    fn incremental_sync_preserves_provider_resolved_family_support() {
        let workspace = TempWorkspace::new("indexing-incremental-provider-support");
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
        let worker = StaticSemanticWorker {
            expected_files,
            result: Ok(facts),
        };

        // Base generation built WITH a worker -> provider-resolved support.
        index_repository_with_discovery_parser_frameworks_semantic_worker_families_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            &detector,
            &worker,
            &store,
        )
        .expect("index base with worker");
        assert_eq!(
            provider_resolved_tsjs_support_fact_count(&state, "gen-000001"),
            3
        );

        // A worker-less incremental sync (no file changes) must recompute the
        // provider-resolved support from the copied-forward worker facts instead
        // of dropping it, so the family stays DOMINANT and support matches base.
        let mut progress = |_event: ProgressEvent| {};
        let outcome = sync_repository_with_optional_semantic_worker(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SyntaxCodeUnitParser,
            IndexingPipelineOptions {
                framework_roles: Some(&detector),
                rust_provider: None,
                semantic_worker: None,
                family_store: Some(&store),
            },
            &store,
            &mut progress,
        )
        .expect("worker-less incremental sync");

        let sync_report = outcome.sync_report.clone().expect("sync report");
        assert_eq!(sync_report.sync_mode, IndexingSyncMode::Incremental);
        assert_eq!(sync_report.fallback_reason, None);
        // A family-recomputing incremental sync with no file changes reproduces
        // the base generation's family ids exactly, so the cross-generation
        // identity delta is present and empty (not null, and not a rename).
        let delta = sync_report
            .family_identity_delta
            .clone()
            .expect("family identity delta present when families are recomputed against a base");
        assert_eq!(delta.added_count, 0);
        assert_eq!(delta.removed_count, 0);
        assert!(delta.added_sample.is_empty());
        assert!(delta.removed_sample.is_empty());
        assert_eq!(
            provider_resolved_tsjs_support_fact_count(&state, "gen-000002"),
            3
        );
        assert_eq!(
            outcome.semantic_facts as u32,
            semantic_fact_count(&state, "gen-000002")
        );
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.families.len(), 1);
        let family = store
            .show_family(&families.families[0].family_id)
            .expect("show family")
            .expect("family exists");
        assert_eq!(family.members.len(), 3);
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
    fn provider_resolved_drizzle_binding_support_is_recorded_after_worker() {
        let workspace = TempWorkspace::new("indexing-provider-drizzle-binding-support");
        fs::write(workspace.path().join("db.ts"), "export const db = {};\n")
            .expect("write shared db");
        fs::write(
            workspace.path().join("schema.ts"),
            "export const users = {};\n",
        )
        .expect("write shared schema");
        fs::write(
            workspace.path().join("package.json"),
            r#"{"dependencies":{"drizzle-orm":"latest"}}"#,
        )
        .expect("write package");
        for name in ["users", "accounts", "orders"] {
            let path = workspace.path().join(format!("{name}.ts"));
            fs::write(
                path,
                "import { db } from './db';\n\
                 import { users } from './schema';\n\
                 export function list() { return db.select().from(users); }\n",
            )
            .expect("write repository");
        }
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let (expected_files, facts) = provider_binding_facts_for_drizzle_queries(&workspace);
        assert_eq!(facts.len(), 6);
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
            .expect("index provider-resolved drizzle support");

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
            .all(|member| member.role == "framework:drizzle.query"));
        let debug = format!("{family:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("db.select().from"));
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
        let drizzle_repository =
            "import { db } from './drizzle-db';\nimport { users } from './schema';\ndb.select().from(users);\n";
        let target = "export const service = true;\n";
        let barrel = "export const value = true;\n";
        let db = "export const prisma = {};\n";
        let drizzle_db = "export const db = {};\n";
        let schema = "export const users = {};\n";
        let next_route = "export async function GET() { return Response.json([]); }\n";
        let config = r#"{"compilerOptions":{"baseUrl":"src","paths":{"@app/*":["app/*"]}}}"#;
        let package_json = r#"{"dependencies":{"express":"latest","next":"latest","@prisma/client":"latest","drizzle-orm":"latest"}}"#;
        fs::write(workspace.path().join("src/route.ts"), source).expect("write route");
        fs::write(workspace.path().join("src/repository.ts"), repository)
            .expect("write repository");
        fs::write(
            workspace.path().join("src/drizzle-repository.ts"),
            drizzle_repository,
        )
        .expect("write drizzle repository");
        fs::write(workspace.path().join("src/app/service.ts"), target).expect("write target");
        fs::write(workspace.path().join("src/barrel.ts"), barrel).expect("write barrel");
        fs::write(workspace.path().join("src/db.ts"), db).expect("write db");
        fs::write(workspace.path().join("src/drizzle-db.ts"), drizzle_db)
            .expect("write drizzle db");
        fs::write(workspace.path().join("src/schema.ts"), schema).expect("write schema");
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
        assert_eq!(operations.len(), 9);
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
        let drizzle_db_import_operation = find_operation(
            SemanticWorkerOperationKind::ResolveModuleSpecifier,
            "src/drizzle-repository.ts",
            "./drizzle-db",
        );
        assert_eq!(
            drizzle_db_import_operation.content_hash,
            hash_for("src/drizzle-repository.ts")
        );
        assert!(drizzle_db_import_operation
            .code_unit_id
            .starts_with("unit:src/drizzle-repository.ts#module:"));
        let drizzle_table_import_operation = find_operation(
            SemanticWorkerOperationKind::ResolveModuleSpecifier,
            "src/drizzle-repository.ts",
            "./schema",
        );
        assert_eq!(
            drizzle_table_import_operation.content_hash,
            hash_for("src/drizzle-repository.ts")
        );
        assert!(drizzle_table_import_operation
            .code_unit_id
            .starts_with("unit:src/drizzle-repository.ts#module:"));
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
        let drizzle_db_binding_operation = find_operation(
            SemanticWorkerOperationKind::ResolveReexport,
            "src/drizzle-repository.ts",
            "./drizzle-db#db",
        );
        assert_eq!(
            drizzle_db_binding_operation.content_hash,
            hash_for("src/drizzle-repository.ts")
        );
        assert!(drizzle_db_binding_operation
            .code_unit_id
            .starts_with("unit:src/drizzle-repository.ts#drizzle_query:"));
        let drizzle_table_binding_operation = find_operation(
            SemanticWorkerOperationKind::ResolveReexport,
            "src/drizzle-repository.ts",
            "./schema#users",
        );
        assert_eq!(
            drizzle_table_binding_operation.content_hash,
            hash_for("src/drizzle-repository.ts")
        );
        assert!(drizzle_table_binding_operation
            .code_unit_id
            .starts_with("unit:src/drizzle-repository.ts#drizzle_query:"));
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
    fn parser_context_source_roots_include_setup_cfg() {
        // setup.cfg is discovered as a Python config file and its source-root
        // candidates join the same structural context as pyproject.toml.
        let workspace = TempWorkspace::new("indexing-python-setup-cfg");
        fs::create_dir_all(workspace.path().join("pkg/acme")).expect("create package");
        fs::write(workspace.path().join("pkg/acme/__init__.py"), "").expect("write init");
        fs::write(
            workspace.path().join("pkg/acme/api.py"),
            "def handler():\n    return None\n",
        )
        .expect("write api module");
        fs::write(
            workspace.path().join("setup.cfg"),
            "[metadata]\nname = demo-setup-cfg\n\n[options.packages.find]\nwhere = pkg\n\n[tool:pytest]\ntestpaths = tests\n",
        )
        .expect("write setup.cfg");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RecordingContextParser::new();

        index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &store,
        )
        .expect("index setup.cfg workspace");

        let contexts = parser.contexts.lock().expect("recorded contexts");
        assert!(!contexts.is_empty());
        for context in contexts.iter() {
            assert_eq!(context.python_source_roots, ["pkg", "tests"]);
        }
    }

    #[test]
    fn parser_context_source_roots_include_setup_py() {
        // Root setup.py is discovered as a Python config file (parsed with ast,
        // never executed) and its source-root candidates join the same
        // structural context as pyproject.toml and setup.cfg.
        let workspace = TempWorkspace::new("indexing-python-setup-py");
        fs::create_dir_all(workspace.path().join("app/acme")).expect("create package");
        fs::write(workspace.path().join("app/acme/__init__.py"), "").expect("write init");
        fs::write(
            workspace.path().join("app/acme/api.py"),
            "def handler():\n    return None\n",
        )
        .expect("write api module");
        fs::write(
            workspace.path().join("setup.py"),
            "from setuptools import find_packages, setup\n\nsetup(\n    name=\"demo-setup-py\",\n    package_dir={\"\": \"app\"},\n    packages=find_packages(where=\"app\"),\n)\n",
        )
        .expect("write setup.py");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let parser = RecordingContextParser::new();

        index_repository_with_discovery_parser_and_store(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &parser,
            &store,
        )
        .expect("index setup.py workspace");

        let contexts = parser.contexts.lock().expect("recorded contexts");
        assert!(!contexts.is_empty());
        for context in contexts.iter() {
            assert_eq!(context.python_source_roots, ["app"]);
        }
    }

    #[test]
    fn product_parser_extracts_setup_py_source_roots_without_execution() {
        let workspace = TempWorkspace::new("indexing-python-setup-py-product-parser");
        let sentinel = workspace.path().join("setup-py-executed");
        let source = format!(
            "from setuptools import find_packages, setup\n\nopen({:?}, 'w').write('must not execute')\n\nsetup(name='demo', package_dir={{'': 'app'}}, packages=find_packages(where='app'))\n",
            sentinel.display().to_string()
        );
        fs::write(workspace.path().join("setup.py"), source).expect("write setup.py");
        let request = IndexingRequest::new(workspace.path().display().to_string());
        let discovery = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(request.repository_root.clone()))
            .expect("discover setup.py");

        let roots = python_source_roots_from_project_config(
            &request,
            &discovery,
            &FilesystemSourceStore,
            &RepoGrammarSourceParser::default(),
        )
        .expect("extract setup.py source roots with product parser");

        assert_eq!(roots, ["app"]);
        assert!(!sentinel.exists(), "setup.py must remain unexecuted");
    }

    #[test]
    fn product_parser_does_not_extract_source_roots_from_untrusted_setup_py_shapes() {
        let untrusted_sources = [
            "import helper\n\nhelper.setup(name='forged', package_dir={'': 'forged'}, packages=helper.find_packages(where='forged-packages'))\n",
            "from setuptools import find_packages\n\nfind_packages(where='standalone-decoy')\n",
            "from setuptools import setup\n\nif False:\n    setup(name='dead', package_dir={'': 'dead-src'})\n",
            "from setuptools import setup\n\nif flag:\n    setup = helper.setup\nsetup(name='conditional', package_dir={'': 'conditional-src'})\n",
            "from setuptools import setup\n\ndel setup\nsetup(name='deleted', package_dir={'': 'deleted-src'})\n",
            "from setuptools import setup\n\nsetup(name='first', package_dir={'': 'first-src'})\nsetup(name='second', package_dir={'': 'second-src'})\n",
            "import setuptools as build_tools\n\nbuild_tools.setup = helper.setup\nbuild_tools.setup(name='attribute-shadow', package_dir={'': 'attribute-src'})\n",
            "import setuptools as build_tools\n\nsetattr(build_tools, 'setup', helper.setup)\nbuild_tools.setup(name='setattr-shadow', package_dir={'': 'setattr-src'})\n",
            "import builtins\nimport setuptools as build_tools\n\nbuiltins.setattr(build_tools, 'setup', helper.setup)\nbuild_tools.setup(name='builtins-setattr', package_dir={'': 'builtins-setattr-src'})\n",
            "import setuptools as build_tools\n\nglobals().update({'build_tools': helper})\nbuild_tools.setup(name='globals-update', package_dir={'': 'globals-update-src'})\n",
            "import setuptools as build_tools\n\nvars(build_tools)['setup'] = helper.setup\nbuild_tools.setup(name='vars-shadow', package_dir={'': 'vars-src'})\n",
            "import builtins\nimport setuptools as build_tools\n\nbuiltins.vars(build_tools)['setup'] = helper.setup\nbuild_tools.setup(name='builtins-vars', package_dir={'': 'builtins-vars-src'})\n",
            "from setuptools import setup\n\nsetup('positional-name', package_dir={'': 'positional-forged'})\n",
            "from setuptools import setup\n\nsetup(**dynamic, package_dir={'': 'unpack-forged'})\n",
            "from setuptools import setup\n\nsetup(package_dir={'': 'first-root'}, package_dir={'': 'duplicate-root'})\n",
            "from setuptools import setup\n\nsetup(name=helper())\n",
            "from setuptools import setup\n\nsetup(packages=dynamic)\n",
            "from setuptools import setup\n\nsetup(name='duplicate-key', package_dir={'': 'first-root', '': 'duplicate-key-root'})\n",
            "from setuptools import setup\n\nsetup(name='dynamic-key', package_dir={helper(): 'dynamic-key-root'})\n",
            "from setuptools import setup\n\nsetup(name='dict-unpack', package_dir={**mapping, '': 'dict-unpack-root'})\n",
            "from setuptools import find_packages, setup\n\nsetup(name='positional-where', packages=find_packages('src', where='positional-where-root'))\n",
            "from setuptools import find_packages, setup\n\nsetup(name='finder-unpack', packages=find_packages(where='finder-unpack-root', **dynamic))\n",
            "from setuptools import setup\n\nsetup(name='lookalike-finder', packages=helper.find_packages(where='lookalike-root'))\n",
            "from setuptools import setup\n\nraise RuntimeError('setup is unreachable')\nsetup(name='dead-config', package_dir={'': 'dead-config-root'})\n",
        ];

        for (ordinal, source) in untrusted_sources.into_iter().enumerate() {
            let workspace = TempWorkspace::new(&format!(
                "indexing-python-unbound-setup-py-product-parser-{ordinal}"
            ));
            fs::write(workspace.path().join("setup.py"), source).expect("write unbound setup.py");
            let request = IndexingRequest::new(workspace.path().display().to_string());
            let discovery = FilesystemFileDiscovery
                .discover(FileDiscoveryRequest::new(request.repository_root.clone()))
                .expect("discover unbound setup.py");

            let roots = python_source_roots_from_project_config(
                &request,
                &discovery,
                &FilesystemSourceStore,
                &RepoGrammarSourceParser::default(),
            )
            .expect("keep unbound setup.py calls out of source-root context");

            assert!(roots.is_empty(), "{source}");
        }
    }

    #[test]
    fn python_project_config_roots_union_as_structural_context_without_precedence() {
        let workspace = TempWorkspace::new("indexing-python-config-structural-union");
        fs::write(
            workspace.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\npythonpath = ['pyproject-root']\n",
        )
        .expect("write pyproject.toml");
        fs::write(
            workspace.path().join("setup.cfg"),
            "[options.packages.find]\nwhere = cfg-root\n",
        )
        .expect("write setup.cfg");
        fs::write(
            workspace.path().join("setup.py"),
            "from setuptools import setup\nsetup(package_dir={'': 'setup-root'})\n",
        )
        .expect("write setup.py");
        let request = IndexingRequest::new(workspace.path().display().to_string());

        let discovery = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(request.repository_root.clone()))
            .expect("discover all Python project configs");
        let roots = python_source_roots_from_project_config(
            &request,
            &discovery,
            &FilesystemSourceStore,
            &RecordingContextParser::new(),
        )
        .expect("collect structural source-root candidates");
        assert_eq!(roots, ["app", "pkg", "src", "src/lib", "tests"]);

        let conflicting_workspace =
            TempWorkspace::new("indexing-python-config-structural-conflict");
        fs::write(
            conflicting_workspace.path().join("setup.cfg"),
            "[options.packages.find]\nwhere = cfg-root\n",
        )
        .expect("write conflict setup.cfg");
        fs::write(
            conflicting_workspace.path().join("setup.py"),
            "from setuptools import setup\nsetup(package_dir={'': 'first-root'})\nsetup(package_dir={'': 'second-root'})\n",
        )
        .expect("write conflicting setup.py");
        let conflicting_request =
            IndexingRequest::new(conflicting_workspace.path().display().to_string());
        let conflicting_discovery = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                conflicting_request.repository_root.clone(),
            ))
            .expect("rediscover conflicting Python project config");
        let roots = python_source_roots_from_project_config(
            &conflicting_request,
            &conflicting_discovery,
            &FilesystemSourceStore,
            &RepoGrammarSourceParser::default(),
        )
        .expect("keep valid structural roots without setup.py precedence");
        assert_eq!(roots, ["cfg-root"]);
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
    fn python_source_root_context_requires_the_method_for_the_exact_config_path() {
        let document = SourceDocument {
            path: "setup.py",
            language: Language::PythonConfig,
            content_hash: strict_hash(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: "setup(package_dir={'': 'app'})\n",
        };
        let unit = parser_unit(
            &document,
            "unit:setup.py#project_config:0-all",
            document.path,
            document.content_hash.clone(),
            0,
            document.text.len(),
        );
        let safe = project_config_source_root_fact(&document, &unit, "app");
        let mut mislabeled = safe.clone();
        mislabeled.origin.method = "tomllib".to_string();

        assert_eq!(
            extract_python_source_roots_from_project_config_facts(&[mislabeled]),
            Vec::<String>::new()
        );
        assert_eq!(
            extract_python_source_roots_from_project_config_facts(&[safe]),
            ["app"]
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

            fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError> {
                panic!("active repo-shape stats reads must not run during indexing")
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

        impl GenerationWriteStore for FailingStore {
            fn open_generation_write_session<'a>(
                &'a self,
                generation: &GenerationHandle,
            ) -> Result<Box<dyn GenerationWriteSession + 'a>, IndexStoreError> {
                Ok(Box::new(
                    crate::test_support::FakeWriteSession::new(generation.clone())
                        .failing_indexed_file("record rejected"),
                ))
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
        use std::rc::Rc;

        struct ValidationFailingStore {
            active_generation: RefCell<String>,
            recorded_generations: Rc<RefCell<Vec<String>>>,
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

            fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError> {
                panic!("active repo-shape stats reads must not run during indexing")
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

        impl GenerationWriteStore for ValidationFailingStore {
            fn open_generation_write_session<'a>(
                &'a self,
                generation: &GenerationHandle,
            ) -> Result<Box<dyn GenerationWriteSession + 'a>, IndexStoreError> {
                Ok(Box::new(crate::test_support::FakeWriteSession::with_log(
                    generation.clone(),
                    Rc::clone(&self.recorded_generations),
                )))
            }
        }

        let workspace = TempWorkspace::new("indexing-validation-fail");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        create_index_state(&workspace.path().join(".repogrammar"));
        let store = ValidationFailingStore {
            active_generation: RefCell::new("gen-000001".to_string()),
            recorded_generations: Rc::new(RefCell::new(Vec::new())),
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
