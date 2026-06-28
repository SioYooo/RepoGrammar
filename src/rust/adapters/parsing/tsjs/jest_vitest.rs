use super::scope_graph::{ImportKind, ScopeGraphLite};
use super::{async_shape, leading_identifier, Anchor, AnchorOutcome, UnknownAnchor};
use crate::core::model::{SemanticFactKind, UnknownReasonCode};
use crate::ports::parser::SourceDocument;
use std::collections::BTreeSet;

pub(super) const RUNNER_MODULES: [&str; 2] = ["vitest", "@jest/globals"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TestRunnerCallNames {
    pub suite_names: BTreeSet<String>,
    pub test_names: BTreeSet<String>,
}

pub(crate) fn exact_test_runner_call_names(text: &str) -> TestRunnerCallNames {
    let bindings = ScopeGraphLite::analyze(text);
    let mut names = TestRunnerCallNames::default();
    for (local, binding) in &bindings.imports {
        if bindings.unsafe_names.contains(local)
            || !RUNNER_MODULES.contains(&binding.module.as_str())
        {
            continue;
        }
        let ImportKind::Named(original) = &binding.kind else {
            continue;
        };
        match original.as_str() {
            "describe" => {
                names.suite_names.insert(local.clone());
            }
            "it" | "test" => {
                names.test_names.insert(local.clone());
            }
            _ => {}
        }
    }
    names
}

pub(super) fn anchor(
    document: &SourceDocument<'_>,
    bindings: &ScopeGraphLite,
    slice: &str,
    start_byte: usize,
    is_suite: bool,
    ambient_runner_allowed: bool,
) -> AnchorOutcome {
    let Some(name) = test_call_name(slice) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "dynamic_test_call",
            note: "TS/JS test runner call shape is dynamic",
        });
    };
    if bindings.name_is_unsafe_at(name, start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is locally reassigned or redeclared",
        });
    }
    if bindings.local_decls.contains(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::ConflictingFacts,
            affected_claim: "tsjs_runner_binding",
            kind: "unsafe_test_runner_binding",
            note: "TS/JS test runner name is a local custom wrapper",
        });
    }
    if let Some((module, original)) = bindings.imported_runner(name) {
        if (is_suite && original == "describe") || (!is_suite && matches!(original, "it" | "test"))
        {
            return AnchorOutcome::Anchor(anchor_for_runner(
                name, original, module, is_suite, slice,
            ));
        }
    }
    if bindings.imports.contains_key(name) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_runner_binding",
            kind: "unresolved_test_runner",
            note: "TS/JS test runner import does not resolve to a known runner",
        });
    }
    let expected_ambient = if is_suite {
        name == "describe"
    } else {
        name == "it" || name == "test"
    };
    if expected_ambient && is_ambient_runner(document.path, bindings, name) {
        if !ambient_runner_allowed {
            return AnchorOutcome::Unknown(UnknownAnchor {
                reason: UnknownReasonCode::MissingProjectConfig,
                affected_claim: "tsjs_runner_binding",
                kind: "ambient_runner_without_project_context",
                note: "TS/JS ambient test runner lacks bounded project test context",
            });
        }
        return AnchorOutcome::Anchor(anchor_for_runner(name, name, "ambient", is_suite, slice));
    }
    AnchorOutcome::Unknown(UnknownAnchor {
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: "tsjs_runner_binding",
        kind: "unresolved_test_runner",
        note: "TS/JS test runner binding is not exact",
    })
}

pub(super) fn anchor_for_runner(
    local_name: &str,
    original: &str,
    runner_kind: &str,
    is_suite: bool,
    slice: &str,
) -> Anchor {
    Anchor {
        target: format!("jest_vitest.{original}"),
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            format!(
                "tsjs_anchor_kind={}",
                if is_suite { "test_suite" } else { "test_case" }
            ),
            format!("runner_kind={runner_kind}"),
            format!("test_shape={original}"),
            format!("async_shape={}", async_shape(slice)),
            format!("import_context={local_name}"),
        ],
    }
}

/// A bare `describe`/`it`/`test` is only treated as a runner global in an actual
/// test file and only when the name is not locally declared or imported from a
/// non-runner module (a custom wrapper / alias).
fn is_ambient_runner(path: &str, bindings: &ScopeGraphLite, name: &str) -> bool {
    is_test_file(path)
        && !bindings.local_decls.contains(name)
        && !bindings.imports.contains_key(name)
}

fn is_test_file(path: &str) -> bool {
    const SUFFIXES: [&str; 8] = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

fn test_call_name(slice: &str) -> Option<&str> {
    let (name, after) = leading_identifier(slice)?;
    if !slice[after..].trim_start().starts_with('(') {
        return None;
    }
    Some(name)
}
