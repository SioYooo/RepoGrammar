//! Query use-case boundary for finding repository analogues.

use crate::application::family::{
    classify_unknown_family_effect, FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX,
};
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
    ActiveClaimInputSnapshot, IndexStore, IndexStoreError, IndexedCodeUnitRecord,
    IndexedFileRecord, IndexedSemanticFactRecord,
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
            "run repogrammar init --yes",
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
                    fallback("no active index generation", "run repogrammar resync", true)
                }
                QueryPreflightOperation::ActiveIndexInventory => QueryPreflightReport::Ready,
                QueryPreflightOperation::PatternFamilyQuery
                    if active_generation == "none"
                        || active_generation == "not implemented"
                        || !inventory_indexing_is_readable(status_report.indexing) =>
                {
                    fallback(
                        "no active index generation",
                        "run repogrammar resync",
                        false,
                    )
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyLookupReport {
    Found(FamilyDetailReport),
    PartialContext(Box<FamilyPartialContextReport>),
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
    pub token_saving_readiness: TokenSavingReadiness,
    pub blocking_reasons: Vec<&'static str>,
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
    pub by_reason_code: Vec<UnknownInventoryBucket>,
    pub by_required_mechanism: Vec<UnknownInventoryBucket>,
    pub by_framework_role: Vec<UnknownInventoryBucket>,
    pub by_role_state: Vec<UnknownInventoryBucket>,
    pub by_blocks_support: Vec<UnknownInventoryBlocksSupportBucket>,
    pub by_recovery_code: Vec<UnknownInventoryBucket>,
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

pub fn unknown_inventory(
    store: &impl IndexStore,
) -> Result<UnknownInventoryReport, RepoGrammarError> {
    let snapshot = store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    Ok(build_unknown_inventory(snapshot))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnknownInventoryEntry {
    language: String,
    reason: UnknownReasonCode,
    class: UnknownClass,
    required_mechanism: String,
    framework_role: String,
    role_state: UnknownInventoryRoleState,
    blocks_support: bool,
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

fn build_unknown_inventory(snapshot: ActiveClaimInputSnapshot) -> UnknownInventoryReport {
    let unit_by_id = snapshot
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let file_language_by_path = snapshot
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.language.as_str()))
        .collect::<BTreeMap<_, _>>();
    let roles_by_unit = inventory_framework_roles_by_unit(&snapshot.semantic_facts);
    let mut entries = snapshot
        .semantic_facts
        .iter()
        .filter(|fact| inventory_fact_is_unknown(fact))
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
            let family_effect = classify_unknown_family_effect(
                language,
                reason,
                &affected_claim,
                (role_state == UnknownInventoryRoleState::Single)
                    .then_some(framework_role.as_str()),
                &fact.origin_engine,
                &fact.origin_method,
            );
            let explicit_class = assumption_value(&fact.assumptions, "unknown_class")
                .and_then(|class| UnknownClass::parse_protocol_str(&class).ok());
            let class = family_effect
                .as_ref()
                .map(|unknown| unknown.class)
                .or(explicit_class)
                .unwrap_or_else(|| default_unknown_class(reason));
            let blocks_support = role_state == UnknownInventoryRoleState::Ambiguous
                || family_effect
                    .as_ref()
                    .is_some_and(|unknown| unknown.class == UnknownClass::Blocking)
                || class == UnknownClass::Blocking;
            let required_mechanism =
                required_unknown_mechanism(language, reason, &affected_claim, &framework_role);
            let recovery_code = unknown_recovery_code(reason, &required_mechanism);
            UnknownInventoryEntry {
                language: language.to_string(),
                reason,
                class,
                required_mechanism,
                framework_role,
                role_state,
                blocks_support,
                recovery_code,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        (
            left.language.as_str(),
            left.reason.as_protocol_str(),
            left.class.as_protocol_str(),
            left.required_mechanism.as_str(),
            left.framework_role.as_str(),
            left.role_state.as_str(),
            left.blocks_support,
            left.recovery_code,
        )
            .cmp(&(
                right.language.as_str(),
                right.reason.as_protocol_str(),
                right.class.as_protocol_str(),
                right.required_mechanism.as_str(),
                right.framework_role.as_str(),
                right.role_state.as_str(),
                right.blocks_support,
                right.recovery_code,
            ))
    });

    UnknownInventoryReport {
        inventory_scope: UNKNOWN_INVENTORY_SCOPE,
        active_generation: snapshot.generation_id,
        total_unknowns: entries.len(),
        blocking_unknowns: count_unknown_class(&entries, UnknownClass::Blocking),
        non_blocking_unknowns: count_unknown_class(&entries, UnknownClass::NonBlocking),
        recoverable_unknowns: count_unknown_class(&entries, UnknownClass::Recoverable),
        irreducible_unknowns: count_unknown_class(&entries, UnknownClass::Irreducible),
        by_language: aggregate_unknown_bucket(&entries, |entry| entry.language.as_str()),
        by_reason_code: aggregate_unknown_bucket(&entries, |entry| entry.reason.as_protocol_str()),
        by_required_mechanism: aggregate_unknown_bucket(&entries, |entry| {
            entry.required_mechanism.as_str()
        }),
        by_framework_role: aggregate_unknown_bucket(&entries, |entry| {
            entry.framework_role.as_str()
        }),
        by_role_state: aggregate_unknown_bucket(&entries, |entry| entry.role_state.as_str()),
        by_blocks_support: aggregate_blocks_support(&entries),
        by_recovery_code: aggregate_unknown_bucket(&entries, |entry| entry.recovery_code),
    }
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
    } else if language == "rust" {
        "rust_family_membership"
    } else {
        "semantic_fact"
    }
}

fn default_unknown_class(reason: UnknownReasonCode) -> UnknownClass {
    match reason {
        UnknownReasonCode::MonkeyPatch => UnknownClass::Irreducible,
        _ => UnknownClass::Recoverable,
    }
}

fn required_unknown_mechanism(
    language: &str,
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> String {
    if let Some(mechanism) =
        claim_specific_required_unknown_mechanism(language, affected_claim, framework_role)
    {
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

fn claim_specific_required_unknown_mechanism(
    language: &str,
    affected_claim: &str,
    framework_role: &str,
) -> Option<&'static str> {
    if language == "python" {
        return match affected_claim {
            "python_import_resolution" => Some("python_import_graph"),
            "pytest_fixture_binding" => Some("pytest_fixture_graph"),
            "fastapi_dependency_target" => Some("fastapi_dependency_graph"),
            _ if framework_role.starts_with("framework:pytest")
                && affected_claim.contains("fixture") =>
            {
                Some("pytest_fixture_graph")
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
            "tsjs_path_alias" | "tsjs_import_resolution" => Some("typescript_paths_resolver"),
            "tsjs_reexport_resolution"
            | "next_default_export"
            | "next_pages_api_export"
            | "next_route_handler_export" => Some("typescript_export_graph"),
            "fastify_receiver_binding" | "fastify_route_shape" | "fastify_route_method" => {
                Some("fastify_receiver_model")
            }
            "prisma_client_binding" | "prisma_query_shape" | "prisma_transaction_shape" => {
                Some("prisma_client_model")
            }
            "drizzle_schema_table"
            | "drizzle_table_binding"
            | "drizzle_query_shape"
            | "drizzle_db_binding"
            | "drizzle_transaction_shape" => Some("drizzle_db_model"),
            _ => None,
        };
    }

    if language == "rust" {
        return match affected_claim {
            "rust_module_resolution" => Some("rust_module_graph"),
            "rust_build_variant" => Some("cargo_feature_cfg_model"),
            "rust_macro_expansion" => Some("rust_macro_boundary"),
            "rust_trait_dispatch" => Some("rust_trait_dispatch_model"),
            _ => None,
        };
    }

    if language == "java" {
        return match affected_claim {
            "java_spring_route_path" => Some("java_spring_route_literal_model"),
            "java_spring_component_scan" => Some("spring_component_scan_model"),
            "java_spring_dependency_injection" => Some("spring_di_model"),
            "java_spring_proxy_semantics" => Some("spring_proxy_model"),
            "java_spring_generated_repository" => Some("spring_data_repository_model"),
            _ => None,
        };
    }

    None
}

fn unknown_recovery_code(reason: UnknownReasonCode, required_mechanism: &str) -> &'static str {
    match reason {
        UnknownReasonCode::StaleEvidence => "run_sync",
        UnknownReasonCode::MissingProjectConfig => "add_project_config",
        UnknownReasonCode::MissingDependency => "resolve_dependency_metadata",
        UnknownReasonCode::UnresolvedImport | UnknownReasonCode::DynamicImport => {
            "resolve_import_graph"
        }
        UnknownReasonCode::PytestFixtureInjection => "resolve_fixture_graph",
        UnknownReasonCode::RuntimeDependencyInjection => {
            if required_mechanism == "pytest_fixture_graph" {
                "resolve_fixture_graph"
            } else {
                "enable_provider"
            }
        }
        UnknownReasonCode::FrameworkMagic => match required_mechanism {
            "python_import_graph"
            | "typescript_paths_resolver"
            | "typescript_export_graph"
            | "rust_module_graph"
            | "java_project_graph" => "resolve_import_graph",
            "pytest_fixture_graph" => "resolve_fixture_graph",
            "fastapi_dependency_graph"
            | "fastify_receiver_model"
            | "prisma_client_model"
            | "drizzle_db_model"
            | "java_spring_route_literal_model"
            | "spring_component_scan_model"
            | "spring_di_model"
            | "spring_proxy_model"
            | "spring_data_repository_model" => "enable_provider",
            "cargo_feature_cfg_model" | "rust_macro_boundary" | "rust_trait_dispatch_model" => {
                "manual_review_required"
            }
            _ => "manual_review_required",
        },
        UnknownReasonCode::MacroOrPreprocessor | UnknownReasonCode::BuildVariantAmbiguity => {
            "manual_review_required"
        }
        UnknownReasonCode::MonkeyPatch => "runtime_trace_required",
        UnknownReasonCode::ConflictingFacts | UnknownReasonCode::InsufficientSupport => {
            "manual_review_required"
        }
    }
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
        *counts.entry(entry.blocks_support).or_default() += 1;
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
        token_saving_readiness,
        blocking_reasons,
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
    let active_generation = active.generation_id;
    let mut matches = Vec::new();
    for family in active.families {
        let Some(active_family) = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        if let Some(target_match) = family_target_match(&active_family, target, mode) {
            matches.push(FamilyTargetMatch {
                family: active_family,
                target_match,
            });
        }
    }
    if let Some(unknown) = ambiguous_target_unknown(&matches) {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            unknowns: vec![unknown],
        }));
    }
    if let Some(matched) = matches.into_iter().next() {
        return Ok(FamilyLookupReport::Found(family_detail(matched.family)));
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation,
        unknowns: vec![insufficient_support_unknown("query target")],
    }))
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
    let active = store.list_active_families().map_err(family_store_error)?;
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: active.generation_id,
            unknowns: vec![insufficient_support_unknown("query target")],
        }));
    };
    let active_generation = active.generation_id;
    let mut matches = Vec::new();
    for family in active.families {
        let Some(active_family) = store
            .show_family(&family.family_id)
            .map_err(family_store_error)?
        else {
            continue;
        };
        let Some(target_match) = family_target_match(&active_family, target, mode) else {
            continue;
        };
        matches.push(FamilyTargetMatch {
            family: active_family,
            target_match,
        });
    }
    if let Some(unknown) = ambiguous_target_unknown(&matches) {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            unknowns: vec![unknown],
        }));
    }
    if let Some(matched) = matches.into_iter().next() {
        if family_evidence_is_fresh(&request, source_store, &matched.family)? {
            return Ok(FamilyLookupReport::Found(family_detail(matched.family)));
        }
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation,
            unknowns: vec![stale_evidence_unknown(format!(
                "{}:evidence_freshness",
                matched.family.family.family_id
            ))],
        }));
    }
    Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
        active_generation,
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
    if !is_query_target_insufficient_support(&unknown_report) {
        return Ok(FamilyLookupReport::Unknown(unknown_report));
    }
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        return Ok(FamilyLookupReport::Unknown(unknown_report));
    };
    let snapshot = index_store
        .load_active_claim_input_snapshot()
        .map_err(index_store_error)?;
    if snapshot.generation_id != unknown_report.active_generation {
        return Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: unknown_report.active_generation,
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::Blocking,
                reason: UnknownReasonCode::StaleEvidence,
                affected_claim: "query target resolution".to_string(),
                recovery: Some("rerun repogrammar resync before using local context".to_string()),
            }],
        }));
    }
    match resolve_local_context(target, &snapshot.files, &snapshot.units)? {
        LocalContextResolution::Resolved(report) => Ok(FamilyLookupReport::PartialContext(
            Box::new(FamilyPartialContextReport {
                active_generation: snapshot.generation_id,
                resolved_target: report.resolved_target,
                read_plan: report.read_plan,
                unknowns: vec![FamilyQueryUnknown {
                    class: UnknownClass::Blocking,
                    reason: UnknownReasonCode::InsufficientSupport,
                    affected_claim: "pattern family evidence for resolved target".to_string(),
                    recovery: Some(
                        "treat this as source-reading context only; rerun repogrammar resync after compatible family evidence exists"
                            .to_string(),
                    ),
                }],
            }),
        )),
        LocalContextResolution::Ambiguous(unknown) => Ok(FamilyLookupReport::Unknown(
            FamilyUnknownReport {
                active_generation: snapshot.generation_id,
                unknowns: vec![unknown],
            },
        )),
        LocalContextResolution::Unresolved => Ok(FamilyLookupReport::Unknown(unknown_report)),
    }
}

fn is_query_target_insufficient_support(report: &FamilyUnknownReport) -> bool {
    matches!(
        report.unknowns.as_slice(),
        [FamilyQueryUnknown {
            class: UnknownClass::Blocking,
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim,
            ..
        }] if affected_claim == "query target"
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

struct FamilyTargetMatch {
    family: ActiveFamily,
    target_match: TargetMatchKind,
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
        .filter(|matched| matched.target_match == TargetMatchKind::EvidencePath)
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
        recovery: Some(
            "run repogrammar resync after adding compatible implementations".to_string(),
        ),
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
        families: Vec<ActiveFamily>,
    }

    impl FakeFamilyStore {
        fn empty() -> Self {
            Self {
                active: ActiveFamilies {
                    generation_id: "gen-000001".to_string(),
                    families: Vec::new(),
                },
                families: Vec::new(),
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
        fact.origin_engine = unit.language.clone();
        fact.origin_method = if unit.language == "python" {
            "cpython_ast".to_string()
        } else {
            "syntax_anchor".to_string()
        };
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
            families: vec![family],
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
        assert_eq!(pattern.guidance, "run repogrammar init --yes");
        assert!(!pattern.implemented);

        let inventory = fallback_report(query_preflight(
            QueryPreflightOperation::ActiveIndexInventory,
            &status,
        ));
        assert_eq!(inventory.reason, "repository is not initialized");
        assert_eq!(inventory.guidance, "run repogrammar init --yes");
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
            diagnostics.token_saving_readiness,
            TokenSavingReadiness::Partial
        );
        assert!(diagnostics.blocking_reasons.is_empty());
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
        assert_eq!(
            diagnostics.token_saving_readiness,
            TokenSavingReadiness::Unknown
        );
        assert_eq!(
            diagnostics.blocking_reasons,
            vec!["no_supported_units", "no_families"]
        );
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
