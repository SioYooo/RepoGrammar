//! Conservative TS/JS exact-anchor extraction.
//!
//! This pass runs after syntax-only code-unit extraction. It emits `STRUCTURAL`
//! semantic facts ONLY for code units whose framework usage can be resolved
//! through exact import/require bindings and literal call shapes. Anything that
//! is dynamic, reassigned, shadowed, conditionally imported, or merely a
//! lookalike yields no anchor, so the family layer keeps it `UNKNOWN`. These
//! structural anchors are later promoted to bounded `DATAFLOW_DERIVED` support
//! facts by the application layer; they never prove membership by themselves.

use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, SourceDocument};
use std::collections::{BTreeMap, BTreeSet};

/// Engine identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_ENGINE: &str = "repogrammar-tsjs-syntax";
/// Method identity for parser-emitted TS/JS structural anchors.
pub const TSJS_ANCHOR_METHOD: &str = "exact_anchor_v1";

const HTTP_METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "use"];
const RUNNER_MODULES: [&str; 2] = ["vitest", "@jest/globals"];

/// Extract exact framework anchors for the given units. Returns `STRUCTURAL`
/// facts whose evidence spans the full owning unit range.
pub fn exact_framework_anchors(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    ambient_runner_allowed: bool,
) -> Result<Vec<SemanticFact>, ParseError> {
    let bindings = ModuleBindings::analyze(document.text);
    let mut facts = Vec::new();
    for unit in units {
        match anchor_for_unit(document, &bindings, unit, ambient_runner_allowed) {
            AnchorOutcome::Anchor(anchor) => facts.push(anchor_fact(document, unit, anchor)?),
            AnchorOutcome::Unknown(unknown) => facts.push(unknown_fact(document, unit, unknown)?),
            AnchorOutcome::None => {}
        }
    }
    Ok(facts)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TestRunnerCallNames {
    pub suite_names: BTreeSet<String>,
    pub test_names: BTreeSet<String>,
}

pub(crate) fn exact_test_runner_call_names(text: &str) -> TestRunnerCallNames {
    let bindings = ModuleBindings::analyze(text);
    let mut names = TestRunnerCallNames::default();
    for (local, binding) in &bindings.imports {
        if bindings.unsafe_names.contains(local)
            || !RUNNER_MODULES.contains(&binding.module.as_str())
        {
            continue;
        }
        let ImportKind::Named(original) = &binding.kind else {
            continue;
        };
        match original.as_str() {
            "describe" => {
                names.suite_names.insert(local.clone());
            }
            "it" | "test" => {
                names.test_names.insert(local.clone());
            }
            _ => {}
        }
    }
    names
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Anchor {
    target: String,
    fact_kind: SemanticFactKind,
    assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnknownAnchor {
    reason: UnknownReasonCode,
    affected_claim: &'static str,
    kind: &'static str,
    note: &'static str,
}

enum AnchorOutcome {
    Anchor(Anchor),
    Unknown(UnknownAnchor),
    None,
}

fn anchor_for_unit(
    document: &SourceDocument<'_>,
    bindings: &ModuleBindings,
    unit: &CodeUnit,
    ambient_runner_allowed: bool,
) -> AnchorOutcome {
    let Some(slice) = document
        .text
        .get(unit.range.start_byte..unit.range.end_byte)
    else {
        return AnchorOutcome::None;
    };
    match unit.kind {
        CodeUnitKind::ExpressRoute => express_route_anchor(bindings, slice),
        CodeUnitKind::TestSuite => {
            test_anchor(document, bindings, slice, true, ambient_runner_allowed)
        }
        CodeUnitKind::TestCase => {
            test_anchor(document, bindings, slice, false, ambient_runner_allowed)
        }
        _ => AnchorOutcome::None,
    }
}

fn express_route_anchor(bindings: &ModuleBindings, slice: &str) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_support_target",
            kind: "dynamic_route_call",
            note: "TS/JS route call shape is dynamic",
        });
    };
    if !HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "tsjs_support_target",
            kind: "unsupported_route_method",
            note: "TS/JS route method is not in the exact anchor allowlist",
        });
    }
    if bindings.unsafe_names.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_receiver_binding",
            kind: "unsafe_receiver_binding",
            note: "TS/JS route receiver is reassigned or redeclared",
        });
    }
    if !bindings.express_receivers.contains_key(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "tsjs_receiver_binding",
            kind: "unresolved_express_receiver",
            note: "TS/JS route receiver is not an exact Express app/router binding",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=express_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
    ];
    if let Some(path_shape) = route_path_shape(slice) {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("express.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn test_anchor(
    document: &SourceDocument<'_>,
    bindings: &ModuleBindings,
    slice: &str,
    is_suite: bool,
    ambient_runner_allowed: bool,
) -> AnchorOutcome {
    let Some(name) = test_call_name(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "dynamic_test_call",
            note: "TS/JS test runner call shape is dynamic",
        });
    };
    if bindings.unsafe_names.contains(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is locally reassigned or redeclared",
        });
    }
    if bindings.local_decls.contains(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is a local custom wrapper",
        });
    }
    if let Some((module, original)) = bindings.imported_runner(name) {
        if (is_suite && original == "describe") || (!is_suite && matches!(original, "it" | "test"))
        {
            return AnchorOutcome::Anchor(test_anchor_for_runner(
                name, original, module, is_suite, slice,
            ));
        }
    }
    if bindings.imports.contains_key(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "unresolved_test_runner",
            note: "TS/JS test runner import does not resolve to a known runner",
        });
    }
    let expected_ambient = if is_suite {
        name == "describe"
    } else {
        name == "it" || name == "test"
    };
    if expected_ambient && is_ambient_runner(document.path, bindings, name) {
        if !ambient_runner_allowed {
            return AnchorOutcome::Unknown(UnknownAnchor {
                reason: UnknownReasonCode::MissingProjectConfig,
                affected_claim: "tsjs_runner_binding",
                kind: "ambient_runner_without_project_context",
                note: "TS/JS ambient test runner lacks bounded project test context",
            });
        }
        return AnchorOutcome::Anchor(test_anchor_for_runner(
            name, name, "ambient", is_suite, slice,
        ));
    }
    AnchorOutcome::Unknown(UnknownAnchor {
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: "tsjs_runner_binding",
        kind: "unresolved_test_runner",
        note: "TS/JS test runner binding is not exact",
    })
}

fn test_anchor_for_runner(
    local_name: &str,
    original: &str,
    runner_kind: &str,
    is_suite: bool,
    slice: &str,
) -> Anchor {
    Anchor {
        target: format!("jest_vitest.{original}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            format!(
                "tsjs_anchor_kind={}",
                if is_suite { "test_suite" } else { "test_case" }
            ),
            format!("runner_kind={runner_kind}"),
            format!("test_shape={original}"),
            format!("async_shape={}", async_shape(slice)),
            format!("import_context={local_name}"),
        ],
    }
}

/// A bare `describe`/`it`/`test` is only treated as a runner global in an actual
/// test file and only when the name is not locally declared or imported from a
/// non-runner module (a custom wrapper / alias).
fn is_ambient_runner(path: &str, bindings: &ModuleBindings, name: &str) -> bool {
    is_test_file(path)
        && !bindings.local_decls.contains(name)
        && !bindings.imports.contains_key(name)
}

fn is_test_file(path: &str) -> bool {
    const SUFFIXES: [&str; 8] = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

fn anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    anchor: Anchor,
) -> Result<SemanticFact, ParseError> {
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let evidence = Evidence::new(
        CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
        SourceRange::new(unit.range.start_byte, unit.range.end_byte)
            .map_err(ParseError::Internal)?,
        provenance,
        "bounded TS/JS exact framework anchor",
    )
    .map_err(ParseError::Internal)?;
    Ok(SemanticFact {
        kind: anchor.fact_kind,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(anchor.target).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: TSJS_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence,
        assumptions: anchor.assumptions,
    })
}

fn unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    unknown: UnknownAnchor,
) -> Result<SemanticFact, ParseError> {
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let evidence = Evidence::new(
        CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
        SourceRange::new(unit.range.start_byte, unit.range.end_byte)
            .map_err(ParseError::Internal)?,
        provenance,
        unknown.note,
    )
    .map_err(ParseError::Internal)?;
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(
            SymbolId::new(unknown.reason.as_protocol_str()).map_err(ParseError::Internal)?,
        ),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: TSJS_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence,
        assumptions: vec![
            format!("affected_claim={}", unknown.affected_claim),
            format!("tsjs_unknown_kind={}", unknown.kind),
        ],
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ImportKind {
    Default,
    Namespace,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportBinding {
    module: String,
    kind: ImportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpressReceiver {
    App,
    Router,
}

#[derive(Debug, Default)]
struct ModuleBindings {
    imports: BTreeMap<String, ImportBinding>,
    local_decls: BTreeSet<String>,
    unsafe_names: BTreeSet<String>,
    express_receivers: BTreeMap<String, ExpressReceiver>,
}

impl ModuleBindings {
    fn analyze(text: &str) -> Self {
        let mut declared_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut reassigned: BTreeSet<String> = BTreeSet::new();
        let mut imports: BTreeMap<String, ImportBinding> = BTreeMap::new();
        let mut local_decls: BTreeSet<String> = BTreeSet::new();
        let mut top_level_lines: Vec<String> = Vec::new();

        let mut depth: i64 = 0;
        for raw_line in text.lines() {
            let at_top_level = depth == 0;
            if let Some(name) = bare_assignment_name(raw_line) {
                reassigned.insert(name.to_string());
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

        let mut unsafe_names: BTreeSet<String> = reassigned;
        for (name, count) in &declared_counts {
            if *count > 1 {
                unsafe_names.insert(name.clone());
            }
        }

        let mut express_receivers: BTreeMap<String, ExpressReceiver> = BTreeMap::new();
        for line in &top_level_lines {
            if let Some((name, receiver)) =
                express_receiver_declaration(line, &imports, &unsafe_names)
            {
                if !unsafe_names.contains(&name) {
                    express_receivers.insert(name, receiver);
                }
            }
        }

        Self {
            imports,
            local_decls,
            unsafe_names,
            express_receivers,
        }
    }

    fn imported_runner(&self, name: &str) -> Option<(&str, &str)> {
        match self.imports.get(name) {
            Some(binding) if RUNNER_MODULES.contains(&binding.module.as_str()) => {
                match &binding.kind {
                    ImportKind::Named(original) => {
                        Some((binding.module.as_str(), original.as_str()))
                    }
                    ImportKind::Default | ImportKind::Namespace => None,
                }
            }
            _ => None,
        }
    }
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

fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export ").unwrap_or(line)
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
        return parse_named_specifiers(lhs)
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
        let (original, local) = match part.split_once(" as ") {
            Some((original, local)) => (original.trim(), local.trim()),
            None => (part, part),
        };
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

fn first_quoted(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let quote = bytes[index];
        if quote == b'"' || quote == b'\'' {
            let start = index + 1;
            let end_relative = text[start..].find(quote as char)?;
            return Some(text[start..start + end_relative].to_string());
        }
        index += 1;
    }
    None
}

fn declared_identifiers(line: &str) -> Vec<String> {
    let trimmed = strip_export_prefix(line.trim());
    for keyword in ["const ", "let ", "var "] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim_start();
            if rest.starts_with('{') {
                return parse_named_specifiers(rest)
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

fn express_receiver_declaration(
    line: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<(String, ExpressReceiver)> {
    let trimmed = strip_export_prefix(line.trim());
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (name, after) = leading_identifier(rest.trim_start())?;
    let after_name = &rest.trim_start()[after..];
    let rhs = after_name.trim_start().strip_prefix('=')?.trim();
    let receiver = express_receiver_from_rhs(rhs, imports, unsafe_names)?;
    Some((name.to_string(), receiver))
}

fn express_receiver_from_rhs(
    rhs: &str,
    imports: &BTreeMap<String, ImportBinding>,
    unsafe_names: &BTreeSet<String>,
) -> Option<ExpressReceiver> {
    let rhs = rhs.trim().trim_end_matches(';').trim();
    let (head, after) = leading_identifier(rhs)?;
    if unsafe_names.contains(head) {
        return None;
    }
    let tail = rhs[after..].trim_start();
    if tail == "()" {
        let binding = imports.get(head)?;
        if binding.module != "express" {
            return None;
        }
        return match &binding.kind {
            ImportKind::Default | ImportKind::Namespace => Some(ExpressReceiver::App),
            ImportKind::Named(original) if original == "Router" => Some(ExpressReceiver::Router),
            ImportKind::Named(_) => None,
        };
    }
    let member_rest = tail.strip_prefix('.')?;
    let (member, after_member) = leading_identifier(member_rest)?;
    if member != "Router" || member_rest[after_member..].trim_start() != "()" {
        return None;
    }
    let binding = imports.get(head)?;
    if binding.module == "express"
        && matches!(binding.kind, ImportKind::Default | ImportKind::Namespace)
    {
        Some(ExpressReceiver::Router)
    } else {
        None
    }
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

fn route_call_parts(slice: &str) -> Option<(&str, &str)> {
    let (receiver, after) = leading_identifier(slice)?;
    let rest = slice[after..].trim_start().strip_prefix('.')?;
    let (method, after_method) = leading_identifier(rest)?;
    if !rest[after_method..].trim_start().starts_with('(') {
        return None;
    }
    Some((receiver, method))
}

fn route_path_shape(slice: &str) -> Option<String> {
    let open = slice.find('(')?;
    let path = first_quoted(&slice[open + 1..])?;
    Some(normalize_route_path(&path))
}

fn normalize_route_path(path: &str) -> String {
    let normalized = path
        .split('/')
        .map(|segment| {
            if segment.is_empty() {
                String::new()
            } else if segment.starts_with(':') {
                ":param".to_string()
            } else if segment
                .chars()
                .any(|character| character == '*' || character == '?')
            {
                ":pattern".to_string()
            } else if segment.chars().all(|character| character.is_ascii_digit()) {
                ":number".to_string()
            } else {
                segment.to_ascii_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

fn handler_shape(slice: &str) -> &'static str {
    let has_inline_arrow = slice.contains("=>");
    let has_inline_function = slice.contains("function");
    let has_req_body = slice.contains(".body");
    let has_req_query = slice.contains(".query");
    let has_req_params = slice.contains(".params");
    let has_res_json = slice.contains(".json(");
    let has_res_send = slice.contains(".send(");
    let has_res_end = slice.contains(".end(");
    match (
        has_inline_arrow || has_inline_function,
        has_req_body,
        has_req_query,
        has_req_params,
        has_res_json,
        has_res_send,
        has_res_end,
    ) {
        (true, true, _, _, true, _, _) => "inline_body_json",
        (true, _, true, _, true, _, _) => "inline_query_json",
        (true, _, _, true, true, _, _) => "inline_params_json",
        (true, _, _, _, true, _, _) => "inline_json",
        (true, _, _, _, _, true, _) => "inline_send",
        (true, _, _, _, _, _, true) => "inline_end",
        (true, _, _, _, _, _, _) => "inline_handler",
        _ => "referenced_handler",
    }
}

fn async_shape(slice: &str) -> &'static str {
    if slice.contains("async ") || slice.contains("async(") || slice.contains("async (") {
        "async"
    } else {
        "sync"
    }
}

fn test_call_name(slice: &str) -> Option<&str> {
    let (name, after) = leading_identifier(slice)?;
    if !slice[after..].trim_start().starts_with('(') {
        return None;
    }
    Some(name)
}

fn leading_identifier(text: &str) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let start = index;
    if index >= bytes.len() || !is_identifier_start(bytes[index]) {
        return None;
    }
    index += 1;
    while index < bytes.len() && is_identifier_byte(bytes[index]) {
        index += 1;
    }
    Some((&text[start..index], index))
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
    use crate::core::model::{ContentHash, Language, RepositoryRevision};
    use crate::ports::parser::{ParserProjectContext, SourceParser};

    fn parse_facts(path: &str, text: &str) -> Vec<SemanticFact> {
        parse_facts_with_context(path, text, None)
    }

    fn parse_facts_with_test_context(path: &str, text: &str) -> Vec<SemanticFact> {
        parse_facts_with_context(
            path,
            text,
            Some(ParserProjectContext {
                tsjs_has_test_runner_context: true,
                ..ParserProjectContext::default()
            }),
        )
    }

    fn parse_facts_with_context(
        path: &str,
        text: &str,
        context: Option<ParserProjectContext>,
    ) -> Vec<SemanticFact> {
        let language = if path.ends_with(".js") || path.ends_with(".jsx") {
            Language::JavaScript
        } else {
            Language::TypeScript
        };
        let document = SourceDocument {
            path,
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        };
        match context {
            Some(context) => {
                SyntaxCodeUnitParser
                    .parse_with_context(document, &context)
                    .expect("parse with context")
                    .semantic_facts
            }
            None => {
                SyntaxCodeUnitParser
                    .parse(document)
                    .expect("parse")
                    .semantic_facts
            }
        }
    }

    fn targets(path: &str, text: &str) -> Vec<String> {
        targets_from_facts(parse_facts(path, text))
    }

    fn targets_with_test_context(path: &str, text: &str) -> Vec<String> {
        targets_from_facts(parse_facts_with_test_context(path, text))
    }

    fn targets_from_facts(facts: Vec<SemanticFact>) -> Vec<String> {
        let mut targets = facts
            .iter()
            .filter(|fact| fact.kind != SemanticFactKind::Unknown)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_kinds(path: &str, text: &str) -> Vec<String> {
        unknown_kinds_from_facts(parse_facts(path, text))
    }

    fn unknown_kinds_with_test_context(path: &str, text: &str) -> Vec<String> {
        unknown_kinds_from_facts(parse_facts_with_test_context(path, text))
    }

    fn unknown_kinds_from_facts(facts: Vec<SemanticFact>) -> Vec<String> {
        let mut kinds = facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .filter_map(|fact| {
                fact.assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("tsjs_unknown_kind="))
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        kinds.sort();
        kinds
    }

    #[test]
    fn express_default_import_and_app_routes_anchor_each_literal_method() {
        let text = r#"import express from "express";
const app = express();
app.get("/users", (req, res) => { res.json([]); });
app.post("/users", (req, res) => { res.json({}); });
app.delete("/users/:id", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/server.ts", text),
            vec![
                "express.route.delete".to_string(),
                "express.route.get".to_string(),
                "express.route.post".to_string(),
            ]
        );
        for fact in parse_facts("src/server.ts", text) {
            assert_eq!(fact.certainty, FactCertainty::Structural);
            assert_eq!(fact.origin.engine, TSJS_ANCHOR_ENGINE);
            assert_eq!(fact.origin.method, TSJS_ANCHOR_METHOD);
        }
        let facts = parse_facts("src/server.ts", text);
        let get_fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "express.route.get")
            })
            .expect("get route fact");
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "route_method=get"));
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "route_path_shape=/users"));
        assert!(get_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "handler_shape=inline_json"));
    }

    #[test]
    fn express_router_named_and_namespace_factories_anchor() {
        let named = r#"import { Router } from "express";
const router = Router();
router.get("/a", (req, res) => { res.end(); });
router.use((req, res, next) => { next(); });
"#;
        assert_eq!(
            targets("src/router.ts", named),
            vec![
                "express.route.get".to_string(),
                "express.route.use".to_string()
            ]
        );

        let namespaced = r#"import * as express from "express";
const router = express.Router();
router.patch("/a", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/ns.ts", namespaced),
            vec!["express.route.patch".to_string()]
        );

        let required = r#"const express = require("express");
const app = express();
app.put("/a", (req, res) => { res.end(); });
"#;
        assert_eq!(
            targets("src/cjs.js", required),
            vec!["express.route.put".to_string()]
        );
    }

    #[test]
    fn express_object_literal_lookalike_has_no_anchor() {
        let text = r#"const app = { get(path, handler) { return handler; } };
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/fake.ts", text).is_empty());
        assert_eq!(
            unknown_kinds("src/fake.ts", text),
            vec!["unresolved_express_receiver".to_string()]
        );
    }

    #[test]
    fn express_reassigned_or_shadowed_app_has_no_anchor() {
        let reassigned = r#"import express from "express";
let app = express();
app = makeOtherApp();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/reassigned.ts", reassigned).is_empty());
        assert_eq!(
            unknown_kinds("src/reassigned.ts", reassigned),
            vec!["unsafe_receiver_binding".to_string()]
        );

        let shadowed = r#"import express from "express";
const express2 = express;
const express = buildFake();
const app = express();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/shadowed.ts", shadowed).is_empty());
        assert_eq!(
            unknown_kinds("src/shadowed.ts", shadowed),
            vec!["unresolved_express_receiver".to_string()]
        );
    }

    #[test]
    fn express_dynamic_receiver_or_unresolved_import_has_no_anchor() {
        let dynamic = r#"import express from "express";
const app = express();
getRouter().get("/users", (req, res) => { res.json([]); });
"#;
        // getRouter() is not a resolved binding, so no anchor is produced.
        assert!(targets("src/dynamic.ts", dynamic).is_empty());
        assert_eq!(
            unknown_kinds("src/dynamic.ts", dynamic),
            vec!["dynamic_route_call".to_string()]
        );

        let unresolved = r#"const app = makeApp();
app.get("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/unresolved.ts", unresolved).is_empty());
        assert_eq!(
            unknown_kinds("src/unresolved.ts", unresolved),
            vec!["unresolved_express_receiver".to_string()]
        );

        let dynamic_method = r#"import express from "express";
const app = express();
const method = "get";
app[method]("/users", (req, res) => { res.json([]); });
"#;
        assert!(targets("src/dynamic-method.ts", dynamic_method).is_empty());
        assert_eq!(
            unknown_kinds("src/dynamic-method.ts", dynamic_method),
            vec!["dynamic_route_call".to_string()]
        );
    }

    #[test]
    fn jest_vitest_imported_runners_anchor_suites_and_tests() {
        let text = r#"import { describe, it, test } from "vitest";
describe("users", () => {
  it("loads", () => {});
  test("filters", () => {});
});
"#;
        assert_eq!(
            targets("src/users.test.ts", text),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string(),
                "jest_vitest.test".to_string(),
            ]
        );

        let jest = r#"import { describe, it } from "@jest/globals";
describe("accounts", () => {
  it("works", () => {});
});
"#;
        assert_eq!(
            targets("src/accounts.spec.ts", jest),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string()
            ]
        );
    }

    #[test]
    fn jest_vitest_imported_runner_aliases_anchor_suites_and_tests() {
        let text = r#"import { describe as suite, test as case_ } from "vitest";
suite("orders", () => {
  case_("creates", async () => {});
});
"#;
        assert_eq!(
            targets("src/orders.test.ts", text),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.test".to_string(),
            ]
        );
        let facts = parse_facts("src/orders.test.ts", text);
        let suite = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "jest_vitest.describe")
            })
            .expect("suite alias fact");
        assert!(suite
            .assumptions
            .iter()
            .any(|assumption| assumption == "runner_kind=vitest"));
        assert!(suite
            .assumptions
            .iter()
            .any(|assumption| assumption == "import_context=suite"));
        let case_fact = facts
            .iter()
            .find(|fact| {
                fact.target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "jest_vitest.test")
            })
            .expect("test alias fact");
        assert!(case_fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "async_shape=async"));
    }

    #[test]
    fn jest_vitest_ambient_globals_anchor_only_in_test_files() {
        let ambient = r#"describe("users", () => {
  it("loads", () => {});
});
"#;
        assert_eq!(
            targets_with_test_context("src/users.test.ts", ambient),
            vec![
                "jest_vitest.describe".to_string(),
                "jest_vitest.it".to_string()
            ]
        );
        assert!(targets("src/users.test.ts", ambient).is_empty());
        assert_eq!(
            unknown_kinds("src/users.test.ts", ambient),
            vec![
                "ambient_runner_without_project_context".to_string(),
                "ambient_runner_without_project_context".to_string()
            ]
        );

        // Same source in a non-test file is ambiguous and yields no anchor.
        assert!(targets("src/users.ts", ambient).is_empty());
    }

    #[test]
    fn jest_vitest_custom_wrapper_or_foreign_import_has_no_anchor() {
        let wrapper = r#"const it = makeWrapper();
describe("users", () => {
  it("loads", () => {});
});
"#;
        // `it` is locally declared (a custom wrapper), so the test case has no anchor;
        // `describe` is ambient in this test file and still anchors with project context.
        assert_eq!(
            targets_with_test_context("src/users.test.ts", wrapper),
            vec!["jest_vitest.describe".to_string()]
        );
        assert_eq!(
            unknown_kinds_with_test_context("src/users.test.ts", wrapper),
            vec!["unsafe_test_runner_binding".to_string()]
        );

        let foreign = r#"import { it } from "./helpers";
it("loads", () => {});
"#;
        assert!(targets("src/users.test.ts", foreign).is_empty());
        assert_eq!(
            unknown_kinds("src/users.test.ts", foreign),
            vec!["unresolved_test_runner".to_string()]
        );
    }
}
