//! Rust language adapter configuration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustLanguageAdapter;

impl RustLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "rs"
    }
}
