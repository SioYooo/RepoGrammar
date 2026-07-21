//! Post-exit cleanup for an ownership-validated RepoGrammar installation.
//!
//! This module deliberately does not discover installation ownership. The
//! product-installation service performs that preflight and hands this module
//! an exact, already-owned set of files. We snapshot those files into a private
//! one-shot cleanup plan, copy the validated authority to that private directory,
//! and require both a parent lifecycle EOF and a matching commit capability
//! before any product file can be removed.

use crate::application::install::AgentTarget;
use crate::application::product_installation::{
    ManagedCommandKind, ProductInstallationPlan, ProductOwnershipSource,
};
use crate::error::RepoGrammarError;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{
    ffi::CString,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
};

const PLAN_SCHEMA_VERSION: u64 = 1;
const PLAN_MANAGED_BY: &str = "repogrammar";
const PLAN_KIND: &str = "product-uninstall-finalizer";
const COMMIT_PREFIX: &str = "REPOGRAMMAR_UNINSTALL_COMMIT ";
const MAX_COMMIT_MESSAGE_BYTES: usize = 512;
const FINALIZER_DIRECTORY_PREFIX: &str = "repogrammar-uninstall-";
const PLAN_FILE_NAME: &str = "cleanup-plan.json";
const HELPER_FILE_NAME: &str = if cfg!(windows) {
    "repogrammar-uninstall-finalizer.exe"
} else {
    "repogrammar-uninstall-finalizer"
};
const REPORT_FILE_NAME: &str = "cleanup-report.json";
static FINALIZER_NONCE: AtomicU64 = AtomicU64::new(0);

/// Product-level uninstall options. These intentionally do not contain agent
/// target or scope fields: selective integration removal belongs to
/// `repogrammar disconnect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProductUninstallRequest {
    pub dry_run: bool,
    pub assume_yes: bool,
}

/// Truthful parent-process result for product uninstall.
///
/// A live uninstall can only report `finalizer_pending`: the helper removes
/// the running installation after this process exits and publishes its own
/// structured report at `report_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductUninstallOutcome {
    pub command: &'static str,
    pub dry_run: bool,
    pub ownership_source: ProductOwnershipSource,
    pub agent_targets: Vec<AgentTarget>,
    pub agent_planned_paths: Vec<String>,
    pub planned_paths: Vec<String>,
    pub preserved: Vec<String>,
    pub residual_copies: Vec<String>,
    pub finalizer_pending: bool,
    pub report_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductFileRole {
    Command,
    Worker,
    ProductReceipt,
    Authority,
}

impl ProductFileRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Worker => "worker",
            Self::ProductReceipt => "product_receipt",
            Self::Authority => "authority",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "command" => Some(Self::Command),
            "worker" => Some(Self::Worker),
            "product_receipt" => Some(Self::ProductReceipt),
            "authority" => Some(Self::Authority),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannedFileKind {
    Regular,
    SymlinkToAuthority,
}

impl PlannedFileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::SymlinkToAuthority => "symlink_to_authority",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "regular" => Some(Self::Regular),
            "symlink_to_authority" => Some(Self::SymlinkToAuthority),
            _ => None,
        }
    }
}

/// A narrow input used by the product-ownership adapter. Callers cannot add an
/// arbitrary directory tree: preparation re-derives and validates every
/// accepted path against the first-party installation layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductCleanupInput {
    pub ownership_source: ProductOwnershipSource,
    pub data_dir: PathBuf,
    pub authority_path: PathBuf,
    pub authority_sha256: String,
    pub command_path: PathBuf,
    pub command_is_authority: bool,
    pub command_is_symlink: bool,
    pub command_sha256: String,
    pub worker_files: Vec<(PathBuf, String)>,
    pub product_receipt_path: Option<PathBuf>,
    pub product_receipt_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedProductUninstall {
    pub helper_path: PathBuf,
    pub plan_path: PathBuf,
    pub report_path: PathBuf,
    pub parent_pid: u32,
    commit_token: String,
}

impl PreparedProductUninstall {
    /// Bytes the parent writes only after agent cleanup commits. Closing the
    /// lifecycle channel after these bytes gives the helper its EOF proof.
    pub fn commit_message(&self) -> Vec<u8> {
        format!("{COMMIT_PREFIX}{}\n", self.commit_token).into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizerRemovedItem {
    pub path: String,
    pub role: String,
    pub disposition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizerPreservedItem {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizerFailure {
    pub path: String,
    pub class: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductUninstallFinalizerReport {
    pub status: String,
    pub removed: Vec<FinalizerRemovedItem>,
    pub preserved: Vec<FinalizerPreservedItem>,
    pub failed: Vec<FinalizerFailure>,
    pub residual_copies: Vec<String>,
    pub manual_recovery: Vec<String>,
}

impl ProductUninstallFinalizerReport {
    pub fn is_complete(&self) -> bool {
        self.status == "complete"
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": 1,
            "managed_by": PLAN_MANAGED_BY,
            "kind": "product-uninstall-report",
            "status": self.status,
            "removed": self.removed.iter().map(|item| json!({
                "path": item.path,
                "role": item.role,
                "disposition": item.disposition,
            })).collect::<Vec<_>>(),
            "preserved": self.preserved.iter().map(|item| json!({
                "path": item.path,
                "reason": item.reason,
            })).collect::<Vec<_>>(),
            "failed": self.failed.iter().map(|item| json!({
                "path": item.path,
                "class": item.class,
                "error": item.error,
            })).collect::<Vec<_>>(),
            "residual_copies": self.residual_copies,
            "manual_recovery": self.manual_recovery,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
    len: u64,
    modified_unix_nanos: Option<u64>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial_number: Option<u32>,
    #[cfg(windows)]
    file_index: Option<u64>,
}

impl FileIdentity {
    #[cfg(not(windows))]
    fn capture(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified_unix_nanos: modified_unix_nanos(metadata),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }

    #[cfg(windows)]
    fn capture_open_file(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        let (volume_serial_number, file_index) = windows_file_identity(file)?;
        Ok(Self {
            len: metadata.len(),
            modified_unix_nanos: modified_unix_nanos(&metadata),
            volume_serial_number: Some(volume_serial_number),
            file_index: Some(file_index),
        })
    }

    #[cfg(not(windows))]
    fn matches(&self, metadata: &Metadata) -> bool {
        if metadata.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                return self.device == metadata.dev() && self.inode == metadata.ino();
            }
            #[cfg(not(unix))]
            {
                // This finalizer is not supported on a platform where a
                // directory replacement cannot be distinguished from the
                // directory that was preflighted.
                return false;
            }
        }
        self == &Self::capture(metadata)
    }

    #[cfg(windows)]
    fn same_windows_object(&self, file: &File) -> io::Result<bool> {
        let (volume_serial_number, file_index) = windows_file_identity(file)?;
        Ok(self.volume_serial_number == Some(volume_serial_number)
            && self.file_index == Some(file_index))
    }

    #[cfg(windows)]
    fn matches_open_file(&self, file: &File) -> io::Result<bool> {
        let metadata = file.metadata()?;
        if metadata.is_dir() {
            self.same_windows_object(file)
        } else {
            Ok(self == &Self::capture_open_file(file)?)
        }
    }

    fn to_json_value(&self) -> Value {
        let mut value = Map::new();
        value.insert("len".to_string(), json!(self.len));
        value.insert(
            "modified_unix_nanos".to_string(),
            self.modified_unix_nanos.map_or(Value::Null, Value::from),
        );
        #[cfg(unix)]
        {
            value.insert("device".to_string(), json!(self.device));
            value.insert("inode".to_string(), json!(self.inode));
        }
        #[cfg(windows)]
        {
            value.insert(
                "volume_serial_number".to_string(),
                self.volume_serial_number.map_or(Value::Null, Value::from),
            );
            value.insert(
                "file_index".to_string(),
                self.file_index.map_or(Value::Null, Value::from),
            );
        }
        Value::Object(value)
    }

    fn from_json(value: &Value) -> Result<Self, RepoGrammarError> {
        let object = strict_object(value, "file identity")?;
        let allowed: &[&str] = if cfg!(unix) {
            &["len", "modified_unix_nanos", "device", "inode"]
        } else if cfg!(windows) {
            &[
                "len",
                "modified_unix_nanos",
                "volume_serial_number",
                "file_index",
            ]
        } else {
            &["len", "modified_unix_nanos"]
        };
        require_exact_keys(object, allowed, "file identity")?;
        Ok(Self {
            len: required_u64(object, "len", "file identity")?,
            modified_unix_nanos: optional_u64(object, "modified_unix_nanos", "file identity")?,
            #[cfg(unix)]
            device: required_u64(object, "device", "file identity")?,
            #[cfg(unix)]
            inode: required_u64(object, "inode", "file identity")?,
            #[cfg(windows)]
            volume_serial_number: optional_u64(object, "volume_serial_number", "file identity")?
                .map(u32::try_from)
                .transpose()
                .map_err(|_| invalid("file identity volume serial number is out of range"))?,
            #[cfg(windows)]
            file_index: optional_u64(object, "file_index", "file identity")?,
        })
    }
}

fn modified_unix_nanos(metadata: &Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
}

fn capture_file_identity(
    path: &Path,
    metadata: &Metadata,
    label: &str,
) -> Result<FileIdentity, RepoGrammarError> {
    #[cfg(windows)]
    {
        let _ = metadata;
        let handle = open_windows_delete_handle(path).map_err(|error| {
            invalid(format!(
                "failed to open {label} for Windows identity capture: {error}"
            ))
        })?;
        FileIdentity::capture_open_file(&handle).map_err(|error| {
            invalid(format!(
                "failed to capture {label} Windows file identity: {error}"
            ))
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (path, label);
        Ok(FileIdentity::capture(metadata))
    }
}

fn file_identity_matches_path(
    expected: &FileIdentity,
    path: &Path,
    metadata: &Metadata,
    label: &str,
) -> Result<bool, RepoGrammarError> {
    #[cfg(windows)]
    {
        let _ = metadata;
        let handle = open_windows_delete_handle(path).map_err(|error| {
            invalid(format!(
                "failed to open {label} for Windows identity validation: {error}"
            ))
        })?;
        expected.matches_open_file(&handle).map_err(|error| {
            invalid(format!(
                "failed to validate {label} Windows file identity: {error}"
            ))
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (path, label);
        Ok(expected.matches(metadata))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParentIdentity {
    path: String,
    canonical_path: String,
    identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedFile {
    path: String,
    role: ProductFileRole,
    kind: PlannedFileKind,
    sha256: String,
    symlink_target: Option<String>,
    identity: FileIdentity,
    parent: ParentIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedDirectory {
    path: String,
    canonical_path: Option<String>,
    identity: Option<FileIdentity>,
    parent: Option<ParentIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanupPlan {
    data_dir: String,
    authority_path: String,
    parent_pid: u32,
    helper_path: String,
    helper_sha256: String,
    report_path: String,
    commit_token_sha256: String,
    files: Vec<PlannedFile>,
    empty_directories: Vec<PlannedDirectory>,
    residual_copies: Vec<String>,
}

#[derive(Debug)]
pub struct ValidatedProductUninstallFinalizer {
    plan: CleanupPlan,
    plan_path: PathBuf,
    plan_identity: FileIdentity,
    plan_sha256: String,
}

#[derive(Debug)]
pub struct FinalizerCommitCapability {
    plan_sha256: String,
}

#[derive(Debug)]
pub struct ParentExitProof {
    plan_sha256: String,
}

/// Injectable removal boundary used by failure-path tests. All ownership and
/// identity checks happen before this boundary.
pub(crate) trait FinalizerRemoval {
    fn remove_file(&mut self, file: &PlannedFile) -> io::Result<()>;
    fn remove_directory(&mut self, directory: &PlannedDirectory) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct SystemFinalizerRemoval;

impl FinalizerRemoval for SystemFinalizerRemoval {
    fn remove_file(&mut self, file: &PlannedFile) -> io::Result<()> {
        secure_remove_planned_file(file)
    }

    fn remove_directory(&mut self, directory: &PlannedDirectory) -> io::Result<()> {
        secure_remove_planned_directory(directory)
    }
}

struct FinalizerPreparationGuard {
    directory: PathBuf,
    armed: bool,
}

impl FinalizerPreparationGuard {
    fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FinalizerPreparationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        for name in [HELPER_FILE_NAME, PLAN_FILE_NAME, REPORT_FILE_NAME] {
            let _ = fs::remove_file(self.directory.join(name));
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(unix)]
fn secure_remove_planned_file(file: &PlannedFile) -> io::Result<()> {
    let path = Path::new(&file.path);
    let parent_path = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "owned file has no parent"))?;
    let leaf = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "owned file has no leaf"))?;
    let parent = File::open(parent_path)?;
    if !file.parent.identity.matches(&parent.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "owned file parent changed before handle-relative removal",
        ));
    }
    let leaf = CString::new(leaf.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "owned file leaf contains NUL"))?;
    let quarantine_name = format!(
        ".repogrammar-uninstall-quarantine-{}",
        secure_random_hex(16)?
    );
    let quarantine = CString::new(quarantine_name.as_bytes()).expect("generated name has no NUL");
    let parent_fd = parent.as_raw_fd();
    let renamed =
        unsafe { libc::renameat(parent_fd, leaf.as_ptr(), parent_fd, quarantine.as_ptr()) };
    if renamed != 0 {
        return Err(io::Error::last_os_error());
    }

    let quarantine_path = parent_path.join(&quarantine_name);
    let verification = verify_quarantined_file(parent_fd, &quarantine, file);
    if let Err(error) = verification {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "quarantined leaf failed identity verification and was preserved at {}: {error}",
                quarantine_path.display()
            ),
        ));
    }
    let unlinked = unsafe { libc::unlinkat(parent_fd, quarantine.as_ptr(), 0) };
    if unlinked != 0 {
        return Err(io::Error::other(format!(
            "failed to remove verified quarantined leaf at {}: {}",
            quarantine_path.display(),
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn verify_quarantined_file(
    parent_fd: std::os::fd::RawFd,
    quarantine: &CString,
    planned: &PlannedFile,
) -> io::Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
    let result = unsafe {
        libc::fstatat(
            parent_fd,
            quarantine.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    if planned.identity.device != unix_stat_device(&stat) || planned.identity.inode != stat.st_ino {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "quarantined leaf identity differs from the cleanup plan",
        ));
    }
    if planned.kind == PlannedFileKind::SymlinkToAuthority {
        return Ok(());
    }

    let fd = unsafe {
        libc::openat(
            parent_fd,
            quarantine.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut quarantined = unsafe { File::from_raw_fd(fd) };
    if !planned.identity.matches(&quarantined.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "quarantined regular file metadata differs from the cleanup plan",
        ));
    }
    if sha256_reader(&mut quarantined)? != planned.sha256 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "quarantined regular file hash differs from the cleanup plan",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn secure_remove_planned_directory(directory: &PlannedDirectory) -> io::Result<()> {
    let path = Path::new(&directory.path);
    if fs::read_dir(path)?.next().transpose()?.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::DirectoryNotEmpty,
            "owned installation directory is not empty",
        ));
    }
    let parent_record = directory.parent.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "directory appeared after finalizer preflight",
        )
    })?;
    let parent_path = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "owned directory has no parent")
    })?;
    let leaf = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "owned directory has no leaf")
    })?;
    let parent = File::open(parent_path)?;
    if !parent_record.identity.matches(&parent.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "owned directory parent changed before handle-relative removal",
        ));
    }
    let leaf = CString::new(leaf.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "owned directory leaf contains NUL",
        )
    })?;
    let quarantine_name = format!(
        ".repogrammar-uninstall-quarantine-{}",
        secure_random_hex(16)?
    );
    let quarantine = CString::new(quarantine_name.as_bytes()).expect("generated name has no NUL");
    let parent_fd = parent.as_raw_fd();
    if unsafe { libc::renameat(parent_fd, leaf.as_ptr(), parent_fd, quarantine.as_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }

    let quarantine_path = parent_path.join(&quarantine_name);
    let verification = verify_quarantined_directory(parent_fd, &quarantine, directory);
    if let Err(error) = verification {
        let restore = restore_quarantined_name(parent_fd, &quarantine, &leaf);
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "quarantined directory failed verification at {}: {error}; restore={restore}",
                quarantine_path.display()
            ),
        ));
    }
    if unsafe { libc::unlinkat(parent_fd, quarantine.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
        let error = io::Error::last_os_error();
        let restore = restore_quarantined_name(parent_fd, &quarantine, &leaf);
        return Err(io::Error::new(
            error.kind(),
            format!(
                "failed to remove verified quarantined directory at {}: {error}; restore={restore}",
                quarantine_path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn verify_quarantined_directory(
    parent_fd: std::os::fd::RawFd,
    quarantine: &CString,
    planned: &PlannedDirectory,
) -> io::Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
    if unsafe {
        libc::fstatat(
            parent_fd,
            quarantine.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let identity = planned.identity.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "directory lacks planned identity",
        )
    })?;
    if identity.device != unix_stat_device(&stat) || identity.inode != stat.st_ino {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "quarantined directory identity differs from cleanup plan",
        ));
    }
    let fd = unsafe {
        libc::openat(
            parent_fd,
            quarantine.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        unsafe { libc::close(fd) };
        return Err(io::Error::last_os_error());
    }
    let mut nonempty = false;
    loop {
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            nonempty = true;
            break;
        }
    }
    unsafe { libc::closedir(stream) };
    if nonempty {
        Err(io::Error::new(
            io::ErrorKind::DirectoryNotEmpty,
            "quarantined installation directory became non-empty",
        ))
    } else {
        Ok(())
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn unix_stat_device(stat: &libc::stat) -> u64 {
    stat.st_dev as u64
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unix_stat_device(stat: &libc::stat) -> u64 {
    stat.st_dev
}

#[cfg(target_os = "linux")]
fn restore_quarantined_name(
    parent_fd: std::os::fd::RawFd,
    quarantine: &CString,
    original: &CString,
) -> String {
    let result = unsafe {
        libc::renameat2(
            parent_fd,
            quarantine.as_ptr(),
            parent_fd,
            original.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        "restored".to_string()
    } else {
        format!("preserved_in_quarantine: {}", io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn restore_quarantined_name(
    parent_fd: std::os::fd::RawFd,
    quarantine: &CString,
    original: &CString,
) -> String {
    const RENAME_EXCL: u32 = 0x0000_0004;
    let result = unsafe {
        renameatx_np(
            parent_fd,
            quarantine.as_ptr(),
            parent_fd,
            original.as_ptr(),
            RENAME_EXCL,
        )
    };
    if result == 0 {
        "restored".to_string()
    } else {
        format!("preserved_in_quarantine: {}", io::Error::last_os_error())
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn restore_quarantined_name(
    _parent_fd: std::os::fd::RawFd,
    _quarantine: &CString,
    _original: &CString,
) -> String {
    "preserved_in_quarantine: atomic no-replace restore is unsupported".to_string()
}

#[cfg(target_os = "macos")]
extern "C" {
    fn renameatx_np(
        old_dir_fd: libc::c_int,
        old_path: *const libc::c_char,
        new_dir_fd: libc::c_int,
        new_path: *const libc::c_char,
        flags: libc::c_uint,
    ) -> libc::c_int;
}

#[cfg(windows)]
fn secure_remove_planned_file(file: &PlannedFile) -> io::Result<()> {
    let mut handle = open_windows_delete_handle(Path::new(&file.path))?;
    let identity_matches = if file.kind == PlannedFileKind::SymlinkToAuthority {
        file.identity.same_windows_object(&handle)?
    } else {
        file.identity.matches_open_file(&handle)?
    };
    if !identity_matches {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "opened Windows file identity differs from cleanup plan",
        ));
    }
    if file.kind == PlannedFileKind::Regular && sha256_reader(&mut handle)? != file.sha256 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "opened Windows file hash differs from cleanup plan",
        ));
    }
    mark_windows_handle_for_deletion(&handle)
}

#[cfg(windows)]
fn secure_remove_planned_directory(directory: &PlannedDirectory) -> io::Result<()> {
    let handle = open_windows_delete_handle(Path::new(&directory.path))?;
    let identity = directory.identity.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows directory appeared after finalizer preflight",
        )
    })?;
    if !identity.matches_open_file(&handle)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "opened Windows directory identity differs from cleanup plan",
        ));
    }
    mark_windows_handle_for_deletion(&handle)
}

#[cfg(windows)]
fn open_windows_delete_handle(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const GENERIC_READ: u32 = 0x8000_0000;
    const DELETE: u32 = 0x0001_0000;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const FILE_SHARE_DELETE: u32 = 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .access_mode(GENERIC_READ | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
fn mark_windows_handle_for_deletion(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    const FILE_DISPOSITION_INFO_CLASS: u32 = 4;
    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: i32,
    }
    let mut disposition = FileDispositionInfo { delete_file: 1 };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FILE_DISPOSITION_INFO_CLASS,
            (&mut disposition as *mut FileDispositionInfo).cast(),
            std::mem::size_of::<FileDispositionInfo>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> io::Result<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;

    let mut information = ByHandleFileInformation::default();
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let file_index =
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low);
    Ok((information.volume_serial_number, file_index))
}

#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
struct WindowsFileTime {
    low_date_time: u32,
    high_date_time: u32,
}

#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation_time: WindowsFileTime,
    last_access_time: WindowsFileTime,
    last_write_time: WindowsFileTime,
    volume_serial_number: u32,
    file_size_high: u32,
    file_size_low: u32,
    number_of_links: u32,
    file_index_high: u32,
    file_index_low: u32,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandle(
        handle: *mut std::ffi::c_void,
        information: *mut ByHandleFileInformation,
    ) -> i32;

    fn SetFileInformationByHandle(
        handle: *mut std::ffi::c_void,
        information_class: u32,
        information: *mut std::ffi::c_void,
        buffer_size: u32,
    ) -> i32;
}

#[cfg(not(any(unix, windows)))]
fn secure_remove_planned_file(_file: &PlannedFile) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure product self-uninstall is unsupported on this platform",
    ))
}

#[cfg(not(any(unix, windows)))]
fn secure_remove_planned_directory(_directory: &PlannedDirectory) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure product self-uninstall is unsupported on this platform",
    ))
}

pub(crate) fn prepare_product_uninstall_finalizer_from_input(
    input: &ProductCleanupInput,
    running_executable: &Path,
    temp_root: &Path,
    residual_copies: Vec<String>,
) -> Result<PreparedProductUninstall, RepoGrammarError> {
    validate_cleanup_input(input)?;
    validate_secure_temp_root(temp_root)?;
    let finalizer_dir = create_private_finalizer_directory(temp_root)?;
    let mut preparation_guard = FinalizerPreparationGuard::new(finalizer_dir.clone());
    let helper_path = finalizer_dir.join(HELPER_FILE_NAME);
    copy_helper_create_new(running_executable, &helper_path)?;
    let helper_sha256 = sha256_regular_file(&helper_path)?;
    let report_path = finalizer_dir.join(REPORT_FILE_NAME);
    let plan_path = finalizer_dir.join(PLAN_FILE_NAME);
    ensure_path_absent(&report_path, "cleanup report")?;
    ensure_path_absent(&plan_path, "cleanup plan")?;

    let parent_pid = std::process::id();
    let commit_token = finalizer_commit_token(&finalizer_dir, &helper_sha256, parent_pid)?;
    let files = snapshot_cleanup_files(input)?;
    let empty_directories = derive_empty_directories(input)?;
    let plan = CleanupPlan {
        data_dir: path_text(&input.data_dir, "data directory")?,
        authority_path: path_text(&input.authority_path, "authority path")?,
        parent_pid,
        helper_path: path_text(&helper_path, "helper path")?,
        helper_sha256,
        report_path: path_text(&report_path, "report path")?,
        commit_token_sha256: sha256_bytes(commit_token.as_bytes()),
        files,
        empty_directories,
        residual_copies,
    };
    validate_plan_layout(&plan, &plan_path)?;
    write_create_new_private(&plan_path, format!("{}\n", plan_to_json(&plan)))?;

    let prepared = PreparedProductUninstall {
        helper_path,
        plan_path,
        report_path,
        parent_pid,
        commit_token,
    };
    preparation_guard.disarm();
    Ok(prepared)
}

/// Prepare a private, post-exit helper from ownership evidence returned by
/// `inspect_product_installation`. No caller-selected product path enters the
/// persisted cleanup plan.
pub fn prepare_product_uninstall_finalizer(
    installation: &ProductInstallationPlan,
    temp_root: &Path,
    residual_copies: Vec<String>,
) -> Result<PreparedProductUninstall, RepoGrammarError> {
    let receipt_sha256 = installation
        .receipt_path
        .as_deref()
        .map(sha256_regular_file)
        .transpose()?;
    let input = ProductCleanupInput {
        ownership_source: installation.source,
        data_dir: installation.data_dir.clone(),
        authority_path: installation.executable.path.clone(),
        authority_sha256: installation.executable.sha256.clone(),
        command_path: installation.command.file.path.clone(),
        command_is_authority: installation.command.kind == ManagedCommandKind::Authority,
        command_is_symlink: installation.command.kind == ManagedCommandKind::Symlink,
        command_sha256: installation.command.file.sha256.clone(),
        worker_files: installation
            .workers
            .iter()
            .map(|worker| (worker.path.clone(), worker.sha256.clone()))
            .collect(),
        product_receipt_path: installation.receipt_path.clone(),
        product_receipt_sha256: receipt_sha256,
    };
    prepare_product_uninstall_finalizer_from_input(
        &input,
        &installation.executable.path,
        temp_root,
        residual_copies,
    )
}

/// Load and fully validate a hidden-finalizer invocation before it announces
/// READY. In particular, the invoking executable must be the private helper
/// copy named by the plan and must still match its recorded hash.
pub fn validate_product_uninstall_finalizer_invocation(
    plan_path: &Path,
    running_executable: &Path,
) -> Result<ValidatedProductUninstallFinalizer, RepoGrammarError> {
    validate_private_plan_path(plan_path)?;
    let metadata = regular_nonsymlink_metadata(plan_path, "cleanup plan")?;
    let plan_identity = capture_file_identity(plan_path, &metadata, "cleanup plan")?;
    let bytes = read_limited(plan_path, 1024 * 1024, "cleanup plan")?;
    let plan_sha256 = sha256_bytes(&bytes);
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("cleanup plan is malformed JSON; refusing finalizer invocation"))?;
    let plan = plan_from_json(&value)?;
    validate_plan_layout(&plan, plan_path)?;

    let expected_helper = Path::new(&plan.helper_path);
    if !same_lexical_path(expected_helper, running_executable) {
        return Err(invalid(
            "hidden uninstall finalizer may run only from the private helper copy",
        ));
    }
    let helper_metadata = regular_nonsymlink_metadata(running_executable, "finalizer helper")?;
    if !helper_metadata.is_file() || sha256_regular_file(running_executable)? != plan.helper_sha256
    {
        return Err(invalid(
            "private uninstall finalizer helper no longer matches the cleanup plan",
        ));
    }
    revalidate_all_planned_files(&plan)?;

    Ok(ValidatedProductUninstallFinalizer {
        plan,
        plan_path: plan_path.to_path_buf(),
        plan_identity,
        plan_sha256,
    })
}

impl ValidatedProductUninstallFinalizer {
    /// Read the one-shot commit capability through EOF. A correct token without
    /// EOF is not sufficient because the still-live parent retains rollback
    /// authority until it closes this lifecycle channel.
    pub fn establish_commit_from_eof<R: Read>(
        &self,
        reader: &mut R,
    ) -> Result<FinalizerCommitCapability, RepoGrammarError> {
        let mut bytes = Vec::new();
        reader
            .take((MAX_COMMIT_MESSAGE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                invalid(format!(
                    "failed to read finalizer lifecycle channel: {error}"
                ))
            })?;
        if bytes.len() > MAX_COMMIT_MESSAGE_BYTES {
            return Err(invalid("finalizer commit capability is oversized"));
        }
        let message = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("finalizer commit capability is not UTF-8"))?;
        let Some(token) = message
            .strip_prefix(COMMIT_PREFIX)
            .and_then(|value| value.strip_suffix('\n'))
        else {
            return Err(invalid("finalizer commit capability is malformed"));
        };
        if token.is_empty() || sha256_bytes(token.as_bytes()) != self.plan.commit_token_sha256 {
            return Err(invalid(
                "finalizer commit capability did not match the cleanup plan",
            ));
        }
        Ok(FinalizerCommitCapability {
            plan_sha256: self.plan_sha256.clone(),
        })
    }

    /// Wait until the exact parent process has exited. The application should
    /// call this only after receiving the commit capability through EOF.
    pub fn wait_for_parent_exit(
        &self,
        timeout: Duration,
    ) -> Result<ParentExitProof, RepoGrammarError> {
        if self.plan.parent_pid == 0 || self.plan.parent_pid == std::process::id() {
            return Err(invalid("cleanup plan records an invalid parent process id"));
        }
        let started = std::time::Instant::now();
        loop {
            #[cfg(unix)]
            if parent_process_is_zombie(self.plan.parent_pid) == Some(true) {
                return Ok(ParentExitProof {
                    plan_sha256: self.plan_sha256.clone(),
                });
            }
            match crate::application::process_liveness::process_liveness_for_lock(
                self.plan.parent_pid,
                None,
            ) {
                crate::application::process_liveness::ProcessLiveness::Dead => {
                    return Ok(ParentExitProof {
                        plan_sha256: self.plan_sha256.clone(),
                    });
                }
                crate::application::process_liveness::ProcessLiveness::Live => {}
                crate::application::process_liveness::ProcessLiveness::Unknown => {
                    return Err(invalid(
                        "cannot prove uninstall parent exited; no product files were removed",
                    ));
                }
            }
            if started.elapsed() >= timeout {
                return Err(invalid(
                    "timed out waiting for uninstall parent to exit; no product files were removed",
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(unix)]
fn parent_process_is_zombie(pid: u32) -> Option<bool> {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let state = String::from_utf8_lossy(&output.stdout);
    process_state_is_zombie(&state)
}

#[cfg(unix)]
fn process_state_is_zombie(state: &str) -> Option<bool> {
    state
        .split_whitespace()
        .next()
        .map(|value| value.starts_with('Z'))
}

pub fn execute_product_uninstall_finalizer(
    validated: ValidatedProductUninstallFinalizer,
    commit: FinalizerCommitCapability,
    parent_exit: ParentExitProof,
) -> Result<ProductUninstallFinalizerReport, RepoGrammarError> {
    execute_product_uninstall_finalizer_with_removal(
        validated,
        commit,
        parent_exit,
        &mut SystemFinalizerRemoval,
    )
}

pub(crate) fn execute_product_uninstall_finalizer_with_removal<R: FinalizerRemoval>(
    validated: ValidatedProductUninstallFinalizer,
    commit: FinalizerCommitCapability,
    parent_exit: ParentExitProof,
    removal: &mut R,
) -> Result<ProductUninstallFinalizerReport, RepoGrammarError> {
    if commit.plan_sha256 != validated.plan_sha256
        || parent_exit.plan_sha256 != validated.plan_sha256
    {
        return Err(invalid(
            "finalizer lifecycle proof belongs to a different cleanup plan",
        ));
    }
    revalidate_plan_file(&validated)?;
    validate_plan_layout(&validated.plan, &validated.plan_path)?;

    let mut report = ProductUninstallFinalizerReport {
        status: "in_progress".to_string(),
        removed: Vec::new(),
        preserved: Vec::new(),
        failed: Vec::new(),
        residual_copies: validated.plan.residual_copies.clone(),
        manual_recovery: Vec::new(),
    };
    write_report_atomically(Path::new(&validated.plan.report_path), &report)?;
    let mut blocked = false;
    for file in &validated.plan.files {
        if blocked {
            report.preserved.push(FinalizerPreservedItem {
                path: file.path.clone(),
                reason: "preserved because an earlier cleanup step failed".to_string(),
            });
            continue;
        }
        match revalidate_planned_file(file, &validated.plan.authority_path) {
            Ok(PlannedFilePresence::AlreadyAbsent) => {
                report.removed.push(FinalizerRemovedItem {
                    path: file.path.clone(),
                    role: file.role.as_str().to_string(),
                    disposition: "already_absent".to_string(),
                });
            }
            Ok(PlannedFilePresence::Present) => match removal.remove_file(file) {
                Ok(()) => report.removed.push(FinalizerRemovedItem {
                    path: file.path.clone(),
                    role: file.role.as_str().to_string(),
                    disposition: "removed".to_string(),
                }),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    report.removed.push(FinalizerRemovedItem {
                        path: file.path.clone(),
                        role: file.role.as_str().to_string(),
                        disposition: "already_absent".to_string(),
                    });
                }
                Err(error) => {
                    blocked = true;
                    report.preserved.push(FinalizerPreservedItem {
                        path: file.path.clone(),
                        reason:
                            "owned leaf removal failed; the failure records any quarantine location"
                                .to_string(),
                    });
                    report.failed.push(FinalizerFailure {
                        path: file.path.clone(),
                        class: format!("{}_remove_failed", file.role.as_str()),
                        error: error.to_string(),
                    });
                }
            },
            Err(error) => {
                blocked = true;
                report.preserved.push(FinalizerPreservedItem {
                    path: file.path.clone(),
                    reason: "ownership drift detected at final deletion boundary".to_string(),
                });
                report.failed.push(FinalizerFailure {
                    path: file.path.clone(),
                    class: format!("{}_ownership_drift", file.role.as_str()),
                    error: error.to_string(),
                });
            }
        }
        write_report_atomically(Path::new(&validated.plan.report_path), &report)?;
    }

    if !blocked {
        for directory in &validated.plan.empty_directories {
            match revalidate_empty_directory_candidate(directory, &validated.plan) {
                Ok(DirectoryPresence::AlreadyAbsent) => {}
                Ok(DirectoryPresence::Present) => match removal.remove_directory(directory) {
                    Ok(()) => report.removed.push(FinalizerRemovedItem {
                        path: directory.path.clone(),
                        role: "empty_installation_directory".to_string(),
                        disposition: "removed".to_string(),
                    }),
                    Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {
                        report.preserved.push(FinalizerPreservedItem {
                            path: directory.path.clone(),
                            reason: "directory was not empty".to_string(),
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        report.preserved.push(FinalizerPreservedItem {
                            path: directory.path.clone(),
                            reason: "directory removal failed; inspect the failure before manual recovery"
                                .to_string(),
                        });
                        report.failed.push(FinalizerFailure {
                            path: directory.path.clone(),
                            class: "directory_remove_failed".to_string(),
                            error: error.to_string(),
                        });
                    }
                },
                Err(error) => {
                    report.preserved.push(FinalizerPreservedItem {
                        path: directory.path.clone(),
                        reason: "directory ownership drift detected at final deletion boundary"
                            .to_string(),
                    });
                    report.failed.push(FinalizerFailure {
                        path: directory.path.clone(),
                        class: "directory_ownership_drift".to_string(),
                        error: error.to_string(),
                    });
                }
            }
            write_report_atomically(Path::new(&validated.plan.report_path), &report)?;
        }
    }

    if report.failed.is_empty() {
        report.status = "complete".to_string();
    } else {
        report.status = "partial".to_string();
        report.manual_recovery.push(format!(
            "review the cleanup report at {} and remove only files whose RepoGrammar ownership can still be verified",
            validated.plan.report_path
        ));
    }
    for residual in &report.residual_copies {
        report.manual_recovery.push(format!(
            "unmanaged or package-manager copy was preserved: {residual}"
        ));
    }
    write_report_atomically(Path::new(&validated.plan.report_path), &report)?;
    Ok(report)
}

pub fn record_product_uninstall_finalizer_abort(
    validated: &ValidatedProductUninstallFinalizer,
    class: &str,
    error: &str,
) -> Result<(), RepoGrammarError> {
    let report = ProductUninstallFinalizerReport {
        status: "partial".to_string(),
        removed: Vec::new(),
        preserved: validated
            .plan
            .files
            .iter()
            .map(|file| FinalizerPreservedItem {
                path: file.path.clone(),
                reason: "product cleanup did not start".to_string(),
            })
            .collect(),
        failed: vec![FinalizerFailure {
            path: validated.plan.report_path.clone(),
            class: class.to_string(),
            error: error.to_string(),
        }],
        residual_copies: validated.plan.residual_copies.clone(),
        manual_recovery: vec![
            "agent integrations may already be disconnected; product files were preserved and uninstall may be retried"
                .to_string(),
        ],
    };
    write_report_atomically(Path::new(&validated.plan.report_path), &report)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannedFilePresence {
    Present,
    AlreadyAbsent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryPresence {
    Present,
    AlreadyAbsent,
}

fn validate_cleanup_input(input: &ProductCleanupInput) -> Result<(), RepoGrammarError> {
    require_safe_absolute(&input.data_dir, "data directory")?;
    let data_metadata = fs::symlink_metadata(&input.data_dir)
        .map_err(|error| invalid(format!("failed to inspect data directory: {error}")))?;
    if data_metadata.file_type().is_symlink() || !data_metadata.is_dir() {
        return Err(invalid(
            "data directory must be a real directory, not a symlink",
        ));
    }
    let expected_authority = input
        .data_dir
        .join("bin")
        .join(crate::application::install::binary_name());
    if !same_lexical_path(&expected_authority, &input.authority_path) {
        return Err(invalid(
            "managed authority is outside the deterministic data-dir/bin location",
        ));
    }
    validate_regular_owned_file(
        &input.authority_path,
        &input.authority_sha256,
        "managed authority",
    )?;
    require_safe_absolute(&input.command_path, "managed command")?;
    if input.command_is_authority {
        if !same_lexical_path(&input.command_path, &input.authority_path)
            || input.command_is_symlink
            || input.command_sha256 != input.authority_sha256
        {
            return Err(invalid("authority command metadata is inconsistent"));
        }
    } else if input.command_is_symlink {
        validate_exact_command_symlink(&input.command_path, &input.authority_path)?;
        if input.command_sha256 != input.authority_sha256 {
            return Err(invalid(
                "managed command symlink hash does not match authority",
            ));
        }
    } else {
        validate_regular_owned_file(
            &input.command_path,
            &input.command_sha256,
            "managed command copy",
        )?;
        if input.command_sha256 != input.authority_sha256 {
            return Err(invalid(
                "managed command copy is not byte-identical to authority",
            ));
        }
    }

    let command_parent = input
        .command_path
        .parent()
        .ok_or_else(|| invalid("managed command has no parent directory"))?;
    let allowed_workers = [
        input.data_dir.join("workers/python/worker.py"),
        command_parent.join("repogrammar-workers/python/worker.py"),
    ];
    if input.worker_files.is_empty() && input.ownership_source == ProductOwnershipSource::Legacy {
        return Err(invalid(
            "legacy managed installation has no bundled worker evidence",
        ));
    }
    let mut unique = HashSet::new();
    for (path, hash) in &input.worker_files {
        require_safe_absolute(path, "bundled worker")?;
        if !allowed_workers
            .iter()
            .any(|allowed| same_lexical_path(allowed, path))
        {
            return Err(invalid(
                "bundled worker is outside a deterministic first-party location",
            ));
        }
        if !unique.insert(normalized_path(path)) {
            return Err(invalid("cleanup plan contains a duplicate bundled worker"));
        }
        validate_regular_owned_file(path, hash, "bundled worker")?;
    }

    match (input.ownership_source, &input.product_receipt_path) {
        (ProductOwnershipSource::Receipt, Some(path)) => {
            let expected = input.data_dir.join("receipts/product-install.json");
            if !same_lexical_path(&expected, path) {
                return Err(invalid(
                    "product receipt is outside the deterministic receipts location",
                ));
            }
            let hash = input
                .product_receipt_sha256
                .as_deref()
                .ok_or_else(|| invalid("receipted installation is missing receipt hash"))?;
            validate_regular_owned_file(path, hash, "product installation receipt")?;
        }
        (ProductOwnershipSource::Legacy, None) => {
            if input.product_receipt_sha256.is_some() {
                return Err(invalid(
                    "legacy installation unexpectedly recorded a receipt hash",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "product ownership source and receipt evidence are inconsistent",
            ));
        }
    }
    Ok(())
}

fn snapshot_cleanup_files(
    input: &ProductCleanupInput,
) -> Result<Vec<PlannedFile>, RepoGrammarError> {
    let mut files = Vec::new();
    if !input.command_is_authority {
        files.push(snapshot_file(
            &input.command_path,
            ProductFileRole::Command,
            if input.command_is_symlink {
                PlannedFileKind::SymlinkToAuthority
            } else {
                PlannedFileKind::Regular
            },
            &input.command_sha256,
            Some(&input.authority_path),
        )?);
    }
    let mut workers = input.worker_files.clone();
    workers.sort_by_key(|worker| normalized_path(&worker.0));
    for (path, hash) in workers {
        files.push(snapshot_file(
            &path,
            ProductFileRole::Worker,
            PlannedFileKind::Regular,
            &hash,
            None,
        )?);
    }
    if let (Some(path), Some(hash)) = (
        input.product_receipt_path.as_deref(),
        input.product_receipt_sha256.as_deref(),
    ) {
        files.push(snapshot_file(
            path,
            ProductFileRole::ProductReceipt,
            PlannedFileKind::Regular,
            hash,
            None,
        )?);
    }
    // The authority is intentionally and unconditionally last.
    files.push(snapshot_file(
        &input.authority_path,
        ProductFileRole::Authority,
        PlannedFileKind::Regular,
        &input.authority_sha256,
        None,
    )?);
    Ok(files)
}

fn snapshot_file(
    path: &Path,
    role: ProductFileRole,
    kind: PlannedFileKind,
    expected_sha256: &str,
    authority: Option<&Path>,
) -> Result<PlannedFile, RepoGrammarError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("failed to snapshot {}: {error}", role.as_str())))?;
    let symlink_target = match kind {
        PlannedFileKind::Regular => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(format!(
                    "{} must be a regular file during finalizer preparation",
                    role.as_str()
                )));
            }
            if sha256_regular_file(path)? != expected_sha256 {
                return Err(invalid(format!(
                    "{} hash drifted during finalizer preparation",
                    role.as_str()
                )));
            }
            None
        }
        PlannedFileKind::SymlinkToAuthority => {
            let authority = authority.ok_or_else(|| invalid("command symlink lacks authority"))?;
            validate_exact_command_symlink(path, authority)?;
            Some(path_text(authority, "authority path")?)
        }
    };
    let parent = snapshot_parent(path)?;
    Ok(PlannedFile {
        path: path_text(path, role.as_str())?,
        role,
        kind,
        sha256: expected_sha256.to_string(),
        symlink_target,
        identity: capture_file_identity(path, &metadata, role.as_str())?,
        parent,
    })
}

fn snapshot_parent(path: &Path) -> Result<ParentIdentity, RepoGrammarError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("owned file has no parent directory"))?;
    require_no_symlink_components(parent, "owned file parent")?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| invalid(format!("failed to inspect owned file parent: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("owned file parent must be a real directory"));
    }
    let canonical = fs::canonicalize(parent)
        .map_err(|error| invalid(format!("failed to canonicalize owned file parent: {error}")))?;
    Ok(ParentIdentity {
        path: path_text(parent, "owned file parent")?,
        canonical_path: path_text(&canonical, "canonical owned file parent")?,
        identity: capture_file_identity(parent, &metadata, "owned file parent")?,
    })
}

fn derive_empty_directories(
    input: &ProductCleanupInput,
) -> Result<Vec<PlannedDirectory>, RepoGrammarError> {
    let mut directories = vec![
        input.data_dir.join("workers/python"),
        input.data_dir.join("workers"),
    ];
    let command_parent = input
        .command_path
        .parent()
        .expect("validated command parent");
    directories.push(command_parent.join("repogrammar-workers/python"));
    directories.push(command_parent.join("repogrammar-workers"));
    if input.product_receipt_path.is_some() {
        directories.push(input.data_dir.join("receipts"));
    }
    directories.push(input.data_dir.join("install/receipts"));
    directories.push(input.data_dir.join("install"));
    directories.push(input.data_dir.join("bin"));
    directories.push(input.data_dir.clone());
    canonicalize_empty_directory_order(&mut directories);
    directories
        .into_iter()
        .map(|path| snapshot_directory_candidate(&path))
        .collect()
}

fn snapshot_directory_candidate(path: &Path) -> Result<PlannedDirectory, RepoGrammarError> {
    require_safe_absolute(path, "empty installation directory")?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(invalid(
                    "empty installation directory candidate must be a real directory",
                ));
            }
            require_no_symlink_components(path, "empty installation directory")?;
            let canonical = fs::canonicalize(path).map_err(|error| {
                invalid(format!(
                    "failed to canonicalize empty installation directory: {error}"
                ))
            })?;
            Ok(PlannedDirectory {
                path: path_text(path, "empty installation directory")?,
                canonical_path: Some(path_text(
                    &canonical,
                    "canonical empty installation directory",
                )?),
                identity: Some(capture_file_identity(
                    path,
                    &metadata,
                    "empty installation directory",
                )?),
                parent: Some(snapshot_parent(path)?),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(PlannedDirectory {
            path: path_text(path, "empty installation directory")?,
            canonical_path: None,
            identity: None,
            parent: None,
        }),
        Err(error) => Err(invalid(format!(
            "failed to inspect empty installation directory: {error}"
        ))),
    }
}

fn canonicalize_empty_directory_order(directories: &mut Vec<PathBuf>) {
    directories.sort_by(|left, right| {
        right
            .components()
            .count()
            .cmp(&left.components().count())
            .then_with(|| normalized_path(left).cmp(&normalized_path(right)))
    });
    directories.dedup_by(|left, right| same_lexical_path(left, right));
}

fn revalidate_all_planned_files(plan: &CleanupPlan) -> Result<(), RepoGrammarError> {
    for file in &plan.files {
        revalidate_planned_file(file, &plan.authority_path)?;
    }
    for directory in &plan.empty_directories {
        revalidate_empty_directory_candidate(directory, plan)?;
    }
    Ok(())
}

fn revalidate_planned_file(
    file: &PlannedFile,
    authority_path: &str,
) -> Result<PlannedFilePresence, RepoGrammarError> {
    let path = Path::new(&file.path);
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // Idempotent retries may run after the first finalizer already
            // removed both the owned file and its now-empty parent directory.
            // There is no deletion target left to redirect through a replaced
            // parent, so absence is safe to record without requiring the old
            // parent identity to remain present.
            return Ok(PlannedFilePresence::AlreadyAbsent);
        }
        Err(error) => {
            return Err(invalid(format!("failed to re-inspect owned file: {error}")));
        }
    };
    revalidate_parent(&file.parent)?;
    if !file_identity_matches_path(&file.identity, path, &metadata, "owned file")? {
        return Err(invalid(
            "owned file identity changed after finalizer preflight",
        ));
    }
    match file.kind {
        PlannedFileKind::Regular => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(
                    "owned regular file was replaced by a non-file or symlink",
                ));
            }
            if sha256_regular_file(path)? != file.sha256 {
                return Err(invalid("owned file hash changed after finalizer preflight"));
            }
        }
        PlannedFileKind::SymlinkToAuthority => {
            let expected = file
                .symlink_target
                .as_deref()
                .ok_or_else(|| invalid("command symlink target is missing from cleanup plan"))?;
            if expected != authority_path {
                return Err(invalid(
                    "command symlink target disagrees with plan authority",
                ));
            }
            validate_exact_command_symlink(path, Path::new(expected))?;
        }
    }
    Ok(PlannedFilePresence::Present)
}

fn revalidate_parent(parent: &ParentIdentity) -> Result<(), RepoGrammarError> {
    let path = Path::new(&parent.path);
    require_no_symlink_components(path, "owned file parent")?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("owned file parent disappeared or changed: {error}")))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || !file_identity_matches_path(&parent.identity, path, &metadata, "owned file parent")?
    {
        return Err(invalid("owned file parent directory identity changed"));
    }
    let canonical = fs::canonicalize(path)
        .map_err(|error| invalid(format!("failed to canonicalize owned file parent: {error}")))?;
    if !same_lexical_path(&canonical, Path::new(&parent.canonical_path)) {
        return Err(invalid("owned file parent canonical path changed"));
    }
    Ok(())
}

fn revalidate_plan_file(
    validated: &ValidatedProductUninstallFinalizer,
) -> Result<(), RepoGrammarError> {
    let metadata = regular_nonsymlink_metadata(&validated.plan_path, "cleanup plan")?;
    if !file_identity_matches_path(
        &validated.plan_identity,
        &validated.plan_path,
        &metadata,
        "cleanup plan",
    )? || sha256_regular_file(&validated.plan_path)? != validated.plan_sha256
    {
        return Err(invalid("cleanup plan changed after helper validation"));
    }
    Ok(())
}

fn revalidate_empty_directory_candidate(
    directory: &PlannedDirectory,
    plan: &CleanupPlan,
) -> Result<DirectoryPresence, RepoGrammarError> {
    let path = Path::new(&directory.path);
    require_safe_absolute(path, "empty installation directory")?;
    let allowed = derived_allowed_directories(plan)?;
    if !allowed
        .iter()
        .any(|candidate| same_lexical_path(candidate, path))
    {
        return Err(invalid(
            "cleanup plan contains an unrecognized directory deletion",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(invalid("empty installation directory was replaced"));
            }
            require_no_symlink_components(path, "empty installation directory")?;
            let expected_identity = directory.identity.as_ref().ok_or_else(|| {
                invalid("empty installation directory appeared after finalizer preflight")
            })?;
            if !file_identity_matches_path(
                expected_identity,
                path,
                &metadata,
                "empty installation directory",
            )? {
                return Err(invalid("empty installation directory identity changed"));
            }
            let canonical = fs::canonicalize(path).map_err(|error| {
                invalid(format!(
                    "failed to canonicalize empty installation directory: {error}"
                ))
            })?;
            if directory
                .canonical_path
                .as_deref()
                .is_none_or(|expected| !same_lexical_path(&canonical, Path::new(expected)))
            {
                return Err(invalid(
                    "empty installation directory canonical path changed",
                ));
            }
            Ok(DirectoryPresence::Present)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok(DirectoryPresence::AlreadyAbsent)
        }
        Err(error) => Err(invalid(format!(
            "failed to inspect empty installation directory: {error}"
        ))),
    }
}

fn validate_plan_layout(plan: &CleanupPlan, plan_path: &Path) -> Result<(), RepoGrammarError> {
    require_safe_absolute(Path::new(&plan.data_dir), "plan data directory")?;
    require_safe_absolute(Path::new(&plan.authority_path), "plan authority")?;
    require_safe_absolute(Path::new(&plan.helper_path), "plan helper")?;
    require_safe_absolute(Path::new(&plan.report_path), "plan report")?;
    require_safe_absolute(plan_path, "cleanup plan")?;
    if plan.parent_pid == 0
        || !is_sha256(&plan.helper_sha256)
        || !is_sha256(&plan.commit_token_sha256)
    {
        return Err(invalid("cleanup plan header is malformed"));
    }
    let private_dir = plan_path
        .parent()
        .ok_or_else(|| invalid("cleanup plan has no private parent directory"))?;
    if plan_path.file_name().and_then(|name| name.to_str()) != Some(PLAN_FILE_NAME)
        || !same_lexical_path(
            &private_dir.join(HELPER_FILE_NAME),
            Path::new(&plan.helper_path),
        )
        || !same_lexical_path(
            &private_dir.join(REPORT_FILE_NAME),
            Path::new(&plan.report_path),
        )
        || !private_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(FINALIZER_DIRECTORY_PREFIX))
    {
        return Err(invalid(
            "cleanup plan is not anchored to its private finalizer directory",
        ));
    }
    let expected_authority = Path::new(&plan.data_dir)
        .join("bin")
        .join(crate::application::install::binary_name());
    if !same_lexical_path(&expected_authority, Path::new(&plan.authority_path)) {
        return Err(invalid("cleanup plan authority escaped data-dir/bin"));
    }
    if plan.files.is_empty()
        || plan.files.last().map(|file| file.role) != Some(ProductFileRole::Authority)
    {
        return Err(invalid("cleanup plan must delete the authority last"));
    }

    let mut paths = HashSet::new();
    let mut stage = 0_u8;
    let mut authority_count = 0;
    let mut receipt_count = 0;
    let mut command_count = 0;
    for file in &plan.files {
        let path = Path::new(&file.path);
        require_safe_absolute(path, "planned owned file")?;
        if !paths.insert(normalized_path(path)) {
            return Err(invalid("cleanup plan contains a duplicate file path"));
        }
        if !is_sha256(&file.sha256) {
            return Err(invalid("cleanup plan contains a malformed file hash"));
        }
        match file.role {
            ProductFileRole::Command => {
                if stage != 0 || command_count != 0 || same_lexical_path(path, &expected_authority)
                {
                    return Err(invalid("cleanup plan command order or identity is invalid"));
                }
                command_count += 1;
                if file.kind == PlannedFileKind::SymlinkToAuthority
                    && file.symlink_target.as_deref() != Some(plan.authority_path.as_str())
                {
                    return Err(invalid("cleanup plan command symlink target is invalid"));
                }
            }
            ProductFileRole::Worker => {
                if stage > 1 || file.kind != PlannedFileKind::Regular {
                    return Err(invalid("cleanup plan worker order or kind is invalid"));
                }
                stage = 1;
                if !is_allowed_worker_path(path, Path::new(&plan.data_dir), &plan.files) {
                    return Err(invalid(
                        "cleanup plan worker escaped deterministic locations",
                    ));
                }
            }
            ProductFileRole::ProductReceipt => {
                if stage > 2 || receipt_count != 0 || file.kind != PlannedFileKind::Regular {
                    return Err(invalid("cleanup plan receipt order or kind is invalid"));
                }
                stage = 2;
                receipt_count += 1;
                if !same_lexical_path(
                    path,
                    &Path::new(&plan.data_dir).join("receipts/product-install.json"),
                ) {
                    return Err(invalid("cleanup plan receipt escaped receipts directory"));
                }
            }
            ProductFileRole::Authority => {
                stage = 3;
                authority_count += 1;
                if authority_count != 1
                    || file.kind != PlannedFileKind::Regular
                    || !same_lexical_path(path, &expected_authority)
                {
                    return Err(invalid("cleanup plan authority entry is invalid"));
                }
            }
        }
        validate_parent_record(&file.parent, path)?;
    }
    if authority_count != 1 || stage != 3 {
        return Err(invalid("cleanup plan has no unique final authority entry"));
    }

    let allowed_directories = derived_allowed_directories(plan)?;
    let actual = plan
        .empty_directories
        .iter()
        .map(|directory| normalized_path(Path::new(&directory.path)))
        .collect::<Vec<_>>();
    let expected = allowed_directories
        .iter()
        .map(|path| normalized_path(path))
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(invalid(
            "cleanup plan empty-directory allowlist is malformed",
        ));
    }
    for directory in &plan.empty_directories {
        validate_planned_directory_record(directory)?;
    }
    if plan.residual_copies.len() > 256
        || plan
            .residual_copies
            .iter()
            .any(|copy| copy.len() > 16 * 1024 || copy.contains('\0'))
    {
        return Err(invalid("cleanup plan residual-copy report is malformed"));
    }
    Ok(())
}

fn validate_planned_directory_record(directory: &PlannedDirectory) -> Result<(), RepoGrammarError> {
    let path = Path::new(&directory.path);
    require_safe_absolute(path, "planned empty installation directory")?;
    match (
        &directory.identity,
        &directory.canonical_path,
        &directory.parent,
    ) {
        (None, None, None) => Ok(()),
        (Some(_), Some(canonical), Some(parent)) => {
            require_safe_absolute(Path::new(canonical), "planned canonical directory")?;
            validate_parent_record(parent, path)
        }
        _ => Err(invalid(
            "planned empty installation directory has incomplete identity evidence",
        )),
    }
}

fn validate_parent_record(parent: &ParentIdentity, child: &Path) -> Result<(), RepoGrammarError> {
    require_safe_absolute(Path::new(&parent.path), "recorded parent")?;
    require_safe_absolute(
        Path::new(&parent.canonical_path),
        "recorded canonical parent",
    )?;
    if child
        .parent()
        .is_none_or(|value| !same_lexical_path(value, Path::new(&parent.path)))
    {
        return Err(invalid(
            "cleanup plan file disagrees with its recorded parent",
        ));
    }
    Ok(())
}

fn is_allowed_worker_path(path: &Path, data_dir: &Path, files: &[PlannedFile]) -> bool {
    if same_lexical_path(path, &data_dir.join("workers/python/worker.py")) {
        return true;
    }
    let Some(command) = files
        .iter()
        .find(|file| file.role == ProductFileRole::Command)
    else {
        return false;
    };
    Path::new(&command.path)
        .parent()
        .is_some_and(|command_parent| {
            same_lexical_path(
                path,
                &command_parent.join("repogrammar-workers/python/worker.py"),
            )
        })
}

fn derived_allowed_directories(plan: &CleanupPlan) -> Result<Vec<PathBuf>, RepoGrammarError> {
    let data_dir = Path::new(&plan.data_dir);
    let mut directories: Vec<PathBuf> =
        vec![data_dir.join("workers/python"), data_dir.join("workers")];
    if let Some(command_parent) = plan
        .files
        .iter()
        .find(|file| file.role == ProductFileRole::Command)
        .and_then(|file| Path::new(&file.path).parent())
    {
        directories.push(command_parent.join("repogrammar-workers/python"));
        directories.push(command_parent.join("repogrammar-workers"));
    }
    for file in &plan.files {
        match file.role {
            ProductFileRole::Worker => {}
            ProductFileRole::ProductReceipt => {
                directories.push(data_dir.join("receipts"));
            }
            _ => {}
        }
    }
    directories.push(data_dir.join("bin"));
    directories.push(data_dir.join("install/receipts"));
    directories.push(data_dir.join("install"));
    directories.push(data_dir.to_path_buf());
    canonicalize_empty_directory_order(&mut directories);
    Ok(directories)
}

fn plan_to_json(plan: &CleanupPlan) -> Value {
    json!({
        "schema_version": PLAN_SCHEMA_VERSION,
        "managed_by": PLAN_MANAGED_BY,
        "kind": PLAN_KIND,
        "data_dir": plan.data_dir,
        "authority_path": plan.authority_path,
        "parent_pid": plan.parent_pid,
        "helper_path": plan.helper_path,
        "helper_sha256": plan.helper_sha256,
        "report_path": plan.report_path,
        "commit_token_sha256": plan.commit_token_sha256,
        "files": plan.files.iter().map(planned_file_to_json).collect::<Vec<_>>(),
        "empty_directories": plan.empty_directories.iter().map(planned_directory_to_json).collect::<Vec<_>>(),
        "residual_copies": plan.residual_copies,
    })
}

fn planned_directory_to_json(directory: &PlannedDirectory) -> Value {
    json!({
        "path": directory.path,
        "canonical_path": directory.canonical_path,
        "identity": directory.identity.as_ref().map(FileIdentity::to_json_value),
        "parent": directory.parent.as_ref().map(|parent| json!({
            "path": parent.path,
            "canonical_path": parent.canonical_path,
            "identity": parent.identity.to_json_value(),
        })),
    })
}

fn planned_file_to_json(file: &PlannedFile) -> Value {
    json!({
        "path": file.path,
        "role": file.role.as_str(),
        "kind": file.kind.as_str(),
        "sha256": file.sha256,
        "symlink_target": file.symlink_target,
        "identity": file.identity.to_json_value(),
        "parent": {
            "path": file.parent.path,
            "canonical_path": file.parent.canonical_path,
            "identity": file.parent.identity.to_json_value(),
        },
    })
}

fn plan_from_json(value: &Value) -> Result<CleanupPlan, RepoGrammarError> {
    let object = strict_object(value, "cleanup plan")?;
    require_exact_keys(
        object,
        &[
            "schema_version",
            "managed_by",
            "kind",
            "data_dir",
            "authority_path",
            "parent_pid",
            "helper_path",
            "helper_sha256",
            "report_path",
            "commit_token_sha256",
            "files",
            "empty_directories",
            "residual_copies",
        ],
        "cleanup plan",
    )?;
    if required_u64(object, "schema_version", "cleanup plan")? != PLAN_SCHEMA_VERSION
        || required_string(object, "managed_by", "cleanup plan")? != PLAN_MANAGED_BY
        || required_string(object, "kind", "cleanup plan")? != PLAN_KIND
    {
        return Err(invalid("cleanup plan header is not owned by RepoGrammar"));
    }
    let parent_pid_u64 = required_u64(object, "parent_pid", "cleanup plan")?;
    let parent_pid = u32::try_from(parent_pid_u64)
        .map_err(|_| invalid("cleanup plan parent pid is out of range"))?;
    let files = required_array(object, "files", "cleanup plan")?
        .iter()
        .map(planned_file_from_json)
        .collect::<Result<Vec<_>, _>>()?;
    let empty_directories = required_array(object, "empty_directories", "cleanup plan")?
        .iter()
        .map(planned_directory_from_json)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CleanupPlan {
        data_dir: required_string(object, "data_dir", "cleanup plan")?.to_string(),
        authority_path: required_string(object, "authority_path", "cleanup plan")?.to_string(),
        parent_pid,
        helper_path: required_string(object, "helper_path", "cleanup plan")?.to_string(),
        helper_sha256: required_string(object, "helper_sha256", "cleanup plan")?.to_string(),
        report_path: required_string(object, "report_path", "cleanup plan")?.to_string(),
        commit_token_sha256: required_string(object, "commit_token_sha256", "cleanup plan")?
            .to_string(),
        files,
        empty_directories,
        residual_copies: string_array(object, "residual_copies", "cleanup plan")?,
    })
}

fn planned_directory_from_json(value: &Value) -> Result<PlannedDirectory, RepoGrammarError> {
    let object = strict_object(value, "planned directory")?;
    require_exact_keys(
        object,
        &["path", "canonical_path", "identity", "parent"],
        "planned directory",
    )?;
    let canonical_path = match object.get("canonical_path") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(invalid("planned directory canonical path is malformed")),
    };
    let identity = match object.get("identity") {
        Some(Value::Null) => None,
        Some(value) => Some(FileIdentity::from_json(value)?),
        None => return Err(invalid("planned directory identity is missing")),
    };
    let parent = match object.get("parent") {
        Some(Value::Null) => None,
        Some(value) => {
            let parent_object = strict_object(value, "planned directory parent")?;
            require_exact_keys(
                parent_object,
                &["path", "canonical_path", "identity"],
                "planned directory parent",
            )?;
            Some(ParentIdentity {
                path: required_string(parent_object, "path", "planned directory parent")?
                    .to_string(),
                canonical_path: required_string(
                    parent_object,
                    "canonical_path",
                    "planned directory parent",
                )?
                .to_string(),
                identity: FileIdentity::from_json(
                    parent_object
                        .get("identity")
                        .ok_or_else(|| invalid("planned directory parent identity is missing"))?,
                )?,
            })
        }
        None => return Err(invalid("planned directory parent is missing")),
    };
    Ok(PlannedDirectory {
        path: required_string(object, "path", "planned directory")?.to_string(),
        canonical_path,
        identity,
        parent,
    })
}

fn planned_file_from_json(value: &Value) -> Result<PlannedFile, RepoGrammarError> {
    let object = strict_object(value, "planned file")?;
    require_exact_keys(
        object,
        &[
            "path",
            "role",
            "kind",
            "sha256",
            "symlink_target",
            "identity",
            "parent",
        ],
        "planned file",
    )?;
    let role = ProductFileRole::parse(required_string(object, "role", "planned file")?)
        .ok_or_else(|| invalid("planned file role is invalid"))?;
    let kind = PlannedFileKind::parse(required_string(object, "kind", "planned file")?)
        .ok_or_else(|| invalid("planned file kind is invalid"))?;
    let symlink_target = match object.get("symlink_target") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(invalid("planned file symlink target is malformed")),
    };
    let parent_value = object
        .get("parent")
        .ok_or_else(|| invalid("planned file parent is missing"))?;
    let parent_object = strict_object(parent_value, "planned file parent")?;
    require_exact_keys(
        parent_object,
        &["path", "canonical_path", "identity"],
        "planned file parent",
    )?;
    Ok(PlannedFile {
        path: required_string(object, "path", "planned file")?.to_string(),
        role,
        kind,
        sha256: required_string(object, "sha256", "planned file")?.to_string(),
        symlink_target,
        identity: FileIdentity::from_json(
            object
                .get("identity")
                .ok_or_else(|| invalid("planned file identity is missing"))?,
        )?,
        parent: ParentIdentity {
            path: required_string(parent_object, "path", "planned file parent")?.to_string(),
            canonical_path: required_string(
                parent_object,
                "canonical_path",
                "planned file parent",
            )?
            .to_string(),
            identity: FileIdentity::from_json(
                parent_object
                    .get("identity")
                    .ok_or_else(|| invalid("planned parent identity is missing"))?,
            )?,
        },
    })
}

fn strict_object<'a>(
    value: &'a Value,
    label: &str,
) -> Result<&'a Map<String, Value>, RepoGrammarError> {
    value
        .as_object()
        .ok_or_else(|| invalid(format!("{label} must be a JSON object")))
}

fn require_exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    label: &str,
) -> Result<(), RepoGrammarError> {
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(invalid(format!(
            "{label} contains missing or unknown fields"
        )));
    }
    Ok(())
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a str, RepoGrammarError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{label}.{key} must be a string")))
}

fn required_u64(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u64, RepoGrammarError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(format!("{label}.{key} must be an unsigned integer")))
}

fn optional_u64(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Option<u64>, RepoGrammarError> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid(format!("{label}.{key} must be null or an unsigned integer"))),
        None => Err(invalid(format!("{label}.{key} is missing"))),
    }
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a Vec<Value>, RepoGrammarError> {
    object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("{label}.{key} must be an array")))
}

fn string_array(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<Vec<String>, RepoGrammarError> {
    required_array(object, key, label)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| invalid(format!("{label}.{key} must contain only strings")))
        })
        .collect()
}

fn invalid(message: impl Into<String>) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.into())
}

fn path_text(path: &Path, label: &str) -> Result<String, RepoGrammarError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| invalid(format!("{label} is not valid UTF-8")))
}

fn normalized_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn same_lexical_path(left: &Path, right: &Path) -> bool {
    normalized_path(left) == normalized_path(right)
}

fn require_safe_absolute(path: &Path, label: &str) -> Result<(), RepoGrammarError> {
    if !path.is_absolute() {
        return Err(invalid(format!("{label} must be absolute")));
    }
    if path
        .to_string_lossy()
        .split(['/', '\\'])
        .any(|component| component == "." || component == "..")
    {
        return Err(invalid(format!(
            "{label} contains unsafe lexical path components"
        )));
    }
    let mut saw_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => saw_root = true,
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir => {
                return Err(invalid(format!(
                    "{label} contains unsafe lexical path components"
                )));
            }
        }
    }
    if !saw_root {
        return Err(invalid(format!("{label} has no filesystem root")));
    }
    Ok(())
}

fn require_no_symlink_components(path: &Path, label: &str) -> Result<(), RepoGrammarError> {
    require_safe_absolute(path, label)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| invalid(format!("failed to inspect {label} component: {error}")))?;
        if metadata.file_type().is_symlink() {
            return Err(invalid(format!("{label} traverses a symlink")));
        }
    }
    Ok(())
}

fn validate_regular_owned_file(
    path: &Path,
    expected_sha256: &str,
    label: &str,
) -> Result<(), RepoGrammarError> {
    if !is_sha256(expected_sha256) {
        return Err(invalid(format!("{label} has a malformed SHA-256")));
    }
    regular_nonsymlink_metadata(path, label)?;
    require_no_symlink_components(
        path.parent()
            .ok_or_else(|| invalid(format!("{label} has no parent")))?,
        label,
    )?;
    if sha256_regular_file(path)? != expected_sha256 {
        return Err(invalid(format!(
            "{label} hash does not match ownership evidence"
        )));
    }
    Ok(())
}

fn validate_exact_command_symlink(
    command: &Path,
    authority: &Path,
) -> Result<(), RepoGrammarError> {
    let metadata = fs::symlink_metadata(command).map_err(|error| {
        invalid(format!(
            "failed to inspect managed command symlink: {error}"
        ))
    })?;
    if !metadata.file_type().is_symlink() {
        return Err(invalid("managed command is not the recorded symlink"));
    }
    require_no_symlink_components(
        command
            .parent()
            .ok_or_else(|| invalid("managed command has no parent"))?,
        "managed command parent",
    )?;
    let target = fs::read_link(command)
        .map_err(|error| invalid(format!("failed to read managed command symlink: {error}")))?;
    let resolved = if target.is_absolute() {
        target
    } else {
        command
            .parent()
            .expect("validated command parent")
            .join(target)
    };
    require_safe_absolute(&resolved, "managed command symlink target")?;
    if !same_lexical_path(&resolved, authority) {
        return Err(invalid(
            "managed command symlink no longer targets the authority",
        ));
    }
    Ok(())
}

fn regular_nonsymlink_metadata(path: &Path, label: &str) -> Result<Metadata, RepoGrammarError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("failed to inspect {label}: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(format!(
            "{label} must be a regular non-symlink file"
        )));
    }
    Ok(metadata)
}

fn sha256_regular_file(path: &Path) -> Result<String, RepoGrammarError> {
    regular_nonsymlink_metadata(path, "hashed file")?;
    let mut file = File::open(path)
        .map_err(|error| invalid(format!("failed to open hashed file: {error}")))?;
    sha256_reader(&mut file).map_err(|error| invalid(format!("failed to hash file: {error}")))
}

fn sha256_reader(reader: &mut impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(lower_hex(hasher.finalize().as_slice()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    lower_hex(Sha256::digest(bytes).as_slice())
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    value
}

fn secure_random_hex(byte_count: usize) -> io::Result<String> {
    let mut bytes = vec![0_u8; byte_count];
    fill_secure_random(&mut bytes)?;
    Ok(lower_hex(&bytes))
}

#[cfg(unix)]
fn fill_secure_random(bytes: &mut [u8]) -> io::Result<()> {
    let mut source = File::open("/dev/urandom")?;
    source.read_exact(bytes)
}

#[cfg(windows)]
fn fill_secure_random(bytes: &mut [u8]) -> io::Result<()> {
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    let len = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "random request is too large"))?;
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            len,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("BCryptGenRandom failed with NTSTATUS {status:#x}"),
        ))
    }
}

#[cfg(not(any(unix, windows)))]
fn fill_secure_random(_bytes: &mut [u8]) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure randomness is unavailable on this platform",
    ))
}

#[cfg(windows)]
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(
        algorithm: *mut std::ffi::c_void,
        buffer: *mut u8,
        buffer_len: u32,
        flags: u32,
    ) -> i32;
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_secure_temp_root(temp_root: &Path) -> Result<(), RepoGrammarError> {
    require_safe_absolute(temp_root, "finalizer temp root")?;
    require_no_symlink_components(temp_root, "finalizer temp root")?;
    let metadata = fs::symlink_metadata(temp_root)
        .map_err(|error| invalid(format!("failed to inspect finalizer temp root: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("finalizer temp root must be a real directory"));
    }
    Ok(())
}

fn create_private_finalizer_directory(temp_root: &Path) -> Result<PathBuf, RepoGrammarError> {
    for _ in 0..128 {
        let nonce = FINALIZER_NONCE.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = temp_root.join(format!(
            "{FINALIZER_DIRECTORY_PREFIX}{}-{now}-{nonce}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                set_private_directory_permissions(&path)?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(invalid(format!(
                    "failed to create private finalizer directory: {error}"
                )));
            }
        }
    }
    Err(invalid("failed to allocate a unique finalizer directory"))
}

fn copy_helper_create_new(source: &Path, destination: &Path) -> Result<(), RepoGrammarError> {
    regular_nonsymlink_metadata(source, "running executable")?;
    let mut input = File::open(source)
        .map_err(|error| invalid(format!("failed to open running executable: {error}")))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            invalid(format!(
                "failed to create private finalizer helper: {error}"
            ))
        })?;
    set_private_helper_permissions(destination)?;
    io::copy(&mut input, &mut output)
        .map_err(|error| invalid(format!("failed to copy private finalizer helper: {error}")))?;
    output
        .sync_all()
        .map_err(|error| invalid(format!("failed to sync private finalizer helper: {error}")))?;
    Ok(())
}

fn write_create_new_private(path: &Path, contents: String) -> Result<(), RepoGrammarError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| invalid(format!("failed to create private cleanup plan: {error}")))?;
    set_private_file_permissions(path)?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| invalid(format!("failed to persist private cleanup plan: {error}")))
}

fn ensure_path_absent(path: &Path, label: &str) -> Result<(), RepoGrammarError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(invalid(format!("{label} destination already exists"))),
        Err(error) => Err(invalid(format!(
            "failed to inspect {label} destination: {error}"
        ))),
    }
}

fn read_limited(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, RepoGrammarError> {
    let mut file =
        File::open(path).map_err(|error| invalid(format!("failed to open {label}: {error}")))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(format!("failed to read {label}: {error}")))?;
    if bytes.len() > limit {
        return Err(invalid(format!("{label} is oversized")));
    }
    Ok(bytes)
}

fn finalizer_commit_token(
    _finalizer_dir: &Path,
    _helper_sha256: &str,
    _parent_pid: u32,
) -> Result<String, RepoGrammarError> {
    secure_random_hex(32).map_err(|error| {
        invalid(format!(
            "failed to create cryptographic finalizer commit capability: {error}"
        ))
    })
}

fn validate_private_plan_path(plan_path: &Path) -> Result<(), RepoGrammarError> {
    require_safe_absolute(plan_path, "cleanup plan")?;
    let parent = plan_path
        .parent()
        .ok_or_else(|| invalid("cleanup plan has no private parent"))?;
    require_no_symlink_components(parent, "cleanup plan parent")?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| invalid(format!("failed to inspect cleanup plan parent: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("cleanup plan parent is not a real directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid("cleanup plan parent permissions are not private"));
        }
    }
    Ok(())
}

fn write_report_atomically(
    report_path: &Path,
    report: &ProductUninstallFinalizerReport,
) -> Result<(), RepoGrammarError> {
    let parent = report_path
        .parent()
        .ok_or_else(|| invalid("cleanup report has no parent"))?;
    validate_private_plan_path(&parent.join(PLAN_FILE_NAME))?;
    let temporary = parent.join(format!(
        ".{REPORT_FILE_NAME}.tmp-{}",
        FINALIZER_NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    let contents = format!("{}\n", report.to_json_value());
    write_create_new_private(&temporary, contents)?;
    if let Err(error) = replace_file_atomically(&temporary, report_path) {
        let _ = fs::remove_file(&temporary);
        return Err(invalid(format!(
            "failed to atomically publish cleanup report: {error}"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), RepoGrammarError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| invalid(format!("failed to secure finalizer directory: {error}")))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), RepoGrammarError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), RepoGrammarError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| invalid(format!("failed to secure finalizer file: {error}")))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), RepoGrammarError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_helper_permissions(path: &Path) -> Result<(), RepoGrammarError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| invalid(format!("failed to secure finalizer helper: {error}")))
}

#[cfg(not(unix))]
fn set_private_helper_permissions(_path: &Path) -> Result<(), RepoGrammarError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::product_installation::{
        ManagedCommandKind, OwnedCommandFile, OwnedProductFile, ProductInstallationPlan,
        ProductOwnershipSource,
    };

    #[cfg(windows)]
    #[test]
    fn windows_directory_identity_fails_closed_after_replacement() {
        let temp_root = fs::canonicalize(std::env::temp_dir()).expect("canonical temp");
        let root = create_private_finalizer_directory(&temp_root).expect("fixture root");
        let directory = root.join("replace-me");
        let replacement = root.join("replacement");
        fs::create_dir(&directory).expect("create original directory");
        fs::create_dir(&replacement).expect("create replacement directory");
        let original_handle =
            open_windows_delete_handle(&directory).expect("open original directory");
        let identity =
            FileIdentity::capture_open_file(&original_handle).expect("capture original identity");
        assert!(identity
            .matches_open_file(&original_handle)
            .expect("match original identity"));
        drop(original_handle);
        fs::remove_dir(&directory).expect("remove original directory");
        fs::rename(&replacement, &directory).expect("replace directory path");
        let replacement_handle =
            open_windows_delete_handle(&directory).expect("open replacement directory");
        assert!(!identity
            .matches_open_file(&replacement_handle)
            .expect("compare replacement identity"));
        drop(replacement_handle);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn zombie_state_is_an_exited_parent_not_a_live_owner() {
        assert_eq!(process_state_is_zombie("Z+\n"), Some(true));
        assert_eq!(process_state_is_zombie("S+\n"), Some(false));
        assert_eq!(process_state_is_zombie("\n"), None);
    }

    struct Fixture {
        root: PathBuf,
        installation: ProductInstallationPlan,
    }

    impl Fixture {
        fn new(receipted: bool) -> Self {
            let temp_root = fs::canonicalize(std::env::temp_dir()).expect("canonical temp");
            let root = create_private_finalizer_directory(&temp_root).expect("fixture root");
            let data = root.join("data/repogrammar");
            let authority = data
                .join("bin")
                .join(crate::application::install::binary_name());
            let worker = data.join("workers/python/worker.py");
            let command = root
                .join("commands")
                .join(crate::application::install::binary_name());
            fs::create_dir_all(authority.parent().unwrap()).unwrap();
            fs::create_dir_all(worker.parent().unwrap()).unwrap();
            fs::create_dir_all(command.parent().unwrap()).unwrap();
            fs::write(&authority, b"owned executable").unwrap();
            fs::write(&worker, b"owned worker").unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&authority, &command).unwrap();
            #[cfg(not(unix))]
            fs::copy(&authority, &command).unwrap();

            let receipt_path = receipted.then(|| data.join("receipts/product-install.json"));
            if let Some(path) = &receipt_path {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, b"owned receipt").unwrap();
            }
            let authority_hash = sha256_regular_file(&authority).unwrap();
            let installation = ProductInstallationPlan {
                source: if receipted {
                    ProductOwnershipSource::Receipt
                } else {
                    ProductOwnershipSource::Legacy
                },
                data_dir: data,
                executable: OwnedProductFile {
                    path: authority.clone(),
                    sha256: authority_hash.clone(),
                },
                command: OwnedCommandFile {
                    file: OwnedProductFile {
                        path: command,
                        sha256: authority_hash,
                    },
                    kind: if cfg!(unix) {
                        ManagedCommandKind::Symlink
                    } else {
                        ManagedCommandKind::Copy
                    },
                },
                workers: vec![OwnedProductFile {
                    path: worker.clone(),
                    sha256: sha256_regular_file(&worker).unwrap(),
                }],
                receipt_path,
            };
            Self { root, installation }
        }

        fn prepare(&self) -> PreparedProductUninstall {
            prepare_product_uninstall_finalizer(
                &self.installation,
                &self.root,
                vec!["/package-manager/repogrammar".to_string()],
            )
            .expect("prepare finalizer")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn capabilities(
        prepared: &PreparedProductUninstall,
    ) -> (
        ValidatedProductUninstallFinalizer,
        FinalizerCommitCapability,
        ParentExitProof,
    ) {
        let validated = validate_product_uninstall_finalizer_invocation(
            &prepared.plan_path,
            &prepared.helper_path,
        )
        .expect("validate helper invocation");
        let commit = validated
            .establish_commit_from_eof(&mut prepared.commit_message().as_slice())
            .expect("commit capability");
        let parent_exit = ParentExitProof {
            plan_sha256: validated.plan_sha256.clone(),
        };
        (validated, commit, parent_exit)
    }

    #[test]
    fn ordinary_product_binary_cannot_invoke_hidden_finalizer() {
        let fixture = Fixture::new(false);
        let prepared = fixture.prepare();
        let error = validate_product_uninstall_finalizer_invocation(
            &prepared.plan_path,
            &fixture.installation.executable.path,
        )
        .unwrap_err();
        assert!(error.to_string().contains("private helper copy"));
        assert!(fixture.installation.executable.path.exists());
    }

    #[test]
    fn plan_canonicalizes_data_and_command_worker_directory_order() {
        let mut fixture = Fixture::new(true);
        let command_worker = fixture
            .installation
            .command
            .file
            .path
            .parent()
            .expect("command parent")
            .join("repogrammar-workers/python/worker.py");
        fs::create_dir_all(command_worker.parent().expect("worker parent")).unwrap();
        fs::write(&command_worker, b"owned command worker").unwrap();
        fixture.installation.workers.push(OwnedProductFile {
            path: command_worker.clone(),
            sha256: sha256_regular_file(&command_worker).unwrap(),
        });

        let prepared = fixture.prepare();
        validate_product_uninstall_finalizer_invocation(&prepared.plan_path, &prepared.helper_path)
            .expect("both deterministic worker roots produce one canonical allowlist");
    }

    #[test]
    fn malformed_or_replaced_plan_is_rejected_before_deletion() {
        let fixture = Fixture::new(false);
        let prepared = fixture.prepare();
        let mut value: Value =
            serde_json::from_slice(&fs::read(&prepared.plan_path).unwrap()).unwrap();
        value["authority_path"] = Value::String("/outside/root".to_string());
        fs::write(&prepared.plan_path, format!("{value}\n")).unwrap();
        let error = validate_product_uninstall_finalizer_invocation(
            &prepared.plan_path,
            &prepared.helper_path,
        )
        .unwrap_err();
        assert!(error.to_string().contains("authority escaped"));
        assert!(fixture.installation.executable.path.exists());
    }

    #[test]
    fn cleanup_plan_rejects_interior_dot_path_spelling() {
        let fixture = Fixture::new(false);
        let prepared = fixture.prepare();
        let mut value: Value =
            serde_json::from_slice(&fs::read(&prepared.plan_path).unwrap()).unwrap();
        let original = value["files"][0]["path"].as_str().unwrap();
        let path = Path::new(original);
        value["files"][0]["path"] = Value::String(
            path.parent()
                .unwrap()
                .join(".")
                .join(path.file_name().unwrap())
                .display()
                .to_string(),
        );
        fs::write(&prepared.plan_path, format!("{value}\n")).unwrap();
        let error = validate_product_uninstall_finalizer_invocation(
            &prepared.plan_path,
            &prepared.helper_path,
        )
        .expect_err("interior dot must be rejected");
        assert!(error.to_string().contains("unsafe lexical path"));
    }

    #[derive(Default)]
    struct RecordingRemoval {
        roles: Vec<ProductFileRole>,
        fail_role: Option<ProductFileRole>,
        fail_directory: bool,
    }

    impl FinalizerRemoval for RecordingRemoval {
        fn remove_file(&mut self, file: &PlannedFile) -> io::Result<()> {
            let path = Path::new(&file.path);
            let role = file.role;
            self.roles.push(role);
            if self.fail_role == Some(role) {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected"));
            }
            fs::remove_file(path)
        }

        fn remove_directory(&mut self, directory: &PlannedDirectory) -> io::Result<()> {
            if self.fail_directory {
                return Err(io::Error::other("injected directory"));
            }
            fs::remove_dir(&directory.path)
        }
    }

    #[test]
    fn finalizer_deletes_in_safe_order_and_is_idempotent() {
        let fixture = Fixture::new(true);
        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        let mut removal = RecordingRemoval::default();
        let report = execute_product_uninstall_finalizer_with_removal(
            validated,
            commit,
            parent_exit,
            &mut removal,
        )
        .expect("execute finalizer");
        assert!(report.is_complete());
        assert_eq!(
            removal.roles,
            vec![
                ProductFileRole::Command,
                ProductFileRole::Worker,
                ProductFileRole::ProductReceipt,
                ProductFileRole::Authority,
            ]
        );
        assert!(!fixture.installation.executable.path.exists());

        let (validated, commit, parent_exit) = capabilities(&prepared);
        let second = execute_product_uninstall_finalizer(validated, commit, parent_exit)
            .expect("idempotent finalizer");
        assert!(second.is_complete());
        assert!(second
            .removed
            .iter()
            .all(|item| item.disposition == "already_absent"));
    }

    #[test]
    fn receipted_zero_worker_install_removes_empty_agent_and_product_directories() {
        let mut fixture = Fixture::new(true);
        for worker in &fixture.installation.workers {
            fs::remove_file(&worker.path).expect("remove optional worker");
        }
        fixture.installation.workers.clear();
        let agent_receipts = fixture.installation.data_dir.join("install/receipts");
        fs::create_dir_all(&agent_receipts).expect("agent receipt directory");

        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        let report = execute_product_uninstall_finalizer(validated, commit, parent_exit)
            .expect("zero-worker receipt uninstall");

        assert!(report.is_complete());
        assert!(!fixture.installation.data_dir.exists());
        assert!(!agent_receipts.exists());
    }

    #[test]
    fn command_and_authority_remove_failures_are_partial_and_preserve_remaining_files() {
        for role in [ProductFileRole::Command, ProductFileRole::Authority] {
            let fixture = Fixture::new(true);
            let prepared = fixture.prepare();
            let (validated, commit, parent_exit) = capabilities(&prepared);
            let mut removal = RecordingRemoval {
                roles: Vec::new(),
                fail_role: Some(role),
                fail_directory: false,
            };
            let report = execute_product_uninstall_finalizer_with_removal(
                validated,
                commit,
                parent_exit,
                &mut removal,
            )
            .expect("structured partial report");
            assert_eq!(report.status, "partial");
            assert!(report
                .failed
                .iter()
                .any(|failure| { failure.class == format!("{}_remove_failed", role.as_str()) }));
            if role == ProductFileRole::Command {
                assert!(fixture.installation.executable.path.exists());
            }
        }
    }

    #[test]
    fn replaced_empty_directory_is_preserved_and_reported_after_file_cleanup() {
        let fixture = Fixture::new(true);
        let agent_receipts = fixture.installation.data_dir.join("install/receipts");
        fs::create_dir_all(&agent_receipts).expect("agent receipt directory");
        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        fs::remove_dir(&agent_receipts).expect("remove snapshotted directory");
        fs::create_dir(&agent_receipts).expect("replace snapshotted directory");

        let report = execute_product_uninstall_finalizer(validated, commit, parent_exit)
            .expect("directory replacement produces report");
        assert_eq!(report.status, "partial");
        assert!(agent_receipts.is_dir());
        assert!(report.failed.iter().any(|failure| {
            failure.class == "directory_ownership_drift"
                && failure.path == path_text(&agent_receipts, "agent receipts").unwrap()
        }));
        assert!(prepared.report_path.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn handle_relative_quarantine_never_deletes_a_replaced_foreign_leaf() {
        let fixture = Fixture::new(true);
        let prepared = fixture.prepare();
        let validated = validate_product_uninstall_finalizer_invocation(
            &prepared.plan_path,
            &prepared.helper_path,
        )
        .expect("validated plan");
        let command = validated
            .plan
            .files
            .iter()
            .find(|file| file.role == ProductFileRole::Command)
            .expect("planned command")
            .clone();
        fs::remove_file(&command.path).expect("replace command");
        fs::write(&command.path, b"foreign replacement").expect("foreign replacement");

        let error = secure_remove_planned_file(&command).expect_err("identity mismatch");
        assert!(error.to_string().contains("preserved at"));
        let parent = Path::new(&command.path).parent().expect("command parent");
        let preserved = fs::read_dir(parent)
            .expect("command parent")
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".repogrammar-uninstall-quarantine-")
            })
            .expect("foreign leaf preserved in quarantine");
        assert_eq!(fs::read(preserved.path()).unwrap(), b"foreign replacement");
    }

    #[test]
    fn directory_remove_failure_is_a_terminal_partial_report() {
        let fixture = Fixture::new(true);
        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        let mut removal = RecordingRemoval {
            roles: Vec::new(),
            fail_role: None,
            fail_directory: true,
        };
        let report = execute_product_uninstall_finalizer_with_removal(
            validated,
            commit,
            parent_exit,
            &mut removal,
        )
        .expect("directory failure report");
        assert_eq!(report.status, "partial");
        assert!(report
            .failed
            .iter()
            .any(|failure| failure.class == "directory_remove_failed"));
        assert!(prepared.report_path.is_file());
    }

    #[test]
    fn conservative_legacy_plan_completes_the_same_finalizer_contract() {
        let fixture = Fixture::new(false);
        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        let report = execute_product_uninstall_finalizer(validated, commit, parent_exit)
            .expect("legacy finalizer");
        assert!(report.is_complete());
        assert!(!fixture.installation.executable.path.exists());
    }

    #[test]
    fn injected_worker_failure_is_truthful_and_preserves_authority() {
        let fixture = Fixture::new(true);
        let prepared = fixture.prepare();
        let (validated, commit, parent_exit) = capabilities(&prepared);
        let mut removal = RecordingRemoval {
            roles: Vec::new(),
            fail_role: Some(ProductFileRole::Worker),
            fail_directory: false,
        };
        let report = execute_product_uninstall_finalizer_with_removal(
            validated,
            commit,
            parent_exit,
            &mut removal,
        )
        .expect("partial report");
        assert_eq!(report.status, "partial");
        assert_eq!(report.failed[0].class, "worker_remove_failed");
        assert!(fixture.installation.executable.path.exists());
        assert!(report
            .preserved
            .iter()
            .any(|item| item.path == fixture.installation.executable.path.display().to_string()));
        assert!(prepared.report_path.is_file());
    }
}
