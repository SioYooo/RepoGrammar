//! Conservative Java/Spring framework adapter registry.
//!
//! These adapters identify only exact source-visible Java and Spring roles. They
//! do not claim classpath, composed annotation, proxy, dependency-injection, or
//! component-scan semantics.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_SPRING_MVC_ROUTE: &str = "framework:spring.mvc_route";
pub(crate) const ROLE_SPRING_COMPONENT: &str = "framework:spring.component";
pub(crate) const ROLE_SPRING_BOOT_APPLICATION: &str = "framework:spring_boot.application";
pub(crate) const ROLE_SPRING_DATA_REPOSITORY: &str = "framework:spring_data.repository";

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
        _ => None,
    }
}

pub(crate) fn framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:spring.")
        || framework_role.starts_with("framework:spring_boot.")
        || framework_role.starts_with("framework:spring_data.")
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
        _ => target.to_string(),
    }
}
