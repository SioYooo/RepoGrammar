use super::parts::{CallParts, Prefix};
use super::scope_graph::ScopeGraphLite;
use super::{
    async_shape, handler_shape, normalize_route_path, route_handler_binding_assumptions, Anchor,
    AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const FASTIFY_HTTP_METHODS: [&str; 8] = [
    "get", "head", "post", "put", "delete", "options", "patch", "all",
];

pub(super) fn anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
    call: Option<&CallParts<'_>>,
) -> AnchorOutcome {
    let Some((receiver, method)) = call.map(|call| (call.receiver, call.method)) else {
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
        return fastify_full_route_anchor(slice, call);
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
    if let Some(path_shape) = call
        .and_then(|call| call.path_literal)
        .map(normalize_route_path)
    {
        assumptions.push(format!("route_path_shape={path_shape}"));
    }
    match route_handler_binding_assumptions(
        bindings,
        call.and_then(|call| call.handler),
        start_byte,
    ) {
        Ok(extra) => assumptions.extend(extra),
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("fastify.route.{method}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

pub(super) fn register_anchor(
    bindings: &ScopeGraphLite,
    _slice: &str,
    start_byte: usize,
    call: Option<&CallParts<'_>>,
) -> AnchorOutcome {
    let Some((receiver, method)) = call.map(|call| (call.receiver, call.method)) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_plugin_registration",
            kind: "fastify_dynamic_register_call",
            note: "Fastify plugin registration call shape is dynamic",
        });
    };
    if method != "register" {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "fastify_plugin_registration",
            kind: "fastify_dynamic_register_call",
            note: "Fastify plugin registration method is not an exact register call",
        });
    }
    if !bindings.fastify_receivers.contains(receiver) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_unresolved",
            note: "Fastify plugin registration receiver is not an exact Fastify binding",
        });
    }
    if bindings.name_is_unsafe_at(receiver, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "fastify_receiver_binding",
            kind: "fastify_receiver_reassigned",
            note: "Fastify plugin registration receiver is reassigned or redeclared",
        });
    }

    let mut assumptions = vec![
        "tsjs_anchor_kind=fastify_plugin_register".to_string(),
        "operation=register".to_string(),
        "fact_scope=context_only".to_string(),
        "prefix_unknown=false".to_string(),
        "plugin_effects=unresolved".to_string(),
    ];
    match fastify_plugin_binding_assumptions(
        bindings,
        call.and_then(|call| call.plugin),
        start_byte,
    ) {
        Ok(extra) => assumptions.extend(extra),
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    }
    let prefix_shape = match call.map_or(
        Err(dynamic_prefix(
            "Fastify plugin registration arguments could not be parsed",
        )),
        fastify_register_prefix_shape,
    ) {
        Ok(prefix_shape) => prefix_shape,
        Err(unknown) => return AnchorOutcome::Unknown(unknown),
    };
    assumptions.push(format!("route_prefix_shape={prefix_shape}"));
    AnchorOutcome::Anchor(Anchor {
        target: "fastify.plugin.register".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn fastify_full_route_anchor(slice: &str, call: Option<&CallParts<'_>>) -> AnchorOutcome {
    let object = call
        .map(|call| call.route_object.clone())
        .unwrap_or_default();
    let Some(method) = object.method else {
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
    let Some(path_shape) = object.path.map(normalize_route_path) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_route_shape",
            kind: "fastify_missing_literal_path",
            note: "Fastify full route path/url is not a literal string",
        });
    };
    if !object.has_handler {
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

fn fastify_plugin_binding_assumptions(
    bindings: &ScopeGraphLite,
    plugin: Option<&str>,
    start_byte: usize,
) -> Result<Vec<String>, UnknownAnchor> {
    let Some(plugin) = plugin else {
        return Err(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_plugin_binding",
            kind: "fastify_dynamic_plugin",
            note: "Fastify plugin registration plugin argument is not an exact identifier",
        });
    };
    if bindings.name_is_unsafe_at(plugin, start_byte) {
        return Err(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "fastify_plugin_binding",
            kind: "unsafe_fastify_plugin_binding",
            note:
                "Fastify plugin registration plugin binding is reassigned, redeclared, or shadowed",
        });
    }
    if bindings.imports.contains_key(plugin) {
        return Err(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "fastify_plugin_binding",
            kind: "imported_fastify_plugin",
            note: "Fastify plugin registration plugin is imported and not resolved by this structural context pass",
        });
    }
    if bindings.local_decls.contains(plugin) {
        return Ok(vec![
            "plugin_binding=local".to_string(),
            format!("plugin_local_name={plugin}"),
        ]);
    }
    Err(UnknownAnchor {
        reason: UnknownReasonCode::UnresolvedImport,
        affected_claim: "fastify_plugin_binding",
        kind: "missing_fastify_plugin",
        note: "Fastify plugin registration plugin identifier is not declared or imported",
    })
}

fn dynamic_prefix(note: &'static str) -> UnknownAnchor {
    UnknownAnchor {
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: "fastify_route_prefix",
        kind: "fastify_dynamic_prefix",
        note,
    }
}

fn fastify_register_prefix_shape(call: &CallParts<'_>) -> Result<String, UnknownAnchor> {
    match call.arity {
        None => Err(dynamic_prefix(
            "Fastify plugin registration arguments could not be parsed",
        )),
        Some(1) => Ok("none".to_string()),
        Some(2) => match call.prefix {
            Prefix::Literal(prefix) => Ok(normalize_route_path(prefix)),
            Prefix::Absent => Ok("none".to_string()),
            Prefix::NotObject => Err(UnknownAnchor {
                reason: UnknownReasonCode::BuildVariantAmbiguity,
                affected_claim: "fastify_route_prefix",
                kind: "fastify_dynamic_prefix",
                note: "Fastify plugin registration options are not an exact object literal",
            }),
            Prefix::Dynamic => Err(UnknownAnchor {
                reason: UnknownReasonCode::BuildVariantAmbiguity,
                affected_claim: "fastify_route_prefix",
                kind: "fastify_dynamic_prefix",
                note: "Fastify plugin registration prefix is not a literal string",
            }),
        },
        Some(_) => Err(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "fastify_plugin_registration",
            kind: "fastify_dynamic_register_call",
            note: "Fastify plugin registration has a non-exact argument shape",
        }),
    }
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
