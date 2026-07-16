//! Ruby discovery-only path classification.
//!
//! This module classifies normalized repository-relative path shape only. It
//! does not read or evaluate Ruby source, Bundler configuration, gemspecs, or
//! runtime configuration.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RubyLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubyPathExclusion {
    BundleDirectory,
    RubyLspDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubyPathClassification {
    NotRuby,
    Excluded(RubyPathExclusion),
    Source,
    Config,
}

impl RubyLanguageAdapter {
    pub fn classify_path(path: &str) -> RubyPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return RubyPathClassification::NotRuby;
        }

        let Some(file_name) = path.rsplit('/').next() else {
            return RubyPathClassification::NotRuby;
        };
        let classification = if is_config_basename(file_name) {
            RubyPathClassification::Config
        } else if file_name.ends_with(".rb") {
            RubyPathClassification::Source
        } else {
            RubyPathClassification::NotRuby
        };
        if classification == RubyPathClassification::NotRuby {
            return classification;
        }

        for component in path.split('/') {
            match component {
                ".bundle" => {
                    return RubyPathClassification::Excluded(RubyPathExclusion::BundleDirectory);
                }
                ".ruby-lsp" => {
                    return RubyPathClassification::Excluded(RubyPathExclusion::RubyLspDirectory);
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
        "Gemfile" | "Gemfile.lock" | "gems.rb" | "gems.locked" | ".ruby-version"
    ) || file_name.ends_with(".gemspec")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_sources_and_configs_with_config_precedence() {
        for path in [
            ".rb",
            "main.rb",
            "lib/example.rb",
            "tmp/cache.rb",
            "pkg/build.rb",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::Source,
                "{path}"
            );
        }
        for path in [
            "Gemfile",
            "Gemfile.lock",
            "gems.rb",
            "gems.locked",
            ".ruby-version",
            ".gemspec",
            "example.gemspec",
            "gems/demo.gemspec",
            "nested/Gemfile",
            "nested/Gemfile.lock",
            "nested/gems.rb",
            "nested/gems.locked",
            "nested/.ruby-version",
            "nested/.gemspec",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::Config,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths_before_candidate_classification() {
        for path in [
            "",
            "/main.rb",
            "C:/main.rb",
            "./main.rb",
            "lib/../main.rb",
            "lib//main.rb",
            "lib\\main.rb",
            "file://main.rb",
            "lib/\u{0000}main.rb",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::NotRuby,
                "{path:?}"
            );
        }
    }

    #[test]
    fn exact_case_and_deferred_candidates_remain_out_of_inventory() {
        for path in [
            "main.RB",
            "main.rb.bak",
            "GEMFILE",
            "gemfile",
            "Gemfile.bak",
            "gems.RB",
            "example.GEMSPEC",
            "example.gemspec.bak",
            "Rakefile",
            "tasks/build.rake",
            "config.ru",
            "view.erb",
            "example.gem",
            "custom.dependencies",
            "custom.lock",
            "Gemfile.custom",
            "CustomGemfile",
            "Alternate.lock",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::NotRuby,
                "{path}"
            );
        }
    }

    #[test]
    fn excludes_only_ruby_candidates_below_exact_ruby_tool_components() {
        for path in [
            ".bundle/cache.rb",
            "nested/.bundle/Gemfile",
            "nested/.bundle/example.gemspec",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::Excluded(RubyPathExclusion::BundleDirectory),
                "{path}"
            );
        }
        for path in [
            ".ruby-lsp/index.rb",
            "nested/.ruby-lsp/gems.rb",
            "nested/.ruby-lsp/example.gemspec",
        ] {
            assert_eq!(
                RubyLanguageAdapter::classify_path(path),
                RubyPathClassification::Excluded(RubyPathExclusion::RubyLspDirectory),
                "{path}"
            );
        }
        for path in [
            ".bundle-not/cache.rb",
            ".ruby-lsp-cache/index.rb",
            ".bundle/other.ts",
            ".ruby-lsp/other.py",
        ] {
            let expected = if path.ends_with(".rb") {
                RubyPathClassification::Source
            } else {
                RubyPathClassification::NotRuby
            };
            assert_eq!(RubyLanguageAdapter::classify_path(path), expected, "{path}");
        }
    }
}
