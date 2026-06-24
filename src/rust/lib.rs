//! RepoGrammar is a local repository analysis engine.
//!
//! The bootstrap crate defines stable module boundaries for a future
//! pattern-family mining engine. It intentionally avoids implementing the full
//! mining pipeline until the domain model, evidence policy, and adapter
//! contracts are validated.

pub mod adapters;
pub mod application;
pub mod config;
pub mod core;
pub mod error;
pub mod interfaces;
pub mod ports;

#[cfg(test)]
pub(crate) mod integration_tests;

#[cfg(test)]
pub(crate) mod test_support;
