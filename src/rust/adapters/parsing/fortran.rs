//! Bounded, non-executing Fortran Package Manager manifest inventory.
//!
//! Only direct string requirements in exact root `[dependencies]` and
//! `[dev-dependencies]` tables are admitted. Dotted namespace, git, path,
//! target-specific, and preprocessor-bearing shapes remain typed UNKNOWN.

use super::{config_source_parse_output, sort_inventory_facts};
use crate::core::model::{
    CodeUnit, DependencyDirectness, DependencyEcosystem, DependencyEvidenceLevel, DependencyRecord,
    DependencyScope, DependencyVersion, Evidence, FactCertainty, FactOrigin, Language,
    PackageIdentity, SemanticFact, SemanticFactKind, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod free_form;
pub mod testdrive;

const FORTRAN_CONFIG_ENGINE: &str = "repogrammar-fortran-project-config";
const FORTRAN_CONFIG_METHOD: &str = "bounded_fpm_dependency_inventory_v1";
const MAX_MANIFEST_BYTES: usize = 1_048_576;
const MAX_MANIFEST_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 4_096;
const MAX_DEPENDENCIES: usize = 2_000;
const MAX_PACKAGE_NAME_BYTES: usize = 128;
const MAX_CONSTRAINT_BYTES: usize = 256;

#[derive(Debug, Default)]
pub struct FortranProjectConfigParser;

impl SourceParser for FortranProjectConfigParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::FortranConfig
        || document.path.rsplit('/').next().unwrap_or(document.path) != "fpm.toml"
    {
        return Err(ParseError::UnsupportedLanguage);
    }
    let unit = super::project_config_unit(&document, Language::FortranConfig)?;
    let (mut facts, dependencies) = fpm_manifest_inventory(document.text, &unit)?;
    sort_inventory_facts(&mut facts);
    config_source_parse_output(vec![unit], facts, dependencies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestSection {
    Other,
    Runtime,
    Development,
}

fn fpm_manifest_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let mut facts = vec![config_fact(unit)?];
    if text.len() > MAX_MANIFEST_BYTES {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "manifest_byte_limit",
            "fpm manifest exceeds the bounded byte limit",
        )?);
        return Ok((facts, Vec::new()));
    }
    let mut section = ManifestSection::Other;
    let mut requirements = BTreeMap::<(String, DependencyScope), String>::new();
    let mut conflicts = BTreeSet::<(String, DependencyScope)>::new();
    let mut unsupported = false;
    let mut malformed = false;
    let mut resource_limit = false;

    for (index, raw_line) in text.split_inclusive('\n').enumerate() {
        let without_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);
        if index >= MAX_MANIFEST_LINES || line.len() > MAX_LINE_BYTES {
            resource_limit = true;
            break;
        }
        if !line.is_ascii()
            || line
                .bytes()
                .any(|byte| byte.is_ascii_control() && byte != b'\t')
        {
            malformed = true;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            let header = trimmed
                .split_once('#')
                .map_or(trimmed, |(header, _)| header.trim_end());
            section = match header {
                "[dependencies]" => ManifestSection::Runtime,
                "[dev-dependencies]" => ManifestSection::Development,
                _ => {
                    if header.starts_with("[dependencies.")
                        || header.starts_with("[dev-dependencies.")
                        || header.contains(".dependencies]")
                    {
                        unsupported = true;
                    }
                    ManifestSection::Other
                }
            };
            continue;
        }
        let scope = match section {
            ManifestSection::Runtime => DependencyScope::Runtime,
            ManifestSection::Development => DependencyScope::Development,
            ManifestSection::Other => continue,
        };
        let Some((name, constraint)) = parse_string_assignment(trimmed) else {
            unsupported = true;
            continue;
        };
        if !is_fpm_package_name(name) || !is_constraint(constraint) {
            unsupported = true;
            continue;
        }
        let key = (name.to_string(), scope);
        if conflicts.contains(&key) {
            continue;
        }
        if let Some(existing) = requirements.get(&key) {
            if existing != constraint {
                requirements.remove(&key);
                conflicts.insert(key);
            }
            continue;
        }
        if requirements.len() == MAX_DEPENDENCIES {
            resource_limit = true;
            continue;
        }
        requirements.insert(key, constraint.to_string());
    }

    if malformed || resource_limit {
        requirements.clear();
    }
    let dependencies = requirements
        .into_iter()
        .map(|((name, scope), constraint)| manifest_dependency(unit, &name, &constraint, scope))
        .collect::<Result<Vec<_>, _>>()?;
    if unsupported {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_dependency_shape",
            "dotted, inline-table, target-specific, or non-string fpm dependencies were omitted",
        )?);
    }
    if malformed {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "malformed_manifest_text",
            "fpm dependency inventory contains malformed non-ASCII or control text",
        )?);
    }
    if !conflicts.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_direct_dependencies",
            "fpm dependency names with conflicting requirements in one scope were omitted",
        )?);
    }
    if resource_limit {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "manifest_resource_limit",
            "fpm dependency inventory exceeded a bounded resource limit",
        )?);
    }
    Ok((facts, dependencies))
}

fn parse_string_assignment(line: &str) -> Option<(&str, &str)> {
    let (name, raw_value) = line.split_once('=')?;
    let name = name.trim();
    if name.contains('.') || name.contains(['"', '\'']) {
        return None;
    }
    let raw_value = raw_value.trim_start();
    let value = raw_value.strip_prefix('"')?;
    let closing = value.find('"')?;
    let constraint = &value[..closing];
    if constraint.contains(['\\', '"']) {
        return None;
    }
    let tail = value[closing + 1..].trim();
    if !tail.is_empty() && !tail.starts_with('#') {
        return None;
    }
    Some((name, constraint))
}

fn is_fpm_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PACKAGE_NAME_BYTES
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn is_constraint(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CONSTRAINT_BYTES
        && value.is_ascii()
        && (value == "*" || value.bytes().any(|byte| byte.is_ascii_digit()))
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b' ' | b'.'
                        | b','
                        | b'*'
                        | b'^'
                        | b'~'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'!'
                        | b'&'
                        | b'|'
                        | b'('
                        | b')'
                        | b'+'
                        | b'-'
                        | b'_'
                )
        })
}

fn manifest_dependency(
    unit: &CodeUnit,
    name: &str,
    constraint: &str,
    scope: DependencyScope,
) -> Result<DependencyRecord, ParseError> {
    DependencyRecord::new(
        PackageIdentity::new(DependencyEcosystem::Fpm, name).map_err(ParseError::Internal)?,
        Some(DependencyVersion::new(constraint).map_err(ParseError::Internal)?),
        None,
        scope,
        false,
        DependencyDirectness::Direct,
        DependencyEvidenceLevel::ManifestDeclared,
        Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded root fpm manifest string dependency declaration",
        )
        .map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)
}

fn config_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new("fortran.fpm_manifest").map_err(ParseError::Internal)?),
        origin: config_origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded fpm manifest dependency inventory",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec!["fortran_project_config=fpm_manifest".to_string()],
    })
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
        target: Some(SymbolId::new(reason).map_err(ParseError::Internal)?),
        origin: config_origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=fortran_dependency_inventory".to_string(),
            format!("fortran_dependency_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: FORTRAN_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: FORTRAN_CONFIG_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn document<'a>(text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path: "fpm.toml",
            language: Language::FortranConfig,
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    #[test]
    fn inventories_root_runtime_and_development_string_requirements() {
        let output = parse_output(document(
            "[dependencies] # runtime\nstdlib = \"*\"\n[dev-dependencies] # development\ntest-drive = \"~0.5\"\n",
        ))
        .expect("parse");
        assert_eq!(output.dependencies.len(), 2);
        assert_eq!(
            output.dependencies[0].package.ecosystem,
            DependencyEcosystem::Fpm
        );
        assert_eq!(output.dependencies[0].scope, DependencyScope::Runtime);
        assert_eq!(output.dependencies[1].scope, DependencyScope::Development);
    }

    #[test]
    fn unsupported_shapes_are_omitted_without_path_or_url_leakage() {
        let output = parse_output(document(
            "[dependencies]\nstdlib = \"*\"\nsecret = { path = \"/private/source\" }\nremote = { git = \"https://token.invalid/repo\" }\nleaky = \"https://credential.invalid/repo\"\nnamespace.v = \"1\"\n",
        ))
        .expect("parse");
        assert_eq!(output.dependencies.len(), 1);
        assert_eq!(output.dependencies[0].package.name, "stdlib");
        let rendered = format!("{:?}", output.report.semantic_facts);
        assert!(rendered.contains("unsupported_dependency_shape"));
        assert!(!rendered.contains("/private/source"));
        assert!(!rendered.contains("token.invalid"));
        assert!(!rendered.contains("credential.invalid"));
    }

    #[test]
    fn conflicts_and_byte_limits_are_typed_unknown() {
        let output =
            parse_output(document("[dependencies]\nfoo = \"1\"\nfoo = \"2\"\n")).expect("parse");
        assert!(output.dependencies.is_empty());
        assert!(format!("{:?}", output.report.semantic_facts)
            .contains("conflicting_direct_dependencies"));

        let exact = format!("{}\n", "#".repeat(MAX_LINE_BYTES - 1))
            .repeat(MAX_MANIFEST_BYTES / MAX_LINE_BYTES);
        assert_eq!(exact.len(), MAX_MANIFEST_BYTES);
        let output = parse_output(document(&exact)).expect("exact");
        assert!(!format!("{:?}", output.report.semantic_facts).contains("manifest_byte_limit"));
        let over = format!("{exact}#");
        let output = parse_output(document(&over)).expect("over");
        assert!(format!("{:?}", output.report.semantic_facts).contains("manifest_byte_limit"));
        assert!(output.dependencies.is_empty());
    }

    #[test]
    fn dependency_record_limit_is_exact_and_plus_one_fails_closed() {
        let manifest = |count: usize| {
            let mut text = "[dependencies]\n".to_string();
            for index in 0..count {
                text.push_str(&format!("package{index} = \"*\"\n"));
            }
            text
        };
        let exact = manifest(MAX_DEPENDENCIES);
        let output = parse_output(document(&exact)).expect("exact records");
        assert_eq!(output.dependencies.len(), MAX_DEPENDENCIES);
        assert!(!format!("{:?}", output.report.semantic_facts).contains("manifest_resource_limit"));

        let over = manifest(MAX_DEPENDENCIES + 1);
        let output = parse_output(document(&over)).expect("over records");
        assert!(output.dependencies.is_empty());
        assert!(format!("{:?}", output.report.semantic_facts).contains("manifest_resource_limit"));
    }

    #[test]
    fn rejects_source_and_non_fpm_config_at_parser_boundary() {
        let mut source = document("program main\nend program\n");
        source.path = "main.f90";
        source.language = Language::Fortran;
        assert_eq!(parse_output(source), Err(ParseError::UnsupportedLanguage));
        let mut other = document("");
        other.path = "other.toml";
        assert_eq!(parse_output(other), Err(ParseError::UnsupportedLanguage));
    }
}
