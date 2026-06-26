//! CLI argument boundary for the `repogrammar` binary.

use crate::application::indexing::IndexingOutcome;
use crate::application::install::{
    normalize_concrete_targets, owned_install_receipt_exists, plan_install,
    supported_concrete_targets, target_config_snippet, target_plan_line, targets_for_display,
    AgentTarget, InstallExecutionContext, InstallExecutionOutcome, InstallRequest, InstallScope,
};
use crate::application::progress::{ProgressEvent, WorkUnits};
use crate::application::query::{
    build_read_plan, query_preflight, read_plan_with_rendered_spans,
    repository_status_unavailable_fallback, select_family_evidence, validate_query_target,
    validate_query_token_budget, DiagnosticSignal, FamilyDetailReport, FamilyEvidenceMode,
    FamilyListReport, FamilyLookupMode, FamilyLookupReport, FamilyOutputOptions,
    FamilyQueryUnknown, FamilyUnknownReport, IndexedCodeUnitsReport, IndexedFilesReport,
    QueryPreflightOperation, QueryPreflightReport, ReadPlan, ReadPlanItem,
    RepoShapeDiagnosticsReport, SourceSpanRenderReport,
};
#[cfg(test)]
use crate::application::query::{
    ReadPlanPurpose, RenderedSourceSpan, SourceSpanOmission, SourceSpanPolicy,
};
use crate::application::repository::{
    init_repository, repository_doctor, repository_logs, repository_status, uninit_repository,
    unlock_repository, RepositoryDoctorCode, RepositoryDoctorFinding, RepositoryDoctorReport,
    RepositoryDoctorRequest, RepositoryDoctorSeverity, RepositoryImplementationStatus,
    RepositoryInitOutcome, RepositoryLifecycleInitRequest, RepositoryLogsReport,
    RepositoryLogsRequest, RepositoryManifestStatus, RepositoryStatus, RepositoryStatusReport,
    RepositoryStatusRequest, RepositoryUninitOutcome, RepositoryUninitRequest,
    RepositoryUnlockReport, RepositoryUnlockRequest,
};
use crate::application::telemetry::{
    experiment_export, experiment_purge, experiment_record, experiment_report,
    experiment_report_json, experiment_start, experiment_stop, export_anonymous_telemetry,
    latest_comparable_experiment_report, purge_telemetry, record_passive_diagnostics_rollup,
    research_export, research_purge, set_anonymous_telemetry, set_research_trace,
    telemetry_disabled_by_environment, telemetry_status, upload_anonymous_telemetry,
    validate_telemetry_endpoint, ExperimentMode, ExperimentRecordRequest, ExperimentStartRequest,
    ExperimentWorkflowMode, MeasurementSource, TelemetryDiagnostics, TelemetryExportReport,
    TelemetryPaths, TelemetryPurgeReport, TelemetryStatusReport, TelemetryUploadReceipt,
    TelemetryUploadReport, TelemetryUploadRequest, TelemetryUploadTransport, TestOutcome,
};
use crate::error::RepoGrammarError;
use serde_json::json;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliIndexRequest {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
    pub max_file_bytes: u64,
    pub strict_gitignore: bool,
    pub semantic_worker_executable: Option<String>,
    pub semantic_worker_args: Vec<String>,
    pub progress: ProgressMode,
    pub json: bool,
    pub quiet: bool,
    pub stderr_is_terminal: bool,
}

pub trait CliRuntime {
    fn index_repository(
        &self,
        command: &str,
        request: CliIndexRequest,
    ) -> Result<IndexingOutcome, RepoGrammarError>;

    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError>;

    fn repository_doctor(
        &self,
        request: RepositoryDoctorRequest,
    ) -> Result<RepositoryDoctorReport, RepoGrammarError>;

    fn indexed_files(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<IndexedFilesReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("files"))
    }

    fn indexed_units(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("units"))
    }

    fn families(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<FamilyListReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("families"))
    }

    fn family_lookup(
        &self,
        _request: RepositoryStatusRequest,
        _target: Option<&str>,
        _mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("family"))
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

    fn repo_shape_diagnostics(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("stats"))
    }

    fn install_agent_integration(
        &self,
        _command: &str,
        _request: InstallRequest,
        _context: InstallExecutionContext,
    ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("install"))
    }

    fn upload_telemetry_payload(
        &self,
        _endpoint: &str,
        _payload: &str,
        _timeout: std::time::Duration,
    ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("telemetry upload"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferredCliRuntime;

impl CliRuntime for DeferredCliRuntime {
    fn index_repository(
        &self,
        command: &str,
        _request: CliIndexRequest,
    ) -> Result<IndexingOutcome, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented(match command {
            "sync" => "sync",
            _ => "index",
        }))
    }

    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError> {
        repository_status(request)
    }

    fn repository_doctor(
        &self,
        request: RepositoryDoctorRequest,
    ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
        repository_doctor(request)
    }
}

pub trait InstallTelemetryPrompt {
    fn is_interactive(&self) -> bool {
        false
    }

    fn prompt_agent_selection(&self, _prompt: &str) -> Result<String, String> {
        Err(
            "interactive install requires a terminal; use --dry-run or --target <agent> --yes"
                .to_string(),
        )
    }

    fn prompt_install_telemetry_consent(&self, prompt: &str) -> Result<String, String> {
        self.prompt_experiment_consent(prompt)
    }

    fn prompt_install_confirmation(&self, _prompt: &str) -> Result<String, String> {
        Err(
            "interactive install requires a terminal; use --dry-run or --target <agent> --yes"
                .to_string(),
        )
    }

    fn prompt_experiment_consent(&self, _prompt: &str) -> Result<String, String> {
        Ok(String::new())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonInteractiveInstallTelemetryPrompt;

impl InstallTelemetryPrompt for NonInteractiveInstallTelemetryPrompt {}

pub const RECORD_EXISTING_EXPERIMENT_PROMPT: &str = "\
This mode records token counts from sessions you already performed.
It does not run extra agent sessions and should not increase token usage.

Experiment records are local by default.
Enable local experiment recording? [y/N] ";

pub const CONTROLLED_PAIR_EXPERIMENT_PROMPT: &str = "\
This mode is for controlled baseline/treatment measurement.

It may increase your token usage, time, and provider cost because you may run
one baseline session without RepoGrammar and one treatment session with RepoGrammar.

RepoGrammar will not run those sessions automatically.
You are responsible for deciding whether to run them and recording the token counts.

Enable controlled paired experiment recording? [y/N] ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutput {
    fn success(stdout: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn failure(status: i32, stderr: impl Into<String>) -> Self {
        Self {
            status,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

pub fn run<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_with_runtime(args, &DeferredCliRuntime)
}

pub fn run_with_runtime<I, S>(args: I, runtime: &impl CliRuntime) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_with_runtime_and_install_prompt(args, runtime, &NonInteractiveInstallTelemetryPrompt)
}

pub fn run_with_runtime_and_install_prompt<I, S>(
    args: I,
    runtime: &impl CliRuntime,
    install_prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            return CliOutput::failure(1, format!("failed to read current directory: {error}\n"));
        }
    };
    let env_lookup = |key: &str| std::env::var(key).ok();
    run_with_context_runtime_prompt(args, &current_dir, &env_lookup, runtime, install_prompt)
}

#[cfg(test)]
fn run_with_context<I, S, F>(args: I, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    run_with_context_and_runtime(args, current_dir, env_lookup, &DeferredCliRuntime)
}

#[cfg(test)]
fn run_with_context_and_runtime<I, S, F>(
    args: I,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    run_with_context_runtime_prompt(
        args,
        current_dir,
        env_lookup,
        runtime,
        &NonInteractiveInstallTelemetryPrompt,
    )
}

fn run_with_context_runtime_prompt<I, S, F>(
    args: I,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
    install_prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    match args.as_slice() {
        [] => CliOutput::success(usage()),
        [arg] if arg == "--help" || arg == "-h" => CliOutput::success(usage()),
        [arg] if arg == "--version" || arg == "-V" => {
            CliOutput::success(format!("repogrammar {}\n", env!("CARGO_PKG_VERSION")))
        }
        [command] if command == "version" => {
            CliOutput::success(format!("repogrammar {}\n", env!("CARGO_PKG_VERSION")))
        }
        [command] if command == "help" => CliOutput::success(usage()),
        [command, rest @ ..] if is_project_lifecycle_command(command) => {
            handle_project_lifecycle(command, rest, current_dir, env_lookup, runtime)
        }
        [command, rest @ ..] if is_query_command(command) => {
            handle_query(command, rest, current_dir, env_lookup, runtime)
        }
        [command, rest @ ..] if is_installer_command(command) => handle_installer(
            command,
            rest,
            current_dir,
            env_lookup,
            runtime,
            install_prompt,
        ),
        [command, rest @ ..] if command == "stats" => {
            handle_stats(rest, current_dir, env_lookup, runtime)
        }
        [command, rest @ ..] if command == "telemetry" => handle_telemetry(
            rest,
            current_dir,
            env_lookup,
            runtime,
            install_prompt,
        ),
        [command] if is_forbidden_graph_command(command) => CliOutput::failure(
            2,
            format!(
                "repogrammar {command} is not a v0.1 top-level command; pattern-family commands are primary, and future graph navigation must live under a secondary namespace\n"
            ),
        ),
        [unknown, ..] => CliOutput::failure(2, format!("unknown command or option: {unknown}\n")),
    }
}

fn usage() -> String {
    [
        "Usage: repogrammar <command> [options]",
        "",
        "Project lifecycle: init, uninit, index, sync, status, doctor, unlock, logs",
        "Pattern-family queries: find, families, family, member, explain, check, files, units",
        "Agent integration: serve, install, uninstall",
        "Metrics: stats, telemetry",
        "Maintenance: version, help",
        "",
    ]
    .join("\n")
}

fn is_project_lifecycle_command(command: &str) -> bool {
    matches!(
        command,
        "init" | "uninit" | "index" | "sync" | "status" | "doctor" | "unlock" | "logs"
    )
}

fn is_query_command(command: &str) -> bool {
    matches!(
        command,
        "find" | "families" | "family" | "member" | "explain" | "check" | "files" | "units"
    )
}

fn is_installer_command(command: &str) -> bool {
    matches!(command, "serve" | "install" | "uninstall")
}

fn is_forbidden_graph_command(command: &str) -> bool {
    matches!(
        command,
        "callers" | "callees" | "impact" | "affected" | "node" | "explore"
    )
}

fn handle_project_lifecycle<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    if command == "logs" {
        return match parse_logs_options(rest) {
            Ok(options) => handle_logs(&options, current_dir, env_lookup),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }

    let options = match parse_lifecycle_options(command, rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };

    match command {
        "init" => handle_init(&options, current_dir, env_lookup),
        "uninit" => handle_uninit(&options, current_dir, env_lookup),
        "status" => handle_status(&options, current_dir, env_lookup, runtime),
        "doctor" => handle_doctor(&options, current_dir, env_lookup, runtime),
        "unlock" => handle_unlock(&options, current_dir, env_lookup),
        "index" | "sync" => handle_index(command, &options, current_dir, env_lookup, runtime),
        _ => CliOutput::failure(2, format!("unknown project lifecycle command: {command}\n")),
    }
}

fn handle_query<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_query_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };
    let operation = if matches!(command, "files" | "units") {
        QueryPreflightOperation::ActiveIndexInventory
    } else {
        QueryPreflightOperation::PatternFamilyQuery
    };
    let status_report = match runtime.repository_status(request.clone()) {
        Ok(report) => report,
        Err(_) => {
            let fallback = repository_status_unavailable_fallback(operation);
            return query_fallback(
                command,
                options.json,
                fallback.reason,
                fallback.guidance,
                fallback.implemented,
            );
        }
    };

    match query_preflight(operation, &status_report) {
        QueryPreflightReport::Fallback(fallback) => {
            return query_fallback(
                command,
                options.json,
                fallback.reason,
                fallback.guidance,
                fallback.implemented,
            );
        }
        QueryPreflightReport::Ready => {}
    }

    if matches!(command, "files" | "units") {
        return match command {
            "files" => match runtime.indexed_files(request) {
                Ok(report) if options.json => CliOutput::success(indexed_files_json(&report)),
                Ok(report) => CliOutput::success(indexed_files_human(&report)),
                Err(_) => query_fallback(
                    command,
                    options.json,
                    "repository status is unavailable",
                    "run repogrammar doctor",
                    true,
                ),
            },
            "units" => match runtime.indexed_units(request) {
                Ok(report) if options.json => CliOutput::success(indexed_units_json(&report)),
                Ok(report) => CliOutput::success(indexed_units_human(&report)),
                Err(_) => query_fallback(
                    command,
                    options.json,
                    "repository status is unavailable",
                    "run repogrammar doctor",
                    true,
                ),
            },
            _ => unreachable!("files/units branch only"),
        };
    }

    match command {
        "families" => match runtime.families(request) {
            Ok(report) if options.json => CliOutput::success(families_json(command, &report)),
            Ok(report) => CliOutput::success(families_human(&report)),
            Err(_) => query_fallback(
                command,
                options.json,
                "repository status is unavailable",
                "run repogrammar doctor",
                false,
            ),
        },
        "family" | "member" | "find" | "explain" | "check" => {
            match runtime.family_lookup(
                request.clone(),
                options.target.as_deref(),
                lookup_mode_for_command(command),
            ) {
                Ok(report) if options.json => {
                    let source_spans = match maybe_render_source_spans(
                        runtime,
                        request,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        options.include_source_spans,
                    ) {
                        Ok(source_spans) => source_spans,
                        Err(_) => {
                            return query_fallback(
                                command,
                                options.json,
                                "repository status is unavailable",
                                "run repogrammar doctor",
                                false,
                            );
                        }
                    };
                    CliOutput::success(family_lookup_json(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        source_spans.as_ref(),
                    ))
                }
                Ok(report) => {
                    let source_spans = match maybe_render_source_spans(
                        runtime,
                        request,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        options.include_source_spans,
                    ) {
                        Ok(source_spans) => source_spans,
                        Err(_) => {
                            return query_fallback(
                                command,
                                options.json,
                                "repository status is unavailable",
                                "run repogrammar doctor",
                                false,
                            );
                        }
                    };
                    CliOutput::success(family_lookup_human(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        source_spans.as_ref(),
                    ))
                }
                Err(_) => query_fallback(
                    command,
                    options.json,
                    "repository status is unavailable",
                    "run repogrammar doctor",
                    false,
                ),
            }
        }
        _ => query_fallback(
            command,
            options.json,
            "query execution requires pattern-family evidence",
            "run repogrammar index after pattern-family indexing is implemented",
            false,
        ),
    }
}

fn lookup_mode_for_command(command: &str) -> FamilyLookupMode {
    match command {
        "family" => FamilyLookupMode::ExactFamilyId,
        "member" => FamilyLookupMode::ExactMemberId,
        "find" | "explain" | "check" => FamilyLookupMode::FuzzyQuery,
        _ => FamilyLookupMode::FuzzyQuery,
    }
}

fn maybe_render_source_spans(
    runtime: &impl CliRuntime,
    request: RepositoryStatusRequest,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    include_source_spans: bool,
) -> Result<Option<SourceSpanRenderReport>, RepoGrammarError> {
    if !include_source_spans {
        return Ok(None);
    }
    let FamilyLookupReport::Found(family) = report else {
        return Ok(None);
    };
    let read_plan = build_read_plan(family, target, mode, options);
    runtime
        .render_source_spans(
            request,
            &read_plan,
            include_source_spans,
            options.token_budget,
        )
        .map(Some)
}

fn query_fallback(
    command: &str,
    json: bool,
    reason: &str,
    guidance: &str,
    implemented: bool,
) -> CliOutput {
    if json {
        return CliOutput::failure(
            2,
            json_line(json!({
                "status": "FALLBACK_TO_CODE_SEARCH",
                "reason": reason,
                "guidance": guidance,
                "command": command,
                "implemented": implemented,
            })),
        );
    }

    let command_detail = if implemented {
        format!(
            "command: repogrammar {command} requires a readable active syntax-only index; no pattern-family claims were made\n"
        )
    } else {
        format!(
            "command: repogrammar {command} is not implemented yet; query execution requires a validated pattern-family index\n"
        )
    };
    CliOutput::failure(
        2,
        format!(
            "FALLBACK_TO_CODE_SEARCH\nreason: {reason}\nguidance: {guidance}\n{command_detail}"
        ),
    )
}

fn indexed_files_human(report: &IndexedFilesReport) -> String {
    let mut output = format!(
        "files: active index metadata\nactive_generation: {}\nindexed_files: {}\nindexing: {}\n",
        report.active_generation,
        report.files.len(),
        report.indexing
    );
    for file in &report.files {
        output.push_str(&format!(
            "file: {}\tlanguage: {}\tsize_bytes: {}\tcontent_hash: {}\n",
            file.path,
            file.language,
            file.size_bytes,
            file.content_hash.as_str()
        ));
    }
    output
}

fn indexed_files_json(report: &IndexedFilesReport) -> String {
    let files = report
        .files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "language": file.language,
                "size_bytes": file.size_bytes,
                "content_hash": file.content_hash.as_str(),
            })
        })
        .collect::<Vec<_>>();
    json_line(json!({
        "command": "files",
        "status": "ok",
        "implemented": true,
        "active_generation": report.active_generation,
        "indexing": report.indexing,
        "files": files,
    }))
}

fn indexed_units_human(report: &IndexedCodeUnitsReport) -> String {
    let mut output = format!(
        "units: active index code units\nactive_generation: {}\nindexed_units: {}\nindexing: {}\nsemantic_worker: deferred\nmining: deferred\n",
        report.active_generation,
        report.units.len(),
        report.indexing
    );
    for unit in &report.units {
        output.push_str(&format!(
            "unit: {}\tpath: {}\tlanguage: {}\tkind: {}\trange: {}-{}\tcontent_hash: {}\n",
            unit.id,
            unit.path,
            unit.language,
            unit.kind,
            unit.start_byte,
            unit.end_byte,
            unit.content_hash.as_str()
        ));
    }
    output
}

fn indexed_units_json(report: &IndexedCodeUnitsReport) -> String {
    let units = report
        .units
        .iter()
        .map(|unit| {
            json!({
                "id": unit.id,
                "path": unit.path,
                "language": unit.language,
                "kind": unit.kind,
                "start_byte": unit.start_byte,
                "end_byte": unit.end_byte,
                "content_hash": unit.content_hash.as_str(),
            })
        })
        .collect::<Vec<_>>();
    json_line(json!({
        "command": "units",
        "status": "ok",
        "implemented": true,
        "active_generation": report.active_generation,
        "indexing": report.indexing,
        "semantic_worker": "deferred",
        "mining": "deferred",
        "units": units,
    }))
}

fn families_human(report: &FamilyListReport) -> String {
    if report.families.is_empty() {
        let mut output = format!(
            "families: UNKNOWN\nactive_generation: {}\n",
            report.active_generation
        );
        if report.unknowns.is_empty() {
            output.push_str("unknown: blocking_unknown:InsufficientSupport affected_claim: repository pattern families\n");
            output.push_str(
                "recovery: run repogrammar index after adding compatible implementations\n",
            );
        } else {
            for unknown in &report.unknowns {
                push_unknown_human(&mut output, unknown);
            }
        }
        return output;
    }
    let mut output = format!(
        "families: evidence-backed pattern families\nactive_generation: {}\ncount: {}\n",
        report.active_generation,
        report.families.len()
    );
    for family in &report.families {
        output.push_str(&format!(
            "family: {}\tclassification: {}\tsupport: {}\n",
            family.family_id, family.classification, family.support
        ));
    }
    output
}

fn families_json(command: &str, report: &FamilyListReport) -> String {
    json_line(json!({
        "command": command,
        "status": if report.families.is_empty() { "UNKNOWN" } else { "ok" },
        "implemented": true,
        "active_generation": report.active_generation,
        "families": report.families.iter().map(|family| {
            json!({
                "family_id": family.family_id,
                "classification": family.classification,
                "support": family.support,
            })
        }).collect::<Vec<_>>(),
        "unknowns": unknowns_json(&report.unknowns),
    }))
}

fn family_lookup_human(
    command: &str,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    source_spans: Option<&SourceSpanRenderReport>,
) -> String {
    match report {
        FamilyLookupReport::Found(family) => {
            let selected_evidence = select_family_evidence(family, options);
            let base_read_plan = build_read_plan(family, target, mode, options);
            let read_plan = source_spans
                .map(|rendered| read_plan_with_rendered_spans(&base_read_plan, rendered))
                .unwrap_or(base_read_plan);
            let snippets = if read_plan.source_snippets_included {
                "included"
            } else {
                "not_included"
            };
            let mut output = if command == "check" {
                format!(
                    "{command}: CONTEXT_ONLY\nactive_generation: {}\nfamily: {}\nclassification: {}\nsupport: {}\nevidence_mode: {}\nestimated_evidence_tokens: {}\nsource_snippets: {}\n",
                    family.active_generation,
                    family.family_id,
                    family.classification,
                    family.support,
                    selected_evidence.mode.as_str(),
                    selected_evidence.estimated_tokens,
                    snippets
                )
            } else {
                format!(
                    "{command}: evidence-backed family\nactive_generation: {}\nfamily: {}\nclassification: {}\nsupport: {}\nevidence_mode: {}\nestimated_evidence_tokens: {}\nsource_snippets: {}\n",
                    family.active_generation,
                    family.family_id,
                    family.classification,
                    family.support,
                    selected_evidence.mode.as_str(),
                    selected_evidence.estimated_tokens,
                    snippets
                )
            };
            output.push_str(&format!(
                "evidence_selection: {}\n",
                selected_evidence.selection_strategy
            ));
            output.push_str(&format!(
                "budget_satisfied: {}\n",
                selected_evidence.budget_satisfied
            ));
            output.push_str(&format!(
                "estimated_read_plan_tokens: {}\n",
                read_plan.estimated_tokens
            ));
            output.push_str(&format!(
                "read_plan_requires_source_before_edit: {}\n",
                read_plan.requires_source_before_edit
            ));
            if !selected_evidence.covered_claims.is_empty() {
                output.push_str(&format!(
                    "covered_claims: {}\n",
                    selected_evidence.covered_claims.join(",")
                ));
            }
            if !selected_evidence.missing_claims.is_empty() {
                output.push_str(&format!(
                    "missing_claims: {}\n",
                    selected_evidence.missing_claims.join(",")
                ));
            }
            push_read_plan_human(&mut output, &read_plan, selected_evidence.mode);
            if let Some(source_spans) = source_spans {
                push_source_spans_human(&mut output, source_spans);
            }
            if command == "check" {
                output.push_str("advisory_status: UNKNOWN\n");
                output.push_str("reason: runtime equivalence remains unproven\n");
            }
            for member in &family.members {
                output.push_str(&format!(
                    "member: {}\trole: {}\n",
                    member.code_unit_id, member.role
                ));
            }
            for evidence in &selected_evidence.evidence {
                let record = &evidence.record;
                output.push_str(&format!(
                    "evidence: {}\tpath: {}\trange: {}-{}\tcontent_hash: {}\testimated_tokens: {}\tcovered_claims: {}\n",
                    record.evidence_id,
                    record.path,
                    record.start_byte,
                    record.end_byte,
                    record.content_hash.as_str(),
                    evidence.estimated_tokens,
                    evidence.covered_claims.join(",")
                ));
            }
            for slot in &family.variation_slots {
                output.push_str(&format!(
                    "variation_slot: {}\tdescription: {}\n",
                    slot.slot_id, slot.description
                ));
            }
            for unknown in &family.unknowns {
                push_unknown_human(&mut output, unknown);
            }
            output
        }
        FamilyLookupReport::Unknown(report) => family_unknown_human(command, report),
    }
}

fn family_unknown_human(command: &str, report: &FamilyUnknownReport) -> String {
    let mut output = format!(
        "{command}: UNKNOWN\nactive_generation: {}\n",
        report.active_generation
    );
    for unknown in &report.unknowns {
        push_unknown_human(&mut output, unknown);
    }
    output
}

fn push_unknown_human(output: &mut String, unknown: &FamilyQueryUnknown) {
    output.push_str(&format!(
        "unknown: {}:{} affected_claim: {}\n",
        unknown.class.as_protocol_str(),
        unknown.reason.as_protocol_str(),
        unknown.affected_claim
    ));
    if let Some(recovery) = &unknown.recovery {
        output.push_str("recovery: ");
        output.push_str(recovery);
        output.push('\n');
    }
}

fn family_lookup_json(
    command: &str,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    source_spans: Option<&SourceSpanRenderReport>,
) -> String {
    match report {
        FamilyLookupReport::Found(family) => {
            family_detail_json(command, family, target, mode, options, source_spans)
        }
        FamilyLookupReport::Unknown(report) => json_line(json!({
            "command": command,
            "status": "UNKNOWN",
            "implemented": true,
            "active_generation": report.active_generation,
            "unknowns": unknowns_json(&report.unknowns),
        })),
    }
}

fn family_detail_json(
    command: &str,
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    source_spans: Option<&SourceSpanRenderReport>,
) -> String {
    let selected_evidence = select_family_evidence(family, options);
    let base_read_plan = build_read_plan(family, target, mode, options);
    let read_plan = source_spans
        .map(|rendered| read_plan_with_rendered_spans(&base_read_plan, rendered))
        .unwrap_or(base_read_plan);
    let check = if command == "check" {
        Some(json!({
            "advisory_status": "UNKNOWN",
            "reason": "runtime equivalence remains unproven",
            "fail_on": "none",
        }))
    } else {
        None
    };
    json_line(json!({
        "command": command,
        "status": if command == "check" { "CONTEXT_ONLY" } else { "ok" },
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
            "source_snippets_included": read_plan.source_snippets_included,
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
        "read_plan": read_plan_json(&read_plan),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&family.unknowns),
        "check": check,
    }))
}

fn push_read_plan_human(output: &mut String, read_plan: &ReadPlan, mode: FamilyEvidenceMode) {
    output.push_str("Suggested source spans to read\n");
    output.push_str(&format!(
        "read_plan: items: {}\testimated_tokens: {}\tsource_snippets: {}\n",
        read_plan.items.len(),
        read_plan.estimated_tokens,
        if read_plan.source_snippets_included {
            "included"
        } else {
            "not_included"
        }
    ));
    let limit = if mode == FamilyEvidenceMode::Compact {
        1
    } else {
        read_plan.items.len()
    };
    for item in read_plan.items.iter().take(limit) {
        push_read_plan_item_human(output, item);
    }
    if read_plan.items.len() > limit {
        output.push_str(&format!(
            "read_plan_additional_items: {}\n",
            read_plan.items.len() - limit
        ));
    }
}

fn push_read_plan_item_human(output: &mut String, item: &ReadPlanItem) {
    let line_range = match (item.start_line, item.end_line) {
        (Some(start), Some(end)) => format!("lines: {start}-{end}"),
        _ => "lines: unavailable".to_string(),
    };
    output.push_str(&format!(
        "read: {}\tpath: {}\trange: {}-{}\t{}\tcontent_hash: {}\testimated_tokens: {}\trequires_source_before_edit: {}\twhy: {}\n",
        item.purpose.as_str(),
        item.path,
        item.start_byte,
        item.end_byte,
        line_range,
        item.content_hash.as_str(),
        item.estimated_tokens,
        item.source_required_before_edit,
        item.why
    ));
}

fn push_source_spans_human(output: &mut String, source_spans: &SourceSpanRenderReport) {
    output.push_str(&format!(
        "source_span_policy: requested: {}\tincluded: {}\testimated_tokens: {}\tbudget_satisfied: {}\tstrategy: {}\n",
        source_spans.policy.requested,
        source_spans.policy.source_snippets_included,
        source_spans.policy.estimated_tokens,
        source_spans.policy.budget_satisfied,
        source_spans.policy.selection_strategy
    ));
    output.push_str("source_span_guidance: ");
    output.push_str(source_spans.policy.fallback_guidance);
    output.push('\n');
    for span in &source_spans.spans {
        output.push_str(&format!(
            "source_span: {}\tpath: {}\trange: {}-{}\tlines: {}-{}\testimated_tokens: {}\trequires_source_before_edit: {}\n",
            span.purpose.as_str(),
            span.path,
            span.start_byte,
            span.end_byte,
            span.start_line,
            span.end_line,
            span.estimated_tokens,
            span.source_required_before_edit
        ));
        output.push_str(&span.text);
        output.push('\n');
    }
    for omission in &source_spans.omissions {
        output.push_str(&format!(
            "source_span_omitted: {}\tpath: {}\trange: {}-{}\treason: {}\tguidance: {}\n",
            omission.purpose.as_str(),
            omission.path,
            omission.start_byte,
            omission.end_byte,
            omission.reason,
            omission.guidance
        ));
    }
}

fn read_plan_json(read_plan: &ReadPlan) -> serde_json::Value {
    json!({
        "estimated_tokens": read_plan.estimated_tokens,
        "source_snippets_included": read_plan.source_snippets_included,
        "requires_source_before_edit": read_plan.requires_source_before_edit,
        "selection_strategy": read_plan.selection_strategy,
        "budget_satisfied": read_plan.budget_satisfied,
        "items": read_plan.items.iter().map(read_plan_item_json).collect::<Vec<_>>(),
    })
}

fn read_plan_item_json(item: &ReadPlanItem) -> serde_json::Value {
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

fn source_spans_json(source_spans: Option<&SourceSpanRenderReport>) -> serde_json::Value {
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

fn unknowns_json(unknowns: &[FamilyQueryUnknown]) -> Vec<serde_json::Value> {
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

fn handle_installer<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
    install_prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    if command == "serve" {
        if let Err(error) = parse_serve_options(rest) {
            return CliOutput::failure(2, format!("{error}\n"));
        }
        return CliOutput::failure(
            2,
            "repogrammar serve runs through the product stdio runtime; use the repogrammar binary for read-only MCP serving\n",
        );
    }

    let mut request = match parse_install_options(rest) {
        Ok(request) => request,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let telemetry_env_disabled = telemetry_disabled_by_environment(env_lookup);
    if telemetry_env_disabled {
        request.telemetry_enabled = false;
    }
    let plan = plan_install(&request);

    if request.print_config && !request.dry_run {
        return match install_print_config_output(&request) {
            Ok(output) => CliOutput::success(output),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }

    if request.dry_run {
        let mut output = format!(
            "{command} dry-run: target={}, scope={}, telemetry={}\n",
            plan.target.as_str(),
            plan.scope.as_str(),
            if plan.telemetry_enabled { "on" } else { "off" }
        );
        if request.print_config {
            output.push_str("config preview: absolute executable path, MCP self-test, reversible receipt, and marker-fenced instruction edits are required\n");
            match install_print_config_output(&request) {
                Ok(config) => output.push_str(&config),
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            }
        }
        for line in install_dry_run_native_plan(&request) {
            output.push_str(&line);
            output.push('\n');
        }
        output.push_str(
            "anonymous telemetry does not run paired token-saving experiments or add model calls\n",
        );
        CliOutput::success(output)
    } else {
        let mut wizard_prefix = String::new();
        if command == "install" && !request.assume_yes && !install_prompt.is_interactive() {
            return CliOutput::failure(
                2,
                "interactive install requires a terminal; use --dry-run or --target <agent> --yes\n",
            );
        }
        let context = match install_execution_context(current_dir, env_lookup) {
            Ok(context) => context,
            Err(error) => return CliOutput::failure(2, format!("{error}\n")),
        };
        if command == "install" && !request.assume_yes {
            match run_interactive_install_wizard(
                &mut request,
                &context,
                env_lookup,
                telemetry_env_disabled,
                install_prompt,
            ) {
                Ok(InteractiveInstallDecision::Proceed(prefix)) => {
                    wizard_prefix = prefix;
                }
                Ok(InteractiveInstallDecision::Exit(output)) => {
                    return CliOutput::success(output);
                }
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            }
        } else if !request.assume_yes {
            return CliOutput::failure(
                2,
                format!("{command} live writes require --yes; rerun with --dry-run to inspect the safe integration plan\n"),
            );
        }
        match runtime.install_agent_integration(command, request.clone(), context.clone()) {
            Ok(outcome) => {
                let mut output = wizard_prefix;
                output.push_str(&install_outcome_human(&outcome));
                if command == "install" {
                    match apply_install_telemetry_preference(&request, &context, env_lookup) {
                        Ok(status) => output.push_str(&install_telemetry_human(&status)),
                        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
                    }
                }
                CliOutput::success(output)
            }
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallAgentStatus {
    target: AgentTarget,
    detected: bool,
    installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveInstallDecision {
    Proceed(String),
    Exit(String),
}

fn run_interactive_install_wizard<F>(
    request: &mut InstallRequest,
    context: &InstallExecutionContext,
    env_lookup: &F,
    telemetry_env_disabled: bool,
    install_prompt: &impl InstallTelemetryPrompt,
) -> Result<InteractiveInstallDecision, String>
where
    F: Fn(&str) -> Option<String>,
{
    let statuses = install_agent_statuses(context, request.scope, env_lookup)?;
    let selection = prompt_agent_selection_until_valid(&statuses, install_prompt)?;
    let Some(selected_targets) = selection else {
        return Ok(InteractiveInstallDecision::Exit(
            "install cancelled; no changes made\n".to_string(),
        ));
    };
    if selected_targets.is_empty() {
        return Ok(InteractiveInstallDecision::Exit(
            "install: selected agents are already managed by RepoGrammar; no changes made\n"
                .to_string(),
        ));
    }
    request.selected_targets = selected_targets;
    request.target = if request.selected_targets.len() == supported_concrete_targets().len() {
        AgentTarget::AllSupported
    } else {
        request.selected_targets[0]
    };

    if !request.telemetry_explicitly_configured && !telemetry_env_disabled {
        let telemetry_response = install_prompt.prompt_install_telemetry_consent(
            "Telemetry is optional and anonymous.\nIt never includes source code, prompts, paths, repo names, symbols, content hashes,\nbyte ranges, raw targets, evidence text, patches, diffs, env vars, credentials,\nor raw errors.\n\nEnable anonymous telemetry? [y/N] ",
        )?;
        request.telemetry_enabled = parse_default_no_prompt_response(&telemetry_response)?;
        request.telemetry_explicitly_configured = true;
    }
    if telemetry_env_disabled {
        request.telemetry_enabled = false;
    }

    let plan = interactive_install_plan(request, &statuses);
    let confirmation = install_prompt
        .prompt_install_confirmation(&format!("{plan}\nProceed with install? [y/N] "))?;
    if !parse_default_no_prompt_response(&confirmation)? {
        return Ok(InteractiveInstallDecision::Exit(
            "install cancelled; no changes made\n".to_string(),
        ));
    }
    request.assume_yes = true;
    Ok(InteractiveInstallDecision::Proceed(plan))
}

fn install_agent_statuses<F>(
    context: &InstallExecutionContext,
    scope: InstallScope,
    env_lookup: &F,
) -> Result<Vec<InstallAgentStatus>, String>
where
    F: Fn(&str) -> Option<String>,
{
    supported_concrete_targets()
        .into_iter()
        .map(|target| {
            let installed = owned_install_receipt_exists(context, target, scope)
                .map_err(|error| error.to_string())?;
            Ok(InstallAgentStatus {
                target,
                detected: agent_cli_detected(target, env_lookup),
                installed,
            })
        })
        .collect()
}

fn prompt_agent_selection_until_valid(
    statuses: &[InstallAgentStatus],
    install_prompt: &impl InstallTelemetryPrompt,
) -> Result<Option<Vec<AgentTarget>>, String> {
    let base_prompt = install_agent_selection_prompt(statuses);
    let mut prompt = base_prompt.clone();
    let mut last_error = None;
    for _ in 0..3 {
        let response = install_prompt.prompt_agent_selection(&prompt)?;
        match parse_interactive_agent_selection(&response, statuses) {
            Ok(selection) => return Ok(selection),
            Err(error) => {
                prompt = format!("Invalid selection: {error}\n\n{base_prompt}");
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "invalid agent selection".to_string()))
}

fn install_agent_selection_prompt(statuses: &[InstallAgentStatus]) -> String {
    let mut prompt = String::from(
        "RepoGrammar installer\n\nThis configures RepoGrammar as a read-only MCP server for coding agents.\nIt does not index this repository.\nIt does not create or modify .repogrammar/.\nIt does not enable telemetry unless you explicitly opt in.\n\nDetected agents:\n",
    );
    for (index, status) in statuses.iter().enumerate() {
        prompt.push_str(&format!(
            "  [{}] {:<15} {:<12} {}\n",
            index + 1,
            install_target_label(status.target),
            if status.detected {
                "detected"
            } else {
                "not detected"
            },
            if status.installed {
                "installed"
            } else {
                "not installed"
            }
        ));
    }
    if statuses.iter().all(|status| !status.detected) {
        prompt.push_str(
            "\nWarning: no supported agent CLI was detected on PATH; selected native configuration may fail.\n",
        );
    }
    prompt.push_str(
        "\nSelect agents to configure:\n  1 = Codex\n  2 = Claude Code\n  1,2 = both\n  a = all available not-yet-installed agents\n  q = cancel\n\nSelection [a]: ",
    );
    prompt
}

fn parse_interactive_agent_selection(
    response: &str,
    statuses: &[InstallAgentStatus],
) -> Result<Option<Vec<AgentTarget>>, String> {
    let trimmed = response.trim();
    if trimmed.eq_ignore_ascii_case("q") {
        return Ok(None);
    }
    let mut selected = Vec::new();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("a")
        || trimmed.eq_ignore_ascii_case("all")
    {
        selected = default_interactive_targets(statuses);
    } else {
        for token in trimmed.split(',').map(str::trim) {
            let normalized = token.to_ascii_lowercase();
            let target = match normalized.as_str() {
                "1" => AgentTarget::Codex,
                "2" => AgentTarget::ClaudeCode,
                "codex" => AgentTarget::Codex,
                "claude" | "claude-code" => AgentTarget::ClaudeCode,
                _ => {
                    return Err(
                        "unknown agent selection; use 1, 2, 1,2, codex, claude-code, all, or q"
                            .to_string(),
                    )
                }
            };
            if !selected.contains(&target) {
                selected.push(target);
            }
        }
    }
    let selected = normalize_concrete_targets(&selected).map_err(|error| error.to_string())?;
    Ok(Some(selected))
}

fn default_interactive_targets(statuses: &[InstallAgentStatus]) -> Vec<AgentTarget> {
    let detected_missing = statuses
        .iter()
        .filter(|status| status.detected && !status.installed)
        .map(|status| status.target)
        .collect::<Vec<_>>();
    if !detected_missing.is_empty() {
        return detected_missing;
    }
    let missing = statuses
        .iter()
        .filter(|status| !status.installed)
        .map(|status| status.target)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        statuses.iter().map(|status| status.target).collect()
    } else {
        missing
    }
}

fn interactive_install_plan(request: &InstallRequest, statuses: &[InstallAgentStatus]) -> String {
    let mut output = String::from("Plan:\n  - Run read-only MCP self-test\n");
    let selected = if request.selected_targets.is_empty() {
        supported_concrete_targets()
    } else {
        request.selected_targets.clone()
    };
    for target in selected {
        let installed = statuses
            .iter()
            .find(|status| status.target == target)
            .map(|status| status.installed)
            .unwrap_or(false);
        if installed {
            output.push_str(&format!(
                "  - Skip {}: already managed by RepoGrammar\n",
                install_target_label(target)
            ));
        } else {
            output.push_str(&format!(
                "  - Configure {} MCP: {}\n",
                install_target_label(target),
                native_command_shape(target)
            ));
        }
    }
    output.push_str(
        "  - Install the repogrammar command in a user-writable command directory\n  - Write RepoGrammar-owned receipts\n  - Roll back all changes from this run if any step fails\n",
    );
    output
}

fn install_target_label(target: AgentTarget) -> &'static str {
    target.display_name()
}

fn native_command_shape(target: AgentTarget) -> &'static str {
    match target {
        AgentTarget::Codex => "codex mcp add repogrammar -- <repogrammar-executable> serve",
        AgentTarget::ClaudeCode => {
            "claude mcp add --scope user repogrammar -- <repogrammar-executable> serve"
        }
        AgentTarget::Cursor => "Cursor MCP JSON config preview",
        AgentTarget::Opencode => "opencode MCP JSONC config preview",
        AgentTarget::Hermes => "Hermes YAML config preview",
        AgentTarget::Gemini => "Gemini MCP JSON config preview",
        AgentTarget::Antigravity => "Antigravity MCP JSON config preview",
        AgentTarget::Kiro => "Kiro MCP JSON config preview",
        AgentTarget::AllSupported => "all supported agent MCP command shapes",
        AgentTarget::None => "no agent MCP command shape",
    }
}

fn agent_cli_detected<F>(target: AgentTarget, env_lookup: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let Some(binary) = target.detection_binary() else {
        return false;
    };
    path_entries(env_lookup)
        .into_iter()
        .map(|entry| entry.join(binary))
        .any(|candidate| candidate.is_file())
}

fn install_dry_run_native_plan(request: &InstallRequest) -> Vec<String> {
    let targets = targets_for_display(request);
    if targets.is_empty() {
        return vec!["native_mcp: no agent targets selected".to_string()];
    }
    targets
        .into_iter()
        .map(|target| target_plan_line(target, request.scope))
        .collect()
}

fn install_print_config_output(request: &InstallRequest) -> Result<String, String> {
    let targets = if let Some(target) = request.print_config_target {
        vec![target]
    } else {
        targets_for_display(request)
    };
    if targets.is_empty() {
        return Ok("config preview: no agent targets selected\n".to_string());
    }
    let mut output = String::new();
    for target in targets {
        output.push_str(&format!(
            "config preview: target={} scope={}\n",
            target.as_str(),
            request.scope.as_str()
        ));
        output.push_str(&target_config_snippet(target, request.scope)?);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

fn parse_default_no_prompt_response(response: &str) -> Result<bool, String> {
    match response.trim().to_ascii_lowercase().as_str() {
        "" | "n" | "no" => Ok(false),
        "y" | "yes" => Ok(true),
        _ => Err("prompt requires y/yes or n/no".to_string()),
    }
}

fn install_execution_context<F>(
    current_dir: &Path,
    env_lookup: &F,
) -> Result<InstallExecutionContext, String>
where
    F: Fn(&str) -> Option<String>,
{
    let executable_path = match env_lookup("REPOGRAMMAR_EXECUTABLE") {
        Some(path) => path,
        None => std::env::current_exe()
            .map_err(|error| format!("failed to resolve current executable: {error}"))?
            .display()
            .to_string(),
    };
    let data_dir = match env_lookup("REPOGRAMMAR_INSTALL_DIR") {
        Some(path) => path,
        None => default_install_data_dir(env_lookup)?,
    };
    let (command_dir, command_dir_on_path) = default_install_command_dir(&data_dir, env_lookup)?;
    Ok(InstallExecutionContext {
        executable_path,
        command_dir,
        command_dir_on_path,
        data_dir,
        current_dir: current_dir.display().to_string(),
    })
}

fn apply_install_telemetry_preference<F>(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    env_lookup: &F,
) -> Result<TelemetryStatusReport, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    let paths = TelemetryPaths {
        global_data_dir: PathBuf::from(&context.data_dir),
        repository_root: PathBuf::from(&context.current_dir),
        state_dir_override: state_dir_override(env_lookup),
    };
    set_anonymous_telemetry(&paths, request.telemetry_enabled)?;
    telemetry_status(&paths, None, env_lookup)
}

fn default_install_data_dir<F>(env_lookup: &F) -> Result<String, String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(value) = env_lookup("XDG_DATA_HOME").filter(|value| !value.trim().is_empty()) {
        return Ok(Path::new(&value).join("repogrammar").display().to_string());
    }
    let home = env_lookup("HOME")
        .or_else(|| env_lookup("USERPROFILE"))
        .ok_or_else(|| "HOME is required for live install/uninstall writes".to_string())?;
    Ok(Path::new(&home)
        .join(".local")
        .join("share")
        .join("repogrammar")
        .display()
        .to_string())
}

fn default_install_command_dir<F>(data_dir: &str, env_lookup: &F) -> Result<(String, bool), String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(value) =
        env_lookup("REPOGRAMMAR_COMMAND_DIR").filter(|value| !value.trim().is_empty())
    {
        let path = Path::new(&value);
        if !path.is_absolute() {
            return Err("REPOGRAMMAR_COMMAND_DIR must be absolute".to_string());
        }
        return Ok((
            path.display().to_string(),
            path_is_on_env_path(path, env_lookup),
        ));
    }
    if let Some(path) = path_entries(env_lookup)
        .into_iter()
        .find(|path| path.is_absolute() && path.is_dir() && !path_is_readonly(path))
    {
        return Ok((path.display().to_string(), true));
    }
    let path = env_lookup("HOME")
        .or_else(|| env_lookup("USERPROFILE"))
        .map(|home| Path::new(&home).join(".local").join("bin"))
        .unwrap_or_else(|| Path::new(data_dir).join("bin"));
    Ok((
        path.display().to_string(),
        path_is_on_env_path(&path, env_lookup),
    ))
}

fn path_entries<F>(env_lookup: &F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let separator = if cfg!(windows) { ';' } else { ':' };
    env_lookup("PATH")
        .unwrap_or_default()
        .split(separator)
        .filter(|entry| !entry.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn path_is_on_env_path<F>(path: &Path, env_lookup: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    path_entries(env_lookup)
        .into_iter()
        .any(|entry| entry == path)
}

fn path_is_readonly(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(true)
}

fn install_outcome_human(outcome: &InstallExecutionOutcome) -> String {
    let targets = outcome
        .configured_targets
        .iter()
        .map(|target| target.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let skipped = outcome
        .skipped_targets
        .iter()
        .map(|target| target.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let mut output = format!(
        "{}: {}\ntarget={}\nscope={}\nconfigured_targets={}\nreceipts={}\n",
        outcome.command,
        outcome.message,
        outcome.target.as_str(),
        outcome.scope.as_str(),
        if targets.is_empty() { "none" } else { &targets },
        outcome.receipt_paths.len()
    );
    if !skipped.is_empty() {
        output.push_str(&format!("skipped_targets={skipped}\n"));
    }
    if let Some(path) = &outcome.command_path {
        output.push_str(&format!("command_path={path}\n"));
        output.push_str(&format!(
            "command_on_path={}\n",
            if outcome.command_on_path { "yes" } else { "no" }
        ));
    }
    output
}

fn install_telemetry_human(report: &TelemetryStatusReport) -> String {
    format!(
        "telemetry={}\neffective_telemetry={}\ndisabled_by_environment={}\nexperiments=separate opt-in; controlled-pair may increase token usage, time, and provider cost\n",
        if report.enabled { "on" } else { "off" },
        if report.effective_enabled { "on" } else { "off" },
        report.disabled_by_environment
    )
}

fn handle_stats<F>(
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_stats_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };
    let status_report = match runtime.repository_status(request.clone()) {
        Ok(report) => report,
        Err(_) => {
            let fallback = repository_status_unavailable_fallback(
                QueryPreflightOperation::ActiveIndexInventory,
            );
            return query_fallback(
                "stats",
                options.json,
                fallback.reason,
                fallback.guidance,
                fallback.implemented,
            );
        }
    };

    match query_preflight(
        QueryPreflightOperation::ActiveIndexInventory,
        &status_report,
    ) {
        QueryPreflightReport::Fallback(fallback) => {
            return query_fallback(
                "stats",
                options.json,
                fallback.reason,
                fallback.guidance,
                fallback.implemented,
            );
        }
        QueryPreflightReport::Ready => {}
    }

    if options.json {
        return match runtime.repo_shape_diagnostics(request) {
            Ok(report) => {
                let measurement = telemetry_global_data_dir(env_lookup)
                    .ok()
                    .and_then(|dir| latest_comparable_experiment_report(&dir).ok().flatten());
                record_stats_telemetry_rollup(
                    current_dir,
                    env_lookup,
                    options.project_path.as_deref(),
                    &report,
                    measurement.as_ref(),
                );
                CliOutput::success(stats_json(&report, measurement.as_ref()))
            }
            Err(_) => query_fallback(
                "stats",
                true,
                "repository status is unavailable",
                "run repogrammar doctor",
                true,
            ),
        };
    }

    match runtime.repo_shape_diagnostics(request) {
        Ok(report) => CliOutput::success(stats_human(&report)),
        Err(_) => query_fallback(
            "stats",
            false,
            "repository status is unavailable",
            "run repogrammar doctor",
            true,
        ),
    }
}

fn record_stats_telemetry_rollup<F>(
    current_dir: &Path,
    env_lookup: &F,
    project_path: Option<&str>,
    report: &RepoShapeDiagnosticsReport,
    measurement: Option<&crate::application::telemetry::ExperimentReport>,
) where
    F: Fn(&str) -> Option<String>,
{
    let Ok(paths) = telemetry_paths(current_dir, env_lookup, project_path) else {
        return;
    };
    let diagnostics = telemetry_diagnostics_from_report(report);
    let _ = record_passive_diagnostics_rollup(
        &paths,
        env!("CARGO_PKG_VERSION"),
        Some(diagnostics),
        measurement,
        env_lookup,
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct StatsOptions {
    json: bool,
    project_path: Option<String>,
}

fn parse_stats_options(rest: &[String]) -> Result<StatsOptions, String> {
    let mut options = StatsOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" => {
                let value = option_value(rest, index, "--project", "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" | "--verbose" => {
                index += 1;
            }
            other => return Err(format!("unknown stats option: {other}")),
        }
    }
    Ok(options)
}

fn stats_human(report: &RepoShapeDiagnosticsReport) -> String {
    format!(
        "stats: repo-shape diagnostics\nactive_generation: {}\neligible_code_units: {}\nfamily_count: {}\nfamily_member_count: {}\ncovered_code_units: {}\nlocal_pattern_density: {}\nfamily_support_coverage: {}\nabstention_rate: {}\nexternal_dependency_signal: {}\nthin_wrapper_risk: {}\ntoken_saving_risk: {}\ninterpretation: {}\n",
        report.active_generation,
        report.eligible_code_units,
        report.family_count,
        report.family_member_count,
        report.covered_code_units,
        optional_ratio_human(report.local_pattern_density),
        optional_ratio_human(report.family_support_coverage),
        optional_ratio_human(report.abstention_rate),
        report.external_dependency_signal.as_str(),
        report.thin_wrapper_risk.as_str(),
        report.token_saving_risk.as_str(),
        report.interpretation
    )
}

fn stats_json(
    report: &RepoShapeDiagnosticsReport,
    measurement: Option<&crate::application::telemetry::ExperimentReport>,
) -> String {
    let measurement_status = if measurement
        .and_then(|measurement| measurement.token_savings)
        .is_some()
    {
        "paired_measurement_available"
    } else {
        "no_paired_measurement"
    };
    json_line(json!({
        "command": "stats",
        "status": "ok",
        "implemented": true,
        "active_generation": report.active_generation,
        "metrics": {
            "local_pattern_density": report.local_pattern_density,
            "family_support_coverage": report.family_support_coverage,
            "abstention_rate": report.abstention_rate,
            "external_dependency_signal": diagnostic_signal_json(report.external_dependency_signal),
            "thin_wrapper_risk": diagnostic_signal_json(report.thin_wrapper_risk),
            "token_saving_risk": diagnostic_signal_json(report.token_saving_risk),
        },
        "counts": {
            "eligible_code_units": report.eligible_code_units,
            "family_count": report.family_count,
            "family_member_count": report.family_member_count,
            "covered_code_units": report.covered_code_units,
        },
        "metric_kinds": ["MEASURED", "DERIVED", "ESTIMATED", "CAUSAL_EXPERIMENT"],
        "token_savings": measurement.and_then(|measurement| measurement.token_savings),
        "token_savings_ratio": measurement.and_then(|measurement| measurement.token_savings_ratio),
        "measurement_source": measurement.and_then(|measurement| measurement.measurement_source.as_deref()),
        "measurement_status": measurement_status,
        "measurement_reason": measurement.and_then(|measurement| measurement.reason.as_deref()),
        "claim_validity": measurement.map(|measurement| measurement.claim_validity.as_str()).unwrap_or("unknown"),
        "context_compression_ratio": null,
        "interpretation": report.interpretation,
        "claim": "diagnostic only; token saving depends on repeated repo-local patterns and is not measured token savings",
    }))
}

fn diagnostic_signal_json(signal: DiagnosticSignal) -> serde_json::Value {
    match signal {
        DiagnosticSignal::Unknown => serde_json::Value::Null,
        _ => json!(signal.as_str()),
    }
}

fn optional_ratio_human(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn handle_telemetry<F>(
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
    install_prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let (command, command_rest) = match rest.split_first() {
        Some((command, rest)) => (command.as_str(), rest),
        None => ("status", &[][..]),
    };
    match command {
        "status" => {
            let options = match parse_telemetry_options(command_rest) {
                Ok(options) => options,
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            };
            let paths =
                match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
                    Ok(paths) => paths,
                    Err(error) => return telemetry_error("status", options.json, error),
                };
            let endpoint = options
                .endpoint
                .clone()
                .or_else(|| env_lookup("REPOGRAMMAR_TELEMETRY_ENDPOINT"));
            match telemetry_status(&paths, endpoint.as_deref(), env_lookup) {
                Ok(report) if options.json => CliOutput::success(telemetry_status_json(&report)),
                Ok(report) => CliOutput::success(telemetry_status_human(&report)),
                Err(error) => telemetry_error("status", options.json, error),
            }
        }
        "on" | "off" => {
            let options = match parse_telemetry_options(command_rest) {
                Ok(options) => options,
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            };
            let paths =
                match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
                    Ok(paths) => paths,
                    Err(error) => return telemetry_error(command, options.json, error),
                };
            match set_anonymous_telemetry(&paths, command == "on")
                .and_then(|_| telemetry_status(&paths, None, env_lookup))
            {
                Ok(report) if options.json => CliOutput::success(telemetry_status_json(&report)),
                Ok(report) => CliOutput::success(telemetry_status_human(&report)),
                Err(error) => telemetry_error(command, options.json, error),
            }
        }
        "export" => {
            let options = match parse_telemetry_options(command_rest) {
                Ok(options) => options,
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            };
            let paths =
                match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
                    Ok(paths) => paths,
                    Err(error) => return telemetry_error("export", options.json, error),
                };
            let diagnostics = telemetry_diagnostics(
                current_dir,
                env_lookup,
                runtime,
                options.project_path.as_deref(),
            );
            let measurement = telemetry_global_data_dir(env_lookup)
                .ok()
                .and_then(|dir| latest_comparable_experiment_report(&dir).ok().flatten());
            match export_anonymous_telemetry(
                &paths,
                env!("CARGO_PKG_VERSION"),
                diagnostics,
                measurement.as_ref(),
            ) {
                Ok(report) if options.json => CliOutput::success(telemetry_export_json(&report)),
                Ok(report) => CliOutput::success(format!(
                    "telemetry export: payload_bytes={}\nsource_snippets_returned=false\n",
                    report.payload_bytes
                )),
                Err(error) => telemetry_error("export", options.json, error),
            }
        }
        "upload" => {
            let options = match parse_telemetry_options(command_rest) {
                Ok(options) => options,
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            };
            if !options.dry_run && !options.yes {
                return telemetry_error(
                    "upload",
                    options.json,
                    RepoGrammarError::InvalidInput(
                        "telemetry upload requires --yes unless --dry-run is set".to_string(),
                    ),
                );
            }
            let paths =
                match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
                    Ok(paths) => paths,
                    Err(error) => return telemetry_error("upload", options.json, error),
                };
            let endpoint = options
                .endpoint
                .clone()
                .or_else(|| env_lookup("REPOGRAMMAR_TELEMETRY_ENDPOINT"));
            let diagnostics = telemetry_diagnostics(
                current_dir,
                env_lookup,
                runtime,
                options.project_path.as_deref(),
            );
            let measurement = telemetry_global_data_dir(env_lookup)
                .ok()
                .and_then(|dir| latest_comparable_experiment_report(&dir).ok().flatten());
            let transport = RuntimeTelemetryTransport { runtime };
            match upload_anonymous_telemetry(
                &paths,
                TelemetryUploadRequest {
                    endpoint,
                    dry_run: options.dry_run,
                },
                env!("CARGO_PKG_VERSION"),
                diagnostics,
                measurement.as_ref(),
                env_lookup,
                &transport,
            ) {
                Ok(report) if options.json => {
                    let status = if telemetry_upload_should_fail(&report) {
                        2
                    } else {
                        0
                    };
                    cli_output_with_status(status, telemetry_upload_json(&report), String::new())
                }
                Ok(report) => {
                    if telemetry_upload_should_fail(&report) {
                        CliOutput::failure(
                            2,
                            format!(
                                "telemetry upload: not_uploaded\nreason: {}\n",
                                report.reason.unwrap_or_else(|| "not uploaded".to_string())
                            ),
                        )
                    } else {
                        CliOutput::success(telemetry_upload_human(&report))
                    }
                }
                Err(error) => telemetry_error("upload", options.json, error),
            }
        }
        "purge" => {
            let options = match parse_telemetry_options(command_rest) {
                Ok(options) => options,
                Err(error) => return CliOutput::failure(2, format!("{error}\n")),
            };
            let paths =
                match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
                    Ok(paths) => paths,
                    Err(error) => return telemetry_error("purge", options.json, error),
                };
            match purge_telemetry(&paths, options.yes) {
                Ok(report) if options.json => {
                    CliOutput::success(telemetry_purge_json("purge", &report))
                }
                Ok(report) => CliOutput::success(telemetry_purge_human("telemetry purge", &report)),
                Err(error) => telemetry_error("purge", options.json, error),
            }
        }
        "research-status" => {
            handle_research("research-status", command_rest, current_dir, env_lookup)
        }
        "research-on" => handle_research("research-on", command_rest, current_dir, env_lookup),
        "research-off" => handle_research("research-off", command_rest, current_dir, env_lookup),
        "research-export" => {
            handle_research("research-export", command_rest, current_dir, env_lookup)
        }
        "research-purge" => {
            handle_research("research-purge", command_rest, current_dir, env_lookup)
        }
        "experiment-start" => handle_experiment_start(command_rest, env_lookup, install_prompt),
        "experiment-record" => handle_experiment_record(command_rest, env_lookup),
        "experiment-stop" => handle_experiment_stop(command_rest, env_lookup),
        "experiment-report" => handle_experiment_report(command_rest, env_lookup),
        "experiment-export" => handle_experiment_export(command_rest, env_lookup),
        "experiment-purge" => handle_experiment_purge(command_rest, env_lookup),
        unknown => CliOutput::failure(2, format!("unknown telemetry command: {unknown}\n")),
    }
}

struct RuntimeTelemetryTransport<'a, R> {
    runtime: &'a R,
}

impl<R: CliRuntime> TelemetryUploadTransport for RuntimeTelemetryTransport<'_, R> {
    fn upload(
        &self,
        endpoint: &str,
        payload: &str,
        timeout: std::time::Duration,
    ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
        self.runtime
            .upload_telemetry_payload(endpoint, payload, timeout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TelemetryCliOptions {
    json: bool,
    project_path: Option<String>,
    yes: bool,
    dry_run: bool,
    endpoint: Option<String>,
}

fn parse_telemetry_options(rest: &[String]) -> Result<TelemetryCliOptions, String> {
    let mut options = TelemetryCliOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--project" => {
                let value = option_value(rest, index, "--project", "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--yes" => {
                options.yes = true;
                index += 1;
            }
            "--dry-run" => {
                options.dry_run = true;
                index += 1;
            }
            "--endpoint" => {
                let value = option_value(rest, index, "--endpoint", "an upload endpoint")?;
                validate_telemetry_endpoint(value).map_err(|error| error.to_string())?;
                options.endpoint = Some(value.to_string());
                index += 2;
            }
            "--quiet" | "--verbose" => {
                index += 1;
            }
            other => return Err(format!("unknown telemetry option: {other}")),
        }
    }
    Ok(options)
}

fn telemetry_paths<F>(
    current_dir: &Path,
    env_lookup: &F,
    project_path: Option<&str>,
) -> Result<TelemetryPaths, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    Ok(TelemetryPaths {
        global_data_dir: telemetry_global_data_dir(env_lookup)?,
        repository_root: PathBuf::from(repository_root(current_dir, project_path)),
        state_dir_override: state_dir_override(env_lookup),
    })
}

fn telemetry_global_data_dir<F>(env_lookup: &F) -> Result<PathBuf, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    default_install_data_dir(env_lookup)
        .map(PathBuf::from)
        .map_err(RepoGrammarError::InvalidInput)
}

fn telemetry_diagnostics<F>(
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
    project_path: Option<&str>,
) -> Option<TelemetryDiagnostics>
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, project_path),
        state_dir_override: state_dir_override(env_lookup),
    };
    runtime
        .repo_shape_diagnostics(request)
        .ok()
        .map(|report| telemetry_diagnostics_from_report(&report))
}

fn telemetry_diagnostics_from_report(report: &RepoShapeDiagnosticsReport) -> TelemetryDiagnostics {
    TelemetryDiagnostics {
        eligible_code_units: report.eligible_code_units,
        family_count: report.family_count,
        family_support_coverage: report.family_support_coverage,
        local_pattern_density: report.local_pattern_density,
        abstention_rate: report.abstention_rate,
        external_dependency_signal: report.external_dependency_signal,
        thin_wrapper_risk: report.thin_wrapper_risk.as_str(),
        token_saving_risk: report.token_saving_risk.as_str(),
        read_plan_item_count: 0,
    }
}

fn telemetry_status_human(report: &TelemetryStatusReport) -> String {
    format!(
        "telemetry: anonymous={}\nresearch-trace={}\neffective_anonymous={}\ndisabled_by_environment={}\ndisabled_by_ci={}\nnetwork_upload_configured={}\nupload_would_open_network_connection={}\nrollup_count={}\nqueue_count={}\nsent_receipts={}\n",
        if report.enabled { "on" } else { "off" },
        if report.research_enabled { "on" } else { "off" },
        if report.effective_enabled { "on" } else { "off" },
        report.disabled_by_environment,
        report.disabled_by_ci,
        report.network_upload_configured,
        report.upload_would_open_network_connection,
        report.rollup_count,
        report.queue_count,
        report.sent_receipt_count,
    )
}

fn telemetry_status_json(report: &TelemetryStatusReport) -> String {
    json_line(json!({
        "command": "telemetry status",
        "status": "ok",
        "schema_version": report.schema_version,
        "enabled": report.enabled,
        "research_enabled": report.research_enabled,
        "disabled_by_environment": report.disabled_by_environment,
        "disabled_by_ci": report.disabled_by_ci,
        "effective_enabled": report.effective_enabled,
        "anonymous_machine_id": report.anonymous_machine_id,
        "rollup_count": report.rollup_count,
        "queue_count": report.queue_count,
        "sent_receipt_count": report.sent_receipt_count,
        "network_upload_configured": report.network_upload_configured,
        "upload_would_open_network_connection": report.upload_would_open_network_connection,
        "updated_at": report.updated_at,
    }))
}

fn telemetry_export_json(report: &TelemetryExportReport) -> String {
    json_line(json!({
        "command": "telemetry export",
        "status": "ok",
        "payload_bytes": report.payload_bytes,
        "queued": report.queued,
        "payload": report.payload,
    }))
}

fn telemetry_upload_should_fail(report: &TelemetryUploadReport) -> bool {
    !report.uploaded
        && report
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("disabled"))
}

fn telemetry_upload_json(report: &TelemetryUploadReport) -> String {
    json_line(json!({
        "command": "telemetry upload",
        "status": if report.uploaded { "uploaded" } else { "not_uploaded" },
        "uploaded": report.uploaded,
        "dry_run": report.dry_run,
        "network_upload_configured": report.network_upload_configured,
        "reason": report.reason,
        "payload": report.payload,
        "receipt": report.receipt.as_ref().map(|receipt| json!({
            "status_code": receipt.status_code,
            "receipt_id": receipt.receipt_id,
        })),
    }))
}

fn telemetry_upload_human(report: &TelemetryUploadReport) -> String {
    if report.uploaded {
        let receipt = report
            .receipt
            .as_ref()
            .map(|receipt| receipt.receipt_id.as_str())
            .unwrap_or("none");
        format!("telemetry upload: uploaded\nreceipt: {receipt}\n")
    } else {
        format!(
            "telemetry upload: not_uploaded\nreason: {}\n",
            report.reason.as_deref().unwrap_or("not uploaded")
        )
    }
}

fn telemetry_purge_json(command: &str, report: &TelemetryPurgeReport) -> String {
    json_line(json!({
        "command": format!("telemetry {command}"),
        "status": "ok",
        "removed_files": report.removed_files,
        "removed_directories": report.removed_directories,
    }))
}

fn telemetry_purge_human(command: &str, report: &TelemetryPurgeReport) -> String {
    format!(
        "{command}: removed_files={} removed_directories={}\n",
        report.removed_files, report.removed_directories
    )
}

fn telemetry_error(command: &str, json: bool, error: RepoGrammarError) -> CliOutput {
    if json {
        CliOutput::failure(
            2,
            json_line(json!({
                "command": format!("telemetry {command}"),
                "status": "error",
                "reason": error.to_string(),
            })),
        )
    } else {
        CliOutput::failure(2, format!("{error}\n"))
    }
}

fn cli_output_with_status(status: i32, stdout: String, stderr: String) -> CliOutput {
    CliOutput {
        status,
        stdout,
        stderr,
    }
}

fn handle_research<F>(
    command: &str,
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_telemetry_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let paths = match telemetry_paths(current_dir, env_lookup, options.project_path.as_deref()) {
        Ok(paths) => paths,
        Err(error) => return telemetry_error(command, options.json, error),
    };
    match command {
        "research-status" => match telemetry_status(&paths, None, env_lookup) {
            Ok(report) if options.json => CliOutput::success(telemetry_status_json(&report)),
            Ok(report) => CliOutput::success(telemetry_status_human(&report)),
            Err(error) => telemetry_error(command, options.json, error),
        },
        "research-on" | "research-off" => {
            match set_research_trace(&paths, command == "research-on")
                .and_then(|_| telemetry_status(&paths, None, env_lookup))
            {
                Ok(report) if options.json => CliOutput::success(telemetry_status_json(&report)),
                Ok(report) => CliOutput::success(telemetry_status_human(&report)),
                Err(error) => telemetry_error(command, options.json, error),
            }
        }
        "research-export" => match research_export(&paths) {
            Ok(value) if options.json => CliOutput::success(json_line(json!({
                "command": "telemetry research-export",
                "status": "ok",
                "payload": value,
            }))),
            Ok(value) => CliOutput::success(format!(
                "research export: redacted_metadata_only\nsource_snippets_returned={}\n",
                value["source_snippets_included"].as_bool().unwrap_or(false)
            )),
            Err(error) => telemetry_error(command, options.json, error),
        },
        "research-purge" => match research_purge(&paths, options.yes) {
            Ok(report) if options.json => {
                CliOutput::success(telemetry_purge_json(command, &report))
            }
            Ok(report) => CliOutput::success(telemetry_purge_human(command, &report)),
            Err(error) => telemetry_error(command, options.json, error),
        },
        _ => CliOutput::failure(2, format!("unknown telemetry command: {command}\n")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExperimentStartOptions {
    json: bool,
    yes: bool,
    name: Option<String>,
    experiment_mode: Option<ExperimentWorkflowMode>,
    mode: Option<ExperimentMode>,
    measurement_source: Option<MeasurementSource>,
    coarse_task_kind: Option<String>,
    elapsed_time_bucket: Option<String>,
    read_plan_used: Option<bool>,
    read_plan_item_count_bucket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExperimentRecordOptions {
    json: bool,
    name: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    tool_tokens: Option<u64>,
    success: Option<bool>,
    test_outcome: TestOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExperimentNameOptions {
    json: bool,
    name: Option<String>,
    yes: bool,
}

fn handle_experiment_start<F>(
    rest: &[String],
    env_lookup: &F,
    install_prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_start_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    if !options.yes {
        match prompt_experiment_start_consent(&options, install_prompt) {
            Ok(true) => {}
            Ok(false) => {
                return experiment_error(
                    "experiment-start",
                    options.json,
                    RepoGrammarError::InvalidInput(
                        "experiment recording requires explicit confirmation".to_string(),
                    ),
                );
            }
            Err(error) => return CliOutput::failure(2, format!("{error}\n")),
        }
    }
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-start", options.json, error),
    };
    let request = match experiment_start_request(options.clone()) {
        Ok(request) => request,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    match experiment_start(&data_dir, request) {
        Ok(report) if options.json => {
            CliOutput::success(experiment_cli_json("experiment-start", &report))
        }
        Ok(report) => CliOutput::success(experiment_human("experiment-start", &report)),
        Err(error) => experiment_error("experiment-start", options.json, error),
    }
}

fn prompt_experiment_start_consent(
    options: &ExperimentStartOptions,
    install_prompt: &impl InstallTelemetryPrompt,
) -> Result<bool, String> {
    let Some(mode) = options.experiment_mode else {
        return Ok(false);
    };
    let prompt = match mode {
        ExperimentWorkflowMode::RecordExisting => RECORD_EXISTING_EXPERIMENT_PROMPT,
        ExperimentWorkflowMode::ControlledPair => CONTROLLED_PAIR_EXPERIMENT_PROMPT,
    };
    let response = install_prompt.prompt_experiment_consent(prompt)?;
    parse_default_no_prompt_response(&response)
}

fn handle_experiment_record<F>(rest: &[String], env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_record_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-record", options.json, error),
    };
    let request = match experiment_record_request(options.clone()) {
        Ok(request) => request,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    match experiment_record(&data_dir, request) {
        Ok(report) if options.json => {
            CliOutput::success(experiment_cli_json("experiment-record", &report))
        }
        Ok(report) => CliOutput::success(experiment_human("experiment-record", &report)),
        Err(error) => experiment_error("experiment-record", options.json, error),
    }
}

fn handle_experiment_stop<F>(rest: &[String], env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_name_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-stop", options.json, error),
    };
    let Some(name) = options.name.as_deref() else {
        return CliOutput::failure(2, "--name is required\n");
    };
    match experiment_stop(&data_dir, name) {
        Ok(report) if options.json => {
            CliOutput::success(experiment_cli_json("experiment-stop", &report))
        }
        Ok(report) => CliOutput::success(experiment_human("experiment-stop", &report)),
        Err(error) => experiment_error("experiment-stop", options.json, error),
    }
}

fn handle_experiment_report<F>(rest: &[String], env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_name_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-report", options.json, error),
    };
    let Some(name) = options.name.as_deref() else {
        return CliOutput::failure(2, "--name is required\n");
    };
    match experiment_report(&data_dir, name) {
        Ok(report) if options.json => {
            CliOutput::success(experiment_cli_json("experiment-report", &report))
        }
        Ok(report) => CliOutput::success(experiment_human("experiment-report", &report)),
        Err(error) => experiment_error("experiment-report", options.json, error),
    }
}

fn handle_experiment_export<F>(rest: &[String], env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_name_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-export", options.json, error),
    };
    let Some(name) = options.name.as_deref() else {
        return CliOutput::failure(2, "--name is required\n");
    };
    match experiment_export(&data_dir, name) {
        Ok(value) if options.json => CliOutput::success(json_line(json!({
            "command": "telemetry experiment-export",
            "status": "ok",
            "payload": value,
        }))),
        Ok(value) => CliOutput::success(format!(
            "experiment export: name={}\nsessions={}\n",
            value["name"].as_str().unwrap_or("unknown"),
            value["sessions"].as_array().map(Vec::len).unwrap_or(0)
        )),
        Err(error) => experiment_error("experiment-export", options.json, error),
    }
}

fn handle_experiment_purge<F>(rest: &[String], env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_experiment_name_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let data_dir = match telemetry_global_data_dir(env_lookup) {
        Ok(data_dir) => data_dir,
        Err(error) => return experiment_error("experiment-purge", options.json, error),
    };
    let Some(name) = options.name.as_deref() else {
        return CliOutput::failure(2, "--name is required\n");
    };
    match experiment_purge(&data_dir, name, options.yes) {
        Ok(report) if options.json => {
            CliOutput::success(telemetry_purge_json("experiment-purge", &report))
        }
        Ok(report) => CliOutput::success(telemetry_purge_human("experiment purge", &report)),
        Err(error) => experiment_error("experiment-purge", options.json, error),
    }
}

fn parse_experiment_start_options(rest: &[String]) -> Result<ExperimentStartOptions, String> {
    let mut options = ExperimentStartOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--yes" => {
                options.yes = true;
                index += 1;
            }
            "--name" => {
                options.name =
                    Some(option_value(rest, index, "--name", "an experiment name")?.to_string());
                index += 2;
            }
            "--experiment-mode" | "--workflow-mode" => {
                options.experiment_mode = Some(ExperimentWorkflowMode::parse(option_value(
                    rest,
                    index,
                    "--experiment-mode",
                    "record_existing or controlled_pair",
                )?)?);
                index += 2;
            }
            "--session" => {
                options.mode = Some(ExperimentMode::parse(option_value(
                    rest,
                    index,
                    "--session",
                    "baseline or treatment",
                )?)?);
                index += 2;
            }
            "--mode" => {
                options.mode = Some(ExperimentMode::parse(option_value(
                    rest,
                    index,
                    "--mode",
                    "baseline or treatment",
                )?)?);
                index += 2;
            }
            "--measurement-source" => {
                options.measurement_source = Some(MeasurementSource::parse(option_value(
                    rest,
                    index,
                    "--measurement-source",
                    "a measurement source",
                )?)?);
                index += 2;
            }
            "--task-kind" => {
                options.coarse_task_kind = Some(
                    option_value(rest, index, "--task-kind", "a coarse task kind")?.to_string(),
                );
                index += 2;
            }
            "--elapsed-time-bucket" => {
                options.elapsed_time_bucket = Some(
                    option_value(rest, index, "--elapsed-time-bucket", "a bucket")?.to_string(),
                );
                index += 2;
            }
            "--read-plan-used" => {
                options.read_plan_used = Some(parse_bool(option_value(
                    rest,
                    index,
                    "--read-plan-used",
                    "true or false",
                )?)?);
                index += 2;
            }
            "--read-plan-item-count" => {
                let value = parse_positive_usize(
                    option_value(rest, index, "--read-plan-item-count", "a count")?,
                    "--read-plan-item-count",
                )?;
                options.read_plan_item_count_bucket =
                    Some(read_plan_count_bucket(value).to_string());
                index += 2;
            }
            other => return Err(format!("unknown experiment-start option: {other}")),
        }
    }
    Ok(options)
}

fn parse_experiment_record_options(rest: &[String]) -> Result<ExperimentRecordOptions, String> {
    let mut options = ExperimentRecordOptions {
        test_outcome: TestOutcome::Unknown,
        ..ExperimentRecordOptions::default()
    };
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--name" => {
                options.name =
                    Some(option_value(rest, index, "--name", "an experiment name")?.to_string());
                index += 2;
            }
            "--input-tokens" => {
                options.input_tokens = Some(parse_u64(
                    option_value(rest, index, "--input-tokens", "a token count")?,
                    "--input-tokens",
                )?);
                index += 2;
            }
            "--output-tokens" => {
                options.output_tokens = Some(parse_u64(
                    option_value(rest, index, "--output-tokens", "a token count")?,
                    "--output-tokens",
                )?);
                index += 2;
            }
            "--tool-tokens" => {
                options.tool_tokens = Some(parse_u64(
                    option_value(rest, index, "--tool-tokens", "a token count")?,
                    "--tool-tokens",
                )?);
                index += 2;
            }
            "--success" => {
                options.success = Some(parse_bool(option_value(
                    rest,
                    index,
                    "--success",
                    "true or false",
                )?)?);
                index += 2;
            }
            "--test-outcome" => {
                options.test_outcome = TestOutcome::parse(option_value(
                    rest,
                    index,
                    "--test-outcome",
                    "passed, failed, not_run, or unknown",
                )?)?;
                index += 2;
            }
            other => return Err(format!("unknown experiment-record option: {other}")),
        }
    }
    Ok(options)
}

fn parse_experiment_name_options(rest: &[String]) -> Result<ExperimentNameOptions, String> {
    let mut options = ExperimentNameOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--yes" => {
                options.yes = true;
                index += 1;
            }
            "--name" => {
                options.name =
                    Some(option_value(rest, index, "--name", "an experiment name")?.to_string());
                index += 2;
            }
            other => return Err(format!("unknown experiment option: {other}")),
        }
    }
    Ok(options)
}

fn experiment_start_request(
    options: ExperimentStartOptions,
) -> Result<ExperimentStartRequest, String> {
    Ok(ExperimentStartRequest {
        name: options
            .name
            .ok_or_else(|| "--name is required".to_string())?,
        experiment_mode: options
            .experiment_mode
            .ok_or_else(|| "--experiment-mode is required".to_string())?,
        mode: options
            .mode
            .ok_or_else(|| "--session is required".to_string())?,
        measurement_source: options
            .measurement_source
            .ok_or_else(|| "--measurement-source is required".to_string())?,
        coarse_task_kind: options.coarse_task_kind,
        elapsed_time_bucket: options.elapsed_time_bucket,
        read_plan_used: options.read_plan_used,
        read_plan_item_count_bucket: options.read_plan_item_count_bucket,
    })
}

fn experiment_record_request(
    options: ExperimentRecordOptions,
) -> Result<ExperimentRecordRequest, String> {
    Ok(ExperimentRecordRequest {
        name: options
            .name
            .ok_or_else(|| "--name is required".to_string())?,
        input_tokens: options
            .input_tokens
            .ok_or_else(|| "--input-tokens is required".to_string())?,
        output_tokens: options
            .output_tokens
            .ok_or_else(|| "--output-tokens is required".to_string())?,
        tool_tokens: options
            .tool_tokens
            .ok_or_else(|| "--tool-tokens is required".to_string())?,
        success: options
            .success
            .ok_or_else(|| "--success is required".to_string())?,
        test_outcome: options.test_outcome,
    })
}

fn experiment_cli_json(
    command: &str,
    report: &crate::application::telemetry::ExperimentReport,
) -> String {
    let mut value = experiment_report_json(report);
    if let Some(object) = value.as_object_mut() {
        object.insert("command".to_string(), json!(format!("telemetry {command}")));
        object.insert("status".to_string(), json!("ok"));
    }
    json_line(value)
}

fn experiment_human(
    command: &str,
    report: &crate::application::telemetry::ExperimentReport,
) -> String {
    format!(
        "{command}: {}\nbaseline_total_tokens: {}\ntreatment_total_tokens: {}\ntoken_savings: {}\nmeasurement_source: {}\nreason: {}\n",
        report.name,
        optional_u64_human(report.baseline_total_tokens),
        optional_u64_human(report.treatment_total_tokens),
        report
            .token_savings
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        report.measurement_source.as_deref().unwrap_or("null"),
        report.reason.as_deref().unwrap_or("none")
    )
}

fn experiment_error(command: &str, json: bool, error: RepoGrammarError) -> CliOutput {
    if json {
        CliOutput::failure(
            2,
            json_line(json!({
                "command": format!("telemetry {command}"),
                "status": "error",
                "reason": error.to_string(),
            })),
        )
    } else {
        CliOutput::failure(2, format!("{error}\n"))
    }
}

fn optional_u64_human(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn parse_u64(value: &str, option: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{option} requires a non-negative integer"))
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("value must be true or false".to_string()),
    }
}

fn read_plan_count_bucket(value: usize) -> &'static str {
    match value {
        0 => "0",
        1..=2 => "1-2",
        3..=9 => "3-9",
        10..=49 => "10-49",
        _ => "50+",
    }
}

fn handle_init<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryLifecycleInitRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        write_root_gitignore: options.write_gitignore,
    };

    match init_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(init_outcome_json(&outcome)),
        Ok(outcome) => CliOutput::success(init_outcome_human(&outcome)),
        Err(error) => lifecycle_error("init", options.json, error),
    }
}

fn handle_uninit<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryUninitRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        yes: options.yes,
    };

    match uninit_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(uninit_outcome_json(&outcome)),
        Ok(outcome) => CliOutput::success(uninit_outcome_human(&outcome)),
        Err(error) => lifecycle_error("uninit", options.json, error),
    }
}

fn handle_status<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryStatusRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };

    match runtime.repository_status(request) {
        Ok(report) if options.json => CliOutput::success(status_json(&report)),
        Ok(report) => CliOutput::success(status_human(&report)),
        Err(error) => lifecycle_error("status", options.json, error),
    }
}

fn handle_doctor<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryDoctorRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
    };

    match runtime.repository_doctor(request) {
        Ok(report) if options.json => CliOutput::success(doctor_json(&report)),
        Ok(report) => CliOutput::success(doctor_human(&report)),
        Err(error) => lifecycle_error("doctor", options.json, error),
    }
}

fn handle_index<F>(
    command: &str,
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let semantic_worker_executable =
        env_lookup("REPOGRAMMAR_TYPESCRIPT_WORKER").filter(|value| !value.trim().is_empty());
    let semantic_worker_args = match semantic_worker_args(env_lookup) {
        Ok(args) => args,
        Err(error) => {
            return lifecycle_error(command, options.json, RepoGrammarError::InvalidInput(error));
        }
    };
    if semantic_worker_executable.is_none() && !semantic_worker_args.is_empty() {
        return lifecycle_error(
            command,
            options.json,
            RepoGrammarError::InvalidInput(
                "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON requires REPOGRAMMAR_TYPESCRIPT_WORKER"
                    .to_string(),
            ),
        );
    }
    let request = CliIndexRequest {
        repository_root: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        max_file_bytes: crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
        strict_gitignore: env_flag_enabled(env_lookup, "REPOGRAMMAR_STRICT_GITIGNORE"),
        semantic_worker_executable,
        semantic_worker_args,
        progress: options.progress,
        json: options.json,
        quiet: options.quiet,
        stderr_is_terminal: std::io::stderr().is_terminal(),
    };

    match runtime.index_repository(command, request) {
        Ok(outcome) if options.json => {
            CliOutput::success(index_outcome_json(command, &outcome, options))
        }
        Ok(outcome) => CliOutput::success(index_outcome_human(command, &outcome, options)),
        Err(RepoGrammarError::NotImplemented(_)) => handle_deferred_long_running(command, options),
        Err(error) => lifecycle_error(command, options.json, error),
    }
}

fn semantic_worker_args<F>(env_lookup: &F) -> Result<Vec<String>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(raw_args) = env_lookup("REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON") else {
        return Ok(Vec::new());
    };
    if raw_args.trim().is_empty() {
        return Ok(Vec::new());
    }
    let args: Vec<String> = serde_json::from_str(&raw_args).map_err(|_| {
        "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON must be a JSON array of strings".to_string()
    })?;
    if args.len() > 64 {
        return Err(
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON must contain at most 64 arguments".to_string(),
        );
    }
    for arg in &args {
        if arg.trim().is_empty()
            || arg.len() > 4096
            || arg.contains('\0')
            || arg.contains('\n')
            || arg.contains('\r')
        {
            return Err(
                "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON contains an invalid argument".to_string(),
            );
        }
    }
    Ok(args)
}

fn handle_unlock<F>(options: &LifecycleOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    if options.force && !options.yes {
        return CliOutput::failure(
            2,
            "repogrammar unlock --force requires --yes after stale-lock diagnosis\n",
        );
    }

    let request = RepositoryUnlockRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        force: options.force,
        yes: options.yes,
    };

    match unlock_repository(request) {
        Ok(outcome) if options.json => CliOutput::success(unlock_json(&outcome)),
        Ok(outcome) => CliOutput::success(unlock_human(&outcome)),
        Err(error) => lifecycle_error("unlock", options.json, error),
    }
}

fn handle_logs<F>(options: &LogsOptions, current_dir: &Path, env_lookup: &F) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = RepositoryLogsRequest {
        path: repository_root(current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(env_lookup),
        component: options.component.clone(),
        tail: options.tail,
        since: options.since.clone(),
        redact: options.redact,
    };

    match repository_logs(request) {
        Ok(outcome) if options.json => CliOutput::success(logs_json(&outcome, options)),
        Ok(outcome) => CliOutput::success(logs_human(&outcome)),
        Err(error) => lifecycle_error("logs", options.json, error),
    }
}

fn handle_deferred_long_running(command: &str, options: &LifecycleOptions) -> CliOutput {
    if options.json {
        return CliOutput::failure(
            2,
            json_line(json!({
                "command": command,
                "status": "not_implemented",
                "implemented": false,
                "progress": options.progress.as_str(),
                "reason": "indexing and sync require discovery, storage, and generation validation",
            })),
        );
    }

    CliOutput::failure(
        2,
        format!(
            "repogrammar {command} is not implemented yet; progress={}, indexing and sync require discovery, storage, and generation validation\n",
            options.progress.as_str()
        ),
    )
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct QueryOptions {
    project_path: Option<String>,
    target: Option<String>,
    json: bool,
    evidence_mode: FamilyEvidenceMode,
    mode_explicit: bool,
    token_budget: Option<usize>,
    include_variations: bool,
    include_exceptions: bool,
    include_source_spans: bool,
}

impl QueryOptions {
    fn output_options(&self) -> FamilyOutputOptions {
        FamilyOutputOptions {
            evidence_mode: self.evidence_mode,
            token_budget: self.token_budget,
            include_variations: self.include_variations,
            include_exceptions: self.include_exceptions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressMode {
    Auto,
    Always,
    Never,
}

impl ProgressMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err("--progress requires auto, always, or never".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleOptions {
    project_path: Option<String>,
    json: bool,
    quiet: bool,
    verbose: bool,
    progress: ProgressMode,
    write_gitignore: bool,
    yes: bool,
    force: bool,
}

impl Default for LifecycleOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            json: false,
            quiet: false,
            verbose: false,
            progress: ProgressMode::Auto,
            write_gitignore: false,
            yes: false,
            force: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogsOptions {
    project_path: Option<String>,
    json: bool,
    quiet: bool,
    verbose: bool,
    tail: Option<usize>,
    since: Option<String>,
    component: Option<String>,
    redact: bool,
}

impl Default for LogsOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            json: false,
            quiet: false,
            verbose: false,
            tail: None,
            since: None,
            component: None,
            redact: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServeOptions {
    pub project_path: Option<String>,
    pub json: bool,
    pub quiet: bool,
    pub verbose: bool,
}

pub fn parse_serve_options(rest: &[String]) -> Result<ServeOptions, String> {
    let mut options = ServeOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--progress" => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                ProgressMode::parse(value)?;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                set_project_path(&mut options.project_path, value)?;
                index += 1;
            }
            other => return Err(format!("unknown serve option: {other}")),
        }
    }
    Ok(options)
}

fn parse_lifecycle_options(command: &str, rest: &[String]) -> Result<LifecycleOptions, String> {
    let mut options = LifecycleOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--progress" if matches!(command, "init" | "index" | "sync") => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                options.progress = ProgressMode::parse(value)?;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            "--write-gitignore" if command == "init" => {
                options.write_gitignore = true;
                index += 1;
            }
            "--yes" if matches!(command, "uninit" | "unlock") => {
                options.yes = true;
                index += 1;
            }
            "--force" if command == "unlock" => {
                options.force = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                set_project_path(&mut options.project_path, value)?;
                index += 1;
            }
            other => return Err(format!("unknown {command} option: {other}")),
        }
    }
    Ok(options)
}

fn parse_logs_options(rest: &[String]) -> Result<LogsOptions, String> {
    let mut options = LogsOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--component" => {
                let value = option_value(
                    rest,
                    index,
                    "--component",
                    "index, daemon, mcp, or telemetry",
                )?;
                validate_log_component(value)?;
                options.component = Some(value.to_string());
                index += 2;
            }
            "--since" => {
                let duration = option_value(rest, index, "--since", "a duration")?;
                options.since = Some(duration.to_string());
                index += 2;
            }
            "--tail" => {
                options.tail = Some(100);
                if let Some(value) = rest.get(index + 1).filter(|value| !value.starts_with('-')) {
                    options.tail = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| "--tail requires a non-negative integer".to_string())?,
                    );
                    index += 2;
                } else {
                    index += 1;
                }
            }
            "--redact" => {
                options.redact = true;
                index += 1;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                set_project_path(&mut options.project_path, value)?;
                index += 1;
            }
            other => return Err(format!("unknown logs option: {other}")),
        }
    }
    Ok(options)
}

fn parse_query_options(rest: &[String]) -> Result<QueryOptions, String> {
    let mut options = QueryOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" => {
                let value = option_value(rest, index, "--project", "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--token-budget" => {
                let value = option_value(rest, index, "--token-budget", "a token budget")?;
                let parsed = parse_positive_usize(value, "--token-budget")?;
                validate_query_token_budget(parsed)
                    .map_err(|error| format!("--token-budget {error}"))?;
                options.token_budget = Some(parsed);
                if !options.mode_explicit && options.evidence_mode == FamilyEvidenceMode::Compact {
                    options.evidence_mode = FamilyEvidenceMode::Evidence;
                }
                index += 2;
            }
            "--mode" => {
                let value = option_value(rest, index, "--mode", "compact, evidence, or deep")?;
                options.evidence_mode = FamilyEvidenceMode::parse(value)
                    .ok_or_else(|| "--mode requires compact, evidence, or deep".to_string())?;
                options.mode_explicit = true;
                index += 2;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--include-variations" => {
                options.include_variations = true;
                index += 1;
            }
            "--include-exceptions" => {
                options.include_exceptions = true;
                index += 1;
            }
            "--include-source-spans" => {
                options.include_source_spans = true;
                index += 1;
            }
            value if !value.starts_with('-') => {
                if options.target.is_none() {
                    validate_query_target(value).map_err(|error| format!("target {error}"))?;
                    options.target = Some(value.to_string());
                } else {
                    return Err(format!("unexpected positional argument: {value}"));
                }
                index += 1;
            }
            other => return Err(format!("unknown query option: {other}")),
        }
    }
    Ok(options)
}

fn parse_install_options(rest: &[String]) -> Result<InstallRequest, String> {
    let mut request = InstallRequest::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--target" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err("--target requires a value".to_string());
                };
                apply_install_target_value(&mut request, value)?;
                index += 2;
            }
            "--scope" | "--location" => {
                let Some(value) = rest.get(index + 1) else {
                    return Err(format!("{} requires global or project", rest[index]));
                };
                request.scope = InstallScope::parse(value)?;
                index += 2;
            }
            "--dry-run" => {
                request.dry_run = true;
                index += 1;
            }
            "--yes" => {
                request.assume_yes = true;
                index += 1;
            }
            "--print-config" => {
                request.print_config = true;
                if let Some(value) = rest.get(index + 1).filter(|value| !value.starts_with("--")) {
                    let target = AgentTarget::parse(value)?;
                    if matches!(target, AgentTarget::AllSupported | AgentTarget::None) {
                        return Err(
                            "--print-config requires a concrete target when a value is supplied"
                                .to_string(),
                        );
                    }
                    request.print_config_target = Some(target);
                    index += 2;
                } else {
                    index += 1;
                }
            }
            "--no-telemetry" => {
                request.telemetry_enabled = false;
                request.telemetry_explicitly_configured = true;
                index += 1;
            }
            "--telemetry" => {
                request.telemetry_enabled = true;
                request.telemetry_explicitly_configured = true;
                index += 1;
            }
            "--no-permissions" => {
                request.no_permissions = true;
                index += 1;
            }
            other => return Err(format!("unknown installer option: {other}")),
        }
    }
    Ok(request)
}

fn apply_install_target_value(request: &mut InstallRequest, value: &str) -> Result<(), String> {
    let tokens = value.split(',').map(str::trim).collect::<Vec<_>>();
    if tokens.iter().any(|token| token.is_empty()) {
        return Err(
            "unsupported target list; use comma-separated target ids without empty entries"
                .to_string(),
        );
    }
    if tokens.len() == 1 {
        request.target = AgentTarget::parse(tokens[0])?;
        request.selected_targets.clear();
        return Ok(());
    }
    let mut targets = Vec::new();
    for token in tokens {
        let target = AgentTarget::parse(token)?;
        if matches!(target, AgentTarget::AllSupported | AgentTarget::None) {
            return Err("target lists must contain concrete agent ids only".to_string());
        }
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    let normalized = normalize_concrete_targets(&targets).map_err(|error| error.to_string())?;
    request.target = if normalized == supported_concrete_targets() {
        AgentTarget::AllSupported
    } else {
        normalized[0]
    };
    request.selected_targets = normalized;
    Ok(())
}

fn option_value<'a>(
    rest: &'a [String],
    index: usize,
    option: &str,
    expectation: &str,
) -> Result<&'a str, String> {
    let Some(value) = rest.get(index + 1) else {
        return Err(format!("{option} requires {expectation}"));
    };
    if value.starts_with('-') {
        return Err(format!("{option} requires {expectation}"));
    }
    Ok(value)
}

fn parse_positive_usize(value: &str, option: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{option} requires a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{option} requires a positive integer"));
    }
    Ok(parsed)
}

fn set_project_path(target: &mut Option<String>, value: &str) -> Result<(), String> {
    if target.is_some() {
        return Err(format!("unexpected positional argument: {value}"));
    }
    *target = Some(value.to_string());
    Ok(())
}

fn validate_log_component(value: &str) -> Result<(), String> {
    match value {
        "index" | "daemon" | "mcp" | "telemetry" => Ok(()),
        _ => Err("--component requires index, daemon, mcp, or telemetry".to_string()),
    }
}

pub fn repository_root(current_dir: &Path, project_path: Option<&str>) -> String {
    let raw = Path::new(project_path.unwrap_or("."));
    let path = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        current_dir.join(raw)
    };
    path.display().to_string()
}

pub fn state_dir_override<F>(env_lookup: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    env_lookup("REPOGRAMMAR_DIR")
}

fn env_flag_enabled<F>(env_lookup: &F, name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env_lookup(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn init_outcome_human(outcome: &RepositoryInitOutcome) -> String {
    let mut output = format!(
        "init: repository-local state ready\nstate_dir: {}\ncreated: {}\ngit_info_exclude: {}\nroot_gitignore: {}\nstorage: not_implemented\nindexing: not_implemented\n",
        outcome.state_dir,
        outcome.created,
        if outcome.git_info_exclude_updated {
            "updated"
        } else {
            "already_present_or_not_applicable"
        },
        if outcome.root_gitignore_updated {
            "updated"
        } else {
            "not_modified"
        }
    );
    for entry in &outcome.repaired_entries {
        output.push_str("repaired_entry: ");
        output.push_str(entry);
        output.push('\n');
    }
    output
}

fn init_outcome_json(outcome: &RepositoryInitOutcome) -> String {
    json_line(json!({
        "command": "init",
        "status": "initialized",
        "state_dir": outcome.state_dir,
        "created": outcome.created,
        "git_info_exclude_updated": outcome.git_info_exclude_updated,
        "root_gitignore_updated": outcome.root_gitignore_updated,
        "storage": "not_implemented",
        "indexing": "not_implemented",
        "repaired_entries": outcome.repaired_entries,
    }))
}

fn uninit_outcome_human(outcome: &RepositoryUninitOutcome) -> String {
    format!(
        "uninit: repository-local state {}\nstate_dir: {}\nlogs: removed with state dir when present\n",
        if outcome.removed {
            "removed"
        } else {
            "was not present"
        },
        outcome.state_dir
    )
}

fn uninit_outcome_json(outcome: &RepositoryUninitOutcome) -> String {
    json_line(json!({
        "command": "uninit",
        "state_dir": outcome.state_dir,
        "removed": outcome.removed,
        "logs_removed": outcome.removed,
    }))
}

fn index_outcome_human(
    command: &str,
    outcome: &IndexingOutcome,
    options: &LifecycleOptions,
) -> String {
    let mut output = format!(
        "{command}: syntax-only code units stored\nactive_generation: {}\ndiscovered_files: {}\nstored_files: {}\nskipped_paths: {}\nindexed_units: {}\nsemantic_facts: {}\nindexing: syntax_only_code_units\nparser: syntax_only\nsemantic_worker: {}\nmining: deferred\nprogress: {}\n",
        outcome.active_generation.as_deref().unwrap_or("none"),
        outcome.discovered_files,
        outcome.discovered_files,
        outcome.skipped_paths,
        outcome.indexed_units,
        outcome.semantic_facts,
        outcome.semantic_worker.as_str(),
        options.progress.as_str()
    );
    for warning in &outcome.warnings {
        output.push_str("warning: ");
        output.push_str(warning);
        output.push('\n');
    }
    output
}

fn index_outcome_json(
    command: &str,
    outcome: &IndexingOutcome,
    options: &LifecycleOptions,
) -> String {
    json_line(json!({
        "command": command,
        "status": "complete",
        "generation_id": outcome.active_generation,
        "discovered_files": outcome.discovered_files,
        "stored_files": outcome.discovered_files,
        "skipped_paths": outcome.skipped_paths,
        "indexed_units": outcome.indexed_units,
        "semantic_facts": outcome.semantic_facts,
        "indexing": "syntax_only_code_units",
        "parser": "syntax_only",
        "semantic_worker": outcome.semantic_worker.as_str(),
        "mining": "deferred",
        "progress": options.progress.as_str(),
        "warnings": outcome.warnings,
    }))
}

fn status_human(report: &RepositoryStatusReport) -> String {
    let mut output = String::new();
    output.push_str(report.status.as_human_message());
    output.push('\n');
    output.push_str(&format!("state_dir: {}\n", report.state_dir));
    output.push_str(&format!("manifest: {}\n", manifest_status(report.manifest)));
    output.push_str(&format!(
        "manifest_schema_version: {}\n",
        optional_human_number(report.manifest_schema_version)
    ));
    output.push_str(&format!(
        "storage_schema_version: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.schema_version)
            .map(|version| version.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    output.push_str(&format!(
        "journal_mode: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.journal_mode.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "integrity_check: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.integrity_check.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "active_generation: {}\n",
        match &report.status {
            RepositoryStatus::Initialized { active_generation } => active_generation.as_str(),
            _ => "none",
        }
    ));
    output.push_str(&format!(
        "storage: {}\n",
        implementation_status(report.storage)
    ));
    output.push_str(&format!(
        "indexing: {}\n",
        implementation_status(report.indexing)
    ));
    if let Some(error) = &report.storage_error {
        output.push_str("storage_error: ");
        output.push_str(error);
        output.push('\n');
    }
    for subdir in &report.missing_subdirs {
        output.push_str("missing_subdir: ");
        output.push_str(subdir);
        output.push('\n');
    }
    output
}

fn status_json(report: &RepositoryStatusReport) -> String {
    let active_generation = match &report.status {
        RepositoryStatus::Initialized { active_generation }
            if active_generation != "none" && active_generation != "not implemented" =>
        {
            Some(active_generation.as_str())
        }
        _ => None,
    };
    let storage_inspection = report.storage_inspection.as_ref();
    json_line(json!({
        "command": "status",
        "initialized": matches!(report.status, RepositoryStatus::Initialized { .. }),
        "state_dir": report.state_dir,
        "status": repository_status_value(&report.status),
        "manifest": manifest_status(report.manifest),
        "active_generation": active_generation,
        "manifest_schema_version": report.manifest_schema_version,
        "storage_schema_version": storage_inspection.and_then(|inspection| inspection.schema_version),
        "journal_mode": storage_inspection.and_then(|inspection| inspection.journal_mode.as_deref()),
        "integrity_check": storage_inspection.and_then(|inspection| inspection.integrity_check.as_deref()),
        "foreign_keys_enabled": storage_inspection.and_then(|inspection| inspection.foreign_keys_enabled),
        "storage": implementation_status(report.storage),
        "indexing": implementation_status(report.indexing),
        "storage_error": report.storage_error,
        "missing_subdirs": report.missing_subdirs,
    }))
}

fn doctor_human(report: &RepositoryDoctorReport) -> String {
    let mut output = String::from("doctor: repository lifecycle diagnostics\n");
    output.push_str(&format!("state_dir: {}\n", report.status.state_dir));
    output.push_str(&format!(
        "status: {}\n",
        repository_status_value(&report.status.status)
    ));
    for finding in &report.findings {
        output.push_str(&format!(
            "{}: {} ({})\n",
            doctor_severity(finding.severity),
            doctor_code(finding.code),
            finding.detail
        ));
    }
    output
}

fn doctor_json(report: &RepositoryDoctorReport) -> String {
    let findings = report.findings.iter().map(finding_json).collect::<Vec<_>>();
    let storage_inspection = report.status.storage_inspection.as_ref();
    let required_subdirectories =
        if matches!(report.status.status, RepositoryStatus::NotInitialized) {
            "not_applicable"
        } else if report.status.missing_subdirs.is_empty() {
            "pass"
        } else {
            "fail"
        };
    json_line(json!({
        "command": "doctor",
        "initialized": matches!(report.status.status, RepositoryStatus::Initialized { .. }),
        "state_dir": report.status.state_dir,
        "status": repository_status_value(&report.status.status),
        "checks": {
            "manifest": manifest_status(report.status.manifest),
            "required_subdirectories": required_subdirectories,
            "lifecycle_hygiene": lifecycle_hygiene_check(report),
            "locks": lock_check(report),
            "storage": implementation_status(report.status.storage),
            "indexing": implementation_status(report.status.indexing),
            "manifest_schema_version": report.status.manifest_schema_version,
            "storage_schema_version": storage_inspection.and_then(|inspection| inspection.schema_version),
            "journal_mode": storage_inspection.and_then(|inspection| inspection.journal_mode.as_deref()),
            "integrity_check": storage_inspection.and_then(|inspection| inspection.integrity_check.as_deref()),
        },
        "findings": findings,
    }))
}

fn lock_check(report: &RepositoryDoctorReport) -> &'static str {
    if matches!(report.status.status, RepositoryStatus::NotInitialized) {
        return "not_applicable";
    }
    if report.findings.iter().any(|finding| {
        matches!(
            finding.code,
            RepositoryDoctorCode::IndexLockActive
                | RepositoryDoctorCode::IndexLockUnknown
                | RepositoryDoctorCode::IndexLockInvalid
        )
    }) {
        "fail"
    } else if report
        .findings
        .iter()
        .any(|finding| finding.code == RepositoryDoctorCode::IndexLockStale)
    {
        "warning"
    } else {
        "pass"
    }
}

fn lifecycle_hygiene_check(report: &RepositoryDoctorReport) -> &'static str {
    if report.findings.iter().any(|finding| {
        matches!(
            finding.code,
            RepositoryDoctorCode::StateGitignoreMissing
                | RepositoryDoctorCode::StateGitignoreInvalid
                | RepositoryDoctorCode::GitInfoExcludeMissing
                | RepositoryDoctorCode::GitInfoExcludeIncomplete
                | RepositoryDoctorCode::RootGitignoreMarkerInvalid
                | RepositoryDoctorCode::InitReceiptMissing
                | RepositoryDoctorCode::InitReceiptInvalid
        )
    }) {
        "fail"
    } else if matches!(report.status.status, RepositoryStatus::NotInitialized) {
        "not_applicable"
    } else {
        "pass"
    }
}

fn unlock_human(outcome: &RepositoryUnlockReport) -> String {
    let mut output = format!(
        "unlock: {}\nremoved_locks: {}\n",
        outcome.message, outcome.removed_locks
    );
    for lock in &outcome.inspected_locks {
        output.push_str("inspected_lock: ");
        output.push_str(lock);
        output.push('\n');
    }
    output
}

fn unlock_json(outcome: &RepositoryUnlockReport) -> String {
    json_line(json!({
        "command": "unlock",
        "state_dir": outcome.state_dir,
        "removed_locks": outcome.removed_locks,
        "inspected_locks": outcome.inspected_locks,
        "message": outcome.message,
    }))
}

fn logs_human(outcome: &RepositoryLogsReport) -> String {
    let mut output = format!(
        "logs: {}\nstate_dir: {}\navailable: {}\nredacted: {}\n",
        outcome.message, outcome.state_dir, outcome.available, outcome.redacted
    );
    output.push_str(&format!("entries: {}\n", outcome.entries.len()));
    output
}

fn logs_json(outcome: &RepositoryLogsReport, options: &LogsOptions) -> String {
    json_line(json!({
        "command": "logs",
        "state_dir": outcome.state_dir,
        "available": outcome.available,
        "redacted": outcome.redacted,
        "paths": "repo_relative_only",
        "component_filter": options.component,
        "tail": options.tail,
        "since": options.since,
        "entries": outcome.entries,
        "message": outcome.message,
    }))
}

fn repository_status_value(status: &RepositoryStatus) -> &'static str {
    match status {
        RepositoryStatus::NotInitialized => "not_initialized",
        RepositoryStatus::Initialized { .. } => "initialized",
        RepositoryStatus::CorruptedManifest => "corrupted_manifest",
    }
}

fn manifest_status(status: RepositoryManifestStatus) -> &'static str {
    match status {
        RepositoryManifestStatus::Missing => "missing",
        RepositoryManifestStatus::Valid => "valid",
        RepositoryManifestStatus::Corrupted => "corrupted",
    }
}

fn implementation_status(status: RepositoryImplementationStatus) -> &'static str {
    match status {
        RepositoryImplementationStatus::NotImplemented => "not_implemented",
        RepositoryImplementationStatus::Available => "available",
        RepositoryImplementationStatus::FileManifestOnly => "file_manifest_only",
        RepositoryImplementationStatus::SyntaxOnlyCodeUnits => "syntax_only_code_units",
        RepositoryImplementationStatus::Unhealthy => "unhealthy",
    }
}

fn doctor_severity(severity: RepositoryDoctorSeverity) -> &'static str {
    match severity {
        RepositoryDoctorSeverity::Info => "info",
        RepositoryDoctorSeverity::Warning => "warning",
        RepositoryDoctorSeverity::Error => "error",
    }
}

fn doctor_code(code: RepositoryDoctorCode) -> &'static str {
    match code {
        RepositoryDoctorCode::NotInitialized => "NOT_INITIALIZED",
        RepositoryDoctorCode::CorruptedManifest => "CORRUPTED_MANIFEST",
        RepositoryDoctorCode::MissingSubdir => "MISSING_SUBDIR",
        RepositoryDoctorCode::StateGitignoreMissing => "STATE_GITIGNORE_MISSING",
        RepositoryDoctorCode::StateGitignoreInvalid => "STATE_GITIGNORE_INVALID",
        RepositoryDoctorCode::GitInfoExcludeMissing => "GIT_INFO_EXCLUDE_MISSING",
        RepositoryDoctorCode::GitInfoExcludeIncomplete => "GIT_INFO_EXCLUDE_INCOMPLETE",
        RepositoryDoctorCode::RootGitignoreMarkerInvalid => "ROOT_GITIGNORE_MARKER_INVALID",
        RepositoryDoctorCode::InitReceiptMissing => "INIT_RECEIPT_MISSING",
        RepositoryDoctorCode::InitReceiptInvalid => "INIT_RECEIPT_INVALID",
        RepositoryDoctorCode::IndexLockActive => "INDEX_LOCK_ACTIVE",
        RepositoryDoctorCode::IndexLockStale => "INDEX_LOCK_STALE",
        RepositoryDoctorCode::IndexLockUnknown => "INDEX_LOCK_UNKNOWN",
        RepositoryDoctorCode::IndexLockInvalid => "INDEX_LOCK_INVALID",
        RepositoryDoctorCode::StorageNotImplemented => "STORAGE_NOT_IMPLEMENTED",
        RepositoryDoctorCode::StorageReady => "STORAGE_READY",
        RepositoryDoctorCode::StorageInvalid => "STORAGE_INVALID",
        RepositoryDoctorCode::StorageNoActiveGeneration => "STORAGE_NO_ACTIVE_GENERATION",
        RepositoryDoctorCode::IndexingNotImplemented => "INDEXING_NOT_IMPLEMENTED",
        RepositoryDoctorCode::IndexingFileManifestOnly => "INDEXING_FILE_MANIFEST_ONLY",
        RepositoryDoctorCode::IndexingSyntaxOnlyCodeUnits => "INDEXING_SYNTAX_ONLY_CODE_UNITS",
    }
}

fn finding_json(finding: &RepositoryDoctorFinding) -> serde_json::Value {
    json!({
        "severity": doctor_severity(finding.severity),
        "code": doctor_code(finding.code),
        "detail": finding.detail,
    })
}

fn lifecycle_error(command: &str, json: bool, error: RepoGrammarError) -> CliOutput {
    if json {
        CliOutput::failure(
            2,
            json_line(json!({
                "command": command,
                "status": "error",
                "reason": error.to_string(),
            })),
        )
    } else {
        CliOutput::failure(2, format!("{error}\n"))
    }
}

fn json_line(value: serde_json::Value) -> String {
    let mut output = value.to_string();
    output.push('\n');
    output
}

pub fn should_emit_progress(
    mode: ProgressMode,
    json_output: bool,
    quiet: bool,
    stderr_is_terminal: bool,
) -> bool {
    if quiet {
        return false;
    }
    match mode {
        ProgressMode::Never => false,
        ProgressMode::Always => true,
        ProgressMode::Auto => !json_output && stderr_is_terminal,
    }
}

pub fn render_index_progress_event(
    command: &str,
    event: &ProgressEvent,
    json_output: bool,
) -> String {
    if json_output {
        return event.render_ndjson();
    }

    let bar = progress_bar(event.work);
    format!(
        "{command}: {bar} {}: {}\n",
        event.stage.as_str(),
        event.message
    )
}

fn progress_bar(work: WorkUnits) -> String {
    match work {
        WorkUnits::Unknown => "[working]".to_string(),
        WorkUnits::Known(work) if work.total() == 0 => "[done] 0/0".to_string(),
        WorkUnits::Known(work) => {
            let width = 20u64;
            let filled = (work.completed().saturating_mul(width) / work.total()).min(width);
            let empty = width.saturating_sub(filled);
            format!(
                "[{}{}] {}/{}",
                "#".repeat(filled as usize),
                "-".repeat(empty as usize),
                work.completed(),
                work.total()
            )
        }
    }
}

fn optional_human_number(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::adapters::filesystem::source_store::FilesystemSourceStore;
    use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::application::indexing::{
        index_repository_with_discovery_parser_frameworks_and_store, IndexingRequest,
    };
    use crate::application::query::{
        list_code_units, list_families, list_indexed_files, lookup_family, FamilySummary,
    };
    use crate::application::repository::{acquire_index_lock, DEFAULT_STATE_DIR};
    use crate::application::repository::{
        repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    };
    use crate::ports::index_store::STORAGE_SCHEMA_VERSION;
    use crate::test_support::TempWorkspace;
    use rusqlite::Connection;
    use serde_json::{json, Value};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::fs;
    use std::process::Command;

    fn current_lock_host_json() -> String {
        let host = ["HOSTNAME", "COMPUTERNAME"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .filter(|value| {
                !value.trim().is_empty()
                    && !value.contains('/')
                    && !value.contains('\\')
                    && !value.chars().any(char::is_control)
            });
        host.map(|value| json!(value).to_string())
            .unwrap_or_else(|| "null".to_string())
    }

    #[test]
    fn progress_policy_keeps_machine_output_clean_by_default() {
        assert!(should_emit_progress(ProgressMode::Auto, false, false, true));
        assert!(!should_emit_progress(ProgressMode::Auto, true, false, true));
        assert!(should_emit_progress(
            ProgressMode::Always,
            true,
            false,
            false
        ));
        assert!(!should_emit_progress(
            ProgressMode::Always,
            false,
            true,
            true
        ));
        assert!(!should_emit_progress(
            ProgressMode::Never,
            false,
            false,
            true
        ));
    }

    #[test]
    fn index_progress_renderer_uses_bar_counts_and_ndjson() {
        let event = ProgressEvent::new(
            crate::application::progress::ProgressStage::FileScanning,
            "stored files",
            WorkUnits::known(2, 4).expect("valid work units"),
        );

        let human = render_index_progress_event("index", &event, false);
        assert!(human.contains("index: [##########----------] 2/4 file_scanning"));
        assert!(!human.contains('%'));
        assert!(!human.to_ascii_lowercase().contains("eta"));

        let machine = render_index_progress_event("index", &event, true);
        let value: Value = serde_json::from_str(machine.trim()).expect("progress NDJSON");
        assert_eq!(value["stage"], "file_scanning");
        assert_eq!(value["message"], "stored files");
        assert_eq!(value["work"]["completed"], 2);
        assert_eq!(value["work"]["total"], 4);
    }

    fn stale_index_lock_json(token: &str) -> String {
        let mut value = json!({
            "kind": "index",
            "pid": 0,
            "host": null,
            "os": std::env::consts::OS,
            "started_unix_seconds": 1,
            "repogrammar_version": env!("CARGO_PKG_VERSION"),
            "token": token,
        });
        value["host"] = serde_json::from_str(&current_lock_host_json()).expect("host JSON");
        json_line(value)
    }

    fn git_init(workspace: &TempWorkspace) -> bool {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(workspace.path())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    struct TestRuntime;

    impl TestRuntime {
        fn store_for_status_request(
            &self,
            request: &RepositoryStatusRequest,
        ) -> Result<SqliteIndexStore, RepoGrammarError> {
            let location = repository_state_location(request.clone())?;
            Ok(SqliteIndexStore::new(location.state_dir))
        }
    }

    struct SemanticWorkerEnvRuntime;

    impl CliRuntime for SemanticWorkerEnvRuntime {
        fn index_repository(
            &self,
            command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            assert!(matches!(command, "index" | "sync"));
            assert_eq!(
                request.semantic_worker_executable.as_deref(),
                Some("/opt/repogrammar/typescript-worker")
            );
            assert_eq!(
                request.semantic_worker_args,
                vec!["src/workers/typescript/worker.js".to_string()]
            );
            Ok(IndexingOutcome {
                indexed_units: 1,
                semantic_facts: 2,
                discovered_files: 1,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Complete,
                warnings: Vec::new(),
            })
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("index command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("index command should not request doctor through CLI test runtime")
        }
    }

    struct BlankSemanticWorkerEnvRuntime;

    impl CliRuntime for BlankSemanticWorkerEnvRuntime {
        fn index_repository(
            &self,
            _command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            assert!(!request.strict_gitignore);
            assert_eq!(request.semantic_worker_executable, None);
            assert_eq!(request.semantic_worker_args, Vec::<String>::new());
            Ok(IndexingOutcome {
                indexed_units: 1,
                semantic_facts: 0,
                discovered_files: 1,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                warnings: Vec::new(),
            })
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("index command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("index command should not request doctor through CLI test runtime")
        }
    }

    struct StrictGitignoreEnvRuntime;

    impl CliRuntime for StrictGitignoreEnvRuntime {
        fn index_repository(
            &self,
            _command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            assert!(request.strict_gitignore);
            Ok(IndexingOutcome {
                indexed_units: 0,
                semantic_facts: 0,
                discovered_files: 0,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                warnings: Vec::new(),
            })
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("index command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("index command should not request doctor through CLI test runtime")
        }
    }

    struct ProgressRequestRuntime;

    impl CliRuntime for ProgressRequestRuntime {
        fn index_repository(
            &self,
            _command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            assert_eq!(request.progress, ProgressMode::Always);
            assert!(request.json);
            assert!(!request.quiet);
            Ok(IndexingOutcome {
                indexed_units: 0,
                semantic_facts: 0,
                discovered_files: 0,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                warnings: Vec::new(),
            })
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("index command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("index command should not request doctor through CLI test runtime")
        }
    }

    struct FamilyQueryRuntime;

    impl FamilyQueryRuntime {
        fn status_report() -> RepositoryStatusReport {
            RepositoryStatusReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                status: RepositoryStatus::Initialized {
                    active_generation: "gen-000001".to_string(),
                },
                manifest: RepositoryManifestStatus::Valid,
                manifest_schema_version: Some(1),
                missing_subdirs: Vec::new(),
                storage: RepositoryImplementationStatus::Available,
                indexing: RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
                storage_inspection: None,
                storage_error: None,
            }
        }

        fn detail() -> FamilyDetailReport {
            let hash = crate::core::model::ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid content hash");
            FamilyDetailReport {
                active_generation: "gen-000001".to_string(),
                family_id: "family:typescript:express_route:express".to_string(),
                classification: "DOMINANT_PATTERN".to_string(),
                support: 2,
                members: vec![crate::ports::family_store::IndexedFamilyMemberRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    code_unit_id: "unit:src/routes/a.ts#express_route:get:0-20:1".to_string(),
                    role: "framework:express.route_handler".to_string(),
                }],
                variation_slots: vec![crate::ports::family_store::IndexedVariationSlotRecord {
                    family_id: "family:typescript:express_route:express".to_string(),
                    slot_id: "slot:runtime_unknown".to_string(),
                    description:
                        "non_blocking_unknown:FrameworkMagic:runtime equivalence remains unproven"
                            .to_string(),
                }],
                evidence: vec![crate::ports::family_store::IndexedFamilyEvidenceRecord {
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
                    class: crate::core::model::UnknownClass::NonBlocking,
                    reason: crate::core::model::UnknownReasonCode::FrameworkMagic,
                    affected_claim: "runtime_equivalence".to_string(),
                    recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
                }],
            }
        }

        fn source_span_report() -> SourceSpanRenderReport {
            let hash = crate::core::model::ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid content hash");
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

    impl CliRuntime for FamilyQueryRuntime {
        fn index_repository(
            &self,
            _command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            unreachable!("family query tests do not index")
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            Ok(Self::status_report())
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("family query tests do not call doctor")
        }

        fn families(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<FamilyListReport, RepoGrammarError> {
            Ok(FamilyListReport {
                active_generation: "gen-000001".to_string(),
                families: vec![FamilySummary {
                    family_id: "family:typescript:express_route:express".to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    support: 2,
                }],
                unknowns: Vec::new(),
            })
        }

        fn family_lookup(
            &self,
            _request: RepositoryStatusRequest,
            target: Option<&str>,
            mode: FamilyLookupMode,
        ) -> Result<FamilyLookupReport, RepoGrammarError> {
            let matched = match mode {
                FamilyLookupMode::FuzzyQuery => target == Some("src/routes/a.ts"),
                FamilyLookupMode::ExactFamilyId => {
                    target == Some("family:typescript:express_route:express")
                }
                FamilyLookupMode::ExactMemberId => {
                    target == Some("unit:src/routes/a.ts#express_route:get:0-20:1")
                }
            };
            if matched {
                Ok(FamilyLookupReport::Found(Self::detail()))
            } else {
                Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
                    active_generation: "gen-000001".to_string(),
                    unknowns: vec![FamilyQueryUnknown {
                        class: crate::core::model::UnknownClass::Blocking,
                        reason: crate::core::model::UnknownReasonCode::InsufficientSupport,
                        affected_claim: "query target".to_string(),
                        recovery: Some(
                            "run repogrammar index after adding compatible implementations"
                                .to_string(),
                        ),
                    }],
                }))
            }
        }

        fn render_source_spans(
            &self,
            _request: RepositoryStatusRequest,
            _read_plan: &ReadPlan,
            include_source_spans: bool,
            _token_budget: Option<usize>,
        ) -> Result<SourceSpanRenderReport, RepoGrammarError> {
            if include_source_spans {
                Ok(Self::source_span_report())
            } else {
                Err(RepoGrammarError::InvalidInput(
                    "source spans were not requested".to_string(),
                ))
            }
        }

        fn repo_shape_diagnostics(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
            Ok(RepoShapeDiagnosticsReport {
                active_generation: "gen-000001".to_string(),
                eligible_code_units: 4,
                family_count: 1,
                family_member_count: 3,
                covered_code_units: 3,
                local_pattern_density: Some(0.75),
                family_support_coverage: Some(0.75),
                abstention_rate: Some(0.25),
                external_dependency_signal: DiagnosticSignal::Unknown,
                thin_wrapper_risk: DiagnosticSignal::Low,
                token_saving_risk: DiagnosticSignal::Low,
                interpretation:
                    "RepoGrammar can provide integration-pattern context when repeated local patterns exist; third-party-heavy or thin-wrapper repositories may see lower token-saving potential.",
            })
        }
    }

    impl CliRuntime for TestRuntime {
        fn index_repository(
            &self,
            _command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            let status_request = RepositoryStatusRequest {
                path: request.repository_root.clone(),
                state_dir_override: request.state_dir_override.clone(),
            };
            let store = self.store_for_status_request(&status_request)?;
            let status = repository_status_with_storage(status_request, &store)?;
            match status.status {
                RepositoryStatus::NotInitialized => {
                    return Err(RepoGrammarError::InvalidInput(
                        "repository is not initialized; run repogrammar init".to_string(),
                    ));
                }
                RepositoryStatus::CorruptedManifest => {
                    return Err(RepoGrammarError::InvalidInput(
                        "repository manifest is corrupted; run repogrammar doctor".to_string(),
                    ));
                }
                RepositoryStatus::Initialized { .. } => {}
            }
            if !status.missing_subdirs.is_empty() {
                return Err(RepoGrammarError::InvalidInput(
                    "repository-local state is missing required subdirectories; run repogrammar doctor"
                        .to_string(),
                ));
            }
            if status.storage == RepositoryImplementationStatus::Unhealthy {
                return Err(RepoGrammarError::InvalidInput(
                    "repository-local storage is unhealthy; run repogrammar doctor".to_string(),
                ));
            }

            let framework_roles = SyntaxFrameworkRoleDetector;
            index_repository_with_discovery_parser_frameworks_and_store(
                IndexingRequest {
                    repository_root: request.repository_root,
                    state_dir_override: request.state_dir_override,
                    max_file_bytes: request.max_file_bytes,
                    strict_gitignore: request.strict_gitignore,
                },
                &FilesystemFileDiscovery,
                &FilesystemSourceStore,
                &SyntaxCodeUnitParser,
                &framework_roles,
                &store,
            )
        }

        fn repository_status(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            repository_status_with_storage(request, &store)
        }

        fn repository_doctor(
            &self,
            request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            let status_request = RepositoryStatusRequest {
                path: request.path.clone(),
                state_dir_override: request.state_dir_override.clone(),
            };
            let store = self.store_for_status_request(&status_request)?;
            repository_doctor_with_storage(request, &store)
        }

        fn indexed_files(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<IndexedFilesReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            list_indexed_files(&store)
        }

        fn indexed_units(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<IndexedCodeUnitsReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            list_code_units(&store)
        }

        fn families(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<FamilyListReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            list_families(&store)
        }

        fn family_lookup(
            &self,
            request: RepositoryStatusRequest,
            target: Option<&str>,
            mode: FamilyLookupMode,
        ) -> Result<FamilyLookupReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            lookup_family(&store, target, mode)
        }
    }

    fn indexed_paths(workspace: &TempWorkspace, generation_id: &str) -> Vec<String> {
        let database = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("generations")
            .join(generation_id)
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open generation database");
        let paths = connection
            .prepare("SELECT path FROM indexed_files ORDER BY path")
            .expect("prepare indexed paths")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query indexed paths")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect indexed paths");
        paths
    }

    fn json_file_count(path: &Path) -> usize {
        if !path.is_dir() {
            return 0;
        }
        fs::read_dir(path)
            .expect("read json file dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("json")
            })
            .count()
    }

    #[test]
    fn version_succeeds() {
        let output = run(["--version"]);

        assert_eq!(output.status, 0);
        assert!(output.stdout.starts_with("repogrammar "));
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn pattern_family_command_surface_is_recognized() {
        let workspace = TempWorkspace::new("cli-query-surface");
        let env = |_: &str| None;
        for command in ["find", "families", "family", "member", "explain", "check"] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init\n"
            ));
            assert!(output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
        for command in ["files", "units"] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init\n"
            ));
            assert!(output
                .stderr
                .contains("requires a readable active syntax-only index"));
            assert!(!output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
    }

    #[test]
    fn families_human_preserves_typed_stale_unknowns() {
        let report = FamilyListReport {
            active_generation: "gen-000001".to_string(),
            families: Vec::new(),
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::StaleEvidence,
                affected_claim:
                    "family:python:fastapi_route:framework_fastapi_route:evidence_freshness"
                        .to_string(),
                recovery: Some("run repogrammar sync".to_string()),
            }],
        };

        let output = families_human(&report);

        assert!(output.starts_with("families: UNKNOWN\nactive_generation: gen-000001\n"));
        assert!(output.contains(
            "unknown: blocking_unknown:StaleEvidence affected_claim: family:python:fastapi_route:framework_fastapi_route:evidence_freshness\n"
        ));
        assert!(output.contains("recovery: run repogrammar sync\n"));
        assert!(!output.contains("InsufficientSupport"));
        assert!(!output.contains("adding compatible implementations"));
    }

    #[test]
    fn family_unknown_human_formats_recovery_as_separate_line() {
        let report = FamilyUnknownReport {
            active_generation: "gen-000001".to_string(),
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::StaleEvidence,
                affected_claim:
                    "family:python:pytest_test:framework_pytest_test:evidence_freshness".to_string(),
                recovery: Some("run repogrammar sync".to_string()),
            }],
        };

        let output = family_unknown_human("family", &report);

        assert!(output.contains(
            "unknown: blocking_unknown:StaleEvidence affected_claim: family:python:pytest_test:framework_pytest_test:evidence_freshness\nrecovery: run repogrammar sync\n"
        ));
    }

    #[test]
    fn index_request_passes_optional_semantic_worker_env_to_runtime() {
        let workspace = TempWorkspace::new("cli-semantic-worker-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => {
                Some("/opt/repogrammar/typescript-worker".to_string())
            }
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => {
                Some(r#"["src/workers/typescript/worker.js"]"#.to_string())
            }
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["index", "--json"],
            workspace.path(),
            &env,
            &SemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["semantic_worker"], "complete");
        assert_eq!(value["semantic_facts"], 2);
        assert_eq!(value["mining"], "deferred");
    }

    #[test]
    fn sync_request_passes_optional_semantic_worker_env_to_runtime() {
        let workspace = TempWorkspace::new("cli-sync-semantic-worker-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => {
                Some("/opt/repogrammar/typescript-worker".to_string())
            }
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => {
                Some(r#"["src/workers/typescript/worker.js"]"#.to_string())
            }
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["sync", "--json"],
            workspace.path(),
            &env,
            &SemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("sync JSON");
        assert_eq!(value["command"], "sync");
        assert_eq!(value["semantic_worker"], "complete");
        assert_eq!(value["semantic_facts"], 2);
    }

    #[test]
    fn blank_semantic_worker_env_is_not_configured() {
        let workspace = TempWorkspace::new("cli-blank-semantic-worker-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => Some(String::new()),
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => Some(String::new()),
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["index", "--json"],
            workspace.path(),
            &env,
            &BlankSemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["semantic_facts"], 0);
    }

    #[test]
    fn strict_gitignore_env_is_passed_to_index_request() {
        let workspace = TempWorkspace::new("cli-strict-gitignore-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_STRICT_GITIGNORE" => Some("true".to_string()),
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["index", "--json"],
            workspace.path(),
            &env,
            &StrictGitignoreEnvRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["command"], "index");
    }

    #[test]
    fn index_progress_options_are_passed_to_runtime() {
        let workspace = TempWorkspace::new("cli-index-progress-request");
        let env = |_: &str| None;
        let output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "always"],
            workspace.path(),
            &env,
            &ProgressRequestRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["progress"], "always");
    }

    #[test]
    fn invalid_semantic_worker_args_json_is_rejected_without_echoing_value() {
        let workspace = TempWorkspace::new("cli-bad-worker-args-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => {
                Some("/opt/repogrammar/typescript-worker".to_string())
            }
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => Some("[\"ok\", 1]".to_string()),
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["index", "--json"],
            workspace.path(),
            &env,
            &SemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert_eq!(value["status"], "error");
        assert!(value["reason"]
            .as_str()
            .expect("reason string")
            .contains("REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON"));
        assert!(!output.stderr.contains("[\"ok\", 1]"));
    }

    #[test]
    fn invalid_semantic_worker_arg_value_is_rejected_without_echoing_value() {
        let workspace = TempWorkspace::new("cli-bad-worker-arg-value-env");
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => {
                Some("/opt/repogrammar/typescript-worker".to_string())
            }
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => {
                Some("[\"ok\", \"bad\\narg\"]".to_string())
            }
            _ => None,
        };
        let output = run_with_context_and_runtime(
            ["index", "--json"],
            workspace.path(),
            &env,
            &SemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert_eq!(value["status"], "error");
        assert_eq!(
            value["reason"],
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON contains an invalid argument"
        );
        assert!(!output.stderr.contains("bad"));
    }

    #[test]
    fn semantic_worker_arg_limits_are_rejected_without_echoing_values() {
        let workspace = TempWorkspace::new("cli-worker-arg-limits-env");
        let cases = [
            (
                format!("[{}]", vec![r#""arg""#; 65].join(",")),
                "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON must contain at most 64 arguments",
            ),
            (
                format!("[\"{}\"]", "x".repeat(4097)),
                "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON contains an invalid argument",
            ),
        ];

        for (raw_args, expected_reason) in cases {
            let env = |key: &str| match key {
                "REPOGRAMMAR_TYPESCRIPT_WORKER" => {
                    Some("/opt/repogrammar/typescript-worker".to_string())
                }
                "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => Some(raw_args.clone()),
                _ => None,
            };
            let output = run_with_context_and_runtime(
                ["index", "--json"],
                workspace.path(),
                &env,
                &SemanticWorkerEnvRuntime,
            );

            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
            assert_eq!(value["command"], "index");
            assert_eq!(value["status"], "error");
            assert_eq!(value["reason"], expected_reason);
            assert!(!output.stderr.contains(&raw_args));
        }
    }

    #[test]
    fn semantic_worker_args_without_executable_are_rejected() {
        let workspace = TempWorkspace::new("cli-worker-args-no-exec");
        let env = |key: &str| {
            (key == "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON")
                .then(|| r#"["src/workers/typescript/worker.js"]"#.to_string())
        };
        let output = run_with_context_and_runtime(
            ["index"],
            workspace.path(),
            &env,
            &BlankSemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert!(output
            .stderr
            .contains("requires REPOGRAMMAR_TYPESCRIPT_WORKER"));
        assert!(!output.stderr.contains("src/workers/typescript/worker.js"));
    }

    #[test]
    fn query_options_are_accepted() {
        let workspace = TempWorkspace::new("cli-query-options");
        let env = |_: &str| None;
        let output = run_with_context(
            [
                "find",
                "--project",
                ".",
                "--token-budget",
                "8000",
                "--mode",
                "evidence",
                "--json",
                "--include-variations",
                "--include-exceptions",
                "--include-source-spans",
                "src/user.ts",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository is not initialized");
        assert_eq!(fallback["guidance"], "run repogrammar init");
        assert_eq!(fallback["command"], "find");
        assert_eq!(fallback["implemented"], false);
    }

    #[test]
    fn query_options_validate_detail_modes_and_token_budget() {
        let workspace = TempWorkspace::new("cli-query-mode-validation");
        let env = |_: &str| None;

        for args in [
            vec!["find", "--mode", "source"],
            vec!["find", "--token-budget", "0"],
            vec!["find", "--token-budget", "many"],
        ] {
            let output = run_with_context(args, workspace.path(), &env);
            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
        }

        let over_budget = (crate::application::query::MAX_QUERY_TOKEN_BUDGET + 1).to_string();
        assert!(parse_query_options(&["--token-budget".to_string(), over_budget,]).is_err());
        let over_target = "x".repeat(crate::application::query::MAX_QUERY_TARGET_BYTES + 1);
        assert!(parse_query_options(&[over_target]).is_err());
        assert!(parse_query_options(&["contains\nnewline".to_string()]).is_err());
        assert!(parse_query_options(&["src/a.py".to_string(), "src/b.py".to_string()]).is_err());
    }

    #[test]
    fn every_query_command_supports_json_missing_index_fallback() {
        let workspace = TempWorkspace::new("cli-query-missing-index");
        let env = |_: &str| None;
        for command in [
            "find", "families", "family", "member", "explain", "check", "files", "units",
        ] {
            let output = run_with_context([command, "--json"], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            let fallback: Value =
                serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
            assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
            assert_eq!(fallback["reason"], "repository is not initialized");
            assert_eq!(fallback["guidance"], "run repogrammar init");
            assert_eq!(fallback["command"], command);
            assert_eq!(
                fallback["implemented"],
                matches!(command, "files" | "units")
            );
        }
    }

    #[test]
    fn stats_json_reports_repo_shape_diagnostics_without_token_savings_claim() {
        let workspace = TempWorkspace::new("cli-stats-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output =
            run_with_context_and_runtime(["stats", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("stats JSON");
        assert_eq!(value["command"], "stats");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(
            value["metrics"]["local_pattern_density"].as_f64(),
            Some(0.75)
        );
        assert_eq!(
            value["metrics"]["family_support_coverage"].as_f64(),
            Some(0.75)
        );
        assert_eq!(value["metrics"]["abstention_rate"].as_f64(), Some(0.25));
        assert_eq!(value["metrics"]["external_dependency_signal"], Value::Null);
        assert_eq!(value["metrics"]["thin_wrapper_risk"], "low");
        assert_eq!(value["metrics"]["token_saving_risk"], "low");
        assert_eq!(value["token_savings"], Value::Null);
        assert!(value["claim"]
            .as_str()
            .expect("claim")
            .contains("not measured token savings"));
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn stats_json_keeps_passive_diagnostics_local_when_telemetry_disabled() {
        let workspace = TempWorkspace::new("cli-stats-telemetry-disabled");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = FamilyQueryRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output =
            run_with_context_and_runtime(["stats", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let telemetry_dir = workspace.path().join(DEFAULT_STATE_DIR).join("telemetry");
        assert_eq!(json_file_count(&telemetry_dir.join("rollups")), 0);
        assert_eq!(json_file_count(&telemetry_dir.join("queue")), 0);
        assert_eq!(json_file_count(&telemetry_dir.join("sent")), 0);
    }

    #[test]
    fn stats_json_records_bucketed_rollup_when_telemetry_enabled() {
        let workspace = TempWorkspace::new("cli-stats-telemetry-rollup");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = FamilyQueryRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context(["telemetry", "on"], workspace.path(), &env).status,
            0
        );

        let output =
            run_with_context_and_runtime(["stats", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let telemetry_dir = workspace.path().join(DEFAULT_STATE_DIR).join("telemetry");
        let rollups_dir = telemetry_dir.join("rollups");
        assert_eq!(json_file_count(&rollups_dir), 1);
        let rollup_files = fs::read_dir(&rollups_dir)
            .expect("rollups dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .collect::<Vec<_>>();
        assert_eq!(rollup_files.len(), 1);
        assert_eq!(json_file_count(&telemetry_dir.join("queue")), 0);
        assert_eq!(json_file_count(&telemetry_dir.join("sent")), 0);
        let payload: Value = serde_json::from_str(
            &fs::read_to_string(&rollup_files[0]).expect("read telemetry rollup"),
        )
        .expect("telemetry rollup JSON");
        assert_eq!(payload["schema_version"], "telemetry.v1");
        assert_eq!(payload["source_snippets_returned"], false);
        assert_eq!(payload["eligible_code_units_bucket"], "3-9");
        assert_eq!(payload["family_count_bucket"], "1-2");
        assert_eq!(payload["measured_token_savings_bucket"], Value::Null);
        let serialized = payload.to_string();
        assert!(!serialized.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("src/"));
        assert!(!serialized.contains("sha256:"));
        assert!(!serialized.contains("query_text"));
        assert!(!serialized.contains("raw_error"));

        let status = run_with_context(
            [
                "telemetry",
                "status",
                "--json",
                "--endpoint",
                "https://telemetry.example.invalid/v1",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(status.status, 0);
        let status_value: Value =
            serde_json::from_str(status.stdout.trim()).expect("telemetry status JSON");
        assert_eq!(status_value["rollup_count"], 1);
        assert_eq!(status_value["queue_count"], 0);
        assert_eq!(status_value["sent_receipt_count"], 0);
        assert_eq!(status_value["network_upload_configured"], true);
        assert_eq!(status_value["upload_would_open_network_connection"], true);
    }

    #[test]
    fn stats_json_uses_fallback_without_active_index() {
        let workspace = TempWorkspace::new("cli-stats-missing-index");
        let env = |_: &str| None;

        let output = run_with_context(["stats", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("stats fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository is not initialized");
        assert_eq!(fallback["guidance"], "run repogrammar init");
        assert_eq!(fallback["command"], "stats");
        assert_eq!(fallback["implemented"], true);
    }

    #[test]
    fn query_fallback_distinguishes_initialized_state_from_missing_state() {
        let workspace = TempWorkspace::new("cli-query-initialized-fallback");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["find", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let unknown: Value =
            serde_json::from_str(output.stdout.trim()).expect("query UNKNOWN must be JSON");
        assert_eq!(unknown["command"], "find");
        assert_eq!(unknown["status"], "UNKNOWN");
        assert_eq!(unknown["implemented"], true);
        assert_eq!(unknown["active_generation"], "gen-000001");
        assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");
    }

    #[test]
    fn family_query_compact_mode_omits_evidence_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-family-query-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains("res.json"));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
        assert_eq!(value["command"], "find");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(
            value["family"]["family_id"],
            "family:typescript:express_route:express"
        );
        assert_eq!(value["output"]["mode"], "compact");
        assert_eq!(value["output"]["estimated_evidence_tokens"], 0);
        assert!(
            value["output"]["estimated_read_plan_tokens"]
                .as_u64()
                .expect("read plan tokens")
                > 0
        );
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert!(value["evidence"].as_array().expect("evidence").is_empty());
        assert_eq!(value["read_plan"]["source_snippets_included"], false);
        assert_eq!(value["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(
            value["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
        assert_eq!(value["read_plan"]["items"][0]["path"], "src/routes/a.ts");
        assert_eq!(value["read_plan"]["items"][0]["start_byte"], 0);
        assert_eq!(value["read_plan"]["items"][0]["end_byte"], 20);
        assert_eq!(value["read_plan"]["items"][0]["start_line"], Value::Null);
        assert_eq!(value["read_plan"]["items"][0]["end_line"], Value::Null);
        assert_eq!(
            value["read_plan"]["items"][0]["content_hash"],
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            value["read_plan"]["items"][0]["source_snippets_included"],
            false
        );
        assert_eq!(value["unknowns"][0]["reason"], "FrameworkMagic");

        let human = run_with_context_and_runtime(
            ["find", "src/routes/a.ts"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(human.status, 0);
        assert!(!human.stdout.contains("evidence:"));
        assert!(human.stdout.contains("evidence_mode: compact"));
        assert!(human.stdout.contains("source_snippets: not_included"));
        assert!(human.stdout.contains("Suggested source spans to read"));
        assert!(human
            .stdout
            .contains("read: target_body_required_for_edit\tpath: src/routes/a.ts"));

        let explicit_compact = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--mode",
                "compact",
                "--token-budget",
                "1",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        let value: Value =
            serde_json::from_str(explicit_compact.stdout.trim()).expect("family JSON");
        assert_eq!(value["output"]["mode"], "compact");
        assert_eq!(value["output"]["token_budget"], 1);
        assert!(value["evidence"].as_array().expect("evidence").is_empty());
        assert_eq!(
            value["read_plan"]["items"][0]["purpose"],
            "target_body_required_for_edit"
        );
    }

    #[test]
    fn family_query_source_spans_require_explicit_flag() {
        let workspace = TempWorkspace::new("cli-family-query-source-spans");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--json",
                "--include-source-spans",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
        assert_eq!(value["output"]["source_snippets_included"], true);
        assert_eq!(value["read_plan"]["source_snippets_included"], true);
        assert_eq!(value["read_plan"]["items"][0]["start_line"], 1);
        assert_eq!(value["read_plan"]["items"][0]["end_line"], 2);
        assert_eq!(
            value["read_plan"]["items"][0]["source_snippets_included"],
            true
        );
        assert_eq!(value["source_spans"]["requested"], true);
        assert_eq!(value["source_spans"]["source_snippets_included"], true);
        assert_eq!(
            value["source_spans"]["spans"][0]["text"],
            "1\texport const handler = () => {\n2\t  return ok\n"
        );
        assert_eq!(
            value["source_spans"]["omissions"][0]["reason"],
            "stale_evidence"
        );

        let human = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--include-source-spans"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(human.status, 0);
        assert!(human.stdout.contains("source_snippets: included"));
        assert!(human.stdout.contains("source_span_policy: requested: true"));
        assert!(human.stdout.contains("1\texport const handler = () => {"));
        assert!(human
            .stdout
            .contains("source_span_omitted: support_evidence"));
    }

    #[test]
    fn family_explain_and_check_json_include_metadata_only_read_plan() {
        let workspace = TempWorkspace::new("cli-family-read-plan-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        for (command, target, requires_source) in [
            ("family", "family:typescript:express_route:express", false),
            ("explain", "src/routes/a.ts", true),
            ("check", "src/routes/a.ts", true),
        ] {
            let output = run_with_context_and_runtime(
                [command, target, "--json"],
                workspace.path(),
                &env,
                &runtime,
            );

            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
            assert_eq!(value["command"], command);
            assert_eq!(value["read_plan"]["source_snippets_included"], false);
            assert_eq!(
                value["read_plan"]["requires_source_before_edit"],
                requires_source
            );
            assert!(!value["read_plan"]["items"]
                .as_array()
                .expect("read plan items")
                .is_empty());
            assert_eq!(value["read_plan"]["items"][0]["path"], "src/routes/a.ts");
            assert!(!output
                .stdout
                .contains(workspace.path().to_string_lossy().as_ref()));
            assert!(!output.stdout.contains("export const"));
        }
    }

    #[test]
    fn family_query_evidence_mode_returns_metadata_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-family-query-evidence-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--mode",
                "evidence",
                "--token-budget",
                "1",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains("res.json"));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
        assert_eq!(value["output"]["mode"], "evidence");
        assert_eq!(value["output"]["token_budget"], 1);
        assert_eq!(
            value["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_eq!(
            value["output"]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert_eq!(value["output"]["missing_claims"], json!([]));
        assert_eq!(value["output"]["budget_satisfied"], false);
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert_eq!(value["read_plan"]["source_snippets_included"], false);
        assert_eq!(value["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(value["read_plan"]["budget_satisfied"], false);
        assert_eq!(value["evidence"][0]["path"], "src/routes/a.ts");
        assert_eq!(value["read_plan"]["items"][0]["path"], "src/routes/a.ts");
        assert_eq!(
            value["evidence"][0]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert!(
            value["output"]["estimated_evidence_tokens"]
                .as_u64()
                .expect("estimated tokens")
                > 1
        );
    }

    #[test]
    fn family_query_include_flags_report_uncovered_variations_and_exceptions() {
        let workspace = TempWorkspace::new("cli-family-query-include-coverage");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--mode",
                "evidence",
                "--include-variations",
                "--include-exceptions",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
        assert_eq!(
            value["output"]["covered_claims"],
            json!(["canonical", "support"])
        );
        assert_eq!(
            value["output"]["missing_claims"],
            json!(["variation", "exception"])
        );
        assert_eq!(
            value["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn family_query_deep_mode_remains_metadata_only_until_span_reader_exists() {
        let workspace = TempWorkspace::new("cli-family-query-deep-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--mode", "deep", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("family JSON");
        assert_eq!(value["output"]["mode"], "deep");
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert_eq!(value["read_plan"]["source_snippets_included"], false);
        assert_eq!(value["evidence"][0]["path"], "src/routes/a.ts");
    }

    #[test]
    fn family_and_member_commands_do_not_use_fuzzy_targets() {
        let workspace = TempWorkspace::new("cli-family-exact-targets");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        for command in ["family", "member"] {
            let output = run_with_context_and_runtime(
                [command, "src/routes/a.ts", "--json"],
                workspace.path(),
                &env,
                &runtime,
            );

            assert_eq!(output.status, 0);
            let value: Value = serde_json::from_str(output.stdout.trim()).expect("UNKNOWN JSON");
            assert_eq!(value["command"], command);
            assert_eq!(value["status"], "UNKNOWN");
            assert_eq!(value["unknowns"][0]["reason"], "InsufficientSupport");
        }
    }

    #[test]
    fn family_check_json_is_context_only_when_conformance_is_unproven() {
        let workspace = TempWorkspace::new("cli-family-check-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            ["check", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("check JSON");
        assert_eq!(value["command"], "check");
        assert_eq!(value["status"], "CONTEXT_ONLY");
        assert_eq!(value["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(value["read_plan"]["source_snippets_included"], false);
        assert_eq!(value["check"]["advisory_status"], "UNKNOWN");
        assert_eq!(
            value["check"]["reason"],
            "runtime equivalence remains unproven"
        );
    }

    #[test]
    fn family_check_human_remains_advisory_when_runtime_equivalence_is_unknown() {
        let workspace = TempWorkspace::new("cli-family-check-human");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            ["check", "src/routes/a.ts"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("check: CONTEXT_ONLY"));
        assert!(output.stdout.contains("advisory_status: UNKNOWN"));
        assert!(output
            .stdout
            .contains("runtime equivalence remains unproven"));
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn files_and_units_fallback_requires_active_index_after_init() {
        let workspace = TempWorkspace::new("cli-query-active-index-required");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        for command in ["files", "units"] {
            let human = run_with_context_and_runtime([command], workspace.path(), &env, &runtime);
            assert_eq!(human.status, 2);
            assert!(human.stderr.contains("FALLBACK_TO_CODE_SEARCH"));
            assert!(human.stderr.contains("reason: no active index generation"));
            assert!(human
                .stderr
                .contains("requires a readable active syntax-only index"));

            let json =
                run_with_context_and_runtime([command, "--json"], workspace.path(), &env, &runtime);
            assert_eq!(json.status, 2);
            let fallback: Value =
                serde_json::from_str(json.stderr.trim()).expect("query fallback must be JSON");
            assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
            assert_eq!(fallback["reason"], "no active index generation");
            assert_eq!(fallback["guidance"], "run repogrammar index");
            assert_eq!(fallback["command"], command);
            assert_eq!(fallback["implemented"], true);
        }
    }

    #[test]
    fn files_json_reads_active_syntax_only_index_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-files-json-read");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        let sentinel = "UNIQUE_SOURCE_SENTINEL_DO_NOT_LEAK";
        fs::write(
            workspace.path().join("a.ts"),
            format!("export const a = '{sentinel}';\n"),
        )
        .expect("write source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["files", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains(sentinel));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("files JSON");
        assert_eq!(value["command"], "files");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        let files = value["files"].as_array().expect("files array");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["path"], "a.ts");
        assert_eq!(files[0]["language"], "typescript");
        assert!(files[0]["content_hash"]
            .as_str()
            .expect("content hash")
            .starts_with("sha256:"));
    }

    #[test]
    fn units_json_reads_active_syntax_only_units_without_family_claims() {
        let workspace = TempWorkspace::new("cli-units-json-read");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        let sentinel = "UNIQUE_SOURCE_SENTINEL_DO_NOT_LEAK";
        fs::write(
            workspace.path().join("a.ts"),
            format!("export function a() {{ return '{sentinel}'; }}\n"),
        )
        .expect("write source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["units", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains(sentinel));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("units JSON");
        assert_eq!(value["command"], "units");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["mining"], "deferred");
        let units = value["units"].as_array().expect("units array");
        assert!(units
            .iter()
            .any(|unit| unit["path"] == "a.ts" && unit["kind"] == "module"));
        assert!(units.iter().all(|unit| unit["content_hash"]
            .as_str()
            .expect("content hash")
            .starts_with("sha256:")));
    }

    #[test]
    fn files_and_units_human_read_active_index_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-files-units-human-read");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        let sentinel = "UNIQUE_SOURCE_SENTINEL_DO_NOT_LEAK";
        fs::write(
            workspace.path().join("a.ts"),
            format!("export const a = '{sentinel}';\n"),
        )
        .expect("write source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let files = run_with_context_and_runtime(["files"], workspace.path(), &env, &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        assert!(files.stdout.contains("files: active index metadata"));
        assert!(files.stdout.contains("file: a.ts"));
        assert!(!files
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!files.stdout.contains(sentinel));

        let units = run_with_context_and_runtime(["units"], workspace.path(), &env, &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        assert!(units.stdout.contains("units: active index code units"));
        assert!(units.stdout.contains("semantic_worker: deferred"));
        assert!(units.stdout.contains("mining: deferred"));
        assert!(!units
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!units.stdout.contains(sentinel));
        assert!(!units.stdout.contains("DOMINANT_PATTERN"));
        assert!(!units.stdout.contains("CONFORMS"));
    }

    #[test]
    fn files_and_units_fallback_to_doctor_for_broken_active_pointer() {
        let workspace = TempWorkspace::new("cli-files-units-broken-pointer");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("current-generation"),
            "gen-999999\n",
        )
        .expect("break pointer");

        for command in ["files", "units"] {
            let output =
                run_with_context_and_runtime([command, "--json"], workspace.path(), &env, &runtime);
            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            let fallback: Value =
                serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
            assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
            assert_eq!(fallback["reason"], "repository status is unavailable");
            assert_eq!(fallback["guidance"], "run repogrammar doctor");
            assert_eq!(fallback["command"], command);
            assert_eq!(fallback["implemented"], true);
        }
    }

    #[test]
    fn query_fallback_reports_unavailable_repository_status() {
        let workspace = TempWorkspace::new("cli-query-corrupted-state");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");

        let output = run_with_context(["find", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository status is unavailable");
        assert_eq!(fallback["guidance"], "run repogrammar doctor");
        assert_eq!(fallback["implemented"], false);
    }

    #[test]
    fn forbidden_graph_commands_are_not_top_level() {
        for command in [
            "callers", "callees", "impact", "affected", "node", "explore",
        ] {
            let output = run([command]);

            assert_eq!(output.status, 2);
            assert!(output.stderr.contains("not a v0.1 top-level command"));
        }
    }

    #[test]
    fn init_creates_state_and_json_is_parseable() {
        let workspace = TempWorkspace::new("cli-init");
        let env = |_: &str| None;

        let output = run_with_context(
            ["init", "--json", "--write-gitignore"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "initialized");
        assert_eq!(value["state_dir"], DEFAULT_STATE_DIR);
        assert_eq!(value["storage"], "not_implemented");
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
        assert!(workspace.path().join(".gitignore").is_file());
    }

    #[test]
    fn init_human_output_mentions_deferred_storage_without_claiming_indexing() {
        let workspace = TempWorkspace::new("cli-init-human");
        let env = |_: &str| None;

        let output = run_with_context(["init"], workspace.path(), &env);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("repository-local state ready"));
        assert!(output.stdout.contains("storage: not_implemented"));
        assert!(output.stdout.contains("indexing: not_implemented"));
    }

    #[test]
    fn status_reports_not_initialized_and_initialized_in_human_and_json() {
        let workspace = TempWorkspace::new("cli-status");
        let env = |_: &str| None;

        let human = run_with_context(["status"], workspace.path(), &env);
        assert_eq!(human.status, 0);
        assert!(human
            .stdout
            .contains("RepoGrammar repository status: not initialized"));
        assert!(human.stdout.contains("manifest_schema_version: none"));
        assert!(human.stdout.contains("storage_schema_version: none"));
        assert!(!human
            .stdout
            .lines()
            .any(|line| line.starts_with("schema_version:")));

        let json = run_with_context(["status", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(json.stdout.trim()).expect("status JSON");
        assert_eq!(value["initialized"], false);
        assert_eq!(value["state_dir"], DEFAULT_STATE_DIR);
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], Value::Null);
        assert_eq!(value["storage_schema_version"], Value::Null);

        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let initialized_human = run_with_context(["status"], workspace.path(), &env);
        assert!(initialized_human
            .stdout
            .contains("manifest_schema_version: 1"));
        assert!(initialized_human
            .stdout
            .contains("storage_schema_version: none"));
        assert!(!initialized_human
            .stdout
            .lines()
            .any(|line| line.starts_with("schema_version:")));
        let initialized = run_with_context(["status", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(initialized.stdout.trim()).expect("status JSON");
        assert_eq!(value["initialized"], true);
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["indexing"], "not_implemented");
    }

    #[test]
    fn doctor_handles_valid_and_corrupted_state() {
        let workspace = TempWorkspace::new("cli-doctor");
        let env = |_: &str| None;

        let missing = run_with_context(["doctor", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(missing.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["initialized"], false);
        assert_eq!(value["checks"]["storage"], "not_implemented");
        assert_eq!(value["checks"]["required_subdirectories"], "not_applicable");

        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");
        let corrupt = run_with_context(["doctor", "--json"], workspace.path(), &env);
        let value: Value = serde_json::from_str(corrupt.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["manifest"], "corrupted");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "CORRUPTED_MANIFEST"));
    }

    #[test]
    fn index_json_stores_syntax_only_code_units_without_family_claims() {
        let workspace = TempWorkspace::new("cli-index-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        fs::write(workspace.path().join("note.md"), "# ignored\n").expect("write ignored");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["command"], "index");
        assert_eq!(value["status"], "complete");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["discovered_files"], 1);
        assert_eq!(value["stored_files"], 1);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 1);
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["mining"], "deferred");
        assert!(workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .is_file());
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks/index.lock")
            .exists());
    }

    #[test]
    fn index_json_stores_framework_role_facts_without_query_claims() {
        let workspace = TempWorkspace::new("cli-index-framework-role-facts");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(
            workspace.path().join("component.tsx"),
            "export function UserCard() { return <section />; }\n",
        )
        .expect("write source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("index JSON");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["semantic_facts"], 1);
        assert_eq!(value["mining"], "deferred");

        for command in ["find", "families", "family", "member", "explain", "check"] {
            let output =
                run_with_context_and_runtime([command, "--json"], workspace.path(), &env, &runtime);
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            let unknown: Value =
                serde_json::from_str(output.stdout.trim()).expect("query UNKNOWN JSON");
            assert_eq!(unknown["status"], "UNKNOWN");
            assert_eq!(unknown["command"], command);
            assert_eq!(unknown["implemented"], true);
            assert_eq!(unknown["active_generation"], "gen-000001");
            assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");
        }
    }

    #[test]
    fn sync_json_rebuilds_a_new_file_manifest_generation() {
        let workspace = TempWorkspace::new("cli-sync-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace.path().join("b.tsx"),
            "export function B(){ return null; }\n",
        )
        .expect("write b");

        let output =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("sync JSON");
        assert_eq!(value["command"], "sync");
        assert_eq!(value["generation_id"], "gen-000002");
        assert_eq!(value["discovered_files"], 2);
        assert_eq!(value["stored_files"], 2);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 2);
        assert_eq!(
            fs::read_to_string(
                workspace
                    .path()
                    .join(DEFAULT_STATE_DIR)
                    .join("current-generation")
            )
            .expect("read current generation")
            .trim(),
            "gen-000002"
        );
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks/index.lock")
            .exists());
        assert_eq!(indexed_paths(&workspace, "gen-000001"), vec!["a.ts"]);
        assert_eq!(
            indexed_paths(&workspace, "gen-000002"),
            vec!["a.ts", "b.tsx"]
        );

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000002");
    }

    #[test]
    fn index_refuses_uninitialized_repository() {
        let workspace = TempWorkspace::new("cli-index-uninitialized");
        let env = |_: &str| None;
        let runtime = TestRuntime;

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("not initialized"));
    }

    #[test]
    fn index_refuses_corrupted_manifest_with_json() {
        let workspace = TempWorkspace::new("cli-index-corrupt-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            "broken",
        )
        .expect("corrupt manifest");

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("corrupted"));
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
    }

    #[test]
    fn index_refuses_missing_state_subdirectories_without_recreating_them() {
        let workspace = TempWorkspace::new("cli-index-missing-subdir");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let generations = workspace.path().join(DEFAULT_STATE_DIR).join("generations");
        fs::remove_dir_all(&generations).expect("remove generations");

        let output =
            run_with_context_and_runtime(["index", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "index");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("missing required subdirectories"));
        assert!(!generations.exists());
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
    }

    #[test]
    fn index_and_sync_refuse_live_index_lock() {
        let workspace = TempWorkspace::new("cli-index-live-lock");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let _guard = acquire_index_lock(workspace.path().to_string_lossy().as_ref(), None)
            .expect("hold index lock");

        for command in ["index", "sync"] {
            let output =
                run_with_context_and_runtime([command, "--json"], workspace.path(), &env, &runtime);

            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            assert!(!output
                .stderr
                .contains(workspace.path().to_string_lossy().as_ref()));
            let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
            assert_eq!(value["command"], command);
            assert!(value["reason"]
                .as_str()
                .expect("reason")
                .contains("index lock is held"));
        }
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
    }

    #[test]
    fn status_and_doctor_report_no_active_generation_before_index() {
        let workspace = TempWorkspace::new("cli-storage-no-active");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "not_implemented");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["indexing"], "not_implemented");
        assert!(value["checks"].get("schema_version").is_none());
        assert_eq!(value["checks"]["manifest_schema_version"], 1);
        assert_eq!(value["checks"]["storage_schema_version"], Value::Null);
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_NO_ACTIVE_GENERATION"));
    }

    #[test]
    fn doctor_json_reports_missing_subdir_and_storage_invalid() {
        let workspace = TempWorkspace::new("cli-doctor-missing-subdir-storage");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let generations = workspace.path().join(DEFAULT_STATE_DIR).join("generations");
        fs::remove_dir_all(&generations).expect("remove generations");

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["journal_mode"], Value::Null);
        assert_eq!(value["integrity_check"], Value::Null);
        assert_eq!(value["storage"], "unhealthy");
        assert!(value["storage_error"]
            .as_str()
            .expect("storage error")
            .contains("generations"));

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["required_subdirectories"], "fail");
        assert!(value["checks"].get("schema_version").is_none());
        assert_eq!(value["checks"]["manifest_schema_version"], 1);
        assert_eq!(value["checks"]["storage_schema_version"], Value::Null);
        assert_eq!(value["checks"]["storage"], "unhealthy");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "MISSING_SUBDIR"));
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_INVALID"));
        assert!(!generations.exists());
    }

    #[test]
    fn doctor_json_reports_lifecycle_hygiene_codes() {
        let workspace = TempWorkspace::new("cli-doctor-lifecycle-hygiene");
        let env = |_: &str| None;
        if !git_init(&workspace) {
            return;
        }
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        fs::write(state.join(".gitignore"), "bad\n").expect("write bad state gitignore");
        fs::write(state.join("receipts/init.json"), "{}\n").expect("write bad receipt");
        fs::write(
            workspace.path().join(".git/info/exclude"),
            ".repogrammar/\n",
        )
        .expect("write incomplete exclude");
        fs::write(
            workspace.path().join(".gitignore"),
            "# BEGIN RepoGrammar local state\n.repogrammar/\n",
        )
        .expect("write incomplete root marker");

        let doctor = run_with_context(["doctor", "--json"], workspace.path(), &env);

        assert_eq!(doctor.status, 0);
        assert!(!doctor
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["lifecycle_hygiene"], "fail");
        let codes = value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .map(|finding| finding["code"].as_str().expect("finding code"))
            .collect::<Vec<_>>();
        for code in [
            "STATE_GITIGNORE_INVALID",
            "INIT_RECEIPT_INVALID",
            "GIT_INFO_EXCLUDE_INCOMPLETE",
            "ROOT_GITIGNORE_MARKER_INVALID",
        ] {
            assert!(codes.contains(&code), "missing doctor code {code}");
        }
    }

    #[test]
    fn doctor_json_reports_active_index_lock() {
        let workspace = TempWorkspace::new("cli-doctor-index-lock");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let _guard = acquire_index_lock(workspace.path().to_string_lossy().as_ref(), None)
            .expect("hold index lock");

        let doctor = run_with_context(["doctor", "--json"], workspace.path(), &env);

        assert_eq!(doctor.status, 0);
        assert!(!doctor
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["locks"], "fail");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "INDEX_LOCK_ACTIVE"));
    }

    #[test]
    fn index_human_reports_syntax_only_without_family_claims() {
        let workspace = TempWorkspace::new("cli-index-human-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let output = run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("syntax-only code units stored"));
        assert!(output.stdout.contains("indexed_units: 1"));
        assert!(output.stdout.contains("indexing: syntax_only_code_units"));
        assert!(output.stdout.contains("parser: syntax_only"));
        assert!(output.stdout.contains("semantic_worker: deferred"));
        assert!(output.stdout.contains("mining: deferred"));
        assert!(!output.stdout.contains("DOMINANT_PATTERN"));
        assert!(!output.stdout.contains("CONFORMS"));
        assert!(!output.stdout.contains("pattern family"));
    }

    #[test]
    fn status_and_doctor_report_storage_health_after_index() {
        let workspace = TempWorkspace::new("cli-storage-health");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.js"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let human_status =
            run_with_context_and_runtime(["status"], workspace.path(), &env, &runtime);
        assert_eq!(human_status.status, 0);
        assert!(human_status.stdout.contains("manifest_schema_version: 1"));
        assert!(human_status
            .stdout
            .contains(&format!("storage_schema_version: {STORAGE_SCHEMA_VERSION}")));
        assert!(!human_status
            .stdout
            .lines()
            .any(|line| line.starts_with("schema_version:")));

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], STORAGE_SCHEMA_VERSION);
        assert_eq!(value["journal_mode"], "wal");
        assert_eq!(value["integrity_check"], "ok");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["indexing"], "syntax_only_code_units");
        assert!(value["checks"].get("schema_version").is_none());
        assert_eq!(value["checks"]["manifest_schema_version"], 1);
        assert_eq!(
            value["checks"]["storage_schema_version"],
            STORAGE_SCHEMA_VERSION
        );
        assert_eq!(value["checks"]["integrity_check"], "ok");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "INDEXING_SYNTAX_ONLY_CODE_UNITS"));
    }

    #[test]
    fn doctor_reports_broken_active_generation_pointer_without_panic() {
        let workspace = TempWorkspace::new("cli-storage-broken-pointer");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("current-generation"),
            "gen-999999\n",
        )
        .expect("break pointer");

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(doctor.status, 0);
        let doctor_value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(doctor_value["checks"]["storage"], "unhealthy");
        assert!(doctor_value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_INVALID"));

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert!(value.get("schema_version").is_none());
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["storage"], "unhealthy");

        let index =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(index.status, 2);
        let value: Value = serde_json::from_str(index.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "sync");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("storage is unhealthy"));
        assert_eq!(
            fs::read_to_string(
                workspace
                    .path()
                    .join(DEFAULT_STATE_DIR)
                    .join("current-generation")
            )
            .expect("read broken pointer")
            .trim(),
            "gen-999999"
        );
    }

    #[test]
    fn uninit_requires_yes_and_removes_state_only() {
        let workspace = TempWorkspace::new("cli-uninit");
        let env = |_: &str| None;
        fs::write(
            workspace.path().join("business.ts"),
            "export const x = 1;\n",
        )
        .expect("write business source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

        let missing_yes = run_with_context(["uninit"], workspace.path(), &env);
        assert_eq!(missing_yes.status, 2);
        assert!(missing_yes
            .stderr
            .contains("requires explicit confirmation"));

        let output = run_with_context(["uninit", "--yes", "--json"], workspace.path(), &env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("uninit JSON");
        assert_eq!(value["removed"], true);
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
        assert!(workspace.path().join("business.ts").is_file());
    }

    #[test]
    fn repogrammar_dir_override_is_used_and_unsafe_values_are_rejected() {
        let workspace = TempWorkspace::new("cli-env-state-dir");
        let safe_env =
            |key: &str| (key == "REPOGRAMMAR_DIR").then(|| ".repogrammar-linux".to_string());

        let output = run_with_context(["init", "--json"], workspace.path(), &safe_env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["state_dir"], ".repogrammar-linux");
        assert!(workspace.path().join(".repogrammar-linux").is_dir());

        let unsafe_env = |key: &str| (key == "REPOGRAMMAR_DIR").then(|| "../outside".to_string());
        let error = run_with_context(["status", "--json"], workspace.path(), &unsafe_env);
        assert_eq!(error.status, 2);
        assert!(error.stderr.contains("repository state directory override"));
    }

    #[test]
    fn unlock_does_not_blind_delete_locks() {
        let workspace = TempWorkspace::new("cli-unlock");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks/index.lock");
        fs::write(&lock_path, "{}").expect("write invalid lock");

        let output = run_with_context(["unlock", "--json"], workspace.path(), &env);
        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("unlock JSON");
        assert_eq!(value["removed_locks"], 0);
        assert_eq!(value["inspected_locks"][0], "index.lock");
        assert!(lock_path.exists());
        assert!(value["message"]
            .as_str()
            .expect("unlock message")
            .contains("inspected"));

        let force = run_with_context(["unlock", "--force"], workspace.path(), &env);
        assert_eq!(force.status, 2);
        assert!(force.stderr.contains("--force requires --yes"));

        let invalid = run_with_context(
            ["unlock", "--force", "--yes", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(invalid.status, 0);
        let value: Value = serde_json::from_str(invalid.stdout.trim()).expect("unlock JSON");
        assert_eq!(value["removed_locks"], 0);
        assert!(lock_path.exists());
        assert!(value["message"]
            .as_str()
            .expect("unlock message")
            .contains("invalid"));
        assert!(!invalid.stdout.contains("not implemented"));
        assert!(!invalid
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let locks_dir = workspace.path().join(DEFAULT_STATE_DIR).join("locks");
        fs::write(&lock_path, stale_index_lock_json("cli-stale")).expect("write stale lock");
        fs::write(locks_dir.join("daemon.lock"), b"daemon").expect("write daemon lock");
        fs::write(locks_dir.join("sqlite.lock"), b"sqlite").expect("write sqlite lock");

        let stale = run_with_context(
            ["unlock", "--force", "--yes", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(stale.status, 0);
        let value: Value = serde_json::from_str(stale.stdout.trim()).expect("unlock JSON");
        assert_eq!(value["removed_locks"], 1);
        assert!(!lock_path.exists());
        assert!(locks_dir.join("daemon.lock").exists());
        assert!(locks_dir.join("sqlite.lock").exists());
        assert!(value["message"]
            .as_str()
            .expect("unlock message")
            .contains("confirmed stale index lock"));
        assert!(!stale.stdout.contains("not implemented"));
        assert!(!stale
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn logs_json_is_redacted_metadata_and_omits_absolute_paths() {
        let workspace = TempWorkspace::new("cli-logs");
        let env = |_: &str| None;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("logs/index.log"),
            format!("absolute path would be {}\n", workspace.path().display()),
        )
        .expect("write log");

        let output = run_with_context(
            [
                "logs",
                "--json",
                "--component",
                "index",
                "--since",
                "1h",
                "--tail",
                "20",
                "--redact",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("logs JSON");
        assert_eq!(value["command"], "logs");
        assert_eq!(value["paths"], "repo_relative_only");
        assert_eq!(value["redacted"], true);
        assert_eq!(value["component_filter"], "index");
        assert!(value["entries"].as_array().expect("entries").is_empty());
    }

    #[test]
    fn unknown_lifecycle_options_are_rejected() {
        let status = run(["status", "--write-gitignore"]);
        assert_eq!(status.status, 2);
        assert!(status
            .stderr
            .contains("unknown status option: --write-gitignore"));

        let logs = run(["logs", "--mystery"]);
        assert_eq!(logs.status, 2);
        assert!(logs.stderr.contains("unknown logs option: --mystery"));
    }

    #[test]
    fn install_dry_run_accepts_required_flags() {
        let output = run([
            "install",
            "--target",
            "codex",
            "--scope",
            "project",
            "--dry-run",
            "--yes",
            "--print-config",
            "--no-telemetry",
            "--no-permissions",
        ]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("target=codex"));
        assert!(output.stdout.contains("scope=project-local"));
        assert!(output.stdout.contains("telemetry=off"));
        assert!(output
            .stdout
            .contains("native_mcp: deferred codex project-local"));
        assert!(output.stdout.contains("config preview:"));
        assert!(output.stdout.contains("MCP self-test"));
        assert!(output.stdout.contains("reversible receipt"));
    }

    #[test]
    fn install_dry_run_reports_native_codex_and_claude_plans() {
        let codex = run([
            "install",
            "--target",
            "codex",
            "--scope",
            "global",
            "--dry-run",
            "--no-telemetry",
        ]);
        assert_eq!(codex.status, 0);
        assert!(codex
            .stdout
            .contains("native_mcp: codex mcp add repogrammar -- <repogrammar-executable> serve"));

        let claude = run([
            "install",
            "--target",
            "claude",
            "--scope",
            "global",
            "--dry-run",
            "--no-telemetry",
        ]);
        assert_eq!(claude.status, 0);
        assert!(claude.stdout.contains(
            "native_mcp: claude mcp add --scope user repogrammar -- <repogrammar-executable> serve"
        ));
    }

    #[test]
    fn install_dry_run_does_not_create_state_receipts_or_delegate_writes() {
        struct DryRunRuntime {
            delegated: std::cell::Cell<bool>,
        }

        impl CliRuntime for DryRunRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer dry-run test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer dry-run test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer dry-run test")
            }

            fn install_agent_integration(
                &self,
                _command: &str,
                _request: InstallRequest,
                _context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                self.delegated.set(true);
                Err(RepoGrammarError::InvalidInput(
                    "dry-run must not delegate native writes".to_string(),
                ))
            }
        }

        let workspace = TempWorkspace::new("cli-install-dry-run-no-side-effects");
        let install_dir = workspace.path().join("install-data");
        let env = |key: &str| {
            if key == "REPOGRAMMAR_INSTALL_DIR" {
                Some(install_dir.display().to_string())
            } else {
                None
            }
        };
        let runtime = DryRunRuntime {
            delegated: std::cell::Cell::new(false),
        };

        let output = run_with_context_and_runtime(
            [
                "install",
                "--target",
                "codex",
                "--scope",
                "project",
                "--dry-run",
                "--yes",
                "--print-config",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("install dry-run"));
        assert!(!runtime.delegated.get());
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
        assert!(!install_dir.exists());
        assert!(!install_dir.join("receipts").exists());
        for instruction_file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
            assert!(!workspace.path().join(instruction_file).exists());
        }
    }

    #[test]
    fn install_print_config_is_no_write_even_without_dry_run_or_home() {
        struct PrintConfigRuntime {
            delegated: std::cell::Cell<bool>,
        }

        impl CliRuntime for PrintConfigRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer print-config test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer print-config test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer print-config test")
            }

            fn install_agent_integration(
                &self,
                _command: &str,
                _request: InstallRequest,
                _context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                self.delegated.set(true);
                unreachable!("print-config must not delegate native writes")
            }
        }

        let workspace = TempWorkspace::new("cli-install-print-config-no-write");
        let install_dir = workspace.path().join("install-data");
        let runtime = PrintConfigRuntime {
            delegated: std::cell::Cell::new(false),
        };
        let env = |key: &str| {
            if key == "REPOGRAMMAR_INSTALL_DIR" {
                Some(install_dir.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context_and_runtime(
            ["install", "--print-config", "cursor", "--location", "local"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stdout.contains("target=cursor"));
        assert!(output.stdout.contains("./.cursor/mcp.json"));
        assert!(output.stdout.contains("--path"));
        assert!(!runtime.delegated.get());
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
        assert!(!install_dir.exists());
        for instruction_file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
            assert!(!workspace.path().join(instruction_file).exists());
        }
    }

    #[test]
    fn install_live_writes_require_yes_before_runtime_delegation() {
        let output = run(["install", "--target", "codex"]);

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("interactive install requires a terminal"));
        assert!(!output.stderr.contains("not implemented"));
    }

    #[test]
    fn interactive_install_wizard_selects_multiple_agents_and_defaults_telemetry_off() {
        #[derive(Default)]
        struct InstallRuntime {
            requests: RefCell<Vec<InstallRequest>>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer wizard test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer wizard test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer wizard test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                self.requests.borrow_mut().push(request.clone());
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: request.selected_targets.clone(),
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-wizard");
        let command_dir = workspace.path().join("commands");
        fs::create_dir_all(&command_dir).expect("command dir");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime::default();
        let prompt = WizardPrompt::new(["2,1"], [""], ["y"]);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stdout.contains("Plan:"));
        assert!(output
            .stdout
            .contains("configured_targets=codex,claude-code"));
        assert!(output.stdout.contains("telemetry=off"));
        let requests = runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].assume_yes);
        assert_eq!(
            requests[0].selected_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert_eq!(prompt.selection_calls.get(), 1);
        assert_eq!(prompt.telemetry_calls.get(), 1);
        assert_eq!(prompt.confirmation_calls.get(), 1);
    }

    #[test]
    fn interactive_install_reprompts_invalid_agent_selection_before_telemetry() {
        #[derive(Default)]
        struct InstallRuntime {
            requests: RefCell<Vec<InstallRequest>>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer invalid selection test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer invalid selection test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer invalid selection test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                self.requests.borrow_mut().push(request.clone());
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: request.selected_targets.clone(),
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-invalid-selection-reprompt");
        let data_home = workspace.path().join("data-home");
        let command_dir = workspace.path().join("commands");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime::default();
        let prompt = WizardPrompt::new(["1a", "1"], [""], ["y"]);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(
            runtime.requests.borrow()[0].selected_targets,
            vec![AgentTarget::Codex]
        );
        assert_eq!(prompt.selection_calls.get(), 2);
        assert_eq!(prompt.telemetry_calls.get(), 1);
        assert_eq!(prompt.confirmation_calls.get(), 1);
        let prompts = prompt.selection_prompts.borrow();
        assert_eq!(prompts.len(), 2);
        assert!(!prompts[0].contains("Invalid selection"));
        assert!(prompts[1].contains("Invalid selection:"));
        assert!(prompts[1].contains("unknown agent selection"));
    }

    #[test]
    fn interactive_install_with_existing_receipts_still_repairs_command_path() {
        #[derive(Default)]
        struct InstallRuntime {
            requests: RefCell<Vec<InstallRequest>>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer existing receipt test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer existing receipt test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer existing receipt test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                self.requests.borrow_mut().push(request.clone());
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: Vec::new(),
                    skipped_targets: request.selected_targets.clone(),
                    receipt_paths: Vec::new(),
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "selected agent MCP integrations are already managed by RepoGrammar"
                        .to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-existing-receipts-repair-command");
        let command_dir = workspace.path().join("commands");
        fs::create_dir_all(&command_dir).expect("command dir");
        let data_home = workspace.path().join("data-home");
        let receipt_dir = data_home
            .join("repogrammar")
            .join("install")
            .join("receipts");
        fs::create_dir_all(&receipt_dir).expect("receipt dir");
        for target in ["codex", "claude-code"] {
            fs::write(
                receipt_dir.join(format!("{target}-global.json")),
                format!(
                    r#"{{"schema_version":1,"managed_by":"repogrammar","mcp_server":"repogrammar","target":"{target}","scope":"global"}}"#
                ),
            )
            .expect("receipt");
        }
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime::default();
        let prompt = WizardPrompt::new([""], [""], ["y"]);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stdout.contains("skipped_targets=codex,claude-code"));
        let requests = runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].selected_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert!(requests[0].assume_yes);
        assert_eq!(prompt.selection_calls.get(), 1);
        assert_eq!(prompt.confirmation_calls.get(), 1);
    }

    #[test]
    fn interactive_install_cancel_stops_before_runtime_writes() {
        struct InstallRuntime {
            delegated: Cell<bool>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer wizard cancel test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer wizard cancel test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer wizard cancel test")
            }

            fn install_agent_integration(
                &self,
                _command: &str,
                _request: InstallRequest,
                _context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                self.delegated.set(true);
                unreachable!("cancel must stop before native writes")
            }
        }

        let workspace = TempWorkspace::new("cli-install-wizard-cancel");
        let data_home = workspace.path().join("data-home");
        let command_dir = workspace.path().join("commands");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime {
            delegated: Cell::new(false),
        };
        let prompt = WizardPrompt::new(["q"], [], []);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("install cancelled"));
        assert!(!runtime.delegated.get());
    }

    #[test]
    fn interactive_agent_selection_normalizes_and_defaults() {
        let statuses = vec![
            InstallAgentStatus {
                target: AgentTarget::Codex,
                detected: true,
                installed: false,
            },
            InstallAgentStatus {
                target: AgentTarget::ClaudeCode,
                detected: true,
                installed: false,
            },
        ];

        assert_eq!(
            parse_interactive_agent_selection("1", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex])
        );
        assert_eq!(
            parse_interactive_agent_selection("2", &statuses).expect("selection"),
            Some(vec![AgentTarget::ClaudeCode])
        );
        assert_eq!(
            parse_interactive_agent_selection("2,1", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex, AgentTarget::ClaudeCode])
        );
        assert_eq!(
            parse_interactive_agent_selection("all", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex, AgentTarget::ClaudeCode])
        );
        assert_eq!(
            parse_interactive_agent_selection("1,1", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex])
        );
        assert_eq!(
            parse_interactive_agent_selection("", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex, AgentTarget::ClaudeCode])
        );
        assert_eq!(
            parse_interactive_agent_selection("q", &statuses).expect("selection"),
            None
        );
        assert!(parse_interactive_agent_selection("unknown", &statuses).is_err());
        assert!(parse_interactive_agent_selection("1a", &statuses).is_err());
        assert!(parse_interactive_agent_selection("1,,2", &statuses).is_err());
    }

    #[test]
    fn install_target_option_accepts_codegraph_style_values() {
        let auto = parse_install_options(&["--target".to_string(), "auto".to_string()])
            .expect("auto target");
        assert_eq!(auto.target, AgentTarget::AllSupported);
        assert!(auto.selected_targets.is_empty());

        let none = parse_install_options(&[
            "--target".to_string(),
            "none".to_string(),
            "--dry-run".to_string(),
        ])
        .expect("none target");
        assert_eq!(none.target, AgentTarget::None);
        assert!(none.selected_targets.is_empty());

        let csv = parse_install_options(&[
            "--target".to_string(),
            "claude-code,codex,codex".to_string(),
        ])
        .expect("csv target");
        assert_eq!(csv.target, AgentTarget::AllSupported);
        assert_eq!(
            csv.selected_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );

        let deferred = parse_install_options(&[
            "--target".to_string(),
            "cursor,gemini".to_string(),
            "--location".to_string(),
            "local".to_string(),
        ])
        .expect("deferred csv target");
        assert_eq!(deferred.target, AgentTarget::Cursor);
        assert_eq!(
            deferred.selected_targets,
            vec![AgentTarget::Cursor, AgentTarget::Gemini]
        );
        assert_eq!(deferred.scope, InstallScope::ProjectLocal);

        assert!(parse_install_options(
            &["--target".to_string(), "codex,,claude-code".to_string(),]
        )
        .is_err());
        assert!(
            parse_install_options(&["--target".to_string(), "codex,none".to_string(),]).is_err()
        );
    }

    #[test]
    fn install_live_writes_delegate_to_runtime_after_yes() {
        struct InstallRuntime;

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                assert_eq!(request.target, AgentTarget::Codex);
                assert_eq!(request.scope, InstallScope::Global);
                assert!(context.data_dir.ends_with("repogrammar"));
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: vec![AgentTarget::Codex],
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-runtime");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context_and_runtime(
            ["install", "--target", "codex", "--yes"],
            workspace.path(),
            &env,
            &InstallRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output
            .stdout
            .contains("install: agent MCP integration installed"));
        assert!(output.stdout.contains("configured_targets=codex"));
    }

    #[test]
    fn install_all_yes_is_noninteractive_and_delegates_safely() {
        #[derive(Default)]
        struct InstallRuntime {
            requests: RefCell<Vec<InstallRequest>>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer all target test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer all target test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer all target test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                self.requests.borrow_mut().push(request.clone());
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: vec![AgentTarget::Codex, AgentTarget::ClaudeCode],
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-all-yes");
        let data_home = workspace.path().join("data-home");
        let command_dir = workspace.path().join("commands");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime::default();
        let prompt = WizardPrompt::new([], [], []);

        let output = run_with_context_runtime_prompt(
            ["install", "--target", "all", "--yes", "--no-telemetry"],
            workspace.path(),
            &env,
            &runtime,
            &prompt,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(prompt.selection_calls.get(), 0);
        assert_eq!(prompt.telemetry_calls.get(), 0);
        assert_eq!(prompt.confirmation_calls.get(), 0);
        let requests = runtime.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, AgentTarget::AllSupported);
        assert!(!requests[0].telemetry_enabled);
        assert!(output
            .stdout
            .contains("configured_targets=codex,claude-code"));
    }

    #[test]
    fn install_yes_alone_persists_telemetry_disabled() {
        struct InstallRuntime;

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                assert!(!request.telemetry_enabled);
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: vec![request.target],
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-telemetry-default-off");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context_and_runtime(
            ["install", "--target", "codex", "--yes"],
            workspace.path(),
            &env,
            &InstallRuntime,
        );
        let status = run_with_context(["telemetry", "status", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("telemetry=off"));
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["enabled"], false);
    }

    #[test]
    fn install_telemetry_flag_persists_consent_unless_env_disabled() {
        struct InstallRuntime;

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: vec![request.target],
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-install-telemetry-on");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context_and_runtime(
            ["install", "--target", "codex", "--yes", "--telemetry"],
            workspace.path(),
            &env,
            &InstallRuntime,
        );
        let status = run_with_context(["telemetry", "status", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("telemetry=on"));
        assert!(output.stdout.contains("effective_telemetry=on"));
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["enabled"], true);

        for disabled_key in ["DO_NOT_TRACK", "REPOGRAMMAR_TELEMETRY", "CI"] {
            let disabled_home = workspace
                .path()
                .join(format!("disabled-data-home-{disabled_key}"));
            let disabled_env = |key: &str| {
                if key == "XDG_DATA_HOME" {
                    Some(disabled_home.display().to_string())
                } else if key == disabled_key {
                    Some(if disabled_key == "REPOGRAMMAR_TELEMETRY" {
                        "0".to_string()
                    } else {
                        "1".to_string()
                    })
                } else {
                    None
                }
            };
            let disabled = run_with_context_and_runtime(
                ["install", "--target", "codex", "--yes", "--telemetry"],
                workspace.path(),
                &disabled_env,
                &InstallRuntime,
            );

            assert_eq!(disabled.status, 0);
            assert!(disabled.stdout.contains("telemetry=off"));
            assert!(disabled.stdout.contains("disabled_by_environment=true"));
        }
    }

    #[test]
    fn install_yes_without_telemetry_flags_does_not_prompt_and_persists_disabled() {
        #[derive(Default)]
        struct InstallRuntime {
            seen_telemetry: std::cell::RefCell<Vec<bool>>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn install_agent_integration(
                &self,
                command: &str,
                request: InstallRequest,
                context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                assert_eq!(command, "install");
                self.seen_telemetry
                    .borrow_mut()
                    .push(request.telemetry_enabled);
                Ok(InstallExecutionOutcome {
                    command: "install",
                    target: request.target,
                    scope: request.scope,
                    configured_targets: vec![request.target],
                    skipped_targets: Vec::new(),
                    receipt_paths: vec![context.data_dir],
                    installed_executable_path: Some(context.executable_path),
                    command_path: Some(context.command_dir),
                    command_on_path: context.command_dir_on_path,
                    message: "agent MCP integration installed after self-test".to_string(),
                })
            }
        }

        struct Prompt;

        impl InstallTelemetryPrompt for Prompt {}

        let workspace = TempWorkspace::new("cli-install-yes-no-prompt");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = InstallRuntime::default();
        let output = run_with_context_runtime_prompt(
            ["install", "--target", "codex", "--yes"],
            workspace.path(),
            &env,
            &runtime,
            &Prompt,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.seen_telemetry.borrow().as_slice(), &[false]);
        assert!(output.stdout.contains("telemetry=off"));

        let status = run_with_context(["telemetry", "status", "--json"], workspace.path(), &env);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["enabled"], false);
    }

    #[test]
    fn install_without_yes_refuses_before_prompt_or_writes() {
        struct InstallRuntime {
            delegated: std::cell::Cell<bool>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer prompt test")
            }

            fn install_agent_integration(
                &self,
                _command: &str,
                _request: InstallRequest,
                _context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                self.delegated.set(true);
                unreachable!("invalid prompt response must stop before native writes")
            }
        }

        struct Prompt;

        impl InstallTelemetryPrompt for Prompt {}

        let workspace = TempWorkspace::new("cli-install-no-yes-no-prompt");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = InstallRuntime {
            delegated: std::cell::Cell::new(false),
        };
        let output = run_with_context_runtime_prompt(
            ["install", "--target", "codex"],
            workspace.path(),
            &env,
            &runtime,
            &Prompt,
        );

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("interactive install requires a terminal"));
        assert!(!runtime.delegated.get());
    }

    struct WizardPrompt {
        selections: RefCell<VecDeque<String>>,
        selection_prompts: RefCell<Vec<String>>,
        telemetry: RefCell<VecDeque<String>>,
        confirmations: RefCell<VecDeque<String>>,
        selection_calls: Cell<usize>,
        telemetry_calls: Cell<usize>,
        confirmation_calls: Cell<usize>,
    }

    impl WizardPrompt {
        fn new<const S: usize, const T: usize, const C: usize>(
            selections: [&str; S],
            telemetry: [&str; T],
            confirmations: [&str; C],
        ) -> Self {
            Self {
                selections: RefCell::new(selections.into_iter().map(str::to_string).collect()),
                selection_prompts: RefCell::new(Vec::new()),
                telemetry: RefCell::new(telemetry.into_iter().map(str::to_string).collect()),
                confirmations: RefCell::new(
                    confirmations.into_iter().map(str::to_string).collect(),
                ),
                selection_calls: Cell::new(0),
                telemetry_calls: Cell::new(0),
                confirmation_calls: Cell::new(0),
            }
        }
    }

    impl InstallTelemetryPrompt for WizardPrompt {
        fn is_interactive(&self) -> bool {
            true
        }

        fn prompt_agent_selection(&self, prompt: &str) -> Result<String, String> {
            self.selection_calls.set(self.selection_calls.get() + 1);
            self.selection_prompts.borrow_mut().push(prompt.to_string());
            self.selections
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "missing fake selection".to_string())
        }

        fn prompt_install_telemetry_consent(&self, _prompt: &str) -> Result<String, String> {
            self.telemetry_calls.set(self.telemetry_calls.get() + 1);
            self.telemetry
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "missing fake telemetry response".to_string())
        }

        fn prompt_install_confirmation(&self, _prompt: &str) -> Result<String, String> {
            self.confirmation_calls
                .set(self.confirmation_calls.get() + 1);
            self.confirmations
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "missing fake confirmation".to_string())
        }
    }

    #[test]
    fn status_doctor_stats_and_telemetry_status_are_safe() {
        let workspace = TempWorkspace::new("cli-safe-status");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        assert_eq!(
            run_with_context(["status"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context(["doctor"], workspace.path(), &env).status,
            0
        );
        let stats = run_with_context(["stats"], workspace.path(), &env);
        assert_eq!(stats.status, 2);
        assert!(stats.stdout.is_empty());
        assert!(stats.stderr.contains("FALLBACK_TO_CODE_SEARCH"));
        assert!(stats.stderr.contains("repository is not initialized"));
        assert_eq!(
            run_with_context(["telemetry", "status"], workspace.path(), &env).status,
            0
        );
    }

    #[test]
    fn telemetry_status_reports_anonymous_and_research_consent_separately() {
        let workspace = TempWorkspace::new("cli-telemetry-consent");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let anonymous = run_with_context(["telemetry", "on", "--json"], workspace.path(), &env);
        assert_eq!(anonymous.status, 0);
        let value: Value = serde_json::from_str(anonymous.stdout.trim()).expect("telemetry JSON");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["research_enabled"], false);

        let research = run_with_context(
            ["telemetry", "research-on", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(research.status, 0);
        let value: Value = serde_json::from_str(research.stdout.trim()).expect("telemetry JSON");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["research_enabled"], true);
        assert!(!research
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn telemetry_env_opt_out_reports_upload_disabled() {
        let workspace = TempWorkspace::new("cli-telemetry-env");
        let data_home = workspace.path().join("data-home");
        let env_on = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        assert_eq!(
            run_with_context(["telemetry", "on"], workspace.path(), &env_on).status,
            0
        );
        for (disabled_key, disabled_by_ci) in [
            ("DO_NOT_TRACK", false),
            ("REPOGRAMMAR_TELEMETRY", false),
            ("CI", true),
        ] {
            let env_disabled = |key: &str| {
                if key == "XDG_DATA_HOME" {
                    Some(data_home.display().to_string())
                } else if key == disabled_key {
                    Some(if disabled_key == "REPOGRAMMAR_TELEMETRY" {
                        "0".to_string()
                    } else {
                        "1".to_string()
                    })
                } else {
                    None
                }
            };

            let status = run_with_context(
                [
                    "telemetry",
                    "status",
                    "--json",
                    "--endpoint",
                    "https://telemetry.example.invalid/v1",
                ],
                workspace.path(),
                &env_disabled,
            );

            assert_eq!(status.status, 0);
            let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
            assert_eq!(value["enabled"], true);
            assert_eq!(value["disabled_by_environment"], true);
            assert_eq!(value["disabled_by_ci"], disabled_by_ci);
            assert_eq!(value["effective_enabled"], false);
            assert_eq!(value["network_upload_configured"], true);
            assert_eq!(value["upload_would_open_network_connection"], false);
        }
    }

    #[test]
    fn telemetry_off_purge_research_and_no_endpoint_paths_are_safe() {
        struct UploadRuntime {
            calls: std::cell::Cell<usize>,
        }

        impl CliRuntime for UploadRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                Ok(FamilyQueryRuntime::status_report())
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repo_shape_diagnostics(
                &self,
                request: RepositoryStatusRequest,
            ) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
                FamilyQueryRuntime.repo_shape_diagnostics(request)
            }

            fn upload_telemetry_payload(
                &self,
                _endpoint: &str,
                _payload: &str,
                _timeout: std::time::Duration,
            ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
                self.calls.set(self.calls.get() + 1);
                Ok(TelemetryUploadReceipt {
                    status_code: 204,
                    receipt_id: "test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-telemetry-safe-paths");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = UploadRuntime {
            calls: std::cell::Cell::new(0),
        };

        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context(["telemetry", "on"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context(["telemetry", "research-on"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["stats", "--json"], workspace.path(), &env, &runtime)
                .status,
            0
        );

        let no_endpoint = run_with_context_and_runtime(
            ["telemetry", "upload", "--json", "--yes"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(no_endpoint.status, 0);
        assert_eq!(runtime.calls.get(), 0);
        let value: Value = serde_json::from_str(no_endpoint.stdout.trim()).expect("upload JSON");
        assert_eq!(value["network_upload_configured"], false);
        assert_eq!(
            value["reason"],
            "telemetry upload endpoint is not configured"
        );

        let research_export = run_with_context(
            ["telemetry", "research-export", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(research_export.status, 0);
        let value: Value =
            serde_json::from_str(research_export.stdout.trim()).expect("research export JSON");
        assert_eq!(value["payload"]["research_enabled"], true);
        assert_eq!(value["payload"]["source_snippets_included"], false);

        let off = run_with_context(["telemetry", "off", "--json"], workspace.path(), &env);
        assert_eq!(off.status, 0);
        let value: Value = serde_json::from_str(off.stdout.trim()).expect("off JSON");
        assert_eq!(value["enabled"], false);

        let purge = run_with_context(
            ["telemetry", "purge", "--json", "--yes"],
            workspace.path(),
            &env,
        );
        assert_eq!(purge.status, 0);
        let value: Value = serde_json::from_str(purge.stdout.trim()).expect("purge JSON");
        assert!(value["removed_files"].as_u64().expect("removed files") >= 1);

        let research_purge = run_with_context(
            ["telemetry", "research-purge", "--json", "--yes"],
            workspace.path(),
            &env,
        );
        assert_eq!(research_purge.status, 0);
    }

    #[test]
    fn telemetry_env_disabled_upload_does_not_call_transport() {
        struct UploadRuntime {
            calls: std::cell::Cell<usize>,
        }

        impl CliRuntime for UploadRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                Ok(FamilyQueryRuntime::status_report())
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn upload_telemetry_payload(
                &self,
                _endpoint: &str,
                _payload: &str,
                _timeout: std::time::Duration,
            ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
                self.calls.set(self.calls.get() + 1);
                Ok(TelemetryUploadReceipt {
                    status_code: 204,
                    receipt_id: "test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-telemetry-env-disabled-upload");
        let data_home = workspace.path().join("data-home");
        let env_on = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        assert_eq!(
            run_with_context(["telemetry", "on"], workspace.path(), &env_on).status,
            0
        );
        let env_disabled = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_TELEMETRY" => Some("0".to_string()),
            _ => None,
        };
        let runtime = UploadRuntime {
            calls: std::cell::Cell::new(0),
        };

        let output = run_with_context_and_runtime(
            [
                "telemetry",
                "upload",
                "--json",
                "--endpoint",
                "https://telemetry.example.invalid/v1",
                "--yes",
            ],
            workspace.path(),
            &env_disabled,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert_eq!(runtime.calls.get(), 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("upload JSON");
        assert_eq!(value["reason"], "telemetry disabled by environment");
    }

    #[test]
    fn telemetry_disabled_upload_does_not_call_transport() {
        struct UploadRuntime {
            calls: std::cell::Cell<usize>,
        }

        impl CliRuntime for UploadRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                Ok(FamilyQueryRuntime::status_report())
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn upload_telemetry_payload(
                &self,
                _endpoint: &str,
                _payload: &str,
                _timeout: std::time::Duration,
            ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
                self.calls.set(self.calls.get() + 1);
                Ok(TelemetryUploadReceipt {
                    status_code: 204,
                    receipt_id: "test".to_string(),
                })
            }
        }

        let workspace = TempWorkspace::new("cli-telemetry-disabled-upload");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let runtime = UploadRuntime {
            calls: std::cell::Cell::new(0),
        };

        let output = run_with_context_and_runtime(
            [
                "telemetry",
                "upload",
                "--json",
                "--endpoint",
                "https://telemetry.example.invalid/v1",
                "--yes",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert_eq!(runtime.calls.get(), 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("upload JSON");
        assert_eq!(value["status"], "not_uploaded");
        assert_eq!(value["reason"], "anonymous telemetry is disabled");
    }

    #[test]
    fn telemetry_upload_dry_run_exports_allowlisted_payload_without_network() {
        struct UploadRuntime;

        impl CliRuntime for UploadRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                Ok(FamilyQueryRuntime::status_report())
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("telemetry upload test")
            }

            fn repo_shape_diagnostics(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
                FamilyQueryRuntime.repo_shape_diagnostics(_request)
            }

            fn upload_telemetry_payload(
                &self,
                _endpoint: &str,
                _payload: &str,
                _timeout: std::time::Duration,
            ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
                panic!("dry-run upload must not call transport")
            }
        }

        let workspace = TempWorkspace::new("cli-telemetry-upload-dry-run");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        assert_eq!(
            run_with_context(["telemetry", "on"], workspace.path(), &env).status,
            0
        );

        let output = run_with_context_and_runtime(
            [
                "telemetry",
                "upload",
                "--json",
                "--dry-run",
                "--endpoint",
                "https://telemetry.example.invalid/v1",
            ],
            workspace.path(),
            &env,
            &UploadRuntime,
        );

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("upload JSON");
        assert_eq!(value["status"], "not_uploaded");
        assert_eq!(value["payload"]["schema_version"], "telemetry.v1");
        assert_eq!(value["payload"]["source_snippets_returned"], false);
        let serialized = output.stdout;
        assert!(!serialized.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("query_text"));
        assert!(!serialized.contains("raw_error"));
    }

    #[test]
    fn telemetry_experiment_pair_makes_stats_report_measured_savings() {
        let workspace = TempWorkspace::new("cli-token-experiment");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        for args in [
            vec![
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--experiment-mode",
                "record-existing",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
                "--yes",
            ],
            vec![
                "telemetry",
                "experiment-record",
                "--name",
                "task-a",
                "--input-tokens",
                "100",
                "--output-tokens",
                "40",
                "--tool-tokens",
                "10",
                "--success",
                "true",
            ],
            vec!["telemetry", "experiment-stop", "--name", "task-a"],
            vec![
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--experiment-mode",
                "record-existing",
                "--session",
                "treatment",
                "--measurement-source",
                "user_entered",
                "--read-plan-used",
                "true",
                "--read-plan-item-count",
                "2",
                "--yes",
            ],
            vec![
                "telemetry",
                "experiment-record",
                "--name",
                "task-a",
                "--input-tokens",
                "70",
                "--output-tokens",
                "30",
                "--tool-tokens",
                "5",
                "--success",
                "true",
            ],
            vec!["telemetry", "experiment-stop", "--name", "task-a"],
        ] {
            let output = run_with_context(args, workspace.path(), &env);
            assert_eq!(output.status, 0, "{:?}", output);
        }

        let stats = run_with_context_and_runtime(
            ["stats", "--json"],
            workspace.path(),
            &env,
            &FamilyQueryRuntime,
        );

        assert_eq!(stats.status, 0);
        let value: Value = serde_json::from_str(stats.stdout.trim()).expect("stats JSON");
        assert_eq!(value["token_savings"], 45);
        assert_eq!(value["measurement_source"], "user_entered");
        assert_eq!(value["measurement_status"], "paired_measurement_available");
        assert_eq!(value["claim_validity"], "valid_for_product_claim");
        assert!(!stats
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn telemetry_experiment_start_requires_explicit_confirmation() {
        let workspace = TempWorkspace::new("cli-token-experiment-confirmation");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context(
            [
                "telemetry",
                "experiment-start",
                "--json",
                "--name",
                "task-a",
                "--experiment-mode",
                "record-existing",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 2);
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(
            value["reason"],
            "experiment recording requires explicit confirmation"
        );
    }

    #[test]
    fn telemetry_record_existing_prompt_defaults_no_and_accepts_yes() {
        struct Prompt {
            response: &'static str,
            seen_prompt: std::cell::RefCell<String>,
        }

        impl InstallTelemetryPrompt for Prompt {
            fn prompt_experiment_consent(&self, prompt: &str) -> Result<String, String> {
                self.seen_prompt.replace(prompt.to_string());
                Ok(self.response.to_string())
            }
        }

        for (response, expected_status) in [("", 2), ("no", 2), ("yes", 0)] {
            let workspace = TempWorkspace::new(&format!("cli-record-existing-prompt-{response}"));
            let data_home = workspace.path().join("data-home");
            let env = |key: &str| {
                if key == "XDG_DATA_HOME" {
                    Some(data_home.display().to_string())
                } else {
                    None
                }
            };
            let prompt = Prompt {
                response,
                seen_prompt: std::cell::RefCell::new(String::new()),
            };

            let output = run_with_context_runtime_prompt(
                [
                    "telemetry",
                    "experiment-start",
                    "--json",
                    "--name",
                    "task-a",
                    "--experiment-mode",
                    "record-existing",
                    "--session",
                    "baseline",
                    "--measurement-source",
                    "user_entered",
                ],
                workspace.path(),
                &env,
                &FamilyQueryRuntime,
                &prompt,
            );

            assert_eq!(output.status, expected_status, "{output:?}");
            let prompt_text = prompt.seen_prompt.borrow();
            assert!(prompt_text.contains("sessions you already performed"));
            assert!(prompt_text.contains("should not increase token usage"));
            assert!(prompt_text.contains("[y/N]"));
            let experiment_file = data_home
                .join("repogrammar")
                .join("experiments")
                .join("task-a.json");
            assert_eq!(experiment_file.exists(), response == "yes");
        }
    }

    #[test]
    fn telemetry_controlled_pair_prompt_warns_about_usage_cost() {
        struct Prompt {
            seen_prompt: std::cell::RefCell<String>,
        }

        impl InstallTelemetryPrompt for Prompt {
            fn prompt_experiment_consent(&self, prompt: &str) -> Result<String, String> {
                self.seen_prompt.replace(prompt.to_string());
                Ok("yes".to_string())
            }
        }

        let workspace = TempWorkspace::new("cli-controlled-pair-prompt");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let prompt = Prompt {
            seen_prompt: std::cell::RefCell::new(String::new()),
        };

        let output = run_with_context_runtime_prompt(
            [
                "telemetry",
                "experiment-start",
                "--json",
                "--name",
                "task-a",
                "--experiment-mode",
                "controlled-pair",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
            ],
            workspace.path(),
            &env,
            &FamilyQueryRuntime,
            &prompt,
        );

        assert_eq!(output.status, 0, "{output:?}");
        let prompt_text = prompt.seen_prompt.borrow();
        assert!(prompt_text.contains("may increase your token usage, time, and provider cost"));
        assert!(prompt_text.contains("will not run those sessions automatically"));
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("experiment JSON");
        assert_eq!(value["experiment_mode"], "controlled_pair");
        assert_eq!(value["cost_notice"]["may_have_increased_usage"], true);
    }

    #[test]
    fn experiment_consent_does_not_enable_telemetry_or_research() {
        let workspace = TempWorkspace::new("cli-experiment-consent-separate");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        let output = run_with_context(
            [
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--experiment-mode",
                "record-existing",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
                "--yes",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(output.status, 0);

        let status = run_with_context(["telemetry", "status", "--json"], workspace.path(), &env);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["enabled"], false);
        assert_eq!(value["research_enabled"], false);
    }

    #[test]
    fn telemetry_controlled_pair_report_warns_about_extra_usage_cost() {
        let workspace = TempWorkspace::new("cli-controlled-pair-cost");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };

        for args in [
            vec![
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--experiment-mode",
                "controlled-pair",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
                "--yes",
            ],
            vec![
                "telemetry",
                "experiment-record",
                "--name",
                "task-a",
                "--input-tokens",
                "100",
                "--output-tokens",
                "20",
                "--tool-tokens",
                "0",
                "--success",
                "true",
            ],
            vec!["telemetry", "experiment-stop", "--name", "task-a"],
            vec![
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--experiment-mode",
                "controlled-pair",
                "--session",
                "treatment",
                "--measurement-source",
                "user_entered",
                "--yes",
            ],
            vec![
                "telemetry",
                "experiment-record",
                "--name",
                "task-a",
                "--input-tokens",
                "60",
                "--output-tokens",
                "20",
                "--tool-tokens",
                "0",
                "--success",
                "false",
                "--test-outcome",
                "failed",
            ],
            vec!["telemetry", "experiment-stop", "--name", "task-a"],
        ] {
            let output = run_with_context(args, workspace.path(), &env);
            assert_eq!(output.status, 0, "{:?}", output);
        }

        let report = run_with_context(
            [
                "telemetry",
                "experiment-report",
                "--name",
                "task-a",
                "--json",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(report.status, 0);
        let value: Value = serde_json::from_str(report.stdout.trim()).expect("report JSON");
        assert_eq!(value["experiment_mode"], "controlled_pair");
        assert_eq!(value["token_savings"], 40);
        assert_eq!(value["claim_validity"], "invalid_for_product_claim");
        assert_eq!(value["correctness"]["treatment_success"], false);
        assert_eq!(value["cost_notice"]["may_have_increased_usage"], true);
    }

    #[test]
    fn stats_json_is_parseable_missing_index_fallback() {
        let workspace = TempWorkspace::new("cli-stats-json-missing-index");
        let env = |_: &str| None;
        let output = run_with_context(["stats", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("stats JSON");
        assert_eq!(value["command"], "stats");
        assert_eq!(value["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["reason"], "repository is not initialized");
        assert_eq!(value["guidance"], "run repogrammar init");
        let serialized = output.stderr;
        assert!(!serialized.contains("/Users/"));
        assert!(!serialized.contains("src/"));
    }

    #[test]
    fn stats_rejects_unknown_options() {
        let output = run(["stats", "--mystery"]);

        assert_eq!(output.status, 2);
        assert!(output.stderr.contains("unknown stats option: --mystery"));
    }
}
