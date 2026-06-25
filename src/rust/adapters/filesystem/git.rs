//! Native Git context helpers shared by filesystem-facing code.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitContext {
    worktree_root: PathBuf,
    git_dir: PathBuf,
    project_prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitContextResolution {
    NotRepository,
    Unavailable,
}

impl GitContext {
    pub(crate) fn resolve(project_root: &Path) -> Result<Self, GitContextResolution> {
        let canonical_project_root =
            fs::canonicalize(project_root).map_err(|_| GitContextResolution::Unavailable)?;
        let output = match Command::new("git")
            .arg("-C")
            .arg(project_root)
            .args(["rev-parse", "--show-toplevel", "--absolute-git-dir"])
            .output()
        {
            Ok(output) => output,
            Err(_) => return Err(GitContextResolution::Unavailable),
        };
        if !output.status.success() {
            if has_git_metadata_in_ancestors(&canonical_project_root) {
                return Err(GitContextResolution::Unavailable);
            }
            return Err(GitContextResolution::NotRepository);
        }

        let stdout =
            String::from_utf8(output.stdout).map_err(|_| GitContextResolution::Unavailable)?;
        let mut lines = stdout.lines();
        let Some(worktree_root) = lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(GitContextResolution::Unavailable);
        };
        let Some(git_dir) = lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(GitContextResolution::Unavailable);
        };
        let worktree_root =
            fs::canonicalize(worktree_root).map_err(|_| GitContextResolution::Unavailable)?;
        let git_dir = fs::canonicalize(git_dir).map_err(|_| GitContextResolution::Unavailable)?;
        let project_prefix = project_prefix(&canonical_project_root, &worktree_root)
            .ok_or(GitContextResolution::Unavailable)?;

        Ok(Self {
            worktree_root,
            git_dir,
            project_prefix,
        })
    }

    pub(crate) fn git_relative_path(&self, project_relative_path: &str) -> String {
        if self.project_prefix.is_empty() {
            project_relative_path.to_string()
        } else {
            format!("{}/{}", self.project_prefix, project_relative_path)
        }
    }

    pub(crate) fn info_exclude_path(&self) -> PathBuf {
        self.git_dir.join("info").join("exclude")
    }

    pub(crate) fn check_ignore(&self, project_relative_path: &str) -> Result<bool, ()> {
        let git_relative_path = self.git_relative_path(project_relative_path);
        match Command::new("git")
            .arg("-C")
            .arg(&self.worktree_root)
            .args(["check-ignore", "-q", "--"])
            .arg(git_relative_path)
            .status()
        {
            Ok(status) if status.success() => Ok(true),
            Ok(status) if status.code() == Some(1) => Ok(false),
            Ok(_) | Err(_) => Err(()),
        }
    }
}

fn has_git_metadata_in_ancestors(root: &Path) -> bool {
    root.ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
}

fn project_prefix(project_root: &Path, worktree_root: &Path) -> Option<String> {
    let relative = project_root.strip_prefix(worktree_root).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(String::new());
    }
    let parts = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempWorkspace;

    #[test]
    fn resolves_non_repository_without_manual_git_parsing() {
        let workspace = TempWorkspace::new("git-context-non-repo");

        let error = GitContext::resolve(workspace.path()).expect_err("not repository");

        assert_eq!(error, GitContextResolution::NotRepository);
    }

    #[test]
    fn marks_invalid_git_metadata_unavailable() {
        let workspace = TempWorkspace::new("git-context-invalid-git");
        fs::create_dir_all(workspace.path().join(".git")).expect("create invalid git dir");

        let error = GitContext::resolve(workspace.path()).expect_err("invalid git");

        assert_eq!(error, GitContextResolution::Unavailable);
    }
}
