//! Contract types for future Rust semantic providers.
//!
//! These types define the owned boundary between Cargo/rustc/rust-analyzer
//! analysis and RepoGrammar facts. Provider implementations must translate
//! external objects into `SemanticFact`, `Evidence`, `Provenance`, and typed
//! `UNKNOWN` values before crossing this port.

use crate::core::model::{
    CodeUnitId, ContentHash, SemanticFact, SourceRange, TypedUnknown, UnknownClass,
    UnknownReasonCode,
};
use crate::core::policy::paths::validate_repo_relative_path;

const MAX_PROVIDER_METADATA_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RustProviderKind {
    CargoMetadata,
    RustAnalyzer,
    Rustc,
    RustdocJson,
}

impl RustProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CargoMetadata => "cargo_metadata",
            Self::RustAnalyzer => "rust_analyzer",
            Self::Rustc => "rustc",
            Self::RustdocJson => "rustdoc_json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RustProviderOperation {
    CargoProjectModel,
    ResolveSymbolsAndTypes,
    ResolveTraitDispatch,
    ResolveCallAndEffectFacts,
    CrossCheckClaim,
}

impl RustProviderOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CargoProjectModel => "cargo_project_model",
            Self::ResolveSymbolsAndTypes => "resolve_symbols_and_types",
            Self::ResolveTraitDispatch => "resolve_trait_dispatch",
            Self::ResolveCallAndEffectFacts => "resolve_call_and_effect_facts",
            Self::CrossCheckClaim => "cross_check_claim",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProviderCandidate {
    pub code_unit_id: CodeUnitId,
    pub path: String,
    pub content_hash: ContentHash,
    pub range: SourceRange,
    pub manifest_path: Option<String>,
    pub crate_root_path: Option<String>,
}

impl RustProviderCandidate {
    pub fn new(
        code_unit_id: CodeUnitId,
        path: impl Into<String>,
        content_hash: ContentHash,
        range: SourceRange,
        manifest_path: Option<String>,
        crate_root_path: Option<String>,
    ) -> Result<Self, String> {
        let path = path.into();
        validate_repo_relative_path(&path)
            .map_err(|_| "rust provider candidate path must be repo-relative".to_string())?;
        Ok(Self {
            code_unit_id,
            path,
            content_hash,
            range,
            manifest_path: validate_optional_repo_relative_path("manifest path", manifest_path)?,
            crate_root_path: validate_optional_repo_relative_path(
                "crate root path",
                crate_root_path,
            )?,
        })
    }

    fn sort_key(&self) -> (&str, usize, usize, &str, Option<&str>, Option<&str>) {
        (
            &self.path,
            self.range.start_byte,
            self.range.end_byte,
            self.code_unit_id.as_str(),
            self.manifest_path.as_deref(),
            self.crate_root_path.as_deref(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProviderRequest {
    pub provider: RustProviderKind,
    pub operation: RustProviderOperation,
    pub candidates: Vec<RustProviderCandidate>,
    pub rust_toolchain: String,
    pub cargo_metadata_hash: ContentHash,
    pub cfg_profile_hash: ContentHash,
    pub environment_fingerprint: String,
    pub build_scripts_executed: bool,
    pub proc_macros_executed: bool,
}

impl RustProviderRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: RustProviderKind,
        operation: RustProviderOperation,
        candidates: impl IntoIterator<Item = RustProviderCandidate>,
        rust_toolchain: impl Into<String>,
        cargo_metadata_hash: ContentHash,
        cfg_profile_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        build_scripts_executed: bool,
        proc_macros_executed: bool,
    ) -> Result<Self, String> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err("rust provider request must include at least one candidate".to_string());
        }
        candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        for window in candidates.windows(2) {
            if window[0].sort_key() == window[1].sort_key() {
                return Err("rust provider request candidates must be unique".to_string());
            }
        }
        Ok(Self {
            provider,
            operation,
            candidates,
            rust_toolchain: validate_provider_metadata("rust toolchain", rust_toolchain)?,
            cargo_metadata_hash,
            cfg_profile_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            build_scripts_executed,
            proc_macros_executed,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProviderProvenance {
    pub provider: RustProviderKind,
    pub provider_version: String,
    pub rust_toolchain: String,
    pub cargo_metadata_hash: ContentHash,
    pub cfg_profile_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: RustProviderOperation,
    pub build_scripts_executed: bool,
    pub proc_macros_executed: bool,
}

impl RustProviderProvenance {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: RustProviderKind,
        provider_version: impl Into<String>,
        rust_toolchain: impl Into<String>,
        cargo_metadata_hash: ContentHash,
        cfg_profile_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        operation: RustProviderOperation,
        build_scripts_executed: bool,
        proc_macros_executed: bool,
    ) -> Result<Self, String> {
        Ok(Self {
            provider,
            provider_version: validate_provider_metadata("provider version", provider_version)?,
            rust_toolchain: validate_provider_metadata("rust toolchain", rust_toolchain)?,
            cargo_metadata_hash,
            cfg_profile_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            operation,
            build_scripts_executed,
            proc_macros_executed,
        })
    }

    pub fn assumptions(&self) -> Vec<String> {
        vec![
            "provider_resolved=true".to_string(),
            format!("provider={}", self.provider.as_str()),
            format!("provider_version={}", self.provider_version),
            format!("rust_toolchain={}", self.rust_toolchain),
            format!("cargo_metadata_hash={}", self.cargo_metadata_hash.as_str()),
            format!("cfg_profile_hash={}", self.cfg_profile_hash.as_str()),
            format!("environment_fingerprint={}", self.environment_fingerprint),
            format!("query_operation={}", self.operation.as_str()),
            format!("build_scripts_executed={}", self.build_scripts_executed),
            format!("proc_macros_executed={}", self.proc_macros_executed),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProviderCacheKey {
    pub provider: RustProviderKind,
    pub provider_version: String,
    pub rust_toolchain: String,
    pub cargo_metadata_hash: ContentHash,
    pub cfg_profile_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: RustProviderOperation,
    pub build_scripts_executed: bool,
    pub proc_macros_executed: bool,
    pub candidates: Vec<RustProviderCandidate>,
}

impl RustProviderCacheKey {
    pub fn new(
        provenance: RustProviderProvenance,
        candidates: impl IntoIterator<Item = RustProviderCandidate>,
    ) -> Result<Self, String> {
        let request = RustProviderRequest::new(
            provenance.provider,
            provenance.operation,
            candidates,
            provenance.rust_toolchain.clone(),
            provenance.cargo_metadata_hash.clone(),
            provenance.cfg_profile_hash.clone(),
            provenance.environment_fingerprint.clone(),
            provenance.build_scripts_executed,
            provenance.proc_macros_executed,
        )?;
        Ok(Self {
            provider: provenance.provider,
            provider_version: provenance.provider_version,
            rust_toolchain: request.rust_toolchain,
            cargo_metadata_hash: request.cargo_metadata_hash,
            cfg_profile_hash: request.cfg_profile_hash,
            environment_fingerprint: request.environment_fingerprint,
            operation: provenance.operation,
            build_scripts_executed: request.build_scripts_executed,
            proc_macros_executed: request.proc_macros_executed,
            candidates: request.candidates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustProviderOutput {
    pub facts: Vec<SemanticFact>,
    pub unknowns: Vec<TypedUnknown>,
    pub provenance: Option<RustProviderProvenance>,
}

impl RustProviderOutput {
    pub fn facts(
        provenance: RustProviderProvenance,
        facts: Vec<SemanticFact>,
        unknowns: Vec<TypedUnknown>,
    ) -> Self {
        Self {
            facts,
            unknowns,
            provenance: Some(provenance),
        }
    }

    pub fn unavailable(provider: RustProviderKind, operation: RustProviderOperation) -> Self {
        Self {
            facts: Vec::new(),
            unknowns: vec![TypedUnknown::new(
                UnknownClass::Recoverable,
                UnknownReasonCode::MissingDependency,
                format!("rust_provider:{}:{}", provider.as_str(), operation.as_str()),
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

fn validate_optional_repo_relative_path(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<String>, String> {
    match value {
        Some(value) => {
            validate_repo_relative_path(&value)
                .map_err(|_| format!("rust provider {field} must be repo-relative"))?;
            Ok(Some(value))
        }
        None => Ok(None),
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

    fn candidate(path: &str, start: usize) -> RustProviderCandidate {
        RustProviderCandidate::new(
            CodeUnitId::new(format!("unit:{path}:{start}")).expect("valid code unit id"),
            path,
            hash('a'),
            SourceRange::new(start, start + 10).expect("valid range"),
            Some("Cargo.toml".to_string()),
            Some("src/lib.rs".to_string()),
        )
        .expect("valid provider candidate")
    }

    #[test]
    fn provider_request_validates_and_sorts_candidate_scope() {
        let request = RustProviderRequest::new(
            RustProviderKind::RustAnalyzer,
            RustProviderOperation::ResolveSymbolsAndTypes,
            [candidate("src/b.rs", 10), candidate("src/a.rs", 0)],
            "rustc-1.88.0-stable",
            hash('b'),
            hash('c'),
            "env-sha256-abc",
            false,
            false,
        )
        .expect("valid provider request");

        assert_eq!(request.candidates[0].path, "src/a.rs");
        assert_eq!(request.candidates[1].path, "src/b.rs");
        assert_eq!(request.provider.as_str(), "rust_analyzer");
        assert_eq!(request.operation.as_str(), "resolve_symbols_and_types");
        assert!(!request.build_scripts_executed);
        assert!(!request.proc_macros_executed);
    }

    #[test]
    fn provider_request_rejects_unsafe_or_duplicate_candidate_scope() {
        assert!(RustProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "../secret.rs",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
            None,
            None,
        )
        .is_err());
        assert!(RustProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "src/a.rs",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
            Some("C:/repo/Cargo.toml".to_string()),
            None,
        )
        .is_err());

        let duplicate = candidate("src/a.rs", 0);
        assert!(RustProviderRequest::new(
            RustProviderKind::RustAnalyzer,
            RustProviderOperation::ResolveSymbolsAndTypes,
            [duplicate.clone(), duplicate],
            "rustc-1.88.0-stable",
            hash('b'),
            hash('c'),
            "env-sha256-abc",
            false,
            false,
        )
        .is_err());
    }

    #[test]
    fn provider_provenance_and_cache_key_record_required_dimensions() {
        let provenance = RustProviderProvenance::new(
            RustProviderKind::Rustc,
            "1.88.0",
            "rustc-1.88.0-stable",
            hash('c'),
            hash('d'),
            "env-sha256-def",
            RustProviderOperation::ResolveCallAndEffectFacts,
            false,
            false,
        )
        .expect("valid provenance");

        let assumptions = provenance.assumptions();
        assert!(assumptions.contains(&"provider_resolved=true".to_string()));
        assert!(assumptions.contains(&"provider=rustc".to_string()));
        assert!(assumptions.contains(&format!("cargo_metadata_hash={}", hash('c').as_str())));
        assert!(assumptions.contains(&"build_scripts_executed=false".to_string()));
        assert!(assumptions.contains(&"proc_macros_executed=false".to_string()));
        assert!(!assumptions.iter().any(|value| value.contains("src/a.rs")));

        let cache_key =
            RustProviderCacheKey::new(provenance, [candidate("src/a.rs", 8)]).expect("cache key");
        assert_eq!(cache_key.provider, RustProviderKind::Rustc);
        assert_eq!(cache_key.provider_version, "1.88.0");
        assert_eq!(cache_key.candidates[0].range.start_byte, 8);
    }

    #[test]
    fn provider_metadata_rejects_blank_path_like_and_control_text() {
        for value in [
            "",
            "   ",
            "/tmp/rust-analyzer",
            "file://env",
            "env\\fingerprint",
            "env\nx",
        ] {
            assert!(
                RustProviderProvenance::new(
                    RustProviderKind::RustAnalyzer,
                    value,
                    "rustc-1.88.0-stable",
                    hash('c'),
                    hash('d'),
                    "env-sha256-def",
                    RustProviderOperation::ResolveSymbolsAndTypes,
                    false,
                    false,
                )
                .is_err(),
                "provider version should reject {value:?}"
            );
        }
    }

    #[test]
    fn unavailable_provider_becomes_recoverable_unknown_without_facts() {
        let output = RustProviderOutput::unavailable(
            RustProviderKind::RustAnalyzer,
            RustProviderOperation::ResolveSymbolsAndTypes,
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
            "rust_provider:rust_analyzer:resolve_symbols_and_types"
        );
    }
}
