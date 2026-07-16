//! JPA / Jakarta Persistence entity recognition with dual `jakarta.persistence`
//! and `javax.persistence` roots.
//!
//! `@Entity`, `@MappedSuperclass`, and `@Embeddable` are class-level anchors.
//! Field/mapping annotations (`@Id`, `@Column`, relationship annotations, ...)
//! become bounded shape assumptions, never separate units. Lazy proxies, naming
//! strategies, and `orm.xml` mapping remain typed UNKNOWN emitted by the scanner.

use super::{
    contains_annotation_simple_name, has_exact_annotation, has_exact_direct_annotation,
    java_class_shape, java_visibility_shape, JavaImportContext,
};
use crate::core::model::CodeUnitKind;

const JAKARTA_PERSISTENCE_PACKAGE: &str = "jakarta.persistence";
const JAVAX_PERSISTENCE_PACKAGE: &str = "javax.persistence";
const PERSISTENCE_ROOTS: &[&str] = &[JAKARTA_PERSISTENCE_PACKAGE, JAVAX_PERSISTENCE_PACKAGE];

const ENTITY_CLASS_ANNOTATIONS: &[&str] = &["Entity", "MappedSuperclass", "Embeddable"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntityAnchor {
    pub(crate) kind: CodeUnitKind,
    pub(crate) target: &'static str,
    pub(crate) anchor_kind: &'static str,
    pub(crate) namespace_root: &'static str,
}

pub(crate) fn entity_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<EntityAnchor> {
    for (annotation, kind, target, anchor_kind) in [
        (
            "Entity",
            CodeUnitKind::JpaEntity,
            "jpa.persistence.Entity",
            "jpa_entity",
        ),
        (
            "MappedSuperclass",
            CodeUnitKind::JpaMappedSuperclass,
            "jpa.persistence.MappedSuperclass",
            "jpa_mapped_superclass",
        ),
        (
            "Embeddable",
            CodeUnitKind::JpaEmbeddable,
            "jpa.persistence.Embeddable",
            "jpa_embeddable",
        ),
    ] {
        if has_exact_direct_annotation(annotation_text, annotation, PERSISTENCE_ROOTS, imports) {
            return Some(EntityAnchor {
                kind,
                target,
                anchor_kind,
                namespace_root: namespace_root(annotation_text, annotation, imports),
            });
        }
    }
    None
}

pub(crate) fn entity_shape_assumptions(
    anchor: &EntityAnchor,
    annotations: &str,
    slice: &str,
    imports: &JavaImportContext,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        format!("java_anchor_kind={}", anchor.anchor_kind),
        format!("jpa_namespace_root={}", anchor.namespace_root),
        format!("jpa_id_present={}", jpa_id_present(slice, imports)),
        format!(
            "jpa_relationship_shape={}",
            jpa_relationship_shape(slice, imports)
        ),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_class_shape={}", java_class_shape(slice)),
    ]
}

pub(crate) fn contains_known_entity_annotation_name(annotation_text: &str) -> bool {
    contains_annotation_simple_name(annotation_text, ENTITY_CLASS_ANNOTATIONS)
}

fn namespace_root(
    annotation_text: &str,
    annotation: &str,
    imports: &JavaImportContext,
) -> &'static str {
    if annotation_text.contains(&format!("{JAKARTA_PERSISTENCE_PACKAGE}.{annotation}"))
        || imports.has_import_for(annotation, &[JAKARTA_PERSISTENCE_PACKAGE])
    {
        return "jakarta";
    }
    if annotation_text.contains(&format!("{JAVAX_PERSISTENCE_PACKAGE}.{annotation}"))
        || imports.has_import_for(annotation, &[JAVAX_PERSISTENCE_PACKAGE])
    {
        return "javax";
    }
    "jakarta"
}

fn jpa_id_present(slice: &str, imports: &JavaImportContext) -> bool {
    has_exact_annotation(slice, "Id", PERSISTENCE_ROOTS, imports)
}

fn jpa_relationship_shape(slice: &str, imports: &JavaImportContext) -> &'static str {
    let to_one = has_exact_annotation(slice, "OneToOne", PERSISTENCE_ROOTS, imports)
        || has_exact_annotation(slice, "ManyToOne", PERSISTENCE_ROOTS, imports);
    let to_many = has_exact_annotation(slice, "OneToMany", PERSISTENCE_ROOTS, imports)
        || has_exact_annotation(slice, "ManyToMany", PERSISTENCE_ROOTS, imports);
    match (to_one, to_many) {
        (true, true) => "mixed",
        (true, false) => "to_one",
        (false, true) => "to_many",
        (false, false) => "none",
    }
}
