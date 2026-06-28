pub(crate) const ROLE_SCHEMA_TABLE: &str = "framework:drizzle.schema.table";
pub(crate) const ROLE_QUERY: &str = "framework:drizzle.query";
pub(crate) const ROLE_TRANSACTION: &str = "framework:drizzle.transaction";

pub(crate) const TARGET_SCHEMA_TABLE: &str = "drizzle.schema.table";
pub(crate) const TARGET_TRANSACTION: &str = "drizzle.transaction";

pub(crate) const QUERY_TARGETS: &[&str] = &[
    "drizzle.query.select",
    "drizzle.query.insert",
    "drizzle.query.update",
    "drizzle.query.delete",
];
