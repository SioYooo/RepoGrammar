use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, handler_shape, normalize_route_path, object_literal_has_field,
    object_literal_string_field, route_call_parts, route_handler_binding_assumptions,
    route_path_shape, Anchor, AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const FASTIFY_HTTP_METHODS: [&str; 8] = [
    "get", "head", "post", "put", "delete", "options", "patch", "all",
];

pub(super) fn anchor(bindings: &ScopeGraphLite, slice: &str, start_byte: usize) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_route_shape",
            kind: "fastify_dynamic_route_call",
            note: "Fastify route call shape is dynamic",
        });
    };
    if !bindings.fastify_receivers.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_unresolved",
            note: "Fastify route receiver is not an exact Fastify binding",
        });
    }
    if bindings.name_is_unsafe_at(receiver, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_reassigned",
            note: "Fastify receiver is reassigned or redeclared",
        });
    }
    if method == "route" {
        return fastify_full_route_anchor(slice);
    }
    if !FASTIFY_HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify route method is not in the exact allowlist",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=fastify_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
        format!("schema_present={}", slice.contains("schema")),
        format!("opts_handler_present={}", slice.contains("handler")),
        format!("reply_shape={}", reply_shape(slice)),
        "plugin_context=none".to_string(),
        "prefix_unknown=false".to_string(),
    ];
    if let Some(path_shape) = route_path_shape(slice) {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    match route_handler_binding_assumptions(bindings, slice, start_byte) {
        Ok(extra) => assumptions.extend(extra),
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("fastify.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn fastify_full_route_anchor(slice: &str) -> AnchorOutcome {
    let Some(method) = object_literal_string_field(slice, "method") else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify full route method is not a literal string",
        });
    };
    let method = method.to_ascii_lowercase();
    if !FASTIFY_HTTP_METHODS.contains(&method.as_str()) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_route_method",
            kind: "fastify_dynamic_method",
            note: "Fastify full route method is not in the exact allowlist",
        });
    }
    let Some(path_shape) = object_literal_string_field(slice, "url")
        .or_else(|| object_literal_string_field(slice, "path"))
        .map(|path| normalize_route_path(&path))
    else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_route_shape",
            kind: "fastify_missing_literal_path",
            note: "Fastify full route path/url is not a literal string",
        });
    };
    if !object_literal_has_field(slice, "handler") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_route_shape",
            kind: "fastify_missing_handler",
            note: "Fastify full route handler is not an exact object-literal field",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=fastify_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
        format!("schema_present={}", slice.contains("schema")),
        "opts_handler_present=true".to_string(),
        format!("reply_shape={}", reply_shape(slice)),
        "plugin_context=none".to_string(),
        "prefix_unknown=false".to_string(),
    ];
    assumptions.push(format!("route_path_shape={path_shape}"));
    AnchorOutcome::Anchor(Anchor {
        target: "fastify.route.route".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn reply_shape(slice: &str) -> &'static str {
    if slice.contains(".send(") {
        "reply_send"
    } else if slice.contains(".code(") || slice.contains(".status(") {
        "reply_status"
    } else {
        "reply_unknown"
    }
}
