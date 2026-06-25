//! Persistence port for pattern-family records.

use crate::core::model::ContentHash;
use crate::ports::index_store::GenerationHandle;

pub const FAMILY_EVIDENCE_COVERED_CLAIMS: &[&str] =
    &["canonical", "support", "variation", "exception"];

pub fn family_evidence_covered_claim_is_supported(value: &str) -> bool {
    FAMILY_EVIDENCE_COVERED_CLAIMS.contains(&value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyRecord {
    pub family_id: String,
    pub classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyMemberRecord {
    pub family_id: String,
    pub code_unit_id: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedVariationSlotRecord {
    pub family_id: String,
    pub slot_id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyEvidenceRecord {
    pub evidence_id: String,
    pub family_id: String,
    pub code_unit_id: String,
    pub covered_claims: Vec<String>,
    pub path: String,
    pub content_hash: ContentHash,
    pub start_byte: usize,
    pub end_byte: usize,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFamilies {
    pub generation_id: String,
    pub families: Vec<IndexedFamilyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFamily {
    pub generation_id: String,
    pub family: IndexedFamilyRecord,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Unavailable(String),
    InvalidState(String),
    InvalidRecord(String),
}

pub trait FamilyStore {
    fn record_family(
        &self,
        generation: &GenerationHandle,
        family: &IndexedFamilyRecord,
    ) -> Result<(), StoreError>;

    fn record_family_member(
        &self,
        generation: &GenerationHandle,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError>;

    fn record_variation_slot(
        &self,
        generation: &GenerationHandle,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError>;

    fn record_family_evidence(
        &self,
        generation: &GenerationHandle,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError>;

    fn list_active_families(&self) -> Result<ActiveFamilies, StoreError>;

    fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError>;
}
