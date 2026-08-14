//! Jakarta/Java Servlet `@WebServlet` recognition with dual `jakarta.servlet`
//! and `javax.servlet` roots.
//!
//! `@WebServlet` is the class-level anchor, and it is the whole claim. Which
//! URL patterns the container maps, how `web.xml` overrides them, how the
//! filter chain is composed, and whether the servlet is loaded on startup are
//! all deployment-descriptor and container behavior, so they stay typed
//! UNKNOWN. Nothing here reads `web.xml` or runs a container.

use super::{
    contains_annotation_simple_name, has_exact_direct_annotation, java_class_shape,
    java_visibility_shape, JavaImportContext,
};
use crate::core::model::CodeUnitKind;

const JAKARTA_SERVLET_PACKAGE: &str = "jakarta.servlet.annotation";
const JAVAX_SERVLET_PACKAGE: &str = "javax.servlet.annotation";
const SERVLET_ROOTS: &[&str] = &[JAKARTA_SERVLET_PACKAGE, JAVAX_SERVLET_PACKAGE];

const SERVLET_CLASS_ANNOTATIONS: &[&str] = &["WebServlet"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServletAnchor {
    pub(crate) kind: CodeUnitKind,
    pub(crate) target: &'static str,
    pub(crate) anchor_kind: &'static str,
    pub(crate) namespace_root: &'static str,
}

pub(crate) fn servlet_class_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<ServletAnchor> {
    has_exact_direct_annotation(annotation_text, "WebServlet", SERVLET_ROOTS, imports).then(|| {
        ServletAnchor {
            kind: CodeUnitKind::ServletHttpServlet,
            target: "servlet.annotation.WebServlet",
            anchor_kind: "servlet_http_servlet",
            namespace_root: namespace_root(annotation_text, "WebServlet", imports),
        }
    })
}

pub(crate) fn servlet_class_assumptions(
    anchor: &ServletAnchor,
    annotations: &str,
    slice: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        format!("java_anchor_kind={}", anchor.anchor_kind),
        format!("servlet_namespace_root={}", anchor.namespace_root),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_class_shape={}", java_class_shape(slice)),
    ]
}

pub(crate) fn contains_known_servlet_annotation_name(annotation_text: &str) -> bool {
    contains_annotation_simple_name(annotation_text, SERVLET_CLASS_ANNOTATIONS)
}

/// Which root spelled the annotation. Jakarta and javax servlets are the same
/// shape but different types, so their families must never cluster together.
fn namespace_root(
    annotation_text: &str,
    annotation: &str,
    imports: &JavaImportContext,
) -> &'static str {
    if annotation_text.contains(&format!("{JAKARTA_SERVLET_PACKAGE}.{annotation}"))
        || imports.has_import_for(annotation, &[JAKARTA_SERVLET_PACKAGE])
    {
        return "jakarta";
    }
    if annotation_text.contains(&format!("{JAVAX_SERVLET_PACKAGE}.{annotation}"))
        || imports.has_import_for(annotation, &[JAVAX_SERVLET_PACKAGE])
    {
        return "javax";
    }
    "jakarta"
}
