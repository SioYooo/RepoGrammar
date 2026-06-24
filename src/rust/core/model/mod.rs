//! Domain types used across RepoGrammar.

pub mod code_unit;
pub mod evidence;
pub mod family;
pub mod ir;
pub mod measurement;
pub mod provenance;
pub mod semantic;

pub use code_unit::{CodeUnit, CodeUnitId, CodeUnitKind, Language, SourceRange};
pub use evidence::Evidence;
pub use family::{FamilyId, PatternClassification, UnknownReason};
pub use ir::{IrEdge, IrNode, IrNodeId};
pub use measurement::{MeasurementKind, MetricReport};
pub use provenance::{ContentHash, Provenance, RepositoryRevision};
pub use semantic::{FactCertainty, FactOrigin, SemanticFact, SemanticFactKind, SymbolId};
