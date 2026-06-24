//! Language-native semantic worker port.
//!
//! Workers may use native compiler, type-checker, or language-server APIs, but
//! must return RepoGrammar-owned semantic facts.

use crate::core::model::SemanticFact;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWorkerRequest {
    pub project_root: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticWorkerError {
    Unavailable(String),
    UnsupportedVersion(String),
    ProtocolViolation(String),
}

pub trait SemanticWorker {
    fn analyze_project(
        &self,
        request: SemanticWorkerRequest,
    ) -> Result<Vec<SemanticFact>, SemanticWorkerError>;
}
