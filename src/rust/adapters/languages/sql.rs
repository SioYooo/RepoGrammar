//! Source-free SQL path classification.
//!
//! The classifier uses normalized repository-relative path shape only. It does
//! not read SQL text, select a dialect, connect to a database, or execute a
//! migration. Artifact roles are conservative inventory labels, not semantic
//! proof that a file is executable or valid for any database.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlPathClassification {
    NotSql,
    Generic,
    Migration,
    Schema,
    Catalog,
}

impl SqlLanguageAdapter {
    pub fn classify_path(path: &str) -> SqlPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return SqlPathClassification::NotSql;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return SqlPathClassification::NotSql;
        };
        if !file_name.ends_with(".sql") {
            return SqlPathClassification::NotSql;
        }
        if path
            .split('/')
            .any(|component| matches!(component, "migration" | "migrations"))
        {
            return SqlPathClassification::Migration;
        }
        match file_name {
            "schema.sql" => SqlPathClassification::Schema,
            "catalog.sql" => SqlPathClassification::Catalog,
            _ => SqlPathClassification::Generic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_sql_inventory_without_inspecting_source() {
        for (path, expected) in [
            ("query.sql", SqlPathClassification::Generic),
            (".sql", SqlPathClassification::Generic),
            ("schema.sql", SqlPathClassification::Schema),
            ("db/schema.sql", SqlPathClassification::Schema),
            ("catalog.sql", SqlPathClassification::Catalog),
            ("db/catalog.sql", SqlPathClassification::Catalog),
            ("migrations/001_init.sql", SqlPathClassification::Migration),
            ("db/migration/schema.sql", SqlPathClassification::Migration),
        ] {
            assert_eq!(SqlLanguageAdapter::classify_path(path), expected, "{path}");
        }
    }

    #[test]
    fn rejects_case_variants_suffixes_and_non_normalized_paths() {
        for path in [
            "query.SQL",
            "query.sql.bak",
            "migrations/001_init.psql",
            "",
            "/query.sql",
            "./query.sql",
            "db/../query.sql",
            "db//query.sql",
            "db\\query.sql",
            "file://query.sql",
        ] {
            assert_eq!(
                SqlLanguageAdapter::classify_path(path),
                SqlPathClassification::NotSql,
                "{path:?}"
            );
        }
        for path in ["Schema.sql", "Catalog.sql"] {
            assert_eq!(
                SqlLanguageAdapter::classify_path(path),
                SqlPathClassification::Generic,
                "case variants remain generic SQL rather than semantic artifacts: {path}"
            );
        }
    }

    #[test]
    fn artifact_names_do_not_select_a_sql_dialect() {
        assert_eq!(
            SqlLanguageAdapter::classify_path("postgres/migrations/001.sql"),
            SqlPathClassification::Migration
        );
        assert_eq!(
            SqlLanguageAdapter::classify_path("sqlite/migrations/001.sql"),
            SqlPathClassification::Migration
        );
    }
}
