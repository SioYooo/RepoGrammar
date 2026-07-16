//! Bounded general Rust framework anchor detection.
//!
//! Recognizes exact source-visible serde/thiserror/tokio/clap derive and
//! attribute shapes plus axum literal `Router::new().route(...)` segments, each
//! gated by same-file `use`-path evidence or an inline fully-qualified path.
//! This module never expands derive/attribute macros, resolves traits, or does
//! points-to analysis: written attribute shapes plus use-path evidence are the
//! anchors, and everything expansion-dependent stays typed UNKNOWN.

use super::unknown::{self, RustUnknownSpec};
use super::{anchors, node_text_checked};
use crate::core::model::{CodeUnit, CodeUnitKind, SemanticFact};
use crate::ports::parser::{ParseError, SourceDocument};
use std::collections::BTreeSet;
use tree_sitter::Node;

/// Same-file `use`-path evidence used to gate general framework anchors.
#[derive(Debug, Default, Clone)]
pub(super) struct FrameworkUseContext {
    serde_root: bool,
    thiserror_root: bool,
    clap_root: bool,
    tokio_main_import: bool,
    axum_routing_helpers: BTreeSet<String>,
    axum_routing_module: bool,
}

impl FrameworkUseContext {
    /// Collect use-path evidence from every `use` declaration in the file.
    pub(super) fn from_tree(root: Node<'_>, source: &str) -> Self {
        let mut context = FrameworkUseContext::default();
        collect_use_context(root, source, &mut context);
        context
    }
}

fn collect_use_context(node: Node<'_>, source: &str, context: &mut FrameworkUseContext) {
    if node.kind() == "use_declaration" {
        if let Some(text) = node_text_checked(source, node) {
            record_use_path(text, context);
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_use_context(child, source, context);
    }
}

fn record_use_path(use_text: &str, context: &mut FrameworkUseContext) {
    let path = use_path_body(use_text);
    let root = use_root_segment(&path);
    match root {
        "serde" => context.serde_root = true,
        "thiserror" => context.thiserror_root = true,
        "clap" => context.clap_root = true,
        "tokio" => {
            if use_leaf_names(&path, "tokio").contains("main") {
                context.tokio_main_import = true;
            }
        }
        "axum" => record_axum_use(&path, context),
        _ => {}
    }
}

fn record_axum_use(path: &str, context: &mut FrameworkUseContext) {
    let Some(rest) = path.strip_prefix("axum::routing") else {
        return;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        context.axum_routing_module = true;
        return;
    }
    let Some(rest) = rest.strip_prefix("::") else {
        return;
    };
    let rest = rest.trim();
    if rest == "*" {
        context.axum_routing_module = true;
        return;
    }
    for leaf in group_leaves(rest) {
        context.axum_routing_helpers.insert(leaf);
    }
}

fn use_path_body(use_text: &str) -> String {
    let mut body = use_text.trim();
    body = body
        .strip_prefix("pub")
        .map(str::trim_start)
        .unwrap_or(body);
    body = body
        .strip_prefix("use")
        .map(str::trim_start)
        .unwrap_or(body);
    body = body.trim();
    body = body.strip_suffix(';').unwrap_or(body).trim();
    body.to_string()
}

fn use_root_segment(path: &str) -> &str {
    path.split("::")
        .next()
        .unwrap_or(path)
        .split(['{', ' '])
        .next()
        .unwrap_or(path)
        .trim()
}

/// Leaf names imported for a given root, e.g. `tokio::{main, test}` -> {main, test}.
fn use_leaf_names(path: &str, root: &str) -> BTreeSet<String> {
    let prefix = format!("{root}::");
    let Some(rest) = path.strip_prefix(&prefix) else {
        return BTreeSet::new();
    };
    group_leaves(rest.trim())
}

fn group_leaves(rest: &str) -> BTreeSet<String> {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        inner
            .split(',')
            .filter_map(single_leaf)
            .collect::<BTreeSet<_>>()
    } else {
        single_leaf(rest).into_iter().collect()
    }
}

fn single_leaf(segment: &str) -> Option<String> {
    let name = segment
        .split(" as ")
        .next()
        .unwrap_or(segment)
        .rsplit("::")
        .next()
        .unwrap_or(segment)
        .trim();
    if !name.is_empty() && name != "*" && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(name.to_string())
    } else {
        None
    }
}

/// Framework classification for a struct/enum item's leading-attributes slice.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeFramework {
    Serde,
    Thiserror,
    Clap,
    /// Framework derive tokens present but no use-path evidence (blocking).
    Unresolved(&'static str),
    None,
}

/// Decide the code-unit kind for a struct/enum from its slice (leading
/// attributes plus body). Unresolved and non-framework shapes keep the plain
/// structural kind.
pub(super) fn type_kind(slice: &str, is_enum: bool, context: &FrameworkUseContext) -> CodeUnitKind {
    match classify_type_item(slice, is_enum, context) {
        TypeFramework::Serde => CodeUnitKind::SerdeModel,
        TypeFramework::Thiserror => CodeUnitKind::ThiserrorErrorEnum,
        TypeFramework::Clap => CodeUnitKind::ClapParser,
        TypeFramework::Unresolved(_) | TypeFramework::None => {
            if is_enum {
                CodeUnitKind::RustEnum
            } else {
                CodeUnitKind::RustStruct
            }
        }
    }
}

/// Decide the code-unit kind for a function from its slice. Tokio entry/test
/// attributes override the plain function/test kind.
pub(super) fn tokio_function_kind(slice: &str) -> Option<CodeUnitKind> {
    if slice.contains("#[tokio::main]") {
        Some(CodeUnitKind::TokioEntry)
    } else if slice.contains("#[tokio::test]") {
        Some(CodeUnitKind::TokioTest)
    } else {
        None
    }
}

pub(super) fn tokio_bare_main_kind(
    slice: &str,
    context: &FrameworkUseContext,
) -> Option<CodeUnitKind> {
    if context.tokio_main_import && has_bare_attribute(slice, "main") {
        Some(CodeUnitKind::TokioEntry)
    } else {
        None
    }
}

fn classify_type_item(slice: &str, is_enum: bool, context: &FrameworkUseContext) -> TypeFramework {
    let derives = derive_tokens(slice);
    // thiserror is enum-only and must carry at least one `#[error(...)]` variant.
    if is_enum
        && (derives.contains("Error") || slice.contains("#[derive(thiserror::Error"))
        && slice.contains("#[error(")
    {
        if context.thiserror_root || slice.contains("thiserror::Error") {
            return TypeFramework::Thiserror;
        }
        return TypeFramework::Unresolved("thiserror");
    }
    let has_serde = derives.contains("Serialize")
        || derives.contains("Deserialize")
        || slice.contains("#[derive(serde::");
    if has_serde {
        if context.serde_root
            || slice.contains("serde::Serialize")
            || slice.contains("serde::Deserialize")
        {
            return TypeFramework::Serde;
        }
        return TypeFramework::Unresolved("serde");
    }
    let has_clap = derives.contains("Parser")
        || derives.contains("Subcommand")
        || derives.contains("Args")
        || slice.contains("#[derive(clap::");
    if has_clap {
        if context.clap_root
            || slice.contains("clap::Parser")
            || slice.contains("clap::Subcommand")
            || slice.contains("clap::Args")
        {
            return TypeFramework::Clap;
        }
        return TypeFramework::Unresolved("clap");
    }
    TypeFramework::None
}

/// Emit framework support anchors plus expansion/binding UNKNOWNs for a
/// struct/enum or tokio function unit, based on its already-decided kind.
pub(super) fn framework_facts_for_unit(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
    context: &FrameworkUseContext,
) -> Result<Vec<SemanticFact>, ParseError> {
    match unit.kind {
        CodeUnitKind::SerdeModel => serde_facts(document, unit, slice),
        CodeUnitKind::ThiserrorErrorEnum => thiserror_facts(document, unit, slice),
        CodeUnitKind::ClapParser => clap_facts(document, unit, slice),
        CodeUnitKind::TokioEntry => tokio_facts(document, unit, "tokio.main"),
        CodeUnitKind::TokioTest => tokio_facts(document, unit, "tokio.test"),
        CodeUnitKind::RustStruct => unresolved_binding_facts(document, unit, slice, false, context),
        CodeUnitKind::RustEnum => unresolved_binding_facts(document, unit, slice, true, context),
        _ => Ok(Vec::new()),
    }
}

fn serde_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let derives = derive_tokens(slice);
    let attr_shape = serde_attr_shape(slice);
    let mut facts = Vec::new();
    let mut traits: Vec<&str> = Vec::new();
    if derives.contains("Serialize") || slice.contains("serde::Serialize") {
        traits.push("serde.Serialize");
    }
    if derives.contains("Deserialize") || slice.contains("serde::Deserialize") {
        traits.push("serde.Deserialize");
    }
    for target in traits {
        facts.push(anchors::structural_anchor_fact(
            document,
            unit,
            target,
            vec![
                "provider_resolved=false".to_string(),
                "rust_anchor_kind=serde_model".to_string(),
                format!("serde_attr_shape={attr_shape}"),
            ],
            "bounded Rust serde derive model anchor",
        )?);
    }
    facts.push(derive_expansion_unknown(document, unit)?);
    Ok(facts)
}

fn thiserror_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let message_shape = error_message_shape(slice);
    let facts = vec![
        anchors::structural_anchor_fact(
            document,
            unit,
            "thiserror.Error",
            vec![
                "provider_resolved=false".to_string(),
                "rust_anchor_kind=thiserror_error_enum".to_string(),
                format!("error_message_shape={message_shape}"),
            ],
            "bounded Rust thiserror error enum anchor",
        )?,
        derive_expansion_unknown(document, unit)?,
    ];
    Ok(facts)
}

fn clap_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let derives = derive_tokens(slice);
    let attr_shape = clap_attr_shape(slice);
    let mut facts = Vec::new();
    for (token, target) in [
        ("Parser", "clap.Parser"),
        ("Subcommand", "clap.Subcommand"),
        ("Args", "clap.Args"),
    ] {
        if derives.contains(token) || slice.contains(&format!("clap::{token}")) {
            facts.push(anchors::structural_anchor_fact(
                document,
                unit,
                target,
                vec![
                    "provider_resolved=false".to_string(),
                    "rust_anchor_kind=clap_parser".to_string(),
                    format!("clap_attr_shape={attr_shape}"),
                ],
                "bounded Rust clap derive parser anchor",
            )?);
        }
    }
    facts.push(derive_expansion_unknown(document, unit)?);
    Ok(facts)
}

fn tokio_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    target: &str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let anchor_kind = if target == "tokio.main" {
        "tokio_entry"
    } else {
        "tokio_test"
    };
    let facts = vec![
        anchors::structural_anchor_fact(
            document,
            unit,
            target,
            vec![
                "provider_resolved=false".to_string(),
                format!("rust_anchor_kind={anchor_kind}"),
            ],
            "bounded Rust tokio attribute anchor",
        )?,
        derive_expansion_unknown(document, unit)?,
    ];
    Ok(facts)
}

fn unresolved_binding_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
    is_enum: bool,
    context: &FrameworkUseContext,
) -> Result<Vec<SemanticFact>, ParseError> {
    match classify_type_item(slice, is_enum, context) {
        TypeFramework::Unresolved(framework) => Ok(vec![unknown::fact_with_assumptions(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "UnresolvedImport",
                affected_claim: "rust_framework_attribute_binding",
                kind: "framework_derive_without_use",
                note:
                    "Rust framework derive tokens are present without same-file use-path evidence",
            },
            vec![format!("rust_framework={framework}")],
        )?]),
        _ => Ok(Vec::new()),
    }
}

fn derive_expansion_unknown(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<SemanticFact, ParseError> {
    unknown::fact(
        document,
        unit,
        unit.range.start_byte,
        unit.range.end_byte,
        RustUnknownSpec {
            reason: "MacroOrPreprocessor",
            affected_claim: "rust_derive_expansion",
            kind: "derive_macro_expansion",
            note: "Rust derive/attribute macro expansion is not performed",
        },
    )
}

fn derive_tokens(slice: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut search = slice;
    while let Some(index) = search.find("#[derive(") {
        let after = &search[index + "#[derive(".len()..];
        let end = after.find(']').unwrap_or(after.len());
        let inner = &after[..end];
        for token in inner.split(',') {
            let token = token
                .trim()
                .rsplit("::")
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'));
            if !token.is_empty() {
                tokens.insert(token.to_string());
            }
        }
        search = &after[end..];
    }
    tokens
}

fn serde_attr_shape(slice: &str) -> &'static str {
    let Some(index) = slice.find("#[serde(") else {
        return "none";
    };
    let after = &slice[index + "#[serde(".len()..];
    let inner = &after[..after.find(']').unwrap_or(after.len())];
    for (marker, shape) in [
        ("rename_all", "rename_all"),
        ("untagged", "untagged"),
        ("tag", "tag"),
        ("skip", "skip"),
        ("default", "default"),
    ] {
        if inner.contains(marker) {
            return shape;
        }
    }
    "other"
}

fn clap_attr_shape(slice: &str) -> &'static str {
    if slice.contains("#[command(") {
        "command"
    } else if slice.contains("#[arg(") {
        "arg"
    } else {
        "none"
    }
}

fn error_message_shape(slice: &str) -> &'static str {
    let Some(index) = slice.find("#[error(") else {
        return "none";
    };
    let after = &slice[index + "#[error(".len()..];
    let inner = &after[..after.find(']').unwrap_or(after.len())];
    if inner.contains('{') {
        "formatted"
    } else {
        "literal"
    }
}

/// True when `slice` carries a bare `#[name]` attribute (not a `name::` path or
/// a longer identifier).
fn has_bare_attribute(slice: &str, name: &str) -> bool {
    let needle = format!("#[{name}]");
    slice.contains(&needle)
}

// ---------------------------------------------------------------------------
// axum literal route detection
// ---------------------------------------------------------------------------

/// Outcome of analyzing a `.route(...)` method-call segment.
pub(super) enum AxumRouteOutcome {
    /// A literal route with a resolved helper and a Router::new()-rooted
    /// receiver: (route path literal name, http method).
    Route {
        name: String,
        http_method: &'static str,
    },
    /// An axum-shaped route segment that failed a gate (blocking identity).
    Blocked,
    /// Not an axum route segment.
    NotRoute,
}

/// Analyze a `call_expression` node as a possible `.route("literal", verb(h))`
/// segment. `receiver_traces` reports whether the receiver chain reaches
/// `Router::new()` (traced by the caller, which owns the binding table).
pub(super) fn analyze_axum_route_call(
    node: Node<'_>,
    source: &str,
    context: &FrameworkUseContext,
    receiver_traces: bool,
) -> AxumRouteOutcome {
    let Some(function) = node.child_by_field_name("function") else {
        return AxumRouteOutcome::NotRoute;
    };
    if function.kind() != "field_expression" {
        return AxumRouteOutcome::NotRoute;
    }
    let field = function
        .child_by_field_name("field")
        .and_then(|child| node_text_checked(source, child));
    if field != Some("route") {
        return AxumRouteOutcome::NotRoute;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return AxumRouteOutcome::NotRoute;
    };
    let mut cursor = arguments.walk();
    let args = arguments.named_children(&mut cursor).collect::<Vec<_>>();
    if args.len() != 2 {
        return AxumRouteOutcome::NotRoute;
    }
    let Some((helper, http_method, helper_inline_qualified)) = route_helper(args[1], source) else {
        return AxumRouteOutcome::NotRoute;
    };
    // Only treat this as an axum route attempt when there is at least one axum
    // signal, to avoid flagging unrelated `.route` chains.
    let helper_resolved = helper_inline_qualified
        || context.axum_routing_module
        || context.axum_routing_helpers.contains(&helper);
    if !helper_resolved && !receiver_traces {
        return AxumRouteOutcome::NotRoute;
    }
    let path_is_literal = args[0].kind() == "string_literal";
    if helper_resolved && receiver_traces && path_is_literal {
        let name = node_text_checked(source, args[0])
            .map(|literal| literal.trim_matches('"').to_string())
            .unwrap_or_else(|| http_method.to_string());
        AxumRouteOutcome::Route { name, http_method }
    } else {
        AxumRouteOutcome::Blocked
    }
}

fn route_helper(arg: Node<'_>, source: &str) -> Option<(String, &'static str, bool)> {
    if arg.kind() != "call_expression" {
        return None;
    }
    let function = arg.child_by_field_name("function")?;
    let (name, inline_qualified) = match function.kind() {
        "identifier" => (node_text_checked(source, function)?.to_string(), false),
        "scoped_identifier" => {
            let text = node_text_checked(source, function)?;
            let inline = text.contains("routing");
            (text.rsplit("::").next().unwrap_or(text).to_string(), inline)
        }
        _ => return None,
    };
    let http_method = match name.as_str() {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "delete" => "DELETE",
        "patch" => "PATCH",
        _ => return None,
    };
    Some((name, http_method, inline_qualified))
}

/// True when `node` is (transitively) a receiver chain rooted at
/// `Router::new()`. `binding_initializer` resolves a same-function `let`
/// binding's initializer text.
pub(super) fn receiver_traces_to_router(
    node: Node<'_>,
    source: &str,
    binding_initializer: &impl Fn(&str) -> Option<String>,
    depth: usize,
) -> bool {
    if depth == 0 {
        return false;
    }
    match node.kind() {
        "call_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return false;
            };
            match function.kind() {
                "scoped_identifier" => node_text_checked(source, function)
                    .is_some_and(|text| text.ends_with("Router::new")),
                "field_expression" => function.child_by_field_name("value").is_some_and(|value| {
                    receiver_traces_to_router(value, source, binding_initializer, depth - 1)
                }),
                _ => false,
            }
        }
        "identifier" => node_text_checked(source, node)
            .and_then(binding_initializer)
            .is_some_and(|initializer| initializer.contains("Router::new(")),
        _ => false,
    }
}

pub(super) fn axum_route_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    http_method: &'static str,
) -> Result<Vec<SemanticFact>, ParseError> {
    let facts = vec![
        anchors::structural_anchor_fact(
            document,
            unit,
            "axum.routing.route",
            vec![
                "provider_resolved=false".to_string(),
                "rust_anchor_kind=axum_route".to_string(),
                format!("http_method={http_method}"),
                "route_path_shape=literal".to_string(),
            ],
            "bounded Rust axum literal route anchor",
        )?,
        unknown::fact(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "FrameworkMagic",
                affected_claim: "rust_axum_extractor_semantics",
                kind: "axum_extractor",
                note: "Rust axum handler extractor trait resolution is not performed",
            },
        )?,
    ];
    Ok(facts)
}

pub(super) fn axum_route_identity_unknown(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
) -> Result<SemanticFact, ParseError> {
    unknown::fact(
        document,
        unit,
        start_byte,
        end_byte,
        RustUnknownSpec {
            reason: "UnresolvedImport",
            affected_claim: "rust_axum_route_identity",
            kind: "axum_route_unresolved",
            note: "Rust axum route segment has a non-literal path, unresolved helper, or an untraceable Router receiver",
        },
    )
}

pub(super) fn axum_middleware_unknown(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
) -> Result<SemanticFact, ParseError> {
    unknown::fact(
        document,
        unit,
        start_byte,
        end_byte,
        RustUnknownSpec {
            reason: "FrameworkMagic",
            affected_claim: "rust_axum_middleware_semantics",
            kind: "axum_middleware",
            note: "Rust axum tower middleware ordering semantics are not resolved",
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_serde() -> FrameworkUseContext {
        FrameworkUseContext {
            serde_root: true,
            ..FrameworkUseContext::default()
        }
    }

    #[test]
    fn derive_tokens_extracts_trait_names() {
        let tokens = derive_tokens("#[derive(Serialize, Deserialize, Debug)]\npub struct A;");
        assert!(tokens.contains("Serialize"));
        assert!(tokens.contains("Deserialize"));
        assert!(tokens.contains("Debug"));
    }

    #[test]
    fn serde_requires_use_evidence() {
        let slice = "#[derive(Serialize, Deserialize)]\npub struct A { id: u64 }";
        assert_eq!(
            classify_type_item(slice, false, &ctx_with_serde()),
            TypeFramework::Serde
        );
        assert_eq!(
            classify_type_item(slice, false, &FrameworkUseContext::default()),
            TypeFramework::Unresolved("serde")
        );
    }

    #[test]
    fn inline_qualified_serde_needs_no_use() {
        let slice = "#[derive(serde::Serialize)]\npub struct A;";
        assert_eq!(
            classify_type_item(slice, false, &FrameworkUseContext::default()),
            TypeFramework::Serde
        );
    }

    #[test]
    fn thiserror_requires_error_attribute_and_enum() {
        let context = FrameworkUseContext {
            thiserror_root: true,
            ..FrameworkUseContext::default()
        };
        let slice = "#[derive(Error, Debug)]\npub enum E { #[error(\"boom\")] Boom }";
        assert_eq!(
            classify_type_item(slice, true, &context),
            TypeFramework::Thiserror
        );
        // A struct with the same tokens is not a thiserror enum.
        assert_eq!(
            classify_type_item(slice, false, &context),
            TypeFramework::None
        );
    }

    #[test]
    fn error_message_shape_distinguishes_literal_and_formatted() {
        assert_eq!(error_message_shape("#[error(\"boom\")]"), "literal");
        assert_eq!(error_message_shape("#[error(\"boom {0}\")]"), "formatted");
    }

    #[test]
    fn use_context_parses_axum_routing_helpers() {
        let mut context = FrameworkUseContext::default();
        record_use_path("use axum::routing::{get, post};", &mut context);
        assert!(context.axum_routing_helpers.contains("get"));
        assert!(context.axum_routing_helpers.contains("post"));
        record_use_path("use serde::{Serialize, Deserialize};", &mut context);
        assert!(context.serde_root);
        record_use_path("use serde_json::Value;", &mut context);
        // serde_json must not be mistaken for serde.
        assert!(!context.tokio_main_import);
    }
}
