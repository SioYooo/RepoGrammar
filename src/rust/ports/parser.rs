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
    pub python_module_files: Vec<ParserProjectFileContext>,
    pub python_source_roots: Vec<String>,
    pub python_conftest_files: Vec<ParserProjectFileContext>,
    pub tsjs_module_paths: Vec<String>,
    pub tsjs_path_aliases: Vec<ParserTsJsPathAlias>,
    pub tsjs_root_dirs: Vec<String>,
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

/// Parser output that carries indexing-only metadata alongside the normalized
/// report. Most frontends return no Python interface hash; the Python frontend
/// supplies the exact hash already computed by its `parse_document` request so
/// indexing can persist it without launching a second worker process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceParseOutput {
    pub report: ParseReport,
    pub python_interface_hash: Option<String>,
}

impl SourceParseOutput {
    pub fn from_report(report: ParseReport) -> Self {
        Self {
            report,
            python_interface_hash: None,
        }
    }
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
    Timeout,
    PythonFrontendContractMismatch,
    Internal(String),
}

/// Result of the single-file Python interface probe the incremental-sync
/// preflight runs on a modified `.py` module before deciding whether the edit is
/// file-local. The interface projection (top-level symbols, literal `__all__`,
/// `__init__` re-exports) is the exact channel by which one module's text
/// reaches another module's parse, so an unchanged hash proves the edit cannot
/// affect any other file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonInterfaceProbe {
    /// The worker computed the module interface hash for the supplied file
    /// content. The preflight compares it against the base generation's stored
    /// hash.
    Computed(String),
    /// The interface could not be computed — the parser does not analyze Python,
    /// or the worker errored, timed out, or reported a contract mismatch. The
    /// preflight treats this as `python_interface_unverified` and falls back to a
    /// full rebuild; it never guesses an interface.
    Unverified,
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

    /// Parse with project context and return any indexing-only metadata emitted
    /// by that same frontend request. The default preserves existing parser
    /// behavior; only the Python frontend currently attaches metadata.
    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        self.parse_with_context(document, context)
            .map(SourceParseOutput::from_report)
    }

    /// Compute the file-local Python interface hash for `text` at `path`. The
    /// default is the conservative-safe answer for any parser that does not
    /// analyze Python (`Unverified` forces a full rebuild); only the Python
    /// frontend overrides it with a computed hash. This is deliberately *not* an
    /// unsound silent fallback: the preflight routes `Unverified` to an explicit
    /// full rebuild, never to the incremental path.
    fn extract_python_interface(&self, _path: &str, _text: &str) -> PythonInterfaceProbe {
        PythonInterfaceProbe::Unverified
    }
}
