//! Object Pascal discovery with an explicitly Delphi-only configuration lane.
//!
//! `.pas` is intentionally labelled `object-pascal`: path shape alone cannot
//! distinguish Delphi from Free Pascal. Exact `.dpr`/`.dpk` source and `.dproj`
//! configuration are Delphi-specific evidence. Free Pascal/Lazarus `.pp`,
//! `.lpr`, `.lpi`, and `.lpk` remain outside this qualified slice.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPascalLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPascalPathExclusion {
    HistoryDirectory,
    RecoveryDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPascalPathClassification {
    NotObjectPascal,
    Excluded(ObjectPascalPathExclusion),
    Source,
    DelphiConfig,
}

impl ObjectPascalLanguageAdapter {
    pub fn classify_path(path: &str) -> ObjectPascalPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return ObjectPascalPathClassification::NotObjectPascal;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return ObjectPascalPathClassification::NotObjectPascal;
        };
        let classification = if file_name.ends_with(".dproj") {
            ObjectPascalPathClassification::DelphiConfig
        } else if [".pas", ".dpr", ".dpk"]
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
        {
            ObjectPascalPathClassification::Source
        } else {
            ObjectPascalPathClassification::NotObjectPascal
        };
        if classification == ObjectPascalPathClassification::NotObjectPascal {
            return classification;
        }
        for component in path.split('/') {
            match component {
                "__history" => {
                    return ObjectPascalPathClassification::Excluded(
                        ObjectPascalPathExclusion::HistoryDirectory,
                    );
                }
                "__recovery" => {
                    return ObjectPascalPathClassification::Excluded(
                        ObjectPascalPathExclusion::RecoveryDirectory,
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
    fn separates_generic_object_pascal_source_from_delphi_config() {
        for path in ["Unit1.pas", ".pas", "src/Main.dpr", "packages/Tools.dpk"] {
            assert_eq!(
                ObjectPascalLanguageAdapter::classify_path(path),
                ObjectPascalPathClassification::Source,
                "{path}"
            );
        }
        for path in ["App.dproj", "nested/App.dproj", ".dproj"] {
            assert_eq!(
                ObjectPascalLanguageAdapter::classify_path(path),
                ObjectPascalPathClassification::DelphiConfig,
                "{path}"
            );
        }
        for path in [
            "Unit1.PAS",
            "App.DPROJ",
            "Unit1.pas.bak",
            "unit.pp",
            "program.lpr",
            "project.lpi",
            "package.lpk",
        ] {
            assert_eq!(
                ObjectPascalLanguageAdapter::classify_path(path),
                ObjectPascalPathClassification::NotObjectPascal,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths_and_delphi_history_copies_only() {
        for path in [
            "",
            "/Unit1.pas",
            "./Unit1.pas",
            "src/../Unit1.pas",
            "src//Unit1.pas",
            "src\\Unit1.pas",
            "C:/Unit1.pas",
        ] {
            assert_eq!(
                ObjectPascalLanguageAdapter::classify_path(path),
                ObjectPascalPathClassification::NotObjectPascal,
                "{path:?}"
            );
        }
        assert_eq!(
            ObjectPascalLanguageAdapter::classify_path("__history/Unit1.pas"),
            ObjectPascalPathClassification::Excluded(ObjectPascalPathExclusion::HistoryDirectory)
        );
        assert_eq!(
            ObjectPascalLanguageAdapter::classify_path("src/__recovery/App.dproj"),
            ObjectPascalPathClassification::Excluded(ObjectPascalPathExclusion::RecoveryDirectory)
        );
        assert_eq!(
            ObjectPascalLanguageAdapter::classify_path("__history/keep.py"),
            ObjectPascalPathClassification::NotObjectPascal
        );
    }
}
