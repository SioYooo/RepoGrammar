//! Conservative general Rust framework adapter registry (bounded v0.2 preview).
//!
//! These adapters recognize only exact source-visible serde/thiserror/tokio/clap
//! derive and attribute shapes plus axum literal `Router::new().route(...)`
//! segments, each gated by same-file `use`-path evidence or an inline
//! fully-qualified path. They never claim derive-macro expansion, trait
//! resolution, or points-to analysis. They are independent from — and must not
//! alter — the Rust self-dogfood role policy in
//! `crate::core::policy::rust_self_dogfood`.

use crate::core::model::CodeUnitKind;
use crate::core::policy::rust_self_dogfood;

pub(crate) const ROLE_SERDE_MODEL: &str = "framework:serde.model";
pub(crate) const ROLE_THISERROR_ERROR: &str = "framework:thiserror.error";
pub(crate) const ROLE_TOKIO_ENTRY: &str = "framework:tokio.entry";
pub(crate) const ROLE_TOKIO_TEST: &str = "framework:tokio.test";
pub(crate) const ROLE_CLAP_PARSER: &str = "framework:clap.parser";
pub(crate) const ROLE_AXUM_ROUTE: &str = "framework:axum.route";

pub(crate) const SERDE_MODEL_TARGETS: &[&str] = &["serde.Serialize", "serde.Deserialize"];
pub(crate) const CLAP_PARSER_TARGETS: &[&str] = &["clap.Parser", "clap.Subcommand", "clap.Args"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RustGeneralFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

/// Map a general-framework code-unit kind to its heuristic framework role fact.
/// Non-framework and self-dogfood kinds return `None` (the self-dogfood policy
/// handles those separately).
pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<RustGeneralFrameworkRole> {
    match kind {
        CodeUnitKind::SerdeModel => Some(RustGeneralFrameworkRole {
            target: ROLE_SERDE_MODEL,
            note: "Tree-sitter Rust code unit indicates exact serde derive model role",
            assumption: "serde derive-macro expansion is not performed",
        }),
        CodeUnitKind::ThiserrorErrorEnum => Some(RustGeneralFrameworkRole {
            target: ROLE_THISERROR_ERROR,
            note: "Tree-sitter Rust code unit indicates exact thiserror error enum role",
            assumption: "thiserror derive-macro expansion is not performed",
        }),
        CodeUnitKind::TokioEntry => Some(RustGeneralFrameworkRole {
            target: ROLE_TOKIO_ENTRY,
            note: "Tree-sitter Rust code unit indicates exact tokio entrypoint role",
            assumption: "tokio runtime attribute expansion is not performed",
        }),
        CodeUnitKind::TokioTest => Some(RustGeneralFrameworkRole {
            target: ROLE_TOKIO_TEST,
            note: "Tree-sitter Rust code unit indicates exact tokio test role",
            assumption: "tokio runtime test attribute expansion is not performed",
        }),
        CodeUnitKind::ClapParser => Some(RustGeneralFrameworkRole {
            target: ROLE_CLAP_PARSER,
            note: "Tree-sitter Rust code unit indicates exact clap derive parser role",
            assumption: "clap derive-macro expansion is not performed",
        }),
        CodeUnitKind::AxumRoute => Some(RustGeneralFrameworkRole {
            target: ROLE_AXUM_ROUTE,
            note: "Tree-sitter Rust code unit indicates exact axum literal route role",
            assumption: "axum extractor trait resolution is not performed",
        }),
        _ => None,
    }
}

/// True when `framework_role` is a general (non self-dogfood) Rust framework
/// role owned by this registry.
pub(crate) fn general_framework_role_is_known(framework_role: &str) -> bool {
    matches!(
        framework_role,
        ROLE_SERDE_MODEL
            | ROLE_THISERROR_ERROR
            | ROLE_TOKIO_ENTRY
            | ROLE_TOKIO_TEST
            | ROLE_CLAP_PARSER
            | ROLE_AXUM_ROUTE
    )
}

/// Whitelisted general-framework support-target compatibility. Returns `None`
/// for roles this registry does not own.
pub(crate) fn general_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        ROLE_SERDE_MODEL => Some(SERDE_MODEL_TARGETS.contains(&target)),
        ROLE_THISERROR_ERROR => Some(target == "thiserror.Error"),
        ROLE_TOKIO_ENTRY => Some(target == "tokio.main"),
        ROLE_TOKIO_TEST => Some(target == "tokio.test"),
        ROLE_CLAP_PARSER => Some(CLAP_PARSER_TARGETS.contains(&target)),
        ROLE_AXUM_ROUTE => Some(target == "axum.routing.route"),
        _ => None,
    }
}

/// General-framework support family grouping (kind→family). serde and clap
/// group all whitelisted traits under one derive family; evidence-pair
/// compatibility keeps the trait/target profile distinct where the wave
/// requires it (see `family.rs`).
pub(crate) fn general_support_family(_target: &str, framework_role: &str) -> String {
    match framework_role {
        ROLE_SERDE_MODEL => "serde.derive_model".to_string(),
        ROLE_THISERROR_ERROR => "thiserror.error_enum".to_string(),
        ROLE_TOKIO_ENTRY => "tokio.entry".to_string(),
        ROLE_TOKIO_TEST => "tokio.test".to_string(),
        ROLE_CLAP_PARSER => "clap.derive_parser".to_string(),
        ROLE_AXUM_ROUTE => "axum.route".to_string(),
        _ => framework_role.to_string(),
    }
}

/// Authoritative Rust support-target/role compatibility across both the
/// self-dogfood policy and the general framework registry. This is the single
/// entrypoint callers must route through; it must not be reimplemented from raw
/// fact fields elsewhere.
pub fn rust_support_target_is_role_compatible(target: &str, framework_role: &str) -> Option<bool> {
    if let Some(result) =
        rust_self_dogfood::rust_support_target_is_role_compatible(target, framework_role)
    {
        return Some(result);
    }
    general_support_target_is_role_compatible(target, framework_role)
}

/// True when `framework_role` is a known Rust role (self-dogfood or general).
pub fn rust_role_is_known(framework_role: &str) -> bool {
    rust_self_dogfood::rust_role_is_known(framework_role)
        || general_framework_role_is_known(framework_role)
}

/// Authoritative Rust support-family grouping across both role families.
pub fn rust_support_family(target: &str, framework_role: &str) -> String {
    if rust_self_dogfood::rust_role_is_known(framework_role) {
        rust_self_dogfood::rust_support_family(target, framework_role)
    } else if general_framework_role_is_known(framework_role) {
        general_support_family(target, framework_role)
    } else {
        framework_role.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_roles_map_kinds_and_targets() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::SerdeModel)
                .expect("serde role")
                .target,
            ROLE_SERDE_MODEL
        );
        assert_eq!(
            general_support_target_is_role_compatible("serde.Serialize", ROLE_SERDE_MODEL),
            Some(true)
        );
        assert_eq!(
            general_support_target_is_role_compatible("clap.Parser", ROLE_SERDE_MODEL),
            Some(false)
        );
        assert_eq!(
            general_support_target_is_role_compatible("axum.routing.route", ROLE_AXUM_ROUTE),
            Some(true)
        );
        assert_eq!(
            general_support_target_is_role_compatible("thiserror.Error", "framework:unknown"),
            None
        );
    }

    #[test]
    fn combined_router_preserves_self_dogfood_and_adds_general() {
        // Self-dogfood role still resolves through the combined entrypoint.
        assert_eq!(
            rust_support_target_is_role_compatible(
                "repogrammar.rust.cli_command",
                "framework:repogrammar.rust_cli_command"
            ),
            Some(true)
        );
        // General role resolves too, without touching self-dogfood tables.
        assert_eq!(
            rust_support_target_is_role_compatible("tokio.main", ROLE_TOKIO_ENTRY),
            Some(true)
        );
        assert!(rust_role_is_known(ROLE_CLAP_PARSER));
        assert!(rust_role_is_known(
            "framework:repogrammar.rust_parser_adapter"
        ));
        assert!(!rust_role_is_known("framework:aspnetcore.controller"));
        assert_eq!(
            rust_support_family("serde.Deserialize", ROLE_SERDE_MODEL),
            "serde.derive_model"
        );
    }
}
