//! Bounded Visual Basic .NET project dependency inventory.
//!
//! `.vb` source remains unread inventory. This adapter reads supplied
//! `visual-basic-config` bytes only, recognizes literal `PackageReference`
//! declarations in an exact `.vbproj`, and never evaluates MSBuild, imports,
//! properties, conditions, targets, SDKs, NuGet restore, or project code.

pub(crate) mod mstest;
mod syntax;

use super::bounded_xml::{parse_bounded_xml, BoundedXmlError, BoundedXmlLimits, XmlDocument};
use super::config_source_parse_output;
use crate::core::model::{
    CodeUnit, DependencyDirectness, DependencyEcosystem, DependencyEvidenceLevel, DependencyRecord,
    DependencyScope, DependencyVersion, Evidence, FactCertainty, FactOrigin, Language,
    PackageIdentity, SemanticFact, SemanticFactKind, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

const VB_CONFIG_ENGINE: &str = "repogrammar-vbnet-project-config";
const VB_CONFIG_METHOD: &str = "bounded_vbproj_package_reference_inventory_v1";
const VB_DEPENDENCY_LIMIT: usize = 2_000;
const VB_XML_LIMITS: BoundedXmlLimits = BoundedXmlLimits {
    max_depth: 64,
    max_nodes: 16_384,
    max_attributes_per_node: 64,
    max_name_bytes: 256,
    max_value_bytes: 65_536,
};

#[derive(Debug, Default)]
pub struct VisualBasicProjectConfigParser;

impl SourceParser for VisualBasicProjectConfigParser {
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
    if document.language != Language::VisualBasicConfig
        || !document
            .path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.ends_with(".vbproj"))
    {
        return Err(ParseError::UnsupportedLanguage);
    }
    let unit = super::project_config_unit(&document, Language::VisualBasicConfig)?;
    let mut facts = vec![config_fact(
        &unit,
        "visual_basic.vbproj",
        "visual_basic_project_config=vbproj",
        "bounded Visual Basic .NET MSBuild project inventory",
    )?];
    let dependencies = match parse_bounded_xml(document.text, VB_XML_LIMITS) {
        Ok(xml) if xml.root().name() == "Project" => {
            let (dependencies, mut unknowns) = package_references(&xml, &unit)?;
            facts.append(&mut unknowns);
            dependencies
        }
        Ok(_) | Err(BoundedXmlError::Malformed) => {
            facts.push(dependency_unknown(
                &unit,
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "malformed_vbproj",
                "the .vbproj XML was malformed or did not have an exact Project root",
            )?);
            Vec::new()
        }
        Err(BoundedXmlError::ResourceLimit) => {
            facts.push(dependency_unknown(
                &unit,
                "ResourceLimit",
                "vbproj_xml_limit",
                "the .vbproj XML exceeded the bounded metadata limits",
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
    config_source_parse_output(vec![unit], facts, dependencies)
}

fn package_references(
    xml: &XmlDocument,
    unit: &CodeUnit,
) -> Result<(Vec<DependencyRecord>, Vec<SemanticFact>), ParseError> {
    let mut candidates = BTreeMap::<String, (String, Option<String>)>::new();
    let mut conflicted = BTreeSet::new();
    let mut malformed = false;
    let mut dynamic = xml.root().attribute("Sdk").is_some();
    let mut truncated = false;

    for node in xml.nodes() {
        if node.name() == "Import" {
            dynamic = true;
        }
        if node.name() != "PackageReference" {
            continue;
        }
        let Some(parent) = node.parent() else {
            malformed = true;
            continue;
        };
        let parent_node = xml.node(parent);
        if parent_node.name() != "ItemGroup"
            || parent_node
                .parent()
                .is_none_or(|grandparent| xml.node(grandparent).name() != "Project")
        {
            dynamic = true;
            continue;
        }
        if node.attribute("Condition").is_some()
            || parent_node.attribute("Condition").is_some()
            || node.attribute("Update").is_some()
            || node.attribute("Remove").is_some()
        {
            dynamic = true;
        }
        let Some(name) = node
            .attribute("Include")
            .filter(|name| valid_nuget_id(name))
        else {
            malformed = true;
            continue;
        };
        if node.attribute("Update").is_some() || node.attribute("Remove").is_some() {
            continue;
        }

        let mut versions = Vec::new();
        if let Some(version) = node.attribute("Version") {
            versions.push(version);
        }
        let mut version_override = node.attribute("VersionOverride").is_some();
        for child in node.children() {
            let child = xml.node(*child);
            if child.name() == "Version" {
                if child.attribute("Condition").is_some() {
                    dynamic = true;
                    continue;
                }
                versions.push(child.text_trimmed());
            } else if child.name() == "VersionOverride" {
                version_override = true;
            }
        }
        if version_override {
            dynamic = true;
            versions.clear();
        }
        versions.sort_unstable();
        versions.dedup();
        if versions.len() > 1 {
            conflicted.insert(name.to_ascii_lowercase());
            continue;
        }
        let requirement = versions.first().copied().filter(|value| !value.is_empty());
        let requirement = match requirement {
            Some(value) if valid_literal_version(value) => Some(value.to_string()),
            Some(_) => {
                dynamic = true;
                None
            }
            None => None,
        };
        let key = name.to_ascii_lowercase();
        if conflicted.contains(&key) {
            continue;
        }
        match candidates.get(&key) {
            Some((_, existing)) if existing != &requirement => {
                candidates.remove(&key);
                conflicted.insert(key);
            }
            Some(_) => {}
            None if candidates.len() == VB_DEPENDENCY_LIMIT => {
                truncated = true;
            }
            None => {
                candidates.insert(key, (name.to_string(), requirement));
            }
        }
    }
    for key in &conflicted {
        candidates.remove(key);
    }

    let mut dependencies = Vec::with_capacity(candidates.len());
    for (_, (name, requirement)) in candidates {
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(DependencyEcosystem::Nuget, name)
                    .map_err(ParseError::Internal)?,
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
                    "literal PackageReference declaration; MSBuild and NuGet were not evaluated",
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
            "partial_package_reference_inventory",
            "invalid or unsupported PackageReference declarations were omitted",
        )?);
    }
    if dynamic {
        unknowns.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "msbuild_evaluation_not_performed",
            "MSBuild imports, SDK defaults, properties, conditions, updates, or overrides were not evaluated",
        )?);
    }
    if !conflicted.is_empty() {
        unknowns.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_package_references",
            "case-insensitive NuGet identities with conflicting requirements were omitted",
        )?);
    }
    if truncated {
        unknowns.push(dependency_unknown(
            unit,
            "ResourceLimit",
            "package_reference_dependency_limit",
            "PackageReference inventory exceeded the bounded record limit",
        )?);
    }
    Ok((dependencies, unknowns))
}

fn valid_nuget_id(value: &str) -> bool {
    (1..=100).contains(&value.len())
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && !value.contains("$(")
}

fn valid_literal_version(value: &str) -> bool {
    (1..=256).contains(&value.len())
        && value.is_ascii()
        && !value.bytes().any(|byte| byte.is_ascii_control())
        && !["$(", "@(", "%("]
            .iter()
            .any(|marker| value.contains(marker))
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
            "affected_claim=visual_basic_dependency_inventory".to_string(),
            format!("visual_basic_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: VB_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: VB_CONFIG_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        VisualBasicProjectConfigParser
            .parse_with_context_output(
                SourceDocument {
                    path: "src/App.vbproj",
                    language: Language::VisualBasicConfig,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse vbproj")
    }

    #[test]
    fn literal_package_references_are_direct_manifest_inventory() {
        let parsed = output(
            r#"<Project><ItemGroup><PackageReference Include="Newtonsoft.Json" Version="[13.0.3]"/><PackageReference Include="xunit"><Version>2.9.3</Version><PrivateAssets>all</PrivateAssets></PackageReference><PackageReference Include="Private--Feed"/></ItemGroup></Project>"#,
        );
        assert_eq!(parsed.report.units.len(), 1);
        assert_eq!(parsed.report.units[0].language, Language::VisualBasicConfig);
        assert_eq!(parsed.dependencies.len(), 3);
        for dependency in &parsed.dependencies {
            assert_eq!(dependency.package.ecosystem, DependencyEcosystem::Nuget);
            assert_eq!(dependency.directness, DependencyDirectness::Direct);
            assert_eq!(dependency.scope, DependencyScope::Unknown);
            assert_eq!(
                dependency.evidence_level,
                DependencyEvidenceLevel::ManifestDeclared
            );
            assert!(dependency.resolved_version.is_none());
        }
    }

    #[test]
    fn conditional_child_version_is_not_claimed_as_a_literal_requirement() {
        let parsed = output(
            r#"<Project><ItemGroup><PackageReference Include="Conditional.Version"><Version Condition="'$(Configuration)' == 'Release'">2.0.0</Version></PackageReference></ItemGroup></Project>"#,
        );
        assert_eq!(parsed.dependencies.len(), 1);
        assert!(parsed.dependencies[0].requirement.is_none());
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .contains(&"visual_basic_unknown_kind=msbuild_evaluation_not_performed".to_string())
        }));
    }

    #[test]
    fn dynamic_conflicting_and_malformed_entries_fail_closed_without_leaking_values() {
        let parsed = output(
            r#"<Project Sdk="Microsoft.NET.Sdk"><Import Project="private.props"/><ItemGroup Condition="'$(TargetFramework)' == 'net8.0'"><PackageReference Include="Example" Version="1.0"/><PackageReference Include="example" Version="2.0"/><PackageReference Include="$(PrivatePackage)" Version="$(PrivateVersion)"/></ItemGroup></Project>"#,
        );
        assert!(parsed.dependencies.is_empty());
        let rendered = format!("{:?}", parsed.report.semantic_facts);
        assert!(rendered.contains("msbuild_evaluation_not_performed"));
        assert!(rendered.contains("conflicting_package_references"));
        assert!(rendered.contains("partial_package_reference_inventory"));
        assert!(!rendered.contains("private.props"));
        assert!(!rendered.contains("PrivatePackage"));
        assert!(!rendered.contains("PrivateVersion"));
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
                        .contains(&"affected_claim=visual_basic_dependency_inventory".to_string())
            }));
        }
    }

    #[test]
    fn dependency_limit_is_inclusive_and_plus_one_is_typed_unknown() {
        let render = |count: usize| {
            let mut text = String::from("<Project><ItemGroup>");
            for index in 0..count {
                text.push_str(&format!(
                    "<PackageReference Include='Package{index}' Version='1.0'/>",
                ));
            }
            text.push_str("</ItemGroup></Project>");
            output(&text)
        };
        assert_eq!(
            render(VB_DEPENDENCY_LIMIT).dependencies.len(),
            VB_DEPENDENCY_LIMIT
        );
        let plus_one = render(VB_DEPENDENCY_LIMIT + 1);
        assert_eq!(plus_one.dependencies.len(), VB_DEPENDENCY_LIMIT);
        assert!(plus_one.report.semantic_facts.iter().any(|fact| {
            fact.assumptions.iter().any(|assumption| {
                assumption == "visual_basic_unknown_kind=package_reference_dependency_limit"
            })
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
        let exact = render(VB_XML_LIMITS.max_depth - 1);
        assert!(!exact.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "visual_basic_unknown_kind=vbproj_xml_limit")
        }));
        let plus_one = render(VB_XML_LIMITS.max_depth);
        assert!(plus_one.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "visual_basic_unknown_kind=vbproj_xml_limit")
        }));
    }
}
