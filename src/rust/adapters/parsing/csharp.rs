//! Tree-sitter-backed structural C# code-unit extraction (bounded preview).
//!
//! This adapter does not execute MSBuild, Roslyn, source generators, or the
//! ASP.NET Core runtime, and it never evaluates preprocessor conditions. It
//! emits structural C# units, exact using/FQN-gated ASP.NET Core, EF Core, and
//! xUnit/NUnit/MSTest anchors, and typed UNKNOWN facts for every runtime,
//! generated, or build-variant semantic that remains unresolved.

use super::binding::{self, Annotation, AnnotationSpec, BindingSpec, DirectiveSpec, ImportScope};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::adapters::frameworks::csharp::{
    ASPNET_MVC_NAMESPACE, CONVENTION_ROUTING_METHODS, EFCORE_NAMESPACE, FLUENTVALIDATION_NAMESPACE,
    HTTP_ROUTE_ATTRIBUTES, KNOWN_CLASS_ATTRIBUTES, KNOWN_METHOD_ATTRIBUTES,
    MINIMAL_API_MAP_METHODS, MSTEST_NAMESPACE, NUNIT_NAMESPACE, XUNIT_NAMESPACE,
};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

mod project_config;
mod test_data;

pub(crate) const CSHARP_ANCHOR_ENGINE: &str = "repogrammar-csharp-syntax";
pub(crate) const CSHARP_ANCHOR_METHOD: &str = "tree_sitter_csharp_structural_anchors_v1";

/// C# attribute names: `Name` or `NameAttribute`, optionally dotted.
const ATTRIBUTE_NAMES: BindingSpec = BindingSpec {
    separator: '.',
    fully_qualified_names: true,
    name_suffixes: &["", "Attribute"],
    type_arguments: None,
};
/// C# type names as written in a base list, with optional type arguments.
const TYPE_NAMES: BindingSpec = BindingSpec {
    separator: '.',
    fully_qualified_names: true,
    name_suffixes: &[""],
    type_arguments: Some('<'),
};
/// Exact `using` directives; `using static`, aliases and calls open nothing.
const USING_DIRECTIVES: DirectiveSpec = DirectiveSpec {
    kinds: &["using_directive", "global_using_directive"],
    prefix: "global ",
    keyword: "using ",
    excluded_forms: &["static "],
    excluded_characters: &['=', '('],
    terminator: ';',
    body_kinds: &["declaration_list"],
};
const ATTRIBUTE_LISTS: AnnotationSpec = AnnotationSpec {
    list_kind: "attribute_list",
    item_kind: "attribute",
    name_field: "name",
    arguments_kind: "attribute_argument_list",
};

/// Tree-sitter query selecting every node the scanner handles.
const NODE_QUERY: &str = include_str!("csharp/nodes.scm");

#[derive(Debug, Default)]
pub struct CSharpSyntaxParser {
    /// Compiling the node query costs far more than parsing a typical file,
    /// so each parser instance compiles it once.
    node_query: OnceLock<Query>,
}

impl SourceParser for CSharpSyntaxParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        if document.language != Language::CSharp {
            return Err(ParseError::UnsupportedLanguage);
        }
        if is_csproj_document(&document) {
            return project_config::parse(document);
        }
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .map_err(|error| ParseError::Internal(format!("load C# grammar: {error}")))?;
        let Some(tree) = parser.parse(document.text, None) else {
            return Err(ParseError::Internal(
                "Tree-sitter C# parse failed".to_string(),
            ));
        };

        let mut scanner = CSharpTreeScanner::new(document);
        scanner.scan_tree(tree.root_node(), self.node_query()?)?;
        scanner.finish()
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        if document.language == Language::CSharp && is_csproj_document(&document) {
            return project_config::parse_output(document);
        }
        self.parse_with_context(document, context)
            .map(SourceParseOutput::from_report)
    }
}

impl CSharpSyntaxParser {
    fn node_query(&self) -> Result<&Query, ParseError> {
        if let Some(query) = self.node_query.get() {
            return Ok(query);
        }
        let query = Query::new(&tree_sitter_c_sharp::LANGUAGE.into(), NODE_QUERY)
            .map_err(|error| ParseError::Internal(format!("compile C# node query: {error}")))?;
        Ok(self.node_query.get_or_init(|| query))
    }
}

fn is_csproj_document(document: &SourceDocument<'_>) -> bool {
    document
        .path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.ends_with(".csproj"))
}

#[derive(Debug, Clone, Default)]
struct VisitContext {
    in_aspnet_controller: bool,
    class_route_template_shape: &'static str,
    in_efcore_db_context: bool,
    in_mstest_test_class: bool,
    usings: Arc<ImportScope>,
    xunit_member_data_scope: Arc<test_data::MemberDataScope>,
}

struct CSharpTreeScanner<'a> {
    document: SourceDocument<'a>,
    units: Vec<CodeUnit>,
    semantic_facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
    module_unit: Option<CodeUnit>,
    /// Same-file `variable name -> initializer text`; `None` marks a name bound
    /// more than once with different initializers (conservatively unresolved).
    receiver_bindings: BTreeMap<String, Option<String>>,
    /// Byte ranges covered by `#if`..`#endif` conditional regions.
    conditional_regions: Vec<(usize, usize)>,
    emitted_module_unknowns: BTreeSet<&'static str>,
}

impl<'a> CSharpTreeScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        let conditional_regions = conditional_preproc_regions(document.text);
        Self {
            document,
            units: Vec::new(),
            semantic_facts: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
            module_unit: None,
            receiver_bindings: BTreeMap::new(),
            conditional_regions,
            emitted_module_unknowns: BTreeSet::new(),
        }
    }

    fn scan_tree(&mut self, root: Node<'_>, query: &Query) -> Result<(), ParseError> {
        let module_unit =
            self.add_unit(CodeUnitKind::Module, "file", 0, self.document.text.len())?;
        self.module_unit = Some(module_unit);
        if root.has_error() {
            self.diagnostics.push(ParseDiagnostic {
                path: self.document.path.to_string(),
                range: None,
                severity: ParseDiagnosticSeverity::Warning,
                message: "Tree-sitter C# parse contains syntax errors; extraction is structural"
                    .to_string(),
            });
        }
        let captures = node_captures(root, self.document.text, query)?;
        // Receiver bindings are order-independent and must be complete before
        // any invocation is classified.
        for (capture, node) in &captures {
            if *capture == "binding" {
                self.record_receiver_binding(*node);
            }
        }
        // Pre-order: an ancestor precedes its descendants, including one that
        // spans the identical byte range.
        let mut ordered = captures
            .into_iter()
            .filter(|(capture, _)| *capture != "binding")
            .collect::<Vec<_>>();
        ordered.sort_by(|(_, left), (_, right)| {
            (left.start_byte(), Reverse(left.end_byte()))
                .cmp(&(right.start_byte(), Reverse(right.end_byte())))
                .then_with(|| node_depth(*left).cmp(&node_depth(*right)))
        });
        // Each node inherits the context of its nearest scope or class-like
        // ancestor.
        let mut contexts = HashMap::new();
        for (capture, node) in ordered {
            let context = inherited_context(node, &contexts);
            match capture {
                "scope" => {
                    let mut next = context.clone();
                    next.usings = Arc::new(binding::scope_imports(
                        self.document.text,
                        node,
                        Arc::clone(&context.usings),
                        &USING_DIRECTIVES,
                        &TYPE_NAMES,
                    ));
                    contexts.insert(node.id(), next);
                }
                "class_like" => {
                    let next = self.scan_class_like(node, &context)?;
                    contexts.insert(node.id(), next);
                }
                "method" => self.scan_method(node, &context)?,
                "constructor" => {
                    self.add_named_node_unit(node, CodeUnitKind::Method, "constructor")?;
                }
                "property" => self.scan_property(node, &context)?,
                "invocation" => self.scan_invocation(node)?,
                other => {
                    return Err(ParseError::Internal(format!(
                        "unexpected C# node capture {other}"
                    )))
                }
            }
        }
        Ok(())
    }

    fn record_receiver_binding(&mut self, node: Node<'_>) {
        let name = node
            .child_by_field_name("name")
            .and_then(|child| node_text_checked(self.document.text, child))
            .map(str::to_string);
        let text = node_text(self.document.text, node);
        let initializer = text
            .split_once('=')
            .map(|(_, initializer)| initializer.trim().to_string());
        if let (Some(name), Some(initializer)) = (name, initializer) {
            match self.receiver_bindings.get(&name) {
                None => {
                    self.receiver_bindings.insert(name, Some(initializer));
                }
                Some(Some(existing)) if *existing == initializer => {}
                Some(_) => {
                    self.receiver_bindings.insert(name, None);
                }
            }
        }
    }

    fn scan_class_like(
        &mut self,
        node: Node<'_>,
        context: &VisitContext,
    ) -> Result<VisitContext, ParseError> {
        let attributes = binding::annotations(self.document.text, node, &ATTRIBUTE_LISTS);
        let modifiers = modifier_tokens(self.document.text, node);
        let bases = binding::listed_names(self.document.text, node, "base_list");
        let xunit_member_data_scope = self.xunit_member_data_scope(node, &modifiers, &bases);
        let class_route_template_shape = context
            .usings
            .exact_annotation(&ATTRIBUTE_NAMES, &attributes, "Route", ASPNET_MVC_NAMESPACE)
            .map(|attribute| route_template_shape(attribute.arguments.as_deref()))
            .unwrap_or("none");

        let controller_target = if context
            .usings
            .exact_annotation(
                &ATTRIBUTE_NAMES,
                &attributes,
                "ApiController",
                ASPNET_MVC_NAMESPACE,
            )
            .is_some()
        {
            Some(("aspnetcore.mvc.ApiController", "ApiController"))
        } else if context.usings.any_denotes(
            &TYPE_NAMES,
            &bases,
            "ControllerBase",
            ASPNET_MVC_NAMESPACE,
        ) {
            Some(("aspnetcore.mvc.ControllerBase", "ControllerBase"))
        } else if context.usings.any_denotes(
            &TYPE_NAMES,
            &bases,
            "Controller",
            ASPNET_MVC_NAMESPACE,
        ) {
            Some(("aspnetcore.mvc.Controller", "Controller"))
        } else {
            None
        };
        let db_context = controller_target.is_none()
            && node.kind() == "class_declaration"
            && context
                .usings
                .any_denotes(&TYPE_NAMES, &bases, "DbContext", EFCORE_NAMESPACE);

        // Ordered after the ASP.NET and EF Core arms so one class never claims
        // two framework roles: a unit holding two is dropped from family
        // support silently instead of reporting a conflict.
        let fluent_validator = controller_target.is_none()
            && !db_context
            && node.kind() == "class_declaration"
            && context.usings.any_denotes(
                &TYPE_NAMES,
                &bases,
                "AbstractValidator",
                FLUENTVALIDATION_NAMESPACE,
            );

        let kind = if controller_target.is_some() {
            CodeUnitKind::AspNetController
        } else if db_context {
            CodeUnitKind::EfCoreDbContext
        } else if fluent_validator {
            CodeUnitKind::FluentValidationValidator
        } else {
            CodeUnitKind::Class
        };
        let unit = self.add_named_node_unit(node, kind, "class")?;
        let class_shape = csharp_class_shape(node.kind(), &modifiers);
        let visibility = csharp_visibility_shape(&modifiers);

        if let Some((target, anchor_attribute)) = controller_target {
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::Type,
                target,
                vec![
                    "provider_resolved=false".to_string(),
                    "csharp_anchor_kind=aspnet_controller".to_string(),
                    format!("aspnet_attribute={anchor_attribute}"),
                    format!("class_route_template_shape={class_route_template_shape}"),
                    format!("csharp_visibility_shape={visibility}"),
                    format!("csharp_class_shape={class_shape}"),
                ],
                "bounded C# ASP.NET Core controller anchor",
            )?);
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::RuntimeDependencyInjection,
                "csharp_di_registration",
                "aspnet_runtime_dependency_injection",
                "ASP.NET Core dependency injection and assembly scanning are runtime behavior",
                Vec::new(),
            )?);
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "csharp_aspnet_filter_pipeline",
                "aspnet_filter_pipeline",
                "ASP.NET Core middleware and filter pipeline semantics are runtime behavior",
                Vec::new(),
            )?);
            self.emit_anchored_unit_boundaries(&unit, &modifiers)?;
        } else if db_context {
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::Type,
                "efcore.DbContext",
                vec![
                    "provider_resolved=false".to_string(),
                    "csharp_anchor_kind=efcore_db_context".to_string(),
                    format!("csharp_visibility_shape={visibility}"),
                    format!("csharp_class_shape={class_shape}"),
                ],
                "bounded C# EF Core DbContext anchor",
            )?);
            self.emit_anchored_unit_boundaries(&unit, &modifiers)?;
        } else if fluent_validator {
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::Type,
                "fluentvalidation.AbstractValidator",
                vec![
                    "provider_resolved=false".to_string(),
                    "csharp_anchor_kind=fluentvalidation_validator".to_string(),
                    format!("csharp_visibility_shape={visibility}"),
                    format!("csharp_class_shape={class_shape}"),
                ],
                "bounded C# FluentValidation validator anchor",
            )?);
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "csharp_fluentvalidation_rule_chain",
                "fluentvalidation_rule_chain",
                "FluentValidation rule builder chains and validator resolution are runtime behavior",
                Vec::new(),
            )?);
            self.emit_anchored_unit_boundaries(&unit, &modifiers)?;
        } else if context.usings.has_unbound_lookalike(
            &ATTRIBUTE_NAMES,
            &attributes,
            KNOWN_CLASS_ATTRIBUTES,
        ) {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "csharp_attribute_binding",
                "class_attribute_unresolved_using",
                "C# class attribute short name lacks an exact same-file using or FQN",
                Vec::new(),
            )?);
        }
        if bases
            .iter()
            .any(|base| TYPE_NAMES.looks_like(base, "JsonSerializerContext"))
        {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "csharp_generated_source",
                "source_generator_context",
                "Roslyn source generator output is not generated or simulated",
                Vec::new(),
            )?);
        }

        Ok(VisitContext {
            in_aspnet_controller: controller_target.is_some(),
            class_route_template_shape,
            in_efcore_db_context: db_context,
            in_mstest_test_class: context
                .usings
                .exact_annotation(&ATTRIBUTE_NAMES, &attributes, "TestClass", MSTEST_NAMESPACE)
                .is_some(),
            usings: Arc::clone(&context.usings),
            xunit_member_data_scope: Arc::new(xunit_member_data_scope),
        })
    }

    fn scan_method(&mut self, node: Node<'_>, context: &VisitContext) -> Result<(), ParseError> {
        let attributes = binding::annotations(self.document.text, node, &ATTRIBUTE_LISTS);
        let modifiers = modifier_tokens(self.document.text, node);
        let route = HTTP_ROUTE_ATTRIBUTES
            .iter()
            .find_map(|(attribute, target, http_method)| {
                context
                    .usings
                    .exact_annotation(
                        &ATTRIBUTE_NAMES,
                        &attributes,
                        attribute,
                        ASPNET_MVC_NAMESPACE,
                    )
                    .map(|found| {
                        (
                            *attribute,
                            *target,
                            *http_method,
                            route_template_shape(found.arguments.as_deref()),
                        )
                    })
            });
        let test = if route.is_none() {
            self.test_method_anchor(&attributes, context)
        } else {
            None
        };

        let kind = match (&route, &test) {
            (Some(_), _) if context.in_aspnet_controller => CodeUnitKind::AspNetControllerAction,
            (Some(_), _) => CodeUnitKind::Method,
            (None, Some(anchor)) if anchor.blocked_claim.is_none() => anchor.unit_kind.clone(),
            _ => CodeUnitKind::Method,
        };
        let unit = self.add_named_node_unit(node, kind, "method")?;
        let slice = node_text(self.document.text, node);
        let visibility = csharp_visibility_shape(&modifiers);
        let return_shape = csharp_return_shape(self.document.text, node);
        let parameter_shape = csharp_parameter_shape(node);

        if let Some((attribute, target, http_method, template_shape)) = route {
            if context.in_aspnet_controller {
                self.semantic_facts.push(structural_anchor_fact(
                    &self.document,
                    &unit,
                    SemanticFactKind::ResolvedCall,
                    target,
                    vec![
                        "provider_resolved=false".to_string(),
                        "csharp_anchor_kind=aspnet_controller_action".to_string(),
                        format!("aspnet_attribute={attribute}"),
                        format!("http_method={http_method}"),
                        format!("route_template_shape={template_shape}"),
                        format!(
                            "class_route_template_shape={}",
                            context.class_route_template_shape
                        ),
                        format!("csharp_visibility_shape={visibility}"),
                        format!("csharp_return_shape={return_shape}"),
                        format!("csharp_parameter_shape={parameter_shape}"),
                    ],
                    "bounded C# ASP.NET Core route attribute anchor",
                )?);
                if template_shape == "dynamic" {
                    self.semantic_facts.push(unknown_fact(
                        &self.document,
                        &unit,
                        UnknownReasonCode::FrameworkMagic,
                        "csharp_aspnet_route_template",
                        "non_literal_route_template",
                        "ASP.NET Core route template is not a direct string literal",
                        Vec::new(),
                    )?);
                }
                self.emit_anchored_method_boundaries(&unit, slice)?;
            } else {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "csharp_controller_identity",
                    "route_attribute_without_controller",
                    "ASP.NET Core route attribute appeared outside an exact controller class",
                    Vec::new(),
                )?);
            }
            return Ok(());
        }

        if let Some(anchor) = test {
            if let Some((reason, affected_claim, unknown_kind, note)) = anchor.blocked_claim {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    reason,
                    affected_claim,
                    unknown_kind,
                    note,
                    Vec::new(),
                )?);
                return Ok(());
            }
            let mut assumptions = vec![
                "provider_resolved=false".to_string(),
                format!("csharp_anchor_kind={}", anchor.unit_kind.as_str()),
                format!("test_attribute={}", anchor.attribute),
                format!("test_data_shape={}", anchor.data_shape),
                format!("csharp_visibility_shape={visibility}"),
                format!("csharp_return_shape={return_shape}"),
                format!("csharp_parameter_shape={parameter_shape}"),
            ];
            assumptions.extend(anchor.member_data.exact_assumptions());
            self.semantic_facts.push(structural_anchor_fact(
                &self.document,
                &unit,
                SemanticFactKind::ResolvedCall,
                anchor.target,
                assumptions,
                "bounded C# test attribute anchor",
            )?);
            if let test_data::MemberDataResolution::Unknown(unknown) = anchor.member_data {
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &unit,
                    UnknownReasonCode::FrameworkMagic,
                    "csharp_test_member_data",
                    unknown.kind,
                    unknown.note,
                    Vec::new(),
                )?);
            }
            self.emit_anchored_method_boundaries(&unit, slice)?;
            return Ok(());
        }

        if context
            .usings
            .exact_annotation(&ATTRIBUTE_NAMES, &attributes, "MemberData", XUNIT_NAMESPACE)
            .is_some()
        {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "csharp_test_member_data",
                "xunit_member_data_without_theory",
                "xUnit MemberData appeared without an exact Theory attribute",
                Vec::new(),
            )?);
        } else if context.usings.has_unbound_lookalike(
            &ATTRIBUTE_NAMES,
            &attributes,
            KNOWN_METHOD_ATTRIBUTES,
        ) {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "csharp_attribute_binding",
                "method_attribute_unresolved_using",
                "C# method attribute short name lacks an exact same-file using or FQN",
                Vec::new(),
            )?);
        }
        Ok(())
    }

    fn test_method_anchor(
        &self,
        attributes: &[Annotation],
        context: &VisitContext,
    ) -> Option<TestMethodAnchor> {
        let exact = |simple: &str, namespace: &str| {
            context
                .usings
                .exact_annotation(&ATTRIBUTE_NAMES, attributes, simple, namespace)
                .is_some()
        };
        if exact("Fact", XUNIT_NAMESPACE) || exact("Theory", XUNIT_NAMESPACE) {
            let theory = exact("Theory", XUNIT_NAMESPACE);
            let member_data_attributes = attributes
                .iter()
                .filter(|attribute| {
                    context.usings.annotation_is_exact(
                        &ATTRIBUTE_NAMES,
                        attribute,
                        "MemberData",
                        XUNIT_NAMESPACE,
                    )
                })
                .collect::<Vec<_>>();
            let has_non_exact_member_data = attributes.iter().any(|attribute| {
                ATTRIBUTE_NAMES.looks_like(&attribute.name, "MemberData")
                    && !context.usings.annotation_is_exact(
                        &ATTRIBUTE_NAMES,
                        attribute,
                        "MemberData",
                        XUNIT_NAMESPACE,
                    )
            });
            let member_data = if theory {
                if has_non_exact_member_data {
                    test_data::MemberDataResolution::Unknown(test_data::MemberDataUnknown {
                        kind: "xunit_member_data_unresolved_attribute",
                        note: "A MemberData-like attribute lacks an exact same-file using or FQN",
                    })
                } else {
                    test_data::resolve(
                        member_data_attributes
                            .iter()
                            .map(|attribute| attribute.arguments.as_deref()),
                        context.xunit_member_data_scope.as_ref(),
                    )
                }
            } else if member_data_attributes.is_empty() && !has_non_exact_member_data {
                test_data::MemberDataResolution::Absent
            } else {
                test_data::MemberDataResolution::Unknown(test_data::MemberDataUnknown {
                    kind: "xunit_member_data_requires_theory",
                    note: "xUnit MemberData is only valid on a Theory test",
                })
            };
            return Some(TestMethodAnchor {
                unit_kind: CodeUnitKind::XunitTestMethod,
                target: if theory { "xunit.Theory" } else { "xunit.Fact" },
                attribute: if theory { "Theory" } else { "Fact" },
                data_shape: if theory {
                    member_data.data_shape()
                } else {
                    "fact"
                },
                member_data,
                blocked_claim: None,
            });
        }
        if exact("Test", NUNIT_NAMESPACE) || exact("TestCase", NUNIT_NAMESPACE) {
            let test_case = exact("TestCase", NUNIT_NAMESPACE);
            return Some(TestMethodAnchor {
                unit_kind: CodeUnitKind::NunitTestMethod,
                target: if test_case {
                    "nunit.framework.TestCase"
                } else {
                    "nunit.framework.Test"
                },
                attribute: if test_case { "TestCase" } else { "Test" },
                data_shape: if test_case { "test_case" } else { "test" },
                member_data: test_data::MemberDataResolution::Absent,
                blocked_claim: None,
            });
        }
        if exact("TestMethod", MSTEST_NAMESPACE) {
            if !context.in_mstest_test_class {
                return Some(TestMethodAnchor {
                    unit_kind: CodeUnitKind::MstestTestMethod,
                    target: "mstest.unittesting.TestMethod",
                    attribute: "TestMethod",
                    data_shape: "none",
                    member_data: test_data::MemberDataResolution::Absent,
                    blocked_claim: Some((
                        UnknownReasonCode::FrameworkMagic,
                        "csharp_test_class_identity",
                        "mstest_method_without_test_class",
                        "MSTest [TestMethod] appeared outside an exact [TestClass] class",
                    )),
                });
            }
            let data_row = exact("DataRow", MSTEST_NAMESPACE);
            return Some(TestMethodAnchor {
                unit_kind: CodeUnitKind::MstestTestMethod,
                target: "mstest.unittesting.TestMethod",
                attribute: "TestMethod",
                data_shape: if data_row { "data_row" } else { "none" },
                member_data: test_data::MemberDataResolution::Absent,
                blocked_claim: None,
            });
        }
        None
    }

    fn xunit_member_data_scope(
        &self,
        class: Node<'_>,
        class_modifiers: &[String],
        bases: &[String],
    ) -> test_data::MemberDataScope {
        let mut scope = test_data::MemberDataScope::default();
        let has_type_parameters = {
            let mut cursor = class.walk();
            let found = class
                .named_children(&mut cursor)
                .any(|child| child.kind() == "type_parameter_list");
            found
        };
        if class.kind() != "class_declaration"
            || class.has_error()
            || class_modifiers.iter().any(|modifier| modifier == "partial")
            || !bases.is_empty()
            || has_type_parameters
        {
            scope.mark_open_world();
        }
        let body = class.child_by_field_name("body").or_else(|| {
            let mut cursor = class.walk();
            let found = class
                .named_children(&mut cursor)
                .find(|child| child.kind() == "declaration_list");
            found
        });
        let Some(body) = body else {
            scope.mark_open_world();
            return scope;
        };
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            self.record_xunit_member_node(&mut scope, member, false);
        }
        scope
    }

    fn record_xunit_member_node(
        &self,
        scope: &mut test_data::MemberDataScope,
        member: Node<'_>,
        inside_preprocessor: bool,
    ) {
        if matches!(
            member.kind(),
            "class_declaration"
                | "record_declaration"
                | "struct_declaration"
                | "interface_declaration"
                | "enum_declaration"
        ) {
            return;
        }
        if member.kind().starts_with("preproc_")
            || (inside_preprocessor && member.kind() == "declaration_list")
        {
            let mut cursor = member.walk();
            for child in member.named_children(&mut cursor) {
                self.record_xunit_member_node(scope, child, true);
            }
            return;
        }

        let kind = match member.kind() {
            "property_declaration" => Some(test_data::MemberSourceKind::Property),
            "method_declaration" => Some(test_data::MemberSourceKind::Method),
            "field_declaration" => Some(test_data::MemberSourceKind::Field),
            _ => None,
        };
        let Some(kind) = kind else {
            return;
        };
        let modifiers = modifier_tokens(self.document.text, member);
        let mut eligible = !member.has_error()
            && modifiers.iter().any(|modifier| modifier == "public")
            && modifiers.iter().any(|modifier| modifier == "static");
        if kind == test_data::MemberSourceKind::Method {
            let has_type_parameters = {
                let mut cursor = member.walk();
                let found = member
                    .named_children(&mut cursor)
                    .any(|child| child.kind() == "type_parameter_list");
                found
            };
            eligible &= csharp_parameter_shape(member) == "arity_0" && !has_type_parameters;
        }
        let conditional = inside_preprocessor
            || range_intersects_regions(
                &self.conditional_regions,
                member.start_byte(),
                member.end_byte(),
            );
        if kind == test_data::MemberSourceKind::Field {
            for name in field_declarator_names(self.document.text, member) {
                scope.record(&name, kind, eligible, conditional);
            }
        } else if let Some(name) = member
            .child_by_field_name("name")
            .and_then(|name| node_text_checked(self.document.text, name))
        {
            scope.record(name, kind, eligible, conditional);
        }
    }

    fn scan_property(&mut self, node: Node<'_>, context: &VisitContext) -> Result<(), ParseError> {
        if !context.in_efcore_db_context {
            return Ok(());
        }
        let Some(entity_type_shape) = db_set_entity_type_shape(self.document.text, node) else {
            return Ok(());
        };
        let unit = self.add_named_node_unit(node, CodeUnitKind::EfCoreEntitySet, "property")?;
        let modifiers = modifier_tokens(self.document.text, node);
        self.semantic_facts.push(structural_anchor_fact(
            &self.document,
            &unit,
            SemanticFactKind::ResolvedCall,
            "efcore.DbSet",
            vec![
                "provider_resolved=false".to_string(),
                "csharp_anchor_kind=efcore_entity_set".to_string(),
                format!("efcore_entity_type_shape={entity_type_shape}"),
                format!(
                    "csharp_visibility_shape={}",
                    csharp_visibility_shape(&modifiers)
                ),
            ],
            "bounded C# EF Core DbSet entity set anchor",
        )?);
        self.emit_anchored_unit_boundaries(&unit, &modifiers)?;
        Ok(())
    }

    fn scan_invocation(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        let Some(function) = node.child_by_field_name("function") else {
            return Ok(());
        };
        if function.kind() != "member_access_expression" {
            return Ok(());
        }
        let receiver = function
            .child_by_field_name("expression")
            .filter(|child| child.kind() == "identifier")
            .and_then(|child| node_text_checked(self.document.text, child));
        let method_name = function
            .child_by_field_name("name")
            .and_then(|child| node_text_checked(self.document.text, child))
            .map(|name| name.split('<').next().unwrap_or(name).trim().to_string());
        let Some(method_name) = method_name else {
            return Ok(());
        };

        if CONVENTION_ROUTING_METHODS.contains(&method_name.as_str()) {
            if self.emitted_module_unknowns.insert("convention_routing") {
                let module_unit = self.module_unit.clone().expect("module unit exists");
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &module_unit,
                    UnknownReasonCode::RuntimeDependencyInjection,
                    "csharp_aspnet_convention_routing",
                    "aspnet_convention_routing",
                    "ASP.NET Core convention routing and hub/service mapping are runtime behavior",
                    Vec::new(),
                )?);
            }
            return Ok(());
        }

        let Some((map_method, target, http_method)) = MINIMAL_API_MAP_METHODS
            .iter()
            .find(|(name, _, _)| *name == method_name)
            .copied()
        else {
            return Ok(());
        };
        let receiver_resolves =
            receiver.is_some_and(|name| self.receiver_is_web_application(name, 4));
        if !receiver_resolves {
            if self.emitted_module_unknowns.insert("minimal_api_receiver") {
                let module_unit = self.module_unit.clone().expect("module unit exists");
                self.semantic_facts.push(unknown_fact(
                    &self.document,
                    &module_unit,
                    UnknownReasonCode::UnresolvedImport,
                    "csharp_minimal_api_receiver",
                    "minimal_api_receiver_unresolved",
                    "Minimal API Map call receiver does not resolve to a same-file WebApplication builder chain",
                    Vec::new(),
                )?);
            }
            return Ok(());
        }

        let template_shape = node
            .child_by_field_name("arguments")
            .map(|arguments| {
                invocation_route_template_shape(node_text(self.document.text, arguments))
            })
            .unwrap_or("none");
        let unit = self.add_unit(
            CodeUnitKind::AspNetMinimalApiRoute,
            map_method,
            node.start_byte(),
            node.end_byte(),
        )?;
        self.semantic_facts.push(structural_anchor_fact(
            &self.document,
            &unit,
            SemanticFactKind::ResolvedCall,
            target,
            vec![
                "provider_resolved=false".to_string(),
                "csharp_anchor_kind=aspnet_minimal_api_route".to_string(),
                format!("http_method={http_method}"),
                format!("route_template_shape={template_shape}"),
            ],
            "bounded C# ASP.NET Core minimal API route anchor",
        )?);
        if template_shape != "literal" {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                &unit,
                UnknownReasonCode::FrameworkMagic,
                "csharp_aspnet_route_template",
                "non_literal_route_template",
                "ASP.NET Core minimal API route template is not a direct string literal",
                Vec::new(),
            )?);
        }
        self.maybe_emit_build_variant_unknown(&unit)?;
        Ok(())
    }

    fn receiver_is_web_application(&self, name: &str, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let Some(Some(initializer)) = self.receiver_bindings.get(name) else {
            return false;
        };
        if initializer.contains("WebApplication.CreateBuilder") && initializer.contains(".Build()")
        {
            return true;
        }
        let trimmed = initializer.trim().trim_end_matches(';').trim();
        if let Some(prefix) = trimmed.strip_suffix(".Build()") {
            let prefix = prefix.trim();
            return is_identifier(prefix) && self.binding_is_builder(prefix, depth - 1);
        }
        false
    }

    fn binding_is_builder(&self, name: &str, depth: usize) -> bool {
        if depth == 0 {
            return false;
        }
        let Some(Some(initializer)) = self.receiver_bindings.get(name) else {
            return false;
        };
        if initializer.contains("WebApplication.CreateBuilder") {
            return true;
        }
        let trimmed = initializer.trim().trim_end_matches(';').trim();
        is_identifier(trimmed) && self.binding_is_builder(trimmed, depth - 1)
    }

    fn emit_anchored_unit_boundaries(
        &mut self,
        unit: &CodeUnit,
        modifiers: &[String],
    ) -> Result<(), ParseError> {
        if modifiers.iter().any(|modifier| modifier == "partial") {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "csharp_partial_external",
                "partial_declaration_external_half",
                "Other partial declaration halves or generated members may exist outside this file",
                Vec::new(),
            )?);
        }
        self.maybe_emit_build_variant_unknown(unit)
    }

    fn emit_anchored_method_boundaries(
        &mut self,
        unit: &CodeUnit,
        slice: &str,
    ) -> Result<(), ParseError> {
        if contains_generated_regex_marker(slice) {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "csharp_generated_source",
                "source_generator_marker",
                "Roslyn source generator output is not generated or simulated",
                Vec::new(),
            )?);
        }
        if identifier_tokens(slice)
            .iter()
            .any(|token| token == "dynamic")
        {
            self.semantic_facts.push(unknown_fact(
                &self.document,
                unit,
                UnknownReasonCode::FrameworkMagic,
                "csharp_dynamic_binding",
                "dynamic_member_binding",
                "C# dynamic member binding is resolved by the runtime binder",
                Vec::new(),
            )?);
        }
        self.maybe_emit_build_variant_unknown(unit)
    }

    fn maybe_emit_build_variant_unknown(&mut self, unit: &CodeUnit) -> Result<(), ParseError> {
        let intersects = range_intersects_regions(
            &self.conditional_regions,
            unit.range.start_byte,
            unit.range.end_byte,
        );
        if !intersects {
            return Ok(());
        }
        self.semantic_facts.push(unknown_fact(
            &self.document,
            unit,
            UnknownReasonCode::BuildVariantAmbiguity,
            "csharp_build_variant",
            "conditional_compilation_region",
            "C# preprocessor conditional selects a build variant that is never evaluated",
            vec!["csharp_preprocessor_shape=conditional".to_string()],
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
            language: Language::CSharp,
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

fn node_captures<'tree>(
    root: Node<'tree>,
    source: &str,
    query: &Query,
) -> Result<Vec<(&'static str, Node<'tree>)>, ParseError> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut captures = Vec::new();
    let mut matches = cursor.matches(query, root, source.as_bytes());
    while let Some(found) = matches.next() {
        for capture in found.captures() {
            let name = match names[capture.index as usize] {
                "binding" => "binding",
                "scope" => "scope",
                "class_like" => "class_like",
                "method" => "method",
                "constructor" => "constructor",
                "property" => "property",
                "invocation" => "invocation",
                other => {
                    return Err(ParseError::Internal(format!(
                        "unexpected C# node capture {other}"
                    )))
                }
            };
            captures.push((name, capture.node));
        }
    }
    if cursor.did_exceed_match_limit() {
        return Err(ParseError::Internal(
            "C# node query exceeded its match limit".to_string(),
        ));
    }
    Ok(captures)
}

fn inherited_context(node: Node<'_>, contexts: &HashMap<usize, VisitContext>) -> VisitContext {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if let Some(context) = contexts.get(&current.id()) {
            return context.clone();
        }
        ancestor = current.parent();
    }
    VisitContext::default()
}

fn node_depth(node: Node<'_>) -> usize {
    let mut depth = 0;
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        depth += 1;
        ancestor = current.parent();
    }
    depth
}

#[derive(Debug, Clone)]
struct TestMethodAnchor {
    unit_kind: CodeUnitKind,
    target: &'static str,
    attribute: &'static str,
    data_shape: &'static str,
    member_data: test_data::MemberDataResolution,
    blocked_claim: Option<(UnknownReasonCode, &'static str, &'static str, &'static str)>,
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
            engine: CSHARP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CSHARP_ANCHOR_METHOD.to_string(),
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
        format!("csharp_unknown_kind={kind}"),
    ];
    assumptions.extend(extra_assumptions);
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: CSHARP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CSHARP_ANCHOR_METHOD.to_string(),
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

/// Byte ranges of `#if`..`#endif` conditional regions, computed lexically and
/// applied span-scoped per unit. Conditions are recorded, never evaluated.
fn conditional_preproc_regions(source: &str) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut stack = Vec::new();
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if is_preproc_directive(trimmed, "#if") {
            stack.push(offset);
        } else if is_preproc_directive(trimmed, "#endif") {
            if let Some(start) = stack.pop() {
                regions.push((start, offset + line.len()));
            }
        }
        offset += line.len();
    }
    for start in stack {
        regions.push((start, source.len()));
    }
    regions.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(regions.len());
    for (start, end) in regions {
        if let Some((_, merged_end)) = merged.last_mut() {
            if start <= *merged_end {
                *merged_end = (*merged_end).max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

fn range_intersects_regions(
    regions: &[(usize, usize)],
    start_byte: usize,
    end_byte: usize,
) -> bool {
    let candidate = regions.partition_point(|(_, end)| *end <= start_byte);
    regions
        .get(candidate)
        .is_some_and(|(start, _)| *start < end_byte)
}

fn is_preproc_directive(trimmed_line: &str, directive: &str) -> bool {
    trimmed_line.strip_prefix(directive).is_some_and(|rest| {
        rest.is_empty()
            || rest.starts_with(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '!' | '(')
            })
    })
}

fn contains_generated_regex_marker(slice: &str) -> bool {
    identifier_tokens(slice)
        .iter()
        .any(|token| token == "GeneratedRegex" || token == "GeneratedRegexAttribute")
}

fn modifier_tokens(source: &str, node: Node<'_>) -> Vec<String> {
    let mut modifiers = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "modifier" {
            modifiers.push(node_text(source, child).trim().to_string());
        }
    }
    modifiers
}

fn field_declarator_names(source: &str, field: Node<'_>) -> Vec<String> {
    let mut field_cursor = field.walk();
    let declaration = field
        .named_children(&mut field_cursor)
        .find(|child| child.kind() == "variable_declaration");
    let Some(declaration) = declaration else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut declaration_cursor = declaration.walk();
    for declarator in declaration.named_children(&mut declaration_cursor) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        if let Some(name) = declarator
            .child_by_field_name("name")
            .and_then(|name| node_text_checked(source, name))
            .map(str::to_string)
            .or_else(|| first_identifier_text(source, declarator))
        {
            names.push(name);
        }
    }
    names
}

fn csharp_visibility_shape(modifiers: &[String]) -> &'static str {
    if modifiers.iter().any(|modifier| modifier == "public") {
        "public"
    } else if modifiers.iter().any(|modifier| modifier == "protected") {
        "protected"
    } else if modifiers.iter().any(|modifier| modifier == "private") {
        "private"
    } else if modifiers.iter().any(|modifier| modifier == "internal") {
        "internal"
    } else {
        "default"
    }
}

fn csharp_class_shape(node_kind: &str, modifiers: &[String]) -> &'static str {
    match node_kind {
        "record_declaration" => "record",
        "struct_declaration" => "struct",
        "interface_declaration" => "interface",
        _ if modifiers.iter().any(|modifier| modifier == "partial") => "partial_class",
        _ => "class",
    }
}

fn csharp_return_shape(source: &str, node: Node<'_>) -> &'static str {
    let header = method_header(source, node);
    if header.contains("ActionResult")
        || header.contains("IActionResult")
        || header.contains("IResult")
    {
        "action_result"
    } else if header.contains("Task") || header.contains("ValueTask") {
        "task"
    } else if identifier_tokens(&header)
        .iter()
        .any(|token| token == "void")
    {
        "void"
    } else if header.contains("string")
        || header.contains("object")
        || header.contains("List<")
        || header.contains("IEnumerable<")
    {
        "object"
    } else if identifier_tokens(&header).iter().any(|token| {
        matches!(
            token.as_str(),
            "int" | "long" | "bool" | "double" | "decimal"
        )
    }) {
        "primitive"
    } else {
        "unknown"
    }
}

/// Method text between the last attribute list and the parameter list: the
/// modifier + return type + name header, without attribute argument noise.
fn method_header(source: &str, node: Node<'_>) -> String {
    let mut header_start = node.start_byte();
    let mut header_end = node.end_byte();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "attribute_list" => header_start = header_start.max(child.end_byte()),
            "parameter_list" => header_end = header_end.min(child.start_byte()),
            _ => {}
        }
    }
    if header_start >= header_end {
        return String::new();
    }
    source
        .get(header_start..header_end)
        .unwrap_or("")
        .to_string()
}

fn csharp_parameter_shape(node: Node<'_>) -> String {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "parameter_list" {
            let mut parameter_cursor = child.walk();
            let arity = child
                .named_children(&mut parameter_cursor)
                .filter(|parameter| parameter.kind() == "parameter")
                .count();
            return format!("arity_{arity}");
        }
    }
    "arity_unknown".to_string()
}

fn db_set_entity_type_shape(source: &str, node: Node<'_>) -> Option<&'static str> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "generic_name" {
            continue;
        }
        let generic_base = child
            .child_by_field_name("name")
            .or_else(|| {
                let mut name_cursor = child.walk();
                let found = child
                    .named_children(&mut name_cursor)
                    .find(|candidate| candidate.kind() == "identifier");
                found
            })
            .and_then(|name| node_text_checked(source, name))?;
        if generic_base != "DbSet" {
            return None;
        }
        let mut argument_cursor = child.walk();
        let argument = child
            .named_children(&mut argument_cursor)
            .find(|candidate| candidate.kind() == "type_argument_list")
            .and_then(|arguments| {
                let mut inner_cursor = arguments.walk();
                let first = arguments.named_children(&mut inner_cursor).next();
                first
            });
        return Some(match argument.map(|argument| argument.kind()) {
            Some("identifier") => "simple",
            Some("qualified_name") => "qualified",
            _ => "unknown",
        });
    }
    None
}

/// Shape of a route/`Route` attribute template argument list: `none` without
/// positional arguments, `literal` for exactly one plain string literal, and
/// `dynamic` for interpolation, concatenation, `nameof`, constants, or any
/// other expression.
fn route_template_shape(arguments: Option<&str>) -> &'static str {
    let Some(arguments) = arguments else {
        return "none";
    };
    let Some(inner) = attribute_argument_inner(arguments) else {
        return "none";
    };
    let candidates = template_argument_expressions(inner);
    if candidates.is_empty() {
        return "none";
    }
    if candidates.len() == 1 && single_string_literal_consumes(candidates[0]) {
        "literal"
    } else {
        "dynamic"
    }
}

fn invocation_route_template_shape(arguments: &str) -> &'static str {
    let Some(inner) = attribute_argument_inner(arguments) else {
        return "none";
    };
    let parts = split_top_level_commas(inner);
    let Some(first) = parts.first() else {
        return "none";
    };
    if single_string_literal_consumes(first.trim()) {
        "literal"
    } else {
        "dynamic"
    }
}

fn attribute_argument_inner(arguments: &str) -> Option<&str> {
    let open = arguments.find('(')?;
    let close = arguments.rfind(')')?;
    (close > open).then(|| arguments[open + 1..close].trim())
}

fn template_argument_expressions(inner: &str) -> Vec<&str> {
    split_top_level_commas(inner)
        .into_iter()
        .filter(|part| split_top_level_assignment(part).is_none())
        .map(|part| {
            // Named constructor arguments (`template: "x"`) contribute their value.
            match split_top_level_named_argument(part) {
                Some(value) => value.trim(),
                None => part.trim(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect()
}

fn split_top_level_named_argument(text: &str) -> Option<&str> {
    let colon = find_top_level(text, ':')?;
    let name = text[..colon].trim();
    is_identifier(name).then(|| &text[colon + 1..])
}

fn split_top_level_commas(text: &str) -> Vec<&str> {
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
            '(' | '{' | '[' | '<' => depth += 1,
            ')' | '}' | ']' | '>' => depth -= 1,
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

fn split_top_level_assignment(text: &str) -> Option<(&str, &str)> {
    let index = find_top_level(text, '=')?;
    Some((&text[..index], &text[index + 1..]))
}

fn find_top_level(text: &str, needle: char) -> Option<usize> {
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
            '(' | '{' | '[' | '<' => depth += 1,
            ')' | '}' | ']' | '>' => depth -= 1,
            character if character == needle && depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn single_string_literal_consumes(text: &str) -> bool {
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

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(character) if character.is_ascii_alphabetic() || character == '_' => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
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
        if matches!(child.kind(), "identifier") {
            return node_text_checked(source, child).map(str::to_string);
        }
        if child.kind() == "attribute_list" {
            continue;
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
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse(text: &str) -> ParseReport {
        let document = SourceDocument {
            path: "src/Api/Program.cs",
            language: Language::CSharp,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        };
        CSharpSyntaxParser::default()
            .parse(document)
            .expect("parse csharp")
    }

    fn unit_kinds(report: &ParseReport) -> Vec<&'static str> {
        report.units.iter().map(|unit| unit.kind.as_str()).collect()
    }

    fn anchor_targets(report: &ParseReport) -> Vec<String> {
        let mut targets = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_affected_claims(report: &ParseReport) -> Vec<String> {
        let mut claims = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("affected_claim="))
            .map(str::to_string)
            .collect::<Vec<_>>();
        claims.sort();
        claims
    }

    fn route_template_shapes(report: &ParseReport) -> Vec<String> {
        report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("route_template_shape="))
            .map(str::to_string)
            .collect()
    }

    fn exact_assumption_values(report: &ParseReport, prefix: &str) -> Vec<String> {
        let mut values = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix(prefix))
            .map(str::to_string)
            .collect::<Vec<_>>();
        values.sort();
        values
    }

    fn member_data_unknown_details(report: &ParseReport) -> Vec<(String, String)> {
        let mut details = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .filter(|fact| {
                fact.assumptions
                    .iter()
                    .any(|value| value == "affected_claim=csharp_test_member_data")
            })
            .filter_map(|fact| {
                let kind = fact
                    .assumptions
                    .iter()
                    .find_map(|value| value.strip_prefix("csharp_unknown_kind="))?;
                let method = fact
                    .subject
                    .split_once("#xunit_test_method:")?
                    .1
                    .rsplit_once('-')?
                    .0;
                Some((method.to_string(), kind.to_string()))
            })
            .collect::<Vec<_>>();
        details.sort();
        details
    }

    #[test]
    fn extracts_controller_actions_only_inside_exact_controller() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Mvc;

namespace Api.Controllers;

[ApiController]
[Route("api/catalog")]
public class CatalogController : ControllerBase
{
    [HttpGet("items")]
    public IActionResult List() => Ok();

    [HttpGet("items/{id}")]
    public IActionResult Get(int id) => Ok(id);

    [HttpPost("items")]
    public IActionResult Create() => Ok();
}
"#,
        );

        assert!(unit_kinds(&report).contains(&"aspnet_controller"));
        assert_eq!(
            unit_kinds(&report)
                .iter()
                .filter(|kind| **kind == "aspnet_controller_action")
                .count(),
            3
        );
        let targets = anchor_targets(&report);
        assert!(targets.contains(&"aspnetcore.mvc.ApiController".to_string()));
        assert_eq!(
            targets
                .iter()
                .filter(|target| **target == "aspnetcore.mvc.HttpGet")
                .count(),
            2
        );
        assert!(targets.contains(&"aspnetcore.mvc.HttpPost".to_string()));
        // Every controller carries non-blocking DI + filter-pipeline unknowns.
        let claims = unknown_affected_claims(&report);
        assert!(claims.contains(&"csharp_di_registration".to_string()));
        assert!(claims.contains(&"csharp_aspnet_filter_pipeline".to_string()));
        // No blocking unknowns for the exact form.
        assert!(!claims.contains(&"csharp_attribute_binding".to_string()));
        assert!(!claims.contains(&"csharp_controller_identity".to_string()));
    }

    #[test]
    fn route_attribute_outside_controller_is_blocking_unknown() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Mvc;

public class NotAController
{
    [HttpGet("items")]
    public string List() => "x";
}
"#,
        );

        assert!(!unit_kinds(&report).contains(&"aspnet_controller_action"));
        assert!(anchor_targets(&report).is_empty());
        assert_eq!(
            unknown_affected_claims(&report),
            vec!["csharp_controller_identity".to_string()]
        );
    }

    #[test]
    fn lookalike_attributes_without_using_are_blocking_unknowns() {
        let report = parse(
            r#"
public class ControllerBase { }

public class LookalikeController : ControllerBase
{
    [HttpGet("items")]
    public string List() => "x";

    [Fact]
    public void Passes() { }
}
"#,
        );

        assert!(anchor_targets(&report).is_empty());
        assert_eq!(
            unknown_affected_claims(&report),
            vec![
                "csharp_attribute_binding".to_string(),
                "csharp_attribute_binding".to_string(),
            ]
        );
    }

    #[test]
    fn route_template_literal_versus_dynamic_matrix() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
public class MixedController : ControllerBase
{
    [HttpGet("items")]
    public IActionResult A() => Ok();

    [HttpGet($"items/{Version}")]
    public IActionResult B() => Ok();

    [HttpGet(nameof(A))]
    public IActionResult C() => Ok();
}
"#,
        );

        let mut shapes = route_template_shapes(&report);
        shapes.sort();
        assert_eq!(
            shapes,
            vec![
                "dynamic".to_string(),
                "dynamic".to_string(),
                "literal".to_string()
            ]
        );
        // Two dynamic templates each raise a non-blocking route-template subclaim.
        assert_eq!(
            unknown_affected_claims(&report)
                .iter()
                .filter(|claim| **claim == "csharp_aspnet_route_template")
                .count(),
            2
        );
    }

    #[test]
    fn minimal_api_receiver_traces_to_web_application_builder() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Builder;

var app = WebApplication.CreateBuilder(args).Build();

app.MapGet("/health", () => "ok");
app.MapPost("/items", () => "created");
"#,
        );

        assert_eq!(
            unit_kinds(&report)
                .iter()
                .filter(|kind| **kind == "aspnet_minimal_api_route")
                .count(),
            2
        );
        let targets = anchor_targets(&report);
        assert!(targets.contains(&"aspnetcore.builder.MapGet".to_string()));
        assert!(targets.contains(&"aspnetcore.builder.MapPost".to_string()));
        assert!(
            !unknown_affected_claims(&report).contains(&"csharp_minimal_api_receiver".to_string())
        );
    }

    #[test]
    fn minimal_api_unresolvable_receiver_is_blocking_unknown() {
        let report = parse(
            r#"
public static class Extensions
{
    public static void Configure(object app)
    {
        app.MapGet("/health", () => "ok");
    }
}
"#,
        );

        assert!(!unit_kinds(&report).contains(&"aspnet_minimal_api_route"));
        assert_eq!(
            unknown_affected_claims(&report),
            vec!["csharp_minimal_api_receiver".to_string()]
        );
    }

    #[test]
    fn efcore_dbcontext_and_dbset_units_anchor() {
        let report = parse(
            r#"
using Microsoft.EntityFrameworkCore;

public class ShopContext : DbContext
{
    public DbSet<Product> Products { get; set; }
    public DbSet<Order> Orders { get; set; }
}
"#,
        );

        assert!(unit_kinds(&report).contains(&"efcore_db_context"));
        assert_eq!(
            unit_kinds(&report)
                .iter()
                .filter(|kind| **kind == "efcore_entity_set")
                .count(),
            2
        );
        let targets = anchor_targets(&report);
        assert!(targets.contains(&"efcore.DbContext".to_string()));
        assert!(targets.contains(&"efcore.DbSet".to_string()));
    }

    #[test]
    fn test_framework_matrix_recognizes_xunit_nunit_and_mstest() {
        let xunit = parse(
            r#"
using Xunit;

public class XTests
{
    [Fact]
    public void Passes() { }

    [Theory]
    public void Cases() { }
}
"#,
        );
        assert_eq!(
            xunit
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::XunitTestMethod)
                .count(),
            2
        );
        assert!(anchor_targets(&xunit).contains(&"xunit.Fact".to_string()));
        assert!(anchor_targets(&xunit).contains(&"xunit.Theory".to_string()));

        let nunit = parse(
            r#"
using NUnit.Framework;

public class NTests
{
    [Test]
    public void Passes() { }
}
"#,
        );
        assert!(unit_kinds(&nunit).contains(&"nunit_test_method"));
        assert!(anchor_targets(&nunit).contains(&"nunit.framework.Test".to_string()));

        let mstest = parse(
            r#"
using Microsoft.VisualStudio.TestTools.UnitTesting;

[TestClass]
public class MTests
{
    [TestMethod]
    public void Passes() { }
}
"#,
        );
        assert!(unit_kinds(&mstest).contains(&"mstest_test_method"));
        assert!(anchor_targets(&mstest).contains(&"mstest.unittesting.TestMethod".to_string()));
    }

    #[test]
    fn xunit_member_data_links_unique_same_class_public_static_members() {
        let report = parse(
            r#"
using System.Collections.Generic;
using Xunit;

public class DataTests
{
    public static IEnumerable<object[]> FieldCases = new[] { new object[] { 1 } };
    public static IEnumerable<object[]> PropertyCases => new[] { new object[] { 2 } };
    public static IEnumerable<object[]> MethodCases() => new[] { new object[] { 3 } };

    [Theory]
    [MemberData("FieldCases")]
    public void FromField(int value) { }

    [Theory]
    [MemberData("PropertyCases")]
    public void FromProperty(int value) { }

    [Theory]
    [MemberData(memberName: "MethodCases")]
    public void FromMethod(int value) { }
}
"#,
        );

        assert_eq!(
            report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::XunitTestMethod)
                .count(),
            3
        );
        assert!(!unknown_affected_claims(&report).contains(&"csharp_test_member_data".to_string()));
        assert_eq!(
            exact_assumption_values(&report, "test_data_shape="),
            vec![
                "member_data_exact".to_string(),
                "member_data_exact".to_string(),
                "member_data_exact".to_string(),
            ]
        );
        assert_eq!(
            exact_assumption_values(&report, "xunit_member_data_sources="),
            vec![
                "FieldCases:field".to_string(),
                "MethodCases:method".to_string(),
                "PropertyCases:property".to_string(),
            ]
        );
    }

    #[test]
    fn xunit_member_data_open_dynamic_and_ineligible_sources_stay_unknown() {
        let report = parse(
            r#"
using System.Collections.Generic;
using Xunit;

public partial class PartialTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases")]
    public void Partial(int value) { }
}

public class DerivedTests : object
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases")]
    public void Derived(int value) { }
}

public class AmbiguousTests
{
    public static IEnumerable<object[]> Cases() => new[] { new object[] { 1 } };
    public static IEnumerable<object[]> Cases(int seed) => new[] { new object[] { seed } };
    [Theory, MemberData("Cases")]
    public void Ambiguous(int value) { }
}

public class PrivateTests
{
    private static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases")]
    public void Private(int value) { }
}

public class DynamicTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData(nameof(Cases))]
    public void Dynamic(int value) { }

    [Theory, MemberData("Cases", MemberType = typeof(OtherData))]
    public void External(int value) { }
}

public class ConditionalTests
{
#if DEBUG
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
#endif
    [Theory, MemberData("Cases")]
    public void Conditional(int value) { }
}

public class FactTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Fact, MemberData("Cases")]
    public void NotATheory() { }
}

public class UnresolvedAttributeTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Xunit.Theory, Fake.MemberData("Cases")]
    public void UnresolvedAttribute(int value) { }
}

public class MixedAttributeTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases"), Fake.MemberData("Cases")]
    public void MixedAttribute(int value) { }
}

public class PropertyArgumentTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases", DisableDiscoveryEnumeration = true)]
    public void PropertyArgument(int value) { }
}

public struct StructTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases")]
    public void Struct(int value) { }
}

public class GenericTests<T>
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };
    [Theory, MemberData("Cases")]
    public void Generic(int value) { }
}

public class ParameterizedSourceTests
{
    public static IEnumerable<object[]> Cases(int seed) => new[] { new object[] { seed } };
    [Theory, MemberData("Cases")]
    public void ParameterizedSource(int value) { }
}

public class OtherData { }
"#,
        );

        assert_eq!(
            member_data_unknown_details(&report),
            vec![
                (
                    "ambiguous".to_string(),
                    "xunit_member_data_ambiguous_source".to_string(),
                ),
                (
                    "conditional".to_string(),
                    "xunit_member_data_ineligible_source".to_string(),
                ),
                (
                    "derived".to_string(),
                    "xunit_member_data_open_class_scope".to_string(),
                ),
                (
                    "dynamic".to_string(),
                    "xunit_member_data_dynamic_source".to_string(),
                ),
                (
                    "external".to_string(),
                    "xunit_member_data_external_type".to_string(),
                ),
                (
                    "generic".to_string(),
                    "xunit_member_data_open_class_scope".to_string(),
                ),
                (
                    "mixedattribute".to_string(),
                    "xunit_member_data_unresolved_attribute".to_string(),
                ),
                (
                    "notatheory".to_string(),
                    "xunit_member_data_requires_theory".to_string(),
                ),
                (
                    "parameterizedsource".to_string(),
                    "xunit_member_data_ineligible_source".to_string(),
                ),
                (
                    "partial".to_string(),
                    "xunit_member_data_open_class_scope".to_string(),
                ),
                (
                    "private".to_string(),
                    "xunit_member_data_ineligible_source".to_string(),
                ),
                (
                    "propertyargument".to_string(),
                    "xunit_member_data_property_arguments".to_string(),
                ),
                (
                    "struct".to_string(),
                    "xunit_member_data_open_class_scope".to_string(),
                ),
                (
                    "unresolvedattribute".to_string(),
                    "xunit_member_data_unresolved_attribute".to_string(),
                ),
            ]
        );
        assert!(exact_assumption_values(&report, "xunit_member_data_sources=").is_empty());
    }

    #[test]
    fn xunit_member_data_does_not_leak_from_an_enclosing_class() {
        let report = parse(
            r#"
using System.Collections.Generic;
using Xunit;

public class OuterTests
{
    public static IEnumerable<object[]> Cases => new[] { new object[] { 1 } };

    public class InnerTests
    {
        [Theory, MemberData("Cases")]
        public void Inner(int value) { }
    }
}
"#,
        );

        assert_eq!(
            unknown_affected_claims(&report)
                .iter()
                .filter(|claim| **claim == "csharp_test_member_data")
                .count(),
            1
        );
    }

    #[test]
    fn using_bindings_are_tree_sitter_scoped_and_ignore_comments() {
        let report = parse(
            r#"
// using Xunit;

namespace CommentOnly
{
    public class CommentTests
    {
        [Fact]
        public void CommentDoesNotBind() { }
    }
}

namespace LocalImport
{
    using Xunit;

    public class LocalTests
    {
        public static object Cases => new object();

        [Theory, MemberData("Cases")]
        public void LocalUsingBinds(int value) { }
    }
}

namespace Sibling
{
    public class SiblingTests
    {
        [Fact]
        public void SiblingDoesNotInheritUsing() { }
    }
}
"#,
        );

        assert_eq!(
            anchor_targets(&report)
                .iter()
                .filter(|target| **target == "xunit.Theory")
                .count(),
            1
        );
        assert!(!unknown_affected_claims(&report).contains(&"csharp_test_member_data".to_string()));
        assert_eq!(
            unknown_affected_claims(&report)
                .iter()
                .filter(|claim| **claim == "csharp_attribute_binding")
                .count(),
            2
        );
    }

    #[test]
    fn parse_degraded_member_data_never_discharges_the_link_unknown() {
        let report = parse(
            r#"
using Xunit;

public class BrokenProviderTests
{
    public static object Cases => ;
    [Theory, MemberData("Cases")]
    public void BrokenProvider(int value) { }
}

public class BrokenAttributeTests
{
    public static object Cases => new object();
    [Theory, MemberData("Cases", Missing = )]
    public void BrokenAttribute(int value) { }
}
"#,
        );

        assert!(exact_assumption_values(&report, "xunit_member_data_sources=").is_empty());
        assert_eq!(
            member_data_unknown_details(&report),
            vec![
                (
                    "brokenattribute".to_string(),
                    "xunit_member_data_unresolved_attribute".to_string(),
                ),
                (
                    "brokenprovider".to_string(),
                    "xunit_member_data_open_class_scope".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn dense_conditional_member_inventory_uses_bounded_interval_queries() {
        let mut source = String::from("using Xunit;\npublic class DenseTests\n{\n");
        for index in 0..1_024 {
            source.push_str(&format!(
                "#if FEATURE_{index}\npublic static object Cases{index} => new object();\n#endif\n\n"
            ));
        }
        source.push_str("[Theory, MemberData(\"Cases0\")]\npublic void Dense(int value) { }\n}\n");

        assert!(source.len() < 1_048_576);
        let report = parse(&source);
        assert_eq!(
            member_data_unknown_details(&report),
            vec![(
                "dense".to_string(),
                "xunit_member_data_ineligible_source".to_string(),
            )]
        );
    }

    #[test]
    fn mstest_method_without_test_class_is_blocking_unknown() {
        let report = parse(
            r#"
using Microsoft.VisualStudio.TestTools.UnitTesting;

public class LooseTests
{
    [TestMethod]
    public void Passes() { }
}
"#,
        );

        assert!(anchor_targets(&report).is_empty());
        assert_eq!(
            unknown_affected_claims(&report),
            vec!["csharp_test_class_identity".to_string()]
        );
    }

    #[test]
    fn conditional_compilation_region_blocks_anchored_units() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
public class GuardedController : ControllerBase
{
#if DEBUG
    [HttpGet("items")]
    public IActionResult List() => Ok();
#endif
}
"#,
        );

        assert!(unknown_affected_claims(&report).contains(&"csharp_build_variant".to_string()));
    }

    #[test]
    fn partial_and_dynamic_shapes_are_non_blocking_subclaims() {
        let report = parse(
            r#"
using Microsoft.AspNetCore.Mvc;

[ApiController]
public partial class PartialController : ControllerBase
{
    [HttpGet("items")]
    public dynamic List() => Ok();
}
"#,
        );

        let claims = unknown_affected_claims(&report);
        assert!(claims.contains(&"csharp_partial_external".to_string()));
        assert!(claims.contains(&"csharp_dynamic_binding".to_string()));
        // These are non-blocking; the exact controller/action anchors still form.
        assert!(anchor_targets(&report).contains(&"aspnetcore.mvc.ApiController".to_string()));
        assert!(anchor_targets(&report).contains(&"aspnetcore.mvc.HttpGet".to_string()));
        assert!(!claims.contains(&"csharp_attribute_binding".to_string()));
        assert!(!claims.contains(&"csharp_controller_identity".to_string()));
    }

    const BRANCH_CORPUS: &[&str] = &[
        "class OnlyClass{}",
        "namespace Api;\nusing Microsoft.AspNetCore.Mvc;\n[ApiController]\npublic class A : ControllerBase { [HttpGet(\"x\")] public IActionResult Get() => Ok(); }",
        "using Microsoft.AspNetCore.Mvc;\nnamespace Outer { using Xunit; namespace Inner { public class T { [Fact] public void F() {} [Theory] [MemberData(nameof(Rows))] public void G(int x) {} public static IEnumerable<object[]> Rows => null; class Nested { [Fact] public void H() {} } } } }",
        "using Microsoft.AspNetCore.Mvc;\n[Route(\"api/[controller]\")]\npublic partial class C : Controller {\n#if DEBUG\n  [HttpPost] public dynamic Post() { dynamic d = 1; return d; }\n#endif\n  public C() {}\n}",
        "var builder = WebApplication.CreateBuilder(args);\nvar app = builder.Build();\napp.MapGet(\"/items\", () => 1).WithName(\"x\");\napp.MapPost(path, () => 2);\nother.MapPut(\"/y\", () => 3);\napp.MapControllerRoute(\"default\", \"{controller}\");\napp.Run();",
        "var app = WebApplication.CreateBuilder(args).Build();\nvar app = Other();\napp.MapDelete(\"/z\", () => 1);",
        "using Microsoft.EntityFrameworkCore;\npublic class Db : DbContext { public DbSet<Item> Items { get; set; } public DbSet<Order> Orders => Set<Order>(); public int Count { get; } }",
        "using FluentValidation;\npublic class V : AbstractValidator<Item> { public V() { RuleFor(x => x.Name).NotEmpty(); } }\npublic partial class W : FluentValidation.AbstractValidator<Item> {}",
        "using Microsoft.VisualStudio.TestTools.UnitTesting;\n[TestClass] public class M { [TestMethod] [DataRow(1)] public void A(int x) {} }\npublic class N { [TestMethod] public void B() {} }",
        "using NUnit.Framework;\n[TestFixture] public class U { [Test] public void A() {} [TestCase(1)] public void B(int x) {} }\npublic record R(int X); public struct S { public void M() {} } public interface I { void N(); } public enum E { A }",
        "// using Xunit;\npublic class Lookalike { [Fact] public void F() {} [HttpGet] public void G() {} }\n[ApiController] public class L {}",
        "[JsonSerializable(typeof(Item))] partial class Ctx : JsonSerializerContext {}\n[GeneratedRegex(\"x\")] private static partial Regex R();",
        "using Microsoft.AspNetCore.Mvc;\npublic class Broken : ControllerBase { [HttpGet] public IActionResult Get( { return Ok(); }\n class",
        "using Xunit;\npublic class P { [Theory] [MemberData(nameof(Data), MemberType = typeof(P))] public void T(int x) {}\n#if NET8\n public static TheoryData<int> Data => new();\n#endif\n}",
        "namespace A.B;\nusing Microsoft.AspNetCore.Mvc;\npublic class G<T> : Microsoft.AspNetCore.Mvc.ControllerBase { [Microsoft.AspNetCore.Mvc.HttpGet(\"{id}\")] public T Get<TKey>(TKey id) { void Local() { app.MapGet(\"/q\", () => 0); } return default; } }",
    ];

    /// Unit kinds and UNKNOWN claims for each branch source, recorded when the
    /// node-query scanner matched the legacy recursive traversal byte for byte
    /// (docs/experiments/declarative-language-pack-spike.md). Branches 0 and 11
    /// make a class span the same bytes as the module; branch 12 is
    /// parse-degraded.
    #[test]
    fn node_query_scanner_branch_corpus_snapshots() {
        let expected: [(&[&str], &[&str]); 15] = [
            (&["class", "module"], &[]),
            (
                &["module", "aspnet_controller", "aspnet_controller_action"],
                &["csharp_aspnet_filter_pipeline", "csharp_di_registration"],
            ),
            (
                &[
                    "module",
                    "class",
                    "xunit_test_method",
                    "xunit_test_method",
                    "class",
                    "xunit_test_method",
                ],
                &["csharp_test_member_data"],
            ),
            (
                &[
                    "module",
                    "aspnet_controller",
                    "aspnet_controller_action",
                    "method",
                ],
                &[
                    "csharp_aspnet_filter_pipeline",
                    "csharp_build_variant",
                    "csharp_build_variant",
                    "csharp_di_registration",
                    "csharp_dynamic_binding",
                    "csharp_partial_external",
                ],
            ),
            (
                &[
                    "module",
                    "aspnet_minimal_api_route",
                    "aspnet_minimal_api_route",
                ],
                &[
                    "csharp_aspnet_convention_routing",
                    "csharp_aspnet_route_template",
                    "csharp_minimal_api_receiver",
                ],
            ),
            (&["module"], &["csharp_minimal_api_receiver"]),
            (
                &[
                    "module",
                    "efcore_db_context",
                    "efcore_entity_set",
                    "efcore_entity_set",
                ],
                &[],
            ),
            (
                &[
                    "module",
                    "fluentvalidation_validator",
                    "method",
                    "fluentvalidation_validator",
                ],
                &[
                    "csharp_fluentvalidation_rule_chain",
                    "csharp_fluentvalidation_rule_chain",
                    "csharp_partial_external",
                ],
            ),
            (
                &["module", "class", "mstest_test_method", "class", "method"],
                &["csharp_test_class_identity"],
            ),
            (
                &[
                    "module",
                    "class",
                    "nunit_test_method",
                    "nunit_test_method",
                    "class",
                    "class",
                    "method",
                    "class",
                    "method",
                    "class",
                ],
                &[],
            ),
            (
                &["module", "class", "method", "method", "class"],
                &[
                    "csharp_attribute_binding",
                    "csharp_attribute_binding",
                    "csharp_attribute_binding",
                ],
            ),
            (&["class", "module"], &["csharp_generated_source"]),
            (&["module", "method"], &["csharp_controller_identity"]),
            (
                &["module", "class", "xunit_test_method"],
                &["csharp_test_member_data"],
            ),
            (
                &["module", "aspnet_controller", "aspnet_controller_action"],
                &[
                    "csharp_aspnet_filter_pipeline",
                    "csharp_di_registration",
                    "csharp_minimal_api_receiver",
                ],
            ),
        ];
        assert_eq!(BRANCH_CORPUS.len(), expected.len());
        for (index, (text, (kinds, claims))) in BRANCH_CORPUS.iter().zip(expected).enumerate() {
            let report = parse(text);
            assert_eq!(unit_kinds(&report), kinds, "branch {index}");
            assert_eq!(unknown_affected_claims(&report), claims, "branch {index}");
        }
    }
}
