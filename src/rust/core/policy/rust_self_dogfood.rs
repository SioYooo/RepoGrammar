//! Conservative Rust self-dogfood role policy.
//!
//! These helpers classify RepoGrammar's own Rust implementation shapes from
//! repo-relative metadata only. They intentionally do not imply compiler-backed
//! Rust semantics.
//!
//! Every role is gated on RepoGrammar's own non-standard source layout,
//! `src/rust/<documented-module-root>/`, because an ordinary Rust crate places
//! its sources directly under `src/`. That layout is a strong signal, but it is
//! **not proof of repository identity**: any repository is free to adopt the
//! same directory names, and this policy has no repository remote, manifest
//! identity, or content evidence with which to refute that. The residual
//! misattribution risk is therefore `UNKNOWN` and is recorded as a declared
//! non-claim in `docs/reports/language-support/rust-completion-review.md`.
//! Callers must treat `framework:repogrammar.rust_*` roles as layout-scoped
//! structural candidates, never as an assertion that the indexed repository is
//! RepoGrammar.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustSelfDogfoodRole {
    pub framework_role: &'static str,
    pub support_target: &'static str,
    pub anchor_kind: &'static str,
    pub note: &'static str,
    pub unresolved_assumption: &'static str,
}

pub fn rust_self_dogfood_role_for_unit(
    path: &str,
    kind: &str,
    unit_id: &str,
) -> Option<RustSelfDogfoodRole> {
    if !is_repogrammar_rust_layout_path(path) {
        return None;
    }
    if !rust_family_eligible_kind(kind) {
        return None;
    }
    let name = unit_name_slug(unit_id).unwrap_or("");
    if kind == "rust_test_function" || path.contains("/integration_tests/") {
        return Some(role(
            "framework:repogrammar.rust_product_test",
            "repogrammar.rust.product_test",
            "product_test",
            "Rust structural unit indicates RepoGrammar product test role",
            "test runtime behavior unresolved",
        ));
    }
    if path.ends_with("src/rust/application/indexing.rs") {
        return Some(role(
            "framework:repogrammar.rust_indexing_phase",
            "repogrammar.rust.indexing_phase",
            "indexing_phase",
            "Rust structural unit indicates RepoGrammar indexing phase role",
            "indexing dataflow unresolved without compiler/provider evidence",
        ));
    }
    if path.ends_with("src/rust/application/family.rs") {
        return Some(role(
            "framework:repogrammar.rust_family_gate",
            "repogrammar.rust.family_gate",
            "family_gate",
            "Rust structural unit indicates RepoGrammar family gate role",
            "family-gate semantics unresolved without compiler/provider evidence",
        ));
    }
    if path.contains("src/rust/adapters/parsing/") {
        return Some(role(
            "framework:repogrammar.rust_parser_adapter",
            "repogrammar.rust.parser_adapter",
            "parser_adapter",
            "Rust structural unit indicates RepoGrammar parser adapter role",
            "parser semantics unresolved without compiler/provider evidence",
        ));
    }
    if path.ends_with("src/rust/application/install.rs")
        || path.ends_with("src/rust/interfaces/cli/install.rs")
        || (path.ends_with("src/rust/interfaces/cli/mod.rs") && name.contains("install"))
    {
        return Some(role(
            "framework:repogrammar.rust_installer_action",
            "repogrammar.rust.installer_action",
            "installer_action",
            "Rust structural unit indicates RepoGrammar installer action role",
            "native-agent side effects unresolved without integration evidence",
        ));
    }
    if path.ends_with("src/rust/application/storage.rs")
        || path.ends_with("src/rust/adapters/persistence/sqlite.rs")
    {
        let target = if name.contains("validate") {
            "repogrammar.rust.storage_validation"
        } else {
            "repogrammar.rust.storage_record"
        };
        return Some(role(
            "framework:repogrammar.rust_storage_validation",
            target,
            "storage_validation",
            "Rust structural unit indicates RepoGrammar storage validation role",
            "storage invariants unresolved without persistence tests",
        ));
    }
    if path.ends_with("src/rust/application/query.rs")
        && (name.contains("source_span") || name.contains("read_plan") || name.contains("render"))
    {
        return Some(role(
            "framework:repogrammar.rust_source_span_renderer",
            "repogrammar.rust.source_span_renderer",
            "source_span_renderer",
            "Rust structural unit indicates RepoGrammar source-span/read-plan role",
            "source-span safety unresolved without freshness checks",
        ));
    }
    if path.ends_with("src/rust/interfaces/mcp/mod.rs")
        || path.contains("src/rust/interfaces/mcp/")
        || name.contains("mcp")
        || name.contains("tools_call")
    {
        return Some(role(
            "framework:repogrammar.rust_mcp_handler",
            "repogrammar.rust.mcp_handler",
            "mcp_handler",
            "Rust structural unit indicates RepoGrammar MCP handler role",
            "MCP transport behavior unresolved without protocol tests",
        ));
    }
    if path.ends_with("src/rust/bin/repogrammar.rs")
        || path.contains("src/rust/interfaces/cli/")
        || name.starts_with("handle_")
    {
        return Some(role(
            "framework:repogrammar.rust_cli_command",
            "repogrammar.rust.cli_command",
            "cli_command",
            "Rust structural unit indicates RepoGrammar CLI command role",
            "CLI behavior unresolved without product tests",
        ));
    }
    None
}

/// Mandatory layout gate for every self-dogfood role.
///
/// Requires the repo-relative path to sit directly inside RepoGrammar's own
/// `src/rust/<documented-module-root>/` tree. A nested or vendored copy such as
/// `vendor/src/rust/application/indexing.rs` is rejected, because a repo-relative
/// prefix is the only layout evidence available here.
///
/// This gate bounds, but does not resolve, repository identity: see the module
/// comment for the declared non-claim.
fn is_repogrammar_rust_layout_path(path: &str) -> bool {
    let Some(inside_rust_root) = path.strip_prefix("src/rust/") else {
        return false;
    };
    let Some((module_root, remainder)) = inside_rust_root.split_once('/') else {
        return false;
    };
    !remainder.is_empty() && is_documented_rust_module_root(module_root)
}

/// The `src/rust/` module roots documented in `docs/architecture/module-map.md`.
///
/// Undocumented roots are rejected on purpose: a role minted under a root the
/// module map does not describe would have no reviewed ownership record.
fn is_documented_rust_module_root(module_root: &str) -> bool {
    matches!(
        module_root,
        "adapters"
            | "application"
            | "bin"
            | "config"
            | "core"
            | "error"
            | "integration_tests"
            | "interfaces"
            | "ports"
            | "test_support"
    )
}

pub fn rust_family_eligible_kind(kind: &str) -> bool {
    matches!(
        kind,
        "rust_function"
            | "rust_method"
            | "rust_trait_method"
            | "rust_associated_function"
            | "rust_test_function"
            | "rust_impl_block"
            | "rust_struct"
            | "rust_enum"
            | "rust_trait"
    )
}

pub fn rust_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:repogrammar.rust_")
}

pub fn rust_support_target_is_role_compatible(target: &str, framework_role: &str) -> Option<bool> {
    match framework_role {
        "framework:repogrammar.rust_cli_command" => Some(target == "repogrammar.rust.cli_command"),
        "framework:repogrammar.rust_mcp_handler" => Some(target == "repogrammar.rust.mcp_handler"),
        "framework:repogrammar.rust_indexing_phase" => {
            Some(target == "repogrammar.rust.indexing_phase")
        }
        "framework:repogrammar.rust_family_gate" => Some(target == "repogrammar.rust.family_gate"),
        "framework:repogrammar.rust_parser_adapter" => {
            Some(target == "repogrammar.rust.parser_adapter")
        }
        "framework:repogrammar.rust_installer_action" => {
            Some(target == "repogrammar.rust.installer_action")
        }
        "framework:repogrammar.rust_storage_validation" => Some(matches!(
            target,
            "repogrammar.rust.storage_validation" | "repogrammar.rust.storage_record"
        )),
        "framework:repogrammar.rust_source_span_renderer" => {
            Some(target == "repogrammar.rust.source_span_renderer")
        }
        "framework:repogrammar.rust_product_test" => {
            Some(target == "repogrammar.rust.product_test")
        }
        _ if rust_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub fn rust_support_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        "framework:repogrammar.rust_storage_validation" => match target {
            "repogrammar.rust.storage_record" => "repogrammar.rust.storage_record".to_string(),
            _ => "repogrammar.rust.storage_validation".to_string(),
        },
        _ if rust_role_is_known(framework_role) => framework_role
            .strip_prefix("framework:")
            .unwrap_or(framework_role)
            .to_string(),
        _ => framework_role.to_string(),
    }
}

fn role(
    framework_role: &'static str,
    support_target: &'static str,
    anchor_kind: &'static str,
    note: &'static str,
    unresolved_assumption: &'static str,
) -> RustSelfDogfoodRole {
    RustSelfDogfoodRole {
        framework_role,
        support_target,
        anchor_kind,
        note,
        unresolved_assumption,
    }
}

fn unit_name_slug(unit_id: &str) -> Option<&str> {
    let marker = unit_id.split('#').nth(1)?;
    let mut parts = marker.split(':');
    let _kind = parts.next()?;
    parts.next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_repo_internal_roles() {
        let cases = [
            (
                "src/rust/application/indexing.rs",
                "rust_function",
                "unit:src/rust/application/indexing.rs#rust_function:index_repository:0-10:0",
                "framework:repogrammar.rust_indexing_phase",
            ),
            (
                "src/rust/application/family.rs",
                "rust_function",
                "unit:src/rust/application/family.rs#rust_function:build_family_claims:0-10:0",
                "framework:repogrammar.rust_family_gate",
            ),
            (
                "src/rust/adapters/parsing/rust/mod.rs",
                "rust_method",
                "unit:src/rust/adapters/parsing/rust/mod.rs#rust_method:parse:0-10:0",
                "framework:repogrammar.rust_parser_adapter",
            ),
            (
                "src/rust/bin/repogrammar.rs",
                "rust_test_function",
                "unit:src/rust/bin/repogrammar.rs#rust_test_function:product_runtime:0-10:0",
                "framework:repogrammar.rust_product_test",
            ),
        ];

        for (path, kind, unit_id, expected_role) in cases {
            let role =
                rust_self_dogfood_role_for_unit(path, kind, unit_id).expect("role should classify");
            assert_eq!(role.framework_role, expected_role);
        }
    }

    #[test]
    fn keeps_unknown_paths_out_of_rust_roles() {
        assert!(rust_self_dogfood_role_for_unit(
            "src/other.rs",
            "rust_function",
            "unit:src/other.rs#rust_function:helper:0-10:0"
        )
        .is_none());
    }

    #[test]
    fn layout_gate_rejects_third_party_rust_paths() {
        let cases = [
            // A `#[test]` function in any ordinary crate layout.
            (
                "src/lib.rs",
                "rust_test_function",
                "unit:src/lib.rs#rust_test_function:it_works:0-10:0",
            ),
            // `handle_*` is an extremely common third-party Rust function name.
            (
                "src/server/mod.rs",
                "rust_function",
                "unit:src/server/mod.rs#rust_function:handle_request:0-10:0",
            ),
            // Any name containing `mcp`, outside RepoGrammar's layout.
            (
                "third_party/src/mcp.rs",
                "rust_function",
                "unit:third_party/src/mcp.rs#rust_function:mcp_client:0-10:0",
            ),
            (
                "src/transport/tools.rs",
                "rust_method",
                "unit:src/transport/tools.rs#rust_method:tools_call:0-10:0",
            ),
            // A conventional third-party integration-test directory.
            (
                "tests/integration_tests/api.rs",
                "rust_test_function",
                "unit:tests/integration_tests/api.rs#rust_test_function:smoke:0-10:0",
            ),
            // A vendored copy is not RepoGrammar's own repo-relative layout.
            (
                "vendor/src/rust/application/indexing.rs",
                "rust_function",
                "unit:vendor/src/rust/application/indexing.rs#rust_function:index_repository:0-10:0",
            ),
        ];

        for (path, kind, unit_id) in cases {
            assert!(
                rust_self_dogfood_role_for_unit(path, kind, unit_id).is_none(),
                "{path} must not receive a RepoGrammar self-dogfood role"
            );
        }
    }

    #[test]
    fn layout_gate_requires_a_documented_module_root() {
        // Directly under `src/rust/`, so no module root at all.
        assert!(rust_self_dogfood_role_for_unit(
            "src/rust/lib.rs",
            "rust_test_function",
            "unit:src/rust/lib.rs#rust_test_function:smoke:0-10:0"
        )
        .is_none());
        // A root that `docs/architecture/module-map.md` does not document.
        assert!(rust_self_dogfood_role_for_unit(
            "src/rust/undocumented_root/mod.rs",
            "rust_test_function",
            "unit:src/rust/undocumented_root/mod.rs#rust_test_function:smoke:0-10:0"
        )
        .is_none());
    }

    #[test]
    fn layout_scoped_rules_keep_every_repogrammar_true_positive() {
        let cases = [
            (
                "src/rust/application/indexing.rs",
                "rust_function",
                "unit:src/rust/application/indexing.rs#rust_function:index_repository:0-10:0",
                "framework:repogrammar.rust_indexing_phase",
                "repogrammar.rust.indexing_phase",
            ),
            (
                "src/rust/application/family.rs",
                "rust_function",
                "unit:src/rust/application/family.rs#rust_function:build_family_claims:0-10:0",
                "framework:repogrammar.rust_family_gate",
                "repogrammar.rust.family_gate",
            ),
            (
                "src/rust/adapters/parsing/rust/mod.rs",
                "rust_method",
                "unit:src/rust/adapters/parsing/rust/mod.rs#rust_method:parse:0-10:0",
                "framework:repogrammar.rust_parser_adapter",
                "repogrammar.rust.parser_adapter",
            ),
            (
                "src/rust/bin/repogrammar.rs",
                "rust_test_function",
                "unit:src/rust/bin/repogrammar.rs#rust_test_function:product_runtime:0-10:0",
                "framework:repogrammar.rust_product_test",
                "repogrammar.rust.product_test",
            ),
            (
                "src/rust/integration_tests/product.rs",
                "rust_function",
                "unit:src/rust/integration_tests/product.rs#rust_function:runs_product:0-10:0",
                "framework:repogrammar.rust_product_test",
                "repogrammar.rust.product_test",
            ),
            (
                "src/rust/application/install.rs",
                "rust_function",
                "unit:src/rust/application/install.rs#rust_function:plan_install:0-10:0",
                "framework:repogrammar.rust_installer_action",
                "repogrammar.rust.installer_action",
            ),
            (
                "src/rust/interfaces/cli/mod.rs",
                "rust_function",
                "unit:src/rust/interfaces/cli/mod.rs#rust_function:run_install_command:0-10:0",
                "framework:repogrammar.rust_installer_action",
                "repogrammar.rust.installer_action",
            ),
            (
                "src/rust/application/storage.rs",
                "rust_function",
                "unit:src/rust/application/storage.rs#rust_function:validate_records:0-10:0",
                "framework:repogrammar.rust_storage_validation",
                "repogrammar.rust.storage_validation",
            ),
            (
                "src/rust/adapters/persistence/sqlite.rs",
                "rust_method",
                "unit:src/rust/adapters/persistence/sqlite.rs#rust_method:insert_unit:0-10:0",
                "framework:repogrammar.rust_storage_validation",
                "repogrammar.rust.storage_record",
            ),
            (
                "src/rust/application/query.rs",
                "rust_function",
                "unit:src/rust/application/query.rs#rust_function:render_read_plan:0-10:0",
                "framework:repogrammar.rust_source_span_renderer",
                "repogrammar.rust.source_span_renderer",
            ),
            (
                "src/rust/interfaces/mcp/mod.rs",
                "rust_function",
                "unit:src/rust/interfaces/mcp/mod.rs#rust_function:dispatch:0-10:0",
                "framework:repogrammar.rust_mcp_handler",
                "repogrammar.rust.mcp_handler",
            ),
            // Name-based `mcp`/`tools_call` rules stay active inside the layout.
            (
                "src/rust/application/query.rs",
                "rust_function",
                "unit:src/rust/application/query.rs#rust_function:mcp_context:0-10:0",
                "framework:repogrammar.rust_mcp_handler",
                "repogrammar.rust.mcp_handler",
            ),
            (
                "src/rust/bin/repogrammar.rs",
                "rust_function",
                "unit:src/rust/bin/repogrammar.rs#rust_function:main:0-10:0",
                "framework:repogrammar.rust_cli_command",
                "repogrammar.rust.cli_command",
            ),
            // The `handle_*` rule stays active inside the layout.
            (
                "src/rust/core/policy/mod.rs",
                "rust_function",
                "unit:src/rust/core/policy/mod.rs#rust_function:handle_policy:0-10:0",
                "framework:repogrammar.rust_cli_command",
                "repogrammar.rust.cli_command",
            ),
        ];

        for (path, kind, unit_id, expected_role, expected_target) in cases {
            let role = rust_self_dogfood_role_for_unit(path, kind, unit_id)
                .unwrap_or_else(|| panic!("{unit_id} should classify"));
            assert_eq!(role.framework_role, expected_role, "role for {unit_id}");
            assert_eq!(role.support_target, expected_target, "target for {unit_id}");
        }
    }

    #[test]
    fn layout_gate_accepts_only_documented_module_roots() {
        for module_root in [
            "adapters",
            "application",
            "bin",
            "config",
            "core",
            "error",
            "integration_tests",
            "interfaces",
            "ports",
            "test_support",
        ] {
            assert!(
                is_repogrammar_rust_layout_path(&format!("src/rust/{module_root}/mod.rs")),
                "{module_root} is documented in docs/architecture/module-map.md"
            );
        }
        for path in [
            "",
            "src/rust/",
            "src/rust/application",
            "src/rust/application/",
            "src/rustacean/application/mod.rs",
            "/src/rust/application/mod.rs",
        ] {
            assert!(
                !is_repogrammar_rust_layout_path(path),
                "{path:?} is outside RepoGrammar's documented Rust layout"
            );
        }
    }
}
