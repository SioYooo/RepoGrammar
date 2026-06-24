//! Conformance use-case boundary.

use crate::core::policy::compatibility::CompatibilityResult;
use crate::error::RepoGrammarError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceRequest {
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceOutcome {
    pub compatibility: CompatibilityResult,
}

pub fn check_conformance(
    _request: ConformanceRequest,
) -> Result<ConformanceOutcome, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("check_conformance"))
}
