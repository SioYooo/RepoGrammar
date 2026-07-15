//! Source storage port for transient repository source reads during indexing.

use crate::core::model::ContentHash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReadRequest {
    pub repository_root: String,
    pub path: String,
    pub expected_content_hash: ContentHash,
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    pub path: String,
    pub content_hash: ContentHash,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceStoreError {
    InvalidRequest(String),
    Missing(String),
    HashMismatch(String),
    TooLarge(String),
    NonUtf8(String),
    Unavailable(String),
}

pub trait SourceStore {
    fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError>;
}
