use super::scope_graph::ScopeGraphLite;
use super::{
    leading_identifier, object_clause_shape, raw_sql_present, Anchor, AnchorOutcome, UnknownAnchor,
};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};

pub(super) const DRIZZLE_TABLE_FACTORIES: [&str; 3] = ["pgTable", "mysqlTable", "sqliteTable"];

pub(super) fn schema_table_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
) -> AnchorOutcome {
    let Some((table_name, factory)) = table_declaration_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_schema_table",
            kind: "drizzle_dynamic_table",
            note: "Drizzle schema table declaration is dynamic",
        });
    };
    if bindings.name_is_unsafe_at(table_name, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "drizzle_table_binding",
            kind: "drizzle_table_unresolved",
            note: "Drizzle table binding is reassigned or redeclared",
        });
    }
    if !bindings.drizzle_table_factories.contains(factory) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_table_binding",
            kind: "drizzle_ambiguous_table_import",
            note: "Drizzle table factory is not an exact Drizzle import",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "drizzle.schema.table".to_string(),
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=drizzle_schema_table".to_string(),
            "operation=schema_table".to_string(),
            format!("table_name={table_name}"),
            "where_shape=none".to_string(),
            "returning_shape=none".to_string(),
            "join_shape=none".to_string(),
            format!("sql_template_present={}", raw_sql_present(slice)),
        ],
    })
}

pub(super) fn query_anchor(
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
) -> AnchorOutcome {
    if raw_sql_present(slice) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_query_shape",
            kind: "drizzle_raw_sql",
            note: "Drizzle raw SQL template is not support evidence",
        });
    }
    let Some((db, operation, table)) = drizzle_query_parts(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_query_shape",
            kind: "drizzle_dynamic_query_builder",
            note: "Drizzle query builder shape is dynamic",
        });
    };
    let db_import = if bindings.drizzle_dbs.contains(db) {
        None
    } else {
        bindings.repo_local_named_import(db)
    };
    if bindings.name_is_unsafe_at(db, start_byte)
        || (!bindings.drizzle_dbs.contains(db) && db_import.is_none())
    {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_db_binding",
            kind: "drizzle_db_binding_unresolved",
            note: "Drizzle db binding is not exact",
        });
    }
    let table_import = if bindings.drizzle_tables.contains(table) {
        None
    } else {
        bindings.repo_local_named_import(table)
    };
    if bindings.name_is_unsafe_at(table, start_byte)
        || (!bindings.drizzle_tables.contains(table) && table_import.is_none())
    {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_table_binding",
            kind: "drizzle_table_unresolved",
            note: "Drizzle query table is not an exact table declaration",
        });
    }
    let mut assumptions = vec![
        "tsjs_anchor_kind=drizzle_query".to_string(),
        format!("operation={operation}"),
        format!("table_name={table}"),
        format!("where_shape={}", object_clause_shape(slice, "where")),
        format!("returning_shape={}", slice.contains(".returning(")),
        format!("join_shape={}", drizzle_join_shape(slice)),
        format!("transaction_shape={}", slice.contains(".transaction(")),
        format!("sql_template_present={}", raw_sql_present(slice)),
    ];
    if db_import.is_some() || table_import.is_some() {
        assumptions.push("provider_required=typescript".to_string());
        assumptions.push("required_mechanism=typescript_export_graph".to_string());
    }
    if let Some((import_specifier, export_name)) = db_import {
        assumptions.extend(provider_required_drizzle_binding_assumptions(
            "db",
            "drizzle_db",
            db,
            import_specifier,
            export_name,
        ));
    }
    if let Some((import_specifier, export_name)) = table_import {
        assumptions.extend(provider_required_drizzle_binding_assumptions(
            "table",
            "drizzle_table",
            table,
            import_specifier,
            export_name,
        ));
    }
    AnchorOutcome::Anchor(Anchor {
        target: format!("drizzle.query.{operation}"),
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
            affected_claim: "drizzle_transaction_shape",
            kind: "drizzle_raw_sql",
            note: "Drizzle raw SQL transaction is not support evidence",
        });
    }
    let Some(db) = drizzle_transaction_db(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "drizzle_transaction_shape",
            kind: "drizzle_dynamic_query_builder",
            note: "Drizzle transaction shape is dynamic",
        });
    };
    if bindings.name_is_unsafe_at(db, start_byte) || !bindings.drizzle_dbs.contains(db) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "drizzle_db_binding",
            kind: "drizzle_db_binding_unresolved",
            note: "Drizzle transaction db binding is not exact",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: "drizzle.transaction".to_string(),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=drizzle_transaction".to_string(),
            "operation=transaction".to_string(),
            "transaction_shape=callback".to_string(),
            format!("sql_template_present={}", raw_sql_present(slice)),
        ],
    })
}

pub(super) fn table_declaration_parts(slice: &str) -> Option<(&str, &str)> {
    let trimmed = strip_export_prefix(slice.trim_start());
    let rest = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let (name, after_name) = leading_identifier(rest)?;
    let rhs = rest[after_name..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let (factory, after_factory) = leading_identifier(rhs)?;
    if !rhs[after_factory..].trim_start().starts_with('(') {
        return None;
    }
    Some((name, factory))
}

fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export ").unwrap_or(line)
}

fn drizzle_query_parts(slice: &str) -> Option<(&str, &str, &str)> {
    let (db, after_db) = leading_identifier(slice)?;
    let rest = slice[after_db..].trim_start().strip_prefix('.')?;
    let (operation, after_operation) = leading_identifier(rest)?;
    if operation == "query" {
        let rest = rest[after_operation..].trim_start().strip_prefix('.')?;
        let (table, after_table) = leading_identifier(rest)?;
        let rest = rest[after_table..].trim_start().strip_prefix('.')?;
        let (query_operation, after_query_operation) = leading_identifier(rest)?;
        if matches!(query_operation, "findMany" | "findFirst")
            && rest[after_query_operation..].trim_start().starts_with('(')
        {
            return Some((
                db,
                if query_operation == "findMany" {
                    "query_findMany"
                } else {
                    "query_findFirst"
                },
                table,
            ));
        }
        return None;
    }
    if !matches!(operation, "select" | "insert" | "update" | "delete")
        || !rest[after_operation..].trim_start().starts_with('(')
    {
        return None;
    }
    if operation == "select" {
        let from_index = slice.find(".from(")?;
        let from_arg = &slice[from_index + ".from(".len()..];
        let (table, _) = leading_identifier(from_arg)?;
        return Some((db, operation, table));
    }
    let arg = &rest[after_operation..].trim_start()["(".len()..];
    let (table, _) = leading_identifier(arg)?;
    Some((db, operation, table))
}

fn drizzle_transaction_db(slice: &str) -> Option<&str> {
    let (db, after_db) = leading_identifier(slice)?;
    slice[after_db..]
        .trim_start()
        .strip_prefix(".transaction(")?;
    Some(db)
}

fn provider_required_drizzle_binding_assumptions(
    binding_id: &str,
    binding_kind: &str,
    local_name: &str,
    import_specifier: &str,
    export_name: &str,
) -> Vec<String> {
    vec![
        format!("binding:{binding_id}:kind={binding_kind}"),
        format!("binding:{binding_id}:local_name={local_name}"),
        format!("binding:{binding_id}:import_specifier={import_specifier}"),
        format!("binding:{binding_id}:export_name={export_name}"),
    ]
}

fn drizzle_join_shape(slice: &str) -> &'static str {
    if slice.contains("Join(") {
        "join"
    } else {
        "none"
    }
}
