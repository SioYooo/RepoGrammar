//! Bounded C/C++ project-configuration inventory.
//!
//! These parsers emit structural `PROJECT_CONFIG` facts only. They never
//! execute build tooling and their output cannot directly support a family.

use super::CPP_ANCHOR_ENGINE;
use crate::adapters::parsing::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument};
use std::collections::BTreeSet;

const CPP_CONFIG_METHOD: &str = "bounded_cpp_project_inventory_v1";

/// The maximum number of per-translation-unit inventory facts emitted for a
/// single `compile_commands.json`, keeping the fact set bounded.
const COMPILE_COMMANDS_TU_LIMIT: usize = 100;

pub(super) fn parse(document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
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
    let mut facts = match basename {
        "compile_commands.json" => compile_commands_facts(&document, &unit)?,
        "vcpkg.json" => vcpkg_facts(&document, &unit)?,
        "conanfile.txt" => conanfile_facts(&document, &unit)?,
        _ => Vec::new(),
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
    Ok(ParseReport {
        units,
        ir_nodes,
        ir_edges,
        semantic_facts: facts,
        diagnostics: Vec::new(),
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

fn vcpkg_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<Vec<SemanticFact>, ParseError> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(document.text) else {
        return Ok(vec![config_unknown_fact(
            document,
            unit,
            "malformed_vcpkg_manifest",
            "vcpkg.json is not readable JSON",
        )?]);
    };
    let mut facts = Vec::new();
    if let Some(dependencies) = value
        .get("dependencies")
        .and_then(serde_json::Value::as_array)
    {
        let mut names = BTreeSet::new();
        for dependency in dependencies {
            let name = match dependency {
                serde_json::Value::String(name) => Some(name.clone()),
                serde_json::Value::Object(object) => object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                _ => None,
            };
            if let Some(name) = name.filter(|name| is_dependency_token(name)) {
                names.insert(name);
            }
        }
        for name in names {
            facts.push(config_project_fact(
                document,
                unit,
                &format!("cpp.dependency:{name}"),
                "cpp_project_config=vcpkg_dependency",
                "bounded vcpkg.json dependency",
            )?);
        }
    }
    Ok(facts)
}

fn conanfile_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mut facts = Vec::new();
    let mut in_requires = false;
    let mut names = BTreeSet::new();
    for raw_line in document.text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_requires = line == "[requires]";
            continue;
        }
        if in_requires {
            let reference = line.split_whitespace().next().unwrap_or("");
            if is_dependency_token(reference) {
                names.insert(reference.to_string());
            }
        }
    }
    for name in names {
        facts.push(config_project_fact(
            document,
            unit,
            &format!("cpp.dependency:{name}"),
            "cpp_project_config=conan_dependency",
            "bounded conanfile.txt requirement",
        )?);
    }
    Ok(facts)
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

fn is_dependency_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '/' | '+')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse_config(text: &str, path: &str) -> ParseReport {
        parse(SourceDocument {
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
    fn vcpkg_and_conan_dependencies_remain_project_config_inventory() {
        let vcpkg = parse_config(
            "{\"dependencies\": [\"fmt\", {\"name\": \"boost-test\"}]}",
            "vcpkg.json",
        );
        let vcpkg_targets = config_targets(&vcpkg);
        assert!(vcpkg_targets.contains(&"cpp.dependency:fmt".to_string()));
        assert!(vcpkg_targets.contains(&"cpp.dependency:boost-test".to_string()));

        let conan = parse_config(
            "[requires]\nfmt/10.1.1\ngtest/1.14.0\n\n[options]\nfmt:shared=True\n",
            "conanfile.txt",
        );
        let conan_targets = config_targets(&conan);
        assert!(conan_targets.contains(&"cpp.dependency:fmt/10.1.1".to_string()));
        assert!(conan_targets.contains(&"cpp.dependency:gtest/1.14.0".to_string()));

        assert!(vcpkg
            .semantic_facts
            .iter()
            .chain(&conan.semantic_facts)
            .all(|fact| fact.kind == SemanticFactKind::ProjectConfig));
    }

    #[test]
    fn malformed_json_configs_emit_project_config_unknowns() {
        for path in ["compile_commands.json", "vcpkg.json"] {
            let report = parse_config("{ not json", path);
            assert_eq!(
                unknown_pairs(&report),
                vec![(
                    "MissingProjectConfig".to_string(),
                    "cpp_project_config".to_string(),
                )]
            );
            assert!(config_targets(&report).is_empty());
        }
    }
}
