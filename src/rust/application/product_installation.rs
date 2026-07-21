//! Persistent ownership evidence for first-party managed machine installations.
//!
//! This module only records and validates ownership. Product deletion is a
//! separate post-exit finalizer concern and must consume an already validated
//! [`ProductInstallationPlan`] instead of accepting caller-selected paths.

use crate::error::RepoGrammarError;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::install::binary_name;

const PRODUCT_RECEIPT_SCHEMA_VERSION: u64 = 1;
const PRODUCT_MANAGED_BY: &str = "repogrammar";
const PRODUCT_INSTALLATION_KIND: &str = "first-party-managed";
const PRODUCT_RECEIPT_FILE: &str = "product-install.json";
const PYTHON_WORKER_RELATIVE: &str = "workers/python/worker.py";
const COMMAND_WORKER_RELATIVE: &str = "repogrammar-workers/python/worker.py";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductInstallationRequest {
    pub data_dir: PathBuf,
    pub executable_path: PathBuf,
    pub command_path: PathBuf,
    /// The process that is requesting legacy inference. Legacy ownership is
    /// accepted only when it is the authority or is byte-identical to it.
    pub current_executable_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductOwnershipSource {
    Receipt,
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedCommandKind {
    Authority,
    Symlink,
    Copy,
}

impl ManagedCommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authority => "authority",
            Self::Symlink => "symlink",
            Self::Copy => "copy",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "authority" => Some(Self::Authority),
            "symlink" => Some(Self::Symlink),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedProductFile {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedCommandFile {
    pub file: OwnedProductFile,
    pub kind: ManagedCommandKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductInstallationPlan {
    pub source: ProductOwnershipSource,
    pub data_dir: PathBuf,
    pub executable: OwnedProductFile,
    pub command: OwnedCommandFile,
    pub workers: Vec<OwnedProductFile>,
    /// `None` for a conservatively inferred legacy installation.
    pub receipt_path: Option<PathBuf>,
}

/// Opaque transaction token used by the install service to restore the exact
/// previous receipt if any later installation step fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductReceiptWrite {
    path: PathBuf,
    previous_contents: Option<Vec<u8>>,
}

pub fn product_receipt_path(data_dir: &Path) -> PathBuf {
    data_dir.join("receipts").join(PRODUCT_RECEIPT_FILE)
}

/// Return the exact command path owned by an existing product receipt.
///
/// Absence is the only case that permits callers to discover a default command
/// directory. A present receipt is validated through the same strict fixed-path
/// parser used before receipt refresh, so malformed or foreign ownership
/// evidence fails closed instead of being bypassed by PATH order. Live command
/// bytes are intentionally not required here because the install transaction
/// owns repair of a missing or stale managed command.
pub fn receipted_command_path(data_dir: &Path) -> Result<Option<PathBuf>, RepoGrammarError> {
    let receipt_path = product_receipt_path(data_dir);
    match fs::symlink_metadata(&receipt_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(
                    "product installation receipt must be a regular file; refusing ownership inference",
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(invalid(format!(
                "failed to inspect product installation receipt: {error}"
            )))
        }
    }

    let bytes = fs::read(&receipt_path).map_err(|error| {
        invalid(format!(
            "failed to read product installation receipt: {error}"
        ))
    })?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("product installation receipt is malformed"))?;
    let command_path = parse_owned_command(&value["command"])?.file.path;
    let request = ProductInstallationRequest {
        data_dir: data_dir.to_path_buf(),
        executable_path: expected_authority(data_dir),
        command_path: command_path.clone(),
        current_executable_path: None,
    };
    validate_existing_receipt_for_refresh(&request, &bytes)?;
    require_safe_descendant(data_dir, &receipt_path, "product receipt", false)?;
    let command_dir = command_path
        .parent()
        .ok_or_else(|| invalid("product receipt command has no parent directory"))?;
    require_no_symlink_components(command_dir, "managed command directory")?;
    require_real_directory(command_dir, "managed command directory")?;
    Ok(Some(command_path))
}

/// Record or refresh the first-party acquisition receipt after the authority,
/// command, and bundled workers have been installed. The write uses a
/// create-new private staging file and atomic rename on supported platforms.
pub fn record_product_installation(
    request: &ProductInstallationRequest,
) -> Result<ProductReceiptWrite, RepoGrammarError> {
    let plan = inspect_live_layout(request, false)?;
    let receipt_path = product_receipt_path(&request.data_dir);
    prepare_receipt_parent(&request.data_dir, &receipt_path)?;
    let previous_contents = capture_regular_file(&receipt_path, "product installation receipt")?;
    if let Some(contents) = previous_contents.as_deref() {
        validate_existing_receipt_for_refresh(request, contents)?;
    }

    let workers = plan
        .workers
        .iter()
        .map(|worker| {
            json!({
                "path": path_string(&worker.path),
                "sha256": worker.sha256,
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "schema_version": PRODUCT_RECEIPT_SCHEMA_VERSION,
        "managed_by": PRODUCT_MANAGED_BY,
        "installation_kind": PRODUCT_INSTALLATION_KIND,
        "version": env!("CARGO_PKG_VERSION"),
        "data_dir": path_string(&plan.data_dir),
        "executable": {
            "path": path_string(&plan.executable.path),
            "sha256": plan.executable.sha256,
        },
        "command": {
            "path": path_string(&plan.command.file.path),
            "kind": plan.command.kind.as_str(),
            "sha256": plan.command.file.sha256,
        },
        "workers": workers,
    });
    let bytes = format!("{value}\n").into_bytes();
    atomic_write_receipt(&receipt_path, &bytes)?;

    // Re-read through the same strict parser before allowing installation to
    // continue. A receipt that cannot authorize a later uninstall is not a
    // successful installation write.
    let write = ProductReceiptWrite {
        path: receipt_path,
        previous_contents,
    };
    if let Err(error) = inspect_receipted(request, &write.path) {
        let rollback = rollback_product_receipt_write(&write);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(invalid(format!(
                "{error}; product receipt rollback failed: {rollback_error}"
            ))),
        };
    }
    Ok(write)
}

pub fn rollback_product_receipt_write(write: &ProductReceiptWrite) -> Result<(), String> {
    match &write.previous_contents {
        Some(contents) => atomic_write_receipt(&write.path, contents)
            .map_err(|error| format!("product receipt restore failed: {error}")),
        None => match fs::symlink_metadata(&write.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err("product receipt rollback refused a non-regular destination".to_string())
            }
            Ok(_) => fs::remove_file(&write.path)
                .map_err(|error| format!("product receipt rollback cleanup failed: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "product receipt rollback inspection failed: {error}"
            )),
        },
    }
}

/// Inspect ownership without mutation. A present receipt is authoritative and
/// must validate completely; it is never bypassed by legacy inference.
pub fn inspect_product_installation(
    request: &ProductInstallationRequest,
) -> Result<ProductInstallationPlan, RepoGrammarError> {
    validate_request_paths(request)?;
    let receipt_path = product_receipt_path(&request.data_dir);
    match fs::symlink_metadata(&receipt_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(invalid(
                    "product installation receipt must be a regular file; refusing ownership inference",
                ));
            }
            inspect_receipted(request, &receipt_path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => inspect_legacy(request),
        Err(error) => Err(invalid(format!(
            "failed to inspect product installation receipt: {error}"
        ))),
    }
}

fn inspect_receipted(
    request: &ProductInstallationRequest,
    receipt_path: &Path,
) -> Result<ProductInstallationPlan, RepoGrammarError> {
    require_safe_descendant(&request.data_dir, receipt_path, "product receipt", false)?;
    let bytes = fs::read(receipt_path).map_err(|error| {
        invalid(format!(
            "failed to read product installation receipt: {error}"
        ))
    })?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("product installation receipt is malformed"))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("product installation receipt is malformed"))?;
    let expected_keys = [
        "schema_version",
        "managed_by",
        "installation_kind",
        "version",
        "data_dir",
        "executable",
        "command",
        "workers",
    ];
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
        || object["schema_version"].as_u64() != Some(PRODUCT_RECEIPT_SCHEMA_VERSION)
        || object["managed_by"].as_str() != Some(PRODUCT_MANAGED_BY)
        || object["installation_kind"].as_str() != Some(PRODUCT_INSTALLATION_KIND)
        || object["version"].as_str().is_none_or(str::is_empty)
    {
        return Err(invalid(
            "product installation receipt has an unsupported schema or ownership identity",
        ));
    }

    let recorded_data_dir = parse_receipt_path(&object["data_dir"], "data_dir")?;
    require_exact_path(
        &recorded_data_dir,
        &request.data_dir,
        "receipt data directory",
    )?;

    let expected_executable = expected_authority(&request.data_dir);
    require_exact_path(
        &request.executable_path,
        &expected_executable,
        "managed executable",
    )?;
    let executable = parse_owned_file(&object["executable"], "executable")?;
    require_exact_path(&executable.path, &expected_executable, "receipt executable")?;
    validate_owned_regular_file(&request.data_dir, &executable, "managed executable")?;

    let command = parse_owned_command(&object["command"])?;
    require_exact_path(&command.file.path, &request.command_path, "receipt command")?;
    validate_command(request, &command, &executable)?;

    let worker_values = object["workers"]
        .as_array()
        .ok_or_else(|| invalid("product installation receipt workers must be an array"))?;
    let expected_workers = expected_worker_paths(request);
    let mut seen = HashSet::new();
    let mut workers = Vec::new();
    for value in worker_values {
        let worker = parse_owned_file(value, "worker")?;
        let key = path_string(&worker.path);
        if !seen.insert(key) {
            return Err(invalid(
                "product installation receipt contains a duplicate worker path",
            ));
        }
        let expected = expected_workers
            .iter()
            .find(|candidate| exact_path(candidate, &worker.path))
            .ok_or_else(|| {
                invalid("product installation receipt contains a non-first-party worker path")
            })?;
        validate_owned_regular_file(
            expected_root_for_worker(request, expected),
            &worker,
            "worker",
        )?;
        workers.push(worker);
    }
    for expected in expected_workers {
        if path_lexists(&expected)
            && !workers
                .iter()
                .any(|worker| exact_path(&worker.path, &expected))
        {
            return Err(invalid(
                "product installation receipt omits an existing deterministic worker; reinstall RepoGrammar once to refresh ownership evidence",
            ));
        }
    }
    workers.sort_by_key(|worker| path_string(&worker.path));

    Ok(ProductInstallationPlan {
        source: ProductOwnershipSource::Receipt,
        data_dir: request.data_dir.clone(),
        executable,
        command,
        workers,
        receipt_path: Some(receipt_path.to_path_buf()),
    })
}

/// A reinstall may legitimately change owned hashes before refreshing the
/// receipt, but it must never overwrite malformed or foreign ownership
/// evidence. Validate the full receipt shape and fixed paths without comparing
/// the recorded hashes to the newly installed bytes.
fn validate_existing_receipt_for_refresh(
    request: &ProductInstallationRequest,
    bytes: &[u8],
) -> Result<(), RepoGrammarError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| invalid("product installation receipt is malformed"))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("product installation receipt is malformed"))?;
    let expected_keys = [
        "schema_version",
        "managed_by",
        "installation_kind",
        "version",
        "data_dir",
        "executable",
        "command",
        "workers",
    ];
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
        || object["schema_version"].as_u64() != Some(PRODUCT_RECEIPT_SCHEMA_VERSION)
        || object["managed_by"].as_str() != Some(PRODUCT_MANAGED_BY)
        || object["installation_kind"].as_str() != Some(PRODUCT_INSTALLATION_KIND)
        || object["version"].as_str().is_none_or(str::is_empty)
    {
        return Err(invalid(
            "product installation receipt has an unsupported schema or ownership identity",
        ));
    }
    require_exact_path(
        &parse_receipt_path(&object["data_dir"], "data_dir")?,
        &request.data_dir,
        "receipt data directory",
    )?;
    require_exact_path(
        &parse_owned_file(&object["executable"], "executable")?.path,
        &expected_authority(&request.data_dir),
        "receipt executable",
    )?;
    require_exact_path(
        &parse_owned_command(&object["command"])?.file.path,
        &request.command_path,
        "receipt command",
    )?;
    let workers = object["workers"]
        .as_array()
        .ok_or_else(|| invalid("product installation receipt workers must be an array"))?;
    let expected = expected_worker_paths(request);
    let mut seen = HashSet::new();
    for value in workers {
        let worker = parse_owned_file(value, "worker")?;
        if !seen.insert(path_string(&worker.path))
            || !expected.iter().any(|path| exact_path(path, &worker.path))
        {
            return Err(invalid(
                "product installation receipt contains a duplicate or non-first-party worker path",
            ));
        }
    }
    Ok(())
}

fn inspect_legacy(
    request: &ProductInstallationRequest,
) -> Result<ProductInstallationPlan, RepoGrammarError> {
    let fail = || {
        invalid(
            "cannot prove ownership of this legacy RepoGrammar installation; reinstall RepoGrammar once to create the product installation receipt, then rerun uninstall",
        )
    };
    let live = inspect_live_layout(request, true).map_err(|_| fail())?;
    let current = request.current_executable_path.as_ref().ok_or_else(fail)?;
    require_safe_absolute(current, "current executable").map_err(|_| fail())?;
    require_no_symlink_components(
        current.parent().ok_or_else(fail)?,
        "current executable parent",
    )
    .map_err(|_| fail())?;
    let current_metadata = fs::symlink_metadata(current).map_err(|_| fail())?;
    let current_matches = if current_metadata.file_type().is_symlink() {
        live.command.kind == ManagedCommandKind::Symlink
            && exact_path(current, &live.command.file.path)
    } else if current_metadata.is_file() {
        exact_path(current, &live.executable.path)
            || exact_path(current, &live.command.file.path)
            || hash_regular_file(current).map_err(|_| fail())? == live.executable.sha256
    } else {
        false
    };
    if !current_matches {
        return Err(fail());
    }
    let primary_worker = request.data_dir.join(PYTHON_WORKER_RELATIVE);
    if !live
        .workers
        .iter()
        .any(|worker| exact_path(&worker.path, &primary_worker))
    {
        return Err(fail());
    }
    Ok(ProductInstallationPlan {
        source: ProductOwnershipSource::Legacy,
        receipt_path: None,
        ..live
    })
}

/// Validate the exact live first-party layout. `require_worker` is used only by
/// legacy inference; a newly recorded direct-Rust install may legitimately
/// have no bundled worker in its layout and therefore owns no worker asset.
fn inspect_live_layout(
    request: &ProductInstallationRequest,
    require_worker: bool,
) -> Result<ProductInstallationPlan, RepoGrammarError> {
    validate_request_paths(request)?;
    let expected_executable = expected_authority(&request.data_dir);
    require_exact_path(
        &request.executable_path,
        &expected_executable,
        "managed executable",
    )?;
    let executable = owned_regular_file(
        &request.data_dir,
        &expected_executable,
        "managed executable",
    )?;
    let command = inspect_live_command(request, &executable)?;

    let mut workers = Vec::new();
    for path in expected_worker_paths(request) {
        let root = expected_root_for_worker(request, &path);
        require_safe_descendant(root, &path, "worker", false)?;
        if path_lexists(&path) {
            workers.push(owned_regular_file(root, &path, "worker")?);
        }
    }
    workers.sort_by_key(|worker| path_string(&worker.path));
    if require_worker && workers.is_empty() {
        return Err(invalid(
            "legacy installation has no deterministic worker evidence",
        ));
    }

    Ok(ProductInstallationPlan {
        source: ProductOwnershipSource::Receipt,
        data_dir: request.data_dir.clone(),
        executable,
        command,
        workers,
        receipt_path: None,
    })
}

fn validate_request_paths(request: &ProductInstallationRequest) -> Result<(), RepoGrammarError> {
    require_safe_absolute(&request.data_dir, "installation data directory")?;
    require_safe_absolute(&request.executable_path, "managed executable")?;
    require_safe_absolute(&request.command_path, "managed command")?;
    require_no_symlink_components(&request.data_dir, "installation data directory")?;
    let data_metadata = fs::symlink_metadata(&request.data_dir).map_err(|error| {
        invalid(format!(
            "failed to inspect installation data directory: {error}"
        ))
    })?;
    if data_metadata.file_type().is_symlink() || !data_metadata.is_dir() {
        return Err(invalid(
            "installation data directory must be a real directory, not a symlink",
        ));
    }
    require_safe_descendant(
        &request.data_dir,
        &request.executable_path,
        "managed executable",
        false,
    )?;
    let command_root = request
        .command_path
        .parent()
        .ok_or_else(|| invalid("managed command has no parent directory"))?;
    require_no_symlink_components(command_root, "managed command directory")?;
    require_real_directory(command_root, "managed command directory")?;
    Ok(())
}

fn expected_authority(data_dir: &Path) -> PathBuf {
    data_dir.join("bin").join(binary_name())
}

fn expected_worker_paths(request: &ProductInstallationRequest) -> Vec<PathBuf> {
    let mut paths = vec![request.data_dir.join(PYTHON_WORKER_RELATIVE)];
    if let Some(command_dir) = request.command_path.parent() {
        let duplicate = command_dir.join(COMMAND_WORKER_RELATIVE);
        if !paths.iter().any(|path| exact_path(path, &duplicate)) {
            paths.push(duplicate);
        }
    }
    paths
}

fn expected_root_for_worker<'a>(
    request: &'a ProductInstallationRequest,
    path: &'a Path,
) -> &'a Path {
    if path.starts_with(&request.data_dir) {
        &request.data_dir
    } else {
        request
            .command_path
            .parent()
            .expect("validated command parent")
    }
}

fn inspect_live_command(
    request: &ProductInstallationRequest,
    executable: &OwnedProductFile,
) -> Result<OwnedCommandFile, RepoGrammarError> {
    let metadata = fs::symlink_metadata(&request.command_path)
        .map_err(|error| invalid(format!("failed to inspect managed command: {error}")))?;
    if exact_path(&request.command_path, &executable.path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid("managed command authority must be a regular file"));
        }
        return Ok(OwnedCommandFile {
            file: executable.clone(),
            kind: ManagedCommandKind::Authority,
        });
    }
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&request.command_path)
            .map_err(|error| invalid(format!("failed to read managed command symlink: {error}")))?;
        let resolved = if target.is_absolute() {
            target
        } else {
            request
                .command_path
                .parent()
                .expect("validated command parent")
                .join(target)
        };
        require_safe_absolute(&resolved, "managed command symlink target")?;
        if !exact_path(&resolved, &executable.path) {
            return Err(invalid(
                "managed command symlink does not point exactly to the managed authority",
            ));
        }
        return Ok(OwnedCommandFile {
            file: OwnedProductFile {
                path: request.command_path.clone(),
                sha256: executable.sha256.clone(),
            },
            kind: ManagedCommandKind::Symlink,
        });
    }
    if !metadata.is_file() {
        return Err(invalid(
            "managed command must be a regular file or exact symlink",
        ));
    }
    let hash = hash_regular_file(&request.command_path)?;
    if hash != executable.sha256 {
        return Err(invalid(
            "managed command copy is not byte-identical to the managed authority",
        ));
    }
    Ok(OwnedCommandFile {
        file: OwnedProductFile {
            path: request.command_path.clone(),
            sha256: hash,
        },
        kind: ManagedCommandKind::Copy,
    })
}

fn validate_command(
    request: &ProductInstallationRequest,
    command: &OwnedCommandFile,
    executable: &OwnedProductFile,
) -> Result<(), RepoGrammarError> {
    let live = inspect_live_command(request, executable)?;
    if live.kind != command.kind || live.file.sha256 != command.file.sha256 {
        return Err(invalid(
            "managed command kind or hash drifted from the product installation receipt",
        ));
    }
    Ok(())
}

fn owned_regular_file(
    root: &Path,
    path: &Path,
    label: &str,
) -> Result<OwnedProductFile, RepoGrammarError> {
    require_safe_descendant(root, path, label, false)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("failed to inspect {label}: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid(format!(
            "{label} must be a regular file, not a symlink"
        )));
    }
    Ok(OwnedProductFile {
        path: path.to_path_buf(),
        sha256: hash_regular_file(path)?,
    })
}

fn validate_owned_regular_file(
    root: &Path,
    recorded: &OwnedProductFile,
    label: &str,
) -> Result<(), RepoGrammarError> {
    let live = owned_regular_file(root, &recorded.path, label)?;
    if live.sha256 != recorded.sha256 {
        return Err(invalid(format!(
            "{label} hash drifted from the product receipt"
        )));
    }
    Ok(())
}

fn parse_owned_file(value: &Value, label: &str) -> Result<OwnedProductFile, RepoGrammarError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(format!("product receipt {label} is malformed")))?;
    if object.len() != 2 || !object.contains_key("path") || !object.contains_key("sha256") {
        return Err(invalid(format!("product receipt {label} is malformed")));
    }
    let path = parse_receipt_path(&object["path"], label)?;
    let sha256 = object["sha256"]
        .as_str()
        .filter(|hash| is_sha256(hash))
        .ok_or_else(|| invalid(format!("product receipt {label} hash is malformed")))?
        .to_string();
    Ok(OwnedProductFile { path, sha256 })
}

fn parse_owned_command(value: &Value) -> Result<OwnedCommandFile, RepoGrammarError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("product receipt command is malformed"))?;
    if object.len() != 3
        || !object.contains_key("path")
        || !object.contains_key("sha256")
        || !object.contains_key("kind")
    {
        return Err(invalid("product receipt command is malformed"));
    }
    let file = parse_owned_file(
        &json!({"path": object["path"], "sha256": object["sha256"]}),
        "command",
    )?;
    let kind = object["kind"]
        .as_str()
        .and_then(ManagedCommandKind::parse)
        .ok_or_else(|| invalid("product receipt command kind is malformed"))?;
    Ok(OwnedCommandFile { file, kind })
}

fn parse_receipt_path(value: &Value, label: &str) -> Result<PathBuf, RepoGrammarError> {
    let raw = value
        .as_str()
        .ok_or_else(|| invalid(format!("product receipt {label} path is malformed")))?;
    let path = PathBuf::from(raw);
    require_safe_absolute(&path, &format!("product receipt {label} path"))?;
    Ok(path)
}

fn prepare_receipt_parent(data_dir: &Path, receipt_path: &Path) -> Result<(), RepoGrammarError> {
    require_safe_absolute(data_dir, "installation data directory")?;
    require_no_symlink_components(data_dir, "installation data directory")?;
    require_real_directory(data_dir, "installation data directory")?;
    let parent = receipt_path
        .parent()
        .ok_or_else(|| invalid("product receipt has no parent directory"))?;
    if !path_lexists(parent) {
        fs::create_dir(parent).map_err(|error| {
            invalid(format!(
                "failed to create product receipt directory: {error}"
            ))
        })?;
    }
    require_safe_descendant(data_dir, parent, "product receipt directory", false)?;
    require_no_symlink_components(parent, "product receipt directory")?;
    require_real_directory(parent, "product receipt directory")
}

fn atomic_write_receipt(path: &Path, contents: &[u8]) -> Result<(), RepoGrammarError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("product receipt has no parent directory"))?;
    require_no_symlink_components(parent, "product receipt directory")?;
    require_real_directory(parent, "product receipt directory")?;
    if path_lexists(path) {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| invalid(format!("failed to inspect product receipt: {error}")))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid(
                "product installation receipt must be a regular file; refusing replacement",
            ));
        }
    }
    let temporary = unique_staging_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|error| {
        invalid(format!(
            "failed to create private product receipt staging file: {error}"
        ))
    })?;
    let staged = (|| -> Result<(), RepoGrammarError> {
        file.write_all(contents)
            .map_err(|error| invalid(format!("failed to write product receipt: {error}")))?;
        file.sync_all()
            .map_err(|error| invalid(format!("failed to sync product receipt: {error}")))?;
        drop(file);
        replace_receipt_atomically(&temporary, path).map_err(|error| {
            invalid(format!(
                "failed to atomically activate product receipt: {error}"
            ))
        })?;
        Ok(())
    })();
    if staged.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    staged
}

#[cfg(not(windows))]
fn replace_receipt_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_receipt_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
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
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
}

fn unique_staging_path(path: &Path) -> PathBuf {
    let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_file_name(format!(
        ".{PRODUCT_RECEIPT_FILE}.tmp-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

fn capture_regular_file(path: &Path, label: &str) -> Result<Option<Vec<u8>>, RepoGrammarError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(invalid(format!("{label} must be a regular file")))
        }
        Ok(_) => fs::read(path)
            .map(Some)
            .map_err(|error| invalid(format!("failed to snapshot {label}: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(invalid(format!("failed to inspect {label}: {error}"))),
    }
}

fn require_safe_absolute(path: &Path, label: &str) -> Result<(), RepoGrammarError> {
    if !path.is_absolute() {
        return Err(invalid(format!("{label} must be absolute")));
    }
    // `Path::components` intentionally normalizes interior `.` components, so
    // inspect the caller-provided lexical spelling first. Ownership evidence
    // must use one unambiguous spelling rather than relying on normalization.
    if path
        .to_string_lossy()
        .split(['/', '\\'])
        .any(|component| component == "." || component == "..")
    {
        return Err(invalid(format!(
            "{label} must not contain dot or parent traversal components"
        )));
    }
    for component in path.components() {
        match component {
            Component::CurDir | Component::ParentDir => {
                return Err(invalid(format!(
                    "{label} must not contain dot or parent traversal components"
                )))
            }
            Component::RootDir | Component::Normal(_) | Component::Prefix(_) => {}
        }
    }
    Ok(())
}

fn require_exact_path(actual: &Path, expected: &Path, label: &str) -> Result<(), RepoGrammarError> {
    require_safe_absolute(actual, label)?;
    if exact_path(actual, expected) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{label} is outside the exact first-party layout"
        )))
    }
}

fn exact_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        path_string(left).eq_ignore_ascii_case(&path_string(right))
    } else {
        left == right
    }
}

fn require_safe_descendant(
    root: &Path,
    target: &Path,
    label: &str,
    allow_leaf_symlink: bool,
) -> Result<(), RepoGrammarError> {
    require_safe_absolute(root, "ownership root")?;
    require_safe_absolute(target, label)?;
    if !target.starts_with(root) {
        return Err(invalid(format!("{label} escapes its ownership root")));
    }
    require_no_symlink_components(root, "ownership root")?;
    require_real_directory(root, "ownership root")?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| invalid(format!("{label} escapes its ownership root")))?;
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    let mut missing_tail = false;
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(invalid(format!("{label} contains unsafe path components")));
        };
        current.push(name);
        if missing_tail {
            continue;
        }
        let is_leaf = index + 1 == components.len();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if !(is_leaf && allow_leaf_symlink) {
                    return Err(invalid(format!("{label} traverses a symlink")));
                }
            }
            Ok(metadata) if !is_leaf && !metadata.is_dir() => {
                return Err(invalid(format!("{label} has a non-directory parent")));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing_tail = true;
            }
            Err(error) => return Err(invalid(format!("failed to inspect {label} path: {error}"))),
        }
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| invalid(format!("failed to canonicalize ownership root: {error}")))?;
    if target.exists()
        && !(allow_leaf_symlink
            && fs::symlink_metadata(target)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false))
    {
        let canonical_target = fs::canonicalize(target)
            .map_err(|error| invalid(format!("failed to canonicalize {label}: {error}")))?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(invalid(format!(
                "{label} escapes its canonical ownership root"
            )));
        }
    }
    Ok(())
}

/// Require every existing component from the filesystem root through `path`
/// to be a real path component. Product installation ownership must be
/// removable under the finalizer's stricter post-exit policy; accepting a
/// lexical alias such as macOS `/tmp` here would otherwise create a receipt
/// that the finalizer correctly refuses later.
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

fn require_real_directory(path: &Path, label: &str) -> Result<(), RepoGrammarError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("failed to inspect {label}: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(invalid(format!(
            "{label} must be a real directory, not a symlink"
        )))
    } else {
        Ok(())
    }
}

fn hash_regular_file(path: &Path) -> Result<String, RepoGrammarError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("failed to inspect owned file: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("owned path must be a regular file"));
    }
    let mut file =
        File::open(path).map_err(|error| invalid(format!("failed to open owned file: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| invalid(format!("failed to hash owned file: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(bytes_to_lower_hex(digest.as_ref()))
}

fn bytes_to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn path_lexists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn invalid(message: impl Into<String>) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace {
        root: PathBuf,
        data: PathBuf,
        command: PathBuf,
        authority: PathBuf,
        current: PathBuf,
    }

    impl Workspace {
        fn new(name: &str) -> Self {
            let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let temp_root = fs::canonicalize(std::env::temp_dir()).expect("canonical temp root");
            let root = temp_root.join(format!(
                "repogrammar-product-install-{name}-{}-{sequence}",
                std::process::id()
            ));
            let data = root.join("data");
            let command_dir = root.join("commands");
            let authority = data.join("bin").join(binary_name());
            let command = command_dir.join(binary_name());
            let current = root.join("running-repogrammar");
            fs::create_dir_all(authority.parent().expect("authority parent")).expect("data bin");
            fs::create_dir_all(&command_dir).expect("command dir");
            fs::write(&authority, b"binary-v1").expect("authority");
            fs::write(&command, b"binary-v1").expect("command");
            fs::write(&current, b"binary-v1").expect("current");
            Self {
                root,
                data,
                command,
                authority,
                current,
            }
        }

        fn request(&self) -> ProductInstallationRequest {
            ProductInstallationRequest {
                data_dir: self.data.clone(),
                executable_path: self.authority.clone(),
                command_path: self.command.clone(),
                current_executable_path: Some(self.current.clone()),
            }
        }

        fn add_workers(&self) {
            let primary = self.data.join(PYTHON_WORKER_RELATIVE);
            let duplicate = self
                .command
                .parent()
                .expect("command parent")
                .join(COMMAND_WORKER_RELATIVE);
            fs::create_dir_all(primary.parent().expect("primary parent")).expect("worker parent");
            fs::create_dir_all(duplicate.parent().expect("duplicate parent"))
                .expect("worker duplicate parent");
            fs::write(primary, b"worker-v1").expect("worker");
            fs::write(duplicate, b"worker-v1").expect("worker duplicate");
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn receipt_round_trip_records_exact_owned_layout() {
        let workspace = Workspace::new("round-trip");
        workspace.add_workers();
        let request = workspace.request();
        record_product_installation(&request).expect("record receipt");

        let plan = inspect_product_installation(&request).expect("inspect receipt");
        assert_eq!(plan.source, ProductOwnershipSource::Receipt);
        assert_eq!(plan.executable.path, workspace.authority);
        assert_eq!(plan.command.kind, ManagedCommandKind::Copy);
        assert_eq!(plan.workers.len(), 2);
        assert_eq!(
            plan.receipt_path,
            Some(product_receipt_path(&workspace.data))
        );
    }

    #[test]
    fn receipt_backed_install_without_workers_remains_owned() {
        let workspace = Workspace::new("zero-workers");
        let request = workspace.request();

        record_product_installation(&request).expect("record zero-worker receipt");
        let plan = inspect_product_installation(&request).expect("inspect zero-worker receipt");

        assert_eq!(plan.source, ProductOwnershipSource::Receipt);
        assert!(plan.workers.is_empty());
        assert_eq!(
            plan.receipt_path,
            Some(product_receipt_path(&workspace.data))
        );
    }

    #[test]
    fn receipt_refresh_can_be_rolled_back_exactly() {
        let workspace = Workspace::new("rollback");
        let request = workspace.request();
        let first = record_product_installation(&request).expect("initial receipt");
        let before = fs::read(product_receipt_path(&workspace.data)).expect("before");
        fs::write(&workspace.authority, b"binary-v2").expect("new authority");
        fs::write(&workspace.command, b"binary-v2").expect("new command");
        let second = record_product_installation(&request).expect("refresh receipt");
        rollback_product_receipt_write(&second).expect("rollback receipt");
        assert_eq!(
            fs::read(product_receipt_path(&workspace.data)).expect("after"),
            before
        );
        rollback_product_receipt_write(&first).expect("remove original receipt");
        assert!(!product_receipt_path(&workspace.data).exists());
    }

    #[test]
    fn malformed_and_hash_drift_receipts_fail_closed_without_writes() {
        let workspace = Workspace::new("malformed");
        let request = workspace.request();
        record_product_installation(&request).expect("receipt");
        let receipt = product_receipt_path(&workspace.data);
        fs::write(&receipt, b"{malformed\n").expect("malform");
        let before = fs::read(&receipt).expect("before");
        assert!(inspect_product_installation(&request).is_err());
        assert_eq!(fs::read(&receipt).expect("after"), before);

        record_product_installation(&request).expect_err("malformed receipt blocks refresh");
        assert_eq!(fs::read(&receipt).expect("preserved"), before);
    }

    #[test]
    fn executable_command_and_worker_hash_drift_are_rejected() {
        for target in ["executable", "command", "worker"] {
            let workspace = Workspace::new(target);
            workspace.add_workers();
            let request = workspace.request();
            record_product_installation(&request).expect("receipt");
            match target {
                "executable" => fs::write(&workspace.authority, b"foreign").expect("drift"),
                "command" => fs::write(&workspace.command, b"foreign").expect("drift"),
                "worker" => fs::write(workspace.data.join(PYTHON_WORKER_RELATIVE), b"foreign")
                    .expect("drift"),
                _ => unreachable!(),
            }
            assert!(inspect_product_installation(&request).is_err(), "{target}");
        }
    }

    #[test]
    fn receipt_rejects_non_absolute_dotdot_unknown_and_duplicate_paths() {
        let workspace = Workspace::new("receipt-paths");
        workspace.add_workers();
        let request = workspace.request();
        record_product_installation(&request).expect("receipt");
        let receipt = product_receipt_path(&workspace.data);
        let original: Value =
            serde_json::from_slice(&fs::read(&receipt).expect("receipt")).expect("json");
        for mutation in ["relative", "dotdot", "unknown", "duplicate"] {
            let mut value = original.clone();
            match mutation {
                "relative" => value["executable"]["path"] = json!("bin/repogrammar"),
                "dotdot" => {
                    value["executable"]["path"] = json!(format!(
                        "{}/bin/../bin/{}",
                        workspace.data.display(),
                        binary_name()
                    ))
                }
                "unknown" => {
                    value["workers"][0]["path"] = json!(workspace.root.join("outside-worker"))
                }
                "duplicate" => {
                    let first = value["workers"][0].clone();
                    value["workers"] = json!([first.clone(), first]);
                }
                _ => unreachable!(),
            }
            fs::write(&receipt, format!("{value}\n")).expect("mutate receipt");
            assert!(
                inspect_product_installation(&request).is_err(),
                "{mutation}"
            );
        }
    }

    #[test]
    fn foreign_command_and_non_file_assets_fail_closed() {
        let workspace = Workspace::new("foreign-command");
        let request = workspace.request();
        fs::write(&workspace.command, b"foreign-command").expect("foreign command");
        assert!(record_product_installation(&request).is_err());
        assert!(!product_receipt_path(&workspace.data).exists());

        fs::remove_file(&workspace.command).expect("remove command");
        fs::create_dir(&workspace.command).expect("directory command");
        assert!(record_product_installation(&request).is_err());
        assert!(!product_receipt_path(&workspace.data).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_receipt_authority_worker_and_parent_traversal_are_rejected() {
        use std::os::unix::fs::symlink;

        let receipt_case = Workspace::new("symlink-receipt");
        let receipt_path = product_receipt_path(&receipt_case.data);
        fs::create_dir_all(receipt_path.parent().expect("receipt parent")).expect("receipts");
        let outside = receipt_case.root.join("outside");
        fs::write(&outside, b"{}").expect("outside");
        symlink(&outside, &receipt_path).expect("receipt symlink");
        assert!(inspect_product_installation(&receipt_case.request()).is_err());

        let authority_case = Workspace::new("symlink-authority");
        fs::remove_file(&authority_case.authority).expect("remove authority");
        symlink(&outside, &authority_case.authority).expect("authority symlink");
        assert!(record_product_installation(&authority_case.request()).is_err());

        let worker_case = Workspace::new("symlink-worker");
        let worker = worker_case.data.join(PYTHON_WORKER_RELATIVE);
        fs::create_dir_all(worker.parent().expect("worker parent")).expect("worker parent");
        symlink(&outside, &worker).expect("worker symlink");
        assert!(record_product_installation(&worker_case.request()).is_err());

        let parent_case = Workspace::new("symlink-parent");
        let workers = parent_case.data.join("workers");
        let outside_dir = parent_case.root.join("outside-dir");
        fs::create_dir(&outside_dir).expect("outside dir");
        symlink(&outside_dir, &workers).expect("parent symlink");
        assert!(record_product_installation(&parent_case.request()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_installation_ancestor_is_rejected_before_receipt_write() {
        use std::os::unix::fs::symlink;

        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_root = fs::canonicalize(std::env::temp_dir()).expect("canonical temp root");
        let container = temp_root.join(format!(
            "repogrammar-product-install-ancestor-{}-{sequence}",
            std::process::id()
        ));
        let real_root = container.join("real");
        let alias_root = container.join("alias");
        let real_data = real_root.join("data");
        let real_authority = real_data.join("bin").join(binary_name());
        let real_command = real_root.join("commands").join(binary_name());
        let real_current = real_root.join("running-repogrammar");
        fs::create_dir_all(real_authority.parent().expect("authority parent")).expect("data bin");
        fs::create_dir_all(real_command.parent().expect("command parent")).expect("command dir");
        fs::write(&real_authority, b"binary-v1").expect("authority");
        fs::write(&real_command, b"binary-v1").expect("command");
        fs::write(&real_current, b"binary-v1").expect("current");
        symlink(&real_root, &alias_root).expect("installation ancestor symlink");

        let request = ProductInstallationRequest {
            data_dir: alias_root.join("data"),
            executable_path: alias_root.join("data/bin").join(binary_name()),
            command_path: alias_root.join("commands").join(binary_name()),
            current_executable_path: Some(alias_root.join("running-repogrammar")),
        };
        let error = record_product_installation(&request).expect_err("reject symlink ancestor");

        assert!(error.to_string().contains("traverses a symlink"));
        assert!(!real_data.join("receipts").exists());
        let _ = fs::remove_file(alias_root);
        let _ = fs::remove_dir_all(container);
    }

    #[cfg(unix)]
    #[test]
    fn lexical_symlink_root_is_rejected_before_receipt_directory_creation() {
        let lexical_temp = Path::new("/tmp");
        let temp_metadata = fs::symlink_metadata(lexical_temp).expect("inspect /tmp");
        if !temp_metadata.file_type().is_symlink() {
            // This regression exercises the standard macOS `/tmp` alias. On
            // Unix hosts where `/tmp` is a real directory there is no lexical
            // symlink ancestor to reject.
            return;
        }

        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "repogrammar-product-install-lexical-tmp-{}-{sequence}",
            std::process::id()
        );
        let canonical_temp = fs::canonicalize(lexical_temp).expect("canonical /tmp");
        let canonical_root = canonical_temp.join(&name);
        let lexical_root = lexical_temp.join(&name);
        let canonical_data = canonical_root.join("data");
        let canonical_command_dir = canonical_root.join("commands");
        let canonical_authority = canonical_data.join("bin").join(binary_name());
        let canonical_command = canonical_command_dir.join(binary_name());
        let canonical_current = canonical_root.join("running-repogrammar");
        fs::create_dir_all(canonical_authority.parent().expect("authority parent"))
            .expect("data bin");
        fs::create_dir_all(&canonical_command_dir).expect("command dir");
        fs::write(&canonical_authority, b"binary-v1").expect("authority");
        fs::write(&canonical_command, b"binary-v1").expect("command");
        fs::write(&canonical_current, b"binary-v1").expect("current");

        let request = ProductInstallationRequest {
            data_dir: lexical_root.join("data"),
            executable_path: lexical_root.join("data/bin").join(binary_name()),
            command_path: lexical_root.join("commands").join(binary_name()),
            current_executable_path: Some(lexical_root.join("running-repogrammar")),
        };
        let error = record_product_installation(&request).expect_err("reject /tmp alias");

        assert!(error.to_string().contains("traverses a symlink"));
        assert!(!canonical_data.join("receipts").exists());
        let _ = fs::remove_dir_all(canonical_root);
    }

    #[test]
    fn canonical_private_tmp_layout_is_accepted_when_available() {
        let private_tmp = Path::new("/private/tmp");
        if !private_tmp.is_dir() {
            return;
        }
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = private_tmp.join(format!(
            "repogrammar-product-install-private-tmp-{}-{sequence}",
            std::process::id()
        ));
        let data = root.join("data");
        let authority = data.join("bin").join(binary_name());
        let command = root.join("commands").join(binary_name());
        let current = root.join("running-repogrammar");
        fs::create_dir_all(authority.parent().expect("authority parent")).expect("data bin");
        fs::create_dir_all(command.parent().expect("command parent")).expect("command dir");
        fs::write(&authority, b"binary-v1").expect("authority");
        fs::write(&command, b"binary-v1").expect("command");
        fs::write(&current, b"binary-v1").expect("current");
        let request = ProductInstallationRequest {
            data_dir: data.clone(),
            executable_path: authority,
            command_path: command,
            current_executable_path: Some(current),
        };

        record_product_installation(&request).expect("record /private/tmp receipt");
        inspect_product_installation(&request).expect("inspect /private/tmp receipt");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn exact_command_symlink_is_allowed_but_drifted_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let workspace = Workspace::new("command-symlink");
        fs::remove_file(&workspace.command).expect("remove copy");
        symlink(&workspace.authority, &workspace.command).expect("command symlink");
        let request = workspace.request();
        record_product_installation(&request).expect("record exact symlink");
        let plan = inspect_product_installation(&request).expect("inspect exact symlink");
        assert_eq!(plan.command.kind, ManagedCommandKind::Symlink);

        fs::remove_file(&workspace.command).expect("remove symlink");
        symlink(&workspace.current, &workspace.command).expect("drift symlink");
        assert!(inspect_product_installation(&request).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn exact_legacy_command_symlink_can_prove_the_running_installation() {
        use std::os::unix::fs::symlink;

        let workspace = Workspace::new("legacy-running-command-symlink");
        workspace.add_workers();
        fs::remove_file(&workspace.command).expect("remove command copy");
        symlink(&workspace.authority, &workspace.command).expect("command symlink");
        let mut request = workspace.request();
        request.current_executable_path = Some(workspace.command.clone());

        let plan = inspect_product_installation(&request).expect("infer exact legacy symlink");
        assert_eq!(plan.source, ProductOwnershipSource::Legacy);
        assert_eq!(plan.command.kind, ManagedCommandKind::Symlink);
    }

    #[test]
    fn exact_legacy_install_is_inferred_but_ambiguous_layout_is_refused() {
        let exact = Workspace::new("legacy-exact");
        exact.add_workers();
        let plan = inspect_product_installation(&exact.request()).expect("legacy exact");
        assert_eq!(plan.source, ProductOwnershipSource::Legacy);
        assert_eq!(plan.workers.len(), 2);
        assert!(plan.receipt_path.is_none());

        let no_worker = Workspace::new("legacy-no-worker");
        let error = inspect_product_installation(&no_worker.request()).expect_err("ambiguous");
        assert!(error.to_string().contains("reinstall RepoGrammar once"));
        assert!(!product_receipt_path(&no_worker.data).exists());

        let wrong_process = Workspace::new("legacy-wrong-process");
        wrong_process.add_workers();
        fs::write(&wrong_process.current, b"not-the-authority").expect("different process");
        let error = inspect_product_installation(&wrong_process.request()).expect_err("ambiguous");
        assert!(error.to_string().contains("reinstall RepoGrammar once"));
        assert!(!product_receipt_path(&wrong_process.data).exists());
    }

    #[test]
    fn inspection_never_claims_unknown_or_user_data_as_deletable() {
        let workspace = Workspace::new("preserve-unknown");
        workspace.add_workers();
        let unknown = workspace.data.join("telemetry/events.ndjson");
        fs::create_dir_all(unknown.parent().expect("unknown parent")).expect("unknown dir");
        fs::write(&unknown, b"user data").expect("unknown data");
        let npm_copy = workspace.root.join("npm/bin/repogrammar");
        fs::create_dir_all(npm_copy.parent().expect("npm parent")).expect("npm dir");
        fs::write(&npm_copy, b"binary-v1").expect("npm copy");

        let plan = inspect_product_installation(&workspace.request()).expect("legacy plan");
        let owned = plan
            .workers
            .iter()
            .map(|file| file.path.clone())
            .chain([plan.executable.path.clone(), plan.command.file.path.clone()])
            .collect::<Vec<_>>();
        assert!(!owned.contains(&unknown));
        assert!(!owned.contains(&npm_copy));
        assert!(unknown.exists());
        assert!(npm_copy.exists());
    }
}
