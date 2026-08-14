//! Go role registry (auxiliary evidence only).
//!
//! The one role marks the `testing` test-function declaration ADR-0041 admits.
//! It is deliberately **not** a family-bearing role: ADR-0021's evidence ladder
//! forbids text or regex matching for the claim, and the bounded scanner that
//! produces this role is exactly that. ADR-0041's correction therefore demotes
//! it to auxiliary evidence, so there is no support-target table, no support
//! family, and no derived-support path anywhere for Go.
//!
//! This is the shape React already has in the TS/JS lane: a role may be
//! detected without being allowed to carry a public family claim. Role facts
//! carry `FrameworkHeuristic` certainty, which never supports membership, and
//! nothing derives a support fact from them.

use crate::core::model::CodeUnitKind;

pub(crate) const ROLE_GO_TESTING_TEST: &str = "framework:go_testing.test_function";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GoFrameworkRole {
    pub target: &'static str,
    pub note: &'static str,
    pub assumption: &'static str,
}

pub(crate) fn role_for_code_unit_kind(kind: &CodeUnitKind) -> Option<GoFrameworkRole> {
    match kind {
        CodeUnitKind::GoTestFunction => Some(GoFrameworkRole {
            target: ROLE_GO_TESTING_TEST,
            note: "bounded Go code unit declares a testing test function",
            assumption: "scanner evidence is auxiliary and supports no family claim",
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_admitted_test_declaration_carries_a_role() {
        assert_eq!(
            role_for_code_unit_kind(&CodeUnitKind::GoTestFunction)
                .expect("test role")
                .target,
            ROLE_GO_TESTING_TEST
        );
        assert!(role_for_code_unit_kind(&CodeUnitKind::GoFunction).is_none());
        assert!(role_for_code_unit_kind(&CodeUnitKind::Module).is_none());
    }
}
