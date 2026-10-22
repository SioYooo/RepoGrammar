//! The branch corpus of the slice-2 differential: each source with the
//! misreading class (or `none`) that explains how the text and the tree
//! reading are expected to differ, and the decision classes expected.

/// (class, path, source, expected decision classes).
pub(super) type Entry = (&'static str, &'static str, String, &'static [&'static str]);

const EXPRESS_HEAD: &str = "import express, { Router } from \"express\";\n";

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
        // ---- syntax_error
        ("syntax_error", "error_route.ts", express("const broken = ;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n"), &[]),
    ]
}
