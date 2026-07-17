//! Application-layer EC-MVFI-lite family claim construction.

use crate::adapters::frameworks::rust_general::{
    rust_role_is_known, rust_support_family, rust_support_target_is_role_compatible,
};
use crate::adapters::frameworks::{cpp, csharp, java, tsjs};
use crate::adapters::parsing::cpp::{CPP_ANCHOR_ENGINE, CPP_ANCHOR_METHOD};
use crate::adapters::parsing::csharp::{CSHARP_ANCHOR_ENGINE, CSHARP_ANCHOR_METHOD};
use crate::adapters::parsing::java::{JAVA_ANCHOR_ENGINE, JAVA_ANCHOR_METHOD};
use crate::adapters::parsing::python::PYTHON_ANCHOR_ENGINE;
use crate::adapters::parsing::rust::{RUST_ANCHOR_ENGINE, RUST_ANCHOR_METHOD};
use crate::adapters::parsing::tsjs::TSJS_ANCHOR_ENGINE;
use crate::application::proof_lattice::{
    add_variation_features_from_assumptions, derived_support_has_safe_origin,
};
use crate::core::model::{
    assess_family_prevalence, coverage_ratio, ClaimImpact, FactCertainty, FamilyPrevalence,
    PrevalenceInputs, SemanticFact, SemanticFactKind, UnknownClass, UnknownReasonCode,
};
use crate::core::policy::rust_self_dogfood::rust_family_eligible_kind;
use crate::ports::family_store::{
    IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedFamilyRecord,
    IndexedVariationSlotRecord,
};
use crate::ports::index_store::IndexedCodeUnitRecord;
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_MIN_FAMILY_SUPPORT: usize = 2;
const PYTHON_MIN_FAMILY_SUPPORT: usize = 3;
const TSJS_MIN_FAMILY_SUPPORT: usize = 3;
const JAVA_MIN_FAMILY_SUPPORT: usize = 3;
const CSHARP_MIN_FAMILY_SUPPORT: usize = 3;
const CPP_MIN_FAMILY_SUPPORT: usize = 3;
const PYTHON_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-python-derived";
const PYTHON_DERIVED_SUPPORT_METHOD: &str = "bounded_ast_anchor_v1";
const PYTHON_FIXTURE_PROVIDER_ENGINE: &str = "python-fixture-provider";
const PYTHON_FIXTURE_PROVIDER_METHOD: &str = "release_fixture_semantic_support";
/// Engine/method that mint conservative TS/JS exact-anchor support facts.
pub(crate) const TSJS_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-tsjs-derived";
pub(crate) const TSJS_DERIVED_SUPPORT_METHOD: &str = "bounded_exact_anchor_v1";
pub(crate) const JAVA_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-java-derived";
pub(crate) const JAVA_DERIVED_SUPPORT_METHOD: &str = "bounded_tree_sitter_java_anchor_v1";
pub(crate) const CSHARP_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-csharp-derived";
pub(crate) const CSHARP_DERIVED_SUPPORT_METHOD: &str = "bounded_tree_sitter_csharp_anchor_v1";
pub(crate) const CPP_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-cpp-derived";
pub(crate) const CPP_DERIVED_SUPPORT_METHOD: &str = "bounded_tree_sitter_c_cpp_anchor_v1";
pub(crate) const RUST_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-rust-derived";
pub(crate) const RUST_DERIVED_SUPPORT_METHOD: &str = "bounded_tree_sitter_anchor_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyCandidate {
    pub language: String,
    pub code_unit_kind: String,
    pub framework_role: String,
    pub normalized_shape: String,
    pub members: Vec<FamilyEvidence>,
}

// Carries `FamilyPrevalence`, whose coverage ratio is floating point, so this
// derives `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyClaim {
    pub family_id: String,
    pub classification: String,
    pub support: usize,
    pub prevalence: FamilyPrevalence,
    pub language: String,
    pub code_unit_kind: String,
    pub framework_role: String,
    pub normalized_shape: String,
    pub evidence: Vec<FamilyEvidence>,
    pub variation_slots: Vec<VariationSlot>,
    pub exceptions: Vec<FamilyException>,
    pub unknowns: Vec<ClaimUnknown>,
    pub readiness: ClaimReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyEvidence {
    pub code_unit_id: String,
    pub path: String,
    pub content_hash: crate::core::model::ContentHash,
    pub start_byte: usize,
    pub end_byte: usize,
    pub support_targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariationSlot {
    pub slot_id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyException {
    pub code_unit_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A claim-scoped typed `UNKNOWN` exposed by the family application API.
///
/// The legacy public `class` field remains source-compatible for downstream
/// callers. Family blocking decisions use an internal conversion that accepts
/// only `Blocking` and `NonBlocking`.
///
/// ```
/// use repogrammar::application::family::ClaimUnknown;
/// use repogrammar::core::model::{UnknownClass, UnknownReasonCode};
///
/// let unknown = ClaimUnknown {
///     class: UnknownClass::Blocking,
///     reason: UnknownReasonCode::InsufficientSupport,
///     affected_claim: "family:example".to_string(),
///     recovery: None,
/// };
/// assert_eq!(unknown.class, UnknownClass::Blocking);
/// ```
pub struct ClaimUnknown {
    pub class: UnknownClass,
    pub reason: UnknownReasonCode,
    pub affected_claim: String,
    pub recovery: Option<String>,
}

impl ClaimUnknown {
    pub(crate) fn claim_impact(&self) -> Option<ClaimImpact> {
        ClaimImpact::from_legacy_family_class(self.class)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimReadiness {
    Ready,
    Unknown(ClaimUnknown),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyBuildReport {
    pub claims: Vec<FamilyClaim>,
    pub unknowns: Vec<ClaimUnknown>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyClaimInput<'a> {
    pub units: &'a [IndexedCodeUnitRecord],
    pub role_facts: Vec<SemanticFact>,
    pub support_facts: Vec<SemanticFact>,
    pub context_facts: Vec<SemanticFact>,
    pub unknown_facts: Vec<SemanticFact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FamilyStorageRecords {
    pub family: IndexedFamilyRecord,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
}

pub const FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX: &str = "unknown|";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FamilyKey {
    language: String,
    code_unit_kind: String,
    framework_role: String,
    normalized_shape: String,
}

/// Per-key peers excluded from the prevalence denominator but tracked for
/// reliability. Blocked peers had their support emptied by a blocking `UNKNOWN`;
/// unsupported peers never had any role-compatible support facts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct KeyPeerCounters {
    blocked: usize,
    unsupported: usize,
}

impl<'a> FamilyClaimInput<'a> {
    pub fn from_facts(units: &'a [IndexedCodeUnitRecord], facts: &[SemanticFact]) -> Self {
        let mut role_facts = Vec::new();
        let mut support_facts = Vec::new();
        let mut context_facts = Vec::new();
        let mut unknown_facts = Vec::new();

        for fact in facts {
            if fact.kind == SemanticFactKind::FrameworkRole {
                role_facts.push(fact.clone());
            } else if fact.kind == SemanticFactKind::Unknown
                || fact.certainty == FactCertainty::Unknown
            {
                unknown_facts.push(fact.clone());
            } else if fact.certainty.supports_family_membership()
                && fact.kind != SemanticFactKind::ProjectConfig
            {
                support_facts.push(fact.clone());
            } else {
                context_facts.push(fact.clone());
            }
        }

        Self {
            units,
            role_facts,
            support_facts,
            context_facts,
            unknown_facts,
        }
    }
}

pub fn build_family_claims(
    units: &[IndexedCodeUnitRecord],
    semantic_facts: &[SemanticFact],
) -> FamilyBuildReport {
    build_family_claims_from_input(&FamilyClaimInput::from_facts(units, semantic_facts))
}

pub fn build_family_claims_from_input(input: &FamilyClaimInput<'_>) -> FamilyBuildReport {
    let role_by_unit = framework_roles_by_unit(&input.role_facts);
    let support_targets_by_unit =
        eligible_support_by_unit(input.units, &input.support_facts, &role_by_unit);
    let blocking_unknowns =
        family_blocking_unknowns_by_unit(input.units, &input.unknown_facts, &role_by_unit);
    let rust_repository_blocking_unknowns = rust_repository_blocking_unknowns(&input.unknown_facts);
    let has_rust_repository_blocking_unknown = !rust_repository_blocking_unknowns.is_empty();
    let non_blocking_unknowns =
        family_non_blocking_unknowns_by_unit(input.units, &input.unknown_facts, &role_by_unit);
    let mut all_facts = Vec::with_capacity(
        input.role_facts.len()
            + input.support_facts.len()
            + input.context_facts.len()
            + input.unknown_facts.len(),
    );
    all_facts.extend(input.role_facts.iter().cloned());
    all_facts.extend(input.support_facts.iter().cloned());
    all_facts.extend(input.context_facts.iter().cloned());
    all_facts.extend(input.unknown_facts.iter().cloned());
    let features_by_unit = family_features_by_unit(
        input.units,
        &all_facts,
        &role_by_unit,
        &support_targets_by_unit,
    );
    let mut groups: BTreeMap<FamilyKey, Vec<FamilyEvidence>> = BTreeMap::new();
    let mut peer_counters: BTreeMap<FamilyKey, KeyPeerCounters> = BTreeMap::new();
    let mut unknowns = blocking_unknowns
        .values()
        .flat_map(|unknowns| unknowns.iter().cloned())
        .collect::<Vec<_>>();
    unknowns.extend(rust_repository_blocking_unknowns);

    for unit in input.units {
        if !family_eligible_kind(&unit.kind) {
            continue;
        }
        let roles = role_by_unit
            .get(&unit.id)
            .cloned()
            .unwrap_or_else(BTreeSet::new);
        let Some(framework_role) = single_framework_role(&roles) else {
            unknowns.push(insufficient_support_unknown(format!(
                "family:{}:{}",
                unit.language, unit.kind
            )));
            continue;
        };
        let key = FamilyKey {
            language: unit.language.clone(),
            code_unit_kind: unit.kind.clone(),
            framework_role: framework_role.to_string(),
            normalized_shape: normalized_shape(&unit.kind, framework_role),
        };
        // A blocking unknown empties this unit's support even if it had support
        // facts; such units are recorded as blocked, never as unsupported.
        let is_blocked = blocking_unknowns.contains_key(&unit.id)
            || (unit.language == "rust" && has_rust_repository_blocking_unknown);
        let support_targets: Vec<String> = if is_blocked {
            Vec::new()
        } else {
            support_targets_by_unit
                .get(&unit.id)
                .map(|targets| targets.iter().cloned().collect())
                .unwrap_or_default()
        };
        let counters = peer_counters.entry(key.clone()).or_default();
        if is_blocked {
            counters.blocked += 1;
        } else if support_targets.is_empty() {
            counters.unsupported += 1;
        }
        // Eligible peers (non-blocked with support facts) are counted through the
        // supported-evidence denominator below.
        groups.entry(key).or_default().push(FamilyEvidence {
            code_unit_id: unit.id.clone(),
            path: unit.path.clone(),
            content_hash: unit.content_hash.clone(),
            start_byte: unit.start_byte,
            end_byte: unit.end_byte,
            support_targets,
        });
    }

    let mut claims = Vec::new();
    for (key, mut evidence) in groups {
        evidence.sort_by(|left, right| {
            (
                left.path.as_str(),
                left.start_byte,
                left.end_byte,
                left.code_unit_id.as_str(),
            )
                .cmp(&(
                    right.path.as_str(),
                    right.start_byte,
                    right.end_byte,
                    right.code_unit_id.as_str(),
                ))
        });
        let supported_evidence = evidence
            .into_iter()
            .filter(|evidence| !evidence.support_targets.is_empty())
            .collect::<Vec<_>>();
        if supported_evidence.is_empty() {
            unknowns.push(insufficient_support_unknown(family_affected_claim(
                &key, None,
            )));
            continue;
        }
        // Units of this key whose supported evidence survived the blocking
        // filter form the prevalence denominator: they sit in this cluster, a
        // competing cluster, or a sub-support cluster.
        let eligible_peer_count = supported_evidence.len();
        let peers = peer_counters.get(&key).copied().unwrap_or_default();
        let min_support = min_family_support_for_key(&key);
        let clusters = complete_link_family_clusters(&key, supported_evidence, &features_by_unit);
        let cluster_sizes: Vec<usize> = clusters.iter().map(Vec::len).collect();
        let ready_flags: Vec<bool> = cluster_sizes
            .iter()
            .map(|size| *size >= min_support)
            .collect();
        let ready_cluster_count = ready_flags.iter().filter(|ready| **ready).count();
        let mut emitted_ready_clusters = 0usize;
        for (index, cluster) in clusters.into_iter().enumerate() {
            let cluster_suffix = (ready_cluster_count > 1)
                .then(|| family_cluster_signature(&key, &cluster, &features_by_unit));
            if cluster.len() < min_support {
                unknowns.push(insufficient_support_unknown(family_affected_claim(
                    &key,
                    cluster_suffix.as_deref(),
                )));
                continue;
            }
            let suffix = if emitted_ready_clusters == 0 {
                None
            } else {
                cluster_suffix.as_deref()
            };
            let normalized_shape = cluster_normalized_shape(&key, suffix);
            // Competitors are the *other* ready clusters of the same key.
            let largest_competing_support = cluster_sizes
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index && ready_flags[*other])
                .map(|(_, size)| *size)
                .max()
                .unwrap_or(0);
            let prevalence_inputs = PrevalenceInputs {
                eligible_peer_count,
                supported_member_count: cluster.len(),
                competing_ready_family_count: ready_cluster_count - 1,
                largest_competing_support,
                blocked_peer_count: peers.blocked,
                unsupported_peer_count: peers.unsupported,
            };
            claims.push(family_claim_from_supported_evidence(
                &key,
                suffix,
                normalized_shape,
                cluster,
                &features_by_unit,
                &non_blocking_unknowns,
                &prevalence_inputs,
            ));
            emitted_ready_clusters += 1;
        }
    }

    claims.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    unknowns.sort_by(|left, right| {
        (
            left.affected_claim.as_str(),
            left.class.as_protocol_str(),
            left.reason.as_protocol_str(),
        )
            .cmp(&(
                right.affected_claim.as_str(),
                right.class.as_protocol_str(),
                right.reason.as_protocol_str(),
            ))
    });
    unknowns.dedup();
    FamilyBuildReport { claims, unknowns }
}

fn family_claim_from_supported_evidence(
    key: &FamilyKey,
    cluster_suffix: Option<&str>,
    normalized_shape: String,
    supported_evidence: Vec<FamilyEvidence>,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
    non_blocking_unknowns_by_unit: &BTreeMap<String, Vec<ClaimUnknown>>,
    prevalence_inputs: &PrevalenceInputs,
) -> FamilyClaim {
    let family_id = family_id(key, cluster_suffix);
    let runtime_unknown = ClaimUnknown {
        class: ClaimImpact::NonBlocking.as_legacy_unknown_class(),
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: format!("{family_id}:runtime_equivalence"),
        recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
    };
    let non_blocking_unknowns = family_non_blocking_unknowns_for_evidence(
        &family_id,
        &supported_evidence,
        non_blocking_unknowns_by_unit,
    );
    let runtime_unknown_slot_description = format!(
        "{}:{}:{}",
        runtime_unknown.class.as_protocol_str(),
        runtime_unknown.reason.as_protocol_str(),
        "runtime equivalence remains unproven"
    );
    let mut claim_unknowns = vec![runtime_unknown];
    claim_unknowns.extend(non_blocking_unknowns);
    let mut variation_slots = vec![VariationSlot {
        slot_id: "slot:runtime_unknown".to_string(),
        description: runtime_unknown_slot_description,
    }];
    if python_framework_anchor_target_varies(key, &supported_evidence) {
        variation_slots.push(VariationSlot {
            slot_id: "slot:python_framework_anchor_target".to_string(),
            description:
                "variation:python_framework_anchor_target:exact compatible framework anchors differ"
                    .to_string(),
        });
    }
    variation_slots.extend(python_context_variation_slots(
        key,
        &supported_evidence,
        features_by_unit,
    ));
    variation_slots.extend(tsjs_context_variation_slots(
        key,
        &supported_evidence,
        features_by_unit,
    ));
    variation_slots.extend(java_context_variation_slots(
        key,
        &supported_evidence,
        features_by_unit,
    ));
    variation_slots.extend(csharp_context_variation_slots(
        key,
        &supported_evidence,
        features_by_unit,
    ));
    variation_slots.extend(cpp_context_variation_slots(
        key,
        &supported_evidence,
        features_by_unit,
    ));
    variation_slots.extend(non_blocking_unknown_variation_slots(&claim_unknowns));
    let assessment = assess_family_prevalence(prevalence_inputs);
    let prevalence = FamilyPrevalence {
        eligible_peer_count: prevalence_inputs.eligible_peer_count,
        supported_member_count: prevalence_inputs.supported_member_count,
        coverage_ratio: coverage_ratio(
            prevalence_inputs.eligible_peer_count,
            prevalence_inputs.supported_member_count,
        ),
        competing_ready_family_count: prevalence_inputs.competing_ready_family_count,
        largest_competing_support: prevalence_inputs.largest_competing_support,
        blocked_peer_count: prevalence_inputs.blocked_peer_count,
        unsupported_peer_count: prevalence_inputs.unsupported_peer_count,
        classification_reason: assessment.reason,
    };
    FamilyClaim {
        family_id,
        classification: assessment.class.as_token().to_string(),
        support: supported_evidence.len(),
        prevalence,
        language: key.language.clone(),
        code_unit_kind: key.code_unit_kind.clone(),
        framework_role: key.framework_role.clone(),
        normalized_shape,
        evidence: supported_evidence,
        variation_slots,
        exceptions: Vec::new(),
        unknowns: claim_unknowns,
        readiness: ClaimReadiness::Ready,
    }
}

fn family_non_blocking_unknowns_for_evidence(
    family_id: &str,
    evidence: &[FamilyEvidence],
    unknowns_by_unit: &BTreeMap<String, Vec<ClaimUnknown>>,
) -> Vec<ClaimUnknown> {
    let mut unknowns = evidence
        .iter()
        .flat_map(|evidence| {
            unknowns_by_unit
                .get(&evidence.code_unit_id)
                .into_iter()
                .flat_map(|unknowns| unknowns.iter())
        })
        .map(|unknown| ClaimUnknown {
            class: unknown.class,
            reason: unknown.reason,
            affected_claim: family_subclaim(family_id, &unknown.affected_claim),
            recovery: unknown.recovery.clone(),
        })
        .collect::<Vec<_>>();
    unknowns.sort_by(|left, right| {
        (
            left.affected_claim.as_str(),
            left.class.as_protocol_str(),
            left.reason.as_protocol_str(),
        )
            .cmp(&(
                right.affected_claim.as_str(),
                right.class.as_protocol_str(),
                right.reason.as_protocol_str(),
            ))
    });
    unknowns.dedup();
    unknowns
}

fn family_subclaim(family_id: &str, affected_claim: &str) -> String {
    if affected_claim.starts_with(family_id) {
        affected_claim.to_string()
    } else {
        format!("{family_id}:{affected_claim}")
    }
}

fn non_blocking_unknown_variation_slots(unknowns: &[ClaimUnknown]) -> Vec<VariationSlot> {
    unknowns
        .iter()
        .filter(|unknown| {
            !(unknown.reason == UnknownReasonCode::FrameworkMagic
                && unknown.affected_claim.ends_with(":runtime_equivalence"))
        })
        .enumerate()
        .map(|(index, unknown)| VariationSlot {
            slot_id: format!(
                "slot:unknown:{}:{}:{index:06}",
                stable_token(unknown.reason.as_protocol_str()),
                stable_token(&unknown.affected_claim)
            ),
            description: format!(
                "{}{}|{}|{}|{}",
                FAMILY_UNKNOWN_SLOT_DESCRIPTION_PREFIX,
                unknown.class.as_protocol_str(),
                unknown.reason.as_protocol_str(),
                unknown.affected_claim,
                unknown.recovery.as_deref().unwrap_or("")
            ),
        })
        .collect()
}

fn python_context_variation_slots(
    key: &FamilyKey,
    evidence: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<VariationSlot> {
    if key.language != "python" {
        return Vec::new();
    }
    python_variation_feature_prefixes(key.framework_role.as_str())
        .iter()
        .filter_map(|(slot_name, prefixes)| {
            let profiles = evidence
                .iter()
                .map(|item| prefixed_feature_profile(item, features_by_unit, prefixes))
                .collect::<BTreeSet<_>>();
            let has_context = profiles.iter().any(|profile| !profile.is_empty());
            (has_context && profiles.len() > 1).then(|| VariationSlot {
                slot_id: format!("slot:{slot_name}"),
                description: format!(
                    "variation:{slot_name}:context metadata differs across supported members"
                ),
            })
        })
        .collect()
}

fn python_variation_feature_prefixes(
    framework_role: &str,
) -> &'static [(&'static str, &'static [&'static str])] {
    match framework_role {
        "framework:fastapi.route" => &[
            ("python_fastapi_effect_marker", &["effect_marker:"]),
            ("python_fastapi_service_call_shape", &["call_shape:"]),
            ("python_fastapi_fixture_context", &["fixture_context:"]),
            ("python_import_context", &["import_context:"]),
        ],
        "framework:pytest.test" | "framework:pytest.fixture" => &[
            ("python_pytest_fixture_context", &["fixture_context:"]),
            ("python_pytest_effect_marker", &["effect_marker:"]),
            ("python_import_context", &["import_context:"]),
        ],
        "framework:pydantic.model" => &[
            ("python_pydantic_model_context", &["model_context:"]),
            ("python_import_context", &["import_context:"]),
        ],
        "framework:sqlalchemy.model" | "framework:sqlalchemy.repository_method" => &[
            ("python_sqlalchemy_model_context", &["model_context:"]),
            ("python_sqlalchemy_effect_marker", &["effect_marker:"]),
            ("python_sqlalchemy_call_shape", &["call_shape:"]),
            ("python_import_context", &["import_context:"]),
        ],
        "framework:django.model" => &[
            (
                "python_django_field_count_shape",
                &["django_field_count_shape:"],
            ),
            ("python_django_model_meta_shape", &["django_meta_shape:"]),
            ("python_django_model_context", &["model_context:"]),
        ],
        "framework:django.test" => &[(
            "python_django_test_method_count_shape",
            &["test_method_count_shape:"],
        )],
        "framework:flask.route" => &[("python_flask_route_method", &["http_method:"])],
        "framework:unittest.test" => &[(
            "python_unittest_fixture_shape",
            &["unittest_fixture_shape:"],
        )],
        "framework:click.command" | "framework:typer.command" => {
            &[("python_cli_param_count_shape", &["cli_param_count_shape:"])]
        }
        "framework:celery.task" => &[("python_celery_task_shape", &["celery_task_shape:"])],
        _ => &[],
    }
}

fn tsjs_context_variation_slots(
    key: &FamilyKey,
    evidence: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<VariationSlot> {
    if !is_tsjs_language_name(&key.language) {
        return Vec::new();
    }
    tsjs_variation_feature_prefixes(key.framework_role.as_str())
        .iter()
        .filter_map(|(slot_name, prefixes)| {
            let profiles = evidence
                .iter()
                .map(|item| prefixed_feature_profile(item, features_by_unit, prefixes))
                .collect::<BTreeSet<_>>();
            let has_context = profiles.iter().any(|profile| !profile.is_empty());
            (has_context && profiles.len() > 1).then(|| VariationSlot {
                slot_id: format!("slot:{slot_name}"),
                description: format!(
                    "variation:{slot_name}:context metadata differs across supported members"
                ),
            })
        })
        .collect()
}

fn tsjs_variation_feature_prefixes(
    framework_role: &str,
) -> &'static [(&'static str, &'static [&'static str])] {
    match framework_role {
        "framework:express.route_handler" => &[
            ("tsjs_route_method", &["route_method:"]),
            ("tsjs_route_path_shape", &["route_path_shape:"]),
            ("tsjs_handler_shape", &["handler_shape:", "async_shape:"]),
        ],
        "framework:jest_vitest.suite" | "framework:jest_vitest.test" => &[
            ("tsjs_runner_kind", &["runner_kind:"]),
            ("tsjs_test_shape", &["test_shape:", "async_shape:"]),
            ("tsjs_import_context", &["import_context:"]),
        ],
        "framework:next.app.page"
        | "framework:next.app.layout"
        | "framework:next.route.handler"
        | "framework:next.pages.api_route"
        | "framework:next.pages.page" => &[
            ("tsjs_next_router_kind", &["router_kind:"]),
            ("tsjs_next_file_convention", &["file_convention:"]),
            ("tsjs_next_component_shape", &["component_shape:"]),
            (
                "tsjs_next_response_shape",
                &["response_shape:", "fetch_shape:"],
            ),
            ("tsjs_next_route_method", &["http_method:"]),
        ],
        "framework:fastify.route_handler" => &[
            ("tsjs_fastify_route_method", &["route_method:"]),
            ("tsjs_fastify_route_path_shape", &["route_path_shape:"]),
            (
                "tsjs_fastify_handler_shape",
                &["handler_shape:", "async_shape:", "opts_handler_present:"],
            ),
            ("tsjs_fastify_reply_shape", &["reply_shape:"]),
        ],
        "framework:prisma.query" | "framework:prisma.transaction" => &[
            ("tsjs_prisma_operation", &["operation:"]),
            ("tsjs_prisma_model", &["model_name:"]),
            (
                "tsjs_prisma_query_shape",
                &[
                    "where_shape:",
                    "select_include_shape:",
                    "transaction_shape:",
                ],
            ),
        ],
        "framework:drizzle.schema.table"
        | "framework:drizzle.query"
        | "framework:drizzle.transaction" => &[
            ("tsjs_drizzle_operation", &["operation:"]),
            ("tsjs_drizzle_table", &["table_name:"]),
            (
                "tsjs_drizzle_query_shape",
                &[
                    "where_shape:",
                    "returning_shape:",
                    "join_shape:",
                    "transaction_shape:",
                ],
            ),
        ],
        "framework:zod.schema" => &[
            ("tsjs_zod_builder_shape", &["zod_builder:"]),
            ("tsjs_zod_field_count_shape", &["zod_field_count_shape:"]),
        ],
        "framework:nestjs.route" => &[
            ("tsjs_nest_route_method", &["http_method:"]),
            ("tsjs_nest_route_path_shape", &["route_path_shape:"]),
        ],
        "framework:nestjs.controller"
        | "framework:nestjs.injectable"
        | "framework:nestjs.module" => &[(
            "tsjs_nest_class_shape",
            &["class_route_path_shape:", "nest_module_shape:"],
        )],
        "framework:hono.route" => &[
            ("tsjs_hono_route_method", &["http_method:"]),
            ("tsjs_hono_route_path_shape", &["route_path_shape:"]),
        ],
        _ => &[],
    }
}

fn java_context_variation_slots(
    key: &FamilyKey,
    evidence: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<VariationSlot> {
    if key.language != "java" {
        return Vec::new();
    }
    java_variation_feature_prefixes(key.framework_role.as_str())
        .iter()
        .filter_map(|(slot_name, prefixes)| {
            let profiles = evidence
                .iter()
                .map(|item| prefixed_feature_profile(item, features_by_unit, prefixes))
                .collect::<BTreeSet<_>>();
            let has_context = profiles.iter().any(|profile| !profile.is_empty());
            (has_context && profiles.len() > 1).then(|| VariationSlot {
                slot_id: format!("slot:{slot_name}"),
                description: format!(
                    "variation:{slot_name}:context metadata differs across supported members"
                ),
            })
        })
        .collect()
}

fn java_variation_feature_prefixes(
    framework_role: &str,
) -> &'static [(&'static str, &'static [&'static str])] {
    match framework_role {
        "framework:spring.mvc_route" => &[
            ("java_spring_route_method", &["http_method:"]),
            (
                "java_spring_route_path_shape",
                &["route_path_shape:", "class_route_path_shape:"],
            ),
            (
                "java_spring_method_shape",
                &["return_shape:", "parameter_shape:"],
            ),
        ],
        "framework:spring.component" => &[
            ("java_spring_stereotype", &["spring_annotation:"]),
            ("java_spring_class_shape", &["class_shape:"]),
        ],
        "framework:spring_boot.application" => {
            &[("java_spring_boot_class_shape", &["class_shape:"])]
        }
        "framework:spring_data.repository" => &[
            ("java_spring_data_repository_kind", &["support_family:"]),
            ("java_spring_data_class_shape", &["class_shape:"]),
        ],
        "framework:junit5.test" => &[("java_junit5_test_data_shape", &["test_data_shape:"])],
        "framework:jpa.entity" | "framework:jpa.mapped_superclass" | "framework:jpa.embeddable" => {
            &[(
                "java_jpa_field_shape",
                &["jpa_id_present:", "jpa_relationship_shape:"],
            )]
        }
        "framework:jaxrs.resource_method" => &[
            ("java_jaxrs_route_method", &["http_method:"]),
            (
                "java_jaxrs_route_path_shape",
                &["route_path_shape:", "class_route_path_shape:"],
            ),
        ],
        _ => &[],
    }
}

fn csharp_context_variation_slots(
    key: &FamilyKey,
    evidence: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<VariationSlot> {
    if key.language != "csharp" {
        return Vec::new();
    }
    csharp_variation_feature_prefixes(key.framework_role.as_str())
        .iter()
        .filter_map(|(slot_name, prefixes)| {
            let profiles = evidence
                .iter()
                .map(|item| prefixed_feature_profile(item, features_by_unit, prefixes))
                .collect::<BTreeSet<_>>();
            let has_context = profiles.iter().any(|profile| !profile.is_empty());
            (has_context && profiles.len() > 1).then(|| VariationSlot {
                slot_id: format!("slot:{slot_name}"),
                description: format!(
                    "variation:{slot_name}:context metadata differs across supported members"
                ),
            })
        })
        .collect()
}

fn csharp_variation_feature_prefixes(
    framework_role: &str,
) -> &'static [(&'static str, &'static [&'static str])] {
    match framework_role {
        "framework:aspnetcore.controller_action" => &[
            ("csharp_aspnet_route_method", &["http_method:"]),
            (
                "csharp_aspnet_route_template_shape",
                &["route_template_shape:", "class_route_template_shape:"],
            ),
            (
                "csharp_aspnet_action_shape",
                &["return_shape:", "parameter_shape:"],
            ),
        ],
        "framework:aspnetcore.minimal_route" => &[
            ("csharp_minimal_route_method", &["http_method:"]),
            (
                "csharp_minimal_route_template_shape",
                &["route_template_shape:"],
            ),
        ],
        "framework:aspnetcore.controller" => {
            &[("csharp_controller_class_shape", &["class_shape:"])]
        }
        "framework:efcore.db_context" | "framework:efcore.entity_set" => &[(
            "csharp_efcore_member_shape",
            &["efcore_entity_type_shape:", "class_shape:"],
        )],
        "framework:xunit.test" | "framework:nunit.test" | "framework:mstest.test" => {
            &[("csharp_test_data_shape", &["test_data_shape:"])]
        }
        _ => &[],
    }
}

/// A unit's file language is `c` or `cpp`; both belong to the shared C/C++
/// bounded preview slice.
fn is_c_cpp_language(language: &str) -> bool {
    language == "c" || language == "cpp"
}

fn cpp_context_variation_slots(
    key: &FamilyKey,
    evidence: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<VariationSlot> {
    if !is_c_cpp_language(&key.language) {
        return Vec::new();
    }
    cpp_variation_feature_prefixes(key.framework_role.as_str())
        .iter()
        .filter_map(|(slot_name, prefixes)| {
            let profiles = evidence
                .iter()
                .map(|item| prefixed_feature_profile(item, features_by_unit, prefixes))
                .collect::<BTreeSet<_>>();
            let has_context = profiles.iter().any(|profile| !profile.is_empty());
            (has_context && profiles.len() > 1).then(|| VariationSlot {
                slot_id: format!("slot:{slot_name}"),
                description: format!(
                    "variation:{slot_name}:context metadata differs across supported members"
                ),
            })
        })
        .collect()
}

fn cpp_variation_feature_prefixes(
    framework_role: &str,
) -> &'static [(&'static str, &'static [&'static str])] {
    match framework_role {
        "framework:gtest.test"
        | "framework:catch2.test"
        | "framework:doctest.test"
        | "framework:boost_test.test" => &[
            ("cpp_test_macro_shape", &["test_macro:"]),
            ("cpp_test_name_shape", &["test_name_shape:"]),
        ],
        "framework:gtest.fixture" => &[("cpp_test_fixture_shape", &["fixture_shape:"])],
        "framework:boost_test.suite" => &[("cpp_test_suite_shape", &["suite_shape:"])],
        _ => &[],
    }
}

pub fn family_storage_records(claim: &FamilyClaim) -> FamilyStorageRecords {
    let variation_evidence_indexes = framework_anchor_variation_evidence_indexes(claim);
    let members = claim
        .evidence
        .iter()
        .map(|evidence| IndexedFamilyMemberRecord {
            family_id: claim.family_id.clone(),
            code_unit_id: evidence.code_unit_id.clone(),
            role: claim.framework_role.clone(),
        })
        .collect::<Vec<_>>();
    let variation_slots = claim
        .variation_slots
        .iter()
        .map(|slot| IndexedVariationSlotRecord {
            family_id: claim.family_id.clone(),
            slot_id: slot.slot_id.clone(),
            description: slot.description.clone(),
        })
        .collect::<Vec<_>>();
    let evidence = claim
        .evidence
        .iter()
        .enumerate()
        .map(|(index, evidence)| IndexedFamilyEvidenceRecord {
            evidence_id: format!(
                "family-evidence:{}:{index:06}",
                stable_token(&claim.family_id)
            ),
            family_id: claim.family_id.clone(),
            code_unit_id: evidence.code_unit_id.clone(),
            covered_claims: family_evidence_covered_claims(index, &variation_evidence_indexes),
            path: evidence.path.clone(),
            content_hash: evidence.content_hash.clone(),
            start_byte: evidence.start_byte,
            end_byte: evidence.end_byte,
            note: format!("{} support evidence", claim.classification),
        })
        .collect::<Vec<_>>();

    FamilyStorageRecords {
        family: IndexedFamilyRecord {
            family_id: claim.family_id.clone(),
            classification: claim.classification.clone(),
            prevalence: claim.prevalence.clone(),
        },
        members,
        variation_slots,
        evidence,
    }
}

fn framework_roles_by_unit(facts: &[SemanticFact]) -> BTreeMap<String, BTreeSet<String>> {
    let mut roles = BTreeMap::new();
    for fact in facts {
        if fact.kind != SemanticFactKind::FrameworkRole
            || fact.certainty != FactCertainty::FrameworkHeuristic
        {
            continue;
        }
        let Some(target) = fact.target.as_ref() else {
            continue;
        };
        roles
            .entry(fact.subject.clone())
            .or_insert_with(BTreeSet::new)
            .insert(target.as_str().to_string());
    }
    roles
}

fn eligible_support_by_unit(
    units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut supported = BTreeMap::new();
    for fact in facts {
        if !fact.certainty.supports_family_membership()
            || matches!(
                fact.kind,
                SemanticFactKind::FrameworkRole
                    | SemanticFactKind::ProjectConfig
                    | SemanticFactKind::Unknown
            )
        {
            continue;
        }
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        if fact.evidence.provenance.path == unit.path
            && fact.evidence.provenance.content_hash == unit.content_hash
            && fact.evidence.range.start_byte == unit.start_byte
            && fact.evidence.range.end_byte == unit.end_byte
            && support_fact_is_role_compatible(fact, framework_role)
        {
            supported
                .entry(code_unit_id.to_string())
                .or_insert_with(BTreeSet::new)
                .insert(
                    fact.target
                        .as_ref()
                        .map(|target| target.as_str().to_string())
                        .unwrap_or_else(|| fact.kind.as_protocol_str().to_string()),
                );
        }
    }
    supported
}

fn family_features_by_unit(
    units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut features = BTreeMap::new();

    for unit in units {
        if !family_eligible_kind(&unit.kind) {
            continue;
        }
        let entry = features
            .entry(unit.id.clone())
            .or_insert_with(BTreeSet::new);
        entry.insert(format!("language:{}", stable_token(&unit.language)));
        entry.insert(format!("unit_kind:{}", stable_token(&unit.kind)));
        entry.insert(format!(
            "ast_skeleton:{}",
            stable_token(&normalized_shape(
                &unit.kind,
                role_by_unit
                    .get(&unit.id)
                    .and_then(single_framework_role)
                    .unwrap_or("unknown")
            ))
        ));
        entry.insert(format!("path_context:{}", path_context(&unit.path)));
        if let Some(framework_role) = role_by_unit.get(&unit.id).and_then(single_framework_role) {
            entry.insert(format!("framework_role:{}", stable_token(framework_role)));
        }
        if let Some(targets) = support_targets_by_unit.get(&unit.id) {
            for target in targets {
                entry.insert(format!("support_exact:{}", stable_token(target)));
                if let Some(framework_role) =
                    role_by_unit.get(&unit.id).and_then(single_framework_role)
                {
                    entry.insert(format!(
                        "support_family:{}",
                        stable_token(&support_target_family(target, framework_role))
                    ));
                }
            }
        }
    }

    for fact in facts {
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let entry = features
            .entry(code_unit_id.to_string())
            .or_insert_with(BTreeSet::new);
        if is_tsjs_language_name(&unit.language) {
            add_tsjs_family_features(entry, fact, role_by_unit, support_targets_by_unit);
            continue;
        }
        if unit.language == "java" {
            add_java_family_features(entry, fact, role_by_unit, support_targets_by_unit);
            continue;
        }
        if unit.language == "csharp" {
            add_csharp_family_features(entry, fact, role_by_unit, support_targets_by_unit);
            continue;
        }
        if is_c_cpp_language(&unit.language) {
            add_cpp_family_features(entry, fact, role_by_unit, support_targets_by_unit);
            continue;
        }
        if unit.language == "rust" {
            add_rust_family_features(entry, fact, role_by_unit, support_targets_by_unit);
            continue;
        }
        if unit.language != "python" {
            continue;
        }
        for anchor_kind in fact
            .assumptions
            .iter()
            .filter_map(|assumption| assumption.strip_prefix("python_anchor_kind="))
        {
            entry.insert(format!("anchor:{}", stable_token(anchor_kind)));
            let target = fact.target.as_ref().map(|target| target.as_str());
            match anchor_kind {
                "fastapi_route_decorator" | "decorator_binding" => {
                    entry.insert(format!(
                        "decorator_shape:{}",
                        stable_token("fastapi_route_decorator")
                    ));
                    if let Some(target) = target {
                        entry.insert(format!("decorator_anchor:{}", stable_token(target)));
                    }
                }
                "import_binding"
                | "repo_local_import_binding"
                | "repo_local_import_symbol"
                | "dynamic_import_literal" => {
                    if let Some(target) = target {
                        entry.insert(format!("import_context:{}", stable_token(target)));
                    }
                }
                "fastapi_service_call"
                | "call_target"
                | "sqlalchemy_select"
                | "sqlalchemy_session_call" => {
                    entry.insert(format!("call_shape_kind:{}", stable_token(anchor_kind)));
                    let is_support_target = target.is_some_and(|target| {
                        support_targets_by_unit
                            .get(code_unit_id)
                            .is_some_and(|targets| targets.contains(target))
                    });
                    if let Some(target) = target.filter(|_| !is_support_target) {
                        entry.insert(format!("call_shape:{}", stable_token(target)));
                    }
                }
                "fastapi_dependency"
                | "fastapi_dependency_target"
                | "fastapi_http_exception"
                | "fastapi_http_exception_status"
                | "fastapi_cookie_param"
                | "fastapi_header_param"
                | "fastapi_path_param"
                | "fastapi_query_param"
                | "fastapi_request_body_model"
                | "fastapi_response_model"
                | "sqlalchemy_relationship" => {
                    entry.insert(format!("effect_shape:{}", stable_token(anchor_kind)));
                    let marker = target
                        .map(|target| format!("{anchor_kind}:{target}"))
                        .unwrap_or_else(|| anchor_kind.to_string());
                    entry.insert(format!("effect_marker:{}", stable_token(&marker)));
                }
                "pytest_fixture_edge"
                | "pytest_conftest_fixture_edge"
                | "pytest_builtin_fixture_context"
                | "pytest_plugin_fixture_context"
                | "pytest_parametrize_arg" => {
                    entry.insert(format!("fixture_shape:{}", stable_token(anchor_kind)));
                    if let Some(target) = target {
                        entry.insert(format!("fixture_context:{}", stable_token(target)));
                    }
                }
                "class_base" => {
                    if let Some(target) = target {
                        entry.insert(format!(
                            "class_base:{}",
                            stable_token(&support_target_family(
                                target,
                                role_by_unit
                                    .get(code_unit_id)
                                    .and_then(single_framework_role)
                                    .unwrap_or("")
                            ))
                        ));
                    }
                }
                "pydantic_field"
                | "pydantic_field_type"
                | "pydantic_model_config"
                | "pydantic_config_class"
                | "pydantic_computed_field"
                | "pydantic_validator"
                | "pydantic_model_validator"
                | "sqlalchemy_mapped_field"
                | "sqlalchemy_mapped_column" => {
                    entry.insert(format!("model_shape:{}", stable_token(anchor_kind)));
                    let marker = target
                        .map(|target| format!("{anchor_kind}:{target}"))
                        .unwrap_or_else(|| anchor_kind.to_string());
                    entry.insert(format!("model_context:{}", stable_token(&marker)));
                }
                "django_url_route" | "flask_route_decorator" => {
                    entry.insert("route_path_shape:literal".to_string());
                }
                "flask_route_method" => {
                    if let Some(method) =
                        target.and_then(|target| target.strip_prefix("flask.http_method."))
                    {
                        entry.insert(format!("http_method:{}", stable_token(method)));
                    }
                }
                "django_model_field" => {
                    if let Some(bucket) =
                        target.and_then(|target| target.strip_prefix("django.field_count."))
                    {
                        entry.insert(format!("django_field_count_shape:{}", stable_token(bucket)));
                    }
                }
                "django_model_meta" => {
                    entry.insert("django_meta_shape:present".to_string());
                }
                "django_test_method" => {
                    if let Some(bucket) =
                        target.and_then(|target| target.strip_prefix("django.test_method_count."))
                    {
                        entry.insert(format!("test_method_count_shape:{}", stable_token(bucket)));
                    }
                }
                "unittest_fixture" => {
                    if let Some(shape) =
                        target.and_then(|target| target.strip_prefix("unittest.fixture."))
                    {
                        entry.insert(format!("unittest_fixture_shape:{}", stable_token(shape)));
                    }
                }
                "cli_param_count" => {
                    if let Some(bucket) =
                        target.and_then(|target| target.strip_prefix("cli.param_count."))
                    {
                        entry.insert(format!("cli_param_count_shape:{}", stable_token(bucket)));
                    }
                }
                "celery_task_decorator" => {
                    if let Some(target) = target
                        .filter(|target| matches!(*target, "celery.task" | "celery.shared_task"))
                    {
                        entry.insert(format!("celery_task_shape:{}", stable_token(target)));
                    }
                }
                _ => {}
            }
        }
    }

    features
}

fn add_tsjs_family_features(
    entry: &mut BTreeSet<String>,
    fact: &SemanticFact,
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) {
    let code_unit_id = fact.evidence.code_unit_id.as_str();
    let framework_role = role_by_unit
        .get(code_unit_id)
        .and_then(single_framework_role)
        .unwrap_or("");

    for anchor_kind in fact
        .assumptions
        .iter()
        .filter_map(|assumption| assumption.strip_prefix("tsjs_anchor_kind="))
    {
        entry.insert(format!("anchor_kind:{}", stable_token(anchor_kind)));
    }
    add_variation_features_from_assumptions(
        entry,
        &fact.assumptions,
        &[
            ("route_method=", "route_method:"),
            ("route_path_shape=", "route_path_shape:"),
            ("handler_shape=", "handler_shape:"),
            ("runner_kind=", "runner_kind:"),
            ("test_shape=", "test_shape:"),
            ("async_shape=", "async_shape:"),
            ("import_context=", "import_context:"),
            ("path_alias=", "import_context:path_alias_"),
            ("router_kind=", "router_kind:"),
            ("file_convention=", "file_convention:"),
            ("http_method=", "http_method:"),
            ("component_shape=", "component_shape:"),
            ("response_shape=", "response_shape:"),
            ("fetch_shape=", "fetch_shape:"),
            ("server_client_directive=", "server_client_directive:"),
            ("schema_present=", "schema_present:"),
            ("opts_handler_present=", "opts_handler_present:"),
            ("reply_shape=", "reply_shape:"),
            ("plugin_context=", "plugin_context:"),
            ("prefix_unknown=", "prefix_unknown:"),
            ("model_name=", "model_name:"),
            ("operation=", "operation:"),
            ("where_shape=", "where_shape:"),
            ("select_include_shape=", "select_include_shape:"),
            ("transaction_shape=", "transaction_shape:"),
            ("raw_sql_present=", "raw_sql_present:"),
            ("table_name=", "table_name:"),
            ("returning_shape=", "returning_shape:"),
            ("join_shape=", "join_shape:"),
            ("sql_template_present=", "sql_template_present:"),
            ("zod_builder=", "zod_builder:"),
            ("zod_field_count_shape=", "zod_field_count_shape:"),
            ("class_route_path_shape=", "class_route_path_shape:"),
            ("nest_module_shape=", "nest_module_shape:"),
        ],
        stable_token,
    );

    if let Some(target) = fact.target.as_ref().map(|target| target.as_str()) {
        entry.insert(format!("framework_api_anchor:{}", stable_token(target)));
        if let Some(method) = target.strip_prefix("express.route.") {
            entry.insert("anchor_kind:express_route_call".to_string());
            entry.insert(format!("route_method:{}", stable_token(method)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        } else if let Some(test_target) = target.strip_prefix("jest_vitest.") {
            entry.insert("runner_kind:jest_vitest".to_string());
            entry.insert(format!("test_shape:{}", stable_token(test_target)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        } else if target.starts_with("next.") {
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        } else if let Some(method) = target.strip_prefix("fastify.route.") {
            entry.insert("anchor_kind:fastify_route_call".to_string());
            entry.insert(format!("route_method:{}", stable_token(method)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        } else if let Some(test_target) = target
            .strip_prefix("mocha.")
            .or_else(|| target.strip_prefix("node_test."))
        {
            entry.insert(format!("test_shape:{}", stable_token(test_target)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        } else if target.starts_with("prisma.")
            || target.starts_with("drizzle.")
            || target.starts_with("zod.")
            || target.starts_with("nestjs.")
            || target.starts_with("hono.")
        {
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        }
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if !is_support_target {
            entry.insert(format!("import_context:{}", stable_token(target)));
        }
    }

    add_classified_unknown_family_features(entry, "typescript", fact, framework_role);
}

fn add_java_family_features(
    entry: &mut BTreeSet<String>,
    fact: &SemanticFact,
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) {
    let code_unit_id = fact.evidence.code_unit_id.as_str();
    let framework_role = role_by_unit
        .get(code_unit_id)
        .and_then(single_framework_role)
        .unwrap_or("");

    add_variation_features_from_assumptions(
        entry,
        &fact.assumptions,
        java::COPIED_ASSUMPTION_FEATURES,
        stable_token,
    );
    if let Some(target) = fact.target.as_ref().map(|target| target.as_str()) {
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if is_support_target {
            entry.insert(format!("framework_api_anchor:{}", stable_token(target)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        }
    }
    add_classified_unknown_family_features(entry, "java", fact, framework_role);
}

fn add_csharp_family_features(
    entry: &mut BTreeSet<String>,
    fact: &SemanticFact,
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) {
    let code_unit_id = fact.evidence.code_unit_id.as_str();
    let framework_role = role_by_unit
        .get(code_unit_id)
        .and_then(single_framework_role)
        .unwrap_or("");

    add_variation_features_from_assumptions(
        entry,
        &fact.assumptions,
        &[
            ("csharp_anchor_kind=", "anchor_kind:"),
            ("aspnet_attribute=", "aspnet_attribute:"),
            ("http_method=", "http_method:"),
            ("route_template_shape=", "route_template_shape:"),
            ("class_route_template_shape=", "class_route_template_shape:"),
            ("test_attribute=", "test_attribute:"),
            ("test_data_shape=", "test_data_shape:"),
            ("csharp_visibility_shape=", "visibility_shape:"),
            ("csharp_class_shape=", "class_shape:"),
            ("csharp_return_shape=", "return_shape:"),
            ("csharp_parameter_shape=", "parameter_shape:"),
            ("efcore_entity_type_shape=", "efcore_entity_type_shape:"),
        ],
        stable_token,
    );
    if let Some(target) = fact.target.as_ref().map(|target| target.as_str()) {
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if is_support_target {
            entry.insert(format!("framework_api_anchor:{}", stable_token(target)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        }
    }
    add_classified_unknown_family_features(entry, "csharp", fact, framework_role);
}

fn add_cpp_family_features(
    entry: &mut BTreeSet<String>,
    fact: &SemanticFact,
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) {
    let code_unit_id = fact.evidence.code_unit_id.as_str();
    let framework_role = role_by_unit
        .get(code_unit_id)
        .and_then(single_framework_role)
        .unwrap_or("");

    add_variation_features_from_assumptions(
        entry,
        &fact.assumptions,
        &[
            ("cpp_anchor_kind=", "anchor_kind:"),
            ("test_framework=", "test_framework:"),
            ("test_macro=", "test_macro:"),
            ("test_name_shape=", "test_name_shape:"),
            ("fixture_shape=", "fixture_shape:"),
            ("suite_shape=", "suite_shape:"),
        ],
        stable_token,
    );
    if let Some(target) = fact.target.as_ref().map(|target| target.as_str()) {
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if is_support_target {
            entry.insert(format!("framework_api_anchor:{}", stable_token(target)));
            entry.insert(format!(
                "support_family:{}",
                stable_token(&support_target_family(target, framework_role))
            ));
        }
    }
    add_classified_unknown_family_features(entry, "cpp", fact, framework_role);
}

fn add_rust_family_features(
    entry: &mut BTreeSet<String>,
    fact: &SemanticFact,
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
    support_targets_by_unit: &BTreeMap<String, BTreeSet<String>>,
) {
    let code_unit_id = fact.evidence.code_unit_id.as_str();
    let framework_role = role_by_unit
        .get(code_unit_id)
        .and_then(single_framework_role)
        .unwrap_or("");
    add_variation_features_from_assumptions(
        entry,
        &fact.assumptions,
        &[
            ("rust_anchor_kind=", "anchor_kind:"),
            ("rust_signature_shape=", "signature_shape:"),
            ("rust_visibility_shape=", "visibility_shape:"),
            ("rust_arity_shape=", "arity_shape:"),
            ("rust_return_shape=", "return_shape:"),
            ("rust_attribute_shape=", "attribute_shape:"),
            ("rust_error_shape=", "error_shape:"),
            ("rust_call_shape=", "call_shape:"),
            ("rust_control_shape=", "control_shape:"),
            ("rust_test_shape=", "test_shape:"),
            ("rust_path_context=", "path_context:"),
            ("rust_module_resolution=", "import_context:"),
            ("serde_attr_shape=", "serde_attr_shape:"),
            ("error_message_shape=", "error_message_shape:"),
            ("clap_attr_shape=", "clap_attr_shape:"),
            ("http_method=", "http_method:"),
            ("route_path_shape=", "route_path_shape:"),
        ],
        stable_token,
    );
    if let Some(target) = fact.target.as_ref().map(|target| target.as_str()) {
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if is_support_target {
            entry.insert(format!("framework_api_anchor:{}", stable_token(target)));
        } else if target.starts_with("module:") {
            entry.insert(format!("import_context:{}", stable_token(target)));
        }
    }
    add_classified_unknown_family_features(entry, "rust", fact, framework_role);
}

fn add_classified_unknown_family_features(
    entry: &mut BTreeSet<String>,
    language: &str,
    fact: &SemanticFact,
    framework_role: &str,
) {
    let Some(unknown) = classify_family_unknown_fact(language, fact, framework_role) else {
        return;
    };
    entry.insert(format!(
        "unknown_reason:{}",
        stable_token(unknown.reason.as_protocol_str())
    ));
    if unknown.claim_impact() == Some(ClaimImpact::Blocking) {
        entry.insert(format!(
            "unknown_blocker:{}",
            stable_token(unknown.reason.as_protocol_str())
        ));
    }
}

fn family_blocking_unknowns_by_unit(
    units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, Vec<ClaimUnknown>> {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut blocking: BTreeMap<String, Vec<ClaimUnknown>> = BTreeMap::new();

    for fact in facts {
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        let unknown = classify_family_unknown_fact(&unit.language, fact, framework_role)
            .filter(|unknown| unknown.claim_impact() == Some(ClaimImpact::Blocking));
        if let Some(unknown) = unknown {
            blocking
                .entry(code_unit_id.to_string())
                .or_default()
                .push(unknown);
        }
    }

    blocking
}

fn rust_repository_blocking_unknowns(facts: &[SemanticFact]) -> Vec<ClaimUnknown> {
    let mut unknowns = facts
        .iter()
        .filter_map(rust_repository_blocking_unknown)
        .collect::<Vec<_>>();
    unknowns.sort_by(|left, right| {
        (
            left.affected_claim.as_str(),
            left.class.as_protocol_str(),
            left.reason.as_protocol_str(),
        )
            .cmp(&(
                right.affected_claim.as_str(),
                right.class.as_protocol_str(),
                right.reason.as_protocol_str(),
            ))
    });
    unknowns.dedup();
    unknowns
}

fn rust_repository_blocking_unknown(fact: &SemanticFact) -> Option<ClaimUnknown> {
    if fact.kind != SemanticFactKind::Unknown
        || fact.certainty != FactCertainty::Unknown
        || fact.origin.engine != RUST_ANCHOR_ENGINE
        || fact.origin.method != RUST_ANCHOR_METHOD
    {
        return None;
    }
    let reason = fact
        .target
        .as_ref()
        .and_then(|target| UnknownReasonCode::parse_protocol_str(target.as_str()).ok())?;
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("rust_family_membership");
    if reason != UnknownReasonCode::BuildVariantAmbiguity
        || affected_claim != "rust_build_variant"
        || fact.evidence.provenance.path != "Cargo.toml"
    {
        return None;
    }
    Some(ClaimUnknown {
        class: ClaimImpact::Blocking.as_legacy_unknown_class(),
        reason,
        affected_claim: "rust_build_variant".to_string(),
        recovery: Some("remove or resolve root Cargo build/target variant ambiguity".to_string()),
    })
}

fn family_non_blocking_unknowns_by_unit(
    units: &[IndexedCodeUnitRecord],
    facts: &[SemanticFact],
    role_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, Vec<ClaimUnknown>> {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut non_blocking: BTreeMap<String, Vec<ClaimUnknown>> = BTreeMap::new();

    for fact in facts {
        let code_unit_id = fact.evidence.code_unit_id.as_str();
        let Some(unit) = unit_by_id.get(code_unit_id) else {
            continue;
        };
        if !fact_evidence_is_within_unit(fact, unit) {
            continue;
        }
        let Some(framework_role) = role_by_unit
            .get(code_unit_id)
            .and_then(single_framework_role)
        else {
            continue;
        };
        let unknown = classify_family_unknown_fact(&unit.language, fact, framework_role)
            .filter(|unknown| unknown.claim_impact() == Some(ClaimImpact::NonBlocking));
        if let Some(unknown) = unknown {
            non_blocking
                .entry(code_unit_id.to_string())
                .or_default()
                .push(unknown);
        }
    }

    non_blocking
}

pub(crate) fn family_unknown_blocks_claim(
    language: &str,
    fact: &SemanticFact,
    framework_role: &str,
) -> bool {
    classify_family_unknown_fact(language, fact, framework_role)
        .is_some_and(|unknown| unknown.claim_impact() == Some(ClaimImpact::Blocking))
}

fn classify_family_unknown_fact(
    language: &str,
    fact: &SemanticFact,
    framework_role: &str,
) -> Option<ClaimUnknown> {
    if fact.kind != SemanticFactKind::Unknown && fact.certainty != FactCertainty::Unknown {
        return None;
    }
    let reason = fact
        .target
        .as_ref()
        .and_then(|target| UnknownReasonCode::parse_protocol_str(target.as_str()).ok())?;
    let domain = FamilyUnknownDomain::from_language_and_origin(
        language,
        &fact.origin.engine,
        &fact.origin.method,
    )?;
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or_else(|| domain.default_affected_claim());

    classify_family_unknown_with_domain(domain, reason, affected_claim, framework_role, Some(fact))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FamilyUnknownDomain {
    Python,
    Tsjs,
    Java,
    CSharp,
    Cpp,
    Rust,
}

impl FamilyUnknownDomain {
    fn from_language_and_origin(
        language: &str,
        origin_engine: &str,
        origin_method: &str,
    ) -> Option<Self> {
        if language == "python" {
            return (origin_engine == PYTHON_ANCHOR_ENGINE).then_some(Self::Python);
        }
        if is_tsjs_language_name(language) {
            return (origin_engine == TSJS_ANCHOR_ENGINE).then_some(Self::Tsjs);
        }
        if language == "java" {
            return (origin_engine == JAVA_ANCHOR_ENGINE && origin_method == JAVA_ANCHOR_METHOD)
                .then_some(Self::Java);
        }
        if language == "csharp" {
            return (origin_engine == CSHARP_ANCHOR_ENGINE
                && origin_method == CSHARP_ANCHOR_METHOD)
                .then_some(Self::CSharp);
        }
        if is_c_cpp_language(language) {
            return (origin_engine == CPP_ANCHOR_ENGINE && origin_method == CPP_ANCHOR_METHOD)
                .then_some(Self::Cpp);
        }
        if language == "rust" {
            return (origin_engine == RUST_ANCHOR_ENGINE).then_some(Self::Rust);
        }
        None
    }

    fn default_affected_claim(self) -> &'static str {
        match self {
            Self::Python => "python_family_membership",
            Self::Tsjs => "tsjs_family_membership",
            Self::Java => "java_family_membership",
            Self::CSharp => "csharp_family_membership",
            Self::Cpp => "cpp_family_membership",
            Self::Rust => "rust_family_membership",
        }
    }

    fn recovery_scope(self) -> &'static str {
        match self {
            Self::Python => "Python",
            Self::Tsjs => "TS/JS",
            Self::Java => "Java/Spring",
            Self::CSharp => "C#",
            Self::Cpp => "C/C++",
            Self::Rust => "Rust",
        }
    }

    fn reason_blocks(
        self,
        reason: UnknownReasonCode,
        affected_claim: &str,
        framework_role: &str,
    ) -> bool {
        match self {
            Self::Python => python_unknown_reason_blocks_family_membership(
                reason,
                affected_claim,
                framework_role,
            ),
            Self::Tsjs => {
                tsjs_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
            }
            Self::Java => {
                java_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
            }
            Self::CSharp => csharp_unknown_reason_blocks_family_membership(
                reason,
                affected_claim,
                framework_role,
            ),
            Self::Cpp => {
                cpp_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
            }
            Self::Rust => {
                rust_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
            }
        }
    }

    fn is_non_blocking_subclaim(
        self,
        reason: UnknownReasonCode,
        affected_claim: &str,
        framework_role: &str,
    ) -> bool {
        match self {
            Self::Python => python_unknown_is_non_blocking_family_subclaim(
                reason,
                affected_claim,
                framework_role,
            ),
            Self::Tsjs => {
                tsjs_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
            }
            Self::Java => {
                java_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
            }
            Self::CSharp => csharp_unknown_is_non_blocking_family_subclaim(
                reason,
                affected_claim,
                framework_role,
            ),
            Self::Cpp => {
                cpp_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
            }
            Self::Rust => {
                rust_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
            }
        }
    }
}

fn classify_family_unknown_with_domain(
    domain: FamilyUnknownDomain,
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
    fact: Option<&SemanticFact>,
) -> Option<ClaimUnknown> {
    if domain.reason_blocks(reason, affected_claim, framework_role) {
        let recovery = if domain == FamilyUnknownDomain::Rust {
            fact.map(|fact| rust_blocking_unknown_recovery(fact, affected_claim))
                .unwrap_or_else(|| {
                    format!(
                        "resolve the blocking {} UNKNOWN before claiming a family",
                        domain.recovery_scope()
                    )
                })
        } else {
            format!(
                "resolve the blocking {} UNKNOWN before claiming a family",
                domain.recovery_scope()
            )
        };
        return Some(ClaimUnknown {
            class: ClaimImpact::Blocking.as_legacy_unknown_class(),
            reason,
            affected_claim: affected_claim.to_string(),
            recovery: Some(recovery),
        });
    }
    if domain.is_non_blocking_subclaim(reason, affected_claim, framework_role) {
        return Some(ClaimUnknown {
            class: ClaimImpact::NonBlocking.as_legacy_unknown_class(),
            reason,
            affected_claim: affected_claim.to_string(),
            recovery: Some(format!(
                "resolve this {} subclaim before relying on it",
                domain.recovery_scope()
            )),
        });
    }
    None
}

fn python_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::DynamicImport
        | UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence
        | UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency => {
            python_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::PytestFixtureInjection => framework_role.starts_with("framework:pytest"),
        UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn python_unknown_affected_claim_blocks_family(affected_claim: &str, framework_role: &str) -> bool {
    match affected_claim {
        "fastapi_dependency_target" | "pydantic_validator_side_effects" => false,
        "python_django_string_dispatch"
        | "python_unittest_patch_target"
        | "python_celery_runtime_routing"
        | "python_django_settings_behavior" => false,
        "pytest_fixture_binding" => framework_role.starts_with("framework:pytest"),
        "python_family_membership"
        | "python_import_resolution"
        | "python_call_target"
        | "python_framework_identity"
        | "python_django_model_identity"
        | "python_django_url_identity"
        | "python_flask_route_identity"
        | "python_cli_command_identity"
        | "python_celery_task_identity" => true,
        claim if claim.starts_with("family:") => true,
        _ => framework_role.starts_with("framework:pytest") && affected_claim.contains("fixture"),
    }
}

fn python_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    matches!(reason, UnknownReasonCode::RuntimeDependencyInjection)
        && framework_role == "framework:fastapi.route"
        && affected_claim == "fastapi_dependency_target"
        || matches!(reason, UnknownReasonCode::FrameworkMagic)
            && framework_role == "framework:pydantic.model"
            && affected_claim == "pydantic_validator_side_effects"
        || matches!(reason, UnknownReasonCode::FrameworkMagic)
            && framework_role.starts_with("framework:django")
            && matches!(
                affected_claim,
                "python_django_string_dispatch" | "python_django_settings_behavior"
            )
        || matches!(reason, UnknownReasonCode::MonkeyPatch)
            && framework_role == "framework:unittest.test"
            && affected_claim == "python_unittest_patch_target"
        || matches!(reason, UnknownReasonCode::FrameworkMagic)
            && framework_role == "framework:celery.task"
            && affected_claim == "python_celery_runtime_routing"
}

fn tsjs_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::DynamicImport
        | UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence => {
            tsjs_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn tsjs_unknown_affected_claim_blocks_family(affected_claim: &str, framework_role: &str) -> bool {
    match affected_claim {
        "tsjs_family_membership"
        | "tsjs_framework_identity"
        | "tsjs_receiver_binding"
        | "tsjs_runner_binding"
        | "tsjs_support_target"
        | "tsjs_import_resolution"
        | "tsjs_path_alias"
        | "tsjs_reexport_resolution"
        | "next_project_context"
        | "next_route_convention"
        | "next_default_export"
        | "next_pages_api_export"
        | "next_route_handler_export"
        | "next_component_shape"
        | "fastify_route_shape"
        | "fastify_receiver_binding"
        | "fastify_route_method"
        | "prisma_query_shape"
        | "prisma_transaction_shape"
        | "prisma_client_binding"
        | "drizzle_schema_table"
        | "drizzle_table_binding"
        | "drizzle_query_shape"
        | "drizzle_db_binding"
        | "drizzle_transaction_shape"
        | "tsjs_nest_controller_identity"
        | "tsjs_hono_receiver" => true,
        claim if claim.starts_with("family:") => true,
        claim => {
            (framework_role.starts_with("framework:express")
                && (claim.contains("receiver")
                    || claim.contains("method")
                    || claim.contains("framework")))
                || (framework_role.starts_with("framework:jest_vitest")
                    && (claim.contains("runner")
                        || claim.contains("wrapper")
                        || claim.contains("framework")))
        }
    }
}

fn tsjs_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    _framework_role: &str,
) -> bool {
    matches!(
        reason,
        UnknownReasonCode::FrameworkMagic
            | UnknownReasonCode::RuntimeDependencyInjection
            | UnknownReasonCode::BuildVariantAmbiguity
    ) && matches!(
        affected_claim,
        "tsjs_handler_shape"
            | "tsjs_variation_detail"
            | "tsjs_optional_call_target"
            | "tsjs_zod_runtime_refinement"
            | "tsjs_nest_di_resolution"
            | "tsjs_nest_dynamic_module"
    )
}

fn java_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence => {
            java_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::DynamicImport
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn java_unknown_affected_claim_blocks_family(affected_claim: &str, _framework_role: &str) -> bool {
    java::affected_claim_blocks_family(affected_claim)
}

fn java_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    matches!(
        reason,
        UnknownReasonCode::RuntimeDependencyInjection
            | UnknownReasonCode::FrameworkMagic
            | UnknownReasonCode::MacroOrPreprocessor
    ) && matches!(
        affected_claim,
        "java_spring_component_scan"
            | "java_spring_route_path"
            | "java_spring_dependency_injection"
            | "java_spring_proxy_semantics"
            | "java_spring_generated_repository"
            | "java_spring_data_query_derivation"
            | "java_jpa_runtime_mapping"
            | "java_jaxrs_route_path"
            | "java_test_method_source"
            | "java_testng_data_provider"
            | "java_mockito_runtime_mocks"
            | "java_generated_members"
    ) && java_framework_role_is_known(framework_role)
}

fn csharp_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence => {
            csharp_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::DynamicImport
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn csharp_unknown_affected_claim_blocks_family(
    affected_claim: &str,
    _framework_role: &str,
) -> bool {
    match affected_claim {
        "csharp_family_membership"
        | "csharp_attribute_binding"
        | "csharp_controller_identity"
        | "csharp_minimal_api_receiver"
        | "csharp_test_class_identity"
        | "csharp_build_variant" => true,
        claim if claim.starts_with("family:") => true,
        _ => false,
    }
}

fn csharp_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    matches!(
        reason,
        UnknownReasonCode::RuntimeDependencyInjection
            | UnknownReasonCode::FrameworkMagic
            | UnknownReasonCode::MacroOrPreprocessor
    ) && matches!(
        affected_claim,
        "csharp_di_registration"
            | "csharp_aspnet_filter_pipeline"
            | "csharp_aspnet_route_template"
            | "csharp_aspnet_convention_routing"
            | "csharp_partial_external"
            | "csharp_generated_source"
            | "csharp_dynamic_binding"
            | "csharp_test_member_data"
    ) && csharp_framework_role_is_known(framework_role)
}

fn cpp_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::FrameworkMagic
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::StaleEvidence => {
            cpp_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::DynamicImport
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn cpp_unknown_affected_claim_blocks_family(affected_claim: &str, _framework_role: &str) -> bool {
    match affected_claim {
        "cpp_family_membership"
        | "cpp_test_framework_identity"
        | "cpp_build_variant"
        | "cpp_macro_boundary" => true,
        claim if claim.starts_with("family:") => true,
        _ => false,
    }
}

fn cpp_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    matches!(
        reason,
        UnknownReasonCode::FrameworkMagic
            | UnknownReasonCode::MacroOrPreprocessor
            | UnknownReasonCode::MissingProjectConfig
    ) && matches!(
        affected_claim,
        "cpp_generated_code"
            | "cpp_indirect_dispatch"
            | "cpp_signal_slot_string_dispatch"
            | "cpp_project_config"
    ) && cpp_framework_role_is_known(framework_role)
}

fn rust_unknown_reason_blocks_family_membership(
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: &str,
) -> bool {
    match reason {
        UnknownReasonCode::MacroOrPreprocessor
        | UnknownReasonCode::BuildVariantAmbiguity
        | UnknownReasonCode::ConflictingFacts
        | UnknownReasonCode::StaleEvidence
        | UnknownReasonCode::UnresolvedImport
        | UnknownReasonCode::FrameworkMagic => {
            rust_unknown_affected_claim_blocks_family(affected_claim, framework_role)
        }
        UnknownReasonCode::DynamicImport
        | UnknownReasonCode::MonkeyPatch
        | UnknownReasonCode::PytestFixtureInjection
        | UnknownReasonCode::RuntimeDependencyInjection
        | UnknownReasonCode::MissingProjectConfig
        | UnknownReasonCode::MissingDependency
        | UnknownReasonCode::InsufficientSupport => false,
    }
}

fn rust_unknown_affected_claim_blocks_family(affected_claim: &str, framework_role: &str) -> bool {
    match affected_claim {
        // Expansion honesty and axum tower/extractor semantics are recorded as
        // non-blocking subclaims and must never suppress a family.
        "rust_derive_expansion"
        | "rust_axum_middleware_semantics"
        | "rust_axum_extractor_semantics" => false,
        "rust_family_membership"
        | "rust_macro_expansion"
        | "rust_build_variant"
        | "rust_trait_dispatch"
        | "rust_module_resolution"
        | "rust_framework_attribute_binding"
        | "rust_axum_route_identity" => true,
        claim if claim.starts_with("family:") => true,
        _ => rust_role_is_known(framework_role) && affected_claim.starts_with("rust_"),
    }
}

fn rust_unknown_is_non_blocking_family_subclaim(
    reason: UnknownReasonCode,
    affected_claim: &str,
    _framework_role: &str,
) -> bool {
    (matches!(reason, UnknownReasonCode::FrameworkMagic)
        && matches!(
            affected_claim,
            "rust_optional_call_shape"
                | "rust_axum_middleware_semantics"
                | "rust_axum_extractor_semantics"
        ))
        || (matches!(reason, UnknownReasonCode::MacroOrPreprocessor)
            && affected_claim == "rust_derive_expansion")
}

fn rust_blocking_unknown_recovery(fact: &SemanticFact, affected_claim: &str) -> String {
    if affected_claim == "rust_build_variant" {
        if let Some(recovery) = rust_cfg_feature_recovery(&fact.assumptions) {
            return recovery;
        }
        if fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "rust_cfg_predicate=target")
        {
            return "select or model the Rust target cfg before claiming a family".to_string();
        }
        if fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "rust_cfg_predicate=complex")
        {
            return "manually resolve the complex Rust cfg expression before claiming a family"
                .to_string();
        }
    }
    "resolve the blocking Rust UNKNOWN before claiming a family".to_string()
}

fn rust_cfg_feature_recovery(assumptions: &[String]) -> Option<String> {
    let states = assumptions
        .iter()
        .filter_map(|assumption| assumption.strip_prefix("rust_cfg_feature_declared="))
        .map(|value| {
            let (feature, state) = value.split_once(':').unwrap_or((value, "unknown"));
            let state = match state {
                "true" => "declared",
                "false" => "undeclared",
                _ => "unknown",
            };
            format!("{feature}:{state}")
        })
        .collect::<BTreeSet<_>>();
    if states.is_empty() {
        None
    } else {
        Some(format!(
            "resolve Rust cfg feature gate before claiming a family ({})",
            states.into_iter().collect::<Vec<_>>().join(",")
        ))
    }
}

pub(crate) fn classify_unknown_family_effect(
    language: &str,
    reason: UnknownReasonCode,
    affected_claim: &str,
    framework_role: Option<&str>,
    origin_engine: &str,
    origin_method: &str,
) -> Option<ClaimUnknown> {
    let domain =
        FamilyUnknownDomain::from_language_and_origin(language, origin_engine, origin_method)?;
    classify_family_unknown_with_domain(domain, reason, affected_claim, framework_role?, None)
}

fn fact_evidence_is_within_unit(fact: &SemanticFact, unit: &IndexedCodeUnitRecord) -> bool {
    fact.evidence.provenance.path == unit.path
        && fact.evidence.provenance.content_hash == unit.content_hash
        && fact.evidence.range.start_byte >= unit.start_byte
        && fact.evidence.range.end_byte <= unit.end_byte
}

fn complete_link_family_clusters(
    key: &FamilyKey,
    evidence: Vec<FamilyEvidence>,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Vec<FamilyEvidence>> {
    let mut clusters: Vec<Vec<FamilyEvidence>> = Vec::new();
    for item in evidence {
        if let Some(cluster) = clusters.iter_mut().find(|cluster| {
            cluster
                .iter()
                .all(|other| evidence_pair_is_compatible(key, &item, other, features_by_unit))
        }) {
            cluster.push(item);
        } else {
            clusters.push(vec![item]);
        }
    }
    clusters
}

fn evidence_pair_is_compatible(
    key: &FamilyKey,
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let left_groups = prefixed_features(left, features_by_unit, "support_family:");
    let right_groups = prefixed_features(right, features_by_unit, "support_family:");
    if left_groups.is_empty() || left_groups.is_disjoint(&right_groups) {
        return false;
    }

    let left_roles = prefixed_features(left, features_by_unit, "framework_role:");
    let right_roles = prefixed_features(right, features_by_unit, "framework_role:");
    if left_roles != right_roles {
        return false;
    }

    if key.language == "python" {
        return python_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    if is_tsjs_language_name(&key.language) {
        return tsjs_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    if key.language == "java" {
        return java_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    if key.language == "csharp" {
        return csharp_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    if is_c_cpp_language(&key.language) {
        return cpp_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    if key.language == "rust" {
        return rust_evidence_pair_is_compatible(left, right, features_by_unit);
    }
    true
}

fn python_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let left_groups = prefixed_features(left, features_by_unit, "support_family:");
    let right_groups = prefixed_features(right, features_by_unit, "support_family:");
    if left_groups.is_empty() || left_groups.is_disjoint(&right_groups) {
        return false;
    }

    let left_roles = prefixed_features(left, features_by_unit, "framework_role:");
    let right_roles = prefixed_features(right, features_by_unit, "framework_role:");
    if left_roles != right_roles {
        return false;
    }

    let role = left_roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        "framework_fastapi_route" => {
            equal_feature_profiles(left, right, features_by_unit, &["decorator_shape:"])
        }
        "framework_pytest_test" | "framework_pytest_fixture" => {
            non_builtin_pytest_fixture_context(left, features_by_unit)
                == non_builtin_pytest_fixture_context(right, features_by_unit)
        }
        "framework_pydantic_model" => {
            equal_feature_profiles(left, right, features_by_unit, &["class_base:"])
        }
        "framework_sqlalchemy_model" | "framework_sqlalchemy_repository_method" => {
            equal_feature_profiles(left, right, features_by_unit, &[])
        }
        "framework_django_url_pattern" => {
            equal_feature_profiles(left, right, features_by_unit, &["route_path_shape:"])
        }
        "framework_flask_route" => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["http_method:", "route_path_shape:"],
        ),
        "framework_django_model"
        | "framework_django_test"
        | "framework_unittest_test"
        | "framework_click_command"
        | "framework_typer_command"
        | "framework_celery_task" => equal_feature_profiles(left, right, features_by_unit, &[]),
        _ => true,
    }
}

fn tsjs_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !prefixed_features(left, features_by_unit, "unknown_blocker:").is_empty()
        || !prefixed_features(right, features_by_unit, "unknown_blocker:").is_empty()
    {
        return false;
    }

    let roles = prefixed_features(left, features_by_unit, "framework_role:");
    let role = roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        "framework_express_route_handler" => {
            equal_feature_profiles(left, right, features_by_unit, &["handler_shape:"])
        }
        "framework_jest_vitest_suite" | "framework_jest_vitest_test" => {
            required_equal_feature_profiles(left, right, features_by_unit, &["runner_kind:"])
                && equal_feature_profiles(
                    left,
                    right,
                    features_by_unit,
                    &["test_shape:", "async_shape:"],
                )
        }
        "framework_next_app_page" | "framework_next_app_layout" | "framework_next_pages_page" => {
            equal_feature_profiles(left, right, features_by_unit, &["component_shape:"])
        }
        "framework_next_route_handler" | "framework_next_pages_api_route" => {
            equal_feature_profiles(left, right, features_by_unit, &["response_shape:"])
        }
        "framework_fastify_route_handler" => {
            equal_feature_profiles(left, right, features_by_unit, &["handler_shape:"])
        }
        "framework_prisma_query" | "framework_prisma_transaction" => {
            equal_feature_profiles(left, right, features_by_unit, &["operation:"])
        }
        "framework_drizzle_schema_table"
        | "framework_drizzle_query"
        | "framework_drizzle_transaction" => {
            equal_feature_profiles(left, right, features_by_unit, &["operation:"])
        }
        "framework_zod_schema" => {
            equal_feature_profiles(left, right, features_by_unit, &["zod_builder:"])
        }
        "framework_nestjs_route" | "framework_hono_route" => required_equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["http_method:", "route_path_shape:"],
        ),
        "framework_nestjs_controller"
        | "framework_nestjs_injectable"
        | "framework_nestjs_module" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
        }
        "framework_react_component" | "framework_react_hook" => false,
        _ => true,
    }
}

fn java_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !prefixed_features(left, features_by_unit, "unknown_blocker:").is_empty()
        || !prefixed_features(right, features_by_unit, "unknown_blocker:").is_empty()
    {
        return false;
    }

    let roles = prefixed_features(left, features_by_unit, "framework_role:");
    let role = roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        "framework_spring_mvc_route" => {
            equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["anchor_kind:", "spring_annotation:"],
            ) && required_equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["http_method:", "route_path_shape:"],
            )
        }
        "framework_spring_component" => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["spring_annotation:", "class_shape:"],
        ),
        "framework_spring_boot_application" | "framework_spring_data_repository" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
        }
        "framework_junit5_test" | "framework_junit4_test" | "framework_testng_test" => {
            required_equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["anchor_kind:", "test_annotation:"],
            )
        }
        "framework_jpa_entity" | "framework_jpa_mapped_superclass" | "framework_jpa_embeddable" => {
            // jakarta and javax entities share identical targets but MUST NOT
            // cluster together: the namespace root keeps them apart.
            equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["support_family:", "jpa_namespace_root:"],
            )
        }
        "framework_jaxrs_resource" => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["support_family:", "class_route_path_shape:"],
        ),
        "framework_jaxrs_resource_method" => {
            equal_feature_profiles(left, right, features_by_unit, &["anchor_kind:"])
                && required_equal_feature_profiles(
                    left,
                    right,
                    features_by_unit,
                    &["http_method:", "route_path_shape:"],
                )
        }
        _ => true,
    }
}

fn csharp_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !prefixed_features(left, features_by_unit, "unknown_blocker:").is_empty()
        || !prefixed_features(right, features_by_unit, "unknown_blocker:").is_empty()
    {
        return false;
    }

    let roles = prefixed_features(left, features_by_unit, "framework_role:");
    let role = roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        "framework_aspnetcore_controller_action" => {
            equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["anchor_kind:", "aspnet_attribute:"],
            ) && required_equal_feature_profiles(
                left,
                right,
                features_by_unit,
                &["http_method:", "route_template_shape:"],
            )
        }
        "framework_aspnetcore_minimal_route" => required_equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["http_method:", "route_template_shape:"],
        ),
        "framework_aspnetcore_controller" => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["aspnet_attribute:", "class_shape:"],
        ),
        "framework_xunit_test" | "framework_nunit_test" | "framework_mstest_test" => {
            equal_feature_profiles(left, right, features_by_unit, &["test_attribute:"])
        }
        "framework_efcore_db_context" | "framework_efcore_entity_set" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
        }
        _ => true,
    }
}

fn cpp_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !prefixed_features(left, features_by_unit, "unknown_blocker:").is_empty()
        || !prefixed_features(right, features_by_unit, "unknown_blocker:").is_empty()
    {
        return false;
    }

    let roles = prefixed_features(left, features_by_unit, "framework_role:");
    let role = roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        "framework_gtest_test"
        | "framework_catch2_test"
        | "framework_doctest_test"
        | "framework_boost_test_test" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
                && required_equal_feature_profiles(
                    left,
                    right,
                    features_by_unit,
                    &["test_framework:", "test_macro:"],
                )
        }
        "framework_gtest_fixture" | "framework_boost_test_suite" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
        }
        _ => true,
    }
}

fn rust_evidence_pair_is_compatible(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if !prefixed_features(left, features_by_unit, "unknown_blocker:").is_empty()
        || !prefixed_features(right, features_by_unit, "unknown_blocker:").is_empty()
    {
        return false;
    }
    let roles = prefixed_features(left, features_by_unit, "framework_role:");
    if roles != prefixed_features(right, features_by_unit, "framework_role:") {
        return false;
    }
    let role = roles.iter().next().map(String::as_str).unwrap_or("");
    match role {
        // serde derive models require a shared support family and a
        // present-and-equal trait/target profile so Serialize-only,
        // Deserialize-only, and both-trait models never merge.
        "framework_serde_model" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
                && required_equal_feature_profiles(
                    left,
                    right,
                    features_by_unit,
                    &["framework_api_anchor:"],
                )
        }
        // axum literal routes require a shared support family and a
        // present-and-equal HTTP method plus literal path shape.
        "framework_axum_route" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
                && required_equal_feature_profiles(
                    left,
                    right,
                    features_by_unit,
                    &["http_method:", "route_path_shape:"],
                )
        }
        "framework_thiserror_error"
        | "framework_clap_parser"
        | "framework_tokio_entry"
        | "framework_tokio_test" => {
            equal_feature_profiles(left, right, features_by_unit, &["support_family:"])
        }
        // Self-dogfood roles keep their structural-shape profile equality.
        _ => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &[
                "anchor_kind:",
                "signature_shape:",
                "visibility_shape:",
                "arity_shape:",
                "return_shape:",
                "attribute_shape:",
                "error_shape:",
                "test_shape:",
            ],
        ),
    }
}

fn equal_feature_profiles(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
    prefixes: &[&str],
) -> bool {
    prefixes.iter().all(|prefix| {
        prefixed_features(left, features_by_unit, prefix)
            == prefixed_features(right, features_by_unit, prefix)
    })
}

fn required_equal_feature_profiles(
    left: &FamilyEvidence,
    right: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
    prefixes: &[&str],
) -> bool {
    prefixes.iter().all(|prefix| {
        let left_features = prefixed_features(left, features_by_unit, prefix);
        !left_features.is_empty()
            && left_features == prefixed_features(right, features_by_unit, prefix)
    })
}

fn prefixed_features(
    evidence: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
    prefix: &str,
) -> BTreeSet<String> {
    features_by_unit
        .get(&evidence.code_unit_id)
        .into_iter()
        .flat_map(|features| features.iter())
        .filter_map(|feature| feature.strip_prefix(prefix).map(str::to_string))
        .collect()
}

fn prefixed_feature_profile(
    evidence: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
    prefixes: &[&str],
) -> BTreeSet<String> {
    prefixes
        .iter()
        .flat_map(|prefix| prefixed_features(evidence, features_by_unit, prefix))
        .collect()
}

fn non_builtin_pytest_fixture_context(
    evidence: &FamilyEvidence,
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    prefixed_features(evidence, features_by_unit, "fixture_context:")
        .into_iter()
        .filter(|value| {
            // External fixture context (built-in or well-known plugin) is
            // metadata-only and must not become a hard family compatibility
            // constraint; only repo-local/conftest fixture edges do.
            !value.starts_with("pytest_builtin_fixture_")
                && !value.starts_with("pytest_plugin_fixture_")
        })
        .collect()
}

fn family_cluster_signature(
    key: &FamilyKey,
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    if key.language == "python" {
        return python_cluster_signature(cluster, features_by_unit);
    }
    if is_tsjs_language_name(&key.language) {
        return tsjs_cluster_signature(cluster, features_by_unit);
    }
    if key.language == "rust" {
        return rust_cluster_signature(cluster, features_by_unit);
    }
    if key.language == "java" {
        return java_cluster_signature(cluster, features_by_unit);
    }
    if key.language == "csharp" {
        return csharp_cluster_signature(cluster, features_by_unit);
    }
    if is_c_cpp_language(&key.language) {
        return cpp_cluster_signature(cluster, features_by_unit);
    }
    "family_cluster".to_string()
}

fn python_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "decorator_shape:",
                "effect_shape:",
                "call_shape_kind:",
                "fixture_shape:",
                "model_shape:",
                "effect_marker:",
                "call_shape:",
                "fixture_context:",
                "model_context:",
                "class_base:",
                "route_path_shape:",
                "http_method:",
                "django_field_count_shape:",
                "django_meta_shape:",
                "test_method_count_shape:",
                "unittest_fixture_shape:",
                "cli_param_count_shape:",
                "celery_task_shape:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "python_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn tsjs_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "anchor_kind:",
                "framework_api_anchor:",
                "route_method:",
                "route_path_shape:",
                "http_method:",
                "handler_shape:",
                "runner_kind:",
                "test_shape:",
                "async_shape:",
                "zod_builder:",
                "path_context:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "tsjs_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn rust_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "anchor_kind:",
                "signature_shape:",
                "visibility_shape:",
                "arity_shape:",
                "return_shape:",
                "attribute_shape:",
                "error_shape:",
                "call_shape:",
                "control_shape:",
                "test_shape:",
                "path_context:",
                "framework_api_anchor:",
                "serde_attr_shape:",
                "error_message_shape:",
                "clap_attr_shape:",
                "http_method:",
                "route_path_shape:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "rust_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn java_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "anchor_kind:",
                "spring_annotation:",
                "http_method:",
                "route_path_shape:",
                "class_route_path_shape:",
                "class_shape:",
                "test_annotation:",
                "test_data_shape:",
                "jpa_namespace_root:",
                "path_context:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "java_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn csharp_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "anchor_kind:",
                "aspnet_attribute:",
                "http_method:",
                "route_template_shape:",
                "test_attribute:",
                "test_data_shape:",
                "class_shape:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "csharp_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn cpp_cluster_signature(
    cluster: &[FamilyEvidence],
    features_by_unit: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let signature_features = cluster
        .iter()
        .flat_map(|evidence| {
            [
                "support_family:",
                "anchor_kind:",
                "test_framework:",
                "test_macro:",
                "test_name_shape:",
                "fixture_shape:",
            ]
            .into_iter()
            .flat_map(move |prefix| prefixed_features(evidence, features_by_unit, prefix))
        })
        .collect::<BTreeSet<_>>();
    if signature_features.is_empty() {
        return "cpp_family_cluster".to_string();
    }
    format!(
        "cluster:{}",
        signature_features.into_iter().collect::<Vec<_>>().join("+")
    )
}

fn support_target_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        "framework:express.route_handler" => "express.route_handler".to_string(),
        "framework:jest_vitest.suite" => "jest_vitest.suite".to_string(),
        "framework:jest_vitest.test" => "jest_vitest.test".to_string(),
        "framework:next.app.page"
        | "framework:next.app.layout"
        | "framework:next.route.handler"
        | "framework:next.pages.api_route"
        | "framework:next.pages.page"
        | "framework:fastify.route_handler"
        | "framework:prisma.query"
        | "framework:prisma.transaction"
        | "framework:drizzle.schema.table"
        | "framework:drizzle.query"
        | "framework:drizzle.transaction"
        | "framework:zod.schema"
        | "framework:nestjs.controller"
        | "framework:nestjs.route"
        | "framework:nestjs.injectable"
        | "framework:nestjs.module"
        | "framework:hono.route" => tsjs::support_family(target, framework_role),
        "framework:fastapi.route" => "fastapi.route_decorator".to_string(),
        "framework:pytest.test" => match target {
            "pytest.fixture" => "pytest.fixture_decorator".to_string(),
            _ => "pytest.test_anchor".to_string(),
        },
        "framework:pytest.fixture" => "pytest.fixture_decorator".to_string(),
        "framework:pydantic.model" => match target {
            "pydantic.BaseSettings" | "pydantic_settings.BaseSettings" => {
                "pydantic.settings_base".to_string()
            }
            _ => "pydantic.model_base".to_string(),
        },
        "framework:sqlalchemy.model" => "sqlalchemy.model_mapping".to_string(),
        "framework:sqlalchemy.repository_method" => match target {
            "sqlalchemy.orm.Session.commit"
            | "sqlalchemy.orm.Session.rollback"
            | "sqlalchemy.ext.asyncio.AsyncSession.commit"
            | "sqlalchemy.ext.asyncio.AsyncSession.rollback" => {
                "sqlalchemy.transaction_boundary".to_string()
            }
            _ => "sqlalchemy.query_call".to_string(),
        },
        "framework:django.model" => "django.model_base".to_string(),
        "framework:django.url_pattern" => "django.urls.path_call".to_string(),
        "framework:django.test" => "django.test_case".to_string(),
        "framework:flask.route" => "flask.route_decorator".to_string(),
        "framework:unittest.test" => "unittest.test_case_method".to_string(),
        "framework:click.command" => "click.command_decorator".to_string(),
        "framework:typer.command" => "typer.command_decorator".to_string(),
        "framework:celery.task" => "celery.task_decorator".to_string(),
        framework_role if java_framework_role_is_known(framework_role) => {
            java::support_family(target, framework_role)
        }
        framework_role if csharp_framework_role_is_known(framework_role) => {
            csharp::support_family(target, framework_role)
        }
        framework_role if cpp_framework_role_is_known(framework_role) => {
            cpp::support_family(target, framework_role)
        }
        framework_role if rust_role_is_known(framework_role) => {
            rust_support_family(target, framework_role)
        }
        _ => framework_role.to_string(),
    }
}

fn path_context(path: &str) -> String {
    let first_segment = path.split('/').next().unwrap_or("repo");
    match first_segment {
        "app" | "api" | "pages" | "src" | "tests" | "test" => stable_token(first_segment),
        _ => "repo".to_string(),
    }
}

fn cluster_normalized_shape(key: &FamilyKey, cluster_suffix: Option<&str>) -> String {
    match cluster_suffix {
        Some(suffix) => format!("{}:{}", key.normalized_shape, stable_token(suffix)),
        None => key.normalized_shape.clone(),
    }
}

fn family_affected_claim(key: &FamilyKey, cluster_suffix: Option<&str>) -> String {
    match cluster_suffix {
        Some(suffix) => format!(
            "family:{}:{}:{}:{}",
            key.language,
            key.code_unit_kind,
            key.framework_role,
            stable_token(suffix)
        ),
        None => format!(
            "family:{}:{}:{}",
            key.language, key.code_unit_kind, key.framework_role
        ),
    }
}

fn python_framework_anchor_target_varies(key: &FamilyKey, evidence: &[FamilyEvidence]) -> bool {
    if key.language != "python" {
        return false;
    }
    distinct_support_targets(evidence).len() > 1
}

fn distinct_support_targets(evidence: &[FamilyEvidence]) -> BTreeSet<String> {
    evidence
        .iter()
        .flat_map(|evidence| evidence.support_targets.iter().cloned())
        .collect()
}

fn framework_anchor_variation_evidence_indexes(claim: &FamilyClaim) -> BTreeSet<usize> {
    if claim.language != "python" || distinct_support_targets(&claim.evidence).len() <= 1 {
        return BTreeSet::new();
    }
    let canonical_targets = claim
        .evidence
        .first()
        .map(|evidence| {
            evidence
                .support_targets
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let index = claim
        .evidence
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, evidence)| {
            evidence
                .support_targets
                .iter()
                .any(|target| !canonical_targets.contains(target))
        })
        .map(|(index, _)| index)
        .or_else(|| (claim.evidence.len() > 1).then_some(1))
        .unwrap_or(0);
    [index].into_iter().collect()
}

fn family_evidence_covered_claims(
    index: usize,
    variation_evidence_indexes: &BTreeSet<usize>,
) -> Vec<String> {
    let mut claims = if index == 0 {
        vec!["canonical".to_string(), "support".to_string()]
    } else {
        vec!["support".to_string()]
    };
    if variation_evidence_indexes.contains(&index) {
        claims.push("variation".to_string());
    }
    claims
}

fn support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> bool {
    if python_framework_role_is_known(framework_role) {
        return python_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    if tsjs_framework_role_is_known(framework_role) {
        return tsjs_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    if java_framework_role_is_known(framework_role) {
        return java_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    if csharp_framework_role_is_known(framework_role) {
        return csharp_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    if cpp_framework_role_is_known(framework_role) {
        return cpp_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    if rust_role_is_known(framework_role) {
        return rust_support_fact_is_role_compatible(fact, framework_role).unwrap_or(false);
    }
    false
}

fn rust_support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = rust_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && rust_support_fact_has_safe_origin(fact, framework_role))
}

fn rust_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    derived_support_has_safe_origin(
        fact,
        RUST_DERIVED_SUPPORT_ENGINE,
        RUST_DERIVED_SUPPORT_METHOD,
        framework_role,
        &["derived_from=tree_sitter_rust_structural_anchors".to_string()],
    )
}

fn tsjs_support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = tsjs_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && tsjs_support_fact_has_safe_origin(fact, framework_role))
}

fn tsjs_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    let mut required = vec!["derived_from=tsjs_structural_anchors".to_string()];
    if let Some(derived_from) = tsjs::expected_derived_from(framework_role) {
        required.push(format!("derived_from={derived_from}"));
    }
    derived_support_has_safe_origin(
        fact,
        TSJS_DERIVED_SUPPORT_ENGINE,
        TSJS_DERIVED_SUPPORT_METHOD,
        framework_role,
        &required,
    ) || tsjs_provider_resolved_support_fact(fact, framework_role, &required)
}

fn tsjs_provider_resolved_support_fact(
    fact: &SemanticFact,
    framework_role: &str,
    required_assumptions: &[String],
) -> bool {
    fact.certainty == FactCertainty::DataflowDerived
        && fact.origin.engine == TSJS_DERIVED_SUPPORT_ENGINE
        && fact.origin.method == TSJS_DERIVED_SUPPORT_METHOD
        && fact_has_assumption(fact, "provider=typescript")
        && fact_has_assumption(fact, "provider_resolved=true")
        && fact_has_assumption(fact, &format!("framework_role={framework_role}"))
        && fact_assumption_value(fact, "query_operation=").is_some_and(|operation| {
            matches!(
                operation,
                "resolve_module_specifier"
                    | "resolve_export"
                    | "resolve_reexport"
                    | "resolve_package_entry"
            )
        })
        && required_assumptions
            .iter()
            .all(|assumption| fact_has_assumption(fact, assumption))
}

fn java_support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = java_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && java_support_fact_has_safe_origin(fact, framework_role))
}

fn java_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    let target = fact.target.as_ref().map(|target| target.as_str());
    let mut required = vec!["derived_from=tree_sitter_java_structural_anchors".to_string()];
    if let Some(target) = target {
        required.push(format!(
            "derived_from={}",
            java::support_family(target, framework_role)
        ));
    }
    derived_support_has_safe_origin(
        fact,
        JAVA_DERIVED_SUPPORT_ENGINE,
        JAVA_DERIVED_SUPPORT_METHOD,
        framework_role,
        &required,
    )
}

/// Exact target whitelist per TS/JS framework role. Mirrors the Python whitelist:
/// support must point at an exact recognized target, never at fact text that merely
/// contains a framework name.
pub(crate) fn tsjs_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    tsjs::support_target_is_role_compatible(target, framework_role)
}

pub(crate) fn tsjs_framework_role_is_known(framework_role: &str) -> bool {
    tsjs::framework_role_is_known(framework_role)
}

pub(crate) fn java_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    java::support_target_is_role_compatible(target, framework_role)
}

pub(crate) fn java_framework_role_is_known(framework_role: &str) -> bool {
    java::framework_role_is_known(framework_role)
}

fn csharp_support_fact_is_role_compatible(
    fact: &SemanticFact,
    framework_role: &str,
) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = csharp_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && csharp_support_fact_has_safe_origin(fact, framework_role))
}

fn csharp_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    let target = fact.target.as_ref().map(|target| target.as_str());
    let mut required = vec!["derived_from=tree_sitter_csharp_structural_anchors".to_string()];
    if let Some(target) = target {
        required.push(format!(
            "derived_from={}",
            csharp::support_family(target, framework_role)
        ));
    }
    derived_support_has_safe_origin(
        fact,
        CSHARP_DERIVED_SUPPORT_ENGINE,
        CSHARP_DERIVED_SUPPORT_METHOD,
        framework_role,
        &required,
    )
}

pub(crate) fn csharp_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    csharp::support_target_is_role_compatible(target, framework_role)
}

pub(crate) fn csharp_framework_role_is_known(framework_role: &str) -> bool {
    csharp::framework_role_is_known(framework_role)
}

fn cpp_support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = cpp_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && cpp_support_fact_has_safe_origin(fact, framework_role))
}

fn cpp_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    let target = fact.target.as_ref().map(|target| target.as_str());
    let mut required = vec!["derived_from=tree_sitter_c_cpp_structural_anchors".to_string()];
    if let Some(target) = target {
        required.push(format!(
            "derived_from={}",
            cpp::support_family(target, framework_role)
        ));
    }
    derived_support_has_safe_origin(
        fact,
        CPP_DERIVED_SUPPORT_ENGINE,
        CPP_DERIVED_SUPPORT_METHOD,
        framework_role,
        &required,
    )
}

pub(crate) fn cpp_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    cpp::support_target_is_role_compatible(target, framework_role)
}

pub(crate) fn cpp_framework_role_is_known(framework_role: &str) -> bool {
    cpp::framework_role_is_known(framework_role)
}

fn python_support_fact_is_role_compatible(
    fact: &SemanticFact,
    framework_role: &str,
) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = python_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && python_support_fact_has_safe_origin(fact, framework_role))
}

fn python_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    match fact.certainty {
        FactCertainty::DataflowDerived => derived_support_has_safe_origin(
            fact,
            PYTHON_DERIVED_SUPPORT_ENGINE,
            PYTHON_DERIVED_SUPPORT_METHOD,
            framework_role,
            &["derived_from=cpython_ast_structural_anchors".to_string()],
        ),
        FactCertainty::Semantic => {
            python_fixture_provider_support_fact(fact)
                || python_provider_resolved_support_fact(fact)
        }
        _ => false,
    }
}

fn python_fixture_provider_support_fact(fact: &SemanticFact) -> bool {
    fact.origin.engine == PYTHON_FIXTURE_PROVIDER_ENGINE
        && fact.origin.method == PYTHON_FIXTURE_PROVIDER_METHOD
}

fn python_provider_resolved_support_fact(fact: &SemanticFact) -> bool {
    let Some(provider) = fact_assumption_value(fact, "provider=") else {
        return false;
    };
    matches!(provider, "pyrefly" | "pyright")
        && fact.origin.engine == provider
        && fact_has_assumption(fact, "provider_resolved=true")
        && fact_assumption_value(fact, "query_operation=").is_some_and(|operation| {
            matches!(
                operation,
                "resolve_framework_identity" | "cross_check_claim"
            )
        })
}

fn fact_has_assumption(fact: &SemanticFact, expected: &str) -> bool {
    fact.assumptions
        .iter()
        .any(|assumption| assumption == expected)
}

fn fact_assumption_value<'a>(fact: &'a SemanticFact, prefix: &str) -> Option<&'a str> {
    fact.assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix(prefix))
}

pub(crate) fn python_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        "framework:fastapi.route" => Some(matches!(
            target,
            "fastapi.FastAPI.delete"
                | "fastapi.FastAPI.get"
                | "fastapi.FastAPI.head"
                | "fastapi.FastAPI.options"
                | "fastapi.FastAPI.patch"
                | "fastapi.FastAPI.post"
                | "fastapi.FastAPI.put"
                | "fastapi.APIRouter.delete"
                | "fastapi.APIRouter.get"
                | "fastapi.APIRouter.head"
                | "fastapi.APIRouter.options"
                | "fastapi.APIRouter.patch"
                | "fastapi.APIRouter.post"
                | "fastapi.APIRouter.put"
        )),
        "framework:pytest.test" => {
            Some(matches!(target, "pytest.test" | "pytest.mark.parametrize"))
        }
        "framework:pytest.fixture" => Some(matches!(target, "pytest.fixture")),
        "framework:pydantic.model" => Some(matches!(
            target,
            "pydantic.BaseModel" | "pydantic.BaseSettings" | "pydantic_settings.BaseSettings"
        )),
        "framework:sqlalchemy.model" => Some(matches!(
            target,
            "sqlalchemy.orm.DeclarativeBase"
                | "sqlalchemy.orm.Mapped"
                | "sqlalchemy.orm.mapped_column"
        )),
        "framework:sqlalchemy.repository_method" => Some(matches!(
            target,
            "sqlalchemy.select"
                | "sqlalchemy.orm.Session.execute"
                | "sqlalchemy.orm.Session.get"
                | "sqlalchemy.orm.Session.commit"
                | "sqlalchemy.orm.Session.rollback"
                | "sqlalchemy.orm.Session.scalar"
                | "sqlalchemy.orm.Session.scalars"
                | "sqlalchemy.ext.asyncio.AsyncSession.execute"
                | "sqlalchemy.ext.asyncio.AsyncSession.get"
                | "sqlalchemy.ext.asyncio.AsyncSession.commit"
                | "sqlalchemy.ext.asyncio.AsyncSession.rollback"
                | "sqlalchemy.ext.asyncio.AsyncSession.scalar"
                | "sqlalchemy.ext.asyncio.AsyncSession.scalars"
        )),
        "framework:django.model" => Some(matches!(target, "django.db.models.Model")),
        "framework:django.url_pattern" => {
            Some(matches!(target, "django.urls.path" | "django.urls.re_path"))
        }
        "framework:django.test" => Some(matches!(
            target,
            "django.test.TestCase"
                | "django.test.SimpleTestCase"
                | "django.test.TransactionTestCase"
        )),
        "framework:flask.route" => Some(matches!(target, "flask.route")),
        "framework:unittest.test" => Some(matches!(target, "unittest.TestCase.test")),
        "framework:click.command" => Some(matches!(target, "click.command")),
        "framework:typer.command" => Some(matches!(target, "typer.command")),
        "framework:celery.task" => Some(matches!(target, "celery.task" | "celery.shared_task")),
        _ if python_framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn python_framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:fastapi")
        || framework_role.starts_with("framework:pytest")
        || framework_role.starts_with("framework:pydantic")
        || framework_role.starts_with("framework:sqlalchemy")
        || framework_role.starts_with("framework:django")
        || framework_role.starts_with("framework:flask")
        || framework_role.starts_with("framework:unittest")
        || framework_role.starts_with("framework:click")
        || framework_role.starts_with("framework:typer")
        || framework_role.starts_with("framework:celery")
}

fn single_framework_role(roles: &BTreeSet<String>) -> Option<&str> {
    if roles.len() == 1 {
        roles.iter().next().map(String::as_str)
    } else {
        None
    }
}

pub(crate) fn family_eligible_kind(kind: &str) -> bool {
    matches!(
        kind,
        "express_route"
            | "next_app_page"
            | "next_app_layout"
            | "next_route_handler"
            | "next_pages_api_route"
            | "next_pages_page"
            | "fastify_route"
            | "prisma_query"
            | "prisma_transaction"
            | "drizzle_schema_table"
            | "drizzle_query"
            | "drizzle_transaction"
            | "zod_schema"
            | "nest_controller"
            | "nest_route"
            | "nest_injectable"
            | "nest_module"
            | "hono_route"
            | "react_component"
            | "react_hook"
            | "test_suite"
            | "test_case"
            | "fastapi_route"
            | "pytest_test"
            | "pytest_fixture"
            | "pydantic_model"
            | "sqlalchemy_model"
            | "sqlalchemy_repository_method"
            | "django_model"
            | "django_url_pattern"
            | "django_test"
            | "flask_route"
            | "unittest_test_method"
            | "click_command"
            | "typer_command"
            | "celery_task"
            | "spring_mvc_route"
            | "spring_component"
            | "spring_boot_application"
            | "spring_data_repository"
            | "junit5_test_method"
            | "junit4_test_method"
            | "testng_test_method"
            | "jpa_entity"
            | "jpa_mapped_superclass"
            | "jpa_embeddable"
            | "jaxrs_resource_class"
            | "jaxrs_resource_method"
            | "aspnet_controller"
            | "aspnet_controller_action"
            | "aspnet_minimal_api_route"
            | "efcore_db_context"
            | "efcore_entity_set"
            | "xunit_test_method"
            | "nunit_test_method"
            | "mstest_test_method"
            | "gtest_test_case"
            | "gtest_test_fixture"
            | "catch2_test_case"
            | "doctest_test_case"
            | "boost_test_case"
            | "boost_test_suite"
            | "serde_model"
            | "thiserror_error_enum"
            | "tokio_entry"
            | "tokio_test"
            | "clap_parser"
            | "axum_route"
    ) || rust_family_eligible_kind(kind)
}

fn min_family_support_for_key(key: &FamilyKey) -> usize {
    if key.language == "rust" && key.framework_role == "framework:repogrammar.rust_parser_adapter" {
        2
    } else {
        min_family_support(&key.language)
    }
}

pub(crate) fn min_family_support(language: &str) -> usize {
    if language == "python" {
        PYTHON_MIN_FAMILY_SUPPORT
    } else if is_tsjs_language_name(language) {
        TSJS_MIN_FAMILY_SUPPORT
    } else if language == "java" {
        JAVA_MIN_FAMILY_SUPPORT
    } else if language == "csharp" {
        CSHARP_MIN_FAMILY_SUPPORT
    } else if is_c_cpp_language(language) {
        CPP_MIN_FAMILY_SUPPORT
    } else if language == "rust" {
        3
    } else {
        DEFAULT_MIN_FAMILY_SUPPORT
    }
}

fn is_tsjs_language_name(language: &str) -> bool {
    matches!(
        language,
        "typescript" | "typescript-react" | "tsx" | "javascript" | "javascript-react" | "jsx"
    )
}

fn normalized_shape(kind: &str, framework_role: &str) -> String {
    format!("shape:{kind}:{}", stable_token(framework_role))
}

fn family_id(key: &FamilyKey, cluster_suffix: Option<&str>) -> String {
    let base = format!(
        "family:{}:{}:{}",
        stable_token(&key.language),
        stable_token(&key.code_unit_kind),
        stable_token(&key.framework_role)
    );
    match cluster_suffix {
        Some(suffix) => format!("{base}:{}", stable_token(suffix)),
        None => base,
    }
}

fn stable_token(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn insufficient_support_unknown(affected_claim: String) -> ClaimUnknown {
    ClaimUnknown {
        class: ClaimImpact::Blocking.as_legacy_unknown_class(),
        reason: UnknownReasonCode::InsufficientSupport,
        affected_claim,
        recovery: Some(
            "add another compatible implementation before claiming a family".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        CodeUnitId, ContentHash, Evidence, FactOrigin, Provenance, RepositoryRevision, SourceRange,
        SymbolId,
    };

    fn hash() -> ContentHash {
        ContentHash::new("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
            .expect("valid hash")
    }

    #[test]
    fn claim_unknown_impact_rejects_resolution_only_classes() {
        for class in [UnknownClass::Recoverable, UnknownClass::Irreducible] {
            let unknown = ClaimUnknown {
                class,
                reason: UnknownReasonCode::InsufficientSupport,
                affected_claim: "family:test".to_string(),
                recovery: None,
            };

            assert_eq!(unknown.claim_impact(), None);
        }
    }

    fn other_hash() -> ContentHash {
        ContentHash::new("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
            .expect("valid hash")
    }

    fn unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(path, "typescript", kind, index)
    }

    fn python_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(path, "python", kind, index)
    }

    fn java_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(path, "java", kind, index)
    }

    fn unit_with_language(
        path: &str,
        language: &str,
        kind: &str,
        index: usize,
    ) -> IndexedCodeUnitRecord {
        IndexedCodeUnitRecord {
            id: format!("unit:{path}#{kind}:{index}:0-10:{index}"),
            path: path.to_string(),
            language: language.to_string(),
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 10,
            content_hash: hash(),
        }
    }

    fn role_fact(unit: &IndexedCodeUnitRecord, role: &str) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::FrameworkRole,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(role).expect("valid role")),
            origin: FactOrigin {
                engine: "repogrammar-frameworks".to_string(),
                engine_version: "0.1.0".to_string(),
                method: "syntax_code_unit_kind".to_string(),
            },
            certainty: FactCertainty::FrameworkHeuristic,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "syntax code unit indicates framework role",
            )
            .expect("valid evidence"),
            assumptions: vec!["binding unresolved".to_string()],
        }
    }

    fn semantic_support_fact_with_target(
        unit: &IndexedCodeUnitRecord,
        target: &str,
    ) -> SemanticFact {
        let mut fact = SemanticFact {
            kind: SemanticFactKind::ResolvedImport,
            subject: format!("{}#import", unit.id),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: "typescript".to_string(),
                engine_version: "6.0.0".to_string(),
                method: "compiler_api".to_string(),
            },
            certainty: FactCertainty::Semantic,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "semantic support evidence",
            )
            .expect("valid evidence"),
            assumptions: Vec::new(),
        };
        if unit.language == "python" {
            fact.origin.engine = PYTHON_DERIVED_SUPPORT_ENGINE.to_string();
            fact.origin.engine_version = "0.1.0".to_string();
            fact.origin.method = PYTHON_DERIVED_SUPPORT_METHOD.to_string();
            fact.certainty = FactCertainty::DataflowDerived;
            fact.assumptions = vec![
                "provider_resolved=false".to_string(),
                "derived_from=cpython_ast_structural_anchors".to_string(),
            ];
            if let Some(framework_role) = python_framework_role_for_kind(&unit.kind) {
                fact.assumptions
                    .push(format!("framework_role={framework_role}"));
            }
        }
        fact
    }

    fn python_framework_role_for_kind(kind: &str) -> Option<&'static str> {
        match kind {
            "fastapi_route" => Some("framework:fastapi.route"),
            "pytest_test" => Some("framework:pytest.test"),
            "pytest_fixture" => Some("framework:pytest.fixture"),
            "pydantic_model" => Some("framework:pydantic.model"),
            "sqlalchemy_model" => Some("framework:sqlalchemy.model"),
            "sqlalchemy_repository_method" => Some("framework:sqlalchemy.repository_method"),
            _ => None,
        }
    }

    fn semantic_support_fact_with_origin(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        engine: &str,
        method: &str,
    ) -> SemanticFact {
        let mut fact = semantic_support_fact_with_target(unit, target);
        fact.origin.engine = engine.to_string();
        fact.origin.method = method.to_string();
        fact
    }

    fn semantic_support_fact_with_range(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> SemanticFact {
        let mut fact = semantic_support_fact_with_target(unit, target);
        fact.evidence = Evidence::new(
            CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
            SourceRange::new(start_byte, end_byte).expect("valid range"),
            Provenance::new(
                &unit.path,
                unit.content_hash.clone(),
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
            "semantic support evidence",
        )
        .expect("valid evidence");
        fact
    }

    fn semantic_support_fact_with_hash(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        content_hash: ContentHash,
    ) -> SemanticFact {
        let mut fact = semantic_support_fact_with_target(unit, target);
        fact.evidence = Evidence::new(
            CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
            SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
            Provenance::new(
                &unit.path,
                content_hash,
                RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            )
            .expect("valid provenance"),
            "semantic support evidence",
        )
        .expect("valid evidence");
        fact
    }

    fn semantic_project_config_fact(unit: &IndexedCodeUnitRecord) -> SemanticFact {
        let mut fact = semantic_support_fact_with_target(unit, "fastapi.APIRouter.get");
        fact.kind = SemanticFactKind::ProjectConfig;
        fact
    }

    fn python_context_fact(
        unit: &IndexedCodeUnitRecord,
        anchor_kind: &str,
        target: Option<&str>,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Symbol,
            subject: format!("{}#{anchor_kind}", unit.id),
            target: target.map(|target| SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "3.12.0".to_string(),
                method: "cpython_ast".to_string(),
            },
            certainty: FactCertainty::Structural,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "python parser context fact",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("python_anchor_kind={anchor_kind}")],
        }
    }

    fn parser_origin_strong_python_anchor_fact(
        unit: &IndexedCodeUnitRecord,
        kind: SemanticFactKind,
        anchor_kind: &str,
        target: &str,
        extra_assumption: &str,
    ) -> SemanticFact {
        let mut fact = python_context_fact(unit, anchor_kind, Some(target));
        fact.kind = kind;
        fact.certainty = FactCertainty::DataflowDerived;
        fact.assumptions.push(extra_assumption.to_string());
        fact
    }

    fn assert_insufficient_support(report: &FamilyBuildReport) {
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    fn assert_unknown_reason(report: &FamilyBuildReport, reason: UnknownReasonCode) {
        assert!(
            report
                .unknowns
                .iter()
                .any(|unknown| unknown.reason == reason),
            "expected {reason:?} in {report:?}"
        );
    }

    fn assert_python_three_member_family(kind: &str, role: &str, targets: [&str; 3]) {
        let first = python_unit(&format!("app/{kind}_a.py"), kind, 0);
        let second = python_unit(&format!("app/{kind}_b.py"), kind, 1);
        let third = python_unit(&format!("app/{kind}_c.py"), kind, 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                semantic_support_fact_with_target(&first, targets[0]),
                semantic_support_fact_with_target(&second, targets[1]),
                semantic_support_fact_with_target(&third, targets[2]),
            ],
        );

        assert_eq!(report.claims.len(), 1, "{kind} should form one family");
        let claim = &report.claims[0];
        assert_eq!(claim.language, "python");
        assert_eq!(claim.code_unit_kind, kind);
        assert_eq!(claim.framework_role, role);
        assert_eq!(claim.support, 3);
        assert_eq!(claim.readiness, ClaimReadiness::Ready);
    }

    fn python_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#unknown", unit.id),
            target: Some(
                SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason target"),
            ),
            origin: FactOrigin {
                engine: "python".to_string(),
                engine_version: "3.12.0".to_string(),
                method: "cpython_ast".to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "python typed UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn framework_heuristic_role_support_stays_unknown_without_semantic_support() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn tsjs_type_worker_package_facts_do_not_prove_framework_family() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                semantic_support_fact_with_target(&first, "package:express"),
                semantic_support_fact_with_target(&second, "package:express"),
                semantic_support_fact_with_target(&third, "package:express"),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn unrelated_semantic_support_does_not_prove_framework_family() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                semantic_support_fact_with_target(&first, "package:lodash"),
                semantic_support_fact_with_target(&second, "package:lodash"),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    fn tsjs_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
    ) -> SemanticFact {
        tsjs_derived_fact_with_assumptions(unit, target, framework_role, Vec::new())
    }

    fn tsjs_derived_fact_with_assumptions(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
        extra_assumptions: Vec<&str>,
    ) -> SemanticFact {
        let mut assumptions = vec![
            "provider_resolved=false".to_string(),
            "derived_from=tsjs_structural_anchors".to_string(),
            format!("framework_role={framework_role}"),
            format!("tsjs_anchor_kind={}", unit.kind),
        ];
        if let Some(derived_from) = tsjs::expected_derived_from(framework_role) {
            assumptions.push(format!("derived_from={derived_from}"));
        }
        assumptions.extend(
            extra_assumptions
                .into_iter()
                .map(std::string::ToString::to_string),
        );
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: TSJS_DERIVED_SUPPORT_ENGINE.to_string(),
                engine_version: "0.1.0".to_string(),
                method: TSJS_DERIVED_SUPPORT_METHOD.to_string(),
            },
            certainty: FactCertainty::DataflowDerived,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded TS/JS framework anchor support",
            )
            .expect("valid evidence"),
            assumptions,
        }
    }

    fn java_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
    ) -> SemanticFact {
        java_derived_fact_with_assumptions(unit, target, framework_role, Vec::new())
    }

    fn java_route_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        http_method: &str,
        route_path_shape: &str,
    ) -> SemanticFact {
        let mut fact = java_derived_fact(unit, target, "framework:spring.mvc_route");
        let annotation = target.rsplit('.').next().unwrap_or("RequestMapping");
        fact.assumptions.extend([
            "java_anchor_kind=spring_mvc_route".to_string(),
            format!("spring_annotation={annotation}"),
            format!("http_method={http_method}"),
            format!("route_path_shape={route_path_shape}"),
        ]);
        fact.assumptions.sort();
        fact.assumptions.dedup();
        fact
    }

    fn java_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#java_unknown", unit.id),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason")),
            origin: FactOrigin {
                engine: JAVA_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: JAVA_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed Java/Spring UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    fn java_derived_fact_with_assumptions(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
        extra_assumptions: Vec<&str>,
    ) -> SemanticFact {
        let mut assumptions = vec![
            "provider_resolved=false".to_string(),
            "derived_from=tree_sitter_java_structural_anchors".to_string(),
            format!(
                "derived_from={}",
                java::support_family(target, framework_role)
            ),
            format!("framework_role={framework_role}"),
            format!("java_anchor_kind={}", unit.kind),
        ];
        assumptions.extend(
            extra_assumptions
                .into_iter()
                .map(std::string::ToString::to_string),
        );
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: JAVA_DERIVED_SUPPORT_ENGINE.to_string(),
                engine_version: "0.1.0".to_string(),
                method: JAVA_DERIVED_SUPPORT_METHOD.to_string(),
            },
            certainty: FactCertainty::DataflowDerived,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded Java Spring structural role support",
            )
            .expect("valid evidence"),
            assumptions,
        }
    }

    fn java_parser_structural_anchor(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
    ) -> SemanticFact {
        let mut fact = java_derived_fact(unit, target, framework_role);
        fact.origin.engine = JAVA_ANCHOR_ENGINE.to_string();
        fact.origin.method = JAVA_ANCHOR_METHOD.to_string();
        fact.certainty = FactCertainty::Structural;
        fact
    }

    #[test]
    fn java_family_requires_three_compatible_exact_anchor_support_facts() {
        let first = java_unit("src/main/java/AController.java", "spring_mvc_route", 0);
        let second = java_unit("src/main/java/BController.java", "spring_mvc_route", 1);
        let third = java_unit("src/main/java/CController.java", "spring_mvc_route", 2);
        let role = "framework:spring.mvc_route";

        let low_support = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                java_route_derived_fact(
                    &first,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &second,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_insufficient_support(&low_support);

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                java_route_derived_fact(
                    &first,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &second,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &third,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "java");
        assert_eq!(report.claims[0].support, 3);
        assert_eq!(report.claims[0].framework_role, role);
    }

    #[test]
    fn java_spring_mvc_route_clustering_separates_request_mapping_methods() {
        let units = (0..6)
            .map(|index| {
                java_unit(
                    &format!("src/main/java/Route{index}Controller.java"),
                    "spring_mvc_route",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, "framework:spring.mvc_route"))
            .collect::<Vec<_>>();
        for unit in units.iter().take(3) {
            facts.push(java_route_derived_fact(
                unit,
                "spring.web.bind.annotation.RequestMapping",
                "GET",
                "literal",
            ));
        }
        for unit in units.iter().skip(3) {
            facts.push(java_route_derived_fact(
                unit,
                "spring.web.bind.annotation.RequestMapping",
                "POST",
                "literal",
            ));
        }

        let report = build_family_claims(&units, &facts);

        assert_eq!(report.claims.len(), 2);
        let mut supports = report
            .claims
            .iter()
            .map(|claim| (claim.framework_role.as_str(), claim.support))
            .collect::<Vec<_>>();
        supports.sort();
        assert_eq!(
            supports,
            [
                ("framework:spring.mvc_route", 3),
                ("framework:spring.mvc_route", 3)
            ]
        );
        let family_ids = report
            .claims
            .iter()
            .map(|claim| claim.family_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(family_ids.len(), 2);
    }

    #[test]
    fn java_jpa_entities_split_families_by_namespace_root() {
        let units = (0..6)
            .map(|index| {
                java_unit(
                    &format!("src/main/java/Entity{index}.java"),
                    "jpa_entity",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, "framework:jpa.entity"))
            .collect::<Vec<_>>();
        for unit in units.iter().take(3) {
            facts.push(java_derived_fact_with_assumptions(
                unit,
                "jpa.persistence.Entity",
                "framework:jpa.entity",
                vec!["jpa_namespace_root=jakarta", "java_class_shape=class"],
            ));
        }
        for unit in units.iter().skip(3) {
            facts.push(java_derived_fact_with_assumptions(
                unit,
                "jpa.persistence.Entity",
                "framework:jpa.entity",
                vec!["jpa_namespace_root=javax", "java_class_shape=class"],
            ));
        }

        let report = build_family_claims(&units, &facts);

        // Identical targets, but jakarta and javax must never conflate.
        assert_eq!(
            report.claims.len(),
            2,
            "jakarta and javax entities must form separate families"
        );
        for claim in &report.claims {
            assert_eq!(claim.framework_role, "framework:jpa.entity");
            assert_eq!(claim.support, 3);
        }
        let family_ids = report
            .claims
            .iter()
            .map(|claim| claim.family_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(family_ids.len(), 2);
    }

    #[test]
    fn java_junit5_test_methods_form_family_when_annotation_and_kind_agree() {
        let units = (0..3)
            .map(|index| {
                java_unit(
                    &format!("src/test/java/Case{index}Test.java"),
                    "junit5_test_method",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, "framework:junit5.test"))
            .collect::<Vec<_>>();
        for unit in &units {
            facts.push(java_derived_fact_with_assumptions(
                unit,
                "junit.jupiter.Test",
                "framework:junit5.test",
                vec!["test_annotation=Test", "test_data_shape=none"],
            ));
        }

        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].framework_role, "framework:junit5.test");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn java_runtime_unknown_subclaims_do_not_block_exact_support() {
        let components = (0..3)
            .map(|index| {
                java_unit(
                    &format!("src/main/java/Service{index}.java"),
                    "spring_component",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut component_facts = components
            .iter()
            .map(|unit| role_fact(unit, "framework:spring.component"))
            .collect::<Vec<_>>();
        for unit in &components {
            component_facts.push(java_derived_fact_with_assumptions(
                unit,
                "spring.stereotype.Service",
                "framework:spring.component",
                vec!["spring_annotation=Service", "java_class_shape=class"],
            ));
        }
        component_facts.extend([
            java_unknown_fact(
                &components[0],
                UnknownReasonCode::RuntimeDependencyInjection,
                "java_spring_component_scan",
            ),
            java_unknown_fact(
                &components[0],
                UnknownReasonCode::RuntimeDependencyInjection,
                "java_spring_dependency_injection",
            ),
            java_unknown_fact(
                &components[0],
                UnknownReasonCode::FrameworkMagic,
                "java_spring_proxy_semantics",
            ),
        ]);

        let component_report = build_family_claims(&components, &component_facts);

        assert_eq!(component_report.claims.len(), 1);
        let component_unknowns = &component_report.claims[0].unknowns;
        for claim in [
            "java_spring_component_scan",
            "java_spring_dependency_injection",
            "java_spring_proxy_semantics",
        ] {
            assert!(component_unknowns.iter().any(|unknown| {
                unknown.claim_impact() == Some(ClaimImpact::NonBlocking)
                    && unknown.affected_claim.ends_with(claim)
            }));
        }

        let repositories = (0..3)
            .map(|index| {
                java_unit(
                    &format!("src/main/java/Repository{index}.java"),
                    "spring_data_repository",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut repository_facts = repositories
            .iter()
            .map(|unit| role_fact(unit, "framework:spring_data.repository"))
            .collect::<Vec<_>>();
        for unit in &repositories {
            repository_facts.push(java_derived_fact(
                unit,
                "spring.data.jpa.repository.JpaRepository",
                "framework:spring_data.repository",
            ));
        }
        repository_facts.push(java_unknown_fact(
            &repositories[0],
            UnknownReasonCode::FrameworkMagic,
            "java_spring_generated_repository",
        ));

        let repository_report = build_family_claims(&repositories, &repository_facts);

        assert_eq!(repository_report.claims.len(), 1);
        assert!(repository_report.claims[0].unknowns.iter().any(|unknown| {
            unknown.claim_impact() == Some(ClaimImpact::NonBlocking)
                && unknown
                    .affected_claim
                    .ends_with("java_spring_generated_repository")
        }));
    }

    #[test]
    fn java_structural_framework_anchors_cannot_directly_support_membership() {
        let first = java_unit("src/main/java/AController.java", "spring_mvc_route", 0);
        let second = java_unit("src/main/java/BController.java", "spring_mvc_route", 1);
        let third = java_unit("src/main/java/CController.java", "spring_mvc_route", 2);
        let role = "framework:spring.mvc_route";
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                java_parser_structural_anchor(
                    &first,
                    "spring.web.bind.annotation.GetMapping",
                    role,
                ),
                java_parser_structural_anchor(
                    &second,
                    "spring.web.bind.annotation.PostMapping",
                    role,
                ),
                java_parser_structural_anchor(
                    &third,
                    "spring.web.bind.annotation.DeleteMapping",
                    role,
                ),
            ],
        );
        assert_insufficient_support(&report);
    }

    #[test]
    fn java_support_requires_safe_origin_and_exact_target() {
        let first = java_unit("src/main/java/AController.java", "spring_mvc_route", 0);
        let second = java_unit("src/main/java/BController.java", "spring_mvc_route", 1);
        let third = java_unit("src/main/java/CController.java", "spring_mvc_route", 2);
        let role = "framework:spring.mvc_route";

        let mut wrong_engine = java_route_derived_fact(
            &first,
            "spring.web.bind.annotation.GetMapping",
            "GET",
            "literal",
        );
        wrong_engine.origin.engine = "java-lookalike".to_string();
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                wrong_engine,
                java_route_derived_fact(
                    &second,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &third,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_insufficient_support(&report);

        let substring = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                java_route_derived_fact(
                    &first,
                    "com.example.spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &second,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
                java_route_derived_fact(
                    &third,
                    "spring.web.bind.annotation.GetMapping",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_insufficient_support(&substring);
    }

    #[test]
    fn tsjs_derived_exact_anchor_support_requires_three_members_but_role_alone_does_not() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let role_only = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
            ],
        );
        assert!(role_only.claims.is_empty());
        assert!(role_only
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));

        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                tsjs_derived_fact(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
            ],
        );
        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                tsjs_derived_fact(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                ),
            ],
        );
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].classification, "DOMINANT_PATTERN");
        assert_eq!(report.claims[0].support, 3);
        assert_eq!(
            report.claims[0].framework_role,
            "framework:express.route_handler"
        );
    }

    #[test]
    fn tsjs_support_requires_safe_origin_and_exact_target() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);

        // Wrong engine: a fact whose text merely contains "express" is not derived support.
        let mut wrong_engine_first = tsjs_derived_fact(
            &first,
            "express.route.get",
            "framework:express.route_handler",
        );
        wrong_engine_first.origin.engine = "repogrammar-frameworks".to_string();
        let mut wrong_engine_second = tsjs_derived_fact(
            &second,
            "express.route.post",
            "framework:express.route_handler",
        );
        wrong_engine_second.origin.engine = "repogrammar-frameworks".to_string();
        let wrong_engine = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                wrong_engine_first,
                wrong_engine_second,
            ],
        );
        assert!(wrong_engine.claims.is_empty());

        // Wrong target: a non-whitelisted target is not compatible even with a safe origin.
        let unrelated = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                tsjs_derived_fact(
                    &first,
                    "express.lookalike.get",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &second,
                    "express.lookalike.post",
                    "framework:express.route_handler",
                ),
            ],
        );
        assert!(unrelated.claims.is_empty());
    }

    #[test]
    fn tsjs_provider_resolved_derived_support_requires_provider_provenance() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let mut provider_first = tsjs_derived_fact(
            &first,
            "express.route.get",
            "framework:express.route_handler",
        );
        provider_first
            .assumptions
            .retain(|value| value != "provider_resolved=false");
        provider_first.assumptions.extend([
            "provider=typescript".to_string(),
            "provider_resolved=true".to_string(),
            "query_operation=resolve_module_specifier".to_string(),
            "tsconfig_hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            "package_json_hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            "environment_fingerprint=node_static_worker_v1".to_string(),
        ]);
        let mut missing_operation = provider_first.clone();
        missing_operation
            .assumptions
            .retain(|value| !value.starts_with("query_operation="));

        let blocked = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                missing_operation,
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                ),
            ],
        );
        assert!(blocked.claims.is_empty());

        let accepted = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                provider_first,
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                ),
            ],
        );
        assert_eq!(accepted.claims.len(), 1);
    }

    fn tsjs_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#tsjs_unknown", unit.id),
            target: Some(
                SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason target"),
            ),
            origin: FactOrigin {
                engine: "repogrammar-tsjs-syntax".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: "exact_anchor_v1".to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed TS/JS UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    fn rust_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
    ) -> SemanticFact {
        let mut fact = semantic_support_fact_with_origin(
            unit,
            target,
            RUST_DERIVED_SUPPORT_ENGINE,
            RUST_DERIVED_SUPPORT_METHOD,
        );
        fact.certainty = FactCertainty::DataflowDerived;
        fact.assumptions = vec![
            "provider_resolved=false".to_string(),
            "derived_from=tree_sitter_rust_structural_anchors".to_string(),
            format!("framework_role={framework_role}"),
        ];
        fact
    }

    fn rust_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#rust_unknown", unit.id),
            target: Some(
                SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason target"),
            ),
            origin: FactOrigin {
                engine: RUST_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: RUST_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed Rust UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn rust_build_variant_repository_blocker_is_rust_manifest_only() {
        let tsjs_manifest = unit("Cargo.toml", "project_config", 0);
        let rust_source = unit_with_language("src/rust/lib.rs", "rust", "rust_module", 0);
        let rust_manifest = unit_with_language("Cargo.toml", "rust_config", "cargo_manifest", 1);

        let tsjs_unknown = tsjs_unknown_fact(
            &tsjs_manifest,
            UnknownReasonCode::BuildVariantAmbiguity,
            "rust_build_variant",
        );
        assert!(rust_repository_blocking_unknowns(&[tsjs_unknown]).is_empty());

        let source_unknown = rust_unknown_fact(
            &rust_source,
            UnknownReasonCode::BuildVariantAmbiguity,
            "rust_build_variant",
        );
        assert!(rust_repository_blocking_unknowns(&[source_unknown]).is_empty());

        let nested_manifest = unit_with_language(
            "src/fixtures/rust/release/v0_2/module_resolution/Cargo.toml",
            "rust-config",
            "cargo_manifest",
            2,
        );
        let nested_manifest_unknown = rust_unknown_fact(
            &nested_manifest,
            UnknownReasonCode::BuildVariantAmbiguity,
            "rust_build_variant",
        );
        assert!(rust_repository_blocking_unknowns(&[nested_manifest_unknown]).is_empty());

        let unrelated_claim_unknown = rust_unknown_fact(
            &rust_manifest,
            UnknownReasonCode::BuildVariantAmbiguity,
            "tsjs_receiver_binding",
        );
        assert!(rust_repository_blocking_unknowns(&[unrelated_claim_unknown]).is_empty());

        let manifest_unknown = rust_unknown_fact(
            &rust_manifest,
            UnknownReasonCode::BuildVariantAmbiguity,
            "rust_build_variant",
        );
        let blockers = rust_repository_blocking_unknowns(&[manifest_unknown]);
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].reason, UnknownReasonCode::BuildVariantAmbiguity);
        assert_eq!(blockers[0].affected_claim, "rust_build_variant");
    }

    #[test]
    fn rust_cfg_feature_unknown_recovery_uses_cargo_context() {
        let unit = unit_with_language("src/rust/application/family.rs", "rust", "rust_function", 0);
        let mut fact = rust_unknown_fact(
            &unit,
            UnknownReasonCode::BuildVariantAmbiguity,
            "rust_build_variant",
        );
        fact.assumptions.extend([
            "rust_cfg_feature=preview".to_string(),
            "rust_cfg_feature_declared=preview:true".to_string(),
            "rust_cfg_model=cargo_feature_cfg_model".to_string(),
            "rust_cfg_predicate=feature".to_string(),
        ]);

        let unknown =
            classify_family_unknown_fact("rust", &fact, "framework:repogrammar.rust_family_gate")
                .expect("cfg feature UNKNOWN should block Rust support");

        assert_eq!(unknown.claim_impact(), Some(ClaimImpact::Blocking));
        assert_eq!(unknown.reason, UnknownReasonCode::BuildVariantAmbiguity);
        assert_eq!(
            unknown.recovery.as_deref(),
            Some("resolve Rust cfg feature gate before claiming a family (preview:declared)")
        );
    }

    #[test]
    fn fixture_cargo_build_variant_unknown_does_not_block_root_rust_families() {
        let first = unit_with_language(
            "src/rust/adapters/parsing/rust/mod.rs",
            "rust",
            "rust_function",
            0,
        );
        let second = unit_with_language(
            "src/rust/adapters/parsing/rust/unknown.rs",
            "rust",
            "rust_function",
            1,
        );
        let third = unit_with_language(
            "src/rust/adapters/parsing/rust/tree_sitter.rs",
            "rust",
            "rust_function",
            2,
        );
        let nested_manifest = unit_with_language(
            "src/fixtures/rust/release/v0_2/cargo_build_blocked_family/Cargo.toml",
            "rust-config",
            "cargo_manifest",
            3,
        );
        let role = "framework:repogrammar.rust_parser_adapter";
        let target = "repogrammar.rust.parser_adapter";
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, role),
                role_fact(&second, role),
                role_fact(&third, role),
                rust_derived_fact(&first, target, role),
                rust_derived_fact(&second, target, role),
                rust_derived_fact(&third, target, role),
                rust_unknown_fact(
                    &nested_manifest,
                    UnknownReasonCode::BuildVariantAmbiguity,
                    "rust_build_variant",
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "rust");
        assert_eq!(report.claims[0].framework_role, role);
        assert_eq!(report.claims[0].support, 3);
        assert!(!report.unknowns.iter().any(|unknown| {
            unknown.reason == UnknownReasonCode::BuildVariantAmbiguity
                && unknown.affected_claim == "rust_build_variant"
        }));
    }

    fn rust_serde_unit(index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(
            &format!("src/model{index}.rs"),
            "rust",
            "serde_model",
            index,
        )
    }

    fn rust_serde_derived_fact(unit: &IndexedCodeUnitRecord, target: &str) -> SemanticFact {
        let mut fact = rust_derived_fact(unit, target, "framework:serde.model");
        fact.assumptions.extend([
            "rust_anchor_kind=serde_model".to_string(),
            "serde_attr_shape=none".to_string(),
        ]);
        fact
    }

    #[test]
    fn rust_serde_models_with_matching_trait_profiles_form_general_family() {
        let units = (0..3).map(rust_serde_unit).collect::<Vec<_>>();
        let mut facts = Vec::new();
        for unit in &units {
            facts.push(role_fact(unit, "framework:serde.model"));
            facts.push(rust_serde_derived_fact(unit, "serde.Serialize"));
            facts.push(rust_serde_derived_fact(unit, "serde.Deserialize"));
        }
        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 1, "{report:?}");
        assert!(report.claims[0]
            .family_id
            .starts_with("family:rust:serde_model:framework_serde_model"));
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn rust_serde_serialize_only_does_not_merge_with_both_trait_models() {
        let units = (0..3).map(rust_serde_unit).collect::<Vec<_>>();
        let mut facts = Vec::new();
        // Two both-trait models plus one Serialize-only model: the trait/target
        // profiles differ, so complete-link clustering must not merge them and
        // no cluster reaches the support-3 gate.
        for unit in units.iter().take(2) {
            facts.push(role_fact(unit, "framework:serde.model"));
            facts.push(rust_serde_derived_fact(unit, "serde.Serialize"));
            facts.push(rust_serde_derived_fact(unit, "serde.Deserialize"));
        }
        facts.push(role_fact(&units[2], "framework:serde.model"));
        facts.push(rust_serde_derived_fact(&units[2], "serde.Serialize"));
        let report = build_family_claims(&units, &facts);
        assert!(
            report.claims.is_empty(),
            "Serialize-only and both-trait serde models must not merge: {report:?}"
        );
    }

    #[test]
    fn rust_general_family_wiring_leaves_self_dogfood_family_intact() {
        // A general serde family and a self-dogfood parser-adapter family coexist
        // in one build without interfering.
        let serde_units = (0..3).map(rust_serde_unit).collect::<Vec<_>>();
        let dogfood_units = (0..3)
            .map(|index| {
                unit_with_language(
                    &format!("src/rust/adapters/parsing/rust/mod{index}.rs"),
                    "rust",
                    "rust_function",
                    index + 10,
                )
            })
            .collect::<Vec<_>>();
        let mut facts = Vec::new();
        for unit in &serde_units {
            facts.push(role_fact(unit, "framework:serde.model"));
            facts.push(rust_serde_derived_fact(unit, "serde.Serialize"));
            facts.push(rust_serde_derived_fact(unit, "serde.Deserialize"));
        }
        for unit in &dogfood_units {
            facts.push(role_fact(unit, "framework:repogrammar.rust_parser_adapter"));
            facts.push(rust_derived_fact(
                unit,
                "repogrammar.rust.parser_adapter",
                "framework:repogrammar.rust_parser_adapter",
            ));
        }
        let mut all_units = serde_units;
        all_units.extend(dogfood_units);
        let report = build_family_claims(&all_units, &facts);
        assert!(report
            .claims
            .iter()
            .any(|claim| claim.framework_role == "framework:serde.model" && claim.support == 3));
        assert!(report.claims.iter().any(|claim| {
            claim.framework_role == "framework:repogrammar.rust_parser_adapter"
                && claim.support == 3
        }));
    }

    #[test]
    fn tsjs_complete_link_clustering_rejects_single_link_bridge() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                tsjs_derived_fact_with_assumptions(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                    vec!["handler_shape=inline_json"],
                ),
                tsjs_derived_fact_with_assumptions(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                    vec!["handler_shape=referenced_handler"],
                ),
                tsjs_derived_fact_with_assumptions(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                    vec!["handler_shape=inline_json"],
                ),
            ],
        );

        assert!(
            report.claims.is_empty(),
            "complete-link clustering must not let a referenced-handler bridge merge incompatible TS/JS route styles"
        );
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn tsjs_complete_link_clustering_emits_distinct_ready_route_clusters() {
        let units = (0..6)
            .map(|index| unit(&format!("src/route{index}.ts"), "express_route", index))
            .collect::<Vec<_>>();
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, "framework:express.route_handler"))
            .collect::<Vec<_>>();
        for unit in units.iter().take(3) {
            facts.push(tsjs_derived_fact_with_assumptions(
                unit,
                "express.route.get",
                "framework:express.route_handler",
                vec!["handler_shape=inline_json"],
            ));
        }
        for unit in units.iter().skip(3) {
            facts.push(tsjs_derived_fact_with_assumptions(
                unit,
                "express.route.post",
                "framework:express.route_handler",
                vec!["handler_shape=referenced_handler"],
            ));
        }

        let report = build_family_claims(&units, &facts);

        assert_eq!(report.claims.len(), 2);
        let mut supports = report
            .claims
            .iter()
            .map(|claim| (claim.framework_role.as_str(), claim.support))
            .collect::<Vec<_>>();
        supports.sort();
        assert_eq!(
            supports,
            [
                ("framework:express.route_handler", 3),
                ("framework:express.route_handler", 3)
            ]
        );
    }

    #[test]
    fn tsjs_blocking_unknown_prevents_exact_anchor_family_support() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                tsjs_derived_fact(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                ),
                tsjs_unknown_fact(
                    &third,
                    UnknownReasonCode::DynamicImport,
                    "tsjs_receiver_binding",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report.unknowns.iter().any(|unknown| {
            unknown.claim_impact() == Some(ClaimImpact::Blocking)
                && unknown.reason == UnknownReasonCode::DynamicImport
        }));
    }

    #[test]
    fn blocking_unknown_detector_requires_the_language_anchor_engine() {
        let ts_unit = unit("src/a.ts", "express_route", 0);
        // An anchor-engine UNKNOWN with a blocking reason blocks the family.
        let anchor_unknown = tsjs_unknown_fact(
            &ts_unit,
            UnknownReasonCode::DynamicImport,
            "tsjs_receiver_binding",
        );
        assert!(
            classify_family_unknown_fact(
                "typescript",
                &anchor_unknown,
                "framework:express.route_handler"
            )
            .filter(|unknown| unknown.claim_impact() == Some(ClaimImpact::Blocking))
            .is_some(),
            "the TS/JS anchor engine must still be able to block"
        );
        // The same UNKNOWN from a non-authoritative engine must not block, so a
        // stored/provider UNKNOWN that merely falls within a unit cannot clear
        // its support (matching the Java anchor-engine gate).
        let mut foreign_unknown = anchor_unknown.clone();
        foreign_unknown.origin.engine = "untrusted-provider".to_string();
        assert!(
            classify_family_unknown_fact(
                "typescript",
                &foreign_unknown,
                "framework:express.route_handler"
            )
            .is_none(),
            "a foreign-engine UNKNOWN must not produce a blocking family unknown"
        );
        assert!(
            classify_unknown_family_effect(
                "typescript",
                UnknownReasonCode::DynamicImport,
                "tsjs_receiver_binding",
                Some("framework:express.route_handler"),
                "untrusted-provider",
                "external_unknown",
            )
            .is_none(),
            "a foreign-engine UNKNOWN must not produce a query-visible family effect"
        );
    }

    #[test]
    fn foreign_unknown_does_not_suppress_exact_anchor_family_support() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let mut foreign_unknown = tsjs_unknown_fact(
            &third,
            UnknownReasonCode::DynamicImport,
            "tsjs_receiver_binding",
        );
        foreign_unknown.origin.engine = "untrusted-provider".to_string();
        foreign_unknown.origin.method = "external_unknown".to_string();

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                tsjs_derived_fact(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                ),
                tsjs_derived_fact(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                ),
                foreign_unknown,
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
        assert!(
            report.unknowns.iter().all(|unknown| {
                unknown.claim_impact() != Some(ClaimImpact::Blocking)
                    || unknown.reason != UnknownReasonCode::DynamicImport
            }),
            "foreign UNKNOWNs must not suppress family support: {report:?}"
        );
    }

    #[test]
    fn tsjs_variation_slots_surface_route_method_path_and_handler_shape() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                tsjs_derived_fact_with_assumptions(
                    &first,
                    "express.route.get",
                    "framework:express.route_handler",
                    vec![
                        "route_method=get",
                        "route_path_shape=/users",
                        "handler_shape=inline_json",
                    ],
                ),
                tsjs_derived_fact_with_assumptions(
                    &second,
                    "express.route.post",
                    "framework:express.route_handler",
                    vec![
                        "route_method=post",
                        "route_path_shape=/users",
                        "handler_shape=inline_json",
                    ],
                ),
                tsjs_derived_fact_with_assumptions(
                    &third,
                    "express.route.delete",
                    "framework:express.route_handler",
                    vec![
                        "route_method=delete",
                        "route_path_shape=/users/:param",
                        "handler_shape=inline_json",
                    ],
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert!(claim
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:tsjs_route_method"));
        assert!(claim
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:tsjs_route_path_shape"));
        assert!(!claim
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:tsjs_handler_shape"));
    }

    #[test]
    fn python_family_requires_three_compatible_canonical_support_facts() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);

        let low_support = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.FastAPI.post"),
            ],
        );
        assert!(low_support.claims.is_empty());
        assert!(low_support
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.FastAPI.post"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.delete"),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "python");
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn exact_three_member_python_framework_positives_form_families() {
        assert_python_three_member_family(
            "fastapi_route",
            "framework:fastapi.route",
            [
                "fastapi.APIRouter.get",
                "fastapi.FastAPI.post",
                "fastapi.APIRouter.delete",
            ],
        );
        assert_python_three_member_family(
            "pytest_test",
            "framework:pytest.test",
            ["pytest.test", "pytest.test", "pytest.test"],
        );
        assert_python_three_member_family(
            "pytest_fixture",
            "framework:pytest.fixture",
            ["pytest.fixture", "pytest.fixture", "pytest.fixture"],
        );
        assert_python_three_member_family(
            "pydantic_model",
            "framework:pydantic.model",
            [
                "pydantic.BaseModel",
                "pydantic.BaseModel",
                "pydantic.BaseModel",
            ],
        );
        assert_python_three_member_family(
            "sqlalchemy_model",
            "framework:sqlalchemy.model",
            [
                "sqlalchemy.orm.DeclarativeBase",
                "sqlalchemy.orm.Mapped",
                "sqlalchemy.orm.mapped_column",
            ],
        );
        assert_python_three_member_family(
            "sqlalchemy_repository_method",
            "framework:sqlalchemy.repository_method",
            [
                "sqlalchemy.orm.Session.execute",
                "sqlalchemy.orm.Session.scalar",
                "sqlalchemy.ext.asyncio.AsyncSession.scalars",
            ],
        );
    }

    #[test]
    fn pytest_fixture_support_does_not_prove_pytest_test_family() {
        let first = python_unit("tests/test_a.py", "pytest_test", 0);
        let second = python_unit("tests/test_b.py", "pytest_test", 1);
        let third = python_unit("tests/test_c.py", "pytest_test", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pytest.test"),
                role_fact(&second, "framework:pytest.test"),
                role_fact(&third, "framework:pytest.test"),
                semantic_support_fact_with_target(&first, "pytest.fixture"),
                semantic_support_fact_with_target(&second, "pytest.fixture"),
                semantic_support_fact_with_target(&third, "pytest.fixture"),
            ],
        );

        assert_insufficient_support(&report);
    }

    #[test]
    fn python_structural_framework_anchors_cannot_directly_support_membership() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                python_context_fact(
                    &first,
                    "fastapi_route_decorator",
                    Some("fastapi.APIRouter.get"),
                ),
                python_context_fact(
                    &second,
                    "fastapi_route_decorator",
                    Some("fastapi.APIRouter.get"),
                ),
                python_context_fact(
                    &third,
                    "fastapi_route_decorator",
                    Some("fastapi.APIRouter.get"),
                ),
            ],
        );

        assert_insufficient_support(&report);
    }

    #[test]
    fn local_client_get_anchor_does_not_support_fastapi_family() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                parser_origin_strong_python_anchor_fact(
                    &first,
                    SemanticFactKind::Symbol,
                    "fastapi_route_decorator",
                    "fastapi.APIRouter.get",
                    "local_receiver=client",
                ),
                parser_origin_strong_python_anchor_fact(
                    &second,
                    SemanticFactKind::Symbol,
                    "fastapi_route_decorator",
                    "fastapi.APIRouter.get",
                    "local_receiver=client",
                ),
                parser_origin_strong_python_anchor_fact(
                    &third,
                    SemanticFactKind::Symbol,
                    "fastapi_route_decorator",
                    "fastapi.APIRouter.get",
                    "local_receiver=client",
                ),
            ],
        );

        assert_insufficient_support(&report);
    }

    #[test]
    fn shadowed_pydantic_and_user_sqlalchemy_bases_do_not_support_families() {
        let first = python_unit("schemas.py", "pydantic_model", 0);
        let second = python_unit("schemas.py", "pydantic_model", 1);
        let third = python_unit("schemas.py", "pydantic_model", 2);
        let pydantic_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pydantic.model"),
                role_fact(&second, "framework:pydantic.model"),
                role_fact(&third, "framework:pydantic.model"),
                parser_origin_strong_python_anchor_fact(
                    &first,
                    SemanticFactKind::Type,
                    "class_base",
                    "pydantic.BaseModel",
                    "shadowed_symbol=BaseModel",
                ),
                parser_origin_strong_python_anchor_fact(
                    &second,
                    SemanticFactKind::Type,
                    "class_base",
                    "pydantic.BaseModel",
                    "shadowed_symbol=BaseModel",
                ),
                parser_origin_strong_python_anchor_fact(
                    &third,
                    SemanticFactKind::Type,
                    "class_base",
                    "pydantic.BaseModel",
                    "shadowed_symbol=BaseModel",
                ),
            ],
        );
        assert_insufficient_support(&pydantic_report);

        let first = python_unit("models.py", "sqlalchemy_model", 0);
        let second = python_unit("models.py", "sqlalchemy_model", 1);
        let third = python_unit("models.py", "sqlalchemy_model", 2);
        let sqlalchemy_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:sqlalchemy.model"),
                role_fact(&second, "framework:sqlalchemy.model"),
                role_fact(&third, "framework:sqlalchemy.model"),
                parser_origin_strong_python_anchor_fact(
                    &first,
                    SemanticFactKind::Type,
                    "class_base",
                    "sqlalchemy.orm.DeclarativeBase",
                    "user_defined_base=Base",
                ),
                parser_origin_strong_python_anchor_fact(
                    &second,
                    SemanticFactKind::Type,
                    "class_base",
                    "sqlalchemy.orm.DeclarativeBase",
                    "user_defined_base=Base",
                ),
                parser_origin_strong_python_anchor_fact(
                    &third,
                    SemanticFactKind::Type,
                    "class_base",
                    "sqlalchemy.orm.DeclarativeBase",
                    "user_defined_base=Base",
                ),
            ],
        );
        assert_insufficient_support(&sqlalchemy_report);
    }

    #[test]
    fn pydantic_validator_side_effect_unknown_does_not_block_model_membership() {
        let first = python_unit("schemas.py", "pydantic_model", 0);
        let second = python_unit("schemas.py", "pydantic_model", 1);
        let third = python_unit("schemas.py", "pydantic_model", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pydantic.model"),
                role_fact(&second, "framework:pydantic.model"),
                role_fact(&third, "framework:pydantic.model"),
                semantic_support_fact_with_target(&first, "pydantic.BaseModel"),
                semantic_support_fact_with_target(&second, "pydantic.BaseModel"),
                semantic_support_fact_with_target(&third, "pydantic.BaseModel"),
                python_unknown_fact(
                    &second,
                    UnknownReasonCode::FrameworkMagic,
                    "pydantic_validator_side_effects",
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
        assert!(report.claims[0].unknowns.iter().any(|unknown| {
            unknown.claim_impact() == Some(ClaimImpact::NonBlocking)
                && unknown.reason == UnknownReasonCode::FrameworkMagic
                && unknown.affected_claim
                    == format!(
                        "{}:pydantic_validator_side_effects",
                        report.claims[0].family_id
                    )
        }));
    }

    #[test]
    fn python_support_requires_fresh_exact_content_hash() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_hash(&third, "fastapi.APIRouter.get", other_hash()),
            ],
        );

        assert_insufficient_support(&report);
    }

    #[test]
    fn python_family_records_exact_anchor_target_variation_metadata() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);

        let same_target = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
            ],
        );
        assert_eq!(same_target.claims.len(), 1);
        let same_target_records = family_storage_records(&same_target.claims[0]);
        assert!(!same_target_records
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:python_framework_anchor_target"));
        assert!(same_target_records.evidence.iter().all(|evidence| {
            !evidence
                .covered_claims
                .iter()
                .any(|claim| claim == "variation" || claim == "exception")
        }));

        let varied_target = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.FastAPI.post"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.delete"),
            ],
        );
        assert_eq!(varied_target.claims.len(), 1);
        let claim = &varied_target.claims[0];
        assert!(claim
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:runtime_unknown"));
        assert!(claim
            .variation_slots
            .iter()
            .any(|slot| slot.slot_id == "slot:python_framework_anchor_target"
                && slot.description
                    == "variation:python_framework_anchor_target:exact compatible framework anchors differ"));

        let records = family_storage_records(claim);
        assert_eq!(
            records.evidence[0].covered_claims,
            vec!["canonical".to_string(), "support".to_string()]
        );
        let variation_records = records
            .evidence
            .iter()
            .filter(|evidence| {
                evidence
                    .covered_claims
                    .iter()
                    .any(|claim| claim == "variation")
            })
            .collect::<Vec<_>>();
        assert_eq!(variation_records.len(), 1);
        assert!(variation_records[0]
            .covered_claims
            .contains(&"support".to_string()));
        assert!(records.evidence.iter().all(|evidence| {
            !evidence
                .covered_claims
                .iter()
                .any(|claim| claim == "exception")
        }));
        let serialized = format!("{records:?}");
        assert!(!serialized.contains("fastapi.APIRouter.get"));
        assert!(!serialized.contains("fastapi.FastAPI.post"));
        assert!(!serialized.contains("@"));
    }

    #[test]
    fn python_complete_link_clustering_rejects_single_link_bridge() {
        let query_only = python_unit("app/query.py", "sqlalchemy_repository_method", 0);
        let bridge = python_unit("app/query_and_commit.py", "sqlalchemy_repository_method", 1);
        let transaction_only = python_unit("app/commit.py", "sqlalchemy_repository_method", 2);

        let report = build_family_claims(
            &[query_only.clone(), bridge.clone(), transaction_only.clone()],
            &[
                role_fact(&query_only, "framework:sqlalchemy.repository_method"),
                role_fact(&bridge, "framework:sqlalchemy.repository_method"),
                role_fact(&transaction_only, "framework:sqlalchemy.repository_method"),
                semantic_support_fact_with_target(&query_only, "sqlalchemy.orm.Session.execute"),
                semantic_support_fact_with_target(&bridge, "sqlalchemy.orm.Session.execute"),
                semantic_support_fact_with_target(&bridge, "sqlalchemy.orm.Session.commit"),
                semantic_support_fact_with_target(
                    &transaction_only,
                    "sqlalchemy.orm.Session.commit",
                ),
            ],
        );

        assert!(
            report.claims.is_empty(),
            "complete-link clustering must not let a bridge member connect incompatible Python support families"
        );
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_complete_link_clustering_splits_distinct_ready_support_families() {
        let units = [
            python_unit("app/query_a.py", "sqlalchemy_repository_method", 0),
            python_unit("app/query_b.py", "sqlalchemy_repository_method", 1),
            python_unit("app/query_c.py", "sqlalchemy_repository_method", 2),
            python_unit("app/transaction_a.py", "sqlalchemy_repository_method", 3),
            python_unit("app/transaction_b.py", "sqlalchemy_repository_method", 4),
            python_unit("app/transaction_c.py", "sqlalchemy_repository_method", 5),
        ];
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, "framework:sqlalchemy.repository_method"))
            .collect::<Vec<_>>();
        for unit in &units[0..3] {
            facts.push(semantic_support_fact_with_target(
                unit,
                "sqlalchemy.orm.Session.execute",
            ));
        }
        for unit in &units[3..6] {
            facts.push(semantic_support_fact_with_target(
                unit,
                "sqlalchemy.orm.Session.commit",
            ));
        }

        let report = build_family_claims(&units, &facts);

        assert_eq!(report.claims.len(), 2);
        assert!(report.claims.iter().all(|claim| claim.support == 3));
        // Two competing ready families of equal size split a six-peer key, so
        // minimum support must not be reported as dominance: each covers only
        // half its eligible peers and neither outnumbers the other.
        for claim in &report.claims {
            assert_eq!(
                claim.classification, "SUPPORTED_PATTERN",
                "a competing equal-size ready family must not be dominant"
            );
            assert_eq!(claim.prevalence.eligible_peer_count, 6);
            assert_eq!(claim.prevalence.supported_member_count, 3);
            assert_eq!(claim.prevalence.competing_ready_family_count, 1);
            assert_eq!(claim.prevalence.largest_competing_support, 3);
            assert_eq!(claim.prevalence.blocked_peer_count, 0);
            assert_eq!(claim.prevalence.unsupported_peer_count, 0);
            assert_eq!(claim.prevalence.coverage_ratio, Some(0.5));
        }
        let ids = report
            .claims
            .iter()
            .map(|claim| claim.family_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(
            "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method"
        ));
        assert!(ids.contains(
            "family:python:sqlalchemy_repository_method:framework_sqlalchemy_repository_method:cluster_sqlalchemy_transaction_boundary"
        ));
    }

    #[test]
    fn python_fastapi_dependency_target_variation_does_not_block_route_membership() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_context_fact(
                    &first,
                    "fastapi_dependency_target",
                    Some("app.dependencies.get_db"),
                ),
                python_context_fact(
                    &second,
                    "fastapi_dependency_target",
                    Some("app.dependencies.get_cache"),
                ),
                python_context_fact(
                    &third,
                    "fastapi_dependency_target",
                    Some("app.dependencies.get_session"),
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
        assert_eq!(report.claims[0].readiness, ClaimReadiness::Ready);
    }

    #[test]
    fn python_fastapi_context_differences_are_explicit_variation_metadata() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_context_fact(&first, "fastapi_response_model", Some("api.UserOut")),
                python_context_fact(
                    &second,
                    "fastapi_http_exception_status",
                    Some("fastapi.http_exception.status_code.404"),
                ),
                python_context_fact(
                    &third,
                    "fastapi_service_call",
                    Some("app.services.UserService.list_users"),
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.support, 3);
        assert!(claim.variation_slots.iter().any(|slot| {
            slot.slot_id == "slot:python_fastapi_effect_marker"
                && slot.description
                    == "variation:python_fastapi_effect_marker:context metadata differs across supported members"
        }));
        assert!(claim.variation_slots.iter().any(|slot| {
            slot.slot_id == "slot:python_fastapi_service_call_shape"
                && slot.description
                    == "variation:python_fastapi_service_call_shape:context metadata differs across supported members"
        }));
        let records = family_storage_records(claim);
        let serialized = format!("{records:?}");
        assert!(!serialized.contains("api.UserOut"));
        assert!(!serialized.contains("app.services"));
        assert!(!serialized.contains("@"));
    }

    #[test]
    fn python_pytest_non_builtin_fixture_context_must_match() {
        let first = python_unit("tests/a.py", "pytest_test", 0);
        let second = python_unit("tests/b.py", "pytest_test", 1);
        let third = python_unit("tests/c.py", "pytest_test", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pytest.test"),
                role_fact(&second, "framework:pytest.test"),
                role_fact(&third, "framework:pytest.test"),
                semantic_support_fact_with_target(&first, "pytest.test"),
                semantic_support_fact_with_target(&second, "pytest.test"),
                semantic_support_fact_with_target(&third, "pytest.test"),
                python_context_fact(&first, "pytest_fixture_edge", Some("pytest.fixture.client")),
                python_context_fact(
                    &second,
                    "pytest_fixture_edge",
                    Some("pytest.fixture.client"),
                ),
                python_context_fact(&third, "pytest_fixture_edge", Some("pytest.fixture.db")),
            ],
        );

        assert!(report.claims.is_empty());
        assert_unknown_reason(&report, UnknownReasonCode::InsufficientSupport);
    }

    #[test]
    fn python_pytest_builtin_fixture_context_variation_is_metadata_only() {
        let first = python_unit("tests/a.py", "pytest_test", 0);
        let second = python_unit("tests/b.py", "pytest_test", 1);
        let third = python_unit("tests/c.py", "pytest_test", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pytest.test"),
                role_fact(&second, "framework:pytest.test"),
                role_fact(&third, "framework:pytest.test"),
                semantic_support_fact_with_target(&first, "pytest.test"),
                semantic_support_fact_with_target(&second, "pytest.test"),
                semantic_support_fact_with_target(&third, "pytest.test"),
                python_context_fact(
                    &first,
                    "pytest_builtin_fixture_context",
                    Some("pytest.builtin_fixture.tmp_path"),
                ),
                python_context_fact(
                    &second,
                    "pytest_builtin_fixture_context",
                    Some("pytest.builtin_fixture.capsys"),
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.support, 3);
        assert!(claim.variation_slots.iter().any(|slot| {
            slot.slot_id == "slot:python_pytest_fixture_context"
                && slot.description
                    == "variation:python_pytest_fixture_context:context metadata differs across supported members"
        }));
    }

    #[test]
    fn python_pytest_plugin_fixture_context_variation_is_metadata_only() {
        // Well-known plugin fixture context is external, like a built-in
        // fixture: differing plugin context across members stays metadata-only
        // and does not become a hard family compatibility constraint.
        let first = python_unit("tests/a.py", "pytest_test", 0);
        let second = python_unit("tests/b.py", "pytest_test", 1);
        let third = python_unit("tests/c.py", "pytest_test", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pytest.test"),
                role_fact(&second, "framework:pytest.test"),
                role_fact(&third, "framework:pytest.test"),
                semantic_support_fact_with_target(&first, "pytest.test"),
                semantic_support_fact_with_target(&second, "pytest.test"),
                semantic_support_fact_with_target(&third, "pytest.test"),
                python_context_fact(
                    &first,
                    "pytest_plugin_fixture_context",
                    Some("pytest.plugin_fixture.mocker"),
                ),
                python_context_fact(
                    &second,
                    "pytest_plugin_fixture_context",
                    Some("pytest.plugin_fixture.freezer"),
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn python_blocking_unknown_removes_claim_relevant_support() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_unknown_fact(
                    &third,
                    UnknownReasonCode::DynamicImport,
                    "python_import_resolution",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert_unknown_reason(&report, UnknownReasonCode::DynamicImport);
        assert_unknown_reason(&report, UnknownReasonCode::InsufficientSupport);
    }

    #[test]
    fn family_prevalence_counts_eligible_blocked_and_unsupported_peers() {
        // Three eligible peers form the ready cluster; a blocking unknown empties
        // one peer's support, and one peer never had support facts. Blocked and
        // unsupported peers are excluded from the denominator but recorded.
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let blocked = python_unit("app/d.py", "fastapi_route", 3);
        let unsupported = python_unit("app/e.py", "fastapi_route", 4);
        let report = build_family_claims(
            &[
                first.clone(),
                second.clone(),
                third.clone(),
                blocked.clone(),
                unsupported.clone(),
            ],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                role_fact(&blocked, "framework:fastapi.route"),
                role_fact(&unsupported, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&blocked, "fastapi.APIRouter.get"),
                python_unknown_fact(
                    &blocked,
                    UnknownReasonCode::DynamicImport,
                    "python_import_resolution",
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let prevalence = &report.claims[0].prevalence;
        assert_eq!(prevalence.eligible_peer_count, 3);
        assert_eq!(prevalence.supported_member_count, 3);
        assert_eq!(prevalence.blocked_peer_count, 1);
        assert_eq!(prevalence.unsupported_peer_count, 1);
        assert_eq!(prevalence.competing_ready_family_count, 0);
        assert_eq!(prevalence.largest_competing_support, 0);
        assert_eq!(prevalence.coverage_ratio, Some(1.0));
        // A reliable denominator (blocked <= eligible) at full coverage with no
        // competitor is dominant.
        assert_eq!(report.claims[0].classification, "DOMINANT_PATTERN");
        assert_eq!(
            prevalence.classification_reason,
            "coverage 3/3 with no competing ready family"
        );
    }

    #[test]
    fn python_monkey_patch_unknown_blocks_claim_relevant_support() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_unknown_fact(&third, UnknownReasonCode::MonkeyPatch, "python_call_target"),
            ],
        );

        assert!(report.claims.is_empty());
        assert_unknown_reason(&report, UnknownReasonCode::MonkeyPatch);
        assert_unknown_reason(&report, UnknownReasonCode::InsufficientSupport);
    }

    #[test]
    fn python_framework_identity_unknown_blocks_claim_relevant_support() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_unknown_fact(
                    &third,
                    UnknownReasonCode::FrameworkMagic,
                    "python_framework_identity",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert_unknown_reason(&report, UnknownReasonCode::FrameworkMagic);
        assert_unknown_reason(&report, UnknownReasonCode::InsufficientSupport);
    }

    #[test]
    fn fastapi_dependency_target_unknown_does_not_block_route_family_membership() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&third, "fastapi.APIRouter.get"),
                python_unknown_fact(
                    &third,
                    UnknownReasonCode::RuntimeDependencyInjection,
                    "fastapi_dependency_target",
                ),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn pytest_fixture_binding_unknown_blocks_pytest_family_membership() {
        let first = python_unit("tests/test_a.py", "pytest_test", 0);
        let second = python_unit("tests/test_b.py", "pytest_test", 1);
        let third = python_unit("tests/test_c.py", "pytest_test", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:pytest.test"),
                role_fact(&second, "framework:pytest.test"),
                role_fact(&third, "framework:pytest.test"),
                semantic_support_fact_with_target(&first, "pytest.test"),
                semantic_support_fact_with_target(&second, "pytest.test"),
                semantic_support_fact_with_target(&third, "pytest.test"),
                python_unknown_fact(
                    &third,
                    UnknownReasonCode::PytestFixtureInjection,
                    "pytest_fixture_binding",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert_unknown_reason(&report, UnknownReasonCode::PytestFixtureInjection);
        assert_unknown_reason(&report, UnknownReasonCode::InsufficientSupport);
    }

    #[test]
    fn python_framework_support_uses_exact_targets_not_substrings() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "myproject.fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get_extra"),
                semantic_support_fact_with_target(&third, "notes:fastapi.FastAPI.post"),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_framework_support_requires_target_before_fallback() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let mut targetless = semantic_support_fact_with_target(&third, "fastapi.APIRouter.get");
        targetless.target = None;

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.APIRouter.get"),
                semantic_support_fact_with_target(&second, "fastapi.APIRouter.get"),
                targetless,
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_provider_origin_still_requires_canonical_support_targets() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_origin(
                    &first,
                    "myproject.fastapi.APIRouter.get",
                    "pyrefly",
                    "definition",
                ),
                semantic_support_fact_with_origin(
                    &second,
                    "fastapi.APIRouter.get_extra",
                    "pyright",
                    "type_definition",
                ),
                semantic_support_fact_with_origin(
                    &third,
                    "notes:fastapi.FastAPI.post",
                    "pyrefly",
                    "call_hierarchy",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_provider_support_requires_exact_unit_evidence_range() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);

        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_range(&first, "fastapi.APIRouter.get", 1, 9),
                semantic_support_fact_with_range(&second, "fastapi.FastAPI.post", 1, 9),
                semantic_support_fact_with_range(&third, "fastapi.APIRouter.delete", 1, 9),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_fastapi_auxiliary_context_effect_targets_do_not_prove_route_family() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "fastapi.Depends"),
                semantic_support_fact_with_target(&second, "fastapi.dependency.get_db"),
                semantic_support_fact_with_target(&third, "fastapi.http_exception.status_code.404"),
                semantic_support_fact_with_target(&third, "fastapi.response_model.UserOut"),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_fastapi_service_call_targets_do_not_prove_route_family() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_support_fact_with_target(&first, "app.services.UserService.list_users"),
                semantic_support_fact_with_target(&second, "app.services.UserService.create_user"),
                semantic_support_fact_with_target(
                    &third,
                    "app.repositories.UserRepository.list_users",
                ),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn python_sqlalchemy_auxiliary_targets_do_not_prove_model_or_repository_family() {
        let first = python_unit("models.py", "sqlalchemy_model", 0);
        let second = python_unit("models.py", "sqlalchemy_model", 1);
        let third = python_unit("models.py", "sqlalchemy_model", 2);
        let model_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:sqlalchemy.model"),
                role_fact(&second, "framework:sqlalchemy.model"),
                role_fact(&third, "framework:sqlalchemy.model"),
                semantic_support_fact_with_target(&first, "sqlalchemy.orm.relationship"),
                semantic_support_fact_with_target(&second, "sqlalchemy.orm.relationship"),
                semantic_support_fact_with_target(&third, "sqlalchemy.orm.relationship"),
            ],
        );

        assert!(model_report.claims.is_empty());
        assert!(model_report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));

        let first = python_unit("repository.py", "sqlalchemy_repository_method", 0);
        let second = python_unit("repository.py", "sqlalchemy_repository_method", 1);
        let third = python_unit("repository.py", "sqlalchemy_repository_method", 2);
        let repository_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:sqlalchemy.repository_method"),
                role_fact(&second, "framework:sqlalchemy.repository_method"),
                role_fact(&third, "framework:sqlalchemy.repository_method"),
                semantic_support_fact_with_target(&first, "sqlalchemy.orm.Session.add"),
                semantic_support_fact_with_target(&second, "sqlalchemy.orm.Session.add"),
                semantic_support_fact_with_target(&third, "sqlalchemy.orm.Session.add"),
            ],
        );

        assert!(repository_report.claims.is_empty());
        assert!(repository_report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn project_config_facts_never_prove_family_membership() {
        let first = python_unit("app/a.py", "fastapi_route", 0);
        let second = python_unit("app/b.py", "fastapi_route", 1);
        let third = python_unit("app/c.py", "fastapi_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:fastapi.route"),
                role_fact(&second, "framework:fastapi.route"),
                role_fact(&third, "framework:fastapi.route"),
                semantic_project_config_fact(&first),
                semantic_project_config_fact(&second),
                semantic_project_config_fact(&third),
            ],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn low_support_and_missing_roles_stay_unknown() {
        let first = unit("src/a.ts", "react_component", 0);
        let second = unit("src/b.ts", "react_component", 1);
        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[role_fact(&first, "framework:react.component")],
        );

        assert!(report.claims.is_empty());
        assert!(report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn unsupported_react_semantic_facts_do_not_form_public_preview_families() {
        let first = unit("src/a.tsx", "react_component", 0);
        let second = unit("src/b.tsx", "react_component", 1);
        let third = unit("src/c.tsx", "react_component", 2);
        let component_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:react.component"),
                role_fact(&second, "framework:react.component"),
                role_fact(&third, "framework:react.component"),
                semantic_support_fact_with_target(&first, "react.component"),
                semantic_support_fact_with_target(&second, "react.component"),
                semantic_support_fact_with_target(&third, "react.component"),
            ],
        );

        assert!(component_report.claims.is_empty());
        assert!(component_report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));

        let first = unit("src/hooks.ts", "react_hook", 0);
        let second = unit("src/more-hooks.ts", "react_hook", 1);
        let third = unit("src/use-feature.ts", "react_hook", 2);
        let hook_report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:react.hook"),
                role_fact(&second, "framework:react.hook"),
                role_fact(&third, "framework:react.hook"),
                semantic_support_fact_with_target(&first, "package:react"),
                semantic_support_fact_with_target(&second, "package:react"),
                semantic_support_fact_with_target(&third, "package:react"),
            ],
        );

        assert!(hook_report.claims.is_empty());
        assert!(hook_report
            .unknowns
            .iter()
            .any(|unknown| unknown.reason == UnknownReasonCode::InsufficientSupport));
    }

    #[test]
    fn storage_records_do_not_contain_source_snippets_or_absolute_paths() {
        let first = unit("src/a.ts", "test_case", 0);
        let second = unit("src/b.ts", "test_case", 1);
        let third = unit("src/c.ts", "test_case", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:jest_vitest.test"),
                role_fact(&second, "framework:jest_vitest.test"),
                role_fact(&third, "framework:jest_vitest.test"),
                tsjs_derived_fact(&first, "jest_vitest.it", "framework:jest_vitest.test"),
                tsjs_derived_fact(&second, "jest_vitest.it", "framework:jest_vitest.test"),
                tsjs_derived_fact(&third, "jest_vitest.it", "framework:jest_vitest.test"),
            ],
        );
        let records = family_storage_records(&report.claims[0]);
        let serialized = format!("{records:?}");

        assert!(!serialized.contains("=>"));
        assert!(!serialized.contains("it("));
        assert!(!serialized.contains("/tmp"));
        assert_eq!(records.members.len(), 3);
        assert_eq!(records.evidence.len(), 3);
    }

    fn csharp_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(path, "csharp", kind, index)
    }

    fn csharp_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework_role: &str,
        extra_assumptions: Vec<String>,
    ) -> SemanticFact {
        let mut assumptions = vec![
            "provider_resolved=false".to_string(),
            "derived_from=tree_sitter_csharp_structural_anchors".to_string(),
            format!(
                "derived_from={}",
                csharp::support_family(target, framework_role)
            ),
            format!("framework_role={framework_role}"),
            format!("csharp_anchor_kind={}", unit.kind),
        ];
        assumptions.extend(extra_assumptions);
        assumptions.sort();
        assumptions.dedup();
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: CSHARP_DERIVED_SUPPORT_ENGINE.to_string(),
                engine_version: "0.1.0".to_string(),
                method: CSHARP_DERIVED_SUPPORT_METHOD.to_string(),
            },
            certainty: FactCertainty::DataflowDerived,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded C# framework structural role support",
            )
            .expect("valid evidence"),
            assumptions,
        }
    }

    fn csharp_action_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        attribute: &str,
        http_method: &str,
        template_shape: &str,
    ) -> SemanticFact {
        csharp_derived_fact(
            unit,
            target,
            "framework:aspnetcore.controller_action",
            vec![
                format!("aspnet_attribute={attribute}"),
                format!("http_method={http_method}"),
                format!("route_template_shape={template_shape}"),
                "class_route_template_shape=literal".to_string(),
                "csharp_return_shape=action_result".to_string(),
                "csharp_parameter_shape=arity_0".to_string(),
            ],
        )
    }

    fn csharp_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#csharp_unknown", unit.id),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason")),
            origin: FactOrigin {
                engine: CSHARP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CSHARP_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed C# UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn csharp_family_requires_three_compatible_exact_anchor_support_facts() {
        let units = (0..3)
            .map(|index| {
                csharp_unit(
                    &format!("src/Controllers/{index}Controller.cs"),
                    "aspnet_controller_action",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let role = "framework:aspnetcore.controller_action";

        let low_support = build_family_claims(
            &units[..2],
            &units[..2]
                .iter()
                .flat_map(|unit| {
                    [
                        role_fact(unit, role),
                        csharp_action_derived_fact(
                            unit,
                            "aspnetcore.mvc.HttpGet",
                            "HttpGet",
                            "GET",
                            "literal",
                        ),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        assert_insufficient_support(&low_support);

        let facts = units
            .iter()
            .flat_map(|unit| {
                [
                    role_fact(unit, role),
                    csharp_action_derived_fact(
                        unit,
                        "aspnetcore.mvc.HttpGet",
                        "HttpGet",
                        "GET",
                        "literal",
                    ),
                ]
            })
            .collect::<Vec<_>>();
        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "csharp");
        assert_eq!(report.claims[0].framework_role, role);
        assert_eq!(report.claims[0].support, 3);
        assert!(report.claims[0].family_id.starts_with(
            "family:csharp:aspnet_controller_action:framework_aspnetcore_controller_action"
        ));
    }

    #[test]
    fn csharp_controller_action_clustering_separates_http_methods() {
        let units = (0..6)
            .map(|index| {
                csharp_unit(
                    &format!("src/Controllers/{index}Controller.cs"),
                    "aspnet_controller_action",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let role = "framework:aspnetcore.controller_action";
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, role))
            .collect::<Vec<_>>();
        for unit in units.iter().take(3) {
            facts.push(csharp_action_derived_fact(
                unit,
                "aspnetcore.mvc.HttpGet",
                "HttpGet",
                "GET",
                "literal",
            ));
        }
        for unit in units.iter().skip(3) {
            facts.push(csharp_action_derived_fact(
                unit,
                "aspnetcore.mvc.HttpPost",
                "HttpPost",
                "POST",
                "literal",
            ));
        }

        let report = build_family_claims(&units, &facts);

        assert_eq!(report.claims.len(), 2);
        assert!(report.claims.iter().all(|claim| claim.support == 3));
        let family_ids = report
            .claims
            .iter()
            .map(|claim| claim.family_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(family_ids.len(), 2);
    }

    #[test]
    fn csharp_runtime_unknown_subclaims_do_not_block_exact_support() {
        let units = (0..3)
            .map(|index| {
                csharp_unit(
                    &format!("src/Controllers/{index}Controller.cs"),
                    "aspnet_controller",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let role = "framework:aspnetcore.controller";
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, role))
            .collect::<Vec<_>>();
        for unit in &units {
            facts.push(csharp_derived_fact(
                unit,
                "aspnetcore.mvc.ApiController",
                role,
                vec![
                    "aspnet_attribute=ApiController".to_string(),
                    "csharp_class_shape=class".to_string(),
                    "csharp_visibility_shape=public".to_string(),
                ],
            ));
        }
        facts.extend([
            csharp_unknown_fact(
                &units[0],
                UnknownReasonCode::RuntimeDependencyInjection,
                "csharp_di_registration",
            ),
            csharp_unknown_fact(
                &units[0],
                UnknownReasonCode::FrameworkMagic,
                "csharp_aspnet_filter_pipeline",
            ),
        ]);

        let report = build_family_claims(&units, &facts);

        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
        for claim in ["csharp_di_registration", "csharp_aspnet_filter_pipeline"] {
            assert!(report.claims[0].unknowns.iter().any(|unknown| {
                unknown.claim_impact() == Some(ClaimImpact::NonBlocking)
                    && unknown.affected_claim.ends_with(claim)
            }));
        }
    }

    #[test]
    fn csharp_structural_anchors_cannot_directly_support_membership() {
        let units = (0..3)
            .map(|index| {
                csharp_unit(
                    &format!("src/Controllers/{index}Controller.cs"),
                    "aspnet_controller_action",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let role = "framework:aspnetcore.controller_action";
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, role))
            .collect::<Vec<_>>();
        for unit in &units {
            let mut structural = csharp_action_derived_fact(
                unit,
                "aspnetcore.mvc.HttpGet",
                "HttpGet",
                "GET",
                "literal",
            );
            structural.origin.engine = CSHARP_ANCHOR_ENGINE.to_string();
            structural.origin.method = CSHARP_ANCHOR_METHOD.to_string();
            structural.certainty = FactCertainty::Structural;
            facts.push(structural);
        }

        assert_insufficient_support(&build_family_claims(&units, &facts));
    }

    #[test]
    fn csharp_support_requires_safe_origin_and_exact_target() {
        let units = (0..3)
            .map(|index| {
                csharp_unit(
                    &format!("src/Controllers/{index}Controller.cs"),
                    "aspnet_controller_action",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let role = "framework:aspnetcore.controller_action";

        let mut wrong_engine = csharp_action_derived_fact(
            &units[0],
            "aspnetcore.mvc.HttpGet",
            "HttpGet",
            "GET",
            "literal",
        );
        wrong_engine.origin.engine = "csharp-lookalike".to_string();
        let wrong_engine_report = build_family_claims(
            &units,
            &[
                role_fact(&units[0], role),
                role_fact(&units[1], role),
                role_fact(&units[2], role),
                wrong_engine,
                csharp_action_derived_fact(
                    &units[1],
                    "aspnetcore.mvc.HttpGet",
                    "HttpGet",
                    "GET",
                    "literal",
                ),
                csharp_action_derived_fact(
                    &units[2],
                    "aspnetcore.mvc.HttpGet",
                    "HttpGet",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_insufficient_support(&wrong_engine_report);

        let substring_report = build_family_claims(
            &units,
            &[
                role_fact(&units[0], role),
                role_fact(&units[1], role),
                role_fact(&units[2], role),
                csharp_action_derived_fact(
                    &units[0],
                    "com.example.aspnetcore.mvc.HttpGet",
                    "HttpGet",
                    "GET",
                    "literal",
                ),
                csharp_action_derived_fact(
                    &units[1],
                    "aspnetcore.mvc.HttpGet",
                    "HttpGet",
                    "GET",
                    "literal",
                ),
                csharp_action_derived_fact(
                    &units[2],
                    "aspnetcore.mvc.HttpGet",
                    "HttpGet",
                    "GET",
                    "literal",
                ),
            ],
        );
        assert_insufficient_support(&substring_report);
    }

    fn cpp_unit(path: &str, kind: &str, index: usize) -> IndexedCodeUnitRecord {
        unit_with_language(path, "cpp", kind, index)
    }

    fn cpp_test_derived_fact(
        unit: &IndexedCodeUnitRecord,
        target: &str,
        framework: &str,
        test_macro: &str,
        name_shape: &str,
    ) -> SemanticFact {
        let framework_role = format!("framework:{framework}.test");
        let mut assumptions = vec![
            "provider_resolved=false".to_string(),
            "derived_from=tree_sitter_c_cpp_structural_anchors".to_string(),
            format!(
                "derived_from={}",
                cpp::support_family(target, &framework_role)
            ),
            format!("framework_role={framework_role}"),
            format!("cpp_anchor_kind={}", unit.kind),
            format!("test_framework={framework}"),
            format!("test_macro={test_macro}"),
            format!("test_name_shape={name_shape}"),
        ];
        assumptions.sort();
        assumptions.dedup();
        SemanticFact {
            kind: SemanticFactKind::ResolvedCall,
            subject: unit.id.clone(),
            target: Some(SymbolId::new(target).expect("valid target")),
            origin: FactOrigin {
                engine: CPP_DERIVED_SUPPORT_ENGINE.to_string(),
                engine_version: "0.1.0".to_string(),
                method: CPP_DERIVED_SUPPORT_METHOD.to_string(),
            },
            certainty: FactCertainty::DataflowDerived,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "bounded C/C++ framework structural role support",
            )
            .expect("valid evidence"),
            assumptions,
        }
    }

    fn cpp_unknown_fact(
        unit: &IndexedCodeUnitRecord,
        reason: UnknownReasonCode,
        affected_claim: &str,
    ) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::Unknown,
            subject: format!("{}#cpp_unknown", unit.id),
            target: Some(SymbolId::new(reason.as_protocol_str()).expect("valid UNKNOWN reason")),
            origin: FactOrigin {
                engine: CPP_ANCHOR_ENGINE.to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                method: CPP_ANCHOR_METHOD.to_string(),
            },
            certainty: FactCertainty::Unknown,
            evidence: Evidence::new(
                CodeUnitId::new(unit.id.clone()).expect("valid unit id"),
                SourceRange::new(unit.start_byte, unit.end_byte).expect("valid range"),
                Provenance::new(
                    &unit.path,
                    unit.content_hash.clone(),
                    RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                )
                .expect("valid provenance"),
                "typed C/C++ UNKNOWN",
            )
            .expect("valid evidence"),
            assumptions: vec![format!("affected_claim={affected_claim}")],
        }
    }

    #[test]
    fn cpp_family_requires_three_compatible_exact_anchor_support_facts() {
        let units = (0..3)
            .map(|index| cpp_unit(&format!("tests/{index}_test.cc"), "gtest_test_case", index))
            .collect::<Vec<_>>();
        let role = "framework:gtest.test";

        let low_support = build_family_claims(
            &units[..2],
            &units[..2]
                .iter()
                .flat_map(|unit| {
                    [
                        role_fact(unit, role),
                        cpp_test_derived_fact(
                            unit,
                            "gtest.TEST",
                            "gtest",
                            "TEST",
                            "identifier_pair",
                        ),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        assert_insufficient_support(&low_support);

        let facts = units
            .iter()
            .flat_map(|unit| {
                [
                    role_fact(unit, role),
                    cpp_test_derived_fact(unit, "gtest.TEST", "gtest", "TEST", "identifier_pair"),
                ]
            })
            .collect::<Vec<_>>();
        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].language, "cpp");
        assert_eq!(report.claims[0].framework_role, role);
        assert_eq!(report.claims[0].support, 3);
        assert!(report.claims[0]
            .family_id
            .starts_with("family:cpp:gtest_test_case:framework_gtest_test"));
    }

    #[test]
    fn cpp_gtest_and_catch2_never_cluster_together() {
        let gtest_units = (0..3)
            .map(|index| cpp_unit(&format!("tests/g{index}_test.cc"), "gtest_test_case", index))
            .collect::<Vec<_>>();
        let catch2_units = (3..6)
            .map(|index| {
                cpp_unit(
                    &format!("tests/c{index}_test.cpp"),
                    "catch2_test_case",
                    index,
                )
            })
            .collect::<Vec<_>>();
        let mut units = gtest_units.clone();
        units.extend(catch2_units.clone());
        let mut facts = Vec::new();
        for unit in &gtest_units {
            facts.push(role_fact(unit, "framework:gtest.test"));
            facts.push(cpp_test_derived_fact(
                unit,
                "gtest.TEST",
                "gtest",
                "TEST",
                "identifier_pair",
            ));
        }
        for unit in &catch2_units {
            facts.push(role_fact(unit, "framework:catch2.test"));
            facts.push(cpp_test_derived_fact(
                unit,
                "catch2.TEST_CASE",
                "catch2",
                "TEST_CASE",
                "string_literal",
            ));
        }

        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 2);
        assert!(report.claims.iter().all(|claim| claim.support == 3));
        let roles = report
            .claims
            .iter()
            .map(|claim| claim.framework_role.as_str())
            .collect::<BTreeSet<_>>();
        assert!(roles.contains("framework:gtest.test"));
        assert!(roles.contains("framework:catch2.test"));
    }

    #[test]
    fn cpp_non_blocking_subclaims_do_not_veto_exact_support() {
        let units = (0..3)
            .map(|index| cpp_unit(&format!("tests/{index}_test.cc"), "gtest_test_case", index))
            .collect::<Vec<_>>();
        let role = "framework:gtest.test";
        let mut facts = units
            .iter()
            .flat_map(|unit| {
                [
                    role_fact(unit, role),
                    cpp_test_derived_fact(unit, "gtest.TEST", "gtest", "TEST", "identifier_pair"),
                ]
            })
            .collect::<Vec<_>>();
        facts.push(cpp_unknown_fact(
            &units[0],
            UnknownReasonCode::FrameworkMagic,
            "cpp_indirect_dispatch",
        ));

        let report = build_family_claims(&units, &facts);
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].support, 3);
    }

    #[test]
    fn cpp_structural_anchors_cannot_directly_support_membership() {
        let units = (0..3)
            .map(|index| cpp_unit(&format!("tests/{index}_test.cc"), "gtest_test_case", index))
            .collect::<Vec<_>>();
        let role = "framework:gtest.test";
        let mut facts = units
            .iter()
            .map(|unit| role_fact(unit, role))
            .collect::<Vec<_>>();
        for unit in &units {
            let mut structural =
                cpp_test_derived_fact(unit, "gtest.TEST", "gtest", "TEST", "identifier_pair");
            structural.origin.engine = CPP_ANCHOR_ENGINE.to_string();
            structural.origin.method = CPP_ANCHOR_METHOD.to_string();
            structural.certainty = FactCertainty::Structural;
            facts.push(structural);
        }

        assert_insufficient_support(&build_family_claims(&units, &facts));
    }

    #[test]
    fn cpp_support_requires_safe_origin_and_exact_target() {
        let units = (0..3)
            .map(|index| cpp_unit(&format!("tests/{index}_test.cc"), "gtest_test_case", index))
            .collect::<Vec<_>>();
        let role = "framework:gtest.test";

        let mut wrong_engine =
            cpp_test_derived_fact(&units[0], "gtest.TEST", "gtest", "TEST", "identifier_pair");
        wrong_engine.origin.engine = "cpp-lookalike".to_string();
        let wrong_engine_report = build_family_claims(
            &units,
            &[
                role_fact(&units[0], role),
                role_fact(&units[1], role),
                role_fact(&units[2], role),
                wrong_engine,
                cpp_test_derived_fact(&units[1], "gtest.TEST", "gtest", "TEST", "identifier_pair"),
                cpp_test_derived_fact(&units[2], "gtest.TEST", "gtest", "TEST", "identifier_pair"),
            ],
        );
        assert_insufficient_support(&wrong_engine_report);

        let substring_report = build_family_claims(
            &units,
            &[
                role_fact(&units[0], role),
                role_fact(&units[1], role),
                role_fact(&units[2], role),
                cpp_test_derived_fact(
                    &units[0],
                    "vendored.gtest.TEST",
                    "gtest",
                    "TEST",
                    "identifier_pair",
                ),
                cpp_test_derived_fact(&units[1], "gtest.TEST", "gtest", "TEST", "identifier_pair"),
                cpp_test_derived_fact(&units[2], "gtest.TEST", "gtest", "TEST", "identifier_pair"),
            ],
        );
        assert_insufficient_support(&substring_report);
    }
}
