//! Shared deterministic helpers for tests.

use crate::core::model::FamilyPrevalence;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// A deterministic dominant-shaped [`FamilyPrevalence`] for tests that need a
/// prevalence value but do not exercise the classifier itself.
pub fn sample_family_prevalence() -> FamilyPrevalence {
    FamilyPrevalence {
        eligible_peer_count: 2,
        supported_member_count: 2,
        coverage_ratio: Some(1.0),
        competing_ready_family_count: 0,
        largest_competing_support: 0,
        blocked_peer_count: 0,
        unsupported_peer_count: 0,
        classification_reason: "coverage 2/2 with no competing ready family".to_string(),
    }
}

#[derive(Debug)]
pub struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    pub fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "repogrammar-{prefix}-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
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

pub fn create_test_symlink_file(target: &Path, link: &Path) -> bool {
    create_test_symlink(
        || {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(target, link)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(target, link)
            }
        },
        "file",
        target,
        link,
    )
}

pub fn create_test_symlink_dir(target: &Path, link: &Path) -> bool {
    create_test_symlink(
        || {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(target, link)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(target, link)
            }
        },
        "directory",
        target,
        link,
    )
}

fn create_test_symlink(
    create: impl FnOnce() -> io::Result<()>,
    kind: &str,
    target: &Path,
    link: &Path,
) -> bool {
    match create() {
        Ok(()) => true,
        Err(error) if symlink_creation_unavailable(&error) => false,
        Err(error) => panic!(
            "create {kind} symlink from {} to {}: {error}",
            link.display(),
            target.display()
        ),
    }
}

fn symlink_creation_unavailable(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        const ERROR_NOT_SUPPORTED: i32 = 50;
        const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
        matches!(
            error.raw_os_error(),
            Some(ERROR_NOT_SUPPORTED | ERROR_PRIVILEGE_NOT_HELD)
        ) || error.kind() == io::ErrorKind::Unsupported
    }
    #[cfg(not(windows))]
    {
        error.kind() == io::ErrorKind::Unsupported
    }
}
