//! The call parts, receivers and decorators the TS/JS anchor modules decide
//! on, read from the Tree-sitter tree (ADR-0059 slice 2).
//!
//! The anchor modules keep every policy; they consume the small data types
//! below. Production reads the tree `ScopeGraphLite` already produced, so a
//! file is parsed once for bindings and anchors. The text reading that
//! preceded it was deleted when slice 2 was accepted (ADR-0059 D6).

use super::scope_graph::{node_text, ExpressReceiver, ImportBinding, ImportKind, ScopeGraphLite};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use tree_sitter::{Node, Tree};

/// How a Fastify plugin registration spells its options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Prefix<'a> {
    /// No second argument, or an object without a `prefix` key.
    Absent,
    Literal(&'a str),
    /// A `prefix` key whose value is not a string literal.
    Dynamic,
    /// The second argument is not an object literal.
    NotObject,
}

/// The fields of a Fastify full-route object literal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RouteObject<'a> {
    pub(super) method: Option<&'a str>,
    /// The literal `url`, else the literal `path`.
    pub(super) path: Option<&'a str>,
    pub(super) has_handler: bool,
}

/// `receiver.method(arguments)` as the Express, Fastify and Hono anchors read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CallParts<'a> {
    pub(super) receiver: &'a str,
    pub(super) method: &'a str,
    /// `None` when the arguments could not be read.
    pub(super) arity: Option<usize>,
    /// Text between the quotes of the route path literal.
    pub(super) path_literal: Option<&'a str>,
    pub(super) first_is_string: bool,
    /// The route handler when it is a bare identifier.
    pub(super) handler: Option<&'a str>,
    /// The first argument when it is a bare identifier.
    pub(super) plugin: Option<&'a str>,
    pub(super) prefix: Prefix<'a>,
    pub(super) route_object: RouteObject<'a>,
}

/// `client.model.operation(...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PrismaQuery<'a> {
    pub(super) client: &'a str,
    pub(super) model: &'a str,
    pub(super) operation: &'a str,
}

/// `client.$transaction(...)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PrismaTransaction<'a> {
    pub(super) client: &'a str,
    pub(super) array_argument: bool,
}

/// `db.operation(table)` and its relatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DrizzleQuery<'a> {
    pub(super) db: &'a str,
    pub(super) operation: &'a str,
    pub(super) table: &'a str,
}

/// One decorator as the NestJS anchors read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Decorator<'a> {
    pub(super) name: &'a str,
    pub(super) path_shape: &'static str,
    pub(super) module_shape: String,
    pub(super) for_root: bool,
}

/// The nodes from the root down to the deepest one containing `byte`.
fn path_to(tree: &Tree, byte: usize) -> Vec<Node<'_>> {
    let mut path = Vec::new();
    let mut cursor = tree.walk();
    loop {
        path.push(cursor.node());
        if cursor.goto_first_child_for_byte(byte).is_none() {
            return path;
        }
    }
}

fn is_optional(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let optional = node
        .children(&mut cursor)
        .any(|child| matches!(child.kind(), "optional_chain" | "?."));
    optional
}

/// The dotted identifier path of a call's callee (`a.b.c(...)`), if that is
/// all it is.
fn callee_path<'a>(text: &'a str, call: Node<'_>) -> Option<Vec<&'a str>> {
    if is_optional(call) {
        return None;
    }
    let mut node = call.child_by_field_name("function")?;
    let mut names = Vec::new();
    loop {
        match node.kind() {
            "identifier" => {
                names.push(node_text(text, node));
                names.reverse();
                return Some(names);
            }
            "member_expression" if !is_optional(node) => {
                let property = node.child_by_field_name("property")?;
                if property.kind() != "property_identifier" {
                    return None;
                }
                names.push(node_text(text, property));
                node = node.child_by_field_name("object")?;
            }
            _ => return None,
        }
    }
}

/// The innermost call starting at `start` whose callee path has one of
/// `segments` identifiers: the path from the root down to it (the call is the
/// last node), and its callee path.
fn call_at<'t, 'a>(
    tree: &'t Tree,
    text: &'a str,
    start: usize,
    segments: &[usize],
) -> Option<(Vec<Node<'t>>, Vec<&'a str>)> {
    let mut path = path_to(tree, start);
    while let Some(node) = path.last().copied() {
        if node.kind() == "call_expression" && node.start_byte() == start {
            if let Some(names) =
                callee_path(text, node).filter(|names| segments.contains(&names.len()))
            {
                return Some((path, names));
            }
        }
        path.pop();
    }
    None
}

/// The call arguments without comments; `None` for a tagged template.
fn arguments(call: Node<'_>) -> Option<Vec<Node<'_>>> {
    let node = call
        .child_by_field_name("arguments")
        .filter(|node| node.kind() == "arguments")?;
    Some(members(node))
}

/// The named children of an argument list or literal, without comments.
fn members(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect();
    found
}

/// The text between the quotes of a string literal node.
fn string_inner<'a>(text: &'a str, node: Node<'_>) -> Option<&'a str> {
    let literal = node_text(text, node);
    (node.kind() == "string" && literal.len() >= 2)
        .then(|| literal.get(1..literal.len() - 1))
        .flatten()
}

/// The name an object member declares, and its value when it has one.
fn member_key<'a, 't>(text: &'a str, member: Node<'t>) -> Option<(&'a str, Option<Node<'t>>)> {
    let key = |node: Node<'_>| match node.kind() {
        "property_identifier" => Some(node_text(text, node)),
        "string" => string_inner(text, node),
        _ => None,
    };
    match member.kind() {
        "pair" => Some((
            key(member.child_by_field_name("key")?)?,
            member.child_by_field_name("value"),
        )),
        "shorthand_property_identifier" => Some((node_text(text, member), None)),
        "method_definition" => Some((key(member.child_by_field_name("name")?)?, None)),
        _ => None,
    }
}

fn string_field<'a>(text: &'a str, object: Node<'_>, name: &str) -> Option<&'a str> {
    members(object).into_iter().find_map(|member| {
        let (key, value) = member_key(text, member)?;
        (key == name)
            .then_some(value)
            .flatten()
            .and_then(|value| string_inner(text, value))
    })
}

fn prefix_of<'a>(text: &'a str, options: Node<'_>) -> Prefix<'a> {
    if options.kind() != "object" {
        return Prefix::NotObject;
    }
    if let Some(prefix) = string_field(text, options, "prefix") {
        return Prefix::Literal(prefix);
    }
    if members(options)
        .into_iter()
        .any(|member| member_key(text, member).is_some_and(|(key, _)| key == "prefix"))
    {
        Prefix::Dynamic
    } else {
        Prefix::Absent
    }
}

fn route_object<'a>(text: &'a str, object: Node<'_>) -> RouteObject<'a> {
    RouteObject {
        method: string_field(text, object, "method"),
        path: string_field(text, object, "url").or_else(|| string_field(text, object, "path")),
        has_handler: members(object)
            .into_iter()
            .any(|member| member_key(text, member).is_some_and(|(key, _)| key == "handler")),
    }
}

fn tree_route_call<'a>(tree: &Tree, text: &'a str, start: usize) -> Option<CallParts<'a>> {
    let (path, names) = call_at(tree, text, start, &[2])?;
    let args = arguments(*path.last()?);
    let first = args.as_ref().and_then(|args| args.first().copied());
    let identifier = |node: Node<'_>| (node.kind() == "identifier").then(|| node_text(text, node));
    let handler = args.as_ref().and_then(|args| match args.as_slice() {
        [first, second] if first.kind() == "string" => identifier(*second),
        [only] => identifier(*only),
        _ => None,
    });
    Some(CallParts {
        receiver: names[0],
        method: names[1],
        arity: args.as_ref().map(Vec::len),
        path_literal: first.and_then(|first| string_inner(text, first)),
        first_is_string: first.is_some_and(|first| first.kind() == "string"),
        handler,
        plugin: first.and_then(identifier),
        prefix: args
            .as_ref()
            .and_then(|args| args.get(1))
            .map_or(Prefix::Absent, |options| prefix_of(text, *options)),
        route_object: first
            .filter(|first| first.kind() == "object")
            .map(|object| route_object(text, object))
            .unwrap_or_default(),
    })
}

/// The route call (`receiver.method(...)`) a unit starts with.
pub(super) fn route_call<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<CallParts<'a>> {
    tree_route_call(bindings.tree.as_ref()?, text, unit.start)
}

fn tree_prisma_query<'a>(tree: &Tree, text: &'a str, start: usize) -> Option<PrismaQuery<'a>> {
    let (_, names) = call_at(tree, text, start, &[3])?;
    Some(PrismaQuery {
        client: names[0],
        model: names[1],
        operation: names[2],
    })
}

pub(super) fn prisma_query<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<PrismaQuery<'a>> {
    tree_prisma_query(bindings.tree.as_ref()?, text, unit.start)
}

fn tree_prisma_transaction<'a>(
    tree: &Tree,
    text: &'a str,
    start: usize,
) -> Option<PrismaTransaction<'a>> {
    let (path, names) = call_at(tree, text, start, &[2])?;
    if names[1] != "$transaction" {
        return None;
    }
    let first = arguments(*path.last()?).and_then(|arguments| arguments.first().copied());
    Some(PrismaTransaction {
        client: names[0],
        array_argument: first.is_some_and(|first| first.kind() == "array"),
    })
}

pub(super) fn prisma_transaction<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<PrismaTransaction<'a>> {
    tree_prisma_transaction(bindings.tree.as_ref()?, text, unit.start)
}

fn first_identifier<'a>(text: &'a str, call: Node<'_>) -> Option<&'a str> {
    let first = arguments(call)?.first().copied()?;
    (first.kind() == "identifier").then(|| node_text(text, first))
}

fn tree_drizzle_query<'a>(tree: &Tree, text: &'a str, start: usize) -> Option<DrizzleQuery<'a>> {
    let (path, names) = call_at(tree, text, start, &[2, 4])?;
    let index = path.len() - 1;
    if names.len() == 4 {
        let operation = names[3];
        return (names[1] == "query" && matches!(operation, "findMany" | "findFirst")).then_some(
            DrizzleQuery {
                db: names[0],
                operation: if operation == "findMany" {
                    "query_findMany"
                } else {
                    "query_findFirst"
                },
                table: names[2],
            },
        );
    }
    let operation = names[1];
    let table = match operation {
        "select" => from_table(text, &path, index)?,
        "insert" | "update" | "delete" => first_identifier(text, path[index])?,
        _ => return None,
    };
    Some(DrizzleQuery {
        db: names[0],
        operation,
        table,
    })
}

/// The table argument of the first `.from(...)` in the call chain that
/// continues the call at `path[index]`.
fn from_table<'a>(text: &'a str, path: &[Node<'_>], index: usize) -> Option<&'a str> {
    let mut at = index;
    while at >= 2 {
        let (member, call) = (path[at - 1], path[at - 2]);
        if member.kind() != "member_expression"
            || call.kind() != "call_expression"
            || member.child_by_field_name("object") != Some(path[at])
            || call.child_by_field_name("function") != Some(member)
        {
            return None;
        }
        let property = member.child_by_field_name("property")?;
        if node_text(text, property) == "from" {
            return first_identifier(text, call);
        }
        at -= 2;
    }
    None
}

pub(super) fn drizzle_query<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<DrizzleQuery<'a>> {
    tree_drizzle_query(bindings.tree.as_ref()?, text, unit.start)
}

pub(super) fn drizzle_transaction<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<&'a str> {
    let (_, names) = call_at(bindings.tree.as_ref()?, text, unit.start, &[2])?;
    (names[1] == "transaction").then_some(names[0])
}

/// The call that starts a call chain: `z.object({}).strict()` is read as
/// `z.object(...)`.
fn chain_base_call(mut node: Node<'_>) -> Option<Node<'_>> {
    let mut base = None;
    loop {
        match node.kind() {
            "call_expression" => {
                base = Some(node).filter(|call| {
                    call.child_by_field_name("function")
                        .and_then(|function| function.child_by_field_name("object"))
                        .is_some_and(|object| object.kind() == "identifier")
                });
                node = node.child_by_field_name("function")?;
            }
            "member_expression" => node = node.child_by_field_name("object")?,
            _ => return base,
        }
    }
}

/// `name = factory(...)` of one declarator.
pub(super) fn table_declaration<'a>(
    text: &'a str,
    declarator: Node<'_>,
) -> Option<(&'a str, &'a str)> {
    let name = declarator
        .child_by_field_name("name")
        .filter(|name| name.kind() == "identifier")?;
    let value = declarator
        .child_by_field_name("value")
        .filter(|value| value.kind() == "call_expression")?;
    match callee_path(text, value)?.as_slice() {
        [factory] => Some((node_text(text, name), *factory)),
        _ => None,
    }
}

/// The first declarator of the declaration that starts at `start`.
fn declarator_at(tree: &Tree, start: usize) -> Option<Node<'_>> {
    let declaration = path_to(tree, start).into_iter().find_map(|node| {
        if node.start_byte() != start {
            return None;
        }
        match node.kind() {
            "lexical_declaration" | "variable_declaration" => Some(node),
            "export_statement" => node.child_by_field_name("declaration").filter(|inner| {
                matches!(inner.kind(), "lexical_declaration" | "variable_declaration")
            }),
            _ => None,
        }
    })?;
    let mut cursor = declaration.walk();
    let declarator = declaration
        .named_children(&mut cursor)
        .find(|child| child.kind() == "variable_declarator");
    declarator
}

/// `name = factory(...)` of the declaration that starts the unit.
pub(super) fn table_declaration_at<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<(&'a str, &'a str)> {
    table_declaration(text, declarator_at(bindings.tree.as_ref()?, unit.start)?)
}

/// `name = head.builder(...)` of the declaration that starts the unit; a call
/// chain is read from its first call.
pub(super) fn zod_builder<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<(&'a str, &'a str)> {
    let value = declarator_at(bindings.tree.as_ref()?, unit.start)?.child_by_field_name("value")?;
    let names = callee_path(text, chain_base_call(value)?)?;
    (names.len() == 2).then(|| (names[0], names[1]))
}

/// The bare name a test-runner call starts with.
pub(super) fn test_call_name<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Option<&'a str> {
    let (_, names) = call_at(bindings.tree.as_ref()?, text, unit.start, &[1])?;
    Some(names[0])
}

fn any_descendant(node: Node<'_>, matches: impl Fn(Node<'_>) -> bool) -> bool {
    let mut pending = vec![node];
    while let Some(next) = pending.pop() {
        if matches(next) {
            return true;
        }
        let mut cursor = next.walk();
        pending.extend(next.children(&mut cursor));
    }
    false
}

/// The number of members of the array that `key` holds in the metadata object.
fn array_member_count(text: &str, object: Option<Node<'_>>, key: &str) -> usize {
    let Some(object) = object.filter(|object| object.kind() == "object") else {
        return 0;
    };
    members(object)
        .into_iter()
        .find_map(|member| {
            let (name, value) = member_key(text, member)?;
            (name == key).then_some(value).flatten()
        })
        .filter(|value| value.kind() == "array")
        .map_or(0, |array| members(array).len())
}

fn decorator<'a>(text: &'a str, node: Node<'_>) -> Option<Decorator<'a>> {
    let expression = members(node).into_iter().next()?;
    let (name, call) = match expression.kind() {
        "identifier" => (node_text(text, expression), None),
        "call_expression" => {
            let function = expression.child_by_field_name("function")?;
            if function.kind() != "identifier" || is_optional(expression) {
                return None;
            }
            (node_text(text, function), Some(expression))
        }
        _ => return None,
    };
    let args = call.and_then(arguments);
    let first = args.as_ref().and_then(|args| args.first().copied());
    Some(Decorator {
        name,
        path_shape: match first {
            None => "none",
            Some(first) if matches!(first.kind(), "string" | "template_string") => "literal",
            Some(_) => "dynamic",
        },
        module_shape: if args.is_none() {
            "empty".to_string()
        } else {
            format!(
                "providers_{}_imports_{}_controllers_{}",
                array_member_count(text, first, "providers"),
                array_member_count(text, first, "imports"),
                array_member_count(text, first, "controllers"),
            )
        },
        for_root: first.is_some_and(|first| {
            any_descendant(first, |node| {
                node.kind() == "call_expression"
                    && node
                        .child_by_field_name("function")
                        .filter(|function| function.kind() == "member_expression")
                        .and_then(|function| function.child_by_field_name("property"))
                        .is_some_and(|property| node_text(text, property).starts_with("forRoot"))
            })
        }),
    })
}

fn tree_decorators<'a>(tree: &Tree, text: &'a str, start: usize) -> Vec<Decorator<'a>> {
    let path = path_to(tree, start);
    let Some(index) = path
        .iter()
        .rposition(|node| node.kind() == "decorator" && node.start_byte() == start)
    else {
        return Vec::new();
    };
    let Some(parent) = index.checked_sub(1).map(|parent| path[parent]) else {
        return Vec::new();
    };
    let mut cursor = parent.walk();
    let stack = parent
        .children(&mut cursor)
        .skip_while(|child| *child != path[index])
        .take_while(|child| matches!(child.kind(), "decorator" | "comment"))
        .filter(|child| child.kind() == "decorator")
        .collect::<Vec<_>>();
    stack
        .into_iter()
        .filter_map(|node| decorator(text, node))
        .collect()
}

/// The leading decorator stack a unit starts with.
pub(super) fn decorators<'a>(
    bindings: &ScopeGraphLite,
    text: &'a str,
    unit: Range<usize>,
) -> Vec<Decorator<'a>> {
    bindings
        .tree
        .as_ref()
        .map(|tree| tree_decorators(tree, text, unit.start))
        .unwrap_or_default()
}

/// How a top-level declarator or statement is handed to the receiver
/// classifiers.
#[derive(Clone, Copy)]
pub(super) enum Item<'t> {
    Declarator(Node<'t>),
    Statement(Node<'t>),
}

/// The receivers one file declares (the single receiver authority).
#[derive(Debug, Default)]
pub(super) struct Receivers {
    pub(super) express: BTreeMap<String, ExpressReceiver>,
    pub(super) express_prefixes: BTreeMap<String, String>,
    pub(super) fastify: BTreeSet<String>,
    pub(super) hono: BTreeSet<String>,
    pub(super) prisma: BTreeSet<String>,
    pub(super) drizzle_tables: BTreeSet<String>,
    pub(super) drizzle_dbs: BTreeSet<String>,
}

/// The kind of import `name` is, when it is a safe import of `module`.
fn imported<'i>(
    imports: &'i BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    name: &str,
    module: &str,
) -> Option<&'i ImportKind> {
    let binding = imports.get(name)?;
    (!unsafe_names.contains(name) && binding.module == module).then_some(&binding.kind)
}

fn express_receiver(
    text: &str,
    value: Node<'_>,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<ExpressReceiver> {
    if value.kind() != "call_expression" || !arguments(value)?.is_empty() {
        return None;
    }
    let express = |head: &str| imported(imports, unsafe_names, head, "express");
    match callee_path(text, value)?.as_slice() {
        [head] => match express(head)? {
            ImportKind::Default | ImportKind::Namespace => Some(ExpressReceiver::App),
            ImportKind::Named(original) if original == "Router" => Some(ExpressReceiver::Router),
            ImportKind::Named(_) => None,
        },
        [head, "Router"] => matches!(express(head)?, ImportKind::Default | ImportKind::Namespace)
            .then_some(ExpressReceiver::Router),
        _ => None,
    }
}

/// The import kind of the identifier a declarator calls (`name = head(...)`).
fn called_import<'i>(
    text: &str,
    value: Node<'_>,
    imports: &'i BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    module: impl Fn(&str) -> bool,
) -> Option<&'i ImportKind> {
    if value.kind() != "call_expression" {
        return None;
    }
    let names = callee_path(text, value)?;
    let [head] = names.as_slice() else {
        return None;
    };
    let binding = imports.get(*head)?;
    (!unsafe_names.contains(*head) && module(&binding.module)).then_some(&binding.kind)
}

/// Whether a declarator constructs `class` imported from `module`
/// (`new Class(...)`, type arguments allowed).
fn constructs(
    text: &str,
    value: Node<'_>,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    (module, class): (&str, &str),
) -> bool {
    if value.kind() != "new_expression" || value.child_by_field_name("arguments").is_none() {
        return false;
    }
    value
        .child_by_field_name("constructor")
        .filter(|constructor| constructor.kind() == "identifier")
        .and_then(|constructor| {
            imported(imports, unsafe_names, node_text(text, constructor), module)
        })
        .is_some_and(|kind| matches!(kind, ImportKind::Named(original) if original == class))
}

/// The prefix mount `app.use("/prefix", router)` of one statement.
fn prefix_mount(
    text: &str,
    statement: Node<'_>,
    express: &BTreeMap<String, ExpressReceiver>,
    unsafe_names: &BTreeSet<String>,
) -> Option<(String, String)> {
    let call = chain_base_call(statement.named_child(0)?)?;
    let names = callee_path(text, call)?;
    let [receiver, "use"] = names.as_slice() else {
        return None;
    };
    let [prefix, router]: [Node<'_>; 2] = arguments(call)?.try_into().ok()?;
    let router = (router.kind() == "identifier").then(|| node_text(text, router))?;
    if unsafe_names.contains(*receiver)
        || unsafe_names.contains(router)
        || !matches!(express.get(*receiver), Some(ExpressReceiver::App))
        || !matches!(express.get(router), Some(ExpressReceiver::Router))
    {
        return None;
    }
    Some((
        router.to_string(),
        super::normalize_route_path(string_inner(text, prefix)?),
    ))
}

/// Classifies the top-level declarators and statements of a file.
pub(super) fn receivers(
    text: &str,
    items: &[Item<'_>],
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    drizzle_table_factories: &BTreeSet<String>,
) -> Receivers {
    tree_receivers(text, items, imports, unsafe_names, drizzle_table_factories)
}

fn tree_receivers(
    text: &str,
    items: &[Item<'_>],
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    drizzle_table_factories: &BTreeSet<String>,
) -> Receivers {
    let mut found = Receivers::default();
    for item in items {
        match *item {
            Item::Declarator(node) => {
                let (Some(name), Some(value)) = (
                    node.child_by_field_name("name")
                        .filter(|name| name.kind() == "identifier")
                        .map(|name| node_text(text, name)),
                    node.child_by_field_name("value"),
                ) else {
                    continue;
                };
                if unsafe_names.contains(name) {
                    continue;
                }
                if let Some(receiver) = express_receiver(text, value, imports, unsafe_names) {
                    found.express.insert(name.to_string(), receiver);
                }
                let fastify = called_import(text, value, imports, unsafe_names, |module| {
                    module == "fastify"
                });
                if match fastify {
                    Some(ImportKind::Default | ImportKind::Namespace) => true,
                    Some(ImportKind::Named(original)) => {
                        matches!(original.as_str(), "fastify" | "Fastify")
                    }
                    None => false,
                } {
                    found.fastify.insert(name.to_string());
                }
                if constructs(text, value, imports, unsafe_names, ("hono", "Hono")) {
                    found.hono.insert(name.to_string());
                }
                if constructs(
                    text,
                    value,
                    imports,
                    unsafe_names,
                    ("@prisma/client", "PrismaClient"),
                ) {
                    found.prisma.insert(name.to_string());
                }
                if let Some((table, factory)) = table_declaration(text, node) {
                    if drizzle_table_factories.contains(factory) {
                        found.drizzle_tables.insert(table.to_string());
                    }
                }
                let drizzle = called_import(text, value, imports, unsafe_names, |module| {
                    module.starts_with("drizzle-orm")
                });
                if matches!(drizzle, Some(ImportKind::Named(original)) if original == "drizzle") {
                    found.drizzle_dbs.insert(name.to_string());
                }
            }
            Item::Statement(statement) => {
                if let Some((router, prefix)) =
                    prefix_mount(text, statement, &found.express, unsafe_names)
                {
                    found.express_prefixes.entry(router).or_insert(prefix);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Language;

    fn parts(text: &str, start: usize) -> Option<CallParts<'_>> {
        let graph = ScopeGraphLite::analyze(text, "a.ts", &Language::TypeScript);
        route_call(&graph, text, start..text.len())
    }

    #[test]
    fn route_call_parts_read_arguments_not_text() {
        let text = "app.get(\"/a\", /* c */ h,);";
        let call = parts(text, 0).expect("call");
        assert_eq!((call.receiver, call.method), ("app", "get"));
        assert_eq!((call.arity, call.path_literal), (Some(2), Some("/a")));
        assert_eq!(call.handler, Some("h"));

        let text = "app.get(p, (req, res) => res.send(\"ok\"));";
        assert_eq!(parts(text, 0).expect("call").path_literal, None);

        let text = "app.route({ method: \"GET\", url: \"/r\", handler() {} });";
        let object = parts(text, 0).expect("call").route_object;
        assert_eq!((object.method, object.path), (Some("GET"), Some("/r")));
        assert!(object.has_handler);

        let text = "app.register(plugin, { name: prefix, prefix: \"/p\" });";
        assert_eq!(parts(text, 0).expect("call").prefix, Prefix::Literal("/p"));
        let text = "app.register(plugin, { prefix });";
        assert_eq!(parts(text, 0).expect("call").prefix, Prefix::Dynamic);
        let text = "app.register(plugin, opts);";
        assert_eq!(parts(text, 0).expect("call").prefix, Prefix::NotObject);
    }

    #[test]
    fn route_call_requires_an_identifier_chain_at_the_unit_start() {
        for text in [
            "this.get(\"/a\");",
            "app?.get(\"/a\");",
            "a.b.get(\"/a\");",
            "// app.get(\"/a\");",
            "get(\"/a\");",
        ] {
            assert_eq!(parts(text, 0), None, "{text}");
        }
        // The leftmost call of a chain is the unit's call.
        let text = "app.use(\"/x\", mw).get(\"/y\", h);";
        assert_eq!(parts(text, 0).expect("call").method, "use");
    }

    #[test]
    fn receivers_read_annotated_and_generic_declarators() {
        let text = "import express from \"express\";\nimport { Hono } from \"hono\";\nconst app: Express = express();\nconst h = new Hono<{ A: { B: C<D> } }>();\nconst router = express.Router();\napp.use(\"/api\", router);\n";
        let graph = ScopeGraphLite::analyze(text, "a.ts", &Language::TypeScript);
        assert_eq!(
            graph.express_receivers.get("app"),
            Some(&ExpressReceiver::App)
        );
        assert!(graph.hono_receivers.contains("h"));
        assert_eq!(
            graph
                .express_router_prefixes
                .get("router")
                .map(String::as_str),
            Some("/api")
        );
    }
}
