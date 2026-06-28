//! Conservative Rust self-dogfood role policy.
//!
//! These helpers classify RepoGrammar's own Rust implementation shapes from
//! repo-relative metadata only. They intentionally do not imply compiler-backed
//! Rust semantics.

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
                "src/rust/adapters/parsing/rust_syntax.rs",
                "rust_method",
                "unit:src/rust/adapters/parsing/rust_syntax.rs#rust_method:parse:0-10:0",
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
}
