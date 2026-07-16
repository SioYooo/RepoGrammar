//! Conservative TS/JS framework adapter registry.
//!
//! These adapters are structural preview adapters. They identify bounded roles and
//! exact support targets, but they do not claim broad TypeScript semantics.

use crate::core::model::CodeUnitKind;

pub mod drizzle;
pub mod express;
pub mod fastify;
pub mod hono;
pub mod jest_vitest;
pub mod nestjs;
pub mod next;
pub mod prisma;
pub mod react;
pub mod zod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TsJsFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<TsJsFrameworkRole> {
    match kind {
        CodeUnitKind::ExpressRoute => Some(TsJsFrameworkRole {
            target: express::ROLE_ROUTE_HANDLER,
            note: "syntax code unit indicates Express route handler role",
            assumption: "handler binding unresolved",
        }),
        CodeUnitKind::ReactComponent => Some(TsJsFrameworkRole {
            target: react::ROLE_COMPONENT,
            note: "syntax code unit indicates React component role",
            assumption: "component runtime behavior unresolved",
        }),
        CodeUnitKind::ReactHook => Some(TsJsFrameworkRole {
            target: react::ROLE_HOOK,
            note: "syntax code unit indicates React hook role",
            assumption: "hook lifecycle behavior unresolved",
        }),
        CodeUnitKind::TestSuite => Some(TsJsFrameworkRole {
            target: jest_vitest::ROLE_SUITE,
            note: "syntax code unit indicates Jest or Vitest suite role",
            assumption: "test runner binding unresolved",
        }),
        CodeUnitKind::TestCase => Some(TsJsFrameworkRole {
            target: jest_vitest::ROLE_TEST,
            note: "syntax code unit indicates Jest or Vitest test role",
            assumption: "test runner binding unresolved",
        }),
        CodeUnitKind::NextAppPage => Some(TsJsFrameworkRole {
            target: next::ROLE_APP_PAGE,
            note: "syntax code unit indicates Next.js App Router page role",
            assumption: "Next.js server/client semantics unresolved",
        }),
        CodeUnitKind::NextAppLayout => Some(TsJsFrameworkRole {
            target: next::ROLE_APP_LAYOUT,
            note: "syntax code unit indicates Next.js App Router layout role",
            assumption: "Next.js layout semantics unresolved",
        }),
        CodeUnitKind::NextRouteHandler => Some(TsJsFrameworkRole {
            target: next::ROLE_ROUTE_HANDLER,
            note: "syntax code unit indicates Next.js route handler role",
            assumption: "Next.js route semantics unresolved",
        }),
        CodeUnitKind::NextPagesApiRoute => Some(TsJsFrameworkRole {
            target: next::ROLE_PAGES_API_ROUTE,
            note: "syntax code unit indicates Next.js Pages API route role",
            assumption: "Next.js Pages API route semantics unresolved",
        }),
        CodeUnitKind::NextPagesPage => Some(TsJsFrameworkRole {
            target: next::ROLE_PAGES_PAGE,
            note: "syntax code unit indicates Next.js Pages Router page role",
            assumption: "Next.js Pages Router semantics unresolved",
        }),
        CodeUnitKind::FastifyRoute => Some(TsJsFrameworkRole {
            target: fastify::ROLE_ROUTE_HANDLER,
            note: "syntax code unit indicates Fastify route handler role",
            assumption: "Fastify plugin/prefix context unresolved",
        }),
        CodeUnitKind::PrismaQuery => Some(TsJsFrameworkRole {
            target: prisma::ROLE_QUERY,
            note: "syntax code unit indicates Prisma query role",
            assumption: "Prisma client extensions unresolved",
        }),
        CodeUnitKind::PrismaTransaction => Some(TsJsFrameworkRole {
            target: prisma::ROLE_TRANSACTION,
            note: "syntax code unit indicates Prisma transaction role",
            assumption: "Prisma transaction semantics unresolved",
        }),
        CodeUnitKind::DrizzleSchemaTable => Some(TsJsFrameworkRole {
            target: drizzle::ROLE_SCHEMA_TABLE,
            note: "syntax code unit indicates Drizzle schema table role",
            assumption: "Drizzle schema import context unresolved",
        }),
        CodeUnitKind::DrizzleQuery => Some(TsJsFrameworkRole {
            target: drizzle::ROLE_QUERY,
            note: "syntax code unit indicates Drizzle query role",
            assumption: "Drizzle query builder semantics unresolved",
        }),
        CodeUnitKind::DrizzleTransaction => Some(TsJsFrameworkRole {
            target: drizzle::ROLE_TRANSACTION,
            note: "syntax code unit indicates Drizzle transaction role",
            assumption: "Drizzle transaction semantics unresolved",
        }),
        CodeUnitKind::ZodSchema => Some(TsJsFrameworkRole {
            target: zod::ROLE_SCHEMA,
            note: "syntax code unit indicates Zod schema role",
            assumption: "Zod runtime refinement semantics unresolved",
        }),
        CodeUnitKind::NestController => Some(TsJsFrameworkRole {
            target: nestjs::ROLE_CONTROLLER,
            note: "syntax code unit indicates NestJS controller role",
            assumption: "NestJS dependency injection unresolved",
        }),
        CodeUnitKind::NestRoute => Some(TsJsFrameworkRole {
            target: nestjs::ROLE_ROUTE,
            note: "syntax code unit indicates NestJS route role",
            assumption: "NestJS controller identity unresolved",
        }),
        CodeUnitKind::NestInjectable => Some(TsJsFrameworkRole {
            target: nestjs::ROLE_INJECTABLE,
            note: "syntax code unit indicates NestJS injectable role",
            assumption: "NestJS dependency injection unresolved",
        }),
        CodeUnitKind::NestModule => Some(TsJsFrameworkRole {
            target: nestjs::ROLE_MODULE,
            note: "syntax code unit indicates NestJS module role",
            assumption: "NestJS dynamic module metadata unresolved",
        }),
        CodeUnitKind::HonoRoute => Some(TsJsFrameworkRole {
            target: hono::ROLE_ROUTE,
            note: "syntax code unit indicates Hono route role",
            assumption: "Hono receiver/middleware context unresolved",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:express")
        || framework_role.starts_with("framework:react")
        || framework_role.starts_with("framework:jest_vitest")
        || framework_role.starts_with("framework:next")
        || framework_role.starts_with("framework:fastify")
        || framework_role.starts_with("framework:prisma")
        || framework_role.starts_with("framework:drizzle")
        || framework_role.starts_with("framework:zod")
        || framework_role.starts_with("framework:nestjs")
        || framework_role.starts_with("framework:hono")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        express::ROLE_ROUTE_HANDLER => Some(express::SUPPORT_TARGETS.contains(&target)),
        jest_vitest::ROLE_SUITE => Some(jest_vitest::SUITE_TARGETS.contains(&target)),
        jest_vitest::ROLE_TEST => Some(jest_vitest::TEST_TARGETS.contains(&target)),
        react::ROLE_COMPONENT | react::ROLE_HOOK => Some(false),
        next::ROLE_APP_PAGE => Some(target == next::TARGET_APP_PAGE),
        next::ROLE_APP_LAYOUT => Some(target == next::TARGET_APP_LAYOUT),
        next::ROLE_ROUTE_HANDLER => Some(next::ROUTE_HANDLER_TARGETS.contains(&target)),
        next::ROLE_PAGES_API_ROUTE => Some(target == next::TARGET_PAGES_API_ROUTE),
        next::ROLE_PAGES_PAGE => Some(target == next::TARGET_PAGES_PAGE),
        fastify::ROLE_ROUTE_HANDLER => Some(fastify::SUPPORT_TARGETS.contains(&target)),
        prisma::ROLE_QUERY => Some(prisma::QUERY_TARGETS.contains(&target)),
        prisma::ROLE_TRANSACTION => Some(target == prisma::TARGET_TRANSACTION),
        drizzle::ROLE_SCHEMA_TABLE => Some(target == drizzle::TARGET_SCHEMA_TABLE),
        drizzle::ROLE_QUERY => Some(drizzle::QUERY_TARGETS.contains(&target)),
        drizzle::ROLE_TRANSACTION => Some(target == drizzle::TARGET_TRANSACTION),
        zod::ROLE_SCHEMA => Some(zod::SCHEMA_TARGETS.contains(&target)),
        nestjs::ROLE_CONTROLLER => Some(target == nestjs::TARGET_CONTROLLER),
        nestjs::ROLE_ROUTE => Some(nestjs::ROUTE_TARGETS.contains(&target)),
        nestjs::ROLE_INJECTABLE => Some(target == nestjs::TARGET_INJECTABLE),
        nestjs::ROLE_MODULE => Some(target == nestjs::TARGET_MODULE),
        hono::ROLE_ROUTE => Some(hono::SUPPORT_TARGETS.contains(&target)),
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        next::ROLE_ROUTE_HANDLER if target.starts_with("next.route.") => {
            "next.route.handler".to_string()
        }
        fastify::ROLE_ROUTE_HANDLER if target.starts_with("fastify.route.") => {
            "fastify.route.handler".to_string()
        }
        prisma::ROLE_QUERY if target.starts_with("prisma.query.") => "prisma.query".to_string(),
        drizzle::ROLE_QUERY if target.starts_with("drizzle.query.") => "drizzle.query".to_string(),
        nestjs::ROLE_ROUTE if target.starts_with("nestjs.common.") => "nestjs.route".to_string(),
        hono::ROLE_ROUTE if target.starts_with("hono.route.") => "hono.route".to_string(),
        zod::ROLE_SCHEMA if target.starts_with("zod.") => "zod.schema".to_string(),
        _ => target.to_string(),
    }
}

pub(crate) fn derived_from_for_target(target: &str) -> Option<&'static str> {
    if target.starts_with("express.") {
        Some("tsjs_express_structural_anchors")
    } else if target.starts_with("jest_vitest.")
        || target.starts_with("mocha.")
        || target.starts_with("node_test.")
    {
        Some("tsjs_jest_vitest_structural_anchors")
    } else if target.starts_with("next.") {
        Some("tsjs_next_structural_anchors")
    } else if target.starts_with("fastify.") {
        Some("tsjs_fastify_structural_anchors")
    } else if target.starts_with("prisma.") {
        Some("tsjs_prisma_structural_anchors")
    } else if target.starts_with("drizzle.") {
        Some("tsjs_drizzle_structural_anchors")
    } else if target.starts_with("zod.") {
        Some("tsjs_zod_structural_anchors")
    } else if target.starts_with("nestjs.") {
        Some("tsjs_nestjs_structural_anchors")
    } else if target.starts_with("hono.") {
        Some("tsjs_hono_structural_anchors")
    } else {
        None
    }
}

pub(crate) fn expected_derived_from(framework_role: &str) -> Option<&'static str> {
    if framework_role.starts_with("framework:express") {
        Some("tsjs_express_structural_anchors")
    } else if framework_role.starts_with("framework:jest_vitest") {
        Some("tsjs_jest_vitest_structural_anchors")
    } else if framework_role.starts_with("framework:next") {
        Some("tsjs_next_structural_anchors")
    } else if framework_role.starts_with("framework:fastify") {
        Some("tsjs_fastify_structural_anchors")
    } else if framework_role.starts_with("framework:prisma") {
        Some("tsjs_prisma_structural_anchors")
    } else if framework_role.starts_with("framework:drizzle") {
        Some("tsjs_drizzle_structural_anchors")
    } else if framework_role.starts_with("framework:zod") {
        Some("tsjs_zod_structural_anchors")
    } else if framework_role.starts_with("framework:nestjs") {
        Some("tsjs_nestjs_structural_anchors")
    } else if framework_role.starts_with("framework:hono") {
        Some("tsjs_hono_structural_anchors")
    } else {
        None
    }
}
