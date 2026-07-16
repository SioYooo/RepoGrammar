//! C# language adapter configuration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CSharpLanguageAdapter;

impl CSharpLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "cs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_only_csharp_source_extension() {
        assert!(CSharpLanguageAdapter::supports_extension("cs"));
        assert!(!CSharpLanguageAdapter::supports_extension("csx"));
        assert!(!CSharpLanguageAdapter::supports_extension("csproj"));
        assert!(!CSharpLanguageAdapter::supports_extension("razor"));
    }
}
