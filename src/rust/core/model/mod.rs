//! Domain types used across RepoGrammar.

pub mod code_unit;
pub mod evidence;
pub mod family;
pub mod ir;
pub mod measurement;
pub mod provenance;
pub mod provider;
pub mod semantic;

pub use code_unit::{CodeUnit, CodeUnitId, CodeUnitKind, Language, SourceRange};
pub use evidence::Evidence;
pub use family::{
    assess_family_prevalence, coverage_ratio, FamilyId, FamilyPrevalence,
    FamilyPrevalenceAssessment, FamilyPrevalenceClass, PatternClassification, PrevalenceInputs,
    SemanticObligation, TypedUnknown, UnknownClass, UnknownReason, UnknownReasonCode,
};
pub(crate) use family::{ClaimImpact, ResolutionClass};
pub use ir::{IrEdge, IrEdgeLabel, IrNode, IrNodeId, IrNodeKind};
pub use measurement::{EstimatedPotentialTokenSavings, MeasurementKind, MetricReport};
pub use provenance::{ContentHash, Provenance, RepositoryRevision};
pub use provider::{provider_availability, ProviderAvailability, SemanticProviderSlot};
pub use semantic::{FactCertainty, FactOrigin, SemanticFact, SemanticFactKind, SymbolId};
