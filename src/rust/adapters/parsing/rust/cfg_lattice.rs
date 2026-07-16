use super::unknown::{self, RustUnknownSpec};
use super::{first_quoted, lines_with_offsets, toml_key_value};
use crate::core::model::{CodeUnit, SemanticFact};
use crate::ports::parser::{
    ParseError, ParserProjectContext, ParserProjectFileContext, SourceDocument,
};
use std::collections::BTreeSet;

pub(super) fn macro_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    kind: &'static str,
) -> Result<SemanticFact, ParseError> {
    unknown::fact(
        document,
        unit,
        start_byte,
        end_byte,
        RustUnknownSpec {
            reason: "MacroOrPreprocessor",
            affected_claim: "rust_macro_expansion",
            kind,
            note: "Rust macro syntax is not expanded",
        },
    )
}

pub(super) fn unit_unknowns(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    slice: &str,
    context: &ParserProjectContext,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mut facts = Vec::new();
    let cfg_attributes = cfg_attribute_slices(slice);
    if !cfg_attributes.is_empty() {
        facts.push(unknown::fact_with_assumptions(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "BuildVariantAmbiguity",
                affected_claim: "rust_build_variant",
                kind: "cfg_attribute",
                note: "Rust cfg/cfg_attr build variant is recorded with bounded Cargo context but not evaluated",
            },
            cfg_assumptions(document.path, &cfg_attributes, context),
        )?);
    }
    if slice.contains("#[proc_macro")
        || slice.contains("#[proc_macro_attribute")
        || slice.contains("#[proc_macro_derive")
    {
        facts.push(unknown::fact(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "MacroOrPreprocessor",
                affected_claim: "rust_macro_expansion",
                kind: "proc_macro_attribute",
                note: "Rust procedural macro attribute is not expanded",
            },
        )?);
    }
    if slice.contains("dyn ") {
        facts.push(unknown::fact_with_assumptions(
            document,
            unit,
            unit.range.start_byte,
            unit.range.end_byte,
            RustUnknownSpec {
                reason: "FrameworkMagic",
                affected_claim: "rust_trait_dispatch",
                kind: "trait_dispatch",
                note: "Rust trait object dispatch is not resolved",
            },
            trait_dispatch_assumptions(slice),
        )?);
    }
    Ok(facts)
}

/// Bounded, sound enrichment for a trait-object dispatch UNKNOWN: record the
/// syntactic trait name(s) that appear after `dyn`. This names which trait is
/// dispatched (a syntactic fact) without resolving the concrete target, which
/// would require name/type resolution and impl-coherence a Tree-sitter substring
/// scan cannot provide soundly. The dispatch claim itself stays UNKNOWN.
fn trait_dispatch_assumptions(slice: &str) -> Vec<String> {
    let mut assumptions = Vec::new();
    let mut traits = BTreeSet::new();
    let mut cursor = 0usize;
    while let Some(relative) = slice[cursor..].find("dyn ") {
        let start = cursor + relative + "dyn ".len();
        let name: String = slice[start..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || *character == '_' || *character == ':'
            })
            .collect();
        let name = name.trim_matches(':');
        if !name.is_empty() {
            traits.insert(assumption_value(name));
        }
        cursor = start;
    }
    for trait_name in traits {
        assumptions.push(format!("rust_trait_dispatch_trait={trait_name}"));
    }
    assumptions.sort();
    assumptions.dedup();
    assumptions
}

fn cfg_assumptions(
    source_path: &str,
    cfg_attributes: &[&str],
    context: &ParserProjectContext,
) -> Vec<String> {
    let mut assumptions = vec!["rust_cfg_model=cargo_feature_cfg_model".to_string()];
    let features = cfg_feature_names(cfg_attributes);
    if !features.is_empty() {
        assumptions.push("rust_cfg_predicate=feature".to_string());
        match nearest_cargo_manifest(context, source_path) {
            Some(manifest) => {
                assumptions.push(format!(
                    "rust_cfg_manifest={}",
                    assumption_value(&manifest.path)
                ));
                let declared = cargo_feature_names(&manifest.text);
                for feature in features {
                    let feature = assumption_value(&feature);
                    let state = if declared.contains(&feature) {
                        "true"
                    } else {
                        "false"
                    };
                    assumptions.push(format!("rust_cfg_feature={feature}"));
                    assumptions.push(format!("rust_cfg_feature_declared={feature}:{state}"));
                }
            }
            None => {
                assumptions.push("rust_cfg_manifest=unknown".to_string());
                for feature in features {
                    let feature = assumption_value(&feature);
                    assumptions.push(format!("rust_cfg_feature={feature}"));
                    assumptions.push(format!("rust_cfg_feature_declared={feature}:unknown"));
                }
            }
        }
    } else if cfg_attributes
        .iter()
        .any(|attribute| target_cfg_attribute(attribute))
    {
        assumptions.push("rust_cfg_predicate=target".to_string());
    } else {
        assumptions.push("rust_cfg_predicate=complex".to_string());
    }
    assumptions.sort();
    assumptions.dedup();
    assumptions
}

fn cfg_attribute_slices(slice: &str) -> Vec<&str> {
    let mut attributes = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = slice[cursor..].find("#[cfg") {
        let start = cursor + relative_start;
        let end = slice[start..]
            .find(']')
            .map(|relative_end| start + relative_end + 1)
            .unwrap_or(slice.len());
        attributes.push(&slice[start..end]);
        if end >= slice.len() {
            break;
        }
        cursor = end;
    }
    attributes
}

fn cfg_feature_names(cfg_attributes: &[&str]) -> BTreeSet<String> {
    let mut features = BTreeSet::new();
    for attribute in cfg_attributes {
        let mut cursor = 0usize;
        while let Some(relative_start) = attribute[cursor..].find("feature") {
            let start = cursor + relative_start;
            let end = start + "feature".len();
            if !identifier_boundary(attribute, start, end) {
                cursor = end;
                continue;
            }
            let after = attribute[end..].trim_start();
            if !after.starts_with('=') {
                cursor = end;
                continue;
            }
            if let Some(feature) = first_quoted(&after[1..]) {
                features.insert(assumption_value(&feature));
            }
            cursor = end;
        }
    }
    features
}

fn identifier_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        && !after.is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn target_cfg_attribute(attribute: &str) -> bool {
    [
        "target_",
        "unix",
        "windows",
        "debug_assertions",
        "test",
        "doc",
        "proc_macro",
    ]
    .iter()
    .any(|marker| attribute.contains(marker))
}

fn nearest_cargo_manifest<'a>(
    context: &'a ParserProjectContext,
    source_path: &str,
) -> Option<&'a ParserProjectFileContext> {
    context
        .rust_cargo_files
        .iter()
        .filter_map(|manifest| {
            let directory = manifest_directory(&manifest.path)?;
            if path_is_under_manifest_directory(source_path, directory) {
                Some((directory.len(), manifest))
            } else {
                None
            }
        })
        .max_by_key(|(directory_len, _)| *directory_len)
        .map(|(_, manifest)| manifest)
}

fn manifest_directory(path: &str) -> Option<&str> {
    path.strip_suffix("Cargo.toml")
        .map(|directory| directory.trim_end_matches('/'))
}

fn path_is_under_manifest_directory(path: &str, directory: &str) -> bool {
    directory.is_empty()
        || path
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn cargo_feature_names(manifest_text: &str) -> BTreeSet<String> {
    let mut section = "";
    let mut features = BTreeSet::new();
    for (_, line) in lines_with_offsets(manifest_text) {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.trim_matches(['[', ']']);
            continue;
        }
        if section != "features" {
            continue;
        }
        if let Some((key, _)) = toml_key_value(trimmed) {
            features.insert(assumption_value(key));
        }
    }
    features
}

fn assumption_value(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|character| {
            !character.is_control() && !character.is_whitespace() && *character != '='
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::trait_dispatch_assumptions;

    #[test]
    fn trait_dispatch_records_syntactic_trait_names_only() {
        // The dispatched trait names are recorded as bounded syntactic context;
        // the concrete target is not resolved (that needs name/type resolution).
        let assumptions = trait_dispatch_assumptions(
            "fn run(gate: &dyn FamilyGate, other: Box<dyn crate::model::Rule + Send>) {}",
        );
        assert!(assumptions.contains(&"rust_trait_dispatch_trait=FamilyGate".to_string()));
        assert!(assumptions.contains(&"rust_trait_dispatch_trait=crate::model::Rule".to_string()));
        // No impl-set / target claim is asserted — only the trait names appear.
        assert!(assumptions
            .iter()
            .all(|assumption| assumption.starts_with("rust_trait_dispatch_trait=")));
    }

    #[test]
    fn trait_dispatch_without_named_trait_records_nothing() {
        assert!(trait_dispatch_assumptions("let x = 1;").is_empty());
    }
}
