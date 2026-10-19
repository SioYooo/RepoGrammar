use super::leading_identifier;
use crate::core::model::Language;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser};

// Temporary ADR-0059 differential scaffolding: the legacy line reader and its
// comparison tests are deleted once the maintainer accepts the slice-1
// classification.
#[cfg(test)]
#[path = "scope_graph_differential.rs"]
mod differential;
#[cfg(test)]
#[path = "legacy_scope_graph.rs"]
mod legacy;

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
#[cfg_attr(test, derive(Clone))]
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
                Self::from_tree(text, tree.root_node()).unwrap_or_else(Self::degraded)
            }
            _ => Self::degraded(),
        }
    }

    fn degraded() -> Self {
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
        let mut items: Vec<String> = Vec::new();

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
                    let keyword = statement.child(0).map_or("", |node| node_text(text, node));
                    let mut declarators = statement.walk();
                    for declarator in statement.named_children(&mut declarators) {
                        if declarator.kind() != "variable_declarator" {
                            continue;
                        }
                        items.push(format!("{keyword} {}", node_text(text, declarator)));
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
                    items.push(node_text(text, statement).to_string());
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
        graph.fill_receivers(items.iter().map(String::as_str));
        Some(graph)
    }

    /// Classifies top-level declarators and statements with the right-hand-side
    /// classifiers below, which are the single receiver authority.
    fn fill_receivers<'a>(&mut self, items: impl IntoIterator<Item = &'a str>) {
        for (local, binding) in &self.imports {
            if binding.module.starts_with("drizzle-orm")
                && matches!(&binding.kind, ImportKind::Named(original) if super::drizzle::DRIZZLE_TABLE_FACTORIES.contains(&original.as_str()))
                && !self.unsafe_names.contains(local)
            {
                self.drizzle_table_factories.insert(local.clone());
            }
        }
        for line in items {
            if let Some((name, receiver)) =
                express_receiver_declaration(line, &self.imports, &self.unsafe_names)
            {
                if !self.unsafe_names.contains(&name) {
                    self.express_receivers.insert(name, receiver);
                }
            }
            if let Some(name) =
                fastify_receiver_declaration(line, &self.imports, &self.unsafe_names)
            {
                if !self.unsafe_names.contains(&name) {
                    self.fastify_receivers.insert(name);
                }
            }
            if let Some(name) = hono_receiver_declaration(line, &self.imports, &self.unsafe_names) {
                if !self.unsafe_names.contains(&name) {
                    self.hono_receivers.insert(name);
                }
            }
            if let Some(name) = prisma_client_declaration(line, &self.imports, &self.unsafe_names) {
                if !self.unsafe_names.contains(&name) {
                    self.prisma_clients.insert(name);
                }
            }
            if let Some((table, factory)) = super::drizzle::table_declaration_parts(line) {
                if self.drizzle_table_factories.contains(factory)
                    && !self.unsafe_names.contains(table)
                {
                    self.drizzle_tables.insert(table.to_string());
                }
            }
            if let Some(name) = drizzle_db_declaration(line, &self.imports, &self.unsafe_names) {
                if !self.unsafe_names.contains(&name) {
                    self.drizzle_dbs.insert(name);
                }
            }
            if let Some((router, prefix)) =
                express_router_prefix_mount(line, &self.express_receivers, &self.unsafe_names)
            {
                self.express_router_prefixes.entry(router).or_insert(prefix);
            }
        }
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

fn node_text<'a>(text: &'a str, node: Node<'_>) -> &'a str {
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

fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export ").unwrap_or(line)
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

fn express_router_prefix_mount(
    line: &str,
    express_receivers: &BTreeMap<String, ExpressReceiver>,
    unsafe_names: &BTreeSet<String>,
) -> Option<(String, String)> {
    let (receiver, method) = super::route_call_parts(line)?;
    if method != "use"
        || unsafe_names.contains(receiver)
        || !matches!(express_receivers.get(receiver), Some(ExpressReceiver::App))
    {
        return None;
    }
    let arguments = super::call_arguments(line)?;
    let [prefix_arg, router_arg] = arguments.as_slice() else {
        return None;
    };
    if !super::string_literal_arg(prefix_arg) {
        return None;
    }
    let prefix = super::first_quoted(prefix_arg)?;
    let (router, after_router) = leading_identifier(router_arg)?;
    if !router_arg[after_router..].trim().is_empty()
        || unsafe_names.contains(router)
        || !matches!(express_receivers.get(router), Some(ExpressReceiver::Router))
    {
        return None;
    }
    Some((router.to_string(), super::normalize_route_path(&prefix)))
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

fn hono_receiver_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<String> {
    let (name, rhs) = top_level_declaration_assignment(line)?;
    let rhs = rhs.trim().trim_end_matches(';').trim();
    let after_new = rhs.strip_prefix("new ")?;
    let (constructor, after_constructor) = leading_identifier(after_new)?;
    let tail = after_new[after_constructor..].trim_start();
    // Accept `new Hono()` and `new Hono<Env>()`.
    let tail = match tail.strip_prefix('<') {
        Some(rest) => rest.find('>').map(|index| rest[index + 1..].trim_start())?,
        None => tail,
    };
    if unsafe_names.contains(constructor) || !tail.starts_with('(') {
        return None;
    }
    let binding = imports.get(constructor)?;
    if binding.module == "hono"
        && matches!(&binding.kind, ImportKind::Named(original) if original == "Hono")
    {
        Some(name.to_string())
    } else {
        None
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
