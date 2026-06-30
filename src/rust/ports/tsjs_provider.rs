//! Contract types for future TypeScript and JavaScript semantic providers.
//!
//! These types keep TypeScript compiler, language-service, CodeQL, and
//! abstract-interpretation objects outside RepoGrammar core, storage, and MCP.
//! Provider implementations must cross this boundary only with owned semantic
//! facts, evidence, provenance, and typed `UNKNOWN` values.

use crate::core::model::{
    CodeUnitId, ContentHash, SemanticFact, SourceRange, TypedUnknown, UnknownClass,
    UnknownReasonCode,
};
use crate::core::policy::paths::validate_repo_relative_path;

const MAX_PROVIDER_METADATA_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TsJsProviderKind {
    TypeScriptCompilerApi,
    TypeScriptLanguageService,
    CodeQl,
    Tajs,
    Jsai,
    Wala,
    ClosureCompiler,
}

impl TsJsProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeScriptCompilerApi => "typescript_compiler_api",
            Self::TypeScriptLanguageService => "typescript_language_service",
            Self::CodeQl => "codeql",
            Self::Tajs => "tajs",
            Self::Jsai => "jsai",
            Self::Wala => "wala",
            Self::ClosureCompiler => "closure_compiler",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TsJsProviderOperation {
    ProjectModel,
    ResolveModules,
    ResolveSymbolsAndCalls,
    AnalyzeDataflowAndTaint,
    CrossCheckClaim,
}

impl TsJsProviderOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectModel => "project_model",
            Self::ResolveModules => "resolve_modules",
            Self::ResolveSymbolsAndCalls => "resolve_symbols_and_calls",
            Self::AnalyzeDataflowAndTaint => "analyze_dataflow_and_taint",
            Self::CrossCheckClaim => "cross_check_claim",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TsJsModuleResolution {
    Node16,
    NodeNext,
    Bundler,
    Node10,
    Classic,
}

impl TsJsModuleResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node16 => "node16",
            Self::NodeNext => "nodenext",
            Self::Bundler => "bundler",
            Self::Node10 => "node10",
            Self::Classic => "classic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsJsProviderCandidate {
    pub code_unit_id: CodeUnitId,
    pub path: String,
    pub content_hash: ContentHash,
    pub range: SourceRange,
    pub project_config_path: Option<String>,
    pub package_json_path: Option<String>,
}

impl TsJsProviderCandidate {
    pub fn new(
        code_unit_id: CodeUnitId,
        path: impl Into<String>,
        content_hash: ContentHash,
        range: SourceRange,
        project_config_path: Option<String>,
        package_json_path: Option<String>,
    ) -> Result<Self, String> {
        let path = path.into();
        validate_repo_relative_path(&path)
            .map_err(|_| "tsjs provider candidate path must be repo-relative".to_string())?;
        Ok(Self {
            code_unit_id,
            path,
            content_hash,
            range,
            project_config_path: validate_optional_repo_relative_path(
                "project config path",
                project_config_path,
            )?,
            package_json_path: validate_optional_repo_relative_path(
                "package json path",
                package_json_path,
            )?,
        })
    }

    fn sort_key(&self) -> (&str, usize, usize, &str, Option<&str>, Option<&str>) {
        (
            &self.path,
            self.range.start_byte,
            self.range.end_byte,
            self.code_unit_id.as_str(),
            self.project_config_path.as_deref(),
            self.package_json_path.as_deref(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsJsProviderRequest {
    pub provider: TsJsProviderKind,
    pub operation: TsJsProviderOperation,
    pub candidates: Vec<TsJsProviderCandidate>,
    pub typescript_version: String,
    pub module_resolution: TsJsModuleResolution,
    pub project_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub allow_js: bool,
    pub check_js: bool,
}

impl TsJsProviderRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: TsJsProviderKind,
        operation: TsJsProviderOperation,
        candidates: impl IntoIterator<Item = TsJsProviderCandidate>,
        typescript_version: impl Into<String>,
        module_resolution: TsJsModuleResolution,
        project_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        allow_js: bool,
        check_js: bool,
    ) -> Result<Self, String> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err("tsjs provider request must include at least one candidate".to_string());
        }
        candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        for window in candidates.windows(2) {
            if window[0].sort_key() == window[1].sort_key() {
                return Err("tsjs provider request candidates must be unique".to_string());
            }
        }
        Ok(Self {
            provider,
            operation,
            candidates,
            typescript_version: validate_provider_metadata(
                "typescript version",
                typescript_version,
            )?,
            module_resolution,
            project_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            allow_js,
            check_js,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsJsProviderProvenance {
    pub provider: TsJsProviderKind,
    pub provider_version: String,
    pub typescript_version: String,
    pub module_resolution: TsJsModuleResolution,
    pub project_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: TsJsProviderOperation,
    pub allow_js: bool,
    pub check_js: bool,
}

impl TsJsProviderProvenance {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: TsJsProviderKind,
        provider_version: impl Into<String>,
        typescript_version: impl Into<String>,
        module_resolution: TsJsModuleResolution,
        project_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        operation: TsJsProviderOperation,
        allow_js: bool,
        check_js: bool,
    ) -> Result<Self, String> {
        Ok(Self {
            provider,
            provider_version: validate_provider_metadata("provider version", provider_version)?,
            typescript_version: validate_provider_metadata(
                "typescript version",
                typescript_version,
            )?,
            module_resolution,
            project_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            operation,
            allow_js,
            check_js,
        })
    }

    pub fn assumptions(&self) -> Vec<String> {
        vec![
            "provider_resolved=true".to_string(),
            format!("provider={}", self.provider.as_str()),
            format!("provider_version={}", self.provider_version),
            format!("typescript_version={}", self.typescript_version),
            format!("module_resolution={}", self.module_resolution.as_str()),
            format!("project_config_hash={}", self.project_config_hash.as_str()),
            format!("environment_fingerprint={}", self.environment_fingerprint),
            format!("query_operation={}", self.operation.as_str()),
            format!("allow_js={}", self.allow_js),
            format!("check_js={}", self.check_js),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsJsProviderCacheKey {
    pub provider: TsJsProviderKind,
    pub provider_version: String,
    pub typescript_version: String,
    pub module_resolution: TsJsModuleResolution,
    pub project_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: TsJsProviderOperation,
    pub allow_js: bool,
    pub check_js: bool,
    pub candidates: Vec<TsJsProviderCandidate>,
}

impl TsJsProviderCacheKey {
    pub fn new(
        provenance: TsJsProviderProvenance,
        candidates: impl IntoIterator<Item = TsJsProviderCandidate>,
    ) -> Result<Self, String> {
        let request = TsJsProviderRequest::new(
            provenance.provider,
            provenance.operation,
            candidates,
            provenance.typescript_version.clone(),
            provenance.module_resolution,
            provenance.project_config_hash.clone(),
            provenance.environment_fingerprint.clone(),
            provenance.allow_js,
            provenance.check_js,
        )?;
        Ok(Self {
            provider: provenance.provider,
            provider_version: provenance.provider_version,
            typescript_version: request.typescript_version,
            module_resolution: request.module_resolution,
            project_config_hash: request.project_config_hash,
            environment_fingerprint: request.environment_fingerprint,
            operation: provenance.operation,
            allow_js: request.allow_js,
            check_js: request.check_js,
            candidates: request.candidates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsJsProviderOutput {
    pub facts: Vec<SemanticFact>,
    pub unknowns: Vec<TypedUnknown>,
    pub provenance: Option<TsJsProviderProvenance>,
}

impl TsJsProviderOutput {
    pub fn facts(
        provenance: TsJsProviderProvenance,
        facts: Vec<SemanticFact>,
        unknowns: Vec<TypedUnknown>,
    ) -> Self {
        Self {
            facts,
            unknowns,
            provenance: Some(provenance),
        }
    }

    pub fn unavailable(provider: TsJsProviderKind, operation: TsJsProviderOperation) -> Self {
        Self {
            facts: Vec::new(),
            unknowns: vec![TypedUnknown::new(
                UnknownClass::Recoverable,
                UnknownReasonCode::MissingDependency,
                format!("tsjs_provider:{}:{}", provider.as_str(), operation.as_str()),
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
                .map_err(|_| format!("tsjs provider {field} must be repo-relative"))?;
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

    fn candidate(path: &str, start: usize) -> TsJsProviderCandidate {
        TsJsProviderCandidate::new(
            CodeUnitId::new(format!("unit:{path}:{start}")).expect("valid code unit id"),
            path,
            hash('a'),
            SourceRange::new(start, start + 10).expect("valid range"),
            Some("tsconfig.json".to_string()),
            Some("package.json".to_string()),
        )
        .expect("valid provider candidate")
    }

    #[test]
    fn provider_request_validates_and_sorts_candidate_scope() {
        let request = TsJsProviderRequest::new(
            TsJsProviderKind::TypeScriptCompilerApi,
            TsJsProviderOperation::ResolveModules,
            [candidate("src/b.ts", 10), candidate("src/a.ts", 0)],
            "6.0.0",
            TsJsModuleResolution::Node16,
            hash('b'),
            "env-sha256-abc",
            true,
            false,
        )
        .expect("valid provider request");

        assert_eq!(request.candidates[0].path, "src/a.ts");
        assert_eq!(request.candidates[1].path, "src/b.ts");
        assert_eq!(request.provider.as_str(), "typescript_compiler_api");
        assert_eq!(request.operation.as_str(), "resolve_modules");
        assert_eq!(request.module_resolution.as_str(), "node16");
        assert!(request.allow_js);
        assert!(!request.check_js);
    }

    #[test]
    fn provider_request_rejects_unsafe_or_duplicate_candidate_scope() {
        assert!(TsJsProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "../secret.ts",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
            None,
            None,
        )
        .is_err());
        assert!(TsJsProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "src/a.ts",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
            Some("C:/repo/tsconfig.json".to_string()),
            None,
        )
        .is_err());

        let duplicate = candidate("src/a.ts", 0);
        assert!(TsJsProviderRequest::new(
            TsJsProviderKind::TypeScriptCompilerApi,
            TsJsProviderOperation::ResolveModules,
            [duplicate.clone(), duplicate],
            "6.0.0",
            TsJsModuleResolution::Node16,
            hash('b'),
            "env-sha256-abc",
            true,
            false,
        )
        .is_err());
    }

    #[test]
    fn provider_provenance_and_cache_key_record_required_dimensions() {
        let provenance = TsJsProviderProvenance::new(
            TsJsProviderKind::TypeScriptLanguageService,
            "6.0.0",
            "6.0.0",
            TsJsModuleResolution::Bundler,
            hash('c'),
            "env-sha256-def",
            TsJsProviderOperation::ResolveSymbolsAndCalls,
            true,
            true,
        )
        .expect("valid provenance");

        let assumptions = provenance.assumptions();
        assert!(assumptions.contains(&"provider_resolved=true".to_string()));
        assert!(assumptions.contains(&"provider=typescript_language_service".to_string()));
        assert!(assumptions.contains(&"module_resolution=bundler".to_string()));
        assert!(assumptions.contains(&"allow_js=true".to_string()));
        assert!(assumptions.contains(&"check_js=true".to_string()));
        assert!(!assumptions.iter().any(|value| value.contains("src/a.ts")));

        let cache_key =
            TsJsProviderCacheKey::new(provenance, [candidate("src/a.ts", 8)]).expect("cache key");
        assert_eq!(
            cache_key.provider,
            TsJsProviderKind::TypeScriptLanguageService
        );
        assert_eq!(cache_key.provider_version, "6.0.0");
        assert_eq!(cache_key.candidates[0].range.start_byte, 8);
    }

    #[test]
    fn provider_metadata_rejects_blank_path_like_and_control_text() {
        for value in [
            "",
            "   ",
            "/tmp/typescript",
            "file://env",
            "env\\fingerprint",
            "env\nx",
        ] {
            assert!(
                TsJsProviderProvenance::new(
                    TsJsProviderKind::TypeScriptCompilerApi,
                    value,
                    "6.0.0",
                    TsJsModuleResolution::Node16,
                    hash('c'),
                    "env-sha256-def",
                    TsJsProviderOperation::ResolveModules,
                    true,
                    false,
                )
                .is_err(),
                "provider version should reject {value:?}"
            );
        }
    }

    #[test]
    fn unavailable_provider_becomes_recoverable_unknown_without_facts() {
        let output = TsJsProviderOutput::unavailable(
            TsJsProviderKind::TypeScriptCompilerApi,
            TsJsProviderOperation::ResolveModules,
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
            "tsjs_provider:typescript_compiler_api:resolve_modules"
        );
    }
}
