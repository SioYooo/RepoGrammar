//! SQLite adapter boundary.
//!
//! The bootstrap does not introduce `rusqlite` or migrations yet.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteIndexLocation {
    pub path: String,
}
