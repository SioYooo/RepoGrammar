//! Source storage port for retrieving auditable evidence.

use crate::core::model::Provenance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStoreError {
    Missing(String),
    Unavailable(String),
}

pub trait SourceStore {
    fn read_source(&self, provenance: &Provenance) -> Result<String, SourceStoreError>;
}
