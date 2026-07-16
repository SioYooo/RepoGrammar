use super::scope_graph::{ImportKind, ScopeGraphLite};
use super::{leading_identifier, Anchor, AnchorOutcome, UnknownAnchor};
use crate::core::model::{CodeUnit, CodeUnitKind, SemanticFactKind, UnknownReasonCode};

const NEST_COMMON_MODULE: &str = "@nestjs/common";
const NEST_HTTP_METHODS: [&str; 8] = [
    "Get", "Post", "Put", "Delete", "Patch", "Head", "Options", "All",
];

/// Bounded, file-local context: byte ranges of NestJS controller units whose
/// `@Controller` decorator resolves to an exact `@nestjs/common` import. A route
/// method only anchors when it lies inside one of these ranges.
pub(super) struct NestContext {
    controller_ranges: Vec<(usize, usize)>,
}

impl NestContext {
    pub(super) fn analyze(bindings: &ScopeGraphLite, text: &str, units: &[CodeUnit]) -> Self {
        let mut controller_ranges = Vec::new();
        for unit in units {
            if unit.kind != CodeUnitKind::NestController {
                continue;
            }
            let Some(slice) = text.get(unit.range.start_byte..unit.range.end_byte) else {
                continue;
            };
            if decorator_original(bindings, slice, |original| original == "Controller").is_some() {
                controller_ranges.push((unit.range.start_byte, unit.range.end_byte));
            }
        }
        Self { controller_ranges }
    }

    fn contains(&self, start_byte: usize) -> bool {
        self.controller_ranges
            .iter()
            .any(|(start, end)| *start <= start_byte && start_byte < *end)
    }
}

pub(super) fn controller_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    _start_byte: usize,
) -> AnchorOutcome {
    let Some((_, args)) = decorator_original(bindings, slice, |original| original == "Controller")
    else {
        return unresolved_controller_import();
    };
    let anchor = Anchor {
        target: "nestjs.common.Controller".to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_controller".to_string(),
            format!("class_route_path_shape={}", arg_path_shape(args)),
            "support_family=nestjs.common.Controller".to_string(),
        ],
    };
    AnchorOutcome::AnchorWithSubclaims(anchor, vec![di_subclaim()])
}

pub(super) fn injectable_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    _start_byte: usize,
) -> AnchorOutcome {
    if decorator_original(bindings, slice, |original| original == "Injectable").is_none() {
        return unresolved_controller_import();
    }
    let anchor = Anchor {
        target: "nestjs.common.Injectable".to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_injectable".to_string(),
            "support_family=nestjs.common.Injectable".to_string(),
        ],
    };
    AnchorOutcome::AnchorWithSubclaims(anchor, vec![di_subclaim()])
}

pub(super) fn module_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    _start_byte: usize,
) -> AnchorOutcome {
    let Some((_, args)) = decorator_original(bindings, slice, |original| original == "Module")
    else {
        return unresolved_controller_import();
    };
    let anchor = Anchor {
        target: "nestjs.common.Module".to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_module".to_string(),
            format!("nest_module_shape={}", module_shape(args)),
            "support_family=nestjs.common.Module".to_string(),
        ],
    };
    if args.is_some_and(|inner| inner.contains("forRoot")) {
        return AnchorOutcome::AnchorWithSubclaims(
            anchor,
            vec![UnknownAnchor {
                reason: UnknownReasonCode::FrameworkMagic,
                affected_claim: "tsjs_nest_dynamic_module",
                kind: "tsjs_nest_dynamic_module",
                note: "NestJS dynamic module (forRoot/forRootAsync) metadata is a runtime concern",
            }],
        );
    }
    AnchorOutcome::Anchor(anchor)
}

pub(super) fn route_anchor(
    bindings: &ScopeGraphLite,
    nest_context: &NestContext,
    slice: &str,
    start_byte: usize,
) -> AnchorOutcome {
    let Some((original, args)) = decorator_original(bindings, slice, |original| {
        NEST_HTTP_METHODS.contains(&original)
    }) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "tsjs_nest_controller_identity",
            kind: "nest_unresolved_route_import",
            note: "NestJS route decorator is not an exact @nestjs/common HTTP-method import",
        });
    };
    if !nest_context.contains(start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_nest_controller_identity",
            kind: "nest_route_outside_controller",
            note: "NestJS route method is not inside an exact @nestjs/common controller",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("nestjs.common.{original}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=nest_route".to_string(),
            format!("http_method={}", original.to_ascii_lowercase()),
            format!("route_path_shape={}", arg_path_shape(args)),
        ],
    })
}

fn unresolved_controller_import() -> AnchorOutcome {
    AnchorOutcome::Unknown(UnknownAnchor {
        reason: UnknownReasonCode::UnresolvedImport,
        affected_claim: "tsjs_nest_controller_identity",
        kind: "nest_unresolved_controller_import",
        note: "NestJS class decorator is not an exact @nestjs/common import",
    })
}

fn di_subclaim() -> UnknownAnchor {
    UnknownAnchor {
        reason: UnknownReasonCode::RuntimeDependencyInjection,
        affected_claim: "tsjs_nest_di_resolution",
        kind: "tsjs_nest_di_resolution",
        note: "NestJS dependency injection token resolution is a runtime concern",
    }
}

/// Return the exact `@nestjs/common` original name (and decorator args) of the
/// first leading decorator whose resolved import original matches `predicate`.
fn decorator_original<'a>(
    bindings: &'a ScopeGraphLite,
    slice: &'a str,
    predicate: impl Fn(&str) -> bool,
) -> Option<(&'a str, Option<&'a str>)> {
    for (local, args) in parse_decorators(slice) {
        let Some(binding) = bindings.imports.get(local) else {
            continue;
        };
        if binding.module != NEST_COMMON_MODULE || bindings.name_is_unsafe_at(local, 0) {
            continue;
        }
        if let ImportKind::Named(original) = &binding.kind {
            if predicate(original) {
                return Some((original.as_str(), args));
            }
        }
    }
    None
}

/// Parse the leading decorator stack (`@Name(...)`), tolerating multi-line
/// parenthesized arguments. Stops at the first non-decorator token.
fn parse_decorators(slice: &str) -> Vec<(&str, Option<&str>)> {
    let bytes = slice.as_bytes();
    let mut decorators = Vec::new();
    let mut cursor = 0usize;
    loop {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'@' {
            break;
        }
        cursor += 1;
        let Some((name, name_end)) = leading_identifier(&slice[cursor..]) else {
            break;
        };
        cursor += name_end;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let args = if bytes.get(cursor) == Some(&b'(') {
            let Some(close) = matching_paren(slice, cursor) else {
                break;
            };
            let inner = &slice[cursor + 1..close];
            cursor = close + 1;
            Some(inner)
        } else {
            None
        };
        decorators.push((name, args));
    }
    decorators
}

fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    for (offset, &byte) in bytes[open..].iter().enumerate() {
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn arg_path_shape(args: Option<&str>) -> &'static str {
    match args {
        None => "none",
        Some(inner) => {
            let trimmed = inner.trim();
            if trimmed.is_empty() {
                "none"
            } else if matches!(trimmed.as_bytes().first(), Some(b'"' | b'\'' | b'`')) {
                "literal"
            } else {
                "dynamic"
            }
        }
    }
}

fn module_shape(args: Option<&str>) -> String {
    let Some(inner) = args else {
        return "empty".to_string();
    };
    format!(
        "providers_{}_imports_{}_controllers_{}",
        array_member_count(inner, "providers"),
        array_member_count(inner, "imports"),
        array_member_count(inner, "controllers"),
    )
}

fn array_member_count(object: &str, key: &str) -> usize {
    let bytes = object.as_bytes();
    for (offset, _) in object.match_indices(key) {
        let before = offset
            .checked_sub(1)
            .and_then(|index| bytes.get(index))
            .copied();
        if before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            continue;
        }
        let after = object[offset + key.len()..].trim_start();
        let Some(rest) = after.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some(open) = rest.find('[') {
            return bracket_member_count(&rest[open..]);
        }
    }
    0
}

fn bracket_member_count(slice: &str) -> usize {
    let bytes = slice.as_bytes();
    let Some(open) = bytes.iter().position(|byte| matches!(byte, b'{' | b'[')) else {
        return 0;
    };
    let mut depth = 0usize;
    let mut commas = 0usize;
    let mut has_content = false;
    let mut quote: Option<u8> = None;
    for &byte in &bytes[open..] {
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => commas += 1,
            byte if depth == 1 && !byte.is_ascii_whitespace() => has_content = true,
            _ => {}
        }
    }
    if has_content {
        commas + 1
    } else {
        0
    }
}
