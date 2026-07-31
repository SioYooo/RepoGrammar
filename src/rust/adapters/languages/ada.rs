//! Conservative Ada source and project-metadata path classification.
//!
//! Only GNAT's default `.ads` specification and `.adb` body suffixes are
//! source-safe without evaluating a GPR naming policy. GPR, Alire manifest,
//! and Alire lock paths remain configuration inventory.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaSourceForm {
    Specification,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaConfigKind {
    GprProject,
    AlireManifest,
    AlireLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaPathClassification {
    NotAda,
    Source(AdaSourceForm),
    Config(AdaConfigKind),
}

impl AdaLanguageAdapter {
    pub fn classify_path(path: &str) -> AdaPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return AdaPathClassification::NotAda;
        }
        let file_name = path.rsplit('/').next().unwrap_or(path);
        match file_name {
            "alire.toml" => AdaPathClassification::Config(AdaConfigKind::AlireManifest),
            "alire.lock" => AdaPathClassification::Config(AdaConfigKind::AlireLock),
            name if name.ends_with(".gpr") => {
                AdaPathClassification::Config(AdaConfigKind::GprProject)
            }
            name if name.ends_with(".ads") => {
                AdaPathClassification::Source(AdaSourceForm::Specification)
            }
            name if name.ends_with(".adb") => AdaPathClassification::Source(AdaSourceForm::Body),
            _ => AdaPathClassification::NotAda,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_only_default_ada_sources_and_exact_configs() {
        for (path, expected) in [
            (
                "src/example.ads",
                AdaPathClassification::Source(AdaSourceForm::Specification),
            ),
            (
                "src/example.adb",
                AdaPathClassification::Source(AdaSourceForm::Body),
            ),
            (
                "project.gpr",
                AdaPathClassification::Config(AdaConfigKind::GprProject),
            ),
            (
                "alire.toml",
                AdaPathClassification::Config(AdaConfigKind::AlireManifest),
            ),
            (
                "alire/alire.lock",
                AdaPathClassification::Config(AdaConfigKind::AlireLock),
            ),
        ] {
            assert_eq!(AdaLanguageAdapter::classify_path(path), expected, "{path}");
        }
    }

    #[test]
    fn defers_alternative_names_case_variants_and_suffix_lookalikes() {
        for path in [
            "src/example.ada",
            "src/example.ADS",
            "src/example.ADB",
            "src/example.ads.bak",
            "project.GPR",
            "Alire.toml",
            "alire.lock.bak",
        ] {
            assert_eq!(
                AdaLanguageAdapter::classify_path(path),
                AdaPathClassification::NotAda,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths() {
        for path in [
            "",
            "/main.adb",
            "./main.ads",
            "src/../main.adb",
            "src\\main.adb",
            "file://main.adb",
        ] {
            assert_eq!(
                AdaLanguageAdapter::classify_path(path),
                AdaPathClassification::NotAda,
                "{path:?}"
            );
        }
    }
}
