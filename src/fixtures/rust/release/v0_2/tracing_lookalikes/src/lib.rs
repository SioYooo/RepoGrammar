//! Nothing here may anchor. A bare `#[instrument]` without the same-file `use`
//! evidence is some other crate's attribute; a locally defined `instrument`
//! macro is not the tracing one; and a function that merely mentions tracing in
//! a comment or body carries no attribute at all.
#[instrument]
pub fn unbound_attribute(id: u64) -> u64 {
    id
}

macro_rules! instrument {
    () => {};
}
instrument!();

/// #[instrument] in a doc comment is not an attribute.
pub fn documented_only(id: u64) -> u64 {
    // tracing::instrument mentioned in a comment only
    id
}

pub fn calls_tracing(id: u64) -> u64 {
    tracing::info!("id");
    id
}
