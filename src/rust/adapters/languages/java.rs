//! Java language adapter configuration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaLanguageAdapter;

impl JavaLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "java"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_only_java_source_extension() {
        assert!(JavaLanguageAdapter::supports_extension("java"));
        assert!(!JavaLanguageAdapter::supports_extension("class"));
        assert!(!JavaLanguageAdapter::supports_extension("kt"));
    }
}
