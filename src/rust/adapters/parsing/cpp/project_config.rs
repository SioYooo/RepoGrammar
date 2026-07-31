//! Bounded C/C++ project-configuration inventory.
//!
//! These parsers emit structural `PROJECT_CONFIG` facts and bounded manifest
//! dependency inventory. They never execute build tooling and their output
//! cannot directly support a family or prove library behavior.

use super::CPP_ANCHOR_ENGINE;
use crate::adapters::parsing::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use crate::adapters::parsing::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot,
    DependencyVersion, Evidence, FactCertainty, FactOrigin, Language, PackageIdentity, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument, SourceParseOutput};
use std::collections::{BTreeMap, BTreeSet};

const CPP_CONFIG_METHOD: &str = "bounded_cpp_project_inventory_v2";

/// The maximum number of per-translation-unit inventory facts emitted for a
/// single `compile_commands.json`, keeping the fact set bounded.
const COMPILE_COMMANDS_TU_LIMIT: usize = 100;
/// Maximum unique direct dependency declarations emitted by one bounded C/C++
/// manifest. Excess input is represented by a typed UNKNOWN rather than an
/// unbounded allocation or a falsely complete inventory.
const CPP_CONFIG_DEPENDENCY_LIMIT: usize = 2_000;
const CPP_CONFIG_JSON_MEMBER_LIMIT: usize = 8_192;
const CPP_CONFIG_JSON_KEY_LIMIT: usize = 256;
const CPP_CONFIG_JSON_DEPTH_LIMIT: usize = 128;

const CPP_CONFIG_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: CPP_CONFIG_JSON_DEPTH_LIMIT,
    max_members: CPP_CONFIG_JSON_MEMBER_LIMIT,
    max_key_bytes: CPP_CONFIG_JSON_KEY_LIMIT,
};

pub(super) fn parse(document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
    parse_output(document).map(|output| output.report)
}

pub(super) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let unit = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::CppConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    };
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    let (mut facts, dependencies) = match basename {
        "compile_commands.json" => (compile_commands_facts(&document, &unit)?, Vec::new()),
        "vcpkg.json" => vcpkg_inventory(&document, &unit)?,
        "conanfile.txt" => conanfile_inventory(&document, &unit)?,
        _ => (Vec::new(), Vec::new()),
    };
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.range.start_byte,
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.range.start_byte,
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

fn config_project_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: CPP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CPP_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![assumption.to_string()],
    })
}

fn config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(
            SymbolId::new(UnknownReasonCode::MissingProjectConfig.as_protocol_str())
                .map_err(ParseError::Internal)?,
        ),
        origin: FactOrigin {
            engine: CPP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CPP_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=cpp_project_config".to_string(),
            format!("cpp_unknown_kind={kind}"),
        ],
    })
}

fn dependency_unknown_fact(
    unit: &CodeUnit,
    reason: &str,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: CPP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CPP_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=cpp_dependency_inventory".to_string(),
            format!("cpp_unknown_kind={kind}"),
        ],
    })
}

fn manifest_dependency(
    unit: &CodeUnit,
    ecosystem: DependencyEcosystem,
    name: &str,
    requirement: Option<&str>,
    note: &str,
) -> Result<DependencyRecord, ParseError> {
    DependencyRecord::new(
        PackageIdentity::new(ecosystem, name).map_err(ParseError::Internal)?,
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
            note,
        )
        .map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)
}

fn compile_commands_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<Vec<SemanticFact>, ParseError> {
    let Ok(serde_json::Value::Array(entries)) =
        serde_json::from_str::<serde_json::Value>(document.text)
    else {
        return Ok(vec![config_unknown_fact(
            document,
            unit,
            "malformed_compile_commands",
            "compile_commands.json is not a readable JSON array",
        )?]);
    };
    let mut facts = vec![config_project_fact(
        document,
        unit,
        &format!("cpp.compile_commands.entries:{}", entries.len()),
        "cpp_project_config=compile_commands",
        "bounded compile_commands.json entry count",
    )?];
    let mut emitted = 0usize;
    let mut has_unlocatable = false;
    for entry in &entries {
        let Some(file) = entry.get("file").and_then(serde_json::Value::as_str) else {
            has_unlocatable = true;
            continue;
        };
        if !is_safe_repo_relative_path(file) {
            has_unlocatable = true;
            continue;
        }
        if emitted < COMPILE_COMMANDS_TU_LIMIT {
            facts.push(config_project_fact(
                document,
                unit,
                &format!("cpp.compile_commands.translation_unit:{file}"),
                "cpp_project_config=translation_unit",
                "bounded compile_commands.json translation unit",
            )?);
            emitted += 1;
        }
    }
    if has_unlocatable {
        facts.push(config_unknown_fact(
            document,
            unit,
            "compile_commands_entry_outside_repo",
            "compile_commands.json references translation units that are not locatable repo-relative files",
        )?);
    }
    Ok(facts)
}

fn vcpkg_inventory(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let Ok(false) = has_duplicate_or_excess_members(document.text, CPP_CONFIG_JSON_LIMITS) else {
        return malformed_vcpkg_inventory(document, unit);
    };
    let Ok(serde_json::Value::Object(value)) =
        serde_json::from_str::<serde_json::Value>(document.text)
    else {
        return malformed_vcpkg_inventory(document, unit);
    };
    let mut facts = Vec::new();
    let Some(raw_dependencies) = value.get("dependencies") else {
        return Ok((facts, Vec::new()));
    };
    let Some(raw_dependencies) = raw_dependencies.as_array() else {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "invalid_vcpkg_dependencies_shape",
            "vcpkg dependencies is not an array; dependency inventory is unavailable",
        )?);
        return Ok((facts, Vec::new()));
    };
    let mut requirements = BTreeMap::<String, Option<String>>::new();
    let mut admitted_names = BTreeSet::new();
    let mut conflicting_names = BTreeSet::new();
    let mut has_invalid_entry = false;
    let mut has_unsupported_semantics = false;
    let mut truncated = false;
    for dependency in raw_dependencies {
        let (name, requirement) = match dependency {
            serde_json::Value::String(name) => (Some(name.as_str()), None),
            serde_json::Value::Object(object) => {
                if object.keys().any(|key| key != "name" && key != "version>=") {
                    has_unsupported_semantics = true;
                }
                let name = object.get("name").and_then(serde_json::Value::as_str);
                let requirement = match object.get("version>=") {
                    None => None,
                    Some(serde_json::Value::String(version))
                        if is_bounded_version_text(version) =>
                    {
                        Some(format!(">={version}"))
                    }
                    Some(_) => {
                        has_invalid_entry = true;
                        continue;
                    }
                };
                (name, requirement)
            }
            _ => (None, None),
        };
        let Some(name) = name.filter(|name| is_vcpkg_package_name(name)) else {
            has_invalid_entry = true;
            continue;
        };
        if !admitted_names.contains(name) {
            if admitted_names.len() == CPP_CONFIG_DEPENDENCY_LIMIT {
                truncated = true;
                continue;
            }
            admitted_names.insert(name.to_string());
        }
        if conflicting_names.contains(name) {
            continue;
        }
        if let Some(existing) = requirements.get(name) {
            if existing != &requirement {
                requirements.remove(name);
                conflicting_names.insert(name.to_string());
            }
            continue;
        }
        requirements.insert(name.to_string(), requirement);
    }
    let mut dependencies = Vec::with_capacity(requirements.len());
    for (name, requirement) in requirements {
        facts.push(config_project_fact(
            document,
            unit,
            &format!("cpp.dependency:{name}"),
            "cpp_project_config=vcpkg_dependency",
            "bounded vcpkg.json dependency",
        )?);
        dependencies.push(manifest_dependency(
            unit,
            DependencyEcosystem::Vcpkg,
            &name,
            requirement.as_deref(),
            "bounded vcpkg.json dependency declaration",
        )?);
    }
    if has_invalid_entry {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_vcpkg_dependency_inventory",
            "vcpkg dependency entries with unsupported shapes or package names were omitted",
        )?);
    }
    if has_unsupported_semantics {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "partial_vcpkg_dependency_semantics",
            "vcpkg dependency fields outside name and version>= were inventoried without interpretation",
        )?);
    }
    if !conflicting_names.is_empty() {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_vcpkg_requirements",
            "vcpkg package names with conflicting direct requirements were omitted",
        )?);
    }
    if truncated {
        facts.push(dependency_unknown_fact(
            unit,
            "ResourceLimit",
            "vcpkg_dependency_limit",
            "vcpkg dependency inventory exceeded the bounded record limit",
        )?);
    }
    Ok((facts, dependencies))
}

fn malformed_vcpkg_inventory(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    Ok((
        vec![
            config_unknown_fact(
                document,
                unit,
                "malformed_vcpkg_manifest",
                "vcpkg.json is not a readable unique-member JSON object",
            )?,
            dependency_unknown_fact(
                unit,
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "malformed_vcpkg_dependency_inventory",
                "vcpkg dependency inventory is unavailable because the manifest is malformed or exceeds JSON member bounds",
            )?,
        ],
        Vec::new(),
    ))
}

fn conanfile_inventory(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let mut facts = Vec::new();
    let mut in_requires = false;
    let mut seen_section = false;
    let mut requirements = BTreeMap::<String, String>::new();
    let mut admitted_names = BTreeSet::new();
    let mut conflicting_names = BTreeSet::new();
    let mut has_invalid_entry = false;
    let mut has_unsupported_semantics = false;
    let mut truncated = false;
    for raw_line in document.text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') || line.ends_with(']') {
            let valid_header = line.starts_with('[')
                && line.ends_with(']')
                && line.len() > 2
                && !line[1..line.len() - 1].contains('[')
                && !line[1..line.len() - 1].contains(']');
            if !valid_header {
                has_invalid_entry = true;
                in_requires = false;
                continue;
            }
            seen_section = true;
            in_requires = match line {
                "[requires]" => true,
                "[generators]" | "[options]" | "[layout]" => false,
                "[tool_requires]" | "[test_requires]" => {
                    has_unsupported_semantics = true;
                    false
                }
                _ => {
                    has_unsupported_semantics = true;
                    false
                }
            };
            continue;
        }
        if !seen_section {
            has_invalid_entry = true;
            continue;
        }
        if in_requires {
            let mut fields = line.split_whitespace();
            let reference = fields.next().unwrap_or("");
            if fields.next().is_some() {
                has_invalid_entry = true;
                continue;
            }
            let Some((name, requirement)) = parse_conan_reference(reference) else {
                has_invalid_entry = true;
                continue;
            };
            if !admitted_names.contains(name) {
                if admitted_names.len() == CPP_CONFIG_DEPENDENCY_LIMIT {
                    truncated = true;
                    continue;
                }
                admitted_names.insert(name.to_string());
            }
            if conflicting_names.contains(name) {
                continue;
            }
            if let Some(existing) = requirements.get(name) {
                if existing != requirement {
                    requirements.remove(name);
                    conflicting_names.insert(name.to_string());
                }
                continue;
            }
            requirements.insert(name.to_string(), requirement.to_string());
        }
    }
    let mut dependencies = Vec::with_capacity(requirements.len());
    for (name, requirement) in requirements {
        let reference = format!("{name}/{requirement}");
        facts.push(config_project_fact(
            document,
            unit,
            &format!("cpp.dependency:{reference}"),
            "cpp_project_config=conan_dependency",
            "bounded conanfile.txt requirement",
        )?);
        dependencies.push(manifest_dependency(
            unit,
            DependencyEcosystem::Conan,
            &name,
            Some(&requirement),
            "bounded conanfile.txt direct requirement",
        )?);
    }
    if has_invalid_entry {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_conan_dependency_inventory",
            "Conan requirements with unsupported reference syntax were omitted",
        )?);
    }
    if has_unsupported_semantics {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_conan_sections",
            "Conan dependency-bearing or unknown sections outside [requires] were not inventoried",
        )?);
    }
    if !conflicting_names.is_empty() {
        facts.push(dependency_unknown_fact(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_conan_requirements",
            "Conan package names with conflicting direct requirements were omitted",
        )?);
    }
    if truncated {
        facts.push(dependency_unknown_fact(
            unit,
            "ResourceLimit",
            "conan_dependency_limit",
            "Conan dependency inventory exceeded the bounded record limit",
        )?);
    }
    Ok((facts, dependencies))
}

fn is_safe_repo_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn is_vcpkg_package_name(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && token.as_bytes().first().is_some_and(|byte| *byte != b'-')
        && token.as_bytes().last().is_some_and(|byte| *byte != b'-')
}

fn is_conan_package_token(token: &str) -> bool {
    (2..=101).contains(&token.len())
        && token.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b'+')
        })
        && token
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn is_bounded_version_text(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 254
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+' | b'#')
        })
    {
        return false;
    }
    let Some((version, port_version)) = value.split_once('#') else {
        return value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            && value
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric);
    };
    !version.is_empty()
        && !port_version.is_empty()
        && !port_version.contains('#')
        && version
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && version
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && port_version.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_conan_reference(reference: &str) -> Option<(&str, &str)> {
    if reference.len() > 257 || reference.matches('/').count() != 1 {
        return None;
    }
    let (name, requirement) = reference.split_once('/')?;
    (is_conan_package_token(name) && is_conan_package_token(requirement))
        .then_some((name, requirement))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse_config(text: &str, path: &str) -> ParseReport {
        parse_config_output(text, path).report
    }

    fn parse_config_output(text: &str, path: &str) -> SourceParseOutput {
        parse_output(SourceDocument {
            path,
            language: Language::CppConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        })
        .expect("parse C/C++ project config")
    }

    fn config_targets(report: &ParseReport) -> Vec<String> {
        let mut targets = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::ProjectConfig)
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_pairs(report: &ParseReport) -> Vec<(String, String)> {
        let mut pairs = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| {
                let reason = fact.target.as_ref().expect("reason").as_str().to_string();
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (reason, claim)
            })
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }

    #[test]
    fn compile_commands_inventory_reports_unlocatable_entries() {
        let report = parse_config(
            "[{\"directory\": \"/build\", \"file\": \"src/api.cc\", \"command\": \"clang\"},\
             {\"directory\": \"/build\", \"file\": \"/abs/other.cc\", \"command\": \"clang\"}]",
            "compile_commands.json",
        );

        let targets = config_targets(&report);
        assert!(targets.contains(&"cpp.compile_commands.entries:2".to_string()));
        assert!(targets.contains(&"cpp.compile_commands.translation_unit:src/api.cc".to_string()));
        assert!(unknown_pairs(&report).contains(&(
            "MissingProjectConfig".to_string(),
            "cpp_project_config".to_string(),
        )));
    }

    #[test]
    fn compile_commands_translation_unit_inventory_is_bounded() {
        let entries = (0..=COMPILE_COMMANDS_TU_LIMIT)
            .map(|index| {
                serde_json::json!({
                    "directory": "/build",
                    "file": format!("src/unit_{index}.cc"),
                    "command": "clang"
                })
            })
            .collect::<Vec<_>>();
        let text = serde_json::to_string(&entries).expect("serialize compile commands");
        let report = parse_config(&text, "compile_commands.json");
        let targets = config_targets(&report);

        assert!(targets.contains(&format!(
            "cpp.compile_commands.entries:{}",
            COMPILE_COMMANDS_TU_LIMIT + 1
        )));
        assert_eq!(
            targets
                .iter()
                .filter(|target| { target.starts_with("cpp.compile_commands.translation_unit:") })
                .count(),
            COMPILE_COMMANDS_TU_LIMIT
        );
        assert!(unknown_pairs(&report).is_empty());
    }

    #[test]
    fn vcpkg_and_conan_dependencies_include_generic_inventory() {
        let vcpkg = parse_config_output(
            "{\"dependencies\": [{\"name\": \"fmt\", \"version>=\": \"10.1.1#2\"}, {\"name\": \"boost-test\"}]}",
            "vcpkg.json",
        );
        let vcpkg_targets = config_targets(&vcpkg.report);
        assert!(vcpkg_targets.contains(&"cpp.dependency:fmt".to_string()));
        assert!(vcpkg_targets.contains(&"cpp.dependency:boost-test".to_string()));
        assert_eq!(vcpkg.dependencies.len(), 2);
        assert_eq!(
            vcpkg
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.package.ecosystem,
                    dependency.package.name.as_str(),
                    dependency
                        .requirement
                        .as_ref()
                        .map(DependencyVersion::as_str),
                    dependency.scope,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    DependencyEcosystem::Vcpkg,
                    "boost-test",
                    None,
                    DependencyScope::Unknown,
                ),
                (
                    DependencyEcosystem::Vcpkg,
                    "fmt",
                    Some(">=10.1.1#2"),
                    DependencyScope::Unknown,
                ),
            ]
        );
        assert!(vcpkg.dependencies.iter().all(|dependency| {
            dependency.directness == DependencyDirectness::Direct
                && !dependency.optional
                && dependency.evidence_level == DependencyEvidenceLevel::ManifestDeclared
        }));

        let conan = parse_config_output(
            "[requires]\nfmt/10.1.1\ngtest/1.14.0\n\n[options]\nfmt:shared=True\n",
            "conanfile.txt",
        );
        let conan_targets = config_targets(&conan.report);
        assert!(conan_targets.contains(&"cpp.dependency:fmt/10.1.1".to_string()));
        assert!(conan_targets.contains(&"cpp.dependency:gtest/1.14.0".to_string()));
        assert_eq!(
            conan
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.package.ecosystem,
                    dependency.package.name.as_str(),
                    dependency
                        .requirement
                        .as_ref()
                        .map(DependencyVersion::as_str),
                ))
                .collect::<Vec<_>>(),
            vec![
                (DependencyEcosystem::Conan, "fmt", Some("10.1.1")),
                (DependencyEcosystem::Conan, "gtest", Some("1.14.0")),
            ]
        );

        assert!(vcpkg
            .report
            .semantic_facts
            .iter()
            .chain(&conan.report.semantic_facts)
            .all(|fact| fact.kind == SemanticFactKind::ProjectConfig));
    }

    #[test]
    fn malformed_json_configs_emit_project_config_unknowns() {
        let compile_commands = parse_config("{ not json", "compile_commands.json");
        assert_eq!(
            unknown_pairs(&compile_commands),
            vec![(
                "MissingProjectConfig".to_string(),
                "cpp_project_config".to_string(),
            )]
        );
        assert!(config_targets(&compile_commands).is_empty());

        let vcpkg = parse_config_output("{ not json", "vcpkg.json");
        assert_eq!(
            unknown_pairs(&vcpkg.report),
            vec![
                (
                    "MissingProjectConfig".to_string(),
                    "cpp_dependency_inventory".to_string(),
                ),
                (
                    "MissingProjectConfig".to_string(),
                    "cpp_project_config".to_string(),
                ),
            ]
        );
        assert!(vcpkg.dependencies.is_empty());
        assert!(config_targets(&vcpkg.report).is_empty());
    }

    #[test]
    fn duplicate_vcpkg_json_members_fail_closed() {
        for manifest in [
            "{\"dependencies\":[\"fmt\"],\"dependencies\":[\"zlib\"]}",
            "{\"dependenc\\u0069es\":[\"fmt\"],\"dependencies\":[\"zlib\"]}",
        ] {
            let duplicate_dependencies = parse_config_output(manifest, "vcpkg.json");
            assert!(duplicate_dependencies.dependencies.is_empty());
            assert_eq!(
                unknown_pairs(&duplicate_dependencies.report),
                vec![
                    (
                        "MissingProjectConfig".to_string(),
                        "cpp_dependency_inventory".to_string(),
                    ),
                    (
                        "MissingProjectConfig".to_string(),
                        "cpp_project_config".to_string(),
                    ),
                ]
            );
        }

        for object in [
            "{\"name\":\"fmt\",\"name\":\"zlib\"}",
            "{\"name\":\"fmt\",\"version>=\":\"1.0\",\"version>=\":\"2.0\"}",
        ] {
            let output =
                parse_config_output(&format!("{{\"dependencies\":[{object}]}}"), "vcpkg.json");
            assert!(output.dependencies.is_empty());
            assert!(unknown_pairs(&output.report).contains(&(
                "MissingProjectConfig".to_string(),
                "cpp_dependency_inventory".to_string(),
            )));
        }
    }

    #[test]
    fn vcpkg_json_member_scanner_bounds_are_inclusive() {
        let maximum_key = "k".repeat(CPP_CONFIG_JSON_KEY_LIMIT);
        assert_eq!(
            has_duplicate_or_excess_members(
                &format!("{{\"{maximum_key}\":null}}"),
                CPP_CONFIG_JSON_LIMITS,
            ),
            Ok(false)
        );
        let oversized_key = "k".repeat(CPP_CONFIG_JSON_KEY_LIMIT + 1);
        assert!(has_duplicate_or_excess_members(
            &format!("{{\"{oversized_key}\":null}}"),
            CPP_CONFIG_JSON_LIMITS,
        )
        .is_err());

        let maximum_members = (0..CPP_CONFIG_JSON_MEMBER_LIMIT)
            .map(|index| format!("\"k{index}\":null"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            has_duplicate_or_excess_members(
                &format!("{{{maximum_members}}}"),
                CPP_CONFIG_JSON_LIMITS,
            ),
            Ok(false)
        );
        assert!(has_duplicate_or_excess_members(
            &format!("{{{maximum_members},\"overflow\":null}}"),
            CPP_CONFIG_JSON_LIMITS,
        )
        .is_err());

        let maximum_depth = format!(
            "{}null{}",
            "[".repeat(CPP_CONFIG_JSON_DEPTH_LIMIT),
            "]".repeat(CPP_CONFIG_JSON_DEPTH_LIMIT)
        );
        assert_eq!(
            has_duplicate_or_excess_members(&maximum_depth, CPP_CONFIG_JSON_LIMITS),
            Ok(false)
        );
        let excessive_depth = format!(
            "{}null{}",
            "[".repeat(CPP_CONFIG_JSON_DEPTH_LIMIT + 1),
            "]".repeat(CPP_CONFIG_JSON_DEPTH_LIMIT + 1)
        );
        assert!(has_duplicate_or_excess_members(&excessive_depth, CPP_CONFIG_JSON_LIMITS).is_err());
    }

    #[test]
    fn unsupported_and_conflicting_manifest_entries_abstain_without_execution() {
        let vcpkg = parse_config_output(
            "{\"dependencies\": [\"fmt\", 17, {\"features\": [\"core\"]}, {\"name\": \"conditional\", \"platform\": \"windows\"}, {\"name\": \"zlib\", \"version>=\": \"1.2\"}, {\"name\": \"zlib\", \"version>=\": \"1.3\"}, \"Boost\", \"boost.asio\", \"-bad\", \"bad-\"]}",
            "vcpkg.json",
        );
        assert_eq!(
            vcpkg
                .dependencies
                .iter()
                .map(|dependency| dependency.package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["conditional", "fmt"]
        );
        assert!(unknown_pairs(&vcpkg.report).contains(&(
            "MissingProjectConfig".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
        assert!(unknown_pairs(&vcpkg.report).contains(&(
            "ConflictingFacts".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
        assert!(unknown_pairs(&vcpkg.report).contains(&(
            "InsufficientSupport".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));

        let conan = parse_config_output(
            "[requires]\nfmt/10.1.1\nfmt/11.0.0\nrange/[>=1.0]\nrevision/1.0#abc\nlegacy/1.0@user/channel\nUpper/1.0\nx/1.0\nvalid/X\n\n[tool_requires]\ncmake/3.23.0\n",
            "conanfile.txt",
        );
        assert!(conan.dependencies.is_empty());
        assert!(unknown_pairs(&conan.report).contains(&(
            "ConflictingFacts".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
        assert!(unknown_pairs(&conan.report).contains(&(
            "MissingProjectConfig".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
        assert!(unknown_pairs(&conan.report).contains(&(
            "InsufficientSupport".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));

        let malformed_conan = parse_config_output("[requires\nfmt/10.1.1\n", "conanfile.txt");
        assert!(malformed_conan.dependencies.is_empty());
        assert!(unknown_pairs(&malformed_conan.report).contains(&(
            "MissingProjectConfig".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
    }

    #[test]
    fn vcpkg_dependency_inventory_is_bounded_and_marks_truncation() {
        let dependencies = (0..=CPP_CONFIG_DEPENDENCY_LIMIT)
            .map(|index| format!("package-{index}"))
            .collect::<Vec<_>>();
        let text = serde_json::json!({ "dependencies": dependencies }).to_string();
        let output = parse_config_output(&text, "vcpkg.json");

        assert_eq!(output.dependencies.len(), CPP_CONFIG_DEPENDENCY_LIMIT);
        assert!(unknown_pairs(&output.report).contains(&(
            "ResourceLimit".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
    }

    #[test]
    fn dependency_name_admission_limit_does_not_refill_after_conflicts() {
        let mut dependencies = Vec::new();
        for version in ["1", "2"] {
            dependencies.extend((0..CPP_CONFIG_DEPENDENCY_LIMIT).map(|index| {
                serde_json::json!({
                    "name": format!("package-{index}"),
                    "version>=": version,
                })
            }));
        }
        dependencies.push(serde_json::Value::String("outside-budget".to_string()));
        let text = serde_json::json!({ "dependencies": dependencies }).to_string();
        let output = parse_config_output(&text, "vcpkg.json");

        assert!(output.dependencies.is_empty());
        assert!(unknown_pairs(&output.report).contains(&(
            "ConflictingFacts".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
        assert!(unknown_pairs(&output.report).contains(&(
            "ResourceLimit".to_string(),
            "cpp_dependency_inventory".to_string(),
        )));
    }
}
