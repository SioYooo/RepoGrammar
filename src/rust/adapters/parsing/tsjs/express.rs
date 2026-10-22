use super::parts::CallParts;
use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, handler_shape, normalize_route_path, route_handler_binding_assumptions, Anchor,
    AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const EXPRESS_HTTP_METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "use"];

pub(super) fn anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
    call: Option<&CallParts<'_>>,
) -> AnchorOutcome {
    let Some((receiver, method)) = call.map(|call| (call.receiver, call.method)) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_support_target",
            kind: "dynamic_route_call",
            note: "TS/JS route call shape is dynamic",
        });
    };
    if !EXPRESS_HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "tsjs_support_target",
            kind: "unsupported_route_method",
            note: "TS/JS route method is not in the exact anchor allowlist",
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
    if bindings.name_is_unsafe_at(receiver, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_receiver_binding",
            kind: "unsafe_receiver_binding",
            note: "TS/JS route receiver is reassigned or redeclared",
        });
    }
    let path_shape = call
        .and_then(|call| call.path_literal)
        .map(normalize_route_path);
    let mut assumptions = vec![
        "tsjs_anchor_kind=express_route".to_string(),
        format!("route_method={method}"),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
    ];
    if let Some(path_shape) = &path_shape {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    if let Some(prefix_shape) = bindings.express_router_prefixes.get(receiver) {
        assumptions.push(format!("route_prefix_shape={prefix_shape}"));
        if let Some(path_shape) = &path_shape {
            assumptions.push(format!(
                "effective_route_path_shape={}",
                effective_route_path_shape(prefix_shape, path_shape)
            ));
        }
    } else {
        assumptions.push("route_prefix_shape=none".to_string());
    }
    if method != "use" {
        match route_handler_binding_assumptions(
            bindings,
            call.and_then(|call| call.handler),
            start_byte,
        ) {
            Ok(extra) => assumptions.extend(extra),
            Err(unknown) => return AnchorOutcome::Unknown(unknown),
        }
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("express.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn effective_route_path_shape(prefix_shape: &str, path_shape: &str) -> String {
    if prefix_shape == "/" {
        return path_shape.to_string();
    }
    if path_shape == "/" {
        return prefix_shape.to_string();
    }
    format!(
        "{}/{}",
        prefix_shape.trim_end_matches('/'),
        path_shape.trim_start_matches('/')
    )
}
