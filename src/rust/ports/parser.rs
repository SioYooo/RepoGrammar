//! Parser port. Implementations must convert third-party parser values to core
//! RepoGrammar types before returning.

use crate::core::model::{CodeUnit, Language};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDocument<'a> {
    pub path: &'a str,
    pub language: Language,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnsupportedLanguage,
    InvalidSyntax(String),
    Internal(String),
}

pub trait SourceParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<Vec<CodeUnit>, ParseError>;
}
