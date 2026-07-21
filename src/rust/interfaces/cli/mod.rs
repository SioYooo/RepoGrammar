//! CLI argument boundary for the `repogrammar` binary.

use crate::application::autosync::{AutosyncReport, AutosyncSettings};
use crate::application::conformance::{AlignmentComputation, ALIGNMENT_DEVIATION_CAP};
use crate::application::indexing::IndexingOutcome;
use crate::application::install::{
    binary_name, known_agent_targets, manage_instruction_file, normalize_concrete_targets,
    normalized_lexical_path, owned_install_receipt_exists, plan_install, resolve_instruction_file,
    supported_concrete_targets, target_adapter, targets_for_display, AgentIntegrationInspection,
    AgentTarget, InstallExecutionContext, InstallExecutionOutcome, InstallRequest, InstallScope,
    ManagedInstructionOperation, ManagedInstructionOutcome, ManagedInstructionRefusal,
    ManagedInstructionRequest, ManagedInstructionState, MANAGED_INSTRUCTION_VERSION,
};
#[cfg(test)]
use crate::application::install::{MANAGED_INSTRUCTION_BEGIN, MANAGED_INSTRUCTION_END};
use crate::application::progress::{ProgressEvent, ProgressStage, WorkUnits};
use crate::application::query::{
    bounded_family_members, build_read_plan, estimate_alignment_potential_token_savings,
    estimate_family_output_potential_token_savings,
    estimate_partial_context_potential_token_savings, family_query_abstention_reason,
    family_query_route_report, family_query_unknown_metric, found_outcome_token_savings,
    product_readiness_value, query_preflight, read_plan_with_rendered_spans,
    repository_status_unavailable_fallback, select_family_evidence, validate_query_target,
    validate_query_token_budget, AlignmentCertificateReport, DiagnosticSignal, FamilyDetailReport,
    FamilyEvidenceMode, FamilyFreshnessCounts, FamilyListReport, FamilyLookupMode,
    FamilyLookupReport, FamilyOutputOptions, FamilyPartialContextReport, FamilyQueryRouteReport,
    FamilyQueryUnknown, FamilyUnknownReport, IndexedCodeUnitsReport, IndexedFilesReport,
    OutcomeTokenSavings, ProductReadinessReport, QueryPreflightOperation, QueryPreflightReport,
    ReadPlan, ReadPlanItem, ReadPlanLineRangeOmission, RepoShapeDiagnosticsReport,
    RepoShapeLanguageDiagnostics, ResolvedQueryTarget, SelectedFamilyEvidence,
    SourceSpanRenderReport, TermRetrievalRoute, TokenSavingReadiness, UnknownInventoryBucket,
    UnknownInventoryReport, Verbosity, VerbosityTier, PRODUCT_SCHEMA_VERSION,
};
#[cfg(test)]
use crate::application::query::{
    ReadPlanPurpose, RenderedSourceSpan, SourceSpanOmission, SourceSpanPolicy,
    UnknownInventoryLanguageSummary,
};
use crate::application::query_resolution::Resolution;
use crate::application::recovery::{
    recovery_command, recovery_guidance, RecoveryEvidenceState, RecoveryFreshness, RecoveryHealth,
    RecoveryLockState,
};
#[cfg(test)]
use crate::application::recovery::{RecoveryAction, RecoveryReason, RecoveryRecommendation};
use crate::application::repository::{
    init_repository, repository_doctor, repository_freshness_for_report, repository_logs,
    repository_status, uninit_repository, unlock_repository, RepositoryDoctorCode,
    RepositoryDoctorFinding, RepositoryDoctorReport, RepositoryDoctorRequest,
    RepositoryDoctorSeverity, RepositoryImplementationStatus, RepositoryInitOutcome,
    RepositoryLifecycleInitRequest, RepositoryLogsReport, RepositoryLogsRequest,
    RepositoryManifestStatus, RepositoryReadiness, RepositoryReadinessState, RepositoryStatus,
    RepositoryStatusReport, RepositoryStatusRequest, RepositoryUninitOutcome,
    RepositoryUninitRequest, RepositoryUnlockReport, RepositoryUnlockRequest,
};
use crate::application::setup::{
    execute_setup, plan_setup, SetupAgentIntegrationState, SetupAgentMutation, SetupAgentState,
    SetupAuthorization, SetupAutosyncMutation, SetupConfirmation, SetupDisposition,
    SetupExecutionPort, SetupFailureClass, SetupFamilyInventory, SetupIndexSummary,
    SetupLimitation, SetupOperationError, SetupOutcome, SetupOutcomeStatus, SetupPlan,
    SetupPreservedResource, SetupProbe, SetupRepositoryMutation, SetupRepositoryState,
    SetupRequest, SetupStage, SetupStageStatus, SetupTarget,
};
use crate::application::storage::DEFAULT_RETAINED_INACTIVE_GENERATIONS;
use crate::application::telemetry::{
    experiment_export, experiment_purge, experiment_record, experiment_report,
    experiment_report_json, experiment_start, experiment_stop, export_anonymous_telemetry,
    family_query_metrics_rollup, latest_comparable_experiment_report, purge_telemetry,
    record_family_query_metric, record_passive_diagnostics_rollup, research_export, research_purge,
    savings_breakdown_map_json, set_anonymous_telemetry, set_research_trace,
    telemetry_disabled_by_environment, telemetry_status, upload_anonymous_telemetry,
    validate_telemetry_endpoint, ExperimentMode, ExperimentRecordRequest, ExperimentStartRequest,
    ExperimentWorkflowMode, FamilyQueryCommandCategory, FamilyQueryEntrypoint,
    FamilyQueryLookupMode, FamilyQueryMetricsRollup, FamilyQueryOutcomeRecord,
    FamilyQueryOutcomeStatus, FamilyQuerySavingsRecord, MeasurementSource, SavingsBreakdown,
    TelemetryDiagnostics, TelemetryExportReport, TelemetryPaths, TelemetryPurgeReport,
    TelemetryStatusReport, TelemetryUploadReceipt, TelemetryUploadReport, TelemetryUploadRequest,
    TelemetryUploadTransport, TestOutcome,
};
use crate::core::model::{
    EstimatedPotentialTokenSavings, FamilyConstraintProfile, FamilyPrevalence, MeasurementKind,
};
use crate::error::RepoGrammarError;
#[cfg(test)]
use crate::ports::index_store::LegacyLayoutCleanupReport;
use crate::ports::index_store::{
    GenerationPruneReport, GenerationPruneRequest, IndexCompactReport, IndexCompactRequest,
    IndexStorageLayout, IndexStorageSizeReport, StorageCleanReport, StorageCleanRequest,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
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

    fn compact_storage(
        &self,
        _request: RepositoryStatusRequest,
        _compact: IndexCompactRequest,
    ) -> Result<IndexCompactReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("compact"))
    }

    fn clean_storage(
        &self,
        _request: RepositoryStatusRequest,
        _clean: StorageCleanRequest,
    ) -> Result<StorageCleanReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("storage clean"))
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
        _against: Option<&str>,
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

    fn unknown_inventory(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<UnknownInventoryReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("unknowns"))
    }

    fn product_readiness(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<ProductReadinessReport, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("readiness"))
    }

    fn install_agent_integration(
        &self,
        _command: &str,
        _request: InstallRequest,
        _context: InstallExecutionContext,
    ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented("install"))
    }

    fn inspect_agent_integration(
        &self,
        _target: AgentTarget,
        _scope: InstallScope,
        _context: &InstallExecutionContext,
    ) -> Result<AgentIntegrationInspection, RepoGrammarError> {
        Err(RepoGrammarError::NotImplemented(
            "native agent integration probe",
        ))
    }

    fn mcp_self_test(&self, _project: &str) -> Result<(), SetupFailureClass> {
        Err(SetupFailureClass::McpSelfTestFailed)
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

    fn prompt_setup_confirmation(&self, prompt: &str) -> Result<String, String> {
        self.prompt_install_confirmation(prompt)
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
        [command, all] if command == "help" && all == "--all" => {
            CliOutput::success(full_usage())
        }
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
        [command, rest @ ..] if command == "setup" => handle_setup(
            rest,
            current_dir,
            env_lookup,
            runtime,
            install_prompt,
        ),
        [command, rest @ ..] if command == "instructions" => {
            handle_instructions(rest, current_dir)
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
        [command, rest @ ..] if command == "unknowns" => {
            handle_unknowns(rest, current_dir, env_lookup, runtime)
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
        "Find source-backed implementation patterns without reading the whole repository.",
        "",
        "Quick start:",
        "  repogrammar setup",
        "  repogrammar find \"How are API routes implemented?\"",
        "",
        "Core commands:",
        "  setup      Wire your agent, index this repository, and keep it fresh.",
        "  find       Find the best-supported implementation pattern for a target.",
        "  families   Summarize implementation pattern groups that are ready.",
        "  doctor     Diagnose readiness and show the next recovery action.",
        "",
        "Learn more:",
        "  repogrammar help <command>   Command options and safety notes.",
        "  repogrammar help --all       Complete command list.",
        "  repogrammar version          Installed version.",
    ])
}

fn full_usage() -> String {
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
        "  setup [--project <path>] [--target auto|codex|claude-code] [--yes] [--dry-run] [--no-autosync] [--json] [--progress auto|always|never]",
        "      Complete agent wiring, repository indexing, autosync, and MCP self-test in one plan.",
        "  init [--project <path>] [--yes] [--state-only] [--resync] [--autosync|--no-autosync] [--write-gitignore] [--json] [--progress auto|always|never]",
        "      Create repo-local state, build the active index, and start autosync by default.",
        "  uninit [--project <path>] --yes [--json]",
        "      Remove RepoGrammar repo-local state after explicit confirmation.",
        "  index [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Build a fresh syntax/code-unit index and activate it atomically.",
        "  sync [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Incrementally update the active index when safe, with full-rebuild fallback.",
        "  resync [--project <path>] [--json] [--progress auto|always|never] [--quiet|--verbose]",
        "      Rebuild the active index and static-analysis facts for any initialized repository.",
        "  autosync <status|enable|start|stop|disable|run> [options]",
        "      Manage optional repo-local automatic sync. Use `autosync start`, not `--start`.",
        "  prune [--project <path>] [--keep <n>] [--dry-run] [--yes] [--json]",
        "      Remove old inactive index generations while preserving the active generation.",
        "  compact [--project <path>] [--dry-run] [--yes] [--json]",
        "      Compact the repo-owned mutable SQLite index database after explicit confirmation.",
        "  storage clean [--project <path>] [--dry-run] [--yes] [--json]",
        "      Run safe repo-local storage maintenance: remove legacy layout, prune inactive generations, and compact SQLite.",
        "  status [--project <path>] [--json]",
        "      Report repository state, active generation, schema, and storage health.",
        "  doctor [--project <path>] [--json]",
        "      Inspect lifecycle hygiene, storage health, locks, and repair guidance.",
        "  unlock [--project <path>] [--force --yes] [--json]",
        "      Inspect locks; remove confirmed stale or legacy pre-host-format index locks with --force --yes.",
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
        "      Report a source-backed static-alignment certificate; runtime equivalence stays UNKNOWN.",
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
        "  instructions <status|sync|remove> --file <path> [--dry-run] [--yes] [--json]",
        "      Inspect or explicitly refresh one marker-fenced agent instruction file.",
        "",
        "Agent-safe repository bootstrap:",
        "  repogrammar setup",
        "      Review one plan for agent wiring, repository indexing, autosync, and product self-test.",
        "",
        "Metrics:",
        "  unknowns [--project <path>] [--json]",
        "      Report aggregate typed UNKNOWN inventory from the active index.",
        "  stats [--project <path>] [--unknowns] [--json]",
        "      Report repo-shape diagnostics and optional UNKNOWN inventory.",
        "  telemetry <status|on|off|export|upload|purge|research-*|experiment-*> [options]",
        "      Manage optional anonymous telemetry and local token experiment records.",
        "",
        "Maintenance:",
        "  version, --version, -V",
        "      Print the product version.",
        "  help [command|--all]",
        "      Print compact guidance, command-specific usage, or this complete command list.",
        "",
    ])
}

fn help_text(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

pub fn command_usage(command: &str) -> Option<String> {
    match command {
        "setup" => Some(help_text(&[
            "Usage: repogrammar setup [--project <path>] [--target auto|codex|claude-code] [--yes] [--dry-run] [--no-autosync] [--json] [--progress auto|always|never]",
            "",
            "Builds one reversible onboarding plan, asks once, then wires a detected agent, initializes and indexes the repository, starts autosync, and verifies the read-only MCP server.",
            "Telemetry remains off. Missing agents do not prevent repository-only setup. Foreign or malformed integration is never overwritten.",
            "",
            "Options:",
            "  --project <path>                  Repository root. Defaults to the current directory.",
            "  --target auto|codex|claude-code  Auto-select live supported agents, or request one explicitly.",
            "  --yes                             Confirm the complete plan noninteractively.",
            "  --dry-run                         Inspect the plan without any writes.",
            "  --no-autosync                     Build the active index without starting background sync.",
            "  --json                            Emit one machine-readable result object.",
            "  --progress auto|always|never      Control indexing progress on stderr.",
        ])),
        "init" => Some(help_text(&[
            "Usage: repogrammar init [--project <path>|--path <path>] [--yes] [--state-only] [--resync] [--autosync|--no-autosync] [--write-gitignore] [--json] [--progress auto|always|never] [--quiet|--verbose]",
            "",
            "Creates repository-local RepoGrammar state under .repogrammar/, builds or refreshes the active index, and starts autosync by default.",
            "Use --state-only only for low-level lifecycle repair without indexing. Without --write-gitignore it avoids tracked .gitignore edits and writes Git exclude hygiene instead.",
            "--yes is accepted as an agent-safe noninteractive confirmation flag; it does not broaden init writes.",
            "--resync and --autosync remain accepted as explicit compatibility spellings of the defaults. Use --no-autosync for CI or one-shot indexing.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root to initialize. Defaults to the current directory.",
            "  --yes                              Accepted no-op confirmation flag for noninteractive agent bootstrap.",
            "  --state-only                       Create or repair lifecycle state without indexing or autosync.",
            "  --resync                           Explicitly request the default active-index build after init succeeds.",
            "  --autosync                         Explicitly request the default repo-local autosync start.",
            "  --no-autosync                      Build the active index without starting a background daemon.",
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
            "Init starts autosync by default. Use autosync start to recover it after a stop, reboot, or daemon failure.",
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
            "Removes old inactive mutable index generations. Legacy generation directories are pruned only when no mutable database exists.",
            "Destructive runs require --yes. Use --dry-run to inspect candidates without deleting.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --keep <n>                         Number of newest inactive generations to keep. Defaults to 2 and may be 0.",
            "  --dry-run                          Report prune candidates without deleting generation records or legacy directories.",
            "  --yes                              Required unless --dry-run is present.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "compact" => Some(help_text(&[
            "Usage: repogrammar compact [--project <path>|--path <path>] [--dry-run] [--yes] [--json] [--quiet|--verbose]",
            "",
            "Compacts the repo-owned mutable SQLite index database. It never removes source files, user files, or legacy generation directories.",
            "Dry-run and mutating runs acquire the repository-local index lock. Mutating runs require --yes; use --dry-run to report database, WAL, and SHM sizes without writes.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --dry-run                          Report before/after size metadata without compacting.",
            "  --yes                              Required unless --dry-run is present.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted lifecycle verbosity flags.",
        ])),
        "storage" => Some(help_text(&[
            "Usage: repogrammar storage clean [--project <path>|--path <path>] [--dry-run] [--yes] [--json] [--quiet|--verbose]",
            "",
            "Runs safe repo-local storage maintenance in one command: status preflight, legacy-layout cleanup when mutable SQLite is authoritative, inactive-generation prune with --keep 0, and mutable SQLite compact.",
            "Mutating clean runs acquire the repository-local index lock and require --yes. Dry-runs report candidates and size metadata without deleting legacy files, pruning rows, or compacting.",
            "Legacy-only repositories are not deleted by storage clean; run repogrammar resync first to create mutable SQLite storage.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --dry-run                          Report cleanup candidates without writes.",
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
            "With --force --yes it removes confirmed stale index locks and legacy pre-host-format index",
            "locks (printing their best-effort provenance); daemon and SQLite locks are preserved, and a",
            "new-format lock whose owner cannot be confirmed stays refused.",
            "",
            "Options:",
            "  --project <path>, --path <path>     Repository root. Defaults to the current directory.",
            "  --force                            Request stale or legacy index-lock removal.",
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
        "families" => Some(help_text(&[
            "Usage: repogrammar families [--project <path>] [--all] [--json]",
            "",
            "Summarizes active supported pattern families without exposing internal cluster ids by default.",
            "Queries never initialize a repository. Missing or stale indexes return fallback or safe recovery guidance.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --all                               Show every canonical family id in human output.",
            "  --json                              Emit the complete machine-readable family inventory.",
        ])),
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
            "Report a source-backed static-alignment certificate for a target; runtime equivalence stays UNKNOWN.",
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
        "instructions" => Some(help_text(&[
            "Usage: repogrammar instructions <status|sync|remove> --file <path> [--dry-run] [--yes] [--json]",
            "",
            "Inspects, refreshes, or removes RepoGrammar's marker-fenced pre-flight gate in one explicitly selected instruction file.",
            "No file is guessed, no AGENTS.md/CLAUDE.md mirroring is imposed, and repository state is never initialized.",
            "Malformed, duplicated, or unrecognized managed sections are preserved and refused for writes.",
            "",
            "Subcommands:",
            "  status                            Report missing/current/outdated/foreign/malformed state without writes.",
            "  sync                              Create, append, or refresh an exact known managed section.",
            "  remove                            Remove only an exact known managed section.",
            "",
            "Options:",
            "  --file <path>                     Required explicit instruction file. Relative paths use the current directory.",
            "  --dry-run                         Report the planned sync/remove without writes.",
            "  --yes                             Required for a non-dry-run sync or remove.",
            "  --json                            Emit a low-cardinality machine-readable result without the file path.",
        ])),
        "unknowns" => Some(help_text(&[
            "Usage: repogrammar unknowns [--project <path>] [--json] [--quiet|--verbose]",
            "",
            "Reports aggregate typed UNKNOWN inventory from the readable active index. It is diagnostic and does not claim quality improvement.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --json                             Emit machine-readable output.",
            "  --quiet, --verbose                 Accepted metrics verbosity flags.",
        ])),
        "stats" => Some(help_text(&[
            "Usage: repogrammar stats [--project <path>] [--unknowns] [--json] [--quiet|--verbose]",
            "",
            "Reports repo-shape diagnostics and estimated potential read displacement. With --unknowns it embeds aggregate UNKNOWN inventory. It does not upload telemetry.",
            "",
            "Options:",
            "  --project <path>                    Repository root. Defaults to the current directory.",
            "  --unknowns                         Include aggregate typed UNKNOWN inventory in JSON or human output.",
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
            "  research-status|research-on|research-off|research-export|research-purge [--json] [--yes] [--project <path>]",
            "",
            "Experiment subcommands:",
            "  experiment-start --name <name> --experiment-mode record_existing|controlled_pair --session baseline|treatment --measurement-source host_reported|user_entered|documented_tokenizer [--yes] [--json]",
            "  experiment-record --name <name> (--usage-json <path>|--input-tokens <n> --output-tokens <n>) [--tool-tokens <n>] [--success true|false] [--json]",
            "  experiment-stop|experiment-report|experiment-export|experiment-purge --name <name> [--yes] [--json]",
            "",
            "Option scope:",
            "  --project <path>                    Repository root for anonymous telemetry or research diagnostics only.",
            "  --json                             Emit machine-readable output where listed above.",
            "  --yes                              Confirm the listed upload, purge, or experiment operation.",
            "  --dry-run                          Validate an upload payload without network activity.",
            "  --endpoint <url>                    HTTPS or localhost telemetry upload endpoint.",
            "Experiment subcommands accept only the options listed in their section; they do not accept --project.",
        ])),
        "version" => Some(help_text(&[
            "Usage: repogrammar version",
            "       repogrammar --version",
            "       repogrammar -V",
            "",
            "Prints the RepoGrammar package version.",
        ])),
        "help" => Some(help_text(&[
            "Usage: repogrammar help [command|--all]",
            "       repogrammar <command> --help",
            "       repogrammar <command> -h",
            "",
            "Prints compact top-level help or command-specific usage and options.",
            "Use --all for the complete command list.",
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
        "  --progress auto|always|never       Control progress-bar events on stderr.",
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
        "  --verbosity minimal|standard|full   Select response field density. Default standard; additive under product-schemas.v1. Orthogonal to --mode.",
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
    command == "setup"
        || command == "instructions"
        || is_project_lifecycle_command(command)
        || is_query_command(command)
        || is_installer_command(command)
        || matches!(
            command,
            "unknowns" | "stats" | "telemetry" | "version" | "help"
        )
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
            | "storage"
            | "prune"
            | "compact"
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
    if command == "storage" {
        return match parse_storage_options(rest) {
            Ok(options) => handle_storage(&options, current_dir, env_lookup, runtime),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }
    if command == "prune" {
        return match parse_prune_options(rest) {
            Ok(options) => handle_prune(&options, current_dir, env_lookup, runtime),
            Err(error) => CliOutput::failure(2, format!("{error}\n")),
        };
    }
    if command == "compact" {
        return match parse_compact_options(rest) {
            Ok(options) => handle_compact(&options, current_dir, env_lookup, runtime),
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
    if options.all && command != "families" {
        return CliOutput::failure(
            2,
            format!("{command} does not accept --all; use an explicit --mode for query detail\n"),
        );
    }
    if options.against.is_some() && !matches!(command, "explain" | "check") {
        return CliOutput::failure(
            2,
            format!("{command} does not accept --against; --against names the comparison family for explain and check only\n"),
        );
    }
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
            record_cli_family_query_fallback(
                request,
                command,
                lookup_mode_for_command(command),
                options.include_source_spans,
            );
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
            record_cli_family_query_fallback(
                request,
                command,
                lookup_mode_for_command(command),
                options.include_source_spans,
            );
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
        if let Some(error) = inventory_flag_rejection(command, rest) {
            return CliOutput::failure(2, format!("{error}\n"));
        }
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
            Ok(report) => CliOutput::success(families_human(&report, options.all)),
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
                options.against.as_deref(),
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
                            record_cli_family_query_fallback(
                                request.clone(),
                                command,
                                lookup_mode_for_command(command),
                                options.include_source_spans,
                            );
                            return query_fallback(
                                command,
                                options.json,
                                "repository status is unavailable",
                                "run repogrammar doctor",
                                false,
                            );
                        }
                    };
                    let savings = family_query_outcome_token_savings(
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                    );
                    record_cli_family_query_metric(
                        request.clone(),
                        command,
                        lookup_mode_for_command(command),
                        &report,
                        prepared_output.as_ref(),
                        options.include_source_spans,
                        savings.as_ref(),
                    );
                    CliOutput::success(family_lookup_json(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                        savings.as_ref(),
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
                            record_cli_family_query_fallback(
                                request.clone(),
                                command,
                                lookup_mode_for_command(command),
                                options.include_source_spans,
                            );
                            return query_fallback(
                                command,
                                options.json,
                                "repository status is unavailable",
                                "run repogrammar doctor",
                                false,
                            );
                        }
                    };
                    let savings = family_query_outcome_token_savings(
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                    );
                    record_cli_family_query_metric(
                        request.clone(),
                        command,
                        lookup_mode_for_command(command),
                        &report,
                        prepared_output.as_ref(),
                        options.include_source_spans,
                        savings.as_ref(),
                    );
                    CliOutput::success(family_lookup_human(
                        command,
                        &report,
                        options.target.as_deref(),
                        lookup_mode_for_command(command),
                        options.output_options(),
                        prepared_output.as_ref(),
                        savings.as_ref(),
                    ))
                }
                Err(_) => {
                    record_cli_family_query_fallback(
                        request,
                        command,
                        lookup_mode_for_command(command),
                        options.include_source_spans,
                    );
                    query_fallback(
                        command,
                        options.json,
                        "repository status is unavailable",
                        "run repogrammar doctor",
                        false,
                    )
                }
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
        // `check` and `explain` both run the two-sided static-alignment flow:
        // `explain` is no longer a `find` alias but a real deviation projection
        // (`target_relationship`). `find` stays on the fuzzy discovery pipeline.
        "check" | "explain" => FamilyLookupMode::Conformance,
        "find" => FamilyLookupMode::FuzzyQuery,
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
        // The certificate already carries its comparison-family read plan.
        FamilyLookupReport::Alignment(certificate) => certificate.read_plan.clone(),
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

/// The single all-scope potential-token-savings event for a lookup outcome, or
/// `None` for an abstention (Unknown), a PARTIAL_CONTEXT with no stored file
/// size, or an abstaining certificate. Delegates every estimate to the query
/// authority; surfaces never reimplement the accounting from raw fields.
fn family_query_outcome_token_savings(
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> Option<OutcomeTokenSavings> {
    match report {
        FamilyLookupReport::Found(family) => {
            let output_components =
                family_output_components(family, target, mode, options, prepared_output, None);
            Some(found_outcome_token_savings(
                family,
                output_components.estimated_potential_token_savings,
            ))
        }
        FamilyLookupReport::PartialContext(report) => {
            let read_plan = prepared_output
                .map(|prepared| &prepared.read_plan)
                .unwrap_or(&report.read_plan);
            let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
            estimate_partial_context_potential_token_savings(report, read_plan, source_spans)
        }
        FamilyLookupReport::Alignment(certificate) => {
            let read_plan = prepared_output
                .map(|prepared| &prepared.read_plan)
                .unwrap_or(&certificate.read_plan);
            let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
            estimate_alignment_potential_token_savings(certificate, read_plan, source_spans)
        }
        FamilyLookupReport::Unknown(_) => None,
    }
}

/// The shared `estimated_potential_token_savings` output block every
/// context-delivering surface renders, carrying the ESTIMATED caveat verbatim.
fn estimated_potential_token_savings_json(savings: &OutcomeTokenSavings) -> serde_json::Value {
    json!({
        "outcome_shape": savings.shape.as_str(),
        "language": savings.language,
        "estimated_baseline_tokens": savings.metric.estimated_baseline_tokens,
        "estimated_returned_tokens": savings.metric.estimated_returned_tokens,
        "estimated_potential_token_savings": savings.metric.estimated_potential_token_savings,
        "estimated_potential_token_savings_kind": savings.metric.measurement_kind.as_str(),
        "estimated_potential_token_savings_caveat": savings.metric.caveat,
    })
}

fn record_cli_family_query_metric(
    request: RepositoryStatusRequest,
    command: &str,
    mode: FamilyLookupMode,
    report: &FamilyLookupReport,
    prepared_output: Option<&PreparedFamilyOutput>,
    source_spans_requested: bool,
    savings: Option<&OutcomeTokenSavings>,
) {
    let Some(command_category) = cli_family_query_command_category(command) else {
        return;
    };
    let unknowns = family_query_unknown_metrics(report);
    let record = FamilyQueryOutcomeRecord {
        status: family_query_outcome_status(report),
        entrypoint: FamilyQueryEntrypoint::Cli,
        command_category,
        lookup_mode: family_query_lookup_mode(mode),
        unknowns: &unknowns,
        abstention_reason: family_query_abstention_reason(report),
        read_plan_item_count: prepared_output.map(|output| output.read_plan.items.len()),
        source_spans_requested,
        source_spans_included: prepared_output
            .is_some_and(|output| output.read_plan.source_snippets_included),
        source_span_omission_count: prepared_output
            .and_then(|output| output.source_spans.as_ref())
            .map(|source_spans| source_spans.omissions.len()),
    };
    let savings = savings.map(|savings| FamilyQuerySavingsRecord {
        metric: &savings.metric,
        outcome_shape: savings.shape.as_str(),
        language: savings.language,
    });
    let _ = record_family_query_metric(request, &record, savings);
}

fn record_cli_family_query_fallback(
    request: RepositoryStatusRequest,
    command: &str,
    mode: FamilyLookupMode,
    source_spans_requested: bool,
) {
    let Some(command_category) = cli_family_query_command_category(command) else {
        return;
    };
    let record = FamilyQueryOutcomeRecord {
        status: FamilyQueryOutcomeStatus::Fallback,
        entrypoint: FamilyQueryEntrypoint::Cli,
        command_category,
        lookup_mode: family_query_lookup_mode(mode),
        unknowns: &[],
        abstention_reason: None,
        read_plan_item_count: None,
        source_spans_requested,
        source_spans_included: false,
        source_span_omission_count: None,
    };
    let _ = record_family_query_metric(request, &record, None);
}

fn cli_family_query_command_category(command: &str) -> Option<FamilyQueryCommandCategory> {
    match command {
        "family" => Some(FamilyQueryCommandCategory::Family),
        "member" => Some(FamilyQueryCommandCategory::Member),
        "find" => Some(FamilyQueryCommandCategory::Find),
        "explain" => Some(FamilyQueryCommandCategory::Explain),
        "check" => Some(FamilyQueryCommandCategory::Check),
        _ => None,
    }
}

fn family_query_lookup_mode(mode: FamilyLookupMode) -> FamilyQueryLookupMode {
    match mode {
        FamilyLookupMode::ExactFamilyId => FamilyQueryLookupMode::ExactFamily,
        FamilyLookupMode::ExactMemberId => FamilyQueryLookupMode::ExactMember,
        // The conformance check runs on the shared fuzzy resolution pipeline.
        FamilyLookupMode::FuzzyQuery | FamilyLookupMode::Conformance => {
            FamilyQueryLookupMode::Fuzzy
        }
    }
}

fn family_query_outcome_status(report: &FamilyLookupReport) -> FamilyQueryOutcomeStatus {
    match report {
        FamilyLookupReport::Found(_) => FamilyQueryOutcomeStatus::Found,
        FamilyLookupReport::PartialContext(_) => FamilyQueryOutcomeStatus::PartialContext,
        FamilyLookupReport::Unknown(_) => FamilyQueryOutcomeStatus::Unknown,
        // Telemetry follows the alignment status, not the presence of a family:
        // a committed certificate (STATICALLY_ALIGNED/STATIC_DEVIATION) is Found,
        // a PARTIAL_ALIGNMENT is partial context, and every abstaining certificate
        // (INSUFFICIENT_EVIDENCE/UNKNOWN) is Unknown — abstentions are never Found.
        FamilyLookupReport::Alignment(certificate) => {
            alignment_outcome_status(certificate.alignment_status)
        }
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

fn family_query_unknown_metrics(
    report: &FamilyLookupReport,
) -> Vec<crate::application::query::FamilyQueryUnknownMetric> {
    family_query_report_unknowns(report)
        .iter()
        .map(family_query_unknown_metric)
        .collect()
}

fn family_query_report_unknowns(report: &FamilyLookupReport) -> &[FamilyQueryUnknown] {
    match report {
        FamilyLookupReport::Found(report) => &report.unknowns,
        FamilyLookupReport::PartialContext(report) => &report.unknowns,
        FamilyLookupReport::Unknown(report) => &report.unknowns,
        FamilyLookupReport::Alignment(certificate) => &certificate.unknowns,
    }
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
                "schema_version": PRODUCT_SCHEMA_VERSION,
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

fn unknowns_fallback(json: bool, reason: &str, guidance: &str, implemented: bool) -> CliOutput {
    if json {
        return CliOutput::failure(
            2,
            json_line(json!({
                "status": "FALLBACK_TO_CODE_SEARCH",
                "reason": reason,
                "guidance": guidance,
                "command": "unknowns",
                "implemented": implemented,
                "inventory_available": false,
            })),
        );
    }

    CliOutput::failure(
        2,
        format!(
            "FALLBACK_TO_CODE_SEARCH\nreason: {reason}\nguidance: {guidance}\ninventory_available: false\ncommand: repogrammar unknowns requires a readable active syntax-only index; no pattern-family claims were made\n"
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
        "active_generation": &report.active_generation,
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

fn families_human(report: &FamilyListReport, show_all: bool) -> String {
    if show_all {
        return families_detailed_human(report);
    }
    if report.families.is_empty() {
        let next =
            if report.unknowns.iter().any(|unknown| {
                unknown.reason == crate::core::model::UnknownReasonCode::StaleEvidence
            }) {
                "run repogrammar resync"
            } else {
                "run repogrammar doctor"
            };
        return format!(
            "families: Cannot verify safely\nactive generation: {}\nreason: supported pattern-family evidence is unavailable or stale\nnext: {next}\n",
            report.active_generation
        );
    }

    let mut groups = BTreeMap::<(String, String), (usize, usize)>::new();
    for family in &report.families {
        let (language, role) = human_family_group(&family.family_id, &family.classification);
        let entry = groups.entry((language, role)).or_default();
        entry.0 += 1;
        entry.1 += family.support;
    }

    const MAX_VISIBLE_GROUPS: usize = 9;
    let mut output = format!(
        "families: {} implementation pattern groups ready\nactive generation: {}\n",
        report.families.len(),
        report.active_generation
    );
    if let Some(counts) = &report.freshness_counts {
        // Lead with the freshness rollup so stale/unverifiable families never
        // read as unqualified usable claims.
        output.push_str(&freshness_summary_line(counts));
        if counts.stale_count > 0 {
            output.push_str(&format!(
                "stale evidence: {} group(s) reference changed or missing source; run repogrammar resync\n",
                counts.stale_count
            ));
        }
        if counts.cannot_verify_count > 0 {
            output.push_str(&format!(
                "unverified: {} group(s) could not be checked (source too large or unreadable)\n",
                counts.cannot_verify_count
            ));
        }
    }
    for ((language, role), (family_count, support)) in groups.iter().take(MAX_VISIBLE_GROUPS) {
        output.push_str(&format!(
            "- {language} · {role} — {family_count} group(s), {support} implementation(s)\n"
        ));
    }
    if groups.len() > MAX_VISIBLE_GROUPS {
        output.push_str(&format!(
            "- {} more categories; use `repogrammar families --all` for canonical ids\n",
            groups.len() - MAX_VISIBLE_GROUPS
        ));
    }
    output.push_str("next: repogrammar find <path, symbol, or pattern question>\n");
    output
}

fn families_detailed_human(report: &FamilyListReport) -> String {
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
    if let Some(counts) = &report.freshness_counts {
        output.push_str(&freshness_summary_line(counts));
    }
    for family in &report.families {
        match family.freshness {
            Some(freshness) => output.push_str(&format!(
                "family: {}\tclassification: {}\tsupport: {}\tfreshness: {}\tprevalence: {}\n",
                family.family_id,
                family.classification,
                family.support,
                freshness.as_str(),
                family.prevalence.classification_reason
            )),
            None => output.push_str(&format!(
                "family: {}\tclassification: {}\tsupport: {}\tprevalence: {}\n",
                family.family_id,
                family.classification,
                family.support,
                family.prevalence.classification_reason
            )),
        }
    }
    // Surface the report-level stale-evidence signal in the detailed view; the
    // freshness-free variant renders no extra unknowns here, as before.
    if report.freshness_counts.is_some() {
        for unknown in &report.unknowns {
            push_unknown_human(&mut output, unknown);
        }
    }
    output
}

fn human_family_group(family_id: &str, classification: &str) -> (String, String) {
    let parts = family_id.split(':').collect::<Vec<_>>();
    if parts.first() == Some(&"family") && parts.len() >= 4 {
        return (
            humanize_family_token(parts[1]),
            humanize_family_token(parts[3].strip_prefix("framework_").unwrap_or(parts[3])),
        );
    }
    ("Other".to_string(), humanize_family_token(classification))
}

fn human_family_label(family_id: &str, classification: &str) -> String {
    let (language, role) = human_family_group(family_id, classification);
    format!("{language} · {role}")
}

fn humanize_family_token(token: &str) -> String {
    token
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "cpp" => "C/C++".to_string(),
            "csharp" => "C#".to_string(),
            "fastapi" => "FastAPI".to_string(),
            "javascript" => "JavaScript".to_string(),
            "nestjs" => "NestJS".to_string(),
            "nextjs" => "Next.js".to_string(),
            "pydantic" => "Pydantic".to_string(),
            "pytest" => "pytest".to_string(),
            "sqlalchemy" => "SQLAlchemy".to_string(),
            "typescript" => "TypeScript".to_string(),
            "tsjs" => "TypeScript/JavaScript".to_string(),
            "xunit" => "xUnit".to_string(),
            _ => {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Metadata-only prevalence object shared by every family output surface.
fn family_prevalence_json(prevalence: &FamilyPrevalence) -> serde_json::Value {
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
/// the active generation persisted none. Every field is a RepoGrammar-owned
/// typed token or count in the profile's own deterministic order; no repository
/// source text is emitted.
fn family_constraint_profile_json(profile: Option<&FamilyConstraintProfile>) -> serde_json::Value {
    let Some(profile) = profile else {
        return serde_json::Value::Null;
    };
    json!({
        "required_equal_features": profile
            .required_equal_features
            .iter()
            .map(feature_constraint_json)
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
            .map(feature_constraint_json)
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

fn feature_constraint_json(
    constraint: &crate::core::model::FeatureConstraint,
) -> serde_json::Value {
    json!({
        "prefix": constraint.prefix,
        "values": constraint.values,
        "origin": constraint.origin.as_token(),
        "semantics": constraint.semantics.as_token(),
    })
}

fn families_json(command: &str, report: &FamilyListReport) -> String {
    let mut value = json!({
        "command": command,
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": if report.families.is_empty() { "UNKNOWN" } else { "ok" },
        "implemented": true,
        "active_generation": report.active_generation,
        "families": report.families.iter().map(|family| {
            let mut entry = json!({
                "family_id": family.family_id,
                "classification": family.classification,
                "support": family.support,
                "prevalence": family_prevalence_json(&family.prevalence),
            });
            // The freshness-verified listing carries a per-family verdict; the
            // freshness-free variant omits the field entirely.
            if let Some(freshness) = family.freshness {
                entry["freshness"] = json!(freshness.as_str());
            }
            entry
        }).collect::<Vec<_>>(),
        "unknowns": unknowns_json(&report.unknowns),
    });
    if let (Some(counts), Some(object)) = (&report.freshness_counts, value.as_object_mut()) {
        object.insert("fresh_count".to_string(), json!(counts.fresh_count));
        object.insert("stale_count".to_string(), json!(counts.stale_count));
        object.insert(
            "cannot_verify_count".to_string(),
            json!(counts.cannot_verify_count),
        );
    }
    json_line(value)
}

/// One-line freshness rollup shared by the compact and detailed human surfaces.
fn freshness_summary_line(counts: &FamilyFreshnessCounts) -> String {
    format!(
        "freshness: {} fresh · {} stale · {} cannot verify\n",
        counts.fresh_count, counts.stale_count, counts.cannot_verify_count
    )
}

fn family_lookup_human(
    command: &str,
    report: &FamilyLookupReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
    savings: Option<&OutcomeTokenSavings>,
) -> String {
    // The static-alignment certificate has its own actionable, result-first
    // rendering and never falls back to the family-context human surface.
    if let FamilyLookupReport::Alignment(certificate) = report {
        let route = family_query_route_report(report, mode);
        return alignment_certificate_human(
            command,
            certificate,
            &route,
            options,
            prepared_output,
            savings,
        );
    }
    if matches!(command, "find" | "explain")
        && options.evidence_mode == FamilyEvidenceMode::Compact
        && !options.include_variations
        && !options.include_exceptions
        && prepared_output
            .and_then(|prepared| prepared.source_spans.as_ref())
            .is_none()
    {
        return family_lookup_compact_human(command, report, prepared_output);
    }
    let route = family_query_route_report(report, mode);
    match report {
        FamilyLookupReport::Found(family) => {
            let output_components = family_output_components(
                family,
                target,
                mode,
                options,
                prepared_output,
                savings.map(|savings| &savings.metric),
            );
            let selected_evidence = &output_components.selected_evidence;
            let read_plan = &output_components.read_plan;
            let estimated_potential = &output_components.estimated_potential_token_savings;
            let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
            let snippets = if read_plan.source_snippets_included {
                "included"
            } else {
                "not_included"
            };
            let mut output = format!(
                "{command}: evidence-backed family\nactive_generation: {}\nfamily: {}\nclassification: {}\nsupport: {}\nevidence_mode: {}\nestimated_evidence_tokens: {}\nsource_snippets: {}\n",
                family.active_generation,
                family.family_id,
                family.classification,
                family.support,
                selected_evidence.mode.as_str(),
                selected_evidence.estimated_tokens,
                snippets
            );
            output.push_str(&format!(
                "prevalence: {}\n",
                family.prevalence.classification_reason
            ));
            push_query_route_human(&mut output, &route);
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
            let (rendered_members, members_truncated) =
                bounded_family_members(family, options.evidence_mode);
            output.push_str(&format!("member_count: {}\n", family.members.len()));
            for member in rendered_members {
                output.push_str(&format!(
                    "member: {}\trole: {}\n",
                    member.code_unit_id, member.role
                ));
            }
            if members_truncated {
                output.push_str(&format!(
                    "members_truncated: {} of {} shown; use --mode deep for the full list\n",
                    rendered_members.len(),
                    family.members.len()
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
            family_partial_context_human(command, report, &route, options, prepared_output, savings)
        }
        FamilyLookupReport::Unknown(report) => family_unknown_human(command, report, &route),
        FamilyLookupReport::Alignment(_) => unreachable!("alignment handled above"),
    }
}

/// Actionable, result-first human rendering of a static-alignment certificate.
/// It leads with the alignment status and explicitly separates static alignment
/// from runtime conformance, which stays UNKNOWN.
fn alignment_certificate_human(
    command: &str,
    certificate: &AlignmentCertificateReport,
    route: &FamilyQueryRouteReport,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
    savings: Option<&OutcomeTokenSavings>,
) -> String {
    let read_plan = prepared_output
        .map(|prepared| &prepared.read_plan)
        .unwrap_or(&certificate.read_plan);
    let mut output = format!(
        "{command}: {}\nresult: static alignment only; runtime conformance is NOT proven\nactive_generation: {}\n",
        certificate.alignment_status.as_token(),
        certificate.active_generation
    );
    output.push_str(&format!(
        "alignment_status: {}\n",
        certificate.alignment_status.as_token()
    ));
    output.push_str("runtime_equivalence: UNKNOWN\n");
    if let Some(relationship) = certificate.target_relationship {
        output.push_str(&format!(
            "target_relationship: {}\n",
            relationship.as_token()
        ));
    }
    if let Some(family_id) = &certificate.selected_family_id {
        output.push_str(&format!("selected_family: {family_id}\n"));
    }
    output.push_str(&format!(
        "target: {}\tcode_unit_id: {}\tbyte_range: {}\n",
        certificate.resolved_target.path,
        certificate
            .resolved_target
            .code_unit_id
            .as_deref()
            .unwrap_or("none"),
        certificate
            .resolved_target
            .byte_range
            .map(|(start, end)| format!("{start}-{end}"))
            .unwrap_or_else(|| "none".to_string()),
    ));
    push_query_route_human(&mut output, route);
    if let Some(computation) = certificate.computation.as_deref() {
        output.push_str(&format!("outcome_reason: {}\n", computation.outcome_reason));
        for matched in &computation.required_features_matched {
            output.push_str(&format!(
                "required_matched: {}\tsemantics: {}\texpected: {}\tsatisfied: {}\n",
                matched.prefix,
                matched.semantics.as_token(),
                matched.expected_summary,
                matched.satisfied_summary
            ));
        }
        for deviation in &computation.static_deviations {
            output.push_str(&format!(
                "static_deviation: {}\tkind: {}\tsemantics: {}\texpected: {}\tobserved: {}\n",
                deviation.prefix,
                deviation.kind.as_token(),
                deviation.semantics_token,
                deviation.expected_summary,
                deviation.observed_summary
            ));
        }
        for variation in &computation.legal_observed_variations {
            output.push_str(&format!(
                "legal_observed_variation: {}\tobserved_profile: {}\n",
                variation.dimension, variation.observed_profile
            ));
        }
        for unknown in &computation.blocking_unknowns {
            output.push_str(&format!(
                "blocking_unknown: {}\treason: {}\taffected_claim: {}\n",
                unknown.class.as_protocol_str(),
                unknown.reason.as_protocol_str(),
                unknown.affected_claim
            ));
        }
        for obligation in &computation.unresolved_runtime_obligations {
            output.push_str(&format!(
                "unresolved_runtime_obligation: {}\treason: {}\taffected_claim: {}\n",
                obligation.class.as_protocol_str(),
                obligation.reason.as_protocol_str(),
                obligation.affected_claim
            ));
        }
    }
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    push_estimated_potential_token_savings_human(&mut output, savings);
    push_read_plan_human(&mut output, read_plan, options.evidence_mode);
    if let Some(source_spans) = source_spans {
        push_source_spans_human(&mut output, source_spans);
    }
    for unknown in &certificate.unknowns {
        push_unknown_human(&mut output, unknown);
    }
    output.push_str(
        "next: read the contrast witness before applying; static alignment does not prove runtime behavior\n",
    );
    output
}

fn family_lookup_compact_human(
    command: &str,
    report: &FamilyLookupReport,
    prepared_output: Option<&PreparedFamilyOutput>,
) -> String {
    match report {
        FamilyLookupReport::Found(family) => {
            let read_plan = prepared_output.map(|prepared| &prepared.read_plan);
            let mut output = format!("{command}: pattern family found\n");
            output.push_str(&format!(
                "pattern: {}\n",
                human_family_label(&family.family_id, &family.classification)
            ));
            output.push_str(&format!(
                "support: {} source-backed implementation(s)\n",
                family.support
            ));
            output.push_str("why: compatible indexed evidence supports this pattern group\n");
            push_compact_read_plan_human(&mut output, read_plan);
            if !family.unknowns.is_empty() {
                output.push_str("unverified: some dynamic or runtime behavior remains unproven\n");
            }
            output.push_str(
                "next: read the suggested span before editing; use --mode evidence for details\n",
            );
            output
        }
        FamilyLookupReport::PartialContext(report) => {
            let read_plan = prepared_output
                .map(|prepared| &prepared.read_plan)
                .unwrap_or(&report.read_plan);
            let mut output = format!(
                "{command}: PARTIAL_CONTEXT\nresult: local read plan only; no family conclusion\ntarget: {}\n",
                report.resolved_target.path
            );
            push_compact_read_plan_human(&mut output, Some(read_plan));
            output.push_str("unverified: pattern-family evidence is insufficient\n");
            output.push_str("next: read the suggested span, then narrow the target if needed\n");
            output
        }
        FamilyLookupReport::Unknown(report) => {
            let stale = report.unknowns.iter().any(|unknown| {
                unknown.reason == crate::core::model::UnknownReasonCode::StaleEvidence
            });
            let next = if stale {
                "run repogrammar resync"
            } else {
                "use an exact repository path or member id and rerun the command"
            };
            let mut output = format!(
                "{command}: Cannot verify safely\nreason: available evidence is insufficient for a supported pattern claim\n"
            );
            output.push_str(&format!("next: {next}\n"));
            output
        }
        FamilyLookupReport::Alignment(_) => unreachable!("alignment handled by the full renderer"),
    }
}

fn push_compact_read_plan_human(output: &mut String, read_plan: Option<&ReadPlan>) {
    let Some(item) = read_plan.and_then(|plan| plan.items.first()) else {
        output.push_str("read: no source span selected\n");
        return;
    };
    match (item.start_line, item.end_line) {
        (Some(start), Some(end)) => {
            output.push_str(&format!("read: {} (lines {start}-{end})\n", item.path));
        }
        _ => output.push_str(&format!(
            "read: {} (bytes {}-{})\n",
            item.path, item.start_byte, item.end_byte
        )),
    }
}

fn family_partial_context_human(
    command: &str,
    report: &FamilyPartialContextReport,
    route: &FamilyQueryRouteReport,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
    savings: Option<&OutcomeTokenSavings>,
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
    push_estimated_potential_token_savings_human(&mut output, savings);
    push_query_route_human(&mut output, route);
    push_read_plan_human(&mut output, read_plan, options.evidence_mode);
    if let Some(source_spans) = source_spans {
        push_source_spans_human(&mut output, source_spans);
    }
    for unknown in &report.unknowns {
        push_unknown_human(&mut output, unknown);
    }
    output
}

/// The shared human `estimated_potential_token_savings` block every
/// context-delivering surface renders, carrying the ESTIMATED caveat verbatim.
/// `None` renders an explicit `unavailable` (never a guessed number).
fn push_estimated_potential_token_savings_human(
    output: &mut String,
    savings: Option<&OutcomeTokenSavings>,
) {
    match savings {
        Some(savings) => output.push_str(&format!(
            "estimated_potential_token_savings: {}\nestimated_potential_token_savings_outcome_shape: {}\nestimated_potential_token_savings_language: {}\nestimated_potential_token_savings_kind: {}\nestimated_potential_token_savings_caveat: {}\n",
            savings.metric.estimated_potential_token_savings,
            savings.shape.as_str(),
            savings.language,
            savings.metric.measurement_kind.as_str(),
            savings.metric.caveat,
        )),
        None => output.push_str(&format!(
            "estimated_potential_token_savings: unavailable\nestimated_potential_token_savings_kind: {}\nestimated_potential_token_savings_caveat: {}\n",
            MeasurementKind::Estimated.as_str(),
            ESTIMATED_TOKEN_SAVING_CAVEAT,
        )),
    }
}

fn family_unknown_human(
    command: &str,
    report: &FamilyUnknownReport,
    route: &FamilyQueryRouteReport,
) -> String {
    let mut output = format!(
        "{command}: UNKNOWN\nactive_generation: {}\n",
        report.active_generation
    );
    push_query_route_human(&mut output, route);
    for unknown in &report.unknowns {
        push_unknown_human(&mut output, unknown);
    }
    output
}

fn push_query_route_human(output: &mut String, route: &FamilyQueryRouteReport) {
    output.push_str(&format!("query_route: {}\n", route.route));
    output.push_str(&format!("query_input_kind: {}\n", route.input_kind));
    output.push_str(&format!(
        "query_family_id_policy: {}\n",
        route.family_id_policy
    ));
    if let Some(candidate_limit) = route.candidate_limit {
        output.push_str(&format!("query_candidate_limit: {candidate_limit}\n"));
    }
    output.push_str(&format!("query_pipeline: {}\n", route.pipeline.join(",")));
    if let Some(selected_family_id) = &route.selected_family_id {
        output.push_str(&format!("query_selected_family_id: {selected_family_id}\n"));
    }
    if !route.candidate_family_ids.is_empty() {
        output.push_str(&format!(
            "query_candidate_family_ids: {}\n",
            route.candidate_family_ids.join(",")
        ));
    }
    if !route.follow_up_family_ids.is_empty() {
        output.push_str(&format!(
            "query_follow_up_family_ids: {}\n",
            route.follow_up_family_ids.join(",")
        ));
    }
    output.push_str(&format!("query_why_selected: {}\n", route.why_selected));
    if let Some(term) = &route.term_retrieval {
        push_term_retrieval_human(output, term, &route.candidate_family_ids);
    }
}

fn push_term_retrieval_human(
    output: &mut String,
    term: &TermRetrievalRoute,
    candidate_family_ids: &[String],
) {
    output.push_str(&format!("query_term_route: {}\n", term.route));
    match (&term.abstention_reason, &term.matched_signals) {
        (None, Some(signals)) => {
            // Found: name the concept/framework signal that anchored the match.
            let mut matched = Vec::new();
            if signals.framework_filter {
                matched.push("framework");
            }
            if signals.concept {
                matched.push("concept");
            }
            if signals.language_filter {
                matched.push("language");
            }
            if signals.residue_hits > 0 {
                matched.push("term");
            }
            output.push_str(&format!(
                "query_term_matched_signal: {}\n",
                if matched.is_empty() {
                    "none".to_string()
                } else {
                    matched.join("+")
                }
            ));
        }
        (Some(reason), _) => {
            output.push_str(&format!(
                "query_term_abstention_reason: {}\n",
                reason.as_str()
            ));
            if !candidate_family_ids.is_empty() {
                output.push_str(&format!(
                    "query_term_candidate_family_ids: {}\n",
                    candidate_family_ids.join(",")
                ));
            }
        }
        (None, None) => {}
    }
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
    savings: Option<&OutcomeTokenSavings>,
) -> String {
    let route = family_query_route_report(report, mode);
    match report {
        FamilyLookupReport::Found(family) => family_detail_json(
            command,
            family,
            &route,
            target,
            mode,
            options,
            prepared_output,
            savings.map(|savings| &savings.metric),
        ),
        FamilyLookupReport::PartialContext(report) => {
            family_partial_context_json(command, report, &route, options, prepared_output, savings)
        }
        FamilyLookupReport::Unknown(report) => json_line(json!({
            "command": command,
            "schema_version": PRODUCT_SCHEMA_VERSION,
            "status": "UNKNOWN",
            "implemented": true,
            "active_generation": report.active_generation,
            // Abstention: `candidate_family_ids` is the narrowing recovery handle,
            // kept even at `minimal` (Minimal tier).
            "query_route": query_route_json(&route, options.verbosity, VerbosityTier::Minimal),
            "unknowns": unknowns_json(&report.unknowns),
        })),
        FamilyLookupReport::Alignment(certificate) => json_line(alignment_certificate_json(
            command,
            certificate,
            &route,
            prepared_output,
            options.verbosity,
            savings,
        )),
    }
}

/// Source-free JSON for a static-alignment certificate. The top-level `status`
/// is the alignment status token; `runtime_equivalence` is always `UNKNOWN`. The
/// `query_route` carries the selected/candidate family ids so downstream tooling
/// reads the selection exactly as it does for other operations.
fn alignment_certificate_json(
    command: &str,
    certificate: &AlignmentCertificateReport,
    route: &FamilyQueryRouteReport,
    prepared_output: Option<&PreparedFamilyOutput>,
    verbosity: Verbosity,
    savings: Option<&OutcomeTokenSavings>,
) -> serde_json::Value {
    let read_plan = prepared_output
        .map(|prepared| &prepared.read_plan)
        .unwrap_or(&certificate.read_plan);
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let mut value = json!({
        "command": command,
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": certificate.alignment_status.as_token(),
        "implemented": true,
        "active_generation": certificate.active_generation,
        // A `check` can abstain (INSUFFICIENT_EVIDENCE) with candidate handles, so
        // `candidate_family_ids` is treated as a recovery handle, kept at `minimal`.
        "query_route": query_route_json(route, verbosity, VerbosityTier::Minimal),
        "alignment_status": certificate.alignment_status.as_token(),
        "runtime_equivalence": "UNKNOWN",
        "target_relationship": certificate
            .target_relationship
            .map(|relationship| relationship.as_token()),
        "selected_family_id": certificate.selected_family_id,
        "target": resolved_target_json(&certificate.resolved_target, verbosity),
        "alignment": certificate
            .computation
            .as_deref()
            .map(alignment_computation_json),
        "estimated_potential_token_savings": alignment_savings_json(savings),
        "read_plan": read_plan_json(read_plan, verbosity),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&certificate.unknowns),
    });
    // Mirrors `alignment_certificate_value`. `alignment_status` is byte-identical
    // to the top-level `status` and drops as a duplicate at `minimal`, while
    // `standard`/`full` stay byte-stable. The top-level `selected_family_id` is
    // KEPT at every tier as the authoritative carrier of the selected-family
    // handle: the `query_route.selected_family_id` copy is the one suppressed at
    // `minimal` (by the route lane), so dropping the certificate top-level copy
    // too would erase "which family was selected" at `minimal`.
    // `runtime_equivalence: "UNKNOWN"` is an invariant and is never removed.
    if !verbosity.renders(VerbosityTier::Standard) {
        let object = value
            .as_object_mut()
            .expect("alignment certificate serializes to a JSON object");
        object.remove("alignment_status");
    }
    drop_unrequested_source_spans(&mut value, source_spans, verbosity);
    value
}

/// The alignment certificate's savings block: the full estimate for a committed
/// or partial certificate, otherwise a no-estimate block (an abstaining
/// certificate displaces no full read) that still carries the ESTIMATED caveat.
fn alignment_savings_json(savings: Option<&OutcomeTokenSavings>) -> serde_json::Value {
    match savings {
        Some(savings) => estimated_potential_token_savings_json(savings),
        None => json!({
            "outcome_shape": "alignment",
            "estimated_baseline_tokens": null,
            "estimated_returned_tokens": null,
            "estimated_potential_token_savings": null,
            "estimated_potential_token_savings_kind": MeasurementKind::Estimated.as_str(),
            "estimated_potential_token_savings_caveat": ESTIMATED_TOKEN_SAVING_CAVEAT,
            "unavailable_reason": "abstaining certificate; no full read displaced",
        }),
    }
}

fn alignment_computation_json(computation: &AlignmentComputation) -> serde_json::Value {
    let mut value = json!({
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
            .take(ALIGNMENT_DEVIATION_CAP)
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
            .take(ALIGNMENT_DEVIATION_CAP)
            .map(|variation| json!({
                "dimension": variation.dimension,
                "observed_profile": variation.observed_profile,
            }))
            .collect::<Vec<_>>(),
        "blocking_unknowns": computation
            .blocking_unknowns
            .iter()
            .map(alignment_typed_unknown_json)
            .collect::<Vec<_>>(),
        "unresolved_runtime_obligations": computation
            .unresolved_runtime_obligations
            .iter()
            .map(alignment_typed_unknown_json)
            .collect::<Vec<_>>(),
    });
    insert_deviation_cap_flags_json(
        value
            .as_object_mut()
            .expect("alignment computation serializes to a JSON object"),
        computation,
    );
    value
}

/// CLI mirror of `insert_deviation_cap_flags`: emit the honest sibling truncation
/// metadata for the capped deviation-style arrays only when the source array
/// exceeds [`ALIGNMENT_DEVIATION_CAP`], keeping the object byte-identical to the
/// pre-cap shape below the cap and byte-parallel with the MCP surface.
fn insert_deviation_cap_flags_json(
    object: &mut serde_json::Map<String, serde_json::Value>,
    computation: &AlignmentComputation,
) {
    let arrays = [
        ("static_deviations", computation.static_deviations.len()),
        (
            "legal_observed_variations",
            computation.legal_observed_variations.len(),
        ),
    ];
    for (name, total) in arrays {
        if total > ALIGNMENT_DEVIATION_CAP {
            object.insert(format!("{name}_truncated"), json!(true));
            object.insert(format!("{name}_count"), json!(total));
        }
    }
}

fn alignment_typed_unknown_json(unknown: &crate::core::model::TypedUnknown) -> serde_json::Value {
    json!({
        "class": unknown.class.as_protocol_str(),
        "reason": unknown.reason.as_protocol_str(),
        "affected_claim": unknown.affected_claim,
        "recovery": unknown.recovery,
    })
}

fn family_partial_context_json(
    command: &str,
    report: &FamilyPartialContextReport,
    route: &FamilyQueryRouteReport,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
    savings: Option<&OutcomeTokenSavings>,
) -> String {
    let read_plan = prepared_output
        .map(|prepared| &prepared.read_plan)
        .unwrap_or(&report.read_plan);
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let mut value = json!({
        "command": command,
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "PARTIAL_CONTEXT",
        "implemented": true,
        "active_generation": report.active_generation,
        // PartialContext: `candidate_family_ids` is a narrowing recovery handle,
        // kept even at `minimal` (Minimal tier).
        "query_route": query_route_json(route, options.verbosity, VerbosityTier::Minimal),
        "resolved_target": resolved_target_json(&report.resolved_target, options.verbosity),
        "output": {
            "mode": options.evidence_mode.as_str(),
            "token_budget": options.token_budget,
            "estimated_read_plan_tokens": read_plan.estimated_tokens,
            "selection_strategy": read_plan.selection_strategy,
            "budget_satisfied": read_plan.budget_satisfied,
            "source_snippets_included": read_plan.source_snippets_included,
        },
        "estimated_potential_token_savings": partial_context_savings_json(savings),
        "read_plan": read_plan_json(read_plan, options.verbosity),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&report.unknowns),
    });
    insert_resolution(&mut value, report.resolution.as_ref(), options.verbosity);
    drop_unrequested_source_spans(&mut value, source_spans, options.verbosity);
    json_line(value)
}

/// The PARTIAL_CONTEXT / alignment savings block: the full estimate when a
/// stored file size was available, otherwise an explicit no-estimate block that
/// still carries the ESTIMATED caveat (never a guessed number).
fn partial_context_savings_json(savings: Option<&OutcomeTokenSavings>) -> serde_json::Value {
    match savings {
        Some(savings) => estimated_potential_token_savings_json(savings),
        None => json!({
            "outcome_shape": "partial_context",
            "estimated_baseline_tokens": null,
            "estimated_returned_tokens": null,
            "estimated_potential_token_savings": null,
            "estimated_potential_token_savings_kind": MeasurementKind::Estimated.as_str(),
            "estimated_potential_token_savings_caveat": ESTIMATED_TOKEN_SAVING_CAVEAT,
            "unavailable_reason": "resolved file size unavailable; no estimate recorded",
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn family_detail_json(
    command: &str,
    family: &FamilyDetailReport,
    route: &FamilyQueryRouteReport,
    target: Option<&str>,
    mode: FamilyLookupMode,
    options: FamilyOutputOptions,
    prepared_output: Option<&PreparedFamilyOutput>,
    estimated_potential: Option<&EstimatedPotentialTokenSavings>,
) -> String {
    let output_components = family_output_components(
        family,
        target,
        mode,
        options,
        prepared_output,
        estimated_potential,
    );
    let selected_evidence = &output_components.selected_evidence;
    let read_plan = &output_components.read_plan;
    let estimated_potential = &output_components.estimated_potential_token_savings;
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let (rendered_members, members_truncated) =
        bounded_family_members(family, options.evidence_mode);
    let mut payload = json!({
        "command": command,
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "ok",
        "implemented": true,
        "active_generation": family.active_generation,
        // Found: `candidate_family_ids` == the follow-up handle, so it is demoted
        // out of the `minimal` shape (Standard tier).
        "query_route": query_route_json(route, options.verbosity, VerbosityTier::Standard),
        "family": {
            "family_id": family.family_id,
            "classification": family.classification,
            "support": family.support,
            "prevalence": family_prevalence_json(&family.prevalence),
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
        "constraint_profile": family_constraint_profile_json(family.constraint_profile.as_deref()),
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
        "read_plan": read_plan_json(read_plan, options.verbosity),
        "source_spans": source_spans_json(source_spans),
        "unknowns": unknowns_json(&family.unknowns),
    });
    insert_resolution(&mut payload, family.resolution.as_ref(), options.verbosity);
    drop_unrequested_source_spans(&mut payload, source_spans, options.verbosity);
    json_line(payload)
}

/// The additive top-level `resolution` object (CLI shape, byte-parallel with the
/// MCP [`resolution_value`] renderer). Renders the candidate-set cardinality
/// (`none`/`one`/`many`/`truncated`) plus bounded, source-free candidate
/// summaries, and never a `selected_family_id`.
fn resolution_json(resolution: &Resolution) -> serde_json::Value {
    json!({
        "cardinality": resolution.cardinality.as_str(),
        "candidates": resolution
            .candidates
            .iter()
            .map(|candidate| json!({
                "family_id": candidate.family_id,
                "summary": candidate.summary,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Insert the additive `resolution` object into a family response value when the
/// report carries one and the requested verbosity renders `Standard`-tier fields.
/// A `None` resolution (every non-scope outcome) leaves the value byte-stable.
fn insert_resolution(
    value: &mut serde_json::Value,
    resolution: Option<&Resolution>,
    verbosity: Verbosity,
) {
    if let Some(resolution) = resolution {
        if verbosity.renders(VerbosityTier::Standard) {
            if let Some(object) = value.as_object_mut() {
                object.insert("resolution".to_string(), resolution_json(resolution));
            }
        }
    }
}

/// Source-free JSON for the `query_route` envelope (CLI shape, byte-parallel with
/// the MCP [`query_route_value`] renderer).
///
/// The single serialization authority every family response routes through.
/// `route` and `follow_up_family_ids` are core (`minimal`); the follow-up handle
/// is the normalized union of `candidate_family_ids` and `selected_family_id`, so
/// those two are demoted at `minimal` without losing any id. `candidate_family_ids`
/// renders at `candidate_family_ids_tier` — `Minimal` when it is a narrowing
/// recovery handle (abstention / partial / conformance), `Standard` when it
/// merely duplicates the follow-up handle on a resolved Found route. The static
/// routing prose and term-retrieval telemetry are diagnostic (`Standard` tier).
/// `standard` (the byte-stable v1 default) and `full` render every field.
fn query_route_json(
    route: &FamilyQueryRouteReport,
    verbosity: Verbosity,
    candidate_family_ids_tier: VerbosityTier,
) -> serde_json::Value {
    let mut value = serde_json::Map::new();
    value.insert("route".to_string(), json!(route.route));
    value.insert(
        "follow_up_family_ids".to_string(),
        json!(route.follow_up_family_ids),
    );
    if verbosity.renders(candidate_family_ids_tier) {
        value.insert(
            "candidate_family_ids".to_string(),
            json!(route.candidate_family_ids),
        );
    }
    if verbosity.renders(VerbosityTier::Standard) {
        value.insert(
            "selected_family_id".to_string(),
            json!(route.selected_family_id),
        );
        value.insert("input_kind".to_string(), json!(route.input_kind));
        value.insert("pipeline".to_string(), json!(route.pipeline));
        value.insert(
            "family_id_policy".to_string(),
            json!(route.family_id_policy),
        );
        value.insert("candidate_limit".to_string(), json!(route.candidate_limit));
        value.insert("why_selected".to_string(), json!(route.why_selected));
        // Numeric fields the product-eval harness reads directly; null unless the
        // deterministic term-retrieval fallback produced this route.
        value.insert(
            "hydrated_family_count".to_string(),
            json!(route
                .term_retrieval
                .as_ref()
                .map(|term| term.hydrated_candidate_count)),
        );
        value.insert(
            "retrieval_stage_count".to_string(),
            json!(route
                .term_retrieval
                .as_ref()
                .map(|term| term.retrieval_stage_count)),
        );
        value.insert(
            "term_retrieval".to_string(),
            json!(route.term_retrieval.as_ref().map(term_retrieval_json)),
        );
    }
    serde_json::Value::Object(value)
}

/// Source-free JSON for a term-retrieval route. Shared shape with the MCP
/// renderer; carries only enum tokens, counts, and small integer scores.
pub(crate) fn term_retrieval_json(term: &TermRetrievalRoute) -> serde_json::Value {
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
    estimated_potential: Option<&EstimatedPotentialTokenSavings>,
) -> FamilyOutputComponents {
    let selected_evidence = select_family_evidence(family, options);
    let read_plan = prepared_output
        .map(|prepared| prepared.read_plan.clone())
        .unwrap_or_else(|| build_read_plan(family, target, mode, options));
    let source_spans = prepared_output.and_then(|prepared| prepared.source_spans.as_ref());
    let estimated_potential_token_savings = estimated_potential.cloned().unwrap_or_else(|| {
        estimate_family_output_potential_token_savings(
            family,
            &selected_evidence,
            &read_plan,
            source_spans,
        )
    });
    FamilyOutputComponents {
        selected_evidence,
        read_plan,
        estimated_potential_token_savings,
    }
}

fn resolved_target_json(target: &ResolvedQueryTarget, verbosity: Verbosity) -> serde_json::Value {
    let mut value = json!({
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
    });
    thin_resolved_target_json(
        value
            .as_object_mut()
            .expect("resolved target serializes to a JSON object"),
        target,
        verbosity,
    );
    value
}

/// CLI mirror of `thin_resolved_target`: trim the shared `resolved_target` object
/// for the `minimal` tier. Standard and full are byte-stable (no-op); at `minimal`
/// the input echo (`original_target`) and normalizer internals (`residue_terms`)
/// always drop, and each `candidate_*` narrowing list drops only when its concrete
/// counterpart resolved, so a genuinely ambiguous resolution keeps its recovery
/// handles. Kept byte-parallel with the MCP surface.
fn thin_resolved_target_json(
    object: &mut serde_json::Map<String, serde_json::Value>,
    target: &ResolvedQueryTarget,
    verbosity: Verbosity,
) {
    if verbosity.renders(VerbosityTier::Standard) {
        return;
    }
    object.remove("original_target");
    object.remove("residue_terms");
    if target.code_unit_id.is_some() {
        object.remove("candidate_code_unit_ids");
    }
    if !target.path.is_empty() {
        object.remove("candidate_paths");
    }
    if target.family_id.is_some() {
        object.remove("candidate_family_ids");
    }
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

fn read_plan_json(read_plan: &ReadPlan, verbosity: Verbosity) -> serde_json::Value {
    let mut plan = json!({
        "estimated_tokens": read_plan.estimated_tokens,
        "source_snippets_included": read_plan.source_snippets_included,
        "requires_source_before_edit": read_plan.requires_source_before_edit,
        "selection_strategy": read_plan.selection_strategy,
        "budget_satisfied": read_plan.budget_satisfied,
        "items": read_plan.items.iter().map(|item| read_plan_item_json(item, verbosity)).collect::<Vec<_>>(),
        "line_range_omissions": read_plan.line_range_omissions.iter().map(read_plan_line_range_omission_json).collect::<Vec<_>>(),
    });
    if !verbosity.renders(VerbosityTier::Standard) {
        // Minimal-only honesty flags (additive; `standard`/`full` keep the
        // pre-precision bytes). `item_count` mirrors the member cap's
        // `member_count`; `truncated` reveals that budget selection dropped
        // candidate spans so a trimmed plan never hides its own capping.
        if let Some(object) = plan.as_object_mut() {
            object.insert("item_count".to_string(), json!(read_plan.items.len()));
            object.insert("truncated".to_string(), json!(read_plan.truncated));
        }
    }
    plan
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

fn read_plan_item_json(item: &ReadPlanItem, verbosity: Verbosity) -> serde_json::Value {
    if !verbosity.renders(VerbosityTier::Standard) && item.source_snippets_included {
        // Dedup: this item's content is already inlined under `source_spans`
        // (a strict superset of the item's locus metadata), so at `minimal` the
        // plan carries only a back-reference stub. The plan still enumerates
        // what to read; the consumer treats the rendered span as already read
        // and does not pay for the repeated hash/byte/line locus.
        return json!({
            "purpose": item.purpose.as_str(),
            "path": item.path,
            "rendered": true,
        });
    }
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

/// At `minimal` verbosity, omit the `source_spans` key entirely when spans were
/// not requested rather than shipping the empty `{requested:false, spans:[],
/// omissions:[]}` stub. `standard`/`full` keep the stub for byte stability; the
/// key omission is an opt-in `minimal`-only reduction.
fn drop_unrequested_source_spans(
    payload: &mut serde_json::Value,
    source_spans: Option<&SourceSpanRenderReport>,
    verbosity: Verbosity,
) {
    if !verbosity.renders(VerbosityTier::Standard) && source_spans.is_none() {
        if let Some(object) = payload.as_object_mut() {
            object.remove("source_spans");
        }
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

fn unknown_inventory_human(command: &str, report: &UnknownInventoryReport) -> String {
    let mut output = format!(
        "{command}: typed UNKNOWN inventory\ninventory_scope: {}\nactive_generation: {}\ntotal_unknowns: {}\nblocking_unknowns: {}\nnon_blocking_unknowns: {}\nrecoverable_unknowns: {}\nirreducible_unknowns: {}\n",
        report.inventory_scope,
        report.active_generation,
        report.total_unknowns,
        report.blocking_unknowns,
        report.non_blocking_unknowns,
        report.recoverable_unknowns,
        report.irreducible_unknowns,
    );
    push_unknown_inventory_bucket_human(&mut output, "by_language", &report.by_language);
    push_unknown_inventory_bucket_human(&mut output, "by_reason_code", &report.by_reason_code);
    push_unknown_inventory_bucket_human(
        &mut output,
        "by_required_mechanism",
        &report.by_required_mechanism,
    );
    push_unknown_inventory_bucket_human(&mut output, "by_obligation", &report.by_obligation);
    push_unknown_inventory_bucket_human(
        &mut output,
        "by_framework_role",
        &report.by_framework_role,
    );
    push_unknown_inventory_bucket_human(&mut output, "by_role_state", &report.by_role_state);
    output.push_str("by_blocks_support:");
    if report.by_blocks_support.is_empty() {
        output.push_str(" none\n");
    } else {
        for bucket in &report.by_blocks_support {
            output.push_str(&format!(" {}={}", bucket.blocks_support, bucket.count));
        }
        output.push('\n');
    }
    push_unknown_inventory_bucket_human(&mut output, "by_recovery_code", &report.by_recovery_code);
    output
}

fn push_unknown_inventory_bucket_human(
    output: &mut String,
    label: &str,
    buckets: &[UnknownInventoryBucket],
) {
    output.push_str(label);
    output.push(':');
    if buckets.is_empty() {
        output.push_str(" none\n");
        return;
    }
    for bucket in buckets {
        output.push_str(&format!(" {}={}", bucket.key, bucket.count));
    }
    output.push('\n');
}

fn unknown_inventory_json(command: &str, report: &UnknownInventoryReport) -> String {
    json_line(json!({
        "command": command,
        "status": "ok",
        "implemented": true,
        "unknown_inventory": unknown_inventory_value(report),
    }))
}

fn unknown_inventory_value(report: &UnknownInventoryReport) -> serde_json::Value {
    json!({
        "inventory_scope": report.inventory_scope,
        "active_generation": report.active_generation,
        "total_unknowns": report.total_unknowns,
        "blocking_unknowns": report.blocking_unknowns,
        "non_blocking_unknowns": report.non_blocking_unknowns,
        "recoverable_unknowns": report.recoverable_unknowns,
        "irreducible_unknowns": report.irreducible_unknowns,
        "by_language": unknown_inventory_bucket_json("language", &report.by_language),
        "by_language_detail": unknown_inventory_language_detail_json(&report.by_language_detail),
        "by_reason_code": unknown_inventory_bucket_json("reason_code", &report.by_reason_code),
        "by_required_mechanism": unknown_inventory_bucket_json(
            "required_mechanism",
            &report.by_required_mechanism,
        ),
        "by_obligation": unknown_inventory_bucket_json("obligation", &report.by_obligation),
        "by_framework_role": unknown_inventory_bucket_json(
            "framework_role",
            &report.by_framework_role,
        ),
        "by_role_state": unknown_inventory_bucket_json("role_state", &report.by_role_state),
        "by_blocks_support": report.by_blocks_support.iter().map(|bucket| {
            json!({
                "blocks_support": bucket.blocks_support,
                "count": bucket.count,
            })
        }).collect::<Vec<_>>(),
        "by_recovery_code": unknown_inventory_bucket_json(
            "recovery_code",
            &report.by_recovery_code,
        ),
    })
}

fn unknown_inventory_language_detail_json(
    summaries: &[crate::application::query::UnknownInventoryLanguageSummary],
) -> Vec<serde_json::Value> {
    summaries
        .iter()
        .map(|summary| {
            json!({
                "language": summary.language,
                "total_unknowns": summary.total_unknowns,
                "blocking_unknowns": summary.blocking_unknowns,
                "top_required_mechanisms": unknown_inventory_bucket_json(
                    "required_mechanism",
                    &summary.top_required_mechanisms,
                ),
                "top_reason_codes": unknown_inventory_bucket_json(
                    "reason_code",
                    &summary.top_reason_codes,
                ),
            })
        })
        .collect()
}

fn unknown_inventory_bucket_json(
    key_name: &str,
    buckets: &[UnknownInventoryBucket],
) -> Vec<serde_json::Value> {
    buckets
        .iter()
        .map(|bucket| {
            json!({
                key_name: bucket.key,
                "count": bucket.count,
            })
        })
        .collect()
}

struct CliSetupProbe<'a, F, R> {
    runtime: &'a R,
    env_lookup: &'a F,
    install_context: &'a InstallExecutionContext,
    state_dir_override: Option<String>,
}

impl<F, R> SetupProbe for CliSetupProbe<'_, F, R>
where
    F: Fn(&str) -> Option<String>,
    R: CliRuntime,
{
    fn inspect_repository(
        &self,
        project: &str,
    ) -> Result<SetupRepositoryState, SetupOperationError> {
        let status = self
            .runtime
            .repository_status(RepositoryStatusRequest {
                path: project.to_string(),
                state_dir_override: self.state_dir_override.clone(),
            })
            .map_err(|_| setup_error(SetupFailureClass::ProjectInspectionFailed))?;
        let doctor = self
            .runtime
            .repository_doctor(RepositoryDoctorRequest {
                path: project.to_string(),
                state_dir_override: self.state_dir_override.clone(),
            })
            .map_err(|_| setup_error(SetupFailureClass::ProjectInspectionFailed))?;
        let lock_state = if doctor
            .findings
            .iter()
            .any(|finding| finding.code == RepositoryDoctorCode::IndexLockActive)
        {
            RecoveryLockState::Blocking
        } else if doctor.findings.iter().any(|finding| {
            matches!(
                finding.code,
                RepositoryDoctorCode::IndexLockUnknown
                    | RepositoryDoctorCode::IndexLockLegacy
                    | RepositoryDoctorCode::IndexLockInvalid
            )
        }) {
            RecoveryLockState::Unknown
        } else {
            RecoveryLockState::Clear
        };
        let storage_health = if status.storage == RepositoryImplementationStatus::Unhealthy {
            RecoveryHealth::Unhealthy
        } else if status.storage == RepositoryImplementationStatus::Available {
            RecoveryHealth::Healthy
        } else {
            RecoveryHealth::Unknown
        };
        let initialized = matches!(status.status, RepositoryStatus::Initialized { .. });
        let active_index = status.readiness.active_generation_available;
        let family_evidence = if active_index {
            match self.runtime.families(RepositoryStatusRequest {
                path: project.to_string(),
                state_dir_override: self.state_dir_override.clone(),
            }) {
                Ok(report) if report.families.is_empty() => RecoveryEvidenceState::Unavailable,
                Ok(_) => RecoveryEvidenceState::Available,
                Err(_) => RecoveryEvidenceState::Unknown,
            }
        } else {
            RecoveryEvidenceState::NotApplicable
        };
        Ok(SetupRepositoryState {
            initialized,
            active_index,
            freshness: repository_freshness_for_report(
                &status,
                status.readiness.active_generation_available,
            ),
            autosync_configured: status.readiness.autosync.configured,
            autosync_running: status.readiness.autosync.running,
            storage_health,
            lock_state,
            family_evidence,
        })
    }

    fn inspect_agent(&self, target: AgentTarget) -> Result<SetupAgentState, SetupOperationError> {
        let detected = agent_cli_detected(target, self.env_lookup);
        if !detected {
            return Ok(SetupAgentState {
                target,
                detected: false,
                live_writer: target.has_live_writer(InstallScope::Global),
                integration: SetupAgentIntegrationState::Unmanaged,
            });
        }
        let integration = match self.runtime.inspect_agent_integration(
            target,
            InstallScope::Global,
            self.install_context,
        ) {
            Ok(AgentIntegrationInspection::OwnedCurrent) => {
                SetupAgentIntegrationState::OwnedCurrent
            }
            Ok(AgentIntegrationInspection::OwnedOutdated) => {
                SetupAgentIntegrationState::OwnedOutdated
            }
            Ok(AgentIntegrationInspection::Unmanaged) => SetupAgentIntegrationState::Unmanaged,
            Ok(AgentIntegrationInspection::Foreign) => SetupAgentIntegrationState::Foreign,
            Ok(AgentIntegrationInspection::OwnedDrifted) => SetupAgentIntegrationState::Malformed,
            Ok(AgentIntegrationInspection::Malformed) => SetupAgentIntegrationState::Malformed,
            Err(_) => return Err(setup_error(SetupFailureClass::AgentDetectionFailed)),
        };
        Ok(SetupAgentState {
            target,
            detected,
            live_writer: target.has_live_writer(InstallScope::Global),
            integration,
        })
    }
}

struct CliSetupExecution<'a, F, R> {
    runtime: &'a R,
    current_dir: &'a Path,
    env_lookup: &'a F,
    install_context: InstallExecutionContext,
    options: &'a SetupCliOptions,
    repository_root: &'a str,
}

impl<F, R> SetupExecutionPort for CliSetupExecution<'_, F, R>
where
    F: Fn(&str) -> Option<String>,
    R: CliRuntime,
{
    fn configure_agent_integrations(
        &self,
        targets: &[AgentTarget],
    ) -> Result<SetupAgentMutation, SetupOperationError> {
        self.runtime
            .install_agent_integration(
                "install",
                setup_install_request(targets),
                self.install_context.clone(),
            )
            .map(|outcome| SetupAgentMutation {
                newly_configured: outcome.configured_targets,
                reconfigured: outcome.reconfigured_targets,
            })
            .map_err(|error| classify_setup_operation_error(SetupStage::AgentIntegration, &error))
    }

    fn initialize_repository(&self) -> Result<SetupRepositoryMutation, SetupOperationError> {
        init_repository(RepositoryLifecycleInitRequest {
            path: self.repository_root.to_string(),
            state_dir_override: state_dir_override(self.env_lookup),
            write_root_gitignore: false,
        })
        .map(|outcome| SetupRepositoryMutation {
            created: outcome.created,
        })
        .map_err(|_| setup_error(SetupFailureClass::RepositoryInitializationFailed))
    }

    fn index_repository(&self) -> Result<SetupIndexSummary, SetupOperationError> {
        let lifecycle = LifecycleOptions {
            project_path: Some(self.repository_root.to_string()),
            json: self.options.json,
            progress: self.options.progress,
            ..LifecycleOptions::default()
        };
        let request = build_cli_index_request(&lifecycle, self.current_dir, self.env_lookup)
            .map_err(|_| setup_error(SetupFailureClass::IndexFailed))?;
        let outcome = self
            .runtime
            .index_repository("resync", request)
            .map_err(|_| setup_error(SetupFailureClass::IndexFailed))?;
        let family_inventory = match self.runtime.families(RepositoryStatusRequest {
            path: self.repository_root.to_string(),
            state_dir_override: state_dir_override(self.env_lookup),
        }) {
            Ok(report) => SetupFamilyInventory::Available(report.families.len()),
            Err(_) => SetupFamilyInventory::Unknown,
        };
        Ok(SetupIndexSummary {
            indexed_files: outcome.discovered_files,
            family_inventory,
        })
    }

    fn start_autosync(&self) -> Result<SetupAutosyncMutation, SetupOperationError> {
        let settings = AutosyncSettings::default();
        let request = build_autosync_request(
            Some(self.repository_root),
            self.options.json,
            false,
            settings.poll_ms,
            settings.debounce_ms,
            self.current_dir,
            self.env_lookup,
        );
        self.runtime
            .autosync(AutosyncCommand::Start, request)
            .map(|report| SetupAutosyncMutation {
                started: report.running,
            })
            .map_err(|_| setup_error(SetupFailureClass::AutosyncFailed))
    }

    fn mcp_self_test(&self, _targets: &[AgentTarget]) -> Result<(), SetupOperationError> {
        self.runtime
            .mcp_self_test(self.repository_root)
            .map_err(setup_error)
    }

    fn rollback_agent_integrations(
        &self,
        targets: &[AgentTarget],
    ) -> Result<(), SetupOperationError> {
        self.runtime
            .install_agent_integration(
                "uninstall",
                setup_install_request(targets),
                self.install_context.clone(),
            )
            .map(|_| ())
            .map_err(|_| setup_error(SetupFailureClass::RollbackFailed))
    }
}

fn handle_setup<F>(
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
    prompt: &impl InstallTelemetryPrompt,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_setup_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let repository_root = repository_root(current_dir, options.project_path.as_deref());
    let install_context = match install_execution_context(current_dir, env_lookup) {
        Ok(context) => context,
        Err(_) => {
            return setup_planning_error(
                options.json,
                SetupStage::Inspect,
                SetupFailureClass::ProjectInspectionFailed,
            );
        }
    };
    let probe = CliSetupProbe {
        runtime,
        env_lookup,
        install_context: &install_context,
        state_dir_override: state_dir_override(env_lookup),
    };
    let mut request = SetupRequest::new(repository_root.clone());
    request.target = options.target;
    request.dry_run = options.dry_run;
    request.autosync = options.autosync;
    let plan = match plan_setup(request, &probe) {
        Ok(plan) => plan,
        Err(failure) => return setup_planning_error(options.json, failure.stage, failure.class),
    };

    let authorization = if options.dry_run {
        SetupAuthorization::DryRun
    } else if options.yes || plan.confirmation() == SetupConfirmation::NotRequiredNoMutation {
        SetupAuthorization::Confirmed
    } else if !prompt.is_interactive() {
        return CliOutput::failure(
            2,
            "setup requires one confirmation before writes; use --dry-run to inspect or --yes for noninteractive setup\n",
        );
    } else {
        let response = match prompt.prompt_setup_confirmation(&format!(
            "{}\nProceed with setup? [Y/n] ",
            setup_plan_human(&plan)
        )) {
            Ok(response) => response,
            Err(error) => return CliOutput::failure(2, format!("{error}\n")),
        };
        match parse_default_yes_prompt_response(&response) {
            Ok(true) => SetupAuthorization::Confirmed,
            Ok(false) => return CliOutput::success("setup cancelled; no changes made\n"),
            Err(error) => return CliOutput::failure(2, format!("{error}\n")),
        }
    };

    let operations = CliSetupExecution {
        runtime,
        current_dir,
        env_lookup,
        install_context,
        options: &options,
        repository_root: &repository_root,
    };
    let outcome = execute_setup(&plan, authorization, &operations);
    let status = i32::from(outcome.status == SetupOutcomeStatus::Failed);
    if options.json {
        let output = setup_outcome_json(&plan, &outcome);
        CliOutput {
            status,
            stdout: output,
            stderr: String::new(),
        }
    } else if status == 0 {
        CliOutput::success(setup_outcome_human(&plan, &outcome))
    } else {
        CliOutput::failure(status, setup_outcome_human(&plan, &outcome))
    }
}

fn setup_install_request(targets: &[AgentTarget]) -> InstallRequest {
    InstallRequest {
        target: if targets.len() == supported_concrete_targets().len() {
            AgentTarget::AllSupported
        } else {
            targets.first().copied().unwrap_or(AgentTarget::None)
        },
        scope: InstallScope::Global,
        assume_yes: true,
        telemetry_enabled: false,
        telemetry_explicitly_configured: false,
        selected_targets: targets.to_vec(),
        ..InstallRequest::default()
    }
}

fn setup_error(class: SetupFailureClass) -> SetupOperationError {
    SetupOperationError::new(class)
}

fn classify_setup_operation_error(
    stage: SetupStage,
    _error: &RepoGrammarError,
) -> SetupOperationError {
    let class = match stage {
        SetupStage::AgentIntegration => SetupFailureClass::NativeAgentConfigurationFailed,
        _ => SetupFailureClass::InvalidOperationResult,
    };
    setup_error(class)
}

fn setup_planning_error(json: bool, stage: SetupStage, class: SetupFailureClass) -> CliOutput {
    if json {
        CliOutput {
            status: 1,
            stdout: json_line(json!({
                "command": "setup",
                "status": "failed",
                "failed_stage": setup_stage_token(stage),
                "failure_class": setup_failure_token(class),
                "recovery": "repogrammar doctor",
            })),
            stderr: String::new(),
        }
    } else {
        CliOutput::failure(
            1,
            format!(
                "setup: could not inspect safely\nfailed step: {}\nnext: repogrammar doctor\n",
                setup_stage_label(stage)
            ),
        )
    }
}

fn setup_plan_human(plan: &SetupPlan) -> String {
    let mut output = String::from("Setup plan:\n");
    for action in plan.actions() {
        output.push_str(&format!(
            "- {}: {}\n",
            setup_stage_label(action.stage),
            setup_disposition_label(action.disposition)
        ));
    }
    output.push_str("- telemetry: unchanged by setup; off by default\n");
    output.push_str("- rollback: only changes created and owned by this run\n");
    output
}

fn setup_outcome_human(plan: &SetupPlan, outcome: &SetupOutcome) -> String {
    if outcome.status == SetupOutcomeStatus::Failed {
        let Some(failure) = outcome.failure else {
            return "setup: not ready\nnext: repogrammar doctor\n".to_string();
        };
        let completed = outcome
            .stages
            .iter()
            .filter(|stage| stage.status == SetupStageStatus::Completed)
            .map(|stage| setup_stage_label(stage.stage))
            .collect::<Vec<_>>();
        let retained = outcome
            .preserved
            .iter()
            .map(|resource| setup_preserved_label(*resource))
            .collect::<Vec<_>>();
        let rollback = match (&outcome.rollback, failure.rollback_failure) {
            (Some(rollback), None) if rollback.succeeded => {
                "new agent integration removed".to_string()
            }
            (Some(_), Some(class)) => {
                format!("failed ({})", setup_failure_token(class))
            }
            (Some(_), None) => "failed (rollback_failed)".to_string(),
            (None, _) => "not required".to_string(),
        };
        let mut output = format!(
            "setup: not ready\ncompleted: {}\nretained: {}\nrollback: {}\nfailed: {} ({})\n",
            comma_list_or_none(&completed),
            comma_list_or_none(&retained),
            rollback,
            setup_stage_label(failure.stage),
            setup_failure_token(failure.class),
        );
        output.push_str(&format!(
            "next: {}\n",
            recovery_command(outcome.recovery.action)
        ));
        return output;
    }

    let mut output = match outcome.status {
        SetupOutcomeStatus::DryRun => setup_plan_human(plan),
        SetupOutcomeStatus::Ready => String::from("setup: ready\n"),
        SetupOutcomeStatus::ReadyWithLimitations => {
            String::from("setup: completed with limitations\n")
        }
        SetupOutcomeStatus::Failed => unreachable!(),
    };
    if let Some(index) = outcome.index {
        match index.family_inventory {
            SetupFamilyInventory::Available(0) => {
                output.push_str(&format!(
                    "repository: {} files indexed; no supported pattern groups verified\n",
                    index.indexed_files
                ));
            }
            SetupFamilyInventory::Available(count) => {
                output.push_str(&format!(
                    "repository: {} files indexed, {} pattern groups verified\n",
                    index.indexed_files, count
                ));
            }
            SetupFamilyInventory::Unknown => {
                output.push_str(&format!(
                    "repository: {} files indexed; pattern-group inventory unavailable\n",
                    index.indexed_files
                ));
            }
        }
    } else if plan.repository_before().active_index {
        output.push_str("repository: existing active index preserved\n");
    } else if outcome.status != SetupOutcomeStatus::DryRun {
        output.push_str("repository: active index created\n");
    }
    for limitation in &outcome.limitations {
        output.push_str(&format!(
            "limitation: {}\n",
            setup_limitation_label(*limitation)
        ));
    }
    if outcome.status != SetupOutcomeStatus::DryRun {
        output.push_str("telemetry: unchanged by setup; off by default\n");
        if setup_product_self_test_passed(outcome) {
            output.push_str("product MCP: repogrammar_context self-test passed\n");
        }
        let ready_targets = setup_ready_agent_targets(plan, outcome);
        if ready_targets.is_empty() {
            output.push_str(
                "agent MCP: not active; use the repository index through the RepoGrammar CLI\n",
            );
        } else {
            output.push_str(&format!(
                "agent MCP: ready for {}\n",
                ready_targets
                    .iter()
                    .map(|target| target.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if setup_product_self_test_passed(outcome) {
                output.push_str(
                    "Ask your coding agent: \"How are API routes implemented in this repository?\"\n",
                );
            }
        }
    }
    output.push_str(&format!(
        "next: {}\n",
        recovery_command(outcome.recovery.action)
    ));
    output
}

fn setup_outcome_json(plan: &SetupPlan, outcome: &SetupOutcome) -> String {
    let ready_agent_targets = setup_ready_agent_targets(plan, outcome);
    let blocked_agent_targets = setup_blocked_agent_targets(plan, &ready_agent_targets);
    let product_self_test_passed = setup_product_self_test_passed(outcome);
    let family_evidence = setup_family_evidence(plan, outcome);
    let agent_query_ready = product_self_test_passed && !ready_agent_targets.is_empty();
    json_line(json!({
        "command": "setup",
        "status": setup_outcome_status_token(outcome.status),
        "target": setup_target_token(plan.request().target),
        "autosync_requested": plan.request().autosync,
        "telemetry_changed": false,
        "telemetry_enabled_by_setup": false,
        "ready_agent_targets": ready_agent_targets.iter().map(|target| target.as_str()).collect::<Vec<_>>(),
        "blocked_agent_targets": blocked_agent_targets.iter().map(|target| target.as_str()).collect::<Vec<_>>(),
        "product_self_test_state": setup_product_self_test_state(outcome),
        "agent_query_ready": agent_query_ready,
        "repository_index_ready": setup_repository_index_ready(plan, outcome),
        "autosync_ready": setup_autosync_ready(plan, outcome),
        "family_evidence_state": setup_family_evidence_token(family_evidence),
        "stages": outcome.stages.iter().map(|report| json!({
            "stage": setup_stage_token(report.stage),
            "status": setup_stage_status_token(report.status),
        })).collect::<Vec<_>>(),
        "limitations": outcome.limitations.iter().map(|limitation| setup_limitation_token(*limitation)).collect::<Vec<_>>(),
        "preserved": outcome.preserved.iter().map(|resource| setup_preserved_token(*resource)).collect::<Vec<_>>(),
        "rollback": outcome.rollback.as_ref().map(|rollback| json!({
            "targets": rollback.targets.iter().map(|target| target.as_str()).collect::<Vec<_>>(),
            "succeeded": rollback.succeeded,
        })),
        "index": outcome.index.map(|index| json!({
            "indexed_files": index.indexed_files,
            "pattern_groups": match index.family_inventory {
                SetupFamilyInventory::Available(count) => Some(count),
                SetupFamilyInventory::Unknown => None,
            },
            "family_evidence_state": setup_family_inventory_token(index.family_inventory),
        })),
        "failure": outcome.failure.map(|failure| json!({
            "stage": setup_stage_token(failure.stage),
            "class": setup_failure_token(failure.class),
            "rollback_failure": failure.rollback_failure.map(setup_failure_token),
        })),
        "recovery": recovery_command(outcome.recovery.action),
        "suggested_question": if agent_query_ready {
            Some("How are API routes implemented in this repository?")
        } else {
            None
        },
    }))
}

fn setup_stage_has_status(
    outcome: &SetupOutcome,
    stage: SetupStage,
    status: SetupStageStatus,
) -> bool {
    outcome
        .stages
        .iter()
        .any(|report| report.stage == stage && report.status == status)
}

fn setup_product_self_test_passed(outcome: &SetupOutcome) -> bool {
    setup_stage_has_status(
        outcome,
        SetupStage::McpSelfTest,
        SetupStageStatus::Completed,
    )
}

fn setup_product_self_test_state(outcome: &SetupOutcome) -> &'static str {
    if setup_product_self_test_passed(outcome) {
        "passed"
    } else if outcome
        .failure
        .is_some_and(|failure| failure.stage == SetupStage::McpSelfTest)
    {
        "failed"
    } else if outcome.status == SetupOutcomeStatus::DryRun {
        "planned"
    } else {
        "not_run"
    }
}

fn setup_ready_agent_targets(plan: &SetupPlan, outcome: &SetupOutcome) -> Vec<AgentTarget> {
    if outcome.status == SetupOutcomeStatus::DryRun {
        return Vec::new();
    }
    let integration_completed = setup_stage_has_status(
        outcome,
        SetupStage::AgentIntegration,
        SetupStageStatus::Completed,
    );
    let rolled_back = outcome
        .rollback
        .as_ref()
        .map(|rollback| rollback.targets.as_slice())
        .unwrap_or(&[]);
    plan.agents()
        .iter()
        .filter(|agent| agent.detected && agent.live_writer)
        .filter(|agent| match agent.integration {
            SetupAgentIntegrationState::OwnedCurrent => true,
            SetupAgentIntegrationState::OwnedOutdated => integration_completed,
            SetupAgentIntegrationState::Unmanaged => {
                integration_completed && !rolled_back.contains(&agent.target)
            }
            SetupAgentIntegrationState::Foreign | SetupAgentIntegrationState::Malformed => false,
        })
        .map(|agent| agent.target)
        .collect()
}

fn setup_blocked_agent_targets(
    plan: &SetupPlan,
    ready_targets: &[AgentTarget],
) -> Vec<AgentTarget> {
    plan.agents()
        .iter()
        .filter(|agent| !ready_targets.contains(&agent.target))
        .map(|agent| agent.target)
        .collect()
}

fn setup_repository_index_ready(plan: &SetupPlan, outcome: &SetupOutcome) -> bool {
    (plan.repository_before().active_index
        && plan.repository_before().freshness == RecoveryFreshness::Fresh)
        || setup_stage_has_status(
            outcome,
            SetupStage::RepositoryIndex,
            SetupStageStatus::Completed,
        )
}

fn setup_autosync_ready(plan: &SetupPlan, outcome: &SetupOutcome) -> bool {
    plan.request().autosync
        && (plan.repository_before().autosync_running
            || setup_stage_has_status(outcome, SetupStage::Autosync, SetupStageStatus::Completed))
}

fn setup_family_evidence(plan: &SetupPlan, outcome: &SetupOutcome) -> RecoveryEvidenceState {
    match outcome.index.map(|index| index.family_inventory) {
        Some(SetupFamilyInventory::Available(count)) if count > 0 => {
            RecoveryEvidenceState::Available
        }
        Some(SetupFamilyInventory::Available(_)) => RecoveryEvidenceState::Unavailable,
        Some(SetupFamilyInventory::Unknown) => RecoveryEvidenceState::Unknown,
        None => plan.repository_before().family_evidence,
    }
}

fn setup_family_inventory_token(inventory: SetupFamilyInventory) -> &'static str {
    match inventory {
        SetupFamilyInventory::Available(0) => "available_zero",
        SetupFamilyInventory::Available(_) => "available",
        SetupFamilyInventory::Unknown => "unknown",
    }
}

fn setup_family_evidence_token(evidence: RecoveryEvidenceState) -> &'static str {
    match evidence {
        RecoveryEvidenceState::Available => "available",
        RecoveryEvidenceState::Unavailable => "available_zero",
        RecoveryEvidenceState::Unknown => "unknown",
        RecoveryEvidenceState::NotApplicable => "not_applicable",
    }
}

fn setup_target_token(target: SetupTarget) -> &'static str {
    match target {
        SetupTarget::Auto => "auto",
        SetupTarget::Codex => "codex",
        SetupTarget::ClaudeCode => "claude-code",
    }
}

fn setup_outcome_status_token(status: SetupOutcomeStatus) -> &'static str {
    match status {
        SetupOutcomeStatus::DryRun => "dry_run",
        SetupOutcomeStatus::Ready => "ready",
        SetupOutcomeStatus::ReadyWithLimitations => "ready_with_limitations",
        SetupOutcomeStatus::Failed => "failed",
    }
}

fn setup_stage_token(stage: SetupStage) -> &'static str {
    match stage {
        SetupStage::Inspect => "inspect",
        SetupStage::Confirm => "confirm",
        SetupStage::AgentIntegration => "agent_integration",
        SetupStage::RepositoryInitialization => "repository_initialization",
        SetupStage::RepositoryIndex => "repository_index",
        SetupStage::Autosync => "autosync",
        SetupStage::McpSelfTest => "mcp_self_test",
        SetupStage::RollbackMachineIntegration => "rollback_machine_integration",
    }
}

fn setup_stage_label(stage: SetupStage) -> &'static str {
    match stage {
        SetupStage::Inspect => "environment inspection",
        SetupStage::Confirm => "confirmation",
        SetupStage::AgentIntegration => "agent MCP wiring",
        SetupStage::RepositoryInitialization => "repository initialization",
        SetupStage::RepositoryIndex => "repository indexing",
        SetupStage::Autosync => "autosync",
        SetupStage::McpSelfTest => "MCP self-test",
        SetupStage::RollbackMachineIntegration => "agent rollback",
    }
}

fn setup_disposition_label(disposition: SetupDisposition) -> &'static str {
    match disposition {
        SetupDisposition::Execute => "will run",
        SetupDisposition::SkipAlreadyComplete => "already complete",
        SetupDisposition::Disabled => "disabled",
        SetupDisposition::Unavailable => "unavailable; repository setup can continue",
        SetupDisposition::Blocked => "blocked; existing state will be preserved",
    }
}

fn setup_stage_status_token(status: SetupStageStatus) -> &'static str {
    match status {
        SetupStageStatus::Planned => "planned",
        SetupStageStatus::Completed => "completed",
        SetupStageStatus::Skipped => "skipped",
        SetupStageStatus::Disabled => "disabled",
        SetupStageStatus::Unavailable => "unavailable",
        SetupStageStatus::Blocked => "blocked",
        SetupStageStatus::Failed => "failed",
        SetupStageStatus::RolledBack => "rolled_back",
        SetupStageStatus::RollbackFailed => "rollback_failed",
    }
}

fn setup_failure_token(class: SetupFailureClass) -> &'static str {
    match class {
        SetupFailureClass::ProjectInspectionFailed => "project_inspection_failed",
        SetupFailureClass::AgentDetectionFailed => "agent_detection_failed",
        SetupFailureClass::AuthorizationRequired => "authorization_required",
        SetupFailureClass::NativeAgentConfigurationFailed => "native_agent_configuration_failed",
        SetupFailureClass::ReceiptWriteFailed => "receipt_write_failed",
        SetupFailureClass::ForeignAgentConfiguration => "foreign_agent_configuration",
        SetupFailureClass::MalformedAgentConfiguration => "malformed_agent_configuration",
        SetupFailureClass::StorageUnhealthy => "storage_unhealthy",
        SetupFailureClass::BlockingLock => "blocking_lock",
        SetupFailureClass::RepositoryInitializationFailed => "repository_initialization_failed",
        SetupFailureClass::IndexFailed => "index_failed",
        SetupFailureClass::AutosyncFailed => "autosync_failed",
        SetupFailureClass::McpSelfTestTimedOut => "mcp_self_test_timed_out",
        SetupFailureClass::McpSelfTestFailed => "mcp_self_test_failed",
        SetupFailureClass::RollbackFailed => "rollback_failed",
        SetupFailureClass::InvalidOperationResult => "invalid_operation_result",
    }
}

fn setup_limitation_label(limitation: SetupLimitation) -> &'static str {
    match limitation {
        SetupLimitation::AgentMissing(_) => "the selected agent CLI was not detected",
        SetupLimitation::NoLiveAgent => "no supported live agent CLI was detected",
        SetupLimitation::ForeignAgentConfiguration(_) => {
            "foreign agent configuration was preserved"
        }
        SetupLimitation::MalformedAgentConfiguration(_) => {
            "malformed agent configuration was preserved"
        }
        SetupLimitation::StorageUnhealthy => "repository storage needs repair",
        SetupLimitation::BlockingLock => "repository indexing is locked",
        SetupLimitation::NoPatternGroups => "no supported pattern groups were verified",
    }
}

fn setup_limitation_token(limitation: SetupLimitation) -> &'static str {
    match limitation {
        SetupLimitation::AgentMissing(_) => "agent_missing",
        SetupLimitation::NoLiveAgent => "no_live_agent",
        SetupLimitation::ForeignAgentConfiguration(_) => "foreign_agent_configuration",
        SetupLimitation::MalformedAgentConfiguration(_) => "malformed_agent_configuration",
        SetupLimitation::StorageUnhealthy => "storage_unhealthy",
        SetupLimitation::BlockingLock => "blocking_lock",
        SetupLimitation::NoPatternGroups => "no_pattern_groups",
    }
}

fn setup_preserved_token(resource: SetupPreservedResource) -> &'static str {
    match resource {
        SetupPreservedResource::PreExistingAgentIntegration(_) => "preexisting_agent_integration",
        SetupPreservedResource::PreExistingRepositoryState => "preexisting_repository_state",
        SetupPreservedResource::ActiveGeneration => "active_generation",
        SetupPreservedResource::AutosyncProcess => "autosync_process",
        SetupPreservedResource::RepositoryStateCreatedThisRun => {
            "repository_state_created_this_run"
        }
    }
}

fn setup_preserved_label(resource: SetupPreservedResource) -> &'static str {
    match resource {
        SetupPreservedResource::PreExistingAgentIntegration(_) => "pre-existing agent integration",
        SetupPreservedResource::PreExistingRepositoryState => "pre-existing repository state",
        SetupPreservedResource::ActiveGeneration => "active repository index",
        SetupPreservedResource::AutosyncProcess => "autosync process",
        SetupPreservedResource::RepositoryStateCreatedThisRun => {
            "repository state created by this run"
        }
    }
}

fn comma_list_or_none(values: &[&str]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstructionCliOptions {
    operation: ManagedInstructionOperation,
    file: String,
    dry_run: bool,
    assume_yes: bool,
    json: bool,
}

fn parse_instruction_options(rest: &[String]) -> Result<InstructionCliOptions, String> {
    let Some(subcommand) = rest.first() else {
        return Err("instructions requires status, sync, or remove plus --file <path>".to_string());
    };
    let operation = match subcommand.as_str() {
        "status" => ManagedInstructionOperation::Status,
        "sync" => ManagedInstructionOperation::Sync,
        "remove" => ManagedInstructionOperation::Remove,
        _ => return Err("instructions subcommand must be status, sync, or remove".to_string()),
    };
    let mut file = None;
    let mut dry_run = false;
    let mut assume_yes = false;
    let mut json = false;
    let mut index = 1;
    while index < rest.len() {
        match rest[index].as_str() {
            "--file" => {
                index += 1;
                let value = rest
                    .get(index)
                    .ok_or_else(|| "--file requires a path".to_string())?;
                if file.is_some() {
                    return Err("--file may be supplied only once".to_string());
                }
                if value.trim().is_empty() || value.chars().any(char::is_control) {
                    return Err(
                        "--file requires a non-blank path without control characters".to_string(),
                    );
                }
                file = Some(value.clone());
            }
            "--dry-run" => dry_run = true,
            "--yes" => assume_yes = true,
            "--json" => json = true,
            option => return Err(format!("unknown instructions option: {option}")),
        }
        index += 1;
    }
    if operation == ManagedInstructionOperation::Status && dry_run {
        return Err(
            "instructions status is already read-only and does not accept --dry-run".to_string(),
        );
    }
    if operation == ManagedInstructionOperation::Status && assume_yes {
        return Err("instructions status does not accept --yes".to_string());
    }
    Ok(InstructionCliOptions {
        operation,
        file: file.ok_or_else(|| "instructions requires --file <path>".to_string())?,
        dry_run,
        assume_yes,
        json,
    })
}

fn instruction_result_status(outcome: &ManagedInstructionOutcome) -> &'static str {
    if outcome.refusal.is_some() {
        "refused"
    } else if outcome.dry_run {
        "dry_run"
    } else {
        "ok"
    }
}

fn instruction_session_restart_recommended(outcome: &ManagedInstructionOutcome) -> bool {
    outcome.operation == ManagedInstructionOperation::Sync
        && !outcome.dry_run
        && outcome.refusal.is_none()
        && outcome.state_after == ManagedInstructionState::Current
}

fn instruction_outcome_json(outcome: &ManagedInstructionOutcome) -> String {
    json_line(json!({
        "command": format!("instructions {}", outcome.operation.as_str()),
        "status": instruction_result_status(outcome),
        "operation": outcome.operation.as_str(),
        "state_before": outcome.state_before.as_str(),
        "state_after": outcome.state_after.as_str(),
        "detected_content_version": outcome.state_before.content_version(),
        "expected_content_version": MANAGED_INSTRUCTION_VERSION,
        "file_existed": outcome.file_existed,
        "dry_run": outcome.dry_run,
        "would_change": outcome.would_change,
        "changed": outcome.changed,
        "action": outcome.disposition.as_str(),
        "session_restart_recommended": instruction_session_restart_recommended(outcome),
        "repairable": !matches!(
            outcome.state_before,
            crate::application::install::ManagedInstructionState::Foreign
                | crate::application::install::ManagedInstructionState::Malformed
        ),
        "refusal": outcome.refusal.map(ManagedInstructionRefusal::as_str),
    }))
}

fn instruction_outcome_human(outcome: &ManagedInstructionOutcome) -> String {
    if let Some(refusal) = outcome.refusal {
        let guidance = match refusal {
            ManagedInstructionRefusal::ConfirmationRequired => {
                "rerun with --dry-run to inspect or --yes to authorize this one file"
            }
            ManagedInstructionRefusal::ForeignSection => {
                "the unrecognized managed section was preserved; review it manually"
            }
            ManagedInstructionRefusal::MalformedSection => {
                "the malformed or duplicated markers were preserved; repair them manually"
            }
        };
        return format!(
            "instructions {}: refused ({})\nnext: {guidance}\n",
            outcome.operation.as_str(),
            refusal.as_str()
        );
    }
    let mut rendered = format!(
        "instructions {}: state={} action={} expected_version={}{}\n",
        outcome.operation.as_str(),
        outcome.state_before.as_str(),
        outcome.disposition.as_str(),
        MANAGED_INSTRUCTION_VERSION,
        if outcome.dry_run { " dry_run=true" } else { "" }
    );
    if instruction_session_restart_recommended(outcome) {
        rendered.push_str(
            "next: restart the coding-agent session; already-open Codex/Claude MCP child processes do not hot-swap RepoGrammar binaries or managed instructions\n",
        );
    }
    rendered
}

fn instruction_failure_output(operation: ManagedInstructionOperation, json: bool) -> CliOutput {
    if json {
        CliOutput::failure(
            2,
            json_line(json!({
                "command": format!("instructions {}", operation.as_str()),
                "status": "error",
                "reason": "instruction_file_unavailable",
                "expected_content_version": MANAGED_INSTRUCTION_VERSION,
            })),
        )
    } else {
        CliOutput::failure(
            2,
            "instruction file operation failed safely; the selected file was preserved\n",
        )
    }
}

fn handle_instructions(rest: &[String], current_dir: &Path) -> CliOutput {
    let options = match parse_instruction_options(rest) {
        Ok(options) => options,
        Err(error) => return CliOutput::failure(2, format!("{error}\n")),
    };
    let supplied = PathBuf::from(&options.file);
    let path = if supplied.is_absolute() {
        supplied
    } else {
        current_dir.join(supplied)
    };
    let outcome = match manage_instruction_file(&ManagedInstructionRequest {
        path,
        operation: options.operation,
        dry_run: options.dry_run,
        assume_yes: options.assume_yes,
    }) {
        Ok(outcome) => outcome,
        Err(_) => return instruction_failure_output(options.operation, options.json),
    };
    let refused = outcome.refusal.is_some();
    let output = if options.json {
        instruction_outcome_json(&outcome)
    } else {
        instruction_outcome_human(&outcome)
    };
    if refused {
        CliOutput::failure(2, output)
    } else {
        CliOutput::success(output)
    }
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
    if outcome.command == "install" {
        output.push_str(
            "next: restart the coding-agent session; already-open Codex/Claude MCP child processes do not hot-swap RepoGrammar binaries or managed instructions\n",
        );
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct UnknownsOptions {
    json: bool,
    project_path: Option<String>,
}

fn handle_unknowns<F>(
    rest: &[String],
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    let options = match parse_unknowns_options(rest) {
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
            return unknowns_fallback(
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
            return unknowns_fallback(
                options.json,
                fallback.reason,
                fallback.guidance,
                fallback.implemented,
            );
        }
        QueryPreflightReport::Ready => {}
    }

    match runtime.unknown_inventory(request) {
        Ok(report) if options.json => {
            CliOutput::success(unknown_inventory_json("unknowns", &report))
        }
        Ok(report) => CliOutput::success(unknown_inventory_human("unknowns", &report)),
        Err(_) => unknowns_fallback(
            options.json,
            "repository status is unavailable",
            "run repogrammar doctor",
            true,
        ),
    }
}

fn parse_unknowns_options(rest: &[String]) -> Result<UnknownsOptions, String> {
    let mut options = UnknownsOptions::default();
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
            other => return Err(format!("unknown unknowns option: {other}")),
        }
    }
    Ok(options)
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
            return stats_fallback(
                options.json,
                fallback.reason,
                fallback.guidance,
                options.include_unknowns,
            );
        }
    };

    match query_preflight(
        QueryPreflightOperation::ActiveIndexInventory,
        &status_report,
    ) {
        QueryPreflightReport::Fallback(fallback) => {
            return stats_fallback(
                options.json,
                fallback.reason,
                fallback.guidance,
                options.include_unknowns,
            );
        }
        QueryPreflightReport::Ready => {}
    }

    if options.json {
        return match runtime.repo_shape_diagnostics(request.clone()) {
            Ok(report) => {
                let measurement = telemetry_global_data_dir(env_lookup)
                    .ok()
                    .and_then(|dir| latest_comparable_experiment_report(&dir).ok().flatten());
                let query_metrics =
                    family_query_metrics_rollup(request.clone()).unwrap_or_default();
                let unknown_inventory = if options.include_unknowns {
                    match runtime.unknown_inventory(request.clone()) {
                        Ok(report) => Some(report),
                        Err(_) => {
                            return stats_fallback(
                                true,
                                "repository status is unavailable",
                                "run repogrammar doctor",
                                options.include_unknowns,
                            );
                        }
                    }
                } else {
                    None
                };
                record_stats_telemetry_rollup(
                    current_dir,
                    env_lookup,
                    options.project_path.as_deref(),
                    &report,
                    measurement.as_ref(),
                );
                CliOutput::success(stats_json(
                    &report,
                    &status_report.readiness,
                    measurement.as_ref(),
                    &query_metrics,
                    unknown_inventory.as_ref(),
                ))
            }
            Err(_) => stats_fallback(
                true,
                "repository status is unavailable",
                "run repogrammar doctor",
                options.include_unknowns,
            ),
        };
    }

    match runtime.repo_shape_diagnostics(request.clone()) {
        Ok(report) => {
            let query_metrics = family_query_metrics_rollup(request.clone()).unwrap_or_default();
            let unknown_inventory = if options.include_unknowns {
                match runtime.unknown_inventory(request) {
                    Ok(report) => Some(report),
                    Err(_) => {
                        return stats_fallback(
                            false,
                            "repository status is unavailable",
                            "run repogrammar doctor",
                            options.include_unknowns,
                        );
                    }
                }
            } else {
                None
            };
            CliOutput::success(stats_human(
                &report,
                &status_report.readiness,
                &query_metrics,
                unknown_inventory.as_ref(),
            ))
        }
        Err(_) => stats_fallback(
            false,
            "repository status is unavailable",
            "run repogrammar doctor",
            options.include_unknowns,
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
    include_unknowns: bool,
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
            "--unknowns" => {
                options.include_unknowns = true;
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

// Alias the authoritative caveat from the measurement model so the CLI's
// estimated-savings surfaces cannot drift a parallel literal from the value the
// metric and MCP carry.
const ESTIMATED_TOKEN_SAVING_CAVEAT: &str = EstimatedPotentialTokenSavings::CAVEAT;
const OFFICIAL_FAMILY_SCOPE: &str = "python_v0_1";
const REPO_SHAPE_SCOPE: &str = "python_family_eligible_units";
const PARTIAL_CONTEXT_RECOMMENDED_ACTION: &str =
    "use repogrammar find/check with exact repo-relative paths for PARTIAL_CONTEXT read plans";

fn stats_fallback(json: bool, reason: &str, guidance: &str, include_unknowns: bool) -> CliOutput {
    if json {
        let mut value = json!({
            "status": "FALLBACK_TO_CODE_SEARCH",
            "reason": reason,
            "guidance": guidance,
            "command": "stats",
            "schema_version": PRODUCT_SCHEMA_VERSION,
            "implemented": true,
            "official_family_scope": OFFICIAL_FAMILY_SCOPE,
            "repo_shape_scope": REPO_SHAPE_SCOPE,
            "token_saving_readiness": TokenSavingReadiness::Unknown.as_str(),
            "blocking_reasons": stats_fallback_blocking_reasons(reason),
            "measurement_kind": "ESTIMATED",
            "caveat": ESTIMATED_TOKEN_SAVING_CAVEAT,
            "readiness_available": false,
            "indexed_inventory": {
                "indexed_file_count": null,
                "indexed_code_unit_count": null,
                "semantic_fact_count": null,
            },
            "by_language": [],
        });
        if include_unknowns {
            value
                .as_object_mut()
                .expect("stats fallback JSON root object")
                .insert("inventory_available".to_string(), json!(false));
        }
        return CliOutput::failure(2, json_line(value));
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
    readiness: &RepositoryReadiness,
    query_metrics: &FamilyQueryMetricsRollup,
    unknown_inventory: Option<&UnknownInventoryReport>,
) -> String {
    // Lead with the essentials (~10 lines): readiness, indexed inventory,
    // family coverage, the all-scope estimated-savings headline, and the scope
    // note with its next action. Every remaining metric, risk signal, and rollup
    // stays available under `stats --json` (no JSON field is dropped).
    let mut output = format!(
        "stats: repo-shape diagnostics\nofficial_family_scope: {}\trepo_shape_scope: {}\nreadiness: {}\tquery_ready: {}\nindexed: {} files\t{} code units\t{} semantic facts\nfamilies: {}\teligible_code_units: {}\tfamily_support_coverage: {}\ntoken_saving_readiness: {}\n",
        OFFICIAL_FAMILY_SCOPE,
        REPO_SHAPE_SCOPE,
        readiness_state_value(readiness.state),
        readiness.query_ready,
        report.indexed_file_count,
        report.indexed_code_unit_count,
        report.semantic_fact_count,
        report.family_count,
        report.eligible_code_units,
        optional_ratio_human(report.family_support_coverage),
        report.token_saving_readiness.as_str(),
    );
    output.push_str(&stats_all_scope_savings_human(query_metrics));
    output.push_str(&stats_scope_human(report));
    output.push_str(
        "detail: run `repogrammar stats --json` for full metrics, risk signals, blocking reasons, and per-language and per-outcome-shape breakdowns\n",
    );
    if let Some(unknown_inventory) = unknown_inventory {
        output.push_str(&unknown_inventory_human(
            "stats_unknowns",
            unknown_inventory,
        ));
    }
    output
}

fn stats_json(
    report: &RepoShapeDiagnosticsReport,
    readiness: &RepositoryReadiness,
    measurement: Option<&crate::application::telemetry::ExperimentReport>,
    query_metrics: &FamilyQueryMetricsRollup,
    unknown_inventory: Option<&UnknownInventoryReport>,
) -> String {
    let estimated_rollup = &query_metrics.savings;
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
    let mut value = json!({
        "command": "stats",
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "status": "ok",
        "implemented": true,
        "official_family_scope": OFFICIAL_FAMILY_SCOPE,
        "repo_shape_scope": REPO_SHAPE_SCOPE,
        "active_generation": report.active_generation,
        "readiness_available": true,
        "readiness": stats_readiness_json(readiness),
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
        "indexed_inventory": {
            "indexed_file_count": report.indexed_file_count,
            "indexed_code_unit_count": report.indexed_code_unit_count,
            "semantic_fact_count": report.semantic_fact_count,
        },
        "by_language": stats_by_language_json(report, unknown_inventory),
        "scope_explanations": stats_scope_json(report),
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
        "all_scope_token_savings": stats_all_scope_savings_json(query_metrics),
        "query_outcome_rollup": query_outcome_rollup_value(query_metrics),
        "measurement_status": measurement_status,
        "measurement_reason": measurement.and_then(|measurement| measurement.reason.as_deref()),
        "claim_validity": measurement.map(|measurement| measurement.claim_validity.as_str()).unwrap_or("unknown"),
        "context_compression_ratio": null,
        "interpretation": report.interpretation,
        "claim": "diagnostic only; token saving depends on repeated repo-local patterns and is not measured token savings",
    });
    if let Some(unknown_inventory) = unknown_inventory {
        value
            .as_object_mut()
            .expect("stats JSON root object")
            .insert(
                "unknown_inventory".to_string(),
                unknown_inventory_value(unknown_inventory),
            );
    }
    json_line(value)
}

fn stats_readiness_json(readiness: &RepositoryReadiness) -> Value {
    json!({
        "state": readiness_state_value(readiness.state),
        "query_ready": readiness.query_ready,
        "active_generation_available": readiness.active_generation_available,
        "recommended_next_command": &readiness.recommended_next_command,
        "requires_user_permission": readiness.requires_user_permission,
        "autosync": {
            "configured": readiness.autosync.configured,
            "running": readiness.autosync.running,
            "recommended": readiness.autosync.recommended,
        },
    })
}

fn stats_scope_human(report: &RepoShapeDiagnosticsReport) -> String {
    let mut output = String::from(
        "python_family_eligible_units_note: eligible_code_units=0 can be expected for non-Python or unsupported-family repositories\n",
    );
    let Some(tsjs) = stats_tsjs_language(report) else {
        return output;
    };
    if !stats_tsjs_indexed_context_available(tsjs) {
        return output;
    }
    output.push_str(&format!(
        "tsjs_indexed_context_available: true\ntsjs_family_support: {}\nreact_rn_family_support: unsupported\nrecommended_next_action: {}\n",
        stats_tsjs_family_support(tsjs),
        PARTIAL_CONTEXT_RECOMMENDED_ACTION,
    ));
    output
}

fn stats_scope_json(report: &RepoShapeDiagnosticsReport) -> Value {
    let tsjs = stats_tsjs_language(report);
    let tsjs_indexed_context_available = tsjs
        .map(stats_tsjs_indexed_context_available)
        .unwrap_or(false);
    let tsjs_family_support = tsjs.map(stats_tsjs_family_support).unwrap_or("not_indexed");
    json!({
        "official_family_scope": OFFICIAL_FAMILY_SCOPE,
        "repo_shape_scope": REPO_SHAPE_SCOPE,
        "python_family_eligible_units_note": "eligible_code_units=0 can be expected for non-Python or unsupported-family repositories",
        "tsjs_indexed_context_available": tsjs_indexed_context_available,
        "tsjs_family_support": tsjs_family_support,
        "react_rn_family_support": "unsupported",
        "recommended_next_action": if tsjs_indexed_context_available {
            Some(PARTIAL_CONTEXT_RECOMMENDED_ACTION)
        } else {
            None
        },
    })
}

fn stats_tsjs_language(
    report: &RepoShapeDiagnosticsReport,
) -> Option<&RepoShapeLanguageDiagnostics> {
    report
        .by_language
        .iter()
        .find(|language| language.language == "typescript/javascript")
}

fn stats_tsjs_indexed_context_available(language: &RepoShapeLanguageDiagnostics) -> bool {
    language.indexed_code_unit_count > 0
}

fn stats_tsjs_family_support(language: &RepoShapeLanguageDiagnostics) -> &'static str {
    if language.indexed_code_unit_count == 0 {
        "not_indexed"
    } else if language.family_count == 0 {
        "none_or_unsupported"
    } else {
        "bounded_preview"
    }
}

/// The additive all-scope estimated-potential-token-savings block: totals plus
/// per-outcome-shape and per-language breakdowns, and the honest denominator
/// `savings_events / total_queries` (savings events over every recorded query).
/// Every value is ESTIMATED; the paired-experiment recorder remains the only
/// path to a MEASURED claim. This block covers all indexed languages and all
/// context-delivering outcome shapes; the `python_family_eligible_units`
/// repo-shape block is the official-scope subset.
fn stats_all_scope_savings_json(query_metrics: &FamilyQueryMetricsRollup) -> Value {
    let estimated_rollup = &query_metrics.savings;
    let query_outcome_rollup = &query_metrics.query_outcomes;
    json!({
        "metric_epoch": query_metrics.epoch,
        "epoch_started_unix_seconds": query_metrics.epoch_started_unix_seconds,
        "producer_version": query_metrics.producer_version,
        "cohort_status": "aligned_atomic_v2",
        "measurement_kind": estimated_rollup.measurement_kind.as_str(),
        "caveat": estimated_rollup.caveat,
        "scope": "all_languages_all_outcome_shapes",
        "savings_events": estimated_rollup.event_count,
        "total_queries": query_outcome_rollup.event_count,
        "estimated_baseline_tokens": estimated_rollup.total_estimated_baseline_tokens,
        "estimated_returned_tokens": estimated_rollup.total_estimated_returned_tokens,
        "estimated_potential_token_savings": estimated_rollup.total_estimated_potential_token_savings,
        "by_outcome_shape": savings_breakdown_map_json(&estimated_rollup.by_outcome_shape),
        "by_language": savings_breakdown_map_json(&estimated_rollup.by_language),
        "note": "one atomic v2 cohort across every recorded query and optional context-delivering savings event; legacy v1 rollups are historical unpaired evidence and are excluded; python_family_eligible_units remains the official-scope subset",
    })
}

/// The compact all-scope estimated-savings headline for the concise human
/// summary: the total (labeled ESTIMATED, never measured), the honest
/// `savings_events / total_queries` denominator, and the per-outcome-shape and
/// per-language breakdowns on one continuation line. The full block is in JSON.
fn stats_all_scope_savings_human(query_metrics: &FamilyQueryMetricsRollup) -> String {
    let estimated_rollup = &query_metrics.savings;
    let query_outcome_rollup = &query_metrics.query_outcomes;
    format!(
        "estimated_potential_token_savings: {}\tmeasurement_kind: {}\tsavings_events: {} / queries: {}\tmetric_epoch: {}\tcaveat: {}\nall_scope_by_outcome_shape: {}\tby_language: {}\n",
        estimated_rollup.total_estimated_potential_token_savings,
        estimated_rollup.measurement_kind.as_str(),
        estimated_rollup.event_count,
        query_outcome_rollup.event_count,
        query_metrics.epoch,
        estimated_rollup.caveat,
        savings_breakdown_map_human(&estimated_rollup.by_outcome_shape),
        savings_breakdown_map_human(&estimated_rollup.by_language),
    )
}

fn savings_breakdown_map_human(map: &BTreeMap<String, SavingsBreakdown>) -> String {
    if map.is_empty() {
        return "none".to_string();
    }
    map.iter()
        .map(|(key, breakdown)| {
            format!(
                "{key}=events:{},potential:{}",
                breakdown.event_count, breakdown.estimated_potential_token_savings
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn query_outcome_rollup_value(query_metrics: &FamilyQueryMetricsRollup) -> Value {
    let rollup = &query_metrics.query_outcomes;
    json!({
        "schema_version": crate::application::telemetry::FAMILY_QUERY_METRICS_SCHEMA_VERSION,
        "rollup_scope": "local_query_outcomes",
        "metric_epoch": query_metrics.epoch,
        "epoch_started_unix_seconds": query_metrics.epoch_started_unix_seconds,
        "producer_version": query_metrics.producer_version,
        "cohort_status": "aligned_atomic_v2",
        "event_count": rollup.event_count,
        "by_status": &rollup.by_status,
        "by_entrypoint": &rollup.by_entrypoint,
        "by_command_category": &rollup.by_command_category,
        "by_lookup_mode": &rollup.by_lookup_mode,
        "by_unknown_class": &rollup.by_unknown_class,
        "by_reason_code": &rollup.by_reason_code,
        "by_required_mechanism": &rollup.by_required_mechanism,
        "by_obligation": &rollup.by_obligation,
        "by_recovery_code": &rollup.by_recovery_code,
        "read_plan_returned_count": rollup.read_plan_returned_count,
        "read_plan_item_count_bucket": &rollup.read_plan_item_count_bucket,
        "source_spans_requested_count": rollup.source_spans_requested_count,
        "source_spans_included_count": rollup.source_spans_included_count,
        "source_span_omission_count_bucket": &rollup.source_span_omission_count_bucket,
    })
}

fn stats_by_language_json(
    report: &RepoShapeDiagnosticsReport,
    unknown_inventory: Option<&UnknownInventoryReport>,
) -> Vec<serde_json::Value> {
    report
        .by_language
        .iter()
        .map(|language| {
            let unknown_summary = unknown_inventory.and_then(|inventory| {
                inventory
                    .by_language_detail
                    .iter()
                    .find(|summary| summary.language == language.language)
            });
            json!({
                "language": language.language,
                "language_scope": language.language_scope,
                "indexed_file_count": language.indexed_file_count,
                "indexed_code_unit_count": language.indexed_code_unit_count,
                "eligible_code_units": language.eligible_code_units,
                "family_count": language.family_count,
                "family_member_count": language.family_member_count,
                "family_support_coverage": language.family_support_coverage,
                "blocking_unknowns": unknown_summary.map(|summary| summary.blocking_unknowns),
                "top_required_mechanisms": unknown_summary
                    .map(|summary| unknown_inventory_bucket_json(
                        "required_mechanism",
                        &summary.top_required_mechanisms,
                    ))
                    .unwrap_or_default(),
                "top_reason_codes": unknown_summary
                    .map(|summary| unknown_inventory_bucket_json(
                        "reason_code",
                        &summary.top_reason_codes,
                    ))
                    .unwrap_or_default(),
                "support_risk": language.support_risk.as_str(),
                "preview_status": language.preview_status,
                "unknown_inventory_available": unknown_inventory.is_some(),
            })
        })
        .collect()
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
    // An upload with no endpoint configured is an intended safe no-op (exit 0):
    // the public preview ships no endpoint, and the JSON report exposes
    // `uploaded`, `network_upload_configured`, and `reason` for scripts to
    // inspect. Only an explicitly disabled telemetry upload is treated as a
    // failure.
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
    // Validate required fields before prompting for consent, so a missing
    // `--experiment-mode` reports the accurate "--experiment-mode is required"
    // rather than the misleading "requires explicit confirmation" that the
    // consent prompt returns when no mode is set.
    let request = match experiment_start_request(options.clone()) {
        Ok(request) => request,
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
    if options.state_only && options.resync {
        return lifecycle_error(
            "init",
            options.json,
            RepoGrammarError::InvalidInput(
                "repogrammar init --state-only cannot be combined with --resync".to_string(),
            ),
        );
    }
    if options.state_only && options.autosync == Some(true) {
        return lifecycle_error(
            "init",
            options.json,
            RepoGrammarError::InvalidInput(
                "repogrammar init --state-only cannot be combined with --autosync".to_string(),
            ),
        );
    }
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
            if options.state_only {
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

            let index_request = match build_cli_index_request(options, current_dir, env_lookup) {
                Ok(request) => request,
                Err(error) => return lifecycle_error("init", options.json, error),
            };
            let resync_outcome = match runtime.index_repository("resync", index_request) {
                Ok(outcome) => {
                    status = runtime
                        .repository_status(RepositoryStatusRequest {
                            path: repository_root.clone(),
                            state_dir_override: state_dir_override.clone(),
                        })
                        .ok();
                    Some(outcome)
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
                        "fix the indexing issue, then run repogrammar resync",
                    );
                }
            };

            let mut autosync_report = None;
            if options.autosync.unwrap_or(true) {
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
    render_index_progress_event("init", &event)
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

    // The decomposed readiness report is independent of the classic status view;
    // runtimes that do not implement it (deferred/test runtimes) leave it absent.
    let readiness = runtime.product_readiness(request.clone()).ok();
    match runtime.repository_status(request) {
        Ok(report) if options.json => CliOutput::success(status_json(&report, readiness.as_ref())),
        Ok(report) => CliOutput::success(status_human(&report, readiness.as_ref())),
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
    let readiness = runtime
        .product_readiness(RepositoryStatusRequest {
            path: request.path.clone(),
            state_dir_override: request.state_dir_override.clone(),
        })
        .ok();

    match runtime.repository_doctor(request) {
        Ok(report) if options.json => {
            CliOutput::success(doctor_json(&report, env_lookup, readiness.as_ref()))
        }
        Ok(report) => CliOutput::success(doctor_human(&report, readiness.as_ref())),
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

fn handle_compact<F>(
    options: &CompactOptions,
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
    let compact_request = IndexCompactRequest {
        dry_run: options.dry_run,
    };

    match runtime.compact_storage(request, compact_request) {
        Ok(report) if options.json => CliOutput::success(compact_report_json(&report)),
        Ok(report) => CliOutput::success(compact_report_human(&report)),
        Err(error) => lifecycle_error("compact", options.json, error),
    }
}

fn handle_storage<F>(
    options: &StorageOptions,
    current_dir: &Path,
    env_lookup: &F,
    runtime: &impl CliRuntime,
) -> CliOutput
where
    F: Fn(&str) -> Option<String>,
{
    match options.command {
        StorageCommand::Clean => handle_storage_clean(options, current_dir, env_lookup, runtime),
    }
}

fn handle_storage_clean<F>(
    options: &StorageOptions,
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
    let clean_request = StorageCleanRequest {
        dry_run: options.dry_run,
    };

    match runtime.clean_storage(request, clean_request) {
        Ok(report) if options.json => CliOutput::success(storage_clean_report_json(&report)),
        Ok(report) => CliOutput::success(storage_clean_report_human(&report)),
        Err(error) => lifecycle_error("storage clean", options.json, error),
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
    /// Optional caller-named comparison family scope (`--against`), meaningful only
    /// for `explain` / `check`, where it pins the comparison side to one family.
    against: Option<String>,
    json: bool,
    evidence_mode: FamilyEvidenceMode,
    mode_explicit: bool,
    token_budget: Option<usize>,
    include_variations: bool,
    include_exceptions: bool,
    include_source_spans: bool,
    verbosity: Verbosity,
    all: bool,
}

impl QueryOptions {
    fn output_options(&self) -> FamilyOutputOptions {
        FamilyOutputOptions {
            evidence_mode: self.evidence_mode,
            token_budget: self.token_budget,
            include_variations: self.include_variations,
            include_exceptions: self.include_exceptions,
            verbosity: self.verbosity,
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
struct SetupCliOptions {
    project_path: Option<String>,
    target: SetupTarget,
    yes: bool,
    dry_run: bool,
    autosync: bool,
    json: bool,
    progress: ProgressMode,
}

impl Default for SetupCliOptions {
    fn default() -> Self {
        Self {
            project_path: None,
            target: SetupTarget::Auto,
            yes: false,
            dry_run: false,
            autosync: true,
            json: false,
            progress: ProgressMode::Auto,
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
    state_only: bool,
    resync: bool,
    autosync: Option<bool>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompactOptions {
    project_path: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
    quiet: bool,
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageCommand {
    Clean,
}

impl StorageCommand {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "clean" => Ok(Self::Clean),
            _ => Err("storage subcommand must be clean".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StorageOptions {
    command: StorageCommand,
    project_path: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
    quiet: bool,
    verbose: bool,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            command: StorageCommand::Clean,
            project_path: None,
            dry_run: false,
            yes: false,
            json: false,
            quiet: false,
            verbose: false,
        }
    }
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
            state_only: false,
            resync: false,
            autosync: None,
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

fn parse_setup_options(rest: &[String]) -> Result<SetupCliOptions, String> {
    let mut options = SetupCliOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" => {
                let value = option_value(rest, index, "--project", "a project path")?;
                set_project_path(&mut options.project_path, value)?;
                index += 2;
            }
            "--target" => {
                let value = option_value(rest, index, "--target", "auto, codex, or claude-code")?;
                options.target = match value {
                    "auto" => SetupTarget::Auto,
                    "codex" => SetupTarget::Codex,
                    "claude-code" | "claude" => SetupTarget::ClaudeCode,
                    _ => {
                        return Err("--target requires auto, codex, or claude-code".to_string());
                    }
                };
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
            "--no-autosync" => {
                options.autosync = false;
                index += 1;
            }
            "--json" => {
                options.json = true;
                index += 1;
            }
            "--progress" => {
                let value = option_value(rest, index, "--progress", "auto, always, or never")?;
                options.progress = ProgressMode::parse(value)?;
                index += 2;
            }
            other => return Err(format!("unknown setup option: {other}")),
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
            "--state-only" if command == "init" => {
                options.state_only = true;
                index += 1;
            }
            "--resync" if command == "init" => {
                options.resync = true;
                index += 1;
            }
            "--autosync" if command == "init" => {
                set_init_autosync_preference(&mut options.autosync, true)?;
                index += 1;
            }
            "--no-autosync" if command == "init" => {
                set_init_autosync_preference(&mut options.autosync, false)?;
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

fn set_init_autosync_preference(
    preference: &mut Option<bool>,
    enabled: bool,
) -> Result<(), String> {
    if preference.is_some_and(|current| current != enabled) {
        return Err("--autosync and --no-autosync cannot be combined".to_string());
    }
    *preference = Some(enabled);
    Ok(())
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

fn parse_compact_options(rest: &[String]) -> Result<CompactOptions, String> {
    let mut options = CompactOptions::default();
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
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
            other => return Err(format!("unknown compact option: {other}")),
        }
    }
    if !options.dry_run && !options.yes {
        return Err("compact requires --yes unless --dry-run is present".to_string());
    }
    Ok(options)
}

fn parse_storage_options(rest: &[String]) -> Result<StorageOptions, String> {
    let mut options = StorageOptions::default();
    let mut index = if let Some(first) = rest.first().filter(|value| !value.starts_with('-')) {
        options.command = StorageCommand::parse(first)?;
        1
    } else {
        return Err("storage subcommand must be clean".to_string());
    };
    while index < rest.len() {
        match rest[index].as_str() {
            "--project" | "--path" => {
                let value = option_value(rest, index, rest[index].as_str(), "a project path")?;
                set_project_path(&mut options.project_path, value)?;
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
            other => return Err(format!("unknown storage option: {other}")),
        }
    }
    if !options.dry_run && !options.yes {
        return Err("storage clean requires --yes unless --dry-run is present".to_string());
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

/// The `files`/`units` inventory commands document only `--project`/`--path`
/// and `--json`, but they share `parse_query_options`, which also accepts
/// `--mode`, `--token-budget`, `--include-*`, and a positional target and then
/// silently ignores them for these two commands. Reject the inapplicable
/// options explicitly so the accepted surface matches the documented one.
fn inventory_flag_rejection(command: &str, rest: &[String]) -> Option<String> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => index += 1,
            // `parse_query_options` already validated that a value follows.
            "--project" | "--path" => index += 2,
            other if other.starts_with('-') => {
                return Some(format!("{command} does not accept {other}"));
            }
            _ => {
                return Some(format!("{command} does not accept a positional argument"));
            }
        }
    }
    None
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
            "--against" => {
                let value = option_value(rest, index, "--against", "a comparison family scope")?;
                validate_query_target(value).map_err(|error| format!("--against {error}"))?;
                options.against = Some(value.to_string());
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
            "--verbosity" => {
                let value = option_value(rest, index, "--verbosity", "minimal, standard, or full")?;
                options.verbosity = Verbosity::parse(value)
                    .ok_or_else(|| "--verbosity requires minimal, standard, or full".to_string())?;
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
            "--all" => {
                options.all = true;
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
                // Use option_value so a following flag (e.g. `--target --dry-run`)
                // is reported as a missing value instead of being consumed.
                let value = option_value(rest, index, "--target", "a value")?;
                apply_install_target_value(&mut request, value)?;
                index += 2;
            }
            "--scope" | "--location" => {
                let value = option_value(rest, index, rest[index].as_str(), "global or project")?;
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
        "{command}: {}\nactive_generation: {}\ndiscovered_files: {}\nstored_files: {}\nskipped_paths: {}\nparser_attempted_files: {}\nindexed_units: {}\nsemantic_facts: {}\nindexing: {}\nparser: {}\nsemantic_worker: {}\nmining: deferred\nprogress: {}\n",
        outcome.indexing_mode.human_summary(),
        outcome.active_generation.as_deref().unwrap_or("none"),
        outcome.discovered_files,
        outcome.discovered_files,
        outcome.skipped_paths,
        outcome.parser_attempted_files,
        outcome.indexed_units,
        outcome.semantic_facts,
        outcome.indexing_mode.as_str(),
        outcome.indexing_mode.parser_status(),
        outcome.semantic_worker.as_str(),
        options.progress.as_str()
    );
    for warning in &outcome.warnings {
        output.push_str("warning: ");
        output.push_str(warning);
        output.push('\n');
    }
    if let Some(sync_report) = &outcome.sync_report {
        let (families_added, families_removed) = match &sync_report.family_identity_delta {
            Some(delta) => (
                delta.added_count.to_string(),
                delta.removed_count.to_string(),
            ),
            None => ("none".to_string(), "none".to_string()),
        };
        output.push_str(&format!(
            "sync_mode: {}\nfallback_reason: {}\nbase_generation: {}\nadded_files: {}\nmodified_files: {}\nremoved_files: {}\nunchanged_files: {}\ncopied_forward_files: {}\nreparsed_files: {}\nfamilies_recomputed: {}\ndirty_records_cleared: {}\nfamilies_added: {}\nfamilies_removed: {}\n",
            sync_report.sync_mode.as_str(),
            sync_report.fallback_reason.as_deref().unwrap_or("none"),
            sync_report.base_generation.as_deref().unwrap_or("none"),
            sync_report.added_files,
            sync_report.modified_files,
            sync_report.removed_files,
            sync_report.unchanged_files,
            sync_report.copied_forward_files,
            sync_report.reparsed_files,
            sync_report.families_recomputed,
            sync_report.dirty_records_cleared,
            families_added,
            families_removed,
        ));
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
    let mut value = json!({
        "command": command,
        "status": "complete",
        "generation_id": outcome.active_generation,
        "active_generation": outcome.active_generation,
        "discovered_files": outcome.discovered_files,
        "stored_files": outcome.discovered_files,
        "skipped_paths": outcome.skipped_paths,
        "parser_attempted_files": outcome.parser_attempted_files,
        "indexed_units": outcome.indexed_units,
        "semantic_facts": outcome.semantic_facts,
        "indexing": outcome.indexing_mode.as_str(),
        "parser": outcome.indexing_mode.parser_status(),
        "semantic_worker": outcome.semantic_worker.as_str(),
        "mining": "deferred",
        "progress": options.progress.as_str(),
        "warnings": outcome.warnings,
    });
    if let (Some(sync_report), Some(object)) = (&outcome.sync_report, value.as_object_mut()) {
        object.insert(
            "sync_mode".to_string(),
            json!(sync_report.sync_mode.as_str()),
        );
        object.insert(
            "fallback_reason".to_string(),
            sync_report
                .fallback_reason
                .as_ref()
                .map(|reason| json!(reason))
                .unwrap_or(Value::Null),
        );
        object.insert(
            "base_generation".to_string(),
            sync_report
                .base_generation
                .as_ref()
                .map(|generation| json!(generation))
                .unwrap_or(Value::Null),
        );
        object.insert("added_files".to_string(), json!(sync_report.added_files));
        object.insert(
            "modified_files".to_string(),
            json!(sync_report.modified_files),
        );
        object.insert(
            "removed_files".to_string(),
            json!(sync_report.removed_files),
        );
        object.insert(
            "unchanged_files".to_string(),
            json!(sync_report.unchanged_files),
        );
        object.insert(
            "copied_forward_files".to_string(),
            json!(sync_report.copied_forward_files),
        );
        object.insert(
            "reparsed_files".to_string(),
            json!(sync_report.reparsed_files),
        );
        object.insert(
            "families_recomputed".to_string(),
            json!(sync_report.families_recomputed),
        );
        object.insert(
            "dirty_records_cleared".to_string(),
            json!(sync_report.dirty_records_cleared),
        );
        let (families_added, families_removed) = match &sync_report.family_identity_delta {
            Some(delta) => (
                json!({ "count": delta.added_count, "sample": delta.added_sample }),
                json!({ "count": delta.removed_count, "sample": delta.removed_sample }),
            ),
            None => (Value::Null, Value::Null),
        };
        object.insert("families_added".to_string(), families_added);
        object.insert("families_removed".to_string(), families_removed);
    }
    value
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

fn compact_report_human(report: &IndexCompactReport) -> String {
    let status = if report.dry_run {
        "dry_run"
    } else {
        "complete"
    };
    format!(
        "compact: {status}\nactive_generation: {}\ndatabase_bytes_before: {}\nwal_bytes_before: {}\nshm_bytes_before: {}\ntotal_bytes_before: {}\ndatabase_bytes_after: {}\nwal_bytes_after: {}\nshm_bytes_after: {}\ntotal_bytes_after: {}\nreclaimed_bytes: {}\n",
        report.active_generation,
        report.before.database_bytes,
        report.before.wal_bytes,
        report.before.shm_bytes,
        report.before.total_bytes,
        report.after.database_bytes,
        report.after.wal_bytes,
        report.after.shm_bytes,
        report.after.total_bytes,
        compact_reclaimed_bytes(report)
    )
}

fn compact_report_json(report: &IndexCompactReport) -> String {
    json_line(json!({
        "command": "compact",
        "status": if report.dry_run { "dry_run" } else { "complete" },
        "active_generation": report.active_generation,
        "dry_run": report.dry_run,
        "before": storage_size_report_json(&report.before),
        "after": storage_size_report_json(&report.after),
        "reclaimed_bytes": compact_reclaimed_bytes(report),
    }))
}

fn storage_clean_report_human(report: &StorageCleanReport) -> String {
    let status = if report.dry_run {
        "dry_run"
    } else {
        "complete"
    };
    let mut output = format!(
        "storage clean: {status}\nactive_generation: {}\ndry_run: {}\nlegacy_layout_present_before: {}\nlegacy_layout_present_after: {}\nlegacy_layout_removed: {}\nlegacy_layout_bytes_before: {}\nlegacy_layout_bytes_after: {}\nlegacy_layout_reclaimable_bytes: {}\nprune_keep_inactive: {}\nprune_candidate_generations: {}\nprune_deleted_generations: {}\ncompact_reclaimed_bytes: {}\ntotal_bytes_before: {}\ntotal_bytes_after: {}\nreclaimed_bytes: {}\n",
        report.active_generation,
        report.dry_run,
        report.legacy_layout.present_before,
        report.legacy_layout.present_after,
        report.legacy_layout.removed,
        report.legacy_layout.bytes_before,
        report.legacy_layout.bytes_after,
        storage_clean_legacy_reclaimable_bytes(report),
        report.prune.keep_inactive,
        report.prune.candidate_generations.len(),
        report.prune.deleted_generations.len(),
        compact_reclaimed_bytes(&report.compact),
        report.total_bytes_before,
        report.total_bytes_after,
        storage_clean_reclaimed_bytes(report)
    );
    for generation in &report.prune.candidate_generations {
        output.push_str(if report.dry_run {
            "would_prune_generation: "
        } else {
            "prune_candidate_generation: "
        });
        output.push_str(generation);
        output.push('\n');
    }
    for generation in &report.prune.deleted_generations {
        output.push_str("pruned_generation: ");
        output.push_str(generation);
        output.push('\n');
    }
    output
}

fn storage_clean_report_json(report: &StorageCleanReport) -> String {
    json_line(json!({
        "command": "storage clean",
        "status": if report.dry_run { "dry_run" } else { "complete" },
        "active_generation": report.active_generation,
        "dry_run": report.dry_run,
        "legacy_layout": {
            "present_before": report.legacy_layout.present_before,
            "present_after": report.legacy_layout.present_after,
            "removed": report.legacy_layout.removed,
            "bytes_before": report.legacy_layout.bytes_before,
            "bytes_after": report.legacy_layout.bytes_after,
            "reclaimable_bytes": storage_clean_legacy_reclaimable_bytes(report),
        },
        "prune": {
            "keep_inactive": report.prune.keep_inactive,
            "retained_inactive_generations": &report.prune.retained_inactive_generations,
            "candidate_generations": &report.prune.candidate_generations,
            "deleted_generations": &report.prune.deleted_generations,
            "dry_run": report.prune.dry_run,
        },
        "compact": {
            "dry_run": report.compact.dry_run,
            "before": storage_size_report_json(&report.compact.before),
            "after": storage_size_report_json(&report.compact.after),
            "reclaimed_bytes": compact_reclaimed_bytes(&report.compact),
        },
        "total_bytes_before": report.total_bytes_before,
        "total_bytes_after": report.total_bytes_after,
        "reclaimed_bytes": storage_clean_reclaimed_bytes(report),
    }))
}

fn storage_size_report_json(report: &IndexStorageSizeReport) -> Value {
    json!({
        "database_bytes": report.database_bytes,
        "wal_bytes": report.wal_bytes,
        "shm_bytes": report.shm_bytes,
        "total_bytes": report.total_bytes,
    })
}

fn compact_reclaimed_bytes(report: &IndexCompactReport) -> u64 {
    report
        .before
        .total_bytes
        .saturating_sub(report.after.total_bytes)
}

fn storage_clean_reclaimed_bytes(report: &StorageCleanReport) -> u64 {
    report
        .total_bytes_before
        .saturating_sub(report.total_bytes_after)
}

fn storage_clean_legacy_reclaimable_bytes(report: &StorageCleanReport) -> u64 {
    report
        .legacy_layout
        .bytes_before
        .saturating_sub(report.legacy_layout.bytes_after)
}

fn status_human(
    report: &RepositoryStatusReport,
    readiness: Option<&ProductReadinessReport>,
) -> String {
    let mut output = String::new();
    if let Some(readiness) = readiness {
        output.push_str(&readiness_human_lead(readiness));
    }
    let storage_inspection = report.storage_inspection.as_ref();
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
        storage_inspection
            .and_then(|inspection| inspection.schema_version)
            .map(|version| version.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    output.push_str(&format!(
        "storage_layout: {}\n",
        storage_inspection
            .map(|inspection| storage_layout(inspection.layout))
            .unwrap_or("none")
    ));
    output.push_str(&format!(
        "mutable_database_present: {}\n",
        optional_human_bool(
            storage_inspection.map(|inspection| inspection.mutable_database_present)
        )
    ));
    output.push_str(&format!(
        "legacy_generation_layout_present: {}\n",
        optional_human_bool(
            storage_inspection.map(|inspection| inspection.legacy_generation_layout_present)
        )
    ));
    output.push_str(&format!(
        "wal_bytes: {}\n",
        optional_human_u64(storage_inspection.and_then(|inspection| inspection.wal_bytes))
    ));
    output.push_str(&format!(
        "shm_bytes: {}\n",
        optional_human_u64(storage_inspection.and_then(|inspection| inspection.shm_bytes))
    ));
    output.push_str(&format!(
        "journal_mode: {}\n",
        storage_inspection
            .and_then(|inspection| inspection.journal_mode.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "integrity_check: {}\n",
        storage_inspection
            .and_then(|inspection| inspection.integrity_check.as_deref())
            .unwrap_or("not_implemented")
    ));
    output.push_str(&format!(
        "dependency_records: {}\n",
        storage_inspection
            .and_then(|inspection| inspection.dependency_record_count)
            .map(|count| count.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    output.push_str(&format!(
        "dirty_records: {}\n",
        storage_inspection
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
        "readiness: {}\n",
        readiness_state_value(report.readiness.state)
    ));
    output.push_str(&format!("query_ready: {}\n", report.readiness.query_ready));
    if let Some(command) = &report.readiness.recommended_next_command {
        output.push_str("recommended_next_command: ");
        output.push_str(command);
        output.push('\n');
    }
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

fn status_json(
    report: &RepositoryStatusReport,
    readiness: Option<&ProductReadinessReport>,
) -> String {
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
        "schema_version": PRODUCT_SCHEMA_VERSION,
        "initialized": matches!(report.status, RepositoryStatus::Initialized { .. }),
        "state_dir": report.state_dir,
        "status": repository_status_value(&report.status),
        "manifest": manifest_status(report.manifest),
        "active_generation": active_generation,
        "manifest_schema_version": report.manifest_schema_version,
        "storage_schema_version": storage_inspection.and_then(|inspection| inspection.schema_version),
        "storage_layout": storage_inspection.map(|inspection| storage_layout(inspection.layout)),
        "mutable_database_present": storage_inspection.map(|inspection| inspection.mutable_database_present),
        "legacy_generation_layout_present": storage_inspection.map(|inspection| inspection.legacy_generation_layout_present),
        "wal_bytes": storage_inspection.and_then(|inspection| inspection.wal_bytes),
        "shm_bytes": storage_inspection.and_then(|inspection| inspection.shm_bytes),
        "journal_mode": storage_inspection.and_then(|inspection| inspection.journal_mode.as_deref()),
        "integrity_check": storage_inspection.and_then(|inspection| inspection.integrity_check.as_deref()),
        "foreign_keys_enabled": storage_inspection.and_then(|inspection| inspection.foreign_keys_enabled),
        "dependency_records": storage_inspection.and_then(|inspection| inspection.dependency_record_count),
        "dirty_records": storage_inspection.and_then(|inspection| inspection.dirty_record_count),
        "storage": implementation_status(report.storage),
        "indexing": implementation_status(report.indexing),
        "storage_error": report.storage_error,
        "missing_subdirs": report.missing_subdirs,
        "readiness": readiness_json(&report.readiness),
        "product_readiness": readiness.map(product_readiness_value),
    }))
}

fn doctor_human(
    report: &RepositoryDoctorReport,
    readiness: Option<&ProductReadinessReport>,
) -> String {
    let mut output = String::from("doctor: repository lifecycle diagnostics\n");
    if let Some(readiness) = readiness {
        output.push_str(&readiness_human_lead(readiness));
    }
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

fn doctor_json<F>(
    report: &RepositoryDoctorReport,
    env_lookup: &F,
    readiness: Option<&ProductReadinessReport>,
) -> String
where
    F: Fn(&str) -> Option<String>,
{
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
        "schema_version": PRODUCT_SCHEMA_VERSION,
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
            "storage_layout": storage_inspection.map(|inspection| storage_layout(inspection.layout)),
            "mutable_database_present": storage_inspection.map(|inspection| inspection.mutable_database_present),
            "legacy_generation_layout_present": storage_inspection.map(|inspection| inspection.legacy_generation_layout_present),
            "wal_bytes": storage_inspection.and_then(|inspection| inspection.wal_bytes),
            "shm_bytes": storage_inspection.and_then(|inspection| inspection.shm_bytes),
            "journal_mode": storage_inspection.and_then(|inspection| inspection.journal_mode.as_deref()),
            "integrity_check": storage_inspection.and_then(|inspection| inspection.integrity_check.as_deref()),
            "dependency_records": storage_inspection.and_then(|inspection| inspection.dependency_record_count),
            "dirty_records": storage_inspection.and_then(|inspection| inspection.dirty_record_count),
        },
        "readiness": readiness_json(&report.status.readiness),
        "product_readiness": readiness.map(product_readiness_value),
        "findings": findings,
        "optional_providers": optional_providers_json(env_lookup),
    }))
}

/// Source-free snapshot of the optional semantic provider slots and their current
/// availability. Missing providers are optional accelerators, never doctor
/// failures, so this is reported alongside `checks` rather than inside it.
fn optional_providers_json<F>(env_lookup: &F) -> Vec<serde_json::Value>
where
    F: Fn(&str) -> Option<String>,
{
    crate::application::providers::optional_provider_report(
        |key| env_lookup(key),
        |binary| crate::application::providers::binary_available_on_path(binary, env_lookup),
    )
    .into_iter()
    .map(|status| {
        json!({
            "id": status.slot.id(),
            "language": status.slot.language(),
            "integrated": status.slot.is_integrated(),
            "availability": status.availability.as_protocol_str(),
            "resolves_mechanisms": status.slot.resolves_mechanisms(),
        })
    })
    .collect()
}

/// Human-readable leading block for `status`/`doctor`: the actionable capability
/// summary and the one canonical next action from the shared recovery classifier.
/// Family-evidence counts are rendered as facts only; the single command shown is
/// the classifier's `next_action`. Callers must not infer a second command from
/// raw freshness counts, so no per-count command hint is printed. Internal
/// mechanism ids stay in the JSON as follow-up handles rather than crowding the
/// human lead.
fn readiness_human_lead(readiness: &ProductReadinessReport) -> String {
    let mut lead = String::new();
    lead.push_str(&format!("capability: {}\n", readiness.summary.as_str()));
    lead.push_str(&format!(
        "next_action: {}\n",
        recovery_guidance(readiness.recovery.action)
    ));
    if readiness.family_evidence.stale_count > 0 {
        lead.push_str(&format!(
            "stale_family_evidence: {}\n",
            readiness.family_evidence.stale_count
        ));
    }
    if readiness.family_evidence.cannot_verify_count > 0 {
        lead.push_str(&format!(
            "unverifiable_family_evidence: {}\n",
            readiness.family_evidence.cannot_verify_count
        ));
    }
    lead
}

fn readiness_json(readiness: &RepositoryReadiness) -> Value {
    let foreign_provider_state = readiness
        .local_state_hygiene
        .foreign_provider_state
        .iter()
        .map(|provider| {
            json!({
                "name": &provider.name,
                "path": &provider.path,
                "present": provider.present,
                "managed_by_repogrammar": provider.managed_by_repogrammar,
                "tracked_risk": provider.tracked_risk,
                "recommendation": &provider.recommendation,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "state": readiness_state_value(readiness.state),
        "query_ready": readiness.query_ready,
        "active_generation_available": readiness.active_generation_available,
        "recommended_next_command": &readiness.recommended_next_command,
        "requires_user_permission": readiness.requires_user_permission,
        "autosync": {
            "configured": readiness.autosync.configured,
            "running": readiness.autosync.running,
            "recommended": readiness.autosync.recommended,
        },
        "local_state_hygiene": {
            "repogrammar_state_present": readiness.local_state_hygiene.repogrammar_state_present,
            "repogrammar_state_ignored": readiness.local_state_hygiene.repogrammar_state_ignored,
            "repogrammar_state_tracked_risk": readiness.local_state_hygiene.repogrammar_state_tracked_risk,
            "repogrammar_recommendation": &readiness.local_state_hygiene.repogrammar_recommendation,
            "foreign_provider_state": foreign_provider_state,
        },
    })
}

fn readiness_state_value(state: RepositoryReadinessState) -> &'static str {
    match state {
        RepositoryReadinessState::NotInitialized => "not_initialized",
        RepositoryReadinessState::StateOnlyNoActiveIndex => "state_only_no_active_index",
        RepositoryReadinessState::ReadyActiveIndex => "ready_active_index",
        RepositoryReadinessState::ActiveIndexUnhealthy => "active_index_unhealthy",
        RepositoryReadinessState::ActiveIndexStaleOrUnreadable => {
            "active_index_stale_or_unreadable"
        }
        RepositoryReadinessState::AutosyncRecommended => "autosync_recommended",
        RepositoryReadinessState::AutosyncActive => "autosync_active",
        RepositoryReadinessState::StorageUnhealthy => "storage_unhealthy",
        RepositoryReadinessState::Unknown => "unknown",
    }
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
                | RepositoryDoctorCode::IndexLockLegacy
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
        "startup_state: {}\ndaemon_state: {}\nrepository_ready: {}\npoll_ms: {}\ndebounce_ms: {}\n",
        report.startup.state.as_str(),
        report.daemon_state.as_str(),
        report.repository_ready,
        report.poll_ms,
        report.debounce_ms
    ));
    if let Some(code) = report.startup.failure_code {
        output.push_str(&format!("startup_failure_code: {}\n", code.as_str()));
    }
    if let Some(code) = report.startup.previous_failure_code {
        output.push_str(&format!(
            "previous_startup_failure_code: {}\n",
            code.as_str()
        ));
    }
    if let Some(run) = &report.last_run {
        output.push_str(&format!(
            "previous_autosync_attempt_unix_seconds: {}\nprevious_autosync_attempt_result: {}\n",
            run.last_sync_unix_seconds,
            run.result.as_str()
        ));
        if let Some(generation) = run.display_synced_generation() {
            output.push_str(&format!(
                "previous_autosync_attempt_generation: {generation}\n"
            ));
        }
        if let Some(error) = run.display_error() {
            output.push_str(&format!("previous_autosync_attempt_error: {error}\n"));
        }
    }
    output
}

fn autosync_json(command: AutosyncCommand, report: &AutosyncReport) -> String {
    json_line(autosync_value(command, report))
}

fn autosync_value(command: AutosyncCommand, report: &AutosyncReport) -> serde_json::Value {
    let previous_autosync_attempt = report.last_run.as_ref().map(|run| {
        json!({
            "unix_seconds": run.last_sync_unix_seconds,
            "result": run.result.as_str(),
            "synced_generation": run.display_synced_generation(),
            "error": run.display_error(),
        })
    });
    json!({
        "command": "autosync",
        "subcommand": command.as_str(),
        "status": "complete",
        "state_dir": report.state_dir,
        "enabled": report.enabled,
        "running": report.running,
        "startup_state": report.startup.state.as_str(),
        "startup_failure_code": report.startup.failure_code.map(|code| code.as_str()),
        "previous_startup_failure_code": report.startup.previous_failure_code.map(|code| code.as_str()),
        "daemon_state": report.daemon_state.as_str(),
        "repository_ready": report.repository_ready,
        "pid": report.pid,
        "poll_ms": report.poll_ms,
        "debounce_ms": report.debounce_ms,
        "last_run": report.last_run.as_ref().map(|run| json!({
            "last_sync_unix_seconds": run.last_sync_unix_seconds,
            "result": run.result.as_str(),
            "synced_generation": run.display_synced_generation(),
            "error": run.display_error(),
        })),
        "previous_autosync_attempt": previous_autosync_attempt,
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

fn storage_layout(layout: IndexStorageLayout) -> &'static str {
    match layout {
        IndexStorageLayout::Empty => "empty",
        IndexStorageLayout::Mutable => "mutable",
        IndexStorageLayout::Legacy => "legacy",
        IndexStorageLayout::MutableWithLegacy => "mutable_with_legacy",
    }
}

fn optional_human_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn optional_human_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
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
        RepositoryDoctorCode::IndexLockLegacy => "INDEX_LOCK_LEGACY",
        RepositoryDoctorCode::IndexLockInvalid => "INDEX_LOCK_INVALID",
        RepositoryDoctorCode::StorageNotImplemented => "STORAGE_NOT_IMPLEMENTED",
        RepositoryDoctorCode::StorageReady => "STORAGE_READY",
        RepositoryDoctorCode::StorageInvalid => "STORAGE_INVALID",
        RepositoryDoctorCode::StorageNoActiveGeneration => "STORAGE_NO_ACTIVE_GENERATION",
        RepositoryDoctorCode::StorageLegacyLayout => "STORAGE_LEGACY_LAYOUT",
        RepositoryDoctorCode::StorageMixedLayout => "STORAGE_MIXED_LAYOUT",
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
                "schema_version": PRODUCT_SCHEMA_VERSION,
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

pub fn render_index_progress_event(command: &str, event: &ProgressEvent) -> String {
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
        index_repository_with_discovery_parser_frameworks_and_store,
        sync_repository_with_discovery_parser_frameworks_and_store, IndexingRequest,
    };
    use crate::application::query::TermRetrievalAbstention;
    use crate::application::query::{
        list_code_units, list_families, list_indexed_files,
        lookup_family_with_freshness_and_local_context, lookup_family_with_local_context,
        FamilyFreshness, FamilySummary,
    };
    use crate::application::query_terms::MatchedSignals;
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

    fn resolved_unit_target() -> ResolvedQueryTarget {
        ResolvedQueryTarget {
            original_target: "src/api/routes.py get_users".to_string(),
            kind: "code_unit",
            path: "src/api/routes.py".to_string(),
            line: Some(12),
            byte_range: Some((0, 40)),
            family_id: Some("family:python:fastapi_route:framework_fastapi_route".to_string()),
            code_unit_id: Some("unit:src/api/routes.py#fastapi_route:get:0-40:1".to_string()),
            symbol_hints: vec!["get_users".to_string()],
            residue_terms: vec!["get".to_string(), "users".to_string()],
            candidate_paths: vec!["src/api/routes.py".to_string()],
            candidate_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            candidate_code_unit_ids: vec![
                "unit:src/api/routes.py#fastapi_route:get:0-40:1".to_string()
            ],
            confidence: "high",
            match_kind: "exact_path",
        }
    }

    /// Golden byte-parity: the CLI `resolved_target` mirror reproduces the
    /// pre-precision shape byte-for-byte at `standard` and `full` (v1 discipline).
    #[test]
    fn resolved_target_json_standard_and_full_are_byte_stable() {
        let target = resolved_unit_target();
        let standard = resolved_target_json(&target, Verbosity::Standard);
        let full = resolved_target_json(&target, Verbosity::Full);
        let golden = json!({
            "original_target": "src/api/routes.py get_users",
            "kind": "code_unit",
            "path": "src/api/routes.py",
            "line": 12,
            "byte_range": {"start": 0, "end": 40},
            "family_id": "family:python:fastapi_route:framework_fastapi_route",
            "code_unit_id": "unit:src/api/routes.py#fastapi_route:get:0-40:1",
            "symbol_hints": ["get_users"],
            "residue_terms": ["get", "users"],
            "candidate_paths": ["src/api/routes.py"],
            "candidate_family_ids": ["family:python:fastapi_route:framework_fastapi_route"],
            "candidate_code_unit_ids": ["unit:src/api/routes.py#fastapi_route:get:0-40:1"],
            "confidence": "high",
            "match_kind": "exact_path",
        });
        assert_eq!(
            serde_json::to_string(&standard).unwrap(),
            serde_json::to_string(&golden).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&full).unwrap(),
            serde_json::to_string(&golden).unwrap()
        );
    }

    /// CLI mirror of the MCP `minimal` thinning: input echo, normalizer internals,
    /// and redundant `candidate_*` echoes drop only at `minimal`.
    #[test]
    fn resolved_target_json_minimal_thins_echo_and_redundant_candidates() {
        let target = resolved_unit_target();
        let minimal = resolved_target_json(&target, Verbosity::Minimal);
        let object = minimal.as_object().expect("resolved_target object");
        for dropped in [
            "original_target",
            "residue_terms",
            "candidate_code_unit_ids",
            "candidate_paths",
            "candidate_family_ids",
        ] {
            assert!(!object.contains_key(dropped), "minimal must drop {dropped}");
        }
        for kept in ["kind", "path", "code_unit_id", "family_id", "confidence"] {
            assert!(object.contains_key(kept), "minimal must keep {kept}");
        }
    }

    /// CLI mirror: an ambiguous resolution keeps the `candidate_*` recovery handles
    /// at `minimal`.
    #[test]
    fn resolved_target_json_minimal_retains_candidates_when_ambiguous() {
        let mut target = resolved_unit_target();
        target.code_unit_id = None;
        target.family_id = None;
        target.path = String::new();
        target.candidate_code_unit_ids = vec!["unit:a".to_string(), "unit:b".to_string()];
        target.candidate_paths = vec!["a.py".to_string(), "b.py".to_string()];
        target.candidate_family_ids = vec!["family:a".to_string(), "family:b".to_string()];
        let minimal = resolved_target_json(&target, Verbosity::Minimal);
        let object = minimal.as_object().expect("resolved_target object");
        assert!(!object.contains_key("original_target"));
        assert!(object.contains_key("candidate_code_unit_ids"));
        assert!(object.contains_key("candidate_paths"));
        assert!(object.contains_key("candidate_family_ids"));
    }

    fn deviation_computation(count: usize) -> AlignmentComputation {
        use crate::application::conformance::{LegalVariation, StaticDeviation};
        use crate::core::policy::alignment::{AlignmentStatus, StaticDeviationKind};
        AlignmentComputation {
            status: AlignmentStatus::StaticDeviation,
            required_features_matched: Vec::new(),
            static_deviations: (0..count)
                .map(|index| StaticDeviation {
                    prefix: format!("prefix_{index}"),
                    kind: StaticDeviationKind::RequiredMismatch,
                    semantics_token: "equal".to_string(),
                    expected_summary: "expected".to_string(),
                    observed_summary: "observed".to_string(),
                })
                .collect(),
            legal_observed_variations: (0..count)
                .map(|index| LegalVariation {
                    dimension: format!("dimension_{index}"),
                    observed_profile: "profile".to_string(),
                })
                .collect(),
            blocking_unknowns: Vec::new(),
            unresolved_runtime_obligations: Vec::new(),
            outcome_reason: "a required feature deviated".to_string(),
        }
    }

    /// CLI mirror of the S8 deviation cap: over-cap arrays truncate to the cap and
    /// carry honest `<name>_truncated`/`<name>_count` siblings.
    #[test]
    fn alignment_computation_json_caps_deviation_arrays_with_honest_flags() {
        let total = ALIGNMENT_DEVIATION_CAP + 3;
        let value = alignment_computation_json(&deviation_computation(total));
        assert_eq!(
            value["static_deviations"].as_array().unwrap().len(),
            ALIGNMENT_DEVIATION_CAP
        );
        assert_eq!(value["static_deviations_truncated"], json!(true));
        assert_eq!(value["static_deviations_count"], json!(total));
        assert_eq!(
            value["legal_observed_variations"].as_array().unwrap().len(),
            ALIGNMENT_DEVIATION_CAP
        );
        assert_eq!(value["legal_observed_variations_truncated"], json!(true));
        assert_eq!(value["legal_observed_variations_count"], json!(total));
    }

    /// Below the cap the CLI mirror emits no truncation metadata (v1 additivity).
    #[test]
    fn alignment_computation_json_below_cap_emits_no_truncation_metadata() {
        let value = alignment_computation_json(&deviation_computation(2));
        let object = value.as_object().expect("computation object");
        assert_eq!(object["static_deviations"].as_array().unwrap().len(), 2);
        for absent in [
            "static_deviations_truncated",
            "static_deviations_count",
            "legal_observed_variations_truncated",
            "legal_observed_variations_count",
        ] {
            assert!(!object.contains_key(absent), "{absent} must be absent");
        }
    }

    fn empty_conformance_read_plan() -> ReadPlan {
        ReadPlan {
            items: Vec::new(),
            estimated_tokens: 0,
            source_snippets_included: false,
            requires_source_before_edit: false,
            selection_strategy: "greedy_marginal_coverage_v1",
            budget_satisfied: true,
            line_range_omissions: Vec::new(),
            truncated: false,
        }
    }

    fn member_certificate() -> AlignmentCertificateReport {
        use crate::core::policy::alignment::{AlignmentStatus, TargetRelationship};
        AlignmentCertificateReport {
            active_generation: "gen-000001".to_string(),
            alignment_status: AlignmentStatus::StaticallyAligned,
            target_relationship: Some(TargetRelationship::Member),
            selected_family_id: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            candidate_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            resolved_target: resolved_unit_target(),
            computation: None,
            read_plan: empty_conformance_read_plan(),
            unknowns: Vec::new(),
            resolved_target_file_size_bytes: None,
            resolved_target_language: String::new(),
            family_evidence_baseline_tokens: 0,
        }
    }

    fn member_route() -> FamilyQueryRouteReport {
        FamilyQueryRouteReport {
            route: "conformance_member",
            input_kind: "path_symbol_role_or_pattern_target",
            pipeline: vec!["resolve_target", "select_family", "align"],
            family_id_policy:
                "family_ids_are_returned_follow_up_handles_not_required_initial_inputs",
            candidate_limit: Some(5),
            selected_family_id: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            candidate_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            follow_up_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            why_selected: "resolved unit is a member of its family",
            term_retrieval: None,
        }
    }

    /// CLI mirror of the S8 dedup + `runtime_equivalence` invariant.
    #[test]
    fn alignment_certificate_json_dedups_duplicates_only_at_minimal() {
        let certificate = member_certificate();
        let route = member_route();
        let render = |verbosity| {
            alignment_certificate_json("check", &certificate, &route, None, verbosity, None)
        };

        let standard = render(Verbosity::Standard);
        let full = render(Verbosity::Full);
        assert_eq!(standard["status"], "STATICALLY_ALIGNED");
        assert_eq!(standard["alignment_status"], "STATICALLY_ALIGNED");
        assert_eq!(
            standard["selected_family_id"],
            "family:python:fastapi_route:framework_fastapi_route"
        );
        assert_eq!(standard["runtime_equivalence"], "UNKNOWN");
        assert_eq!(
            serde_json::to_string(&standard).unwrap(),
            serde_json::to_string(&full).unwrap()
        );

        let minimal = render(Verbosity::Minimal);
        let object = minimal.as_object().expect("certificate object");
        assert!(!object.contains_key("alignment_status"));
        assert_eq!(object["status"], "STATICALLY_ALIGNED");
        assert_eq!(
            object["runtime_equivalence"], "UNKNOWN",
            "runtime_equivalence is an invariant and is never removed"
        );
        // The top-level `selected_family_id` is retained at every tier as the
        // authoritative selected-family handle (the route lane suppresses the
        // `query_route.selected_family_id` copy at `minimal`).
        assert_eq!(
            object["selected_family_id"],
            "family:python:fastapi_route:framework_fastapi_route"
        );
    }

    #[test]
    fn constraint_profile_json_is_metadata_only_and_null_when_absent() {
        assert_eq!(family_constraint_profile_json(None), Value::Null);
        let profile = crate::test_support::sample_family_constraint_profile();
        let value = family_constraint_profile_json(Some(&profile));
        // Required-equal features carry typed origin/semantics tokens, never source.
        let required = value["required_equal_features"]
            .as_array()
            .expect("required_equal_features");
        assert!(required.iter().any(|constraint| {
            constraint["origin"] == "framework_role_identity" && constraint["semantics"] == "equal"
        }));
        // The observed-only variation dimension is surfaced verbatim.
        assert_eq!(
            value["allowed_variations"][0]["dimension"],
            "python_import_context"
        );
        assert_eq!(value["allowed_variations"][0]["observed_only"], true);
        // The prohibited-presence blocker and the runtime obligation are present.
        assert_eq!(
            value["prohibited_or_blocking_features"][0]["semantics"],
            "prohibited_presence"
        );
        assert_eq!(
            value["unresolved_obligations"][0]["affected_claim"],
            "family:example:runtime_equivalence"
        );
        // The serializer echoes the profile's typed fields verbatim and adds no
        // others; the profile model itself carries only RepoGrammar-owned tokens
        // (source-freedom is enforced by the storage-side hydration validators, not
        // re-litigated here). Assert the emitted object has exactly the four
        // documented keys.
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
    fn index_progress_renderer_uses_bar_counts_without_machine_payloads() {
        let event = ProgressEvent::new(
            crate::application::progress::ProgressStage::FileScanning,
            "stored files",
            WorkUnits::known(2, 4).expect("valid work units"),
        );

        let human = render_index_progress_event("index", &event);
        assert!(human.contains("index: [##########----------] 50% 2/4 file_scanning"));
        assert!(!human.to_ascii_lowercase().contains("eta"));
        assert!(serde_json::from_str::<Value>(human.trim()).is_err());
    }

    #[test]
    fn unknown_progress_renderer_remains_indeterminate_without_percentages() {
        let event = ProgressEvent::new(
            crate::application::progress::ProgressStage::SemanticResolution,
            "waiting for worker",
            WorkUnits::Unknown,
        );

        let human = render_index_progress_event("sync", &event);
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

    struct SetupIntegrationRuntime {
        fail_index: bool,
        fail_autosync: bool,
        fail_mcp: bool,
        fail_rollback: bool,
        force_stale: bool,
        fail_families: bool,
        inspection: AgentIntegrationInspection,
        claude_inspection: Option<AgentIntegrationInspection>,
        fail_probe: bool,
        calls: RefCell<Vec<&'static str>>,
    }

    impl SetupIntegrationRuntime {
        fn new(fail_index: bool) -> Self {
            Self {
                fail_index,
                fail_autosync: false,
                fail_mcp: false,
                fail_rollback: false,
                force_stale: false,
                fail_families: false,
                inspection: AgentIntegrationInspection::Unmanaged,
                claude_inspection: None,
                fail_probe: false,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn failing(stage: SetupStage) -> Self {
            Self {
                fail_index: stage == SetupStage::RepositoryIndex,
                fail_autosync: stage == SetupStage::Autosync,
                fail_mcp: stage == SetupStage::McpSelfTest,
                fail_rollback: false,
                force_stale: false,
                fail_families: false,
                inspection: AgentIntegrationInspection::Unmanaged,
                claude_inspection: None,
                fail_probe: false,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn with_rollback_failure(mut self) -> Self {
            self.fail_rollback = true;
            self
        }

        fn with_stale_status(mut self) -> Self {
            self.force_stale = true;
            self
        }

        fn with_family_inventory_failure(mut self) -> Self {
            self.fail_families = true;
            self
        }

        fn with_inspection(mut self, inspection: AgentIntegrationInspection) -> Self {
            self.inspection = inspection;
            self
        }

        fn with_target_inspections(
            mut self,
            codex: AgentIntegrationInspection,
            claude_code: AgentIntegrationInspection,
        ) -> Self {
            self.inspection = codex;
            self.claude_inspection = Some(claude_code);
            self
        }

        fn with_probe_failure(mut self) -> Self {
            self.fail_probe = true;
            self
        }
    }

    struct SetupConfirmationPrompt {
        confirmations: Cell<usize>,
        response: &'static str,
    }

    impl InstallTelemetryPrompt for SetupConfirmationPrompt {
        fn is_interactive(&self) -> bool {
            true
        }

        fn prompt_setup_confirmation(&self, prompt: &str) -> Result<String, String> {
            assert!(prompt.contains("Setup plan:"));
            assert_eq!(prompt.matches("Proceed with setup?").count(), 1);
            self.confirmations.set(self.confirmations.get() + 1);
            Ok(self.response.to_string())
        }
    }

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
                indexing_mode:
                    crate::application::indexing::IndexingGenerationMode::SyntaxOnlyCodeUnits,
                parser_attempted_files: 1,
                indexed_units: 1,
                semantic_facts: 2,
                discovered_files: 1,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Complete,
                sync_report: None,
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
                indexing_mode:
                    crate::application::indexing::IndexingGenerationMode::SyntaxOnlyCodeUnits,
                parser_attempted_files: 1,
                indexed_units: 1,
                semantic_facts: 0,
                discovered_files: 1,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                sync_report: None,
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
                indexing_mode:
                    crate::application::indexing::IndexingGenerationMode::FileManifestOnly,
                parser_attempted_files: 0,
                indexed_units: 0,
                semantic_facts: 0,
                discovered_files: 0,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                sync_report: None,
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
                indexing_mode:
                    crate::application::indexing::IndexingGenerationMode::FileManifestOnly,
                parser_attempted_files: 0,
                indexed_units: 0,
                semantic_facts: 0,
                discovered_files: 0,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                sync_report: None,
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

    #[derive(Default)]
    struct CompactRuntime {
        last_request: RefCell<Option<RepositoryStatusRequest>>,
        last_compact: RefCell<Option<IndexCompactRequest>>,
    }

    impl CliRuntime for CompactRuntime {
        fn index_repository(
            &self,
            _command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            unreachable!("compact command should not index")
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("compact command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("compact command should not request doctor through CLI test runtime")
        }

        fn compact_storage(
            &self,
            request: RepositoryStatusRequest,
            compact: IndexCompactRequest,
        ) -> Result<IndexCompactReport, RepoGrammarError> {
            self.last_request.replace(Some(request));
            self.last_compact.replace(Some(compact));
            let before = IndexStorageSizeReport {
                database_bytes: 128,
                wal_bytes: 32,
                shm_bytes: 16,
                total_bytes: 176,
            };
            let after = if compact.dry_run {
                before.clone()
            } else {
                IndexStorageSizeReport {
                    database_bytes: 96,
                    wal_bytes: 0,
                    shm_bytes: 16,
                    total_bytes: 112,
                }
            };
            Ok(IndexCompactReport {
                active_generation: "gen-000004".to_string(),
                dry_run: compact.dry_run,
                before,
                after,
            })
        }
    }

    #[derive(Default)]
    struct StorageCleanRuntime {
        last_request: RefCell<Option<RepositoryStatusRequest>>,
        last_clean: RefCell<Option<StorageCleanRequest>>,
    }

    impl CliRuntime for StorageCleanRuntime {
        fn index_repository(
            &self,
            _command: &str,
            _request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            unreachable!("storage clean command should not index")
        }

        fn repository_status(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            unreachable!("storage clean command should not request status through CLI test runtime")
        }

        fn repository_doctor(
            &self,
            _request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            unreachable!("storage clean command should not request doctor through CLI test runtime")
        }

        fn clean_storage(
            &self,
            request: RepositoryStatusRequest,
            clean: StorageCleanRequest,
        ) -> Result<StorageCleanReport, RepoGrammarError> {
            self.last_request.replace(Some(request));
            self.last_clean.replace(Some(clean));
            let compact_before = IndexStorageSizeReport {
                database_bytes: 128,
                wal_bytes: 32,
                shm_bytes: 16,
                total_bytes: 176,
            };
            let compact_after = if clean.dry_run {
                compact_before.clone()
            } else {
                IndexStorageSizeReport {
                    database_bytes: 96,
                    wal_bytes: 0,
                    shm_bytes: 16,
                    total_bytes: 112,
                }
            };
            Ok(StorageCleanReport {
                active_generation: "gen-000004".to_string(),
                dry_run: clean.dry_run,
                legacy_layout: LegacyLayoutCleanupReport {
                    present_before: true,
                    present_after: clean.dry_run,
                    removed: !clean.dry_run,
                    bytes_before: 1024,
                    bytes_after: if clean.dry_run { 1024 } else { 0 },
                },
                prune: GenerationPruneReport {
                    active_generation: "gen-000004".to_string(),
                    keep_inactive: 0,
                    retained_inactive_generations: Vec::new(),
                    candidate_generations: vec!["gen-000001".to_string(), "gen-000002".to_string()],
                    deleted_generations: if clean.dry_run {
                        Vec::new()
                    } else {
                        vec!["gen-000001".to_string(), "gen-000002".to_string()]
                    },
                    dry_run: clean.dry_run,
                },
                compact: IndexCompactReport {
                    active_generation: "gen-000004".to_string(),
                    dry_run: clean.dry_run,
                    before: compact_before,
                    after: compact_after,
                },
                total_bytes_before: 1200,
                total_bytes_after: if clean.dry_run { 1200 } else { 112 },
            })
        }
    }

    struct FamilyQueryRuntime;

    impl FamilyQueryRuntime {
        fn status_report() -> RepositoryStatusReport {
            let readiness = RepositoryReadiness {
                // Preserve the legacy fixture's status-presentation contract;
                // the explicit recovery decision below is what declares the
                // mocked active index safe for query preflight.
                state: RepositoryReadinessState::Unknown,
                query_ready: false,
                active_generation_available: true,
                recovery: Some(RecoveryRecommendation {
                    action: RecoveryAction::None,
                    reason: RecoveryReason::Ready,
                }),
                ..RepositoryReadiness::default()
            };
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
                readiness,
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
                prevalence: crate::test_support::sample_family_prevalence(),
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
                constraint_profile: None,
                term_retrieval: None,
                resolution: None,
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

        fn unknown_inventory_report() -> UnknownInventoryReport {
            UnknownInventoryReport {
                inventory_scope: crate::application::query::UNKNOWN_INVENTORY_SCOPE,
                active_generation: "gen-000001".to_string(),
                total_unknowns: 2,
                blocking_unknowns: 1,
                non_blocking_unknowns: 1,
                recoverable_unknowns: 0,
                irreducible_unknowns: 0,
                by_language: vec![UnknownInventoryBucket {
                    key: "python".to_string(),
                    count: 2,
                }],
                by_language_detail: vec![UnknownInventoryLanguageSummary {
                    language: "python".to_string(),
                    total_unknowns: 2,
                    blocking_unknowns: 1,
                    top_required_mechanisms: vec![
                        UnknownInventoryBucket {
                            key: "fastapi_dependency_graph".to_string(),
                            count: 1,
                        },
                        UnknownInventoryBucket {
                            key: "python_import_graph".to_string(),
                            count: 1,
                        },
                    ],
                    top_reason_codes: vec![
                        UnknownInventoryBucket {
                            key: "RuntimeDependencyInjection".to_string(),
                            count: 1,
                        },
                        UnknownInventoryBucket {
                            key: "UnresolvedImport".to_string(),
                            count: 1,
                        },
                    ],
                }],
                by_reason_code: vec![
                    UnknownInventoryBucket {
                        key: "RuntimeDependencyInjection".to_string(),
                        count: 1,
                    },
                    UnknownInventoryBucket {
                        key: "UnresolvedImport".to_string(),
                        count: 1,
                    },
                ],
                by_required_mechanism: vec![
                    UnknownInventoryBucket {
                        key: "fastapi_dependency_graph".to_string(),
                        count: 1,
                    },
                    UnknownInventoryBucket {
                        key: "python_import_graph".to_string(),
                        count: 1,
                    },
                ],
                by_obligation: vec![
                    UnknownInventoryBucket {
                        key: "framework_identity".to_string(),
                        count: 1,
                    },
                    UnknownInventoryBucket {
                        key: "symbol_binding".to_string(),
                        count: 1,
                    },
                ],
                by_framework_role: vec![UnknownInventoryBucket {
                    key: "framework:fastapi.route".to_string(),
                    count: 2,
                }],
                by_role_state: vec![UnknownInventoryBucket {
                    key: "single".to_string(),
                    count: 2,
                }],
                by_blocks_support: vec![
                    crate::application::query::UnknownInventoryBlocksSupportBucket {
                        blocks_support: false,
                        count: 1,
                    },
                    crate::application::query::UnknownInventoryBlocksSupportBucket {
                        blocks_support: true,
                        count: 1,
                    },
                ],
                by_recovery_code: vec![
                    // Both mechanisms (fastapi_dependency_graph, python_import_graph)
                    // belong to the registered-but-not-integrated python provider
                    // slot, so both recover via not_implemented_in_current_version.
                    UnknownInventoryBucket {
                        key: "not_implemented_in_current_version".to_string(),
                        count: 2,
                    },
                ],
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
                    prevalence: crate::test_support::sample_family_prevalence(),
                    freshness: None,
                }],
                freshness_counts: None,
                unknowns: Vec::new(),
            })
        }

        fn family_lookup(
            &self,
            _request: RepositoryStatusRequest,
            target: Option<&str>,
            _against: Option<&str>,
            mode: FamilyLookupMode,
        ) -> Result<FamilyLookupReport, RepoGrammarError> {
            let matched = match mode {
                FamilyLookupMode::FuzzyQuery | FamilyLookupMode::Conformance => {
                    target == Some("src/routes/a.ts")
                }
                FamilyLookupMode::ExactFamilyId => {
                    target == Some("family:typescript:express_route:express")
                }
                FamilyLookupMode::ExactMemberId => {
                    target == Some("unit:src/routes/a.ts#express_route:get:0-20:1")
                }
            };
            if matched {
                Ok(FamilyLookupReport::Found(Box::new(Self::detail())))
            } else {
                Ok(FamilyLookupReport::Unknown(FamilyUnknownReport {
                    active_generation: "gen-000001".to_string(),
                    candidate_family_ids: Vec::new(),
                    unknowns: vec![FamilyQueryUnknown {
                        class: crate::core::model::UnknownClass::Blocking,
                        reason: crate::core::model::UnknownReasonCode::InsufficientSupport,
                        affected_claim: "query target".to_string(),
                        recovery: Some(
                            "run repogrammar resync after adding compatible implementations"
                                .to_string(),
                        ),
                    }],
                    term_retrieval: None,
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
                indexed_file_count: 4,
                indexed_code_unit_count: 4,
                semantic_fact_count: 0,
                eligible_code_units: 4,
                family_count: 1,
                family_member_count: 3,
                covered_code_units: 3,
                by_language: vec![
                    RepoShapeLanguageDiagnostics {
                        language: "python".to_string(),
                        language_scope: "official_v0_1",
                        indexed_file_count: 4,
                        indexed_code_unit_count: 4,
                        eligible_code_units: 4,
                        family_count: 1,
                        family_member_count: 3,
                        covered_code_units: 3,
                        family_support_coverage: Some(0.75),
                        support_risk: DiagnosticSignal::Low,
                        preview_status: "official",
                    },
                    RepoShapeLanguageDiagnostics {
                        language: "typescript/javascript".to_string(),
                        language_scope: "bounded_v0_2_preview",
                        indexed_file_count: 0,
                        indexed_code_unit_count: 0,
                        eligible_code_units: 0,
                        family_count: 0,
                        family_member_count: 0,
                        covered_code_units: 0,
                        family_support_coverage: None,
                        support_risk: DiagnosticSignal::Unknown,
                        preview_status: "bounded_preview",
                    },
                    RepoShapeLanguageDiagnostics {
                        language: "rust".to_string(),
                        language_scope: "internal_self_dogfood_preview",
                        indexed_file_count: 0,
                        indexed_code_unit_count: 0,
                        eligible_code_units: 0,
                        family_count: 0,
                        family_member_count: 0,
                        covered_code_units: 0,
                        family_support_coverage: None,
                        support_risk: DiagnosticSignal::Unknown,
                        preview_status: "internal_preview",
                    },
                    RepoShapeLanguageDiagnostics {
                        language: "java".to_string(),
                        language_scope: "bounded_v0_2_preview",
                        indexed_file_count: 0,
                        indexed_code_unit_count: 0,
                        eligible_code_units: 0,
                        family_count: 0,
                        family_member_count: 0,
                        covered_code_units: 0,
                        family_support_coverage: None,
                        support_risk: DiagnosticSignal::Unknown,
                        preview_status: "bounded_preview",
                    },
                    RepoShapeLanguageDiagnostics {
                        language: "csharp".to_string(),
                        language_scope: "bounded_v0_2_preview",
                        indexed_file_count: 0,
                        indexed_code_unit_count: 0,
                        eligible_code_units: 0,
                        family_count: 0,
                        family_member_count: 0,
                        covered_code_units: 0,
                        family_support_coverage: None,
                        support_risk: DiagnosticSignal::Unknown,
                        preview_status: "bounded_preview",
                    },
                    RepoShapeLanguageDiagnostics {
                        language: "c/cpp".to_string(),
                        language_scope: "bounded_v0_2_preview",
                        indexed_file_count: 0,
                        indexed_code_unit_count: 0,
                        eligible_code_units: 0,
                        family_count: 0,
                        family_member_count: 0,
                        covered_code_units: 0,
                        family_support_coverage: None,
                        support_risk: DiagnosticSignal::Unknown,
                        preview_status: "bounded_preview",
                    },
                ],
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

        fn unknown_inventory(
            &self,
            _request: RepositoryStatusRequest,
        ) -> Result<UnknownInventoryReport, RepoGrammarError> {
            Ok(Self::unknown_inventory_report())
        }
    }

    #[derive(Default)]
    struct BootstrapRuntime {
        index_calls: Cell<usize>,
        autosync_calls: Cell<usize>,
        indexed: Cell<bool>,
        active_before_index: bool,
        fail_index: bool,
        fail_index_with_resource_limit: bool,
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

        fn fail_index() -> Self {
            Self {
                fail_index: true,
                ..Self::default()
            }
        }

        fn fail_autosync() -> Self {
            Self {
                fail_autosync: true,
                ..Self::default()
            }
        }

        fn fail_index_with_resource_limit() -> Self {
            Self {
                fail_index_with_resource_limit: true,
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
            if self.fail_index_with_resource_limit {
                return Err(RepoGrammarError::InvalidInput(
                    "filesystem discovery resource limit exceeded: resource=visited_entries, limit=1, observed=2; narrow the repository scope or exclude generated, dependency, build, and cache content"
                        .to_string(),
                ));
            }
            self.indexed.set(true);
            Ok(IndexingOutcome {
                indexing_mode:
                    crate::application::indexing::IndexingGenerationMode::SyntaxOnlyCodeUnits,
                parser_attempted_files: 2,
                indexed_units: 3,
                semantic_facts: 0,
                discovered_files: 2,
                skipped_paths: 0,
                active_generation: Some("gen-000001".to_string()),
                semantic_worker: crate::application::indexing::SemanticWorkerRunStatus::Deferred,
                sync_report: None,
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
                readiness: RepositoryReadiness::default(),
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
            assert!(
                self.indexed.get(),
                "init must finish resync before starting autosync"
            );
            if self.fail_autosync {
                return Err(RepoGrammarError::InvalidInput(
                    "synthetic autosync failure".to_string(),
                ));
            }
            Ok(AutosyncReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                enabled: true,
                running: true,
                daemon_state: crate::application::autosync::AutosyncDaemonState::Running,
                pid: Some(1234),
                poll_ms: request.poll_ms,
                debounce_ms: request.debounce_ms,
                last_run: None,
                startup: crate::application::autosync::AutosyncStartupReport {
                    state: crate::application::autosync::AutosyncStartupState::Ready,
                    failure_code: None,
                    previous_failure_code: None,
                },
                repository_ready: true,
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
                daemon_state: if matches!(command, AutosyncCommand::Start | AutosyncCommand::Run) {
                    crate::application::autosync::AutosyncDaemonState::Running
                } else {
                    crate::application::autosync::AutosyncDaemonState::Stopped
                },
                pid: matches!(command, AutosyncCommand::Start | AutosyncCommand::Run)
                    .then_some(1234),
                poll_ms: request.poll_ms,
                debounce_ms: request.debounce_ms,
                last_run: None,
                startup: crate::application::autosync::AutosyncStartupReport {
                    state: if matches!(command, AutosyncCommand::Start | AutosyncCommand::Run) {
                        crate::application::autosync::AutosyncStartupState::Ready
                    } else {
                        crate::application::autosync::AutosyncStartupState::Idle
                    },
                    failure_code: None,
                    previous_failure_code: None,
                },
                repository_ready: true,
                message: format!("autosync {} ok", command.as_str()),
            })
        }
    }

    impl CliRuntime for TestRuntime {
        fn index_repository(
            &self,
            command: &str,
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
            let indexing_request = IndexingRequest {
                repository_root: request.repository_root,
                state_dir_override: request.state_dir_override,
                max_file_bytes: request.max_file_bytes,
                strict_gitignore: request.strict_gitignore,
            };
            if command == "sync" {
                return sync_repository_with_discovery_parser_frameworks_and_store(
                    indexing_request,
                    &FilesystemFileDiscovery,
                    &FilesystemSourceStore,
                    &SyntaxCodeUnitParser,
                    &framework_roles,
                    &store,
                );
            }
            index_repository_with_discovery_parser_frameworks_and_store(
                indexing_request,
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

        fn inspect_agent_integration(
            &self,
            target: AgentTarget,
            scope: InstallScope,
            context: &InstallExecutionContext,
        ) -> Result<AgentIntegrationInspection, RepoGrammarError> {
            Ok(if owned_install_receipt_exists(context, target, scope)? {
                AgentIntegrationInspection::OwnedCurrent
            } else {
                AgentIntegrationInspection::Unmanaged
            })
        }

        fn family_lookup(
            &self,
            request: RepositoryStatusRequest,
            target: Option<&str>,
            against: Option<&str>,
            mode: FamilyLookupMode,
        ) -> Result<FamilyLookupReport, RepoGrammarError> {
            let store = self.store_for_status_request(&request)?;
            // The conformance/deviation flow needs the freshness gate and a source
            // store, exactly like production; other modes keep the source-store-free
            // path.
            if mode == FamilyLookupMode::Conformance {
                return lookup_family_with_freshness_and_local_context(
                    crate::application::query::FamilyEvidenceFreshnessRequest {
                        repository_root: request.path.clone(),
                        max_file_bytes: crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
                    },
                    &store,
                    &store,
                    &FilesystemSourceStore,
                    target,
                    against,
                    mode,
                );
            }
            lookup_family_with_local_context(&store, &store, target, mode)
        }

        fn autosync(
            &self,
            command: AutosyncCommand,
            request: CliAutosyncRequest,
        ) -> Result<AutosyncReport, RepoGrammarError> {
            Ok(AutosyncReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                enabled: true,
                running: command == AutosyncCommand::Start,
                daemon_state: if command == AutosyncCommand::Start {
                    crate::application::autosync::AutosyncDaemonState::Running
                } else {
                    crate::application::autosync::AutosyncDaemonState::Stopped
                },
                pid: (command == AutosyncCommand::Start).then_some(1234),
                poll_ms: request.poll_ms,
                debounce_ms: request.debounce_ms,
                last_run: None,
                startup: crate::application::autosync::AutosyncStartupReport {
                    state: crate::application::autosync::AutosyncStartupState::Ready,
                    failure_code: None,
                    previous_failure_code: None,
                },
                repository_ready: true,
                message: "setup autosync test".to_string(),
            })
        }

        fn mcp_self_test(&self, _project: &str) -> Result<(), SetupFailureClass> {
            Ok(())
        }
    }

    impl CliRuntime for SetupIntegrationRuntime {
        fn index_repository(
            &self,
            command: &str,
            request: CliIndexRequest,
        ) -> Result<IndexingOutcome, RepoGrammarError> {
            self.calls.borrow_mut().push("index");
            if self.fail_index {
                return Err(RepoGrammarError::InvalidInput(
                    "injected setup index failure".to_string(),
                ));
            }
            TestRuntime.index_repository(command, request)
        }

        fn repository_status(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<RepositoryStatusReport, RepoGrammarError> {
            let mut status = TestRuntime.repository_status(request)?;
            if self.force_stale {
                if let Some(inspection) = status.storage_inspection.as_mut() {
                    inspection.dirty_record_count = Some(1);
                }
            }
            Ok(status)
        }

        fn repository_doctor(
            &self,
            request: RepositoryDoctorRequest,
        ) -> Result<RepositoryDoctorReport, RepoGrammarError> {
            TestRuntime.repository_doctor(request)
        }

        fn families(
            &self,
            request: RepositoryStatusRequest,
        ) -> Result<FamilyListReport, RepoGrammarError> {
            if self.fail_families {
                return Err(RepoGrammarError::InvalidInput(
                    "synthetic family inventory failure with /private/secret".to_string(),
                ));
            }
            TestRuntime.families(request)
        }

        fn inspect_agent_integration(
            &self,
            target: AgentTarget,
            _scope: InstallScope,
            _context: &InstallExecutionContext,
        ) -> Result<AgentIntegrationInspection, RepoGrammarError> {
            if self.fail_probe {
                Err(RepoGrammarError::InvalidInput(
                    "native MCP probe failed".to_string(),
                ))
            } else {
                Ok(if target == AgentTarget::ClaudeCode {
                    self.claude_inspection.unwrap_or(self.inspection)
                } else {
                    self.inspection
                })
            }
        }

        fn install_agent_integration(
            &self,
            command: &str,
            request: InstallRequest,
            _context: InstallExecutionContext,
        ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
            self.calls.borrow_mut().push(match command {
                "install" => "install",
                "uninstall" => "uninstall",
                _ => {
                    return Err(RepoGrammarError::InvalidInput(
                        "invalid setup action".into(),
                    ))
                }
            });
            assert!(!request.telemetry_enabled);
            assert!(!request.telemetry_explicitly_configured);
            if command == "uninstall" && self.fail_rollback {
                return Err(RepoGrammarError::InvalidInput(
                    "injected rollback failure with /private/secret".to_string(),
                ));
            }
            let (configured_targets, reconfigured_targets) = if command == "install"
                && self.inspection == AgentIntegrationInspection::OwnedOutdated
            {
                (Vec::new(), request.selected_targets)
            } else {
                (request.selected_targets, Vec::new())
            };
            Ok(InstallExecutionOutcome {
                command: if command == "install" {
                    "install"
                } else {
                    "uninstall"
                },
                target: request.target,
                scope: request.scope,
                configured_targets,
                reconfigured_targets,
                skipped_targets: Vec::new(),
                receipt_paths: Vec::new(),
                installed_executable_path: None,
                command_path: None,
                command_on_path: true,
                message: "fake native integration complete".to_string(),
            })
        }

        fn mcp_self_test(&self, _project: &str) -> Result<(), SetupFailureClass> {
            self.calls.borrow_mut().push("mcp");
            if self.fail_mcp {
                Err(SetupFailureClass::McpSelfTestFailed)
            } else {
                Ok(())
            }
        }

        fn autosync(
            &self,
            _command: AutosyncCommand,
            _request: CliAutosyncRequest,
        ) -> Result<AutosyncReport, RepoGrammarError> {
            self.calls.borrow_mut().push("autosync");
            if self.fail_autosync {
                return Err(RepoGrammarError::InvalidInput(
                    "injected autosync failure with /private/secret".to_string(),
                ));
            }
            Ok(AutosyncReport {
                state_dir: DEFAULT_STATE_DIR.to_string(),
                enabled: true,
                running: true,
                daemon_state: crate::application::autosync::AutosyncDaemonState::Running,
                pid: Some(1234),
                poll_ms: AutosyncSettings::default().poll_ms,
                debounce_ms: AutosyncSettings::default().debounce_ms,
                last_run: None,
                startup: crate::application::autosync::AutosyncStartupReport {
                    state: crate::application::autosync::AutosyncStartupState::Ready,
                    failure_code: None,
                    previous_failure_code: None,
                },
                repository_ready: true,
                message: "setup autosync test".to_string(),
            })
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
    fn top_level_help_is_compact_and_points_to_the_core_journey() {
        let output = run(["--help"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output.stdout.lines().count() <= 25);
        assert!(output
            .stdout
            .contains("Usage: repogrammar <command> [options]"));
        assert!(output.stdout.contains("repogrammar help <command>"));
        assert!(output.stdout.contains("repogrammar setup"));
        assert!(output.stdout.contains("find"));
        assert!(output.stdout.contains("doctor"));
        assert!(output.stdout.contains("repogrammar help --all"));
        assert!(!output.stdout.contains("telemetry <status|on|off"));

        for equivalent in [run(std::iter::empty::<&str>()), run(["help"])] {
            assert_eq!(equivalent.status, 0);
            assert_eq!(equivalent.stdout, output.stdout);
            assert!(equivalent.stdout.lines().count() <= 25);
        }
    }

    #[test]
    fn help_all_preserves_complete_command_discovery() {
        let output = run(["help", "--all"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        for command in [
            "setup [--project <path>]",
            "autosync <status|enable|start|stop|disable|run>",
            "storage clean [--project <path>]",
            "resync [--project <path>]",
            "install [--target <agent[,agent]>]",
            "instructions <status|sync|remove> --file <path>",
            "telemetry <status|on|off|export|upload|purge|research-*|experiment-*>",
        ] {
            assert!(output.stdout.contains(command), "missing {command}");
        }
    }

    #[test]
    fn instructions_sync_is_explicit_json_dry_run_and_reversible() {
        let workspace = TempWorkspace::new("cli-instructions-sync");
        let instruction_file = workspace.path().join("AGENTS.md");
        fs::write(&instruction_file, "# User guide\n\nkeep me\n").expect("seed guide");
        let env = |_: &str| None;

        let status = run_with_context(
            ["instructions", "status", "--file", "AGENTS.md", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(status.status, 0, "{}", status.stderr);
        let status_json: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(status_json["state_before"], "missing");
        assert_eq!(status_json["expected_content_version"], 3);
        assert_eq!(status_json["changed"], false);
        assert_eq!(status_json["session_restart_recommended"], false);
        assert!(!status
            .stdout
            .contains(&workspace.path().display().to_string()));

        let dry_run = run_with_context(
            [
                "instructions",
                "sync",
                "--file",
                "AGENTS.md",
                "--dry-run",
                "--json",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(dry_run.status, 0, "{}", dry_run.stderr);
        let dry_run_json: Value =
            serde_json::from_str(dry_run.stdout.trim()).expect("dry-run JSON");
        assert_eq!(dry_run_json["status"], "dry_run");
        assert_eq!(dry_run_json["action"], "would_append");
        assert_eq!(dry_run_json["would_change"], true);
        assert_eq!(dry_run_json["changed"], false);
        assert_eq!(dry_run_json["session_restart_recommended"], false);
        assert!(!fs::read_to_string(&instruction_file)
            .expect("dry-run preserved")
            .contains(MANAGED_INSTRUCTION_BEGIN));

        let unconfirmed = run_with_context(
            ["instructions", "sync", "--file", "AGENTS.md", "--json"],
            workspace.path(),
            &env,
        );
        assert_eq!(unconfirmed.status, 2);
        let unconfirmed_json: Value =
            serde_json::from_str(unconfirmed.stderr.trim()).expect("refusal JSON");
        assert_eq!(unconfirmed_json["status"], "refused");
        assert_eq!(unconfirmed_json["refusal"], "confirmation_required");
        assert_eq!(unconfirmed_json["session_restart_recommended"], false);

        let synced = run_with_context(
            [
                "instructions",
                "sync",
                "--file",
                "AGENTS.md",
                "--yes",
                "--json",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(synced.status, 0, "{}", synced.stderr);
        let synced_json: Value = serde_json::from_str(synced.stdout.trim()).expect("sync JSON");
        assert_eq!(synced_json["state_after"], "current");
        assert_eq!(synced_json["action"], "appended");
        assert_eq!(synced_json["changed"], true);
        assert_eq!(synced_json["session_restart_recommended"], true);
        let with_gate = fs::read_to_string(&instruction_file).expect("synced guide");
        assert!(with_gate.contains("before any non-trivial code location"));
        assert!(with_gate.contains("operation: \"find_analogues\""));
        assert!(with_gate.contains("state that reason before CodeGraph"));
        assert!(with_gate.contains("Call a given target only once"));
        assert!(!workspace.path().join("CLAUDE.md").exists());
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());

        let synced_human = run_with_context(
            ["instructions", "sync", "--file", "AGENTS.md", "--yes"],
            workspace.path(),
            &env,
        );
        assert_eq!(synced_human.status, 0, "{}", synced_human.stderr);
        assert!(synced_human.stdout.contains(
            "next: restart the coding-agent session; already-open Codex/Claude MCP child processes do not hot-swap RepoGrammar binaries or managed instructions"
        ));

        let remove_plan = run_with_context(
            [
                "instructions",
                "remove",
                "--file",
                "AGENTS.md",
                "--dry-run",
                "--json",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(remove_plan.status, 0, "{}", remove_plan.stderr);
        assert!(fs::read_to_string(&instruction_file)
            .expect("remove dry-run preserved")
            .contains(MANAGED_INSTRUCTION_BEGIN));

        let removed = run_with_context(
            [
                "instructions",
                "remove",
                "--file",
                "AGENTS.md",
                "--yes",
                "--json",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(removed.status, 0, "{}", removed.stderr);
        let after = fs::read_to_string(&instruction_file).expect("removed guide");
        assert!(after.contains("keep me"));
        assert!(!after.contains(MANAGED_INSTRUCTION_BEGIN));
    }

    #[test]
    fn instructions_sync_preserves_foreign_and_malformed_sections() {
        let workspace = TempWorkspace::new("cli-instructions-refusal");
        let env = |_: &str| None;
        for (name, contents, expected_state, expected_refusal) in [
            (
                "FOREIGN.md",
                format!("{MANAGED_INSTRUCTION_BEGIN}\nforeign body\n{MANAGED_INSTRUCTION_END}\n"),
                "foreign",
                "foreign_managed_section",
            ),
            (
                "MALFORMED.md",
                format!("{MANAGED_INSTRUCTION_BEGIN}\nmissing end\n"),
                "malformed",
                "malformed_managed_section",
            ),
        ] {
            let path = workspace.path().join(name);
            fs::write(&path, &contents).expect("seed refused guide");
            let status = run_with_context(
                ["instructions", "status", "--file", name, "--json"],
                workspace.path(),
                &env,
            );
            assert_eq!(status.status, 0, "{name}: {}", status.stderr);
            let status_json: Value =
                serde_json::from_str(status.stdout.trim()).expect("status JSON");
            assert_eq!(status_json["state_before"], expected_state);
            assert_eq!(status_json["repairable"], false);

            let sync = run_with_context(
                ["instructions", "sync", "--file", name, "--yes", "--json"],
                workspace.path(),
                &env,
            );
            assert_eq!(sync.status, 2, "{name}");
            let sync_json: Value = serde_json::from_str(sync.stderr.trim()).expect("refusal JSON");
            assert_eq!(sync_json["refusal"], expected_refusal);
            assert_eq!(fs::read_to_string(&path).expect("preserved"), contents);
        }
    }

    #[test]
    fn setup_dry_run_is_json_and_zero_write() {
        let project = TempWorkspace::new("cli-setup-dry-run-project");
        let home = TempWorkspace::new("cli-setup-dry-run-home");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--target",
                "auto",
                "--dry-run",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &TestRuntime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["telemetry_changed"], false);
        assert_eq!(value["telemetry_enabled_by_setup"], false);
        assert_eq!(value["limitations"][0], "no_live_agent");
        assert!(!project.path().join(DEFAULT_STATE_DIR).exists());
        assert_eq!(fs::read_dir(home.path()).expect("home").count(), 0);
    }

    #[test]
    fn setup_requires_one_confirmation_before_writes() {
        let project = TempWorkspace::new("cli-setup-confirm-project");
        let home = TempWorkspace::new("cli-setup-confirm-home");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };

        let noninteractive = run_with_context_and_runtime(
            ["setup", "--no-autosync"],
            project.path(),
            &env,
            &TestRuntime,
        );
        assert_eq!(noninteractive.status, 2);
        assert!(!project.path().join(DEFAULT_STATE_DIR).exists());

        let prompt = SetupConfirmationPrompt {
            confirmations: Cell::new(0),
            response: "yes",
        };
        let confirmed = run_with_context_runtime_prompt(
            ["setup", "--no-autosync", "--progress", "never"],
            project.path(),
            &env,
            &TestRuntime,
            &prompt,
        );
        assert_eq!(confirmed.status, 0, "{}", confirmed.stderr);
        assert_eq!(prompt.confirmations.get(), 1);
        assert!(confirmed
            .stdout
            .contains("setup: completed with limitations"));
        assert!(confirmed
            .stdout
            .contains("no supported pattern groups verified"));
        assert!(!confirmed.stdout.contains("pattern groups ready"));
        assert!(confirmed
            .stdout
            .contains("repogrammar_context self-test passed"));
        assert!(confirmed.stdout.contains("agent MCP: not active"));
        assert!(!confirmed.stdout.contains("Ask your coding agent:"));
        assert!(project.path().join(DEFAULT_STATE_DIR).is_dir());
    }

    #[test]
    fn setup_yes_builds_an_active_index_without_an_agent() {
        let project = TempWorkspace::new("cli-setup-live-project");
        let home = TempWorkspace::new("cli-setup-live-home");
        fs::write(
            project.path().join("app.py"),
            "def handler():\n    return 1\n",
        )
        .expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &TestRuntime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "ready_with_limitations");
        assert_eq!(value["index"]["indexed_files"], 1);
        assert_eq!(value["limitations"][0], "no_live_agent");
        assert_eq!(value["ready_agent_targets"], json!([]));
        assert_eq!(
            value["blocked_agent_targets"],
            json!(["codex", "claude-code"])
        );
        assert_eq!(value["product_self_test_state"], "passed");
        assert_eq!(value["agent_query_ready"], false);
        assert_eq!(value["repository_index_ready"], true);
        assert!(value["suggested_question"].is_null());
        let status = TestRuntime
            .repository_status(RepositoryStatusRequest {
                path: project.path().display().to_string(),
                state_dir_override: None,
            })
            .expect("status");
        assert!(status.readiness.active_generation_available);
    }

    #[test]
    fn setup_cli_probe_refreshes_an_active_but_stale_repository() {
        let project = TempWorkspace::new("cli-setup-stale-project");
        let home = TempWorkspace::new("cli-setup-stale-home");
        fs::write(
            project.path().join("app.py"),
            "def handler():\n    return 1\n",
        )
        .expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };
        let first = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &TestRuntime,
        );
        assert_eq!(first.status, 0, "{}", first.stderr);

        let runtime = SetupIntegrationRuntime::new(false).with_stale_status();
        let rerun = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(rerun.status, 0, "{}", rerun.stderr);
        assert_eq!(&*runtime.calls.borrow(), &["index", "mcp"]);
        let value: Value = serde_json::from_str(rerun.stdout.trim()).expect("setup JSON");
        assert!(value["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .any(|stage| stage["stage"] == "repository_index" && stage["status"] == "completed"));
    }

    #[test]
    fn setup_live_agent_path_orders_install_index_and_product_self_test() {
        let project = TempWorkspace::new("cli-setup-agent-project");
        let home = TempWorkspace::new("cli-setup-agent-home");
        let path = TempWorkspace::new("cli-setup-agent-path");
        fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
        fs::write(
            project.path().join("routes.py"),
            "def list_routes():\n    return []\n",
        )
        .expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(path.path().display().to_string()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(false);

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--target",
                "codex",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(&*runtime.calls.borrow(), &["install", "index", "mcp"]);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "ready_with_limitations");
        assert_eq!(value["telemetry_changed"], false);
        assert_eq!(value["telemetry_enabled_by_setup"], false);
        assert_eq!(value["limitations"][0], "no_pattern_groups");
        assert_eq!(value["ready_agent_targets"], json!(["codex"]));
        assert_eq!(value["agent_query_ready"], true);
        assert!(value["suggested_question"].as_str().is_some());
        assert!(!output
            .stdout
            .contains(&project.path().display().to_string()));
    }

    #[test]
    fn setup_index_failure_rolls_back_only_new_agent_integration() {
        let project = TempWorkspace::new("cli-setup-rollback-project");
        let home = TempWorkspace::new("cli-setup-rollback-home");
        let path = TempWorkspace::new("cli-setup-rollback-path");
        fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(path.path().display().to_string()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(true);

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--target",
                "codex",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 1);
        assert_eq!(&*runtime.calls.borrow(), &["install", "index", "uninstall"]);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "failed");
        assert_eq!(value["failure"]["class"], "index_failed");
        assert_eq!(value["rollback"]["succeeded"], true);
        assert!(project.path().join(DEFAULT_STATE_DIR).is_dir());
        let status = TestRuntime
            .repository_status(RepositoryStatusRequest {
                path: project.path().display().to_string(),
                state_dir_override: None,
            })
            .expect("status");
        assert!(!status.readiness.active_generation_available);
    }

    #[test]
    fn setup_human_failures_are_sanitized_and_auditable() {
        for (name, runtime, autosync) in [
            (
                "index",
                SetupIntegrationRuntime::failing(SetupStage::RepositoryIndex),
                false,
            ),
            (
                "autosync",
                SetupIntegrationRuntime::failing(SetupStage::Autosync),
                true,
            ),
            (
                "mcp",
                SetupIntegrationRuntime::failing(SetupStage::McpSelfTest),
                false,
            ),
            (
                "rollback",
                SetupIntegrationRuntime::failing(SetupStage::RepositoryIndex)
                    .with_rollback_failure(),
                false,
            ),
        ] {
            let project = TempWorkspace::new(&format!("cli-setup-human-{name}-project"));
            let home = TempWorkspace::new(&format!("cli-setup-human-{name}-home"));
            let path = TempWorkspace::new(&format!("cli-setup-human-{name}-path"));
            fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
            fs::write(
                project.path().join("routes.py"),
                "def list_routes():\n    return []\n",
            )
            .expect("fixture");
            let env = |key: &str| match key {
                "HOME" => Some(home.path().display().to_string()),
                "PATH" => Some(path.path().display().to_string()),
                _ => None,
            };
            let mut args = vec!["setup", "--target", "codex", "--yes", "--progress", "never"];
            if !autosync {
                args.push("--no-autosync");
            }

            let output = run_with_context_and_runtime(args, project.path(), &env, &runtime);

            assert_eq!(output.status, 1, "{name}: {}", output.stderr);
            let human = &output.stderr;
            for prefix in ["completed: ", "retained: ", "rollback: ", "failed: "] {
                assert_eq!(
                    human
                        .lines()
                        .filter(|line| line.starts_with(prefix))
                        .count(),
                    1,
                    "{name}: {}",
                    human
                );
            }
            assert_eq!(
                human
                    .lines()
                    .filter(|line| line.starts_with("next: "))
                    .count(),
                1,
                "{name}: {}",
                human
            );
            assert!(!human.contains("/private/secret"));
            assert!(!human.contains(&project.path().display().to_string()));
        }
    }

    #[test]
    fn setup_preserves_foreign_and_drifted_native_integrations() {
        for (inspection, limitation) in [
            (
                AgentIntegrationInspection::Foreign,
                "foreign_agent_configuration",
            ),
            (
                AgentIntegrationInspection::OwnedDrifted,
                "malformed_agent_configuration",
            ),
            (
                AgentIntegrationInspection::Malformed,
                "malformed_agent_configuration",
            ),
        ] {
            let project = TempWorkspace::new("cli-setup-preserve-native-project");
            let home = TempWorkspace::new("cli-setup-preserve-native-home");
            let path = TempWorkspace::new("cli-setup-preserve-native-path");
            fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
            fs::write(project.path().join("app.py"), "def app():\n    return 1\n")
                .expect("fixture");
            let env = |key: &str| match key {
                "HOME" => Some(home.path().display().to_string()),
                "PATH" => Some(path.path().display().to_string()),
                _ => None,
            };
            let runtime = SetupIntegrationRuntime::new(false).with_inspection(inspection);

            let output = run_with_context_and_runtime(
                [
                    "setup",
                    "--target",
                    "codex",
                    "--yes",
                    "--no-autosync",
                    "--json",
                    "--progress",
                    "never",
                ],
                project.path(),
                &env,
                &runtime,
            );

            assert_eq!(output.status, 0, "{}", output.stderr);
            assert_eq!(&*runtime.calls.borrow(), &["index", "mcp"]);
            let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
            assert_eq!(value["status"], "ready_with_limitations");
            assert!(value["limitations"]
                .as_array()
                .expect("limitations")
                .iter()
                .any(|value| value.as_str() == Some(limitation)));
            if inspection == AgentIntegrationInspection::Malformed {
                assert_eq!(value["recovery"], "repogrammar doctor");
            }
        }
    }

    #[test]
    fn setup_fresh_rerun_with_zero_families_uses_source_fallback() {
        let project = TempWorkspace::new("cli-setup-zero-family-rerun-project");
        let home = TempWorkspace::new("cli-setup-zero-family-rerun-home");
        fs::write(project.path().join("app.py"), "def app():\n    return 1\n").expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };
        let first = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &TestRuntime,
        );
        assert_eq!(first.status, 0, "{}", first.stderr);

        let runtime = SetupIntegrationRuntime::new(false);
        let rerun = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(rerun.status, 0, "{}", rerun.stderr);
        assert_eq!(&*runtime.calls.borrow(), &["mcp"]);
        let value: Value = serde_json::from_str(rerun.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "ready_with_limitations");
        assert!(value["limitations"]
            .as_array()
            .expect("limitations")
            .iter()
            .any(|value| value.as_str() == Some("no_pattern_groups")));
        assert_eq!(value["recovery"], "use normal source search");
        assert!(value["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .any(|stage| { stage["stage"] == "repository_index" && stage["status"] == "skipped" }));
    }

    #[test]
    fn setup_family_inventory_failure_remains_unknown_instead_of_zero() {
        let project = TempWorkspace::new("cli-setup-family-inventory-failure-project");
        let home = TempWorkspace::new("cli-setup-family-inventory-failure-home");
        fs::write(project.path().join("app.py"), "def app():\n    return 1\n").expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(false).with_family_inventory_failure();

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["family_evidence_state"], "unknown");
        assert_eq!(value["index"]["pattern_groups"], Value::Null);
        assert!(!value["limitations"]
            .as_array()
            .expect("limitations")
            .iter()
            .any(|value| value == "no_pattern_groups"));
        assert!(!output.stdout.contains("/private/secret"));
    }

    #[test]
    fn setup_refreshes_outdated_owned_agent_without_deleting_it_after_index_failure() {
        let project = TempWorkspace::new("cli-setup-outdated-owned-project");
        let home = TempWorkspace::new("cli-setup-outdated-owned-home");
        let path = TempWorkspace::new("cli-setup-outdated-owned-path");
        fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(path.path().display().to_string()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(true)
            .with_inspection(AgentIntegrationInspection::OwnedOutdated);

        let output = run_with_context_and_runtime(
            ["setup", "--target", "codex", "--yes", "--no-autosync"],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 1);
        assert_eq!(&*runtime.calls.borrow(), &["install", "index"]);
        assert!(output.stderr.contains("pre-existing agent integration"));
        assert!(output.stderr.contains("rollback: not required"));
    }

    #[test]
    fn setup_human_plan_distinguishes_initialization_from_indexing() {
        let project = TempWorkspace::new("cli-setup-stage-label-project");
        let home = TempWorkspace::new("cli-setup-stage-label-home");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };

        let output = run_with_context_and_runtime(
            ["setup", "--dry-run", "--no-autosync"],
            project.path(),
            &env,
            &TestRuntime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        assert!(output
            .stdout
            .contains("- repository initialization: will run"));
        assert!(output.stdout.contains("- repository indexing: will run"));
    }

    #[test]
    fn setup_human_output_reports_every_preserved_agent_limitation() {
        let project = TempWorkspace::new("cli-setup-all-limitations-project");
        let home = TempWorkspace::new("cli-setup-all-limitations-home");
        let path = TempWorkspace::new("cli-setup-all-limitations-path");
        fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
        fs::write(path.path().join("claude"), "fake native agent").expect("fake claude");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(path.path().display().to_string()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(false).with_target_inspections(
            AgentIntegrationInspection::Foreign,
            AgentIntegrationInspection::Malformed,
        );

        let output = run_with_context_and_runtime(
            ["setup", "--yes", "--no-autosync"],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        assert!(output
            .stdout
            .contains("foreign agent configuration was preserved"));
        assert!(output
            .stdout
            .contains("malformed agent configuration was preserved"));
        assert!(!output.stdout.contains("Ask your coding agent:"));
    }

    #[test]
    fn setup_unknown_native_probe_fails_closed_without_raw_error_output() {
        let project = TempWorkspace::new("cli-setup-native-probe-unknown-project");
        let home = TempWorkspace::new("cli-setup-native-probe-unknown-home");
        let path = TempWorkspace::new("cli-setup-native-probe-unknown-path");
        fs::write(path.path().join("codex"), "fake native agent").expect("fake codex");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(path.path().display().to_string()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(false).with_probe_failure();

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--target",
                "codex",
                "--yes",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 1);
        assert!(runtime.calls.borrow().is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["failed_stage"], "inspect");
        assert_eq!(value["failure_class"], "agent_detection_failed");
        assert!(!output.stdout.contains("native MCP probe failed"));
        assert!(!output
            .stdout
            .contains(&project.path().display().to_string()));
    }

    #[test]
    fn setup_missing_agent_skips_native_probe_and_completes_repository_only() {
        let project = TempWorkspace::new("cli-setup-native-probe-missing-project");
        let home = TempWorkspace::new("cli-setup-native-probe-missing-home");
        fs::write(project.path().join("app.py"), "def app():\n    return 1\n").expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };
        let runtime = SetupIntegrationRuntime::new(false).with_probe_failure();

        let output = run_with_context_and_runtime(
            [
                "setup",
                "--target",
                "codex",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(&*runtime.calls.borrow(), &["index", "mcp"]);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("setup JSON");
        assert_eq!(value["status"], "ready_with_limitations");
        assert!(value["limitations"]
            .as_array()
            .expect("limitations")
            .iter()
            .any(|value| value.as_str() == Some("agent_missing")));
    }

    #[test]
    fn setup_preserves_pre_enabled_telemetry_and_reports_policy_not_current_state() {
        let project = TempWorkspace::new("cli-setup-telemetry-project");
        let home = TempWorkspace::new("cli-setup-telemetry-home");
        fs::write(project.path().join("app.py"), "def app():\n    return 1\n").expect("fixture");
        let env = |key: &str| match key {
            "HOME" => Some(home.path().display().to_string()),
            "PATH" => Some(String::new()),
            _ => None,
        };
        let enabled = run_with_context_and_runtime(
            ["telemetry", "on", "--json"],
            project.path(),
            &env,
            &TestRuntime,
        );
        assert_eq!(enabled.status, 0, "{}", enabled.stderr);

        let setup = run_with_context_and_runtime(
            [
                "setup",
                "--yes",
                "--no-autosync",
                "--json",
                "--progress",
                "never",
            ],
            project.path(),
            &env,
            &TestRuntime,
        );
        assert_eq!(setup.status, 0, "{}", setup.stderr);
        let setup_json: Value = serde_json::from_str(setup.stdout.trim()).expect("setup JSON");
        assert_eq!(setup_json["telemetry_changed"], false);
        assert_eq!(setup_json["telemetry_enabled_by_setup"], false);
        assert!(setup_json.get("telemetry_enabled").is_none());

        let status = run_with_context_and_runtime(
            ["telemetry", "status", "--json"],
            project.path(),
            &env,
            &TestRuntime,
        );
        let status_json: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(status_json["enabled"], true);
    }

    #[test]
    fn setup_option_contract_rejects_invalid_values_and_accepts_redundant_yes() {
        assert_eq!(run(["setup", "--target", "cursor", "--dry-run"]).status, 2);
        assert!(parse_setup_options(&["--dry-run".to_string(), "--yes".to_string()]).is_ok());
        assert_eq!(run(["setup", "--progress", "sometimes"]).status, 2);
        assert_eq!(run(["setup", "--unknown"]).status, 2);
        assert!(run(["help", "setup"]).stdout.contains("--no-autosync"));
    }

    #[test]
    fn init_help_lists_combined_bootstrap_options() {
        let output = run(["help", "init"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output
            .stdout
            .contains("[--state-only] [--resync] [--autosync|--no-autosync]"));
        assert!(output.stdout.contains("starts autosync by default"));
        assert!(output.stdout.contains("Use --no-autosync for CI"));
        assert!(output
            .stdout
            .contains("Create or repair lifecycle state without indexing or autosync"));

        let full = run(["help", "--all"]);
        assert_eq!(full.status, 0);
        assert!(full.stdout.contains("[--autosync|--no-autosync]"));
        assert!(full.stdout.contains("start autosync by default"));
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
    fn telemetry_help_scopes_project_away_from_experiment_subcommands() {
        let output = run(["help", "telemetry"]);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert!(output
            .stdout
            .contains("Repository root for anonymous telemetry or research diagnostics only"));
        assert!(output
            .stdout
            .contains("Experiment subcommands accept only the options listed in their section"));
        assert!(output.stdout.contains("they do not accept --project"));
        assert!(output
            .stdout
            .contains("research-purge [--json] [--yes] [--project <path>]"));
    }

    #[test]
    fn telemetry_experiment_subcommands_reject_project_option() {
        for (subcommand, expected_error) in [
            (
                "experiment-start",
                "unknown experiment-start option: --project\n",
            ),
            (
                "experiment-record",
                "unknown experiment-record option: --project\n",
            ),
            (
                "experiment-report",
                "unknown experiment option: --project\n",
            ),
        ] {
            let output = run(["telemetry", subcommand, "--project", "."]);

            assert_eq!(output.status, 2, "{subcommand}");
            assert!(output.stdout.is_empty(), "{subcommand}");
            assert_eq!(output.stderr, expected_error, "{subcommand}");
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
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar setup\n"
            ));
            assert!(output.stderr.contains("not implemented yet"));
            assert!(output.stdout.is_empty());
        }
        for command in ["files", "units"] {
            let output = run_with_context([command], workspace.path(), &env);

            assert_eq!(output.status, 2);
            assert!(output.stderr.starts_with(
                "FALLBACK_TO_CODE_SEARCH\nreason: repository is not initialized\nguidance: run repogrammar setup\n"
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
            freshness_counts: None,
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::StaleEvidence,
                affected_claim:
                    "family:python:fastapi_route:framework_fastapi_route:evidence_freshness"
                        .to_string(),
                recovery: Some("run repogrammar resync".to_string()),
            }],
        };

        let output = families_human(&report, true);

        assert!(output.starts_with("families: UNKNOWN\nactive_generation: gen-000001\n"));
        assert!(output.contains(
            "unknown: blocking_unknown:StaleEvidence affected_claim: family:python:fastapi_route:framework_fastapi_route:evidence_freshness\n"
        ));
        assert!(output.contains("recovery: run repogrammar resync\n"));
        assert!(!output.contains("InsufficientSupport"));
        assert!(!output.contains("adding compatible implementations"));
    }

    #[test]
    fn families_compact_human_groups_variants_without_internal_ids() {
        let report = FamilyListReport {
            active_generation: "gen-000001".to_string(),
            families: vec![
                FamilySummary {
                    family_id: "family:python:route:framework_fastapi_route:cluster_alpha"
                        .to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    support: 3,
                    prevalence: crate::test_support::sample_family_prevalence(),
                    freshness: None,
                },
                FamilySummary {
                    family_id: "family:python:route:framework_fastapi_route:cluster_beta"
                        .to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    support: 4,
                    prevalence: crate::test_support::sample_family_prevalence(),
                    freshness: None,
                },
            ],
            freshness_counts: None,
            unknowns: Vec::new(),
        };

        let output = families_human(&report, false);

        assert!(output.lines().count() <= 15);
        assert!(output.contains("Python · FastAPI Route"));
        assert!(output.contains("2 group(s), 7 implementation(s)"));
        assert!(output.contains("next: repogrammar find"));
        assert!(!output.contains("cluster_"));
        assert!(!output.contains("family:python"));
    }

    #[test]
    fn families_all_and_json_retain_canonical_family_ids() {
        let report = FamilyListReport {
            active_generation: "gen-000001".to_string(),
            families: vec![FamilySummary {
                family_id: "family:python:route:framework_fastapi_route:cluster_alpha".to_string(),
                classification: "DOMINANT_PATTERN".to_string(),
                support: 3,
                prevalence: crate::test_support::sample_family_prevalence(),
                freshness: None,
            }],
            freshness_counts: None,
            unknowns: Vec::new(),
        };

        let detailed = families_human(&report, true);
        assert!(detailed.contains("family:python:route:framework_fastapi_route:cluster_alpha"));

        let value: Value =
            serde_json::from_str(families_json("families", &report).trim()).expect("families JSON");
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(
            value["families"][0]["family_id"],
            "family:python:route:framework_fastapi_route:cluster_alpha"
        );
        assert_eq!(value["families"][0]["classification"], "DOMINANT_PATTERN");
        assert_eq!(value["families"][0]["support"], 3);
        // The list surface exposes the metadata-only prevalence object.
        let prevalence = &value["families"][0]["prevalence"];
        assert_eq!(prevalence["eligible_peer_count"], 2);
        assert_eq!(prevalence["supported_member_count"], 2);
        assert_eq!(prevalence["coverage_ratio"], 1.0);
        assert_eq!(prevalence["competing_ready_family_count"], 0);
        assert_eq!(prevalence["largest_competing_support"], 0);
        assert_eq!(prevalence["blocked_peer_count"], 0);
        assert_eq!(prevalence["unsupported_peer_count"], 0);
        assert_eq!(
            prevalence["classification_reason"],
            "coverage 2/2 with no competing ready family"
        );
    }

    #[test]
    fn families_freshness_fields_surface_in_json_and_human() {
        let report = FamilyListReport {
            active_generation: "gen-000001".to_string(),
            families: vec![
                FamilySummary {
                    family_id: "family:python:route:framework_fastapi_route:cluster_fresh"
                        .to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    support: 3,
                    prevalence: crate::test_support::sample_family_prevalence(),
                    freshness: Some(FamilyFreshness::Fresh),
                },
                FamilySummary {
                    family_id: "family:python:route:framework_fastapi_route:cluster_stale"
                        .to_string(),
                    classification: "DOMINANT_PATTERN".to_string(),
                    support: 2,
                    prevalence: crate::test_support::sample_family_prevalence(),
                    freshness: Some(FamilyFreshness::Stale),
                },
            ],
            freshness_counts: Some(FamilyFreshnessCounts {
                fresh_count: 1,
                stale_count: 1,
                cannot_verify_count: 0,
            }),
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::StaleEvidence,
                affected_claim: "repository pattern families:evidence_freshness".to_string(),
                recovery: Some("run repogrammar resync".to_string()),
            }],
        };

        // JSON carries the verbatim per-family field and the report-level counts.
        let value: Value =
            serde_json::from_str(families_json("families", &report).trim()).expect("families JSON");
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["families"][0]["freshness"], "fresh");
        assert_eq!(value["families"][1]["freshness"], "stale");
        assert_eq!(value["fresh_count"], 1);
        assert_eq!(value["stale_count"], 1);
        assert_eq!(value["cannot_verify_count"], 0);
        assert_eq!(value["unknowns"][0]["reason"], "StaleEvidence");

        // Compact human leads with the counts and the resync guidance.
        let compact = families_human(&report, false);
        assert!(compact.contains("freshness: 1 fresh · 1 stale · 0 cannot verify\n"));
        assert!(compact.contains("run repogrammar resync"));

        // Detailed human annotates each family and surfaces the stale unknown.
        let detailed = families_human(&report, true);
        assert!(detailed.contains("freshness: 1 fresh · 1 stale · 0 cannot verify\n"));
        assert!(detailed.contains("\tfreshness: stale\t"));
        assert!(detailed.contains("blocking_unknown:StaleEvidence"));
    }

    #[test]
    fn family_unknown_human_formats_recovery_as_separate_line() {
        let report = FamilyUnknownReport {
            active_generation: "gen-000001".to_string(),
            candidate_family_ids: Vec::new(),
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::StaleEvidence,
                affected_claim:
                    "family:python:pytest_test:framework_pytest_test:evidence_freshness".to_string(),
                recovery: Some("run repogrammar resync".to_string()),
            }],
            term_retrieval: None,
        };

        let lookup = FamilyLookupReport::Unknown(report.clone());
        let route = family_query_route_report(&lookup, FamilyLookupMode::ExactFamilyId);
        let output = family_unknown_human("family", &report, &route);

        assert!(output.contains(
            "unknown: blocking_unknown:StaleEvidence affected_claim: family:python:pytest_test:framework_pytest_test:evidence_freshness\nrecovery: run repogrammar resync\n"
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
    fn compact_json_dry_run_reports_sizes_without_absolute_paths() {
        let workspace = TempWorkspace::new("cli-compact-dry-run");
        let env = |_: &str| None;
        let runtime = CompactRuntime::default();
        let output = run_with_context_and_runtime(
            ["compact", "--dry-run", "--json"],
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
        let compact = runtime.last_compact.borrow().expect("compact request");
        assert!(compact.dry_run);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("compact JSON");
        assert_eq!(value["command"], "compact");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(value["before"]["total_bytes"], 176);
        assert_eq!(value["after"]["total_bytes"], 176);
        assert_eq!(value["reclaimed_bytes"], 0);
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn compact_requires_yes_without_dry_run() {
        let workspace = TempWorkspace::new("cli-compact-requires-yes");
        let env = |_: &str| None;
        let runtime = CompactRuntime::default();

        let output = run_with_context_and_runtime(["compact"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("compact requires --yes unless --dry-run is present"));
        assert!(runtime.last_compact.borrow().is_none());
    }

    #[test]
    fn compact_human_with_yes_reports_size_effects() {
        let workspace = TempWorkspace::new("cli-compact-human");
        let env = |_: &str| None;
        let runtime = CompactRuntime::default();
        let output =
            run_with_context_and_runtime(["compact", "--yes"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0, "{output:?}");
        let compact = runtime.last_compact.borrow().expect("compact request");
        assert!(!compact.dry_run);
        assert!(output.stdout.contains("compact: complete\n"));
        assert!(output.stdout.contains("active_generation: gen-000004\n"));
        assert!(output.stdout.contains("total_bytes_before: 176\n"));
        assert!(output.stdout.contains("total_bytes_after: 112\n"));
        assert!(output.stdout.contains("reclaimed_bytes: 64\n"));
    }

    #[test]
    fn compact_rejects_unknown_options() {
        let workspace = TempWorkspace::new("cli-compact-unknown-option");
        let env = |_: &str| None;
        let runtime = CompactRuntime::default();

        let output = run_with_context_and_runtime(
            ["compact", "--dry-run", "--mystery"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stderr.contains("unknown compact option: --mystery"));
        assert!(runtime.last_compact.borrow().is_none());
    }

    #[test]
    fn storage_clean_json_dry_run_reports_candidates_without_absolute_paths() {
        let workspace = TempWorkspace::new("cli-storage-clean-dry-run");
        let env = |_: &str| None;
        let runtime = StorageCleanRuntime::default();
        let output = run_with_context_and_runtime(
            ["storage", "clean", "--dry-run", "--json"],
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
        let clean = runtime.last_clean.borrow().expect("clean request");
        assert!(clean.dry_run);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("storage JSON");
        assert_eq!(value["command"], "storage clean");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(value["legacy_layout"]["present_before"], true);
        assert_eq!(value["legacy_layout"]["removed"], false);
        assert_eq!(value["prune"]["keep_inactive"], 0);
        assert_eq!(value["prune"]["candidate_generations"][0], "gen-000001");
        assert_eq!(value["compact"]["reclaimed_bytes"], 0);
        assert_eq!(value["reclaimed_bytes"], 0);
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn storage_clean_requires_yes_without_dry_run() {
        let workspace = TempWorkspace::new("cli-storage-clean-requires-yes");
        let env = |_: &str| None;
        let runtime = StorageCleanRuntime::default();

        let output =
            run_with_context_and_runtime(["storage", "clean"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output
            .stderr
            .contains("storage clean requires --yes unless --dry-run is present"));
        assert!(runtime.last_clean.borrow().is_none());
    }

    #[test]
    fn storage_clean_human_with_yes_reports_reclaimed_bytes() {
        let workspace = TempWorkspace::new("cli-storage-clean-human");
        let env = |_: &str| None;
        let runtime = StorageCleanRuntime::default();
        let output = run_with_context_and_runtime(
            ["storage", "clean", "--yes"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        let clean = runtime.last_clean.borrow().expect("clean request");
        assert!(!clean.dry_run);
        assert!(output.stdout.contains("storage clean: complete\n"));
        assert!(output.stdout.contains("legacy_layout_removed: true\n"));
        assert!(output.stdout.contains("prune_keep_inactive: 0\n"));
        assert!(output.stdout.contains("pruned_generation: gen-000001\n"));
        assert!(output.stdout.contains("compact_reclaimed_bytes: 64\n"));
        assert!(output.stdout.contains("reclaimed_bytes: 1088\n"));
    }

    #[test]
    fn storage_rejects_unknown_subcommand() {
        let workspace = TempWorkspace::new("cli-storage-unknown-subcommand");
        let env = |_: &str| None;
        let runtime = StorageCleanRuntime::default();

        let output =
            run_with_context_and_runtime(["storage", "vacuum"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stderr.contains("storage subcommand must be clean"));
        assert!(runtime.last_clean.borrow().is_none());
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
        assert_eq!(fallback["guidance"], "run repogrammar setup");
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
        assert!(
            parse_query_options(&["--all".to_string()])
                .expect("families detail option")
                .all
        );
    }

    #[test]
    fn query_options_parse_and_gate_against_scope() {
        // `--against` parses into the query options as the comparison-family scope.
        let parsed = parse_query_options(&[
            "--against".to_string(),
            "family:python:fastapi_route:framework_fastapi_route".to_string(),
            "unit:app/api/routes.py#fastapi_route:read:0-1:1".to_string(),
        ])
        .expect("--against parses");
        assert_eq!(
            parsed.against.as_deref(),
            Some("family:python:fastapi_route:framework_fastapi_route")
        );

        // `--against` validates its value like a target (rejects control text).
        assert!(parse_query_options(&["--against".to_string(), "bad\nvalue".to_string()]).is_err());
        // `--against` requires a value.
        assert!(parse_query_options(&["--against".to_string()]).is_err());

        // `explain`/`check` accept `--against`; other query commands reject it
        // (never silently ignore) with a nonzero exit.
        let workspace = TempWorkspace::new("cli-query-against-gate");
        let env = |_: &str| None;
        for command in ["find", "family", "member"] {
            let output = run_with_context(
                [
                    command,
                    "--against",
                    "family:python:fastapi_route:framework_fastapi_route",
                    "target",
                ],
                workspace.path(),
                &env,
            );
            assert_eq!(output.status, 2, "{command} must reject --against");
            assert!(
                output.stderr.contains("does not accept --against"),
                "{command} rejection message: {}",
                output.stderr
            );
        }
    }

    #[test]
    fn query_options_parse_verbosity_default_valid_and_invalid() {
        // Absent `--verbosity` defaults to the byte-stable `standard` shape.
        assert_eq!(
            parse_query_options(&[])
                .expect("default query options")
                .verbosity,
            Verbosity::Standard
        );

        // Each documented value parses on the CLI surface.
        for (value, expected) in [
            ("minimal", Verbosity::Minimal),
            ("standard", Verbosity::Standard),
            ("full", Verbosity::Full),
        ] {
            assert_eq!(
                parse_query_options(&["--verbosity".to_string(), value.to_string()])
                    .expect("verbosity query options")
                    .verbosity,
                expected
            );
        }

        // Unknown value and a missing value are rejected, never silently
        // defaulted.
        assert!(parse_query_options(&["--verbosity".to_string(), "loud".to_string()]).is_err());
        assert!(parse_query_options(&["--verbosity".to_string()]).is_err());
    }

    #[test]
    fn inventory_commands_reject_inapplicable_flags() {
        let strings = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        // Only --json and --project/--path are accepted.
        assert!(inventory_flag_rejection("units", &strings(&["--json"])).is_none());
        assert!(
            inventory_flag_rejection("files", &strings(&["--project", "/repo", "--json"]))
                .is_none()
        );
        // A query-only flag is rejected rather than silently ignored.
        assert_eq!(
            inventory_flag_rejection("units", &strings(&["--token-budget", "50"])).as_deref(),
            Some("units does not accept --token-budget")
        );
        // A positional target is rejected.
        assert_eq!(
            inventory_flag_rejection("units", &strings(&["deep-target"])).as_deref(),
            Some("units does not accept a positional argument")
        );
    }

    #[test]
    fn install_scope_and_target_reject_a_following_flag() {
        assert!(parse_install_options(&["--scope".to_string(), "--yes".to_string()]).is_err());
        assert!(parse_install_options(&["--target".to_string(), "--dry-run".to_string()]).is_err());
        // A real value still parses.
        assert!(parse_install_options(&["--scope".to_string(), "project".to_string()]).is_ok());
    }

    #[test]
    fn serve_command_usage_is_available_for_help() {
        assert!(command_usage("serve").is_some());
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
            assert_eq!(fallback["guidance"], "run repogrammar setup");
            assert_eq!(fallback["command"], command);
            assert_eq!(
                fallback["implemented"],
                matches!(command, "files" | "units")
            );
        }
    }

    #[test]
    fn family_query_preflight_fallback_records_local_query_outcome() {
        let workspace = TempWorkspace::new("cli-query-fallback-rollup");
        let env = |_: &str| None;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let output = run_with_context(
            ["find", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 2);
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("query fallback JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        let rollup_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("telemetry")
            .join("local-metrics")
            .join("family_query_metrics.json");
        let rollup: Value =
            serde_json::from_str(&fs::read_to_string(rollup_path).expect("query rollup JSON"))
                .expect("query rollup");
        assert_eq!(rollup["schema_version"], "family-query-metrics.v2");
        assert_eq!(rollup["epoch"], "atomic-query-accounting.v2");
        assert_eq!(rollup["total_queries"], 1);
        assert_eq!(rollup["savings_events"], 0);
        assert_eq!(rollup["by_status"]["fallback"], 1);
        assert_eq!(rollup["by_entrypoint"]["cli"], 1);
        assert_eq!(rollup["by_command_category"]["find"], 1);
        assert_eq!(rollup["by_lookup_mode"]["fuzzy"], 1);
        assert!(rollup.get("target").is_none());
        assert!(!rollup.to_string().contains("src/routes/a.ts"));
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["official_family_scope"], "python_v0_1");
        assert_eq!(value["repo_shape_scope"], "python_family_eligible_units");
        assert_eq!(value["indexed_inventory"]["indexed_file_count"], 4);
        assert_eq!(value["indexed_inventory"]["indexed_code_unit_count"], 4);
        assert_eq!(value["indexed_inventory"]["semantic_fact_count"], 0);
        assert_eq!(value["readiness"]["state"], "unknown");
        assert_eq!(value["readiness"]["query_ready"], false);
        assert!(value["readiness"].get("local_state_hygiene").is_none());
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
        assert_eq!(value["query_outcome_rollup"]["event_count"], 0);
        assert_eq!(
            value["query_outcome_rollup"]["rollup_scope"],
            "local_query_outcomes"
        );
        assert!(value["estimated_potential_token_savings_metric"]["caveat"]
            .as_str()
            .expect("estimated caveat")
            .contains("not measured token savings"));
        assert!(value["claim"]
            .as_str()
            .expect("claim")
            .contains("not measured token savings"));
        assert_eq!(value["readiness_available"], true);
        assert_eq!(
            value["scope_explanations"]["official_family_scope"],
            "python_v0_1"
        );
        assert_eq!(
            value["scope_explanations"]["repo_shape_scope"],
            "python_family_eligible_units"
        );
        assert_eq!(
            value["scope_explanations"]["react_rn_family_support"],
            "unsupported"
        );
        assert_eq!(
            value["scope_explanations"]["tsjs_family_support"],
            "not_indexed"
        );
        assert_eq!(value["by_language"][0]["language"], "python");
        assert_eq!(value["by_language"][0]["language_scope"], "official_v0_1");
        assert_eq!(value["by_language"][0]["indexed_file_count"], 4);
        assert_eq!(value["by_language"][0]["indexed_code_unit_count"], 4);
        assert_eq!(value["by_language"][0]["eligible_code_units"], 4);
        assert_eq!(
            value["by_language"][0]["family_support_coverage"].as_f64(),
            Some(0.75)
        );
        assert_eq!(value["by_language"][0]["support_risk"], "low");
        assert_eq!(value["by_language"][0]["preview_status"], "official");
        assert_eq!(value["by_language"][0]["blocking_unknowns"], Value::Null);
        assert_eq!(
            value["by_language"][0]["unknown_inventory_available"],
            false
        );
        assert_eq!(value["by_language"][1]["language"], "typescript/javascript");
        assert_eq!(
            value["by_language"][1]["language_scope"],
            "bounded_v0_2_preview"
        );
        assert_eq!(value["by_language"][1]["indexed_code_unit_count"], 0);
        assert!(value.get("unknown_inventory").is_none());
        // The additive all-scope savings block is present with the ESTIMATED
        // discipline intact, the savings_events / total_queries denominator, and
        // the per-outcome-shape and per-language breakdown maps (empty here since
        // no query has run in this repo yet).
        let all_scope = &value["all_scope_token_savings"];
        assert_eq!(all_scope["measurement_kind"], "ESTIMATED");
        assert_eq!(all_scope["scope"], "all_languages_all_outcome_shapes");
        assert_eq!(all_scope["savings_events"], 0);
        assert_eq!(all_scope["total_queries"], 0);
        assert_eq!(all_scope["estimated_potential_token_savings"], 0);
        assert!(all_scope["by_outcome_shape"].is_object());
        assert!(all_scope["by_language"].is_object());
        assert!(all_scope["caveat"]
            .as_str()
            .expect("caveat")
            .contains("not measured token savings"));
        assert!(all_scope["note"]
            .as_str()
            .expect("note")
            .contains("official-scope subset"));
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        // The human rendering leads with a concise summary (~10 lines): it keeps
        // the readiness, inventory, family, scope, and all-scope savings
        // essentials but drops the verbose per-metric dump behind `--json`.
        let human = run_with_context_and_runtime(["stats"], workspace.path(), &env, &runtime);
        assert_eq!(human.status, 0);
        assert!(human.stdout.contains("stats: repo-shape diagnostics"));
        assert!(human.stdout.contains("official_family_scope: python_v0_1"));
        assert!(human
            .stdout
            .contains("estimated_potential_token_savings: 0"));
        assert!(human.stdout.contains("all_scope_by_outcome_shape:"));
        assert!(human.stdout.contains("run `repogrammar stats --json`"));
        // Verbose per-metric lines moved behind --json.
        assert!(!human.stdout.contains("local_pattern_density:"));
        assert!(!human.stdout.contains("thin_wrapper_risk:"));
        assert!(!human.stdout.contains("interpretation:"));
        assert!(human.stdout.lines().count() <= 14);
    }

    #[test]
    fn unknowns_json_reports_aggregate_inventory_without_paths() {
        let workspace = TempWorkspace::new("cli-unknowns-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output =
            run_with_context_and_runtime(["unknowns", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("unknowns JSON");
        assert_eq!(value["command"], "unknowns");
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(
            value["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );
        assert_eq!(value["unknown_inventory"]["total_unknowns"], 2);
        assert_eq!(value["unknown_inventory"]["blocking_unknowns"], 1);
        assert_eq!(value["unknown_inventory"]["non_blocking_unknowns"], 1);
        assert_eq!(value["unknown_inventory"]["recoverable_unknowns"], 0);
        assert_eq!(value["unknown_inventory"]["irreducible_unknowns"], 0);
        let mut inventory_keys = value["unknown_inventory"]
            .as_object()
            .expect("unknown inventory object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        inventory_keys.sort_unstable();
        assert_eq!(
            inventory_keys,
            vec![
                "active_generation",
                "blocking_unknowns",
                "by_blocks_support",
                "by_framework_role",
                "by_language",
                "by_language_detail",
                "by_obligation",
                "by_reason_code",
                "by_recovery_code",
                "by_required_mechanism",
                "by_role_state",
                "inventory_scope",
                "irreducible_unknowns",
                "non_blocking_unknowns",
                "recoverable_unknowns",
                "total_unknowns",
            ]
        );
        assert_eq!(
            value["unknown_inventory"]["by_language"][0]["language"],
            "python"
        );
        assert_eq!(
            value["unknown_inventory"]["by_required_mechanism"][0]["required_mechanism"],
            "fastapi_dependency_graph"
        );
        assert_eq!(
            value["unknown_inventory"]["by_role_state"][0]["role_state"],
            "single"
        );
        assert_eq!(
            value["unknown_inventory"]["by_blocks_support"][1]["blocks_support"],
            true
        );
        assert_eq!(
            value["unknown_inventory"]["by_recovery_code"][0]["recovery_code"],
            "not_implemented_in_current_version"
        );
        assert!(value["unknown_inventory"].get("by_recovery").is_none());
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains("app/routes.py"));
        assert!(!output.stdout.contains("src/"));
    }

    #[test]
    fn stats_unknowns_json_embeds_aggregate_inventory() {
        let workspace = TempWorkspace::new("cli-stats-unknowns-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let output = run_with_context_and_runtime(
            ["stats", "--unknowns", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("stats JSON");
        assert_eq!(value["command"], "stats");
        assert_eq!(value["unknown_inventory"]["total_unknowns"], 2);
        assert_eq!(
            value["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );
        assert_eq!(
            value["query_outcome_rollup"]["rollup_scope"],
            "local_query_outcomes"
        );
        assert_eq!(value["query_outcome_rollup"]["event_count"], 0);
        assert_eq!(
            value["unknown_inventory"]["by_framework_role"][0]["framework_role"],
            "framework:fastapi.route"
        );
        assert_eq!(
            value["unknown_inventory"]["by_recovery_code"][0]["recovery_code"],
            "not_implemented_in_current_version"
        );
        assert_eq!(
            value["unknown_inventory"]["by_recovery_code"][0]["count"],
            2
        );
        assert_eq!(value["by_language"][0]["blocking_unknowns"], 1);
        assert_eq!(
            value["by_language"][0]["top_required_mechanisms"][0]["required_mechanism"],
            "fastapi_dependency_graph"
        );
        assert_eq!(
            value["by_language"][0]["top_reason_codes"][0]["reason_code"],
            "RuntimeDependencyInjection"
        );
        assert_eq!(value["by_language"][0]["unknown_inventory_available"], true);
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(fallback["guidance"], "run repogrammar setup");
        assert_eq!(fallback["command"], "stats");
        assert_eq!(fallback["implemented"], true);
        assert_eq!(fallback["readiness_available"], false);
        assert_eq!(
            fallback["by_language"]
                .as_array()
                .expect("by_language")
                .len(),
            0
        );
        assert!(fallback.get("inventory_available").is_none());
    }

    #[test]
    fn stats_unknowns_json_reports_inventory_unavailable_without_active_index() {
        let workspace = TempWorkspace::new("cli-stats-unknowns-missing-index");
        let env = |_: &str| None;

        let output = run_with_context(["stats", "--unknowns", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("stats fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository is not initialized");
        assert_eq!(fallback["guidance"], "run repogrammar setup");
        assert_eq!(fallback["command"], "stats");
        assert_eq!(fallback["implemented"], true);
        assert_eq!(fallback["readiness_available"], false);
        assert_eq!(
            fallback["by_language"]
                .as_array()
                .expect("by_language")
                .len(),
            0
        );
        assert_eq!(fallback["inventory_available"], false);
    }

    #[test]
    fn unknowns_json_uses_fallback_without_active_index() {
        let workspace = TempWorkspace::new("cli-unknowns-missing-index");
        let env = |_: &str| None;

        let output = run_with_context(["unknowns", "--json"], workspace.path(), &env);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let fallback: Value =
            serde_json::from_str(output.stderr.trim()).expect("unknowns fallback must be JSON");
        assert_eq!(fallback["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(fallback["reason"], "repository is not initialized");
        assert_eq!(fallback["guidance"], "run repogrammar setup");
        assert_eq!(fallback["command"], "unknowns");
        assert_eq!(fallback["implemented"], true);
        assert_eq!(fallback["inventory_available"], false);
    }

    #[test]
    fn unknowns_and_stats_reject_unknown_options() {
        let unknowns = run(["unknowns", "--mystery"]);
        assert_eq!(unknowns.status, 2);
        assert!(unknowns
            .stderr
            .contains("unknown unknowns option: --mystery"));

        let stats = run(["stats", "--unknowns", "--mystery"]);
        assert_eq!(stats.status, 2);
        assert!(stats.stderr.contains("unknown stats option: --mystery"));
    }

    #[test]
    fn query_fallback_distinguishes_initialized_state_from_missing_state() {
        let workspace = TempWorkspace::new("cli-query-initialized-fallback");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        let rollup_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("telemetry")
            .join("local-metrics")
            .join("family_query_metrics.json");
        let rollup: Value =
            serde_json::from_str(&fs::read_to_string(rollup_path).expect("query rollup JSON"))
                .expect("query rollup");
        assert_eq!(rollup["total_queries"], 1);
        assert_eq!(rollup["savings_events"], 0);
        assert_eq!(rollup["by_status"]["unknown"], 1);
        assert_eq!(rollup["by_reason_code"]["InsufficientSupport"], 1);
        assert_eq!(
            rollup["by_required_mechanism"]["compatible_support_evidence"],
            1
        );
    }

    #[test]
    fn family_query_json_returns_partial_context_for_indexed_target_without_family() {
        let workspace = TempWorkspace::new("cli-query-partial-context");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["status"], "PARTIAL_CONTEXT");
        assert_eq!(value["query_route"]["route"], "partial_context_read_plan");
        assert_eq!(
            value["query_route"]["family_id_policy"],
            "family_ids_are_returned_follow_up_handles_not_required_initial_inputs"
        );
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
        // A PARTIAL_CONTEXT response now carries the ESTIMATED savings block with
        // its caveat verbatim (never omitted), attributed to the resolved file's
        // language and the partial_context outcome shape.
        let savings = &value["estimated_potential_token_savings"];
        assert_eq!(savings["outcome_shape"], "partial_context");
        assert_eq!(savings["language"], "typescript/javascript");
        assert!(savings["estimated_potential_token_savings"].is_number());
        assert_eq!(
            savings["estimated_potential_token_savings_kind"],
            "ESTIMATED"
        );
        assert!(savings["estimated_potential_token_savings_caveat"]
            .as_str()
            .expect("caveat")
            .contains("not measured token savings"));
        assert!(!output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!output.stdout.contains("export const"));
        let rollup_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("telemetry")
            .join("local-metrics")
            .join("family_query_metrics.json");
        let rollup: Value =
            serde_json::from_str(&fs::read_to_string(rollup_path).expect("query rollup JSON"))
                .expect("query rollup");
        assert_eq!(rollup["total_queries"], 1);
        assert_eq!(rollup["savings_events"], 1);
        assert_eq!(rollup["by_status"]["partial_context"], 1);
        assert_eq!(rollup["read_plan_returned_count"], 1);
        assert_eq!(rollup["read_plan_item_count_bucket"]["1-2"], 1);
        assert_eq!(rollup["by_reason_code"]["InsufficientSupport"], 1);
        // The all-scope savings rollup recorded this PARTIAL_CONTEXT as an event
        // under the partial_context outcome shape (proving partial-context read
        // plans now accrue savings accounting, not just found families).
        let savings_rollup = &rollup;
        assert_eq!(
            savings_rollup["by_outcome_shape"]["partial_context"]["event_count"],
            1
        );
        assert_eq!(
            savings_rollup["by_language"]["typescript/javascript"]["event_count"],
            1
        );
    }

    /// A directory-scope PARTIAL_CONTEXT report with a `many` resolution, built
    /// directly so the serializer contract can be asserted without a fixture repo.
    fn directory_scope_many_report() -> FamilyPartialContextReport {
        use crate::application::query_resolution::{
            Resolution, ResolutionCandidate, ResolutionCardinality,
        };
        let fastapi = "family:python:fastapi_route:framework_fastapi_route";
        let sqlalchemy = "family:python:sqlalchemy_model:framework_sqlalchemy_model";
        FamilyPartialContextReport {
            active_generation: "gen-000001".to_string(),
            resolved_target: ResolvedQueryTarget {
                original_target: "app/api".to_string(),
                kind: "directory_scope",
                path: "app/api".to_string(),
                line: None,
                byte_range: None,
                family_id: None,
                code_unit_id: None,
                symbol_hints: Vec::new(),
                residue_terms: Vec::new(),
                candidate_paths: vec![
                    "app/api/models.py".to_string(),
                    "app/api/routes.py".to_string(),
                ],
                candidate_family_ids: vec![fastapi.to_string(), sqlalchemy.to_string()],
                candidate_code_unit_ids: Vec::new(),
                confidence: "scope",
                match_kind: "directory_scope",
            },
            read_plan: ReadPlan {
                items: Vec::new(),
                estimated_tokens: 0,
                source_snippets_included: false,
                requires_source_before_edit: false,
                selection_strategy: "directory_scope_no_read_plan",
                budget_satisfied: true,
                truncated: false,
                line_range_omissions: Vec::new(),
            },
            resolved_file_size_bytes: None,
            resolved_file_language: String::new(),
            unknowns: vec![FamilyQueryUnknown {
                class: crate::core::model::UnknownClass::Blocking,
                reason: crate::core::model::UnknownReasonCode::InsufficientSupport,
                affected_claim: "pattern family evidence for resolved directory scope".to_string(),
                recovery: Some("pick one from resolution.candidates".to_string()),
            }],
            resolution: Some(Resolution {
                cardinality: ResolutionCardinality::Many,
                candidates: vec![
                    ResolutionCandidate {
                        family_id: fastapi.to_string(),
                        summary: "python fastapi.route · DOMINANT_PATTERN".to_string(),
                    },
                    ResolutionCandidate {
                        family_id: sqlalchemy.to_string(),
                        summary: "python sqlalchemy.model · DOMINANT_PATTERN".to_string(),
                    },
                ],
            }),
        }
    }

    #[test]
    fn cli_partial_context_resolution_many_never_selects_and_keeps_minimal_handles() {
        let fastapi = "family:python:fastapi_route:framework_fastapi_route";
        let sqlalchemy = "family:python:sqlalchemy_model:framework_sqlalchemy_model";
        let report = directory_scope_many_report();
        let route = family_query_route_report(
            &FamilyLookupReport::PartialContext(Box::new(report.clone())),
            FamilyLookupMode::FuzzyQuery,
        );
        let options = FamilyOutputOptions::default();

        // Standard: the additive `resolution` object carries `many`, both candidate
        // summaries, and never a `selected_family_id`.
        let value: Value = serde_json::from_str(&family_partial_context_json(
            "find", &report, &route, options, None, None,
        ))
        .expect("partial context json");
        assert_eq!(value["status"], "PARTIAL_CONTEXT");
        assert_eq!(value["resolution"]["cardinality"], "many");
        assert_eq!(
            value["resolution"]["candidates"]
                .as_array()
                .expect("candidates")
                .len(),
            2
        );
        assert_eq!(value["resolution"]["candidates"][0]["family_id"], fastapi);
        assert!(value["resolution"]["candidates"][0]["summary"].is_string());
        assert!(value["resolution"].get("selected_family_id").is_none());
        assert!(value["query_route"]["selected_family_id"].is_null());

        // Minimal: the rich object is dropped, but the narrowing handles survive on
        // `query_route.follow_up_family_ids`.
        let minimal = FamilyOutputOptions {
            verbosity: Verbosity::Minimal,
            ..options
        };
        let value_min: Value = serde_json::from_str(&family_partial_context_json(
            "find", &report, &route, minimal, None, None,
        ))
        .expect("minimal partial context json");
        assert!(value_min.get("resolution").is_none());
        let handles = value_min["query_route"]["follow_up_family_ids"]
            .as_array()
            .expect("follow-up handles");
        assert!(handles.iter().any(|handle| handle == fastapi));
        assert!(handles.iter().any(|handle| handle == sqlalchemy));
    }

    #[test]
    fn family_query_after_default_init_does_not_hit_no_active_generation_fallback() {
        let workspace = TempWorkspace::new("cli-query-after-default-init");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");

        let init =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(init.status, 0, "{init:?}");

        let output = run_with_context_and_runtime(
            ["find", "a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stderr.is_empty());
        let value: Value =
            serde_json::from_str(output.stdout.trim()).expect("partial context JSON");
        assert_eq!(value["status"], "PARTIAL_CONTEXT");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["resolved_target"]["path"], "a.ts");
    }

    #[test]
    fn family_check_json_partial_context_remains_advisory_without_proof_fields() {
        let workspace = TempWorkspace::new("cli-check-partial-context");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        let value: Value =
            serde_json::from_str(output.stdout.trim()).expect("check certificate JSON");
        // A target with no comparison family abstains with a static-alignment
        // certificate: INSUFFICIENT_EVIDENCE, no selected family, runtime
        // equivalence explicitly UNKNOWN, and never a legacy advisory block.
        assert_eq!(value["status"], "INSUFFICIENT_EVIDENCE");
        assert_eq!(value["alignment_status"], "INSUFFICIENT_EVIDENCE");
        assert_eq!(value["runtime_equivalence"], "UNKNOWN");
        assert!(value["selected_family_id"].is_null());
        assert!(value["alignment"].is_null());
        assert!(value.get("check").is_none());
        assert!(value["query_route"]["selected_family_id"].is_null());
    }

    /// The exact stdout JSON line (trailing newline appended in the assertion)
    /// the Found `find` response produced at commit 8337c1a, captured before any
    /// precision slice landed. Anchoring `standard`/`full` to this literal proves
    /// the query_route rewrite is byte-neutral above `minimal`.
    const FIND_FOUND_JSON_PREGOLDEN_V0: &str = r#"{"active_generation":"gen-000001","command":"find","constraint_profile":null,"evidence":[],"family":{"classification":"DOMINANT_PATTERN","family_id":"family:typescript:express_route:express","prevalence":{"blocked_peer_count":0,"classification_reason":"coverage 2/2 with no competing ready family","competing_ready_family_count":0,"coverage_ratio":1.0,"eligible_peer_count":2,"largest_competing_support":0,"supported_member_count":2,"unsupported_peer_count":0},"support":2},"implemented":true,"member_count":1,"members":[{"code_unit_id":"unit:src/routes/a.ts#express_route:get:0-20:1","family_id":"family:typescript:express_route:express","role":"framework:express.route_handler"}],"members_truncated":false,"output":{"budget_satisfied":true,"covered_claims":[],"estimated_baseline_tokens":135,"estimated_evidence_tokens":0,"estimated_potential_token_savings":65,"estimated_potential_token_savings_caveat":"estimated potential only; not measured token savings","estimated_potential_token_savings_kind":"ESTIMATED","estimated_read_plan_tokens":70,"estimated_returned_tokens":70,"missing_claims":[],"mode":"compact","selection_strategy":"greedy_marginal_coverage_v1","source_snippets_included":false,"token_budget":null},"query_route":{"candidate_family_ids":["family:typescript:express_route:express"],"candidate_limit":5,"family_id_policy":"family_ids_are_returned_follow_up_handles_not_required_initial_inputs","follow_up_family_ids":["family:typescript:express_route:express"],"hydrated_family_count":null,"input_kind":"path_symbol_role_or_pattern_target","pipeline":["discover_candidates","hydrate_bounded_candidates","select_single_fresh_family","compose_context_bundle"],"retrieval_stage_count":null,"route":"discover_hydrate_compose","selected_family_id":"family:typescript:express_route:express","term_retrieval":null,"why_selected":"target resolved to one fresh candidate family; RepoGrammar hydrated that family and composed bounded context"},"read_plan":{"budget_satisfied":true,"estimated_tokens":70,"items":[{"content_hash":"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","end_byte":20,"end_line":2,"estimated_tokens":70,"path":"src/routes/a.ts","purpose":"target_body_required_for_edit","source_required_before_edit":true,"source_snippets_included":false,"start_byte":0,"start_line":1,"why":"read this target body before editing; family metadata is context only"}],"line_range_omissions":[],"requires_source_before_edit":true,"selection_strategy":"deterministic_read_plan_v1","source_snippets_included":false},"schema_version":"product-schemas.v1","source_spans":{"omissions":[],"requested":false,"source_snippets_included":false,"spans":[]},"status":"ok","unknowns":[{"affected_claim":"runtime_equivalence","class":"non_blocking_unknown","reason":"FrameworkMagic","recovery":"add semantic-worker or framework adapter evidence"}],"variation_slots":[{"description":"non_blocking_unknown:FrameworkMagic:runtime equivalence remains unproven","family_id":"family:typescript:express_route:express","slot_id":"slot:runtime_unknown"}]}"#;

    #[test]
    fn find_standard_and_full_match_pregolden_byte_for_byte() {
        let workspace = TempWorkspace::new("cli-verbosity-byte-parity");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        let golden = format!("{}\n", FIND_FOUND_JSON_PREGOLDEN_V0);

        let default = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(default.status, 0);
        let default_value: Value =
            serde_json::from_str(default.stdout.trim()).expect("default family JSON");
        assert_eq!(default_value["status"], "ok");
        // v1 must not echo `verbosity` into the structured payload.
        assert!(default_value.get("verbosity").is_none());
        assert_eq!(
            default.stdout, golden,
            "default (implicit standard) stdout drifted from the pre-precision golden",
        );

        // v1 discipline: `standard` (the default) and `full` reproduce the
        // pre-precision stdout exactly. All minimal-tier reductions are opt-in;
        // see the dedicated minimal-shape tests below.
        for verbosity in ["standard", "full"] {
            let output = run_with_context_and_runtime(
                [
                    "find",
                    "src/routes/a.ts",
                    "--verbosity",
                    verbosity,
                    "--json",
                ],
                workspace.path(),
                &env,
                &runtime,
            );
            assert_eq!(output.status, 0);
            assert_eq!(
                output.stdout, golden,
                "--verbosity {verbosity} must match the pre-precision golden byte-for-byte",
            );
        }

        // `minimal` must genuinely diverge (a regression that silently made it
        // equal `standard` would mean the precision reductions stopped firing).
        let minimal = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--verbosity",
                "minimal",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(minimal.status, 0);
        assert_ne!(
            minimal.stdout, golden,
            "--verbosity minimal must diverge from the standard default output",
        );
    }

    #[test]
    fn find_minimal_adds_honest_truncation_flags_and_drops_source_spans_stub() {
        let workspace = TempWorkspace::new("cli-minimal-read-plan-shape");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let minimal = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--verbosity",
                "minimal",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(minimal.status, 0);
        let value: Value =
            serde_json::from_str(minimal.stdout.trim()).expect("minimal family JSON");
        assert_eq!(value["status"], "ok");
        // Honest truncation flag always present at minimal; item_count consistent
        // with items. The concrete `truncated: true` capping path is covered by
        // the domain-level `build_read_plan` truncation test.
        assert!(value["read_plan"]["truncated"].is_boolean());
        assert_eq!(
            value["read_plan"]["item_count"]
                .as_u64()
                .expect("item_count"),
            value["read_plan"]["items"].as_array().expect("items").len() as u64
        );
        // Empty source_spans stub omitted when spans are not requested.
        assert!(value.get("source_spans").is_none());

        // Standard keeps the pre-precision shape.
        let standard = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );
        let standard_value: Value =
            serde_json::from_str(standard.stdout.trim()).expect("standard family JSON");
        assert!(standard_value["read_plan"].get("truncated").is_none());
        assert!(standard_value["read_plan"].get("item_count").is_none());
        assert_eq!(standard_value["source_spans"]["requested"], false);
    }

    #[test]
    fn find_minimal_dedups_rendered_read_plan_items_into_source_spans() {
        let workspace = TempWorkspace::new("cli-minimal-read-plan-dedup");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;

        let minimal = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--include-source-spans",
                "--verbosity",
                "minimal",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(minimal.status, 0);
        let value: Value =
            serde_json::from_str(minimal.stdout.trim()).expect("minimal family JSON");
        // The rendered item collapses to a back-reference stub.
        let item = &value["read_plan"]["items"][0];
        assert_eq!(item["rendered"], true);
        assert!(item.get("path").is_some());
        assert!(item.get("purpose").is_some());
        assert!(item.get("content_hash").is_none());
        assert!(item.get("start_byte").is_none());
        // Content lives only under source_spans.
        assert_eq!(value["source_spans"]["source_snippets_included"], true);
        assert!(value["source_spans"]["spans"][0]["content_hash"].is_string());
        assert!(value["source_spans"]["spans"][0]["text"].is_string());

        // Standard keeps the full item metadata (no dedup).
        let standard = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--include-source-spans",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        let standard_value: Value =
            serde_json::from_str(standard.stdout.trim()).expect("standard family JSON");
        let standard_item = &standard_value["read_plan"]["items"][0];
        assert_eq!(standard_item["source_snippets_included"], true);
        assert!(standard_item["content_hash"].is_string());
        assert!(standard_item.get("rendered").is_none());
    }

    #[test]
    fn find_minimal_slims_query_route_only() {
        let workspace = TempWorkspace::new("cli-verbosity-minimal-shape");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let baseline = run_with_context_and_runtime(
            ["find", "src/routes/a.ts", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(baseline.status, 0);
        let baseline_value: Value =
            serde_json::from_str(baseline.stdout.trim()).expect("baseline family JSON");

        // `minimal` on a resolved Found route collapses `query_route` to the two
        // core fields while leaving the rest of the payload untouched.
        let minimal = run_with_context_and_runtime(
            [
                "find",
                "src/routes/a.ts",
                "--verbosity",
                "minimal",
                "--json",
            ],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(minimal.status, 0);
        assert_ne!(
            minimal.stdout, baseline.stdout,
            "--verbosity minimal must slim the Found query_route",
        );
        let minimal_value: Value =
            serde_json::from_str(minimal.stdout.trim()).expect("minimal family JSON");
        let route_keys: Vec<&String> = minimal_value["query_route"]
            .as_object()
            .expect("query_route object")
            .keys()
            .collect();
        assert_eq!(
            route_keys,
            vec!["follow_up_family_ids", "route"],
            "minimal Found query_route keeps only route + follow_up_family_ids",
        );
        assert_eq!(minimal_value["family"], baseline_value["family"]);
        assert_eq!(minimal_value["members"], baseline_value["members"]);
    }

    #[test]
    fn family_query_compact_mode_omits_evidence_without_source_leakage() {
        let workspace = TempWorkspace::new("cli-family-query-json");
        let env = |_: &str| None;
        let runtime = FamilyQueryRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["query_route"]["route"], "discover_hydrate_compose");
        assert!(value["query_route"]["pipeline"].is_array());
        assert!(value["query_route"]["candidate_limit"].is_number());
        assert!(value["query_route"]["candidate_family_ids"].is_array());
        assert_eq!(
            value["query_route"]["follow_up_family_ids"],
            json!(["family:typescript:express_route:express"])
        );
        assert_eq!(
            value["family"]["family_id"],
            "family:typescript:express_route:express"
        );
        // The detail surface exposes the metadata-only prevalence object.
        assert_eq!(
            value["family"]["prevalence"]["classification_reason"],
            "coverage 2/2 with no competing ready family"
        );
        assert_eq!(value["family"]["prevalence"]["eligible_peer_count"], 2);
        assert_eq!(value["family"]["prevalence"]["coverage_ratio"], 1.0);
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
        assert!(human.stdout.lines().count() <= 20);
        assert!(!human.stdout.contains("evidence:"));
        assert!(human.stdout.contains("pattern: TypeScript · Express"));
        assert!(human.stdout.contains("read: src/routes/a.ts (lines 1-2)"));
        assert!(human.stdout.contains("next:"));
        for internal in [
            "cluster_",
            "query_pipeline:",
            "query_candidate_family_ids:",
            "query_candidate_limit:",
            "query_follow_up_family_ids:",
        ] {
            assert!(!human.stdout.contains(internal), "leaked {internal}");
        }

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
            .join("family_query_metrics.json");
        let rollup: Value = serde_json::from_str(
            &fs::read_to_string(local_metric).expect("local estimated rollup"),
        )
        .expect("local estimated rollup JSON");
        assert_eq!(rollup["metric_name"], "family_query_metrics");
        assert_eq!(rollup["measurement_kind"], "ESTIMATED");
        assert_eq!(rollup["savings_events"], 3);
        assert_eq!(rollup["total_queries"], 3);
        assert!(
            rollup["total_estimated_potential_token_savings"]
                .as_u64()
                .expect("total estimated potential")
                > 0
        );
        let query_rollup = &rollup;
        assert_eq!(query_rollup["total_queries"], 3);
        assert_eq!(query_rollup["by_status"]["found"], 3);
        assert_eq!(query_rollup["by_entrypoint"]["cli"], 3);
        assert_eq!(query_rollup["by_command_category"]["find"], 3);
        assert_eq!(query_rollup["by_lookup_mode"]["fuzzy"], 3);
        assert_eq!(query_rollup["read_plan_returned_count"], 3);
        assert_eq!(query_rollup["read_plan_item_count_bucket"]["1-2"], 3);
        assert_eq!(query_rollup["by_reason_code"]["FrameworkMagic"], 3);
        assert_eq!(
            query_rollup["by_required_mechanism"]["framework_semantic_provider"],
            3
        );
        assert!(!query_rollup.to_string().contains("src/routes/a.ts"));
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
        assert_eq!(
            stats_value["query_outcome_rollup"]["event_count"],
            query_rollup["total_queries"]
        );
        assert_eq!(stats_value["query_outcome_rollup"]["by_status"]["found"], 3);
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

    // The static-alignment check JSON surface is covered end-to-end by
    // `family_check_json_partial_context_remains_advisory_without_proof_fields`
    // (abstaining certificate) and the binary integration tests
    // (STATICALLY_ALIGNED members). `check` never returns the legacy
    // `CONTEXT_ONLY` advisory, so the former advisory-shape test was removed.

    #[test]
    fn family_check_human_reports_static_alignment_certificate_not_advisory() {
        let workspace = TempWorkspace::new("cli-family-check-human");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["check", "a.ts"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        // The check leads with the static-alignment result and explicitly
        // separates static alignment from runtime conformance.
        assert!(output.stdout.contains("check: INSUFFICIENT_EVIDENCE"));
        assert!(output
            .stdout
            .contains("result: static alignment only; runtime conformance is NOT proven"));
        assert!(output.stdout.contains("runtime_equivalence: UNKNOWN"));
        // The legacy advisory vocabulary is gone.
        assert!(!output.stdout.contains("CONTEXT_ONLY"));
        assert!(!output.stdout.contains("advisory_status"));
        assert!(!output
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
    fn init_json_invokes_indexing_and_autosync_by_default() {
        let workspace = TempWorkspace::new("cli-init");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--json", "--write-gitignore"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "initialized");
        assert_eq!(value["state_dir"], DEFAULT_STATE_DIR);
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["resync"]["command"], "resync");
        assert_eq!(value["autosync"]["subcommand"], "start");
        assert_eq!(value["autosync"]["running"], true);
        assert_eq!(value["bootstrap"]["resync_requested"], true);
        assert_eq!(value["bootstrap"]["autosync_requested"], true);
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
        assert!(workspace.path().join(".gitignore").is_file());
    }

    #[test]
    fn init_state_only_json_preserves_lifecycle_only_behavior() {
        let workspace = TempWorkspace::new("cli-init-state-only");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--state-only", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "initialized");
        assert_eq!(value["storage"], "not_implemented");
        assert_eq!(value["indexing"], "not_implemented");
        assert!(value.get("bootstrap").is_none());
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
    }

    #[test]
    fn init_yes_indexes_without_broadening_gitignore_writes() {
        let workspace = TempWorkspace::new("cli-init-agent-safe-yes");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--no-autosync", "--json"],
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
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 0);
        assert_eq!(value["resync"]["command"], "resync");
        assert_eq!(value["autosync"], Value::Null);
    }

    #[test]
    fn init_resync_runs_index_and_reports_subresult() {
        let workspace = TempWorkspace::new("cli-init-resync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            [
                "init",
                "--yes",
                "--resync",
                "--no-autosync",
                "--progress",
                "never",
                "--json",
            ],
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
    fn init_no_autosync_builds_index_without_starting_daemon() {
        let workspace = TempWorkspace::new("cli-init-no-autosync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--no-autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["bootstrap"]["resync_requested"], true);
        assert_eq!(value["bootstrap"]["autosync_requested"], false);
        assert_eq!(value["autosync"], Value::Null);
    }

    #[test]
    fn init_rejects_conflicting_autosync_preferences_before_writes() {
        let workspace = TempWorkspace::new("cli-init-conflicting-autosync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        for args in [
            ["init", "--autosync", "--no-autosync", "--json"],
            ["init", "--no-autosync", "--autosync", "--json"],
        ] {
            let output = run_with_context_and_runtime(args, workspace.path(), &env, &runtime);
            assert_eq!(output.status, 2);
            assert!(output.stdout.is_empty());
            assert!(output
                .stderr
                .contains("--autosync and --no-autosync cannot be combined"));
        }

        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 0);
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
    }

    #[test]
    fn init_autosync_indexes_first_and_starts_autosync() {
        let workspace = TempWorkspace::new("cli-init-autosync-default-resync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert!(output.stderr.is_empty());
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["autosync"]["subcommand"], "start");
        assert_eq!(value["autosync"]["running"], true);
        assert_eq!(value["bootstrap"]["resync_requested"], true);
        assert_eq!(value["bootstrap"]["autosync_requested"], true);
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
    fn init_autosync_refreshes_existing_active_generation_before_start() {
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
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 1);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["autosync"]["running"], true);
    }

    #[test]
    fn init_state_only_autosync_fails_without_creating_state() {
        let workspace = TempWorkspace::new("cli-init-state-only-autosync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--state-only", "--autosync", "--json"],
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
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("--state-only cannot be combined with --autosync"));
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
    }

    #[test]
    fn init_state_only_accepts_explicit_no_autosync() {
        let workspace = TempWorkspace::new("cli-init-state-only-no-autosync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--state-only", "--no-autosync", "--json"],
            workspace.path(),
            &env,
            &runtime,
        );

        assert_eq!(output.status, 0, "{output:?}");
        assert_eq!(runtime.index_calls.get(), 0);
        assert_eq!(runtime.autosync_calls.get(), 0);
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
    }

    #[test]
    fn init_state_only_resync_fails_without_creating_state() {
        let workspace = TempWorkspace::new("cli-init-state-only-resync");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::default();

        let output = run_with_context_and_runtime(
            ["init", "--state-only", "--resync", "--json"],
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
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("--state-only cannot be combined with --resync"));
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
    }

    #[test]
    fn init_index_failure_preserves_state_and_guides_resync() {
        let workspace = TempWorkspace::new("cli-init-index-failure");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::fail_index();

        let output =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 0);
        assert!(workspace.path().join(DEFAULT_STATE_DIR).is_dir());
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("init error JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["status"], "error");
        assert_eq!(value["failed_step"], "resync");
        assert_eq!(
            value["guidance"],
            "fix the indexing issue, then run repogrammar resync"
        );
        assert_eq!(value["resync"], Value::Null);
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("synthetic resync failure"));
    }

    #[test]
    fn init_discovery_resource_limit_remains_a_resync_step_failure() {
        let workspace = TempWorkspace::new("cli-init-discovery-resource-limit");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::fail_index_with_resource_limit();

        let output =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert_eq!(runtime.index_calls.get(), 1);
        assert_eq!(runtime.autosync_calls.get(), 0);
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("init error JSON");
        assert_eq!(value["command"], "init");
        assert_eq!(value["failed_step"], "resync");
        assert_eq!(value["resync"], Value::Null);
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("resource=visited_entries"));
        assert_eq!(
            value["guidance"],
            "fix the indexing issue, then run repogrammar resync"
        );
    }

    #[test]
    fn init_preserves_resync_subresult_when_autosync_fails() {
        let workspace = TempWorkspace::new("cli-init-autosync-failure");
        let env = |_: &str| None;
        let runtime = BootstrapRuntime::fail_autosync();

        let output = run_with_context_and_runtime(
            ["init", "--yes", "--resync", "--json"],
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

        let output = run_with_context(
            ["init", "--state-only", "--progress", "always"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        assert!(output.stdout.contains("repository-local state ready"));
        assert!(output
            .stderr
            .contains("init: [####################] 100% 1/1 persistence_validation"));
        assert!(!output.stderr.to_ascii_lowercase().contains("eta"));
    }

    #[test]
    fn init_json_progress_always_keeps_result_on_stdout_and_human_bar_on_stderr() {
        let workspace = TempWorkspace::new("cli-init-progress-json");
        let env = |_: &str| None;

        let output = run_with_context(
            ["init", "--state-only", "--json", "--progress", "always"],
            workspace.path(),
            &env,
        );

        assert_eq!(output.status, 0);
        let result: Value = serde_json::from_str(output.stdout.trim()).expect("init JSON");
        assert_eq!(result["command"], "init");
        assert!(output
            .stderr
            .contains("init: [####################] 100% 1/1 persistence_validation"));
        assert!(serde_json::from_str::<Value>(output.stderr.trim()).is_err());
    }

    #[test]
    fn init_human_output_mentions_deferred_storage_without_claiming_indexing() {
        let workspace = TempWorkspace::new("cli-init-human");
        let env = |_: &str| None;

        let output = run_with_context(["init", "--state-only"], workspace.path(), &env);

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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["manifest_schema_version"], Value::Null);
        assert_eq!(value["storage_schema_version"], Value::Null);

        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
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

        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
    fn go_only_index_reports_file_manifest_mode_and_deferred_parser() {
        let workspace = TempWorkspace::new("cli-index-go-file-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("main.go"), "package demo\n").expect("write Go source");
        fs::write(
            workspace.path().join("go.mod"),
            "module example.test/demo\n",
        )
        .expect("write go.mod");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let json_output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(json_output.status, 0);
        assert!(json_output.stderr.is_empty());
        let value: Value = serde_json::from_str(json_output.stdout.trim()).expect("Go index JSON");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);

        let human_output =
            run_with_context_and_runtime(["resync"], workspace.path(), &env, &runtime);
        assert_eq!(human_output.status, 0);
        assert!(human_output.stderr.is_empty());
        assert!(human_output.stdout.contains("resync: file manifest stored"));
        assert!(human_output.stdout.contains("indexing: file_manifest_only"));
        assert!(human_output.stdout.contains("parser: deferred"));
        assert!(human_output.stdout.contains("parser_attempted_files: 0"));
        assert!(!human_output
            .stdout
            .contains("syntax-only code units stored"));
    }

    #[test]
    fn ruby_only_index_reports_file_manifest_metadata_without_claims() {
        let workspace = TempWorkspace::new("cli-index-ruby-file-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("main.rb"), [0xff, 0xfe, 0xfd])
            .expect("write binary Ruby source");
        fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://must-not-be-read.invalid'\n",
        )
        .expect("write Gemfile");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let json_output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(json_output.status, 0);
        assert!(json_output.stderr.is_empty());
        assert!(!json_output.stdout.contains("must-not-be-read"));
        let value: Value =
            serde_json::from_str(json_output.stdout.trim()).expect("Ruby index JSON");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);
        assert_eq!(
            value["warnings"],
            json!([
                "parser skipped unsupported language token: ruby",
                "parser skipped unsupported language token: ruby-config"
            ])
        );

        let files_output =
            run_with_context_and_runtime(["files", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(files_output.status, 0);
        assert!(files_output.stderr.is_empty());
        let files: Value =
            serde_json::from_str(files_output.stdout.trim()).expect("Ruby files JSON");
        assert_eq!(files["indexing"], "file_manifest_only");
        assert_eq!(
            files["files"]
                .as_array()
                .expect("Ruby files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("Ruby file path"),
                        file["language"].as_str().expect("Ruby file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("Gemfile", "ruby-config"), ("main.rb", "ruby")]
        );

        let units_output =
            run_with_context_and_runtime(["units", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(units_output.status, 0);
        assert!(units_output.stderr.is_empty());
        let units: Value =
            serde_json::from_str(units_output.stdout.trim()).expect("Ruby units JSON");
        assert_eq!(units["indexing"], "file_manifest_only");
        assert_eq!(units["units"], json!([]));

        let human_output =
            run_with_context_and_runtime(["resync"], workspace.path(), &env, &runtime);
        assert_eq!(human_output.status, 0);
        assert!(human_output.stderr.is_empty());
        assert!(human_output.stdout.contains("resync: file manifest stored"));
        assert!(human_output.stdout.contains("indexing: file_manifest_only"));
        assert!(human_output.stdout.contains("parser: deferred"));
        assert!(human_output.stdout.contains("parser_attempted_files: 0"));
        assert!(!human_output
            .stdout
            .contains("syntax-only code units stored"));
        assert!(!human_output.stdout.contains("must-not-be-read"));
        assert!(!human_output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn php_only_index_reports_file_manifest_metadata_without_claims() {
        let workspace = TempWorkspace::new("cli-index-php-file-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        let mut php_source = vec![0xff, 0xfe, 0xfd];
        php_source.extend_from_slice(b"php-source-must-not-be-read");
        fs::write(workspace.path().join("main.php"), php_source).expect("write binary PHP source");
        let mut composer_config = vec![0xff, 0xfe, 0xfd];
        composer_config.extend_from_slice(b"php-config-must-not-be-read");
        fs::write(workspace.path().join("composer.json"), composer_config)
            .expect("write binary Composer config");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let json_output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(json_output.status, 0);
        assert!(json_output.stderr.is_empty());
        assert!(!json_output.stdout.contains("php-source-must-not-be-read"));
        assert!(!json_output.stdout.contains("php-config-must-not-be-read"));
        let value: Value = serde_json::from_str(json_output.stdout.trim()).expect("PHP index JSON");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);
        assert_eq!(
            value["warnings"],
            json!([
                "parser skipped unsupported language token: php",
                "parser skipped unsupported language token: php-config"
            ])
        );

        let status_output =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status_output.status, 0);
        assert!(status_output.stderr.is_empty());
        assert!(!status_output.stdout.contains("php-source-must-not-be-read"));
        assert!(!status_output.stdout.contains("php-config-must-not-be-read"));
        let status: Value =
            serde_json::from_str(status_output.stdout.trim()).expect("PHP status JSON");
        assert_eq!(status["active_generation"], "gen-000001");
        assert_eq!(status["indexing"], "file_manifest_only");

        let files_output =
            run_with_context_and_runtime(["files", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(files_output.status, 0);
        assert!(files_output.stderr.is_empty());
        assert!(!files_output.stdout.contains("php-source-must-not-be-read"));
        assert!(!files_output.stdout.contains("php-config-must-not-be-read"));
        let files: Value =
            serde_json::from_str(files_output.stdout.trim()).expect("PHP files JSON");
        assert_eq!(files["indexing"], "file_manifest_only");
        assert_eq!(
            files["files"]
                .as_array()
                .expect("PHP files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("PHP file path"),
                        file["language"].as_str().expect("PHP file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("composer.json", "php-config"), ("main.php", "php")]
        );

        let units_output =
            run_with_context_and_runtime(["units", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(units_output.status, 0);
        assert!(units_output.stderr.is_empty());
        assert!(!units_output.stdout.contains("php-source-must-not-be-read"));
        assert!(!units_output.stdout.contains("php-config-must-not-be-read"));
        let units: Value =
            serde_json::from_str(units_output.stdout.trim()).expect("PHP units JSON");
        assert_eq!(units["indexing"], "file_manifest_only");
        assert_eq!(units["units"], json!([]));

        let human_output =
            run_with_context_and_runtime(["resync"], workspace.path(), &env, &runtime);
        assert_eq!(human_output.status, 0);
        assert!(human_output.stderr.is_empty());
        assert!(human_output.stdout.contains("resync: file manifest stored"));
        assert!(human_output.stdout.contains("indexing: file_manifest_only"));
        assert!(human_output.stdout.contains("parser: deferred"));
        assert!(human_output.stdout.contains("parser_attempted_files: 0"));
        assert!(human_output
            .stdout
            .contains("warning: parser skipped unsupported language token: php\n"));
        assert!(human_output
            .stdout
            .contains("warning: parser skipped unsupported language token: php-config\n"));
        assert!(!human_output
            .stdout
            .contains("syntax-only code units stored"));
        assert!(!human_output.stdout.contains("php-source-must-not-be-read"));
        assert!(!human_output.stdout.contains("php-config-must-not-be-read"));
        assert!(!human_output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn swift_only_index_reports_file_manifest_metadata_without_claims() {
        let workspace = TempWorkspace::new("cli-index-swift-file-manifest");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        let mut swift_source = vec![0xff, 0xfe, 0xfd];
        swift_source.extend_from_slice(b"swift-source-must-not-be-read");
        fs::write(workspace.path().join("main.swift"), swift_source)
            .expect("write binary Swift source");
        let mut swift_config = vec![0xff, 0xfe, 0xfd];
        swift_config.extend_from_slice(b"swift-config-must-not-be-read");
        fs::write(
            workspace.path().join("Package@swift-6.3.swift"),
            swift_config,
        )
        .expect("write binary Swift config");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let json_output = run_with_context_and_runtime(
            ["index", "--json", "--progress", "never"],
            workspace.path(),
            &env,
            &runtime,
        );
        assert_eq!(json_output.status, 0);
        assert!(json_output.stderr.is_empty());
        assert!(!json_output.stdout.contains("swift-source-must-not-be-read"));
        assert!(!json_output.stdout.contains("swift-config-must-not-be-read"));
        let value: Value =
            serde_json::from_str(json_output.stdout.trim()).expect("Swift index JSON");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);
        assert_eq!(
            value["warnings"],
            json!([
                "parser skipped unsupported language token: swift",
                "parser skipped unsupported language token: swift-config"
            ])
        );

        let status_output =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status_output.status, 0);
        assert!(status_output.stderr.is_empty());
        assert!(!status_output
            .stdout
            .contains("swift-source-must-not-be-read"));
        assert!(!status_output
            .stdout
            .contains("swift-config-must-not-be-read"));
        let status: Value =
            serde_json::from_str(status_output.stdout.trim()).expect("Swift status JSON");
        assert_eq!(status["active_generation"], "gen-000001");
        assert_eq!(status["indexing"], "file_manifest_only");

        let files_output =
            run_with_context_and_runtime(["files", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(files_output.status, 0);
        assert!(files_output.stderr.is_empty());
        assert!(!files_output
            .stdout
            .contains("swift-source-must-not-be-read"));
        assert!(!files_output
            .stdout
            .contains("swift-config-must-not-be-read"));
        let files: Value =
            serde_json::from_str(files_output.stdout.trim()).expect("Swift files JSON");
        assert_eq!(files["indexing"], "file_manifest_only");
        assert_eq!(
            files["files"]
                .as_array()
                .expect("Swift files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("Swift file path"),
                        file["language"].as_str().expect("Swift file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("Package@swift-6.3.swift", "swift-config"),
                ("main.swift", "swift")
            ]
        );

        let units_output =
            run_with_context_and_runtime(["units", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(units_output.status, 0);
        assert!(units_output.stderr.is_empty());
        assert!(!units_output
            .stdout
            .contains("swift-source-must-not-be-read"));
        assert!(!units_output
            .stdout
            .contains("swift-config-must-not-be-read"));
        let units: Value =
            serde_json::from_str(units_output.stdout.trim()).expect("Swift units JSON");
        assert_eq!(units["indexing"], "file_manifest_only");
        assert_eq!(units["units"], json!([]));

        let human_output =
            run_with_context_and_runtime(["resync"], workspace.path(), &env, &runtime);
        assert_eq!(human_output.status, 0);
        assert!(human_output.stderr.is_empty());
        assert!(human_output.stdout.contains("resync: file manifest stored"));
        assert!(human_output.stdout.contains("indexing: file_manifest_only"));
        assert!(human_output.stdout.contains("parser: deferred"));
        assert!(human_output.stdout.contains("parser_attempted_files: 0"));
        assert!(human_output
            .stdout
            .contains("warning: parser skipped unsupported language token: swift\n"));
        assert!(human_output
            .stdout
            .contains("warning: parser skipped unsupported language token: swift-config\n"));
        assert!(!human_output
            .stdout
            .contains("syntax-only code units stored"));
        assert!(!human_output
            .stdout
            .contains("swift-source-must-not-be-read"));
        assert!(!human_output
            .stdout
            .contains("swift-config-must-not-be-read"));
        assert!(!human_output
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
    fn sync_json_retains_active_generation_when_manifest_is_unchanged() {
        let workspace = TempWorkspace::new("cli-sync-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        fs::write(
            workspace.path().join("copied.ts"),
            "export const copied = true;\n",
        )
        .expect("write copied");
        fs::write(
            workspace.path().join("stable.ts"),
            "export const stable = true;\n",
        )
        .expect("write stable");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );

        let output =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("sync JSON");
        assert_eq!(value["command"], "sync");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["sync_mode"], "incremental");
        assert_eq!(value["fallback_reason"], Value::Null);
        assert_eq!(value["base_generation"], "gen-000001");
        assert_eq!(value["discovered_files"], 3);
        assert_eq!(value["stored_files"], 3);
        assert_eq!(value["added_files"], 0);
        assert_eq!(value["modified_files"], 0);
        assert_eq!(value["removed_files"], 0);
        assert_eq!(value["unchanged_files"], 3);
        assert_eq!(value["copied_forward_files"], 0);
        assert_eq!(value["reparsed_files"], 0);
        assert_eq!(value["dirty_records_cleared"], 0);
        assert_eq!(value["families_recomputed"], 0);
        // No generation comparison occurs on the zero-delta path, so family
        // identity deltas remain present and null.
        let sync_object = value.as_object().expect("sync JSON object");
        assert!(sync_object.contains_key("families_added"));
        assert!(sync_object.contains_key("families_removed"));
        assert_eq!(value["families_added"], Value::Null);
        assert_eq!(value["families_removed"], Value::Null);
        assert!(value["indexed_units"].as_u64().expect("indexed unit count") >= 3);
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
        assert_eq!(
            indexed_paths(&workspace, "gen-000001"),
            vec!["a.ts", "copied.ts", "stable.ts"]
        );
        assert_eq!(
            indexed_paths(&workspace, "gen-000002"),
            Vec::<String>::new()
        );

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
    }

    #[test]
    fn resync_json_rebuilds_static_analysis_for_non_rust_repository() {
        let workspace = TempWorkspace::new("cli-resync-real-runtime");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
    fn sync_json_falls_back_to_full_rebuild_for_project_context_change() {
        let workspace = TempWorkspace::new("cli-sync-context-fallback");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        fs::write(
            workspace.path().join("package.json"),
            r#"{"dependencies":{"typescript":"6.0.0"}}"#,
        )
        .expect("write package");

        let output =
            run_with_context_and_runtime(["sync", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(output.status, 0);
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_str(output.stdout.trim()).expect("sync JSON");
        assert_eq!(value["command"], "sync");
        assert_eq!(value["generation_id"], "gen-000002");
        assert_eq!(value["sync_mode"], "full_rebuild_fallback");
        assert_eq!(value["fallback_reason"], "project_context_changed");
        assert_eq!(value["base_generation"], "gen-000001");
        assert_eq!(value["added_files"], 1);
        assert_eq!(value["reparsed_files"], 2);
        assert_eq!(
            indexed_paths(&workspace, "gen-000002"),
            vec!["a.ts", "package.json"]
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
    fn status_and_doctor_json_report_readiness_when_not_initialized() {
        let workspace = TempWorkspace::new("cli-readiness-not-initialized");
        let env = |_: &str| None;
        let runtime = TestRuntime;

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["readiness"]["state"], "not_initialized");
        assert_eq!(value["readiness"]["query_ready"], false);
        assert_eq!(
            value["readiness"]["recommended_next_command"],
            "repogrammar setup"
        );
        assert_eq!(value["readiness"]["requires_user_permission"], true);
        assert_eq!(
            value["readiness"]["local_state_hygiene"]["repogrammar_state_present"],
            false
        );

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["readiness"]["state"], "not_initialized");
        assert_eq!(value["readiness"]["query_ready"], false);
        assert_eq!(
            value["readiness"]["recommended_next_command"],
            "repogrammar setup"
        );
    }

    #[test]
    fn status_and_doctor_report_no_active_generation_before_index() {
        let workspace = TempWorkspace::new("cli-storage-no-active");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["storage_layout"], "empty");
        assert_eq!(value["mutable_database_present"], false);
        assert_eq!(value["legacy_generation_layout_present"], false);
        assert_eq!(value["wal_bytes"], Value::Null);
        assert_eq!(value["shm_bytes"], Value::Null);
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "not_implemented");
        assert_eq!(value["readiness"]["state"], "state_only_no_active_index");
        assert_eq!(value["readiness"]["query_ready"], false);
        assert_eq!(
            value["readiness"]["recommended_next_command"],
            "repogrammar resync"
        );
        assert_eq!(
            value["readiness"]["local_state_hygiene"]["repogrammar_state_present"],
            true
        );

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["indexing"], "not_implemented");
        assert!(value["checks"].get("schema_version").is_none());
        assert_eq!(value["checks"]["manifest_schema_version"], 1);
        assert_eq!(value["checks"]["storage_schema_version"], Value::Null);
        assert_eq!(value["checks"]["storage_layout"], "empty");
        assert_eq!(value["checks"]["mutable_database_present"], false);
        assert_eq!(value["checks"]["legacy_generation_layout_present"], false);
        assert_eq!(value["checks"]["wal_bytes"], Value::Null);
        assert_eq!(value["checks"]["shm_bytes"], Value::Null);
        assert_eq!(value["readiness"]["state"], "state_only_no_active_index");
        assert_eq!(value["readiness"]["query_ready"], false);
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_NO_ACTIVE_GENERATION"));
    }

    #[test]
    fn status_and_doctor_json_report_query_ready_after_default_init() {
        let workspace = TempWorkspace::new("cli-readiness-active-index");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let init =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(init.status, 0);

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["readiness"]["state"], "ready_active_index");
        assert_eq!(value["readiness"]["query_ready"], true);
        assert_eq!(value["readiness"]["active_generation_available"], true);
        assert_eq!(value["readiness"]["recommended_next_command"], Value::Null);
        assert_eq!(value["readiness"]["autosync"]["configured"], false);
        assert_eq!(value["readiness"]["autosync"]["running"], false);
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["readiness"]["state"], "ready_active_index");
        assert_eq!(value["readiness"]["query_ready"], true);
        assert_eq!(value["readiness"]["active_generation_available"], true);
        assert!(!doctor
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn status_json_recommends_autosync_start_when_enabled_but_not_running() {
        let workspace = TempWorkspace::new("cli-readiness-autosync-recommended");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let init =
            run_with_context_and_runtime(["init", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(init.status, 0);
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("autosync.json"),
            r#"{"schema_version":1,"enabled":true}"#,
        )
        .expect("write autosync config");

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["readiness"]["state"], "autosync_recommended");
        assert_eq!(value["readiness"]["query_ready"], true);
        assert_eq!(
            value["readiness"]["recommended_next_command"],
            "repogrammar autosync start"
        );
        assert_eq!(value["readiness"]["autosync"]["configured"], true);
        assert_eq!(value["readiness"]["autosync"]["running"], false);
        assert_eq!(value["readiness"]["autosync"]["recommended"], true);
    }

    #[test]
    fn status_json_reports_local_state_hygiene_and_foreign_provider_state() {
        let workspace = TempWorkspace::new("cli-readiness-local-state-hygiene");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        if !git_init(&workspace) {
            return;
        }
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        fs::create_dir_all(workspace.path().join(".codegraph")).expect("create codegraph state");
        fs::write(
            workspace.path().join(".codegraph/index.db"),
            b"foreign state",
        )
        .expect("write codegraph state");
        let add_status = Command::new("git")
            .args([
                "add",
                "-f",
                ".repogrammar/.gitignore",
                ".codegraph/index.db",
            ])
            .current_dir(workspace.path())
            .status()
            .expect("git add local state");
        if !add_status.success() {
            return;
        }

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);

        assert_eq!(status.status, 0);
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        let hygiene = &value["readiness"]["local_state_hygiene"];
        assert_eq!(hygiene["repogrammar_state_present"], true);
        assert_eq!(hygiene["repogrammar_state_ignored"], true);
        assert_eq!(hygiene["repogrammar_state_tracked_risk"], true);
        assert!(hygiene["repogrammar_recommendation"]
            .as_str()
            .expect("repogrammar recommendation")
            .contains("remove .repogrammar/ from the Git index"));
        let foreign = hygiene["foreign_provider_state"]
            .as_array()
            .expect("foreign provider state");
        let codegraph = foreign
            .iter()
            .find(|provider| provider["name"] == "codegraph")
            .expect("codegraph provider");
        assert_eq!(codegraph["path"], ".codegraph/");
        assert_eq!(codegraph["present"], true);
        assert_eq!(codegraph["managed_by_repogrammar"], false);
        assert_eq!(codegraph["tracked_risk"], true);
    }

    #[test]
    fn doctor_json_reports_missing_subdir_and_storage_invalid() {
        let workspace = TempWorkspace::new("cli-doctor-missing-subdir-storage");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        let cache = workspace.path().join(DEFAULT_STATE_DIR).join("cache");
        fs::remove_dir_all(&cache).expect("remove cache");

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], Value::Null);
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], Value::Null);
        assert_eq!(value["storage_layout"], Value::Null);
        assert_eq!(value["mutable_database_present"], Value::Null);
        assert_eq!(value["legacy_generation_layout_present"], Value::Null);
        assert_eq!(value["wal_bytes"], Value::Null);
        assert_eq!(value["shm_bytes"], Value::Null);
        assert_eq!(value["journal_mode"], Value::Null);
        assert_eq!(value["integrity_check"], Value::Null);
        assert_eq!(value["storage"], "unhealthy");
        assert!(value["storage_error"]
            .as_str()
            .expect("storage error")
            .contains("cache"));
        assert_eq!(value["readiness"]["state"], "storage_unhealthy");
        assert_eq!(
            value["readiness"]["recommended_next_command"],
            "repogrammar doctor"
        );

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["required_subdirectories"], "fail");
        assert!(value["checks"].get("schema_version").is_none());
        assert_eq!(value["checks"]["manifest_schema_version"], 1);
        assert_eq!(value["checks"]["storage_schema_version"], Value::Null);
        assert_eq!(value["checks"]["storage_layout"], Value::Null);
        assert_eq!(value["checks"]["mutable_database_present"], Value::Null);
        assert_eq!(
            value["checks"]["legacy_generation_layout_present"],
            Value::Null
        );
        assert_eq!(value["checks"]["wal_bytes"], Value::Null);
        assert_eq!(value["checks"]["shm_bytes"], Value::Null);
        assert_eq!(value["checks"]["storage"], "unhealthy");
        assert_eq!(value["readiness"]["state"], "storage_unhealthy");
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
    fn doctor_json_reports_optional_provider_availability_source_free() {
        let workspace = TempWorkspace::new("cli-doctor-optional-providers");
        let unconfigured = |_: &str| None;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &unconfigured).status,
            0
        );

        // No provider configured: the integrated TypeScript slot is absent (not
        // configured) and the not-yet-integrated slots report not_integrated.
        let doctor = run_with_context(["doctor", "--json"], workspace.path(), &unconfigured);
        assert_eq!(doctor.status, 0);
        assert!(!doctor
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        let providers = value["optional_providers"]
            .as_array()
            .expect("optional_providers array");
        let availability = |id: &str| {
            providers
                .iter()
                .find(|slot| slot["id"] == id)
                .unwrap_or_else(|| panic!("provider {id} present"))["availability"]
                .as_str()
                .expect("availability string")
                .to_string()
        };
        assert_eq!(availability("typescript_compiler"), "not_configured");
        assert_eq!(availability("python_type_provider"), "not_integrated");
        assert_eq!(availability("rust_analyzer"), "not_integrated");
        // The report is a fixed, source-free vocabulary (repository path leakage
        // is already asserted above against the whole doctor payload).
        assert_eq!(providers.len(), 3);
        for slot in providers {
            assert!(slot["resolves_mechanisms"]
                .as_array()
                .is_some_and(|mechanisms| !mechanisms.is_empty()));
        }

        // Configuring the integrated TypeScript worker flips it to configured.
        let configured = |key: &str| {
            (key == "REPOGRAMMAR_TYPESCRIPT_WORKER").then(|| "/opt/ts-worker".to_string())
        };
        let doctor = run_with_context(["doctor", "--json"], workspace.path(), &configured);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        let configured_availability = value["optional_providers"]
            .as_array()
            .expect("optional_providers array")
            .iter()
            .find(|slot| slot["id"] == "typescript_compiler")
            .expect("typescript slot")["availability"]
            .as_str()
            .expect("availability string");
        assert_eq!(configured_availability, "configured");

        // When the bundled worker's runtime (node) is present on PATH but the
        // worker is not configured, the slot is enable-able here (available_bundled).
        let bin_dir = workspace.path().join("fake-bin");
        fs::create_dir_all(&bin_dir).expect("create fake bin dir");
        fs::write(bin_dir.join("node"), "#!/bin/sh\n").expect("write fake node");
        let bin_dir_str = bin_dir.to_string_lossy().to_string();
        let bundled = |key: &str| (key == "PATH").then(|| bin_dir_str.clone());
        let doctor = run_with_context(["doctor", "--json"], workspace.path(), &bundled);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        let bundled_availability = value["optional_providers"]
            .as_array()
            .expect("optional_providers array")
            .iter()
            .find(|slot| slot["id"] == "typescript_compiler")
            .expect("typescript slot")["availability"]
            .as_str()
            .expect("availability string");
        assert_eq!(bundled_availability, "available_bundled");
    }

    #[test]
    fn doctor_json_reports_active_index_lock() {
        let workspace = TempWorkspace::new("cli-doctor-index-lock");
        let env = |_: &str| None;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert!(human_status.stdout.contains("storage_layout: mutable"));
        assert!(human_status
            .stdout
            .contains("mutable_database_present: true"));
        assert!(human_status
            .stdout
            .contains("legacy_generation_layout_present: false"));
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
        assert_eq!(value["manifest_schema_version"], 1);
        assert_eq!(value["storage_schema_version"], STORAGE_SCHEMA_VERSION);
        assert_eq!(value["storage_layout"], "mutable");
        assert_eq!(value["mutable_database_present"], true);
        assert_eq!(value["legacy_generation_layout_present"], false);
        assert!(value["wal_bytes"].as_u64().is_some());
        assert!(value["shm_bytes"].as_u64().is_some());
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
        assert_eq!(value["checks"]["storage_layout"], "mutable");
        assert_eq!(value["checks"]["mutable_database_present"], true);
        assert_eq!(value["checks"]["legacy_generation_layout_present"], false);
        assert!(value["checks"]["wal_bytes"].as_u64().is_some());
        assert!(value["checks"]["shm_bytes"].as_u64().is_some());
        assert_eq!(value["checks"]["integrity_check"], "ok");
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "INDEXING_SYNTAX_ONLY_CODE_UNITS"));
    }

    fn move_mutable_database_to_legacy_layout(workspace: &TempWorkspace) {
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        let database = state.join("repogrammar.sqlite");
        let connection = Connection::open(&database).expect("open mutable database");
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, u32>(2)?,
                ))
            })
            .expect("checkpoint mutable database before legacy move");
        drop(connection);
        for sidecar in ["repogrammar.sqlite-wal", "repogrammar.sqlite-shm"] {
            let sidecar_path = state.join(sidecar);
            if sidecar_path.exists() {
                fs::remove_file(&sidecar_path).expect("remove mutable sidecar");
            }
        }
        let legacy_generation = state.join("generations").join("gen-000001");
        fs::create_dir_all(&legacy_generation).expect("create legacy generation directory");
        fs::rename(database, legacy_generation.join("repogrammar.sqlite"))
            .expect("move mutable database into legacy generation directory");
        fs::write(state.join("current-generation"), "gen-000001\n")
            .expect("write legacy current generation pointer");
    }

    #[test]
    fn status_and_doctor_report_mutable_with_legacy_layout() {
        let workspace = TempWorkspace::new("cli-storage-mutable-with-legacy");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.js"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        fs::create_dir_all(state.join("generations").join("gen-999999"))
            .expect("create legacy generation directory");
        fs::write(state.join("current-generation"), "gen-999999\n")
            .expect("write legacy current generation pointer");

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["storage_layout"], "mutable_with_legacy");
        assert_eq!(value["mutable_database_present"], true);
        assert_eq!(value["legacy_generation_layout_present"], true);
        assert!(value["wal_bytes"].as_u64().is_some());
        assert!(value["shm_bytes"].as_u64().is_some());

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["storage_layout"], "mutable_with_legacy");
        assert_eq!(value["checks"]["mutable_database_present"], true);
        assert_eq!(value["checks"]["legacy_generation_layout_present"], true);
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_MIXED_LAYOUT"));
        assert!(!value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_INVALID"));
    }

    #[test]
    fn status_and_doctor_report_legacy_layout_without_mutable_database() {
        let workspace = TempWorkspace::new("cli-storage-legacy-layout");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        fs::write(workspace.path().join("a.js"), "export const a = 1;\n").expect("write a");
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
        assert_eq!(
            run_with_context_and_runtime(["index"], workspace.path(), &env, &runtime).status,
            0
        );
        move_mutable_database_to_legacy_layout(&workspace);

        let status =
            run_with_context_and_runtime(["status", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["storage_layout"], "legacy");
        assert_eq!(value["mutable_database_present"], false);
        assert_eq!(value["legacy_generation_layout_present"], true);
        assert_eq!(value["wal_bytes"], Value::Null);
        assert_eq!(value["shm_bytes"], Value::Null);

        let doctor =
            run_with_context_and_runtime(["doctor", "--json"], workspace.path(), &env, &runtime);
        assert_eq!(doctor.status, 0);
        let value: Value = serde_json::from_str(doctor.stdout.trim()).expect("doctor JSON");
        assert_eq!(value["checks"]["storage"], "available");
        assert_eq!(value["checks"]["storage_layout"], "legacy");
        assert_eq!(value["checks"]["mutable_database_present"], false);
        assert_eq!(value["checks"]["legacy_generation_layout_present"], true);
        assert!(value["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|finding| finding["code"] == "STORAGE_LEGACY_LAYOUT"));
    }

    #[test]
    fn doctor_reports_legacy_broken_active_generation_pointer_without_panic() {
        let workspace = TempWorkspace::new("cli-storage-broken-pointer");
        let env = |_: &str| None;
        let runtime = TestRuntime;
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
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
        assert_eq!(value["schema_version"], PRODUCT_SCHEMA_VERSION);
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );

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

        let output = run_with_context(
            ["init", "--state-only", "--json"],
            workspace.path(),
            &safe_env,
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
            daemon_state: crate::application::autosync::AutosyncDaemonState::Running,
            pid: Some(42),
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: Some(crate::application::autosync::AutosyncRunReport {
                last_sync_unix_seconds: 1_700_000_000,
                result: crate::application::autosync::AutosyncRunResult::Ok,
                synced_generation: Some("gen-000007".to_string()),
                error: None,
            }),
            startup: crate::application::autosync::AutosyncStartupReport {
                state: crate::application::autosync::AutosyncStartupState::Ready,
                failure_code: None,
                previous_failure_code: Some(
                    crate::application::autosync::AutosyncStartupFailureCode::StartupTimeout,
                ),
            },
            repository_ready: true,
            message: "auto-sync status".to_string(),
        };
        let output = autosync_human(
            AutosyncCommand::Status,
            &report,
            &AutosyncOptions::default(),
        );
        assert!(
            output.contains("previous_autosync_attempt_result: ok"),
            "{output}"
        );
        assert!(
            output.contains("previous_autosync_attempt_generation: gen-000007"),
            "{output}"
        );
        assert!(!output.contains("last_sync_result"), "{output}");
        assert!(
            output.contains("previous_startup_failure_code: startup_timeout"),
            "{output}"
        );
    }

    #[test]
    fn autosync_json_separates_current_state_from_previous_attempt() {
        let report = AutosyncReport {
            state_dir: ".repogrammar".to_string(),
            enabled: true,
            running: false,
            daemon_state: crate::application::autosync::AutosyncDaemonState::Stopped,
            pid: None,
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: Some(crate::application::autosync::AutosyncRunReport {
                last_sync_unix_seconds: 1_700_000_000,
                result: crate::application::autosync::AutosyncRunResult::Error,
                synced_generation: None,
                error: Some("repository sync failed".to_string()),
            }),
            startup: crate::application::autosync::AutosyncStartupReport {
                state: crate::application::autosync::AutosyncStartupState::Failed,
                failure_code: Some(
                    crate::application::autosync::AutosyncStartupFailureCode::StartupTimeout,
                ),
                previous_failure_code: None,
            },
            repository_ready: true,
            message: "auto-sync status".to_string(),
        };

        let value = autosync_value(AutosyncCommand::Status, &report);
        assert_eq!(value["startup_state"], "failed");
        assert_eq!(value["startup_failure_code"], "startup_timeout");
        assert_eq!(value["previous_startup_failure_code"], Value::Null);
        assert_eq!(value["daemon_state"], "stopped");
        assert_eq!(value["repository_ready"], true);
        assert_eq!(value["previous_autosync_attempt"]["result"], "error");
        assert_eq!(value["last_run"]["result"], "error");
    }

    #[test]
    fn autosync_output_preserves_unknown_daemon_liveness() {
        let report = AutosyncReport {
            state_dir: ".repogrammar".to_string(),
            enabled: true,
            running: false,
            daemon_state: crate::application::autosync::AutosyncDaemonState::Unknown,
            pid: None,
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: None,
            startup: crate::application::autosync::AutosyncStartupReport {
                state: crate::application::autosync::AutosyncStartupState::Idle,
                failure_code: None,
                previous_failure_code: None,
            },
            repository_ready: true,
            message: "auto-sync status".to_string(),
        };

        let value = autosync_value(AutosyncCommand::Status, &report);
        assert_eq!(value["daemon_state"], "unknown");
        assert_eq!(value["startup_state"], "idle");
        let human = autosync_human(
            AutosyncCommand::Status,
            &report,
            &AutosyncOptions::default(),
        );
        assert!(human.contains("daemon_state: unknown"), "{human}");
        assert!(!human.contains("daemon_state: stopped"), "{human}");
    }

    #[test]
    fn autosync_output_sanitizes_untrusted_run_error_and_generation() {
        let sensitive = "/private/repository SECRET_SOURCE REPOGRAMMAR_TOKEN=credential-value";
        let invalid_generation = "gen-000007/../../private";
        let report = AutosyncReport {
            state_dir: ".repogrammar".to_string(),
            enabled: true,
            running: false,
            daemon_state: crate::application::autosync::AutosyncDaemonState::Stopped,
            pid: None,
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: Some(crate::application::autosync::AutosyncRunReport {
                last_sync_unix_seconds: 1_700_000_000,
                result: crate::application::autosync::AutosyncRunResult::Error,
                synced_generation: Some(invalid_generation.to_string()),
                error: Some(sensitive.to_string()),
            }),
            startup: crate::application::autosync::AutosyncStartupReport {
                state: crate::application::autosync::AutosyncStartupState::Idle,
                failure_code: None,
                previous_failure_code: None,
            },
            repository_ready: true,
            message: "auto-sync status".to_string(),
        };

        let human = autosync_human(
            AutosyncCommand::Status,
            &report,
            &AutosyncOptions::default(),
        );
        let value = autosync_value(AutosyncCommand::Status, &report);
        let rendered_json = value.to_string();

        for rendered in [&human, &rendered_json] {
            assert!(!rendered.contains(sensitive), "{rendered}");
            assert!(!rendered.contains(invalid_generation), "{rendered}");
            assert!(!rendered.contains("SECRET_SOURCE"), "{rendered}");
            assert!(!rendered.contains("REPOGRAMMAR_TOKEN"), "{rendered}");
            assert!(!rendered.contains("credential-value"), "{rendered}");
        }
        assert!(
            human.contains("previous_autosync_attempt_error: previous autosync attempt failed"),
            "{human}"
        );
        assert_eq!(
            value["previous_autosync_attempt"]["error"],
            "previous autosync attempt failed"
        );
        assert_eq!(
            value["previous_autosync_attempt"]["synced_generation"],
            Value::Null
        );
        assert_eq!(value["last_run"]["synced_generation"], Value::Null);
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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
        assert!(output.stdout.contains(
            "next: restart the coding-agent session; already-open Codex/Claude MCP child processes do not hot-swap RepoGrammar binaries or managed instructions"
        ));
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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
                    reconfigured_targets: Vec::new(),
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

        assert_eq!(
            run_with_context(["init", "--state-only"], workspace.path(), &env).status,
            0
        );
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
    fn experiment_start_without_mode_reports_missing_flag() {
        let workspace = TempWorkspace::new("cli-experiment-start-missing-mode");
        let env = |_: &str| None;
        let output = run_with_context(
            [
                "telemetry",
                "experiment-start",
                "--name",
                "task-a",
                "--session",
                "baseline",
                "--measurement-source",
                "user_entered",
            ],
            workspace.path(),
            &env,
        );
        assert_eq!(output.status, 2);
        // Required-field validation runs before the consent prompt, so a missing
        // mode reports the accurate error rather than "requires explicit
        // confirmation".
        assert!(
            output.stderr.contains("--experiment-mode is required"),
            "{}",
            output.stderr
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
        assert_eq!(value["guidance"], "run repogrammar setup");
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

    #[test]
    fn query_route_json_serializes_term_retrieval_metadata() {
        let route = FamilyQueryRouteReport {
            route: "discover_hydrate_compose",
            input_kind: "path_symbol_role_or_pattern_target",
            pipeline: vec!["discover_candidates", "hydrate_bounded_candidates"],
            family_id_policy:
                "family_ids_are_returned_follow_up_handles_not_required_initial_inputs",
            candidate_limit: Some(5),
            selected_family_id: Some(
                "family:python:fastapi_route:framework_fastapi_route".to_string(),
            ),
            candidate_family_ids: vec![
                "family:python:fastapi_route:framework_fastapi_route".to_string()
            ],
            follow_up_family_ids: vec![],
            why_selected: "target resolved to one fresh candidate family",
            term_retrieval: Some(TermRetrievalRoute {
                route: "term_retrieval_hydrate",
                retrieved_summary_count: 8,
                ranked_candidate_count: 1,
                hydrated_candidate_count: 1,
                retrieval_stage_count: 5,
                top_score: Some(10),
                margin: None,
                top_score_bucket: "at_or_above_min",
                margin_bucket: "none",
                matched_signals: Some(MatchedSignals {
                    framework_filter: true,
                    concept: true,
                    language_filter: false,
                    residue_hits: 0,
                }),
                truncated: false,
                abstention_reason: None,
            }),
        };
        let value = query_route_json(&route, Verbosity::Standard, VerbosityTier::Minimal);
        assert_eq!(value["hydrated_family_count"], 1);
        assert_eq!(value["retrieval_stage_count"], 5);
        let term = &value["term_retrieval"];
        assert_eq!(term["route"], "term_retrieval_hydrate");
        assert_eq!(term["abstention_reason"], serde_json::Value::Null);
        assert_eq!(term["top_score"], 10);
        assert_eq!(term["matched_signals"]["framework_filter"], true);
        assert_eq!(term["matched_signals"]["concept"], true);
    }

    #[test]
    fn query_route_json_omits_term_retrieval_for_exact_routes() {
        let route = FamilyQueryRouteReport {
            route: "exact_family_hydrate",
            input_kind: "family_id_follow_up_handle",
            pipeline: vec!["hydrate_exact_family", "compose_context_bundle"],
            family_id_policy: "show_family_requires_exact_family_id",
            candidate_limit: None,
            selected_family_id: Some("family:python:fastapi_route".to_string()),
            candidate_family_ids: vec!["family:python:fastapi_route".to_string()],
            follow_up_family_ids: vec![],
            why_selected: "exact family id was used as a follow-up handle",
            term_retrieval: None,
        };
        let value = query_route_json(&route, Verbosity::Standard, VerbosityTier::Standard);
        assert_eq!(value["hydrated_family_count"], serde_json::Value::Null);
        assert_eq!(value["retrieval_stage_count"], serde_json::Value::Null);
        assert_eq!(value["term_retrieval"], serde_json::Value::Null);
        // The abstention enum vocabulary is exhaustive and stable.
        assert_eq!(TermRetrievalAbstention::ALL.len(), 7);
    }

    #[test]
    fn query_route_json_minimal_slims_by_candidate_tier() {
        let route = FamilyQueryRouteReport {
            route: "discover_hydrate_compose",
            input_kind: "path_symbol_role_or_pattern_target",
            pipeline: vec!["discover_candidates", "hydrate_bounded_candidates"],
            family_id_policy:
                "family_ids_are_returned_follow_up_handles_not_required_initial_inputs",
            candidate_limit: Some(5),
            selected_family_id: Some("family:python:fastapi_route".to_string()),
            candidate_family_ids: vec!["family:python:fastapi_route".to_string()],
            follow_up_family_ids: vec!["family:python:fastapi_route".to_string()],
            why_selected: "target resolved to one fresh candidate family",
            term_retrieval: None,
        };
        const ALL_FIELDS: [&str; 12] = [
            "route",
            "input_kind",
            "pipeline",
            "family_id_policy",
            "candidate_limit",
            "selected_family_id",
            "candidate_family_ids",
            "follow_up_family_ids",
            "why_selected",
            "hydrated_family_count",
            "retrieval_stage_count",
            "term_retrieval",
        ];

        // v1 discipline: `standard` and `full` render every field regardless of
        // the candidate tier, so both stay byte-identical to the prior shape.
        for verbosity in [Verbosity::Standard, Verbosity::Full] {
            for tier in [VerbosityTier::Minimal, VerbosityTier::Standard] {
                let value = query_route_json(&route, verbosity, tier);
                let object = value.as_object().expect("query_route object");
                assert_eq!(object.len(), ALL_FIELDS.len());
                for key in ALL_FIELDS {
                    assert!(object.contains_key(key), "{verbosity:?} must render {key}");
                }
            }
        }

        // Minimal on a resolved Found route (candidate tier Standard): only the
        // two core fields survive.
        let found = query_route_json(&route, Verbosity::Minimal, VerbosityTier::Standard);
        let found_keys: Vec<&String> = found.as_object().unwrap().keys().collect();
        assert_eq!(found_keys, vec!["follow_up_family_ids", "route"]);

        // Minimal on a recovery route (candidate tier Minimal): the narrowing
        // candidate handle survives alongside the two core fields.
        let recovery = query_route_json(&route, Verbosity::Minimal, VerbosityTier::Minimal);
        let recovery_keys: Vec<&String> = recovery.as_object().unwrap().keys().collect();
        assert_eq!(
            recovery_keys,
            vec!["candidate_family_ids", "follow_up_family_ids", "route"],
        );
        assert!(found.get("selected_family_id").is_none());
        assert!(recovery.get("selected_family_id").is_none());
    }
}
