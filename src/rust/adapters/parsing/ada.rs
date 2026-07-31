//! Bounded, non-executing Ada/Alire project-metadata inventory.
//!
//! Only exact, unconditional string declarations under `[[depends-on]]` in an
//! `alire.toml` are admitted. GPR source naming and Alire's internal lockfile
//! format remain inventory-only/UNKNOWN; no Ada or Alire tool is invoked.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot,
    DependencyVersion, Evidence, FactCertainty, FactOrigin, Language, PackageIdentity, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

const ADA_CONFIG_ENGINE: &str = "repogrammar-ada-project-config";
const ADA_CONFIG_METHOD: &str = "bounded_alire_dependency_inventory_v1";
const MAX_MANIFEST_BYTES: usize = 1_048_576;
const MAX_MANIFEST_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 4_096;
const MAX_DEPENDENCIES: usize = 2_000;
const MAX_CRATE_NAME_BYTES: usize = 128;
const MAX_CONSTRAINT_BYTES: usize = 256;

#[derive(Debug, Default)]
pub struct AdaProjectConfigParser;

impl SourceParser for AdaProjectConfigParser {
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
    if document.language != Language::AdaConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    if !matches!(basename, "alire.toml" | "alire.lock") {
        return Err(ParseError::UnsupportedLanguage);
    }
    let unit = project_config_unit(&document)?;
    let (mut facts, dependencies) = if basename == "alire.toml" {
        alire_manifest_inventory(document.text, &unit)?
    } else {
        alire_lock_inventory(document.text, &unit)?
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
    Ok(SourceParseOutput {
        report: ParseReport {
            ir_nodes: ir_nodes_for_units(&units).map_err(ParseError::Internal)?,
            ir_edges: ir_edges_for_units(&units).map_err(ParseError::Internal)?,
            units,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies,
    })
}

fn project_config_unit(document: &SourceDocument<'_>) -> Result<CodeUnit, ParseError> {
    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    Ok(CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::AdaConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance: Provenance::new(
            document.path,
            document.content_hash.clone(),
            document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestSection {
    Other,
    DirectDependencies,
}

fn alire_manifest_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let mut facts = vec![config_fact(
        unit,
        "ada.alire_manifest",
        "ada_project_config=alire_manifest",
        "bounded Alire manifest dependency inventory",
    )?];
    if text.len() > MAX_MANIFEST_BYTES {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "manifest_byte_limit",
            "Alire manifest exceeds the bounded byte limit",
        )?);
        return Ok((facts, Vec::new()));
    }

    let mut section = ManifestSection::Other;
    let mut requirements = BTreeMap::<String, String>::new();
    let mut conflicts = BTreeSet::new();
    let mut malformed = false;
    let mut unsupported_dynamic = false;
    let mut pins_present = false;
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
            section = if header == "[[depends-on]]" {
                ManifestSection::DirectDependencies
            } else {
                if header.starts_with("[depends-on.") || header.starts_with("[[depends-on.") {
                    unsupported_dynamic = true;
                }
                if header.starts_with("[pins") || header.starts_with("[[pins") {
                    pins_present = true;
                }
                ManifestSection::Other
            };
            continue;
        }
        if section != ManifestSection::DirectDependencies {
            continue;
        }
        let Some((name, constraint)) = parse_string_assignment(trimmed) else {
            malformed = true;
            continue;
        };
        if !is_alire_crate_name(name) || !is_constraint(constraint) {
            malformed = true;
            continue;
        }
        if conflicts.contains(name) {
            continue;
        }
        if let Some(existing) = requirements.get(name) {
            if existing != constraint {
                requirements.remove(name);
                conflicts.insert(name.to_string());
            }
            continue;
        }
        if requirements.len() == MAX_DEPENDENCIES {
            resource_limit = true;
            continue;
        }
        requirements.insert(name.to_string(), constraint.to_string());
    }

    if malformed || resource_limit {
        requirements.clear();
    }
    let dependencies = requirements
        .into_iter()
        .map(|(name, constraint)| manifest_dependency(unit, &name, &constraint))
        .collect::<Result<Vec<_>, _>>()?;
    if malformed {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "malformed_direct_dependency_table",
            "Alire direct dependency inventory contains unsupported or malformed syntax",
        )?);
    }
    if unsupported_dynamic {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::BuildVariantAmbiguity.as_protocol_str(),
            "conditional_dependency_table",
            "conditional Alire dependencies were omitted because case selection is unresolved",
        )?);
    }
    if pins_present {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "dependency_pin_override",
            "Alire pins may override declared requirements and were not resolved",
        )?);
    }
    if !conflicts.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_direct_dependencies",
            "Alire dependency names with conflicting constraints were omitted",
        )?);
    }
    if resource_limit {
        facts.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "manifest_resource_limit",
            "Alire dependency inventory exceeded a bounded resource limit",
        )?);
    }
    Ok((facts, dependencies))
}

fn alire_lock_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let reason = if text.len() > MAX_MANIFEST_BYTES {
        "ResourceLimit"
    } else {
        UnknownReasonCode::InsufficientSupport.as_protocol_str()
    };
    let kind = if text.len() > MAX_MANIFEST_BYTES {
        "lockfile_byte_limit"
    } else {
        "internal_lockfile_schema"
    };
    let note = if text.len() > MAX_MANIFEST_BYTES {
        "Alire lockfile exceeds the bounded byte limit"
    } else {
        "Alire lockfiles are internal implementation state and were not interpreted"
    };
    Ok((
        vec![
            config_fact(
                unit,
                "ada.alire_lock",
                "ada_project_config=alire_lock",
                "Alire internal lockfile inventory without schema interpretation",
            )?,
            dependency_unknown(unit, reason, kind, note)?,
        ],
        Vec::new(),
    ))
}

fn parse_string_assignment(line: &str) -> Option<(&str, &str)> {
    let (name, raw_value) = line.split_once('=')?;
    let name = name.trim();
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

fn is_alire_crate_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CRATE_NAME_BYTES
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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
) -> Result<DependencyRecord, ParseError> {
    DependencyRecord::new(
        PackageIdentity::new(DependencyEcosystem::Alire, name).map_err(ParseError::Internal)?,
        Some(DependencyVersion::new(constraint).map_err(ParseError::Internal)?),
        None,
        DependencyScope::Runtime,
        false,
        DependencyDirectness::Direct,
        DependencyEvidenceLevel::ManifestDeclared,
        Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded unconditional Alire manifest dependency declaration",
        )
        .map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)
}

fn config_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: config_origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![assumption.to_string()],
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
            "affected_claim=ada_dependency_inventory".to_string(),
            format!("ada_dependency_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: ADA_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: ADA_CONFIG_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::AdaConfig,
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    #[test]
    fn inventories_only_unconditional_alire_dependencies() {
        let output = parse_output(document(
            "alire.toml",
            "name = \"demo\"\n[[depends-on]] # direct\ngnatcoll = \"^25.0\"\nlibadalang = \"~25.0\" # bounded\n",
        ))
        .expect("parse");
        assert_eq!(output.dependencies.len(), 2);
        assert_eq!(
            output.dependencies[0].package.ecosystem,
            DependencyEcosystem::Alire
        );
        assert_eq!(output.dependencies[0].package.name, "gnatcoll");
        assert_eq!(output.dependencies[0].scope, DependencyScope::Runtime);
        assert!(output.report.semantic_facts.iter().all(|fact| {
            !fact.evidence.note.contains("demo") && !fact.assumptions.join(" ").contains("demo")
        }));
    }

    #[test]
    fn dynamic_pins_conflicts_and_internal_lockfile_are_typed_unknown() {
        let output = parse_output(document(
            "alire.toml",
            "[[depends-on]]\nfoo = \"^1\"\nfoo = \"^2\"\n[[depends-on.'case(os)']]\nsecret = \"*\"\n[[pins]]\nfoo = { path = \"/private/secret\" }\n",
        ))
        .expect("parse");
        assert!(output.dependencies.is_empty());
        let rendered = format!("{:?}", output.report.semantic_facts);
        assert!(rendered.contains("conditional_dependency_table"));
        assert!(rendered.contains("dependency_pin_override"));
        assert!(rendered.contains("conflicting_direct_dependencies"));
        assert!(!rendered.contains("/private/secret"));
        assert!(!rendered.contains("secret ="));

        let lock = parse_output(document("alire/alire.lock", "secret = '/private/path'\n"))
            .expect("parse lock");
        assert!(lock.dependencies.is_empty());
        let rendered = format!("{:?}", lock.report.semantic_facts);
        assert!(rendered.contains("internal_lockfile_schema"));
        assert!(!rendered.contains("/private/path"));

        let leaky = parse_output(document(
            "alire.toml",
            "[[depends-on]]\nprivate_crate = \"https://token.invalid/repo\"\n",
        ))
        .expect("parse invalid URL constraint");
        assert!(leaky.dependencies.is_empty());
        assert!(!format!("{:?}", leaky.report.semantic_facts).contains("token.invalid"));
    }

    #[test]
    fn byte_limit_is_exact_and_plus_one_fails_closed() {
        let exact = format!("{}\n", "#".repeat(MAX_LINE_BYTES - 1))
            .repeat(MAX_MANIFEST_BYTES / MAX_LINE_BYTES);
        assert_eq!(exact.len(), MAX_MANIFEST_BYTES);
        let output = parse_output(document("alire.toml", &exact)).expect("exact");
        assert!(!format!("{:?}", output.report.semantic_facts).contains("manifest_byte_limit"));
        let over = format!("{exact}#");
        let output = parse_output(document("alire.toml", &over)).expect("over");
        assert!(format!("{:?}", output.report.semantic_facts).contains("manifest_byte_limit"));
        assert!(output.dependencies.is_empty());

        let lock = parse_output(document("alire/alire.lock", &over)).expect("over lock");
        let rendered = format!("{:?}", lock.report.semantic_facts);
        assert!(rendered.contains("ResourceLimit"));
        assert!(rendered.contains("lockfile_byte_limit"));
    }

    #[test]
    fn dependency_record_limit_is_exact_and_plus_one_fails_closed() {
        let manifest = |count: usize| {
            let mut text = "[[depends-on]]\n".to_string();
            for index in 0..count {
                text.push_str(&format!("crate{index} = \"*\"\n"));
            }
            text
        };
        let exact = manifest(MAX_DEPENDENCIES);
        let output = parse_output(document("alire.toml", &exact)).expect("exact records");
        assert_eq!(output.dependencies.len(), MAX_DEPENDENCIES);
        assert!(!format!("{:?}", output.report.semantic_facts).contains("manifest_resource_limit"));

        let over = manifest(MAX_DEPENDENCIES + 1);
        let output = parse_output(document("alire.toml", &over)).expect("over records");
        assert!(output.dependencies.is_empty());
        assert!(format!("{:?}", output.report.semantic_facts).contains("manifest_resource_limit"));
    }

    #[test]
    fn rejects_gpr_and_source_at_parser_boundary() {
        for (path, language) in [
            ("demo.gpr", Language::AdaConfig),
            ("main.adb", Language::Ada),
        ] {
            let mut doc = document(path, "project Demo is end Demo;");
            doc.language = language;
            assert_eq!(parse_output(doc), Err(ParseError::UnsupportedLanguage));
        }
    }
}
