//! CPython-ast-backed Python code-unit extraction.
//!
//! This adapter uses the repository's Python worker process so Rust does not
//! hand-roll Python parsing rules. The worker returns owned metadata only.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    RepositoryRevision, SemanticFact, SemanticFactKind, SourceRange, SymbolId,
};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAX_PYTHON_FRONTEND_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_PYTHON_FRONTEND_INPUT_BYTES: usize = 1024 * 1024;
const MAX_PYTHON_FRONTEND_FACTS: usize = 2_000;
const MAX_PYTHON_FACT_TEXT_BYTES: usize = 2_048;
const MAX_PYTHON_FACT_TARGET_BYTES: usize = 256;
const MAX_PYTHON_FACT_NOTE_BYTES: usize = 160;
const MAX_PYTHON_FACT_ASSUMPTIONS: usize = 4;
const MAX_PYTHON_FACT_ASSUMPTION_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonAstParser {
    executable: String,
    worker_script: PathBuf,
}

impl Default for PythonAstParser {
    fn default() -> Self {
        Self {
            executable: "python3".to_string(),
            worker_script: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src/workers/python/worker.py"),
        }
    }
}

impl PythonAstParser {
    #[cfg(test)]
    fn with_worker(executable: impl Into<String>, worker_script: PathBuf) -> Self {
        Self {
            executable: executable.into(),
            worker_script,
        }
    }
}

impl SourceParser for PythonAstParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        if is_python_project_config_path(document.path) {
            if document.language != Language::PythonConfig {
                return Err(ParseError::UnsupportedLanguage);
            }
            let response = self.parse_project_config(&document)?;
            return parse_project_config_response(&document, &response);
        }
        if document.language != Language::Python {
            return Err(ParseError::UnsupportedLanguage);
        }
        let output = self.parse_document(&document, None)?;
        parse_worker_response(&document, &output.response)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        if is_python_project_config_path(document.path) {
            if document.language != Language::PythonConfig {
                return Err(ParseError::UnsupportedLanguage);
            }
            let response = self.parse_project_config(&document)?;
            return parse_project_config_response(&document, &response);
        }
        if document.language != Language::Python {
            return Err(ParseError::UnsupportedLanguage);
        }
        let output = self.parse_document(&document, Some(context))?;
        let mut report = parse_worker_response(&document, &output.response)?;
        if output.context_omitted {
            report.diagnostics.push(ParseDiagnostic {
                path: document.path.to_string(),
                range: None,
                severity: ParseDiagnosticSeverity::Warning,
                message: "python parse context omitted because request exceeded size limit"
                    .to_string(),
            });
        }
        Ok(report)
    }
}

impl PythonAstParser {
    fn parse_project_config(&self, document: &SourceDocument<'_>) -> Result<String, ParseError> {
        let payload = json!({
            "protocol_version": 1,
            "mode": "parse_project_config",
            "path": document.path,
            "content_hash": document.content_hash.as_str(),
            "repository_revision": document.repository_revision.as_str(),
            "text": document.text,
        })
        .to_string();
        if payload.len() > MAX_PYTHON_FRONTEND_INPUT_BYTES {
            return Err(ParseError::Internal(
                "python ast frontend request exceeded size limit".to_string(),
            ));
        }
        self.run_worker_request(&payload)
    }

    fn parse_document(
        &self,
        document: &SourceDocument<'_>,
        context: Option<&ParserProjectContext>,
    ) -> Result<PythonParseOutput, ParseError> {
        let (serialized, context_omitted) = serialize_parse_request(document, context)?;
        let response = self.run_worker_request(&serialized)?;
        Ok(PythonParseOutput {
            response,
            context_omitted,
        })
    }

    fn run_worker_request(&self, serialized: &str) -> Result<String, ParseError> {
        let mut child = Command::new(&self.executable)
            .arg(&self.worker_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ParseError::Internal("python ast frontend is unavailable".to_string()))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ParseError::Internal("python ast frontend stdin unavailable".into()))?;
        stdin
            .write_all(serialized.as_bytes())
            .map_err(|_| ParseError::Internal("python ast frontend request failed".into()))?;
        stdin
            .write_all(b"\n")
            .map_err(|_| ParseError::Internal("python ast frontend request failed".into()))?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .map_err(|_| ParseError::Internal("python ast frontend failed".into()))?;
        if !output.status.success() {
            return Err(ParseError::Internal(
                "python ast frontend rejected parse request".to_string(),
            ));
        }
        if output.stdout.len() > MAX_PYTHON_FRONTEND_OUTPUT_BYTES {
            return Err(ParseError::Internal(
                "python ast frontend output exceeded size limit".to_string(),
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| ParseError::Internal("python ast frontend output was not UTF-8".into()))
    }
}

struct PythonParseOutput {
    response: String,
    context_omitted: bool,
}

fn serialize_parse_request(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
) -> Result<(String, bool), ParseError> {
    let payload = parse_document_payload(document, context);
    let serialized = payload.to_string();
    if serialized.len() <= MAX_PYTHON_FRONTEND_INPUT_BYTES {
        return Ok((serialized, false));
    }
    if context.is_some() {
        let fallback = parse_document_payload(document, None).to_string();
        if fallback.len() <= MAX_PYTHON_FRONTEND_INPUT_BYTES {
            return Ok((fallback, true));
        }
    }
    Err(ParseError::Internal(
        "python ast frontend request exceeded size limit".to_string(),
    ))
}

fn parse_document_payload(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
) -> Value {
    let mut payload = json!({
        "protocol_version": 1,
        "mode": "parse_document",
        "path": document.path,
        "content_hash": document.content_hash.as_str(),
        "repository_revision": document.repository_revision.as_str(),
        "text": document.text,
    });
    if let Some(context) = context {
        let object = payload
            .as_object_mut()
            .expect("parse document payload must be an object");
        object.insert(
            "module_paths".to_string(),
            json!(context.python_module_paths),
        );
        object.insert(
            "source_roots".to_string(),
            json!(context.python_source_roots),
        );
        object.insert(
            "conftest_files".to_string(),
            json!(context
                .python_conftest_files
                .iter()
                .map(|file| json!({
                    "path": &file.path,
                    "text": &file.text,
                }))
                .collect::<Vec<_>>()),
        );
    }
    payload
}

fn parse_worker_response(
    document: &SourceDocument<'_>,
    response: &str,
) -> Result<ParseReport, ParseError> {
    parse_report_response(
        document,
        response,
        "parse_document",
        &[
            "protocol_version",
            "mode",
            "path",
            "units",
            "facts",
            "diagnostics",
        ],
    )
}

fn parse_project_config_response(
    document: &SourceDocument<'_>,
    response: &str,
) -> Result<ParseReport, ParseError> {
    if response.len() > MAX_PYTHON_FRONTEND_OUTPUT_BYTES {
        return Err(ParseError::Internal(
            "python ast frontend output exceeded size limit".to_string(),
        ));
    }
    let lines = response
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let [line] = lines.as_slice() else {
        return Err(ParseError::Internal(
            "python ast frontend must return exactly one response".to_string(),
        ));
    };
    let value: Value = serde_json::from_str(line)
        .map_err(|_| ParseError::Internal("python ast frontend returned invalid JSON".into()))?;
    let object = value.as_object().ok_or_else(|| {
        ParseError::Internal("python ast frontend response was not an object".into())
    })?;
    validate_allowed_keys(
        object,
        &["protocol_version", "mode", "path", "config", "unknowns"],
        "python ast frontend response",
    )?;
    if object.get("protocol_version").and_then(Value::as_u64) != Some(1)
        || object.get("mode").and_then(Value::as_str) != Some("parse_project_config")
        || object.get("path").and_then(Value::as_str) != Some(document.path)
    {
        return Err(ParseError::Internal(
            "python ast frontend response envelope was invalid".to_string(),
        ));
    }

    let unit = project_config_unit(document)?;
    let mut semantic_facts = project_config_facts(document, &unit, object)?;
    sort_semantic_facts(&mut semantic_facts);
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    Ok(ParseReport {
        units,
        ir_nodes,
        ir_edges: Vec::new(),
        semantic_facts,
        diagnostics: Vec::new(),
    })
}

fn parse_report_response(
    document: &SourceDocument<'_>,
    response: &str,
    expected_mode: &str,
    allowed_keys: &[&str],
) -> Result<ParseReport, ParseError> {
    if response.len() > MAX_PYTHON_FRONTEND_OUTPUT_BYTES {
        return Err(ParseError::Internal(
            "python ast frontend output exceeded size limit".to_string(),
        ));
    }
    let lines = response
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let [line] = lines.as_slice() else {
        return Err(ParseError::Internal(
            "python ast frontend must return exactly one response".to_string(),
        ));
    };
    let value: Value = serde_json::from_str(line)
        .map_err(|_| ParseError::Internal("python ast frontend returned invalid JSON".into()))?;
    let object = value.as_object().ok_or_else(|| {
        ParseError::Internal("python ast frontend response was not an object".into())
    })?;
    validate_allowed_keys(object, allowed_keys, "python ast frontend response")?;
    if object.get("protocol_version").and_then(Value::as_u64) != Some(1)
        || object.get("mode").and_then(Value::as_str) != Some(expected_mode)
        || object.get("path").and_then(Value::as_str) != Some(document.path)
    {
        return Err(ParseError::Internal(
            "python ast frontend response envelope was invalid".to_string(),
        ));
    }
    let mut units = object
        .get("units")
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Internal("python ast frontend units were invalid".into()))?
        .iter()
        .map(|value| parse_unit(document, value))
        .collect::<Result<Vec<_>, _>>()?;
    units.sort_by(|left, right| {
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
    let diagnostics = object
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Internal("python ast frontend diagnostics were invalid".into()))?
        .iter()
        .map(|value| parse_diagnostic(document, value))
        .collect::<Result<Vec<_>, _>>()?;
    let fact_values = object
        .get("facts")
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Internal("python ast frontend facts were invalid".into()))?;
    if fact_values.len() > MAX_PYTHON_FRONTEND_FACTS {
        return Err(ParseError::Internal(
            "python ast frontend returned too many facts".to_string(),
        ));
    }
    let unit_ids = units
        .iter()
        .map(|unit| unit.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut semantic_facts = fact_values
        .iter()
        .map(|value| parse_fact(document, &units, &unit_ids, value))
        .collect::<Result<Vec<_>, _>>()?;
    sort_semantic_facts(&mut semantic_facts);
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

fn is_python_project_config_path(path: &str) -> bool {
    path == "pyproject.toml"
}

fn project_config_unit(document: &SourceDocument<'_>) -> Result<CodeUnit, ParseError> {
    let end_byte = document.text.len();
    let range = SourceRange::new(0, end_byte).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let id = CodeUnitId::new(format!(
        "unit:{}#project_config:project_config:0-{}:0",
        document.path, end_byte
    ))
    .map_err(ParseError::Internal)?;
    Ok(CodeUnit {
        id,
        language: Language::PythonConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    })
}

fn project_config_facts(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    object: &Map<String, Value>,
) -> Result<Vec<SemanticFact>, ParseError> {
    let config = object
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(|| ParseError::Internal("python project config summary was invalid".into()))?;
    validate_allowed_keys(
        config,
        &["project_name", "source_roots", "tool_sections"],
        "python project config summary",
    )?;
    let mut facts = Vec::new();
    if let Some(name) = optional_project_config_name(config.get("project_name"))? {
        facts.push(project_config_structural_fact(
            document,
            unit,
            "project.name",
            &project_config_target("project_name", name),
        )?);
    }
    for root in project_config_string_array(config.get("source_roots"), "source_roots")? {
        crate::core::policy::paths::validate_repo_relative_path(root).map_err(|_| {
            ParseError::Internal("python project config source root was invalid".to_string())
        })?;
        facts.push(project_config_structural_fact(
            document,
            unit,
            "source_roots",
            &project_config_target("source_root", &root.replace('/', ".")),
        )?);
    }
    for section in project_config_string_array(config.get("tool_sections"), "tool_sections")? {
        if !matches!(section, "pyrefly" | "pyright" | "pytest") {
            return Err(ParseError::Internal(
                "python project config tool section was invalid".to_string(),
            ));
        }
        facts.push(project_config_structural_fact(
            document,
            unit,
            "tool_sections",
            &project_config_target("tool_section", section),
        )?);
    }
    let unknowns = object
        .get("unknowns")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ParseError::Internal("python project config unknowns were invalid".into())
        })?;
    for unknown in unknowns {
        facts.push(project_config_unknown_fact(document, unit, unknown)?);
    }
    Ok(facts)
}

fn optional_project_config_name(value: Option<&Value>) -> Result<Option<&str>, ParseError> {
    match value {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) if is_safe_project_config_name(value) => Ok(Some(value)),
        _ => Err(ParseError::Internal(
            "python project config project name was invalid".to_string(),
        )),
    }
}

fn project_config_string_array<'a>(
    value: Option<&'a Value>,
    label: &'static str,
) -> Result<Vec<&'a str>, ParseError> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Internal(format!("python project config {label} invalid")))?
        .iter()
        .map(|value| {
            value.as_str().ok_or_else(|| {
                ParseError::Internal(format!("python project config {label} invalid"))
            })
        })
        .collect()
}

fn is_safe_project_config_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
}

fn project_config_target(kind: &str, value: &str) -> String {
    format!("python.project_config.{kind}.{value}")
}

fn project_config_structural_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    field: &str,
    target: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: "python".to_string(),
            engine_version: "UNKNOWN".to_string(),
            method: "tomllib".to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: project_config_evidence(document, unit, "Python project config structural fact")?,
        assumptions: vec![
            format!("python_config_field={field}"),
            "parsed_with=tomllib".to_string(),
            "not_family_claim_input".to_string(),
        ],
    })
}

fn project_config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    value: &Value,
) -> Result<SemanticFact, ParseError> {
    let object = value.as_object().ok_or_else(|| {
        ParseError::Internal("python project config UNKNOWN was invalid".to_string())
    })?;
    validate_allowed_keys(
        object,
        &["reason", "affected_claim"],
        "python project config UNKNOWN",
    )?;
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ParseError::Internal("python project config UNKNOWN reason was invalid".to_string())
        })?;
    if !matches!(reason, "MissingProjectConfig" | "MissingDependency") {
        return Err(ParseError::Internal(
            "python project config UNKNOWN reason was unsupported".to_string(),
        ));
    }
    let affected_claim = object
        .get("affected_claim")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ParseError::Internal(
                "python project config UNKNOWN affected claim was invalid".to_string(),
            )
        })?;
    if affected_claim != "python_project_config" {
        return Err(ParseError::Internal(
            "python project config UNKNOWN affected claim was unsupported".to_string(),
        ));
    }
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: "python".to_string(),
            engine_version: "UNKNOWN".to_string(),
            method: "tomllib".to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: project_config_evidence(
            document,
            unit,
            "typed UNKNOWN from Python project config",
        )?,
        assumptions: vec![
            format!("reason_code={reason}"),
            "affected_claim=python_project_config".to_string(),
        ],
    })
}

fn project_config_evidence(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    note: &str,
) -> Result<Evidence, ParseError> {
    Evidence::new(
        unit.id.clone(),
        unit.range.clone(),
        Provenance::new(
            document.path,
            document.content_hash.clone(),
            document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?,
        note,
    )
    .map_err(ParseError::Internal)
}

fn parse_unit(document: &SourceDocument<'_>, value: &Value) -> Result<CodeUnit, ParseError> {
    let object = value
        .as_object()
        .ok_or_else(|| ParseError::Internal("python ast frontend unit was invalid".into()))?;
    validate_allowed_keys(
        object,
        &["name", "kind", "start_byte", "end_byte", "ordinal"],
        "python ast frontend unit",
    )?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::Internal("python ast frontend unit name was invalid".into()))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .and_then(code_unit_kind)
        .ok_or_else(|| ParseError::Internal("python ast frontend unit kind was invalid".into()))?;
    let start_byte = json_usize(object.get("start_byte"))
        .ok_or_else(|| ParseError::Internal("python ast frontend unit range was invalid".into()))?;
    let end_byte = json_usize(object.get("end_byte"))
        .ok_or_else(|| ParseError::Internal("python ast frontend unit range was invalid".into()))?;
    let ordinal = json_usize(object.get("ordinal")).ok_or_else(|| {
        ParseError::Internal("python ast frontend unit ordinal was invalid".into())
    })?;
    if end_byte > document.text.len() {
        return Err(ParseError::Internal(
            "python ast frontend unit exceeded source length".to_string(),
        ));
    }
    let range = SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let id = CodeUnitId::new(format!(
        "unit:{}#{}:{}:{}-{}:{}",
        document.path,
        kind.as_str(),
        slug(name),
        start_byte,
        end_byte,
        ordinal
    ))
    .map_err(ParseError::Internal)?;
    Ok(CodeUnit {
        id,
        language: Language::Python,
        kind,
        range,
        provenance,
    })
}

fn parse_diagnostic(
    document: &SourceDocument<'_>,
    value: &Value,
) -> Result<ParseDiagnostic, ParseError> {
    let object = value
        .as_object()
        .ok_or_else(|| ParseError::Internal("python ast frontend diagnostic was invalid".into()))?;
    validate_allowed_keys(
        object,
        &["severity", "message", "start_byte", "end_byte"],
        "python ast frontend diagnostic",
    )?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ParseError::Internal("python ast frontend diagnostic message invalid".into())
        })?;
    let severity = match object.get("severity").and_then(Value::as_str) {
        Some("error") => ParseDiagnosticSeverity::Error,
        Some("warning") => ParseDiagnosticSeverity::Warning,
        _ => {
            return Err(ParseError::Internal(
                "python ast frontend diagnostic severity invalid".into(),
            ));
        }
    };
    let range = match (
        json_usize(object.get("start_byte")),
        json_usize(object.get("end_byte")),
    ) {
        (Some(start), Some(end)) => {
            Some(SourceRange::new(start, end).map_err(ParseError::Internal)?)
        }
        _ => None,
    };
    Ok(ParseDiagnostic {
        path: document.path.to_string(),
        range,
        severity,
        message: message.to_string(),
    })
}

fn parse_fact(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    unit_ids: &BTreeSet<String>,
    value: &Value,
) -> Result<SemanticFact, ParseError> {
    let object = value
        .as_object()
        .ok_or_else(|| ParseError::Internal("python ast frontend fact was invalid".into()))?;
    validate_allowed_keys(
        object,
        &[
            "fact_kind",
            "subject",
            "target",
            "origin",
            "certainty",
            "evidence",
            "assumptions",
        ],
        "python ast frontend fact",
    )?;
    let kind = object
        .get("fact_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::Internal("python ast frontend fact kind was invalid".into()))
        .and_then(|kind| {
            SemanticFactKind::parse_protocol_str(kind).map_err(ParseError::Internal)
        })?;
    let certainty = object
        .get("certainty")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ParseError::Internal("python ast frontend fact certainty was invalid".into())
        })
        .and_then(|certainty| {
            FactCertainty::parse_protocol_str(certainty).map_err(ParseError::Internal)
        })?;
    validate_python_fact_kind_certainty(kind.clone(), certainty)?;

    let subject = required_protocol_text(
        object,
        "subject",
        "python ast frontend fact subject",
        MAX_PYTHON_FACT_TEXT_BYTES,
    )?;
    let target = match object.get("target") {
        Some(Value::Null) | None => None,
        Some(value) => Some(
            SymbolId::new(protocol_text(
                value.as_str().ok_or_else(|| {
                    ParseError::Internal("python ast frontend fact target was invalid".into())
                })?,
                "python ast frontend fact target",
                MAX_PYTHON_FACT_TARGET_BYTES,
            )?)
            .map_err(ParseError::Internal)?,
        ),
    };
    let origin = parse_origin(object.get("origin"))?;
    let evidence = parse_fact_evidence(document, units, unit_ids, object.get("evidence"))?;
    if subject != evidence.code_unit_id.as_str() {
        return Err(ParseError::Internal(
            "python ast frontend fact subject must match evidence code unit".to_string(),
        ));
    }
    validate_python_fact_target(
        kind.clone(),
        certainty,
        target.as_ref().map(SymbolId::as_str),
    )?;
    validate_python_fact_note(kind.clone(), certainty, &evidence.note)?;
    let assumptions = object
        .get("assumptions")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ParseError::Internal("python ast frontend fact assumptions were invalid".into())
        })?;
    if assumptions.len() > MAX_PYTHON_FACT_ASSUMPTIONS {
        return Err(ParseError::Internal(
            "python ast frontend fact had too many assumptions".to_string(),
        ));
    }
    let assumptions = assumptions
        .iter()
        .map(|value| {
            let value = value.as_str().ok_or_else(|| {
                ParseError::Internal("python ast frontend fact assumption was invalid".into())
            })?;
            protocol_text(
                value,
                "python ast frontend fact assumption",
                MAX_PYTHON_FACT_ASSUMPTION_BYTES,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_python_fact_assumptions(
        kind.clone(),
        certainty,
        target.as_ref().map(SymbolId::as_str),
        &assumptions,
    )?;

    Ok(SemanticFact {
        kind,
        subject,
        target,
        origin,
        certainty,
        evidence,
        assumptions,
    })
}

fn parse_origin(value: Option<&Value>) -> Result<FactOrigin, ParseError> {
    let object = value.and_then(Value::as_object).ok_or_else(|| {
        ParseError::Internal("python ast frontend fact origin was invalid".into())
    })?;
    validate_allowed_keys(
        object,
        &["engine", "engine_version", "method"],
        "python ast frontend fact origin",
    )?;
    let origin = FactOrigin {
        engine: required_protocol_text(
            object,
            "engine",
            "python ast frontend fact origin engine",
            MAX_PYTHON_FACT_TEXT_BYTES,
        )?,
        engine_version: required_protocol_text(
            object,
            "engine_version",
            "python ast frontend fact origin engine version",
            MAX_PYTHON_FACT_TEXT_BYTES,
        )?,
        method: required_protocol_text(
            object,
            "method",
            "python ast frontend fact origin method",
            MAX_PYTHON_FACT_TEXT_BYTES,
        )?,
    };
    if origin.engine != "python" || !matches!(origin.method.as_str(), "cpython_ast" | "tomllib") {
        return Err(ParseError::Internal(
            "python ast frontend fact origin was unsupported".to_string(),
        ));
    }
    Ok(origin)
}

fn parse_fact_evidence(
    document: &SourceDocument<'_>,
    units: &[CodeUnit],
    unit_ids: &BTreeSet<String>,
    value: Option<&Value>,
) -> Result<Evidence, ParseError> {
    let object = value.and_then(Value::as_object).ok_or_else(|| {
        ParseError::Internal("python ast frontend fact evidence was invalid".into())
    })?;
    validate_allowed_keys(
        object,
        &[
            "code_unit_id",
            "path",
            "content_hash",
            "repository_revision",
            "start_byte",
            "end_byte",
            "note",
        ],
        "python ast frontend fact evidence",
    )?;
    let code_unit_id = required_protocol_text(
        object,
        "code_unit_id",
        "python ast frontend fact evidence code unit id",
        MAX_PYTHON_FACT_TEXT_BYTES,
    )?;
    if !unit_ids.contains(&code_unit_id) {
        return Err(ParseError::Internal(
            "python ast frontend fact referenced unknown code unit".to_string(),
        ));
    }
    let path = required_protocol_text(
        object,
        "path",
        "python ast frontend fact evidence path",
        MAX_PYTHON_FACT_TEXT_BYTES,
    )?;
    if path != document.path {
        return Err(ParseError::Internal(
            "python ast frontend fact evidence path was invalid".to_string(),
        ));
    }
    let content_hash = required_protocol_text(
        object,
        "content_hash",
        "python ast frontend fact evidence content hash",
        MAX_PYTHON_FACT_TEXT_BYTES,
    )?;
    if content_hash != document.content_hash.as_str() {
        return Err(ParseError::Internal(
            "python ast frontend fact evidence hash was invalid".to_string(),
        ));
    }
    let repository_revision = required_protocol_text(
        object,
        "repository_revision",
        "python ast frontend fact evidence repository revision",
        MAX_PYTHON_FACT_TEXT_BYTES,
    )?;
    if repository_revision != document.repository_revision.as_str() {
        return Err(ParseError::Internal(
            "python ast frontend fact evidence revision was invalid".to_string(),
        ));
    }
    let start_byte = json_usize(object.get("start_byte")).ok_or_else(|| {
        ParseError::Internal("python ast frontend fact evidence range was invalid".into())
    })?;
    let end_byte = json_usize(object.get("end_byte")).ok_or_else(|| {
        ParseError::Internal("python ast frontend fact evidence range was invalid".into())
    })?;
    if end_byte > document.text.len() {
        return Err(ParseError::Internal(
            "python ast frontend fact evidence exceeded source length".to_string(),
        ));
    }
    let unit = units
        .iter()
        .find(|unit| unit.id.as_str() == code_unit_id)
        .ok_or_else(|| {
            ParseError::Internal("python ast frontend fact referenced unknown code unit".into())
        })?;
    if start_byte < unit.range.start_byte || end_byte > unit.range.end_byte {
        return Err(ParseError::Internal(
            "python ast frontend fact evidence must stay within its code unit".to_string(),
        ));
    }
    let range = SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        RepositoryRevision::new(repository_revision).map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)?;
    Evidence::new(
        CodeUnitId::new(code_unit_id).map_err(ParseError::Internal)?,
        range,
        provenance,
        required_protocol_text(
            object,
            "note",
            "python ast frontend fact evidence note",
            MAX_PYTHON_FACT_NOTE_BYTES,
        )?,
    )
    .map_err(ParseError::Internal)
}

fn validate_allowed_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &'static str,
) -> Result<(), ParseError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ParseError::Internal(format!(
                "{context} contained unsupported field"
            )));
        }
    }
    Ok(())
}

fn required_protocol_text(
    object: &Map<String, Value>,
    key: &str,
    label: &'static str,
    max_bytes: usize,
) -> Result<String, ParseError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::Internal(format!("{label} was invalid")))?;
    protocol_text(value, label, max_bytes)
}

fn protocol_text(value: &str, label: &'static str, max_bytes: usize) -> Result<String, ParseError> {
    if value.trim().is_empty() {
        return Err(ParseError::Internal(format!("{label} must not be empty")));
    }
    if value.len() > max_bytes
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains("://")
        || value.split_whitespace().any(|token| {
            std::path::Path::new(token).is_absolute()
                || crate::core::policy::paths::looks_like_windows_absolute_path(token)
        })
        || looks_like_python_source_snippet(value)
    {
        Err(ParseError::Internal(format!(
            "{label} contained unsupported content"
        )))
    } else {
        Ok(value.to_string())
    }
}

fn looks_like_python_source_snippet(value: &str) -> bool {
    let trimmed = value.trim_start();
    value.contains("=>")
        || (value.contains('=') && value.contains(';'))
        || value.contains('{')
        || value.contains('}')
        || trimmed.starts_with("def ")
        || trimmed.starts_with("async def ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("@")
}

fn validate_python_fact_kind_certainty(
    kind: SemanticFactKind,
    certainty: FactCertainty,
) -> Result<(), ParseError> {
    match (kind, certainty) {
        (
            SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type,
            FactCertainty::Structural,
        )
        | (SemanticFactKind::Unknown, FactCertainty::Unknown) => Ok(()),
        _ => Err(ParseError::Internal(
            "python ast frontend fact kind/certainty was unsupported".to_string(),
        )),
    }
}

fn validate_python_fact_target(
    kind: SemanticFactKind,
    certainty: FactCertainty,
    target: Option<&str>,
) -> Result<(), ParseError> {
    match (kind, certainty, target) {
        (SemanticFactKind::Unknown, FactCertainty::Unknown, Some(target))
            if python_unknown_reason_is_supported(target) =>
        {
            Ok(())
        }
        (
            SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type,
            FactCertainty::Structural,
            Some(target),
        ) if python_structural_target_is_supported(target) => Ok(()),
        _ => Err(ParseError::Internal(
            "python ast frontend fact target was unsupported".to_string(),
        )),
    }
}

fn validate_python_fact_assumptions(
    kind: SemanticFactKind,
    certainty: FactCertainty,
    target: Option<&str>,
    assumptions: &[String],
) -> Result<(), ParseError> {
    match (kind, certainty) {
        (SemanticFactKind::Unknown, FactCertainty::Unknown) => {
            let Some(target) = target else {
                return Err(ParseError::Internal(
                    "python ast frontend UNKNOWN assumptions were unsupported".to_string(),
                ));
            };
            let reason = format!("reason_code={target}");
            let affected_claim = assumptions
                .iter()
                .find_map(|value| value.strip_prefix("affected_claim="));
            if assumptions.len() == 2
                && assumptions.iter().any(|value| value == &reason)
                && affected_claim.is_some_and(python_affected_claim_is_supported)
            {
                Ok(())
            } else {
                Err(ParseError::Internal(
                    "python ast frontend UNKNOWN assumptions were unsupported".to_string(),
                ))
            }
        }
        (
            SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type,
            FactCertainty::Structural,
        ) => {
            let anchor = assumptions
                .iter()
                .find_map(|value| value.strip_prefix("python_anchor_kind="));
            let has_boundary = assumptions
                .iter()
                .any(|value| value == "binding unresolved without provider");
            if assumptions.len() == 2
                && anchor.is_some_and(python_anchor_kind_is_supported)
                && has_boundary
            {
                Ok(())
            } else {
                Err(ParseError::Internal(
                    "python ast frontend structural assumptions were unsupported".to_string(),
                ))
            }
        }
        _ => Err(ParseError::Internal(
            "python ast frontend assumptions were unsupported".to_string(),
        )),
    }
}

fn validate_python_fact_note(
    kind: SemanticFactKind,
    certainty: FactCertainty,
    note: &str,
) -> Result<(), ParseError> {
    match (kind, certainty) {
        (SemanticFactKind::Unknown, FactCertainty::Unknown)
            if note.starts_with("typed UNKNOWN ") =>
        {
            Ok(())
        }
        (
            SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type,
            FactCertainty::Structural,
        ) if note.starts_with("CPython ast structural ")
            || note.starts_with("CPython tomllib structural ") =>
        {
            Ok(())
        }
        _ => Err(ParseError::Internal(
            "python ast frontend fact note was unsupported".to_string(),
        )),
    }
}

fn python_structural_target_is_supported(value: &str) -> bool {
    if value.len() > MAX_PYTHON_FACT_TARGET_BYTES
        || value.contains(char::is_whitespace)
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains(';')
        || value.contains('(')
        || value.contains(')')
        || value.contains('[')
        || value.contains(']')
        || value.contains('{')
        || value.contains('}')
    {
        return false;
    }
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-'))
}

fn python_unknown_reason_is_supported(value: &str) -> bool {
    matches!(
        value,
        "DynamicImport"
            | "MonkeyPatch"
            | "PytestFixtureInjection"
            | "RuntimeDependencyInjection"
            | "UnresolvedImport"
            | "MissingProjectConfig"
            | "MissingDependency"
            | "FrameworkMagic"
            | "ConflictingFacts"
            | "StaleEvidence"
            | "InsufficientSupport"
    )
}

fn python_affected_claim_is_supported(value: &str) -> bool {
    matches!(
        value,
        "python_import_resolution"
            | "python_call_target"
            | "pytest_fixture_binding"
            | "python_project_config"
    )
}

fn python_anchor_kind_is_supported(value: &str) -> bool {
    matches!(
        value,
        "import_binding"
            | "dynamic_import_literal"
            | "decorator_binding"
            | "class_base"
            | "call_target"
            | "pytest_test_function"
            | "pytest_fixture_edge"
            | "pytest_conftest_fixture_edge"
            | "sqlalchemy_mapped_field"
            | "sqlalchemy_mapped_column"
            | "module_name"
            | "scope_imported"
            | "scope_namespace"
            | "scope_assigned"
            | "repo_local_import_binding"
            | "project_config"
            | "project_config_name"
            | "project_config_tool"
            | "project_config_source_root"
    )
}

fn sort_semantic_facts(facts: &mut [SemanticFact]) {
    facts.sort_by(|left, right| {
        (
            left.evidence.provenance.path.as_str(),
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
            left.evidence.code_unit_id.as_str(),
            left.kind.as_protocol_str(),
            left.subject.as_str(),
            left.target.as_ref().map(SymbolId::as_str),
            left.certainty.as_protocol_str(),
            left.origin.engine.as_str(),
            left.origin.engine_version.as_str(),
            left.origin.method.as_str(),
        )
            .cmp(&(
                right.evidence.provenance.path.as_str(),
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.evidence.code_unit_id.as_str(),
                right.kind.as_protocol_str(),
                right.subject.as_str(),
                right.target.as_ref().map(SymbolId::as_str),
                right.certainty.as_protocol_str(),
                right.origin.engine.as_str(),
                right.origin.engine_version.as_str(),
                right.origin.method.as_str(),
            ))
    });
}

fn json_usize(value: Option<&Value>) -> Option<usize> {
    value?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
}

fn code_unit_kind(value: &str) -> Option<CodeUnitKind> {
    match value {
        "module" => Some(CodeUnitKind::Module),
        "function" => Some(CodeUnitKind::Function),
        "async_function" => Some(CodeUnitKind::AsyncFunction),
        "class" => Some(CodeUnitKind::Class),
        "method" => Some(CodeUnitKind::Method),
        "fastapi_route" => Some(CodeUnitKind::FastApiRoute),
        "pytest_test" => Some(CodeUnitKind::PytestTest),
        "pytest_fixture" => Some(CodeUnitKind::PytestFixture),
        "pydantic_model" => Some(CodeUnitKind::PydanticModel),
        "sqlalchemy_model" => Some(CodeUnitKind::SqlAlchemyModel),
        "sqlalchemy_repository_method" => Some(CodeUnitKind::SqlAlchemyRepositoryMethod),
        "project_config" => Some(CodeUnitKind::ProjectConfig),
        _ => None,
    }
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() || character == '_' {
            slug.push(character);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        "anonymous".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};
    use crate::ports::parser::ParserProjectFileContext;

    fn document(text: &str) -> SourceDocument<'_> {
        document_at("app.py", text)
    }

    fn document_at<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::Python,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    fn project_config_document(text: &str) -> SourceDocument<'_> {
        SourceDocument {
            path: "pyproject.toml",
            language: Language::PythonConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    #[test]
    fn cpython_frontend_extracts_python_units_and_facts_without_snippets() {
        let source = r#"
from fastapi import APIRouter
from pydantic import BaseModel
router = APIRouter()

class UserOut(BaseModel):
    id: int

@router.get("/users")
async def list_users():
    return []

def test_users(client):
    assert client.get("/users").status_code == 200
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");
        let kinds = report
            .units
            .iter()
            .map(|unit| unit.kind.as_str())
            .collect::<Vec<_>>();

        assert!(kinds.contains(&"module"));
        assert!(kinds.contains(&"pydantic_model"));
        assert!(kinds.contains(&"fastapi_route"));
        assert!(kinds.contains(&"pytest_test"));
        assert!(report.diagnostics.is_empty());
        assert!(report
            .units
            .iter()
            .all(|unit| unit.provenance.path == "app.py" && unit.language == Language::Python));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedImport
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("app")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("scope.imported.APIRouter")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("scope.namespace.UserOut")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("scope.assigned.router")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.BaseModel")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("client.get")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.test")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_test_function")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
        }));
        for fact in &report.semantic_facts {
            assert!(matches!(
                fact.certainty,
                FactCertainty::Structural | FactCertainty::Unknown
            ));
            assert_eq!(fact.origin.engine, "python");
            assert_eq!(fact.origin.method, "cpython_ast");
            assert_eq!(fact.evidence.provenance.path, "app.py");
            assert_eq!(fact.subject, fact.evidence.code_unit_id.as_str());
            assert!(report
                .units
                .iter()
                .any(|unit| unit.id == fact.evidence.code_unit_id
                    && fact.evidence.range.start_byte >= unit.range.start_byte
                    && fact.evidence.range.end_byte <= unit.range.end_byte));
        }
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("from fastapi"));
        assert!(!debug.contains("@router.get"));
        assert!(!debug.contains("assert client.get"));
    }

    #[test]
    fn cpython_frontend_propagates_simple_framework_aliases() {
        let source = r#"
from fastapi import APIRouter

router = APIRouter()
api = router
v1 = api

@v1.get("/users")
def list_users():
    return []
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "fastapi_route"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=decorator_binding")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("@v1.get"));
    }

    #[test]
    fn cpython_frontend_does_not_keep_shadowed_framework_aliases() {
        let source = r#"
from fastapi import APIRouter

router = APIRouter()
api = router
api = object()

@api.get("/users")
def list_users():
    return []
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "fastapi_route"));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("@api.get"));
    }

    #[test]
    fn cpython_frontend_extracts_pydantic_settings_bases() {
        let source = r#"
from pydantic import BaseSettings as LegacyBaseSettings
from pydantic_settings import BaseSettings

class LegacySettings(LegacyBaseSettings):
    debug: bool = False

class AppSettings(BaseSettings):
    debug: bool = False
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert_eq!(
            report
                .units
                .iter()
                .filter(|unit| unit.kind.as_str() == "pydantic_model")
                .count(),
            2
        );
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.BaseSettings")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("pydantic_settings.BaseSettings")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("from pydantic import"));
    }

    #[test]
    fn cpython_frontend_extracts_sqlalchemy_model_field_anchors() {
        let source = r#"
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import Mapped, Session, mapped_column

class User:
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)

class Account:
    __tablename__ = "accounts"
    id: Mapped[int] = mapped_column(primary_key=True)

class UserRepository:
    def list_users(self, session: Session):
        return session.execute("select users")

    async def list_accounts(self, db: AsyncSession):
        return await db.execute("select accounts")
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert_eq!(
            report
                .units
                .iter()
                .filter(|unit| unit.kind.as_str() == "sqlalchemy_model")
                .count(),
            2
        );
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.Mapped")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_mapped_field")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.mapped_column")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_mapped_column")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.execute")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("mapped_column(primary_key=True)"));
    }

    #[test]
    fn cpython_frontend_does_not_treat_local_sqlalchemy_names_as_exact_anchors() {
        let source = r#"
class Mapped:
    pass

def mapped_column():
    return object()

class User:
    __tablename__ = "users"
    id: Mapped[int] = mapped_column()
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "sqlalchemy_model"));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.Mapped")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.mapped_column")
        }));
    }

    #[test]
    fn project_config_frontend_synthesizes_structural_unit_and_facts_without_leaks() {
        let source = r#"
[project]
name = "demo-api"

[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]

[tool.pyrefly]
project_includes = ["src"]
"#;

        let document = project_config_document(source);
        let response = json!({
            "protocol_version": 1,
            "mode": "parse_project_config",
            "path": "pyproject.toml",
            "config": {
                "project_name": "demo-api",
                "source_roots": ["src", "src/lib", "tests"],
                "tool_sections": ["pyrefly", "pyright", "pytest"]
            },
            "unknowns": []
        })
        .to_string();
        let report =
            parse_project_config_response(&document, &response).expect("parse project config");

        assert_eq!(report.units.len(), 1);
        let unit = &report.units[0];
        assert_eq!(unit.language, Language::PythonConfig);
        assert_eq!(unit.kind, CodeUnitKind::ProjectConfig);
        assert_eq!(unit.range.start_byte, 0);
        assert_eq!(unit.range.end_byte, source.len());
        assert_eq!(unit.provenance.path, "pyproject.toml");
        assert_eq!(report.ir_nodes.len(), 1);
        assert!(report.ir_edges.is_empty());
        assert!(report.diagnostics.is_empty());

        let targets = report
            .semantic_facts
            .iter()
            .map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<Vec<_>>();
        assert!(targets.contains(&Some("python.project_config.project_name.demo-api")));
        assert!(targets.contains(&Some("python.project_config.source_root.src.lib")));
        assert!(targets.contains(&Some("python.project_config.tool_section.pyright")));
        assert!(report.semantic_facts.iter().all(|fact| {
            fact.kind == SemanticFactKind::ProjectConfig
                && fact.certainty == FactCertainty::Structural
                && fact.origin.engine == "python"
                && fact.origin.method == "tomllib"
                && fact.subject == unit.id.as_str()
                && fact.evidence.code_unit_id == unit.id
                && fact.evidence.provenance.path == "pyproject.toml"
                && fact.evidence.provenance.content_hash == unit.provenance.content_hash
                && fact.evidence.range.start_byte == 0
                && fact.evidence.range.end_byte == source.len()
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "not_family_claim_input")
        }));

        let debug = format!("{:?}", report);
        for forbidden in ["../secret", "/tmp/secret", "C:/secret", "project_includes"] {
            assert!(
                !debug.contains(forbidden),
                "project config leaked forbidden text {forbidden}"
            );
        }
    }

    #[test]
    fn malformed_project_config_becomes_typed_unknown_without_leaking_toml() {
        let source = "[project\nname = 'broken'\n";
        let report = PythonAstParser::default()
            .parse(project_config_document(source))
            .expect("malformed project config is represented as UNKNOWN");

        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && matches!(
                    fact.target.as_ref().map(SymbolId::as_str),
                    Some("MissingProjectConfig" | "MissingDependency")
                )
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_project_config")
        }));
        assert!(!format!("{:?}", report).contains("[project"));
    }

    #[test]
    fn parse_with_context_accepts_repo_local_import_binding_without_claim_upgrade() {
        let source = "\
from acme.services import users\n\
from .services import users as relative_users\n\
from acme.missing import value\n";
        let context = ParserProjectContext {
            python_module_paths: vec![
                "src/acme/api.py".to_string(),
                "src/acme/__init__.py".to_string(),
                "src/acme/services/__init__.py".to_string(),
                "src/acme/services/users.py".to_string(),
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: Vec::new(),
        };
        let parser = PythonAstParser::default();
        let report = parser
            .parse_with_context(document_at("src/acme/api.py", source), &context)
            .expect("parse with repo-local context");
        let mut reversed_context = context.clone();
        reversed_context.python_module_paths.reverse();
        let reversed_report = parser
            .parse_with_context(document_at("src/acme/api.py", source), &reversed_context)
            .expect("parse with reordered repo-local context");
        assert_eq!(report.semantic_facts, reversed_report.semantic_facts);

        let repo_local_imports = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::ResolvedImport
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("acme.services.users")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=repo_local_import_binding"
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(repo_local_imports.len(), 2);
        assert!(repo_local_imports.iter().all(|fact| {
            fact.certainty == FactCertainty::Structural
                && fact.origin.engine == "python"
                && fact.origin.method == "cpython_ast"
                && fact.evidence.provenance.path == "src/acme/api.py"
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "reason_code=UnresolvedImport")
        }));
        assert!(report.semantic_facts.iter().all(|fact| {
            matches!(
                fact.certainty,
                FactCertainty::Structural | FactCertainty::Unknown
            )
        }));
        let debug = format!("{:?}", report.semantic_facts);
        for forbidden in [
            "from acme.services",
            "from .services",
            "src/acme/services/users.py",
        ] {
            assert!(
                !debug.contains(forbidden),
                "parser facts leaked forbidden text {forbidden}"
            );
        }
    }

    #[test]
    fn parse_with_context_accepts_conftest_fixture_edges_without_source_leakage() {
        let source = "def test_users(client, missing_fixture):\n    assert client is not None\n";
        let context = ParserProjectContext {
            python_module_paths: vec![
                "tests/conftest.py".to_string(),
                "tests/sub/test_api.py".to_string(),
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: vec![ParserProjectFileContext {
                path: "tests/conftest.py".to_string(),
                text: "import pytest\n\n@pytest.fixture\ndef client():\n    return object()\n"
                    .to_string(),
            }],
        };
        let parser = PythonAstParser::default();
        let report = parser
            .parse_with_context(document_at("tests/sub/test_api.py", source), &context)
            .expect("parse with conftest context");

        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.client")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                    })
            }),
            "{:?}",
            report.semantic_facts
        );
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=pytest_fixture_binding")
        }));
        let debug = format!("{:?}", report);
        assert!(!debug.contains("tests/conftest.py"));
        assert!(!debug.contains("return object"));
        assert!(!debug.contains("missing_fixture"));
    }

    #[test]
    fn parse_document_rejects_protocol_drift_and_unsafe_fact_text() {
        let source = "def ok():\n    pass\n";
        let mut valid = valid_response(source, vec![valid_structural_fact(source)]);
        assert!(parse_worker_response(&document(source), &valid).is_ok());

        valid.push_str("\n{}");
        assert!(matches!(
            parse_worker_response(&document(source), &valid),
            Err(ParseError::Internal(_))
        ));

        let mut response = valid_response(source, vec![valid_structural_fact(source)]);
        let mut value: Value = serde_json::from_str(&response).expect("response JSON");
        value["snippet"] = json!("def ok(): pass");
        response = value.to_string();
        assert!(matches!(
            parse_worker_response(&document(source), &response),
            Err(ParseError::Internal(_))
        ));

        let mut response = valid_response(source, vec![valid_structural_fact(source)]);
        let mut value: Value = serde_json::from_str(&response).expect("response JSON");
        value["facts"][0]["target"] = json!("def leaked(): pass");
        response = value.to_string();
        assert!(matches!(
            parse_worker_response(&document(source), &response),
            Err(ParseError::Internal(_))
        ));

        let mut response = valid_response(source, vec![valid_structural_fact(source)]);
        let mut value: Value = serde_json::from_str(&response).expect("response JSON");
        value["facts"][0]["subject"] = json!("unit:app.py#function:other:0-1:99");
        response = value.to_string();
        assert!(matches!(
            parse_worker_response(&document(source), &response),
            Err(ParseError::Internal(_))
        ));

        let mut response = valid_response(source, vec![valid_unknown_fact(source)]);
        let mut value: Value = serde_json::from_str(&response).expect("response JSON");
        value["facts"][0]["origin"]["method"] = json!("pyright");
        response = value.to_string();
        assert!(matches!(
            parse_worker_response(&document(source), &response),
            Err(ParseError::Internal(_))
        ));

        let mut response = valid_response(source, vec![valid_unknown_fact(source)]);
        let mut value: Value = serde_json::from_str(&response).expect("response JSON");
        value["facts"][0]["assumptions"][0] = json!("reason_code=FrameworkMagic");
        response = value.to_string();
        assert!(matches!(
            parse_worker_response(&document(source), &response),
            Err(ParseError::Internal(_))
        ));
    }

    #[test]
    fn cpython_frontend_reports_syntax_errors_without_units() {
        let report = PythonAstParser::default()
            .parse(document("def broken(:\n"))
            .expect("syntax errors are diagnostics");

        assert!(report.units.is_empty());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
    }

    #[test]
    fn rejects_non_python_documents() {
        let mut document = document("export const x = 1;\n");
        document.language = Language::TypeScript;

        assert!(matches!(
            PythonAstParser::default().parse(document),
            Err(ParseError::UnsupportedLanguage)
        ));
    }

    #[test]
    fn missing_frontend_is_reported_as_internal_error() {
        let parser = PythonAstParser::with_worker("python3", PathBuf::from("missing-worker.py"));

        assert!(matches!(
            parser.parse(document("def ok():\n    pass\n")),
            Err(ParseError::Internal(_))
        ));
    }

    fn valid_response(source: &str, facts: Vec<Value>) -> String {
        json!({
            "protocol_version": 1,
            "mode": "parse_document",
            "path": "app.py",
            "units": [{
                "name": "module",
                "kind": "module",
                "start_byte": 0,
                "end_byte": source.len(),
                "ordinal": 0
            }],
            "facts": facts,
            "diagnostics": []
        })
        .to_string()
    }

    fn module_unit_id(source: &str) -> String {
        format!("unit:app.py#module:module:0-{}:0", source.len())
    }

    fn valid_structural_fact(source: &str) -> Value {
        let unit_id = module_unit_id(source);
        json!({
            "fact_kind": "RESOLVED_IMPORT",
            "subject": unit_id,
            "target": "fastapi.APIRouter",
            "origin": {
                "engine": "python",
                "engine_version": "3.12.0",
                "method": "cpython_ast"
            },
            "certainty": "STRUCTURAL",
            "evidence": {
                "code_unit_id": module_unit_id(source),
                "path": "app.py",
                "content_hash": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "repository_revision": "UNKNOWN",
                "start_byte": 0,
                "end_byte": source.len(),
                "note": "CPython ast structural import_binding"
            },
            "assumptions": [
                "python_anchor_kind=import_binding",
                "binding unresolved without provider"
            ]
        })
    }

    fn valid_unknown_fact(source: &str) -> Value {
        let unit_id = module_unit_id(source);
        json!({
            "fact_kind": "UNKNOWN",
            "subject": unit_id,
            "target": "DynamicImport",
            "origin": {
                "engine": "python",
                "engine_version": "3.12.0",
                "method": "cpython_ast"
            },
            "certainty": "UNKNOWN",
            "evidence": {
                "code_unit_id": module_unit_id(source),
                "path": "app.py",
                "content_hash": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "repository_revision": "UNKNOWN",
                "start_byte": 0,
                "end_byte": source.len(),
                "note": "typed UNKNOWN DynamicImport for python_import_resolution"
            },
            "assumptions": [
                "reason_code=DynamicImport",
                "affected_claim=python_import_resolution"
            ]
        })
    }
}
