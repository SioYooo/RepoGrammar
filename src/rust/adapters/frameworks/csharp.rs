//! Conservative C# framework adapter registry (bounded v0.2 preview).
//!
//! These adapters identify only exact source-visible ASP.NET Core, EF Core,
//! and xUnit/NUnit/MSTest roles. They do not claim MSBuild evaluation, source
//! generator output, runtime dependency-injection, assembly scanning,
//! convention routing, or Razor semantics.

use crate::adapters::parsing::csharp::{CSHARP_ANCHOR_ENGINE, CSHARP_ANCHOR_METHOD};
use crate::core::model::{CodeUnitKind, LanguageFamilyPack, RoleFamilyPolicy, UnknownReasonCode};

pub(crate) const ROLE_ASPNET_CONTROLLER: &str = "framework:aspnetcore.controller";
pub(crate) const ROLE_ASPNET_CONTROLLER_ACTION: &str = "framework:aspnetcore.controller_action";
pub(crate) const ROLE_ASPNET_MINIMAL_ROUTE: &str = "framework:aspnetcore.minimal_route";
pub(crate) const ROLE_EFCORE_DB_CONTEXT: &str = "framework:efcore.db_context";
pub(crate) const ROLE_EFCORE_ENTITY_SET: &str = "framework:efcore.entity_set";
pub(crate) const ROLE_FLUENTVALIDATION_VALIDATOR: &str = "framework:fluentvalidation.validator";
pub(crate) const ROLE_XUNIT_TEST: &str = "framework:xunit.test";
pub(crate) const ROLE_NUNIT_TEST: &str = "framework:nunit.test";
pub(crate) const ROLE_MSTEST_TEST: &str = "framework:mstest.test";

pub(crate) const ASPNET_CONTROLLER_TARGETS: &[&str] = &[
    "aspnetcore.mvc.ApiController",
    "aspnetcore.mvc.ControllerBase",
    "aspnetcore.mvc.Controller",
];

pub(crate) const ASPNET_MVC_NAMESPACE: &str = "Microsoft.AspNetCore.Mvc";
pub(crate) const EFCORE_NAMESPACE: &str = "Microsoft.EntityFrameworkCore";
pub(crate) const FLUENTVALIDATION_NAMESPACE: &str = "FluentValidation";
pub(crate) const XUNIT_NAMESPACE: &str = "Xunit";
pub(crate) const NUNIT_NAMESPACE: &str = "NUnit.Framework";
pub(crate) const MSTEST_NAMESPACE: &str = "Microsoft.VisualStudio.TestTools.UnitTesting";

/// `(attribute simple name, support target, HTTP method token)`; the targets are
/// the controller-action role's support targets.
pub(crate) const HTTP_ROUTE_ATTRIBUTES: &[(&str, &str, &str)] = &[
    ("HttpGet", "aspnetcore.mvc.HttpGet", "GET"),
    ("HttpPost", "aspnetcore.mvc.HttpPost", "POST"),
    ("HttpPut", "aspnetcore.mvc.HttpPut", "PUT"),
    ("HttpDelete", "aspnetcore.mvc.HttpDelete", "DELETE"),
    ("HttpPatch", "aspnetcore.mvc.HttpPatch", "PATCH"),
    ("HttpHead", "aspnetcore.mvc.HttpHead", "HEAD"),
    ("HttpOptions", "aspnetcore.mvc.HttpOptions", "OPTIONS"),
];

/// `(invocation name, support target, HTTP method token)`; the targets are the
/// minimal-route role's support targets.
pub(crate) const MINIMAL_API_MAP_METHODS: &[(&str, &str, &str)] = &[
    ("MapGet", "aspnetcore.builder.MapGet", "GET"),
    ("MapPost", "aspnetcore.builder.MapPost", "POST"),
    ("MapPut", "aspnetcore.builder.MapPut", "PUT"),
    ("MapDelete", "aspnetcore.builder.MapDelete", "DELETE"),
    ("MapPatch", "aspnetcore.builder.MapPatch", "PATCH"),
];

/// Convention-based registration calls whose routing stays runtime-defined.
pub(crate) const CONVENTION_ROUTING_METHODS: &[&str] =
    &["MapControllerRoute", "MapHub", "MapGrpcService"];

/// Known attribute short names on class-like declarations; a lookalike without
/// exact using/FQN evidence emits a blocking `csharp_attribute_binding` UNKNOWN.
pub(crate) const KNOWN_CLASS_ATTRIBUTES: &[(&str, &str)] = &[
    ("ApiController", ASPNET_MVC_NAMESPACE),
    ("Route", ASPNET_MVC_NAMESPACE),
    ("TestFixture", NUNIT_NAMESPACE),
    ("TestClass", MSTEST_NAMESPACE),
];

/// Known attribute short names on methods; same lookalike rule as classes.
pub(crate) const KNOWN_METHOD_ATTRIBUTES: &[(&str, &str)] = &[
    ("HttpGet", ASPNET_MVC_NAMESPACE),
    ("HttpPost", ASPNET_MVC_NAMESPACE),
    ("HttpPut", ASPNET_MVC_NAMESPACE),
    ("HttpDelete", ASPNET_MVC_NAMESPACE),
    ("HttpPatch", ASPNET_MVC_NAMESPACE),
    ("HttpHead", ASPNET_MVC_NAMESPACE),
    ("HttpOptions", ASPNET_MVC_NAMESPACE),
    ("Fact", XUNIT_NAMESPACE),
    ("Theory", XUNIT_NAMESPACE),
    ("InlineData", XUNIT_NAMESPACE),
    ("MemberData", XUNIT_NAMESPACE),
    ("Test", NUNIT_NAMESPACE),
    ("TestCase", NUNIT_NAMESPACE),
    ("TestMethod", MSTEST_NAMESPACE),
    ("DataRow", MSTEST_NAMESPACE),
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
        CodeUnitKind::FluentValidationValidator => Some(CSharpFrameworkRole {
            target: ROLE_FLUENTVALIDATION_VALIDATOR,
            note: "Tree-sitter C# code unit indicates exact FluentValidation validator role",
            assumption: "FluentValidation rule chains and validator resolution unresolved",
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
        || framework_role.starts_with("framework:fluentvalidation.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_ASPNET_CONTROLLER => Some(ASPNET_CONTROLLER_TARGETS.contains(&target)),
        ROLE_ASPNET_CONTROLLER_ACTION => Some(
            HTTP_ROUTE_ATTRIBUTES
                .iter()
                .any(|(_, anchor, _)| *anchor == target),
        ),
        ROLE_ASPNET_MINIMAL_ROUTE => Some(
            MINIMAL_API_MAP_METHODS
                .iter()
                .any(|(_, anchor, _)| *anchor == target),
        ),
        ROLE_EFCORE_DB_CONTEXT => Some(target == "efcore.DbContext"),
        ROLE_FLUENTVALIDATION_VALIDATOR => Some(target == "fluentvalidation.AbstractValidator"),
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
        ROLE_FLUENTVALIDATION_VALIDATOR => "fluentvalidation.validator".to_string(),
        ROLE_EFCORE_ENTITY_SET => "efcore.entity_set".to_string(),
        ROLE_XUNIT_TEST => "xunit.test_attribute".to_string(),
        ROLE_NUNIT_TEST => "nunit.test_attribute".to_string(),
        ROLE_MSTEST_TEST => "mstest.test_attribute".to_string(),
        _ => target.to_string(),
    }
}

const ACTION_VARIATION: &[(&str, &[&str])] = &[
    ("csharp_aspnet_route_method", &["http_method:"]),
    (
        "csharp_aspnet_route_template_shape",
        &["route_template_shape:", "class_route_template_shape:"],
    ),
    (
        "csharp_aspnet_action_shape",
        &["return_shape:", "parameter_shape:"],
    ),
];
const MINIMAL_ROUTE_VARIATION: &[(&str, &[&str])] = &[
    ("csharp_minimal_route_method", &["http_method:"]),
    (
        "csharp_minimal_route_template_shape",
        &["route_template_shape:"],
    ),
];
const EFCORE_VARIATION: &[(&str, &[&str])] = &[(
    "csharp_efcore_member_shape",
    &["efcore_entity_type_shape:", "class_shape:"],
)];
const TEST_VARIATION: &[(&str, &[&str])] = &[("csharp_test_data_shape", &["test_data_shape:"])];
const SUPPORT_FAMILY: &[&str] = &["support_family:"];
const TEST_ATTRIBUTE: &[&str] = &["test_attribute:"];

const fn support_family_role(
    role: &'static str,
    feature: &'static str,
    variation: &'static [(&'static str, &'static [&'static str])],
) -> RoleFamilyPolicy {
    RoleFamilyPolicy {
        role,
        feature,
        variation,
        equal: SUPPORT_FAMILY,
        required_equal: &[],
        characteristic: SUPPORT_FAMILY,
    }
}

const fn test_role(role: &'static str, feature: &'static str) -> RoleFamilyPolicy {
    RoleFamilyPolicy {
        role,
        feature,
        variation: TEST_VARIATION,
        equal: TEST_ATTRIBUTE,
        required_equal: &[],
        characteristic: TEST_ATTRIBUTE,
    }
}

fn reason_may_block(reason: UnknownReasonCode) -> bool {
    match reason {
        UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence => true,
        UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::DynamicImport
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

/// The C# family policy read by the language-neutral family builder.
pub(crate) static FAMILY_PACK: LanguageFamilyPack = LanguageFamilyPack {
    language: "csharp",
    anchor_engine: CSHARP_ANCHOR_ENGINE,
    anchor_method: CSHARP_ANCHOR_METHOD,
    derived_support_engine: "repogrammar-csharp-derived",
    derived_support_method: "bounded_tree_sitter_csharp_anchor_v1",
    derived_from_anchor: "derived_from=tree_sitter_csharp_structural_anchors",
    min_family_support: 3,
    assumption_features: &[
        ("csharp_anchor_kind=", "anchor_kind:"),
        ("aspnet_attribute=", "aspnet_attribute:"),
        ("http_method=", "http_method:"),
        ("route_template_shape=", "route_template_shape:"),
        ("class_route_template_shape=", "class_route_template_shape:"),
        ("test_attribute=", "test_attribute:"),
        ("test_data_shape=", "test_data_shape:"),
        ("csharp_visibility_shape=", "visibility_shape:"),
        ("csharp_class_shape=", "class_shape:"),
        ("csharp_return_shape=", "return_shape:"),
        ("csharp_parameter_shape=", "parameter_shape:"),
        ("efcore_entity_type_shape=", "efcore_entity_type_shape:"),
    ],
    roles: &[
        RoleFamilyPolicy {
            role: ROLE_ASPNET_CONTROLLER_ACTION,
            feature: "framework_aspnetcore_controller_action",
            variation: ACTION_VARIATION,
            equal: &["anchor_kind:", "aspnet_attribute:"],
            required_equal: &["http_method:", "route_template_shape:"],
            characteristic: &[
                "anchor_kind:",
                "aspnet_attribute:",
                "http_method:",
                "route_template_shape:",
            ],
        },
        RoleFamilyPolicy {
            role: ROLE_ASPNET_MINIMAL_ROUTE,
            feature: "framework_aspnetcore_minimal_route",
            variation: MINIMAL_ROUTE_VARIATION,
            equal: &[],
            required_equal: &["http_method:", "route_template_shape:"],
            characteristic: &["http_method:", "route_template_shape:"],
        },
        RoleFamilyPolicy {
            role: ROLE_ASPNET_CONTROLLER,
            feature: "framework_aspnetcore_controller",
            variation: &[("csharp_controller_class_shape", &["class_shape:"])],
            equal: &["aspnet_attribute:", "class_shape:"],
            required_equal: &[],
            characteristic: &["aspnet_attribute:", "class_shape:"],
        },
        support_family_role(
            ROLE_EFCORE_DB_CONTEXT,
            "framework_efcore_db_context",
            EFCORE_VARIATION,
        ),
        support_family_role(
            ROLE_EFCORE_ENTITY_SET,
            "framework_efcore_entity_set",
            EFCORE_VARIATION,
        ),
        support_family_role(
            ROLE_FLUENTVALIDATION_VALIDATOR,
            "framework_fluentvalidation_validator",
            &[],
        ),
        test_role(ROLE_XUNIT_TEST, "framework_xunit_test"),
        test_role(ROLE_NUNIT_TEST, "framework_nunit_test"),
        test_role(ROLE_MSTEST_TEST, "framework_mstest_test"),
    ],
    default_affected_claim: "csharp_family_membership",
    recovery_scope: "C#",
    reason_may_block,
    blocking_claims: &[
        "csharp_family_membership",
        "csharp_attribute_binding",
        "csharp_controller_identity",
        "csharp_minimal_api_receiver",
        "csharp_test_class_identity",
        "csharp_build_variant",
    ],
    non_blocking_reasons: &[
        UnknownReasonCode::RuntimeDependencyInjection,
        UnknownReasonCode::FrameworkMagic,
        UnknownReasonCode::MacroOrPreprocessor,
    ],
    non_blocking_claims: &[
        "csharp_di_registration",
        "csharp_aspnet_filter_pipeline",
        "csharp_aspnet_route_template",
        "csharp_aspnet_convention_routing",
        "csharp_partial_external",
        "csharp_generated_source",
        "csharp_dynamic_binding",
        "csharp_test_member_data",
    ],
    role_is_known: framework_role_is_known,
    support_target_is_role_compatible,
    support_family,
};
