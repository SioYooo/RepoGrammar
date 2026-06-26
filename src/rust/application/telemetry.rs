//! Anonymous telemetry, research trace consent, and local token measurements.

use crate::application::query::DiagnosticSignal;
use crate::application::repository::{repository_state_location, RepositoryStatusRequest};
use crate::error::RepoGrammarError;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TELEMETRY_SCHEMA_VERSION: &str = "telemetry.v1";
pub const TELEMETRY_PREFERENCE_SCHEMA_VERSION: &str = "telemetry-preferences.v1";
pub const TOKEN_EXPERIMENT_SCHEMA_VERSION: &str = "token-experiment.v1";
pub const TELEMETRY_UPLOAD_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_TELEMETRY_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_STATE_FILE_BYTES: u64 = 1024 * 1024;
const MAX_EXPERIMENT_NAME_BYTES: usize = 128;
const COUNT_BUCKETS: &[&str] = &["0", "1-2", "3-9", "10-49", "50-199", "200+"];
const RATIO_BUCKETS: &[&str] = &["unknown", "0", "0-25", "25-50", "50-75", "75-100"];
const RISK_BUCKETS: &[&str] = &["unknown", "low", "medium", "high"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    Enabled,
    Disabled,
}

impl ConsentDecision {
    pub fn is_enabled(self) -> bool {
        self == Self::Enabled
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConsent {
    pub anonymous_product_telemetry: ConsentDecision,
    pub research_trace_collection: ConsentDecision,
}

impl Default for TelemetryConsent {
    fn default() -> Self {
        Self {
            anonymous_product_telemetry: ConsentDecision::Disabled,
            research_trace_collection: ConsentDecision::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryCommand {
    Status,
    On,
    Off,
    Purge,
    ExportLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnonymousTelemetrySchema {
    pub version: u32,
    pub allowed_fields: &'static [&'static str],
    pub forbidden_payloads: &'static [&'static str],
}

pub const ANONYMOUS_TELEMETRY_SCHEMA: AnonymousTelemetrySchema = AnonymousTelemetrySchema {
    version: 1,
    allowed_fields: &[
        "schema_version",
        "repogrammar_version",
        "os_family",
        "agent_target",
        "event_window_day_utc",
        "anonymous_machine_id",
        "eligible_code_units_bucket",
        "family_count_bucket",
        "family_support_coverage_bucket",
        "local_pattern_density_bucket",
        "abstention_rate_bucket",
        "external_dependency_signal",
        "thin_wrapper_risk",
        "token_saving_risk",
        "command_category_counts_bucket",
        "mcp_call_count_bucket",
        "read_plan_returned_count_bucket",
        "read_plan_item_count_bucket",
        "unknown_reason_code_counts_bucket",
        "typed_error_code_counts_bucket",
        "source_snippets_returned",
        "measured_token_savings_bucket",
    ],
    forbidden_payloads: &[
        "code",
        "source",
        "source_snippet",
        "path",
        "absolute_path",
        "repository_name",
        "repo_name",
        "symbol",
        "function_name",
        "class_name",
        "prompt",
        "query_text",
        "raw_tool_input",
        "raw_tool_output",
        "evidence_text",
        "environment_variable",
        "credential",
        "raw_error_message",
        "sha256:",
        "byte_range",
        "raw_target",
        "patch",
        "diff",
    ],
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryPaths {
    pub global_data_dir: PathBuf,
    pub repository_root: PathBuf,
    pub state_dir_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryPreference {
    pub enabled: bool,
    pub research_enabled: bool,
    pub anonymous_machine_id: String,
    pub local_salt: String,
    pub schema_version: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryStatusReport {
    pub enabled: bool,
    pub research_enabled: bool,
    pub disabled_by_environment: bool,
    pub effective_enabled: bool,
    pub anonymous_machine_id: String,
    pub schema_version: String,
    pub queue_count: usize,
    pub sent_receipt_count: usize,
    pub network_upload_configured: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryExportReport {
    pub payload: Value,
    pub payload_bytes: usize,
    pub queued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryPurgeReport {
    pub removed_files: usize,
    pub removed_directories: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryUploadReceipt {
    pub status_code: u16,
    pub receipt_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryUploadReport {
    pub uploaded: bool,
    pub dry_run: bool,
    pub network_upload_configured: bool,
    pub reason: Option<String>,
    pub payload: Option<Value>,
    pub receipt: Option<TelemetryUploadReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryUploadRequest {
    pub endpoint: Option<String>,
    pub dry_run: bool,
}

pub trait TelemetryUploadTransport {
    fn upload(
        &self,
        endpoint: &str,
        payload: &str,
        timeout: Duration,
    ) -> Result<TelemetryUploadReceipt, RepoGrammarError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentMode {
    Baseline,
    Treatment,
}

impl ExperimentMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "baseline" => Ok(Self::Baseline),
            "treatment" => Ok(Self::Treatment),
            _ => Err("mode must be baseline or treatment".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Treatment => "treatment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentWorkflowMode {
    RecordExisting,
    ControlledPair,
}

impl ExperimentWorkflowMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "record_existing" | "record-existing" => Ok(Self::RecordExisting),
            "controlled_pair" | "controlled-pair" => Ok(Self::ControlledPair),
            _ => Err("experiment mode must be record_existing or controlled_pair".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecordExisting => "record_existing",
            Self::ControlledPair => "controlled_pair",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementSource {
    HostReported,
    UserEntered,
    DocumentedTokenizer,
}

impl MeasurementSource {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "host_reported" | "host-reported" => Ok(Self::HostReported),
            "user_entered" | "user-entered" => Ok(Self::UserEntered),
            "documented_tokenizer" | "documented-tokenizer" => Ok(Self::DocumentedTokenizer),
            _ => Err(
                "measurement source must be host_reported, user_entered, or documented_tokenizer"
                    .to_string(),
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HostReported => "host_reported",
            Self::UserEntered => "user_entered",
            Self::DocumentedTokenizer => "documented_tokenizer",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TestOutcome {
    Passed,
    Failed,
    NotRun,
    #[default]
    Unknown,
}

impl TestOutcome {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "not_run" | "not-run" => Ok(Self::NotRun),
            "unknown" => Ok(Self::Unknown),
            _ => Err("test outcome must be passed, failed, not_run, or unknown".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::NotRun => "not_run",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentStartRequest {
    pub name: String,
    pub experiment_mode: ExperimentWorkflowMode,
    pub mode: ExperimentMode,
    pub measurement_source: MeasurementSource,
    pub coarse_task_kind: Option<String>,
    pub elapsed_time_bucket: Option<String>,
    pub read_plan_used: Option<bool>,
    pub read_plan_item_count_bucket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentRecordRequest {
    pub name: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_tokens: u64,
    pub success: bool,
    pub test_outcome: TestOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExperimentReport {
    pub name: String,
    pub experiment_mode: Option<String>,
    pub measurement_status: String,
    pub baseline_total_tokens: Option<u64>,
    pub treatment_total_tokens: Option<u64>,
    pub token_savings: Option<i128>,
    pub token_savings_ratio: Option<f64>,
    pub baseline_success: Option<bool>,
    pub treatment_success: Option<bool>,
    pub correctness_comparison: String,
    pub claim_validity: String,
    pub measurement_source: Option<String>,
    pub reason: Option<String>,
    pub caveat: String,
    pub cost_notice_may_have_increased_usage: bool,
    pub cost_notice_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExperimentSession {
    session_id: String,
    experiment_mode: ExperimentWorkflowMode,
    mode: ExperimentMode,
    measurement_source: MeasurementSource,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    tool_tokens: Option<u64>,
    total_tokens: Option<u64>,
    success: Option<bool>,
    active: bool,
    coarse_task_kind: Option<String>,
    elapsed_time_bucket: Option<String>,
    read_plan_used: Option<bool>,
    read_plan_item_count_bucket: Option<String>,
    test_outcome: TestOutcome,
}

pub fn telemetry_disabled_by_environment<F>(lookup: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env_equals(lookup("REPOGRAMMAR_TELEMETRY"), "0")
        || env_equals(lookup("DO_NOT_TRACK"), "1")
        || env_truthy(lookup("CI"))
}

pub fn telemetry_status<F>(
    paths: &TelemetryPaths,
    endpoint: Option<&str>,
    env_lookup: &F,
) -> Result<TelemetryStatusReport, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    let disabled_by_environment = telemetry_disabled_by_environment(env_lookup);
    let preference = load_or_default_preference(&paths.global_data_dir)?;
    let (queue_count, sent_receipt_count) = telemetry_file_counts(paths)?;
    Ok(TelemetryStatusReport {
        enabled: preference.enabled,
        research_enabled: preference.research_enabled,
        disabled_by_environment,
        effective_enabled: preference.enabled && !disabled_by_environment,
        anonymous_machine_id: preference.anonymous_machine_id,
        schema_version: preference.schema_version,
        queue_count,
        sent_receipt_count,
        network_upload_configured: endpoint.is_some_and(|endpoint| !endpoint.trim().is_empty()),
        updated_at: preference.updated_at,
    })
}

pub fn set_anonymous_telemetry(
    paths: &TelemetryPaths,
    enabled: bool,
) -> Result<TelemetryStatusReport, RepoGrammarError> {
    let mut preference = load_or_create_preference(&paths.global_data_dir)?;
    preference.enabled = enabled;
    preference.updated_at = now_unix_seconds();
    write_preference(&paths.global_data_dir, &preference)?;
    telemetry_status(paths, None, &|_| None)
}

pub fn set_research_trace(
    paths: &TelemetryPaths,
    enabled: bool,
) -> Result<TelemetryStatusReport, RepoGrammarError> {
    let mut preference = load_or_create_preference(&paths.global_data_dir)?;
    preference.research_enabled = enabled;
    preference.updated_at = now_unix_seconds();
    write_preference(&paths.global_data_dir, &preference)?;
    telemetry_status(paths, None, &|_| None)
}

pub fn export_anonymous_telemetry(
    paths: &TelemetryPaths,
    repogrammar_version: &str,
    diagnostics: Option<TelemetryDiagnostics>,
    measured_token_savings: Option<&ExperimentReport>,
) -> Result<TelemetryExportReport, RepoGrammarError> {
    let preference = load_or_default_preference(&paths.global_data_dir)?;
    let payload = build_anonymous_payload(
        paths,
        repogrammar_version,
        &preference,
        diagnostics,
        measured_token_savings,
    );
    validate_anonymous_payload(&payload)?;
    let payload_bytes = payload.to_string().len();
    if payload_bytes > MAX_TELEMETRY_PAYLOAD_BYTES {
        return Err(invalid_input(
            "telemetry payload exceeds the maximum supported size",
        ));
    }
    Ok(TelemetryExportReport {
        payload,
        payload_bytes,
        queued: false,
    })
}

pub fn upload_anonymous_telemetry<F>(
    paths: &TelemetryPaths,
    request: TelemetryUploadRequest,
    repogrammar_version: &str,
    diagnostics: Option<TelemetryDiagnostics>,
    measured_token_savings: Option<&ExperimentReport>,
    env_lookup: &F,
    transport: &impl TelemetryUploadTransport,
) -> Result<TelemetryUploadReport, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    let preference = load_or_create_preference(&paths.global_data_dir)?;
    if telemetry_disabled_by_environment(env_lookup) {
        return Ok(not_uploaded("telemetry disabled by environment"));
    }
    if !preference.enabled {
        return Ok(not_uploaded("anonymous telemetry is disabled"));
    }
    let Some(endpoint) = request
        .endpoint
        .as_deref()
        .filter(|endpoint| !endpoint.trim().is_empty())
    else {
        return Ok(TelemetryUploadReport {
            uploaded: false,
            dry_run: request.dry_run,
            network_upload_configured: false,
            reason: Some("telemetry upload endpoint is not configured".to_string()),
            payload: None,
            receipt: None,
        });
    };
    validate_telemetry_endpoint(endpoint)?;

    let payload = build_anonymous_payload(
        paths,
        repogrammar_version,
        &preference,
        diagnostics,
        measured_token_savings,
    );
    validate_anonymous_payload(&payload)?;
    let payload_string = payload.to_string();
    if payload_string.len() > MAX_TELEMETRY_PAYLOAD_BYTES {
        return Err(invalid_input(
            "telemetry payload exceeds the maximum supported size",
        ));
    }

    if request.dry_run {
        return Ok(TelemetryUploadReport {
            uploaded: false,
            dry_run: true,
            network_upload_configured: true,
            reason: Some("dry run; no network upload attempted".to_string()),
            payload: Some(payload),
            receipt: None,
        });
    }

    let queue_file = write_upload_queue(paths, &payload)?;
    write_rollup(paths, &payload)?;
    validate_anonymous_payload(&payload)?;
    let receipt = transport.upload(endpoint, &payload_string, TELEMETRY_UPLOAD_TIMEOUT)?;
    write_upload_receipt(paths, &receipt)?;
    let _ = fs::remove_file(queue_file);
    Ok(TelemetryUploadReport {
        uploaded: true,
        dry_run: false,
        network_upload_configured: true,
        reason: None,
        payload: None,
        receipt: Some(receipt),
    })
}

pub fn purge_telemetry(
    paths: &TelemetryPaths,
    yes: bool,
) -> Result<TelemetryPurgeReport, RepoGrammarError> {
    if !yes {
        return Err(invalid_input("telemetry purge requires --yes"));
    }
    let Some(root) = repo_telemetry_dir(paths)? else {
        return Ok(TelemetryPurgeReport {
            removed_files: 0,
            removed_directories: 0,
        });
    };
    remove_tree_contents(&root)
}

pub fn research_export(paths: &TelemetryPaths) -> Result<Value, RepoGrammarError> {
    let preference = load_or_default_preference(&paths.global_data_dir)?;
    Ok(json!({
        "schema_version": "research-trace-consent.v1",
        "research_enabled": preference.research_enabled,
        "trace_mode": "redacted_metadata_only",
        "full_prompt_or_source_trace": "not_implemented",
        "source_snippets_included": false,
    }))
}

pub fn research_purge(
    paths: &TelemetryPaths,
    yes: bool,
) -> Result<TelemetryPurgeReport, RepoGrammarError> {
    if !yes {
        return Err(invalid_input("research purge requires --yes"));
    }
    let research_dir = paths.global_data_dir.join("telemetry").join("research");
    if !research_dir.exists() {
        return Ok(TelemetryPurgeReport {
            removed_files: 0,
            removed_directories: 0,
        });
    }
    remove_tree_contents(&research_dir)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryDiagnostics {
    pub eligible_code_units: usize,
    pub family_count: usize,
    pub family_support_coverage: Option<f64>,
    pub local_pattern_density: Option<f64>,
    pub abstention_rate: Option<f64>,
    pub external_dependency_signal: DiagnosticSignal,
    pub thin_wrapper_risk: &'static str,
    pub token_saving_risk: &'static str,
    pub read_plan_item_count: usize,
}

pub fn validate_anonymous_payload(payload: &Value) -> Result<(), RepoGrammarError> {
    let Some(object) = payload.as_object() else {
        return Err(invalid_input("telemetry payload must be a JSON object"));
    };
    for key in object.keys() {
        if !ANONYMOUS_TELEMETRY_SCHEMA
            .allowed_fields
            .contains(&key.as_str())
        {
            return Err(invalid_input(
                "telemetry payload contains a non-allowlisted field",
            ));
        }
    }
    for field in ANONYMOUS_TELEMETRY_SCHEMA.allowed_fields {
        if !object.contains_key(*field) {
            return Err(invalid_input(
                "telemetry payload is missing a required field",
            ));
        }
    }
    if payload.get("schema_version").and_then(Value::as_str) != Some(TELEMETRY_SCHEMA_VERSION) {
        return Err(invalid_input(
            "telemetry payload schema version is unsupported",
        ));
    }
    require_safe_text(payload, "repogrammar_version", 1, 64)?;
    require_enum_value(
        field_value(payload, "os_family")?,
        "os_family",
        &["macos", "linux", "windows", "other"],
    )?;
    require_enum_value(
        field_value(payload, "agent_target")?,
        "agent_target",
        &["codex", "claude-code", "unknown"],
    )?;
    if payload
        .get("event_window_day_utc")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err(invalid_input("telemetry event window is invalid"));
    }
    require_prefixed_hex(payload, "anonymous_machine_id", "anon-", 64)?;
    for field in [
        "eligible_code_units_bucket",
        "family_count_bucket",
        "mcp_call_count_bucket",
        "read_plan_returned_count_bucket",
        "read_plan_item_count_bucket",
    ] {
        require_enum_value(field_value(payload, field)?, field, COUNT_BUCKETS)?;
    }
    for field in [
        "family_support_coverage_bucket",
        "local_pattern_density_bucket",
        "abstention_rate_bucket",
    ] {
        require_enum_value(field_value(payload, field)?, field, RATIO_BUCKETS)?;
    }
    for field in [
        "external_dependency_signal",
        "thin_wrapper_risk",
        "token_saving_risk",
    ] {
        require_enum_value(field_value(payload, field)?, field, RISK_BUCKETS)?;
    }
    for field in [
        "command_category_counts_bucket",
        "unknown_reason_code_counts_bucket",
        "typed_error_code_counts_bucket",
    ] {
        require_bucket_map(payload, field)?;
    }
    if let Some(value) = payload.get("measured_token_savings_bucket") {
        if !value.is_null() {
            require_enum_value(
                value,
                "measured_token_savings_bucket",
                &["negative", "0", "1-999", "1000-9999", "10000+"],
            )?;
        }
    }
    if payload
        .get("source_snippets_returned")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(invalid_input(
            "telemetry payload cannot report source snippets",
        ));
    }
    if payload.to_string().len() > MAX_TELEMETRY_PAYLOAD_BYTES {
        return Err(invalid_input(
            "telemetry payload exceeds the maximum supported size",
        ));
    }
    Ok(())
}

pub fn validate_telemetry_endpoint(endpoint: &str) -> Result<(), RepoGrammarError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint.len() > 2048 || endpoint.chars().any(char::is_control) {
        return Err(invalid_input("telemetry endpoint is invalid"));
    }
    if endpoint.starts_with("https://") {
        let host = endpoint.trim_start_matches("https://");
        if host.is_empty() || host.starts_with('/') {
            return Err(invalid_input("telemetry endpoint is invalid"));
        }
        return Ok(());
    }
    if is_local_http_endpoint(endpoint) {
        return Ok(());
    }
    Err(invalid_input(
        "telemetry endpoint must use HTTPS except localhost test endpoints",
    ))
}

pub fn experiment_start(
    global_data_dir: &Path,
    request: ExperimentStartRequest,
) -> Result<ExperimentReport, RepoGrammarError> {
    validate_experiment_name(&request.name)?;
    validate_coarse_task_kind(request.coarse_task_kind.as_deref())?;
    validate_optional_bucket(
        "elapsed time bucket",
        request.elapsed_time_bucket.as_deref(),
    )?;
    validate_optional_count_bucket(
        "read-plan item count bucket",
        request.read_plan_item_count_bucket.as_deref(),
    )?;
    let mut sessions = read_experiment_sessions(global_data_dir, &request.name)?;
    sessions
        .iter_mut()
        .for_each(|session| session.active = false);
    sessions.push(ExperimentSession {
        session_id: format!(
            "{}-{}",
            request.mode.as_str(),
            stable_hash(&format!("{}:{}", request.name, now_unix_nanos()))
        ),
        experiment_mode: request.experiment_mode,
        mode: request.mode,
        measurement_source: request.measurement_source,
        input_tokens: None,
        output_tokens: None,
        tool_tokens: None,
        total_tokens: None,
        success: None,
        active: true,
        coarse_task_kind: request.coarse_task_kind,
        elapsed_time_bucket: request.elapsed_time_bucket,
        read_plan_used: request.read_plan_used,
        read_plan_item_count_bucket: request.read_plan_item_count_bucket,
        test_outcome: TestOutcome::Unknown,
    });
    write_experiment_sessions(global_data_dir, &request.name, &sessions)?;
    Ok(report_for_sessions(&request.name, &sessions))
}

pub fn experiment_record(
    global_data_dir: &Path,
    request: ExperimentRecordRequest,
) -> Result<ExperimentReport, RepoGrammarError> {
    validate_experiment_name(&request.name)?;
    let mut sessions = read_experiment_sessions(global_data_dir, &request.name)?;
    let Some(session) = sessions.iter_mut().rev().find(|session| session.active) else {
        return Err(invalid_input(
            "no active experiment session; run experiment-start first",
        ));
    };
    let total_tokens = request
        .input_tokens
        .saturating_add(request.output_tokens)
        .saturating_add(request.tool_tokens);
    session.input_tokens = Some(request.input_tokens);
    session.output_tokens = Some(request.output_tokens);
    session.tool_tokens = Some(request.tool_tokens);
    session.total_tokens = Some(total_tokens);
    session.success = Some(request.success);
    session.test_outcome = request.test_outcome;
    write_experiment_sessions(global_data_dir, &request.name, &sessions)?;
    Ok(report_for_sessions(&request.name, &sessions))
}

pub fn experiment_stop(
    global_data_dir: &Path,
    name: &str,
) -> Result<ExperimentReport, RepoGrammarError> {
    validate_experiment_name(name)?;
    let mut sessions = read_experiment_sessions(global_data_dir, name)?;
    let Some(session) = sessions.iter_mut().rev().find(|session| session.active) else {
        return Err(invalid_input("no active experiment session"));
    };
    session.active = false;
    write_experiment_sessions(global_data_dir, name, &sessions)?;
    Ok(report_for_sessions(name, &sessions))
}

pub fn experiment_report(
    global_data_dir: &Path,
    name: &str,
) -> Result<ExperimentReport, RepoGrammarError> {
    validate_experiment_name(name)?;
    let sessions = read_experiment_sessions(global_data_dir, name)?;
    Ok(report_for_sessions(name, &sessions))
}

pub fn experiment_export(global_data_dir: &Path, name: &str) -> Result<Value, RepoGrammarError> {
    validate_experiment_name(name)?;
    let sessions = read_experiment_sessions(global_data_dir, name)?;
    Ok(redacted_experiment_export(
        &sessions,
        &report_for_sessions(name, &sessions),
    ))
}

pub fn experiment_purge(
    global_data_dir: &Path,
    name: &str,
    yes: bool,
) -> Result<TelemetryPurgeReport, RepoGrammarError> {
    validate_experiment_name(name)?;
    if !yes {
        return Err(invalid_input("experiment purge requires --yes"));
    }
    let path = experiment_file(global_data_dir, name);
    if path.exists() {
        fs::remove_file(path).map_err(|_| invalid_input("failed to purge experiment"))?;
        Ok(TelemetryPurgeReport {
            removed_files: 1,
            removed_directories: 0,
        })
    } else {
        Ok(TelemetryPurgeReport {
            removed_files: 0,
            removed_directories: 0,
        })
    }
}

pub fn latest_comparable_experiment_report(
    global_data_dir: &Path,
) -> Result<Option<ExperimentReport>, RepoGrammarError> {
    let experiment_dir = global_data_dir.join("experiments");
    if !experiment_dir.is_dir() {
        return Ok(None);
    }
    let mut files = fs::read_dir(&experiment_dir)
        .map_err(|_| invalid_input("failed to read experiments"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();
    let mut latest = None;
    for path in files {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let sessions = read_experiment_sessions(global_data_dir, stem)?;
        let report = report_for_sessions(stem, &sessions);
        if report.token_savings.is_some() {
            latest = Some(report);
        }
    }
    Ok(latest)
}

pub fn experiment_report_json(report: &ExperimentReport) -> Value {
    json!({
        "schema_version": TOKEN_EXPERIMENT_SCHEMA_VERSION,
        "name": report.name,
        "metric_kind": "CAUSAL_EXPERIMENT",
        "measurement_status": report.measurement_status,
        "experiment_mode": report.experiment_mode,
        "baseline_total_tokens": report.baseline_total_tokens,
        "treatment_total_tokens": report.treatment_total_tokens,
        "token_savings": report.token_savings,
        "token_savings_ratio": report.token_savings_ratio,
        "correctness_comparison": report.correctness_comparison,
        "correctness": {
            "baseline_success": report.baseline_success,
            "treatment_success": report.treatment_success,
        },
        "claim_validity": report.claim_validity,
        "measurement_source": report.measurement_source,
        "reason": report.reason,
        "cost_notice": {
            "may_have_increased_usage": report.cost_notice_may_have_increased_usage,
            "reason": report.cost_notice_reason,
        },
        "caveat": report.caveat,
    })
}

fn not_uploaded(reason: &str) -> TelemetryUploadReport {
    TelemetryUploadReport {
        uploaded: false,
        dry_run: false,
        network_upload_configured: false,
        reason: Some(reason.to_string()),
        payload: None,
        receipt: None,
    }
}

fn load_or_default_preference(
    global_data_dir: &Path,
) -> Result<TelemetryPreference, RepoGrammarError> {
    match read_preference(global_data_dir)? {
        Some(preference) => Ok(preference),
        None => Ok(new_preference()),
    }
}

fn load_or_create_preference(
    global_data_dir: &Path,
) -> Result<TelemetryPreference, RepoGrammarError> {
    match read_preference(global_data_dir)? {
        Some(preference) => Ok(preference),
        None => {
            let preference = new_preference();
            write_preference(global_data_dir, &preference)?;
            Ok(preference)
        }
    }
}

fn read_preference(
    global_data_dir: &Path,
) -> Result<Option<TelemetryPreference>, RepoGrammarError> {
    let path = preference_file(global_data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let value = read_json_file_bounded(&path)?;
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let research_enabled = value
        .get("research_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let anonymous_machine_id = value
        .get("anonymous_machine_id")
        .and_then(Value::as_str)
        .filter(|value| safe_identifier(value))
        .unwrap_or("anonymous-unknown")
        .to_string();
    let local_salt = value
        .get("local_salt")
        .and_then(Value::as_str)
        .filter(|value| safe_identifier(value))
        .unwrap_or("salt-unknown")
        .to_string();
    Ok(Some(TelemetryPreference {
        enabled,
        research_enabled,
        anonymous_machine_id,
        local_salt,
        schema_version: value
            .get("schema_version")
            .and_then(Value::as_str)
            .unwrap_or(TELEMETRY_PREFERENCE_SCHEMA_VERSION)
            .to_string(),
        updated_at: value.get("updated_at").and_then(Value::as_u64).unwrap_or(0),
    }))
}

fn write_preference(
    global_data_dir: &Path,
    preference: &TelemetryPreference,
) -> Result<(), RepoGrammarError> {
    let path = preference_file(global_data_dir);
    ensure_parent_dir(&path)?;
    write_json_atomically(
        &path,
        &json!({
            "schema_version": TELEMETRY_PREFERENCE_SCHEMA_VERSION,
            "enabled": preference.enabled,
            "research_enabled": preference.research_enabled,
            "anonymous_machine_id": preference.anonymous_machine_id,
            "local_salt": preference.local_salt,
            "updated_at": preference.updated_at,
        }),
    )
}

fn new_preference() -> TelemetryPreference {
    let seed = format!(
        "{}:{}:{}",
        now_unix_nanos(),
        std::process::id(),
        std::env::consts::OS
    );
    TelemetryPreference {
        enabled: false,
        research_enabled: false,
        anonymous_machine_id: format!("anon-{}", stable_hash(&seed)),
        local_salt: format!("salt-{}", stable_hash(&format!("salt:{seed}"))),
        schema_version: TELEMETRY_PREFERENCE_SCHEMA_VERSION.to_string(),
        updated_at: now_unix_seconds(),
    }
}

fn build_anonymous_payload(
    _paths: &TelemetryPaths,
    repogrammar_version: &str,
    preference: &TelemetryPreference,
    diagnostics: Option<TelemetryDiagnostics>,
    measured_token_savings: Option<&ExperimentReport>,
) -> Value {
    let diagnostics = diagnostics.unwrap_or(TelemetryDiagnostics {
        eligible_code_units: 0,
        family_count: 0,
        family_support_coverage: None,
        local_pattern_density: None,
        abstention_rate: None,
        external_dependency_signal: DiagnosticSignal::Unknown,
        thin_wrapper_risk: "unknown",
        token_saving_risk: "unknown",
        read_plan_item_count: 0,
    });
    let measured_token_savings_bucket = measured_token_savings
        .and_then(|report| report.token_savings)
        .map(token_savings_bucket);
    json!({
        "schema_version": TELEMETRY_SCHEMA_VERSION,
        "repogrammar_version": repogrammar_version,
        "os_family": os_family(),
        "agent_target": "unknown",
        "event_window_day_utc": unix_day(),
        "anonymous_machine_id": preference.anonymous_machine_id,
        "eligible_code_units_bucket": count_bucket(diagnostics.eligible_code_units),
        "family_count_bucket": count_bucket(diagnostics.family_count),
        "family_support_coverage_bucket": ratio_bucket(diagnostics.family_support_coverage),
        "local_pattern_density_bucket": ratio_bucket(diagnostics.local_pattern_density),
        "abstention_rate_bucket": ratio_bucket(diagnostics.abstention_rate),
        "external_dependency_signal": diagnostics.external_dependency_signal.as_str(),
        "thin_wrapper_risk": diagnostics.thin_wrapper_risk,
        "token_saving_risk": diagnostics.token_saving_risk,
        "command_category_counts_bucket": empty_bucket_map(),
        "mcp_call_count_bucket": count_bucket(0),
        "read_plan_returned_count_bucket": count_bucket(0),
        "read_plan_item_count_bucket": count_bucket(diagnostics.read_plan_item_count),
        "unknown_reason_code_counts_bucket": empty_bucket_map(),
        "typed_error_code_counts_bucket": empty_bucket_map(),
        "source_snippets_returned": false,
        "measured_token_savings_bucket": measured_token_savings_bucket,
    })
}

fn telemetry_file_counts(paths: &TelemetryPaths) -> Result<(usize, usize), RepoGrammarError> {
    let Some(root) = repo_telemetry_dir(paths)? else {
        return Ok((0, 0));
    };
    Ok((
        count_json_files(&root.join("queue"))?,
        count_json_files(&root.join("sent"))?,
    ))
}

fn write_upload_queue(
    paths: &TelemetryPaths,
    payload: &Value,
) -> Result<PathBuf, RepoGrammarError> {
    let root = require_repo_telemetry_dir(paths)?;
    let queue_dir = root.join("queue");
    fs::create_dir_all(&queue_dir)
        .map_err(|_| invalid_input("failed to create telemetry queue"))?;
    let next = count_json_files(&queue_dir)?.saturating_add(1);
    let path = queue_dir.join(format!("batch-{next:06}.json"));
    write_json_atomically(&path, payload)?;
    Ok(path)
}

fn write_rollup(paths: &TelemetryPaths, payload: &Value) -> Result<(), RepoGrammarError> {
    let root = require_repo_telemetry_dir(paths)?;
    let rollups_dir = root.join("rollups");
    fs::create_dir_all(&rollups_dir)
        .map_err(|_| invalid_input("failed to create telemetry rollups"))?;
    let path = rollups_dir.join(format!("{}.telemetry.json", unix_day()));
    write_json_atomically(&path, payload)
}

fn write_upload_receipt(
    paths: &TelemetryPaths,
    receipt: &TelemetryUploadReceipt,
) -> Result<(), RepoGrammarError> {
    let root = require_repo_telemetry_dir(paths)?;
    let sent_dir = root.join("sent");
    fs::create_dir_all(&sent_dir)
        .map_err(|_| invalid_input("failed to create telemetry receipts"))?;
    let path = sent_dir.join(format!("{}.receipt.json", receipt.receipt_id));
    write_json_atomically(
        &path,
        &json!({
            "schema_version": "telemetry-upload-receipt.v1",
            "status_code": receipt.status_code,
            "receipt_id": receipt.receipt_id,
            "uploaded_at": now_unix_seconds(),
        }),
    )
}

fn require_repo_telemetry_dir(paths: &TelemetryPaths) -> Result<PathBuf, RepoGrammarError> {
    let location = repository_state_location(RepositoryStatusRequest {
        path: paths.repository_root.display().to_string(),
        state_dir_override: paths.state_dir_override.clone(),
    })?;
    let root = location.state_dir.join("telemetry");
    fs::create_dir_all(root.join("rollups"))
        .and_then(|_| fs::create_dir_all(root.join("queue")))
        .and_then(|_| fs::create_dir_all(root.join("sent")))
        .map_err(|_| invalid_input("failed to create repository-local telemetry state"))?;
    Ok(root)
}

fn repo_telemetry_dir(paths: &TelemetryPaths) -> Result<Option<PathBuf>, RepoGrammarError> {
    let location = repository_state_location(RepositoryStatusRequest {
        path: paths.repository_root.display().to_string(),
        state_dir_override: paths.state_dir_override.clone(),
    })?;
    let root = location.state_dir.join("telemetry");
    if root.exists() {
        Ok(Some(root))
    } else {
        Ok(None)
    }
}

fn read_experiment_sessions(
    global_data_dir: &Path,
    name: &str,
) -> Result<Vec<ExperimentSession>, RepoGrammarError> {
    let path = experiment_file(global_data_dir, name);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let value = read_json_file_bounded(&path)?;
    let sessions = value
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_input("experiment file is invalid"))?;
    let mut parsed = Vec::new();
    for session in sessions {
        parsed.push(parse_experiment_session(session)?);
    }
    Ok(parsed)
}

fn write_experiment_sessions(
    global_data_dir: &Path,
    name: &str,
    sessions: &[ExperimentSession],
) -> Result<(), RepoGrammarError> {
    let path = experiment_file(global_data_dir, name);
    ensure_parent_dir(&path)?;
    write_json_atomically(
        &path,
        &json!({
            "schema_version": TOKEN_EXPERIMENT_SCHEMA_VERSION,
            "name": name,
            "sessions": sessions.iter().map(session_json).collect::<Vec<_>>(),
        }),
    )
}

fn report_for_sessions(name: &str, sessions: &[ExperimentSession]) -> ExperimentReport {
    let baseline = latest_completed_session(sessions, ExperimentMode::Baseline);
    let treatment = latest_completed_session(sessions, ExperimentMode::Treatment);
    let caveat =
        "valid only for comparable paired baseline/treatment token measurements".to_string();
    let latest_mode = sessions.last().map(|session| session.experiment_mode);
    let Some(baseline) = baseline else {
        return missing_report(
            name,
            "missing comparable baseline/treatment measurement",
            caveat,
            latest_mode,
        );
    };
    let Some(treatment) = treatment else {
        return missing_report(
            name,
            "missing comparable baseline/treatment measurement",
            caveat,
            Some(baseline.experiment_mode),
        );
    };
    if baseline.experiment_mode != treatment.experiment_mode {
        return missing_report(
            name,
            "baseline and treatment experiment modes differ",
            caveat,
            Some(baseline.experiment_mode),
        );
    }
    if baseline.measurement_source != treatment.measurement_source {
        return missing_report(
            name,
            "baseline and treatment measurement sources differ",
            caveat,
            Some(baseline.experiment_mode),
        );
    }
    if baseline.coarse_task_kind != treatment.coarse_task_kind {
        return missing_report(
            name,
            "baseline and treatment task kinds differ",
            caveat,
            Some(baseline.experiment_mode),
        );
    }
    let Some(baseline_total) = baseline.total_tokens else {
        return missing_report(
            name,
            "missing comparable baseline/treatment measurement",
            caveat,
            Some(baseline.experiment_mode),
        );
    };
    let Some(treatment_total) = treatment.total_tokens else {
        return missing_report(
            name,
            "missing comparable baseline/treatment measurement",
            caveat,
            Some(baseline.experiment_mode),
        );
    };
    let savings = baseline_total as i128 - treatment_total as i128;
    let ratio = if baseline_total == 0 {
        None
    } else {
        Some(savings as f64 / baseline_total as f64)
    };
    let correctness_comparison = correctness_comparison(baseline.success, treatment.success);
    let treatment_failed = matches!(treatment.success, Some(false));
    let claim_validity = if treatment_failed {
        "invalid_for_product_claim"
    } else if correctness_comparison == "both_success" {
        "valid_for_product_claim"
    } else {
        "unknown"
    };
    ExperimentReport {
        name: name.to_string(),
        experiment_mode: Some(baseline.experiment_mode.as_str().to_string()),
        measurement_status: "paired_measurement_available".to_string(),
        baseline_total_tokens: Some(baseline_total),
        treatment_total_tokens: Some(treatment_total),
        token_savings: Some(savings),
        token_savings_ratio: ratio,
        baseline_success: baseline.success,
        treatment_success: treatment.success,
        correctness_comparison,
        claim_validity: claim_validity.to_string(),
        measurement_source: Some(baseline.measurement_source.as_str().to_string()),
        reason: None,
        cost_notice_may_have_increased_usage: baseline.experiment_mode
            == ExperimentWorkflowMode::ControlledPair,
        cost_notice_reason: experiment_cost_notice_reason(baseline.experiment_mode).to_string(),
        caveat,
    }
}

fn missing_report(
    name: &str,
    reason: &str,
    caveat: String,
    experiment_mode: Option<ExperimentWorkflowMode>,
) -> ExperimentReport {
    ExperimentReport {
        name: name.to_string(),
        experiment_mode: experiment_mode.map(|mode| mode.as_str().to_string()),
        measurement_status: "no_paired_measurement".to_string(),
        baseline_total_tokens: None,
        treatment_total_tokens: None,
        token_savings: None,
        token_savings_ratio: None,
        baseline_success: None,
        treatment_success: None,
        correctness_comparison: "unknown".to_string(),
        claim_validity: "unknown".to_string(),
        measurement_source: None,
        reason: Some(reason.to_string()),
        cost_notice_may_have_increased_usage: experiment_mode
            == Some(ExperimentWorkflowMode::ControlledPair),
        cost_notice_reason: experiment_mode
            .map(experiment_cost_notice_reason)
            .unwrap_or("default diagnostics do not run paired experiments")
            .to_string(),
        caveat,
    }
}

fn experiment_cost_notice_reason(mode: ExperimentWorkflowMode) -> &'static str {
    match mode {
        ExperimentWorkflowMode::RecordExisting => {
            "token counts were recorded from existing sessions"
        }
        ExperimentWorkflowMode::ControlledPair => {
            "controlled paired measurement may require separate baseline and treatment sessions"
        }
    }
}

fn latest_completed_session(
    sessions: &[ExperimentSession],
    mode: ExperimentMode,
) -> Option<&ExperimentSession> {
    sessions
        .iter()
        .rev()
        .find(|session| session.mode == mode && !session.active && session.total_tokens.is_some())
}

fn correctness_comparison(baseline: Option<bool>, treatment: Option<bool>) -> String {
    match (baseline, treatment) {
        (Some(true), Some(true)) => "both_success".to_string(),
        (Some(false), Some(true)) => "baseline_failed_treatment_success".to_string(),
        (Some(true), Some(false)) => "treatment_failed".to_string(),
        (Some(false), Some(false)) => "both_failed".to_string(),
        _ => "unknown".to_string(),
    }
}

fn redacted_experiment_export(sessions: &[ExperimentSession], report: &ExperimentReport) -> Value {
    json!({
        "schema_version": TOKEN_EXPERIMENT_SCHEMA_VERSION,
        "name": "redacted",
        "redacted": true,
        "source_snippets_returned": false,
        "sessions": sessions.iter().map(redacted_session_json).collect::<Vec<_>>(),
        "report": redacted_experiment_report_json(report),
    })
}

fn redacted_session_json(session: &ExperimentSession) -> Value {
    json!({
        "experiment_mode": session.experiment_mode.as_str(),
        "mode": session.mode.as_str(),
        "measurement_source": session.measurement_source.as_str(),
        "total_tokens_bucket": session.total_tokens.map(count_bucket_u64),
        "success": session.success,
        "active": session.active,
        "coarse_task_kind": session.coarse_task_kind,
        "elapsed_time_bucket": session.elapsed_time_bucket,
        "read_plan_used": session.read_plan_used,
        "read_plan_item_count_bucket": session.read_plan_item_count_bucket,
        "test_outcome": session.test_outcome.as_str(),
    })
}

fn redacted_experiment_report_json(report: &ExperimentReport) -> Value {
    json!({
        "schema_version": TOKEN_EXPERIMENT_SCHEMA_VERSION,
        "name": "redacted",
        "metric_kind": "CAUSAL_EXPERIMENT",
        "measurement_status": report.measurement_status,
        "experiment_mode": report.experiment_mode,
        "baseline_total_tokens_bucket": report.baseline_total_tokens.map(count_bucket_u64),
        "treatment_total_tokens_bucket": report.treatment_total_tokens.map(count_bucket_u64),
        "token_savings_bucket": report.token_savings.map(token_savings_bucket),
        "token_savings_ratio_bucket": ratio_bucket(report.token_savings_ratio),
        "correctness_comparison": report.correctness_comparison,
        "correctness": {
            "baseline_success": report.baseline_success,
            "treatment_success": report.treatment_success,
        },
        "claim_validity": report.claim_validity,
        "measurement_source": report.measurement_source,
        "reason": report.reason,
        "cost_notice": {
            "may_have_increased_usage": report.cost_notice_may_have_increased_usage,
            "reason": report.cost_notice_reason,
        },
        "caveat": report.caveat,
    })
}

fn session_json(session: &ExperimentSession) -> Value {
    json!({
        "session_id": session.session_id,
        "experiment_mode": session.experiment_mode.as_str(),
        "mode": session.mode.as_str(),
        "measurement_source": session.measurement_source.as_str(),
        "input_tokens": session.input_tokens,
        "output_tokens": session.output_tokens,
        "tool_tokens": session.tool_tokens,
        "total_tokens": session.total_tokens,
        "success": session.success,
        "active": session.active,
        "coarse_task_kind": session.coarse_task_kind,
        "elapsed_time_bucket": session.elapsed_time_bucket,
        "read_plan_used": session.read_plan_used,
        "read_plan_item_count_bucket": session.read_plan_item_count_bucket,
        "test_outcome": session.test_outcome.as_str(),
    })
}

fn parse_experiment_session(value: &Value) -> Result<ExperimentSession, RepoGrammarError> {
    let experiment_mode = value
        .get("experiment_mode")
        .and_then(Value::as_str)
        .map(ExperimentWorkflowMode::parse)
        .transpose()
        .map_err(invalid_input)?
        .unwrap_or(ExperimentWorkflowMode::RecordExisting);
    let mode = ExperimentMode::parse(required_str(value, "mode")?).map_err(invalid_input)?;
    let measurement_source = MeasurementSource::parse(required_str(value, "measurement_source")?)
        .map_err(invalid_input)?;
    let test_outcome = value
        .get("test_outcome")
        .and_then(Value::as_str)
        .map(TestOutcome::parse)
        .transpose()
        .map_err(invalid_input)?
        .unwrap_or(TestOutcome::Unknown);
    Ok(ExperimentSession {
        session_id: required_str(value, "session_id")?.to_string(),
        experiment_mode,
        mode,
        measurement_source,
        input_tokens: value.get("input_tokens").and_then(Value::as_u64),
        output_tokens: value.get("output_tokens").and_then(Value::as_u64),
        tool_tokens: value.get("tool_tokens").and_then(Value::as_u64),
        total_tokens: value.get("total_tokens").and_then(Value::as_u64),
        success: value.get("success").and_then(Value::as_bool),
        active: value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        coarse_task_kind: value
            .get("coarse_task_kind")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        elapsed_time_bucket: value
            .get("elapsed_time_bucket")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        read_plan_used: value.get("read_plan_used").and_then(Value::as_bool),
        read_plan_item_count_bucket: value
            .get("read_plan_item_count_bucket")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        test_outcome,
    })
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, RepoGrammarError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_input("experiment file is invalid"))
}

fn preference_file(global_data_dir: &Path) -> PathBuf {
    global_data_dir.join("telemetry").join("preference.json")
}

fn experiment_file(global_data_dir: &Path, name: &str) -> PathBuf {
    global_data_dir
        .join("experiments")
        .join(format!("{name}.json"))
}

fn validate_experiment_name(name: &str) -> Result<(), RepoGrammarError> {
    if name.trim().is_empty()
        || name.len() > MAX_EXPERIMENT_NAME_BYTES
        || name.chars().any(char::is_control)
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(invalid_input("experiment name is invalid"));
    }
    Ok(())
}

fn validate_optional_bucket(name: &str, value: Option<&str>) -> Result<(), RepoGrammarError> {
    if let Some(value) = value {
        if value.trim().is_empty()
            || value.len() > 128
            || value.chars().any(char::is_control)
            || unsafe_metadata_text(value)
        {
            return Err(invalid_input(format!("{name} is invalid")));
        }
    }
    Ok(())
}

fn validate_coarse_task_kind(value: Option<&str>) -> Result<(), RepoGrammarError> {
    if let Some(value) = value {
        if !matches!(
            value,
            "implementation" | "test" | "review" | "refactor" | "unknown"
        ) {
            return Err(invalid_input("coarse task kind is invalid"));
        }
    }
    Ok(())
}

fn validate_optional_count_bucket(name: &str, value: Option<&str>) -> Result<(), RepoGrammarError> {
    if let Some(value) = value {
        if !COUNT_BUCKETS.contains(&value) {
            return Err(invalid_input(format!("{name} is invalid")));
        }
    }
    Ok(())
}

fn field_value<'a>(payload: &'a Value, field: &str) -> Result<&'a Value, RepoGrammarError> {
    payload
        .get(field)
        .ok_or_else(|| invalid_input("telemetry payload is missing a required field"))
}

fn require_safe_text(
    payload: &Value,
    field: &str,
    min_len: usize,
    max_len: usize,
) -> Result<(), RepoGrammarError> {
    let Some(value) = field_value(payload, field)?.as_str() else {
        return Err(invalid_input("telemetry payload text field is invalid"));
    };
    if value.len() < min_len || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(invalid_input("telemetry payload text field is invalid"));
    }
    if forbidden_text(value) {
        return Err(invalid_input(
            "telemetry payload text field contains forbidden content",
        ));
    }
    Ok(())
}

fn require_enum_value(
    value: &Value,
    field: &str,
    allowed: &[&str],
) -> Result<(), RepoGrammarError> {
    let Some(text) = value.as_str() else {
        return Err(invalid_input(format!("telemetry {field} is invalid")));
    };
    if !allowed.contains(&text) {
        return Err(invalid_input(format!("telemetry {field} is invalid")));
    }
    Ok(())
}

fn require_prefixed_hex(
    payload: &Value,
    field: &str,
    prefix: &str,
    hex_len: usize,
) -> Result<(), RepoGrammarError> {
    let Some(text) = field_value(payload, field)?.as_str() else {
        return Err(invalid_input("telemetry identifier is invalid"));
    };
    let Some(hex) = text.strip_prefix(prefix) else {
        return Err(invalid_input("telemetry identifier is invalid"));
    };
    if hex.len() != hex_len
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_input("telemetry identifier is invalid"));
    }
    Ok(())
}

fn require_bucket_map(payload: &Value, field: &str) -> Result<(), RepoGrammarError> {
    let Some(object) = field_value(payload, field)?.as_object() else {
        return Err(invalid_input("telemetry bucket map is invalid"));
    };
    for (key, value) in object {
        if key.trim().is_empty()
            || key.len() > 64
            || key.chars().any(char::is_control)
            || unsafe_metadata_text(key)
        {
            return Err(invalid_input("telemetry bucket map key is invalid"));
        }
        require_enum_value(value, field, COUNT_BUCKETS)?;
    }
    Ok(())
}

fn read_json_file_bounded(path: &Path) -> Result<Value, RepoGrammarError> {
    let metadata = fs::metadata(path).map_err(|_| invalid_input("failed to read state file"))?;
    if !metadata.is_file() || metadata.len() > MAX_STATE_FILE_BYTES {
        return Err(invalid_input("state file is invalid"));
    }
    let text = fs::read_to_string(path).map_err(|_| invalid_input("failed to read state file"))?;
    serde_json::from_str(&text).map_err(|_| invalid_input("state file is invalid"))
}

fn write_json_atomically(path: &Path, value: &Value) -> Result<(), RepoGrammarError> {
    ensure_parent_dir(path)?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, value.to_string())
        .map_err(|_| invalid_input("failed to write state file"))?;
    fs::rename(&tmp_path, path).map_err(|_| invalid_input("failed to replace state file"))
}

fn ensure_parent_dir(path: &Path) -> Result<(), RepoGrammarError> {
    let Some(parent) = path.parent() else {
        return Err(invalid_input("state path is invalid"));
    };
    fs::create_dir_all(parent).map_err(|_| invalid_input("failed to create state directory"))
}

fn remove_tree_contents(path: &Path) -> Result<TelemetryPurgeReport, RepoGrammarError> {
    let mut report = TelemetryPurgeReport {
        removed_files: 0,
        removed_directories: 0,
    };
    if !path.exists() {
        return Ok(report);
    }
    if path.is_file() {
        fs::remove_file(path).map_err(|_| invalid_input("failed to purge telemetry state"))?;
        report.removed_files += 1;
        return Ok(report);
    }
    let entries =
        fs::read_dir(path).map_err(|_| invalid_input("failed to read telemetry state"))?;
    for entry in entries {
        let entry = entry.map_err(|_| invalid_input("failed to read telemetry state"))?;
        let child = entry.path();
        if child.is_dir() {
            let child_report = remove_tree_contents(&child)?;
            report.removed_files += child_report.removed_files;
            report.removed_directories += child_report.removed_directories;
            fs::remove_dir(&child).map_err(|_| invalid_input("failed to purge telemetry state"))?;
            report.removed_directories += 1;
        } else if child.is_file() {
            fs::remove_file(&child)
                .map_err(|_| invalid_input("failed to purge telemetry state"))?;
            report.removed_files += 1;
        }
    }
    Ok(report)
}

fn count_json_files(path: &Path) -> Result<usize, RepoGrammarError> {
    if !path.is_dir() {
        return Ok(0);
    }
    Ok(fs::read_dir(path)
        .map_err(|_| invalid_input("failed to read telemetry state"))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("json")
        })
        .count())
}

fn safe_identifier(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn forbidden_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ANONYMOUS_TELEMETRY_SCHEMA
        .forbidden_payloads
        .iter()
        .any(|forbidden| lower.contains(forbidden))
}

fn unsafe_metadata_text(value: &str) -> bool {
    forbidden_text(value)
        || value.contains('/')
        || value.contains('\\')
        || value.contains('.')
        || value.contains("..")
        || value.contains("::")
        || value.contains("=>")
        || value.contains('{')
        || value.contains('}')
        || value.contains('(')
        || value.contains(')')
        || looks_like_byte_range(value)
}

fn count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1..=2 => "1-2",
        3..=9 => "3-9",
        10..=49 => "10-49",
        50..=199 => "50-199",
        _ => "200+",
    }
}

fn count_bucket_u64(count: u64) -> &'static str {
    match count {
        0 => "0",
        1..=2 => "1-2",
        3..=9 => "3-9",
        10..=49 => "10-49",
        50..=199 => "50-199",
        _ => "200+",
    }
}

fn ratio_bucket(value: Option<f64>) -> &'static str {
    match value {
        None => "unknown",
        Some(value) if value <= 0.0 => "0",
        Some(value) if value <= 0.25 => "0-25",
        Some(value) if value <= 0.50 => "25-50",
        Some(value) if value <= 0.75 => "50-75",
        Some(_) => "75-100",
    }
}

fn token_savings_bucket(value: i128) -> &'static str {
    match value {
        value if value < 0 => "negative",
        0 => "0",
        1..=999 => "1-999",
        1000..=9_999 => "1000-9999",
        _ => "10000+",
    }
}

fn empty_bucket_map() -> Value {
    Value::Object(Map::new())
}

fn os_family() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => "other",
    }
}

fn unix_day() -> u64 {
    now_unix_seconds() / 86_400
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn is_local_http_endpoint(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("http://") else {
        return false;
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    local_authority_matches(&authority, "localhost")
        || local_authority_matches(&authority, "127.0.0.1")
        || local_authority_matches(&authority, "[::1]")
}

fn local_authority_matches(authority: &str, host: &str) -> bool {
    authority == host
        || authority
            .strip_prefix(host)
            .and_then(|suffix| suffix.strip_prefix(':'))
            .is_some_and(|port| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()))
}

fn looks_like_byte_range(value: &str) -> bool {
    let Some((start, end)) = value.split_once('-') else {
        return false;
    };
    !start.is_empty()
        && !end.is_empty()
        && start.bytes().all(|byte| byte.is_ascii_digit())
        && end.bytes().all(|byte| byte.is_ascii_digit())
}

fn env_equals(value: Option<String>, expected: &str) -> bool {
    value
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
}

fn env_truthy(value: Option<String>) -> bool {
    value.as_deref().is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no"
        )
    })
}

fn invalid_input(message: impl Into<String>) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempWorkspace;
    use std::cell::Cell;

    #[derive(Default)]
    struct FakeTransport {
        calls: Cell<usize>,
        fail: bool,
    }

    impl TelemetryUploadTransport for FakeTransport {
        fn upload(
            &self,
            _endpoint: &str,
            _payload: &str,
            _timeout: Duration,
        ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
            self.calls.set(self.calls.get() + 1);
            if self.fail {
                return Err(RepoGrammarError::InvalidInput(
                    "fake upload failed".to_string(),
                ));
            }
            Ok(TelemetryUploadReceipt {
                status_code: 204,
                receipt_id: "receipt-test".to_string(),
            })
        }
    }

    fn paths(workspace: &TempWorkspace) -> TelemetryPaths {
        let repository_root = workspace.path().join("repo");
        fs::create_dir_all(&repository_root).expect("repo root");
        TelemetryPaths {
            global_data_dir: workspace.path().join("global"),
            repository_root,
            state_dir_override: None,
        }
    }

    #[test]
    fn telemetry_and_research_consent_are_separate_and_disabled_by_default() {
        let consent = TelemetryConsent::default();

        assert_eq!(
            consent.anonymous_product_telemetry,
            ConsentDecision::Disabled
        );
        assert_eq!(consent.research_trace_collection, ConsentDecision::Disabled);
    }

    #[test]
    fn environment_disables_telemetry() {
        assert!(telemetry_disabled_by_environment(|key| {
            (key == "REPOGRAMMAR_TELEMETRY").then(|| "0".to_string())
        }));
        assert!(telemetry_disabled_by_environment(|key| {
            (key == "DO_NOT_TRACK").then(|| "1".to_string())
        }));
        assert!(telemetry_disabled_by_environment(|key| {
            (key == "CI").then(|| "true".to_string())
        }));
        assert!(!telemetry_disabled_by_environment(|_| None));
    }

    #[test]
    fn anonymous_schema_forbids_sensitive_payloads() {
        assert!(ANONYMOUS_TELEMETRY_SCHEMA
            .forbidden_payloads
            .contains(&"path"));
        assert!(ANONYMOUS_TELEMETRY_SCHEMA
            .forbidden_payloads
            .contains(&"prompt"));
    }

    #[test]
    fn anonymous_telemetry_consent_does_not_enable_research_trace_collection() {
        let workspace = TempWorkspace::new("telemetry-consent-separate");
        let paths = paths(&workspace);

        let status = set_anonymous_telemetry(&paths, true).expect("telemetry on");

        assert!(status.enabled);
        assert!(!status.research_enabled);
    }

    #[test]
    fn research_trace_consent_does_not_enable_anonymous_uploads() {
        let workspace = TempWorkspace::new("research-consent-separate");
        let paths = paths(&workspace);

        let status = set_research_trace(&paths, true).expect("research on");

        assert!(!status.enabled);
        assert!(status.research_enabled);
    }

    #[test]
    fn environment_opt_out_overrides_enabled_telemetry_consent() {
        let workspace = TempWorkspace::new("telemetry-env-disabled");
        let paths = paths(&workspace);
        set_anonymous_telemetry(&paths, true).expect("telemetry on");

        let status = telemetry_status(&paths, None, &|key| {
            (key == "DO_NOT_TRACK").then(|| "1".to_string())
        })
        .expect("status");

        assert!(status.enabled);
        assert!(status.disabled_by_environment);
        assert!(!status.effective_enabled);
    }

    #[test]
    fn anonymous_upload_payload_rejects_sensitive_fields() {
        let mut payload = json!({
            "schema_version": TELEMETRY_SCHEMA_VERSION,
            "source_snippets_returned": false,
        });
        payload["path"] = json!("src/main.py");

        assert!(validate_anonymous_payload(&payload).is_err());
    }

    #[test]
    fn telemetry_export_is_inspect_only_and_not_repo_identifying() {
        let workspace = TempWorkspace::new("telemetry-export-inspect-only");
        let paths = paths(&workspace);

        let report = export_anonymous_telemetry(&paths, "0.1.0", None, None).expect("export");
        let status = telemetry_status(&paths, None, &|_| None).expect("status");

        assert!(!report
            .payload
            .as_object()
            .unwrap()
            .contains_key("repository_instance_id"));
        assert_eq!(report.payload["external_dependency_signal"], "unknown");
        assert!(!paths.global_data_dir.exists());
        assert_eq!(status.queue_count, 0);
        assert_eq!(status.sent_receipt_count, 0);
    }

    #[test]
    fn telemetry_bucket_keys_reject_paths_hashes_and_byte_ranges() {
        let workspace = TempWorkspace::new("telemetry-bucket-key-validation");
        let paths = paths(&workspace);
        let payload = export_anonymous_telemetry(&paths, "0.1.0", None, None)
            .expect("export")
            .payload;

        for key in [
            "src/main.py",
            "sha256:abcd",
            "12-99",
            "module::symbol",
            "UserService.create",
        ] {
            let mut invalid = payload.clone();
            invalid["unknown_reason_code_counts_bucket"] = json!({ key: "1-2" });
            assert!(
                validate_anonymous_payload(&invalid).is_err(),
                "accepted sensitive bucket key {key}"
            );
        }
    }

    #[test]
    fn disabled_telemetry_does_not_call_upload_transport() {
        let workspace = TempWorkspace::new("telemetry-disabled-no-upload");
        let paths = paths(&workspace);
        let transport = FakeTransport::default();

        let report = upload_anonymous_telemetry(
            &paths,
            TelemetryUploadRequest {
                endpoint: Some("https://example.invalid/telemetry".to_string()),
                dry_run: false,
            },
            "0.1.0",
            None,
            None,
            &|_| None,
            &transport,
        )
        .expect("disabled upload");

        assert!(!report.uploaded);
        assert_eq!(transport.calls.get(), 0);
    }

    #[test]
    fn successful_upload_writes_receipt_and_clears_queue() {
        let workspace = TempWorkspace::new("telemetry-successful-upload");
        let paths = paths(&workspace);
        set_anonymous_telemetry(&paths, true).expect("telemetry on");
        let transport = FakeTransport::default();

        let report = upload_anonymous_telemetry(
            &paths,
            TelemetryUploadRequest {
                endpoint: Some("http://localhost:9191/telemetry".to_string()),
                dry_run: false,
            },
            "0.1.0",
            None,
            None,
            &|_| None,
            &transport,
        )
        .expect("upload");
        let status = telemetry_status(&paths, None, &|_| None).expect("status");

        assert!(report.uploaded);
        assert_eq!(transport.calls.get(), 1);
        assert_eq!(status.queue_count, 0);
        assert_eq!(status.sent_receipt_count, 1);
    }

    #[test]
    fn failed_upload_keeps_queue_for_retry() {
        let workspace = TempWorkspace::new("telemetry-failed-upload");
        let paths = paths(&workspace);
        set_anonymous_telemetry(&paths, true).expect("telemetry on");
        let transport = FakeTransport {
            fail: true,
            ..FakeTransport::default()
        };

        let error = upload_anonymous_telemetry(
            &paths,
            TelemetryUploadRequest {
                endpoint: Some("http://localhost:9191/telemetry".to_string()),
                dry_run: false,
            },
            "0.1.0",
            None,
            None,
            &|_| None,
            &transport,
        )
        .expect_err("upload failure");
        let status = telemetry_status(&paths, None, &|_| None).expect("status");

        assert!(error.to_string().contains("fake upload failed"));
        assert_eq!(transport.calls.get(), 1);
        assert_eq!(status.queue_count, 1);
        assert_eq!(status.sent_receipt_count, 0);
    }

    #[test]
    fn endpoint_validation_requires_https_except_localhost() {
        assert!(validate_telemetry_endpoint("https://telemetry.example.test/v1").is_ok());
        assert!(validate_telemetry_endpoint("http://localhost:9000/v1").is_ok());
        assert!(validate_telemetry_endpoint("http://127.0.0.1:9000/v1").is_ok());
        assert!(validate_telemetry_endpoint("http://[::1]:9000/v1").is_ok());
        assert!(validate_telemetry_endpoint("http://localhost.evil/v1").is_err());
        assert!(validate_telemetry_endpoint("http://127.0.0.1.evil/v1").is_err());
        assert!(validate_telemetry_endpoint("http://example.test/v1").is_err());
    }

    #[test]
    fn telemetry_payload_validation_rejects_schema_shape_drift() {
        let workspace = TempWorkspace::new("telemetry-schema-validation");
        let paths = paths(&workspace);
        let payload = export_anonymous_telemetry(&paths, "0.1.0", None, None)
            .expect("export")
            .payload;

        let mut missing = payload.clone();
        missing
            .as_object_mut()
            .expect("payload object")
            .remove("agent_target");
        assert!(validate_anonymous_payload(&missing).is_err());

        let mut bad_bucket = payload.clone();
        bad_bucket["eligible_code_units_bucket"] = json!("500 raw files");
        assert!(validate_anonymous_payload(&bad_bucket).is_err());

        let mut bad_identifier = payload;
        bad_identifier["anonymous_machine_id"] = json!("anon-not-a-hash");
        assert!(validate_anonymous_payload(&bad_identifier).is_err());
    }

    #[test]
    fn telemetry_protocol_schema_matches_application_allowlist() {
        let schema_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("protocol")
            .join("telemetry-v1.schema.json");
        let schema: Value =
            serde_json::from_str(&fs::read_to_string(schema_path).expect("schema")).expect("json");
        let required = schema["required"].as_array().expect("required fields");

        for field in ANONYMOUS_TELEMETRY_SCHEMA.allowed_fields {
            assert!(
                required.iter().any(|value| value.as_str() == Some(field)),
                "schema missing field {field}"
            );
        }
        assert!(schema["properties"].get("repository_instance_id").is_none());
        assert_eq!(
            schema["properties"]["source_snippets_returned"]["const"],
            false
        );
    }

    #[test]
    fn experiment_start_rejects_non_allowlisted_task_kind() {
        let workspace = TempWorkspace::new("experiment-task-kind");
        let data_dir = workspace.path().join("global");

        let error = experiment_start(
            &data_dir,
            ExperimentStartRequest {
                name: "task-a".to_string(),
                experiment_mode: ExperimentWorkflowMode::RecordExisting,
                mode: ExperimentMode::Baseline,
                measurement_source: MeasurementSource::UserEntered,
                coarse_task_kind: Some("src/main.py".to_string()),
                elapsed_time_bucket: None,
                read_plan_used: None,
                read_plan_item_count_bucket: None,
            },
        )
        .expect_err("invalid task kind");

        assert_eq!(error.to_string(), "coarse task kind is invalid");
    }

    #[test]
    fn token_savings_requires_paired_baseline_and_treatment_measurements() {
        let workspace = TempWorkspace::new("experiment-paired");
        let data_dir = workspace.path().join("global");

        experiment_start(
            &data_dir,
            ExperimentStartRequest {
                name: "task-a".to_string(),
                experiment_mode: ExperimentWorkflowMode::RecordExisting,
                mode: ExperimentMode::Baseline,
                measurement_source: MeasurementSource::UserEntered,
                coarse_task_kind: None,
                elapsed_time_bucket: None,
                read_plan_used: None,
                read_plan_item_count_bucket: None,
            },
        )
        .expect("start baseline");
        experiment_record(
            &data_dir,
            ExperimentRecordRequest {
                name: "task-a".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                tool_tokens: 25,
                success: true,
                test_outcome: TestOutcome::Passed,
            },
        )
        .expect("record baseline");
        let missing = experiment_stop(&data_dir, "task-a").expect("stop baseline");
        assert_eq!(missing.token_savings, None);
        assert_eq!(
            missing.reason.as_deref(),
            Some("missing comparable baseline/treatment measurement")
        );

        experiment_start(
            &data_dir,
            ExperimentStartRequest {
                name: "task-a".to_string(),
                experiment_mode: ExperimentWorkflowMode::RecordExisting,
                mode: ExperimentMode::Treatment,
                measurement_source: MeasurementSource::UserEntered,
                coarse_task_kind: None,
                elapsed_time_bucket: None,
                read_plan_used: Some(true),
                read_plan_item_count_bucket: Some("1-2".to_string()),
            },
        )
        .expect("start treatment");
        experiment_record(
            &data_dir,
            ExperimentRecordRequest {
                name: "task-a".to_string(),
                input_tokens: 80,
                output_tokens: 40,
                tool_tokens: 15,
                success: true,
                test_outcome: TestOutcome::Passed,
            },
        )
        .expect("record treatment");
        let report = experiment_stop(&data_dir, "task-a").expect("stop treatment");

        assert_eq!(report.baseline_total_tokens, Some(175));
        assert_eq!(report.treatment_total_tokens, Some(135));
        assert_eq!(report.token_savings, Some(40));
        assert_eq!(report.measurement_status, "paired_measurement_available");
        assert_eq!(report.claim_validity, "valid_for_product_claim");
        assert_eq!(report.measurement_source.as_deref(), Some("user_entered"));
        assert!(report.caveat.contains("comparable paired"));
    }

    #[test]
    fn treatment_failure_keeps_delta_but_invalidates_product_savings_claim() {
        let workspace = TempWorkspace::new("experiment-treatment-failed");
        let data_dir = workspace.path().join("global");

        for (mode, input_tokens, success) in [
            (ExperimentMode::Baseline, 100, true),
            (ExperimentMode::Treatment, 50, false),
        ] {
            experiment_start(
                &data_dir,
                ExperimentStartRequest {
                    name: "task-a".to_string(),
                    experiment_mode: ExperimentWorkflowMode::ControlledPair,
                    mode,
                    measurement_source: MeasurementSource::UserEntered,
                    coarse_task_kind: None,
                    elapsed_time_bucket: None,
                    read_plan_used: Some(mode == ExperimentMode::Treatment),
                    read_plan_item_count_bucket: None,
                },
            )
            .expect("start");
            experiment_record(
                &data_dir,
                ExperimentRecordRequest {
                    name: "task-a".to_string(),
                    input_tokens,
                    output_tokens: 10,
                    tool_tokens: 0,
                    success,
                    test_outcome: if success {
                        TestOutcome::Passed
                    } else {
                        TestOutcome::Failed
                    },
                },
            )
            .expect("record");
            experiment_stop(&data_dir, "task-a").expect("stop");
        }

        let report = experiment_report(&data_dir, "task-a").expect("report");

        assert_eq!(report.experiment_mode.as_deref(), Some("controlled_pair"));
        assert_eq!(report.token_savings, Some(50));
        assert_eq!(report.treatment_success, Some(false));
        assert_eq!(report.claim_validity, "invalid_for_product_claim");
        assert!(report.cost_notice_may_have_increased_usage);
    }

    #[test]
    fn experiment_records_do_not_store_prompt_source_or_paths() {
        let workspace = TempWorkspace::new("experiment-redacted");
        let data_dir = workspace.path().join("global");
        experiment_start(
            &data_dir,
            ExperimentStartRequest {
                name: "safe-task".to_string(),
                experiment_mode: ExperimentWorkflowMode::RecordExisting,
                mode: ExperimentMode::Baseline,
                measurement_source: MeasurementSource::HostReported,
                coarse_task_kind: Some("implementation".to_string()),
                elapsed_time_bucket: Some("1-5m".to_string()),
                read_plan_used: Some(false),
                read_plan_item_count_bucket: Some("0".to_string()),
            },
        )
        .expect("start");

        let serialized =
            fs::read_to_string(experiment_file(&data_dir, "safe-task")).expect("experiment JSON");
        assert!(!serialized.contains("prompt"));
        assert!(!serialized.contains("source_code"));
        assert!(!serialized.contains("source_snippet"));
        assert!(!serialized.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn experiment_export_is_redacted_by_default() {
        let workspace = TempWorkspace::new("experiment-export-redacted");
        let data_dir = workspace.path().join("global");
        for (mode, input_tokens) in [
            (ExperimentMode::Baseline, 100),
            (ExperimentMode::Treatment, 70),
        ] {
            experiment_start(
                &data_dir,
                ExperimentStartRequest {
                    name: "customer-task-42".to_string(),
                    experiment_mode: ExperimentWorkflowMode::RecordExisting,
                    mode,
                    measurement_source: MeasurementSource::UserEntered,
                    coarse_task_kind: Some("implementation".to_string()),
                    elapsed_time_bucket: Some("1-5m".to_string()),
                    read_plan_used: Some(mode == ExperimentMode::Treatment),
                    read_plan_item_count_bucket: Some("1-2".to_string()),
                },
            )
            .expect("start");
            experiment_record(
                &data_dir,
                ExperimentRecordRequest {
                    name: "customer-task-42".to_string(),
                    input_tokens,
                    output_tokens: 10,
                    tool_tokens: 0,
                    success: true,
                    test_outcome: TestOutcome::Passed,
                },
            )
            .expect("record");
            experiment_stop(&data_dir, "customer-task-42").expect("stop");
        }

        let exported = experiment_export(&data_dir, "customer-task-42").expect("export");
        let serialized = exported.to_string();

        assert_eq!(exported["name"], "redacted");
        assert_eq!(exported["redacted"], true);
        assert_eq!(exported["source_snippets_returned"], false);
        assert!(!serialized.contains("customer-task-42"));
        assert!(!serialized.contains("session_id"));
        assert!(!serialized.contains("input_tokens"));
        assert!(!serialized.contains("output_tokens"));
        assert!(!serialized.contains("total_tokens\":"));
        assert!(serialized.contains("token_savings_bucket"));
    }
}
