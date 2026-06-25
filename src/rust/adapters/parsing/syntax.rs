//! Dependency-free syntax-only TS/JS code-unit extraction.
//!
//! This adapter is a bootstrap parser boundary. It emits structural code-unit
//! candidates and diagnostics only; it does not provide semantic certainty.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{CodeUnit, CodeUnitId, CodeUnitKind, Language, Provenance, SourceRange};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, SourceDocument, SourceParser,
};

#[derive(Debug, Default)]
pub struct SyntaxCodeUnitParser;

impl SourceParser for SyntaxCodeUnitParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        if !matches!(
            document.language,
            Language::TypeScript | Language::JavaScript
        ) {
            return Err(ParseError::UnsupportedLanguage);
        }
        let mut scanner = SyntaxScanner::new(document);
        scanner.scan()?;
        scanner.finish()
    }
}

struct SyntaxScanner<'a> {
    document: SourceDocument<'a>,
    units: Vec<CodeUnit>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
}

impl<'a> SyntaxScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        Self {
            document,
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
        Ok(ParseReport {
            units: self.units,
            ir_nodes,
            ir_edges,
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
            let start = line_start + class_offset;
            let end = declaration_extent(self.document.text, start, line_start + line.len());
            self.add_unit(CodeUnitKind::Class, &name, start, end)?;
            ranges.push((start, end));
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
            let start = line_start + offset;
            let end = declaration_extent(self.document.text, start, line_start + line.len());
            self.add_unit(CodeUnitKind::Method, &name, start, end)?;
        }
        Ok(())
    }

    fn scan_top_level_patterns(
        &mut self,
        lines: &[(usize, &str)],
        class_ranges: &[(usize, usize)],
    ) -> Result<(), ParseError> {
        for (line_start, line) in lines {
            let line_end = line_start + line.len();
            if let Some((method, offset)) = express_route_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::ExpressRoute, method, start, end)?;
            }
            if let Some(offset) = call_offset(line, "describe") {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::TestSuite, "describe", start, end)?;
            }
            for test_name in ["it", "test"] {
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
        let equals_offset = line[name_offset + name.len()..].find('=')? + name_offset + name.len();
        if line[equals_offset..].contains("=>") {
            return Some((name, keyword_offset));
        }
    }
    None
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

fn express_route_call(line: &str) -> Option<(&'static str, usize)> {
    for method in ["get", "post", "put", "patch", "delete", "use"] {
        let pattern = format!(".{method}(");
        if let Some(method_dot) = line.find(&pattern) {
            let start = line[..method_dot]
                .rfind(|character: char| character.is_whitespace())
                .map(|offset| offset + 1)
                .unwrap_or(0);
            return Some((method, start));
        }
    }
    None
}

fn call_offset(line: &str, function_name: &str) -> Option<usize> {
    line.match_indices(function_name)
        .find(|(offset, _)| {
            has_identifier_boundaries(line, *offset, function_name.len())
                && line[*offset + function_name.len()..]
                    .trim_start()
                    .starts_with('(')
        })
        .map(|(offset, _)| offset)
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

fn is_hook_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("use") else {
        return false;
    };
    rest.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

fn is_react_path(path: &str) -> bool {
    path.ends_with(".tsx") || path.ends_with(".jsx")
}

fn contains_jsx(text: &str) -> bool {
    text.contains("return <") || text.contains("</") || text.contains("/>")
}

fn delimiters_are_balanced(text: &str) -> bool {
    let mut braces = 0isize;
    let mut parentheses = 0isize;
    let mut brackets = 0isize;
    for byte in text.bytes() {
        match byte {
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            _ => {}
        }
        if braces < 0 || parentheses < 0 || brackets < 0 {
            return false;
        }
    }
    braces == 0 && parentheses == 0 && brackets == 0
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if is_identifier_byte(byte) {
            output.push(byte as char);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    output.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, IrEdgeLabel, IrNodeId, RepositoryRevision};

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: if path.ends_with(".js") || path.ends_with(".jsx") {
                Language::JavaScript
            } else {
                Language::TypeScript
            },
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    #[test]
    fn extracts_structural_ts_js_units_in_deterministic_order() {
        let text = r#"import express from "express";
const app = express();
app.get("/users", async (req, res) => {
  res.json([]);
});
function useUsers() {
  return [];
}
export function UserList() {
  return <section />;
}
const loadUsers = async () => {
  return [];
};
class UserService {
  async findAll() {
    return [];
  }
}
describe("users", () => {
  it("loads", () => {});
  test("filters", () => {});
});
"#;

        let first = SyntaxCodeUnitParser
            .parse(document("src/users.tsx", text))
            .expect("parse");
        let second = SyntaxCodeUnitParser
            .parse(document("src/users.tsx", text))
            .expect("parse");

        let kinds = first
            .units
            .iter()
            .map(|unit| unit.kind.as_str())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"module"));
        assert!(kinds.contains(&"express_route"));
        assert!(kinds.contains(&"react_hook"));
        assert!(kinds.contains(&"react_component"));
        assert!(kinds.contains(&"arrow_function"));
        assert!(kinds.contains(&"class"));
        assert!(kinds.contains(&"method"));
        assert!(kinds.contains(&"test_suite"));
        assert!(kinds.contains(&"test_case"));
        assert_eq!(
            first
                .units
                .iter()
                .map(|unit| unit.id.as_str().to_string())
                .collect::<Vec<_>>(),
            second
                .units
                .iter()
                .map(|unit| unit.id.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(first.ir_nodes.len(), first.units.len());
        assert_eq!(
            first
                .ir_nodes
                .iter()
                .map(|node| node.id.as_str().to_string())
                .collect::<Vec<_>>(),
            second
                .ir_nodes
                .iter()
                .map(|node| node.id.as_str().to_string())
                .collect::<Vec<_>>()
        );
        let route = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::ExpressRoute)
            .expect("route unit");
        assert_eq!(
            route.range.start_byte,
            text.find("app.get").expect("route start")
        );
        assert!(route.range.end_byte <= text.len());
        assert_eq!(route.provenance.path, "src/users.tsx");
        let module = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Module)
            .expect("module unit");
        let class = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Class)
            .expect("class unit");
        let method = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Method)
            .expect("method unit");
        let module_id = IrNodeId::for_code_unit(&module.id).expect("module IR id");
        let route_id = IrNodeId::for_code_unit(&route.id).expect("route IR id");
        let class_id = IrNodeId::for_code_unit(&class.id).expect("class IR id");
        let method_id = IrNodeId::for_code_unit(&method.id).expect("method IR id");
        assert!(first.ir_edges.iter().any(|edge| {
            edge.from_node_id == module_id
                && edge.to_node_id == route_id
                && edge.label == IrEdgeLabel::Contains
        }));
        assert!(first.ir_edges.iter().any(|edge| {
            edge.from_node_id == class_id
                && edge.to_node_id == method_id
                && edge.label == IrEdgeLabel::Contains
        }));
    }

    #[test]
    fn syntax_errors_return_partial_units_with_diagnostics() {
        let text = "export function broken() {\n  return 1;\n";

        let report = SyntaxCodeUnitParser
            .parse(document("src/broken.ts", text))
            .expect("parse partial");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::Module));
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::Function));
        assert_eq!(report.ir_nodes.len(), report.units.len());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].path, "src/broken.ts");
        assert!(report.diagnostics[0].message.contains("unbalanced"));
    }

    #[test]
    fn unsupported_language_is_reported_without_units() {
        let error = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "src/tool.py",
                language: Language::Unknown("python".to_string()),
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: "def tool(): pass\n",
            })
            .expect_err("unsupported language");

        assert_eq!(error, ParseError::UnsupportedLanguage);
    }
}
