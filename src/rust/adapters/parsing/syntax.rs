//! Dependency-free syntax-only TS/JS code-unit extraction.
//!
//! This adapter is a bootstrap parser boundary. It emits structural code-unit
//! candidates and diagnostics only; it does not provide semantic certainty.

use super::{
    ir_edges_for_units, ir_nodes_for_units,
    tsjs::{TsJsAnalysis, TSJS_ANCHOR_ENGINE},
};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencyVersion, Evidence,
    FactCertainty, FactOrigin, Language, PackageIdentity, Provenance, SemanticFact,
    SemanticFactKind, SourceRange, SymbolId,
};
use crate::core::policy::paths::validate_repo_relative_path;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct SyntaxCodeUnitParser;

impl SourceParser for SyntaxCodeUnitParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        if document.language == Language::TsJsConfig {
            return tsjs_project_config_output(document).map(|output| output.report);
        }
        if !matches!(
            document.language,
            Language::TypeScript | Language::JavaScript
        ) {
            return Err(ParseError::UnsupportedLanguage);
        }
        let mut scanner = SyntaxScanner::new(document);
        scanner.scan()?;
        scanner.finish()
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        if document.language == Language::TsJsConfig {
            return tsjs_project_config_output(document).map(|output| output.report);
        }
        if !matches!(
            document.language,
            Language::TypeScript | Language::JavaScript
        ) {
            return Err(ParseError::UnsupportedLanguage);
        }
        let mut scanner = SyntaxScanner::new_with_context(document, context);
        scanner.scan()?;
        scanner.finish()
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        if document.language == Language::TsJsConfig {
            return tsjs_project_config_output(document);
        }
        self.parse_with_context(document, context)
            .map(SourceParseOutput::from_report)
    }
}

fn tsjs_project_config_output(
    document: SourceDocument<'_>,
) -> Result<SourceParseOutput, ParseError> {
    let unit = super::project_config_unit(&document, Language::TsJsConfig)?;
    let mut semantic_facts = Vec::new();
    let mut dependencies = Vec::new();
    let mut diagnostics = Vec::new();
    if document.path.ends_with(".json") {
        match serde_json::from_str::<Value>(document.text) {
            Ok(value) => {
                semantic_facts.extend(tsjs_json_project_config_facts(&document, &unit, &value)?);
                dependencies.extend(tsjs_json_dependencies(&document, &unit, &value)?);
            }
            Err(_) => {
                semantic_facts.push(tsjs_project_config_unknown_fact(
                    &document,
                    &unit,
                    "BuildVariantAmbiguity",
                    "tsjs_project_config",
                    "invalid_json_project_config",
                    "bounded JSON project config could not be parsed",
                )?);
                diagnostics.push(ParseDiagnostic {
                    path: document.path.to_string(),
                    range: None,
                    severity: ParseDiagnosticSeverity::Warning,
                    message: "TS/JS project config JSON could not be parsed".to_string(),
                });
            }
        }
    } else {
        semantic_facts.push(tsjs_project_config_fact(
            &document,
            &unit,
            "tsjs.config.metadata",
            vec![
                "tsjs_project_config=script_config".to_string(),
                "config_execution=disabled".to_string(),
            ],
            "non-executing TS/JS project config metadata",
        )?);
        semantic_facts.push(tsjs_project_config_unknown_fact(
            &document,
            &unit,
            "BuildVariantAmbiguity",
            "tsjs_project_config",
            script_config_unknown_kind(document.path),
            "script project config was not executed",
        )?);
    }
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts,
            diagnostics,
        },
        python_interface_hash: None,
        dependencies,
    })
}

fn tsjs_json_project_config_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    value: &Value,
) -> Result<Vec<SemanticFact>, ParseError> {
    let mut facts = Vec::new();
    let object = value.as_object();
    facts.push(tsjs_project_config_fact(
        document,
        unit,
        match document.path {
            "package.json" => "tsjs.package_json",
            "tsconfig.json" => "tsjs.tsconfig",
            "jsconfig.json" => "tsjs.jsconfig",
            "jest.config.json" => "tsjs.jest_config",
            "vitest.config.json" => "tsjs.vitest_config",
            _ => "tsjs.config.metadata",
        },
        vec![json_config_assumption(document.path).to_string()],
        "bounded TS/JS project config metadata",
    )?);
    if document.path == "package.json" {
        if let Some(object) = object {
            for field in [
                "dependencies",
                "devDependencies",
                "optionalDependencies",
                "peerDependencies",
            ] {
                if let Some(dependencies) = object.get(field).and_then(Value::as_object) {
                    for package in dependencies.keys() {
                        facts.push(tsjs_project_config_fact(
                            document,
                            unit,
                            &format!("package:{package}"),
                            vec![
                                json_config_assumption(document.path).to_string(),
                                format!("dependency_field={field}"),
                            ],
                            "bounded package dependency metadata",
                        )?);
                    }
                }
            }
        }
    }
    if matches!(document.path, "tsconfig.json" | "jsconfig.json") {
        if let Some(compiler_options) = object
            .and_then(|object| object.get("compilerOptions"))
            .and_then(Value::as_object)
        {
            if let Some(paths) = compiler_options.get("paths").and_then(Value::as_object) {
                for alias in paths.keys() {
                    facts.push(tsjs_project_config_fact(
                        document,
                        unit,
                        &format!("tsconfig.path_alias:{alias}"),
                        vec![
                            json_config_assumption(document.path).to_string(),
                            "project_config=path_alias".to_string(),
                        ],
                        "bounded path alias metadata",
                    )?);
                }
            }
            if let Some(root_dirs) = compiler_options.get("rootDirs").and_then(Value::as_array) {
                for root_dir in root_dirs
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(tsjs_project_config_root_dir)
                {
                    facts.push(tsjs_project_config_fact(
                        document,
                        unit,
                        &format!("tsconfig.root_dir:{root_dir}"),
                        vec![
                            json_config_assumption(document.path).to_string(),
                            "project_config=root_dirs".to_string(),
                        ],
                        "bounded rootDirs metadata",
                    )?);
                }
            }
            if let Some(jsx) = compiler_options.get("jsx").and_then(Value::as_str) {
                facts.push(tsjs_project_config_fact(
                    document,
                    unit,
                    &format!("tsconfig.jsx:{jsx}"),
                    vec![
                        json_config_assumption(document.path).to_string(),
                        "project_config=jsx_runtime".to_string(),
                    ],
                    "bounded JSX runtime metadata",
                )?);
            }
        }
    }
    Ok(facts)
}

fn tsjs_json_dependencies(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    value: &Value,
) -> Result<Vec<DependencyRecord>, ParseError> {
    if document.path != "package.json" {
        return Ok(Vec::new());
    }
    let Some(object) = value.as_object() else {
        return Ok(Vec::new());
    };
    let peer_meta = object
        .get("peerDependenciesMeta")
        .and_then(Value::as_object);
    let mut dependencies = Vec::new();
    for (field, scope, optional_by_field) in [
        ("dependencies", DependencyScope::Runtime, false),
        ("devDependencies", DependencyScope::Development, false),
        ("optionalDependencies", DependencyScope::Runtime, true),
        ("peerDependencies", DependencyScope::Unknown, false),
    ] {
        let Some(entries) = object.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (package_name, requirement_value) in entries {
            if !is_bounded_npm_package_name(package_name) {
                continue;
            }
            let package = PackageIdentity::new(DependencyEcosystem::Npm, package_name)
                .map_err(ParseError::Internal)?;
            let requirement = requirement_value
                .as_str()
                .and_then(|value| DependencyVersion::new(value).ok());
            let optional_by_meta = field == "peerDependencies"
                && peer_meta
                    .and_then(|meta| meta.get(package_name))
                    .and_then(Value::as_object)
                    .and_then(|entry| entry.get("optional"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            dependencies.push(
                DependencyRecord::new(
                    package,
                    requirement,
                    None,
                    scope,
                    optional_by_field || optional_by_meta,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared,
                    Evidence::new(
                        unit.id.clone(),
                        unit.range.clone(),
                        unit.provenance.clone(),
                        format!("bounded package.json {field} declaration"),
                    )
                    .map_err(ParseError::Internal)?,
                )
                .map_err(ParseError::Internal)?,
            );
        }
    }
    dependencies.sort_by(|left, right| {
        (
            left.package.name.as_str(),
            left.scope.as_str(),
            left.optional,
            left.requirement.as_ref().map(DependencyVersion::as_str),
        )
            .cmp(&(
                right.package.name.as_str(),
                right.scope.as_str(),
                right.optional,
                right.requirement.as_ref().map(DependencyVersion::as_str),
            ))
    });
    Ok(dependencies)
}

fn is_bounded_npm_package_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 214 || !value.is_ascii() {
        return false;
    }
    fn valid_component(component: &str) -> bool {
        !component.is_empty()
            && component.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.' | b'~')
            })
    }
    match value.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| valid_component(scope) && valid_component(name)),
        None => !value.contains('/') && valid_component(value),
    }
}

fn tsjs_project_config_root_dir(root_dir: &str) -> Option<String> {
    let normalized = root_dir
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/');
    if normalized.is_empty() || normalized.contains('*') || normalized.contains('?') {
        return None;
    }
    if validate_repo_relative_path(normalized).is_err() {
        return None;
    }
    Some(normalized.to_string())
}

fn json_config_assumption(path: &str) -> &'static str {
    match path {
        "package.json" => "tsjs_project_config=package_json",
        "tsconfig.json" => "tsjs_project_config=tsconfig_json",
        "jsconfig.json" => "tsjs_project_config=jsconfig_json",
        "jest.config.json" => "tsjs_project_config=jest_config_json",
        "vitest.config.json" => "tsjs_project_config=vitest_config_json",
        _ => "tsjs_project_config=json_config",
    }
}

fn script_config_unknown_kind(path: &str) -> &'static str {
    if path.starts_with("next.config.") {
        "next_config_execution_disabled"
    } else {
        "script_config_execution_disabled"
    }
}

fn tsjs_project_config_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    target: &str,
    assumptions: Vec<String>,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "bounded_project_inventory_v1".to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?,
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions,
    })
}

fn tsjs_project_config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    reason: &str,
    affected_claim: &str,
    unknown_kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "bounded_project_inventory_v1".to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?,
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={affected_claim}"),
            format!("tsjs_unknown_kind={unknown_kind}"),
        ],
    })
}

pub(super) fn tsjs_import_resolution_facts(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    context: &ParserProjectContext,
) -> Result<Vec<SemanticFact>, ParseError> {
    let Some(module_unit) = units.iter().find(|unit| unit.kind == CodeUnitKind::Module) else {
        return Ok(Vec::new());
    };
    let module_paths = context
        .tsjs_module_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut facts = Vec::new();
    let mut seen = BTreeSet::new();
    for (line_start, line) in lines_with_offsets(document.text) {
        let line_end = line_start + line.len();
        if line_contains_dynamic_import(line) {
            push_unique_import_fact(
                &mut facts,
                &mut seen,
                tsjs_source_unknown_fact(
                    document,
                    module_unit,
                    line_start,
                    line_end,
                    TsJsUnknownSpec {
                        reason: "DynamicImport",
                        affected_claim: "tsjs_import_resolution",
                        kind: "dynamic_import",
                        literal_specifier: None,
                        note: "dynamic TS/JS import expression is not resolved",
                    },
                )?,
            );
            continue;
        }
        if line_contains_conditional_require(line) {
            push_unique_import_fact(
                &mut facts,
                &mut seen,
                tsjs_source_unknown_fact(
                    document,
                    module_unit,
                    line_start,
                    line_end,
                    TsJsUnknownSpec {
                        reason: "BuildVariantAmbiguity",
                        affected_claim: "tsjs_import_resolution",
                        kind: "conditional_require",
                        literal_specifier: None,
                        note: "conditional TS/JS require call is not resolved",
                    },
                )?,
            );
            continue;
        }
        if line_contains_dynamic_require(line) {
            push_unique_import_fact(
                &mut facts,
                &mut seen,
                tsjs_source_unknown_fact(
                    document,
                    module_unit,
                    line_start,
                    line_end,
                    TsJsUnknownSpec {
                        reason: "DynamicImport",
                        affected_claim: "tsjs_import_resolution",
                        kind: "dynamic_require",
                        literal_specifier: None,
                        note: "dynamic TS/JS require call is not resolved",
                    },
                )?,
            );
            continue;
        }
        if is_export_star_line(line) {
            let literal_specifier = first_quoted_after(line.trim_start(), " from ")
                .map(|specifier| format!("{specifier}#*"));
            push_unique_import_fact(
                &mut facts,
                &mut seen,
                tsjs_source_unknown_fact(
                    document,
                    module_unit,
                    line_start,
                    line_end,
                    TsJsUnknownSpec {
                        reason: "ConflictingFacts",
                        affected_claim: "tsjs_reexport_resolution",
                        kind: "ambiguous_reexport",
                        literal_specifier: literal_specifier.as_deref(),
                        note: "star re-export is ambiguous without a semantic provider",
                    },
                )?,
            );
            continue;
        }
        for specifier in static_import_export_specifiers(line) {
            match resolve_tsjs_import_specifier(
                document.path,
                &specifier,
                &module_paths,
                &context.tsjs_path_aliases,
                &context.tsjs_root_dirs,
            ) {
                ImportResolution::Resolved {
                    path,
                    resolution_kind,
                } => push_unique_import_fact(
                    &mut facts,
                    &mut seen,
                    tsjs_resolved_import_fact(
                        document,
                        module_unit,
                        line_start,
                        line_end,
                        &specifier,
                        &path,
                        resolution_kind,
                    )?,
                ),
                ImportResolution::Unknown { reason, kind } => push_unique_import_fact(
                    &mut facts,
                    &mut seen,
                    tsjs_source_unknown_fact(
                        document,
                        module_unit,
                        line_start,
                        line_end,
                        TsJsUnknownSpec {
                            reason,
                            affected_claim: "tsjs_import_resolution",
                            kind,
                            literal_specifier: Some(&specifier),
                            note: "bounded TS/JS import resolution could not prove a unique target",
                        },
                    )?,
                ),
                ImportResolution::IgnoredExternal => {}
            }
        }
    }
    facts.sort_by(|left, right| {
        (
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
            left.subject.as_str(),
        )
            .cmp(&(
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.subject.as_str(),
            ))
    });
    Ok(facts)
}

fn push_unique_import_fact(
    facts: &mut Vec<SemanticFact>,
    seen: &mut BTreeSet<(String, String, usize, usize)>,
    fact: SemanticFact,
) {
    let target = fact
        .target
        .as_ref()
        .map(SymbolId::as_str)
        .unwrap_or("")
        .to_string();
    let key = (
        fact.kind.as_protocol_str().to_string(),
        target,
        fact.evidence.range.start_byte,
        fact.evidence.range.end_byte,
    );
    if seen.insert(key) {
        facts.push(fact);
    }
}

fn tsjs_resolved_import_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    literal_specifier: &str,
    target_path: &str,
    resolution_kind: &'static str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ResolvedImport,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(format!("module:{target_path}")).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "bounded_import_resolver_v1".to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?,
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            "bounded static TS/JS import target",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("tsjs_import_resolution={resolution_kind}"),
            format!("literal_specifier={literal_specifier}"),
            "provider_resolved=false".to_string(),
        ],
    })
}

#[derive(Debug, Clone, Copy)]
struct TsJsUnknownSpec<'a> {
    reason: &'static str,
    affected_claim: &'static str,
    kind: &'static str,
    literal_specifier: Option<&'a str>,
    note: &'static str,
}

fn tsjs_source_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    start_byte: usize,
    end_byte: usize,
    spec: TsJsUnknownSpec<'_>,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(spec.reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: TSJS_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: "bounded_import_resolver_v1".to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?,
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            spec.note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: {
            let mut assumptions = vec![
                format!("affected_claim={}", spec.affected_claim),
                format!("tsjs_unknown_kind={}", spec.kind),
            ];
            if let Some(literal_specifier) = spec.literal_specifier {
                assumptions.push(format!("literal_specifier={literal_specifier}"));
            }
            assumptions
        },
    })
}

enum ImportResolution {
    Resolved {
        path: String,
        resolution_kind: &'static str,
    },
    Unknown {
        reason: &'static str,
        kind: &'static str,
    },
    IgnoredExternal,
}

fn resolve_tsjs_import_specifier(
    current_path: &str,
    specifier: &str,
    module_paths: &BTreeSet<String>,
    aliases: &[crate::ports::parser::ParserTsJsPathAlias],
    root_dirs: &[String],
) -> ImportResolution {
    if specifier.starts_with("./") || specifier.starts_with("../") {
        let Some(base) = normalize_relative_specifier(current_path, specifier) else {
            return ImportResolution::Unknown {
                reason: "UnresolvedImport",
                kind: "unresolved_import",
            };
        };
        let direct = resolve_module_base(&base, module_paths, "literal_relative");
        if matches!(
            &direct,
            ImportResolution::Unknown {
                reason: "UnresolvedImport",
                kind: "unresolved_import"
            }
        ) {
            if let Some(root_dirs_resolution) =
                resolve_root_dirs_relative_import(current_path, &base, root_dirs, module_paths)
            {
                return root_dirs_resolution;
            }
        }
        return direct;
    }
    let mut matched_alias = false;
    let mut matches = BTreeSet::new();
    for alias in aliases {
        let Some(replacements) = alias_replacements(specifier, &alias.alias_pattern) else {
            continue;
        };
        matched_alias = true;
        for replacement in replacements {
            for target_pattern in &alias.target_patterns {
                let candidate = apply_alias_target(target_pattern, &replacement);
                match resolve_module_base(&candidate, module_paths, "path_alias") {
                    ImportResolution::Resolved { path, .. } => {
                        matches.insert(path);
                    }
                    ImportResolution::Unknown { .. } | ImportResolution::IgnoredExternal => {}
                }
            }
        }
    }
    if matches.len() == 1 {
        return ImportResolution::Resolved {
            path: matches.into_iter().next().expect("one alias match"),
            resolution_kind: "path_alias",
        };
    }
    if matched_alias {
        return ImportResolution::Unknown {
            reason: if matches.is_empty() {
                "UnresolvedImport"
            } else {
                "ConflictingFacts"
            },
            kind: if matches.is_empty() {
                "unresolved_path_alias"
            } else {
                "path_alias_conflict"
            },
        };
    }
    ImportResolution::IgnoredExternal
}

fn resolve_root_dirs_relative_import(
    current_path: &str,
    base: &str,
    root_dirs: &[String],
    module_paths: &BTreeSet<String>,
) -> Option<ImportResolution> {
    let root_dir_set = root_dirs.iter().collect::<BTreeSet<_>>();
    let current_root = root_dir_set
        .iter()
        .filter(|root_dir| path_is_within_root(current_path, root_dir))
        .max_by_key(|root_dir| root_dir.len())?;
    let suffix = path_suffix_within_root(base, current_root)?;
    if suffix.is_empty() {
        return None;
    }

    let mut matches = BTreeSet::new();
    let mut saw_conflict = false;
    for root_dir in root_dir_set {
        let candidate = format!("{root_dir}/{suffix}");
        match resolve_module_base(&candidate, module_paths, "root_dirs") {
            ImportResolution::Resolved { path, .. } => {
                matches.insert(path);
            }
            ImportResolution::Unknown {
                reason: "ConflictingFacts",
                ..
            } => saw_conflict = true,
            ImportResolution::Unknown { .. } | ImportResolution::IgnoredExternal => {}
        }
    }

    if saw_conflict || matches.len() > 1 {
        return Some(ImportResolution::Unknown {
            reason: "ConflictingFacts",
            kind: "root_dirs_conflict",
        });
    }
    if let Some(path) = matches.into_iter().next() {
        return Some(ImportResolution::Resolved {
            path,
            resolution_kind: "root_dirs",
        });
    }
    Some(ImportResolution::Unknown {
        reason: "UnresolvedImport",
        kind: "unresolved_root_dirs",
    })
}

fn path_is_within_root(path: &str, root_dir: &str) -> bool {
    path.strip_prefix(root_dir)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn path_suffix_within_root<'a>(path: &'a str, root_dir: &str) -> Option<&'a str> {
    path.strip_prefix(root_dir)?.strip_prefix('/')
}

fn resolve_module_base(
    base: &str,
    module_paths: &BTreeSet<String>,
    resolution_kind: &'static str,
) -> ImportResolution {
    let mut matches = BTreeSet::new();
    for candidate in module_path_candidates(base) {
        if module_paths.contains(&candidate) {
            matches.insert(candidate);
        }
    }
    match matches.len() {
        0 => ImportResolution::Unknown {
            reason: "UnresolvedImport",
            kind: "unresolved_import",
        },
        1 => ImportResolution::Resolved {
            path: matches.into_iter().next().expect("one module match"),
            resolution_kind,
        },
        _ => ImportResolution::Unknown {
            reason: "ConflictingFacts",
            kind: "ambiguous_import",
        },
    }
}

fn module_path_candidates(base: &str) -> Vec<String> {
    let has_extension = [".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|extension| base.ends_with(extension));
    if has_extension {
        return vec![base.to_string()];
    }
    let mut candidates = Vec::new();
    for extension in [".ts", ".tsx", ".js", ".jsx"] {
        candidates.push(format!("{base}{extension}"));
    }
    for extension in [".ts", ".tsx", ".js", ".jsx"] {
        candidates.push(format!("{base}/index{extension}"));
    }
    candidates
}

fn normalize_relative_specifier(current_path: &str, specifier: &str) -> Option<String> {
    let mut parts = current_path
        .rsplit_once('/')
        .map(|(directory, _)| directory.split('/').collect::<Vec<_>>())
        .unwrap_or_default();
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value => parts.push(value),
        }
    }
    Some(parts.join("/"))
}

fn alias_replacements(specifier: &str, alias_pattern: &str) -> Option<Vec<String>> {
    if !alias_pattern.contains('*') {
        return (specifier == alias_pattern).then(|| vec![String::new()]);
    }
    let mut segments = alias_pattern.split('*');
    let prefix = segments.next().unwrap_or("");
    let suffix = segments.next().unwrap_or("");
    if segments.next().is_some() {
        return None;
    }
    if !specifier.starts_with(prefix) || !specifier.ends_with(suffix) {
        return None;
    }
    let replacement = &specifier[prefix.len()..specifier.len() - suffix.len()];
    Some(vec![replacement.to_string()])
}

fn apply_alias_target(target_pattern: &str, replacement: &str) -> String {
    if target_pattern.contains('*') {
        target_pattern.replace('*', replacement)
    } else {
        target_pattern.to_string()
    }
}

fn static_import_export_specifiers(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    let mut specifiers = Vec::new();
    if trimmed.starts_with("import ") {
        if let Some(specifier) = first_quoted_after(trimmed, " from ") {
            specifiers.push(specifier);
        } else if let Some(specifier) = first_quoted_after(trimmed, "import ") {
            specifiers.push(specifier);
        }
    }
    if trimmed.starts_with("export ") && !trimmed.starts_with("export *") {
        if let Some(specifier) = first_quoted_after(trimmed, " from ") {
            specifiers.push(specifier);
        }
    }
    for specifier in quoted_require_specifiers(trimmed) {
        specifiers.push(specifier);
    }
    specifiers
}

pub(super) fn first_quoted_after(line: &str, marker: &str) -> Option<String> {
    let index = line.find(marker)? + marker.len();
    first_quoted(&line[index..])
}

pub(super) fn first_quoted(text: &str) -> Option<String> {
    let quote_index = text.find(['"', '\''])?;
    let quote = text.as_bytes()[quote_index] as char;
    let rest = &text[quote_index + 1..];
    // Honor backslash escaping so a specifier like `a\"b` is not truncated at
    // the escaped quote (`a\`).
    let mut escaped = false;
    for (index, character) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            _ if character == quote => return Some(rest[..index].to_string()),
            _ => {}
        }
    }
    None
}

fn quoted_require_specifiers(line: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut rest = line;
    while let Some(index) = rest.find("require(") {
        let after = &rest[index + "require(".len()..];
        if let Some(specifier) = first_quoted(after) {
            output.push(specifier);
        }
        rest = &after[after
            .find(')')
            .map(|offset| offset + 1)
            .unwrap_or(after.len())..];
    }
    output
}

fn line_contains_dynamic_import(line: &str) -> bool {
    line.contains("import(")
}

fn line_contains_dynamic_require(line: &str) -> bool {
    let mut rest = line;
    while let Some(index) = rest.find("require(") {
        let after = rest[index + "require(".len()..].trim_start();
        if !after.starts_with('"') && !after.starts_with('\'') {
            return true;
        }
        rest = &after[after
            .find(')')
            .map(|offset| offset + 1)
            .unwrap_or(after.len())..];
    }
    false
}

fn line_contains_conditional_require(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.contains("require(")
        && (trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.contains(" ? require("))
}

fn is_export_star_line(line: &str) -> bool {
    line.trim_start().starts_with("export *")
}

struct SyntaxScanner<'a> {
    document: SourceDocument<'a>,
    context: Option<&'a ParserProjectContext>,
    units: Vec<CodeUnit>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
    analysis: Option<TsJsAnalysis>,
}

impl<'a> SyntaxScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        Self::with_context(document, None)
    }

    fn new_with_context(document: SourceDocument<'a>, context: &'a ParserProjectContext) -> Self {
        Self::with_context(document, Some(context))
    }

    fn with_context(
        document: SourceDocument<'a>,
        context: Option<&'a ParserProjectContext>,
    ) -> Self {
        Self {
            document,
            context,
            units: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
            analysis: None,
        }
    }

    /// Reads the units from the file's one Tree-sitter tree (ADR-0059 slice 3);
    /// the anchor pass in `finish` reads the same analysis.
    fn scan(&mut self) -> Result<(), ParseError> {
        self.add_unit(CodeUnitKind::Module, "module", 0, self.document.text.len())?;
        let analysis = TsJsAnalysis::analyze(&self.document);
        for unit in analysis.units(&self.document) {
            self.add_unit(unit.kind, &unit.name, unit.start, unit.end)?;
        }
        self.analysis = Some(analysis);
        self.add_delimiter_diagnostic_if_needed();
        Ok(())
    }

    fn finish(mut self) -> Result<ParseReport, ParseError> {
        self.units.sort_by(|left, right| {
            (
                left.range.start_byte,
                left.range.end_byte,
                left.kind.as_str(),
                left.id.as_str(),
            )
                .cmp(&(
                    right.range.start_byte,
                    right.range.end_byte,
                    right.kind.as_str(),
                    right.id.as_str(),
                ))
        });
        let ir_nodes = ir_nodes_for_units(&self.units).map_err(ParseError::Internal)?;
        let ir_edges = ir_edges_for_units(&self.units).map_err(ParseError::Internal)?;
        let mut semantic_facts = match self.analysis.take() {
            Some(analysis) => analysis.anchors(&self.document, &self.units, self.context)?,
            None => Vec::new(),
        };
        if let Some(context) = self.context {
            semantic_facts.extend(tsjs_import_resolution_facts(
                &self.document,
                &self.units,
                context,
            )?);
        }
        Ok(ParseReport {
            units: self.units,
            ir_nodes,
            ir_edges,
            semantic_facts,
            diagnostics: self.diagnostics,
        })
    }

    fn add_delimiter_diagnostic_if_needed(&mut self) {
        if delimiters_are_balanced(self.document.text) {
            return;
        }
        self.diagnostics.push(ParseDiagnostic {
            path: self.document.path.to_string(),
            range: None,
            severity: ParseDiagnosticSeverity::Warning,
            message: "source has unbalanced delimiters; syntax-only extraction may be partial"
                .to_string(),
        });
    }

    fn add_unit(
        &mut self,
        kind: CodeUnitKind,
        name: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<(), ParseError> {
        let range = SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?;
        let provenance = Provenance::new(
            self.document.path,
            self.document.content_hash.clone(),
            self.document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?;
        let id = CodeUnitId::new(format!(
            "unit:{}#{}:{}:{}-{}:{}",
            self.document.path,
            kind.as_str(),
            slug(name),
            start_byte,
            end_byte,
            self.ordinal
        ))
        .map_err(ParseError::Internal)?;
        self.ordinal += 1;
        self.units.push(CodeUnit {
            id,
            language: self.document.language.clone(),
            kind,
            range,
            provenance,
        });
        Ok(())
    }
}

pub(super) fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        lines.push((start, line));
        start += line.len();
    }
    if text.is_empty() {
        lines.push((0, ""));
    }
    lines
}

pub(super) fn delimiters_are_balanced(text: &str) -> bool {
    let mut braces = 0isize;
    let mut parentheses = 0isize;
    let mut brackets = 0isize;
    for byte in text.bytes() {
        match byte {
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            _ => {}
        }
        if braces < 0 || parentheses < 0 || brackets < 0 {
            return false;
        }
    }
    braces == 0 && parentheses == 0 && brackets == 0
}

pub(super) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

pub(super) fn slug(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if is_identifier_byte(byte) {
            output.push(byte as char);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    output.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::super::tsjs::next_file_convention_kind;
    use super::*;
    use crate::core::model::{ContentHash, IrEdgeLabel, IrNodeId, RepositoryRevision};

    fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: if path.ends_with(".js") || path.ends_with(".jsx") {
                Language::JavaScript
            } else {
                Language::TypeScript
            },
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    #[test]
    fn declaration_after_multi_byte_text_does_not_panic() {
        // Six bytes before `class` lands inside `你`; the decorator walk used
        // to slice there and abort the whole index.
        for text in ["//你好a\nclass A {}\n", "败败a class A {}\n"] {
            let parsed = SyntaxCodeUnitParser
                .parse(document("src/a.ts", text))
                .expect("parse");
            let classes = parsed
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::Class)
                .map(|unit| unit.id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(classes.len(), 1, "{text:?}");
            assert!(classes[0].contains('A'), "{classes:?}");
        }
    }

    #[test]
    fn extracts_structural_ts_js_units_in_deterministic_order() {
        let text = r#"import express from "express";
const app = express();
app.get("/users", async (req, res) => {
  res.json([]);
});
function useUsers() {
  return [];
}
export function UserList() {
  return <section />;
}
const loadUsers = async () => {
  return [];
};
class UserService {
  async findAll() {
    return [];
  }
}
describe("users", () => {
  it("loads", () => {});
  test("filters", () => {});
});
"#;

        let first = SyntaxCodeUnitParser
            .parse(document("src/users.tsx", text))
            .expect("parse");
        let second = SyntaxCodeUnitParser
            .parse(document("src/users.tsx", text))
            .expect("parse");

        let kinds = first
            .units
            .iter()
            .map(|unit| unit.kind.as_str())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"module"));
        assert!(kinds.contains(&"express_route"));
        assert!(kinds.contains(&"react_hook"));
        assert!(kinds.contains(&"react_component"));
        assert!(kinds.contains(&"arrow_function"));
        assert!(kinds.contains(&"class"));
        assert!(kinds.contains(&"method"));
        assert!(kinds.contains(&"test_suite"));
        assert!(kinds.contains(&"test_case"));
        assert_eq!(
            first
                .units
                .iter()
                .map(|unit| unit.id.as_str().to_string())
                .collect::<Vec<_>>(),
            second
                .units
                .iter()
                .map(|unit| unit.id.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(first.ir_nodes.len(), first.units.len());
        assert_eq!(
            first
                .ir_nodes
                .iter()
                .map(|node| node.id.as_str().to_string())
                .collect::<Vec<_>>(),
            second
                .ir_nodes
                .iter()
                .map(|node| node.id.as_str().to_string())
                .collect::<Vec<_>>()
        );
        let route = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::ExpressRoute)
            .expect("route unit");
        assert_eq!(
            route.range.start_byte,
            text.find("app.get").expect("route start")
        );
        assert!(route.range.end_byte <= text.len());
        assert_eq!(route.provenance.path, "src/users.tsx");
        let module = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Module)
            .expect("module unit");
        let class = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Class)
            .expect("class unit");
        let method = first
            .units
            .iter()
            .find(|unit| unit.kind == CodeUnitKind::Method)
            .expect("method unit");
        let module_id = IrNodeId::for_code_unit(&module.id).expect("module IR id");
        let route_id = IrNodeId::for_code_unit(&route.id).expect("route IR id");
        let class_id = IrNodeId::for_code_unit(&class.id).expect("class IR id");
        let method_id = IrNodeId::for_code_unit(&method.id).expect("method IR id");
        assert!(first.ir_edges.iter().any(|edge| {
            edge.from_node_id == module_id
                && edge.to_node_id == route_id
                && edge.label == IrEdgeLabel::Contains
        }));
        assert!(first.ir_edges.iter().any(|edge| {
            edge.from_node_id == class_id
                && edge.to_node_id == method_id
                && edge.label == IrEdgeLabel::Contains
        }));
    }

    #[test]
    fn syntax_errors_return_partial_units_with_diagnostics() {
        let text = "export function broken() {\n  return 1;\n";

        let report = SyntaxCodeUnitParser
            .parse(document("src/broken.ts", text))
            .expect("parse partial");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::Module));
        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::Function));
        assert_eq!(report.ir_nodes.len(), report.units.len());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].path, "src/broken.ts");
        assert!(report.diagnostics[0].message.contains("unbalanced"));
    }

    #[test]
    fn tsjs_project_config_json_emits_metadata_facts_without_source_units() {
        let report = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "package.json",
                language: Language::TsJsConfig,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: r#"{"dependencies":{"express":"latest"},"devDependencies":{"vitest":"latest"}}"#,
            })
            .expect("parse package metadata");

        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert_eq!(report.units[0].language, Language::TsJsConfig);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "package:express")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "package:vitest")
        }));
    }

    #[test]
    fn package_json_emits_typed_dependency_inventory_with_scopes_and_requirements() {
        let output = SyntaxCodeUnitParser
            .parse_with_context_output(
                SourceDocument {
                    path: "package.json",
                    language: Language::TsJsConfig,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("valid hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN")
                        .expect("valid revision"),
                    text: r#"{
                        "dependencies":{"express":"^4"},
                        "devDependencies":{"vitest":"^3"},
                        "optionalDependencies":{"fsevents":"~2.3"},
                        "peerDependencies":{"@scope/plugin":"^1"},
                        "peerDependenciesMeta":{"@scope/plugin":{"optional":true}}
                    }"#,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse typed package inventory");

        assert_eq!(output.dependencies.len(), 4);
        let dependency = |name: &str| {
            output
                .dependencies
                .iter()
                .find(|dependency| dependency.package.name == name)
                .expect("dependency exists")
        };
        assert_eq!(
            dependency("express").package.ecosystem,
            DependencyEcosystem::Npm
        );
        assert_eq!(dependency("express").scope, DependencyScope::Runtime);
        assert_eq!(
            dependency("express")
                .requirement
                .as_ref()
                .map(DependencyVersion::as_str),
            Some("^4")
        );
        assert_eq!(dependency("vitest").scope, DependencyScope::Development);
        assert!(dependency("fsevents").optional);
        assert_eq!(dependency("@scope/plugin").scope, DependencyScope::Unknown);
        assert!(dependency("@scope/plugin").optional);
        assert!(output.dependencies.iter().all(|dependency| {
            dependency.directness == DependencyDirectness::Direct
                && dependency.evidence_level == DependencyEvidenceLevel::ManifestDeclared
                && dependency.evidence.provenance.path == "package.json"
        }));
    }

    #[test]
    fn npm_package_name_contract_is_bounded_and_conservative() {
        for accepted in ["express", "@types/node", "package_name", "a.b-c~d"] {
            assert!(is_bounded_npm_package_name(accepted), "{accepted}");
        }
        for rejected in [
            "",
            "@scope",
            "scope/name",
            "@scope/",
            "@/name",
            "BadName",
            "bad name",
            "../escape",
        ] {
            assert!(!is_bounded_npm_package_name(rejected), "{rejected}");
        }
        assert!(!is_bounded_npm_package_name(&"a".repeat(215)));
    }

    #[test]
    fn tsjs_scoped_package_dependencies_stay_structural_config_not_family_support() {
        // Scoped packages such as `@types/node` and `@prisma/client` must not
        // break JSON project-config parsing, and their mere presence in
        // package.json is PROJECT_CONFIG/STRUCTURAL inventory only — never
        // family-support evidence, because structural certainty does not prove
        // semantic family membership.
        let report = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "package.json",
                language: Language::TsJsConfig,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: r#"{"dependencies":{"@prisma/client":"^5.0.0","express":"^4"},"devDependencies":{"@types/node":"^20"}}"#,
            })
            .expect("parse scoped package metadata without failure");

        let scoped_facts = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().is_some_and(|target| {
                    target.as_str() == "package:@prisma/client"
                        || target.as_str() == "package:@types/node"
                })
            })
            .collect::<Vec<_>>();

        // Both scoped packages are inventoried as bounded project-config facts.
        assert_eq!(scoped_facts.len(), 2);
        for fact in &scoped_facts {
            assert_eq!(fact.kind, SemanticFactKind::ProjectConfig);
            assert_eq!(fact.certainty, FactCertainty::Structural);
            assert!(!fact.certainty.supports_family_membership());
        }

        // No fact derived from package presence supports family membership, and
        // none is a framework-role claim.
        assert!(report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
        assert!(report
            .semantic_facts
            .iter()
            .all(|fact| fact.kind != SemanticFactKind::FrameworkRole));
    }

    #[test]
    fn tsjs_project_config_json_emits_safe_root_dirs_metadata() {
        let report = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "tsconfig.json",
                language: Language::TsJsConfig,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: r#"{"compilerOptions":{"paths":{"@/*":["src/*"]},"rootDirs":["src","generated","../secret","src/*","C:/repo/src"],"jsx":"react-jsx"}}"#,
            })
            .expect("parse tsconfig metadata");

        let targets = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::ProjectConfig)
            .filter_map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<BTreeSet<_>>();

        assert!(targets.contains("tsconfig.path_alias:@/*"));
        assert!(targets.contains("tsconfig.root_dir:src"));
        assert!(targets.contains("tsconfig.root_dir:generated"));
        assert!(targets.contains("tsconfig.jsx:react-jsx"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|assumption| assumption == "project_config=root_dirs")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("../secret"));
        assert!(!debug.contains("tsconfig.root_dir:src/*"));
        assert!(!debug.contains("C:/repo/src"));
    }

    #[test]
    fn tsjs_script_config_is_metadata_only_and_unknown() {
        let report = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "vitest.config.ts",
                language: Language::TsJsConfig,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: "export default defineConfig({});\n",
            })
            .expect("parse script config metadata");

        assert_eq!(report.units.len(), 1);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "config_execution=disabled")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "BuildVariantAmbiguity")
        }));
    }

    #[test]
    fn tsjs_context_resolves_static_relative_and_alias_imports_only() {
        let context = ParserProjectContext {
            tsjs_module_paths: vec![
                "src/api/users.ts".to_string(),
                "src/api/orders.ts".to_string(),
                "src/lib/client.ts".to_string(),
                "generated/api/template.ts".to_string(),
            ],
            tsjs_path_aliases: vec![crate::ports::parser::ParserTsJsPathAlias {
                alias_pattern: "@/*".to_string(),
                target_patterns: vec!["src/*".to_string()],
            }],
            tsjs_root_dirs: vec!["generated".to_string(), "src".to_string()],
            tsjs_has_test_runner_context: true,
            ..ParserProjectContext::default()
        };
        let source = r#"
import users from "./users";
import { client } from "@/lib/client";
import template from "./template";
import express from "express";
export { orders } from "./orders";
"#;

        let report = SyntaxCodeUnitParser
            .parse_with_context(document("src/api/routes.ts", source), &context)
            .expect("parse with TS/JS context");
        let resolved_targets = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::ResolvedImport)
            .filter_map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<BTreeSet<_>>();

        assert!(resolved_targets.contains("module:src/api/users.ts"));
        assert!(resolved_targets.contains("module:src/api/orders.ts"));
        assert!(resolved_targets.contains("module:src/lib/client.ts"));
        assert!(resolved_targets.contains("module:generated/api/template.ts"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedImport
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("module:generated/api/template.ts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_import_resolution=root_dirs")
        }));
        assert!(!resolved_targets
            .iter()
            .any(|target| target.contains("express")));
        assert!(report.semantic_facts.iter().all(|fact| {
            fact.evidence.provenance.path == "src/api/routes.ts"
                && !format!("{fact:?}").contains("/Users/")
        }));
    }

    #[test]
    fn tsjs_context_marks_conflicting_root_dirs_imports_unknown() {
        let context = ParserProjectContext {
            tsjs_module_paths: vec![
                "generated/views/template.ts".to_string(),
                "mocks/views/template.ts".to_string(),
            ],
            tsjs_root_dirs: vec![
                "generated".to_string(),
                "mocks".to_string(),
                "src".to_string(),
            ],
            ..ParserProjectContext::default()
        };
        let source = r#"import template from "./template";"#;

        let report = SyntaxCodeUnitParser
            .parse_with_context(document("src/views/view.ts", source), &context)
            .expect("parse rootDirs conflict");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=root_dirs_conflict")
        }));
        assert!(!report
            .semantic_facts
            .iter()
            .any(|fact| fact.kind == SemanticFactKind::ResolvedImport));
    }

    #[test]
    fn tsjs_context_marks_dynamic_and_ambiguous_import_boundaries_unknown() {
        let context = ParserProjectContext {
            tsjs_module_paths: vec![
                "src/a.ts".to_string(),
                "src/ambiguous.ts".to_string(),
                "src/ambiguous.tsx".to_string(),
            ],
            ..ParserProjectContext::default()
        };
        let source = r#"
const name = "./a";
await import(name);
const loaded = require(name);
if (flag) require("./a");
export * from "./a";
import ambiguous from "./ambiguous";
"#;

        let report = SyntaxCodeUnitParser
            .parse_with_context(document("src/main.ts", source), &context)
            .expect("parse dynamic import boundaries");
        let unknown_kinds = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("tsjs_unknown_kind="))
            .collect::<BTreeSet<_>>();

        assert!(unknown_kinds.contains("dynamic_import"));
        assert!(unknown_kinds.contains("dynamic_require"));
        assert!(unknown_kinds.contains("conditional_require"));
        assert!(unknown_kinds.contains("ambiguous_reexport"));
        assert!(unknown_kinds.contains("ambiguous_import"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "tsjs_unknown_kind=ambiguous_reexport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "literal_specifier=./a#*")
        }));
    }

    #[test]
    fn ambient_jest_vitest_runner_requires_project_context() {
        let source = r#"
describe("users", () => {
  it("loads", () => {});
});
"#;
        let no_context = SyntaxCodeUnitParser
            .parse_with_context(
                document("tests/users.test.ts", source),
                &ParserProjectContext::default(),
            )
            .expect("parse without test context");
        assert!(no_context.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "tsjs_unknown_kind=ambient_runner_without_project_context"
                })
        }));
        assert!(no_context.semantic_facts.iter().all(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            != Some("jest_vitest.describe")));

        let with_context = SyntaxCodeUnitParser
            .parse_with_context(
                document("tests/users.test.ts", source),
                &ParserProjectContext {
                    tsjs_has_test_runner_context: true,
                    ..ParserProjectContext::default()
                },
            )
            .expect("parse with test context");
        assert!(with_context.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("jest_vitest.describe")
        }));
    }

    #[test]
    fn unsupported_language_is_reported_without_units() {
        let error = SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: "src/tool.py",
                language: Language::Unknown("python".to_string()),
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: "def tool(): pass\n",
            })
            .expect_err("unsupported language");

        assert_eq!(error, ParseError::UnsupportedLanguage);
    }

    #[test]
    fn next_file_convention_recognizes_src_pages_router() {
        assert_eq!(
            next_file_convention_kind("src/pages/api/users.ts"),
            Some(CodeUnitKind::NextPagesApiRoute)
        );
        assert_eq!(
            next_file_convention_kind("src/pages/home.tsx"),
            Some(CodeUnitKind::NextPagesPage)
        );
        // The un-prefixed Pages Router still works.
        assert_eq!(
            next_file_convention_kind("pages/api/orders.ts"),
            Some(CodeUnitKind::NextPagesApiRoute)
        );
        // A nested page under src/pages is still a page.
        assert_eq!(
            next_file_convention_kind("src/pages/dashboard/index.tsx"),
            Some(CodeUnitKind::NextPagesPage)
        );
    }

    #[test]
    fn first_quoted_honors_backslash_escapes() {
        assert_eq!(first_quoted("\"plain\""), Some("plain".to_string()));
        // The escaped quote must not truncate the specifier.
        assert_eq!(first_quoted("\"a\\\"b\""), Some("a\\\"b".to_string()));
        assert_eq!(first_quoted("'a\\'b'"), Some("a\\'b".to_string()));
        // An unterminated quote yields nothing rather than a partial match.
        assert_eq!(first_quoted("\"unterminated"), None);
    }
}
