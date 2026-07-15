//! Contract types for future Python semantic providers such as Pyrefly and Pyright.

use crate::core::model::{
    CodeUnitId, ContentHash, SemanticFact, SourceRange, TypedUnknown, UnknownClass,
    UnknownReasonCode,
};
use crate::core::policy::paths::validate_repo_relative_path;

const MAX_PROVIDER_METADATA_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PythonProviderKind {
    Pyrefly,
    Pyright,
    RightTyper,
}

impl PythonProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pyrefly => "pyrefly",
            Self::Pyright => "pyright",
            Self::RightTyper => "righttyper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PythonProviderOperation {
    ResolveFrameworkIdentity,
    CrossCheckClaim,
    CallHierarchy,
    ObserveRuntimeTypes,
}

impl PythonProviderOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResolveFrameworkIdentity => "resolve_framework_identity",
            Self::CrossCheckClaim => "cross_check_claim",
            Self::CallHierarchy => "call_hierarchy",
            Self::ObserveRuntimeTypes => "observe_runtime_types",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderCandidate {
    pub code_unit_id: CodeUnitId,
    pub path: String,
    pub content_hash: ContentHash,
    pub range: SourceRange,
}

impl PythonProviderCandidate {
    pub fn new(
        code_unit_id: CodeUnitId,
        path: impl Into<String>,
        content_hash: ContentHash,
        range: SourceRange,
    ) -> Result<Self, String> {
        let path = path.into();
        validate_repo_relative_path(&path)
            .map_err(|_| "python provider candidate path must be repo-relative".to_string())?;
        Ok(Self {
            code_unit_id,
            path,
            content_hash,
            range,
        })
    }

    fn sort_key(&self) -> (&str, usize, usize, &str) {
        (
            &self.path,
            self.range.start_byte,
            self.range.end_byte,
            self.code_unit_id.as_str(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderRequest {
    pub provider: PythonProviderKind,
    pub operation: PythonProviderOperation,
    pub candidates: Vec<PythonProviderCandidate>,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
}

impl PythonProviderRequest {
    pub fn new(
        provider: PythonProviderKind,
        operation: PythonProviderOperation,
        candidates: impl IntoIterator<Item = PythonProviderCandidate>,
        python_version: impl Into<String>,
        provider_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
    ) -> Result<Self, String> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err("python provider request must include at least one candidate".to_string());
        }
        candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        for window in candidates.windows(2) {
            if window[0].sort_key() == window[1].sort_key() {
                return Err("python provider request candidates must be unique".to_string());
            }
        }
        Ok(Self {
            provider,
            operation,
            candidates,
            python_version: validate_provider_metadata("python version", python_version)?,
            provider_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderProvenance {
    pub provider: PythonProviderKind,
    pub provider_version: String,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: PythonProviderOperation,
}

impl PythonProviderProvenance {
    pub fn new(
        provider: PythonProviderKind,
        provider_version: impl Into<String>,
        python_version: impl Into<String>,
        provider_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        operation: PythonProviderOperation,
    ) -> Result<Self, String> {
        Ok(Self {
            provider,
            provider_version: validate_provider_metadata("provider version", provider_version)?,
            python_version: validate_provider_metadata("python version", python_version)?,
            provider_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            operation,
        })
    }

    pub fn assumptions(&self) -> Vec<String> {
        vec![
            "provider_resolved=true".to_string(),
            format!("provider={}", self.provider.as_str()),
            format!("provider_version={}", self.provider_version),
            format!("python_version={}", self.python_version),
            format!(
                "provider_config_hash={}",
                self.provider_config_hash.as_str()
            ),
            format!("environment_fingerprint={}", self.environment_fingerprint),
            format!("query_operation={}", self.operation.as_str()),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderCacheKey {
    pub provider: PythonProviderKind,
    pub provider_version: String,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: PythonProviderOperation,
    pub candidates: Vec<PythonProviderCandidate>,
}

impl PythonProviderCacheKey {
    pub fn new(
        provenance: PythonProviderProvenance,
        candidates: impl IntoIterator<Item = PythonProviderCandidate>,
    ) -> Result<Self, String> {
        let request = PythonProviderRequest::new(
            provenance.provider,
            provenance.operation,
            candidates,
            provenance.python_version.clone(),
            provenance.provider_config_hash.clone(),
            provenance.environment_fingerprint.clone(),
        )?;
        Ok(Self {
            provider: provenance.provider,
            provider_version: provenance.provider_version,
            python_version: request.python_version,
            provider_config_hash: request.provider_config_hash,
            environment_fingerprint: request.environment_fingerprint,
            operation: provenance.operation,
            candidates: request.candidates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderOutput {
    pub facts: Vec<SemanticFact>,
    pub unknowns: Vec<TypedUnknown>,
    pub provenance: Option<PythonProviderProvenance>,
}

impl PythonProviderOutput {
    pub fn facts(
        provenance: PythonProviderProvenance,
        facts: Vec<SemanticFact>,
        unknowns: Vec<TypedUnknown>,
    ) -> Self {
        Self {
            facts,
            unknowns,
            provenance: Some(provenance),
        }
    }

    pub fn unavailable(provider: PythonProviderKind, operation: PythonProviderOperation) -> Self {
        Self {
            facts: Vec::new(),
            unknowns: vec![TypedUnknown::new(
                UnknownClass::Recoverable,
                UnknownReasonCode::MissingDependency,
                format!(
                    "python_provider:{}:{}",
                    provider.as_str(),
                    operation.as_str()
                ),
                Some(format!(
                    "install or configure {} provider",
                    provider.as_str()
                )),
            )
            .expect("provider unavailable UNKNOWN uses non-empty fields")],
            provenance: None,
        }
    }
}

fn validate_provider_metadata(
    field: &'static str,
    value: impl Into<String>,
) -> Result<String, String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > MAX_PROVIDER_METADATA_CHARS || value.chars().any(char::is_control) {
        return Err(format!("{field} must be sanitized metadata"));
    }
    if value.contains('/') || value.contains('\\') || value.contains("://") {
        return Err(format!("{field} must not contain path-like text"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(character: char) -> ContentHash {
        ContentHash::new(format!("sha256:{}", character.to_string().repeat(64)))
            .expect("valid content hash")
    }

    fn candidate(path: &str, start: usize) -> PythonProviderCandidate {
        PythonProviderCandidate::new(
            CodeUnitId::new(format!("unit:{path}:{start}")).expect("valid code unit id"),
            path,
            hash('a'),
            SourceRange::new(start, start + 10).expect("valid range"),
        )
        .expect("valid provider candidate")
    }

    #[test]
    fn provider_request_validates_and_sorts_candidate_scope() {
        let request = PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
            [candidate("src/b.py", 10), candidate("src/a.py", 0)],
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
        )
        .expect("valid provider request");

        assert_eq!(request.candidates[0].path, "src/a.py");
        assert_eq!(request.candidates[1].path, "src/b.py");
        assert_eq!(request.provider.as_str(), "pyrefly");
        assert_eq!(request.operation.as_str(), "resolve_framework_identity");
    }

    #[test]
    fn provider_request_rejects_unsafe_or_duplicate_candidate_scope() {
        assert!(PythonProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "../secret.py",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
        )
        .is_err());

        let duplicate = candidate("src/a.py", 0);
        assert!(PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
            [duplicate.clone(), duplicate],
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
        )
        .is_err());
    }

    #[test]
    fn provider_provenance_and_cache_key_record_required_dimensions() {
        let provenance = PythonProviderProvenance::new(
            PythonProviderKind::Pyright,
            "1.1.400",
            "3.12.6",
            hash('c'),
            "env-sha256-def",
            PythonProviderOperation::CrossCheckClaim,
        )
        .expect("valid provenance");

        let assumptions = provenance.assumptions();
        assert!(assumptions.contains(&"provider_resolved=true".to_string()));
        assert!(assumptions.contains(&"provider=pyright".to_string()));
        assert!(assumptions.contains(&format!("provider_config_hash={}", hash('c').as_str())));
        assert!(!assumptions.iter().any(|value| value.contains("src/a.py")));

        let cache_key =
            PythonProviderCacheKey::new(provenance, [candidate("src/a.py", 8)]).expect("cache key");
        assert_eq!(cache_key.provider, PythonProviderKind::Pyright);
        assert_eq!(cache_key.provider_version, "1.1.400");
        assert_eq!(cache_key.candidates[0].range.start_byte, 8);
    }

    #[test]
    fn provider_metadata_rejects_blank_path_like_and_control_text() {
        for value in [
            "",
            "   ",
            "/tmp/pyrefly",
            "file://env",
            "env\\fingerprint",
            "env\nx",
        ] {
            assert!(
                PythonProviderProvenance::new(
                    PythonProviderKind::Pyrefly,
                    value,
                    "3.12.6",
                    hash('c'),
                    "env-sha256-def",
                    PythonProviderOperation::ResolveFrameworkIdentity,
                )
                .is_err(),
                "provider version should reject {value:?}"
            );
        }
    }

    #[test]
    fn unavailable_provider_becomes_recoverable_unknown_without_facts() {
        let output = PythonProviderOutput::unavailable(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
        );

        assert!(output.facts.is_empty());
        assert!(output.provenance.is_none());
        assert_eq!(output.unknowns.len(), 1);
        assert_eq!(output.unknowns[0].class, UnknownClass::Recoverable);
        assert_eq!(
            output.unknowns[0].reason,
            UnknownReasonCode::MissingDependency
        );
        assert_eq!(
            output.unknowns[0].affected_claim,
            "python_provider:pyrefly:resolve_framework_identity"
        );
    }
}
