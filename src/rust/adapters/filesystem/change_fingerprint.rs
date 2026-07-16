//! Bounded metadata fingerprinting for the opt-in autosync watcher.

use super::discovery::{
    is_default_excluded_directory_name, is_repogrammar_state_directory_name,
    supported_language_for_path,
};
use super::resource_limits::{DiscoveryLimits, DiscoveryResourceBudget};
use crate::ports::file_discovery::FileDiscoveryError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn repository_change_fingerprint(
    repository_root: &str,
    max_file_bytes: u64,
) -> Result<String, FileDiscoveryError> {
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
) -> Result<String, FileDiscoveryError> {
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
        entries: Vec::new(),
        budget: DiscoveryResourceBudget::new(limits),
    };
    state.walk(PathBuf::new())?;
    state.finish()
}

struct FingerprintState {
    root: PathBuf,
    canonical_root: PathBuf,
    max_file_bytes: u64,
    entries: Vec<String>,
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

            self.budget.record_accepted_file(metadata.len())?;
            let modified = metadata.modified().ok().and_then(|value| {
                value.duration_since(UNIX_EPOCH).ok().map(|duration| {
                    format!("{}.{:09}", duration.as_secs(), duration.subsec_nanos())
                })
            });
            self.entries.push(format!(
                "{relative_path}\0{}\0{}\0{}",
                metadata.len(),
                modified.as_deref().unwrap_or("unknown"),
                language.as_str()
            ));
        }
        Ok(())
    }

    fn finish(mut self) -> Result<String, FileDiscoveryError> {
        self.entries.sort();
        let mut hasher = Sha256::new();
        for entry in self.entries {
            hasher.update(entry.as_bytes());
            hasher.update([0xff]);
        }
        Ok(bytes_to_lower_hex(hasher.finalize().as_ref()))
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
        FileDiscoveryLimitExceeded, FileDiscoveryLimitKind, DEFAULT_MAX_FILE_BYTES,
    };
    use crate::test_support::TempWorkspace;
    use std::process::Command;

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
    fn fingerprint_deliberately_charges_git_ignored_supported_files() {
        let workspace = TempWorkspace::new("fingerprint-resource-git-ignored");
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(workspace.path())
                .status()
                .map(|status| status.success())
                .unwrap_or(false),
            "Git is required for this regression"
        );
        fs::write(workspace.path().join(".gitignore"), "ignored.ts\n").expect("write gitignore");
        fs::write(workspace.path().join("ignored.ts"), "ignored").expect("write ignored source");
        let root = workspace.path().display().to_string();

        assert_limit(
            repository_change_fingerprint_with_limits(&root, 64, limits(0, 0, 3, 0)),
            FileDiscoveryLimitKind::AcceptedFiles,
            0,
            1,
        );
    }

    fn assert_limit(
        result: Result<String, FileDiscoveryError>,
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
