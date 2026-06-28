use super::{is_identifier_byte, is_identifier_start, leading_identifier};
use std::collections::{BTreeMap, BTreeSet};

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
    line_depths: Vec<(usize, usize)>,
    pub(super) express_receivers: BTreeMap<String, ExpressReceiver>,
    pub(super) fastify_receivers: BTreeSet<String>,
    pub(super) prisma_clients: BTreeSet<String>,
    pub(super) drizzle_table_factories: BTreeSet<String>,
    pub(super) drizzle_tables: BTreeSet<String>,
    pub(super) drizzle_dbs: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct UnsafeNameEvent {
    byte: usize,
    depth: usize,
}

impl ScopeGraphLite {
    pub(super) fn analyze(text: &str) -> Self {
        let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut unsafe_events: BTreeMap<String, Vec<UnsafeNameEvent>> = BTreeMap::new();
        let mut line_depths = Vec::new();
        let mut imports: BTreeMap<String, ImportBinding> = BTreeMap::new();
        let mut local_decls: BTreeSet<String> = BTreeSet::new();
        let mut top_level_lines: Vec<String> = Vec::new();

        let mut depth: i64 = 0;
        for (line_start, raw_line) in lines_with_offsets(text) {
            let at_top_level = depth == 0;
            let line_depth = depth.max(0) as usize;
            line_depths.push((line_start, line_depth));
            if let Some(name) = bare_assignment_name(raw_line) {
                push_unsafe_event(&mut unsafe_events, name, line_start, line_depth);
            }
            let parameter_depth = if at_top_level && raw_line.contains("function") {
                1
            } else {
                line_depth
            };
            for name in parameter_identifiers_from_line(raw_line) {
                push_unsafe_event(&mut unsafe_events, &name, line_start, parameter_depth);
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
            } else {
                for name in declared_identifiers(raw_line) {
                    push_unsafe_event(&mut unsafe_events, &name, line_start, line_depth);
                }
            }
            depth += brace_delta(raw_line);
            if depth < 0 {
                depth = 0;
            }
        }

        let mut unsafe_names: BTreeSet<String> = BTreeSet::new();
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
                && matches!(&binding.kind, ImportKind::Named(original) if super::drizzle::DRIZZLE_TABLE_FACTORIES.contains(&original.as_str()))
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
            if let Some((table, factory)) = super::drizzle::table_declaration_parts(line) {
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
            unsafe_events,
            line_depths,
            express_receivers,
            fastify_receivers,
            prisma_clients,
            drizzle_table_factories,
            drizzle_tables,
            drizzle_dbs,
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

    pub(super) fn name_is_unsafe_at(&self, name: &str, byte: usize) -> bool {
        if self.unsafe_names.contains(name) {
            return true;
        }
        let unit_depth = self.depth_at(byte);
        self.unsafe_events.get(name).is_some_and(|events| {
            events
                .iter()
                .any(|event| event.byte <= byte && unsafe_event_applies(*event, unit_depth))
        })
    }

    fn depth_at(&self, byte: usize) -> usize {
        self.line_depths
            .iter()
            .rev()
            .find_map(|(start, depth)| (*start <= byte).then_some(*depth))
            .unwrap_or(0)
    }
}

fn unsafe_event_applies(event: UnsafeNameEvent, unit_depth: usize) -> bool {
    event.depth == 0 || (unit_depth > 0 && event.depth <= unit_depth)
}

fn push_unsafe_event(
    unsafe_events: &mut BTreeMap<String, Vec<UnsafeNameEvent>>,
    name: &str,
    byte: usize,
    depth: usize,
) {
    unsafe_events
        .entry(name.to_string())
        .or_default()
        .push(UnsafeNameEvent { byte, depth });
}

fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        lines.push((start, line));
        start += line.len();
    }
    if text.is_empty() {
        lines.push((0, ""));
    }
    lines
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

fn parameter_identifiers_from_line(line: &str) -> Vec<String> {
    let mut identifiers = function_parameter_identifiers(line);
    identifiers.extend(arrow_parameter_identifiers(line));
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

fn function_parameter_identifiers(line: &str) -> Vec<String> {
    let Some(function_offset) = line.find("function") else {
        return Vec::new();
    };
    if !has_identifier_boundaries(line, function_offset, "function".len()) {
        return Vec::new();
    }
    let after_function = &line[function_offset + "function".len()..];
    let Some(open) = after_function.find('(') else {
        return Vec::new();
    };
    let Some(close) = after_function[open + 1..].find(')') else {
        return Vec::new();
    };
    parameter_identifiers(&after_function[open + 1..open + 1 + close])
}

fn has_identifier_boundaries(line: &str, offset: usize, len: usize) -> bool {
    let before = offset
        .checked_sub(1)
        .and_then(|index| line.as_bytes().get(index))
        .copied();
    let after = line.as_bytes().get(offset + len).copied();
    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
}

fn arrow_parameter_identifiers(line: &str) -> Vec<String> {
    let Some(arrow_offset) = line.find("=>") else {
        return Vec::new();
    };
    let before_arrow = line[..arrow_offset].trim_end();
    if before_arrow.ends_with(')') {
        let Some(open) = before_arrow.rfind('(') else {
            return Vec::new();
        };
        return parameter_identifiers(&before_arrow[open + 1..before_arrow.len() - 1]);
    }
    identifier_before_arrow(before_arrow)
        .map(|identifier| vec![identifier.to_string()])
        .unwrap_or_default()
}

fn identifier_before_arrow(text: &str) -> Option<&str> {
    let mut end = text.len();
    let bytes = text.as_bytes();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_identifier_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == end || !is_identifier_start(bytes[start]) {
        return None;
    }
    Some(&text[start..end])
}

fn parameter_identifiers(parameters: &str) -> Vec<String> {
    parameters
        .split(',')
        .filter_map(|parameter| {
            let trimmed = parameter.trim();
            if trimmed.is_empty() || trimmed.starts_with("...") || trimmed.starts_with('{') {
                return None;
            }
            let before_type = trimmed.split(':').next().unwrap_or(trimmed).trim();
            let before_default = before_type.split('=').next().unwrap_or(before_type).trim();
            leading_identifier(before_default).map(|(name, _)| name.to_string())
        })
        .collect()
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
