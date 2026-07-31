//! Bounded Swift project-metadata inventory.
//!
//! Swift source remains discovery-only. This adapter reads supplied
//! `swift-config` bytes only: it never evaluates `Package.swift`, invokes
//! SwiftPM, opens a toolchain, downloads packages, or executes plugins/macros.

use super::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot,
    DependencyVersion, Evidence, FactCertainty, FactOrigin, Language, PackageIdentity, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const SWIFT_CONFIG_ENGINE: &str = "repogrammar-swift-project-config";
const SWIFT_CONFIG_METHOD: &str = "bounded_swift_project_inventory_v1";
const SWIFT_DEPENDENCY_LIMIT: usize = 2_000;
const SWIFT_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: 128,
    max_members: 8_192,
    max_key_bytes: 256,
};

#[derive(Debug, Default)]
pub struct SwiftProjectConfigParser;

impl SourceParser for SwiftProjectConfigParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::SwiftConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let unit = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::SwiftConfig,
        kind: CodeUnitKind::ProjectConfig,
        range: range.clone(),
        provenance: Provenance::new(
            document.path,
            document.content_hash.clone(),
            document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?,
    };
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    let (mut facts, dependencies) = match basename {
        "Package.resolved" => package_resolved_inventory(&document, &unit)?,
        "Package.swift" => (
            vec![
                config_fact(
                    &unit,
                    "swift.project_manifest",
                    "swift_project_config=package_manifest",
                    "SwiftPM manifest inventory; manifest is never evaluated",
                )?,
                dependency_unknown(
                    &unit,
                    UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                    "executable_package_manifest",
                    "Package.swift is executable and was not evaluated for dependency declarations",
                )?,
            ],
            Vec::new(),
        ),
        ".swift-version" => (
            vec![config_fact(
                &unit,
                "swift.toolchain_selector",
                "swift_project_config=toolchain_selector",
                "Swift toolchain selector inventory; no toolchain is selected",
            )?],
            Vec::new(),
        ),
        name if is_version_specific_manifest(name) => (
            vec![
                config_fact(
                    &unit,
                    "swift.version_specific_manifest",
                    "swift_project_config=version_specific_manifest",
                    "version-specific SwiftPM manifest inventory; manifest is never evaluated",
                )?,
                dependency_unknown(
                    &unit,
                    UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                    "executable_version_specific_manifest",
                    "version-specific Package.swift is executable and was not evaluated",
                )?,
            ],
            Vec::new(),
        ),
        _ => return Err(ParseError::UnsupportedLanguage),
    };
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.note.as_str(),
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.note.as_str(),
            ))
    });
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    let dependencies = DependencySnapshot::new(dependencies, Vec::new())
        .map_err(ParseError::Internal)?
        .dependencies;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies,
    })
}

fn package_resolved_inventory(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let Ok(false) = has_duplicate_or_excess_members(document.text, SWIFT_JSON_LIMITS) else {
        return malformed_lock_inventory(unit);
    };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(document.text) else {
        return malformed_lock_inventory(unit);
    };
    let schema_version = root.get("version").and_then(Value::as_u64);
    if !matches!(schema_version, Some(2 | 3)) {
        return malformed_lock_inventory(unit);
    }
    let Some(pins) = root.get("pins").and_then(Value::as_array) else {
        return malformed_lock_inventory(unit);
    };

    let mut facts = vec![config_fact(
        unit,
        "swift.package_resolution_inventory",
        "swift_project_config=package_resolved",
        "bounded SwiftPM Package.resolved dependency inventory",
    )?];
    let mut versions = BTreeMap::<String, String>::new();
    let mut admitted = BTreeSet::new();
    let mut conflicts = BTreeSet::new();
    let mut invalid = false;
    let mut unsupported_state = false;
    let mut truncated = false;

    for pin in pins {
        let Some(pin) = pin.as_object() else {
            invalid = true;
            continue;
        };
        let Some(identity) = pin
            .get("identity")
            .and_then(Value::as_str)
            .filter(|identity| is_swift_package_identity(identity))
        else {
            invalid = true;
            continue;
        };
        let Some(state) = pin.get("state").and_then(Value::as_object) else {
            invalid = true;
            continue;
        };
        let version = state
            .get("version")
            .and_then(Value::as_str)
            .filter(|version| is_swift_semantic_version(version));
        let has_branch = state.get("branch").is_some_and(|value| !value.is_null());
        if has_branch || version.is_none() {
            unsupported_state = true;
            continue;
        }
        let version = version.expect("checked above");
        if !admitted.contains(identity) {
            if admitted.len() == SWIFT_DEPENDENCY_LIMIT {
                truncated = true;
                continue;
            }
            admitted.insert(identity.to_string());
        }
        if conflicts.contains(identity) {
            continue;
        }
        if let Some(existing) = versions.get(identity) {
            if existing != version {
                versions.remove(identity);
                conflicts.insert(identity.to_string());
            }
            continue;
        }
        versions.insert(identity.to_string(), version.to_string());
    }

    let mut dependencies = Vec::with_capacity(versions.len());
    for (identity, version) in versions {
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(DependencyEcosystem::SwiftPackageManager, identity)
                    .map_err(ParseError::Internal)?,
                None,
                Some(DependencyVersion::new(version).map_err(ParseError::Internal)?),
                DependencyScope::Unknown,
                false,
                DependencyDirectness::Unknown,
                DependencyEvidenceLevel::LockfileResolved,
                Evidence::new(
                    unit.id.clone(),
                    unit.range.clone(),
                    unit.provenance.clone(),
                    "bounded Package.resolved pin; directness and installation remain unknown",
                )
                .map_err(ParseError::Internal)?,
            )
            .map_err(ParseError::Internal)?,
        );
    }
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_package_resolved_inventory",
            "malformed Package.resolved pins were omitted",
        )?);
    }
    if unsupported_state {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_package_resolved_state",
            "branch, revision-only, or non-semantic-version SwiftPM pins were omitted",
        )?);
    }
    if !conflicts.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_package_resolved_pins",
            "SwiftPM identities with conflicting resolved versions were omitted",
        )?);
    }
    if truncated {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "package_resolved_dependency_limit",
            "SwiftPM pin inventory exceeded the bounded record limit",
        )?);
    }
    if !dependencies.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "package_resolved_directness",
            "Package.resolved does not identify root-direct versus transitive pins",
        )?);
    }
    Ok((facts, dependencies))
}

fn malformed_lock_inventory(
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    Ok((
        vec![dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "malformed_package_resolved",
            "Package.resolved is malformed, ambiguous, over-budget, or outside schema 2/3",
        )?],
        Vec::new(),
    ))
}

fn config_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: config_origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![assumption.to_string()],
    })
}

fn dependency_unknown(
    unit: &CodeUnit,
    reason: &str,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason).map_err(ParseError::Internal)?),
        origin: config_origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=swift_dependency_inventory".to_string(),
            format!("swift_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: SWIFT_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: SWIFT_CONFIG_METHOD.to_string(),
    }
}

fn is_version_specific_manifest(name: &str) -> bool {
    let Some(version) = name
        .strip_prefix("Package@swift-")
        .and_then(|value| value.strip_suffix(".swift"))
    else {
        return false;
    };
    let components = version.split('.').collect::<Vec<_>>();
    (1..=3).contains(&components.len())
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn is_swift_package_identity(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn is_swift_semantic_version(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return false;
    }
    let core_and_pre = match value.split_once('+') {
        Some((_, "")) => return false,
        Some((core_and_pre, build)) if valid_semver_suffix(build, false) => core_and_pre,
        Some(_) => return false,
        None => value,
    };
    let core = match core_and_pre.split_once('-') {
        Some((_, "")) => return false,
        Some((core, prerelease)) if valid_semver_suffix(prerelease, true) => core,
        Some(_) => return false,
        None => core_and_pre,
    };
    let components = core.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty()
                && component.bytes().all(|byte| byte.is_ascii_digit())
                && (component == &"0" || !component.starts_with('0'))
        })
}

fn valid_semver_suffix(value: &str, reject_numeric_leading_zero: bool) -> bool {
    value.split('.').all(|component| {
        !component.is_empty()
            && component
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !(reject_numeric_leading_zero
                && component.len() > 1
                && component.bytes().all(|byte| byte.is_ascii_digit())
                && component.starts_with('0'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(path: &str, text: &str) -> SourceParseOutput {
        SwiftProjectConfigParser
            .parse_with_context_output(
                SourceDocument {
                    path,
                    language: Language::SwiftConfig,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse Swift config")
    }

    fn unknowns(output: &SourceParseOutput) -> Vec<(&str, &str)> {
        output
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| {
                (
                    fact.target.as_ref().expect("reason").as_str(),
                    fact.assumptions[0].as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn package_resolved_v2_and_v3_emit_resolved_unknown_directness_inventory() {
        for schema in [2, 3] {
            let parsed = output(
                "Package.resolved",
                &format!(
                    r#"{{"pins":[{{"identity":"swift-argument-parser","kind":"remoteSourceControl","location":"https://example.invalid/private","state":{{"revision":"abc","version":"1.5.0"}}}}],"version":{schema}}}"#
                ),
            );
            assert_eq!(parsed.dependencies.len(), 1);
            let dependency = &parsed.dependencies[0];
            assert_eq!(
                dependency.package.ecosystem,
                DependencyEcosystem::SwiftPackageManager
            );
            assert_eq!(dependency.package.name, "swift-argument-parser");
            assert_eq!(
                dependency
                    .resolved_version
                    .as_ref()
                    .map(DependencyVersion::as_str),
                Some("1.5.0")
            );
            assert_eq!(dependency.directness, DependencyDirectness::Unknown);
            assert_eq!(
                dependency.evidence_level,
                DependencyEvidenceLevel::LockfileResolved
            );
            assert!(unknowns(&parsed).contains(&(
                "InsufficientSupport",
                "affected_claim=swift_dependency_inventory"
            )));
            assert!(!format!("{parsed:?}").contains("example.invalid"));
        }
    }

    #[test]
    fn malformed_duplicate_conflicting_and_unsupported_pins_fail_closed() {
        for text in [
            r#"{"pins":[],"pins":[],"version":2}"#,
            r#"{"pins":[],"version":1}"#,
            r#"{"pins":"not-an-array","version":2}"#,
        ] {
            let parsed = output("Package.resolved", text);
            assert!(parsed.dependencies.is_empty());
            assert!(unknowns(&parsed).contains(&(
                "MissingProjectConfig",
                "affected_claim=swift_dependency_inventory"
            )));
        }

        let parsed = output(
            "Package.resolved",
            r#"{"pins":[{"identity":"same","state":{"version":"1.0.0"}},{"identity":"same","state":{"version":"2.0.0"}},{"identity":"branch-only","state":{"branch":"main","revision":"abc"}},{"identity":"Upper","state":{"version":"1.0.0"}}],"version":3}"#,
        );
        assert!(parsed.dependencies.is_empty());
        let pairs = unknowns(&parsed);
        assert!(pairs
            .iter()
            .any(|(reason, _)| *reason == "ConflictingFacts"));
        assert!(pairs
            .iter()
            .any(|(reason, _)| *reason == "InsufficientSupport"));
        assert!(pairs
            .iter()
            .any(|(reason, _)| *reason == "MissingProjectConfig"));
    }

    #[test]
    fn executable_manifests_are_never_evaluated() {
        for path in ["Package.swift", "Package@swift-6.3.swift"] {
            let parsed = output(path, "fatalError(\"must not run\")\n");
            assert!(parsed.dependencies.is_empty());
            assert!(unknowns(&parsed).contains(&(
                "InsufficientSupport",
                "affected_claim=swift_dependency_inventory"
            )));
        }
        let selector = output(".swift-version", "6.3.3\n");
        assert!(selector.dependencies.is_empty());
        assert!(unknowns(&selector).is_empty());
    }

    #[test]
    fn dependency_limit_is_inclusive_and_overflow_is_typed() {
        let pins = (0..=SWIFT_DEPENDENCY_LIMIT)
            .map(|index| {
                serde_json::json!({
                    "identity": format!("package-{index}"),
                    "state": {"version": "1.0.0"},
                })
            })
            .collect::<Vec<_>>();
        let text = serde_json::json!({"pins": pins, "version": 3}).to_string();
        let parsed = output("Package.resolved", &text);
        assert_eq!(parsed.dependencies.len(), SWIFT_DEPENDENCY_LIMIT);
        assert!(unknowns(&parsed)
            .iter()
            .any(|(reason, _)| *reason == "ResourceLimit"));
    }

    #[test]
    fn resolved_version_grammar_rejects_non_semver_lookalikes() {
        for version in [
            "1.0.0-",
            "1.0.0+",
            "1.0.0-01",
            "1.0.0-alpha..1",
            "01.0.0",
            "1.0",
            "v1.0.0",
        ] {
            assert!(!is_swift_semantic_version(version), "{version}");
        }
        for version in ["0.0.0", "1.2.3-alpha.1", "1.2.3+01", "1.2.3-rc-1+build.7"] {
            assert!(is_swift_semantic_version(version), "{version}");
        }
    }
}
