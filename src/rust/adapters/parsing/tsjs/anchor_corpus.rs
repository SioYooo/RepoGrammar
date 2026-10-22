//! The branch corpus of the slice-2 differential: each source with the
//! misreading class (or `none`) that explains how the text and the tree
//! reading are expected to differ, and the decision classes expected.

/// (class, path, source, expected decision classes).
pub(super) type Entry = (&'static str, &'static str, String, &'static [&'static str]);

const EXPRESS_HEAD: &str = "import express, { Router } from \"express\";\n";

fn prisma(body: &str) -> String {
    format!("import {{ PrismaClient }} from \"@prisma/client\";\nconst prisma = new PrismaClient();\n{body}")
}

fn drizzle(body: &str) -> String {
    format!("import {{ drizzle }} from \"drizzle-orm/node-postgres\";\nimport {{ pgTable }} from \"drizzle-orm/pg-core\";\nexport const users = pgTable(\"users\", {{}});\nconst db = drizzle(pool);\n{body}")
}

fn zod(body: &str) -> String {
    format!("import {{ z }} from \"zod\";\n{body}")
}

fn express(body: &str) -> String {
    format!("{EXPRESS_HEAD}const app = express();\n{body}")
}

pub(super) fn branch_corpus() -> Vec<Entry> {
    let fastify = |body: &str| {
        format!("import Fastify from \"fastify\";\nconst app = Fastify();\nconst plugin = async () => {{}};\nfunction handler() {{}}\n{body}")
    };
    let hono = |body: &str| {
        format!(
            "import {{ Hono }} from \"hono\";\nconst app = new Hono();\nfunction h() {{}}\n{body}"
        )
    };
    vec![
        // ---- equivalence
        ("none", "app.ts", express("function h() {}\napp.get(\"/a\", h);\napp.post(\"/b\", (req, res) => res.send(\"b\"));\n"), &[]),
        ("none", "router.ts", format!("{EXPRESS_HEAD}const router = Router();\nconst app = express();\nrouter.get(\"/r\", (req, res) => res.send(\"r\"));\napp.use(\"/api\", router);\n"), &[]),
        ("none", "fastify.ts", fastify("app.get(\"/f\", async () => ({ ok: true }));\napp.get(\"/g\", handler);\napp.register(plugin, { prefix: \"/p\" });\napp.register(plugin);\napp.route({ method: \"GET\", url: \"/r\", handler: async () => 1 });\n"), &[]),
        ("none", "hono.ts", hono("app.get(\"/h\", (c) => c.text(\"h\"));\napp.post(\"/p\", h);\n"), &[]),
        ("none", "chain.ts", express("app.use(\"/x\", (req, res, next) => next()).get(\"/y\", (req, res) => res.send(\"y\"));\n"), &[]),
        ("none", "optional_chain.ts", express("app?.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &[]),
        ("none", "dynamic_prefix.ts", fastify("app.register(plugin, { prefix: base });\napp.register(plugin, opts);\napp.register(plugin, { prefix: \"/a\" }, extra);\n"), &[]),
        // ---- type_arguments
        // The extractor makes no unit of `app.get<P>(`: no decision to compare (slice 3).
        ("type_arguments", "typed_route.ts", express("function h() {}\napp.get<Params>(\"/a\", h);\n"), &[]),
        ("type_arguments", "typed_hono.ts", "import { Hono } from \"hono\";\nconst app = new Hono<{ Bindings: { A: Map<string, B> } }>();\napp.get(\"/h\", (c) => c.text(\"h\"));\n".to_string(), &[]),
        // ---- comment_in_call
        ("comment_in_call", "comment_arg.ts", express("function h() {}\napp.get(\"/a\", /* c */ h);\n"), &["anchor_changed"]),
        // ---- path_from_non_first_argument
        ("path_from_non_first_argument", "path_later.ts", express("const p = \"/x\";\napp.get(p, (req, res) => res.send(\"ok\"));\n"), &["anchor_changed"]),
        // ---- trailing_comma
        ("trailing_comma", "trailing_comma.ts", express("function h() {}\napp.get(\"/a\", h,);\n"), &["anchor_changed"]),
        // ---- string_prefix_expression
        ("string_prefix_expression", "string_prefix.ts", express("function h() {}\nconst x = \"1\";\napp.get(\"/a\" + x, h);\n"), &["anchor_changed"]),
        ("string_prefix_expression", "string_prefix_hono.ts", hono("const x = \"1\";\napp.get(\"/a\" + x, h);\n"), &["unknown_added"]),
        // ---- escaped_quote
        ("escaped_quote", "escaped_quote.ts", express("function h() {}\napp.get('/it\\'s', h);\n"), &["anchor_changed"]),
        // ---- member_receiver
        ("member_receiver", "member_receiver.ts", express("class S {\n  app = other();\n  start() {\n    this.app.get(\"/a\", (req, res) => res.send(\"a\"));\n  }\n}\n"), &[]),
        // ---- unit_not_code
        ("unit_not_code", "comment_unit.ts", express("// app.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &["unknown_added"]),
        ("unit_not_code", "template_unit.ts", express("const doc = `\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n`;\n"), &["unknown_added"]),
        // ---- keyword_receiver (a preregistration deviation, found on the external corpus)
        ("keyword_receiver", "this_receiver.ts", express("class C {\n  m() {\n    this.get(\"/a\");\n  }\n}\n"), &["unknown_kind_changed"]),
        // ---- declarator_type_annotation
        ("declarator_type_annotation", "typed_app.ts", format!("{EXPRESS_HEAD}const app: Express = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &["unknown_removed"]),
        ("declarator_type_annotation", "typed_fastify.ts", "import Fastify from \"fastify\";\nconst app: FastifyInstance = Fastify();\napp.get(\"/f\", async () => 1);\n".to_string(), &["unknown_removed"]),
        // ---- object_field_lookalike and quoted_in_expression
        ("object_field_lookalike", "nested_method.ts", fastify("app.route({ url: \"/r\", handler: async () => ({ method: \"POST\" }) });\n"), &["unknown_added"]),
        ("object_field_lookalike", "url_expression.ts", fastify("app.route({ method: \"GET\", url: `/r`, handler: async () => \"ok\" });\n"), &["unknown_added"]),
        // ---- shorthand_or_method_field
        ("shorthand_or_method_field", "shorthand_handler.ts", fastify("app.route({ method: \"GET\", url: \"/r\", handler });\n"), &["unknown_removed"]),
        ("shorthand_or_method_field", "method_handler.ts", fastify("app.route({ method: \"GET\", url: \"/r\", handler(request) { return 1; } });\n"), &["unknown_removed"]),
        // ---- prefix_identifier_lookalike
        ("prefix_identifier_lookalike", "prefix_value.ts", fastify("app.register(plugin, { name: prefix });\n"), &["unknown_removed"]),
        // ---- Prisma and Drizzle: equivalence
        ("none", "prisma.ts", prisma("export async function list() {\n  return prisma.user.findMany({ where: { active: true } });\n}\nexport async function tx() {\n  return prisma.$transaction([prisma.user.findMany()]);\n}\n"), &[]),
        ("none", "prisma_callback.ts", prisma("export async function tx() {\n  return prisma.$transaction(async (t) => t.user.findMany());\n}\nexport async function raw() {\n  return prisma.$queryRaw`select 1`;\n}\nexport async function dyn(m: string) {\n  return prisma[m].findMany();\n}\n"), &[]),
        ("none", "drizzle.ts", drizzle("export async function list() {\n  return db.select().from(users).where(eq(users.id, 1));\n}\nexport async function add() {\n  return db.insert(users).values({ name: \"a\" });\n}\nexport async function edit() {\n  return db.update(users).set({ name: \"b\" });\n}\nexport async function drop() {\n  return db.delete(users);\n}\nexport async function tx() {\n  return db.transaction(async (t) => t.select().from(users));\n}\n"), &[]),
        ("none", "drizzle_dynamic.ts", drizzle("export async function f(t) {\n  return db.select().from(t);\n}\nexport async function g() {\n  return db.execute(sql`select 1`);\n}\n"), &[]),
        // ---- type_arguments and declarator_type_annotation on receivers
        ("type_arguments", "prisma_generic.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient<Options>();\nexport async function list() {\n  return prisma.user.findMany();\n}\n".to_string(), &["unknown_removed"]),
        ("declarator_type_annotation", "prisma_typed.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma: PrismaClient = new PrismaClient();\nexport async function list() {\n  return prisma.user.findMany();\n}\n".to_string(), &["unknown_removed"]),
        ("declarator_type_annotation", "drizzle_typed.ts", "import { drizzle } from \"drizzle-orm/node-postgres\";\nimport { pgTable } from \"drizzle-orm/pg-core\";\nexport const users = pgTable(\"users\", {});\nconst db: Db = drizzle(pool);\nexport async function list() {\n  return db.select().from(users);\n}\n".to_string(), &["unknown_removed"]),
        // The extractor also starts a Prisma-shaped unit mid-chain at `query.users.findMany(`.
        ("member_receiver", "drizzle_query_member.ts", drizzle("export async function rel() {\n  return db.query.users.findMany();\n}\n"), &["unknown_kind_changed"]),
        // ---- comment_in_call on a member chain (the extractor makes no unit of it)
        ("comment_in_call", "prisma_comment_chain.ts", prisma("export async function list() {\n  return prisma.user /* c */ .findMany();\n}\n"), &[]),
        // ---- chain_scope and table_expression_prefix
        ("chain_scope", "drizzle_from_elsewhere.ts", drizzle("export async function list(xs) {\n  return db.select({ n: Array.from(xs) }).from(users);\n}\n"), &["unknown_removed"]),
        ("table_expression_prefix", "drizzle_member_table.ts", drizzle("export async function add() {\n  return db.insert(schema.users).values({ name: \"a\" });\n}\n"), &["unknown_kind_changed"]),
        // ---- transaction argument shape
        ("string_prefix_expression", "prisma_tx_spaced.ts", prisma("export async function tx() {\n  return prisma.$transaction( [prisma.user.findMany()] );\n}\n"), &["unknown_removed"]),
        // ---- unit_not_code
        ("unit_not_code", "prisma_comment_unit.ts", prisma("// prisma.user.findMany();\n"), &["unknown_added"]),
        ("unit_not_code", "drizzle_comment_unit.ts", drizzle("// db.select().from(users);\n"), &["unknown_added"]),
        ("keyword_receiver", "this_prisma.ts", prisma("class C {\n  m() {\n    this.items.delete(key);\n  }\n}\n"), &["unknown_kind_changed"]),
        ("unit_not_code", "comment_transaction_unit.ts", drizzle("/**\n * db.transaction(() => run());\n */\nexport function f() {}\n"), &["unknown_added"]),
        // ---- Zod and Jest/Vitest
        ("none", "zod.ts", zod("export const A = z.object({ name: z.string() });\nexport const B = z.union([z.string(), z.number()]).optional();\nexport const C = z.enum([\"a\", \"b\"]);\nexport const D = z.string().min(2);\nexport const E = z.object({}).strict().refine((v) => v);\nexport const F = z.object({}).shape.a;\n"), &[]),
        ("none", "vitest.test.ts", "import { describe, it, expect } from \"vitest\";\ndescribe(\"s\", () => {\n  it(\"t\", () => {\n    expect(1).toBe(1);\n  });\n});\n".to_string(), &[]),
        ("none", "alias.test.ts", "import { test as t, describe as d } from \"vitest\";\nd(\"s\", () => {\n  t(\"t\", () => {});\n});\n".to_string(), &[]),
        ("declarator_type_annotation", "zod_typed.ts", zod("export const A: ZodType = z.object({ name: z.string() });\n"), &["anchor_changed"]),
        ("unit_not_code", "zod_comment_unit.ts", zod("// const Ghost = z.object({});\n"), &[]),
        ("unit_not_code", "vitest_comment_unit.test.ts", "import { describe } from \"vitest\";\n// describe(\"ghost\", () => {});\n".to_string(), &["unknown_added"]),
        ("unit_not_code", "method_describe.ts", "class A {\n  describe(d: string): this {\n    return this;\n  }\n}\n".to_string(), &["unknown_kind_changed"]),
        ("table_expression_prefix", "drizzle_call_table.ts", drizzle("export async function f() {\n  return db.delete(table(\"users\"));\n}\n"), &["unknown_kind_changed"]),
        ("member_receiver", "member_chain_prisma.ts", prisma("export async function f(drill) {\n  drill.client.customers.delete(1);\n}\n"), &["unknown_kind_changed"]),
        // ---- syntax_error
        ("syntax_error", "error_route.ts", express("const broken = ;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &[]),
    ]
}
