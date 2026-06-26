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
    Python,
    PythonConfig,
    Unknown(String),
}

impl Language {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::PythonConfig => "python-config",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeUnitKind {
    Module,
    Function,
    AsyncFunction,
    ArrowFunction,
    Class,
    Method,
    ReactComponent,
    ReactHook,
    ExpressRoute,
    TestSuite,
    TestCase,
    FastApiRoute,
    PytestTest,
    PytestFixture,
    PydanticModel,
    SqlAlchemyModel,
    SqlAlchemyRepositoryMethod,
    ProjectConfig,
    Unknown,
}

impl CodeUnitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Function => "function",
            Self::AsyncFunction => "async_function",
            Self::ArrowFunction => "arrow_function",
            Self::Class => "class",
            Self::Method => "method",
            Self::ReactComponent => "react_component",
            Self::ReactHook => "react_hook",
            Self::ExpressRoute => "express_route",
            Self::TestSuite => "test_suite",
            Self::TestCase => "test_case",
            Self::FastApiRoute => "fastapi_route",
            Self::PytestTest => "pytest_test",
            Self::PytestFixture => "pytest_fixture",
            Self::PydanticModel => "pydantic_model",
            Self::SqlAlchemyModel => "sqlalchemy_model",
            Self::SqlAlchemyRepositoryMethod => "sqlalchemy_repository_method",
            Self::ProjectConfig => "project_config",
            Self::Unknown => "unknown",
        }
    }
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

    #[test]
    fn python_language_and_unit_kinds_use_stable_tokens() {
        assert_eq!(Language::Python.as_str(), "python");
        assert_eq!(Language::PythonConfig.as_str(), "python-config");
        assert_eq!(CodeUnitKind::AsyncFunction.as_str(), "async_function");
        assert_eq!(CodeUnitKind::FastApiRoute.as_str(), "fastapi_route");
        assert_eq!(CodeUnitKind::PytestTest.as_str(), "pytest_test");
        assert_eq!(CodeUnitKind::PytestFixture.as_str(), "pytest_fixture");
        assert_eq!(CodeUnitKind::PydanticModel.as_str(), "pydantic_model");
        assert_eq!(CodeUnitKind::SqlAlchemyModel.as_str(), "sqlalchemy_model");
        assert_eq!(
            CodeUnitKind::SqlAlchemyRepositoryMethod.as_str(),
            "sqlalchemy_repository_method"
        );
        assert_eq!(CodeUnitKind::ProjectConfig.as_str(), "project_config");
    }
}
