//! Repository file discovery port.
//!
//! Implementations must return repository-relative paths and owned metadata
//! only. Source text, absolute paths, and concrete Git/process types stay out of
//! this boundary.

use crate::core::model::ContentHash;
use std::fmt::{Display, Formatter};

pub const DEFAULT_MAX_FILE_BYTES: u64 = 1_048_576;
pub const DEFAULT_MAX_ACCEPTED_FILES: u64 = 100_000;
pub const DEFAULT_MAX_ACCEPTED_BYTES: u64 = 512 * 1_024 * 1_024;
pub const DEFAULT_MAX_REPORTED_SKIPPED_PATHS: u64 = 100_000;
pub const DEFAULT_MAX_VISITED_ENTRIES: u64 = 250_000;
pub const DEFAULT_MAX_DIRECTORY_DEPTH: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveredLanguage {
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Python,
    PythonConfig,
    TsJsConfig,
    Java,
    CSharp,
    C,
    Cpp,
    CppConfig,
    Go,
    GoConfig,
    Php,
    PhpConfig,
    Ruby,
    RubyConfig,
    Swift,
    SwiftConfig,
    Rust,
    RustConfig,
}

impl DiscoveredLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::TypeScriptReact => "typescript-react",
            Self::JavaScript => "javascript",
            Self::JavaScriptReact => "javascript-react",
            Self::Python => "python",
            Self::PythonConfig => "python-config",
            Self::TsJsConfig => "tsjs-config",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CppConfig => "cpp-config",
            Self::Go => "go",
            Self::GoConfig => "go-config",
            Self::Php => "php",
            Self::PhpConfig => "php-config",
            Self::Ruby => "ruby",
            Self::RubyConfig => "ruby-config",
            Self::Swift => "swift",
            Self::SwiftConfig => "swift-config",
            Self::Rust => "rust",
            Self::RustConfig => "rust-config",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiscoveryRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
    pub strict_gitignore: bool,
}

impl FileDiscoveryRequest {
    pub fn new(repository_root: impl Into<String>) -> Self {
        Self {
            repository_root: repository_root.into(),
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            strict_gitignore: false,
        }
    }

    pub fn with_strict_gitignore(mut self, strict: bool) -> Self {
        self.strict_gitignore = strict;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: String,
    pub language: DiscoveredLanguage,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkippedReason {
    RepoGrammarStateDirectory,
    DefaultExcludedDirectory,
    UnsupportedExtension,
    LanguageSpecificExclusion,
    GitIgnored,
    TooLarge,
    SymlinkNotFollowed,
    SymlinkEscape,
    OutsideRepository,
    NonUtf8Path,
    Unreadable,
}

impl SkippedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepoGrammarStateDirectory => "repogrammar_state_directory",
            Self::DefaultExcludedDirectory => "default_excluded_directory",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::LanguageSpecificExclusion => "language_specific_exclusion",
            Self::GitIgnored => "git_ignored",
            Self::TooLarge => "too_large",
            Self::SymlinkNotFollowed => "symlink_not_followed",
            Self::SymlinkEscape => "symlink_escape",
            Self::OutsideRepository => "outside_repository",
            Self::NonUtf8Path => "non_utf8_path",
            Self::Unreadable => "unreadable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedPath {
    pub path: String,
    pub reason: SkippedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitIgnoreStatus {
    Applied,
    NotRepository,
    Unavailable,
}

impl GitIgnoreStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::NotRepository => "not_repository",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiscoveryReport {
    pub files: Vec<DiscoveredFile>,
    pub skipped: Vec<SkippedPath>,
    pub warnings: Vec<String>,
    pub git_ignore_status: GitIgnoreStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDiscoveryLimitKind {
    AcceptedFiles,
    AcceptedBytes,
    ReportedSkippedPaths,
    VisitedEntries,
    DirectoryDepth,
}

impl FileDiscoveryLimitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedFiles => "accepted_files",
            Self::AcceptedBytes => "accepted_bytes",
            Self::ReportedSkippedPaths => "reported_skipped_paths",
            Self::VisitedEntries => "visited_entries",
            Self::DirectoryDepth => "directory_depth",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileDiscoveryLimitExceeded {
    pub kind: FileDiscoveryLimitKind,
    pub limit: u64,
    pub observed: u64,
}

impl Display for FileDiscoveryLimitExceeded {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "filesystem discovery resource limit exceeded: resource={}, limit={}, observed={}; narrow the repository scope or exclude generated, dependency, build, and cache content",
            self.kind.as_str(),
            self.limit,
            self.observed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDiscoveryError {
    InvalidRoot(String),
    Unavailable(String),
    ResourceLimitExceeded(FileDiscoveryLimitExceeded),
}

impl Display for FileDiscoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRoot(message) | Self::Unavailable(message) => formatter.write_str(message),
            Self::ResourceLimitExceeded(limit) => Display::fmt(limit, formatter),
        }
    }
}

impl std::error::Error for FileDiscoveryError {}

pub trait FileDiscovery {
    fn discover(
        &self,
        request: FileDiscoveryRequest,
    ) -> Result<FileDiscoveryReport, FileDiscoveryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_discovery_tokens_are_stable_and_distinct() {
        assert_eq!(DiscoveredLanguage::Go.as_str(), "go");
        assert_eq!(DiscoveredLanguage::GoConfig.as_str(), "go-config");
        assert_ne!(DiscoveredLanguage::Go, DiscoveredLanguage::GoConfig);
    }

    #[test]
    fn ruby_discovery_and_exclusion_tokens_are_stable() {
        assert_eq!(DiscoveredLanguage::Ruby.as_str(), "ruby");
        assert_eq!(DiscoveredLanguage::RubyConfig.as_str(), "ruby-config");
        assert_ne!(DiscoveredLanguage::Ruby, DiscoveredLanguage::RubyConfig);
        assert_eq!(
            SkippedReason::LanguageSpecificExclusion.as_str(),
            "language_specific_exclusion"
        );
    }

    #[test]
    fn php_discovery_and_exclusion_tokens_are_stable() {
        assert_eq!(DiscoveredLanguage::Php.as_str(), "php");
        assert_eq!(DiscoveredLanguage::PhpConfig.as_str(), "php-config");
        assert_ne!(DiscoveredLanguage::Php, DiscoveredLanguage::PhpConfig);
        assert_eq!(
            SkippedReason::LanguageSpecificExclusion.as_str(),
            "language_specific_exclusion"
        );
    }

    #[test]
    fn swift_discovery_and_exclusion_tokens_are_stable() {
        assert_eq!(DiscoveredLanguage::Swift.as_str(), "swift");
        assert_eq!(DiscoveredLanguage::SwiftConfig.as_str(), "swift-config");
        assert_ne!(DiscoveredLanguage::Swift, DiscoveredLanguage::SwiftConfig);
        assert_eq!(
            SkippedReason::LanguageSpecificExclusion.as_str(),
            "language_specific_exclusion"
        );
    }

    #[test]
    fn discovery_resource_defaults_and_tokens_are_stable() {
        assert_eq!(DEFAULT_MAX_ACCEPTED_FILES, 100_000);
        assert_eq!(DEFAULT_MAX_ACCEPTED_BYTES, 536_870_912);
        assert_eq!(DEFAULT_MAX_REPORTED_SKIPPED_PATHS, 100_000);
        assert_eq!(DEFAULT_MAX_VISITED_ENTRIES, 250_000);
        assert_eq!(DEFAULT_MAX_DIRECTORY_DEPTH, 256);
        assert_eq!(
            FileDiscoveryLimitKind::AcceptedFiles.as_str(),
            "accepted_files"
        );
        assert_eq!(
            FileDiscoveryLimitKind::AcceptedBytes.as_str(),
            "accepted_bytes"
        );
        assert_eq!(
            FileDiscoveryLimitKind::ReportedSkippedPaths.as_str(),
            "reported_skipped_paths"
        );
        assert_eq!(
            FileDiscoveryLimitKind::VisitedEntries.as_str(),
            "visited_entries"
        );
        assert_eq!(
            FileDiscoveryLimitKind::DirectoryDepth.as_str(),
            "directory_depth"
        );
    }

    #[test]
    fn resource_error_display_is_path_and_source_free() {
        let error = FileDiscoveryError::ResourceLimitExceeded(FileDiscoveryLimitExceeded {
            kind: FileDiscoveryLimitKind::AcceptedFiles,
            limit: 1,
            observed: 2,
        });
        let rendered = error.to_string();
        assert!(rendered.contains("resource=accepted_files"));
        assert!(rendered.contains("limit=1"));
        assert!(rendered.contains("observed=2"));
        assert!(!rendered.contains('/'));
        assert!(!rendered.contains("secret source snippet"));
    }
}
