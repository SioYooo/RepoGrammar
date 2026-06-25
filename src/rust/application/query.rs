//! Query use-case boundary for finding repository analogues.

use crate::application::repository::{
    RepositoryImplementationStatus, RepositoryStatus, RepositoryStatusReport,
};
use crate::core::model::{FactCertainty, PatternClassification, SemanticFactKind};
use crate::core::policy::freshness::{
    content_hash_freshness, semantic_fact_claim_input_readiness, ClaimInputReadiness,
};
use crate::error::RepoGrammarError;
use crate::ports::index_store::{
    IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedSemanticFactRecord,
};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueQuery {
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueResult {
    pub classification: PatternClassification,
}

pub fn find_analogues(_query: AnalogueQuery) -> Result<Vec<AnalogueResult>, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("find_analogues"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryPreflightOperation {
    PatternFamilyQuery,
    ActiveIndexInventory,
}

impl QueryPreflightOperation {
    pub fn command_is_implemented(self) -> bool {
        matches!(self, Self::ActiveIndexInventory)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFallbackReport {
    pub reason: &'static str,
    pub guidance: &'static str,
    pub implemented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryPreflightReport {
    Ready,
    Fallback(QueryFallbackReport),
}

pub fn repository_status_unavailable_fallback(
    operation: QueryPreflightOperation,
) -> QueryFallbackReport {
    QueryFallbackReport {
        reason: "repository status is unavailable",
        guidance: "run repogrammar doctor",
        implemented: operation.command_is_implemented(),
    }
}

pub fn query_preflight(
    operation: QueryPreflightOperation,
    status_report: &RepositoryStatusReport,
) -> QueryPreflightReport {
    match &status_report.status {
        RepositoryStatus::NotInitialized => fallback(
            "repository is not initialized",
            "run repogrammar init",
            operation.command_is_implemented(),
        ),
        RepositoryStatus::CorruptedManifest => {
            QueryPreflightReport::Fallback(repository_status_unavailable_fallback(operation))
        }
        RepositoryStatus::Initialized { active_generation } => {
            if !status_report.missing_subdirs.is_empty()
                || status_report.storage == RepositoryImplementationStatus::Unhealthy
            {
                return QueryPreflightReport::Fallback(repository_status_unavailable_fallback(
                    operation,
                ));
            }

            match operation {
                QueryPreflightOperation::ActiveIndexInventory
                    if active_generation == "none"
                        || active_generation == "not implemented"
                        || !inventory_indexing_is_readable(status_report.indexing) =>
                {
                    fallback("no active index generation", "run repogrammar index", true)
                }
                QueryPreflightOperation::ActiveIndexInventory => QueryPreflightReport::Ready,
                QueryPreflightOperation::PatternFamilyQuery => fallback(
                    "query execution requires pattern-family evidence",
                    "run repogrammar index after pattern-family indexing is implemented",
                    false,
                ),
            }
        }
    }
}

fn fallback(
    reason: &'static str,
    guidance: &'static str,
    implemented: bool,
) -> QueryPreflightReport {
    QueryPreflightReport::Fallback(QueryFallbackReport {
        reason,
        guidance,
        implemented,
    })
}

fn inventory_indexing_is_readable(status: RepositoryImplementationStatus) -> bool {
    matches!(
        status,
        RepositoryImplementationStatus::FileManifestOnly
            | RepositoryImplementationStatus::SyntaxOnlyCodeUnits
    )
}

fn inventory_indexing_for_unit_count(unit_count: usize) -> &'static str {
    if unit_count == 0 {
        "file_manifest_only"
    } else {
        "syntax_only_code_units"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFilesReport {
    pub active_generation: String,
    pub indexing: String,
    pub files: Vec<IndexedFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedCodeUnitsReport {
    pub active_generation: String,
    pub indexing: String,
    pub units: Vec<IndexedCodeUnitRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedSemanticFactsReport {
    pub active_generation: String,
    pub facts: Vec<IndexedSemanticFactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFactReadinessRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFactReadinessReport {
    pub active_generation: String,
    pub facts: Vec<SemanticFactReadinessRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFactReadinessRecord {
    pub fact_id: String,
    pub readiness: ClaimInputReadiness,
}

pub fn list_indexed_files(store: &impl IndexStore) -> Result<IndexedFilesReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    Ok(IndexedFilesReport {
        active_generation: snapshot.generation_id,
        indexing: inventory_indexing_for_unit_count(snapshot.units.len()).to_string(),
        files: snapshot.files,
    })
}

pub fn list_code_units(
    store: &impl IndexStore,
) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    Ok(IndexedCodeUnitsReport {
        active_generation: snapshot.generation_id,
        indexing: inventory_indexing_for_unit_count(snapshot.units.len()).to_string(),
        units: snapshot.units,
    })
}

pub fn list_semantic_facts(
    store: &impl IndexStore,
) -> Result<IndexedSemanticFactsReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    Ok(IndexedSemanticFactsReport {
        active_generation: snapshot.generation_id,
        facts: snapshot.semantic_facts,
    })
}

pub fn assess_semantic_fact_readiness(
    request: SemanticFactReadinessRequest,
    store: &impl IndexStore,
    source_store: &impl SourceStore,
) -> Result<SemanticFactReadinessReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    let mut facts = Vec::with_capacity(snapshot.semantic_facts.len());
    for fact in snapshot.semantic_facts {
        let fact_path = fact.path;
        let source = source_store.read_source(SourceReadRequest {
            repository_root: request.repository_root.clone(),
            path: fact_path.clone(),
            expected_content_hash: fact.content_hash.clone(),
            max_file_bytes: request.max_file_bytes,
        });
        let current_hash = match source {
            Ok(source) if source.path == fact_path => Some(source.content_hash),
            Ok(_) => {
                return Err(RepoGrammarError::InvalidInput(
                    "source freshness response is invalid".to_string(),
                ));
            }
            Err(SourceStoreError::InvalidRequest(_)) => {
                return Err(RepoGrammarError::InvalidInput(
                    "source freshness request is invalid".to_string(),
                ));
            }
            Err(_) => None,
        };
        let freshness = content_hash_freshness(&fact.content_hash, current_hash.as_ref());
        let kind = SemanticFactKind::parse_protocol_str(&fact.kind).map_err(|_| {
            RepoGrammarError::InvalidInput("stored semantic fact kind is invalid".to_string())
        })?;
        let certainty = FactCertainty::parse_protocol_str(&fact.certainty).map_err(|_| {
            RepoGrammarError::InvalidInput("stored semantic fact certainty is invalid".to_string())
        })?;
        facts.push(SemanticFactReadinessRecord {
            fact_id: fact.fact_id,
            readiness: semantic_fact_claim_input_readiness(kind, certainty, freshness),
        });
    }
    Ok(SemanticFactReadinessReport {
        active_generation: snapshot.generation_id,
        facts,
    })
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
    use crate::application::repository::RepositoryManifestStatus;
    use crate::core::model::{ContentHash, UnknownClass, UnknownReasonCode};
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveSemanticFacts, GenerationHandle, IndexedIrEdgeRecord, IndexedIrNodeRecord,
        StorageInspection,
    };
    use crate::ports::source_store::SourceText;

    struct FakeStore {
        facts: Vec<IndexedSemanticFactRecord>,
        files: Vec<IndexedFileRecord>,
        units: Vec<IndexedCodeUnitRecord>,
    }

    impl FakeStore {
        fn new(facts: Vec<IndexedSemanticFactRecord>) -> Self {
            Self {
                facts,
                files: Vec::new(),
                units: Vec::new(),
            }
        }

        fn with_files(mut self, files: Vec<IndexedFileRecord>) -> Self {
            self.files = files;
            self
        }

        fn with_units(mut self, units: Vec<IndexedCodeUnitRecord>) -> Self {
            self.units = units;
            self
        }
    }

    impl IndexStore for FakeStore {
        fn prepare_next_generation(&self) -> Result<GenerationHandle, IndexStoreError> {
            panic!("query read tests must not prepare generations")
        }

        fn record_indexed_file(
            &self,
            _generation: &GenerationHandle,
            _file: &IndexedFileRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write indexed files")
        }

        fn record_code_unit(
            &self,
            _generation: &GenerationHandle,
            _unit: &IndexedCodeUnitRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write code units")
        }

        fn record_ir_node(
            &self,
            _generation: &GenerationHandle,
            _node: &IndexedIrNodeRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write IR nodes")
        }

        fn record_ir_edge(
            &self,
            _generation: &GenerationHandle,
            _edge: &IndexedIrEdgeRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write IR edges")
        }

        fn record_semantic_fact(
            &self,
            _generation: &GenerationHandle,
            _fact: &IndexedSemanticFactRecord,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not write semantic facts")
        }

        fn list_active_indexed_files(&self) -> Result<ActiveIndexedFiles, IndexStoreError> {
            Ok(ActiveIndexedFiles {
                generation_id: "gen-000001".to_string(),
                files: self.files.clone(),
            })
        }

        fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
            Ok(ActiveCodeUnits {
                generation_id: "gen-000001".to_string(),
                units: self.units.clone(),
            })
        }

        fn list_active_semantic_facts(&self) -> Result<ActiveSemanticFacts, IndexStoreError> {
            Ok(ActiveSemanticFacts {
                generation_id: "gen-000001".to_string(),
                facts: self.facts.clone(),
            })
        }

        fn list_active_ir_graph(&self) -> Result<ActiveIrGraph, IndexStoreError> {
            Ok(ActiveIrGraph {
                generation_id: "gen-000001".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }

        fn load_active_claim_input_snapshot(
            &self,
        ) -> Result<ActiveClaimInputSnapshot, IndexStoreError> {
            Ok(ActiveClaimInputSnapshot {
                generation_id: "gen-000001".to_string(),
                files: self.files.clone(),
                units: self.units.clone(),
                ir_nodes: Vec::new(),
                ir_edges: Vec::new(),
                semantic_facts: self.facts.clone(),
            })
        }

        fn validate_generation(
            &self,
            _generation: &GenerationHandle,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not validate generations")
        }

        fn activate_generation(
            &self,
            _generation: &GenerationHandle,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not activate generations")
        }

        fn inspect(&self) -> Result<StorageInspection, IndexStoreError> {
            panic!("query read tests must not inspect storage")
        }
    }

    fn semantic_fact() -> IndexedSemanticFactRecord {
        semantic_fact_with_certainty("SEMANTIC")
    }

    fn semantic_fact_with_certainty(certainty: &str) -> IndexedSemanticFactRecord {
        IndexedSemanticFactRecord {
            fact_id: "semantic-fact:000000".to_string(),
            kind: "RESOLVED_IMPORT".to_string(),
            subject: "src/a.ts#import:express".to_string(),
            target: None,
            certainty: certainty.to_string(),
            origin_engine: "typescript".to_string(),
            origin_engine_version: "6.0.0".to_string(),
            origin_method: "compiler_api".to_string(),
            assumptions: Vec::new(),
            evidence_id: "semantic-evidence:000000".to_string(),
            code_unit_id: "unit:src/a.ts#module:0-10".to_string(),
            path: "src/a.ts".to_string(),
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid content hash"),
            start_byte: 0,
            end_byte: 10,
            note: "compiler resolved import target".to_string(),
        }
    }

    fn indexed_file(path: &str) -> IndexedFileRecord {
        IndexedFileRecord {
            path: path.to_string(),
            content_hash: semantic_fact().content_hash,
            size_bytes: 10,
            language: "typescript".to_string(),
        }
    }

    fn indexed_unit(path: &str) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#module:0-10"),
            path: path.to_string(),
            language: "typescript".to_string(),
            kind: "module".to_string(),
            start_byte: 0,
            end_byte: 10,
            content_hash: semantic_fact().content_hash,
        }
    }

    struct StaticSourceStore {
        result: Result<SourceText, SourceStoreError>,
    }

    impl SourceStore for StaticSourceStore {
        fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
            assert_eq!(request.repository_root, "/repo");
            assert_eq!(request.path, "src/a.ts");
            assert_eq!(request.max_file_bytes, 1024);
            self.result.clone()
        }
    }

    fn source_store_with_hash(content_hash: ContentHash) -> StaticSourceStore {
        StaticSourceStore {
            result: Ok(SourceText {
                path: "src/a.ts".to_string(),
                content_hash,
                text: "source is not inspected by readiness tests".to_string(),
            }),
        }
    }

    fn missing_source_store(message: &str) -> StaticSourceStore {
        StaticSourceStore {
            result: Err(SourceStoreError::Missing(message.to_string())),
        }
    }

    fn hash_mismatch_source_store(message: &str) -> StaticSourceStore {
        StaticSourceStore {
            result: Err(SourceStoreError::HashMismatch(message.to_string())),
        }
    }

    fn readiness_request() -> SemanticFactReadinessRequest {
        SemanticFactReadinessRequest {
            repository_root: "/repo".to_string(),
            max_file_bytes: 1024,
        }
    }

    fn status_report(
        status: RepositoryStatus,
        storage: RepositoryImplementationStatus,
        active_indexing: RepositoryImplementationStatus,
        missing_subdirs: Vec<&str>,
    ) -> RepositoryStatusReport {
        RepositoryStatusReport {
            state_dir: ".repogrammar".to_string(),
            status,
            manifest: RepositoryManifestStatus::Valid,
            missing_subdirs: missing_subdirs.into_iter().map(str::to_string).collect(),
            storage,
            indexing: active_indexing,
            storage_inspection: None,
            storage_error: None,
        }
    }

    fn fallback_report(report: QueryPreflightReport) -> QueryFallbackReport {
        match report {
            QueryPreflightReport::Fallback(fallback) => fallback,
            QueryPreflightReport::Ready => panic!("expected fallback report"),
        }
    }

    #[test]
    fn query_preflight_reports_missing_repository_without_storage_claims() {
        let status = status_report(
            RepositoryStatus::NotInitialized,
            RepositoryImplementationStatus::NotImplemented,
            RepositoryImplementationStatus::NotImplemented,
            Vec::new(),
        );

        let pattern = fallback_report(query_preflight(
            QueryPreflightOperation::PatternFamilyQuery,
            &status,
        ));
        assert_eq!(pattern.reason, "repository is not initialized");
        assert_eq!(pattern.guidance, "run repogrammar init");
        assert!(!pattern.implemented);

        let inventory = fallback_report(query_preflight(
            QueryPreflightOperation::ActiveIndexInventory,
            &status,
        ));
        assert_eq!(inventory.reason, "repository is not initialized");
        assert_eq!(inventory.guidance, "run repogrammar init");
        assert!(inventory.implemented);
    }

    #[test]
    fn query_preflight_treats_unreadable_repository_state_as_status_unavailable() {
        for status in [
            status_report(
                RepositoryStatus::CorruptedManifest,
                RepositoryImplementationStatus::NotImplemented,
                RepositoryImplementationStatus::NotImplemented,
                Vec::new(),
            ),
            status_report(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                RepositoryImplementationStatus::Unhealthy,
                RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
                Vec::new(),
            ),
            status_report(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                RepositoryImplementationStatus::Available,
                RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
                vec!["generations"],
            ),
        ] {
            let fallback = fallback_report(query_preflight(
                QueryPreflightOperation::ActiveIndexInventory,
                &status,
            ));

            assert_eq!(fallback.reason, "repository status is unavailable");
            assert_eq!(fallback.guidance, "run repogrammar doctor");
            assert!(fallback.implemented);
        }
    }

    #[test]
    fn query_preflight_requires_active_generation_for_inventory_commands() {
        for active_generation in ["none", "not implemented"] {
            let status = status_report(
                RepositoryStatus::Initialized {
                    active_generation: active_generation.to_string(),
                },
                RepositoryImplementationStatus::Available,
                RepositoryImplementationStatus::FileManifestOnly,
                Vec::new(),
            );

            let fallback = fallback_report(query_preflight(
                QueryPreflightOperation::ActiveIndexInventory,
                &status,
            ));

            assert_eq!(fallback.reason, "no active index generation");
            assert_eq!(fallback.guidance, "run repogrammar index");
            assert!(fallback.implemented);
        }
    }

    #[test]
    fn query_preflight_allows_inventory_reads_for_active_generation() {
        for indexing in [
            RepositoryImplementationStatus::FileManifestOnly,
            RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
        ] {
            let status = status_report(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                RepositoryImplementationStatus::Available,
                indexing,
                Vec::new(),
            );

            assert_eq!(
                query_preflight(QueryPreflightOperation::ActiveIndexInventory, &status),
                QueryPreflightReport::Ready
            );
        }
    }

    #[test]
    fn query_preflight_keeps_pattern_queries_fallback_only_without_family_evidence() {
        let status = status_report(
            RepositoryStatus::Initialized {
                active_generation: "gen-000001".to_string(),
            },
            RepositoryImplementationStatus::Available,
            RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
            Vec::new(),
        );

        let fallback = fallback_report(query_preflight(
            QueryPreflightOperation::PatternFamilyQuery,
            &status,
        ));

        assert_eq!(
            fallback.reason,
            "query execution requires pattern-family evidence"
        );
        assert_eq!(
            fallback.guidance,
            "run repogrammar index after pattern-family indexing is implemented"
        );
        assert!(!fallback.implemented);
    }

    #[test]
    fn list_semantic_facts_delegates_through_index_store() {
        let report = list_semantic_facts(&FakeStore::new(vec![semantic_fact()]))
            .expect("list semantic facts");

        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.facts, vec![semantic_fact()]);
    }

    #[test]
    fn inventory_reports_are_derived_from_active_claim_input_snapshot() {
        let store = FakeStore::new(Vec::new()).with_files(vec![indexed_file("src/a.ts")]);

        let report = list_indexed_files(&store).expect("list indexed files");
        assert_eq!(
            report,
            IndexedFilesReport {
                active_generation: "gen-000001".to_string(),
                indexing: "file_manifest_only".to_string(),
                files: vec![indexed_file("src/a.ts")],
            }
        );

        let store = FakeStore::new(Vec::new()).with_units(vec![indexed_unit("src/a.ts")]);
        let report = list_code_units(&store).expect("list code units");
        assert_eq!(
            report,
            IndexedCodeUnitsReport {
                active_generation: "gen-000001".to_string(),
                indexing: "syntax_only_code_units".to_string(),
                units: vec![indexed_unit("src/a.ts")],
            }
        );
    }

    #[test]
    fn fresh_semantic_fact_is_only_eligible_future_claim_input() {
        let fact = semantic_fact();
        let report = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact.clone()]),
            &source_store_with_hash(fact.content_hash),
        )
        .expect("assess readiness");

        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.facts.len(), 1);
        assert_eq!(report.facts[0].fact_id, "semantic-fact:000000");
        assert_eq!(
            report.facts[0].readiness,
            ClaimInputReadiness::EligibleInput
        );
        assert!(matches!(
            find_analogues(AnalogueQuery {
                target_path: "src/a.ts".to_string()
            }),
            Err(RepoGrammarError::NotImplemented("find_analogues"))
        ));
    }

    #[test]
    fn changed_source_blocks_semantic_fact_with_stale_evidence_unknown() {
        let fact = semantic_fact();
        let changed_hash = ContentHash::new(
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .expect("valid changed hash");
        let report = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact]),
            &source_store_with_hash(changed_hash),
        )
        .expect("assess readiness");

        let ClaimInputReadiness::Blocked { unknown } = &report.facts[0].readiness else {
            panic!("changed source must block semantic fact readiness");
        };
        assert_eq!(unknown.class, UnknownClass::Blocking);
        assert_eq!(unknown.reason, UnknownReasonCode::StaleEvidence);
        assert_eq!(unknown.affected_claim, "semantic_fact_claim_input");
        assert_eq!(unknown.recovery.as_deref(), Some("run repogrammar sync"));
    }

    #[test]
    fn missing_source_blocks_semantic_fact_without_leaking_source_error() {
        let fact = semantic_fact();
        let report = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact]),
            &missing_source_store("/repo/src/a.ts vanished with secret detail"),
        )
        .expect("assess readiness");

        let debug = format!("{report:?}");
        assert!(!debug.contains("/repo"));
        assert!(!debug.contains("secret detail"));
        let ClaimInputReadiness::Blocked { unknown } = &report.facts[0].readiness else {
            panic!("missing source must block semantic fact readiness");
        };
        assert_eq!(unknown.reason, UnknownReasonCode::StaleEvidence);
    }

    #[test]
    fn hash_mismatch_blocks_semantic_fact_without_leaking_source_error() {
        let fact = semantic_fact();
        let report = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact]),
            &hash_mismatch_source_store("/repo/src/a.ts hash mismatch with secret detail"),
        )
        .expect("assess readiness");

        let debug = format!("{report:?}");
        assert!(!debug.contains("/repo"));
        assert!(!debug.contains("secret detail"));
        let ClaimInputReadiness::Blocked { unknown } = &report.facts[0].readiness else {
            panic!("hash mismatch must block semantic fact readiness");
        };
        assert_eq!(unknown.reason, UnknownReasonCode::StaleEvidence);
        assert_eq!(unknown.recovery.as_deref(), Some("run repogrammar sync"));
    }

    #[test]
    fn invalid_source_freshness_request_is_sanitized_error() {
        let fact = semantic_fact();
        let error = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact]),
            &StaticSourceStore {
                result: Err(SourceStoreError::InvalidRequest(
                    "/repo/src/a.ts invalid with secret detail".to_string(),
                )),
            },
        )
        .expect_err("invalid freshness request must be an application error");

        assert_eq!(
            error,
            RepoGrammarError::InvalidInput("source freshness request is invalid".to_string())
        );
        let debug = format!("{error:?}");
        assert!(!debug.contains("/repo"));
        assert!(!debug.contains("secret detail"));
    }

    #[test]
    fn source_freshness_response_path_must_match_fact_path() {
        let fact = semantic_fact();
        let content_hash = fact.content_hash.clone();
        let error = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact]),
            &StaticSourceStore {
                result: Ok(SourceText {
                    path: "src/other.ts".to_string(),
                    content_hash,
                    text: "source mismatch detail must not leak".to_string(),
                }),
            },
        )
        .expect_err("mismatched source path must be an application error");

        assert_eq!(
            error,
            RepoGrammarError::InvalidInput("source freshness response is invalid".to_string())
        );
        let debug = format!("{error:?}");
        assert!(!debug.contains("src/other.ts"));
        assert!(!debug.contains("source mismatch detail"));
    }

    #[test]
    fn weak_certainty_facts_are_not_claim_inputs_even_when_fresh() {
        for (certainty, reason) in [
            ("STRUCTURAL", UnknownReasonCode::InsufficientSupport),
            (
                "FRAMEWORK_HEURISTIC",
                UnknownReasonCode::InsufficientSupport,
            ),
            ("UNKNOWN", UnknownReasonCode::InsufficientSupport),
            ("CONFLICTING", UnknownReasonCode::ConflictingFacts),
        ] {
            let fact = semantic_fact_with_certainty(certainty);
            let report = assess_semantic_fact_readiness(
                readiness_request(),
                &FakeStore::new(vec![fact.clone()]),
                &source_store_with_hash(fact.content_hash),
            )
            .expect("assess readiness");

            let ClaimInputReadiness::Blocked { unknown } = &report.facts[0].readiness else {
                panic!("{certainty} must not become eligible claim input");
            };
            assert_eq!(unknown.class, UnknownClass::Blocking);
            assert_eq!(unknown.reason, reason);
        }
    }

    #[test]
    fn unknown_fact_kind_is_not_claim_input_even_with_semantic_certainty() {
        let mut fact = semantic_fact();
        fact.kind = "UNKNOWN".to_string();
        let report = assess_semantic_fact_readiness(
            readiness_request(),
            &FakeStore::new(vec![fact.clone()]),
            &source_store_with_hash(fact.content_hash),
        )
        .expect("assess readiness");

        let ClaimInputReadiness::Blocked { unknown } = &report.facts[0].readiness else {
            panic!("UNKNOWN fact kind must not become eligible claim input");
        };
        assert_eq!(unknown.class, UnknownClass::Blocking);
        assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
    }
}
