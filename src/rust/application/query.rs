//! Query use-case boundary for finding repository analogues.

use crate::application::family::{
    classify_unknown_family_effect, FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX,
};
use crate::application::recovery::{
    classify_recovery, recovery_guidance, RecoveryAction, RecoveryAgentState,
    RecoveryAutosyncState, RecoveryContext, RecoveryEvidenceState, RecoveryFreshness,
    RecoveryHealth, RecoveryLockState, RecoveryRecommendation,
};
use crate::application::repository::{
    repository_recovery_for_report, RepositoryImplementationStatus, RepositoryStatusReport,
};
use crate::core::mining::representative_selection::{
    select_representative_evidence, EvidenceCoverage, EvidenceSelectionCandidate,
};
use crate::core::model::{
    ClaimImpact, ContentHash, EstimatedPotentialTokenSavings, FactCertainty, FamilyPrevalence,
    ResolutionClass, SemanticFactKind, SemanticObligation, UnknownClass, UnknownReasonCode,
};
use crate::core::policy::freshness::{
    content_hash_freshness, semantic_fact_claim_input_readiness, ClaimInputReadiness,
};
use crate::error::RepoGrammarError;
use crate::ports::family_store::{
    ActiveFamily, ActiveFamilyCandidates, FamilyStore, IndexedFamilyCandidateRecord,
    IndexedFamilyEvidenceProjectionRecord, IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord,
    IndexedVariationSlotRecord, StoreError,
};
use crate::ports::index_store::{
    ActiveRepoShapeStats, IndexStore, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedSemanticFactRecord, RepoShapeLanguageStats,
};
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError, SourceText};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_QUERY_TARGET_BYTES: usize = 8 * 1024;
pub const MAX_QUERY_TOKEN_BUDGET: usize = 200_000;
pub const MAX_RENDERED_SOURCE_SPAN_BYTES: usize = 16 * 1024;
const FUZZY_FAMILY_CANDIDATE_LIMIT: usize = 5;

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
    let recovery = repository_recovery_for_report(status_report);
    match recovery.action {
        RecoveryAction::Setup => fallback(
            "repository is not initialized",
            recovery_guidance(RecoveryAction::Setup),
            operation.command_is_implemented(),
        ),
        RecoveryAction::RepairStorage | RecoveryAction::ResolveLock => {
            QueryPreflightReport::Fallback(repository_status_unavailable_fallback(operation))
        }
        RecoveryAction::Resync => fallback(
            if status_report.readiness.active_generation_available {
                "active index evidence is stale or unreadable"
            } else {
                "no active index generation"
            },
            "run repogrammar resync",
            operation.command_is_implemented(),
        ),
        RecoveryAction::UseSourceFallback => fallback(
            "repository evidence cannot be verified safely",
            "use source fallback",
            operation.command_is_implemented(),
        ),
        RecoveryAction::Unsupported => fallback(
            "query target is unsupported",
            "use source fallback",
            operation.command_is_implemented(),
        ),
        RecoveryAction::StartAutosync
            if recovery.reason
                != crate::application::recovery::RecoveryReason::AutosyncRecommended =>
        {
            fallback(
                "active index evidence is stale or unreadable",
                "run repogrammar autosync start",
                operation.command_is_implemented(),
            )
        }
        RecoveryAction::StartAutosync
        | RecoveryAction::InstallAgent(_)
        | RecoveryAction::InstallSupportedAgent
        | RecoveryAction::RepairAgentIntegration(_)
        | RecoveryAction::None => {
            if inventory_indexing_is_readable(status_report.indexing) {
                QueryPreflightReport::Ready
            } else {
                fallback(
                    "no active index generation",
                    "run repogrammar resync",
                    operation.command_is_implemented(),
                )
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

fn classify_query_evidence_recovery(
    freshness: RecoveryFreshness,
    family_evidence: RecoveryEvidenceState,
) -> RecoveryRecommendation {
    classify_recovery(&RecoveryContext {
        initialized: true,
        storage_health: RecoveryHealth::Healthy,
        lock_state: RecoveryLockState::Clear,
        active_index: true,
        freshness,
        family_evidence,
        autosync: RecoveryAutosyncState {
            configured: false,
            running: false,
            recommended: false,
        },
        agent: RecoveryAgentState::NotRequired,
    })
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

/// Per-family evidence-freshness verdict for the families listing. Derived from
/// hash-checked reads of the family's distinct evidence paths at query time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyFreshness {
    /// Every evidence path verified with a matching content hash.
    Fresh,
    /// At least one evidence path is missing or its content hash changed, so the
    /// claim no longer reflects the working tree.
    Stale,
    /// No stale path, but at least one path could not be verified for a
    /// non-content reason (too large, non-UTF-8, or otherwise unreadable). A
    /// family with zero evidence rows is also `CannotVerify`.
    CannotVerify,
}

impl FamilyFreshness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::CannotVerify => "cannot_verify",
        }
    }
}

/// Freshness rollup for the families listing: one deterministic count per state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FamilyFreshnessCounts {
    pub fresh_count: usize,
    pub stale_count: usize,
    pub cannot_verify_count: usize,
}

// Carries `FamilyPrevalence` (floating-point coverage ratio), so these reports
// derive `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilySummary {
    pub family_id: String,
    pub classification: String,
    pub support: usize,
    pub prevalence: FamilyPrevalence,
    /// `Some` only for the freshness-verified listing; `None` for the
    /// freshness-free `list_families` variant, which does not evaluate evidence.
    pub freshness: Option<FamilyFreshness>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyListReport {
    pub active_generation: String,
    pub families: Vec<FamilySummary>,
    /// `Some` only for the freshness-verified listing; carries the per-state
    /// rollup. `None` for the freshness-free `list_families` variant.
    pub freshness_counts: Option<FamilyFreshnessCounts>,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyDetailReport {
    pub active_generation: String,
    pub family_id: String,
    pub classification: String,
    pub support: usize,
    pub prevalence: FamilyPrevalence,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyUnknownReport {
    pub active_generation: String,
    pub candidate_family_ids: Vec<String>,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedQueryTarget {
    pub original_target: String,
    pub kind: &'static str,
    pub path: String,
    pub line: Option<usize>,
    pub byte_range: Option<(usize, usize)>,
    pub family_id: Option<String>,
    pub code_unit_id: Option<String>,
    pub symbol_hints: Vec<String>,
    pub residue_terms: Vec<String>,
    pub candidate_paths: Vec<String>,
    pub candidate_family_ids: Vec<String>,
    pub candidate_code_unit_ids: Vec<String>,
    pub confidence: &'static str,
    pub match_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyPartialContextReport {
    pub active_generation: String,
    pub resolved_target: ResolvedQueryTarget,
    pub read_plan: ReadPlan,
    pub unknowns: Vec<FamilyQueryUnknown>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FamilyLookupReport {
    Found(FamilyDetailReport),
    PartialContext(Box<FamilyPartialContextReport>),
    Unknown(FamilyUnknownReport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyQueryRouteReport {
    pub route: &'static str,
    pub input_kind: &'static str,
    pub pipeline: Vec<&'static str>,
    pub family_id_policy: &'static str,
    pub candidate_limit: Option<usize>,
    pub selected_family_id: Option<String>,
    pub candidate_family_ids: Vec<String>,
    pub follow_up_family_ids: Vec<String>,
    pub why_selected: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepoShapeDiagnosticsReport {
    pub active_generation: String,
    pub indexed_file_count: usize,
    pub indexed_code_unit_count: usize,
    pub semantic_fact_count: usize,
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_member_count: usize,
    pub covered_code_units: usize,
    pub by_language: Vec<RepoShapeLanguageDiagnostics>,
    pub local_pattern_density: Option<f64>,
    pub family_support_coverage: Option<f64>,
    pub abstention_rate: Option<f64>,
    pub external_dependency_signal: DiagnosticSignal,
    pub thin_wrapper_risk: DiagnosticSignal,
    pub token_saving_risk: DiagnosticSignal,
    pub token_saving_readiness: TokenSavingReadiness,
    pub blocking_reasons: Vec<&'static str>,
    pub interpretation: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepoShapeLanguageDiagnostics {
    pub language: String,
    pub language_scope: &'static str,
    pub indexed_file_count: usize,
    pub indexed_code_unit_count: usize,
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_member_count: usize,
    pub covered_code_units: usize,
    pub family_support_coverage: Option<f64>,
    pub support_risk: DiagnosticSignal,
    pub preview_status: &'static str,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSavingReadiness {
    Ready,
    Partial,
    Poor,
    Unknown,
}

impl TokenSavingReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Partial => "partial",
            Self::Poor => "poor",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyQueryUnknownMetric {
    pub unknown_class: &'static str,
    pub reason_code: &'static str,
    pub required_mechanism: String,
    pub obligation: &'static str,
    pub recovery_code: &'static str,
}

pub fn family_query_unknown_metric(unknown: &FamilyQueryUnknown) -> FamilyQueryUnknownMetric {
    let affected_claim = query_unknown_claim_for_mechanism(unknown.affected_claim.as_str());
    let language = query_unknown_language_for_claim(affected_claim);
    let framework_role = query_unknown_framework_role_for_claim(affected_claim);
    let disposition = classify_unknown_disposition(UnknownPolicyContext {
        language,
        reason: unknown.reason,
        affected_claim,
        framework_role,
        assumptions: &[],
        origin_engine: "",
        origin_method: "",
        explicit_legacy_class: Some(unknown.class),
        role_is_ambiguous: false,
        family_role_is_exact: false,
    });
    FamilyQueryUnknownMetric {
        unknown_class: unknown.class.as_protocol_str(),
        reason_code: unknown.reason.as_protocol_str(),
        required_mechanism: disposition.required_mechanism,
        obligation: disposition.obligation.as_protocol_str(),
        recovery_code: disposition.recovery_code,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownInventoryBucket {
    pub key: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownInventoryBlocksSupportBucket {
    pub blocks_support: bool,
    pub count: usize,
}

pub const UNKNOWN_INVENTORY_SCOPE: &str = "persisted_semantic_unknowns";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownInventoryReport {
    pub inventory_scope: &'static str,
    pub active_generation: String,
    pub total_unknowns: usize,
    pub blocking_unknowns: usize,
    pub non_blocking_unknowns: usize,
    pub recoverable_unknowns: usize,
    pub irreducible_unknowns: usize,
    pub by_language: Vec<UnknownInventoryBucket>,
    pub by_language_detail: Vec<UnknownInventoryLanguageSummary>,
    pub by_reason_code: Vec<UnknownInventoryBucket>,
    pub by_required_mechanism: Vec<UnknownInventoryBucket>,
    pub by_obligation: Vec<UnknownInventoryBucket>,
    pub by_framework_role: Vec<UnknownInventoryBucket>,
    pub by_role_state: Vec<UnknownInventoryBucket>,
    pub by_blocks_support: Vec<UnknownInventoryBlocksSupportBucket>,
    pub by_recovery_code: Vec<UnknownInventoryBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownInventoryLanguageSummary {
    pub language: String,
    pub total_unknowns: usize,
    pub blocking_unknowns: usize,
    pub top_required_mechanisms: Vec<UnknownInventoryBucket>,
    pub top_reason_codes: Vec<UnknownInventoryBucket>,
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
pub struct ReadPlanLineRangeOmission {
    pub purpose: ReadPlanPurpose,
    pub path: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub reason: &'static str,
    pub guidance: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    pub items: Vec<ReadPlanItem>,
    pub estimated_tokens: usize,
    pub source_snippets_included: bool,
    pub requires_source_before_edit: bool,
    pub selection_strategy: &'static str,
    pub budget_satisfied: bool,
    pub line_range_omissions: Vec<ReadPlanLineRangeOmission>,
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

pub fn family_query_route_report(
    report: &FamilyLookupReport,
    mode: FamilyLookupMode,
) -> FamilyQueryRouteReport {
    let selected_family_id = match report {
        FamilyLookupReport::Found(family) => Some(family.family_id.clone()),
        FamilyLookupReport::PartialContext(report) => report.resolved_target.family_id.clone(),
        FamilyLookupReport::Unknown(_) => None,
    };
    let candidate_family_ids = report_candidate_family_ids(report);
    let follow_up_family_ids =
        family_id_handles(&candidate_family_ids, selected_family_id.as_ref());
    FamilyQueryRouteReport {
        route: query_route_name(report, mode),
        input_kind: query_route_input_kind(mode),
        pipeline: query_route_pipeline(report, mode),
        family_id_policy: query_route_family_id_policy(mode),
        candidate_limit: (mode == FamilyLookupMode::FuzzyQuery)
            .then_some(FUZZY_FAMILY_CANDIDATE_LIMIT),
        selected_family_id,
        candidate_family_ids,
        follow_up_family_ids,
        why_selected: query_route_why(report, mode),
    }
}

fn query_route_name(report: &FamilyLookupReport, mode: FamilyLookupMode) -> &'static str {
    match (report, mode) {
        (FamilyLookupReport::Found(_), FamilyLookupMode::FuzzyQuery) => "discover_hydrate_compose",
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactFamilyId) => "exact_family_hydrate",
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactMemberId) => "exact_member_hydrate",
        (FamilyLookupReport::PartialContext(_), _) => "partial_context_read_plan",
        (FamilyLookupReport::Unknown(_), FamilyLookupMode::FuzzyQuery) => "discovery_unknown",
        (FamilyLookupReport::Unknown(_), _) => "exact_lookup_unknown",
    }
}

fn query_route_input_kind(mode: FamilyLookupMode) -> &'static str {
    match mode {
        FamilyLookupMode::FuzzyQuery => "path_symbol_role_or_pattern_target",
        FamilyLookupMode::ExactFamilyId => "family_id_follow_up_handle",
        FamilyLookupMode::ExactMemberId => "member_or_code_unit_follow_up_handle",
    }
}

fn query_route_family_id_policy(mode: FamilyLookupMode) -> &'static str {
    match mode {
        FamilyLookupMode::FuzzyQuery => {
            "family_ids_are_returned_follow_up_handles_not_required_initial_inputs"
        }
        FamilyLookupMode::ExactFamilyId => "show_family_requires_exact_family_id",
        FamilyLookupMode::ExactMemberId => "member_lookup_requires_exact_member_or_code_unit_id",
    }
}

fn query_route_pipeline(report: &FamilyLookupReport, mode: FamilyLookupMode) -> Vec<&'static str> {
    match (report, mode) {
        (FamilyLookupReport::Found(_), FamilyLookupMode::FuzzyQuery) => vec![
            "discover_candidates",
            "hydrate_bounded_candidates",
            "select_single_fresh_family",
            "compose_context_bundle",
        ],
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactFamilyId) => {
            vec!["hydrate_exact_family", "compose_context_bundle"]
        }
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactMemberId) => vec![
            "resolve_exact_member",
            "hydrate_single_family",
            "compose_context_bundle",
        ],
        (FamilyLookupReport::PartialContext(_), _) => vec![
            "discover_candidates",
            "resolve_local_target",
            "compose_read_plan",
        ],
        (FamilyLookupReport::Unknown(_), FamilyLookupMode::FuzzyQuery) => {
            vec!["discover_candidates", "abstain"]
        }
        (FamilyLookupReport::Unknown(_), FamilyLookupMode::ExactFamilyId) => {
            vec!["hydrate_exact_family", "abstain"]
        }
        (FamilyLookupReport::Unknown(_), FamilyLookupMode::ExactMemberId) => {
            vec!["resolve_exact_member", "abstain"]
        }
    }
}

fn query_route_why(report: &FamilyLookupReport, mode: FamilyLookupMode) -> &'static str {
    match (report, mode) {
        (FamilyLookupReport::Found(_), FamilyLookupMode::FuzzyQuery) => {
            "target resolved to one fresh candidate family; RepoGrammar hydrated that family and composed bounded context"
        }
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactFamilyId) => {
            "exact family id was used as a follow-up handle and hydrated directly"
        }
        (FamilyLookupReport::Found(_), FamilyLookupMode::ExactMemberId) => {
            "exact member or code-unit id resolved to one family and hydrated that family"
        }
        (FamilyLookupReport::PartialContext(_), _) => {
            "target resolved to one indexed path or code unit but family evidence was insufficient; returned read-plan context only"
        }
        (FamilyLookupReport::Unknown(_), FamilyLookupMode::FuzzyQuery) => {
            "candidate discovery or local target resolution could not produce a single supported family without overclaiming"
        }
        (FamilyLookupReport::Unknown(_), _) => {
            "exact follow-up handle did not resolve to a supported fresh family"
        }
    }
}

fn report_candidate_family_ids(report: &FamilyLookupReport) -> Vec<String> {
    match report {
        FamilyLookupReport::Found(family) => vec![family.family_id.clone()],
        FamilyLookupReport::PartialContext(report) => {
            let mut ids = report.resolved_target.candidate_family_ids.clone();
            if let Some(family_id) = &report.resolved_target.family_id {
                ids.push(family_id.clone());
            }
            normalized_family_ids(ids)
        }
        FamilyLookupReport::Unknown(report) => {
            normalized_family_ids(report.candidate_family_ids.clone())
        }
    }
}

fn family_id_handles(
    candidate_family_ids: &[String],
    selected_family_id: Option<&String>,
) -> Vec<String> {
    let mut ids = candidate_family_ids.to_vec();
    if let Some(family_id) = selected_family_id {
        ids.push(family_id.clone());
    }
    normalized_family_ids(ids)
}

fn normalized_family_ids(ids: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    ids.into_iter()
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect()
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
    let active = store
        .list_active_indexed_files()
        .map_err(index_store_error)?;
    let units = store.list_active_code_units().map_err(index_store_error)?;
    // The file list and the unit count come from two separate reads. If the
    // active generation was switched between them, the reported files and the
    // derived indexing label could describe different generations.
    if active.generation_id != units.generation_id {
        return Err(RepoGrammarError::InvalidInput(
            "active inventory generation changed during indexed files read".to_string(),
        ));
    }
    Ok(IndexedFilesReport {
        active_generation: active.generation_id,
        indexing: inventory_indexing_for_unit_count(units.units.len()).to_string(),
        files: active.files,
    })
}

pub fn list_code_units(
    store: &impl IndexStore,
) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
    let active = store.list_active_code_units().map_err(index_store_error)?;
    Ok(IndexedCodeUnitsReport {
        indexing: inventory_indexing_for_unit_count(active.units.len()).to_string(),
        active_generation: active.generation_id,
        units: active.units,
    })
}

pub fn list_semantic_facts(
    store: &impl IndexStore,
) -> Result<IndexedSemanticFactsReport, RepoGrammarError> {
    let active = store
        .list_active_semantic_facts()
        .map_err(index_store_error)?;
    Ok(IndexedSemanticFactsReport {
        active_generation: active.generation_id,
        facts: active.facts,
    })
}

pub fn unknown_inventory(
    store: &impl IndexStore,
) -> Result<UnknownInventoryReport, RepoGrammarError> {
    let facts = store
        .list_active_semantic_facts()
        .map_err(index_store_error)?;
    let files = store
        .list_active_indexed_files()
        .map_err(index_store_error)?;
    let units = store.list_active_code_units().map_err(index_store_error)?;
    if files.generation_id != facts.generation_id || units.generation_id != facts.generation_id {
        return Err(RepoGrammarError::InvalidInput(
            "active inventory generation changed during unknown inventory read".to_string(),
        ));
    }
    Ok(build_unknown_inventory(
        facts.generation_id,
        facts.facts,
        files.files,
        units.units,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnknownInventoryEntry {
    language: String,
    reason: UnknownReasonCode,
    class: UnknownClass,
    claim_impact: ClaimImpact,
    resolution_class: ResolutionClass,
    required_mechanism: String,
    obligation: SemanticObligation,
    framework_role: String,
    role_state: UnknownInventoryRoleState,
    recovery_code: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnknownInventoryRoleState {
    None,
    Single,
    Ambiguous,
}

impl UnknownInventoryRoleState {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Single => "single",
            Self::Ambiguous => "ambiguous",
        }
    }
}

fn build_unknown_inventory(
    generation_id: String,
    semantic_facts: Vec<IndexedSemanticFactRecord>,
    files: Vec<IndexedFileRecord>,
    units: Vec<IndexedCodeUnitRecord>,
) -> UnknownInventoryReport {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let file_language_by_path = files
        .iter()
        .map(|file| (file.path.as_str(), file.language.as_str()))
        .collect::<BTreeMap<_, _>>();
    let roles_by_unit = inventory_framework_roles_by_unit(&semantic_facts);
    let resolved_tsjs_operation_keys = provider_resolved_tsjs_operation_keys(&semantic_facts);
    let mut entries = semantic_facts
        .iter()
        .filter(|fact| inventory_fact_is_unknown(fact))
        .filter(|fact| {
            !tsjs_unknown_resolved_by_provider_operation(fact, &resolved_tsjs_operation_keys)
        })
        .map(|fact| {
            let reason = inventory_unknown_reason(fact);
            let language = unit_by_id
                .get(fact.code_unit_id.as_str())
                .map(|unit| unit.language.as_str())
                .or_else(|| file_language_by_path.get(fact.path.as_str()).copied())
                .unwrap_or("unknown");
            let affected_claim = assumption_value(&fact.assumptions, "affected_claim")
                .unwrap_or_else(|| default_unknown_affected_claim(language).to_string());
            let (framework_role, role_state) =
                inventory_framework_role(&fact.assumptions, &roles_by_unit, &fact.code_unit_id);
            let explicit_class = assumption_value(&fact.assumptions, "unknown_class")
                .and_then(|class| UnknownClass::parse_protocol_str(&class).ok());
            let disposition = classify_unknown_disposition(UnknownPolicyContext {
                language,
                reason,
                affected_claim: &affected_claim,
                framework_role: &framework_role,
                assumptions: &fact.assumptions,
                origin_engine: &fact.origin_engine,
                origin_method: &fact.origin_method,
                explicit_legacy_class: explicit_class,
                role_is_ambiguous: role_state == UnknownInventoryRoleState::Ambiguous,
                family_role_is_exact: role_state == UnknownInventoryRoleState::Single,
            });
            UnknownInventoryEntry {
                language: language.to_string(),
                reason,
                class: disposition.legacy_class,
                claim_impact: disposition.claim_impact,
                resolution_class: disposition.resolution_class,
                required_mechanism: disposition.required_mechanism,
                obligation: disposition.obligation,
                framework_role,
                role_state,
                recovery_code: disposition.recovery_code,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        (
            left.language.as_str(),
            left.reason.as_protocol_str(),
            left.class.as_protocol_str(),
            left.claim_impact
                .as_legacy_unknown_class()
                .as_protocol_str(),
            left.resolution_class
                .as_legacy_unknown_class()
                .as_protocol_str(),
            left.required_mechanism.as_str(),
            left.obligation.as_protocol_str(),
            left.framework_role.as_str(),
            left.role_state.as_str(),
            left.recovery_code,
        )
            .cmp(&(
                right.language.as_str(),
                right.reason.as_protocol_str(),
                right.class.as_protocol_str(),
                right
                    .claim_impact
                    .as_legacy_unknown_class()
                    .as_protocol_str(),
                right
                    .resolution_class
                    .as_legacy_unknown_class()
                    .as_protocol_str(),
                right.required_mechanism.as_str(),
                right.obligation.as_protocol_str(),
                right.framework_role.as_str(),
                right.role_state.as_str(),
                right.recovery_code,
            ))
    });

    UnknownInventoryReport {
        inventory_scope: UNKNOWN_INVENTORY_SCOPE,
        active_generation: generation_id,
        total_unknowns: entries.len(),
        blocking_unknowns: count_unknown_class(&entries, UnknownClass::Blocking),
        non_blocking_unknowns: count_unknown_class(&entries, UnknownClass::NonBlocking),
        recoverable_unknowns: count_unknown_class(&entries, UnknownClass::Recoverable),
        irreducible_unknowns: count_unknown_class(&entries, UnknownClass::Irreducible),
        by_language: aggregate_unknown_bucket(&entries, |entry| entry.language.as_str()),
        by_language_detail: aggregate_unknown_language_detail(&entries),
        by_reason_code: aggregate_unknown_bucket(&entries, |entry| entry.reason.as_protocol_str()),
        by_required_mechanism: aggregate_unknown_bucket(&entries, |entry| {
            entry.required_mechanism.as_str()
        }),
        by_obligation: aggregate_unknown_bucket(&entries, |entry| {
            entry.obligation.as_protocol_str()
        }),
        by_framework_role: aggregate_unknown_bucket(&entries, |entry| {
            entry.framework_role.as_str()
        }),
        by_role_state: aggregate_unknown_bucket(&entries, |entry| entry.role_state.as_str()),
        by_blocks_support: aggregate_blocks_support(&entries),
        by_recovery_code: aggregate_unknown_bucket(&entries, |entry| entry.recovery_code),
    }
}

fn aggregate_unknown_language_detail(
    entries: &[UnknownInventoryEntry],
) -> Vec<UnknownInventoryLanguageSummary> {
    readiness_language_scopes()
        .iter()
        .map(|scope| {
            let matching = entries
                .iter()
                .filter(|entry| inventory_language_scope(entry.language.as_str()) == scope.language)
                .collect::<Vec<_>>();
            UnknownInventoryLanguageSummary {
                language: scope.language.to_string(),
                total_unknowns: matching.len(),
                blocking_unknowns: matching
                    .iter()
                    .filter(|entry| entry.claim_impact == ClaimImpact::Blocking)
                    .count(),
                top_required_mechanisms: top_unknown_buckets(
                    matching
                        .iter()
                        .map(|entry| entry.required_mechanism.as_str()),
                ),
                top_reason_codes: top_unknown_buckets(
                    matching.iter().map(|entry| entry.reason.as_protocol_str()),
                ),
            }
        })
        .collect()
}

fn top_unknown_buckets<'a>(values: impl Iterator<Item = &'a str>) -> Vec<UnknownInventoryBucket> {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        *counts.entry(value.to_string()).or_default() += 1;
    }
    let mut buckets = counts
        .into_iter()
        .map(|(key, count)| UnknownInventoryBucket { key, count })
        .collect::<Vec<_>>();
    buckets.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.key.cmp(&right.key))
    });
    buckets.truncate(3);
    buckets
}

type TsJsOperationEvidenceKey = (String, String, String, usize, usize, String);

fn provider_resolved_tsjs_operation_keys(
    facts: &[IndexedSemanticFactRecord],
) -> BTreeSet<TsJsOperationEvidenceKey> {
    facts
        .iter()
        .filter(|fact| {
            matches!(fact.kind.as_str(), "RESOLVED_IMPORT" | "SYMBOL" | "TYPE")
                && fact.certainty == FactCertainty::Semantic.as_protocol_str()
                && fact.origin_engine == "typescript"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider=typescript")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=true")
        })
        .filter_map(|fact| {
            let operation = assumption_value(&fact.assumptions, "query_operation")?;
            Some(tsjs_operation_key_for_fact(fact, &operation))
        })
        .collect()
}

fn tsjs_unknown_resolved_by_provider_operation(
    fact: &IndexedSemanticFactRecord,
    resolved_keys: &BTreeSet<TsJsOperationEvidenceKey>,
) -> bool {
    if fact.kind != "UNKNOWN" || !fact.origin_method.contains("import_resolver") {
        return false;
    }
    let Some(affected_claim) = assumption_value(&fact.assumptions, "affected_claim") else {
        return false;
    };
    let Some(operation) = tsjs_operation_for_affected_claim(&affected_claim) else {
        return false;
    };
    resolved_keys.contains(&tsjs_operation_key_for_fact(fact, operation))
}

fn tsjs_operation_for_affected_claim(affected_claim: &str) -> Option<&'static str> {
    match affected_claim {
        "tsjs_import_resolution" => Some("resolve_module_specifier"),
        "tsjs_export_resolution" => Some("resolve_export"),
        "tsjs_reexport_resolution" => Some("resolve_reexport"),
        "tsjs_package_entry" => Some("resolve_package_entry"),
        _ => None,
    }
}

fn tsjs_operation_key_for_fact(
    fact: &IndexedSemanticFactRecord,
    operation: &str,
) -> TsJsOperationEvidenceKey {
    (
        fact.path.clone(),
        fact.content_hash.as_str().to_string(),
        fact.code_unit_id.clone(),
        fact.start_byte,
        fact.end_byte,
        operation.to_string(),
    )
}

fn inventory_fact_is_unknown(fact: &IndexedSemanticFactRecord) -> bool {
    fact.kind == "UNKNOWN" || fact.certainty == "UNKNOWN"
}

fn inventory_unknown_reason(fact: &IndexedSemanticFactRecord) -> UnknownReasonCode {
    fact.target
        .as_deref()
        .and_then(|target| UnknownReasonCode::parse_protocol_str(target).ok())
        .unwrap_or(UnknownReasonCode::InsufficientSupport)
}

fn inventory_framework_roles_by_unit(
    facts: &[IndexedSemanticFactRecord],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut roles: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for fact in facts {
        if fact.kind != "FRAMEWORK_ROLE" || fact.certainty != "FRAMEWORK_HEURISTIC" {
            continue;
        }
        let Some(target) = fact.target.as_deref() else {
            continue;
        };
        roles
            .entry(fact.code_unit_id.clone())
            .or_default()
            .insert(target.to_string());
    }
    roles
}

fn inventory_framework_role(
    assumptions: &[String],
    roles_by_unit: &BTreeMap<String, BTreeSet<String>>,
    code_unit_id: &str,
) -> (String, UnknownInventoryRoleState) {
    if let Some(role) = assumption_value(assumptions, "framework_role") {
        let state = match role.as_str() {
            "unknown" => UnknownInventoryRoleState::None,
            "ambiguous" => UnknownInventoryRoleState::Ambiguous,
            _ => UnknownInventoryRoleState::Single,
        };
        return (role, state);
    }

    match roles_by_unit.get(code_unit_id) {
        None => ("unknown".to_string(), UnknownInventoryRoleState::None),
        Some(roles) if roles.len() == 1 => (
            roles.iter().next().expect("single role").clone(),
            UnknownInventoryRoleState::Single,
        ),
        Some(_) => (
            "ambiguous".to_string(),
            UnknownInventoryRoleState::Ambiguous,
        ),
    }
}

fn assumption_value(assumptions: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    assumptions.iter().find_map(|assumption| {
        assumption
            .strip_prefix(&prefix)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    })
}

fn default_unknown_affected_claim(language: &str) -> &'static str {
    if language == "python" {
        "python_family_membership"
    } else if is_tsjs_language(language) {
        "tsjs_family_membership"
    } else if language == "java" {
        "java_family_membership"
    } else if language == "csharp" {
        "csharp_family_membership"
    } else if is_c_cpp_language(language) {
        "cpp_family_membership"
    } else if language == "rust" {
        "rust_family_membership"
    } else {
        "semantic_fact"
    }
}

fn default_legacy_unknown_class(reason: UnknownReasonCode) -> UnknownClass {
    match reason {
        UnknownReasonCode::MonkeyPatch => UnknownClass::Irreducible,
        _ => UnknownClass::Recoverable,
    }
}

#[derive(Debug, Clone, Copy)]
struct UnknownPolicyContext<'a> {
    language: &'a str,
    reason: UnknownReasonCode,
    affected_claim: &'a str,
    framework_role: &'a str,
    assumptions: &'a [String],
    origin_engine: &'a str,
    origin_method: &'a str,
    explicit_legacy_class: Option<UnknownClass>,
    role_is_ambiguous: bool,
    family_role_is_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnknownDisposition {
    claim_impact: ClaimImpact,
    resolution_class: ResolutionClass,
    legacy_class: UnknownClass,
    required_mechanism: String,
    obligation: SemanticObligation,
    recovery_code: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegisteredRecoveryMechanism {
    resolution_class: ResolutionClass,
    recovery_code: &'static str,
}

/// Single internal entrypoint for the orthogonal effect and recoverability axes.
/// Public callers keep receiving the legacy `UnknownClass` projection.
fn classify_unknown_disposition(context: UnknownPolicyContext<'_>) -> UnknownDisposition {
    let family_effect = classify_unknown_family_effect(
        context.language,
        context.reason,
        context.affected_claim,
        context
            .family_role_is_exact
            .then_some(context.framework_role),
        context.origin_engine,
        context.origin_method,
    );
    let family_impact = family_effect
        .as_ref()
        .and_then(|unknown| unknown.claim_impact());
    let explicit_impact = context
        .explicit_legacy_class
        .and_then(ClaimImpact::from_legacy_family_class);
    let claim_impact = if context.role_is_ambiguous {
        ClaimImpact::Blocking
    } else {
        family_impact
            .or(explicit_impact)
            .unwrap_or(ClaimImpact::NonBlocking)
    };
    let legacy_class = family_effect
        .as_ref()
        .map(|unknown| unknown.class)
        .or(context.explicit_legacy_class)
        .unwrap_or_else(|| default_legacy_unknown_class(context.reason));
    let required_mechanism = required_unknown_mechanism(
        context.language,
        context.reason,
        context.affected_claim,
        context.framework_role,
        context.assumptions,
    );
    let obligation = semantic_obligation(
        context.reason,
        context.affected_claim,
        context.framework_role,
        context.assumptions,
    );
    let registered_mechanism = registered_recovery_mechanism(&required_mechanism, context);
    let resolution_class = classify_resolution_class(context, registered_mechanism);
    let recovery_code = unknown_recovery_code(
        resolution_class,
        context.reason,
        obligation,
        registered_mechanism,
    );
    UnknownDisposition {
        claim_impact,
        resolution_class,
        legacy_class,
        required_mechanism,
        obligation,
        recovery_code,
    }
}

fn classify_resolution_class(
    context: UnknownPolicyContext<'_>,
    registered_mechanism: Option<RegisteredRecoveryMechanism>,
) -> ResolutionClass {
    if context.explicit_legacy_class == Some(UnknownClass::Irreducible)
        || unknown_has_irreducible_runtime_boundary(context)
        || rust_unknown_has_irreducible_execution_boundary(context)
    {
        return ResolutionClass::Irreducible;
    }
    registered_mechanism
        .map(|mechanism| mechanism.resolution_class)
        .unwrap_or(ResolutionClass::Irreducible)
}

fn unknown_has_irreducible_runtime_boundary(context: UnknownPolicyContext<'_>) -> bool {
    if context.reason == UnknownReasonCode::MonkeyPatch {
        return true;
    }
    if assumptions_mark_runtime_boundary(context.assumptions) {
        return true;
    }
    matches!(
        assumption_value(context.assumptions, "tsjs_unknown_kind").as_deref(),
        Some("dynamic_require" | "conditional_require" | "dynamic_import")
    ) || matches!(
        assumption_value(context.assumptions, "java_unknown_kind").as_deref(),
        Some("spring_proxy_semantics" | "mockito_runtime_mocks" | "reflection")
    ) || matches!(
        assumption_value(context.assumptions, "csharp_unknown_kind").as_deref(),
        Some("dynamic_member_binding" | "reflection")
    )
}

fn assumptions_mark_runtime_boundary(assumptions: &[String]) -> bool {
    matches!(
        assumption_value(assumptions, "runtime_boundary").as_deref(),
        Some(
            "data_dependent_import"
                | "eval"
                | "exec"
                | "compile"
                | "getattr"
                | "namespace_lookup"
                | "reflection"
                | "runtime_proxy"
                | "runtime_generated_mock"
                | "dynamic_binding"
        )
    )
}

fn rust_unknown_has_irreducible_execution_boundary(context: UnknownPolicyContext<'_>) -> bool {
    if context.language != "rust" {
        return false;
    }
    let rust_unknown_kind = assumption_value(context.assumptions, "rust_unknown_kind");
    if matches!(
        rust_unknown_kind.as_deref(),
        Some("proc_macro" | "proc_macro_attribute" | "build_script")
    ) {
        return true;
    }
    false
}

fn query_unknown_claim_for_mechanism(affected_claim: &str) -> &str {
    let trimmed = affected_claim.trim();
    if let Some((_, suffix)) = trimmed.rsplit_once(':') {
        if claim_specific_required_unknown_mechanism(
            query_unknown_language_for_claim(suffix),
            suffix,
            query_unknown_framework_role_for_claim(suffix),
            &[],
        )
        .is_some()
            || matches!(suffix, "runtime_equivalence")
        {
            return suffix;
        }
    }
    trimmed
}

fn query_unknown_language_for_claim(affected_claim: &str) -> &'static str {
    if affected_claim.starts_with("fastapi_")
        || affected_claim.starts_with("pytest_")
        || affected_claim.starts_with("sqlalchemy_")
        || affected_claim.starts_with("pydantic_")
        || affected_claim.starts_with("python_")
    {
        "python"
    } else if affected_claim.starts_with("tsjs_")
        || affected_claim.starts_with("next_")
        || affected_claim.starts_with("fastify_")
        || affected_claim.starts_with("prisma_")
        || affected_claim.starts_with("drizzle_")
    {
        "typescript"
    } else if affected_claim.starts_with("rust_") {
        "rust"
    } else if affected_claim.starts_with("java_") {
        "java"
    } else if affected_claim.starts_with("csharp_") {
        "csharp"
    } else if affected_claim.starts_with("cpp_") {
        "cpp"
    } else {
        "unknown"
    }
}

fn query_unknown_framework_role_for_claim(affected_claim: &str) -> &'static str {
    if affected_claim.starts_with("fastapi_") {
        "framework:fastapi"
    } else if affected_claim.starts_with("pytest_") {
        "framework:pytest"
    } else if affected_claim.starts_with("sqlalchemy_") {
        "framework:sqlalchemy"
    } else if affected_claim.starts_with("pydantic_") {
        "framework:pydantic"
    } else if affected_claim.starts_with("fastify_") {
        "framework:fastify"
    } else if affected_claim.starts_with("prisma_") {
        "framework:prisma"
    } else if affected_claim.starts_with("drizzle_") {
        "framework:drizzle"
    } else if affected_claim.starts_with("java_spring_") {
        "framework:spring"
    } else if affected_claim.starts_with("java_testng_") {
        "framework:testng"
    } else if affected_claim.starts_with("java_test_") {
        "framework:junit5"
    } else if affected_claim.starts_with("java_jpa_") {
        "framework:jpa"
    } else if affected_claim.starts_with("java_jaxrs_") {
        "framework:jaxrs"
    } else if affected_claim.starts_with("java_mockito_") {
        "framework:mockito"
    } else if affected_claim.starts_with("csharp_aspnet_") {
        "framework:aspnetcore"
    } else if affected_claim.starts_with("csharp_efcore_") {
        "framework:efcore"
    } else if affected_claim.starts_with("csharp_test") {
        "framework:xunit"
    } else if affected_claim.starts_with("cpp_test") {
        "framework:cpp_test"
    } else if affected_claim.starts_with("python_django_") {
        "framework:django"
    } else if affected_claim.starts_with("python_flask_") {
        "framework:flask"
    } else if affected_claim.starts_with("python_cli_") {
        "framework:click"
    } else if affected_claim.starts_with("python_celery_") {
        "framework:celery"
    } else if affected_claim.starts_with("python_unittest_") {
        "framework:unittest"
    } else {
        "unknown"
    }
}

fn required_unknown_mechanism(
    language: &str,
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
    assumptions: &[String],
) -> String {
    if let Some(mechanism) = claim_specific_required_unknown_mechanism(
        language,
        affected_claim,
        framework_role,
        assumptions,
    ) {
        return mechanism.to_string();
    }

    match reason {
        UnknownReasonCode::StaleEvidence => "source_refresh".to_string(),
        UnknownReasonCode::ConflictingFacts => "conflict_resolution".to_string(),
        UnknownReasonCode::InsufficientSupport => "compatible_support_evidence".to_string(),
        UnknownReasonCode::MissingProjectConfig => "project_config_reader".to_string(),
        UnknownReasonCode::MissingDependency => "resolve_dependency_metadata".to_string(),
        UnknownReasonCode::PytestFixtureInjection => "pytest_fixture_graph".to_string(),
        UnknownReasonCode::RuntimeDependencyInjection => {
            if language == "python"
                && (framework_role.starts_with("framework:fastapi")
                    || affected_claim.starts_with("fastapi_"))
            {
                "fastapi_dependency_graph".to_string()
            } else if language == "java" && affected_claim.starts_with("java_spring_") {
                "spring_di_model".to_string()
            } else {
                "dependency_injection_model".to_string()
            }
        }
        UnknownReasonCode::UnresolvedImport | UnknownReasonCode::DynamicImport => {
            if language == "python" {
                "python_import_graph".to_string()
            } else if is_tsjs_language(language) {
                "typescript_paths_resolver".to_string()
            } else if language == "rust" {
                "rust_module_graph".to_string()
            } else if language == "java" {
                "java_project_graph".to_string()
            } else if language == "csharp" {
                "csharp_project_model".to_string()
            } else if is_c_cpp_language(language) {
                "cpp_test_framework_model".to_string()
            } else {
                "import_resolution_provider".to_string()
            }
        }
        UnknownReasonCode::FrameworkMagic => {
            if language == "python" && framework_role.starts_with("framework:pytest") {
                "pytest_fixture_graph".to_string()
            } else if language == "python" && framework_role.starts_with("framework:fastapi") {
                "fastapi_dependency_graph".to_string()
            } else if is_tsjs_language(language) {
                "typescript_export_graph".to_string()
            } else if language == "rust" {
                "cargo_feature_cfg_model".to_string()
            } else if language == "java" {
                "spring_component_scan_model".to_string()
            } else {
                "framework_semantic_provider".to_string()
            }
        }
        UnknownReasonCode::MacroOrPreprocessor | UnknownReasonCode::BuildVariantAmbiguity => {
            if language == "rust" {
                if reason == UnknownReasonCode::MacroOrPreprocessor {
                    "rust_macro_boundary".to_string()
                } else {
                    "cargo_feature_cfg_model".to_string()
                }
            } else {
                "build_variant_model".to_string()
            }
        }
        UnknownReasonCode::MonkeyPatch => "runtime_trace_required".to_string(),
    }
}

/// Classify the semantic obligation a typed `UNKNOWN` poses, deterministically
/// from its already-typed reason plus the same language/claim/role/assumption
/// context used to pick the required mechanism. This is a source-free refinement
/// (a fixed enum vocabulary): it never resolves the `UNKNOWN`, changes whether it
/// blocks, or weakens any gate — it only names the kind of question. Reasons that
/// are runtime-defined (`MonkeyPatch`, dynamic execution call targets) map to
/// `RuntimeIrreducible` (ADR-0015 class c), and quality states (stale,
/// conflicting, insufficient support) map to `Governance` rather than a semantic
/// obligation.
fn semantic_obligation(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
    assumptions: &[String],
) -> SemanticObligation {
    match reason {
        UnknownReasonCode::StaleEvidence
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::InsufficientSupport => SemanticObligation::Governance,
        UnknownReasonCode::MonkeyPatch => SemanticObligation::RuntimeIrreducible,
        UnknownReasonCode::BuildVariantAmbiguity => SemanticObligation::BuildVariant,
        UnknownReasonCode::MacroOrPreprocessor => SemanticObligation::MacroExpansion,
        UnknownReasonCode::MissingDependency => SemanticObligation::ExternalDependency,
        UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::DynamicImport => SemanticObligation::SymbolBinding,
        UnknownReasonCode::PytestFixtureInjection => SemanticObligation::FrameworkIdentity,
        UnknownReasonCode::RuntimeDependencyInjection => {
            if framework_role.starts_with("framework:")
                || affected_claim.starts_with("fastapi_")
                || affected_claim.starts_with("java_spring_")
            {
                SemanticObligation::FrameworkIdentity
            } else {
                SemanticObligation::TypeIdentity
            }
        }
        UnknownReasonCode::FrameworkMagic => {
            if assumptions
                .iter()
                .any(|assumption| assumption.starts_with("rust_trait_dispatch"))
            {
                SemanticObligation::DispatchTarget
            } else if affected_claim == "python_call_target"
                && assumptions_mark_runtime_boundary(assumptions)
            {
                // Only an exact runtime-boundary assumption makes the call
                // target irreducible; static aliases remain provider questions.
                SemanticObligation::RuntimeIrreducible
            } else if affected_claim == "python_call_target" {
                SemanticObligation::DispatchTarget
            } else {
                SemanticObligation::FrameworkIdentity
            }
        }
    }
}

fn claim_specific_required_unknown_mechanism(
    language: &str,
    affected_claim: &str,
    framework_role: &str,
    assumptions: &[String],
) -> Option<&'static str> {
    if language == "python" {
        return match affected_claim {
            "python_import_resolution" => Some("python_import_graph"),
            "pytest_fixture_binding" => Some("pytest_fixture_graph"),
            "fastapi_dependency_target" | "fastapi_router_binding" | "fastapi_router_prefix" => {
                Some("fastapi_dependency_graph")
            }
            "sqlalchemy_query_shape" => Some("sqlalchemy_session_model"),
            "sqlalchemy_relationship_target" => Some("sqlalchemy_model_graph"),
            "pydantic_validator_side_effects" => Some("pydantic_validator_model"),
            "python_django_model_identity"
            | "python_django_url_identity"
            | "python_django_string_dispatch" => Some("django_project_model"),
            "python_django_settings_behavior" => Some("django_settings_model"),
            "python_flask_route_identity" => Some("flask_app_model"),
            "python_cli_command_identity"
            | "python_celery_task_identity"
            | "python_celery_runtime_routing"
            | "python_unittest_patch_target" => Some("python_import_graph"),
            _ if framework_role.starts_with("framework:pytest")
                && affected_claim.contains("fixture") =>
            {
                Some("pytest_fixture_graph")
            }
            _ if framework_role.starts_with("framework:sqlalchemy")
                && affected_claim.starts_with("sqlalchemy_") =>
            {
                Some("sqlalchemy_model_graph")
            }
            _ if framework_role.starts_with("framework:fastapi")
                && affected_claim.starts_with("fastapi_") =>
            {
                Some("fastapi_dependency_graph")
            }
            _ => None,
        };
    }

    if is_tsjs_language(language) {
        return match affected_claim {
            "tsjs_import_resolution" => tsjs_import_resolution_mechanism(assumptions),
            "tsjs_path_alias" => Some("typescript_paths_resolver"),
            "tsjs_package_entry" => Some("typescript_package_entry_model"),
            "tsjs_reexport_resolution"
            | "next_default_export"
            | "next_pages_api_export"
            | "next_route_handler_export" => Some("typescript_export_graph"),
            "fastify_receiver_binding"
            | "fastify_route_shape"
            | "fastify_route_method"
            | "fastify_route_prefix"
            | "fastify_plugin_binding"
            | "fastify_plugin_registration" => Some("fastify_receiver_model"),
            "prisma_client_binding" | "prisma_query_shape" | "prisma_transaction_shape" => {
                Some("prisma_client_model")
            }
            "drizzle_schema_table"
            | "drizzle_table_binding"
            | "drizzle_query_shape"
            | "drizzle_db_binding"
            | "drizzle_transaction_shape" => Some("drizzle_db_model"),
            "tsjs_nest_controller_identity"
            | "tsjs_nest_di_resolution"
            | "tsjs_nest_dynamic_module" => Some("nestjs_di_model"),
            "tsjs_hono_receiver" => Some("hono_receiver_model"),
            _ => None,
        };
    }

    if language == "rust" {
        return match affected_claim {
            "rust_module_resolution" | "rust_framework_attribute_binding" => {
                Some("rust_module_graph")
            }
            "rust_build_variant" => Some("cargo_feature_cfg_model"),
            "rust_macro_expansion" | "rust_derive_expansion" => Some("rust_macro_boundary"),
            "rust_trait_dispatch" => Some("rust_trait_dispatch_model"),
            "rust_axum_route_identity"
            | "rust_axum_middleware_semantics"
            | "rust_axum_extractor_semantics" => Some("axum_route_model"),
            _ => None,
        };
    }

    if language == "java" {
        return match affected_claim {
            "java_spring_route_path" => Some("java_spring_route_literal_model"),
            "java_spring_component_scan" => Some("spring_component_scan_model"),
            "java_spring_dependency_injection" => Some("spring_di_model"),
            "java_spring_proxy_semantics" => Some("spring_proxy_model"),
            "java_spring_generated_repository" | "java_spring_data_query_derivation" => {
                Some("spring_data_repository_model")
            }
            "java_test_annotation_binding"
            | "java_test_method_source"
            | "java_testng_data_provider" => Some("java_test_annotation_model"),
            "java_jpa_entity_identity" | "java_jpa_runtime_mapping" => Some("jpa_entity_model"),
            "java_jaxrs_resource_identity" | "java_jaxrs_route_path" => {
                Some("jaxrs_resource_model")
            }
            "java_generated_members" => Some("java_annotation_processor_boundary"),
            "java_mockito_runtime_mocks" => Some("java_mockito_runtime_mock_model"),
            _ => None,
        };
    }

    if language == "csharp" {
        return match affected_claim {
            "csharp_build_variant" => Some("csharp_build_variant_model"),
            "csharp_partial_external" | "csharp_generated_source" => {
                Some("csharp_source_generator_boundary")
            }
            "csharp_di_registration" | "csharp_aspnet_convention_routing" => {
                Some("csharp_di_model")
            }
            "csharp_aspnet_route_template" => Some("aspnet_route_literal_model"),
            _ => None,
        };
    }

    if is_c_cpp_language(language) {
        return match affected_claim {
            "cpp_build_variant" => Some("cpp_build_variant_model"),
            "cpp_macro_boundary" | "cpp_generated_code" => Some("cpp_macro_boundary"),
            "cpp_test_framework_identity" => Some("cpp_test_framework_model"),
            "cpp_project_config" => Some("cpp_compile_commands_model"),
            _ => None,
        };
    }

    None
}

fn tsjs_import_resolution_mechanism(assumptions: &[String]) -> Option<&'static str> {
    match assumption_value(assumptions, "tsjs_unknown_kind").as_deref() {
        Some("unresolved_path_alias" | "path_alias_conflict") => Some("typescript_paths_resolver"),
        Some("unresolved_root_dirs" | "root_dirs_conflict") => Some("typescript_rootdirs_model"),
        Some("dynamic_require" | "conditional_require") => Some("typescript_commonjs_alias_model"),
        Some("missing_dependency" | "missing_package_entry" | "unsafe_package_entry") => {
            Some("typescript_package_entry_model")
        }
        Some("unresolved_export" | "unresolved_reexport" | "ambiguous_reexport") => {
            Some("typescript_export_graph")
        }
        Some("unresolved_import" | "ambiguous_import" | "dynamic_import") | None => {
            Some("typescript_module_resolver")
        }
        Some(_) => Some("typescript_module_resolver"),
    }
}

fn registered_recovery_mechanism(
    mechanism: &str,
    context: UnknownPolicyContext<'_>,
) -> Option<RegisteredRecoveryMechanism> {
    let recoverable = |recovery_code| RegisteredRecoveryMechanism {
        resolution_class: ResolutionClass::Recoverable,
        recovery_code,
    };
    let irreducible = |recovery_code| RegisteredRecoveryMechanism {
        resolution_class: ResolutionClass::Irreducible,
        recovery_code,
    };
    match mechanism {
        "source_refresh" => Some(recoverable("run_sync")),
        "project_config_reader" => Some(recoverable("add_project_config")),
        "resolve_dependency_metadata" => Some(recoverable("resolve_dependency_metadata")),
        "pytest_fixture_graph" => Some(recoverable("resolve_fixture_graph")),
        "python_import_graph"
        | "typescript_paths_resolver"
        | "typescript_rootdirs_model"
        | "typescript_module_resolver"
        | "typescript_export_graph"
        | "typescript_package_entry_model"
        | "typescript_commonjs_alias_model"
        | "rust_module_graph"
        | "java_project_graph"
        | "csharp_project_model" => Some(recoverable("resolve_import_graph")),
        "fastapi_dependency_graph"
        | "sqlalchemy_session_model"
        | "sqlalchemy_model_graph"
        | "pydantic_validator_model"
        | "django_project_model"
        | "django_settings_model"
        | "flask_app_model"
        | "fastify_receiver_model"
        | "prisma_client_model"
        | "drizzle_db_model"
        | "nestjs_di_model"
        | "hono_receiver_model"
        | "rust_trait_dispatch_model"
        | "axum_route_model"
        | "java_spring_route_literal_model"
        | "spring_component_scan_model"
        | "spring_di_model"
        | "spring_data_repository_model"
        | "java_test_annotation_model"
        | "jpa_entity_model"
        | "jaxrs_resource_model"
        | "csharp_di_model"
        | "aspnet_route_literal_model"
        | "cpp_test_framework_model"
        | "cpp_compile_commands_model" => Some(recoverable("enable_provider")),
        "rust_macro_boundary" | "cpp_macro_boundary" => Some(recoverable("manual_review_required")),
        "framework_semantic_provider"
            if context.language == "python"
                && matches!(
                    context.affected_claim,
                    "python_call_target" | "python_framework_identity"
                ) =>
        {
            Some(recoverable("manual_review_required"))
        }
        "dependency_injection_model" => Some(recoverable("enable_provider")),
        "cargo_feature_cfg_model"
        | "csharp_build_variant_model"
        | "cpp_build_variant_model"
        | "build_variant_model"
        | "conflict_resolution"
        | "compatible_support_evidence" => Some(recoverable("manual_review_required")),
        "runtime_trace_required" => Some(irreducible("runtime_trace_required")),
        "java_annotation_processor_boundary"
        | "java_mockito_runtime_mock_model"
        | "csharp_source_generator_boundary"
        | "spring_proxy_model" => Some(irreducible("manual_review_required")),
        _ => None,
    }
}

fn unknown_recovery_code(
    resolution_class: ResolutionClass,
    reason: UnknownReasonCode,
    obligation: SemanticObligation,
    registered_mechanism: Option<RegisteredRecoveryMechanism>,
) -> &'static str {
    if resolution_class == ResolutionClass::Irreducible {
        if reason == UnknownReasonCode::DynamicImport
            || reason == UnknownReasonCode::MonkeyPatch
            || obligation == SemanticObligation::RuntimeIrreducible
        {
            return "runtime_trace_required";
        }
        return registered_mechanism
            .filter(|mechanism| mechanism.resolution_class == ResolutionClass::Irreducible)
            .map(|mechanism| mechanism.recovery_code)
            .unwrap_or("manual_review_required");
    }
    registered_mechanism
        .filter(|mechanism| mechanism.resolution_class == ResolutionClass::Recoverable)
        .map(|mechanism| mechanism.recovery_code)
        .unwrap_or("manual_review_required")
}

fn count_unknown_class(entries: &[UnknownInventoryEntry], class: UnknownClass) -> usize {
    entries.iter().filter(|entry| entry.class == class).count()
}

fn aggregate_unknown_bucket<F>(
    entries: &[UnknownInventoryEntry],
    key_fn: F,
) -> Vec<UnknownInventoryBucket>
where
    F: Fn(&UnknownInventoryEntry) -> &str,
{
    let mut counts = BTreeMap::<String, usize>::new();
    for entry in entries {
        *counts.entry(key_fn(entry).to_string()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(key, count)| UnknownInventoryBucket { key, count })
        .collect()
}

fn aggregate_blocks_support(
    entries: &[UnknownInventoryEntry],
) -> Vec<UnknownInventoryBlocksSupportBucket> {
    let mut counts = BTreeMap::<bool, usize>::new();
    for entry in entries {
        *counts
            .entry(entry.claim_impact == ClaimImpact::Blocking)
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(
            |(blocks_support, count)| UnknownInventoryBlocksSupportBucket {
                blocks_support,
                count,
            },
        )
        .collect()
}

fn is_tsjs_language(language: &str) -> bool {
    matches!(
        language,
        "typescript" | "typescript-react" | "tsx" | "javascript" | "javascript-react" | "jsx"
    )
}

/// The `c`, `cpp`, and `cpp-config` raw language tokens share the bounded
/// `c/cpp` preview readiness/inventory scope.
fn is_c_cpp_language(language: &str) -> bool {
    matches!(language, "c" | "cpp" | "cpp-config")
}

#[derive(Debug, Clone, Copy)]
struct ReadinessLanguageScope {
    language: &'static str,
    language_scope: &'static str,
    preview_status: &'static str,
}

fn readiness_language_scopes() -> &'static [ReadinessLanguageScope] {
    &[
        ReadinessLanguageScope {
            language: "python",
            language_scope: "official_v0_1",
            preview_status: "official",
        },
        ReadinessLanguageScope {
            language: "typescript/javascript",
            language_scope: "bounded_v0_2_preview",
            preview_status: "bounded_preview",
        },
        ReadinessLanguageScope {
            language: "rust",
            language_scope: "bounded_v0_2_preview",
            preview_status: "bounded_preview",
        },
        ReadinessLanguageScope {
            language: "java",
            language_scope: "bounded_v0_2_preview",
            preview_status: "bounded_preview",
        },
        ReadinessLanguageScope {
            language: "csharp",
            language_scope: "bounded_v0_2_preview",
            preview_status: "bounded_preview",
        },
        ReadinessLanguageScope {
            language: "c/cpp",
            language_scope: "bounded_v0_2_preview",
            preview_status: "bounded_preview",
        },
    ]
}

fn inventory_language_scope(language: &str) -> &'static str {
    if is_tsjs_language(language) {
        "typescript/javascript"
    } else if is_c_cpp_language(language) {
        "c/cpp"
    } else {
        match language {
            "python" => "python",
            "rust" => "rust",
            "java" => "java",
            "csharp" => "csharp",
            _ => "unknown",
        }
    }
}

pub fn repo_shape_diagnostics(
    index_store: &impl IndexStore,
    _family_store: &impl FamilyStore,
) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
    let stats = index_store
        .active_repo_shape_stats()
        .map_err(index_store_error)?;
    Ok(repo_shape_diagnostics_from_stats(stats))
}

fn repo_shape_diagnostics_from_stats(stats: ActiveRepoShapeStats) -> RepoShapeDiagnosticsReport {
    let eligible_code_units = stats.eligible_code_units;
    let family_count = stats.family_count;
    let family_member_count = stats.family_member_count;
    let covered_code_units = stats.covered_code_units;
    let local_pattern_density = ratio(family_member_count, eligible_code_units);
    let family_support_coverage = ratio(covered_code_units, eligible_code_units);
    let abstention_rate = family_support_coverage.map(|coverage| (1.0 - coverage).max(0.0));
    let token_saving_risk = risk_from_density(local_pattern_density);
    let thin_wrapper_risk = risk_from_density(family_support_coverage);
    let blocking_reasons = token_saving_blocking_reasons(
        eligible_code_units,
        family_count,
        local_pattern_density,
        family_support_coverage,
    );
    let token_saving_readiness = token_saving_readiness(
        eligible_code_units,
        family_count,
        local_pattern_density,
        family_support_coverage,
    );
    RepoShapeDiagnosticsReport {
        active_generation: stats.generation_id,
        indexed_file_count: stats.indexed_file_count,
        indexed_code_unit_count: stats.indexed_code_unit_count,
        semantic_fact_count: stats.semantic_fact_count,
        eligible_code_units,
        family_count,
        family_member_count,
        covered_code_units,
        by_language: readiness_language_diagnostics_from_stats(&stats.by_language),
        local_pattern_density,
        family_support_coverage,
        abstention_rate,
        external_dependency_signal: DiagnosticSignal::Unknown,
        thin_wrapper_risk,
        token_saving_risk,
        token_saving_readiness,
        blocking_reasons,
        interpretation:
            "RepoGrammar can provide integration-pattern context when repeated local patterns exist; third-party-heavy or thin-wrapper repositories may see lower token-saving potential.",
    }
}

pub fn list_families(store: &impl FamilyStore) -> Result<FamilyListReport, RepoGrammarError> {
    let active = store
        .list_active_family_summaries()
        .map_err(family_store_error)?;
    let mut families = active
        .families
        .into_iter()
        .map(|family| FamilySummary {
            family_id: family.family_id,
            classification: family.classification,
            support: family.support,
            prevalence: family.prevalence,
            // The freshness-free variant does not read source; leave the
            // per-family verdict unevaluated rather than asserting freshness.
            freshness: None,
        })
        .collect::<Vec<_>>();
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    let unknowns = if families.is_empty() {
        vec![insufficient_support_unknown("repository pattern families")]
    } else {
        Vec::new()
    };
    Ok(FamilyListReport {
        active_generation: active.generation_id,
        families,
        freshness_counts: None,
        unknowns,
    })
}

/// Deterministic, generic claim label for the report-level stale signal. Kept
/// low-cardinality: one stale-evidence unknown covers the whole listing rather
/// than one per stale family.
const FAMILY_LIST_FRESHNESS_CLAIM: &str = "repository pattern families:evidence_freshness";

pub fn list_families_with_freshness(
    request: FamilyEvidenceFreshnessRequest,
    store: &impl FamilyStore,
    source_store: &impl SourceStore,
) -> Result<FamilyListReport, RepoGrammarError> {
    let base = list_families(store)?;
    // Empty listings already carry the insufficient-support unknown and render
    // through the existing empty path; there is nothing to verify.
    if base.families.is_empty() {
        return Ok(base);
    }

    // One bounded projection read of the active generation's family evidence.
    let projection = store
        .list_active_family_evidence_projection()
        .map_err(family_store_error)?;
    // The summaries and the evidence projection are two separate reads. If the
    // active generation switched between them, the projected evidence could
    // describe a different generation than the listed families.
    if projection.generation_id != base.active_generation {
        return Err(RepoGrammarError::InvalidInput(
            "active inventory generation changed during family freshness read".to_string(),
        ));
    }

    // Group evidence by family, preserving projection order per family.
    let mut evidence_by_family: BTreeMap<String, Vec<IndexedFamilyEvidenceProjectionRecord>> =
        BTreeMap::new();
    for row in projection.rows {
        evidence_by_family
            .entry(row.family_id.clone())
            .or_default()
            .push(row);
    }

    // Verify each distinct (path, expected hash) at most once. Within a single
    // generation every evidence row for a path carries that path's indexed hash,
    // so the distinct pairs equal the distinct evidence paths: the number of
    // source reads is bounded by the distinct paths, never the sum over families.
    let mut path_verdicts: BTreeMap<(String, String), EvidencePathVerdict> = BTreeMap::new();
    for rows in evidence_by_family.values() {
        for row in rows {
            let key = (row.path.clone(), row.content_hash.as_str().to_string());
            if path_verdicts.contains_key(&key) {
                continue;
            }
            let verdict =
                verify_evidence_path(&request, source_store, &row.path, &row.content_hash)?;
            path_verdicts.insert(key, verdict);
        }
    }

    let mut counts = FamilyFreshnessCounts::default();
    let families = base
        .families
        .into_iter()
        .map(|summary| {
            let freshness =
                derive_family_freshness(evidence_by_family.get(&summary.family_id), &path_verdicts);
            match freshness {
                FamilyFreshness::Fresh => counts.fresh_count += 1,
                FamilyFreshness::Stale => counts.stale_count += 1,
                FamilyFreshness::CannotVerify => counts.cannot_verify_count += 1,
            }
            FamilySummary {
                freshness: Some(freshness),
                ..summary
            }
        })
        .collect::<Vec<_>>();

    let mut unknowns = base.unknowns;
    if counts.stale_count > 0 {
        // Non-fatal, low-cardinality signal: the listing stays served, but a
        // typed stale-evidence unknown with a resync recovery marks that at
        // least one family's evidence no longer reflects the working tree.
        unknowns.push(stale_evidence_unknown(FAMILY_LIST_FRESHNESS_CLAIM));
    }

    Ok(FamilyListReport {
        active_generation: base.active_generation,
        families,
        freshness_counts: Some(counts),
        unknowns,
    })
}

/// Derives a family's freshness from the shared per-path verdicts. Stale takes
/// precedence over cannot-verify, which takes precedence over fresh. A family
/// with no evidence rows abstains as `CannotVerify`, matching the single-family
/// evidence-less abstention in `family_evidence_is_fresh`.
fn derive_family_freshness(
    evidence: Option<&Vec<IndexedFamilyEvidenceProjectionRecord>>,
    path_verdicts: &BTreeMap<(String, String), EvidencePathVerdict>,
) -> FamilyFreshness {
    let Some(evidence) = evidence.filter(|rows| !rows.is_empty()) else {
        return FamilyFreshness::CannotVerify;
    };
    let mut cannot_verify = false;
    for row in evidence {
        let key = (row.path.clone(), row.content_hash.as_str().to_string());
        match path_verdicts.get(&key) {
            Some(EvidencePathVerdict::Stale) => return FamilyFreshness::Stale,
            Some(EvidencePathVerdict::CannotVerify) | None => cannot_verify = true,
            Some(EvidencePathVerdict::Fresh) => {}
        }
    }
    if cannot_verify {
        FamilyFreshness::CannotVerify
    } else {
        FamilyFreshness::Fresh
    }
}

pub fn lookup_family(
    store: &impl FamilyStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        let active = store
            .list_active_family_summaries()
            .map_err(family_store_error)?;
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            candidate_family_ids: Vec::new(),
            unknowns: vec![insufficient_support_unknown("query target")],
        }));
    };
    let FamilyMatchSet {
        active_generation,
        matches,
        unknown,
        candidate_family_ids,
    } = bounded_family_matches(store, target, mode)?;
    if let Some(unknown) = unknown {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            candidate_family_ids,
            unknowns: vec![unknown],
        }));
    }
    if let Some(unknown) = ambiguous_target_unknown(&matches) {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            candidate_family_ids: candidate_family_ids_from_matches(&matches),
            unknowns: vec![unknown],
        }));
    }
    if let Some(matched) = matches.into_iter().next() {
        return Ok(FamilyLookupReport::Found(family_detail(matched.family)));
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation,
        candidate_family_ids,
        unknowns: vec![insufficient_support_unknown("query target")],
    }))
}

struct FamilyMatchSet {
    active_generation: String,
    matches: Vec<FamilyTargetMatch>,
    unknown: Option<FamilyQueryUnknown>,
    candidate_family_ids: Vec<String>,
}

fn bounded_family_matches(
    store: &impl FamilyStore,
    target: &str,
    mode: FamilyLookupMode,
) -> Result<FamilyMatchSet, RepoGrammarError> {
    match mode {
        FamilyLookupMode::ExactFamilyId => exact_family_match_set(store, target),
        FamilyLookupMode::ExactMemberId => exact_member_match_set(store, target),
        FamilyLookupMode::FuzzyQuery => fuzzy_family_match_set(store, target),
    }
}

fn exact_family_match_set(
    store: &impl FamilyStore,
    target: &str,
) -> Result<FamilyMatchSet, RepoGrammarError> {
    if let Some(active_family) = store.show_family(target).map_err(family_store_error)? {
        let active_generation = active_family.generation_id.clone();
        return Ok(FamilyMatchSet {
            active_generation,
            matches: vec![FamilyTargetMatch {
                family: active_family,
            }],
            unknown: None,
            candidate_family_ids: vec![target.to_string()],
        });
    }
    Ok(FamilyMatchSet {
        active_generation: active_generation_from_summaries(store)?,
        matches: Vec::new(),
        unknown: None,
        candidate_family_ids: Vec::new(),
    })
}

fn exact_member_match_set(
    store: &impl FamilyStore,
    target: &str,
) -> Result<FamilyMatchSet, RepoGrammarError> {
    let candidates = store
        .find_active_families_by_member(target)
        .map_err(family_store_error)?;
    let active_generation = candidates.generation_id.clone();
    let candidate_family_ids = candidate_family_ids(&candidates.candidates);
    if candidates.candidates.len() > 1 {
        return Ok(FamilyMatchSet {
            active_generation,
            matches: Vec::new(),
            unknown: Some(candidate_ambiguity_unknown(
                "query target ambiguity",
                &candidate_family_ids,
            )),
            candidate_family_ids,
        });
    }
    let matches = hydrate_candidate_matches(
        store,
        target,
        FamilyLookupMode::ExactMemberId,
        candidates.candidates,
    )?;
    Ok(FamilyMatchSet {
        active_generation,
        matches,
        unknown: None,
        candidate_family_ids,
    })
}

fn fuzzy_family_match_set(
    store: &impl FamilyStore,
    target: &str,
) -> Result<FamilyMatchSet, RepoGrammarError> {
    if let Some(active_family) = store.show_family(target).map_err(family_store_error)? {
        let active_generation = active_family.generation_id.clone();
        return Ok(FamilyMatchSet {
            active_generation,
            matches: vec![FamilyTargetMatch {
                family: active_family,
            }],
            unknown: None,
            candidate_family_ids: vec![target.to_string()],
        });
    }

    if target.starts_with("unit:") {
        let candidates = store
            .find_active_families_by_member(target)
            .map_err(family_store_error)?;
        let active_generation = candidates.generation_id.clone();
        let candidate_family_ids = candidate_family_ids(&candidates.candidates);
        if candidates.candidates.len() > 1 {
            return Ok(FamilyMatchSet {
                active_generation,
                matches: Vec::new(),
                unknown: Some(candidate_ambiguity_unknown(
                    "query target ambiguity",
                    &candidate_family_ids,
                )),
                candidate_family_ids,
            });
        }
        let matches = hydrate_candidate_matches(
            store,
            target,
            FamilyLookupMode::FuzzyQuery,
            candidates.candidates,
        )?;
        return Ok(FamilyMatchSet {
            active_generation,
            matches,
            unknown: None,
            candidate_family_ids,
        });
    }

    let mut active_generation = None;
    let mut candidate_ids = Vec::new();
    let mut truncated = false;

    let role_candidates = store
        .find_active_families_by_role(target, FUZZY_FAMILY_CANDIDATE_LIMIT)
        .map_err(family_store_error)?;
    collect_family_candidates(
        &mut active_generation,
        &mut candidate_ids,
        &mut truncated,
        role_candidates,
    );

    for path in family_candidate_path_tokens(target) {
        let path_candidates = store
            .find_active_families_by_evidence_path(&path, FUZZY_FAMILY_CANDIDATE_LIMIT)
            .map_err(family_store_error)?;
        collect_family_candidates(
            &mut active_generation,
            &mut candidate_ids,
            &mut truncated,
            path_candidates,
        );
        if truncated || candidate_ids.len() > FUZZY_FAMILY_CANDIDATE_LIMIT {
            break;
        }
    }

    let active_generation = match active_generation {
        Some(active_generation) => active_generation,
        None => active_generation_from_summaries(store)?,
    };
    if truncated || candidate_ids.len() > FUZZY_FAMILY_CANDIDATE_LIMIT {
        return Ok(FamilyMatchSet {
            active_generation,
            matches: Vec::new(),
            unknown: Some(candidate_ambiguity_unknown(
                "query target candidate set",
                &candidate_ids,
            )),
            candidate_family_ids: normalized_family_ids(candidate_ids),
        });
    }
    let candidate_family_ids = normalized_family_ids(candidate_ids);
    let matches = hydrate_candidate_matches_by_id(
        store,
        target,
        FamilyLookupMode::FuzzyQuery,
        candidate_family_ids.clone(),
    )?;
    Ok(FamilyMatchSet {
        active_generation,
        matches,
        unknown: None,
        candidate_family_ids,
    })
}

fn active_generation_from_summaries(store: &impl FamilyStore) -> Result<String, RepoGrammarError> {
    store
        .list_active_family_summaries()
        .map(|active| active.generation_id)
        .map_err(family_store_error)
}

fn candidate_family_ids(candidates: &[IndexedFamilyCandidateRecord]) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| candidate.family_id.clone())
        .collect()
}

fn candidate_family_ids_from_matches(matches: &[FamilyTargetMatch]) -> Vec<String> {
    normalized_family_ids(
        matches
            .iter()
            .map(|matched| matched.family.family.family_id.clone())
            .collect(),
    )
}

fn collect_family_candidates(
    active_generation: &mut Option<String>,
    candidate_ids: &mut Vec<String>,
    truncated: &mut bool,
    candidates: ActiveFamilyCandidates,
) {
    if active_generation.is_none() {
        *active_generation = Some(candidates.generation_id);
    }
    *truncated |= candidates.truncated;
    let mut seen = candidate_ids.iter().cloned().collect::<BTreeSet<_>>();
    for candidate in candidates.candidates {
        if seen.insert(candidate.family_id.clone()) {
            candidate_ids.push(candidate.family_id);
        }
    }
}

fn family_candidate_path_tokens(target: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    target_path_tokens(target)
        .into_iter()
        .filter_map(|token| {
            let (path, _) = split_query_path_locator(token);
            (is_family_candidate_path_text(path) && seen.insert(path.to_string()))
                .then(|| path.to_string())
        })
        .collect()
}

fn is_family_candidate_path_text(path: &str) -> bool {
    is_safe_query_path_text(path) && !path.contains(':')
}

fn hydrate_candidate_matches(
    store: &impl FamilyStore,
    target: &str,
    mode: FamilyLookupMode,
    candidates: Vec<IndexedFamilyCandidateRecord>,
) -> Result<Vec<FamilyTargetMatch>, RepoGrammarError> {
    hydrate_candidate_matches_by_id(store, target, mode, candidate_family_ids(&candidates))
}

fn hydrate_candidate_matches_by_id(
    store: &impl FamilyStore,
    target: &str,
    mode: FamilyLookupMode,
    candidate_ids: Vec<String>,
) -> Result<Vec<FamilyTargetMatch>, RepoGrammarError> {
    let mut matches = Vec::new();
    for family_id in candidate_ids {
        let Some(active_family) = store.show_family(&family_id).map_err(family_store_error)? else {
            continue;
        };
        if family_target_match(&active_family, target, mode).is_none() {
            continue;
        }
        matches.push(FamilyTargetMatch {
            family: active_family,
        });
    }
    Ok(matches)
}

fn candidate_ambiguity_unknown(
    affected_claim: impl Into<String>,
    candidate_family_ids: &[String],
) -> FamilyQueryUnknown {
    let mut candidates = candidate_family_ids.to_vec();
    candidates.sort();
    candidates.dedup();
    let candidate_list = if candidates.is_empty() {
        "none".to_string()
    } else {
        candidates.join(", ")
    };
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim: affected_claim.into(),
        recovery: Some(format!(
            "narrow the target to one exact family id or member id, or one repo-relative path; bounded candidate families: {candidate_list}"
        )),
    }
}

pub fn lookup_family_with_local_context(
    index_store: &impl IndexStore,
    family_store: &impl FamilyStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let report = lookup_family(family_store, target, mode)?;
    add_local_context_fallback(index_store, report, target, mode)
}

pub fn lookup_family_with_freshness(
    request: FamilyEvidenceFreshnessRequest,
    store: &impl FamilyStore,
    source_store: &impl SourceStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        let active = store
            .list_active_family_summaries()
            .map_err(family_store_error)?;
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            candidate_family_ids: Vec::new(),
            unknowns: vec![insufficient_support_unknown("query target")],
        }));
    };
    let FamilyMatchSet {
        active_generation,
        matches,
        unknown,
        candidate_family_ids,
    } = bounded_family_matches(store, target, mode)?;
    if let Some(unknown) = unknown {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            candidate_family_ids,
            unknowns: vec![unknown],
        }));
    }
    if let Some(unknown) = ambiguous_target_unknown(&matches) {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            candidate_family_ids: candidate_family_ids_from_matches(&matches),
            unknowns: vec![unknown],
        }));
    }
    if let Some(matched) = matches.into_iter().next() {
        if family_evidence_is_fresh(&request, source_store, &matched.family)? {
            return Ok(FamilyLookupReport::Found(family_detail(matched.family)));
        }
        let family_id = matched.family.family.family_id;
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            candidate_family_ids: vec![family_id.clone()],
            unknowns: vec![stale_evidence_unknown(format!(
                "{}:evidence_freshness",
                family_id
            ))],
        }));
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation,
        candidate_family_ids,
        unknowns: vec![insufficient_support_unknown("query target")],
    }))
}

pub fn lookup_family_with_freshness_and_local_context(
    request: FamilyEvidenceFreshnessRequest,
    index_store: &impl IndexStore,
    family_store: &impl FamilyStore,
    source_store: &impl SourceStore,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    let report = lookup_family_with_freshness(request, family_store, source_store, target, mode)?;
    add_local_context_fallback(index_store, report, target, mode)
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
        line_range_omissions: Vec::new(),
    }
}

fn add_local_context_fallback(
    index_store: &impl IndexStore,
    report: FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
) -> Result<FamilyLookupReport, RepoGrammarError> {
    if mode != FamilyLookupMode::FuzzyQuery {
        return Ok(report);
    }
    let FamilyLookupReport::Unknown(unknown_report) = report else {
        return Ok(report);
    };
    if !family_evidence_insufficient_for_local_context(&unknown_report) {
        return Ok(FamilyLookupReport::Unknown(unknown_report));
    }
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(FamilyLookupReport::Unknown(unknown_report));
    };
    let files = index_store
        .list_active_indexed_files()
        .map_err(index_store_error)?;
    let units = index_store
        .list_active_code_units()
        .map_err(index_store_error)?;
    if files.generation_id != unknown_report.active_generation
        || units.generation_id != unknown_report.active_generation
    {
        let recovery = classify_query_evidence_recovery(
            RecoveryFreshness::Stale,
            RecoveryEvidenceState::Unknown,
        );
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: unknown_report.active_generation,
            candidate_family_ids: unknown_report.candidate_family_ids.clone(),
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::Blocking,
                reason: UnknownReasonCode::StaleEvidence,
                affected_claim: "query target resolution".to_string(),
                recovery: Some(recovery_guidance(recovery.action).to_string()),
            }],
        }));
    }
    match resolve_local_context(target, &files.files, &units.units)? {
        LocalContextResolution::Resolved(report) => {
            let recovery = classify_query_evidence_recovery(
                RecoveryFreshness::Fresh,
                RecoveryEvidenceState::Unavailable,
            );
            let mut resolved_target = report.resolved_target;
            if resolved_target.candidate_family_ids.is_empty() {
                resolved_target.candidate_family_ids = unknown_report.candidate_family_ids.clone();
            }
            Ok(FamilyLookupReport::PartialContext(Box::new(
                FamilyPartialContextReport {
                    active_generation: files.generation_id,
                    resolved_target,
                    read_plan: report.read_plan,
                    unknowns: vec![FamilyQueryUnknown {
                        class: UnknownClass::Blocking,
                        reason: UnknownReasonCode::InsufficientSupport,
                        affected_claim: "pattern family evidence for resolved target".to_string(),
                        recovery: Some(format!(
                            "{}; treat this PARTIAL_CONTEXT read plan as source-reading context only",
                            recovery_guidance(recovery.action)
                        )),
                    }],
                },
            )))
        }
        LocalContextResolution::Ambiguous(unknown) => {
            Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
                active_generation: files.generation_id,
                candidate_family_ids: unknown_report.candidate_family_ids.clone(),
                unknowns: vec![unknown],
            }))
        }
        LocalContextResolution::Unresolved => Ok(FamilyLookupReport::Unknown(unknown_report)),
    }
}

/// A fuzzy family lookup can block with a single `InsufficientSupport` unknown
/// for three distinct reasons, all of which mean "no compatible pattern-family
/// evidence could be attached to this target": the target matched no family
/// (`query target`), the candidate set was too broad or truncated to trust
/// (`query target candidate set`), or the target mapped to several competing
/// families (`query target ambiguity`). In every one of those cases the family
/// evidence is missing or ambiguous, so — without ever guessing a family — the
/// caller may still fall back to a bounded local read plan when the target
/// resolves to exactly one repository file or code unit. Exact-family and
/// exact-member lookups never reach this path (guarded by the `FuzzyQuery` mode
/// check in `add_local_context_fallback`), and stale-evidence blocks keep their
/// own reason code so they are deliberately excluded here.
fn family_evidence_insufficient_for_local_context(report: &FamilyUnknownReport) -> bool {
    matches!(
        report.unknowns.as_slice(),
        [FamilyQueryUnknown {
            class: UnknownClass::Blocking,
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim,
            ..
        }] if matches!(
            affected_claim.as_str(),
            "query target" | "query target candidate set" | "query target ambiguity"
        )
    )
}

struct LocalContextReport {
    resolved_target: ResolvedQueryTarget,
    read_plan: ReadPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetLocator {
    line: Option<usize>,
    byte_range: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetPathMatch {
    match_kind: &'static str,
    line: Option<usize>,
    byte_range: Option<(usize, usize)>,
    rank: usize,
}

#[derive(Debug)]
struct LocalPathCandidate<'a> {
    file: &'a IndexedFileRecord,
    path_match: TargetPathMatch,
}

enum LocalContextResolution {
    Resolved(Box<LocalContextReport>),
    Ambiguous(FamilyQueryUnknown),
    Unresolved,
}

fn resolve_local_context(
    target: &str,
    files: &[IndexedFileRecord],
    units: &[IndexedCodeUnitRecord],
) -> Result<LocalContextResolution, RepoGrammarError> {
    let mut path_candidates = files
        .iter()
        .filter_map(|file| {
            target_path_match(target, &file.path)
                .map(|path_match| LocalPathCandidate { file, path_match })
        })
        .collect::<Vec<_>>();
    path_candidates.sort_by(|left, right| {
        (
            left.file.path.as_str(),
            left.path_match.rank,
            left.path_match.match_kind,
        )
            .cmp(&(
                right.file.path.as_str(),
                right.path_match.rank,
                right.path_match.match_kind,
            ))
    });
    path_candidates.dedup_by(|left, right| left.file.path == right.file.path);

    if path_candidates.is_empty() {
        path_candidates = files
            .iter()
            .filter_map(|file| {
                units
                    .iter()
                    .any(|unit| {
                        unit.path == file.path && target_contains_code_unit_id(target, &unit.id)
                    })
                    .then_some(LocalPathCandidate {
                        file,
                        path_match: TargetPathMatch {
                            match_kind: "code_unit_id",
                            line: None,
                            byte_range: None,
                            rank: 0,
                        },
                    })
            })
            .collect::<Vec<_>>();
        path_candidates.sort_by(|left, right| left.file.path.cmp(&right.file.path));
        path_candidates.dedup_by(|left, right| left.file.path == right.file.path);
    }

    if path_candidates.is_empty() {
        return Ok(LocalContextResolution::Unresolved);
    }
    if path_candidates.len() > 1 {
        return Ok(LocalContextResolution::Ambiguous(
            local_context_ambiguity_unknown(
                "query target path ambiguity",
                path_candidates
                    .iter()
                    .map(|candidate| candidate.file.path.as_str())
                    .collect::<Vec<_>>(),
            ),
        ));
    }

    let path_candidate = &path_candidates[0];
    let file = path_candidate.file;
    let mut units_for_path = units
        .iter()
        .filter(|unit| unit.path == file.path)
        .collect::<Vec<_>>();
    units_for_path.sort_by(|left, right| {
        (
            left.start_byte,
            left.end_byte,
            left.id.as_str(),
            left.kind.as_str(),
        )
            .cmp(&(
                right.start_byte,
                right.end_byte,
                right.id.as_str(),
                right.kind.as_str(),
            ))
    });

    let target_terms = target_identifier_tokens(target)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unit_hint_terms = target_terms
        .iter()
        .filter(|term| !file.path.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let mut matching_units = units_for_path
        .iter()
        .copied()
        .filter(|unit| target_mentions_unit(target, unit, &unit_hint_terms))
        .collect::<Vec<_>>();
    if matching_units.is_empty() {
        if let Some((start, end)) = path_candidate.path_match.byte_range {
            matching_units = units_for_path
                .iter()
                .copied()
                .filter(|unit| ranges_overlap(start, end, unit.start_byte, unit.end_byte))
                .collect::<Vec<_>>();
        }
    }
    if matching_units.len() > 1 {
        return Ok(LocalContextResolution::Ambiguous(
            local_context_ambiguity_unknown(
                "query target code unit ambiguity",
                matching_units
                    .iter()
                    .map(|unit| unit.id.as_str())
                    .collect::<Vec<_>>(),
            ),
        ));
    }

    let unit = if matching_units.len() == 1 {
        Some(matching_units[0])
    } else if units_for_path.len() == 1 {
        Some(units_for_path[0])
    } else {
        None
    };
    let symbol_hints = unit
        .map(|unit| symbol_hints_for_unit(&unit_hint_terms, unit))
        .unwrap_or_default();
    let residue_terms = residue_terms(&target_terms, &symbol_hints, &file.path);
    let candidate_code_unit_ids = unit.map(|unit| vec![unit.id.clone()]).unwrap_or_default();
    let candidate_paths = vec![file.path.clone()];
    let confidence = local_context_confidence(path_candidate.path_match.match_kind, unit.is_some());

    let (read_plan, resolved_target) = match unit {
        Some(unit) => (
            local_context_read_plan_for_unit(unit),
            ResolvedQueryTarget {
                original_target: target.to_string(),
                kind: "code_unit",
                path: unit.path.clone(),
                line: path_candidate.path_match.line,
                byte_range: path_candidate.path_match.byte_range,
                family_id: None,
                code_unit_id: Some(unit.id.clone()),
                symbol_hints,
                residue_terms,
                candidate_paths,
                candidate_family_ids: Vec::new(),
                candidate_code_unit_ids,
                confidence,
                match_kind: if target_contains_code_unit_id(target, &unit.id) {
                    "code_unit_id"
                } else {
                    path_candidate.path_match.match_kind
                },
            },
        ),
        None => (
            local_context_read_plan_for_file(file)?,
            ResolvedQueryTarget {
                original_target: target.to_string(),
                kind: "path",
                path: file.path.clone(),
                line: path_candidate.path_match.line,
                byte_range: path_candidate.path_match.byte_range,
                family_id: None,
                code_unit_id: None,
                symbol_hints,
                residue_terms,
                candidate_paths,
                candidate_family_ids: Vec::new(),
                candidate_code_unit_ids,
                confidence,
                match_kind: path_candidate.path_match.match_kind,
            },
        ),
    };

    Ok(LocalContextResolution::Resolved(Box::new(
        LocalContextReport {
            resolved_target,
            read_plan,
        },
    )))
}

fn target_contains_code_unit_id(target: &str, code_unit_id: &str) -> bool {
    if target == code_unit_id {
        return true;
    }
    target.contains(code_unit_id)
}

fn target_mentions_unit(
    target: &str,
    unit: &IndexedCodeUnitRecord,
    target_terms: &[String],
) -> bool {
    if target_contains_code_unit_id(target, &unit.id) {
        return true;
    }
    target_terms
        .iter()
        .any(|token| token.len() >= 4 && unit.id.contains(token))
}

fn symbol_hints_for_unit(target_terms: &[String], unit: &IndexedCodeUnitRecord) -> Vec<String> {
    target_terms
        .iter()
        .filter(|term| term.len() >= 4 && unit.id.contains(term.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn residue_terms(target_terms: &[String], symbol_hints: &[String], path: &str) -> Vec<String> {
    let symbol_hints = symbol_hints.iter().collect::<BTreeSet<_>>();
    target_terms
        .iter()
        .filter(|term| !symbol_hints.contains(term))
        .filter(|term| !path.contains(term.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ranges_overlap(start: usize, end: usize, unit_start: usize, unit_end: usize) -> bool {
    start < unit_end && end > unit_start
}

fn local_context_confidence(match_kind: &'static str, has_unit: bool) -> &'static str {
    match (match_kind, has_unit) {
        ("code_unit_id", _) | ("path_exact", true) => "exact",
        ("path_exact", false) | ("path_embedded", true) => "high",
        ("path_embedded", false) | ("path_suffix", true) => "medium",
        _ => "low",
    }
}

fn target_path_match(target: &str, path: &str) -> Option<TargetPathMatch> {
    find_embedded_path_match(target, path).or_else(|| find_query_path_token_match(target, path))
}

fn find_embedded_path_match(target: &str, path: &str) -> Option<TargetPathMatch> {
    if !path.contains('/') && !path.contains('.') && target.trim() != path {
        return None;
    }
    let mut search_start = 0;
    while let Some(relative_start) = target[search_start..].find(path) {
        let start = search_start + relative_start;
        let end = start + path.len();
        search_start = start + 1;
        if !target_boundary_before(target, start) {
            continue;
        }
        let (locator, locator_end) = parse_locator_after_path(target, end);
        if !target_boundary_after_locator(target, locator_end) {
            continue;
        }
        let exact_token =
            target[..start].trim().is_empty() && target[locator_end..].trim().is_empty();
        return Some(TargetPathMatch {
            match_kind: if exact_token {
                "path_exact"
            } else {
                "path_embedded"
            },
            line: locator.and_then(|locator| locator.line),
            byte_range: locator.and_then(|locator| locator.byte_range),
            rank: if exact_token { 0 } else { 1 },
        });
    }
    None
}

fn find_query_path_token_match(target: &str, indexed_path: &str) -> Option<TargetPathMatch> {
    target_path_tokens(target).into_iter().find_map(|token| {
        let (path_text, locator) = split_query_path_locator(token);
        if !is_safe_query_path_text(path_text) {
            return None;
        }
        if indexed_path == path_text {
            return Some(TargetPathMatch {
                match_kind: "path_exact",
                line: locator.and_then(|locator| locator.line),
                byte_range: locator.and_then(|locator| locator.byte_range),
                rank: 0,
            });
        }
        if path_text.contains('/') && indexed_path.ends_with(&format!("/{path_text}")) {
            return Some(TargetPathMatch {
                match_kind: "path_suffix",
                line: locator.and_then(|locator| locator.line),
                byte_range: locator.and_then(|locator| locator.byte_range),
                rank: 2,
            });
        }
        None
    })
}

fn target_path_tokens(target: &str) -> Vec<&str> {
    target
        .split(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
                )
        })
        .map(|token| token.trim_matches(|character: char| matches!(character, '.' | ':' | '=')))
        .filter(|token| token.contains('/') || token.contains('.'))
        .collect()
}

fn split_query_path_locator(token: &str) -> (&str, Option<TargetLocator>) {
    let Some((path, suffix)) = token.rsplit_once(':') else {
        return (token, None);
    };
    if let Some(locator) = parse_locator_text(suffix) {
        (path, Some(locator))
    } else {
        (token, None)
    }
}

fn parse_locator_after_path(target: &str, path_end: usize) -> (Option<TargetLocator>, usize) {
    let rest = &target[path_end..];
    let Some(rest) = rest.strip_prefix(':') else {
        return (None, path_end);
    };
    let locator_len = rest
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '-')
        .map(char::len_utf8)
        .sum::<usize>();
    if locator_len == 0 {
        return (None, path_end);
    }
    let locator_text = &rest[..locator_len];
    match parse_locator_text(locator_text) {
        Some(locator) => (Some(locator), path_end + 1 + locator_len),
        None => (None, path_end),
    }
}

fn parse_locator_text(text: &str) -> Option<TargetLocator> {
    if let Some((start, end)) = text.split_once('-') {
        let start = start.parse::<usize>().ok()?;
        let end = end.parse::<usize>().ok()?;
        if start >= end {
            return None;
        }
        return Some(TargetLocator {
            line: None,
            byte_range: Some((start, end)),
        });
    }
    let line = text.parse::<usize>().ok()?;
    if line == 0 {
        return None;
    }
    Some(TargetLocator {
        line: Some(line),
        byte_range: None,
    })
}

fn target_boundary_after_locator(target: &str, end: usize) -> bool {
    target[end..]
        .chars()
        .next()
        .is_none_or(|character| !is_path_character(character) && character != ':')
}

fn is_safe_query_path_text(path: &str) -> bool {
    !path.is_empty()
        && (path.contains('/') || path.contains('.'))
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains("://")
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn target_identifier_tokens(target: &str) -> Vec<&str> {
    target
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .filter(|token| !token.chars().all(|character| character.is_ascii_digit()))
        .collect()
}

fn local_context_read_plan_for_unit(unit: &IndexedCodeUnitRecord) -> ReadPlan {
    let why =
        "read this resolved target body before editing; no pattern-family evidence is available";
    let item = ReadPlanItem {
        purpose: ReadPlanPurpose::TargetBodyRequiredForEdit,
        path: unit.path.clone(),
        content_hash: unit.content_hash.clone(),
        start_byte: unit.start_byte,
        end_byte: unit.end_byte,
        start_line: None,
        end_line: None,
        estimated_tokens: estimated_local_read_plan_tokens(
            &unit.path,
            unit.content_hash.as_str(),
            unit.start_byte,
            unit.end_byte,
            why,
        ),
        why: why.to_string(),
        source_required_before_edit: true,
        source_snippets_included: false,
    };
    local_context_read_plan(item)
}

fn local_context_read_plan_for_file(
    file: &IndexedFileRecord,
) -> Result<ReadPlan, RepoGrammarError> {
    let end_byte = usize::try_from(file.size_bytes).map_err(|_| {
        RepoGrammarError::InvalidInput(
            "indexed file size exceeds supported read-plan range".to_string(),
        )
    })?;
    let why =
        "read this resolved target file before editing; no pattern-family evidence is available";
    let item = ReadPlanItem {
        purpose: ReadPlanPurpose::TargetBodyRequiredForEdit,
        path: file.path.clone(),
        content_hash: file.content_hash.clone(),
        start_byte: 0,
        end_byte,
        start_line: None,
        end_line: None,
        estimated_tokens: estimated_local_read_plan_tokens(
            &file.path,
            file.content_hash.as_str(),
            0,
            end_byte,
            why,
        ),
        why: why.to_string(),
        source_required_before_edit: true,
        source_snippets_included: false,
    };
    Ok(local_context_read_plan(item))
}

fn local_context_read_plan(item: ReadPlanItem) -> ReadPlan {
    ReadPlan {
        estimated_tokens: item.estimated_tokens,
        source_snippets_included: false,
        requires_source_before_edit: true,
        selection_strategy: "deterministic_local_context_v1",
        budget_satisfied: true,
        items: vec![item],
        line_range_omissions: Vec::new(),
    }
}

fn estimated_local_read_plan_tokens(
    path: &str,
    content_hash: &str,
    start_byte: usize,
    end_byte: usize,
    why: &str,
) -> usize {
    let bytes = ReadPlanPurpose::TargetBodyRequiredForEdit.as_str().len()
        + path.len()
        + content_hash.len()
        + start_byte.to_string().len()
        + end_byte.to_string().len()
        + why.len()
        + 48;
    bytes.div_ceil(4).max(1)
}

fn local_context_ambiguity_unknown(
    affected_claim: &'static str,
    candidates: Vec<&str>,
) -> FamilyQueryUnknown {
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim: affected_claim.to_string(),
        recovery: Some(format!(
            "narrow the target to one exact repo-relative path or code unit id; candidate targets: {}",
            candidates.join(", ")
        )),
    }
}

pub fn enrich_read_plan_line_ranges(
    request: SourceSpanRenderRequest,
    source_store: &impl SourceStore,
    read_plan: &ReadPlan,
) -> Result<ReadPlan, RepoGrammarError> {
    let mut enriched = read_plan.clone();
    enriched.line_range_omissions.clear();
    for item in &mut enriched.items {
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
                enriched.line_range_omissions.push(read_plan_line_omission(
                    item,
                    "stale_evidence",
                    "source changed or disappeared; use normal Read/Grep for this span",
                ));
                continue;
            }
            Err(SourceStoreError::TooLarge(_)) => {
                enriched.line_range_omissions.push(read_plan_line_omission(
                    item,
                    "source_too_large",
                    "source exceeds the configured read limit; use normal Read/Grep if needed",
                ));
                continue;
            }
            Err(SourceStoreError::NonUtf8(_)) => {
                enriched.line_range_omissions.push(read_plan_line_omission(
                    item,
                    "non_utf8_source",
                    "source is not UTF-8; use normal tooling for this file",
                ));
                continue;
            }
            Err(SourceStoreError::Unavailable(_)) => {
                enriched.line_range_omissions.push(read_plan_line_omission(
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

        match line_range_for_span(&source.text, item.start_byte, item.end_byte) {
            Ok((start_line, end_line)) => {
                item.start_line = Some(start_line);
                item.end_line = Some(end_line);
            }
            Err(RenderSourceSpanItemError::InvalidRange) => {
                enriched.line_range_omissions.push(read_plan_line_omission(
                    item,
                    "invalid_source_range",
                    "stored source range is invalid; use normal Read/Grep for this span",
                ));
            }
            Err(RenderSourceSpanItemError::SpanTooLarge) => {
                enriched.line_range_omissions.push(read_plan_line_omission(
                    item,
                    "source_span_too_large",
                    "source span exceeds the configured render limit; use normal Read if needed",
                ));
            }
        }
    }
    Ok(enriched)
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

fn read_plan_line_omission(
    item: &ReadPlanItem,
    reason: &'static str,
    guidance: &'static str,
) -> ReadPlanLineRangeOmission {
    ReadPlanLineRangeOmission {
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
    let (start_line, end_line) = line_range_for_span(source, start_byte, end_byte)?;
    let line_starts = source_line_starts(source);
    let mut rendered = String::new();
    for line_number in start_line..=end_line {
        let line_start = line_starts[line_number - 1];
        let line_end = line_starts
            .get(line_number)
            .copied()
            .unwrap_or(source.len());
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

fn line_range_for_span(
    source: &str,
    start_byte: usize,
    end_byte: usize,
) -> Result<(usize, usize), RenderSourceSpanItemError> {
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
    Ok((start_line, end_line))
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
    if options.include_variations {
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
        let mut readiness = semantic_fact_claim_input_readiness(kind, certainty, freshness);
        if let ClaimInputReadiness::Blocked { unknown } = &mut readiness {
            if unknown.reason == UnknownReasonCode::StaleEvidence {
                unknown.recovery = Some(recovery_guidance(RecoveryAction::Resync).to_string());
            }
        }
        facts.push(SemanticFactReadinessRecord {
            fact_id: fact.fact_id,
            readiness,
        });
    }
    Ok(SemanticFactReadinessReport {
        active_generation: snapshot.generation_id,
        facts,
    })
}

fn index_store_error(error: IndexStoreError) -> RepoGrammarError {
    match error {
        IndexStoreError::SchemaVersionOutdated(message) => RepoGrammarError::InvalidInput(format!(
            "{message}; {}",
            recovery_guidance(RecoveryAction::Resync)
        )),
        IndexStoreError::Unavailable(message)
        | IndexStoreError::InvalidState(message)
        | IndexStoreError::InvalidRecord(message) => RepoGrammarError::InvalidInput(message),
    }
}

fn family_store_error(error: StoreError) -> RepoGrammarError {
    match error {
        StoreError::SchemaVersionOutdated(message) => RepoGrammarError::InvalidInput(format!(
            "{message}; {}",
            recovery_guidance(RecoveryAction::Resync)
        )),
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
        prevalence: family.family.prevalence,
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

struct FamilyTargetMatch {
    family: ActiveFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetMatchKind {
    ExactFamilyId,
    ExactMemberId,
    ExactMemberRole,
    EvidencePath,
}

fn family_target_match(
    family: &ActiveFamily,
    target: &str,
    mode: FamilyLookupMode,
) -> Option<TargetMatchKind> {
    match mode {
        FamilyLookupMode::ExactFamilyId => {
            (family.family.family_id == target).then_some(TargetMatchKind::ExactFamilyId)
        }
        FamilyLookupMode::ExactMemberId => family
            .members
            .iter()
            .any(|member| member.code_unit_id == target)
            .then_some(TargetMatchKind::ExactMemberId),
        FamilyLookupMode::FuzzyQuery => {
            if family.family.family_id == target {
                Some(TargetMatchKind::ExactFamilyId)
            } else if family
                .members
                .iter()
                .any(|member| member.code_unit_id == target)
            {
                Some(TargetMatchKind::ExactMemberId)
            } else if family.members.iter().any(|member| member.role == target) {
                Some(TargetMatchKind::ExactMemberRole)
            } else if family
                .evidence
                .iter()
                .any(|evidence| path_matches_target(&evidence.path, target))
            {
                Some(TargetMatchKind::EvidencePath)
            } else {
                None
            }
        }
    }
}

fn path_matches_target(path: &str, target: &str) -> bool {
    target_path_match(target, path).is_some()
}

fn target_boundary_before(target: &str, start: usize) -> bool {
    target[..start]
        .chars()
        .next_back()
        .is_none_or(|character| !is_path_character(character))
}

fn is_path_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-')
}

fn ambiguous_target_unknown(matches: &[FamilyTargetMatch]) -> Option<FamilyQueryUnknown> {
    let mut candidate_family_ids = matches
        .iter()
        .map(|matched| matched.family.family.family_id.as_str())
        .collect::<Vec<_>>();
    candidate_family_ids.sort_unstable();
    candidate_family_ids.dedup();
    if candidate_family_ids.len() <= 1 {
        return None;
    }
    Some(FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim: "query target ambiguity".to_string(),
        recovery: Some(format!(
            "narrow the target to one exact family id or member id; candidate families: {}",
            candidate_family_ids.join(", ")
        )),
    })
}

fn readiness_language_diagnostics_from_stats(
    stats: &[RepoShapeLanguageStats],
) -> Vec<RepoShapeLanguageDiagnostics> {
    readiness_language_scopes()
        .iter()
        .map(|scope| {
            let stats = stats
                .iter()
                .find(|stats| stats.language == scope.language)
                .cloned()
                .unwrap_or_else(|| RepoShapeLanguageStats {
                    language: scope.language.to_string(),
                    indexed_file_count: 0,
                    indexed_code_unit_count: 0,
                    eligible_code_units: 0,
                    family_count: 0,
                    family_member_count: 0,
                    covered_code_units: 0,
                });
            let family_support_coverage =
                ratio(stats.covered_code_units, stats.eligible_code_units);
            RepoShapeLanguageDiagnostics {
                language: scope.language.to_string(),
                language_scope: scope.language_scope,
                indexed_file_count: stats.indexed_file_count,
                indexed_code_unit_count: stats.indexed_code_unit_count,
                eligible_code_units: stats.eligible_code_units,
                family_count: stats.family_count,
                family_member_count: stats.family_member_count,
                covered_code_units: stats.covered_code_units,
                family_support_coverage,
                support_risk: risk_from_density(family_support_coverage),
                preview_status: scope.preview_status,
            }
        })
        .collect()
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

fn token_saving_readiness(
    eligible_code_units: usize,
    family_count: usize,
    local_pattern_density: Option<f64>,
    family_support_coverage: Option<f64>,
) -> TokenSavingReadiness {
    if eligible_code_units == 0 {
        return TokenSavingReadiness::Unknown;
    }
    if family_count == 0 {
        return TokenSavingReadiness::Poor;
    }
    let density = local_pattern_density.unwrap_or(0.0);
    let coverage = family_support_coverage.unwrap_or(0.0);
    if density >= 0.35 && coverage >= 0.35 {
        TokenSavingReadiness::Partial
    } else {
        TokenSavingReadiness::Poor
    }
}

fn token_saving_blocking_reasons(
    eligible_code_units: usize,
    family_count: usize,
    local_pattern_density: Option<f64>,
    family_support_coverage: Option<f64>,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if eligible_code_units == 0 {
        reasons.push("no_supported_units");
    }
    if family_count == 0 {
        reasons.push("no_families");
    }
    if local_pattern_density.is_some_and(|density| density < 0.35)
        || family_support_coverage.is_some_and(|coverage| coverage < 0.35)
    {
        reasons.push("low_pattern_density");
    }
    reasons
}

/// Per-evidence-path freshness verdict. `Stale` is content-based (the file is
/// missing or its hash changed); `CannotVerify` is a non-content read failure
/// (too large, non-UTF-8, unavailable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvidencePathVerdict {
    Fresh,
    Stale,
    CannotVerify,
}

/// The single authoritative per-path freshness check. Both the single-family
/// gate (`family_evidence_is_fresh`) and the list-level rollup route their
/// per-path decision through here, so the content-vs-read-failure rules live in
/// exactly one place.
fn verify_evidence_path(
    request: &FamilyEvidenceFreshnessRequest,
    source_store: &impl SourceStore,
    path: &str,
    expected_content_hash: &ContentHash,
) -> Result<EvidencePathVerdict, RepoGrammarError> {
    let source = source_store.read_source(SourceReadRequest {
        repository_root: request.repository_root.clone(),
        path: path.to_string(),
        expected_content_hash: expected_content_hash.clone(),
        max_file_bytes: request.max_file_bytes,
    });
    match source {
        Ok(source) if source.path == path && &source.content_hash == expected_content_hash => {
            Ok(EvidencePathVerdict::Fresh)
        }
        Ok(_) => Err(RepoGrammarError::InvalidInput(
            "source freshness response is invalid".to_string(),
        )),
        Err(SourceStoreError::InvalidRequest(_)) => Err(RepoGrammarError::InvalidInput(
            "stored family evidence path is invalid".to_string(),
        )),
        // Missing or changed content means the claim no longer reflects the tree.
        Err(SourceStoreError::Missing(_)) | Err(SourceStoreError::HashMismatch(_)) => {
            Ok(EvidencePathVerdict::Stale)
        }
        // Non-content read failures cannot confirm staleness or freshness.
        Err(SourceStoreError::TooLarge(_))
        | Err(SourceStoreError::NonUtf8(_))
        | Err(SourceStoreError::Unavailable(_)) => Ok(EvidencePathVerdict::CannotVerify),
    }
}

fn family_evidence_is_fresh(
    request: &FamilyEvidenceFreshnessRequest,
    source_store: &impl SourceStore,
    family: &ActiveFamily,
) -> Result<bool, RepoGrammarError> {
    // A family with no evidence rows cannot be proven fresh. The freshness loop
    // would be vacuously true, so a corrupt or adversarial evidence-less family
    // row would be served as a confident match. Abstain instead.
    if family.evidence.is_empty() {
        return Ok(false);
    }
    for evidence in &family.evidence {
        // The single-family gate treats any non-`Fresh` verdict (stale or
        // unverifiable) as not fresh, preserving its original behavior.
        if verify_evidence_path(
            request,
            source_store,
            &evidence.path,
            &evidence.content_hash,
        )? != EvidencePathVerdict::Fresh
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn insufficient_support_unknown(affected_claim: impl Into<String>) -> FamilyQueryUnknown {
    let recovery = classify_query_evidence_recovery(
        RecoveryFreshness::Fresh,
        RecoveryEvidenceState::Unavailable,
    );
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim: affected_claim.into(),
        recovery: Some(recovery_guidance(recovery.action).to_string()),
    }
}

fn stale_evidence_unknown(affected_claim: impl Into<String>) -> FamilyQueryUnknown {
    let recovery =
        classify_query_evidence_recovery(RecoveryFreshness::Stale, RecoveryEvidenceState::Unknown);
    FamilyQueryUnknown {
        class: UnknownClass::Blocking,
        reason: UnknownReasonCode::StaleEvidence,
        affected_claim: affected_claim.into(),
        recovery: Some(recovery_guidance(recovery.action).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        RepositoryManifestStatus, RepositoryReadiness, RepositoryStatus,
    };
    use crate::core::model::{
        ContentHash, EstimatedPotentialTokenSavings, UnknownClass, UnknownReasonCode,
    };
    use crate::ports::family_store::{
        ActiveFamilies, ActiveFamilyCandidates, ActiveFamilyEvidenceProjection,
        ActiveFamilySummaries, IndexedFamilyCandidateRecord, IndexedFamilyRecord,
        IndexedFamilySummaryRecord, IndexedVariationSlotRecord,
    };
    use crate::ports::index_store::{
        ActiveClaimInputSnapshot, ActiveCodeUnits, ActiveIndexedFiles, ActiveIrGraph,
        ActiveRepoShapeStats, ActiveSemanticFacts, GenerationHandle, IndexedIrEdgeRecord,
        IndexedIrNodeRecord, RepoShapeLanguageStats, StorageInspection,
    };
    use crate::ports::source_store::SourceText;

    struct FakeStore {
        facts: Vec<IndexedSemanticFactRecord>,
        files: Vec<IndexedFileRecord>,
        units: Vec<IndexedCodeUnitRecord>,
        repo_shape_stats: ActiveRepoShapeStats,
        snapshot_reads: std::cell::Cell<usize>,
        indexed_file_reads: std::cell::Cell<usize>,
        code_unit_reads: std::cell::Cell<usize>,
    }

    impl FakeStore {
        fn new(facts: Vec<IndexedSemanticFactRecord>) -> Self {
            Self {
                facts,
                files: Vec::new(),
                units: Vec::new(),
                repo_shape_stats: empty_repo_shape_stats(),
                snapshot_reads: std::cell::Cell::new(0),
                indexed_file_reads: std::cell::Cell::new(0),
                code_unit_reads: std::cell::Cell::new(0),
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

        fn with_repo_shape_stats(mut self, stats: ActiveRepoShapeStats) -> Self {
            self.repo_shape_stats = stats;
            self
        }

        fn snapshot_reads(&self) -> usize {
            self.snapshot_reads.get()
        }

        fn indexed_file_reads(&self) -> usize {
            self.indexed_file_reads.get()
        }

        fn code_unit_reads(&self) -> usize {
            self.code_unit_reads.get()
        }
    }

    fn empty_repo_shape_stats() -> ActiveRepoShapeStats {
        repo_shape_stats(0, 0, 0, 0, Vec::new())
    }

    fn repo_shape_stats(
        eligible_code_units: usize,
        family_count: usize,
        family_member_count: usize,
        covered_code_units: usize,
        by_language: Vec<RepoShapeLanguageStats>,
    ) -> ActiveRepoShapeStats {
        let indexed_file_count = by_language
            .iter()
            .map(|stats| stats.indexed_file_count)
            .sum();
        let indexed_code_unit_count = by_language
            .iter()
            .map(|stats| stats.indexed_code_unit_count)
            .sum();
        ActiveRepoShapeStats {
            generation_id: "gen-000001".to_string(),
            indexed_file_count,
            indexed_code_unit_count,
            semantic_fact_count: 0,
            eligible_code_units,
            family_count,
            family_member_count,
            covered_code_units,
            by_language,
        }
    }

    fn repo_shape_language_stats(
        language: &str,
        eligible_code_units: usize,
        family_count: usize,
        family_member_count: usize,
        covered_code_units: usize,
    ) -> RepoShapeLanguageStats {
        RepoShapeLanguageStats {
            language: language.to_string(),
            indexed_file_count: eligible_code_units,
            indexed_code_unit_count: eligible_code_units,
            eligible_code_units,
            family_count,
            family_member_count,
            covered_code_units,
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

        fn remove_indexed_file(
            &self,
            _generation: &GenerationHandle,
            _path: &str,
        ) -> Result<(), IndexStoreError> {
            panic!("query read tests must not remove indexed files")
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
            self.indexed_file_reads
                .set(self.indexed_file_reads.get() + 1);
            Ok(ActiveIndexedFiles {
                generation_id: "gen-000001".to_string(),
                files: self.files.clone(),
            })
        }

        fn list_active_code_units(&self) -> Result<ActiveCodeUnits, IndexStoreError> {
            self.code_unit_reads.set(self.code_unit_reads.get() + 1);
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
            self.snapshot_reads.set(self.snapshot_reads.get() + 1);
            Ok(ActiveClaimInputSnapshot {
                generation_id: "gen-000001".to_string(),
                files: self.files.clone(),
                units: self.units.clone(),
                ir_nodes: Vec::new(),
                ir_edges: Vec::new(),
                semantic_facts: self.facts.clone(),
            })
        }

        fn active_repo_shape_stats(&self) -> Result<ActiveRepoShapeStats, IndexStoreError> {
            Ok(self.repo_shape_stats.clone())
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
        families: Vec<ActiveFamily>,
        show_family_reads: std::cell::Cell<usize>,
    }

    impl FakeFamilyStore {
        fn empty() -> Self {
            Self {
                active: ActiveFamilies {
                    generation_id: "gen-000001".to_string(),
                    families: Vec::new(),
                },
                families: Vec::new(),
                show_family_reads: std::cell::Cell::new(0),
            }
        }

        fn with_family() -> Self {
            let family = ActiveFamily {
                generation_id: "gen-000001".to_string(),
                family: IndexedFamilyRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    prevalence: crate::test_support::sample_family_prevalence(),
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
            Self::with_families(vec![family])
        }

        fn with_families(families: Vec<ActiveFamily>) -> Self {
            Self {
                active: ActiveFamilies {
                    generation_id: "gen-000001".to_string(),
                    families: families
                        .iter()
                        .map(|family| family.family.clone())
                        .collect(),
                },
                families,
                show_family_reads: std::cell::Cell::new(0),
            }
        }

        fn show_family_reads(&self) -> usize {
            self.show_family_reads.get()
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

        fn list_active_family_summaries(&self) -> Result<ActiveFamilySummaries, StoreError> {
            Ok(ActiveFamilySummaries {
                generation_id: self.active.generation_id.clone(),
                families: self
                    .families
                    .iter()
                    .map(|family| IndexedFamilySummaryRecord {
                        family_id: family.family.family_id.clone(),
                        classification: family.family.classification.clone(),
                        support: family.members.len(),
                        prevalence: family.family.prevalence.clone(),
                    })
                    .collect(),
            })
        }

        fn list_active_family_evidence_projection(
            &self,
        ) -> Result<ActiveFamilyEvidenceProjection, StoreError> {
            let mut rows = Vec::new();
            for family in &self.families {
                for evidence in &family.evidence {
                    rows.push(IndexedFamilyEvidenceProjectionRecord {
                        family_id: family.family.family_id.clone(),
                        path: evidence.path.clone(),
                        content_hash: evidence.content_hash.clone(),
                    });
                }
            }
            Ok(ActiveFamilyEvidenceProjection {
                generation_id: self.active.generation_id.clone(),
                rows,
            })
        }

        fn find_active_families_by_member(
            &self,
            code_unit_id: &str,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let candidates = self
                .families
                .iter()
                .filter(|family| {
                    family
                        .members
                        .iter()
                        .any(|member| member.code_unit_id == code_unit_id)
                })
                .map(|family| IndexedFamilyCandidateRecord {
                    family_id: family.family.family_id.clone(),
                })
                .collect();
            Ok(ActiveFamilyCandidates {
                generation_id: self.active.generation_id.clone(),
                candidates,
                truncated: false,
            })
        }

        fn find_active_families_by_role(
            &self,
            role: &str,
            limit: usize,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let mut candidates = self
                .families
                .iter()
                .filter(|family| family.members.iter().any(|member| member.role == role))
                .map(|family| IndexedFamilyCandidateRecord {
                    family_id: family.family.family_id.clone(),
                })
                .collect::<Vec<_>>();
            let truncated = candidates.len() > limit;
            candidates.truncate(limit);
            Ok(ActiveFamilyCandidates {
                generation_id: self.active.generation_id.clone(),
                candidates,
                truncated,
            })
        }

        fn find_active_families_by_evidence_path(
            &self,
            path: &str,
            limit: usize,
        ) -> Result<ActiveFamilyCandidates, StoreError> {
            let mut candidates = self
                .families
                .iter()
                .filter(|family| {
                    family.evidence.iter().any(|evidence| {
                        evidence.path == path
                            || (evidence.path.len() > path.len()
                                && evidence.path.ends_with(path)
                                && evidence.path.as_bytes()[evidence.path.len() - path.len() - 1]
                                    == b'/')
                    })
                })
                .map(|family| IndexedFamilyCandidateRecord {
                    family_id: family.family.family_id.clone(),
                })
                .collect::<Vec<_>>();
            let truncated = candidates.len() > limit;
            candidates.truncate(limit);
            Ok(ActiveFamilyCandidates {
                generation_id: self.active.generation_id.clone(),
                candidates,
                truncated,
            })
        }

        fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError> {
            self.show_family_reads.set(self.show_family_reads.get() + 1);
            Ok(self
                .families
                .iter()
                .find(|family| family.family.family_id == family_id)
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
        indexed_language_unit(path, "python", kind, index)
    }

    fn indexed_language_unit(
        path: &str,
        language: &str,
        kind: &str,
        index: usize,
    ) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}"),
            path: path.to_string(),
            language: language.to_string(),
            kind: kind.to_string(),
            start_byte: index * 10,
            end_byte: index * 10 + 8,
            content_hash: semantic_fact().content_hash,
        }
    }

    fn framework_role_fact(unit: &IndexedCodeUnitRecord, role: &str) -> IndexedSemanticFactRecord {
        let mut fact = semantic_fact_with_certainty("FRAMEWORK_HEURISTIC");
        fact.fact_id = format!("semantic-fact:{}:role", unit.id);
        fact.kind = "FRAMEWORK_ROLE".to_string();
        fact.subject = unit.id.clone();
        fact.target = Some(role.to_string());
        fact.origin_engine = "tree-sitter".to_string();
        fact.origin_method = "framework_role_detector".to_string();
        fact.code_unit_id = unit.id.clone();
        fact.path = unit.path.clone();
        fact.content_hash = unit.content_hash.clone();
        fact.start_byte = unit.start_byte;
        fact.end_byte = unit.end_byte;
        fact.note = "framework role".to_string();
        fact
    }

    fn unknown_fact_for_unit(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> IndexedSemanticFactRecord {
        unknown_fact_for_unit_with_assumptions(unit, reason, affected_claim, Vec::new())
    }

    fn unknown_fact_for_unit_with_assumptions(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
        extra_assumptions: Vec<String>,
    ) -> IndexedSemanticFactRecord {
        let mut fact = semantic_fact_with_certainty("UNKNOWN");
        fact.fact_id = format!("semantic-fact:{}:{}", unit.id, reason.as_protocol_str());
        fact.kind = "UNKNOWN".to_string();
        fact.subject = unit.id.clone();
        fact.target = Some(reason.as_protocol_str().to_string());
        let (origin_engine, origin_method) = if unit.language == "python" {
            ("python", "cpython_ast")
        } else if is_tsjs_language(&unit.language) {
            ("repogrammar-tsjs-syntax", "exact_anchor_v1")
        } else if unit.language == "java" {
            (
                "repogrammar-java-syntax",
                "tree_sitter_java_structural_anchors_v1",
            )
        } else if unit.language == "csharp" {
            (
                "repogrammar-csharp-syntax",
                "tree_sitter_csharp_structural_anchors_v1",
            )
        } else if is_c_cpp_language(&unit.language) {
            (
                "repogrammar-cpp-syntax",
                "tree_sitter_c_cpp_structural_anchors_v1",
            )
        } else if unit.language == "rust" {
            (
                "repogrammar-rust-syntax",
                "tree_sitter_rust_structural_anchors_v1",
            )
        } else {
            (unit.language.as_str(), "syntax_anchor")
        };
        fact.origin_engine = origin_engine.to_string();
        fact.origin_method = origin_method.to_string();
        fact.assumptions = vec![format!("affected_claim={affected_claim}")];
        fact.assumptions.extend(extra_assumptions);
        fact.code_unit_id = unit.id.clone();
        fact.path = unit.path.clone();
        fact.content_hash = unit.content_hash.clone();
        fact.start_byte = unit.start_byte;
        fact.end_byte = unit.end_byte;
        fact.note = "typed UNKNOWN".to_string();
        fact
    }

    fn bucket_count(buckets: &[UnknownInventoryBucket], key: &str) -> usize {
        buckets
            .iter()
            .find(|bucket| bucket.key == key)
            .map(|bucket| bucket.count)
            .unwrap_or(0)
    }

    fn blocks_support_count(
        buckets: &[UnknownInventoryBlocksSupportBucket],
        blocks_support: bool,
    ) -> usize {
        buckets
            .iter()
            .find(|bucket| bucket.blocks_support == blocks_support)
            .map(|bucket| bucket.count)
            .unwrap_or(0)
    }

    fn language_diagnostic<'a>(
        report: &'a RepoShapeDiagnosticsReport,
        language: &str,
    ) -> &'a RepoShapeLanguageDiagnostics {
        report
            .by_language
            .iter()
            .find(|diagnostic| diagnostic.language == language)
            .expect("language diagnostic exists")
    }

    fn language_unknown_summary<'a>(
        report: &'a UnknownInventoryReport,
        language: &str,
    ) -> &'a UnknownInventoryLanguageSummary {
        report
            .by_language_detail
            .iter()
            .find(|summary| summary.language == language)
            .expect("language unknown summary exists")
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

    fn active_family_with_evidence(family_id: &str, path: &str, index: usize) -> ActiveFamily {
        ActiveFamily {
            generation_id: "gen-000001".to_string(),
            family: IndexedFamilyRecord {
                family_id: family_id.to_string(),
                classification: "DOMINANT_PATTERN".to_string(),
                prevalence: crate::test_support::sample_family_prevalence(),
            },
            members: vec![IndexedFamilyMemberRecord {
                family_id: family_id.to_string(),
                code_unit_id: format!("unit:{path}#handler:{index}"),
                role: format!("role:{index}"),
            }],
            variation_slots: Vec::new(),
            evidence: vec![IndexedFamilyEvidenceRecord {
                evidence_id: format!("family-evidence:{index:06}"),
                family_id: family_id.to_string(),
                code_unit_id: format!("unit:{path}#handler:{index}"),
                covered_claims: vec!["canonical".to_string(), "support".to_string()],
                path: path.to_string(),
                content_hash: semantic_fact().content_hash,
                start_byte: index,
                end_byte: index + 20,
                note: "support evidence".to_string(),
            }],
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
            prevalence: crate::test_support::sample_family_prevalence(),
            members: Vec::new(),
            variation_slots: Vec::new(),
            evidence,
            unknowns: Vec::new(),
        }
    }

    fn claim_evidence(
        evidence_id: &str,
        path: &str,
        claim: &str,
        index: usize,
    ) -> IndexedFamilyEvidenceRecord {
        IndexedFamilyEvidenceRecord {
            evidence_id: evidence_id.to_string(),
            family_id: "family:typescript:express_route:express".to_string(),
            code_unit_id: format!("unit:{path}#handler:{index}"),
            covered_claims: vec![claim.to_string()],
            path: path.to_string(),
            content_hash: semantic_fact().content_hash,
            start_byte: index,
            end_byte: index + 20,
            note: claim.to_string(),
        }
    }

    #[test]
    fn read_plan_variation_guard_respects_include_variations_flag() {
        let family = family_detail_with_evidence(vec![
            claim_evidence("family-evidence:000000", "src/a.ts", "canonical", 0),
            claim_evidence("family-evidence:000001", "src/b.ts", "variation", 1),
        ]);

        let hidden = build_read_plan(
            &family,
            None,
            FamilyLookupMode::ExactFamilyId,
            FamilyOutputOptions {
                include_variations: false,
                ..Default::default()
            },
        );
        assert!(
            hidden
                .items
                .iter()
                .all(|item| item.purpose != ReadPlanPurpose::VariationGuard),
            "include_variations=false must not emit a variation guard"
        );

        let shown = build_read_plan(
            &family,
            None,
            FamilyLookupMode::ExactFamilyId,
            FamilyOutputOptions {
                include_variations: true,
                ..Default::default()
            },
        );
        assert!(
            shown
                .items
                .iter()
                .any(|item| item.purpose == ReadPlanPurpose::VariationGuard),
            "include_variations=true must emit a variation guard when variation evidence exists"
        );
    }

    #[test]
    fn family_detail_restores_non_blocking_unknowns_from_variation_slot_metadata() {
        let family_id = "family:python:fastapi_route:framework_fastapi_route".to_string();
        let detail = family_detail(ActiveFamily {
            generation_id: "gen-000001".to_string(),
            family: IndexedFamilyRecord {
                family_id: family_id.clone(),
                classification: "DOMINANT_PATTERN".to_string(),
                prevalence: crate::test_support::sample_family_prevalence(),
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
        let active_generation_available = matches!(
            &status,
            RepositoryStatus::Initialized { active_generation }
                if active_generation != "none" && active_generation != "not implemented"
        ) && inventory_indexing_is_readable(active_indexing);
        let initialized = matches!(&status, RepositoryStatus::Initialized { .. });
        let storage_health = if matches!(&status, RepositoryStatus::CorruptedManifest)
            || !missing_subdirs.is_empty()
            || storage == RepositoryImplementationStatus::Unhealthy
        {
            RecoveryHealth::Unhealthy
        } else if storage == RepositoryImplementationStatus::Available {
            RecoveryHealth::Healthy
        } else {
            RecoveryHealth::Unknown
        };
        let recovery = classify_recovery(&RecoveryContext {
            initialized,
            storage_health,
            lock_state: RecoveryLockState::Clear,
            active_index: active_generation_available,
            freshness: if active_generation_available {
                RecoveryFreshness::Fresh
            } else {
                RecoveryFreshness::NotApplicable
            },
            family_evidence: RecoveryEvidenceState::NotApplicable,
            autosync: RecoveryAutosyncState {
                configured: false,
                running: false,
                recommended: false,
            },
            agent: RecoveryAgentState::NotRequired,
        });
        let readiness = RepositoryReadiness {
            active_generation_available,
            query_ready: recovery.action == RecoveryAction::None,
            recovery: Some(recovery),
            ..RepositoryReadiness::default()
        };
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
            readiness,
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
        assert_eq!(pattern.guidance, "run repogrammar setup");
        assert!(!pattern.implemented);

        let inventory = fallback_report(query_preflight(
            QueryPreflightOperation::ActiveIndexInventory,
            &status,
        ));
        assert_eq!(inventory.reason, "repository is not initialized");
        assert_eq!(inventory.guidance, "run repogrammar setup");
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
            assert_eq!(fallback.guidance, "run repogrammar resync");
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
    fn query_preflight_uses_the_same_stale_and_unsupported_recovery_actions() {
        let mut status = status_report(
            RepositoryStatus::Initialized {
                active_generation: "gen-000001".to_string(),
            },
            RepositoryImplementationStatus::Available,
            RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
            Vec::new(),
        );
        status.readiness.recovery = Some(classify_query_evidence_recovery(
            RecoveryFreshness::Stale,
            RecoveryEvidenceState::Available,
        ));
        let stale = fallback_report(query_preflight(
            QueryPreflightOperation::PatternFamilyQuery,
            &status,
        ));
        assert_eq!(stale.guidance, "run repogrammar resync");

        status.readiness.recovery = Some(classify_query_evidence_recovery(
            RecoveryFreshness::Unsupported,
            RecoveryEvidenceState::NotApplicable,
        ));
        let unsupported = fallback_report(query_preflight(
            QueryPreflightOperation::PatternFamilyQuery,
            &status,
        ));
        assert_eq!(unsupported.guidance, "use source fallback");
        assert_eq!(
            recovery_guidance(status.readiness.recovery.expect("recovery").action),
            "use source fallback; the target is unsupported"
        );
    }

    #[test]
    fn query_preflight_does_not_block_a_readable_index_on_autosync_recommendation() {
        let mut status = status_report(
            RepositoryStatus::Initialized {
                active_generation: "gen-000001".to_string(),
            },
            RepositoryImplementationStatus::Available,
            RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
            Vec::new(),
        );
        status.readiness.recovery = Some(classify_recovery(&RecoveryContext {
            initialized: true,
            storage_health: RecoveryHealth::Healthy,
            lock_state: RecoveryLockState::Clear,
            active_index: true,
            freshness: RecoveryFreshness::Fresh,
            family_evidence: RecoveryEvidenceState::NotApplicable,
            autosync: RecoveryAutosyncState {
                configured: true,
                running: false,
                recommended: true,
            },
            agent: RecoveryAgentState::NotRequired,
        }));

        assert_eq!(
            status.readiness.recovery.expect("recovery").action,
            RecoveryAction::StartAutosync
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
    fn unknown_inventory_aggregates_typed_unknowns_from_active_snapshot() {
        let unit = indexed_python_unit("app/routes.py", "fastapi_route", 0);
        let facts = vec![
            framework_role_fact(&unit, "framework:fastapi.route"),
            unknown_fact_for_unit(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "python_import_resolution",
            ),
            unknown_fact_for_unit(
                &unit,
                UnknownReasonCode::RuntimeDependencyInjection,
                "fastapi_dependency_target",
            ),
        ];
        let store = FakeStore::new(facts)
            .with_files(vec![IndexedFileRecord {
                path: unit.path.clone(),
                content_hash: unit.content_hash.clone(),
                size_bytes: 42,
                language: unit.language.clone(),
            }])
            .with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        assert_eq!(report.inventory_scope, UNKNOWN_INVENTORY_SCOPE);
        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.total_unknowns, 2);
        assert_eq!(report.blocking_unknowns, 1);
        assert_eq!(report.non_blocking_unknowns, 1);
        assert_eq!(bucket_count(&report.by_language, "python"), 2);
        assert_eq!(bucket_count(&report.by_reason_code, "UnresolvedImport"), 1);
        assert_eq!(
            bucket_count(&report.by_reason_code, "RuntimeDependencyInjection"),
            1
        );
        assert_eq!(
            bucket_count(&report.by_required_mechanism, "python_import_graph"),
            1
        );
        assert_eq!(
            bucket_count(&report.by_required_mechanism, "fastapi_dependency_graph"),
            1
        );
        assert_eq!(
            bucket_count(&report.by_framework_role, "framework:fastapi.route"),
            2
        );
        assert_eq!(bucket_count(&report.by_role_state, "single"), 2);
        assert_eq!(blocks_support_count(&report.by_blocks_support, true), 1);
        assert_eq!(blocks_support_count(&report.by_blocks_support, false), 1);
        assert_eq!(
            bucket_count(&report.by_recovery_code, "resolve_import_graph"),
            1
        );
        assert_eq!(bucket_count(&report.by_recovery_code, "enable_provider"), 1);
        let python = language_unknown_summary(&report, "python");
        assert_eq!(python.total_unknowns, 2);
        assert_eq!(python.blocking_unknowns, 1);
        assert_eq!(
            bucket_count(&python.top_required_mechanisms, "fastapi_dependency_graph"),
            1
        );
        assert_eq!(
            bucket_count(&python.top_required_mechanisms, "python_import_graph"),
            1
        );
        assert_eq!(
            bucket_count(&python.top_reason_codes, "UnresolvedImport"),
            1
        );
    }

    #[test]
    fn unknown_inventory_keeps_legacy_class_counts_orthogonal_to_resolution() {
        let unit = indexed_language_unit("src/routes.ts", "typescript", "handler", 0);
        let facts = vec![
            framework_role_fact(&unit, "framework:express.route_handler"),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::DynamicImport,
                "tsjs_import_resolution",
                vec!["tsjs_unknown_kind=dynamic_import".to_string()],
            ),
        ];
        let store = FakeStore::new(facts).with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        assert_eq!(report.total_unknowns, 1);
        assert_eq!(report.blocking_unknowns, 1);
        assert_eq!(report.non_blocking_unknowns, 0);
        assert_eq!(report.recoverable_unknowns, 0);
        assert_eq!(report.irreducible_unknowns, 0);
        assert_eq!(blocks_support_count(&report.by_blocks_support, true), 1);
        assert_eq!(
            bucket_count(&report.by_recovery_code, "runtime_trace_required"),
            1
        );
    }

    #[test]
    fn unknown_inventory_counts_ambiguous_roles_as_support_risk() {
        let unit = indexed_python_unit("app/routes.py", "handler", 0);
        let facts = vec![
            framework_role_fact(&unit, "framework:fastapi.route"),
            framework_role_fact(&unit, "framework:pytest.fixture"),
            unknown_fact_for_unit(
                &unit,
                UnknownReasonCode::InsufficientSupport,
                "python_family_membership",
            ),
        ];
        let store = FakeStore::new(facts).with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        assert_eq!(report.total_unknowns, 1);
        assert_eq!(bucket_count(&report.by_framework_role, "ambiguous"), 1);
        assert_eq!(bucket_count(&report.by_role_state, "ambiguous"), 1);
        assert_eq!(blocks_support_count(&report.by_blocks_support, true), 1);
    }

    #[test]
    fn unknown_inventory_maps_required_mechanisms_by_language_and_claim() {
        let python = indexed_language_unit("tests/test_app.py", "python", "pytest_test", 0);
        let tsx = indexed_language_unit("src/routes.tsx", "typescript-react", "handler", 1);
        let rust = indexed_language_unit("src/lib.rs", "rust", "function", 2);
        let java = indexed_language_unit("src/main/App.java", "java", "method", 3);
        let facts = vec![
            framework_role_fact(&python, "framework:pytest.fixture"),
            unknown_fact_for_unit(
                &python,
                UnknownReasonCode::PytestFixtureInjection,
                "pytest_fixture_binding",
            ),
            framework_role_fact(&tsx, "framework:fastify.route"),
            unknown_fact_for_unit(
                &tsx,
                UnknownReasonCode::FrameworkMagic,
                "fastify_receiver_binding",
            ),
            unknown_fact_for_unit(
                &rust,
                UnknownReasonCode::MacroOrPreprocessor,
                "rust_macro_expansion",
            ),
            framework_role_fact(&java, "framework:spring.controller"),
            unknown_fact_for_unit(
                &java,
                UnknownReasonCode::FrameworkMagic,
                "java_spring_route_path",
            ),
        ];
        let store = FakeStore::new(facts).with_units(vec![python, tsx, rust, java]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        assert_eq!(
            bucket_count(&report.by_required_mechanism, "pytest_fixture_graph"),
            1
        );
        assert_eq!(
            bucket_count(&report.by_required_mechanism, "fastify_receiver_model"),
            1
        );
        assert_eq!(
            bucket_count(&report.by_required_mechanism, "rust_macro_boundary"),
            1
        );
        assert_eq!(
            bucket_count(
                &report.by_required_mechanism,
                "java_spring_route_literal_model"
            ),
            1
        );
        assert_eq!(bucket_count(&report.by_language, "typescript-react"), 1);
        let tsjs = language_unknown_summary(&report, "typescript/javascript");
        assert_eq!(tsjs.total_unknowns, 1);
        assert_eq!(tsjs.blocking_unknowns, 1);
        assert_eq!(
            bucket_count(&tsjs.top_required_mechanisms, "fastify_receiver_model"),
            1
        );
        let rust = language_unknown_summary(&report, "rust");
        assert_eq!(
            bucket_count(&rust.top_required_mechanisms, "rust_macro_boundary"),
            1
        );
        // Semantic obligations are a source-free refinement over the same typed
        // unknowns: the fixture and both framework-magic questions become
        // framework-identity obligations, and the macro question becomes a
        // macro-expansion obligation. The vocabulary is fixed and source-free.
        assert_eq!(bucket_count(&report.by_obligation, "framework_identity"), 3);
        assert_eq!(bucket_count(&report.by_obligation, "macro_expansion"), 1);
        assert!(!format!("{:?}", report.by_obligation).contains('/'));
    }

    #[test]
    fn semantic_obligation_refines_reason_and_context_without_weakening_unknown() {
        // Governance quality states are not semantic obligations.
        for reason in [
            UnknownReasonCode::StaleEvidence,
            UnknownReasonCode::ConflictingFacts,
            UnknownReasonCode::InsufficientSupport,
        ] {
            assert_eq!(
                semantic_obligation(reason, "any_claim", "", &[]),
                SemanticObligation::Governance
            );
        }
        // Runtime-defined reasons stay irreducible.
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::MonkeyPatch,
                "python_call_target",
                "",
                &[]
            ),
            SemanticObligation::RuntimeIrreducible
        );
        // Structural obligation kinds.
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::UnresolvedImport,
                "python_import_resolution",
                "",
                &[]
            ),
            SemanticObligation::SymbolBinding
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::MissingDependency,
                "python_project_config",
                "",
                &[]
            ),
            SemanticObligation::ExternalDependency
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::BuildVariantAmbiguity,
                "rust_build_variant",
                "",
                &[]
            ),
            SemanticObligation::BuildVariant
        );
        // Dependency injection: framework role -> framework identity; bare untyped
        // injection -> type identity.
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::RuntimeDependencyInjection,
                "fastapi_dependency_target",
                "framework:fastapi.route",
                &[],
            ),
            SemanticObligation::FrameworkIdentity
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::RuntimeDependencyInjection,
                "python_framework_identity",
                "",
                &[],
            ),
            SemanticObligation::TypeIdentity
        );
        // FrameworkMagic disambiguates: rust/static Python dispatch remains a
        // provider target; only an exact runtime boundary is irreducible.
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::FrameworkMagic,
                "rust_trait_dispatch",
                "",
                &["rust_trait_dispatch_trait=Handler".to_string()],
            ),
            SemanticObligation::DispatchTarget
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::FrameworkMagic,
                "python_call_target",
                "",
                &[]
            ),
            SemanticObligation::DispatchTarget
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::FrameworkMagic,
                "python_call_target",
                "",
                &["runtime_boundary=getattr".to_string()]
            ),
            SemanticObligation::RuntimeIrreducible
        );
        assert_eq!(
            semantic_obligation(
                UnknownReasonCode::FrameworkMagic,
                "python_framework_identity",
                "framework:pytest.fixture",
                &[],
            ),
            SemanticObligation::FrameworkIdentity
        );
    }

    #[test]
    fn query_unknown_metric_preserves_legacy_dynamic_import_projection_without_assumptions() {
        let metric = family_query_unknown_metric(&FamilyQueryUnknown {
            class: UnknownClass::Blocking,
            reason: UnknownReasonCode::DynamicImport,
            affected_claim: "python_import_resolution".to_string(),
            recovery: Some("legacy free text is not metric input".to_string()),
        });

        assert_eq!(metric.unknown_class, "blocking_unknown");
        assert_eq!(metric.reason_code, "DynamicImport");
        assert_eq!(metric.required_mechanism, "python_import_graph");
        assert_eq!(metric.obligation, "symbol_binding");
        assert_eq!(metric.recovery_code, "resolve_import_graph");
    }

    #[test]
    fn unknown_disposition_claim_impact_matches_authoritative_family_classifier() {
        let cases = [
            (
                "python",
                UnknownReasonCode::UnresolvedImport,
                "python_import_resolution",
                "framework:fastapi.route",
                "python",
                "cpython_ast",
            ),
            (
                "typescript",
                UnknownReasonCode::DynamicImport,
                "tsjs_import_resolution",
                "framework:express.route_handler",
                "repogrammar-tsjs-syntax",
                "exact_anchor_v1",
            ),
            (
                "java",
                UnknownReasonCode::FrameworkMagic,
                "java_spring_proxy_semantics",
                "framework:spring.controller",
                "repogrammar-java-syntax",
                "tree_sitter_java_structural_anchors_v1",
            ),
            (
                "csharp",
                UnknownReasonCode::FrameworkMagic,
                "csharp_dynamic_binding",
                "framework:aspnetcore.controller",
                "repogrammar-csharp-syntax",
                "tree_sitter_csharp_structural_anchors_v1",
            ),
            (
                "cpp",
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_macro_boundary",
                "framework:gtest.test",
                "repogrammar-cpp-syntax",
                "tree_sitter_c_cpp_structural_anchors_v1",
            ),
            (
                "rust",
                UnknownReasonCode::MacroOrPreprocessor,
                "rust_macro_expansion",
                "framework:repogrammar.rust_family_gate",
                "repogrammar-rust-syntax",
                "tree_sitter_rust_structural_anchors_v1",
            ),
        ];

        for (language, reason, claim, role, engine, method) in cases {
            let family_effect =
                classify_unknown_family_effect(language, reason, claim, Some(role), engine, method)
                    .expect("table case must have a family effect");
            let disposition = classify_unknown_disposition(UnknownPolicyContext {
                language,
                reason,
                affected_claim: claim,
                framework_role: role,
                assumptions: &[],
                origin_engine: engine,
                origin_method: method,
                explicit_legacy_class: None,
                role_is_ambiguous: false,
                family_role_is_exact: true,
            });

            assert_eq!(
                disposition.claim_impact,
                family_effect
                    .claim_impact()
                    .expect("family effects must carry a family impact class"),
                "{claim}"
            );
            assert_eq!(
                disposition.legacy_class, family_effect.class,
                "legacy public class must preserve the family-effect projection for {claim}"
            );
        }
    }

    #[test]
    fn resolution_axis_distinguishes_static_recovery_from_runtime_and_execution_boundaries() {
        let disposition =
            |language, reason, claim, role, assumptions: &[String], engine, method| {
                classify_unknown_disposition(UnknownPolicyContext {
                    language,
                    reason,
                    affected_claim: claim,
                    framework_role: role,
                    assumptions,
                    origin_engine: engine,
                    origin_method: method,
                    explicit_legacy_class: None,
                    role_is_ambiguous: false,
                    family_role_is_exact: true,
                })
            };

        let static_import = disposition(
            "python",
            UnknownReasonCode::UnresolvedImport,
            "python_import_resolution",
            "framework:fastapi.route",
            &[],
            "python",
            "cpython_ast",
        );
        let data_dependent_import = ["runtime_boundary=data_dependent_import".to_string()];
        let dynamic_import = disposition(
            "python",
            UnknownReasonCode::DynamicImport,
            "python_import_resolution",
            "framework:fastapi.route",
            &data_dependent_import,
            "python",
            "cpython_ast",
        );
        assert_eq!(static_import.claim_impact, ClaimImpact::Blocking);
        assert_eq!(dynamic_import.claim_impact, ClaimImpact::Blocking);
        assert_eq!(static_import.resolution_class, ResolutionClass::Recoverable);
        assert_eq!(
            dynamic_import.resolution_class,
            ResolutionClass::Irreducible
        );
        assert_eq!(static_import.recovery_code, "resolve_import_graph");
        assert_eq!(dynamic_import.recovery_code, "runtime_trace_required");
        // Public class remains the legacy claim-impact projection for both.
        assert_eq!(static_import.legacy_class, UnknownClass::Blocking);
        assert_eq!(dynamic_import.legacy_class, UnknownClass::Blocking);

        let runtime_eval = ["runtime_boundary=eval".to_string()];
        let spring_proxy = ["java_unknown_kind=spring_proxy_semantics".to_string()];
        let mockito_runtime = ["java_unknown_kind=mockito_runtime_mocks".to_string()];
        let dynamic_binder = ["csharp_unknown_kind=dynamic_member_binding".to_string()];
        for (language, claim, role, assumptions, engine, method) in [
            (
                "python",
                "python_call_target",
                "framework:fastapi.route",
                runtime_eval.as_slice(),
                "python",
                "cpython_ast",
            ),
            (
                "java",
                "java_spring_proxy_semantics",
                "framework:spring.controller",
                spring_proxy.as_slice(),
                "repogrammar-java-syntax",
                "tree_sitter_java_structural_anchors_v1",
            ),
            (
                "java",
                "java_mockito_runtime_mocks",
                "framework:mockito.test",
                mockito_runtime.as_slice(),
                "repogrammar-java-syntax",
                "tree_sitter_java_structural_anchors_v1",
            ),
            (
                "csharp",
                "csharp_dynamic_binding",
                "framework:aspnetcore.controller",
                dynamic_binder.as_slice(),
                "repogrammar-csharp-syntax",
                "tree_sitter_csharp_structural_anchors_v1",
            ),
        ] {
            assert_eq!(
                disposition(
                    language,
                    UnknownReasonCode::FrameworkMagic,
                    claim,
                    role,
                    assumptions,
                    engine,
                    method,
                )
                .resolution_class,
                ResolutionClass::Irreducible,
                "{claim} must not advertise a static recovery path"
            );
        }

        let proc_macro = ["rust_unknown_kind=proc_macro_attribute".to_string()];
        let build_script = ["rust_unknown_kind=build_script".to_string()];
        let bounded_cfg = [
            "rust_cfg_model=cargo_feature_cfg_model".to_string(),
            "rust_cfg_predicate=feature".to_string(),
            "rust_cfg_feature_declared=preview:true".to_string(),
        ];
        let complex_cfg = [
            "rust_cfg_model=cargo_feature_cfg_model".to_string(),
            "rust_cfg_predicate=complex".to_string(),
        ];
        for (reason, claim, assumptions) in [
            (
                UnknownReasonCode::MacroOrPreprocessor,
                "rust_macro_expansion",
                proc_macro.as_slice(),
            ),
            (
                UnknownReasonCode::BuildVariantAmbiguity,
                "rust_build_variant",
                build_script.as_slice(),
            ),
        ] {
            assert_eq!(
                disposition(
                    "rust",
                    reason,
                    claim,
                    "framework:repogrammar.rust_family_gate",
                    assumptions,
                    "repogrammar-rust-syntax",
                    "tree_sitter_rust_structural_anchors_v1",
                )
                .resolution_class,
                ResolutionClass::Irreducible
            );
        }
        assert_eq!(
            disposition(
                "rust",
                UnknownReasonCode::BuildVariantAmbiguity,
                "rust_build_variant",
                "framework:repogrammar.rust_family_gate",
                &bounded_cfg,
                "repogrammar-rust-syntax",
                "tree_sitter_rust_structural_anchors_v1",
            )
            .resolution_class,
            ResolutionClass::Recoverable
        );
        assert_eq!(
            disposition(
                "rust",
                UnknownReasonCode::BuildVariantAmbiguity,
                "rust_build_variant",
                "framework:repogrammar.rust_family_gate",
                &complex_cfg,
                "repogrammar-rust-syntax",
                "tree_sitter_rust_structural_anchors_v1",
            )
            .resolution_class,
            ResolutionClass::Recoverable,
            "unselected cfg requires a registered variability model, not execution"
        );
        assert_eq!(
            disposition(
                "rust",
                UnknownReasonCode::MacroOrPreprocessor,
                "rust_macro_expansion",
                "framework:repogrammar.rust_family_gate",
                &[],
                "repogrammar-rust-syntax",
                "tree_sitter_rust_structural_anchors_v1",
            )
            .resolution_class,
            ResolutionClass::Recoverable,
            "declarative macros can be expanded by a registered Rust analyzer"
        );
        assert_eq!(
            disposition(
                "cpp",
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_macro_boundary",
                "framework:gtest.test",
                &[],
                "repogrammar-cpp-syntax",
                "tree_sitter_c_cpp_structural_anchors_v1",
            )
            .resolution_class,
            ResolutionClass::Recoverable,
            "a fixed compile command can recover ordinary preprocessor expansion"
        );

        let static_call_target = disposition(
            "python",
            UnknownReasonCode::FrameworkMagic,
            "python_call_target",
            "framework:fastapi.route",
            &[],
            "python",
            "cpython_ast",
        );
        assert_eq!(
            static_call_target.resolution_class,
            ResolutionClass::Recoverable,
            "an absent runtime-boundary assumption leaves static provider recovery available"
        );

        let unregistered = disposition(
            "unknown",
            UnknownReasonCode::FrameworkMagic,
            "future_unknown_claim",
            "unknown",
            &[],
            "future-engine",
            "future-method",
        );
        assert_eq!(unregistered.resolution_class, ResolutionClass::Irreducible);
        assert_eq!(unregistered.recovery_code, "manual_review_required");
        assert_eq!(unregistered.legacy_class, UnknownClass::Recoverable);
    }

    #[test]
    fn unknown_inventory_recovery_codes_ignore_free_text_assumptions() {
        let unit = indexed_language_unit("src/app.ts", "typescript", "handler", 0);
        let leaking_recovery =
            "recovery=/Users/example/project/src/app.ts code_unit_id=unit:src/app.ts fact_id=semantic-fact:secret fn handler()"
                .to_string();
        let facts = vec![unknown_fact_for_unit_with_assumptions(
            &unit,
            UnknownReasonCode::UnresolvedImport,
            "tsjs_import_resolution",
            vec![leaking_recovery],
        )];
        let store = FakeStore::new(facts).with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");
        let recovery_debug = format!("{:?}", report.by_recovery_code);

        assert_eq!(
            bucket_count(&report.by_recovery_code, "resolve_import_graph"),
            1
        );
        assert!(!recovery_debug.contains("/Users/example"));
        assert!(!recovery_debug.contains("src/app.ts"));
        assert!(!recovery_debug.contains("code_unit_id"));
        assert!(!recovery_debug.contains("fact_id"));
        assert!(!recovery_debug.contains("fn handler"));
    }

    #[test]
    fn unknown_inventory_maps_tsjs_resolver_mechanism_buckets() {
        let unit = indexed_language_unit("src/app.ts", "typescript", "handler", 0);
        let facts = vec![
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "tsjs_import_resolution",
                vec!["tsjs_unknown_kind=unresolved_import".to_string()],
            ),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "tsjs_import_resolution",
                vec!["tsjs_unknown_kind=unresolved_path_alias".to_string()],
            ),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "tsjs_import_resolution",
                vec!["tsjs_unknown_kind=unresolved_root_dirs".to_string()],
            ),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::MissingDependency,
                "tsjs_package_entry",
                vec!["tsjs_unknown_kind=missing_package_entry".to_string()],
            ),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::DynamicImport,
                "tsjs_import_resolution",
                vec!["tsjs_unknown_kind=dynamic_require".to_string()],
            ),
            unknown_fact_for_unit_with_assumptions(
                &unit,
                UnknownReasonCode::ConflictingFacts,
                "tsjs_reexport_resolution",
                vec!["tsjs_unknown_kind=ambiguous_reexport".to_string()],
            ),
        ];
        let store = FakeStore::new(facts).with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        for mechanism in [
            "typescript_module_resolver",
            "typescript_paths_resolver",
            "typescript_rootdirs_model",
            "typescript_package_entry_model",
            "typescript_commonjs_alias_model",
            "typescript_export_graph",
        ] {
            assert_eq!(
                bucket_count(&report.by_required_mechanism, mechanism),
                1,
                "{mechanism} should be counted once"
            );
        }
    }

    #[test]
    fn unknown_inventory_suppresses_tsjs_unknowns_resolved_by_provider_operation() {
        let unit = indexed_language_unit("src/app.ts", "typescript", "handler", 0);
        let mut unresolved = unknown_fact_for_unit_with_assumptions(
            &unit,
            UnknownReasonCode::UnresolvedImport,
            "tsjs_import_resolution",
            vec!["tsjs_unknown_kind=unresolved_import".to_string()],
        );
        unresolved.origin_engine = "repogrammar-tsjs-syntax".to_string();
        unresolved.origin_method = "bounded_import_resolver_v1".to_string();
        let mut resolved = semantic_fact();
        resolved.kind = "RESOLVED_IMPORT".to_string();
        resolved.certainty = "SEMANTIC".to_string();
        resolved.origin_engine = "typescript".to_string();
        resolved.origin_engine_version = "6.0.0".to_string();
        resolved.origin_method = "compiler_api_module_resolver_v1".to_string();
        resolved.assumptions = vec![
            "provider=typescript".to_string(),
            "provider_resolved=true".to_string(),
            "query_operation=resolve_module_specifier".to_string(),
        ];
        resolved.code_unit_id = unit.id.clone();
        resolved.path = unit.path.clone();
        resolved.content_hash = unit.content_hash.clone();
        resolved.start_byte = unit.start_byte;
        resolved.end_byte = unit.end_byte;
        let store = FakeStore::new(vec![unresolved, resolved]).with_units(vec![unit]);

        let report = unknown_inventory(&store).expect("unknown inventory");

        assert_eq!(report.total_unknowns, 0);
    }

    #[test]
    fn inventory_reports_use_targeted_active_inventory_reads() {
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
        assert_eq!(store.snapshot_reads(), 0);

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
        assert_eq!(store.snapshot_reads(), 0);

        let store = FakeStore::new(vec![semantic_fact()]);
        let report = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(report.facts, vec![semantic_fact()]);
        assert_eq!(store.snapshot_reads(), 0);

        let unit = indexed_unit("src/a.ts");
        let store = FakeStore::new(vec![unknown_fact_for_unit(
            &unit,
            UnknownReasonCode::DynamicImport,
            "dynamic import target cannot be resolved statically",
        )])
        .with_files(vec![indexed_file("src/a.ts")])
        .with_units(vec![unit]);
        let report = unknown_inventory(&store).expect("unknown inventory");
        assert_eq!(report.total_unknowns, 1);
        assert_eq!(store.snapshot_reads(), 0);
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
        assert_eq!(store.show_family_reads(), 0);

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
    fn fuzzy_path_lookup_abstains_when_path_matches_multiple_families() {
        let first = active_family_with_evidence(
            "family:rust:mcp_handler:tool_call",
            "src/rust/interfaces/mcp/mod.rs",
            0,
        );
        let first_family_id = first.family.family_id.clone();
        let first_member_id = first.members[0].code_unit_id.clone();
        let second = active_family_with_evidence(
            "family:rust:mcp_handler:initialize",
            "src/rust/interfaces/mcp/mod.rs",
            1,
        );
        let second_family_id = second.family.family_id.clone();
        let store = FakeFamilyStore::with_families(vec![second, first]);

        let lookup = lookup_family(
            &store,
            Some("src/rust/interfaces/mcp/mod.rs"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup family");
        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("ambiguous path lookup must not return the first family");
        };
        assert_eq!(report.active_generation, "gen-000001");
        assert_eq!(report.unknowns[0].class, UnknownClass::Blocking);
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(report.unknowns[0].affected_claim, "query target ambiguity");
        let recovery = report.unknowns[0]
            .recovery
            .as_deref()
            .expect("ambiguity recovery");
        assert!(recovery.contains(&first_family_id));
        assert!(recovery.contains(&second_family_id));
        assert!(recovery.contains("exact family id or member id"));

        let exact_family = lookup_family(
            &store,
            Some(&first_family_id),
            FamilyLookupMode::ExactFamilyId,
        )
        .expect("exact family lookup");
        let FamilyLookupReport::Found(report) = exact_family else {
            panic!("exact family id must remain exact");
        };
        assert_eq!(report.family_id, first_family_id);

        let exact_member = lookup_family(
            &store,
            Some(&first_member_id),
            FamilyLookupMode::ExactMemberId,
        )
        .expect("exact member lookup");
        let FamilyLookupReport::Found(report) = exact_member else {
            panic!("exact member id must remain exact");
        };
        assert_eq!(report.family_id, first_family_id);
    }

    #[test]
    fn fuzzy_path_lookup_with_freshness_abstains_before_stale_claims_when_ambiguous() {
        let store = FakeFamilyStore::with_families(vec![
            active_family_with_evidence(
                "family:rust:mcp_handler:tool_call",
                "src/rust/interfaces/mcp/mod.rs",
                0,
            ),
            active_family_with_evidence(
                "family:rust:mcp_handler:initialize",
                "src/rust/interfaces/mcp/mod.rs",
                1,
            ),
        ]);
        let source_store = FamilyEvidenceSourceStore {
            path: "src/rust/interfaces/mcp/mod.rs".to_string(),
            result: Err(SourceStoreError::HashMismatch(
                "source content changed after discovery".to_string(),
            )),
        };

        let lookup = lookup_family_with_freshness(
            family_freshness_request(),
            &store,
            &source_store,
            Some("src/rust/interfaces/mcp/mod.rs"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup with freshness");
        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("ambiguous path lookup must not produce a family claim");
        };
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(report.unknowns[0].affected_claim, "query target ambiguity");
    }

    #[test]
    fn fuzzy_role_lookup_abstains_when_candidate_cap_is_truncated() {
        let families = (0..6)
            .map(|index| {
                let mut family = active_family_with_evidence(
                    &format!("family:typescript:express_route:handler_{index}"),
                    &format!("src/routes/{index}.ts"),
                    index,
                );
                family.members[0].role = "framework:express.route_handler".to_string();
                family
            })
            .collect::<Vec<_>>();
        let store = FakeFamilyStore::with_families(families);

        let lookup = lookup_family(
            &store,
            Some("framework:express.route_handler"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup family");

        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("truncated fuzzy candidate set must remain UNKNOWN");
        };
        assert_eq!(
            report.unknowns[0].affected_claim,
            "query target candidate set"
        );
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(
            store.show_family_reads(),
            1,
            "fuzzy cap should not hydrate candidate families after the exact-id probe"
        );
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
            line_range_omissions: Vec::new(),
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
    fn read_plan_line_range_enrichment_is_metadata_only() {
        let source = SourceText {
            path: "src/a.ts".to_string(),
            content_hash: semantic_fact().content_hash,
            text: "first\nsecond\nthird\n".to_string(),
        };
        let read_plan = read_plan_for_source_span(6, 12);

        let enriched = enrich_read_plan_line_ranges(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &StaticSourceStore { result: Ok(source) },
            &read_plan,
        )
        .expect("enrich read plan");

        assert!(!enriched.source_snippets_included);
        assert_eq!(enriched.items[0].start_line, Some(2));
        assert_eq!(enriched.items[0].end_line, Some(2));
        assert!(!enriched.items[0].source_snippets_included);
        assert!(enriched.line_range_omissions.is_empty());
        let debug = format!("{enriched:?}");
        assert!(!debug.contains("second"));
    }

    #[test]
    fn read_plan_line_range_enrichment_keeps_stale_items_with_guidance() {
        let read_plan = read_plan_for_source_span(6, 12);

        let enriched = enrich_read_plan_line_ranges(
            SourceSpanRenderRequest {
                repository_root: "/repo".to_string(),
                max_file_bytes: 1024,
            },
            &hash_mismatch_source_store("hash mismatch"),
            &read_plan,
        )
        .expect("stale read plan is not a transport failure");

        assert_eq!(enriched.items.len(), 1);
        assert_eq!(enriched.items[0].start_line, None);
        assert_eq!(enriched.items[0].end_line, None);
        assert_eq!(enriched.line_range_omissions.len(), 1);
        assert_eq!(enriched.line_range_omissions[0].reason, "stale_evidence");
        assert!(enriched.line_range_omissions[0]
            .guidance
            .contains("Read/Grep"));
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
        let index_store = FakeStore::new(Vec::new()).with_repo_shape_stats(repo_shape_stats(
            4,
            1,
            3,
            3,
            vec![repo_shape_language_stats("python", 4, 1, 3, 3)],
        ));
        let family_store = FakeFamilyStore::empty();

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        assert_eq!(index_store.snapshot_reads(), 0);
        assert_eq!(family_store.show_family_reads(), 0);
        assert_eq!(diagnostics.indexed_file_count, 4);
        assert_eq!(diagnostics.indexed_code_unit_count, 4);
        assert_eq!(diagnostics.semantic_fact_count, 0);
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
            diagnostics.token_saving_readiness,
            TokenSavingReadiness::Partial
        );
        assert!(diagnostics.blocking_reasons.is_empty());
        assert_eq!(
            diagnostics.external_dependency_signal,
            DiagnosticSignal::Unknown
        );
        let python = language_diagnostic(&diagnostics, "python");
        assert_eq!(python.language_scope, "official_v0_1");
        assert_eq!(python.indexed_file_count, 4);
        assert_eq!(python.indexed_code_unit_count, 4);
        assert_eq!(python.eligible_code_units, 4);
        assert_eq!(python.family_count, 1);
        assert_eq!(python.family_member_count, 3);
        assert_eq!(python.covered_code_units, 3);
        assert_eq!(python.family_support_coverage, Some(0.75));
        assert_eq!(python.support_risk, DiagnosticSignal::Low);
        assert_eq!(python.preview_status, "official");
        let tsjs = language_diagnostic(&diagnostics, "typescript/javascript");
        assert_eq!(tsjs.language_scope, "bounded_v0_2_preview");
        assert_eq!(tsjs.eligible_code_units, 0);
        assert_eq!(tsjs.preview_status, "bounded_preview");
    }

    #[test]
    fn repo_shape_by_language_counts_tsx_preview_families() {
        let index_store = FakeStore::new(Vec::new()).with_repo_shape_stats(repo_shape_stats(
            0,
            1,
            0,
            0,
            vec![repo_shape_language_stats(
                "typescript/javascript",
                3,
                1,
                3,
                3,
            )],
        ));
        let family_store = FakeFamilyStore::empty();

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        let tsjs = language_diagnostic(&diagnostics, "typescript/javascript");
        assert_eq!(tsjs.eligible_code_units, 3);
        assert_eq!(tsjs.indexed_file_count, 3);
        assert_eq!(tsjs.indexed_code_unit_count, 3);
        assert_eq!(tsjs.family_count, 1);
        assert_eq!(tsjs.family_member_count, 3);
        assert_eq!(tsjs.covered_code_units, 3);
        assert_eq!(tsjs.family_support_coverage, Some(1.0));
        assert_eq!(tsjs.support_risk, DiagnosticSignal::Low);
        assert_eq!(diagnostics.eligible_code_units, 0);
    }

    #[test]
    fn repo_shape_reports_tsjs_indexed_inventory_without_family_support() {
        let index_store = FakeStore::new(Vec::new()).with_repo_shape_stats(repo_shape_stats(
            0,
            0,
            0,
            0,
            vec![RepoShapeLanguageStats {
                language: "typescript/javascript".to_string(),
                indexed_file_count: 2,
                indexed_code_unit_count: 2,
                eligible_code_units: 0,
                family_count: 0,
                family_member_count: 0,
                covered_code_units: 0,
            }],
        ));
        let family_store = FakeFamilyStore::empty();

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        assert_eq!(diagnostics.indexed_file_count, 2);
        assert_eq!(diagnostics.indexed_code_unit_count, 2);
        assert_eq!(diagnostics.eligible_code_units, 0);
        let tsjs = language_diagnostic(&diagnostics, "typescript/javascript");
        assert_eq!(tsjs.indexed_file_count, 2);
        assert_eq!(tsjs.indexed_code_unit_count, 2);
        assert_eq!(tsjs.eligible_code_units, 0);
        assert_eq!(tsjs.family_count, 0);
        assert_eq!(tsjs.family_support_coverage, None);
        assert_eq!(tsjs.support_risk, DiagnosticSignal::Unknown);
    }

    #[test]
    fn repo_shape_diagnostics_abstains_when_no_eligible_python_units_exist() {
        let index_store = FakeStore::new(Vec::new());
        let family_store = FakeFamilyStore::empty();

        let diagnostics =
            repo_shape_diagnostics(&index_store, &family_store).expect("repo diagnostics");

        assert_eq!(diagnostics.eligible_code_units, 0);
        assert_eq!(diagnostics.local_pattern_density, None);
        assert_eq!(diagnostics.family_support_coverage, None);
        assert_eq!(diagnostics.abstention_rate, None);
        assert_eq!(diagnostics.token_saving_risk, DiagnosticSignal::Unknown);
        assert_eq!(diagnostics.thin_wrapper_risk, DiagnosticSignal::Unknown);
        assert_eq!(
            diagnostics.token_saving_readiness,
            TokenSavingReadiness::Unknown
        );
        assert_eq!(
            diagnostics.blocking_reasons,
            vec!["no_supported_units", "no_families"]
        );
        let python = language_diagnostic(&diagnostics, "python");
        assert_eq!(python.eligible_code_units, 0);
        let tsjs = language_diagnostic(&diagnostics, "typescript/javascript");
        assert_eq!(tsjs.eligible_code_units, 0);
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
    fn family_lookup_matches_embedded_exact_path_without_short_substrings() {
        let store = FakeFamilyStore::with_family();

        let lookup = lookup_family(
            &store,
            Some("src/routes/a.ts process_boundary_diagnostics"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup family");

        let FamilyLookupReport::Found(report) = lookup else {
            panic!("embedded repo-relative path must find the family");
        };
        assert_eq!(report.family_id, "family:typescript:express_route:express");
    }

    #[test]
    fn missing_family_target_returns_partial_local_context_for_resolved_path() {
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("src/routes/a.ts")])
            .with_units(vec![IndexedCodeUnitRecord {
                id: "unit:src/routes/a.ts#process_boundary_diagnostics:0-10".to_string(),
                ..indexed_unit("src/routes/a.ts")
            }]);
        let family_store = FakeFamilyStore::empty();

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("src/routes/a.ts process_boundary_diagnostics timeout"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::PartialContext(report) = lookup else {
            panic!("resolved local target must return partial context");
        };
        assert_eq!(report.resolved_target.path, "src/routes/a.ts");
        assert_eq!(
            report.resolved_target.code_unit_id.as_deref(),
            Some("unit:src/routes/a.ts#process_boundary_diagnostics:0-10")
        );
        assert_eq!(report.resolved_target.kind, "code_unit");
        assert_eq!(report.resolved_target.line, None);
        assert_eq!(report.resolved_target.byte_range, None);
        assert_eq!(
            report.resolved_target.symbol_hints,
            vec!["process_boundary_diagnostics"]
        );
        assert_eq!(report.resolved_target.residue_terms, vec!["timeout"]);
        assert_eq!(
            report.resolved_target.candidate_paths,
            vec!["src/routes/a.ts"]
        );
        assert_eq!(
            report.resolved_target.candidate_code_unit_ids,
            vec!["unit:src/routes/a.ts#process_boundary_diagnostics:0-10"]
        );
        assert_eq!(report.resolved_target.confidence, "high");
        assert_eq!(report.resolved_target.match_kind, "path_embedded");
        assert_eq!(
            report.read_plan.selection_strategy,
            "deterministic_local_context_v1"
        );
        assert_eq!(report.read_plan.items.len(), 1);
        assert_eq!(
            report.read_plan.items[0].purpose,
            ReadPlanPurpose::TargetBodyRequiredForEdit
        );
        assert_eq!(report.read_plan.items[0].path, "src/routes/a.ts");
        assert!(!report.read_plan.source_snippets_included);
        assert!(report.read_plan.requires_source_before_edit);
        assert_eq!(
            report.unknowns[0].affected_claim,
            "pattern family evidence for resolved target"
        );
        assert_eq!(index_store.snapshot_reads(), 0);
        assert_eq!(index_store.indexed_file_reads(), 1);
        assert_eq!(index_store.code_unit_reads(), 1);
    }

    #[test]
    fn partial_local_context_resolves_root_path_plus_symbol_terms() {
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("a.ts")])
            .with_units(vec![IndexedCodeUnitRecord {
                id: "unit:a.ts#helper_symbol:0-10".to_string(),
                ..indexed_unit("a.ts")
            }]);
        let family_store = FakeFamilyStore::empty();

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("a.ts helper_symbol timeout"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::PartialContext(report) = lookup else {
            panic!("root file path plus symbol must return partial context");
        };
        assert_eq!(report.resolved_target.path, "a.ts");
        assert_eq!(
            report.resolved_target.code_unit_id.as_deref(),
            Some("unit:a.ts#helper_symbol:0-10")
        );
        assert_eq!(report.resolved_target.symbol_hints, vec!["helper_symbol"]);
        assert_eq!(report.resolved_target.residue_terms, vec!["timeout"]);
        assert_eq!(report.resolved_target.match_kind, "path_embedded");
    }

    #[test]
    fn partial_local_context_preserves_path_line_and_byte_range_targets() {
        let file = IndexedFileRecord {
            size_bytes: 120,
            ..indexed_file("src/routes/a.ts")
        };
        let first_unit = IndexedCodeUnitRecord {
            id: "unit:src/routes/a.ts#first:0-20".to_string(),
            start_byte: 0,
            end_byte: 20,
            ..indexed_unit("src/routes/a.ts")
        };
        let second_unit = IndexedCodeUnitRecord {
            id: "unit:src/routes/a.ts#second:40-80".to_string(),
            start_byte: 40,
            end_byte: 80,
            ..indexed_unit("src/routes/a.ts")
        };
        let family_store = FakeFamilyStore::empty();
        let range_lookup = lookup_family_with_local_context(
            &FakeStore::new(Vec::new())
                .with_files(vec![file.clone()])
                .with_units(vec![first_unit.clone(), second_unit.clone()]),
            &family_store,
            Some("src/routes/a.ts:45-60"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup byte range");

        let FamilyLookupReport::PartialContext(range_report) = range_lookup else {
            panic!("byte-range target must resolve local context");
        };
        assert_eq!(range_report.resolved_target.byte_range, Some((45, 60)));
        assert_eq!(range_report.resolved_target.line, None);
        assert_eq!(
            range_report.resolved_target.code_unit_id.as_deref(),
            Some("unit:src/routes/a.ts#second:40-80")
        );
        assert_eq!(range_report.resolved_target.match_kind, "path_exact");
        assert_eq!(range_report.resolved_target.confidence, "exact");

        let line_lookup = lookup_family_with_local_context(
            &FakeStore::new(Vec::new())
                .with_files(vec![file])
                .with_units(vec![IndexedCodeUnitRecord {
                    id: "unit:src/routes/a.ts#process_boundary_diagnostics:0-20".to_string(),
                    start_byte: 0,
                    end_byte: 20,
                    ..indexed_unit("src/routes/a.ts")
                }]),
            &family_store,
            Some("src/routes/a.ts:7 process_boundary_diagnostics timeout"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup line target");

        let FamilyLookupReport::PartialContext(line_report) = line_lookup else {
            panic!("line target must resolve local context");
        };
        assert_eq!(line_report.resolved_target.line, Some(7));
        assert_eq!(line_report.resolved_target.byte_range, None);
        assert_eq!(
            line_report.resolved_target.symbol_hints,
            vec!["process_boundary_diagnostics"]
        );
        assert_eq!(line_report.resolved_target.residue_terms, vec!["timeout"]);
    }

    #[test]
    fn partial_local_context_abstains_on_ambiguous_paths() {
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![
                indexed_file("src/routes/a.ts"),
                indexed_file("tests/src/routes/a.ts"),
            ])
            .with_units(vec![
                indexed_unit("src/routes/a.ts"),
                indexed_unit("tests/src/routes/a.ts"),
            ]);
        let family_store = FakeFamilyStore::empty();

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("ambiguous suffix target must remain UNKNOWN");
        };
        assert_eq!(
            report.unknowns[0].affected_claim,
            "query target path ambiguity"
        );
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
    }

    /// Builds a fuzzy candidate set that is deliberately too broad: every family
    /// shares the same evidence path so an evidence-path probe returns more than
    /// `FUZZY_FAMILY_CANDIDATE_LIMIT` matches and blocks with
    /// `query target candidate set`.
    fn broad_shared_path_family_store(path: &str, count: usize) -> FakeFamilyStore {
        let families = (0..count)
            .map(|index| {
                active_family_with_evidence(
                    &format!("family:typescript:express_route:handler_{index}"),
                    path,
                    index,
                )
            })
            .collect::<Vec<_>>();
        FakeFamilyStore::with_families(families)
    }

    #[test]
    fn broad_family_candidate_set_falls_back_to_partial_local_context() {
        // The fuzzy family probe blocks with `query target candidate set`
        // (too many families reference the same path), but the target still
        // resolves to exactly one indexed file/code unit, so the caller earns a
        // bounded local read plan without any family being guessed.
        let family_store = broad_shared_path_family_store("src/routes/shared.ts", 6);
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("src/routes/shared.ts")])
            .with_units(vec![IndexedCodeUnitRecord {
                id: "unit:src/routes/shared.ts#handler:0-10".to_string(),
                ..indexed_unit("src/routes/shared.ts")
            }]);

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("src/routes/shared.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");
        let route = family_query_route_report(&lookup, FamilyLookupMode::FuzzyQuery);

        let FamilyLookupReport::PartialContext(report) = lookup else {
            panic!("broad family candidate set with a unique target must return partial context");
        };
        assert_eq!(report.resolved_target.path, "src/routes/shared.ts");
        assert!(
            report.resolved_target.family_id.is_none(),
            "partial context must not guess a family"
        );
        assert_eq!(
            report.resolved_target.candidate_family_ids,
            vec![
                "family:typescript:express_route:handler_0".to_string(),
                "family:typescript:express_route:handler_1".to_string(),
                "family:typescript:express_route:handler_2".to_string(),
                "family:typescript:express_route:handler_3".to_string(),
                "family:typescript:express_route:handler_4".to_string(),
            ]
        );
        assert_eq!(route.route, "partial_context_read_plan");
        assert!(route.selected_family_id.is_none());
        assert_eq!(
            route.follow_up_family_ids,
            report.resolved_target.candidate_family_ids
        );
        assert_eq!(
            report.unknowns[0].affected_claim,
            "pattern family evidence for resolved target"
        );
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(report.unknowns[0].class, UnknownClass::Blocking);
        assert!(!report.read_plan.source_snippets_included);
    }

    #[test]
    fn ambiguous_family_target_falls_back_to_partial_local_context() {
        // Two families claim the same evidence path, so the fuzzy probe blocks
        // with `query target ambiguity`. Because the path still resolves to one
        // indexed file, the caller earns partial context instead of a dead-end
        // UNKNOWN — the family stays unguessed.
        let family_store = broad_shared_path_family_store("src/routes/a.ts", 2);
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("src/routes/a.ts")])
            .with_units(vec![indexed_unit("src/routes/a.ts")]);

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("src/routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::PartialContext(report) = lookup else {
            panic!("ambiguous family target with a unique path must return partial context");
        };
        assert_eq!(report.resolved_target.path, "src/routes/a.ts");
        assert!(report.resolved_target.family_id.is_none());
        assert_eq!(
            report.unknowns[0].affected_claim,
            "pattern family evidence for resolved target"
        );
    }

    #[test]
    fn broad_family_candidate_set_stays_unknown_when_target_path_is_ambiguous() {
        // Broad family evidence must not lower the local-resolution bar: a suffix
        // target that matches two indexed files stays UNKNOWN and returns no
        // source.
        let family_store = broad_shared_path_family_store("src/routes/a.ts", 6);
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![
                indexed_file("src/routes/a.ts"),
                indexed_file("tests/src/routes/a.ts"),
            ])
            .with_units(vec![
                indexed_unit("src/routes/a.ts"),
                indexed_unit("tests/src/routes/a.ts"),
            ]);

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("ambiguous suffix target must remain UNKNOWN even with broad family evidence");
        };
        assert_eq!(
            report.unknowns[0].affected_claim,
            "query target path ambiguity"
        );
    }

    #[test]
    fn broad_family_candidate_set_stays_unknown_when_target_does_not_resolve() {
        // A broad candidate set with a target that matches no indexed file must
        // preserve the original blocking UNKNOWN — no fabricated local context.
        let family_store = broad_shared_path_family_store("src/routes/shared.ts", 6);
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("src/routes/other.ts")])
            .with_units(vec![indexed_unit("src/routes/other.ts")]);

        let lookup = lookup_family_with_local_context(
            &index_store,
            &family_store,
            Some("src/routes/shared.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup local context");

        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("unresolvable target must remain UNKNOWN");
        };
        assert_eq!(
            report.unknowns[0].affected_claim,
            "query target candidate set"
        );
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
    }

    #[test]
    fn exact_lookup_modes_never_fall_back_to_local_context() {
        // Exact-family and exact-member lookups must keep their strict UNKNOWN
        // contract: even when the same target would resolve locally under fuzzy
        // mode, the exact modes never return partial context.
        let index_store = FakeStore::new(Vec::new())
            .with_files(vec![indexed_file("src/routes/a.ts")])
            .with_units(vec![indexed_unit("src/routes/a.ts")]);
        let family_store = FakeFamilyStore::empty();

        for mode in [
            FamilyLookupMode::ExactFamilyId,
            FamilyLookupMode::ExactMemberId,
        ] {
            let lookup = lookup_family_with_local_context(
                &index_store,
                &family_store,
                Some("src/routes/a.ts"),
                mode,
            )
            .expect("lookup local context");
            assert!(
                matches!(lookup, FamilyLookupReport::Unknown(_)),
                "exact lookup mode must not fall back to partial local context"
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
            Some("run repogrammar resync")
        );

        let lookup = lookup_family_with_freshness(
            family_freshness_request(),
            &store,
            &source_store,
            Some("routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
        )
        .expect("lookup with freshness");
        let FamilyLookupReport::Unknown(report) = lookup else {
            panic!("stale fuzzy path evidence must block public detail");
        };
        assert_eq!(report.unknowns[0].reason, UnknownReasonCode::StaleEvidence);

        // The families listing keeps the stale family visible but qualifies it:
        // the entry carries a `Stale` verdict, the rollup counts it, and a
        // low-cardinality stale-evidence unknown recovers via resync.
        let families =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");
        assert_eq!(families.families.len(), 1);
        assert_eq!(families.families[0].freshness, Some(FamilyFreshness::Stale));
        assert_eq!(
            families.freshness_counts,
            Some(FamilyFreshnessCounts {
                fresh_count: 0,
                stale_count: 1,
                cannot_verify_count: 0,
            })
        );
        assert_eq!(families.unknowns.len(), 1);
        assert_eq!(
            families.unknowns[0].reason,
            UnknownReasonCode::StaleEvidence
        );
        assert_eq!(
            families.unknowns[0].recovery.as_deref(),
            Some("run repogrammar resync")
        );
    }

    fn freshness_hash(nibble: char) -> ContentHash {
        ContentHash::new(format!("sha256:{}", nibble.to_string().repeat(64)))
            .expect("valid content hash")
    }

    fn family_with_evidence(family_id: &str, evidence: &[(&str, ContentHash)]) -> ActiveFamily {
        ActiveFamily {
            generation_id: "gen-000001".to_string(),
            family: IndexedFamilyRecord {
                family_id: family_id.to_string(),
                classification: "DOMINANT_PATTERN".to_string(),
                prevalence: crate::test_support::sample_family_prevalence(),
            },
            members: vec![IndexedFamilyMemberRecord {
                family_id: family_id.to_string(),
                code_unit_id: format!("unit:{family_id}#member:0-1"),
                role: "framework:fastapi.route".to_string(),
            }],
            variation_slots: Vec::new(),
            evidence: evidence
                .iter()
                .enumerate()
                .map(|(index, (path, hash))| IndexedFamilyEvidenceRecord {
                    evidence_id: format!("family-evidence:{index:06}"),
                    family_id: family_id.to_string(),
                    code_unit_id: format!("unit:{path}#member:0-1"),
                    covered_claims: vec!["canonical".to_string()],
                    path: (*path).to_string(),
                    content_hash: hash.clone(),
                    start_byte: 0,
                    end_byte: 1,
                    note: "support evidence".to_string(),
                })
                .collect(),
        }
    }

    /// Source-store double that records every read and resolves each path to an
    /// on-disk hash or an explicit read failure, mirroring the filesystem store's
    /// contract (missing path, hash mismatch, and non-content read errors).
    struct CountingSourceStore {
        results: BTreeMap<String, Result<ContentHash, SourceStoreError>>,
        reads: std::cell::Cell<usize>,
    }

    impl CountingSourceStore {
        fn new(results: &[(&str, Result<ContentHash, SourceStoreError>)]) -> Self {
            Self {
                results: results
                    .iter()
                    .map(|(path, result)| ((*path).to_string(), result.clone()))
                    .collect(),
                reads: std::cell::Cell::new(0),
            }
        }

        fn reads(&self) -> usize {
            self.reads.get()
        }
    }

    impl SourceStore for CountingSourceStore {
        fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
            self.reads.set(self.reads.get() + 1);
            match self.results.get(&request.path) {
                None => Err(SourceStoreError::Missing(format!(
                    "source is missing: {}",
                    request.path
                ))),
                Some(Err(error)) => Err(error.clone()),
                Some(Ok(actual)) if actual == &request.expected_content_hash => Ok(SourceText {
                    path: request.path,
                    content_hash: actual.clone(),
                    text: "source".to_string(),
                }),
                Some(Ok(_)) => Err(SourceStoreError::HashMismatch(format!(
                    "source content changed after discovery: {}",
                    request.path
                ))),
            }
        }
    }

    #[test]
    fn families_listing_marks_missing_evidence_family_stale() {
        let store = FakeFamilyStore::with_families(vec![
            family_with_evidence(
                "family:python:fastapi_route:a",
                &[("src/a.py", freshness_hash('a'))],
            ),
            family_with_evidence(
                "family:python:fastapi_route:b",
                &[("src/b.py", freshness_hash('a'))],
            ),
        ]);
        // src/a.py is gone from the tree; src/b.py still matches its indexed hash.
        let source_store = CountingSourceStore::new(&[("src/b.py", Ok(freshness_hash('a')))]);

        let report =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");

        assert_eq!(report.families.len(), 2);
        assert_eq!(
            report.families[0].family_id,
            "family:python:fastapi_route:a"
        );
        assert_eq!(report.families[0].freshness, Some(FamilyFreshness::Stale));
        assert_eq!(report.families[1].freshness, Some(FamilyFreshness::Fresh));
        assert_eq!(
            report.freshness_counts,
            Some(FamilyFreshnessCounts {
                fresh_count: 1,
                stale_count: 1,
                cannot_verify_count: 0,
            })
        );
        assert_eq!(report.unknowns.len(), 1);
        assert_eq!(report.unknowns[0].reason, UnknownReasonCode::StaleEvidence);
    }

    #[test]
    fn families_listing_marks_unreadable_evidence_cannot_verify() {
        let store = FakeFamilyStore::with_families(vec![family_with_evidence(
            "family:python:fastapi_route:big",
            &[("src/big.py", freshness_hash('a'))],
        )]);
        // A non-content read failure cannot decide freshness or staleness.
        let source_store = CountingSourceStore::new(&[(
            "src/big.py",
            Err(SourceStoreError::TooLarge("too large".to_string())),
        )]);

        let report =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");

        assert_eq!(
            report.families[0].freshness,
            Some(FamilyFreshness::CannotVerify)
        );
        assert_eq!(
            report.freshness_counts,
            Some(FamilyFreshnessCounts {
                fresh_count: 0,
                stale_count: 0,
                cannot_verify_count: 1,
            })
        );
        // Cannot-verify is not stale, so no report-level stale signal is raised.
        assert!(report.unknowns.is_empty());
    }

    #[test]
    fn families_listing_abstains_for_evidence_less_family() {
        let store = FakeFamilyStore::with_families(vec![family_with_evidence(
            "family:python:fastapi_route:bare",
            &[],
        )]);
        let source_store = CountingSourceStore::new(&[]);

        let report =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");

        assert_eq!(
            report.families[0].freshness,
            Some(FamilyFreshness::CannotVerify)
        );
        assert_eq!(
            report.freshness_counts,
            Some(FamilyFreshnessCounts {
                fresh_count: 0,
                stale_count: 0,
                cannot_verify_count: 1,
            })
        );
        // No evidence rows means no source reads.
        assert_eq!(source_store.reads(), 0);
    }

    #[test]
    fn families_listing_verifies_each_distinct_path_once() {
        let hash = freshness_hash('a');
        let store = FakeFamilyStore::with_families(vec![
            family_with_evidence(
                "family:python:fastapi_route:a",
                &[("src/shared.py", hash.clone()), ("src/a.py", hash.clone())],
            ),
            family_with_evidence(
                "family:python:fastapi_route:b",
                &[("src/shared.py", hash.clone()), ("src/b.py", hash.clone())],
            ),
            family_with_evidence(
                "family:python:fastapi_route:c",
                &[("src/c.py", hash.clone())],
            ),
        ]);
        // Every path resolves fresh. Distinct evidence paths: shared, a, b, c = 4.
        // Sum over families would be 5; a bounded check must read exactly 4.
        let source_store = CountingSourceStore::new(&[
            ("src/shared.py", Ok(hash.clone())),
            ("src/a.py", Ok(hash.clone())),
            ("src/b.py", Ok(hash.clone())),
            ("src/c.py", Ok(hash.clone())),
        ]);

        let report =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");

        assert!(report
            .families
            .iter()
            .all(|family| family.freshness == Some(FamilyFreshness::Fresh)));
        assert_eq!(
            report.freshness_counts,
            Some(FamilyFreshnessCounts {
                fresh_count: 3,
                stale_count: 0,
                cannot_verify_count: 0,
            })
        );
        assert_eq!(source_store.reads(), 4);
    }

    #[test]
    fn families_listing_stays_empty_without_source_reads() {
        let store = FakeFamilyStore::empty();
        let source_store = CountingSourceStore::new(&[]);

        let report =
            list_families_with_freshness(family_freshness_request(), &store, &source_store)
                .expect("list families with freshness");

        assert!(report.families.is_empty());
        assert!(report.freshness_counts.is_none());
        assert_eq!(report.unknowns.len(), 1);
        assert_eq!(
            report.unknowns[0].reason,
            UnknownReasonCode::InsufficientSupport
        );
        assert_eq!(source_store.reads(), 0);
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
        assert_eq!(unknown.recovery.as_deref(), Some("run repogrammar resync"));
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
        assert_eq!(unknown.recovery.as_deref(), Some("run repogrammar resync"));
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
