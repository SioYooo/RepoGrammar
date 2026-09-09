//! Conservative source-free Fortran path classification.
//!
//! The accepted lowercase suffixes are the GNU frontend's non-preprocessed
//! fixed/free-form defaults. Uppercase and `.fpp` inputs are deliberately
//! deferred because their documented meaning includes preprocessing.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FortranLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortranSourceForm {
    Fixed,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FortranPathClassification {
    NotFortran,
    Source(FortranSourceForm),
    Config,
}

impl FortranLanguageAdapter {
    pub fn classify_path(path: &str) -> FortranPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return FortranPathClassification::NotFortran;
        }
        let file_name = path.rsplit('/').next().unwrap_or(path);
        if file_name == "fpm.toml" {
            return FortranPathClassification::Config;
        }
        let Some((_, extension)) = file_name.rsplit_once('.') else {
            return FortranPathClassification::NotFortran;
        };
        match extension {
            "f" | "for" | "ftn" => FortranPathClassification::Source(FortranSourceForm::Fixed),
            "f90" | "f95" | "f03" | "f08" => {
                FortranPathClassification::Source(FortranSourceForm::Free)
            }
            _ => FortranPathClassification::NotFortran,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_documented_non_preprocessed_forms_and_fpm_manifest() {
        for path in ["a.f", "a.for", "a.ftn", "src/a.f"] {
            assert_eq!(
                FortranLanguageAdapter::classify_path(path),
                FortranPathClassification::Source(FortranSourceForm::Fixed),
                "{path}"
            );
        }
        for path in ["a.f90", "a.f95", "a.f03", "a.f08", "src/a.f90"] {
            assert_eq!(
                FortranLanguageAdapter::classify_path(path),
                FortranPathClassification::Source(FortranSourceForm::Free),
                "{path}"
            );
        }
        for path in ["fpm.toml", "nested/fpm.toml"] {
            assert_eq!(
                FortranLanguageAdapter::classify_path(path),
                FortranPathClassification::Config,
                "{path}"
            );
        }
    }

    #[test]
    fn defers_preprocessed_case_variants_and_unproven_suffixes() {
        for path in [
            "a.F",
            "a.FOR",
            "a.FTN",
            "a.F90",
            "a.F95",
            "a.F03",
            "a.F08",
            "a.fpp",
            "a.fi",
            "a.fii",
            "a.f90.bak",
            "FPM.toml",
        ] {
            assert_eq!(
                FortranLanguageAdapter::classify_path(path),
                FortranPathClassification::NotFortran,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths() {
        for path in [
            "",
            "/main.f90",
            "./main.f",
            "src/../main.f90",
            "src\\main.f90",
            "file://main.f90",
        ] {
            assert_eq!(
                FortranLanguageAdapter::classify_path(path),
                FortranPathClassification::NotFortran,
                "{path:?}"
            );
        }
    }
}
