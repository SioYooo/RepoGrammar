//! Source provenance needed to audit every pattern-family conclusion.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let hash = value
            .strip_prefix("sha256:")
            .ok_or_else(|| "content hash must match sha256:<64 hex chars>".to_string())?;
        if hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
            Err("content hash must match sha256:<64 hex chars>".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA256: &str =
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const VALID_UPPERCASE_SHA256: &str =
        "sha256:0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";

    #[test]
    fn accepts_sha256_content_hashes_with_sixty_four_hex_chars() {
        let hash = ContentHash::new(VALID_SHA256).expect("valid sha256 content hash");

        assert_eq!(hash.as_str(), VALID_SHA256);

        let hash =
            ContentHash::new(VALID_UPPERCASE_SHA256).expect("valid uppercase sha256 content hash");
        assert_eq!(hash.as_str(), VALID_UPPERCASE_SHA256);
    }

    #[test]
    fn rejects_invalid_content_hashes() {
        for value in [
            "",
            "   ",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha256:0123456789abcdef",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg",
            "sha256:test",
        ] {
            assert!(
                ContentHash::new(value).is_err(),
                "expected invalid content hash: {value}"
            );
        }
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
