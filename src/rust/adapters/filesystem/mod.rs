//! Filesystem and Git-backed adapter boundaries.

mod bounded_read;
pub mod change_fingerprint;
pub mod discovery;
pub(crate) mod git;
mod resource_limits;
pub mod source_store;
