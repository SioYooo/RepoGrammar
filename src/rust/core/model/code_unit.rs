//! Repository-owned representation of analyzable source units.

use super::provenance::Provenance;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeUnitId(String);

impl CodeUnitId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("code unit id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub start_byte: usize,
    pub end_byte: usize,
}

impl SourceRange {
    pub fn new(start_byte: usize, end_byte: usize) -> Result<Self, String> {
        if start_byte > end_byte {
            Err("source range start must not exceed end".to_string())
        } else {
            Ok(Self {
                start_byte,
                end_byte,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    TypeScript,
    JavaScript,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeUnitKind {
    Function,
    Class,
    Module,
    TestCase,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnit {
    pub id: CodeUnitId,
    pub language: Language,
    pub kind: CodeUnitKind,
    pub range: SourceRange,
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    #[test]
    fn rejects_empty_code_unit_ids() {
        assert!(CodeUnitId::new("   ").is_err());
    }

    #[test]
    fn rejects_reversed_source_ranges() {
        assert!(SourceRange::new(10, 2).is_err());
    }

    #[test]
    fn builds_code_unit_without_external_parser_types() {
        let unit = CodeUnit {
            id: CodeUnitId::new("unit:handler").expect("valid id"),
            language: Language::TypeScript,
            kind: CodeUnitKind::Function,
            range: SourceRange::new(0, 42).expect("valid range"),
            provenance: Provenance::new(
                "src/handler.ts",
                ContentHash::new(
                    "sha256:7c6e428e33561b59254d2efa13efac30fc391e9dc5d42f6c58132aaa8b2c8a03",
                )
                .expect("valid hash"),
                RepositoryRevision::new("rev-1").expect("valid revision"),
            )
            .expect("valid provenance"),
        };

        assert_eq!(unit.id.as_str(), "unit:handler");
    }
}
