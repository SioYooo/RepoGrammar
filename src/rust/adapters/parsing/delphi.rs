//! Bounded Delphi `.dproj` runtime-package inventory.
//!
//! Object Pascal source (`.pas`, `.dpr`, `.dpk`) remains unread inventory.
//! This adapter reads supplied `delphi-config` bytes only and extracts literal
//! `DCC_UsePackage` values without evaluating MSBuild, conditions, imports,
//! targets, compiler settings, package source, or project code. Free Pascal and
//! Lazarus metadata are explicitly outside this dialect profile.

use super::bounded_xml::{parse_bounded_xml, BoundedXmlError, BoundedXmlLimits, XmlDocument};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot, Evidence,
    FactCertainty, FactOrigin, Language, PackageIdentity, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::BTreeMap;

pub mod dunitx;

const DELPHI_CONFIG_ENGINE: &str = "repogrammar-delphi-project-config";
const DELPHI_CONFIG_METHOD: &str = "bounded_dproj_runtime_package_inventory_v1";
const DELPHI_DEPENDENCY_LIMIT: usize = 2_000;
const DELPHI_XML_LIMITS: BoundedXmlLimits = BoundedXmlLimits {
    max_depth: 64,
    max_nodes: 16_384,
    max_attributes_per_node: 64,
    max_name_bytes: 256,
    max_value_bytes: 65_536,
};

#[derive(Debug, Default)]
pub struct DelphiProjectConfigParser;

impl SourceParser for DelphiProjectConfigParser {
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
    if document.language != Language::DelphiConfig
        || !document
            .path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.ends_with(".dproj"))
    {
        return Err(ParseError::UnsupportedLanguage);
    }
    let unit = project_config_unit(&document)?;
    let mut facts = vec![config_fact(
        &unit,
        "delphi.dproj",
        "delphi_project_config=dproj",
        "bounded Delphi MSBuild project inventory",
    )?];
    let dependencies = match parse_bounded_xml(document.text, DELPHI_XML_LIMITS) {
        Ok(xml) if xml.root().name() == "Project" => {
            let (dependencies, mut unknowns) = runtime_packages(&xml, &unit)?;
            facts.append(&mut unknowns);
            dependencies
        }
        Ok(_) | Err(BoundedXmlError::Malformed) => {
            facts.push(dependency_unknown(
                &unit,
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "malformed_dproj",
                "the .dproj XML was malformed or did not have an exact Project root",
            )?);
            Vec::new()
        }
        Err(BoundedXmlError::ResourceLimit) => {
            facts.push(dependency_unknown(
                &unit,
                "ResourceLimit",
                "dproj_xml_limit",
                "the .dproj XML exceeded the bounded metadata limits",
            )?);
            Vec::new()
        }
    };
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.note.as_str(),
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.note.as_str(),
            ))
    });
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    let dependencies = DependencySnapshot::new(dependencies, Vec::new())
        .map_err(ParseError::Internal)?
        .dependencies;
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

fn runtime_packages(
    xml: &XmlDocument,
    unit: &CodeUnit,
) -> Result<(Vec<DependencyRecord>, Vec<SemanticFact>), ParseError> {
    let mut packages = BTreeMap::<String, String>::new();
    let mut dynamic = false;
    let mut malformed = false;
    let mut truncated = false;

    for node in xml.nodes() {
        if node.name() == "Import" {
            dynamic = true;
        }
        if node.name() != "DCC_UsePackage" {
            continue;
        }
        let Some(parent) = node.parent() else {
            malformed = true;
            continue;
        };
        let parent = xml.node(parent);
        if parent.name() != "PropertyGroup"
            || parent
                .parent()
                .is_none_or(|grandparent| xml.node(grandparent).name() != "Project")
        {
            dynamic = true;
            continue;
        }
        if parent.attribute("Condition").is_some() || node.attribute("Condition").is_some() {
            dynamic = true;
        }
        for package in node.text_trimmed().split(';').map(str::trim) {
            if package.is_empty() {
                continue;
            }
            if package.contains("$(") || package.contains("@(") || package.contains("%(") {
                dynamic = true;
                continue;
            }
            if !valid_delphi_package_name(package) {
                malformed = true;
                continue;
            }
            let key = package.to_ascii_lowercase();
            if packages.contains_key(&key) {
                continue;
            }
            if packages.len() == DELPHI_DEPENDENCY_LIMIT {
                truncated = true;
                continue;
            }
            packages.insert(key, package.to_string());
        }
    }

    let mut dependencies = Vec::with_capacity(packages.len());
    for (_, package) in packages {
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(DependencyEcosystem::DelphiPackage, package)
                    .map_err(ParseError::Internal)?,
                None,
                None,
                DependencyScope::Runtime,
                false,
                DependencyDirectness::Unknown,
                DependencyEvidenceLevel::ManifestDeclared,
                Evidence::new(
                    unit.id.clone(),
                    unit.range.clone(),
                    unit.provenance.clone(),
                    "literal DCC_UsePackage entry; MSBuild and Delphi package resolution were not evaluated",
                )
                .map_err(ParseError::Internal)?,
            )
            .map_err(ParseError::Internal)?,
        );
    }

    let mut unknowns = Vec::new();
    if malformed {
        unknowns.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_runtime_package_inventory",
            "invalid or unsupported runtime-package entries were omitted",
        )?);
    }
    if dynamic {
        unknowns.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "delphi_msbuild_evaluation_not_performed",
            "Delphi MSBuild imports, properties, conditions, or targets were not evaluated",
        )?);
    }
    if !dependencies.is_empty() {
        unknowns.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "runtime_package_directness",
            "the project list does not prove authored-direct versus automatically added package relationships",
        )?);
    }
    if truncated {
        unknowns.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "delphi_runtime_package_limit",
            "Delphi runtime-package inventory exceeded the bounded record limit",
        )?);
    }
    Ok((dependencies, unknowns))
}

fn valid_delphi_package_name(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    (1..=128).contains(&value.len())
        && value.is_ascii()
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        && !lowercase.ends_with(".bpl")
        && !lowercase.ends_with(".dcp")
}

fn project_config_unit(document: &SourceDocument<'_>) -> Result<CodeUnit, ParseError> {
    Ok(CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::DelphiConfig,
        kind: CodeUnitKind::ProjectConfig,
        range: SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?,
        provenance: Provenance::new(
            document.path,
            document.content_hash.clone(),
            document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?,
    })
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
            "affected_claim=delphi_dependency_inventory".to_string(),
            format!("delphi_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: DELPHI_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: DELPHI_CONFIG_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        DelphiProjectConfigParser
            .parse_with_context_output(
                SourceDocument {
                    path: "src/App.dproj",
                    language: Language::DelphiConfig,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse dproj")
    }

    #[test]
    fn literal_runtime_packages_are_unknown_directness_manifest_inventory() {
        let parsed = output(
            r#"<Project><PropertyGroup><DCC_UsePackage>rtl;vcl;Example.Runtime</DCC_UsePackage></PropertyGroup></Project>"#,
        );
        assert_eq!(parsed.report.units[0].language, Language::DelphiConfig);
        assert_eq!(parsed.dependencies.len(), 3);
        for dependency in &parsed.dependencies {
            assert_eq!(
                dependency.package.ecosystem,
                DependencyEcosystem::DelphiPackage
            );
            assert_eq!(dependency.scope, DependencyScope::Runtime);
            assert_eq!(dependency.directness, DependencyDirectness::Unknown);
            assert_eq!(
                dependency.evidence_level,
                DependencyEvidenceLevel::ManifestDeclared
            );
            assert!(dependency.requirement.is_none());
            assert!(dependency.resolved_version.is_none());
        }
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .contains(&"delphi_unknown_kind=runtime_package_directness".to_string())
        }));
    }

    #[test]
    fn conditions_property_chains_and_invalid_entries_are_source_free_unknowns() {
        let parsed = output(
            r#"<Project><PropertyGroup Condition="'$(Base)'!=''"><DCC_UsePackage>rtl;$(DCC_UsePackage);private/path.bpl</DCC_UsePackage></PropertyGroup><Import Project="private.targets"/></Project>"#,
        );
        assert_eq!(parsed.dependencies.len(), 1);
        assert_eq!(parsed.dependencies[0].package.name, "rtl");
        let rendered = format!("{:?}", parsed.report.semantic_facts);
        assert!(rendered.contains("delphi_msbuild_evaluation_not_performed"));
        assert!(rendered.contains("partial_runtime_package_inventory"));
        assert!(!rendered.contains("private.targets"));
        assert!(!rendered.contains("private/path"));
    }

    #[test]
    fn doctype_and_non_project_roots_emit_only_typed_unknown() {
        for text in [
            "<!DOCTYPE Project [<!ENTITY x SYSTEM 'file:///secret'>]><Project>&x;</Project>",
            "<NotProject/>",
        ] {
            let parsed = output(text);
            assert!(parsed.dependencies.is_empty());
            assert!(parsed.report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact
                        .assumptions
                        .contains(&"affected_claim=delphi_dependency_inventory".to_string())
            }));
        }
    }

    #[test]
    fn dependency_limit_is_inclusive_and_plus_one_is_typed_unknown() {
        let render = |count: usize| {
            let mut text = String::from("<Project><PropertyGroup><DCC_UsePackage>");
            for index in 0..count {
                text.push_str(&format!("Package{index};"));
            }
            text.push_str("</DCC_UsePackage></PropertyGroup></Project>");
            output(&text)
        };
        assert_eq!(
            render(DELPHI_DEPENDENCY_LIMIT).dependencies.len(),
            DELPHI_DEPENDENCY_LIMIT
        );
        let plus_one = render(DELPHI_DEPENDENCY_LIMIT + 1);
        assert_eq!(plus_one.dependencies.len(), DELPHI_DEPENDENCY_LIMIT);
        assert!(plus_one.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "delphi_unknown_kind=delphi_runtime_package_limit")
        }));
    }

    #[test]
    fn xml_depth_limit_is_inclusive_and_plus_one_is_typed_unknown() {
        let render = |nested_nodes: usize| {
            let mut text = String::from("<Project>");
            for _ in 0..nested_nodes {
                text.push_str("<Group>");
            }
            for _ in 0..nested_nodes {
                text.push_str("</Group>");
            }
            text.push_str("</Project>");
            output(&text)
        };
        let exact = render(DELPHI_XML_LIMITS.max_depth - 1);
        assert!(!exact.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "delphi_unknown_kind=dproj_xml_limit")
        }));
        let plus_one = render(DELPHI_XML_LIMITS.max_depth);
        assert!(plus_one.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "delphi_unknown_kind=dproj_xml_limit")
        }));
    }
}
