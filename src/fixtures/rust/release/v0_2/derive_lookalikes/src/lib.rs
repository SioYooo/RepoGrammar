// No `use serde` anywhere: the derive tokens are lookalikes without use-path
// evidence, so no serde family may form and each unit records a blocking
// UnresolvedImport/rust_framework_attribute_binding UNKNOWN.

#[derive(Serialize, Deserialize)]
pub struct LookalikeItem {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize)]
pub struct LookalikePage {
    pub id: u64,
}

#[derive(Deserialize)]
pub struct LookalikeFilter {
    pub id: u64,
}
