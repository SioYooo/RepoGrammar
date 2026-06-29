//! Repository-local auto-sync lifecycle state.

use crate::application::repository::{
    repository_state_location, repository_status, RepositoryStatus, RepositoryStatusRequest,
};
use crate::error::RepoGrammarError;
use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const AUTOSYNC_CONFIG_FILE: &str = "autosync.json";
const DAEMON_LOCK_FILE: &str = "daemon.lock";
const AUTOSYNC_RUN_FILE: &str = "autosync-run.json";
const AUTOSYNC_SCHEMA_VERSION: u64 = 1;

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
    let lock = inspect_daemon_lock(&state.state_dir)?;
    if lock.running {
        return Err(invalid_input(
            "auto-sync is running; run repogrammar autosync stop before disable",
        ));
    }
    if lock.pid.is_some() {
        let _ = fs::remove_file(state.state_dir.join("locks").join(DAEMON_LOCK_FILE));
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
    let lock_path = locks_dir.join(DAEMON_LOCK_FILE);
    let lock = inspect_daemon_lock(&state.state_dir)?;
    if lock.running {
        return Err(invalid_input("auto-sync is already running"));
    }
    if lock.pid.is_some() {
        let _ = fs::remove_file(&lock_path);
    }
    let contents = daemon_lock_contents();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|_| invalid_input("failed to create auto-sync daemon lock"))?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        let _ = fs::remove_file(&lock_path);
        return Err(invalid_input(format!(
            "failed to write auto-sync daemon lock: {error}"
        )));
    }
    Ok((
        AutosyncDaemonGuard {
            path: lock_path,
            contents,
        },
        config.settings,
        state.root,
    ))
}

pub fn stop_autosync(request: AutosyncRequest) -> Result<AutosyncReport, RepoGrammarError> {
    let state = require_initialized_state(&request)?;
    let lock = inspect_daemon_lock(&state.state_dir)?;
    let Some(pid) = lock.pid else {
        return autosync_status_for_state(&state, "auto-sync is not running");
    };
    if lock.running {
        terminate_process(pid)?;
    }
    let _ = fs::remove_file(state.state_dir.join("locks").join(DAEMON_LOCK_FILE));
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
    let text = match read_limited_text(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DaemonLockStatus {
                running: false,
                pid: None,
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
    Ok(DaemonLockStatus {
        running: process_is_running(pid),
        pid: Some(pid),
    })
}

fn daemon_lock_contents() -> String {
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
    })
    .to_string()
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

fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}

fn terminate_process(pid: u32) -> Result<(), RepoGrammarError> {
    #[cfg(unix)]
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    #[cfg(windows)]
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .status();
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

fn invalid_input(message: impl Into<String>) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::repository::{init_repository, RepositoryLifecycleInitRequest};
    use crate::test_support::TempWorkspace;

    fn request(workspace: &TempWorkspace) -> AutosyncRequest {
        AutosyncRequest::new(workspace.path().display().to_string())
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
