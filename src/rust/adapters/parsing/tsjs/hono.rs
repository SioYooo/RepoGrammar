use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, call_arguments, handler_shape, route_call_parts,
    route_handler_binding_assumptions, string_literal_arg, Anchor, AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const HONO_HTTP_METHODS: [&str; 5] = ["get", "post", "put", "delete", "patch"];

pub(super) fn anchor(bindings: &ScopeGraphLite, slice: &str, start_byte: usize) -> AnchorOutcome {
    let Some((receiver, method)) = route_call_parts(slice) else {
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
    let path_is_literal = call_arguments(slice)
        .as_ref()
        .and_then(|arguments| arguments.first())
        .is_some_and(|argument| string_literal_arg(argument));
    if !path_is_literal {
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
    match route_handler_binding_assumptions(bindings, slice, start_byte) {
        Ok(extra) => assumptions.extend(extra),
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("hono.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}
