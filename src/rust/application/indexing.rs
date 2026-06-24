//! Indexing use-case boundary.

use crate::error::RepoGrammarError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingRequest {
    pub repository_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingOutcome {
    pub indexed_units: usize,
}

pub fn index_repository(_request: IndexingRequest) -> Result<IndexingOutcome, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("index"))
}
