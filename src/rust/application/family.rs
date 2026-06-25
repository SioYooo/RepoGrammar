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
pub struct FamilyStorageRecords {
    pub family: IndexedFamilyRecord,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FamilyKey {
    language: String,
    code_unit_kind: String,
    framework_role: String,
    normalized_shape: String,
}

pub fn build_family_claims(
    units: &[IndexedCodeUnitRecord],
    semantic_facts: &[SemanticFact],
) -> FamilyBuildReport {
    let role_by_unit = framework_roles_by_unit(semantic_facts);
    let supported_units = eligible_support_by_unit(units, semantic_facts, &role_by_unit);
    let mut groups: BTreeMap<FamilyKey, Vec<FamilyEvidence>> = BTreeMap::new();
    let mut unknowns = Vec::new();

    for unit in units {
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
            .filter(|evidence| supported_units.contains(&evidence.code_unit_id))
            .collect::<Vec<_>>();
        if supported_evidence.len() < min_family_support(&key.language) {
            unknowns.push(insufficient_support_unknown(format!(
                "family:{}:{}:{}",
                key.language, key.code_unit_kind, key.framework_role
            )));
            continue;
        }
        let family_id = family_id(&key);
        let runtime_unknown = ClaimUnknown {
            class: UnknownClass::NonBlocking,
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: format!("{family_id}:runtime_equivalence"),
            recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
        };
        claims.push(FamilyClaim {
            family_id,
            classification: "DOMINANT_PATTERN".to_string(),
            support: supported_evidence.len(),
            language: key.language,
            code_unit_kind: key.code_unit_kind,
            framework_role: key.framework_role,
            normalized_shape: key.normalized_shape,
            evidence: supported_evidence,
            variation_slots: vec![VariationSlot {
                slot_id: "slot:runtime_unknown".to_string(),
                description: format!(
                    "{}:{}:{}",
                    runtime_unknown.class.as_protocol_str(),
                    runtime_unknown.reason.as_protocol_str(),
                    "runtime equivalence remains unproven"
                ),
            }],
            exceptions: Vec::new(),
            unknowns: vec![runtime_unknown],
            readiness: ClaimReadiness::Ready,
        });
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

pub fn family_storage_records(claim: &FamilyClaim) -> FamilyStorageRecords {
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
) -> BTreeSet<String> {
    let unit_by_id = units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut supported = BTreeSet::new();
    for fact in facts {
        if !fact.certainty.supports_family_membership()
            || matches!(
                fact.kind,
                SemanticFactKind::FrameworkRole | SemanticFactKind::Unknown
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
            supported.insert(code_unit_id.to_string());
        }
    }
    supported
}

fn support_fact_is_role_compatible(fact: &SemanticFact, framework_role: &str) -> bool {
    if let Some(result) = python_support_fact_is_role_compatible(fact, framework_role) {
        return result;
    }

    let mut evidence_text = format!(
        "{} {} {}",
        fact.kind.as_protocol_str(),
        fact.origin.engine,
        fact.origin.method
    )
    .to_ascii_lowercase();
    if let Some(target) = &fact.target {
        evidence_text.push(' ');
        evidence_text.push_str(&target.as_str().to_ascii_lowercase());
    }
    for assumption in &fact.assumptions {
        evidence_text.push(' ');
        evidence_text.push_str(&assumption.to_ascii_lowercase());
    }

    if framework_role.contains("express") {
        evidence_text.contains("express")
    } else if framework_role.contains("react") {
        evidence_text.contains("react")
    } else if framework_role.contains("jest_vitest") {
        evidence_text.contains("jest") || evidence_text.contains("vitest")
    } else {
        false
    }
}

fn python_support_fact_is_role_compatible(
    fact: &SemanticFact,
    framework_role: &str,
) -> Option<bool> {
    let target = fact.target.as_ref().map(|target| target.as_str())?;
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
        "framework:pytest.test" => Some(matches!(
            target,
            "pytest.test" | "pytest.mark.parametrize" | "pytest.fixture"
        )),
        "framework:pytest.fixture" => Some(matches!(target, "pytest.fixture")),
        "framework:pydantic.model" => Some(matches!(
            target,
            "pydantic.BaseModel"
                | "pydantic_settings.BaseSettings"
                | "pydantic.field_validator"
                | "pydantic.validator"
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
                | "sqlalchemy.ext.asyncio.AsyncSession.execute"
                | "sqlalchemy.ext.asyncio.AsyncSession.commit"
                | "sqlalchemy.ext.asyncio.AsyncSession.rollback"
        )),
        _ if framework_role.starts_with("framework:fastapi")
            || framework_role.starts_with("framework:pytest")
            || framework_role.starts_with("framework:pydantic")
            || framework_role.starts_with("framework:sqlalchemy") =>
        {
            Some(false)
        }
        _ => None,
    }
}

fn single_framework_role(roles: &BTreeSet<String>) -> Option<&str> {
    if roles.len() == 1 {
        roles.iter().next().map(String::as_str)
    } else {
        None
    }
}

fn family_eligible_kind(kind: &str) -> bool {
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

fn min_family_support(language: &str) -> usize {
    if language == "python" {
        PYTHON_MIN_FAMILY_SUPPORT
    } else {
        DEFAULT_MIN_FAMILY_SUPPORT
    }
}

fn normalized_shape(kind: &str, framework_role: &str) -> String {
    format!("shape:{kind}:{}", stable_token(framework_role))
}

fn family_id(key: &FamilyKey) -> String {
    format!(
        "family:{}:{}:{}",
        stable_token(&key.language),
        stable_token(&key.code_unit_kind),
        stable_token(&key.framework_role)
    )
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
        SemanticFact {
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
        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:express.route_handler"),
                role_fact(&second, "framework:express.route_handler"),
                semantic_support_fact(&first),
                semantic_support_fact(&second),
            ],
        );

        assert_eq!(report.claims.len(), 1);
        let claim = &report.claims[0];
        assert_eq!(claim.classification, "DOMINANT_PATTERN");
        assert_eq!(claim.support, 2);
        assert_eq!(claim.evidence.len(), 2);
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
    fn storage_records_do_not_contain_source_snippets_or_absolute_paths() {
        let first = unit("src/a.ts", "test_case", 0);
        let second = unit("src/b.ts", "test_case", 1);
        let report = build_family_claims(
            &[first.clone(), second.clone()],
            &[
                role_fact(&first, "framework:jest_vitest.test"),
                role_fact(&second, "framework:jest_vitest.test"),
                semantic_support_fact_with_target(&first, "package:vitest"),
                semantic_support_fact_with_target(&second, "package:vitest"),
            ],
        );
        let records = family_storage_records(&report.claims[0]);
        let serialized = format!("{records:?}");

        assert!(!serialized.contains("=>"));
        assert!(!serialized.contains("it("));
        assert!(!serialized.contains("/tmp"));
        assert_eq!(records.members.len(), 2);
        assert_eq!(records.evidence.len(), 2);
    }
}
