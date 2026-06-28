//! Conservative TS/JS exact-anchor extraction.
//!
//! This pass runs after syntax-only code-unit extraction. It emits `STRUCTURAL`
//! semantic facts ONLY for code units whose framework usage can be resolved
//! through exact import/require bindings and literal call shapes. Anything that
//! is dynamic, reassigned, shadowed, conditionally imported, or merely a
//! lookalike yields no anchor, so the family layer keeps it `UNKNOWN`. These
//! structural anchors are later promoted to bounded `DATAFLOW_DERIVED` support
//! facts by the application layer; they never prove membership by themselves.

mod drizzle;
mod express;
mod fastify;
mod jest_vitest;
mod next;
mod prisma;
mod project_context;
mod scope_graph;
mod unknown;

use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId,
};
use crate::ports::parser::{ParseError, ParserProjectContext, SourceDocument};
use scope_graph::ScopeGraphLite;
use unknown::UnknownAnchor;

pub(crate) use jest_vitest::exact_test_runner_call_names;

/// Engine identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_ENGINE: &str = "repogrammar-tsjs-syntax";
/// Method identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_METHOD: &str = "exact_anchor_v1";

/// Extract exact framework anchors for the given units. Returns `STRUCTURAL`
/// facts whose evidence spans the full owning unit range.
pub fn exact_framework_anchors(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    context: Option<&ParserProjectContext>,
) -> Result<Vec<SemanticFact>, ParseError> {
    let bindings = ScopeGraphLite::analyze(document.text);
    let mut facts = Vec::new();
    for unit in units {
        match anchor_for_unit(document, context, &bindings, unit) {
            AnchorOutcome::Anchor(anchor) => facts.push(anchor_fact(document, unit, anchor)?),
            AnchorOutcome::Unknown(unknown) => facts.push(unknown::fact(document, unit, unknown)?),
            AnchorOutcome::None => {}
        }
    }
    Ok(facts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    target: String,
    fact_kind: SemanticFactKind,
    assumptions: Vec<String>,
}

enum AnchorOutcome {
    Anchor(Anchor),
    Unknown(UnknownAnchor),
    None,
}

fn anchor_for_unit(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
    bindings: &ScopeGraphLite,
    unit: &CodeUnit,
) -> AnchorOutcome {
    let Some(slice) = document
        .text
        .get(unit.range.start_byte..unit.range.end_byte)
    else {
        return AnchorOutcome::None;
    };
    match unit.kind {
        CodeUnitKind::ExpressRoute => express::anchor(bindings, slice, unit.range.start_byte),
        CodeUnitKind::NextAppPage
        | CodeUnitKind::NextAppLayout
        | CodeUnitKind::NextRouteHandler
        | CodeUnitKind::NextPagesApiRoute
        | CodeUnitKind::NextPagesPage => next::anchor(document, context, unit, slice),
        CodeUnitKind::FastifyRoute => fastify::anchor(bindings, slice, unit.range.start_byte),
        CodeUnitKind::PrismaQuery => prisma::query_anchor(bindings, slice, unit.range.start_byte),
        CodeUnitKind::PrismaTransaction => {
            prisma::transaction_anchor(bindings, slice, unit.range.start_byte)
        }
        CodeUnitKind::DrizzleSchemaTable => {
            drizzle::schema_table_anchor(bindings, slice, unit.range.start_byte)
        }
        CodeUnitKind::DrizzleQuery => drizzle::query_anchor(bindings, slice, unit.range.start_byte),
        CodeUnitKind::DrizzleTransaction => {
            drizzle::transaction_anchor(bindings, slice, unit.range.start_byte)
        }
        CodeUnitKind::TestSuite => jest_vitest::anchor(
            document,
            bindings,
            slice,
            unit.range.start_byte,
            true,
            context.is_some_and(|context| context.tsjs_has_test_runner_context),
        ),
        CodeUnitKind::TestCase => jest_vitest::anchor(
            document,
            bindings,
            slice,
            unit.range.start_byte,
            false,
            context.is_some_and(|context| context.tsjs_has_test_runner_context),
        ),
        _ => AnchorOutcome::None,
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

pub(super) fn object_literal_string_field(slice: &str, field: &str) -> Option<String> {
    let field_index = slice.find(field)?;
    let after_field = &slice[field_index + field.len()..];
    let after_colon = after_field.trim_start().strip_prefix(':')?.trim_start();
    first_quoted(after_colon)
}

pub(super) fn object_clause_shape(slice: &str, field: &str) -> &'static str {
    let pattern = format!("{field}:");
    if slice.contains(&pattern) {
        "object_literal"
    } else {
        "none"
    }
}

pub(super) fn raw_sql_present(slice: &str) -> bool {
    slice.contains("sql`")
        || slice.contains("sql.raw")
        || slice.contains("sql.fromList")
        || slice.contains("sql.join")
        || slice.contains("sql.append")
        || slice.contains("sql.empty")
        || slice.contains(".execute(")
        || slice.contains("$queryRaw")
        || slice.contains("$executeRaw")
        || slice.contains("queryRaw")
        || slice.contains("executeRaw")
}

pub(super) fn route_call_parts(slice: &str) -> Option<(&str, &str)> {
    let (receiver, after) = leading_identifier(slice)?;
    let rest = slice[after..].trim_start().strip_prefix('.')?;
    let (method, after_method) = leading_identifier(rest)?;
    if !rest[after_method..].trim_start().starts_with('(') {
        return None;
    }
    Some((receiver, method))
}

pub(super) fn route_path_shape(slice: &str) -> Option<String> {
    let open = slice.find('(')?;
    let path = first_quoted(&slice[open + 1..])?;
    Some(normalize_route_path(&path))
}

pub(super) fn first_quoted(text: &str) -> Option<String> {
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

pub(super) fn normalize_route_path(path: &str) -> String {
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

pub(super) fn handler_shape(slice: &str) -> &'static str {
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

pub(super) fn async_shape(slice: &str) -> &'static str {
    if slice.contains("async ") || slice.contains("async(") || slice.contains("async (") {
        "async"
    } else {
        "sync"
    }
}

pub(super) fn leading_identifier(text: &str) -> Option<(&str, usize)> {
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

pub(super) fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

pub(super) fn is_identifier_byte(byte: u8) -> bool {
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

        let const_route = r#"export const POST = async (request: Request) => {
  return Response.json({ ok: true });
}
"#;
        let const_route_facts =
            parse_facts_with_packages("src/app/users/route.ts", const_route, &["next"]);
        assert_eq!(
            targets_from_facts(const_route_facts.clone()),
            vec!["next.route.POST".to_string()]
        );
        let const_route_fact = const_route_facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "next.route.POST")
            })
            .expect("next const route fact");
        assert!(const_route_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "async_shape=async"));

        assert!(targets("app/users/page.tsx", page).is_empty());
        assert_eq!(
            unknown_kinds("app/users/page.tsx", page),
            vec!["next_missing_package_context".to_string()]
        );
    }

    #[test]
    fn next_dynamic_segments_are_context_not_membership_blockers() {
        let page = r#"export default function Page() {
  return <main>User</main>;
}
"#;
        let facts = parse_facts_with_packages("app/users/[id]/page.tsx", page, &["next"]);
        assert_eq!(
            targets_from_facts(facts.clone()),
            vec!["next.app.page".to_string()]
        );
        let fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "next.app.page")
            })
            .expect("dynamic route page fact");
        assert!(fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "dynamic_segment_present=true"));
        assert!(unknown_kinds_from_facts(facts).is_empty());
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
                "fastify.route.get".to_string(),
                "fastify.route.route".to_string()
            ]
        );
    }

    #[test]
    fn top_level_app_support_survives_unrelated_nested_app_parameter() {
        let text = r#"import express from "express";
const app = express();
app.get("/top", (req, res) => res.json([]));

export function register(app) {
  app.post("/nested", (req, res) => res.json({}));
}
"#;
        assert_eq!(
            targets("src/top_level_app.ts", text),
            vec!["express.route.get".to_string()]
        );
        assert_eq!(
            unknown_kinds("src/top_level_app.ts", text),
            vec!["unsafe_receiver_binding".to_string()]
        );
    }

    #[test]
    fn nested_local_shadowing_blocks_exact_tsjs_anchors() {
        let express = r#"import express from "express";
const app = express();
export function register() {
  const app = makeFake();
  app.get("/users", (req, res) => res.json([]));
}
"#;
        assert!(targets("src/express_shadow.ts", express).is_empty());
        assert_eq!(
            unknown_kinds("src/express_shadow.ts", express),
            vec!["unsafe_receiver_binding".to_string()]
        );

        let fastify = r#"import fastify from "fastify";
const app = fastify();
export function register() {
  const app = makeFake();
  app.get("/users", async () => []);
}
"#;
        assert!(targets("src/fastify_shadow.ts", fastify).is_empty());
        assert_eq!(
            unknown_kinds("src/fastify_shadow.ts", fastify),
            vec!["fastify_receiver_reassigned".to_string()]
        );

        let prisma = r#"import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
export async function listUsers() {
  const prisma = getInjectedClient();
  return prisma.user.findMany();
}
"#;
        assert!(targets("src/prisma_shadow.ts", prisma).is_empty());
        assert_eq!(
            unknown_kinds("src/prisma_shadow.ts", prisma),
            vec!["prisma_injected_client".to_string()]
        );

        let drizzle = r#"import { drizzle } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";
export const users = pgTable("users", {});
const db = drizzle(pool);
export async function listUsers() {
  const db = getInjectedDb();
  return db.select().from(users);
}
"#;
        assert!(!targets("src/drizzle_shadow.ts", drizzle)
            .iter()
            .any(|target| target == "drizzle.query.select"));
        assert_eq!(
            unknown_kinds("src/drizzle_shadow.ts", drizzle),
            vec!["drizzle_db_binding_unresolved".to_string()]
        );

        let jest = r#"import { describe, it } from "vitest";
describe("users", () => {
  const it = makeWrapper();
  it("loads", () => {});
});
"#;
        assert_eq!(
            targets("src/jest_shadow.test.ts", jest),
            vec!["jest_vitest.describe".to_string()]
        );
        assert_eq!(
            unknown_kinds("src/jest_shadow.test.ts", jest),
            vec!["unsafe_test_runner_binding".to_string()]
        );
    }

    #[test]
    fn imported_fastify_prisma_and_drizzle_without_local_providers_remain_unknown() {
        let fastify = r#"import fastify from "fastify";
export function register(app) {
  app.get("/users", async () => []);
}
"#;
        assert!(targets("src/fastify_imported_only.ts", fastify).is_empty());
        assert_eq!(
            unknown_kinds("src/fastify_imported_only.ts", fastify),
            vec!["unresolved_express_receiver".to_string()]
        );

        let prisma = r#"import { PrismaClient } from "@prisma/client";
export async function listUsers(prisma: PrismaClient) {
  return prisma.user.findMany();
}
"#;
        assert!(targets("src/prisma_imported_only.ts", prisma).is_empty());
        assert_eq!(
            unknown_kinds("src/prisma_imported_only.ts", prisma),
            vec!["prisma_injected_client".to_string()]
        );

        let drizzle = r#"import { drizzle } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";
export const users = pgTable("users", {});
export async function listUsers(db) {
  return db.select().from(users);
}
"#;
        assert!(!targets("src/drizzle_imported_only.ts", drizzle)
            .iter()
            .any(|target| target == "drizzle.query.select"));
        assert_eq!(
            unknown_kinds("src/drizzle_imported_only.ts", drizzle),
            vec!["drizzle_db_binding_unresolved".to_string()]
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
                "prisma.query.create".to_string(),
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

        let raw_transaction = r#"import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
prisma.$transaction([prisma.$executeRaw("DELETE FROM users")]);
"#;
        assert!(targets("src/raw-transaction.ts", raw_transaction).is_empty());
        assert!(unknown_kinds("src/raw-transaction.ts", raw_transaction)
            .iter()
            .all(|kind| kind == "prisma_raw_query"));

        let bulk = r#"import { PrismaClient } from "@prisma/client";
const prisma = new PrismaClient();
prisma.user.createMany({ data: [] });
"#;
        assert!(targets("src/bulk.ts", bulk).is_empty());
        assert_eq!(
            unknown_kinds("src/bulk.ts", bulk),
            vec!["prisma_dynamic_model_or_operation".to_string()]
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
export async function queryUsers() {
  return db.query.users.findMany({ where: eq(users.id, 1) });
}
"#;
        assert_eq!(
            targets("src/drizzle.ts", text),
            vec![
                "drizzle.query.query_findMany".to_string(),
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
