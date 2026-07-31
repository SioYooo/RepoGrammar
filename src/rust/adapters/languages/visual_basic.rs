//! Visual Basic .NET discovery-only path classification.
//!
//! This adapter accepts only `.vb` source and `.vbproj` MSBuild project files.
//! Classic Visual Basic/VB6 extensions such as `.vbp`, `.frm`, `.bas`, and
//! `.cls` are deliberately outside this token contract.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualBasicLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualBasicPathExclusion {
    BinDirectory,
    VisualStudioDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualBasicPathClassification {
    NotVisualBasic,
    Excluded(VisualBasicPathExclusion),
    Source,
    Config,
}

impl VisualBasicLanguageAdapter {
    pub fn classify_path(path: &str) -> VisualBasicPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return VisualBasicPathClassification::NotVisualBasic;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return VisualBasicPathClassification::NotVisualBasic;
        };
        let classification = if file_name.ends_with(".vbproj") {
            VisualBasicPathClassification::Config
        } else if file_name.ends_with(".vb") {
            VisualBasicPathClassification::Source
        } else {
            VisualBasicPathClassification::NotVisualBasic
        };
        if classification == VisualBasicPathClassification::NotVisualBasic {
            return classification;
        }
        for component in path.split('/') {
            match component {
                "bin" => {
                    return VisualBasicPathClassification::Excluded(
                        VisualBasicPathExclusion::BinDirectory,
                    );
                }
                ".vs" => {
                    return VisualBasicPathClassification::Excluded(
                        VisualBasicPathExclusion::VisualStudioDirectory,
                    );
                }
                _ => {}
            }
        }
        classification
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_exact_vbnet_source_and_project_extensions() {
        for path in ["Program.vb", ".vb", "src/App.vb"] {
            assert_eq!(
                VisualBasicLanguageAdapter::classify_path(path),
                VisualBasicPathClassification::Source,
                "{path}"
            );
        }
        for path in ["App.vbproj", "src/App.vbproj", ".vbproj"] {
            assert_eq!(
                VisualBasicLanguageAdapter::classify_path(path),
                VisualBasicPathClassification::Config,
                "{path}"
            );
        }
        for path in [
            "Program.VB",
            "App.VBPROJ",
            "Program.vb.bak",
            "legacy.vbp",
            "Form1.frm",
            "Module1.bas",
            "Class1.cls",
        ] {
            assert_eq!(
                VisualBasicLanguageAdapter::classify_path(path),
                VisualBasicPathClassification::NotVisualBasic,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths_and_only_excludes_vbnet_candidates() {
        for path in [
            "",
            "/Program.vb",
            "./Program.vb",
            "src/../Program.vb",
            "src//Program.vb",
            "src\\Program.vb",
            "C:/Program.vb",
        ] {
            assert_eq!(
                VisualBasicLanguageAdapter::classify_path(path),
                VisualBasicPathClassification::NotVisualBasic,
                "{path:?}"
            );
        }
        assert_eq!(
            VisualBasicLanguageAdapter::classify_path("bin/Generated.vb"),
            VisualBasicPathClassification::Excluded(VisualBasicPathExclusion::BinDirectory)
        );
        assert_eq!(
            VisualBasicLanguageAdapter::classify_path(".vs/cache/App.vbproj"),
            VisualBasicPathClassification::Excluded(
                VisualBasicPathExclusion::VisualStudioDirectory
            )
        );
        assert_eq!(
            VisualBasicLanguageAdapter::classify_path("bin/keep.py"),
            VisualBasicPathClassification::NotVisualBasic
        );
    }
}
