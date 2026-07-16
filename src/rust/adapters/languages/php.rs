//! PHP discovery-only path classification.
//!
//! This module classifies normalized repository-relative path shape only. It
//! does not decode or parse PHP source, Composer data, PHPUnit XML, autoloaders,
//! scripts, plugins, or runtime configuration.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhpLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhpPathExclusion {
    ComposerDirectory,
    PhpUnitCacheDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhpPathClassification {
    NotPhp,
    Excluded(PhpPathExclusion),
    Source,
    Config,
}

impl PhpLanguageAdapter {
    pub fn classify_path(path: &str) -> PhpPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return PhpPathClassification::NotPhp;
        }

        let Some(file_name) = path.rsplit('/').next() else {
            return PhpPathClassification::NotPhp;
        };
        let classification = if is_config_basename(file_name) {
            PhpPathClassification::Config
        } else if file_name.ends_with(".php") {
            PhpPathClassification::Source
        } else {
            PhpPathClassification::NotPhp
        };
        if classification == PhpPathClassification::NotPhp {
            return classification;
        }

        for component in path.split('/') {
            match component {
                ".composer" => {
                    return PhpPathClassification::Excluded(PhpPathExclusion::ComposerDirectory);
                }
                ".phpunit.cache" => {
                    return PhpPathClassification::Excluded(
                        PhpPathExclusion::PhpUnitCacheDirectory,
                    );
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
        "composer.json" | "composer.lock" | "phpunit.xml" | "phpunit.xml.dist"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_sources_and_configs_with_config_precedence() {
        for path in [".php", "main.php", "src/example.php", "nested/.php"] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "composer.json",
            "composer.lock",
            "phpunit.xml",
            "phpunit.xml.dist",
            "nested/composer.json",
            "nested/composer.lock",
            "nested/phpunit.xml",
            "nested/phpunit.xml.dist",
        ] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::Config,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths_before_candidate_classification() {
        for path in [
            "",
            "   ",
            "/main.php",
            "C:/main.php",
            "C:\\main.php",
            "./main.php",
            "src/../main.php",
            "src//main.php",
            "src\\main.php",
            "file://main.php",
            "src/\u{0000}main.php",
            "/composer.json",
            "nested/../phpunit.xml",
        ] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::NotPhp,
                "{path:?}"
            );
        }
    }

    #[test]
    fn exact_case_and_deferred_candidates_remain_out_of_inventory() {
        for path in [
            "main.PHP",
            "main.php.bak",
            "view.phtml",
            "bootstrap.inc",
            "test.phpt",
            "config.php.dist",
            "artisan",
            "composer.phar",
            "auth.json",
            "Composer.json",
            "composer.JSON",
            "composer.lock.bak",
            "phpunit.XML",
            "phpunit.xml.bak",
            "phpunit.xml.dist.bak",
        ] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::NotPhp,
                "{path}"
            );
        }
    }

    #[test]
    fn excludes_only_php_candidates_below_exact_tool_components() {
        for path in [
            ".composer/cache.php",
            "nested/.composer/composer.json",
            "nested/.composer/phpunit.xml",
        ] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::Excluded(PhpPathExclusion::ComposerDirectory),
                "{path}"
            );
        }
        for path in [
            ".phpunit.cache/result.php",
            "nested/.phpunit.cache/composer.lock",
            "nested/.phpunit.cache/phpunit.xml.dist",
        ] {
            assert_eq!(
                PhpLanguageAdapter::classify_path(path),
                PhpPathClassification::Excluded(PhpPathExclusion::PhpUnitCacheDirectory),
                "{path}"
            );
        }
        for path in [
            ".composer-cache/cache.php",
            ".phpunit.cache-data/result.php",
            ".composer/other.ts",
            ".phpunit.cache/other.py",
        ] {
            let expected = if path.ends_with(".php") {
                PhpPathClassification::Source
            } else {
                PhpPathClassification::NotPhp
            };
            assert_eq!(PhpLanguageAdapter::classify_path(path), expected, "{path}");
        }
    }
}
