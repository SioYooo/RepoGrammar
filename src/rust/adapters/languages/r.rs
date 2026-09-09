//! Non-executing R source and project-metadata path classification.
//!
//! Only exact `.R`/`.r` source files and exact `DESCRIPTION`, `NAMESPACE`, and
//! `renv.lock` basenames are admitted. No R profile, project file, package
//! archive, generated library, or executable package code is classified as
//! project metadata.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RPathExclusion {
    RenvManagedDirectory,
    RProjectStateDirectory,
    PackratManagedDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RPathClassification {
    NotR,
    Excluded(RPathExclusion),
    Source,
    Config,
}

impl RLanguageAdapter {
    pub fn classify_path(path: &str) -> RPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return RPathClassification::NotR;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return RPathClassification::NotR;
        };
        let classification = if matches!(file_name, "DESCRIPTION" | "NAMESPACE" | "renv.lock") {
            RPathClassification::Config
        } else if file_name.ends_with(".R") || file_name.ends_with(".r") {
            RPathClassification::Source
        } else {
            RPathClassification::NotR
        };
        if classification == RPathClassification::NotR {
            return classification;
        }

        let components = path.split('/').collect::<Vec<_>>();
        if components.contains(&".Rproj.user") {
            return RPathClassification::Excluded(RPathExclusion::RProjectStateDirectory);
        }
        if components
            .windows(2)
            .any(|pair| pair[0] == "renv" && matches!(pair[1], "library" | "cache" | "staging"))
        {
            return RPathClassification::Excluded(RPathExclusion::RenvManagedDirectory);
        }
        if components
            .windows(2)
            .any(|pair| pair[0] == "packrat" && matches!(pair[1], "lib" | "lib-ext" | "lib-R"))
        {
            return RPathClassification::Excluded(RPathExclusion::PackratManagedDirectory);
        }
        classification
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_sources_and_supported_metadata() {
        for path in [
            "main.R",
            ".R",
            "R/model.R",
            "nested/main.R",
            "main.r",
            ".r",
            "R/model.r",
            "nested/main.r",
        ] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "DESCRIPTION",
            "NAMESPACE",
            "renv.lock",
            "nested/DESCRIPTION",
            "nested/NAMESPACE",
            "nested/renv.lock",
        ] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::Config
            );
        }
    }

    #[test]
    fn keeps_executable_and_remote_selectors_out_of_metadata_inventory() {
        for path in [
            ".Rprofile",
            ".Renviron",
            "project.Rproj",
            "DESCRIPTION.in",
            "NAMESPACE.in",
            "renv.lock.json",
            "main.Rmd",
            "main.rmd",
            "main.Rhistory",
            "main.Rdata",
            "main.rda",
        ] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::NotR,
                "{path}"
            );
        }
    }

    #[test]
    fn excludes_managed_library_and_ide_state_candidates_only() {
        for (path, expected) in [
            (
                "renv/library/R-4.4/pkg/R/code.R",
                RPathExclusion::RenvManagedDirectory,
            ),
            (
                "renv/cache/pkg/DESCRIPTION",
                RPathExclusion::RenvManagedDirectory,
            ),
            (
                "renv/staging/pkg/NAMESPACE",
                RPathExclusion::RenvManagedDirectory,
            ),
            (
                ".Rproj.user/session/main.R",
                RPathExclusion::RProjectStateDirectory,
            ),
            (
                "packrat/lib/pkg/R/code.R",
                RPathExclusion::PackratManagedDirectory,
            ),
            (
                "nested/renv/library/pkg/R/code.R",
                RPathExclusion::RenvManagedDirectory,
            ),
            (
                "nested/.Rproj.user/session/main.R",
                RPathExclusion::RProjectStateDirectory,
            ),
            (
                "renv/library/R-4.4/pkg/R/code.r",
                RPathExclusion::RenvManagedDirectory,
            ),
            (
                ".Rproj.user/session/main.r",
                RPathExclusion::RProjectStateDirectory,
            ),
            (
                "packrat/lib/pkg/R/code.r",
                RPathExclusion::PackratManagedDirectory,
            ),
        ] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::Excluded(expected),
                "{path}"
            );
        }
        for path in ["renv/activate.R", "renv/activate.r"] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::Source,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_case_variants_and_non_normalized_paths() {
        for path in [
            "description",
            "namespace",
            "RENV.LOCK",
            "",
            "/main.R",
            "./main.R",
            "R/../main.R",
            "R//main.R",
            "R\\main.R",
            "file://main.R",
            "/main.r",
            "./main.r",
            "R/../main.r",
        ] {
            assert_eq!(
                RLanguageAdapter::classify_path(path),
                RPathClassification::NotR,
                "{path:?}"
            );
        }
    }
}
