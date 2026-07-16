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
const AUTOSYNC_SCHEMA_VERSION: u64 = 1;
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
    pub pid: Option<u32>,
    pub poll_ms: u64,
    pub debounce_ms: u64,
    pub last_run: Option<AutosyncRunReport>,
    pub message: String,
}

/// Typed result of the parent's bounded daemon-startup probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutosyncStartupReadiness {
    Pending,
    Ready(AutosyncReport),
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

#[derive(Debug)]
pub struct AutosyncDaemonGuard {
    path: PathBuf,
    contents: String,
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
    if lock.status.running {
        return Err(invalid_input(
            "auto-sync is running; run repogrammar autosync stop before disable",
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
        return Ok(AutosyncStartupReadiness::Pending);
    };
    let owns_expected_lock = inspection.status.pid == Some(expected_pid)
        && inspection.status.startup_nonce.as_deref() == Some(expected_nonce);
    if owns_expected_lock {
        let config = read_config(&state.state_dir)?;
        return Ok(AutosyncStartupReadiness::Ready(AutosyncReport {
            state_dir: state.state_dir_relative,
            enabled: config.enabled,
            running: true,
            pid: Some(expected_pid),
            poll_ms: config.settings.poll_ms,
            debounce_ms: config.settings.debounce_ms,
            last_run: read_run_state(&state.state_dir),
            message: "auto-sync started".to_string(),
        }));
    }
    if !inspection.status.running {
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
    let contents = daemon_lock_contents(startup_nonce.as_deref());

    for _attempt in 0..2 {
        match create_daemon_lock_atomically(&lock_path, &contents)? {
            CreateDaemonLockResult::Acquired => {
                return Ok((
                    AutosyncDaemonGuard {
                        path: lock_path,
                        contents,
                    },
                    config.settings,
                    state.root,
                ));
            }
            CreateDaemonLockResult::AlreadyExists => {
                let inspection = inspect_daemon_lock_path(&lock_path)?;
                if inspection.status.running {
                    return Err(invalid_input("auto-sync is already running"));
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
    if lock.status.running {
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
                Some(_) if !current.status.running => {
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
    } else if let Some(contents) = lock.contents {
        let _ = remove_daemon_lock_if_contents_match(&lock_path, &contents)?;
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
    pid: Option<u32>,
    startup_nonce: Option<String>,
}

struct DaemonLockInspection {
    status: DaemonLockStatus,
    contents: Option<String>,
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
        pid: lock.pid.filter(|_| lock.running),
        poll_ms: config.settings.poll_ms,
        debounce_ms: config.settings.debounce_ms,
        last_run: read_run_state(&state.state_dir),
        message: message.to_string(),
    })
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
        .map(str::to_string);
    let error = value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(AutosyncRunReport {
        last_sync_unix_seconds,
        result,
        synced_generation,
        error,
    })
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
                    pid: None,
                    startup_nonce: None,
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
    Ok(DaemonLockInspection {
        status: DaemonLockStatus {
            // The PID alone is not proof of liveness: after an unclean daemon
            // exit the OS can reuse it for an unrelated process. Require both
            // that the PID exists and that it is actually a RepoGrammar autosync
            // daemon, so `stop` never signals a stranger and `start` is never
            // permanently blocked by a reused PID.
            running: autosync_daemon_process_liveness(pid, started_unix_seconds)
                == ProcessLiveness::Live,
            pid: Some(pid),
            startup_nonce,
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
    if let Err(error) = writer.write_all(contents.as_bytes()) {
        let _ = fs::remove_file(lock_path);
        return Err(invalid_input(format!(
            "failed to write auto-sync daemon lock: {error}"
        )));
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

fn daemon_lock_contents(startup_nonce: Option<&str>) -> String {
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
    fn daemon_lock_records_only_valid_startup_nonces() {
        let valid = daemon_lock_contents(Some("0123456789abcdef"));
        let value: Value = serde_json::from_str(&valid).expect("valid lock JSON");
        assert_eq!(value["startup_nonce"], "0123456789abcdef");
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

        write_run_state(dir, AutosyncRunResult::Error, None, Some("boom"))
            .expect("write error run state");
        let state = read_run_state(dir).expect("read run state");
        assert_eq!(state.result, AutosyncRunResult::Error);
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(state.synced_generation.is_none());
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
