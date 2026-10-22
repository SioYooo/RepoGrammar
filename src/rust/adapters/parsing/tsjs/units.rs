//! TS/JS code units read from the Tree-sitter tree (ADR-0059 slice 3).
//!
//! The unit kinds, framework policies and unit names are those of the
//! line-based extractor this replaced (kept under `cfg(test)` in
//! `parsing/legacy_units.rs` as the differential oracle). Only the reading
//! changes: a unit is a syntax node, so its end is the node's end, and text
//! inside comments, strings and templates is never a unit. See
//! `docs/experiments/tsjs-unit-extraction-tree.md` for the node each unit kind
//! comes from.

use super::jest_vitest::runner_call_names;
use super::parts::{chain_base_call, is_optional};
use super::scope_graph::{node_text, ScopeGraphLite};
use crate::core::model::CodeUnitKind;
use std::collections::BTreeSet;
use tree_sitter::Node;

/// A unit as the syntax scanner records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawUnit {
    pub(crate) kind: CodeUnitKind,
    pub(crate) name: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

const STATIC_ROUTE_METHODS: [&str; 9] = [
    "get", "head", "post", "put", "delete", "options", "patch", "all", "use",
];
const PRISMA_RAW: [&str; 4] = [
    "$queryRaw",
    "$executeRaw",
    "$queryRawUnsafe",
    "$executeRawUnsafe",
];
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
const ZOD_BUILDERS: [&str; 5] = ["object", "union", "discriminatedUnion", "enum", "array"];
const NEXT_ROUTE_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const NEST_HTTP_DECORATORS: [&str; 8] = [
    "Get", "Post", "Put", "Delete", "Patch", "Head", "Options", "All",
];

/// How a unit's kind is settled once the whole file has been read.
enum Pending {
    Kind(CodeUnitKind),
    /// A function or arrow: a React hook or component by name, else this kind.
    Callable(CodeUnitKind),
}

/// A unit before ordering. The legacy extractor numbered units by phase
/// (classes, methods, then per-line patterns), so a unit set that is equal
/// keeps equal ids.
struct Candidate {
    phase: u8,
    /// The byte whose line orders the unit (the legacy anchor offset).
    anchor: usize,
    rank: u8,
    start: usize,
    end: usize,
    name: String,
    pending: Pending,
}

pub(crate) fn extract(text: &str, path: &str, graph: &ScopeGraphLite) -> Vec<RawUnit> {
    let Some(tree) = graph.tree.as_ref() else {
        return Vec::new();
    };
    let mut walker = Walker::new(text, path, graph);
    walker.run(tree.root_node());
    walker.finish()
}

struct Walker<'a> {
    text: &'a str,
    path: &'a str,
    graph: &'a ScopeGraphLite,
    suites: BTreeSet<String>,
    tests: BTreeSet<String>,
    next_kind: Option<CodeUnitKind>,
    candidates: Vec<Candidate>,
    /// Named classes the walk is inside.
    class_depth: usize,
    jsx: bool,
}

impl<'a> Walker<'a> {
    fn new(text: &'a str, path: &'a str, graph: &'a ScopeGraphLite) -> Self {
        let runner = runner_call_names(graph);
        let mut suites = runner.suite_names;
        let mut tests = runner.test_names;
        suites.insert("describe".to_string());
        tests.insert("it".to_string());
        tests.insert("test".to_string());
        Self {
            text,
            path,
            graph,
            suites,
            tests,
            next_kind: next_file_convention_kind(path),
            candidates: Vec::new(),
            class_depth: 0,
            jsx: false,
        }
    }

    /// An iterative walk (nesting is untrusted); `stack` holds the ancestors.
    fn run(&mut self, root: Node<'a>) {
        let mut cursor = root.walk();
        let mut stack: Vec<Node<'a>> = Vec::new();
        loop {
            let node = cursor.node();
            self.enter(node, stack.last().copied(), &stack);
            if cursor.goto_first_child() {
                stack.push(node);
                continue;
            }
            loop {
                self.leave(cursor.node());
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    return;
                }
                stack.pop();
            }
        }
    }

    fn enter(&mut self, node: Node<'a>, parent: Option<Node<'a>>, stack: &[Node<'a>]) {
        if !node.is_named() {
            return;
        }
        match node.kind() {
            "class_declaration" | "abstract_class_declaration" | "class" => {
                self.class(node, parent);
                if is_named_class(node) {
                    self.class_depth += 1;
                }
            }
            "method_definition" | "method_signature" | "abstract_method_signature" => {
                self.method(node, parent);
            }
            "function_declaration"
            | "generator_function_declaration"
            | "function_signature"
            | "function_expression"
            | "generator_function" => self.function(node),
            "lexical_declaration" | "variable_declaration" => self.declaration(node),
            "call_expression" => self.call(node, stack),
            "export_statement" => self.export(node),
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => self.jsx = true,
            _ => {}
        }
    }

    fn leave(&mut self, node: Node<'a>) {
        if node.is_named()
            && matches!(
                node.kind(),
                "class_declaration" | "abstract_class_declaration" | "class"
            )
            && is_named_class(node)
        {
            self.class_depth -= 1;
        }
    }

    fn text_of(&self, node: Node<'_>) -> &'a str {
        node_text(self.text, node)
    }

    fn push(
        &mut self,
        (phase, rank): (u8, u8),
        anchor: usize,
        range: (usize, usize),
        name: &str,
        pending: Pending,
    ) {
        if name.is_empty() {
            return;
        }
        self.candidates.push(Candidate {
            phase,
            anchor,
            rank,
            start: range.0,
            end: range.1,
            name: name.to_string(),
            pending,
        });
    }

    /// The expression name of a decorator: `@Name` or `@Name(...)`.
    fn decorator_name(&self, decorator: Node<'_>) -> Option<&'a str> {
        let mut cursor = decorator.walk();
        let expression = decorator
            .named_children(&mut cursor)
            .find(|child| child.kind() != "comment")?;
        match expression.kind() {
            "identifier" => Some(self.text_of(expression)),
            "call_expression" => expression
                .child_by_field_name("function")
                .filter(|function| function.kind() == "identifier")
                .map(|function| self.text_of(function)),
            _ => None,
        }
    }

    fn decorators_of(node: Node<'a>) -> Vec<Node<'a>> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "decorator")
            .collect();
        found
    }

    fn class(&mut self, node: Node<'a>, parent: Option<Node<'a>>) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let keyword = keyword_start(node, "class");
        let mut decorators = Self::decorators_of(node);
        if let Some(parent) = parent.filter(|parent| parent.kind() == "export_statement") {
            decorators.extend(Self::decorators_of(parent));
        }
        decorators.sort_by_key(|decorator| decorator.start_byte());
        let nest = decorators
            .iter()
            .filter_map(|decorator| self.decorator_name(*decorator))
            .find_map(|name| match name {
                "Controller" => Some(CodeUnitKind::NestController),
                "Injectable" => Some(CodeUnitKind::NestInjectable),
                "Module" => Some(CodeUnitKind::NestModule),
                _ => None,
            });
        let (kind, start) = match nest {
            Some(kind) => (
                kind,
                decorators
                    .first()
                    .map_or(keyword, |decorator| decorator.start_byte()),
            ),
            None => (CodeUnitKind::Class, keyword),
        };
        let name = self.text_of(name);
        self.push(
            (0, 0),
            keyword,
            (start, node.end_byte()),
            name,
            Pending::Kind(kind),
        );
    }

    fn method(&mut self, node: Node<'a>, parent: Option<Node<'a>>) {
        if self.class_depth == 0 || parent.is_none_or(|parent| parent.kind() != "class_body") {
            return;
        }
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let (name_start, name_text) = match name.kind() {
            "property_identifier" => (name.start_byte(), self.text_of(name)),
            "private_property_identifier" => (
                name.start_byte() + 1,
                self.text_of(name).trim_start_matches('#'),
            ),
            _ => return,
        };
        // A class member's decorators are its preceding siblings in the body.
        let mut decorators = Vec::new();
        let mut previous = node.prev_sibling();
        while let Some(sibling) = previous {
            match sibling.kind() {
                "decorator" => decorators.push(sibling),
                "comment" => {}
                _ => break,
            }
            previous = sibling.prev_sibling();
        }
        decorators.reverse();
        let route = decorators.iter().any(|decorator| {
            self.decorator_name(*decorator)
                .is_some_and(|name| NEST_HTTP_DECORATORS.contains(&name))
        });
        let (kind, start) = if route {
            (
                CodeUnitKind::NestRoute,
                decorators
                    .first()
                    .map_or(name_start, |decorator| decorator.start_byte()),
            )
        } else {
            (CodeUnitKind::Method, name_start)
        };
        self.push(
            (1, 0),
            name_start,
            (start, node.end_byte()),
            name_text,
            Pending::Kind(kind),
        );
    }

    fn function(&mut self, node: Node<'a>) {
        if self.class_depth > 0 {
            return;
        }
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let start = keyword_start(node, "function");
        let name = self.text_of(name);
        self.push(
            (2, 13),
            start,
            (start, node.end_byte()),
            name,
            Pending::Callable(CodeUnitKind::Function),
        );
    }

    fn declaration(&mut self, node: Node<'a>) {
        let mut cursor = node.walk();
        let declarators: Vec<Node<'a>> = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "variable_declarator")
            .collect();
        for (index, declarator) in declarators.into_iter().enumerate() {
            let Some(name) = declarator
                .child_by_field_name("name")
                .filter(|name| name.kind() == "identifier")
            else {
                continue;
            };
            let Some(value) = declarator.child_by_field_name("value") else {
                continue;
            };
            let start = if index == 0 {
                node.start_byte()
            } else {
                declarator.start_byte()
            };
            let range = (start, declarator.end_byte());
            let name = self.text_of(name);
            if value.kind() == "arrow_function" && self.class_depth == 0 {
                self.push(
                    (2, 14),
                    start,
                    range,
                    name,
                    Pending::Callable(CodeUnitKind::ArrowFunction),
                );
            }
            if value.kind() != "call_expression" {
                continue;
            }
            let names = self.graph.unit_names();
            let is_table = value
                .child_by_field_name("function")
                .filter(|function| function.kind() == "identifier")
                .is_some_and(|function| names.table_factories.contains(self.text_of(function)));
            if is_table {
                self.push(
                    (2, 5),
                    start,
                    range,
                    name,
                    Pending::Kind(CodeUnitKind::DrizzleSchemaTable),
                );
            }
            let is_schema = chain_base_call(value)
                .and_then(|call| call.child_by_field_name("function"))
                .and_then(|function| {
                    Some((
                        function.child_by_field_name("object")?,
                        function.child_by_field_name("property")?,
                    ))
                })
                .is_some_and(|(head, builder)| {
                    names.zod.contains(self.text_of(head))
                        && ZOD_BUILDERS.contains(&self.text_of(builder))
                });
            if is_schema {
                self.push(
                    (2, 8),
                    start,
                    range,
                    name,
                    Pending::Kind(CodeUnitKind::ZodSchema),
                );
            }
        }
    }

    fn call(&mut self, node: Node<'a>, stack: &[Node<'a>]) {
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let range = (node.start_byte(), node.end_byte());
        match function.kind() {
            "identifier" => {
                let name = self.text_of(function);
                if self.suites.contains(name) {
                    self.push(
                        (2, 9),
                        range.0,
                        range,
                        name,
                        Pending::Kind(CodeUnitKind::TestSuite),
                    );
                }
                if self.tests.contains(name) {
                    self.push(
                        (2, 11),
                        range.0,
                        range,
                        name,
                        Pending::Kind(CodeUnitKind::TestCase),
                    );
                }
            }
            "member_expression" => self.member_call(node, function, stack),
            "subscript_expression" => self.subscript_call(node, function),
            _ => {}
        }
    }

    fn member_call(&mut self, node: Node<'a>, function: Node<'a>, stack: &[Node<'a>]) {
        let (Some(object), Some(property)) = (
            function.child_by_field_name("object"),
            function.child_by_field_name("property"),
        ) else {
            return;
        };
        if property.kind() != "property_identifier" {
            return;
        }
        let name = self.text_of(property);
        let range = (node.start_byte(), chain_end(node, stack));
        let receiver =
            (object.kind() == "identifier" && !is_optional(function)).then(|| self.text_of(object));
        let names = self.graph.unit_names();
        let fastify = receiver.is_some_and(|name| names.fastify.contains(name));
        if STATIC_ROUTE_METHODS.contains(&name) && !self.chain_has_route(object) {
            let kind = if receiver.is_some_and(|name| names.hono.contains(name)) {
                CodeUnitKind::HonoRoute
            } else if fastify {
                CodeUnitKind::FastifyRoute
            } else {
                CodeUnitKind::ExpressRoute
            };
            self.push((2, 2), range.0, range, name, Pending::Kind(kind));
        } else if name == "route" && fastify {
            self.push(
                (2, 2),
                range.0,
                range,
                "route",
                Pending::Kind(CodeUnitKind::FastifyRoute),
            );
        } else if name == "register" && fastify {
            self.push(
                (2, 2),
                range.0,
                range,
                "register",
                Pending::Kind(CodeUnitKind::FastifyPluginRegistration),
            );
        }
        // Prisma.
        if PRISMA_RAW.contains(&name) && ends_in_identifier(object) {
            self.push(
                (2, 3),
                range.0,
                range,
                "raw",
                Pending::Kind(CodeUnitKind::PrismaQuery),
            );
        } else if PRISMA_OPERATIONS.contains(&name)
            && object.kind() == "member_expression"
            && object
                .child_by_field_name("property")
                .is_some_and(|model| model.kind() == "property_identifier")
            && object
                .child_by_field_name("object")
                .is_some_and(ends_in_identifier)
        {
            self.push(
                (2, 3),
                range.0,
                range,
                name,
                Pending::Kind(CodeUnitKind::PrismaQuery),
            );
        }
        if name == "$transaction" && ends_in_identifier(object) {
            self.push(
                (2, 4),
                range.0,
                range,
                "transaction",
                Pending::Kind(CodeUnitKind::PrismaTransaction),
            );
        }
        // Drizzle.
        if let Some(operation) = self.drizzle_operation(node, name, object, stack) {
            self.push(
                (2, 6),
                range.0,
                range,
                operation,
                Pending::Kind(CodeUnitKind::DrizzleQuery),
            );
        }
        if name == "transaction" && ends_in_identifier(object) {
            self.push(
                (2, 7),
                range.0,
                range,
                "transaction",
                Pending::Kind(CodeUnitKind::DrizzleTransaction),
            );
        }
    }

    /// Whether the call chain that `object` begins already holds a route call:
    /// the unit is made once per chain, at its innermost route call.
    fn chain_has_route(&self, object: Node<'a>) -> bool {
        let mut node = object;
        loop {
            match node.kind() {
                "call_expression" => {
                    let Some(function) = node.child_by_field_name("function") else {
                        return false;
                    };
                    if function.kind() == "member_expression"
                        && function
                            .child_by_field_name("property")
                            .is_some_and(|property| {
                                STATIC_ROUTE_METHODS.contains(&self.text_of(property))
                            })
                    {
                        return true;
                    }
                    node = function;
                }
                "member_expression" => match node.child_by_field_name("object") {
                    Some(object) => node = object,
                    None => return false,
                },
                "await_expression" | "non_null_expression" | "parenthesized_expression" => {
                    match node.named_child(0) {
                        Some(inner) => node = inner,
                        None => return false,
                    }
                }
                _ => return false,
            }
        }
    }

    /// The Drizzle query operation this call is, if any.
    fn drizzle_operation(
        &self,
        node: Node<'a>,
        name: &'a str,
        object: Node<'a>,
        stack: &[Node<'a>],
    ) -> Option<&'static str> {
        match name {
            "findMany" | "findFirst" => {
                let table = object
                    .child_by_field_name("object")
                    .filter(|_| object.kind() == "member_expression")?;
                let relation = (table.kind() == "member_expression"
                    && table
                        .child_by_field_name("property")
                        .is_some_and(|query| self.text_of(query) == "query"))
                .then(|| table.child_by_field_name("object"))
                .flatten()?;
                (object
                    .child_by_field_name("property")
                    .is_some_and(|model| model.kind() == "property_identifier")
                    && ends_in_identifier(relation))
                .then_some(if name == "findMany" {
                    "query_findMany"
                } else {
                    "query_findFirst"
                })
            }
            "execute" if object.kind() == "identifier" => Some("execute"),
            "select" if object.kind() == "identifier" => self
                .chain_continues_with_from(node, stack)
                .then_some("select"),
            "insert" | "update" | "delete" if object.kind() == "identifier" => {
                let first_is_string = node
                    .child_by_field_name("arguments")
                    .and_then(|arguments| arguments.named_child(0))
                    .is_some_and(|first| first.kind() == "string");
                (!first_is_string).then_some(match name {
                    "insert" => "insert",
                    "update" => "update",
                    _ => "delete",
                })
            }
            _ => None,
        }
    }

    /// Whether the chain `call` begins goes on to a `.from(...)` call.
    fn chain_continues_with_from(&self, call: Node<'a>, stack: &[Node<'a>]) -> bool {
        let mut current = call;
        let mut ancestors = stack.iter().rev();
        while let (Some(member), Some(outer)) = (ancestors.next(), ancestors.next()) {
            if member.kind() != "member_expression"
                || outer.kind() != "call_expression"
                || member.child_by_field_name("object") != Some(current)
                || outer.child_by_field_name("function") != Some(*member)
            {
                return false;
            }
            if member
                .child_by_field_name("property")
                .is_some_and(|property| self.text_of(property) == "from")
            {
                return true;
            }
            current = *outer;
        }
        false
    }

    fn subscript_call(&mut self, node: Node<'a>, function: Node<'a>) {
        let Some(object) = function.child_by_field_name("object") else {
            return;
        };
        let begins_with_identifier = self
            .text_of(object)
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'));
        let has_string = node
            .child_by_field_name("arguments")
            .is_some_and(|arguments| {
                let mut cursor = arguments.walk();
                let found = arguments
                    .named_children(&mut cursor)
                    .any(|argument| argument.kind() == "string");
                found
            });
        if !begins_with_identifier || !has_string {
            return;
        }
        let fastify = object.kind() == "identifier"
            && self
                .graph
                .unit_names()
                .fastify
                .contains(self.text_of(object));
        let kind = if fastify {
            CodeUnitKind::FastifyRoute
        } else {
            CodeUnitKind::ExpressRoute
        };
        self.push(
            (2, 2),
            node.start_byte(),
            (node.start_byte(), node.end_byte()),
            "dynamic_route",
            Pending::Kind(kind),
        );
    }

    fn export(&mut self, node: Node<'a>) {
        let Some(kind) = self.next_kind.clone() else {
            return;
        };
        let start = node.start_byte();
        let range = (start, node.end_byte());
        if kind == CodeUnitKind::NextRouteHandler {
            if let Some(method) = self.route_handler_method(node) {
                self.push(
                    (2, 1),
                    start,
                    range,
                    method,
                    Pending::Kind(CodeUnitKind::NextRouteHandler),
                );
            }
            return;
        }
        let is_default = {
            let mut cursor = node.walk();
            let found = node
                .children(&mut cursor)
                .any(|child| !child.is_named() && child.kind() == "default");
            found
        };
        if !is_default {
            return;
        }
        let Some(value) = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("declaration"))
        else {
            return;
        };
        let name = match kind {
            CodeUnitKind::NextAppPage
            | CodeUnitKind::NextAppLayout
            | CodeUnitKind::NextPagesPage => "default",
            CodeUnitKind::NextPagesApiRoute => "handler",
            _ => return,
        };
        let create_element = kind != CodeUnitKind::NextPagesApiRoute;
        if self.default_export_is_function(node, value, create_element) {
            self.push((2, 0), start, range, name, Pending::Kind(kind));
        }
    }

    /// Whether the exported value is function-like, or holds a function-like
    /// node (or a `createElement` call) that starts on the statement's first
    /// line.
    fn default_export_is_function(
        &self,
        statement: Node<'a>,
        value: Node<'a>,
        create_element: bool,
    ) -> bool {
        let first_line_end = statement.start_byte()
            + self
                .text
                .get(statement.start_byte()..)
                .and_then(|rest| rest.find('\n'))
                .unwrap_or(usize::MAX - statement.start_byte());
        let mut pending = vec![value];
        while let Some(node) = pending.pop() {
            if node.start_byte() >= first_line_end {
                continue;
            }
            if node.is_named()
                && (is_function_like(node.kind())
                    || (create_element
                        && matches!(node.kind(), "identifier" | "property_identifier")
                        && self.text_of(node) == "createElement"))
            {
                return true;
            }
            let mut cursor = node.walk();
            pending.extend(node.children(&mut cursor));
        }
        false
    }

    /// `export function GET` or `export const GET = async (...) =>`, the first
    /// of the HTTP methods in list order.
    fn route_handler_method(&self, statement: Node<'a>) -> Option<&'static str> {
        let declaration = statement.child_by_field_name("declaration")?;
        let mut declared: Vec<&str> = Vec::new();
        match declaration.kind() {
            "function_declaration" => {
                if let Some(name) = declaration.child_by_field_name("name") {
                    declared.push(self.text_of(name));
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut cursor = declaration.walk();
                for declarator in declaration.named_children(&mut cursor) {
                    let value = declarator.child_by_field_name("value");
                    let is_async_arrow = value.is_some_and(|value| {
                        let mut parts = value.walk();
                        let async_arrow = value.kind() == "arrow_function"
                            && value
                                .children(&mut parts)
                                .any(|child| !child.is_named() && child.kind() == "async");
                        async_arrow
                    });
                    if let (true, Some(name)) =
                        (is_async_arrow, declarator.child_by_field_name("name"))
                    {
                        declared.push(self.text_of(name));
                    }
                }
            }
            _ => {}
        }
        NEXT_ROUTE_METHODS
            .into_iter()
            .find(|method| declared.contains(method))
    }

    fn finish(mut self) -> Vec<RawUnit> {
        let mut line_starts = vec![0usize];
        line_starts.extend(self.text.match_indices('\n').map(|(index, _)| index + 1));
        let line_of = |offset: usize| line_starts.partition_point(|start| *start <= offset);
        self.candidates.sort_by_key(|candidate| {
            (
                candidate.phase,
                line_of(candidate.anchor),
                candidate.rank,
                candidate.start,
                candidate.end,
            )
        });
        let path = self.path;
        let jsx = self.jsx;
        self.candidates
            .into_iter()
            .map(|candidate| {
                let kind = match candidate.pending {
                    Pending::Kind(kind) => kind,
                    Pending::Callable(fallback) => {
                        classify_callable(path, &candidate.name, fallback, jsx)
                    }
                };
                RawUnit {
                    kind,
                    name: candidate.name,
                    start: candidate.start,
                    end: candidate.end,
                }
            })
            .collect()
    }
}

/// The end of the call chain that `call` begins: the outermost call that
/// continues it with `.member(...)`, so the unit holds the whole expression
/// (`db.select().from(t).where(...)`).
fn chain_end(call: Node<'_>, stack: &[Node<'_>]) -> usize {
    let mut current = call;
    let mut ancestors = stack.iter().rev();
    while let (Some(member), Some(outer)) = (ancestors.next(), ancestors.next()) {
        if member.kind() != "member_expression"
            || outer.kind() != "call_expression"
            || member.child_by_field_name("object") != Some(current)
            || outer.child_by_field_name("function") != Some(*member)
        {
            break;
        }
        current = *outer;
    }
    current.end_byte()
}

fn is_named_class(node: Node<'_>) -> bool {
    node.child_by_field_name("name").is_some()
}

/// The start of the anonymous keyword token `keyword` among `node`'s children.
fn keyword_start(node: Node<'_>, keyword: &str) -> usize {
    let mut cursor = node.walk();
    let start = node
        .children(&mut cursor)
        .find(|child| !child.is_named() && child.kind() == keyword)
        .map_or(node.start_byte(), |token| token.start_byte());
    start
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
    )
}

/// An identifier, or a member chain that ends in a property identifier
/// (`prisma`, `this.prisma`, `a.b.c`): what the line reader took for the
/// identifier before a dot.
fn ends_in_identifier(node: Node<'_>) -> bool {
    match node.kind() {
        "identifier" => true,
        "member_expression" => node
            .child_by_field_name("property")
            .is_some_and(|property| property.kind() == "property_identifier"),
        _ => false,
    }
}

fn classify_callable(path: &str, name: &str, fallback: CodeUnitKind, jsx: bool) -> CodeUnitKind {
    if is_hook_name(name) {
        return CodeUnitKind::ReactHook;
    }
    if is_component_name(name) && (is_react_path(path) || jsx) {
        return CodeUnitKind::ReactComponent;
    }
    fallback
}

pub(crate) fn is_hook_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("use") else {
        return false;
    };
    rest.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

pub(crate) fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

pub(crate) fn is_react_path(path: &str) -> bool {
    path.ends_with(".tsx") || path.ends_with(".jsx")
}

fn is_named_file(path: &str, stem: &str) -> bool {
    [".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|extension| path.ends_with(&format!("/{stem}{extension}")))
}

pub(crate) fn next_file_convention_kind(path: &str) -> Option<CodeUnitKind> {
    let path = path.trim_start_matches("./");
    let extension_ok = [".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|extension| path.ends_with(extension));
    if !extension_ok {
        return None;
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "page") {
        return Some(CodeUnitKind::NextAppPage);
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "layout") {
        return Some(CodeUnitKind::NextAppLayout);
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "route") {
        return Some(CodeUnitKind::NextRouteHandler);
    }
    // Next.js supports the Pages Router under both `pages/` and `src/pages/`,
    // mirroring the `app/` vs `src/app/` handling above.
    let pages_path = path.strip_prefix("src/").unwrap_or(path);
    if pages_path.starts_with("pages/api/")
        && !pages_path.ends_with(".tsx")
        && !pages_path.ends_with(".jsx")
    {
        return Some(CodeUnitKind::NextPagesApiRoute);
    }
    if pages_path.starts_with("pages/") && !pages_path.starts_with("pages/api/") {
        return Some(CodeUnitKind::NextPagesPage);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Language;

    /// `kind:name:text` of every unit, in emission order, without the module.
    fn read(path: &str, text: &str) -> Vec<String> {
        let language = if path.ends_with(".js") || path.ends_with(".jsx") {
            Language::JavaScript
        } else {
            Language::TypeScript
        };
        let graph = ScopeGraphLite::analyze(text, path, &language);
        extract(text, path, &graph)
            .into_iter()
            .map(|unit| {
                format!(
                    "{}:{}:{}",
                    unit.kind.as_str(),
                    unit.name,
                    &text[unit.start..unit.end]
                )
            })
            .collect()
    }

    fn kinds(path: &str, text: &str) -> Vec<String> {
        read(path, text)
            .into_iter()
            .map(|unit| unit.splitn(3, ':').take(2).collect::<Vec<_>>().join(":"))
            .collect()
    }

    #[test]
    fn classes_and_members_come_from_class_nodes() {
        let text = "export abstract class A extends B<{ x: 1 }> {\n  private m(): void {}\n  static async s() {}\n  #p() {}\n  get g() { return 1; }\n  run() {\n    this.go();\n    await f();\n    const o = { inner() {} };\n  }\n}\n";
        assert_eq!(
            read("a.ts", text),
            [
                "class:A:class A extends B<{ x: 1 }> {\n  private m(): void {}\n  static async s() {}\n  #p() {}\n  get g() { return 1; }\n  run() {\n    this.go();\n    await f();\n    const o = { inner() {} };\n  }\n}",
                "method:m:m(): void {}",
                "method:s:s() {}",
                "method:p:p() {}",
                "method:g:g() { return 1; }",
                "method:run:run() {\n    this.go();\n    await f();\n    const o = { inner() {} };\n  }",
            ]
        );
        // Statements and object-literal methods in a class body are not
        // methods (`statement_taken_for_method`).
        assert_eq!(
            kinds("a.ts", text)
                .iter()
                .filter(|k| *k == "method:this")
                .count(),
            0
        );
    }

    #[test]
    fn nest_decorators_start_the_unit_at_the_earliest_decorator() {
        let text = "@Controller(\"a\")\nexport class C {\n  // note\n  @Get(\"x\")\n  @Other()\n  find() {}\n  @ns.Get()\n  plain() {}\n}\n@Injectable() class S {}\n@Foo() @Module({}) class M {}\n";
        let units = read("a.ts", text);
        assert_eq!(
            units,
            [
                "nest_controller:C:@Controller(\"a\")\nexport class C {\n  // note\n  @Get(\"x\")\n  @Other()\n  find() {}\n  @ns.Get()\n  plain() {}\n}",
                "nest_injectable:S:@Injectable() class S {}",
                "nest_module:M:@Foo() @Module({}) class M {}",
                "nest_route:find:@Get(\"x\")\n  @Other()\n  find() {}",
                "method:plain:plain() {}",
            ]
        );
    }

    #[test]
    fn functions_and_arrows_start_at_their_keyword_and_end_at_the_node() {
        let text = "export async function* gen() {}\nconst a = (x): number => x + 1, b = async <T,>(y: T) => y;\nfunction over(a: string): void;\nconst f: (x: number) => void = (x) => {};\nconst v = foo(() => 1);\nconst e = function named() {};\nconst anon = function () {};\n";
        assert_eq!(
            read("a.ts", text),
            [
                "function:gen:function* gen() {}",
                "arrow_function:a:const a = (x): number => x + 1",
                "arrow_function:b:b = async <T,>(y: T) => y",
                "function:over:function over(a: string): void;",
                "arrow_function:f:const f: (x: number) => void = (x) => {}",
                "function:named:function named() {}",
            ]
        );
    }

    #[test]
    fn react_kinds_follow_names_and_jsx_nodes() {
        let hook = "export const useThing = () => 1;\nfunction Widget() { return 1; }\n";
        assert_eq!(
            kinds("a.ts", hook),
            ["react_hook:useThing", "function:Widget"]
        );
        // `</` in a string is not JSX (`jsx_text_lookalike`); a JSX node is.
        let lookalike = "function Widget() { return \"</div>\"; }\n";
        assert_eq!(kinds("a.js", lookalike), ["function:Widget"]);
        let jsx = "function Widget() { return <div />; }\nconst Card = () => <p>x</p>;\n";
        assert_eq!(
            kinds("a.js", jsx),
            ["react_component:Widget", "react_component:Card"]
        );
        assert_eq!(
            kinds("a.tsx", "function Widget() { return 1; }\n"),
            ["react_component:Widget"]
        );
    }

    #[test]
    fn route_units_are_made_once_per_chain_at_the_chain_start() {
        let text = "import Fastify from \"fastify\";\nimport { Hono } from \"hono\";\nconst f = Fastify();\nconst h = new Hono();\nconst app = express();\nf.get(\"/f\", a);\nh.post(\"/h\", b);\napp.get(\"/a\", c).post(\"/b\", d);\nrouter\n  .route(\"/r\")\n  .put(e);\nf.route({ url: \"/r\" });\nf.register(p);\napp?.get(\"/o\", g);\nhandlers[k](\"x\");\nconst m = new Map(); m.get(1); m.delete(2);\n";
        let units = read("a.ts", text);
        let routes: Vec<&str> = units.iter().map(String::as_str).collect();
        let expect = |needle: &str| assert!(routes.contains(&needle), "{needle}\n{routes:#?}");
        expect("fastify_route:get:f.get(\"/f\", a)");
        expect("hono_route:post:h.post(\"/h\", b)");
        expect("express_route:get:app.get(\"/a\", c).post(\"/b\", d)");
        expect("express_route:put:router\n  .route(\"/r\")\n  .put(e)");
        expect("fastify_route:route:f.route({ url: \"/r\" })");
        expect("fastify_plugin_registration:register:f.register(p)");
        expect("express_route:get:app?.get(\"/o\", g)");
        expect("express_route:dynamic_route:handlers[k](\"x\")");
        // The `.post` after `.get` in one chain is not a second unit.
        assert_eq!(
            routes
                .iter()
                .filter(|unit| unit.contains("post(\"/b\""))
                .count(),
            1
        );
        // `m.get(1)` and `m.delete(2)` are route-shaped, as they always were.
        expect("express_route:get:m.get(1)");
        // Comments and strings are not code.
        let quiet =
            "// app.get(\"/x\", h);\nconst s = \"app.get('/y', h)\";\n/* app.post(\"/z\", h) */\n";
        assert!(kinds("a.ts", quiet).is_empty(), "{:?}", read("a.ts", quiet));
    }

    #[test]
    fn prisma_and_drizzle_units_hold_the_whole_chain() {
        let text = "const r = prisma.user.findMany({ where: { a: 1 } }).then((x) => x);\nprisma.$transaction([a]);\nprisma.$queryRaw`select 1`;\nthis.prisma.user.create({});\nthis.items.delete(k);\ndb.select().from(t).where(eq(t.id, 1));\ndb.select();\ndb.insert(t).values(v).returning();\ndb.delete(\"x\");\ndb.execute(sql`x`);\ndb.query.users.findMany({});\ndb.transaction(async (tx) => {});\n";
        let units = read("a.ts", text);
        let has = |needle: &str| {
            assert!(
                units.iter().any(|unit| unit == needle),
                "{needle}\n{units:#?}"
            )
        };
        has("prisma_query:findMany:prisma.user.findMany({ where: { a: 1 } }).then((x) => x)");
        has("prisma_transaction:transaction:prisma.$transaction([a])");
        has("prisma_query:raw:prisma.$queryRaw`select 1`");
        // A keyword receiver starts at the chain start, not mid-chain.
        has("prisma_query:create:this.prisma.user.create({})");
        assert!(!units
            .iter()
            .any(|unit| unit.starts_with("prisma_query:delete")));
        has("drizzle_query:select:db.select().from(t).where(eq(t.id, 1))");
        assert_eq!(
            units
                .iter()
                .filter(|unit| unit.starts_with("drizzle_query:select"))
                .count(),
            1,
            "select without from is not a query"
        );
        has("drizzle_query:insert:db.insert(t).values(v).returning()");
        assert!(!units
            .iter()
            .any(|unit| unit.starts_with("drizzle_query:delete")));
        has("drizzle_query:execute:db.execute(sql`x`)");
        has("drizzle_query:query_findMany:db.query.users.findMany({})");
        has("drizzle_transaction:transaction:db.transaction(async (tx) => {})");
    }

    #[test]
    fn schema_and_test_units() {
        let text = "import { z } from \"zod\";\nimport { pgTable as table } from \"drizzle-orm/pg-core\";\nexport const A = z.object({}).strict();\nconst B = z.string();\nexport const users = table(\"u\", {}), t2 = pgTable(\"v\", {});\n";
        assert_eq!(
            read("a.ts", text),
            [
                "zod_schema:A:const A = z.object({}).strict()",
                "drizzle_schema_table:users:const users = table(\"u\", {})",
                "drizzle_schema_table:t2:t2 = pgTable(\"v\", {})",
            ]
        );
        let tests = "import { describe as d, it, expect } from \"vitest\";\nd(\"s\", () => {\n  it(\"t\", () => {});\n  test.skip(\"u\", () => {});\n  // it(\"ghost\", () => {});\n});\ntest(\"ambient\", () => {});\nclass A { describe() {} }\n";
        assert_eq!(
            kinds("a.test.ts", tests),
            [
                "class:A",
                "method:describe",
                "test_suite:d",
                "test_case:it",
                "test_case:test",
            ]
        );
    }

    #[test]
    fn next_conventions_read_export_statements() {
        let page = "export default function Page() { return 1; }\n";
        assert_eq!(
            kinds("app/page.tsx", page),
            ["next_app_page:default", "react_component:Page"]
        );
        assert!(kinds("lib/page.tsx", page)
            .iter()
            .all(|k| !k.starts_with("next_")));
        let wrapped = "export default memo(function Page() { return 1; });\nconst x = 1;\n";
        assert_eq!(
            kinds("pages/index.tsx", wrapped),
            ["next_pages_page:default", "react_component:Page"]
        );
        let not_function = "export default Page;\n";
        assert!(kinds("pages/index.tsx", not_function).is_empty());
        let api = "export default async (req, res) => res.end();\n";
        assert_eq!(
            kinds("pages/api/x.ts", api),
            ["next_pages_api_route:handler"]
        );
        let route = "export async function GET() {}\nexport const POST = async () => {};\nexport const PUT = () => {};\nexport function GETX() {}\n";
        let handlers: Vec<String> = kinds("app/api/route.ts", route)
            .into_iter()
            .filter(|unit| unit.starts_with("next_"))
            .collect();
        assert_eq!(
            handlers,
            ["next_route_handler:GET", "next_route_handler:POST"]
        );
    }

    #[test]
    fn syntax_errors_keep_the_units_the_tree_still_holds() {
        let text = "import express from \"express\";\nconst app = express();\nconst broken = ;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\nfunction ok() {}\n";
        let units = kinds("a.ts", text);
        assert!(
            units.contains(&"express_route:get".to_string()),
            "{units:?}"
        );
        assert!(units.contains(&"function:ok".to_string()), "{units:?}");
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let depth = 4000;
        let text = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        let _ = read("a.ts", &text);
        let nested = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
        let _ = read("a.ts", &nested);
    }
}
