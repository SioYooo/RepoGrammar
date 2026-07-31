//! Bounded static Composer dependency inventory.
//!
//! This adapter reads only discovered `composer.json` and `composer.lock`
//! documents. It never executes PHP, Composer, autoloaders, plugins, scripts,
//! repositories, or project code. Lock entries are static lockfile evidence;
//! they do not prove installation or runtime selection.

use super::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
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

const PHP_CONFIG_ENGINE: &str = "repogrammar-php-project-config";
const PHP_CONFIG_METHOD: &str = "bounded_composer_dependency_inventory_v1";
const COMPOSER_DEPENDENCY_LIMIT: usize = 2_000;
const COMPOSER_PACKAGE_NAME_LIMIT: usize = 255;
const COMPOSER_VERSION_LIMIT: usize = 256;
const COMPOSER_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: 128,
    max_members: 8_192,
    max_key_bytes: 256,
};

#[derive(Debug, Default)]
pub struct PhpConfigParser;

impl SourceParser for PhpConfigParser {
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
    if document.language != Language::PhpConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    if !matches!(basename, "composer.json" | "composer.lock") {
        return Err(ParseError::UnsupportedLanguage);
    }

    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let unit = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::PhpConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    };

    let (mut facts, dependencies) = match basename {
        "composer.json" => manifest_inventory(document.text, &unit)?,
        "composer.lock" => lock_inventory(document.text, &unit)?,
        _ => unreachable!("basename was checked above"),
    };
    facts.sort_by(|left, right| {
        (
            left.target.as_ref().map(SymbolId::as_str),
            left.assumptions.as_slice(),
            left.evidence.note.as_str(),
        )
            .cmp(&(
                right.target.as_ref().map(SymbolId::as_str),
                right.assumptions.as_slice(),
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

fn manifest_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let Ok(false) = has_duplicate_or_excess_members(text, COMPOSER_JSON_LIMITS) else {
        return malformed_inventory(unit, "malformed_composer_manifest");
    };
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return malformed_inventory(unit, "malformed_composer_manifest");
    };

    let mut declarations = BTreeMap::<String, (String, DependencyScope)>::new();
    let mut conflicting_names = BTreeSet::new();
    let mut invalid = false;
    let mut unsupported = root
        .keys()
        .any(|key| matches!(key.as_str(), "provide" | "replace" | "conflict"));
    let mut declaration_count = 0usize;

    for (field, scope) in [
        ("require", DependencyScope::Runtime),
        ("require-dev", DependencyScope::Development),
    ] {
        let Some(value) = root.get(field) else {
            continue;
        };
        let serde_json::Value::Object(requirements) = value else {
            invalid = true;
            continue;
        };
        declaration_count = declaration_count.saturating_add(requirements.len());
        for (name, value) in requirements {
            if is_composer_platform_package(name) {
                unsupported = true;
                continue;
            }
            let Some(constraint) = value.as_str() else {
                invalid = true;
                continue;
            };
            if !is_composer_package_name(name) || !is_composer_constraint(constraint) {
                invalid = true;
                continue;
            }
            if conflicting_names.contains(name) {
                continue;
            }
            if declarations.contains_key(name) {
                declarations.remove(name);
                conflicting_names.insert(name.clone());
                continue;
            }
            declarations.insert(name.clone(), (constraint.to_string(), scope));
        }
    }

    if declaration_count > COMPOSER_DEPENDENCY_LIMIT {
        return Ok((
            vec![dependency_unknown(
                unit,
                "ResourceLimit",
                "composer_dependency_limit",
                "Composer manifest dependency inventory exceeded the bounded record limit",
            )?],
            Vec::new(),
        ));
    }

    let mut dependencies = Vec::with_capacity(declarations.len());
    for (name, (constraint, scope)) in declarations {
        dependencies.push(dependency_record(
            unit,
            DependencyRecordInput {
                name: &name,
                requirement: Some(&constraint),
                resolved_version: None,
                scope,
                directness: DependencyDirectness::Direct,
                evidence_level: DependencyEvidenceLevel::ManifestDeclared,
                note: "bounded static composer.json direct requirement; no execution or runtime resolution",
            },
        )?);
    }
    let mut facts = Vec::new();
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_composer_manifest_inventory",
            "Composer requirements with unsupported shapes, names, or constraints were omitted",
        )?);
    }
    if unsupported {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_composer_platform_or_virtual_packages",
            "Composer platform packages or virtual dependency relations were not inventoried",
        )?);
    }
    if !conflicting_names.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_composer_requirements",
            "Composer package names declared in conflicting direct scopes were omitted",
        )?);
    }
    Ok((facts, dependencies))
}

fn lock_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let Ok(false) = has_duplicate_or_excess_members(text, COMPOSER_JSON_LIMITS) else {
        return malformed_inventory(unit, "malformed_composer_lock");
    };
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return malformed_inventory(unit, "malformed_composer_lock");
    };
    if !root.contains_key("packages") && !root.contains_key("packages-dev") {
        return malformed_inventory(unit, "unsupported_composer_lock_schema");
    }

    let mut packages = BTreeMap::<String, (String, DependencyScope)>::new();
    let mut conflicting_names = BTreeSet::new();
    let mut invalid = false;
    let mut unsupported = false;
    let mut entry_count = 0usize;
    for (field, scope) in [
        ("packages", DependencyScope::Runtime),
        ("packages-dev", DependencyScope::Development),
    ] {
        let Some(value) = root.get(field) else {
            continue;
        };
        let serde_json::Value::Array(entries) = value else {
            invalid = true;
            continue;
        };
        entry_count = entry_count.saturating_add(entries.len());
        for entry in entries {
            let serde_json::Value::Object(package) = entry else {
                invalid = true;
                continue;
            };
            if package
                .keys()
                .any(|key| matches!(key.as_str(), "provide" | "replace" | "conflict"))
            {
                unsupported = true;
            }
            let (Some(name), Some(version)) = (
                package.get("name").and_then(serde_json::Value::as_str),
                package.get("version").and_then(serde_json::Value::as_str),
            ) else {
                invalid = true;
                continue;
            };
            if is_composer_platform_package(name) {
                unsupported = true;
                continue;
            }
            if !is_composer_package_name(name) || !is_composer_lock_version(version) {
                invalid = true;
                continue;
            }
            if conflicting_names.contains(name) {
                continue;
            }
            if packages.contains_key(name) {
                packages.remove(name);
                conflicting_names.insert(name.to_string());
                continue;
            }
            packages.insert(name.to_string(), (version.to_string(), scope));
        }
    }

    if entry_count > COMPOSER_DEPENDENCY_LIMIT {
        return Ok((
            vec![dependency_unknown(
                unit,
                "ResourceLimit",
                "composer_lock_dependency_limit",
                "Composer lock dependency inventory exceeded the bounded record limit",
            )?],
            Vec::new(),
        ));
    }

    let mut dependencies = Vec::with_capacity(packages.len());
    for (name, (version, scope)) in packages {
        dependencies.push(dependency_record(
            unit,
            DependencyRecordInput {
                name: &name,
                requirement: None,
                resolved_version: Some(&version),
                scope,
                directness: DependencyDirectness::Unknown,
                evidence_level: DependencyEvidenceLevel::LockfileResolved,
                note: "bounded static composer.lock package entry; directness, installation, and runtime selection remain unknown",
            },
        )?);
    }
    let mut facts = vec![dependency_unknown(
        unit,
        UnknownReasonCode::InsufficientSupport.as_protocol_str(),
        "unverified_composer_lock_coherence",
        "Composer lock entries were inventoried without proving manifest coherence, root directness, installation, or runtime selection",
    )?];
    if invalid {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "partial_composer_lock_inventory",
            "Composer lock entries with unsupported shapes, names, or versions were omitted",
        )?);
    }
    if unsupported {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "unsupported_composer_lock_platform_or_virtual_packages",
            "Composer lock platform packages or virtual dependency relations were not inventoried",
        )?);
    }
    if !conflicting_names.is_empty() {
        facts.push(dependency_unknown(
            unit,
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "conflicting_composer_lock_entries",
            "Composer lock package names repeated across dependency scopes were omitted",
        )?);
    }
    Ok((facts, dependencies))
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
            "Composer dependency inventory is unavailable because the JSON document is malformed, duplicated, unsupported, or exceeds JSON bounds",
        )?],
        Vec::new(),
    ))
}

struct DependencyRecordInput<'a> {
    name: &'a str,
    requirement: Option<&'a str>,
    resolved_version: Option<&'a str>,
    scope: DependencyScope,
    directness: DependencyDirectness,
    evidence_level: DependencyEvidenceLevel,
    note: &'a str,
}

fn dependency_record(
    unit: &CodeUnit,
    input: DependencyRecordInput<'_>,
) -> Result<DependencyRecord, ParseError> {
    DependencyRecord::new(
        PackageIdentity::new(DependencyEcosystem::Composer, input.name)
            .map_err(ParseError::Internal)?,
        input
            .requirement
            .map(DependencyVersion::new)
            .transpose()
            .map_err(ParseError::Internal)?,
        input
            .resolved_version
            .map(DependencyVersion::new)
            .transpose()
            .map_err(ParseError::Internal)?,
        input.scope,
        false,
        input.directness,
        input.evidence_level,
        Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            input.note,
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
            engine: PHP_CONFIG_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: PHP_CONFIG_METHOD.to_string(),
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
            "affected_claim=php_dependency_inventory".to_string(),
            format!("php_unknown_kind={kind}"),
        ],
    })
}

fn is_composer_platform_package(name: &str) -> bool {
    !name.contains('/')
        && (matches!(
            name,
            "php" | "hhvm" | "composer" | "composer-plugin-api" | "composer-runtime-api"
        ) || name.starts_with("ext-")
            || name.starts_with("php-")
            || name.starts_with("lib-"))
}

fn is_composer_package_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > COMPOSER_PACKAGE_NAME_LIMIT
        || !name.is_ascii()
        || name.matches('/').count() != 1
    {
        return false;
    }
    let Some((vendor, package)) = name.split_once('/') else {
        return false;
    };
    is_name_half(vendor, false) && is_name_half(package, true)
}

fn is_name_half(value: &str, allow_double_hyphen: bool) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !is_lower_alphanumeric(bytes[0]) {
        return false;
    }
    let mut cursor = 1usize;
    while cursor < bytes.len() {
        if is_lower_alphanumeric(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let separator = bytes[cursor];
        if !matches!(separator, b'.' | b'_' | b'-') {
            return false;
        }
        cursor += 1;
        if allow_double_hyphen
            && separator == b'-'
            && bytes.get(cursor).is_some_and(|byte| *byte == b'-')
        {
            cursor += 1;
        }
        if !bytes
            .get(cursor)
            .is_some_and(|byte| is_lower_alphanumeric(*byte))
        {
            return false;
        }
    }
    true
}

fn is_lower_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn is_composer_constraint(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= COMPOSER_VERSION_LIMIT
        && value.trim() == value
        && value.is_ascii()
        && composer_dev_branch_slashes_are_safe(value)
        && value
            .bytes()
            .any(|byte| byte.is_ascii_alphanumeric() || byte == b'*')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b' '
                || matches!(
                    byte,
                    b'.' | b'_'
                        | b'-'
                        | b'+'
                        | b'*'
                        | b'!'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'~'
                        | b'^'
                        | b'|'
                        | b','
                        | b'&'
                        | b'@'
                        | b'#'
                        | b'/'
                        | b'('
                        | b')'
                )
        })
}

fn is_composer_lock_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= COMPOSER_VERSION_LIMIT
        && value.trim() == value
        && value.is_ascii()
        && composer_dev_branch_slashes_are_safe(value)
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+' | b'#' | b'/')
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

/// Composer stores branch requirements and locked development versions as
/// bounded opaque text such as `dev-feature/foo`. A slash is admitted only
/// inside a `dev-` token, which preserves valid VCS branch names without
/// admitting URL, absolute-path, or relative-path shapes.
fn composer_dev_branch_slashes_are_safe(value: &str) -> bool {
    for (slash, _) in value.match_indices('/') {
        let token_start = value[..slash]
            .rfind(|character: char| {
                character.is_ascii_whitespace()
                    || matches!(
                        character,
                        '|' | ',' | '&' | '(' | ')' | '=' | '<' | '>' | '~' | '^' | '!'
                    )
            })
            .map_or(0, |index| index + 1);
        let token_prefix = &value[token_start..slash];
        if !token_prefix.starts_with("dev-") {
            return false;
        }
        let Some(next) = value.as_bytes().get(slash + 1).copied() else {
            return false;
        };
        if !next.is_ascii_alphanumeric() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse(text: &str, path: &str) -> SourceParseOutput {
        parse_output(SourceDocument {
            path,
            language: Language::PhpConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        })
        .expect("parse Composer config")
    }

    fn unknowns(output: &SourceParseOutput) -> Vec<(String, String)> {
        output
            .report
            .semantic_facts
            .iter()
            .map(|fact| {
                (
                    fact.target.as_ref().expect("reason").as_str().to_string(),
                    fact.assumptions
                        .iter()
                        .find_map(|item| item.strip_prefix("php_unknown_kind="))
                        .expect("typed PHP unknown")
                        .to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn manifest_emits_bounded_direct_runtime_and_development_records() {
        let output = parse(
            r#"{"require":{"symfony/console":"^7.2","acme/pkg":"dev-main as 1.0.x-dev"},"require-dev":{"phpunit/phpunit":"~11.0"}}"#,
            "nested/composer.json",
        );

        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.package.name.as_str(),
                    dependency
                        .requirement
                        .as_ref()
                        .map(DependencyVersion::as_str),
                    dependency.scope,
                    dependency.directness,
                    dependency.evidence_level,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "acme/pkg",
                    Some("dev-main as 1.0.x-dev"),
                    DependencyScope::Runtime,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "phpunit/phpunit",
                    Some("~11.0"),
                    DependencyScope::Development,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
                (
                    "symfony/console",
                    Some("^7.2"),
                    DependencyScope::Runtime,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared
                ),
            ]
        );
        assert!(unknowns(&output).is_empty());
    }

    #[test]
    fn manifest_omits_invalid_platform_virtual_and_conflicting_entries() {
        let output = parse(
            r#"{"require":{"php":">=8.2","Bad/Name":"^1","acme/conflict":"^1","acme/url":"https://example.test/pkg"},"require-dev":{"acme/conflict":"^2","ok/tool":"1.*"},"provide":{"virtual/api":"1"}}"#,
            "composer.json",
        );

        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| dependency.package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ok/tool"]
        );
        let unknowns = unknowns(&output);
        assert!(unknowns.contains(&(
            "MissingProjectConfig".to_string(),
            "partial_composer_manifest_inventory".to_string()
        )));
        assert!(unknowns.contains(&(
            "InsufficientSupport".to_string(),
            "unsupported_composer_platform_or_virtual_packages".to_string()
        )));
        assert!(unknowns.contains(&(
            "ConflictingFacts".to_string(),
            "conflicting_composer_requirements".to_string()
        )));
    }

    #[test]
    fn malformed_and_duplicate_json_fail_closed() {
        for text in [
            "{not-json",
            r#"{"require":{"a/pkg":"1"},"require":{"b/pkg":"2"}}"#,
            r#"{"require":{"a/pkg":"1","a\u002fpkg":"2"}}"#,
        ] {
            let output = parse(text, "composer.json");
            assert!(output.dependencies.is_empty());
            assert_eq!(
                unknowns(&output),
                vec![(
                    "MissingProjectConfig".to_string(),
                    "malformed_composer_manifest".to_string()
                )]
            );
        }

        let duplicate_lock = parse(r#"{"packages":[],"pack\u0061ges":[]}"#, "composer.lock");
        assert!(duplicate_lock.dependencies.is_empty());
        assert_eq!(
            unknowns(&duplicate_lock),
            vec![(
                "MissingProjectConfig".to_string(),
                "malformed_composer_lock".to_string()
            )]
        );
    }

    #[test]
    fn manifest_record_limit_is_exact_and_fail_closed() {
        let at_limit = (0..COMPOSER_DEPENDENCY_LIMIT)
            .map(|index| format!(r#""v{index}/p":"1""#))
            .collect::<Vec<_>>()
            .join(",");
        let output = parse(&format!(r#"{{"require":{{{at_limit}}}}}"#), "composer.json");
        assert_eq!(output.dependencies.len(), COMPOSER_DEPENDENCY_LIMIT);

        let over_limit = format!(r#"{at_limit},"overflow/pkg":"1""#);
        let output = parse(
            &format!(r#"{{"require":{{{over_limit}}}}}"#),
            "composer.json",
        );
        assert!(output.dependencies.is_empty());
        assert_eq!(
            unknowns(&output),
            vec![(
                "ResourceLimit".to_string(),
                "composer_dependency_limit".to_string()
            )]
        );
    }

    #[test]
    fn lock_entries_have_unknown_directness_and_static_resolution_evidence_only() {
        let output = parse(
            r#"{"content-hash":"not-interpreted","packages":[{"name":"symfony/console","version":"v7.2.1"}],"packages-dev":[{"name":"phpunit/phpunit","version":"11.5.0"}]}"#,
            "composer.lock",
        );

        assert_eq!(output.dependencies.len(), 2);
        assert!(output.dependencies.iter().all(|dependency| {
            dependency.directness == DependencyDirectness::Unknown
                && dependency.requirement.is_none()
                && dependency.evidence_level == DependencyEvidenceLevel::LockfileResolved
                && dependency
                    .evidence
                    .note
                    .contains("directness, installation, and runtime selection remain unknown")
        }));
        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| (dependency.package.name.as_str(), dependency.scope))
                .collect::<Vec<_>>(),
            vec![
                ("phpunit/phpunit", DependencyScope::Development),
                ("symfony/console", DependencyScope::Runtime),
            ]
        );
        assert_eq!(
            unknowns(&output),
            vec![(
                "InsufficientSupport".to_string(),
                "unverified_composer_lock_coherence".to_string()
            )]
        );
    }

    #[test]
    fn lock_record_limit_is_exact_and_fail_closed() {
        let at_limit = (0..COMPOSER_DEPENDENCY_LIMIT)
            .map(|index| format!(r#"{{"name":"v{index}/p","version":"1.0.0"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let output = parse(
            &format!(r#"{{"packages":[{at_limit}],"packages-dev":[]}}"#),
            "composer.lock",
        );
        assert_eq!(output.dependencies.len(), COMPOSER_DEPENDENCY_LIMIT);

        let over_limit = format!(r#"{at_limit},{{"name":"overflow/pkg","version":"1.0.0"}}"#);
        let output = parse(
            &format!(r#"{{"packages":[{over_limit}],"packages-dev":[]}}"#),
            "composer.lock",
        );
        assert!(output.dependencies.is_empty());
        assert_eq!(
            unknowns(&output),
            vec![(
                "ResourceLimit".to_string(),
                "composer_lock_dependency_limit".to_string()
            )]
        );
    }

    #[test]
    fn package_and_version_text_bounds_are_inclusive() {
        let vendor = "v".repeat(126);
        let package = "p".repeat(128);
        let maximum_name = format!("{vendor}/{package}");
        assert_eq!(maximum_name.len(), COMPOSER_PACKAGE_NAME_LIMIT);
        assert!(is_composer_package_name(&maximum_name));
        assert!(!is_composer_package_name(&format!("{maximum_name}x")));

        let maximum_version = "1".repeat(COMPOSER_VERSION_LIMIT);
        assert!(is_composer_constraint(&maximum_version));
        assert!(is_composer_lock_version(&maximum_version));
        let oversized_version = format!("{maximum_version}1");
        assert!(!is_composer_constraint(&oversized_version));
        assert!(!is_composer_lock_version(&oversized_version));

        assert!(is_composer_platform_package("php-64bit"));
        assert!(is_composer_platform_package("ext-json"));
        assert!(!is_composer_platform_package("php-http/client-common"));
        assert!(is_composer_package_name("php-http/client-common"));
    }

    #[test]
    fn development_branch_slashes_are_bounded_without_admitting_paths_or_urls() {
        for value in [
            "dev-feature/foo",
            "dev-feature/foo/bar#abcdef",
            "dev-feature/foo as 1.0.x-dev",
            "(dev-feature/foo || ^2.0)",
        ] {
            assert!(is_composer_constraint(value), "manifest value: {value}");
        }
        for value in ["dev-feature/foo", "dev-feature/foo/bar#abcdef"] {
            assert!(is_composer_lock_version(value), "lock value: {value}");
        }
        for value in [
            "https://example.test/pkg",
            "../private/pkg",
            "./private/pkg",
            "/private/pkg",
            "feature/foo",
            "dev-feature//foo",
            "dev-feature/../foo",
        ] {
            assert!(!is_composer_constraint(value), "unsafe value: {value}");
            assert!(
                !is_composer_lock_version(value),
                "unsafe lock value: {value}"
            );
        }

        let manifest = parse(
            r#"{"require":{"acme/branch":"dev-feature/foo"}}"#,
            "composer.json",
        );
        assert_eq!(
            manifest.dependencies[0]
                .requirement
                .as_ref()
                .map(DependencyVersion::as_str),
            Some("dev-feature/foo")
        );
        let lock = parse(
            r#"{"packages":[{"name":"acme/branch","version":"dev-feature/foo"}],"packages-dev":[]}"#,
            "composer.lock",
        );
        assert_eq!(
            lock.dependencies[0]
                .resolved_version
                .as_ref()
                .map(DependencyVersion::as_str),
            Some("dev-feature/foo")
        );
    }

    #[test]
    fn lock_invalid_virtual_conflicting_and_schema_inputs_are_typed_unknown() {
        let output = parse(
            r#"{"packages":[{"name":"a/pkg","version":"1.0.0","provide":{"virtual/api":"1"}},{"name":"a/pkg","version":"2.0.0"},{"name":"ext-json","version":"8.3"},{"name":"bad/pkg","version":"https://bad"},42],"packages-dev":{}}"#,
            "composer.lock",
        );
        assert!(output.dependencies.is_empty());
        let reported_unknowns = unknowns(&output);
        assert!(reported_unknowns
            .iter()
            .any(|(reason, _)| reason == "ConflictingFacts"));
        assert!(reported_unknowns
            .iter()
            .any(|(reason, _)| reason == "MissingProjectConfig"));
        assert!(reported_unknowns.iter().any(|(reason, kind)| {
            reason == "InsufficientSupport"
                && kind == "unsupported_composer_lock_platform_or_virtual_packages"
        }));

        let unsupported = parse(r#"{"content-hash":"abc"}"#, "composer.lock");
        assert!(unsupported.dependencies.is_empty());
        assert_eq!(
            unknowns(&unsupported),
            vec![(
                "MissingProjectConfig".to_string(),
                "unsupported_composer_lock_schema".to_string()
            )]
        );
    }

    #[test]
    fn parser_rejects_php_source_and_phpunit_config() {
        let parser = PhpConfigParser;
        for (path, language) in [
            ("main.php", Language::Php),
            ("phpunit.xml", Language::PhpConfig),
        ] {
            assert!(matches!(
                parser.parse(SourceDocument {
                    path,
                    language,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("valid hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN")
                        .expect("valid revision"),
                    text: "must not be parsed",
                }),
                Err(ParseError::UnsupportedLanguage)
            ));
        }
    }
}
