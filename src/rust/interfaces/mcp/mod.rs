//! Transport-neutral MCP contract and read-only JSON-RPC stdio handling.

use crate::application::conformance::AlignmentComputation;
use crate::application::install::AGENT_PREFLIGHT_GATE;
use crate::application::query::{
    bounded_family_members, build_read_plan, estimate_alignment_potential_token_savings,
    estimate_family_output_potential_token_savings,
    estimate_partial_context_potential_token_savings, family_query_route_report,
    family_query_unknown_metric, found_outcome_token_savings, product_readiness_value,
    query_preflight, read_plan_with_rendered_spans, repository_status_unavailable_fallback,
    select_family_evidence, validate_query_target, validate_query_token_budget,
    AlignmentCertificateReport, FamilyDetailReport, FamilyEvidenceMode, FamilyLookupMode,
    FamilyLookupReport, FamilyOutputOptions, FamilyPartialContextReport, FamilyQueryRouteReport,
    FamilyQueryUnknown, OutcomeTokenSavings, ProductReadinessReport, QueryPreflightOperation,
    QueryPreflightReport, ReadPlan, ReadPlanItem, ReadPlanLineRangeOmission, ResolvedQueryTarget,
    SourceSpanRenderReport, TermRetrievalAbstention, TermRetrievalRoute, Verbosity,
    MAX_QUERY_TARGET_BYTES, MAX_QUERY_TOKEN_BUDGET, PRODUCT_SCHEMA_VERSION,
};
#[cfg(test)]
use crate::application::query::{
    ReadPlanPurpose, RenderedSourceSpan, SourceSpanOmission, SourceSpanPolicy,
};
use crate::application::repository::{RepositoryStatusReport, RepositoryStatusRequest};
use crate::application::telemetry::{
    record_estimated_potential_token_savings, record_family_query_outcome,
    FamilyQueryCommandCategory, FamilyQueryEntrypoint, FamilyQueryLookupMode,
    FamilyQueryOutcomeRecord, FamilyQueryOutcomeStatus,
};
use crate::core::model::{
    EstimatedPotentialTokenSavings, FamilyConstraintProfile, FamilyPrevalence, FeatureConstraint,
    MeasurementKind,
};
use crate::error::RepoGrammarError;
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_LINE_BYTES: usize = 1_048_576;
pub const MAX_MCP_TARGET_BYTES: usize = MAX_QUERY_TARGET_BYTES;
pub const MAX_MCP_TOKEN_BUDGET: usize = MAX_QUERY_TOKEN_BUDGET;
pub const MCP_AGENT_INSTRUCTIONS: &str = AGENT_PREFLIGHT_GATE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolName {
    Context,
}

impl McpToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Context => "repogrammar_context",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpOperation {
    FindAnalogues,
    ShowFamily,
    ExplainDeviation,
    CheckConformance,
    InspectReadiness,
}

impl McpOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "find_analogues" => Some(Self::FindAnalogues),
            "show_family" => Some(Self::ShowFamily),
            "explain_deviation" => Some(Self::ExplainDeviation),
            "check_conformance" => Some(Self::CheckConformance),
            "inspect_readiness" => Some(Self::InspectReadiness),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FindAnalogues => "find_analogues",
            Self::ShowFamily => "show_family",
            Self::ExplainDeviation => "explain_deviation",
            Self::CheckConformance => "check_conformance",
            Self::InspectReadiness => "inspect_readiness",
        }
    }

    fn cli_command(self) -> &'static str {
        match self {
            Self::FindAnalogues => "find",
            Self::ShowFamily => "family",
            Self::ExplainDeviation => "explain",
            Self::CheckConformance => "check",
            Self::InspectReadiness => "status",
        }
    }
}

pub trait McpReadOnlyRuntime {
    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError>;

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError>;

    /// Assemble the decomposed product readiness report. Read-only and
    /// source-free, like `repository_status`.
    fn product_readiness(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<ProductReadinessReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("readiness"))
    }

    fn render_source_spans(
        &self,
        _request: RepositoryStatusRequest,
        _read_plan: &ReadPlan,
        _include_source_spans: bool,
        _token_budget: Option<usize>,
    ) -> Result<SourceSpanRenderReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("source spans"))
    }

    fn enrich_read_plan_line_ranges(
        &self,
        _request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
    ) -> Result<ReadPlan, RepoGrammarError> {
        Ok(read_plan.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServeContext {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProtocolError {
    code: i64,
    message: String,
}

impl McpProtocolError {
    fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: message.into(),
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
        }
    }

    fn method_not_found(message: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: message.into(),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    pub fn code(&self) -> i64 {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpJsonRpcOutcome {
    pub response: Option<Value>,
    pub should_shutdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContextArguments {
    operation: McpOperation,
    target: Option<String>,
    output_options: FamilyOutputOptions,
    include_source_spans: bool,
}

pub fn tool_schema() -> Value {
    json!({
        "name": McpToolName::Context.as_str(),
        "description": "Read-only RepoGrammar pattern-family context. For find_analogues, explain_deviation, and check_conformance, pass the path, symbol/member id, framework role, or pattern question you have; RepoGrammar discovers candidate families internally and returns family ids as follow-up handles. Use show_family only with an exact family id. Use inspect_readiness (no target) for a bounded, source-free capability report: a summary token, per-dimension states (repository, active index, family evidence freshness, prevalence, retrieval, static alignment, providers, autosync, measurement), the top blocking-unknown mechanisms, and one executable recovery action. Start with compact mode and do not request include_source_spans by default. Use read_plan before editing, and fall back on UNKNOWN/FALLBACK/stale/omitted/insufficient results. Do not run repogrammar stats in normal agent loops. No silent setup; run setup/resync/autosync only when user or project policy permits machine integration and repo-local analysis state.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": [
                        McpOperation::FindAnalogues.as_str(),
                        McpOperation::ShowFamily.as_str(),
                        McpOperation::ExplainDeviation.as_str(),
                        McpOperation::CheckConformance.as_str(),
                        McpOperation::InspectReadiness.as_str(),
                    ],
                },
                "target": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_MCP_TARGET_BYTES,
                },
                "token_budget": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_MCP_TOKEN_BUDGET,
                },
                "mode": {
                    "type": "string",
                    "enum": ["compact", "evidence", "deep"],
                },
                "verbosity": {
                    "type": "string",
                    "enum": [
                        Verbosity::Minimal.as_str(),
                        Verbosity::Standard.as_str(),
                        Verbosity::Full.as_str(),
                    ],
                    "description": "Response field density. Additive under product-schemas.v1: `standard` (default) is the current shape, `minimal` opts into the lean shape, `full` retains every diagnostic field. Orthogonal to `mode`.",
                },
                "include_variations": {
                    "type": "boolean",
                },
                "include_exceptions": {
                    "type": "boolean",
                },
                "include_source_spans": {
                    "type": "boolean",
                    "description": "Explicit opt-in for bounded, line-numbered, hash-checked source spans selected from the read_plan. Defaults to false.",
                },
            },
        },
    })
}

pub fn handle_context_call(
    runtime: &impl McpReadOnlyRuntime,
    context: &McpServeContext,
    arguments: &Value,
) -> Result<Value, McpProtocolError> {
    let arguments = parse_context_arguments(arguments)?;
    let request = RepositoryStatusRequest {
        path: context.repository_root.clone(),
        state_dir_override: context.state_dir_override.clone(),
    };

    // `inspect_readiness` is a read-only, source-free capability inspection. It
    // does not run the query preflight or family lookup; it returns the decomposed
    // readiness report, or a clean fallback when readiness cannot be assembled.
    if arguments.operation == McpOperation::InspectReadiness {
        return Ok(match runtime.product_readiness(request) {
            Ok(readiness) => inspect_readiness_value(&readiness),
            Err(_) => fallback_value(
                arguments.operation,
                repository_status_unavailable_fallback(QueryPreflightOperation::PatternFamilyQuery),
            ),
        });
    }

    let status_report = match runtime.repository_status(request.clone()) {
        Ok(report) => report,
        Err(_) => {
            record_mcp_family_query_fallback(
                request,
                arguments.operation,
                arguments.include_source_spans,
            );
            return Ok(fallback_value(
                arguments.operation,
                repository_status_unavailable_fallback(QueryPreflightOperation::PatternFamilyQuery),
            ));
        }
    };

    match query_preflight(QueryPreflightOperation::PatternFamilyQuery, &status_report) {
        QueryPreflightReport::Fallback(fallback) => {
            record_mcp_family_query_fallback(
                request,
                arguments.operation,
                arguments.include_source_spans,
            );
            Ok(fallback_value(arguments.operation, fallback))
        }
        QueryPreflightReport::Ready => match runtime.family_lookup(
            request.clone(),
            arguments.target.as_deref(),
            lookup_mode_for_operation(arguments.operation),
        ) {
            Ok(FamilyLookupReport::Found(family)) => {
                let route = family_query_route_report(
                    &FamilyLookupReport::Found(family.clone()),
                    lookup_mode_for_operation(arguments.operation),
                );
                let base_read_plan = build_read_plan(
                    &family,
                    arguments.target.as_deref(),
                    lookup_mode_for_operation(arguments.operation),
                    arguments.output_options,
                );
                let mut read_plan =
                    match runtime.enrich_read_plan_line_ranges(request.clone(), &base_read_plan) {
                        Ok(read_plan) => read_plan,
                        Err(_) => {
                            record_mcp_family_query_fallback(
                                request,
                                arguments.operation,
                                arguments.include_source_spans,
                            );
                            return Ok(fallback_value(
                                arguments.operation,
                                repository_status_unavailable_fallback(
                                    QueryPreflightOperation::PatternFamilyQuery,
                                ),
                            ));
                        }
                    };
                let source_spans = if arguments.include_source_spans {
                    match runtime.render_source_spans(
                        request.clone(),
                        &read_plan,
                        true,
                        arguments.output_options.token_budget,
                    ) {
                        Ok(source_spans) => {
                            read_plan = read_plan_with_rendered_spans(&read_plan, &source_spans);
                            Some(source_spans)
                        }
                        Err(_) => {
                            record_mcp_family_query_fallback(
                                request,
                                arguments.operation,
                                arguments.include_source_spans,
                            );
                            return Ok(fallback_value(
                                arguments.operation,
                                repository_status_unavailable_fallback(
                                    QueryPreflightOperation::PatternFamilyQuery,
                                ),
                            ));
                        }
                    }
                } else {
                    None
                };
                let selected_evidence = select_family_evidence(&family, arguments.output_options);
                let estimated_potential = estimate_family_output_potential_token_savings(
                    &family,
                    &selected_evidence,
                    &read_plan,
                    source_spans.as_ref(),
                );
                record_mcp_estimated_potential_token_savings(
                    request.clone(),
                    Some(found_outcome_token_savings(&family, estimated_potential)),
                );
                record_mcp_family_query_outcome(
                    request,
                    arguments.operation,
                    FamilyQueryOutcomeStatus::Found,
                    &family.unknowns,
                    None,
                    Some(&read_plan),
                    source_spans.as_ref(),
                    arguments.include_source_spans,
                );
                Ok(family_detail_value(
                    arguments.operation,
                    &family,
                    &route,
                    &read_plan,
                    arguments.output_options,
                    source_spans.as_ref(),
                ))
            }
            Ok(FamilyLookupReport::PartialContext(report)) => {
                let route = family_query_route_report(
                    &FamilyLookupReport::PartialContext(report.clone()),
                    lookup_mode_for_operation(arguments.operation),
                );
                let mut read_plan = match runtime
                    .enrich_read_plan_line_ranges(request.clone(), &report.read_plan)
                {
                    Ok(read_plan) => read_plan,
                    Err(_) => {
                        record_mcp_family_query_fallback(
                            request,
                            arguments.operation,
                            arguments.include_source_spans,
                        );
                        return Ok(fallback_value(
                            arguments.operation,
                            repository_status_unavailable_fallback(
                                QueryPreflightOperation::PatternFamilyQuery,
                            ),
                        ));
                    }
                };
                let source_spans = if arguments.include_source_spans {
                    match runtime.render_source_spans(
                        request.clone(),
                        &read_plan,
                        true,
                        arguments.output_options.token_budget,
                    ) {
                        Ok(source_spans) => {
                            read_plan = read_plan_with_rendered_spans(&read_plan, &source_spans);
                            Some(source_spans)
                        }
                        Err(_) => {
                            record_mcp_family_query_fallback(
                                request,
                                arguments.operation,
                                arguments.include_source_spans,
                            );
                            return Ok(fallback_value(
                                arguments.operation,
                                repository_status_unavailable_fallback(
                                    QueryPreflightOperation::PatternFamilyQuery,
                                ),
                            ));
                        }
                    }
                } else {
                    None
                };
                record_mcp_estimated_potential_token_savings(
                    request.clone(),
                    estimate_partial_context_potential_token_savings(
                        &report,
                        &read_plan,
                        source_spans.as_ref(),
                    ),
                );
                record_mcp_family_query_outcome(
                    request,
                    arguments.operation,
                    FamilyQueryOutcomeStatus::PartialContext,
                    &report.unknowns,
                    None,
                    Some(&read_plan),
                    source_spans.as_ref(),
                    arguments.include_source_spans,
                );
                Ok(family_partial_context_value(
                    arguments.operation,
                    &report,
                    &route,
                    &read_plan,
                    arguments.output_options,
                    source_spans.as_ref(),
                ))
            }
            Ok(FamilyLookupReport::Unknown(report)) => {
                let route = family_query_route_report(
                    &FamilyLookupReport::Unknown(report.clone()),
                    lookup_mode_for_operation(arguments.operation),
                );
                record_mcp_family_query_outcome(
                    request,
                    arguments.operation,
                    FamilyQueryOutcomeStatus::Unknown,
                    &report.unknowns,
                    report
                        .term_retrieval
                        .as_ref()
                        .and_then(|term| term.abstention_reason)
                        .map(TermRetrievalAbstention::as_str),
                    None,
                    None,
                    arguments.include_source_spans,
                );
                Ok(json!({
                    "operation": arguments.operation.as_str(),
                    "command": arguments.operation.cli_command(),
                    "schema_version": PRODUCT_SCHEMA_VERSION,
                    "status": "UNKNOWN",
                    "implemented": true,
                    "active_generation": report.active_generation,
                    "query_route": query_route_value(&route),
                    "unknowns": unknowns_value(&report.unknowns),
                }))
            }
            Ok(FamilyLookupReport::Alignment(certificate)) => {
                let route = family_query_route_report(
                    &FamilyLookupReport::Alignment(certificate.clone()),
                    lookup_mode_for_operation(arguments.operation),
                );
                let mut read_plan = match runtime
                    .enrich_read_plan_line_ranges(request.clone(), &certificate.read_plan)
                {
                    Ok(read_plan) => read_plan,
                    Err(_) => {
                        record_mcp_family_query_fallback(
                            request,
                            arguments.operation,
                            arguments.include_source_spans,
                        );
                        return Ok(fallback_value(
                            arguments.operation,
                            repository_status_unavailable_fallback(
                                QueryPreflightOperation::PatternFamilyQuery,
                            ),
                        ));
                    }
                };
                let source_spans = if arguments.include_source_spans {
                    match runtime.render_source_spans(
                        request.clone(),
                        &read_plan,
                        true,
                        arguments.output_options.token_budget,
                    ) {
                        Ok(source_spans) => {
                            read_plan = read_plan_with_rendered_spans(&read_plan, &source_spans);
                            Some(source_spans)
                        }
                        Err(_) => {
                            record_mcp_family_query_fallback(
                                request,
                                arguments.operation,
                                arguments.include_source_spans,
                            );
                            return Ok(fallback_value(
                                arguments.operation,
                                repository_status_unavailable_fallback(
                                    QueryPreflightOperation::PatternFamilyQuery,
                                ),
                            ));
                        }
                    }
                } else {
                    None
                };
                let outcome_status = alignment_outcome_status(certificate.alignment_status);
                record_mcp_estimated_potential_token_savings(
                    request.clone(),
                    estimate_alignment_potential_token_savings(
                        &certificate,
                        &read_plan,
                        source_spans.as_ref(),
                    ),
                );
                record_mcp_family_query_outcome(
                    request,
                    arguments.operation,
                    outcome_status,
                    &certificate.unknowns,
                    None,
                    Some(&read_plan),
                    source_spans.as_ref(),
                    arguments.include_source_spans,
                );
                Ok(alignment_certificate_value(
                    arguments.operation,
                    &certificate,
                    &route,
                    &read_plan,
                    source_spans.as_ref(),
                ))
            }
            Err(_) => {
                record_mcp_family_query_fallback(
                    request,
                    arguments.operation,
                    arguments.include_source_spans,
                );
                Ok(fallback_value(
                    arguments.operation,
                    repository_status_unavailable_fallback(
                        QueryPreflightOperation::PatternFamilyQuery,
                    ),
                ))
            }
        },
    }
}

/// Map an alignment status onto the query-outcome telemetry bucket via the core
/// commitment-class authority, so an abstaining certificate is never Found.
fn alignment_outcome_status(
    status: crate::core::policy::alignment::AlignmentStatus,
) -> FamilyQueryOutcomeStatus {
    use crate::core::policy::alignment::AlignmentOutcomeClass;
    match status.outcome_class() {
        AlignmentOutcomeClass::Committed => FamilyQueryOutcomeStatus::Found,
        AlignmentOutcomeClass::Partial => FamilyQueryOutcomeStatus::PartialContext,
        AlignmentOutcomeClass::Abstained => FamilyQueryOutcomeStatus::Unknown,
    }
}

fn lookup_mode_for_operation(operation: McpOperation) -> FamilyLookupMode {
    match operation {
        McpOperation::ShowFamily => FamilyLookupMode::ExactFamilyId,
        McpOperation::FindAnalogues | McpOperation::ExplainDeviation => {
            FamilyLookupMode::FuzzyQuery
        }
        // check_conformance runs the static-alignment flow.
        McpOperation::CheckConformance => FamilyLookupMode::Conformance,
        // inspect_readiness never enters the family-lookup path.
        McpOperation::InspectReadiness => {
            unreachable!("inspect_readiness is not a family-query operation")
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_mcp_family_query_outcome(
    request: RepositoryStatusRequest,
    operation: McpOperation,
    status: FamilyQueryOutcomeStatus,
    unknowns: &[FamilyQueryUnknown],
    abstention_reason: Option<&str>,
    read_plan: Option<&ReadPlan>,
    source_spans: Option<&SourceSpanRenderReport>,
    source_spans_requested: bool,
) {
    let unknown_metrics = unknowns
        .iter()
        .map(family_query_unknown_metric)
        .collect::<Vec<_>>();
    let record = FamilyQueryOutcomeRecord {
        status,
        entrypoint: FamilyQueryEntrypoint::Mcp,
        command_category: family_query_operation_category(operation),
        lookup_mode: family_query_lookup_mode(lookup_mode_for_operation(operation)),
        unknowns: &unknown_metrics,
        abstention_reason,
        read_plan_item_count: read_plan.map(|read_plan| read_plan.items.len()),
        source_spans_requested,
        source_spans_included: read_plan
            .is_some_and(|read_plan| read_plan.source_snippets_included),
        source_span_omission_count: source_spans.map(|source_spans| source_spans.omissions.len()),
    };
    let _ = record_family_query_outcome(request, &record);
}

/// Record one all-scope potential-token-savings event through the telemetry
/// authority. `None` (an abstention or a PARTIAL_CONTEXT/certificate with no
/// stored file size) records nothing, keeping the query in the denominator only.
fn record_mcp_estimated_potential_token_savings(
    request: RepositoryStatusRequest,
    savings: Option<OutcomeTokenSavings>,
) {
    if let Some(savings) = savings {
        let _ = record_estimated_potential_token_savings(
            request,
            &savings.metric,
            savings.shape.as_str(),
            savings.language,
        );
    }
}

fn record_mcp_family_query_fallback(
    request: RepositoryStatusRequest,
    operation: McpOperation,
    source_spans_requested: bool,
) {
    let record = FamilyQueryOutcomeRecord {
        status: FamilyQueryOutcomeStatus::Fallback,
        entrypoint: FamilyQueryEntrypoint::Mcp,
        command_category: family_query_operation_category(operation),
        lookup_mode: family_query_lookup_mode(lookup_mode_for_operation(operation)),
        unknowns: &[],
        abstention_reason: None,
        read_plan_item_count: None,
        source_spans_requested,
        source_spans_included: false,
        source_span_omission_count: None,
    };
    let _ = record_family_query_outcome(request, &record);
}

fn family_query_operation_category(operation: McpOperation) -> FamilyQueryCommandCategory {
    match operation {
        McpOperation::FindAnalogues => FamilyQueryCommandCategory::FindAnalogues,
        McpOperation::ShowFamily => FamilyQueryCommandCategory::ShowFamily,
        McpOperation::ExplainDeviation => FamilyQueryCommandCategory::ExplainDeviation,
        McpOperation::CheckConformance => FamilyQueryCommandCategory::CheckConformance,
        // inspect_readiness records no family-query telemetry (like status/doctor).
        McpOperation::InspectReadiness => {
            unreachable!("inspect_readiness records no family-query outcome")
        }
    }
}

fn family_query_lookup_mode(mode: FamilyLookupMode) -> FamilyQueryLookupMode {
    match mode {
        FamilyLookupMode::ExactFamilyId => FamilyQueryLookupMode::ExactFamily,
        FamilyLookupMode::ExactMemberId => FamilyQueryLookupMode::ExactMember,
        FamilyLookupMode::FuzzyQuery | FamilyLookupMode::Conformance => {
            FamilyQueryLookupMode::Fuzzy
        }
    }
}

pub fn handle_json_rpc_value(
    runtime: &impl McpReadOnlyRuntime,
    context: &McpServeContext,
    request: Value,
) -> McpJsonRpcOutcome {
    match handle_json_rpc_value_result(runtime, context, request) {
        Ok(outcome) => outcome,
        Err((id, error)) => McpJsonRpcOutcome {
            response: Some(error_response(id, error)),
            should_shutdown: false,
        },
    }
}

pub fn serve_json_lines(
    runtime: &impl McpReadOnlyRuntime,
    context: &McpServeContext,
    mut reader: impl BufRead,
    mut writer: impl Write,
) -> Result<(), RepoGrammarError> {
    loop {
        let mut line = String::new();
        // Bound the read to one byte past the limit so an unterminated
        // multi-gigabyte line cannot be buffered into memory before the size
        // check below rejects it. Reading MAX+1 bytes is exactly enough to
        // detect `len > MAX` while capping the allocation.
        let bytes = (&mut reader)
            .take((MAX_MCP_LINE_BYTES as u64) + 1)
            .read_line(&mut line)
            .map_err(|error| {
                RepoGrammarError::InvalidInput(format!("failed to read MCP stdin: {error}"))
            })?;
        if bytes == 0 {
            return Ok(());
        }
        if line.len() > MAX_MCP_LINE_BYTES {
            return Err(RepoGrammarError::InvalidInput(
                "MCP request line exceeds the 1 MiB limit".to_string(),
            ));
        }
        if line.trim().is_empty() {
            continue;
        }
        let outcome = match serde_json::from_str::<Value>(line.trim_end()) {
            Ok(value) => handle_json_rpc_value(runtime, context, value),
            Err(_) => McpJsonRpcOutcome {
                response: Some(error_response(
                    Value::Null,
                    McpProtocolError::parse_error("invalid JSON-RPC message"),
                )),
                should_shutdown: false,
            },
        };
        if let Some(response) = outcome.response {
            writeln!(writer, "{response}").map_err(|error| {
                RepoGrammarError::InvalidInput(format!("failed to write MCP stdout: {error}"))
            })?;
        }
        if outcome.should_shutdown {
            return Ok(());
        }
    }
}

fn handle_json_rpc_value_result(
    runtime: &impl McpReadOnlyRuntime,
    context: &McpServeContext,
    request: Value,
) -> Result<McpJsonRpcOutcome, (Value, McpProtocolError)> {
    let object = request.as_object().ok_or_else(|| {
        (
            Value::Null,
            McpProtocolError::invalid_request("JSON-RPC request must be an object"),
        )
    })?;
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                id.clone(),
                McpProtocolError::invalid_request("JSON-RPC method is required"),
            )
        })?;

    match method {
        "initialize" => {
            let protocol_version = object
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(MCP_PROTOCOL_VERSION);
            Ok(McpJsonRpcOutcome {
                response: Some(success_response(
                    id,
                    json!({
                        "protocolVersion": protocol_version,
                        "capabilities": {
                            "tools": {},
                        },
                        "serverInfo": {
                            "name": "repogrammar",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                        "instructions": MCP_AGENT_INSTRUCTIONS,
                    }),
                )),
                should_shutdown: false,
            })
        }
        "notifications/initialized" => Ok(McpJsonRpcOutcome {
            response: None,
            should_shutdown: false,
        }),
        "tools/list" => Ok(McpJsonRpcOutcome {
            response: Some(success_response(
                id,
                json!({
                    "tools": [tool_schema()],
                }),
            )),
            should_shutdown: false,
        }),
        "tools/call" => {
            let params = object.get("params").ok_or_else(|| {
                (
                    id.clone(),
                    McpProtocolError::invalid_params("tools/call params are required"),
                )
            })?;
            let name = params.get("name").and_then(Value::as_str).ok_or_else(|| {
                (
                    id.clone(),
                    McpProtocolError::invalid_params("tools/call params.name is required"),
                )
            })?;
            if name != McpToolName::Context.as_str() {
                return Err((
                    id,
                    McpProtocolError::invalid_params("unknown MCP tool name"),
                ));
            }
            let arguments = params.get("arguments").unwrap_or(&Value::Null);
            let payload = handle_context_call(runtime, context, arguments)
                .map_err(|error| (object.get("id").cloned().unwrap_or(Value::Null), error))?;
            Ok(McpJsonRpcOutcome {
                response: Some(success_response(id, mcp_tool_result(payload))),
                should_shutdown: false,
            })
        }
        "shutdown" => Ok(McpJsonRpcOutcome {
            response: Some(success_response(id, Value::Null)),
            should_shutdown: true,
        }),
        _ => Err((
            id,
            McpProtocolError::method_not_found("unsupported MCP JSON-RPC method"),
        )),
    }
}

fn parse_context_arguments(arguments: &Value) -> Result<ContextArguments, McpProtocolError> {
    let object = arguments.as_object().ok_or_else(|| {
        McpProtocolError::invalid_params("repogrammar_context arguments must be an object")
    })?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "operation"
                | "target"
                | "token_budget"
                | "mode"
                | "verbosity"
                | "include_variations"
                | "include_exceptions"
                | "include_source_spans"
        ) {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context arguments contain an unsupported field",
            ));
        }
    }
    let operation = object
        .get("operation")
        .and_then(Value::as_str)
        .and_then(McpOperation::parse)
        .ok_or_else(|| {
            McpProtocolError::invalid_params(
                "repogrammar_context operation must be one of find_analogues, show_family, explain_deviation, check_conformance, or inspect_readiness",
            )
        })?;
    let target = match object.get("target") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => {
            validate_query_target(value).map_err(|error| {
                McpProtocolError::invalid_params(format!("repogrammar_context {error}"))
            })?;
            Some(value.clone())
        }
        Some(_) => {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context target must be a string when provided",
            ));
        }
    };
    let token_budget = if let Some(token_budget) = object.get("token_budget") {
        let Some(value) = token_budget.as_u64() else {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context token_budget must be a positive integer",
            ));
        };
        let parsed = usize::try_from(value).map_err(|_| {
            McpProtocolError::invalid_params(
                "repogrammar_context token_budget must fit in the local machine word size",
            )
        })?;
        validate_query_token_budget(parsed).map_err(|error| {
            McpProtocolError::invalid_params(format!("repogrammar_context {error}"))
        })?;
        Some(parsed)
    } else {
        None
    };
    let mode_explicit = object.get("mode").is_some_and(|value| !value.is_null());
    let mut evidence_mode = match object.get("mode") {
        None | Some(Value::Null) => FamilyEvidenceMode::Compact,
        Some(Value::String(value)) => FamilyEvidenceMode::parse(value).ok_or_else(|| {
            McpProtocolError::invalid_params(
                "repogrammar_context mode must be compact, evidence, or deep",
            )
        })?,
        Some(_) => {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context mode must be a string when provided",
            ));
        }
    };
    if token_budget.is_some() && !mode_explicit && evidence_mode == FamilyEvidenceMode::Compact {
        evidence_mode = FamilyEvidenceMode::Evidence;
    }
    // `verbosity` selects response field density and is orthogonal to `mode`,
    // which selects evidence detail. Absent verbosity keeps the byte-stable
    // `standard` shape; an unrecognized value is a schema error, never a silent
    // fallback.
    let verbosity = match object.get("verbosity") {
        None | Some(Value::Null) => Verbosity::Standard,
        Some(Value::String(value)) => Verbosity::parse(value).ok_or_else(|| {
            McpProtocolError::invalid_params(
                "repogrammar_context verbosity must be minimal, standard, or full",
            )
        })?,
        Some(_) => {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context verbosity must be a string when provided",
            ));
        }
    };
    for field in [
        "include_variations",
        "include_exceptions",
        "include_source_spans",
    ] {
        if object.get(field).is_some_and(|value| !value.is_boolean()) {
            return Err(McpProtocolError::invalid_params(
                "repogrammar_context include flags must be boolean",
            ));
        }
    }
    let include_variations = object
        .get("include_variations")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_exceptions = object
        .get("include_exceptions")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_source_spans = object
        .get("include_source_spans")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(ContextArguments {
        operation,
        target,
        output_options: FamilyOutputOptions {
            evidence_mode,
            token_budget,
            include_variations,
            include_exceptions,
            verbosity,
        },
        include_source_spans,
    })
}

fn fallback_value(
    operation: McpOperation,
    fallback: crate::application::query::QueryFallbackReport,
) -> Value {
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "FALLBACK_TO_CODE_SEARCH",
        "reason": fallback.reason,
        "guidance": fallback.guidance,
        "implemented": fallback.implemented,
    })
}

/// Bounded, source-free MCP result for `inspect_readiness`: the decomposed
/// product readiness report plus the shared operation/command/schema envelope.
/// It carries no source text, evidence, paths, or family detail — only typed
/// tokens, counts, and the single recovery action.
fn inspect_readiness_value(readiness: &ProductReadinessReport) -> Value {
    json!({
        "operation": McpOperation::InspectReadiness.as_str(),
        "command": McpOperation::InspectReadiness.cli_command(),
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "ok",
        "implemented": true,
        "readiness": product_readiness_value(readiness),
    })
}

/// Metadata-only prevalence object exposed on the MCP family detail payload.
fn family_prevalence_value(prevalence: &FamilyPrevalence) -> Value {
    json!({
        "eligible_peer_count": prevalence.eligible_peer_count,
        "supported_member_count": prevalence.supported_member_count,
        "coverage_ratio": prevalence.coverage_ratio,
        "competing_ready_family_count": prevalence.competing_ready_family_count,
        "largest_competing_support": prevalence.largest_competing_support,
        "blocked_peer_count": prevalence.blocked_peer_count,
        "unsupported_peer_count": prevalence.unsupported_peer_count,
        "classification_reason": prevalence.classification_reason,
    })
}

/// Metadata-only view of a family's hydrated constraint profile, or `null` when
/// the active generation persisted none. Mirrors the CLI serializer: every field
/// is a RepoGrammar-owned typed token or count in the profile's deterministic
/// order, and no repository source text is emitted.
fn family_constraint_profile_value(profile: Option<&FamilyConstraintProfile>) -> Value {
    let Some(profile) = profile else {
        return Value::Null;
    };
    json!({
        "required_equal_features": profile
            .required_equal_features
            .iter()
            .map(feature_constraint_value)
            .collect::<Vec<_>>(),
        "allowed_variations": profile
            .allowed_variations
            .iter()
            .map(|variation| json!({
                "dimension": variation.dimension,
                "observed_profiles": variation.observed_profiles,
                "observed_profiles_truncated": variation.observed_profiles_truncated,
                "includes_absent_profile": variation.includes_absent_profile,
                "representative_member_ids": variation.representative_member_ids,
                "observed_only": variation.observed_only,
            }))
            .collect::<Vec<_>>(),
        "prohibited_or_blocking_features": profile
            .prohibited_or_blocking_features
            .iter()
            .map(feature_constraint_value)
            .collect::<Vec<_>>(),
        "unresolved_obligations": profile
            .unresolved_obligations
            .iter()
            .map(|obligation| json!({
                "class": obligation.class.as_protocol_str(),
                "reason": obligation.reason.as_protocol_str(),
                "affected_claim": obligation.affected_claim,
                "recovery": obligation.recovery,
            }))
            .collect::<Vec<_>>(),
    })
}

fn feature_constraint_value(constraint: &FeatureConstraint) -> Value {
    json!({
        "prefix": constraint.prefix,
        "values": constraint.values,
        "origin": constraint.origin.as_token(),
        "semantics": constraint.semantics.as_token(),
    })
}

fn family_detail_value(
    operation: McpOperation,
    family: &FamilyDetailReport,
    route: &FamilyQueryRouteReport,
    read_plan: &ReadPlan,
    options: FamilyOutputOptions,
    source_spans: Option<&SourceSpanRenderReport>,
) -> Value {
    let selected_evidence = select_family_evidence(family, options);
    let estimated_potential = estimate_family_output_potential_token_savings(
        family,
        &selected_evidence,
        read_plan,
        source_spans,
    );
    let (rendered_members, members_truncated) =
        bounded_family_members(family, options.evidence_mode);
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "ok",
        "implemented": true,
        "active_generation": family.active_generation,
        "query_route": query_route_value(route),
        "family": {
            "family_id": family.family_id,
            "classification": family.classification,
            "support": family.support,
            "prevalence": family_prevalence_value(&family.prevalence),
        },
        "output": {
            "mode": selected_evidence.mode.as_str(),
            "token_budget": selected_evidence.token_budget,
            "estimated_evidence_tokens": selected_evidence.estimated_tokens,
            "estimated_read_plan_tokens": read_plan.estimated_tokens,
            "estimated_baseline_tokens": estimated_potential.estimated_baseline_tokens,
            "estimated_returned_tokens": estimated_potential.estimated_returned_tokens,
            "estimated_potential_token_savings": estimated_potential.estimated_potential_token_savings,
            "estimated_potential_token_savings_kind": estimated_potential.measurement_kind.as_str(),
            "estimated_potential_token_savings_caveat": estimated_potential.caveat,
            "selection_strategy": selected_evidence.selection_strategy,
            "budget_satisfied": selected_evidence.budget_satisfied,
            "covered_claims": selected_evidence.covered_claims,
            "missing_claims": selected_evidence.missing_claims,
            "source_snippets_included": read_plan.source_snippets_included,
        },
        "member_count": family.members.len(),
        "members_truncated": members_truncated,
        "members": rendered_members.iter().map(|member| {
            json!({
                "family_id": member.family_id,
                "code_unit_id": member.code_unit_id,
                "role": member.role,
            })
        }).collect::<Vec<_>>(),
        "variation_slots": family.variation_slots.iter().map(|slot| {
            json!({
                "family_id": slot.family_id,
                "slot_id": slot.slot_id,
                "description": slot.description,
            })
        }).collect::<Vec<_>>(),
        "constraint_profile": family_constraint_profile_value(family.constraint_profile.as_deref()),
        "evidence": selected_evidence.evidence.iter().map(|evidence| {
            let record = &evidence.record;
            json!({
                "evidence_id": record.evidence_id,
                "family_id": record.family_id,
                "code_unit_id": record.code_unit_id,
                "path": record.path,
                "content_hash": record.content_hash.as_str(),
                "start_byte": record.start_byte,
                "end_byte": record.end_byte,
                "note": record.note,
                "estimated_tokens": evidence.estimated_tokens,
                "covered_claims": evidence.covered_claims,
            })
        }).collect::<Vec<_>>(),
        "read_plan": read_plan_value(read_plan),
        "source_spans": source_spans_value(source_spans),
        "unknowns": unknowns_value(&family.unknowns),
    })
}

fn family_partial_context_value(
    operation: McpOperation,
    report: &FamilyPartialContextReport,
    route: &FamilyQueryRouteReport,
    read_plan: &ReadPlan,
    options: FamilyOutputOptions,
    source_spans: Option<&SourceSpanRenderReport>,
) -> Value {
    let savings = estimate_partial_context_potential_token_savings(report, read_plan, source_spans);
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "PARTIAL_CONTEXT",
        "implemented": true,
        "active_generation": report.active_generation,
        "query_route": query_route_value(route),
        "resolved_target": resolved_target_value(&report.resolved_target),
        "output": {
            "mode": options.evidence_mode.as_str(),
            "token_budget": options.token_budget,
            "estimated_read_plan_tokens": read_plan.estimated_tokens,
            "selection_strategy": read_plan.selection_strategy,
            "budget_satisfied": read_plan.budget_satisfied,
            "source_snippets_included": read_plan.source_snippets_included,
        },
        "estimated_potential_token_savings": savings_block_value(savings.as_ref(), "partial_context", "resolved file size unavailable; no estimate recorded"),
        "read_plan": read_plan_value(read_plan),
        "source_spans": source_spans_value(source_spans),
        "unknowns": unknowns_value(&report.unknowns),
    })
}

/// The shared MCP `estimated_potential_token_savings` block, at CLI parity: the
/// full estimate when available, otherwise an explicit no-estimate block that
/// still carries the ESTIMATED caveat (never a guessed number).
fn savings_block_value(
    savings: Option<&OutcomeTokenSavings>,
    outcome_shape: &str,
    unavailable_reason: &str,
) -> Value {
    match savings {
        Some(savings) => json!({
            "outcome_shape": savings.shape.as_str(),
            "language": savings.language,
            "estimated_baseline_tokens": savings.metric.estimated_baseline_tokens,
            "estimated_returned_tokens": savings.metric.estimated_returned_tokens,
            "estimated_potential_token_savings": savings.metric.estimated_potential_token_savings,
            "estimated_potential_token_savings_kind": savings.metric.measurement_kind.as_str(),
            "estimated_potential_token_savings_caveat": savings.metric.caveat,
        }),
        None => json!({
            "outcome_shape": outcome_shape,
            "estimated_baseline_tokens": null,
            "estimated_returned_tokens": null,
            "estimated_potential_token_savings": null,
            "estimated_potential_token_savings_kind": MeasurementKind::Estimated.as_str(),
            "estimated_potential_token_savings_caveat": EstimatedPotentialTokenSavings::CAVEAT,
            "unavailable_reason": unavailable_reason,
        }),
    }
}

/// Source-free MCP value for a static-alignment certificate. Mirrors the CLI
/// JSON shape: the `status` is the alignment token, `runtime_equivalence` is
/// always `UNKNOWN`, and `query_route` carries the selected/candidate family ids.
fn alignment_certificate_value(
    operation: McpOperation,
    certificate: &AlignmentCertificateReport,
    route: &FamilyQueryRouteReport,
    read_plan: &ReadPlan,
    source_spans: Option<&SourceSpanRenderReport>,
) -> Value {
    let savings = estimate_alignment_potential_token_savings(certificate, read_plan, source_spans);
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": certificate.alignment_status.as_token(),
        "implemented": true,
        "active_generation": certificate.active_generation,
        "query_route": query_route_value(route),
        "alignment_status": certificate.alignment_status.as_token(),
        "runtime_equivalence": "UNKNOWN",
        "target_relationship": certificate
            .target_relationship
            .map(|relationship| relationship.as_token()),
        "selected_family_id": certificate.selected_family_id,
        "target": resolved_target_value(&certificate.resolved_target),
        "alignment": certificate
            .computation
            .as_deref()
            .map(alignment_computation_value),
        "estimated_potential_token_savings": savings_block_value(savings.as_ref(), "alignment", "abstaining certificate; no full read displaced"),
        "read_plan": read_plan_value(read_plan),
        "source_spans": source_spans_value(source_spans),
        "unknowns": unknowns_value(&certificate.unknowns),
    })
}

fn alignment_computation_value(computation: &AlignmentComputation) -> Value {
    json!({
        "outcome_reason": computation.outcome_reason,
        "required_features_matched": computation
            .required_features_matched
            .iter()
            .map(|matched| json!({
                "prefix": matched.prefix,
                "semantics": matched.semantics.as_token(),
                "expected_summary": matched.expected_summary,
                "satisfied_summary": matched.satisfied_summary,
            }))
            .collect::<Vec<_>>(),
        "static_deviations": computation
            .static_deviations
            .iter()
            .map(|deviation| json!({
                "prefix": deviation.prefix,
                "kind": deviation.kind.as_token(),
                "semantics_token": deviation.semantics_token,
                "expected_summary": deviation.expected_summary,
                "observed_summary": deviation.observed_summary,
            }))
            .collect::<Vec<_>>(),
        "legal_observed_variations": computation
            .legal_observed_variations
            .iter()
            .map(|variation| json!({
                "dimension": variation.dimension,
                "observed_profile": variation.observed_profile,
            }))
            .collect::<Vec<_>>(),
        "blocking_unknowns": computation
            .blocking_unknowns
            .iter()
            .map(alignment_typed_unknown_value)
            .collect::<Vec<_>>(),
        "unresolved_runtime_obligations": computation
            .unresolved_runtime_obligations
            .iter()
            .map(alignment_typed_unknown_value)
            .collect::<Vec<_>>(),
    })
}

fn alignment_typed_unknown_value(unknown: &crate::core::model::TypedUnknown) -> Value {
    json!({
        "class": unknown.class.as_protocol_str(),
        "reason": unknown.reason.as_protocol_str(),
        "affected_claim": unknown.affected_claim,
        "recovery": unknown.recovery,
    })
}

fn query_route_value(route: &FamilyQueryRouteReport) -> Value {
    json!({
        "route": route.route,
        "input_kind": route.input_kind,
        "pipeline": route.pipeline,
        "family_id_policy": route.family_id_policy,
        "candidate_limit": route.candidate_limit,
        "selected_family_id": route.selected_family_id,
        "candidate_family_ids": route.candidate_family_ids,
        "follow_up_family_ids": route.follow_up_family_ids,
        "why_selected": route.why_selected,
        // Numeric fields the product-eval harness reads directly; null unless the
        // deterministic term-retrieval fallback produced this route.
        "hydrated_family_count": route
            .term_retrieval
            .as_ref()
            .map(|term| term.hydrated_candidate_count),
        "retrieval_stage_count": route
            .term_retrieval
            .as_ref()
            .map(|term| term.retrieval_stage_count),
        "term_retrieval": route.term_retrieval.as_ref().map(term_retrieval_value),
    })
}

/// Source-free JSON for a term-retrieval route (MCP shape). Carries only enum
/// tokens, counts, and small integer scores; never raw target text.
fn term_retrieval_value(term: &TermRetrievalRoute) -> Value {
    json!({
        "route": term.route,
        "retrieved_summary_count": term.retrieved_summary_count,
        "ranked_candidate_count": term.ranked_candidate_count,
        "hydrated_candidate_count": term.hydrated_candidate_count,
        "retrieval_stage_count": term.retrieval_stage_count,
        "top_score": term.top_score,
        "margin": term.margin,
        "top_score_bucket": term.top_score_bucket,
        "margin_bucket": term.margin_bucket,
        "truncated": term.truncated,
        "matched_signals": term.matched_signals.map(|signals| json!({
            "framework_filter": signals.framework_filter,
            "concept": signals.concept,
            "language_filter": signals.language_filter,
            "residue_hits": signals.residue_hits,
        })),
        "abstention_reason": term.abstention_reason.map(|reason| reason.as_str()),
    })
}

fn resolved_target_value(target: &ResolvedQueryTarget) -> Value {
    json!({
        "original_target": target.original_target,
        "kind": target.kind,
        "path": target.path,
        "line": target.line,
        "byte_range": target.byte_range.map(|(start, end)| json!({"start": start, "end": end})),
        "family_id": target.family_id,
        "code_unit_id": target.code_unit_id,
        "symbol_hints": target.symbol_hints,
        "residue_terms": target.residue_terms,
        "candidate_paths": target.candidate_paths,
        "candidate_family_ids": target.candidate_family_ids,
        "candidate_code_unit_ids": target.candidate_code_unit_ids,
        "confidence": target.confidence,
        "match_kind": target.match_kind,
    })
}

fn read_plan_value(read_plan: &ReadPlan) -> Value {
    json!({
        "estimated_tokens": read_plan.estimated_tokens,
        "source_snippets_included": read_plan.source_snippets_included,
        "requires_source_before_edit": read_plan.requires_source_before_edit,
        "selection_strategy": read_plan.selection_strategy,
        "budget_satisfied": read_plan.budget_satisfied,
        "items": read_plan.items.iter().map(read_plan_item_value).collect::<Vec<_>>(),
        "line_range_omissions": read_plan.line_range_omissions.iter().map(read_plan_line_range_omission_value).collect::<Vec<_>>(),
    })
}

fn read_plan_line_range_omission_value(omission: &ReadPlanLineRangeOmission) -> Value {
    json!({
        "purpose": omission.purpose.as_str(),
        "path": omission.path,
        "start_byte": omission.start_byte,
        "end_byte": omission.end_byte,
        "reason": omission.reason,
        "guidance": omission.guidance,
    })
}

fn read_plan_item_value(item: &ReadPlanItem) -> Value {
    json!({
        "purpose": item.purpose.as_str(),
        "path": item.path,
        "content_hash": item.content_hash.as_str(),
        "start_byte": item.start_byte,
        "end_byte": item.end_byte,
        "start_line": item.start_line,
        "end_line": item.end_line,
        "estimated_tokens": item.estimated_tokens,
        "why": item.why,
        "source_required_before_edit": item.source_required_before_edit,
        "source_snippets_included": item.source_snippets_included,
    })
}

fn source_spans_value(source_spans: Option<&SourceSpanRenderReport>) -> Value {
    match source_spans {
        None => json!({
            "requested": false,
            "source_snippets_included": false,
            "spans": [],
            "omissions": [],
        }),
        Some(source_spans) => json!({
            "requested": source_spans.policy.requested,
            "source_snippets_included": source_spans.policy.source_snippets_included,
            "estimated_tokens": source_spans.policy.estimated_tokens,
            "budget_satisfied": source_spans.policy.budget_satisfied,
            "selection_strategy": source_spans.policy.selection_strategy,
            "fallback_guidance": source_spans.policy.fallback_guidance,
            "spans": source_spans.spans.iter().map(|span| {
                json!({
                    "purpose": span.purpose.as_str(),
                    "path": span.path,
                    "content_hash": span.content_hash.as_str(),
                    "start_byte": span.start_byte,
                    "end_byte": span.end_byte,
                    "start_line": span.start_line,
                    "end_line": span.end_line,
                    "estimated_tokens": span.estimated_tokens,
                    "why": span.why,
                    "source_required_before_edit": span.source_required_before_edit,
                    "text": span.text,
                })
            }).collect::<Vec<_>>(),
            "omissions": source_spans.omissions.iter().map(|omission| {
                json!({
                    "purpose": omission.purpose.as_str(),
                    "path": omission.path,
                    "start_byte": omission.start_byte,
                    "end_byte": omission.end_byte,
                    "reason": omission.reason,
                    "guidance": omission.guidance,
                })
            }).collect::<Vec<_>>(),
        }),
    }
}

fn unknowns_value(unknowns: &[FamilyQueryUnknown]) -> Vec<Value> {
    unknowns
        .iter()
        .map(|unknown| {
            json!({
                "class": unknown.class.as_protocol_str(),
                "reason": unknown.reason.as_protocol_str(),
                "affected_claim": unknown.affected_claim,
                "recovery": unknown.recovery,
            })
        })
        .collect()
}

fn mcp_tool_result(payload: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": payload.to_string(),
        }],
        "isError": false,
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn error_response(id: Value, error: McpProtocolError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": error.code,
            "message": error.message,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::query::{FamilyQueryUnknown, FamilyUnknownReport};
    use crate::application::query_terms::MatchedSignals;
    use crate::application::repository::{
        RepositoryImplementationStatus, RepositoryReadiness, RepositoryStatus,
    };
    use crate::core::model::{ContentHash, UnknownClass, UnknownReasonCode};
    use crate::ports::family_store::{
        IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedVariationSlotRecord,
    };
    use crate::test_support::TempWorkspace;

    #[test]
    fn constraint_profile_value_is_metadata_only_and_null_when_absent() {
        assert_eq!(family_constraint_profile_value(None), Value::Null);
        let profile = crate::test_support::sample_family_constraint_profile();
        let value = family_constraint_profile_value(Some(&profile));
        let required = value["required_equal_features"]
            .as_array()
            .expect("required_equal_features");
        assert!(required.iter().any(|constraint| {
            constraint["origin"] == "framework_role_identity" && constraint["semantics"] == "equal"
        }));
        assert_eq!(
            value["allowed_variations"][0]["dimension"],
            "python_import_context"
        );
        assert_eq!(
            value["prohibited_or_blocking_features"][0]["semantics"],
            "prohibited_presence"
        );
        assert_eq!(
            value["unresolved_obligations"][0]["affected_claim"],
            "family:example:runtime_equivalence"
        );
        // The serializer echoes the profile's typed fields and adds no others;
        // source-freedom is enforced by the storage-side hydration validators.
        let mut keys = value
            .as_object()
            .expect("constraint_profile object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "allowed_variations".to_string(),
                "prohibited_or_blocking_features".to_string(),
                "required_equal_features".to_string(),
                "unresolved_obligations".to_string(),
            ]
        );
    }
    use std::fs;

    #[test]
    fn query_route_value_serializes_term_retrieval_metadata() {
        let route = FamilyQueryRouteReport {
            route: "discovery_unknown",
            input_kind: "path_symbol_role_or_pattern_target",
            pipeline: vec!["discover_candidates", "abstain"],
            family_id_policy:
                "family_ids_are_returned_follow_up_handles_not_required_initial_inputs",
            candidate_limit: Some(5),
            selected_family_id: None,
            candidate_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            follow_up_family_ids: vec![],
            why_selected: "candidate discovery could not produce a single supported family",
            term_retrieval: Some(TermRetrievalRoute {
                route: "term_retrieval_unknown",
                retrieved_summary_count: 8,
                ranked_candidate_count: 1,
                hydrated_candidate_count: 0,
                retrieval_stage_count: 4,
                top_score: Some(4),
                margin: None,
                top_score_bucket: "below_min",
                margin_bucket: "none",
                matched_signals: Some(MatchedSignals {
                    framework_filter: false,
                    concept: true,
                    language_filter: false,
                    residue_hits: 0,
                }),
                truncated: false,
                abstention_reason: Some(TermRetrievalAbstention::BelowMinScore),
            }),
        };
        let value = query_route_value(&route);
        assert_eq!(value["hydrated_family_count"], 0);
        assert_eq!(value["retrieval_stage_count"], 4);
        let term = &value["term_retrieval"];
        assert_eq!(term["route"], "term_retrieval_unknown");
        assert_eq!(term["abstention_reason"], "below_min_score");
        assert_eq!(term["top_score"], 4);
        assert_eq!(term["top_score_bucket"], "below_min");
        assert_eq!(term["ranked_candidate_count"], 1);
        assert_eq!(term["matched_signals"]["concept"], true);
    }

    #[test]
    fn tool_names_match_bootstrap_contract() {
        assert_eq!(McpToolName::Context.as_str(), "repogrammar_context");
        assert_eq!(McpOperation::FindAnalogues.as_str(), "find_analogues");
        assert_eq!(McpOperation::ShowFamily.as_str(), "show_family");
        assert_eq!(McpOperation::ExplainDeviation.as_str(), "explain_deviation");
        assert_eq!(McpOperation::CheckConformance.as_str(), "check_conformance");
        assert_eq!(McpOperation::InspectReadiness.as_str(), "inspect_readiness");
    }

    #[test]
    fn tool_schema_exposes_only_default_context_tool_shape() {
        let schema = tool_schema();

        assert_eq!(schema["name"], "repogrammar_context");
        let description = schema["description"].as_str().expect("tool description");
        assert!(description.contains("pass the path, symbol/member id, framework role"));
        assert!(description.contains("family ids as follow-up handles"));
        assert!(description.contains("Use show_family only with an exact family id"));
        assert!(description.contains("Start with compact mode"));
        assert!(description.contains("do not request include_source_spans by default"));
        assert!(description.contains("Do not run repogrammar stats"));
        assert!(description.contains("No silent setup"));
        assert!(description.contains("inspect_readiness"));
        assert_eq!(
            schema["inputSchema"]["properties"]["operation"]["enum"],
            json!([
                "find_analogues",
                "show_family",
                "explain_deviation",
                "check_conformance",
                "inspect_readiness"
            ])
        );
        assert_eq!(schema["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            schema["inputSchema"]["properties"]["target"]["minLength"],
            1
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["target"]["maxLength"],
            MAX_MCP_TARGET_BYTES
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["token_budget"]["maximum"],
            MAX_MCP_TOKEN_BUDGET
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["mode"]["enum"],
            json!(["compact", "evidence", "deep"])
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["include_source_spans"]["type"],
            "boolean"
        );
    }

    #[test]
    fn context_arguments_reject_oversized_or_control_text_inputs() {
        let runtime = FakeMcpRuntime::ready_unknown();
        let oversized_target = "x".repeat(MAX_MCP_TARGET_BYTES + 1);
        for arguments in [
            json!({"operation": "find_analogues", "target": oversized_target}),
            json!({"operation": "find_analogues", "target": "contains\nnewline"}),
            json!({"operation": "find_analogues", "token_budget": MAX_MCP_TOKEN_BUDGET + 1}),
            json!({"operation": "find_analogues", "include_source_spans": "yes"}),
        ] {
            let error = handle_context_call(&runtime, &context(), &arguments)
                .expect_err("invalid query input must be rejected");

            assert_eq!(error.code(), -32602);
        }
    }

    #[test]
    fn context_arguments_parse_verbosity_default_valid_and_invalid() {
        // Absent verbosity defaults to the byte-stable `standard` shape.
        let default = parse_context_arguments(&json!({"operation": "find_analogues"}))
            .expect("default arguments");
        assert_eq!(default.output_options.verbosity, Verbosity::Standard);

        // Each documented value parses on the MCP surface.
        for (value, expected) in [
            ("minimal", Verbosity::Minimal),
            ("standard", Verbosity::Standard),
            ("full", Verbosity::Full),
        ] {
            let parsed = parse_context_arguments(
                &json!({"operation": "find_analogues", "verbosity": value}),
            )
            .expect("verbosity arguments");
            assert_eq!(parsed.output_options.verbosity, expected);
        }

        // Unknown-string and non-string values are schema errors (-32602), never
        // a silent fallback to the default.
        for arguments in [
            json!({"operation": "find_analogues", "verbosity": "loud"}),
            json!({"operation": "find_analogues", "verbosity": "STANDARD"}),
            json!({"operation": "find_analogues", "verbosity": 3}),
        ] {
            let error = parse_context_arguments(&arguments)
                .expect_err("invalid verbosity must be rejected");
            assert_eq!(error.code(), -32602);
        }
    }

    #[test]
    fn verbosity_standard_matches_default_and_all_tiers_byte_for_byte() {
        let workspace = TempWorkspace::new("mcp-verbosity-byte-parity");
        let context = context_for_workspace(&workspace);
        let target = "src/routes/a.ts";

        let baseline = handle_context_call(
            &FakeMcpRuntime::ready_found(),
            &context,
            &json!({"operation": "find_analogues", "target": target}),
        )
        .expect("baseline found response");
        assert_eq!(baseline["status"], "ok");
        // v1 must not introduce a `verbosity` echo in the response envelope.
        assert!(baseline.get("verbosity").is_none());

        // S0 is additive: the explicit `standard` default and the skeleton
        // `minimal`/`full` tiers each reproduce the pre-precision bytes exactly.
        for verbosity in ["standard", "minimal", "full"] {
            let response = handle_context_call(
                &FakeMcpRuntime::ready_found(),
                &context,
                &json!({
                    "operation": "find_analogues",
                    "target": target,
                    "verbosity": verbosity,
                }),
            )
            .expect("verbosity found response");
            assert_eq!(
                serde_json::to_string(&response).expect("serialize response"),
                serde_json::to_string(&baseline).expect("serialize baseline"),
                "verbosity={verbosity} must be byte-identical to the default response in S0",
            );
        }
    }

    #[test]
    fn missing_repository_state_returns_fallback_without_writes() {
        let runtime = FakeMcpRuntime::not_initialized();
        let context = context();

        let response = handle_context_call(
            &runtime,
            &context,
            &json!({"operation": "find_analogues", "target": "src/routes/a.ts"}),
        )
        .expect("fallback response");

        assert_eq!(response["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(response["reason"], "repository is not initialized");
        assert_eq!(response["guidance"], "run repogrammar setup");
        assert_eq!(response["implemented"], false);
        assert_eq!(runtime.lookup_calls(), 0);
    }

    #[test]
    fn inspect_readiness_returns_bounded_source_free_report_without_family_lookup() {
        let runtime = FakeMcpRuntime::ready_unknown();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "inspect_readiness"}),
        )
        .expect("readiness response");

        assert_eq!(response["operation"], "inspect_readiness");
        assert_eq!(response["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(response["status"], "ok");
        let readiness = &response["readiness"];
        // The one-stale-family fixture must surface as degraded with the count.
        assert_eq!(readiness["summary"], "degraded");
        assert_eq!(readiness["family_evidence"]["stale_count"], 1);
        assert_eq!(readiness["repository_state"], "initialized");
        assert!(readiness["recovery"]["action"].is_string());
        // Read-only capability inspection: no family lookup runs.
        assert_eq!(runtime.lookup_calls(), 0);
        // Bounded and source-free: no read plan, evidence, or source spans.
        assert!(response.get("read_plan").is_none());
        assert!(response.get("source_spans").is_none());
        let serialized = response.to_string();
        assert!(!serialized.contains("content_hash"));
        assert!(!serialized.contains("start_byte"));
    }

    #[test]
    fn inspect_readiness_reports_not_ready_for_uninitialized_repository() {
        let runtime = FakeMcpRuntime::not_initialized();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "inspect_readiness"}),
        )
        .expect("readiness response");

        assert_eq!(response["readiness"]["summary"], "not_ready");
        assert_eq!(runtime.lookup_calls(), 0);
    }

    #[test]
    fn inspect_readiness_over_tools_call_wraps_bounded_text_content() {
        let runtime = FakeMcpRuntime::ready_unknown();

        let outcome = handle_json_rpc_value(
            &runtime,
            &context(),
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "repogrammar_context",
                    "arguments": {"operation": "inspect_readiness"},
                },
            }),
        );

        let payload = outcome.response.expect("tools/call response");
        assert_eq!(payload["result"]["isError"], false);
        let text = payload["result"]["content"][0]["text"]
            .as_str()
            .expect("text content");
        let parsed: Value = serde_json::from_str(text).expect("readiness JSON");
        assert_eq!(parsed["operation"], "inspect_readiness");
        assert_eq!(parsed["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(parsed["readiness"]["summary"], "degraded");
    }

    #[test]
    fn no_active_generation_returns_index_guidance() {
        let runtime = FakeMcpRuntime::initialized_without_generation();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "show_family", "target": "family:routes"}),
        )
        .expect("fallback response");

        assert_eq!(response["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(response["reason"], "no active index generation");
        assert_eq!(response["guidance"], "run repogrammar resync");
        assert_eq!(runtime.lookup_calls(), 0);
    }

    #[test]
    fn active_generation_with_insufficient_evidence_returns_typed_unknown() {
        let runtime = FakeMcpRuntime::ready_unknown();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "explain_deviation", "target": "src/routes/missing.ts"}),
        )
        .expect("unknown response");

        assert_eq!(response["status"], "UNKNOWN");
        assert_eq!(response["implemented"], true);
        assert_eq!(response["query_route"]["route"], "discovery_unknown");
        assert_eq!(
            response["query_route"]["family_id_policy"],
            "family_ids_are_returned_follow_up_handles_not_required_initial_inputs"
        );
        assert_eq!(response["unknowns"][0]["reason"], "InsufficientSupport");
        assert_eq!(runtime.lookup_calls(), 1);
    }

    #[test]
    fn active_generation_with_resolved_target_without_family_returns_partial_context() {
        let runtime = FakeMcpRuntime::ready_partial_context();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "find_analogues", "target": "src/routes/a.ts missing_family"}),
        )
        .expect("partial context response");
        let text = response.to_string();

        assert_eq!(response["status"], "PARTIAL_CONTEXT");
        assert_eq!(response["implemented"], true);
        assert_eq!(
            response["query_route"]["route"],
            "partial_context_read_plan"
        );
        assert_eq!(
            response["query_route"]["pipeline"],
            json!([
                "discover_candidates",
                "resolve_local_target",
                "compose_read_plan"
            ])
        );
        assert_eq!(response["resolved_target"]["kind"], "code_unit");
        assert_eq!(response["resolved_target"]["path"], "src/routes/a.ts");
        assert_eq!(
            response["resolved_target"]["candidate_paths"][0],
            "src/routes/a.ts"
        );
        assert_eq!(
            response["resolved_target"]["candidate_code_unit_ids"][0],
            "unit:src/routes/a.ts#express_route:get:0-20:1"
        );
        assert_eq!(response["resolved_target"]["confidence"], "high");
        assert_eq!(
            response["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
        assert_eq!(response["read_plan"]["source_snippets_included"], false);
        assert_eq!(response["source_spans"]["requested"], false);
        assert_eq!(
            response["unknowns"][0]["affected_claim"],
            "pattern family evidence for resolved target"
        );
        // MCP parity: a PARTIAL_CONTEXT response carries the same ESTIMATED
        // savings block the CLI does. The 4 KiB resolved TypeScript file yields a
        // nonzero whole-file baseline against the 20-byte read plan.
        let savings = &response["estimated_potential_token_savings"];
        assert_eq!(savings["outcome_shape"], "partial_context");
        assert_eq!(savings["language"], "typescript/javascript");
        assert!(
            savings["estimated_potential_token_savings"]
                .as_u64()
                .expect("savings")
                > 0
        );
        assert_eq!(
            savings["estimated_potential_token_savings_kind"],
            "ESTIMATED"
        );
        assert!(savings["estimated_potential_token_savings_caveat"]
            .as_str()
            .expect("caveat")
            .contains("not measured token savings"));
        assert!(!text.contains("/tmp/repogrammar"));
        assert!(!text.contains("export const"));
    }

    // `check_conformance` now returns a static-alignment certificate and never
    // the legacy `PARTIAL_CONTEXT`/advisory shape, so the former advisory
    // partial-context test was removed. The static-alignment abstention surface
    // is covered end-to-end by the binary integration tests.

    #[test]
    fn active_family_compact_response_has_no_absolute_path_source_snippet_or_evidence() {
        let runtime = FakeMcpRuntime::ready_found();

        // This exercises the family-detail compact rendering and its source-free
        // guarantees; `find_analogues` is the operation that returns family
        // detail (`check_conformance` now returns a static-alignment certificate).
        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "find_analogues", "target": "src/routes/a.ts"}),
        )
        .expect("family response");
        let text = response.to_string();

        assert_eq!(response["status"], "ok");
        assert_eq!(response["query_route"]["route"], "discover_hydrate_compose");
        assert_eq!(
            response["query_route"]["follow_up_family_ids"],
            json!(["family:typescript:express_route:express"])
        );
        assert!(response["check"].is_null());
        assert_eq!(response["output"]["mode"], "compact");
        assert_eq!(response["output"]["estimated_evidence_tokens"], 0);
        assert!(
            response["output"]["estimated_read_plan_tokens"]
                .as_u64()
                .expect("read plan tokens")
                > 0
        );
        assert!(
            response["output"]["estimated_potential_token_savings"]
                .as_u64()
                .expect("estimated potential")
                > 0
        );
        assert_eq!(
            response["output"]["estimated_potential_token_savings_kind"],
            "ESTIMATED"
        );
        assert!(
            response["output"]["estimated_potential_token_savings_caveat"]
                .as_str()
                .expect("estimated caveat")
                .contains("not measured token savings")
        );
        assert_eq!(response["output"]["source_snippets_included"], false);
        assert_eq!(response["source_spans"]["requested"], false);
        assert_eq!(runtime.source_span_calls(), 0);
        assert!(response["evidence"]
            .as_array()
            .expect("evidence")
            .is_empty());
        // The member list is bounded metadata: the true total and the truncation
        // flag ship alongside the (here single-member) array.
        assert_eq!(response["member_count"], 1);
        assert_eq!(response["members_truncated"], false);
        assert_eq!(response["members"].as_array().expect("members").len(), 1);
        assert_eq!(response["read_plan"]["source_snippets_included"], false);
        assert_eq!(response["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(
            response["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
        assert_eq!(response["read_plan"]["items"][0]["path"], "src/routes/a.ts");
        assert_eq!(response["read_plan"]["items"][0]["start_byte"], 0);
        assert_eq!(response["read_plan"]["items"][0]["end_byte"], 20);
        assert_eq!(response["read_plan"]["items"][0]["start_line"], 1);
        assert_eq!(response["read_plan"]["items"][0]["end_line"], 2);
        assert!(response["read_plan"]["line_range_omissions"]
            .as_array()
            .expect("line range omissions")
            .is_empty());
        assert_eq!(
            response["read_plan"]["items"][0]["content_hash"],
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            response["read_plan"]["items"][0]["source_snippets_included"],
            false
        );
        assert!(!text.contains("/tmp/repogrammar"));
        assert!(!text.contains("export const"));
    }

    #[test]
    fn found_family_value_bounds_large_member_list_outside_deep_mode() {
        // The recorded live hazard: a 123-member family inflating a single MCP
        // response. Outside `--mode deep` the inline list is capped and the true
        // total plus a truncation flag ship alongside it; deep mode restores the
        // full deterministic list.
        let mut family = family_detail();
        let total = crate::application::query::MAX_RENDERED_FAMILY_MEMBERS + 103;
        family.members = (0..total)
            .map(|index| IndexedFamilyMemberRecord {
                family_id: family.family_id.clone(),
                code_unit_id: format!("unit:src/routes/a.ts#express_route:get:{index}"),
                role: "framework:express.route_handler".to_string(),
            })
            .collect();
        let options = FamilyOutputOptions::default();
        let read_plan = build_read_plan(
            &family,
            Some("src/routes/a.ts"),
            FamilyLookupMode::FuzzyQuery,
            options,
        );
        let route = family_query_route_report(
            &FamilyLookupReport::Found(family.clone()),
            FamilyLookupMode::FuzzyQuery,
        );

        let value = family_detail_value(
            McpOperation::FindAnalogues,
            &family,
            &route,
            &read_plan,
            options,
            None,
        );
        assert_eq!(value["member_count"], total);
        assert_eq!(value["members_truncated"], true);
        assert_eq!(
            value["members"].as_array().expect("members").len(),
            crate::application::query::MAX_RENDERED_FAMILY_MEMBERS
        );

        let deep_options = FamilyOutputOptions {
            evidence_mode: FamilyEvidenceMode::Deep,
            ..options
        };
        let deep = family_detail_value(
            McpOperation::FindAnalogues,
            &family,
            &route,
            &read_plan,
            deep_options,
            None,
        );
        assert_eq!(deep["member_count"], total);
        assert_eq!(deep["members_truncated"], false);
        assert_eq!(deep["members"].as_array().expect("members").len(), total);
    }

    #[test]
    fn active_family_source_spans_require_explicit_opt_in() {
        let runtime = FakeMcpRuntime::ready_found_with_source_spans();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({
                "operation": "check_conformance",
                "target": "src/routes/a.ts",
                "include_source_spans": true
            }),
        )
        .expect("family response");

        assert_eq!(runtime.source_span_calls(), 1);
        assert_eq!(response["output"]["source_snippets_included"], true);
        assert_eq!(response["read_plan"]["source_snippets_included"], true);
        assert_eq!(
            response["read_plan"]["items"][0]["source_snippets_included"],
            true
        );
        assert_eq!(response["read_plan"]["items"][0]["start_line"], 1);
        assert_eq!(response["read_plan"]["items"][0]["end_line"], 2);
        assert_eq!(response["source_spans"]["requested"], true);
        assert_eq!(response["source_spans"]["source_snippets_included"], true);
        assert_eq!(
            response["source_spans"]["spans"][0]["text"],
            "1\texport const handler = () => {\n2\t  return ok\n"
        );
        assert_eq!(
            response["source_spans"]["omissions"][0]["reason"],
            "stale_evidence"
        );
    }

    #[test]
    fn active_family_explicit_compact_mode_overrides_token_budget() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
                "mode": "compact",
                "token_budget": 1
            }),
        )
        .expect("family response");

        assert_eq!(response["status"], "ok");
        assert_eq!(response["output"]["mode"], "compact");
        assert_eq!(response["output"]["token_budget"], 1);
        assert!(response["evidence"]
            .as_array()
            .expect("evidence")
            .is_empty());
        assert_eq!(
            response["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
    }

    #[test]
    fn active_family_evidence_mode_returns_budgeted_metadata_without_source_snippet() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
                "mode": "evidence",
                "token_budget": 1
            }),
        )
        .expect("family response");
        let text = response.to_string();

        assert_eq!(response["status"], "ok");
        // The MCP detail payload exposes the metadata-only prevalence object.
        assert_eq!(
            response["family"]["prevalence"]["classification_reason"],
            "coverage 2/2 with no competing ready family"
        );
        assert_eq!(response["family"]["prevalence"]["coverage_ratio"], 1.0);
        assert_eq!(
            response["family"]["prevalence"]["supported_member_count"],
            2
        );
        assert_eq!(response["output"]["mode"], "evidence");
        assert_eq!(response["output"]["token_budget"], 1);
        assert_eq!(
            response["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_eq!(
            response["output"]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert_eq!(response["output"]["missing_claims"], json!([]));
        assert_eq!(response["output"]["budget_satisfied"], false);
        assert_eq!(response["output"]["source_snippets_included"], false);
        assert_eq!(response["read_plan"]["source_snippets_included"], false);
        assert_eq!(response["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(response["read_plan"]["budget_satisfied"], false);
        assert_eq!(response["evidence"][0]["path"], "src/routes/a.ts");
        assert_eq!(response["read_plan"]["items"][0]["path"], "src/routes/a.ts");
        assert_eq!(
            response["evidence"][0]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert!(
            response["output"]["estimated_evidence_tokens"]
                .as_u64()
                .expect("estimated tokens")
                > 1
        );
        assert!(!text.contains("/tmp/repogrammar"));
        assert!(!text.contains("export const"));
    }

    #[test]
    fn supported_operations_include_metadata_only_read_plan() {
        let runtime = FakeMcpRuntime::ready_found();

        for (operation, target, requires_source) in [
            ("find_analogues", "src/routes/a.ts", true),
            (
                "show_family",
                "family:typescript:express_route:express",
                false,
            ),
            ("explain_deviation", "src/routes/a.ts", true),
            ("check_conformance", "src/routes/a.ts", true),
        ] {
            let response = handle_context_call(
                &runtime,
                &context(),
                &json!({"operation": operation, "target": target}),
            )
            .expect("family response");
            let text = response.to_string();

            assert_eq!(response["read_plan"]["source_snippets_included"], false);
            assert_eq!(
                response["read_plan"]["requires_source_before_edit"],
                requires_source
            );
            assert!(!response["read_plan"]["items"]
                .as_array()
                .expect("read plan items")
                .is_empty());
            assert_eq!(response["read_plan"]["items"][0]["path"], "src/routes/a.ts");
            assert!(!text.contains("/tmp/repogrammar"));
            assert!(!text.contains("export const"));
        }
    }

    #[test]
    fn active_family_include_flags_report_uncovered_variations_and_exceptions() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
                "mode": "evidence",
                "include_variations": true,
                "include_exceptions": true
            }),
        )
        .expect("family response");

        assert_eq!(response["status"], "ok");
        assert_eq!(
            response["output"]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert_eq!(
            response["output"]["missing_claims"],
            json!(["variation", "exception"])
        );
        assert_eq!(
            response["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
    }

    #[test]
    fn context_call_records_source_free_query_outcome_rollups() {
        let workspace = TempWorkspace::new("mcp-query-outcome-rollup");
        let context = context_for_workspace(&workspace);

        let found = handle_context_call(
            &FakeMcpRuntime::ready_found(),
            &context,
            &json!({"operation": "check_conformance", "target": "src/routes/a.ts"}),
        )
        .expect("found response");
        // This exercises the query-outcome telemetry rollup: the fake returns a
        // resolved family report so the check_conformance category records a
        // `found` outcome.
        assert_eq!(found["status"], "ok");

        let partial = handle_context_call(
            &FakeMcpRuntime::ready_partial_context(),
            &context,
            &json!({"operation": "find_analogues", "target": "src/routes/a.ts missing_family"}),
        )
        .expect("partial response");
        assert_eq!(partial["status"], "PARTIAL_CONTEXT");

        let unknown = handle_context_call(
            &FakeMcpRuntime::ready_unknown(),
            &context,
            &json!({"operation": "explain_deviation", "target": "src/routes/missing.ts"}),
        )
        .expect("unknown response");
        assert_eq!(unknown["status"], "UNKNOWN");

        let fallback = handle_context_call(
            &FakeMcpRuntime::initialized_without_generation(),
            &context,
            &json!({"operation": "show_family", "target": "family:routes"}),
        )
        .expect("fallback response");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");

        let rollup_path = workspace
            .path()
            .join(".repogrammar")
            .join("telemetry")
            .join("local-metrics")
            .join("family_query_outcomes.json");
        let rollup: Value =
            serde_json::from_str(&fs::read_to_string(rollup_path).expect("query rollup JSON"))
                .expect("query rollup");
        assert_eq!(rollup["schema_version"], "family-query-outcomes.v1");
        assert_eq!(rollup["event_count"], 4);
        assert_eq!(rollup["by_entrypoint"]["mcp"], 4);
        assert_eq!(rollup["by_status"]["found"], 1);
        assert_eq!(rollup["by_status"]["partial_context"], 1);
        assert_eq!(rollup["by_status"]["unknown"], 1);
        assert_eq!(rollup["by_status"]["fallback"], 1);
        assert_eq!(rollup["by_command_category"]["check_conformance"], 1);
        assert_eq!(rollup["by_command_category"]["find_analogues"], 1);
        assert_eq!(rollup["by_command_category"]["explain_deviation"], 1);
        assert_eq!(rollup["by_command_category"]["show_family"], 1);
        assert_eq!(rollup["by_lookup_mode"]["fuzzy"], 3);
        assert_eq!(rollup["by_lookup_mode"]["exact_family"], 1);
        assert_eq!(rollup["read_plan_returned_count"], 2);
        assert_eq!(rollup["read_plan_item_count_bucket"]["1-2"], 2);
        assert_eq!(rollup["by_reason_code"]["FrameworkMagic"], 1);
        assert_eq!(rollup["by_reason_code"]["InsufficientSupport"], 2);
        assert_eq!(
            rollup["by_required_mechanism"]["compatible_support_evidence"],
            2
        );
        let serialized = rollup.to_string();
        assert!(!serialized.contains("src/routes/a.ts"));
        assert!(!serialized.contains("family:routes"));
        assert!(!serialized.contains("sha256:"));
        assert!(!serialized.contains("export const"));
    }

    #[test]
    fn active_family_deep_mode_is_metadata_only_until_span_reader_exists() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
                "mode": "deep"
            }),
        )
        .expect("family response");

        assert_eq!(response["status"], "ok");
        assert_eq!(response["output"]["mode"], "deep");
        assert_eq!(response["output"]["source_snippets_included"], false);
        assert_eq!(response["read_plan"]["source_snippets_included"], false);
        assert_eq!(response["evidence"][0]["path"], "src/routes/a.ts");
    }

    #[test]
    fn show_family_uses_exact_family_id_lookup_mode() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "show_family", "target": "src/routes/a.ts"}),
        )
        .expect("UNKNOWN response");

        assert_eq!(response["status"], "UNKNOWN");
        assert_eq!(response["query_route"]["route"], "exact_lookup_unknown");
        assert_eq!(
            response["query_route"]["family_id_policy"],
            "show_family_requires_exact_family_id"
        );
        assert_eq!(response["unknowns"][0]["reason"], "InsufficientSupport");
        assert_eq!(runtime.lookup_calls(), 1);
    }

    #[test]
    fn invalid_operation_and_blank_target_are_protocol_errors() {
        let runtime = FakeMcpRuntime::ready_unknown();

        let error = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "callers", "target": "src/a.ts"}),
        )
        .expect_err("invalid operation");
        assert_eq!(error.code(), -32602);

        let error = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "find_analogues", "target": "   "}),
        )
        .expect_err("blank target");
        assert_eq!(error.code(), -32602);

        let error = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "find_analogues", "target": "src/a.py", "mode": "source"}),
        )
        .expect_err("invalid mode");
        assert_eq!(error.code(), -32602);
    }

    #[test]
    fn json_rpc_lists_context_tool_and_wraps_tool_result() {
        let runtime = FakeMcpRuntime::ready_unknown();
        let context = context();

        let initialize = handle_json_rpc_value(
            &runtime,
            &context,
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize"}),
        );
        let initialize_response = initialize.response.expect("initialize response");
        let instructions = initialize_response["result"]["instructions"]
            .as_str()
            .expect("initialize instructions");
        assert!(instructions.contains("pre-flight gate"));
        assert!(instructions.contains("before any non-trivial code location"));
        assert!(instructions.contains("operation: \"find_analogues\""));
        assert!(instructions.contains("repo-relative path, symbol/member id"));
        assert!(instructions.contains("mode: \"compact\""));
        assert!(instructions.contains("CodeGraph"));
        assert!(instructions.contains("State that fallback reason"));
        assert!(instructions.contains("Do not repeat the same RepoGrammar call"));
        assert!(instructions.contains("show_family"));
        assert!(instructions.contains("include_source_spans"));
        assert!(instructions.contains("repogrammar stats"));
        assert!(instructions.contains("Never initialize"));

        let list = handle_json_rpc_value(
            &runtime,
            &context,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        );
        let list_response = list.response.expect("list response");
        let tools = list_response["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "repogrammar_context");

        let call = handle_json_rpc_value(
            &runtime,
            &context,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "repogrammar_context",
                    "arguments": {
                        "operation": "find_analogues",
                        "target": "src/routes/a.ts"
                    }
                }
            }),
        );
        let response = call.response.expect("call response");
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text");
        let payload: Value = serde_json::from_str(text).expect("payload JSON");
        assert_eq!(payload["status"], "UNKNOWN");
    }

    #[test]
    fn json_rpc_rejects_unknown_tool_as_transport_error() {
        let runtime = FakeMcpRuntime::ready_unknown();

        let response = handle_json_rpc_value(
            &runtime,
            &context(),
            json!({
                "jsonrpc": "2.0",
                "id": "bad-tool",
                "method": "tools/call",
                "params": {
                    "name": "find",
                    "arguments": {"operation": "find_analogues"}
                }
            }),
        )
        .response
        .expect("error response");

        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn serve_json_lines_handles_initialize_list_call_and_shutdown() {
        let runtime = FakeMcpRuntime::ready_unknown();
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"repogrammar_context","arguments":{"operation":"find_analogues","target":"src/a.ts"}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
            "",
        ]
        .join("\n");
        let mut output = Vec::new();

        serve_json_lines(&runtime, &context(), input.as_bytes(), &mut output).expect("serve lines");
        let lines = String::from_utf8(output).expect("utf8 output");
        let responses = lines.lines().collect::<Vec<_>>();

        assert_eq!(responses.len(), 4);
        let initialize: Value = serde_json::from_str(responses[0]).expect("initialize JSON");
        assert_eq!(initialize["result"]["serverInfo"]["name"], "repogrammar");
        let list: Value = serde_json::from_str(responses[1]).expect("list JSON");
        let tools = list["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "repogrammar_context");
        let call: Value = serde_json::from_str(responses[2]).expect("call JSON");
        assert_eq!(call["result"]["isError"], false);
        let shutdown: Value = serde_json::from_str(responses[3]).expect("shutdown JSON");
        assert!(shutdown["result"].is_null());
    }

    #[test]
    fn serve_json_lines_rejects_oversized_unterminated_line() {
        let runtime = FakeMcpRuntime::ready_unknown();
        // A line one byte past the limit with no trailing newline. The bounded
        // read must reject it rather than buffering an unbounded amount.
        let input = "x".repeat(MAX_MCP_LINE_BYTES + 1);
        let mut output = Vec::new();

        let error = serve_json_lines(&runtime, &context(), input.as_bytes(), &mut output)
            .expect_err("oversized line must be rejected");
        match error {
            RepoGrammarError::InvalidInput(message) => {
                assert!(message.contains("exceeds the 1 MiB limit"), "{message}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    fn context() -> McpServeContext {
        McpServeContext {
            repository_root: "/tmp/repogrammar-project".to_string(),
            state_dir_override: None,
        }
    }

    fn context_for_workspace(workspace: &TempWorkspace) -> McpServeContext {
        fs::create_dir_all(workspace.path().join(".repogrammar")).expect("state dir");
        McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        }
    }

    struct FakeMcpRuntime {
        status: RepositoryStatusReport,
        lookup: FamilyLookupReport,
        lookup_calls: std::cell::Cell<usize>,
        source_spans: Option<SourceSpanRenderReport>,
        source_span_calls: std::cell::Cell<usize>,
    }

    impl FakeMcpRuntime {
        fn not_initialized() -> Self {
            Self::new(RepositoryStatus::NotInitialized, unknown_report())
        }

        fn initialized_without_generation() -> Self {
            Self::new(
                RepositoryStatus::Initialized {
                    active_generation: "none".to_string(),
                },
                unknown_report(),
            )
        }

        fn ready_unknown() -> Self {
            Self::new(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                unknown_report(),
            )
        }

        fn ready_found() -> Self {
            Self::new(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                FamilyLookupReport::Found(family_detail()),
            )
        }

        fn ready_partial_context() -> Self {
            Self::new(
                RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                partial_context_report(),
            )
        }

        fn ready_found_with_source_spans() -> Self {
            let mut runtime = Self::ready_found();
            runtime.source_spans = Some(source_span_report());
            runtime
        }

        fn new(status: RepositoryStatus, lookup: FamilyLookupReport) -> Self {
            let initialized = matches!(status, RepositoryStatus::Initialized { .. });
            let active_generation_available = matches!(
                &status,
                RepositoryStatus::Initialized { active_generation }
                    if active_generation != "none" && active_generation != "not implemented"
            );
            let recovery = if !initialized {
                crate::application::recovery::RecoveryRecommendation {
                    action: crate::application::recovery::RecoveryAction::Setup,
                    reason: crate::application::recovery::RecoveryReason::RepositoryNotInitialized,
                }
            } else if !active_generation_available {
                crate::application::recovery::RecoveryRecommendation {
                    action: crate::application::recovery::RecoveryAction::Resync,
                    reason: crate::application::recovery::RecoveryReason::ActiveIndexMissing,
                }
            } else {
                crate::application::recovery::RecoveryRecommendation {
                    action: crate::application::recovery::RecoveryAction::None,
                    reason: crate::application::recovery::RecoveryReason::Ready,
                }
            };
            let readiness = RepositoryReadiness {
                active_generation_available,
                query_ready: active_generation_available,
                recovery: Some(recovery),
                ..RepositoryReadiness::default()
            };
            Self {
                status: RepositoryStatusReport {
                    state_dir: ".repogrammar".to_string(),
                    status,
                    manifest: crate::application::repository::RepositoryManifestStatus::Valid,
                    manifest_schema_version: Some(1),
                    missing_subdirs: Vec::new(),
                    storage: RepositoryImplementationStatus::Available,
                    indexing: RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
                    storage_inspection: None,
                    storage_error: None,
                    readiness,
                },
                lookup,
                lookup_calls: std::cell::Cell::new(0),
                source_spans: None,
                source_span_calls: std::cell::Cell::new(0),
            }
        }

        fn lookup_calls(&self) -> usize {
            self.lookup_calls.get()
        }

        fn source_span_calls(&self) -> usize {
            self.source_span_calls.get()
        }
    }

    impl McpReadOnlyRuntime for FakeMcpRuntime {
        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            Ok(self.status.clone())
        }

        fn family_lookup(
            &self,
            _request: RepositoryStatusRequest,
            target: Option<&str>,
            mode: FamilyLookupMode,
        ) -> Result<FamilyLookupReport, RepoGrammarError> {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            if mode == FamilyLookupMode::ExactFamilyId
                && target != Some("family:typescript:express_route:express")
            {
                return Ok(unknown_report());
            }
            Ok(self.lookup.clone())
        }

        fn product_readiness(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<ProductReadinessReport, RepoGrammarError> {
            // A fixture with one stale family so the bounded readiness output can be
            // asserted to surface the degraded-with-stale-count case.
            Ok(crate::application::query::assemble_product_readiness(
                &self.status,
                Some(crate::application::query::FamilyFreshnessCounts {
                    fresh_count: 2,
                    stale_count: 1,
                    cannot_verify_count: 0,
                }),
                Some(crate::application::query::FamilyPrevalenceReadiness::default()),
                crate::application::query::StaticAlignmentReadiness::NotApplicable,
                Vec::new(),
                false,
                Some(Vec::new()),
            ))
        }

        fn render_source_spans(
            &self,
            _request: RepositoryStatusRequest,
            _read_plan: &ReadPlan,
            include_source_spans: bool,
            _token_budget: Option<usize>,
        ) -> Result<SourceSpanRenderReport, RepoGrammarError> {
            self.source_span_calls.set(self.source_span_calls.get() + 1);
            if !include_source_spans {
                return Err(RepoGrammarError::InvalidInput(
                    "source spans were not requested".to_string(),
                ));
            }
            self.source_spans
                .clone()
                .ok_or(RepoGrammarError::NotImplemented("source spans"))
        }

        fn enrich_read_plan_line_ranges(
            &self,
            _request: RepositoryStatusRequest,
            read_plan: &ReadPlan,
        ) -> Result<ReadPlan, RepoGrammarError> {
            let mut enriched = read_plan.clone();
            for item in &mut enriched.items {
                if item.path == "src/routes/a.ts" && item.start_byte == 0 && item.end_byte == 20 {
                    item.start_line = Some(1);
                    item.end_line = Some(2);
                }
            }
            Ok(enriched)
        }
    }

    fn unknown_report() -> FamilyLookupReport {
        FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: "gen-000001".to_string(),
            candidate_family_ids: Vec::new(),
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::Blocking,
                reason: UnknownReasonCode::InsufficientSupport,
                affected_claim: "query target".to_string(),
                recovery: Some(
                    "run repogrammar resync after adding compatible implementations".to_string(),
                ),
            }],
            term_retrieval: None,
        })
    }

    fn partial_context_report() -> FamilyLookupReport {
        let hash = ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash");
        let why = "read this resolved target body before editing; no pattern-family evidence is available";
        FamilyLookupReport::PartialContext(Box::new(FamilyPartialContextReport {
            active_generation: "gen-000001".to_string(),
            resolved_target: ResolvedQueryTarget {
                original_target: "src/routes/a.ts missing_family".to_string(),
                kind: "code_unit",
                path: "src/routes/a.ts".to_string(),
                line: None,
                byte_range: None,
                family_id: None,
                code_unit_id: Some("unit:src/routes/a.ts#express_route:get:0-20:1".to_string()),
                symbol_hints: Vec::new(),
                residue_terms: vec!["missing_family".to_string()],
                candidate_paths: vec!["src/routes/a.ts".to_string()],
                candidate_family_ids: Vec::new(),
                candidate_code_unit_ids: vec![
                    "unit:src/routes/a.ts#express_route:get:0-20:1".to_string(),
                ],
                confidence: "high",
                match_kind: "path_embedded",
            },
            read_plan: ReadPlan {
                items: vec![ReadPlanItem {
                    purpose: ReadPlanPurpose::TargetBodyRequiredForEdit,
                    path: "src/routes/a.ts".to_string(),
                    content_hash: hash,
                    start_byte: 0,
                    end_byte: 20,
                    start_line: None,
                    end_line: None,
                    estimated_tokens: 42,
                    why: why.to_string(),
                    source_required_before_edit: true,
                    source_snippets_included: false,
                }],
                estimated_tokens: 42,
                source_snippets_included: false,
                requires_source_before_edit: true,
                selection_strategy: "deterministic_local_context_v1",
                budget_satisfied: true,
                line_range_omissions: Vec::new(),
            },
            // A 4 KiB TypeScript file resolves to a 20-byte read-plan span: the
            // whole-file baseline (1024 tokens) far exceeds the returned read
            // plan, so this PARTIAL_CONTEXT carries a nonzero savings estimate.
            resolved_file_size_bytes: Some(4096),
            resolved_file_language: "typescript".to_string(),
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::Blocking,
                reason: UnknownReasonCode::InsufficientSupport,
                affected_claim: "pattern family evidence for resolved target".to_string(),
                recovery: Some(
                    "treat this as source-reading context only; rerun repogrammar resync after compatible family evidence exists"
                        .to_string(),
                ),
            }],
        }))
    }

    fn family_detail() -> FamilyDetailReport {
        let hash = ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash");
        FamilyDetailReport {
            active_generation: "gen-000001".to_string(),
            family_id: "family:typescript:express_route:express".to_string(),
            classification: "DOMINANT_PATTERN".to_string(),
            support: 2,
            prevalence: crate::test_support::sample_family_prevalence(),
            members: vec![IndexedFamilyMemberRecord {
                family_id: "family:typescript:express_route:express".to_string(),
                code_unit_id: "unit:src/routes/a.ts#express_route:get:0-20:1".to_string(),
                role: "framework:express.route_handler".to_string(),
            }],
            variation_slots: vec![IndexedVariationSlotRecord {
                family_id: "family:typescript:express_route:express".to_string(),
                slot_id: "slot:runtime_unknown".to_string(),
                description: "non_blocking_unknown:FrameworkMagic".to_string(),
            }],
            evidence: vec![IndexedFamilyEvidenceRecord {
                evidence_id: "family-evidence:000000".to_string(),
                family_id: "family:typescript:express_route:express".to_string(),
                code_unit_id: "unit:src/routes/a.ts#express_route:get:0-20:1".to_string(),
                covered_claims: vec!["canonical".to_string(), "support".to_string()],
                path: "src/routes/a.ts".to_string(),
                content_hash: hash,
                start_byte: 0,
                end_byte: 20,
                note: "DOMINANT_PATTERN support evidence".to_string(),
            }],
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::NonBlocking,
                reason: UnknownReasonCode::FrameworkMagic,
                affected_claim: "runtime_equivalence".to_string(),
                recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
            }],
            constraint_profile: None,
            term_retrieval: None,
        }
    }

    fn source_span_report() -> SourceSpanRenderReport {
        let hash = ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash");
        SourceSpanRenderReport {
            policy: SourceSpanPolicy {
                requested: true,
                source_snippets_included: true,
                estimated_tokens: 6,
                budget_satisfied: true,
                selection_strategy: "hash_checked_line_numbered_spans_v1",
                fallback_guidance: "use rendered source spans only for the listed byte ranges; use normal Read before editing outside them",
            },
            spans: vec![RenderedSourceSpan {
                purpose: ReadPlanPurpose::TargetBodyRequiredForEdit,
                path: "src/routes/a.ts".to_string(),
                content_hash: hash,
                start_byte: 0,
                end_byte: 20,
                start_line: 1,
                end_line: 2,
                estimated_tokens: 6,
                why: "read this target body before editing; family metadata is context only"
                    .to_string(),
                source_required_before_edit: true,
                text: "1\texport const handler = () => {\n2\t  return ok\n".to_string(),
            }],
            omissions: vec![SourceSpanOmission {
                purpose: ReadPlanPurpose::SupportEvidence,
                path: "src/routes/stale.ts".to_string(),
                start_byte: 0,
                end_byte: 10,
                reason: "stale_evidence",
                guidance: "source changed or disappeared; use normal Read/Grep for this span",
            }],
        }
    }
}
