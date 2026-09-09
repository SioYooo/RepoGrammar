//! Bounded, non-executing R package metadata inventory.
//!
//! R source remains discovery-only. This adapter reads only exact
//! `DESCRIPTION`, `NAMESPACE`, and `renv.lock` documents supplied by the
//! source-store boundary. It never invokes R, evaluates namespace directives,
//! loads profiles, restores packages, connects to repositories, or retains
//! URLs, local paths, credentials, or arbitrary source literals in output.

pub(crate) mod testthat;

use super::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use super::config_source_parse_output;
use crate::core::model::{
    CodeUnit, DependencyDirectness, DependencyEcosystem, DependencyEvidenceLevel, DependencyRecord,
    DependencyScope, DependencyVersion, Evidence, FactCertainty, FactOrigin, Language,
    PackageIdentity, SemanticFact, SemanticFactKind, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const R_CONFIG_ENGINE: &str = "repogrammar-r-project-config";
const R_CONFIG_METHOD: &str = "bounded_r_package_metadata_inventory_v1";
const R_CONFIG_MAX_BYTES: usize = 1_048_576;
const R_CONFIG_MAX_LINES: usize = 16_384;
const R_DESCRIPTION_MAX_FIELDS: usize = 512;
const R_DEPENDENCY_LIMIT: usize = 2_000;
const R_PACKAGE_NAME_LIMIT: usize = 256;
const R_VERSION_LIMIT: usize = 256;
const R_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: 128,
    max_members: 8_192,
    max_key_bytes: 256,
};

#[derive(Debug, Default)]
pub struct RProjectConfigParser;

impl SourceParser for RProjectConfigParser {
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
    if document.language != Language::RConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    if !matches!(basename, "DESCRIPTION" | "NAMESPACE" | "renv.lock") {
        return Err(ParseError::UnsupportedLanguage);
    }

    let unit = super::project_config_unit(&document, Language::RConfig)?;

    let line_count = document.text.split('\n').count();
    let (mut facts, dependencies) =
        if document.text.len() > R_CONFIG_MAX_BYTES || line_count > R_CONFIG_MAX_LINES {
            (
                vec![dependency_unknown(
                    &unit,
                    "ResourceLimit",
                    "r_config_document_limit",
                    "R project metadata exceeded the bounded byte or line limit",
                )?],
                Vec::new(),
            )
        } else if document
            .text
            .bytes()
            .any(|byte| byte == 0 || (byte < 0x20 && !matches!(byte, b'\t' | b'\r' | b'\n')))
        {
            malformed_inventory(&unit, "r_config_control_character")?
        } else {
            match basename {
                "DESCRIPTION" => description_inventory(document.text, &unit)?,
                "NAMESPACE" => namespace_inventory(document.text, &unit)?,
                "renv.lock" => renv_lock_inventory(document.text, &unit)?,
                _ => unreachable!("basename checked above"),
            }
        };
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.assumptions.as_slice(),
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.assumptions.as_slice(),
            ))
    });
    config_source_parse_output(vec![unit], facts, dependencies)
}

fn description_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let fields = match parse_dcf_fields(text) {
        Ok(fields) => fields,
        Err(DcfError::ResourceLimit) => {
            return Ok((
                vec![dependency_unknown(
                    unit,
                    "ResourceLimit",
                    "description_field_limit",
                    "DESCRIPTION exceeded the bounded field limit",
                )?],
                Vec::new(),
            ));
        }
        Err(DcfError::Malformed) => return malformed_inventory(unit, "malformed_description"),
    };

    let mut declaration_count = 0usize;
    let mut invalid = false;
    let mut saw_r_runtime = false;
    let has_unretained_source_metadata = fields.keys().any(|field| {
        matches!(
            field.as_str(),
            "Additional_repositories" | "Remotes" | "Repository"
        ) || field.starts_with("Remote")
    });
    let mut names = BTreeSet::new();
    let mut conflicts = BTreeSet::new();
    for field in ["Depends", "Imports", "LinkingTo", "Suggests", "Enhances"] {
        let Some(value) = fields.get(field) else {
            continue;
        };
        for candidate in value.split(',') {
            declaration_count = declaration_count.saturating_add(1);
            let Some((name, _requirement)) = parse_description_dependency(candidate) else {
                invalid = true;
                continue;
            };
            if name == "R" {
                saw_r_runtime = true;
                continue;
            }
            if !names.insert(name.to_string()) {
                conflicts.insert(name.to_string());
            }
        }
    }
    if declaration_count > R_DEPENDENCY_LIMIT {
        return Ok((
            vec![dependency_unknown(
                unit,
                "ResourceLimit",
                "description_dependency_limit",
                "DESCRIPTION dependency declarations exceeded the bounded record limit",
            )?],
            Vec::new(),
        ));
    }

    let mut facts = vec![config_fact(
        unit,
        "r.description_dependency_inventory",
        "r_project_config=description",
        "bounded DESCRIPTION dependency fields were classified without evaluating R",
    )?];
    if !names.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "description_dependency_ecosystem",
            "DESCRIPTION proves direct R package declarations but not whether each package comes from CRAN, Bioconductor, or another repository; ambiguous records were omitted",
        )?);
    }
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_description_dependency_inventory",
            "DESCRIPTION dependency entries with unsupported names or version requirements were omitted",
        )?);
    }
    if !conflicts.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_description_dependency_fields",
            "R package names repeated across DESCRIPTION dependency fields were not collapsed into one scope",
        )?);
    }
    if saw_r_runtime {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "r_runtime_requirement",
            "The R runtime requirement is not a CRAN or Bioconductor package dependency and was omitted",
        )?);
    }
    if has_unretained_source_metadata {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_or_sensitive_description_source",
            "DESCRIPTION repository or remote-source metadata was not retained or used to infer dependency ecosystems",
        )?);
    }
    Ok((facts, Vec::new()))
}

fn namespace_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let mut imports = BTreeSet::new();
    let mut unsupported = false;
    let mut import_count = 0usize;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(names) = parse_namespace_imports(line) {
            for name in names {
                import_count = import_count.saturating_add(1);
                if import_count > R_DEPENDENCY_LIMIT {
                    return Ok((
                        vec![dependency_unknown(
                            unit,
                            "ResourceLimit",
                            "namespace_import_limit",
                            "NAMESPACE import directives exceeded the bounded record limit",
                        )?],
                        Vec::new(),
                    ));
                }
                imports.insert(name.to_string());
            }
        } else if line.starts_with("import") || line.starts_with("if(") || line.starts_with("if (")
        {
            unsupported = true;
        }
    }

    let mut facts = vec![config_fact(
        unit,
        "r.namespace_import_inventory",
        "r_project_config=namespace",
        "bounded literal NAMESPACE imports were classified without processing R directives",
    )?];
    if !imports.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "namespace_dependency_ecosystem",
            "NAMESPACE proves direct package imports but not CRAN versus Bioconductor source or a version; ambiguous records were omitted",
        )?);
    }
    if unsupported {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_namespace_import_inventory",
            "Conditional, multiline, quoted, or otherwise non-literal NAMESPACE imports were omitted",
        )?);
    }
    Ok((facts, Vec::new()))
}

fn renv_lock_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let Ok(false) = has_duplicate_or_excess_members(text, R_JSON_LIMITS) else {
        return malformed_inventory(unit, "malformed_renv_lock");
    };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
        return malformed_inventory(unit, "malformed_renv_lock");
    };
    let Some(Value::Object(packages)) = root.get("Packages") else {
        return malformed_inventory(unit, "unsupported_renv_lock_schema");
    };
    let repository_locations_present = root
        .get("R")
        .and_then(Value::as_object)
        .is_some_and(|r| r.contains_key("Repositories"));
    if packages.len() > R_DEPENDENCY_LIMIT {
        return Ok((
            vec![dependency_unknown(
                unit,
                "ResourceLimit",
                "renv_lock_dependency_limit",
                "renv.lock package records exceeded the bounded record limit",
            )?],
            Vec::new(),
        ));
    }

    let mut dependencies = Vec::new();
    let mut invalid = false;
    let mut unsupported_source = false;
    for (key, value) in packages {
        let Some(record) = value.as_object() else {
            invalid = true;
            continue;
        };
        let (Some(name), Some(version)) = (
            record.get("Package").and_then(Value::as_str),
            record.get("Version").and_then(Value::as_str),
        ) else {
            invalid = true;
            continue;
        };
        if key != name || !is_r_package_name(name) || !is_r_version(version) {
            invalid = true;
            continue;
        }
        if record.keys().any(|field| {
            field.starts_with("Remote")
                || matches!(field.as_str(), "Path" | "URL" | "Url" | "RemoteUrl")
        }) {
            unsupported_source = true;
            continue;
        }
        let source = record.get("Source").and_then(Value::as_str);
        let repository = record.get("Repository").and_then(Value::as_str);
        let ecosystem = match (source, repository) {
            (Some("Repository"), Some("CRAN")) if !record.contains_key("biocViews") => {
                DependencyEcosystem::Cran
            }
            (Some("Bioconductor"), None | Some("Bioconductor")) => {
                DependencyEcosystem::Bioconductor
            }
            _ => {
                unsupported_source = true;
                continue;
            }
        };
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(ecosystem, name).map_err(ParseError::Internal)?,
                None,
                Some(DependencyVersion::new(version).map_err(ParseError::Internal)?),
                DependencyScope::Unknown,
                false,
                DependencyDirectness::Unknown,
                DependencyEvidenceLevel::LockfileResolved,
                Evidence::new(
                    unit.id.clone(),
                    unit.range.clone(),
                    unit.provenance.clone(),
                    "bounded renv.lock package record with explicit CRAN or Bioconductor source; directness, installation, and runtime selection remain unknown",
                )
                .map_err(ParseError::Internal)?,
            )
            .map_err(ParseError::Internal)?,
        );
    }

    let mut facts = vec![config_fact(
        unit,
        "r.renv_lock_inventory",
        "r_project_config=renv_lock",
        "bounded renv.lock package identity and version inventory",
    )?];
    if !dependencies.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "renv_lock_directness",
            "renv.lock package records do not prove root-direct versus transitive dependency relationships",
        )?);
    }
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_renv_lock_inventory",
            "renv.lock package records with malformed identity or version fields were omitted",
        )?);
    }
    if unsupported_source {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_or_sensitive_renv_source",
            "custom repository, remote, URL, local-path, or otherwise unproved package sources were omitted without retaining source details",
        )?);
    }
    if repository_locations_present {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "renv_repository_locations_not_retained",
            "renv.lock repository location metadata was intentionally omitted from inventory output",
        )?);
    }
    Ok((facts, dependencies))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DcfError {
    Malformed,
    ResourceLimit,
}

fn parse_dcf_fields(text: &str) -> Result<BTreeMap<String, String>, DcfError> {
    let mut fields = BTreeMap::<String, String>::new();
    let mut current_key: Option<String> = None;
    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.starts_with([' ', '\t']) {
            let Some(key) = current_key.as_ref() else {
                return Err(DcfError::Malformed);
            };
            let continuation = line.trim();
            if !continuation.is_empty() {
                let value = fields.get_mut(key).ok_or(DcfError::Malformed)?;
                value.push(' ');
                value.push_str(continuation);
            }
            continue;
        }
        if line.is_empty() {
            current_key = None;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(DcfError::Malformed);
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || fields.contains_key(key)
        {
            return Err(DcfError::Malformed);
        }
        if fields.len() == R_DESCRIPTION_MAX_FIELDS {
            return Err(DcfError::ResourceLimit);
        }
        fields.insert(key.to_string(), value.trim().to_string());
        current_key = Some(key.to_string());
    }
    Ok(fields)
}

fn parse_description_dependency(candidate: &str) -> Option<(&str, Option<&str>)> {
    let candidate = candidate.trim();
    if let Some(open) = candidate.find('(') {
        let name = candidate[..open].trim();
        let requirement = candidate[open + 1..].strip_suffix(')')?.trim();
        if !is_r_package_name_or_runtime(name) || !is_r_requirement(requirement) {
            return None;
        }
        Some((name, Some(requirement)))
    } else if is_r_package_name_or_runtime(candidate) {
        Some((candidate, None))
    } else {
        None
    }
}

fn parse_namespace_imports(line: &str) -> Option<Vec<&str>> {
    if let Some(arguments) = line
        .strip_prefix("importFrom(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (name, symbols) = arguments.split_once(',')?;
        let name = name.trim();
        return (is_r_package_name(name) && !symbols.trim().is_empty()).then_some(vec![name]);
    }
    let arguments = line
        .strip_prefix("import(")
        .and_then(|value| value.strip_suffix(')'))?;
    let names = arguments.split(',').map(str::trim).collect::<Vec<_>>();
    (!names.is_empty() && names.iter().all(|name| is_r_package_name(name))).then_some(names)
}

fn is_r_package_name_or_runtime(name: &str) -> bool {
    name == "R" || is_r_package_name(name)
}

fn is_r_package_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= R_PACKAGE_NAME_LIMIT
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.')
        && bytes.last() != Some(&b'.')
}

fn is_r_requirement(requirement: &str) -> bool {
    if requirement.is_empty() || requirement.len() > R_VERSION_LIMIT {
        return false;
    }
    let Some((operator, version)) = requirement.split_once(char::is_whitespace) else {
        return false;
    };
    matches!(operator, ">=" | "<=" | "==" | ">" | "<") && is_r_version(version.trim())
}

fn is_r_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= R_VERSION_LIMIT
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

fn malformed_inventory(
    unit: &CodeUnit,
    kind: &str,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    Ok((
        vec![dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            kind,
            "R package metadata is unavailable because the document is malformed, ambiguous, unsupported, or outside bounded admission rules",
        )?],
        Vec::new(),
    ))
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
            "affected_claim=r_dependency_inventory".to_string(),
            format!("r_unknown_kind={kind}"),
        ],
    })
}

fn config_origin() -> FactOrigin {
    FactOrigin {
        engine: R_CONFIG_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: R_CONFIG_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::RConfig,
            content_hash: ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
            repository_revision: RepositoryRevision::new("r-config-revision").expect("revision"),
            text,
        }
    }

    #[test]
    fn description_and_namespace_are_classified_but_registry_ambiguity_stays_unknown() {
        let description = parse_output(document(
            "DESCRIPTION",
            "Package: demo\nVersion: 1.0\nDepends: R (>= 4.3), stats\nImports: jsonlite (>= 1.8)\nSuggests: testthat\nLinkingTo: Rcpp\nAdditional_repositories: https://user:DESCRIPTION_SECRET@example.invalid/r\n",
        ))
        .expect("DESCRIPTION inventory");
        assert!(description.dependencies.is_empty());
        assert!(has_unknown(
            &description,
            "description_dependency_ecosystem"
        ));
        assert!(has_unknown(&description, "r_runtime_requirement"));
        assert!(has_unknown(
            &description,
            "unsupported_or_sensitive_description_source"
        ));
        let description_debug = format!("{description:?}");
        assert!(!description_debug.contains("DESCRIPTION_SECRET"));
        assert!(!description_debug.contains("example.invalid"));

        let namespace = parse_output(document(
            "NAMESPACE",
            "import(stats)\nimportFrom(jsonlite, fromJSON)\nexport(run)\n",
        ))
        .expect("NAMESPACE inventory");
        assert!(namespace.dependencies.is_empty());
        assert!(has_unknown(&namespace, "namespace_dependency_ecosystem"));
    }

    #[test]
    fn renv_lock_admits_only_explicit_cran_and_bioconductor_records() {
        let output = parse_output(document(
            "renv.lock",
            r#"{
              "R": {"Version": "4.4.0", "Repositories": [{"Name": "CRAN", "URL": "https://user:REPOSITORY_SECRET@example.invalid/r"}]},
              "Packages": {
                "jsonlite": {"Package": "jsonlite", "Version": "1.8.8", "Source": "Repository", "Repository": "CRAN"},
                "BiocGenerics": {"Package": "BiocGenerics", "Version": "0.50.0", "Source": "Bioconductor"},
                "privatepkg": {"Package": "privatepkg", "Version": "1.0", "Source": "GitHub", "RemoteUrl": "https://user:secret@example.invalid/private.git"},
                "localpkg": {"Package": "localpkg", "Version": "1.0", "Source": "/private/secret/pkg.tar.gz"}
              }
            }"#,
        ))
        .expect("renv inventory");
        assert_eq!(output.dependencies.len(), 2);
        assert!(output.dependencies.iter().any(|dependency| {
            dependency.package.ecosystem == DependencyEcosystem::Cran
                && dependency.package.name == "jsonlite"
        }));
        assert!(output.dependencies.iter().any(|dependency| {
            dependency.package.ecosystem == DependencyEcosystem::Bioconductor
                && dependency.package.name == "BiocGenerics"
        }));
        assert!(output
            .dependencies
            .iter()
            .all(|dependency| dependency.directness == DependencyDirectness::Unknown));
        assert!(has_unknown(&output, "unsupported_or_sensitive_renv_source"));
        assert!(has_unknown(
            &output,
            "renv_repository_locations_not_retained"
        ));
        let debug = format!("{output:?}");
        for secret in [
            "user:secret",
            "REPOSITORY_SECRET",
            "example.invalid",
            "/private/secret",
        ] {
            assert!(!debug.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn duplicate_json_members_and_conflicting_description_fields_fail_closed() {
        let lock = parse_output(document(
            "renv.lock",
            r#"{"Packages":{"x":{"Package":"x","Package":"y","Version":"1","Source":"Repository","Repository":"CRAN"}}}"#,
        ))
        .expect("typed malformed lock");
        assert!(lock.dependencies.is_empty());
        assert!(has_unknown(&lock, "malformed_renv_lock"));

        let description = parse_output(document(
            "DESCRIPTION",
            "Package: demo\nImports: jsonlite\nImports: secretpkg\n",
        ))
        .expect("typed malformed description");
        assert!(description.dependencies.is_empty());
        assert!(has_unknown(&description, "malformed_description"));
        assert!(!format!("{description:?}").contains("secretpkg"));
    }

    #[test]
    fn byte_line_field_and_dependency_limits_are_exact_then_fail_at_plus_one() {
        let exact_bytes = " ".repeat(R_CONFIG_MAX_BYTES);
        let exact = parse_output(document("NAMESPACE", &exact_bytes)).expect("exact bytes");
        assert!(!has_unknown(&exact, "r_config_document_limit"));
        let plus_one_bytes = " ".repeat(R_CONFIG_MAX_BYTES + 1);
        let plus_one =
            parse_output(document("NAMESPACE", &plus_one_bytes)).expect("plus one bytes");
        assert!(has_unknown(&plus_one, "r_config_document_limit"));

        let exact_lines = "#\n".repeat(R_CONFIG_MAX_LINES - 1);
        let exact = parse_output(document("NAMESPACE", &exact_lines)).expect("exact lines");
        assert!(!has_unknown(&exact, "r_config_document_limit"));
        let plus_one_lines = "#\n".repeat(R_CONFIG_MAX_LINES);
        let plus_one =
            parse_output(document("NAMESPACE", &plus_one_lines)).expect("plus one lines");
        assert!(has_unknown(&plus_one, "r_config_document_limit"));

        let exact_fields = (0..R_DESCRIPTION_MAX_FIELDS)
            .map(|index| format!("Field{index}: value\n"))
            .collect::<String>();
        let exact = parse_output(document("DESCRIPTION", &exact_fields)).expect("exact fields");
        assert!(!has_unknown(&exact, "description_field_limit"));
        let plus_one_fields = format!("{exact_fields}Overflow: value\n");
        let plus_one =
            parse_output(document("DESCRIPTION", &plus_one_fields)).expect("plus one field");
        assert!(has_unknown(&plus_one, "description_field_limit"));

        let exact_dependencies = format!(
            "Package: demo\nImports: {}\n",
            (0..R_DEPENDENCY_LIMIT)
                .map(|index| format!("pkg{index}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let exact = parse_output(document("DESCRIPTION", &exact_dependencies)).expect("exact deps");
        assert!(!has_unknown(&exact, "description_dependency_limit"));
        let plus_one_dependencies = format!("{exact_dependencies}Suggests: overflow\n");
        let plus_one =
            parse_output(document("DESCRIPTION", &plus_one_dependencies)).expect("plus one dep");
        assert!(has_unknown(&plus_one, "description_dependency_limit"));
    }

    fn has_unknown(output: &SourceParseOutput, kind: &str) -> bool {
        output.report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("r_unknown_kind={kind}"))
        })
    }
}
