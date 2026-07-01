//! Tree-sitter-backed structural Java and Spring code-unit extraction.
//!
//! This adapter does not execute Maven, Gradle, javac, annotation processors, or
//! Spring runtime wiring. It emits structural Java units, exact Spring annotation
//! anchors, and typed UNKNOWN facts for the runtime/classpath pieces that remain
//! unresolved.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use std::collections::BTreeSet;
use tree_sitter::{Node, Parser};

pub(crate) const JAVA_ANCHOR_ENGINE: &str = "repogrammar-java-syntax";
pub(crate) const JAVA_ANCHOR_METHOD: &str = "tree_sitter_java_structural_anchors_v1";

const ROUTE_MAPPING_ANNOTATIONS: &[(&str, &str, &str)] = &[
    (
        "RequestMapping",
        "spring.web.bind.annotation.RequestMapping",
        "REQUEST",
    ),
    ("GetMapping", "spring.web.bind.annotation.GetMapping", "GET"),
    (
        "PostMapping",
        "spring.web.bind.annotation.PostMapping",
        "POST",
    ),
    ("PutMapping", "spring.web.bind.annotation.PutMapping", "PUT"),
    (
        "PatchMapping",
        "spring.web.bind.annotation.PatchMapping",
        "PATCH",
    ),
    (
        "DeleteMapping",
        "spring.web.bind.annotation.DeleteMapping",
        "DELETE",
    ),
];

const SPRING_WEB_BIND_ANNOTATION_PACKAGE: &str = "org.springframework.web.bind.annotation";
const SPRING_STEREOTYPE_PACKAGE: &str = "org.springframework.stereotype";
const SPRING_BOOT_AUTOCONFIGURE_PACKAGE: &str = "org.springframework.boot.autoconfigure";
const SPRING_DATA_JPA_PACKAGE: &str = "org.springframework.data.jpa.repository";
const SPRING_DATA_REPOSITORY_PACKAGE: &str = "org.springframework.data.repository";

#[derive(Debug, Default)]
pub struct JavaSyntaxParser;

impl SourceParser for JavaSyntaxParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        if document.language != Language::Java {
            return Err(ParseError::UnsupportedLanguage);
        }
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .map_err(|error| ParseError::Internal(format!("load Java grammar: {error}")))?;
        let Some(tree) = parser.parse(document.text, None) else {
            return Err(ParseError::Internal(
                "Tree-sitter Java parse failed".to_string(),
            ));
        };

        let mut scanner = JavaTreeScanner::new(document);
        scanner.scan_tree(tree.root_node())?;
        scanner.finish()
    }
}

struct JavaTreeScanner<'a> {
    document: SourceDocument<'a>,
    imports: JavaImportContext,
    units: Vec<CodeUnit>,
    semantic_facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
}

#[derive(Debug, Clone, Default)]
struct VisitContext {
    in_spring_controller: bool,
    class_route_path_shape: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpringRouteAnchor {
    target: &'static str,
    annotation: &'static str,
    http_method: &'static str,
    route_path_shape: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpringClassAnchor {
    target: &'static str,
    annotation: &'static str,
    anchor_kind: &'static str,
}

#[derive(Debug, Clone, Default)]
struct JavaImportContext {
    exact_imports: BTreeSet<String>,
    wildcard_imports: BTreeSet<String>,
}

impl JavaImportContext {
    fn has_import_for(&self, simple_name: &str, packages: &[&str]) -> bool {
        packages.iter().any(|package| {
            self.exact_imports
                .contains(&format!("{package}.{simple_name}"))
                || self.wildcard_imports.contains(*package)
        })
    }
}

impl<'a> JavaTreeScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        let imports = parse_import_context(document.text);
        Self {
            document,
            imports,
            units: Vec::new(),
            semantic_facts: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn scan_tree(&mut self, root: Node<'_>) -> Result<(), ParseError> {
        self.add_unit(CodeUnitKind::Module, "file", 0, self.document.text.len())?;
        if root.has_error() {
            self.diagnostics.push(ParseDiagnostic {
                path: self.document.path.to_string(),
                range: None,
                severity: ParseDiagnosticSeverity::Warning,
                message: "Tree-sitter Java parse contains syntax errors; extraction is structural"
                    .to_string(),
            });
        }
        self.visit(root, VisitContext::default())?;
        Ok(())
    }

    fn visit(&mut self, node: Node<'_>, context: VisitContext) -> Result<(), ParseError> {
        let mut next_context = context.clone();
        match node.kind() {
            "class_declaration" | "enum_declaration" | "record_declaration" => {
                next_context = self.scan_class_like(node)?;
            }
            "interface_declaration" => {
                next_context = self.scan_interface(node)?;
            }
            "method_declaration" => {
                self.scan_method(node, &context)?;
            }
            "constructor_declaration" => {
                self.add_named_node_unit(node, CodeUnitKind::Method, "constructor")?;
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, next_context.clone())?;
        }
        Ok(())
    }

    fn scan_class_like(&mut self, node: Node<'_>) -> Result<VisitContext, ParseError> {
        let annotations = modifier_text(self.document.text, node, "class");
        let class_route_path_shape = class_route_path_shape(&annotations, &self.imports);
        let anchor = spring_class_anchor(&annotations, &self.imports);
        let kind = match anchor.as_ref().map(|anchor| anchor.anchor_kind) {
            Some("spring_boot_application") => CodeUnitKind::SpringBootApplication,
            Some(_) => CodeUnitKind::SpringComponent,
            None => CodeUnitKind::Class,
        };
        let unit = self.add_named_node_unit(node, kind, "class")?;
        if let Some(anchor) = anchor {
            self.semantic_facts.push(class_anchor_fact(
                &self.document,
                &unit,
                &anchor,
                class_shape_assumptions(
                    anchor.anchor_kind,
                    anchor.annotation,
                    &annotations,
                    node_text(self.document.text, node),
                ),
            )?);
            if anchor.anchor_kind == "spring_boot_application" {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    UnknownReasonCode::RuntimeDependencyInjection,
                    "java_spring_component_scan",
                    "spring_boot_component_scan",
                    "Spring Boot component scan is runtime framework behavior",
                )?);
            }
        } else if contains_spring_known_annotation_name(&annotations) {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "java_spring_annotation_binding",
                "spring_class_annotation_unresolved_import",
                "Spring class annotation simple name lacks an exact import or FQN",
            )?);
        }
        Ok(VisitContext {
            in_spring_controller: has_exact_annotation(
                &annotations,
                "Controller",
                &[SPRING_STEREOTYPE_PACKAGE],
                &self.imports,
            ) || has_exact_annotation(
                &annotations,
                "RestController",
                &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
                &self.imports,
            ),
            class_route_path_shape,
        })
    }

    fn scan_interface(&mut self, node: Node<'_>) -> Result<VisitContext, ParseError> {
        let annotations = modifier_text(self.document.text, node, "interface");
        let text = node_text(self.document.text, node);
        let repository_anchor = spring_data_repository_anchor(&annotations, text, &self.imports);
        let kind = if repository_anchor.is_some() {
            CodeUnitKind::SpringDataRepository
        } else {
            CodeUnitKind::Class
        };
        let unit = self.add_named_node_unit(node, kind, "interface")?;
        if let Some(anchor) = repository_anchor {
            self.semantic_facts.push(class_anchor_fact(
                &self.document,
                &unit,
                &anchor,
                class_shape_assumptions(anchor.anchor_kind, anchor.annotation, &annotations, text),
            )?);
        }
        Ok(VisitContext::default())
    }

    fn scan_method(&mut self, node: Node<'_>, context: &VisitContext) -> Result<(), ParseError> {
        let annotations = modifier_text(self.document.text, node, "method");
        let route = spring_route_anchor(&annotations, &self.imports);
        let spring_like_route = contains_route_mapping_annotation_name(&annotations);
        let kind = if route.is_some() && context.in_spring_controller {
            CodeUnitKind::SpringMvcRoute
        } else {
            CodeUnitKind::Method
        };
        let unit = self.add_named_node_unit(node, kind, "method")?;
        match route {
            Some(route) if context.in_spring_controller => {
                let slice = node_text(self.document.text, node);
                self.semantic_facts.push(route_anchor_fact(
                    &self.document,
                    &unit,
                    &route,
                    route_assumptions(&route, context.class_route_path_shape, &annotations, slice),
                )?);
                if route.route_path_shape == "dynamic" {
                    self.semantic_facts.push(unknown_fact(
                        &self.document,
                        &unit,
                        UnknownReasonCode::FrameworkMagic,
                        "java_spring_route_path",
                        "non_literal_route_path",
                        "Spring route path is not a direct string literal",
                    )?);
                }
            }
            Some(_) => {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "java_spring_controller_identity",
                    "route_mapping_without_controller",
                    "Spring route mapping annotation appeared outside an exact controller class",
                )?);
            }
            None if spring_like_route => {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    UnknownReasonCode::UnresolvedImport,
                    "java_spring_annotation_binding",
                    "spring_route_annotation_unresolved_import",
                    "Spring route annotation simple name lacks an exact import or FQN",
                )?);
            }
            None => {}
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
        self.add_unit(kind, &name, node.start_byte(), node.end_byte())
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
            "unit:{}#{}:{}-{}:{}",
            self.document.path,
            kind.as_str(),
            slug(name),
            start_byte,
            self.ordinal
        ))
        .map_err(ParseError::Internal)?;
        self.ordinal += 1;
        let unit = CodeUnit {
            id,
            language: Language::Java,
            kind,
            range,
            provenance,
        };
        self.units.push(unit.clone());
        Ok(unit)
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

fn class_anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    anchor: &SpringClassAnchor,
    assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    structural_anchor_fact(
        document,
        unit,
        SemanticFactKind::Type,
        anchor.target,
        assumptions,
        "bounded Java Spring class annotation anchor",
    )
}

fn route_anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    anchor: &SpringRouteAnchor,
    assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    structural_anchor_fact(
        document,
        unit,
        SemanticFactKind::ResolvedCall,
        anchor.target,
        assumptions,
        "bounded Java Spring route annotation anchor",
    )
}

fn structural_anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    kind: SemanticFactKind,
    target: &str,
    assumptions: Vec<String>,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: JAVA_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: JAVA_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions,
    })
}

fn unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    affected_claim: &str,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: JAVA_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: JAVA_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={affected_claim}"),
            format!("java_unknown_kind={kind}"),
        ],
    })
}

fn spring_route_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringRouteAnchor> {
    for (annotation, target, default_method) in ROUTE_MAPPING_ANNOTATIONS {
        let Some(segment) = annotation_segment_exact(
            annotation_text,
            annotation,
            &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
            imports,
        ) else {
            continue;
        };
        let http_method = if *annotation == "RequestMapping" {
            request_mapping_http_method(&segment).unwrap_or(default_method)
        } else {
            default_method
        };
        return Some(SpringRouteAnchor {
            target,
            annotation,
            http_method,
            route_path_shape: route_path_shape(&segment),
        });
    }
    None
}

fn spring_class_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringClassAnchor> {
    for (annotation, package, target, anchor_kind) in [
        (
            "SpringBootApplication",
            SPRING_BOOT_AUTOCONFIGURE_PACKAGE,
            "spring.boot.autoconfigure.SpringBootApplication",
            "spring_boot_application",
        ),
        (
            "RestController",
            SPRING_WEB_BIND_ANNOTATION_PACKAGE,
            "spring.web.bind.annotation.RestController",
            "spring_component",
        ),
        (
            "Controller",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Controller",
            "spring_component",
        ),
        (
            "Service",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Service",
            "spring_component",
        ),
        (
            "Repository",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Repository",
            "spring_component",
        ),
        (
            "Component",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Component",
            "spring_component",
        ),
    ] {
        if has_exact_annotation(annotation_text, annotation, &[package], imports) {
            return Some(SpringClassAnchor {
                target,
                annotation,
                anchor_kind,
            });
        }
    }
    None
}

fn spring_data_repository_anchor(
    annotation_text: &str,
    interface_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringClassAnchor> {
    if extends_exact_type(
        interface_text,
        "JpaRepository",
        &[SPRING_DATA_JPA_PACKAGE],
        imports,
    ) {
        return Some(SpringClassAnchor {
            target: "spring.data.jpa.repository.JpaRepository",
            annotation: "JpaRepository",
            anchor_kind: "spring_data_jpa_repository",
        });
    }
    if has_exact_annotation(
        annotation_text,
        "RepositoryDefinition",
        &[SPRING_DATA_REPOSITORY_PACKAGE],
        imports,
    ) {
        return Some(SpringClassAnchor {
            target: "spring.data.repository.RepositoryDefinition",
            annotation: "RepositoryDefinition",
            anchor_kind: "spring_data_repository_definition",
        });
    }
    None
}

fn class_shape_assumptions(
    anchor_kind: &str,
    annotation: &str,
    annotations: &str,
    slice: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        format!("java_anchor_kind={anchor_kind}"),
        format!("spring_annotation={annotation}"),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_class_shape={}", java_class_shape(slice)),
    ]
}

fn route_assumptions(
    anchor: &SpringRouteAnchor,
    class_route_path_shape: &str,
    annotations: &str,
    slice: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        "java_anchor_kind=spring_mvc_route".to_string(),
        format!("spring_annotation={}", anchor.annotation),
        format!("http_method={}", anchor.http_method),
        format!("route_path_shape={}", anchor.route_path_shape),
        format!("class_route_path_shape={class_route_path_shape}"),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_return_shape={}", java_return_shape(slice)),
        format!("java_parameter_shape={}", java_parameter_shape(slice)),
    ]
}

fn class_route_path_shape(annotation_text: &str, imports: &JavaImportContext) -> &'static str {
    annotation_segment_exact(
        annotation_text,
        "RequestMapping",
        &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
        imports,
    )
    .as_deref()
    .map(route_path_shape)
    .unwrap_or("none")
}

fn request_mapping_http_method(segment: &str) -> Option<&'static str> {
    if segment.contains("RequestMethod.GET") {
        Some("GET")
    } else if segment.contains("RequestMethod.POST") {
        Some("POST")
    } else if segment.contains("RequestMethod.PUT") {
        Some("PUT")
    } else if segment.contains("RequestMethod.PATCH") {
        Some("PATCH")
    } else if segment.contains("RequestMethod.DELETE") {
        Some("DELETE")
    } else {
        None
    }
}

fn route_path_shape(segment: &str) -> &'static str {
    if first_quoted_string(segment).is_some() {
        "literal"
    } else if segment.contains('(') {
        "dynamic"
    } else {
        "none"
    }
}

fn java_visibility_shape(annotations_or_header: &str) -> &'static str {
    let tokens = identifier_tokens(annotations_or_header);
    if tokens.iter().any(|token| token == "public") {
        "public"
    } else if tokens.iter().any(|token| token == "protected") {
        "protected"
    } else if tokens.iter().any(|token| token == "private") {
        "private"
    } else {
        "package_private"
    }
}

fn java_class_shape(slice: &str) -> &'static str {
    let header = declaration_header(slice);
    if header.contains(" record ") || header.contains(" record") {
        "record"
    } else if header.contains(" enum ") || header.contains(" enum") {
        "enum"
    } else if header.contains(" interface ") || header.contains(" interface") {
        "interface"
    } else {
        "class"
    }
}

fn java_return_shape(slice: &str) -> &'static str {
    let header = declaration_header(slice);
    if header.contains(" ResponseEntity<") || header.contains(" ResponseEntity ") {
        "response_entity"
    } else if header.contains(" void ") {
        "void"
    } else if header.contains(" String ") || header.contains(" List<") || header.contains(" Map<") {
        "object"
    } else if header.contains(" int ")
        || header.contains(" long ")
        || header.contains(" boolean ")
        || header.contains(" double ")
    {
        "primitive"
    } else {
        "unknown"
    }
}

fn java_parameter_shape(slice: &str) -> String {
    let header = declaration_header(slice);
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

fn declaration_header(slice: &str) -> &str {
    slice.split('{').next().unwrap_or(slice)
}

fn extends_exact_type(
    slice: &str,
    simple_type: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> bool {
    let header = declaration_header(slice);
    let Some(after_extends) = header.split_once("extends").map(|(_, after)| after) else {
        return false;
    };
    if packages
        .iter()
        .any(|package| after_extends.contains(&format!("{package}.{simple_type}")))
    {
        return true;
    }
    identifier_tokens(after_extends)
        .iter()
        .any(|token| token == simple_type)
        && imports.has_import_for(simple_type, packages)
}

fn has_exact_annotation(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> bool {
    annotation_full_names(annotation_text)
        .iter()
        .any(|full_name| annotation_name_is_exact(full_name, simple_name, packages, imports))
}

fn contains_route_mapping_annotation_name(annotation_text: &str) -> bool {
    ROUTE_MAPPING_ANNOTATIONS.iter().any(|(name, _, _)| {
        annotation_full_names(annotation_text)
            .iter()
            .any(|full_name| full_name.split('.').next_back() == Some(*name))
    })
}

fn contains_spring_known_annotation_name(annotation_text: &str) -> bool {
    [
        "SpringBootApplication",
        "RestController",
        "Controller",
        "Service",
        "Repository",
        "Component",
        "RepositoryDefinition",
    ]
    .iter()
    .chain(ROUTE_MAPPING_ANNOTATIONS.iter().map(|(name, _, _)| name))
    .any(|name| {
        annotation_full_names(annotation_text)
            .iter()
            .any(|full_name| full_name.split('.').next_back() == Some(*name))
    })
}

fn annotation_full_names(annotation_text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (index, byte) in annotation_text.bytes().enumerate() {
        if byte != b'@' {
            continue;
        }
        let mut cursor = index + 1;
        while annotation_text
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        let start = cursor;
        while annotation_text
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.')
        {
            cursor += 1;
        }
        if cursor > start {
            names.push(annotation_text[start..cursor].to_string());
        }
    }
    names
}

fn annotation_name_is_exact(
    full_name: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> bool {
    if full_name.contains('.') {
        return packages
            .iter()
            .any(|package| full_name == format!("{package}.{simple_name}"));
    }
    full_name == simple_name && imports.has_import_for(simple_name, packages)
}

fn annotation_segment_exact(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> Option<String> {
    let bytes = annotation_text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'@' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        let name_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.')
        {
            cursor += 1;
        }
        let full_name = &annotation_text[name_start..cursor];
        if !annotation_name_is_exact(full_name, simple_name, packages, imports) {
            index = cursor.saturating_add(1);
            continue;
        }
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'(') {
            return Some(annotation_text[index..cursor].to_string());
        }
        let mut depth = 0i32;
        let mut end = cursor;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(annotation_text[index..=end].to_string());
                    }
                }
                _ => {}
            }
            end += 1;
        }
        return Some(annotation_text[index..].to_string());
    }
    None
}

fn parse_import_context(source: &str) -> JavaImportContext {
    let mut context = JavaImportContext::default();
    for raw_line in source.lines() {
        let line = raw_line.trim();
        let Some(rest) = line.strip_prefix("import ") else {
            continue;
        };
        if rest.starts_with("static ") {
            continue;
        }
        let import_path = rest.trim_end_matches(';').trim();
        if let Some(package) = import_path.strip_suffix(".*") {
            context.wildcard_imports.insert(package.to_string());
        } else if !import_path.is_empty() {
            context.exact_imports.insert(import_path.to_string());
        }
    }
    context
}

fn modifier_text(source: &str, node: Node<'_>, fallback_kind: &str) -> String {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "modifiers" {
            return node_text(source, child).to_string();
        }
    }
    let text = node_text(source, node);
    let marker = match fallback_kind {
        "class" => "class",
        "interface" => "interface",
        "method" => "(",
        _ => "{",
    };
    text.split(marker).next().unwrap_or("").to_string()
}

fn first_quoted_string(text: &str) -> Option<String> {
    let mut chars = text.char_indices();
    while let Some((start_index, character)) = chars.next() {
        if character != '"' {
            continue;
        }
        let value_start = start_index + 1;
        let mut escaped = false;
        for (end_index, character) in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == '"' {
                return Some(text[value_start..end_index].to_string());
            }
        }
    }
    None
}

fn identifier_tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
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
        if matches!(child.kind(), "identifier" | "type_identifier") {
            return node_text_checked(source, child).map(str::to_string);
        }
        if let Some(value) = first_identifier_text(source, child) {
            return Some(value);
        }
    }
    None
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
    let compact = slug.trim_matches('_').to_string();
    if compact.is_empty() {
        "unit".to_string()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, IrEdgeLabel, RepositoryRevision};

    fn parse(text: &str) -> ParseReport {
        let document = SourceDocument {
            path: "src/main/java/com/example/DemoController.java",
            language: Language::Java,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        };
        JavaSyntaxParser.parse(document).expect("parse java")
    }

    fn unit_kinds(report: &ParseReport) -> Vec<&'static str> {
        report.units.iter().map(|unit| unit.kind.as_str()).collect()
    }

    fn targets(report: &ParseReport) -> Vec<String> {
        let mut targets = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_targets(report: &ParseReport) -> Vec<String> {
        report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect()
    }

    #[test]
    fn extracts_spring_mvc_routes_only_inside_exact_controller_classes() {
        let report = parse(
            r#"
package com.example;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/books")
public class DemoController {
    @GetMapping("/{id}")
    public String show(String id) {
        return id;
    }
}
"#,
        );

        assert!(unit_kinds(&report).contains(&"spring_component"));
        assert!(unit_kinds(&report).contains(&"spring_mvc_route"));
        assert_eq!(
            targets(&report),
            vec![
                "spring.web.bind.annotation.GetMapping".to_string(),
                "spring.web.bind.annotation.RestController".to_string()
            ]
        );
        assert!(report.ir_edges.iter().any(|edge| {
            edge.label == IrEdgeLabel::Contains
                && edge
                    .from_node_id
                    .as_str()
                    .contains("spring_component:democontroller")
                && edge.to_node_id.as_str().contains("spring_mvc_route:show")
        }));
    }

    #[test]
    fn route_annotation_without_controller_stays_unknown() {
        let report = parse(
            r#"
package com.example;

public class Utility {
    @org.springframework.web.bind.annotation.GetMapping("/accidental")
    public String accidental() {
        return "no";
    }
}
"#,
        );

        assert!(unit_kinds(&report).contains(&"method"));
        assert!(!unit_kinds(&report).contains(&"spring_mvc_route"));
        assert_eq!(unknown_targets(&report), vec!["FrameworkMagic".to_string()]);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "affected_claim=java_spring_controller_identity")
        }));
    }

    #[test]
    fn extracts_boot_application_and_jpa_repository_anchors() {
        let app = parse(
            r#"
package com.example;

import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class DemoApplication {
    public static void main(String[] args) {
        SpringApplication.run(DemoApplication.class, args);
    }
}
"#,
        );
        assert!(unit_kinds(&app).contains(&"spring_boot_application"));
        assert!(
            targets(&app).contains(&"spring.boot.autoconfigure.SpringBootApplication".to_string())
        );
        assert!(app.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=java_spring_component_scan")
        }));

        let repository = parse(
            r#"
package com.example;

import org.springframework.data.jpa.repository.JpaRepository;

interface BookRepository extends JpaRepository<Book, Long> {
}
"#,
        );
        assert!(unit_kinds(&repository).contains(&"spring_data_repository"));
        assert_eq!(
            targets(&repository),
            vec!["spring.data.jpa.repository.JpaRepository".to_string()]
        );
    }

    #[test]
    fn dynamic_route_paths_are_non_exact_unknown_subclaims() {
        let report = parse(
            r#"
package com.example;

import org.springframework.stereotype.Controller;
import org.springframework.web.bind.annotation.RequestMapping;

@Controller
class DynamicController {
    @RequestMapping(value = Routes.SHOW, method = RequestMethod.GET)
    String show() {
        return "ok";
    }
}
"#,
        );

        assert!(unit_kinds(&report).contains(&"spring_mvc_route"));
        assert!(targets(&report).contains(&"spring.web.bind.annotation.RequestMapping".to_string()));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=java_spring_route_path")
        }));
    }

    #[test]
    fn spring_lookalike_simple_annotations_without_import_are_unresolved_unknowns() {
        let report = parse(
            r#"
package com.example;

@RestController
class LookalikeController {
    @GetMapping("/books")
    String list() {
        return "ok";
    }
}
"#,
        );

        assert!(!unit_kinds(&report).contains(&"spring_component"));
        assert!(!unit_kinds(&report).contains(&"spring_mvc_route"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().expect("target").as_str() == "UnresolvedImport"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=java_spring_annotation_binding")
        }));
    }
}
