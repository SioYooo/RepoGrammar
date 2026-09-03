//! Bounded, non-executing Ruby/Bundler project-configuration inventory.
//!
//! Ruby source and executable Bundler DSL files are never evaluated here. The
//! only dependency-producing input is the direct `DEPENDENCIES` section of an
//! exact `Gemfile.lock`. Every other Ruby configuration is rejected at this
//! boundary and remains source-free inventory in the indexing application.

use super::{config_source_parse_output, sort_inventory_facts};
use crate::core::model::{
    CodeUnit, DependencyDirectness, DependencyEcosystem, DependencyEvidenceLevel, DependencyRecord,
    DependencyScope, DependencyVersion, Evidence, FactCertainty, FactOrigin, Language,
    PackageIdentity, SemanticFact, SemanticFactKind, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument, SourceParseOutput};
use std::collections::{BTreeMap, BTreeSet};

const RUBY_CONFIG_METHOD: &str = "bounded_bundler_lock_inventory_v1";
const MAX_LOCKFILE_BYTES: usize = 1_048_576;
const MAX_LOCKFILE_LINES: usize = 50_000;
const MAX_LOCKFILE_LINE_BYTES: usize = 1_024;
const MAX_DIRECT_DEPENDENCIES: usize = 2_000;
const MAX_GEM_NAME_BYTES: usize = 128;
const MAX_REQUIREMENT_BYTES: usize = 256;
const MAX_VERSION_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockSection {
    Other,
    Dependencies,
}

pub(super) fn parse(document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
    parse_output(document).map(|output| output.report)
}

pub(super) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::RubyConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    if basename != "Gemfile.lock" {
        return Err(ParseError::UnsupportedLanguage);
    }
    let unit = super::project_config_unit(&document, Language::RubyConfig)?;
    let (mut facts, dependencies) = gemfile_lock_inventory(&unit, document.text)?;
    sort_inventory_facts(&mut facts);
    config_source_parse_output(vec![unit], facts, dependencies)
}

fn gemfile_lock_inventory(
    unit: &CodeUnit,
    text: &str,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    if text.len() > MAX_LOCKFILE_BYTES {
        return Ok((
            vec![dependency_unknown(
                unit,
                "ResourceLimit",
                "lockfile_byte_limit",
                "Gemfile.lock exceeds the bounded byte limit",
            )?],
            Vec::new(),
        ));
    }

    let mut facts = Vec::new();
    let mut requirements = BTreeMap::<String, Option<String>>::new();
    let mut admitted_names = BTreeSet::new();
    let mut conflicting_names = BTreeSet::new();
    let mut section = LockSection::Other;
    let mut dependency_sections = 0usize;
    let mut malformed = false;
    let mut unsupported_source = false;
    let mut truncated = false;
    let mut line_count = 0usize;

    for raw_line in text.lines() {
        line_count += 1;
        if line_count > MAX_LOCKFILE_LINES || raw_line.len() > MAX_LOCKFILE_LINE_BYTES {
            truncated = true;
            break;
        }
        if !raw_line.is_ascii()
            || raw_line
                .bytes()
                .any(|byte| byte.is_ascii_control() && byte != b'\t')
        {
            malformed = true;
            continue;
        }
        if raw_line.is_empty() {
            continue;
        }
        if !raw_line.starts_with([' ', '\t']) {
            section = match raw_line {
                "DEPENDENCIES" => {
                    dependency_sections += 1;
                    LockSection::Dependencies
                }
                "GIT" | "PATH" | "PLUGIN SOURCE" => {
                    unsupported_source = true;
                    LockSection::Other
                }
                "GEM" | "PLATFORMS" | "RUBY VERSION" | "BUNDLED WITH" | "CHECKSUMS" => {
                    LockSection::Other
                }
                // Bundler deliberately ignores top-level sections it does not
                // understand. Do the same outside `DEPENDENCIES` so a future
                // metadata section cannot erase otherwise exact direct rows.
                _ => LockSection::Other,
            };
            continue;
        }
        if section != LockSection::Dependencies {
            continue;
        }
        let Some(entry) = raw_line.strip_prefix("  ") else {
            malformed = true;
            continue;
        };
        if entry.is_empty() || entry.starts_with([' ', '\t']) || entry.ends_with([' ', '\t']) {
            malformed = true;
            continue;
        }
        let Some(parsed) = parse_direct_dependency(entry) else {
            malformed = true;
            continue;
        };
        if parsed.source_specific {
            unsupported_source = true;
            continue;
        }
        if !admitted_names.contains(parsed.name) {
            if admitted_names.len() == MAX_DIRECT_DEPENDENCIES {
                truncated = true;
                continue;
            }
            admitted_names.insert(parsed.name.to_string());
        }
        if conflicting_names.contains(parsed.name) {
            continue;
        }
        if let Some(existing) = requirements.get(parsed.name) {
            if existing.as_deref() != parsed.requirement {
                requirements.remove(parsed.name);
                conflicting_names.insert(parsed.name.to_string());
            }
            continue;
        }
        requirements.insert(
            parsed.name.to_string(),
            parsed.requirement.map(str::to_string),
        );
    }

    if dependency_sections != 1 {
        malformed = true;
    }
    if malformed {
        requirements.clear();
    }
    let mut dependencies = Vec::with_capacity(requirements.len());
    for (name, requirement) in requirements {
        dependencies.push(manifest_dependency(unit, &name, requirement.as_deref())?);
    }
    if malformed {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_or_malformed_lockfile",
            "Gemfile.lock direct dependency inventory is malformed or incomplete",
        )?);
    }
    if unsupported_source {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_source_section",
            "PATH, GIT, PLUGIN, or source-specific direct dependencies are not RubyGems identities",
        )?);
    }
    if !conflicting_names.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_direct_requirements",
            "direct dependency names with conflicting requirements were omitted",
        )?);
    }
    if truncated {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "lockfile_resource_limit",
            "Gemfile.lock dependency inventory exceeded a bounded resource limit",
        )?);
    }
    Ok((facts, dependencies))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectDependency<'a> {
    name: &'a str,
    requirement: Option<&'a str>,
    source_specific: bool,
}

fn parse_direct_dependency(entry: &str) -> Option<DirectDependency<'_>> {
    let (entry, source_specific) = match entry.strip_suffix('!') {
        Some(entry) if !entry.is_empty() => (entry, true),
        Some(_) => return None,
        None => (entry, false),
    };
    let (name, requirement) = if let Some(open) = entry.find(" (") {
        if !entry.ends_with(')') || entry[open + 2..entry.len() - 1].contains(['(', ')']) {
            return None;
        }
        let name = &entry[..open];
        let requirement = &entry[open + 2..entry.len() - 1];
        (name, Some(requirement))
    } else {
        (entry, None)
    };
    if !is_gem_name(name) || requirement.is_some_and(|value| !is_requirement(value)) {
        return None;
    }
    Some(DirectDependency {
        name,
        requirement,
        source_specific,
    })
}

fn is_gem_name(value: &str) -> bool {
    // Match RubyGems validity, not its stricter lowercase naming advice:
    // names must contain an ASCII letter, may use either case plus `._-`, and
    // may not begin with punctuation.
    !value.is_empty()
        && value.len() <= MAX_GEM_NAME_BYTES
        && value.bytes().any(|byte| byte.is_ascii_alphabetic())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn is_requirement(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_REQUIREMENT_BYTES || !value.is_ascii() {
        return false;
    }
    value.split(',').all(|term| {
        let term = term.trim();
        let (operator, version) = ["~>", ">=", "<=", "!=", "=", ">", "<"]
            .into_iter()
            .find_map(|operator| {
                term.strip_prefix(operator)
                    .map(|rest| (operator, rest.trim()))
            })
            .unwrap_or(("=", term));
        !operator.is_empty() && is_version(version)
    })
}

fn is_version(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_VERSION_BYTES || !value.is_ascii() {
        return false;
    }
    let (release, prerelease) = value
        .split_once('-')
        .map_or((value, None), |(release, suffix)| (release, Some(suffix)));
    let mut release_parts = release.split('.');
    let Some(first) = release_parts.next() else {
        return false;
    };
    if first.is_empty() || !first.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    if !release_parts
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
    {
        return false;
    }
    prerelease.is_none_or(|suffix| {
        !suffix.is_empty()
            && suffix.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
    })
}

fn manifest_dependency(
    unit: &CodeUnit,
    name: &str,
    requirement: Option<&str>,
) -> Result<DependencyRecord, ParseError> {
    DependencyRecord::new(
        PackageIdentity::new(DependencyEcosystem::RubyGems, name).map_err(ParseError::Internal)?,
        requirement
            .map(DependencyVersion::new)
            .transpose()
            .map_err(ParseError::Internal)?,
        None,
        DependencyScope::Unknown,
        false,
        DependencyDirectness::Direct,
        DependencyEvidenceLevel::ManifestDeclared,
        Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded Gemfile.lock direct dependency declaration",
        )
        .map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)
}

fn dependency_unknown(
    unit: &CodeUnit,
    reason: &str,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: "repogrammar-ruby-config".to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: RUBY_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=ruby_dependency_inventory".to_string(),
            format!("ruby_dependency_unknown_kind={kind}"),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::RubyConfig,
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn parse_lock(text: &str) -> SourceParseOutput {
        parse_output(document("Gemfile.lock", text)).expect("parse Gemfile.lock")
    }

    fn unknown_pairs(report: &ParseReport) -> BTreeSet<(String, String)> {
        report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| {
                (
                    fact.target
                        .as_ref()
                        .map(SymbolId::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    fact.assumptions
                        .iter()
                        .find_map(|value| value.strip_prefix("affected_claim="))
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn inventories_bounded_direct_rubygems_dependencies() {
        let output = parse_lock(
            "GEM\r\n  remote: https://rubygems.org/\r\n  specs:\r\n    rack (3.1.0)\r\n    rails (8.0.0)\r\n\r\nFUTURE METADATA\r\n  opaque: ignored\r\n\r\nDEPENDENCIES\r\n  Rack_Compat. (>= 3, < 4)\r\n  rails (~> 8.0.pre-1)\r\n  rake\r\n\r\nBUNDLED WITH\r\n   2.6.0\r\n",
        );
        assert!(output.report.semantic_facts.is_empty());
        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.package.ecosystem.as_str(),
                    dependency.package.name.as_str(),
                    dependency
                        .requirement
                        .as_ref()
                        .map(DependencyVersion::as_str),
                    dependency.scope,
                    dependency.directness,
                    dependency.optional,
                    dependency.evidence_level,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "rubygems",
                    "Rack_Compat.",
                    Some(">= 3, < 4"),
                    DependencyScope::Unknown,
                    DependencyDirectness::Direct,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "rubygems",
                    "rails",
                    Some("~> 8.0.pre-1"),
                    DependencyScope::Unknown,
                    DependencyDirectness::Direct,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "rubygems",
                    "rake",
                    None,
                    DependencyScope::Unknown,
                    DependencyDirectness::Direct,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
            ]
        );
    }

    #[test]
    fn parser_rejects_executable_and_unqualified_ruby_config_inputs() {
        for path in [
            "Gemfile",
            "demo.gemspec",
            "gems.rb",
            "gems.locked",
            ".ruby-version",
        ] {
            assert_eq!(
                parse_output(document(path, "raise 'must never execute'\n")),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
    }

    #[test]
    fn malformed_conflicting_and_unsupported_sources_abstain_conservatively() {
        let output = parse_lock(
            "GIT\n  remote: https://user:secret@example.invalid/repo.git\n\nDEPENDENCIES\n  rack (>= 3)\n  rack (< 2)\n  safe-gem (~> 1.0)\n  private-gem!\n  bad/name (= 1.0)\n  broken (wat)\n",
        );
        assert!(output.dependencies.is_empty());
        let unknowns = unknown_pairs(&output.report);
        for reason in [
            "MissingProjectConfig",
            "ConflictingFacts",
            "InsufficientSupport",
        ] {
            assert!(
                unknowns.contains(&(reason.to_string(), "ruby_dependency_inventory".to_string()))
            );
        }
        let debug = format!("{:?}", output.report);
        assert!(!debug.contains("user:secret"));
        assert!(!debug.contains("private-gem"));
    }

    #[test]
    fn every_non_registry_source_section_is_typed_unknown() {
        for section in ["GIT", "PATH", "PLUGIN SOURCE"] {
            let output = parse_lock(&format!(
                "{section}\n  specs:\n    private (1.0.0)\nDEPENDENCIES\n  rack (~> 3.1)\n"
            ));
            assert_eq!(output.dependencies.len(), 1, "{section}");
            assert!(unknown_pairs(&output.report).contains(&(
                "InsufficientSupport".to_string(),
                "ruby_dependency_inventory".to_string(),
            )));
        }
    }

    #[test]
    fn identical_direct_dependencies_deduplicate_without_conflict() {
        let output =
            parse_lock("DEPENDENCIES\n  rack (>= 3, < 4)\n  rack (>= 3, < 4)\n  rake\n  rake\n");
        assert!(output.report.semantic_facts.is_empty());
        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| dependency.package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["rack", "rake"]
        );
    }

    #[test]
    fn dependency_count_limit_is_inclusive_and_plus_one_is_typed() {
        let mut text = String::from("DEPENDENCIES\n");
        for index in 0..MAX_DIRECT_DEPENDENCIES {
            text.push_str(&format!("  gem-{index}\n"));
        }
        let exact = parse_lock(&text);
        assert_eq!(exact.dependencies.len(), MAX_DIRECT_DEPENDENCIES);
        assert!(!unknown_pairs(&exact.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));

        text.push_str(&format!("  gem-{}\n", MAX_DIRECT_DEPENDENCIES));
        let plus_one = parse_lock(&text);
        assert_eq!(plus_one.dependencies.len(), MAX_DIRECT_DEPENDENCIES);
        assert!(unknown_pairs(&plus_one.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
    }

    #[test]
    fn ruby_gems_identity_and_requirement_grammar_matches_the_bounded_official_subset() {
        for name in ["rack", "Rack_Compat.", "1gem", "net-http"] {
            assert!(is_gem_name(name), "valid RubyGems name: {name}");
        }
        for name in ["", ".rack", "123", "bad/name", "räck"] {
            assert!(!is_gem_name(name), "invalid RubyGems name: {name}");
        }

        for requirement in [
            "1.2",
            "= 1.2",
            "~> 8.0.pre-1",
            ">= 1.0, < 2.0",
            "!= 1.0-rc-1",
        ] {
            assert!(
                is_requirement(requirement),
                "valid requirement: {requirement}"
            );
        }
        for requirement in ["", "^ 1.0", ">= v1", "= 1_0", ">= 1.", ">= 1-"] {
            assert!(
                !is_requirement(requirement),
                "invalid requirement: {requirement}"
            );
        }

        let exact_name = format!("a{}", "1".repeat(MAX_GEM_NAME_BYTES - 1));
        let oversized_name = format!("{exact_name}1");
        assert!(is_gem_name(&exact_name));
        assert!(!is_gem_name(&oversized_name));

        let exact_version = "1".repeat(MAX_VERSION_BYTES);
        let oversized_version = "1".repeat(MAX_VERSION_BYTES + 1);
        assert!(is_version(&exact_version));
        assert!(!is_version(&oversized_version));

        let exact_requirement = format!(">= {}, < {}", "1".repeat(123), "2".repeat(126));
        let oversized_requirement = format!(">= {}, < {}", "1".repeat(123), "2".repeat(127));
        assert_eq!(exact_requirement.len(), MAX_REQUIREMENT_BYTES);
        assert_eq!(oversized_requirement.len(), MAX_REQUIREMENT_BYTES + 1);
        assert!(is_requirement(&exact_requirement));
        assert!(!is_requirement(&oversized_requirement));
    }

    #[test]
    fn lockfile_byte_line_count_and_line_width_limits_are_inclusive() {
        fn padded_lockfile(total_bytes: usize) -> String {
            let mut text = String::from("DEPENDENCIES\n  rack\n");
            assert!(total_bytes >= text.len());
            let mut remaining = total_bytes - text.len();
            while remaining > MAX_LOCKFILE_LINE_BYTES + 1 {
                text.push_str(&"X".repeat(MAX_LOCKFILE_LINE_BYTES));
                text.push('\n');
                remaining -= MAX_LOCKFILE_LINE_BYTES + 1;
            }
            if remaining == 1 {
                text.push('\n');
            } else if remaining > 1 {
                text.push_str(&"X".repeat(remaining - 1));
                text.push('\n');
            }
            assert_eq!(text.len(), total_bytes);
            text
        }

        let exact_bytes = parse_lock(&padded_lockfile(MAX_LOCKFILE_BYTES));
        assert_eq!(exact_bytes.dependencies.len(), 1);
        assert!(!unknown_pairs(&exact_bytes.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
        let oversized_bytes = parse_lock(&padded_lockfile(MAX_LOCKFILE_BYTES + 1));
        assert!(oversized_bytes.dependencies.is_empty());
        assert!(unknown_pairs(&oversized_bytes.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));

        let mut exact_lines = String::from("DEPENDENCIES\n  rack");
        for _ in 2..MAX_LOCKFILE_LINES {
            exact_lines.push_str("\nX");
        }
        let exact_lines_output = parse_lock(&exact_lines);
        assert_eq!(exact_lines_output.dependencies.len(), 1);
        assert!(!unknown_pairs(&exact_lines_output.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
        exact_lines.push_str("\nX");
        let oversized_lines = parse_lock(&exact_lines);
        assert_eq!(oversized_lines.dependencies.len(), 1);
        assert!(unknown_pairs(&oversized_lines.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));

        let exact_width = parse_lock(&format!(
            "DEPENDENCIES\n  rack\n{}",
            "X".repeat(MAX_LOCKFILE_LINE_BYTES)
        ));
        assert_eq!(exact_width.dependencies.len(), 1);
        assert!(!unknown_pairs(&exact_width.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
        let oversized_width = parse_lock(&format!(
            "DEPENDENCIES\n  rack\n{}",
            "X".repeat(MAX_LOCKFILE_LINE_BYTES + 1)
        ));
        assert_eq!(oversized_width.dependencies.len(), 1);
        assert!(unknown_pairs(&oversized_width.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
    }
}
