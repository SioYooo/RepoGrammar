//! Dependency-free syntax-only TS/JS code-unit extraction.
//!
//! This adapter is a bootstrap parser boundary. It emits structural code-unit
//! candidates and diagnostics only; it does not provide semantic certainty.

use super::{ir_edges_for_units, ir_nodes_for_units, tsjs::TSJS_ANCHOR_ENGINE};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId,
};
use crate::core::policy::paths::validate_repo_relative_path;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct SyntaxCodeUnitParser;

impl SourceParser for SyntaxCodeUnitParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        if document.language == Language::TsJsConfig {
            return tsjs_project_config_report(document);
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
            return tsjs_project_config_report(document);
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
}

fn tsjs_project_config_report(document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
    let unit = project_config_unit(&document)?;
    let mut semantic_facts = Vec::new();
    let mut diagnostics = Vec::new();
    if document.path.ends_with(".json") {
        match serde_json::from_str::<Value>(document.text) {
            Ok(value) => {
                semantic_facts.extend(tsjs_json_project_config_facts(&document, &unit, &value)?);
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
    Ok(ParseReport {
        units,
        ir_nodes,
        ir_edges,
        semantic_facts,
        diagnostics,
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
    let id = CodeUnitId::new(format!(
        "unit:{}#project_config:0-{}:0",
        document.path,
        document.text.len()
    ))
    .map_err(ParseError::Internal)?;
    Ok(CodeUnit {
        id,
        language: Language::TsJsConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
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
            for field in ["dependencies", "devDependencies", "peerDependencies"] {
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

fn tsjs_import_resolution_facts(
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

fn first_quoted_after(line: &str, marker: &str) -> Option<String> {
    let index = line.find(marker)? + marker.len();
    first_quoted(&line[index..])
}

fn first_quoted(text: &str) -> Option<String> {
    let quote_index = text.find(['"', '\''])?;
    let quote = text.as_bytes()[quote_index] as char;
    let rest = &text[quote_index + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
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
}

impl<'a> SyntaxScanner<'a> {
    fn new(document: SourceDocument<'a>) -> Self {
        Self {
            document,
            context: None,
            units: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn new_with_context(document: SourceDocument<'a>, context: &'a ParserProjectContext) -> Self {
        Self {
            document,
            context: Some(context),
            units: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
        }
    }

    fn scan(&mut self) -> Result<(), ParseError> {
        self.add_unit(CodeUnitKind::Module, "module", 0, self.document.text.len())?;
        let lines = lines_with_offsets(self.document.text);
        let class_ranges = self.scan_classes(&lines)?;
        self.scan_class_methods(&lines, &class_ranges)?;
        self.scan_top_level_patterns(&lines, &class_ranges)?;
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
        let mut semantic_facts =
            super::tsjs::exact_framework_anchors(&self.document, &self.units, self.context)?;
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

    fn scan_classes(&mut self, lines: &[(usize, &str)]) -> Result<Vec<(usize, usize)>, ParseError> {
        let mut ranges = Vec::new();
        for (line_start, line) in lines {
            let Some(class_offset) = find_keyword(line, "class") else {
                continue;
            };
            let identifier_start = class_offset + "class".len();
            let Some((name, _)) = parse_identifier_after(line, identifier_start) else {
                continue;
            };
            let start = line_start + class_offset;
            let end = declaration_extent(self.document.text, start, line_start + line.len());
            self.add_unit(CodeUnitKind::Class, &name, start, end)?;
            ranges.push((start, end));
        }
        Ok(ranges)
    }

    fn scan_class_methods(
        &mut self,
        lines: &[(usize, &str)],
        class_ranges: &[(usize, usize)],
    ) -> Result<(), ParseError> {
        for (line_start, line) in lines {
            if !is_inside_any_range(*line_start, class_ranges) {
                continue;
            }
            let Some((name, offset)) = method_name_from_line(line) else {
                continue;
            };
            let start = line_start + offset;
            let end = declaration_extent(self.document.text, start, line_start + line.len());
            self.add_unit(CodeUnitKind::Method, &name, start, end)?;
        }
        Ok(())
    }

    fn scan_top_level_patterns(
        &mut self,
        lines: &[(usize, &str)],
        class_ranges: &[(usize, usize)],
    ) -> Result<(), ParseError> {
        let test_runner_names = super::tsjs::exact_test_runner_call_names(self.document.text);
        let fastify_receivers = fastify_receivers_for_scan(self.document.text);
        let drizzle_table_factories = drizzle_table_factories_for_scan(self.document.text);
        for (line_start, line) in lines {
            let line_end = line_start + line.len();
            if let Some((kind, name, offset)) = next_default_export_unit(self.document.path, line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(kind, name, start, end)?;
            }
            if let Some((method, offset)) = next_route_handler_export(self.document.path, line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::NextRouteHandler, method, start, end)?;
            }
            if let Some((receiver, method, offset)) = static_route_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                if fastify_receivers.contains(receiver) {
                    self.add_unit(CodeUnitKind::FastifyRoute, method, start, end)?;
                } else {
                    self.add_unit(CodeUnitKind::ExpressRoute, method, start, end)?;
                }
            } else if let Some((receiver, offset)) = fastify_full_route_call(line) {
                if fastify_receivers.contains(receiver) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::FastifyRoute, "route", start, end)?;
                }
            } else if let Some((receiver, offset)) = dynamic_route_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                let kind = if fastify_receivers.contains(receiver) {
                    CodeUnitKind::FastifyRoute
                } else {
                    CodeUnitKind::ExpressRoute
                };
                self.add_unit(kind, "dynamic_route", start, end)?;
            }
            if let Some((name, offset)) = prisma_query_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::PrismaQuery, name, start, end)?;
            }
            if let Some(offset) = prisma_transaction_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::PrismaTransaction, "transaction", start, end)?;
            }
            if let Some((name, offset)) =
                drizzle_schema_table_declaration(line, &drizzle_table_factories)
            {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleSchemaTable, name, start, end)?;
            }
            if let Some((operation, offset)) = drizzle_query_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleQuery, operation, start, end)?;
            }
            if let Some(offset) = drizzle_transaction_call(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                self.add_unit(CodeUnitKind::DrizzleTransaction, "transaction", start, end)?;
            }
            for suite_name in &test_runner_names.suite_names {
                if let Some(offset) = call_offset(line, suite_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestSuite, suite_name, start, end)?;
                }
            }
            if !test_runner_names.suite_names.contains("describe") {
                if let Some(offset) = call_offset(line, "describe") {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestSuite, "describe", start, end)?;
                }
            }
            for test_name in &test_runner_names.test_names {
                if let Some(offset) = call_offset(line, test_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestCase, test_name, start, end)?;
                }
            }
            for test_name in ["it", "test"] {
                if test_runner_names.test_names.contains(test_name) {
                    continue;
                }
                if let Some(offset) = call_offset(line, test_name) {
                    let start = line_start + offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    self.add_unit(CodeUnitKind::TestCase, test_name, start, end)?;
                }
            }
            if is_inside_any_range(*line_start, class_ranges) {
                continue;
            }
            if let Some(function_offset) = find_keyword(line, "function") {
                let identifier_start = function_offset + "function".len();
                if let Some((name, _)) = parse_identifier_after(line, identifier_start) {
                    let start = line_start + function_offset;
                    let end = declaration_extent(self.document.text, start, line_end);
                    let kind = classify_callable(&self.document, &name, CodeUnitKind::Function);
                    self.add_unit(kind, &name, start, end)?;
                }
            }
            if let Some((name, offset)) = assigned_arrow_name(line) {
                let start = line_start + offset;
                let end = declaration_extent(self.document.text, start, line_end);
                let kind = classify_callable(&self.document, &name, CodeUnitKind::ArrowFunction);
                self.add_unit(kind, &name, start, end)?;
            }
        }
        Ok(())
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

fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
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

fn declaration_extent(text: &str, start: usize, fallback_end: usize) -> usize {
    let Some(open_relative) = text[start..].find('{') else {
        return fallback_end;
    };
    let open = start + open_relative;
    matching_closing_brace(text, open).unwrap_or(text.len())
}

fn matching_closing_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in text.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_inside_any_range(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| offset > *start && offset < *end)
}

fn find_keyword(line: &str, keyword: &str) -> Option<usize> {
    line.match_indices(keyword)
        .find(|(offset, _)| has_identifier_boundaries(line, *offset, keyword.len()))
        .map(|(offset, _)| offset)
}

fn has_identifier_boundaries(line: &str, offset: usize, len: usize) -> bool {
    let before = offset
        .checked_sub(1)
        .and_then(|index| line.as_bytes().get(index))
        .copied();
    let after = line.as_bytes().get(offset + len).copied();
    !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
}

fn parse_identifier_after(line: &str, offset: usize) -> Option<(String, usize)> {
    let mut cursor = offset;
    let bytes = line.as_bytes();
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'*') {
        cursor += 1;
    }
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let start = cursor;
    if !bytes.get(cursor).copied().is_some_and(is_identifier_start) {
        return None;
    }
    cursor += 1;
    while cursor < bytes.len() && bytes[cursor].is_ascii() && is_identifier_byte(bytes[cursor]) {
        cursor += 1;
    }
    Some((line[start..cursor].to_string(), start))
}

fn assigned_arrow_name(line: &str) -> Option<(String, usize)> {
    for keyword in ["const", "let", "var"] {
        let Some(keyword_offset) = find_keyword(line, keyword) else {
            continue;
        };
        let Some((name, name_offset)) =
            parse_identifier_after(line, keyword_offset + keyword.len())
        else {
            continue;
        };
        let equals_offset = line[name_offset + name.len()..].find('=')? + name_offset + name.len();
        if line[equals_offset..].contains("=>") {
            return Some((name, keyword_offset));
        }
    }
    None
}

fn method_name_from_line(line: &str) -> Option<(String, usize)> {
    let trimmed_start = line.find(|character: char| !character.is_whitespace())?;
    let trimmed = &line[trimmed_start..];
    if trimmed.starts_with("//")
        || trimmed.starts_with('*')
        || trimmed.starts_with("function ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("switch ")
        || trimmed.starts_with("catch ")
        || trimmed.starts_with("return ")
    {
        return None;
    }
    let mut cursor = 0usize;
    for modifier in ["public ", "private ", "protected ", "static ", "async "] {
        if trimmed[cursor..].starts_with(modifier) {
            cursor += modifier.len();
        }
    }
    if trimmed[cursor..].starts_with('#') {
        cursor += 1;
    }
    let (name, name_offset) = parse_identifier_after(trimmed, cursor)?;
    let after_name = name_offset + name.len();
    let rest = &trimmed[after_name..];
    let paren_offset = rest.find('(')? + after_name;
    let equals_before_paren = trimmed[..paren_offset].contains('=');
    if equals_before_paren || trimmed[paren_offset..].contains("=>") {
        return None;
    }
    Some((name, trimmed_start + name_offset))
}

fn static_route_call(line: &str) -> Option<(&str, &'static str, usize)> {
    for method in [
        "get", "head", "post", "put", "delete", "options", "patch", "all", "use",
    ] {
        let pattern = format!(".{method}(");
        if let Some(method_dot) = line.find(&pattern) {
            let start = line[..method_dot]
                .rfind(|character: char| character.is_whitespace())
                .map(|offset| offset + 1)
                .unwrap_or(0);
            let receiver = line[start..method_dot].trim();
            if !receiver.is_empty() {
                return Some((receiver, method, start));
            }
        }
    }
    None
}

fn fastify_full_route_call(line: &str) -> Option<(&str, usize)> {
    let pattern = ".route(";
    let method_dot = line.find(pattern)?;
    let start = line[..method_dot]
        .rfind(|character: char| character.is_whitespace())
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let receiver = line[start..method_dot].trim();
    (!receiver.is_empty()).then_some((receiver, start))
}

fn dynamic_route_call(line: &str) -> Option<(&str, usize)> {
    let bracket = line.find('[')?;
    let close = line[bracket + 1..].find(']')? + bracket + 1;
    if !line[close + 1..].trim_start().starts_with('(') {
        return None;
    }
    if !line[close + 1..].contains('"') && !line[close + 1..].contains('\'') {
        return None;
    }
    let start = line[..bracket]
        .rfind(|character: char| character.is_whitespace())
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let receiver = line[start..bracket].trim();
    if receiver.is_empty()
        || !receiver.chars().next().is_some_and(|character| {
            character.is_ascii_alphabetic() || character == '_' || character == '$'
        })
    {
        return None;
    }
    Some((receiver, start))
}

fn fastify_receivers_for_scan(text: &str) -> BTreeSet<String> {
    let mut factories = BTreeSet::new();
    let mut receivers = BTreeSet::new();
    for line in top_level_lines(text) {
        if let Some(name) = imported_default_or_require_name(line, "fastify") {
            factories.insert(name.to_string());
        }
        if line.contains("import { fastify") && line.contains(" from ") && line.contains("fastify")
        {
            factories.insert("fastify".to_string());
        }
        if line.contains("import { Fastify") && line.contains(" from ") && line.contains("fastify")
        {
            factories.insert("Fastify".to_string());
        }
        for name in named_import_or_require_names(line, "fastify", &["fastify", "Fastify"]) {
            factories.insert(name);
        }
    }
    for line in top_level_lines(text) {
        let Some((name, rhs)) = declaration_assignment(line) else {
            continue;
        };
        if factories
            .iter()
            .any(|factory| exact_call_head(rhs, factory.as_str()))
        {
            receivers.insert(name.to_string());
        }
    }
    receivers
}

fn drizzle_table_factories_for_scan(text: &str) -> BTreeSet<String> {
    let mut factories = ["pgTable", "mysqlTable", "sqliteTable"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    for line in top_level_lines(text) {
        for module in [
            "drizzle-orm/pg-core",
            "drizzle-orm/mysql-core",
            "drizzle-orm/sqlite-core",
        ] {
            for name in named_import_or_require_names(
                line,
                module,
                &["pgTable", "mysqlTable", "sqliteTable"],
            ) {
                factories.insert(name);
            }
        }
    }
    factories
}

fn next_default_export_unit(path: &str, line: &str) -> Option<(CodeUnitKind, &'static str, usize)> {
    let kind = next_file_convention_kind(path)?;
    let offset = line.find("export default")?;
    let rest = line[offset..].trim_start();
    match kind {
        CodeUnitKind::NextAppPage | CodeUnitKind::NextAppLayout | CodeUnitKind::NextPagesPage => {
            if rest.contains("function") || rest.contains("=>") || rest.contains("createElement") {
                Some((kind, "default", offset))
            } else {
                None
            }
        }
        CodeUnitKind::NextPagesApiRoute => {
            if rest.contains("function") || rest.contains("=>") {
                Some((kind, "handler", offset))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn next_route_handler_export(path: &str, line: &str) -> Option<(&'static str, usize)> {
    if next_file_convention_kind(path) != Some(CodeUnitKind::NextRouteHandler) {
        return None;
    }
    let offset = line.find("export ")?;
    let trimmed = line[offset..].trim_start();
    for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
        let function_pattern = format!("function {method}");
        if trimmed.contains(&function_pattern) {
            return Some((method, offset));
        }
        if exported_const_async_route_handler(trimmed, method) {
            return Some((method, offset));
        }
    }
    None
}

fn exported_const_async_route_handler(line: &str, method: &str) -> bool {
    let Some(after_export) = line.trim_start().strip_prefix("export ") else {
        return false;
    };
    let Some(after_const) = after_export.trim_start().strip_prefix("const ") else {
        return false;
    };
    let Some((name, name_offset)) = parse_identifier_after(after_const, 0) else {
        return false;
    };
    if name != method {
        return false;
    }
    let after_name = name_offset + name.len();
    let Some(rhs) = after_const[after_name..]
        .trim_start()
        .strip_prefix('=')
        .map(str::trim_start)
    else {
        return false;
    };
    rhs.starts_with("async ") && rhs.contains("=>")
}

fn next_file_convention_kind(path: &str) -> Option<CodeUnitKind> {
    let path = path.trim_start_matches("./");
    let extension_ok = [".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|extension| path.ends_with(extension));
    if !extension_ok {
        return None;
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "page") {
        return Some(CodeUnitKind::NextAppPage);
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "layout") {
        return Some(CodeUnitKind::NextAppLayout);
    }
    if (path.starts_with("app/") || path.starts_with("src/app/")) && is_named_file(path, "route") {
        return Some(CodeUnitKind::NextRouteHandler);
    }
    if path.starts_with("pages/api/") && !path.ends_with(".tsx") && !path.ends_with(".jsx") {
        return Some(CodeUnitKind::NextPagesApiRoute);
    }
    if path.starts_with("pages/") && !path.starts_with("pages/api/") {
        return Some(CodeUnitKind::NextPagesPage);
    }
    None
}

fn is_named_file(path: &str, stem: &str) -> bool {
    [".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|extension| path.ends_with(&format!("/{stem}{extension}")))
}

fn prisma_query_call(line: &str) -> Option<(&'static str, usize)> {
    for pattern in [
        ".$queryRaw(",
        ".$executeRaw(",
        ".$queryRawUnsafe(",
        ".$executeRawUnsafe(",
    ] {
        if let Some(offset) = receiver_offset_before_pattern(line, pattern) {
            return Some(("raw", offset));
        }
    }
    for operation in [
        "findMany",
        "findUnique",
        "findFirst",
        "create",
        "createMany",
        "update",
        "updateMany",
        "upsert",
        "delete",
        "deleteMany",
        "count",
        "aggregate",
        "groupBy",
    ] {
        if let Some(offset) = model_operation_receiver_offset(line, operation) {
            return Some((operation, offset));
        }
    }
    None
}

fn prisma_transaction_call(line: &str) -> Option<usize> {
    receiver_offset_before_pattern(line, ".$transaction(")
}

fn drizzle_schema_table_declaration<'a>(
    line: &'a str,
    table_factories: &BTreeSet<String>,
) -> Option<(&'a str, usize)> {
    let (name, rhs, start) = declaration_assignment_with_offset(line)?;
    let rhs = rhs.trim_start();
    let (factory, factory_start) = parse_identifier_after(rhs, 0)?;
    let after_factory = factory_start + factory.len();
    if table_factories.contains(&factory) && rhs[after_factory..].trim_start().starts_with('(') {
        Some((name, start))
    } else {
        None
    }
}

fn drizzle_query_call(line: &str) -> Option<(&'static str, usize)> {
    for (operation, target) in [
        ("findMany", "query_findMany"),
        ("findFirst", "query_findFirst"),
    ] {
        if let Some(offset) = drizzle_query_relation_offset(line, operation) {
            return Some((target, offset));
        }
    }
    if let Some(offset) = receiver_offset_before_pattern(line, ".execute(") {
        if !receiver_is_chained(line, offset) {
            return Some(("execute", offset));
        }
    }
    for operation in ["select", "insert", "update", "delete"] {
        let pattern = format!(".{operation}(");
        let Some(offset) = receiver_offset_before_pattern(line, &pattern) else {
            continue;
        };
        if receiver_is_chained(line, offset) {
            continue;
        }
        if operation != "select" || line[offset..].contains(".from(") {
            if operation != "select" && call_first_argument_starts_with_quote(&line[offset..]) {
                continue;
            }
            return Some((operation, offset));
        }
    }
    None
}

fn drizzle_transaction_call(line: &str) -> Option<usize> {
    receiver_offset_before_pattern(line, ".transaction(")
}

fn drizzle_query_relation_offset(line: &str, operation: &str) -> Option<usize> {
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(".query.")?;
        let query_dot = search_offset + relative;
        let (_, db_start) = identifier_before_dot(line, query_dot)?;
        let after_query = &line[query_dot + ".query.".len()..];
        let (table, table_offset) = parse_identifier_after(after_query, 0)?;
        let after_table = table_offset + table.len();
        let Some(rest) = after_query[after_table..].trim_start().strip_prefix('.') else {
            search_offset = query_dot + ".query.".len();
            continue;
        };
        if rest.starts_with(operation) && rest[operation.len()..].trim_start().starts_with('(') {
            return Some(db_start);
        }
        search_offset = query_dot + ".query.".len();
    }
    None
}

fn model_operation_receiver_offset(line: &str, operation: &str) -> Option<usize> {
    let pattern = format!(".{operation}(");
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(&pattern)?;
        let operation_dot = search_offset + relative;
        let (_, model_start) = identifier_before_dot(line, operation_dot)?;
        let model_prefix_end = line[..model_start].trim_end().len();
        if model_prefix_end == 0 || line.as_bytes()[model_prefix_end - 1] != b'.' {
            search_offset = operation_dot + pattern.len();
            continue;
        }
        let client_dot = model_prefix_end - 1;
        if let Some((_, client_start)) = identifier_before_dot(line, client_dot) {
            return Some(client_start);
        }
        search_offset = operation_dot + pattern.len();
    }
    None
}

fn receiver_offset_before_pattern(line: &str, pattern: &str) -> Option<usize> {
    let mut search_offset = 0usize;
    while search_offset < line.len() {
        let relative = line[search_offset..].find(pattern)?;
        let dot_offset = search_offset + relative;
        if let Some((_, receiver_start)) = identifier_before_dot(line, dot_offset) {
            return Some(receiver_start);
        }
        search_offset = dot_offset + pattern.len();
    }
    None
}

fn receiver_is_chained(line: &str, receiver_start: usize) -> bool {
    let prefix_end = line[..receiver_start].trim_end().len();
    prefix_end > 0 && line.as_bytes()[prefix_end - 1] == b'.'
}

fn call_first_argument_starts_with_quote(call: &str) -> bool {
    let Some(open) = call.find('(') else {
        return false;
    };
    let after_open = call[open + 1..].trim_start();
    after_open.starts_with('"') || after_open.starts_with('\'')
}

fn identifier_before_dot(line: &str, dot_offset: usize) -> Option<(&str, usize)> {
    if line.as_bytes().get(dot_offset) != Some(&b'.') {
        return None;
    }
    let mut end = dot_offset;
    let bytes = line.as_bytes();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_identifier_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == end || !is_identifier_start(bytes[start]) {
        return None;
    }
    Some((&line[start..end], start))
}

fn top_level_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut depth: i64 = 0;
    for line in text.lines() {
        if depth == 0 {
            lines.push(line);
        }
        depth += line.bytes().fold(0, |delta, byte| match byte {
            b'{' => delta + 1,
            b'}' => delta - 1,
            _ => delta,
        });
        if depth < 0 {
            depth = 0;
        }
    }
    lines
}

fn imported_default_or_require_name<'a>(line: &'a str, module: &str) -> Option<&'a str> {
    let trimmed = line.trim();
    let import_suffix = format!(" from \"{module}\"");
    let import_suffix_single = format!(" from '{module}'");
    if let Some(rest) = trimmed.strip_prefix("import ") {
        if rest.contains(&import_suffix) || rest.contains(&import_suffix_single) {
            let before_from = rest.split(" from ").next()?.trim();
            if !before_from.starts_with('{') && !before_from.starts_with('*') {
                return before_from.split(',').next().map(str::trim);
            }
        }
    }
    let require_double = format!("require(\"{module}\")");
    let require_single = format!("require('{module}')");
    if (trimmed.contains(&require_double) || trimmed.contains(&require_single))
        && trimmed.starts_with("const ")
    {
        return declaration_assignment(trimmed).map(|(name, _)| name);
    }
    None
}

fn named_import_or_require_names(line: &str, module: &str, exported_names: &[&str]) -> Vec<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("import ") {
        if let Some(from_index) = rest.find(" from ") {
            let clause = rest[..from_index].trim();
            let quoted_module = first_quoted(&rest[from_index + " from ".len()..]);
            if quoted_module.as_deref() == Some(module) {
                return braced_binding_names(clause, false, exported_names);
            }
        }
    }
    let Some(module_name) = first_quoted_after(trimmed, "require(") else {
        return Vec::new();
    };
    if module_name != module {
        return Vec::new();
    }
    let Some(after_keyword) = ["const ", "let ", "var "]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))
    else {
        return Vec::new();
    };
    let Some(equals) = after_keyword.find('=') else {
        return Vec::new();
    };
    braced_binding_names(after_keyword[..equals].trim(), true, exported_names)
}

fn braced_binding_names(
    clause: &str,
    allow_colon_alias: bool,
    exported_names: &[&str],
) -> Vec<String> {
    let Some(open) = clause.find('{') else {
        return Vec::new();
    };
    let Some(close_relative) = clause[open..].find('}') else {
        return Vec::new();
    };
    let inner = &clause[open + 1..open + close_relative];
    let mut names = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (original, local) = if allow_colon_alias {
            match part.split_once(':') {
                Some((original, local)) => (original.trim(), local.trim()),
                None => (part, part),
            }
        } else {
            match part.split_once(" as ") {
                Some((original, local)) => (original.trim(), local.trim()),
                None => (part, part),
            }
        };
        let Some((original, _)) = parse_identifier_after(original, 0) else {
            continue;
        };
        if !exported_names.contains(&original.as_str()) {
            continue;
        }
        let local = local.split('=').next().unwrap_or(local).trim();
        if let Some((local, _)) = parse_identifier_after(local, 0) {
            names.push(local);
        }
    }
    names
}

fn declaration_assignment(line: &str) -> Option<(&str, &str)> {
    declaration_assignment_with_offset(line).map(|(name, rhs, _)| (name, rhs))
}

fn declaration_assignment_with_offset(line: &str) -> Option<(&str, &str, usize)> {
    let trimmed_start = line
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(0);
    let mut trimmed = &line[trimmed_start..];
    let mut prefix_len = 0usize;
    if let Some(rest) = trimmed.strip_prefix("export ") {
        prefix_len += "export ".len();
        trimmed = rest.trim_start();
    }
    for keyword in ["const ", "let ", "var "] {
        let Some(rest) = trimmed.strip_prefix(keyword) else {
            continue;
        };
        let start = trimmed_start + prefix_len;
        let (name, name_offset) = parse_identifier_after(rest, 0)?;
        let equals = rest[name_offset + name.len()..].find('=')? + name_offset + name.len();
        let rhs = rest[equals + 1..].trim();
        return Some((&rest[name_offset..name_offset + name.len()], rhs, start));
    }
    None
}

fn exact_call_head(rhs: &str, head: &str) -> bool {
    let rhs = rhs.trim().trim_end_matches(';').trim();
    rhs.starts_with(head) && rhs[head.len()..].trim_start().starts_with('(')
}

fn call_offset(line: &str, function_name: &str) -> Option<usize> {
    line.match_indices(function_name)
        .find(|(offset, _)| {
            has_identifier_boundaries(line, *offset, function_name.len())
                && line[*offset + function_name.len()..]
                    .trim_start()
                    .starts_with('(')
        })
        .map(|(offset, _)| offset)
}

fn classify_callable(
    document: &SourceDocument<'_>,
    name: &str,
    fallback: CodeUnitKind,
) -> CodeUnitKind {
    if is_hook_name(name) {
        return CodeUnitKind::ReactHook;
    }
    if is_component_name(name) && (is_react_path(document.path) || contains_jsx(document.text)) {
        return CodeUnitKind::ReactComponent;
    }
    fallback
}

fn is_hook_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("use") else {
        return false;
    };
    rest.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
}

fn is_react_path(path: &str) -> bool {
    path.ends_with(".tsx") || path.ends_with(".jsx")
}

fn contains_jsx(text: &str) -> bool {
    text.contains("return <") || text.contains("</") || text.contains("/>")
}

fn delimiters_are_balanced(text: &str) -> bool {
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

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn slug(value: &str) -> String {
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
        assert!(report.semantic_facts.iter().any(|fact| fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "project_config=root_dirs")));
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
}
