//! Bounded, non-executing MATLAB Package Manager inventory.
//!
//! This parser accepts only exact `resources/mpackage.json` inputs from the
//! R2024b+ package-definition family. It consumes supplied JSON bytes and never
//! starts MATLAB/Octave, opens a project, mutates the MATLAB path, installs a
//! toolbox, loads Java/MEX code, evaluates scripts, or includes Simulink.

pub mod unittest;

use super::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot,
    DependencyVersion, Evidence, FactCertainty, FactOrigin, Language, PackageIdentity, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MATLAB_CONFIG_ENGINE: &str = "repogrammar-matlab-package-config";
const MATLAB_CONFIG_METHOD: &str = "bounded_mpackage_dependency_inventory_v1";
const MATLAB_DEPENDENCY_LIMIT: usize = 2_000;
const MATLAB_NAME_LIMIT: usize = 128;
const MATLAB_VERSION_LIMIT: usize = 256;
const MATLAB_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: 128,
    max_members: 8_192,
    max_key_bytes: 256,
};

#[derive(Debug, Default)]
pub struct MatlabPackageConfigParser;

impl SourceParser for MatlabPackageConfigParser {
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
    if document.language != Language::MatlabConfig || !is_mpackage_path(document.path) {
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
        language: Language::MatlabConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance: Provenance::new(
            document.path,
            document.content_hash.clone(),
            document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?,
    };

    let (mut facts, dependencies) = inventory(document.text, &unit)?;
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.assumptions.as_slice(),
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.assumptions.as_slice(),
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

fn inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    if text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        return invalid_inventory(
            unit,
            UnknownReasonCode::InsufficientSupport,
            "manifest_byte_limit",
            "MATLAB package inventory exceeded the bounded input-byte limit",
        );
    }
    let Ok(false) = has_duplicate_or_excess_members(text, MATLAB_JSON_LIMITS) else {
        return invalid_inventory(
            unit,
            UnknownReasonCode::MissingProjectConfig,
            "ambiguous_or_over_budget_json",
            "MATLAB package definition is malformed, duplicate-key ambiguous, or over budget",
        );
    };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
        return invalid_inventory(
            unit,
            UnknownReasonCode::MissingProjectConfig,
            "malformed_package_definition",
            "MATLAB package definition is not a JSON object",
        );
    };

    let root_valid = root
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(is_matlab_identifier)
        && root
            .get("version")
            .and_then(Value::as_str)
            .is_some_and(is_bounded_version)
        && root.get("id").and_then(Value::as_str).is_some_and(is_uuid)
        && root
            .get("schemaVersion")
            .and_then(Value::as_str)
            .is_some_and(is_schema_version);
    if !root_valid {
        return invalid_inventory(
            unit,
            UnknownReasonCode::MissingProjectConfig,
            "invalid_package_identity",
            "MATLAB package definition lacks a valid bounded name, version, UUID, or schema version",
        );
    }

    let mut facts = vec![config_fact(unit)?];
    let schema = root
        .get("schemaVersion")
        .and_then(Value::as_str)
        .expect("root validity checked schemaVersion");
    if !matches!(schema, "1.0.0" | "1.1.0") {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport,
            "forward_schema",
            "MATLAB package schema is outside the qualified 1.0.0/1.1.0 snapshot",
        )?);
        return Ok((facts, Vec::new()));
    }

    let Some(dependency_value) = root.get("dependencies") else {
        return Ok((facts, Vec::new()));
    };
    if dependency_value.as_str() == Some("") {
        return Ok((facts, Vec::new()));
    }
    let Some(entries) = dependency_value.as_array() else {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig,
            "invalid_dependency_container",
            "MATLAB package dependencies must be an object array or the documented empty string",
        )?);
        return Ok((facts, Vec::new()));
    };
    if entries.len() > MATLAB_DEPENDENCY_LIMIT {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport,
            "dependency_record_limit",
            "MATLAB package dependency inventory exceeded the bounded record limit",
        )?);
        return Ok((facts, Vec::new()));
    }

    let mut declarations = BTreeMap::<String, Option<String>>::new();
    let mut conflicts = BTreeSet::new();
    let mut invalid = false;
    for entry in entries {
        let Some(entry) = entry.as_object() else {
            invalid = true;
            continue;
        };
        let Some(name) = entry
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| is_matlab_identifier(name))
        else {
            invalid = true;
            continue;
        };
        let Some(id) = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| is_uuid(id))
        else {
            invalid = true;
            continue;
        };
        let requirement = match entry.get("compatibleVersions") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.is_empty() => None,
            Some(Value::String(value)) if is_bounded_version(value) => Some(value.clone()),
            _ => {
                invalid = true;
                continue;
            }
        };
        if entry
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "id" | "compatibleVersions"))
        {
            invalid = true;
            continue;
        }
        let identity = format!("{name}@{id}");
        if conflicts.contains(&identity) {
            continue;
        }
        if let Some(existing) = declarations.get(&identity) {
            if existing != &requirement {
                declarations.remove(&identity);
                conflicts.insert(identity);
            }
            continue;
        }
        declarations.insert(identity, requirement);
    }

    let mut dependencies = Vec::with_capacity(declarations.len());
    for (identity, requirement) in declarations {
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(DependencyEcosystem::MatlabAddOn, identity)
                    .map_err(ParseError::Internal)?,
                requirement
                    .map(DependencyVersion::new)
                    .transpose()
                    .map_err(ParseError::Internal)?,
                None,
                DependencyScope::Unknown,
                false,
                DependencyDirectness::Direct,
                DependencyEvidenceLevel::ManifestDeclared,
                Evidence::new(
                    unit.id.clone(),
                    unit.range.clone(),
                    unit.provenance.clone(),
                    "bounded static MATLAB package dependency declaration; installation and runtime selection remain unknown",
                )
                .map_err(ParseError::Internal)?,
            )
            .map_err(ParseError::Internal)?,
        );
    }
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport,
            "partial_dependency_inventory",
            "MATLAB package dependencies with unsupported fields or shapes were omitted",
        )?);
    }
    if !conflicts.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts,
            "conflicting_dependency_declarations",
            "conflicting MATLAB package dependency declarations were omitted",
        )?);
    }
    Ok((facts, dependencies))
}

fn invalid_inventory(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    kind: &str,
    note: &str,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    Ok((
        vec![dependency_unknown(unit, reason, kind, note)?],
        Vec::new(),
    ))
}

fn config_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new("matlab.package_definition").map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded static resources/mpackage.json inventory; no MATLAB execution",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec!["matlab_package_schema=R2024b_plus".to_string()],
    })
}

fn dependency_unknown(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=matlab_dependency_inventory".to_string(),
            format!("matlab_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: MATLAB_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: MATLAB_CONFIG_METHOD.to_string(),
    }
}

fn is_mpackage_path(path: &str) -> bool {
    path.split('/')
        .collect::<Vec<_>>()
        .ends_with(&["resources", "mpackage.json"])
}

fn is_matlab_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MATLAB_NAME_LIMIT
        && value.is_ascii()
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn is_schema_version(value: &str) -> bool {
    value.len() <= 32
        && value.is_ascii()
        && value.split('.').count() == 3
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_bounded_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MATLAB_VERSION_LIMIT
        && value.is_ascii()
        && value.bytes().all(|byte| {
            !byte.is_ascii_control() && !matches!(byte, b'/' | b'\\' | b':' | b'@' | b'"' | b'\'')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    const UUID_A: &str = "e6c4123e-0068-42be-aef2-00d49d1509f5";

    fn output(text: &str) -> SourceParseOutput {
        MatlabPackageConfigParser
            .parse_with_context_output(
                SourceDocument {
                    path: "pkg/resources/mpackage.json",
                    language: Language::MatlabConfig,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse MATLAB package definition")
    }

    fn manifest(dependencies: &str) -> String {
        format!(
            r#"{{"name":"DemoPkg","version":"1.2.3","id":"af92112b-8b66-44d1-b4b1-848f54affa3e","schemaVersion":"1.1.0","dependencies":{dependencies}}}"#
        )
    }

    #[test]
    fn inventories_direct_package_uuid_identities_without_resolution_claims() {
        let parsed = output(&manifest(&format!(
            r#"[{{"name":"CornersPkg","compatibleVersions":">1.0.0","id":"{UUID_A}"}}]"#
        )));
        assert_eq!(parsed.dependencies.len(), 1);
        let dependency = &parsed.dependencies[0];
        assert_eq!(
            dependency.package.ecosystem,
            DependencyEcosystem::MatlabAddOn
        );
        assert_eq!(dependency.package.name, format!("CornersPkg@{UUID_A}"));
        assert_eq!(
            dependency
                .requirement
                .as_ref()
                .map(DependencyVersion::as_str),
            Some(">1.0.0")
        );
        assert_eq!(dependency.resolved_version, None);
        assert_eq!(dependency.directness, DependencyDirectness::Direct);
        assert_eq!(dependency.scope, DependencyScope::Unknown);
        assert_eq!(
            dependency.evidence_level,
            DependencyEvidenceLevel::ManifestDeclared
        );
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact.certainty == FactCertainty::Structural
        }));
    }

    #[test]
    fn empty_string_and_empty_array_mean_no_declared_dependencies() {
        for dependencies in [r#"""#, "[]"] {
            let parsed = output(&manifest(dependencies));
            assert!(parsed.dependencies.is_empty());
        }
    }

    #[test]
    fn unqualified_forward_schema_abstains_from_dependency_records() {
        let text = format!(
            r#"{{"name":"DemoPkg","version":"1.2.3","id":"af92112b-8b66-44d1-b4b1-848f54affa3e","schemaVersion":"1.2.0","dependencies":[{{"name":"CornersPkg","compatibleVersions":">1.0.0","id":"{UUID_A}"}}]}}"#
        );
        let parsed = output(&text);
        assert!(parsed.dependencies.is_empty());
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|value| value == "matlab_unknown_kind=forward_schema")
        }));
    }

    #[test]
    fn malformed_duplicate_conflicting_and_unsafe_values_fail_closed() {
        let duplicate = output(
            r#"{"name":"Demo","name":"Shadow","version":"1.0.0","id":"af92112b-8b66-44d1-b4b1-848f54affa3e","schemaVersion":"1.1.0"}"#,
        );
        assert!(duplicate.dependencies.is_empty());
        assert_eq!(
            duplicate.report.semantic_facts[0].kind,
            SemanticFactKind::Unknown
        );

        let conflict = output(&manifest(&format!(
            r#"[{{"name":"Dep","compatibleVersions":"1.0.0","id":"{UUID_A}"}},{{"name":"Dep","compatibleVersions":"2.0.0","id":"{UUID_A}"}}]"#
        )));
        assert!(conflict.dependencies.is_empty());
        assert!(conflict.report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
        }));

        for dependencies in [
            format!(r#"[{{"name":"Bad Name","id":"{UUID_A}"}}]"#),
            r#"[{"name":"Dep","id":"not-a-uuid"}]"#.to_string(),
            format!(
                r#"[{{"name":"Dep","compatibleVersions":"https://secret.invalid/pkg","id":"{UUID_A}"}}]"#
            ),
            format!(r#"[{{"name":"Dep","id":"{UUID_A}","futureMeaning":true}}]"#),
        ] {
            let parsed = output(&manifest(&dependencies));
            assert!(parsed.dependencies.is_empty(), "{dependencies}");
            assert!(parsed
                .report
                .semantic_facts
                .iter()
                .any(|fact| fact.kind == SemanticFactKind::Unknown));
        }
    }

    #[test]
    fn resource_limits_are_inclusive_and_plus_one_abstains() {
        let dependency = format!(r#"{{"name":"Dep","id":"{UUID_A}"}}"#);
        let at_limit = format!(
            "[{}]",
            vec![dependency.as_str(); MATLAB_DEPENDENCY_LIMIT].join(",")
        );
        let parsed = output(&manifest(&at_limit));
        assert_eq!(
            parsed.dependencies.len(),
            1,
            "identical declarations deduplicate"
        );

        let over_limit = format!(
            "[{}]",
            vec![dependency.as_str(); MATLAB_DEPENDENCY_LIMIT + 1].join(",")
        );
        let parsed = output(&manifest(&over_limit));
        assert!(parsed.dependencies.is_empty());
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|value| value == "matlab_unknown_kind=dependency_record_limit")
        }));
    }

    #[test]
    fn rejects_wrong_language_and_noncanonical_path() {
        let parser = MatlabPackageConfigParser;
        let base = SourceDocument {
            path: "mpackage.json",
            language: Language::MatlabConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text: "{}",
        };
        assert_eq!(
            parser.parse(base.clone()),
            Err(ParseError::UnsupportedLanguage)
        );
        assert_eq!(
            parser.parse(SourceDocument {
                path: "resources/mpackage.json",
                language: Language::Matlab,
                ..base
            }),
            Err(ParseError::UnsupportedLanguage)
        );
    }

    #[test]
    fn debug_output_contains_no_provider_or_url_fields() {
        let parsed = output(
            r#"{"name":"DemoPkg","version":"1.2.3","id":"af92112b-8b66-44d1-b4b1-848f54affa3e","schemaVersion":"1.1.0","provider":{"email":"secret@example.invalid","url":"https://secret.invalid"},"dependencies":[]}"#,
        );
        let debug = format!("{parsed:?}");
        assert!(!debug.contains("secret@example.invalid"));
        assert!(!debug.contains("secret.invalid"));
    }
}
