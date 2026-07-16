//! Framework adapters own framework-specific recognition rules.

use crate::core::model::{
    CodeUnit, CodeUnitKind, Evidence, FactCertainty, FactOrigin, SemanticFact, SemanticFactKind,
    SymbolId,
};
use crate::core::policy::rust_self_dogfood::rust_self_dogfood_role_for_unit;
use crate::ports::framework_roles::{FrameworkRoleDetector, FrameworkRoleError};

pub mod cpp;
pub mod csharp;
pub mod express;
pub mod java;
pub mod jest;
pub mod nestjs;
pub mod react;
pub mod rust_general;
pub mod tsjs;
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
    if let Some(role) = tsjs::role_for_code_unit_kind(&unit.kind) {
        return Some(FrameworkRole {
            unit,
            target: role.target,
            note: role.note,
            assumption: role.assumption,
        });
    }
    if let Some(role) = java::role_for_code_unit_kind(&unit.kind) {
        return Some(FrameworkRole {
            unit,
            target: role.target,
            note: role.note,
            assumption: role.assumption,
        });
    }
    if let Some(role) = csharp::role_for_code_unit_kind(&unit.kind) {
        return Some(FrameworkRole {
            unit,
            target: role.target,
            note: role.note,
            assumption: role.assumption,
        });
    }
    if let Some(role) = cpp::role_for_code_unit_kind(&unit.kind) {
        return Some(FrameworkRole {
            unit,
            target: role.target,
            note: role.note,
            assumption: role.assumption,
        });
    }
    if let Some(role) = rust_general::role_for_code_unit_kind(&unit.kind) {
        return Some(FrameworkRole {
            unit,
            target: role.target,
            note: role.note,
            assumption: role.assumption,
        });
    }
    let (target, note, assumption) = match &unit.kind {
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
        CodeUnitKind::DjangoModel => (
            "framework:django.model",
            "CPython ast code unit indicates Django model role",
            "Django settings-driven behavior unresolved without provider",
        ),
        CodeUnitKind::DjangoUrlPattern => (
            "framework:django.url_pattern",
            "CPython ast code unit indicates Django URL pattern role",
            "Django string dispatch unresolved without provider",
        ),
        CodeUnitKind::DjangoTest => (
            "framework:django.test",
            "CPython ast code unit indicates Django test role",
            "Django settings-driven behavior unresolved without provider",
        ),
        CodeUnitKind::FlaskRoute => (
            "framework:flask.route",
            "CPython ast code unit indicates Flask route role",
            "Flask app context binding unresolved without provider",
        ),
        CodeUnitKind::UnittestTestMethod => (
            "framework:unittest.test",
            "CPython ast code unit indicates stdlib unittest test role",
            "unittest patch target unresolved without provider",
        ),
        CodeUnitKind::ClickCommand => (
            "framework:click.command",
            "CPython ast code unit indicates click command role",
            "click plugin composition unresolved without provider",
        ),
        CodeUnitKind::TyperCommand => (
            "framework:typer.command",
            "CPython ast code unit indicates typer command role",
            "typer app composition unresolved without provider",
        ),
        CodeUnitKind::CeleryTask => (
            "framework:celery.task",
            "CPython ast code unit indicates Celery task role",
            "Celery runtime routing unresolved without provider",
        ),
        _ => {
            let rust_role = rust_self_dogfood_role_for_unit(
                unit.provenance.path.as_str(),
                unit.kind.as_str(),
                unit.id.as_str(),
            )?;
            (
                rust_role.framework_role,
                rust_role.note,
                rust_role.unresolved_assumption,
            )
        }
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
                unit(CodeUnitKind::NextAppPage, "next-app-page"),
                unit(CodeUnitKind::NextAppLayout, "next-app-layout"),
                unit(CodeUnitKind::NextRouteHandler, "next-route"),
                unit(CodeUnitKind::NextPagesApiRoute, "next-api"),
                unit(CodeUnitKind::NextPagesPage, "next-page"),
                unit(CodeUnitKind::FastifyRoute, "fastify"),
                unit(CodeUnitKind::PrismaQuery, "prisma-query"),
                unit(CodeUnitKind::PrismaTransaction, "prisma-transaction"),
                unit(CodeUnitKind::DrizzleSchemaTable, "drizzle-schema"),
                unit(CodeUnitKind::DrizzleQuery, "drizzle-query"),
                unit(CodeUnitKind::DrizzleTransaction, "drizzle-transaction"),
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
                java_unit(CodeUnitKind::SpringMvcRoute, "spring-route"),
                java_unit(CodeUnitKind::SpringComponent, "spring-component"),
                java_unit(
                    CodeUnitKind::SpringBootApplication,
                    "spring-boot-application",
                ),
                java_unit(CodeUnitKind::SpringDataRepository, "spring-data-repository"),
                csharp_unit(CodeUnitKind::AspNetController, "aspnet-controller"),
                csharp_unit(CodeUnitKind::AspNetControllerAction, "aspnet-action"),
                csharp_unit(CodeUnitKind::AspNetMinimalApiRoute, "aspnet-minimal-route"),
                csharp_unit(CodeUnitKind::EfCoreDbContext, "efcore-db-context"),
                csharp_unit(CodeUnitKind::EfCoreEntitySet, "efcore-entity-set"),
                csharp_unit(CodeUnitKind::XunitTestMethod, "xunit-test"),
                csharp_unit(CodeUnitKind::NunitTestMethod, "nunit-test"),
                csharp_unit(CodeUnitKind::MstestTestMethod, "mstest-test"),
                cpp_unit(CodeUnitKind::GtestTestCase, "gtest-test"),
                cpp_unit(CodeUnitKind::GtestTestFixture, "gtest-fixture"),
                cpp_unit(CodeUnitKind::Catch2TestCase, "catch2-test"),
                cpp_unit(CodeUnitKind::DoctestTestCase, "doctest-test"),
                cpp_unit(CodeUnitKind::BoostTestCase, "boost-test"),
                cpp_unit(CodeUnitKind::BoostTestSuite, "boost-suite"),
                rust_unit(
                    CodeUnitKind::RustFunction,
                    "src/rust/application/indexing.rs",
                    "index_repository",
                ),
                rust_unit(
                    CodeUnitKind::RustTestFunction,
                    "src/rust/bin/repogrammar.rs",
                    "product_runtime",
                ),
            ])
            .expect("detect roles");

        assert_eq!(facts.len(), 42);
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
                && !fact.evidence.provenance.path.starts_with('/')
                && !fact.evidence.provenance.path.contains("..")
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
                "framework:next.app.page",
                "framework:next.app.layout",
                "framework:next.route.handler",
                "framework:next.pages.api_route",
                "framework:next.pages.page",
                "framework:fastify.route_handler",
                "framework:prisma.query",
                "framework:prisma.transaction",
                "framework:drizzle.schema.table",
                "framework:drizzle.query",
                "framework:drizzle.transaction",
                "framework:jest_vitest.suite",
                "framework:jest_vitest.test",
                "framework:fastapi.route",
                "framework:pytest.test",
                "framework:pytest.fixture",
                "framework:pydantic.model",
                "framework:sqlalchemy.model",
                "framework:sqlalchemy.repository_method",
                "framework:spring.mvc_route",
                "framework:spring.component",
                "framework:spring_boot.application",
                "framework:spring_data.repository",
                "framework:aspnetcore.controller",
                "framework:aspnetcore.controller_action",
                "framework:aspnetcore.minimal_route",
                "framework:efcore.db_context",
                "framework:efcore.entity_set",
                "framework:xunit.test",
                "framework:nunit.test",
                "framework:mstest.test",
                "framework:gtest.test",
                "framework:gtest.fixture",
                "framework:catch2.test",
                "framework:doctest.test",
                "framework:boost_test.test",
                "framework:boost_test.suite",
                "framework:repogrammar.rust_indexing_phase",
                "framework:repogrammar.rust_product_test"
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
                unit(CodeUnitKind::RustFunction, "rust-helper"),
                unit(CodeUnitKind::QtObjectClass, "qt-object"),
                unit(CodeUnitKind::Unknown, "unknown"),
            ])
            .expect("detect roles");

        assert!(facts.is_empty());
    }

    fn java_unit(kind: CodeUnitKind, id_suffix: &str) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(format!("unit:src/main/java/App.java#{id_suffix}"))
                .expect("valid id"),
            language: Language::Java,
            kind,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                "src/main/java/App.java",
                ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
        }
    }

    fn csharp_unit(kind: CodeUnitKind, id_suffix: &str) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(format!("unit:src/Api/Program.cs#{id_suffix}")).expect("valid id"),
            language: Language::CSharp,
            kind,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                "src/Api/Program.cs",
                ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
        }
    }

    fn cpp_unit(kind: CodeUnitKind, id_suffix: &str) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(format!("unit:tests/catalog_test.cc#{id_suffix}"))
                .expect("valid id"),
            language: Language::Cpp,
            kind,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                "tests/catalog_test.cc",
                ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
        }
    }

    fn rust_unit(kind: CodeUnitKind, path: &str, name: &str) -> CodeUnit {
        CodeUnit {
            id: CodeUnitId::new(format!("unit:{path}#{}:{name}:0-10:0", kind.as_str()))
                .expect("valid id"),
            language: Language::Rust,
            kind,
            range: SourceRange::new(0, 10).expect("valid range"),
            provenance: Provenance::new(
                path,
                ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
        }
    }
}
