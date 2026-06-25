//! CPython-ast-backed Python code-unit extraction.
//!
//! This adapter uses the repository's Python worker process so Rust does not
//! hand-roll Python parsing rules. The worker returns owned metadata only.

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{CodeUnit, CodeUnitId, CodeUnitKind, Language, Provenance, SourceRange};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, SourceDocument, SourceParser,
};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const MAX_PYTHON_FRONTEND_OUTPUT_BYTES: usize = 1024 * 1024;

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
        if document.language != Language::Python {
            return Err(ParseError::UnsupportedLanguage);
        }
        let response = self.parse_document(&document)?;
        parse_worker_response(&document, &response)
    }
}

impl PythonAstParser {
    fn parse_document(&self, document: &SourceDocument<'_>) -> Result<String, ParseError> {
        let payload = json!({
            "protocol_version": 1,
            "mode": "parse_document",
            "path": document.path,
            "content_hash": document.content_hash.as_str(),
            "repository_revision": document.repository_revision.as_str(),
            "text": document.text,
        });
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
            .write_all(payload.to_string().as_bytes())
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

fn parse_worker_response(
    document: &SourceDocument<'_>,
    response: &str,
) -> Result<ParseReport, ParseError> {
    let line = response
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| ParseError::Internal("python ast frontend returned no response".into()))?;
    let value: Value = serde_json::from_str(line)
        .map_err(|_| ParseError::Internal("python ast frontend returned invalid JSON".into()))?;
    let object = value.as_object().ok_or_else(|| {
        ParseError::Internal("python ast frontend response was not an object".into())
    })?;
    if object.get("protocol_version").and_then(Value::as_u64) != Some(1)
        || object.get("mode").and_then(Value::as_str) != Some("parse_document")
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
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(ParseReport {
        units,
        ir_nodes,
        ir_edges,
        diagnostics,
    })
}

fn parse_unit(document: &SourceDocument<'_>, value: &Value) -> Result<CodeUnit, ParseError> {
    let object = value
        .as_object()
        .ok_or_else(|| ParseError::Internal("python ast frontend unit was invalid".into()))?;
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

    fn document(text: &str) -> SourceDocument<'_> {
        SourceDocument {
            path: "app.py",
            language: Language::Python,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    #[test]
    fn cpython_frontend_extracts_python_units_without_snippets() {
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
}
