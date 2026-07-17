//! Persistence port for pattern-family records.

use crate::core::model::{ContentHash, FamilyPrevalence};
use crate::ports::index_store::GenerationHandle;

pub const FAMILY_EVIDENCE_COVERED_CLAIMS: &[&str] =
    &["canonical", "support", "variation", "exception"];

pub fn family_evidence_covered_claim_is_supported(value: &str) -> bool {
    FAMILY_EVIDENCE_COVERED_CLAIMS.contains(&value)
}

// `FamilyPrevalence` carries a floating-point coverage ratio, so records that
// embed it derive `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFamilyRecord {
    pub family_id: String,
    pub classification: String,
    pub prevalence: FamilyPrevalence,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFamilies {
    pub generation_id: String,
    pub families: Vec<IndexedFamilyRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFamilySummaryRecord {
    pub family_id: String,
    pub classification: String,
    pub support: usize,
    pub prevalence: FamilyPrevalence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFamilySummaries {
    pub generation_id: String,
    pub families: Vec<IndexedFamilySummaryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyCandidateRecord {
    pub family_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFamilyCandidates {
    pub generation_id: String,
    pub candidates: Vec<IndexedFamilyCandidateRecord>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFamily {
    pub generation_id: String,
    pub family: IndexedFamilyRecord,
    pub members: Vec<IndexedFamilyMemberRecord>,
    pub variation_slots: Vec<IndexedVariationSlotRecord>,
    pub evidence: Vec<IndexedFamilyEvidenceRecord>,
}

/// One row of the bounded active-generation family-evidence projection. Carries
/// only the columns list-level freshness verification needs: which family the
/// evidence belongs to, the evidence path, and the expected content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyEvidenceProjectionRecord {
    pub family_id: String,
    pub path: String,
    pub content_hash: ContentHash,
}

/// Bounded projection of every active-generation family-evidence row. A single
/// store read backs list-level freshness so verification stays bounded by the
/// number of distinct evidence paths rather than an unbounded per-family loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveFamilyEvidenceProjection {
    pub generation_id: String,
    pub rows: Vec<IndexedFamilyEvidenceProjectionRecord>,
}

/// Maximum distinct repo-relative path components retained per family in the
/// searchable-metadata projection. Bounds the projection so a family with many
/// evidence files cannot contribute an unbounded token set.
pub const FAMILY_SEARCH_PATH_COMPONENT_CAP: usize = 16;

/// One row of the bounded active-generation family searchable-metadata
/// projection. Carries only source-free structural metadata a deterministic,
/// dependency-free retrieval layer can rank on: identity, language, code-unit
/// kind, framework role, prevalence classification, support count, prevalence,
/// and a bounded set of repo-relative evidence-path components. It never carries
/// source text, comments, snippets, raw queries, or absolute paths.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFamilySearchSummaryRecord {
    pub family_id: String,
    pub language: String,
    pub code_unit_kind: String,
    pub framework_role: String,
    pub classification: String,
    pub support: usize,
    pub prevalence: FamilyPrevalence,
    /// Distinct repo-relative path segments (ancestor directory components and
    /// basenames) drawn from this family's evidence paths, deterministically
    /// ordered and capped at [`FAMILY_SEARCH_PATH_COMPONENT_CAP`].
    pub evidence_path_components: Vec<String>,
}

/// Bounded projection of every active-generation family's searchable metadata. A
/// single store read backs deterministic term-based retrieval so discovery stays
/// bounded by the number of active families rather than an unbounded per-family
/// hydration loop.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFamilySearchSummaries {
    pub generation_id: String,
    pub families: Vec<IndexedFamilySearchSummaryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Unavailable(String),
    InvalidState(String),
    InvalidRecord(String),
    /// The stored index predates the current storage schema version. Recovery is
    /// a full rebuild via `repogrammar resync`; the message carries that
    /// guidance sourced from the recovery classifier vocabulary.
    SchemaVersionOutdated(String),
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

    fn list_active_family_summaries(&self) -> Result<ActiveFamilySummaries, StoreError>;

    /// Returns one bounded projection of every active-generation family-evidence
    /// row as `(family_id, path, content_hash)`. Callers verifying list-level
    /// freshness use this single read to derive per-family verdicts, keeping the
    /// number of source reads bounded by the distinct evidence paths.
    fn list_active_family_evidence_projection(
        &self,
    ) -> Result<ActiveFamilyEvidenceProjection, StoreError>;

    /// Returns one bounded, source-free projection of every active-generation
    /// family's searchable metadata: identity, language, code-unit kind,
    /// framework role, prevalence classification, support count, prevalence, and
    /// bounded repo-relative evidence-path components. Deterministic (`family_id`
    /// byte order) and generation-consistent, mirroring the evidence projection.
    /// Backs deterministic term-based family retrieval; it is not yet routed into
    /// the production fuzzy lookup path.
    fn list_active_family_search_summaries(
        &self,
    ) -> Result<ActiveFamilySearchSummaries, StoreError>;

    fn find_active_families_by_member(
        &self,
        code_unit_id: &str,
    ) -> Result<ActiveFamilyCandidates, StoreError>;

    fn find_active_families_by_role(
        &self,
        role: &str,
        limit: usize,
    ) -> Result<ActiveFamilyCandidates, StoreError>;

    fn find_active_families_by_evidence_path(
        &self,
        path: &str,
        limit: usize,
    ) -> Result<ActiveFamilyCandidates, StoreError>;

    fn show_family(&self, family_id: &str) -> Result<Option<ActiveFamily>, StoreError>;
}
