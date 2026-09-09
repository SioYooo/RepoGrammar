//! C# language adapter configuration.
//!
//! The path classifier separates `.cs` source from `.csproj` MSBuild project
//! files so discovery can admit SDK-style project inventory without executing
//! MSBuild. Script and view extensions such as `.csx` and `.razor`, and
//! non-normalized paths, stay outside this contract. C# has no
//! language-specific directory exclusion: `obj` is excluded by the shared
//! default exclusion list and `bin` stays admitted by design.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CSharpLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CSharpPathClassification {
    NotCSharp,
    Source,
    Config,
}

impl CSharpLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "cs"
    }

    pub fn classify_path(path: &str) -> CSharpPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return CSharpPathClassification::NotCSharp;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return CSharpPathClassification::NotCSharp;
        };
        if file_name.ends_with(".csproj") {
            CSharpPathClassification::Config
        } else if file_name.ends_with(".cs") {
            CSharpPathClassification::Source
        } else {
            CSharpPathClassification::NotCSharp
        }
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

    #[test]
    fn classifies_exact_cs_source_and_csproj_project_paths() {
        for path in [
            "Program.cs",
            ".cs",
            "src/CatalogController.cs",
            "bin/keep.cs",
        ] {
            assert_eq!(
                CSharpLanguageAdapter::classify_path(path),
                CSharpPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "App.csproj",
            "src/App.csproj",
            ".csproj",
            "nested/tests/App.csproj",
        ] {
            assert_eq!(
                CSharpLanguageAdapter::classify_path(path),
                CSharpPathClassification::Config,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_lookalikes_and_non_normalized_paths() {
        for path in [
            "Program.CS",
            "App.CSPROJ",
            "Program.cs.bak",
            "Script.csx",
            "View.razor",
            "legacy.cc",
        ] {
            assert_eq!(
                CSharpLanguageAdapter::classify_path(path),
                CSharpPathClassification::NotCSharp,
                "{path}"
            );
        }
        for path in [
            "",
            "/Program.cs",
            "./Program.cs",
            "src/../Program.cs",
            "src//Program.cs",
            "src\\Program.cs",
            "C:/Program.cs",
        ] {
            assert_eq!(
                CSharpLanguageAdapter::classify_path(path),
                CSharpPathClassification::NotCSharp,
                "{path:?}"
            );
        }
    }
}
