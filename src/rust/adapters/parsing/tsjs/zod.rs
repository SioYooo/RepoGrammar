use super::scope_graph::{ImportKind, ScopeGraphLite};
use super::{leading_identifier, Anchor, AnchorOutcome, UnknownAnchor};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const ZOD_MODULES: [&str; 2] = ["zod", "zod/v4"];

pub(super) fn anchor(bindings: &ScopeGraphLite, slice: &str, start_byte: usize) -> AnchorOutcome {
    let Some((head, builder)) = schema_builder_parts(slice) else {
        return AnchorOutcome::None;
    };
    let Some((builder_token, target)) = builder_target(builder) else {
        return AnchorOutcome::None;
    };
    if !head_is_exact_zod_import(bindings, head) || bindings.name_is_unsafe_at(head, start_byte) {
        return AnchorOutcome::None;
    }
    let assumptions = vec![
        "tsjs_anchor_kind=zod_schema".to_string(),
        format!("zod_builder={builder_token}"),
        format!("zod_field_count_shape={}", field_count_shape(slice)),
    ];
    let anchor = Anchor {
        target: target.to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions,
    };
    if has_runtime_refinement(slice) {
        return AnchorOutcome::AnchorWithSubclaims(
            anchor,
            vec![UnknownAnchor {
                reason: UnknownReasonCode::FrameworkMagic,
                affected_claim: "tsjs_zod_runtime_refinement",
                kind: "tsjs_zod_runtime_refinement",
                note: "Zod runtime refinement/transform semantics are not statically resolved",
            }],
        );
    }
    AnchorOutcome::Anchor(anchor)
}

fn head_is_exact_zod_import(bindings: &ScopeGraphLite, head: &str) -> bool {
    match bindings.imports.get(head) {
        Some(binding) if ZOD_MODULES.contains(&binding.module.as_str()) => {
            matches!(binding.kind, ImportKind::Named(_) | ImportKind::Default)
        }
        _ => false,
    }
}

fn schema_builder_parts(slice: &str) -> Option<(&str, &str)> {
    let trimmed = slice.trim_start();
    let trimmed = trimmed
        .strip_prefix("export ")
        .unwrap_or(trimmed)
        .trim_start();
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (_, after_name) = leading_identifier(rest)?;
    let rhs = rest[after_name..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let (head, after_head) = leading_identifier(rhs)?;
    let after_dot = rhs[after_head..].trim_start().strip_prefix('.')?;
    let (builder, after_builder) = leading_identifier(after_dot)?;
    if !after_dot[after_builder..].trim_start().starts_with('(') {
        return None;
    }
    Some((head, builder))
}

fn builder_target(builder: &str) -> Option<(&'static str, &'static str)> {
    match builder {
        "object" => Some(("object", "zod.object")),
        "union" => Some(("union", "zod.union")),
        "discriminatedUnion" => Some(("discriminated_union", "zod.discriminated_union")),
        "enum" => Some(("enum", "zod.enum")),
        "array" => Some(("array", "zod.array")),
        _ => None,
    }
}

fn field_count_shape(slice: &str) -> &'static str {
    match bracket_member_count(slice) {
        0 => "empty",
        1..=3 => "small",
        4..=8 => "medium",
        _ => "large",
    }
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

fn has_runtime_refinement(slice: &str) -> bool {
    slice.contains(".refine(") || slice.contains(".transform(") || slice.contains(".superRefine(")
}
