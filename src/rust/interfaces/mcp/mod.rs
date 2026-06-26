//! Transport-neutral MCP contract and read-only JSON-RPC stdio handling.

use crate::application::query::{
    build_read_plan, query_preflight, repository_status_unavailable_fallback,
    select_family_evidence, validate_query_target, validate_query_token_budget, FamilyDetailReport,
    FamilyEvidenceMode, FamilyLookupMode, FamilyLookupReport, FamilyOutputOptions,
    FamilyQueryUnknown, QueryPreflightOperation, QueryPreflightReport, ReadPlan, ReadPlanItem,
    MAX_QUERY_TARGET_BYTES, MAX_QUERY_TOKEN_BUDGET,
};
use crate::application::repository::{RepositoryStatusReport, RepositoryStatusRequest};
use crate::error::RepoGrammarError;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_LINE_BYTES: usize = 1_048_576;
pub const MAX_MCP_TARGET_BYTES: usize = MAX_QUERY_TARGET_BYTES;
pub const MAX_MCP_TOKEN_BUDGET: usize = MAX_QUERY_TOKEN_BUDGET;

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
}

impl McpOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "find_analogues" => Some(Self::FindAnalogues),
            "show_family" => Some(Self::ShowFamily),
            "explain_deviation" => Some(Self::ExplainDeviation),
            "check_conformance" => Some(Self::CheckConformance),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FindAnalogues => "find_analogues",
            Self::ShowFamily => "show_family",
            Self::ExplainDeviation => "explain_deviation",
            Self::CheckConformance => "check_conformance",
        }
    }

    fn cli_command(self) -> &'static str {
        match self {
            Self::FindAnalogues => "find",
            Self::ShowFamily => "family",
            Self::ExplainDeviation => "explain",
            Self::CheckConformance => "check",
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
}

pub fn tool_schema() -> Value {
    json!({
        "name": McpToolName::Context.as_str(),
        "description": "Read-only RepoGrammar pattern-family context",
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
                "include_variations": {
                    "type": "boolean",
                },
                "include_exceptions": {
                    "type": "boolean",
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
    let status_report = match runtime.repository_status(request.clone()) {
        Ok(report) => report,
        Err(_) => {
            return Ok(fallback_value(
                arguments.operation,
                repository_status_unavailable_fallback(QueryPreflightOperation::PatternFamilyQuery),
            ));
        }
    };

    match query_preflight(QueryPreflightOperation::PatternFamilyQuery, &status_report) {
        QueryPreflightReport::Fallback(fallback) => {
            Ok(fallback_value(arguments.operation, fallback))
        }
        QueryPreflightReport::Ready => match runtime.family_lookup(
            request,
            arguments.target.as_deref(),
            lookup_mode_for_operation(arguments.operation),
        ) {
            Ok(FamilyLookupReport::Found(family)) => Ok(family_detail_value(
                arguments.operation,
                &family,
                arguments.target.as_deref(),
                lookup_mode_for_operation(arguments.operation),
                arguments.output_options,
            )),
            Ok(FamilyLookupReport::Unknown(report)) => Ok(json!({
                "operation": arguments.operation.as_str(),
                "command": arguments.operation.cli_command(),
                "status": "UNKNOWN",
                "implemented": true,
                "active_generation": report.active_generation,
                "unknowns": unknowns_value(&report.unknowns),
            })),
            Err(_) => Ok(fallback_value(
                arguments.operation,
                repository_status_unavailable_fallback(QueryPreflightOperation::PatternFamilyQuery),
            )),
        },
    }
}

fn lookup_mode_for_operation(operation: McpOperation) -> FamilyLookupMode {
    match operation {
        McpOperation::ShowFamily => FamilyLookupMode::ExactFamilyId,
        McpOperation::FindAnalogues
        | McpOperation::ExplainDeviation
        | McpOperation::CheckConformance => FamilyLookupMode::FuzzyQuery,
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
        let bytes = reader.read_line(&mut line).map_err(|error| {
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
                        "instructions": "RepoGrammar MCP is read-only; call repogrammar_context for evidence-backed pattern-family context or typed UNKNOWN.",
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
                | "include_variations"
                | "include_exceptions"
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
                "repogrammar_context operation must be one of find_analogues, show_family, explain_deviation, or check_conformance",
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
    for field in ["include_variations", "include_exceptions"] {
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
    Ok(ContextArguments {
        operation,
        target,
        output_options: FamilyOutputOptions {
            evidence_mode,
            token_budget,
            include_variations,
            include_exceptions,
        },
    })
}

fn fallback_value(
    operation: McpOperation,
    fallback: crate::application::query::QueryFallbackReport,
) -> Value {
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "status": "FALLBACK_TO_CODE_SEARCH",
        "reason": fallback.reason,
        "guidance": fallback.guidance,
        "implemented": fallback.implemented,
    })
}

fn family_detail_value(
    operation: McpOperation,
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
) -> Value {
    let selected_evidence = select_family_evidence(family, options);
    let read_plan = build_read_plan(family, target, mode, options);
    let check = if operation == McpOperation::CheckConformance {
        Some(json!({
            "advisory_status": "UNKNOWN",
            "reason": "runtime equivalence remains unproven",
            "fail_on": "none",
        }))
    } else {
        None
    };
    json!({
        "operation": operation.as_str(),
        "command": operation.cli_command(),
        "status": if operation == McpOperation::CheckConformance { "CONTEXT_ONLY" } else { "ok" },
        "implemented": true,
        "active_generation": family.active_generation,
        "family": {
            "family_id": family.family_id,
            "classification": family.classification,
            "support": family.support,
        },
        "output": {
            "mode": selected_evidence.mode.as_str(),
            "token_budget": selected_evidence.token_budget,
            "estimated_evidence_tokens": selected_evidence.estimated_tokens,
            "estimated_read_plan_tokens": read_plan.estimated_tokens,
            "selection_strategy": selected_evidence.selection_strategy,
            "budget_satisfied": selected_evidence.budget_satisfied,
            "covered_claims": selected_evidence.covered_claims,
            "missing_claims": selected_evidence.missing_claims,
            "source_snippets_included": selected_evidence.source_snippets_included,
        },
        "members": family.members.iter().map(|member| {
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
        "read_plan": read_plan_value(&read_plan),
        "unknowns": unknowns_value(&family.unknowns),
        "check": check,
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
    use crate::application::repository::{RepositoryImplementationStatus, RepositoryStatus};
    use crate::core::model::{ContentHash, UnknownClass, UnknownReasonCode};
    use crate::ports::family_store::{
        IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedVariationSlotRecord,
    };

    #[test]
    fn tool_names_match_bootstrap_contract() {
        assert_eq!(McpToolName::Context.as_str(), "repogrammar_context");
        assert_eq!(McpOperation::FindAnalogues.as_str(), "find_analogues");
        assert_eq!(McpOperation::ShowFamily.as_str(), "show_family");
        assert_eq!(McpOperation::ExplainDeviation.as_str(), "explain_deviation");
        assert_eq!(McpOperation::CheckConformance.as_str(), "check_conformance");
    }

    #[test]
    fn tool_schema_exposes_only_default_context_tool_shape() {
        let schema = tool_schema();

        assert_eq!(schema["name"], "repogrammar_context");
        assert_eq!(
            schema["inputSchema"]["properties"]["operation"]["enum"],
            json!([
                "find_analogues",
                "show_family",
                "explain_deviation",
                "check_conformance"
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
    }

    #[test]
    fn context_arguments_reject_oversized_or_control_text_inputs() {
        let runtime = FakeMcpRuntime::ready_unknown();
        let oversized_target = "x".repeat(MAX_MCP_TARGET_BYTES + 1);
        for arguments in [
            json!({"operation": "find_analogues", "target": oversized_target}),
            json!({"operation": "find_analogues", "target": "contains\nnewline"}),
            json!({"operation": "find_analogues", "token_budget": MAX_MCP_TOKEN_BUDGET + 1}),
        ] {
            let error = handle_context_call(&runtime, &context(), &arguments)
                .expect_err("invalid query input must be rejected");

            assert_eq!(error.code(), -32602);
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
        assert_eq!(response["guidance"], "run repogrammar init");
        assert_eq!(response["implemented"], false);
        assert_eq!(runtime.lookup_calls(), 0);
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
        assert_eq!(response["guidance"], "run repogrammar index");
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
        assert_eq!(response["unknowns"][0]["reason"], "InsufficientSupport");
        assert_eq!(runtime.lookup_calls(), 1);
    }

    #[test]
    fn active_family_compact_response_has_no_absolute_path_source_snippet_or_evidence() {
        let runtime = FakeMcpRuntime::ready_found();

        let response = handle_context_call(
            &runtime,
            &context(),
            &json!({"operation": "check_conformance", "target": "src/routes/a.ts"}),
        )
        .expect("family response");
        let text = response.to_string();

        assert_eq!(response["status"], "CONTEXT_ONLY");
        assert_eq!(response["check"]["advisory_status"], "UNKNOWN");
        assert_eq!(response["output"]["mode"], "compact");
        assert_eq!(response["output"]["estimated_evidence_tokens"], 0);
        assert!(
            response["output"]["estimated_read_plan_tokens"]
                .as_u64()
                .expect("read plan tokens")
                > 0
        );
        assert_eq!(response["output"]["source_snippets_included"], false);
        assert!(response["evidence"]
            .as_array()
            .expect("evidence")
            .is_empty());
        assert_eq!(response["read_plan"]["source_snippets_included"], false);
        assert_eq!(response["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(
            response["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
        assert_eq!(response["read_plan"]["items"][0]["path"], "src/routes/a.ts");
        assert_eq!(response["read_plan"]["items"][0]["start_byte"], 0);
        assert_eq!(response["read_plan"]["items"][0]["end_byte"], 20);
        assert_eq!(response["read_plan"]["items"][0]["start_line"], Value::Null);
        assert_eq!(response["read_plan"]["items"][0]["end_line"], Value::Null);
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

        let list = handle_json_rpc_value(
            &runtime,
            &context,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        );
        assert_eq!(
            list.response.expect("list response")["result"]["tools"][0]["name"],
            "repogrammar_context"
        );

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
        assert_eq!(list["result"]["tools"][0]["name"], "repogrammar_context");
        let call: Value = serde_json::from_str(responses[2]).expect("call JSON");
        assert_eq!(call["result"]["isError"], false);
        let shutdown: Value = serde_json::from_str(responses[3]).expect("shutdown JSON");
        assert!(shutdown["result"].is_null());
    }

    fn context() -> McpServeContext {
        McpServeContext {
            repository_root: "/tmp/repogrammar-project".to_string(),
            state_dir_override: None,
        }
    }

    struct FakeMcpRuntime {
        status: RepositoryStatusReport,
        lookup: FamilyLookupReport,
        lookup_calls: std::cell::Cell<usize>,
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

        fn new(status: RepositoryStatus, lookup: FamilyLookupReport) -> Self {
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
                },
                lookup,
                lookup_calls: std::cell::Cell::new(0),
            }
        }

        fn lookup_calls(&self) -> usize {
            self.lookup_calls.get()
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
    }

    fn unknown_report() -> FamilyLookupReport {
        FamilyLookupReport::Unknown(FamilyUnknownReport {
            active_generation: "gen-000001".to_string(),
            unknowns: vec![FamilyQueryUnknown {
                class: UnknownClass::Blocking,
                reason: UnknownReasonCode::InsufficientSupport,
                affected_claim: "query target".to_string(),
                recovery: Some(
                    "run repogrammar index after adding compatible implementations".to_string(),
                ),
            }],
        })
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
        }
    }
}
