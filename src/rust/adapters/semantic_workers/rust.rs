//! Rust semantic-provider adapters.
//!
//! The implemented slice is a conservative Cargo metadata provider. It is an
//! explicit adapter boundary: default indexing does not call it, and it never
//! executes build scripts or procedural macros.

use crate::core::model::{
    CodeUnitId, Evidence, FactCertainty, FactOrigin, Provenance, RepositoryRevision, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId, TypedUnknown, UnknownClass, UnknownReasonCode,
};
use crate::core::policy::paths::validate_repo_relative_path;
use crate::ports::rust_provider::RustProviderError;
use crate::ports::rust_provider::{
    RustProviderCandidate, RustProviderKind, RustProviderOperation, RustProviderOutput,
    RustProviderProvenance, RustProviderRequest, RustSemanticProvider,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const CARGO_METADATA_ENGINE: &str = "cargo_metadata";
const CARGO_METADATA_METHOD: &str = "cargo_metadata_no_deps_v1";
const UNKNOWN_REVISION: &str = "UNKNOWN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoMetadataProviderError {
    InvalidRequest(String),
    ProtocolViolation(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoMetadataRustProvider {
    cargo_executable: PathBuf,
    provider_version: String,
}

impl CargoMetadataRustProvider {
    pub fn new(cargo_executable: impl Into<PathBuf>) -> Self {
        Self {
            cargo_executable: cargo_executable.into(),
            provider_version: "unknown".to_string(),
        }
    }

    pub fn with_provider_version(mut self, provider_version: impl Into<String>) -> Self {
        self.provider_version = provider_version.into();
        self
    }

    pub fn analyze_project(
        &self,
        project_root: impl AsRef<Path>,
        request: RustProviderRequest,
    ) -> Result<RustProviderOutput, CargoMetadataProviderError> {
        validate_request_shape(&request)?;
        let project_root = project_root.as_ref();
        if !project_root.is_absolute() || !project_root.is_dir() {
            return Err(CargoMetadataProviderError::InvalidRequest(
                "cargo metadata project root must be an absolute directory".to_string(),
            ));
        }
        let output = Command::new(&self.cargo_executable)
            .current_dir(project_root)
            .args(["metadata", "--format-version=1", "--no-deps"])
            .output();
        let output = match output {
            Ok(output) => output,
            Err(_) => {
                return Ok(RustProviderOutput::unavailable(
                    RustProviderKind::CargoMetadata,
                    RustProviderOperation::CargoProjectModel,
                ));
            }
        };
        if !output.status.success() {
            return Ok(project_model_unknown(
                UnknownReasonCode::MissingProjectConfig,
                "rust_provider:cargo_metadata:cargo_project_model",
                "run cargo metadata with a readable root Cargo.toml".to_string(),
            ));
        }
        let stdout = String::from_utf8(output.stdout).map_err(|_| {
            CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata stdout was not valid UTF-8".to_string(),
            )
        })?;
        parse_cargo_metadata_output(
            &stdout,
            project_root,
            request,
            self.provider_version.clone(),
        )
    }
}

impl RustSemanticProvider for CargoMetadataRustProvider {
    fn analyze_project(
        &self,
        project_root: &Path,
        request: RustProviderRequest,
    ) -> Result<RustProviderOutput, RustProviderError> {
        CargoMetadataRustProvider::analyze_project(self, project_root, request).map_err(|error| {
            match error {
                CargoMetadataProviderError::InvalidRequest(message) => {
                    RustProviderError::InvalidRequest(message)
                }
                CargoMetadataProviderError::ProtocolViolation(message) => {
                    RustProviderError::ProtocolViolation(message)
                }
            }
        })
    }
}

pub fn parse_cargo_metadata_output(
    metadata_json: &str,
    project_root: impl AsRef<Path>,
    request: RustProviderRequest,
    provider_version: impl Into<String>,
) -> Result<RustProviderOutput, CargoMetadataProviderError> {
    validate_request_shape(&request)?;
    let provenance = RustProviderProvenance::new(
        request.provider,
        provider_version,
        request.rust_toolchain.clone(),
        request.cargo_metadata_hash.clone(),
        request.cfg_profile_hash.clone(),
        request.environment_fingerprint.clone(),
        request.operation,
        request.build_scripts_executed,
        request.proc_macros_executed,
    )
    .map_err(CargoMetadataProviderError::InvalidRequest)?;
    let metadata: Value = serde_json::from_str(metadata_json).map_err(|_| {
        CargoMetadataProviderError::ProtocolViolation(
            "cargo metadata output must be a JSON object".to_string(),
        )
    })?;
    let object = metadata.as_object().ok_or_else(|| {
        CargoMetadataProviderError::ProtocolViolation(
            "cargo metadata output must be a JSON object".to_string(),
        )
    })?;
    let packages = object
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata packages must be an array".to_string(),
            )
        })?;
    let workspace_members = object
        .get("workspace_members")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let candidates = request
        .candidates
        .iter()
        .map(|candidate| (candidate.path.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut facts = Vec::new();
    let mut unknowns = Vec::new();
    if let Some(root_candidate) = request.candidates.first() {
        facts.push(cargo_project_fact(
            root_candidate,
            &provenance,
            "workspace",
            "cargo.workspace",
            "Cargo metadata workspace model",
            [
                "cargo_fact=workspace".to_string(),
                format!("workspace_member_count={workspace_members}"),
                "cargo_metadata_no_deps=true".to_string(),
            ],
        )?);
    }
    let project_root = project_root.as_ref();
    for package in packages {
        let package = package.as_object().ok_or_else(|| {
            CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata package must be an object".to_string(),
            )
        })?;
        let name = required_string(package, "name")?;
        let manifest_path = required_string(package, "manifest_path")?;
        let manifest_path = repo_relative_metadata_path(project_root, manifest_path)?;
        let Some(candidate) = candidates.get(manifest_path.as_str()).copied() else {
            unknowns.push(
                TypedUnknown::new(
                    UnknownClass::Recoverable,
                    UnknownReasonCode::MissingProjectConfig,
                    "rust_provider:cargo_metadata:manifest_candidate",
                    Some(
                        "index Cargo.toml manifests before accepting Cargo metadata facts"
                            .to_string(),
                    ),
                )
                .expect("static cargo metadata UNKNOWN is valid"),
            );
            continue;
        };
        let package_token = stable_token(name);
        facts.push(cargo_project_fact(
            candidate,
            &provenance,
            &format!("package:{package_token}"),
            &format!("cargo.package.{package_token}"),
            "Cargo metadata package scope",
            [
                "cargo_fact=package".to_string(),
                format!("package_name={}", sanitize_metadata(name)),
                "cargo_metadata_no_deps=true".to_string(),
            ],
        )?);
        facts.extend(target_facts(
            package,
            candidate,
            &provenance,
            &package_token,
        )?);
        facts.extend(feature_facts(
            package,
            candidate,
            &provenance,
            &package_token,
        )?);
        facts.extend(dependency_facts(
            package,
            candidate,
            &provenance,
            &package_token,
        )?);
    }
    Ok(RustProviderOutput::facts(provenance, facts, unknowns))
}

fn validate_request_shape(request: &RustProviderRequest) -> Result<(), CargoMetadataProviderError> {
    if request.provider != RustProviderKind::CargoMetadata
        || request.operation != RustProviderOperation::CargoProjectModel
    {
        return Err(CargoMetadataProviderError::InvalidRequest(
            "cargo metadata provider requires CargoMetadata/CargoProjectModel request".to_string(),
        ));
    }
    if request.build_scripts_executed || request.proc_macros_executed {
        return Err(CargoMetadataProviderError::InvalidRequest(
            "cargo metadata provider must not report build script or proc macro execution"
                .to_string(),
        ));
    }
    Ok(())
}

fn target_facts(
    package: &serde_json::Map<String, Value>,
    candidate: &RustProviderCandidate,
    provenance: &RustProviderProvenance,
    package_token: &str,
) -> Result<Vec<SemanticFact>, CargoMetadataProviderError> {
    let Some(targets) = package.get("targets").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut facts = Vec::new();
    for target in targets {
        let target = target.as_object().ok_or_else(|| {
            CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata target must be an object".to_string(),
            )
        })?;
        let name = required_string(target, "name")?;
        let target_token = stable_token(name);
        let kinds = target
            .get("kind")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(stable_token)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let kind_profile = if kinds.is_empty() {
            "unknown".to_string()
        } else {
            kinds.into_iter().collect::<Vec<_>>().join("_")
        };
        facts.push(cargo_project_fact(
            candidate,
            provenance,
            &format!("target:{package_token}:{target_token}:{kind_profile}"),
            &format!("cargo.target.{package_token}.{kind_profile}.{target_token}"),
            "Cargo metadata target scope",
            [
                "cargo_fact=target".to_string(),
                format!("package_token={package_token}"),
                format!("target_name={}", sanitize_metadata(name)),
                format!("target_kind={kind_profile}"),
                "cargo_metadata_no_deps=true".to_string(),
            ],
        )?);
    }
    Ok(facts)
}

fn feature_facts(
    package: &serde_json::Map<String, Value>,
    candidate: &RustProviderCandidate,
    provenance: &RustProviderProvenance,
    package_token: &str,
) -> Result<Vec<SemanticFact>, CargoMetadataProviderError> {
    let Some(features) = package.get("features").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut facts = Vec::new();
    for feature_name in features.keys() {
        let feature_token = stable_token(feature_name);
        facts.push(cargo_project_fact(
            candidate,
            provenance,
            &format!("feature:{package_token}:{feature_token}"),
            &format!("cargo.feature.{package_token}.{feature_token}"),
            "Cargo metadata feature scope",
            [
                "cargo_fact=feature".to_string(),
                format!("package_token={package_token}"),
                format!("feature_name={}", sanitize_metadata(feature_name)),
                "cargo_metadata_no_deps=true".to_string(),
            ],
        )?);
    }
    Ok(facts)
}

fn dependency_facts(
    package: &serde_json::Map<String, Value>,
    candidate: &RustProviderCandidate,
    provenance: &RustProviderProvenance,
    package_token: &str,
) -> Result<Vec<SemanticFact>, CargoMetadataProviderError> {
    let Some(dependencies) = package.get("dependencies").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut facts = Vec::new();
    for dependency in dependencies {
        let dependency = dependency.as_object().ok_or_else(|| {
            CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata dependency must be an object".to_string(),
            )
        })?;
        let name = required_string(dependency, "name")?;
        let dependency_token = stable_token(name);
        let kind = dependency
            .get("kind")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("normal");
        let optional = dependency
            .get("optional")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        facts.push(cargo_project_fact(
            candidate,
            provenance,
            &format!("dependency:{package_token}:{dependency_token}"),
            &format!("cargo.dependency.{package_token}.{dependency_token}"),
            "Cargo metadata dependency scope",
            [
                "cargo_fact=dependency".to_string(),
                format!("package_token={package_token}"),
                format!("dependency_name={}", sanitize_metadata(name)),
                format!("dependency_kind={}", stable_token(kind)),
                format!("optional={optional}"),
                "cargo_metadata_no_deps=true".to_string(),
            ],
        )?);
    }
    Ok(facts)
}

fn cargo_project_fact(
    candidate: &RustProviderCandidate,
    provenance: &RustProviderProvenance,
    subject_suffix: &str,
    target: &str,
    note: &str,
    extra_assumptions: impl IntoIterator<Item = String>,
) -> Result<SemanticFact, CargoMetadataProviderError> {
    let mut assumptions = provenance.assumptions();
    assumptions.extend(extra_assumptions);
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: format!(
            "{}#cargo_metadata:{subject_suffix}",
            candidate.code_unit_id.as_str()
        ),
        target: Some(SymbolId::new(target).map_err(CargoMetadataProviderError::InvalidRequest)?),
        origin: FactOrigin {
            engine: CARGO_METADATA_ENGINE.to_string(),
            engine_version: provenance.provider_version.clone(),
            method: CARGO_METADATA_METHOD.to_string(),
        },
        certainty: FactCertainty::Semantic,
        evidence: Evidence::new(
            CodeUnitId::new(candidate.code_unit_id.as_str())
                .map_err(CargoMetadataProviderError::InvalidRequest)?,
            SourceRange::new(candidate.range.start_byte, candidate.range.end_byte)
                .map_err(CargoMetadataProviderError::InvalidRequest)?,
            Provenance::new(
                &candidate.path,
                candidate.content_hash.clone(),
                RepositoryRevision::new(UNKNOWN_REVISION)
                    .map_err(CargoMetadataProviderError::InvalidRequest)?,
            )
            .map_err(CargoMetadataProviderError::InvalidRequest)?,
            note,
        )
        .map_err(CargoMetadataProviderError::InvalidRequest)?,
        assumptions,
    })
}

fn project_model_unknown(
    reason: UnknownReasonCode,
    affected_claim: &str,
    recovery: String,
) -> RustProviderOutput {
    RustProviderOutput {
        facts: Vec::new(),
        unknowns: vec![TypedUnknown::new(
            UnknownClass::Recoverable,
            reason,
            affected_claim,
            Some(recovery),
        )
        .expect("static cargo metadata UNKNOWN is valid")],
        provenance: None,
    }
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, CargoMetadataProviderError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CargoMetadataProviderError::ProtocolViolation(format!(
                "cargo metadata {field} must be a non-empty string"
            ))
        })
}

fn repo_relative_metadata_path(
    project_root: &Path,
    metadata_path: &str,
) -> Result<String, CargoMetadataProviderError> {
    if validate_repo_relative_path(metadata_path).is_ok() {
        return Ok(metadata_path.to_string());
    }
    let path = Path::new(metadata_path);
    if !path.is_absolute() {
        return Err(CargoMetadataProviderError::ProtocolViolation(
            "cargo metadata path must be repo-relative or under project root".to_string(),
        ));
    }
    let relative = match path.strip_prefix(project_root) {
        Ok(relative) => relative,
        Err(_) => {
            let canonical_root = std::fs::canonicalize(project_root).map_err(|_| {
                CargoMetadataProviderError::ProtocolViolation(
                    "cargo metadata path must stay under project root".to_string(),
                )
            })?;
            path.strip_prefix(&canonical_root).map_err(|_| {
                CargoMetadataProviderError::ProtocolViolation(
                    "cargo metadata path must stay under project root".to_string(),
                )
            })?
        }
    };
    let path = slash_path(relative)?;
    validate_repo_relative_path(&path).map_err(|_| {
        CargoMetadataProviderError::ProtocolViolation(
            "cargo metadata path must become repository-relative".to_string(),
        )
    })?;
    Ok(path)
}

fn slash_path(path: &Path) -> Result<String, CargoMetadataProviderError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(value) = component else {
            return Err(CargoMetadataProviderError::ProtocolViolation(
                "cargo metadata path must not contain prefixes or traversal".to_string(),
            ));
        };
        parts.push(value.to_string_lossy().to_string());
    }
    Ok(parts.join("/"))
}

fn sanitize_metadata(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control() && *character != '/' && *character != '\\')
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn stable_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if token.trim_matches('_').is_empty() {
        "unknown".to_string()
    } else {
        token
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ContentHash;
    use crate::test_support::{create_test_symlink_dir, TempWorkspace};
    use std::fs;

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
            Some(path.to_string()),
            None,
        )
        .expect("valid provider candidate")
    }

    fn request(candidates: Vec<RustProviderCandidate>) -> RustProviderRequest {
        RustProviderRequest::new(
            RustProviderKind::CargoMetadata,
            RustProviderOperation::CargoProjectModel,
            candidates,
            "rustc-1.88.0-stable",
            hash('b'),
            hash('c'),
            "env-sha256-cargo",
            false,
            false,
        )
        .expect("valid cargo metadata request")
    }

    fn sample_metadata(root_manifest: &str, member_manifest: &str) -> String {
        format!(
            r#"{{
  "packages": [
    {{
      "name": "root-crate",
      "manifest_path": "{root_manifest}",
      "targets": [{{"name": "root_crate", "kind": ["lib"]}}],
      "features": {{"default": ["serde"], "cli": []}},
      "dependencies": [{{"name": "serde", "kind": null, "optional": false}}]
    }},
    {{
      "name": "member-crate",
      "manifest_path": "{member_manifest}",
      "targets": [{{"name": "member_bin", "kind": ["bin"]}}],
      "features": {{"default": []}},
      "dependencies": [{{"name": "anyhow", "kind": "dev", "optional": true}}]
    }}
  ],
  "workspace_members": ["root-crate 0.1.0", "member-crate 0.1.0"],
  "resolve": null,
  "target_directory": "target",
  "version": 1
}}"#
        )
    }

    #[test]
    fn parses_cargo_metadata_into_owned_project_config_facts() {
        let output = parse_cargo_metadata_output(
            &sample_metadata("Cargo.toml", "crates/member/Cargo.toml"),
            Path::new("E:/repo"),
            request(vec![
                candidate("Cargo.toml", 0),
                candidate("crates/member/Cargo.toml", 20),
            ]),
            "1.88.0",
        )
        .expect("cargo metadata output should parse");

        assert!(output.provenance.is_some());
        assert!(output.unknowns.is_empty());
        let targets = output
            .facts
            .iter()
            .filter_map(|fact| fact.target.as_ref().map(|target| target.as_str()))
            .collect::<BTreeSet<_>>();
        assert!(targets.contains("cargo.workspace"));
        assert!(targets.contains("cargo.package.root_crate"));
        assert!(targets.contains("cargo.target.root_crate.lib.root_crate"));
        assert!(targets.contains("cargo.feature.root_crate.default"));
        assert!(targets.contains("cargo.dependency.root_crate.serde"));
        assert!(targets.contains("cargo.package.member_crate"));
        assert!(targets.contains("cargo.target.member_crate.bin.member_bin"));
        assert!(targets.contains("cargo.dependency.member_crate.anyhow"));
        assert!(output.facts.iter().all(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact.certainty == FactCertainty::Semantic
                && fact.origin.engine == CARGO_METADATA_ENGINE
                && fact.origin.method == CARGO_METADATA_METHOD
        }));
        assert!(output.facts.iter().any(|fact| {
            fact.assumptions
                .contains(&"build_scripts_executed=false".to_string())
                && fact
                    .assumptions
                    .contains(&"proc_macros_executed=false".to_string())
                && fact
                    .assumptions
                    .contains(&"cargo_metadata_no_deps=true".to_string())
        }));
    }

    #[test]
    fn absolute_manifest_paths_are_scoped_to_project_root() {
        let project_root = std::env::current_dir().expect("cwd");
        let manifest_path = project_root.join("Cargo.toml").display().to_string();
        let output = parse_cargo_metadata_output(
            &sample_metadata(
                &manifest_path.replace('\\', "\\\\"),
                "crates/member/Cargo.toml",
            ),
            &project_root,
            request(vec![
                candidate("Cargo.toml", 0),
                candidate("crates/member/Cargo.toml", 20),
            ]),
            "1.88.0",
        )
        .expect("absolute manifest under root should parse");

        assert!(output
            .facts
            .iter()
            .any(|fact| fact.evidence.provenance.path == "Cargo.toml"));
    }

    #[test]
    fn absolute_manifest_paths_accept_canonical_project_root_equivalents() {
        let workspace = TempWorkspace::new("cargo-metadata-canonical-root");
        let actual_root = workspace.path().join("actual");
        fs::create_dir_all(&actual_root).expect("actual root");
        let linked_root = workspace.path().join("linked");
        if !create_test_symlink_dir(&actual_root, &linked_root) {
            return;
        }
        let manifest_path = actual_root
            .canonicalize()
            .expect("canonical root")
            .join("Cargo.toml")
            .display()
            .to_string();

        let output = parse_cargo_metadata_output(
            &sample_metadata(
                &manifest_path.replace('\\', "\\\\"),
                "crates/member/Cargo.toml",
            ),
            &linked_root,
            request(vec![
                candidate("Cargo.toml", 0),
                candidate("crates/member/Cargo.toml", 20),
            ]),
            "1.88.0",
        )
        .expect("canonical manifest under symlink-equivalent root should parse");

        assert!(output.unknowns.is_empty());
        assert!(output.facts.iter().any(|fact| {
            fact.evidence.provenance.path == "Cargo.toml"
                && fact.target.as_ref().map(|target| target.as_str())
                    == Some("cargo.package.root_crate")
        }));
    }

    #[test]
    fn missing_manifest_candidate_becomes_recoverable_unknown() {
        let output = parse_cargo_metadata_output(
            &sample_metadata("Cargo.toml", "crates/member/Cargo.toml"),
            Path::new("E:/repo"),
            request(vec![candidate("Cargo.toml", 0)]),
            "1.88.0",
        )
        .expect("cargo metadata output should parse with scoped unknown");

        assert_eq!(output.unknowns.len(), 1);
        assert_eq!(output.unknowns[0].class, UnknownClass::Recoverable);
        assert_eq!(
            output.unknowns[0].reason,
            UnknownReasonCode::MissingProjectConfig
        );
        assert_eq!(
            output.unknowns[0].affected_claim,
            "rust_provider:cargo_metadata:manifest_candidate"
        );
    }

    #[test]
    fn rejects_requests_that_claim_build_script_or_proc_macro_execution() {
        let mut request = request(vec![candidate("Cargo.toml", 0)]);
        request.build_scripts_executed = true;
        assert_eq!(
            parse_cargo_metadata_output(
                &sample_metadata("Cargo.toml", "crates/member/Cargo.toml"),
                Path::new("E:/repo"),
                request,
                "1.88.0",
            ),
            Err(CargoMetadataProviderError::InvalidRequest(
                "cargo metadata provider must not report build script or proc macro execution"
                    .to_string()
            ))
        );
    }

    #[test]
    fn unavailable_cargo_executable_returns_recoverable_unknown() {
        let workspace = TempWorkspace::new("cargo-metadata-provider-unavailable");
        let missing = workspace.path().join("missing-cargo.exe");
        let provider = CargoMetadataRustProvider::new(missing);
        let output = provider
            .analyze_project(workspace.path(), request(vec![candidate("Cargo.toml", 0)]))
            .expect("unavailable cargo should be a provider output");

        assert!(output.facts.is_empty());
        assert_eq!(output.unknowns.len(), 1);
        assert_eq!(output.unknowns[0].class, UnknownClass::Recoverable);
        assert_eq!(
            output.unknowns[0].reason,
            UnknownReasonCode::MissingDependency
        );
        assert_eq!(
            output.unknowns[0].affected_claim,
            "rust_provider:cargo_metadata:cargo_project_model"
        );
    }
}
