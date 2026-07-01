//! Filesystem-backed repository file discovery.

use super::bounded_read::{read_file_bounded, BoundedReadError};
use super::git::{GitContext, GitContextResolution};
use crate::adapters::languages::java::JavaLanguageAdapter;
use crate::adapters::languages::python::PythonLanguageAdapter;
use crate::adapters::languages::rust::RustLanguageAdapter;
use crate::core::model::ContentHash;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, GitIgnoreStatus, SkippedPath, SkippedReason,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "target",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".nox",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "Pods",
    ".next",
    "coverage",
    "generated",
    ".cache",
    "cache",
    "out",
    "site-packages",
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

pub fn supported_language_for_path(path: &str) -> Option<DiscoveredLanguage> {
    language_for_path(path)
}

pub fn is_repogrammar_state_directory_name(name: Option<&str>) -> bool {
    is_repogrammar_state_dir(name)
}

pub fn is_default_excluded_directory_name(name: Option<&str>) -> bool {
    is_default_excluded_dir(name)
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
        git_ignore: GitIgnoreChecker::new(&request.repository_root, request.strict_gitignore),
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
                self.visit_file(entry.path(), relative_path, metadata.len())?;
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

    fn visit_file(
        &mut self,
        path: PathBuf,
        relative_path: String,
        size_bytes: u64,
    ) -> Result<(), FileDiscoveryError> {
        let Some(language) = language_for_path(&relative_path) else {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::UnsupportedExtension,
            });
            return Ok(());
        };
        if self
            .git_ignore
            .is_ignored(&relative_path, &mut self.warnings)?
        {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::GitIgnored,
            });
            return Ok(());
        }
        if size_bytes > self.max_file_bytes {
            self.skipped.push(SkippedPath {
                path: relative_path,
                reason: SkippedReason::TooLarge,
            });
            return Ok(());
        }

        let canonical = match fs::canonicalize(&path) {
            Ok(canonical) if canonical.starts_with(&self.canonical_root) => canonical,
            Ok(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::OutsideRepository,
                });
                return Ok(());
            }
            Err(_) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
                return Ok(());
            }
        };
        let bytes = match read_file_bounded(&canonical, self.max_file_bytes) {
            Ok(bytes) => bytes,
            Err(BoundedReadError::TooLarge) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::TooLarge,
                });
                return Ok(());
            }
            Err(BoundedReadError::Unreadable) => {
                self.skipped.push(SkippedPath {
                    path: relative_path,
                    reason: SkippedReason::Unreadable,
                });
                return Ok(());
            }
        };
        let actual_size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);

        let content_hash = ContentHash::new(format!("sha256:{}", sha256_hex(&bytes)))
            .expect("sha256_hex returns strict sha256:<64 hex chars> payload");
        self.files.push(DiscoveredFile {
            path: relative_path,
            language,
            content_hash,
            size_bytes: actual_size_bytes,
        });
        Ok(())
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
    context: Option<GitContext>,
    status: GitIgnoreStatus,
    strict: bool,
}

impl GitIgnoreChecker {
    fn new(root: &str, strict: bool) -> Self {
        let root_path = Path::new(root);
        match GitContext::resolve(root_path) {
            Ok(context) => Self {
                context: Some(context),
                status: GitIgnoreStatus::Applied,
                strict,
            },
            Err(GitContextResolution::NotRepository) => Self {
                context: None,
                status: GitIgnoreStatus::NotRepository,
                strict,
            },
            Err(GitContextResolution::Unavailable) => Self {
                context: None,
                status: GitIgnoreStatus::Unavailable,
                strict,
            },
        }
    }

    fn is_ignored(
        &mut self,
        relative_path: &str,
        warnings: &mut Vec<String>,
    ) -> Result<bool, FileDiscoveryError> {
        match self.status {
            GitIgnoreStatus::Applied => {
                let Some(context) = &self.context else {
                    self.status = GitIgnoreStatus::Unavailable;
                    if self.strict {
                        return Err(strict_gitignore_error());
                    }
                    warnings.push(
                        "git ignore checks became unavailable; using safe non-git fallback"
                            .to_string(),
                    );
                    return Ok(false);
                };
                match context.check_ignore(relative_path) {
                    Ok(ignored) => Ok(ignored),
                    Err(()) => {
                        self.status = GitIgnoreStatus::Unavailable;
                        if self.strict {
                            return Err(strict_gitignore_error());
                        }
                        warnings.push(
                            "git ignore checks became unavailable; using safe non-git fallback"
                                .to_string(),
                        );
                        Ok(false)
                    }
                }
            }
            GitIgnoreStatus::Unavailable => {
                if self.strict {
                    return Err(strict_gitignore_error());
                }
                warnings.push(
                    "git ignore checks are unavailable; using safe non-git fallback".to_string(),
                );
                Ok(false)
            }
            GitIgnoreStatus::NotRepository => Ok(false),
        }
    }
}

fn strict_gitignore_error() -> FileDiscoveryError {
    FileDiscoveryError::Unavailable("git ignore checks are unavailable in strict mode".to_string())
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
    if path == "pyproject.toml" {
        return Some(DiscoveredLanguage::PythonConfig);
    }
    if path == "Cargo.toml" || path.ends_with("/Cargo.toml") {
        return Some(DiscoveredLanguage::RustConfig);
    }
    if is_tsjs_project_config_path(path) {
        return Some(DiscoveredLanguage::TsJsConfig);
    }
    let extension = Path::new(path).extension()?.to_str()?;
    match extension {
        "ts" => Some(DiscoveredLanguage::TypeScript),
        "tsx" => Some(DiscoveredLanguage::TypeScriptReact),
        "js" => Some(DiscoveredLanguage::JavaScript),
        "jsx" => Some(DiscoveredLanguage::JavaScriptReact),
        extension if PythonLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::Python)
        }
        extension if JavaLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::Java)
        }
        extension if RustLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::Rust)
        }
        _ => None,
    }
}

fn is_tsjs_project_config_path(path: &str) -> bool {
    matches!(
        path,
        "package.json"
            | "tsconfig.json"
            | "jsconfig.json"
            | "jest.config.json"
            | "jest.config.js"
            | "jest.config.cjs"
            | "jest.config.mjs"
            | "jest.config.ts"
            | "vitest.config.json"
            | "vitest.config.js"
            | "vitest.config.cjs"
            | "vitest.config.mjs"
            | "vitest.config.ts"
            | "next.config.js"
            | "next.config.cjs"
            | "next.config.mjs"
            | "next.config.ts"
    )
}

fn repo_relative_string(path: &Path) -> Option<String> {
    let parts = path
        .iter()
        .map(|part| part.to_str())
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    bytes_to_lower_hex(digest.as_ref())
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
    use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
    use crate::test_support::{create_test_symlink_file, TempWorkspace};
    use std::fs;
    use std::process::Command;

    fn git_init(workspace: &TempWorkspace) -> bool {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(workspace.path())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

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
    fn discovers_tsjs_project_configs_as_metadata() {
        let workspace = TempWorkspace::new("discovery-tsjs-configs");
        fs::write(
            workspace.path().join("package.json"),
            r#"{"dependencies":{"express":"latest"}}
"#,
        )
        .expect("write package");
        fs::write(
            workspace.path().join("tsconfig.json"),
            r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}
"#,
        )
        .expect("write tsconfig");
        fs::write(
            workspace.path().join("jest.config.ts"),
            "export default {};\n",
        )
        .expect("write jest config");
        fs::write(
            workspace.path().join("next.config.js"),
            "module.exports = {};\n",
        )
        .expect("write next config");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                ("jest.config.ts", DiscoveredLanguage::TsJsConfig),
                ("next.config.js", DiscoveredLanguage::TsJsConfig),
                ("package.json", DiscoveredLanguage::TsJsConfig),
                ("tsconfig.json", DiscoveredLanguage::TsJsConfig),
            ]
        );
    }

    #[test]
    fn discovers_rust_sources_and_cargo_manifest_without_target_output() {
        let workspace = TempWorkspace::new("discovery-rust");
        fs::create_dir_all(workspace.path().join("src")).expect("create src");
        fs::create_dir_all(workspace.path().join("target/debug")).expect("create target");
        fs::write(
            workspace.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .expect("write cargo");
        fs::write(workspace.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("write rust");
        fs::write(
            workspace.path().join("target/debug/generated.rs"),
            "pub fn generated() {}\n",
        )
        .expect("write target rust");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                ("Cargo.toml", DiscoveredLanguage::RustConfig),
                ("src/lib.rs", DiscoveredLanguage::Rust),
            ]
        );
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "target" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
    }

    #[test]
    fn discovers_java_sources_and_skips_java_build_outputs() {
        let workspace = TempWorkspace::new("discovery-java");
        fs::create_dir_all(workspace.path().join("src/main/java/com/example"))
            .expect("create java source dir");
        fs::create_dir_all(workspace.path().join("build/classes")).expect("create build dir");
        fs::create_dir_all(workspace.path().join("out/classes")).expect("create out dir");
        fs::write(
            workspace
                .path()
                .join("src/main/java/com/example/DemoController.java"),
            "class DemoController {}\n",
        )
        .expect("write java");
        fs::write(
            workspace.path().join("build/classes/Generated.java"),
            "class Generated {}\n",
        )
        .expect("write build java");
        fs::write(
            workspace.path().join("out/classes/Generated.java"),
            "class Generated {}\n",
        )
        .expect("write out java");
        fs::write(workspace.path().join("Demo.class"), b"bytecode").expect("write class");
        fs::write(workspace.path().join("build.gradle"), "plugins {}\n").expect("write gradle");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![(
                "src/main/java/com/example/DemoController.java",
                DiscoveredLanguage::Java
            )]
        );
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "build" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "out" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "Demo.class" && skipped.reason == SkippedReason::UnsupportedExtension
        }));
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "build.gradle" && skipped.reason == SkippedReason::UnsupportedExtension
        }));
    }

    #[test]
    fn discovers_python_files_and_skips_python_runtime_directories() {
        let workspace = TempWorkspace::new("discovery-python");
        for directory in [
            "app",
            "tests",
            ".venv",
            "venv",
            "env",
            ".tox",
            ".nox",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            "build",
            "dist",
            "site-packages",
            "nested/__pycache__",
            "nested/site-packages",
        ] {
            fs::create_dir_all(workspace.path().join(directory)).expect("create directory");
        }
        fs::write(
            workspace.path().join("app/main.py"),
            "def main():\n    pass\n",
        )
        .expect("write app");
        fs::write(
            workspace.path().join("tests/test_main.py"),
            "def test_main():\n    pass\n",
        )
        .expect("write test");
        fs::write(
            workspace.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\n",
        )
        .expect("write pyproject");
        for directory in [
            ".venv",
            "venv",
            "env",
            ".tox",
            ".nox",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            "build",
            "dist",
            "site-packages",
            "nested/__pycache__",
            "nested/site-packages",
        ] {
            fs::write(
                workspace.path().join(directory).join("ignored.py"),
                "def ignored():\n    pass\n",
            )
            .expect("write ignored");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                ("app/main.py", DiscoveredLanguage::Python),
                ("pyproject.toml", DiscoveredLanguage::PythonConfig),
                ("tests/test_main.py", DiscoveredLanguage::Python),
            ]
        );
        for directory in [
            ".venv",
            "venv",
            "env",
            ".tox",
            ".nox",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            "build",
            "dist",
            "site-packages",
            "nested/__pycache__",
            "nested/site-packages",
        ] {
            assert!(
                report.skipped.iter().any(|skip| {
                    skip.path == directory && skip.reason == SkippedReason::DefaultExcludedDirectory
                }),
                "expected default skip for {directory}"
            );
        }
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
    fn strict_git_ignore_fails_when_git_ignore_is_unavailable() {
        let workspace = TempWorkspace::new("discovery-strict-git-unavailable");
        fs::create_dir(workspace.path().join(".git")).expect("create invalid git dir");
        fs::write(
            workspace.path().join("included.py"),
            "def included():\n    pass\n",
        )
        .expect("write source");

        let error = FilesystemFileDiscovery
            .discover(
                FileDiscoveryRequest::new(workspace.path().display().to_string())
                    .with_strict_gitignore(true),
            )
            .expect_err("strict mode must reject unavailable git ignore checks");

        assert!(
            matches!(error, FileDiscoveryError::Unavailable(message) if message.contains("strict mode"))
        );
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

        if !create_test_symlink_file(
            &outside.path().join("outside.ts"),
            &workspace.path().join("link.ts"),
        ) {
            return;
        }

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
        if !git_init(&workspace) {
            return;
        }
        fs::write(
            workspace.path().join(".gitignore"),
            "ignored.ts\nignored.py\n",
        )
        .expect("write gitignore");
        fs::write(
            workspace.path().join("ignored.ts"),
            "export const ignored = true;\n",
        )
        .expect("write ignored");
        fs::write(
            workspace.path().join("ignored.py"),
            "def ignored():\n    pass\n",
        )
        .expect("write ignored python");
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
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "ignored.py" && skip.reason == SkippedReason::GitIgnored));
    }

    #[test]
    fn parent_git_ignore_rules_apply_to_project_subdirectories() {
        let workspace = TempWorkspace::new("discovery-parent-git-ignore");
        if !git_init(&workspace) {
            return;
        }
        fs::create_dir_all(workspace.path().join("packages/app")).expect("create project");
        fs::write(
            workspace.path().join(".gitignore"),
            "packages/app/ignored.ts\npackages/app/ignored.py\npackages/app/secrets/\n",
        )
        .expect("write parent gitignore");
        fs::create_dir_all(workspace.path().join("packages/app/secrets"))
            .expect("create ignored directory");
        fs::write(
            workspace.path().join("packages/app/ignored.ts"),
            "export const ignored = true;\n",
        )
        .expect("write ignored");
        fs::write(
            workspace.path().join("packages/app/ignored.py"),
            "def ignored():\n    pass\n",
        )
        .expect("write ignored python");
        fs::write(
            workspace.path().join("packages/app/secrets/hidden.ts"),
            "export const hidden = true;\n",
        )
        .expect("write ignored directory file");
        fs::write(
            workspace.path().join("packages/app/included.ts"),
            "export const included = true;\n",
        )
        .expect("write included");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().join("packages/app").display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, "included.ts");
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "ignored.ts" && skip.reason == SkippedReason::GitIgnored));
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "ignored.py" && skip.reason == SkippedReason::GitIgnored));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == "secrets/hidden.ts" && skip.reason == SkippedReason::GitIgnored
        }));
        let debug = format!("{report:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("packages/app/ignored.ts"));
        assert!(!debug.contains("packages/app/secrets/hidden.ts"));
    }
}
