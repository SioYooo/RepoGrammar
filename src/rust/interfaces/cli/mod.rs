//! CLI argument boundary for the `repogrammar` binary.

use crate::application::autosync::{AutosyncReport, AutosyncSettings};
use crate::application::indexing::IndexingOutcome;
use crate::application::install::{
    binary_name, known_agent_targets, normalize_concrete_targets, normalized_lexical_path,
    owned_install_receipt_exists, plan_install, resolve_instruction_file,
    supported_concrete_targets, target_adapter, targets_for_display, AgentTarget,
    InstallExecutionContext, InstallExecutionOutcome, InstallRequest, InstallScope,
};
use crate::application::progress::{ProgressEvent, ProgressStage, WorkUnits};
use crate::application::query::{
    build_read_plan, estimate_family_output_potential_token_savings, query_preflight,
    read_plan_with_rendered_spans, repository_status_unavailable_fallback, select_family_evidence,
    validate_query_target, validate_query_token_budget, DiagnosticSignal, FamilyDetailReport,
    FamilyEvidenceMode, FamilyListReport, FamilyLookupMode, FamilyLookupReport,
    FamilyOutputOptions, FamilyPartialContextReport, FamilyQueryUnknown, FamilyUnknownReport,
    IndexedCodeUnitsReport, IndexedFilesReport, QueryPreflightOperation, QueryPreflightReport,
    ReadPlan, ReadPlanItem, ReadPlanLineRangeOmission, RepoShapeDiagnosticsReport,
    ResolvedQueryTarget, SelectedFamilyEvidence, SourceSpanRenderReport, TokenSavingReadiness,
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
use crate::application::storage::DEFAULT_RETAINED_INACTIVE_GENERATIONS;
use crate::application::telemetry::{
    estimated_potential_token_savings_rollup, experiment_export, experiment_purge,
    experiment_record, experiment_report, experiment_report_json, experiment_start,
    experiment_stop, export_anonymous_telemetry, latest_comparable_experiment_report,
    purge_telemetry, record_estimated_potential_token_savings, record_passive_diagnostics_rollup,
    research_export, research_purge, set_anonymous_telemetry, set_research_trace,
    telemetry_disabled_by_environment, telemetry_status, upload_anonymous_telemetry,
    validate_telemetry_endpoint, EstimatedPotentialTokenSavingsRollup, ExperimentMode,
    ExperimentRecordRequest, ExperimentStartRequest, ExperimentWorkflowMode, MeasurementSource,
    TelemetryDiagnostics, TelemetryExportReport, TelemetryPaths, TelemetryPurgeReport,
    TelemetryStatusReport, TelemetryUploadReceipt, TelemetryUploadReport, TelemetryUploadRequest,
    TelemetryUploadTransport, TestOutcome,
};
use crate::core::model::EstimatedPotentialTokenSavings;
use crate::error::RepoGrammarError;
use crate::ports::index_store::{GenerationPruneReport, GenerationPruneRequest};
use serde_json::{json, Map, Value};
use std::fs;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncCommand {
    Enable,
    Disable,
    Start,
    Stop,
    Status,
    Run,
}

impl AutosyncCommand {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "enable" => Ok(Self::Enable),
            "disable" => Ok(Self::Disable),
            "start" => Ok(Self::Start),
            "stop" => Ok(Self::Stop),
            "status" => Ok(Self::Status),
            "run" => Ok(Self::Run),
            _ => Err(
                "autosync subcommand must be enable, disable, start, stop, status, or run"
                    .to_string(),
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Disable => "disable",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Status => "status",
            Self::Run => "run",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliAutosyncRequest {
    pub repository_root: String,
    pub state_dir_override: Option<String>,
    pub strict_gitignore: bool,
    pub poll_ms: u64,
    pub debounce_ms: u64,
    pub json: bool,
    pub quiet: bool,
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

    fn autosync(
        &self,
        _command: AutosyncCommand,
        _request: CliAutosyncRequest,
    ) -> Result<AutosyncReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("autosync"))
    }

    fn prune_generations(
        &self,
        _request: RepositoryStatusRequest,
        _prune: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("prune"))
    }

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

    fn enrich_read_plan_line_ranges(
        &self,
        _request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
    ) -> Result<ReadPlan, RepoGrammarError> {
        Ok(read_plan.clone())
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
            "resync" => "resync",
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

    fn success_with_stderr(stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: stderr.into(),
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
        [command, topic] if command == "help" => match command_usage(topic) {
            Some(usage) => CliOutput::success(usage),
            None => CliOutput::failure(2, format!("unknown help topic: {topic}\n")),
        },
        [command, ..] if command == "help" => CliOutput::failure(
            2,
            "help accepts at most one command topic; use repogrammar help <command>\n",
        ),
        [command, rest @ ..] if is_known_cli_command(command) && contains_help_flag(rest) => {
            CliOutput::success(command_usage(command).unwrap_or_else(usage))
        }
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
    help_text(&[
        "Usage: repogrammar <command> [options]",
        "",
        "RepoGrammar finds source-backed implementation-pattern families and exposes them to humans and coding agents.",
        "",
        "How to read help:",
        "  repogrammar help <command>",
        "  repogrammar <command> --help",
        "  repogrammar <command> -h",
        "",
        "Project lifecycle:",
        "  init [--project <path>] [--yes] [--resync] [--autosync] [--write-gitignore] [--json] [--progress auto|always|never]",
        "      Create safe repo-local state; optionally resync and start autosync in one agent-safe command.",
        "  uninit [--project <path>] --yes [--json]",
        "      Remove RepoGrammar repo-local state after explicit confirmation.",
        "  index [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Build a fresh syntax/code-unit index and activate it atomically.",
        "  sync [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Rebuild the active index using the same safe indexing path.",
        "  resync [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Rebuild the active index and static-analysis facts for any initialized repository.",
        "  autosync <status|enable|start|stop|disable|run> [options]",
        "      Manage optional repo-local automatic sync. Use `autosync start`, not `--start`.",
        "  prune [--project <path>] [--keep <n>] [--dry-run] [--yes] [--json]",
        "      Remove old inactive index generations while preserving the active generation.",
        "  status [--project <path>] [--json]",
        "      Report repository state, active generation, schema, and storage health.",
        "  doctor [--project <path>] [--json]",
        "      Inspect lifecycle hygiene, storage health, locks, and repair guidance.",
        "  unlock [--project <path>] [--force --yes] [--json]",
        "      Inspect locks; remove only confirmed stale index locks with --force --yes.",
        "  logs [--project <path>] [--component index|daemon|mcp|telemetry] [--tail [n]] [--since <duration>] [--json]",
        "      Read redacted repo-local diagnostics.",
        "",
        "Pattern-family queries:",
        "  find [target] [--mode compact|evidence|deep] [--token-budget <n>] [--json]",
        "      Find analogous implementations for a target without returning source by default.",
        "  families [--json]",
        "      List active supported pattern families, or typed UNKNOWN if support is insufficient.",
        "  family <family-id> [--mode compact|evidence|deep] [--include-source-spans] [--json]",
        "      Show one family by exact id.",
        "  member <member-id> [--mode compact|evidence|deep] [--json]",
        "      Show the family context for one exact indexed member id.",
        "  explain [target] [--include-variations] [--include-exceptions] [--json]",
        "      Explain whether a target is a legal variation, exception, incompatibility, or UNKNOWN.",
        "  check [target] [--mode compact|evidence|deep] [--json]",
        "      Return advisory conformance context; runtime equivalence remains UNKNOWN in this slice.",
        "  files [--project <path>] [--json]",
        "      Read active indexed file inventory.",
        "  units [--project <path>] [--json]",
        "      Read active indexed code-unit inventory.",
        "",
        "Agent integration:",
        "  serve [--project <path>]",
        "      Run the read-only MCP stdio server from the product binary.",
        "  install [--target <agent[,agent]>] [--scope global|project-local] [--dry-run] [--yes] [--print-config [agent]]",
        "      Configure RepoGrammar as a read-only MCP server for agents.",
        "  uninstall [--target <agent[,agent]>] [--scope global|project-local] --yes",
        "      Remove only RepoGrammar-owned agent integration receipts.",
        "",
        "Agent-safe repository bootstrap:",
        "  repogrammar init --yes --resync --autosync",
        "      install wires agents; init/resync/autosync remain explicit per-repository analysis steps.",
        "",
        "Metrics:",
        "  stats [--project <path>] [--json]",
        "      Report repo-shape diagnostics and estimated potential read displacement.",
        "  telemetry <status|on|off|export|upload|purge|research-*|experiment-*> [options]",
        "      Manage optional anonymous telemetry and local token experiment records.",
        "",
        "Maintenance:",
        "  version, --version, -V",
        "      Print the product version.",
        "  help [command]",
        "      Print this overview or command-specific usage.",
        "",
    ])
}

fn help_text(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn command_usage(command: &str) -> Option<String> {
    match command {
        "init" => Some(help_text(&[
            "Usage: repogrammar init [--project <path>|--path <path>] [--yes] [--resync] [--autosync] [--write-gitignore] [--json] [--progress auto|always|never] [--quiet|--verbose]",
            "",
            "Creates repository-local RepoGrammar state under .repogrammar/ by default.",
            "Without --resync it does not index source code. Without --write-gitignore it avoids tracked .gitignore edits and writes Git exclude hygiene instead.",
            "--yes is accepted as an agent-safe noninteractive confirmation flag; it does not broaden init writes.",
            "Use --resync to build or refresh static-analysis facts; add --autosync to keep them fresh during an agent editing session.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root to initialize. Defaults to the current directory.",
            "  --yes                              Accepted no-op confirmation flag for noninteractive agent bootstrap.",
            "  --resync                           Build or refresh the active index after init succeeds.",
            "  --autosync                         Start repo-local autosync after init; requires --resync or an existing active generation.",
            "  --write-gitignore                  Add a marker-fenced .gitignore entry in the repository root.",
            "  --json                             Emit machine-readable output.",
            "  --progress auto|always|never       Control progress output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "uninit" => Some(help_text(&[
            "Usage: repogrammar uninit [--project <path>|--path <path>] --yes [--json] [--quiet|--verbose]",
            "",
            "Removes RepoGrammar repo-local state. This is the only command that removes .repogrammar/.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --yes                              Required for removal.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "index" => Some(index_or_sync_usage("index", "Build a fresh index and atomically activate it.")),
        "sync" => Some(index_or_sync_usage("sync", "Rebuild the active index after repository changes.")),
        "resync" => Some(index_or_sync_usage("resync", "Rebuild the active index and static-analysis facts after repository changes.")),
        "autosync" => Some(help_text(&[
            "Usage: repogrammar autosync [status|enable|start|stop|disable|run] [options]",
            "",
            "Manages optional repository-local automatic sync. With no subcommand, autosync is equivalent to autosync status.",
            "Subcommands are positional: use `repogrammar autosync start`, not `repogrammar autosync --start`.",
            "Use autosync start after an initial resync when new or modified files should enter RepoGrammar results without manual resync.",
            "",
            "Subcommands:",
            "  status     Show auto-sync configuration and daemon state.",
            "  enable     Enable auto-sync configuration for this repository.",
            "  start      Enable if needed and launch a background autosync run worker.",
            "  stop       Stop the recorded autosync daemon and remove its daemon lock.",
            "  disable    Disable auto-sync; the daemon must already be stopped.",
            "  run        Run the foreground polling worker used by start.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --json                             Emit machine-readable output.",
            "  --quiet                            Suppress nonessential human output.",
            "  --progress auto|always|never       Accepted for long-running command compatibility.",
            "  --poll-ms <n>                      Poll interval, 100 through 600000 milliseconds.",
            "  --debounce-ms <n>                  Debounce interval, 0 through 60000 milliseconds.",
        ])),
        "prune" => Some(help_text(&[
            "Usage: repogrammar prune [--project <path>|--path <path>] [--keep <n>] [--dry-run] [--yes] [--json] [--quiet|--verbose]",
            "",
            "Removes old inactive index generation directories. The active generation is always preserved.",
            "Destructive runs require --yes. Use --dry-run to inspect candidates without deleting.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --keep <n>                         Number of newest inactive generations to keep. Defaults to 2 and may be 0.",
            "  --dry-run                          Report prune candidates without deleting directories.",
            "  --yes                              Required unless --dry-run is present.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "status" => Some(status_or_doctor_usage("status", "Report repository initialization, manifest, active generation, schema, and storage health.")),
        "doctor" => Some(status_or_doctor_usage("doctor", "Inspect lifecycle hygiene, storage health, lock state, and recovery guidance.")),
        "unlock" => Some(help_text(&[
            "Usage: repogrammar unlock [--project <path>|--path <path>] [--force --yes] [--json] [--quiet|--verbose]",
            "",
            "Inspects RepoGrammar locks. Without --force --yes it is inspection-only.",
            "With --force --yes it removes only confirmed stale index locks; daemon and SQLite locks are preserved.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --force                            Request stale index-lock removal.",
            "  --yes                              Confirm --force removal.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "logs" => Some(help_text(&[
            "Usage: repogrammar logs [--project <path>|--path <path>] [--component index|daemon|mcp|telemetry] [--since <duration>] [--tail [n]] [--redact] [--json] [--quiet|--verbose]",
            "",
            "Reads redacted repo-local diagnostic logs. Logs are local diagnostics, not telemetry.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --component <name>                  Filter to index, daemon, mcp, or telemetry logs.",
            "  --since <duration>                  Filter by duration such as 1h.",
            "  --tail [n]                          Return the last n entries; defaults to 100 when n is omitted.",
            "  --redact                            Keep output metadata-only and source-free. This is the default.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "find" => Some(query_usage(
            "find",
            "repogrammar find [target] [options]",
            "Find source-backed analogous implementations for a target.",
        )),
        "families" => Some(query_usage(
            "families",
            "repogrammar families [options]",
            "List active supported pattern families, or typed UNKNOWN if evidence is insufficient.",
        )),
        "family" => Some(query_usage(
            "family",
            "repogrammar family <family-id> [options]",
            "Show a family by exact family id.",
        )),
        "member" => Some(query_usage(
            "member",
            "repogrammar member <member-id> [options]",
            "Show family context for an exact indexed member id.",
        )),
        "explain" => Some(query_usage(
            "explain",
            "repogrammar explain [target] [options]",
            "Explain whether a target is a legal variation, exception, incompatibility, or UNKNOWN.",
        )),
        "check" => Some(query_usage(
            "check",
            "repogrammar check [target] [options]",
            "Return advisory conformance context; runtime equivalence remains UNKNOWN in this slice.",
        )),
        "files" => Some(help_text(&[
            "Usage: repogrammar files [--project <path>] [--json]",
            "",
            "Reads repo-relative active indexed-file inventory from a readable active generation.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --json                             Emit machine-readable output.",
        ])),
        "units" => Some(help_text(&[
            "Usage: repogrammar units [--project <path>] [--json]",
            "",
            "Reads repo-relative active code-unit inventory from a readable active generation.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --json                             Emit machine-readable output.",
        ])),
        "serve" => Some(help_text(&[
            "Usage: repogrammar serve [--project <path>|--path <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
            "",
            "Runs the read-only MCP stdio server from the product binary.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root served through MCP. Defaults to the current directory.",
            "  --progress auto|always|never       Accepted for long-running command compatibility.",
            "  --json                             Accepted by the CLI parser.",
            "  --quiet, --verbose                 Accepted serving verbosity flags.",
        ])),
        "install" => Some(install_usage("install", "Configure RepoGrammar as a read-only MCP server for coding agents.")),
        "uninstall" => Some(install_usage("uninstall", "Remove RepoGrammar-owned agent integration receipts and managed entries.")),
        "stats" => Some(help_text(&[
            "Usage: repogrammar stats [--project <path>] [--json] [--quiet|--verbose]",
            "",
            "Reports repo-shape diagnostics and estimated potential read displacement. It does not upload telemetry.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted metrics verbosity flags.",
        ])),
        "telemetry" => Some(help_text(&[
            "Usage: repogrammar telemetry [status|on|off|export|upload|purge|research-*|experiment-*] [options]",
            "",
            "Manages optional anonymous telemetry, research-trace consent, and local paired token experiment records.",
            "Telemetry is disabled by default and uploads only through explicit telemetry upload.",
            "",
            "Anonymous telemetry subcommands:",
            "  status [--json] [--project <path>]",
            "  on|off [--json] [--project <path>]",
            "  export [--json] [--project <path>]",
            "  upload [--json] [--dry-run] [--yes] [--endpoint <url>] [--project <path>]",
            "  purge [--json] --yes [--project <path>]",
            "",
            "Research trace subcommands:",
            "  research-status|research-on|research-off|research-export|research-purge [--json] [--yes]",
            "",
            "Experiment subcommands:",
            "  experiment-start --name <name> --experiment-mode record_existing|controlled_pair --session baseline|treatment --measurement-source host_reported|user_entered|documented_tokenizer [--yes] [--json]",
            "  experiment-record --name <name> (--usage-json <path>|--input-tokens <n> --output-tokens <n>) [--tool-tokens <n>] [--success true|false] [--json]",
            "  experiment-stop|experiment-report|experiment-export|experiment-purge --name <name> [--yes] [--json]",
            "",
            "Common options:",
            "  --project <path>                    Repository root used for local diagnostics.",
            "  --json                             Emit machine-readable output.",
            "  --yes                              Confirm upload, purge, or experiment recording where required.",
            "  --dry-run                          Validate upload payload without network activity.",
            "  --endpoint <url>                    HTTPS or localhost telemetry upload endpoint.",
        ])),
        "version" => Some(help_text(&[
            "Usage: repogrammar version",
            "       repogrammar --version",
            "       repogrammar -V",
            "",
            "Prints the RepoGrammar package version.",
        ])),
        "help" => Some(help_text(&[
            "Usage: repogrammar help [command]",
            "       repogrammar <command> --help",
            "       repogrammar <command> -h",
            "",
            "Prints top-level help or command-specific usage and options.",
        ])),
        _ => None,
    }
}

fn index_or_sync_usage(command: &str, summary: &str) -> String {
    help_text(&[
        &format!("Usage: repogrammar {command} [--project <path>|--path <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]"),
        "",
        summary,
        "Requires initialized repo-local state and writes a new validated active generation. Agents may run resync after init when analysis is missing or stale, and autosync start when subsequent edits should update automatically.",
        "",
        "Options:",
        "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
        "  --json                             Emit machine-readable output.",
        "  --progress auto|always|never       Control human or NDJSON progress events on stderr.",
        "  --quiet                            Suppress progress and nonessential human output.",
        "  --verbose                          Accepted lifecycle verbosity flag.",
    ])
}

fn status_or_doctor_usage(command: &str, summary: &str) -> String {
    help_text(&[
        &format!("Usage: repogrammar {command} [--project <path>|--path <path>] [--json] [--quiet|--verbose]"),
        "",
        summary,
        "",
        "Options:",
        "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
        "  --json                             Emit machine-readable output.",
        "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
    ])
}

fn query_usage(_command: &str, usage_line: &str, summary: &str) -> String {
    help_text(&[
        &format!("Usage: {usage_line}"),
        "",
        summary,
        "Queries never initialize a repository. Missing or stale indexes return fallback or typed UNKNOWN guidance.",
        "",
        "Options:",
        "  --project <path>                    Repository root. Defaults to the current directory.",
        "  --token-budget <n>                  Positive budget up to 200000; implies --mode evidence unless mode is explicit.",
        "  --mode compact|evidence|deep        Select metadata detail. Deep still omits source unless source spans are requested.",
        "  --json                             Emit machine-readable output.",
        "  --include-variations               Request variation evidence coverage metadata.",
        "  --include-exceptions               Request exception evidence coverage metadata.",
        "  --include-source-spans             Explicitly render bounded hash-checked source spans when available.",
    ])
}

fn install_usage(command: &str, summary: &str) -> String {
    help_text(&[
        &format!("Usage: repogrammar {command} [--target <target[,target]>] [--scope global|project-local] [--location global|local] [--dry-run] [--yes] [--print-config [target]] [--telemetry|--no-telemetry] [--no-permissions]"),
        "",
        summary,
        "Installer commands configure agent integration only; they do not initialize, index, or rewrite .repogrammar/.",
        "",
        "Targets:",
        "  auto, all, none, codex, claude-code, claude, cursor, opencode, hermes, gemini, antigravity, kiro",
        "",
        "Options:",
        "  --target <target[,target]>          Select one target, all/auto, none, or comma-separated concrete targets.",
        "  --scope global|project-local        Select integration scope. project and local are accepted aliases.",
        "  --location global|local             Alias for --scope.",
        "  --dry-run                          Print the reversible plan without writing.",
        "  --yes                              Required for noninteractive live writes.",
        "  --print-config [target]             Print MCP config snippet without live writes.",
        "  --telemetry, --no-telemetry         Explicit anonymous telemetry consent choice for install.",
        "  --no-permissions                   Reserve no extra permission prompts.",
    ])
}

fn contains_help_flag(rest: &[String]) -> bool {
    rest.iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

fn is_known_cli_command(command: &str) -> bool {
    is_project_lifecycle_command(command)
        || is_query_command(command)
        || is_installer_command(command)
        || matches!(command, "stats" | "telemetry" | "version" | "help")
}

fn is_project_lifecycle_command(command: &str) -> bool {
    matches!(
        command,
        "init"
            | "uninit"
            | "index"
            | "sync"
            | "resync"
            | "autosync"
            | "prune"
            | "status"
            | "doctor"
            | "unlock"
            | "logs"
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
    if command == "autosync" {
        return match parse_autosync_options(rest) {
            Ok(options) => handle_autosync(&options, current_dir, env_lookup, runtime),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }
    if command == "prune" {
        return match parse_prune_options(rest) {
            Ok(options) => handle_prune(&options, current_dir, env_lookup, runtime),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }

    let options = match parse_lifecycle_options(command, rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };

    match command {
        "init" => handle_init(&options, current_dir, env_lookup, runtime),
        "uninit" => handle_uninit(&options, current_dir, env_lookup),
        "status" => handle_status(&options, current_dir, env_lookup, runtime),
        "doctor" => handle_doctor(&options, current_dir, env_lookup, runtime),
        "unlock" => handle_unlock(&options, current_dir, env_lookup),
        "index" | "sync" | "resync" => {
            handle_index(command, &options, current_dir, env_lookup, runtime)
        }
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
                    let prepared_output = match prepare_family_output(
                        runtime,
                        request.clone(),
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
                    record_family_query_estimated_potential_token_savings(
                        request.clone(),
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                    );
                    CliOutput::success(family_lookup_json(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                    ))
                }
                Ok(report) => {
                    let prepared_output = match prepare_family_output(
                        runtime,
                        request.clone(),
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
                    record_family_query_estimated_potential_token_savings(
                        request.clone(),
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                    );
                    CliOutput::success(family_lookup_human(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
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
            "run repogrammar resync after pattern-family indexing is implemented",
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

struct PreparedFamilyOutput {
    read_plan: ReadPlan,
    source_spans: Option<SourceSpanRenderReport>,
}

fn prepare_family_output(
    runtime: &impl CliRuntime,
    request: RepositoryStatusRequest,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    include_source_spans: bool,
) -> Result<Option<PreparedFamilyOutput>, RepoGrammarError> {
    let base_read_plan = match report {
        FamilyLookupReport::Found(family) => build_read_plan(family, target, mode, options),
        FamilyLookupReport::PartialContext(report) => report.read_plan.clone(),
        FamilyLookupReport::Unknown(_) => return Ok(None),
    };
    let mut read_plan = runtime.enrich_read_plan_line_ranges(request.clone(), &base_read_plan)?;
    let source_spans = if include_source_spans {
        let rendered =
            runtime.render_source_spans(request, &read_plan, true, options.token_budget)?;
        read_plan = read_plan_with_rendered_spans(&read_plan, &rendered);
        Some(rendered)
    } else {
        None
    };
    Ok(Some(PreparedFamilyOutput {
        read_plan,
        source_spans,
    }))
}

fn record_family_query_estimated_potential_token_savings(
    request: RepositoryStatusRequest,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) {
    let FamilyLookupReport::Found(family) = report else {
        return;
    };
    let output_components =
        family_output_components(family, target, mode, options, prepared_output);
    let _ = record_estimated_potential_token_savings(
        request,
        &output_components.estimated_potential_token_savings,
    );
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
                "recovery: run repogrammar resync after adding compatible implementations\n",
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
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    match report {
        FamilyLookupReport::Found(family) => {
            let output_components =
                family_output_components(family, target, mode, options, prepared_output);
            let selected_evidence = &output_components.selected_evidence;
            let read_plan = &output_components.read_plan;
            let estimated_potential = &output_components.estimated_potential_token_savings;
            let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
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
                "estimated_potential_token_savings: {}\n",
                estimated_potential.estimated_potential_token_savings
            ));
            output.push_str(&format!(
                "estimated_potential_token_savings_kind: {}\n",
                estimated_potential.measurement_kind.as_str()
            ));
            output.push_str(&format!(
                "estimated_potential_token_savings_caveat: {}\n",
                estimated_potential.caveat
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
            push_read_plan_human(&mut output, read_plan, selected_evidence.mode);
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
        FamilyLookupReport::PartialContext(report) => {
            family_partial_context_human(command, report, options, prepared_output)
        }
        FamilyLookupReport::Unknown(report) => family_unknown_human(command, report),
    }
}

fn family_partial_context_human(
    command: &str,
    report: &FamilyPartialContextReport,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    let read_plan = prepared_output
        .map(|prepared| &prepared.read_plan)
        .unwrap_or(&report.read_plan);
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let snippets = if read_plan.source_snippets_included {
        "included"
    } else {
        "not_included"
    };
    let mut output = format!(
        "{command}: PARTIAL_CONTEXT\nactive_generation: {}\nresolved_target: {}\tkind: {}\tpath: {}\tline: {}\tbyte_range: {}\tcode_unit_id: {}\tconfidence: {}\tmatch_kind: {}\nestimated_read_plan_tokens: {}\nsource_snippets: {}\nread_plan_requires_source_before_edit: {}\n",
        report.active_generation,
        report.resolved_target.original_target,
        report.resolved_target.kind,
        report.resolved_target.path,
        report
            .resolved_target
            .line
            .map(|line| line.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .resolved_target
            .byte_range
            .map(|(start, end)| format!("{start}-{end}"))
            .unwrap_or_else(|| "none".to_string()),
        report
            .resolved_target
            .code_unit_id
            .as_deref()
            .unwrap_or("none"),
        report.resolved_target.confidence,
        report.resolved_target.match_kind,
        read_plan.estimated_tokens,
        snippets,
        read_plan.requires_source_before_edit
    );
    push_read_plan_human(&mut output, read_plan, options.evidence_mode);
    if let Some(source_spans) = source_spans {
        push_source_spans_human(&mut output, source_spans);
    }
    if command == "check" {
        output.push_str("advisory_status: UNKNOWN\n");
        output.push_str("advisory_reason: runtime equivalence remains unproven\n");
    }
    for unknown in &report.unknowns {
        push_unknown_human(&mut output, unknown);
    }
    output
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
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    match report {
        FamilyLookupReport::Found(family) => {
            family_detail_json(command, family, target, mode, options, prepared_output)
        }
        FamilyLookupReport::PartialContext(report) => {
            family_partial_context_json(command, report, options, prepared_output)
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

fn family_partial_context_json(
    command: &str,
    report: &FamilyPartialContextReport,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    let read_plan = prepared_output
        .map(|prepared| &prepared.read_plan)
        .unwrap_or(&report.read_plan);
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let mut value = json!({
        "command": command,
        "status": "PARTIAL_CONTEXT",
        "implemented": true,
        "active_generation": report.active_generation,
        "resolved_target": resolved_target_json(&report.resolved_target),
        "output": {
            "mode": options.evidence_mode.as_str(),
            "token_budget": options.token_budget,
            "estimated_read_plan_tokens": read_plan.estimated_tokens,
            "selection_strategy": read_plan.selection_strategy,
            "budget_satisfied": read_plan.budget_satisfied,
            "source_snippets_included": read_plan.source_snippets_included,
        },
        "read_plan": read_plan_json(read_plan),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&report.unknowns),
    });
    if command == "check" {
        value["check"] = check_advisory_json();
    }
    json_line(value)
}

fn family_detail_json(
    command: &str,
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    let output_components =
        family_output_components(family, target, mode, options, prepared_output);
    let selected_evidence = &output_components.selected_evidence;
    let read_plan = &output_components.read_plan;
    let estimated_potential = &output_components.estimated_potential_token_savings;
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let check = if command == "check" {
        Some(check_advisory_json())
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
        "read_plan": read_plan_json(read_plan),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&family.unknowns),
        "check": check,
    }))
}

struct FamilyOutputComponents {
    selected_evidence: SelectedFamilyEvidence,
    read_plan: ReadPlan,
    estimated_potential_token_savings: EstimatedPotentialTokenSavings,
}

fn family_output_components(
    family: &FamilyDetailReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> FamilyOutputComponents {
    let selected_evidence = select_family_evidence(family, options);
    let read_plan = prepared_output
        .map(|prepared| prepared.read_plan.clone())
        .unwrap_or_else(|| build_read_plan(family, target, mode, options));
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let estimated_potential_token_savings = estimate_family_output_potential_token_savings(
        family,
        &selected_evidence,
        &read_plan,
        source_spans,
    );
    FamilyOutputComponents {
        selected_evidence,
        read_plan,
        estimated_potential_token_savings,
    }
}

fn resolved_target_json(target: &ResolvedQueryTarget) -> serde_json::Value {
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

fn check_advisory_json() -> serde_json::Value {
    json!({
        "advisory_status": "UNKNOWN",
        "reason": "runtime equivalence remains unproven",
    })
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
    for omission in &read_plan.line_range_omissions {
        output.push_str(&format!(
            "read_plan_line_range_omitted: {}\tpath: {}\trange: {}-{}\treason: {}\tguidance: {}\n",
            omission.purpose.as_str(),
            omission.path,
            omission.start_byte,
            omission.end_byte,
            omission.reason,
            omission.guidance
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
        "line_range_omissions": read_plan.line_range_omissions.iter().map(read_plan_line_range_omission_json).collect::<Vec<_>>(),
    })
}

fn read_plan_line_range_omission_json(omission: &ReadPlanLineRangeOmission) -> serde_json::Value {
    json!({
        "purpose": omission.purpose.as_str(),
        "path": omission.path,
        "start_byte": omission.start_byte,
        "end_byte": omission.end_byte,
        "reason": omission.reason,
        "guidance": omission.guidance,
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
        for line in install_dry_run_native_plan(&request, env_lookup) {
            output.push_str(&line);
            output.push('\n');
        }
        if command == "install" {
            for line in install_environment_warnings(
                &discovered_repogrammar_executables(env_lookup),
                None,
                None,
            ) {
                output.push_str(&line);
                output.push('\n');
            }
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
                    for line in install_environment_warnings(
                        &discovered_repogrammar_executables(env_lookup),
                        outcome.command_path.as_deref(),
                        outcome.installed_executable_path.as_deref(),
                    ) {
                        output.push_str(&line);
                        output.push('\n');
                    }
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
            "install: no detected uninstalled agent integrations were selected; no changes made\n"
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
        .prompt_install_confirmation(&format!("{plan}\nProceed with install? [Y/n] "))?;
    if !parse_default_yes_prompt_response(&confirmation)? {
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
    let automatic_targets = default_interactive_targets(statuses);
    let all_installed = statuses.iter().all(|status| status.installed);
    let automatic_label = if all_installed {
        "refresh all already managed agents"
    } else if automatic_targets.is_empty() {
        "all detected not-yet-installed agents (currently none)"
    } else {
        "all detected not-yet-installed agents"
    };
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
    prompt.push_str(&format!(
        "\nSelect agents to configure:\n  1 = Codex\n  2 = Claude Code\n  1,2 = both\n  a = {automatic_label}\n  none = configure no agents\n  q = cancel\n\nSelection [a]: "
    ));
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
    if trimmed.eq_ignore_ascii_case("none") {
        selected = Vec::new();
    } else if trimmed.is_empty()
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
                _ => return Err(
                    "unknown agent selection; use 1, 2, 1,2, codex, claude-code, all, none, or q"
                        .to_string(),
                ),
            };
            if !selected.contains(&target) {
                selected.push(target);
            }
        }
    }
    if selected.is_empty() {
        return Ok(Some(Vec::new()));
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
    if statuses.iter().all(|status| status.installed) {
        statuses.iter().map(|status| status.target).collect()
    } else {
        Vec::new()
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

fn install_dry_run_native_plan<F>(request: &InstallRequest, env_lookup: &F) -> Vec<String>
where
    F: Fn(&str) -> Option<String>,
{
    let targets = targets_for_display(request);
    if targets.is_empty() {
        return vec!["native_mcp: no agent targets selected".to_string()];
    }
    targets
        .into_iter()
        .flat_map(|target| target_adapter(target).describe_paths(request.scope, env_lookup))
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
        let adapter = target_adapter(target);
        output.push_str(&format!(
            "config preview: target={} scope={}\n",
            adapter.target_id(),
            request.scope.as_str()
        ));
        output.push_str(&adapter.print_config(request.scope)?);
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

fn parse_default_yes_prompt_response(response: &str) -> Result<bool, String> {
    match response.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
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
    let mut instruction_files = Vec::new();
    for target in known_agent_targets() {
        if let Some(path) = resolve_instruction_file(target, env_lookup) {
            instruction_files.push((target, path));
        }
    }
    Ok(InstallExecutionContext {
        executable_path,
        command_dir,
        command_dir_on_path,
        data_dir,
        current_dir: current_dir.display().to_string(),
        instruction_files,
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

fn discovered_repogrammar_executables<F>(env_lookup: &F) -> Vec<String>
where
    F: Fn(&str) -> Option<String>,
{
    let name = binary_name();
    let mut found = Vec::new();
    let mut seen = Vec::new();
    for dir in path_entries(env_lookup) {
        let candidate = dir.join(name);
        if !candidate.is_file() {
            continue;
        }
        let key = normalized_lexical_path(&candidate);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        found.push(candidate.display().to_string());
    }
    found
}

/// Advisory install-time environment self-check. Reports when multiple
/// `repogrammar` executables are discoverable on PATH, or when the PATH-resolved
/// `repogrammar` is not the RepoGrammar-managed command. It never blocks install.
fn install_environment_warnings(
    discovered_copies: &[String],
    command_path: Option<&str>,
    installed_executable: Option<&str>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if discovered_copies.len() > 1 {
        warnings.push(format!(
            "self-check: multiple repogrammar executables are on PATH ({}); your shell and coding agents may run different versions. Keep one authority and remove the rest.",
            discovered_copies.join(", ")
        ));
    }
    if let (Some(first), Some(command)) = (discovered_copies.first(), command_path) {
        let first_key = normalized_lexical_path(Path::new(first));
        let matches_authority = [Some(command), installed_executable]
            .into_iter()
            .flatten()
            .any(|authority| normalized_lexical_path(Path::new(authority)) == first_key);
        if !matches_authority {
            warnings.push(format!(
                "self-check: the repogrammar resolved first on PATH ({first}) is not the RepoGrammar-managed command ({command}); reinstalls update the managed copy, not the PATH entry."
            ));
        }
    }
    warnings
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
            return stats_fallback(options.json, fallback.reason, fallback.guidance);
        }
    };

    match query_preflight(
        QueryPreflightOperation::ActiveIndexInventory,
        &status_report,
    ) {
        QueryPreflightReport::Fallback(fallback) => {
            return stats_fallback(options.json, fallback.reason, fallback.guidance);
        }
        QueryPreflightReport::Ready => {}
    }

    if options.json {
        return match runtime.repo_shape_diagnostics(request.clone()) {
            Ok(report) => {
                let measurement = telemetry_global_data_dir(env_lookup)
                    .ok()
                    .and_then(|dir| latest_comparable_experiment_report(&dir).ok().flatten());
                let estimated_rollup =
                    estimated_potential_token_savings_rollup(request.clone()).unwrap_or_default();
                record_stats_telemetry_rollup(
                    current_dir,
                    env_lookup,
                    options.project_path.as_deref(),
                    &report,
                    measurement.as_ref(),
                );
                CliOutput::success(stats_json(&report, measurement.as_ref(), &estimated_rollup))
            }
            Err(_) => stats_fallback(
                true,
                "repository status is unavailable",
                "run repogrammar doctor",
            ),
        };
    }

    match runtime.repo_shape_diagnostics(request.clone()) {
        Ok(report) => {
            let estimated_rollup =
                estimated_potential_token_savings_rollup(request).unwrap_or_default();
            CliOutput::success(stats_human(&report, &estimated_rollup))
        }
        Err(_) => stats_fallback(
            false,
            "repository status is unavailable",
            "run repogrammar doctor",
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

const ESTIMATED_TOKEN_SAVING_CAVEAT: &str = "estimated potential only; not measured token savings";

fn stats_fallback(json: bool, reason: &str, guidance: &str) -> CliOutput {
    if json {
        return CliOutput::failure(
            2,
            json_line(json!({
                "status": "FALLBACK_TO_CODE_SEARCH",
                "reason": reason,
                "guidance": guidance,
                "command": "stats",
                "implemented": true,
                "token_saving_readiness": TokenSavingReadiness::Unknown.as_str(),
                "blocking_reasons": stats_fallback_blocking_reasons(reason),
                "measurement_kind": "ESTIMATED",
                "caveat": ESTIMATED_TOKEN_SAVING_CAVEAT,
            })),
        );
    }

    query_fallback("stats", false, reason, guidance, true)
}

fn stats_fallback_blocking_reasons(reason: &str) -> Vec<&'static str> {
    if reason.contains("active index") || reason.contains("initialized") {
        vec!["no_active_generation"]
    } else {
        vec!["repository_status_unavailable"]
    }
}

fn stats_human(
    report: &RepoShapeDiagnosticsReport,
    estimated_rollup: &EstimatedPotentialTokenSavingsRollup,
) -> String {
    format!(
        "stats: repo-shape diagnostics\nactive_generation: {}\neligible_code_units: {}\nfamily_count: {}\nfamily_member_count: {}\ncovered_code_units: {}\nlocal_pattern_density: {}\nfamily_support_coverage: {}\nabstention_rate: {}\nexternal_dependency_signal: {}\nthin_wrapper_risk: {}\ntoken_saving_risk: {}\ntoken_saving_readiness: {}\nblocking_reasons: {}\nestimated_potential_token_savings: {}\nestimated_potential_token_savings_events: {}\nmeasurement_kind: ESTIMATED\ncaveat: {}\nestimated_potential_token_savings_kind: {}\nestimated_potential_token_savings_caveat: {}\ninterpretation: {}\n",
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
        report.token_saving_readiness.as_str(),
        stats_blocking_reasons_human(report.blocking_reasons.iter().copied()),
        estimated_rollup.total_estimated_potential_token_savings,
        estimated_rollup.event_count,
        ESTIMATED_TOKEN_SAVING_CAVEAT,
        estimated_rollup.measurement_kind.as_str(),
        estimated_rollup.caveat,
        report.interpretation
    )
}

fn stats_json(
    report: &RepoShapeDiagnosticsReport,
    measurement: Option<&crate::application::telemetry::ExperimentReport>,
    estimated_rollup: &EstimatedPotentialTokenSavingsRollup,
) -> String {
    let paired_measurement_available = measurement
        .and_then(|measurement| measurement.token_savings)
        .is_some();
    let measurement_status = if paired_measurement_available {
        "paired_measurement_available"
    } else {
        "no_paired_measurement"
    };
    let measurement_kind = if paired_measurement_available {
        "MEASURED"
    } else {
        "ESTIMATED"
    };
    let caveat = if paired_measurement_available {
        "paired measurement available; estimated potential remains diagnostic"
    } else {
        ESTIMATED_TOKEN_SAVING_CAVEAT
    };
    let blocking_reasons = stats_blocking_reasons(
        report.blocking_reasons.iter().copied(),
        paired_measurement_available,
    );
    json_line(json!({
        "command": "stats",
        "status": "ok",
        "implemented": true,
        "active_generation": report.active_generation,
        "token_saving_readiness": report.token_saving_readiness.as_str(),
        "blocking_reasons": blocking_reasons,
        "measurement_kind": measurement_kind,
        "caveat": caveat,
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
        "estimated_potential_token_savings": estimated_rollup.total_estimated_potential_token_savings,
        "estimated_potential_token_savings_metric": {
            "measurement_kind": estimated_rollup.measurement_kind.as_str(),
            "event_count": estimated_rollup.event_count,
            "total_estimated_baseline_tokens": estimated_rollup.total_estimated_baseline_tokens,
            "total_estimated_returned_tokens": estimated_rollup.total_estimated_returned_tokens,
            "total_estimated_potential_token_savings": estimated_rollup.total_estimated_potential_token_savings,
            "caveat": estimated_rollup.caveat,
        },
        "measurement_status": measurement_status,
        "measurement_reason": measurement.and_then(|measurement| measurement.reason.as_deref()),
        "claim_validity": measurement.map(|measurement| measurement.claim_validity.as_str()).unwrap_or("unknown"),
        "context_compression_ratio": null,
        "interpretation": report.interpretation,
        "claim": "diagnostic only; token saving depends on repeated repo-local patterns and is not measured token savings",
    }))
}

fn stats_blocking_reasons<I>(reasons: I, paired_measurement_available: bool) -> Vec<&'static str>
where
    I: IntoIterator<Item = &'static str>,
{
    let mut output = Vec::new();
    for reason in reasons {
        if !output.contains(&reason) {
            output.push(reason);
        }
    }
    if !paired_measurement_available && !output.contains(&"no_paired_experiment") {
        output.push("no_paired_experiment");
    }
    output
}

fn stats_blocking_reasons_human<I>(reasons: I) -> String
where
    I: IntoIterator<Item = &'static str>,
{
    let reasons = stats_blocking_reasons(reasons, false);
    if reasons.is_empty() {
        "none".to_string()
    } else {
        reasons.join(",")
    }
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
    usage_json_path: Option<PathBuf>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    tool_tokens: Option<u64>,
    success: Option<bool>,
    test_outcome: Option<TestOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExperimentTokenUsageImport {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    tool_tokens: Option<u64>,
    success: Option<bool>,
    test_outcome: Option<TestOutcome>,
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
    let mut options = ExperimentRecordOptions::default();
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
            "--usage-json" => {
                options.usage_json_path = Some(PathBuf::from(option_value(
                    rest,
                    index,
                    "--usage-json",
                    "a token usage JSON file",
                )?));
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
                )?)
                .map(Some)?;
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
    let imported = match options.usage_json_path.as_deref() {
        Some(path) => load_experiment_token_usage_json(path)?,
        None => ExperimentTokenUsageImport::default(),
    };
    let input_tokens = options
        .input_tokens
        .or(imported.input_tokens)
        .ok_or_else(|| "--input-tokens is required".to_string())?;
    let output_tokens = options
        .output_tokens
        .or(imported.output_tokens)
        .ok_or_else(|| "--output-tokens is required".to_string())?;
    let tool_tokens = options.tool_tokens.or(imported.tool_tokens).unwrap_or(0);
    let success = options
        .success
        .or(imported.success)
        .ok_or_else(|| "--success is required".to_string())?;
    let test_outcome = options
        .test_outcome
        .or(imported.test_outcome)
        .unwrap_or(TestOutcome::Unknown);
    Ok(ExperimentRecordRequest {
        name: options
            .name
            .ok_or_else(|| "--name is required".to_string())?,
        input_tokens,
        output_tokens,
        tool_tokens,
        success,
        test_outcome,
    })
}

const MAX_EXPERIMENT_TOKEN_USAGE_JSON_BYTES: u64 = 64 * 1024;

fn load_experiment_token_usage_json(path: &Path) -> Result<ExperimentTokenUsageImport, String> {
    let metadata = fs::metadata(path).map_err(|_| "failed to read token usage JSON".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_EXPERIMENT_TOKEN_USAGE_JSON_BYTES {
        return Err("token usage JSON must be a regular file no larger than 64 KiB".to_string());
    }
    let text =
        fs::read_to_string(path).map_err(|_| "failed to read token usage JSON".to_string())?;
    let value: Value =
        serde_json::from_str(&text).map_err(|_| "token usage JSON is invalid".to_string())?;
    parse_experiment_token_usage_json(&value)
}

fn parse_experiment_token_usage_json(value: &Value) -> Result<ExperimentTokenUsageImport, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "token usage JSON must be an object".to_string())?;
    reject_unsupported_token_usage_keys(
        object,
        &[
            "schema_version",
            "usage",
            "input_tokens",
            "prompt_tokens",
            "output_tokens",
            "completion_tokens",
            "tool_tokens",
            "total_tokens",
            "success",
            "test_outcome",
        ],
    )?;
    let usage_object = match object.get("usage") {
        Some(usage) => {
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "input_tokens"
                        | "prompt_tokens"
                        | "output_tokens"
                        | "completion_tokens"
                        | "tool_tokens"
                        | "total_tokens"
                )
            }) {
                return Err(
                    "token usage JSON must put token counts either under usage or at top level"
                        .to_string(),
                );
            }
            let usage_object = usage
                .as_object()
                .ok_or_else(|| "token usage field must be an object".to_string())?;
            reject_unsupported_token_usage_keys(
                usage_object,
                &[
                    "input_tokens",
                    "prompt_tokens",
                    "output_tokens",
                    "completion_tokens",
                    "tool_tokens",
                    "total_tokens",
                ],
            )?;
            usage_object
        }
        None => object,
    };
    let input_tokens = aliased_usage_u64(usage_object, "input_tokens", "prompt_tokens")?;
    let output_tokens = aliased_usage_u64(usage_object, "output_tokens", "completion_tokens")?;
    let explicit_tool_tokens = usage_u64(usage_object, "tool_tokens")?;
    let total_tokens = usage_u64(usage_object, "total_tokens")?;
    let tool_tokens = match (
        explicit_tool_tokens,
        total_tokens,
        input_tokens,
        output_tokens,
    ) {
        (Some(tool_tokens), _, _, _) => Some(tool_tokens),
        (None, Some(total), Some(input), Some(output)) => {
            let known = input.saturating_add(output);
            if total < known {
                return Err(
                    "token usage total_tokens is smaller than input plus output tokens".to_string(),
                );
            }
            Some(total - known)
        }
        _ => None,
    };
    let success = object.get("success").map(usage_bool).transpose()?;
    let test_outcome = object
        .get("test_outcome")
        .map(usage_test_outcome)
        .transpose()?;
    Ok(ExperimentTokenUsageImport {
        input_tokens,
        output_tokens,
        tool_tokens,
        success,
        test_outcome,
    })
}

fn reject_unsupported_token_usage_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), String> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("token usage JSON contains unsupported fields".to_string());
    }
    Ok(())
}

fn aliased_usage_u64(
    object: &Map<String, Value>,
    primary: &str,
    alias: &str,
) -> Result<Option<u64>, String> {
    let primary_value = usage_u64(object, primary)?;
    let alias_value = usage_u64(object, alias)?;
    match (primary_value, alias_value) {
        (Some(primary_value), Some(alias_value)) if primary_value != alias_value => {
            Err("token usage JSON contains conflicting aliased token counts".to_string())
        }
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn usage_u64(object: &Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    value
        .as_u64()
        .map(Some)
        .ok_or_else(|| "token usage counts must be non-negative integers".to_string())
}

fn usage_bool(value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| "token usage success must be true or false".to_string())
}

fn usage_test_outcome(value: &Value) -> Result<TestOutcome, String> {
    let outcome = value
        .as_str()
        .ok_or_else(|| "token usage test_outcome must be a string".to_string())?;
    TestOutcome::parse(outcome)
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

fn handle_init<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let repository_root = repository_root(current_dir, options.project_path.as_deref());
    let state_dir_override = state_dir_override(env_lookup);
    let request = RepositoryLifecycleInitRequest {
        path: repository_root.clone(),
        state_dir_override: state_dir_override.clone(),
        write_root_gitignore: options.write_gitignore,
    };

    match init_repository(request) {
        Ok(outcome) => {
            let mut status = runtime
                .repository_status(RepositoryStatusRequest {
                    path: repository_root.clone(),
                    state_dir_override: state_dir_override.clone(),
                })
                .ok();
            if !options.resync && !options.autosync {
                let progress = init_progress_stderr(options);
                if options.json {
                    return CliOutput::success_with_stderr(
                        init_outcome_json(&outcome, status.as_ref()),
                        progress,
                    );
                }
                return CliOutput::success_with_stderr(
                    init_outcome_human(&outcome, status.as_ref()),
                    progress,
                );
            }

            if options.autosync
                && !options.resync
                && !init_status_has_readable_active_generation(status.as_ref())
            {
                return init_bootstrap_failure(
                    options,
                    InitBootstrapState {
                        outcome: &outcome,
                        status: status.as_ref(),
                        resync_outcome: None,
                        autosync_report: None,
                    },
                    "autosync",
                    "autosync start requires an active index generation",
                    "run repogrammar init --yes --resync --autosync",
                );
            }

            let mut resync_outcome = None;
            if options.resync {
                let index_request = match build_cli_index_request(options, current_dir, env_lookup)
                {
                    Ok(request) => request,
                    Err(error) => return lifecycle_error("init", options.json, error),
                };
                match runtime.index_repository("resync", index_request) {
                    Ok(outcome) => {
                        resync_outcome = Some(outcome);
                        status = runtime
                            .repository_status(RepositoryStatusRequest {
                                path: repository_root.clone(),
                                state_dir_override: state_dir_override.clone(),
                            })
                            .ok();
                    }
                    Err(error) => {
                        return init_bootstrap_failure(
                            options,
                            InitBootstrapState {
                                outcome: &outcome,
                                status: status.as_ref(),
                                resync_outcome: None,
                                autosync_report: None,
                            },
                            "resync",
                            &error.to_string(),
                            "run repogrammar doctor, then retry repogrammar init --yes --resync --autosync",
                        );
                    }
                }
            }

            let mut autosync_report = None;
            if options.autosync {
                let autosync_request = build_cli_autosync_request(options, current_dir, env_lookup);
                match runtime.autosync(AutosyncCommand::Start, autosync_request) {
                    Ok(report) => autosync_report = Some(report),
                    Err(error) => {
                        return init_bootstrap_failure(
                            options,
                            InitBootstrapState {
                                outcome: &outcome,
                                status: status.as_ref(),
                                resync_outcome: resync_outcome.as_ref(),
                                autosync_report: None,
                            },
                            "autosync",
                            &error.to_string(),
                            "run repogrammar autosync start after resolving the reported issue",
                        );
                    }
                }
            }

            let progress = init_progress_stderr(options);
            if options.json {
                CliOutput::success_with_stderr(
                    init_bootstrap_json(
                        options,
                        &outcome,
                        status.as_ref(),
                        resync_outcome.as_ref(),
                        autosync_report.as_ref(),
                    ),
                    progress,
                )
            } else {
                CliOutput::success_with_stderr(
                    init_bootstrap_human(
                        &outcome,
                        status.as_ref(),
                        resync_outcome.as_ref(),
                        autosync_report.as_ref(),
                    ),
                    progress,
                )
            }
        }
        Err(error) => lifecycle_error("init", options.json, error),
    }
}

fn init_progress_stderr(options: &LifecycleOptions) -> String {
    if !should_emit_progress(
        options.progress,
        options.json,
        options.quiet,
        std::io::stderr().is_terminal(),
    ) {
        return String::new();
    }
    let event = ProgressEvent::new(
        ProgressStage::PersistenceValidation,
        "repository state initialized",
        WorkUnits::known(1, 1).expect("init progress uses valid known work units"),
    );
    render_index_progress_event("init", &event, options.json)
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
    let _accepted_verbosity_flags = (options.quiet, options.verbose);
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

fn handle_prune<F>(
    options: &PruneOptions,
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
    let prune_request = GenerationPruneRequest {
        keep_inactive: options.keep_inactive,
        dry_run: options.dry_run,
    };

    match runtime.prune_generations(request, prune_request) {
        Ok(report) if options.json => CliOutput::success(prune_report_json(&report)),
        Ok(report) => CliOutput::success(prune_report_human(&report)),
        Err(error) => lifecycle_error("prune", options.json, error),
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
    let request = match build_cli_index_request(options, current_dir, env_lookup) {
        Ok(request) => request,
        Err(error) => {
            return lifecycle_error(command, options.json, error);
        }
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

fn handle_autosync<F>(
    options: &AutosyncOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let request = build_autosync_request(
        options.project_path.as_deref(),
        options.json,
        options.quiet,
        options.poll_ms,
        options.debounce_ms,
        current_dir,
        env_lookup,
    );
    match runtime.autosync(options.command, request) {
        Ok(report) if options.json => CliOutput::success(autosync_json(options.command, &report)),
        Ok(report) => CliOutput::success(autosync_human(options.command, &report, options)),
        Err(error) => lifecycle_error("autosync", options.json, error),
    }
}

fn build_cli_index_request<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
) -> Result<CliIndexRequest, RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    let semantic_worker_executable =
        env_lookup("REPOGRAMMAR_TYPESCRIPT_WORKER").filter(|value| !value.trim().is_empty());
    let semantic_worker_args =
        semantic_worker_args_from_env_lookup(env_lookup).map_err(RepoGrammarError::InvalidInput)?;
    if semantic_worker_executable.is_none() && !semantic_worker_args.is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON requires REPOGRAMMAR_TYPESCRIPT_WORKER"
                .to_string(),
        ));
    }
    Ok(CliIndexRequest {
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
    })
}

fn build_cli_autosync_request<F>(
    options: &LifecycleOptions,
    current_dir: &Path,
    env_lookup: &F,
) -> CliAutosyncRequest
where
    F: Fn(&str) -> Option<String>,
{
    let settings = AutosyncSettings::default();
    build_autosync_request(
        options.project_path.as_deref(),
        options.json,
        options.quiet,
        settings.poll_ms,
        settings.debounce_ms,
        current_dir,
        env_lookup,
    )
}

fn build_autosync_request<F>(
    project_path: Option<&str>,
    json: bool,
    quiet: bool,
    poll_ms: u64,
    debounce_ms: u64,
    current_dir: &Path,
    env_lookup: &F,
) -> CliAutosyncRequest
where
    F: Fn(&str) -> Option<String>,
{
    CliAutosyncRequest {
        repository_root: repository_root(current_dir, project_path),
        state_dir_override: state_dir_override(env_lookup),
        strict_gitignore: env_flag_enabled(env_lookup, "REPOGRAMMAR_STRICT_GITIGNORE"),
        poll_ms,
        debounce_ms,
        json,
        quiet,
    }
}

fn init_status_has_readable_active_generation(status: Option<&RepositoryStatusReport>) -> bool {
    let Some(status) = status else {
        return false;
    };
    let RepositoryStatus::Initialized { active_generation } = &status.status else {
        return false;
    };
    active_generation != "none"
        && active_generation != "not implemented"
        && matches!(
            status.indexing,
            RepositoryImplementationStatus::FileManifestOnly
                | RepositoryImplementationStatus::SyntaxOnlyCodeUnits
        )
}

pub fn semantic_worker_args_from_env_lookup<F>(env_lookup: &F) -> Result<Vec<String>, String>
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
    resync: bool,
    autosync: bool,
    force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PruneOptions {
    project_path: Option<String>,
    keep_inactive: usize,
    dry_run: bool,
    yes: bool,
    json: bool,
    quiet: bool,
    verbose: bool,
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
            resync: false,
            autosync: false,
            force: false,
        }
    }
}

impl Default for PruneOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            keep_inactive: DEFAULT_RETAINED_INACTIVE_GENERATIONS,
            dry_run: false,
            yes: false,
            json: false,
            quiet: false,
            verbose: false,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutosyncOptions {
    command: AutosyncCommand,
    project_path: Option<String>,
    json: bool,
    quiet: bool,
    poll_ms: u64,
    debounce_ms: u64,
}

impl Default for AutosyncOptions {
    fn default() -> Self {
        Self {
            command: AutosyncCommand::Status,
            project_path: None,
            json: false,
            quiet: false,
            poll_ms: AutosyncSettings::default().poll_ms,
            debounce_ms: AutosyncSettings::default().debounce_ms,
        }
    }
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
            "--progress" if matches!(command, "init" | "index" | "sync" | "resync") => {
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
            "--yes" if matches!(command, "init" | "uninit" | "unlock") => {
                options.yes = true;
                index += 1;
            }
            "--resync" if command == "init" => {
                options.resync = true;
                index += 1;
            }
            "--autosync" if command == "init" => {
                options.autosync = true;
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

fn parse_prune_options(rest: &[String]) -> Result<PruneOptions, String> {
    let mut options = PruneOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--keep" => {
                let value = option_value(rest, index, "--keep", "a non-negative integer")?;
                options.keep_inactive = parse_nonnegative_usize(value, "--keep")?;
                index += 2;
            }
            "--dry-run" => {
                options.dry_run = true;
                index += 1;
            }
            "--yes" => {
                options.yes = true;
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
            other => return Err(format!("unknown prune option: {other}")),
        }
    }
    if !options.dry_run && !options.yes {
        return Err("prune requires --yes unless --dry-run is present".to_string());
    }
    Ok(options)
}

fn parse_autosync_options(rest: &[String]) -> Result<AutosyncOptions, String> {
    let mut options = AutosyncOptions::default();
    let mut index = 0;
    if let Some(first) = rest.first().filter(|value| !value.starts_with('-')) {
        options.command = AutosyncCommand::parse(first)?;
        index = 1;
    }
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
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
            "--poll-ms" => {
                let value = option_value(rest, index, "--poll-ms", "milliseconds")?;
                options.poll_ms = parse_autosync_millis("--poll-ms", value, 100, 600_000)?;
                index += 2;
            }
            "--debounce-ms" => {
                let value = option_value(rest, index, "--debounce-ms", "milliseconds")?;
                options.debounce_ms = parse_autosync_millis("--debounce-ms", value, 0, 60_000)?;
                index += 2;
            }
            "--progress" => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                ProgressMode::parse(value)?;
                index += 2;
            }
            other if !other.starts_with('-') => {
                return Err(format!("unexpected autosync argument: {other}"));
            }
            other => return Err(format!("unknown autosync option: {other}")),
        }
    }
    Ok(options)
}

fn parse_autosync_millis(option: &str, value: &str, min: u64, max: u64) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{option} requires an integer"))?;
    if parsed < min || parsed > max {
        return Err(format!("{option} must be between {min} and {max}"));
    }
    Ok(parsed)
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

fn parse_nonnegative_usize(value: &str, option: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{option} requires a non-negative integer"))
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

fn init_outcome_human(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
) -> String {
    let mut output = format!(
        "init: repository-local state ready\nstate_dir: {}\ncreated: {}\ngit_info_exclude: {}\nroot_gitignore: {}\nstorage: {}\nindexing: {}\n",
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
        },
        implementation_status(init_storage_status(outcome, status)),
        implementation_status(init_indexing_status(outcome, status))
    );
    for entry in &outcome.repaired_entries {
        output.push_str("repaired_entry: ");
        output.push_str(entry);
        output.push('\n');
    }
    output
}

fn init_outcome_json(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
) -> String {
    json_line(init_outcome_value(outcome, status))
}

fn init_outcome_value(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
) -> serde_json::Value {
    json!({
        "command": "init",
        "status": "initialized",
        "state_dir": outcome.state_dir,
        "created": outcome.created,
        "git_info_exclude_updated": outcome.git_info_exclude_updated,
        "root_gitignore_updated": outcome.root_gitignore_updated,
        "storage": implementation_status(init_storage_status(outcome, status)),
        "indexing": implementation_status(init_indexing_status(outcome, status)),
        "repaired_entries": outcome.repaired_entries,
    })
}

fn init_bootstrap_json(
    options: &LifecycleOptions,
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
    resync_outcome: Option<&IndexingOutcome>,
    autosync_report: Option<&AutosyncReport>,
) -> String {
    let mut value = init_outcome_value(outcome, status);
    value["bootstrap"] = json!({
        "resync_requested": resync_outcome.is_some(),
        "autosync_requested": autosync_report.is_some(),
    });
    value["resync"] = resync_outcome
        .map(|outcome| index_outcome_value("resync", outcome, options))
        .unwrap_or(Value::Null);
    value["autosync"] = autosync_report
        .map(|report| autosync_value(AutosyncCommand::Start, report))
        .unwrap_or(Value::Null);
    json_line(value)
}

fn init_bootstrap_human(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
    resync_outcome: Option<&IndexingOutcome>,
    autosync_report: Option<&AutosyncReport>,
) -> String {
    let mut output = init_outcome_human(outcome, status);
    if let Some(outcome) = resync_outcome {
        output.push_str(&format!(
            "resync: complete\nactive_generation: {}\nindexed_units: {}\n",
            outcome.active_generation.as_deref().unwrap_or("none"),
            outcome.indexed_units
        ));
    }
    if let Some(report) = autosync_report {
        output.push_str(&format!(
            "autosync: started\nrunning: {}\nenabled: {}\n",
            report.running, report.enabled
        ));
    }
    output
}

struct InitBootstrapState<'a> {
    outcome: &'a RepositoryInitOutcome,
    status: Option<&'a RepositoryStatusReport>,
    resync_outcome: Option<&'a IndexingOutcome>,
    autosync_report: Option<&'a AutosyncReport>,
}

fn init_bootstrap_failure(
    options: &LifecycleOptions,
    state: InitBootstrapState<'_>,
    failed_step: &str,
    reason: &str,
    guidance: &str,
) -> CliOutput {
    if options.json {
        let mut value = init_outcome_value(state.outcome, state.status);
        value["status"] = json!("error");
        value["failed_step"] = json!(failed_step);
        value["reason"] = json!(reason);
        value["guidance"] = json!(guidance);
        value["resync"] = state
            .resync_outcome
            .map(|outcome| index_outcome_value("resync", outcome, options))
            .unwrap_or(Value::Null);
        value["autosync"] = state
            .autosync_report
            .map(|report| autosync_value(AutosyncCommand::Start, report))
            .unwrap_or(Value::Null);
        return CliOutput::failure(2, json_line(value));
    }

    CliOutput::failure(
        2,
        format!(
            "{}{failed_step}: error\nreason: {reason}\nguidance: {guidance}\n",
            init_bootstrap_human(
                state.outcome,
                state.status,
                state.resync_outcome,
                state.autosync_report
            )
        ),
    )
}

fn init_storage_status(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
) -> RepositoryImplementationStatus {
    status
        .map(|report| report.storage)
        .unwrap_or(outcome.storage)
}

fn init_indexing_status(
    outcome: &RepositoryInitOutcome,
    status: Option<&RepositoryStatusReport>,
) -> RepositoryImplementationStatus {
    status
        .map(|report| report.indexing)
        .unwrap_or(outcome.indexing)
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
    json_line(index_outcome_value(command, outcome, options))
}

fn index_outcome_value(
    command: &str,
    outcome: &IndexingOutcome,
    options: &LifecycleOptions,
) -> serde_json::Value {
    json!({
        "command": command,
        "status": "complete",
        "generation_id": outcome.active_generation,
        "active_generation": outcome.active_generation,
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
    })
}

fn prune_report_human(report: &GenerationPruneReport) -> String {
    let status = if report.dry_run {
        "dry_run"
    } else {
        "complete"
    };
    let mut output = format!(
        "prune: {status}\nactive_generation: {}\nkeep_inactive: {}\nretained_inactive_generations: {}\ncandidate_generations: {}\ndeleted_generations: {}\n",
        report.active_generation,
        report.keep_inactive,
        report.retained_inactive_generations.len(),
        report.candidate_generations.len(),
        report.deleted_generations.len()
    );
    for generation in &report.retained_inactive_generations {
        output.push_str("retained_inactive_generation: ");
        output.push_str(generation);
        output.push('\n');
    }
    for generation in &report.candidate_generations {
        output.push_str(if report.dry_run {
            "would_delete_generation: "
        } else {
            "candidate_generation: "
        });
        output.push_str(generation);
        output.push('\n');
    }
    for generation in &report.deleted_generations {
        output.push_str("deleted_generation: ");
        output.push_str(generation);
        output.push('\n');
    }
    output
}

fn prune_report_json(report: &GenerationPruneReport) -> String {
    json_line(json!({
        "command": "prune",
        "status": if report.dry_run { "dry_run" } else { "complete" },
        "active_generation": report.active_generation,
        "keep_inactive": report.keep_inactive,
        "dry_run": report.dry_run,
        "retained_inactive_generations": report.retained_inactive_generations,
        "candidate_generations": report.candidate_generations,
        "deleted_generations": report.deleted_generations,
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
        "dependency_records: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.dependency_record_count)
            .map(|count| count.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    output.push_str(&format!(
        "dirty_records: {}\n",
        report
            .storage_inspection
            .as_ref()
            .and_then(|inspection| inspection.dirty_record_count)
            .map(|count| count.to_string())
            .unwrap_or_else(|| "none".to_string())
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
        "dependency_records": storage_inspection.and_then(|inspection| inspection.dependency_record_count),
        "dirty_records": storage_inspection.and_then(|inspection| inspection.dirty_record_count),
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
            "dependency_records": storage_inspection.and_then(|inspection| inspection.dependency_record_count),
            "dirty_records": storage_inspection.and_then(|inspection| inspection.dirty_record_count),
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
    let component = options.component.as_deref().unwrap_or("daemon");
    json_line(json!({
        "command": "logs",
        "state_dir": outcome.state_dir,
        "available": outcome.available,
        "redacted": outcome.redacted,
        "paths": "repo_relative_only",
        "component": component,
        "component_filter": options.component,
        "tail": options.tail,
        "since": options.since,
        "entries": outcome.entries,
        "message": outcome.message,
    }))
}

fn autosync_human(
    command: AutosyncCommand,
    report: &AutosyncReport,
    options: &AutosyncOptions,
) -> String {
    if options.quiet {
        return String::new();
    }
    let mut output = format!(
        "autosync: {}\ncommand: {}\nstate_dir: {}\nenabled: {}\nrunning: {}\n",
        report.message,
        command.as_str(),
        report.state_dir,
        report.enabled,
        report.running
    );
    if let Some(pid) = report.pid {
        output.push_str(&format!("pid: {pid}\n"));
    }
    output.push_str(&format!(
        "poll_ms: {}\ndebounce_ms: {}\n",
        report.poll_ms, report.debounce_ms
    ));
    if let Some(run) = &report.last_run {
        output.push_str(&format!(
            "last_sync_unix_seconds: {}\nlast_sync_result: {}\n",
            run.last_sync_unix_seconds,
            run.result.as_str()
        ));
        if let Some(generation) = &run.synced_generation {
            output.push_str(&format!("last_sync_generation: {generation}\n"));
        }
        if let Some(error) = &run.error {
            output.push_str(&format!("last_sync_error: {error}\n"));
        }
    }
    output
}

fn autosync_json(command: AutosyncCommand, report: &AutosyncReport) -> String {
    json_line(autosync_value(command, report))
}

fn autosync_value(command: AutosyncCommand, report: &AutosyncReport) -> serde_json::Value {
    json!({
        "command": "autosync",
        "subcommand": command.as_str(),
        "status": "complete",
        "state_dir": report.state_dir,
        "enabled": report.enabled,
        "running": report.running,
        "pid": report.pid,
        "poll_ms": report.poll_ms,
        "debounce_ms": report.debounce_ms,
        "last_run": report.last_run.as_ref().map(|run| json!({
            "last_sync_unix_seconds": run.last_sync_unix_seconds,
            "result": run.result.as_str(),
            "synced_generation": run.synced_generation,
            "error": run.error,
        })),
        "message": report.message,
    })
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
        WorkUnits::Known(work) => {
            let width = 20u64;
            let filled = if work.total() == 0 {
                width
            } else {
                (work.completed().saturating_mul(width) / work.total()).min(width)
            };
            let empty = width.saturating_sub(filled);
            format!(
                "[{}{}] {}% {}/{}",
                "#".repeat(filled as usize),
                "-".repeat(empty as usize),
                work.percent(),
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
        list_code_units, list_families, list_indexed_files, lookup_family_with_local_context,
        FamilySummary,
    };
    use crate::application::repository::{acquire_index_lock, DEFAULT_STATE_DIR};
    use crate::application::repository::{
        repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    };
    use crate::ports::index_store::STORAGE_SCHEMA_VERSION;
    use crate::test_support::TempWorkspace;
    use rusqlite::{params, Connection};
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
        assert!(human.contains("index: [##########----------] 50% 2/4 file_scanning"));
        assert!(!human.to_ascii_lowercase().contains("eta"));

        let machine = render_index_progress_event("index", &event, true);
        let value: Value = serde_json::from_str(machine.trim()).expect("progress NDJSON");
        assert_eq!(value["stage"], "file_scanning");
        assert_eq!(value["message"], "stored files");
        assert_eq!(value["work"]["completed"], 2);
        assert_eq!(value["work"]["total"], 4);
        assert_eq!(value["work"]["percent"], 50);
    }

    #[test]
    fn unknown_progress_renderer_remains_indeterminate_without_percentages() {
        let event = ProgressEvent::new(
            crate::application::progress::ProgressStage::SemanticResolution,
            "waiting for worker",
            WorkUnits::Unknown,
        );

        let human = render_index_progress_event("sync", &event, false);
        assert!(human.contains("sync: [working] semantic_resolution"));
        assert!(!human.contains('%'));
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
            assert!(matches!(command, "index" | "sync" | "resync"));
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

    #[derive(Default)]
    struct PruneRuntime {
        last_request: RefCell<Option<RepositoryStatusRequest>>,
        last_prune: RefCell<Option<GenerationPruneRequest>>,
    }

    impl CliRuntime for PruneRuntime {
        fn index_repository(
            &self,
            _command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            unreachable!("prune command should not index")
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("prune command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("prune command should not request doctor through CLI test runtime")
        }

        fn prune_generations(
            &self,
            request: RepositoryStatusRequest,
            prune: GenerationPruneRequest,
        ) -> Result<GenerationPruneReport, RepoGrammarError> {
            self.last_request.replace(Some(request));
            self.last_prune.replace(Some(prune));
            Ok(GenerationPruneReport {
                active_generation: "gen-000004".to_string(),
                keep_inactive: prune.keep_inactive,
                retained_inactive_generations: if prune.keep_inactive == 0 {
                    Vec::new()
                } else {
                    vec!["gen-000003".to_string()]
                },
                candidate_generations: vec!["gen-000001".to_string(), "gen-000002".to_string()],
                deleted_generations: if prune.dry_run {
                    Vec::new()
                } else {
                    vec!["gen-000001".to_string(), "gen-000002".to_string()]
                },
                dry_run: prune.dry_run,
            })
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
                            "run repogrammar resync after adding compatible implementations"
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
                token_saving_readiness: TokenSavingReadiness::Partial,
                blocking_reasons: Vec::new(),
                interpretation:
                    "RepoGrammar can provide integration-pattern context when repeated local patterns exist; third-party-heavy or thin-wrapper repositories may see lower token-saving potential.",
            })
        }
    }

    #[derive(Default)]
    struct BootstrapRuntime {
        index_calls: Cell<usize>,
        autosync_calls: Cell<usize>,
        indexed: Cell<bool>,
        active_before_index: bool,
        fail_index: bool,
        fail_autosync: bool,
        last_index_command: RefCell<Option<String>>,
    }

    impl BootstrapRuntime {
        fn active_before_index() -> Self {
            Self {
                active_before_index: true,
                ..Self::default()
            }
        }

        fn fail_autosync() -> Self {
            Self {
                fail_autosync: true,
                ..Self::default()
            }
        }
    }

    impl CliRuntime for BootstrapRuntime {
        fn index_repository(
            &self,
            command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            self.index_calls.set(self.index_calls.get() + 1);
            self.last_index_command.replace(Some(command.to_string()));
            if self.fail_index {
                return Err(RepoGrammarError::InvalidInput(
                    "synthetic resync failure".to_string(),
                ));
            }
            self.indexed.set(true);
            Ok(IndexingOutcome {
                indexed_units: 3,
                semantic_facts: 0,
                discovered_files: 2,
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
            let active = self.indexed.get() || self.active_before_index;
            Ok(RepositoryStatusReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                status: RepositoryStatus::Initialized {
                    active_generation: if active {
                        "gen-000001".to_string()
                    } else {
                        "none".to_string()
                    },
                },
                manifest: RepositoryManifestStatus::Valid,
                manifest_schema_version: Some(1),
                missing_subdirs: Vec::new(),
                storage: if active {
                    RepositoryImplementationStatus::Available
                } else {
                    RepositoryImplementationStatus::NotImplemented
                },
                indexing: if active {
                    RepositoryImplementationStatus::SyntaxOnlyCodeUnits
                } else {
                    RepositoryImplementationStatus::NotImplemented
                },
                storage_inspection: None,
                storage_error: None,
            })
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("bootstrap tests do not call doctor")
        }

        fn autosync(
            &self,
            command: AutosyncCommand,
            request: CliAutosyncRequest,
        ) -> Result<AutosyncReport, RepoGrammarError> {
            self.autosync_calls.set(self.autosync_calls.get() + 1);
            assert_eq!(command, AutosyncCommand::Start);
            if self.fail_autosync {
                return Err(RepoGrammarError::InvalidInput(
                    "synthetic autosync failure".to_string(),
                ));
            }
            Ok(AutosyncReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                enabled: true,
                running: true,
                pid: Some(1234),
                poll_ms: request.poll_ms,
                debounce_ms: request.debounce_ms,
                last_run: None,
                message: "autosync start ok".to_string(),
            })
        }
    }

    #[derive(Default)]
    struct AutosyncRequestRuntime {
        last_command: Cell<Option<AutosyncCommand>>,
        last_request: RefCell<Option<CliAutosyncRequest>>,
    }

    impl CliRuntime for AutosyncRequestRuntime {
        fn index_repository(
            &self,
            _command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            unreachable!("autosync command should not call index directly through CLI test runtime")
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("autosync command should not request status directly")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("autosync command should not request doctor directly")
        }

        fn autosync(
            &self,
            command: AutosyncCommand,
            request: CliAutosyncRequest,
        ) -> Result<AutosyncReport, RepoGrammarError> {
            self.last_command.set(Some(command));
            self.last_request.replace(Some(request.clone()));
            Ok(AutosyncReport {
                state_dir: ".repogrammar".to_string(),
                enabled: matches!(
                    command,
                    AutosyncCommand::Enable
                        | AutosyncCommand::Start
                        | AutosyncCommand::Status
                        | AutosyncCommand::Run
                ),
                running: matches!(command, AutosyncCommand::Start | AutosyncCommand::Run),
                pid: matches!(command, AutosyncCommand::Start | AutosyncCommand::Run)
                    .then_some(1234),
                poll_ms: request.poll_ms,
                debounce_ms: request.debounce_ms,
                last_run: None,
                message: format!("autosync {} ok", command.as_str()),
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
                        "repository is not initialized; run repogrammar init --yes".to_string(),
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
            lookup_family_with_local_context(&store, &store, target, mode)
        }
    }

    fn indexed_paths(workspace: &TempWorkspace, generation_id: &str) -> Vec<String> {
        let database = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("repogrammar.sqlite");
        let connection = Connection::open(database).expect("open repository database");
        let paths = connection
            .prepare("SELECT path FROM indexed_files WHERE generation_id = ?1 ORDER BY path")
            .expect("prepare indexed paths")
            .query_map(params![generation_id], |row| row.get::<_, String>(0))
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
    fn top_level_help_lists_usage_options_and_topics() {
        let output = run(["--help"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output
            .stdout
            .contains("Usage: repogrammar <command> [options]"));
        assert!(output.stdout.contains("repogrammar help <command>"));
        assert!(output
            .stdout
            .contains("autosync <status|enable|start|stop|disable|run>"));
        assert!(output.stdout.contains("resync [--project <path>]"));
        assert!(output
            .stdout
            .contains("repogrammar init --yes --resync --autosync"));
        assert!(output.stdout.contains("install [--target <agent[,agent]>]"));
        assert!(output
            .stdout
            .contains("telemetry <status|on|off|export|upload|purge|research-*|experiment-*>"));
    }

    #[test]
    fn init_help_lists_combined_bootstrap_options() {
        let output = run(["help", "init"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.contains("[--resync] [--autosync]"));
        assert!(output.stdout.contains("Use --resync"));
        assert!(output
            .stdout
            .contains("requires --resync or an existing active generation"));
    }

    #[test]
    fn command_help_is_available_from_help_topic_and_help_flags() {
        let by_topic = run(["help", "autosync"]);
        let by_short_flag = run(["autosync", "-h"]);
        let by_long_flag = run(["autosync", "--help"]);

        for output in [by_topic, by_short_flag, by_long_flag] {
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            assert!(output
                .stdout
                .contains("Usage: repogrammar autosync [status|enable|start|stop|disable|run]"));
            assert!(output.stdout.contains("Subcommands are positional"));
            assert!(output.stdout.contains("repogrammar autosync start"));
            assert!(output.stdout.contains("not `repogrammar autosync --start`"));
            assert!(output.stdout.contains("--poll-ms <n>"));
            assert!(output.stdout.contains("--debounce-ms <n>"));
        }
    }

    #[test]
    fn command_help_does_not_call_command_runtime() {
        let runtime = AutosyncRequestRuntime::default();
        let env = |_: &str| None;
        let output =
            run_with_context_and_runtime(["autosync", "--help"], Path::new("."), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(runtime.last_command.get().is_none());
        assert!(runtime.last_request.borrow().is_none());
    }

    #[test]
    fn unknown_help_topic_is_rejected() {
        let output = run(["help", "missing-command"]);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, "unknown help topic: missing-command\n");
    }

    #[test]
    fn pattern_family_command_surface_is_recognized() {
        let workspace = TempWorkspace::new("cli-query-surface");
        let env = |_: &str| None;
        for command in ["find", "families", "family", "member", "explain", "check"] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init --yes\n"
            ));
            assert!(output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
        for command in ["files", "units"] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar init --yes\n"
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
    fn resync_request_uses_sync_indexing_path_for_any_repository() {
        let workspace = TempWorkspace::new("cli-resync-semantic-worker-env");
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
            ["resync", "--json"],
            workspace.path(),
            &env,
            &SemanticWorkerEnvRuntime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("resync JSON");
        assert_eq!(value["command"], "resync");
        assert_eq!(value["semantic_worker"], "complete");
        assert_eq!(value["semantic_facts"], 2);
    }

    #[test]
    fn prune_json_dry_run_reports_candidates_without_absolute_paths() {
        let workspace = TempWorkspace::new("cli-prune-dry-run");
        let env = |_: &str| None;
        let runtime = PruneRuntime::default();
        let output = run_with_context_and_runtime(
            ["prune", "--dry-run", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stderr.is_empty());
        let request = runtime
            .last_request
            .borrow()
            .clone()
            .expect("status request");
        assert!(request
            .path
            .starts_with(workspace.path().to_string_lossy().as_ref()));
        let prune = runtime.last_prune.borrow().expect("prune request");
        assert_eq!(prune.keep_inactive, DEFAULT_RETAINED_INACTIVE_GENERATIONS);
        assert!(prune.dry_run);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("prune JSON");
        assert_eq!(value["command"], "prune");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(
            value["keep_inactive"],
            DEFAULT_RETAINED_INACTIVE_GENERATIONS
        );
        assert_eq!(value["candidate_generations"][0], "gen-000001");
        assert_eq!(value["deleted_generations"].as_array().unwrap().len(), 0);
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn prune_requires_yes_without_dry_run() {
        let workspace = TempWorkspace::new("cli-prune-requires-yes");
        let env = |_: &str| None;
        let runtime = PruneRuntime::default();

        let output = run_with_context_and_runtime(["prune"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("prune requires --yes unless --dry-run is present"));
        assert!(runtime.last_prune.borrow().is_none());
    }

    #[test]
    fn prune_human_with_yes_reports_deleted_generations() {
        let workspace = TempWorkspace::new("cli-prune-human");
        let env = |_: &str| None;
        let runtime = PruneRuntime::default();
        let output = run_with_context_and_runtime(
            ["prune", "--yes", "--keep", "0"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        let prune = runtime.last_prune.borrow().expect("prune request");
        assert_eq!(prune.keep_inactive, 0);
        assert!(!prune.dry_run);
        assert!(output.stdout.contains("prune: complete\n"));
        assert!(output.stdout.contains("active_generation: gen-000004\n"));
        assert!(output.stdout.contains("deleted_generation: gen-000001\n"));
        assert!(output.stdout.contains("deleted_generation: gen-000002\n"));
    }

    #[test]
    fn prune_rejects_invalid_keep_value() {
        let workspace = TempWorkspace::new("cli-prune-invalid-keep");
        let env = |_: &str| None;
        let runtime = PruneRuntime::default();

        let output = run_with_context_and_runtime(
            ["prune", "--dry-run", "--keep", "not-a-number"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("--keep requires a non-negative integer"));
        assert!(runtime.last_prune.borrow().is_none());
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
    fn autosync_start_passes_settings_to_runtime() {
        let workspace = TempWorkspace::new("cli-autosync-start");
        let env = |_: &str| None;
        let runtime = AutosyncRequestRuntime::default();
        let output = run_with_context_and_runtime(
            [
                "autosync",
                "start",
                "--json",
                "--poll-ms",
                "250",
                "--debounce-ms",
                "125",
            ],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(runtime.last_command.get(), Some(AutosyncCommand::Start));
        let request = runtime
            .last_request
            .borrow()
            .clone()
            .expect("autosync request");
        assert_eq!(request.poll_ms, 250);
        assert_eq!(request.debounce_ms, 125);
        assert!(!request.strict_gitignore);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("autosync JSON");
        assert_eq!(value["command"], "autosync");
        assert_eq!(value["subcommand"], "start");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["running"], true);
        assert_eq!(value["pid"], 1234);
    }

    #[test]
    fn autosync_defaults_to_status_human_output() {
        let workspace = TempWorkspace::new("cli-autosync-status");
        let env = |_: &str| None;
        let runtime = AutosyncRequestRuntime::default();
        let output = run_with_context_and_runtime(["autosync"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("command: status\n"));
        assert_eq!(runtime.last_command.get(), Some(AutosyncCommand::Status));
    }

    #[test]
    fn autosync_rejects_invalid_poll_interval() {
        let workspace = TempWorkspace::new("cli-autosync-bad-poll");
        let env = |_: &str| None;
        let runtime = AutosyncRequestRuntime::default();
        let output = run_with_context_and_runtime(
            ["autosync", "start", "--poll-ms", "99"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert!(output
            .stderr
            .contains("--poll-ms must be between 100 and 600000"));
        assert_eq!(runtime.last_command.get(), None);
    }

    #[test]
    fn autosync_accepts_progress_for_long_running_compatibility() {
        let workspace = TempWorkspace::new("cli-autosync-progress");
        let env = |key: &str| match key {
            "REPOGRAMMAR_STRICT_GITIGNORE" => Some("true".to_string()),
            _ => None,
        };
        let runtime = AutosyncRequestRuntime::default();
        let output = run_with_context_and_runtime(
            ["autosync", "run", "--progress", "never", "--quiet"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert_eq!(runtime.last_command.get(), Some(AutosyncCommand::Run));
        let request = runtime
            .last_request
            .borrow()
            .clone()
            .expect("autosync request");
        assert!(request.strict_gitignore);
        assert!(request.quiet);
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
        assert_eq!(fallback["guidance"], "run repogrammar init --yes");
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
            assert_eq!(fallback["guidance"], "run repogrammar init --yes");
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
        assert_eq!(value["estimated_potential_token_savings"], 0);
        assert_eq!(
            value["estimated_potential_token_savings_metric"]["measurement_kind"],
            "ESTIMATED"
        );
        assert_eq!(
            value["estimated_potential_token_savings_metric"]["event_count"],
            0
        );
        assert!(value["estimated_potential_token_savings_metric"]["caveat"]
            .as_str()
            .expect("estimated caveat")
            .contains("not measured token savings"));
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
        assert_eq!(fallback["guidance"], "run repogrammar init --yes");
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
    fn family_query_json_returns_partial_context_for_indexed_target_without_family() {
        let workspace = TempWorkspace::new("cli-query-partial-context");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output = run_with_context_and_runtime(
            ["find", "a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value =
            serde_json::from_str(output.stdout.trim()).expect("partial context JSON");
        assert_eq!(value["command"], "find");
        assert_eq!(value["status"], "PARTIAL_CONTEXT");
        assert_eq!(value["resolved_target"]["kind"], "code_unit");
        assert_eq!(value["resolved_target"]["path"], "a.ts");
        assert_eq!(value["resolved_target"]["line"], Value::Null);
        assert_eq!(value["resolved_target"]["byte_range"], Value::Null);
        assert_eq!(value["resolved_target"]["family_id"], Value::Null);
        assert_eq!(value["resolved_target"]["candidate_paths"][0], "a.ts");
        assert_eq!(value["resolved_target"]["confidence"], "exact");
        assert_eq!(value["read_plan"]["source_snippets_included"], false);
        assert_eq!(value["read_plan"]["requires_source_before_edit"], true);
        assert_eq!(value["read_plan"]["items"][0]["path"], "a.ts");
        assert_eq!(
            value["unknowns"][0]["affected_claim"],
            "pattern family evidence for resolved target"
        );
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains("export const"));
    }

    #[test]
    fn family_check_json_partial_context_remains_advisory_without_proof_fields() {
        let workspace = TempWorkspace::new("cli-check-partial-context");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output = run_with_context_and_runtime(
            ["check", "a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("partial check JSON");
        assert_eq!(value["status"], "PARTIAL_CONTEXT");
        assert_eq!(value["check"]["advisory_status"], "UNKNOWN");
        assert_eq!(
            value["check"]["reason"],
            "runtime equivalence remains unproven"
        );
        assert!(value["check"].get("fail_on").is_none());
        assert!(value["check"].get("pass").is_none());
        assert!(value["check"].get("conforms").is_none());
    }

    #[test]
    fn family_query_compact_mode_omits_evidence_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-family-query-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);

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
        assert!(
            value["output"]["estimated_baseline_tokens"]
                .as_u64()
                .expect("estimated baseline")
                >= value["output"]["estimated_returned_tokens"]
                    .as_u64()
                    .expect("estimated returned")
        );
        assert!(
            value["output"]["estimated_potential_token_savings"]
                .as_u64()
                .expect("estimated potential")
                > 0
        );
        assert_eq!(
            value["output"]["estimated_potential_token_savings_kind"],
            "ESTIMATED"
        );
        assert!(value["output"]["estimated_potential_token_savings_caveat"]
            .as_str()
            .expect("estimated caveat")
            .contains("not measured token savings"));
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
        assert_eq!(value["read_plan"]["items"][0]["start_line"], 1);
        assert_eq!(value["read_plan"]["items"][0]["end_line"], 2);
        assert!(value["read_plan"]["line_range_omissions"]
            .as_array()
            .expect("line range omissions")
            .is_empty());
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
        assert!(human
            .stdout
            .contains("estimated_potential_token_savings_kind: ESTIMATED"));
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

        let local_metric = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("telemetry")
            .join("local-metrics")
            .join("estimated_potential_token_savings.json");
        let rollup: Value = serde_json::from_str(
            &fs::read_to_string(local_metric).expect("local estimated rollup"),
        )
        .expect("local estimated rollup JSON");
        assert_eq!(rollup["metric_name"], "estimated_potential_token_savings");
        assert_eq!(rollup["measurement_kind"], "ESTIMATED");
        assert_eq!(rollup["event_count"], 3);
        assert!(
            rollup["total_estimated_potential_token_savings"]
                .as_u64()
                .expect("total estimated potential")
                > 0
        );
        assert_eq!(
            json_file_count(
                &workspace
                    .path()
                    .join(DEFAULT_STATE_DIR)
                    .join("telemetry")
                    .join("queue")
            ),
            0
        );

        let stats =
            run_with_context_and_runtime(["stats", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(stats.status, 0);
        let stats_value: Value =
            serde_json::from_str(stats.stdout.trim()).expect("stats JSON after family query");
        assert_eq!(
            stats_value["estimated_potential_token_savings"],
            rollup["total_estimated_potential_token_savings"]
        );
        assert_eq!(stats_value["token_savings"], Value::Null);
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
        assert!(value["check"].get("fail_on").is_none());
        assert!(value["check"].get("pass").is_none());
        assert!(value["check"].get("conforms").is_none());
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
            assert_eq!(fallback["guidance"], "run repogrammar resync");
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
    fn files_and_units_ignore_legacy_pointer_when_mutable_index_exists() {
        let workspace = TempWorkspace::new("cli-files-units-legacy-pointer");
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
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            let value: Value =
                serde_json::from_str(output.stdout.trim()).expect("inventory output must be JSON");
            assert_eq!(value["active_generation"], "gen-000001");
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
    fn init_yes_is_agent_safe_confirmation_only() {
        let workspace = TempWorkspace::new("cli-init-agent-safe-yes");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "initialized");
        assert_eq!(value["root_gitignore_updated"], false);
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
        assert!(!workspace.path().join(".gitignore").exists());
        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 0);
    }

    #[test]
    fn init_resync_runs_index_and_reports_subresult() {
        let workspace = TempWorkspace::new("cli-init-resync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--resync", "--progress", "never", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 0);
        assert_eq!(
            runtime.last_index_command.borrow().as_deref(),
            Some("resync")
        );
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["resync"]["command"], "resync");
        assert_eq!(value["resync"]["progress"], "never");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["resync"]["active_generation"], "gen-000001");
        assert_eq!(value["bootstrap"]["resync_requested"], true);
        assert_eq!(value["bootstrap"]["autosync_requested"], false);
        assert_eq!(value["autosync"], Value::Null);
    }

    #[test]
    fn init_autosync_without_active_generation_requires_resync() {
        let workspace = TempWorkspace::new("cli-init-autosync-needs-resync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 0);
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("init error JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "error");
        assert_eq!(value["failed_step"], "autosync");
        assert_eq!(
            value["guidance"],
            "run repogrammar init --yes --resync --autosync"
        );
    }

    #[test]
    fn init_resync_autosync_starts_after_resync() {
        let workspace = TempWorkspace::new("cli-init-bootstrap-complete");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--resync", "--autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["autosync"]["subcommand"], "start");
        assert_eq!(value["autosync"]["running"], true);
        assert_eq!(value["bootstrap"]["resync_requested"], true);
        assert_eq!(value["bootstrap"]["autosync_requested"], true);
    }

    #[test]
    fn init_autosync_can_use_existing_active_generation() {
        let workspace = TempWorkspace::new("cli-init-autosync-existing-index");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::active_before_index();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["resync"], Value::Null);
        assert_eq!(value["autosync"]["running"], true);
    }

    #[test]
    fn init_preserves_resync_subresult_when_autosync_fails() {
        let workspace = TempWorkspace::new("cli-init-autosync-failure");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::fail_autosync();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--resync", "--autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("init error JSON");
        assert_eq!(value["failed_step"], "autosync");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["autosync"], Value::Null);
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("synthetic autosync failure"));
    }

    #[test]
    fn init_progress_always_emits_human_bar_percentage_on_stderr() {
        let workspace = TempWorkspace::new("cli-init-progress-human");
        let env = |_: &str| None;

        let output = run_with_context(["init", "--progress", "always"], workspace.path(), &env);

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("repository-local state ready"));
        assert!(output
            .stderr
            .contains("init: [####################] 100% 1/1 persistence_validation"));
        assert!(!output.stderr.to_ascii_lowercase().contains("eta"));
    }

    #[test]
    fn init_json_progress_always_keeps_result_on_stdout_and_ndjson_on_stderr() {
        let workspace = TempWorkspace::new("cli-init-progress-json");
        let env = |_: &str| None;

        let output = run_with_context(
            ["init", "--json", "--progress", "always"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        let result: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(result["command"], "init");
        let progress: Value =
            serde_json::from_str(output.stderr.trim()).expect("init progress NDJSON");
        assert_eq!(progress["stage"], "persistence_validation");
        assert_eq!(progress["work"]["kind"], "known");
        assert_eq!(progress["work"]["percent"], 100);
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
    fn init_rerun_reports_active_storage_and_indexing_status() {
        let workspace = TempWorkspace::new("cli-init-rerun-active-status");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let json_output =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(json_output.status, 0);
        let value: Value = serde_json::from_str(json_output.stdout.trim()).expect("init JSON");
        assert_eq!(value["created"], false);
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");

        let human_output = run_with_context_and_runtime(["init"], workspace.path(), &env, &runtime);

        assert_eq!(human_output.status, 0);
        assert!(human_output.stdout.contains("created: false"));
        assert!(human_output.stdout.contains("storage: available"));
        assert!(human_output
            .stdout
            .contains("indexing: syntax_only_code_units"));
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
            .join("repogrammar.sqlite")
            .is_file());
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
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
        assert!(workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("repogrammar.sqlite")
            .is_file());
        assert!(!workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("current-generation")
            .exists());
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
    fn resync_json_rebuilds_static_analysis_for_non_rust_repository() {
        let workspace = TempWorkspace::new("cli-resync-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace.path().join("b.ts"),
            "export function b(){ return 2; }\n",
        )
        .expect("write b");

        let output =
            run_with_context_and_runtime(["resync", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("resync JSON");
        assert_eq!(value["command"], "resync");
        assert_eq!(value["generation_id"], "gen-000002");
        assert_eq!(value["discovered_files"], 2);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 2);
        assert_eq!(
            indexed_paths(&workspace, "gen-000002"),
            vec!["a.ts", "b.ts"]
        );
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
        let cache = workspace.path().join(DEFAULT_STATE_DIR).join("cache");
        fs::remove_dir_all(&cache).expect("remove cache");

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
        assert!(!cache.exists());
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

        for command in ["index", "sync", "resync"] {
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
        let cache = workspace.path().join(DEFAULT_STATE_DIR).join("cache");
        fs::remove_dir_all(&cache).expect("remove cache");

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
            .contains("cache"));

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
        assert!(!cache.exists());
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
    fn doctor_reports_legacy_broken_active_generation_pointer_without_panic() {
        let workspace = TempWorkspace::new("cli-storage-broken-pointer");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(run_with_context(["init"], workspace.path(), &env).status, 0);
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
        assert_eq!(value["component"], "index");
        assert_eq!(value["component_filter"], "index");
        let entries = value["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 1);
        assert!(entries[0]
            .as_str()
            .expect("log entry")
            .contains("<redacted-path>"));
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
    fn install_environment_warnings_flag_multiple_copies_and_shadowed_authority() {
        let copies = vec![
            "/usr/local/cargo/bin/repogrammar".to_string(),
            "/home/u/.local/share/repogrammar/bin/repogrammar".to_string(),
        ];
        let warnings = install_environment_warnings(
            &copies,
            Some("/home/u/.local/bin/repogrammar"),
            Some("/home/u/.local/share/repogrammar/bin/repogrammar"),
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("multiple repogrammar executables")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("not the RepoGrammar-managed command")));
    }

    #[test]
    fn install_environment_warnings_quiet_for_single_authoritative_copy() {
        let copies = vec!["/home/u/.local/bin/repogrammar".to_string()];
        let warnings = install_environment_warnings(
            &copies,
            Some("/home/u/.local/bin/repogrammar"),
            Some("/home/u/.local/share/repogrammar/bin/repogrammar"),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn autosync_human_renders_last_sync_summary() {
        let report = AutosyncReport {
            state_dir: ".repogrammar".to_string(),
            enabled: true,
            running: true,
            pid: Some(42),
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: Some(crate::application::autosync::AutosyncRunReport {
                last_sync_unix_seconds: 1_700_000_000,
                result: crate::application::autosync::AutosyncRunResult::Ok,
                synced_generation: Some("gen-000007".to_string()),
                error: None,
            }),
            message: "auto-sync status".to_string(),
        };
        let output = autosync_human(
            AutosyncCommand::Status,
            &report,
            &AutosyncOptions::default(),
        );
        assert!(output.contains("last_sync_result: ok"), "{output}");
        assert!(
            output.contains("last_sync_generation: gen-000007"),
            "{output}"
        );
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
    fn install_dry_run_reports_deferred_instruction_plan() {
        let workspace = TempWorkspace::new("install-dry-run-instruction");
        let env = |_: &str| None;
        let output = run_with_context(
            [
                "install",
                "--target",
                "codex",
                "--scope",
                "global",
                "--dry-run",
                "--no-telemetry",
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("native_mcp: codex mcp add"));
        assert!(output.stdout.contains("instruction: deferred"));
        assert!(output.stdout.contains("REPOGRAMMAR_INSTRUCTION_FILE_CODEX"));
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
        let prompt = WizardPrompt::new(["2,1"], [""], [""]);

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
        assert!(prompt.confirmation_prompts.borrow()[0].contains("[Y/n]"));
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
    fn interactive_install_default_ignores_undetected_unmanaged_agent() {
        struct InstallRuntime {
            delegated: Cell<bool>,
        }

        impl CliRuntime for InstallRuntime {
            fn index_repository(
                &self,
                _command: &str,
                _request: CliIndexRequest,
            ) -> Result<IndexingOutcome, RepoGrammarError> {
                unreachable!("installer undetected default test")
            }

            fn repository_status(
                &self,
                _request: RepositoryStatusRequest,
            ) -> Result<RepositoryStatusReport, RepoGrammarError> {
                unreachable!("installer undetected default test")
            }

            fn repository_doctor(
                &self,
                _request: RepositoryDoctorRequest,
            ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
                unreachable!("installer undetected default test")
            }

            fn install_agent_integration(
                &self,
                _command: &str,
                _request: InstallRequest,
                _context: InstallExecutionContext,
            ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
                self.delegated.set(true);
                unreachable!("undetected default must stop before native writes")
            }
        }

        let workspace = TempWorkspace::new("cli-install-undetected-default");
        let command_dir = workspace.path().join("commands");
        fs::create_dir_all(&command_dir).expect("command dir");
        fs::write(command_dir.join("codex"), "").expect("fake codex cli");
        let data_home = workspace.path().join("data-home");
        let receipt_dir = data_home
            .join("repogrammar")
            .join("install")
            .join("receipts");
        fs::create_dir_all(&receipt_dir).expect("receipt dir");
        fs::write(
            receipt_dir.join("codex-global.json"),
            r#"{"schema_version":1,"managed_by":"repogrammar","mcp_server":"repogrammar","target":"codex","scope":"global"}"#,
        )
        .expect("codex receipt");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            "PATH" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime {
            delegated: Cell::new(false),
        };
        let prompt = WizardPrompt::new([""], [""], ["y"]);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output
            .stdout
            .contains("no detected uninstalled agent integrations were selected"));
        assert!(!runtime.delegated.get());
        assert_eq!(prompt.selection_calls.get(), 1);
        assert_eq!(prompt.telemetry_calls.get(), 0);
        assert_eq!(prompt.confirmation_calls.get(), 0);
        let selection_prompt = &prompt.selection_prompts.borrow()[0];
        assert!(selection_prompt.contains("Codex CLI"));
        assert!(selection_prompt.contains("detected     installed"));
        assert!(selection_prompt.contains("Claude Code"));
        assert!(selection_prompt.contains("not detected not installed"));
        assert!(
            selection_prompt.contains("a = all detected not-yet-installed agents (currently none)")
        );
        assert!(selection_prompt.contains("Selection [a]:"));
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
    fn interactive_install_confirmation_no_stops_before_runtime_writes() {
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
                unreachable!("confirmation no must stop before native writes")
            }
        }

        let workspace = TempWorkspace::new("cli-install-wizard-confirmation-no");
        let command_dir = workspace.path().join("commands");
        fs::create_dir_all(&command_dir).expect("command dir");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| match key {
            "XDG_DATA_HOME" => Some(data_home.display().to_string()),
            "REPOGRAMMAR_COMMAND_DIR" => Some(command_dir.display().to_string()),
            "PATH" => Some(command_dir.display().to_string()),
            _ => None,
        };
        let runtime = InstallRuntime {
            delegated: Cell::new(false),
        };
        let prompt = WizardPrompt::new(["1"], [""], ["n"]);

        let output =
            run_with_context_runtime_prompt(["install"], workspace.path(), &env, &runtime, &prompt);

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stdout.contains("install cancelled"));
        assert!(!runtime.delegated.get());
        assert_eq!(prompt.selection_calls.get(), 1);
        assert_eq!(prompt.telemetry_calls.get(), 1);
        assert_eq!(prompt.confirmation_calls.get(), 1);
        assert!(prompt.confirmation_prompts.borrow()[0].contains("[Y/n]"));
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
            parse_interactive_agent_selection("none", &statuses).expect("selection"),
            Some(Vec::new())
        );
        assert_eq!(
            parse_interactive_agent_selection("q", &statuses).expect("selection"),
            None
        );
        assert!(parse_interactive_agent_selection("unknown", &statuses).is_err());
        assert!(parse_interactive_agent_selection("1a", &statuses).is_err());
        assert!(parse_interactive_agent_selection("1,,2", &statuses).is_err());

        let statuses = vec![
            InstallAgentStatus {
                target: AgentTarget::Codex,
                detected: true,
                installed: true,
            },
            InstallAgentStatus {
                target: AgentTarget::ClaudeCode,
                detected: false,
                installed: false,
            },
        ];
        assert_eq!(
            parse_interactive_agent_selection("", &statuses).expect("selection"),
            Some(Vec::new())
        );
        assert_eq!(
            parse_interactive_agent_selection("a", &statuses).expect("selection"),
            Some(Vec::new())
        );
        assert_eq!(
            parse_interactive_agent_selection("2", &statuses).expect("selection"),
            Some(vec![AgentTarget::ClaudeCode])
        );

        let statuses = vec![
            InstallAgentStatus {
                target: AgentTarget::Codex,
                detected: true,
                installed: true,
            },
            InstallAgentStatus {
                target: AgentTarget::ClaudeCode,
                detected: false,
                installed: true,
            },
        ];
        assert_eq!(
            parse_interactive_agent_selection("", &statuses).expect("selection"),
            Some(vec![AgentTarget::Codex, AgentTarget::ClaudeCode])
        );
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
        confirmation_prompts: RefCell<Vec<String>>,
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
                confirmation_prompts: RefCell::new(Vec::new()),
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

        fn prompt_install_confirmation(&self, prompt: &str) -> Result<String, String> {
            self.confirmation_calls
                .set(self.confirmation_calls.get() + 1);
            self.confirmation_prompts
                .borrow_mut()
                .push(prompt.to_string());
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
    fn telemetry_experiment_record_imports_token_usage_json() {
        let workspace = TempWorkspace::new("cli-token-experiment-usage-json");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let baseline_usage = workspace.path().join("baseline-usage.json");
        let treatment_usage = workspace.path().join("treatment-usage.json");
        fs::write(
            &baseline_usage,
            json!({
                "schema_version": "repogrammar-token-usage.v1",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 40,
                    "total_tokens": 155
                },
                "success": true,
                "test_outcome": "passed"
            })
            .to_string(),
        )
        .expect("write baseline usage");
        fs::write(
            &treatment_usage,
            json!({
                "schema_version": "repogrammar-token-usage.v1",
                "prompt_tokens": 70,
                "completion_tokens": 30,
                "tool_tokens": 5,
                "success": true,
                "test_outcome": "passed"
            })
            .to_string(),
        )
        .expect("write treatment usage");

        for args in [
            vec![
                "telemetry".to_string(),
                "experiment-start".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
                "--experiment-mode".to_string(),
                "record-existing".to_string(),
                "--session".to_string(),
                "baseline".to_string(),
                "--measurement-source".to_string(),
                "host_reported".to_string(),
                "--yes".to_string(),
            ],
            vec![
                "telemetry".to_string(),
                "experiment-record".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
                "--usage-json".to_string(),
                baseline_usage.display().to_string(),
            ],
            vec![
                "telemetry".to_string(),
                "experiment-stop".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
            ],
            vec![
                "telemetry".to_string(),
                "experiment-start".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
                "--experiment-mode".to_string(),
                "record-existing".to_string(),
                "--session".to_string(),
                "treatment".to_string(),
                "--measurement-source".to_string(),
                "host_reported".to_string(),
                "--yes".to_string(),
            ],
            vec![
                "telemetry".to_string(),
                "experiment-record".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
                "--usage-json".to_string(),
                treatment_usage.display().to_string(),
            ],
            vec![
                "telemetry".to_string(),
                "experiment-stop".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
            ],
        ] {
            let output = run_with_context(args, workspace.path(), &env);
            assert_eq!(output.status, 0, "{output:?}");
            assert!(!output
                .stdout
                .contains(workspace.path().to_string_lossy().as_ref()));
            assert!(!output
                .stderr
                .contains(workspace.path().to_string_lossy().as_ref()));
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
        assert_eq!(value["measurement_status"], "paired_measurement_available");
        assert_eq!(value["baseline_total_tokens"], 155);
        assert_eq!(value["treatment_total_tokens"], 105);
        assert_eq!(value["token_savings"], 50);
        assert_eq!(value["measurement_source"], "host_reported");
        assert_eq!(value["correctness_comparison"], "both_success");
    }

    #[test]
    fn telemetry_experiment_record_rejects_raw_usage_json_fields() {
        let workspace = TempWorkspace::new("cli-token-experiment-usage-json-raw");
        let data_home = workspace.path().join("data-home");
        let env = |key: &str| {
            if key == "XDG_DATA_HOME" {
                Some(data_home.display().to_string())
            } else {
                None
            }
        };
        let usage = workspace.path().join("raw-usage.json");
        fs::write(
            &usage,
            json!({
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 3
                },
                "success": true,
                "messages": [
                    {
                        "role": "user",
                        "content": "secret prompt text"
                    }
                ]
            })
            .to_string(),
        )
        .expect("write raw usage");

        let output = run_with_context(
            vec![
                "telemetry".to_string(),
                "experiment-record".to_string(),
                "--name".to_string(),
                "task-a".to_string(),
                "--usage-json".to_string(),
                usage.display().to_string(),
            ],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("token usage JSON contains unsupported fields"));
        assert!(!output.stderr.contains("secret prompt text"));
        assert!(!output
            .stderr
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
        assert_eq!(value["guidance"], "run repogrammar init --yes");
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
