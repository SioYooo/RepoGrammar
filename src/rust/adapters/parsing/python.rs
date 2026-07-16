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
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Origin engine stamped on facts produced by the CPython `ast` frontend. Used
/// to gate which UNKNOWN facts are trusted to affect Python family membership.
pub(crate) const PYTHON_ANCHOR_ENGINE: &str = "python";

// A source file can legitimately produce substantially more metadata than its
// input bytes while remaining below the worker's 2,000-fact bound. Keep stdout
// bounded, but leave enough room for the bundled worker to analyze itself.
const MAX_PYTHON_FRONTEND_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
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
            executable: default_python_executable(|key| std::env::var(key).ok()),
            worker_script: default_python_worker_script(),
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

fn default_python_executable<F>(env_lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    env_lookup("REPOGRAMMAR_PYTHON_EXECUTABLE")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| platform_python_executable().to_string())
}

#[cfg(windows)]
fn platform_python_executable() -> &'static str {
    "python"
}

#[cfg(not(windows))]
fn platform_python_executable() -> &'static str {
    "python3"
}

fn default_python_worker_script() -> PathBuf {
    if let Ok(worker) = std::env::var("REPOGRAMMAR_PYTHON_WORKER") {
        if !worker.trim().is_empty() {
            return PathBuf::from(worker);
        }
    }
    let source_worker = source_checkout_python_worker_script();
    if let Ok(executable) = std::env::current_exe() {
        for candidate in python_worker_script_candidates(&executable) {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    source_worker
}

fn source_checkout_python_worker_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/workers/python/worker.py")
}

fn python_worker_script_candidates(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(canonical) = fs::canonicalize(executable) {
        if canonical != executable {
            push_python_worker_script_candidates(&canonical, &mut candidates);
        }
    }
    push_python_worker_script_candidates(executable, &mut candidates);
    candidates
}

fn push_python_worker_script_candidates(executable: &Path, candidates: &mut Vec<PathBuf>) {
    let Some(executable_dir) = executable.parent() else {
        return;
    };
    push_unique_candidate(
        candidates,
        executable_dir.join("repogrammar-workers/python/worker.py"),
    );
    if let Some(prefix) = executable_dir.parent() {
        push_unique_candidate(
            candidates,
            prefix.join("share/repogrammar/workers/python/worker.py"),
        );
        push_unique_candidate(candidates, prefix.join("workers/python/worker.py"));
    }
    push_unique_candidate(candidates, executable_dir.join("workers/python/worker.py"));
}

fn push_unique_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
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
            "module_files".to_string(),
            json!(context
                .python_module_files
                .iter()
                .map(|file| json!({
                    "path": &file.path,
                    "text": &file.text,
                }))
                .collect::<Vec<_>>()),
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

pub(crate) fn python_project_config_parser_method(path: &str) -> Option<&'static str> {
    match path {
        "pyproject.toml" => Some("tomllib"),
        "setup.cfg" => Some("configparser"),
        "setup.py" => Some("cpython_ast"),
        _ => None,
    }
}

fn is_python_project_config_path(path: &str) -> bool {
    python_project_config_parser_method(path).is_some()
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
            Vec::new(),
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
            vec![format!("python_config_source_root={root}")],
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
            Vec::new(),
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
    extra_assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    let parser_method = python_project_config_parser_method(document.path).ok_or_else(|| {
        ParseError::Internal("python project config parser method was unavailable".to_string())
    })?;
    let mut assumptions = vec![
        format!("python_config_field={field}"),
        format!("parsed_with={parser_method}"),
        "not_family_claim_input".to_string(),
    ];
    assumptions.extend(extra_assumptions);
    Ok(SemanticFact {
        kind: SemanticFactKind::ProjectConfig,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: "python".to_string(),
            engine_version: "UNKNOWN".to_string(),
            method: parser_method.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: project_config_evidence(document, unit, "Python project config structural fact")?,
        assumptions,
    })
}

fn project_config_unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    value: &Value,
) -> Result<SemanticFact, ParseError> {
    let parser_method = python_project_config_parser_method(document.path).ok_or_else(|| {
        ParseError::Internal("python project config parser method was unavailable".to_string())
    })?;
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
    if !matches!(
        reason,
        "MissingProjectConfig" | "MissingDependency" | "ConflictingFacts"
    ) {
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
            method: parser_method.to_string(),
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
            format!("parsed_with={parser_method}"),
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
    match (kind.clone(), certainty) {
        (
            SemanticFactKind::ResolvedCall
            | SemanticFactKind::ResolvedImport
            | SemanticFactKind::Symbol
            | SemanticFactKind::Type,
            FactCertainty::Structural,
        )
        | (
            SemanticFactKind::ResolvedImport | SemanticFactKind::Symbol | SemanticFactKind::Type,
            FactCertainty::DataflowDerived,
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
        (
            SemanticFactKind::ResolvedImport | SemanticFactKind::Symbol | SemanticFactKind::Type,
            FactCertainty::DataflowDerived,
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
    match (kind.clone(), certainty) {
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
            let default_structural_assumptions = assumptions.len() == 2
                && anchor.is_some_and(python_anchor_kind_is_supported)
                && has_boundary;
            let relationship_context_assumptions = assumptions.len() == 4
                && anchor == Some("sqlalchemy_relationship_target")
                && has_boundary
                && assumptions
                    .iter()
                    .any(|value| value == "fact_scope=context_only")
                && assumptions
                    .iter()
                    .any(|value| value == "relationship_target_binding=local_literal");
            if default_structural_assumptions || relationship_context_assumptions {
                Ok(())
            } else {
                Err(ParseError::Internal(
                    "python ast frontend structural assumptions were unsupported".to_string(),
                ))
            }
        }
        (
            SemanticFactKind::ResolvedImport | SemanticFactKind::Symbol | SemanticFactKind::Type,
            FactCertainty::DataflowDerived,
        ) => {
            let anchor = assumptions
                .iter()
                .find_map(|value| value.strip_prefix("python_anchor_kind="));
            let derived_from = assumptions
                .iter()
                .find_map(|value| value.strip_prefix("derived_from="));
            if assumptions.len() == 3
                && assumptions
                    .iter()
                    .any(|value| value == "provider_resolved=false")
                && anchor.is_some_and(|anchor| {
                    python_graph_anchor_kind_is_supported(kind.clone(), anchor, derived_from)
                })
                && derived_from.is_some()
            {
                Ok(())
            } else {
                Err(ParseError::Internal(
                    "python ast frontend graph-derived assumptions were unsupported".to_string(),
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
        (
            SemanticFactKind::ResolvedImport | SemanticFactKind::Symbol | SemanticFactKind::Type,
            FactCertainty::DataflowDerived,
        ) if note.starts_with("CPython ast repo_local_python_import_graph ")
            || note.starts_with("CPython ast repo_local_pytest_fixture_graph ") =>
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
            | "python_framework_identity"
            | "fastapi_dependency_target"
            | "pydantic_validator_side_effects"
            | "sqlalchemy_query_shape"
            | "sqlalchemy_relationship_target"
            | "pytest_fixture_binding"
            | "python_project_config"
            | "python_django_model_identity"
            | "python_django_url_identity"
            | "python_flask_route_identity"
            | "python_cli_command_identity"
            | "python_celery_task_identity"
            | "python_django_string_dispatch"
            | "python_unittest_patch_target"
            | "python_celery_runtime_routing"
            | "python_django_settings_behavior"
    )
}

fn python_anchor_kind_is_supported(value: &str) -> bool {
    matches!(
        value,
        "import_binding"
            | "dynamic_import_literal"
            | "decorator_binding"
            | "fastapi_dependency"
            | "fastapi_dependency_target"
            | "fastapi_http_exception"
            | "fastapi_http_exception_status"
            | "fastapi_cookie_param"
            | "fastapi_header_param"
            | "fastapi_path_param"
            | "fastapi_query_param"
            | "fastapi_request_body_model"
            | "fastapi_response_model"
            | "fastapi_route_decorator"
            | "fastapi_service_call"
            | "class_base"
            | "call_target"
            | "pydantic_computed_field"
            | "pydantic_config_class"
            | "pydantic_field"
            | "pydantic_field_metadata"
            | "pydantic_field_type"
            | "pydantic_model_config"
            | "pydantic_model_validator"
            | "pydantic_validator"
            | "pytest_fixture_decorator"
            | "pytest_parametrize"
            | "pytest_parametrize_arg"
            | "pytest_test_function"
            | "pytest_builtin_fixture_context"
            | "pytest_plugin_fixture_context"
            | "pytest_fixture_edge"
            | "pytest_conftest_fixture_edge"
            | "sqlalchemy_select"
            | "sqlalchemy_session_call"
            | "sqlalchemy_mapped_field"
            | "sqlalchemy_mapped_column"
            | "sqlalchemy_relationship"
            | "sqlalchemy_relationship_target"
            | "django_url_route"
            | "django_model_field"
            | "django_model_meta"
            | "django_test_method"
            | "flask_route_decorator"
            | "flask_route_method"
            | "unittest_test_method"
            | "unittest_fixture"
            | "click_command_decorator"
            | "typer_command_decorator"
            | "cli_param_count"
            | "celery_task_decorator"
            | "module_name"
            | "scope_imported"
            | "scope_namespace"
            | "scope_assigned"
            | "repo_local_import_binding"
            | "repo_local_import_symbol"
            | "project_config"
            | "project_config_name"
            | "project_config_tool"
            | "project_config_source_root"
    )
}

fn python_graph_anchor_kind_is_supported(
    kind: SemanticFactKind,
    anchor: &str,
    derived_from: Option<&str>,
) -> bool {
    matches!(
        (kind, anchor, derived_from),
        (
            SemanticFactKind::ResolvedImport,
            "repo_local_import_binding",
            Some("repo_local_python_import_graph")
        ) | (
            SemanticFactKind::Symbol | SemanticFactKind::Type,
            "repo_local_import_symbol",
            Some("repo_local_python_import_graph")
        ) | (
            SemanticFactKind::Symbol,
            "pytest_fixture_edge" | "pytest_conftest_fixture_edge",
            Some("repo_local_pytest_fixture_graph")
        )
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
        "django_model" => Some(CodeUnitKind::DjangoModel),
        "django_url_pattern" => Some(CodeUnitKind::DjangoUrlPattern),
        "django_test" => Some(CodeUnitKind::DjangoTest),
        "flask_route" => Some(CodeUnitKind::FlaskRoute),
        "unittest_test_method" => Some(CodeUnitKind::UnittestTestMethod),
        "click_command" => Some(CodeUnitKind::ClickCommand),
        "typer_command" => Some(CodeUnitKind::TyperCommand),
        "celery_task" => Some(CodeUnitKind::CeleryTask),
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

    #[test]
    fn python_worker_candidates_cover_installed_and_portable_layouts() {
        let executable = Path::new("/opt/repogrammar/bin/repogrammar");
        let candidates = python_worker_script_candidates(executable);

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/opt/repogrammar/bin/repogrammar-workers/python/worker.py"),
                PathBuf::from("/opt/repogrammar/share/repogrammar/workers/python/worker.py"),
                PathBuf::from("/opt/repogrammar/workers/python/worker.py"),
                PathBuf::from("/opt/repogrammar/bin/workers/python/worker.py"),
            ]
        );
    }

    #[test]
    fn python_executable_env_override_wins_when_non_blank() {
        let executable = default_python_executable(|key| {
            (key == "REPOGRAMMAR_PYTHON_EXECUTABLE").then_some("E:/conda/python.exe".to_string())
        });

        assert_eq!(executable, "E:/conda/python.exe");
    }

    #[test]
    fn blank_python_executable_env_override_uses_platform_default() {
        let executable = default_python_executable(|key| {
            (key == "REPOGRAMMAR_PYTHON_EXECUTABLE").then_some("   ".to_string())
        });

        assert_eq!(executable, platform_python_executable());
    }

    #[cfg(windows)]
    #[test]
    fn python_executable_defaults_to_python_on_windows() {
        assert_eq!(default_python_executable(|_| None), "python");
    }

    #[cfg(not(windows))]
    #[test]
    fn python_executable_defaults_to_python3_off_windows() {
        assert_eq!(default_python_executable(|_| None), "python3");
    }

    #[cfg(unix)]
    #[test]
    fn python_worker_candidates_follow_command_symlink_to_managed_install() {
        let root = std::env::temp_dir().join(format!(
            "repogrammar-python-worker-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let installed = root.join("data/bin/repogrammar");
        let command = root.join("commands/repogrammar");
        fs::create_dir_all(installed.parent().expect("installed parent"))
            .expect("installed parent");
        fs::create_dir_all(command.parent().expect("command parent")).expect("command parent");
        fs::write(&installed, "stub").expect("installed executable");
        std::os::unix::fs::symlink(&installed, &command).expect("command symlink");

        let candidates = python_worker_script_candidates(&command);

        let expected_data_dir = fs::canonicalize(root.join("data")).expect("canonical data dir");
        assert!(candidates.contains(&expected_data_dir.join("workers/python/worker.py")));
        let _ = fs::remove_dir_all(root);
    }

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
from fastapi import Body, Cookie, Depends, Header, HTTPException, Path, Query
from app.services import UserService, run_query
from pydantic import BaseModel, ConfigDict, Field, computed_field, field_validator, model_validator, validator
from typing import Annotated
import pytest
import pytest as pt
from pytest import fixture as pytest_fixture
router = APIRouter()

class UserOut(BaseModel):
    model_config: ConfigDict = ConfigDict(from_attributes=True)
    id: int
    display_name: str = Field(default="", min_length=1)

    @field_validator("id")
    @classmethod
    def validate_id(cls, value):
        return value

    @validator("display_name")
    @classmethod
    def validate_display_name(cls, value):
        return value

    @computed_field
    @property
    def label(self) -> str:
        return self.display_name

    @model_validator(mode="after")
    def validate_model(self):
        return self

    class Config:
        arbitrary_types_allowed = True

def get_db():
    return object()

@router.get("/users/{user_id}", response_model=list[UserOut])
async def list_users(
    user_id: int = Path(...),
    payload: Annotated[UserOut, Body()] = None,
    query: str = Query(""),
    request_id: str = Header(""),
    session_id: str = Cookie(""),
    dependency=Depends(get_db),
):
    service = UserService()
    alias = service
    getattr(alias, "dynamic_users")()
    if False:
        raise HTTPException(status_code=404)
    return alias.list_users()

@router.get("/products")
def list_products():
    return run_query()

@router.get("/orders")
def list_orders():
    service = UserService()
    service = object()
    return service.list_orders()

@pytest_fixture
def client():
    return object()

@pt.fixture
def db():
    return object()

@pytest.mark.parametrize("status", [200])
def test_users(client, status, missing_fixture):
    assert client.get("/users").status_code == status
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
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == "pytest_fixture")
                .count(),
            2
        );
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
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.field.id")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_field")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.field_type.int")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_field_type")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.Field")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_field_metadata")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.model_config")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_model_config")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            == Some("pydantic.field.model_config")));
        assert!(!report.semantic_facts.iter().any(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            == Some("pydantic.field_type.pydantic.ConfigDict")));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.Config")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_config_class")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_route_decorator")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("app.services.UserService.list_users")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_service_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("app.services.run_query")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_service_call")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            == Some("app.services.UserService.list_orders")));
        assert!(!report.semantic_facts.iter().any(|fact| fact
            .target
            .as_ref()
            .map(SymbolId::as_str)
            == Some("service.list_orders")
            && fact
                .assumptions
                .iter()
                .any(|assumption| assumption == "python_anchor_kind=fastapi_service_call")));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_call_target")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.response_model.UserOut")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_response_model")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.request_body.UserOut")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_request_body_model")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.request_param.path.user_id")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_path_param")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.request_param.query.query")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_query_param")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.request_param.header.request_id")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_header_param")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.request_param.cookie.session_id")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_cookie_param")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.Depends")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.dependency.get_db")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.HTTPException")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_http_exception")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::Structural
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.http_exception.status_code.404")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=fastapi_http_exception_status"
                })
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.field_validator")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_validator")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.validator")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_validator")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.computed_field")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_computed_field")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.model_validator")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pydantic_model_validator")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.mark.parametrize")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_parametrize")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_decorator")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.status")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_parametrize_arg")
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
                FactCertainty::Structural | FactCertainty::DataflowDerived | FactCertainty::Unknown
            ));
            if fact.certainty == FactCertainty::DataflowDerived {
                assert!(fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false"));
                assert!(fact.assumptions.iter().any(|assumption| {
                    assumption == "derived_from=repo_local_python_import_graph"
                        || assumption == "derived_from=repo_local_pytest_fixture_graph"
                }));
            }
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
        assert!(!debug.contains("model_config ="));
        assert!(!debug.contains("min_length"));
        assert!(!debug.contains("arbitrary_types_allowed"));
        assert!(!debug.contains("dynamic_users"));
        assert!(!debug.contains("@router.get"));
        assert!(!debug.contains("response_model="));
        assert!(!debug.contains("list[UserOut]"));
        assert!(!debug.contains("Body()"));
        assert!(!debug.contains("Path("));
        assert!(!debug.contains("Query("));
        assert!(!debug.contains("Header("));
        assert!(!debug.contains("Cookie("));
        assert!(!debug.contains("Depends("));
        assert!(!debug.contains("Depends(get_db"));
        assert!(!debug.contains("HTTPException("));
        assert!(!debug.contains("assert client.get"));
    }

    #[test]
    fn cpython_frontend_preserves_fastapi_route_method_matrix() {
        let route_methods = ["delete", "get", "head", "options", "patch", "post", "put"];
        let mut source = String::from(
            "from fastapi import APIRouter, FastAPI\nrouter = APIRouter()\napp = FastAPI()\n\n",
        );
        for method in route_methods {
            source.push_str(&format!("@router.{method}('/router-{method}')\ndef router_{method}():\n    return {{}}\n\n@app.{method}('/app-{method}')\ndef app_{method}():\n    return {{}}\n\n"));
        }

        let report = PythonAstParser::default()
            .parse(document_at("routes.py", &source))
            .expect("parse FastAPI route matrix");

        let route_targets = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=fastapi_route_decorator"
                    })
            })
            .filter_map(|fact| fact.target.as_ref().map(SymbolId::as_str))
            .collect::<BTreeSet<_>>();
        let expected_targets = route_methods
            .iter()
            .flat_map(|method| {
                [
                    format!("fastapi.APIRouter.{method}"),
                    format!("fastapi.FastAPI.{method}"),
                ]
            })
            .collect::<BTreeSet<_>>();
        let actual_targets = route_targets
            .iter()
            .map(|target| target.to_string())
            .collect::<BTreeSet<_>>();

        assert_eq!(actual_targets, expected_targets);
        assert_eq!(
            report
                .units
                .iter()
                .filter(|unit| unit.kind.as_str() == "fastapi_route")
                .count(),
            route_methods.len() * 2
        );
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("@router."));
        assert!(!debug.contains("@app."));
    }

    #[test]
    fn cpython_frontend_marks_dynamic_decorators_and_monkey_patches_unknown() {
        let source = r#"
def decorator_factory(name):
    def inner(function):
        return function
    return inner

@decorator_factory("secret")
def decorated(target, method):
    setattr(target, method, object())
    return target
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_framework_identity")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("MonkeyPatch")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_call_target")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("decorator_factory(\"secret\")"));
        assert!(!debug.contains("setattr(target"));
    }

    #[test]
    fn cpython_frontend_marks_unresolved_bare_decorators_unknown() {
        let source = r#"
def local_decorator(function):
    return function

@local_decorator
def local_view():
    return {}

@unknown_policy
def protected_view():
    return {}

class Resource:
    @property
    def label(self):
        return "resource"
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse unresolved decorator");

        let framework_identity_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(framework_identity_unknowns, 1);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("unknown_policy")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=decorator_binding")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("return function"));
        assert!(!debug.contains("return \"resource\""));
    }

    #[test]
    fn cpython_frontend_marks_dynamic_pydantic_models_unknown() {
        let source = r#"
from pydantic import create_model
import pydantic as pyd

DynamicUser = create_model("DynamicUser", secret=(str, ...))
DynamicOrder = pyd.create_model("DynamicOrder", amount=(int, ...))
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse dynamic pydantic models");

        let dynamic_model_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(dynamic_model_unknowns, 2);
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.create_model")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("secret=(str"));
    }

    #[test]
    fn cpython_frontend_marks_dynamic_pydantic_config_unknown() {
        let source = r#"
from pydantic import BaseModel, ConfigDict

class DynamicConfigModel(BaseModel):
    model_config = ConfigDict(extra=policy())
    id: int

class StaticConfigModel(BaseModel):
    model_config = ConfigDict(from_attributes=True, extra="ignore")
    id: int
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse dynamic pydantic config");

        let dynamic_config_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(dynamic_config_unknowns, 1);
        let model_config_facts = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.model_config")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "python_anchor_kind=pydantic_model_config")
            })
            .count();
        assert_eq!(model_config_facts, 2);
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("policy()"));
    }

    #[test]
    fn cpython_frontend_marks_pydantic_validator_side_effects_unknown() {
        let source = r#"
from pydantic import BaseModel, field_validator, model_validator

class User(BaseModel):
    name: str

    @field_validator("name")
    @classmethod
    def normalize_name(cls, value):
        audit.write(value)
        return value

    @model_validator(mode="after")
    def audit_model(self):
        sink(self)
        return self
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse pydantic validator side effects");

        let side_effect_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "affected_claim=pydantic_validator_side_effects"
                    })
            })
            .count();
        assert_eq!(side_effect_unknowns, 2);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.BaseModel")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("audit.write(value)"));
        assert!(!debug.contains("sink(self)"));
    }

    #[test]
    fn cpython_frontend_emits_generic_python_code_units() {
        let source = r#"
def helper():
    return 1

async def fetch():
    return 2

class Plain:
    def method(self):
        return helper()

    async def async_method(self):
        return await fetch()
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
        assert!(kinds.contains(&"function"));
        assert!(kinds.contains(&"async_function"));
        assert!(kinds.contains(&"class"));
        assert_eq!(kinds.iter().filter(|kind| **kind == "method").count(), 2);
        assert!(!kinds.iter().any(|kind| matches!(
            *kind,
            "fastapi_route"
                | "pytest_test"
                | "pytest_fixture"
                | "pydantic_model"
                | "sqlalchemy_model"
                | "sqlalchemy_repository_method"
        )));
        assert!(report
            .units
            .iter()
            .all(|unit| unit.language == Language::Python
                && unit.provenance.path == "app.py"
                && unit.range.start_byte <= unit.range.end_byte));
    }

    #[test]
    fn cpython_frontend_blocks_dynamic_imports_without_unique_repo_local_resolution() {
        let source = r#"
import importlib
import sys

def load(name, extra_path):
    sys.path.insert(0, extra_path)
    safe = importlib.import_module("plugins.safe")
    importlib.import_module("../secret")
    importlib.import_module(name)
    handler = getattr(safe, "handle")
    locals()[name]()
    eval("/tmp/secret")
    exec("/tmp/secret")
    compile("/tmp/secret", "/tmp/secret", "exec")
    __import__(name)
    return handler
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("RuntimeDependencyInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedImport
                && fact.target.as_ref().map(SymbolId::as_str) == Some("plugins.safe")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=dynamic_import_literal")
        }));
        let dynamic_import_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("DynamicImport")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_import_resolution")
            })
            .count();
        assert!(dynamic_import_unknowns >= 4);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_call_target")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("../secret"));
        assert!(!debug.contains("locals()[name]"));
        assert!(!debug.contains("eval(\"/tmp/secret\")"));
        assert!(!debug.contains("__import__(name)"));
    }

    #[test]
    fn cpython_frontend_resolves_literal_dynamic_imports_only_when_repo_local_unique() {
        let source = r#"
import importlib

def load():
    return importlib.import_module("plugins.safe")
"#;
        let context = ParserProjectContext {
            python_module_paths: vec![
                "app.py".to_string(),
                "plugins/__init__.py".to_string(),
                "plugins/safe.py".to_string(),
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: Vec::new(),
            ..ParserProjectContext::default()
        };
        let report = PythonAstParser::default()
            .parse_with_context(document(source), &context)
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedImport
                && fact.target.as_ref().map(SymbolId::as_str) == Some("plugins.safe")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=dynamic_import_literal")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("DynamicImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
    }

    #[test]
    fn cpython_frontend_preserves_indirect_parametrize_fixture_unknowns() {
        let source = r#"
import pytest

@pytest.mark.parametrize("client,status", [("api", 200)], indirect=["client"])
def test_indirect_list(client, status):
    assert status == 200

@pytest.mark.parametrize("resource", ["db"], indirect=True)
def test_indirect_all(resource):
    assert resource
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.status")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_parametrize_arg")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.client")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.resource")
        }));
        let fixture_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
            })
            .count();
        assert!(fixture_unknowns >= 2);
    }

    #[test]
    fn cpython_frontend_prefers_direct_parametrize_over_same_named_fixture() {
        let source = r#"
import pytest

@pytest.fixture
def client():
    return object()

@pytest.mark.parametrize("client", ["api"])
def test_direct_client(client):
    assert client

@pytest.mark.parametrize("db", ["db"], indirect=True)
def test_indirect_db(db):
    assert db
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_parametrize_arg")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.parametrize.db")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=pytest_fixture_binding")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("return object"));
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
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_route_decorator")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("@v1.get"));
    }

    #[test]
    fn cpython_frontend_marks_dynamic_fastapi_dependency_targets_unknown() {
        let source = r#"
from fastapi import APIRouter, Depends

router = APIRouter()

def make_dependency():
    return object()

@router.get("/dynamic")
def dynamic_dependency(current_user=Depends(make_dependency())):
    return {}

@router.get("/lambda")
def lambda_dependency(current_user=Depends(lambda: object())):
    return {}

@router.get("/conditional")
def conditional_dependency(current_user=Depends(make_dependency if True else None)):
    return {}

@router.get("/missing")
def missing_dependency(current_user=Depends(missing_dep)):
    return {}

@router.get("/attribute")
def attribute_dependency(current_user=Depends(plugins.current_user)):
    return {}

@router.get("/empty")
def empty_dependency(current_user=Depends()):
    return {}
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert_eq!(
            report
                .semantic_facts
                .iter()
                .filter(|fact| {
                    fact.kind == SemanticFactKind::ResolvedCall
                        && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.Depends")
                        && fact
                            .assumptions
                            .iter()
                            .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency")
                })
                .count(),
            6
        );
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        assert_eq!(
            report
                .semantic_facts
                .iter()
                .filter(|fact| {
                    fact.kind == SemanticFactKind::Unknown
                        && fact.target.as_ref().map(SymbolId::as_str)
                            == Some("RuntimeDependencyInjection")
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "affected_claim=fastapi_dependency_target"
                        })
                })
                .count(),
            5
        );
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("Depends(make_dependency"));
        assert!(!debug.contains("lambda: object"));
        assert!(!debug.contains("plugins.current_user"));
    }

    #[test]
    fn cpython_frontend_preserves_static_fastapi_dependency_targets() {
        let source = r#"
from fastapi import APIRouter, Depends
from app.dependencies import get_current_user

router = APIRouter()

def get_db():
    return object()

@router.get("/static")
def static_dependency(db=Depends(get_db), current_user=Depends(get_current_user)):
    return {}
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.Depends")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("RuntimeDependencyInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=fastapi_dependency_target")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.dependency.get_db")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("fastapi.dependency.app.dependencies.get_current_user")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=fastapi_dependency_target")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("Depends(get_db"));
        assert!(!debug.contains("Depends(get_current_user"));
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

        assert!(!report
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
    fn cpython_frontend_drops_shadowed_framework_import_exact_anchors() {
        let source = r#"
from fastapi import APIRouter
from pydantic import BaseModel
from pytest import fixture
from sqlalchemy.orm import Mapped, mapped_column

APIRouter = object
BaseModel = object
fixture = object
Mapped = list
mapped_column = object

router = APIRouter()

@router.get("/users")
def list_users():
    return []

class UserOut(BaseModel):
    id: int

@fixture
def client():
    return object()

class User:
    __tablename__ = "users"
    id: Mapped[int] = mapped_column()
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pydantic.BaseModel")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_decorator")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.Mapped")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_mapped_field")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.mapped_column")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_mapped_column")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_framework_identity")
        }));
    }

    #[test]
    fn cpython_frontend_copies_module_dynamic_unknowns_to_family_units() {
        let source = r#"
import importlib
import sys
from fastapi import APIRouter

sys.path.insert(0, "/tmp/secret")
importlib.import_module("plugins.dynamic")

router = APIRouter()

@router.get("/users")
def list_users():
    return []
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");
        let route_unit = report
            .units
            .iter()
            .find(|unit| unit.kind.as_str() == "fastapi_route")
            .expect("route unit");
        let route_unit_id = route_unit.id.as_str();

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.evidence.code_unit_id.as_str() == route_unit_id
                && fact.target.as_ref().map(SymbolId::as_str) == Some("fastapi.APIRouter.get")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.evidence.code_unit_id.as_str() == route_unit_id
                && fact.target.as_ref().map(SymbolId::as_str) == Some("RuntimeDependencyInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.evidence.code_unit_id.as_str() == route_unit_id
                && fact.target.as_ref().map(SymbolId::as_str) == Some("DynamicImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("/tmp/secret"));
        assert!(!debug.contains("plugins.dynamic"));
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
    fn cpython_frontend_resolves_sqlalchemy_declarative_base_assignments() {
        let source = r#"
from sqlalchemy.orm import declarative_base

Base = declarative_base()

class User(Base):
    __tablename__ = "users"
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "sqlalchemy_model"));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.declarative_base")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=class_base")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("Base = declarative_base()"));
    }

    #[test]
    fn cpython_frontend_extracts_sqlalchemy_model_field_anchors() {
        let source = r#"
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, text
from sqlalchemy.orm import Mapped, Session, mapped_column, relationship

class User:
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
    accounts = relationship("Account")

class Account:
    __tablename__ = "accounts"
    id: Mapped[int] = mapped_column(primary_key=True)

class UserRepository:
    def list_users(self, session: Session):
        session.add(User())
        return session.execute("select users")

    def select_users(self, session: Session):
        return session.execute(select(User))

    def raw_text_users(self, session: Session):
        return session.execute(text("select users"))

    def get_user(self, session: Session):
        return session.scalar("select user")

    def stream_users(self, session: Session):
        return session.scalars("select users")

    def load_user(self, session: Session):
        return session.get(User, 1)

    async def list_accounts(self, db: AsyncSession):
        return await db.execute("select accounts")

    async def get_account(self, db: AsyncSession):
        return await db.scalar("select account")

    async def stream_accounts(self, db: AsyncSession):
        return await db.scalars("select accounts")

    async def load_account(self, db: AsyncSession):
        return await db.get(User, 1)

class StoredSessionRepository:
    def __init__(self, session: Session, db: AsyncSession):
        self.session = session
        self.db: AsyncSession = db

    def commit_users(self):
        self.session.commit()
        return self.session.execute("select users")

    def rollback_users(self):
        self.session.rollback()

    async def commit_accounts(self):
        await self.db.commit()

    async def rollback_accounts(self):
        await self.db.rollback()
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
                && fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.relationship")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_relationship")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.relationship_target.Account")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=sqlalchemy_relationship_target"
                })
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "fact_scope=context_only")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.Session.add")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=sqlalchemy_query_shape")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.scalar")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.scalars")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str) == Some("sqlalchemy.orm.Session.get")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.execute")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.scalar")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.scalars")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.get")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.commit")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.rollback")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.commit")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.ext.asyncio.AsyncSession.rollback")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=sqlalchemy_session_call")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("mapped_column(primary_key=True)"));
        assert!(!debug.contains("select users"));
    }

    #[test]
    fn cpython_frontend_keeps_structured_sqlalchemy_select_non_raw() {
        let source = r#"
from sqlalchemy import select
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

class UserRepository:
    def list_users(self, session: Session):
        return session.execute(select(User))
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=sqlalchemy_query_shape")
        }));
    }

    #[test]
    fn cpython_frontend_drops_shadowed_sqlalchemy_instance_roles() {
        let source = r#"
from sqlalchemy.orm import Session

class UserRepository:
    def __init__(self, session: Session):
        self.session = session

    def list_users(self):
        self.session = object()
        return self.session.execute("select users")
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("return self.session.execute"));
    }

    #[test]
    fn cpython_frontend_marks_runtime_sqlalchemy_session_injection_unknown() {
        let source = r#"
class UserRepository:
    def __init__(self, session):
        self.session = session

    def list_users(self):
        return self.session.execute("select users")
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("RuntimeDependencyInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_framework_identity")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("select users"));
    }

    #[test]
    fn cpython_frontend_marks_sqlalchemy_event_listeners_unknown() {
        let source = r#"
from sqlalchemy import event

class User:
    pass

def audit(mapper, connection, target):
    pass

event.listen(User, "before_insert", audit)

@event.listens_for(User, "after_update")
def receive_update(mapper, connection, target):
    pass
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        let event_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert!(event_unknowns >= 2);
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && matches!(
                    fact.target.as_ref().map(SymbolId::as_str),
                    Some("sqlalchemy.event.listen" | "sqlalchemy.event.listens_for")
                )
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("before_insert"));
    }

    #[test]
    fn cpython_frontend_marks_dynamic_sqlalchemy_model_classes_unknown() {
        let source = r#"
from sqlalchemy.orm import declarative_base

Base = declarative_base()
DynamicUser = type("DynamicUser", (Base,), {"__tablename__": "users"})
Plain = type("Plain", (object,), {})
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        let dynamic_model_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(dynamic_model_unknowns, 1);
        assert!(!report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "sqlalchemy_model"));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("__tablename__"));
    }

    #[test]
    fn cpython_frontend_marks_sqlalchemy_query_wrappers_unknown() {
        let source = r#"
from sqlalchemy import select
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

def execute_users(session: Session):
    return session.execute(select(User))

class UserRepository:
    def _execute_users(self, session: Session):
        return session.execute(select(User))

    def list_users(self, session: Session):
        return execute_users(session)

    def list_wrapped_users(self, session: Session):
        return self._execute_users(session)
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        let wrapper_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(wrapper_unknowns, 2);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("sqlalchemy.orm.Session.execute")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && matches!(
                    fact.target.as_ref().map(SymbolId::as_str),
                    Some("execute_users" | "self._execute_users")
                )
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("return execute_users(session)"));
        assert!(!debug.contains("return self._execute_users(session)"));
    }

    #[test]
    fn cpython_frontend_does_not_classify_plain_get_calls_as_sqlalchemy() {
        let source = r#"
class CacheRepository:
    def read_cache(self, cache):
        return cache.get("users")
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        assert!(!report
            .units
            .iter()
            .any(|unit| unit.kind.as_str() == "sqlalchemy_repository_method"));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::ResolvedCall
                && fact
                    .target
                    .as_ref()
                    .map(SymbolId::as_str)
                    .is_some_and(|target| target.starts_with("sqlalchemy."))
        }));
    }

    #[test]
    fn cpython_frontend_marks_external_framework_bases_unknown() {
        let source = r#"
from app.db import Base
from app.schemas import BaseSchema
from pydantic import Field
from sqlalchemy.orm import Mapped, mapped_column

class UserOut(BaseSchema):
    id: int = Field(default=0)

class User(Base):
    __tablename__ = "users"
    id: Mapped[int] = mapped_column(primary_key=True)
"#;
        let report = PythonAstParser::default()
            .parse(document(source))
            .expect("parse python");

        let framework_identity_unknowns = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("FrameworkMagic")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=python_framework_identity")
            })
            .count();
        assert_eq!(framework_identity_unknowns, 2);
        assert!(!report.units.iter().any(|unit| {
            unit.id.as_str().contains("userout") && unit.kind.as_str() == "pydantic_model"
        }));
        assert!(report.units.iter().any(|unit| {
            unit.id.as_str().contains("user") && unit.kind.as_str() == "sqlalchemy_model"
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("Field(default=0)"));
        assert!(!debug.contains("__tablename__ = "));
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

        assert!(!report
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
        assert!(report.semantic_facts.iter().any(|fact| fact
            .assumptions
            .iter()
            .any(|assumption| assumption == "python_config_source_root=src/lib")));
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
    fn parse_with_context_accepts_repo_local_import_graph_facts_without_source_leakage() {
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
            ..ParserProjectContext::default()
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
            fact.certainty == FactCertainty::DataflowDerived
                && fact.origin.engine == "python"
                && fact.origin.method == "cpython_ast"
                && fact.evidence.provenance.path == "src/acme/api.py"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "provider_resolved=false")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_python_import_graph")
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
                FactCertainty::Structural | FactCertainty::DataflowDerived | FactCertainty::Unknown
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
    fn parse_with_context_resolves_repo_local_symbols_reexports_and_safe_star_imports() {
        let source = "\
from acme.models import User, make_user\n\
from acme import PublicUser\n\
from acme.models import *\n";
        let context = ParserProjectContext {
            python_module_paths: vec![
                "src/acme/api.py".to_string(),
                "src/acme/__init__.py".to_string(),
                "src/acme/models.py".to_string(),
            ],
            python_module_files: vec![
                ParserProjectFileContext {
                    path: "src/acme/__init__.py".to_string(),
                    text: "from .models import User as PublicUser\n".to_string(),
                },
                ParserProjectFileContext {
                    path: "src/acme/models.py".to_string(),
                    text: "__all__ = ['User']\nclass User: pass\ndef make_user(): pass\n"
                        .to_string(),
                },
                ParserProjectFileContext {
                    path: "src/acme/api.py".to_string(),
                    text: String::new(),
                },
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: Vec::new(),
            ..ParserProjectContext::default()
        };
        let report = PythonAstParser::default()
            .parse_with_context(document_at("src/acme/api.py", source), &context)
            .expect("parse with repo-local symbol context");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Type
                && fact.certainty == FactCertainty::DataflowDerived
                && fact.target.as_ref().map(SymbolId::as_str) == Some("acme.models.User")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=repo_local_import_symbol")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_python_import_graph")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::DataflowDerived
                && fact.target.as_ref().map(SymbolId::as_str) == Some("acme.models.make_user")
        }));
        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
        let debug = format!("{:?}", report.semantic_facts);
        assert!(!debug.contains("class User"));
        assert!(!debug.contains("def make_user"));
    }

    #[test]
    fn parse_with_context_keeps_unsafe_star_import_unknown() {
        let source = "from acme.models import *\n";
        let context = ParserProjectContext {
            python_module_paths: vec![
                "src/acme/api.py".to_string(),
                "src/acme/models.py".to_string(),
            ],
            python_module_files: vec![
                ParserProjectFileContext {
                    path: "src/acme/models.py".to_string(),
                    text: "class User: pass\n".to_string(),
                },
                ParserProjectFileContext {
                    path: "src/acme/api.py".to_string(),
                    text: String::new(),
                },
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: Vec::new(),
            ..ParserProjectContext::default()
        };
        let report = PythonAstParser::default()
            .parse_with_context(document_at("src/acme/api.py", source), &context)
            .expect("parse unsafe star import");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("UnresolvedImport")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=python_import_resolution")
        }));
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
                text: "import pytest as pt\n\n@pt.fixture\ndef client():\n    return object()\n"
                    .to_string(),
            }],
            ..ParserProjectContext::default()
        };
        let parser = PythonAstParser::default();
        let report = parser
            .parse_with_context(document_at("tests/sub/test_api.py", source), &context)
            .expect("parse with conftest context");

        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.certainty == FactCertainty::DataflowDerived
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.client")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                    })
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "derived_from=repo_local_pytest_fixture_graph"
                    })
            }),
            "units={:?} diagnostics={:?} facts={:?}",
            report.units,
            report.diagnostics,
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
    fn parse_document_tracks_fixture_to_fixture_edges_without_source_leakage() {
        let source = r#"
import pytest

fixture_alias = pytest.fixture

@pytest.fixture
def db():
    return object()

@fixture_alias(name="api_client")
def client(db, tmp_path, missing_fixture):
    return object()

def helper(db):
    return db

def test_users(api_client):
    assert api_client

def test_literal_lookup(request):
    assert request.getfixturevalue("api_client")

def test_dynamic_lookup(request, fixture_name):
    assert request.getfixturevalue(fixture_name)
"#;
        let report = PythonAstParser::default()
            .parse(document_at("tests/test_fixture_graph.py", source))
            .expect("parse fixture graph");

        assert_eq!(
            report
                .semantic_facts
                .iter()
                .filter(|fact| {
                    fact.kind == SemanticFactKind::Symbol
                        && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.db")
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "python_anchor_kind=pytest_fixture_edge"
                        })
                })
                .count(),
            1,
            "non-fixture helpers must not produce fixture edges"
        );
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str)
                    == Some("pytest.builtin_fixture.tmp_path")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_builtin_fixture_context"
                })
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.api_client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.certainty == FactCertainty::DataflowDerived
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.api_client")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "derived_from=repo_local_pytest_fixture_graph")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=pytest_fixture_binding")
        }));
        let debug = format!("{:?}", report);
        assert!(!debug.contains("return object"));
        assert!(!debug.contains("missing_fixture"));
    }

    #[test]
    fn parse_with_context_marks_duplicate_conftest_and_builtin_fixture_boundaries() {
        let source = r#"
def test_users(client, tmp_path, capsys, django_db):
    assert tmp_path
"#;
        let context = ParserProjectContext {
            python_module_paths: vec![
                "conftest.py".to_string(),
                "tests/conftest.py".to_string(),
                "tests/sub/test_api.py".to_string(),
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: vec![ParserProjectFileContext {
                path: "tests/conftest.py".to_string(),
                text: r#"
import pytest

@pytest.fixture
def client():
    return object()

@pytest.fixture
def client():
    return object()
"#
                .to_string(),
            }],
            ..ParserProjectContext::default()
        };
        let report = PythonAstParser::default()
            .parse_with_context(document_at("tests/sub/test_api.py", source), &context)
            .expect("parse with fixture boundary context");

        assert!(!report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                })
        }));
        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "affected_claim=pytest_fixture_binding")
            }),
            "units={:?} diagnostics={:?} facts={:?}",
            report.units,
            report.diagnostics,
            report.semantic_facts
        );
        for builtin_target in [
            "pytest.builtin_fixture.tmp_path",
            "pytest.builtin_fixture.capsys",
        ] {
            assert!(
                report.semantic_facts.iter().any(|fact| {
                    fact.kind == SemanticFactKind::Symbol
                        && fact.target.as_ref().map(SymbolId::as_str) == Some(builtin_target)
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "python_anchor_kind=pytest_builtin_fixture_context"
                        })
                }),
                "missing builtin fixture context {builtin_target}"
            );
        }
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
        assert!(!debug.contains("django_db"));
    }

    #[test]
    fn parse_document_honors_literal_pytest_fixture_name_aliases() {
        let source = r#"
import pytest

fixture_name = "dynamic_client"
fixture_alias = pytest.fixture

@pytest.fixture(name="api_client")
def _api_client():
    return object()

@pytest.fixture(name=fixture_name)
def dynamic_client():
    return object()

@fixture_alias(name="settings")
def _settings():
    return object()

@pytest.fixture(name="bad/client")
def unsafe_client():
    return object()

def test_fixture_aliases(api_client, settings, _api_client, dynamic_client, unsafe_client):
    assert api_client
"#;
        let report = PythonAstParser::default()
            .parse(document_at("tests/test_fixture_alias_name.py", source))
            .expect("parse literal fixture name aliases");

        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.target.as_ref().map(SymbolId::as_str)
                        == Some("pytest.fixture.api_client")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
            }),
            "facts={:?}",
            report.semantic_facts
        );
        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture.settings")
                    && fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption == "python_anchor_kind=pytest_fixture_edge")
            }),
            "facts={:?}",
            report.semantic_facts
        );
        for target in [
            "pytest.fixture._api_client",
            "pytest.fixture._settings",
            "pytest.fixture.dynamic_client",
            "pytest.fixture.unsafe_client",
        ] {
            assert!(
                report.semantic_facts.iter().all(|fact| {
                    !(fact.kind == SemanticFactKind::Symbol
                        && fact.target.as_ref().map(SymbolId::as_str) == Some(target)
                        && fact.assumptions.iter().any(|assumption| {
                            assumption == "python_anchor_kind=pytest_fixture_edge"
                        }))
                }),
                "fixture implementation name should not become a fixture binding target: {target}"
            );
        }
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("PytestFixtureInjection")
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "affected_claim=pytest_fixture_binding")
        }));
        let debug = format!("{:?}", report);
        assert!(!debug.contains("name=fixture_name"));
        assert!(!debug.contains("bad/client"));
        assert!(!debug.contains("return object"));
    }

    #[test]
    fn parse_with_context_honors_conftest_literal_pytest_fixture_name_aliases() {
        let source = r#"
def test_fixture_aliases(api_client, _api_client):
    assert api_client
"#;
        let context = ParserProjectContext {
            python_module_paths: vec![
                "tests/conftest.py".to_string(),
                "tests/sub/test_fixture_alias_name.py".to_string(),
            ],
            python_source_roots: Vec::new(),
            python_conftest_files: vec![ParserProjectFileContext {
                path: "tests/conftest.py".to_string(),
                text: r#"
import pytest

@pytest.fixture(name="api_client")
def _api_client():
    return object()
"#
                .to_string(),
            }],
            ..ParserProjectContext::default()
        };
        let report = PythonAstParser::default()
            .parse_with_context(
                document_at("tests/sub/test_fixture_alias_name.py", source),
                &context,
            )
            .expect("parse conftest literal fixture name alias");

        assert!(
            report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Symbol
                    && fact.target.as_ref().map(SymbolId::as_str)
                        == Some("pytest.fixture.api_client")
                    && fact.assumptions.iter().any(|assumption| {
                        assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                    })
            }),
            "facts={:?}",
            report.semantic_facts
        );
        assert!(report.semantic_facts.iter().all(|fact| {
            !(fact.kind == SemanticFactKind::Symbol
                && fact.target.as_ref().map(SymbolId::as_str) == Some("pytest.fixture._api_client")
                && fact.assumptions.iter().any(|assumption| {
                    assumption == "python_anchor_kind=pytest_conftest_fixture_edge"
                }))
        }));
        let debug = format!("{:?}", report);
        assert!(!debug.contains("tests/conftest.py"));
        assert!(!debug.contains("return object"));
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
    fn cpython_frontend_parses_bundled_worker_with_bounded_large_output() {
        let worker_path = source_checkout_python_worker_script();
        let source = fs::read_to_string(&worker_path).expect("checked-in Python worker");
        assert!(
            source.len() > 200_000,
            "fixture must exercise a large module"
        );

        let report = PythonAstParser::default()
            .parse(document_at("src/workers/python/worker.py", &source))
            .expect("bundled worker should analyze its own source");

        assert!(report
            .units
            .iter()
            .any(|unit| unit.kind == CodeUnitKind::Module));
        assert!(report.units.len() > 100);
        assert!(report.semantic_facts.len() > 1_000);
        assert!(report.diagnostics.is_empty());
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
