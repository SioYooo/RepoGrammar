//! The line-based TS/JS unit extractor, kept as the differential oracle of
//! ADR-0059 slice 3 (D6). Production reads the units from the Tree-sitter tree
//! (`tsjs/units.rs`); this module is compiled for tests only and is unchanged
//! from `main@5097d7a` apart from the helpers it now shares.

use super::syntax::{
    delimiters_are_balanced, first_quoted, first_quoted_after, is_identifier_byte,
    lines_with_offsets, slug, tsjs_import_resolution_facts,
};
use super::tsjs::{is_component_name, is_hook_name, is_react_path, next_file_convention_kind};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{CodeUnit, CodeUnitId, CodeUnitKind, Provenance, SourceRange};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub(super) struct LegacyUnitParser;

impl SourceParser for LegacyUnitParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        let mut scanner = SyntaxScanner::new(document);
        scanner.scan()?;
        scanner.finish()
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        let mut scanner = SyntaxScanner::new_with_context(document, context);
        scanner.scan()?;
        scanner.finish()
    }
}

struct SyntaxScanner<'a> {
    document: SourceDocument<'a>,
    context: Option<&'a ParserProjectContext>,
    units: Vec<CodeUnit>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
}

impl<'a> SyntaxScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        Self {
            document,
            context: None,
            units: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn new_with_context(document: SourceDocument<'a>, context: &'a ParserProjectContext) -> Self {
        Self {
            document,
            context: Some(context),
            units: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn scan(&mut self) -> Result<(), ParseError> {
        self.add_unit(CodeUnitKind::Module, "module", 0, self.document.text.len())?;
        let lines = lines_with_offsets(self.document.text);
        let class_ranges = self.scan_classes(&lines)?;
        self.scan_class_methods(&lines, &class_ranges)?;
        self.scan_top_level_patterns(&lines, &class_ranges)?;
        self.add_delimiter_diagnostic_if_needed();
        Ok(())
    }

    fn finish(mut self) -> Result<ParseReport, ParseError> {
        self.units.sort_by(|left, right| {
            (
                left.range.start_byte,
                left.range.end_byte,
                left.kind.as_str(),
                left.id.as_str(),
            )
                .cmp(&(
                    right.range.start_byte,
                    right.range.end_byte,
                    right.kind.as_str(),
                    right.id.as_str(),
                ))
        });
        let ir_nodes = ir_nodes_for_units(&self.units).map_err(ParseError::Internal)?;
        let ir_edges = ir_edges_for_units(&self.units).map_err(ParseError::Internal)?;
        let mut semantic_facts =
            super::tsjs::exact_framework_anchors(&self.document, &self.units, self.context)?;
        if let Some(context) = self.context {
            semantic_facts.extend(tsjs_import_resolution_facts(
                &self.document,
                &self.units,
                context,
            )?);
        }
        Ok(ParseReport {
            units: self.units,
            ir_nodes,
            ir_edges,
            semantic_facts,
            diagnostics: self.diagnostics,
        })
    }

    fn scan_classes(&mut self, lines: &[(usize, &str)]) -> Result<Vec<(usize, usize)>, ParseError> {
        let mut ranges = Vec::new();
        for (line_start, line) in lines {
            let Some(class_offset) = find_keyword(line, "class") else {
                continue;
            };
            let identifier_start = class_offset + "class".len();
            let Some((name, _)) = parse_identifier_after(line, identifier_start) else {
                continue;
            };
            let class_keyword_start = line_start + class_offset;
            let end = declaration_extent(
                self.document.text,
                class_keyword_start,
                line_start + line.len(),
            );
            let prefix_start = decorator_prefix_start(self.document.text, class_keyword_start);
            let decorator_block = &self.document.text[prefix_start..class_keyword_start];
            let (kind, unit_start) = match nest_class_decorator_kind(decorator_block) {
                Some(kind) => (kind, prefix_start),
                None => (CodeUnitKind::Class, class_keyword_start),
            };
            self.add_unit(kind, &name, unit_start, end)?;
            ranges.push((unit_start, end));
        }
        Ok(ranges)
    }

    fn scan_class_methods(
        &mut self,
        lines: &[(usize, &str)],
        class_ranges: &[(usize, usize)],
    ) -> Result<(), ParseError> {
        for (line_start, line) in lines {
            if !is_inside_any_range(*line_start, class_ranges) {
                continue;
            }
            let Some((name, offset)) = method_name_from_line(line) else {
                continue;
            };
            let method_start = line_start + offset;
            let end = declaration_extent(self.document.text, method_start, line_start + line.len());
            let prefix_start = decorator_prefix_start(self.document.text, method_start);
            let decorator_block = &self.document.text[prefix_start..method_start];
            let (kind, unit_start) = if nest_method_decorator(decorator_block) {
                (CodeUnitKind::NestRoute, prefix_start)
            } else {
                (CodeUnitKind::Method, method_start)
            };
            self.add_unit(kind, &name, unit_start, end)?;
        }
        Ok(())
    }

    fn scan_top_level_patterns(
        &mut self,
        lines: &[(usize, &str)],
        class_ranges: &[(usize, usize)],
    ) -> Result<(), ParseError> {
        let test_runner_names = super::tsjs::exact_test_runner_call_names(&self.document);
        let fastify_receivers = fastify_receivers_for_scan(self.document.text);
        let hono_receivers = hono_receivers_for_scan(self.document.text);
        let zod_bindings = zod_bindings_for_scan(self.document.text);
        let drizzle_table_factories = drizzle_table_factories_for_scan(self.document.text);
        for (line_start, line) in lines {
            let line_end = line_start + line.len();
            if let Some((kind, name, offset)) = next_default_export_unit(self.document.path, line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(kind, name, start, end)?;
            }
            if let Some((method, offset)) = next_route_handler_export(self.document.path, line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::NextRouteHandler, method, start, end)?;
            }
            if let Some((receiver, method, offset)) = static_route_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                if hono_receivers.contains(receiver) {
                    self.add_unit(CodeUnitKind::HonoRoute, method, start, end)?;
                } else if fastify_receivers.contains(receiver) {
                    self.add_unit(CodeUnitKind::FastifyRoute, method, start, end)?;
                } else {
                    self.add_unit(CodeUnitKind::ExpressRoute, method, start, end)?;
                }
            } else if let Some((receiver, offset)) = fastify_full_route_call(line) {
                if fastify_receivers.contains(receiver) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::FastifyRoute, "route", start, end)?;
                }
            } else if let Some((receiver, offset)) = fastify_register_call(line) {
                if fastify_receivers.contains(receiver) {
                    let start = line_start + offset;
                    let end = line_end;
                    self.add_unit(
                        CodeUnitKind::FastifyPluginRegistration,
                        "register",
                        start,
                        end,
                    )?;
                }
            } else if let Some((receiver, offset)) = dynamic_route_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                let kind = if fastify_receivers.contains(receiver) {
                    CodeUnitKind::FastifyRoute
                } else {
                    CodeUnitKind::ExpressRoute
                };
                self.add_unit(kind, "dynamic_route", start, end)?;
            }
            if let Some((name, offset)) = prisma_query_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::PrismaQuery, name, start, end)?;
            }
            if let Some(offset) = prisma_transaction_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::PrismaTransaction, "transaction", start, end)?;
            }
            if let Some((name, offset)) =
                drizzle_schema_table_declaration(line, &drizzle_table_factories)
            {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleSchemaTable, name, start, end)?;
            }
            if let Some((operation, offset)) = drizzle_query_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleQuery, operation, start, end)?;
            }
            if let Some(offset) = drizzle_transaction_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleTransaction, "transaction", start, end)?;
            }
            if let Some((name, offset)) = zod_schema_declaration(line, &zod_bindings) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::ZodSchema, name, start, end)?;
            }
            for suite_name in &test_runner_names.suite_names {
                if let Some(offset) = call_offset(line, suite_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestSuite, suite_name, start, end)?;
                }
            }
            if !test_runner_names.suite_names.contains("describe") {
                if let Some(offset) = call_offset(line, "describe") {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestSuite, "describe", start, end)?;
                }
            }
            for test_name in &test_runner_names.test_names {
                if let Some(offset) = call_offset(line, test_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestCase, test_name, start, end)?;
                }
            }
            for test_name in ["it", "test"] {
                if test_runner_names.test_names.contains(test_name) {
                    continue;
                }
                if let Some(offset) = call_offset(line, test_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestCase, test_name, start, end)?;
                }
            }
            if is_inside_any_range(*line_start, class_ranges) {
                continue;
            }
            if let Some(function_offset) = find_keyword(line, "function") {
                let identifier_start = function_offset + "function".len();
                if let Some((name, _)) = parse_identifier_after(line, identifier_start) {
                    let start = line_start + function_offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    let kind = classify_callable(&self.document, &name, CodeUnitKind::Function);
                    self.add_unit(kind, &name, start, end)?;
                }
            }
            if let Some((name, offset)) = assigned_arrow_name(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                let kind = classify_callable(&self.document, &name, CodeUnitKind::ArrowFunction);
                self.add_unit(kind, &name, start, end)?;
            }
        }
        Ok(())
    }

    fn add_delimiter_diagnostic_if_needed(&mut self) {
        if delimiters_are_balanced(self.document.text) {
            return;
        }
        self.diagnostics.push(ParseDiagnostic {
            path: self.document.path.to_string(),
            range: None,
            severity: ParseDiagnosticSeverity::Warning,
            message: "source has unbalanced delimiters; syntax-only extraction may be partial"
                .to_string(),
        });
    }

    fn add_unit(
        &mut self,
        kind: CodeUnitKind,
        name: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<(), ParseError> {
        let range = SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?;
        let provenance = Provenance::new(
            self.document.path,
            self.document.content_hash.clone(),
            self.document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?;
        let id = CodeUnitId::new(format!(
            "unit:{}#{}:{}:{}-{}:{}",
            self.document.path,
            kind.as_str(),
            slug(name),
            start_byte,
            end_byte,
            self.ordinal
        ))
        .map_err(ParseError::Internal)?;
        self.ordinal += 1;
        self.units.push(CodeUnit {
            id,
            language: self.document.language.clone(),
            kind,
            range,
            provenance,
        });
        Ok(())
    }
}

fn declaration_extent(text: &str, start: usize, fallback_end: usize) -> usize {
    let Some(open_relative) = text[start..].find('{') else {
        return fallback_end;
    };
    let open = start + open_relative;
    matching_closing_brace(text, open).unwrap_or(text.len())
}

fn matching_closing_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in text.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_inside_any_range(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| offset > *start && offset < *end)
}

fn find_keyword(line: &str, keyword: &str) -> Option<usize> {
    line.match_indices(keyword)
        .find(|(offset, _)| has_identifier_boundaries(line, *offset, keyword.len()))
        .map(|(offset, _)| offset)
}

fn has_identifier_boundaries(line: &str, offset: usize, len: usize) -> bool {
    let before = offset
        .checked_sub(1)
        .and_then(|index| line.as_bytes().get(index))
        .copied();
    let after = line.as_bytes().get(offset + len).copied();
    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
}

fn parse_identifier_after(line: &str, offset: usize) -> Option<(String, usize)> {
    let mut cursor = offset;
    let bytes = line.as_bytes();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'*') {
        cursor += 1;
    }
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let start = cursor;
    if !bytes.get(cursor).copied().is_some_and(is_identifier_start) {
        return None;
    }
    cursor += 1;
    while cursor < bytes.len() && bytes[cursor].is_ascii() && is_identifier_byte(bytes[cursor]) {
        cursor += 1;
    }
    Some((line[start..cursor].to_string(), start))
}

fn assigned_arrow_name(line: &str) -> Option<(String, usize)> {
    for keyword in ["const", "let", "var"] {
        let Some(keyword_offset) = find_keyword(line, keyword) else {
            continue;
        };
        let Some((name, name_offset)) =
            parse_identifier_after(line, keyword_offset + keyword.len())
        else {
            continue;
        };
        let after_name = name_offset + name.len();
        let Some(relative_equals) = line[after_name..].find('=') else {
            continue;
        };
        let equals_offset = after_name + relative_equals;
        // Only an assignment whose right-hand side actually begins an arrow
        // function is an arrow binding. The previous `contains("=>")` check
        // misclassified assignments like `const ids = items.map(x => x.id)` as a
        // function named `ids`.
        if rhs_is_arrow_function_head(&line[equals_offset + 1..]) {
            return Some((name, keyword_offset));
        }
    }
    None
}

/// True when `rhs` (the text after an assignment `=`) begins an arrow function:
/// an optional `async`, then either a parenthesized parameter list whose
/// matching close paren is immediately followed by `=>`, or a single identifier
/// parameter immediately followed by `=>`.
fn rhs_is_arrow_function_head(rhs: &str) -> bool {
    let rhs = rhs.trim_start();
    let rhs = match rhs.strip_prefix("async") {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_whitespace() || c == '(') => {
            rest.trim_start()
        }
        _ => rhs,
    };
    let bytes = rhs.as_bytes();
    if bytes.first() == Some(&b'(') {
        let mut depth = 0usize;
        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return rhs[index + 1..].trim_start().starts_with("=>");
                    }
                }
                _ => {}
            }
        }
        false
    } else if bytes.first().copied().is_some_and(is_identifier_start) {
        let mut cursor = 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii() && is_identifier_byte(bytes[cursor])
        {
            cursor += 1;
        }
        rhs[cursor..].trim_start().starts_with("=>")
    } else {
        false
    }
}

fn method_name_from_line(line: &str) -> Option<(String, usize)> {
    let trimmed_start = line.find(|character: char| !character.is_whitespace())?;
    let trimmed = &line[trimmed_start..];
    if trimmed.starts_with("//")
        || trimmed.starts_with('*')
        || trimmed.starts_with("function ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("switch ")
        || trimmed.starts_with("catch ")
        || trimmed.starts_with("return ")
    {
        return None;
    }
    let mut cursor = 0usize;
    for modifier in ["public ", "private ", "protected ", "static ", "async "] {
        if trimmed[cursor..].starts_with(modifier) {
            cursor += modifier.len();
        }
    }
    if trimmed[cursor..].starts_with('#') {
        cursor += 1;
    }
    let (name, name_offset) = parse_identifier_after(trimmed, cursor)?;
    let after_name = name_offset + name.len();
    let rest = &trimmed[after_name..];
    let paren_offset = rest.find('(')? + after_name;
    let equals_before_paren = trimmed[..paren_offset].contains('=');
    if equals_before_paren || trimmed[paren_offset..].contains("=>") {
        return None;
    }
    Some((name, trimmed_start + name_offset))
}

fn static_route_call(line: &str) -> Option<(&str, &'static str, usize)> {
    for method in [
        "get", "head", "post", "put", "delete", "options", "patch", "all", "use",
    ] {
        let pattern = format!(".{method}(");
        if let Some(method_dot) = line.find(&pattern) {
            let start = line[..method_dot]
                .rfind(|character: char| character.is_whitespace())
                .map(|offset| offset + 1)
                .unwrap_or(0);
            let receiver = line[start..method_dot].trim();
            if !receiver.is_empty() {
                return Some((receiver, method, start));
            }
        }
    }
    None
}

fn fastify_full_route_call(line: &str) -> Option<(&str, usize)> {
    let pattern = ".route(";
    let method_dot = line.find(pattern)?;
    let start = line[..method_dot]
        .rfind(|character: char| character.is_whitespace())
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let receiver = line[start..method_dot].trim();
    (!receiver.is_empty()).then_some((receiver, start))
}

fn fastify_register_call(line: &str) -> Option<(&str, usize)> {
    let pattern = ".register(";
    let method_dot = line.find(pattern)?;
    let start = line[..method_dot]
        .rfind(|character: char| character.is_whitespace())
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let receiver = line[start..method_dot].trim();
    (!receiver.is_empty()).then_some((receiver, start))
}

fn dynamic_route_call(line: &str) -> Option<(&str, usize)> {
    let bracket = line.find('[')?;
    let close = line[bracket + 1..].find(']')? + bracket + 1;
    if !line[close + 1..].trim_start().starts_with('(') {
        return None;
    }
    if !line[close + 1..].contains('"') && !line[close + 1..].contains('\'') {
        return None;
    }
    let start = line[..bracket]
        .rfind(|character: char| character.is_whitespace())
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let receiver = line[start..bracket].trim();
    if receiver.is_empty()
        || !receiver.chars().next().is_some_and(|character| {
            character.is_ascii_alphabetic() || character == '_' || character == '$'
        })
    {
        return None;
    }
    Some((receiver, start))
}

fn fastify_receivers_for_scan(text: &str) -> BTreeSet<String> {
    let mut factories = BTreeSet::new();
    let mut receivers = BTreeSet::new();
    for line in top_level_lines(text) {
        if let Some(name) = imported_default_or_require_name(line, "fastify") {
            factories.insert(name.to_string());
        }
        if line.contains("import { fastify") && line.contains(" from ") && line.contains("fastify")
        {
            factories.insert("fastify".to_string());
        }
        if line.contains("import { Fastify") && line.contains(" from ") && line.contains("fastify")
        {
            factories.insert("Fastify".to_string());
        }
        for name in named_import_or_require_names(line, "fastify", &["fastify", "Fastify"]) {
            factories.insert(name);
        }
    }
    for line in top_level_lines(text) {
        let Some((name, rhs)) = declaration_assignment(line) else {
            continue;
        };
        if factories
            .iter()
            .any(|factory| exact_call_head(rhs, factory.as_str()))
        {
            receivers.insert(name.to_string());
        }
    }
    receivers
}

fn hono_receivers_for_scan(text: &str) -> BTreeSet<String> {
    let mut factories = BTreeSet::new();
    for line in top_level_lines(text) {
        for name in named_import_or_require_names(line, "hono", &["Hono"]) {
            factories.insert(name);
        }
    }
    let mut receivers = BTreeSet::new();
    for line in top_level_lines(text) {
        let Some((name, rhs)) = declaration_assignment(line) else {
            continue;
        };
        let rhs = rhs.trim().trim_end_matches(';').trim();
        let Some(after_new) = rhs.strip_prefix("new ") else {
            continue;
        };
        let Some((constructor, constructor_start)) = parse_identifier_after(after_new, 0) else {
            continue;
        };
        if !factories.contains(&constructor) {
            continue;
        }
        let after_constructor = after_new[constructor_start + constructor.len()..].trim_start();
        // Accept `new Hono()` and `new Hono<Env>()`.
        let after_generic = match after_constructor.strip_prefix('<') {
            Some(rest) => match rest.find('>') {
                Some(index) => rest[index + 1..].trim_start(),
                None => continue,
            },
            None => after_constructor,
        };
        if after_generic.starts_with('(') {
            receivers.insert(name.to_string());
        }
    }
    receivers
}

fn zod_bindings_for_scan(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in top_level_lines(text) {
        for module in ["zod", "zod/v4"] {
            for name in named_import_or_require_names(line, module, &["z"]) {
                names.insert(name);
            }
            if let Some(name) = imported_default_or_require_name(line, module) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn zod_schema_declaration<'a>(
    line: &'a str,
    zod_bindings: &BTreeSet<String>,
) -> Option<(&'a str, usize)> {
    let (name, rhs, start) = declaration_assignment_with_offset(line)?;
    let rhs = rhs.trim_start();
    let (head, head_start) = parse_identifier_after(rhs, 0)?;
    if !zod_bindings.contains(&head) {
        return None;
    }
    let after_head = rhs[head_start + head.len()..].trim_start();
    let after_dot = after_head.strip_prefix('.')?;
    let (builder, builder_start) = parse_identifier_after(after_dot, 0)?;
    if !after_dot[builder_start + builder.len()..]
        .trim_start()
        .starts_with('(')
    {
        return None;
    }
    if !matches!(
        builder.as_str(),
        "object" | "union" | "discriminatedUnion" | "enum" | "array"
    ) {
        return None;
    }
    Some((name, start))
}

/// Walk backward from a class/method declaration over its decorator stack
/// (`@Name(...)`, tolerating an intervening `export` keyword and multi-line
/// decorator arguments). Returns the byte offset of the earliest decorator, or
/// `decl_start` when the declaration has no leading decorator.
fn decorator_prefix_start(text: &str, decl_start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut earliest = decl_start;
    let mut cursor = decl_start;
    loop {
        while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        if text_ends_with_keyword(text, cursor, "export") {
            cursor -= "export".len();
            while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
                cursor -= 1;
            }
        }
        if cursor == 0 || bytes[cursor - 1] != b')' {
            break;
        }
        let Some(open) = match_paren_backward(text, cursor - 1) else {
            break;
        };
        let mut name_end = open;
        while name_end > 0 && bytes[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0 && is_identifier_byte(bytes[name_start - 1]) {
            name_start -= 1;
        }
        if name_start == name_end || name_start == 0 || bytes[name_start - 1] != b'@' {
            break;
        }
        earliest = name_start - 1;
        cursor = name_start - 1;
    }
    earliest
}

fn text_ends_with_keyword(text: &str, offset: usize, keyword: &str) -> bool {
    // `get` refuses a start inside a multi-byte character instead of panicking.
    let Some(before) = offset.checked_sub(keyword.len()) else {
        return false;
    };
    if text.get(before..offset) != Some(keyword) {
        return false;
    }
    before == 0 || !is_identifier_byte(text.as_bytes()[before - 1])
}

fn match_paren_backward(text: &str, close: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(close) != Some(&b')') {
        return None;
    }
    let mut depth = 0usize;
    let mut index = close;
    loop {
        match bytes[index] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        if index == 0 {
            return None;
        }
        index -= 1;
    }
}

fn nest_class_decorator_kind(block: &str) -> Option<CodeUnitKind> {
    decorator_identifiers(block)
        .into_iter()
        .find_map(|name| match name {
            "Controller" => Some(CodeUnitKind::NestController),
            "Injectable" => Some(CodeUnitKind::NestInjectable),
            "Module" => Some(CodeUnitKind::NestModule),
            _ => None,
        })
}

fn nest_method_decorator(block: &str) -> bool {
    decorator_identifiers(block).iter().any(|name| {
        matches!(
            *name,
            "Get" | "Post" | "Put" | "Delete" | "Patch" | "Head" | "Options" | "All"
        )
    })
}

fn decorator_identifiers(block: &str) -> Vec<&str> {
    let bytes = block.as_bytes();
    let mut names = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'@' {
            if let Some((name, name_start)) = parse_identifier_after(block, index + 1) {
                if name_start == index + 1 {
                    names.push(&block[name_start..name_start + name.len()]);
                }
            }
        }
        index += 1;
    }
    names
}

fn drizzle_table_factories_for_scan(text: &str) -> BTreeSet<String> {
    let mut factories = ["pgTable", "mysqlTable", "sqliteTable"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    for line in top_level_lines(text) {
        for module in [
            "drizzle-orm/pg-core",
            "drizzle-orm/mysql-core",
            "drizzle-orm/sqlite-core",
        ] {
            for name in named_import_or_require_names(
                line,
                module,
                &["pgTable", "mysqlTable", "sqliteTable"],
            ) {
                factories.insert(name);
            }
        }
    }
    factories
}

fn next_default_export_unit(path: &str, line: &str) -> Option<(CodeUnitKind, &'static str, usize)> {
    let kind = next_file_convention_kind(path)?;
    let offset = line.find("export default")?;
    let rest = line[offset..].trim_start();
    match kind {
        CodeUnitKind::NextAppPage | CodeUnitKind::NextAppLayout | CodeUnitKind::NextPagesPage => {
            if rest.contains("function") || rest.contains("=>") || rest.contains("createElement") {
                Some((kind, "default", offset))
            } else {
                None
            }
        }
        CodeUnitKind::NextPagesApiRoute => {
            if rest.contains("function") || rest.contains("=>") {
                Some((kind, "handler", offset))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn next_route_handler_export(path: &str, line: &str) -> Option<(&'static str, usize)> {
    if next_file_convention_kind(path) != Some(CodeUnitKind::NextRouteHandler) {
        return None;
    }
    let offset = line.find("export ")?;
    let trimmed = line[offset..].trim_start();
    for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
        let function_pattern = format!("function {method}");
        if trimmed.contains(&function_pattern) {
            return Some((method, offset));
        }
        if exported_const_async_route_handler(trimmed, method) {
            return Some((method, offset));
        }
    }
    None
}

fn exported_const_async_route_handler(line: &str, method: &str) -> bool {
    let Some(after_export) = line.trim_start().strip_prefix("export ") else {
        return false;
    };
    let Some(after_const) = after_export.trim_start().strip_prefix("const ") else {
        return false;
    };
    let Some((name, name_offset)) = parse_identifier_after(after_const, 0) else {
        return false;
    };
    if name != method {
        return false;
    }
    let after_name = name_offset + name.len();
    let Some(rhs) = after_const[after_name..]
        .trim_start()
        .strip_prefix('=')
        .map(str::trim_start)
    else {
        return false;
    };
    rhs.starts_with("async ") && rhs.contains("=>")
}

fn prisma_query_call(line: &str) -> Option<(&'static str, usize)> {
    for pattern in [
        ".$queryRaw(",
        ".$executeRaw(",
        ".$queryRawUnsafe(",
        ".$executeRawUnsafe(",
    ] {
        if let Some(offset) = receiver_offset_before_pattern(line, pattern) {
            return Some(("raw", offset));
        }
    }
    for operation in [
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
    ] {
        if let Some(offset) = model_operation_receiver_offset(line, operation) {
            return Some((operation, offset));
        }
    }
    None
}

fn prisma_transaction_call(line: &str) -> Option<usize> {
    receiver_offset_before_pattern(line, ".$transaction(")
}

fn drizzle_schema_table_declaration<'a>(
    line: &'a str,
    table_factories: &BTreeSet<String>,
) -> Option<(&'a str, usize)> {
    let (name, rhs, start) = declaration_assignment_with_offset(line)?;
    let rhs = rhs.trim_start();
    let (factory, factory_start) = parse_identifier_after(rhs, 0)?;
    let after_factory = factory_start + factory.len();
    if table_factories.contains(&factory) && rhs[after_factory..].trim_start().starts_with('(') {
        Some((name, start))
    } else {
        None
    }
}

fn drizzle_query_call(line: &str) -> Option<(&'static str, usize)> {
    for (operation, target) in [
        ("findMany", "query_findMany"),
        ("findFirst", "query_findFirst"),
    ] {
        if let Some(offset) = drizzle_query_relation_offset(line, operation) {
            return Some((target, offset));
        }
    }
    if let Some(offset) = receiver_offset_before_pattern(line, ".execute(") {
        if !receiver_is_chained(line, offset) {
            return Some(("execute", offset));
        }
    }
    for operation in ["select", "insert", "update", "delete"] {
        let pattern = format!(".{operation}(");
        let Some(offset) = receiver_offset_before_pattern(line, &pattern) else {
            continue;
        };
        if receiver_is_chained(line, offset) {
            continue;
        }
        if operation != "select" || line[offset..].contains(".from(") {
            if operation != "select" && call_first_argument_starts_with_quote(&line[offset..]) {
                continue;
            }
            return Some((operation, offset));
        }
    }
    None
}

fn drizzle_transaction_call(line: &str) -> Option<usize> {
    receiver_offset_before_pattern(line, ".transaction(")
}

fn drizzle_query_relation_offset(line: &str, operation: &str) -> Option<usize> {
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(".query.")?;
        let query_dot = search_offset + relative;
        let (_, db_start) = identifier_before_dot(line, query_dot)?;
        let after_query = &line[query_dot + ".query.".len()..];
        let (table, table_offset) = parse_identifier_after(after_query, 0)?;
        let after_table = table_offset + table.len();
        let Some(rest) = after_query[after_table..].trim_start().strip_prefix('.') else {
            search_offset = query_dot + ".query.".len();
            continue;
        };
        if rest.starts_with(operation) && rest[operation.len()..].trim_start().starts_with('(') {
            return Some(db_start);
        }
        search_offset = query_dot + ".query.".len();
    }
    None
}

fn model_operation_receiver_offset(line: &str, operation: &str) -> Option<usize> {
    let pattern = format!(".{operation}(");
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(&pattern)?;
        let operation_dot = search_offset + relative;
        let (_, model_start) = identifier_before_dot(line, operation_dot)?;
        let model_prefix_end = line[..model_start].trim_end().len();
        if model_prefix_end == 0 || line.as_bytes()[model_prefix_end - 1] != b'.' {
            search_offset = operation_dot + pattern.len();
            continue;
        }
        let client_dot = model_prefix_end - 1;
        if let Some((_, client_start)) = identifier_before_dot(line, client_dot) {
            return Some(client_start);
        }
        search_offset = operation_dot + pattern.len();
    }
    None
}

fn receiver_offset_before_pattern(line: &str, pattern: &str) -> Option<usize> {
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(pattern)?;
        let dot_offset = search_offset + relative;
        if let Some((_, receiver_start)) = identifier_before_dot(line, dot_offset) {
            return Some(receiver_start);
        }
        search_offset = dot_offset + pattern.len();
    }
    None
}

fn receiver_is_chained(line: &str, receiver_start: usize) -> bool {
    let prefix_end = line[..receiver_start].trim_end().len();
    prefix_end > 0 && line.as_bytes()[prefix_end - 1] == b'.'
}

fn call_first_argument_starts_with_quote(call: &str) -> bool {
    let Some(open) = call.find('(') else {
        return false;
    };
    let after_open = call[open + 1..].trim_start();
    after_open.starts_with('"') || after_open.starts_with('\'')
}

fn identifier_before_dot(line: &str, dot_offset: usize) -> Option<(&str, usize)> {
    if line.as_bytes().get(dot_offset) != Some(&b'.') {
        return None;
    }
    let mut end = dot_offset;
    let bytes = line.as_bytes();
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
    Some((&line[start..end], start))
}

fn top_level_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut depth: i64 = 0;
    for line in text.lines() {
        if depth == 0 {
            lines.push(line);
        }
        depth += line.bytes().fold(0, |delta, byte| match byte {
            b'{' => delta + 1,
            b'}' => delta - 1,
            _ => delta,
        });
        if depth < 0 {
            depth = 0;
        }
    }
    lines
}

fn imported_default_or_require_name<'a>(line: &'a str, module: &str) -> Option<&'a str> {
    let trimmed = line.trim();
    let import_suffix = format!(" from \"{module}\"");
    let import_suffix_single = format!(" from '{module}'");
    if let Some(rest) = trimmed.strip_prefix("import ") {
        if rest.contains(&import_suffix) || rest.contains(&import_suffix_single) {
            let before_from = rest.split(" from ").next()?.trim();
            if !before_from.starts_with('{') && !before_from.starts_with('*') {
                return before_from.split(',').next().map(str::trim);
            }
        }
    }
    let require_double = format!("require(\"{module}\")");
    let require_single = format!("require('{module}')");
    if (trimmed.contains(&require_double) || trimmed.contains(&require_single))
        && trimmed.starts_with("const ")
    {
        return declaration_assignment(trimmed).map(|(name, _)| name);
    }
    None
}

fn named_import_or_require_names(line: &str, module: &str, exported_names: &[&str]) -> Vec<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("import ") {
        if let Some(from_index) = rest.find(" from ") {
            let clause = rest[..from_index].trim();
            let quoted_module = first_quoted(&rest[from_index + " from ".len()..]);
            if quoted_module.as_deref() == Some(module) {
                return braced_binding_names(clause, false, exported_names);
            }
        }
    }
    let Some(module_name) = first_quoted_after(trimmed, "require(") else {
        return Vec::new();
    };
    if module_name != module {
        return Vec::new();
    }
    let Some(after_keyword) = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))
    else {
        return Vec::new();
    };
    let Some(equals) = after_keyword.find('=') else {
        return Vec::new();
    };
    braced_binding_names(after_keyword[..equals].trim(), true, exported_names)
}

fn braced_binding_names(
    clause: &str,
    allow_colon_alias: bool,
    exported_names: &[&str],
) -> Vec<String> {
    let Some(open) = clause.find('{') else {
        return Vec::new();
    };
    let Some(close_relative) = clause[open..].find('}') else {
        return Vec::new();
    };
    let inner = &clause[open + 1..open + close_relative];
    let mut names = Vec::new();
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
        let Some((original, _)) = parse_identifier_after(original, 0) else {
            continue;
        };
        if !exported_names.contains(&original.as_str()) {
            continue;
        }
        let local = local.split('=').next().unwrap_or(local).trim();
        if let Some((local, _)) = parse_identifier_after(local, 0) {
            names.push(local);
        }
    }
    names
}

fn declaration_assignment(line: &str) -> Option<(&str, &str)> {
    declaration_assignment_with_offset(line).map(|(name, rhs, _)| (name, rhs))
}

fn declaration_assignment_with_offset(line: &str) -> Option<(&str, &str, usize)> {
    let trimmed_start = line
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(0);
    let mut trimmed = &line[trimmed_start..];
    let mut prefix_len = 0usize;
    if let Some(rest) = trimmed.strip_prefix("export ") {
        prefix_len += "export ".len();
        trimmed = rest.trim_start();
    }
    for keyword in ["const ", "let ", "var "] {
        let Some(rest) = trimmed.strip_prefix(keyword) else {
            continue;
        };
        let start = trimmed_start + prefix_len;
        let (name, name_offset) = parse_identifier_after(rest, 0)?;
        let equals = rest[name_offset + name.len()..].find('=')? + name_offset + name.len();
        let rhs = rest[equals + 1..].trim();
        return Some((&rest[name_offset..name_offset + name.len()], rhs, start));
    }
    None
}

fn exact_call_head(rhs: &str, head: &str) -> bool {
    let rhs = rhs.trim().trim_end_matches(';').trim();
    rhs.starts_with(head) && rhs[head.len()..].trim_start().starts_with('(')
}

/// Offset of a bare call to `function_name` on this line, if any.
///
/// A member call is not a bare call. `test.describe(...)`, `suite.it(...)`, and
/// `runner?.test(...)` all read the runner name off some object, and this
/// scanner's whole gate is the import binding of a bare identifier -- so
/// matching a member access would attribute another object's method to the
/// imported runner. Playwright makes that concrete: its suites are written
/// `test.describe(...)`, which must not be read as an ambient `describe`.
fn call_offset(line: &str, function_name: &str) -> Option<usize> {
    line.match_indices(function_name)
        .find(|(offset, _)| {
            has_identifier_boundaries(line, *offset, function_name.len())
                && !is_member_access(line, *offset)
                && line[*offset + function_name.len()..]
                    .trim_start()
                    .starts_with('(')
        })
        .map(|(offset, _)| offset)
}

fn is_member_access(line: &str, offset: usize) -> bool {
    offset
        .checked_sub(1)
        .and_then(|index| line.as_bytes().get(index))
        .is_some_and(|byte| *byte == b'.')
}

fn classify_callable(
    document: &SourceDocument<'_>,
    name: &str,
    fallback: CodeUnitKind,
) -> CodeUnitKind {
    if is_hook_name(name) {
        return CodeUnitKind::ReactHook;
    }
    if is_component_name(name) && (is_react_path(document.path) || contains_jsx(document.text)) {
        return CodeUnitKind::ReactComponent;
    }
    fallback
}

fn contains_jsx(text: &str) -> bool {
    text.contains("return <") || text.contains("</") || text.contains("/>")
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigned_arrow_name_requires_an_arrow_function_rhs() {
        // Genuine arrow bindings in each parameter form.
        assert_eq!(
            assigned_arrow_name("const handler = (req, res) => {}").map(|(name, _)| name),
            Some("handler".to_string())
        );
        assert_eq!(
            assigned_arrow_name("let identity = x => x").map(|(name, _)| name),
            Some("identity".to_string())
        );
        assert_eq!(
            assigned_arrow_name("var run = async () => {}").map(|(name, _)| name),
            Some("run".to_string())
        );

        // A `=>` that merely appears inside the right-hand side (a callback
        // argument) must not turn the assignment into an arrow function.
        assert_eq!(
            assigned_arrow_name("const ids = items.map(x => x.id);"),
            None
        );
        assert_eq!(
            assigned_arrow_name("const total = rows.reduce((a, b) => a + b, 0);"),
            None
        );
        assert_eq!(assigned_arrow_name("const value = compute();"), None);
    }
}
