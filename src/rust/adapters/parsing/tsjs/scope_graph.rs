use super::parts::{self, Item};
use crate::core::model::Language;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser, Tree};

/// Upper bound on name-to-scope insertions while reading one file. Nesting is
/// untrusted, and every declaration is recorded in each enclosing scope, so
/// the work is depth times declarations; past this bound the file is refused
/// like a syntax error.
// ponytail: refuses pathological nesting; interval-indexed declaration sites
// would remove the bound if real code ever reaches it.
const MAX_SCOPE_WORK: usize = 4_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImportKind {
    Default,
    Namespace,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ImportBinding {
    pub(super) module: String,
    pub(super) kind: ImportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExpressReceiver {
    App,
    Router,
}

#[derive(Debug, Default)]
pub(super) struct ScopeGraphLite {
    pub(super) imports: BTreeMap<String, ImportBinding>,
    pub(super) local_decls: BTreeSet<String>,
    pub(super) unsafe_names: BTreeSet<String>,
    unsafe_events: BTreeMap<String, Vec<UnsafeNameEvent>>,
    scope_intervals: Vec<ScopeInterval>,
    pub(super) express_receivers: BTreeMap<String, ExpressReceiver>,
    pub(super) express_router_prefixes: BTreeMap<String, String>,
    pub(super) fastify_receivers: BTreeSet<String>,
    pub(super) hono_receivers: BTreeSet<String>,
    pub(super) prisma_clients: BTreeSet<String>,
    pub(super) drizzle_table_factories: BTreeSet<String>,
    pub(super) drizzle_tables: BTreeSet<String>,
    pub(super) drizzle_dbs: BTreeSet<String>,
    /// The tree had a syntax error, or the file exceeded the scope-work
    /// bound; no binding was read (ADR-0059 D4).
    pub(super) degraded: bool,
    /// The tree the graph was read from, shared with the anchor readers.
    pub(super) tree: Option<Tree>,
}

#[derive(Debug, Clone, Copy)]
struct UnsafeNameEvent {
    byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScopeInterval {
    start_byte: usize,
    end_byte: usize,
    local_decls: BTreeSet<String>,
    params: BTreeSet<String>,
}

impl ScopeGraphLite {
    pub(super) fn analyze(text: &str, path: &str, language: &Language) -> Self {
        let mut parser = Parser::new();
        let tree = parser
            .set_language(&grammar(path, language))
            .ok()
            .and_then(|()| parser.parse(text, None));
        match tree {
            Some(tree) if !tree.root_node().has_error() => {
                match Self::from_tree(text, tree.root_node()) {
                    Some(mut graph) => {
                        graph.tree = Some(tree);
                        graph
                    }
                    None => Self::degraded(),
                }
            }
            _ => Self::degraded(),
        }
    }

    pub(super) fn degraded() -> Self {
        Self {
            degraded: true,
            ..Self::default()
        }
    }

    fn from_tree(text: &str, root: Node<'_>) -> Option<Self> {
        let scope_intervals = scope_intervals(text, root)?;
        let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut unsafe_events: BTreeMap<String, Vec<UnsafeNameEvent>> = BTreeMap::new();
        let mut imports: BTreeMap<String, ImportBinding> = BTreeMap::new();
        let mut local_decls: BTreeSet<String> = BTreeSet::new();
        let mut items: Vec<Item<'_>> = Vec::new();

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            let statement = match child.kind() {
                "export_statement" => match child.child_by_field_name("declaration") {
                    Some(declaration) => declaration,
                    None => continue,
                },
                _ => child,
            };
            match statement.kind() {
                "import_statement" => {
                    for (local, binding) in import_bindings(text, statement) {
                        *declared_counts.entry(local.clone()).or_insert(0) += 1;
                        imports.insert(local, binding);
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    let mut declarators = statement.walk();
                    for declarator in statement.named_children(&mut declarators) {
                        if declarator.kind() != "variable_declarator" {
                            continue;
                        }
                        items.push(Item::Declarator(declarator));
                        let Some(name) = declarator.child_by_field_name("name") else {
                            continue;
                        };
                        let (required, declared) = require_declarator(text, declarator, name);
                        for (local, binding) in required {
                            *declared_counts.entry(local.clone()).or_insert(0) += 1;
                            imports.insert(local, binding);
                        }
                        for declared_name in declared {
                            *declared_counts.entry(declared_name.clone()).or_insert(0) += 1;
                            local_decls.insert(declared_name);
                        }
                    }
                }
                "function_declaration"
                | "generator_function_declaration"
                | "class_declaration"
                | "abstract_class_declaration" => {
                    if let Some(name) = statement.child_by_field_name("name") {
                        let name = node_text(text, name).to_string();
                        *declared_counts.entry(name.clone()).or_insert(0) += 1;
                        local_decls.insert(name);
                    }
                }
                "expression_statement" => {
                    if let Some(name) = bare_assignment_target(text, statement) {
                        push_unsafe_event(&mut unsafe_events, name, statement.start_byte());
                    }
                    items.push(Item::Statement(statement));
                }
                _ => {}
            }
        }

        let unsafe_names = declared_counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name)
            .collect();
        let mut graph = Self {
            imports,
            local_decls,
            unsafe_names,
            unsafe_events,
            scope_intervals,
            ..Self::default()
        };
        graph.fill_receivers(text, &items);
        Some(graph)
    }

    /// Classifies top-level declarators and statements; `parts::receivers` is
    /// the single receiver authority.
    fn fill_receivers(&mut self, text: &str, items: &[Item<'_>]) {
        for (local, binding) in &self.imports {
            if binding.module.starts_with("drizzle-orm")
                && matches!(&binding.kind, ImportKind::Named(original) if super::drizzle::DRIZZLE_TABLE_FACTORIES.contains(&original.as_str()))
                && !self.unsafe_names.contains(local)
            {
                self.drizzle_table_factories.insert(local.clone());
            }
        }
        let found = parts::receivers(
            text,
            items,
            &self.imports,
            &self.unsafe_names,
            &self.drizzle_table_factories,
        );
        self.express_receivers = found.express;
        self.express_router_prefixes = found.express_prefixes;
        self.fastify_receivers = found.fastify;
        self.hono_receivers = found.hono;
        self.prisma_clients = found.prisma;
        self.drizzle_tables = found.drizzle_tables;
        self.drizzle_dbs = found.drizzle_dbs;
    }

    pub(super) fn imported_runner(&self, name: &str) -> Option<(&str, &str)> {
        match self.imports.get(name) {
            Some(binding)
                if super::jest_vitest::RUNNER_MODULES.contains(&binding.module.as_str()) =>
            {
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

    pub(super) fn repo_local_named_import(&self, name: &str) -> Option<(&str, &str)> {
        let binding = self.imports.get(name)?;
        if !binding.module.starts_with("./") && !binding.module.starts_with("../") {
            return None;
        }
        match &binding.kind {
            ImportKind::Named(original) => Some((binding.module.as_str(), original.as_str())),
            ImportKind::Default | ImportKind::Namespace => None,
        }
    }

    pub(super) fn name_is_unsafe_at(&self, name: &str, byte: usize) -> bool {
        if self.unsafe_names.contains(name) {
            return true;
        }
        if self.scope_intervals.iter().any(|scope| {
            scope.contains(byte)
                && (scope.local_decls.contains(name) || scope.params.contains(name))
        }) {
            return true;
        }
        self.unsafe_events
            .get(name)
            .is_some_and(|events| events.iter().any(|event| event.byte <= byte))
    }
}

impl ScopeInterval {
    fn contains(&self, byte: usize) -> bool {
        self.start_byte <= byte && byte < self.end_byte
    }
}

fn push_unsafe_event(
    unsafe_events: &mut BTreeMap<String, Vec<UnsafeNameEvent>>,
    name: &str,
    byte: usize,
) {
    unsafe_events
        .entry(name.to_string())
        .or_default()
        .push(UnsafeNameEvent { byte });
}

/// Grammar by extension (ADR-0059 D1); an unknown extension falls back to the
/// document language.
fn grammar(path: &str, language: &Language) -> tree_sitter::Language {
    if path.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else if path.ends_with(".js") || path.ends_with(".jsx") {
        tree_sitter_javascript::LANGUAGE.into()
    } else if path.ends_with(".ts") || *language != Language::JavaScript {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    }
}

pub(super) fn node_text<'a>(text: &'a str, node: Node<'_>) -> &'a str {
    text.get(node.byte_range()).unwrap_or("")
}

fn string_value(text: &str, node: Node<'_>) -> Option<String> {
    let literal = node_text(text, node);
    (node.kind() == "string" && literal.len() >= 2)
        .then(|| literal.get(1..literal.len() - 1).map(str::to_string))
        .flatten()
}

fn import_bindings(text: &str, statement: Node<'_>) -> Vec<(String, ImportBinding)> {
    let Some(module) = statement
        .child_by_field_name("source")
        .and_then(|source| string_value(text, source))
    else {
        return Vec::new();
    };
    let binding = |kind| ImportBinding {
        module: module.clone(),
        kind,
    };
    let mut bindings = Vec::new();
    let mut cursor = statement.walk();
    for clause in statement
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "import_clause")
    {
        let mut parts = clause.walk();
        for part in clause.named_children(&mut parts) {
            match part.kind() {
                "identifier" => bindings.push((
                    node_text(text, part).to_string(),
                    binding(ImportKind::Default),
                )),
                "namespace_import" => {
                    if let Some(name) = part.named_child(0) {
                        bindings.push((
                            node_text(text, name).to_string(),
                            binding(ImportKind::Namespace),
                        ));
                    }
                }
                "named_imports" => {
                    let mut specifiers = part.walk();
                    for specifier in part.named_children(&mut specifiers) {
                        let Some(original) = specifier
                            .child_by_field_name("name")
                            .filter(|name| name.kind() == "identifier")
                        else {
                            continue;
                        };
                        let local = specifier.child_by_field_name("alias").unwrap_or(original);
                        bindings.push((
                            node_text(text, local).to_string(),
                            binding(ImportKind::Named(node_text(text, original).to_string())),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    bindings
}

/// The module of a `require("module")` call, and nothing else.
fn require_module(text: &str, value: Node<'_>) -> Option<String> {
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = value.child_by_field_name("function")?;
    let arguments = value.child_by_field_name("arguments")?;
    if node_text(text, callee) != "require" || arguments.named_child_count() != 1 {
        return None;
    }
    string_value(text, arguments.named_child(0)?)
}

/// Splits one top-level declarator into the imports it binds (a plain or
/// destructured `require`) and the names it merely declares.
fn require_declarator(
    text: &str,
    declarator: Node<'_>,
    name: Node<'_>,
) -> (Vec<(String, ImportBinding)>, Vec<String>) {
    let Some(module) = declarator
        .child_by_field_name("value")
        .and_then(|value| require_module(text, value))
    else {
        return (Vec::new(), pattern_names(text, name));
    };
    let binding = |kind| ImportBinding {
        module: module.clone(),
        kind,
    };
    match name.kind() {
        "identifier" => (
            vec![(
                node_text(text, name).to_string(),
                binding(ImportKind::Default),
            )],
            Vec::new(),
        ),
        "object_pattern" => {
            let (mut imported, mut declared) = (Vec::new(), Vec::new());
            let mut cursor = name.walk();
            for entry in name.named_children(&mut cursor) {
                let named = match entry.kind() {
                    "shorthand_property_identifier_pattern" => Some((entry, entry)),
                    "object_assignment_pattern" => {
                        entry.child_by_field_name("left").map(|left| (left, left))
                    }
                    "pair_pattern" => entry
                        .child_by_field_name("value")
                        .filter(|value| value.kind() == "identifier")
                        .zip(entry.child_by_field_name("key")),
                    _ => None,
                };
                match named {
                    Some((local, original)) => imported.push((
                        node_text(text, local).to_string(),
                        binding(ImportKind::Named(node_text(text, original).to_string())),
                    )),
                    None => declared.extend(pattern_names(text, entry)),
                }
            }
            (imported, declared)
        }
        _ => (Vec::new(), pattern_names(text, name)),
    }
}

/// Every name a binding pattern or parameter list declares, including rest,
/// default and nested destructured entries.
fn pattern_names(text: &str, pattern: Node<'_>) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = vec![pattern];
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                names.push(node_text(text, node).to_string())
            }
            "pair_pattern" => pending.extend(node.child_by_field_name("value")),
            "assignment_pattern" | "object_assignment_pattern" => {
                pending.extend(node.child_by_field_name("left"))
            }
            "required_parameter" | "optional_parameter" => {
                pending.extend(node.child_by_field_name("pattern"))
            }
            "formal_parameters" | "object_pattern" | "array_pattern" | "rest_pattern" => {
                let mut cursor = node.walk();
                pending.extend(node.named_children(&mut cursor));
            }
            _ => {}
        }
    }
    names
}

/// The target of a bare `name = …` expression statement.
fn bare_assignment_target<'a>(text: &'a str, statement: Node<'_>) -> Option<&'a str> {
    let assignment = statement
        .named_child(0)
        .filter(|node| node.kind() == "assignment_expression")?;
    let target = assignment
        .child_by_field_name("left")
        .filter(|node| node.kind() == "identifier")?;
    Some(node_text(text, target))
}

fn is_scope_kind(kind: &str) -> bool {
    matches!(
        kind,
        "statement_block" | "class_body" | "object" | "switch_body"
    )
}

fn declared_name(text: &str, node: Node<'_>) -> Vec<String> {
    node.child_by_field_name("name")
        .map(|name| vec![node_text(text, name).to_string()])
        .unwrap_or_default()
}

/// One interval per block-like node, read by a single iterative walk. Returns
/// `None` when the scope work exceeds `MAX_SCOPE_WORK`.
fn scope_intervals(text: &str, root: Node<'_>) -> Option<Vec<ScopeInterval>> {
    let mut intervals: Vec<ScopeInterval> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut body_params: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut work = 0usize;

    let mut cursor = root.walk();
    let mut descend = true;
    loop {
        let node = cursor.node();
        if descend {
            let kind = node.kind();
            let mut names: Vec<String> = Vec::new();
            match kind {
                kind if is_scope_kind(kind) => {
                    open.push(intervals.len());
                    intervals.push(ScopeInterval {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        local_decls: BTreeSet::new(),
                        params: body_params.remove(&node.start_byte()).unwrap_or_default(),
                    });
                }
                "function_declaration"
                | "generator_function_declaration"
                | "function_expression"
                | "generator_function"
                | "arrow_function" => {
                    if let Some(body) = node
                        .child_by_field_name("body")
                        .filter(|body| body.kind() == "statement_block")
                    {
                        let parameters = node
                            .child_by_field_name("parameters")
                            .or_else(|| node.child_by_field_name("parameter"));
                        body_params.insert(
                            body.start_byte(),
                            parameters
                                .map(|parameters| pattern_names(text, parameters))
                                .unwrap_or_default()
                                .into_iter()
                                .collect(),
                        );
                    }
                    if kind.ends_with("_declaration") {
                        names = declared_name(text, node);
                    }
                }
                "class_declaration" | "abstract_class_declaration" => {
                    names = declared_name(text, node);
                }
                "lexical_declaration" | "variable_declaration" => {
                    let mut declarators = node.walk();
                    names = node
                        .named_children(&mut declarators)
                        .filter_map(|declarator| declarator.child_by_field_name("name"))
                        .flat_map(|name| pattern_names(text, name))
                        .collect();
                }
                "expression_statement" => {
                    names.extend(bare_assignment_target(text, node).map(str::to_string));
                }
                _ => {}
            }
            work = work.saturating_add(open.len().saturating_mul(names.len()));
            if work > MAX_SCOPE_WORK {
                return None;
            }
            if !names.is_empty() {
                for index in &open {
                    intervals[*index].local_decls.extend(names.iter().cloned());
                }
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        if is_scope_kind(node.kind()) {
            open.pop();
        }
        if cursor.goto_next_sibling() {
            descend = true;
        } else if cursor.goto_parent() {
            descend = false;
        } else {
            return Some(intervals);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::jest_vitest::runner_call_names;
    use super::*;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::core::model::{ContentHash, RepositoryRevision, SemanticFactKind};
    use crate::ports::parser::{SourceDocument, SourceParser};
    use std::time::{Duration, Instant};

    const EXPRESS: &str = "import express from \"express\";\n";
    const GET: &str = "app.get(\"/a\", (req, res) => res.send(\"a\"));\n";

    fn language(path: &str) -> Language {
        if path.ends_with(".js") {
            Language::JavaScript
        } else {
            Language::TypeScript
        }
    }

    fn graph(path: &str, text: &str) -> ScopeGraphLite {
        ScopeGraphLite::analyze(text, path, &language(path))
    }

    fn is_express_app(path: &str, text: &str) -> bool {
        let graph = graph(path, text);
        !graph.degraded
            && graph
                .imports
                .get("express")
                .is_some_and(|b| b.module == "express")
            && graph.express_receivers.get("app") == Some(&ExpressReceiver::App)
            && !graph.name_is_unsafe_at("app", text.len())
    }

    /// Sources the retired line reader misread (multi-line clause, brace in a
    /// literal or comment, extra declarator, declaration after other code,
    /// keyword lookalike) bind `app` to express.
    #[test]
    fn tree_reads_what_the_line_reader_misread() {
        let cases = [
            ("multi_import.ts", format!("import express, {{\n  Router,\n}} from \"express\";\nconst app = express();\n{GET}")),
            ("multi_decl.ts", format!("{EXPRESS}const app =\n  express();\n{GET}")),
            ("brace_string.ts", format!("const b = \"{{\";\n{EXPRESS}const app = express();\n{GET}")),
            ("brace_comment.ts", format!("// {{\n{EXPRESS}const app = express();\n{GET}")),
            ("brace_block_comment.ts", format!("/* open {{ */\n{EXPRESS}const app = express();\n{GET}")),
            ("brace_regex.ts", format!("const o = /{{/;\n{EXPRESS}const app = express();\n{GET}")),
            ("brace_template.ts", format!("const t = `{{`;\n{EXPRESS}const app = express();\n{GET}")),
            ("extra_declarator.ts", format!("{EXPRESS}const n = 1, app = express();\n{GET}")),
            ("not_line_start.ts", format!("{EXPRESS}start(); const app = express();\n{GET}")),
            ("import_not_line_start.ts", format!("start(); {EXPRESS}const app = express();\n{GET}")),
            ("template_lookalike.ts", format!("{EXPRESS}const app = express();\nconst d = `\nconst app = express();\n`;\n{GET}")),
            ("type_import.ts", format!("import type {{ Router }} from \"express\";\n{EXPRESS}const app = express();\n{GET}")),
            ("balanced.ts", format!("{EXPRESS}const app = express();\n// {{ }}\nconst t = `v ${{1}}`;\n{GET}")),
        ];
        for (path, text) in &cases {
            assert!(is_express_app(path, text), "{path}");
        }
        let g = graph(
            "multi_require.js",
            "const {\n  Router,\n} = require(\"express\");\nconst r = Router();\n",
        );
        assert_eq!(g.express_receivers.get("r"), Some(&ExpressReceiver::Router));
    }

    /// Shadows and non-imports the retired reader missed.
    #[test]
    fn tree_reads_shadows_and_non_imports() {
        let head = format!("{EXPRESS}const app = express();\n");
        for (path, tail) in [
            ("rest.ts", "function f(...app) { app.get(\"/x\", g); }\n"),
            ("object.ts", "function f({ app }) { app.get(\"/x\", g); }\n"),
            (
                "array.ts",
                "const f = ([app]) => { app.get(\"/x\", g); };\n",
            ),
            (
                "default.ts",
                "function f(a, { b: app = 1 } = {}) { app.get(\"/x\", g); }\n",
            ),
        ] {
            let text = format!("{head}{tail}");
            let g = graph(path, &text);
            assert!(
                g.name_is_unsafe_at("app", text.find("app.get").unwrap()),
                "{path}"
            );
            assert!(!g.name_is_unsafe_at("app", head.len() - 1), "{path}");
        }
        // `require(...).default` is not an express import.
        assert!(
            graph("m.js", "const express = require(\"express\").default;\n")
                .imports
                .is_empty()
        );
        // A later declarator and an `async function` redeclare `app`.
        for (path, tail) in [
            ("dup.ts", "let n2 = 2, app = 4;\n"),
            ("async.ts", "export async function app() {}\n"),
        ] {
            assert!(
                graph(path, &format!("{head}{tail}"))
                    .unsafe_names
                    .contains("app"),
                "{path}"
            );
        }
    }

    /// Unlisted classes: a non-scope brace pair opens no scope, a function
    /// or class name is not shadowed by its own declaration, and an overload
    /// signature is not a redeclaration.
    #[test]
    fn tree_scopes_only_block_like_nodes() {
        let text = "const app = 1;\ntype T = { a: string };\nenum E { A }\nconst { x } = y;\n";
        let g = graph("braces.ts", text);
        assert!(g.scope_intervals.is_empty() && !g.name_is_unsafe_at("a", text.len()));
        let text = "function register() {\n  return 1;\n}\nclass UsersController {}\n";
        let g = graph("overlap.ts", text);
        assert!(!g.name_is_unsafe_at("register", text.find("register").unwrap()));
        assert!(g.local_decls.contains("register") && g.local_decls.contains("UsersController"));
        let text =
            "function f(a: string): void;\nfunction f(a: number): void;\nfunction f(a: any) {}\n";
        let g = graph("overload.ts", text);
        assert!(!g.unsafe_names.contains("f") && g.local_decls.contains("f"));
    }

    /// D4: a syntax error reads no binding and no runner name, and every
    /// considered unit gets exactly one `tsjs_parse_degraded` UNKNOWN.
    #[test]
    fn syntax_errors_refuse_every_considered_unit_once() {
        let cases = [
            ("error_top.ts", format!("{EXPRESS}const app = express();\n{GET}}}\n")),
            ("error_unclosed.ts", format!("{EXPRESS}const app = express();\napp.get(\"/a\", (req, res) => {{\n  res.send(\"a\");\n")),
            ("error_missing.ts", format!("{EXPRESS}const app = express();\nconst broken = ;\n{GET}")),
            ("error_inside.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport async function list() {\n  const x = (;\n  return prisma.user.findMany();\n}\n".to_string()),
            ("error_test.test.ts", "import { describe, it } from \"vitest\";\ndescribe(\"s\", () => {\n  it(\"t\", () => {\n    const = 1;\n  });\n});\n".to_string()),
            ("error_jsx_in_ts.ts", format!("{EXPRESS}const app = express();\nconst el = <div>;\n{GET}")),
        ];
        for (path, text) in &cases {
            let g = graph(path, text);
            assert!(g.degraded && g.imports.is_empty(), "{path}");
            assert_eq!(runner_call_names(&g), Default::default(), "{path}");
            let document = || SourceDocument {
                path,
                language: language(path),
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                text,
            };
            let units = SyntaxCodeUnitParser.parse(document()).expect("parse").units;
            let facts =
                super::super::anchors_with_bindings(&document(), &units, None, g).expect("facts");
            assert!(!facts.is_empty(), "{path}");
            for fact in &facts {
                assert_eq!(fact.kind, SemanticFactKind::Unknown, "{path}");
                assert!(
                    fact.assumptions
                        .iter()
                        .any(|a| a == "tsjs_unknown_kind=tsjs_parse_degraded"),
                    "{path}"
                );
            }
            let subjects: BTreeSet<_> = facts.iter().map(|fact| &fact.subject).collect();
            assert_eq!(subjects.len(), facts.len(), "{path}: one UNKNOWN per unit");
        }
    }

    /// Runner-name channel: aliased runner imports are named unless shadowed.
    #[test]
    fn runner_names_follow_the_graph() {
        let text = "import { test as t, describe as d, it } from \"vitest\";\nd(\"s\", () => { t(\"a\", () => {}); });\n";
        let names = runner_call_names(&graph("a.test.ts", text));
        assert_eq!(names.suite_names, BTreeSet::from(["d".to_string()]));
        assert_eq!(
            names.test_names,
            BTreeSet::from(["t".to_string(), "it".to_string()])
        );
        let text = "import { test as t } from \"vitest\";\nlet t = 1;\n";
        assert!(runner_call_names(&graph("b.test.ts", text))
            .test_names
            .is_empty());
    }

    /// Input is untrusted: deep nesting with a declaration per level is
    /// refused inside the scope-work bound instead of running quadratically.
    #[test]
    fn deep_nesting_stays_bounded() {
        let depth = 3_000;
        let text = format!("{}{}", "{ let a = 1;\n".repeat(depth), "}".repeat(depth));
        let started = Instant::now();
        assert!(graph("deep.ts", &text).degraded);
        assert!(started.elapsed() < Duration::from_secs(30));
        let started = Instant::now();
        let flat = "{ ".repeat(50_000) + &"}".repeat(50_000);
        let g = graph("flat.ts", &flat);
        assert!(!g.degraded && g.scope_intervals.len() == 50_000);
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    /// The tree reads every clause form.
    #[test]
    fn tree_graph_reads_every_clause_form() {
        let text = "import d, { a as b, type T } from \"m\";\nimport * as ns from \"n\";\nconst { x, y: z, ...rest } = require(\"r\");\nconst q = require(\"q\"), w = 1;\nlet [p, [o]] = pair;\nexport default async function* gen() {}\nconst f = ({ a1 }, [a2], ...a3) => {\n  v = 1;\n};\nuse = 2;\n";
        let g = graph("x.ts", text);
        let imports: BTreeSet<String> = g
            .imports
            .iter()
            .map(|(name, binding)| format!("{name}={}:{:?}", binding.module, binding.kind))
            .collect();
        assert_eq!(
            imports,
            BTreeSet::from(
                [
                    "d=m:Default",
                    "b=m:Named(\"a\")",
                    "T=m:Named(\"T\")",
                    "ns=n:Namespace",
                    "x=r:Named(\"x\")",
                    "z=r:Named(\"y\")",
                    "q=q:Default",
                ]
                .map(String::from)
            )
        );
        let decls: Vec<&str> = g.local_decls.iter().map(String::as_str).collect();
        assert_eq!(decls, ["f", "gen", "o", "p", "rest", "w"]);
        assert!(g.name_is_unsafe_at("a3", text.find("v = 1").unwrap()));
        assert!(g.name_is_unsafe_at("v", text.find("v = 1").unwrap()));
        assert!(g.name_is_unsafe_at("use", text.len()));
        assert!(!g.name_is_unsafe_at("use", 0));
    }
}
