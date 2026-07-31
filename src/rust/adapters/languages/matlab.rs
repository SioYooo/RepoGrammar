//! MATLAB discovery-only source and package-metadata path classification.
//!
//! The bounded lane is MATLAB source text plus the `resources/mpackage.json`
//! package definition introduced in R2024b. It does not include Simulink,
//! live scripts, apps, P-code, MEX binaries, projects' unstable XML definition
//! files, toolbox archives, or any executable MATLAB/Octave behavior.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatlabLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatlabPathClassification {
    NotMatlab,
    Source,
    PackageConfig,
}

impl MatlabLanguageAdapter {
    pub fn classify_path(path: &str) -> MatlabPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return MatlabPathClassification::NotMatlab;
        }
        let components = path.split('/').collect::<Vec<_>>();
        if components.ends_with(&["resources", "mpackage.json"]) {
            return MatlabPathClassification::PackageConfig;
        }
        if components
            .last()
            .is_some_and(|file_name| file_name.ends_with(".m"))
        {
            MatlabPathClassification::Source
        } else {
            MatlabPathClassification::NotMatlab
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_matlab_text_and_exact_package_definition_paths() {
        for path in ["main.m", "+pkg/function.m", "@Class/method.m"] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "resources/mpackage.json",
            "packages/demo/resources/mpackage.json",
        ] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::PackageConfig,
                "{path}"
            );
        }
        for path in [
            "mpackage.json",
            "resources/project/Project.xml",
            "toolbox.mltbx",
            "project.prj",
            "live.mlx",
            "model.slx",
            "code.p",
            "binary.mexa64",
            "MAIN.M",
        ] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::NotMatlab,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_or_non_portable_paths() {
        for path in [
            "",
            "/main.m",
            "./main.m",
            "pkg/../main.m",
            "pkg//main.m",
            "pkg\\main.m",
            "C:/main.m",
        ] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::NotMatlab,
                "{path:?}"
            );
        }
    }
}
