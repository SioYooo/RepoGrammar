//! Tree-sitter-backed structural Rust code-unit extraction.
//!
//! This adapter does not execute Cargo, rustc, build scripts, macros, or project
//! binaries. It emits structural code units, structural anchors, and typed
//! UNKNOWN facts only.

mod anchors;
mod cargo_manifest;
mod cfg_lattice;
mod module_graph;
mod unknown;

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Language, Provenance, SemanticFact, SourceRange, SymbolId,
};
use crate::core::policy::rust_self_dogfood::rust_self_dogfood_role_for_unit;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use tree_sitter::{Node, Parser};

pub(crate) const RUST_ANCHOR_ENGINE: &str = "repogrammar-rust-syntax";
pub(crate) const RUST_ANCHOR_METHOD: &str = "tree_sitter_rust_structural_anchors_v1";

#[derive(Debug, Default)]
pub struct RustSyntaxParser;

impl SourceParser for RustSyntaxParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        if document.language == Language::RustConfig {
            return cargo_manifest::project_config_report(document);
        }
        if document.language != Language::Rust {
            return Err(ParseError::UnsupportedLanguage);
        }
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|error| ParseError::Internal(format!("load Rust grammar: {error}")))?;
        let Some(tree) = parser.parse(document.text, None) else {
            return Err(ParseError::Internal(
                "Tree-sitter Rust parse failed".to_string(),
            ));
        };

        let mut scanner = RustTreeScanner::new(document, context);
        scanner.scan_tree(tree.root_node())?;
        scanner.finish()
    }
}

struct RustTreeScanner<'a> {
    document: SourceDocument<'a>,
    context: &'a ParserProjectContext,
    units: Vec<CodeUnit>,
    semantic_facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct VisitContext {
    in_impl: bool,
    in_trait: bool,
}

impl<'a> RustTreeScanner<'a> {
    fn new(document: SourceDocument<'a>, context: &'a ParserProjectContext) -> Self {
        Self {
            document,
            context,
            units: Vec::new(),
            semantic_facts: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn scan_tree(&mut self, root: Node<'_>) -> Result<(), ParseError> {
        self.add_unit(
            CodeUnitKind::RustModule,
            "file",
            0,
            self.document.text.len(),
        )?;
        if root.has_error() {
            self.diagnostics.push(ParseDiagnostic {
                path: self.document.path.to_string(),
                range: None,
                severity: ParseDiagnosticSeverity::Warning,
                message: "Tree-sitter Rust parse contains syntax errors; extraction is structural"
                    .to_string(),
            });
        }
        self.visit(root, VisitContext::default())?;
        Ok(())
    }

    fn visit(&mut self, node: Node<'_>, context: VisitContext) -> Result<(), ParseError> {
        let mut next_context = context;
        match node.kind() {
            "mod_item" => {
                self.scan_mod_item(node)?;
                if node_text(self.document.text, node).contains('{') {
                    next_context = VisitContext {
                        in_impl: false,
                        in_trait: false,
                    };
                }
            }
            "use_declaration" => {
                let name = first_identifier_text(self.document.text, node)
                    .unwrap_or_else(|| "use".to_string());
                self.add_unit(
                    CodeUnitKind::RustUseItem,
                    &name,
                    node.start_byte(),
                    node.end_byte(),
                )?;
            }
            "struct_item" => {
                self.add_named_node_unit(node, CodeUnitKind::RustStruct, "struct")?;
            }
            "enum_item" => {
                self.add_named_node_unit(node, CodeUnitKind::RustEnum, "enum")?;
            }
            "trait_item" => {
                self.add_named_node_unit(node, CodeUnitKind::RustTrait, "trait")?;
                next_context = VisitContext {
                    in_impl: false,
                    in_trait: true,
                };
            }
            "impl_item" => {
                let name = impl_name(self.document.text, node);
                self.add_unit(
                    CodeUnitKind::RustImplBlock,
                    &name,
                    node.start_byte(),
                    node.end_byte(),
                )?;
                next_context = VisitContext {
                    in_impl: true,
                    in_trait: false,
                };
            }
            "function_item" => {
                let kind = self.function_kind(node, context);
                self.add_named_node_unit(node, kind, "function")?;
            }
            "function_signature_item" => {
                let kind = if context.in_trait {
                    CodeUnitKind::RustTraitMethod
                } else {
                    CodeUnitKind::RustAssociatedFunction
                };
                self.add_named_node_unit(node, kind, "function_signature")?;
            }
            "macro_invocation" | "macro_definition" => {
                let name = first_identifier_text(self.document.text, node)
                    .unwrap_or_else(|| "macro".to_string());
                let unit = self.add_unit(
                    CodeUnitKind::RustMacroInvocation,
                    &name,
                    node.start_byte(),
                    node.end_byte(),
                )?;
                self.semantic_facts.push(cfg_lattice::macro_unknown_fact(
                    &self.document,
                    &unit,
                    node.start_byte(),
                    node.end_byte(),
                    node.kind(),
                )?);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, next_context)?;
        }
        Ok(())
    }

    fn scan_mod_item(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        let name =
            first_identifier_text(self.document.text, node).unwrap_or_else(|| "mod".to_string());
        let text = node_text(self.document.text, node);
        let is_external = !text.contains('{');
        let kind = if is_external {
            CodeUnitKind::RustExternalModule
        } else {
            CodeUnitKind::RustInlineModule
        };
        let start_byte = if is_external {
            leading_attribute_start_byte(node).unwrap_or(node.start_byte())
        } else {
            node.start_byte()
        };
        let unit = self.add_unit(kind, &name, start_byte, node.end_byte())?;
        if is_external {
            self.semantic_facts.extend(module_graph::resolution_facts(
                &self.document,
                &unit,
                self.context,
                &name,
                node,
            )?);
        }
        Ok(())
    }

    fn function_kind(&self, node: Node<'_>, context: VisitContext) -> CodeUnitKind {
        if has_adjacent_attribute(self.document.text, node, "test") {
            return CodeUnitKind::RustTestFunction;
        }
        if context.in_trait {
            return CodeUnitKind::RustTraitMethod;
        }
        if context.in_impl {
            let text = node_text(self.document.text, node);
            if text.contains("&self")
                || text.contains("&mut self")
                || text.contains("(self")
                || text.contains(" self")
            {
                CodeUnitKind::RustMethod
            } else {
                CodeUnitKind::RustAssociatedFunction
            }
        } else {
            CodeUnitKind::RustFunction
        }
    }

    fn add_named_node_unit(
        &mut self,
        node: Node<'_>,
        kind: CodeUnitKind,
        fallback: &str,
    ) -> Result<CodeUnit, ParseError> {
        let name = node
            .child_by_field_name("name")
            .and_then(|child| node_text_checked(self.document.text, child))
            .map(str::to_string)
            .or_else(|| first_identifier_text(self.document.text, node))
            .unwrap_or_else(|| fallback.to_string());
        let start_byte = leading_attribute_start_byte(node).unwrap_or_else(|| node.start_byte());
        self.add_unit(kind, &name, start_byte, node.end_byte())
    }

    fn add_unit(
        &mut self,
        kind: CodeUnitKind,
        name: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<CodeUnit, ParseError> {
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
        let unit = CodeUnit {
            id,
            language: Language::Rust,
            kind,
            range,
            provenance,
        };
        self.semantic_facts
            .extend(self.anchor_and_unknown_facts_for_unit(&unit)?);
        self.units.push(unit.clone());
        Ok(unit)
    }

    fn anchor_and_unknown_facts_for_unit(
        &self,
        unit: &CodeUnit,
    ) -> Result<Vec<SemanticFact>, ParseError> {
        let mut facts = Vec::new();
        let slice = self
            .document
            .text
            .get(unit.range.start_byte..unit.range.end_byte)
            .unwrap_or("");
        if let Some(role) = rust_self_dogfood_role_for_unit(
            unit.provenance.path.as_str(),
            unit.kind.as_str(),
            unit.id.as_str(),
        ) {
            facts.push(anchors::structural_anchor_fact(
                &self.document,
                unit,
                role.support_target,
                vec![
                    "provider_resolved=false".to_string(),
                    format!("rust_anchor_kind={}", role.anchor_kind),
                    format!("rust_signature_shape={}", rust_signature_shape(slice)),
                    format!("rust_visibility_shape={}", rust_visibility_shape(slice)),
                    format!("rust_arity_shape={}", rust_arity_shape(slice)),
                    format!("rust_return_shape={}", rust_return_shape(slice)),
                    format!("rust_attribute_shape={}", rust_attribute_shape(slice)),
                    format!("rust_error_shape={}", rust_error_shape(slice)),
                    format!("rust_call_shape={}", rust_call_shape(slice)),
                    format!("rust_control_shape={}", rust_control_shape(slice)),
                    format!("rust_test_shape={}", rust_test_shape(slice)),
                    format!(
                        "rust_path_context={}",
                        rust_path_context(&unit.provenance.path)
                    ),
                ],
                "bounded Rust structural role anchor",
            )?);
        }
        facts.extend(cfg_lattice::unit_unknowns(&self.document, unit, slice)?);
        Ok(facts)
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
        self.semantic_facts.sort_by(|left, right| {
            (
                left.evidence.range.start_byte,
                left.evidence.range.end_byte,
                left.kind.as_protocol_str(),
                left.subject.as_str(),
                left.target.as_ref().map(SymbolId::as_str),
            )
                .cmp(&(
                    right.evidence.range.start_byte,
                    right.evidence.range.end_byte,
                    right.kind.as_protocol_str(),
                    right.subject.as_str(),
                    right.target.as_ref().map(SymbolId::as_str),
                ))
        });
        let ir_nodes = ir_nodes_for_units(&self.units).map_err(ParseError::Internal)?;
        let ir_edges = ir_edges_for_units(&self.units).map_err(ParseError::Internal)?;
        Ok(ParseReport {
            units: self.units,
            ir_nodes,
            ir_edges,
            semantic_facts: self.semantic_facts,
            diagnostics: self.diagnostics,
        })
    }
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    node_text_checked(source, node).unwrap_or("")
}

fn node_text_checked<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    source.get(node.start_byte()..node.end_byte())
}

fn first_identifier_text(source: &str, node: Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "type_identifier" | "field_identifier"
        ) {
            return node_text_checked(source, child).map(str::to_string);
        }
        if let Some(value) = first_identifier_text(source, child) {
            return Some(value);
        }
    }
    None
}

fn impl_name(source: &str, node: Node<'_>) -> String {
    let text = node_text(source, node);
    let header = text.split('{').next().unwrap_or(text);
    let compact = header
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join("_");
    if compact.is_empty() {
        "impl".to_string()
    } else {
        compact
    }
}

fn has_adjacent_attribute(source: &str, node: Node<'_>, needle: &str) -> bool {
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if previous.kind() != "attribute_item" {
            break;
        }
        let text = node_text(source, previous);
        if text.contains(needle) {
            return true;
        }
        sibling = previous.prev_named_sibling();
    }
    false
}

fn leading_attribute_start_byte(node: Node<'_>) -> Option<usize> {
    let mut sibling = node.prev_named_sibling();
    let mut start_byte = None;
    while let Some(previous) = sibling {
        if previous.kind() != "attribute_item" {
            break;
        }
        start_byte = Some(previous.start_byte());
        sibling = previous.prev_named_sibling();
    }
    start_byte
}

fn rust_signature_shape(slice: &str) -> String {
    let mut parts = Vec::new();
    let header = slice.split('{').next().unwrap_or(slice);
    let header_tokens = header
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if header_tokens.contains(&"async") && header_tokens.contains(&"fn") {
        parts.push("async");
    }
    if header_tokens.contains(&"unsafe") && header_tokens.contains(&"fn") {
        parts.push("unsafe");
    }
    if header_tokens.contains(&"const") && header_tokens.contains(&"fn") {
        parts.push("const");
    }
    if slice.contains("<") && header.contains('>') {
        parts.push("generic");
    }
    if slice.contains("&mut self") {
        parts.push("receiver_mut_ref");
    } else if slice.contains("&self") {
        parts.push("receiver_ref");
    } else if slice.contains("(self") || slice.contains(" self") {
        parts.push("receiver_value");
    } else {
        parts.push("free_or_associated");
    }
    if header.contains("->") {
        parts.push("returns_value");
    } else {
        parts.push("returns_unit");
    }
    if parts.is_empty() {
        "plain".to_string()
    } else {
        parts.join("_")
    }
}

fn rust_visibility_shape(slice: &str) -> &'static str {
    let header = slice.split('{').next().unwrap_or(slice).trim_start();
    if header.starts_with("pub(") || header.starts_with("pub (") {
        "restricted_public"
    } else if header.starts_with("pub ") || header.contains("\npub ") {
        "public"
    } else {
        "private"
    }
}

fn rust_arity_shape(slice: &str) -> String {
    let header = slice.split('{').next().unwrap_or(slice);
    let Some(start) = header.find('(') else {
        return "arity_unknown".to_string();
    };
    let Some(end) = header[start + 1..].find(')') else {
        return "arity_unknown".to_string();
    };
    let parameters = &header[start + 1..start + 1 + end];
    let arity = parameters
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .count();
    format!("arity_{arity}")
}

fn rust_return_shape(slice: &str) -> &'static str {
    let header = slice.split('{').next().unwrap_or(slice);
    let Some((_, return_type)) = header.split_once("->") else {
        return "unit";
    };
    let return_type = return_type.trim();
    if return_type.starts_with("Result") {
        "result"
    } else if return_type.starts_with("Option") {
        "option"
    } else if matches!(return_type, "bool") {
        "bool"
    } else if matches!(
        return_type,
        "usize"
            | "u64"
            | "u32"
            | "u16"
            | "u8"
            | "isize"
            | "i64"
            | "i32"
            | "i16"
            | "i8"
            | "f64"
            | "f32"
    ) {
        "numeric"
    } else if return_type.contains("String") || return_type.contains("&str") {
        "string"
    } else if return_type.contains("Vec<")
        || return_type.contains("BTree")
        || return_type.contains("Hash")
        || return_type.contains("[]")
    {
        "collection"
    } else if return_type.is_empty() {
        "unknown"
    } else {
        "custom"
    }
}

fn rust_attribute_shape(slice: &str) -> String {
    let mut parts = Vec::new();
    if slice.contains("#[test]") {
        parts.push("test");
    }
    if slice.contains("#[cfg(test)]") {
        parts.push("cfg_test");
    } else if slice.contains("#[cfg(") || slice.contains("#[cfg_attr(") {
        parts.push("cfg");
    }
    if slice.contains("#[derive(") {
        parts.push("derive");
    }
    if slice.contains("#[serde(") {
        parts.push("serde");
    }
    if slice.contains("#[allow(") {
        parts.push("allow");
    }
    if slice.contains("#[proc_macro") {
        parts.push("proc_macro");
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("_")
    }
}

fn rust_error_shape(slice: &str) -> String {
    let mut parts = Vec::new();
    if slice.contains("Result<") || slice.contains("Result <") {
        parts.push("result_return");
    }
    if slice.contains('?') {
        parts.push("question_mark");
    }
    if slice.contains("map_err") {
        parts.push("map_err");
    }
    if slice.contains("unwrap(") {
        parts.push("unwrap");
    }
    if slice.contains("expect(") {
        parts.push("expect");
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join("_")
    }
}

fn rust_call_shape(slice: &str) -> String {
    let mut parts = Vec::new();
    for marker in [
        "record_",
        "validate_",
        "parse_",
        "render_",
        "install",
        "query",
        "family",
    ] {
        if slice.contains(marker) {
            parts.push(marker.trim_end_matches('_'));
        }
    }
    if parts.is_empty() {
        "generic".to_string()
    } else {
        parts.join("_")
    }
}

fn rust_control_shape(slice: &str) -> String {
    let mut parts = Vec::new();
    if slice.contains("match ") {
        parts.push("match");
    }
    if slice.contains("if let") {
        parts.push("if_let");
    } else if slice.contains("if ") {
        parts.push("if");
    }
    if slice.contains("for ") {
        parts.push("for");
    }
    if slice.contains("while ") {
        parts.push("while");
    }
    if parts.is_empty() {
        "straightline".to_string()
    } else {
        parts.join("_")
    }
}

fn rust_test_shape(slice: &str) -> String {
    if !slice.contains("#[test]") {
        return "not_test".to_string();
    }
    let mut parts = vec!["test"];
    for marker in ["assert_eq!", "assert_ne!", "assert_matches!", "assert!"] {
        if slice.contains(marker) {
            parts.push(marker.trim_end_matches('!'));
        }
    }
    parts.join("_")
}

fn rust_path_context(path: &str) -> String {
    if path.contains("/application/") {
        "application".to_string()
    } else if path.contains("/adapters/") {
        "adapters".to_string()
    } else if path.contains("/interfaces/") {
        "interfaces".to_string()
    } else if path.contains("/bin/") {
        "bin".to_string()
    } else if path.contains("/core/") {
        "core".to_string()
    } else if path.contains("/ports/") {
        "ports".to_string()
    } else {
        "repo".to_string()
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

fn toml_key_value(line: &str) -> Option<(&str, &str)> {
    if line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim()))
}

fn toml_string(value: &str) -> Option<String> {
    first_quoted(value)
}

fn first_quoted(text: &str) -> Option<String> {
    let quote_index = text.find(['"', '\''])?;
    let quote = text.as_bytes()[quote_index] as char;
    let rest = &text[quote_index + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "anonymous".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        ContentHash, FactCertainty, IrEdgeLabel, IrNodeId, RepositoryRevision, SemanticFactKind,
    };
    use std::collections::BTreeSet;

    fn document<'a>(path: &'a str, text: &'a str, language: Language) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    #[test]
    fn extracts_rust_units_and_structural_role_anchors() {
        let text = r#"
use crate::ports::parser::SourceParser;

pub struct RustSyntaxParser;

impl SourceParser for RustSyntaxParser {
    fn parse(&self) -> Result<(), String> {
        self.scan()?;
        Ok(())
    }
}

#[test]
fn product_runtime_smoke() {
    assert!(true);
}
"#;
        let report = RustSyntaxParser
            .parse(document(
                "src/rust/adapters/parsing/rust/mod.rs",
                text,
                Language::Rust,
            ))
            .expect("parse Rust");
        let kinds = report
            .units
            .iter()
            .map(|unit| unit.kind.as_str())
            .collect::<BTreeSet<_>>();
        assert!(kinds.contains("rust_module"));
        assert!(kinds.contains("rust_struct"));
        assert!(kinds.contains("rust_impl_block"));
        assert!(kinds.contains("rust_method"));
        assert!(kinds.contains("rust_test_function"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.certainty == FactCertainty::Structural
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "repogrammar.rust.parser_adapter")
        }));
    }

    #[test]
    fn rust_module_contains_rust_children() {
        let text = r#"
pub struct ParserState;

pub fn parse_top_level() {}

impl ParserState {
    pub fn parse_method(&self) {}
}
"#;
        let report = RustSyntaxParser
            .parse(document(
                "src/rust/adapters/parsing/rust/mod.rs",
                text,
                Language::Rust,
            ))
            .expect("parse Rust");
        let root = unit_by_kind(&report, CodeUnitKind::RustModule);
        let function = unit_by_kind(&report, CodeUnitKind::RustFunction);
        let structure = unit_by_kind(&report, CodeUnitKind::RustStruct);
        let impl_block = unit_by_kind(&report, CodeUnitKind::RustImplBlock);
        let method = unit_by_kind(&report, CodeUnitKind::RustMethod);

        assert!(ir_contains(&report, root, function));
        assert!(ir_contains(&report, root, structure));
        assert!(ir_contains(&report, root, impl_block));
        assert!(ir_contains(&report, impl_block, method));
    }

    #[test]
    fn records_cfg_and_macro_unknowns_without_supporting_families() {
        let text = r#"
#[cfg(feature = "nightly")]
fn gated() {}

macro_rules! make_item {
    () => {};
}
"#;
        let report = RustSyntaxParser
            .parse(document(
                "src/rust/application/family.rs",
                text,
                Language::Rust,
            ))
            .expect("parse Rust");
        let reasons = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .filter_map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<BTreeSet<_>>();
        assert!(reasons.contains("BuildVariantAmbiguity"));
        assert!(reasons.contains("MacroOrPreprocessor"));
    }

    #[test]
    fn extracts_nested_traits_impls_attributes_and_signature_shapes() {
        let text = r#"
#[derive(Debug)]
pub struct ParserState<T> {
    value: T,
}

pub trait ParserAdapter {
    fn parse_trait(&self, input: &str) -> Result<(), String>;
}

impl<T> ParserState<T> {
    pub async unsafe fn parse_generic<'a>(&mut self, input: &'a str) -> Result<Option<&'a str>, String> {
        parse_input(input)?;
        Ok(Some(input))
    }
}

mod nested {
    pub fn parse_nested() {}
}

parse_macro!(ParserState);
"#;
        let report = RustSyntaxParser
            .parse(document(
                "src/rust/adapters/parsing/rust/mod.rs",
                text,
                Language::Rust,
            ))
            .expect("parse Rust");
        let kinds = report
            .units
            .iter()
            .map(|unit| unit.kind.as_str())
            .collect::<BTreeSet<_>>();
        assert!(kinds.contains("rust_struct"));
        assert!(kinds.contains("rust_trait"));
        assert!(kinds.contains("rust_impl_block"));
        assert!(kinds.contains("rust_method"));
        assert!(kinds.contains("rust_trait_method"));
        assert!(kinds.contains("rust_inline_module"));
        assert!(kinds.contains("rust_macro_invocation"));

        let parser_facts = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "repogrammar.rust.parser_adapter")
            })
            .collect::<Vec<_>>();
        assert!(parser_facts.iter().any(|fact| fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "rust_attribute_shape=derive")));
        assert!(parser_facts.iter().any(|fact| {
            fact.assumptions.iter().any(|assumption| {
                assumption == "rust_signature_shape=async_unsafe_generic_receiver_mut_ref_returns_value"
            }) && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "rust_visibility_shape=public")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_arity_shape=arity_2")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "rust_return_shape=result")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "MacroOrPreprocessor")
        }));
    }

    #[test]
    fn cargo_toml_is_structural_config_only() {
        let text = r#"
[package]
name = "repogrammar"
build = "build.rs"

[dependencies]
serde_json = "1"

[features]
preview = []
"#;
        let report = RustSyntaxParser
            .parse(document("Cargo.toml", text, Language::RustConfig))
            .expect("parse Cargo");
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert!(report.semantic_facts.iter().any(|fact| fact.kind
            == SemanticFactKind::ProjectConfig
            && fact
                .target
                .as_ref()
                .is_some_and(|target| target.as_str() == "cargo.dependency:serde_json")));
        assert!(report
            .semantic_facts
            .iter()
            .any(|fact| fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "BuildVariantAmbiguity")));
    }

    fn unit_by_kind(report: &ParseReport, kind: CodeUnitKind) -> &CodeUnit {
        report
            .units
            .iter()
            .find(|unit| unit.kind == kind)
            .expect("unit kind exists")
    }

    fn ir_contains(report: &ParseReport, parent: &CodeUnit, child: &CodeUnit) -> bool {
        let from_node_id = IrNodeId::for_code_unit(&parent.id).expect("parent IR node id");
        let to_node_id = IrNodeId::for_code_unit(&child.id).expect("child IR node id");
        report.ir_edges.iter().any(|edge| {
            edge.from_node_id == from_node_id
                && edge.to_node_id == to_node_id
                && edge.label == IrEdgeLabel::Contains
        })
    }
}
