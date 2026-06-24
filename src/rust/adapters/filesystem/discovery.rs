//! Filesystem-backed repository file discovery.

use crate::adapters::languages::typescript::TypeScriptLanguageAdapter;
use crate::core::model::ContentHash;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, GitIgnoreStatus, SkippedPath, SkippedReason,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "target",
    ".venv",
    "Pods",
    ".next",
    "coverage",
    "generated",
    ".cache",
    "cache",
    "out",
];

#[derive(Debug, Default)]
pub struct FilesystemFileDiscovery;

impl FileDiscovery for FilesystemFileDiscovery {
    fn discover(
        &self,
        request: FileDiscoveryRequest,
    ) -> Result<FileDiscoveryReport, FileDiscoveryError> {
        discover_files(request)
    }
}

fn discover_files(
    request: FileDiscoveryRequest,
) -> Result<FileDiscoveryReport, FileDiscoveryError> {
    if request.repository_root.trim().is_empty() {
        return Err(FileDiscoveryError::InvalidRoot(
            "repository root must not be empty".to_string(),
        ));
    }
    let root = PathBuf::from(&request.repository_root);
    let metadata = fs::symlink_metadata(&root)
        .map_err(|_| FileDiscoveryError::InvalidRoot("repository root is not readable".into()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FileDiscoveryError::InvalidRoot(
            "repository root must be a real directory".to_string(),
        ));
    }
    let canonical_root = fs::canonicalize(&root)
        .map_err(|_| FileDiscoveryError::InvalidRoot("repository root is not readable".into()))?;

    let mut state = DiscoveryState {
        root,
        canonical_root,
        max_file_bytes: request.max_file_bytes,
        files: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
        git_ignore: GitIgnoreChecker::new(&request.repository_root),
    };
    state.walk(PathBuf::new())?;
    state.finish()
}

struct DiscoveryState {
    root: PathBuf,
    canonical_root: PathBuf,
    max_file_bytes: u64,
    files: Vec<DiscoveredFile>,
    skipped: Vec<SkippedPath>,
    warnings: Vec<String>,
    git_ignore: GitIgnoreChecker,
}

impl DiscoveryState {
    fn walk(&mut self, relative_dir: PathBuf) -> Result<(), FileDiscoveryError> {
        let directory = self.root.join(&relative_dir);
        let mut entries = fs::read_dir(&directory)
            .map_err(|_| FileDiscoveryError::Unavailable("failed to read directory".into()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                FileDiscoveryError::Unavailable("failed to read directory entry".into())
            })?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let relative = relative_dir.join(entry.file_name());
            let Some(relative_path) = repo_relative_string(&relative) else {
                self.skipped.push(SkippedPath {
                    path: "<non-utf8>".to_string(),
                    reason: SkippedReason::NonUtf8Path,
                });
                continue;
            };

            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.skipped.push(SkippedPath {
                        path: relative_path,
                        reason: SkippedReason::Unreadable,
                    });
                    continue;
                }
            };

            if metadata.file_type().is_symlink() {
                self.skip_symlink(entry.path(), relative_path);
            } else if metadata.is_dir() {
                self.visit_directory(entry.path(), relative, relative_path)?;
            } else if metadata.is_file() {
                self.visit_file(entry.path(), relative_path, metadata.len());
            } else {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
            }
        }

        Ok(())
    }

    fn skip_symlink(&mut self, path: PathBuf, relative_path: String) {
        let reason = match fs::canonicalize(path) {
            Ok(target) if target.starts_with(&self.canonical_root) => {
                SkippedReason::SymlinkNotFollowed
            }
            Ok(_) => SkippedReason::SymlinkEscape,
            Err(_) => SkippedReason::Unreadable,
        };
        self.skipped.push(SkippedPath {
            path: relative_path,
            reason,
        });
    }

    fn visit_directory(
        &mut self,
        path: PathBuf,
        relative: PathBuf,
        relative_path: String,
    ) -> Result<(), FileDiscoveryError> {
        if is_repogrammar_state_dir(relative.file_name().and_then(|name| name.to_str())) {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::RepoGrammarStateDirectory,
            });
            return Ok(());
        }
        if is_default_excluded_dir(relative.file_name().and_then(|name| name.to_str())) {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::DefaultExcludedDirectory,
            });
            return Ok(());
        }

        match fs::canonicalize(path) {
            Ok(canonical) if canonical.starts_with(&self.canonical_root) => self.walk(relative),
            Ok(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::OutsideRepository,
                });
                Ok(())
            }
            Err(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
                Ok(())
            }
        }
    }

    fn visit_file(&mut self, path: PathBuf, relative_path: String, size_bytes: u64) {
        let Some(language) = language_for_path(&relative_path) else {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::UnsupportedExtension,
            });
            return;
        };
        if self
            .git_ignore
            .is_ignored(&relative_path, &mut self.warnings)
        {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::GitIgnored,
            });
            return;
        }
        if size_bytes > self.max_file_bytes {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::TooLarge,
            });
            return;
        }

        let canonical = match fs::canonicalize(&path) {
            Ok(canonical) if canonical.starts_with(&self.canonical_root) => canonical,
            Ok(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::OutsideRepository,
                });
                return;
            }
            Err(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
                return;
            }
        };
        let bytes = match fs::read(canonical) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
                return;
            }
        };
        let actual_size_bytes = bytes.len() as u64;
        if actual_size_bytes > self.max_file_bytes {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::TooLarge,
            });
            return;
        }

        let content_hash = ContentHash::new(format!("sha256:{}", sha256_hex(&bytes)))
            .expect("sha256_hex returns strict sha256:<64 hex chars> payload");
        self.files.push(DiscoveredFile {
            path: relative_path,
            language,
            content_hash,
            size_bytes: actual_size_bytes,
        });
    }

    fn finish(mut self) -> Result<FileDiscoveryReport, FileDiscoveryError> {
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        self.skipped.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.reason.as_str().cmp(right.reason.as_str()))
        });
        self.warnings.sort();
        self.warnings.dedup();

        Ok(FileDiscoveryReport {
            files: self.files,
            skipped: self.skipped,
            warnings: self.warnings,
            git_ignore_status: self.git_ignore.status,
        })
    }
}

#[derive(Debug, Clone)]
struct GitIgnoreChecker {
    root: String,
    status: GitIgnoreStatus,
}

impl GitIgnoreChecker {
    fn new(root: &str) -> Self {
        let has_git_metadata = Path::new(root).join(".git").exists();
        if !has_git_metadata {
            return Self {
                root: root.to_string(),
                status: GitIgnoreStatus::NotRepository,
            };
        }

        let status = match Command::new("git")
            .args(["-C", root, "rev-parse", "--is-inside-work-tree"])
            .output()
        {
            Ok(output) if output.status.success() => GitIgnoreStatus::Applied,
            Ok(_) | Err(_) => GitIgnoreStatus::Unavailable,
        };

        Self {
            root: root.to_string(),
            status,
        }
    }

    fn is_ignored(&mut self, relative_path: &str, warnings: &mut Vec<String>) -> bool {
        match self.status {
            GitIgnoreStatus::Applied => match Command::new("git")
                .args(["-C", &self.root, "check-ignore", "-q", "--", relative_path])
                .status()
            {
                Ok(status) if status.success() => true,
                Ok(status) if status.code() == Some(1) => false,
                Ok(_) | Err(_) => {
                    self.status = GitIgnoreStatus::Unavailable;
                    warnings.push(
                        "git ignore checks became unavailable; using safe non-git fallback"
                            .to_string(),
                    );
                    false
                }
            },
            GitIgnoreStatus::Unavailable => {
                warnings.push(
                    "git ignore checks are unavailable; using safe non-git fallback".to_string(),
                );
                false
            }
            GitIgnoreStatus::NotRepository => false,
        }
    }
}

fn is_repogrammar_state_dir(name: Option<&str>) -> bool {
    matches!(name, Some(".repogrammar"))
        || name
            .and_then(|name| name.strip_prefix(".repogrammar-"))
            .is_some_and(|suffix| !suffix.is_empty())
}

fn is_default_excluded_dir(name: Option<&str>) -> bool {
    name.is_some_and(|name| DEFAULT_EXCLUDED_DIRS.contains(&name))
}

fn language_for_path(path: &str) -> Option<DiscoveredLanguage> {
    let extension = Path::new(path).extension()?.to_str()?;
    if !TypeScriptLanguageAdapter::supports_extension(extension) {
        return None;
    }
    match extension {
        "ts" => Some(DiscoveredLanguage::TypeScript),
        "tsx" => Some(DiscoveredLanguage::TypeScriptReact),
        "js" => Some(DiscoveredLanguage::JavaScript),
        "jsx" => Some(DiscoveredLanguage::JavaScriptReact),
        _ => None,
    }
}

fn repo_relative_string(path: &Path) -> Option<String> {
    let parts = path
        .iter()
        .map(|part| part.to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (bytes.len() as u64) * 8;
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    state
        .iter()
        .map(|word| format!("{word:08x}"))
        .collect::<Vec<_>>()
        .join("")
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
    use crate::test_support::TempWorkspace;
    use std::fs;

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn discovers_ts_js_files_with_hashes_in_deterministic_order() {
        let workspace = TempWorkspace::new("discovery-basic");
        fs::create_dir_all(workspace.path().join("src")).expect("create src");
        fs::write(workspace.path().join("src/b.js"), "export const b = 2;\n").expect("write js");
        fs::write(workspace.path().join("src/a.ts"), "export const a = 1;\n").expect("write ts");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(report.files.len(), 2);
        assert_eq!(report.files[0].path, "src/a.ts");
        assert_eq!(report.files[0].language, DiscoveredLanguage::TypeScript);
        assert!(report.files[0].content_hash.as_str().starts_with("sha256:"));
        assert_eq!(report.files[1].path, "src/b.js");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::NotRepository);
    }

    #[test]
    fn skips_default_and_repogrammar_state_directories() {
        let workspace = TempWorkspace::new("discovery-skips");
        for directory in [".repogrammar", ".repogrammar-linux", "node_modules", "dist"] {
            fs::create_dir_all(workspace.path().join(directory)).expect("create skipped dir");
            fs::write(
                workspace.path().join(directory).join("ignored.ts"),
                "export const ignored = true;\n",
            )
            .expect("write ignored");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert!(report.files.is_empty());
        assert!(report.skipped.iter().any(|skip| {
            skip.path == ".repogrammar" && skip.reason == SkippedReason::RepoGrammarStateDirectory
        }));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == ".repogrammar-linux"
                && skip.reason == SkippedReason::RepoGrammarStateDirectory
        }));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == "node_modules" && skip.reason == SkippedReason::DefaultExcludedDirectory
        }));
    }

    #[test]
    fn skips_large_files_with_reason() {
        let workspace = TempWorkspace::new("discovery-large");
        fs::write(workspace.path().join("large.ts"), vec![b'x'; 12]).expect("write large file");
        let mut request = FileDiscoveryRequest::new(workspace.path().display().to_string());
        request.max_file_bytes = 8;

        let report = FilesystemFileDiscovery
            .discover(request)
            .expect("discover files");

        assert!(report.files.is_empty());
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "large.ts" && skip.reason == SkippedReason::TooLarge));
    }

    #[test]
    fn size_limit_is_inclusive_at_one_mebibyte() {
        let workspace = TempWorkspace::new("discovery-size-boundary");
        fs::write(
            workspace.path().join("exact.ts"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize],
        )
        .expect("write exact limit file");
        fs::write(
            workspace.path().join("too_large.ts"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize + 1],
        )
        .expect("write too large file");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert!(report.files.iter().any(|file| file.path == "exact.ts"));
        assert!(report
            .skipped
            .iter()
            .any(|skip| { skip.path == "too_large.ts" && skip.reason == SkippedReason::TooLarge }));
    }

    #[test]
    fn module_extensions_remain_deferred_until_language_policy_expands() {
        let workspace = TempWorkspace::new("discovery-module-extensions");
        for path in ["module.mjs", "common.cjs", "typed.mts", "typed.cts"] {
            fs::write(
                workspace.path().join(path),
                "export const deferred = true;\n",
            )
            .expect("write deferred extension");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert!(report.files.is_empty());
        for path in ["module.mjs", "common.cjs", "typed.mts", "typed.cts"] {
            assert!(
                report
                    .skipped
                    .iter()
                    .any(|skip| skip.path == path
                        && skip.reason == SkippedReason::UnsupportedExtension),
                "expected unsupported extension skip for {path}"
            );
        }
    }

    #[test]
    fn unavailable_git_ignore_reports_warning_without_blocking_discovery() {
        let workspace = TempWorkspace::new("discovery-git-unavailable");
        fs::create_dir(workspace.path().join(".git")).expect("create invalid git dir");
        fs::write(
            workspace.path().join("included.ts"),
            "export const included = true;\n",
        )
        .expect("write source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Unavailable);
        assert_eq!(report.files.len(), 1);
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("git ignore checks are unavailable")));
    }

    #[test]
    fn report_uses_relative_metadata_without_source_text_or_absolute_paths() {
        let workspace = TempWorkspace::new("discovery-no-leak");
        fs::create_dir_all(workspace.path().join("src")).expect("create src");
        fs::write(
            workspace.path().join("src/secret.ts"),
            "export const secret = 'source snippet must not leak';\n",
        )
        .expect("write source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");
        let debug = format!("{report:?}");

        assert!(report.files.iter().all(|file| !file.path.starts_with('/')));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("source snippet must not leak"));
    }

    #[test]
    fn default_size_limit_is_one_mebibyte() {
        assert_eq!(DEFAULT_MAX_FILE_BYTES, 1_048_576);
    }

    #[test]
    fn rejects_symlink_escape_without_following_it() {
        let workspace = TempWorkspace::new("discovery-symlink");
        let outside = TempWorkspace::new("discovery-symlink-outside");
        fs::write(
            outside.path().join("outside.ts"),
            "export const outside = true;\n",
        )
        .expect("write outside");

        #[cfg(unix)]
        std::os::unix::fs::symlink(
            outside.path().join("outside.ts"),
            workspace.path().join("link.ts"),
        )
        .expect("create symlink");

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            outside.path().join("outside.ts"),
            workspace.path().join("link.ts"),
        )
        .expect("create symlink");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert!(report.files.is_empty());
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "link.ts" && skip.reason == SkippedReason::SymlinkEscape));
    }

    #[test]
    fn git_ignored_ts_files_are_skipped_when_git_is_available() {
        let workspace = TempWorkspace::new("discovery-git-ignore");
        if Command::new("git")
            .args(["init", "-q"])
            .current_dir(workspace.path())
            .status()
            .map(|status| !status.success())
            .unwrap_or(true)
        {
            return;
        }
        fs::write(workspace.path().join(".gitignore"), "ignored.ts\n").expect("write gitignore");
        fs::write(
            workspace.path().join("ignored.ts"),
            "export const ignored = true;\n",
        )
        .expect("write ignored");
        fs::write(
            workspace.path().join("included.ts"),
            "export const included = true;\n",
        )
        .expect("write included");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, "included.ts");
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "ignored.ts" && skip.reason == SkippedReason::GitIgnored));
    }
}
