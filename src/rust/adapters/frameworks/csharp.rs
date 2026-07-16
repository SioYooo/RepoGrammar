//! Conservative C# framework adapter registry (bounded v0.2 preview).
//!
//! These adapters identify only exact source-visible ASP.NET Core, EF Core,
//! and xUnit/NUnit/MSTest roles. They do not claim MSBuild evaluation, source
//! generator output, runtime dependency-injection, assembly scanning,
//! convention routing, or Razor semantics.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_ASPNET_CONTROLLER: &str = "framework:aspnetcore.controller";
pub(crate) const ROLE_ASPNET_CONTROLLER_ACTION: &str = "framework:aspnetcore.controller_action";
pub(crate) const ROLE_ASPNET_MINIMAL_ROUTE: &str = "framework:aspnetcore.minimal_route";
pub(crate) const ROLE_EFCORE_DB_CONTEXT: &str = "framework:efcore.db_context";
pub(crate) const ROLE_EFCORE_ENTITY_SET: &str = "framework:efcore.entity_set";
pub(crate) const ROLE_XUNIT_TEST: &str = "framework:xunit.test";
pub(crate) const ROLE_NUNIT_TEST: &str = "framework:nunit.test";
pub(crate) const ROLE_MSTEST_TEST: &str = "framework:mstest.test";

pub(crate) const ASPNET_CONTROLLER_TARGETS: &[&str] = &[
    "aspnetcore.mvc.ApiController",
    "aspnetcore.mvc.ControllerBase",
    "aspnetcore.mvc.Controller",
];

pub(crate) const ASPNET_CONTROLLER_ACTION_TARGETS: &[&str] = &[
    "aspnetcore.mvc.HttpGet",
    "aspnetcore.mvc.HttpPost",
    "aspnetcore.mvc.HttpPut",
    "aspnetcore.mvc.HttpDelete",
    "aspnetcore.mvc.HttpPatch",
    "aspnetcore.mvc.HttpHead",
    "aspnetcore.mvc.HttpOptions",
];

pub(crate) const ASPNET_MINIMAL_ROUTE_TARGETS: &[&str] = &[
    "aspnetcore.builder.MapGet",
    "aspnetcore.builder.MapPost",
    "aspnetcore.builder.MapPut",
    "aspnetcore.builder.MapDelete",
    "aspnetcore.builder.MapPatch",
];

pub(crate) const XUNIT_TEST_TARGETS: &[&str] = &["xunit.Fact", "xunit.Theory"];

pub(crate) const NUNIT_TEST_TARGETS: &[&str] =
    &["nunit.framework.Test", "nunit.framework.TestCase"];

pub(crate) const MSTEST_TEST_TARGETS: &[&str] = &[
    "mstest.unittesting.TestMethod",
    "mstest.unittesting.DataRow",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CSharpFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<CSharpFrameworkRole> {
    match kind {
        CodeUnitKind::AspNetController => Some(CSharpFrameworkRole {
            target: ROLE_ASPNET_CONTROLLER,
            note: "Tree-sitter C# code unit indicates exact ASP.NET Core controller role",
            assumption: "ASP.NET Core runtime dependency injection unresolved",
        }),
        CodeUnitKind::AspNetControllerAction => Some(CSharpFrameworkRole {
            target: ROLE_ASPNET_CONTROLLER_ACTION,
            note: "Tree-sitter C# code unit indicates exact ASP.NET Core controller action role",
            assumption: "ASP.NET Core runtime routing dispatch unresolved",
        }),
        CodeUnitKind::AspNetMinimalApiRoute => Some(CSharpFrameworkRole {
            target: ROLE_ASPNET_MINIMAL_ROUTE,
            note: "Tree-sitter C# code unit indicates exact ASP.NET Core minimal API route role",
            assumption: "ASP.NET Core endpoint pipeline behavior unresolved",
        }),
        CodeUnitKind::EfCoreDbContext => Some(CSharpFrameworkRole {
            target: ROLE_EFCORE_DB_CONTEXT,
            note: "Tree-sitter C# code unit indicates exact EF Core DbContext role",
            assumption: "EF Core runtime model building unresolved",
        }),
        CodeUnitKind::EfCoreEntitySet => Some(CSharpFrameworkRole {
            target: ROLE_EFCORE_ENTITY_SET,
            note: "Tree-sitter C# code unit indicates exact EF Core entity set role",
            assumption: "EF Core entity mapping behavior unresolved",
        }),
        CodeUnitKind::XunitTestMethod => Some(CSharpFrameworkRole {
            target: ROLE_XUNIT_TEST,
            note: "Tree-sitter C# code unit indicates exact xUnit test role",
            assumption: "xUnit runtime test discovery unresolved",
        }),
        CodeUnitKind::NunitTestMethod => Some(CSharpFrameworkRole {
            target: ROLE_NUNIT_TEST,
            note: "Tree-sitter C# code unit indicates exact NUnit test role",
            assumption: "NUnit runtime test discovery unresolved",
        }),
        CodeUnitKind::MstestTestMethod => Some(CSharpFrameworkRole {
            target: ROLE_MSTEST_TEST,
            note: "Tree-sitter C# code unit indicates exact MSTest test role",
            assumption: "MSTest runtime test discovery unresolved",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:aspnetcore.")
        || framework_role.starts_with("framework:efcore.")
        || framework_role.starts_with("framework:xunit.")
        || framework_role.starts_with("framework:nunit.")
        || framework_role.starts_with("framework:mstest.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_ASPNET_CONTROLLER => Some(ASPNET_CONTROLLER_TARGETS.contains(&target)),
        ROLE_ASPNET_CONTROLLER_ACTION => Some(ASPNET_CONTROLLER_ACTION_TARGETS.contains(&target)),
        ROLE_ASPNET_MINIMAL_ROUTE => Some(ASPNET_MINIMAL_ROUTE_TARGETS.contains(&target)),
        ROLE_EFCORE_DB_CONTEXT => Some(target == "efcore.DbContext"),
        ROLE_EFCORE_ENTITY_SET => Some(target == "efcore.DbSet"),
        ROLE_XUNIT_TEST => Some(XUNIT_TEST_TARGETS.contains(&target)),
        ROLE_NUNIT_TEST => Some(NUNIT_TEST_TARGETS.contains(&target)),
        ROLE_MSTEST_TEST => Some(MSTEST_TEST_TARGETS.contains(&target)),
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_ASPNET_CONTROLLER => "aspnetcore.mvc.controller".to_string(),
        ROLE_ASPNET_CONTROLLER_ACTION => "aspnetcore.mvc.http_attribute_route".to_string(),
        ROLE_ASPNET_MINIMAL_ROUTE => "aspnetcore.minimal.map_route".to_string(),
        ROLE_EFCORE_DB_CONTEXT => "efcore.db_context".to_string(),
        ROLE_EFCORE_ENTITY_SET => "efcore.entity_set".to_string(),
        ROLE_XUNIT_TEST => "xunit.test_attribute".to_string(),
        ROLE_NUNIT_TEST => "nunit.test_attribute".to_string(),
        ROLE_MSTEST_TEST => "mstest.test_attribute".to_string(),
        _ => target.to_string(),
    }
}
