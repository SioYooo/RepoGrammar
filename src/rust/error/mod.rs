//! Shared error types for application and interface boundaries.

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoGrammarError {
    NotImplemented(&'static str),
    InvalidInput(String),
}

impl Display for RepoGrammarError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented(command) => {
                write!(formatter, "repogrammar {command} is not implemented yet")
            }
            Self::InvalidInput(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RepoGrammarError {}
