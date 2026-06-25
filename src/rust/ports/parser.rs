//! Parser port. Implementations must convert third-party parser values to core
//! RepoGrammar types before returning.

use crate::core::model::{
    CodeUnit, ContentHash, IrEdge, IrNode, Language, RepositoryRevision, SemanticFact, SourceRange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDocument<'a> {
    pub path: &'a str,
    pub language: Language,
    pub content_hash: ContentHash,
    pub repository_revision: RepositoryRevision,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseReport {
    pub units: Vec<CodeUnit>,
    pub ir_nodes: Vec<IrNode>,
    pub ir_edges: Vec<IrEdge>,
    pub semantic_facts: Vec<SemanticFact>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub path: String,
    pub range: Option<SourceRange>,
    pub severity: ParseDiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnsupportedLanguage,
    Internal(String),
}

pub trait SourceParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError>;
}
