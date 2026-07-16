//! Spring MVC / stereotype / Boot / Spring Data recognition and UNKNOWN recipes.
//!
//! All anchors gate on an exact imported simple name (or a wildcard import of the
//! exact allowed package) or an inline fully-qualified name. Runtime behavior
//! (component scan, dependency injection, AOP proxies, generated repository
//! implementations, derived-query property paths) is emitted as typed UNKNOWN by
//! the scanner, never simulated.

use super::{
    annotation_segment_exact, contains_annotation_simple_name, extends_exact_type,
    has_exact_direct_annotation, java_parameter_shape, java_return_shape, java_visibility_shape,
    route_path_shape, JavaImportContext,
};

pub(crate) const ROUTE_MAPPING_ANNOTATIONS: &[(&str, &str, &str)] = &[
    (
        "RequestMapping",
        "spring.web.bind.annotation.RequestMapping",
        "REQUEST",
    ),
    ("GetMapping", "spring.web.bind.annotation.GetMapping", "GET"),
    (
        "PostMapping",
        "spring.web.bind.annotation.PostMapping",
        "POST",
    ),
    ("PutMapping", "spring.web.bind.annotation.PutMapping", "PUT"),
    (
        "PatchMapping",
        "spring.web.bind.annotation.PatchMapping",
        "PATCH",
    ),
    (
        "DeleteMapping",
        "spring.web.bind.annotation.DeleteMapping",
        "DELETE",
    ),
];

const SPRING_WEB_BIND_ANNOTATION_PACKAGE: &str = "org.springframework.web.bind.annotation";
const SPRING_STEREOTYPE_PACKAGE: &str = "org.springframework.stereotype";
const SPRING_BOOT_AUTOCONFIGURE_PACKAGE: &str = "org.springframework.boot.autoconfigure";
const SPRING_DATA_JPA_PACKAGE: &str = "org.springframework.data.jpa.repository";
const SPRING_DATA_REPOSITORY_PACKAGE: &str = "org.springframework.data.repository";

const SPRING_CLASS_ANNOTATION_NAMES: &[&str] = &[
    "SpringBootApplication",
    "RestController",
    "Controller",
    "Service",
    "Repository",
    "Component",
    "RepositoryDefinition",
];

const DERIVED_QUERY_PREFIXES: &[&str] = &[
    "find", "read", "get", "query", "search", "stream", "count", "exists", "delete", "remove",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpringRouteAnchor {
    pub(crate) target: &'static str,
    pub(crate) annotation: &'static str,
    pub(crate) http_method: &'static str,
    pub(crate) route_path_shape: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpringClassAnchor {
    pub(crate) target: &'static str,
    pub(crate) annotation: &'static str,
    pub(crate) anchor_kind: &'static str,
}

pub(crate) fn spring_route_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringRouteAnchor> {
    for (annotation, target, default_method) in ROUTE_MAPPING_ANNOTATIONS {
        let Some(segment) = annotation_segment_exact(
            annotation_text,
            annotation,
            &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
            imports,
        ) else {
            continue;
        };
        let http_method = if *annotation == "RequestMapping" {
            request_mapping_http_method(&segment).unwrap_or(default_method)
        } else {
            default_method
        };
        return Some(SpringRouteAnchor {
            target,
            annotation,
            http_method,
            route_path_shape: route_path_shape(&segment),
        });
    }
    None
}

pub(crate) fn spring_class_anchor(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringClassAnchor> {
    for (annotation, package, target, anchor_kind) in [
        (
            "SpringBootApplication",
            SPRING_BOOT_AUTOCONFIGURE_PACKAGE,
            "spring.boot.autoconfigure.SpringBootApplication",
            "spring_boot_application",
        ),
        (
            "RestController",
            SPRING_WEB_BIND_ANNOTATION_PACKAGE,
            "spring.web.bind.annotation.RestController",
            "spring_component",
        ),
        (
            "Controller",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Controller",
            "spring_component",
        ),
        (
            "Service",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Service",
            "spring_component",
        ),
        (
            "Repository",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Repository",
            "spring_component",
        ),
        (
            "Component",
            SPRING_STEREOTYPE_PACKAGE,
            "spring.stereotype.Component",
            "spring_component",
        ),
    ] {
        if has_exact_direct_annotation(annotation_text, annotation, &[package], imports) {
            return Some(SpringClassAnchor {
                target,
                annotation,
                anchor_kind,
            });
        }
    }
    None
}

pub(crate) fn spring_data_repository_anchor(
    annotation_text: &str,
    interface_text: &str,
    imports: &JavaImportContext,
) -> Option<SpringClassAnchor> {
    if extends_exact_type(
        interface_text,
        "JpaRepository",
        &[SPRING_DATA_JPA_PACKAGE],
        imports,
    ) {
        return Some(SpringClassAnchor {
            target: "spring.data.jpa.repository.JpaRepository",
            annotation: "JpaRepository",
            anchor_kind: "spring_data_jpa_repository",
        });
    }
    if has_exact_direct_annotation(
        annotation_text,
        "RepositoryDefinition",
        &[SPRING_DATA_REPOSITORY_PACKAGE],
        imports,
    ) {
        return Some(SpringClassAnchor {
            target: "spring.data.repository.RepositoryDefinition",
            annotation: "RepositoryDefinition",
            anchor_kind: "spring_data_repository_definition",
        });
    }
    None
}

pub(crate) fn route_assumptions(
    anchor: &SpringRouteAnchor,
    class_route_path_shape: &str,
    annotations: &str,
    slice: &str,
) -> Vec<String> {
    vec![
        "provider_resolved=false".to_string(),
        "java_anchor_kind=spring_mvc_route".to_string(),
        format!("spring_annotation={}", anchor.annotation),
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

pub(crate) fn class_route_path_shape(
    annotation_text: &str,
    imports: &JavaImportContext,
) -> &'static str {
    annotation_segment_exact(
        annotation_text,
        "RequestMapping",
        &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
        imports,
    )
    .as_deref()
    .map(route_path_shape)
    .unwrap_or("none")
}

pub(crate) fn is_controller_context(annotation_text: &str, imports: &JavaImportContext) -> bool {
    has_exact_direct_annotation(
        annotation_text,
        "Controller",
        &[SPRING_STEREOTYPE_PACKAGE],
        imports,
    ) || has_exact_direct_annotation(
        annotation_text,
        "RestController",
        &[SPRING_WEB_BIND_ANNOTATION_PACKAGE],
        imports,
    )
}

fn request_mapping_http_method(segment: &str) -> Option<&'static str> {
    if segment.contains("RequestMethod.GET") {
        Some("GET")
    } else if segment.contains("RequestMethod.POST") {
        Some("POST")
    } else if segment.contains("RequestMethod.PUT") {
        Some("PUT")
    } else if segment.contains("RequestMethod.PATCH") {
        Some("PATCH")
    } else if segment.contains("RequestMethod.DELETE") {
        Some("DELETE")
    } else {
        None
    }
}

pub(crate) fn contains_route_mapping_annotation_name(annotation_text: &str) -> bool {
    let names = ROUTE_MAPPING_ANNOTATIONS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>();
    contains_annotation_simple_name(annotation_text, &names)
}

pub(crate) fn contains_spring_known_annotation_name(annotation_text: &str) -> bool {
    let mut names = SPRING_CLASS_ANNOTATION_NAMES.to_vec();
    names.extend(ROUTE_MAPPING_ANNOTATIONS.iter().map(|(name, _, _)| *name));
    contains_annotation_simple_name(annotation_text, &names)
}

/// Bounded Spring Data derived-query method-name grammar:
/// `^(find|read|get|query|search|stream|count|exists|delete|remove)(First|Top\d*)?(Distinct)?By[A-Z].*`.
/// This is structural metadata only; property-path validity is never claimed.
pub(crate) fn is_derived_query_method_name(name: &str) -> bool {
    let Some(mut rest) = DERIVED_QUERY_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
    else {
        return false;
    };
    if let Some(after) = rest.strip_prefix("First") {
        rest = after;
    } else if let Some(after) = rest.strip_prefix("Top") {
        rest = after.trim_start_matches(|character: char| character.is_ascii_digit());
    }
    if let Some(after) = rest.strip_prefix("Distinct") {
        rest = after;
    }
    rest.strip_prefix("By")
        .and_then(|after| after.chars().next())
        .is_some_and(|character| character.is_ascii_uppercase())
}
