use repogrammar::adapters::filesystem::change_fingerprint::repository_change_fingerprint;
use repogrammar::adapters::filesystem::discovery::FilesystemFileDiscovery;
use repogrammar::adapters::filesystem::source_store::FilesystemSourceStore;
use repogrammar::adapters::frameworks::SyntaxFrameworkRoleDetector;
use repogrammar::adapters::parsing::RepoGrammarSourceParser;
use repogrammar::adapters::persistence::sqlite::SqliteIndexStore;
use repogrammar::adapters::semantic_workers::rust::CargoMetadataRustProvider;
use repogrammar::adapters::semantic_workers::typescript::TypeScriptSemanticWorkerBoundary;
use repogrammar::application::autosync::{
    acquire_autosync_daemon, autosync_status, classify_autosync_repository_status, daemon_log_path,
    disable_autosync, enable_autosync, inspect_autosync_startup, record_autosync_run,
    record_autosync_startup_failure, record_autosync_startup_ready, stop_autosync,
    AutosyncDaemonState, AutosyncReport, AutosyncRepositoryUnavailable, AutosyncRequest,
    AutosyncRunResult, AutosyncSettings, AutosyncStartupFailureCode, AutosyncStartupReadiness,
    AUTOSYNC_STARTUP_NONCE_ENV,
};
use repogrammar::application::indexing::{
    index_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress,
    index_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress,
    sync_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress,
    sync_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress,
    IndexingOutcome, IndexingRequest,
};
use repogrammar::application::install::{
    execute_install, execute_uninstall, inspect_agent_integration, AgentIntegrationInspection,
    AgentTarget, InstallExecutionContext, InstallExecutionOutcome, InstallRequest, InstallScope,
    McpSelfTestRunner, NativeAgentAction, NativeAgentConfigurator, NativeMcpServerConfig,
    NativeMcpServerState, MCP_SERVER_NAME,
};
use repogrammar::application::progress::ProgressEvent;
use repogrammar::application::query::{
    enrich_read_plan_line_ranges, list_code_units, list_families_with_freshness,
    list_indexed_files, lookup_family_with_freshness_and_local_context, render_source_spans,
    repo_shape_diagnostics, unknown_inventory, FamilyEvidenceFreshnessRequest, FamilyListReport,
    FamilyLookupMode, FamilyLookupReport, IndexedCodeUnitsReport, IndexedFilesReport, ReadPlan,
    RepoShapeDiagnosticsReport, SourceSpanRenderReport, SourceSpanRenderRequest,
    UnknownInventoryReport,
};
use repogrammar::application::repository::{
    repository_doctor_with_storage, repository_state_location, repository_status_with_storage,
    RepositoryDoctorReport, RepositoryDoctorRequest, RepositoryImplementationStatus,
    RepositoryStatus, RepositoryStatusReport, RepositoryStatusRequest,
};
use repogrammar::application::setup::SetupFailureClass;
use repogrammar::application::storage::{
    clean_index_storage, compact_index_storage, prune_index_generations,
};
use repogrammar::application::telemetry::TelemetryUploadReceipt;
use repogrammar::error::RepoGrammarError;
#[cfg(test)]
use repogrammar::interfaces::cli::run_with_runtime;
use repogrammar::interfaces::cli::{
    command_usage, parse_serve_options, render_index_progress_event, repository_root,
    run_with_runtime_and_install_prompt, semantic_worker_args_from_env_lookup,
    should_emit_progress, state_dir_override, AutosyncCommand, CliAutosyncRequest, CliIndexRequest,
    CliRuntime, InstallTelemetryPrompt, ProgressMode,
};
use repogrammar::interfaces::mcp::{
    serve_json_lines, McpReadOnlyRuntime, McpServeContext, McpToolName,
};
use repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use repogrammar::ports::index_store::{
    GenerationPruneReport, GenerationPruneRequest, IndexCompactReport, IndexCompactRequest,
    StorageCleanReport, StorageCleanRequest,
};
#[cfg(windows)]
use std::ffi::{c_void, OsString};
use std::fs;
use std::io::IsTerminal;
use std::io::Write;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

const AUTOSYNC_STARTUP_MAX_ATTEMPTS: usize = 100;
const AUTOSYNC_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(25);
static AUTOSYNC_STARTUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let runtime = ProductCliRuntime;
    if args.first().is_some_and(|command| command == "serve") {
        let status = run_serve_command(&args[1..], &runtime);
        std::process::exit(status);
    }
    let output =
        run_with_runtime_and_install_prompt(args, &runtime, &ProductInstallTelemetryPrompt);
    write_std_streams(&output.stdout, &output.stderr);
    std::process::exit(output.status);
}

/// Writes captured stdout/stderr, tolerating a broken pipe (e.g. piping to
/// `head`). The `print!`/`eprint!` macros panic on a write error, which would
/// exit 101 with an ugly message instead of the command's intended status.
fn write_std_streams(stdout_text: &str, stderr_text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(stdout_text.as_bytes());
    let _ = out.flush();
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(stderr_text.as_bytes());
    let _ = err.flush();
}

struct ProductCliRuntime;

struct ProductInstallTelemetryPrompt;

struct ProductProgressSink<'a> {
    command: &'a str,
    interactive: bool,
    last_width: usize,
}

impl<'a> ProductProgressSink<'a> {
    fn new(command: &'a str, interactive: bool) -> Self {
        Self {
            command,
            interactive,
            last_width: 0,
        }
    }

    fn emit(&mut self, event: &ProgressEvent) {
        if self.interactive {
            let (frame, width) =
                render_interactive_index_progress_event(self.command, event, self.last_width);
            eprint!("{frame}");
            self.last_width = width;
        } else {
            eprint!("{}", render_index_progress_event(self.command, event));
        }
        let _ = std::io::stderr().flush();
    }

    fn finish(&mut self) {
        if self.interactive && self.last_width > 0 {
            eprintln!();
            let _ = std::io::stderr().flush();
            self.last_width = 0;
        }
    }
}

fn render_interactive_index_progress_event(
    command: &str,
    event: &ProgressEvent,
    previous_width: usize,
) -> (String, usize) {
    let line = render_index_progress_event(command, event)
        .trim_end_matches('\n')
        .to_string();
    let width = line.chars().count();
    let mut frame = format!("\r{line}");
    let padding = previous_width.saturating_sub(width);
    if padding > 0 {
        frame.push_str(&" ".repeat(padding));
    }
    (frame, width)
}

impl InstallTelemetryPrompt for ProductInstallTelemetryPrompt {
    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    }

    fn prompt_agent_selection(&self, prompt: &str) -> Result<String, String> {
        read_prompt_response(prompt)
    }

    fn prompt_install_telemetry_consent(&self, prompt: &str) -> Result<String, String> {
        read_prompt_response(prompt)
    }

    fn prompt_install_confirmation(&self, prompt: &str) -> Result<String, String> {
        read_prompt_response(prompt)
    }

    fn prompt_experiment_consent(&self, prompt: &str) -> Result<String, String> {
        read_prompt_response(prompt)
    }
}

fn read_prompt_response(prompt: &str) -> Result<String, String> {
    eprint!("{prompt}");
    std::io::stderr()
        .flush()
        .map_err(|error| format!("failed to write prompt: {error}"))?;
    let mut response = String::new();
    std::io::stdin()
        .read_line(&mut response)
        .map_err(|error| format!("failed to read prompt: {error}"))?;
    Ok(response)
}

/// One-line auto-sync summary written to `.repogrammar/logs/daemon.log` after a
/// successful sync. Records files seen, code units indexed, elapsed time, and the
/// activated generation so the daemon log shows what each sync did.
fn format_autosync_sync_log(outcome: &IndexingOutcome, elapsed_ms: u128) -> String {
    if let Some(sync_report) = &outcome.sync_report {
        return format!(
            "autosync: {} sync +{} ~{} -{} unchanged {} copied {} reparsed {} file(s), {} unit(s) in {}ms (generation {})",
            sync_report.sync_mode.as_str(),
            sync_report.added_files,
            sync_report.modified_files,
            sync_report.removed_files,
            sync_report.unchanged_files,
            sync_report.copied_forward_files,
            sync_report.reparsed_files,
            outcome.indexed_units,
            elapsed_ms,
            outcome.active_generation.as_deref().unwrap_or("none")
        );
    }
    format!(
        "autosync: synced {} file(s), {} unit(s) in {}ms (generation {})",
        outcome.discovered_files,
        outcome.indexed_units,
        elapsed_ms,
        outcome.active_generation.as_deref().unwrap_or("none")
    )
}

fn append_autosync_daemon_log(path: Option<&Path>, line: &str) {
    let Some(path) = path else {
        return;
    };
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn autosync_semantic_worker_environment_from_lookup<F>(
    env_lookup: &F,
) -> Result<(Option<String>, Vec<String>), RepoGrammarError>
where
    F: Fn(&str) -> Option<String>,
{
    let executable =
        env_lookup("REPOGRAMMAR_TYPESCRIPT_WORKER").filter(|value| !value.trim().is_empty());
    let args =
        semantic_worker_args_from_env_lookup(env_lookup).map_err(RepoGrammarError::InvalidInput)?;
    if executable.is_none() && !args.is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON requires REPOGRAMMAR_TYPESCRIPT_WORKER"
                .to_string(),
        ));
    }
    Ok((executable, args))
}

#[derive(Debug, Default)]
struct AutosyncFailureLogState {
    last_error: Option<String>,
    failed_attempts: u64,
}

impl AutosyncFailureLogState {
    fn failure_lines(&mut self, error: &str, elapsed_ms: u128) -> Vec<String> {
        if self.last_error.as_deref() == Some(error) {
            self.failed_attempts = self.failed_attempts.saturating_add(1);
            if self.failed_attempts.is_power_of_two() {
                return vec![format!(
                    "autosync: sync still failing after {} attempts: {error}",
                    self.failed_attempts
                )];
            }
            return Vec::new();
        }

        let mut lines = self.changed_error_lines();
        self.last_error = Some(error.to_string());
        self.failed_attempts = 1;
        lines.push(format!(
            "autosync: sync failed after {elapsed_ms}ms: {error}"
        ));
        lines
    }

    fn success_lines(&mut self) -> Vec<String> {
        self.summary_lines()
    }

    fn changed_error_lines(&mut self) -> Vec<String> {
        let Some(error) = self.last_error.take() else {
            return Vec::new();
        };
        let attempts = self.failed_attempts;
        self.failed_attempts = 0;
        if attempts <= 1 {
            return Vec::new();
        }
        vec![format!(
            "autosync: previous sync error reached {attempts} failed attempts before a different error: {error}"
        )]
    }

    fn terminal_lines(&mut self) -> Vec<String> {
        let Some(error) = self.last_error.take() else {
            return Vec::new();
        };
        let attempts = self.failed_attempts;
        self.failed_attempts = 0;
        if attempts <= 1 {
            return Vec::new();
        }
        vec![format!(
            "autosync: previous sync error reached {attempts} failed attempts before stop: {error}"
        )]
    }

    fn summary_lines(&mut self) -> Vec<String> {
        let Some(error) = self.last_error.take() else {
            return Vec::new();
        };
        let attempts = self.failed_attempts;
        self.failed_attempts = 0;
        if attempts <= 1 {
            return Vec::new();
        }
        vec![format!(
            "autosync: recovered after {attempts} failed attempts: {error}"
        )]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutosyncStartupFailure {
    Classified(AutosyncStartupFailureCode),
    VerificationUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutosyncRecordedFailure {
    FingerprintFailed,
    StateUnavailable,
    SyncFailed,
}

impl AutosyncRecordedFailure {
    fn as_str(self) -> &'static str {
        match self {
            Self::FingerprintFailed => "repository fingerprint failed",
            Self::StateUnavailable => "repository state is unavailable",
            Self::SyncFailed => "repository sync failed",
        }
    }
}

fn record_autosync_runtime_failure(
    request: &AutosyncRequest,
    log_path: &Path,
    failure_log: &mut AutosyncFailureLogState,
    failure: AutosyncRecordedFailure,
    elapsed_ms: u128,
    quiet: bool,
) {
    let message = failure.as_str();
    for line in failure_log.failure_lines(message, elapsed_ms) {
        append_autosync_daemon_log(Some(log_path), &line);
        if !quiet {
            eprintln!("{line}");
        }
    }
    let _ = record_autosync_run(request, AutosyncRunResult::Error, None, Some(message));
}

fn record_autosync_runtime_recovery(
    log_path: &Path,
    failure_log: &mut AutosyncFailureLogState,
    quiet: bool,
) {
    for line in failure_log.success_lines() {
        append_autosync_daemon_log(Some(log_path), &line);
        if !quiet {
            eprintln!("{line}");
        }
    }
}

impl AutosyncStartupFailure {
    fn code(self) -> Option<AutosyncStartupFailureCode> {
        match self {
            Self::Classified(code) => Some(code),
            Self::VerificationUnavailable => None,
        }
    }

    fn into_error(self) -> RepoGrammarError {
        let message = match self {
            Self::Classified(AutosyncStartupFailureCode::WorkerEnvironmentInvalid) => {
                "auto-sync worker environment is invalid; correct the worker configuration and retry"
            }
            Self::Classified(AutosyncStartupFailureCode::RepositoryFingerprintFailed) => {
                "auto-sync could not compute the initial repository fingerprint"
            }
            Self::Classified(AutosyncStartupFailureCode::RepositoryStateUnavailable) => {
                "auto-sync repository state is unavailable; run repogrammar doctor"
            }
            Self::Classified(AutosyncStartupFailureCode::DaemonLockRefused) => {
                "auto-sync could not acquire daemon ownership"
            }
            Self::Classified(AutosyncStartupFailureCode::ChildExitedBeforeReady) => {
                "auto-sync exited before startup readiness was confirmed"
            }
            Self::Classified(AutosyncStartupFailureCode::StartupTimeout) => {
                "auto-sync startup readiness timed out"
            }
            Self::Classified(AutosyncStartupFailureCode::FirstHeartbeatFailed) => {
                "auto-sync failed its first repository heartbeat"
            }
            Self::VerificationUnavailable => "failed to verify auto-sync startup readiness",
        };
        RepoGrammarError::InvalidInput(message.to_string())
    }
}

fn wait_for_autosync_startup<P, S>(
    mut observe: P,
    mut pause: S,
    max_attempts: usize,
) -> Result<AutosyncReport, AutosyncStartupFailure>
where
    P: FnMut() -> Result<(AutosyncStartupReadiness, bool), RepoGrammarError>,
    S: FnMut(),
{
    for attempt in 0..max_attempts {
        let (readiness, child_exited) =
            observe().map_err(|_| AutosyncStartupFailure::VerificationUnavailable)?;
        match readiness {
            AutosyncStartupReadiness::Ready(report) => {
                if child_exited {
                    return Err(AutosyncStartupFailure::Classified(
                        AutosyncStartupFailureCode::ChildExitedBeforeReady,
                    ));
                }
                return Ok(report);
            }
            AutosyncStartupReadiness::Failed(code) => {
                return Err(AutosyncStartupFailure::Classified(code));
            }
            AutosyncStartupReadiness::LockRefused => {
                return Err(AutosyncStartupFailure::Classified(
                    AutosyncStartupFailureCode::DaemonLockRefused,
                ));
            }
            AutosyncStartupReadiness::Pending => {}
        }
        if child_exited {
            return Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::ChildExitedBeforeReady,
            ));
        }
        if attempt + 1 < max_attempts {
            pause();
        }
    }
    Err(AutosyncStartupFailure::Classified(
        AutosyncStartupFailureCode::StartupTimeout,
    ))
}

fn initialize_autosync_service<W, R, E, F, L, O, S, H>(
    repository_validation: R,
    worker_validation: E,
    initial_fingerprint: F,
    daemon_log_initialization: L,
    starting_owner_validation: O,
    heartbeat_repository_validation: S,
    heartbeat_fingerprint: H,
) -> Result<(W, String, std::path::PathBuf), AutosyncStartupFailureCode>
where
    R: FnOnce() -> Result<(), ()>,
    E: FnOnce() -> Result<W, ()>,
    F: FnOnce() -> Result<String, ()>,
    L: FnOnce() -> Result<std::path::PathBuf, ()>,
    O: FnOnce() -> Result<(), ()>,
    S: FnOnce() -> Result<(), ()>,
    H: FnOnce() -> Result<String, ()>,
{
    repository_validation().map_err(|()| AutosyncStartupFailureCode::RepositoryStateUnavailable)?;
    let worker =
        worker_validation().map_err(|()| AutosyncStartupFailureCode::WorkerEnvironmentInvalid)?;
    let _initial_fingerprint = initial_fingerprint()
        .map_err(|()| AutosyncStartupFailureCode::RepositoryFingerprintFailed)?;
    let log_path = daemon_log_initialization()
        .map_err(|()| AutosyncStartupFailureCode::RepositoryStateUnavailable)?;
    starting_owner_validation().map_err(|()| AutosyncStartupFailureCode::FirstHeartbeatFailed)?;
    heartbeat_repository_validation()
        .map_err(|()| AutosyncStartupFailureCode::FirstHeartbeatFailed)?;
    let fingerprint =
        heartbeat_fingerprint().map_err(|()| AutosyncStartupFailureCode::FirstHeartbeatFailed)?;
    Ok((worker, fingerprint, log_path))
}

fn initialize_autosync_daemon_log(path: std::path::PathBuf) -> Result<std::path::PathBuf, ()> {
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map(|_| path)
        .map_err(|_| ())
}

fn startup_nonce_from_process() -> Option<String> {
    std::env::var(AUTOSYNC_STARTUP_NONCE_ENV)
        .ok()
        .filter(|value| !value.is_empty())
}

fn persist_startup_failure(
    request: &AutosyncRequest,
    startup_nonce: Option<&str>,
    code: AutosyncStartupFailureCode,
) {
    if let Some(startup_nonce) = startup_nonce {
        let _ = record_autosync_startup_failure(request, startup_nonce, code);
    }
}

impl ProductCliRuntime {
    fn store_for_status_request(
        &self,
        request: &RepositoryStatusRequest,
    ) -> Result<SqliteIndexStore, RepoGrammarError> {
        let location = repository_state_location(request.clone())?;
        Ok(SqliteIndexStore::new(location.state_dir))
    }

    fn autosync_request(&self, request: &CliAutosyncRequest) -> AutosyncRequest {
        AutosyncRequest {
            path: request.repository_root.clone(),
            state_dir_override: request.state_dir_override.clone(),
        }
    }

    fn autosync_settings(&self, request: &CliAutosyncRequest) -> AutosyncSettings {
        AutosyncSettings {
            poll_ms: request.poll_ms,
            debounce_ms: request.debounce_ms,
        }
    }

    fn autosync_terminal_repository_state(
        &self,
        request: &CliAutosyncRequest,
    ) -> Result<Option<AutosyncRepositoryUnavailable>, RepoGrammarError> {
        let status_request = RepositoryStatusRequest {
            path: request.repository_root.clone(),
            state_dir_override: request.state_dir_override.clone(),
        };
        let store = self.store_for_status_request(&status_request)?;
        let status = repository_status_with_storage(status_request, &store)?;
        Ok(classify_autosync_repository_status(&status))
    }

    fn repository_fingerprint(
        &self,
        request: &CliAutosyncRequest,
    ) -> Result<String, RepoGrammarError> {
        repository_change_fingerprint(&request.repository_root, DEFAULT_MAX_FILE_BYTES)
            .map_err(|error| RepoGrammarError::InvalidInput(error.to_string()))
    }

    fn run_autosync_loop(
        &self,
        request: CliAutosyncRequest,
    ) -> Result<AutosyncReport, RepoGrammarError> {
        let autosync_request = self.autosync_request(&request);
        let initial_report = autosync_status(autosync_request.clone())?;
        let (mut guard, settings, root) = match acquire_autosync_daemon(autosync_request.clone()) {
            Ok(acquired) => acquired,
            Err(error) => {
                persist_startup_failure(
                    &autosync_request,
                    startup_nonce_from_process().as_deref(),
                    AutosyncStartupFailureCode::DaemonLockRefused,
                );
                return Err(error);
            }
        };
        let startup_nonce = startup_nonce_from_process();
        let env_lookup = |key: &str| std::env::var(key).ok();
        let initialized = initialize_autosync_service(
            || match self.autosync_terminal_repository_state(&request) {
                Ok(None) => Ok(()),
                Ok(Some(_)) | Err(_) => Err(()),
            },
            || autosync_semantic_worker_environment_from_lookup(&env_lookup).map_err(|_| ()),
            || self.repository_fingerprint(&request).map_err(|_| ()),
            || {
                daemon_log_path(&autosync_request)
                    .map_err(|_| ())
                    .and_then(initialize_autosync_daemon_log)
            },
            || guard.verify_starting_owner().map_err(|_| ()),
            || match self.autosync_terminal_repository_state(&request) {
                Ok(None) => Ok(()),
                Ok(Some(_)) | Err(_) => Err(()),
            },
            || self.repository_fingerprint(&request).map_err(|_| ()),
        );
        let ((semantic_worker_executable, semantic_worker_args), mut current, log_path) =
            match initialized {
                Ok(initialized) => initialized,
                Err(code) => {
                    persist_startup_failure(&autosync_request, startup_nonce.as_deref(), code);
                    return Err(AutosyncStartupFailure::Classified(code).into_error());
                }
            };
        if let Some(startup_nonce) = startup_nonce.as_deref() {
            if record_autosync_startup_ready(&autosync_request, startup_nonce).is_err() {
                let code = AutosyncStartupFailureCode::RepositoryStateUnavailable;
                persist_startup_failure(&autosync_request, Some(startup_nonce), code);
                return Err(AutosyncStartupFailure::Classified(code).into_error());
            }
        }
        if guard.publish_ready().is_err() {
            let code = AutosyncStartupFailureCode::DaemonLockRefused;
            persist_startup_failure(&autosync_request, startup_nonce.as_deref(), code);
            return Err(AutosyncStartupFailure::Classified(code).into_error());
        }
        if !request.quiet {
            eprintln!("autosync: watching repository for changes");
        }
        let mut failure_log = AutosyncFailureLogState::default();
        loop {
            std::thread::sleep(Duration::from_millis(settings.poll_ms));
            match autosync_status(autosync_request.clone()) {
                Ok(status) if status.enabled => {}
                Ok(status) => {
                    for line in failure_log.terminal_lines() {
                        append_autosync_daemon_log(Some(&log_path), &line);
                        if !request.quiet {
                            eprintln!("{line}");
                        }
                    }
                    let line = "autosync: stopping because auto-sync is disabled".to_string();
                    append_autosync_daemon_log(Some(&log_path), &line);
                    if !request.quiet {
                        eprintln!("{line}");
                    }
                    return Ok(AutosyncReport {
                        running: false,
                        pid: None,
                        message: "auto-sync stopped because it is disabled".to_string(),
                        ..status
                    });
                }
                Err(_error) if self.autosync_terminal_repository_state(&request)?.is_some() => {
                    let message = AutosyncRecordedFailure::StateUnavailable.as_str();
                    let _ = record_autosync_run(
                        &autosync_request,
                        AutosyncRunResult::Error,
                        None,
                        Some(message),
                    );
                    for line in failure_log.terminal_lines() {
                        append_autosync_daemon_log(Some(&log_path), &line);
                        if !request.quiet {
                            eprintln!("{line}");
                        }
                    }
                    let line =
                        "autosync: stopping because repository state is unavailable".to_string();
                    append_autosync_daemon_log(Some(&log_path), &line);
                    if !request.quiet {
                        eprintln!("{line}");
                    }
                    return Ok(AutosyncReport {
                        state_dir: initial_report.state_dir.clone(),
                        enabled: initial_report.enabled,
                        running: false,
                        daemon_state: AutosyncDaemonState::Stopped,
                        pid: None,
                        poll_ms: settings.poll_ms,
                        debounce_ms: settings.debounce_ms,
                        last_run: initial_report.last_run.clone(),
                        startup: initial_report.startup.clone(),
                        repository_ready: false,
                        message: "auto-sync stopped because repository state is unavailable"
                            .to_string(),
                    });
                }
                Err(_) => {
                    return Err(RepoGrammarError::InvalidInput(
                        "auto-sync status check failed".to_string(),
                    ));
                }
            }
            let fingerprint_started = Instant::now();
            let next = match self.repository_fingerprint(&request) {
                Ok(next) => {
                    record_autosync_runtime_recovery(&log_path, &mut failure_log, request.quiet);
                    next
                }
                Err(_) => {
                    record_autosync_runtime_failure(
                        &autosync_request,
                        &log_path,
                        &mut failure_log,
                        AutosyncRecordedFailure::FingerprintFailed,
                        fingerprint_started.elapsed().as_millis(),
                        request.quiet,
                    );
                    continue;
                }
            };
            if next == current {
                continue;
            }
            std::thread::sleep(Duration::from_millis(settings.debounce_ms));
            let fingerprint_started = Instant::now();
            let stable = match self.repository_fingerprint(&request) {
                Ok(stable) => stable,
                Err(_) => {
                    record_autosync_runtime_failure(
                        &autosync_request,
                        &log_path,
                        &mut failure_log,
                        AutosyncRecordedFailure::FingerprintFailed,
                        fingerprint_started.elapsed().as_millis(),
                        request.quiet,
                    );
                    continue;
                }
            };
            if stable == current {
                continue;
            }
            current = stable;
            if !request.quiet {
                eprintln!("autosync: change detected; running sync");
            }
            let sync_request = CliIndexRequest {
                repository_root: root.display().to_string(),
                state_dir_override: request.state_dir_override.clone(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                strict_gitignore: request.strict_gitignore,
                semantic_worker_executable: semantic_worker_executable.clone(),
                semantic_worker_args: semantic_worker_args.clone(),
                progress: ProgressMode::Never,
                json: false,
                quiet: true,
                stderr_is_terminal: false,
            };
            let started = Instant::now();
            match self.index_repository("sync", sync_request) {
                Ok(outcome) => {
                    for line in failure_log.success_lines() {
                        append_autosync_daemon_log(Some(&log_path), &line);
                        if !request.quiet {
                            eprintln!("{line}");
                        }
                    }
                    let line = format_autosync_sync_log(&outcome, started.elapsed().as_millis());
                    append_autosync_daemon_log(Some(&log_path), &line);
                    if !request.quiet {
                        eprintln!("{line}");
                    }
                    let _ = record_autosync_run(
                        &autosync_request,
                        AutosyncRunResult::Ok,
                        outcome.active_generation.as_deref(),
                        None,
                    );
                }
                Err(_error) => {
                    let message = AutosyncRecordedFailure::SyncFailed.as_str();
                    let lines = failure_log.failure_lines(message, started.elapsed().as_millis());
                    for line in lines {
                        append_autosync_daemon_log(Some(&log_path), &line);
                        if !request.quiet {
                            eprintln!("{line}");
                        }
                    }
                    let _ = record_autosync_run(
                        &autosync_request,
                        AutosyncRunResult::Error,
                        None,
                        Some(message),
                    );
                    if self.autosync_terminal_repository_state(&request)?.is_some() {
                        for line in failure_log.terminal_lines() {
                            append_autosync_daemon_log(Some(&log_path), &line);
                            if !request.quiet {
                                eprintln!("{line}");
                            }
                        }
                        let line = "autosync: stopping because repository state is unavailable"
                            .to_string();
                        append_autosync_daemon_log(Some(&log_path), &line);
                        if !request.quiet {
                            eprintln!("{line}");
                        }
                        return Ok(AutosyncReport {
                            state_dir: initial_report.state_dir.clone(),
                            enabled: initial_report.enabled,
                            running: false,
                            daemon_state: AutosyncDaemonState::Stopped,
                            pid: None,
                            poll_ms: settings.poll_ms,
                            debounce_ms: settings.debounce_ms,
                            last_run: initial_report.last_run.clone(),
                            startup: initial_report.startup.clone(),
                            repository_ready: false,
                            message: "auto-sync stopped because repository state is unavailable"
                                .to_string(),
                        });
                    }
                }
            }
        }
    }

    fn start_autosync_process(
        &self,
        request: &CliAutosyncRequest,
    ) -> Result<AutosyncReport, RepoGrammarError> {
        let autosync_request = self.autosync_request(request);
        let settings = self.autosync_settings(request);
        let status = autosync_status(autosync_request.clone())?;
        if status.running {
            return Ok(AutosyncReport {
                message: "auto-sync is already running".to_string(),
                ..status
            });
        }
        let startup_nonce = new_autosync_startup_nonce();
        let env_lookup = |key: &str| std::env::var(key).ok();
        if autosync_semantic_worker_environment_from_lookup(&env_lookup).is_err() {
            let code = AutosyncStartupFailureCode::WorkerEnvironmentInvalid;
            persist_startup_failure(&autosync_request, Some(&startup_nonce), code);
            return Err(AutosyncStartupFailure::Classified(code).into_error());
        }
        enable_autosync(autosync_request.clone(), settings)?;
        let status = autosync_status(autosync_request.clone())?;
        if status.running {
            return Ok(AutosyncReport {
                message: "auto-sync is already running".to_string(),
                ..status
            });
        }
        if daemon_log_path(&autosync_request).is_err() {
            let code = AutosyncStartupFailureCode::RepositoryStateUnavailable;
            persist_startup_failure(&autosync_request, Some(&startup_nonce), code);
            return Err(AutosyncStartupFailure::Classified(code).into_error());
        }
        let mut child = match spawn_autosync_daemon(request, &startup_nonce) {
            Ok(child) => child,
            Err(_) => {
                let code = AutosyncStartupFailureCode::ChildExitedBeforeReady;
                persist_startup_failure(&autosync_request, Some(&startup_nonce), code);
                return Err(AutosyncStartupFailure::Classified(code).into_error());
            }
        };
        let child_pid = child.id();
        let readiness = wait_for_autosync_startup(
            || {
                let readiness =
                    inspect_autosync_startup(&autosync_request, child_pid, &startup_nonce)?;
                let child_exited = child.has_exited()?;
                Ok((readiness, child_exited))
            },
            || std::thread::sleep(AUTOSYNC_STARTUP_POLL_INTERVAL),
            AUTOSYNC_STARTUP_MAX_ATTEMPTS,
        );
        match readiness {
            Ok(report) => Ok(report),
            Err(failure) => {
                child.terminate();
                if let Some(code) = failure.code() {
                    persist_startup_failure(&autosync_request, Some(&startup_nonce), code);
                }
                Err(failure.into_error())
            }
        }
    }
}

fn new_autosync_startup_nonce() -> String {
    let sequence = AUTOSYNC_STARTUP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{timestamp:032x}{:08x}{sequence:016x}", std::process::id())
}

#[cfg(not(windows))]
struct SpawnedAutosyncProcess {
    child: std::process::Child,
}

#[cfg(not(windows))]
impl SpawnedAutosyncProcess {
    fn id(&self) -> u32 {
        self.child.id()
    }

    fn has_exited(&mut self) -> Result<bool, RepoGrammarError> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|_| {
                RepoGrammarError::InvalidInput(
                    "failed to verify auto-sync startup readiness".to_string(),
                )
            })
    }

    fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(not(windows))]
fn spawn_autosync_daemon(
    request: &CliAutosyncRequest,
    startup_nonce: &str,
) -> Result<SpawnedAutosyncProcess, RepoGrammarError> {
    let mut command = Command::new(std::env::current_exe().map_err(|_| {
        RepoGrammarError::InvalidInput("failed to resolve repogrammar executable".to_string())
    })?);
    command
        .arg("autosync")
        .arg("run")
        .arg("--path")
        .arg(&request.repository_root)
        .arg("--poll-ms")
        .arg(request.poll_ms.to_string())
        .arg("--debounce-ms")
        .arg(request.debounce_ms.to_string())
        .arg("--quiet")
        .current_dir(&request.repository_root)
        .env(AUTOSYNC_STARTUP_NONCE_ENV, startup_nonce)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(state_dir) = &request.state_dir_override {
        command.env("REPOGRAMMAR_DIR", state_dir);
    }
    command
        .spawn()
        .map(|child| SpawnedAutosyncProcess { child })
        .map_err(|_| RepoGrammarError::InvalidInput("failed to start auto-sync".to_string()))
}

#[cfg(windows)]
struct SpawnedAutosyncProcess {
    process_handle: *mut c_void,
    pid: u32,
}

#[cfg(windows)]
impl SpawnedAutosyncProcess {
    fn id(&self) -> u32 {
        self.pid
    }

    fn has_exited(&mut self) -> Result<bool, RepoGrammarError> {
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 258;
        match unsafe { WaitForSingleObject(self.process_handle, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(RepoGrammarError::InvalidInput(
                "failed to verify auto-sync startup readiness".to_string(),
            )),
        }
    }

    fn terminate(&mut self) {
        unsafe {
            let _ = TerminateProcess(self.process_handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for SpawnedAutosyncProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.process_handle);
        }
    }
}

#[cfg(windows)]
fn spawn_autosync_daemon(
    request: &CliAutosyncRequest,
    startup_nonce: &str,
) -> Result<SpawnedAutosyncProcess, RepoGrammarError> {
    let executable = std::env::current_exe().map_err(|_| {
        RepoGrammarError::InvalidInput("failed to resolve repogrammar executable".to_string())
    })?;
    let args = vec![
        executable.as_os_str().to_os_string(),
        OsString::from("autosync"),
        OsString::from("run"),
        OsString::from("--path"),
        OsString::from(request.repository_root.as_str()),
        OsString::from("--poll-ms"),
        OsString::from(request.poll_ms.to_string()),
        OsString::from("--debounce-ms"),
        OsString::from(request.debounce_ms.to_string()),
        OsString::from("--quiet"),
    ];
    let mut command_line = windows_command_line(&args);
    let application_name = windows_null_terminated(executable.as_os_str());
    let current_directory =
        windows_null_terminated(Path::new(&request.repository_root).as_os_str());
    let mut environment =
        windows_environment_block(request.state_dir_override.as_deref(), Some(startup_nonce));
    let environment_ptr = environment
        .as_mut()
        .map(|block| block.as_mut_ptr().cast::<c_void>())
        .unwrap_or(std::ptr::null_mut());
    let mut startup_info = StartupInfoW {
        cb: std::mem::size_of::<StartupInfoW>() as u32,
        ..Default::default()
    };
    let mut process_info = ProcessInformation::default();
    let creation_flags = CREATE_NEW_PROCESS_GROUP
        | DETACHED_PROCESS
        | environment
            .as_ref()
            .map(|_| CREATE_UNICODE_ENVIRONMENT)
            .unwrap_or(0);
    let created = unsafe {
        CreateProcessW(
            application_name.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            creation_flags,
            environment_ptr,
            current_directory.as_ptr(),
            &mut startup_info,
            &mut process_info,
        ) != 0
    };
    if !created {
        return Err(RepoGrammarError::InvalidInput(
            "failed to start auto-sync".to_string(),
        ));
    }
    unsafe {
        let _ = CloseHandle(process_info.h_thread);
    }
    Ok(SpawnedAutosyncProcess {
        process_handle: process_info.h_process,
        pid: process_info.dw_process_id,
    })
}

#[cfg(windows)]
fn windows_command_line(args: &[OsString]) -> Vec<u16> {
    let mut command_line = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            command_line.push(b' ' as u16);
        }
        push_windows_quoted_arg(&mut command_line, arg);
    }
    command_line.push(0);
    command_line
}

#[cfg(windows)]
fn push_windows_quoted_arg(command_line: &mut Vec<u16>, arg: &OsString) {
    let value = arg.as_os_str().encode_wide().collect::<Vec<_>>();
    let needs_quotes = value.is_empty()
        || value
            .iter()
            .any(|ch| *ch == b' ' as u16 || *ch == b'\t' as u16 || *ch == b'"' as u16);
    if !needs_quotes {
        command_line.extend(value);
        return;
    }

    command_line.push(b'"' as u16);
    let mut backslashes = 0_usize;
    for ch in value {
        if ch == b'\\' as u16 {
            backslashes += 1;
        } else if ch == b'"' as u16 {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
            command_line.push(ch);
            backslashes = 0;
        } else {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            command_line.push(ch);
            backslashes = 0;
        }
    }
    command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    command_line.push(b'"' as u16);
}

#[cfg(windows)]
fn windows_null_terminated(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn windows_environment_block(
    state_dir_override: Option<&str>,
    startup_nonce: Option<&str>,
) -> Option<Vec<u16>> {
    if state_dir_override.is_none() && startup_nonce.is_none() {
        return None;
    }
    let mut values = std::env::vars_os().collect::<Vec<_>>();
    values.retain(|(key, _)| {
        let key = key.to_string_lossy();
        !key.eq_ignore_ascii_case("REPOGRAMMAR_DIR")
            && !key.eq_ignore_ascii_case(AUTOSYNC_STARTUP_NONCE_ENV)
    });
    if let Some(state_dir) = state_dir_override {
        values.push((OsString::from("REPOGRAMMAR_DIR"), OsString::from(state_dir)));
    }
    if let Some(startup_nonce) = startup_nonce {
        values.push((
            OsString::from(AUTOSYNC_STARTUP_NONCE_ENV),
            OsString::from(startup_nonce),
        ));
    }
    values.sort_by_key(|(key, _)| key.to_string_lossy().to_ascii_uppercase());

    let mut block = Vec::new();
    for (key, value) in values {
        block.extend(key.as_os_str().encode_wide());
        block.push(b'=' as u16);
        block.extend(value.as_os_str().encode_wide());
        block.push(0);
    }
    block.push(0);
    Some(block)
}

#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
struct StartupInfoW {
    cb: u32,
    lp_reserved: *mut u16,
    lp_desktop: *mut u16,
    lp_title: *mut u16,
    dw_x: u32,
    dw_y: u32,
    dw_x_size: u32,
    dw_y_size: u32,
    dw_x_count_chars: u32,
    dw_y_count_chars: u32,
    dw_fill_attribute: u32,
    dw_flags: u32,
    w_show_window: u16,
    cb_reserved2: u16,
    lp_reserved2: *mut u8,
    h_std_input: *mut c_void,
    h_std_output: *mut c_void,
    h_std_error: *mut c_void,
}

#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
struct ProcessInformation {
    h_process: *mut c_void,
    h_thread: *mut c_void,
    dw_process_id: u32,
    dw_thread_id: u32,
}

#[cfg(windows)]
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateProcessW(
        lp_application_name: *const u16,
        lp_command_line: *mut u16,
        lp_process_attributes: *mut c_void,
        lp_thread_attributes: *mut c_void,
        b_inherit_handles: i32,
        dw_creation_flags: u32,
        lp_environment: *mut c_void,
        lp_current_directory: *const u16,
        lp_startup_info: *mut StartupInfoW,
        lp_process_information: *mut ProcessInformation,
    ) -> i32;
    fn WaitForSingleObject(h_handle: *mut c_void, dw_milliseconds: u32) -> u32;
    fn TerminateProcess(h_process: *mut c_void, u_exit_code: u32) -> i32;
    fn CloseHandle(h_object: *mut c_void) -> i32;
}

impl CliRuntime for ProductCliRuntime {
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

        let indexing_request = IndexingRequest {
            repository_root: request.repository_root,
            state_dir_override: request.state_dir_override,
            max_file_bytes: request.max_file_bytes,
            strict_gitignore: request.strict_gitignore,
        };
        let framework_roles = SyntaxFrameworkRoleDetector;
        let parser = RepoGrammarSourceParser::default();
        let rust_provider = CargoMetadataRustProvider::new("cargo")
            .with_provider_version(env!("CARGO_PKG_VERSION"));
        let emit_progress = should_emit_progress(
            request.progress,
            request.json,
            request.quiet,
            request.stderr_is_terminal,
        );
        let interactive_progress = emit_progress && request.stderr_is_terminal;
        let mut progress_sink = ProductProgressSink::new(command, interactive_progress);
        let result = {
            let mut progress = |event| {
                if emit_progress {
                    progress_sink.emit(&event);
                }
            };
            if let Some(executable) = request.semantic_worker_executable {
                let worker = TypeScriptSemanticWorkerBoundary::new(executable)
                    .with_args(request.semantic_worker_args);
                if command == "sync" {
                    sync_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress(
                        indexing_request,
                        &FilesystemFileDiscovery,
                        &FilesystemSourceStore,
                        &parser,
                        (&framework_roles, &worker, &rust_provider),
                        &store,
                        &mut progress,
                    )
                } else {
                    index_repository_with_discovery_parser_frameworks_semantic_worker_rust_provider_families_and_store_with_progress(
                        indexing_request,
                        &FilesystemFileDiscovery,
                        &FilesystemSourceStore,
                        &parser,
                        (&framework_roles, &worker, &rust_provider),
                        &store,
                        &mut progress,
                    )
                }
            } else if command == "sync" {
                sync_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress(
                    indexing_request,
                    &FilesystemFileDiscovery,
                    &FilesystemSourceStore,
                    &parser,
                    (&framework_roles, &rust_provider),
                    &store,
                    &mut progress,
                )
            } else {
                index_repository_with_discovery_parser_frameworks_rust_provider_families_and_store_with_progress(
                    indexing_request,
                    &FilesystemFileDiscovery,
                    &FilesystemSourceStore,
                    &parser,
                    (&framework_roles, &rust_provider),
                    &store,
                    &mut progress,
                )
            }
        };
        progress_sink.finish();
        result
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

    fn autosync(
        &self,
        command: AutosyncCommand,
        request: CliAutosyncRequest,
    ) -> Result<AutosyncReport, RepoGrammarError> {
        let autosync_request = self.autosync_request(&request);
        let settings = self.autosync_settings(&request);
        match command {
            AutosyncCommand::Enable => enable_autosync(autosync_request, settings),
            AutosyncCommand::Disable => disable_autosync(autosync_request),
            AutosyncCommand::Start => self.start_autosync_process(&request),
            AutosyncCommand::Stop => stop_autosync(autosync_request),
            AutosyncCommand::Status => autosync_status(autosync_request),
            AutosyncCommand::Run => self.run_autosync_loop(request),
        }
    }

    fn prune_generations(
        &self,
        request: RepositoryStatusRequest,
        prune: GenerationPruneRequest,
    ) -> Result<GenerationPruneReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        let status = repository_status_with_storage(request.clone(), &store)?;
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

        prune_index_generations(
            &store,
            &request.path,
            request.state_dir_override.as_deref(),
            prune,
        )
    }

    fn compact_storage(
        &self,
        request: RepositoryStatusRequest,
        compact: IndexCompactRequest,
    ) -> Result<IndexCompactReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        let status = repository_status_with_storage(request.clone(), &store)?;
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

        compact_index_storage(
            &store,
            &request.path,
            request.state_dir_override.as_deref(),
            compact,
        )
    }

    fn clean_storage(
        &self,
        request: RepositoryStatusRequest,
        clean: StorageCleanRequest,
    ) -> Result<StorageCleanReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        let status = repository_status_with_storage(request.clone(), &store)?;
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

        clean_index_storage(
            &store,
            &request.path,
            request.state_dir_override.as_deref(),
            clean,
        )
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
        list_families_with_freshness(
            FamilyEvidenceFreshnessRequest {
                repository_root: request.path.clone(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
        )
    }

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        lookup_family_with_freshness_and_local_context(
            FamilyEvidenceFreshnessRequest {
                repository_root: request.path.clone(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &store,
            &FilesystemSourceStore,
            target,
            mode,
        )
    }

    fn render_source_spans(
        &self,
        request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
        include_source_spans: bool,
        token_budget: Option<usize>,
    ) -> Result<SourceSpanRenderReport, RepoGrammarError> {
        render_source_spans(
            SourceSpanRenderRequest {
                repository_root: request.path,
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &FilesystemSourceStore,
            read_plan,
            include_source_spans,
            token_budget,
        )
    }

    fn enrich_read_plan_line_ranges(
        &self,
        request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
    ) -> Result<ReadPlan, RepoGrammarError> {
        enrich_read_plan_line_ranges(
            SourceSpanRenderRequest {
                repository_root: request.path,
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &FilesystemSourceStore,
            read_plan,
        )
    }

    fn repo_shape_diagnostics(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepoShapeDiagnosticsReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        repo_shape_diagnostics(&store, &store)
    }

    fn unknown_inventory(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<UnknownInventoryReport, RepoGrammarError> {
        let store = self.store_for_status_request(&request)?;
        unknown_inventory(&store)
    }

    fn install_agent_integration(
        &self,
        command: &str,
        request: InstallRequest,
        context: InstallExecutionContext,
    ) -> Result<InstallExecutionOutcome, RepoGrammarError> {
        let configurator = ProductNativeAgentConfigurator;
        let self_tester = ProductMcpSelfTester::new();
        match command {
            "install" => execute_install(&request, &context, &configurator, &self_tester),
            "uninstall" => execute_uninstall(&request, &context, &configurator),
            _ => Err(RepoGrammarError::InvalidInput(
                "unknown installer command".to_string(),
            )),
        }
    }

    fn inspect_agent_integration(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        context: &InstallExecutionContext,
    ) -> Result<AgentIntegrationInspection, RepoGrammarError> {
        inspect_agent_integration(context, target, scope, &ProductNativeAgentConfigurator)
    }

    fn mcp_self_test(&self, project: &str) -> Result<(), SetupFailureClass> {
        let executable = std::env::current_exe()
            .ok()
            .and_then(|path| path.to_str().map(str::to_string))
            .ok_or(SetupFailureClass::McpSelfTestFailed)?;
        ProductMcpSelfTester::new()
            .run(&executable, project)
            .map_err(|error| match error {
                ProductMcpSelfTestError::TimedOut => SetupFailureClass::McpSelfTestTimedOut,
                ProductMcpSelfTestError::Failed => SetupFailureClass::McpSelfTestFailed,
            })
    }

    fn upload_telemetry_payload(
        &self,
        endpoint: &str,
        payload: &str,
        timeout: Duration,
    ) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
        upload_telemetry_with_curl(endpoint, payload, timeout)
    }
}

fn upload_telemetry_with_curl(
    endpoint: &str,
    payload: &str,
    timeout: Duration,
) -> Result<TelemetryUploadReceipt, RepoGrammarError> {
    let mut child = Command::new("curl")
        .arg("--fail")
        .arg("--silent")
        .arg("--show-error")
        .arg("--max-time")
        .arg(timeout.as_secs().max(1).to_string())
        .arg("--request")
        .arg("POST")
        .arg("--header")
        .arg("content-type: application/json")
        .arg("--data-binary")
        .arg("@-")
        .arg(endpoint)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            RepoGrammarError::InvalidInput("telemetry upload transport unavailable".to_string())
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|_| RepoGrammarError::InvalidInput("telemetry upload failed".to_string()))?;
    }
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| RepoGrammarError::InvalidInput("telemetry upload failed".to_string()))?
        {
            if status.success() {
                return Ok(TelemetryUploadReceipt {
                    status_code: 200,
                    receipt_id: format!("receipt-{}", std::process::id()),
                });
            }
            return Err(RepoGrammarError::InvalidInput(
                "telemetry upload failed".to_string(),
            ));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RepoGrammarError::InvalidInput(
                "telemetry upload timed out".to_string(),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl McpReadOnlyRuntime for ProductCliRuntime {
    fn repository_status(
        &self,
        request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, RepoGrammarError> {
        <Self as CliRuntime>::repository_status(self, request)
    }

    fn family_lookup(
        &self,
        request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, RepoGrammarError> {
        <Self as CliRuntime>::family_lookup(self, request, target, mode)
    }

    fn render_source_spans(
        &self,
        request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
        include_source_spans: bool,
        token_budget: Option<usize>,
    ) -> Result<SourceSpanRenderReport, RepoGrammarError> {
        <Self as CliRuntime>::render_source_spans(
            self,
            request,
            read_plan,
            include_source_spans,
            token_budget,
        )
    }

    fn enrich_read_plan_line_ranges(
        &self,
        request: RepositoryStatusRequest,
        read_plan: &ReadPlan,
    ) -> Result<ReadPlan, RepoGrammarError> {
        <Self as CliRuntime>::enrich_read_plan_line_ranges(self, request, read_plan)
    }
}

fn run_serve_command(rest: &[String], runtime: &impl McpReadOnlyRuntime) -> i32 {
    // `serve` is intercepted in main() before the shared `--help` handler, so it
    // must handle help itself to stay consistent with every other command.
    if rest.iter().any(|arg| arg == "--help" || arg == "-h") {
        if let Some(usage) = command_usage("serve") {
            print!("{usage}");
        }
        return 0;
    }
    let options = match parse_serve_options(rest) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let current_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir,
        Err(error) => {
            eprintln!("failed to read current directory: {error}");
            return 1;
        }
    };
    let env_lookup = |key: &str| std::env::var(key).ok();
    let context = McpServeContext {
        repository_root: repository_root(&current_dir, options.project_path.as_deref()),
        state_dir_override: state_dir_override(&env_lookup),
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve_json_lines(runtime, &context, stdin.lock(), stdout.lock()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            2
        }
    }
}

struct ProductNativeAgentConfigurator;

impl NativeAgentConfigurator for ProductNativeAgentConfigurator {
    fn inspect_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        current_dir: &str,
    ) -> Result<NativeMcpServerState, RepoGrammarError> {
        let (program, args) = native_get_command(target, scope)?;
        run_native_agent_probe(target, scope, &program, &args, current_dir)
    }

    fn add_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        executable_path: &str,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError> {
        let (program, args) = native_add_command(target, scope, executable_path)?;
        run_native_agent_command(&program, &args, current_dir)?;
        Ok(NativeAgentAction {
            target,
            program,
            args,
        })
    }

    fn remove_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError> {
        let (program, args) = native_remove_command(target, scope)?;
        run_native_agent_command(&program, &args, current_dir)?;
        Ok(NativeAgentAction {
            target,
            program,
            args,
        })
    }
}

struct ProductMcpSelfTester {
    timeout: std::time::Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProductMcpSelfTestError {
    TimedOut,
    Failed,
}

impl ProductMcpSelfTester {
    fn new() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(5),
        }
    }

    #[cfg(all(test, unix))]
    fn with_timeout(timeout: std::time::Duration) -> Self {
        Self { timeout }
    }

    fn run(&self, executable_path: &str, current_dir: &str) -> Result<(), ProductMcpSelfTestError> {
        let mut child = Command::new(executable_path)
            .args(["serve", "--project", current_dir])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ProductMcpSelfTestError::Failed)?;
        if let Some(mut stdin) = child.stdin.take() {
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
            )
            .map_err(|_| ProductMcpSelfTestError::Failed)?;
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"shutdown"})
            )
            .map_err(|_| ProductMcpSelfTestError::Failed)?;
        }
        let output = wait_with_timeout(child, self.timeout)?;
        if !output.status.success() {
            return Err(ProductMcpSelfTestError::Failed);
        }
        let stdout =
            String::from_utf8(output.stdout).map_err(|_| ProductMcpSelfTestError::Failed)?;
        let first = stdout
            .lines()
            .next()
            .ok_or(ProductMcpSelfTestError::Failed)?;
        let value: serde_json::Value =
            serde_json::from_str(first).map_err(|_| ProductMcpSelfTestError::Failed)?;
        let tools = value["result"]["tools"]
            .as_array()
            .ok_or(ProductMcpSelfTestError::Failed)?;
        if tools.len() == 1 && tools[0]["name"] == McpToolName::Context.as_str() {
            Ok(())
        } else {
            Err(ProductMcpSelfTestError::Failed)
        }
    }
}

impl McpSelfTestRunner for ProductMcpSelfTester {
    fn self_test(&self, executable_path: &str, current_dir: &str) -> Result<(), RepoGrammarError> {
        self.run(executable_path, current_dir).map_err(|error| {
            let message = match error {
                ProductMcpSelfTestError::TimedOut => "MCP self-test timed out",
                ProductMcpSelfTestError::Failed => "MCP self-test failed",
            };
            RepoGrammarError::InvalidInput(message.to_string())
        })
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::Output, ProductMcpSelfTestError> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .map_err(|_| ProductMcpSelfTestError::Failed);
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProductMcpSelfTestError::TimedOut);
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(_) => {
                return Err(ProductMcpSelfTestError::Failed);
            }
        }
    }
}

fn native_add_command(
    target: AgentTarget,
    scope: InstallScope,
    executable_path: &str,
) -> Result<(String, Vec<String>), RepoGrammarError> {
    match target {
        AgentTarget::Codex => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "codex project-local install is unsupported by the native codex mcp CLI"
                        .to_string(),
                ));
            }
            Ok((
                native_agent_program(target).to_string(),
                vec![
                    "mcp".to_string(),
                    "add".to_string(),
                    MCP_SERVER_NAME.to_string(),
                    "--".to_string(),
                    executable_path.to_string(),
                    "serve".to_string(),
                ],
            ))
        }
        AgentTarget::ClaudeCode => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "claude-code project-local install is deferred".to_string(),
                ));
            }
            let scope = claude_scope(scope);
            Ok((
                native_agent_program(target).to_string(),
                vec![
                    "mcp".to_string(),
                    "add".to_string(),
                    "--scope".to_string(),
                    scope.to_string(),
                    MCP_SERVER_NAME.to_string(),
                    "--".to_string(),
                    executable_path.to_string(),
                    "serve".to_string(),
                ],
            ))
        }
        AgentTarget::AllSupported | AgentTarget::None => Err(RepoGrammarError::InvalidInput(
            "native command requires a concrete agent target".to_string(),
        )),
        target => Err(RepoGrammarError::InvalidInput(format!(
            "{} {} live install is deferred; use --dry-run or --print-config {}",
            target.as_str(),
            scope.as_str(),
            target.as_str()
        ))),
    }
}

fn native_remove_command(
    target: AgentTarget,
    scope: InstallScope,
) -> Result<(String, Vec<String>), RepoGrammarError> {
    match target {
        AgentTarget::Codex => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "codex project-local uninstall is unsupported by the native codex mcp CLI"
                        .to_string(),
                ));
            }
            Ok((
                native_agent_program(target).to_string(),
                vec![
                    "mcp".to_string(),
                    "remove".to_string(),
                    MCP_SERVER_NAME.to_string(),
                ],
            ))
        }
        AgentTarget::ClaudeCode => {
            if scope == InstallScope::ProjectLocal {
                return Err(RepoGrammarError::InvalidInput(
                    "claude-code project-local uninstall is deferred".to_string(),
                ));
            }
            Ok((
                native_agent_program(target).to_string(),
                vec![
                    "mcp".to_string(),
                    "remove".to_string(),
                    "--scope".to_string(),
                    claude_scope(scope).to_string(),
                    MCP_SERVER_NAME.to_string(),
                ],
            ))
        }
        AgentTarget::AllSupported | AgentTarget::None => Err(RepoGrammarError::InvalidInput(
            "native command requires a concrete agent target".to_string(),
        )),
        target => Err(RepoGrammarError::InvalidInput(format!(
            "{} {} live uninstall is deferred; use --dry-run or --print-config {}",
            target.as_str(),
            scope.as_str(),
            target.as_str()
        ))),
    }
}

fn native_get_command(
    target: AgentTarget,
    scope: InstallScope,
) -> Result<(String, Vec<String>), RepoGrammarError> {
    if !target.has_live_writer(scope) {
        return Err(RepoGrammarError::InvalidInput(format!(
            "{} {} native MCP probe is unsupported",
            target.as_str(),
            scope.as_str()
        )));
    }
    let mut args = vec![
        "mcp".to_string(),
        "get".to_string(),
        MCP_SERVER_NAME.to_string(),
    ];
    if target == AgentTarget::Codex {
        args.push("--json".to_string());
    }
    Ok((native_agent_program(target).to_string(), args))
}

fn native_agent_program(target: AgentTarget) -> &'static str {
    match target {
        AgentTarget::Codex => native_codex_program(),
        AgentTarget::ClaudeCode => "claude",
        _ => unreachable!("native_agent_program requires a concrete native agent target"),
    }
}

#[cfg(windows)]
fn native_codex_program() -> &'static str {
    "codex.cmd"
}

#[cfg(not(windows))]
fn native_codex_program() -> &'static str {
    "codex"
}

fn claude_scope(scope: InstallScope) -> &'static str {
    match scope {
        InstallScope::Global => "user",
        InstallScope::ProjectLocal => "project",
    }
}

fn run_native_agent_command(
    program: &str,
    args: &[String],
    current_dir: &str,
) -> Result<(), RepoGrammarError> {
    let status = Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| {
            RepoGrammarError::InvalidInput(format!("native {program} CLI is unavailable"))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(format!(
            "native {program} MCP command failed"
        )))
    }
}

fn run_native_agent_probe(
    target: AgentTarget,
    scope: InstallScope,
    program: &str,
    args: &[String],
    current_dir: &str,
) -> Result<NativeMcpServerState, RepoGrammarError> {
    let child = Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| {
            RepoGrammarError::InvalidInput(format!("native {program} CLI is unavailable"))
        })?;
    let output = wait_with_timeout(child, std::time::Duration::from_secs(5)).map_err(|_| {
        RepoGrammarError::InvalidInput(format!("native {} MCP probe failed", target.as_str()))
    })?;
    classify_native_agent_probe(
        target,
        scope,
        output.status.success(),
        &output.stdout,
        &output.stderr,
    )
}

fn classify_native_agent_probe(
    target: AgentTarget,
    scope: InstallScope,
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<NativeMcpServerState, RepoGrammarError> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let expected = match target {
        AgentTarget::Codex => format!("Error: No MCP server named '{MCP_SERVER_NAME}' found."),
        AgentTarget::ClaudeCode => {
            format!("No MCP server named \"{MCP_SERVER_NAME}\". Run `claude mcp add` to add one.")
        }
        _ => {
            return Err(RepoGrammarError::InvalidInput(
                "native MCP probe requires a live agent target".to_string(),
            ));
        }
    };
    if !success {
        return if stdout.trim() == expected || stderr.trim() == expected {
            Ok(NativeMcpServerState::NotFound)
        } else {
            Err(RepoGrammarError::InvalidInput(format!(
                "native {} MCP probe failed",
                target.as_str()
            )))
        };
    }

    let config = match target {
        AgentTarget::Codex => parse_codex_mcp_probe(&stdout, scope),
        AgentTarget::ClaudeCode => parse_claude_mcp_probe(&stdout),
        _ => unreachable!("live target checked above"),
    };
    Ok(config.map_or(
        NativeMcpServerState::Malformed,
        NativeMcpServerState::Present,
    ))
}

fn parse_codex_mcp_probe(output: &str, scope: InstallScope) -> Option<NativeMcpServerConfig> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let transport = value.get("transport")?;
    if value.get("name")?.as_str()? != MCP_SERVER_NAME
        || transport.get("type")?.as_str()? != "stdio"
    {
        return None;
    }
    let executable_path = transport.get("command")?.as_str()?.to_string();
    let args = transport
        .get("args")?
        .as_array()?
        .iter()
        .map(|arg| arg.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    Some(NativeMcpServerConfig {
        executable_path,
        args,
        scope,
        enabled: value.get("enabled")?.as_bool()?,
    })
}

fn parse_claude_mcp_probe(output: &str) -> Option<NativeMcpServerConfig> {
    let mut scope = None;
    let mut executable_path = None;
    let mut args = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Scope: ") {
            scope = if value.starts_with("User config") {
                Some(InstallScope::Global)
            } else if value.starts_with("Local config") || value.starts_with("Project config") {
                Some(InstallScope::ProjectLocal)
            } else {
                return None;
            };
        } else if let Some(value) = line.strip_prefix("Command: ") {
            executable_path = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Args: ") {
            args = Some(if value.is_empty() {
                Vec::new()
            } else {
                value.split_whitespace().map(str::to_string).collect()
            });
        }
    }
    Some(NativeMcpServerConfig {
        executable_path: executable_path?,
        args: args?,
        scope: scope?,
        enabled: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use repogrammar::application::progress::{ProgressStage, WorkUnits};
    use repogrammar::application::query::{
        assess_semantic_fact_readiness, list_semantic_facts, IndexedSemanticFactsReport,
        SemanticFactReadinessRequest,
    };
    use repogrammar::core::model::UnknownReasonCode;
    #[cfg(unix)]
    use repogrammar::core::model::{CodeUnitKind, Language, RepositoryRevision};
    use repogrammar::core::policy::freshness::ClaimInputReadiness;
    use repogrammar::interfaces::mcp::handle_context_call;
    #[cfg(unix)]
    use repogrammar::ports::file_discovery::{
        DiscoveredLanguage, FileDiscovery, FileDiscoveryRequest,
    };
    use repogrammar::ports::index_store::IndexedSemanticFactRecord;
    #[cfg(unix)]
    use repogrammar::ports::parser::{SourceDocument, SourceParser};
    #[cfg(unix)]
    use repogrammar::ports::source_store::{SourceReadRequest, SourceStore};
    use rusqlite::{params, Connection};
    use serde_json::Value;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "repogrammar-bin-{prefix}-{}-{}",
                std::process::id(),
                unique_suffix()
            ));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos()
    }

    fn cli_args(command: &str, project: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            command.to_string(),
            "--project".to_string(),
            project.display().to_string(),
        ];
        args.extend(extra.iter().map(|value| value.to_string()));
        args
    }

    fn stored_generation_count(project: &Path) -> u32 {
        let connection = Connection::open(project.join(".repogrammar/repogrammar.sqlite"))
            .expect("open repository database");
        connection
            .query_row("SELECT count(*) FROM index_generations", [], |row| {
                row.get(0)
            })
            .expect("count generations")
    }

    fn stored_generation_exists(project: &Path, generation_id: &str) -> bool {
        let connection = Connection::open(project.join(".repogrammar/repogrammar.sqlite"))
            .expect("open repository database");
        let count: u32 = connection
            .query_row(
                "SELECT count(*) FROM index_generations WHERE generation_id = ?1",
                params![generation_id],
                |row| row.get(0),
            )
            .expect("check generation");
        count == 1
    }

    #[test]
    fn interactive_index_progress_rewrites_single_terminal_line() {
        let long = ProgressEvent::new(
            ProgressStage::FileScanning,
            "stored file metadata",
            WorkUnits::known(12, 236).expect("valid work"),
        );
        let (long_frame, long_width) = render_interactive_index_progress_event("sync", &long, 0);

        assert!(long_frame.starts_with('\r'));
        assert!(!long_frame.contains('\n'));
        assert!(long_frame.contains("sync: [#-------------------] 5% 12/236 file_scanning"));

        let short = ProgressEvent::new(ProgressStage::ProjectDiscovery, "done", WorkUnits::Unknown);
        let (short_frame, short_width) =
            render_interactive_index_progress_event("sync", &short, long_width);

        assert!(short_frame.starts_with('\r'));
        assert!(!short_frame.contains('\n'));
        assert!(short_frame.contains("sync: [working] project_discovery: done"));
        assert!(!short_frame.contains('%'));
        assert!(short_frame.ends_with(&" ".repeat(long_width - short_width)));
    }

    #[test]
    fn autosync_sync_log_summarizes_files_units_time_and_generation() {
        let outcome = IndexingOutcome {
            indexing_mode:
                repogrammar::application::indexing::IndexingGenerationMode::SyntaxOnlyCodeUnits,
            parser_attempted_files: 3,
            indexed_units: 42,
            semantic_facts: 7,
            discovered_files: 3,
            skipped_paths: 1,
            active_generation: Some("gen-000007".to_string()),
            semantic_worker: repogrammar::application::indexing::SemanticWorkerRunStatus::Deferred,
            sync_report: None,
            warnings: Vec::new(),
        };
        assert_eq!(
            format_autosync_sync_log(&outcome, 241),
            "autosync: synced 3 file(s), 42 unit(s) in 241ms (generation gen-000007)"
        );
    }

    #[test]
    fn autosync_sync_log_handles_missing_generation() {
        let outcome = IndexingOutcome {
            indexing_mode:
                repogrammar::application::indexing::IndexingGenerationMode::FileManifestOnly,
            parser_attempted_files: 0,
            indexed_units: 0,
            semantic_facts: 0,
            discovered_files: 0,
            skipped_paths: 0,
            active_generation: None,
            semantic_worker: repogrammar::application::indexing::SemanticWorkerRunStatus::Deferred,
            sync_report: None,
            warnings: Vec::new(),
        };
        assert_eq!(
            format_autosync_sync_log(&outcome, 5),
            "autosync: synced 0 file(s), 0 unit(s) in 5ms (generation none)"
        );
    }

    #[test]
    fn autosync_sync_log_includes_incremental_delta_when_available() {
        let outcome = IndexingOutcome {
            indexing_mode:
                repogrammar::application::indexing::IndexingGenerationMode::SyntaxOnlyCodeUnits,
            parser_attempted_files: 2,
            indexed_units: 5,
            semantic_facts: 2,
            discovered_files: 3,
            skipped_paths: 0,
            active_generation: Some("gen-000008".to_string()),
            semantic_worker: repogrammar::application::indexing::SemanticWorkerRunStatus::Deferred,
            sync_report: Some(repogrammar::application::indexing::IndexingSyncReport {
                base_generation: Some("gen-000007".to_string()),
                sync_mode: repogrammar::application::indexing::IndexingSyncMode::Incremental,
                fallback_reason: None,
                added_files: 1,
                modified_files: 1,
                removed_files: 1,
                unchanged_files: 1,
                copied_forward_files: 1,
                reparsed_files: 2,
                families_recomputed: 0,
                dirty_records_cleared: 0,
                family_identity_delta: None,
            }),
            warnings: Vec::new(),
        };
        assert_eq!(
            format_autosync_sync_log(&outcome, 19),
            "autosync: incremental sync +1 ~1 -1 unchanged 1 copied 1 reparsed 2 file(s), 5 unit(s) in 19ms (generation gen-000008)"
        );
    }

    #[test]
    fn autosync_sync_log_reports_zero_reparsed_for_go_file_manifest_fallback() {
        let outcome = IndexingOutcome {
            indexing_mode:
                repogrammar::application::indexing::IndexingGenerationMode::FileManifestOnly,
            parser_attempted_files: 0,
            indexed_units: 0,
            semantic_facts: 0,
            discovered_files: 1,
            skipped_paths: 0,
            active_generation: Some("gen-000001".to_string()),
            semantic_worker: repogrammar::application::indexing::SemanticWorkerRunStatus::Deferred,
            sync_report: Some(repogrammar::application::indexing::IndexingSyncReport {
                base_generation: None,
                sync_mode:
                    repogrammar::application::indexing::IndexingSyncMode::FullRebuildFallback,
                fallback_reason: Some("missing_active_generation".to_string()),
                added_files: 1,
                modified_files: 0,
                removed_files: 0,
                unchanged_files: 0,
                copied_forward_files: 0,
                reparsed_files: 0,
                families_recomputed: 0,
                dirty_records_cleared: 0,
                family_identity_delta: None,
            }),
            warnings: vec!["parser skipped unsupported language token: go".to_string()],
        };

        let log = format_autosync_sync_log(&outcome, 7);
        assert!(log.contains("full_rebuild_fallback"));
        assert!(log.contains("reparsed 0 file(s)"));
    }

    #[test]
    fn autosync_daemon_log_append_is_direct_and_line_based() {
        let workspace = TempWorkspace::new("autosync-log-append");
        let log_path = workspace.path().join("daemon.log");

        append_autosync_daemon_log(Some(&log_path), "autosync: first");
        append_autosync_daemon_log(Some(&log_path), "autosync: second");
        append_autosync_daemon_log(None, "autosync: ignored");

        let contents = fs::read_to_string(log_path).expect("read daemon log");
        assert_eq!(contents, "autosync: first\nautosync: second\n");
    }

    fn startup_ready_report() -> AutosyncReport {
        AutosyncReport {
            state_dir: ".repogrammar".to_string(),
            enabled: true,
            running: true,
            daemon_state: AutosyncDaemonState::Running,
            pid: Some(42),
            poll_ms: 1000,
            debounce_ms: 750,
            last_run: None,
            startup: repogrammar::application::autosync::AutosyncStartupReport {
                state: repogrammar::application::autosync::AutosyncStartupState::Ready,
                failure_code: None,
                previous_failure_code: None,
            },
            repository_ready: true,
            message: "auto-sync started".to_string(),
        }
    }

    #[test]
    fn autosync_startup_fails_when_child_exits_before_readiness() {
        let result =
            wait_for_autosync_startup(|| Ok((AutosyncStartupReadiness::Pending, true)), || {}, 3);

        assert_eq!(
            result,
            Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::ChildExitedBeforeReady
            ))
        );
    }

    #[test]
    fn autosync_startup_rejects_lock_from_child_that_already_exited() {
        let result = wait_for_autosync_startup(
            || {
                Ok((
                    AutosyncStartupReadiness::Ready(startup_ready_report()),
                    true,
                ))
            },
            || {},
            1,
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::ChildExitedBeforeReady
            ))
        );
    }

    #[test]
    fn autosync_startup_classifies_lock_refusal() {
        let result = wait_for_autosync_startup(
            || Ok((AutosyncStartupReadiness::LockRefused, false)),
            || {},
            3,
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::DaemonLockRefused
            ))
        );
    }

    #[test]
    fn autosync_startup_timeout_is_bounded_without_sleeping() {
        let mut observations = 0_usize;
        let mut pauses = 0_usize;
        let result = wait_for_autosync_startup(
            || {
                observations += 1;
                Ok((AutosyncStartupReadiness::Pending, false))
            },
            || pauses += 1,
            4,
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::StartupTimeout
            ))
        );
        assert_eq!(observations, 4);
        assert_eq!(pauses, 3);
    }

    #[test]
    fn autosync_startup_succeeds_only_after_readiness_proof() {
        let expected = startup_ready_report();
        let mut observations = 0_usize;
        let result = wait_for_autosync_startup(
            || {
                observations += 1;
                if observations == 2 {
                    Ok((AutosyncStartupReadiness::Ready(expected.clone()), false))
                } else {
                    Ok((AutosyncStartupReadiness::Pending, false))
                }
            },
            || {},
            4,
        );

        assert_eq!(result, Ok(expected));
        assert_eq!(observations, 2);
    }

    #[test]
    fn autosync_startup_preserves_child_reported_failure_class() {
        let result = wait_for_autosync_startup(
            || {
                Ok((
                    AutosyncStartupReadiness::Failed(
                        AutosyncStartupFailureCode::RepositoryFingerprintFailed,
                    ),
                    false,
                ))
            },
            || {},
            3,
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailure::Classified(
                AutosyncStartupFailureCode::RepositoryFingerprintFailed
            ))
        );
    }

    #[test]
    fn autosync_initialization_completes_all_steps_before_ready_publication() {
        let calls = std::cell::RefCell::new(Vec::new());
        let initialized = initialize_autosync_service(
            || {
                calls.borrow_mut().push("repository");
                Ok(())
            },
            || {
                calls.borrow_mut().push("worker");
                Ok((None::<String>, Vec::<String>::new()))
            },
            || {
                calls.borrow_mut().push("fingerprint");
                Ok("fingerprint".to_string())
            },
            || {
                calls.borrow_mut().push("log");
                Ok(std::path::PathBuf::from("daemon.log"))
            },
            || {
                calls.borrow_mut().push("owner");
                Ok(())
            },
            || {
                calls.borrow_mut().push("heartbeat_repository");
                Ok(())
            },
            || {
                calls.borrow_mut().push("heartbeat_fingerprint");
                Ok("heartbeat-fingerprint".to_string())
            },
        )
        .expect("initialize service");

        calls.borrow_mut().push("publish_ready");
        calls.borrow_mut().push("next_poll");
        assert_eq!(initialized.1, "heartbeat-fingerprint");
        assert_eq!(
            calls.into_inner(),
            vec![
                "repository",
                "worker",
                "fingerprint",
                "log",
                "owner",
                "heartbeat_repository",
                "heartbeat_fingerprint",
                "publish_ready",
                "next_poll",
            ]
        );
    }

    #[test]
    fn autosync_initialization_stops_at_repository_state_failure() {
        let later_step_called = std::cell::Cell::new(false);
        let result = initialize_autosync_service(
            || Err(()),
            || {
                later_step_called.set(true);
                Ok(())
            },
            || Ok("fingerprint".to_string()),
            || Ok(std::path::PathBuf::from("daemon.log")),
            || Ok(()),
            || Ok(()),
            || Ok("heartbeat-fingerprint".to_string()),
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailureCode::RepositoryStateUnavailable)
        );
        assert!(!later_step_called.get());
    }

    #[test]
    fn autosync_initialization_classifies_initial_fingerprint_failure() {
        let heartbeat_called = std::cell::Cell::new(false);
        let result = initialize_autosync_service(
            || Ok(()),
            || Ok(()),
            || Err(()),
            || Ok(std::path::PathBuf::from("daemon.log")),
            || {
                heartbeat_called.set(true);
                Ok(())
            },
            || {
                heartbeat_called.set(true);
                Ok(())
            },
            || {
                heartbeat_called.set(true);
                Ok("heartbeat-fingerprint".to_string())
            },
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailureCode::RepositoryFingerprintFailed)
        );
        assert!(!heartbeat_called.get());
    }

    #[test]
    fn autosync_initialization_classifies_first_heartbeat_failure() {
        let result = initialize_autosync_service(
            || Ok(()),
            || Ok(()),
            || Ok("fingerprint".to_string()),
            || Ok(std::path::PathBuf::from("daemon.log")),
            || Err(()),
            || Ok(()),
            || Ok("heartbeat-fingerprint".to_string()),
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailureCode::FirstHeartbeatFailed)
        );
    }

    #[test]
    fn autosync_initialization_classifies_second_fingerprint_as_first_heartbeat_failure() {
        let result = initialize_autosync_service(
            || Ok(()),
            || Ok(()),
            || Ok("initial-fingerprint".to_string()),
            || Ok(std::path::PathBuf::from("daemon.log")),
            || Ok(()),
            || Ok(()),
            || Err(()),
        );

        assert_eq!(
            result,
            Err(AutosyncStartupFailureCode::FirstHeartbeatFailed)
        );
    }

    #[test]
    fn autosync_startup_errors_are_sanitized_by_semantic_class() {
        assert_eq!(
            AutosyncStartupFailure::Classified(AutosyncStartupFailureCode::ChildExitedBeforeReady)
                .into_error(),
            RepoGrammarError::InvalidInput(
                "auto-sync exited before startup readiness was confirmed".to_string()
            )
        );
        assert_eq!(
            AutosyncStartupFailure::Classified(AutosyncStartupFailureCode::DaemonLockRefused)
                .into_error(),
            RepoGrammarError::InvalidInput(
                "auto-sync could not acquire daemon ownership".to_string()
            )
        );
        assert_eq!(
            AutosyncStartupFailure::Classified(AutosyncStartupFailureCode::StartupTimeout)
                .into_error(),
            RepoGrammarError::InvalidInput("auto-sync startup readiness timed out".to_string())
        );
    }

    #[test]
    fn autosync_persisted_failures_are_low_cardinality_and_source_free() {
        assert_eq!(
            AutosyncRecordedFailure::FingerprintFailed.as_str(),
            "repository fingerprint failed"
        );
        assert_eq!(
            AutosyncRecordedFailure::StateUnavailable.as_str(),
            "repository state is unavailable"
        );
        assert_eq!(
            AutosyncRecordedFailure::SyncFailed.as_str(),
            "repository sync failed"
        );
        for failure in [
            AutosyncRecordedFailure::FingerprintFailed,
            AutosyncRecordedFailure::StateUnavailable,
            AutosyncRecordedFailure::SyncFailed,
        ] {
            assert!(!failure.as_str().contains('/'));
            assert!(!failure.as_str().contains("token"));
            assert!(!failure.as_str().contains("REPOGRAMMAR_"));
        }
    }

    #[test]
    fn autosync_worker_environment_preflight_rejects_invalid_args_json() {
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => Some("/opt/ts-worker".to_string()),
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => Some("not-json".to_string()),
            _ => None,
        };

        let error =
            autosync_semantic_worker_environment_from_lookup(&env).expect_err("invalid args");

        assert!(error
            .to_string()
            .contains("REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON"));
        assert!(!error.to_string().contains("not-json"));
    }

    #[test]
    fn autosync_worker_environment_preflight_rejects_args_without_executable() {
        let env = |key: &str| {
            (key == "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON")
                .then(|| r#"["src/workers/typescript/worker.js"]"#.to_string())
        };

        let error = autosync_semantic_worker_environment_from_lookup(&env)
            .expect_err("args without executable");

        assert!(error
            .to_string()
            .contains("requires REPOGRAMMAR_TYPESCRIPT_WORKER"));
        assert!(!error.to_string().contains("worker.js"));
    }

    #[test]
    fn autosync_worker_environment_preflight_accepts_blank_env() {
        let env = |key: &str| match key {
            "REPOGRAMMAR_TYPESCRIPT_WORKER" => Some(String::new()),
            "REPOGRAMMAR_TYPESCRIPT_WORKER_ARGS_JSON" => Some(String::new()),
            _ => None,
        };

        let (executable, args) =
            autosync_semantic_worker_environment_from_lookup(&env).expect("blank env");

        assert_eq!(executable, None);
        assert!(args.is_empty());
    }

    #[test]
    fn autosync_failure_log_summarizes_repeated_errors() {
        let mut state = AutosyncFailureLogState::default();

        assert_eq!(
            state.failure_lines("parser failed", 12),
            vec!["autosync: sync failed after 12ms: parser failed"]
        );
        assert_eq!(
            state.failure_lines("parser failed", 13),
            vec!["autosync: sync still failing after 2 attempts: parser failed"]
        );
        assert!(state.failure_lines("parser failed", 14).is_empty());
        assert_eq!(
            state.failure_lines("parser failed", 15),
            vec!["autosync: sync still failing after 4 attempts: parser failed"]
        );
        assert_eq!(
            state.success_lines(),
            vec!["autosync: recovered after 4 failed attempts: parser failed"]
        );
        assert!(state.success_lines().is_empty());

        assert_eq!(
            state.failure_lines("state missing", 5),
            vec!["autosync: sync failed after 5ms: state missing"]
        );
        assert_eq!(
            state.failure_lines("state missing", 6),
            vec!["autosync: sync still failing after 2 attempts: state missing"]
        );
        assert_eq!(
            state.terminal_lines(),
            vec![
                "autosync: previous sync error reached 2 failed attempts before stop: state missing"
            ]
        );

        assert_eq!(
            state.failure_lines("error a", 7),
            vec!["autosync: sync failed after 7ms: error a"]
        );
        assert_eq!(
            state.failure_lines("error a", 8),
            vec!["autosync: sync still failing after 2 attempts: error a"]
        );
        assert_eq!(
            state.failure_lines("error b", 9),
            vec![
                "autosync: previous sync error reached 2 failed attempts before a different error: error a",
                "autosync: sync failed after 9ms: error b",
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_daemon_spawn_helpers_quote_args_and_override_state_dir() {
        let args = vec![
            OsString::from(r"C:\Program Files\RepoGrammar\repogrammar.exe"),
            OsString::from("--path"),
            OsString::from(r"C:\repo path"),
            OsString::from("a\"b"),
        ];
        let command_line = windows_command_line(&args);
        let rendered =
            String::from_utf16(&command_line[..command_line.len() - 1]).expect("valid utf16");
        assert_eq!(
            rendered,
            r#""C:\Program Files\RepoGrammar\repogrammar.exe" --path "C:\repo path" "a\"b""#
        );

        let environment =
            windows_environment_block(Some(r"C:\state dir"), Some("0123456789abcdef"))
                .expect("environment block");
        let rendered_environment = String::from_utf16(&environment).expect("valid environment");
        assert!(rendered_environment.contains("REPOGRAMMAR_DIR=C:\\state dir\0"));
        assert!(
            rendered_environment.contains("REPOGRAMMAR_AUTOSYNC_STARTUP_NONCE=0123456789abcdef\0")
        );
        assert!(rendered_environment.ends_with("\0\0"));
    }

    #[test]
    fn autosync_change_fingerprint_tracks_supported_sources_and_skips_noise() {
        let workspace = TempWorkspace::new("autosync-change-fingerprint");

        let empty =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("empty fingerprint");

        fs::create_dir_all(workspace.path().join(".repogrammar/logs")).expect("create state noise");
        fs::write(
            workspace.path().join(".repogrammar/logs/generated.ts"),
            "export const ignored = true;\n",
        )
        .expect("write state noise");
        let after_state_noise =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("state noise fingerprint");
        assert_eq!(after_state_noise, empty);

        fs::write(
            workspace.path().join("app.ts"),
            "export const tracked = 1;\n",
        )
        .expect("write tracked source");
        let after_source =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("source fingerprint");
        assert_ne!(after_source, empty);

        fs::write(workspace.path().join("notes.md"), "# ignored\n").expect("write ignored md");
        fs::create_dir_all(workspace.path().join("node_modules/pkg")).expect("create excluded dir");
        fs::write(
            workspace.path().join("node_modules/pkg/index.ts"),
            "export const ignored = true;\n",
        )
        .expect("write excluded source");
        let after_excluded_noise =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("excluded noise fingerprint");
        assert_eq!(after_excluded_noise, after_source);

        fs::create_dir(workspace.path().join(".bundle")).expect("create Ruby tool directory");
        fs::write(workspace.path().join(".bundle/cache.rb"), "ignored\n")
            .expect("write excluded Ruby source");
        let after_ruby_tool_noise =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("Ruby tool noise fingerprint");
        assert_eq!(after_ruby_tool_noise, after_source);

        fs::write(workspace.path().join("main.rb"), "puts :tracked\n")
            .expect("write tracked Ruby source");
        let after_ruby_source =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("Ruby source fingerprint");
        assert_ne!(after_ruby_source, after_source);

        fs::create_dir(workspace.path().join(".build")).expect("create Swift build directory");
        fs::create_dir(workspace.path().join(".swiftpm")).expect("create SwiftPM tool directory");
        fs::write(workspace.path().join(".build/cache.swift"), "ignored\n")
            .expect("write excluded Swift build source");
        fs::write(
            workspace.path().join(".swiftpm/Package.swift"),
            "// ignored SwiftPM manifest\n",
        )
        .expect("write excluded SwiftPM config");
        let after_swift_tool_noise =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("Swift tool noise fingerprint");
        assert_eq!(after_swift_tool_noise, after_ruby_source);

        fs::write(workspace.path().join("main.swift"), "let tracked = true\n")
            .expect("write tracked Swift source");
        let after_swift_source =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("Swift source fingerprint");
        assert_ne!(after_swift_source, after_swift_tool_noise);

        fs::write(
            workspace.path().join("Package@swift-6.3.swift"),
            "// swift-tools-version: 6.3\n",
        )
        .expect("write tracked Swift config");
        let after_swift_config =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("Swift config fingerprint");
        assert_ne!(after_swift_config, after_swift_source);

        fs::write(
            workspace.path().join(".build/tracked.ts"),
            "export const tracked = true;\n",
        )
        .expect("write tracked TypeScript source under Swift build directory");
        let after_build_cross_language =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("cross-language Swift build fingerprint");
        assert_ne!(after_build_cross_language, after_swift_config);

        fs::write(
            workspace.path().join(".swiftpm/tracked.py"),
            "tracked = True\n",
        )
        .expect("write tracked Python source under SwiftPM tool directory");
        let after_swiftpm_cross_language =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("cross-language SwiftPM fingerprint");
        assert_ne!(after_swiftpm_cross_language, after_build_cross_language);

        fs::write(
            workspace.path().join("app.ts"),
            "export const tracked = 12345;\n",
        )
        .expect("modify tracked source");
        let after_modified =
            repository_change_fingerprint(&workspace.path().display().to_string(), 1_048_576)
                .expect("modified fingerprint");
        assert_ne!(after_modified, after_swiftpm_cross_language);
    }

    fn release_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("typescript")
            .join("release")
            .join("v0_1")
    }

    fn python_release_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("python")
            .join("release")
            .join("v0_1")
    }

    fn release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("typescript")
            .join("release")
            .join("v0_2")
    }

    fn rust_release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("rust")
            .join("release")
            .join("v0_2")
    }

    fn java_release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("java")
            .join("release")
            .join("v0_2")
    }

    fn csharp_release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("csharp")
            .join("release")
            .join("v0_2")
    }

    fn cpp_release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("cpp")
            .join("release")
            .join("v0_2")
    }

    fn unknown_reduction_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("unknown_reduction")
    }

    fn copy_release_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&release_fixture_root().join(name), destination);
    }

    fn copy_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&release_fixture_v0_2_root().join(name), destination);
    }

    fn copy_rust_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&rust_release_fixture_v0_2_root().join(name), destination);
    }

    fn copy_java_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&java_release_fixture_v0_2_root().join(name), destination);
    }

    fn copy_csharp_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&csharp_release_fixture_v0_2_root().join(name), destination);
    }

    fn copy_cpp_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&cpp_release_fixture_v0_2_root().join(name), destination);
    }

    fn python_release_fixture_v0_2_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("fixtures")
            .join("python")
            .join("release")
            .join("v0_2")
    }

    fn copy_python_release_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&python_release_fixture_root().join(name), destination);
    }

    fn copy_python_release_v0_2_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&python_release_fixture_v0_2_root().join(name), destination);
    }

    fn copy_unknown_reduction_fixture(name: &str, destination: &Path) {
        copy_dir_contents(&unknown_reduction_fixture_root().join(name), destination);
    }

    fn copy_dir_contents(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("create fixture destination");
        let mut entries = fs::read_dir(source)
            .unwrap_or_else(|error| panic!("read fixture directory {source:?}: {error}"))
            .collect::<Result<Vec<_>, _>>()
            .expect("collect fixture entries");
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let file_type = entry.file_type().expect("fixture entry file type");
            let target = destination.join(entry.file_name());
            if file_type.is_dir() {
                copy_dir_contents(&entry.path(), &target);
            } else if file_type.is_file() {
                fs::copy(entry.path(), target).expect("copy fixture file");
            }
        }
    }

    fn parse_machine_output(
        command: &str,
        output: &repogrammar::interfaces::cli::CliOutput,
        workspace: &TempWorkspace,
    ) -> Value {
        assert_eq!(output.status, 0, "{command} stderr: {}", output.stderr);
        assert!(
            output.stderr.is_empty(),
            "{command} wrote stderr: {}",
            output.stderr
        );
        assert_no_output_leakage(command, &output.stdout, workspace);
        serde_json::from_str(output.stdout.trim())
            .unwrap_or_else(|error| panic!("parse {command} JSON: {error}"))
    }

    fn unknown_inventory_bucket_count(
        inventory_json: &Value,
        bucket_name: &str,
        key_name: &str,
        key: &str,
    ) -> u64 {
        inventory_json["unknown_inventory"][bucket_name]
            .as_array()
            .unwrap_or_else(|| panic!("{bucket_name} array"))
            .iter()
            .find(|bucket| bucket[key_name] == key)
            .and_then(|bucket| bucket["count"].as_u64())
            .unwrap_or(0)
    }

    fn required_mechanism_count(inventory_json: &Value, mechanism: &str) -> u64 {
        unknown_inventory_bucket_count(
            inventory_json,
            "by_required_mechanism",
            "required_mechanism",
            mechanism,
        )
    }

    fn unknown_inventory_count(inventory_json: &Value, field: &str) -> u64 {
        inventory_json["unknown_inventory"][field]
            .as_u64()
            .unwrap_or_else(|| panic!("unknown inventory numeric field {field}"))
    }

    struct UnknownReductionRun {
        workspace: TempWorkspace,
        unknowns_json: Value,
        unknowns_stdout: String,
        facts: IndexedSemanticFactsReport,
        families_json: Value,
    }

    fn run_unknown_reduction_fixture(
        runtime: &ProductCliRuntime,
        fixture_name: &str,
    ) -> UnknownReductionRun {
        let workspace = TempWorkspace::new(&format!("unknown-reduction-{fixture_name}"));
        copy_unknown_reduction_fixture(fixture_name, workspace.path());

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let resync = run_with_runtime(
            cli_args(
                "resync",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            runtime,
        );
        let resync_json = parse_machine_output("resync", &resync, &workspace);
        assert_eq!(resync_json["command"], "resync");
        assert_eq!(resync_json["status"], "complete");

        let unknowns =
            run_with_runtime(cli_args("unknowns", workspace.path(), &["--json"]), runtime);
        let unknowns_stdout = unknowns.stdout.clone();
        let unknowns_json = parse_machine_output("unknowns", &unknowns, &workspace);
        assert_eq!(unknowns_json["command"], "unknowns");
        assert_eq!(unknowns_json["status"], "ok");
        assert_eq!(
            unknowns_json["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open unknown reduction store");
        let facts = list_semantic_facts(&store).expect("list unknown reduction semantic facts");

        let families =
            run_with_runtime(cli_args("families", workspace.path(), &["--json"]), runtime);
        let families_json = parse_machine_output("families", &families, &workspace);

        UnknownReductionRun {
            workspace,
            unknowns_json,
            unknowns_stdout,
            facts,
            families_json,
        }
    }

    fn assert_unknown_reduction(
        label: &str,
        unresolved: &UnknownReductionRun,
        resolved: &UnknownReductionRun,
        mechanism: &str,
    ) {
        let unresolved_blocking =
            unknown_inventory_count(&unresolved.unknowns_json, "blocking_unknowns");
        let resolved_blocking =
            unknown_inventory_count(&resolved.unknowns_json, "blocking_unknowns");
        let unresolved_mechanism = required_mechanism_count(&unresolved.unknowns_json, mechanism);
        let resolved_mechanism = required_mechanism_count(&resolved.unknowns_json, mechanism);
        assert!(
            resolved_blocking < unresolved_blocking || resolved_mechanism < unresolved_mechanism,
            "{label} must reduce blocking UNKNOWNs or {mechanism} bucket: unresolved={unresolved_blocking}/{unresolved_mechanism}, resolved={resolved_blocking}/{resolved_mechanism}"
        );
    }

    fn assert_no_false_family(label: &str, run: &UnknownReductionRun) {
        assert!(
            run.families_json["families"]
                .as_array()
                .expect("families array")
                .is_empty(),
            "{label} unresolved fixture must not form a family: {}",
            run.families_json
        );
        assert_no_claim_payload(label, &run.families_json);
    }

    fn assert_no_unknown_output_fragments(run: &UnknownReductionRun, fragments: &[&str]) {
        assert_no_output_leakage("unknowns", &run.unknowns_stdout, &run.workspace);
        for fragment in fragments {
            assert!(
                !run.unknowns_stdout.contains(fragment),
                "unknowns JSON leaked source-like fragment {fragment}"
            );
        }
    }

    fn assert_source_backed_fact(
        facts: &IndexedSemanticFactsReport,
        label: &str,
        predicate: impl Fn(&IndexedSemanticFactRecord) -> bool,
    ) {
        let fact = facts
            .facts
            .iter()
            .find(|fact| predicate(fact))
            .unwrap_or_else(|| panic!("missing source-backed replacement fact for {label}"));
        assert!(!fact.path.is_empty(), "{label} fact path must not be empty");
        assert!(
            !fact.path.starts_with('/'),
            "{label} fact path must be repo-relative: {}",
            fact.path
        );
        assert!(
            !fact.path.split('/').any(|component| component == ".."),
            "{label} fact path must not traverse: {}",
            fact.path
        );
        assert!(
            fact.start_byte < fact.end_byte,
            "{label} fact must carry a non-empty source range"
        );
        let hash = fact.content_hash.as_str();
        assert!(
            hash.starts_with("sha256:") && hash.len() == "sha256:".len() + 64,
            "{label} fact must carry a sha256 content hash: {hash}"
        );
    }

    fn assert_no_output_leakage(command: &str, output: &str, workspace: &TempWorkspace) {
        assert!(
            !output.contains(workspace.path().to_string_lossy().as_ref()),
            "{command} leaked absolute workspace path: {output}"
        );
        assert!(
            !output.contains(release_fixture_root().to_string_lossy().as_ref()),
            "{command} leaked absolute fixture path: {output}"
        );
        assert!(
            !output.contains(python_release_fixture_root().to_string_lossy().as_ref()),
            "{command} leaked absolute Python fixture path: {output}"
        );
        assert!(
            !output.contains(
                python_release_fixture_v0_2_root()
                    .to_string_lossy()
                    .as_ref()
            ),
            "{command} leaked absolute Python v0.2 fixture path: {output}"
        );
        assert!(
            !output.contains(release_fixture_v0_2_root().to_string_lossy().as_ref()),
            "{command} leaked absolute v0.2 fixture path: {output}"
        );
        assert!(
            !output.contains(rust_release_fixture_v0_2_root().to_string_lossy().as_ref()),
            "{command} leaked absolute Rust v0.2 fixture path: {output}"
        );
        assert!(
            !output.contains(unknown_reduction_fixture_root().to_string_lossy().as_ref()),
            "{command} leaked absolute UNKNOWN reduction fixture path: {output}"
        );
        assert!(
            !output.contains(java_release_fixture_v0_2_root().to_string_lossy().as_ref()),
            "{command} leaked absolute Java v0.2 fixture path: {output}"
        );
        assert!(
            !output.contains(
                csharp_release_fixture_v0_2_root()
                    .to_string_lossy()
                    .as_ref()
            ),
            "{command} leaked absolute C# v0.2 fixture path: {output}"
        );
        assert!(
            !output.contains(cpp_release_fixture_v0_2_root().to_string_lossy().as_ref()),
            "{command} leaked absolute C/C++ v0.2 fixture path: {output}"
        );
        for snippet in [
            "app.get(",
            "app.route(",
            "NextResponse",
            "Response.json",
            "PrismaClient",
            "pgTable",
            "drizzle(",
            "db.select",
            "export function",
            "describe(",
            "expect(",
            "return <",
            "/accounts",
            "/users",
            "/health",
            "/lonely",
            "accounts: []",
            "users: []",
            "ok: true",
            "loading: false",
            "Promise.resolve",
            "props.name",
            "props.status",
            "toHaveLength",
            "toBe(true)",
            "from fastapi",
            "@router.",
            "response_model=",
            "BaseModel",
            "mapped_column",
            "@pytest.fixture",
            "importlib.import_module",
            "sys.path.append",
            "client.get(",
            "return {",
            "Depends(",
            "HTTPException",
            "DeclarativeBase",
            "select(",
            "getattr(",
            "setattr(",
            "cpython_ast",
            "STRUCTURAL",
            "FRAMEWORK_HEURISTIC",
            "python-fixture-provider",
            "release_fixture_semantic_support",
            "origin_engine",
            "pub fn support_",
            "fn validate_family",
            "#[cfg(",
            "macro_rules!",
            "@GetMapping",
            "@RestController",
            "ResponseEntity",
            "@Entity",
            "@Path(",
            "@ParameterizedTest",
            "@MethodSource",
            "@Mock",
            "[HttpGet",
            "[ApiController]",
            "[Fact]",
            "return Ok(",
            "IActionResult",
            "WebApplication.CreateBuilder",
            "Assert.True",
            "TEST(",
            "TEST_CASE(",
            "EXPECT_TRUE",
            "EXPECT_EQ",
            "CHECK(",
            "#include <gtest",
            "#ifdef ENABLE",
            "BOOST_AUTO_TEST",
            "Q_OBJECT",
            "SIGNAL(",
            "from django",
            "models.Model",
            "models.CharField",
            "@app.route",
            "urlpatterns",
            "unittest.TestCase",
            "assertEqual",
            "@shared_task",
            "@click.command",
            "typer.Typer",
        ] {
            assert!(
                !output.contains(snippet),
                "{command} leaked source-like snippet {snippet}: {output}"
            );
        }
    }

    fn assert_unknown_query_json(command: &str, value: &Value) {
        assert_eq!(value["command"], command);
        if matches!(command, "find" | "explain" | "check") && value["status"] == "PARTIAL_CONTEXT" {
            assert_eq!(value["implemented"], true);
            assert_eq!(value["read_plan"]["source_snippets_included"], false);
            assert_eq!(value["read_plan"]["requires_source_before_edit"], true);
            assert_eq!(
                value["unknowns"][0]["affected_claim"],
                "pattern family evidence for resolved target"
            );
            return;
        }
        assert_eq!(value["status"], "UNKNOWN");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["unknowns"][0]["reason"], "InsufficientSupport");
    }

    fn assert_no_claim_payload(command: &str, value: &Value) {
        if let Some(families) = value.get("families") {
            assert!(
                families.as_array().expect("families array").is_empty(),
                "{command} UNKNOWN leaked families: {value}"
            );
        }
        let forbidden_fields: &[&str] = if value["status"] == "PARTIAL_CONTEXT" {
            &["family", "member", "members", "variation_slots", "evidence"]
        } else {
            &[
                "family",
                "member",
                "members",
                "variation_slots",
                "evidence",
                "output",
                "check",
                "read_plan",
            ]
        };
        for field in forbidden_fields {
            assert!(
                value.get(field).is_none(),
                "{command} no-claim response leaked claim field {field}: {value}"
            );
        }
        if value["status"] == "PARTIAL_CONTEXT" && command != "check" {
            assert!(
                value.get("check").is_none(),
                "{command} partial context should not include advisory check metadata: {value}"
            );
        }
    }

    fn assert_stored_python_structural_fact(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        path: &str,
        kind: &str,
        target: &str,
        anchor_kind: &str,
    ) {
        let expected_anchor = format!("python_anchor_kind={anchor_kind}");
        assert!(
            facts.iter().any(|fact| {
                fact.path == path
                    && fact.kind == kind
                    && fact.target.as_deref() == Some(target)
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "STRUCTURAL"
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &expected_anchor)
            }),
            "missing Python structural fact {kind} {target} with anchor {anchor_kind}"
        );
    }

    fn assert_no_derived_python_support_for_targets(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        targets: &[&str],
    ) {
        for target in targets {
            assert!(
                facts.iter().all(|fact| {
                    !(fact.origin_engine == "repogrammar-python-derived"
                        && fact.origin_method == "bounded_ast_anchor_v1"
                        && fact.target.as_deref() == Some(*target))
                }),
                "auxiliary target {target} must not be derived family support"
            );
        }
    }

    fn assert_no_dynamic_boundary_fact_leakage(
        workspace: &TempWorkspace,
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
    ) {
        let debug = format!("{facts:?}");
        for forbidden in [
            workspace.path().to_string_lossy().as_ref(),
            release_fixture_root().to_string_lossy().as_ref(),
            python_release_fixture_root().to_string_lossy().as_ref(),
            "importlib.import_module",
            "sys.path.append",
            "getattr(module",
            "secret=(str",
            "decorator_factory(\"secret\")",
            "setattr(target",
            "Depends(make_dependency",
            "return getattr",
            "django_db",
            "return object",
        ] {
            assert!(
                !debug.contains(forbidden),
                "leaked forbidden dynamic source text {forbidden}"
            );
        }
        for fact in facts {
            assert_repo_relative_json_path(&Value::String(fact.path.clone()));
        }
    }

    fn assert_stored_python_unknown_fact(
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        path: &str,
        reason_code: &str,
        affected_claim: &str,
    ) {
        let reason_assumption = format!("reason_code={reason_code}");
        let claim_assumption = format!("affected_claim={affected_claim}");
        assert!(
            facts.iter().any(|fact| {
                fact.path == path
                    && fact.kind == "UNKNOWN"
                    && fact.target.as_deref() == Some(reason_code)
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "UNKNOWN"
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &reason_assumption)
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == &claim_assumption)
            }),
            "missing Python UNKNOWN {reason_code} for {affected_claim}"
        );
    }

    fn assert_targets_blocked_from_claim_input(
        workspace: &TempWorkspace,
        store: &impl repogrammar::ports::index_store::IndexStore,
        facts: &[repogrammar::ports::index_store::IndexedSemanticFactRecord],
        targets: &[&str],
    ) {
        let fact_ids = facts
            .iter()
            .filter(|fact| targets.contains(&fact.target.as_deref().unwrap_or_default()))
            .map(|fact| fact.fact_id.clone())
            .collect::<BTreeSet<_>>();
        for target in targets {
            assert!(
                facts
                    .iter()
                    .any(|fact| fact.target.as_deref() == Some(*target)),
                "missing persisted target {target}"
            );
        }

        let readiness = assess_semantic_fact_readiness(
            SemanticFactReadinessRequest {
                repository_root: workspace.path().display().to_string(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            store,
            &FilesystemSourceStore,
        )
        .expect("assess auxiliary fact readiness");
        let mut checked = BTreeSet::new();
        for fact in readiness.facts {
            if !fact_ids.contains(&fact.fact_id) {
                continue;
            }
            checked.insert(fact.fact_id);
            let ClaimInputReadiness::Blocked { unknown } = fact.readiness else {
                panic!("auxiliary fact must stay blocked from claim input");
            };
            assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
        }
        assert_eq!(checked, fact_ids);
    }

    #[derive(Debug, Clone, Copy)]
    struct PythonExactAnchorSmokeCase {
        fixture: &'static str,
        family_id: &'static str,
        support_target: &'static str,
        evidence_path: &'static str,
        member_role: &'static str,
    }

    const PYTHON_EXACT_ANCHOR_SMOKE_CASES: &[PythonExactAnchorSmokeCase] = &[
        PythonExactAnchorSmokeCase {
            fixture: "positive-strong-evidence",
            family_id: "family:python:fastapi_route:framework_fastapi_route",
            support_target: "fastapi.APIRouter.get",
            evidence_path: "routes.py",
            member_role: "framework:fastapi.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "fastapi-alias-strong-evidence",
            family_id: "family:python:fastapi_route:framework_fastapi_route",
            support_target: "fastapi.APIRouter.get",
            evidence_path: "routes.py",
            member_role: "framework:fastapi.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "stale-evidence",
            family_id: "family:python:fastapi_route:framework_fastapi_route",
            support_target: "fastapi.APIRouter.get",
            evidence_path: "routes.py",
            member_role: "framework:fastapi.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pytest-strong-evidence",
            family_id: "family:python:pytest_test:framework_pytest_test",
            support_target: "pytest.test",
            evidence_path: "test_api.py",
            member_role: "framework:pytest.test",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pytest-fixture-alias-strong-evidence",
            family_id: "family:python:pytest_fixture:framework_pytest_fixture",
            support_target: "pytest.fixture",
            evidence_path: "conftest.py",
            member_role: "framework:pytest.fixture",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-basic",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic.BaseModel",
            evidence_path: "schemas.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-settings-strong-evidence",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic.BaseSettings",
            evidence_path: "settings.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "pydantic-settings-package-strong-evidence",
            family_id: "family:python:pydantic_model:framework_pydantic_model",
            support_target: "pydantic_settings.BaseSettings",
            evidence_path: "settings.py",
            member_role: "framework:pydantic.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.select",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.execute",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-scalar-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.scalar",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-scalars-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.scalars",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-get-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.get",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-async-session-get-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.ext.asyncio.AsyncSession.get",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-commit-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.commit",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-session-rollback-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.orm.Session.rollback",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-async-session-commit-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.ext.asyncio.AsyncSession.commit",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-async-session-rollback-strong-evidence",
            family_id:
                "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method",
            support_target: "sqlalchemy.ext.asyncio.AsyncSession.rollback",
            evidence_path: "repository.py",
            member_role: "framework:sqlalchemy.repository_method",
        },
        PythonExactAnchorSmokeCase {
            fixture: "sqlalchemy-model-strong-evidence",
            family_id: "family:python:sqlalchemy_model:framework_sqlalchemy_model",
            support_target: "sqlalchemy.orm.Mapped",
            evidence_path: "models.py",
            member_role: "framework:sqlalchemy.model",
        },
    ];

    fn assert_content_hash_json(value: &Value) {
        let hash = value.as_str().expect("content hash string");
        let Some(hex) = hash.strip_prefix("sha256:") else {
            panic!("hash missing sha256 prefix: {hash}");
        };
        assert_eq!(hex.len(), 64, "sha256 hash length");
        assert!(
            hex.as_bytes().iter().all(|byte| byte.is_ascii_hexdigit()),
            "sha256 hash must be hex: {hash}"
        );
    }

    fn assert_repo_relative_json_path(value: &Value) {
        let path = value.as_str().expect("path string");
        assert!(!path.is_empty(), "path must not be empty");
        assert!(!path.starts_with('/'), "path must be relative: {path}");
        assert!(
            !path.split('/').any(|component| component == ".."),
            "path must not traverse: {path}"
        );
        assert!(
            !path.contains('\\'),
            "path must use forward slashes: {path}"
        );
        assert!(!path.contains("://"), "path must not be a URI: {path}");
        assert!(
            !path
                .as_bytes()
                .windows(2)
                .any(|window| window[0].is_ascii_alphabetic() && window[1] == b':'),
            "path must not be Windows-absolute: {path}"
        );
    }

    fn assert_python_exact_anchor_family_detail(
        command: &str,
        value: &Value,
        case: PythonExactAnchorSmokeCase,
    ) -> String {
        assert_eq!(value["command"], command);
        assert_eq!(value["implemented"], true);
        assert_eq!(value["family"]["family_id"], case.family_id);
        assert_eq!(value["family"]["classification"], "DOMINANT_PATTERN");
        assert_eq!(value["family"]["support"], 3);
        assert_eq!(value["output"]["mode"], "compact");
        assert_eq!(value["output"]["estimated_evidence_tokens"], 0);
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert!(value["evidence"].as_array().expect("evidence").is_empty());
        assert_python_read_plan(command, value, case);
        assert_eq!(value["unknowns"][0]["reason"], "FrameworkMagic");
        assert_eq!(value["unknowns"][0]["class"], "non_blocking_unknown");
        let members = value["members"].as_array().expect("members");
        assert_eq!(members.len(), 3);
        assert!(members.iter().all(|member| {
            member["family_id"] == case.family_id && member["role"] == case.member_role
        }));
        members[0]["code_unit_id"]
            .as_str()
            .expect("member code unit id")
            .to_string()
    }

    fn assert_python_exact_anchor_evidence(
        command: &str,
        value: &Value,
        case: PythonExactAnchorSmokeCase,
        mode: &str,
        token_budget: Option<u64>,
    ) {
        assert_eq!(value["command"], command);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["implemented"], true);
        assert_eq!(value["family"]["family_id"], case.family_id);
        assert_eq!(value["family"]["support"], 3);
        assert_eq!(value["output"]["mode"], mode);
        match token_budget {
            Some(token_budget) => assert_eq!(value["output"]["token_budget"], token_budget),
            None => assert!(value["output"]["token_budget"].is_null()),
        }
        assert_eq!(value["output"]["source_snippets_included"], false);
        assert_eq!(
            value["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_python_read_plan(command, value, case);
        assert_eq!(
            value["output"]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_eq!(value["output"]["missing_claims"], serde_json::json!([]));
        let evidence = value["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["family_id"], case.family_id);
        assert_eq!(evidence[0]["path"], case.evidence_path);
        assert_eq!(
            evidence[0]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_repo_relative_json_path(&evidence[0]["path"]);
        assert_content_hash_json(&evidence[0]["content_hash"]);
        assert!(
            evidence[0]["start_byte"].as_u64().expect("start")
                < evidence[0]["end_byte"].as_u64().expect("end")
        );
    }

    fn assert_python_read_plan(command: &str, value: &Value, case: PythonExactAnchorSmokeCase) {
        let read_plan = &value["read_plan"];
        assert_eq!(read_plan["source_snippets_included"], false);
        assert_eq!(
            read_plan["requires_source_before_edit"],
            command != "family"
        );
        assert!(
            read_plan["estimated_tokens"]
                .as_u64()
                .expect("read plan tokens")
                > 0
        );
        let items = read_plan["items"].as_array().expect("read plan items");
        assert!(!items.is_empty());
        let first = &items[0];
        assert_eq!(first["path"], case.evidence_path);
        assert_repo_relative_json_path(&first["path"]);
        assert_content_hash_json(&first["content_hash"]);
        assert!(
            first["start_byte"].as_u64().expect("start") < first["end_byte"].as_u64().expect("end")
        );
        assert_read_plan_item_has_line_range(first);
        assert!(read_plan["line_range_omissions"]
            .as_array()
            .expect("line range omissions")
            .is_empty());
        assert_eq!(first["source_snippets_included"], false);
        assert!(!value.to_string().contains("def "));
        assert!(!value.to_string().contains("/tmp/"));
    }

    fn assert_read_plan_item_has_line_range(item: &Value) {
        let start_line = item["start_line"].as_u64().expect("start line");
        let end_line = item["end_line"].as_u64().expect("end line");
        assert!(start_line > 0, "start line must be 1-based");
        assert!(
            end_line >= start_line,
            "end line must be greater than or equal to start line"
        );
    }

    fn assert_python_stale_unknown(command: &str, value: &Value, family_id: &str) {
        assert_eq!(value["command"], command);
        assert_eq!(value["status"], "UNKNOWN");
        assert_eq!(value["implemented"], true);
        assert!(
            value.get("family").is_none(),
            "stale output must not include family"
        );
        assert!(
            value.get("members").is_none(),
            "stale output must not include members"
        );
        assert!(
            value.get("evidence").is_none(),
            "stale output must not include evidence"
        );
        assert!(
            value.get("check").is_none(),
            "stale output must not include check"
        );
        assert!(
            value.get("read_plan").is_none(),
            "stale output must not include read_plan"
        );
        assert_eq!(value["unknowns"][0]["class"], "blocking_unknown");
        assert_eq!(value["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            value["unknowns"][0]["affected_claim"],
            format!("{family_id}:evidence_freshness")
        );
        assert_eq!(value["unknowns"][0]["recovery"], "run repogrammar resync");
    }

    fn mcp_context_payload(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
        arguments: Value,
    ) -> Value {
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let payload = handle_context_call(runtime, &context, &arguments).expect("MCP payload");
        let payload_text = payload.to_string();
        assert_no_output_leakage("mcp", &payload_text, workspace);
        payload
    }

    #[cfg(unix)]
    fn language_from_discovered(language: DiscoveredLanguage) -> Language {
        match language {
            DiscoveredLanguage::TypeScript | DiscoveredLanguage::TypeScriptReact => {
                Language::TypeScript
            }
            DiscoveredLanguage::JavaScript | DiscoveredLanguage::JavaScriptReact => {
                Language::JavaScript
            }
            DiscoveredLanguage::Python => Language::Python,
            DiscoveredLanguage::PythonConfig => Language::PythonConfig,
            DiscoveredLanguage::TsJsConfig => Language::TsJsConfig,
            DiscoveredLanguage::Java => Language::Java,
            DiscoveredLanguage::CSharp => Language::CSharp,
            DiscoveredLanguage::C => Language::C,
            DiscoveredLanguage::Cpp => Language::Cpp,
            DiscoveredLanguage::CppConfig => Language::CppConfig,
            DiscoveredLanguage::Go => Language::Go,
            DiscoveredLanguage::GoConfig => Language::GoConfig,
            DiscoveredLanguage::Php => Language::Php,
            DiscoveredLanguage::PhpConfig => Language::PhpConfig,
            DiscoveredLanguage::Ruby => Language::Ruby,
            DiscoveredLanguage::RubyConfig => Language::RubyConfig,
            DiscoveredLanguage::Swift => Language::Swift,
            DiscoveredLanguage::SwiftConfig => Language::SwiftConfig,
            DiscoveredLanguage::Rust => Language::Rust,
            DiscoveredLanguage::RustConfig => Language::RustConfig,
        }
    }

    #[cfg(unix)]
    fn semantic_support_worker_script(workspace: &TempWorkspace) -> PathBuf {
        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files for worker fixture");
        let parser = RepoGrammarSourceParser::default();
        let mut messages = Vec::new();
        for file in report.files {
            let source = FilesystemSourceStore
                .read_source(SourceReadRequest {
                    repository_root: workspace.path().display().to_string(),
                    path: file.path.clone(),
                    expected_content_hash: file.content_hash.clone(),
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                })
                .expect("read source for worker fixture");
            let parsed = parser
                .parse(SourceDocument {
                    path: &source.path,
                    language: language_from_discovered(file.language),
                    content_hash: source.content_hash.clone(),
                    repository_revision: RepositoryRevision::new("UNKNOWN")
                        .expect("valid revision"),
                    text: &source.text,
                })
                .expect("parse source for worker fixture");
            for unit in parsed.units.into_iter() {
                let Some((target, engine, engine_version, method, note)) =
                    semantic_support_for_unit(&unit.kind)
                else {
                    continue;
                };
                messages.push(serde_json::json!({
                    "protocol_version": 1,
                    "message_type": "fact",
                    "request_id": "repogrammar-typescript-semantic-worker",
                    "fact_kind": "RESOLVED_IMPORT",
                    "subject": format!("{}#semantic-support", unit.id.as_str()),
                    "target": target,
                    "origin": {
                        "engine": engine,
                        "engine_version": engine_version,
                        "method": method
                    },
                    "certainty": "SEMANTIC",
                    "evidence": {
                        "code_unit_id": unit.id.as_str(),
                        "path": unit.provenance.path,
                        "content_hash": unit.provenance.content_hash.as_str(),
                        "repository_revision": "UNKNOWN",
                        "start_byte": unit.range.start_byte,
                        "end_byte": unit.range.end_byte,
                        "note": note
                    },
                    "assumptions": []
                }));
            }
        }
        messages.push(serde_json::json!({
            "protocol_version": 1,
            "message_type": "end_of_stream",
            "request_id": "repogrammar-typescript-semantic-worker"
        }));
        let ndjson = messages
            .into_iter()
            .map(|message| message.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let worker_script = workspace.path().join("semantic-support-worker.sh");
        fs::write(
            &worker_script,
            format!("#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'EOF'\n{ndjson}\nEOF\n"),
        )
        .expect("write semantic support worker");
        worker_script
    }

    #[cfg(unix)]
    fn semantic_support_for_unit(
        kind: &CodeUnitKind,
    ) -> Option<(
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    )> {
        match kind {
            CodeUnitKind::ExpressRoute => Some((
                "package:express",
                "typescript",
                "6.0.0",
                "compiler_api",
                "compiler resolved Express import target",
            )),
            CodeUnitKind::FastApiRoute => Some((
                "fastapi.APIRouter.get",
                "python-fixture-provider",
                "0.1.0",
                "release_fixture_semantic_support",
                "provider resolved FastAPI route decorator",
            )),
            _ => None,
        }
    }

    #[cfg(unix)]
    fn executable_script(workspace: &TempWorkspace, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = workspace.path().join(name);
        fs::write(&path, body).expect("write executable script");
        let mut permissions = fs::metadata(&path)
            .expect("read executable script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("set executable script mode");
        path
    }

    #[test]
    fn release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        // These v0.1 fixtures are object-literal/react lookalikes that must stay
        // UNKNOWN. `jest-vitest-basic` was moved out of this no-inflation baseline
        // because it contains genuine ambient `describe`/`it`/`test` calls in
        // `.test.ts`/`.spec.ts` files, which v0.2 conservative ambient support
        // legitimately resolves into families (covered by
        // `tsjs_v0_1_jest_vitest_basic_ambient_tests_form_families`).
        const RELEASE_FIXTURES: &[(&str, &str)] = &[
            ("express-basic", "users.ts"),
            ("react-basic", "UserCard.tsx"),
            ("mixed-js-ts", "routes.js"),
            ("unknown-low-support", "lonely-route.ts"),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("release-{fixture}"));
            copy_release_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["generation_id"], "gen-000001");
            assert_eq!(index_json["indexing"], "syntax_only_code_units");
            assert_eq!(index_json["parser"], "syntax_only");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["mining"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["command"], "files");
            assert_eq!(files_json["status"], "ok");
            assert_eq!(files_json["implemented"], true);
            assert_eq!(files_json["active_generation"], "gen-000001");
            assert_eq!(files_json["indexing"], "syntax_only_code_units");
            assert!(
                !files_json["files"]
                    .as_array()
                    .expect("files array")
                    .is_empty(),
                "fixture {fixture} should report indexed files"
            );

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            assert_eq!(units_json["command"], "units");
            assert_eq!(units_json["status"], "ok");
            assert_eq!(units_json["implemented"], true);
            assert_eq!(units_json["active_generation"], "gen-000001");
            assert_eq!(units_json["indexing"], "syntax_only_code_units");
            assert_eq!(units_json["semantic_worker"], "deferred");
            assert_eq!(units_json["mining"], "deferred");
            assert!(
                !units_json["units"]
                    .as_array()
                    .expect("units array")
                    .is_empty(),
                "fixture {fixture} should report indexed units"
            );

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
            assert!(doctor_json["checks"].get("schema_version").is_none());

            if *fixture == "low-support" {
                let stats =
                    run_with_runtime(cli_args("stats", workspace.path(), &["--json"]), &runtime);
                let stats_json = parse_machine_output("stats", &stats, &workspace);
                assert_eq!(stats_json["status"], "ok");
                assert_eq!(stats_json["token_savings"], Value::Null);
                assert!(
                    stats_json["counts"]["eligible_code_units"]
                        .as_u64()
                        .unwrap_or_default()
                        > 0,
                    "low-support fixture should still have analyzable Python units"
                );
                assert_eq!(stats_json["counts"]["covered_code_units"], 0);
                assert_eq!(stats_json["metrics"]["token_saving_risk"], "high");
            }
        }
    }

    #[test]
    fn python_release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        const RELEASE_FIXTURES: &[(&str, &str, &[&str])] = &[
            (
                "fastapi-basic",
                "app.py",
                &["fastapi_route", "pydantic_model"],
            ),
            (
                "pytest-basic",
                "test_users.py",
                &["pytest_fixture", "pytest_test"],
            ),
            (
                "mixed-python",
                "api.py",
                &["fastapi_route", "pydantic_model", "pytest_test"],
            ),
            (
                "dynamic-unknown",
                "dynamic.py",
                &["function", "fastapi_route"],
            ),
            ("low-support", "lonely.py", &["fastapi_route"]),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target, expected_kinds) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("python-release-{fixture}"));
            copy_python_release_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["generation_id"], "gen-000001");
            assert_eq!(index_json["indexing"], "syntax_only_code_units");
            assert_eq!(index_json["parser"], "syntax_only");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["mining"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["command"], "files");
            assert_eq!(files_json["status"], "ok");
            assert_eq!(files_json["implemented"], true);
            assert_eq!(files_json["active_generation"], "gen-000001");
            assert_eq!(files_json["indexing"], "syntax_only_code_units");
            assert!(
                !files_json["files"]
                    .as_array()
                    .expect("files array")
                    .is_empty(),
                "fixture {fixture} should report indexed files"
            );

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            assert_eq!(units_json["command"], "units");
            assert_eq!(units_json["status"], "ok");
            assert_eq!(units_json["implemented"], true);
            assert_eq!(units_json["active_generation"], "gen-000001");
            assert_eq!(units_json["indexing"], "syntax_only_code_units");
            assert_eq!(units_json["semantic_worker"], "deferred");
            assert_eq!(units_json["mining"], "deferred");
            let unit_kinds = units_json["units"]
                .as_array()
                .expect("units array")
                .iter()
                .filter(|unit| unit["language"] == "python")
                .filter_map(|unit| unit["kind"].as_str())
                .collect::<Vec<_>>();
            for expected_kind in *expected_kinds {
                assert!(
                    unit_kinds.contains(expected_kind),
                    "fixture {fixture} should include Python unit kind {expected_kind}; got {unit_kinds:?}"
                );
            }

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
            assert!(doctor_json["checks"].get("schema_version").is_none());
        }
    }

    #[test]
    fn csharp_release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        const RELEASE_FIXTURES: &[(&str, &str, &[&str])] = &[
            (
                "framework_lookalikes",
                "Controllers/LookalikeController.cs",
                &["class", "method"],
            ),
            (
                "preprocessor_variant_unknown",
                "Controllers/VariantController.cs",
                &["aspnet_controller", "aspnet_controller_action"],
            ),
            (
                "low_support",
                "Controllers/SingleController.cs",
                &["aspnet_controller", "aspnet_controller_action"],
            ),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target, expected_kinds) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("csharp-release-{fixture}"));
            copy_csharp_release_v0_2_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["status"], "ok");
            assert!(!files_json["files"]
                .as_array()
                .expect("files array")
                .is_empty());

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            let unit_kinds = units_json["units"]
                .as_array()
                .expect("units array")
                .iter()
                .filter(|unit| unit["language"] == "csharp")
                .filter_map(|unit| unit["kind"].as_str())
                .collect::<Vec<_>>();
            for expected_kind in *expected_kinds {
                assert!(
                    unit_kinds.contains(expected_kind),
                    "fixture {fixture} should include C# unit kind {expected_kind}; got {unit_kinds:?}"
                );
            }

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
        }
    }

    #[test]
    fn cpp_release_fixtures_default_product_smoke_returns_json_without_claim_inflation() {
        const RELEASE_FIXTURES: &[(&str, &str, &[&str])] = &[
            (
                "test_macro_lookalikes",
                "tests/lookalikes_test.cc",
                &["function"],
            ),
            (
                "preprocessor_variant_unknown",
                "tests/variant_test.cc",
                &["gtest_test_case"],
            ),
            ("low_support", "tests/single_test.cc", &["gtest_test_case"]),
        ];
        const QUERY_COMMANDS: &[&str] =
            &["families", "family", "member", "find", "explain", "check"];

        for (fixture, target, expected_kinds) in RELEASE_FIXTURES {
            let workspace = TempWorkspace::new(&format!("cpp-release-{fixture}"));
            copy_cpp_release_v0_2_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert!(
                index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
                "fixture {fixture} should index at least one unit"
            );

            let files =
                run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
            let files_json = parse_machine_output("files", &files, &workspace);
            assert_eq!(files_json["status"], "ok");
            assert!(!files_json["files"]
                .as_array()
                .expect("files array")
                .is_empty());

            let units =
                run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
            let units_json = parse_machine_output("units", &units, &workspace);
            let unit_kinds = units_json["units"]
                .as_array()
                .expect("units array")
                .iter()
                .filter(|unit| unit["language"] == "c" || unit["language"] == "cpp")
                .filter_map(|unit| unit["kind"].as_str())
                .collect::<Vec<_>>();
            for expected_kind in *expected_kinds {
                assert!(
                    unit_kinds.contains(expected_kind),
                    "fixture {fixture} should include C/C++ unit kind {expected_kind}; got {unit_kinds:?}"
                );
            }

            for command in QUERY_COMMANDS {
                let output = if *command == "families" {
                    run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
                } else {
                    run_with_runtime(
                        cli_args(command, workspace.path(), &[target, "--json"]),
                        &runtime,
                    )
                };
                let value = parse_machine_output(command, &output, &workspace);
                assert_unknown_query_json(command, &value);
            }

            let doctor =
                run_with_runtime(cli_args("doctor", workspace.path(), &["--json"]), &runtime);
            let doctor_json = parse_machine_output("doctor", &doctor, &workspace);
            assert_eq!(doctor_json["command"], "doctor");
            assert_eq!(doctor_json["checks"]["storage"], "available");
        }
    }

    #[test]
    fn python_release_dynamic_boundaries_persist_unknowns_without_claims() {
        let workspace = TempWorkspace::new("python-release-dynamic-boundaries");
        copy_python_release_fixture("dynamic-unknown", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        for (reason_code, affected_claim) in [
            ("DynamicImport", "python_import_resolution"),
            ("RuntimeDependencyInjection", "python_import_resolution"),
            ("RuntimeDependencyInjection", "fastapi_dependency_target"),
            ("ConflictingFacts", "pytest_fixture_binding"),
            ("PytestFixtureInjection", "pytest_fixture_binding"),
            ("FrameworkMagic", "python_call_target"),
            ("FrameworkMagic", "python_framework_identity"),
            ("MonkeyPatch", "python_call_target"),
        ] {
            let path = match reason_code {
                "ConflictingFacts" | "PytestFixtureInjection" => {
                    "tests/sub/test_fixture_boundaries.py"
                }
                _ => "dynamic.py",
            };
            assert_stored_python_unknown_fact(&facts.facts, path, reason_code, affected_claim);
        }
        for target in [
            "pytest.builtin_fixture.tmp_path",
            "pytest.builtin_fixture.capsys",
        ] {
            assert_stored_python_structural_fact(
                &facts.facts,
                "tests/sub/test_fixture_boundaries.py",
                "SYMBOL",
                target,
                "pytest_builtin_fixture_context",
            );
        }
        let framework_identity_unknowns = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.path == "dynamic.py"
                    && fact.kind == "UNKNOWN"
                    && fact.target.as_deref() == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert!(
            framework_identity_unknowns >= 3,
            "dynamic decorator, dynamic pydantic model factory, and unresolved decorator must remain UNKNOWN"
        );
        assert!(facts.facts.iter().all(|fact| {
            !(fact.path == "tests/sub/test_fixture_boundaries.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                }))
        }));
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &[
                "DynamicImport",
                "RuntimeDependencyInjection",
                "ConflictingFacts",
                "PytestFixtureInjection",
                "FrameworkMagic",
                "MonkeyPatch",
            ],
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &[
                "DynamicImport",
                "RuntimeDependencyInjection",
                "ConflictingFacts",
                "PytestFixtureInjection",
                "pytest.builtin_fixture.tmp_path",
                "pytest.builtin_fixture.capsys",
                "pytest.fixture.client",
                "FrameworkMagic",
                "MonkeyPatch",
                "pydantic.create_model",
                "pydantic.BaseModel",
                "unknown_policy",
            ],
        );

        assert_no_dynamic_boundary_fact_leakage(&workspace, &facts.facts);

        for command in ["families", "find", "family", "member", "explain", "check"] {
            let output = if command == "families" {
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
            } else {
                run_with_runtime(
                    cli_args(command, workspace.path(), &["dynamic.py", "--json"]),
                    &runtime,
                )
            };
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
            assert_no_claim_payload(command, &value);
        }
    }

    #[test]
    fn python_release_dynamic_pytest_fixture_name_stays_unknown_without_claims() {
        let workspace = TempWorkspace::new("python-release-dynamic-fixture-name");
        copy_python_release_fixture("pytest-dynamic-fixture-name", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_stored_python_unknown_fact(
            &facts.facts,
            "conftest.py",
            "PytestFixtureInjection",
            "pytest_fixture_binding",
        );
        assert_stored_python_unknown_fact(
            &facts.facts,
            "test_fixture_names.py",
            "PytestFixtureInjection",
            "pytest_fixture_binding",
        );
        assert!(facts.facts.iter().all(|fact| {
            !(fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.dynamic_client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_fixture_edge"
                        || assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                }))
        }));
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &["PytestFixtureInjection"],
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &["pytest.fixture.dynamic_client", "PytestFixtureInjection"],
        );
        assert_no_dynamic_boundary_fact_leakage(&workspace, &facts.facts);

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_unknown_query_json("families", &families_json);
        assert_no_claim_payload("families", &families_json);
    }

    #[cfg(unix)]
    #[test]
    fn product_runtime_strong_worker_support_produces_family_then_stale_unknown() {
        let workspace = TempWorkspace::new("product-runtime-positive-family");
        fs::write(
            workspace.path().join("users.ts"),
            "import express from 'express';\nconst app = express();\napp.get('/users', function listUsers(req, res) { res.json([]); });\n",
        )
        .expect("write users route");
        fs::write(
            workspace.path().join("accounts.ts"),
            "import express from 'express';\nconst app = express();\napp.get('/accounts', function listAccounts(req, res) { res.json([]); });\n",
        )
        .expect("write accounts route");
        fs::write(
            workspace.path().join("orders.ts"),
            "import express from 'express';\nconst app = express();\napp.get('/orders', function listOrders(req, res) { res.json([]); });\n",
        )
        .expect("write orders route");
        let worker_script = semantic_support_worker_script(&workspace);
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                    strict_gitignore: false,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                    progress: ProgressMode::Never,
                    json: false,
                    quiet: true,
                    stderr_is_terminal: false,
                },
            )
            .expect("index with semantic support worker");
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::Complete
        );
        assert_eq!(outcome.semantic_facts, 12);

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        let family_id = families_json["families"][0]["family_id"]
            .as_str()
            .expect("family id")
            .to_string();

        let family = run_with_runtime(
            cli_args("family", workspace.path(), &[&family_id, "--json"]),
            &runtime,
        );
        let family_json = parse_machine_output("family", &family, &workspace);
        assert_eq!(family_json["status"], "ok");
        assert_eq!(family_json["family"]["family_id"], family_id);

        let check = run_with_runtime(
            cli_args("check", workspace.path(), &["users.ts", "--json"]),
            &runtime,
        );
        let check_json = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_json["status"], "CONTEXT_ONLY");
        assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");

        fs::write(
            workspace.path().join("users.ts"),
            "app.get('/users', function listChanged(req, res) { res.json(['changed']); });\n",
        )
        .expect("mutate users route");

        let stale = run_with_runtime(
            cli_args("family", workspace.path(), &[&family_id, "--json"]),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_eq!(stale_json["status"], "UNKNOWN");
        assert_eq!(stale_json["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            stale_json["unknowns"][0]["recovery"],
            "run repogrammar resync"
        );
    }

    #[test]
    fn families_listing_verifies_evidence_freshness_per_family() {
        let workspace = TempWorkspace::new("families-freshness-stale");
        // Two independent framework families in one repository, each backed by
        // its own single evidence file, so one file's mutation isolates to one
        // family while the other stays verifiable.
        copy_python_release_v0_2_fixture("django_exact_models", &workspace.path().join("alpha"));
        copy_python_release_v0_2_fixture("flask_exact_routes", &workspace.path().join("beta"));
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("index", &index, &workspace)["status"],
            "complete"
        );

        // Freshly indexed: every family verifies fresh against the tree.
        let fresh = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let fresh_json = parse_machine_output("families", &fresh, &workspace);
        assert_eq!(fresh_json["status"], "ok");
        let fresh_families = fresh_json["families"].as_array().expect("families array");
        assert!(fresh_families.iter().any(|family| family["family_id"]
            .as_str()
            .is_some_and(|id| id.contains("django_model"))));
        assert!(fresh_families.iter().any(|family| family["family_id"]
            .as_str()
            .is_some_and(|id| id.contains("flask_route"))));
        assert!(fresh_families
            .iter()
            .all(|family| family["freshness"] == "fresh"));
        assert_eq!(fresh_json["fresh_count"], fresh_families.len());
        assert_eq!(fresh_json["stale_count"], 0);
        assert_eq!(fresh_json["cannot_verify_count"], 0);

        // Mutate only the django family's evidence file.
        fs::write(
            workspace.path().join("alpha").join("models.py"),
            "from django.db import models\n\n\nclass Changed(models.Model):\n    name = models.CharField(max_length=1)\n",
        )
        .expect("mutate django evidence");

        let stale = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let stale_json = parse_machine_output("families", &stale, &workspace);
        // The listing stays served (not turned into UNKNOWN) and qualifies the
        // stale family while keeping the untouched family fresh.
        assert_eq!(stale_json["status"], "ok");
        assert_eq!(stale_json["stale_count"], 1);
        let stale_families = stale_json["families"].as_array().expect("families array");
        let django = stale_families
            .iter()
            .find(|family| {
                family["family_id"]
                    .as_str()
                    .is_some_and(|id| id.contains("django_model"))
            })
            .expect("django family present");
        let flask = stale_families
            .iter()
            .find(|family| {
                family["family_id"]
                    .as_str()
                    .is_some_and(|id| id.contains("flask_route"))
            })
            .expect("flask family present");
        assert_eq!(django["freshness"], "stale");
        assert_eq!(flask["freshness"], "fresh");
        assert_eq!(stale_json["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            stale_json["unknowns"][0]["recovery"],
            "run repogrammar resync"
        );
    }

    #[test]
    fn families_listing_ignores_non_evidence_file_changes() {
        let workspace = TempWorkspace::new("families-freshness-unrelated");
        copy_python_release_v0_2_fixture("django_exact_models", &workspace.path().join("alpha"));
        copy_python_release_v0_2_fixture("flask_exact_routes", &workspace.path().join("beta"));
        // An indexed-but-non-evidence module: plain functions form no family.
        fs::write(
            workspace.path().join("util.py"),
            "def add(left, right):\n    return left + right\n\n\ndef sub(left, right):\n    return left - right\n",
        )
        .expect("write unrelated module");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("index", &index, &workspace)["status"],
            "complete"
        );

        // Change a file that is not any family's evidence.
        fs::write(
            workspace.path().join("util.py"),
            "def add(left, right):\n    return left + right + 1\n",
        )
        .expect("mutate unrelated module");

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        let listed = families_json["families"]
            .as_array()
            .expect("families array");
        assert!(listed.iter().all(|family| family["freshness"] == "fresh"));
        assert_eq!(families_json["fresh_count"], listed.len());
        assert_eq!(families_json["stale_count"], 0);
        assert_eq!(families_json["cannot_verify_count"], 0);
    }

    const PYTHON_V0_2_PREVIEW_SMOKE_CASES: &[PythonExactAnchorSmokeCase] = &[
        PythonExactAnchorSmokeCase {
            fixture: "django_exact_models",
            family_id: "family:python:django_model:framework_django_model",
            support_target: "django.db.models.Model",
            evidence_path: "models.py",
            member_role: "framework:django.model",
        },
        PythonExactAnchorSmokeCase {
            fixture: "flask_exact_routes",
            family_id: "family:python:flask_route:framework_flask_route",
            support_target: "flask.route",
            evidence_path: "app.py",
            member_role: "framework:flask.route",
        },
        PythonExactAnchorSmokeCase {
            fixture: "unittest_exact_tests",
            family_id: "family:python:unittest_test_method:framework_unittest_test",
            support_target: "unittest.TestCase.test",
            evidence_path: "test_core.py",
            member_role: "framework:unittest.test",
        },
        PythonExactAnchorSmokeCase {
            fixture: "django_urls_exact",
            family_id: "family:python:django_url_pattern:framework_django_url_pattern",
            support_target: "django.urls.path",
            evidence_path: "urls.py",
            member_role: "framework:django.url_pattern",
        },
    ];

    #[test]
    fn python_v0_2_preview_fixtures_produce_bounded_families() {
        for case in PYTHON_V0_2_PREVIEW_SMOKE_CASES {
            let workspace = TempWorkspace::new(&format!("python-v0-2-preview-{}", case.fixture));
            copy_python_release_v0_2_fixture(case.fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["semantic_worker"], "deferred");

            let status_request = RepositoryStatusRequest {
                path: workspace.path().display().to_string(),
                state_dir_override: None,
            };
            let store = runtime
                .store_for_status_request(&status_request)
                .expect("open store");
            let facts = list_semantic_facts(&store).expect("list semantic facts");
            let target_support_facts = facts
                .facts
                .iter()
                .filter(|fact| {
                    fact.origin_engine == "repogrammar-python-derived"
                        && fact.origin_method == "bounded_ast_anchor_v1"
                        && fact.target.as_deref() == Some(case.support_target)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                target_support_facts.len(),
                3,
                "fixture {} support facts",
                case.fixture
            );
            assert!(target_support_facts.iter().all(|fact| {
                matches!(fact.kind.as_str(), "RESOLVED_CALL" | "SYMBOL" | "TYPE")
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.path == case.evidence_path
                    && fact.start_byte < fact.end_byte
            }));

            let families = run_with_runtime(
                cli_args("families", workspace.path(), &["--json"]),
                &runtime,
            );
            let families_json = parse_machine_output("families", &families, &workspace);
            assert_eq!(families_json["status"], "ok");
            assert_eq!(
                families_json["families"]
                    .as_array()
                    .expect("families")
                    .len(),
                1,
                "fixture {} family count",
                case.fixture
            );
            assert_eq!(
                families_json["families"][0]["family_id"], case.family_id,
                "fixture {} family id",
                case.fixture
            );
            assert_eq!(
                families_json["families"][0]["classification"],
                "DOMINANT_PATTERN"
            );
            assert_eq!(families_json["families"][0]["support"], 3);

            let member = run_with_runtime(
                cli_args("member", workspace.path(), &[case.member_role, "--json"]),
                &runtime,
            );
            let _ = parse_machine_output("member", &member, &workspace);
            assert_no_output_leakage("families", &families.stdout, &workspace);
        }
    }

    #[test]
    fn python_v0_2_preview_lookalikes_and_low_support_form_no_family() {
        for fixture in ["framework_lookalikes", "low_support"] {
            let workspace = TempWorkspace::new(&format!("python-v0-2-negative-{fixture}"));
            copy_python_release_v0_2_fixture(fixture, workspace.path());
            let runtime = ProductCliRuntime;

            run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );

            let status_request = RepositoryStatusRequest {
                path: workspace.path().display().to_string(),
                state_dir_override: None,
            };
            let store = runtime
                .store_for_status_request(&status_request)
                .expect("open store");
            let facts = list_semantic_facts(&store).expect("list semantic facts");
            let preview_support = facts
                .facts
                .iter()
                .filter(|fact| {
                    fact.origin_engine == "repogrammar-python-derived"
                        && matches!(
                            fact.target.as_deref(),
                            Some("django.db.models.Model") | Some("flask.route")
                        )
                })
                .count();
            if fixture == "framework_lookalikes" {
                assert_eq!(
                    preview_support, 0,
                    "unresolved lookalikes minted preview support facts"
                );
            } else {
                // low_support: exactly one resolved route, still below the
                // min-support-3 threshold, so no family may form.
                assert!(
                    preview_support < 3,
                    "low_support exceeded support threshold"
                );
            }

            let families = run_with_runtime(
                cli_args("families", workspace.path(), &["--json"]),
                &runtime,
            );
            let families_json = parse_machine_output("families", &families, &workspace);
            assert_eq!(
                families_json["families"]
                    .as_array()
                    .expect("families")
                    .len(),
                0,
                "fixture {fixture} formed a family"
            );
            assert_no_output_leakage("families", &families.stdout, &workspace);
        }
    }

    #[test]
    fn term_retrieval_resolves_natural_language_fastapi_query_end_to_end() {
        let workspace = TempWorkspace::new("term-retrieval-fastapi-nl");
        copy_python_release_fixture("positive-strong-evidence", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("index", &index, &workspace)["status"],
            "complete"
        );

        // A natural-language target that names a framework and a pattern concept
        // resolves to the single FastAPI route family via deterministic term
        // retrieval, not through any exact-anchor layer.
        let found = run_with_runtime(
            cli_args(
                "find",
                workspace.path(),
                &["How are FastAPI routes implemented?", "--json"],
            ),
            &runtime,
        );
        let value = parse_machine_output("find", &found, &workspace);
        assert_eq!(value["status"], "ok");
        assert_eq!(
            value["family"]["family_id"],
            "family:python:fastapi_route:framework_fastapi_route"
        );
        let route = &value["query_route"];
        assert_eq!(route["hydrated_family_count"], 1);
        assert!(
            route["retrieval_stage_count"]
                .as_u64()
                .expect("stage count")
                >= 3
        );
        let term = &route["term_retrieval"];
        assert_eq!(term["route"], "term_retrieval_hydrate");
        assert_eq!(term["abstention_reason"], Value::Null);
        assert_eq!(term["matched_signals"]["concept"], true);
        assert_eq!(term["matched_signals"]["framework_filter"], true);
        assert!(
            term["hydrated_candidate_count"]
                .as_u64()
                .expect("hydrated count")
                <= 5
        );

        // Determinism: the same query twice yields byte-identical output.
        let found_again = run_with_runtime(
            cli_args(
                "find",
                workspace.path(),
                &["How are FastAPI routes implemented?", "--json"],
            ),
            &runtime,
        );
        assert_eq!(found, found_again, "term retrieval must be deterministic");

        // A bare framework name is not a locatable pattern concept: it abstains
        // with a low-cardinality route reason and never selects a family.
        let bare = run_with_runtime(
            cli_args("find", workspace.path(), &["fastapi", "--json"]),
            &runtime,
        );
        let bare_value = parse_machine_output("find", &bare, &workspace);
        assert_eq!(bare_value["status"], "UNKNOWN");
        assert_eq!(bare_value["query_route"]["selected_family_id"], Value::Null);
        let bare_reason = bare_value["query_route"]["term_retrieval"]["abstention_reason"]
            .as_str()
            .expect("abstention reason");
        assert!(
            ["below_min_score", "unsupported_target", "no_candidate"].contains(&bare_reason),
            "unexpected abstention reason {bare_reason}"
        );
    }

    #[test]
    fn python_release_fixture_exact_anchors_produce_family_without_worker() {
        for case in PYTHON_EXACT_ANCHOR_SMOKE_CASES {
            let workspace =
                TempWorkspace::new(&format!("python-release-derived-family-{}", case.fixture));
            copy_python_release_fixture(case.fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["command"], "index");
            assert_eq!(index_json["status"], "complete");
            assert_eq!(index_json["semantic_worker"], "deferred");
            assert_eq!(index_json["generation_id"], "gen-000001");

            let status_request = RepositoryStatusRequest {
                path: workspace.path().display().to_string(),
                state_dir_override: None,
            };
            let store = runtime
                .store_for_status_request(&status_request)
                .expect("open store");
            let facts = list_semantic_facts(&store).expect("list semantic facts");
            let derived_support_facts = facts
                .facts
                .iter()
                .filter(|fact| {
                    fact.origin_engine == "repogrammar-python-derived"
                        && fact.origin_method == "bounded_ast_anchor_v1"
                })
                .collect::<Vec<_>>();
            let target_support_facts = derived_support_facts
                .iter()
                .copied()
                .filter(|fact| fact.target.as_deref() == Some(case.support_target))
                .collect::<Vec<_>>();
            assert_eq!(target_support_facts.len(), 3);
            assert!(target_support_facts.iter().all(|fact| {
                matches!(fact.kind.as_str(), "RESOLVED_CALL" | "SYMBOL" | "TYPE")
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.path == case.evidence_path
                    && fact.start_byte < fact.end_byte
            }));
            assert!(facts.facts.iter().all(|fact| {
                if !(fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "DATAFLOW_DERIVED")
                {
                    return true;
                }
                fact.assumptions.iter().any(|assumption| {
                    assumption == "derived_from=repo_local_python_import_graph"
                        || assumption == "derived_from=repo_local_pytest_fixture_graph"
                }) && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false")
            }));
            if case.fixture == "pytest-fixture-alias-strong-evidence" {
                assert!(
                    facts.facts.iter().any(|fact| {
                        fact.path == "test_fixture_names.py"
                            && fact.kind == "SYMBOL"
                            && fact.target.as_deref() == Some("pytest.fixture.api_client")
                            && fact.certainty == "DATAFLOW_DERIVED"
                            && fact.assumptions.iter().any(|assumption| {
                                assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                            })
                            && fact.assumptions.iter().any(|assumption| {
                                assumption == "derived_from=repo_local_pytest_fixture_graph"
                            })
                    }),
                    "facts={:?}",
                    facts.facts
                );
                assert!(facts.facts.iter().all(|fact| {
                    !(fact.path == "test_fixture_names.py"
                        && fact.kind == "SYMBOL"
                        && fact.target.as_deref() == Some("pytest.fixture._api_client")
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                        }))
                }));
            }

            let families = run_with_runtime(
                cli_args("families", workspace.path(), &["--json"]),
                &runtime,
            );
            let families_json = parse_machine_output("families", &families, &workspace);
            assert_eq!(families_json["status"], "ok");
            assert_eq!(families_json["command"], "families");
            assert_eq!(families_json["implemented"], true);
            assert_eq!(families_json["active_generation"], "gen-000001");
            assert_eq!(
                families_json["families"]
                    .as_array()
                    .expect("families")
                    .len(),
                1
            );
            let family_id = families_json["families"][0]["family_id"]
                .as_str()
                .expect("family id")
                .to_string();
            assert_eq!(family_id, case.family_id);
            assert_eq!(
                families_json["families"][0]["classification"],
                "DOMINANT_PATTERN"
            );
            assert_eq!(families_json["families"][0]["support"], 3);
            let stats =
                run_with_runtime(cli_args("stats", workspace.path(), &["--json"]), &runtime);
            let stats_json = parse_machine_output("stats", &stats, &workspace);
            assert_eq!(stats_json["status"], "ok");
            assert!(
                stats_json["metrics"]["local_pattern_density"]
                    .as_f64()
                    .expect("known local pattern density")
                    > 0.0
            );
            assert!(
                stats_json["metrics"]["family_support_coverage"]
                    .as_f64()
                    .expect("known family support coverage")
                    > 0.0
            );
            assert!(stats_json["metrics"]["token_saving_risk"].is_string());
            assert_eq!(
                stats_json["metrics"]["external_dependency_signal"],
                Value::Null
            );
            assert_eq!(stats_json["token_savings"], Value::Null);

            let family = run_with_runtime(
                cli_args("family", workspace.path(), &[&family_id, "--json"]),
                &runtime,
            );
            let family_json = parse_machine_output("family", &family, &workspace);
            assert_eq!(family_json["status"], "ok");
            let member_id = assert_python_exact_anchor_family_detail("family", &family_json, *case);

            let member = run_with_runtime(
                cli_args("member", workspace.path(), &[&member_id, "--json"]),
                &runtime,
            );
            let member_json = parse_machine_output("member", &member, &workspace);
            assert_eq!(member_json["status"], "ok");
            assert_python_exact_anchor_family_detail("member", &member_json, *case);

            for command in ["find", "explain"] {
                let output = run_with_runtime(
                    cli_args(command, workspace.path(), &[case.evidence_path, "--json"]),
                    &runtime,
                );
                let value = parse_machine_output(command, &output, &workspace);
                assert_eq!(value["status"], "ok", "{command} should find family");
                assert_python_exact_anchor_family_detail(command, &value, *case);
            }

            let check = run_with_runtime(
                cli_args("check", workspace.path(), &[case.evidence_path, "--json"]),
                &runtime,
            );
            let check_json = parse_machine_output("check", &check, &workspace);
            assert_eq!(check_json["status"], "CONTEXT_ONLY");
            assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");
            assert_eq!(
                check_json["check"]["reason"],
                "runtime equivalence remains unproven"
            );
            assert!(check_json["check"].get("fail_on").is_none());
            assert!(check_json["check"].get("pass").is_none());
            assert!(check_json["check"].get("conforms").is_none());
            assert_python_exact_anchor_family_detail("check", &check_json, *case);

            let family_auto_evidence = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[&family_id, "--token-budget", "1", "--json"],
                ),
                &runtime,
            );
            let auto_evidence_json =
                parse_machine_output("family", &family_auto_evidence, &workspace);
            assert_python_exact_anchor_evidence(
                "family",
                &auto_evidence_json,
                *case,
                "evidence",
                Some(1),
            );
            assert_eq!(auto_evidence_json["output"]["budget_satisfied"], false);

            let family_compact_override = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[
                        &family_id,
                        "--mode",
                        "compact",
                        "--token-budget",
                        "1",
                        "--json",
                    ],
                ),
                &runtime,
            );
            let compact_override_json =
                parse_machine_output("family", &family_compact_override, &workspace);
            assert_eq!(compact_override_json["status"], "ok");
            assert_eq!(compact_override_json["output"]["mode"], "compact");
            assert_eq!(compact_override_json["output"]["token_budget"], 1);
            assert_eq!(
                compact_override_json["output"]["estimated_evidence_tokens"],
                0
            );
            assert!(compact_override_json["evidence"]
                .as_array()
                .expect("evidence")
                .is_empty());

            let family_evidence = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[
                        &family_id,
                        "--mode",
                        "evidence",
                        "--token-budget",
                        "1",
                        "--json",
                    ],
                ),
                &runtime,
            );
            let evidence_json = parse_machine_output("family", &family_evidence, &workspace);
            assert_python_exact_anchor_evidence(
                "family",
                &evidence_json,
                *case,
                "evidence",
                Some(1),
            );

            let family_deep = run_with_runtime(
                cli_args(
                    "family",
                    workspace.path(),
                    &[&family_id, "--mode", "deep", "--json"],
                ),
                &runtime,
            );
            let deep_json = parse_machine_output("family", &family_deep, &workspace);
            assert_python_exact_anchor_evidence("family", &deep_json, *case, "deep", None);
        }
    }

    fn index_release_v0_2_fixture(
        fixture: &str,
        prefix: &str,
    ) -> (TempWorkspace, ProductCliRuntime) {
        let workspace = TempWorkspace::new(prefix);
        copy_release_v0_2_fixture(fixture, workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");
        assert_eq!(index_json["generation_id"], "gen-000001");
        (workspace, runtime)
    }

    fn tsjs_derived_support_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        // No FRAMEWORK_HEURISTIC role fact may ever masquerade as derived support.
        assert!(facts.facts.iter().all(|fact| {
            !(fact.certainty == "DATAFLOW_DERIVED"
                && fact.origin_engine == "repogrammar-frameworks")
        }));
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-tsjs-derived"
                    && fact.origin_method == "bounded_exact_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                (
                    fact.path.clone(),
                    fact.target.clone().unwrap_or_default(),
                    fact.certainty.clone(),
                )
            })
            .collect()
    }

    fn assert_tsjs_unknown(
        facts: &IndexedSemanticFactsReport,
        path: &str,
        reason: &str,
        unknown_kind: &str,
    ) {
        assert!(
            facts.facts.iter().any(|fact| {
                fact.kind == "UNKNOWN"
                    && fact.path == path
                    && fact.target.as_deref() == Some(reason)
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == &format!("tsjs_unknown_kind={unknown_kind}")
                    })
            }),
            "missing TS/JS UNKNOWN {reason}/{unknown_kind} for {path}: {:?}",
            facts.facts
        );
    }

    fn index_rust_release_v0_2_fixture(
        fixture: &str,
        prefix: &str,
    ) -> (TempWorkspace, ProductCliRuntime) {
        let workspace = TempWorkspace::new(prefix);
        copy_rust_release_v0_2_fixture(fixture, workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert!(
            index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
            "Rust fixture should index units: {index_json}"
        );
        (workspace, runtime)
    }

    fn rust_derived_support_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().all(|fact| {
            !(fact.certainty == "DATAFLOW_DERIVED"
                && fact.origin_engine == "repogrammar-frameworks")
        }));
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-rust-derived"
                    && fact.origin_method == "bounded_tree_sitter_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                (
                    fact.path.clone(),
                    fact.target.clone().unwrap_or_default(),
                    fact.certainty.clone(),
                )
            })
            .collect()
    }

    fn index_java_release_v0_2_fixture(
        fixture: &str,
        prefix: &str,
    ) -> (TempWorkspace, ProductCliRuntime) {
        let workspace = TempWorkspace::new(prefix);
        copy_java_release_v0_2_fixture(fixture, workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert!(
            index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
            "Java fixture should index units: {index_json}"
        );
        (workspace, runtime)
    }

    fn java_derived_support_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().all(|fact| {
            !(fact.certainty == "DATAFLOW_DERIVED"
                && fact.origin_engine == "repogrammar-frameworks")
        }));
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-java-derived"
                    && fact.origin_method == "bounded_tree_sitter_java_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                (
                    fact.path.clone(),
                    fact.target.clone().unwrap_or_default(),
                    fact.certainty.clone(),
                )
            })
            .collect()
    }

    fn rust_semantic_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> IndexedSemanticFactsReport {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        list_semantic_facts(&store).expect("list semantic facts")
    }

    fn rust_family_json(runtime: &ProductCliRuntime, workspace: &TempWorkspace) -> Value {
        let families =
            run_with_runtime(cli_args("families", workspace.path(), &["--json"]), runtime);
        parse_machine_output("families", &families, workspace)
    }

    fn assert_rust_family_role(value: &Value, role_token: &str, min_support: u64) {
        assert!(
            value["families"]
                .as_array()
                .expect("families")
                .iter()
                .any(|family| family["family_id"]
                    .as_str()
                    .is_some_and(|id| id.contains(role_token))
                    && family["support"].as_u64().unwrap_or_default() >= min_support),
            "missing Rust family role {role_token} with support >= {min_support}: {value}"
        );
    }

    #[test]
    fn java_spring_mvc_exact_routes_form_family_without_worker() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "spring_mvc_exact_routes",
            "java-release-spring-mvc-exact-routes",
        );

        let derived = java_derived_support_facts(&runtime, &workspace);
        let route_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "src/main/java/com/example/catalog/CatalogController.java"
                    && target == "spring.web.bind.annotation.GetMapping"
            })
            .count();
        assert_eq!(route_derived, 3, "derived Java support facts: {derived:?}");
        assert!(
            derived.iter().any(|(path, target, _)| {
                path == "src/main/java/com/example/catalog/CatalogController.java"
                    && target == "spring.web.bind.annotation.RestController"
            }),
            "missing exact RestController support fact: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(family_id.starts_with("family:java:spring_mvc_route:framework_spring_mvc_route"));
        assert_eq!(family_array[0]["support"], 3);
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
    }

    fn java_persisted_unknowns(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.kind == "UNKNOWN" && fact.origin_engine == "repogrammar-java-syntax"
            })
            .map(|fact| {
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (fact.target.clone().unwrap_or_default(), claim)
            })
            .collect()
    }

    fn assert_single_java_family(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
        family_prefix: &str,
    ) {
        let families =
            run_with_runtime(cli_args("families", workspace.path(), &["--json"]), runtime);
        let families_json = parse_machine_output("families", &families, workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1, "families: {families_json}");
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(
            family_id.starts_with(family_prefix),
            "unexpected family id: {family_id}"
        );
        assert_eq!(family_array[0]["support"], 3);
        assert_eq!(family_array[0]["classification"], "DOMINANT_PATTERN");
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            runtime,
        );
        let detail_json = parse_machine_output("family", &detail, workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
    }

    #[test]
    fn java_junit5_exact_tests_form_family_without_worker() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "junit5_exact_tests",
            "java-release-junit5-exact-tests",
        );
        let derived = java_derived_support_facts(&runtime, &workspace);
        let test_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "src/test/java/com/example/orders/OrderServiceTest.java"
                    && target == "junit.jupiter.Test"
            })
            .count();
        assert_eq!(
            test_derived, 3,
            "derived JUnit 5 support facts: {derived:?}"
        );
        assert_single_java_family(
            &runtime,
            &workspace,
            "family:java:junit5_test_method:framework_junit5_test",
        );
    }

    #[test]
    fn java_local_test_data_bindings_preserve_family_support_and_remove_runtime_link_unknowns() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "test_data_local_resolution",
            "java-release-test-data-local-resolution",
        );
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open test-data fixture store");
        let facts = list_semantic_facts(&store).expect("list test-data fixture facts");
        for (target, expected) in [
            ("junit.jupiter.MethodSource.local_factory", 3),
            ("testng.annotations.DataProvider.local_method", 3),
        ] {
            let bindings = facts
                .facts
                .iter()
                .filter(|fact| {
                    fact.origin_engine == "repogrammar-java-syntax"
                        && fact.certainty == "STRUCTURAL"
                        && fact.target.as_deref() == Some(target)
                })
                .count();
            assert_eq!(bindings, expected, "bounded binding facts: {facts:?}");
        }
        assert!(facts.facts.iter().all(|fact| {
            fact.kind != "UNKNOWN"
                || !fact.assumptions.iter().any(|assumption| {
                    matches!(
                        assumption.as_str(),
                        "affected_claim=java_test_method_source"
                            | "affected_claim=java_testng_data_provider"
                    )
                })
        }));

        let derived = java_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "junit.jupiter.ParameterizedTest")
                .count(),
            3
        );
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "testng.annotations.Test")
                .count(),
            3
        );
        assert!(derived.iter().all(|(_, target, _)| {
            target != "junit.jupiter.MethodSource.local_factory"
                && target != "testng.annotations.DataProvider.local_method"
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_rows = families_json["families"].as_array().expect("families");
        assert_eq!(family_rows.len(), 2, "families: {families_json}");
        for prefix in [
            "family:java:junit5_test_method:framework_junit5_test",
            "family:java:testng_test_method:framework_testng_test",
        ] {
            let family = family_rows
                .iter()
                .find(|family| {
                    family["family_id"]
                        .as_str()
                        .is_some_and(|family_id| family_id.starts_with(prefix))
                })
                .unwrap_or_else(|| panic!("missing {prefix}: {families_json}"));
            assert_eq!(family["support"], 3);
            assert_eq!(family["classification"], "DOMINANT_PATTERN");
        }
    }

    #[test]
    fn java_jpa_exact_entities_form_family_without_worker() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "jpa_exact_entities",
            "java-release-jpa-exact-entities",
        );
        let derived = java_derived_support_facts(&runtime, &workspace);
        let entity_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "src/main/java/com/example/catalog/Entities.java"
                    && target == "jpa.persistence.Entity"
            })
            .count();
        assert_eq!(entity_derived, 3, "derived JPA support facts: {derived:?}");
        assert_single_java_family(
            &runtime,
            &workspace,
            "family:java:jpa_entity:framework_jpa_entity",
        );
    }

    #[test]
    fn java_jaxrs_exact_resources_form_family_without_worker() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "jaxrs_exact_resources",
            "java-release-jaxrs-exact-resources",
        );
        let derived = java_derived_support_facts(&runtime, &workspace);
        let method_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "src/main/java/com/example/api/BookResource.java"
                    && target == "jaxrs.ws.rs.GET"
            })
            .count();
        assert_eq!(
            method_derived, 3,
            "derived JAX-RS support facts: {derived:?}"
        );
        assert!(
            derived.iter().any(|(path, target, _)| {
                path == "src/main/java/com/example/api/BookResource.java"
                    && target == "jaxrs.ws.rs.Path"
            }),
            "missing exact @Path resource-class support fact: {derived:?}"
        );
        assert_single_java_family(
            &runtime,
            &workspace,
            "family:java:jaxrs_resource_method:framework_jaxrs_resource_method",
        );
    }

    #[test]
    fn java_framework_lookalikes_stay_unknown_without_family() {
        let (workspace, runtime) = index_java_release_v0_2_fixture(
            "test_annotation_lookalikes",
            "java-release-test-annotation-lookalikes",
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);

        let unknowns = java_persisted_unknowns(&runtime, &workspace);
        assert!(
            unknowns
                .iter()
                .any(|(reason, claim)| reason == "UnresolvedImport"
                    && claim == "java_test_annotation_binding"),
            "missing (UnresolvedImport, java_test_annotation_binding): {unknowns:?}"
        );
        assert!(
            unknowns
                .iter()
                .any(|(reason, claim)| reason == "UnresolvedImport"
                    && claim == "java_jpa_entity_identity"),
            "missing (UnresolvedImport, java_jpa_entity_identity): {unknowns:?}"
        );
    }

    fn index_csharp_release_v0_2_fixture(
        fixture: &str,
        prefix: &str,
    ) -> (TempWorkspace, ProductCliRuntime) {
        let workspace = TempWorkspace::new(prefix);
        copy_csharp_release_v0_2_fixture(fixture, workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert!(
            index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
            "C# fixture should index units: {index_json}"
        );
        (workspace, runtime)
    }

    fn csharp_derived_support_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().all(|fact| {
            !(fact.certainty == "DATAFLOW_DERIVED"
                && fact.origin_engine == "repogrammar-frameworks")
        }));
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-csharp-derived"
                    && fact.origin_method == "bounded_tree_sitter_csharp_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                (
                    fact.path.clone(),
                    fact.target.clone().unwrap_or_default(),
                    fact.certainty.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn csharp_aspnet_exact_controllers_form_family_without_worker() {
        let (workspace, runtime) = index_csharp_release_v0_2_fixture(
            "aspnet_exact_controllers",
            "csharp-release-aspnet-exact-controllers",
        );

        let derived = csharp_derived_support_facts(&runtime, &workspace);
        let route_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "Controllers/CatalogController.cs" && target == "aspnetcore.mvc.HttpGet"
            })
            .count();
        assert_eq!(route_derived, 3, "derived C# support facts: {derived:?}");
        assert!(
            derived.iter().any(|(path, target, _)| {
                path == "Controllers/CatalogController.cs"
                    && target == "aspnetcore.mvc.ApiController"
            }),
            "missing exact ApiController support fact: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(family_id.starts_with(
            "family:csharp:aspnet_controller_action:framework_aspnetcore_controller_action"
        ));
        assert_eq!(family_array[0]["support"], 3);
        assert_eq!(family_array[0]["classification"], "DOMINANT_PATTERN");
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
    }

    #[test]
    fn csharp_xunit_exact_tests_form_family_without_worker() {
        let (workspace, runtime) = index_csharp_release_v0_2_fixture(
            "xunit_exact_tests",
            "csharp-release-xunit-exact-tests",
        );

        let derived = csharp_derived_support_facts(&runtime, &workspace);
        let test_derived = derived
            .iter()
            .filter(|(path, target, _)| path == "Tests/CatalogTests.cs" && target == "xunit.Fact")
            .count();
        assert_eq!(
            test_derived, 3,
            "derived C# xUnit support facts: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(
            family_id.starts_with("family:csharp:xunit_test_method:framework_xunit_test"),
            "unexpected family id: {family_id}"
        );
        assert_eq!(family_array[0]["support"], 3);
    }

    #[test]
    fn csharp_xunit_same_class_member_data_resolves_without_link_unknown() {
        let (workspace, runtime) = index_csharp_release_v0_2_fixture(
            "xunit_member_data_exact",
            "csharp-release-xunit-member-data-exact",
        );

        let derived = csharp_derived_support_facts(&runtime, &workspace);
        let theory_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "Tests/CatalogTheoryTests.cs" && target == "xunit.Theory"
            })
            .count();
        assert_eq!(
            theory_derived, 3,
            "derived C# xUnit theory support facts: {derived:?}"
        );
        assert!(
            csharp_persisted_unknowns(&runtime, &workspace)
                .iter()
                .all(|(_, claim)| claim != "csharp_test_member_data"),
            "exact same-class MemberData links must not persist a link UNKNOWN"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        assert_eq!(family_array[0]["support"], 3);
        assert!(family_array[0]["family_id"]
            .as_str()
            .expect("family id")
            .starts_with("family:csharp:xunit_test_method:framework_xunit_test"));
    }

    #[test]
    fn csharp_framework_lookalikes_stay_unknown_without_family() {
        let (workspace, runtime) = index_csharp_release_v0_2_fixture(
            "framework_lookalikes",
            "csharp-release-framework-lookalikes",
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);

        let facts = csharp_persisted_unknowns(&runtime, &workspace);
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "UnresolvedImport"
                    && claim == "csharp_attribute_binding"),
            "missing (UnresolvedImport, csharp_attribute_binding) pair: {facts:?}"
        );
    }

    #[test]
    fn csharp_preprocessor_variant_blocks_family_with_build_variant_unknown() {
        let (workspace, runtime) = index_csharp_release_v0_2_fixture(
            "preprocessor_variant_unknown",
            "csharp-release-preprocessor-variant",
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());

        let facts = csharp_persisted_unknowns(&runtime, &workspace);
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "BuildVariantAmbiguity"
                    && claim == "csharp_build_variant"),
            "missing (BuildVariantAmbiguity, csharp_build_variant) pair: {facts:?}"
        );
    }

    fn csharp_persisted_unknowns(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.kind == "UNKNOWN" && fact.origin_engine == "repogrammar-csharp-syntax"
            })
            .map(|fact| {
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (fact.target.clone().unwrap_or_default(), claim)
            })
            .collect()
    }

    fn index_cpp_release_v0_2_fixture(
        fixture: &str,
        prefix: &str,
    ) -> (TempWorkspace, ProductCliRuntime) {
        let workspace = TempWorkspace::new(prefix);
        copy_cpp_release_v0_2_fixture(fixture, workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");
        assert_eq!(index_json["generation_id"], "gen-000001");
        assert!(
            index_json["indexed_units"].as_u64().unwrap_or_default() > 0,
            "C/C++ fixture should index units: {index_json}"
        );
        (workspace, runtime)
    }

    fn cpp_derived_support_facts(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().all(|fact| {
            !(fact.certainty == "DATAFLOW_DERIVED"
                && fact.origin_engine == "repogrammar-frameworks")
        }));
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-cpp-derived"
                    && fact.origin_method == "bounded_tree_sitter_c_cpp_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                (
                    fact.path.clone(),
                    fact.target.clone().unwrap_or_default(),
                    fact.certainty.clone(),
                )
            })
            .collect()
    }

    fn cpp_persisted_unknowns(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        facts
            .facts
            .iter()
            .filter(|fact| fact.kind == "UNKNOWN" && fact.origin_engine == "repogrammar-cpp-syntax")
            .map(|fact| {
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (fact.target.clone().unwrap_or_default(), claim)
            })
            .collect()
    }

    fn rust_persisted_unknowns(
        runtime: &ProductCliRuntime,
        workspace: &TempWorkspace,
    ) -> Vec<(String, String)> {
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        facts
            .facts
            .iter()
            .filter(|fact| {
                fact.kind == "UNKNOWN" && fact.origin_engine == "repogrammar-rust-syntax"
            })
            .map(|fact| {
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (fact.target.clone().unwrap_or_default(), claim)
            })
            .collect()
    }

    #[test]
    fn cpp_gtest_exact_tests_form_family_without_worker() {
        let (workspace, runtime) =
            index_cpp_release_v0_2_fixture("gtest_exact_tests", "cpp-release-gtest-exact-tests");

        let derived = cpp_derived_support_facts(&runtime, &workspace);
        let test_derived = derived
            .iter()
            .filter(|(path, target, _)| path == "tests/catalog_test.cc" && target == "gtest.TEST")
            .count();
        assert_eq!(
            test_derived, 3,
            "derived C/C++ gtest support facts: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(
            family_id.starts_with("family:cpp:gtest_test_case:framework_gtest_test"),
            "unexpected family id: {family_id}"
        );
        assert_eq!(family_array[0]["support"], 3);
        assert_eq!(family_array[0]["classification"], "DOMINANT_PATTERN");
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
    }

    #[test]
    fn cpp_catch2_exact_tests_form_family_without_worker() {
        let (workspace, runtime) =
            index_cpp_release_v0_2_fixture("catch2_exact_tests", "cpp-release-catch2-exact-tests");

        let derived = cpp_derived_support_facts(&runtime, &workspace);
        let test_derived = derived
            .iter()
            .filter(|(path, target, _)| {
                path == "tests/catalog_test.cpp" && target == "catch2.TEST_CASE"
            })
            .count();
        assert_eq!(
            test_derived, 3,
            "derived C/C++ catch2 support facts: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(
            family_id.starts_with("family:cpp:catch2_test_case:framework_catch2_test"),
            "unexpected family id: {family_id}"
        );
        assert_eq!(family_array[0]["support"], 3);
    }

    #[test]
    fn cpp_test_macro_lookalikes_stay_unknown_without_family() {
        let (workspace, runtime) =
            index_cpp_release_v0_2_fixture("test_macro_lookalikes", "cpp-release-lookalikes");

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);

        let facts = cpp_persisted_unknowns(&runtime, &workspace);
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "UnresolvedImport"
                    && claim == "cpp_test_framework_identity"),
            "missing (UnresolvedImport, cpp_test_framework_identity) pair: {facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "ConflictingFacts"
                    && claim == "cpp_test_framework_identity"),
            "missing (ConflictingFacts, cpp_test_framework_identity) pair: {facts:?}"
        );
    }

    #[test]
    fn cpp_test_macro_contract_violations_stay_unknown_without_family() {
        let (workspace, runtime) = index_cpp_release_v0_2_fixture(
            "test_macro_contract_unknown",
            "cpp-release-contract-unknown",
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);

        let facts = cpp_persisted_unknowns(&runtime, &workspace);
        assert_eq!(
            facts
                .iter()
                .filter(|(reason, claim)| reason == "MacroOrPreprocessor"
                    && claim == "cpp_test_framework_identity")
                .count(),
            6,
            "contract violation UNKNOWNs: {facts:?}"
        );
    }

    #[test]
    fn cpp_preprocessor_variant_blocks_family_with_build_variant_unknown() {
        let (workspace, runtime) = index_cpp_release_v0_2_fixture(
            "preprocessor_variant_unknown",
            "cpp-release-preprocessor-variant",
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "UNKNOWN");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());

        let facts = cpp_persisted_unknowns(&runtime, &workspace);
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "BuildVariantAmbiguity"
                    && claim == "cpp_build_variant"),
            "missing (BuildVariantAmbiguity, cpp_build_variant) pair: {facts:?}"
        );
        // The include-guarded header must not add any build-variant UNKNOWN.
        assert!(
            !facts.iter().any(|(_, claim)| claim == "cpp_project_config"),
            "unexpected project-config UNKNOWN in variant fixture: {facts:?}"
        );
    }

    #[test]
    fn rust_serde_exact_models_form_general_framework_family_without_worker() {
        let (workspace, runtime) = index_rust_release_v0_2_fixture(
            "serde_exact_models",
            "rust-release-serde-exact-models",
        );

        let derived = rust_derived_support_facts(&runtime, &workspace);
        let serialize_derived = derived
            .iter()
            .filter(|(path, target, _)| path == "src/lib.rs" && target == "serde.Serialize")
            .count();
        assert_eq!(
            serialize_derived, 3,
            "derived serde support facts: {derived:?}"
        );
        assert!(
            derived
                .iter()
                .any(|(path, target, _)| path == "src/lib.rs" && target == "serde.Deserialize"),
            "missing serde.Deserialize support fact: {derived:?}"
        );

        let families_json = rust_family_json(&runtime, &workspace);
        assert_rust_family_role(&families_json, "serde_model:framework_serde_model", 3);
        let family_array = families_json["families"].as_array().expect("families");
        assert!(family_array.iter().all(|family| {
            family["family_id"]
                .as_str()
                .is_some_and(|id| !id.contains("serde_model") || family["support"] == 3)
        }));
    }

    #[test]
    fn rust_axum_exact_routes_form_general_framework_family_without_worker() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("axum_exact_routes", "rust-release-axum-exact-routes");

        let derived = rust_derived_support_facts(&runtime, &workspace);
        let route_derived = derived
            .iter()
            .filter(|(path, target, _)| path == "src/lib.rs" && target == "axum.routing.route")
            .count();
        assert_eq!(
            route_derived, 3,
            "derived axum route support facts: {derived:?}"
        );

        let families_json = rust_family_json(&runtime, &workspace);
        assert_rust_family_role(&families_json, "axum_route:framework_axum_route", 3);
    }

    #[test]
    fn rust_thiserror_exact_errors_form_general_framework_family_without_worker() {
        let (workspace, runtime) = index_rust_release_v0_2_fixture(
            "thiserror_exact_errors",
            "rust-release-thiserror-exact-errors",
        );

        let derived = rust_derived_support_facts(&runtime, &workspace);
        let error_derived = derived
            .iter()
            .filter(|(path, target, _)| path == "src/lib.rs" && target == "thiserror.Error")
            .count();
        assert_eq!(
            error_derived, 3,
            "derived thiserror support facts: {derived:?}"
        );

        let families_json = rust_family_json(&runtime, &workspace);
        assert_rust_family_role(
            &families_json,
            "thiserror_error_enum:framework_thiserror_error",
            3,
        );
    }

    #[test]
    fn rust_derive_lookalikes_stay_unknown_without_family() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("derive_lookalikes", "rust-release-derive-lookalikes");

        let families_json = rust_family_json(&runtime, &workspace);
        assert!(
            !families_json["families"]
                .as_array()
                .expect("families")
                .iter()
                .any(|family| family["family_id"]
                    .as_str()
                    .is_some_and(|id| id.contains("serde_model"))),
            "derive lookalikes must not form a serde family: {families_json}"
        );

        let facts = rust_persisted_unknowns(&runtime, &workspace);
        assert!(
            facts
                .iter()
                .any(|(reason, claim)| reason == "UnresolvedImport"
                    && claim == "rust_framework_attribute_binding"),
            "missing (UnresolvedImport, rust_framework_attribute_binding) pair: {facts:?}"
        );
    }

    #[test]
    fn rust_structural_self_dogfood_forms_internal_family_without_source_snippets() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("internal_family_gates", "rust-release-family-gates");

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let units_json = parse_machine_output("units", &units, &workspace);
        let rust_functions = units_json["units"]
            .as_array()
            .expect("units")
            .iter()
            .filter(|unit| {
                unit["language"] == "rust"
                    && unit["kind"] == "rust_function"
                    && unit["path"] == "src/rust/application/family.rs"
            })
            .collect::<Vec<_>>();
        assert!(
            rust_functions.len() >= 4,
            "expected Rust functions, got {units_json}"
        );

        let derived = rust_derived_support_facts(&runtime, &workspace);
        let family_gate_support = derived
            .iter()
            .filter(|(path, target, certainty)| {
                path == "src/rust/application/family.rs"
                    && target == "repogrammar.rust.family_gate"
                    && certainty == "DATAFLOW_DERIVED"
            })
            .count();
        assert!(
            family_gate_support >= 4,
            "Rust structural anchors should derive bounded support: {derived:?}"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1, "{families_json}");
        let family = &family_array[0];
        assert_eq!(family["classification"], "DOMINANT_PATTERN");
        assert_eq!(family["support"], 3);
        let family_id = family["family_id"].as_str().expect("family id");
        assert!(
            family_id
                .starts_with("family:rust:rust_function:framework_repogrammar_rust_family_gate"),
            "unexpected Rust family id: {family_id}"
        );

        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
        assert_eq!(detail_json["read_plan"]["source_snippets_included"], false);
        assert_read_plan_item_has_line_range(&detail_json["read_plan"]["items"][0]);

        let spanned = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[family_id, "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        assert_eq!(spanned.status, 0, "stderr: {}", spanned.stderr);
        assert!(!spanned
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let spanned_json: Value =
            serde_json::from_str(spanned.stdout.trim()).expect("span json parses");
        assert_eq!(spanned_json["source_spans"]["requested"], true);
        assert_eq!(
            spanned_json["source_spans"]["source_snippets_included"],
            true
        );
        assert!(spanned_json["source_spans"]["spans"]
            .as_array()
            .expect("spans")
            .iter()
            .any(|span| span["text"]
                .as_str()
                .is_some_and(|text| text.contains("pub fn support_"))));

        fs::write(
            workspace.path().join("src/rust/application/family.rs"),
            "pub fn support_route_family(value: usize) -> Result<usize, String> { Ok(value) }\n",
        )
        .expect("mutate Rust family fixture");
        let stale = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[family_id, "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_python_stale_unknown("family", &stale_json, family_id);
        assert!(stale_json.get("source_spans").is_none());
    }

    #[test]
    fn rust_release_fixtures_form_required_internal_role_families() {
        for (fixture, role_token, support) in [
            (
                "parser_adapters",
                "framework_repogrammar_rust_parser_adapter",
                2,
            ),
            (
                "installer_actions",
                "framework_repogrammar_rust_installer_action",
                3,
            ),
            (
                "product_tests",
                "framework_repogrammar_rust_product_test",
                3,
            ),
        ] {
            let (workspace, runtime) = index_rust_release_v0_2_fixture(
                fixture,
                &format!("rust-release-required-{fixture}"),
            );
            let derived = rust_derived_support_facts(&runtime, &workspace);
            assert!(
                derived
                    .iter()
                    .filter(|(_, target, _)| target.starts_with("repogrammar.rust."))
                    .count()
                    >= support as usize,
                "{fixture} should derive Rust support facts: {derived:?}"
            );
            let families_json = rust_family_json(&runtime, &workspace);
            assert_eq!(families_json["status"], "ok");
            assert_rust_family_role(&families_json, role_token, support);
        }
    }

    #[test]
    fn rust_low_support_stays_unknown_without_family_rows() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("low_support_family", "rust-release-low-support");
        let derived = rust_derived_support_facts(&runtime, &workspace);
        let family_gate_support_count = derived
            .iter()
            .filter(|(_, target, _)| target == "repogrammar.rust.family_gate")
            .count();
        assert!(
            family_gate_support_count >= 2,
            "low-support fixture should derive family-gate support facts before clustering: {derived:?}"
        );
        let families_json = rust_family_json(&runtime, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);
    }

    #[test]
    fn product_runtime_resync_records_cargo_metadata_project_model_without_build_script() {
        let workspace = TempWorkspace::new("product-runtime-rust-cargo-metadata");
        fs::create_dir_all(workspace.path().join("src")).expect("create src");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
        )
        .expect("write manifest");
        fs::write(
            workspace.path().join("build.rs"),
            "fn main() { std::fs::write(\"build-script-ran.txt\", \"ran\").unwrap(); }\n",
        )
        .expect("write build script");
        fs::write(
            workspace.path().join("src/lib.rs"),
            "pub fn demo() -> usize { 1 }\n",
        )
        .expect("write lib");
        let runtime = ProductCliRuntime;
        assert_eq!(
            run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only"]),
                &runtime
            )
            .status,
            0
        );

        let output = run_with_runtime(
            cli_args(
                "resync",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );

        assert_eq!(output.status, 0);
        assert!(
            !workspace.path().join("build-script-ran.txt").exists(),
            "resync must not execute Cargo build scripts"
        );
        let value = parse_machine_output("resync", &output, &workspace);
        assert_eq!(value["command"], "resync");
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.certainty == "SEMANTIC"
                && fact.origin_engine == "cargo_metadata"
                && fact.origin_method == "cargo_metadata_no_deps_v1"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("cargo.package.demo_crate")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "cargo_metadata_no_deps=true")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "build_scripts_executed=false")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "proc_macros_executed=false")
        }));
    }

    #[test]
    fn rust_cfg_unknown_blocks_structural_family_claim() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("cfg_blocked_family", "rust-release-cfg-blocked");

        let derived = rust_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "repogrammar.rust.family_gate")
                .count(),
            1,
            "cfg-gated units must not derive claim support; only the un-gated helper may remain: {derived:?}"
        );

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/rust/application/family.rs"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("BuildVariantAmbiguity")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=rust_build_variant")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_cfg_feature=preview")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_cfg_feature_declared=preview:true")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_cfg_model=cargo_feature_cfg_model")
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);
    }

    #[test]
    fn rust_cargo_build_script_unknown_blocks_repository_family_claim() {
        let (workspace, runtime) = index_rust_release_v0_2_fixture(
            "cargo_build_blocked_family",
            "rust-release-cargo-build-blocked",
        );

        assert!(
            !workspace.path().join("build-script-ran.txt").exists(),
            "indexing must not execute Cargo build scripts"
        );

        let derived = rust_derived_support_facts(&runtime, &workspace);
        let family_gate_support = derived
            .iter()
            .filter(|(_, target, _)| target == "repogrammar.rust.family_gate")
            .count();
        assert!(
            family_gate_support >= 3,
            "build-script fixture should still derive structural anchors before repository blocking: {derived:?}"
        );

        let facts = rust_semantic_facts(&runtime, &workspace);
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("BuildVariantAmbiguity")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=rust_build_variant")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_unknown_kind=build_script")
        }));

        let families_json = rust_family_json(&runtime, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        assert_no_claim_payload("families", &families_json);
    }

    #[test]
    fn rust_macro_cfg_and_trait_dispatch_unknowns_block_claims() {
        for (fixture, expected_reason) in [
            ("macro_cfg_unknowns", "MacroOrPreprocessor"),
            ("trait_dispatch_unknowns", "FrameworkMagic"),
        ] {
            let (workspace, runtime) = index_rust_release_v0_2_fixture(
                fixture,
                &format!("rust-release-unknown-{fixture}"),
            );
            let facts = rust_semantic_facts(&runtime, &workspace);
            assert!(
                facts.facts.iter().any(|fact| {
                    fact.kind == "UNKNOWN" && fact.target.as_deref() == Some(expected_reason)
                }),
                "{fixture} should emit {expected_reason}: {:?}",
                facts.facts
            );
            let families_json = rust_family_json(&runtime, &workspace);
            assert!(families_json["families"]
                .as_array()
                .expect("families")
                .is_empty());
            assert_no_claim_payload("families", &families_json);
        }
    }

    #[test]
    fn rust_module_resolution_records_bounded_context_and_unknowns() {
        let (workspace, runtime) =
            index_rust_release_v0_2_fixture("module_resolution", "rust-release-module-resolution");
        let facts = rust_semantic_facts(&runtime, &workspace);
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("cargo.workspace:members")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("cargo.dependency:libc")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "Cargo.toml"
                && fact.target.as_deref() == Some("BuildVariantAmbiguity")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "SYMBOL"
                && fact.path == "src/rust/adapters/parsing/mod.rs"
                && fact
                    .target
                    .as_deref()
                    .is_some_and(|target| target == "module:src/rust/adapters/parsing/resolved.rs")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "SYMBOL"
                && fact.path == "src/rust/adapters/parsing/mod.rs"
                && fact.target.as_deref().is_some_and(|target| {
                    target == "module:src/rust/adapters/parsing/custom_alias.rs"
                })
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/rust/adapters/parsing/mod.rs"
                && fact.target.as_deref() == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=rust_module_resolution")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/rust/adapters/parsing/mod.rs"
                && fact.target.as_deref() == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=rust_module_resolution")
        }));
    }

    #[test]
    fn tsjs_express_exact_routes_form_family_without_worker() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("express_exact_routes", "tsjs-release-express-exact-routes");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(derived.len(), 5, "five exact express routes derive support");
        assert!(derived
            .iter()
            .all(|(_, target, _)| target.starts_with("express.route.")));
        // The object-literal lookalike and dynamic receiver never derive support.
        assert!(derived.iter().all(|(path, _, _)| path != "lookalikes.ts"));
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "package.json"
                && fact.target.as_deref() == Some("package:express")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "tsconfig.json"
                && fact
                    .target
                    .as_deref()
                    .is_some_and(|target| target.starts_with("tsconfig.path_alias:"))
        }));
        let resolved_imports = facts
            .facts
            .iter()
            .filter(|fact| fact.kind == "RESOLVED_IMPORT")
            .filter_map(|fact| fact.target.as_deref())
            .collect::<Vec<_>>();
        assert!(resolved_imports.contains(&"module:src/lib/client.ts"));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/import_context.ts"
                && fact.target.as_deref() == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=unresolved_path_alias")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/import_context.ts"
                && fact.target.as_deref() == Some("DynamicImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=dynamic_import")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/import_context.ts"
                && fact.target.as_deref() == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=ambiguous_reexport")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "lookalikes.ts"
                && fact.target.as_deref() == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=dynamic_route_call")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "lookalikes.ts"
                && fact.target.as_deref() == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=unresolved_express_receiver")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "lookalikes.ts"
                && fact.target.as_deref() == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=unsafe_receiver_binding")
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        assert_eq!(family_array[0]["classification"], "DOMINANT_PATTERN");
        assert_eq!(family_array[0]["support"], 5);
        let family_id = family_array[0]["family_id"]
            .as_str()
            .expect("family id")
            .to_string();
        assert!(family_id.starts_with("family:typescript:express_route:"));

        // Default family detail is metadata-only and source-free.
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[&family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
        assert_eq!(detail_json["read_plan"]["source_snippets_included"], false);
        assert_read_plan_item_has_line_range(&detail_json["read_plan"]["items"][0]);
        let members = detail_json["members"].as_array().expect("members");
        assert_eq!(members.len(), 5);
        assert!(members
            .iter()
            .all(|member| member["role"] == "framework:express.route_handler"));

        // find resolves the family; check stays advisory CONTEXT_ONLY.
        let find = run_with_runtime(
            cli_args("find", workspace.path(), &["app.ts", "--json"]),
            &runtime,
        );
        let find_json = parse_machine_output("find", &find, &workspace);
        assert_eq!(find_json["status"], "ok");
        assert_eq!(find_json["family"]["family_id"], family_id);
        let check = run_with_runtime(
            cli_args("check", workspace.path(), &["app.ts", "--json"]),
            &runtime,
        );
        let check_json = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_json["status"], "CONTEXT_ONLY");
        assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");

        // Default MCP output is also source-free.
        let default_mcp = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({"operation": "show_family", "target": family_id}),
        );
        assert_eq!(default_mcp["source_spans"]["requested"], false);
        assert_eq!(default_mcp["family"]["family_id"], family_id);

        // Explicit opt-in renders bounded, hash-checked, line-numbered spans only.
        let spanned = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[&family_id, "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        assert_eq!(spanned.status, 0, "stderr: {}", spanned.stderr);
        let spanned_json: Value =
            serde_json::from_str(spanned.stdout.trim()).expect("span json parses");
        assert_eq!(spanned_json["source_spans"]["requested"], true);
        assert_eq!(
            spanned_json["source_spans"]["source_snippets_included"],
            true
        );
        let spans = spanned_json["source_spans"]["spans"]
            .as_array()
            .expect("spans");
        assert!(!spans.is_empty());
        assert!(spans.iter().all(|span| {
            let text = span["text"].as_str().expect("span text");
            text.contains('\t')
                && span["start_line"].as_u64().expect("start line")
                    <= span["end_line"].as_u64().expect("end line")
        }));
        assert!(spans.iter().any(|span| {
            let text = span["text"].as_str().expect("span text");
            text.contains("app.get(") || text.contains("router.put(")
        }));
        assert_eq!(spanned_json["read_plan"]["source_snippets_included"], true);

        // MCP opt-in renders the same bounded spans.
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let mcp_spanned = handle_context_call(
            &runtime,
            &context,
            &serde_json::json!({
                "operation": "show_family",
                "target": family_id,
                "include_source_spans": true,
            }),
        )
        .expect("mcp source-span payload");
        assert_eq!(mcp_spanned["source_spans"]["requested"], true);
        assert_eq!(
            mcp_spanned["source_spans"]["source_snippets_included"],
            true
        );
        assert!(!mcp_spanned["source_spans"]["spans"]
            .as_array()
            .expect("mcp spans")
            .is_empty());

        // find also honors the explicit source-span opt-in on the product path.
        let find_spanned = run_with_runtime(
            cli_args(
                "find",
                workspace.path(),
                &["app.ts", "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        assert_eq!(find_spanned.status, 0, "stderr: {}", find_spanned.stderr);
        assert!(!find_spanned
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let find_spanned_json: Value =
            serde_json::from_str(find_spanned.stdout.trim()).expect("find span json parses");
        assert_eq!(find_spanned_json["status"], "ok");
        assert_eq!(find_spanned_json["source_spans"]["requested"], true);
        assert_eq!(
            find_spanned_json["source_spans"]["source_snippets_included"],
            true
        );
        assert!(find_spanned_json["source_spans"]["spans"]
            .as_array()
            .expect("find spans")
            .iter()
            .any(|span| span["text"]
                .as_str()
                .is_some_and(|text| { text.contains('\t') && text.contains("app.get(") })));

        let mcp_find_spanned = handle_context_call(
            &runtime,
            &context,
            &serde_json::json!({
                "operation": "find_analogues",
                "target": "app.ts",
                "include_source_spans": true,
            }),
        )
        .expect("mcp find source-span payload");
        assert_eq!(mcp_find_spanned["source_spans"]["requested"], true);
        assert!(!mcp_find_spanned
            .to_string()
            .contains(workspace.path().to_string_lossy().as_ref()));

        fs::write(
            workspace.path().join("app.ts"),
            "import express from \"express\";\nconst app = express();\napp.get(\"/users\", (req, res) => res.json([\"changed\"]));\n",
        )
        .expect("mutate express fixture");
        let stale = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[&family_id, "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_python_stale_unknown("family", &stale_json, &family_id);
        assert!(stale_json.get("source_spans").is_none());

        let mcp_stale = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "show_family",
                "target": family_id,
                "include_source_spans": true,
            }),
        );
        assert_python_stale_unknown("family", &mcp_stale, &family_id);
    }

    #[test]
    fn tsjs_jest_vitest_exact_tests_form_suite_and_test_families() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("jest_vitest_exact_tests", "tsjs-release-jest-vitest-exact");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        let suites = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.describe")
            .count();
        let cases = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.it" || target == "jest_vitest.test")
            .count();
        assert_eq!(
            suites, 3,
            "three imported-runner describe suites, including aliases"
        );
        assert_eq!(
            cases, 6,
            "six imported-runner test cases, including aliases"
        );
        // The custom-wrapper test file derives no support.
        assert!(derived.iter().all(|(path, _, _)| path != "wrapper.test.ts"));
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "wrapper.test.ts"
                && fact.target.as_deref() == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=unsafe_test_runner_binding")
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 3);
        let family_ids = family_array
            .iter()
            .map(|family| family["family_id"].as_str().expect("family id").to_string())
            .collect::<Vec<_>>();
        assert!(family_ids
            .iter()
            .any(|id| id.starts_with("family:typescript:test_suite:")));
        assert_eq!(
            family_ids
                .iter()
                .filter(|id| id.starts_with("family:typescript:test_case:"))
                .count(),
            2
        );

        for family in family_array {
            let family_id = family["family_id"].as_str().expect("family id");
            let detail = run_with_runtime(
                cli_args("family", workspace.path(), &[family_id, "--json"]),
                &runtime,
            );
            let detail_json = parse_machine_output("family", &detail, &workspace);
            assert_eq!(detail_json["status"], "ok");
            assert_eq!(detail_json["output"]["source_snippets_included"], false);
        }

        let case_family_id = family_ids
            .iter()
            .find(|id| id.starts_with("family:typescript:test_case:"))
            .expect("test case family id");
        let spanned = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[case_family_id, "--include-source-spans", "--json"],
            ),
            &runtime,
        );
        assert_eq!(spanned.status, 0, "stderr: {}", spanned.stderr);
        assert!(!spanned
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let spanned_json: Value =
            serde_json::from_str(spanned.stdout.trim()).expect("Jest/Vitest span json parses");
        assert_eq!(spanned_json["source_spans"]["requested"], true);
        assert_eq!(
            spanned_json["source_spans"]["source_snippets_included"],
            true
        );
        let spans = spanned_json["source_spans"]["spans"]
            .as_array()
            .expect("Jest/Vitest spans");
        assert!(!spans.is_empty());
        assert!(spans.iter().all(|span| {
            let text = span["text"].as_str().expect("span text");
            text.contains('\t') && !text.contains("wrapper.test.ts")
        }));
        assert!(spans.iter().any(|span| {
            let text = span["text"].as_str().expect("span text");
            text.contains("it(") || text.contains("test(") || text.contains("case_(")
        }));
    }

    #[test]
    fn tsjs_jest_vitest_ambient_project_context_forms_families() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "jest_vitest_ambient_context_tests",
            "tsjs-release-jest-vitest-ambient-context",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        let suites = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.describe")
            .count();
        let cases = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.it" || target == "jest_vitest.test")
            .count();
        assert_eq!(
            suites, 3,
            "ambient describe in test files with package context derives support"
        );
        assert_eq!(
            cases, 6,
            "ambient it/test in test files with package context derives support"
        );
        assert!(derived
            .iter()
            .all(|(path, _, _)| path != "src/not_a_test.ts"));
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "src/not_a_test.ts"
                && fact.target.as_deref() == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=unresolved_test_runner")
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 3);
        assert!(family_array.iter().any(|family| {
            family["family_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("family:typescript:test_suite:"))
                && family["support"] == 3
        }));
        assert_eq!(
            family_array
                .iter()
                .filter(|family| family["family_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("family:typescript:test_case:"))
                    && family["support"] == 3)
                .count(),
            2
        );
    }

    #[test]
    fn tsjs_javascript_exact_routes_form_family_without_worker() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "javascript_exact_routes",
            "tsjs-release-javascript-exact-routes",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(derived.len(), 3);
        assert!(derived
            .iter()
            .all(|(path, target, _)| path == "app.js" && target.starts_with("express.route.")));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert_eq!(family_array.len(), 1);
        let family_id = family_array[0]["family_id"].as_str().expect("family id");
        assert!(family_id.starts_with("family:javascript:express_route:"));
        assert_eq!(family_array[0]["support"], 3);
        let detail = run_with_runtime(
            cli_args("family", workspace.path(), &[family_id, "--json"]),
            &runtime,
        );
        let detail_json = parse_machine_output("family", &detail, &workspace);
        assert_eq!(detail_json["status"], "ok");
        assert_eq!(detail_json["output"]["source_snippets_included"], false);
    }

    #[test]
    fn tsjs_javascript_jest_vitest_exact_tests_form_families() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "javascript_jest_vitest_exact_tests",
            "tsjs-release-javascript-jest-vitest-exact",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "jest_vitest.describe")
                .count(),
            3
        );
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "jest_vitest.it" || target == "jest_vitest.test")
                .count(),
            6
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert!(family_array.iter().all(|family| {
            family["family_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("family:javascript:test_"))
        }));
        assert!(family_array.iter().any(|family| {
            family["family_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("family:javascript:test_suite:"))
                && family["support"] == 3
        }));
        assert!(family_array.iter().any(|family| {
            family["family_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("family:javascript:test_case:"))
                && family["support"]
                    .as_u64()
                    .is_some_and(|support| support >= 3)
        }));
    }

    #[test]
    fn tsjs_unsupported_framework_lookalikes_do_not_form_public_families() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "unsupported_framework_lookalikes",
            "tsjs-release-unsupported-framework-lookalikes",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert!(
            derived.is_empty(),
            "React/Next/Fastify/Prisma/Drizzle lookalikes must not derive JS/TS support"
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "package.json"
                && fact.target.as_deref() == Some("package:react")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "PROJECT_CONFIG"
                && fact.path == "package.json"
                && fact.target.as_deref() == Some("package:next")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.path == "fastify_route.ts"
                && fact.target.as_deref() == Some("FrameworkMagic")
        }));
    }

    fn assert_family_role(value: &Value, role: &str) {
        let role_token = role
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        assert!(
            value["families"]
                .as_array()
                .expect("families")
                .iter()
                .any(|family| {
                    family["family_id"]
                        .as_str()
                        .is_some_and(|family_id| family_id.contains(&role_token))
                        && family["support"]
                            .as_u64()
                            .is_some_and(|support| support >= 3)
                }),
            "missing family role {role}: {value}"
        );
    }

    #[test]
    fn tsjs_next_exact_routes_form_preview_families_without_worker() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("next_exact_routes", "tsjs-release-next-exact-routes");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived.len(),
            16,
            "Next fixture should derive all exact anchors: {derived:?}"
        );
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.app.page"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.app.layout"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.route.GET"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.route.POST"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.pages.api_route"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "next.pages.page"));
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:next.app.page");
        assert_family_role(&families_json, "framework:next.app.layout");
        assert_family_role(&families_json, "framework:next.route.handler");
        assert_family_role(&families_json, "framework:next.pages.api_route");
        assert_family_role(&families_json, "framework:next.pages.page");
    }

    #[test]
    fn tsjs_fastify_exact_routes_form_preview_family_without_worker() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("fastify_exact_routes", "tsjs-release-fastify-exact-routes");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived.len(),
            6,
            "Fastify fixture should derive exact routes"
        );
        assert!(derived
            .iter()
            .all(|(_, target, _)| target.starts_with("fastify.route.")));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "fastify.route.route"));
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:fastify.route_handler");
    }

    #[test]
    fn tsjs_prisma_exact_repositories_form_preview_family_without_worker() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "prisma_exact_repositories",
            "tsjs-release-prisma-exact-repositories",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived.len(),
            6,
            "Prisma fixture should derive queries and transaction"
        );
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "prisma.query.findMany")
                .count(),
            3
        );
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "prisma.transaction"));
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "prisma.query.create")
                .count(),
            2
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:prisma.query");
    }

    #[test]
    fn tsjs_drizzle_exact_repositories_form_preview_families_without_worker() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "drizzle_exact_repositories",
            "tsjs-release-drizzle-exact-repositories",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived.len(),
            9,
            "Drizzle fixture should derive schema and queries"
        );
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "drizzle.schema.table")
                .count(),
            3
        );
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "drizzle.query.select")
                .count(),
            3
        );
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "drizzle.query.query_findMany"));
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "drizzle.query.query_findFirst"));
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:drizzle.schema.table");
        assert_family_role(&families_json, "framework:drizzle.query");
    }

    #[test]
    fn tsjs_zod_exact_schemas_form_preview_family_without_worker() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("zod_exact_schemas", "tsjs-release-zod-exact-schemas");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "zod.object")
                .count(),
            3,
            "Zod fixture should derive three exact object schemas: {derived:?}"
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:zod.schema");
    }

    #[test]
    fn tsjs_nest_exact_controllers_form_route_family_without_worker() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "nest_exact_controllers",
            "tsjs-release-nest-exact-controllers",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "nestjs.common.Get")
                .count(),
            3,
            "Nest controller fixture should derive three exact routes: {derived:?}"
        );
        assert!(derived
            .iter()
            .any(|(_, target, _)| target == "nestjs.common.Controller"));
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:nestjs.route");
    }

    #[test]
    fn tsjs_hono_exact_routes_form_preview_family_without_worker() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("hono_exact_routes", "tsjs-release-hono-exact-routes");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "hono.route.get")
                .count(),
            3,
            "Hono fixture should derive three exact routes: {derived:?}"
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:hono.route");
    }

    #[test]
    fn tsjs_mocha_exact_tests_form_runner_kind_mocha_family() {
        let (workspace, runtime) =
            index_release_v0_2_fixture("mocha_exact_tests", "tsjs-release-mocha-exact-tests");

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived
                .iter()
                .filter(|(_, target, _)| target == "mocha.it")
                .count(),
            3,
            "Mocha fixture should derive three exact test cases: {derived:?}"
        );
        // The mocha alias never adopts a jest/vitest target.
        assert!(!derived
            .iter()
            .any(|(_, target, _)| target.starts_with("jest_vitest.")));
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(
            facts.facts.iter().any(|fact| {
                fact.origin_engine == "repogrammar-tsjs-derived"
                    && fact.target.as_deref() == Some("mocha.it")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "runner_kind=mocha")
            }),
            "mocha test support facts must carry runner_kind=mocha"
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_family_role(&families_json, "framework:jest_vitest.test");
    }

    #[test]
    fn tsjs_new_framework_lookalikes_do_not_form_public_families() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "tsjs_new_framework_lookalikes",
            "tsjs-release-new-framework-lookalikes",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert!(
            derived.is_empty(),
            "Zod/Nest/Hono lookalikes without exact imports must not derive support: {derived:?}"
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_tsjs_unknown(
            &facts,
            "src/nest_lookalike.ts",
            "UnresolvedImport",
            "nest_unresolved_controller_import",
        );
        assert_tsjs_unknown(
            &facts,
            "src/nest_lookalike.ts",
            "UnresolvedImport",
            "nest_unresolved_route_import",
        );
        assert_tsjs_unknown(
            &facts,
            "src/hono_lookalike.ts",
            "UnresolvedImport",
            "unresolved_express_receiver",
        );
    }

    #[test]
    fn tsjs_framework_adapter_negative_cases_do_not_form_public_families() {
        let (workspace, runtime) = index_release_v0_2_fixture(
            "framework_adapter_negative_cases",
            "tsjs-release-framework-adapter-negative-cases",
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        assert_eq!(
            derived,
            vec![(
                "src/drizzle_raw.ts".to_string(),
                "drizzle.schema.table".to_string(),
                "DATAFLOW_DERIVED".to_string()
            )],
            "negative fixture should only derive the exact schema table, not route/query support"
        );
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_tsjs_unknown(
            &facts,
            "src/express_shadow.ts",
            "ConflictingFacts",
            "unsafe_receiver_binding",
        );
        assert_tsjs_unknown(
            &facts,
            "src/fastify_dynamic.ts",
            "FrameworkMagic",
            "fastify_dynamic_route_call",
        );
        assert_tsjs_unknown(
            &facts,
            "src/fastify_shadow.ts",
            "ConflictingFacts",
            "fastify_receiver_reassigned",
        );
        assert_tsjs_unknown(
            &facts,
            "src/prisma_dynamic.ts",
            "UnresolvedImport",
            "prisma_injected_client",
        );
        assert_tsjs_unknown(
            &facts,
            "src/prisma_shadow.ts",
            "UnresolvedImport",
            "prisma_injected_client",
        );
        assert_tsjs_unknown(
            &facts,
            "src/prisma_raw.ts",
            "FrameworkMagic",
            "prisma_raw_query",
        );
        assert_tsjs_unknown(
            &facts,
            "src/prisma_bulk.ts",
            "FrameworkMagic",
            "prisma_dynamic_model_or_operation",
        );
        assert_tsjs_unknown(
            &facts,
            "src/drizzle_raw.ts",
            "FrameworkMagic",
            "drizzle_raw_sql",
        );
        assert_tsjs_unknown(
            &facts,
            "src/drizzle_shadow.ts",
            "UnresolvedImport",
            "drizzle_db_binding_unresolved",
        );
        assert_tsjs_unknown(
            &facts,
            "src/drizzle_raw_execute.ts",
            "FrameworkMagic",
            "drizzle_raw_sql",
        );
        assert_tsjs_unknown(
            &facts,
            "src/jest_shadow.test.ts",
            "ConflictingFacts",
            "unsafe_test_runner_binding",
        );
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .is_empty());
    }

    #[test]
    fn tsjs_v0_1_jest_vitest_basic_ambient_tests_require_project_context() {
        let workspace = TempWorkspace::new("tsjs-v0-1-jest-vitest-basic-ambient");
        copy_release_fixture("jest-vitest-basic", workspace.path());
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("init", &init, &workspace)["status"],
            "initialized"
        );
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        assert_eq!(
            parse_machine_output("index", &index, &workspace)["status"],
            "complete"
        );

        let derived = tsjs_derived_support_facts(&runtime, &workspace);
        let suites = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.describe")
            .count();
        let cases = derived
            .iter()
            .filter(|(_, target, _)| target == "jest_vitest.it" || target == "jest_vitest.test")
            .count();
        assert_eq!(
            suites, 0,
            "ambient describe without package/config context must not support a family"
        );
        assert_eq!(
            cases, 0,
            "ambient it/test without package/config context must not support a family"
        );
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open indexed store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert!(facts.facts.iter().any(|fact| {
            fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("MissingProjectConfig")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "tsjs_unknown_kind=ambient_runner_without_project_context"
                })
        }));

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        let family_array = families_json["families"].as_array().expect("families");
        assert!(
            family_array.is_empty(),
            "ambient suites/tests without project context must stay UNKNOWN"
        );
    }

    #[test]
    fn python_release_fixture_records_exact_anchor_variation_evidence() {
        let workspace = TempWorkspace::new("python-release-derived-anchor-variation");
        copy_python_release_fixture("fastapi-route-variation", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["semantic_worker"], "deferred");

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let units_json = parse_machine_output("units", &units, &workspace);
        let route_units = units_json["units"]
            .as_array()
            .expect("units array")
            .iter()
            .filter(|unit| {
                unit["language"] == "python"
                    && unit["kind"] == "fastapi_route"
                    && unit["path"] == "routes.py"
            })
            .collect::<Vec<_>>();
        assert_eq!(route_units.len(), 14);
        for unit in route_units {
            assert_repo_relative_json_path(&unit["path"]);
            assert_content_hash_json(&unit["content_hash"]);
            assert!(
                unit["start_byte"].as_u64().expect("unit start")
                    < unit["end_byte"].as_u64().expect("unit end")
            );
        }

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        let mut derived_targets = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-python-derived"
                    && fact.origin_method == "bounded_ast_anchor_v1"
                    && fact.path == "routes.py"
            })
            .map(|fact| fact.target.as_deref().expect("support target").to_string())
            .collect::<Vec<_>>();
        derived_targets.sort();
        assert_eq!(
            derived_targets,
            vec![
                "fastapi.APIRouter.delete".to_string(),
                "fastapi.APIRouter.get".to_string(),
                "fastapi.APIRouter.head".to_string(),
                "fastapi.APIRouter.options".to_string(),
                "fastapi.APIRouter.patch".to_string(),
                "fastapi.APIRouter.post".to_string(),
                "fastapi.APIRouter.put".to_string(),
                "fastapi.FastAPI.delete".to_string(),
                "fastapi.FastAPI.get".to_string(),
                "fastapi.FastAPI.head".to_string(),
                "fastapi.FastAPI.options".to_string(),
                "fastapi.FastAPI.patch".to_string(),
                "fastapi.FastAPI.post".to_string(),
                "fastapi.FastAPI.put".to_string(),
            ]
        );

        let family_id = "family:python:fastapi_route:framework_fastapi_route";
        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        assert_eq!(
            families_json["families"]
                .as_array()
                .expect("families array")
                .len(),
            1
        );
        assert_eq!(families_json["families"][0]["family_id"], family_id);
        assert_eq!(families_json["families"][0]["support"], 14);

        let family = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[
                    family_id,
                    "--mode",
                    "evidence",
                    "--include-variations",
                    "--json",
                ],
            ),
            &runtime,
        );
        let family_json = parse_machine_output("family", &family, &workspace);
        assert_eq!(family_json["status"], "ok");
        assert_eq!(family_json["family"]["family_id"], family_id);
        assert_eq!(family_json["family"]["support"], 14);
        let members = family_json["members"].as_array().expect("members");
        assert_eq!(members.len(), 14);
        assert!(members.iter().all(|member| {
            member["family_id"] == family_id && member["role"] == "framework:fastapi.route"
        }));
        assert_eq!(
            family_json["output"]["covered_claims"],
            serde_json::json!(["canonical", "support", "variation"])
        );
        assert_eq!(
            family_json["output"]["missing_claims"],
            serde_json::json!([])
        );
        assert_eq!(family_json["output"]["source_snippets_included"], false);
        assert!(family_json["variation_slots"]
            .as_array()
            .expect("variation slots")
            .iter()
            .any(|slot| slot["slot_id"] == "slot:python_framework_anchor_target"));
        let evidence = family_json["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 2);
        assert!(evidence.iter().any(|record| {
            record["covered_claims"] == serde_json::json!(["canonical", "support"])
        }));
        assert!(evidence.iter().any(|record| {
            record["covered_claims"] == serde_json::json!(["support", "variation"])
        }));
        for record in evidence {
            assert_eq!(record["family_id"], family_id);
            assert_eq!(record["path"], "routes.py");
            assert_repo_relative_json_path(&record["path"]);
            assert_content_hash_json(&record["content_hash"]);
            assert!(
                record["start_byte"].as_u64().expect("start")
                    < record["end_byte"].as_u64().expect("end")
            );
        }

        // End-to-end profile persistence + hydration: real indexing persisted the
        // co-derived constraint profile, and the family detail now carries it.
        let profile = &family_json["constraint_profile"];
        assert!(
            profile.is_object(),
            "family detail must carry a hydrated constraint profile: {family_json}"
        );
        let required = profile["required_equal_features"]
            .as_array()
            .expect("required_equal_features array");
        assert!(
            required.iter().any(|constraint| {
                constraint["origin"] == "framework_role_identity"
                    && constraint["semantics"] == "equal"
            }),
            "the framework-role identity is a required-equal feature: {profile}"
        );
        // Python families carry no unknown-blocker prohibition.
        assert!(profile["prohibited_or_blocking_features"]
            .as_array()
            .expect("prohibited array")
            .is_empty());
        let obligations = profile["unresolved_obligations"]
            .as_array()
            .expect("unresolved_obligations array");
        assert!(
            obligations.iter().any(|obligation| {
                obligation["affected_claim"]
                    .as_str()
                    .is_some_and(|claim| claim.ends_with(":runtime_equivalence"))
            }),
            "the always-present runtime-equivalence obligation must be listed: {profile}"
        );

        // The read plan leads with the canonical (medoid) evidence for an exact
        // family-id lookup with no edit target.
        let read_plan_items = family_json["read_plan"]["items"]
            .as_array()
            .expect("read_plan items array");
        assert_eq!(read_plan_items[0]["purpose"], "canonical_evidence");
    }

    #[test]
    fn python_exact_anchor_queries_return_stale_unknown_without_worker() {
        fn assert_stale_queries(
            runtime: &ProductCliRuntime,
            workspace: &TempWorkspace,
            case: PythonExactAnchorSmokeCase,
            member_id: &str,
        ) {
            let families =
                run_with_runtime(cli_args("families", workspace.path(), &["--json"]), runtime);
            let families_json = parse_machine_output("families", &families, workspace);
            assert_eq!(families_json["command"], "families");
            // The listing stays served, but now qualifies the family as stale and
            // carries the report-level stale-evidence signal instead of serving it
            // as an unqualified usable claim.
            assert_eq!(families_json["status"], "ok");
            let stale_family = families_json["families"]
                .as_array()
                .expect("families")
                .iter()
                .find(|family| family["family_id"] == case.family_id)
                .expect("stale family still listed");
            assert_eq!(stale_family["freshness"], "stale");
            assert_eq!(families_json["stale_count"], 1);
            assert!(families_json["unknowns"]
                .as_array()
                .expect("unknowns")
                .iter()
                .any(|unknown| unknown["reason"] == "StaleEvidence"));

            for (command, target) in [
                ("family", case.family_id),
                ("member", member_id),
                ("find", case.evidence_path),
                ("explain", case.evidence_path),
                ("check", case.evidence_path),
            ] {
                let output = run_with_runtime(
                    cli_args(command, workspace.path(), &[target, "--json"]),
                    runtime,
                );
                let value = parse_machine_output(command, &output, workspace);
                assert_python_stale_unknown(command, &value, case.family_id);
            }
        }

        let case = *PYTHON_EXACT_ANCHOR_SMOKE_CASES
            .iter()
            .find(|case| case.fixture == "stale-evidence")
            .expect("stale-evidence exact-anchor case");

        for mode in ["mutated", "deleted"] {
            let workspace =
                TempWorkspace::new(&format!("python-release-derived-family-stale-{mode}"));
            copy_python_release_fixture(case.fixture, workspace.path());
            let runtime = ProductCliRuntime;

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let index = run_with_runtime(
                cli_args(
                    "index",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let index_json = parse_machine_output("index", &index, &workspace);
            assert_eq!(index_json["status"], "complete");

            let family = run_with_runtime(
                cli_args("family", workspace.path(), &[case.family_id, "--json"]),
                &runtime,
            );
            let family_json = parse_machine_output("family", &family, &workspace);
            assert_eq!(family_json["status"], "ok");
            let member_id = assert_python_exact_anchor_family_detail("family", &family_json, case);

            let evidence_path = workspace.path().join(case.evidence_path);
            match mode {
                "mutated" => fs::write(&evidence_path, "# stale replacement\n")
                    .expect("mutate exact-anchor evidence file"),
                "deleted" => {
                    fs::remove_file(&evidence_path).expect("delete exact-anchor evidence file")
                }
                _ => unreachable!("covered stale mode"),
            }

            assert_stale_queries(&runtime, &workspace, case, &member_id);
        }
    }

    #[cfg(unix)]
    #[test]
    fn python_release_fixture_strong_fastapi_support_produces_family_then_stale_unknown() {
        let workspace = TempWorkspace::new("python-release-positive-family");
        copy_python_release_fixture("positive-strong-evidence", workspace.path());
        let worker_script = semantic_support_worker_script(&workspace);
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: DEFAULT_MAX_FILE_BYTES,
                    strict_gitignore: false,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                    progress: ProgressMode::Never,
                    json: false,
                    quiet: true,
                    stderr_is_terminal: false,
                },
            )
            .expect("index Python release fixture with semantic support worker");
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::Complete
        );
        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert!(
            outcome.semantic_facts >= 6,
            "Python fixture should store parser/framework facts plus three semantic support facts"
        );
        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        let support_facts = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "python-fixture-provider"
                    && fact.origin_method == "release_fixture_semantic_support"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            support_facts.len(),
            3,
            "fixture provider should emit exactly one strong support fact per route"
        );
        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let units_json = parse_machine_output("units", &units, &workspace);
        let route_units = units_json["units"]
            .as_array()
            .expect("units array")
            .iter()
            .filter(|unit| unit["language"] == "python" && unit["kind"] == "fastapi_route")
            .collect::<Vec<_>>();
        assert_eq!(route_units.len(), 3);
        for fact in &support_facts {
            assert_eq!(fact.certainty, "SEMANTIC");
            assert_eq!(fact.origin_engine_version, "0.1.0");
            assert_eq!(fact.target.as_deref(), Some("fastapi.APIRouter.get"));
            assert!(route_units.iter().any(|unit| {
                unit["id"].as_str() == Some(fact.code_unit_id.as_str())
                    && unit["path"].as_str() == Some(fact.path.as_str())
                    && unit["content_hash"].as_str() == Some(fact.content_hash.as_str())
                    && unit["start_byte"].as_u64() == Some(fact.start_byte as u64)
                    && unit["end_byte"].as_u64() == Some(fact.end_byte as u64)
            }));
        }
        assert!(
            facts.facts.iter().all(|fact| {
                !(fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "SEMANTIC")
            }),
            "CPython parser facts must never be promoted to SEMANTIC"
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        assert_eq!(
            families_json["families"]
                .as_array()
                .expect("families")
                .len(),
            1
        );
        let family_id = families_json["families"][0]["family_id"]
            .as_str()
            .expect("family id")
            .to_string();
        assert_eq!(
            family_id,
            "family:python:fastapi_route:framework_fastapi_route"
        );
        assert_eq!(families_json["families"][0]["support"], 3);

        for command in ["family", "find", "explain"] {
            let args = if command == "family" {
                vec![family_id.as_str(), "--json"]
            } else {
                vec!["routes.py", "--json"]
            };
            let output = run_with_runtime(cli_args(command, workspace.path(), &args), &runtime);
            let value = parse_machine_output(command, &output, &workspace);
            assert_eq!(value["status"], "ok", "{command} should find family");
            assert_eq!(value["family"]["family_id"], family_id);
            assert_eq!(value["family"]["support"], 3);
            assert_eq!(value["members"].as_array().expect("members").len(), 3);
            assert!(value["members"]
                .as_array()
                .expect("members")
                .iter()
                .all(|member| member["role"] == "framework:fastapi.route"));
            assert_eq!(value["output"]["mode"], "compact");
            assert_eq!(value["output"]["source_snippets_included"], false);
            assert!(value["evidence"].as_array().expect("evidence").is_empty());
            assert_eq!(
                value["unknowns"][0]["reason"], "FrameworkMagic",
                "runtime equivalence must remain non-blocking UNKNOWN"
            );
        }

        let evidence = run_with_runtime(
            cli_args(
                "find",
                workspace.path(),
                &["routes.py", "--mode", "evidence", "--json"],
            ),
            &runtime,
        );
        let evidence_json = parse_machine_output("find", &evidence, &workspace);
        assert_eq!(evidence_json["status"], "ok");
        assert_eq!(evidence_json["output"]["mode"], "evidence");
        assert_eq!(evidence_json["output"]["source_snippets_included"], false);
        assert_eq!(
            evidence_json["output"]["selection_strategy"],
            "greedy_marginal_coverage_v1"
        );
        assert_eq!(
            evidence_json["output"]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );
        assert_eq!(
            evidence_json["output"]["missing_claims"],
            serde_json::json!([])
        );
        assert_eq!(
            evidence_json["evidence"]
                .as_array()
                .expect("evidence")
                .len(),
            1
        );
        assert_eq!(
            evidence_json["evidence"][0]["covered_claims"],
            serde_json::json!(["canonical", "support"])
        );

        let check = run_with_runtime(
            cli_args("check", workspace.path(), &["routes.py", "--json"]),
            &runtime,
        );
        let check_json = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_json["status"], "CONTEXT_ONLY");
        assert_eq!(check_json["check"]["advisory_status"], "UNKNOWN");

        fs::write(
            workspace.path().join("routes.py"),
            "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get(\"/changed\")\ndef changed_route():\n    return []\n",
        )
        .expect("mutate Python route fixture");

        let stale = run_with_runtime(
            cli_args(
                "family",
                workspace.path(),
                &[&family_id, "--mode", "evidence", "--json"],
            ),
            &runtime,
        );
        let stale_json = parse_machine_output("family", &stale, &workspace);
        assert_eq!(stale_json["status"], "UNKNOWN");
        assert!(stale_json.get("evidence").is_none());
        assert_eq!(stale_json["unknowns"][0]["reason"], "StaleEvidence");
        assert_eq!(
            stale_json["unknowns"][0]["recovery"],
            "run repogrammar resync"
        );
    }

    #[cfg(unix)]
    #[test]
    fn product_mcp_self_test_times_out_and_reaps_hanging_child() {
        let workspace = TempWorkspace::new("product-mcp-self-test-timeout");
        let script = executable_script(&workspace, "hang.sh", "#!/bin/sh\nsleep 10\n");
        let tester = ProductMcpSelfTester::with_timeout(std::time::Duration::from_millis(100));
        let started = std::time::Instant::now();

        let error = tester
            .run(
                script.to_str().expect("script path utf8"),
                workspace.path().to_str().expect("workspace path utf8"),
            )
            .expect_err("hanging self-test should time out");

        assert_eq!(error, ProductMcpSelfTestError::TimedOut);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn product_mcp_self_test_accepts_exact_context_tool_list() {
        let workspace = TempWorkspace::new("product-mcp-self-test-tools-list");
        let script = executable_script(
            &workspace,
            "mcp-self-test.sh",
            "#!/bin/sh\nIFS= read -r _request\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"repogrammar_context\"}]}}'\nIFS= read -r _shutdown\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":null}'\n",
        );

        ProductMcpSelfTester::with_timeout(std::time::Duration::from_secs(1))
            .self_test(
                script.to_str().expect("script path utf8"),
                workspace.path().to_str().expect("workspace path utf8"),
            )
            .expect("exact tools/list response should pass");
    }

    #[test]
    fn product_runtime_indexes_and_reports_storage_status() {
        let workspace = TempWorkspace::new("product-runtime");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(cli_args("init", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("init", &init, &workspace);
        assert_eq!(value["resync"]["generation_id"], "gen-000001");
        assert_eq!(value["resync"]["indexed_units"], 1);
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["resync"]["parser"], "syntax_only");
        assert_eq!(value["resync"]["semantic_worker"], "deferred");

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["readiness"]["state"], "ready_active_index");
        assert_eq!(value["readiness"]["query_ready"], true);
        assert_eq!(value["readiness"]["active_generation_available"], true);
        assert_eq!(value["readiness"]["recommended_next_command"], Value::Null);
        assert!(!status
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["files"][0]["path"], "a.ts");
        assert!(!files
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["mining"], "deferred");
        assert_eq!(value["units"][0]["path"], "a.ts");
        assert!(!units
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn product_runtime_stats_explains_tsx_react_native_unsupported_scope() {
        let workspace = TempWorkspace::new("product-runtime-tsx-unsupported-stats");
        fs::create_dir_all(workspace.path().join("src/navigation")).expect("create navigation");
        fs::create_dir_all(workspace.path().join("src/screens")).expect("create screens");
        fs::write(
            workspace.path().join("package.json"),
            r#"{"dependencies":{"@react-navigation/native":"latest","react-native":"latest"}}"#,
        )
        .expect("write package");
        fs::write(
            workspace.path().join("src/navigation/AppNavigator.tsx"),
            r#"
import { NavigationContainer } from '@react-navigation/native';
import { HomeScreen } from '../screens/HomeScreen';

export function AppNavigator() {
  return <NavigationContainer><HomeScreen /></NavigationContainer>;
}
"#,
        )
        .expect("write navigator");
        fs::write(
            workspace.path().join("src/screens/HomeScreen.tsx"),
            r#"
import { Text, View } from 'react-native';

export function HomeScreen() {
  return <View><Text>Dashboard</Text></View>;
}
"#,
        )
        .expect("write screen");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let resync = run_with_runtime(
            cli_args(
                "resync",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let resync_json = parse_machine_output("resync", &resync, &workspace);
        assert_eq!(resync_json["status"], "complete");
        assert!(
            resync_json["indexed_units"].as_u64().unwrap_or_default() > 0,
            "TSX fixture should produce indexed code units: {resync_json}"
        );

        let stats = run_with_runtime(cli_args("stats", workspace.path(), &[]), &runtime);
        assert_eq!(stats.status, 0, "stats stderr: {}", stats.stderr);
        assert!(stats.stderr.is_empty());
        assert_no_output_leakage("stats", &stats.stdout, &workspace);
        assert!(stats.stdout.contains("official_family_scope: python_v0_1"));
        assert!(stats
            .stdout
            .contains("repo_shape_scope: python_family_eligible_units"));
        assert!(stats.stdout.contains("eligible_code_units: 0"));
        assert!(stats
            .stdout
            .contains("tsjs_indexed_context_available: true"));
        assert!(stats
            .stdout
            .contains("tsjs_family_support: none_or_unsupported"));
        assert!(stats
            .stdout
            .contains("react_rn_family_support: unsupported"));
        assert!(stats.stdout.contains(
            "recommended_next_action: use repogrammar find/check with exact repo-relative paths for PARTIAL_CONTEXT read plans"
        ));

        let stats_json =
            run_with_runtime(cli_args("stats", workspace.path(), &["--json"]), &runtime);
        let stats_payload = parse_machine_output("stats", &stats_json, &workspace);
        assert_eq!(stats_payload["official_family_scope"], "python_v0_1");
        assert_eq!(
            stats_payload["repo_shape_scope"],
            "python_family_eligible_units"
        );
        assert!(
            stats_payload["indexed_inventory"]["indexed_file_count"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert!(
            stats_payload["indexed_inventory"]["indexed_code_unit_count"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(stats_payload["counts"]["eligible_code_units"], 0);
        assert_eq!(
            stats_payload["scope_explanations"]["tsjs_indexed_context_available"],
            true
        );
        assert_eq!(
            stats_payload["scope_explanations"]["tsjs_family_support"],
            "none_or_unsupported"
        );
        assert_eq!(
            stats_payload["scope_explanations"]["react_rn_family_support"],
            "unsupported"
        );
        assert_eq!(
            stats_payload["scope_explanations"]["recommended_next_action"],
            "use repogrammar find/check with exact repo-relative paths for PARTIAL_CONTEXT read plans"
        );
        let tsjs_stats = stats_payload["by_language"]
            .as_array()
            .expect("by_language array")
            .iter()
            .find(|language| language["language"] == "typescript/javascript")
            .expect("tsjs stats");
        assert!(
            tsjs_stats["indexed_file_count"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert!(
            tsjs_stats["indexed_code_unit_count"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(tsjs_stats["eligible_code_units"], 0);
        assert_eq!(tsjs_stats["family_count"], 0);

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_payload = parse_machine_output("families", &families, &workspace);
        if let Some(families) = families_payload["families"].as_array() {
            assert!(families.iter().all(|family| {
                let family_id = family["family_id"].as_str().unwrap_or_default();
                !family_id.contains("react") && !family_id.contains("rn")
            }));
        }

        let check = run_with_runtime(
            cli_args(
                "check",
                workspace.path(),
                &["src/screens/HomeScreen.tsx", "--json"],
            ),
            &runtime,
        );
        let check_payload = parse_machine_output("check", &check, &workspace);
        assert_eq!(check_payload["status"], "PARTIAL_CONTEXT");
        assert_eq!(
            check_payload["read_plan"]["requires_source_before_edit"],
            true
        );
        assert_eq!(
            check_payload["read_plan"]["source_snippets_included"],
            false
        );
    }

    #[test]
    fn product_runtime_prunes_inactive_generations_and_preserves_active_status() {
        let workspace = TempWorkspace::new("product-runtime-prune");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        for version in 1..=4 {
            fs::write(
                workspace.path().join("a.ts"),
                format!("export const a = {version};\n"),
            )
            .expect("write source");
            let index =
                run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
            assert_eq!(index.status, 0, "{index:?}");
        }
        assert_eq!(stored_generation_count(workspace.path()), 4);

        let dry_run = run_with_runtime(
            cli_args(
                "prune",
                workspace.path(),
                &["--dry-run", "--keep", "1", "--json"],
            ),
            &runtime,
        );
        assert_eq!(dry_run.status, 0, "{dry_run:?}");
        let value: Value = serde_json::from_str(dry_run.stdout.trim()).expect("prune JSON");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(value["candidate_generations"].as_array().unwrap().len(), 2);
        assert_eq!(value["deleted_generations"].as_array().unwrap().len(), 0);
        assert!(!dry_run
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        assert_eq!(stored_generation_count(workspace.path()), 4);

        let prune = run_with_runtime(
            cli_args(
                "prune",
                workspace.path(),
                &["--yes", "--keep", "1", "--json"],
            ),
            &runtime,
        );
        assert_eq!(prune.status, 0, "{prune:?}");
        let value: Value = serde_json::from_str(prune.stdout.trim()).expect("prune JSON");
        assert_eq!(value["status"], "complete");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(value["retained_inactive_generations"][0], "gen-000003");
        assert_eq!(value["deleted_generations"].as_array().unwrap().len(), 2);
        assert_eq!(stored_generation_count(workspace.path()), 2);
        assert!(!stored_generation_exists(workspace.path(), "gen-000001"));
        assert!(!stored_generation_exists(workspace.path(), "gen-000002"));
        assert!(stored_generation_exists(workspace.path(), "gen-000003"));
        assert!(stored_generation_exists(workspace.path(), "gen-000004"));

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000004");
        assert_eq!(value["storage"], "available");
    }

    #[test]
    fn product_runtime_compacts_mutable_index_and_preserves_active_status() {
        let workspace = TempWorkspace::new("product-runtime-compact");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);
        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0, "{index:?}");

        let dry_run = run_with_runtime(
            cli_args("compact", workspace.path(), &["--dry-run", "--json"]),
            &runtime,
        );
        assert_eq!(dry_run.status, 0, "{dry_run:?}");
        assert!(dry_run.stderr.is_empty());
        let value: Value = serde_json::from_str(dry_run.stdout.trim()).expect("compact JSON");
        assert_eq!(value["command"], "compact");
        assert_eq!(value["status"], "dry_run");
        assert_eq!(value["active_generation"], "gen-000001");
        assert!(value["before"]["total_bytes"].as_u64().unwrap() > 0);
        assert_eq!(value["before"], value["after"]);
        assert!(!dry_run
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let compact = run_with_runtime(
            cli_args("compact", workspace.path(), &["--yes", "--json"]),
            &runtime,
        );
        assert_eq!(compact.status, 0, "{compact:?}");
        let value: Value = serde_json::from_str(compact.stdout.trim()).expect("compact JSON");
        assert_eq!(value["status"], "complete");
        assert_eq!(value["active_generation"], "gen-000001");
        assert!(value["before"]["total_bytes"].as_u64().unwrap() > 0);
        assert!(value["after"]["total_bytes"].as_u64().unwrap() > 0);
        assert!(!compact
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["storage"], "available");
    }

    #[test]
    fn product_runtime_compact_refuses_live_index_lock() {
        let workspace = TempWorkspace::new("product-runtime-compact-lock");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);
        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0, "{index:?}");
        let _guard = repogrammar::application::repository::acquire_index_lock(
            workspace.path().to_string_lossy().as_ref(),
            None,
        )
        .expect("hold index lock");

        let output = run_with_runtime(
            cli_args("compact", workspace.path(), &["--dry-run", "--json"]),
            &runtime,
        );

        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        assert!(!output
            .stderr
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(output.stderr.trim()).expect("error JSON");
        assert_eq!(value["command"], "compact");
        assert!(value["reason"]
            .as_str()
            .expect("reason")
            .contains("index lock is held"));
    }

    #[test]
    fn product_runtime_indexes_framework_roles_without_query_claims() {
        let workspace = TempWorkspace::new("product-runtime-framework-roles");
        fs::write(
            workspace.path().join("component.tsx"),
            "export function UserCard() { return <section />; }\n",
        )
        .expect("write source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        assert!(!index
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["parser"], "syntax_only");
        assert_eq!(value["semantic_worker"], "deferred");
        assert_eq!(value["semantic_facts"], 1);
        assert_eq!(value["mining"], "deferred");

        for command in ["find", "families", "family", "explain", "check"] {
            let output =
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime);
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            assert!(!output
                .stdout
                .contains(workspace.path().to_string_lossy().as_ref()));
            let unknown: Value = serde_json::from_str(output.stdout.trim()).expect("UNKNOWN JSON");
            assert_eq!(unknown["status"], "UNKNOWN");
            assert_eq!(unknown["command"], command);
            assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");
            assert_eq!(unknown["implemented"], true);
        }
    }

    #[test]
    fn product_runtime_persists_python_parse_facts_without_query_claims() {
        let workspace = TempWorkspace::new("product-runtime-python");
        fs::create_dir_all(workspace.path().join("src/acme/services")).expect("create package");
        fs::write(workspace.path().join("src/acme/__init__.py"), "").expect("write init");
        fs::write(workspace.path().join("src/acme/services/__init__.py"), "")
            .expect("write services init");
        fs::write(
            workspace.path().join("src/acme/services/users.py"),
            "def list_users():\n    return []\n",
        )
        .expect("write users module");
        fs::write(
            workspace.path().join("src/acme/api.py"),
            r#"
from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel
from acme.services import users
from .services import users as relative_users
from acme.missing import value

router = APIRouter()

class UserOut(BaseModel):
    id: int

def get_db():
    return object()

@router.get("/users", response_model=UserOut)
async def list_users(dependency=Depends(get_db)):
    if dependency is None:
        raise HTTPException(status_code=400)
    return []

def test_users(client, missing_fixture):
    assert client.get("/users").status_code == 200
"#,
        )
        .expect("write source");
        fs::write(
            workspace.path().join("src/acme/conftest.py"),
            r#"
import pytest

@pytest.fixture
def db():
    return object()

@pytest.fixture
def client(db, tmp_path):
    return object()
"#,
        )
        .expect("write conftest");
        fs::write(
            workspace.path().join("pyproject.toml"),
            r#"
[project]
name = "demo-api"

[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]

[tool.pyrefly]
project_includes = ["src"]
"#,
        )
        .expect("write pyproject");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["indexing"], "syntax_only_code_units");
        assert_eq!(value["semantic_worker"], "deferred");
        assert!(
            value["semantic_facts"].as_u64().unwrap_or_default() > 3,
            "Python parse facts should be stored in addition to framework-role heuristics"
        );

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert!(value["files"]
            .as_array()
            .expect("files")
            .iter()
            .any(|file| file["path"] == "pyproject.toml" && file["language"] == "python-config"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "src/acme/api.py"
                    && unit["language"] == "python"
                    && unit["kind"] == "fastapi_route"
            }));
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "src/acme/api.py"
                    && unit["language"] == "python"
                    && unit["kind"] == "pydantic_model"
            }));
        assert!(value["units"]
            .as_array()
            .expect("units")
            .iter()
            .any(|unit| {
                unit["path"] == "pyproject.toml"
                    && unit["language"] == "python-config"
                    && unit["kind"] == "project_config"
            }));
        assert!(!units
            .stdout
            .contains(workspace.path().to_string_lossy().as_ref()));

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "RESOLVED_IMPORT"
                && fact.target.as_deref() == Some("fastapi.APIRouter")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        let repo_local_imports = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.path == "src/acme/api.py"
                    && fact.kind == "RESOLVED_IMPORT"
                    && fact.target.as_deref() == Some("acme.services.users")
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=repo_local_import_binding"
                    })
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "provider_resolved=false")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "derived_from=repo_local_python_import_graph"
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(repo_local_imports.len(), 2);
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("UnresolvedImport")
                && fact.certainty == "UNKNOWN"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "reason_code=UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        assert!(facts
            .facts
            .iter()
            .filter(|fact| fact.origin_engine == "python")
            .all(|fact| fact.certainty != "SEMANTIC"));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("src.acme.api")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("scope.imported.APIRouter")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("scope.namespace.UserOut")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "TYPE"
                && fact.target.as_deref() == Some("pydantic.BaseModel")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.APIRouter.get")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "TYPE"
                && fact.target.as_deref() == Some("fastapi.response_model.UserOut")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_response_model")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "RESOLVED_CALL"
                && fact.target.as_deref() == Some("client.get")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.dependency.get_db")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("fastapi.http_exception.status_code.400")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=fastapi_http_exception_status"
                })
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.test")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_test_function")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                })
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_pytest_fixture_graph")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/conftest.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.db")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_pytest_fixture_graph")
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/conftest.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.builtin_fixture.tmp_path")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "STRUCTURAL"
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_builtin_fixture_context"
                })
        }));
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("PytestFixtureInjection")
                && fact.origin_engine == "python"
                && fact.origin_method == "cpython_ast"
                && fact.certainty == "UNKNOWN"
        }));
        let has_project_config_summary = facts.facts.iter().any(|fact| {
            fact.path == "pyproject.toml"
                && fact.kind == "PROJECT_CONFIG"
                && fact.target.as_deref() == Some("python.project_config.project_name.demo-api")
                && fact.origin_engine == "python"
                && fact.origin_method == "tomllib"
                && fact.certainty == "STRUCTURAL"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "not_family_claim_input")
        });
        let has_project_config_missing_dependency_unknown = facts.facts.iter().any(|fact| {
            fact.path == "pyproject.toml"
                && fact.kind == "UNKNOWN"
                && fact.target.as_deref() == Some("MissingDependency")
                && fact.origin_engine == "python"
                && fact.origin_method == "tomllib"
                && fact.certainty == "UNKNOWN"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_project_config")
        });
        assert!(
            has_project_config_summary || has_project_config_missing_dependency_unknown,
            "pyproject.toml must persist either sanitized config facts or a typed provider UNKNOWN"
        );
        if has_project_config_summary {
            assert!(facts.facts.iter().any(|fact| {
                fact.path == "pyproject.toml"
                    && fact.kind == "PROJECT_CONFIG"
                    && fact.target.as_deref() == Some("python.project_config.source_root.src.lib")
                    && fact.certainty == "STRUCTURAL"
            }));
        }
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "FRAMEWORK_ROLE"
                && fact.certainty == "FRAMEWORK_HEURISTIC"
        }));
        let debug = format!("{:?}", facts.facts);
        for forbidden in [
            workspace.path().to_string_lossy().as_ref(),
            "from fastapi",
            "from acme.services",
            "@router.get",
            "response_model=",
            "Depends(get_db",
            "HTTPException(",
            "assert client.get",
            "return object",
            "missing_fixture",
            "../secret",
            "/tmp/secret",
            "C:/secret",
            "project_includes",
        ] {
            assert!(
                !debug.contains(forbidden),
                "leaked forbidden text {forbidden}"
            );
        }

        let readiness = assess_semantic_fact_readiness(
            SemanticFactReadinessRequest {
                repository_root: workspace.path().display().to_string(),
                max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            },
            &store,
            &FilesystemSourceStore,
        )
        .expect("assess Python fact readiness");
        assert_eq!(readiness.active_generation, "gen-000001");
        assert_eq!(readiness.facts.len(), facts.facts.len());
        let mut derived_targets = BTreeSet::new();
        let derived_fact_ids = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "repogrammar-python-derived"
                    && fact.origin_method == "bounded_ast_anchor_v1"
            })
            .map(|fact| {
                assert_eq!(fact.certainty, "DATAFLOW_DERIVED");
                let target = fact.target.as_deref().expect("derived target");
                assert!(
                    matches!(
                        target,
                        "fastapi.APIRouter.get" | "pydantic.BaseModel" | "pytest.fixture"
                    ),
                    "unexpected derived target {target}"
                );
                derived_targets.insert(target.to_string());
                assert!(fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false"));
                fact.fact_id.clone()
            })
            .collect::<BTreeSet<_>>();
        assert!(derived_targets.contains("fastapi.APIRouter.get"));
        assert!(derived_targets.contains("pydantic.BaseModel"));
        assert!(derived_targets.contains("pytest.fixture"));
        assert!(
            !derived_targets.contains("pytest.test"),
            "pytest fixture-binding UNKNOWN must block pytest test family support"
        );
        let graph_fact_ids = facts
            .facts
            .iter()
            .filter(|fact| {
                fact.origin_engine == "python"
                    && fact.origin_method == "cpython_ast"
                    && fact.certainty == "DATAFLOW_DERIVED"
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "derived_from=repo_local_python_import_graph"
                            || assumption == "derived_from=repo_local_pytest_fixture_graph"
                    })
            })
            .map(|fact| fact.fact_id.clone())
            .collect::<BTreeSet<_>>();
        for fact in readiness.facts {
            if derived_fact_ids.contains(&fact.fact_id) || graph_fact_ids.contains(&fact.fact_id) {
                assert!(matches!(fact.readiness, ClaimInputReadiness::EligibleInput));
            } else {
                let ClaimInputReadiness::Blocked { unknown } = fact.readiness else {
                    panic!("raw Python parser, framework, and config facts must stay blocked");
                };
                assert_eq!(unknown.reason, UnknownReasonCode::InsufficientSupport);
            }
        }

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        assert_eq!(families.status, 0);
        let unknown: Value = serde_json::from_str(families.stdout.trim()).expect("UNKNOWN JSON");
        assert_eq!(unknown["status"], "UNKNOWN");
        assert_eq!(unknown["unknowns"][0]["reason"], "InsufficientSupport");

        for command in ["find", "family", "member", "explain", "check"] {
            let output = run_with_runtime(
                cli_args(command, workspace.path(), &["src/acme/api.py", "--json"]),
                &runtime,
            );
            assert_eq!(output.status, 0);
            assert!(output.stderr.is_empty());
            let unknown: Value = serde_json::from_str(output.stdout.trim()).expect("query JSON");
            assert_unknown_query_json(command, &unknown);
            assert_no_claim_payload(command, &unknown);
        }

        let sync = run_with_runtime(cli_args("sync", workspace.path(), &["--json"]), &runtime);
        assert_eq!(sync.status, 0);
        assert!(sync.stderr.is_empty());
        let value: Value = serde_json::from_str(sync.stdout.trim()).expect("sync JSON");
        assert_eq!(value["generation_id"], "gen-000002");
        assert!(
            value["semantic_facts"].as_u64().unwrap_or_default() > 3,
            "sync should persist Python parse facts again"
        );

        let facts = list_semantic_facts(&store).expect("list synced semantic facts");
        assert_eq!(facts.active_generation, "gen-000002");
        assert!(facts.facts.iter().any(|fact| {
            fact.path == "src/acme/api.py"
                && fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("pytest.fixture.client")
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_pytest_fixture_graph")
        }));
    }

    #[test]
    fn product_runtime_persists_fastapi_request_shape_without_support_claims() {
        let workspace = TempWorkspace::new("product-runtime-fastapi-request-shape");
        fs::write(
            workspace.path().join("app.py"),
            r#"
from typing import Annotated

from fastapi import APIRouter, Body, Cookie, Depends, Header, HTTPException, Path, Query
from pydantic import BaseModel

router = APIRouter()

class UserIn(BaseModel):
    email: str

class UserOut(BaseModel):
    id: int

def get_db():
    return object()

@router.post("/users/{user_id}", response_model=UserOut)
def create_user(
    body: UserIn = Body(...),
    user_id: int = Path(...),
    query: str = Query(None),
    request_id: str = Header(...),
    session_id: str = Cookie(None),
    trace_id: Annotated[str, Header()] = "",
    db=Depends(get_db),
):
    if db is None:
        raise HTTPException(status_code=409)
    return UserOut(id=user_id)
"#,
        )
        .expect("write FastAPI source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        for (kind, target, anchor_kind) in [
            (
                "TYPE",
                "fastapi.request_body.UserIn",
                "fastapi_request_body_model",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.path.user_id",
                "fastapi_path_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.query.query",
                "fastapi_query_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.header.request_id",
                "fastapi_header_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.header.trace_id",
                "fastapi_header_param",
            ),
            (
                "SYMBOL",
                "fastapi.request_param.cookie.session_id",
                "fastapi_cookie_param",
            ),
        ] {
            assert_stored_python_structural_fact(&facts.facts, "app.py", kind, target, anchor_kind);
        }

        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "TYPE",
            "fastapi.response_model.UserOut",
            "fastapi_response_model",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "RESOLVED_CALL",
            "fastapi.Depends",
            "fastapi_dependency",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "SYMBOL",
            "fastapi.dependency.get_db",
            "fastapi_dependency_target",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "RESOLVED_CALL",
            "fastapi.HTTPException",
            "fastapi_http_exception",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "app.py",
            "SYMBOL",
            "fastapi.http_exception.status_code.409",
            "fastapi_http_exception_status",
        );
        let persisted_auxiliary_targets = [
            "fastapi.request_body.UserIn",
            "fastapi.request_param.path.user_id",
            "fastapi.request_param.query.query",
            "fastapi.request_param.header.request_id",
            "fastapi.request_param.header.trace_id",
            "fastapi.request_param.cookie.session_id",
            "fastapi.response_model.UserOut",
            "fastapi.Depends",
            "fastapi.dependency.get_db",
            "fastapi.HTTPException",
            "fastapi.http_exception.status_code.409",
        ];
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &persisted_auxiliary_targets,
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &[
                "fastapi.request_body.UserIn",
                "fastapi.request_param.path.user_id",
                "fastapi.request_param.query.query",
                "fastapi.request_param.header.request_id",
                "fastapi.request_param.header.trace_id",
                "fastapi.request_param.cookie.session_id",
                "fastapi.response_model.UserOut",
                "fastapi.dependency.get_db",
                "fastapi.Depends",
                "fastapi.HTTPException",
                "fastapi.http_exception.status_code.409",
            ],
        );

        for command in ["families", "find", "family", "explain", "check"] {
            let output = if command == "families" {
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
            } else {
                run_with_runtime(
                    cli_args(command, workspace.path(), &["app.py", "--json"]),
                    &runtime,
                )
            };
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
        }
    }

    #[test]
    fn product_runtime_python_framework_lookalikes_do_not_produce_families() {
        let workspace = TempWorkspace::new("product-runtime-python-framework-lookalikes");
        fs::write(
            workspace.path().join("lookalikes.py"),
            r#"
client = object()

@client.get("/users")
def not_a_fastapi_route():
    return {}

class BaseModel:
    pass

class UserOut(BaseModel):
    id: int

class Base:
    pass

class User(Base):
    __tablename__ = "users"
"#,
        )
        .expect("write Python lookalike source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        let files_json = parse_machine_output("files", &files, &workspace);
        assert!(files_json["files"]
            .as_array()
            .expect("files")
            .iter()
            .any(|file| file["path"] == "lookalikes.py"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let units_json = parse_machine_output("units", &units, &workspace);
        let unit_kinds = units_json["units"]
            .as_array()
            .expect("units")
            .iter()
            .filter(|unit| unit["path"] == "lookalikes.py")
            .filter_map(|unit| unit["kind"].as_str())
            .collect::<BTreeSet<_>>();
        for forbidden_kind in ["fastapi_route", "pydantic_model", "sqlalchemy_model"] {
            assert!(
                !unit_kinds.contains(forbidden_kind),
                "lookalike source must not produce {forbidden_kind}: {unit_kinds:?}"
            );
        }

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &[
                "fastapi.APIRouter.get",
                "fastapi.FastAPI.get",
                "pydantic.BaseModel",
                "sqlalchemy.orm.DeclarativeBase",
                "sqlalchemy.orm.Mapped",
                "sqlalchemy.orm.mapped_column",
            ],
        );

        for command in ["families", "find", "family", "member", "explain", "check"] {
            let output = if command == "families" {
                run_with_runtime(cli_args(command, workspace.path(), &["--json"]), &runtime)
            } else {
                run_with_runtime(
                    cli_args(command, workspace.path(), &["lookalikes.py", "--json"]),
                    &runtime,
                )
            };
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
            assert_no_claim_payload(command, &value);
        }
    }

    #[test]
    fn python_release_sqlalchemy_auxiliary_context_stays_metadata_only() {
        let workspace = TempWorkspace::new("python-release-sqlalchemy-auxiliary-context");
        copy_python_release_fixture("sqlalchemy-basic", workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");
        assert_eq!(index_json["generation_id"], "gen-000001");

        let status_request = RepositoryStatusRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let store = runtime
            .store_for_status_request(&status_request)
            .expect("open store");
        let facts = list_semantic_facts(&store).expect("list semantic facts");
        assert_eq!(facts.active_generation, "gen-000001");

        assert_stored_python_structural_fact(
            &facts.facts,
            "models.py",
            "RESOLVED_CALL",
            "sqlalchemy.orm.relationship",
            "sqlalchemy_relationship",
        );
        assert_stored_python_structural_fact(
            &facts.facts,
            "repository.py",
            "RESOLVED_CALL",
            "sqlalchemy.orm.Session.add",
            "sqlalchemy_session_call",
        );
        assert_no_derived_python_support_for_targets(
            &facts.facts,
            &["sqlalchemy.orm.relationship", "sqlalchemy.orm.Session.add"],
        );
        assert_targets_blocked_from_claim_input(
            &workspace,
            &store,
            &facts.facts,
            &["sqlalchemy.orm.relationship", "sqlalchemy.orm.Session.add"],
        );

        let families = run_with_runtime(
            cli_args("families", workspace.path(), &["--json"]),
            &runtime,
        );
        let families_json = parse_machine_output("families", &families, &workspace);
        assert_eq!(families_json["status"], "ok");
        assert!(families_json["families"]
            .as_array()
            .expect("families")
            .iter()
            .any(|family| {
                family["family_id"] == "family:python:sqlalchemy_model:framework_sqlalchemy_model"
                    && family["support"] == 3
            }));

        for command in ["find", "family", "explain", "check"] {
            let output = run_with_runtime(
                cli_args(command, workspace.path(), &["repository.py", "--json"]),
                &runtime,
            );
            let value = parse_machine_output(command, &output, &workspace);
            assert_unknown_query_json(command, &value);
        }
    }

    #[test]
    fn product_runtime_inventory_reads_file_manifest_only_generation() {
        let workspace = TempWorkspace::new("product-runtime-ruby-inventory-index");
        fs::write(workspace.path().join("README.txt"), "not a TS/JS source\n")
            .expect("write ignored source");
        fs::write(workspace.path().join("main.rb"), [0xff, 0xfe, 0xfd])
            .expect("write binary Ruby source");
        fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://must-not-be-read.invalid'\n",
        )
        .expect("write Ruby config");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        assert!(index.stderr.is_empty());
        let value: Value = serde_json::from_str(index.stdout.trim()).expect("index JSON");
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["discovered_files"], 2);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(
            value["warnings"],
            serde_json::json!([
                "parser skipped unsupported language token: ruby",
                "parser skipped unsupported language token: ruby-config"
            ])
        );
        assert!(!index.stdout.contains("must-not-be-read"));

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        assert_eq!(status.status, 0);
        let value: Value = serde_json::from_str(status.stdout.trim()).expect("status JSON");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        assert_eq!(files.status, 0);
        assert!(files.stderr.is_empty());
        let value: Value = serde_json::from_str(files.stdout.trim()).expect("files JSON");
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(
            value["files"]
                .as_array()
                .expect("files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("file path"),
                        file["language"].as_str().expect("file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("Gemfile", "ruby-config"), ("main.rb", "ruby")]
        );
        assert!(!files.stdout.contains("must-not-be-read"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        assert_eq!(units.status, 0);
        assert!(units.stderr.is_empty());
        let value: Value = serde_json::from_str(units.stdout.trim()).expect("units JSON");
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["units"].as_array().expect("units array").len(), 0);
    }

    #[test]
    fn product_runtime_php_inventory_reads_file_manifest_only_generation() {
        let workspace = TempWorkspace::new("product-runtime-php-inventory-index");
        fs::write(workspace.path().join("README.txt"), "not a PHP source\n")
            .expect("write ignored source");
        let mut php_source = vec![0xff, 0xfe, 0xfd];
        php_source.extend_from_slice(b"php-source-must-not-be-read");
        fs::write(workspace.path().join("main.php"), php_source).expect("write binary PHP source");
        let mut composer_config = vec![0xff, 0xfe, 0xfd];
        composer_config.extend_from_slice(b"php-config-must-not-be-read");
        fs::write(workspace.path().join("composer.json"), composer_config)
            .expect("write binary Composer config");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("index", &index, &workspace);
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["discovered_files"], 2);
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);
        assert_eq!(
            value["warnings"],
            serde_json::json!([
                "parser skipped unsupported language token: php",
                "parser skipped unsupported language token: php-config"
            ])
        );
        assert!(!index.stdout.contains("php-source-must-not-be-read"));
        assert!(!index.stdout.contains("php-config-must-not-be-read"));

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("status", &status, &workspace);
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert!(!status.stdout.contains("php-source-must-not-be-read"));
        assert!(!status.stdout.contains("php-config-must-not-be-read"));

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("files", &files, &workspace);
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(
            value["files"]
                .as_array()
                .expect("files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("file path"),
                        file["language"].as_str().expect("file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("composer.json", "php-config"), ("main.php", "php")]
        );
        assert!(!files.stdout.contains("php-source-must-not-be-read"));
        assert!(!files.stdout.contains("php-config-must-not-be-read"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("units", &units, &workspace);
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["units"].as_array().expect("units array").len(), 0);
        assert!(!units.stdout.contains("php-source-must-not-be-read"));
        assert!(!units.stdout.contains("php-config-must-not-be-read"));

        let human = run_with_runtime(cli_args("resync", workspace.path(), &[]), &runtime);
        assert_eq!(human.status, 0);
        assert!(human.stderr.is_empty());
        assert_no_output_leakage("resync", &human.stdout, &workspace);
        assert!(human.stdout.contains("resync: file manifest stored"));
        assert!(human.stdout.contains("indexing: file_manifest_only"));
        assert!(human.stdout.contains("parser: deferred"));
        assert!(human.stdout.contains("parser_attempted_files: 0"));
        assert!(human
            .stdout
            .contains("warning: parser skipped unsupported language token: php\n"));
        assert!(human
            .stdout
            .contains("warning: parser skipped unsupported language token: php-config\n"));
        assert!(!human.stdout.contains("syntax-only code units stored"));
        assert!(!human.stdout.contains("php-source-must-not-be-read"));
        assert!(!human.stdout.contains("php-config-must-not-be-read"));
    }

    #[test]
    fn product_runtime_swift_inventory_reads_file_manifest_only_generation() {
        let workspace = TempWorkspace::new("product-runtime-swift-inventory-index");
        fs::write(workspace.path().join("README.txt"), "not a Swift source\n")
            .expect("write ignored source");
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
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("index", &index, &workspace);
        assert_eq!(value["generation_id"], "gen-000001");
        assert_eq!(value["discovered_files"], 2);
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["parser"], "deferred");
        assert_eq!(value["parser_attempted_files"], 0);
        assert_eq!(value["indexed_units"], 0);
        assert_eq!(value["semantic_facts"], 0);
        assert_eq!(
            value["warnings"],
            serde_json::json!([
                "parser skipped unsupported language token: swift",
                "parser skipped unsupported language token: swift-config"
            ])
        );
        assert!(!index.stdout.contains("swift-source-must-not-be-read"));
        assert!(!index.stdout.contains("swift-config-must-not-be-read"));

        let status = run_with_runtime(cli_args("status", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("status", &status, &workspace);
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert!(!status.stdout.contains("swift-source-must-not-be-read"));
        assert!(!status.stdout.contains("swift-config-must-not-be-read"));

        let files = run_with_runtime(cli_args("files", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("files", &files, &workspace);
        assert_eq!(value["command"], "files");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(
            value["files"]
                .as_array()
                .expect("files array")
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().expect("file path"),
                        file["language"].as_str().expect("file language"),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("Package@swift-6.3.swift", "swift-config"),
                ("main.swift", "swift")
            ]
        );
        assert!(!files.stdout.contains("swift-source-must-not-be-read"));
        assert!(!files.stdout.contains("swift-config-must-not-be-read"));

        let units = run_with_runtime(cli_args("units", workspace.path(), &["--json"]), &runtime);
        let value = parse_machine_output("units", &units, &workspace);
        assert_eq!(value["command"], "units");
        assert_eq!(value["active_generation"], "gen-000001");
        assert_eq!(value["indexing"], "file_manifest_only");
        assert_eq!(value["units"].as_array().expect("units array").len(), 0);
        assert!(!units.stdout.contains("swift-source-must-not-be-read"));
        assert!(!units.stdout.contains("swift-config-must-not-be-read"));

        let human = run_with_runtime(cli_args("resync", workspace.path(), &[]), &runtime);
        assert_eq!(human.status, 0);
        assert!(human.stderr.is_empty());
        assert_no_output_leakage("resync", &human.stdout, &workspace);
        assert!(human.stdout.contains("resync: file manifest stored"));
        assert!(human.stdout.contains("indexing: file_manifest_only"));
        assert!(human.stdout.contains("parser: deferred"));
        assert!(human.stdout.contains("parser_attempted_files: 0"));
        assert!(human
            .stdout
            .contains("warning: parser skipped unsupported language token: swift\n"));
        assert!(human
            .stdout
            .contains("warning: parser skipped unsupported language token: swift-config\n"));
        assert!(!human.stdout.contains("syntax-only code units stored"));
        assert!(!human.stdout.contains("swift-source-must-not-be-read"));
        assert!(!human.stdout.contains("swift-config-must-not-be-read"));
    }

    #[test]
    fn product_runtime_fresh_resync_reports_unknown_inventory_schema() {
        let workspace = TempWorkspace::new("product-runtime-unknown-inventory-schema");
        fs::write(
            workspace.path().join("app.py"),
            "def handler():\n    return {'ok': True}\n",
        )
        .expect("write source");
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");

        let resync = run_with_runtime(
            cli_args(
                "resync",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let resync_json = parse_machine_output("resync", &resync, &workspace);
        assert_eq!(resync_json["command"], "resync");
        assert_eq!(resync_json["status"], "complete");

        let unknowns = run_with_runtime(
            cli_args("unknowns", workspace.path(), &["--json"]),
            &runtime,
        );
        let unknowns_json = parse_machine_output("unknowns", &unknowns, &workspace);
        assert_eq!(unknowns_json["command"], "unknowns");
        assert_eq!(unknowns_json["status"], "ok");
        let inventory = &unknowns_json["unknown_inventory"];
        assert_eq!(inventory["inventory_scope"], "persisted_semantic_unknowns");
        assert!(inventory["by_role_state"].is_array());
        assert!(inventory["by_recovery_code"].is_array());
        assert!(inventory.get("by_recovery").is_none());
        assert!(!unknowns.stdout.contains("def handler"));
        assert!(!unknowns.stdout.contains("return {'ok': True}"));

        let stats = run_with_runtime(
            cli_args("stats", workspace.path(), &["--unknowns", "--json"]),
            &runtime,
        );
        let stats_json = parse_machine_output("stats", &stats, &workspace);
        assert_eq!(stats_json["command"], "stats");
        assert_eq!(stats_json["status"], "ok");
        assert_eq!(
            stats_json["repo_shape_scope"],
            "python_family_eligible_units"
        );
        assert_eq!(
            stats_json["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );
        assert!(!stats.stdout.contains("def handler"));
        assert!(!stats.stdout.contains("return {'ok': True}"));
    }

    #[test]
    fn product_runtime_unknown_inventory_drops_for_resolved_python_imports_and_fixtures() {
        let resolved = TempWorkspace::new("product-runtime-python-unknowns-resolved");
        fs::create_dir_all(resolved.path().join("src/acme")).expect("create resolved package");
        fs::write(resolved.path().join("src/acme/__init__.py"), "").expect("write init");
        fs::write(
            resolved.path().join("src/acme/util.py"),
            "def make_client():\n    return object()\n",
        )
        .expect("write util");
        fs::write(
            resolved.path().join("src/acme/test_app.py"),
            r#"
from acme.util import make_client
import pytest

@pytest.fixture
def client():
    return make_client()

def test_ok(client):
    assert client is not None
"#,
        )
        .expect("write resolved test");
        fs::write(
            resolved.path().join("pyproject.toml"),
            r#"
[tool.pytest.ini_options]
pythonpath = ["src"]
"#,
        )
        .expect("write pyproject");

        let unresolved = TempWorkspace::new("product-runtime-python-unknowns-unresolved");
        fs::create_dir_all(unresolved.path().join("src/acme")).expect("create unresolved package");
        fs::write(unresolved.path().join("src/acme/__init__.py"), "").expect("write init");
        fs::write(
            unresolved.path().join("src/acme/test_app.py"),
            r#"
from acme.missing import make_client

def test_ok(client):
    assert client is not None
"#,
        )
        .expect("write unresolved test");
        fs::write(
            unresolved.path().join("pyproject.toml"),
            r#"
[tool.pytest.ini_options]
pythonpath = ["src"]
"#,
        )
        .expect("write pyproject");

        let runtime = ProductCliRuntime;
        for workspace in [&resolved, &unresolved] {
            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, workspace);
            assert_eq!(init_json["status"], "initialized");

            let resync = run_with_runtime(
                cli_args(
                    "resync",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let resync_json = parse_machine_output("resync", &resync, workspace);
            assert_eq!(resync_json["command"], "resync");
            assert_eq!(resync_json["status"], "complete");
        }

        let resolved_unknowns =
            run_with_runtime(cli_args("unknowns", resolved.path(), &["--json"]), &runtime);
        let resolved_json = parse_machine_output("unknowns", &resolved_unknowns, &resolved);
        assert_eq!(resolved_json["command"], "unknowns");
        assert_eq!(resolved_json["status"], "ok");
        assert_eq!(
            resolved_json["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );

        let unresolved_unknowns = run_with_runtime(
            cli_args("unknowns", unresolved.path(), &["--json"]),
            &runtime,
        );
        let unresolved_json = parse_machine_output("unknowns", &unresolved_unknowns, &unresolved);
        assert_eq!(unresolved_json["command"], "unknowns");
        assert_eq!(unresolved_json["status"], "ok");
        assert_eq!(
            unresolved_json["unknown_inventory"]["inventory_scope"],
            "persisted_semantic_unknowns"
        );

        let unresolved_import_unknowns =
            required_mechanism_count(&unresolved_json, "python_import_graph");
        let unresolved_fixture_unknowns =
            required_mechanism_count(&unresolved_json, "pytest_fixture_graph");
        assert!(
            unresolved_import_unknowns >= 1,
            "unresolved repo must expose import graph UNKNOWN inventory: {unresolved_json}"
        );
        assert!(
            unresolved_fixture_unknowns >= 1,
            "unresolved repo must expose fixture graph UNKNOWN inventory: {unresolved_json}"
        );
        assert!(
            required_mechanism_count(&resolved_json, "python_import_graph")
                < unresolved_import_unknowns,
            "resolved import graph should reduce python_import_graph UNKNOWN inventory"
        );
        assert!(
            required_mechanism_count(&resolved_json, "pytest_fixture_graph")
                < unresolved_fixture_unknowns,
            "resolved fixture graph should reduce pytest_fixture_graph UNKNOWN inventory"
        );
        for forbidden in ["from acme", "make_client", "assert client", "acme.missing"] {
            assert!(
                !resolved_unknowns.stdout.contains(forbidden),
                "resolved unknowns leaked source text {forbidden}"
            );
            assert!(
                !unresolved_unknowns.stdout.contains(forbidden),
                "unresolved unknowns leaked source text {forbidden}"
            );
        }
    }

    #[test]
    fn product_runtime_unknown_reduction_positive_baselines_have_replacement_evidence() {
        let runtime = ProductCliRuntime;

        let python_unresolved =
            run_unknown_reduction_fixture(&runtime, "python_fastapi_unresolved");
        let python_resolved = run_unknown_reduction_fixture(&runtime, "python_fastapi_resolved");
        assert_no_false_family("python_fastapi_unresolved", &python_unresolved);
        assert_unknown_reduction(
            "python_fastapi",
            &python_unresolved,
            &python_resolved,
            "fastapi_dependency_graph",
        );
        assert_source_backed_fact(&python_resolved.facts, "python_fastapi_resolved", |fact| {
            fact.origin_engine == "repogrammar-python-derived"
                && fact.origin_method == "bounded_ast_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("fastapi.APIRouter.get")
        });
        assert_no_unknown_output_fragments(&python_unresolved, &["make_dependency", "Depends("]);
        assert_no_unknown_output_fragments(&python_resolved, &["UserOut", "current_user"]);

        let django_unresolved = run_unknown_reduction_fixture(&runtime, "python_django_unresolved");
        let django_resolved = run_unknown_reduction_fixture(&runtime, "python_django_resolved");
        assert_no_false_family("python_django_unresolved", &django_unresolved);
        assert_unknown_reduction(
            "python_django",
            &django_unresolved,
            &django_resolved,
            "django_project_model",
        );
        assert_source_backed_fact(&django_resolved.facts, "python_django_resolved", |fact| {
            fact.origin_engine == "repogrammar-python-derived"
                && fact.origin_method == "bounded_ast_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("django.db.models.Model")
        });
        assert_no_unknown_output_fragments(&django_unresolved, &["myapp", "CharField"]);
        assert_no_unknown_output_fragments(&django_resolved, &["CharField", "TextField"]);

        let express_unresolved = run_unknown_reduction_fixture(&runtime, "tsjs_express_unresolved");
        let express_resolved = run_unknown_reduction_fixture(&runtime, "tsjs_express_resolved");
        assert_no_false_family("tsjs_express_unresolved", &express_unresolved);
        assert_unknown_reduction(
            "tsjs_express",
            &express_unresolved,
            &express_resolved,
            "typescript_paths_resolver",
        );
        assert_source_backed_fact(&express_resolved.facts, "tsjs_express_resolved", |fact| {
            fact.origin_engine == "repogrammar-tsjs-derived"
                && fact.origin_method == "bounded_exact_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("express.route.get")
        });
        assert_no_unknown_output_fragments(&express_unresolved, &["createApp", "listUsers"]);
        assert_no_unknown_output_fragments(&express_resolved, &["express()", "listUsers"]);

        let prisma_unresolved = run_unknown_reduction_fixture(&runtime, "tsjs_prisma_unresolved");
        let prisma_resolved = run_unknown_reduction_fixture(&runtime, "tsjs_prisma_resolved");
        assert_no_false_family("tsjs_prisma_unresolved", &prisma_unresolved);
        assert_unknown_reduction(
            "tsjs_prisma",
            &prisma_unresolved,
            &prisma_resolved,
            "prisma_client_model",
        );
        assert_source_backed_fact(&prisma_resolved.facts, "tsjs_prisma_resolved", |fact| {
            fact.origin_engine == "repogrammar-tsjs-derived"
                && fact.origin_method == "bounded_exact_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("prisma.query.findMany")
        });
        assert_no_unknown_output_fragments(&prisma_unresolved, &["getPrismaClient", "findMany("]);
        assert_no_unknown_output_fragments(&prisma_resolved, &["PrismaClient", "findMany("]);

        let nest_unresolved = run_unknown_reduction_fixture(&runtime, "tsjs_nest_unresolved");
        let nest_resolved = run_unknown_reduction_fixture(&runtime, "tsjs_nest_resolved");
        assert_no_false_family("tsjs_nest_unresolved", &nest_unresolved);
        assert_unknown_reduction(
            "tsjs_nest",
            &nest_unresolved,
            &nest_resolved,
            "nestjs_di_model",
        );
        assert_source_backed_fact(&nest_resolved.facts, "tsjs_nest_resolved", |fact| {
            fact.origin_engine == "repogrammar-tsjs-derived"
                && fact.origin_method == "bounded_exact_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("nestjs.common.Get")
        });
        assert_no_unknown_output_fragments(&nest_unresolved, &["function Controller", "findAll"]);
        assert_no_unknown_output_fragments(&nest_resolved, &["@nestjs/common", "findActive"]);

        let rust_unresolved = run_unknown_reduction_fixture(&runtime, "rust_module_unresolved");
        let rust_resolved = run_unknown_reduction_fixture(&runtime, "rust_module_resolved");
        assert_no_false_family("rust_module_unresolved", &rust_unresolved);
        assert_unknown_reduction(
            "rust_module",
            &rust_unresolved,
            &rust_resolved,
            "rust_module_graph",
        );
        assert_source_backed_fact(&rust_resolved.facts, "rust_module_resolved", |fact| {
            fact.kind == "SYMBOL"
                && fact.target.as_deref() == Some("module:src/users.rs")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_module_resolution=external_mod")
        });
        assert_no_unknown_output_fragments(&rust_unresolved, &["mod users", "load()"]);
        assert_no_unknown_output_fragments(&rust_resolved, &["load_users", "mod users"]);

        let rust_serde_unresolved =
            run_unknown_reduction_fixture(&runtime, "rust_serde_unresolved");
        let rust_serde_resolved = run_unknown_reduction_fixture(&runtime, "rust_serde_resolved");
        assert_no_false_family("rust_serde_unresolved", &rust_serde_unresolved);
        assert_unknown_reduction(
            "rust_serde",
            &rust_serde_unresolved,
            &rust_serde_resolved,
            "rust_module_graph",
        );
        assert_source_backed_fact(&rust_serde_resolved.facts, "rust_serde_resolved", |fact| {
            fact.origin_engine == "repogrammar-rust-derived"
                && fact.origin_method == "bounded_tree_sitter_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("serde.Serialize")
        });
        assert_no_unknown_output_fragments(&rust_serde_unresolved, &["struct Alpha", "derive("]);
        assert_no_unknown_output_fragments(&rust_serde_resolved, &["struct Alpha", "use serde"]);

        let csharp_unresolved = run_unknown_reduction_fixture(&runtime, "csharp_aspnet_unresolved");
        let csharp_resolved = run_unknown_reduction_fixture(&runtime, "csharp_aspnet_resolved");
        assert_no_false_family("csharp_aspnet_unresolved", &csharp_unresolved);
        assert_unknown_reduction(
            "csharp_aspnet",
            &csharp_unresolved,
            &csharp_resolved,
            "csharp_project_model",
        );
        assert_source_backed_fact(&csharp_resolved.facts, "csharp_aspnet_resolved", |fact| {
            fact.origin_engine == "repogrammar-csharp-derived"
                && fact.origin_method == "bounded_tree_sitter_csharp_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("aspnetcore.mvc.HttpGet")
        });
        assert_no_unknown_output_fragments(&csharp_unresolved, &["[HttpGet", "return \"items\""]);
        assert_no_unknown_output_fragments(&csharp_resolved, &["return Ok(", "IActionResult"]);

        let cpp_unresolved = run_unknown_reduction_fixture(&runtime, "cpp_gtest_unresolved");
        let cpp_resolved = run_unknown_reduction_fixture(&runtime, "cpp_gtest_resolved");
        assert_no_false_family("cpp_gtest_unresolved", &cpp_unresolved);
        assert_unknown_reduction(
            "cpp_gtest",
            &cpp_unresolved,
            &cpp_resolved,
            "cpp_test_framework_model",
        );
        assert_source_backed_fact(&cpp_resolved.facts, "cpp_gtest_resolved", |fact| {
            fact.origin_engine == "repogrammar-cpp-derived"
                && fact.origin_method == "bounded_tree_sitter_c_cpp_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("gtest.TEST")
        });
        assert_no_unknown_output_fragments(&cpp_unresolved, &["TEST(", "#ifdef ENABLE"]);
        assert_no_unknown_output_fragments(&cpp_resolved, &["EXPECT_TRUE", "EXPECT_EQ"]);
        let java_unresolved = run_unknown_reduction_fixture(&runtime, "java_junit_unresolved");
        let java_resolved = run_unknown_reduction_fixture(&runtime, "java_junit_resolved");
        assert_no_false_family("java_junit_unresolved", &java_unresolved);
        assert_unknown_reduction(
            "java_junit",
            &java_unresolved,
            &java_resolved,
            "java_test_annotation_model",
        );
        assert_source_backed_fact(&java_resolved.facts, "java_junit_resolved", |fact| {
            fact.origin_engine == "repogrammar-java-derived"
                && fact.origin_method == "bounded_tree_sitter_java_anchor_v1"
                && fact.certainty == "DATAFLOW_DERIVED"
                && fact.target.as_deref() == Some("junit.jupiter.Test")
        });
        assert_no_unknown_output_fragments(&java_unresolved, &["no exact import", "void alpha"]);
        assert_no_unknown_output_fragments(&java_resolved, &["support facts", "void alpha"]);

        let java_test_data_unresolved =
            run_unknown_reduction_fixture(&runtime, "java_test_data_unresolved");
        let java_test_data_resolved =
            run_unknown_reduction_fixture(&runtime, "java_test_data_resolved");
        assert_no_false_family("java_test_data_unresolved", &java_test_data_unresolved);
        assert_unknown_reduction(
            "java_test_data",
            &java_test_data_unresolved,
            &java_test_data_resolved,
            "java_test_annotation_model",
        );
        for target in [
            "junit.jupiter.MethodSource.local_factory",
            "testng.annotations.DataProvider.local_method",
        ] {
            assert_source_backed_fact(
                &java_test_data_resolved.facts,
                "java_test_data_resolved",
                |fact| {
                    fact.origin_engine == "repogrammar-java-syntax"
                        && fact.origin_method == "tree_sitter_java_structural_anchors_v1"
                        && fact.certainty == "STRUCTURAL"
                        && fact.target.as_deref() == Some(target)
                },
            );
        }
        assert_no_unknown_output_fragments(
            &java_test_data_unresolved,
            &["ExternalRows", "dataProviderClass", "MethodSource"],
        );
        assert_no_unknown_output_fragments(
            &java_test_data_resolved,
            &["static int[] values", "Object[][] rows", "DataProvider"],
        );
    }

    #[test]
    fn product_runtime_unknown_regression_benchmark_tracks_mechanisms_without_false_certainty() {
        #[derive(Clone, Copy)]
        enum FixtureKind {
            Python,
            PythonV02,
            TsJsV02,
            RustV02,
            CSharpV02,
            CppV02,
            JavaV02,
        }

        struct BucketExpectation {
            key: &'static str,
            count: u64,
        }

        struct BenchmarkCase {
            name: &'static str,
            fixture_kind: FixtureKind,
            fixture_name: &'static str,
            total_unknowns: u64,
            blocking_unknowns: u64,
            non_blocking_unknowns: u64,
            recoverable_unknowns: u64,
            irreducible_unknowns: u64,
            languages: &'static [BucketExpectation],
            reasons: &'static [BucketExpectation],
            mechanisms: &'static [BucketExpectation],
            forbidden_output_fragments: &'static [&'static str],
        }

        const PYTHON_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "python",
            count: 16,
        }];
        const PYTHON_REASONS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "ConflictingFacts",
                count: 1,
            },
            BucketExpectation {
                key: "DynamicImport",
                count: 2,
            },
            BucketExpectation {
                key: "FrameworkMagic",
                count: 9,
            },
            BucketExpectation {
                key: "MonkeyPatch",
                count: 1,
            },
            BucketExpectation {
                key: "PytestFixtureInjection",
                count: 1,
            },
            BucketExpectation {
                key: "RuntimeDependencyInjection",
                count: 2,
            },
        ];
        const PYTHON_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "fastapi_dependency_graph",
                count: 1,
            },
            BucketExpectation {
                key: "framework_semantic_provider",
                count: 9,
            },
            BucketExpectation {
                key: "pytest_fixture_graph",
                count: 2,
            },
            BucketExpectation {
                key: "python_import_graph",
                count: 3,
            },
            BucketExpectation {
                key: "runtime_trace_required",
                count: 1,
            },
        ];

        const PYTHON_V02_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "python",
            count: 3,
        }];
        const PYTHON_V02_REASONS: &[BucketExpectation] = &[BucketExpectation {
            key: "FrameworkMagic",
            count: 3,
        }];
        const PYTHON_V02_MECHANISMS: &[BucketExpectation] = &[BucketExpectation {
            key: "framework_semantic_provider",
            count: 3,
        }];

        const TSJS_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "typescript",
            count: 14,
        }];
        const TSJS_REASONS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "ConflictingFacts",
                count: 3,
            },
            BucketExpectation {
                key: "FrameworkMagic",
                count: 7,
            },
            BucketExpectation {
                key: "UnresolvedImport",
                count: 4,
            },
        ];
        const TSJS_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "conflict_resolution",
                count: 2,
            },
            BucketExpectation {
                key: "drizzle_db_model",
                count: 3,
            },
            BucketExpectation {
                key: "fastify_receiver_model",
                count: 2,
            },
            BucketExpectation {
                key: "prisma_client_model",
                count: 6,
            },
            BucketExpectation {
                key: "typescript_module_resolver",
                count: 1,
            },
        ];

        const RUST_LANGUAGES: &[BucketExpectation] = &[
            BucketExpectation {
                key: "rust",
                count: 9,
            },
            BucketExpectation {
                key: "rust-config",
                count: 1,
            },
        ];
        const RUST_REASONS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "BuildVariantAmbiguity",
                count: 3,
            },
            BucketExpectation {
                key: "MacroOrPreprocessor",
                count: 6,
            },
            BucketExpectation {
                key: "MissingProjectConfig",
                count: 1,
            },
        ];
        const RUST_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "cargo_feature_cfg_model",
                count: 3,
            },
            BucketExpectation {
                key: "project_config_reader",
                count: 1,
            },
            BucketExpectation {
                key: "rust_macro_boundary",
                count: 6,
            },
        ];
        const RUST_LOOKALIKE_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "rust",
            count: 3,
        }];
        const RUST_LOOKALIKE_REASONS: &[BucketExpectation] = &[BucketExpectation {
            key: "UnresolvedImport",
            count: 3,
        }];
        const RUST_LOOKALIKE_MECHANISMS: &[BucketExpectation] = &[BucketExpectation {
            key: "rust_module_graph",
            count: 3,
        }];
        const CSHARP_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "csharp",
            count: 6,
        }];
        const CSHARP_REASONS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "BuildVariantAmbiguity",
                count: 4,
            },
            BucketExpectation {
                key: "FrameworkMagic",
                count: 1,
            },
            BucketExpectation {
                key: "RuntimeDependencyInjection",
                count: 1,
            },
        ];
        const CSHARP_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "csharp_build_variant_model",
                count: 4,
            },
            BucketExpectation {
                key: "csharp_di_model",
                count: 1,
            },
            BucketExpectation {
                key: "framework_semantic_provider",
                count: 1,
            },
        ];
        const CPP_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "cpp",
            count: 3,
        }];
        const CPP_REASONS: &[BucketExpectation] = &[BucketExpectation {
            key: "BuildVariantAmbiguity",
            count: 3,
        }];
        const CPP_MECHANISMS: &[BucketExpectation] = &[BucketExpectation {
            key: "cpp_build_variant_model",
            count: 3,
        }];

        const TSJS_NEST_LOOKALIKE_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "typescript",
            count: 5,
        }];
        const TSJS_NEST_LOOKALIKE_REASONS: &[BucketExpectation] = &[BucketExpectation {
            key: "UnresolvedImport",
            count: 5,
        }];
        const TSJS_NEST_LOOKALIKE_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "nestjs_di_model",
                count: 4,
            },
            BucketExpectation {
                key: "typescript_paths_resolver",
                count: 1,
            },
        ];

        const JAVA_LANGUAGES: &[BucketExpectation] = &[BucketExpectation {
            key: "java",
            count: 4,
        }];
        const JAVA_REASONS: &[BucketExpectation] = &[BucketExpectation {
            key: "UnresolvedImport",
            count: 4,
        }];
        const JAVA_MECHANISMS: &[BucketExpectation] = &[
            BucketExpectation {
                key: "java_test_annotation_model",
                count: 3,
            },
            BucketExpectation {
                key: "jpa_entity_model",
                count: 1,
            },
        ];

        const CASES: &[BenchmarkCase] = &[
            BenchmarkCase {
                name: "python_dynamic_unknowns",
                fixture_kind: FixtureKind::Python,
                fixture_name: "dynamic-unknown",
                total_unknowns: 16,
                blocking_unknowns: 2,
                non_blocking_unknowns: 1,
                recoverable_unknowns: 12,
                irreducible_unknowns: 1,
                languages: PYTHON_LANGUAGES,
                reasons: PYTHON_REASONS,
                mechanisms: PYTHON_MECHANISMS,
                forbidden_output_fragments: &[
                    "importlib.import_module",
                    "setattr(",
                    "request.getfixturevalue",
                ],
            },
            BenchmarkCase {
                name: "python_preview_framework_lookalike_unknowns",
                fixture_kind: FixtureKind::PythonV02,
                fixture_name: "framework_lookalikes",
                total_unknowns: 3,
                blocking_unknowns: 0,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 3,
                irreducible_unknowns: 0,
                languages: PYTHON_V02_LANGUAGES,
                reasons: PYTHON_V02_REASONS,
                mechanisms: PYTHON_V02_MECHANISMS,
                forbidden_output_fragments: &["models.Model", "@app.route", "Flask("],
            },
            BenchmarkCase {
                name: "tsjs_framework_negative_unknowns",
                fixture_kind: FixtureKind::TsJsV02,
                fixture_name: "framework_adapter_negative_cases",
                total_unknowns: 14,
                blocking_unknowns: 13,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 1,
                irreducible_unknowns: 0,
                languages: TSJS_LANGUAGES,
                reasons: TSJS_REASONS,
                mechanisms: TSJS_MECHANISMS,
                forbidden_output_fragments: &["app.route", "findMany(", "$queryRaw"],
            },
            BenchmarkCase {
                name: "rust_macro_cfg_unknowns",
                fixture_kind: FixtureKind::RustV02,
                fixture_name: "macro_cfg_unknowns",
                total_unknowns: 10,
                blocking_unknowns: 3,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 7,
                irreducible_unknowns: 0,
                languages: RUST_LANGUAGES,
                reasons: RUST_REASONS,
                mechanisms: RUST_MECHANISMS,
                forbidden_output_fragments: &["macro_rules!", "#[cfg", "build.rs"],
            },
            BenchmarkCase {
                name: "rust_derive_lookalike_unknowns",
                fixture_kind: FixtureKind::RustV02,
                fixture_name: "derive_lookalikes",
                total_unknowns: 3,
                blocking_unknowns: 0,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 3,
                irreducible_unknowns: 0,
                languages: RUST_LOOKALIKE_LANGUAGES,
                reasons: RUST_LOOKALIKE_REASONS,
                mechanisms: RUST_LOOKALIKE_MECHANISMS,
                forbidden_output_fragments: &[
                    "derive(Serialize",
                    "LookalikeItem",
                    "struct Lookalike",
                ],
            },
            BenchmarkCase {
                name: "csharp_preprocessor_variant_unknowns",
                fixture_kind: FixtureKind::CSharpV02,
                fixture_name: "preprocessor_variant_unknown",
                total_unknowns: 6,
                blocking_unknowns: 4,
                non_blocking_unknowns: 2,
                recoverable_unknowns: 0,
                irreducible_unknowns: 0,
                languages: CSHARP_LANGUAGES,
                reasons: CSHARP_REASONS,
                mechanisms: CSHARP_MECHANISMS,
                forbidden_output_fragments: &["#if DEBUG", "return Ok(", "IActionResult"],
            },
            BenchmarkCase {
                name: "cpp_preprocessor_variant_unknowns",
                fixture_kind: FixtureKind::CppV02,
                fixture_name: "preprocessor_variant_unknown",
                total_unknowns: 3,
                blocking_unknowns: 3,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 0,
                irreducible_unknowns: 0,
                languages: CPP_LANGUAGES,
                reasons: CPP_REASONS,
                mechanisms: CPP_MECHANISMS,
                forbidden_output_fragments: &["#ifdef ENABLE", "EXPECT_TRUE", "TEST("],
            },
            BenchmarkCase {
                name: "tsjs_new_framework_lookalike_unknowns",
                fixture_kind: FixtureKind::TsJsV02,
                fixture_name: "tsjs_new_framework_lookalikes",
                total_unknowns: 5,
                blocking_unknowns: 5,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 0,
                irreducible_unknowns: 0,
                languages: TSJS_NEST_LOOKALIKE_LANGUAGES,
                reasons: TSJS_NEST_LOOKALIKE_REASONS,
                mechanisms: TSJS_NEST_LOOKALIKE_MECHANISMS,
                forbidden_output_fragments: &["z.object", "makeRouter", "findAll"],
            },
            BenchmarkCase {
                name: "java_framework_lookalike_unknowns",
                fixture_kind: FixtureKind::JavaV02,
                fixture_name: "test_annotation_lookalikes",
                total_unknowns: 4,
                blocking_unknowns: 0,
                non_blocking_unknowns: 0,
                recoverable_unknowns: 4,
                irreducible_unknowns: 0,
                languages: JAVA_LANGUAGES,
                reasons: JAVA_REASONS,
                mechanisms: JAVA_MECHANISMS,
                forbidden_output_fragments: &["No JUnit", "void one", "class LooseEntity"],
            },
        ];

        let runtime = ProductCliRuntime;
        for case in CASES {
            let workspace = TempWorkspace::new(&format!("unknown-benchmark-{}", case.name));
            match case.fixture_kind {
                FixtureKind::Python => {
                    copy_python_release_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::PythonV02 => {
                    copy_python_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::TsJsV02 => {
                    copy_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::RustV02 => {
                    copy_rust_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::CSharpV02 => {
                    copy_csharp_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::CppV02 => {
                    copy_cpp_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
                FixtureKind::JavaV02 => {
                    copy_java_release_v0_2_fixture(case.fixture_name, workspace.path())
                }
            }

            let init = run_with_runtime(
                cli_args("init", workspace.path(), &["--state-only", "--json"]),
                &runtime,
            );
            let init_json = parse_machine_output("init", &init, &workspace);
            assert_eq!(init_json["status"], "initialized");

            let resync = run_with_runtime(
                cli_args(
                    "resync",
                    workspace.path(),
                    &["--json", "--progress", "never"],
                ),
                &runtime,
            );
            let resync_json = parse_machine_output("resync", &resync, &workspace);
            assert_eq!(resync_json["command"], "resync");
            assert_eq!(resync_json["status"], "complete");

            let unknowns = run_with_runtime(
                cli_args("unknowns", workspace.path(), &["--json"]),
                &runtime,
            );
            let unknowns_json = parse_machine_output("unknowns", &unknowns, &workspace);
            assert_eq!(unknowns_json["command"], "unknowns");
            assert_eq!(unknowns_json["status"], "ok");
            let inventory = &unknowns_json["unknown_inventory"];
            assert_eq!(inventory["inventory_scope"], "persisted_semantic_unknowns");
            assert_eq!(inventory["total_unknowns"], case.total_unknowns);
            assert_eq!(inventory["blocking_unknowns"], case.blocking_unknowns);
            assert_eq!(
                inventory["non_blocking_unknowns"],
                case.non_blocking_unknowns
            );
            assert_eq!(inventory["recoverable_unknowns"], case.recoverable_unknowns);
            assert_eq!(inventory["irreducible_unknowns"], case.irreducible_unknowns);

            for expectation in case.languages {
                assert_eq!(
                    unknown_inventory_bucket_count(
                        &unknowns_json,
                        "by_language",
                        "language",
                        expectation.key
                    ),
                    expectation.count,
                    "{} language bucket {}",
                    case.name,
                    expectation.key
                );
            }
            for expectation in case.reasons {
                assert_eq!(
                    unknown_inventory_bucket_count(
                        &unknowns_json,
                        "by_reason_code",
                        "reason_code",
                        expectation.key
                    ),
                    expectation.count,
                    "{} reason bucket {}",
                    case.name,
                    expectation.key
                );
            }
            for expectation in case.mechanisms {
                assert_eq!(
                    required_mechanism_count(&unknowns_json, expectation.key),
                    expectation.count,
                    "{} mechanism bucket {}",
                    case.name,
                    expectation.key
                );
            }
            for forbidden in case.forbidden_output_fragments {
                assert!(
                    !unknowns.stdout.contains(forbidden),
                    "{} leaked source-like output fragment {forbidden}",
                    case.name
                );
            }

            let families = run_with_runtime(
                cli_args("families", workspace.path(), &["--json"]),
                &runtime,
            );
            let families_json = parse_machine_output("families", &families, &workspace);
            assert_eq!(families_json["status"], "UNKNOWN");
            assert!(families_json["families"]
                .as_array()
                .expect("families")
                .is_empty());
            assert_no_claim_payload("families", &families_json);
        }
    }

    #[test]
    fn product_runtime_missing_semantic_worker_falls_back_to_syntax_only() {
        let workspace = TempWorkspace::new("product-runtime-worker-missing");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let runtime = ProductCliRuntime;
        let missing_worker = workspace.path().join("missing-worker");
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
                    strict_gitignore: false,
                    semantic_worker_executable: Some(missing_worker.display().to_string()),
                    semantic_worker_args: Vec::new(),
                    progress: ProgressMode::Never,
                    json: false,
                    quiet: true,
                    stderr_is_terminal: false,
                },
            )
            .expect("missing worker should fall back to syntax-only indexing");

        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::FallbackUnavailable
        );
        assert_eq!(
            outcome.warnings,
            vec!["semantic worker fallback: unavailable".to_string()]
        );
        assert!(!outcome.warnings.iter().any(|warning| {
            warning.contains(workspace.path().to_string_lossy().as_ref())
                || warning.contains("missing-worker")
        }));
    }

    #[test]
    fn product_mcp_context_missing_state_returns_fallback_without_creating_state() {
        let workspace = TempWorkspace::new("product-mcp-missing-state");
        let runtime = ProductCliRuntime;
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };

        let response = handle_context_call(
            &runtime,
            &context,
            &serde_json::json!({
                "operation": "find_analogues",
                "target": "src/routes/a.ts",
            }),
        )
        .expect("fallback response");

        assert_eq!(response["status"], "FALLBACK_TO_CODE_SEARCH");
        assert_eq!(response["reason"], "repository is not initialized");
        assert!(!workspace.path().join(".repogrammar").exists());
    }

    #[test]
    fn product_mcp_serve_reads_active_query_without_source_leakage() {
        let workspace = TempWorkspace::new("product-mcp-serve");
        fs::write(
            workspace.path().join("component.tsx"),
            "export function UserCard() { return <section />; }\n",
        )
        .expect("write source");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);
        let index = run_with_runtime(cli_args("index", workspace.path(), &["--json"]), &runtime);
        assert_eq!(index.status, 0);
        let input = format!(
            "{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "repogrammar_context",
                    "arguments": {
                        "operation": "check_conformance",
                        "target": "component.tsx"
                    }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "shutdown"
            })
        );
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let mut output = Vec::new();

        serve_json_lines(&runtime, &context, input.as_bytes(), &mut output)
            .expect("serve MCP lines");
        let output = String::from_utf8(output).expect("utf8 MCP output");
        let first_line = output.lines().next().expect("tool response");
        let response: Value = serde_json::from_str(first_line).expect("JSON-RPC response");
        let payload_text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool payload");
        let payload: Value = serde_json::from_str(payload_text).expect("tool payload JSON");

        assert!(
            payload["status"] == "UNKNOWN" || payload["status"] == "PARTIAL_CONTEXT",
            "MCP path query should return UNKNOWN or source-free partial context: {payload}"
        );
        assert_eq!(payload["unknowns"][0]["reason"], "InsufficientSupport");
        assert!(!payload_text.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!payload_text.contains("export function"));
    }

    #[test]
    fn product_mcp_matches_python_exact_anchor_family_queries_without_source_leakage() {
        let case = *PYTHON_EXACT_ANCHOR_SMOKE_CASES
            .iter()
            .find(|case| case.fixture == "sqlalchemy-model-strong-evidence")
            .expect("SQLAlchemy model exact-anchor case");
        let workspace = TempWorkspace::new("product-mcp-python-exact-anchor");
        copy_python_release_fixture(case.fixture, workspace.path());
        let runtime = ProductCliRuntime;

        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only", "--json"]),
            &runtime,
        );
        let init_json = parse_machine_output("init", &init, &workspace);
        assert_eq!(init_json["status"], "initialized");
        let index = run_with_runtime(
            cli_args(
                "index",
                workspace.path(),
                &["--json", "--progress", "never"],
            ),
            &runtime,
        );
        let index_json = parse_machine_output("index", &index, &workspace);
        assert_eq!(index_json["status"], "complete");

        let find = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "find_analogues",
                "target": case.evidence_path,
                "mode": "evidence",
                "token_budget": 1,
            }),
        );
        assert_python_exact_anchor_evidence("find", &find, case, "evidence", Some(1));

        let show = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "show_family",
                "target": case.family_id,
                "mode": "deep",
            }),
        );
        assert_python_exact_anchor_evidence("family", &show, case, "deep", None);

        let explain = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "explain_deviation",
                "target": case.evidence_path,
            }),
        );
        assert_eq!(explain["status"], "ok");
        assert_python_exact_anchor_family_detail("explain", &explain, case);

        let check = mcp_context_payload(
            &runtime,
            &workspace,
            serde_json::json!({
                "operation": "check_conformance",
                "target": case.evidence_path,
            }),
        );
        assert_eq!(check["status"], "CONTEXT_ONLY");
        assert_eq!(check["check"]["advisory_status"], "UNKNOWN");
        assert_python_exact_anchor_family_detail("check", &check, case);

        let input = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": McpToolName::Context.as_str(),
                    "arguments": {
                        "operation": "find_analogues",
                        "target": case.evidence_path,
                        "mode": "evidence",
                        "token_budget": 1,
                    },
                },
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "shutdown",
            })
        );
        let context = McpServeContext {
            repository_root: workspace.path().display().to_string(),
            state_dir_override: None,
        };
        let mut output = Vec::new();
        serve_json_lines(&runtime, &context, input.as_bytes(), &mut output)
            .expect("serve MCP lines");
        let output = String::from_utf8(output).expect("utf8 MCP output");
        assert_no_output_leakage("mcp-jsonrpc", &output, &workspace);
        let lines = output.lines().collect::<Vec<_>>();
        assert!(
            lines.len() >= 2,
            "MCP should return list and tool responses"
        );
        let tools_response: Value = serde_json::from_str(lines[0]).expect("tools/list response");
        assert_eq!(
            tools_response["result"]["tools"][0]["name"],
            McpToolName::Context.as_str()
        );
        assert_eq!(
            tools_response["result"]["tools"]
                .as_array()
                .expect("tools")
                .len(),
            1
        );
        let tool_response: Value = serde_json::from_str(lines[1]).expect("tools/call response");
        assert_eq!(tool_response["result"]["isError"], false);
        assert_eq!(tool_response["result"]["content"][0]["type"], "text");
        let payload_text = tool_response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool payload text");
        assert_no_output_leakage("mcp-jsonrpc-payload", payload_text, &workspace);
        let payload: Value = serde_json::from_str(payload_text).expect("tool payload JSON");
        assert_python_exact_anchor_evidence("find", &payload, case, "evidence", Some(1));

        fs::write(
            workspace.path().join(case.evidence_path),
            "# stale replacement\n",
        )
        .expect("mutate exact-anchor evidence file");
        for (operation, target, command) in [
            ("find_analogues", case.evidence_path, "find"),
            ("show_family", case.family_id, "family"),
            ("explain_deviation", case.evidence_path, "explain"),
            ("check_conformance", case.evidence_path, "check"),
        ] {
            let stale = mcp_context_payload(
                &runtime,
                &workspace,
                serde_json::json!({
                    "operation": operation,
                    "target": target,
                }),
            );
            assert_python_stale_unknown(command, &stale, case.family_id);
        }

        let stale_input = format!(
            "{}\n{}\n",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "stale-family",
                "method": "tools/call",
                "params": {
                    "name": McpToolName::Context.as_str(),
                    "arguments": {
                        "operation": "show_family",
                        "target": case.family_id,
                    },
                },
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": "shutdown",
                "method": "shutdown",
            })
        );
        let mut stale_output = Vec::new();
        serve_json_lines(
            &runtime,
            &context,
            stale_input.as_bytes(),
            &mut stale_output,
        )
        .expect("serve stale MCP lines");
        let stale_output = String::from_utf8(stale_output).expect("utf8 stale MCP output");
        assert_no_output_leakage("mcp-jsonrpc-stale", &stale_output, &workspace);
        let stale_lines = stale_output.lines().collect::<Vec<_>>();
        assert_eq!(stale_lines.len(), 2);
        let stale_response: Value =
            serde_json::from_str(stale_lines[0]).expect("stale tools/call response");
        assert_eq!(stale_response["id"], "stale-family");
        assert_eq!(stale_response["result"]["isError"], false);
        let stale_payload_text = stale_response["result"]["content"][0]["text"]
            .as_str()
            .expect("stale tool payload text");
        assert_no_output_leakage("mcp-jsonrpc-stale-payload", stale_payload_text, &workspace);
        let stale_payload: Value =
            serde_json::from_str(stale_payload_text).expect("stale tool payload JSON");
        assert_python_stale_unknown("family", &stale_payload, case.family_id);
        let shutdown_response: Value =
            serde_json::from_str(stale_lines[1]).expect("shutdown response");
        assert_eq!(shutdown_response["id"], "shutdown");
        assert!(shutdown_response["result"].is_null());
    }

    #[test]
    fn native_agent_commands_use_public_mcp_cli_shapes() {
        let (codex_program, codex_args) =
            native_add_command(AgentTarget::Codex, InstallScope::Global, "/opt/repogrammar")
                .expect("codex add");
        #[cfg(windows)]
        assert_eq!(codex_program, "codex.cmd");
        #[cfg(not(windows))]
        assert_eq!(codex_program, "codex");
        assert_eq!(
            codex_args,
            vec![
                "mcp",
                "add",
                "repogrammar",
                "--",
                "/opt/repogrammar",
                "serve"
            ]
        );
        assert!(native_add_command(
            AgentTarget::Codex,
            InstallScope::ProjectLocal,
            "/opt/repogrammar"
        )
        .is_err());

        let (claude_program, claude_args) =
            native_add_command(AgentTarget::ClaudeCode, InstallScope::Global, "/opt/rg")
                .expect("claude add");
        assert_eq!(claude_program, "claude");
        assert_eq!(
            claude_args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "repogrammar",
                "--",
                "/opt/rg",
                "serve"
            ]
        );
        assert!(native_add_command(
            AgentTarget::ClaudeCode,
            InstallScope::ProjectLocal,
            "/opt/rg"
        )
        .is_err());

        let (codex_remove_program, codex_remove_args) =
            native_remove_command(AgentTarget::Codex, InstallScope::Global).expect("codex remove");
        assert_eq!(codex_remove_program, codex_program);
        assert_eq!(codex_remove_args, vec!["mcp", "remove", "repogrammar"]);

        let (claude_remove_program, remove_args) =
            native_remove_command(AgentTarget::ClaudeCode, InstallScope::Global)
                .expect("claude remove");
        assert_eq!(claude_remove_program, "claude");
        assert_eq!(
            remove_args,
            vec!["mcp", "remove", "--scope", "user", "repogrammar"]
        );

        let (codex_get_program, codex_get_args) =
            native_get_command(AgentTarget::Codex, InstallScope::Global).expect("codex get");
        assert_eq!(codex_get_program, codex_program);
        assert_eq!(codex_get_args, vec!["mcp", "get", "repogrammar", "--json"]);
        let (claude_get_program, claude_get_args) =
            native_get_command(AgentTarget::ClaudeCode, InstallScope::Global).expect("claude get");
        assert_eq!(claude_get_program, "claude");
        assert_eq!(claude_get_args, vec!["mcp", "get", "repogrammar"]);
    }

    #[test]
    fn native_probe_classifies_exact_absence_present_config_and_unknown_safely() {
        let codex_absent = classify_native_agent_probe(
            AgentTarget::Codex,
            InstallScope::Global,
            false,
            b"",
            b"Error: No MCP server named 'repogrammar' found.\n",
        )
        .expect("exact Codex absence");
        assert_eq!(codex_absent, NativeMcpServerState::NotFound);

        let claude_absent = classify_native_agent_probe(
            AgentTarget::ClaudeCode,
            InstallScope::Global,
            false,
            b"No MCP server named \"repogrammar\". Run `claude mcp add` to add one.\n",
            b"",
        )
        .expect("exact Claude absence");
        assert_eq!(claude_absent, NativeMcpServerState::NotFound);

        let codex_present = classify_native_agent_probe(
            AgentTarget::Codex,
            InstallScope::Global,
            true,
            br#"{"name":"repogrammar","enabled":true,"transport":{"type":"stdio","command":"/opt/repogrammar","args":["serve"]}}"#,
            b"",
        )
        .expect("valid Codex config");
        assert_eq!(
            codex_present,
            NativeMcpServerState::Present(NativeMcpServerConfig {
                executable_path: "/opt/repogrammar".to_string(),
                args: vec!["serve".to_string()],
                scope: InstallScope::Global,
                enabled: true,
            })
        );

        let claude_present = classify_native_agent_probe(
            AgentTarget::ClaudeCode,
            InstallScope::Global,
            true,
            b"repogrammar:\n  Scope: User config (available in all your projects)\n  Type: stdio\n  Command: /opt/repogrammar\n  Args: serve\n  Environment:\n",
            b"",
        )
        .expect("valid Claude config");
        assert_eq!(
            claude_present,
            NativeMcpServerState::Present(NativeMcpServerConfig {
                executable_path: "/opt/repogrammar".to_string(),
                args: vec!["serve".to_string()],
                scope: InstallScope::Global,
                enabled: true,
            })
        );

        let raw_secret = "/Users/alice/.codex/config.toml SECRET_TOKEN";
        let error = classify_native_agent_probe(
            AgentTarget::Codex,
            InstallScope::Global,
            false,
            b"",
            raw_secret.as_bytes(),
        )
        .expect_err("unexpected native output is unknown, not absent");
        let message = error.to_string();
        assert_eq!(message, "native codex MCP probe failed");
        assert!(!message.contains("/Users/alice"));
        assert!(!message.contains("SECRET_TOKEN"));

        let malformed = classify_native_agent_probe(
            AgentTarget::Codex,
            InstallScope::Global,
            true,
            raw_secret.as_bytes(),
            b"",
        )
        .expect("successful but unrecognized output is a preserved malformed state");
        assert_eq!(malformed, NativeMcpServerState::Malformed);
        let rendered = format!("{malformed:?}");
        assert!(!rendered.contains("/Users/alice"));
        assert!(!rendered.contains("SECRET_TOKEN"));
    }

    #[cfg(unix)]
    #[test]
    fn product_runtime_forwards_semantic_worker_args() {
        let workspace = TempWorkspace::new("product-runtime-worker-args");
        fs::write(workspace.path().join("a.ts"), "export const a = 1;\n").expect("write source");
        let worker_script = workspace.path().join("worker-fallback.sh");
        fs::write(
            &worker_script,
            r#"#!/bin/sh
/bin/cat >/dev/null
/bin/cat <<'EOF'
{"protocol_version":1,"message_type":"worker_error","request_id":"repogrammar-typescript-semantic-worker","error_code":"SEMANTIC_WORKER_UNAVAILABLE","message":"stub unavailable","fallback":{"mode":"syntax_only","certainty":"UNKNOWN"}}
{"protocol_version":1,"message_type":"end_of_stream","request_id":"repogrammar-typescript-semantic-worker"}
EOF
"#,
        )
        .expect("write worker script");
        let runtime = ProductCliRuntime;
        let init = run_with_runtime(
            cli_args("init", workspace.path(), &["--state-only"]),
            &runtime,
        );
        assert_eq!(init.status, 0);

        let outcome = runtime
            .index_repository(
                "index",
                CliIndexRequest {
                    repository_root: workspace.path().display().to_string(),
                    state_dir_override: None,
                    max_file_bytes: repogrammar::ports::file_discovery::DEFAULT_MAX_FILE_BYTES,
                    strict_gitignore: false,
                    semantic_worker_executable: Some("/bin/sh".to_string()),
                    semantic_worker_args: vec![worker_script.display().to_string()],
                    progress: ProgressMode::Never,
                    json: false,
                    quiet: true,
                    stderr_is_terminal: false,
                },
            )
            .expect("worker fallback should keep syntax-only indexing");

        assert_eq!(outcome.active_generation.as_deref(), Some("gen-000001"));
        assert_eq!(outcome.indexed_units, 1);
        assert_eq!(outcome.semantic_facts, 0);
        assert_eq!(
            outcome.semantic_worker,
            repogrammar::application::indexing::SemanticWorkerRunStatus::FallbackUnavailable
        );
        assert_eq!(
            outcome.warnings,
            vec!["semantic worker fallback: unavailable".to_string()]
        );
        assert!(!outcome
            .warnings
            .iter()
            .any(|warning| warning.contains(worker_script.to_string_lossy().as_ref())));
    }
}
