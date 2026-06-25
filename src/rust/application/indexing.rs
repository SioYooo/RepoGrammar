//! Indexing use-case boundary.

use crate::core::model::{
    CodeUnit, IrEdge, IrNode, Language, RepositoryRevision, SemanticFact, SymbolId,
};
use crate::error::RepoGrammarError;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, DEFAULT_MAX_FILE_BYTES,
};
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};
use crate::ports::index_store::{
    GenerationHandle, IndexStore, IndexedCodeUnitRecord, IndexedFileRecord, IndexedIrEdgeRecord,
    IndexedIrNodeRecord, IndexedSemanticFactRecord,
};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument, SourceParser};
use crate::ports::semantic_worker::{SemanticWorker, SemanticWorkerError, SemanticWorkerRequest};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError};
use std::collections::BTreeSet;

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
        None,
        None,
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
        Some(framework_roles),
        None,
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
        None,
        Some(semantic_worker),
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
        Some(framework_roles),
        Some(semantic_worker),
        store,
    )
}

fn index_repository_with_optional_semantic_worker(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
    source_store: &impl SourceStore,
    parser: &impl SourceParser,
    framework_roles: Option<&dyn FrameworkRoleDetector>,
    semantic_worker: Option<&dyn SemanticWorker>,
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
    let mut framework_role_facts = Vec::new();
    let mut warnings = report.warnings.clone();
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
        let parse_outcome = record_parse_report(
            store,
            &generation,
            file,
            &source.text,
            parse_report,
            framework_roles,
            &mut warnings,
        )?;
        indexed_units += parse_outcome.indexed_units;
        framework_role_facts.extend(parse_outcome.framework_role_facts);
    }

    sort_semantic_facts(&mut framework_role_facts);
    let framework_fact_count = record_semantic_facts(store, &generation, 0, &framework_role_facts)?;

    let (semantic_worker, worker_semantic_facts) = record_semantic_worker_facts(
        &request,
        &report,
        &generation,
        semantic_worker,
        store,
        &mut warnings,
        framework_fact_count,
    )?;

    crate::application::storage::validate_index_generation(store, &generation)?;
    crate::application::storage::activate_index_generation(store, &generation)?;

    Ok(IndexingOutcome {
        indexed_units,
        semantic_facts: framework_fact_count + worker_semantic_facts,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        active_generation: Some(generation.generation_id),
        semantic_worker,
        warnings,
    })
}

struct ParseStorageOutcome {
    indexed_units: usize,
    framework_role_facts: Vec<SemanticFact>,
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
    let framework_role_facts = match framework_roles {
        Some(detector) => detector
            .detect_roles(&parse_report.units)
            .map_err(framework_role_error)?,
        None => Vec::new(),
    };

    let mut count = 0usize;
    for unit in &parse_report.units {
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
        framework_role_facts,
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
) -> Result<(SemanticWorkerRunStatus, usize), RepoGrammarError> {
    let Some(semantic_worker) = semantic_worker else {
        return Ok((SemanticWorkerRunStatus::Deferred, 0));
    };

    if discovery_report.files.is_empty() {
        return Ok((SemanticWorkerRunStatus::Deferred, 0));
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
            return Ok((status, 0));
        }
    };

    sort_semantic_facts(&mut facts);
    let count = record_semantic_facts(store, generation, fact_id_offset, &facts)?;
    Ok((SemanticWorkerRunStatus::Complete, count))
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
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveSemanticFacts, GenerationHandle, IndexStore, IndexStoreError, IndexedCodeUnitRecord,
        IndexedFileRecord, IndexedSemanticFactRecord, StorageInspection, STORAGE_SCHEMA_VERSION,
    };
    use crate::ports::parser::{ParseDiagnostic, ParseDiagnosticSeverity};
    use crate::ports::semantic_worker::{
        SemanticWorker, SemanticWorkerError, SemanticWorkerRequest,
    };
    use crate::ports::source_store::{SourceStore, SourceText};
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
                diagnostics: Vec::new(),
            })
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
        SemanticFact {
            kind: SemanticFactKind::ResolvedImport,
            subject: format!("{path}#import:express"),
            target: None,
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
