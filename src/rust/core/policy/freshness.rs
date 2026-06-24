//! Freshness policy connects evidence to repository revision and content hashes.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Stale { reason: String },
    Unknown { reason: String },
}
