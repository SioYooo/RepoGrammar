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

pub(super) fn use_resolution_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    node: Node<'_>,
) -> Result<Vec<SemanticFact>, ParseError> {
    let use_text = node_text(document.text, node);
    match local_use_path(use_text) {
        UsePathResolution::Resolved { root, path, shape } => {
            Ok(vec![anchors::resolved_import_fact(
                document,
                unit,
                &format!("module:{path}"),
                vec![
                    "provider_resolved=false".to_string(),
                    "rust_anchor_kind=use_path".to_string(),
                    "rust_module_resolution=use_path".to_string(),
                    format!("rust_use_root={root}"),
                    format!("rust_use_path_shape={shape}"),
                ],
                "bounded Rust repo-local use path",
            )?])
        }
        UsePathResolution::UnresolvedLocal => Ok(vec![unknown::fact(
            document,
            unit,
            node.start_byte(),
            node.end_byte(),
            RustUnknownSpec {
                reason: "UnresolvedImport",
                affected_claim: "rust_module_resolution",
                kind: "unresolved_use_path",
                note: "Rust repo-local use path is not an exact crate/super/self path",
            },
        )?]),
        UsePathResolution::ExternalOrUnsupported => Ok(Vec::new()),
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
    // Match a `#[path = "..."]` attribute specifically. `path` must be the
    // attribute identifier immediately inside `#[`, not an arbitrary "path"
    // substring in another attribute or doc comment (e.g. `#[doc = "path = x"]`).
    let mut search = text;
    while let Some(open) = search.find("#[") {
        let inner = search[open + 2..].trim_start();
        if let Some(after_path) = inner.strip_prefix("path") {
            if !after_path
                .starts_with(|character: char| character.is_alphanumeric() || character == '_')
            {
                if let Some(rest) = after_path.trim_start().strip_prefix('=') {
                    return first_quoted(rest);
                }
            }
        }
        search = &search[open + 2..];
    }
    None
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

enum UsePathResolution {
    Resolved {
        root: &'static str,
        path: String,
        shape: &'static str,
    },
    UnresolvedLocal,
    ExternalOrUnsupported,
}

fn local_use_path(text: &str) -> UsePathResolution {
    let Some(mut path) = text
        .trim()
        .find("use ")
        .map(|index| text.trim()[index + "use ".len()..].trim())
    else {
        return UsePathResolution::ExternalOrUnsupported;
    };
    path = path.strip_suffix(';').unwrap_or(path).trim();
    let local_root = local_use_root(path);
    if path.contains('{')
        || path.contains('*')
        || path.contains(" as ")
        || path.contains('\n')
        || path.contains('\r')
    {
        return if local_root.is_some() {
            UsePathResolution::UnresolvedLocal
        } else {
            UsePathResolution::ExternalOrUnsupported
        };
    }
    let segments = path.split("::").collect::<Vec<_>>();
    let Some(root) = local_root else {
        return UsePathResolution::ExternalOrUnsupported;
    };
    if segments.len() < 2 || !segments.iter().all(|segment| rust_path_segment(segment)) {
        return UsePathResolution::UnresolvedLocal;
    }
    UsePathResolution::Resolved {
        root,
        path: segments.join("::"),
        shape: match root {
            "crate" => "absolute_crate",
            "super" => "relative_super",
            "self" => "relative_self",
            _ => "unknown",
        },
    }
}

fn local_use_root(path: &str) -> Option<&'static str> {
    let root = path.split("::").next().unwrap_or(path).trim();
    match root {
        "crate" => Some("crate"),
        "super" => Some("super"),
        "self" => Some("self"),
        _ => None,
    }
}

fn rust_path_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_attribute_value_matches_only_the_path_attribute() {
        assert_eq!(
            path_attribute_value("#[path = \"sys/foo.rs\"]\nmod foo;"),
            Some("sys/foo.rs".to_string())
        );
        assert_eq!(
            path_attribute_value("#[path=\"bar.rs\"] mod bar;"),
            Some("bar.rs".to_string())
        );
        // A `path` substring inside another attribute or doc comment must not be
        // read as a path attribute.
        assert_eq!(
            path_attribute_value("#[doc = \"path = not-a-real-path.rs\"]\nmod baz;"),
            None
        );
        assert_eq!(
            path_attribute_value("#[cfg(feature = \"pathfinder\")]\nmod qux;"),
            None
        );
        assert_eq!(path_attribute_value("mod plain;"), None);
    }
}
