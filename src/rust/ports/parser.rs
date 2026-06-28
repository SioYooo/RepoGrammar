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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserProjectContext {
    pub python_module_paths: Vec<String>,
    pub python_source_roots: Vec<String>,
    pub python_conftest_files: Vec<ParserProjectFileContext>,
    pub tsjs_module_paths: Vec<String>,
    pub tsjs_path_aliases: Vec<ParserTsJsPathAlias>,
    pub tsjs_package_dependencies: Vec<String>,
    pub tsjs_has_test_runner_context: bool,
    pub rust_module_paths: Vec<String>,
    pub rust_cargo_files: Vec<ParserProjectFileContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserProjectFileContext {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserTsJsPathAlias {
    pub alias_pattern: String,
    pub target_patterns: Vec<String>,
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

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }
}
