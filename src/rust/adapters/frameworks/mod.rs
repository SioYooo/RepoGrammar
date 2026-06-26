//! Framework adapters own framework-specific recognition rules.

use crate::core::model::{
    CodeUnit, CodeUnitKind, Evidence, FactCertainty, FactOrigin, SemanticFact, SemanticFactKind,
    SymbolId,
};
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};

pub mod express;
pub mod jest;
pub mod nestjs;
pub mod react;
pub mod vitest;

pub trait FrameworkAdapter {
    fn framework_name(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxFrameworkRoleDetector;

impl FrameworkRoleDetector for SyntaxFrameworkRoleDetector {
    fn detect_roles(&self, units: &[CodeUnit]) -> Result<Vec<SemanticFact>, FrameworkRoleError> {
        units
            .iter()
            .filter_map(framework_role_for_unit)
            .map(framework_role_fact)
            .collect()
    }
}

struct FrameworkRole<'a> {
    unit: &'a CodeUnit,
    target: &'static str,
    note: &'static str,
    assumption: &'static str,
}

fn framework_role_for_unit(unit: &CodeUnit) -> Option<FrameworkRole<'_>> {
    let (target, note, assumption) = match &unit.kind {
        CodeUnitKind::ExpressRoute => (
            "framework:express.route_handler",
            "syntax code unit indicates Express route handler role",
            "handler binding unresolved",
        ),
        CodeUnitKind::ReactComponent => (
            "framework:react.component",
            "syntax code unit indicates React component role",
            "component runtime behavior unresolved",
        ),
        CodeUnitKind::ReactHook => (
            "framework:react.hook",
            "syntax code unit indicates React hook role",
            "hook lifecycle behavior unresolved",
        ),
        CodeUnitKind::TestSuite => (
            "framework:jest_vitest.suite",
            "syntax code unit indicates Jest or Vitest suite role",
            "test runner binding unresolved",
        ),
        CodeUnitKind::TestCase => (
            "framework:jest_vitest.test",
            "syntax code unit indicates Jest or Vitest test role",
            "test runner binding unresolved",
        ),
        CodeUnitKind::FastApiRoute => (
            "framework:fastapi.route",
            "CPython ast code unit indicates FastAPI route role",
            "FastAPI binding unresolved without provider",
        ),
        CodeUnitKind::PytestTest => (
            "framework:pytest.test",
            "CPython ast code unit indicates pytest test role",
            "pytest fixture binding unresolved",
        ),
        CodeUnitKind::PytestFixture => (
            "framework:pytest.fixture",
            "CPython ast code unit indicates pytest fixture role",
            "pytest fixture graph unresolved",
        ),
        CodeUnitKind::PydanticModel => (
            "framework:pydantic.model",
            "CPython ast code unit indicates Pydantic model role",
            "Pydantic runtime validation behavior unresolved",
        ),
        CodeUnitKind::SqlAlchemyModel => (
            "framework:sqlalchemy.model",
            "CPython ast code unit indicates SQLAlchemy model role",
            "SQLAlchemy mapping behavior unresolved",
        ),
        CodeUnitKind::SqlAlchemyRepositoryMethod => (
            "framework:sqlalchemy.repository_method",
            "CPython ast code unit indicates SQLAlchemy repository method role",
            "SQLAlchemy transaction behavior unresolved",
        ),
        _ => return None,
    };
    Some(FrameworkRole {
        unit,
        target,
        note,
        assumption,
    })
}

fn framework_role_fact(role: FrameworkRole<'_>) -> Result<SemanticFact, FrameworkRoleError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::FrameworkRole,
        subject: role.unit.id.as_str().to_string(),
        target: Some(SymbolId::new(role.target).map_err(FrameworkRoleError::InvalidFact)?),
        origin: FactOrigin {
            engine: "repogrammar-frameworks".to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "syntax_code_unit_kind".to_string(),
        },
        certainty: FactCertainty::FrameworkHeuristic,
        evidence: Evidence::new(
            role.unit.id.clone(),
            role.unit.range.clone(),
            role.unit.provenance.clone(),
            role.note,
        )
        .map_err(FrameworkRoleError::InvalidFact)?,
        assumptions: vec![role.assumption.to_string()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        CodeUnitId, ContentHash, Language, Provenance, RepositoryRevision, SourceRange,
    };

    fn unit(kind: CodeUnitKind, id_suffix: &str) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(format!("unit:src/app.ts#{id_suffix}")).expect("valid id"),
            language: Language::TypeScript,
            kind,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                "src/app.ts",
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
    fn syntax_framework_role_detector_emits_conservative_role_facts() {
        let facts = SyntaxFrameworkRoleDetector
            .detect_roles(&[
                unit(CodeUnitKind::Module, "module"),
                unit(CodeUnitKind::ExpressRoute, "express"),
                unit(CodeUnitKind::ReactComponent, "component"),
                unit(CodeUnitKind::ReactHook, "hook"),
                unit(CodeUnitKind::TestSuite, "suite"),
                unit(CodeUnitKind::TestCase, "test"),
                unit(CodeUnitKind::FastApiRoute, "fastapi"),
                unit(CodeUnitKind::PytestTest, "pytest-test"),
                unit(CodeUnitKind::PytestFixture, "pytest-fixture"),
                unit(CodeUnitKind::PydanticModel, "pydantic"),
                unit(CodeUnitKind::SqlAlchemyModel, "sqlalchemy-model"),
                unit(
                    CodeUnitKind::SqlAlchemyRepositoryMethod,
                    "sqlalchemy-repository",
                ),
            ])
            .expect("detect roles");

        assert_eq!(facts.len(), 11);
        let forbidden_fragments = [
            "/tmp/secret",
            "UNIQUE_SOURCE_SENTINEL_DO_NOT_STORE",
            "app.get",
            "return <",
            "describe(",
            "it(",
            "=>",
            "{",
        ];
        assert!(facts.iter().all(|fact| {
            fact.kind == SemanticFactKind::FrameworkRole
                && fact.certainty == FactCertainty::FrameworkHeuristic
                && fact.origin.engine == "repogrammar-frameworks"
                && fact.origin.method == "syntax_code_unit_kind"
                && fact.evidence.provenance.path == "src/app.ts"
                && forbidden_fragments.iter().all(|fragment| {
                    !fact.subject.contains(fragment)
                        && fact
                            .target
                            .as_ref()
                            .is_none_or(|target| !target.as_str().contains(fragment))
                        && !fact.evidence.note.contains(fragment)
                })
                && fact.assumptions.iter().all(|assumption| {
                    forbidden_fragments
                        .iter()
                        .all(|fragment| !assumption.contains(fragment))
                })
        }));
        let targets = facts
            .iter()
            .map(|fact| fact.target.as_ref().expect("target").as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            [
                "framework:express.route_handler",
                "framework:react.component",
                "framework:react.hook",
                "framework:jest_vitest.suite",
                "framework:jest_vitest.test",
                "framework:fastapi.route",
                "framework:pytest.test",
                "framework:pytest.fixture",
                "framework:pydantic.model",
                "framework:sqlalchemy.model",
                "framework:sqlalchemy.repository_method"
            ]
        );
    }

    #[test]
    fn syntax_framework_role_detector_ignores_non_framework_units() {
        let facts = SyntaxFrameworkRoleDetector
            .detect_roles(&[
                unit(CodeUnitKind::Module, "module"),
                unit(CodeUnitKind::Function, "function"),
                unit(CodeUnitKind::AsyncFunction, "async"),
                unit(CodeUnitKind::ArrowFunction, "arrow"),
                unit(CodeUnitKind::Class, "class"),
                unit(CodeUnitKind::Method, "method"),
                unit(CodeUnitKind::Unknown, "unknown"),
            ])
            .expect("detect roles");

        assert!(facts.is_empty());
    }
}
