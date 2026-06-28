use super::unknown::RustUnknownSpec;
use super::{anchors, first_quoted, node_text, unknown};
use crate::core::model::{CodeUnit, SemanticFact};
use crate::ports::parser::{ParseError, ParserProjectContext, SourceDocument};
use std::collections::BTreeSet;
use tree_sitter::Node;

pub(super) fn resolution_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    context: &ParserProjectContext,
    module_name: &str,
    node: Node<'_>,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mod_text = document
        .text
        .get(unit.range.start_byte..node.end_byte())
        .unwrap_or_else(|| node_text(document.text, node));
    let Some(base) = module_base_path(document.path, module_name, mod_text) else {
        return Ok(vec![unknown::fact(
            document,
            unit,
            node.start_byte(),
            node.end_byte(),
            RustUnknownSpec {
                reason: "UnresolvedImport",
                affected_claim: "rust_module_resolution",
                kind: "unsafe_path_attribute",
                note: "Rust path attribute is outside bounded repo-relative resolution",
            },
        )?]);
    };
    let module_paths = context
        .rust_module_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut matches = BTreeSet::new();
    for candidate in [format!("{base}.rs"), format!("{base}/mod.rs")] {
        if module_paths.contains(&candidate) {
            matches.insert(candidate);
        }
    }
    match matches.len() {
        0 => Ok(vec![unknown::fact(
            document,
            unit,
            node.start_byte(),
            node.end_byte(),
            RustUnknownSpec {
                reason: "UnresolvedImport",
                affected_claim: "rust_module_resolution",
                kind: "unresolved_mod_decl",
                note: "Rust external mod declaration did not resolve to a unique repo-local file",
            },
        )?]),
        1 => {
            let path = matches.into_iter().next().expect("one Rust module match");
            Ok(vec![anchors::structural_anchor_fact(
                document,
                unit,
                &format!("module:{path}"),
                vec![
                    "provider_resolved=false".to_string(),
                    "rust_anchor_kind=module_resolution".to_string(),
                    "rust_module_resolution=external_mod".to_string(),
                ],
                "bounded Rust module resolution target",
            )?])
        }
        _ => Ok(vec![unknown::fact(
            document,
            unit,
            node.start_byte(),
            node.end_byte(),
            RustUnknownSpec {
                reason: "ConflictingFacts",
                affected_claim: "rust_module_resolution",
                kind: "ambiguous_mod_decl",
                note: "Rust external mod declaration has multiple repo-local candidates",
            },
        )?]),
    }
}

fn module_base_path(current_path: &str, module_name: &str, mod_text: &str) -> Option<String> {
    if let Some(path_value) = path_attribute_value(mod_text) {
        return safe_relative_module_path(current_path, &path_value);
    }
    let directory = if current_path.ends_with("/mod.rs") {
        current_path.trim_end_matches("/mod.rs").to_string()
    } else if let Some((directory, _)) = current_path.rsplit_once('/') {
        directory.to_string()
    } else {
        String::new()
    };
    Some(if directory.is_empty() {
        module_name.to_string()
    } else {
        format!("{directory}/{module_name}")
    })
}

fn path_attribute_value(text: &str) -> Option<String> {
    let marker = "path";
    let index = text.find(marker)?;
    let after = &text[index + marker.len()..];
    let equals = after.find('=')?;
    first_quoted(&after[equals + 1..])
}

fn safe_relative_module_path(current_path: &str, value: &str) -> Option<String> {
    if value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.split('/').any(|part| part == ".." || part.is_empty())
    {
        return None;
    }
    let directory = current_path
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or("");
    let path = if directory.is_empty() {
        value.to_string()
    } else {
        format!("{directory}/{value}")
    };
    Some(path.trim_end_matches(".rs").to_string())
}
