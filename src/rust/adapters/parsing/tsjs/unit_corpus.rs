//! The branch corpus of the slice-3 differential: each source with the
//! misreading class (or `none`) that explains how the line reader and the tree
//! reader are expected to differ, and the unit and decision classes expected
//! (`unit_boundary_only` is not listed: it is the expected end difference).

/// (misreading class, path, source, expected classes).
pub(super) type Entry = (&'static str, &'static str, String, &'static [&'static str]);

const EXPRESS_HEAD: &str = "import express from \"express\";\nconst app = express();\n";
const PRISMA_HEAD: &str =
    "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\n";
const DRIZZLE_HEAD: &str = "import { drizzle } from \"drizzle-orm/node-postgres\";\nimport { pgTable } from \"drizzle-orm/pg-core\";\nexport const users = pgTable(\"users\", {});\nconst db = drizzle(pool);\n";
const VITEST_HEAD: &str = "import { describe, it } from \"vitest\";\n";
const NEST_HEAD: &str = "import { Controller, Get, Injectable, Module } from \"@nestjs/common\";\n";

fn with(head: &str, body: &str) -> String {
    format!("{head}{body}")
}

pub(super) fn branch_corpus() -> Vec<Entry> {
    vec![
        // ---- no difference beyond the end byte
        ("none", "routes.ts", with(EXPRESS_HEAD, "function h() {}\napp.get(\"/a\", h);\napp.post(\"/b\", (req, res) => res.send(\"b\"));\n"), &[]),
        // The line reader starts a chain unit at the whitespace-delimited token
        // (`mw)`), the tree at the chain start with the chain's first route call.
        ("receiver_token_start", "chain.ts", with(EXPRESS_HEAD, "app.use(\"/x\", mw).get(\"/y\", h);\n"), &["unit_added", "unit_retired"]),
        ("none", "nest.ts", with(NEST_HEAD, "@Controller(\"u\")\nexport class C {\n  @Get(\"a\")\n  find() {}\n  plain() {}\n}\n@Injectable()\nexport class S {}\n@Module({})\nexport class M {}\n"), &[]),
        ("none", "prisma.ts", with(PRISMA_HEAD, "export async function f() {\n  return prisma.user.findMany({ where: { a: 1 } });\n}\nexport async function t() {\n  return prisma.$transaction([prisma.user.findMany()]);\n}\n"), &[]),
        ("none", "vitest.test.ts", with(VITEST_HEAD, "describe(\"s\", () => {\n  it(\"t\", () => {});\n});\n"), &[]),
        ("none", "react.tsx", "export function Widget() {\n  return <div />;\n}\nexport const useThing = () => 1;\n".to_string(), &[]),
        // ---- unit_not_code
        ("unit_not_code", "comment_route.ts", with(EXPRESS_HEAD, "// app.get(\"/a\", h);\n"), &["unit_retired"]),
        ("unit_not_code", "template_route.ts", with(EXPRESS_HEAD, "const doc = `\napp.get(\"/a\", h);\n`;\n"), &["unit_retired"]),
        ("unit_not_code", "comment_test.test.ts", with(VITEST_HEAD, "// describe(\"ghost\", () => {});\n"), &["unit_retired"]),
        ("unit_not_code", "comment_function.ts", "// function ghost() {}\nexport function real() {}\n".to_string(), &["unit_retired"]),
        ("unit_not_code", "method_describe.ts", "class A {\n  describe(d: string) {\n    return d;\n  }\n}\n".to_string(), &["unit_retired"]),
        // ---- statement_taken_for_method
        ("statement_taken_for_method", "class_statements.ts", "class A {\n  run() {\n    this.go();\n    console.log(1);\n    const o = { inner() {} };\n  }\n}\n".to_string(), &["unit_retired"]),
        // ---- member_receiver
        ("member_receiver", "this_prisma.ts", with(PRISMA_HEAD, "class S {\n  prisma = other();\n  run() {\n    return this.prisma.user.findMany();\n  }\n}\n"), &["unit_start_corrected"]),
        ("member_receiver", "drizzle_query_chain.ts", with(DRIZZLE_HEAD, "export async function rel() {\n  return db.query.users.findMany();\n}\n"), &["unit_start_corrected"]),
        // ---- keyword_receiver
        ("keyword_receiver", "this_delete.ts", with(PRISMA_HEAD, "class C {\n  m() {\n    this.items.delete(key);\n  }\n}\n"), &["unit_retired"]),
        // ---- extent_by_first_brace
        ("extent_by_first_brace", "where_next_function.ts", with(DRIZZLE_HEAD, "export async function a() {\n  return db.select().from(users).where(eq(users.id, 1));\n}\nexport async function b() {\n  return db.query.users.findMany({ where: eq(users.id, 1) });\n}\n"), &["anchor_changed", "unit_start_corrected"]),
        ("extent_by_first_brace", "destructured_arrow.ts", "export const f = ({ a }) => a + 1;\nexport const g = () => 2;\n".to_string(), &[]),
        // ---- one_unit_per_line
        ("one_unit_per_line", "two_routes.ts", with(EXPRESS_HEAD, "app.get(\"/a\", h); app.post(\"/b\", h);\n"), &["unit_added"]),
        ("one_unit_per_line", "leading_dot_chain.ts", with(EXPRESS_HEAD, "app\n  .get(\"/a\", h)\n  .post(\"/b\", h);\n"), &["unit_added"]),
        ("one_unit_per_line", "two_functions.ts", "function a() {} function b() {}\n".to_string(), &["unit_added"]),
        // ---- declarator_shape
        ("declarator_shape", "typed_arrow.ts", "export const f = (x: number): number => x + 1;\nexport const g: (x: number) => void = (x) => {};\n".to_string(), &["unit_added"]),
        ("declarator_shape", "generic_arrow.ts", "export const id = <T,>(x: T) => x;\n".to_string(), &["unit_added"]),
        ("declarator_shape", "second_declarator.ts", "const a = () => 1, b = () => 2;\n".to_string(), &["unit_added"]),
        // ---- accessor_name
        ("accessor_name", "accessor.ts", "class A {\n  get size() {\n    return 1;\n  }\n}\n".to_string(), &["unit_added", "unit_retired"]),
        // ---- decorator_walk
        ("decorator_walk", "decorator_comment.ts", with(NEST_HEAD, "@Controller()\nexport class C {\n  @Get(\"a\") // note\n  find() {}\n}\n"), &["unit_added", "unit_retired"]),
        // ---- jsx_text_lookalike
        ("jsx_text_lookalike", "jsx_string.js", "export function Widget() {\n  return \"</div>\";\n}\n".to_string(), &["unit_kind_changed"]),
        // ---- string_prefix_expression (subscript call)
        ("string_prefix_expression", "subscript.ts", with(EXPRESS_HEAD, "console['warn']('a' + 'b');\nhandlers[k](\"x\");\n"), &["unit_retired"]),
        // ---- export_default_wrapper
        ("none", "pages/index.tsx", "export default memo(function Page() {\n  return <p />;\n});\n".to_string(), &[]),
        // ---- degraded_unit_set (D4)
        ("degraded_unit_set", "error_route.ts", with(EXPRESS_HEAD, "const broken = ;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &[]),
        ("degraded_unit_set", "error_unclosed.ts", with(EXPRESS_HEAD, "app.get(\"/a\", (req, res) => {\n  res.send(\"a\");\n"), &["unit_retired"]),
    ]
}
