pub(crate) const ROLE_QUERY: &str = "framework:prisma.query";
pub(crate) const ROLE_TRANSACTION: &str = "framework:prisma.transaction";

pub(crate) const QUERY_TARGETS: &[&str] = &[
    "prisma.query.findMany",
    "prisma.query.findUnique",
    "prisma.query.findFirst",
    "prisma.query.create",
    "prisma.query.update",
    "prisma.query.upsert",
    "prisma.query.delete",
    "prisma.query.count",
    "prisma.query.aggregate",
    "prisma.query.groupBy",
];

pub(crate) const TARGET_TRANSACTION: &str = "prisma.transaction";
