//! Test-only legacy line-and-brace reader, kept as the ADR-0059 differential
//! oracle until the maintainer accepts the slice-1 classification.

use super::super::{first_quoted, is_identifier_byte, is_identifier_start, leading_identifier};
use super::{
    push_unsafe_event, strip_export_prefix, ImportBinding, ImportKind, ScopeGraphLite,
    ScopeInterval, UnsafeNameEvent,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyInterval {
    start_byte: usize,
    end_byte: usize,
    depth: usize,
    local_decls: BTreeSet<String>,
    params: BTreeSet<String>,
}

impl ScopeGraphLite {
    pub(super) fn analyze_legacy(text: &str) -> Self {
        let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut unsafe_events: BTreeMap<String, Vec<UnsafeNameEvent>> = BTreeMap::new();
        let mut imports: BTreeMap<String, ImportBinding> = BTreeMap::new();
        let mut local_decls: BTreeSet<String> = BTreeSet::new();
        let mut top_level_lines: Vec<String> = Vec::new();

        let mut depth: i64 = 0;
        for (line_start, raw_line) in lines_with_offsets(text) {
            let at_top_level = depth == 0;
            if at_top_level {
                if let Some(name) = bare_assignment_name(raw_line) {
                    push_unsafe_event(&mut unsafe_events, name, line_start);
                }
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
        let scope_intervals = scope_intervals(text);

        let mut graph = Self {
            imports,
            local_decls,
            unsafe_names,
            unsafe_events,
            scope_intervals,
            ..Self::default()
        };
        graph.fill_receivers(top_level_lines.iter().map(String::as_str));
        graph
    }
}

fn scope_intervals(text: &str) -> Vec<ScopeInterval> {
    let mut intervals = block_scope_intervals(text);
    add_function_parameters_to_intervals(text, &mut intervals);
    add_arrow_parameters_to_intervals(text, &mut intervals);
    add_local_declarations_to_intervals(text, &mut intervals);
    intervals.sort_by_key(|interval| (interval.start_byte, interval.end_byte, interval.depth));
    intervals
        .into_iter()
        .map(|interval| ScopeInterval {
            start_byte: interval.start_byte,
            end_byte: interval.end_byte,
            local_decls: interval.local_decls,
            params: interval.params,
        })
        .collect()
}

fn block_scope_intervals(text: &str) -> Vec<LegacyInterval> {
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut intervals = Vec::new();
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'{' => {
                let depth = stack.len() + 1;
                stack.push((index, depth));
            }
            b'}' => {
                if let Some((start_byte, depth)) = stack.pop() {
                    intervals.push(LegacyInterval {
                        start_byte,
                        end_byte: index + 1,
                        depth,
                        local_decls: BTreeSet::new(),
                        params: BTreeSet::new(),
                    });
                }
            }
            _ => {}
        }
    }
    for (start_byte, depth) in stack {
        intervals.push(LegacyInterval {
            start_byte,
            end_byte: text.len(),
            depth,
            local_decls: BTreeSet::new(),
            params: BTreeSet::new(),
        });
    }
    intervals
}

fn add_function_parameters_to_intervals(text: &str, intervals: &mut [LegacyInterval]) {
    let mut search_start = 0usize;
    while let Some(relative) = text[search_start..].find("function") {
        let function_offset = search_start + relative;
        search_start = function_offset + "function".len();
        if !has_identifier_boundaries(text, function_offset, "function".len()) {
            continue;
        }
        let Some(open_relative) = text[search_start..].find('(') else {
            continue;
        };
        let open = search_start + open_relative;
        let Some(close) = matching_forward_delimiter(text, open, b'(', b')') else {
            continue;
        };
        let Some(body_open_relative) = text[close + 1..].find('{') else {
            continue;
        };
        let body_open = close + 1 + body_open_relative;
        let params = parameter_identifiers(&text[open + 1..close]);
        add_params_to_interval(intervals, body_open, params);
    }
}

fn add_arrow_parameters_to_intervals(text: &str, intervals: &mut [LegacyInterval]) {
    let mut search_start = 0usize;
    while let Some(relative) = text[search_start..].find("=>") {
        let arrow = search_start + relative;
        search_start = arrow + "=>".len();
        let after_arrow = &text[search_start..];
        let body_open_relative = after_arrow
            .bytes()
            .position(|byte| !byte.is_ascii_whitespace());
        let Some(body_open_relative) = body_open_relative else {
            continue;
        };
        if after_arrow.as_bytes().get(body_open_relative) != Some(&b'{') {
            continue;
        }
        let body_open = search_start + body_open_relative;
        let params = arrow_parameter_identifiers_before(text, arrow);
        add_params_to_interval(intervals, body_open, params);
    }
}

fn arrow_parameter_identifiers_before(text: &str, arrow: usize) -> Vec<String> {
    let before_arrow = text[..arrow].trim_end();
    if before_arrow.ends_with(')') {
        let close = before_arrow.len() - 1;
        let Some(open) = matching_backward_delimiter(text, close, b'(', b')') else {
            return Vec::new();
        };
        return parameter_identifiers(&text[open + 1..close]);
    }
    identifier_before_arrow(before_arrow)
        .map(|identifier| vec![identifier.to_string()])
        .unwrap_or_default()
}

fn add_params_to_interval(intervals: &mut [LegacyInterval], body_open: usize, params: Vec<String>) {
    if params.is_empty() {
        return;
    }
    if let Some(interval) = intervals
        .iter_mut()
        .find(|interval| interval.start_byte == body_open)
    {
        interval.params.extend(params);
    }
}

fn add_local_declarations_to_intervals(text: &str, intervals: &mut [LegacyInterval]) {
    for (line_start, line) in lines_with_offsets(text) {
        let line_end = line_start + line.len();
        let mut names = declared_identifiers(line);
        if let Some(name) = bare_assignment_name(line) {
            names.push(name.to_string());
        }
        if names.is_empty() {
            continue;
        }
        names.sort();
        names.dedup();
        for interval in intervals
            .iter_mut()
            .filter(|interval| line_start < interval.end_byte && line_end > interval.start_byte)
        {
            interval.local_decls.extend(names.iter().cloned());
        }
    }
}

fn matching_forward_delimiter(
    text: &str,
    open: usize,
    open_byte: u8,
    close_byte: u8,
) -> Option<usize> {
    if text.as_bytes().get(open) != Some(&open_byte) {
        return None;
    }
    let mut depth = 0usize;
    for (offset, byte) in text.as_bytes()[open..].iter().enumerate() {
        if *byte == open_byte {
            depth += 1;
        } else if *byte == close_byte {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn matching_backward_delimiter(
    text: &str,
    close: usize,
    open_byte: u8,
    close_byte: u8,
) -> Option<usize> {
    if text.as_bytes().get(close) != Some(&close_byte) {
        return None;
    }
    let mut depth = 0usize;
    for index in (0..=close).rev() {
        let byte = text.as_bytes()[index];
        if byte == close_byte {
            depth += 1;
        } else if byte == open_byte {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
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
        return parse_object_binding_specifiers(lhs)
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
    parse_braced_specifiers(clause, false)
}

fn parse_object_binding_specifiers(clause: &str) -> Vec<(String, String)> {
    parse_braced_specifiers(clause, true)
}

fn parse_braced_specifiers(clause: &str, allow_colon_alias: bool) -> Vec<(String, String)> {
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
        let (original, local) = if allow_colon_alias {
            match part.split_once(':') {
                Some((original, local)) => (original.trim(), local.trim()),
                None => (part, part),
            }
        } else {
            match part.split_once(" as ") {
                Some((original, local)) => (original.trim(), local.trim()),
                None => (part, part),
            }
        };
        let local = local.split('=').next().unwrap_or(local).trim();
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

fn declared_identifiers(line: &str) -> Vec<String> {
    let trimmed = strip_export_prefix(line.trim());
    for keyword in ["const ", "let ", "var "] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim_start();
            if rest.starts_with('{') {
                return parse_object_binding_specifiers(rest)
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

fn has_identifier_boundaries(line: &str, offset: usize, len: usize) -> bool {
    let before = offset
        .checked_sub(1)
        .and_then(|index| line.as_bytes().get(index))
        .copied();
    let after = line.as_bytes().get(offset + len).copied();
    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
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
