//! Conservative Java framework adapter registry.
//!
//! These adapters identify only exact source-visible Java framework roles
//! (Spring MVC/stereotype/Boot/Data, JUnit 5/4, TestNG, JPA/Jakarta Persistence,
//! JAX-RS/Jakarta REST). They do not claim classpath, composed annotation, proxy,
//! dependency-injection, component-scan, annotation-processing, or generated-code
//! semantics.
//!
//! This module is the single authoritative Java role/target registry and also
//! owns the two cross-file policy tables — the blocking family-claim list and the
//! copied-assumption/feature table — so the application indexing and family
//! layers reference one source of truth (one-authoritative-classifier contract).

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_SPRING_MVC_ROUTE: &str = "framework:spring.mvc_route";
pub(crate) const ROLE_SPRING_COMPONENT: &str = "framework:spring.component";
pub(crate) const ROLE_SPRING_BOOT_APPLICATION: &str = "framework:spring_boot.application";
pub(crate) const ROLE_SPRING_DATA_REPOSITORY: &str = "framework:spring_data.repository";
pub(crate) const ROLE_JUNIT5_TEST: &str = "framework:junit5.test";
pub(crate) const ROLE_JUNIT4_TEST: &str = "framework:junit4.test";
pub(crate) const ROLE_TESTNG_TEST: &str = "framework:testng.test";
pub(crate) const ROLE_JPA_ENTITY: &str = "framework:jpa.entity";
pub(crate) const ROLE_JPA_MAPPED_SUPERCLASS: &str = "framework:jpa.mapped_superclass";
pub(crate) const ROLE_JPA_EMBEDDABLE: &str = "framework:jpa.embeddable";
pub(crate) const ROLE_JAXRS_RESOURCE: &str = "framework:jaxrs.resource";
pub(crate) const ROLE_JAXRS_RESOURCE_METHOD: &str = "framework:jaxrs.resource_method";

pub(crate) const SPRING_MVC_ROUTE_TARGETS: &[&str] = &[
    "spring.web.bind.annotation.RequestMapping",
    "spring.web.bind.annotation.GetMapping",
    "spring.web.bind.annotation.PostMapping",
    "spring.web.bind.annotation.PutMapping",
    "spring.web.bind.annotation.PatchMapping",
    "spring.web.bind.annotation.DeleteMapping",
];

pub(crate) const SPRING_COMPONENT_TARGETS: &[&str] = &[
    "spring.stereotype.Component",
    "spring.stereotype.Service",
    "spring.stereotype.Repository",
    "spring.stereotype.Controller",
    "spring.web.bind.annotation.RestController",
];

pub(crate) const JUNIT5_TEST_TARGETS: &[&str] =
    &["junit.jupiter.Test", "junit.jupiter.ParameterizedTest"];
pub(crate) const JUNIT4_TEST_TARGETS: &[&str] = &["junit4.Test"];
pub(crate) const TESTNG_TEST_TARGETS: &[&str] = &["testng.annotations.Test"];
pub(crate) const JPA_ENTITY_TARGETS: &[&str] = &["jpa.persistence.Entity"];
pub(crate) const JPA_MAPPED_SUPERCLASS_TARGETS: &[&str] = &["jpa.persistence.MappedSuperclass"];
pub(crate) const JPA_EMBEDDABLE_TARGETS: &[&str] = &["jpa.persistence.Embeddable"];
pub(crate) const JAXRS_RESOURCE_TARGETS: &[&str] = &["jaxrs.ws.rs.Path"];
pub(crate) const JAXRS_RESOURCE_METHOD_TARGETS: &[&str] = &[
    "jaxrs.ws.rs.GET",
    "jaxrs.ws.rs.POST",
    "jaxrs.ws.rs.PUT",
    "jaxrs.ws.rs.DELETE",
    "jaxrs.ws.rs.PATCH",
    "jaxrs.ws.rs.HEAD",
    "jaxrs.ws.rs.OPTIONS",
];

/// Authoritative blocking family-affecting affected-claims for Java. A blocking
/// UNKNOWN carrying one of these claims (or a `family:`-prefixed claim) vetoes
/// support minting and family membership. Referenced from both the application
/// family gate and the indexing support-minting filter so the two never drift.
pub(crate) const BLOCKING_FAMILY_AFFECTED_CLAIMS: &[&str] = &[
    "java_family_membership",
    "java_spring_annotation_binding",
    "java_spring_controller_identity",
    "java_spring_framework_identity",
    "java_spring_repository_identity",
    "java_test_annotation_binding",
    "java_jpa_entity_identity",
    "java_jaxrs_resource_identity",
];

/// Whether a Java UNKNOWN affected-claim blocks family membership/support minting.
pub(crate) fn affected_claim_blocks_family(affected_claim: &str) -> bool {
    BLOCKING_FAMILY_AFFECTED_CLAIMS.contains(&affected_claim)
        || affected_claim.starts_with("family:")
}

/// Authoritative `(parser-assumption-prefix, family-feature-prefix)` pairs copied
/// from Structural anchors onto DATAFLOW_DERIVED support facts and mapped into
/// family features. Referenced from both the indexing derived-fact copy filter
/// and the family feature extractor so the two never drift.
pub(crate) const COPIED_ASSUMPTION_FEATURES: &[(&str, &str)] = &[
    ("java_anchor_kind=", "anchor_kind:"),
    ("spring_annotation=", "spring_annotation:"),
    ("http_method=", "http_method:"),
    ("route_path_shape=", "route_path_shape:"),
    ("class_route_path_shape=", "class_route_path_shape:"),
    ("java_visibility_shape=", "visibility_shape:"),
    ("java_return_shape=", "return_shape:"),
    ("java_parameter_shape=", "parameter_shape:"),
    ("java_class_shape=", "class_shape:"),
    ("test_annotation=", "test_annotation:"),
    ("test_data_shape=", "test_data_shape:"),
    ("mockito_context=", "mockito_context:"),
    ("jpa_namespace_root=", "jpa_namespace_root:"),
    ("jpa_id_present=", "jpa_id_present:"),
    ("jpa_relationship_shape=", "jpa_relationship_shape:"),
];

/// Whether a Structural-anchor assumption is copied onto its derived support fact.
pub(crate) fn assumption_is_copied_to_support(assumption: &str) -> bool {
    COPIED_ASSUMPTION_FEATURES
        .iter()
        .any(|(prefix, _)| assumption.starts_with(prefix))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JavaFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<JavaFrameworkRole> {
    match kind {
        CodeUnitKind::SpringMvcRoute => Some(JavaFrameworkRole {
            target: ROLE_SPRING_MVC_ROUTE,
            note: "Tree-sitter Java code unit indicates exact Spring MVC route role",
            assumption: "Spring MVC runtime dispatch unresolved",
        }),
        CodeUnitKind::SpringComponent => Some(JavaFrameworkRole {
            target: ROLE_SPRING_COMPONENT,
            note: "Tree-sitter Java code unit indicates exact Spring stereotype component role",
            assumption: "Spring dependency injection behavior unresolved",
        }),
        CodeUnitKind::SpringBootApplication => Some(JavaFrameworkRole {
            target: ROLE_SPRING_BOOT_APPLICATION,
            note: "Tree-sitter Java code unit indicates Spring Boot application role",
            assumption: "Spring Boot auto-configuration behavior unresolved",
        }),
        CodeUnitKind::SpringDataRepository => Some(JavaFrameworkRole {
            target: ROLE_SPRING_DATA_REPOSITORY,
            note: "Tree-sitter Java code unit indicates Spring Data JPA repository role",
            assumption: "Spring Data generated implementation unresolved",
        }),
        CodeUnitKind::Junit5TestMethod => Some(JavaFrameworkRole {
            target: ROLE_JUNIT5_TEST,
            note: "Tree-sitter Java code unit indicates exact JUnit 5 test-method role",
            assumption: "JUnit runtime test execution unresolved",
        }),
        CodeUnitKind::Junit4TestMethod => Some(JavaFrameworkRole {
            target: ROLE_JUNIT4_TEST,
            note: "Tree-sitter Java code unit indicates exact JUnit 4 test-method role",
            assumption: "JUnit runtime test execution unresolved",
        }),
        CodeUnitKind::TestngTestMethod => Some(JavaFrameworkRole {
            target: ROLE_TESTNG_TEST,
            note: "Tree-sitter Java code unit indicates exact TestNG test-method role",
            assumption: "TestNG runtime test execution unresolved",
        }),
        CodeUnitKind::JpaEntity => Some(JavaFrameworkRole {
            target: ROLE_JPA_ENTITY,
            note: "Tree-sitter Java code unit indicates exact JPA entity role",
            assumption: "JPA runtime mapping unresolved",
        }),
        CodeUnitKind::JpaMappedSuperclass => Some(JavaFrameworkRole {
            target: ROLE_JPA_MAPPED_SUPERCLASS,
            note: "Tree-sitter Java code unit indicates exact JPA mapped-superclass role",
            assumption: "JPA runtime mapping unresolved",
        }),
        CodeUnitKind::JpaEmbeddable => Some(JavaFrameworkRole {
            target: ROLE_JPA_EMBEDDABLE,
            note: "Tree-sitter Java code unit indicates exact JPA embeddable role",
            assumption: "JPA runtime mapping unresolved",
        }),
        CodeUnitKind::JaxrsResourceClass => Some(JavaFrameworkRole {
            target: ROLE_JAXRS_RESOURCE,
            note: "Tree-sitter Java code unit indicates exact JAX-RS resource-path role",
            assumption: "JAX-RS runtime dispatch unresolved",
        }),
        CodeUnitKind::JaxrsResourceMethod => Some(JavaFrameworkRole {
            target: ROLE_JAXRS_RESOURCE_METHOD,
            note: "Tree-sitter Java code unit indicates exact JAX-RS resource-method role",
            assumption: "JAX-RS runtime dispatch unresolved",
        }),
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:spring.")
        || framework_role.starts_with("framework:spring_boot.")
        || framework_role.starts_with("framework:spring_data.")
        || framework_role.starts_with("framework:junit5.")
        || framework_role.starts_with("framework:junit4.")
        || framework_role.starts_with("framework:testng.")
        || framework_role.starts_with("framework:jpa.")
        || framework_role.starts_with("framework:jaxrs.")
}

pub(crate) fn support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_SPRING_MVC_ROUTE => Some(SPRING_MVC_ROUTE_TARGETS.contains(&target)),
        ROLE_SPRING_COMPONENT => Some(SPRING_COMPONENT_TARGETS.contains(&target)),
        ROLE_SPRING_BOOT_APPLICATION => {
            Some(target == "spring.boot.autoconfigure.SpringBootApplication")
        }
        ROLE_SPRING_DATA_REPOSITORY => Some(matches!(
            target,
            "spring.data.jpa.repository.JpaRepository"
                | "spring.data.repository.RepositoryDefinition"
        )),
        ROLE_JUNIT5_TEST => Some(JUNIT5_TEST_TARGETS.contains(&target)),
        ROLE_JUNIT4_TEST => Some(JUNIT4_TEST_TARGETS.contains(&target)),
        ROLE_TESTNG_TEST => Some(TESTNG_TEST_TARGETS.contains(&target)),
        ROLE_JPA_ENTITY => Some(JPA_ENTITY_TARGETS.contains(&target)),
        ROLE_JPA_MAPPED_SUPERCLASS => Some(JPA_MAPPED_SUPERCLASS_TARGETS.contains(&target)),
        ROLE_JPA_EMBEDDABLE => Some(JPA_EMBEDDABLE_TARGETS.contains(&target)),
        ROLE_JAXRS_RESOURCE => Some(JAXRS_RESOURCE_TARGETS.contains(&target)),
        ROLE_JAXRS_RESOURCE_METHOD => Some(JAXRS_RESOURCE_METHOD_TARGETS.contains(&target)),
        _ if framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_SPRING_MVC_ROUTE if target.starts_with("spring.web.bind.annotation.") => {
            "spring.mvc.route_mapping".to_string()
        }
        ROLE_SPRING_COMPONENT if target.starts_with("spring.stereotype.") => {
            "spring.stereotype.component".to_string()
        }
        ROLE_SPRING_BOOT_APPLICATION => "spring.boot.application".to_string(),
        ROLE_SPRING_DATA_REPOSITORY => match target {
            "spring.data.repository.RepositoryDefinition" => {
                "spring.data.repository_definition".to_string()
            }
            _ => "spring.data.jpa_repository".to_string(),
        },
        ROLE_JUNIT5_TEST => "junit.jupiter.test_annotation".to_string(),
        ROLE_JUNIT4_TEST => "junit4.test_annotation".to_string(),
        ROLE_TESTNG_TEST => "testng.test_annotation".to_string(),
        ROLE_JPA_ENTITY => "jpa.persistence.entity".to_string(),
        ROLE_JPA_MAPPED_SUPERCLASS => "jpa.persistence.mapped_superclass".to_string(),
        ROLE_JPA_EMBEDDABLE => "jpa.persistence.embeddable".to_string(),
        ROLE_JAXRS_RESOURCE => "jaxrs.resource_path".to_string(),
        ROLE_JAXRS_RESOURCE_METHOD => "jaxrs.resource_method_mapping".to_string(),
        _ => target.to_string(),
    }
}
