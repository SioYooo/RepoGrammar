//! The text readings that preceded the tree readings of `parts` (ADR-0059
//! slice 2). Test-only: the differential oracle (D6), kept until the
//! maintainer accepts the slice's classification. Each function is the
//! pre-slice implementation, moved here and adapted only to return the shared
//! parts types.

use super::parts::{
    CallParts, DrizzleQuery, Item, Prefix, PrismaQuery, PrismaTransaction, Receivers, RouteObject,
};
use super::scope_graph::{ExpressReceiver, ImportBinding, ImportKind};
use super::{is_identifier_byte, leading_identifier, normalize_route_path};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn route_call_parts(slice: &str) -> Option<(&str, &str)> {
    let (receiver, after) = leading_identifier(slice)?;
    let rest = slice[after..].trim_start().strip_prefix('.')?;
    let (method, after_method) = leading_identifier(rest)?;
    if !rest[after_method..].trim_start().starts_with('(') {
        return None;
    }
    Some((receiver, method))
}

fn route_path_literal(slice: &str) -> Option<&str> {
    let open = slice.find('(')?;
    first_quoted(&slice[open + 1..])
}

fn first_quoted(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote == b'"' || quote == b'\'' {
            let start = index + 1;
            let end_relative = text[start..].find(quote as char)?;
            return Some(&text[start..start + end_relative]);
        }
        index += 1;
    }
    None
}

fn route_handler_identifier(slice: &str) -> Option<&str> {
    let arguments = call_arguments(slice)?;
    let candidate = match arguments.as_slice() {
        [first, second] if string_literal_arg(first) => *second,
        [only] => *only,
        _ => return None,
    };
    let (handler, after_handler) = leading_identifier(candidate)?;
    if candidate[after_handler..].trim().is_empty() {
        Some(handler)
    } else {
        None
    }
}

fn string_literal_arg(argument: &str) -> bool {
    let trimmed = argument.trim_start();
    matches!(trimmed.as_bytes().first(), Some(b'"' | b'\''))
}

fn call_arguments(slice: &str) -> Option<Vec<&str>> {
    let open = slice.find('(')?;
    let bytes = slice.as_bytes();
    let mut arguments = Vec::new();
    let mut start = open + 1;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for index in open + 1..bytes.len() {
        let byte = bytes[index];
        if let Some(quote_byte) = quote {
            if escape {
                escape = false;
                continue;
            }
            if byte == b'\\' {
                escape = true;
                continue;
            }
            if byte == quote_byte {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'(' | b'{' | b'[' => depth += 1,
            b')' => {
                if depth == 0 {
                    arguments.push(slice[start..index].trim());
                    return Some(arguments);
                }
                depth = depth.saturating_sub(1);
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => {
                arguments.push(slice[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    None
}

fn object_literal_string_field<'a>(slice: &'a str, field: &str) -> Option<&'a str> {
    for (field_index, _) in slice.match_indices(field) {
        if let Some(after_colon) = object_literal_field_after_colon(slice, field, field_index) {
            if let Some(value) = first_quoted(after_colon) {
                return Some(value);
            }
        }
    }
    None
}

fn object_literal_has_field(slice: &str, field: &str) -> bool {
    slice.match_indices(field).any(|(field_index, _)| {
        object_literal_field_after_colon(slice, field, field_index).is_some()
    })
}

fn object_literal_field_after_colon<'a>(
    slice: &'a str,
    field: &str,
    field_index: usize,
) -> Option<&'a str> {
    if !field_has_identifier_boundaries(slice, field_index, field.len()) {
        return None;
    }
    let after_field = &slice[field_index + field.len()..];
    after_field
        .trim_start()
        .strip_prefix(':')
        .map(str::trim_start)
}

fn field_has_identifier_boundaries(text: &str, offset: usize, len: usize) -> bool {
    let before = offset
        .checked_sub(1)
        .and_then(|index| text.as_bytes().get(index))
        .copied();
    let after = text.as_bytes().get(offset + len).copied();
    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
}

fn contains_identifier(text: &str, identifier: &str) -> bool {
    text.match_indices(identifier).any(|(offset, _)| {
        let before = offset
            .checked_sub(1)
            .and_then(|index| text.as_bytes().get(index))
            .copied();
        let after = text.as_bytes().get(offset + identifier.len()).copied();
        !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
    })
}

fn register_prefix(options: &str) -> Prefix<'_> {
    let trimmed = options.trim_start();
    if !trimmed.starts_with('{') {
        return Prefix::NotObject;
    }
    if let Some(prefix) = object_literal_string_field(trimmed, "prefix") {
        return Prefix::Literal(prefix);
    }
    if contains_identifier(trimmed, "prefix") {
        return Prefix::Dynamic;
    }
    Prefix::Absent
}

fn fastify_plugin_identifier(slice: &str) -> Option<&str> {
    let arguments = call_arguments(slice)?;
    let candidate = arguments.first()?;
    let (plugin, after_plugin) = leading_identifier(candidate)?;
    if candidate[after_plugin..].trim().is_empty() {
        Some(plugin)
    } else {
        None
    }
}

/// The route-call parts of a unit slice, as the text readers saw them.
pub(super) fn route_call(slice: &str) -> Option<CallParts<'_>> {
    let (receiver, method) = route_call_parts(slice)?;
    let arguments = call_arguments(slice);
    Some(CallParts {
        receiver,
        method,
        arity: arguments.as_ref().map(Vec::len),
        path_literal: route_path_literal(slice),
        first_is_string: arguments
            .as_ref()
            .and_then(|arguments| arguments.first())
            .is_some_and(|argument| string_literal_arg(argument)),
        handler: route_handler_identifier(slice),
        plugin: fastify_plugin_identifier(slice),
        prefix: arguments
            .as_ref()
            .and_then(|arguments| arguments.get(1))
            .map_or(Prefix::Absent, |options| register_prefix(options)),
        route_object: RouteObject {
            method: object_literal_string_field(slice, "method"),
            path: object_literal_string_field(slice, "url")
                .or_else(|| object_literal_string_field(slice, "path")),
            has_handler: object_literal_has_field(slice, "handler"),
        },
    })
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
    let (receiver, method) = route_call_parts(line)?;
    if method != "use"
        || unsafe_names.contains(receiver)
        || !matches!(express_receivers.get(receiver), Some(ExpressReceiver::App))
    {
        return None;
    }
    let arguments = call_arguments(line)?;
    let [prefix_arg, router_arg] = arguments.as_slice() else {
        return None;
    };
    if !string_literal_arg(prefix_arg) {
        return None;
    }
    let prefix = first_quoted(prefix_arg)?;
    let (router, after_router) = leading_identifier(router_arg)?;
    if !router_arg[after_router..].trim().is_empty()
        || unsafe_names.contains(router)
        || !matches!(express_receivers.get(router), Some(ExpressReceiver::Router))
    {
        return None;
    }
    Some((router.to_string(), normalize_route_path(prefix)))
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

pub(super) fn table_declaration_parts(slice: &str) -> Option<(&str, &str)> {
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
    if !rhs[after_factory..].trim_start().starts_with('(') {
        return None;
    }
    Some((name, factory))
}

pub(super) fn prisma_query(slice: &str) -> Option<PrismaQuery<'_>> {
    let (client, after_client) = leading_identifier(slice)?;
    let rest = slice[after_client..].trim_start().strip_prefix('.')?;
    let (model, after_model) = leading_identifier(rest)?;
    let rest = rest[after_model..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if !rest[after_operation..].trim_start().starts_with('(') {
        return None;
    }
    Some(PrismaQuery {
        client,
        model,
        operation,
    })
}

pub(super) fn prisma_transaction(slice: &str) -> Option<PrismaTransaction<'_>> {
    let (client, after_client) = leading_identifier(slice)?;
    slice[after_client..]
        .trim_start()
        .strip_prefix(".$transaction(")?;
    Some(PrismaTransaction {
        client,
        array_argument: slice.contains("$transaction(["),
    })
}

pub(super) fn drizzle_query(slice: &str) -> Option<DrizzleQuery<'_>> {
    let (db, after_db) = leading_identifier(slice)?;
    let rest = slice[after_db..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if operation == "query" {
        let rest = rest[after_operation..].trim_start().strip_prefix('.')?;
        let (table, after_table) = leading_identifier(rest)?;
        let rest = rest[after_table..].trim_start().strip_prefix('.')?;
        let (query_operation, after_query_operation) = leading_identifier(rest)?;
        if matches!(query_operation, "findMany" | "findFirst")
            && rest[after_query_operation..].trim_start().starts_with('(')
        {
            return Some(DrizzleQuery {
                db,
                operation: if query_operation == "findMany" {
                    "query_findMany"
                } else {
                    "query_findFirst"
                },
                table,
            });
        }
        return None;
    }
    if !matches!(operation, "select" | "insert" | "update" | "delete")
        || !rest[after_operation..].trim_start().starts_with('(')
    {
        return None;
    }
    if operation == "select" {
        let from_index = slice.find(".from(")?;
        let from_arg = &slice[from_index + ".from(".len()..];
        let (table, _) = leading_identifier(from_arg)?;
        return Some(DrizzleQuery {
            db,
            operation,
            table,
        });
    }
    let arg = &rest[after_operation..].trim_start()["(".len()..];
    let (table, _) = leading_identifier(arg)?;
    Some(DrizzleQuery {
        db,
        operation,
        table,
    })
}

pub(super) fn drizzle_transaction(slice: &str) -> Option<&str> {
    let (db, after_db) = leading_identifier(slice)?;
    slice[after_db..]
        .trim_start()
        .strip_prefix(".transaction(")?;
    Some(db)
}

/// Classifies the declarators and statements as one text string each, the way
/// the pre-slice line reader did.
pub(super) fn receivers(
    text: &str,
    items: &[Item<'_>],
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
    drizzle_table_factories: &BTreeSet<String>,
) -> Receivers {
    let mut found = Receivers::default();
    for item in items {
        let line = match *item {
            Item::Declarator(node) => {
                format!("const {}", super::scope_graph::node_text(text, node))
            }
            Item::Statement(node) => super::scope_graph::node_text(text, node).to_string(),
        };
        if let Some((name, receiver)) = express_receiver_declaration(&line, imports, unsafe_names) {
            if !unsafe_names.contains(&name) {
                found.express.insert(name, receiver);
            }
        }
        if let Some(name) = fastify_receiver_declaration(&line, imports, unsafe_names) {
            if !unsafe_names.contains(&name) {
                found.fastify.insert(name);
            }
        }
        if let Some(name) = hono_receiver_declaration(&line, imports, unsafe_names) {
            if !unsafe_names.contains(&name) {
                found.hono.insert(name);
            }
        }
        if let Some(name) = prisma_client_declaration(&line, imports, unsafe_names) {
            if !unsafe_names.contains(&name) {
                found.prisma.insert(name);
            }
        }
        if let Some((table, factory)) = table_declaration_parts(&line) {
            if drizzle_table_factories.contains(factory) && !unsafe_names.contains(table) {
                found.drizzle_tables.insert(table.to_string());
            }
        }
        if let Some(name) = drizzle_db_declaration(&line, imports, unsafe_names) {
            if !unsafe_names.contains(&name) {
                found.drizzle_dbs.insert(name);
            }
        }
        if let Some((router, prefix)) =
            express_router_prefix_mount(&line, &found.express, unsafe_names)
        {
            found.express_prefixes.entry(router).or_insert(prefix);
        }
    }
    found
}
