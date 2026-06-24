//! Indexing use-case boundary.

use crate::error::RepoGrammarError;
use crate::ports::file_discovery::{
    FileDiscovery, FileDiscoveryError, FileDiscoveryReport, FileDiscoveryRequest,
    DEFAULT_MAX_FILE_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingRequest {
    pub repository_root: String,
    pub max_file_bytes: u64,
}

impl IndexingRequest {
    pub fn new(repository_root: impl Into<String>) -> Self {
        Self {
            repository_root: repository_root.into(),
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingOutcome {
    pub indexed_units: usize,
    pub discovered_files: usize,
    pub skipped_paths: usize,
    pub warnings: Vec<String>,
}

pub fn index_repository(_request: IndexingRequest) -> Result<IndexingOutcome, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("index"))
}

pub fn discover_repository_files(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
) -> Result<FileDiscoveryReport, RepoGrammarError> {
    discovery
        .discover(FileDiscoveryRequest {
            repository_root: request.repository_root,
            max_file_bytes: request.max_file_bytes,
        })
        .map_err(discovery_error)
}

pub fn index_repository_with_discovery(
    request: IndexingRequest,
    discovery: &impl FileDiscovery,
) -> Result<IndexingOutcome, RepoGrammarError> {
    let report = discover_repository_files(request, discovery)?;
    Ok(IndexingOutcome {
        indexed_units: 0,
        discovered_files: report.files.len(),
        skipped_paths: report.skipped.len(),
        warnings: report.warnings,
    })
}

fn discovery_error(error: FileDiscoveryError) -> RepoGrammarError {
    match error {
        FileDiscoveryError::InvalidRoot(message) | FileDiscoveryError::Unavailable(message) => {
            RepoGrammarError::InvalidInput(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::discovery::FilesystemFileDiscovery;
    use crate::ports::file_discovery::GitIgnoreStatus;
    use crate::test_support::TempWorkspace;
    use std::fs;

    #[test]
    fn discovery_use_case_returns_files_without_claiming_indexed_units() {
        let workspace = TempWorkspace::new("indexing-discovery");
        fs::write(
            workspace.path().join("handler.ts"),
            "export const handler = () => 1;\n",
        )
        .expect("write source");

        let report = discover_repository_files(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("discover files");

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, "handler.ts");
        assert_eq!(report.git_ignore_status, GitIgnoreStatus::NotRepository);

        let outcome = index_repository_with_discovery(
            IndexingRequest::new(workspace.path().display().to_string()),
            &FilesystemFileDiscovery,
        )
        .expect("scan repository");
        assert_eq!(outcome.discovered_files, 1);
        assert_eq!(outcome.indexed_units, 0);
    }

    #[test]
    fn discovery_use_case_rejects_invalid_roots() {
        let error = discover_repository_files(IndexingRequest::new(""), &FilesystemFileDiscovery)
            .expect_err("empty root must fail");

        assert!(error.to_string().contains("repository root"));
    }
}
