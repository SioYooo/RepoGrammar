//! Application-layer EC-MVFI-lite family claim construction.

use crate::core::model::{
    FactCertainty, SemanticFact, SemanticFactKind, TypedUnknown, UnknownClass, UnknownReasonCode,
};
use crate::ports::family_store::{
    IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord, IndexedFamilyRecord,
    IndexedVariationSlotRecord,
};
use crate::ports::index_store::IndexedCodeUnitRecord;
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_MIN_FAMILY_SUPPORT: usize = 2;
const PYTHON_MIN_FAMILY_SUPPORT: usize = 3;
const TSJS_MIN_FAMILY_SUPPORT: usize = 3;
const PYTHON_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-python-derived";
const PYTHON_DERIVED_SUPPORT_METHOD: &str = "bounded_ast_anchor_v1";
const PYTHON_FIXTURE_PROVIDER_ENGINE: &str = "python-fixture-provider";
const PYTHON_FIXTURE_PROVIDER_METHOD: &str = "release_fixture_semantic_support";
/// Engine/method that mint conservative TS/JS exact-anchor support facts.
pub(crate) const TSJS_DERIVED_SUPPORT_ENGINE: &str = "repogrammar-tsjs-derived";
pub(crate) const TSJS_DERIVED_SUPPORT_METHOD: &str = "bounded_exact_anchor_v1";
/// Pinned TS/JS semantic worker engine identity accepted as a safe support origin.
const TSJS_WORKER_ENGINE: &str = "typescript";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyCandidate {
    pub language: String,
    pub code_unit_kind: String,
    pub framework_role: String,
    pub normalized_shape: String,
    pub members: Vec<FamilyEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyClaim {
    pub family_id: String,
    pub classification: String,
    pub support: usize,
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
pub struct ClaimUnknown {
    pub class: UnknownClass,
    pub reason: UnknownReasonCode,
    pub affected_claim: String,
    pub recovery: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimReadiness {
    Ready,
    Unknown(ClaimUnknown),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let mut unknowns = blocking_unknowns
        .values()
        .flat_map(|unknowns| unknowns.iter().cloned())
        .collect::<Vec<_>>();

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
        groups.entry(key).or_default().push(FamilyEvidence {
            code_unit_id: unit.id.clone(),
            path: unit.path.clone(),
            content_hash: unit.content_hash.clone(),
            start_byte: unit.start_byte,
            end_byte: unit.end_byte,
            support_targets: if blocking_unknowns.contains_key(&unit.id) {
                Vec::new()
            } else {
                support_targets_by_unit
                    .get(&unit.id)
                    .map(|targets| targets.iter().cloned().collect())
                    .unwrap_or_default()
            },
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
        let clusters = complete_link_family_clusters(&key, supported_evidence, &features_by_unit);
        let ready_cluster_count = clusters
            .iter()
            .filter(|cluster| cluster.len() >= min_family_support(&key.language))
            .count();
        let mut emitted_ready_clusters = 0usize;
        for cluster in clusters {
            let cluster_suffix = (ready_cluster_count > 1)
                .then(|| family_cluster_signature(&key, &cluster, &features_by_unit));
            if cluster.len() < min_family_support(&key.language) {
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
            claims.push(family_claim_from_supported_evidence(
                &key,
                suffix,
                normalized_shape,
                cluster,
                &features_by_unit,
                &non_blocking_unknowns,
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
) -> FamilyClaim {
    let family_id = family_id(key, cluster_suffix);
    let runtime_unknown = ClaimUnknown {
        class: UnknownClass::NonBlocking,
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
    variation_slots.extend(non_blocking_unknown_variation_slots(&claim_unknowns));
    FamilyClaim {
        family_id,
        classification: "DOMINANT_PATTERN".to_string(),
        support: supported_evidence.len(),
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
                "import_binding" | "repo_local_import_binding" | "dynamic_import_literal" => {
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
    for assumption in &fact.assumptions {
        for (prefix, feature_prefix) in [
            ("route_method=", "route_method:"),
            ("route_path_shape=", "route_path_shape:"),
            ("handler_shape=", "handler_shape:"),
            ("runner_kind=", "runner_kind:"),
            ("test_shape=", "test_shape:"),
            ("async_shape=", "async_shape:"),
            ("import_context=", "import_context:"),
            ("path_alias=", "import_context:path_alias_"),
        ] {
            if let Some(value) = assumption.strip_prefix(prefix) {
                entry.insert(format!("{feature_prefix}{}", stable_token(value)));
            }
        }
    }

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
        }
        let is_support_target = support_targets_by_unit
            .get(code_unit_id)
            .is_some_and(|targets| targets.contains(target));
        if !is_support_target {
            entry.insert(format!("import_context:{}", stable_token(target)));
        }
    }

    if fact.kind == SemanticFactKind::Unknown || fact.certainty == FactCertainty::Unknown {
        if let Some(reason) = fact
            .target
            .as_ref()
            .and_then(|target| UnknownReasonCode::parse_protocol_str(target.as_str()).ok())
        {
            let affected_claim = fact
                .assumptions
                .iter()
                .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                .unwrap_or("tsjs_family_membership");
            entry.insert(format!(
                "unknown_reason:{}",
                stable_token(reason.as_protocol_str())
            ));
            if tsjs_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
            {
                entry.insert(format!(
                    "unknown_blocker:{}",
                    stable_token(reason.as_protocol_str())
                ));
            }
        }
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
        let unknown = if unit.language == "python" {
            python_family_blocking_unknown(fact, framework_role)
        } else if is_tsjs_language_name(&unit.language) {
            tsjs_family_blocking_unknown(fact, framework_role)
        } else {
            None
        };
        if let Some(unknown) = unknown {
            blocking
                .entry(code_unit_id.to_string())
                .or_default()
                .push(unknown);
        }
    }

    blocking
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
        let unknown = if unit.language == "python" {
            python_family_non_blocking_unknown(fact, framework_role)
        } else if is_tsjs_language_name(&unit.language) {
            tsjs_family_non_blocking_unknown(fact, framework_role)
        } else {
            None
        };
        if let Some(unknown) = unknown {
            non_blocking
                .entry(code_unit_id.to_string())
                .or_default()
                .push(unknown);
        }
    }

    non_blocking
}

pub(crate) fn python_family_unknown_blocks_claim(
    fact: &SemanticFact,
    framework_role: &str,
) -> bool {
    python_family_blocking_unknown(fact, framework_role).is_some()
}

fn python_family_blocking_unknown(
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
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("python_family_membership");

    if !python_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role) {
        return None;
    }

    Some(ClaimUnknown {
        class: UnknownClass::Blocking,
        reason,
        affected_claim: affected_claim.to_string(),
        recovery: Some("resolve the blocking Python UNKNOWN before claiming a family".to_string()),
    })
}

fn python_family_non_blocking_unknown(
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
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("python_family_membership");

    if python_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
        || !python_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
    {
        return None;
    }

    Some(ClaimUnknown {
        class: UnknownClass::NonBlocking,
        reason,
        affected_claim: affected_claim.to_string(),
        recovery: Some("resolve this Python subclaim before relying on it".to_string()),
    })
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
        "fastapi_dependency_target" => false,
        "pytest_fixture_binding" => framework_role.starts_with("framework:pytest"),
        "python_family_membership"
        | "python_import_resolution"
        | "python_call_target"
        | "python_framework_identity" => true,
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
}

fn tsjs_family_blocking_unknown(fact: &SemanticFact, framework_role: &str) -> Option<ClaimUnknown> {
    if fact.kind != SemanticFactKind::Unknown && fact.certainty != FactCertainty::Unknown {
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
        .unwrap_or("tsjs_family_membership");

    if !tsjs_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role) {
        return None;
    }

    Some(ClaimUnknown {
        class: UnknownClass::Blocking,
        reason,
        affected_claim: affected_claim.to_string(),
        recovery: Some("resolve the blocking TS/JS UNKNOWN before claiming a family".to_string()),
    })
}

fn tsjs_family_non_blocking_unknown(
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
    let affected_claim = fact
        .assumptions
        .iter()
        .find_map(|assumption| assumption.strip_prefix("affected_claim="))
        .unwrap_or("tsjs_family_membership");

    if tsjs_unknown_reason_blocks_family_membership(reason, affected_claim, framework_role)
        || !tsjs_unknown_is_non_blocking_family_subclaim(reason, affected_claim, framework_role)
    {
        return None;
    }

    Some(ClaimUnknown {
        class: UnknownClass::NonBlocking,
        reason,
        affected_claim: affected_claim.to_string(),
        recovery: Some("resolve this TS/JS subclaim before relying on it".to_string()),
    })
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
        | "tsjs_reexport_resolution" => true,
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
        "tsjs_handler_shape" | "tsjs_variation_detail" | "tsjs_optional_call_target"
    )
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
        "framework_jest_vitest_suite" | "framework_jest_vitest_test" => equal_feature_profiles(
            left,
            right,
            features_by_unit,
            &["runner_kind:", "test_shape:", "async_shape:"],
        ),
        "framework_react_component" | "framework_react_hook" => false,
        _ => true,
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
        .filter(|value| !value.starts_with("pytest_builtin_fixture_"))
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
                "handler_shape:",
                "runner_kind:",
                "test_shape:",
                "async_shape:",
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

fn support_target_family(target: &str, framework_role: &str) -> String {
    match framework_role {
        "framework:express.route_handler" => "express.route_handler".to_string(),
        "framework:jest_vitest.suite" => "jest_vitest.suite".to_string(),
        "framework:jest_vitest.test" => "jest_vitest.test".to_string(),
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
        _ => framework_role.to_string(),
    }
}

fn path_context(path: &str) -> String {
    let first_segment = path.split('/').next().unwrap_or("repo");
    match first_segment {
        "app" | "api" | "src" | "tests" | "test" => stable_token(first_segment),
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
    false
}

fn tsjs_support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
    let target_is_compatible = tsjs_support_target_is_role_compatible(target, framework_role)?;
    Some(target_is_compatible && tsjs_support_fact_has_safe_origin(fact, framework_role))
}

fn tsjs_support_fact_has_safe_origin(fact: &SemanticFact, framework_role: &str) -> bool {
    match fact.certainty {
        FactCertainty::DataflowDerived => {
            fact.origin.engine == TSJS_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == TSJS_DERIVED_SUPPORT_METHOD
                && fact_has_assumption(fact, "provider_resolved=false")
                && fact_has_assumption(fact, "derived_from=tsjs_structural_anchors")
                && fact_has_assumption(fact, &format!("framework_role={framework_role}"))
        }
        FactCertainty::Semantic => fact.origin.engine == TSJS_WORKER_ENGINE,
        _ => false,
    }
}

/// Exact target whitelist per TS/JS framework role. Mirrors the Python whitelist:
/// support must point at an exact recognized target, never at fact text that merely
/// contains a framework name.
pub(crate) fn tsjs_support_target_is_role_compatible(
    target: &str,
    framework_role: &str,
) -> Option<bool> {
    match framework_role {
        "framework:express.route_handler" => Some(matches!(
            target,
            "package:express"
                | "express.route.get"
                | "express.route.post"
                | "express.route.put"
                | "express.route.patch"
                | "express.route.delete"
                | "express.route.use"
        )),
        "framework:jest_vitest.suite" => Some(matches!(
            target,
            "package:vitest" | "package:@jest/globals" | "jest_vitest.describe"
        )),
        "framework:jest_vitest.test" => Some(matches!(
            target,
            "package:vitest" | "package:@jest/globals" | "jest_vitest.it" | "jest_vitest.test"
        )),
        "framework:react.component" | "framework:react.hook" => Some(false),
        _ if tsjs_framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn tsjs_framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:express")
        || framework_role.starts_with("framework:react")
        || framework_role.starts_with("framework:jest_vitest")
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
        FactCertainty::DataflowDerived => {
            fact.origin.engine == PYTHON_DERIVED_SUPPORT_ENGINE
                && fact.origin.method == PYTHON_DERIVED_SUPPORT_METHOD
                && fact_has_assumption(fact, "provider_resolved=false")
                && fact_has_assumption(fact, "derived_from=cpython_ast_structural_anchors")
                && fact_has_assumption(fact, &format!("framework_role={framework_role}"))
        }
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
                | "sqlalchemy.orm.Session.commit"
                | "sqlalchemy.orm.Session.rollback"
                | "sqlalchemy.orm.Session.scalar"
                | "sqlalchemy.orm.Session.scalars"
                | "sqlalchemy.ext.asyncio.AsyncSession.execute"
                | "sqlalchemy.ext.asyncio.AsyncSession.commit"
                | "sqlalchemy.ext.asyncio.AsyncSession.rollback"
                | "sqlalchemy.ext.asyncio.AsyncSession.scalar"
                | "sqlalchemy.ext.asyncio.AsyncSession.scalars"
        )),
        _ if python_framework_role_is_known(framework_role) => Some(false),
        _ => None,
    }
}

pub(crate) fn python_framework_role_is_known(framework_role: &str) -> bool {
    framework_role.starts_with("framework:fastapi")
        || framework_role.starts_with("framework:pytest")
        || framework_role.starts_with("framework:pydantic")
        || framework_role.starts_with("framework:sqlalchemy")
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
    )
}

pub(crate) fn min_family_support(language: &str) -> usize {
    if language == "python" {
        PYTHON_MIN_FAMILY_SUPPORT
    } else if is_tsjs_language_name(language) {
        TSJS_MIN_FAMILY_SUPPORT
    } else {
        DEFAULT_MIN_FAMILY_SUPPORT
    }
}

fn is_tsjs_language_name(language: &str) -> bool {
    language == "typescript" || language == "javascript"
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
    let typed = TypedUnknown::new(
        UnknownClass::Blocking,
        UnknownReasonCode::InsufficientSupport,
        affected_claim,
        Some("add another compatible implementation before claiming a family".to_string()),
    )
    .expect("static UNKNOWN values are valid");
    ClaimUnknown {
        class: typed.class,
        reason: typed.reason,
        affected_claim: typed.affected_claim,
        recovery: typed.recovery,
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

    fn semantic_support_fact(unit: &IndexedCodeUnitRecord) -> SemanticFact {
        semantic_support_fact_with_target(unit, "package:express")
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
    fn builds_family_only_with_repeated_compatible_semantic_support() {
        let first = unit("src/a.ts", "express_route", 0);
        let second = unit("src/b.ts", "express_route", 1);
        let third = unit("src/c.ts", "express_route", 2);
        let report = build_family_claims(
            &[first.clone(), second.clone(), third.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                role_fact(&third, "framework:express.route_handler"),
                semantic_support_fact(&first),
                semantic_support_fact(&second),
                semantic_support_fact(&third),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.classification, "DOMINANT_PATTERN");
        assert_eq!(claim.support, 3);
        assert_eq!(claim.evidence.len(), 3);
        assert_eq!(claim.unknowns[0].class, UnknownClass::NonBlocking);
        assert_eq!(claim.unknowns[0].reason, UnknownReasonCode::FrameworkMagic);
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
            unknown.class == UnknownClass::Blocking
                && unknown.reason == UnknownReasonCode::DynamicImport
        }));
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
                semantic_support_fact_with_target(&first, "package:vitest"),
                semantic_support_fact_with_target(&second, "package:vitest"),
                semantic_support_fact_with_target(&third, "package:vitest"),
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
}
