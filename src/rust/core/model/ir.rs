//! Lightweight unified IR owned by RepoGrammar.
//!
//! Tree-sitter AST nodes are intentionally not exposed here.

use super::{CodeUnit, CodeUnitId, CodeUnitKind, Provenance, SourceRange};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IrNodeId(String);

impl IrNodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            Err("IR node id must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn for_code_unit(code_unit_id: &CodeUnitId) -> Result<Self, String> {
        Self::new(format!("ir:{}", code_unit_id.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrNodeKind {
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

impl IrNodeKind {
    pub fn from_code_unit_kind(kind: &CodeUnitKind) -> Self {
        match kind {
            CodeUnitKind::Module => Self::Module,
            CodeUnitKind::Function => Self::Function,
            CodeUnitKind::AsyncFunction => Self::AsyncFunction,
            CodeUnitKind::ArrowFunction => Self::ArrowFunction,
            CodeUnitKind::Class => Self::Class,
            CodeUnitKind::Method => Self::Method,
            CodeUnitKind::ReactComponent => Self::ReactComponent,
            CodeUnitKind::ReactHook => Self::ReactHook,
            CodeUnitKind::ExpressRoute => Self::ExpressRoute,
            CodeUnitKind::TestSuite => Self::TestSuite,
            CodeUnitKind::TestCase => Self::TestCase,
            CodeUnitKind::FastApiRoute => Self::FastApiRoute,
            CodeUnitKind::PytestTest => Self::PytestTest,
            CodeUnitKind::PytestFixture => Self::PytestFixture,
            CodeUnitKind::PydanticModel => Self::PydanticModel,
            CodeUnitKind::SqlAlchemyModel => Self::SqlAlchemyModel,
            CodeUnitKind::SqlAlchemyRepositoryMethod => Self::SqlAlchemyRepositoryMethod,
            CodeUnitKind::RustModule
            | CodeUnitKind::RustInlineModule
            | CodeUnitKind::RustExternalModule => Self::Module,
            CodeUnitKind::RustStruct | CodeUnitKind::RustEnum | CodeUnitKind::RustTrait => {
                Self::Class
            }
            CodeUnitKind::RustImplBlock => Self::Class,
            CodeUnitKind::RustFunction | CodeUnitKind::RustTestFunction => Self::Function,
            CodeUnitKind::RustMethod
            | CodeUnitKind::RustTraitMethod
            | CodeUnitKind::RustAssociatedFunction => Self::Method,
            CodeUnitKind::RustUseItem | CodeUnitKind::RustMacroInvocation => Self::Unknown,
            CodeUnitKind::ProjectConfig => Self::ProjectConfig,
            CodeUnitKind::Unknown => Self::Unknown,
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "module" => Ok(Self::Module),
            "function" => Ok(Self::Function),
            "async_function" => Ok(Self::AsyncFunction),
            "arrow_function" => Ok(Self::ArrowFunction),
            "class" => Ok(Self::Class),
            "method" => Ok(Self::Method),
            "react_component" => Ok(Self::ReactComponent),
            "react_hook" => Ok(Self::ReactHook),
            "express_route" => Ok(Self::ExpressRoute),
            "test_suite" => Ok(Self::TestSuite),
            "test_case" => Ok(Self::TestCase),
            "fastapi_route" => Ok(Self::FastApiRoute),
            "pytest_test" => Ok(Self::PytestTest),
            "pytest_fixture" => Ok(Self::PytestFixture),
            "pydantic_model" => Ok(Self::PydanticModel),
            "sqlalchemy_model" => Ok(Self::SqlAlchemyModel),
            "sqlalchemy_repository_method" => Ok(Self::SqlAlchemyRepositoryMethod),
            "project_config" => Ok(Self::ProjectConfig),
            "unknown" => Ok(Self::Unknown),
            _ => Err(format!("unsupported IR node kind: {value}")),
        }
    }

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
pub struct IrNode {
    pub id: IrNodeId,
    pub code_unit_id: CodeUnitId,
    pub kind: IrNodeKind,
    pub range: SourceRange,
    pub provenance: Provenance,
}

impl IrNode {
    pub fn from_code_unit(unit: &CodeUnit) -> Result<Self, String> {
        Ok(Self {
            id: IrNodeId::for_code_unit(&unit.id)?,
            code_unit_id: unit.id.clone(),
            kind: IrNodeKind::from_code_unit_kind(&unit.kind),
            range: unit.range.clone(),
            provenance: unit.provenance.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrEdgeLabel {
    Contains,
}

impl IrEdgeLabel {
    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "contains" => Ok(Self::Contains),
            _ => Err(format!("unsupported IR edge label: {value}")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "contains",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEdge {
    pub from_node_id: IrNodeId,
    pub to_node_id: IrNodeId,
    pub label: IrEdgeLabel,
}

impl IrEdge {
    pub fn new(
        from_node_id: IrNodeId,
        to_node_id: IrNodeId,
        label: IrEdgeLabel,
    ) -> Result<Self, String> {
        if from_node_id == to_node_id {
            Err("IR edge must not point to itself".to_string())
        } else {
            Ok(Self {
                from_node_id,
                to_node_id,
                label,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, Language, RepositoryRevision};

    fn code_unit() -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new("unit:src/a.ts#function:0-10:0").expect("valid id"),
            language: Language::TypeScript,
            kind: CodeUnitKind::Function,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                "src/a.ts",
                ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
        }
    }

    #[test]
    fn ir_node_id_rejects_empty_or_control_values() {
        assert!(IrNodeId::new(" ").is_err());
        assert!(IrNodeId::new("ir:bad\nid").is_err());
    }

    #[test]
    fn code_unit_ir_node_is_repo_owned_and_deterministic() {
        let unit = code_unit();
        let node = IrNode::from_code_unit(&unit).expect("build IR node");

        assert_eq!(node.id.as_str(), "ir:unit:src/a.ts#function:0-10:0");
        assert_eq!(node.code_unit_id, unit.id);
        assert_eq!(node.kind, IrNodeKind::Function);
        assert_eq!(node.range.start_byte, 0);
        assert_eq!(node.provenance.path, "src/a.ts");
    }

    #[test]
    fn ir_edges_reject_self_edges() {
        let id = IrNodeId::new("ir:unit:src/a.ts#module:0-10:0").expect("valid id");

        assert!(IrEdge::new(id.clone(), id, IrEdgeLabel::Contains).is_err());
    }
}
