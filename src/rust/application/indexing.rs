//! Indexing use-case boundary.

use crate::application::family::{
    build_family_claims, family_storage_records, python_support_target_is_role_compatible,
};
use crate::core::model::{
    CodeUnit, CodeUnitId, Evidence, FactCertainty, FactOrigin, IrEdge, IrNode, Language,
    Provenance, RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange, SymbolId,
};
use crate::error::RepoGrammarError;
use crate::ports::family_store::FamilyStore;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};
use crate::ports::index_store::{
    GenerationHandle, IndexStore, IndexedCodeUnitRecord, IndexedFileRecord, IndexedIrEdgeRecord,
    IndexedIrNodeRecord, IndexedSemanticFactRecord,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, ParserProjectFileContext, SourceDocument,
    SourceParser,
};
use crate::ports::semantic_worker::{SemanticWorker, SemanticWorkerError, SemanticWorkerRequest};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingRequest {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
    pub max_file_bytes: u64,
}

impl IndexingRequest {
    pub fn new(repository_root: impl Into<String>) -> Self {
        Self {
            repository_root: repository_root.into(),
            state_dir_override: None,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
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
    pub warnings: Vec<String>,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: None,
            semantic_worker: None,
            family_store: None,
        },
        store,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            semantic_worker: None,
            family_store: None,
        },
        store,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            semantic_worker: None,
            family_store: Some(store),
        },
        store,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: None,
            semantic_worker: Some(semantic_worker),
            family_store: None,
        },
        store,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            semantic_worker: Some(semantic_worker),
            family_store: None,
        },
        store,
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
    index_repository_with_optional_semantic_worker(
        request,
        discovery,
        source_store,
        parser,
        IndexingPipelineOptions {
            framework_roles: Some(framework_roles),
            semantic_worker: Some(semantic_worker),
            family_store: Some(store),
        },
        store,
    )
}

fn index_repository_with_optional_semantic_worker(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    options: IndexingPipelineOptions<'_>,
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

    let mut indexed_units = 0usize;
    let mut indexed_code_units = Vec::new();
    let mut parser_semantic_facts = Vec::new();
    let mut framework_role_facts = Vec::new();
    let mut warnings = report.warnings.clone();
    let parser_context = parser_project_context(&request, &report, source_store)?;
    for file in &report.files {
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
    }

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

    let (semantic_worker, worker_facts) = record_semantic_worker_facts(
        &request,
        &report,
        &generation,
        options.semantic_worker,
        store,
        &mut warnings,
        parser_fact_count + framework_fact_count + derived_python_support_fact_count,
    )?;
    let worker_semantic_facts = worker_facts.len();

    if let Some(family_store) = options.family_store {
        let mut family_facts = Vec::with_capacity(
            framework_role_facts.len() + derived_python_support_facts.len() + worker_facts.len(),
        );
        family_facts.extend(framework_role_facts.iter().cloned());
        family_facts.extend(derived_python_support_facts);
        family_facts.extend(worker_facts);
        record_family_claims(
            family_store,
            &generation,
            &indexed_code_units,
            &family_facts,
        )?;
    }

    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexed_units,
        semantic_facts: parser_fact_count
            + framework_fact_count
            + derived_python_support_fact_count
            + worker_semantic_facts,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        semantic_worker,
        warnings,
    })
}

#[derive(Clone, Copy)]
struct IndexingPipelineOptions<'a> {
    framework_roles: Option<&'a dyn FrameworkRoleDetector>,
    semantic_worker: Option<&'a dyn SemanticWorker>,
    family_store: Option<&'a dyn FamilyStore>,
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
) -> Result<ParserProjectContext, RepoGrammarError> {
    let python_module_paths = report
        .files
        .iter()
        .filter(|file| file.language == DiscoveredLanguage::Python)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
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
        python_source_roots: Vec::new(),
        python_conftest_files,
    })
}

fn is_python_conftest_path(path: &str) -> bool {
    path == "conftest.py" || path.ends_with("/conftest.py")
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
    Ok(SemanticFact {
        kind,
        subject: unit.id.clone(),
        target: Some(SymbolId::new(target).map_err(RepoGrammarError::InvalidInput)?),
        origin: FactOrigin {
            engine: "repogrammar-python-derived".to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "bounded_ast_anchor_v1".to_string(),
        },
        certainty: FactCertainty::DataflowDerived,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.clone()).map_err(RepoGrammarError::InvalidInput)?,
            SourceRange::new(unit.start_byte, unit.end_byte)
                .map_err(RepoGrammarError::InvalidInput)?,
            Provenance::new(
                &unit.path,
                unit.content_hash.clone(),
                repository_revision.clone(),
            )
            .map_err(RepoGrammarError::InvalidInput)?,
            "bounded Python framework anchor support",
        )
        .map_err(RepoGrammarError::InvalidInput)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "derived_from=cpython_ast_structural_anchors".to_string(),
            format!("framework_role={framework_role}"),
        ],
    })
}

fn record_semantic_worker_facts(
    request: &IndexingRequest,
    discovery_report: &FileDiscoveryReport,
    generation: &GenerationHandle,
    semantic_worker: Option<&dyn SemanticWorker>,
    store: &impl IndexStore,
    warnings: &mut Vec<String>,
    fact_id_offset: usize,
) -> Result<(SemanticWorkerRunStatus, Vec<SemanticFact>), RepoGrammarError> {
    let Some(semantic_worker) = semantic_worker else {
        return Ok((SemanticWorkerRunStatus::Deferred, Vec::new()));
    };

    if discovery_report.files.is_empty() {
        return Ok((SemanticWorkerRunStatus::Deferred, Vec::new()));
    }

    let changed_files = discovery_report
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let mut facts = match semantic_worker.analyze_project(SemanticWorkerRequest {
        project_root: request.repository_root.clone(),
        changed_files,
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
    record_semantic_facts(store, generation, fact_id_offset, &facts)?;
    Ok((SemanticWorkerRunStatus::Complete, facts))
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
    ) {
        return Err(RepoGrammarError::InvalidInput(
            "parser semantic facts must stay structural or unknown".to_string(),
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
    use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::application::query::{assess_semantic_fact_readiness, SemanticFactReadinessRequest};
    use crate::core::model::{
        CodeUnitId, CodeUnitKind, ContentHash, Evidence, FactCertainty, FactOrigin, IrEdgeLabel,
        IrNodeId, Provenance, RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange,
        UnknownReasonCode,
    };
    use crate::core::policy::freshness::ClaimInputReadiness;
    use crate::ports::family_store::FamilyStore;
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveSemanticFacts, GenerationHandle, IndexStore, IndexStoreError, IndexedCodeUnitRecord,
        IndexedFileRecord, IndexedSemanticFactRecord, StorageInspection, STORAGE_SCHEMA_VERSION,
    };
    use crate::ports::parser::{ParseDiagnostic, ParseDiagnosticSeverity, ParserProjectContext};
    use crate::ports::semantic_worker::{
        SemanticWorker, SemanticWorkerError, SemanticWorkerRequest,
    };
    use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceText};
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

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
            Ok(ParseReport {
                units: vec![unit],
                ir_nodes: vec![ir_node],
                ir_edges: Vec::new(),
                semantic_facts: Vec::new(),
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
        semantic_fact_for_unit_with_target(content_hash, code_unit_id, path, end_byte, None)
    }

    fn semantic_fact_for_unit_with_target(
        content_hash: ContentHash,
        code_unit_id: &str,
        path: &str,
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
                SourceRange::new(0, end_byte).expect("valid range"),
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

    fn semantic_fact_count(state: &Path, generation_id: &str) -> u32 {
        let database = state
            .join("generations")
            .join(generation_id)
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        connection
            .query_row("SELECT count(*) FROM semantic_facts", [], |row| row.get(0))
            .expect("count semantic facts")
    }

    fn semantic_fact_ids(state: &Path, generation_id: &str) -> Vec<(String, String)> {
        let database = state
            .join("generations")
            .join(generation_id)
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let mut statement = connection
            .prepare("SELECT fact_id, evidence_id FROM semantic_facts ORDER BY fact_id")
            .expect("prepare fact id query");
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query semantic fact ids")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect semantic fact ids")
    }

    fn create_index_state(state: &Path) {
        fs::create_dir_all(state.join("generations")).expect("create generations");
        fs::create_dir_all(state.join("tmp")).expect("create tmp");
        fs::create_dir_all(state.join("locks")).expect("create locks");
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
            "sqlalchemy.orm.Session.scalar",
            "sqlalchemy.orm.Session.scalars",
            "sqlalchemy.ext.asyncio.AsyncSession.commit",
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
        assert_eq!(outcome.semantic_facts, 5);
        assert_eq!(outcome.semantic_worker, SemanticWorkerRunStatus::Deferred);
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));

        let facts = store
            .list_active_semantic_facts()
            .expect("list semantic facts");
        assert_eq!(facts.generation_id, "gen-000001");
        assert_eq!(facts.facts.len(), 5);
        assert!(facts.facts.iter().all(|fact| {
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
        let targets = facts
            .facts
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
        let database = state
            .join("generations")
            .join("gen-000001")
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let family_bound_evidence: u32 = connection
            .query_row(
                "SELECT count(*) FROM evidence WHERE family_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("count family-bound evidence");
        let families: u32 = connection
            .query_row("SELECT count(*) FROM families", [], |row| row.get(0))
            .expect("count families");
        let family_members: u32 = connection
            .query_row("SELECT count(*) FROM family_members", [], |row| row.get(0))
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
        assert_eq!(readiness.facts.len(), 5);
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
            "app.get('/users', (req, res) => { res.json([]); });\n",
        )
        .expect("write users route");
        fs::write(
            workspace.path().join("accounts.ts"),
            "app.get('/accounts', (req, res) => { res.json([]); });\n",
        )
        .expect("write accounts route");
        let state = workspace.path().join(".repogrammar");
        create_index_state(&state);
        let store = SqliteIndexStore::new(&state);
        let detector = SyntaxFrameworkRoleDetector;
        let (expected_files, facts) = semantic_support_facts_for_express_routes(&workspace);
        assert_eq!(facts.len(), 2);
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
        assert_eq!(outcome.semantic_facts, 4);
        let families = store.list_active_families().expect("list families");
        assert_eq!(families.generation_id, "gen-000001");
        assert_eq!(families.families.len(), 1);
        assert_eq!(families.families[0].classification, "DOMINANT_PATTERN");
        let family = store
            .show_family(&families.families[0].family_id)
            .expect("show family")
            .expect("family exists");
        assert_eq!(family.members.len(), 2);
        assert_eq!(family.evidence.len(), 2);
        assert!(family
            .members
            .iter()
            .all(|member| member.role == "framework:express.route_handler"));
        let debug = format!("{family:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("res.json"));
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
        assert_eq!(
            fs::read_to_string(state.join("current-generation"))
                .expect("read active generation")
                .trim(),
            "gen-000001"
        );
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

        assert_eq!(outcome.discovered_files, 4);
        let contexts = parser.contexts.lock().expect("recorded contexts");
        assert_eq!(contexts.len(), 4);
        let expected = vec![
            "src/acme/__init__.py".to_string(),
            "src/acme/api.py".to_string(),
            "src/acme/services/users.py".to_string(),
            "tests/conftest.py".to_string(),
        ];
        for context in contexts.iter() {
            assert_eq!(context.python_module_paths, expected);
            assert!(context.python_source_roots.is_empty());
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
        }
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
            assert_eq!(
                fs::read_to_string(state.join("current-generation"))
                    .expect("read active generation")
                    .trim(),
                "gen-000001"
            );
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
