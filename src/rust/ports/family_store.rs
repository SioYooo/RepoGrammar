//! Persistence port for pattern-family records.

use crate::core::model::{ContentHash, FamilyConstraintProfile, FamilyPrevalence};
use crate::ports::index_store::{
    GenerationHandle, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedIrEdgeRecord, IndexedIrNodeRecord, IndexedSemanticFactRecord,
};

pub const FAMILY_EVIDENCE_COVERED_CLAIMS: &[&str] =
    &["canonical", "support", "contrast", "variation", "exception"];

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
    /// Backs the deterministic term-based family retrieval that the production
    /// fuzzy lookup path invokes (see `application::query::run_term_retrieval`).
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

/// One family's derived [`FamilyConstraintProfile`], keyed by family id and
/// scoped to a generation by the store. The profile is a source-backed
/// implementation specification; the record only carries RepoGrammar-owned typed
/// values, never repository source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFamilyConstraintProfileRecord {
    pub family_id: String,
    pub profile: FamilyConstraintProfile,
}

/// Persistence port for family constraint profiles.
///
/// This is a separate port from [`FamilyStore`] so the profile round-trip can be
/// added without widening the `FamilyStore` contract that read-only consumers
/// already implement. The concrete storage adapter implements both ports; the
/// hydration read lives beside `show_family` on the same store.
pub trait FamilyConstraintProfileStore {
    /// Persist one family's constraint profile for the building generation. The
    /// referenced family must already exist in the same generation.
    fn record_family_constraint_profile(
        &self,
        generation: &GenerationHandle,
        record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), StoreError>;

    /// Hydrate one family's constraint profile from the active generation, or
    /// `None` when the family has no stored profile.
    fn show_family_constraint_profile(
        &self,
        family_id: &str,
    ) -> Result<Option<FamilyConstraintProfile>, StoreError>;
}

/// A store that records both family records and their derived constraint
/// profiles in the same generation.
///
/// This supertrait lets the production indexing pipeline thread a single trait
/// object that can persist family evidence *and* the co-derived constraint
/// profile. The concrete SQLite adapter implements both parent traits, and the
/// blanket impl below makes every such store usable as a
/// `dyn FamilyStoreWithProfiles`. Methods of either parent trait are callable on
/// the combined object because both are supertraits, and a
/// `dyn FamilyStoreWithProfiles` still satisfies a `FamilyStore` or
/// `FamilyConstraintProfileStore` bound.
pub trait FamilyStoreWithProfiles: FamilyStore + FamilyConstraintProfileStore {}

impl<T: FamilyStore + FamilyConstraintProfileStore + ?Sized> FamilyStoreWithProfiles for T {}

/// Bounded, session-local counters an adapter exposes about a single generation
/// build, for tests and benchmarks. `transactions` counts committed batches (the
/// capacity commits, phase checkpoints, and the final seal), `rows_written`
/// counts logical rows persisted, and `checkpoints` counts phase-boundary
/// commits. All three are live counters incremented as the session runs, not
/// values asserted by construction. Connection-open counts are measured at the
/// adapter, not here, since one session opens exactly one connection.
/// RepoGrammar-owned values only; carries no repository source text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteSessionStats {
    pub transactions: usize,
    pub rows_written: usize,
    pub checkpoints: usize,
}

/// Single writer for one `building` generation.
///
/// A build opens exactly one session (see [`GenerationWriteStore`]), routes every
/// record through it under bounded-batch transactions on one connection, and
/// closes it with [`finish`](GenerationWriteSession::finish) (commit and seal) or
/// [`abandon`](GenerationWriteSession::abandon) (roll back the open batch and, if
/// a batch already committed, stamp the terminal `failed` status). The
/// `building -> validated -> active` state machine stays on
/// [`IndexStore::validate_generation`](crate::ports::index_store::IndexStore) and
/// [`activate_generation`](crate::ports::index_store::IndexStore), which run after
/// the session is finished, so they always observe fully committed data.
///
/// Every method mirrors the field-level and referential validation of the
/// historical per-record store methods, so the session is invisible to build
/// results. It is object-safe (`&mut self`, concrete record types) so build
/// pipelines thread `&mut dyn GenerationWriteSession` uniformly.
pub trait GenerationWriteSession {
    /// The `building` generation this session writes into.
    fn generation(&self) -> &GenerationHandle;

    fn record_indexed_file(&mut self, file: &IndexedFileRecord) -> Result<(), IndexStoreError>;

    fn remove_indexed_file(&mut self, path: &str) -> Result<(), IndexStoreError>;

    fn record_code_unit(&mut self, unit: &IndexedCodeUnitRecord) -> Result<(), IndexStoreError>;

    fn record_ir_node(&mut self, node: &IndexedIrNodeRecord) -> Result<(), IndexStoreError>;

    fn record_ir_edge(&mut self, edge: &IndexedIrEdgeRecord) -> Result<(), IndexStoreError>;

    fn record_semantic_fact(
        &mut self,
        fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError>;

    fn record_family(&mut self, family: &IndexedFamilyRecord) -> Result<(), StoreError>;

    fn record_family_member(
        &mut self,
        member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError>;

    fn record_variation_slot(
        &mut self,
        slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError>;

    fn record_family_evidence(
        &mut self,
        evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError>;

    fn record_family_constraint_profile(
        &mut self,
        record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), StoreError>;

    /// Commit any open batch and checkpoint the write-ahead log at a pipeline
    /// phase boundary, bounding WAL growth to roughly one phase rather than the
    /// whole build. The production pipelines call it after the file/unit/IR write
    /// phase, after the semantic-fact phase, and after the family phase.
    /// Correctness never depends on it — a committed batch and the still-open
    /// batch are both visible to this session's own referential reads — so a
    /// best-effort WAL checkpoint failure is not fatal. Errors on a sealed
    /// session.
    fn checkpoint(&mut self) -> Result<(), IndexStoreError>;

    /// Commit everything and seal the session, then run best-effort post-commit
    /// maintenance whose failure cannot fail the committed build. The generation
    /// stays `building` and is validated and activated through the [`IndexStore`]
    /// state machine. A record call, `checkpoint`, or a second `finish` after the
    /// session is sealed (by `finish` or `abandon`) is a typed error, so a build
    /// that abandoned on one path can never silently `finish` on another.
    fn finish(&mut self) -> Result<(), IndexStoreError>;

    /// Roll back the open batch and, if a batch already committed, mark the
    /// generation `failed` (best effort). Dropping an unsealed session performs
    /// the same reclamation. Idempotent once sealed.
    fn abandon(&mut self) -> Result<(), IndexStoreError>;

    /// Bounded build counters for tests and benchmarks.
    fn stats(&self) -> WriteSessionStats;
}

/// Opens the single [`GenerationWriteSession`] for a `building` generation that
/// was created by
/// [`IndexStore::prepare_next_generation`](crate::ports::index_store::IndexStore).
pub trait GenerationWriteStore {
    fn open_generation_write_session<'a>(
        &'a self,
        generation: &GenerationHandle,
    ) -> Result<Box<dyn GenerationWriteSession + 'a>, IndexStoreError>;
}
