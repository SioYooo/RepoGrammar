//! MATLAB discovery-only source and package-metadata path classification.
//!
//! The bounded lane is MATLAB source text plus the `resources/mpackage.json`
//! package definition introduced in R2024b. It does not include Simulink,
//! live scripts, apps, P-code, MEX binaries, projects' unstable XML definition
//! files, toolbox archives, or any executable MATLAB/Octave behavior.
//!
//! Selection is deterministic. Exact lowercase `.m` files and exact
//! root/nested `resources/mpackage.json` are candidates, and a candidate below
//! a MathWorks code-generation output component is excluded as generated
//! output rather than discovered. MATLAB Coder writes generated code under a
//! `codegen` folder, and Simulink simulation/code-generation targets are built
//! under `slprj` and `sccprj`; the standard MATLAB ignore template lists
//! exactly these three as the code-generation folders
//! (<https://www.toptal.com/developers/gitignore/api/matlab>). Component
//! matching is exact and case-sensitive, matching the lane's exact-lowercase
//! `.m` classification, and the exclusion applies only to MATLAB candidates:
//! other languages below those components stay eligible.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatlabLanguageAdapter;

/// MathWorks code-generation output folder names. Generated build output
/// under these components is never checked-in MATLAB source.
const CODE_GENERATION_OUTPUT_DIRS: &[&str] = &["codegen", "slprj", "sccprj"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatlabPathExclusion {
    CodeGenerationOutputDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatlabPathClassification {
    NotMatlab,
    Excluded(MatlabPathExclusion),
    Source,
    PackageConfig,
}

impl MatlabLanguageAdapter {
    pub fn classify_path(path: &str) -> MatlabPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return MatlabPathClassification::NotMatlab;
        }
        let components = path.split('/').collect::<Vec<_>>();
        let classification = if components.ends_with(&["resources", "mpackage.json"]) {
            MatlabPathClassification::PackageConfig
        } else if components
            .last()
            .is_some_and(|file_name| file_name.ends_with(".m"))
        {
            MatlabPathClassification::Source
        } else {
            return MatlabPathClassification::NotMatlab;
        };
        if components
            .iter()
            .any(|component| CODE_GENERATION_OUTPUT_DIRS.contains(component))
        {
            return MatlabPathClassification::Excluded(
                MatlabPathExclusion::CodeGenerationOutputDirectory,
            );
        }
        classification
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

    #[test]
    fn excludes_code_generation_output_candidates_only() {
        for path in [
            "codegen/lib/demo/build.m",
            "codegen/foo.m",
            "slprj/_simcommon/cache.m",
            "sccprj/target/model.m",
            "nested/codegen/out.m",
            "codegen/demo/resources/mpackage.json",
        ] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::Excluded(
                    MatlabPathExclusion::CodeGenerationOutputDirectory
                ),
                "{path}"
            );
        }
    }

    #[test]
    fn ordinary_and_generated_named_candidates_stay_selected() {
        for path in [
            "main.m",
            "+pkg/function.m",
            "@Class/method.m",
            "codegen.m",
            "Codegen/x.m",
        ] {
            assert_eq!(
                MatlabLanguageAdapter::classify_path(path),
                MatlabPathClassification::Source,
                "{path}"
            );
        }
        assert_eq!(
            MatlabLanguageAdapter::classify_path("pkg/resources/mpackage.json"),
            MatlabPathClassification::PackageConfig
        );
        // The exclusion is MATLAB-only: non-candidates below a generated
        // output component stay other languages' business.
        assert_eq!(
            MatlabLanguageAdapter::classify_path("codegen/main.py"),
            MatlabPathClassification::NotMatlab
        );
        assert_eq!(
            MatlabLanguageAdapter::classify_path("codegen/MAIN.M"),
            MatlabPathClassification::NotMatlab
        );
    }
}
