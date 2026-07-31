//! Bounded, non-executing Ruby/Bundler project-configuration inventory.
//!
//! Ruby source and executable Bundler DSL files are never evaluated here. The
//! only dependency-producing input is the direct `DEPENDENCIES` section of an
//! exact `Gemfile.lock`. Everything else either stays empty structural config
//! or becomes a typed dependency-inventory `UNKNOWN`.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyEcosystem, DependencyEvidenceLevel,
    DependencyRecord, DependencyScope, DependencySnapshot, DependencyVersion, Evidence,
    FactCertainty, FactOrigin, Language, PackageIdentity, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
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
    let unit = project_config_unit(&document)?;
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    let (mut facts, dependencies) = match basename {
        "Gemfile.lock" => gemfile_lock_inventory(&unit, document.text)?,
        "Gemfile" | "gems.rb" => (
            vec![dependency_unknown(
                &unit,
                UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                "executable_bundler_dsl",
                "executable Bundler DSL is not statically interpreted",
            )?],
            Vec::new(),
        ),
        name if name.ends_with(".gemspec") => (
            vec![dependency_unknown(
                &unit,
                UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                "executable_gemspec_dsl",
                "executable gemspec DSL is not statically interpreted",
            )?],
            Vec::new(),
        ),
        "gems.locked" => (
            vec![dependency_unknown(
                &unit,
                UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                "unqualified_bundler_lock_variant",
                "gems.locked dependency syntax is outside the qualified subset",
            )?],
            Vec::new(),
        ),
        ".ruby-version" => (Vec::new(), Vec::new()),
        _ => return Err(ParseError::UnsupportedLanguage),
    };
    facts.sort_by(|left, right| {
        (
            left.target.as_ref().map(SymbolId::as_str),
            left.assumptions.as_slice(),
        )
            .cmp(&(
                right.target.as_ref().map(SymbolId::as_str),
                right.assumptions.as_slice(),
            ))
    });
    let dependencies = DependencySnapshot::new(dependencies, Vec::new())
        .map_err(ParseError::Internal)?
        .dependencies;
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies,
    })
}

fn project_config_unit(document: &SourceDocument<'_>) -> Result<CodeUnit, ParseError> {
    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    Ok(CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::RubyConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    })
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
                "GIT" | "PATH" | "PLUGIN" => {
                    unsupported_source = true;
                    LockSection::Other
                }
                "GEM" | "PLATFORMS" | "RUBY VERSION" | "BUNDLED WITH" | "CHECKSUMS" => {
                    LockSection::Other
                }
                _ => {
                    malformed = true;
                    LockSection::Other
                }
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
    !value.is_empty()
        && value.len() <= MAX_GEM_NAME_BYTES
        && value.bytes().any(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn is_requirement(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_REQUIREMENT_BYTES || !value.is_ascii() {
        return false;
    }
    value.split(',').all(|term| {
        let term = term.trim();
        let Some((operator, version)) = ["~>", ">=", "<=", "!=", "=", ">", "<"]
            .into_iter()
            .find_map(|operator| {
                term.strip_prefix(operator)
                    .map(|rest| (operator, rest.trim()))
            })
        else {
            return false;
        };
        !operator.is_empty() && is_version(version)
    })
}

fn is_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VERSION_BYTES
        && value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
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
        true,
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
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    rack (3.1.0)\n    rails (8.0.0)\n\nDEPENDENCIES\n  rack (>= 3, < 4)\n  rails (~> 8.0)\n  rake\n\nBUNDLED WITH\n   2.6.0\n",
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
                    dependency.direct,
                    dependency.optional,
                    dependency.evidence_level,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "rubygems",
                    "rack",
                    Some(">= 3, < 4"),
                    DependencyScope::Unknown,
                    true,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "rubygems",
                    "rails",
                    Some("~> 8.0"),
                    DependencyScope::Unknown,
                    true,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "rubygems",
                    "rake",
                    None,
                    DependencyScope::Unknown,
                    true,
                    false,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
            ]
        );
    }

    #[test]
    fn executable_dsl_and_unqualified_lock_variant_are_typed_unknown_without_execution() {
        for (path, source, kind) in [
            (
                "Gemfile",
                "raise 'must never execute'\ngem 'rack'\n",
                "executable_bundler_dsl",
            ),
            (
                "demo.gemspec",
                "raise 'must never execute'\n",
                "executable_gemspec_dsl",
            ),
            (
                "gems.rb",
                "raise 'must never execute'\n",
                "executable_bundler_dsl",
            ),
            (
                "gems.locked",
                "DEPENDENCIES\n  rack\n",
                "unqualified_bundler_lock_variant",
            ),
        ] {
            let output = parse_output(document(path, source)).expect("typed UNKNOWN");
            assert!(output.dependencies.is_empty(), "{path}");
            assert!(output.report.semantic_facts.iter().any(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some("InsufficientSupport")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == &format!("ruby_dependency_unknown_kind={kind}")
                    })
            }));
            assert!(!format!("{:?}", output.report).contains("must never execute"));
        }
    }

    #[test]
    fn malformed_conflicting_and_unsupported_sources_abstain_conservatively() {
        let output = parse_lock(
            "GIT\n  remote: https://user:secret@example.invalid/repo.git\n\nDEPENDENCIES\n  rack (>= 3)\n  rack (< 2)\n  safe-gem (~> 1.0)\n  private-gem!\n  UpperCase (= 1.0)\n  broken (wat)\n",
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
        for section in ["GIT", "PATH", "PLUGIN"] {
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
    fn resource_limit_is_typed_and_deterministically_truncates() {
        let mut text = String::from("DEPENDENCIES\n");
        for index in 0..=MAX_DIRECT_DEPENDENCIES {
            text.push_str(&format!("  gem-{index}\n"));
        }
        let output = parse_lock(&text);
        assert_eq!(output.dependencies.len(), MAX_DIRECT_DEPENDENCIES);
        assert!(unknown_pairs(&output.report).contains(&(
            "ResourceLimit".to_string(),
            "ruby_dependency_inventory".to_string(),
        )));
    }
}
