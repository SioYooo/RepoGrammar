//! Query use-case boundary for finding repository analogues.

use crate::application::family::FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX;
use crate::application::repository::{
    RepositoryImplementationStatus, RepositoryStatus, RepositoryStatusReport,
};
use crate::core::mining::representative_selection::{
    select_representative_evidence, EvidenceCoverage, EvidenceSelectionCandidate,
};
use crate::core::model::{
    EstimatedPotentialTokenSavings, FactCertainty, SemanticFactKind, UnknownClass,
    UnknownReasonCode,
};
use crate::core::policy::freshness::{
    content_hash_freshness, semantic_fact_claim_input_readiness, ClaimInputReadiness,
};
use crate::error::RepoGrammarError;
use crate::ports::family_store::{
    ActiveFamily, FamilyStore, IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord,
    IndexedVariationSlotRecord, StoreError,
};
use crate::ports::index_store::{
    IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedSemanticFactRecord,
};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError, SourceText};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_QUERY_TARGET_BYTES: usize = 8 * 1024;
pub const MAX_QUERY_TOKEN_BUDGET: usize = 200_000;
pub const MAX_RENDERED_SOURCE_SPAN_BYTES: usize = 16 * 1024;

pub fn validate_query_target(value: &str) -> Result<(), &'static str> {
    if value.trim().is_empty() {
        return Err("target must be non-empty when provided");
    }
    if value.len() > MAX_QUERY_TARGET_BYTES {
        return Err("target exceeds the maximum query target length");
    }
    if value.chars().any(char::is_control) {
        return Err("target must not contain control characters");
    }
    Ok(())
}

pub fn validate_query_token_budget(value: usize) -> Result<(), &'static str> {
    if value == 0 {
        return Err("token budget must be a positive integer");
    }
    if value > MAX_QUERY_TOKEN_BUDGET {
        return Err("token budget exceeds the maximum supported value");
    }
    Ok(())
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
                QueryPreflightOperation::PatternFamilyQuery
                    if active_generation == "none"
                        || active_generation == "not implemented"
                        || !inventory_indexing_is_readable(status_report.indexing) =>
                {
                    fallback("no active index generation", "run repogrammar index", false)
                }
                QueryPreflightOperation::PatternFamilyQuery => QueryPreflightReport::Ready,
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
pub struct FamilySummary {
    pub family_id: String,
    pub classification: String,
    pub support: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyListReport {
    pub active_generation: String,
    pub families: Vec<FamilySummary>,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyDetailReport {
    pub active_generation: String,
    pub family_id: String,
    pub classification: String,
    pub support: usize,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyUnknownReport {
    pub active_generation: String,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyLookupReport {
    Found(FamilyDetailReport),
    Unknown(FamilyUnknownReport),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepoShapeDiagnosticsReport {
    pub active_generation: String,
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_member_count: usize,
    pub covered_code_units: usize,
    pub local_pattern_density: Option<f64>,
    pub family_support_coverage: Option<f64>,
    pub abstention_rate: Option<f64>,
    pub external_dependency_signal: DiagnosticSignal,
    pub thin_wrapper_risk: DiagnosticSignal,
    pub token_saving_risk: DiagnosticSignal,
    pub interpretation: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSignal {
    Low,
    Medium,
    High,
    Unknown,
}

impl DiagnosticSignal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyQueryUnknown {
    pub class: UnknownClass,
    pub reason: UnknownReasonCode,
    pub affected_claim: String,
    pub recovery: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FamilyEvidenceMode {
    #[default]
    Compact,
    Evidence,
    Deep,
}

impl FamilyEvidenceMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "compact" => Some(Self::Compact),
            "evidence" => Some(Self::Evidence),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Evidence => "evidence",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FamilyOutputOptions {
    pub evidence_mode: FamilyEvidenceMode,
    pub token_budget: Option<usize>,
    pub include_variations: bool,
    pub include_exceptions: bool,
}

impl Default for FamilyOutputOptions {
    fn default() -> Self {
        Self {
            evidence_mode: FamilyEvidenceMode::Compact,
            token_budget: None,
            include_variations: false,
            include_exceptions: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFamilyEvidence {
    pub mode: FamilyEvidenceMode,
    pub token_budget: Option<usize>,
    pub estimated_tokens: usize,
    pub selection_strategy: &'static str,
    pub budget_satisfied: bool,
    pub covered_claims: Vec<String>,
    pub missing_claims: Vec<String>,
    pub source_snippets_included: bool,
    pub evidence: Vec<SelectedFamilyEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFamilyEvidenceRecord {
    pub record: IndexedFamilyEvidenceRecord,
    pub estimated_tokens: usize,
    pub covered_claims: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadPlanPurpose {
    TargetBodyRequiredForEdit,
    CanonicalEvidence,
    SupportEvidence,
    VariationGuard,
    ExceptionGuard,
    UnknownBlocker,
    StaleEvidenceCheck,
    OptionalContext,
}

impl ReadPlanPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TargetBodyRequiredForEdit => "target_body_required_for_edit",
            Self::CanonicalEvidence => "canonical_evidence",
            Self::SupportEvidence => "support_evidence",
            Self::VariationGuard => "variation_guard",
            Self::ExceptionGuard => "exception_guard",
            Self::UnknownBlocker => "unknown_blocker",
            Self::StaleEvidenceCheck => "stale_evidence_check",
            Self::OptionalContext => "optional_context",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlanItem {
    pub purpose: ReadPlanPurpose,
    pub path: String,
    pub content_hash: crate::core::model::ContentHash,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub estimated_tokens: usize,
    pub why: String,
    pub source_required_before_edit: bool,
    pub source_snippets_included: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    pub items: Vec<ReadPlanItem>,
    pub estimated_tokens: usize,
    pub source_snippets_included: bool,
    pub requires_source_before_edit: bool,
    pub selection_strategy: &'static str,
    pub budget_satisfied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanRenderRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSourceSpan {
    pub purpose: ReadPlanPurpose,
    pub path: String,
    pub content_hash: crate::core::model::ContentHash,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub estimated_tokens: usize,
    pub why: String,
    pub source_required_before_edit: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanOmission {
    pub purpose: ReadPlanPurpose,
    pub path: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub reason: &'static str,
    pub guidance: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanPolicy {
    pub requested: bool,
    pub source_snippets_included: bool,
    pub estimated_tokens: usize,
    pub budget_satisfied: bool,
    pub selection_strategy: &'static str,
    pub fallback_guidance: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanRenderReport {
    pub policy: SourceSpanPolicy,
    pub spans: Vec<RenderedSourceSpan>,
    pub omissions: Vec<SourceSpanOmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyLookupMode {
    ExactFamilyId,
    ExactMemberId,
    FuzzyQuery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyEvidenceFreshnessRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
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

pub fn repo_shape_diagnostics(
    index_store: &impl IndexStore,
    family_store: &impl FamilyStore,
) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
    let snapshot = index_store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    let active_families = family_store
        .list_active_families()
        .map_err(family_store_error)?;
    let eligible_ids = snapshot
        .units
        .iter()
        .filter(|unit| python_family_eligible_unit(unit))
        .map(|unit| unit.id.clone())
        .collect::<BTreeSet<_>>();
    let eligible_code_units = eligible_ids.len();
    let mut family_member_count = 0usize;
    let mut covered_code_units = BTreeSet::new();
    let mut family_count = 0usize;
    for family in &active_families.families {
        let Some(active_family) = family_store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        family_count += 1;
        for member in active_family.members {
            if eligible_ids.contains(&member.code_unit_id) {
                family_member_count = family_member_count.saturating_add(1);
                covered_code_units.insert(member.code_unit_id);
            }
        }
    }
    let local_pattern_density = ratio(family_member_count, eligible_code_units);
    let family_support_coverage = ratio(covered_code_units.len(), eligible_code_units);
    let abstention_rate = family_support_coverage.map(|coverage| (1.0 - coverage).max(0.0));
    let token_saving_risk = risk_from_density(local_pattern_density);
    let thin_wrapper_risk = risk_from_density(family_support_coverage);
    Ok(RepoShapeDiagnosticsReport {
        active_generation: snapshot.generation_id,
        eligible_code_units,
        family_count,
        family_member_count,
        covered_code_units: covered_code_units.len(),
        local_pattern_density,
        family_support_coverage,
        abstention_rate,
        external_dependency_signal: DiagnosticSignal::Unknown,
        thin_wrapper_risk,
        token_saving_risk,
        interpretation:
            "RepoGrammar can provide integration-pattern context when repeated local patterns exist; third-party-heavy or thin-wrapper repositories may see lower token-saving potential.",
    })
}

pub fn list_families(store: &impl FamilyStore) -> Result<FamilyListReport, RepoGrammarError> {
    let active = store.list_active_families().map_err(family_store_error)?;
    let mut families = Vec::with_capacity(active.families.len());
    for family in &active.families {
        let support = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
            .map(|family| family.members.len())
            .unwrap_or(0);
        families.push(FamilySummary {
            family_id: family.family_id.clone(),
            classification: family.classification.clone(),
            support,
        });
    }
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    let unknowns = if families.is_empty() {
        vec![insufficient_support_unknown("repository pattern families")]
    } else {
        Vec::new()
    };
    Ok(FamilyListReport {
        active_generation: active.generation_id,
        families,
        unknowns,
    })
}

pub fn list_families_with_freshness(
    request: FamilyEvidenceFreshnessRequest,
    store: &impl FamilyStore,
    source_store: &impl SourceStore,
) -> Result<FamilyListReport, RepoGrammarError> {
    let active = store.list_active_families().map_err(family_store_error)?;
    let mut families = Vec::with_capacity(active.families.len());
    let mut unknowns = Vec::new();
    for family in &active.families {
        let Some(active_family) = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        if family_evidence_is_fresh(&request, source_store, &active_family)? {
            families.push(FamilySummary {
                family_id: family.family_id.clone(),
                classification: family.classification.clone(),
                support: active_family.members.len(),
            });
        } else {
            unknowns.push(stale_evidence_unknown(format!(
                "{}:evidence_freshness",
                family.family_id
            )));
        }
    }
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    if families.is_empty() && unknowns.is_empty() {
        unknowns.push(insufficient_support_unknown("repository pattern families"));
    }
    Ok(FamilyListReport {
        active_generation: active.generation_id,
        families,
        unknowns,
    })
}

pub fn lookup_family(
    store: &impl FamilyStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let active = store.list_active_families().map_err(family_store_error)?;
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            unknowns: vec![insufficient_support_unknown("query target")],
        }));
    };
    for family in active.families {
        let Some(active_family) = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        if family_matches_target(&active_family, target, mode) {
            return Ok(FamilyLookupReport::Found(family_detail(active_family)));
        }
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation: active.generation_id,
        unknowns: vec![insufficient_support_unknown("query target")],
    }))
}

pub fn lookup_family_with_freshness(
    request: FamilyEvidenceFreshnessRequest,
    store: &impl FamilyStore,
    source_store: &impl SourceStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let active = store.list_active_families().map_err(family_store_error)?;
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            unknowns: vec![insufficient_support_unknown("query target")],
        }));
    };
    for family in active.families {
        let Some(active_family) = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        if !family_matches_target(&active_family, target, mode) {
            continue;
        }
        if family_evidence_is_fresh(&request, source_store, &active_family)? {
            return Ok(FamilyLookupReport::Found(family_detail(active_family)));
        }
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            unknowns: vec![stale_evidence_unknown(format!(
                "{}:evidence_freshness",
                active_family.family.family_id
            ))],
        }));
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation: active.generation_id,
        unknowns: vec![insufficient_support_unknown("query target")],
    }))
}

pub fn select_family_evidence(
    family: &FamilyDetailReport,
    options: FamilyOutputOptions,
) -> SelectedFamilyEvidence {
    let selected = match options.evidence_mode {
        FamilyEvidenceMode::Compact => BudgetedEvidenceSelection::empty(),
        FamilyEvidenceMode::Evidence | FamilyEvidenceMode::Deep => {
            select_budgeted_evidence(family, options)
        }
    };

    SelectedFamilyEvidence {
        mode: options.evidence_mode,
        token_budget: options.token_budget,
        estimated_tokens: selected.estimated_tokens,
        selection_strategy: "greedy_marginal_coverage_v1",
        budget_satisfied: selected.budget_satisfied,
        covered_claims: selected.covered_claims,
        missing_claims: selected.missing_claims,
        source_snippets_included: false,
        evidence: selected.evidence,
    }
}

pub fn build_read_plan(
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
) -> ReadPlan {
    let candidates = read_plan_candidates(family, target, mode, options);
    let selected = select_budgeted_read_plan(candidates, options.token_budget);
    let estimated_tokens = selected.iter().map(|item| item.estimated_tokens).sum();
    ReadPlan {
        requires_source_before_edit: selected.iter().any(|item| item.source_required_before_edit),
        source_snippets_included: false,
        budget_satisfied: match options.token_budget {
            Some(budget) => estimated_tokens <= budget,
            None => true,
        },
        estimated_tokens,
        selection_strategy: "deterministic_read_plan_v1",
        items: selected,
    }
}

pub fn render_source_spans(
    request: SourceSpanRenderRequest,
    source_store: &impl SourceStore,
    read_plan: &ReadPlan,
    include_source_spans: bool,
    token_budget: Option<usize>,
) -> Result<SourceSpanRenderReport, RepoGrammarError> {
    if !include_source_spans {
        return Ok(SourceSpanRenderReport {
            policy: SourceSpanPolicy {
                requested: false,
                source_snippets_included: false,
                estimated_tokens: 0,
                budget_satisfied: true,
                selection_strategy: "metadata_only_v1",
                fallback_guidance:
                    "source spans were not requested; use the read_plan before editing",
            },
            spans: Vec::new(),
            omissions: Vec::new(),
        });
    }

    let mut spans = Vec::new();
    let mut omissions = Vec::new();
    let mut estimated_tokens = 0usize;
    let mut budget_satisfied = true;

    for item in &read_plan.items {
        let source = match source_store.read_source(SourceReadRequest {
            repository_root: request.repository_root.clone(),
            path: item.path.clone(),
            expected_content_hash: item.content_hash.clone(),
            max_file_bytes: request.max_file_bytes,
        }) {
            Ok(source) => source,
            Err(SourceStoreError::InvalidRequest(_)) => {
                return Err(RepoGrammarError::InvalidInput(
                    "stored read-plan source path is invalid".to_string(),
                ));
            }
            Err(SourceStoreError::Missing(_)) | Err(SourceStoreError::HashMismatch(_)) => {
                omissions.push(source_span_omission(
                    item,
                    "stale_evidence",
                    "source changed or disappeared; use normal Read/Grep for this span",
                ));
                continue;
            }
            Err(SourceStoreError::TooLarge(_)) => {
                omissions.push(source_span_omission(
                    item,
                    "source_too_large",
                    "source exceeds the configured read limit; use normal Read/Grep if needed",
                ));
                continue;
            }
            Err(SourceStoreError::NonUtf8(_)) => {
                omissions.push(source_span_omission(
                    item,
                    "non_utf8_source",
                    "source is not UTF-8; use normal tooling for this file",
                ));
                continue;
            }
            Err(SourceStoreError::Unavailable(_)) => {
                omissions.push(source_span_omission(
                    item,
                    "source_unavailable",
                    "source store is unavailable; use normal Read/Grep for this span",
                ));
                continue;
            }
        };

        if source.path != item.path || source.content_hash != item.content_hash {
            return Err(RepoGrammarError::InvalidInput(
                "source store returned mismatched source for read-plan span".to_string(),
            ));
        }

        let rendered = match render_source_span_item(item, &source) {
            Ok(rendered) => rendered,
            Err(RenderSourceSpanItemError::InvalidRange) => {
                omissions.push(source_span_omission(
                    item,
                    "invalid_source_range",
                    "stored source range is invalid; use normal Read/Grep for this span",
                ));
                continue;
            }
            Err(RenderSourceSpanItemError::SpanTooLarge) => {
                omissions.push(source_span_omission(
                    item,
                    "source_span_too_large",
                    "source span exceeds the configured render limit; use normal Read if needed",
                ));
                continue;
            }
        };
        let next_estimated_tokens = estimated_tokens.saturating_add(rendered.estimated_tokens);
        if let Some(budget) = token_budget {
            if next_estimated_tokens > budget {
                budget_satisfied = false;
                omissions.push(source_span_omission(
                    item,
                    "token_budget_exceeded",
                    "source span omitted to stay within the requested token budget; use normal Read if this span is necessary",
                ));
                continue;
            }
        }
        estimated_tokens = next_estimated_tokens;
        spans.push(rendered);
    }

    Ok(SourceSpanRenderReport {
        policy: SourceSpanPolicy {
            requested: true,
            source_snippets_included: !spans.is_empty(),
            estimated_tokens,
            budget_satisfied,
            selection_strategy: "hash_checked_line_numbered_spans_v1",
            fallback_guidance: if omissions.is_empty() {
                "use rendered source spans only for the listed byte ranges; use normal Read before editing outside them"
            } else {
                "some spans were omitted; use normal Read/Grep for omitted or stale cases"
            },
        },
        spans,
        omissions,
    })
}

pub fn read_plan_with_rendered_spans(
    read_plan: &ReadPlan,
    rendered: &SourceSpanRenderReport,
) -> ReadPlan {
    let mut output = read_plan.clone();
    output.source_snippets_included = rendered.policy.source_snippets_included;
    output.budget_satisfied = output.budget_satisfied && rendered.policy.budget_satisfied;
    for item in &mut output.items {
        if let Some(span) = rendered.spans.iter().find(|span| {
            span.path == item.path
                && span.content_hash == item.content_hash
                && span.start_byte == item.start_byte
                && span.end_byte == item.end_byte
                && span.purpose == item.purpose
        }) {
            item.start_line = Some(span.start_line);
            item.end_line = Some(span.end_line);
            item.source_snippets_included = true;
        }
    }
    output
}

pub fn estimate_family_output_potential_token_savings(
    family: &FamilyDetailReport,
    selected_evidence: &SelectedFamilyEvidence,
    read_plan: &ReadPlan,
    source_spans: Option<&SourceSpanRenderReport>,
) -> EstimatedPotentialTokenSavings {
    let all_family_evidence_tokens = family
        .evidence
        .iter()
        .map(estimated_evidence_tokens)
        .sum::<usize>();
    let source_span_tokens = source_spans
        .map(|spans| spans.policy.estimated_tokens)
        .unwrap_or(0);
    let estimated_baseline_tokens =
        all_family_evidence_tokens.saturating_add(read_plan.estimated_tokens);
    let estimated_returned_tokens = selected_evidence
        .estimated_tokens
        .saturating_add(read_plan.estimated_tokens)
        .saturating_add(source_span_tokens);
    EstimatedPotentialTokenSavings::new(
        usize_to_u64_saturating(estimated_baseline_tokens),
        usize_to_u64_saturating(estimated_returned_tokens),
    )
}

fn usize_to_u64_saturating(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn render_source_span_item(
    item: &ReadPlanItem,
    source: &SourceText,
) -> Result<RenderedSourceSpan, RenderSourceSpanItemError> {
    let (text, start_line, end_line) =
        line_numbered_span(&source.text, item.start_byte, item.end_byte)?;
    if text.len() > MAX_RENDERED_SOURCE_SPAN_BYTES {
        return Err(RenderSourceSpanItemError::SpanTooLarge);
    }
    let estimated_tokens = estimate_text_tokens(&text);
    Ok(RenderedSourceSpan {
        purpose: item.purpose,
        path: item.path.clone(),
        content_hash: item.content_hash.clone(),
        start_byte: item.start_byte,
        end_byte: item.end_byte,
        start_line,
        end_line,
        estimated_tokens,
        why: item.why.clone(),
        source_required_before_edit: item.source_required_before_edit,
        text,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderSourceSpanItemError {
    InvalidRange,
    SpanTooLarge,
}

fn source_span_omission(
    item: &ReadPlanItem,
    reason: &'static str,
    guidance: &'static str,
) -> SourceSpanOmission {
    SourceSpanOmission {
        purpose: item.purpose,
        path: item.path.clone(),
        start_byte: item.start_byte,
        end_byte: item.end_byte,
        reason,
        guidance,
    }
}

fn line_numbered_span(
    source: &str,
    start_byte: usize,
    end_byte: usize,
) -> Result<(String, usize, usize), RenderSourceSpanItemError> {
    if start_byte >= end_byte || end_byte > source.len() {
        return Err(RenderSourceSpanItemError::InvalidRange);
    }
    if !source.is_char_boundary(start_byte) || !source.is_char_boundary(end_byte) {
        return Err(RenderSourceSpanItemError::InvalidRange);
    }

    let line_starts = source_line_starts(source);
    let mut selected = Vec::new();
    for (index, line_start) in line_starts.iter().copied().enumerate() {
        let line_end = line_starts.get(index + 1).copied().unwrap_or(source.len());
        if line_end > start_byte && line_start < end_byte {
            selected.push((index + 1, line_start, line_end));
        }
    }
    let Some((start_line, _, _)) = selected.first().copied() else {
        return Err(RenderSourceSpanItemError::InvalidRange);
    };
    let end_line = selected
        .last()
        .map(|(line_number, _, _)| *line_number)
        .unwrap_or(start_line);

    let mut rendered = String::new();
    for (line_number, line_start, line_end) in selected {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        let mut line = &source[line_start..line_end];
        line = line.strip_suffix('\n').unwrap_or(line);
        line = line.strip_suffix('\r').unwrap_or(line);
        rendered.push_str(&format!("{line_number}\t{line}"));
    }
    Ok((rendered, start_line, end_line))
}

fn source_line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' && index + 1 < source.len() {
            starts.push(index + 1);
        }
    }
    starts
}

fn estimate_text_tokens(text: &str) -> usize {
    text.len().max(1).div_ceil(4)
}

fn read_plan_candidates(
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
) -> Vec<ReadPlanItem> {
    let mut candidates = Vec::new();
    let target = target.map(str::trim).filter(|target| !target.is_empty());
    if let Some(target_evidence) = target.and_then(|target| target_evidence(family, target, mode)) {
        candidates.push(read_plan_item(
            target_evidence,
            ReadPlanPurpose::TargetBodyRequiredForEdit,
            "read this target body before editing; family metadata is context only",
            true,
        ));
    }

    if let Some(canonical) = first_evidence_for_claim(family, "canonical") {
        candidates.push(read_plan_item(
            canonical,
            ReadPlanPurpose::CanonicalEvidence,
            "canonical source span supporting the family claim",
            false,
        ));
    }
    if let Some(support) = first_distinct_evidence_for_claim(family, "support", &candidates) {
        candidates.push(read_plan_item(
            support,
            ReadPlanPurpose::SupportEvidence,
            "additional supporting source span for contrast",
            false,
        ));
    }
    if options.include_variations || !family.variation_slots.is_empty() {
        if let Some(variation) = first_distinct_evidence_for_claim(family, "variation", &candidates)
        {
            candidates.push(read_plan_item(
                variation,
                ReadPlanPurpose::VariationGuard,
                "variation guard source span; verify before applying the family blindly",
                false,
            ));
        }
    }
    if options.include_exceptions {
        if let Some(exception) = first_distinct_evidence_for_claim(family, "exception", &candidates)
        {
            candidates.push(read_plan_item(
                exception,
                ReadPlanPurpose::ExceptionGuard,
                "exception guard source span; verify before assuming conformance",
                false,
            ));
        }
    }
    if !family.unknowns.is_empty() {
        if let Some(unknown_guard) = first_distinct_evidence(family, &candidates) {
            candidates.push(read_plan_item(
                unknown_guard,
                ReadPlanPurpose::UnknownBlocker,
                "unknown guard source span; verify before upgrading this family context into an edit or conformance claim",
                false,
            ));
        }
    }
    if candidates.is_empty() {
        if let Some(first) = family.evidence.first() {
            candidates.push(read_plan_item(
                first,
                ReadPlanPurpose::OptionalContext,
                "optional family context span",
                false,
            ));
        }
    }

    candidates
        .into_iter()
        .fold(Vec::new(), |mut unique, candidate| {
            if !unique
                .iter()
                .any(|item| same_read_plan_span(item, &candidate))
            {
                unique.push(candidate);
            }
            unique
        })
}

fn select_budgeted_read_plan(
    candidates: Vec<ReadPlanItem>,
    token_budget: Option<usize>,
) -> Vec<ReadPlanItem> {
    let Some(budget) = token_budget else {
        return candidates;
    };
    let mut selected = Vec::new();
    let mut estimated_tokens = 0usize;
    for candidate in candidates {
        if selected.is_empty()
            || estimated_tokens.saturating_add(candidate.estimated_tokens) <= budget
        {
            estimated_tokens = estimated_tokens.saturating_add(candidate.estimated_tokens);
            selected.push(candidate);
        }
    }
    selected
}

fn target_evidence<'a>(
    family: &'a FamilyDetailReport,
    target: &str,
    mode: FamilyLookupMode,
) -> Option<&'a IndexedFamilyEvidenceRecord> {
    match mode {
        FamilyLookupMode::ExactFamilyId => None,
        FamilyLookupMode::ExactMemberId => family
            .evidence
            .iter()
            .find(|evidence| evidence.code_unit_id == target),
        FamilyLookupMode::FuzzyQuery => family.evidence.iter().find(|evidence| {
            evidence.code_unit_id == target || path_matches_target(&evidence.path, target)
        }),
    }
}

fn first_evidence_for_claim<'a>(
    family: &'a FamilyDetailReport,
    claim: &str,
) -> Option<&'a IndexedFamilyEvidenceRecord> {
    family.evidence.iter().find(|evidence| {
        evidence
            .covered_claims
            .iter()
            .any(|covered| covered == claim)
    })
}

fn first_distinct_evidence_for_claim<'a>(
    family: &'a FamilyDetailReport,
    claim: &str,
    existing: &[ReadPlanItem],
) -> Option<&'a IndexedFamilyEvidenceRecord> {
    family.evidence.iter().find(|evidence| {
        evidence
            .covered_claims
            .iter()
            .any(|covered| covered == claim)
            && !existing.iter().any(|item| {
                item.path == evidence.path
                    && item.start_byte == evidence.start_byte
                    && item.end_byte == evidence.end_byte
            })
            && !existing.iter().any(|item| item.path == evidence.path)
    })
}

fn first_distinct_evidence<'a>(
    family: &'a FamilyDetailReport,
    existing: &[ReadPlanItem],
) -> Option<&'a IndexedFamilyEvidenceRecord> {
    family.evidence.iter().find(|evidence| {
        !existing.iter().any(|item| {
            item.path == evidence.path
                && item.start_byte == evidence.start_byte
                && item.end_byte == evidence.end_byte
        }) && !existing.iter().any(|item| item.path == evidence.path)
    })
}

fn read_plan_item(
    evidence: &IndexedFamilyEvidenceRecord,
    purpose: ReadPlanPurpose,
    why: &str,
    source_required_before_edit: bool,
) -> ReadPlanItem {
    ReadPlanItem {
        purpose,
        path: evidence.path.clone(),
        content_hash: evidence.content_hash.clone(),
        start_byte: evidence.start_byte,
        end_byte: evidence.end_byte,
        start_line: None,
        end_line: None,
        estimated_tokens: estimated_read_plan_tokens(evidence, purpose, why),
        why: why.to_string(),
        source_required_before_edit,
        source_snippets_included: false,
    }
}

fn estimated_read_plan_tokens(
    evidence: &IndexedFamilyEvidenceRecord,
    purpose: ReadPlanPurpose,
    why: &str,
) -> usize {
    let bytes = purpose.as_str().len()
        + evidence.path.len()
        + evidence.content_hash.as_str().len()
        + evidence.code_unit_id.len()
        + why.len()
        + 48;
    bytes.div_ceil(4).max(1)
}

fn same_read_plan_span(left: &ReadPlanItem, right: &ReadPlanItem) -> bool {
    left.path == right.path
        && left.start_byte == right.start_byte
        && left.end_byte == right.end_byte
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BudgetedEvidenceSelection {
    estimated_tokens: usize,
    budget_satisfied: bool,
    covered_claims: Vec<String>,
    missing_claims: Vec<String>,
    evidence: Vec<SelectedFamilyEvidenceRecord>,
}

impl BudgetedEvidenceSelection {
    fn empty() -> Self {
        Self {
            estimated_tokens: 0,
            budget_satisfied: true,
            covered_claims: Vec::new(),
            missing_claims: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

fn select_budgeted_evidence(
    family: &FamilyDetailReport,
    options: FamilyOutputOptions,
) -> BudgetedEvidenceSelection {
    if family.evidence.is_empty() {
        return BudgetedEvidenceSelection::empty();
    }
    let candidates = family
        .evidence
        .iter()
        .enumerate()
        .map(|(index, evidence)| EvidenceSelectionCandidate {
            stable_id: evidence.evidence_id.clone(),
            estimated_tokens: estimated_evidence_tokens(evidence),
            coverage: evidence_coverage(family, index, evidence),
            source_order: index,
        })
        .collect::<Vec<_>>();
    let required_coverage = required_evidence_coverage(family, options);
    let selected =
        select_representative_evidence(&candidates, &required_coverage, options.token_budget);
    let coverage_by_id = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.stable_id.as_str(),
                coverage_strings(&candidate.coverage),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let evidence_by_id = family
        .evidence
        .iter()
        .map(|evidence| (evidence.evidence_id.as_str(), evidence))
        .collect::<BTreeMap<_, _>>();

    let evidence = selected
        .selected_ids
        .iter()
        .filter_map(|evidence_id| {
            evidence_by_id
                .get(evidence_id.as_str())
                .map(|evidence| SelectedFamilyEvidenceRecord {
                    record: (*evidence).clone(),
                    estimated_tokens: estimated_evidence_tokens(evidence),
                    covered_claims: coverage_by_id
                        .get(evidence_id.as_str())
                        .cloned()
                        .unwrap_or_default(),
                })
        })
        .collect::<Vec<_>>();

    BudgetedEvidenceSelection {
        estimated_tokens: selected.estimated_tokens,
        budget_satisfied: selected.budget_satisfied,
        covered_claims: coverage_strings(&selected.covered),
        missing_claims: coverage_strings(&selected.missing),
        evidence,
    }
}

fn estimated_evidence_tokens(evidence: &IndexedFamilyEvidenceRecord) -> usize {
    let bytes = evidence.evidence_id.len()
        + evidence.family_id.len()
        + evidence.code_unit_id.len()
        + evidence.path.len()
        + evidence.content_hash.as_str().len()
        + evidence.note.len()
        + 32;
    bytes.div_ceil(4).max(1)
}

fn required_evidence_coverage(
    family: &FamilyDetailReport,
    options: FamilyOutputOptions,
) -> BTreeSet<EvidenceCoverage> {
    let mut required = BTreeSet::new();
    if !family.evidence.is_empty() {
        required.insert(EvidenceCoverage::Canonical);
        required.insert(EvidenceCoverage::Support);
    }
    if !family.variation_slots.is_empty() && options.include_variations {
        required.insert(EvidenceCoverage::Variation);
    }
    if options.include_exceptions {
        required.insert(EvidenceCoverage::Exception);
    }
    required
}

fn evidence_coverage(
    _family: &FamilyDetailReport,
    _index: usize,
    evidence: &IndexedFamilyEvidenceRecord,
) -> BTreeSet<EvidenceCoverage> {
    evidence
        .covered_claims
        .iter()
        .filter_map(|claim| match claim.as_str() {
            "canonical" => Some(EvidenceCoverage::Canonical),
            "support" => Some(EvidenceCoverage::Support),
            "variation" => Some(EvidenceCoverage::Variation),
            "exception" => Some(EvidenceCoverage::Exception),
            _ => None,
        })
        .collect()
}

fn coverage_strings(coverage: &BTreeSet<EvidenceCoverage>) -> Vec<String> {
    coverage
        .iter()
        .map(|coverage| coverage.as_str().to_string())
        .collect()
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

fn family_store_error(error: StoreError) -> RepoGrammarError {
    match error {
        StoreError::Unavailable(message)
        | StoreError::InvalidState(message)
        | StoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}

fn family_detail(family: ActiveFamily) -> FamilyDetailReport {
    let family_id = family.family.family_id;
    let mut unknowns = vec![FamilyQueryUnknown {
        class: UnknownClass::NonBlocking,
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: format!("{family_id}:runtime_equivalence"),
        recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
    }];
    unknowns.extend(
        family
            .variation_slots
            .iter()
            .filter_map(family_unknown_from_variation_slot),
    );
    unknowns.sort_by(|left, right| {
        (
            left.affected_claim.as_str(),
            left.class.as_protocol_str(),
            left.reason.as_protocol_str(),
        )
            .cmp(&(
                right.affected_claim.as_str(),
                right.class.as_protocol_str(),
                right.reason.as_protocol_str(),
            ))
    });
    unknowns.dedup();
    FamilyDetailReport {
        active_generation: family.generation_id,
        family_id: family_id.clone(),
        classification: family.family.classification,
        support: family.members.len(),
        members: family.members,
        variation_slots: family.variation_slots,
        evidence: family.evidence,
        unknowns,
    }
}

fn family_unknown_from_variation_slot(
    slot: &IndexedVariationSlotRecord,
) -> Option<FamilyQueryUnknown> {
    let payload = slot
        .description
        .strip_prefix(FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX)?;
    let mut parts = payload.splitn(4, '|');
    let class = UnknownClass::parse_protocol_str(parts.next()?).ok()?;
    let reason = UnknownReasonCode::parse_protocol_str(parts.next()?).ok()?;
    let affected_claim = parts.next()?.to_string();
    let recovery = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    Some(FamilyQueryUnknown {
        class,
        reason,
        affected_claim,
        recovery,
    })
}

fn family_matches_target(family: &ActiveFamily, target: &str, mode: FamilyLookupMode) -> bool {
    match mode {
        FamilyLookupMode::ExactFamilyId => family.family.family_id == target,
        FamilyLookupMode::ExactMemberId => family
            .members
            .iter()
            .any(|member| member.code_unit_id == target),
        FamilyLookupMode::FuzzyQuery => {
            family.family.family_id == target
                || family
                    .members
                    .iter()
                    .any(|member| member.code_unit_id == target || member.role == target)
                || family
                    .evidence
                    .iter()
                    .any(|evidence| path_matches_target(&evidence.path, target))
        }
    }
}

fn path_matches_target(path: &str, target: &str) -> bool {
    path == target || (target.contains('/') && path.ends_with(&format!("/{target}")))
}

fn python_family_eligible_unit(unit: &IndexedCodeUnitRecord) -> bool {
    unit.language == "python"
        && matches!(
            unit.kind.as_str(),
            "fastapi_route"
                | "pytest_test"
                | "pytest_fixture"
                | "pydantic_model"
                | "pydantic_settings"
                | "sqlalchemy_model"
                | "sqlalchemy_repository_method"
        )
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some(numerator as f64 / denominator as f64)
    }
}

fn risk_from_density(density: Option<f64>) -> DiagnosticSignal {
    let Some(density) = density else {
        return DiagnosticSignal::Unknown;
    };
    if density >= 0.35 {
        DiagnosticSignal::Low
    } else if density >= 0.10 {
        DiagnosticSignal::Medium
    } else {
        DiagnosticSignal::High
    }
}

fn family_evidence_is_fresh(
    request: &FamilyEvidenceFreshnessRequest,
    source_store: &impl SourceStore,
    family: &ActiveFamily,
) -> Result<bool, RepoGrammarError> {
    for evidence in &family.evidence {
        let source = source_store.read_source(SourceReadRequest {
            repository_root: request.repository_root.clone(),
            path: evidence.path.clone(),
            expected_content_hash: evidence.content_hash.clone(),
            max_file_bytes: request.max_file_bytes,
        });
        match source {
            Ok(source)
                if source.path == evidence.path && source.content_hash == evidence.content_hash => {
            }
            Ok(_) => {
                return Err(RepoGrammarError::InvalidInput(
                    "source freshness response is invalid".to_string(),
                ));
            }
            Err(SourceStoreError::InvalidRequest(_)) => {
                return Err(RepoGrammarError::InvalidInput(
                    "stored family evidence path is invalid".to_string(),
                ));
            }
            Err(_) => return Ok(false),
        }
    }
    Ok(true)
}

fn insufficient_support_unknown(affected_claim: impl Into<String>) -> FamilyQueryUnknown {
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim: affected_claim.into(),
        recovery: Some("run repogrammar index after adding compatible implementations".to_string()),
    }
}

fn stale_evidence_unknown(affected_claim: impl Into<String>) -> FamilyQueryUnknown {
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::StaleEvidence,
        affected_claim: affected_claim.into(),
        recovery: Some("run repogrammar sync".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::RepositoryManifestStatus;
    use crate::core::model::{
        ContentHash, EstimatedPotentialTokenSavings, UnknownClass, UnknownReasonCode,
    };
    use crate::ports::family_store::{
        ActiveFamilies, IndexedFamilyRecord, IndexedVariationSlotRecord,
    };
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

    struct FakeFamilyStore {
        active: ActiveFamilies,
        family: Option<ActiveFamily>,
    }

    impl FakeFamilyStore {
        fn empty() -> Self {
            Self {
                active: ActiveFamilies {
                    generation_id: "gen-000001".to_string(),
                    families: Vec::new(),
                },
                family: None,
            }
        }

        fn with_family() -> Self {
            let family = ActiveFamily {
                generation_id: "gen-000001".to_string(),
                family: IndexedFamilyRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                },
                members: vec![IndexedFamilyMemberRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    code_unit_id: "unit:src/routes/a.ts#express_route:0-20".to_string(),
                    role: "framework:express.route_handler".to_string(),
                }],
                variation_slots: vec![IndexedVariationSlotRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    slot_id: "slot:runtime_unknown".to_string(),
                    description:
                        "non_blocking_unknown:FrameworkMagic:runtime equivalence remains unproven"
                            .to_string(),
                }],
                evidence: vec![IndexedFamilyEvidenceRecord {
                    evidence_id: "family-evidence:000000".to_string(),
                    family_id: "family:typescript:express_route:express".to_string(),
                    code_unit_id: "unit:src/routes/a.ts#express_route:0-20".to_string(),
                    covered_claims: vec!["canonical".to_string(), "support".to_string()],
                    path: "src/routes/a.ts".to_string(),
                    content_hash: semantic_fact().content_hash,
                    start_byte: 0,
                    end_byte: 20,
                    note: "DOMINANT_PATTERN support evidence".to_string(),
                }],
            };
            Self {
                active: ActiveFamilies {
                    generation_id: family.generation_id.clone(),
                    families: vec![family.family.clone()],
                },
                family: Some(family),
            }
        }
    }

    impl FamilyStore for FakeFamilyStore {
        fn record_family(
            &self,
            _generation: &GenerationHandle,
            _family: &IndexedFamilyRecord,
        ) -> Result<(), StoreError> {
            panic!("query read tests must not write families")
        }

        fn record_family_member(
            &self,
            _generation: &GenerationHandle,
            _member: &IndexedFamilyMemberRecord,
        ) -> Result<(), StoreError> {
            panic!("query read tests must not write family members")
        }

        fn record_variation_slot(
            &self,
            _generation: &GenerationHandle,
            _slot: &IndexedVariationSlotRecord,
        ) -> Result<(), StoreError> {
            panic!("query read tests must not write variation slots")
        }

        fn record_family_evidence(
            &self,
            _generation: &GenerationHandle,
            _evidence: &IndexedFamilyEvidenceRecord,
        ) -> Result<(), StoreError> {
            panic!("query read tests must not write family evidence")
        }

        fn list_active_families(&self) -> Result<ActiveFamilies, StoreError> {
            Ok(self.active.clone())
        }

        fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError> {
            Ok(self
                .family
                .as_ref()
                .filter(|family| family.family.family_id == family_id)
                .cloned())
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

    fn indexed_python_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}"),
            path: path.to_string(),
            language: "python".to_string(),
            kind: kind.to_string(),
            start_byte: index * 10,
            end_byte: index * 10 + 8,
            content_hash: semantic_fact().content_hash,
        }
    }

    fn family_evidence(path: &str, index: usize, note: &str) -> IndexedFamilyEvidenceRecord {
        IndexedFamilyEvidenceRecord {
            evidence_id: format!("family-evidence:{index:06}"),
            family_id: "family:typescript:express_route:express".to_string(),
            code_unit_id: format!("unit:{path}#express_route:0-20"),
            covered_claims: if index == 0 {
                vec!["canonical".to_string(), "support".to_string()]
            } else {
                vec!["support".to_string()]
            },
            path: path.to_string(),
            content_hash: semantic_fact().content_hash,
            start_byte: index,
            end_byte: index + 20,
            note: note.to_string(),
        }
    }

    fn family_detail_with_evidence(
        evidence: Vec<IndexedFamilyEvidenceRecord>,
    ) -> FamilyDetailReport {
        FamilyDetailReport {
            active_generation: "gen-000001".to_string(),
            family_id: "family:typescript:express_route:express".to_string(),
            classification: "DOMINANT_PATTERN".to_string(),
            support: evidence.len(),
            members: Vec::new(),
            variation_slots: Vec::new(),
            evidence,
            unknowns: Vec::new(),
        }
    }

    #[test]
    fn family_detail_restores_non_blocking_unknowns_from_variation_slot_metadata() {
        let family_id = "family:python:fastapi_route:framework_fastapi_route".to_string();
        let detail = family_detail(ActiveFamily {
            generation_id: "gen-000001".to_string(),
            family: IndexedFamilyRecord {
                family_id: family_id.clone(),
                classification: "DOMINANT_PATTERN".to_string(),
            },
            members: Vec::new(),
            variation_slots: vec![
                IndexedVariationSlotRecord {
                    family_id: family_id.clone(),
                    slot_id: "slot:unknown:runtime_dependency_injection:000000".to_string(),
                    description: format!(
                        "unknown|non_blocking_unknown|RuntimeDependencyInjection|{family_id}:fastapi_dependency_target|resolve this Python subclaim before relying on it"
                    ),
                },
                IndexedVariationSlotRecord {
                    family_id: family_id.clone(),
                    slot_id: "slot:ordinary".to_string(),
                    description: "variation:ordinary:metadata differs".to_string(),
                },
            ],
            evidence: Vec::new(),
        });

        assert!(detail.unknowns.iter().any(|unknown| {
            unknown.class == UnknownClass::NonBlocking
                && unknown.reason == UnknownReasonCode::RuntimeDependencyInjection
                && unknown.affected_claim == format!("{family_id}:fastapi_dependency_target")
                && unknown.recovery.as_deref()
                    == Some("resolve this Python subclaim before relying on it")
        }));
        assert!(detail.unknowns.iter().any(|unknown| {
            unknown.class == UnknownClass::NonBlocking
                && unknown.reason == UnknownReasonCode::FrameworkMagic
                && unknown.affected_claim == format!("{family_id}:runtime_equivalence")
        }));
        assert_eq!(detail.variation_slots.len(), 2);
    }

    fn family_store_with_members(members: Vec<IndexedFamilyMemberRecord>) -> FakeFamilyStore {
        let family = ActiveFamily {
            generation_id: "gen-000001".to_string(),
            family: IndexedFamilyRecord {
                family_id: "family:python:fastapi_route:framework_fastapi_route".to_string(),
                classification: "DOMINANT_PATTERN".to_string(),
            },
            members,
            variation_slots: Vec::new(),
            evidence: Vec::new(),
        };
        FakeFamilyStore {
            active: ActiveFamilies {
                generation_id: family.generation_id.clone(),
                families: vec![family.family.clone()],
            },
            family: Some(family),
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

    struct FamilyEvidenceSourceStore {
        path: String,
        result: Result<SourceText, SourceStoreError>,
    }

    impl SourceStore for FamilyEvidenceSourceStore {
        fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
            assert_eq!(request.repository_root, "/repo");
            assert_eq!(request.path, self.path);
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

    fn family_freshness_request() -> FamilyEvidenceFreshnessRequest {
        FamilyEvidenceFreshnessRequest {
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
            manifest_schema_version: Some(1),
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
    fn query_preflight_allows_pattern_queries_for_active_generation() {
        let status = status_report(
            RepositoryStatus::Initialized {
                active_generation: "gen-000001".to_string(),
            },
            RepositoryImplementationStatus::Available,
            RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
            Vec::new(),
        );

        assert_eq!(
            query_preflight(QueryPreflightOperation::PatternFamilyQuery, &status),
            QueryPreflightReport::Ready
        );
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
    fn family_queries_return_typed_unknown_when_no_family_evidence_exists() {
        let store = FakeFamilyStore::empty();

        let families = list_families(&store).expect("list families");
        assert_eq!(families.active_generation, "gen-000001");
        assert!(families.families.is_empty());
        assert_eq!(families.unknowns[0].class, UnknownClass::Blocking);
        assert_eq!(
            families.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );

        let lookup = lookup_family(
            &store,
            Some("/repo/secret.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup family");
        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("missing family evidence must be UNKNOWN");
        };
        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(report.unknowns[0].affected_claim, "query target");
        let debug = format!("{report:?}");
        assert!(!debug.contains("/repo/secret.ts"));
    }

    #[test]
    fn family_lookup_matches_id_and_evidence_path_without_source_leakage() {
        let store = FakeFamilyStore::with_family();

        let families = list_families(&store).expect("list families");
        assert_eq!(families.families.len(), 1);
        assert_eq!(families.families[0].support, 1);

        for (target, mode) in [
            (
                "family:typescript:express_route:express",
                FamilyLookupMode::ExactFamilyId,
            ),
            (
                "unit:src/routes/a.ts#express_route:0-20",
                FamilyLookupMode::ExactMemberId,
            ),
            ("routes/a.ts", FamilyLookupMode::FuzzyQuery),
        ] {
            let lookup = lookup_family(&store, Some(target), mode).expect("lookup family");
            let FamilyLookupReport::Found(report) = lookup else {
                panic!("expected family detail");
            };
            assert_eq!(report.family_id, "family:typescript:express_route:express");
            assert_eq!(report.support, 1);
            assert_eq!(report.evidence[0].path, "src/routes/a.ts");
            assert_eq!(
                report.unknowns[0].affected_claim,
                "family:typescript:express_route:express:runtime_equivalence"
            );
            let debug = format!("{report:?}");
            assert!(!debug.contains("/repo"));
            assert!(!debug.contains("function"));
        }
    }

    #[test]
    fn family_evidence_selection_defaults_to_compact_metadata_free_output() {
        let detail = family_detail_with_evidence(vec![family_evidence(
            "src/routes/a.ts",
            0,
            "canonical support evidence",
        )]);

        let selected = select_family_evidence(&detail, FamilyOutputOptions::default());

        assert_eq!(selected.mode, FamilyEvidenceMode::Compact);
        assert!(selected.evidence.is_empty());
        assert_eq!(selected.estimated_tokens, 0);
        assert!(selected.covered_claims.is_empty());
        assert!(selected.missing_claims.is_empty());
        assert!(selected.budget_satisfied);
        assert!(!selected.source_snippets_included);
    }

    #[test]
    fn family_evidence_selection_uses_greedy_claim_coverage() {
        let first = family_evidence("src/routes/b.ts", 0, "first stored evidence");
        let second = family_evidence("src/routes/a.ts", 1, "VARIATION: alternate handler shape");
        let mut detail = family_detail_with_evidence(vec![first.clone(), second.clone()]);
        detail.variation_slots = vec![IndexedVariationSlotRecord {
            family_id: detail.family_id.clone(),
            slot_id: "slot:handler".to_string(),
            description: "handler shape differs".to_string(),
        }];

        let selected = select_family_evidence(
            &detail,
            FamilyOutputOptions {
                evidence_mode: FamilyEvidenceMode::Evidence,
                token_budget: None,
                include_variations: true,
                include_exceptions: false,
            },
        );
        assert_eq!(
            selected
                .evidence
                .iter()
                .map(|evidence| evidence.record.evidence_id.as_str())
                .collect::<Vec<_>>(),
            vec![first.evidence_id.as_str()]
        );
        assert_eq!(selected.covered_claims, vec!["canonical", "support"]);
        assert_eq!(selected.missing_claims, vec!["variation"]);
        assert!(selected.budget_satisfied);
        assert!(!selected.source_snippets_included);
        assert!(selected.estimated_tokens > 0);
        assert!(!selected.evidence[0]
            .covered_claims
            .contains(&"variation".to_string()));
    }

    #[test]
    fn family_evidence_selection_uses_stored_claim_coverage_labels() {
        let first = family_evidence("src/routes/b.ts", 0, "first stored evidence");
        let mut second = family_evidence("src/routes/a.ts", 1, "alternate handler shape");
        second.covered_claims = vec!["variation".to_string()];
        let mut detail = family_detail_with_evidence(vec![first.clone(), second.clone()]);
        detail.variation_slots = vec![IndexedVariationSlotRecord {
            family_id: detail.family_id.clone(),
            slot_id: "slot:handler".to_string(),
            description: "handler shape differs".to_string(),
        }];

        let selected = select_family_evidence(
            &detail,
            FamilyOutputOptions {
                evidence_mode: FamilyEvidenceMode::Evidence,
                token_budget: None,
                include_variations: true,
                include_exceptions: false,
            },
        );

        assert_eq!(
            selected
                .evidence
                .iter()
                .map(|evidence| evidence.record.evidence_id.as_str())
                .collect::<Vec<_>>(),
            vec![first.evidence_id.as_str(), second.evidence_id.as_str()]
        );
        assert_eq!(
            selected.covered_claims,
            vec!["canonical", "support", "variation"]
        );
        assert!(selected.missing_claims.is_empty());
        assert_eq!(selected.evidence[1].covered_claims, vec!["variation"]);
    }

    #[test]
    fn family_evidence_selection_reports_missing_claim_coverage_and_tiny_budget() {
        let first = family_evidence("src/routes/b.ts", 0, "first stored evidence");
        let mut detail = family_detail_with_evidence(vec![first.clone()]);
        detail.variation_slots = vec![IndexedVariationSlotRecord {
            family_id: detail.family_id.clone(),
            slot_id: "slot:handler".to_string(),
            description: "handler shape differs".to_string(),
        }];

        let tiny_budget = select_family_evidence(
            &detail,
            FamilyOutputOptions {
                evidence_mode: FamilyEvidenceMode::Evidence,
                token_budget: Some(1),
                include_variations: true,
                include_exceptions: true,
            },
        );
        assert_eq!(tiny_budget.evidence[0].record, first);
        assert_eq!(tiny_budget.covered_claims, vec!["canonical", "support"]);
        assert_eq!(tiny_budget.missing_claims, vec!["variation", "exception"]);
        assert!(tiny_budget.estimated_tokens > 1);
        assert!(!tiny_budget.budget_satisfied);
        assert!(!tiny_budget.source_snippets_included);
    }

    #[test]
    fn deep_family_evidence_mode_remains_metadata_only_without_safe_span_reader() {
        let detail = family_detail_with_evidence(vec![family_evidence(
            "src/routes/a.ts",
            0,
            "support evidence, not a source snippet",
        )]);

        let selected = select_family_evidence(
            &detail,
            FamilyOutputOptions {
                evidence_mode: FamilyEvidenceMode::Deep,
                token_budget: None,
                include_variations: false,
                include_exceptions: false,
            },
        );

        assert_eq!(selected.mode, FamilyEvidenceMode::Deep);
        assert_eq!(selected.evidence.len(), 1);
        assert_eq!(
            selected.evidence[0].covered_claims,
            vec!["canonical", "support"]
        );
        assert!(!selected.source_snippets_included);
    }

    #[test]
    fn read_plan_marks_target_body_required_without_source_snippets() {
        let detail = family_detail_with_evidence(vec![family_evidence(
            "src/routes/a.ts",
            0,
            "canonical support evidence",
        )]);

        let read_plan = build_read_plan(
            &detail,
            Some("src/routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
            FamilyOutputOptions::default(),
        );

        assert_eq!(read_plan.items.len(), 1);
        assert!(read_plan.requires_source_before_edit);
        assert!(!read_plan.source_snippets_included);
        assert!(read_plan.budget_satisfied);
        let item = &read_plan.items[0];
        assert_eq!(item.purpose, ReadPlanPurpose::TargetBodyRequiredForEdit);
        assert_eq!(item.path, "src/routes/a.ts");
        assert_eq!(item.content_hash, semantic_fact().content_hash);
        assert_eq!(item.start_byte, 0);
        assert_eq!(item.end_byte, 20);
        assert_eq!(item.start_line, None);
        assert_eq!(item.end_line, None);
        assert!(item.source_required_before_edit);
        assert!(!item.source_snippets_included);
        assert!(item.estimated_tokens > 0);
        let debug = format!("{read_plan:?}");
        assert!(!debug.contains("function"));
        assert!(!debug.contains("/repo"));
    }

    #[test]
    fn estimated_potential_token_savings_is_estimated_from_omitted_family_metadata() {
        let detail = family_detail_with_evidence(vec![
            family_evidence("src/routes/a.ts", 0, "canonical support evidence"),
            family_evidence("src/routes/b.ts", 1, "second support evidence"),
        ]);
        let options = FamilyOutputOptions::default();
        let selected = select_family_evidence(&detail, options);
        let read_plan = build_read_plan(
            &detail,
            Some("src/routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
            options,
        );

        let metric =
            estimate_family_output_potential_token_savings(&detail, &selected, &read_plan, None);

        assert_eq!(
            metric.measurement_kind,
            EstimatedPotentialTokenSavings::new(1, 1).measurement_kind
        );
        assert!(metric.estimated_baseline_tokens > metric.estimated_returned_tokens);
        assert!(metric.estimated_potential_token_savings > 0);
        assert!(metric.caveat.contains("not measured token savings"));

        let rendered = SourceSpanRenderReport {
            policy: SourceSpanPolicy {
                requested: true,
                source_snippets_included: true,
                estimated_tokens: usize::MAX,
                budget_satisfied: false,
                selection_strategy: "hash_checked_line_numbered_spans_v1",
                fallback_guidance: "test",
            },
            spans: Vec::new(),
            omissions: Vec::new(),
        };
        let saturated = estimate_family_output_potential_token_savings(
            &detail,
            &selected,
            &read_plan,
            Some(&rendered),
        );
        assert_eq!(saturated.estimated_potential_token_savings, 0);
    }

    #[test]
    fn read_plan_selection_is_deterministic_and_budget_bounded() {
        let first = family_evidence("src/routes/a.ts", 0, "canonical support evidence");
        let second = family_evidence("src/routes/b.ts", 1, "second support evidence");
        let mut third = family_evidence("src/routes/c.ts", 2, "variation evidence");
        third.covered_claims = vec!["variation".to_string()];
        let mut detail = family_detail_with_evidence(vec![first, second, third]);
        detail.variation_slots = vec![IndexedVariationSlotRecord {
            family_id: detail.family_id.clone(),
            slot_id: "slot:method".to_string(),
            description: "method differs".to_string(),
        }];
        let options = FamilyOutputOptions {
            evidence_mode: FamilyEvidenceMode::Evidence,
            token_budget: Some(1),
            include_variations: true,
            include_exceptions: false,
        };

        let first_plan = build_read_plan(&detail, None, FamilyLookupMode::ExactFamilyId, options);
        let second_plan = build_read_plan(&detail, None, FamilyLookupMode::ExactFamilyId, options);

        assert_eq!(first_plan, second_plan);
        assert_eq!(first_plan.items.len(), 1);
        assert_eq!(
            first_plan.items[0].purpose,
            ReadPlanPurpose::CanonicalEvidence
        );
        assert_eq!(first_plan.items[0].path, "src/routes/a.ts");
        assert!(first_plan.estimated_tokens > 1);
        assert!(!first_plan.budget_satisfied);
        assert!(!first_plan.source_snippets_included);
    }

    fn read_plan_for_source_span(start_byte: usize, end_byte: usize) -> ReadPlan {
        ReadPlan {
            items: vec![ReadPlanItem {
                purpose: ReadPlanPurpose::CanonicalEvidence,
                path: "src/a.ts".to_string(),
                content_hash: semantic_fact().content_hash,
                start_byte,
                end_byte,
                start_line: None,
                end_line: None,
                estimated_tokens: 8,
                why: "canonical source span supporting the family claim".to_string(),
                source_required_before_edit: false,
                source_snippets_included: false,
            }],
            estimated_tokens: 8,
            source_snippets_included: false,
            requires_source_before_edit: false,
            selection_strategy: "deterministic_read_plan_v1",
            budget_satisfied: true,
        }
    }

    #[test]
    fn source_span_rendering_is_explicit_opt_in() {
        let read_plan = read_plan_for_source_span(6, 12);
        let report = render_source_spans(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &source_store_with_hash(semantic_fact().content_hash),
            &read_plan,
            false,
            None,
        )
        .expect("render report");

        assert!(!report.policy.requested);
        assert!(!report.policy.source_snippets_included);
        assert!(report.spans.is_empty());
        assert!(report.omissions.is_empty());
        let hydrated = read_plan_with_rendered_spans(&read_plan, &report);
        assert!(!hydrated.source_snippets_included);
        assert_eq!(hydrated.items[0].start_line, None);
    }

    #[test]
    fn source_span_rendering_returns_line_numbered_hash_checked_spans() {
        let source = SourceText {
            path: "src/a.ts".to_string(),
            content_hash: semantic_fact().content_hash,
            text: "first\nsecond\nthird\n".to_string(),
        };
        let read_plan = read_plan_for_source_span(6, 12);

        let report = render_source_spans(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &StaticSourceStore { result: Ok(source) },
            &read_plan,
            true,
            None,
        )
        .expect("render report");

        assert!(report.policy.requested);
        assert!(report.policy.source_snippets_included);
        assert_eq!(report.spans.len(), 1);
        assert!(report.omissions.is_empty());
        assert_eq!(report.spans[0].start_line, 2);
        assert_eq!(report.spans[0].end_line, 2);
        assert_eq!(report.spans[0].text, "2\tsecond");
        let hydrated = read_plan_with_rendered_spans(&read_plan, &report);
        assert!(hydrated.source_snippets_included);
        assert_eq!(hydrated.items[0].start_line, Some(2));
        assert_eq!(hydrated.items[0].end_line, Some(2));
        assert!(hydrated.items[0].source_snippets_included);
    }

    #[test]
    fn source_span_rendering_omits_invalid_ranges_without_panicking() {
        let source = SourceText {
            path: "src/a.ts".to_string(),
            content_hash: semantic_fact().content_hash,
            text: "first\nsecond\nthird\n".to_string(),
        };
        let read_plan = read_plan_for_source_span(12, 6);

        let report = render_source_spans(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &StaticSourceStore { result: Ok(source) },
            &read_plan,
            true,
            None,
        )
        .expect("render report");

        assert!(report.spans.is_empty());
        assert_eq!(report.omissions.len(), 1);
        assert_eq!(report.omissions[0].reason, "invalid_source_range");
        assert!(report.omissions[0].guidance.contains("Read/Grep"));
    }

    #[test]
    fn source_span_rendering_omits_stale_or_hash_mismatched_source() {
        let read_plan = read_plan_for_source_span(6, 12);

        let report = render_source_spans(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &hash_mismatch_source_store("hash mismatch"),
            &read_plan,
            true,
            None,
        )
        .expect("render report");

        assert!(report.spans.is_empty());
        assert_eq!(report.omissions.len(), 1);
        assert_eq!(report.omissions[0].reason, "stale_evidence");
        assert!(report.policy.fallback_guidance.contains("omitted"));
        assert!(!read_plan_with_rendered_spans(&read_plan, &report).source_snippets_included);
    }

    #[test]
    fn read_plan_includes_metadata_only_unknown_guard_when_family_has_unknowns() {
        let first = family_evidence("src/routes/a.ts", 0, "canonical support evidence");
        let second = family_evidence("src/routes/b.ts", 1, "second support evidence");
        let third = family_evidence("src/routes/c.ts", 2, "unknown guard evidence");
        let mut detail = family_detail_with_evidence(vec![first, second, third]);
        detail.unknowns = vec![FamilyQueryUnknown {
            class: UnknownClass::NonBlocking,
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "family:typescript:express_route:express:runtime_equivalence"
                .to_string(),
            recovery: Some("add semantic-worker evidence".to_string()),
        }];

        let read_plan = build_read_plan(
            &detail,
            None,
            FamilyLookupMode::ExactFamilyId,
            FamilyOutputOptions::default(),
        );

        let unknown_item = read_plan
            .items
            .iter()
            .find(|item| item.purpose == ReadPlanPurpose::UnknownBlocker)
            .expect("family unknowns should add a source-backed read-plan guard");
        assert_eq!(unknown_item.path, "src/routes/c.ts");
        assert!(!unknown_item.source_required_before_edit);
        assert!(!unknown_item.source_snippets_included);
    }

    #[test]
    fn repo_shape_diagnostics_reports_local_pattern_density() {
        let units = vec![
            indexed_python_unit("app/a.py", "fastapi_route", 0),
            indexed_python_unit("app/b.py", "fastapi_route", 1),
            indexed_python_unit("app/c.py", "fastapi_route", 2),
            indexed_python_unit("app/d.py", "fastapi_route", 3),
        ];
        let members = units
            .iter()
            .take(3)
            .map(|unit| IndexedFamilyMemberRecord {
                family_id: "family:python:fastapi_route:framework_fastapi_route".to_string(),
                code_unit_id: unit.id.clone(),
                role: "framework:fastapi.route".to_string(),
            })
            .collect::<Vec<_>>();
        let index_store = FakeStore::new(Vec::new()).with_units(units);
        let family_store = family_store_with_members(members);

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        assert_eq!(diagnostics.eligible_code_units, 4);
        assert_eq!(diagnostics.family_count, 1);
        assert_eq!(diagnostics.family_member_count, 3);
        assert_eq!(diagnostics.covered_code_units, 3);
        assert_eq!(diagnostics.local_pattern_density, Some(0.75));
        assert_eq!(diagnostics.family_support_coverage, Some(0.75));
        assert_eq!(diagnostics.abstention_rate, Some(0.25));
        assert_eq!(diagnostics.token_saving_risk, DiagnosticSignal::Low);
        assert_eq!(diagnostics.thin_wrapper_risk, DiagnosticSignal::Low);
        assert_eq!(
            diagnostics.external_dependency_signal,
            DiagnosticSignal::Unknown
        );
    }

    #[test]
    fn repo_shape_diagnostics_abstains_when_no_eligible_python_units_exist() {
        let index_store = FakeStore::new(Vec::new()).with_units(vec![indexed_unit("src/a.ts")]);
        let family_store = FakeFamilyStore::empty();

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        assert_eq!(diagnostics.eligible_code_units, 0);
        assert_eq!(diagnostics.local_pattern_density, None);
        assert_eq!(diagnostics.family_support_coverage, None);
        assert_eq!(diagnostics.abstention_rate, None);
        assert_eq!(diagnostics.token_saving_risk, DiagnosticSignal::Unknown);
        assert_eq!(diagnostics.thin_wrapper_risk, DiagnosticSignal::Unknown);
    }

    #[test]
    fn family_lookup_rejects_short_substring_false_matches() {
        let store = FakeFamilyStore::with_family();

        for (target, mode) in [
            ("express", FamilyLookupMode::ExactFamilyId),
            ("src/routes/a.ts", FamilyLookupMode::ExactMemberId),
            ("routes", FamilyLookupMode::FuzzyQuery),
            ("DOMINANT_PATTERN", FamilyLookupMode::FuzzyQuery),
            ("framework:express", FamilyLookupMode::FuzzyQuery),
        ] {
            let lookup = lookup_family(&store, Some(target), mode).expect("lookup family");
            let FamilyLookupReport::Unknown(report) = lookup else {
                panic!("short target {target} must not match");
            };
            assert_eq!(
                report.unknowns[0].reason,
                UnknownReasonCode::InsufficientSupport
            );
        }
    }

    #[test]
    fn stale_family_evidence_blocks_public_family_claims() {
        let store = FakeFamilyStore::with_family();
        let source_store = FamilyEvidenceSourceStore {
            path: "src/routes/a.ts".to_string(),
            result: Err(SourceStoreError::HashMismatch(
                "source content changed after discovery".to_string(),
            )),
        };

        let lookup = lookup_family_with_freshness(
            family_freshness_request(),
            &store,
            &source_store,
            Some("family:typescript:express_route:express"),
            FamilyLookupMode::ExactFamilyId,
        )
        .expect("lookup with freshness");
        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("stale family evidence must block public detail");
        };
        assert_eq!(report.unknowns[0].reason, UnknownReasonCode::StaleEvidence);
        assert_eq!(
            report.unknowns[0].recovery.as_deref(),
            Some("run repogrammar sync")
        );

        let families =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");
        assert!(families.families.is_empty());
        assert_eq!(
            families.unknowns[0].reason,
            UnknownReasonCode::StaleEvidence
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
