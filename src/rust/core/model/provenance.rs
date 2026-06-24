//! Source provenance needed to audit every pattern-family conclusion.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("content hash must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryRevision(String);

impl RepositoryRevision {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("repository revision must not be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub path: String,
    pub content_hash: ContentHash,
    pub repository_revision: RepositoryRevision,
}

impl Provenance {
    pub fn new(
        path: impl Into<String>,
        content_hash: ContentHash,
        repository_revision: RepositoryRevision,
    ) -> Result<Self, String> {
        let path = path.into();
        if path.trim().is_empty() {
            Err("provenance path must not be empty".to_string())
        } else {
            Ok(Self {
                path,
                content_hash,
                repository_revision,
            })
        }
    }
}
