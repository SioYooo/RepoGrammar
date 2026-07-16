//! Tree-sitter-backed structural Java and framework code-unit extraction.
//!
//! This adapter does not execute Maven, Gradle, javac, annotation processors,
//! Lombok/MapStruct code generation, or framework runtime wiring. It emits
//! structural Java units, exact annotation anchors gated by import/FQN evidence,
//! and typed UNKNOWN facts for the runtime/classpath/generated pieces that remain
//! unresolved.
//!
//! The framework-agnostic scanner, unit/fact plumbing, import context, and text
//! utilities live in this module. Per-framework recognition tables and UNKNOWN
//! recipes live in the sibling modules ([`spring`], [`junit`], [`jpa`],
//! [`jaxrs`]).

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser};

pub(crate) mod jaxrs;
pub(crate) mod jpa;
pub(crate) mod junit;
pub(crate) mod spring;
pub(crate) mod test_data;

pub(crate) const JAVA_ANCHOR_ENGINE: &str = "repogrammar-java-syntax";
pub(crate) const JAVA_ANCHOR_METHOD: &str = "tree_sitter_java_structural_anchors_v1";

const LOMBOK_PACKAGE: &str = "lombok";
const LOMBOK_ANNOTATIONS: &[&str] = &[
    "Data",
    "Getter",
    "Setter",
    "Builder",
    "Value",
    "NoArgsConstructor",
    "AllArgsConstructor",
    "RequiredArgsConstructor",
    "Slf4j",
    "EqualsAndHashCode",
    "ToString",
];

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
    test_data_registries: Vec<test_data::ClassTestDataRegistry>,
    ordinal: usize,
}

/// Per-framework context propagated from an enclosing class-like declaration to
/// its members. Each framework owns disjoint slots; the scanner fills them in
/// [`JavaTreeScanner::scan_class_like`] / [`JavaTreeScanner::scan_interface`].
#[derive(Debug, Clone, Default)]
pub(crate) struct VisitContext {
    in_spring_controller: bool,
    class_route_path_shape: &'static str,
    in_jaxrs_resource: bool,
    jaxrs_class_route_path_shape: &'static str,
    in_spring_data_repository: bool,
    mockito_context: Option<&'static str>,
    test_data_registry: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JavaImportContext {
    exact_imports: BTreeSet<String>,
    exact_imports_by_simple: BTreeMap<String, BTreeSet<String>>,
    wildcard_imports: BTreeSet<String>,
    local_type_names: BTreeSet<String>,
    local_type_inventory_open: bool,
}

impl JavaImportContext {
    pub(crate) fn has_import_for(&self, simple_name: &str, packages: &[&str]) -> bool {
        packages.iter().any(|package| {
            self.exact_imports
                .contains(&format!("{package}.{simple_name}"))
                || self.wildcard_imports.contains(*package)
        })
    }

    pub(crate) fn has_unambiguous_explicit_import_for(
        &self,
        simple_name: &str,
        packages: &[&str],
    ) -> bool {
        let matching_imports = self.exact_imports_by_simple.get(simple_name);
        !self.local_type_inventory_open
            && !self.local_type_names.contains(simple_name)
            && matching_imports.is_some_and(|imports| imports.len() == 1)
            && packages.iter().any(|package| {
                matching_imports
                    .is_some_and(|imports| imports.contains(&format!("{package}.{simple_name}")))
            })
    }
}

impl<'a> JavaTreeScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        Self {
            document,
            imports: JavaImportContext::default(),
            units: Vec::new(),
            semantic_facts: Vec::new(),
            diagnostics: Vec::new(),
            test_data_registries: Vec::new(),
            ordinal: 0,
        }
    }

    fn scan_tree(&mut self, root: Node<'_>) -> Result<(), ParseError> {
        self.imports = parse_import_context(root, self.document.text);
        self.imports.local_type_names = collect_local_type_names(root, self.document.text);
        self.imports.local_type_inventory_open = root.has_error();
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
            "class_body"
                if node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        "object_creation_expression" | "enum_constant"
                    )
                }) =>
            {
                next_context = self.scan_anonymous_class_body(node);
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
        let slice = node_text(self.document.text, node);
        let test_data_registry = self.register_test_data_class(node);
        let class_route_path_shape = spring::class_route_path_shape(&annotations, &self.imports);

        // Recognition precedence: Spring class anchor, then JPA entity, then a
        // JAX-RS `@Path` resource class. A class-like declaration produces one
        // structural unit, so only the first matching anchor sets the kind.
        let spring_anchor = spring::spring_class_anchor(&annotations, &self.imports);
        let jpa_anchor = spring_anchor
            .is_none()
            .then(|| jpa::entity_anchor(&annotations, &self.imports))
            .flatten();
        let jaxrs_anchor = (spring_anchor.is_none() && jpa_anchor.is_none())
            .then(|| jaxrs::resource_class_anchor(&annotations, &self.imports))
            .flatten();

        let kind = if let Some(anchor) = spring_anchor.as_ref() {
            match anchor.anchor_kind {
                "spring_boot_application" => CodeUnitKind::SpringBootApplication,
                _ => CodeUnitKind::SpringComponent,
            }
        } else if let Some(anchor) = jpa_anchor.as_ref() {
            anchor.kind.clone()
        } else if jaxrs_anchor.is_some() {
            CodeUnitKind::JaxrsResourceClass
        } else {
            CodeUnitKind::Class
        };

        let unit = self.add_named_node_unit(node, kind, "class")?;

        if contains_annotation_simple_name(&annotations, &["MethodSource"]) {
            if test_data::has_strict_annotation_identity(
                &annotations,
                "MethodSource",
                "org.junit.jupiter.params.provider",
                &self.imports,
            ) {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "java_test_method_source",
                    "type_level_method_source",
                    "Type-level or inherited JUnit MethodSource resolution is outside the bounded same-method slice",
                )?;
            } else {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::UnresolvedImport,
                    "java_test_method_source",
                    "unresolved_type_level_method_source_annotation",
                    "Type-level MethodSource lacks an unambiguous explicit import or FQN",
                )?;
            }
        }

        if let Some(anchor) = spring_anchor.as_ref() {
            self.semantic_facts.push(class_anchor_fact(
                &self.document,
                &unit,
                anchor.target,
                class_shape_assumptions(anchor.anchor_kind, anchor.annotation, &annotations, slice),
            )?);
            match anchor.anchor_kind {
                "spring_boot_application" => {
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::RuntimeDependencyInjection,
                        "java_spring_component_scan",
                        "spring_boot_component_scan",
                        "Spring Boot component scan is runtime framework behavior",
                    )?;
                }
                "spring_component" => {
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::RuntimeDependencyInjection,
                        "java_spring_component_scan",
                        "spring_component_scan",
                        "Spring component discovery depends on runtime component scan behavior",
                    )?;
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::RuntimeDependencyInjection,
                        "java_spring_dependency_injection",
                        "spring_dependency_injection",
                        "Spring dependency injection bindings are runtime framework behavior",
                    )?;
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::FrameworkMagic,
                        "java_spring_proxy_semantics",
                        "spring_proxy_semantics",
                        "Spring AOP/proxy semantics are runtime framework behavior",
                    )?;
                }
                _ => {}
            }
        } else if let Some(anchor) = jpa_anchor.as_ref() {
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::Type,
                anchor.target,
                jpa::entity_shape_assumptions(anchor, &annotations, slice, &self.imports),
                "bounded Java JPA entity annotation anchor",
            )?);
            self.push_unknown(
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "java_jpa_runtime_mapping",
                "jpa_runtime_mapping",
                "JPA lazy proxies, naming strategies, and orm.xml mapping are runtime behavior",
            )?;
        } else if let Some(anchor) = jaxrs_anchor.as_ref() {
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::Type,
                anchor.target,
                jaxrs::resource_class_assumptions(anchor, &annotations, slice, &self.imports),
                "bounded Java JAX-RS resource path annotation anchor",
            )?);
        } else if spring::contains_spring_known_annotation_name(&annotations) {
            self.push_unknown(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "java_spring_annotation_binding",
                "spring_class_annotation_unresolved_import",
                "Spring class annotation simple name lacks an exact import or FQN",
            )?;
        } else if jpa::contains_known_entity_annotation_name(&annotations) {
            self.push_unknown(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "java_jpa_entity_identity",
                "jpa_entity_annotation_unresolved_import",
                "JPA entity annotation simple name lacks an exact import or FQN",
            )?;
        } else if jaxrs::contains_known_path_annotation_name(&annotations) {
            self.push_unknown(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "java_jaxrs_resource_identity",
                "jaxrs_resource_annotation_unresolved_import",
                "JAX-RS resource annotation simple name lacks an exact import or FQN",
            )?;
        }

        if lombok_annotation_present(&annotations, &self.imports) {
            self.push_unknown(
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "java_generated_members",
                "lombok_generated_members",
                "Lombok generated members are annotation-processor output, never simulated",
            )?;
        }

        let mockito_context = junit::mockito_context(slice, &self.imports);
        if mockito_context.is_some() {
            self.push_unknown(
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "java_mockito_runtime_mocks",
                "mockito_runtime_mocks",
                "Mockito mocks are bytecode-generated at runtime, never simulated",
            )?;
        }

        Ok(VisitContext {
            in_spring_controller: spring::is_controller_context(&annotations, &self.imports),
            class_route_path_shape,
            in_jaxrs_resource: jaxrs_anchor.is_some(),
            jaxrs_class_route_path_shape: jaxrs::class_path_shape(&annotations, &self.imports),
            in_spring_data_repository: false,
            mockito_context,
            test_data_registry: Some(test_data_registry),
        })
    }

    fn scan_interface(&mut self, node: Node<'_>) -> Result<VisitContext, ParseError> {
        let annotations = modifier_text(self.document.text, node, "interface");
        let text = node_text(self.document.text, node);
        let test_data_registry = self.register_test_data_class(node);
        let repository_anchor =
            spring::spring_data_repository_anchor(&annotations, text, &self.imports);
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
                anchor.target,
                class_shape_assumptions(anchor.anchor_kind, anchor.annotation, &annotations, text),
            )?);
            self.push_unknown(
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "java_spring_generated_repository",
                "spring_data_generated_repository",
                "Spring Data repository implementations are generated runtime framework behavior",
            )?;
            return Ok(VisitContext {
                in_spring_data_repository: true,
                test_data_registry: Some(test_data_registry),
                ..VisitContext::default()
            });
        }
        Ok(VisitContext {
            test_data_registry: Some(test_data_registry),
            ..VisitContext::default()
        })
    }

    fn register_test_data_class(&mut self, class_like: Node<'_>) -> usize {
        let registry = test_data::ClassTestDataRegistry::from_class_like(
            self.document.text,
            class_like,
            &self.imports,
        );
        let index = self.test_data_registries.len();
        self.test_data_registries.push(registry);
        index
    }

    fn scan_anonymous_class_body(&mut self, body: Node<'_>) -> VisitContext {
        let registry = test_data::ClassTestDataRegistry::from_class_body(
            self.document.text,
            body,
            &self.imports,
        );
        let index = self.test_data_registries.len();
        self.test_data_registries.push(registry);
        VisitContext {
            test_data_registry: Some(index),
            ..VisitContext::default()
        }
    }

    fn scan_method(&mut self, node: Node<'_>, context: &VisitContext) -> Result<(), ParseError> {
        let annotations = modifier_text(self.document.text, node, "method");
        let slice = node_text(self.document.text, node);
        let route = spring::spring_route_anchor(&annotations, &self.imports);
        let spring_like_route = spring::contains_route_mapping_annotation_name(&annotations);
        let jaxrs_verb = jaxrs::resource_method_anchor(&annotations, &self.imports);
        let jaxrs_like_verb = jaxrs::contains_known_verb_annotation_name(&annotations);
        let test = junit::classify_test_method(&annotations, &self.imports);
        let method_name = node
            .child_by_field_name("name")
            .and_then(|name| node_text_checked(self.document.text, name))
            .unwrap_or("");

        let kind = if route.is_some() && context.in_spring_controller {
            CodeUnitKind::SpringMvcRoute
        } else if jaxrs_verb.is_some() && context.in_jaxrs_resource {
            CodeUnitKind::JaxrsResourceMethod
        } else if let junit::TestClassification::Resolved(anchor) = &test {
            anchor.kind.clone()
        } else {
            CodeUnitKind::Method
        };
        let unit = self.add_named_node_unit(node, kind.clone(), "method")?;

        // Spring MVC routes.
        match &route {
            Some(route) if context.in_spring_controller => {
                self.semantic_facts.push(route_anchor_fact(
                    &self.document,
                    &unit,
                    route.target,
                    spring::route_assumptions(
                        route,
                        context.class_route_path_shape,
                        &annotations,
                        slice,
                    ),
                )?);
                if route.route_path_shape == "dynamic" {
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::FrameworkMagic,
                        "java_spring_route_path",
                        "non_literal_route_path",
                        "Spring route path is not a direct string literal",
                    )?;
                }
            }
            Some(_) => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "java_spring_controller_identity",
                    "route_mapping_without_controller",
                    "Spring route mapping annotation appeared outside an exact controller class",
                )?;
            }
            None if spring_like_route => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::UnresolvedImport,
                    "java_spring_annotation_binding",
                    "spring_route_annotation_unresolved_import",
                    "Spring route annotation simple name lacks an exact import or FQN",
                )?;
            }
            None => {}
        }

        // JAX-RS resource methods.
        match &jaxrs_verb {
            Some(verb) if context.in_jaxrs_resource => {
                self.semantic_facts.push(structural_anchor_fact(
                    &self.document,
                    &unit,
                    SemanticFactKind::ResolvedCall,
                    verb.target,
                    jaxrs::resource_method_assumptions(
                        verb,
                        context.jaxrs_class_route_path_shape,
                        &annotations,
                        slice,
                    ),
                    "bounded Java JAX-RS resource method annotation anchor",
                )?);
                if verb.route_path_shape == "dynamic" {
                    self.push_unknown(
                        &unit,
                        UnknownReasonCode::FrameworkMagic,
                        "java_jaxrs_route_path",
                        "non_literal_resource_path",
                        "JAX-RS resource path is not a direct string literal",
                    )?;
                }
            }
            Some(_) => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "java_jaxrs_resource_identity",
                    "resource_method_without_path_class",
                    "JAX-RS verb annotation appeared outside an exact @Path resource class",
                )?;
            }
            None if jaxrs_like_verb => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::UnresolvedImport,
                    "java_jaxrs_resource_identity",
                    "jaxrs_verb_annotation_unresolved_import",
                    "JAX-RS resource verb annotation simple name lacks an exact import or FQN",
                )?;
            }
            None => {}
        }

        // JUnit / TestNG test methods.
        match &test {
            junit::TestClassification::Resolved(anchor)
                if kind == anchor.kind && route.is_none() && jaxrs_verb.is_none() =>
            {
                self.semantic_facts.push(structural_anchor_fact(
                    &self.document,
                    &unit,
                    SemanticFactKind::ResolvedCall,
                    anchor.target,
                    junit::test_method_assumptions(
                        anchor,
                        context.mockito_context,
                        &annotations,
                        slice,
                    ),
                    "bounded Java test annotation anchor",
                )?);
                let registry = context
                    .test_data_registry
                    .and_then(|index| self.test_data_registries.get(index));
                match test_data::resolve(
                    &anchor.data_reference,
                    registry,
                    method_name,
                    node.start_byte(),
                    test_data::method_header_is_parse_degraded(node),
                ) {
                    test_data::TestDataResolution::None => {}
                    test_data::TestDataResolution::Resolved(binding) => {
                        self.semantic_facts.push(structural_anchor_fact(
                            &self.document,
                            &unit,
                            SemanticFactKind::ResolvedCall,
                            binding.target,
                            binding.assumptions,
                            binding.note,
                        )?);
                    }
                    test_data::TestDataResolution::Unknown(unknown) => {
                        let affected_claim = match &anchor.data_reference {
                            test_data::TestDataReference::Junit(_) => "java_test_method_source",
                            test_data::TestDataReference::Testng(_) => "java_testng_data_provider",
                            test_data::TestDataReference::None => "java_test_annotation_binding",
                        };
                        self.push_unknown_with(
                            &unit,
                            unknown.reason,
                            affected_claim,
                            unknown.kind,
                            unknown.note,
                            unknown.assumptions,
                        )?;
                    }
                }
            }
            junit::TestClassification::Conflict => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::ConflictingFacts,
                    "java_test_annotation_binding",
                    "ambiguous_test_annotation_binding",
                    "Java test annotations resolve to incompatible framework or test kinds",
                )?;
            }
            junit::TestClassification::Lookalike => {
                self.push_unknown(
                    &unit,
                    UnknownReasonCode::UnresolvedImport,
                    "java_test_annotation_binding",
                    "test_annotation_unresolved_import",
                    "Test annotation simple name lacks an exact import or FQN",
                )?;
            }
            _ => {}
        }

        // Spring Data derived-query metadata on repository interface members.
        if context.in_spring_data_repository {
            let method_name = node
                .child_by_field_name("name")
                .and_then(|child| node_text_checked(self.document.text, child))
                .unwrap_or("");
            if spring::is_derived_query_method_name(method_name) {
                self.push_unknown_with(
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "java_spring_data_query_derivation",
                    "spring_data_derived_query",
                    "Spring Data derived-query property paths are resolved by the runtime",
                    vec!["spring_data_derived_query=matched".to_string()],
                )?;
            }
        }

        Ok(())
    }

    fn push_unknown(
        &mut self,
        unit: &CodeUnit,
        reason: UnknownReasonCode,
        affected_claim: &str,
        kind: &str,
        note: &str,
    ) -> Result<(), ParseError> {
        self.semantic_facts.push(unknown_fact(
            &self.document,
            unit,
            reason,
            affected_claim,
            kind,
            note,
            Vec::new(),
        )?);
        Ok(())
    }

    fn push_unknown_with(
        &mut self,
        unit: &CodeUnit,
        reason: UnknownReasonCode,
        affected_claim: &str,
        kind: &str,
        note: &str,
        extra: Vec<String>,
    ) -> Result<(), ParseError> {
        self.semantic_facts.push(unknown_fact(
            &self.document,
            unit,
            reason,
            affected_claim,
            kind,
            note,
            extra,
        )?);
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
    target: &str,
    assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    structural_anchor_fact(
        document,
        unit,
        SemanticFactKind::Type,
        target,
        assumptions,
        "bounded Java Spring class annotation anchor",
    )
}

fn route_anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    target: &str,
    assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    structural_anchor_fact(
        document,
        unit,
        SemanticFactKind::ResolvedCall,
        target,
        assumptions,
        "bounded Java Spring route annotation anchor",
    )
}

pub(crate) fn structural_anchor_fact(
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
    extra_assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    let mut assumptions = vec![
        format!("affected_claim={affected_claim}"),
        format!("java_unknown_kind={kind}"),
    ];
    assumptions.extend(extra_assumptions);
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
        assumptions,
    })
}

fn lombok_annotation_present(annotation_text: &str, imports: &JavaImportContext) -> bool {
    LOMBOK_ANNOTATIONS.iter().any(|annotation| {
        has_exact_direct_annotation(annotation_text, annotation, &[LOMBOK_PACKAGE], imports)
    })
}

// ---------------------------------------------------------------------------
// Shared, framework-agnostic annotation and text utilities.
// ---------------------------------------------------------------------------

pub(crate) fn class_shape_assumptions(
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

pub(crate) fn route_path_shape(segment: &str) -> &'static str {
    let Some(arguments) = annotation_arguments(segment) else {
        return "none";
    };
    let expressions = route_path_argument_expressions(arguments);
    if expressions.is_empty() {
        return "none";
    }
    if expressions
        .iter()
        .all(|expression| route_path_expression_is_literal(expression))
    {
        "literal"
    } else {
        "dynamic"
    }
}

pub(crate) fn annotation_arguments(segment: &str) -> Option<&str> {
    let open = segment.find('(')?;
    let close = segment.rfind(')')?;
    (close > open).then(|| segment[open + 1..close].trim())
}

fn route_path_argument_expressions(arguments: &str) -> Vec<&str> {
    let parts = split_top_level_commas(arguments);
    let named_route_parts = parts
        .iter()
        .filter_map(|part| {
            let (name, value) = split_top_level_assignment(part)?;
            matches!(name.trim(), "value" | "path").then_some(value.trim())
        })
        .collect::<Vec<_>>();
    if !named_route_parts.is_empty() {
        return named_route_parts;
    }
    if parts
        .iter()
        .any(|part| split_top_level_assignment(part).is_some())
    {
        return Vec::new();
    }
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn route_path_expression_is_literal(expression: &str) -> bool {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Some(inner) = trimmed
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        let entries = split_top_level_commas(inner);
        return !entries.is_empty()
            && entries
                .iter()
                .all(|entry| single_string_literal_consumes(entry.trim()));
    }
    single_string_literal_consumes(trimmed)
}

pub(crate) fn split_top_level_commas(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    let trailing = text[start..].trim();
    if !trailing.is_empty() {
        parts.push(trailing);
    }
    parts
}

pub(crate) fn split_top_level_assignment(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            '=' if depth == 0 => return Some((&text[..index], &text[index + 1..])),
            _ => {}
        }
    }
    None
}

pub(crate) fn single_string_literal_consumes(text: &str) -> bool {
    let mut chars = text.char_indices();
    let Some((start_index, character)) = chars.next() else {
        return false;
    };
    if start_index != 0 || character != '"' {
        return false;
    }
    let mut escaped = false;
    for (end_index, character) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            return text[end_index + character.len_utf8()..].trim().is_empty();
        }
    }
    false
}

pub(crate) fn java_visibility_shape(annotations_or_header: &str) -> &'static str {
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

pub(crate) fn java_class_shape(slice: &str) -> &'static str {
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

pub(crate) fn java_return_shape(slice: &str) -> &'static str {
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

pub(crate) fn java_parameter_shape(slice: &str) -> String {
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

pub(crate) fn extends_exact_type(
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

pub(crate) fn has_exact_annotation(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> bool {
    annotation_full_names(annotation_text)
        .iter()
        .any(|full_name| annotation_name_is_exact(full_name, simple_name, packages, imports))
}

pub(crate) fn has_exact_direct_annotation(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> bool {
    direct_annotation_full_names(annotation_text)
        .iter()
        .any(|full_name| annotation_name_is_exact(full_name, simple_name, packages, imports))
}

pub(crate) fn contains_annotation_simple_name(annotation_text: &str, names: &[&str]) -> bool {
    let full_names = annotation_full_names(annotation_text);
    names.iter().any(|name| {
        full_names
            .iter()
            .any(|full_name| full_name.split('.').next_back() == Some(*name))
    })
}

pub(crate) fn annotation_full_names(annotation_text: &str) -> Vec<String> {
    annotation_names_at_indices(annotation_text, annotation_marker_indices(annotation_text))
}

fn direct_annotation_full_names(annotation_text: &str) -> Vec<String> {
    annotation_names_at_indices(
        annotation_text,
        direct_annotation_marker_indices(annotation_text),
    )
}

fn annotation_names_at_indices(
    annotation_text: &str,
    indices: impl IntoIterator<Item = usize>,
) -> Vec<String> {
    let mut names = Vec::new();
    for index in indices {
        let mut cursor = skip_java_trivia(annotation_text, index + 1);
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

pub(crate) fn annotation_segment_exact(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> Option<String> {
    annotation_segments_exact(annotation_text, simple_name, packages, imports)
        .and_then(|segments| segments.into_iter().next())
}

pub(crate) fn annotation_segments_exact(
    annotation_text: &str,
    simple_name: &str,
    packages: &[&str],
    imports: &JavaImportContext,
) -> Option<Vec<String>> {
    const MAX_MATCHING_SEGMENTS: usize = 64;
    const MAX_TOTAL_SEGMENT_BYTES: usize = 64 * 1024;

    let bytes = annotation_text.as_bytes();
    let mut candidates = Vec::new();
    for index in direct_annotation_marker_indices(annotation_text) {
        let mut cursor = skip_java_trivia(annotation_text, index + 1);
        let name_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.')
        {
            cursor += 1;
        }
        let full_name = &annotation_text[name_start..cursor];
        if !annotation_name_is_exact(full_name, simple_name, packages, imports) {
            continue;
        }
        cursor = skip_java_trivia(annotation_text, cursor);
        if candidates.len() == MAX_MATCHING_SEGMENTS {
            return None;
        }
        candidates.push((
            index,
            cursor,
            (bytes.get(cursor) == Some(&b'(')).then_some(cursor),
        ));
    }

    // Resolve only the candidate opening parentheses in one lexical pass.
    // Tracking depth for at most the bounded target set avoids both overlapping
    // suffix rescans and a source-sized close-index allocation.
    let target_opens = candidates
        .iter()
        .filter_map(|(_, _, open)| *open)
        .collect::<BTreeSet<_>>();
    let parenthesis_ends = parenthesis_ends_for(annotation_text, &target_opens);
    let mut ranges = Vec::with_capacity(candidates.len());
    let mut total_segment_bytes = 0usize;
    for (index, cursor, open) in candidates {
        let end = match open {
            Some(open) => parenthesis_ends
                .get(&open)
                .copied()
                .and_then(|end| end.checked_add(1))?,
            None => cursor,
        };
        let segment_bytes = end.checked_sub(index)?;
        total_segment_bytes = total_segment_bytes.checked_add(segment_bytes)?;
        if total_segment_bytes > MAX_TOTAL_SEGMENT_BYTES {
            return None;
        }
        ranges.push((index, end));
    }
    Some(
        ranges
            .into_iter()
            .map(|(start, end)| annotation_text[start..end].to_string())
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JavaLexicalState {
    Normal,
    String,
    Character,
    TextBlock,
    LineComment,
    BlockComment,
}

fn skip_java_trivia(text: &str, mut cursor: usize) -> usize {
    let bytes = text.as_bytes();
    loop {
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'/') {
            cursor += 2;
            while bytes.get(cursor).is_some_and(|byte| *byte != b'\n') {
                cursor += 1;
            }
            continue;
        }
        if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'*') {
            cursor += 2;
            while cursor < bytes.len()
                && !(bytes.get(cursor) == Some(&b'*') && bytes.get(cursor + 1) == Some(&b'/'))
            {
                cursor += 1;
            }
            cursor = cursor.saturating_add(2).min(bytes.len());
            continue;
        }
        return cursor;
    }
}

fn annotation_marker_indices(text: &str) -> Vec<usize> {
    annotation_marker_indices_impl(text, false)
}

fn direct_annotation_marker_indices(text: &str) -> Vec<usize> {
    annotation_marker_indices_impl(text, true)
}

fn annotation_marker_indices_impl(text: &str, direct_only: bool) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut markers = Vec::new();
    let mut state = JavaLexicalState::Normal;
    let mut parenthesis_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match state {
            JavaLexicalState::Normal => match bytes[index] {
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    state = JavaLexicalState::LineComment;
                    index += 2;
                    continue;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    state = JavaLexicalState::BlockComment;
                    index += 2;
                    continue;
                }
                b'"' if bytes.get(index + 1) == Some(&b'"')
                    && bytes.get(index + 2) == Some(&b'"') =>
                {
                    state = JavaLexicalState::TextBlock;
                    index += 3;
                    continue;
                }
                b'"' => state = JavaLexicalState::String,
                b'\'' => state = JavaLexicalState::Character,
                b'(' => parenthesis_depth += 1,
                b')' => parenthesis_depth = parenthesis_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'@' if !direct_only
                    || (parenthesis_depth == 0 && bracket_depth == 0 && brace_depth == 0) =>
                {
                    markers.push(index);
                }
                _ => {}
            },
            JavaLexicalState::String => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'"' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::Character => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'\'' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::TextBlock => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'"'
                    && bytes.get(index + 1) == Some(&b'"')
                    && bytes.get(index + 2) == Some(&b'"')
                {
                    state = JavaLexicalState::Normal;
                    index += 3;
                    continue;
                }
            }
            JavaLexicalState::LineComment => {
                if bytes[index] == b'\n' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    state = JavaLexicalState::Normal;
                    index += 2;
                    continue;
                }
            }
        }
        index += 1;
    }
    markers
}

fn parenthesis_ends_for(text: &str, target_opens: &BTreeSet<usize>) -> BTreeMap<usize, usize> {
    let bytes = text.as_bytes();
    let mut state = JavaLexicalState::Normal;
    let mut depth = 0usize;
    let mut active_targets = BTreeMap::new();
    let mut ends = BTreeMap::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match state {
            JavaLexicalState::Normal => match bytes[index] {
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    state = JavaLexicalState::LineComment;
                    index += 2;
                    continue;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    state = JavaLexicalState::BlockComment;
                    index += 2;
                    continue;
                }
                b'"' if bytes.get(index + 1) == Some(&b'"')
                    && bytes.get(index + 2) == Some(&b'"') =>
                {
                    state = JavaLexicalState::TextBlock;
                    index += 3;
                    continue;
                }
                b'"' => state = JavaLexicalState::String,
                b'\'' => state = JavaLexicalState::Character,
                b'(' => {
                    depth += 1;
                    if target_opens.contains(&index) {
                        active_targets.insert(depth, index);
                    }
                }
                b')' => {
                    if let Some(open) = active_targets.remove(&depth) {
                        ends.insert(open, index);
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            },
            JavaLexicalState::String => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'"' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::Character => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'\'' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::TextBlock => {
                if bytes[index] == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if bytes[index] == b'"'
                    && bytes.get(index + 1) == Some(&b'"')
                    && bytes.get(index + 2) == Some(&b'"')
                {
                    state = JavaLexicalState::Normal;
                    index += 3;
                    continue;
                }
            }
            JavaLexicalState::LineComment => {
                if bytes[index] == b'\n' {
                    state = JavaLexicalState::Normal;
                }
            }
            JavaLexicalState::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    state = JavaLexicalState::Normal;
                    index += 2;
                    continue;
                }
            }
        }
        index += 1;
    }
    ends
}

fn parse_import_context(root: Node<'_>, source: &str) -> JavaImportContext {
    let mut context = JavaImportContext::default();
    let mut cursor = root.walk();
    for import in root
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "import_declaration")
    {
        if import.is_error() || import.is_missing() || import.has_error() {
            continue;
        }
        let Some(line) = node_text_checked(source, import).map(str::trim) else {
            continue;
        };
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
            if let Some(simple_name) = import_path.split('.').next_back() {
                context
                    .exact_imports_by_simple
                    .entry(simple_name.to_string())
                    .or_default()
                    .insert(import_path.to_string());
            }
        }
    }
    context
}

fn collect_local_type_names(root: Node<'_>, source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if matches!(
            node.kind(),
            "class_declaration"
                | "enum_declaration"
                | "interface_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            if let Some(name) = node
                .child_by_field_name("name")
                .and_then(|name| node_text_checked(source, name))
            {
                names.insert(name.to_string());
            }
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    names
}

pub(crate) fn modifier_text(source: &str, node: Node<'_>, fallback_kind: &str) -> String {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "modifiers" {
            return node_text(source, child).to_string();
        }
    }
    let text = node_text(source, node);
    let marker = match (fallback_kind, node.kind()) {
        ("class", "enum_declaration") => "enum",
        ("class", "record_declaration") => "record",
        ("class", _) => "class",
        ("interface", _) => "interface",
        ("method", _) => "(",
        _ => "{",
    };
    text.split(marker).next().unwrap_or("").to_string()
}

pub(crate) fn identifier_tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    node_text_checked(source, node).unwrap_or("")
}

pub(crate) fn node_text_checked<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
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
mod tests;
