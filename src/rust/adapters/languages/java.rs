//! Java language adapter configuration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaLanguageAdapter;

impl JavaLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "java"
    }

    pub fn is_project_config_path(path: &str) -> bool {
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|component| !component.is_empty() && !matches!(component, "." | ".."))
            && path.rsplit('/').next() == Some("pom.xml")
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

    #[test]
    fn recognizes_only_exact_root_or_nested_maven_poms() {
        assert!(JavaLanguageAdapter::is_project_config_path("pom.xml"));
        assert!(JavaLanguageAdapter::is_project_config_path(
            "modules/api/pom.xml"
        ));
        for path in ["pom.xml.bak", "POM.xml", "build.gradle", "../pom.xml"] {
            assert!(!JavaLanguageAdapter::is_project_config_path(path), "{path}");
        }
    }
}
