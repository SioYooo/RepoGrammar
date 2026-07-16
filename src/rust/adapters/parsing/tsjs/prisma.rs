use super::scope_graph::ScopeGraphLite;
use super::{
    leading_identifier, object_clause_shape, raw_sql_present, Anchor, AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

const PRISMA_OPERATIONS: [&str; 10] = [
    "findMany",
    "findUnique",
    "findFirst",
    "create",
    "update",
    "upsert",
    "delete",
    "count",
    "aggregate",
    "groupBy",
];

pub(super) fn query_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
) -> AnchorOutcome {
    if raw_sql_present(slice) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_raw_query",
            note: "Prisma raw SQL query is not support evidence",
        });
    }
    let Some((client, model, operation)) = prisma_query_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_dynamic_model_or_operation",
            note: "Prisma query model or operation is dynamic",
        });
    };
    if bindings.name_is_unsafe_at(client, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma client is not an exact local PrismaClient binding",
        });
    }
    if !PRISMA_OPERATIONS.contains(&operation) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_query_shape",
            kind: "prisma_dynamic_model_or_operation",
            note: "Prisma operation is not in the exact allowlist",
        });
    }
    let imported_client = bindings.repo_local_named_import(client);
    if !bindings.prisma_clients.contains(client) && imported_client.is_none() {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma client is not an exact local PrismaClient binding",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=prisma_query".to_string(),
        format!("model_name={model}"),
        format!("operation={operation}"),
        format!("where_shape={}", object_clause_shape(slice, "where")),
        format!("select_include_shape={}", select_include_shape(slice)),
        "transaction_shape=none".to_string(),
        format!("raw_sql_present={}", raw_sql_present(slice)),
    ];
    if let Some((specifier, export_name)) = imported_client {
        assumptions.extend(provider_required_prisma_client_assumptions(
            client,
            specifier,
            export_name,
        ));
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("prisma.query.{operation}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

pub(super) fn transaction_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
) -> AnchorOutcome {
    if raw_sql_present(slice) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_transaction_shape",
            kind: "prisma_raw_query",
            note: "Prisma raw SQL transaction is not support evidence",
        });
    }
    let Some(client) = prisma_transaction_client(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_transaction_shape",
            kind: "prisma_transaction_callback",
            note: "Prisma transaction shape is not exact",
        });
    };
    if bindings.name_is_unsafe_at(client, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma transaction client is not an exact local PrismaClient binding",
        });
    }
    if !slice.contains("$transaction([") {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "prisma_transaction_shape",
            kind: "prisma_transaction_callback",
            note: "Prisma callback transaction is not a safe exact anchor",
        });
    }
    let imported_client = bindings.repo_local_named_import(client);
    if !bindings.prisma_clients.contains(client) && imported_client.is_none() {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "prisma_client_binding",
            kind: "prisma_injected_client",
            note: "Prisma transaction client is not an exact local PrismaClient binding",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=prisma_transaction".to_string(),
        "operation=transaction".to_string(),
        "transaction_shape=array".to_string(),
        format!("raw_sql_present={}", raw_sql_present(slice)),
    ];
    if let Some((specifier, export_name)) = imported_client {
        assumptions.extend(provider_required_prisma_client_assumptions(
            client,
            specifier,
            export_name,
        ));
    }
    AnchorOutcome::Anchor(Anchor {
        target: "prisma.transaction".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions,
    })
}

fn provider_required_prisma_client_assumptions(
    local_name: &str,
    specifier: &str,
    export_name: &str,
) -> Vec<String> {
    vec![
        "provider_required=typescript".to_string(),
        "binding_kind=prisma_client".to_string(),
        format!("binding_local_name={local_name}"),
        format!("binding_import_specifier={specifier}"),
        format!("binding_export_name={export_name}"),
        "required_mechanism=typescript_export_graph".to_string(),
    ]
}

fn select_include_shape(slice: &str) -> &'static str {
    match (slice.contains("select:"), slice.contains("include:")) {
        (true, true) => "select_include",
        (true, false) => "select",
        (false, true) => "include",
        (false, false) => "none",
    }
}

fn prisma_query_parts(slice: &str) -> Option<(&str, &str, &str)> {
    let (client, after_client) = leading_identifier(slice)?;
    let rest = slice[after_client..].trim_start().strip_prefix('.')?;
    let (model, after_model) = leading_identifier(rest)?;
    let rest = rest[after_model..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if !rest[after_operation..].trim_start().starts_with('(') {
        return None;
    }
    Some((client, model, operation))
}

fn prisma_transaction_client(slice: &str) -> Option<&str> {
    let (client, after_client) = leading_identifier(slice)?;
    slice[after_client..]
        .trim_start()
        .strip_prefix(".$transaction(")?;
    Some(client)
}
