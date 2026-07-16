pub(crate) const ROLE_SCHEMA: &str = "framework:zod.schema";

pub(crate) const SCHEMA_TARGETS: &[&str] = &[
    "zod.object",
    "zod.union",
    "zod.discriminated_union",
    "zod.enum",
    "zod.array",
];
