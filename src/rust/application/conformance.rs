//! Static-alignment authority.
//!
//! This module is the single decision entrypoint for comparing a target code
//! unit's indexed feature profile against a pattern family's source-backed
//! [`FamilyConstraintProfile`]. It produces a static-alignment certificate and
//! never a runtime-conformance verdict: the runtime-equivalence obligation is
//! reported verbatim as an unresolved obligation and the caller keeps
//! `runtime_equivalence: UNKNOWN` in every certificate.
//!
//! The comparison is purely structural. Every value it reports is a RepoGrammar
//! feature TOKEN (a stripped, source-free value such as `fastapi_route_decorator`
//! or `http_method_get`), never repository source text. Callers route, format,
//! and persist the result but must not re-derive the alignment decision.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::model::{
    FamilyConstraintProfile, FeatureConstraintSemantics, TypedUnknown, UnknownObligation,
    VariationConstraint,
};
use crate::core::policy::alignment::{AlignmentStatus, StaticDeviationKind};

/// The indexed feature profile of a target code unit.
///
/// It is extracted by the SAME family-induction authorities that build a
/// family's constraint profile (`family::extract_target_unit_features`): the
/// per-unit feature map, the per-language characteristic/variation prefix tables,
/// and the typed blocking-unknown vocabulary. No source is re-parsed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetFeatureProfile {
    pub code_unit_id: String,
    pub language: String,
    pub code_unit_kind: String,
    /// Stable-token framework role (e.g. `framework_fastapi_route`), or `None`
    /// when the unit carries no single framework-role fact.
    pub framework_role: Option<String>,
    /// The original framework-role identity (e.g. `framework:fastapi.route`) used
    /// to match the target's family key against candidate families. `None` when
    /// the unit carries no single framework-role fact.
    pub framework_role_key: Option<String>,
    /// Full feature tokens for this unit (with their prefixes), including any
    /// synthetic characteristic tokens the family authority injects (e.g. the
    /// pytest `fixture_context_nonbuiltin:` tokens).
    pub feature_tokens: BTreeSet<String>,
    /// The target's rendered profile for each variation dimension of its role,
    /// keyed by dimension name. An absent or empty entry means the target carries
    /// no value on that dimension. Rendered by the same authority that renders a
    /// family member's variation profile, so the strings compare directly.
    pub variation_profiles: BTreeMap<String, String>,
    /// Typed blocking unknowns recorded for this unit by family induction (both
    /// per-unit blockers and repository-level blockers such as a rust build-variant
    /// ambiguity), so a blocked target classifies as `BLOCKED_UNKNOWN`.
    pub blocking_unknowns: Vec<TypedUnknown>,
}

/// Rendered token used when a variation dimension carries no value.
pub const ABSENT_PROFILE_TOKEN: &str = "<absent>";

impl TargetFeatureProfile {
    /// The stripped, deterministically sorted values the target carries under a
    /// feature prefix (e.g. `http_method:` -> `["http_method_get"]`).
    pub fn values_under(&self, prefix: &str) -> Vec<String> {
        self.feature_tokens
            .iter()
            .filter_map(|token| token.strip_prefix(prefix).map(str::to_string))
            .collect()
    }

    /// Whether the target carries any feature token under a prefix.
    pub fn has_any_under(&self, prefix: &str) -> bool {
        self.feature_tokens
            .iter()
            .any(|token| token.starts_with(prefix))
    }
}

/// A required (or prohibited) constraint the target satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedConstraint {
    pub prefix: String,
    pub semantics: FeatureConstraintSemantics,
    /// Deterministic, source-free summary of what the constraint required.
    pub expected_summary: String,
    /// Deterministic, source-free summary of how the target satisfied it.
    pub satisfied_summary: String,
}

/// A single static deviation. `observed_summary` is always a rendered list of
/// feature TOKENS (or the absent marker), never source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticDeviation {
    /// The required-constraint prefix, or the variation dimension name.
    pub prefix: String,
    pub kind: StaticDeviationKind,
    /// The constraint semantics token, or `variation_dimension` for an unobserved
    /// variation deviation.
    pub semantics_token: String,
    pub expected_summary: String,
    pub observed_summary: String,
}

/// A variation dimension the target differs on within the observed legal space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalVariation {
    pub dimension: String,
    /// The target's rendered variation profile that matched an observed profile,
    /// or [`ABSENT_PROFILE_TOKEN`] when the family observed an absent profile.
    pub observed_profile: String,
}

/// The full static-alignment computation for one target against one family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentComputation {
    pub status: AlignmentStatus,
    pub required_features_matched: Vec<MatchedConstraint>,
    pub static_deviations: Vec<StaticDeviation>,
    pub legal_observed_variations: Vec<LegalVariation>,
    pub blocking_unknowns: Vec<TypedUnknown>,
    pub unresolved_runtime_obligations: Vec<UnknownObligation>,
    /// One deterministic, source-free sentence explaining the outcome.
    pub outcome_reason: String,
}

/// Scale-protection cap on the alignment certificate's deviation-style arrays
/// (`static_deviations` and `legal_observed_variations`).
///
/// Both arrays are per-family bounded in practice — a target only deviates on the
/// constraint prefixes its family declares — but they are structurally unbounded,
/// so an adversarial or pathological target could inflate the certificate past the
/// transport request-line ceiling. When either array exceeds this cap the
/// certificate serializers emit the first [`ALIGNMENT_DEVIATION_CAP`] entries plus
/// an honest sibling `<name>_truncated: true` flag and `<name>_count` total; below
/// the cap the full array is emitted with no extra fields (byte-stable). The cap
/// is sized well above any observed per-family deviation count so a real
/// certificate is never truncated.
pub const ALIGNMENT_DEVIATION_CAP: usize = 24;

fn render_values(values: &[String]) -> String {
    if values.is_empty() {
        ABSENT_PROFILE_TOKEN.to_string()
    } else {
        values.join(",")
    }
}

/// Authoritative static-alignment comparator.
///
/// Compares `target`'s indexed feature profile against `profile` and returns the
/// certificate body. The status follows a deterministic, documented rule:
///
/// - any required-feature violation or a prohibited-presence match -> `STATIC_DEVIATION`;
/// - else a blocking unknown or a non-violating deviation signal (unobserved or
///   truncated variation, or a blocking-suppressed requirement) -> `PARTIAL_ALIGNMENT`;
/// - else every required constraint matched with no deviation -> `STATICALLY_ALIGNED`;
/// - a profile with nothing comparable -> `UNKNOWN`.
///
/// Precedence: a blocking unknown on the target downgrades *absence-driven*
/// required checks (a required value simply missing) to a non-violating
/// `blocking_suppressed_requirement`; *presence-driven* checks (a value definitely
/// present and wrong or prohibited) still deviate.
///
/// `INSUFFICIENT_EVIDENCE` is a *selection* outcome (no or ambiguous family) and
/// is decided by the caller before a profile is available, never here.
pub fn compute_alignment(
    profile: &FamilyConstraintProfile,
    target: &TargetFeatureProfile,
) -> AlignmentComputation {
    let mut matched = Vec::new();
    let mut deviations = Vec::new();
    let mut legal_variations = Vec::new();
    // Precedence: a blocking unknown on the target plausibly suppressed a feature
    // from the static view, so an ABSENCE-driven required check (a required value
    // simply missing) must not fabricate a `STATIC_DEVIATION`. Presence-driven
    // checks (a value that is definitely present and wrong/prohibited) still
    // deviate. See the precedence table in docs/specifications/query-resolution.md.
    let has_blocking = !target.blocking_unknowns.is_empty();

    // Required-feature axis: Equal / EqualEmpty / MustContain.
    for constraint in &profile.required_equal_features {
        let expected: BTreeSet<String> = constraint.values.iter().cloned().collect();
        let observed_vec = target.values_under(&constraint.prefix);
        let observed: BTreeSet<String> = observed_vec.iter().cloned().collect();
        match constraint.semantics {
            FeatureConstraintSemantics::Equal => {
                if observed == expected {
                    matched.push(MatchedConstraint {
                        prefix: constraint.prefix.clone(),
                        semantics: constraint.semantics,
                        expected_summary: format!("equal to {}", render_values(&constraint.values)),
                        satisfied_summary: render_values(&observed_vec),
                    });
                } else {
                    // Absence-driven when the observed values are a (strict) subset
                    // of the expected set: the target carries only expected values
                    // but is missing one or more, which a blocking unknown can
                    // plausibly suppress from the static view. The empty set is such
                    // a subset. Any offending value present (a value not in
                    // `expected`) makes it presence-driven -> a definite mismatch,
                    // even when required values are simultaneously missing.
                    let kind = required_deviation_kind(
                        observed.is_subset(&expected),
                        has_blocking,
                        StaticDeviationKind::RequiredMismatch,
                    );
                    deviations.push(StaticDeviation {
                        prefix: constraint.prefix.clone(),
                        kind,
                        semantics_token: constraint.semantics.as_token().to_string(),
                        expected_summary: format!("equal to {}", render_values(&constraint.values)),
                        observed_summary: render_values(&observed_vec),
                    });
                }
            }
            FeatureConstraintSemantics::EqualEmpty => {
                if observed.is_empty() {
                    matched.push(MatchedConstraint {
                        prefix: constraint.prefix.clone(),
                        semantics: constraint.semantics,
                        expected_summary: "no value (must be empty)".to_string(),
                        satisfied_summary: ABSENT_PROFILE_TOKEN.to_string(),
                    });
                } else {
                    // Presence-driven: a value is definitely present where none is
                    // allowed. A blocking unknown cannot explain extra presence.
                    deviations.push(StaticDeviation {
                        prefix: constraint.prefix.clone(),
                        kind: StaticDeviationKind::MustBeEmptyViolation,
                        semantics_token: constraint.semantics.as_token().to_string(),
                        expected_summary: "no value (must be empty)".to_string(),
                        observed_summary: render_values(&observed_vec),
                    });
                }
            }
            FeatureConstraintSemantics::MustContain => {
                if expected.is_subset(&observed) {
                    matched.push(MatchedConstraint {
                        prefix: constraint.prefix.clone(),
                        semantics: constraint.semantics,
                        expected_summary: format!(
                            "contains all of {}",
                            render_values(&constraint.values)
                        ),
                        satisfied_summary: render_values(&observed_vec),
                    });
                } else {
                    // A missing core is absence-driven: the required value is not
                    // present, which a blocking unknown can suppress.
                    let kind = required_deviation_kind(
                        true,
                        has_blocking,
                        StaticDeviationKind::MissingRequiredCore,
                    );
                    deviations.push(StaticDeviation {
                        prefix: constraint.prefix.clone(),
                        kind,
                        semantics_token: constraint.semantics.as_token().to_string(),
                        expected_summary: format!(
                            "contains all of {}",
                            render_values(&constraint.values)
                        ),
                        observed_summary: render_values(&observed_vec),
                    });
                }
            }
            // A prohibition never appears on the required axis; ignore defensively.
            FeatureConstraintSemantics::ProhibitedPresence => {}
        }
    }

    // Prohibited axis: presence excludes membership.
    for constraint in &profile.prohibited_or_blocking_features {
        if !matches!(
            constraint.semantics,
            FeatureConstraintSemantics::ProhibitedPresence
        ) {
            continue;
        }
        let observed_vec = target.values_under(&constraint.prefix);
        if target.has_any_under(&constraint.prefix) {
            deviations.push(StaticDeviation {
                prefix: constraint.prefix.clone(),
                kind: StaticDeviationKind::ProhibitedPresence,
                semantics_token: constraint.semantics.as_token().to_string(),
                expected_summary: "absent (prohibited)".to_string(),
                observed_summary: render_values(&observed_vec),
            });
        } else {
            matched.push(MatchedConstraint {
                prefix: constraint.prefix.clone(),
                semantics: constraint.semantics,
                expected_summary: "absent (prohibited)".to_string(),
                satisfied_summary: ABSENT_PROFILE_TOKEN.to_string(),
            });
        }
    }

    // Variation axis: observed-only. A target profile not among the enumerated
    // observed profiles is an unobserved variation — unless the enumeration was
    // truncated at the cap, in which case "never observed" cannot be proven and it
    // is reported as a distinct truncated observation. Neither is ever a violation.
    for variation in &profile.allowed_variations {
        let target_profile = target
            .variation_profiles
            .get(&variation.dimension)
            .cloned()
            .unwrap_or_default();
        if target_profile.is_empty() {
            if variation.includes_absent_profile {
                legal_variations.push(LegalVariation {
                    dimension: variation.dimension.clone(),
                    observed_profile: ABSENT_PROFILE_TOKEN.to_string(),
                });
            } else {
                deviations.push(variation_deviation(variation, ABSENT_PROFILE_TOKEN));
            }
        } else if variation
            .observed_profiles
            .iter()
            .any(|profile| profile == &target_profile)
        {
            legal_variations.push(LegalVariation {
                dimension: variation.dimension.clone(),
                observed_profile: target_profile,
            });
        } else {
            deviations.push(variation_deviation(variation, &target_profile));
        }
    }

    let blocking_unknowns = target.blocking_unknowns.clone();
    let unresolved_runtime_obligations = profile.unresolved_obligations.clone();

    let violation_count = deviations
        .iter()
        .filter(|deviation| deviation.kind.is_violation())
        .count();
    let partial_signal_count = deviations
        .iter()
        .filter(|deviation| !deviation.kind.is_violation())
        .count();
    let nothing_comparable =
        matched.is_empty() && deviations.is_empty() && legal_variations.is_empty();

    let (status, outcome_reason) = if nothing_comparable {
        (
            AlignmentStatus::Unknown,
            "the family profile carried no comparable required feature".to_string(),
        )
    } else if violation_count > 0 {
        (
            AlignmentStatus::StaticDeviation,
            format!("{violation_count} required-feature violation(s) against the family profile"),
        )
    } else if !blocking_unknowns.is_empty() || partial_signal_count > 0 {
        (
            AlignmentStatus::PartialAlignment,
            format!(
                "all required features matched, but {} blocking unknown(s) and {partial_signal_count} non-violating deviation signal(s) (unobserved/truncated variation or blocking-suppressed requirement) prevent a clean alignment",
                blocking_unknowns.len()
            ),
        )
    } else {
        (
            AlignmentStatus::StaticallyAligned,
            format!(
                "all {} required feature(s) matched with no deviation",
                matched.len()
            ),
        )
    };

    AlignmentComputation {
        status,
        required_features_matched: matched,
        static_deviations: deviations,
        legal_observed_variations: legal_variations,
        blocking_unknowns,
        unresolved_runtime_obligations,
        outcome_reason,
    }
}

/// The deviation kind for an absence-driven required check: a blocking unknown on
/// the target downgrades it from `violation_kind` to a non-violating
/// blocking-suppressed requirement. `absence_driven` is `false` when an offending
/// value is present (a value the constraint does not allow), which always keeps
/// `violation_kind` even if required values are simultaneously missing.
fn required_deviation_kind(
    absence_driven: bool,
    has_blocking: bool,
    violation_kind: StaticDeviationKind,
) -> StaticDeviationKind {
    if absence_driven && has_blocking {
        StaticDeviationKind::BlockingSuppressedRequirement
    } else {
        violation_kind
    }
}

/// A variation-dimension deviation: `truncated_observation` when the family's
/// observed-profile enumeration was capped (so "never observed" is unprovable),
/// otherwise `unobserved_variation`. Neither is a violation.
fn variation_deviation(variation: &VariationConstraint, observed_profile: &str) -> StaticDeviation {
    let kind = if variation.observed_profiles_truncated {
        StaticDeviationKind::TruncatedObservation
    } else {
        StaticDeviationKind::UnobservedVariation
    };
    let expected_summary = if variation.observed_profiles_truncated {
        format!(
            "one of the observed profiles (enumeration truncated at the cap; not among {})",
            render_values(&variation.observed_profiles)
        )
    } else {
        format!(
            "one of observed profiles {}",
            render_values(&variation.observed_profiles)
        )
    };
    StaticDeviation {
        prefix: variation.dimension.clone(),
        kind,
        semantics_token: "variation_dimension".to_string(),
        expected_summary,
        observed_summary: observed_profile.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        FeatureConstraint, FeatureConstraintOrigin, UnknownClass, UnknownReasonCode,
    };

    fn equal(prefix: &str, values: &[&str]) -> FeatureConstraint {
        FeatureConstraint {
            prefix: prefix.to_string(),
            values: values.iter().map(|value| value.to_string()).collect(),
            origin: FeatureConstraintOrigin::CharacteristicProfile,
            semantics: if values.is_empty() {
                FeatureConstraintSemantics::EqualEmpty
            } else {
                FeatureConstraintSemantics::Equal
            },
        }
    }

    fn must_contain(prefix: &str, values: &[&str]) -> FeatureConstraint {
        FeatureConstraint {
            prefix: prefix.to_string(),
            values: values.iter().map(|value| value.to_string()).collect(),
            origin: FeatureConstraintOrigin::SupportFamilyIntersection,
            semantics: FeatureConstraintSemantics::MustContain,
        }
    }

    fn prohibited(prefix: &str) -> FeatureConstraint {
        FeatureConstraint {
            prefix: prefix.to_string(),
            values: Vec::new(),
            origin: FeatureConstraintOrigin::IncompatibilityBlocker,
            semantics: FeatureConstraintSemantics::ProhibitedPresence,
        }
    }

    fn runtime_obligation() -> UnknownObligation {
        TypedUnknown::new(
            UnknownClass::NonBlocking,
            UnknownReasonCode::FrameworkMagic,
            "family:example:runtime_equivalence",
            Some("add semantic-worker evidence".to_string()),
        )
        .expect("valid obligation")
    }

    fn base_profile() -> FamilyConstraintProfile {
        FamilyConstraintProfile {
            required_equal_features: vec![
                equal("framework_role:", &["framework_fastapi_route"]),
                equal("decorator_shape:", &["fastapi_route_decorator"]),
            ],
            allowed_variations: vec![VariationConstraint {
                dimension: "python_fastapi_effect_marker".to_string(),
                observed_profiles: vec!["effect_db".to_string(), "effect_pure".to_string()],
                observed_profiles_truncated: false,
                includes_absent_profile: true,
                representative_member_ids: vec!["unit:a".to_string()],
                observed_only: true,
            }],
            prohibited_or_blocking_features: vec![prohibited("unknown_blocker:")],
            unresolved_obligations: vec![runtime_obligation()],
        }
    }

    fn target_with(tokens: &[&str]) -> TargetFeatureProfile {
        TargetFeatureProfile {
            code_unit_id: "unit:target".to_string(),
            language: "python".to_string(),
            code_unit_kind: "function".to_string(),
            framework_role: Some("framework_fastapi_route".to_string()),
            framework_role_key: Some("framework:fastapi.route".to_string()),
            feature_tokens: tokens.iter().map(|token| token.to_string()).collect(),
            variation_profiles: BTreeMap::new(),
            blocking_unknowns: Vec::new(),
        }
    }

    fn blocking_unknown() -> TypedUnknown {
        TypedUnknown::new(
            UnknownClass::Blocking,
            UnknownReasonCode::DynamicImport,
            "unit:target:membership",
            None,
        )
        .expect("valid unknown")
    }

    #[test]
    fn clean_member_is_statically_aligned() {
        let mut target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:fastapi_route_decorator",
        ]);
        target.variation_profiles.insert(
            "python_fastapi_effect_marker".to_string(),
            "effect_db".to_string(),
        );
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::StaticallyAligned);
        assert_eq!(computation.static_deviations, Vec::new());
        // role + decorator + prohibited-absent.
        assert_eq!(computation.required_features_matched.len(), 3);
        assert_eq!(computation.legal_observed_variations.len(), 1);
        // The runtime-equivalence obligation is always carried, never discharged.
        assert_eq!(computation.unresolved_runtime_obligations.len(), 1);
    }

    #[test]
    fn required_mismatch_is_static_deviation() {
        let target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:django_view_decorator",
        ]);
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        let deviation = computation
            .static_deviations
            .iter()
            .find(|deviation| deviation.prefix == "decorator_shape:")
            .expect("decorator deviation");
        assert_eq!(deviation.kind, StaticDeviationKind::RequiredMismatch);
        assert_eq!(deviation.observed_summary, "django_view_decorator");
    }

    #[test]
    fn missing_required_core_is_static_deviation() {
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![must_contain("support_family:", &["sqlalchemy"])],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let target = target_with(&["support_family:django"]);
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::MissingRequiredCore
        );
    }

    #[test]
    fn must_contain_superset_matches() {
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![must_contain("support_family:", &["sqlalchemy"])],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let target = target_with(&["support_family:sqlalchemy", "support_family:extra"]);
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::StaticallyAligned);
    }

    #[test]
    fn prohibited_presence_is_static_deviation() {
        let target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:fastapi_route_decorator",
            "unknown_blocker:dynamic_import",
        ]);
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert!(computation
            .static_deviations
            .iter()
            .any(|deviation| deviation.kind == StaticDeviationKind::ProhibitedPresence));
    }

    #[test]
    fn equal_empty_violation_is_static_deviation() {
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![equal("fixture_context_nonbuiltin:", &[])],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let target = target_with(&["fixture_context_nonbuiltin:conftest_db"]);
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::MustBeEmptyViolation
        );
    }

    #[test]
    fn unobserved_variation_is_partial_alignment_not_illegal() {
        let mut target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:fastapi_route_decorator",
        ]);
        target.variation_profiles.insert(
            "python_fastapi_effect_marker".to_string(),
            "effect_network".to_string(),
        );
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        let deviation = &computation.static_deviations[0];
        assert_eq!(deviation.kind, StaticDeviationKind::UnobservedVariation);
        // Vocabulary must say unobserved, never illegal.
        assert_eq!(deviation.semantics_token, "variation_dimension");
        assert!(!deviation.kind.is_violation());
    }

    #[test]
    fn blocking_unknown_forces_partial_alignment() {
        let mut target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:fastapi_route_decorator",
        ]);
        target.variation_profiles.insert(
            "python_fastapi_effect_marker".to_string(),
            "effect_db".to_string(),
        );
        target.blocking_unknowns.push(
            TypedUnknown::new(
                UnknownClass::Blocking,
                UnknownReasonCode::DynamicImport,
                "unit:target:membership",
                None,
            )
            .expect("valid unknown"),
        );
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        assert_eq!(computation.blocking_unknowns.len(), 1);
    }

    #[test]
    fn empty_profile_is_unknown() {
        let profile = FamilyConstraintProfile::empty();
        let target = target_with(&["framework_role:framework_fastapi_route"]);
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::Unknown);
    }

    #[test]
    fn deviation_summaries_are_source_free_tokens() {
        // The observed summary must be a feature token, never source text.
        let target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:some_other_decorator",
        ]);
        let computation = compute_alignment(&base_profile(), &target);
        for deviation in &computation.static_deviations {
            assert!(!deviation.observed_summary.contains('@'));
            assert!(
                !deviation.observed_summary.contains(' ')
                    || deviation.observed_summary == ABSENT_PROFILE_TOKEN
            );
        }
    }

    #[test]
    fn absence_driven_required_check_under_blocking_unknown_is_partial_not_deviation() {
        // The characteristic feature is absent, but a blocking unknown plausibly
        // suppressed it: precedence routes to PARTIAL_ALIGNMENT, not STATIC_DEVIATION.
        let mut target = target_with(&["framework_role:framework_fastapi_route"]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        let deviation = computation
            .static_deviations
            .iter()
            .find(|deviation| deviation.prefix == "decorator_shape:")
            .expect("decorator deviation");
        assert_eq!(
            deviation.kind,
            StaticDeviationKind::BlockingSuppressedRequirement
        );
        assert!(!deviation.kind.is_violation());
    }

    #[test]
    fn missing_core_under_blocking_unknown_is_partial_not_deviation() {
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![must_contain("support_family:", &["sqlalchemy"])],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let mut target = target_with(&[]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::BlockingSuppressedRequirement
        );
    }

    #[test]
    fn presence_driven_violation_still_deviates_under_blocking_unknown() {
        // A definitely-present wrong value is a real deviation even with a blocking
        // unknown: the unknown cannot explain extra presence.
        let mut target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:django_view_decorator",
        ]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert!(computation
            .static_deviations
            .iter()
            .any(|deviation| deviation.kind == StaticDeviationKind::RequiredMismatch));
    }

    #[test]
    fn multivalue_equal_strict_subset_under_blocking_unknown_is_partial_not_deviation() {
        // A multi-value `Equal` constraint whose target carries a strict SUBSET of
        // the expected values (only `get` of {get, post}, with no offending value)
        // is absence-driven: the missing `post` may have been suppressed from the
        // static view by the blocking unknown, so it downgrades to a non-violating
        // blocking-suppressed requirement, not a fabricated STATIC_DEVIATION.
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![equal(
                "http_method:",
                &["http_method_get", "http_method_post"],
            )],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let mut target = target_with(&["http_method:http_method_get"]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::BlockingSuppressedRequirement
        );
        assert!(!computation.static_deviations[0].kind.is_violation());
    }

    #[test]
    fn multivalue_equal_strict_subset_without_blocking_still_deviates() {
        // The same strict-subset shape WITHOUT a blocking unknown: a plainly missing
        // required value is a genuine violation, so it must stay STATIC_DEVIATION /
        // required_mismatch. Absence downgrades only under a blocking unknown.
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![equal(
                "http_method:",
                &["http_method_get", "http_method_post"],
            )],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let target = target_with(&["http_method:http_method_get"]);
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::RequiredMismatch
        );
    }

    #[test]
    fn multivalue_equal_with_offending_value_still_deviates_under_blocking_unknown() {
        // The target is missing `post` (absence) but also carries an offending `put`
        // not in the expected set (presence). Presence wins: observed {get, put} is
        // NOT a subset of expected {get, post}, so it is a real required_mismatch
        // even under a blocking unknown, which cannot explain the extra wrong value.
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![equal(
                "http_method:",
                &["http_method_get", "http_method_post"],
            )],
            allowed_variations: Vec::new(),
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let mut target =
            target_with(&["http_method:http_method_get", "http_method:http_method_put"]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
        assert_eq!(
            computation.static_deviations[0].kind,
            StaticDeviationKind::RequiredMismatch
        );
    }

    #[test]
    fn prohibited_presence_still_deviates_under_blocking_unknown() {
        let mut target = target_with(&[
            "framework_role:framework_fastapi_route",
            "decorator_shape:fastapi_route_decorator",
            "unknown_blocker:dynamic_import",
        ]);
        target.blocking_unknowns.push(blocking_unknown());
        let computation = compute_alignment(&base_profile(), &target);
        assert_eq!(computation.status, AlignmentStatus::StaticDeviation);
    }

    #[test]
    fn truncated_observation_is_partial_not_unobserved() {
        // When the observed-profile enumeration was truncated at the cap, a
        // non-matching target profile cannot be called 'never observed'.
        let profile = FamilyConstraintProfile {
            required_equal_features: vec![equal("framework_role:", &["framework_fastapi_route"])],
            allowed_variations: vec![VariationConstraint {
                dimension: "python_fastapi_effect_marker".to_string(),
                observed_profiles: vec!["effect_a".to_string(), "effect_b".to_string()],
                observed_profiles_truncated: true,
                includes_absent_profile: false,
                representative_member_ids: vec!["unit:a".to_string()],
                observed_only: true,
            }],
            prohibited_or_blocking_features: Vec::new(),
            unresolved_obligations: vec![runtime_obligation()],
        };
        let mut target = target_with(&["framework_role:framework_fastapi_route"]);
        target.variation_profiles.insert(
            "python_fastapi_effect_marker".to_string(),
            "effect_z".to_string(),
        );
        let computation = compute_alignment(&profile, &target);
        assert_eq!(computation.status, AlignmentStatus::PartialAlignment);
        let deviation = &computation.static_deviations[0];
        assert_eq!(deviation.kind, StaticDeviationKind::TruncatedObservation);
        assert!(!deviation.kind.is_violation());
        assert!(deviation.expected_summary.contains("truncated"));
    }
}
