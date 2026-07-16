//! Swift discovery-only path classification.
//!
//! This module classifies normalized repository-relative path shape only. It
//! does not decode or parse Swift source, evaluate SwiftPM manifests, inspect
//! lockfiles, or select a toolchain or version-specific manifest.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwiftLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwiftPathExclusion {
    BuildDirectory,
    SwiftPmDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwiftPathClassification {
    NotSwift,
    Excluded(SwiftPathExclusion),
    Source,
    Config,
}

impl SwiftLanguageAdapter {
    pub fn classify_path(path: &str) -> SwiftPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return SwiftPathClassification::NotSwift;
        }

        let Some(file_name) = path.rsplit('/').next() else {
            return SwiftPathClassification::NotSwift;
        };
        let classification = if is_config_basename(file_name) {
            SwiftPathClassification::Config
        } else if file_name.ends_with(".swift") {
            SwiftPathClassification::Source
        } else {
            SwiftPathClassification::NotSwift
        };
        if classification == SwiftPathClassification::NotSwift {
            return classification;
        }

        for component in path.split('/') {
            match component {
                ".build" => {
                    return SwiftPathClassification::Excluded(SwiftPathExclusion::BuildDirectory);
                }
                ".swiftpm" => {
                    return SwiftPathClassification::Excluded(SwiftPathExclusion::SwiftPmDirectory);
                }
                _ => {}
            }
        }
        classification
    }
}

fn is_config_basename(file_name: &str) -> bool {
    matches!(
        file_name,
        "Package.swift" | "Package.resolved" | ".swift-version"
    ) || is_version_specific_manifest_basename(file_name)
}

fn is_version_specific_manifest_basename(file_name: &str) -> bool {
    let Some(version) = file_name
        .strip_prefix("Package@swift-")
        .and_then(|value| value.strip_suffix(".swift"))
    else {
        return false;
    };

    let mut component_count = 0_usize;
    for component in version.split('.') {
        component_count += 1;
        if component_count > 3
            || component.is_empty()
            || !component.bytes().all(|byte| byte.is_ascii_digit())
        {
            return false;
        }
    }
    (1..=3).contains(&component_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_sources_and_configs_with_config_precedence() {
        for path in [
            ".swift",
            "main.swift",
            "Sources/App/main.swift",
            "package.swift",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "Package.swift",
            "Package.resolved",
            ".swift-version",
            "nested/Package.swift",
            "nested/Package.resolved",
            "nested/.swift-version",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Config,
                "{path}"
            );
        }
    }

    #[test]
    fn version_specific_manifest_grammar_is_complete_and_ascii() {
        for path in [
            "Package@swift-6.swift",
            "Package@swift-6.3.swift",
            "Package@swift-6.3.3.swift",
            "Package@swift-0.swift",
            "Package@swift-06.003.000.swift",
            "nested/Package@swift-6.3.swift",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Config,
                "{path}"
            );
        }

        for path in [
            "Package@swift-.swift",
            "Package@swift-6..swift",
            "Package@swift-6.3..swift",
            "Package@swift-6.3.3.1.swift",
            "Package@swift-6.x.swift",
            "Package@swift-6.3-beta.swift",
            "Package@swift-+6.swift",
            "Package@swift-６.swift",
            "Package@Swift-6.swift",
            "package@swift-6.swift",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Source,
                "invalid manifest grammar remains ordinary Swift source: {path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths_before_candidate_classification() {
        for path in [
            "",
            "   ",
            "/main.swift",
            "C:/main.swift",
            "C:\\main.swift",
            "./main.swift",
            "Sources/../main.swift",
            "Sources//main.swift",
            "Sources\\main.swift",
            "file://main.swift",
            "Sources/\u{0000}main.swift",
            "/Package.swift",
            "nested/../Package.resolved",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::NotSwift,
                "{path:?}"
            );
        }
    }

    #[test]
    fn exact_case_and_non_candidates_remain_out_of_config_inventory() {
        for path in [
            "main.SWIFT",
            "main.swift.bak",
            "Package.SWIFT",
            "Package.Resolved",
            ".Swift-version",
            ".swift-version.bak",
            "Package@swift-6.SWIFT",
            "Package@swift-6.swift.bak",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::NotSwift,
                "{path}"
            );
        }
        for path in ["PACKAGE.swift", "Package@swift.swift"] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Source,
                "non-config Swift basename remains source: {path}"
            );
        }
    }

    #[test]
    fn excludes_only_swift_candidates_below_exact_tool_components() {
        for path in [
            ".build/main.swift",
            "nested/.build/Package.swift",
            "nested/.build/Package@swift-6.3.swift",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Excluded(SwiftPathExclusion::BuildDirectory),
                "{path}"
            );
        }
        for path in [
            ".swiftpm/cache.swift",
            "nested/.swiftpm/Package.resolved",
            "nested/.swiftpm/.swift-version",
        ] {
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                SwiftPathClassification::Excluded(SwiftPathExclusion::SwiftPmDirectory),
                "{path}"
            );
        }
        for path in [
            ".build-cache/main.swift",
            ".swiftpm-cache/main.swift",
            ".build/other.ts",
            ".swiftpm/other.py",
        ] {
            let expected = if path.ends_with(".swift") {
                SwiftPathClassification::Source
            } else {
                SwiftPathClassification::NotSwift
            };
            assert_eq!(
                SwiftLanguageAdapter::classify_path(path),
                expected,
                "{path}"
            );
        }
    }
}
