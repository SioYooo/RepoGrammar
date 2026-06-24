//! Tree-sitter adapter boundary.
//!
//! The actual Tree-sitter dependency is intentionally deferred until parser
//! behavior and language grammar wiring are designed.

use crate::core::model::CodeUnit;
use crate::ports::parser::{ParseError, SourceDocument, SourceParser};

#[derive(Debug, Default)]
pub struct TreeSitterParserBoundary;

impl SourceParser for TreeSitterParserBoundary {
    fn parse(&self, _document: SourceDocument<'_>) -> Result<Vec<CodeUnit>, ParseError> {
        Err(ParseError::UnsupportedLanguage)
    }
}
