use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct CatalogPage {
    pub id: u64,
    pub items: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct CatalogFilter {
    pub id: u64,
    pub label: String,
}
