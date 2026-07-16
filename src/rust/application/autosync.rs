//! Repository-local auto-sync lifecycle state.

use crate::application::process_liveness::{
    autosync_daemon_process_liveness, process_liveness_for_lock, ProcessLiveness,
};
use crate::application::repository::{
    repository_state_location, repository_status, RepositoryImplementationStatus, RepositoryStatus,
    RepositoryStatusReport, RepositoryStatusRequest,
};
use crate::error::RepoGrammarError;
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

const AUTOSYNC_CONFIG_FILE: &str = "autosync.json";
const DAEMON_LOCK_FILE: &str = "daemon.lock";
const DAEMON_LIFECYCLE_FILE: &str = "daemon.lifecycle";
const AUTOSYNC_RUN_FILE: &str = "autosync-run.json";
const AUTOSYNC_STARTUP_FILE: &str = "autosync-startup.json";
const AUTOSYNC_SCHEMA_VERSION: u64 = 1;
const AUTOSYNC_RUN_ERROR_FINGERPRINT_FAILED: &str = "repository fingerprint failed";
const AUTOSYNC_RUN_ERROR_STATE_UNAVAILABLE: &str = "repository state is unavailable";
const AUTOSYNC_RUN_ERROR_SYNC_FAILED: &str = "repository sync failed";
const AUTOSYNC_RUN_ERROR_UNKNOWN: &str = "previous autosync attempt failed";
const AUTOSYNC_STOP_MAX_ATTEMPTS: usize = 40;
const AUTOSYNC_STOP_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
pub const AUTOSYNC_STARTUP_NONCE_ENV: &str = "REPOGRAMMAR_AUTOSYNC_STARTUP_NONCE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosyncRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
}

impl AutosyncRequest {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            state_dir_override: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutosyncSettings {
    pub poll_ms: u64,
    pub debounce_ms: u64,
}

impl Default for AutosyncSettings {
    fn default() -> Self {
        Self {
            poll_ms: 1000,
            debounce_ms: 750,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosyncReport {
    pub state_dir: String,
    pub enabled: bool,
    pub running: bool,
    pub daemon_state: AutosyncDaemonState,
    pub pid: Option<u32>,
    pub poll_ms: u64,
    pub debounce_ms: u64,
    pub last_run: Option<AutosyncRunReport>,
    pub startup: AutosyncStartupReport,
    pub repository_ready: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncDaemonState {
    Stopped,
    Starting,
    Running,
    Unknown,
}

impl AutosyncDaemonState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncDaemonPhase {
    Starting,
    Ready,
}

impl AutosyncDaemonPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncStartupState {
    Idle,
    Starting,
    Ready,
    Failed,
}

impl AutosyncStartupState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncStartupFailureCode {
    WorkerEnvironmentInvalid,
    RepositoryFingerprintFailed,
    RepositoryStateUnavailable,
    DaemonLockRefused,
    ChildExitedBeforeReady,
    StartupTimeout,
    FirstHeartbeatFailed,
}

impl AutosyncStartupFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkerEnvironmentInvalid => "worker_environment_invalid",
            Self::RepositoryFingerprintFailed => "repository_fingerprint_failed",
            Self::RepositoryStateUnavailable => "repository_state_unavailable",
            Self::DaemonLockRefused => "daemon_lock_refused",
            Self::ChildExitedBeforeReady => "child_exited_before_ready",
            Self::StartupTimeout => "startup_timeout",
            Self::FirstHeartbeatFailed => "first_heartbeat_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosyncStartupReport {
    pub state: AutosyncStartupState,
    pub failure_code: Option<AutosyncStartupFailureCode>,
    pub previous_failure_code: Option<AutosyncStartupFailureCode>,
}

/// Typed result of the parent's bounded daemon-startup probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutosyncStartupReadiness {
    Pending,
    Ready(AutosyncReport),
    Failed(AutosyncStartupFailureCode),
    LockRefused,
}

/// Repository conditions that require the daemon to stop instead of retrying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncRepositoryUnavailable {
    NotInitialized,
    CorruptedManifest,
    MissingStateSubdirectories,
    StorageUnhealthy,
}

/// Classify terminal daemon lifecycle conditions from typed repository status.
pub fn classify_autosync_repository_status(
    status: &RepositoryStatusReport,
) -> Option<AutosyncRepositoryUnavailable> {
    match status.status {
        RepositoryStatus::NotInitialized => {
            return Some(AutosyncRepositoryUnavailable::NotInitialized);
        }
        RepositoryStatus::CorruptedManifest => {
            return Some(AutosyncRepositoryUnavailable::CorruptedManifest);
        }
        RepositoryStatus::Initialized { .. } => {}
    }
    if !status.missing_subdirs.is_empty() {
        return Some(AutosyncRepositoryUnavailable::MissingStateSubdirectories);
    }
    (status.storage == RepositoryImplementationStatus::Unhealthy)
        .then_some(AutosyncRepositoryUnavailable::StorageUnhealthy)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutosyncRunResult {
    Ok,
    Error,
}

impl AutosyncRunResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AutosyncRunResult::Ok => "ok",
            AutosyncRunResult::Error => "error",
        }
    }
}

/// Last recorded auto-sync run, written by the daemon after each sync attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutosyncRunReport {
    pub last_sync_unix_seconds: u64,
    pub result: AutosyncRunResult,
    pub synced_generation: Option<String>,
    pub error: Option<String>,
}

impl AutosyncRunReport {
    pub(crate) fn display_synced_generation(&self) -> Option<&str> {
        self.synced_generation
            .as_deref()
            .filter(|generation| valid_autosync_generation_id(generation))
    }

    pub(crate) fn display_error(&self) -> Option<&str> {
        sanitized_autosync_run_error(self.result, self.error.as_deref())
    }
}

#[derive(Debug)]
pub struct AutosyncDaemonGuard {
    path: PathBuf,
    contents: String,
}

impl AutosyncDaemonGuard {
    /// Confirm that the exact lock acquired by this guard still exists in the
    /// `starting` phase. This is the ownership leg of the immediate service
    /// heartbeat that must succeed before readiness can be published.
    pub fn verify_starting_owner(&self) -> Result<(), RepoGrammarError> {
        let current = read_limited_text(&self.path)
            .map_err(|_| invalid_input("failed to verify auto-sync startup ownership"))?;
        if current != self.contents {
            return Err(invalid_input(
                "auto-sync daemon ownership changed before readiness",
            ));
        }
        let value: Value = serde_json::from_str(&current)
            .map_err(|_| invalid_input("failed to verify auto-sync startup ownership"))?;
        if value.get("phase").and_then(Value::as_str)
            != Some(AutosyncDaemonPhase::Starting.as_str())
        {
            return Err(invalid_input(
                "auto-sync daemon ownership changed before readiness",
            ));
        }
        Ok(())
    }

    /// Publish service readiness only while this guard still owns the exact
    /// `starting` record. The lifecycle marker serializes cooperating lock
    /// mutations, and the guard adopts the replacement bytes so `Drop`
    /// remains compare-and-remove safe.
    pub fn publish_ready(&mut self) -> Result<(), RepoGrammarError> {
        let locks_dir = self
            .path
            .parent()
            .ok_or_else(|| invalid_input("failed to publish auto-sync readiness"))?;
        let _lifecycle = acquire_daemon_lifecycle_guard(locks_dir)?;
        let current = read_limited_text(&self.path)
            .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;
        if current != self.contents {
            return Err(invalid_input(
                "auto-sync daemon ownership changed before readiness",
            ));
        }
        let mut value: Value = serde_json::from_str(&current)
            .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;
        if value.get("phase").and_then(Value::as_str)
            != Some(AutosyncDaemonPhase::Starting.as_str())
        {
            return Err(invalid_input("failed to publish auto-sync readiness"));
        }
        value["phase"] = Value::String(AutosyncDaemonPhase::Ready.as_str().to_string());
        let replacement = value.to_string();
        replace_owned_daemon_lock(&self.path, &current, &replacement)?;
        self.contents = replacement;
        Ok(())
    }
}

#[derive(Debug)]
struct AutosyncDaemonLifecycleGuard {
    path: PathBuf,
    contents: String,
}

impl Drop for AutosyncDaemonLifecycleGuard {
    fn drop(&mut self) {
        if read_limited_text(&self.path)
            .map(|current| current == self.contents)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Drop for AutosyncDaemonGuard {
    fn drop(&mut self) {
        if read_limited_text(&self.path)
            .map(|current| current == self.contents)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn enable_autosync(
    request: AutosyncRequest,
    settings: AutosyncSettings,
) -> Result<AutosyncReport, RepoGrammarError> {
    validate_autosync_settings(settings)?;
    let state = require_initialized_state(&request)?;
    write_config(
        &state.state_dir,
        &AutosyncConfig {
            enabled: true,
            settings,
        },
    )?;
    autosync_status_for_state(
        &state,
        "auto-sync is enabled; run repogrammar autosync start to run it",
    )
}

pub fn disable_autosync(request: AutosyncRequest) -> Result<AutosyncReport, RepoGrammarError> {
    let state = require_initialized_state(&request)?;
    let locks_dir = state.state_dir.join("locks");
    ensure_dir(&locks_dir, "failed to open auto-sync lock directory")?;
    let _lifecycle = acquire_daemon_lifecycle_guard(&locks_dir)?;
    let lock_path = locks_dir.join(DAEMON_LOCK_FILE);
    let lock = inspect_daemon_lock_path(&lock_path)?;
    if !daemon_owner_is_confirmed_dead(lock.status.owner_liveness) {
        return Err(invalid_input(
            "auto-sync ownership is active or cannot be verified; run repogrammar autosync stop before disable",
        ));
    }
    if let Some(stale_contents) = lock.contents {
        let _ = remove_daemon_lock_if_contents_match(&lock_path, &stale_contents)?;
    }
    let path = config_path(&state.state_dir);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid_input("failed to remove auto-sync config")),
    }
    autosync_status_for_state(&state, "auto-sync is disabled")
}

pub fn autosync_status(request: AutosyncRequest) -> Result<AutosyncReport, RepoGrammarError> {
    let state = require_initialized_state(&request)?;
    autosync_status_for_state(&state, "auto-sync status")
}

/// Inspect the daemon lock for one specific parent-spawned child.
///
/// A PID alone is insufficient because it can be reused. The startup nonce is
/// written by the child only after it has acquired the daemon lock, so the
/// matching tuple proves that this start attempt owns the repository daemon;
/// the parent separately verifies that its child handle is still alive before
/// accepting the `Ready` result.
pub fn inspect_autosync_startup(
    request: &AutosyncRequest,
    expected_pid: u32,
    expected_nonce: &str,
) -> Result<AutosyncStartupReadiness, RepoGrammarError> {
    let state = require_initialized_state(request)?;
    let lock_path = state.state_dir.join("locks").join(DAEMON_LOCK_FILE);
    let inspection = match inspect_daemon_lock_path(&lock_path) {
        Ok(inspection) => inspection,
        Err(_) => return Ok(AutosyncStartupReadiness::LockRefused),
    };
    let Some(_contents) = inspection.contents else {
        if let Some(code) = read_startup_failure(&state.state_dir, expected_nonce) {
            return Ok(AutosyncStartupReadiness::Failed(code));
        }
        return Ok(AutosyncStartupReadiness::Pending);
    };
    let owns_expected_lock = inspection.status.pid == Some(expected_pid)
        && inspection.status.startup_nonce.as_deref() == Some(expected_nonce);
    if owns_expected_lock && inspection.status.phase == Some(AutosyncDaemonPhase::Ready) {
        let config = read_config(&state.state_dir)?;
        return Ok(AutosyncStartupReadiness::Ready(AutosyncReport {
            state_dir: state.state_dir_relative,
            enabled: config.enabled,
            running: true,
            daemon_state: AutosyncDaemonState::Running,
            pid: Some(expected_pid),
            poll_ms: config.settings.poll_ms,
            debounce_ms: config.settings.debounce_ms,
            last_run: read_run_state(&state.state_dir),
            startup: AutosyncStartupReport {
                state: AutosyncStartupState::Ready,
                failure_code: None,
                previous_failure_code: read_startup_state(&state.state_dir).and_then(|startup| {
                    (startup.state == AutosyncStartupState::Failed
                        && startup.startup_nonce != expected_nonce)
                        .then_some(startup.failure_code)
                        .flatten()
                }),
            },
            repository_ready: true,
            message: "auto-sync started".to_string(),
        }));
    }
    if owns_expected_lock {
        if let Some(code) = read_startup_failure(&state.state_dir, expected_nonce) {
            return Ok(AutosyncStartupReadiness::Failed(code));
        }
        return Ok(AutosyncStartupReadiness::Pending);
    }
    if daemon_owner_is_confirmed_dead(inspection.status.owner_liveness) {
        return Ok(AutosyncStartupReadiness::Pending);
    }
    Ok(AutosyncStartupReadiness::LockRefused)
}

pub fn acquire_autosync_daemon(
    request: AutosyncRequest,
) -> Result<(AutosyncDaemonGuard, AutosyncSettings, PathBuf), RepoGrammarError> {
    let state = require_initialized_state(&request)?;
    let config = read_config(&state.state_dir)?;
    if !config.enabled {
        return Err(invalid_input(
            "auto-sync is disabled; run repogrammar autosync start",
        ));
    }
    let locks_dir = state.state_dir.join("locks");
    ensure_dir(&locks_dir, "failed to open auto-sync lock directory")?;
    let _lifecycle = acquire_daemon_lifecycle_guard(&locks_dir)?;
    let lock_path = locks_dir.join(DAEMON_LOCK_FILE);
    let startup_nonce = startup_nonce_from_environment();
    let contents = daemon_lock_contents(AutosyncDaemonPhase::Starting, startup_nonce.as_deref());

    for _attempt in 0..2 {
        match create_daemon_lock_atomically(&lock_path, &contents)? {
            CreateDaemonLockResult::Acquired => {
                let guard = AutosyncDaemonGuard {
                    path: lock_path.clone(),
                    contents: contents.clone(),
                };
                if let Some(nonce) = startup_nonce.as_deref() {
                    write_startup_state(
                        &state.state_dir,
                        nonce,
                        AutosyncStartupState::Starting,
                        None,
                    )?;
                }
                return Ok((guard, config.settings, state.root));
            }
            CreateDaemonLockResult::AlreadyExists => {
                let inspection = inspect_daemon_lock_path(&lock_path)?;
                if !daemon_owner_is_confirmed_dead(inspection.status.owner_liveness) {
                    return Err(invalid_input(
                        "auto-sync daemon ownership is active or cannot be verified",
                    ));
                }
                if let Some(stale_contents) = inspection.contents {
                    if remove_daemon_lock_if_contents_match(&lock_path, &stale_contents)? {
                        continue;
                    }
                } else {
                    continue;
                }
            }
        }
    }

    Err(invalid_input("failed to acquire auto-sync daemon lock"))
}

pub fn stop_autosync(request: AutosyncRequest) -> Result<AutosyncReport, RepoGrammarError> {
    let state = require_initialized_state(&request)?;
    let locks_dir = state.state_dir.join("locks");
    ensure_dir(&locks_dir, "failed to open auto-sync lock directory")?;
    // Serialize every cooperating acquire, stale cleanup, disable, and stop.
    // This closes the read/compare/unlink window: while this guard is held no
    // successor can publish a new daemon lock for this repository.
    let _lifecycle = acquire_daemon_lifecycle_guard(&locks_dir)?;
    let lock_path = locks_dir.join(DAEMON_LOCK_FILE);
    let lock = inspect_daemon_lock_path(&lock_path)?;
    let Some(pid) = lock.status.pid else {
        return autosync_status_for_state(&state, "auto-sync is not running");
    };
    if lock.status.owner_liveness == ProcessLiveness::Live {
        // A failed signal is not evidence that the owner stopped. Preserve its
        // lock and fail closed instead of reporting a false stopped state.
        terminate_process(pid)?;
        let expected = lock.contents.as_deref().ok_or_else(|| {
            invalid_input("failed to verify auto-sync daemon ownership during stop")
        })?;
        let mut stopped = false;
        for attempt in 0..AUTOSYNC_STOP_MAX_ATTEMPTS {
            let current = inspect_daemon_lock_path(&lock_path)?;
            match current.contents.as_deref() {
                None => {
                    stopped = true;
                    break;
                }
                Some(contents) if contents != expected => {
                    return Err(invalid_input(
                        "auto-sync daemon ownership changed during stop",
                    ));
                }
                Some(_) if daemon_owner_is_confirmed_dead(current.status.owner_liveness) => {
                    let _ = remove_daemon_lock_if_contents_match(&lock_path, expected)?;
                    stopped = true;
                    break;
                }
                Some(_) => {}
            }
            if attempt + 1 < AUTOSYNC_STOP_MAX_ATTEMPTS {
                std::thread::sleep(AUTOSYNC_STOP_POLL_INTERVAL);
            }
        }
        if !stopped {
            return Err(invalid_input(
                "auto-sync did not stop before the bounded shutdown timeout",
            ));
        }
    } else if daemon_owner_is_confirmed_dead(lock.status.owner_liveness) {
        if let Some(contents) = lock.contents {
            let _ = remove_daemon_lock_if_contents_match(&lock_path, &contents)?;
        }
    } else {
        return Err(invalid_input(
            "auto-sync daemon ownership cannot be verified; lock preserved",
        ));
    }
    autosync_status_for_state(&state, "auto-sync stopped")
}

pub fn daemon_log_path(request: &AutosyncRequest) -> Result<PathBuf, RepoGrammarError> {
    let state = require_initialized_state(request)?;
    let logs = state.state_dir.join("logs");
    ensure_dir(&logs, "failed to open auto-sync log directory")?;
    Ok(logs.join("daemon.log"))
}

struct AutosyncConfig {
    enabled: bool,
    settings: AutosyncSettings,
}

struct AutosyncState {
    root: PathBuf,
    state_dir: PathBuf,
    state_dir_relative: String,
}

struct DaemonLockStatus {
    running: bool,
    owner_liveness: ProcessLiveness,
    pid: Option<u32>,
    startup_nonce: Option<String>,
    phase: Option<AutosyncDaemonPhase>,
}

struct DaemonLockInspection {
    status: DaemonLockStatus,
    contents: Option<String>,
}

fn daemon_owner_is_confirmed_dead(liveness: ProcessLiveness) -> bool {
    liveness == ProcessLiveness::Dead
}

fn require_initialized_state(request: &AutosyncRequest) -> Result<AutosyncState, RepoGrammarError> {
    let status = repository_status(RepositoryStatusRequest {
        path: request.path.clone(),
        state_dir_override: request.state_dir_override.clone(),
    })?;
    match status.status {
        RepositoryStatus::Initialized { .. } => {}
        RepositoryStatus::NotInitialized => {
            return Err(invalid_input(
                "repository is not initialized; run repogrammar init",
            ));
        }
        RepositoryStatus::CorruptedManifest => {
            return Err(invalid_input(
                "repository manifest is corrupted; run repogrammar doctor",
            ));
        }
    }
    if !status.missing_subdirs.is_empty() {
        return Err(invalid_input(
            "repository-local state is missing required subdirectories; run repogrammar doctor",
        ));
    }
    let location = repository_state_location(RepositoryStatusRequest {
        path: request.path.clone(),
        state_dir_override: request.state_dir_override.clone(),
    })?;
    Ok(AutosyncState {
        root: location.root,
        state_dir: location.state_dir,
        state_dir_relative: location.state_dir_relative,
    })
}

fn autosync_status_for_state(
    state: &AutosyncState,
    message: &str,
) -> Result<AutosyncReport, RepoGrammarError> {
    let config = read_config(&state.state_dir)?;
    let lock = inspect_daemon_lock(&state.state_dir)?;
    Ok(AutosyncReport {
        state_dir: state.state_dir_relative.clone(),
        enabled: config.enabled,
        running: lock.running,
        daemon_state: match lock.owner_liveness {
            ProcessLiveness::Unknown => AutosyncDaemonState::Unknown,
            ProcessLiveness::Live if lock.running => AutosyncDaemonState::Running,
            ProcessLiveness::Live => AutosyncDaemonState::Starting,
            ProcessLiveness::Dead => AutosyncDaemonState::Stopped,
        },
        pid: lock.pid.filter(|_| lock.running),
        poll_ms: config.settings.poll_ms,
        debounce_ms: config.settings.debounce_ms,
        last_run: read_run_state(&state.state_dir),
        startup: startup_report(&state.state_dir, &lock),
        repository_ready: true,
        message: message.to_string(),
    })
}

pub fn record_autosync_startup_failure(
    request: &AutosyncRequest,
    startup_nonce: &str,
    code: AutosyncStartupFailureCode,
) -> Result<(), RepoGrammarError> {
    let state = require_initialized_state(request)?;
    write_startup_state(
        &state.state_dir,
        startup_nonce,
        AutosyncStartupState::Failed,
        Some(code),
    )
}

pub fn record_autosync_startup_ready(
    request: &AutosyncRequest,
    startup_nonce: &str,
) -> Result<(), RepoGrammarError> {
    let state = require_initialized_state(request)?;
    write_startup_state(
        &state.state_dir,
        startup_nonce,
        AutosyncStartupState::Ready,
        None,
    )
}

/// Record the outcome of one daemon sync attempt. Best-effort: callers ignore
/// errors so a failed status write never aborts the daemon.
pub fn record_autosync_run(
    request: &AutosyncRequest,
    result: AutosyncRunResult,
    synced_generation: Option<&str>,
    error: Option<&str>,
) -> Result<(), RepoGrammarError> {
    let state = require_initialized_state(request)?;
    write_run_state(&state.state_dir, result, synced_generation, error)
}

fn run_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(AUTOSYNC_RUN_FILE)
}

fn write_run_state(
    state_dir: &Path,
    result: AutosyncRunResult,
    synced_generation: Option<&str>,
    error: Option<&str>,
) -> Result<(), RepoGrammarError> {
    let path = run_state_path(state_dir);
    let tmp = path.with_extension("tmp");
    let last_sync_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let synced_generation =
        synced_generation.filter(|generation| valid_autosync_generation_id(generation));
    let error = sanitized_autosync_run_error(result, error);
    let value = json!({
        "schema_version": AUTOSYNC_SCHEMA_VERSION,
        "last_sync_unix_seconds": last_sync_unix_seconds,
        "result": result.as_str(),
        "synced_generation": synced_generation,
        "error": error,
    });
    fs::write(&tmp, value.to_string())
        .map_err(|_| invalid_input("failed to write auto-sync run state"))?;
    fs::rename(&tmp, path).map_err(|_| invalid_input("failed to replace auto-sync run state"))
}

fn read_run_state(state_dir: &Path) -> Option<AutosyncRunReport> {
    let text = read_limited_text(&run_state_path(state_dir)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(AUTOSYNC_SCHEMA_VERSION) {
        return None;
    }
    let last_sync_unix_seconds = value
        .get("last_sync_unix_seconds")
        .and_then(Value::as_u64)?;
    let result = match value.get("result").and_then(Value::as_str)? {
        "ok" => AutosyncRunResult::Ok,
        "error" => AutosyncRunResult::Error,
        _ => return None,
    };
    let synced_generation = value
        .get("synced_generation")
        .and_then(Value::as_str)
        .filter(|generation| valid_autosync_generation_id(generation))
        .map(str::to_string);
    let error = sanitized_autosync_run_error(result, value.get("error").and_then(Value::as_str))
        .map(str::to_string);
    Some(AutosyncRunReport {
        last_sync_unix_seconds,
        result,
        synced_generation,
        error,
    })
}

fn sanitized_autosync_run_error(result: AutosyncRunResult, error: Option<&str>) -> Option<&str> {
    if result != AutosyncRunResult::Error {
        return None;
    }
    Some(match error {
        Some(AUTOSYNC_RUN_ERROR_FINGERPRINT_FAILED) => AUTOSYNC_RUN_ERROR_FINGERPRINT_FAILED,
        Some(AUTOSYNC_RUN_ERROR_STATE_UNAVAILABLE) => AUTOSYNC_RUN_ERROR_STATE_UNAVAILABLE,
        Some(AUTOSYNC_RUN_ERROR_SYNC_FAILED) => AUTOSYNC_RUN_ERROR_SYNC_FAILED,
        _ => AUTOSYNC_RUN_ERROR_UNKNOWN,
    })
}

fn valid_autosync_generation_id(value: &str) -> bool {
    let Some(digits) = value.strip_prefix("gen-") else {
        return false;
    };
    digits.len() == 6
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && digits.parse::<u32>().is_ok_and(|number| number > 0)
}

struct PersistedStartupState {
    startup_nonce: String,
    state: AutosyncStartupState,
    failure_code: Option<AutosyncStartupFailureCode>,
}

fn startup_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(AUTOSYNC_STARTUP_FILE)
}

fn write_startup_state(
    state_dir: &Path,
    startup_nonce: &str,
    state: AutosyncStartupState,
    failure_code: Option<AutosyncStartupFailureCode>,
) -> Result<(), RepoGrammarError> {
    if !valid_startup_nonce(startup_nonce) {
        return Err(invalid_input("failed to record auto-sync startup state"));
    }
    let path = startup_state_path(state_dir);
    let tmp = path.with_extension("tmp");
    let value = json!({
        "schema_version": AUTOSYNC_SCHEMA_VERSION,
        "startup_unix_seconds": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "startup_nonce": startup_nonce,
        "state": state.as_str(),
        "failure_code": failure_code.map(AutosyncStartupFailureCode::as_str),
    });
    fs::write(&tmp, value.to_string())
        .map_err(|_| invalid_input("failed to write auto-sync startup state"))?;
    fs::rename(&tmp, path).map_err(|_| invalid_input("failed to replace auto-sync startup state"))
}

fn read_startup_state(state_dir: &Path) -> Option<PersistedStartupState> {
    let text = read_limited_text(&startup_state_path(state_dir)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(AUTOSYNC_SCHEMA_VERSION) {
        return None;
    }
    let startup_nonce = value
        .get("startup_nonce")
        .and_then(Value::as_str)
        .filter(|nonce| valid_startup_nonce(nonce))?
        .to_string();
    let state = match value.get("state").and_then(Value::as_str)? {
        "starting" => AutosyncStartupState::Starting,
        "ready" => AutosyncStartupState::Ready,
        "failed" => AutosyncStartupState::Failed,
        _ => return None,
    };
    let failure_code = value
        .get("failure_code")
        .and_then(Value::as_str)
        .and_then(parse_startup_failure_code);
    if state == AutosyncStartupState::Failed && failure_code.is_none() {
        return None;
    }
    Some(PersistedStartupState {
        startup_nonce,
        state,
        failure_code,
    })
}

fn parse_startup_failure_code(value: &str) -> Option<AutosyncStartupFailureCode> {
    match value {
        "worker_environment_invalid" => Some(AutosyncStartupFailureCode::WorkerEnvironmentInvalid),
        "repository_fingerprint_failed" => {
            Some(AutosyncStartupFailureCode::RepositoryFingerprintFailed)
        }
        "repository_state_unavailable" => {
            Some(AutosyncStartupFailureCode::RepositoryStateUnavailable)
        }
        "daemon_lock_refused" => Some(AutosyncStartupFailureCode::DaemonLockRefused),
        "child_exited_before_ready" => Some(AutosyncStartupFailureCode::ChildExitedBeforeReady),
        "startup_timeout" => Some(AutosyncStartupFailureCode::StartupTimeout),
        "first_heartbeat_failed" => Some(AutosyncStartupFailureCode::FirstHeartbeatFailed),
        _ => None,
    }
}

fn read_startup_failure(
    state_dir: &Path,
    expected_nonce: &str,
) -> Option<AutosyncStartupFailureCode> {
    let startup = read_startup_state(state_dir)?;
    (startup.startup_nonce == expected_nonce && startup.state == AutosyncStartupState::Failed)
        .then_some(startup.failure_code)
        .flatten()
}

fn startup_report(state_dir: &Path, lock: &DaemonLockStatus) -> AutosyncStartupReport {
    let persisted = read_startup_state(state_dir);
    if lock.owner_liveness == ProcessLiveness::Live {
        let current_failure_code = persisted.as_ref().and_then(|startup| {
            (startup.state == AutosyncStartupState::Failed
                && lock.startup_nonce.as_deref() == Some(startup.startup_nonce.as_str()))
            .then_some(startup.failure_code)
            .flatten()
        });
        let previous_failure_code = persisted.as_ref().and_then(|startup| {
            (startup.state == AutosyncStartupState::Failed
                && lock.startup_nonce.as_deref() != Some(startup.startup_nonce.as_str()))
            .then_some(startup.failure_code)
            .flatten()
        });
        return AutosyncStartupReport {
            state: if current_failure_code.is_some() {
                AutosyncStartupState::Failed
            } else {
                match lock.phase {
                    Some(AutosyncDaemonPhase::Starting) | None => AutosyncStartupState::Starting,
                    Some(AutosyncDaemonPhase::Ready) => AutosyncStartupState::Ready,
                }
            },
            failure_code: current_failure_code,
            previous_failure_code,
        };
    }
    let previous_failure_code = persisted.and_then(|startup| {
        (startup.state == AutosyncStartupState::Failed)
            .then_some(startup.failure_code)
            .flatten()
    });
    AutosyncStartupReport {
        state: AutosyncStartupState::Idle,
        failure_code: None,
        previous_failure_code,
    }
}

fn read_config(state_dir: &Path) -> Result<AutosyncConfig, RepoGrammarError> {
    let path = config_path(state_dir);
    let text = match read_limited_text(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AutosyncConfig {
                enabled: false,
                settings: AutosyncSettings::default(),
            });
        }
        Err(_) => return Err(invalid_input("failed to read auto-sync config")),
    };
    let value: Value =
        serde_json::from_str(&text).map_err(|_| invalid_input("auto-sync config is invalid"))?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(AUTOSYNC_SCHEMA_VERSION) {
        return Err(invalid_input("auto-sync config schema is unsupported"));
    }
    let enabled = value
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_input("auto-sync config is invalid"))?;
    let poll_ms = value
        .get("poll_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_input("auto-sync config is invalid"))?;
    let debounce_ms = value
        .get("debounce_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_input("auto-sync config is invalid"))?;
    let settings = AutosyncSettings {
        poll_ms,
        debounce_ms,
    };
    validate_autosync_settings(settings)?;
    Ok(AutosyncConfig { enabled, settings })
}

fn write_config(state_dir: &Path, config: &AutosyncConfig) -> Result<(), RepoGrammarError> {
    let path = config_path(state_dir);
    let tmp = path.with_extension("tmp");
    let value = json!({
        "schema_version": AUTOSYNC_SCHEMA_VERSION,
        "enabled": config.enabled,
        "poll_ms": config.settings.poll_ms,
        "debounce_ms": config.settings.debounce_ms,
    });
    fs::write(&tmp, value.to_string())
        .map_err(|_| invalid_input("failed to write auto-sync config"))?;
    fs::rename(&tmp, path).map_err(|_| invalid_input("failed to replace auto-sync config"))
}

fn inspect_daemon_lock(state_dir: &Path) -> Result<DaemonLockStatus, RepoGrammarError> {
    let path = state_dir.join("locks").join(DAEMON_LOCK_FILE);
    Ok(inspect_daemon_lock_path(&path)?.status)
}

fn inspect_daemon_lock_path(path: &Path) -> Result<DaemonLockInspection, RepoGrammarError> {
    let text = match read_limited_text(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DaemonLockInspection {
                status: DaemonLockStatus {
                    running: false,
                    owner_liveness: ProcessLiveness::Dead,
                    pid: None,
                    startup_nonce: None,
                    phase: None,
                },
                contents: None,
            });
        }
        Err(_) => return Err(invalid_input("failed to inspect auto-sync daemon lock")),
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| invalid_input("auto-sync daemon lock is invalid"))?;
    if value.get("kind").and_then(Value::as_str) != Some("autosync_daemon") {
        return Err(invalid_input("auto-sync daemon lock is invalid"));
    }
    let pid_u64 = value
        .get("pid")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_input("auto-sync daemon lock is invalid"))?;
    let pid =
        u32::try_from(pid_u64).map_err(|_| invalid_input("auto-sync daemon lock is invalid"))?;
    let started_unix_seconds = value.get("started_unix_seconds").and_then(Value::as_u64);
    let startup_nonce = value
        .get("startup_nonce")
        .and_then(Value::as_str)
        .filter(|nonce| valid_startup_nonce(nonce))
        .map(str::to_string);
    let phase = match value.get("phase").and_then(Value::as_str) {
        Some("starting") => Some(AutosyncDaemonPhase::Starting),
        Some("ready") => Some(AutosyncDaemonPhase::Ready),
        Some(_) => return Err(invalid_input("auto-sync daemon lock is invalid")),
        None => None,
    };
    let owner_liveness = autosync_daemon_process_liveness(pid, started_unix_seconds);
    Ok(DaemonLockInspection {
        status: DaemonLockStatus {
            // The PID alone is not proof of liveness: after an unclean daemon
            // exit the OS can reuse it for an unrelated process. Require both
            // that the PID exists and that it is actually a RepoGrammar autosync
            // daemon, so `stop` never signals a stranger and `start` is never
            // permanently blocked by a reused PID.
            running: owner_liveness == ProcessLiveness::Live
                && phase == Some(AutosyncDaemonPhase::Ready),
            owner_liveness,
            pid: Some(pid),
            startup_nonce,
            phase,
        },
        contents: Some(text),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateDaemonLockResult {
    Acquired,
    AlreadyExists,
}

fn create_daemon_lock_atomically(
    lock_path: &Path,
    contents: &str,
) -> Result<CreateDaemonLockResult, RepoGrammarError> {
    let tmp_path = temporary_daemon_lock_path(lock_path);
    let tmp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|_| invalid_input("failed to create auto-sync daemon lock temp file"))?;
    write_daemon_lock_contents(tmp_file, &tmp_path, contents)?;

    let link_result = match fs::hard_link(&tmp_path, lock_path) {
        Ok(()) => CreateDaemonLockResult::Acquired,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            CreateDaemonLockResult::AlreadyExists
        }
        Err(_) => create_daemon_lock_with_exclusive_open(lock_path, contents)?,
    };
    let _ = fs::remove_file(&tmp_path);
    Ok(link_result)
}

fn acquire_daemon_lifecycle_guard(
    locks_dir: &Path,
) -> Result<AutosyncDaemonLifecycleGuard, RepoGrammarError> {
    let path = locks_dir.join(DAEMON_LIFECYCLE_FILE);
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let contents = json!({
        "schema_version": AUTOSYNC_SCHEMA_VERSION,
        "kind": "autosync_daemon_lifecycle",
        "pid": std::process::id(),
        "started_unix_seconds": started.as_secs(),
        "token": format!("{}-{}", std::process::id(), started.as_nanos()),
    })
    .to_string();
    for _attempt in 0..2 {
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if remove_stale_daemon_lifecycle_marker(&path)? {
                    continue;
                }
                return Err(invalid_input(
                    "auto-sync lifecycle operation is already in progress",
                ));
            }
            Err(_) => {
                return Err(invalid_input(
                    "failed to acquire auto-sync lifecycle ownership",
                ));
            }
        };
        write_daemon_lock_contents(file, &path, &contents)
            .map_err(|_| invalid_input("failed to acquire auto-sync lifecycle ownership"))?;
        return Ok(AutosyncDaemonLifecycleGuard { path, contents });
    }
    Err(invalid_input(
        "failed to acquire auto-sync lifecycle ownership",
    ))
}

fn remove_stale_daemon_lifecycle_marker(path: &Path) -> Result<bool, RepoGrammarError> {
    let expected = read_limited_text(path)
        .map_err(|_| invalid_input("failed to inspect auto-sync lifecycle ownership"))?;
    let value: Value = serde_json::from_str(&expected)
        .map_err(|_| invalid_input("auto-sync lifecycle ownership is invalid"))?;
    if value.get("kind").and_then(Value::as_str) != Some("autosync_daemon_lifecycle") {
        return Err(invalid_input("auto-sync lifecycle ownership is invalid"));
    }
    let pid = value
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| invalid_input("auto-sync lifecycle ownership is invalid"))?;
    let started = value.get("started_unix_seconds").and_then(Value::as_u64);
    if process_liveness_for_lock(pid, started) != ProcessLiveness::Dead {
        return Ok(false);
    }

    // Moving the fixed marker out of the way is the serialization point. Only
    // the process whose rename succeeds may discard the inspected stale owner;
    // all contenders then race through create_new, where exactly one wins.
    let quarantine = path.with_file_name(format!(
        "{DAEMON_LIFECYCLE_FILE}.{}-{}.stale",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    match fs::rename(path, &quarantine) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(_) => {
            return Err(invalid_input(
                "failed to recover stale auto-sync lifecycle ownership",
            ));
        }
    }
    let quarantined = read_limited_text(&quarantine)
        .map_err(|_| invalid_input("failed to verify stale auto-sync lifecycle ownership"))?;
    if quarantined != expected {
        if !path.exists() {
            let _ = fs::rename(&quarantine, path);
        }
        return Err(invalid_input(
            "auto-sync lifecycle ownership changed during recovery",
        ));
    }
    fs::remove_file(&quarantine)
        .map_err(|_| invalid_input("failed to recover stale auto-sync lifecycle ownership"))?;
    Ok(true)
}

fn temporary_daemon_lock_path(lock_path: &Path) -> PathBuf {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    lock_path.with_file_name(format!(
        "{DAEMON_LOCK_FILE}.{}-{started}.tmp",
        std::process::id()
    ))
}

fn create_daemon_lock_with_exclusive_open(
    lock_path: &Path,
    contents: &str,
) -> Result<CreateDaemonLockResult, RepoGrammarError> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(file) => {
            write_daemon_lock_contents(file, lock_path, contents)?;
            Ok(CreateDaemonLockResult::Acquired)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(CreateDaemonLockResult::AlreadyExists)
        }
        Err(_) => Err(invalid_input("failed to create auto-sync daemon lock")),
    }
}

fn write_daemon_lock_contents<W: Write>(
    mut writer: W,
    lock_path: &Path,
    contents: &str,
) -> Result<(), RepoGrammarError> {
    if writer.write_all(contents.as_bytes()).is_err() {
        let _ = fs::remove_file(lock_path);
        return Err(invalid_input("failed to write auto-sync daemon lock"));
    }
    Ok(())
}

fn remove_daemon_lock_if_contents_match(
    lock_path: &Path,
    expected: &str,
) -> Result<bool, RepoGrammarError> {
    let current = match read_limited_text(lock_path) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(_) => return Err(invalid_input("failed to inspect auto-sync daemon lock")),
    };
    if current != expected {
        return Ok(false);
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err(invalid_input(
            "failed to remove stale auto-sync daemon lock",
        )),
    }
}

fn replace_owned_daemon_lock(
    lock_path: &Path,
    expected: &str,
    replacement: &str,
) -> Result<(), RepoGrammarError> {
    replace_owned_daemon_lock_with_hook(lock_path, expected, replacement, || {})
}

fn replace_owned_daemon_lock_with_hook<F>(
    lock_path: &Path,
    expected: &str,
    replacement: &str,
    after_quarantine: F,
) -> Result<(), RepoGrammarError>
where
    F: FnOnce(),
{
    let current = read_limited_text(lock_path)
        .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;
    if current != expected {
        return Err(invalid_input(
            "auto-sync daemon ownership changed before readiness",
        ));
    }
    let tmp_path = temporary_daemon_lock_path(lock_path);
    let tmp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;
    write_daemon_lock_contents(tmp_file, &tmp_path, replacement)
        .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;

    // Move the observed owner out of the canonical name, then create the ready
    // name with a no-overwrite hard link. Unlike rename-over-destination, this
    // cannot silently replace a non-cooperating owner that appears during the
    // transition.
    let quarantine_path = lock_path.with_file_name(format!(
        "{DAEMON_LOCK_FILE}.{}-{}.transition",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if fs::rename(lock_path, &quarantine_path).is_err() {
        let _ = fs::remove_file(&tmp_path);
        return Err(invalid_input(
            "auto-sync daemon ownership changed before readiness",
        ));
    }
    let quarantined = read_limited_text(&quarantine_path)
        .map_err(|_| invalid_input("failed to publish auto-sync readiness"))?;
    if quarantined != expected {
        let restored = fs::hard_link(&quarantine_path, lock_path).is_ok();
        if restored {
            let _ = fs::remove_file(&quarantine_path);
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(invalid_input(
            "auto-sync daemon ownership changed before readiness",
        ));
    }

    after_quarantine();
    match fs::hard_link(&tmp_path, lock_path) {
        Ok(()) => {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&quarantine_path);
            Ok(())
        }
        Err(_) => {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&quarantine_path);
            Err(invalid_input(
                "auto-sync daemon ownership changed before readiness",
            ))
        }
    }
}

fn daemon_lock_contents(phase: AutosyncDaemonPhase, startup_nonce: Option<&str>) -> String {
    let startup_nonce = startup_nonce.filter(|nonce| valid_startup_nonce(nonce));
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    json!({
        "schema_version": AUTOSYNC_SCHEMA_VERSION,
        "kind": "autosync_daemon",
        "pid": std::process::id(),
        "started_unix_seconds": started,
        "repogrammar_version": env!("CARGO_PKG_VERSION"),
        "startup_nonce": startup_nonce,
        "phase": phase.as_str(),
    })
    .to_string()
}

fn startup_nonce_from_environment() -> Option<String> {
    std::env::var(AUTOSYNC_STARTUP_NONCE_ENV)
        .ok()
        .filter(|nonce| valid_startup_nonce(nonce))
}

fn valid_startup_nonce(nonce: &str) -> bool {
    (16..=64).contains(&nonce.len()) && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn config_path(state_dir: &Path) -> PathBuf {
    state_dir.join(AUTOSYNC_CONFIG_FILE)
}

fn validate_autosync_settings(settings: AutosyncSettings) -> Result<(), RepoGrammarError> {
    if !(100..=600_000).contains(&settings.poll_ms) {
        return Err(invalid_input("--poll-ms must be between 100 and 600000"));
    }
    if settings.debounce_ms > 60_000 {
        return Err(invalid_input("--debounce-ms must be no greater than 60000"));
    }
    Ok(())
}

fn ensure_dir(path: &Path, message: &str) -> Result<(), RepoGrammarError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid_input(message))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_input(message));
    }
    Ok(())
}

fn read_limited_text(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    if bytes.len() > 64 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "state file is too large",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "state file is not UTF-8")
    })
}

fn terminate_process(pid: u32) -> Result<(), RepoGrammarError> {
    #[cfg(unix)]
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(unix)]
    {
        status
            .map_err(|_| invalid_input("failed to stop auto-sync process"))
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(invalid_input("failed to stop auto-sync process"))
                }
            })
    }
    #[cfg(windows)]
    {
        if windows_terminate_process(pid) {
            Ok(())
        } else {
            Err(invalid_input("failed to stop auto-sync process"))
        }
    }
}

#[cfg(windows)]
fn windows_terminate_process(pid: u32) -> bool {
    const PROCESS_TERMINATE: u32 = 0x0001;

    let Some(handle) = open_process(PROCESS_TERMINATE, pid) else {
        return false;
    };
    unsafe { TerminateProcess(handle.0, 1) != 0 }
}

#[cfg(windows)]
fn open_process(access: u32, pid: u32) -> Option<WindowsProcessHandle> {
    let handle = unsafe { OpenProcess(access, 0, pid) };
    (!handle.is_null()).then_some(WindowsProcessHandle(handle))
}

#[cfg(windows)]
struct WindowsProcessHandle(*mut std::ffi::c_void);

#[cfg(windows)]
impl Drop for WindowsProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        dw_process_id: u32,
    ) -> *mut std::ffi::c_void;
    fn TerminateProcess(h_process: *mut std::ffi::c_void, u_exit_code: u32) -> i32;
    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
}

fn invalid_input(message: impl Into<String>) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{
        init_repository, RepositoryLifecycleInitRequest, RepositoryManifestStatus,
        RepositoryReadiness,
    };
    use crate::test_support::TempWorkspace;

    fn request(workspace: &TempWorkspace) -> AutosyncRequest {
        AutosyncRequest::new(workspace.path().display().to_string())
    }

    fn initialized_autosync_workspace(name: &str) -> TempWorkspace {
        let workspace = TempWorkspace::new(name);
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        workspace
    }

    fn repository_report(
        status: RepositoryStatus,
        missing_subdirs: Vec<String>,
        storage: RepositoryImplementationStatus,
    ) -> RepositoryStatusReport {
        RepositoryStatusReport {
            state_dir: ".repogrammar".to_string(),
            manifest: match status {
                RepositoryStatus::NotInitialized => RepositoryManifestStatus::Missing,
                RepositoryStatus::CorruptedManifest => RepositoryManifestStatus::Corrupted,
                RepositoryStatus::Initialized { .. } => RepositoryManifestStatus::Valid,
            },
            status,
            manifest_schema_version: None,
            missing_subdirs,
            storage,
            indexing: RepositoryImplementationStatus::NotImplemented,
            storage_inspection: None,
            storage_error: None,
            readiness: RepositoryReadiness::default(),
        }
    }

    #[test]
    fn daemon_terminal_state_uses_typed_repository_status() {
        let not_initialized = repository_report(
            RepositoryStatus::NotInitialized,
            Vec::new(),
            RepositoryImplementationStatus::NotImplemented,
        );
        assert_eq!(
            classify_autosync_repository_status(&not_initialized),
            Some(AutosyncRepositoryUnavailable::NotInitialized)
        );

        let corrupted = repository_report(
            RepositoryStatus::CorruptedManifest,
            Vec::new(),
            RepositoryImplementationStatus::NotImplemented,
        );
        assert_eq!(
            classify_autosync_repository_status(&corrupted),
            Some(AutosyncRepositoryUnavailable::CorruptedManifest)
        );

        let missing = repository_report(
            RepositoryStatus::Initialized {
                active_generation: "none".to_string(),
            },
            vec!["locks".to_string()],
            RepositoryImplementationStatus::Unhealthy,
        );
        assert_eq!(
            classify_autosync_repository_status(&missing),
            Some(AutosyncRepositoryUnavailable::MissingStateSubdirectories)
        );

        let unhealthy = repository_report(
            RepositoryStatus::Initialized {
                active_generation: "none".to_string(),
            },
            Vec::new(),
            RepositoryImplementationStatus::Unhealthy,
        );
        assert_eq!(
            classify_autosync_repository_status(&unhealthy),
            Some(AutosyncRepositoryUnavailable::StorageUnhealthy)
        );
    }

    #[test]
    fn unknown_daemon_owner_never_allows_stale_cleanup() {
        assert!(daemon_owner_is_confirmed_dead(ProcessLiveness::Dead));
        assert!(!daemon_owner_is_confirmed_dead(ProcessLiveness::Live));
        assert!(!daemon_owner_is_confirmed_dead(ProcessLiveness::Unknown));
    }

    #[test]
    fn startup_probe_reports_pending_without_lock_and_refusal_for_malformed_lock() {
        let workspace = TempWorkspace::new("autosync-startup-probe");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");

        let request = request(&workspace);
        assert_eq!(
            inspect_autosync_startup(&request, 42, "0123456789abcdef")
                .expect("pending startup probe"),
            AutosyncStartupReadiness::Pending
        );

        fs::write(
            workspace.path().join(".repogrammar/locks/daemon.lock"),
            "not-json",
        )
        .expect("write malformed lock");
        assert_eq!(
            inspect_autosync_startup(&request, 42, "0123456789abcdef")
                .expect("refused startup probe"),
            AutosyncStartupReadiness::LockRefused
        );
    }

    #[test]
    fn starting_lock_is_not_ready_until_guard_publishes_ready() {
        let workspace = initialized_autosync_workspace("autosync-starting-ready-phase");
        let nonce = "0123456789abcdef";
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let starting = daemon_lock_contents(AutosyncDaemonPhase::Starting, Some(nonce));
        fs::write(&lock_path, &starting).expect("write starting lock");
        let mut guard = AutosyncDaemonGuard {
            path: lock_path.clone(),
            contents: starting,
        };

        assert_eq!(
            inspect_autosync_startup(&request(&workspace), std::process::id(), nonce)
                .expect("inspect starting"),
            AutosyncStartupReadiness::Pending
        );

        guard.publish_ready().expect("publish ready");
        let ready = fs::read_to_string(&lock_path).expect("read ready lock");
        assert_eq!(
            serde_json::from_str::<Value>(&ready).expect("ready JSON")["phase"],
            "ready"
        );
        assert!(matches!(
            inspect_autosync_startup(&request(&workspace), std::process::id(), nonce)
                .expect("inspect ready"),
            AutosyncStartupReadiness::Ready(_)
        ));

        drop(guard);
        assert!(!lock_path.exists());
    }

    #[test]
    fn startup_failure_after_starting_lock_is_observed_before_false_ready() {
        let workspace = initialized_autosync_workspace("autosync-startup-failure-record");
        let nonce = "fedcba9876543210";
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let starting = daemon_lock_contents(AutosyncDaemonPhase::Starting, Some(nonce));
        fs::write(&lock_path, &starting).expect("write starting lock");
        let guard = AutosyncDaemonGuard {
            path: lock_path.clone(),
            contents: starting,
        };
        record_autosync_startup_failure(
            &request(&workspace),
            nonce,
            AutosyncStartupFailureCode::RepositoryFingerprintFailed,
        )
        .expect("record failure");

        assert_eq!(
            inspect_autosync_startup(&request(&workspace), std::process::id(), nonce)
                .expect("inspect failed startup"),
            AutosyncStartupReadiness::Failed(
                AutosyncStartupFailureCode::RepositoryFingerprintFailed
            )
        );

        drop(guard);
        let status = autosync_status(request(&workspace)).expect("status after failure");
        assert!(!status.running);
        assert_eq!(status.startup.state, AutosyncStartupState::Idle);
        assert_eq!(status.startup.failure_code, None);
        assert_eq!(
            status.startup.previous_failure_code,
            Some(AutosyncStartupFailureCode::RepositoryFingerprintFailed)
        );
    }

    #[test]
    fn stale_ready_record_from_another_pid_or_nonce_is_never_accepted() {
        let workspace = initialized_autosync_workspace("autosync-stale-ready-record");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let mut stale: Value = serde_json::from_str(&daemon_lock_contents(
            AutosyncDaemonPhase::Ready,
            Some("0123456789abcdef"),
        ))
        .expect("parse stale ready lock");
        stale["pid"] = Value::from(0);
        fs::write(lock_path, stale.to_string()).expect("write stale ready lock");

        assert_eq!(
            inspect_autosync_startup(&request(&workspace), 42, "fedcba9876543210")
                .expect("inspect stale ready"),
            AutosyncStartupReadiness::Pending
        );
    }

    #[test]
    fn ready_transition_preserves_replacement_owner_and_guard_cleanup_is_safe() {
        let workspace = initialized_autosync_workspace("autosync-ready-replacement-owner");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let starting =
            daemon_lock_contents(AutosyncDaemonPhase::Starting, Some("0123456789abcdef"));
        fs::write(&lock_path, &starting).expect("write starting lock");
        let mut guard = AutosyncDaemonGuard {
            path: lock_path.clone(),
            contents: starting,
        };
        let replacement =
            daemon_lock_contents(AutosyncDaemonPhase::Ready, Some("fedcba9876543210"));
        fs::write(&lock_path, &replacement).expect("replace owner");

        let error = guard
            .publish_ready()
            .expect_err("replacement owner must block transition");
        assert_eq!(
            error,
            RepoGrammarError::InvalidInput(
                "auto-sync daemon ownership changed before readiness".to_string()
            )
        );
        drop(guard);
        assert_eq!(
            fs::read_to_string(lock_path).expect("replacement preserved"),
            replacement
        );
    }

    #[test]
    fn ready_transition_never_overwrites_owner_created_after_quarantine() {
        let workspace = initialized_autosync_workspace("autosync-ready-quarantine-race");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let starting =
            daemon_lock_contents(AutosyncDaemonPhase::Starting, Some("0123456789abcdef"));
        fs::write(&lock_path, &starting).expect("write starting lock");
        let mut ready_value: Value = serde_json::from_str(&starting).expect("parse starting lock");
        ready_value["phase"] = Value::String("ready".to_string());
        let ready = ready_value.to_string();
        let foreign = daemon_lock_contents(AutosyncDaemonPhase::Starting, Some("fedcba9876543210"));

        let error = replace_owned_daemon_lock_with_hook(&lock_path, &starting, &ready, || {
            fs::write(&lock_path, &foreign).expect("install competing owner")
        })
        .expect_err("competing owner must win without overwrite");

        assert_eq!(
            error,
            RepoGrammarError::InvalidInput(
                "auto-sync daemon ownership changed before readiness".to_string()
            )
        );
        assert_eq!(
            fs::read_to_string(&lock_path).expect("read competing owner"),
            foreign
        );
        assert!(fs::read_dir(lock_path.parent().expect("locks dir"))
            .expect("read locks dir")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains("transition")));
    }

    #[test]
    fn concurrent_startup_failure_is_previous_while_current_owner_is_ready() {
        let workspace = initialized_autosync_workspace("autosync-concurrent-startup-report");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let current_nonce = "0123456789abcdef";
        let losing_nonce = "fedcba9876543210";
        let ready = daemon_lock_contents(AutosyncDaemonPhase::Ready, Some(current_nonce));
        fs::write(&lock_path, &ready).expect("write current ready lock");
        let guard = AutosyncDaemonGuard {
            path: lock_path,
            contents: ready,
        };
        record_autosync_startup_failure(
            &request(&workspace),
            losing_nonce,
            AutosyncStartupFailureCode::DaemonLockRefused,
        )
        .expect("record losing startup");

        let parent_view =
            inspect_autosync_startup(&request(&workspace), std::process::id(), current_nonce)
                .expect("inspect current startup");
        let AutosyncStartupReadiness::Ready(parent_report) = parent_view else {
            panic!("exact current owner must be ready: {parent_view:?}");
        };
        assert_eq!(
            parent_report.startup.previous_failure_code,
            Some(AutosyncStartupFailureCode::DaemonLockRefused)
        );

        let current = startup_report(
            workspace.path().join(".repogrammar").as_path(),
            &DaemonLockStatus {
                running: true,
                owner_liveness: ProcessLiveness::Live,
                pid: Some(std::process::id()),
                startup_nonce: Some(current_nonce.to_string()),
                phase: Some(AutosyncDaemonPhase::Ready),
            },
        );
        assert_eq!(current.state, AutosyncStartupState::Ready);
        assert_eq!(current.failure_code, None);
        assert_eq!(
            current.previous_failure_code,
            Some(AutosyncStartupFailureCode::DaemonLockRefused)
        );

        drop(guard);
        let stopped = autosync_status(request(&workspace)).expect("stopped status");
        assert_eq!(stopped.startup.state, AutosyncStartupState::Idle);
        assert_eq!(stopped.startup.failure_code, None);
        assert_eq!(
            stopped.startup.previous_failure_code,
            Some(AutosyncStartupFailureCode::DaemonLockRefused)
        );
    }

    #[test]
    fn startup_failure_codes_round_trip_without_sensitive_payloads() {
        let workspace = initialized_autosync_workspace("autosync-startup-code-roundtrip");
        let nonce = "0123456789abcdef";
        for code in [
            AutosyncStartupFailureCode::WorkerEnvironmentInvalid,
            AutosyncStartupFailureCode::RepositoryFingerprintFailed,
            AutosyncStartupFailureCode::RepositoryStateUnavailable,
            AutosyncStartupFailureCode::DaemonLockRefused,
            AutosyncStartupFailureCode::ChildExitedBeforeReady,
            AutosyncStartupFailureCode::StartupTimeout,
            AutosyncStartupFailureCode::FirstHeartbeatFailed,
        ] {
            record_autosync_startup_failure(&request(&workspace), nonce, code)
                .expect("record startup failure");
            assert_eq!(
                read_startup_failure(workspace.path().join(".repogrammar").as_path(), nonce),
                Some(code)
            );
            assert!(code
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_'));
            assert!(!code.as_str().contains('/'));
            assert!(!code.as_str().contains("REPOGRAMMAR_"));
        }
        let persisted =
            fs::read_to_string(workspace.path().join(".repogrammar/autosync-startup.json"))
                .expect("read startup state");
        assert!(!persisted.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!persisted.contains("credential"));
    }

    #[test]
    fn daemon_lock_records_only_valid_startup_nonces() {
        let valid = daemon_lock_contents(AutosyncDaemonPhase::Starting, Some("0123456789abcdef"));
        let value: Value = serde_json::from_str(&valid).expect("valid lock JSON");
        assert_eq!(value["startup_nonce"], "0123456789abcdef");
        assert_eq!(value["phase"], "starting");
        assert!(valid_startup_nonce("0123456789abcdef"));
        assert!(!valid_startup_nonce("secret/path/value"));
        assert!(!valid_startup_nonce("short"));
    }

    #[cfg(unix)]
    #[test]
    fn reused_non_daemon_pid_is_not_reported_running() {
        let workspace = TempWorkspace::new("autosync-reused-pid");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        // The current test process exists but is not an autosync daemon; it
        // stands in for an unrelated process that reused the daemon's PID.
        fs::write(
            &lock_path,
            json!({
                "schema_version": AUTOSYNC_SCHEMA_VERSION,
                "kind": "autosync_daemon",
                "pid": std::process::id(),
                "started_unix_seconds": 0,
                "repogrammar_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string(),
        )
        .expect("write reused-pid lock");

        let status = autosync_status(request(&workspace)).expect("status");
        assert!(!status.running);
    }

    #[test]
    fn stop_clears_stale_lock_without_error() {
        let workspace = TempWorkspace::new("autosync-stop-stale");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        fs::write(
            &lock_path,
            json!({
                "schema_version": AUTOSYNC_SCHEMA_VERSION,
                "kind": "autosync_daemon",
                "pid": 0,
                "started_unix_seconds": 0,
                "repogrammar_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string(),
        )
        .expect("write stale lock");

        let stopped = stop_autosync(request(&workspace)).expect("stop");
        assert!(!stopped.running);
        assert!(!lock_path.exists());
    }

    #[test]
    fn acquire_daemon_writes_complete_lock_and_removes_own_lock() {
        let workspace = TempWorkspace::new("autosync-acquire-lock");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");

        let (guard, _settings, _root) =
            acquire_autosync_daemon(request(&workspace)).expect("acquire daemon");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        let contents = fs::read_to_string(&lock_path).expect("read daemon lock");
        let value: Value = serde_json::from_str(&contents).expect("daemon lock JSON");
        assert_eq!(value["kind"], "autosync_daemon");
        assert_eq!(value["pid"], std::process::id());
        assert!(!workspace
            .path()
            .join(".repogrammar/locks")
            .read_dir()
            .expect("read locks dir")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")));

        drop(guard);
        assert!(!lock_path.exists());
    }

    #[test]
    fn acquire_daemon_replaces_stale_lock_by_content_match() {
        let workspace = TempWorkspace::new("autosync-acquire-stale-lock");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        fs::write(
            &lock_path,
            json!({
                "schema_version": AUTOSYNC_SCHEMA_VERSION,
                "kind": "autosync_daemon",
                "pid": 0,
                "started_unix_seconds": 0,
                "repogrammar_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string(),
        )
        .expect("write stale daemon lock");

        let (guard, _settings, _root) =
            acquire_autosync_daemon(request(&workspace)).expect("replace stale daemon lock");

        let contents = fs::read_to_string(&lock_path).expect("read replacement lock");
        let value: Value = serde_json::from_str(&contents).expect("daemon lock JSON");
        assert_eq!(value["pid"], std::process::id());

        drop(guard);
        assert!(!lock_path.exists());
    }

    #[test]
    fn lifecycle_guard_serializes_daemon_lock_mutations() {
        let workspace = TempWorkspace::new("autosync-lifecycle-serialization");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        let locks_dir = workspace.path().join(".repogrammar/locks");
        let lifecycle = acquire_daemon_lifecycle_guard(&locks_dir).expect("lifecycle owner");

        let error = acquire_autosync_daemon(request(&workspace))
            .expect_err("concurrent daemon lock mutation must be refused");

        assert_eq!(
            error,
            RepoGrammarError::InvalidInput(
                "auto-sync lifecycle operation is already in progress".to_string()
            )
        );
        assert!(!locks_dir.join(DAEMON_LOCK_FILE).exists());
        drop(lifecycle);
        assert!(!locks_dir.join(DAEMON_LIFECYCLE_FILE).exists());

        let (daemon, _settings, _root) =
            acquire_autosync_daemon(request(&workspace)).expect("acquire after lifecycle release");
        drop(daemon);
    }

    #[test]
    fn lifecycle_guard_removes_only_its_exact_marker() {
        let workspace = TempWorkspace::new("autosync-lifecycle-marker-ownership");
        let locks_dir = workspace.path().join("locks");
        fs::create_dir_all(&locks_dir).expect("locks");
        let lifecycle = acquire_daemon_lifecycle_guard(&locks_dir).expect("lifecycle owner");
        let marker = locks_dir.join(DAEMON_LIFECYCLE_FILE);
        fs::write(&marker, "replacement-owner").expect("replace marker contents");

        drop(lifecycle);

        assert_eq!(
            fs::read_to_string(marker).expect("replacement marker preserved"),
            "replacement-owner"
        );
    }

    #[test]
    fn lifecycle_guard_recovers_a_confirmed_dead_owner() {
        let workspace = TempWorkspace::new("autosync-lifecycle-stale-owner");
        let locks_dir = workspace.path().join("locks");
        fs::create_dir_all(&locks_dir).expect("locks");
        let marker = locks_dir.join(DAEMON_LIFECYCLE_FILE);
        fs::write(
            &marker,
            json!({
                "schema_version": AUTOSYNC_SCHEMA_VERSION,
                "kind": "autosync_daemon_lifecycle",
                "pid": 0,
                "started_unix_seconds": 0,
                "token": "stale",
            })
            .to_string(),
        )
        .expect("stale lifecycle marker");

        let lifecycle = acquire_daemon_lifecycle_guard(&locks_dir).expect("recover stale owner");
        let current = fs::read_to_string(&marker).expect("new lifecycle marker");
        assert_ne!(
            serde_json::from_str::<Value>(&current).expect("marker JSON")["token"],
            "stale"
        );
        drop(lifecycle);
        assert!(!marker.exists());
    }

    #[test]
    fn remove_daemon_lock_preserves_concurrently_replaced_contents() {
        let workspace = TempWorkspace::new("autosync-lock-content-match");
        let lock_path = workspace.path().join("daemon.lock");
        fs::write(&lock_path, "new").expect("write daemon lock");

        let removed =
            remove_daemon_lock_if_contents_match(&lock_path, "old").expect("compare daemon lock");

        assert!(!removed);
        assert_eq!(
            fs::read_to_string(&lock_path).expect("read daemon lock"),
            "new"
        );
    }

    #[test]
    fn run_state_round_trips_ok_and_error() {
        let workspace = TempWorkspace::new("autosync-run-roundtrip");
        let dir = workspace.path();
        write_run_state(dir, AutosyncRunResult::Ok, Some("gen-000007"), None)
            .expect("write ok run state");
        let state = read_run_state(dir).expect("read run state");
        assert_eq!(state.result, AutosyncRunResult::Ok);
        assert_eq!(state.synced_generation.as_deref(), Some("gen-000007"));
        assert!(state.error.is_none());
        assert!(state.last_sync_unix_seconds > 0);

        write_run_state(
            dir,
            AutosyncRunResult::Error,
            None,
            Some(AUTOSYNC_RUN_ERROR_SYNC_FAILED),
        )
        .expect("write error run state");
        let state = read_run_state(dir).expect("read run state");
        assert_eq!(state.result, AutosyncRunResult::Error);
        assert_eq!(state.error.as_deref(), Some(AUTOSYNC_RUN_ERROR_SYNC_FAILED));
        assert!(state.synced_generation.is_none());
    }

    #[test]
    fn run_state_write_sanitizes_error_and_invalid_generation_before_persisting() {
        let workspace = TempWorkspace::new("autosync-run-write-sanitized");
        let sensitive = "/private/repository SECRET_SOURCE REPOGRAMMAR_TOKEN=credential-value";
        let invalid_generation = "gen-000007/../../private";

        write_run_state(
            workspace.path(),
            AutosyncRunResult::Error,
            Some(invalid_generation),
            Some(sensitive),
        )
        .expect("write sanitized run state");

        let raw = fs::read_to_string(run_state_path(workspace.path())).expect("read raw run state");
        assert!(!raw.contains(sensitive));
        assert!(!raw.contains(invalid_generation));
        let state = read_run_state(workspace.path()).expect("read sanitized run state");
        assert_eq!(state.synced_generation, None);
        assert_eq!(state.error.as_deref(), Some(AUTOSYNC_RUN_ERROR_UNKNOWN));
    }

    #[test]
    fn legacy_run_state_is_preserved_but_untrusted_fields_are_sanitized_on_read() {
        let workspace = TempWorkspace::new("autosync-run-legacy-sanitized");
        let sensitive = "/private/repository SECRET_SOURCE REPOGRAMMAR_TOKEN=credential-value";
        let invalid_generation = "gen-1000000/../../private";
        let raw = json!({
            "schema_version": AUTOSYNC_SCHEMA_VERSION,
            "last_sync_unix_seconds": 1_700_000_000_u64,
            "result": "error",
            "synced_generation": invalid_generation,
            "error": sensitive,
        })
        .to_string();
        fs::write(run_state_path(workspace.path()), &raw).expect("write legacy run state");

        let state = read_run_state(workspace.path()).expect("read legacy run state");

        assert_eq!(state.synced_generation, None);
        assert_eq!(state.error.as_deref(), Some(AUTOSYNC_RUN_ERROR_UNKNOWN));
        assert_eq!(state.display_synced_generation(), None);
        assert_eq!(state.display_error(), Some(AUTOSYNC_RUN_ERROR_UNKNOWN));
        assert_eq!(
            fs::read_to_string(run_state_path(workspace.path())).expect("legacy file preserved"),
            raw
        );

        let malformed = json!({
            "schema_version": AUTOSYNC_SCHEMA_VERSION,
            "last_sync_unix_seconds": 1_700_000_001_u64,
            "result": "error",
            "synced_generation": [invalid_generation],
            "error": {"raw": sensitive},
        })
        .to_string();
        fs::write(run_state_path(workspace.path()), &malformed)
            .expect("write malformed legacy run state");
        let malformed_state =
            read_run_state(workspace.path()).expect("read malformed legacy run state");
        assert_eq!(malformed_state.synced_generation, None);
        assert_eq!(
            malformed_state.error.as_deref(),
            Some(AUTOSYNC_RUN_ERROR_UNKNOWN)
        );
        assert_eq!(
            fs::read_to_string(run_state_path(workspace.path()))
                .expect("malformed legacy file preserved"),
            malformed
        );
    }

    #[test]
    fn autosync_run_error_allowlist_is_exact_and_result_aware() {
        for allowed in [
            AUTOSYNC_RUN_ERROR_FINGERPRINT_FAILED,
            AUTOSYNC_RUN_ERROR_STATE_UNAVAILABLE,
            AUTOSYNC_RUN_ERROR_SYNC_FAILED,
        ] {
            assert_eq!(
                sanitized_autosync_run_error(AutosyncRunResult::Error, Some(allowed)),
                Some(allowed)
            );
        }
        assert_eq!(
            sanitized_autosync_run_error(AutosyncRunResult::Error, Some("repository sync failed ")),
            Some(AUTOSYNC_RUN_ERROR_UNKNOWN)
        );
        assert_eq!(
            sanitized_autosync_run_error(AutosyncRunResult::Error, None),
            Some(AUTOSYNC_RUN_ERROR_UNKNOWN)
        );
        assert_eq!(
            sanitized_autosync_run_error(
                AutosyncRunResult::Ok,
                Some(AUTOSYNC_RUN_ERROR_SYNC_FAILED)
            ),
            None
        );
    }

    #[test]
    fn autosync_generation_ids_are_bounded_and_canonical() {
        for valid in ["gen-000001", "gen-999999"] {
            assert!(valid_autosync_generation_id(valid), "{valid}");
        }
        for invalid in [
            "gen-000000",
            "gen-0000001",
            "gen-1000000",
            "gen-01",
            "gen-100000000000000000000",
            "gen-999999x",
            "../gen-000001",
        ] {
            assert!(!valid_autosync_generation_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn read_run_state_is_none_when_absent() {
        let workspace = TempWorkspace::new("autosync-run-absent");
        assert!(read_run_state(workspace.path()).is_none());
    }

    #[test]
    fn enable_status_disable_autosync_config() {
        let workspace = TempWorkspace::new("autosync-enable-disable");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");

        let enabled = enable_autosync(
            request(&workspace),
            AutosyncSettings {
                poll_ms: 250,
                debounce_ms: 125,
            },
        )
        .expect("enable");
        assert!(enabled.enabled);
        assert!(!enabled.running);
        assert_eq!(enabled.poll_ms, 250);
        assert_eq!(enabled.debounce_ms, 125);

        let status = autosync_status(request(&workspace)).expect("status");
        assert!(status.enabled);
        assert!(!status.running);

        let disabled = disable_autosync(request(&workspace)).expect("disable");
        assert!(!disabled.enabled);
        assert!(!disabled.running);
    }

    #[test]
    fn disable_removes_stale_daemon_lock() {
        let workspace = TempWorkspace::new("autosync-disable-stale-lock");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");
        enable_autosync(request(&workspace), AutosyncSettings::default()).expect("enable");
        let lock_path = workspace.path().join(".repogrammar/locks/daemon.lock");
        fs::write(
            &lock_path,
            json!({
                "schema_version": AUTOSYNC_SCHEMA_VERSION,
                "kind": "autosync_daemon",
                "pid": 0,
                "started_unix_seconds": 0,
                "repogrammar_version": env!("CARGO_PKG_VERSION"),
            })
            .to_string(),
        )
        .expect("write stale lock");

        let disabled = disable_autosync(request(&workspace)).expect("disable");

        assert!(!disabled.enabled);
        assert!(!disabled.running);
        assert!(!lock_path.exists());
    }

    #[test]
    fn enable_rejects_invalid_poll_interval() {
        let workspace = TempWorkspace::new("autosync-invalid-poll");
        init_repository(RepositoryLifecycleInitRequest {
            path: workspace.path().display().to_string(),
            state_dir_override: None,
            write_root_gitignore: false,
        })
        .expect("init");

        let error = enable_autosync(
            request(&workspace),
            AutosyncSettings {
                poll_ms: 99,
                debounce_ms: 0,
            },
        )
        .expect_err("invalid poll");
        assert!(error.to_string().contains("--poll-ms"));
    }
}
