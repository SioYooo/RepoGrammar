//! Python language adapter placeholder.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonLanguageAdapter;

impl PythonLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        matches!(extension, "py")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_only_python_source_extension() {
        assert!(PythonLanguageAdapter::supports_extension("py"));
        assert!(!PythonLanguageAdapter::supports_extension("pyc"));
        assert!(!PythonLanguageAdapter::supports_extension("pyi"));
        assert!(!PythonLanguageAdapter::supports_extension("ts"));
    }
}
