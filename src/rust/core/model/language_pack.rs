//! Declarative per-language family policy (ADR-0057, proposed).
//!
//! A pack is plain data that the family builder reads in place of a
//! per-language function set. Packs are `static` items, so identity is the
//! pack's address.

use super::UnknownReasonCode;

/// Family policy for one framework role.
#[derive(Debug)]
pub(crate) struct RoleFamilyPolicy {
    /// Raw framework role, for example `framework:aspnetcore.controller_action`.
    pub role: &'static str,
    /// The role's `framework_role:` feature value (its stable-token form).
    pub feature: &'static str,
    /// Variation slots as `(slot name, feature prefixes)`.
    pub variation: &'static [(&'static str, &'static [&'static str])],
    /// Prefixes whose values must be equal across members; absence is allowed.
    pub equal: &'static [&'static str],
    /// Prefixes that must be present and equal across members.
    pub required_equal: &'static [&'static str],
    /// The identity-defining prefixes: `equal` followed by `required_equal`.
    pub characteristic: &'static [&'static str],
}

/// Family policy for one language lane.
#[derive(Debug)]
pub(crate) struct LanguageFamilyPack {
    pub language: &'static str,
    /// Origin of the parser's structural anchor and UNKNOWN facts.
    pub anchor_engine: &'static str,
    pub anchor_method: &'static str,
    /// Origin of application-derived support facts.
    pub derived_support_engine: &'static str,
    pub derived_support_method: &'static str,
    /// Required `derived_from=` assumption naming the anchor source.
    pub derived_from_anchor: &'static str,
    pub min_family_support: usize,
    /// `(assumption prefix, feature prefix)` pairs copied into family features.
    pub assumption_features: &'static [(&'static str, &'static str)],
    pub roles: &'static [RoleFamilyPolicy],
    pub default_affected_claim: &'static str,
    pub recovery_scope: &'static str,
    /// Whether a reason can block membership; an exhaustive match in the pack.
    pub reason_may_block: fn(UnknownReasonCode) -> bool,
    /// Claims that block membership when the reason may block. Claims under
    /// `family:` always do.
    pub blocking_claims: &'static [&'static str],
    pub non_blocking_reasons: &'static [UnknownReasonCode],
    pub non_blocking_claims: &'static [&'static str],
    pub role_is_known: fn(&str) -> bool,
    pub support_target_is_role_compatible: fn(&str, &str) -> Option<bool>,
    pub support_family: fn(&str, &str) -> String,
}

impl LanguageFamilyPack {
    pub(crate) fn role(&self, framework_role: &str) -> Option<&'static RoleFamilyPolicy> {
        self.roles
            .iter()
            .find(|policy| policy.role == framework_role)
    }

    pub(crate) fn role_by_feature(&self, feature: &str) -> Option<&'static RoleFamilyPolicy> {
        self.roles.iter().find(|policy| policy.feature == feature)
    }
}

impl PartialEq for LanguageFamilyPack {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for LanguageFamilyPack {}
