//! The one authoritative classifier for what an indexing generation holds.
//!
//! This decision has exactly one input: the number of code units the generation
//! actually holds. It is deliberately not derived from a discovery report.
//!
//! Discovery can only say what a run *intended* to parse. Whether a generation
//! holds units is settled after parsing, and the mode is never persisted, so
//! the recorded unit count is the only durable fact about it. Classifying from
//! discovery on one path and from the unit count on another gives two answers
//! under one name whenever an admitted file yields nothing -- an undecodable
//! source, or one whose frontend recognizes no declaration.
//!
//! Callers may route, format, persist, or test this result; they must not
//! re-derive it from raw fields.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexingGenerationMode {
    FileManifestOnly,
    SyntaxOnlyCodeUnits,
}

impl IndexingGenerationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "file_manifest_only",
            Self::SyntaxOnlyCodeUnits => "syntax_only_code_units",
        }
    }

    /// The `parser` field restates this same decision. `docs/specifications/cli.md`
    /// pairs the two tokens in every case it enumerates, including an unchanged
    /// incremental round that reports `syntax_only` with zero parser attempts, so
    /// this is one decision expressed twice and not a second decision about
    /// whether the parser ran.
    pub fn parser_status(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "deferred",
            Self::SyntaxOnlyCodeUnits => "syntax_only",
        }
    }

    pub fn human_summary(self) -> &'static str {
        match self {
            Self::FileManifestOnly => "file manifest stored",
            Self::SyntaxOnlyCodeUnits => "syntax-only code units stored",
        }
    }
}

/// Classify a generation by the code units it holds.
pub fn generation_mode_for_code_unit_count(code_unit_count: usize) -> IndexingGenerationMode {
    if code_unit_count > 0 {
        IndexingGenerationMode::SyntaxOnlyCodeUnits
    } else {
        IndexingGenerationMode::FileManifestOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generation_holds_syntax_only_code_units_exactly_when_it_holds_units() {
        assert_eq!(
            generation_mode_for_code_unit_count(0),
            IndexingGenerationMode::FileManifestOnly
        );
        assert_eq!(
            generation_mode_for_code_unit_count(1),
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
        assert_eq!(
            generation_mode_for_code_unit_count(usize::MAX),
            IndexingGenerationMode::SyntaxOnlyCodeUnits
        );
    }

    #[test]
    fn protocol_tokens_are_stable() {
        assert_eq!(
            IndexingGenerationMode::FileManifestOnly.as_str(),
            "file_manifest_only"
        );
        assert_eq!(
            IndexingGenerationMode::SyntaxOnlyCodeUnits.as_str(),
            "syntax_only_code_units"
        );
        assert_eq!(
            IndexingGenerationMode::FileManifestOnly.parser_status(),
            "deferred"
        );
        assert_eq!(
            IndexingGenerationMode::SyntaxOnlyCodeUnits.parser_status(),
            "syntax_only"
        );
    }
}
