//! Repository file discovery port.
//!
//! Implementations must return repository-relative paths and owned metadata
//! only. Source text, absolute paths, and concrete Git/process types stay out of
//! this boundary.

use crate::core::model::ContentHash;

pub const DEFAULT_MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveredLanguage {
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Python,
    PythonConfig,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDiscoveryError {
    InvalidRoot(String),
    Unavailable(String),
}

pub trait FileDiscovery {
    fn discover(
        &self,
        request: FileDiscoveryRequest,
    ) -> Result<FileDiscoveryReport, FileDiscoveryError>;
}
