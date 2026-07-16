use super::unknown::RustUnknownSpec;
use super::{anchors, lines_with_offsets, toml_key_value, toml_string, unknown};
use crate::adapters::parsing::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Language, Provenance, SemanticFact, SourceRange, SymbolId,
};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument};

pub(super) fn project_config_report(
    document: SourceDocument<'_>,
) -> Result<ParseReport, ParseError> {
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
        language: Language::RustConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    };
    let mut facts = cargo_toml_facts(&document, &unit)?;
    facts.sort_by(|left, right| {
        (
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
        )
            .cmp(&(
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
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

fn cargo_toml_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mut facts = Vec::new();
    let mut section = "";
    facts.push(anchors::project_config_fact(
        document,
        unit,
        "cargo.toml",
        vec!["rust_project_config=cargo_toml".to_string()],
        "bounded Cargo.toml metadata",
    )?);
    for (start, line) in lines_with_offsets(document.text) {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(['[', ']']);
            if section.starts_with("target.") {
                facts.push(unknown::project_config_unknown_fact(
                    document,
                    unit,
                    start,
                    start + line.len(),
                    RustUnknownSpec {
                        reason: "BuildVariantAmbiguity",
                        affected_claim: "rust_build_variant",
                        kind: "target_specific_config",
                        note: "target-specific Cargo config is not evaluated",
                    },
                )?);
            }
            continue;
        }
        if trimmed.starts_with("build") && trimmed.contains('=') && section == "package" {
            facts.push(unknown::project_config_unknown_fact(
                document,
                unit,
                start,
                start + line.len(),
                RustUnknownSpec {
                    reason: "BuildVariantAmbiguity",
                    affected_claim: "rust_build_variant",
                    kind: "build_script",
                    note: "Cargo build script is not executed",
                },
            )?);
        }
        let Some((key, value)) = toml_key_value(trimmed) else {
            continue;
        };
        match section {
            "package" if key == "name" => {
                if let Some(name) = toml_string(value) {
                    facts.push(anchors::project_config_fact(
                        document,
                        unit,
                        &format!("cargo.package:{name}"),
                        vec!["rust_project_config=package".to_string()],
                        "bounded Cargo package metadata",
                    )?);
                }
            }
            "package" if key == "edition" => {
                if let Some(edition) = toml_string(value) {
                    facts.push(anchors::project_config_fact(
                        document,
                        unit,
                        &format!("cargo.edition:{edition}"),
                        vec!["rust_project_config=edition".to_string()],
                        "bounded Cargo edition metadata",
                    )?);
                }
            }
            _ if target_section_kind(section).is_some() && key == "name" => {
                if let Some(name) = toml_string(value) {
                    let target_kind = target_section_kind(section).expect("checked target section");
                    facts.push(anchors::project_config_fact(
                        document,
                        unit,
                        &format!("cargo.target:{target_kind}:{name}"),
                        vec![
                            "rust_project_config=package_target".to_string(),
                            format!("cargo_target_kind={target_kind}"),
                        ],
                        "bounded Cargo target metadata",
                    )?);
                }
            }
            _ if target_section_kind(section).is_some() && key == "path" => {
                if let Some(path) = toml_string(value).filter(|path| safe_cargo_path(path)) {
                    let target_kind = target_section_kind(section).expect("checked target section");
                    facts.push(anchors::project_config_fact(
                        document,
                        unit,
                        &format!("cargo.crate_root:{path}"),
                        vec![
                            "rust_project_config=crate_root".to_string(),
                            format!("cargo_target_kind={target_kind}"),
                        ],
                        "bounded Cargo crate root metadata",
                    )?);
                }
            }
            _ if is_cargo_dependency_section(section) => {
                facts.push(anchors::project_config_fact(
                    document,
                    unit,
                    &format!("cargo.dependency:{key}"),
                    vec![
                        "rust_project_config=dependency".to_string(),
                        format!("dependency_section={section}"),
                    ],
                    "bounded Cargo dependency metadata",
                )?);
            }
            "features" => facts.push(anchors::project_config_fact(
                document,
                unit,
                &format!("cargo.feature:{key}"),
                vec!["rust_project_config=feature".to_string()],
                "bounded Cargo feature metadata",
            )?),
            "workspace" if key == "members" => facts.push(anchors::project_config_fact(
                document,
                unit,
                "cargo.workspace:members",
                vec!["rust_project_config=workspace_members".to_string()],
                "bounded Cargo workspace metadata",
            )?),
            _ => {}
        }
    }

    Ok(facts)
}

fn target_section_kind(section: &str) -> Option<&'static str> {
    match section {
        "lib" => Some("lib"),
        "bin" => Some("bin"),
        "test" => Some("test"),
        "bench" => Some("bench"),
        _ => None,
    }
}

fn safe_cargo_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn is_cargo_dependency_section(section: &str) -> bool {
    matches!(
        section,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    ) || section.ends_with(".dependencies")
        || section.ends_with(".dev-dependencies")
        || section.ends_with(".build-dependencies")
}
