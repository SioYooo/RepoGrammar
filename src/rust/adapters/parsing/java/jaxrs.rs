//! JAX-RS / Jakarta REST resource recognition with dual `jakarta.ws.rs` and
//! `javax.ws.rs` roots.
//!
//! A class annotated with an exact `@Path` is a resource class; a method with an
//! exact verb annotation (`@GET`/`@POST`/...) *inside* an exact `@Path` class is a
//! resource method. A verb annotation outside a `@Path` class is a blocking
//! identity UNKNOWN, mirroring the Spring "route outside controller" rule. Path
//! shape reuses the shared literal/dynamic/none classifier.

use super::{
    annotation_segment_exact, contains_annotation_simple_name, has_exact_direct_annotation,
    java_parameter_shape, java_return_shape, java_visibility_shape, route_path_shape,
    JavaImportContext,
};

const JAKARTA_WS_RS_PACKAGE: &str = "jakarta.ws.rs";
const JAVAX_WS_RS_PACKAGE: &str = "javax.ws.rs";
const WS_RS_ROOTS: &[&str] = &[JAKARTA_WS_RS_PACKAGE, JAVAX_WS_RS_PACKAGE];

const VERB_ANNOTATIONS: &[(&str, &str)] = &[
    ("GET", "jaxrs.ws.rs.GET"),
    ("POST", "jaxrs.ws.rs.POST"),
    ("PUT", "jaxrs.ws.rs.PUT"),
    ("DELETE", "jaxrs.ws.rs.DELETE"),
    ("PATCH", "jaxrs.ws.rs.PATCH"),
    ("HEAD", "jaxrs.ws.rs.HEAD"),
    ("OPTIONS", "jaxrs.ws.rs.OPTIONS"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceClassAnchor {
    pub(crate) target: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceMethodAnchor {
    pub(crate) target: &'static str,
    pub(crate) http_method: &'static str,
    pub(crate) route_path_shape: &'static str,
}

pub(crate) fn resource_class_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<ResourceClassAnchor> {
    has_exact_direct_annotation(annotation_text, "Path", WS_RS_ROOTS, imports).then_some(
        ResourceClassAnchor {
            target: "jaxrs.ws.rs.Path",
        },
    )
}

pub(crate) fn resource_method_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<ResourceMethodAnchor> {
    for (verb, target) in VERB_ANNOTATIONS {
        if has_exact_direct_annotation(annotation_text, verb, WS_RS_ROOTS, imports) {
            return Some(ResourceMethodAnchor {
                target,
                http_method: verb,
                route_path_shape: method_path_shape(annotation_text, imports),
            });
        }
    }
    None
}

pub(crate) fn resource_class_assumptions(
    _anchor: &ResourceClassAnchor,
    annotations: &str,
    slice: &str,
    imports: &JavaImportContext,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        "java_anchor_kind=jaxrs_resource_class".to_string(),
        format!(
            "class_route_path_shape={}",
            class_path_shape(annotations, imports)
        ),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_class_shape={}", super::java_class_shape(slice)),
    ]
}

pub(crate) fn resource_method_assumptions(
    anchor: &ResourceMethodAnchor,
    class_route_path_shape: &str,
    annotations: &str,
    slice: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        "java_anchor_kind=jaxrs_resource_method".to_string(),
        format!("http_method={}", anchor.http_method),
        format!("route_path_shape={}", anchor.route_path_shape),
        format!("class_route_path_shape={class_route_path_shape}"),
        format!(
            "java_visibility_shape={}",
            java_visibility_shape(annotations)
        ),
        format!("java_return_shape={}", java_return_shape(slice)),
        format!("java_parameter_shape={}", java_parameter_shape(slice)),
    ]
}

pub(crate) fn class_path_shape(annotation_text: &str, imports: &JavaImportContext) -> &'static str {
    method_path_shape(annotation_text, imports)
}

fn method_path_shape(annotation_text: &str, imports: &JavaImportContext) -> &'static str {
    annotation_segment_exact(annotation_text, "Path", WS_RS_ROOTS, imports)
        .as_deref()
        .map(route_path_shape)
        .unwrap_or("none")
}

pub(crate) fn contains_known_verb_annotation_name(annotation_text: &str) -> bool {
    let names = VERB_ANNOTATIONS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    contains_annotation_simple_name(annotation_text, &names)
}

pub(crate) fn contains_known_path_annotation_name(annotation_text: &str) -> bool {
    contains_annotation_simple_name(annotation_text, &["Path"])
}
