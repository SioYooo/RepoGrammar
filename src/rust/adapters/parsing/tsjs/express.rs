use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, handler_shape, route_call_parts, route_path_shape, Anchor, AnchorOutcome,
    UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const EXPRESS_HTTP_METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "use"];

pub(super) fn anchor(bindings: &ScopeGraphLite, slice: &str, start_byte: usize) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
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
