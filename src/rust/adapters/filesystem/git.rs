//! Native Git context helpers shared by filesystem-facing code.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

/// Upper bound on the NUL-delimited pathspec payload streamed to a single
/// `git check-ignore --stdin` pass. A candidate set large enough to exceed this
/// falls back to the non-git filtering path rather than buffering an unbounded
/// request; at typical path lengths this bound covers far more paths than the
/// discovery visited-entry and accepted-file ceilings allow.
const CHECK_IGNORE_MAX_STDIN_BYTES: usize = 16 * 1024 * 1024;

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

    /// Evaluate Git ignore status for many candidate paths through ONE
    /// `git check-ignore -z --stdin` subprocess instead of one process per path.
    ///
    /// Returns the subset of `project_relative_paths` that Git reports as
    /// ignored, using the same index-aware semantics as [`Self::check_ignore`]
    /// (a tracked path is never reported ignored). Returns `Err(())` when Git is
    /// absent, the request exceeds the bounded stdin size, or Git reports a fatal
    /// error, so callers can apply the same non-git warning fallback that
    /// discovery uses. Input and reply are NUL-delimited so untrusted paths
    /// containing newlines or quotes are handled verbatim.
    pub(crate) fn check_ignore_batch(
        &self,
        project_relative_paths: &[String],
    ) -> Result<BTreeSet<String>, ()> {
        if project_relative_paths.is_empty() {
            return Ok(BTreeSet::new());
        }
        // Git echoes each ignored pathspec verbatim, so map Git-root-relative
        // input back to the caller's project-relative paths for the reply.
        let mut git_to_project: HashMap<String, String> =
            HashMap::with_capacity(project_relative_paths.len());
        let mut stdin_bytes: Vec<u8> = Vec::new();
        for project_relative in project_relative_paths {
            let git_relative = self.git_relative_path(project_relative);
            stdin_bytes.extend_from_slice(git_relative.as_bytes());
            stdin_bytes.push(0);
            if stdin_bytes.len() > CHECK_IGNORE_MAX_STDIN_BYTES {
                return Err(());
            }
            git_to_project.insert(git_relative, project_relative.clone());
        }

        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.worktree_root)
            .args(["check-ignore", "-z", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ())?;
        let mut stdin = child.stdin.take().ok_or(())?;
        // Feed stdin from a separate thread while this thread drains stdout, so
        // a large reply cannot deadlock against a full stdin pipe buffer.
        let writer = thread::spawn(move || {
            let _ = stdin.write_all(&stdin_bytes);
        });
        let output = child.wait_with_output().map_err(|_| ())?;
        let _ = writer.join();
        // 0 => at least one path ignored, 1 => none ignored; anything else is a
        // fatal Git error that must fall back rather than under-report.
        match output.status.code() {
            Some(0) | Some(1) => {}
            _ => return Err(()),
        }

        let mut ignored = BTreeSet::new();
        for chunk in output.stdout.split(|&byte| byte == 0) {
            if chunk.is_empty() {
                continue;
            }
            let git_relative = std::str::from_utf8(chunk).map_err(|_| ())?;
            if let Some(project_relative) = git_to_project.get(git_relative) {
                ignored.insert(project_relative.clone());
            }
        }
        Ok(ignored)
    }

    pub(crate) fn check_ignore_policy(&self, project_relative_path: &str) -> Result<bool, ()> {
        let git_relative_path = self.git_relative_path(project_relative_path);
        match Command::new("git")
            .arg("-C")
            .arg(&self.worktree_root)
            .args(["check-ignore", "-q", "--no-index", "--"])
            .arg(git_relative_path)
            .status()
        {
            Ok(status) if status.success() => Ok(true),
            Ok(status) if status.code() == Some(1) => Ok(false),
            Ok(_) | Err(_) => Err(()),
        }
    }

    pub(crate) fn has_tracked_entries_under(
        &self,
        project_relative_path: &str,
    ) -> Result<bool, ()> {
        let git_relative_path = self.git_relative_path(project_relative_path);
        match Command::new("git")
            .arg("-C")
            .arg(&self.worktree_root)
            .args(["ls-files", "--"])
            .arg(git_relative_path)
            .output()
        {
            Ok(output) if output.status.success() => Ok(!output.stdout.is_empty()),
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

    fn git_init(workspace: &TempWorkspace) -> bool {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(workspace.path())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[test]
    fn check_ignore_batch_matches_per_file_index_aware_semantics() {
        let workspace = TempWorkspace::new("git-context-check-ignore-batch");
        if !git_init(&workspace) {
            return;
        }
        fs::write(
            workspace.path().join(".gitignore"),
            "ignored.ts\nsecret/\ntracked.ts\n",
        )
        .expect("write gitignore");
        fs::create_dir(workspace.path().join("secret")).expect("create secret dir");
        for path in ["ignored.ts", "secret/a.ts", "kept.ts", "tracked.ts"] {
            fs::write(workspace.path().join(path), "content").expect("write candidate");
        }
        // Force-staging tracked.ts (its pattern is ignored) puts it in the
        // index; a tracked path is never reported ignored by the index-aware
        // check even though the pattern still matches it.
        assert!(
            Command::new("git")
                .args(["add", "-f", "tracked.ts"])
                .current_dir(workspace.path())
                .status()
                .map(|status| status.success())
                .unwrap_or(false),
            "git add must stage the tracked file"
        );

        let context = GitContext::resolve(workspace.path()).expect("resolve git context");
        let candidates: Vec<String> = ["ignored.ts", "secret/a.ts", "kept.ts", "tracked.ts"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let batch = context
            .check_ignore_batch(&candidates)
            .expect("batch check-ignore succeeds");
        let per_file: BTreeSet<String> = candidates
            .iter()
            .filter(|path| context.check_ignore(path) == Ok(true))
            .cloned()
            .collect();

        assert_eq!(batch, per_file);
        assert!(batch.contains("ignored.ts"));
        assert!(batch.contains("secret/a.ts"));
        assert!(!batch.contains("kept.ts"));
        assert!(!batch.contains("tracked.ts"));
    }

    #[test]
    fn check_ignore_batch_returns_empty_for_no_candidates() {
        let workspace = TempWorkspace::new("git-context-check-ignore-empty");
        if !git_init(&workspace) {
            return;
        }
        let context = GitContext::resolve(workspace.path()).expect("resolve git context");
        assert!(context
            .check_ignore_batch(&[])
            .expect("empty batch succeeds")
            .is_empty());
    }
}
