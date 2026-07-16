//! Bounded, source-visible JUnit `@MethodSource` and TestNG `@DataProvider`
//! binding.
//!
//! The resolver deliberately stays inside one class-like declaration. It never
//! follows inheritance, external classes, overload signatures, generated code,
//! or runtime configuration. Each class body is indexed once and lookups are
//! logarithmic, avoiding a full class rescan for every test method.

use super::{
    annotation_arguments, annotation_full_names, annotation_segment_exact,
    annotation_segments_exact, contains_annotation_simple_name, has_exact_direct_annotation,
    modifier_text, node_text_checked, single_string_literal_consumes, split_top_level_assignment,
    split_top_level_commas, JavaImportContext,
};
use crate::core::model::UnknownReasonCode;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

const JUNIT5_PARAMS_PROVIDER_PACKAGE: &str = "org.junit.jupiter.params.provider";
const TESTNG_PACKAGE: &str = "org.testng.annotations";

pub(crate) const JUNIT_METHOD_SOURCE_TARGET: &str = "junit.jupiter.MethodSource.local_factory";
pub(crate) const TESTNG_DATA_PROVIDER_TARGET: &str = "testng.annotations.DataProvider.local_method";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestDataReference {
    None,
    Junit(MethodSourceReference),
    Testng(DataProviderReference),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MethodSourceReference {
    Local(Vec<LocalMethodSource>),
    External,
    UnknownIdentity,
    UnknownTestIdentity,
    InvalidTestKind,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalMethodSource {
    DefaultMethodName,
    Literal(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DataProviderReference {
    LocalLiteral(String),
    ExternalClass,
    UnknownIdentity,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTestDataBinding {
    pub(crate) target: &'static str,
    pub(crate) note: &'static str,
    pub(crate) assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnresolvedTestDataBinding {
    pub(crate) reason: UnknownReasonCode,
    pub(crate) kind: &'static str,
    pub(crate) note: &'static str,
    pub(crate) assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestDataResolution {
    None,
    Resolved(ResolvedTestDataBinding),
    Unknown(UnresolvedTestDataBinding),
}

#[derive(Debug, Default)]
pub(crate) struct ClassTestDataRegistry {
    methods: BTreeMap<String, MethodDeclarations>,
    testng_providers: BTreeMap<String, usize>,
    degraded_testng_providers: BTreeSet<String>,
    open_world: bool,
}

#[derive(Debug, Default)]
struct MethodDeclarations {
    total: usize,
    static_total: usize,
    degraded_total: usize,
    starts: BTreeSet<usize>,
    static_starts: BTreeSet<usize>,
    degraded_starts: BTreeSet<usize>,
}

impl ClassTestDataRegistry {
    pub(crate) fn from_class_like(
        source: &str,
        class_like: Node<'_>,
        imports: &JavaImportContext,
    ) -> Self {
        let Some(body) = class_like.child_by_field_name("body") else {
            return Self {
                open_world: true,
                ..Self::default()
            };
        };
        let mut registry = Self::from_class_body(source, body, imports);
        registry.open_world |=
            class_like.has_error() || class_header_is_parse_degraded(class_like, body);
        registry
    }

    pub(crate) fn from_class_body(
        source: &str,
        body: Node<'_>,
        imports: &JavaImportContext,
    ) -> Self {
        let mut registry = Self {
            open_world: body.has_error(),
            ..Self::default()
        };
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "enum_body_declarations" {
                let mut declarations_cursor = child.walk();
                for member in child.named_children(&mut declarations_cursor) {
                    registry.record_member(source, member, imports);
                }
                continue;
            }
            registry.record_member(source, child, imports);
        }
        registry
    }

    fn record_member(&mut self, source: &str, member: Node<'_>, imports: &JavaImportContext) {
        if member.kind() != "method_declaration" {
            if member.is_error() || member.is_missing() || member.has_error() {
                self.open_world = true;
            }
            return;
        }
        let method = member;
        let Some(name) = method
            .child_by_field_name("name")
            .and_then(|name| node_text_checked(source, name))
            .filter(|name| java_identifier(name))
        else {
            self.open_world = true;
            return;
        };
        let is_static = method_is_static(method);
        let parse_degraded = method_header_is_parse_degraded(method);
        let declarations = self.methods.entry(name.to_string()).or_default();
        declarations.total += 1;
        declarations.starts.insert(method.start_byte());
        declarations.degraded_total += usize::from(parse_degraded);
        if parse_degraded {
            declarations.degraded_starts.insert(method.start_byte());
        }
        if is_static {
            declarations.static_total += 1;
            declarations.static_starts.insert(method.start_byte());
        }

        let annotations = modifier_text(source, method, "method");
        if parse_degraded {
            if contains_annotation_simple_name(&annotations, &["DataProvider"]) {
                if let Some(provider_name) =
                    testng_provider_declaration_name(&annotations, name, imports)
                {
                    self.degraded_testng_providers.insert(provider_name);
                } else {
                    self.open_world = true;
                }
            }
            return;
        }
        if let Some(provider_name) = testng_provider_declaration_name(&annotations, name, imports) {
            *self.testng_providers.entry(provider_name).or_default() += 1;
        }
    }
}

pub(crate) fn method_source_reference(
    annotations: &str,
    imports: &JavaImportContext,
) -> Option<MethodSourceReference> {
    let Some(segments) = annotation_segments_exact(
        annotations,
        "MethodSource",
        &[JUNIT5_PARAMS_PROVIDER_PACKAGE],
        imports,
    ) else {
        return Some(MethodSourceReference::Unsupported);
    };
    let total = annotation_full_names(annotations)
        .iter()
        .filter(|name| name.split('.').next_back() == Some("MethodSource"))
        .count();
    if total == 0 {
        return None;
    }
    let strict = annotation_full_names(annotations)
        .iter()
        .filter(|name| {
            annotation_name_has_strict_identity(
                name,
                "MethodSource",
                JUNIT5_PARAMS_PROVIDER_PACKAGE,
                imports,
            )
        })
        .count();
    if segments.len() != total || strict != total {
        return Some(MethodSourceReference::UnknownIdentity);
    }

    let mut sources = Vec::new();
    for segment in segments {
        match method_sources_from_segment(&segment) {
            Ok(mut local) => sources.append(&mut local),
            Err(reference) => return Some(reference),
        }
    }
    (!sources.is_empty())
        .then_some(MethodSourceReference::Local(sources))
        .or(Some(MethodSourceReference::Unsupported))
}

fn method_sources_from_segment(
    segment: &str,
) -> Result<Vec<LocalMethodSource>, MethodSourceReference> {
    let arguments = match annotation_arguments(segment) {
        Some(arguments) => arguments.trim(),
        None if !segment.contains('(') => {
            return Ok(vec![LocalMethodSource::DefaultMethodName]);
        }
        None => return Err(MethodSourceReference::Unsupported),
    };
    if arguments.is_empty() {
        return Ok(vec![LocalMethodSource::DefaultMethodName]);
    }
    let parts = split_top_level_commas(arguments);
    if parts.len() != 1 {
        return Err(MethodSourceReference::Unsupported);
    }
    let expression = match split_top_level_assignment(parts[0]) {
        Some((name, value)) if name.trim() == "value" => value.trim(),
        Some(_) => return Err(MethodSourceReference::Unsupported),
        None => parts[0].trim(),
    };
    method_sources_from_expression(expression)
}

fn method_sources_from_expression(
    expression: &str,
) -> Result<Vec<LocalMethodSource>, MethodSourceReference> {
    let trimmed = expression.trim();
    let entries = if let Some(inner) = trimmed
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    {
        let entries = split_top_level_commas(inner);
        if entries.is_empty() {
            return Err(MethodSourceReference::Unsupported);
        }
        entries
    } else {
        vec![trimmed]
    };

    let mut sources = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(name) = bounded_literal_contents(entry) else {
            return Err(MethodSourceReference::Unsupported);
        };
        if name.contains('#') {
            return Err(MethodSourceReference::External);
        }
        if name.is_empty() {
            sources.push(LocalMethodSource::DefaultMethodName);
        } else if java_identifier(name) {
            sources.push(LocalMethodSource::Literal(name.to_string()));
        } else {
            return Err(MethodSourceReference::Unsupported);
        }
    }
    Ok(sources)
}

pub(crate) fn data_provider_reference(
    annotations: &str,
    imports: &JavaImportContext,
) -> Option<DataProviderReference> {
    let segment = match annotation_segment_exact(annotations, "Test", &[TESTNG_PACKAGE], imports) {
        Some(segment) => segment,
        None if has_exact_direct_annotation(annotations, "Test", &[TESTNG_PACKAGE], imports) => {
            return Some(DataProviderReference::Unsupported);
        }
        None => return None,
    };
    let arguments = match annotation_arguments(&segment) {
        Some(arguments) => arguments,
        None if segment.contains('(') => return Some(DataProviderReference::Unsupported),
        None => return None,
    };
    let mut provider_expression = None;
    let mut external_class = false;
    for part in split_top_level_commas(arguments) {
        let Some((raw_name, value)) = split_top_level_assignment(part) else {
            continue;
        };
        let Some(name) = exact_annotation_assignment_name(raw_name) else {
            return Some(DataProviderReference::Unsupported);
        };
        match name {
            "dataProvider" if provider_expression.is_none() => {
                provider_expression = Some(value.trim());
            }
            "dataProvider" => return Some(DataProviderReference::Unsupported),
            "dataProviderClass" => external_class = true,
            _ => {}
        }
    }
    let expression = provider_expression?;
    if !annotation_segment_has_strict_identity(&segment, "Test", TESTNG_PACKAGE, imports) {
        return Some(DataProviderReference::UnknownIdentity);
    }
    if external_class {
        return Some(DataProviderReference::ExternalClass);
    }
    bounded_testng_name(expression)
        .map(|name| DataProviderReference::LocalLiteral(name.to_string()))
        .or(Some(DataProviderReference::Unsupported))
}

pub(crate) fn data_shape(reference: &TestDataReference) -> &'static str {
    match reference {
        TestDataReference::Junit(MethodSourceReference::External) => "method_source_external",
        TestDataReference::Junit(MethodSourceReference::UnknownIdentity) => {
            "method_source_unknown_identity"
        }
        TestDataReference::Junit(MethodSourceReference::UnknownTestIdentity) => {
            "method_source_unknown_test_identity"
        }
        TestDataReference::Junit(MethodSourceReference::InvalidTestKind) => {
            "method_source_invalid_test_kind"
        }
        TestDataReference::Junit(MethodSourceReference::Unsupported) => "method_source_dynamic",
        TestDataReference::Junit(MethodSourceReference::Local(_)) => "method_source_local",
        _ => "none",
    }
}

pub(crate) fn resolve(
    reference: &TestDataReference,
    registry: Option<&ClassTestDataRegistry>,
    test_method_name: &str,
    test_method_start: usize,
    test_method_parse_degraded: bool,
) -> TestDataResolution {
    if !matches!(reference, TestDataReference::None) && test_method_parse_degraded {
        return unresolved(
            UnknownReasonCode::FrameworkMagic,
            "parse_degraded_test_data_reference",
            "Java test-data annotation or test declaration is parse-degraded",
            "parse_degraded_test_method",
        );
    }
    match reference {
        TestDataReference::None => TestDataResolution::None,
        TestDataReference::Junit(reference) => {
            resolve_method_source(reference, registry, test_method_name, test_method_start)
        }
        TestDataReference::Testng(reference) => resolve_data_provider(reference, registry),
    }
}

fn resolve_method_source(
    reference: &MethodSourceReference,
    registry: Option<&ClassTestDataRegistry>,
    test_method_name: &str,
    test_method_start: usize,
) -> TestDataResolution {
    let sources = match reference {
        MethodSourceReference::Local(sources) => sources,
        MethodSourceReference::External => {
            return unresolved(
                UnknownReasonCode::FrameworkMagic,
                "external_method_source",
                "JUnit @MethodSource references an external factory resolved at runtime",
                "external_class",
            );
        }
        MethodSourceReference::UnknownIdentity => {
            return unresolved(
                UnknownReasonCode::UnresolvedImport,
                "unresolved_method_source_annotation",
                "JUnit @MethodSource simple name lacks an exact import or FQN",
                "unknown_annotation_identity",
            );
        }
        MethodSourceReference::UnknownTestIdentity => {
            return unresolved(
                UnknownReasonCode::UnresolvedImport,
                "unresolved_parameterized_test_annotation",
                "JUnit @MethodSource owner lacks an unambiguous @ParameterizedTest import or FQN",
                "unknown_test_annotation_identity",
            );
        }
        MethodSourceReference::InvalidTestKind => {
            return unresolved(
                UnknownReasonCode::FrameworkMagic,
                "method_source_without_parameterized_test",
                "JUnit @MethodSource appears without @ParameterizedTest",
                "invalid_test_annotation",
            );
        }
        MethodSourceReference::Unsupported => {
            return unresolved(
                UnknownReasonCode::FrameworkMagic,
                "dynamic_or_ambiguous_method_source",
                "JUnit @MethodSource names are not a bounded set of local literals",
                "unsupported_expression",
            );
        }
    };
    let Some(registry) = registry else {
        return unresolved(
            UnknownReasonCode::FrameworkMagic,
            "missing_local_method_source",
            "JUnit @MethodSource has no source-visible enclosing class inventory",
            "missing_class_inventory",
        );
    };
    if registry.open_world {
        return unresolved(
            UnknownReasonCode::FrameworkMagic,
            "parse_degraded_test_data_scope",
            "JUnit @MethodSource class member inventory is parse-degraded or incomplete",
            "open_class_inventory",
        );
    }

    let mut names = Vec::with_capacity(sources.len());
    let mut reference_kinds = BTreeSet::new();
    for source in sources {
        let (name, reference_kind) = match source {
            LocalMethodSource::DefaultMethodName => (test_method_name, "default_same_name"),
            LocalMethodSource::Literal(name) => (name.as_str(), "explicit_literal"),
        };
        if let Err(unknown) =
            resolve_local_method_source(registry, name, reference_kind, test_method_start)
        {
            return TestDataResolution::Unknown(unknown);
        }
        names.push(name.to_string());
        reference_kinds.insert(reference_kind);
    }

    let binding = if names.len() == 1 {
        "local_unique_static_method"
    } else {
        "local_unique_static_methods"
    };
    let reference_kind = if reference_kinds.len() == 1 {
        reference_kinds
            .first()
            .copied()
            .unwrap_or("explicit_literal")
    } else {
        "mixed_local_sources"
    };
    names.sort();
    TestDataResolution::Resolved(ResolvedTestDataBinding {
        target: JUNIT_METHOD_SOURCE_TARGET,
        note: "bounded same-class JUnit MethodSource factory bindings",
        assumptions: resolved_assumptions(
            "junit_method_source_binding",
            binding,
            reference_kind,
            &names.join("|"),
        ),
    })
}

fn resolve_local_method_source(
    registry: &ClassTestDataRegistry,
    name: &str,
    reference_kind: &'static str,
    test_method_start: usize,
) -> Result<(), UnresolvedTestDataBinding> {
    let Some(declarations) = registry.methods.get(name) else {
        return Err(unresolved_value(
            UnknownReasonCode::FrameworkMagic,
            "missing_local_method_source",
            "JUnit @MethodSource local factory declaration is not source-visible in this class",
            reference_kind,
            Some(name),
        ));
    };
    let excludes_test = declarations.starts.contains(&test_method_start);
    let total = declarations.total - usize::from(excludes_test);
    let static_total = declarations.static_total
        - usize::from(declarations.static_starts.contains(&test_method_start));
    let degraded_total = declarations.degraded_total
        - usize::from(declarations.degraded_starts.contains(&test_method_start));
    if total == 0 {
        return Err(unresolved_value(
            UnknownReasonCode::FrameworkMagic,
            "missing_local_method_source",
            "JUnit @MethodSource local factory declaration is not source-visible in this class",
            reference_kind,
            Some(name),
        ));
    }
    if degraded_total != 0 {
        return Err(unresolved_value(
            UnknownReasonCode::FrameworkMagic,
            "parse_degraded_local_method_source",
            "JUnit @MethodSource local factory declaration is parse-degraded",
            reference_kind,
            Some(name),
        ));
    }
    if total != 1 {
        return Err(unresolved_value(
            UnknownReasonCode::ConflictingFacts,
            "ambiguous_local_method_source",
            "JUnit @MethodSource local factory name matches multiple declarations",
            reference_kind,
            Some(name),
        ));
    }
    if static_total != 1 {
        return Err(unresolved_value(
            UnknownReasonCode::FrameworkMagic,
            "non_static_local_method_source",
            "JUnit @MethodSource local factory is not statically provable under the default lifecycle",
            reference_kind,
            Some(name),
        ));
    }
    Ok(())
}

fn resolve_data_provider(
    reference: &DataProviderReference,
    registry: Option<&ClassTestDataRegistry>,
) -> TestDataResolution {
    let name = match reference {
        DataProviderReference::LocalLiteral(name) => name,
        DataProviderReference::ExternalClass => {
            return unresolved(
                UnknownReasonCode::FrameworkMagic,
                "external_testng_data_provider",
                "TestNG dataProviderClass resolution is outside the current class",
                "external_class",
            );
        }
        DataProviderReference::UnknownIdentity => {
            return unresolved(
                UnknownReasonCode::UnresolvedImport,
                "unresolved_testng_test_annotation",
                "TestNG @Test dataProvider binding lacks an unambiguous explicit import or FQN",
                "unknown_annotation_identity",
            );
        }
        DataProviderReference::Unsupported => {
            return unresolved(
                UnknownReasonCode::FrameworkMagic,
                "dynamic_testng_data_provider",
                "TestNG dataProvider name is not one bounded string literal",
                "unsupported_expression",
            );
        }
    };
    let Some(registry) = registry else {
        return unresolved(
            UnknownReasonCode::FrameworkMagic,
            "missing_local_testng_data_provider",
            "TestNG data provider has no source-visible enclosing class inventory",
            "missing_class_inventory",
        );
    };
    if registry.open_world {
        return unresolved(
            UnknownReasonCode::FrameworkMagic,
            "parse_degraded_test_data_scope",
            "TestNG data-provider class member inventory is parse-degraded or incomplete",
            "open_class_inventory",
        );
    }
    if registry.degraded_testng_providers.contains(name) {
        return unresolved_with_name(
            UnknownReasonCode::FrameworkMagic,
            "parse_degraded_local_testng_data_provider",
            "TestNG data provider declaration is parse-degraded",
            "explicit_literal",
            name,
        );
    }
    let count = registry.testng_providers.get(name).copied().unwrap_or(0);
    if count == 0 {
        return unresolved_with_name(
            UnknownReasonCode::FrameworkMagic,
            "missing_local_testng_data_provider",
            "TestNG data provider declaration is not source-visible in this class",
            "explicit_literal",
            name,
        );
    }
    if count != 1 {
        return unresolved_with_name(
            UnknownReasonCode::ConflictingFacts,
            "ambiguous_local_testng_data_provider",
            "TestNG data provider name matches multiple declarations in this class",
            "explicit_literal",
            name,
        );
    }
    TestDataResolution::Resolved(ResolvedTestDataBinding {
        target: TESTNG_DATA_PROVIDER_TARGET,
        note: "bounded same-class TestNG DataProvider binding",
        assumptions: resolved_assumptions(
            "testng_data_provider_binding",
            "local_unique_provider",
            "explicit_literal",
            name,
        ),
    })
}

fn unresolved(
    reason: UnknownReasonCode,
    kind: &'static str,
    note: &'static str,
    reference_kind: &'static str,
) -> TestDataResolution {
    TestDataResolution::Unknown(unresolved_value(reason, kind, note, reference_kind, None))
}

fn unresolved_with_name(
    reason: UnknownReasonCode,
    kind: &'static str,
    note: &'static str,
    reference_kind: &'static str,
    name: &str,
) -> TestDataResolution {
    TestDataResolution::Unknown(unresolved_value(
        reason,
        kind,
        note,
        reference_kind,
        Some(name),
    ))
}

fn unresolved_value(
    reason: UnknownReasonCode,
    kind: &'static str,
    note: &'static str,
    reference_kind: &'static str,
    name: Option<&str>,
) -> UnresolvedTestDataBinding {
    let mut assumptions = vec![format!("test_data_reference={reference_kind}")];
    if let Some(name) = name {
        assumptions.push(format!("test_data_provider_name={name}"));
    }
    UnresolvedTestDataBinding {
        reason,
        kind,
        note,
        assumptions,
    }
}

fn resolved_assumptions(
    anchor_kind: &str,
    binding: &str,
    reference_kind: &str,
    name: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        format!("java_anchor_kind={anchor_kind}"),
        format!("test_data_binding={binding}"),
        format!("test_data_reference={reference_kind}"),
        format!("test_data_provider_name={name}"),
    ]
}

fn annotation_name_has_strict_identity(
    full_name: &str,
    simple_name: &str,
    package: &str,
    imports: &JavaImportContext,
) -> bool {
    if full_name.contains('.') {
        full_name == format!("{package}.{simple_name}")
    } else {
        full_name == simple_name
            && imports.has_unambiguous_explicit_import_for(simple_name, &[package])
    }
}

pub(crate) fn has_strict_annotation_identity(
    annotations: &str,
    simple_name: &str,
    package: &str,
    imports: &JavaImportContext,
) -> bool {
    annotation_full_names(annotations)
        .iter()
        .any(|name| annotation_name_has_strict_identity(name, simple_name, package, imports))
}

fn annotation_segment_has_strict_identity(
    segment: &str,
    simple_name: &str,
    package: &str,
    imports: &JavaImportContext,
) -> bool {
    annotation_full_names(segment).first().is_some_and(|name| {
        annotation_name_has_strict_identity(name, simple_name, package, imports)
    })
}

fn testng_provider_declaration_name(
    annotations: &str,
    method_name: &str,
    imports: &JavaImportContext,
) -> Option<String> {
    let segment =
        annotation_segment_exact(annotations, "DataProvider", &[TESTNG_PACKAGE], imports)?;
    if !annotation_segment_has_strict_identity(&segment, "DataProvider", TESTNG_PACKAGE, imports) {
        return None;
    }
    let Some(arguments) = annotation_arguments(&segment) else {
        return (!segment.contains('(')).then(|| method_name.to_string());
    };
    if arguments.trim().is_empty() {
        return Some(method_name.to_string());
    }
    let parts = split_top_level_commas(arguments);
    let mut name_expression = None;
    for part in parts {
        let (raw_attribute, value) = split_top_level_assignment(part)?;
        let attribute = exact_annotation_assignment_name(raw_attribute)?;
        if attribute == "name" && name_expression.is_none() {
            name_expression = Some(value.trim());
        } else if attribute == "name" {
            return None;
        }
    }
    let Some(expression) = name_expression else {
        return Some(method_name.to_string());
    };
    bounded_testng_name(expression).map(str::to_string)
}

fn class_header_is_parse_degraded(class_like: Node<'_>, body: Node<'_>) -> bool {
    if class_like.is_error() || class_like.is_missing() || body.is_error() || body.is_missing() {
        return true;
    }
    let mut cursor = class_like.walk();
    let degraded = class_like
        .named_children(&mut cursor)
        .take_while(|child| child.id() != body.id())
        .any(|child| child.is_error() || child.is_missing() || child.has_error());
    degraded
}

pub(crate) fn method_header_is_parse_degraded(method: Node<'_>) -> bool {
    if method.is_error() || method.is_missing() {
        return true;
    }
    let mut cursor = method.walk();
    let degraded = method
        .named_children(&mut cursor)
        .take_while(|child| child.kind() != "block")
        .any(|child| child.is_error() || child.is_missing() || child.has_error());
    degraded
}

fn method_is_static(method: Node<'_>) -> bool {
    let mut method_cursor = method.walk();
    let Some(modifiers) = method
        .children(&mut method_cursor)
        .find(|child| child.kind() == "modifiers")
    else {
        return false;
    };
    let mut modifiers_cursor = modifiers.walk();
    let is_static = modifiers
        .children(&mut modifiers_cursor)
        .any(|child| child.kind() == "static");
    is_static
}

fn bounded_testng_name(expression: &str) -> Option<&str> {
    let name = bounded_literal_contents(expression)?;
    (!name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/')
        }))
    .then_some(name)
}

fn exact_annotation_assignment_name(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    let start = skip_annotation_trivia(value, 0)?;
    let mut end = start;
    if !bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return None;
    }
    end += 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    (skip_annotation_trivia(value, end)? == bytes.len()).then_some(&value[start..end])
}

fn skip_annotation_trivia(value: &str, mut cursor: usize) -> Option<usize> {
    let bytes = value.as_bytes();
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
            if cursor == bytes.len() {
                return None;
            }
            cursor += 2;
            continue;
        }
        return Some(cursor);
    }
}

fn bounded_literal_contents(expression: &str) -> Option<&str> {
    let trimmed = expression.trim();
    if !single_string_literal_consumes(trimmed) || trimmed.len() < 2 {
        return None;
    }
    let value = &trimmed[1..trimmed.len() - 1];
    (!value.contains('\\') && !value.chars().any(char::is_control)).then_some(value)
}

fn java_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
