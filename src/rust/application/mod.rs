//! Application use-case boundaries.

pub mod autosync;
pub mod conformance;
pub mod family;
pub mod indexing;
pub mod install;
pub(crate) mod process_liveness;
pub mod product_installation;
pub mod product_uninstall;
pub mod progress;
pub(crate) mod proof_lattice;
pub mod providers;
pub mod query;
pub mod query_terms;
pub mod recovery;
pub mod repository;
pub mod setup;
pub mod storage;
pub mod telemetry;
