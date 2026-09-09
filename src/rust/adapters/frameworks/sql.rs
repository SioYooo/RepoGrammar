//! Language-internal SQL role registry for the ADR-0040 admitted subset.
//!
//! ADR-0020 gate 5 lets a language substitute a recurring language-internal
//! pattern for a framework family, and names SQL statement shapes as eligible.
//! There is no SQL "framework" to detect here: the role marks a statement shape
//! the bounded frontend proved, and the shape is the whole claim. No dialect,
//! catalog, execution order, or table identity is asserted by it.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_SQL_TABLE_DEFINITION: &str = "framework:sql.table_definition";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SqlRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<SqlRole> {
    match kind {
        CodeUnitKind::SqlTableDefinition => Some(SqlRole {
            target: ROLE_SQL_TABLE_DEFINITION,
            note: "bounded SQL code unit is a CREATE TABLE definition list",
            assumption:
                "dialect, catalog state, execution order, and table identity are unresolved",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:sql.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_SQL_TABLE_DEFINITION => {
            Some(target == crate::adapters::parsing::sql::SQL_CREATE_TABLE_TARGET)
        }
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_SQL_TABLE_DEFINITION => "sql.schema.table_definition".to_string(),
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_table_definition_kind_carries_the_sql_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::SqlTableDefinition)
                .expect("table definition role")
                .target,
            ROLE_SQL_TABLE_DEFINITION
        );
        // A statement the frontend did not admit is inventory, so it must not
        // reach a role and therefore can never join the family.
        assert!(role_for_code_unit_kind(&CodeUnitKind::SqlStatement).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }

    #[test]
    fn only_the_fixed_anchor_target_supports_the_role() {
        assert_eq!(
            support_target_is_role_compatible("sql.ddl.create_table", ROLE_SQL_TABLE_DEFINITION),
            Some(true)
        );
        assert_eq!(
            support_target_is_role_compatible("sql.ddl.create_index", ROLE_SQL_TABLE_DEFINITION),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("sql.ddl.create_table", "framework:sql.unknown_role"),
            Some(false)
        );
        assert_eq!(
            support_target_is_role_compatible("sql.ddl.create_table", "framework:fastapi.route"),
            None
        );
        assert_eq!(
            support_family("sql.ddl.create_table", ROLE_SQL_TABLE_DEFINITION),
            "sql.schema.table_definition"
        );
    }
}
