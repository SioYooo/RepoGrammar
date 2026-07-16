//! Filesystem-backed repository file discovery.

use super::bounded_read::{read_file_bounded, BoundedReadError};
use super::git::{GitContext, GitContextResolution};
use super::resource_limits::{DiscoveryLimits, DiscoveryResourceBudget};
use crate::adapters::languages::cpp::CppLanguageAdapter;
use crate::adapters::languages::csharp::CSharpLanguageAdapter;
use crate::adapters::languages::go::GoLanguageAdapter;
use crate::adapters::languages::java::JavaLanguageAdapter;
use crate::adapters::languages::php::{PhpLanguageAdapter, PhpPathClassification};
use crate::adapters::languages::python::PythonLanguageAdapter;
use crate::adapters::languages::ruby::{RubyLanguageAdapter, RubyPathClassification};
use crate::adapters::languages::rust::RustLanguageAdapter;
use crate::adapters::languages::swift::{SwiftLanguageAdapter, SwiftPathClassification};
use crate::core::model::ContentHash;
use crate::ports::file_discovery::{
    DiscoveredFile, DiscoveredLanguage, FileDiscovery, FileDiscoveryError, FileDiscoveryReport,
    FileDiscoveryRequest, GitIgnoreStatus, SkippedPath, SkippedReason,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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
    // MSBuild writes generated .cs sources (obj/*.GlobalUsings.g.cs, source
    // generator output) under obj/. bin/ is intentionally NOT excluded: it
    // contains no .cs sources and other ecosystems keep scripts there.
    "obj",
    // CLion default CMake build output directories; they hold generated and
    // compiled artifacts, never checked-in C/C++ sources.
    "cmake-build-debug",
    "cmake-build-release",
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
    discover_files_with_limits(request, DiscoveryLimits::default())
}

fn discover_files_with_limits(
    request: FileDiscoveryRequest,
    limits: DiscoveryLimits,
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
        warnings: BTreeSet::new(),
        git_ignore: GitIgnoreChecker::new(&request.repository_root, request.strict_gitignore),
        budget: DiscoveryResourceBudget::new(limits),
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
    warnings: BTreeSet<String>,
    git_ignore: GitIgnoreChecker,
    budget: DiscoveryResourceBudget,
}

impl DiscoveryState {
    fn walk(&mut self, relative_dir: PathBuf) -> Result<(), FileDiscoveryError> {
        self.budget.check_directory_depth(
            u64::try_from(relative_dir.components().count()).unwrap_or(u64::MAX),
        )?;
        let directory = self.root.join(&relative_dir);
        let read_dir = fs::read_dir(&directory)
            .map_err(|_| FileDiscoveryError::Unavailable("failed to read directory".into()))?;
        let mut entries = Vec::new();
        for entry in read_dir {
            self.budget.record_visited_entry()?;
            entries.push(entry.map_err(|_| {
                FileDiscoveryError::Unavailable("failed to read directory entry".into())
            })?);
        }
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let relative = relative_dir.join(entry.file_name());
            let Some(relative_path) = repo_relative_string(&relative) else {
                self.record_skip("<non-utf8>".to_string(), SkippedReason::NonUtf8Path)?;
                continue;
            };

            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.record_skip(relative_path, SkippedReason::Unreadable)?;
                    continue;
                }
            };

            if metadata.file_type().is_symlink() {
                self.skip_symlink(entry.path(), relative_path)?;
            } else if metadata.is_dir() {
                self.visit_directory(entry.path(), relative, relative_path)?;
            } else if metadata.is_file() {
                self.visit_file(entry.path(), relative_path, metadata.len())?;
            } else {
                self.record_skip(relative_path, SkippedReason::Unreadable)?;
            }
        }

        Ok(())
    }

    fn record_skip(
        &mut self,
        path: String,
        reason: SkippedReason,
    ) -> Result<(), FileDiscoveryError> {
        self.budget.record_skipped_path()?;
        self.skipped.push(SkippedPath { path, reason });
        Ok(())
    }

    fn skip_symlink(
        &mut self,
        path: PathBuf,
        relative_path: String,
    ) -> Result<(), FileDiscoveryError> {
        let reason = match fs::canonicalize(path) {
            Ok(target) if target.starts_with(&self.canonical_root) => {
                SkippedReason::SymlinkNotFollowed
            }
            Ok(_) => SkippedReason::SymlinkEscape,
            Err(_) => SkippedReason::Unreadable,
        };
        self.record_skip(relative_path, reason)
    }

    fn visit_directory(
        &mut self,
        path: PathBuf,
        relative: PathBuf,
        relative_path: String,
    ) -> Result<(), FileDiscoveryError> {
        if is_repogrammar_state_dir(relative.file_name().and_then(|name| name.to_str())) {
            self.record_skip(relative_path, SkippedReason::RepoGrammarStateDirectory)?;
            return Ok(());
        }
        if is_default_excluded_dir(relative.file_name().and_then(|name| name.to_str())) {
            self.record_skip(relative_path, SkippedReason::DefaultExcludedDirectory)?;
            return Ok(());
        }

        match fs::canonicalize(path) {
            Ok(canonical) if canonical.starts_with(&self.canonical_root) => self.walk(relative),
            Ok(_) => {
                self.record_skip(relative_path, SkippedReason::OutsideRepository)?;
                Ok(())
            }
            Err(_) => {
                self.record_skip(relative_path, SkippedReason::Unreadable)?;
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
        let language = match classify_language_path(&relative_path) {
            LanguagePathClassification::Supported(language) => language,
            LanguagePathClassification::LanguageSpecificExclusion => {
                self.record_skip(relative_path, SkippedReason::LanguageSpecificExclusion)?;
                return Ok(());
            }
            LanguagePathClassification::Unsupported => {
                self.record_skip(relative_path, SkippedReason::UnsupportedExtension)?;
                return Ok(());
            }
        };
        if self
            .git_ignore
            .is_ignored(&relative_path, &mut self.warnings)?
        {
            self.record_skip(relative_path, SkippedReason::GitIgnored)?;
            return Ok(());
        }
        if size_bytes > self.max_file_bytes {
            self.record_skip(relative_path, SkippedReason::TooLarge)?;
            return Ok(());
        }

        let canonical = match fs::canonicalize(&path) {
            Ok(canonical) if canonical.starts_with(&self.canonical_root) => canonical,
            Ok(_) => {
                self.record_skip(relative_path, SkippedReason::OutsideRepository)?;
                return Ok(());
            }
            Err(_) => {
                self.record_skip(relative_path, SkippedReason::Unreadable)?;
                return Ok(());
            }
        };
        let bytes = match read_file_bounded(&canonical, self.max_file_bytes) {
            Ok(bytes) => bytes,
            Err(BoundedReadError::TooLarge) => {
                self.record_skip(relative_path, SkippedReason::TooLarge)?;
                return Ok(());
            }
            Err(BoundedReadError::Unreadable) => {
                self.record_skip(relative_path, SkippedReason::Unreadable)?;
                return Ok(());
            }
        };
        let actual_size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.budget.record_accepted_file(actual_size_bytes)?;

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
        Ok(FileDiscoveryReport {
            files: self.files,
            skipped: self.skipped,
            warnings: self.warnings.into_iter().collect(),
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
        warnings: &mut BTreeSet<String>,
    ) -> Result<bool, FileDiscoveryError> {
        match self.status {
            GitIgnoreStatus::Applied => {
                let Some(context) = &self.context else {
                    self.status = GitIgnoreStatus::Unavailable;
                    if self.strict {
                        return Err(strict_gitignore_error());
                    }
                    warnings.insert(
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
                        warnings.insert(
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
                warnings.insert(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguagePathClassification {
    Supported(DiscoveredLanguage),
    LanguageSpecificExclusion,
    Unsupported,
}

fn language_for_path(path: &str) -> Option<DiscoveredLanguage> {
    match classify_language_path(path) {
        LanguagePathClassification::Supported(language) => Some(language),
        LanguagePathClassification::LanguageSpecificExclusion
        | LanguagePathClassification::Unsupported => None,
    }
}

fn classify_language_path(path: &str) -> LanguagePathClassification {
    match PhpLanguageAdapter::classify_path(path) {
        PhpPathClassification::Source => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::Php);
        }
        PhpPathClassification::Config => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::PhpConfig);
        }
        PhpPathClassification::Excluded(_) => {
            return LanguagePathClassification::LanguageSpecificExclusion;
        }
        PhpPathClassification::NotPhp => {}
    }
    match RubyLanguageAdapter::classify_path(path) {
        RubyPathClassification::Source => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::Ruby);
        }
        RubyPathClassification::Config => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::RubyConfig);
        }
        RubyPathClassification::Excluded(_) => {
            return LanguagePathClassification::LanguageSpecificExclusion;
        }
        RubyPathClassification::NotRuby => {}
    }
    match SwiftLanguageAdapter::classify_path(path) {
        SwiftPathClassification::Source => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::Swift);
        }
        SwiftPathClassification::Config => {
            return LanguagePathClassification::Supported(DiscoveredLanguage::SwiftConfig);
        }
        SwiftPathClassification::Excluded(_) => {
            return LanguagePathClassification::LanguageSpecificExclusion;
        }
        SwiftPathClassification::NotSwift => {}
    }
    if path == "pyproject.toml" || path == "setup.cfg" || path == "setup.py" {
        return LanguagePathClassification::Supported(DiscoveredLanguage::PythonConfig);
    }
    if path == "Cargo.toml" || path.ends_with("/Cargo.toml") {
        return LanguagePathClassification::Supported(DiscoveredLanguage::RustConfig);
    }
    if path == "compile_commands.json" || path == "vcpkg.json" || path == "conanfile.txt" {
        return LanguagePathClassification::Supported(DiscoveredLanguage::CppConfig);
    }
    if GoLanguageAdapter::is_project_config_path(path) {
        return LanguagePathClassification::Supported(DiscoveredLanguage::GoConfig);
    }
    if is_tsjs_project_config_path(path) {
        return LanguagePathClassification::Supported(DiscoveredLanguage::TsJsConfig);
    }
    let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) else {
        return LanguagePathClassification::Unsupported;
    };
    let language = match extension {
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
        extension if CSharpLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::CSharp)
        }
        extension if CppLanguageAdapter::is_c_extension(extension) => Some(DiscoveredLanguage::C),
        extension if CppLanguageAdapter::is_cpp_extension(extension) => {
            Some(DiscoveredLanguage::Cpp)
        }
        extension if GoLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::Go)
        }
        extension if RustLanguageAdapter::supports_extension(extension) => {
            Some(DiscoveredLanguage::Rust)
        }
        _ => None,
    };
    match language {
        Some(language) => LanguagePathClassification::Supported(language),
        None => LanguagePathClassification::Unsupported,
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
            | ".mocharc.json"
            | ".mocharc.jsonc"
            | ".mocharc.js"
            | ".mocharc.cjs"
            | ".mocharc.yml"
            | ".mocharc.yaml"
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
    use crate::ports::file_discovery::{
        FileDiscoveryLimitExceeded, FileDiscoveryLimitKind, DEFAULT_MAX_FILE_BYTES,
    };
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

    fn test_limits(
        accepted_files: u64,
        accepted_bytes: u64,
        skipped: u64,
        visited: u64,
        depth: u64,
    ) -> DiscoveryLimits {
        DiscoveryLimits {
            accepted_files,
            accepted_bytes,
            reported_skipped_paths: skipped,
            visited_entries: visited,
            directory_depth: depth,
        }
    }

    fn discover_with_test_limits(
        workspace: &TempWorkspace,
        limits: DiscoveryLimits,
    ) -> Result<FileDiscoveryReport, FileDiscoveryError> {
        discover_files_with_limits(
            FileDiscoveryRequest::new(workspace.path().display().to_string()),
            limits,
        )
    }

    fn assert_limit(
        result: Result<FileDiscoveryReport, FileDiscoveryError>,
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
    fn discovers_go_sources_tests_and_nested_configs_in_deterministic_order() {
        let workspace = TempWorkspace::new("discovery-go");
        fs::create_dir_all(workspace.path().join(".hidden")).expect("create dot dir");
        fs::create_dir_all(workspace.path().join("_generated")).expect("create underscore dir");
        fs::create_dir_all(workspace.path().join("cmd/demo")).expect("create source dir");
        fs::create_dir_all(workspace.path().join("module/tests")).expect("create test dir");
        fs::create_dir_all(workspace.path().join("pkg/testdata")).expect("create testdata dir");
        fs::create_dir_all(workspace.path().join("vendor/example.test/lib"))
            .expect("create vendor dir");
        fs::write(workspace.path().join(".hidden/file.go"), "package hidden\n")
            .expect("write dot-directory Go source");
        fs::write(
            workspace.path().join("_generated/file.go"),
            "package generated\n",
        )
        .expect("write underscore-directory Go source");
        fs::write(workspace.path().join("go.work"), "go 1.25\n").expect("write go.work");
        fs::write(workspace.path().join("cmd/demo/main.go"), "package main\n")
            .expect("write Go source");
        fs::write(
            workspace.path().join("module/go.mod"),
            "module example.test/module\n",
        )
        .expect("write nested go.mod");
        fs::write(
            workspace.path().join("module/tests/main_test.go"),
            "package tests\n",
        )
        .expect("write Go test");
        fs::write(
            workspace.path().join("pkg/testdata/fixture.go"),
            "package fixture\n",
        )
        .expect("write testdata Go source");
        fs::write(workspace.path().join("pkg/file_linux.go"), "package pkg\n")
            .expect("write GOOS-suffix Go source");
        fs::write(
            workspace.path().join("pkg/file_amd64_test.go"),
            "package pkg\n",
        )
        .expect("write GOARCH test Go source");
        fs::write(
            workspace.path().join("vendor/example.test/lib/vendor.go"),
            "package lib\n",
        )
        .expect("write vendored Go source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Go inventory");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                (".hidden/file.go", DiscoveredLanguage::Go),
                ("_generated/file.go", DiscoveredLanguage::Go),
                ("cmd/demo/main.go", DiscoveredLanguage::Go),
                ("go.work", DiscoveredLanguage::GoConfig),
                ("module/go.mod", DiscoveredLanguage::GoConfig),
                ("module/tests/main_test.go", DiscoveredLanguage::Go),
                ("pkg/file_amd64_test.go", DiscoveredLanguage::Go),
                ("pkg/file_linux.go", DiscoveredLanguage::Go),
                ("pkg/testdata/fixture.go", DiscoveredLanguage::Go),
            ]
        );
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "vendor" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
        let debug = format!("{report:?}");
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!debug.contains("module example.test/module"));
        assert!(!debug.contains("package tests"));
    }

    #[test]
    fn discovers_source_free_ruby_inventory_with_language_specific_exclusions() {
        let workspace = TempWorkspace::new("discovery-ruby");
        for directory in [
            ".bundle",
            ".ruby-lsp",
            "gems",
            "lib",
            "nested",
            "pkg",
            "tmp",
        ] {
            fs::create_dir_all(workspace.path().join(directory)).expect("create Ruby fixture dir");
        }
        for path in [
            "Gemfile",
            "Gemfile.lock",
            "gems.rb",
            "gems.locked",
            ".ruby-version",
            ".gemspec",
            "gems/example.gemspec",
            "nested/Gemfile",
        ] {
            fs::write(workspace.path().join(path), "must not be evaluated\n")
                .expect("write Ruby config inventory");
        }
        fs::write(workspace.path().join("lib/main.rb"), [0xff, 0xfe, 0xfd])
            .expect("write binary Ruby source inventory");
        fs::write(workspace.path().join(".rb"), "literal suffix\n")
            .expect("write literal .rb source");
        fs::write(workspace.path().join("tmp/cache.rb"), "tmp\n").expect("write tmp Ruby source");
        fs::write(workspace.path().join("pkg/build.rb"), "pkg\n").expect("write pkg Ruby source");
        fs::write(workspace.path().join(".bundle/ignored.rb"), "ignored\n")
            .expect("write bundled Ruby source");
        fs::write(workspace.path().join(".ruby-lsp/Gemfile"), "ignored\n")
            .expect("write Ruby LSP config");
        fs::write(workspace.path().join(".bundle/kept.ts"), "export {};\n")
            .expect("write non-Ruby source under .bundle");
        fs::write(workspace.path().join(".ruby-lsp/kept.py"), "value = 1\n")
            .expect("write non-Ruby source under .ruby-lsp");
        for path in [
            "Rakefile",
            "tasks.rake",
            "config.ru",
            "view.erb",
            "archive.gem",
        ] {
            fs::write(workspace.path().join(path), "deferred\n")
                .expect("write deferred Ruby candidate");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Ruby inventory");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                (".bundle/kept.ts", DiscoveredLanguage::TypeScript),
                (".gemspec", DiscoveredLanguage::RubyConfig),
                (".rb", DiscoveredLanguage::Ruby),
                (".ruby-lsp/kept.py", DiscoveredLanguage::Python),
                (".ruby-version", DiscoveredLanguage::RubyConfig),
                ("Gemfile", DiscoveredLanguage::RubyConfig),
                ("Gemfile.lock", DiscoveredLanguage::RubyConfig),
                ("gems.locked", DiscoveredLanguage::RubyConfig),
                ("gems.rb", DiscoveredLanguage::RubyConfig),
                ("gems/example.gemspec", DiscoveredLanguage::RubyConfig),
                ("lib/main.rb", DiscoveredLanguage::Ruby),
                ("nested/Gemfile", DiscoveredLanguage::RubyConfig),
                ("pkg/build.rb", DiscoveredLanguage::Ruby),
                ("tmp/cache.rb", DiscoveredLanguage::Ruby),
            ]
        );
        for path in [".bundle/ignored.rb", ".ruby-lsp/Gemfile"] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::LanguageSpecificExclusion
            }));
        }
        for path in [
            "Rakefile",
            "tasks.rake",
            "config.ru",
            "view.erb",
            "archive.gem",
        ] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::UnsupportedExtension
            }));
        }
        let debug = format!("{report:?}");
        assert!(!debug.contains("must not be evaluated"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn discovers_source_free_php_inventory_with_language_specific_exclusions() {
        let workspace = TempWorkspace::new("discovery-php");
        for directory in [
            ".composer",
            ".phpunit.cache",
            "nested",
            "src",
            "vendor/example",
        ] {
            fs::create_dir_all(workspace.path().join(directory)).expect("create PHP fixture dir");
        }
        for path in [
            "composer.json",
            "composer.lock",
            "phpunit.xml",
            "phpunit.xml.dist",
            "nested/composer.json",
            "nested/phpunit.xml.dist",
        ] {
            fs::write(
                workspace.path().join(path),
                "must not be decoded or evaluated\n",
            )
            .expect("write PHP config inventory");
        }
        fs::write(workspace.path().join("src/main.php"), [0xff, 0xfe, 0xfd])
            .expect("write binary PHP source inventory");
        fs::write(workspace.path().join(".php"), "literal suffix\n")
            .expect("write literal .php source");
        fs::write(workspace.path().join(".composer/ignored.php"), "ignored\n")
            .expect("write Composer tool source");
        fs::write(
            workspace.path().join(".phpunit.cache/phpunit.xml"),
            "ignored\n",
        )
        .expect("write PHPUnit cache config");
        fs::write(workspace.path().join(".composer/kept.ts"), "export {};\n")
            .expect("write non-PHP source under .composer");
        fs::write(
            workspace.path().join(".phpunit.cache/kept.py"),
            "value = 1\n",
        )
        .expect("write non-PHP source under .phpunit.cache");
        fs::write(
            workspace.path().join("vendor/example/ignored.php"),
            "ignored\n",
        )
        .expect("write vendored PHP source");
        for path in [
            "bootstrap.inc",
            "view.phtml",
            "test.phpt",
            "config.php.dist",
            "artisan",
            "composer.phar",
            "auth.json",
        ] {
            fs::write(workspace.path().join(path), "deferred\n")
                .expect("write deferred PHP candidate");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover PHP inventory");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                (".composer/kept.ts", DiscoveredLanguage::TypeScript),
                (".php", DiscoveredLanguage::Php),
                (".phpunit.cache/kept.py", DiscoveredLanguage::Python),
                ("composer.json", DiscoveredLanguage::PhpConfig),
                ("composer.lock", DiscoveredLanguage::PhpConfig),
                ("nested/composer.json", DiscoveredLanguage::PhpConfig),
                ("nested/phpunit.xml.dist", DiscoveredLanguage::PhpConfig),
                ("phpunit.xml", DiscoveredLanguage::PhpConfig),
                ("phpunit.xml.dist", DiscoveredLanguage::PhpConfig),
                ("src/main.php", DiscoveredLanguage::Php),
            ]
        );
        let binary = report
            .files
            .iter()
            .find(|file| file.path == "src/main.php")
            .expect("binary PHP file metadata");
        assert_eq!(binary.size_bytes, 3);
        assert_eq!(
            binary.content_hash.as_str(),
            "sha256:8ca9f8c269c0a4b1d8bf0efc67d97df8ad5e0ea93630fd9099860d36c0fe75ea"
        );
        for path in [".composer/ignored.php", ".phpunit.cache/phpunit.xml"] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::LanguageSpecificExclusion
            }));
        }
        for path in [
            "bootstrap.inc",
            "view.phtml",
            "test.phpt",
            "config.php.dist",
            "artisan",
            "composer.phar",
            "auth.json",
        ] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::UnsupportedExtension
            }));
        }
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "vendor" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
        let debug = format!("{report:?}");
        assert!(!debug.contains("must not be decoded or evaluated"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn discovers_source_free_swift_inventory_with_language_specific_exclusions() {
        let workspace = TempWorkspace::new("discovery-swift");
        for directory in [
            ".build",
            ".swiftpm",
            "nested",
            "Sources/App",
            "Tests/AppTests",
        ] {
            fs::create_dir_all(workspace.path().join(directory)).expect("create Swift fixture dir");
        }
        for path in [
            "Package.swift",
            "Package.resolved",
            ".swift-version",
            "Package@swift-6.swift",
            "nested/Package@swift-6.3.swift",
            "nested/Package@swift-6.3.3.swift",
            "nested/Package.resolved",
        ] {
            fs::write(
                workspace.path().join(path),
                "must not be decoded or evaluated\n",
            )
            .expect("write Swift config inventory");
        }
        fs::write(
            workspace.path().join("Sources/App/main.swift"),
            [0xff, 0xfe, 0xfd],
        )
        .expect("write binary Swift source inventory");
        fs::write(workspace.path().join(".swift"), "literal suffix\n")
            .expect("write literal .swift source");
        fs::write(
            workspace.path().join("Tests/AppTests/AppTests.swift"),
            "final class AppTests {}\n",
        )
        .expect("write Swift test source inventory");
        fs::write(workspace.path().join(".build/ignored.swift"), "ignored\n")
            .expect("write Swift build output");
        fs::write(workspace.path().join(".swiftpm/Package.swift"), "ignored\n")
            .expect("write SwiftPM tool config");
        fs::write(workspace.path().join(".build/kept.ts"), "export {};\n")
            .expect("write non-Swift source under .build");
        fs::write(workspace.path().join(".swiftpm/kept.py"), "value = 1\n")
            .expect("write non-Swift source under .swiftpm");
        for path in [
            "Package@swift-.swift",
            "Package@swift-6..swift",
            "Package@swift-6.3.3.1.swift",
            "nested/Package@Swift-6.swift",
            "Package@swift-6.swift.bak",
            "project.pbxproj",
            "Package.resolved.bak",
            "nested/.Swift-version",
        ] {
            fs::write(workspace.path().join(path), "deferred\n")
                .expect("write deferred Swift candidate");
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Swift inventory");

        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![
                (".build/kept.ts", DiscoveredLanguage::TypeScript),
                (".swift", DiscoveredLanguage::Swift),
                (".swift-version", DiscoveredLanguage::SwiftConfig),
                (".swiftpm/kept.py", DiscoveredLanguage::Python),
                ("Package.resolved", DiscoveredLanguage::SwiftConfig),
                ("Package.swift", DiscoveredLanguage::SwiftConfig),
                ("Package@swift-.swift", DiscoveredLanguage::Swift),
                ("Package@swift-6..swift", DiscoveredLanguage::Swift),
                ("Package@swift-6.3.3.1.swift", DiscoveredLanguage::Swift),
                ("Package@swift-6.swift", DiscoveredLanguage::SwiftConfig),
                ("Sources/App/main.swift", DiscoveredLanguage::Swift),
                ("Tests/AppTests/AppTests.swift", DiscoveredLanguage::Swift),
                ("nested/Package.resolved", DiscoveredLanguage::SwiftConfig),
                ("nested/Package@Swift-6.swift", DiscoveredLanguage::Swift),
                (
                    "nested/Package@swift-6.3.3.swift",
                    DiscoveredLanguage::SwiftConfig
                ),
                (
                    "nested/Package@swift-6.3.swift",
                    DiscoveredLanguage::SwiftConfig
                ),
            ]
        );
        let binary = report
            .files
            .iter()
            .find(|file| file.path == "Sources/App/main.swift")
            .expect("binary Swift file metadata");
        assert_eq!(binary.size_bytes, 3);
        assert_eq!(
            binary.content_hash.as_str(),
            "sha256:8ca9f8c269c0a4b1d8bf0efc67d97df8ad5e0ea93630fd9099860d36c0fe75ea"
        );
        for path in [".build/ignored.swift", ".swiftpm/Package.swift"] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::LanguageSpecificExclusion
            }));
        }
        for path in [
            "Package@swift-6.swift.bak",
            "project.pbxproj",
            "Package.resolved.bak",
            "nested/.Swift-version",
        ] {
            assert!(report.skipped.iter().any(|skipped| {
                skipped.path == path && skipped.reason == SkippedReason::UnsupportedExtension
            }));
        }
        let debug = format!("{report:?}");
        assert!(!debug.contains("must not be decoded or evaluated"));
        assert!(!debug.contains(workspace.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn supported_language_for_path_rejects_invalid_php_paths_without_fallback() {
        for path in [
            "/main.php",
            "C:/main.php",
            "./main.php",
            "src/../main.php",
            "src//main.php",
            "src\\main.php",
            "file://main.php",
            "/composer.json",
            "nested/../phpunit.xml",
        ] {
            assert_eq!(supported_language_for_path(path), None, "{path:?}");
        }
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
    fn discovers_csharp_sources_and_skips_msbuild_obj_output() {
        let workspace = TempWorkspace::new("discovery-csharp");
        fs::create_dir_all(workspace.path().join("Controllers")).expect("create controllers dir");
        fs::create_dir_all(workspace.path().join("obj/Debug/net8.0"))
            .expect("create obj output dir");
        fs::create_dir_all(workspace.path().join("bin/Debug/net8.0"))
            .expect("create bin output dir");
        fs::write(
            workspace.path().join("Controllers/CatalogController.cs"),
            "public class CatalogController { }\n",
        )
        .expect("write csharp source");
        fs::write(
            workspace
                .path()
                .join("obj/Debug/net8.0/Demo.GlobalUsings.g.cs"),
            "global using global::System;\n",
        )
        .expect("write generated csharp");
        fs::write(
            workspace.path().join("bin/Debug/net8.0/run.sh"),
            "#!/bin/sh\n",
        )
        .expect("write bin script");
        fs::write(workspace.path().join("Demo.csproj"), "<Project />\n").expect("write csproj");

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
                "Controllers/CatalogController.cs",
                DiscoveredLanguage::CSharp
            )]
        );
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "obj" && skipped.reason == SkippedReason::DefaultExcludedDirectory
        }));
        assert!(!report.skipped.iter().any(|skipped| skipped.path == "bin"
            && skipped.reason == SkippedReason::DefaultExcludedDirectory));
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "Demo.csproj" && skipped.reason == SkippedReason::UnsupportedExtension
        }));
    }

    #[test]
    fn discovers_c_and_cpp_sources_and_skips_cmake_build_output() {
        let workspace = TempWorkspace::new("discovery-cpp");
        fs::create_dir_all(workspace.path().join("tests")).expect("create tests dir");
        fs::create_dir_all(workspace.path().join("include")).expect("create include dir");
        fs::create_dir_all(workspace.path().join("cmake-build-debug"))
            .expect("create cmake build dir");
        fs::write(
            workspace.path().join("tests/catalog_test.cc"),
            "TEST(Suite, Name) { }\n",
        )
        .expect("write cpp source");
        fs::write(workspace.path().join("include/api.h"), "int api(void);\n")
            .expect("write c header");
        fs::write(
            workspace.path().join("cmake-build-debug/generated.cpp"),
            "int generated() { return 0; }\n",
        )
        .expect("write generated cpp");
        fs::write(workspace.path().join("compile_commands.json"), "[]\n")
            .expect("write compile commands");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        let mut discovered = report
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.language))
            .collect::<Vec<_>>();
        discovered.sort_by(|left, right| left.0.cmp(right.0));
        assert_eq!(
            discovered,
            vec![
                ("compile_commands.json", DiscoveredLanguage::CppConfig),
                ("include/api.h", DiscoveredLanguage::C),
                ("tests/catalog_test.cc", DiscoveredLanguage::Cpp),
            ]
        );
        assert!(report.skipped.iter().any(|skipped| {
            skipped.path == "cmake-build-debug"
                && skipped.reason == SkippedReason::DefaultExcludedDirectory
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
        fs::write(
            workspace.path().join("setup.cfg"),
            "[metadata]\nname = demo\n",
        )
        .expect("write setup.cfg");
        fs::write(
            workspace.path().join("setup.py"),
            "from setuptools import setup\n\nsetup(name=\"demo\")\n",
        )
        .expect("write setup.py");
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
                ("setup.cfg", DiscoveredLanguage::PythonConfig),
                ("setup.py", DiscoveredLanguage::PythonConfig),
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
    fn setup_py_discovery_is_root_exact_and_keeps_similar_paths_as_python() {
        assert_eq!(
            supported_language_for_path("setup.py"),
            Some(DiscoveredLanguage::PythonConfig)
        );
        assert_eq!(
            supported_language_for_path("setup_helper.py"),
            Some(DiscoveredLanguage::Python)
        );
        assert_eq!(
            supported_language_for_path("nested/setup.py"),
            Some(DiscoveredLanguage::Python)
        );
        assert_eq!(supported_language_for_path("setup.py.bak"), None);
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
    fn go_size_limit_is_inclusive_at_one_mebibyte() {
        let workspace = TempWorkspace::new("discovery-go-size-boundary");
        fs::write(
            workspace.path().join("exact.go"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize],
        )
        .expect("write exact Go limit file");
        fs::write(
            workspace.path().join("too_large.go"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize + 1],
        )
        .expect("write too large Go file");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Go size boundary");

        assert!(report
            .files
            .iter()
            .any(|file| { file.path == "exact.go" && file.language == DiscoveredLanguage::Go }));
        assert!(report
            .skipped
            .iter()
            .any(|skip| { skip.path == "too_large.go" && skip.reason == SkippedReason::TooLarge }));
    }

    #[test]
    fn ruby_size_limit_is_inclusive_at_one_mebibyte() {
        let workspace = TempWorkspace::new("discovery-ruby-size-boundary");
        fs::write(
            workspace.path().join("exact.rb"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize],
        )
        .expect("write exact Ruby limit file");
        fs::write(
            workspace.path().join("too_large.gemspec"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize + 1],
        )
        .expect("write too large Ruby config");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Ruby size boundary");

        assert!(report
            .files
            .iter()
            .any(|file| { file.path == "exact.rb" && file.language == DiscoveredLanguage::Ruby }));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == "too_large.gemspec" && skip.reason == SkippedReason::TooLarge
        }));
    }

    #[test]
    fn php_size_limit_is_inclusive_at_one_mebibyte() {
        let workspace = TempWorkspace::new("discovery-php-size-boundary");
        fs::write(
            workspace.path().join("exact.php"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize],
        )
        .expect("write exact PHP limit file");
        fs::write(
            workspace.path().join("composer.json"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize + 1],
        )
        .expect("write too large PHP config");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover PHP size boundary");

        assert!(report
            .files
            .iter()
            .any(|file| { file.path == "exact.php" && file.language == DiscoveredLanguage::Php }));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == "composer.json" && skip.reason == SkippedReason::TooLarge
        }));
    }

    #[test]
    fn swift_size_limit_is_inclusive_at_one_mebibyte() {
        let workspace = TempWorkspace::new("discovery-swift-size-boundary");
        fs::write(
            workspace.path().join("exact.swift"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize],
        )
        .expect("write exact Swift limit file");
        fs::write(
            workspace.path().join("Package@swift-6.3.swift"),
            vec![b'x'; DEFAULT_MAX_FILE_BYTES as usize + 1],
        )
        .expect("write too large Swift config");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Swift size boundary");

        assert!(report.files.iter().any(|file| {
            file.path == "exact.swift" && file.language == DiscoveredLanguage::Swift
        }));
        assert!(report.skipped.iter().any(|skip| {
            skip.path == "Package@swift-6.3.swift" && skip.reason == SkippedReason::TooLarge
        }));
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
        fs::write(
            workspace.path().join("second.ts"),
            "export const second = true;\n",
        )
        .expect("write second source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Unavailable);
        assert_eq!(report.files.len(), 2);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("git ignore checks are unavailable"));
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
    fn aggregate_file_and_byte_limits_are_inclusive_and_zero_byte_files_count() {
        let workspace = TempWorkspace::new("discovery-resource-accepted");
        fs::write(workspace.path().join("a.ts"), "aa").expect("write a");
        discover_with_test_limits(&workspace, test_limits(1, 2, 0, 1, 0))
            .expect("exact file and byte limits");

        fs::write(workspace.path().join("b.ts"), "").expect("write zero byte b");
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(1, 2, 0, 2, 0)),
            FileDiscoveryLimitKind::AcceptedFiles,
            1,
            2,
        );

        let zero = TempWorkspace::new("discovery-resource-zero-byte");
        fs::write(zero.path().join("zero.ts"), "").expect("write zero");
        assert_limit(
            discover_with_test_limits(&zero, test_limits(0, 0, 0, 1, 0)),
            FileDiscoveryLimitKind::AcceptedFiles,
            0,
            1,
        );

        let bytes = TempWorkspace::new("discovery-resource-bytes");
        fs::write(bytes.path().join("bytes.ts"), "aa").expect("write bytes");
        assert_limit(
            discover_with_test_limits(&bytes, test_limits(1, 1, 0, 1, 0)),
            FileDiscoveryLimitKind::AcceptedBytes,
            1,
            2,
        );
    }

    #[test]
    fn visited_skip_and_depth_limits_are_inclusive_and_bound_directory_buffers() {
        let workspace = TempWorkspace::new("discovery-resource-walk");
        fs::write(workspace.path().join("one.txt"), "one").expect("write one");
        fs::write(workspace.path().join("two.txt"), "two").expect("write two");
        discover_with_test_limits(&workspace, test_limits(0, 0, 2, 2, 0))
            .expect("exact visited and skip limits");
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(0, 0, 2, 1, 0)),
            FileDiscoveryLimitKind::VisitedEntries,
            1,
            2,
        );
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(0, 0, 1, 2, 0)),
            FileDiscoveryLimitKind::ReportedSkippedPaths,
            1,
            2,
        );

        let depth = TempWorkspace::new("discovery-resource-depth");
        fs::create_dir(depth.path().join("one")).expect("create one");
        fs::write(depth.path().join("one/a.ts"), "a").expect("write nested");
        discover_with_test_limits(&depth, test_limits(1, 1, 0, 2, 1)).expect("exact depth limit");
        assert_limit(
            discover_with_test_limits(&depth, test_limits(1, 1, 0, 2, 0)),
            FileDiscoveryLimitKind::DirectoryDepth,
            0,
            1,
        );
    }

    #[test]
    fn skipped_candidates_do_not_consume_accepted_budgets_and_errors_do_not_leak() {
        let workspace = TempWorkspace::new("discovery-resource-no-leak-secret");
        fs::write(workspace.path().join("too-large.ts"), "xx").expect("write oversized");
        fs::write(workspace.path().join("unsupported.secret"), "secret source")
            .expect("write unsupported");
        let request = FileDiscoveryRequest {
            repository_root: workspace.path().display().to_string(),
            max_file_bytes: 1,
            strict_gitignore: false,
        };
        discover_files_with_limits(request.clone(), test_limits(0, 0, 2, 2, 0))
            .expect("skipped files do not consume accepted budgets");

        let error = discover_files_with_limits(request, test_limits(0, 0, 2, 1, 0))
            .expect_err("visited limit must fail");
        let rendered = error.to_string();
        assert!(!rendered.contains(workspace.path().to_string_lossy().as_ref()));
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("too-large.ts"));
    }

    #[test]
    fn ruby_inventory_obeys_accepted_and_exclusion_resource_boundaries() {
        let workspace = TempWorkspace::new("discovery-ruby-resource-boundary");
        fs::create_dir(workspace.path().join(".bundle")).expect("create .bundle");
        fs::write(workspace.path().join("main.rb"), "x").expect("write Ruby source");
        fs::write(workspace.path().join("Gemfile"), "").expect("write Ruby config");
        fs::write(workspace.path().join(".bundle/ignored.rb"), "ignored")
            .expect("write excluded Ruby source");
        fs::write(workspace.path().join(".bundle/kept.ts"), "y")
            .expect("write supported non-Ruby source");

        discover_with_test_limits(&workspace, test_limits(3, 2, 1, 5, 1))
            .expect("exact Ruby inventory budgets");
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(2, 2, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedFiles,
            2,
            3,
        );
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(3, 1, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedBytes,
            1,
            2,
        );
    }

    #[test]
    fn php_inventory_obeys_accepted_and_exclusion_resource_boundaries() {
        let workspace = TempWorkspace::new("discovery-php-resource-boundary");
        fs::create_dir(workspace.path().join(".composer")).expect("create .composer");
        fs::write(workspace.path().join("main.php"), "x").expect("write PHP source");
        fs::write(workspace.path().join("composer.json"), "").expect("write PHP config");
        fs::write(workspace.path().join(".composer/ignored.php"), "ignored")
            .expect("write excluded PHP source");
        fs::write(workspace.path().join(".composer/kept.ts"), "y")
            .expect("write supported non-PHP source");

        discover_with_test_limits(&workspace, test_limits(3, 2, 1, 5, 1))
            .expect("exact PHP inventory budgets");
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(2, 2, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedFiles,
            2,
            3,
        );
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(3, 1, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedBytes,
            1,
            2,
        );
    }

    #[test]
    fn swift_inventory_obeys_accepted_and_exclusion_resource_boundaries() {
        let workspace = TempWorkspace::new("discovery-swift-resource-boundary");
        fs::create_dir(workspace.path().join(".build")).expect("create .build");
        fs::write(workspace.path().join("main.swift"), "x").expect("write Swift source");
        fs::write(workspace.path().join("Package.swift"), "").expect("write Swift config");
        fs::write(workspace.path().join(".build/ignored.swift"), "ignored")
            .expect("write excluded Swift source");
        fs::write(workspace.path().join(".build/kept.ts"), "y")
            .expect("write supported non-Swift source");

        discover_with_test_limits(&workspace, test_limits(3, 2, 1, 5, 1))
            .expect("exact Swift inventory budgets");
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(2, 2, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedFiles,
            2,
            3,
        );
        assert_limit(
            discover_with_test_limits(&workspace, test_limits(3, 1, 1, 5, 1)),
            FileDiscoveryLimitKind::AcceptedBytes,
            1,
            2,
        );
    }

    #[test]
    fn symlink_and_git_ignored_candidates_do_not_consume_accepted_budgets() {
        let symlink_workspace = TempWorkspace::new("discovery-resource-symlink-budget");
        let outside = TempWorkspace::new("discovery-resource-symlink-budget-outside");
        fs::write(outside.path().join("outside.ts"), "outside").expect("write outside source");
        if create_test_symlink_file(
            &outside.path().join("outside.ts"),
            &symlink_workspace.path().join("link.ts"),
        ) {
            let report = discover_with_test_limits(&symlink_workspace, test_limits(0, 0, 1, 1, 0))
                .expect("symlink skip must not consume accepted budgets");
            assert_eq!(report.skipped[0].reason, SkippedReason::SymlinkEscape);
        }

        let git_workspace = TempWorkspace::new("discovery-resource-git-ignore-budget");
        assert!(
            git_init(&git_workspace),
            "Git is required for this regression"
        );
        fs::write(git_workspace.path().join(".gitignore"), "ignored.ts\n")
            .expect("write gitignore");
        fs::write(git_workspace.path().join("ignored.ts"), "ignored")
            .expect("write ignored source");
        let report = discover_with_test_limits(&git_workspace, test_limits(0, 0, 3, 3, 0))
            .expect("Git-ignored source must not consume accepted budgets");
        assert!(report
            .skipped
            .iter()
            .any(|skip| { skip.path == "ignored.ts" && skip.reason == SkippedReason::GitIgnored }));
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_unreadable_without_consuming_accepted_budgets() {
        let workspace = TempWorkspace::new("discovery-resource-broken-symlink");
        assert!(create_test_symlink_file(
            &workspace.path().join("missing.ts"),
            &workspace.path().join("broken.ts"),
        ));

        let report = discover_with_test_limits(&workspace, test_limits(0, 0, 1, 1, 0))
            .expect("broken symlink must consume only the skip budget");
        assert_eq!(report.files, Vec::new());
        assert_eq!(
            report.skipped,
            vec![SkippedPath {
                path: "broken.ts".to_string(),
                reason: SkippedReason::Unreadable,
            }]
        );
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
    fn rejects_go_symlink_escape_without_following_it() {
        let workspace = TempWorkspace::new("discovery-go-symlink");
        let outside = TempWorkspace::new("discovery-go-symlink-outside");
        fs::write(outside.path().join("outside.go"), "package outside\n")
            .expect("write outside Go source");

        if !create_test_symlink_file(
            &outside.path().join("outside.go"),
            &workspace.path().join("link.go"),
        ) {
            return;
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Go symlink");

        assert!(report.files.is_empty());
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "link.go" && skip.reason == SkippedReason::SymlinkEscape));
    }

    #[test]
    fn rejects_ruby_symlink_escape_without_following_it() {
        let workspace = TempWorkspace::new("discovery-ruby-symlink");
        let outside = TempWorkspace::new("discovery-ruby-symlink-outside");
        fs::write(outside.path().join("outside.rb"), "outside\n")
            .expect("write outside Ruby source");

        if !create_test_symlink_file(
            &outside.path().join("outside.rb"),
            &workspace.path().join("link.rb"),
        ) {
            return;
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Ruby symlink");

        assert!(report.files.is_empty());
        assert!(report
            .skipped
            .iter()
            .any(|skip| skip.path == "link.rb" && skip.reason == SkippedReason::SymlinkEscape));
    }

    #[test]
    fn rejects_php_source_and_config_symlink_escapes_without_following_them() {
        let workspace = TempWorkspace::new("discovery-php-symlink");
        let outside = TempWorkspace::new("discovery-php-symlink-outside");
        fs::write(outside.path().join("outside.php"), "outside\n")
            .expect("write outside PHP source");
        fs::write(outside.path().join("composer.json"), "{}\n").expect("write outside PHP config");

        if !create_test_symlink_file(
            &outside.path().join("outside.php"),
            &workspace.path().join("link.php"),
        ) || !create_test_symlink_file(
            &outside.path().join("composer.json"),
            &workspace.path().join("composer.json"),
        ) {
            return;
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover PHP symlinks");

        assert!(report.files.is_empty());
        for path in ["composer.json", "link.php"] {
            assert!(report
                .skipped
                .iter()
                .any(|skip| skip.path == path && skip.reason == SkippedReason::SymlinkEscape));
        }
    }

    #[test]
    fn rejects_swift_source_and_config_symlink_escapes_without_following_them() {
        let workspace = TempWorkspace::new("discovery-swift-symlink");
        let outside = TempWorkspace::new("discovery-swift-symlink-outside");
        fs::write(outside.path().join("outside.swift"), "outside\n")
            .expect("write outside Swift source");
        fs::write(outside.path().join("Package.swift"), "outside\n")
            .expect("write outside Swift config");

        if !create_test_symlink_file(
            &outside.path().join("outside.swift"),
            &workspace.path().join("link.swift"),
        ) || !create_test_symlink_file(
            &outside.path().join("Package.swift"),
            &workspace.path().join("Package.swift"),
        ) {
            return;
        }

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Swift symlinks");

        assert!(report.files.is_empty());
        for path in ["Package.swift", "link.swift"] {
            assert!(report
                .skipped
                .iter()
                .any(|skip| skip.path == path && skip.reason == SkippedReason::SymlinkEscape));
        }
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
    fn git_ignored_ruby_source_and_config_are_skipped() {
        let workspace = TempWorkspace::new("discovery-ruby-git-ignore");
        if !git_init(&workspace) {
            return;
        }
        fs::write(
            workspace.path().join(".gitignore"),
            "ignored.rb\nignored.gemspec\n",
        )
        .expect("write Ruby gitignore");
        fs::write(workspace.path().join("ignored.rb"), "ignored\n")
            .expect("write ignored Ruby source");
        fs::write(workspace.path().join("ignored.gemspec"), "ignored\n")
            .expect("write ignored Ruby config");
        fs::write(workspace.path().join("included.rb"), "included\n")
            .expect("write included Ruby source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Ruby files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![("included.rb", DiscoveredLanguage::Ruby)]
        );
        for path in ["ignored.rb", "ignored.gemspec"] {
            assert!(report
                .skipped
                .iter()
                .any(|skip| { skip.path == path && skip.reason == SkippedReason::GitIgnored }));
        }
    }

    #[test]
    fn git_ignored_php_source_and_config_are_skipped() {
        let workspace = TempWorkspace::new("discovery-php-git-ignore");
        if !git_init(&workspace) {
            return;
        }
        fs::write(
            workspace.path().join(".gitignore"),
            "ignored.php\ncomposer.json\n",
        )
        .expect("write PHP gitignore");
        fs::write(workspace.path().join("ignored.php"), "ignored\n")
            .expect("write ignored PHP source");
        fs::write(workspace.path().join("composer.json"), "{}\n")
            .expect("write ignored PHP config");
        fs::write(workspace.path().join("included.php"), "included\n")
            .expect("write included PHP source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover PHP files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![("included.php", DiscoveredLanguage::Php)]
        );
        for path in ["composer.json", "ignored.php"] {
            assert!(report
                .skipped
                .iter()
                .any(|skip| { skip.path == path && skip.reason == SkippedReason::GitIgnored }));
        }
    }

    #[test]
    fn git_ignored_swift_source_and_config_are_skipped() {
        let workspace = TempWorkspace::new("discovery-swift-git-ignore");
        if !git_init(&workspace) {
            return;
        }
        fs::write(
            workspace.path().join(".gitignore"),
            "ignored.swift\nPackage@swift-6.3.swift\n",
        )
        .expect("write Swift gitignore");
        fs::write(workspace.path().join("ignored.swift"), "ignored\n")
            .expect("write ignored Swift source");
        fs::write(
            workspace.path().join("Package@swift-6.3.swift"),
            "ignored\n",
        )
        .expect("write ignored Swift config");
        fs::write(workspace.path().join("included.swift"), "included\n")
            .expect("write included Swift source");

        let report = FilesystemFileDiscovery
            .discover(FileDiscoveryRequest::new(
                workspace.path().display().to_string(),
            ))
            .expect("discover Swift files");

        assert_eq!(report.git_ignore_status, GitIgnoreStatus::Applied);
        assert_eq!(
            report
                .files
                .iter()
                .map(|file| (file.path.as_str(), file.language))
                .collect::<Vec<_>>(),
            vec![("included.swift", DiscoveredLanguage::Swift)]
        );
        for path in ["Package@swift-6.3.swift", "ignored.swift"] {
            assert!(report
                .skipped
                .iter()
                .any(|skip| skip.path == path && skip.reason == SkippedReason::GitIgnored));
        }
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
