//! TypeScript and JavaScript language adapter placeholder.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeScriptLanguageAdapter;

impl TypeScriptLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        matches!(extension, "ts" | "tsx" | "js" | "jsx")
    }
}
