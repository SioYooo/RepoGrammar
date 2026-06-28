//! Conservative TS/JS exact-anchor extraction.
//!
//! This pass runs after syntax-only code-unit extraction. It emits `STRUCTURAL`
//! semantic facts ONLY for code units whose framework usage can be resolved
//! through exact import/require bindings and literal call shapes. Anything that
//! is dynamic, reassigned, shadowed, conditionally imported, or merely a
//! lookalike yields no anchor, so the family layer keeps it `UNKNOWN`. These
//! structural anchors are later promoted to bounded `DATAFLOW_DERIVED` support
//! facts by the application layer; they never prove membership by themselves.

use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, ParserProjectContext, SourceDocument};
use std::collections::{BTreeMap, BTreeSet};

/// Engine identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_ENGINE: &str = "repogrammar-tsjs-syntax";
/// Method identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_METHOD: &str = "exact_anchor_v1";

const EXPRESS_HTTP_METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "use"];
const FASTIFY_HTTP_METHODS: [&str; 8] = [
    "get", "head", "post", "put", "delete", "options", "patch", "all",
];
const NEXT_HTTP_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const PRISMA_OPERATIONS: [&str; 13] = [
    "findMany",
    "findUnique",
    "findFirst",
    "create",
    "createMany",
    "update",
    "updateMany",
    "upsert",
    "delete",
    "deleteMany",
    "count",
    "aggregate",
    "groupBy",
];
const DRIZZLE_TABLE_FACTORIES: [&str; 3] = ["pgTable", "mysqlTable", "sqliteTable"];
const RUNNER_MODULES: [&str; 2] = ["vitest", "@jest/globals"];

/// Extract exact framework anchors for the given units. Returns `STRUCTURAL`
/// facts whose evidence spans the full owning unit range.
pub fn exact_framework_anchors(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    context: Option<&ParserProjectContext>,
) -> Result<Vec<SemanticFact>, ParseError> {
    let bindings = ModuleBindings::analyze(document.text);
    let mut facts = Vec::new();
    for unit in units {
        match anchor_for_unit(document, context, &bindings, unit) {
            AnchorOutcome::Anchor(anchor) => facts.push(anchor_fact(document, unit, anchor)?),
            AnchorOutcome::Unknown(unknown) => facts.push(unknown_fact(document, unit, unknown)?),
            AnchorOutcome::None => {}
        }
    }
    Ok(facts)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TestRunnerCallNames {
    pub suite_names: BTreeSet<String>,
    pub test_names: BTreeSet<String>,
}

pub(crate) fn exact_test_runner_call_names(text: &str) -> TestRunnerCallNames {
    let bindings = ModuleBindings::analyze(text);
    let mut names = TestRunnerCallNames::default();
    for (local, binding) in &bindings.imports {
        if bindings.unsafe_names.contains(local)
            || !RUNNER_MODULES.contains(&binding.module.as_str())
        {
            continue;
        }
        let ImportKind::Named(original) = &binding.kind else {
            continue;
        };
        match original.as_str() {
            "describe" => {
                names.suite_names.insert(local.clone());
            }
            "it" | "test" => {
                names.test_names.insert(local.clone());
            }
            _ => {}
        }
    }
    names
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    target: String,
    fact_kind: SemanticFactKind,
    assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnknownAnchor {
    reason: UnknownReasonCode,
    affected_claim: &'static str,
    kind: &'static str,
    note: &'static str,
}

enum AnchorOutcome {
    Anchor(Anchor),
    Unknown(UnknownAnchor),
    None,
}

fn anchor_for_unit(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
    bindings: &ModuleBindings,
    unit: &CodeUnit,
) -> AnchorOutcome {
    let Some(slice) = document
        .text
        .get(unit.range.start_byte..unit.range.end_byte)
    else {
        return AnchorOutcome::None;
    };
    match unit.kind {
        CodeUnitKind::ExpressRoute => express_route_anchor(bindings, slice),
        CodeUnitKind::NextAppPage
        | CodeUnitKind::NextAppLayout
        | CodeUnitKind::NextRouteHandler
        | CodeUnitKind::NextPagesApiRoute
        | CodeUnitKind::NextPagesPage => next_anchor(document, context, unit, slice),
        CodeUnitKind::FastifyRoute => fastify_route_anchor(bindings, slice),
        CodeUnitKind::PrismaQuery => prisma_query_anchor(bindings, slice),
        CodeUnitKind::PrismaTransaction => prisma_transaction_anchor(bindings, slice),
        CodeUnitKind::DrizzleSchemaTable => drizzle_schema_table_anchor(bindings, slice),
        CodeUnitKind::DrizzleQuery => drizzle_query_anchor(bindings, slice),
        CodeUnitKind::DrizzleTransaction => drizzle_transaction_anchor(bindings, slice),
        CodeUnitKind::TestSuite => test_anchor(
            document,
            bindings,
            slice,
            true,
            context.is_some_and(|context| context.tsjs_has_test_runner_context),
        ),
        CodeUnitKind::TestCase => test_anchor(
            document,
            bindings,
            slice,
            false,
            context.is_some_and(|context| context.tsjs_has_test_runner_context),
        ),
        _ => AnchorOutcome::None,
    }
}

fn express_route_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_support_target",
            kind: "dynamic_route_call",
            note: "TS/JS route call shape is dynamic",
        });
    };
    if !EXPRESS_HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "tsjs_support_target",
            kind: "unsupported_route_method",
            note: "TS/JS route method is not in the exact anchor allowlist",
        });
    }
    if bindings.unsafe_names.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_receiver_binding",
            kind: "unsafe_receiver_binding",
            note: "TS/JS route receiver is reassigned or redeclared",
        });
    }
    if !bindings.express_receivers.contains_key(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "tsjs_receiver_binding",
            kind: "unresolved_express_receiver",
            note: "TS/JS route receiver is not an exact Express app/router binding",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=express_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
    ];
    if let Some(path_shape) = route_path_shape(slice) {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("express.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn next_anchor(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
    unit: &CodeUnit,
    slice: &str,
) -> AnchorOutcome {
    if !context.is_some_and(|context| tsjs_context_has_package(context, "next")) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::MissingProjectConfig,
            affected_claim: "next_project_context",
            kind: "next_missing_package_context",
            note: "Next.js file convention requires package context",
        });
    }
    if let Some(unknown) = next_path_unknown(document.path) {
        return AnchorOutcome::Unknown(unknown);
    }
    match unit.kind {
        CodeUnitKind::NextAppPage => next_component_anchor(
            "next.app.page",
            "next_app_page",
            "app",
            "page",
            slice,
            document.path,
        ),
        CodeUnitKind::NextAppLayout => next_component_anchor(
            "next.app.layout",
            "next_app_layout",
            "app",
            "layout",
            slice,
            document.path,
        ),
        CodeUnitKind::NextPagesPage => next_component_anchor(
            "next.pages.page",
            "next_pages_page",
            "pages",
            "page",
            slice,
            document.path,
        ),
        CodeUnitKind::NextPagesApiRoute => next_pages_api_route_anchor(slice, document.path),
        CodeUnitKind::NextRouteHandler => next_route_handler_anchor(slice, document.path),
        _ => AnchorOutcome::None,
    }
}

fn next_component_anchor(
    target: &'static str,
    anchor_kind: &'static str,
    router_kind: &'static str,
    file_convention: &'static str,
    slice: &str,
    path: &str,
) -> AnchorOutcome {
    if !slice.contains("export default") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "next_default_export",
            kind: "next_reexported_page_unknown",
            note: "Next.js page/layout default export is not exact and local",
        });
    }
    let component_shape = if contains_jsx_like(slice) {
        "jsx_component"
    } else if slice.contains("createElement") {
        "create_element_component"
    } else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "next_component_shape",
            kind: "next_component_body_unknown",
            note: "Next.js page/layout component body is not an exact JSX/createElement anchor",
        });
    };
    AnchorOutcome::Anchor(Anchor {
        target: target.to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            format!("tsjs_anchor_kind={anchor_kind}"),
            format!("router_kind={router_kind}"),
            format!("file_convention={file_convention}"),
            format!("route_path_shape={}", next_route_path_shape(path)),
            format!("component_shape={component_shape}"),
            "server_client_directive=unknown".to_string(),
        ],
    })
}

fn next_pages_api_route_anchor(slice: &str, path: &str) -> AnchorOutcome {
    if !slice.contains("export default") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "next_pages_api_export",
            kind: "next_reexported_page_unknown",
            note: "Next.js Pages API route export is not exact and local",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "next.pages.api_route".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=next_pages_api_route".to_string(),
            "router_kind=pages".to_string(),
            "file_convention=api_route".to_string(),
            format!("route_path_shape={}", next_route_path_shape(path)),
            format!("response_shape={}", response_shape(slice)),
            format!("async_shape={}", async_shape(slice)),
        ],
    })
}

fn next_route_handler_anchor(slice: &str, path: &str) -> AnchorOutcome {
    let Some(method) = next_route_method(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "next_route_handler_export",
            kind: "next_route_handler_export_unknown",
            note: "Next.js route handler export is not an exact HTTP method function",
        });
    };
    AnchorOutcome::Anchor(Anchor {
        target: format!("next.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=next_route_handler".to_string(),
            "router_kind=app".to_string(),
            "file_convention=route".to_string(),
            format!("http_method={method}"),
            format!("route_path_shape={}", next_route_path_shape(path)),
            format!("response_shape={}", response_shape(slice)),
            format!("fetch_shape={}", fetch_shape(slice)),
            format!("async_shape={}", async_shape(slice)),
            "server_client_directive=server_assumed".to_string(),
        ],
    })
}

fn fastify_route_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_route_shape",
            kind: "fastify_dynamic_route_call",
            note: "Fastify route call shape is dynamic",
        });
    };
    if bindings.unsafe_names.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_reassigned",
            note: "Fastify receiver is reassigned or redeclared",
        });
    }
    if !bindings.fastify_receivers.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_unresolved",
            note: "Fastify route receiver is not an exact Fastify binding",
        });
    }
    if method == "route" {
        return fastify_full_route_anchor(slice);
    }
    if !FASTIFY_HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify route method is not in the exact allowlist",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=fastify_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
        format!("schema_present={}", slice.contains("schema")),
        format!("opts_handler_present={}", slice.contains("handler")),
        format!("reply_shape={}", reply_shape(slice)),
        "plugin_context=none".to_string(),
        "prefix_unknown=false".to_string(),
    ];
    if let Some(path_shape) = route_path_shape(slice) {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("fastify.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn fastify_full_route_anchor(slice: &str) -> AnchorOutcome {
    let Some(method) = object_literal_string_field(slice, "method") else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify full route method is not a literal string",
        });
    };
    let method = method.to_ascii_lowercase();
    if !FASTIFY_HTTP_METHODS.contains(&method.as_str()) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify full route method is not in the exact allowlist",
        });
    }
    let path_shape = object_literal_string_field(slice, "url")
        .or_else(|| object_literal_string_field(slice, "path"))
        .map(|path| normalize_route_path(&path));
    let mut assumptions = vec![
        "tsjs_anchor_kind=fastify_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
        format!("schema_present={}", slice.contains("schema")),
        "opts_handler_present=true".to_string(),
        format!("reply_shape={}", reply_shape(slice)),
        "plugin_context=none".to_string(),
        "prefix_unknown=false".to_string(),
    ];
    if let Some(path_shape) = path_shape {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    AnchorOutcome::Anchor(Anchor {
        target: "fastify.route.full_declaration".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn prisma_query_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    if raw_sql_present(slice) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_raw_query",
            note: "Prisma raw SQL query is not support evidence",
        });
    }
    let Some((client, model, operation)) = prisma_query_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_dynamic_model_or_operation",
            note: "Prisma query model or operation is dynamic",
        });
    };
    if bindings.unsafe_names.contains(client) || !bindings.prisma_clients.contains(client) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma client is not an exact local PrismaClient binding",
        });
    }
    if !PRISMA_OPERATIONS.contains(&operation) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_dynamic_model_or_operation",
            note: "Prisma operation is not in the exact allowlist",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("prisma.query.{operation}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=prisma_query".to_string(),
            format!("model_name={model}"),
            format!("operation={operation}"),
            format!("where_shape={}", object_clause_shape(slice, "where")),
            format!("select_include_shape={}", select_include_shape(slice)),
            "transaction_shape=none".to_string(),
            format!("raw_sql_present={}", raw_sql_present(slice)),
        ],
    })
}

fn prisma_transaction_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some(client) = prisma_transaction_client(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_transaction_shape",
            kind: "prisma_transaction_callback",
            note: "Prisma transaction shape is not exact",
        });
    };
    if bindings.unsafe_names.contains(client) || !bindings.prisma_clients.contains(client) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma transaction client is not an exact local PrismaClient binding",
        });
    }
    if !slice.contains("$transaction([") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_transaction_shape",
            kind: "prisma_transaction_callback",
            note: "Prisma callback transaction is not a safe exact anchor",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "prisma.transaction".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=prisma_transaction".to_string(),
            "operation=transaction".to_string(),
            "transaction_shape=array".to_string(),
            format!("raw_sql_present={}", raw_sql_present(slice)),
        ],
    })
}

fn drizzle_schema_table_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some((table_name, factory)) = drizzle_table_declaration_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_schema_table",
            kind: "drizzle_dynamic_table",
            note: "Drizzle schema table declaration is dynamic",
        });
    };
    if !bindings.drizzle_table_factories.contains(factory) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_table_binding",
            kind: "drizzle_ambiguous_table_import",
            note: "Drizzle table factory is not an exact Drizzle import",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "drizzle.schema.table".to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=drizzle_schema_table".to_string(),
            "operation=schema_table".to_string(),
            format!("table_name={table_name}"),
            "where_shape=none".to_string(),
            "returning_shape=none".to_string(),
            "join_shape=none".to_string(),
            format!("sql_template_present={}", raw_sql_present(slice)),
        ],
    })
}

fn drizzle_query_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    if raw_sql_present(slice) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_query_shape",
            kind: "drizzle_raw_sql",
            note: "Drizzle raw SQL template is not support evidence",
        });
    }
    let Some((db, operation, table)) = drizzle_query_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_query_shape",
            kind: "drizzle_dynamic_query_builder",
            note: "Drizzle query builder shape is dynamic",
        });
    };
    if !bindings.drizzle_dbs.contains(db) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_db_binding",
            kind: "drizzle_db_binding_unresolved",
            note: "Drizzle db binding is not exact",
        });
    }
    if !bindings.drizzle_tables.contains(table) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_table_binding",
            kind: "drizzle_table_unresolved",
            note: "Drizzle query table is not an exact table declaration",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("drizzle.query.{operation}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=drizzle_query".to_string(),
            format!("operation={operation}"),
            format!("table_name={table}"),
            format!("where_shape={}", object_clause_shape(slice, "where")),
            format!("returning_shape={}", slice.contains(".returning(")),
            format!("join_shape={}", drizzle_join_shape(slice)),
            format!("transaction_shape={}", slice.contains(".transaction(")),
            format!("sql_template_present={}", raw_sql_present(slice)),
        ],
    })
}

fn drizzle_transaction_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some(db) = drizzle_transaction_db(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_transaction_shape",
            kind: "drizzle_dynamic_query_builder",
            note: "Drizzle transaction shape is dynamic",
        });
    };
    if !bindings.drizzle_dbs.contains(db) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_db_binding",
            kind: "drizzle_db_binding_unresolved",
            note: "Drizzle transaction db binding is not exact",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "drizzle.transaction".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=drizzle_transaction".to_string(),
            "operation=transaction".to_string(),
            "transaction_shape=callback".to_string(),
            format!("sql_template_present={}", raw_sql_present(slice)),
        ],
    })
}

fn test_anchor(
    document: &SourceDocument<'_>,
    bindings: &ModuleBindings,
    slice: &str,
    is_suite: bool,
    ambient_runner_allowed: bool,
) -> AnchorOutcome {
    let Some(name) = test_call_name(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "dynamic_test_call",
            note: "TS/JS test runner call shape is dynamic",
        });
    };
    if bindings.unsafe_names.contains(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is locally reassigned or redeclared",
        });
    }
    if bindings.local_decls.contains(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is a local custom wrapper",
        });
    }
    if let Some((module, original)) = bindings.imported_runner(name) {
        if (is_suite && original == "describe") || (!is_suite && matches!(original, "it" | "test"))
        {
            return AnchorOutcome::Anchor(test_anchor_for_runner(
                name, original, module, is_suite, slice,
            ));
        }
    }
    if bindings.imports.contains_key(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "unresolved_test_runner",
            note: "TS/JS test runner import does not resolve to a known runner",
        });
    }
    let expected_ambient = if is_suite {
        name == "describe"
    } else {
        name == "it" || name == "test"
    };
    if expected_ambient && is_ambient_runner(document.path, bindings, name) {
        if !ambient_runner_allowed {
            return AnchorOutcome::Unknown(UnknownAnchor {
                reason: UnknownReasonCode::MissingProjectConfig,
                affected_claim: "tsjs_runner_binding",
                kind: "ambient_runner_without_project_context",
                note: "TS/JS ambient test runner lacks bounded project test context",
            });
        }
        return AnchorOutcome::Anchor(test_anchor_for_runner(
            name, name, "ambient", is_suite, slice,
        ));
    }
    AnchorOutcome::Unknown(UnknownAnchor {
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: "tsjs_runner_binding",
        kind: "unresolved_test_runner",
        note: "TS/JS test runner binding is not exact",
    })
}

fn test_anchor_for_runner(
    local_name: &str,
    original: &str,
    runner_kind: &str,
    is_suite: bool,
    slice: &str,
) -> Anchor {
    Anchor {
        target: format!("jest_vitest.{original}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            format!(
                "tsjs_anchor_kind={}",
                if is_suite { "test_suite" } else { "test_case" }
            ),
            format!("runner_kind={runner_kind}"),
            format!("test_shape={original}"),
            format!("async_shape={}", async_shape(slice)),
            format!("import_context={local_name}"),
        ],
    }
}

/// A bare `describe`/`it`/`test` is only treated as a runner global in an actual
/// test file and only when the name is not locally declared or imported from a
/// non-runner module (a custom wrapper / alias).
fn is_ambient_runner(path: &str, bindings: &ModuleBindings, name: &str) -> bool {
    is_test_file(path)
        && !bindings.local_decls.contains(name)
        && !bindings.imports.contains_key(name)
}

fn is_test_file(path: &str) -> bool {
    const SUFFIXES: [&str; 8] = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

fn tsjs_context_has_package(context: &ParserProjectContext, package: &str) -> bool {
    context
        .tsjs_package_dependencies
        .iter()
        .any(|dependency| dependency == package)
}

fn next_path_unknown(path: &str) -> Option<UnknownAnchor> {
    if path.contains("/(") || path.contains("/@") {
        Some(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "next_route_convention",
            kind: "next_route_group_semantics_unknown",
            note: "Next.js route group or parallel route semantics are not modeled",
        })
    } else if path.contains("/[") {
        Some(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "next_route_convention",
            kind: "next_dynamic_segment_unknown",
            note:
                "Next.js dynamic segment semantics are not modeled beyond normalized path metadata",
        })
    } else {
        None
    }
}

fn next_route_path_shape(path: &str) -> String {
    let mut path = path.trim_start_matches("./").to_string();
    for extension in [".tsx", ".jsx", ".ts", ".js"] {
        if path.ends_with(extension) {
            path.truncate(path.len() - extension.len());
            break;
        }
    }
    for suffix in ["/page", "/layout", "/route"] {
        if path.ends_with(suffix) {
            path.truncate(path.len() - suffix.len());
            break;
        }
    }
    if let Some(rest) = path.strip_prefix("app/") {
        normalize_route_path(&format!("/{rest}"))
    } else if let Some(rest) = path.strip_prefix("pages/api/") {
        normalize_route_path(&format!("/api/{rest}"))
    } else if let Some(rest) = path.strip_prefix("pages/") {
        normalize_route_path(&format!("/{rest}"))
    } else {
        normalize_route_path(&format!("/{path}"))
    }
}

fn next_route_method(slice: &str) -> Option<&'static str> {
    NEXT_HTTP_METHODS
        .iter()
        .copied()
        .find(|method| slice.contains(&format!("function {method}")))
}

fn contains_jsx_like(slice: &str) -> bool {
    slice.contains("return <")
        || slice.contains("</")
        || slice.contains("/>")
        || slice.contains("jsx(")
}

fn response_shape(slice: &str) -> &'static str {
    if slice.contains("NextResponse.json") || slice.contains("Response.json") {
        "response_json"
    } else if slice.contains("new Response") {
        "response_object"
    } else if slice.contains(".json(") {
        "res_json"
    } else if slice.contains(".send(") {
        "res_send"
    } else if slice.contains(".end(") {
        "res_end"
    } else {
        "response_unknown"
    }
}

fn fetch_shape(slice: &str) -> &'static str {
    if slice.contains("request.json(") {
        "request_json"
    } else if slice.contains("request.nextUrl") {
        "next_url"
    } else {
        "none"
    }
}

fn reply_shape(slice: &str) -> &'static str {
    if slice.contains(".send(") {
        "reply_send"
    } else if slice.contains(".code(") || slice.contains(".status(") {
        "reply_status"
    } else {
        "reply_unknown"
    }
}

fn object_literal_string_field(slice: &str, field: &str) -> Option<String> {
    let field_index = slice.find(field)?;
    let after_field = &slice[field_index + field.len()..];
    let after_colon = after_field.trim_start().strip_prefix(':')?.trim_start();
    first_quoted(after_colon)
}

fn object_clause_shape(slice: &str, field: &str) -> &'static str {
    let pattern = format!("{field}:");
    if slice.contains(&pattern) {
        "object_literal"
    } else {
        "none"
    }
}

fn select_include_shape(slice: &str) -> &'static str {
    match (slice.contains("select:"), slice.contains("include:")) {
        (true, true) => "select_include",
        (true, false) => "select",
        (false, true) => "include",
        (false, false) => "none",
    }
}

fn raw_sql_present(slice: &str) -> bool {
    slice.contains("sql`")
        || slice.contains("$queryRaw")
        || slice.contains("$executeRaw")
        || slice.contains("queryRaw")
        || slice.contains("executeRaw")
}

fn prisma_query_parts(slice: &str) -> Option<(&str, &str, &str)> {
    let (client, after_client) = leading_identifier(slice)?;
    let rest = slice[after_client..].trim_start().strip_prefix('.')?;
    let (model, after_model) = leading_identifier(rest)?;
    let rest = rest[after_model..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if !rest[after_operation..].trim_start().starts_with('(') {
        return None;
    }
    Some((client, model, operation))
}

fn prisma_transaction_client(slice: &str) -> Option<&str> {
    let (client, after_client) = leading_identifier(slice)?;
    slice[after_client..]
        .trim_start()
        .strip_prefix(".$transaction(")?;
    Some(client)
}

fn drizzle_table_declaration_parts(slice: &str) -> Option<(&str, &str)> {
    let trimmed = strip_export_prefix(slice.trim_start());
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (name, after_name) = leading_identifier(rest)?;
    let rhs = rest[after_name..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let (factory, after_factory) = leading_identifier(rhs)?;
    if !DRIZZLE_TABLE_FACTORIES.contains(&factory)
        || !rhs[after_factory..].trim_start().starts_with('(')
    {
        return None;
    }
    Some((name, factory))
}

fn drizzle_query_parts(slice: &str) -> Option<(&str, &str, &str)> {
    let (db, after_db) = leading_identifier(slice)?;
    let rest = slice[after_db..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if !matches!(operation, "select" | "insert" | "update" | "delete")
        || !rest[after_operation..].trim_start().starts_with('(')
    {
        return None;
    }
    if operation == "select" {
        let from_index = slice.find(".from(")?;
        let from_arg = &slice[from_index + ".from(".len()..];
        let (table, _) = leading_identifier(from_arg)?;
        return Some((db, operation, table));
    }
    let arg = &rest[after_operation..].trim_start()["(".len()..];
    let (table, _) = leading_identifier(arg)?;
    Some((db, operation, table))
}

fn drizzle_transaction_db(slice: &str) -> Option<&str> {
    let (db, after_db) = leading_identifier(slice)?;
    slice[after_db..]
        .trim_start()
        .strip_prefix(".transaction(")?;
    Some(db)
}

fn drizzle_join_shape(slice: &str) -> &'static str {
    if slice.contains("Join(") {
        "join"
    } else {
        "none"
    }
}

fn anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    anchor: Anchor,
) -> Result<SemanticFact, ParseError> {
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let evidence = Evidence::new(
        CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
        SourceRange::new(unit.range.start_byte, unit.range.end_byte)
            .map_err(ParseError::Internal)?,
        provenance,
        "bounded TS/JS exact framework anchor",
    )
    .map_err(ParseError::Internal)?;
    Ok(SemanticFact {
        kind: anchor.fact_kind,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(anchor.target).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: TSJS_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence,
        assumptions: anchor.assumptions,
    })
}

fn unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    unknown: UnknownAnchor,
) -> Result<SemanticFact, ParseError> {
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let evidence = Evidence::new(
        CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
        SourceRange::new(unit.range.start_byte, unit.range.end_byte)
            .map_err(ParseError::Internal)?,
        provenance,
        unknown.note,
    )
    .map_err(ParseError::Internal)?;
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(
            SymbolId::new(unknown.reason.as_protocol_str()).map_err(ParseError::Internal)?,
        ),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: TSJS_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence,
        assumptions: vec![
            format!("affected_claim={}", unknown.affected_claim),
            format!("tsjs_unknown_kind={}", unknown.kind),
        ],
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ImportKind {
    Default,
    Namespace,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportBinding {
    module: String,
    kind: ImportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpressReceiver {
    App,
    Router,
}

#[derive(Debug, Default)]
struct ModuleBindings {
    imports: BTreeMap<String, ImportBinding>,
    local_decls: BTreeSet<String>,
    unsafe_names: BTreeSet<String>,
    express_receivers: BTreeMap<String, ExpressReceiver>,
    fastify_receivers: BTreeSet<String>,
    prisma_clients: BTreeSet<String>,
    drizzle_table_factories: BTreeSet<String>,
    drizzle_tables: BTreeSet<String>,
    drizzle_dbs: BTreeSet<String>,
}

impl ModuleBindings {
    fn analyze(text: &str) -> Self {
        let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut reassigned: BTreeSet<String> = BTreeSet::new();
        let mut imports: BTreeMap<String, ImportBinding> = BTreeMap::new();
        let mut local_decls: BTreeSet<String> = BTreeSet::new();
        let mut top_level_lines: Vec<String> = Vec::new();

        let mut depth: i64 = 0;
        for raw_line in text.lines() {
            let at_top_level = depth == 0;
            if let Some(name) = bare_assignment_name(raw_line) {
                reassigned.insert(name.to_string());
            }
            if at_top_level {
                let import_bindings = parse_import_line(raw_line);
                let produced_imports = !import_bindings.is_empty();
                for (local, binding) in import_bindings {
                    *declared_counts.entry(local.clone()).or_insert(0) += 1;
                    imports.insert(local, binding);
                }
                // A `const x = require(...)` line is also a `const` declaration; count it
                // only once so a single require binding is not mistaken for a redeclaration.
                if !produced_imports {
                    for name in declared_identifiers(raw_line) {
                        *declared_counts.entry(name.clone()).or_insert(0) += 1;
                        local_decls.insert(name);
                    }
                }
                top_level_lines.push(raw_line.to_string());
            }
            depth += brace_delta(raw_line);
            if depth < 0 {
                depth = 0;
            }
        }

        let mut unsafe_names: BTreeSet<String> = reassigned;
        for (name, count) in &declared_counts {
            if *count > 1 {
                unsafe_names.insert(name.clone());
            }
        }

        let mut express_receivers: BTreeMap<String, ExpressReceiver> = BTreeMap::new();
        let mut fastify_receivers = BTreeSet::new();
        let mut prisma_clients = BTreeSet::new();
        let mut drizzle_table_factories = BTreeSet::new();
        let mut drizzle_tables = BTreeSet::new();
        let mut drizzle_dbs = BTreeSet::new();
        for (local, binding) in &imports {
            if binding.module.starts_with("drizzle-orm")
                && matches!(&binding.kind, ImportKind::Named(original) if DRIZZLE_TABLE_FACTORIES.contains(&original.as_str()))
                && !unsafe_names.contains(local)
            {
                drizzle_table_factories.insert(local.clone());
            }
        }
        for line in &top_level_lines {
            if let Some((name, receiver)) =
                express_receiver_declaration(line, &imports, &unsafe_names)
            {
                if !unsafe_names.contains(&name) {
                    express_receivers.insert(name, receiver);
                }
            }
            if let Some(name) = fastify_receiver_declaration(line, &imports, &unsafe_names) {
                if !unsafe_names.contains(&name) {
                    fastify_receivers.insert(name);
                }
            }
            if let Some(name) = prisma_client_declaration(line, &imports, &unsafe_names) {
                if !unsafe_names.contains(&name) {
                    prisma_clients.insert(name);
                }
            }
            if let Some((table, factory)) = drizzle_table_declaration_parts(line) {
                if drizzle_table_factories.contains(factory) && !unsafe_names.contains(table) {
                    drizzle_tables.insert(table.to_string());
                }
            }
            if let Some(name) = drizzle_db_declaration(line, &imports, &unsafe_names) {
                if !unsafe_names.contains(&name) {
                    drizzle_dbs.insert(name);
                }
            }
        }

        Self {
            imports,
            local_decls,
            unsafe_names,
            express_receivers,
            fastify_receivers,
            prisma_clients,
            drizzle_table_factories,
            drizzle_tables,
            drizzle_dbs,
        }
    }

    fn imported_runner(&self, name: &str) -> Option<(&str, &str)> {
        match self.imports.get(name) {
            Some(binding) if RUNNER_MODULES.contains(&binding.module.as_str()) => {
                match &binding.kind {
                    ImportKind::Named(original) => {
                        Some((binding.module.as_str(), original.as_str()))
                    }
                    ImportKind::Default | ImportKind::Namespace => None,
                }
            }
            _ => None,
        }
    }
}

fn brace_delta(line: &str) -> i64 {
    let mut delta = 0i64;
    for byte in line.bytes() {
        match byte {
            b'{' => delta += 1,
            b'}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn parse_import_line(line: &str) -> Vec<(String, ImportBinding)> {
    let trimmed = strip_export_prefix(line.trim());
    if let Some(rest) = trimmed.strip_prefix("import ") {
        return parse_es_import(rest);
    }
    parse_require_declaration(trimmed)
}

fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export ").unwrap_or(line)
}

fn parse_es_import(rest: &str) -> Vec<(String, ImportBinding)> {
    let Some(module) = module_after_from(rest) else {
        return Vec::new();
    };
    let clause = match rest.find(" from ") {
        Some(index) => rest[..index].trim(),
        None => return Vec::new(),
    };
    let mut bindings = Vec::new();
    let mut remaining = clause;

    if let Some(after_star) = remaining.strip_prefix("* as ") {
        if let Some((name, _)) = leading_identifier(after_star) {
            bindings.push((
                name.to_string(),
                ImportBinding {
                    module: module.clone(),
                    kind: ImportKind::Namespace,
                },
            ));
        }
        return bindings;
    }

    if !remaining.starts_with('{') {
        if let Some((name, end)) = leading_identifier(remaining) {
            bindings.push((
                name.to_string(),
                ImportBinding {
                    module: module.clone(),
                    kind: ImportKind::Default,
                },
            ));
            remaining = remaining[end..].trim_start();
            remaining = remaining
                .strip_prefix(',')
                .unwrap_or(remaining)
                .trim_start();
        }
    }

    if remaining.starts_with('{') {
        for (local, original) in parse_named_specifiers(remaining) {
            bindings.push((
                local,
                ImportBinding {
                    module: module.clone(),
                    kind: ImportKind::Named(original),
                },
            ));
        }
    }

    bindings
}

fn parse_require_declaration(line: &str) -> Vec<(String, ImportBinding)> {
    if !line.contains("require(") {
        return Vec::new();
    }
    let Some(after_keyword) = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| line.strip_prefix(keyword))
    else {
        return Vec::new();
    };
    let Some(module) = require_module(line) else {
        return Vec::new();
    };
    let lhs = match after_keyword.find('=') {
        Some(index) => after_keyword[..index].trim(),
        None => return Vec::new(),
    };
    if lhs.starts_with('{') {
        return parse_named_specifiers(lhs)
            .into_iter()
            .map(|(local, original)| {
                (
                    local,
                    ImportBinding {
                        module: module.clone(),
                        kind: ImportKind::Named(original),
                    },
                )
            })
            .collect();
    }
    match leading_identifier(lhs) {
        Some((name, _)) => vec![(
            name.to_string(),
            ImportBinding {
                module,
                kind: ImportKind::Default,
            },
        )],
        None => Vec::new(),
    }
}

fn parse_named_specifiers(clause: &str) -> Vec<(String, String)> {
    let open = match clause.find('{') {
        Some(index) => index,
        None => return Vec::new(),
    };
    let close = match clause[open..].find('}') {
        Some(index) => open + index,
        None => return Vec::new(),
    };
    let inner = &clause[open + 1..close];
    let mut specifiers = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (original, local) = match part.split_once(" as ") {
            Some((original, local)) => (original.trim(), local.trim()),
            None => (part, part),
        };
        let original = match leading_identifier(original) {
            Some((name, _)) => name.to_string(),
            None => continue,
        };
        let local = match leading_identifier(local) {
            Some((name, _)) => name.to_string(),
            None => continue,
        };
        specifiers.push((local, original));
    }
    specifiers
}

fn module_after_from(rest: &str) -> Option<String> {
    let index = rest.find(" from ")?;
    first_quoted(&rest[index + " from ".len()..])
}

fn require_module(line: &str) -> Option<String> {
    let index = line.find("require(")?;
    first_quoted(&line[index + "require(".len()..])
}

fn first_quoted(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote == b'"' || quote == b'\'' {
            let start = index + 1;
            let end_relative = text[start..].find(quote as char)?;
            return Some(text[start..start + end_relative].to_string());
        }
        index += 1;
    }
    None
}

fn declared_identifiers(line: &str) -> Vec<String> {
    let trimmed = strip_export_prefix(line.trim());
    for keyword in ["const ", "let ", "var "] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim_start();
            if rest.starts_with('{') {
                return parse_named_specifiers(rest)
                    .into_iter()
                    .map(|(local, _)| local)
                    .collect();
            }
            return leading_identifier(rest)
                .map(|(name, _)| vec![name.to_string()])
                .unwrap_or_default();
        }
    }
    for keyword in ["function ", "class "] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim_start().trim_start_matches('*').trim_start();
            return leading_identifier(rest)
                .map(|(name, _)| vec![name.to_string()])
                .unwrap_or_default();
        }
    }
    Vec::new()
}

fn express_receiver_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<(String, ExpressReceiver)> {
    let trimmed = strip_export_prefix(line.trim());
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (name, after) = leading_identifier(rest.trim_start())?;
    let after_name = &rest.trim_start()[after..];
    let rhs = after_name.trim_start().strip_prefix('=')?.trim();
    let receiver = express_receiver_from_rhs(rhs, imports, unsafe_names)?;
    Some((name.to_string(), receiver))
}

fn express_receiver_from_rhs(
    rhs: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<ExpressReceiver> {
    let rhs = rhs.trim().trim_end_matches(';').trim();
    let (head, after) = leading_identifier(rhs)?;
    if unsafe_names.contains(head) {
        return None;
    }
    let tail = rhs[after..].trim_start();
    if tail == "()" {
        let binding = imports.get(head)?;
        if binding.module != "express" {
            return None;
        }
        return match &binding.kind {
            ImportKind::Default | ImportKind::Namespace => Some(ExpressReceiver::App),
            ImportKind::Named(original) if original == "Router" => Some(ExpressReceiver::Router),
            ImportKind::Named(_) => None,
        };
    }
    let member_rest = tail.strip_prefix('.')?;
    let (member, after_member) = leading_identifier(member_rest)?;
    if member != "Router" || member_rest[after_member..].trim_start() != "()" {
        return None;
    }
    let binding = imports.get(head)?;
    if binding.module == "express"
        && matches!(binding.kind, ImportKind::Default | ImportKind::Namespace)
    {
        Some(ExpressReceiver::Router)
    } else {
        None
    }
}

fn fastify_receiver_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<String> {
    let (name, rhs) = top_level_declaration_assignment(line)?;
    let (head, after_head) = leading_identifier(rhs)?;
    if unsafe_names.contains(head) || !rhs[after_head..].trim_start().starts_with('(') {
        return None;
    }
    let binding = imports.get(head)?;
    if binding.module != "fastify" {
        return None;
    }
    match &binding.kind {
        ImportKind::Default | ImportKind::Namespace => Some(name.to_string()),
        ImportKind::Named(original) if matches!(original.as_str(), "fastify" | "Fastify") => {
            Some(name.to_string())
        }
        ImportKind::Named(_) => None,
    }
}

fn prisma_client_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<String> {
    let (name, rhs) = top_level_declaration_assignment(line)?;
    let rhs = rhs.trim().trim_end_matches(';').trim();
    let after_new = rhs.strip_prefix("new ")?;
    let (constructor, after_constructor) = leading_identifier(after_new)?;
    if unsafe_names.contains(constructor)
        || !after_new[after_constructor..].trim_start().starts_with('(')
    {
        return None;
    }
    let binding = imports.get(constructor)?;
    if binding.module == "@prisma/client"
        && matches!(&binding.kind, ImportKind::Named(original) if original == "PrismaClient")
    {
        Some(name.to_string())
    } else {
        None
    }
}

fn drizzle_db_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<String> {
    let (name, rhs) = top_level_declaration_assignment(line)?;
    let (factory, after_factory) = leading_identifier(rhs)?;
    if unsafe_names.contains(factory) || !rhs[after_factory..].trim_start().starts_with('(') {
        return None;
    }
    let binding = imports.get(factory)?;
    if binding.module.starts_with("drizzle-orm")
        && matches!(&binding.kind, ImportKind::Named(original) if original == "drizzle")
    {
        Some(name.to_string())
    } else {
        None
    }
}

fn top_level_declaration_assignment(line: &str) -> Option<(&str, &str)> {
    let trimmed = strip_export_prefix(line.trim());
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (name, after_name) = leading_identifier(rest)?;
    let rhs = rest[after_name..].trim_start().strip_prefix('=')?.trim();
    Some((name, rhs))
}

fn bare_assignment_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    for keyword in [
        "const ", "let ", "var ", "return ", "case ", "import ", "export ", "if ", "while ", "for ",
    ] {
        if trimmed.starts_with(keyword) {
            return None;
        }
    }
    let (name, after) = leading_identifier(trimmed)?;
    let rest = trimmed[after..].trim_start();
    let bytes = rest.as_bytes();
    if bytes.first() == Some(&b'=') {
        let next = bytes.get(1).copied();
        if next != Some(b'=') && next != Some(b'>') {
            return Some(name);
        }
    }
    None
}

fn route_call_parts(slice: &str) -> Option<(&str, &str)> {
    let (receiver, after) = leading_identifier(slice)?;
    let rest = slice[after..].trim_start().strip_prefix('.')?;
    let (method, after_method) = leading_identifier(rest)?;
    if !rest[after_method..].trim_start().starts_with('(') {
        return None;
    }
    Some((receiver, method))
}

fn route_path_shape(slice: &str) -> Option<String> {
    let open = slice.find('(')?;
    let path = first_quoted(&slice[open + 1..])?;
    Some(normalize_route_path(&path))
}

fn normalize_route_path(path: &str) -> String {
    let normalized = path
        .split('/')
        .map(|segment| {
            if segment.is_empty() {
                String::new()
            } else if segment.starts_with(':') {
                ":param".to_string()
            } else if segment
                .chars()
                .any(|character| character == '*' || character == '?')
            {
                ":pattern".to_string()
            } else if segment.chars().all(|character| character.is_ascii_digit()) {
                ":number".to_string()
            } else {
                segment.to_ascii_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

fn handler_shape(slice: &str) -> &'static str {
    let has_inline_arrow = slice.contains("=>");
    let has_inline_function = slice.contains("function");
    let has_req_body = slice.contains(".body");
    let has_req_query = slice.contains(".query");
    let has_req_params = slice.contains(".params");
    let has_res_json = slice.contains(".json(");
    let has_res_send = slice.contains(".send(");
    let has_res_end = slice.contains(".end(");
    match (
        has_inline_arrow || has_inline_function,
        has_req_body,
        has_req_query,
        has_req_params,
        has_res_json,
        has_res_send,
        has_res_end,
    ) {
        (true, true, _, _, true, _, _) => "inline_body_json",
        (true, _, true, _, true, _, _) => "inline_query_json",
        (true, _, _, true, true, _, _) => "inline_params_json",
        (true, _, _, _, true, _, _) => "inline_json",
        (true, _, _, _, _, true, _) => "inline_send",
        (true, _, _, _, _, _, true) => "inline_end",
        (true, _, _, _, _, _, _) => "inline_handler",
        _ => "referenced_handler",
    }
}

fn async_shape(slice: &str) -> &'static str {
    if slice.contains("async ") || slice.contains("async(") || slice.contains("async (") {
        "async"
    } else {
        "sync"
    }
}

fn test_call_name(slice: &str) -> Option<&str> {
    let (name, after) = leading_identifier(slice)?;
    if !slice[after..].trim_start().starts_with('(') {
        return None;
    }
    Some(name)
}

fn leading_identifier(text: &str) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let start = index;
    if index >= bytes.len() || !is_identifier_start(bytes[index]) {
        return None;
    }
    index += 1;
    while index < bytes.len() && is_identifier_byte(bytes[index]) {
        index += 1;
    }
    Some((&text[start..index], index))
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::core::model::{ContentHash, Language, RepositoryRevision};
    use crate::ports::parser::{ParserProjectContext, SourceParser};

    fn parse_facts(path: &str, text: &str) -> Vec<SemanticFact> {
        parse_facts_with_context(path, text, None)
    }

    fn parse_facts_with_test_context(path: &str, text: &str) -> Vec<SemanticFact> {
        parse_facts_with_context(
            path,
            text,
            Some(ParserProjectContext {
                tsjs_has_test_runner_context: true,
                ..ParserProjectContext::default()
            }),
        )
    }

    fn parse_facts_with_packages(path: &str, text: &str, packages: &[&str]) -> Vec<SemanticFact> {
        parse_facts_with_context(
            path,
            text,
            Some(ParserProjectContext {
                tsjs_package_dependencies: packages
                    .iter()
                    .map(|package| package.to_string())
                    .collect(),
                ..ParserProjectContext::default()
            }),
        )
    }

    fn parse_facts_with_context(
        path: &str,
        text: &str,
        context: Option<ParserProjectContext>,
    ) -> Vec<SemanticFact> {
        let language = if path.ends_with(".js") || path.ends_with(".jsx") {
            Language::JavaScript
        } else {
            Language::TypeScript
        };
        let document = SourceDocument {
            path,
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        };
        match context {
            Some(context) => {
                SyntaxCodeUnitParser
                    .parse_with_context(document, &context)
                    .expect("parse with context")
                    .semantic_facts
            }
            None => {
                SyntaxCodeUnitParser
                    .parse(document)
                    .expect("parse")
                    .semantic_facts
            }
        }
    }

    fn targets(path: &str, text: &str) -> Vec<String> {
        targets_from_facts(parse_facts(path, text))
    }

    fn targets_with_test_context(path: &str, text: &str) -> Vec<String> {
        targets_from_facts(parse_facts_with_test_context(path, text))
    }

    fn targets_from_facts(facts: Vec<SemanticFact>) -> Vec<String> {
        let mut targets = facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_kinds(path: &str, text: &str) -> Vec<String> {
        unknown_kinds_from_facts(parse_facts(path, text))
    }

    fn unknown_kinds_with_test_context(path: &str, text: &str) -> Vec<String> {
        unknown_kinds_from_facts(parse_facts_with_test_context(path, text))
    }

    fn targets_with_packages(path: &str, text: &str, packages: &[&str]) -> Vec<String> {
        targets_from_facts(parse_facts_with_packages(path, text, packages))
    }

    fn unknown_kinds_from_facts(facts: Vec<SemanticFact>) -> Vec<String> {
        let mut kinds = facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .filter_map(|fact| {
                fact.assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("tsjs_unknown_kind="))
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        kinds.sort();
        kinds
    }

    #[test]
    fn express_default_import_and_app_routes_anchor_each_literal_method() {
        let text = r#"import express from "express";
const app = express();
app.get("/users", (req, res) => { res.json([]); });
app.post("/users", (req, res) => { res.json({}); });
app.delete("/users/:id", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/server.ts", text),
            vec![
                "express.route.delete".to_string(),
                "express.route.get".to_string(),
                "express.route.post".to_string(),
            ]
        );
        for fact in parse_facts("src/server.ts", text) {
            assert_eq!(fact.certainty, FactCertainty::Structural);
            assert_eq!(fact.origin.engine, TSJS_ANCHOR_ENGINE);
            assert_eq!(fact.origin.method, TSJS_ANCHOR_METHOD);
        }
        let facts = parse_facts("src/server.ts", text);
        let get_fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "express.route.get")
            })
            .expect("get route fact");
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "route_method=get"));
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "route_path_shape=/users"));
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "handler_shape=inline_json"));
    }

    #[test]
    fn express_router_named_and_namespace_factories_anchor() {
        let named = r#"import { Router } from "express";
const router = Router();
router.get("/a", (req, res) => { res.end(); });
router.use((req, res, next) => { next(); });
"#;
        assert_eq!(
            targets("src/router.ts", named),
            vec![
                "express.route.get".to_string(),
                "express.route.use".to_string()
            ]
        );

        let namespaced = r#"import * as express from "express";
const router = express.Router();
router.patch("/a", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/ns.ts", namespaced),
            vec!["express.route.patch".to_string()]
        );

        let required = r#"const express = require("express");
const app = express();
app.put("/a", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/cjs.js", required),
            vec!["express.route.put".to_string()]
        );
    }

    #[test]
    fn express_object_literal_lookalike_has_no_anchor() {
        let text = r#"const app = { get(path, handler) { return handler; } };
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/fake.ts", text).is_empty());
        assert_eq!(
            unknown_kinds("src/fake.ts", text),
            vec!["unresolved_express_receiver".to_string()]
        );
    }

    #[test]
    fn express_reassigned_or_shadowed_app_has_no_anchor() {
        let reassigned = r#"import express from "express";
let app = express();
app = makeOtherApp();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/reassigned.ts", reassigned).is_empty());
        assert_eq!(
            unknown_kinds("src/reassigned.ts", reassigned),
            vec!["unsafe_receiver_binding".to_string()]
        );

        let shadowed = r#"import express from "express";
const express2 = express;
const express = buildFake();
const app = express();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/shadowed.ts", shadowed).is_empty());
        assert_eq!(
            unknown_kinds("src/shadowed.ts", shadowed),
            vec!["unresolved_express_receiver".to_string()]
        );
    }

    #[test]
    fn express_dynamic_receiver_or_unresolved_import_has_no_anchor() {
        let dynamic = r#"import express from "express";
const app = express();
getRouter().get("/users", (req, res) => { res.json([]); });
"#;
        // getRouter() is not a resolved binding, so no anchor is produced.
        assert!(targets("src/dynamic.ts", dynamic).is_empty());
        assert_eq!(
            unknown_kinds("src/dynamic.ts", dynamic),
            vec!["dynamic_route_call".to_string()]
        );

        let unresolved = r#"const app = makeApp();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/unresolved.ts", unresolved).is_empty());
        assert_eq!(
            unknown_kinds("src/unresolved.ts", unresolved),
            vec!["unresolved_express_receiver".to_string()]
        );

        let dynamic_method = r#"import express from "express";
const app = express();
const method = "get";
app[method]("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/dynamic-method.ts", dynamic_method).is_empty());
        assert_eq!(
            unknown_kinds("src/dynamic-method.ts", dynamic_method),
            vec!["dynamic_route_call".to_string()]
        );
    }

    #[test]
    fn jest_vitest_imported_runners_anchor_suites_and_tests() {
        let text = r#"import { describe, it, test } from "vitest";
describe("users", () => {
  it("loads", () => {});
  test("filters", () => {});
});
"#;
        assert_eq!(
            targets("src/users.test.ts", text),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string(),
                "jest_vitest.test".to_string(),
            ]
        );

        let jest = r#"import { describe, it } from "@jest/globals";
describe("accounts", () => {
  it("works", () => {});
});
"#;
        assert_eq!(
            targets("src/accounts.spec.ts", jest),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string()
            ]
        );
    }

    #[test]
    fn next_exact_file_conventions_anchor_with_package_context() {
        let page = r#"export default function Page() {
  return <main>Users</main>;
}
"#;
        assert_eq!(
            targets_with_packages("app/users/page.tsx", page, &["next"]),
            vec!["next.app.page".to_string()]
        );

        let route = r#"export async function GET(request: Request) {
  return Response.json({ ok: true });
}
"#;
        let facts = parse_facts_with_packages("app/users/route.ts", route, &["next"]);
        assert_eq!(
            targets_from_facts(facts.clone()),
            vec!["next.route.GET".to_string()]
        );
        let route_fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "next.route.GET")
            })
            .expect("next route fact");
        assert!(route_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "http_method=GET"));

        assert!(targets("app/users/page.tsx", page).is_empty());
        assert_eq!(
            unknown_kinds("app/users/page.tsx", page),
            vec!["next_missing_package_context".to_string()]
        );
    }

    #[test]
    fn next_dynamic_segments_are_unknown_not_support() {
        let page = r#"export default function Page() {
  return <main>User</main>;
}
"#;
        assert!(targets_with_packages("app/users/[id]/page.tsx", page, &["next"]).is_empty());
        assert_eq!(
            unknown_kinds_from_facts(parse_facts_with_packages(
                "app/users/[id]/page.tsx",
                page,
                &["next"],
            )),
            vec!["next_dynamic_segment_unknown".to_string()]
        );
    }

    #[test]
    fn fastify_exact_routes_anchor_shorthand_and_full_declarations() {
        let text = r#"import fastify from "fastify";
const app = fastify();
app.get("/users", async (request, reply) => { return reply.send([]); });
app.route({ method: "POST", url: "/users", handler: async (request, reply) => reply.send({}) });
"#;
        assert_eq!(
            targets("src/server.ts", text),
            vec![
                "fastify.route.full_declaration".to_string(),
                "fastify.route.get".to_string()
            ]
        );
    }

    #[test]
    fn prisma_exact_client_operations_and_transactions_anchor() {
        let text = r#"import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
export async function listUsers() {
  return prisma.user.findMany({ where: { active: true }, select: { id: true } });
}
export async function saveMany() {
  return prisma.$transaction([prisma.user.create({ data: { name: "Ada" } })]);
}
"#;
        assert_eq!(
            targets("src/repository.ts", text),
            vec![
                "prisma.query.findMany".to_string(),
                "prisma.transaction".to_string()
            ]
        );

        let raw = r#"import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
prisma.user.findMany({ where: sql`unsafe` });
"#;
        assert!(targets("src/raw.ts", raw).is_empty());
        assert_eq!(
            unknown_kinds("src/raw.ts", raw),
            vec!["prisma_raw_query".to_string()]
        );
    }

    #[test]
    fn drizzle_exact_schema_queries_and_transactions_anchor() {
        let text = r#"import { drizzle } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";
export const users = pgTable("users", {});
const db = drizzle(pool);
export async function listUsers() {
  return db.select().from(users).where(eq(users.id, 1));
}
export async function inTx() {
  return db.transaction(async (tx) => tx.select().from(users));
}
"#;
        assert_eq!(
            targets("src/drizzle.ts", text),
            vec![
                "drizzle.query.select".to_string(),
                "drizzle.schema.table".to_string(),
                "drizzle.transaction".to_string()
            ]
        );

        let raw = r#"import { drizzle, sql } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";
export const users = pgTable("users", {});
const db = drizzle(pool);
db.select({ unsafe: sql`raw` }).from(users);
"#;
        assert!(!targets("src/drizzle_raw.ts", raw)
            .iter()
            .any(|target| target == "drizzle.query.select"));
        assert_eq!(
            unknown_kinds("src/drizzle_raw.ts", raw),
            vec!["drizzle_raw_sql".to_string()]
        );
    }

    #[test]
    fn jest_vitest_imported_runner_aliases_anchor_suites_and_tests() {
        let text = r#"import { describe as suite, test as case_ } from "vitest";
suite("orders", () => {
  case_("creates", async () => {});
});
"#;
        assert_eq!(
            targets("src/orders.test.ts", text),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.test".to_string(),
            ]
        );
        let facts = parse_facts("src/orders.test.ts", text);
        let suite = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "jest_vitest.describe")
            })
            .expect("suite alias fact");
        assert!(suite
            .assumptions
            .iter()
            .any(|assumption| assumption == "runner_kind=vitest"));
        assert!(suite
            .assumptions
            .iter()
            .any(|assumption| assumption == "import_context=suite"));
        let case_fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "jest_vitest.test")
            })
            .expect("test alias fact");
        assert!(case_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "async_shape=async"));
    }

    #[test]
    fn jest_vitest_ambient_globals_anchor_only_in_test_files() {
        let ambient = r#"describe("users", () => {
  it("loads", () => {});
});
"#;
        assert_eq!(
            targets_with_test_context("src/users.test.ts", ambient),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string()
            ]
        );
        assert!(targets("src/users.test.ts", ambient).is_empty());
        assert_eq!(
            unknown_kinds("src/users.test.ts", ambient),
            vec![
                "ambient_runner_without_project_context".to_string(),
                "ambient_runner_without_project_context".to_string()
            ]
        );

        // Same source in a non-test file is ambiguous and yields no anchor.
        assert!(targets("src/users.ts", ambient).is_empty());
    }

    #[test]
    fn jest_vitest_custom_wrapper_or_foreign_import_has_no_anchor() {
        let wrapper = r#"const it = makeWrapper();
describe("users", () => {
  it("loads", () => {});
});
"#;
        // `it` is locally declared (a custom wrapper), so the test case has no anchor;
        // `describe` is ambient in this test file and still anchors with project context.
        assert_eq!(
            targets_with_test_context("src/users.test.ts", wrapper),
            vec!["jest_vitest.describe".to_string()]
        );
        assert_eq!(
            unknown_kinds_with_test_context("src/users.test.ts", wrapper),
            vec!["unsafe_test_runner_binding".to_string()]
        );

        let foreign = r#"import { it } from "./helpers";
it("loads", () => {});
"#;
        assert!(targets("src/users.test.ts", foreign).is_empty());
        assert_eq!(
            unknown_kinds("src/users.test.ts", foreign),
            vec!["unresolved_test_runner".to_string()]
        );
    }
}
