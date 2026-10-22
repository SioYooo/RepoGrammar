//! Language-neutral exact import binding.
//!
//! Decides whether a written name denotes an exact framework identity under
//! the lexical import scopes a lane extracts from its Tree-sitter tree. Lanes
//! supply only syntax specifications (data) and keep their own policy, such as
//! which import forms refuse a file. Nothing here resolves a package graph: an
//! identity is exact only when the source spells it in full, reaches it through
//! an explicit alias, or opens its namespace in an enclosing scope.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tree_sitter::Node;

/// How a lane spells qualified names.
#[derive(Debug)]
pub(crate) struct BindingSpec {
    pub separator: char,
    /// Whether a qualifier may spell the namespace in full (C#
    /// `System.Text.Json`); otherwise a qualifier is always a local binding
    /// (a Go package name).
    pub fully_qualified_names: bool,
    /// Accepted suffixes on the final segment; `""` is the bare name.
    pub name_suffixes: &'static [&'static str],
    /// Written names are cut at this character (type arguments) and trimmed.
    pub type_arguments: Option<char>,
}

impl BindingSpec {
    fn strip<'a>(&self, written: &'a str) -> &'a str {
        match self.type_arguments {
            Some(open) => written.split(open).next().unwrap_or(written).trim(),
            None => written,
        }
    }

    fn spells(&self, segment: &str, name: &str) -> bool {
        segment
            .strip_prefix(name)
            .is_some_and(|suffix| self.name_suffixes.contains(&suffix))
    }

    /// Whether the final segment of `written` spells `name`, bound or not.
    pub(crate) fn looks_like(&self, written: &str, name: &str) -> bool {
        let written = self.strip(written);
        let last = written.rsplit(self.separator).next().unwrap_or(written);
        self.spells(last, name)
    }
}

/// One lexical scope of opened namespaces and explicit aliases.
#[derive(Debug, Default)]
pub(crate) struct ImportScope {
    parent: Option<Arc<ImportScope>>,
    opened: BTreeSet<String>,
    aliases: BTreeMap<String, String>,
}

impl ImportScope {
    pub(crate) fn nested(parent: Arc<ImportScope>) -> Self {
        Self {
            parent: Some(parent),
            ..Self::default()
        }
    }

    pub(crate) fn open(&mut self, namespace: String) {
        self.opened.insert(namespace);
    }

    pub(crate) fn alias(&mut self, local: String, path: String) {
        self.aliases.insert(local, path);
    }

    fn scopes(&self) -> impl Iterator<Item = &ImportScope> {
        std::iter::successors(Some(self), |scope| scope.parent.as_deref())
    }

    /// Whether `written` denotes `namespace` + separator + `name`: spelled in
    /// full where the spec allows it, qualified by an alias of `namespace`, or
    /// bare while an enclosing scope opens `namespace`.
    pub(crate) fn denotes(
        &self,
        spec: &BindingSpec,
        written: &str,
        name: &str,
        namespace: &str,
    ) -> bool {
        let written = spec.strip(written);
        match written.rsplit_once(spec.separator) {
            Some((qualifier, last)) => {
                spec.spells(last, name)
                    && ((spec.fully_qualified_names && qualifier == namespace)
                        || self.scopes().any(|scope| {
                            scope.aliases.get(qualifier).map(String::as_str) == Some(namespace)
                        }))
            }
            None => {
                spec.spells(written, name)
                    && self.scopes().any(|scope| scope.opened.contains(namespace))
            }
        }
    }

    pub(crate) fn annotation_is_exact(
        &self,
        spec: &BindingSpec,
        annotation: &Annotation,
        name: &str,
        namespace: &str,
    ) -> bool {
        !annotation.parse_degraded && self.denotes(spec, &annotation.name, name, namespace)
    }

    pub(crate) fn exact_annotation<'a>(
        &self,
        spec: &BindingSpec,
        annotations: &'a [Annotation],
        name: &str,
        namespace: &str,
    ) -> Option<&'a Annotation> {
        annotations
            .iter()
            .find(|annotation| self.annotation_is_exact(spec, annotation, name, namespace))
    }

    /// Whether an annotation spells one of `known` `(name, namespace)` pairs
    /// without exact binding: a lookalike that must not anchor.
    pub(crate) fn has_unbound_lookalike(
        &self,
        spec: &BindingSpec,
        annotations: &[Annotation],
        known: &[(&str, &str)],
    ) -> bool {
        annotations.iter().any(|annotation| {
            known.iter().any(|(name, namespace)| {
                spec.looks_like(&annotation.name, name)
                    && !self.annotation_is_exact(spec, annotation, name, namespace)
            })
        })
    }

    pub(crate) fn any_denotes(
        &self,
        spec: &BindingSpec,
        written: &[String],
        name: &str,
        namespace: &str,
    ) -> bool {
        written
            .iter()
            .any(|written| self.denotes(spec, written, name, namespace))
    }
}

/// What binds a declaration to a framework role.
#[derive(Debug)]
pub(crate) enum RoleEvidence {
    /// An exactly bound annotation (attribute, decorator).
    Annotation {
        name: &'static str,
        namespace: &'static str,
    },
    /// An exactly bound base type.
    BaseType {
        name: &'static str,
        namespace: &'static str,
    },
}

impl RoleEvidence {
    fn name(&self) -> &'static str {
        match self {
            Self::Annotation { name, .. } | Self::BaseType { name, .. } => name,
        }
    }
}

/// One framework role a declaration may take. Rows are tried in order and the
/// first match wins, so row order is the lane's role precedence.
#[derive(Debug)]
pub(crate) struct RoleRow<R: 'static> {
    /// The lane's description of the role (unit kind, assumptions, UNKNOWNs).
    pub role: R,
    /// Declaration node kinds the role applies to; empty means any.
    pub declaration_kinds: &'static [&'static str],
    /// Alternative evidence in precedence order, each with the target it proves.
    pub evidence: &'static [(RoleEvidence, &'static str)],
}

/// The row a declaration matched, with the target and evidence name it proved.
pub(crate) struct RoleMatch<'r, R: 'static> {
    pub row: &'r RoleRow<R>,
    pub target: &'static str,
    pub evidence: &'static str,
}

impl ImportScope {
    /// The first row whose evidence binds exactly on this declaration.
    pub(crate) fn first_role<'r, R>(
        &self,
        rows: &'r [RoleRow<R>],
        declaration_kind: &str,
        annotations: (&BindingSpec, &[Annotation]),
        base_types: (&BindingSpec, &[String]),
    ) -> Option<RoleMatch<'r, R>> {
        rows.iter()
            .filter(|row| {
                row.declaration_kinds.is_empty()
                    || row.declaration_kinds.contains(&declaration_kind)
            })
            .find_map(|row| {
                row.evidence.iter().find_map(|(evidence, target)| {
                    let bound = match evidence {
                        RoleEvidence::Annotation { name, namespace } => self
                            .exact_annotation(annotations.0, annotations.1, name, namespace)
                            .is_some(),
                        RoleEvidence::BaseType { name, namespace } => {
                            self.any_denotes(base_types.0, base_types.1, name, namespace)
                        }
                    };
                    bound.then_some(RoleMatch {
                        row,
                        target,
                        evidence: evidence.name(),
                    })
                })
            })
    }
}

/// How a lane writes a namespace-opening import directive.
#[derive(Debug)]
pub(crate) struct DirectiveSpec {
    pub kinds: &'static [&'static str],
    /// Optional leading modifier (`global ` in C#).
    pub prefix: &'static str,
    pub keyword: &'static str,
    /// Directive forms that open nothing exact, by leading text.
    pub excluded_forms: &'static [&'static str],
    /// Characters whose presence marks an alias or call form.
    pub excluded_characters: &'static [char],
    pub terminator: char,
    /// Container kinds whose children also belong to the enclosing scope.
    pub body_kinds: &'static [&'static str],
}

/// The namespace an exact directive opens, from its text.
fn opened_namespace(spec: &DirectiveSpec, binding: &BindingSpec, text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix(spec.prefix).unwrap_or(text);
    let rest = text.strip_prefix(spec.keyword)?;
    if spec
        .excluded_forms
        .iter()
        .any(|form| rest.starts_with(form))
        || rest.contains(spec.excluded_characters)
    {
        return None;
    }
    let namespace = rest.trim_end_matches(spec.terminator).trim();
    (!namespace.is_empty()
        && namespace.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == binding.separator
        }))
    .then(|| namespace.to_string())
}

/// The import scope a scope node opens: directives among its children and the
/// children of its body containers, nested in `parent`.
pub(crate) fn scope_imports(
    source: &str,
    scope: Node<'_>,
    parent: Arc<ImportScope>,
    directives: &DirectiveSpec,
    binding: &BindingSpec,
) -> ImportScope {
    let mut imports = ImportScope::nested(parent);
    let mut record = |node: Node<'_>| {
        if directives.kinds.contains(&node.kind()) {
            if let Some(namespace) = source
                .get(node.byte_range())
                .and_then(|text| opened_namespace(directives, binding, text))
            {
                imports.open(namespace);
            }
        }
    };
    let mut cursor = scope.walk();
    for child in scope.named_children(&mut cursor) {
        record(child);
        if directives.body_kinds.contains(&child.kind()) {
            let mut body_cursor = child.walk();
            for body_child in child.named_children(&mut body_cursor) {
                record(body_child);
            }
        }
    }
    imports
}

/// One attribute, annotation or decorator as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Annotation {
    pub name: String,
    pub arguments: Option<String>,
    pub parse_degraded: bool,
}

/// Where a lane's grammar puts annotations on a declaration.
#[derive(Debug)]
pub(crate) struct AnnotationSpec {
    pub list_kind: &'static str,
    pub item_kind: &'static str,
    pub name_field: &'static str,
    pub arguments_kind: &'static str,
}

/// The annotations directly attached to `declaration`, in source order.
pub(crate) fn annotations(
    source: &str,
    declaration: Node<'_>,
    spec: &AnnotationSpec,
) -> Vec<Annotation> {
    let mut found = Vec::new();
    let mut list_cursor = declaration.walk();
    for list in declaration.named_children(&mut list_cursor) {
        if list.kind() != spec.list_kind {
            continue;
        }
        let mut item_cursor = list.walk();
        for item in list.named_children(&mut item_cursor) {
            if item.kind() != spec.item_kind {
                continue;
            }
            let Some(name) = item
                .child_by_field_name(spec.name_field)
                .and_then(|name| source.get(name.byte_range()))
            else {
                continue;
            };
            let mut argument_cursor = item.walk();
            let arguments = item
                .named_children(&mut argument_cursor)
                .find(|candidate| candidate.kind() == spec.arguments_kind)
                .map(|arguments| source[arguments.byte_range()].to_string());
            found.push(Annotation {
                name: name.to_string(),
                arguments,
                parse_degraded: item.has_error(),
            });
        }
    }
    found
}

/// The texts of the named children of `declaration`'s `list_kind` children,
/// such as a base-type list.
pub(crate) fn listed_names(source: &str, declaration: Node<'_>, list_kind: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = declaration.walk();
    for list in declaration.named_children(&mut cursor) {
        if list.kind() != list_kind {
            continue;
        }
        let mut item_cursor = list.walk();
        for item in list.named_children(&mut item_cursor) {
            names.push(source[item.byte_range()].to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOTTED: BindingSpec = BindingSpec {
        separator: '.',
        fully_qualified_names: true,
        name_suffixes: &["", "Attribute"],
        type_arguments: None,
    };
    const TYPES: BindingSpec = BindingSpec {
        separator: '.',
        fully_qualified_names: true,
        name_suffixes: &[""],
        type_arguments: Some('<'),
    };

    fn scope_opening(namespace: &str) -> ImportScope {
        let mut root = ImportScope::default();
        root.open(namespace.to_string());
        ImportScope::nested(Arc::new(root))
    }

    #[test]
    fn exact_names_need_a_full_spelling_an_alias_or_an_opened_namespace() {
        let opened = scope_opening("Web.Mvc");
        let closed = ImportScope::default();
        assert!(opened.denotes(&DOTTED, "HttpGet", "HttpGet", "Web.Mvc"));
        assert!(opened.denotes(&DOTTED, "HttpGetAttribute", "HttpGet", "Web.Mvc"));
        assert!(!closed.denotes(&DOTTED, "HttpGet", "HttpGet", "Web.Mvc"));
        assert!(closed.denotes(&DOTTED, "Web.Mvc.HttpGet", "HttpGet", "Web.Mvc"));
        assert!(!closed.denotes(&DOTTED, "Other.Mvc.HttpGet", "HttpGet", "Web.Mvc"));
        assert!(!opened.denotes(&DOTTED, "HttpGetter", "HttpGet", "Web.Mvc"));
        let mut aliased = ImportScope::default();
        aliased.alias("tt".to_string(), "testing".to_string());
        assert!(aliased.denotes(&TYPES, "tt.T", "T", "testing"));
        assert!(!aliased.denotes(&TYPES, "testing.T", "T", "other"));
        let local_only = BindingSpec {
            fully_qualified_names: false,
            ..TYPES
        };
        assert!(aliased.denotes(&local_only, "tt.T", "T", "testing"));
        assert!(!aliased.denotes(&local_only, "testing.T", "T", "testing"));
    }

    #[test]
    fn type_arguments_are_cut_before_resolution_and_lookalikes_are_detected() {
        let opened = scope_opening("Data");
        assert!(opened.denotes(&TYPES, "Context<T> ", "Context", "Data"));
        assert!(opened.denotes(&TYPES, "Data.Context<T>", "Context", "Data"));
        assert!(TYPES.looks_like("Some.Other.Context<T>", "Context"));
        assert!(DOTTED.looks_like("Fake.TestAttribute", "Test"));
        assert!(!DOTTED.looks_like("Testing", "Test"));
        let lookalike = Annotation {
            name: "Fact".to_string(),
            arguments: None,
            parse_degraded: false,
        };
        let closed = ImportScope::default();
        assert!(closed.has_unbound_lookalike(
            &DOTTED,
            std::slice::from_ref(&lookalike),
            &[("Fact", "Xunit")]
        ));
        assert!(!scope_opening("Xunit").has_unbound_lookalike(
            &DOTTED,
            &[lookalike],
            &[("Fact", "Xunit")]
        ));
    }

    #[test]
    fn degraded_annotations_never_bind_exactly() {
        let degraded = Annotation {
            name: "Fact".to_string(),
            arguments: None,
            parse_degraded: true,
        };
        let opened = scope_opening("Xunit");
        assert!(!opened.annotation_is_exact(&DOTTED, &degraded, "Fact", "Xunit"));
        assert!(opened.has_unbound_lookalike(&DOTTED, &[degraded], &[("Fact", "Xunit")]));
    }

    #[test]
    fn directive_text_opens_only_exact_namespaces() {
        let spec = DirectiveSpec {
            kinds: &["using_directive"],
            prefix: "global ",
            keyword: "using ",
            excluded_forms: &["static "],
            excluded_characters: &['=', '('],
            terminator: ';',
            body_kinds: &[],
        };
        assert_eq!(
            opened_namespace(&spec, &DOTTED, "using A.B;"),
            Some("A.B".to_string())
        );
        assert_eq!(
            opened_namespace(&spec, &DOTTED, " global using A_1;"),
            Some("A_1".to_string())
        );
        for excluded in [
            "using static A.B;",
            "using X = A.B;",
            "using A.B /*c*/;",
            "using ;",
            "import A;",
        ] {
            assert_eq!(
                opened_namespace(&spec, &DOTTED, excluded),
                None,
                "{excluded}"
            );
        }
    }

    #[test]
    fn the_first_bound_row_and_evidence_win() {
        const ROWS: &[RoleRow<&str>] = &[
            RoleRow {
                role: "controller",
                declaration_kinds: &[],
                evidence: &[
                    (
                        RoleEvidence::Annotation {
                            name: "ApiController",
                            namespace: "Web.Mvc",
                        },
                        "api",
                    ),
                    (
                        RoleEvidence::BaseType {
                            name: "Controller",
                            namespace: "Web.Mvc",
                        },
                        "base",
                    ),
                ],
            },
            RoleRow {
                role: "context",
                declaration_kinds: &["class_declaration"],
                evidence: &[(
                    RoleEvidence::BaseType {
                        name: "DbContext",
                        namespace: "Data",
                    },
                    "db",
                )],
            },
        ];
        let annotation = |name: &str| Annotation {
            name: name.to_string(),
            arguments: None,
            parse_degraded: false,
        };
        let opened = scope_opening("Web.Mvc");
        let role = |kind: &str, annotations: &[Annotation], bases: &[&str]| {
            let bases = bases.iter().map(ToString::to_string).collect::<Vec<_>>();
            opened
                .first_role(ROWS, kind, (&DOTTED, annotations), (&TYPES, &bases))
                .map(|found| (found.row.role, found.target, found.evidence))
        };
        let api = [annotation("ApiController")];
        assert_eq!(
            role(
                "struct_declaration",
                &api,
                &["Controller", "Data.DbContext"]
            ),
            Some(("controller", "api", "ApiController"))
        );
        assert_eq!(
            role("class_declaration", &[], &["Controller<T>"]),
            Some(("controller", "base", "Controller"))
        );
        assert_eq!(
            role("class_declaration", &[], &["Data.DbContext"]),
            Some(("context", "db", "DbContext"))
        );
        assert_eq!(role("record_declaration", &[], &["Data.DbContext"]), None);
        assert_eq!(role("class_declaration", &[], &["DbContext"]), None);
    }
}
