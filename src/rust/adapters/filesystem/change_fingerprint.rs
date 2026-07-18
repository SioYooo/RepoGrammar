//! Bounded metadata fingerprinting for the opt-in autosync watcher.

use super::discovery::{
    is_default_excluded_directory_name, is_repogrammar_state_directory_name,
    supported_language_for_path,
};
use super::git::{GitContext, GitContextResolution};
use super::resource_limits::{DiscoveryLimits, DiscoveryResourceBudget};
use crate::ports::file_discovery::{DiscoveredLanguage, FileDiscoveryError, GitIgnoreStatus};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Result of one metadata fingerprint pass.
///
/// Alongside the content-free `digest`, the pass carries bounded, path-free
/// observability counters so the daemon can log how Git ignore rules were
/// applied. It never carries repository paths or source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryChangeFingerprint {
    /// Stable content-free digest of the accepted supported-file inventory.
    pub digest: String,
    /// How this pass evaluated Git ignore rules, mirroring discovery's
    /// applied/not-repository/unavailable classification.
    pub git_ignore_status: GitIgnoreStatus,
    /// Count of supported candidate files excluded because Git reported them
    /// ignored this pass. Bounded by the visited-entry budget; never a path.
    pub git_ignored_skipped: u64,
}

pub fn repository_change_fingerprint(
    repository_root: &str,
    max_file_bytes: u64,
) -> Result<String, FileDiscoveryError> {
    Ok(repository_change_fingerprint_report(repository_root, max_file_bytes)?.digest)
}

pub fn repository_change_fingerprint_report(
    repository_root: &str,
    max_file_bytes: u64,
) -> Result<RepositoryChangeFingerprint, FileDiscoveryError> {
    repository_change_fingerprint_with_limits(
        repository_root,
        max_file_bytes,
        DiscoveryLimits::default(),
    )
}

fn repository_change_fingerprint_with_limits(
    repository_root: &str,
    max_file_bytes: u64,
    limits: DiscoveryLimits,
) -> Result<RepositoryChangeFingerprint, FileDiscoveryError> {
    let root = PathBuf::from(repository_root);
    let metadata = fs::symlink_metadata(&root).map_err(|_| {
        FileDiscoveryError::InvalidRoot("repository root is not readable".to_string())
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FileDiscoveryError::InvalidRoot(
            "repository root must be a real directory".to_string(),
        ));
    }
    let canonical_root = fs::canonicalize(&root).map_err(|_| {
        FileDiscoveryError::InvalidRoot("repository root is not readable".to_string())
    })?;
    let mut state = FingerprintState {
        root,
        canonical_root,
        max_file_bytes,
        candidates: Vec::new(),
        budget: DiscoveryResourceBudget::new(limits),
    };
    state.walk(PathBuf::new())?;
    state.finish(repository_root)
}

/// One accepted supported-file candidate, retained in walk order before Git
/// ignore filtering and accepted-budget charging.
struct FingerprintCandidate {
    relative_path: String,
    size: u64,
    modified: Option<String>,
    language: DiscoveredLanguage,
}

struct FingerprintState {
    root: PathBuf,
    canonical_root: PathBuf,
    max_file_bytes: u64,
    candidates: Vec<FingerprintCandidate>,
    budget: DiscoveryResourceBudget,
}

impl FingerprintState {
    fn walk(&mut self, relative_dir: PathBuf) -> Result<(), FileDiscoveryError> {
        self.budget
            .check_directory_depth(path_depth(&relative_dir))?;
        let directory = self.root.join(&relative_dir);
        let read_dir = fs::read_dir(&directory)
            .map_err(|_| FileDiscoveryError::Unavailable("failed to read directory".to_string()))?;
        let mut children = Vec::new();
        for child in read_dir {
            self.budget.record_visited_entry()?;
            children.push(child.map_err(|_| {
                FileDiscoveryError::Unavailable("failed to read directory entry".to_string())
            })?);
        }
        children.sort_by_key(|entry| entry.file_name());

        for child in children {
            let relative = relative_dir.join(child.file_name());
            let Some(relative_path) = repo_relative_string(&relative) else {
                continue;
            };
            let metadata = match fs::symlink_metadata(child.path()) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                let name = relative.file_name().and_then(|value| value.to_str());
                if is_repogrammar_state_directory_name(name)
                    || is_default_excluded_directory_name(name)
                {
                    continue;
                }
                if matches!(
                    fs::canonicalize(child.path()),
                    Ok(canonical) if canonical.starts_with(&self.canonical_root)
                ) {
                    self.walk(relative)?;
                }
                continue;
            }
            if !metadata.is_file() || metadata.len() > self.max_file_bytes {
                continue;
            }
            let Some(language) = supported_language_for_path(&relative_path) else {
                continue;
            };
            if !matches!(
                fs::canonicalize(child.path()),
                Ok(canonical) if canonical.starts_with(&self.canonical_root)
            ) {
                continue;
            }

            // Defer accepted-budget charging until after Git ignore filtering so
            // ignored candidates never consume the accepted-file/byte ceilings,
            // matching the population manual Git-aware discovery charges.
            let modified = metadata.modified().ok().and_then(|value| {
                value.duration_since(UNIX_EPOCH).ok().map(|duration| {
                    format!("{}.{:09}", duration.as_secs(), duration.subsec_nanos())
                })
            });
            self.candidates.push(FingerprintCandidate {
                relative_path,
                size: metadata.len(),
                modified,
                language,
            });
        }
        Ok(())
    }

    fn finish(
        mut self,
        repository_root: &str,
    ) -> Result<RepositoryChangeFingerprint, FileDiscoveryError> {
        let (git_ignore_status, ignored) = resolve_git_ignored(repository_root, &self.candidates);
        let git_ignored_skipped = u64::try_from(ignored.len()).unwrap_or(u64::MAX);

        let mut entries: Vec<String> = Vec::with_capacity(self.candidates.len());
        for candidate in &self.candidates {
            if ignored.contains(&candidate.relative_path) {
                continue;
            }
            // Charge in walk order so a breached ceiling reports the same
            // deterministic observed value regardless of Git ignore state.
            self.budget.record_accepted_file(candidate.size)?;
            entries.push(format!(
                "{}\0{}\0{}\0{}",
                candidate.relative_path,
                candidate.size,
                candidate.modified.as_deref().unwrap_or("unknown"),
                candidate.language.as_str()
            ));
        }
        entries.sort();
        let mut hasher = Sha256::new();
        for entry in entries {
            hasher.update(entry.as_bytes());
            hasher.update([0xff]);
        }
        Ok(RepositoryChangeFingerprint {
            digest: bytes_to_lower_hex(hasher.finalize().as_ref()),
            git_ignore_status,
            git_ignored_skipped,
        })
    }
}

/// Evaluate Git ignore for the candidate inventory with the same status
/// classification and warning-fallback semantics as manual discovery: when Git
/// is unavailable or errors, fall back to no-ignore filtering (keep every
/// candidate) instead of silently dropping files.
fn resolve_git_ignored(
    repository_root: &str,
    candidates: &[FingerprintCandidate],
) -> (GitIgnoreStatus, BTreeSet<String>) {
    match GitContext::resolve(Path::new(repository_root)) {
        Ok(context) => {
            let paths: Vec<String> = candidates
                .iter()
                .map(|candidate| candidate.relative_path.clone())
                .collect();
            match context.check_ignore_batch(&paths) {
                Ok(ignored) => (GitIgnoreStatus::Applied, ignored),
                Err(()) => (GitIgnoreStatus::Unavailable, BTreeSet::new()),
            }
        }
        Err(GitContextResolution::NotRepository) => {
            (GitIgnoreStatus::NotRepository, BTreeSet::new())
        }
        Err(GitContextResolution::Unavailable) => (GitIgnoreStatus::Unavailable, BTreeSet::new()),
    }
}

fn path_depth(path: &Path) -> u64 {
    u64::try_from(path.components().count()).unwrap_or(u64::MAX)
}

fn repo_relative_string(path: &Path) -> Option<String> {
    let parts = path
        .iter()
        .map(|part| part.to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

fn bytes_to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::file_discovery::{
        FileDiscoveryLimitExceeded, FileDiscoveryLimitKind, GitIgnoreStatus, DEFAULT_MAX_FILE_BYTES,
    };
    use crate::test_support::TempWorkspace;
    use std::process::Command;

    fn git_init(workspace: &TempWorkspace) -> bool {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(workspace.path())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn limits(
        accepted_files: u64,
        accepted_bytes: u64,
        visited: u64,
        depth: u64,
    ) -> DiscoveryLimits {
        DiscoveryLimits {
            accepted_files,
            accepted_bytes,
            reported_skipped_paths: 0,
            visited_entries: visited,
            directory_depth: depth,
        }
    }

    #[test]
    fn fingerprint_tracks_ruby_inventory_but_not_language_specific_exclusions() {
        let workspace = TempWorkspace::new("fingerprint-ruby-inventory");
        fs::create_dir(workspace.path().join(".bundle")).expect("create .bundle");
        fs::create_dir(workspace.path().join(".ruby-lsp")).expect("create .ruby-lsp");
        let root = workspace.path().display().to_string();
        let baseline = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint empty Ruby workspace");

        fs::write(workspace.path().join(".bundle/cache.rb"), "ignored")
            .expect("write excluded Ruby source");
        fs::write(workspace.path().join(".ruby-lsp/Gemfile"), "ignored")
            .expect("write excluded Ruby config");
        fs::write(workspace.path().join("Rakefile"), "deferred")
            .expect("write deferred Ruby candidate");
        assert_eq!(
            repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
                .expect("fingerprint excluded Ruby candidates"),
            baseline
        );

        fs::write(workspace.path().join("main.rb"), "inventory")
            .expect("write Ruby source inventory");
        let with_source = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint Ruby source");
        assert_ne!(with_source, baseline);

        fs::write(workspace.path().join("Gemfile"), "inventory")
            .expect("write Ruby config inventory");
        let with_config = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint Ruby config");
        assert_ne!(with_config, with_source);

        fs::write(workspace.path().join(".bundle/kept.ts"), "inventory")
            .expect("write supported non-Ruby source");
        let with_cross_language = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint cross-language source");
        assert_ne!(with_cross_language, with_config);
    }

    #[test]
    fn fingerprint_tracks_php_inventory_but_not_language_specific_exclusions() {
        let workspace = TempWorkspace::new("fingerprint-php-inventory");
        fs::create_dir(workspace.path().join(".composer")).expect("create .composer");
        fs::create_dir(workspace.path().join(".phpunit.cache")).expect("create .phpunit.cache");
        let root = workspace.path().display().to_string();
        let baseline = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint empty PHP workspace");

        fs::write(workspace.path().join(".composer/cache.php"), "ignored")
            .expect("write excluded PHP source");
        fs::write(
            workspace.path().join(".phpunit.cache/phpunit.xml"),
            "ignored",
        )
        .expect("write excluded PHP config");
        fs::write(workspace.path().join("view.phtml"), "deferred")
            .expect("write deferred PHP candidate");
        assert_eq!(
            repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
                .expect("fingerprint excluded PHP candidates"),
            baseline
        );

        fs::write(workspace.path().join("main.php"), "inventory")
            .expect("write PHP source inventory");
        let with_source = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint PHP source");
        assert_ne!(with_source, baseline);

        fs::write(workspace.path().join("composer.json"), "inventory")
            .expect("write PHP config inventory");
        let with_config = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint PHP config");
        assert_ne!(with_config, with_source);

        fs::write(workspace.path().join(".composer/kept.ts"), "inventory")
            .expect("write supported non-PHP source");
        let with_cross_language = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint cross-language source");
        assert_ne!(with_cross_language, with_config);
    }

    #[test]
    fn fingerprint_tracks_swift_inventory_but_not_language_specific_exclusions() {
        let workspace = TempWorkspace::new("fingerprint-swift-inventory");
        fs::create_dir(workspace.path().join(".build")).expect("create .build");
        fs::create_dir(workspace.path().join(".swiftpm")).expect("create .swiftpm");
        let root = workspace.path().display().to_string();
        let baseline = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint empty Swift workspace");

        fs::write(workspace.path().join(".build/cache.swift"), "ignored")
            .expect("write excluded Swift source");
        fs::write(workspace.path().join(".swiftpm/Package.swift"), "ignored")
            .expect("write excluded Swift config");
        fs::write(workspace.path().join("project.pbxproj"), "deferred")
            .expect("write deferred Swift candidate");
        assert_eq!(
            repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
                .expect("fingerprint excluded Swift candidates"),
            baseline
        );

        fs::write(workspace.path().join("main.swift"), "inventory")
            .expect("write Swift source inventory");
        let with_source = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint Swift source");
        assert_ne!(with_source, baseline);

        fs::write(
            workspace.path().join("Package@swift-6.3.swift"),
            "inventory",
        )
        .expect("write Swift config inventory");
        let with_config = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint Swift config");
        assert_ne!(with_config, with_source);

        fs::write(workspace.path().join(".build/kept.ts"), "inventory")
            .expect("write supported non-Swift source");
        let with_cross_language = repository_change_fingerprint(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint cross-language source");
        assert_ne!(with_cross_language, with_config);
    }

    #[test]
    fn fingerprint_accepts_exact_file_and_byte_limits_then_rejects_plus_one() {
        let workspace = TempWorkspace::new("fingerprint-resource-files");
        fs::write(workspace.path().join("a.ts"), "a").expect("write a");
        let root = workspace.path().display().to_string();

        repository_change_fingerprint_with_limits(
            &root,
            DEFAULT_MAX_FILE_BYTES,
            limits(1, 1, 1, 0),
        )
        .expect("exact file and byte limits");
        fs::write(workspace.path().join("b.ts"), "").expect("write zero byte b");
        assert_limit(
            repository_change_fingerprint_with_limits(
                &root,
                DEFAULT_MAX_FILE_BYTES,
                limits(1, 1, 2, 0),
            ),
            FileDiscoveryLimitKind::AcceptedFiles,
            1,
            2,
        );

        let byte_workspace = TempWorkspace::new("fingerprint-resource-bytes");
        fs::write(byte_workspace.path().join("a.ts"), "aa").expect("write bytes");
        let byte_root = byte_workspace.path().display().to_string();
        repository_change_fingerprint_with_limits(
            &byte_root,
            DEFAULT_MAX_FILE_BYTES,
            limits(1, 2, 1, 0),
        )
        .expect("exact byte limit");
        assert_limit(
            repository_change_fingerprint_with_limits(
                &byte_root,
                DEFAULT_MAX_FILE_BYTES,
                limits(1, 1, 1, 0),
            ),
            FileDiscoveryLimitKind::AcceptedBytes,
            1,
            2,
        );
    }

    #[test]
    fn fingerprint_accepts_exact_visited_and_depth_limits_then_rejects_plus_one() {
        let workspace = TempWorkspace::new("fingerprint-resource-walk");
        fs::create_dir(workspace.path().join("one")).expect("create one");
        fs::write(workspace.path().join("one/a.ts"), "a").expect("write a");
        let root = workspace.path().display().to_string();

        repository_change_fingerprint_with_limits(
            &root,
            DEFAULT_MAX_FILE_BYTES,
            limits(1, 1, 2, 1),
        )
        .expect("exact visited and depth limits");
        assert_limit(
            repository_change_fingerprint_with_limits(
                &root,
                DEFAULT_MAX_FILE_BYTES,
                limits(1, 1, 1, 1),
            ),
            FileDiscoveryLimitKind::VisitedEntries,
            1,
            2,
        );
        assert_limit(
            repository_change_fingerprint_with_limits(
                &root,
                DEFAULT_MAX_FILE_BYTES,
                limits(1, 1, 2, 0),
            ),
            FileDiscoveryLimitKind::DirectoryDepth,
            0,
            1,
        );
    }

    #[test]
    fn fingerprint_ignored_candidates_do_not_consume_acceptance_budgets_or_leak() {
        let workspace = TempWorkspace::new("fingerprint-resource-no-leak-secret");
        fs::write(workspace.path().join("too-large.ts"), "xx").expect("write oversized");
        fs::write(workspace.path().join("unsupported.secret"), "secret source")
            .expect("write unsupported");
        let root = workspace.path().display().to_string();

        repository_change_fingerprint_with_limits(&root, 1, limits(0, 0, 2, 0))
            .expect("ignored candidates do not consume accepted budgets");
        let error = repository_change_fingerprint_with_limits(&root, 1, limits(0, 0, 1, 0))
            .expect_err("visited limit must fail");
        let rendered = error.to_string();
        assert!(!rendered.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("too-large.ts"));
    }

    #[test]
    fn fingerprint_excludes_git_ignored_supported_files_from_budget_and_digest() {
        let workspace = TempWorkspace::new("fingerprint-git-ignored-parity");
        if !git_init(&workspace) {
            return;
        }
        fs::write(workspace.path().join(".gitignore"), "ignored.ts\n").expect("write gitignore");
        fs::write(workspace.path().join("ignored.ts"), "ignored").expect("write ignored source");
        fs::write(workspace.path().join("kept.ts"), "kept").expect("write kept source");
        let root = workspace.path().display().to_string();

        let report = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint with Git ignore parity");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(report.git_ignored_skipped, 1);
        let baseline = report.digest.clone();

        // A change confined to the Git-ignored file must not move the digest,
        // so autosync does not trigger a sync discovery would then exclude.
        fs::write(
            workspace.path().join("ignored.ts"),
            "ignored content changed to a different length",
        )
        .expect("rewrite ignored source");
        let after_ignored = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint after ignored change");
        assert_eq!(after_ignored.digest, baseline);
        assert_eq!(after_ignored.git_ignored_skipped, 1);

        // A change to the unignored file still moves the digest.
        fs::write(workspace.path().join("kept.ts"), "kept changed").expect("rewrite kept source");
        let after_kept = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint after tracked change");
        assert_ne!(after_kept.digest, baseline);
    }

    #[test]
    fn fingerprint_git_ignored_files_do_not_breach_accepted_ceiling() {
        let workspace = TempWorkspace::new("fingerprint-git-ignored-ceiling");
        if !git_init(&workspace) {
            return;
        }
        // `gen` is not a default-excluded directory, so only Git ignore keeps
        // its supported files out of the accepted population.
        fs::write(workspace.path().join(".gitignore"), "gen/\n").expect("write gitignore");
        fs::create_dir(workspace.path().join("gen")).expect("create gen dir");
        for index in 0..20 {
            fs::write(workspace.path().join(format!("gen/f{index}.ts")), "x")
                .expect("write git-ignored source");
        }
        for name in ["a.ts", "b.ts", "c.ts"] {
            fs::write(workspace.path().join(name), "1").expect("write tracked source");
        }
        let root = workspace.path().display().to_string();

        // Only three unignored supported files exist; a ceiling of three
        // accepted files accepts the repository, matching manual Git-aware
        // discovery, instead of failing on the 20 Git-ignored candidates.
        let report = repository_change_fingerprint_with_limits(&root, 64, limits(3, 64, 100, 10))
            .expect("Git-ignored files must not breach the accepted ceiling");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(report.git_ignored_skipped, 20);
    }

    #[test]
    fn fingerprint_without_git_repository_applies_no_ignore_filtering() {
        let workspace = TempWorkspace::new("fingerprint-no-git-fallback");
        // No Git repository: a `.gitignore` is present but must not filter.
        fs::write(workspace.path().join(".gitignore"), "ignored.ts\n").expect("write gitignore");
        fs::write(workspace.path().join("ignored.ts"), "kept without git")
            .expect("write candidate");
        let root = workspace.path().display().to_string();

        let report = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint outside a Git repository");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::NotRepository);
        assert_eq!(report.git_ignored_skipped, 0);
    }

    #[test]
    fn fingerprint_falls_back_to_no_ignore_filtering_when_git_metadata_invalid() {
        let workspace = TempWorkspace::new("fingerprint-invalid-git-fallback");
        fs::create_dir_all(workspace.path().join(".git")).expect("create invalid git dir");
        fs::write(workspace.path().join(".gitignore"), "ignored.ts\n").expect("write gitignore");
        fs::write(workspace.path().join("ignored.ts"), "kept on fallback")
            .expect("write candidate");
        let root = workspace.path().display().to_string();

        let report = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("fingerprint with unavailable Git");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Unavailable);
        assert_eq!(report.git_ignored_skipped, 0);
    }

    #[test]
    fn fingerprint_is_deterministic_across_repeated_passes() {
        let workspace = TempWorkspace::new("fingerprint-determinism");
        if !git_init(&workspace) {
            return;
        }
        fs::write(workspace.path().join(".gitignore"), "gen/\n").expect("write gitignore");
        fs::create_dir(workspace.path().join("gen")).expect("create gen dir");
        fs::write(workspace.path().join("gen/ignored.ts"), "ignored").expect("write ignored");
        fs::write(workspace.path().join("kept.ts"), "kept").expect("write kept source");
        let root = workspace.path().display().to_string();

        let first = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("first pass");
        let second = repository_change_fingerprint_report(&root, DEFAULT_MAX_FILE_BYTES)
            .expect("second pass");
        assert_eq!(first, second);
    }

    fn assert_limit(
        result: Result<RepositoryChangeFingerprint, FileDiscoveryError>,
        kind: FileDiscoveryLimitKind,
        limit: u64,
        observed: u64,
    ) {
        assert_eq!(
            result,
            Err(FileDiscoveryError::ResourceLimitExceeded(
                FileDiscoveryLimitExceeded {
                    kind,
                    limit,
                    observed,
                }
            ))
        );
    }
}
