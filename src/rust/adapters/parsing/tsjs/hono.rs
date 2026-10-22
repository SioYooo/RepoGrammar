use super::parts::CallParts;
use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, handler_shape, route_handler_binding_assumptions, Anchor, AnchorOutcome,
    UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const HONO_HTTP_METHODS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

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
    if !HONO_HTTP_METHODS.contains(&method) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "tsjs_support_target",
            kind: "unsupported_route_method",
            note: "TS/JS Hono route method is not in the exact anchor allowlist",
        });
    }
    if !bindings.hono_receivers.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "tsjs_hono_receiver",
            kind: "tsjs_hono_receiver",
            note: "TS/JS route receiver is not an exact Hono app binding",
        });
    }
    if bindings.name_is_unsafe_at(receiver, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_hono_receiver",
            kind: "tsjs_hono_receiver",
            note: "TS/JS Hono route receiver is reassigned or redeclared",
        });
    }
    if !call.is_some_and(|call| call.first_is_string) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_support_target",
            kind: "dynamic_route_call",
            note: "TS/JS Hono route path is not a literal string",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=hono_route".to_string(),
        format!("http_method={method}"),
        "route_path_shape=literal".to_string(),
        format!("handler_shape={}", handler_shape(slice)),
        format!("async_shape={}", async_shape(slice)),
    ];
    match route_handler_binding_assumptions(
        bindings,
        call.and_then(|call| call.handler),
        start_byte,
    ) {
        Ok(extra) => assumptions.extend(extra),
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("hono.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}
