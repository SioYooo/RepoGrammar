//! Repository-level initialization, indexing, status, and generation policy.

use crate::application::progress::{initialization_stages, ProgressStage};
use crate::error::RepoGrammarError;
use crate::ports::index_store::{IndexStore, StorageInspection};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_STATE_DIR: &str = ".repogrammar";

const STATE_DIR_OVERRIDE_PREFIX: &str = ".repogrammar-";
const REQUIRED_STATE_SUBDIRS: [&str; 6] =
    ["generations", "cache", "logs", "locks", "tmp", "receipts"];
const STATE_GITIGNORE: &str = "# RepoGrammar local generated state.\n\
# This directory contains repository-local indexes, logs, caches, locks,\n\
# telemetry rollups, and temporary files. Do not commit it.\n\
\n\
*\n\
!.gitignore\n";
const GIT_INFO_EXCLUDE_PATTERNS: [&str; 2] = [".repogrammar/", ".repogrammar-*/"];
const INDEX_LOCK_FILE: &str = "index.lock";
static INDEX_LOCK_TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const ROOT_GITIGNORE_BEGIN: &str = "# BEGIN RepoGrammar local state";
const ROOT_GITIGNORE_END: &str = "# END RepoGrammar local state";
const ROOT_GITIGNORE_SECTION: &str = "# BEGIN RepoGrammar local state\n\
.repogrammar/\n\
.repogrammar-*/\n\
# END RepoGrammar local state\n";
const LIFECYCLE_TEXT_MAX_BYTES: u64 = 1024 * 1024;
const BOOTSTRAP_MANIFEST_SCHEMA_VERSION: u32 = 1;
const BOOTSTRAP_STORAGE_STATUSES: &[&str] = &["not_implemented"];
const BOOTSTRAP_INDEXING_STATUSES: &[&str] = &["not_implemented"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInitRequest {
    pub path: String,
    pub progress_json: bool,
    pub quiet: bool,
    pub verbose: bool,
}

impl RepositoryInitRequest {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            progress_json: false,
            quiet: false,
            verbose: false,
        }
    }
}

pub trait RepositoryInitApplicationRequest {
    fn path(&self) -> &str;

    fn state_dir_override(&self) -> Option<&str> {
        None
    }

    fn write_root_gitignore(&self) -> bool {
        false
    }
}

impl RepositoryInitApplicationRequest for RepositoryInitRequest {
    fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryLifecycleInitRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
    pub write_root_gitignore: bool,
}

impl RepositoryLifecycleInitRequest {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            state_dir_override: None,
            write_root_gitignore: false,
        }
    }
}

impl RepositoryInitApplicationRequest for RepositoryLifecycleInitRequest {
    fn path(&self) -> &str {
        &self.path
    }

    fn state_dir_override(&self) -> Option<&str> {
        self.state_dir_override.as_deref()
    }

    fn write_root_gitignore(&self) -> bool {
        self.write_root_gitignore
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStatusRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
}

impl RepositoryStatusRequest {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            state_dir_override: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStateLocation {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    pub state_dir_relative: String,
}

#[must_use]
#[derive(Debug)]
pub struct IndexLockGuard {
    path: PathBuf,
    contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryDoctorRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
}

impl RepositoryDoctorRequest {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            state_dir_override: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUninitRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
    pub yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUnlockRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
    pub force: bool,
    pub yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryLogsRequest {
    pub path: String,
    pub state_dir_override: Option<String>,
    pub component: Option<String>,
    pub tail: Option<usize>,
    pub since: Option<String>,
    pub redact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInitOutcome {
    pub state_dir: String,
    pub created: bool,
    pub repaired_entries: Vec<String>,
    pub git_info_exclude_updated: bool,
    pub root_gitignore_updated: bool,
    pub storage: RepositoryImplementationStatus,
    pub indexing: RepositoryImplementationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexGenerationPolicy {
    pub build_new_generation: bool,
    pub atomically_activate_after_validation: bool,
    pub preserve_previous_valid_index_on_failure: bool,
}

impl Default for IndexGenerationPolicy {
    fn default() -> Self {
        Self {
            build_new_generation: true,
            atomically_activate_after_validation: true,
            preserve_previous_valid_index_on_failure: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryStatus {
    NotInitialized,
    Initialized { active_generation: String },
    CorruptedManifest,
}

impl RepositoryStatus {
    pub fn as_human_message(&self) -> &'static str {
        match self {
            Self::NotInitialized => "RepoGrammar repository status: not initialized",
            Self::Initialized { .. } => "RepoGrammar repository status: initialized",
            Self::CorruptedManifest => "RepoGrammar repository status: corrupted manifest",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryManifestStatus {
    Missing,
    Valid,
    Corrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryImplementationStatus {
    NotImplemented,
    Available,
    FileManifestOnly,
    SyntaxOnlyCodeUnits,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStatusReport {
    pub state_dir: String,
    pub status: RepositoryStatus,
    pub manifest: RepositoryManifestStatus,
    pub manifest_schema_version: Option<u32>,
    pub missing_subdirs: Vec<String>,
    pub storage: RepositoryImplementationStatus,
    pub indexing: RepositoryImplementationStatus,
    pub storage_inspection: Option<StorageInspection>,
    pub storage_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryDoctorSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryDoctorCode {
    NotInitialized,
    CorruptedManifest,
    MissingSubdir,
    StateGitignoreMissing,
    StateGitignoreInvalid,
    GitInfoExcludeMissing,
    GitInfoExcludeIncomplete,
    RootGitignoreMarkerInvalid,
    InitReceiptMissing,
    InitReceiptInvalid,
    IndexLockActive,
    IndexLockStale,
    IndexLockUnknown,
    IndexLockInvalid,
    StorageNotImplemented,
    StorageReady,
    StorageInvalid,
    StorageNoActiveGeneration,
    IndexingNotImplemented,
    IndexingFileManifestOnly,
    IndexingSyntaxOnlyCodeUnits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryDoctorFinding {
    pub severity: RepositoryDoctorSeverity,
    pub code: RepositoryDoctorCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryDoctorReport {
    pub status: RepositoryStatusReport,
    pub findings: Vec<RepositoryDoctorFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUninitOutcome {
    pub state_dir: String,
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUnlockReport {
    pub state_dir: String,
    pub removed_locks: usize,
    pub inspected_locks: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryLogsReport {
    pub state_dir: String,
    pub available: bool,
    pub redacted: bool,
    pub entries: Vec<String>,
    pub message: String,
}

pub fn required_initialization_stages() -> Vec<ProgressStage> {
    initialization_stages()
}

pub fn init_repository(
    request: impl RepositoryInitApplicationRequest,
) -> Result<RepositoryInitOutcome, RepoGrammarError> {
    let resolved = resolve_state_dir(request.path(), request.state_dir_override())?;
    let created = !resolved.absolute.exists();
    ensure_state_path_can_be_directory(&resolved.absolute)?;
    fs::create_dir_all(&resolved.absolute).map_err(|_| {
        invalid_input("failed to create repository-local RepoGrammar state directory")
    })?;

    let mut repaired_entries = Vec::new();
    ensure_generated_file(
        &resolved.absolute.join(".gitignore"),
        STATE_GITIGNORE,
        ".gitignore",
        &mut repaired_entries,
    )?;
    ensure_manifest(
        &resolved.absolute.join("manifest.json"),
        &mut repaired_entries,
    )?;

    for subdir in REQUIRED_STATE_SUBDIRS {
        ensure_generated_dir(
            &resolved.absolute.join(subdir),
            subdir,
            &mut repaired_entries,
        )?;
    }
    ensure_generated_file(
        &resolved.absolute.join("receipts").join("init.json"),
        &init_receipt_contents(),
        "receipts/init.json",
        &mut repaired_entries,
    )?;

    let git_info_exclude_updated = ensure_git_info_exclude(&resolved.root)?;
    let root_gitignore_updated = if request.write_root_gitignore() {
        ensure_root_gitignore_marker(&resolved.root)?
    } else {
        false
    };

    Ok(RepositoryInitOutcome {
        state_dir: resolved.relative,
        created,
        repaired_entries,
        git_info_exclude_updated,
        root_gitignore_updated,
        storage: RepositoryImplementationStatus::NotImplemented,
        indexing: RepositoryImplementationStatus::NotImplemented,
    })
}

pub fn repository_status(
    request: RepositoryStatusRequest,
) -> Result<RepositoryStatusReport, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    status_for_resolved_state(&resolved, None)
}

pub fn repository_state_location(
    request: RepositoryStatusRequest,
) -> Result<RepositoryStateLocation, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    Ok(RepositoryStateLocation {
        root: resolved.root,
        state_dir: resolved.absolute,
        state_dir_relative: resolved.relative,
    })
}

pub fn acquire_index_lock(
    repository_root: &str,
    state_dir_override: Option<&str>,
) -> Result<IndexLockGuard, RepoGrammarError> {
    let resolved = resolve_state_dir(repository_root, state_dir_override)?;
    if !resolved.absolute.exists() {
        return Err(invalid_input(
            "repository is not initialized; run repogrammar init",
        ));
    }
    ensure_state_path_can_be_directory(&resolved.absolute)?;
    let locks_dir = require_locks_dir(&resolved.absolute)?;
    let lock_path = locks_dir.join(INDEX_LOCK_FILE);
    let contents = current_index_lock_metadata().to_json();

    for _attempt in 0..2 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                file.write_all(contents.as_bytes())
                    .map_err(|_| invalid_input("failed to write repository-local index lock"))?;
                return Ok(IndexLockGuard {
                    path: lock_path,
                    contents,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let inspection = inspect_index_lock_path_with_contents(&lock_path);
                match inspection.state {
                    IndexLockState::Stale => {
                        let Some(stale_contents) = inspection.contents else {
                            return Err(invalid_input(
                                "index lock metadata is invalid; run repogrammar doctor",
                            ));
                        };
                        if !remove_index_lock_if_contents_match(&lock_path, &stale_contents)? {
                            continue;
                        }
                        continue;
                    }
                    IndexLockState::Active => {
                        return Err(invalid_input(
                            "index lock is held by another RepoGrammar indexing process; run repogrammar doctor",
                        ));
                    }
                    IndexLockState::Unknown => {
                        return Err(invalid_input(
                            "index lock ownership is unknown; run repogrammar doctor",
                        ));
                    }
                    IndexLockState::Invalid => {
                        return Err(invalid_input(
                            "index lock metadata is invalid; run repogrammar doctor",
                        ));
                    }
                    IndexLockState::Missing => continue,
                }
            }
            Err(_) => {
                return Err(invalid_input(
                    "failed to create repository-local index lock",
                ));
            }
        }
    }

    Err(invalid_input(
        "index lock changed during acquisition; retry repogrammar index",
    ))
}

pub fn repository_status_with_storage(
    request: RepositoryStatusRequest,
    store: &impl IndexStore,
) -> Result<RepositoryStatusReport, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    status_for_resolved_state(&resolved, Some(store))
}

pub fn repository_doctor(
    request: RepositoryDoctorRequest,
) -> Result<RepositoryDoctorReport, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    let status = status_for_resolved_state(&resolved, None)?;
    let mut findings = doctor_findings_for_status(&status);
    findings.extend(lifecycle_hygiene_findings(&resolved)?);
    Ok(RepositoryDoctorReport { findings, status })
}

pub fn repository_doctor_with_storage(
    request: RepositoryDoctorRequest,
    store: &impl IndexStore,
) -> Result<RepositoryDoctorReport, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    let status = status_for_resolved_state(&resolved, Some(store))?;
    let mut findings = doctor_findings_for_status(&status);
    findings.extend(lifecycle_hygiene_findings(&resolved)?);
    Ok(RepositoryDoctorReport { findings, status })
}

fn doctor_findings_for_status(status: &RepositoryStatusReport) -> Vec<RepositoryDoctorFinding> {
    let mut findings = Vec::new();

    match status.status {
        RepositoryStatus::NotInitialized => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Warning,
            code: RepositoryDoctorCode::NotInitialized,
            detail: "repository-local RepoGrammar state is not initialized".to_string(),
        }),
        RepositoryStatus::CorruptedManifest => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Error,
            code: RepositoryDoctorCode::CorruptedManifest,
            detail: "manifest.json is present but does not match the bootstrap manifest shape"
                .to_string(),
        }),
        RepositoryStatus::Initialized { .. } => {}
    }

    for subdir in &status.missing_subdirs {
        findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Error,
            code: RepositoryDoctorCode::MissingSubdir,
            detail: format!("required repository-local state subdirectory is missing: {subdir}"),
        });
    }

    match status.storage {
        RepositoryImplementationStatus::Available => {
            if status
                .storage_inspection
                .as_ref()
                .and_then(|inspection| inspection.active_generation.as_ref())
                .is_some()
            {
                findings.push(RepositoryDoctorFinding {
                    severity: RepositoryDoctorSeverity::Info,
                    code: RepositoryDoctorCode::StorageReady,
                    detail: "SQLite storage is readable and has an active generation".to_string(),
                });
            } else {
                findings.push(RepositoryDoctorFinding {
                    severity: RepositoryDoctorSeverity::Warning,
                    code: RepositoryDoctorCode::StorageNoActiveGeneration,
                    detail: "SQLite storage is available but no generation is active".to_string(),
                });
            }
        }
        RepositoryImplementationStatus::Unhealthy => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Error,
            code: RepositoryDoctorCode::StorageInvalid,
            detail: format!(
                "SQLite storage health check failed: {}",
                status.storage_error.as_deref().unwrap_or("unknown")
            ),
        }),
        RepositoryImplementationStatus::NotImplemented
        | RepositoryImplementationStatus::FileManifestOnly
        | RepositoryImplementationStatus::SyntaxOnlyCodeUnits => {
            findings.push(RepositoryDoctorFinding {
                severity: RepositoryDoctorSeverity::Info,
                code: RepositoryDoctorCode::StorageNotImplemented,
                detail: "SQLite storage is not wired for this command".to_string(),
            });
        }
    }

    match status.indexing {
        RepositoryImplementationStatus::FileManifestOnly => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Info,
            code: RepositoryDoctorCode::IndexingFileManifestOnly,
            detail: "file discovery metadata is stored; parser, code-unit extraction, and mining remain deferred".to_string(),
        }),
        RepositoryImplementationStatus::SyntaxOnlyCodeUnits => {
            findings.push(RepositoryDoctorFinding {
                severity: RepositoryDoctorSeverity::Warning,
                code: RepositoryDoctorCode::IndexingSyntaxOnlyCodeUnits,
                detail: "syntax-only code units are stored; semantic worker, mining, queries, and pattern-family evidence remain deferred".to_string(),
            })
        }
        _ => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Info,
            code: RepositoryDoctorCode::IndexingNotImplemented,
            detail: "repository indexing has not produced code units or pattern families yet"
                .to_string(),
        }),
    }

    findings
}

fn lifecycle_hygiene_findings(
    resolved: &ResolvedStateDir,
) -> Result<Vec<RepositoryDoctorFinding>, RepoGrammarError> {
    if !resolved.absolute.exists() {
        return Ok(Vec::new());
    }

    let mut findings = Vec::new();
    inspect_state_gitignore(&resolved.absolute, &mut findings)?;
    inspect_init_receipt(&resolved.absolute, &mut findings)?;
    inspect_index_lock(&resolved.absolute, &mut findings);
    inspect_git_info_exclude(&resolved.root, &mut findings)?;
    inspect_root_gitignore_marker(&resolved.root, &mut findings)?;
    Ok(findings)
}

fn inspect_index_lock(state_dir: &Path, findings: &mut Vec<RepositoryDoctorFinding>) {
    let lock_path = state_dir.join("locks").join(INDEX_LOCK_FILE);
    match inspect_index_lock_path(&lock_path) {
        IndexLockState::Missing => {}
        IndexLockState::Active => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Warning,
            code: RepositoryDoctorCode::IndexLockActive,
            detail: "index.lock is held by a live RepoGrammar indexing process".to_string(),
        }),
        IndexLockState::Stale => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Warning,
            code: RepositoryDoctorCode::IndexLockStale,
            detail: "index.lock is stale and may be replaced by the next index or sync run"
                .to_string(),
        }),
        IndexLockState::Unknown => findings.push(RepositoryDoctorFinding {
            severity: RepositoryDoctorSeverity::Warning,
            code: RepositoryDoctorCode::IndexLockUnknown,
            detail: "index.lock ownership cannot be confirmed on this host".to_string(),
        }),
        IndexLockState::Invalid => findings.push(doctor_error(
            RepositoryDoctorCode::IndexLockInvalid,
            "index.lock metadata is malformed or unreadable",
        )),
    }
}

fn inspect_state_gitignore(
    state_dir: &Path,
    findings: &mut Vec<RepositoryDoctorFinding>,
) -> Result<(), RepoGrammarError> {
    let path = state_dir.join(".gitignore");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            findings.push(doctor_error(
                RepositoryDoctorCode::StateGitignoreInvalid,
                ".repogrammar/.gitignore is not a regular file",
            ));
        }
        Ok(_) => {
            if read_lifecycle_text(&path).as_deref() != Ok(STATE_GITIGNORE) {
                findings.push(doctor_error(
                    RepositoryDoctorCode::StateGitignoreInvalid,
                    ".repogrammar/.gitignore does not match the required generated-state ignore policy",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            findings.push(doctor_error(
                RepositoryDoctorCode::StateGitignoreMissing,
                ".repogrammar/.gitignore is missing",
            ));
        }
        Err(_) => {
            findings.push(doctor_error(
                RepositoryDoctorCode::StateGitignoreInvalid,
                ".repogrammar/.gitignore could not be inspected",
            ));
        }
    }
    Ok(())
}

fn inspect_init_receipt(
    state_dir: &Path,
    findings: &mut Vec<RepositoryDoctorFinding>,
) -> Result<(), RepoGrammarError> {
    let path = state_dir.join("receipts").join("init.json");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            findings.push(doctor_error(
                RepositoryDoctorCode::InitReceiptInvalid,
                "receipts/init.json is not a regular file",
            ));
        }
        Ok(_) => {
            let valid = read_lifecycle_text(&path)
                .map(|contents| is_valid_init_receipt(&contents))
                .unwrap_or(false);
            if !valid {
                findings.push(doctor_error(
                    RepositoryDoctorCode::InitReceiptInvalid,
                    "receipts/init.json does not match the expected init receipt shape",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            findings.push(doctor_error(
                RepositoryDoctorCode::InitReceiptMissing,
                "receipts/init.json is missing",
            ));
        }
        Err(_) => {
            findings.push(doctor_error(
                RepositoryDoctorCode::InitReceiptInvalid,
                "receipts/init.json could not be inspected",
            ));
        }
    }
    Ok(())
}

fn inspect_git_info_exclude(
    root: &Path,
    findings: &mut Vec<RepositoryDoctorFinding>,
) -> Result<(), RepoGrammarError> {
    let Some(git_dir) = resolve_git_dir(root)? else {
        return Ok(());
    };
    let exclude = git_dir.join("info").join("exclude");
    match fs::symlink_metadata(&exclude) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            findings.push(doctor_error(
                RepositoryDoctorCode::GitInfoExcludeIncomplete,
                ".git/info/exclude is not a regular file",
            ));
        }
        Ok(_) => match read_lifecycle_text(&exclude) {
            Ok(contents) => {
                let lines = contents.lines().collect::<Vec<_>>();
                let missing = GIT_INFO_EXCLUDE_PATTERNS
                    .iter()
                    .any(|pattern| !lines.iter().any(|line| line == pattern));
                if missing {
                    findings.push(doctor_error(
                        RepositoryDoctorCode::GitInfoExcludeIncomplete,
                        ".git/info/exclude is missing RepoGrammar state patterns",
                    ));
                }
            }
            Err(_) => {
                findings.push(doctor_error(
                    RepositoryDoctorCode::GitInfoExcludeIncomplete,
                    ".git/info/exclude could not be read as bounded UTF-8 text",
                ));
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            findings.push(doctor_error(
                RepositoryDoctorCode::GitInfoExcludeMissing,
                ".git/info/exclude is missing",
            ));
        }
        Err(_) => {
            findings.push(doctor_error(
                RepositoryDoctorCode::GitInfoExcludeIncomplete,
                ".git/info/exclude could not be inspected",
            ));
        }
    }
    Ok(())
}

fn inspect_root_gitignore_marker(
    root: &Path,
    findings: &mut Vec<RepositoryDoctorFinding>,
) -> Result<(), RepoGrammarError> {
    let path = root.join(".gitignore");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            findings.push(doctor_error(
                RepositoryDoctorCode::RootGitignoreMarkerInvalid,
                "root .gitignore is not a regular file",
            ));
        }
        Ok(_) => match read_lifecycle_text(&path) {
            Ok(contents) => {
                if root_gitignore_marker_is_invalid(&contents) {
                    findings.push(doctor_error(
                        RepositoryDoctorCode::RootGitignoreMarkerInvalid,
                        "root .gitignore has an invalid RepoGrammar marker section",
                    ));
                }
            }
            Err(_) => {
                findings.push(doctor_error(
                    RepositoryDoctorCode::RootGitignoreMarkerInvalid,
                    "root .gitignore could not be read as bounded UTF-8 text",
                ));
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            findings.push(doctor_error(
                RepositoryDoctorCode::RootGitignoreMarkerInvalid,
                "root .gitignore could not be inspected",
            ));
        }
    }
    Ok(())
}

fn doctor_error(code: RepositoryDoctorCode, detail: impl Into<String>) -> RepositoryDoctorFinding {
    RepositoryDoctorFinding {
        severity: RepositoryDoctorSeverity::Error,
        code,
        detail: detail.into(),
    }
}

fn read_lifecycle_text(path: &Path) -> Result<String, ()> {
    let mut file = fs::File::open(path).map_err(|_| ())?;
    let mut buffer = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(LIFECYCLE_TEXT_MAX_BYTES + 1)
        .read_to_end(&mut buffer)
        .map_err(|_| ())?;
    if buffer.len() as u64 > LIFECYCLE_TEXT_MAX_BYTES {
        return Err(());
    }
    String::from_utf8(buffer).map_err(|_| ())
}

fn require_locks_dir(state_dir: &Path) -> Result<PathBuf, RepoGrammarError> {
    let locks_dir = state_dir.join("locks");
    match fs::symlink_metadata(&locks_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(
            invalid_input("repository-local locks path is not a directory; run repogrammar doctor"),
        ),
        Ok(_) => Ok(locks_dir),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(invalid_input(
            "repository-local state is missing locks directory; run repogrammar doctor",
        )),
        Err(_) => Err(invalid_input(
            "failed to inspect repository-local locks directory",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexLockState {
    Missing,
    Active,
    Stale,
    Unknown,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexLockMetadata {
    pid: u32,
    host: Option<String>,
    os: String,
    started_unix_seconds: u64,
    repogrammar_version: String,
    token: String,
}

impl IndexLockMetadata {
    fn to_json(&self) -> String {
        let value = serde_json::json!({
            "kind": "index",
            "pid": self.pid,
            "host": self.host,
            "os": self.os,
            "started_unix_seconds": self.started_unix_seconds,
            "repogrammar_version": self.repogrammar_version,
            "token": self.token,
        });
        format!("{value}\n")
    }

    fn parse(contents: &str) -> Option<Self> {
        let value = serde_json::from_str::<serde_json::Value>(contents).ok()?;
        let object = value.as_object()?;
        if object.get("kind").and_then(serde_json::Value::as_str) != Some("index") {
            return None;
        }
        let pid = object.get("pid")?.as_u64()?;
        let pid = u32::try_from(pid).ok()?;
        let host = match object.get("host") {
            Some(value) if value.is_null() => None,
            Some(value) => {
                let host = value.as_str()?.trim();
                if host.is_empty() || !lock_text_field_is_safe(host) {
                    return None;
                }
                Some(host.to_string())
            }
            None => None,
        };
        let os = object.get("os")?.as_str()?.trim();
        let repogrammar_version = object.get("repogrammar_version")?.as_str()?.trim();
        let token = object.get("token")?.as_str()?.trim();
        if os.is_empty()
            || repogrammar_version.is_empty()
            || token.is_empty()
            || !lock_text_field_is_safe(os)
            || !lock_text_field_is_safe(repogrammar_version)
            || !lock_text_field_is_safe(token)
        {
            return None;
        }
        Some(Self {
            pid,
            host,
            os: os.to_string(),
            started_unix_seconds: object.get("started_unix_seconds")?.as_u64()?,
            repogrammar_version: repogrammar_version.to_string(),
            token: token.to_string(),
        })
    }
}

impl Drop for IndexLockGuard {
    fn drop(&mut self) {
        if read_lifecycle_text(&self.path)
            .map(|current| current == self.contents)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn current_index_lock_metadata() -> IndexLockMetadata {
    let pid = std::process::id();
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let sequence = INDEX_LOCK_TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    IndexLockMetadata {
        pid,
        host: current_host(),
        os: std::env::consts::OS.to_string(),
        started_unix_seconds: duration.as_secs(),
        repogrammar_version: env!("CARGO_PKG_VERSION").to_string(),
        token: format!("{pid}-{}-{sequence}", duration.as_nanos()),
    }
}

fn inspect_index_lock_path(path: &Path) -> IndexLockState {
    inspect_index_lock_path_with_contents(path).state
}

struct IndexLockInspection {
    state: IndexLockState,
    contents: Option<String>,
}

fn inspect_index_lock_path_with_contents(path: &Path) -> IndexLockInspection {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            IndexLockInspection {
                state: IndexLockState::Invalid,
                contents: None,
            }
        }
        Ok(_) => {
            let Ok(contents) = read_lifecycle_text(path) else {
                return IndexLockInspection {
                    state: IndexLockState::Invalid,
                    contents: None,
                };
            };
            let Some(lock) = IndexLockMetadata::parse(&contents) else {
                return IndexLockInspection {
                    state: IndexLockState::Invalid,
                    contents: Some(contents),
                };
            };
            IndexLockInspection {
                state: classify_index_lock(&lock),
                contents: Some(contents),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => IndexLockInspection {
            state: IndexLockState::Missing,
            contents: None,
        },
        Err(_) => IndexLockInspection {
            state: IndexLockState::Invalid,
            contents: None,
        },
    }
}

fn remove_index_lock_if_contents_match(
    path: &Path,
    expected_contents: &str,
) -> Result<bool, RepoGrammarError> {
    match read_lifecycle_text(path) {
        Ok(current) if current == expected_contents => match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(invalid_input(
                "failed to remove stale repository-local index lock",
            )),
        },
        Ok(_) => Ok(false),
        Err(()) => Ok(false),
    }
}

fn classify_index_lock(lock: &IndexLockMetadata) -> IndexLockState {
    if lock.os != std::env::consts::OS {
        return IndexLockState::Unknown;
    }
    let current_host = current_host();
    if lock.host != current_host {
        return IndexLockState::Unknown;
    }
    if current_host.is_none() && lock.pid != std::process::id() && lock.pid != 0 {
        return IndexLockState::Unknown;
    }
    match process_state(lock.pid) {
        LockProcessState::Live => IndexLockState::Active,
        LockProcessState::Dead => IndexLockState::Stale,
        LockProcessState::Unknown => IndexLockState::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockProcessState {
    Live,
    Dead,
    Unknown,
}

#[cfg(unix)]
fn process_state(pid: u32) -> LockProcessState {
    if pid == std::process::id() {
        return LockProcessState::Live;
    }
    if pid == 0 {
        return LockProcessState::Dead;
    }
    match std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) if output.status.success() => LockProcessState::Live,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Operation not permitted") || stderr.contains("not permitted") {
                LockProcessState::Unknown
            } else {
                LockProcessState::Dead
            }
        }
        Err(_) => LockProcessState::Unknown,
    }
}

#[cfg(not(unix))]
fn process_state(pid: u32) -> LockProcessState {
    if pid == std::process::id() {
        LockProcessState::Live
    } else if pid == 0 {
        LockProcessState::Dead
    } else {
        LockProcessState::Unknown
    }
}

fn current_host() -> Option<String> {
    ["HOSTNAME", "COMPUTERNAME"].iter().find_map(|key| {
        std::env::var(key).ok().and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty() && lock_text_field_is_safe(trimmed)).then(|| trimmed.to_string())
        })
    })
}

fn lock_text_field_is_safe(value: &str) -> bool {
    value.len() <= 255
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
}

pub fn uninit_repository(
    request: RepositoryUninitRequest,
) -> Result<RepositoryUninitOutcome, RepoGrammarError> {
    if !request.yes {
        return Err(invalid_input(
            "repogrammar uninit requires explicit confirmation",
        ));
    }

    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    if !resolved.absolute.exists() {
        return Ok(RepositoryUninitOutcome {
            state_dir: resolved.relative,
            removed: false,
        });
    }
    ensure_state_path_can_be_directory(&resolved.absolute)?;
    fs::remove_dir_all(&resolved.absolute)
        .map_err(|_| invalid_input("failed to remove repository-local RepoGrammar state"))?;

    Ok(RepositoryUninitOutcome {
        state_dir: resolved.relative,
        removed: true,
    })
}

pub fn unlock_repository(
    request: RepositoryUnlockRequest,
) -> Result<RepositoryUnlockReport, RepoGrammarError> {
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    let status = status_for_resolved_state(&resolved, None)?;
    if matches!(status.status, RepositoryStatus::NotInitialized) {
        return Ok(RepositoryUnlockReport {
            state_dir: resolved.relative,
            removed_locks: 0,
            inspected_locks: Vec::new(),
            message: "unlock skipped: repository-local RepoGrammar state is not initialized"
                .to_string(),
        });
    }

    let mut inspected_locks = Vec::new();
    let locks_dir = resolved.absolute.join("locks");
    for lock_name in ["index.lock", "daemon.lock", "sqlite.lock"] {
        if locks_dir.join(lock_name).exists() {
            inspected_locks.push(lock_name.to_string());
        }
    }

    let message = if request.force && request.yes {
        "unlock refused: lock ownership validation is not implemented, so no locks were removed"
    } else {
        "unlock is inspection-only until stale-lock validation is implemented"
    };

    Ok(RepositoryUnlockReport {
        state_dir: resolved.relative,
        removed_locks: 0,
        inspected_locks,
        message: message.to_string(),
    })
}

pub fn repository_logs(
    request: RepositoryLogsRequest,
) -> Result<RepositoryLogsReport, RepoGrammarError> {
    validate_log_component(request.component.as_deref())?;
    let resolved = resolve_state_dir(&request.path, request.state_dir_override.as_deref())?;
    let status = status_for_resolved_state(&resolved, None)?;
    if matches!(status.status, RepositoryStatus::NotInitialized) {
        return Ok(RepositoryLogsReport {
            state_dir: resolved.relative,
            available: false,
            redacted: true,
            entries: Vec::new(),
            message: "repo-local logs unavailable: repository is not initialized".to_string(),
        });
    }
    if status.missing_subdirs.iter().any(|subdir| subdir == "logs") {
        return Ok(RepositoryLogsReport {
            state_dir: resolved.relative,
            available: false,
            redacted: true,
            entries: Vec::new(),
            message: "repo-local logs unavailable: logs directory is missing".to_string(),
        });
    }

    Ok(RepositoryLogsReport {
        state_dir: resolved.relative,
        available: false,
        redacted: request.redact,
        entries: Vec::new(),
        message: "repo-local log streaming is not implemented yet".to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedStateDir {
    root: PathBuf,
    absolute: PathBuf,
    relative: String,
}

fn status_for_resolved_state(
    resolved: &ResolvedStateDir,
    store: Option<&dyn IndexStore>,
) -> Result<RepositoryStatusReport, RepoGrammarError> {
    if !resolved.absolute.exists() {
        return Ok(RepositoryStatusReport {
            state_dir: resolved.relative.clone(),
            status: RepositoryStatus::NotInitialized,
            manifest: RepositoryManifestStatus::Missing,
            manifest_schema_version: None,
            missing_subdirs: Vec::new(),
            storage: RepositoryImplementationStatus::NotImplemented,
            indexing: RepositoryImplementationStatus::NotImplemented,
            storage_inspection: None,
            storage_error: None,
        });
    }
    ensure_state_path_can_be_directory(&resolved.absolute)?;

    let manifest_path = resolved.absolute.join("manifest.json");
    if !manifest_path.exists() {
        return Ok(RepositoryStatusReport {
            state_dir: resolved.relative.clone(),
            status: RepositoryStatus::NotInitialized,
            manifest: RepositoryManifestStatus::Missing,
            manifest_schema_version: None,
            missing_subdirs: missing_subdirs(&resolved.absolute),
            storage: RepositoryImplementationStatus::NotImplemented,
            indexing: RepositoryImplementationStatus::NotImplemented,
            storage_inspection: None,
            storage_error: None,
        });
    }

    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|_| invalid_input("failed to read repository-local RepoGrammar manifest"))?;
    let manifest_schema_version = bootstrap_manifest_schema_version(&manifest);
    if !is_valid_bootstrap_manifest(&manifest) {
        return Ok(RepositoryStatusReport {
            state_dir: resolved.relative.clone(),
            status: RepositoryStatus::CorruptedManifest,
            manifest: RepositoryManifestStatus::Corrupted,
            manifest_schema_version,
            missing_subdirs: missing_subdirs(&resolved.absolute),
            storage: RepositoryImplementationStatus::NotImplemented,
            indexing: RepositoryImplementationStatus::NotImplemented,
            storage_inspection: None,
            storage_error: None,
        });
    }

    let mut report = RepositoryStatusReport {
        state_dir: resolved.relative.clone(),
        status: RepositoryStatus::Initialized {
            active_generation: "not implemented".to_string(),
        },
        manifest: RepositoryManifestStatus::Valid,
        manifest_schema_version,
        missing_subdirs: missing_subdirs(&resolved.absolute),
        storage: RepositoryImplementationStatus::NotImplemented,
        indexing: RepositoryImplementationStatus::NotImplemented,
        storage_inspection: None,
        storage_error: None,
    };

    if store.is_some() && !report.missing_subdirs.is_empty() {
        report.status = RepositoryStatus::Initialized {
            active_generation: "none".to_string(),
        };
        report.storage = RepositoryImplementationStatus::Unhealthy;
        report.storage_error = Some(missing_state_subdirs_message(&report.missing_subdirs));
        return Ok(report);
    }

    if let Some(store) = store {
        match store.inspect() {
            Ok(inspection) => {
                let active_generation = inspection
                    .active_generation
                    .clone()
                    .unwrap_or_else(|| "none".to_string());
                report.status = RepositoryStatus::Initialized { active_generation };
                report.storage = RepositoryImplementationStatus::Available;
                if inspection.active_generation.is_some() {
                    report.indexing = if inspection.code_unit_count.unwrap_or(0) > 0 {
                        RepositoryImplementationStatus::SyntaxOnlyCodeUnits
                    } else {
                        RepositoryImplementationStatus::FileManifestOnly
                    };
                }
                report.storage_inspection = Some(inspection);
            }
            Err(error) => {
                report.status = RepositoryStatus::Initialized {
                    active_generation: "none".to_string(),
                };
                report.storage = RepositoryImplementationStatus::Unhealthy;
                report.storage_error = Some(index_store_error_message(error));
            }
        }
    }

    Ok(report)
}

fn resolve_state_dir(
    repository_root: &str,
    state_dir_override: Option<&str>,
) -> Result<ResolvedStateDir, RepoGrammarError> {
    if repository_root.trim().is_empty() {
        return Err(invalid_input("repository root must not be empty"));
    }

    let root = PathBuf::from(repository_root);
    if !root.exists() {
        return Err(invalid_input("repository root must exist"));
    }
    if !root.is_dir() {
        return Err(invalid_input("repository root must be a directory"));
    }

    let canonical_root =
        fs::canonicalize(&root).map_err(|_| invalid_input("repository root must be readable"))?;
    let relative = validate_state_dir_name(state_dir_override)?;
    let absolute = root.join(&relative);
    ensure_state_path_is_repo_local(&canonical_root, &absolute)?;

    Ok(ResolvedStateDir {
        root,
        absolute,
        relative,
    })
}

fn validate_state_dir_name(state_dir_override: Option<&str>) -> Result<String, RepoGrammarError> {
    let raw = state_dir_override.unwrap_or(DEFAULT_STATE_DIR);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_input(
            "repository state directory override must not be empty",
        ));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(invalid_input(
            "repository state directory override must be relative",
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(invalid_input(
            "repository state directory override must not contain control characters",
        ));
    }

    let mut components = path.components();
    let name = match components.next() {
        Some(Component::Normal(name)) => name
            .to_str()
            .ok_or_else(|| invalid_input("repository state directory name must be UTF-8"))?,
        _ => {
            return Err(invalid_input(
                "repository state directory override must be a local directory name",
            ));
        }
    };
    if components.next().is_some() {
        return Err(invalid_input(
            "repository state directory override must be a single local directory name",
        ));
    }
    if name == DEFAULT_STATE_DIR
        || name
            .strip_prefix(STATE_DIR_OVERRIDE_PREFIX)
            .is_some_and(|suffix| !suffix.is_empty())
    {
        Ok(name.to_string())
    } else {
        Err(invalid_input(
            "repository state directory override must be .repogrammar or .repogrammar-*",
        ))
    }
}

fn ensure_state_path_is_repo_local(
    canonical_root: &Path,
    state_path: &Path,
) -> Result<(), RepoGrammarError> {
    if let Ok(metadata) = fs::symlink_metadata(state_path) {
        if metadata.file_type().is_symlink() {
            return Err(invalid_input(
                "repository state directory must not be a symlink",
            ));
        }
        if metadata.is_dir() {
            let canonical_state = fs::canonicalize(state_path).map_err(|_| {
                invalid_input("repository state directory must stay inside the repository")
            })?;
            if !canonical_state.starts_with(canonical_root) {
                return Err(invalid_input(
                    "repository state directory must stay inside the repository",
                ));
            }
        }
    }
    Ok(())
}

fn ensure_state_path_can_be_directory(state_path: &Path) -> Result<(), RepoGrammarError> {
    if let Ok(metadata) = fs::symlink_metadata(state_path) {
        if metadata.file_type().is_symlink() {
            return Err(invalid_input(
                "repository state directory must not be a symlink",
            ));
        }
        if !metadata.is_dir() {
            return Err(invalid_input(
                "repository state path exists and is not a directory",
            ));
        }
    }
    Ok(())
}

fn ensure_generated_dir(
    path: &Path,
    relative: &str,
    repaired_entries: &mut Vec<String>,
) -> Result<(), RepoGrammarError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(invalid_input(
            "repository state entry must not be a symlink",
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(invalid_input(
            "repository state entry exists and is not a directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|_| {
                invalid_input("failed to create repository-local RepoGrammar state directory")
            })?;
            repaired_entries.push(relative.to_string());
            Ok(())
        }
        Err(_) => Err(invalid_input(
            "failed to inspect repository-local RepoGrammar state directory",
        )),
    }
}

fn ensure_generated_file(
    path: &Path,
    contents: &str,
    relative: &str,
    repaired_entries: &mut Vec<String>,
) -> Result<(), RepoGrammarError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_input("repository state file must not be a symlink"))
        }
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_input(
            "repository state entry exists and is not a file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(path, contents)
                .map_err(|_| invalid_input("failed to write repository-local RepoGrammar state"))?;
            repaired_entries.push(relative.to_string());
            Ok(())
        }
        Err(_) => Err(invalid_input(
            "failed to inspect repository-local RepoGrammar state file",
        )),
    }
}

fn ensure_manifest(
    path: &Path,
    repaired_entries: &mut Vec<String>,
) -> Result<(), RepoGrammarError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_input("repository manifest must not be a symlink"))
        }
        Ok(metadata) if metadata.is_file() => {
            let manifest = fs::read_to_string(path).map_err(|_| {
                invalid_input("failed to read repository-local RepoGrammar manifest")
            })?;
            if is_valid_bootstrap_manifest(&manifest) {
                Ok(())
            } else {
                Err(invalid_input(
                    "repository manifest is corrupted; run doctor before reinitializing",
                ))
            }
        }
        Ok(_) => Err(invalid_input("repository manifest path is not a file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(path, manifest_contents())
                .map_err(|_| invalid_input("failed to write repository-local manifest"))?;
            repaired_entries.push("manifest.json".to_string());
            Ok(())
        }
        Err(_) => Err(invalid_input(
            "failed to inspect repository-local RepoGrammar manifest",
        )),
    }
}

fn ensure_git_info_exclude(root: &Path) -> Result<bool, RepoGrammarError> {
    let Some(git_dir) = resolve_git_dir(root)? else {
        return Ok(false);
    };

    let info_dir = git_dir.join("info");
    fs::create_dir_all(&info_dir)
        .map_err(|_| invalid_input("failed to prepare Git exclude directory"))?;
    let exclude = info_dir.join("exclude");
    append_missing_lines(&exclude, &GIT_INFO_EXCLUDE_PATTERNS)
}

fn resolve_git_dir(root: &Path) -> Result<Option<PathBuf>, RepoGrammarError> {
    let git_path = root.join(".git");
    let metadata = match fs::symlink_metadata(&git_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid_input("failed to inspect .git")),
    };
    if metadata.file_type().is_symlink() {
        return Err(invalid_input(".git must not be a symlink"));
    }
    if metadata.is_dir() {
        return Ok(Some(git_path));
    }
    if !metadata.is_file() {
        return Ok(None);
    }

    let content = fs::read_to_string(&git_path)
        .map_err(|_| invalid_input("failed to read Git directory pointer"))?;
    let Some(raw_git_dir) = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("gitdir:"))
        .map(str::trim)
    else {
        return Ok(None);
    };
    if raw_git_dir.is_empty() {
        return Err(invalid_input("Git directory pointer must not be empty"));
    }

    let git_dir = PathBuf::from(raw_git_dir);
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        root.join(git_dir)
    };
    match fs::symlink_metadata(&git_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_input("Git directory pointer must not be a symlink"))
        }
        Ok(metadata) if metadata.is_dir() => Ok(Some(git_dir)),
        Ok(_) => Err(invalid_input("Git directory pointer is not a directory")),
        Err(_) => Err(invalid_input("Git directory pointer is not readable")),
    }
}

fn ensure_root_gitignore_marker(root: &Path) -> Result<bool, RepoGrammarError> {
    let gitignore = root.join(".gitignore");
    if let Ok(metadata) = fs::symlink_metadata(&gitignore) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid_input("root .gitignore must be a regular file"));
        }
    }

    let existing = match fs::read_to_string(&gitignore) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(invalid_input("failed to read root .gitignore")),
    };
    let has_begin = existing.contains(ROOT_GITIGNORE_BEGIN);
    let has_end = existing.contains(ROOT_GITIGNORE_END);
    if has_begin && has_end {
        return Ok(false);
    }
    if has_begin || has_end {
        return Err(invalid_input(
            "root .gitignore has an incomplete RepoGrammar marker section",
        ));
    }

    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    if !next.is_empty() {
        next.push('\n');
    }
    next.push_str(ROOT_GITIGNORE_SECTION);
    fs::write(gitignore, next).map_err(|_| invalid_input("failed to write root .gitignore"))?;
    Ok(true)
}

fn append_missing_lines(path: &Path, lines: &[&str]) -> Result<bool, RepoGrammarError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid_input("Git exclude path must be a regular file"));
        }
    }

    let existing = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(invalid_input("failed to read Git exclude file")),
    };
    let existing_lines: Vec<&str> = existing.lines().collect();
    let missing: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !existing_lines.iter().any(|existing| existing == line))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }

    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    for line in missing {
        next.push_str(line);
        next.push('\n');
    }
    fs::write(path, next).map_err(|_| invalid_input("failed to write Git exclude file"))?;
    Ok(true)
}

fn root_gitignore_marker_is_invalid(contents: &str) -> bool {
    let begin_count = contents.matches(ROOT_GITIGNORE_BEGIN).count();
    let end_count = contents.matches(ROOT_GITIGNORE_END).count();
    match (begin_count, end_count) {
        (0, 0) => false,
        (1, 1) => {
            let Some(begin) = contents.find(ROOT_GITIGNORE_BEGIN) else {
                return true;
            };
            let Some(end) = contents.find(ROOT_GITIGNORE_END) else {
                return true;
            };
            if end <= begin {
                return true;
            }
            let section = &contents[begin..end];
            let section_lines = section.lines().collect::<Vec<_>>();
            GIT_INFO_EXCLUDE_PATTERNS
                .iter()
                .any(|pattern| !section_lines.iter().any(|line| line == pattern))
        }
        _ => true,
    }
}

fn missing_subdirs(state_dir: &Path) -> Vec<String> {
    REQUIRED_STATE_SUBDIRS
        .iter()
        .copied()
        .filter(|subdir| !state_dir.join(subdir).is_dir())
        .map(str::to_string)
        .collect()
}

fn manifest_contents() -> String {
    format!(
        "{{\n  \"schema_version\": {},\n  \"repogrammar_version\": \"{}\",\n  \"state\": \"initialized\",\n  \"storage\": {{ \"status\": \"not_implemented\" }},\n  \"indexing\": {{ \"status\": \"not_implemented\" }}\n}}\n",
        BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
        env!("CARGO_PKG_VERSION")
    )
}

fn init_receipt_contents() -> String {
    format!(
        "{{\n  \"schema_version\": {},\n  \"repogrammar_version\": \"{}\",\n  \"operation\": \"init\",\n  \"status\": \"complete\"\n}}\n",
        BOOTSTRAP_MANIFEST_SCHEMA_VERSION,
        env!("CARGO_PKG_VERSION")
    )
}

fn is_valid_bootstrap_manifest(manifest: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    bootstrap_manifest_schema_version(manifest) == Some(BOOTSTRAP_MANIFEST_SCHEMA_VERSION)
        && object
            .get("repogrammar_version")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|version| !version.trim().is_empty())
        && object.get("state").and_then(serde_json::Value::as_str) == Some("initialized")
        && manifest_status_is(object.get("storage"), BOOTSTRAP_STORAGE_STATUSES)
        && manifest_status_is(object.get("indexing"), BOOTSTRAP_INDEXING_STATUSES)
}

fn bootstrap_manifest_schema_version(manifest: &str) -> Option<u32> {
    let value = serde_json::from_str::<serde_json::Value>(manifest).ok()?;
    let version = value.as_object()?.get("schema_version")?.as_u64()?;
    u32::try_from(version).ok()
}

fn manifest_status_is(value: Option<&serde_json::Value>, allowed: &[&str]) -> bool {
    let Some(status) = value
        .and_then(serde_json::Value::as_object)
        .and_then(|object| object.get("status"))
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    allowed.contains(&status)
}

fn is_valid_init_receipt(receipt: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(receipt) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        == Some(1)
        && object
            .get("repogrammar_version")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|version| !version.trim().is_empty())
        && object.get("operation").and_then(serde_json::Value::as_str) == Some("init")
        && object.get("status").and_then(serde_json::Value::as_str) == Some("complete")
}

fn validate_log_component(component: Option<&str>) -> Result<(), RepoGrammarError> {
    match component {
        None => Ok(()),
        Some("daemon" | "index" | "mcp" | "telemetry") => Ok(()),
        Some(_) => Err(invalid_input(
            "log component must be daemon, index, mcp, or telemetry",
        )),
    }
}

fn index_store_error_message(error: crate::ports::index_store::IndexStoreError) -> String {
    match error {
        crate::ports::index_store::IndexStoreError::Unavailable(message)
        | crate::ports::index_store::IndexStoreError::InvalidState(message)
        | crate::ports::index_store::IndexStoreError::InvalidRecord(message) => message,
    }
}

fn missing_state_subdirs_message(missing_subdirs: &[String]) -> String {
    format!(
        "required repository-local state subdirectories are missing: {}",
        missing_subdirs.join(", ")
    )
}

fn invalid_input(message: &'static str) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::persistence::sqlite::SqliteIndexStore;
    use crate::test_support::TempWorkspace;
    use std::fs;
    use std::path::Path;

    fn root_string(path: &Path) -> String {
        path.display().to_string()
    }

    fn init_request(path: &Path) -> RepositoryInitRequest {
        RepositoryInitRequest::new(root_string(path))
    }

    #[test]
    fn index_generation_policy_preserves_previous_valid_index() {
        let policy = IndexGenerationPolicy::default();

        assert!(policy.build_new_generation);
        assert!(policy.atomically_activate_after_validation);
        assert!(policy.preserve_previous_valid_index_on_failure);
    }

    #[test]
    fn status_can_represent_not_initialized_without_storage() {
        assert_eq!(
            RepositoryStatus::NotInitialized.as_human_message(),
            "RepoGrammar repository status: not initialized"
        );
    }

    #[test]
    fn init_creates_repo_local_state_layout_and_git_exclude() {
        let workspace = TempWorkspace::new("repository-init");
        fs::create_dir_all(workspace.path().join(".git")).expect("create git dir");

        let outcome = init_repository(init_request(workspace.path())).expect("init repository");

        assert_eq!(outcome.state_dir, DEFAULT_STATE_DIR);
        assert!(outcome.created);
        assert!(outcome.git_info_exclude_updated);
        assert!(!outcome.root_gitignore_updated);

        let state = workspace.path().join(DEFAULT_STATE_DIR);
        assert!(state.join(".gitignore").is_file());
        assert!(state.join("manifest.json").is_file());
        assert!(state.join("receipts").join("init.json").is_file());
        for subdir in REQUIRED_STATE_SUBDIRS {
            assert!(state.join(subdir).is_dir(), "missing {subdir}");
        }

        let exclude =
            fs::read_to_string(workspace.path().join(".git/info/exclude")).expect("read exclude");
        assert!(exclude.contains(".repogrammar/"));
        assert!(exclude.contains(".repogrammar-*/"));
    }

    #[test]
    fn init_is_idempotent_and_repairs_missing_generated_entries() {
        let workspace = TempWorkspace::new("repository-init-idempotent");
        fs::create_dir_all(workspace.path().join(".git/info")).expect("create git info");

        let first = init_repository(init_request(workspace.path())).expect("first init");
        assert!(first.created);

        fs::remove_dir_all(workspace.path().join(DEFAULT_STATE_DIR).join("cache"))
            .expect("remove generated cache dir");
        let second = init_repository(init_request(workspace.path())).expect("second init");

        assert!(!second.created);
        assert!(second.repaired_entries.contains(&"cache".to_string()));
        assert!(!second.git_info_exclude_updated);
        assert!(workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("cache")
            .is_dir());
    }

    #[test]
    fn init_can_use_safe_repogrammar_override_and_root_gitignore_marker() {
        let workspace = TempWorkspace::new("repository-init-override");
        let mut request = RepositoryLifecycleInitRequest::new(root_string(workspace.path()));
        request.state_dir_override = Some(".repogrammar-linux".to_string());
        request.write_root_gitignore = true;

        let outcome = init_repository(request).expect("init with override");

        assert_eq!(outcome.state_dir, ".repogrammar-linux");
        assert!(workspace.path().join(".repogrammar-linux").is_dir());
        assert!(outcome.root_gitignore_updated);
        let gitignore =
            fs::read_to_string(workspace.path().join(".gitignore")).expect("read root gitignore");
        assert!(gitignore.contains(ROOT_GITIGNORE_BEGIN));
        assert!(gitignore.contains(".repogrammar-*/"));
    }

    #[test]
    fn init_rejects_incomplete_root_gitignore_marker() {
        let workspace = TempWorkspace::new("repository-init-bad-marker");
        fs::write(
            workspace.path().join(".gitignore"),
            format!("{ROOT_GITIGNORE_BEGIN}\n.repogrammar/\n"),
        )
        .expect("write incomplete marker");
        let mut request = RepositoryLifecycleInitRequest::new(root_string(workspace.path()));
        request.write_root_gitignore = true;

        let error = init_repository(request).expect_err("incomplete marker must be rejected");

        assert!(error
            .to_string()
            .contains("incomplete RepoGrammar marker section"));
    }

    #[test]
    fn init_updates_git_info_exclude_for_worktree_gitdir_pointer() {
        let workspace = TempWorkspace::new("repository-worktree");
        let git_storage = TempWorkspace::new("repository-worktree-gitdir");
        fs::write(
            workspace.path().join(".git"),
            format!("gitdir: {}\n", git_storage.path().display()),
        )
        .expect("write gitdir pointer");

        let outcome = init_repository(init_request(workspace.path())).expect("init repository");

        assert!(outcome.git_info_exclude_updated);
        let exclude =
            fs::read_to_string(git_storage.path().join("info/exclude")).expect("read exclude");
        assert!(exclude.contains(".repogrammar/"));
        assert!(exclude.contains(".repogrammar-*/"));
    }

    #[test]
    fn state_dir_override_rejects_empty_absolute_traversal_nested_and_unknown_names() {
        for override_value in [
            "",
            "   ",
            "/tmp/.repogrammar",
            "../.repogrammar",
            ".repogrammar/child",
            ".repogrammar-\u{7}",
            "repogrammar",
            ".repogrammar-",
        ] {
            assert!(
                validate_state_dir_name(Some(override_value)).is_err(),
                "expected invalid override: {override_value:?}"
            );
        }
    }

    #[test]
    fn init_rejects_file_vs_dir_conflict() {
        let workspace = TempWorkspace::new("repository-file-conflict");
        fs::write(workspace.path().join(DEFAULT_STATE_DIR), b"not a directory")
            .expect("write conflicting file");

        let error = init_repository(init_request(workspace.path())).expect_err("conflict error");

        assert!(error
            .to_string()
            .contains("state path exists and is not a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn init_rejects_symlink_state_directory_escape() {
        use std::os::unix::fs::symlink;

        let workspace = TempWorkspace::new("repository-symlink");
        let outside = TempWorkspace::new("repository-symlink-outside");
        symlink(outside.path(), workspace.path().join(DEFAULT_STATE_DIR)).expect("create symlink");

        let error = init_repository(init_request(workspace.path())).expect_err("symlink error");

        assert!(error
            .to_string()
            .contains("state directory must not be a symlink"));
    }

    #[test]
    fn status_reports_not_initialized_and_initialized_without_storage_or_indexing() {
        let workspace = TempWorkspace::new("repository-status");

        let status = repository_status(RepositoryStatusRequest::new(root_string(workspace.path())))
            .expect("status before init");
        assert_eq!(status.status, RepositoryStatus::NotInitialized);
        assert_eq!(status.manifest, RepositoryManifestStatus::Missing);
        assert_eq!(status.manifest_schema_version, None);
        assert_eq!(
            status.storage,
            RepositoryImplementationStatus::NotImplemented
        );
        assert_eq!(
            status.indexing,
            RepositoryImplementationStatus::NotImplemented
        );

        init_repository(init_request(workspace.path())).expect("init repository");
        let status = repository_status(RepositoryStatusRequest::new(root_string(workspace.path())))
            .expect("status after init");
        assert_eq!(
            status.status,
            RepositoryStatus::Initialized {
                active_generation: "not implemented".to_string()
            }
        );
        assert_eq!(status.manifest, RepositoryManifestStatus::Valid);
        assert_eq!(
            status.manifest_schema_version,
            Some(BOOTSTRAP_MANIFEST_SCHEMA_VERSION)
        );
        assert!(status.missing_subdirs.is_empty());
    }

    #[test]
    fn status_accepts_valid_manifest_with_different_json_order_and_formatting() {
        let workspace = TempWorkspace::new("repository-status-reordered-manifest");
        init_repository(init_request(workspace.path())).expect("init repository");
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            r#"{
  "indexing": {
    "status": "not_implemented"
  },
  "state": "initialized",
  "repogrammar_version": "0.1.0",
  "storage": {
    "status": "not_implemented"
  },
  "schema_version": 1
}
"#,
        )
        .expect("rewrite manifest");

        let status = repository_status(RepositoryStatusRequest::new(root_string(workspace.path())))
            .expect("status after manifest rewrite");

        assert_eq!(
            status.status,
            RepositoryStatus::Initialized {
                active_generation: "not implemented".to_string()
            }
        );
        assert_eq!(status.manifest, RepositoryManifestStatus::Valid);
        assert_eq!(
            status.manifest_schema_version,
            Some(BOOTSTRAP_MANIFEST_SCHEMA_VERSION)
        );
    }

    #[test]
    fn bootstrap_manifest_validation_rejects_invalid_required_fields() {
        let valid = manifest_contents();
        let invalid_cases = [
            (
                "not json",
                "schema_version: 1, state: initialized".to_string(),
            ),
            (
                "invalid schema version",
                valid.replace("\"schema_version\": 1", "\"schema_version\": 2"),
            ),
            (
                "invalid state",
                valid.replace("\"state\": \"initialized\"", "\"state\": \"ready\""),
            ),
            (
                "empty version",
                valid.replace(
                    &format!("\"repogrammar_version\": \"{}\"", env!("CARGO_PKG_VERSION")),
                    "\"repogrammar_version\": \"\"",
                ),
            ),
            (
                "whitespace version",
                valid.replace(
                    &format!("\"repogrammar_version\": \"{}\"", env!("CARGO_PKG_VERSION")),
                    "\"repogrammar_version\": \"   \"",
                ),
            ),
            (
                "missing version",
                valid.replace(
                    &format!(
                        "  \"repogrammar_version\": \"{}\",\n",
                        env!("CARGO_PKG_VERSION")
                    ),
                    "",
                ),
            ),
            (
                "string schema version",
                valid.replace("\"schema_version\": 1", "\"schema_version\": \"1\""),
            ),
            (
                "float schema version",
                valid.replace("\"schema_version\": 1", "\"schema_version\": 1.0"),
            ),
            (
                "invalid storage status",
                valid.replace(
                    "\"storage\": { \"status\": \"not_implemented\" }",
                    "\"storage\": { \"status\": \"available\" }",
                ),
            ),
            (
                "storage not object",
                valid.replace(
                    "\"storage\": { \"status\": \"not_implemented\" }",
                    "\"storage\": \"not_implemented\"",
                ),
            ),
            (
                "storage status not string",
                valid.replace(
                    "\"storage\": { \"status\": \"not_implemented\" }",
                    "\"storage\": { \"status\": 1 }",
                ),
            ),
            (
                "invalid indexing status",
                valid.replace(
                    "\"indexing\": { \"status\": \"not_implemented\" }",
                    "\"indexing\": { \"status\": \"syntax_only_code_units\" }",
                ),
            ),
            (
                "indexing not object",
                valid.replace(
                    "\"indexing\": { \"status\": \"not_implemented\" }",
                    "\"indexing\": \"not_implemented\"",
                ),
            ),
            (
                "indexing status not string",
                valid.replace(
                    "\"indexing\": { \"status\": \"not_implemented\" }",
                    "\"indexing\": { \"status\": 1 }",
                ),
            ),
            (
                "missing storage status",
                valid.replace(
                    "\"storage\": { \"status\": \"not_implemented\" }",
                    "\"storage\": {}",
                ),
            ),
            (
                "missing indexing status",
                valid.replace(
                    "\"indexing\": { \"status\": \"not_implemented\" }",
                    "\"indexing\": {}",
                ),
            ),
        ];

        for (case, manifest) in invalid_cases {
            assert!(
                !is_valid_bootstrap_manifest(&manifest),
                "expected invalid manifest for {case}"
            );
        }
    }

    #[test]
    fn storage_status_reports_missing_subdirs_without_recreating_them() {
        let workspace = TempWorkspace::new("repository-storage-missing-subdir");
        init_repository(init_request(workspace.path())).expect("init repository");
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        fs::remove_dir_all(state.join("generations")).expect("remove generations dir");
        let store = SqliteIndexStore::new(&state);

        let status = repository_status_with_storage(
            RepositoryStatusRequest::new(root_string(workspace.path())),
            &store,
        )
        .expect("status with storage");

        assert_eq!(status.missing_subdirs, vec!["generations".to_string()]);
        assert_eq!(status.storage, RepositoryImplementationStatus::Unhealthy);
        assert!(status
            .storage_error
            .as_deref()
            .expect("storage error")
            .contains("generations"));
        assert!(!state.join("generations").exists());
    }

    #[test]
    fn doctor_reports_corrupted_manifest_and_missing_subdir() {
        let workspace = TempWorkspace::new("repository-doctor");
        init_repository(init_request(workspace.path())).expect("init repository");
        fs::write(
            workspace
                .path()
                .join(DEFAULT_STATE_DIR)
                .join("manifest.json"),
            b"{",
        )
        .expect("corrupt manifest");
        fs::remove_dir_all(workspace.path().join(DEFAULT_STATE_DIR).join("logs"))
            .expect("remove logs dir");

        let report = repository_doctor(RepositoryDoctorRequest::new(root_string(workspace.path())))
            .expect("doctor report");

        assert_eq!(report.status.status, RepositoryStatus::CorruptedManifest);
        assert!(report.findings.iter().any(|finding| {
            finding.code == RepositoryDoctorCode::CorruptedManifest
                && finding.severity == RepositoryDoctorSeverity::Error
        }));
        assert!(report.findings.iter().any(|finding| {
            finding.code == RepositoryDoctorCode::MissingSubdir && finding.detail.contains("logs")
        }));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == RepositoryDoctorCode::StorageNotImplemented));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == RepositoryDoctorCode::IndexingNotImplemented));
    }

    #[test]
    fn doctor_reports_missing_lifecycle_hygiene_without_repairing() {
        let workspace = TempWorkspace::new("repository-doctor-missing-hygiene");
        fs::create_dir_all(workspace.path().join(".git")).expect("create git dir");
        init_repository(init_request(workspace.path())).expect("init repository");
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        let state_gitignore = state.join(".gitignore");
        let init_receipt = state.join("receipts").join("init.json");
        let git_exclude = workspace.path().join(".git/info/exclude");
        fs::remove_file(&state_gitignore).expect("remove state gitignore");
        fs::remove_file(&init_receipt).expect("remove init receipt");
        fs::remove_file(&git_exclude).expect("remove git exclude");

        let report = repository_doctor(RepositoryDoctorRequest::new(root_string(workspace.path())))
            .expect("doctor report");

        for code in [
            RepositoryDoctorCode::StateGitignoreMissing,
            RepositoryDoctorCode::InitReceiptMissing,
            RepositoryDoctorCode::GitInfoExcludeMissing,
        ] {
            assert!(
                report.findings.iter().any(|finding| {
                    finding.code == code && finding.severity == RepositoryDoctorSeverity::Error
                }),
                "missing doctor finding for {code:?}"
            );
        }
        assert!(!state_gitignore.exists());
        assert!(!init_receipt.exists());
        assert!(!git_exclude.exists());
    }

    #[test]
    fn doctor_reports_invalid_lifecycle_hygiene_without_repairing() {
        let workspace = TempWorkspace::new("repository-doctor-invalid-hygiene");
        fs::create_dir_all(workspace.path().join(".git")).expect("create git dir");
        init_repository(init_request(workspace.path())).expect("init repository");
        let state = workspace.path().join(DEFAULT_STATE_DIR);
        let state_gitignore = state.join(".gitignore");
        let init_receipt = state.join("receipts").join("init.json");
        let git_exclude = workspace.path().join(".git/info/exclude");
        let root_gitignore = workspace.path().join(".gitignore");
        fs::write(&state_gitignore, "bad\n").expect("write bad state gitignore");
        fs::write(&init_receipt, "{}\n").expect("write bad init receipt");
        fs::write(&git_exclude, ".repogrammar/\n").expect("write incomplete exclude");
        fs::write(
            &root_gitignore,
            format!("{ROOT_GITIGNORE_BEGIN}\n.repogrammar/\n"),
        )
        .expect("write incomplete marker");

        let report = repository_doctor(RepositoryDoctorRequest::new(root_string(workspace.path())))
            .expect("doctor report");

        for code in [
            RepositoryDoctorCode::StateGitignoreInvalid,
            RepositoryDoctorCode::InitReceiptInvalid,
            RepositoryDoctorCode::GitInfoExcludeIncomplete,
            RepositoryDoctorCode::RootGitignoreMarkerInvalid,
        ] {
            assert!(
                report.findings.iter().any(|finding| {
                    finding.code == code && finding.severity == RepositoryDoctorSeverity::Error
                }),
                "missing doctor finding for {code:?}"
            );
        }
        assert_eq!(
            fs::read_to_string(&state_gitignore).expect("state gitignore"),
            "bad\n"
        );
        assert_eq!(
            fs::read_to_string(&init_receipt).expect("init receipt"),
            "{}\n"
        );
        assert_eq!(
            fs::read_to_string(&git_exclude).expect("git exclude"),
            ".repogrammar/\n"
        );
        assert_eq!(
            fs::read_to_string(&root_gitignore).expect("root gitignore"),
            format!("{ROOT_GITIGNORE_BEGIN}\n.repogrammar/\n")
        );
    }

    #[test]
    fn uninit_requires_explicit_yes_and_removes_only_state_dir() {
        let workspace = TempWorkspace::new("repository-uninit");
        init_repository(init_request(workspace.path())).expect("init repository");
        fs::write(workspace.path().join("keep.txt"), b"keep").expect("write sibling");

        let error = uninit_repository(RepositoryUninitRequest {
            path: root_string(workspace.path()),
            state_dir_override: None,
            yes: false,
        })
        .expect_err("confirmation required");
        assert!(error.to_string().contains("requires explicit confirmation"));

        let outcome = uninit_repository(RepositoryUninitRequest {
            path: root_string(workspace.path()),
            state_dir_override: None,
            yes: true,
        })
        .expect("uninit repository");

        assert!(outcome.removed);
        assert!(!workspace.path().join(DEFAULT_STATE_DIR).exists());
        assert!(workspace.path().join("keep.txt").is_file());
    }

    #[test]
    fn unlock_placeholder_inspects_known_locks_without_deleting_them() {
        let workspace = TempWorkspace::new("repository-unlock");
        init_repository(init_request(workspace.path())).expect("init repository");
        let lock = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks")
            .join("index.lock");
        fs::write(&lock, b"{\"kind\":\"index\"}").expect("write lock");

        let report = unlock_repository(RepositoryUnlockRequest {
            path: root_string(workspace.path()),
            state_dir_override: None,
            force: true,
            yes: true,
        })
        .expect("unlock report");

        assert_eq!(report.removed_locks, 0);
        assert_eq!(report.inspected_locks, vec!["index.lock".to_string()]);
        assert!(lock.is_file());
        assert!(!report.message.contains(&root_string(workspace.path())));
    }

    #[test]
    fn index_lock_guard_writes_metadata_and_removes_own_lock() {
        let workspace = TempWorkspace::new("repository-index-lock-guard");
        init_repository(init_request(workspace.path())).expect("init repository");
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks")
            .join(INDEX_LOCK_FILE);

        let guard =
            acquire_index_lock(&root_string(workspace.path()), None).expect("acquire index lock");

        let contents = fs::read_to_string(&lock_path).expect("read lock");
        let value: serde_json::Value = serde_json::from_str(&contents).expect("lock JSON");
        assert_eq!(value["kind"], "index");
        assert_eq!(value["pid"], std::process::id());
        assert_eq!(value["os"], std::env::consts::OS);
        assert!(
            value["started_unix_seconds"]
                .as_u64()
                .expect("started timestamp")
                > 0
        );
        assert!(!value["repogrammar_version"]
            .as_str()
            .expect("version")
            .is_empty());
        assert!(!value["token"].as_str().expect("token").is_empty());

        drop(guard);
        assert!(!lock_path.exists());
    }

    #[test]
    fn index_lock_guard_does_not_remove_replaced_lock() {
        let workspace = TempWorkspace::new("repository-index-lock-replaced");
        init_repository(init_request(workspace.path())).expect("init repository");
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks")
            .join(INDEX_LOCK_FILE);

        let guard =
            acquire_index_lock(&root_string(workspace.path()), None).expect("acquire index lock");
        let replacement = IndexLockMetadata {
            pid: std::process::id(),
            host: current_host(),
            os: std::env::consts::OS.to_string(),
            started_unix_seconds: 1,
            repogrammar_version: env!("CARGO_PKG_VERSION").to_string(),
            token: "replacement-token".to_string(),
        }
        .to_json();
        fs::write(&lock_path, &replacement).expect("replace lock");

        drop(guard);

        assert_eq!(
            fs::read_to_string(&lock_path).expect("replacement lock remains"),
            replacement
        );
    }

    #[test]
    fn stale_index_lock_removal_requires_matching_contents() {
        let workspace = TempWorkspace::new("repository-index-lock-content-match");
        init_repository(init_request(workspace.path())).expect("init repository");
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks")
            .join(INDEX_LOCK_FILE);
        let stale = IndexLockMetadata {
            pid: 0,
            host: current_host(),
            os: std::env::consts::OS.to_string(),
            started_unix_seconds: 1,
            repogrammar_version: env!("CARGO_PKG_VERSION").to_string(),
            token: "stale-token".to_string(),
        }
        .to_json();
        let replacement = IndexLockMetadata {
            pid: std::process::id(),
            host: current_host(),
            os: std::env::consts::OS.to_string(),
            started_unix_seconds: 2,
            repogrammar_version: env!("CARGO_PKG_VERSION").to_string(),
            token: "replacement-token".to_string(),
        }
        .to_json();
        fs::write(&lock_path, &replacement).expect("write replacement lock");

        let removed =
            remove_index_lock_if_contents_match(&lock_path, &stale).expect("remove if matched");

        assert!(!removed);
        assert_eq!(
            fs::read_to_string(&lock_path).expect("replacement lock remains"),
            replacement
        );
    }

    #[test]
    fn index_lock_refuses_live_lock_and_doctor_reports_it() {
        let workspace = TempWorkspace::new("repository-index-lock-live");
        init_repository(init_request(workspace.path())).expect("init repository");
        let guard =
            acquire_index_lock(&root_string(workspace.path()), None).expect("acquire index lock");

        let error = acquire_index_lock(&root_string(workspace.path()), None)
            .expect_err("live lock must be refused");

        assert!(error.to_string().contains("index lock is held"));
        let report = repository_doctor(RepositoryDoctorRequest::new(root_string(workspace.path())))
            .expect("doctor report");
        assert!(report.findings.iter().any(|finding| {
            finding.code == RepositoryDoctorCode::IndexLockActive
                && finding.severity == RepositoryDoctorSeverity::Warning
        }));

        drop(guard);
    }

    #[test]
    fn index_lock_replaces_confirmed_stale_lock() {
        let workspace = TempWorkspace::new("repository-index-lock-stale");
        init_repository(init_request(workspace.path())).expect("init repository");
        let lock_path = workspace
            .path()
            .join(DEFAULT_STATE_DIR)
            .join("locks")
            .join(INDEX_LOCK_FILE);
        let stale = IndexLockMetadata {
            pid: 0,
            host: current_host(),
            os: std::env::consts::OS.to_string(),
            started_unix_seconds: 1,
            repogrammar_version: env!("CARGO_PKG_VERSION").to_string(),
            token: "stale-token".to_string(),
        };
        fs::write(&lock_path, stale.to_json()).expect("write stale lock");

        let report = repository_doctor(RepositoryDoctorRequest::new(root_string(workspace.path())))
            .expect("doctor report");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == RepositoryDoctorCode::IndexLockStale));

        let guard =
            acquire_index_lock(&root_string(workspace.path()), None).expect("replace stale lock");

        let contents = fs::read_to_string(&lock_path).expect("read replacement lock");
        let value: serde_json::Value = serde_json::from_str(&contents).expect("lock JSON");
        assert_eq!(value["pid"], std::process::id());

        drop(guard);
        assert!(!lock_path.exists());
    }

    #[test]
    fn logs_placeholder_is_redacted_and_does_not_expose_paths() {
        let workspace = TempWorkspace::new("repository-logs");
        init_repository(init_request(workspace.path())).expect("init repository");

        let report = repository_logs(RepositoryLogsRequest {
            path: root_string(workspace.path()),
            state_dir_override: None,
            component: Some("index".to_string()),
            tail: Some(20),
            since: Some("1h".to_string()),
            redact: true,
        })
        .expect("logs report");

        assert!(!report.available);
        assert!(report.redacted);
        assert!(report.entries.is_empty());
        assert!(!report.message.contains(&root_string(workspace.path())));
    }

    #[test]
    fn logs_reject_unknown_component() {
        let workspace = TempWorkspace::new("repository-logs-component");

        let error = repository_logs(RepositoryLogsRequest {
            path: root_string(workspace.path()),
            state_dir_override: None,
            component: Some("source".to_string()),
            tail: None,
            since: None,
            redact: true,
        })
        .expect_err("component error");

        assert!(error.to_string().contains("log component must be"));
    }
}
