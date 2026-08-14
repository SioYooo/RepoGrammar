//! Three exact `tracing` instrument anchors, enough to reach Rust's minimum
//! family support of three. Two spellings appear on purpose: the bare
//! attribute gated by the same-file `use`, and the fully qualified path.
use tracing::instrument;

#[instrument]
pub fn load_catalog(id: u64) -> u64 {
    id
}

#[instrument(skip(secret))]
pub fn refresh_catalog(id: u64, secret: &str) -> usize {
    let _ = secret;
    id as usize
}

#[tracing::instrument(level = "debug")]
pub fn expire_catalog(id: u64) -> bool {
    id > 0
}
