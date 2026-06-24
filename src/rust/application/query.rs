//! Query use-case boundary for finding repository analogues.

use crate::core::model::PatternClassification;
use crate::error::RepoGrammarError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueQuery {
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueResult {
    pub classification: PatternClassification,
}

pub fn find_analogues(_query: AnalogueQuery) -> Result<Vec<AnalogueResult>, RepoGrammarError> {
    Err(RepoGrammarError::NotImplemented("find_analogues"))
}
