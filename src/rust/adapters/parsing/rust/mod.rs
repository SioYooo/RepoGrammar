//! Tree-sitter-backed structural Rust code-unit extraction.
//!
//! This adapter does not execute Cargo, rustc, build scripts, macros, or project
//! binaries. It emits structural code units, structural anchors, and typed
//! UNKNOWN facts only.

mod anchors;
mod cargo_manifest;
mod cfg_lattice;
mod framework_anchors;
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
use framework_anchors::{AxumRouteOutcome, FrameworkUseContext};
use std::collections::BTreeMap;
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
    use_ctx: FrameworkUseContext,
    let_bindings: BTreeMap<String, String>,
    axum_middleware_emitted: bool,
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
            use_ctx: FrameworkUseContext::default(),
            let_bindings: BTreeMap::new(),
            axum_middleware_emitted: false,
        }
    }

    fn scan_tree(&mut self, root: Node<'_>) -> Result<(), ParseError> {
        self.use_ctx = FrameworkUseContext::from_tree(root, self.document.text);
        self.collect_let_bindings(root);
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
                let unit = self.add_unit(
                    CodeUnitKind::RustUseItem,
                    &name,
                    node.start_byte(),
                    node.end_byte(),
                )?;
                self.semantic_facts
                    .extend(module_graph::use_resolution_facts(
                        &self.document,
                        &unit,
                        node,
                    )?);
            }
            "struct_item" => {
                let kind = self.type_item_kind(node, false);
                self.add_named_node_unit(node, kind, "struct")?;
            }
            "enum_item" => {
                let kind = self.type_item_kind(node, true);
                self.add_named_node_unit(node, kind, "enum")?;
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
                let kind = self.function_item_kind(node, context);
                self.add_named_node_unit(node, kind, "function")?;
            }
            "call_expression" => {
                self.scan_axum_route_call(node)?;
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
            if function_has_self_receiver(node) {
                CodeUnitKind::RustMethod
            } else {
                CodeUnitKind::RustAssociatedFunction
            }
        } else {
            CodeUnitKind::RustFunction
        }
    }

    /// Slice covering a node's leading attributes plus its body, used for
    /// bounded framework-attribute detection.
    fn unit_slice_with_attributes<'b>(&'b self, node: Node<'_>) -> &'b str {
        let start = leading_attribute_start_byte(node).unwrap_or_else(|| node.start_byte());
        self.document.text.get(start..node.end_byte()).unwrap_or("")
    }

    /// Decide a struct/enum kind, promoting to a general framework kind when the
    /// exact serde/thiserror/clap derive shape and use-path evidence are present.
    fn type_item_kind(&self, node: Node<'_>, is_enum: bool) -> CodeUnitKind {
        let slice = self.unit_slice_with_attributes(node);
        framework_anchors::type_kind(slice, is_enum, &self.use_ctx)
    }

    /// Decide a function kind, promoting to a tokio entry/test kind when the
    /// exact `#[tokio::main]`/`#[tokio::test]` attribute (or a bare `#[main]`
    /// with `use tokio::main`) is present.
    fn function_item_kind(&self, node: Node<'_>, context: VisitContext) -> CodeUnitKind {
        let slice = self.unit_slice_with_attributes(node);
        if let Some(kind) = framework_anchors::tokio_function_kind(slice) {
            return kind;
        }
        if let Some(kind) = framework_anchors::tokio_bare_main_kind(slice, &self.use_ctx) {
            return kind;
        }
        self.function_kind(node, context)
    }

    fn collect_let_bindings(&mut self, node: Node<'_>) {
        if node.kind() == "let_declaration" {
            if let (Some(pattern), Some(value)) = (
                node.child_by_field_name("pattern"),
                node.child_by_field_name("value"),
            ) {
                if pattern.kind() == "identifier" {
                    if let (Some(name), Some(text)) = (
                        node_text_checked(self.document.text, pattern),
                        node_text_checked(self.document.text, value),
                    ) {
                        self.let_bindings
                            .entry(name.to_string())
                            .or_insert_with(|| text.to_string());
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.collect_let_bindings(child);
        }
    }

    /// Recognize an axum `.route("literal", verb(handler))` segment on a
    /// `Router::new()`-rooted receiver. Literal + resolved helper + traced
    /// receiver forms an `AxumRoute` unit; any failed gate records a blocking
    /// `rust_axum_route_identity` UNKNOWN scoped to the enclosing file unit.
    fn scan_axum_route_call(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        let outcome = {
            let source = self.document.text;
            let bindings = &self.let_bindings;
            let receiver_traces = node
                .child_by_field_name("function")
                .and_then(|function| function.child_by_field_name("value"))
                .is_some_and(|value| {
                    framework_anchors::receiver_traces_to_router(
                        value,
                        source,
                        &|name| bindings.get(name).cloned(),
                        16,
                    )
                });
            self.maybe_emit_axum_middleware(node, source, receiver_traces)?;
            framework_anchors::analyze_axum_route_call(node, source, &self.use_ctx, receiver_traces)
        };
        match outcome {
            AxumRouteOutcome::Route { name, http_method } => {
                let Some(arguments) = node.child_by_field_name("arguments") else {
                    return Ok(());
                };
                let unit = self.add_unit(
                    CodeUnitKind::AxumRoute,
                    &name,
                    arguments.start_byte(),
                    arguments.end_byte(),
                )?;
                self.semantic_facts
                    .extend(framework_anchors::axum_route_facts(
                        &self.document,
                        &unit,
                        http_method,
                    )?);
            }
            AxumRouteOutcome::Blocked => {
                if let Some(enclosing) = self.units.first().cloned() {
                    self.semantic_facts
                        .push(framework_anchors::axum_route_identity_unknown(
                            &self.document,
                            &enclosing,
                            node.start_byte(),
                            node.end_byte(),
                        )?);
                }
            }
            AxumRouteOutcome::NotRoute => {}
        }
        Ok(())
    }

    fn maybe_emit_axum_middleware(
        &mut self,
        node: Node<'_>,
        source: &str,
        receiver_traces: bool,
    ) -> Result<(), ParseError> {
        if self.axum_middleware_emitted || !receiver_traces {
            return Ok(());
        }
        let is_layer = node
            .child_by_field_name("function")
            .filter(|function| function.kind() == "field_expression")
            .and_then(|function| function.child_by_field_name("field"))
            .and_then(|field| node_text_checked(source, field))
            == Some("layer");
        if !is_layer {
            return Ok(());
        }
        if let Some(enclosing) = self.units.first().cloned() {
            self.semantic_facts
                .push(framework_anchors::axum_middleware_unknown(
                    &self.document,
                    &enclosing,
                    node.start_byte(),
                    node.end_byte(),
                )?);
            self.axum_middleware_emitted = true;
        }
        Ok(())
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
        facts.extend(cfg_lattice::unit_unknowns(
            &self.document,
            unit,
            slice,
            self.context,
        )?);
        facts.extend(framework_anchors::framework_facts_for_unit(
            &self.document,
            unit,
            slice,
            &self.use_ctx,
        )?);
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

/// True when a function's parameter list begins with a `self` receiver. Uses
/// the grammar's `self_parameter` node rather than a substring search over the
/// whole function text, so a `self` token in the body, a comment, or a string
/// literal cannot misclassify an associated function as a method.
fn function_has_self_receiver(node: Node<'_>) -> bool {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = parameters.walk();
    let has_self_receiver = parameters
        .children(&mut cursor)
        .any(|child| child.kind() == "self_parameter");
    has_self_receiver
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
    fn associated_function_with_self_in_body_is_not_a_method() {
        // `make` takes no `self` receiver, so it is an associated function even
        // though its body contains the substring " self" in a string literal.
        let text = r#"
pub struct Widget;

impl Widget {
    pub fn make() -> Self {
        let note = "&self is text here, not a receiver";
        let _ = note;
        Widget
    }
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert_eq!(
            unit_by_kind(&report, CodeUnitKind::RustAssociatedFunction).kind,
            CodeUnitKind::RustAssociatedFunction
        );
        assert!(
            report
                .units
                .iter()
                .all(|unit| unit.kind != CodeUnitKind::RustMethod),
            "associated function with `self` only in its body must not be a method"
        );
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
    fn cfg_unknowns_carry_bounded_cargo_feature_context() {
        let text = r#"
#[cfg(feature = "preview")]
fn declared_feature_gate() {}

#[cfg(feature = "missing")]
fn undeclared_feature_gate() {}
"#;
        let context = ParserProjectContext {
            rust_cargo_files: vec![crate::ports::parser::ParserProjectFileContext {
                path: "Cargo.toml".to_string(),
                text: r#"
[package]
name = "repogrammar"

[features]
preview = []
"#
                .to_string(),
            }],
            ..ParserProjectContext::default()
        };
        let report = RustSyntaxParser
            .parse_with_context(
                document("src/rust/application/family.rs", text, Language::Rust),
                &context,
            )
            .expect("parse Rust");
        let cfg_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact
                        .target
                        .as_ref()
                        .is_some_and(|target| target.as_str() == "BuildVariantAmbiguity")
            })
            .collect::<Vec<_>>();
        assert!(
            cfg_unknowns.len() >= 2,
            "source-level cfgs should remain typed UNKNOWNs: {cfg_unknowns:?}"
        );
        assert!(cfg_unknowns.iter().any(|fact| {
            fact.assumptions
                .contains(&"rust_cfg_feature=preview".to_string())
                && fact
                    .assumptions
                    .contains(&"rust_cfg_feature_declared=preview:true".to_string())
        }));
        assert!(cfg_unknowns.iter().any(|fact| {
            fact.assumptions
                .contains(&"rust_cfg_feature=missing".to_string())
                && fact
                    .assumptions
                    .contains(&"rust_cfg_feature_declared=missing:false".to_string())
        }));
        assert!(cfg_unknowns.iter().all(|fact| {
            fact.assumptions
                .contains(&"rust_cfg_model=cargo_feature_cfg_model".to_string())
                && fact
                    .assumptions
                    .contains(&"rust_cfg_manifest=Cargo.toml".to_string())
                && fact
                    .assumptions
                    .contains(&"rust_cfg_predicate=feature".to_string())
        }));
    }

    #[test]
    fn resolves_repo_local_use_paths_without_treating_external_uses_as_blocking() {
        let text = r#"
use crate::ports::parser;
use super::module_graph;
use self::nested::Item;
use serde::Serialize;
use crate::{alpha, beta};
"#;
        let report = RustSyntaxParser
            .parse(document(
                "src/rust/adapters/parsing/rust/mod.rs",
                text,
                Language::Rust,
            ))
            .expect("parse Rust");

        assert!(has_fact_target(
            &report,
            SemanticFactKind::ResolvedImport,
            "module:crate::ports::parser"
        ));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::ResolvedImport,
            "module:super::module_graph"
        ));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::ResolvedImport,
            "module:self::nested::Item"
        ));
        assert!(!has_fact_target(
            &report,
            SemanticFactKind::ResolvedImport,
            "module:serde::Serialize"
        ));
        assert!(has_unknown_kind(
            &report,
            "UnresolvedImport",
            "unresolved_use_path"
        ));
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
edition = "2021"
build = "build.rs"

[dependencies]
serde_json = "1"

[features]
preview = []

[workspace]
members = ["crates/*"]

[lib]
name = "repogrammar"
path = "src/rust/lib.rs"

[[bin]]
name = "repogrammar"
path = "src/rust/bin/repogrammar.rs"

[[test]]
name = "integration"
path = "src/rust/integration_tests/main.rs"

[[bench]]
name = "unknowns"
path = "src/rust/benches/unknowns.rs"
"#;
        let report = RustSyntaxParser
            .parse(document("Cargo.toml", text, Language::RustConfig))
            .expect("parse Cargo");
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        for target in [
            "cargo.dependency:serde_json",
            "cargo.edition:2021",
            "cargo.workspace:members",
            "cargo.target:lib:repogrammar",
            "cargo.target:bin:repogrammar",
            "cargo.target:test:integration",
            "cargo.target:bench:unknowns",
            "cargo.crate_root:src/rust/lib.rs",
            "cargo.crate_root:src/rust/bin/repogrammar.rs",
            "cargo.crate_root:src/rust/integration_tests/main.rs",
            "cargo.crate_root:src/rust/benches/unknowns.rs",
        ] {
            assert!(
                has_fact_target(&report, SemanticFactKind::ProjectConfig, target),
                "missing Cargo project config target {target}"
            );
        }
        assert!(report
            .semantic_facts
            .iter()
            .any(|fact| fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "BuildVariantAmbiguity")));
    }

    #[test]
    fn resolves_external_mods_and_preserves_ambiguous_or_unsafe_paths_as_unknown() {
        let context = ParserProjectContext {
            rust_module_paths: vec![
                "src/rust/adapters/parsing/rust/parser.rs".to_string(),
                "src/rust/adapters/parsing/rust/custom/parser.rs".to_string(),
            ],
            ..ParserProjectContext::default()
        };
        let report = RustSyntaxParser
            .parse_with_context(
                document(
                    "src/rust/adapters/parsing/rust/mod.rs",
                    r#"
mod parser;
#[path = "custom/parser.rs"] mod custom_parser;
"#,
                    Language::Rust,
                ),
                &context,
            )
            .expect("parse Rust");
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "module:src/rust/adapters/parsing/rust/parser.rs"
        ));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "module:src/rust/adapters/parsing/rust/custom/parser.rs"
        ));

        let ambiguous_context = ParserProjectContext {
            rust_module_paths: vec![
                "src/rust/adapters/parsing/rust/parser.rs".to_string(),
                "src/rust/adapters/parsing/rust/parser/mod.rs".to_string(),
            ],
            ..ParserProjectContext::default()
        };
        let ambiguous = RustSyntaxParser
            .parse_with_context(
                document(
                    "src/rust/adapters/parsing/rust/mod.rs",
                    "mod parser;",
                    Language::Rust,
                ),
                &ambiguous_context,
            )
            .expect("parse Rust");
        assert!(has_unknown_kind(
            &ambiguous,
            "ConflictingFacts",
            "ambiguous_mod_decl"
        ));

        let unsafe_path = RustSyntaxParser
            .parse_with_context(
                document(
                    "src/rust/adapters/parsing/rust/mod.rs",
                    r#"#[path = "../parser.rs"] mod parser;"#,
                    Language::Rust,
                ),
                &ParserProjectContext::default(),
            )
            .expect("parse Rust");
        assert!(has_unknown_kind(
            &unsafe_path,
            "UnresolvedImport",
            "unsafe_path_attribute"
        ));
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

    fn has_fact_target(report: &ParseReport, kind: SemanticFactKind, target: &str) -> bool {
        report.semantic_facts.iter().any(|fact| {
            fact.kind == kind
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|fact_target| fact_target.as_str() == target)
        })
    }

    fn has_unknown_kind(report: &ParseReport, reason: &str, kind: &str) -> bool {
        report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == reason)
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("rust_unknown_kind={kind}"))
        })
    }

    fn has_unknown_claim(report: &ParseReport, reason: &str, claim: &str) -> bool {
        report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == reason)
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("affected_claim={claim}"))
        })
    }

    #[test]
    fn serde_model_with_use_evidence_anchors_both_traits() {
        let text = r#"
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::SerdeModel));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "serde.Serialize"
        ));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "serde.Deserialize"
        ));
        assert!(report.semantic_facts.iter().any(|fact| fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "serde_attr_shape=rename_all")));
        // Derive-macro expansion stays a non-blocking honesty subclaim.
        assert!(has_unknown_claim(
            &report,
            "MacroOrPreprocessor",
            "rust_derive_expansion"
        ));
    }

    #[test]
    fn serde_derive_without_use_is_blocking_binding() {
        let text = r#"
#[derive(Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .all(|unit| unit.kind != CodeUnitKind::SerdeModel));
        assert!(has_unknown_claim(
            &report,
            "UnresolvedImport",
            "rust_framework_attribute_binding"
        ));
        assert!(!has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "serde.Serialize"
        ));
    }

    #[test]
    fn cfg_on_serde_model_still_blocks() {
        let text = r#"
use serde::Serialize;

#[cfg(feature = "extra")]
#[derive(Serialize)]
pub struct Item {
    pub id: u64,
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::SerdeModel));
        assert!(has_unknown_claim(
            &report,
            "BuildVariantAmbiguity",
            "rust_build_variant"
        ));
    }

    #[test]
    fn thiserror_enum_with_error_attribute_anchors() {
        let text = r#"
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CatalogError {
    #[error("missing")]
    Missing,
    #[error("invalid: {0}")]
    Invalid(String),
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::ThiserrorErrorEnum));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "thiserror.Error"
        ));
    }

    #[test]
    fn tokio_entry_and_test_attributes_anchor() {
        let text = r#"
#[tokio::main]
async fn main() {}

#[tokio::test]
async fn works() {}
"#;
        let report = RustSyntaxParser
            .parse(document("src/main.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::TokioEntry));
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::TokioTest));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "tokio.main"
        ));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "tokio.test"
        ));
    }

    #[test]
    fn clap_parser_derive_anchors() {
        let text = r#"
use clap::Parser;

#[derive(Parser)]
#[command(name = "app")]
pub struct Cli {
    pub verbose: bool,
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::ClapParser));
        assert!(has_fact_target(
            &report,
            SemanticFactKind::Symbol,
            "clap.Parser"
        ));
    }

    #[test]
    fn axum_literal_routes_anchor_each_segment_and_trace_router() {
        let text = r#"
use axum::routing::get;
use axum::Router;

pub fn router() -> Router {
    Router::new()
        .route("/alpha", get(alpha))
        .route("/beta", get(beta))
        .route("/gamma", get(gamma))
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        let route_units = report
            .units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::AxumRoute)
            .count();
        assert_eq!(route_units, 3, "expected 3 axum route units");
        let route_anchors = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "axum.routing.route")
            })
            .count();
        assert_eq!(route_anchors, 3);
        assert!(report.semantic_facts.iter().any(|fact| fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "http_method=GET")));
    }

    #[test]
    fn axum_nonliteral_route_records_blocking_identity_without_unit() {
        let text = r#"
use axum::routing::get;
use axum::Router;

pub fn router(path: &str) -> Router {
    Router::new().route(path, get(handler))
}
"#;
        let report = RustSyntaxParser
            .parse(document("src/lib.rs", text, Language::Rust))
            .expect("parse Rust");
        assert!(report
            .units
            .iter()
            .all(|unit| unit.kind != CodeUnitKind::AxumRoute));
        assert!(has_unknown_claim(
            &report,
            "UnresolvedImport",
            "rust_axum_route_identity"
        ));
    }
}
