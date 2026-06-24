//! Language-native semantic worker port.
//!
//! Workers may use native compiler, type-checker, or language-server APIs, but
//! must return RepoGrammar-owned semantic facts.

use crate::core::model::SemanticFact;

pub const SEMANTIC_WORKER_PROTOCOL_VERSION: u16 = 1;
pub const SEMANTIC_VERSION_UNSUPPORTED_CODE: &str = "SEMANTIC_VERSION_UNSUPPORTED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticWorkerMessageKind {
    Fact,
    Progress,
    WorkerError,
    EndOfStream,
}

impl SemanticWorkerMessageKind {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Progress => "progress",
            Self::WorkerError => "worker_error",
            Self::EndOfStream => "end_of_stream",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWorkerRequest {
    pub project_root: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticWorkerError {
    Unavailable(String),
    UnsupportedVersion(String),
    ProtocolViolation(String),
}

pub trait SemanticWorker {
    fn analyze_project(
        &self,
        request: SemanticWorkerRequest,
    ) -> Result<Vec<SemanticFact>, SemanticWorkerError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, FactCertainty, SemanticFactKind};
    use serde_json::{json, Map, Value};
    use std::{fs, path::Path};

    #[test]
    fn protocol_version_is_pinned_to_v1() {
        assert_eq!(SEMANTIC_WORKER_PROTOCOL_VERSION, 1);
    }

    #[test]
    fn message_kinds_use_ndjson_protocol_tokens() {
        assert_eq!(SemanticWorkerMessageKind::Fact.as_protocol_str(), "fact");
        assert_eq!(
            SemanticWorkerMessageKind::Progress.as_protocol_str(),
            "progress"
        );
        assert_eq!(
            SemanticWorkerMessageKind::WorkerError.as_protocol_str(),
            "worker_error"
        );
        assert_eq!(
            SemanticWorkerMessageKind::EndOfStream.as_protocol_str(),
            "end_of_stream"
        );
    }

    #[test]
    fn schema_documents_unsupported_version_code() {
        let schema = include_str!("../../protocol/semantic-worker-message.schema.json");

        assert!(schema.contains(SEMANTIC_VERSION_UNSUPPORTED_CODE));
        assert!(schema.contains("\"protocol_version\""));
        assert!(schema.contains("\"message_type\""));
        assert!(schema.contains("\"code_unit_id\""));
        assert!(schema.contains("\"note\""));
        assert!(schema.contains("sha256:[A-Fa-f0-9]{64}"));
    }

    #[test]
    fn ndjson_fixtures_are_valid_protocol_messages() {
        let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocol/fixtures");
        let mut checked_fixtures = 0;

        for entry in fs::read_dir(&fixture_dir).expect("protocol fixture directory must exist") {
            let path = entry
                .expect("protocol fixture entry must be readable")
                .path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("ndjson") {
                continue;
            }

            checked_fixtures += 1;
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!("{} must be readable: {error}", path.display());
            });

            validate_protocol_fixture(&content).unwrap_or_else(|error| {
                panic!("{} failed protocol validation: {error}", path.display());
            });
        }

        assert!(
            checked_fixtures > 0,
            "expected at least one protocol fixture under {}",
            fixture_dir.display()
        );
    }

    #[test]
    fn protocol_fixture_validation_rejects_invalid_content_hash() {
        let mut fact = valid_fact_message();
        fact["evidence"]["content_hash"] = json!("sha256:fixture");
        let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

        let error = validate_protocol_fixture(&fixture).expect_err("content hash must be rejected");

        assert!(error.contains("content_hash"));
    }

    #[test]
    fn protocol_fixture_validation_rejects_empty_target() {
        let mut fact = valid_fact_message();
        fact["target"] = json!("");
        let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

        let error = validate_protocol_fixture(&fixture).expect_err("empty target must be rejected");

        assert!(error.contains("target must not be empty"));
    }

    fn validate_protocol_fixture(content: &str) -> Result<(), String> {
        let mut saw_end_of_stream = false;

        for (line_index, line) in content.lines().enumerate() {
            let line_number = line_index + 1;
            let message: Value = serde_json::from_str(line)
                .map_err(|error| format!("line {line_number} is not valid JSON: {error}"))?;
            let object = message
                .as_object()
                .ok_or_else(|| format!("line {line_number} must be a JSON object"))?;

            validate_required_fields(object, &["protocol_version", "message_type", "request_id"])
                .map_err(|error| format!("line {line_number}: {error}"))?;

            let protocol_version = object
                .get("protocol_version")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("line {line_number}: protocol_version must be an integer")
                })?;
            if protocol_version != u64::from(SEMANTIC_WORKER_PROTOCOL_VERSION) {
                return Err(format!(
                    "line {line_number}: protocol_version must be {}",
                    SEMANTIC_WORKER_PROTOCOL_VERSION
                ));
            }
            required_string(object, "request_id")
                .map_err(|error| format!("line {line_number}: {error}"))?;

            let message_type = required_string(object, "message_type")
                .map_err(|error| format!("line {line_number}: {error}"))?;
            if !allowed_message_types().contains(&message_type) {
                return Err(format!(
                    "line {line_number}: unsupported message_type {message_type}"
                ));
            }

            match message_type {
                "fact" => validate_fact_message(object)
                    .map_err(|error| format!("line {line_number}: {error}"))?,
                "progress" => validate_progress_message(object)
                    .map_err(|error| format!("line {line_number}: {error}"))?,
                "worker_error" => validate_worker_error_message(object)
                    .map_err(|error| format!("line {line_number}: {error}"))?,
                "end_of_stream" => saw_end_of_stream = true,
                _ => unreachable!("message_type was already checked"),
            }
        }

        if saw_end_of_stream {
            Ok(())
        } else {
            Err("fixture must include an end_of_stream message".to_string())
        }
    }

    fn validate_fact_message(object: &Map<String, Value>) -> Result<(), String> {
        validate_required_fields(
            object,
            &[
                "fact_kind",
                "subject",
                "origin",
                "certainty",
                "evidence",
                "assumptions",
            ],
        )?;

        SemanticFactKind::parse_protocol_str(required_string(object, "fact_kind")?)?;
        required_string(object, "subject")?;
        FactCertainty::parse_protocol_str(required_string(object, "certainty")?)?;
        validate_origin(object)?;
        validate_evidence(object)?;

        if let Some(target) = object.get("target") {
            match target {
                Value::Null => {}
                Value::String(value) if !value.trim().is_empty() => {}
                Value::String(_) => return Err("target must not be empty".to_string()),
                _ => return Err("target must be a string or null when present".to_string()),
            }
        }

        object
            .get("assumptions")
            .and_then(Value::as_array)
            .ok_or_else(|| "assumptions must be an array".to_string())?;

        Ok(())
    }

    fn validate_progress_message(object: &Map<String, Value>) -> Result<(), String> {
        validate_required_fields(object, &["stage", "message", "work"])?;
        match required_string(object, "stage")? {
            "project_discovery"
            | "file_scanning"
            | "syntax_parsing"
            | "semantic_resolution"
            | "code_unit_extraction_normalization"
            | "candidate_discovery"
            | "family_construction"
            | "persistence_validation" => {}
            other => return Err(format!("unsupported progress stage {other}")),
        }
        required_string(object, "message")?;
        let work = object
            .get("work")
            .and_then(Value::as_object)
            .ok_or_else(|| "work must be an object".to_string())?;
        match required_string(work, "kind")? {
            "unknown" => {}
            "known" => {
                let completed = required_u64(work, "completed")?;
                let total = required_u64(work, "total")?;
                if completed > total {
                    return Err("completed work must not exceed total work".to_string());
                }
            }
            other => return Err(format!("unsupported work kind {other}")),
        }
        Ok(())
    }

    fn validate_worker_error_message(object: &Map<String, Value>) -> Result<(), String> {
        validate_required_fields(object, &["error_code", "message"])?;
        match required_string(object, "error_code")? {
            "SEMANTIC_WORKER_UNAVAILABLE"
            | "SEMANTIC_VERSION_UNSUPPORTED"
            | "SEMANTIC_PROTOCOL_VIOLATION" => {}
            other => return Err(format!("unsupported error_code {other}")),
        }
        required_string(object, "message")?;
        if let Some(fallback) = object.get("fallback") {
            let fallback = fallback
                .as_object()
                .ok_or_else(|| "fallback must be an object".to_string())?;
            match required_string(fallback, "mode")? {
                "syntax_only" => {}
                other => return Err(format!("unsupported fallback mode {other}")),
            }
            match required_string(fallback, "certainty")? {
                "STRUCTURAL" | "UNKNOWN" => {}
                other => return Err(format!("unsupported fallback certainty {other}")),
            }
        }
        Ok(())
    }

    fn validate_origin(object: &Map<String, Value>) -> Result<(), String> {
        let origin = object
            .get("origin")
            .and_then(Value::as_object)
            .ok_or_else(|| "origin must be an object".to_string())?;
        validate_required_fields(origin, &["engine", "engine_version", "method"])?;
        required_string(origin, "engine")?;
        required_string(origin, "engine_version")?;
        required_string(origin, "method")?;
        Ok(())
    }

    fn validate_evidence(object: &Map<String, Value>) -> Result<(), String> {
        let evidence = object
            .get("evidence")
            .and_then(Value::as_object)
            .ok_or_else(|| "evidence must be an object".to_string())?;
        validate_required_fields(
            evidence,
            &[
                "code_unit_id",
                "path",
                "content_hash",
                "repository_revision",
                "start_byte",
                "end_byte",
                "note",
            ],
        )?;

        required_string(evidence, "code_unit_id")?;
        required_string(evidence, "path")?;
        ContentHash::new(required_string(evidence, "content_hash")?)
            .map_err(|error| format!("invalid content_hash: {error}"))?;
        required_string(evidence, "repository_revision")?;
        required_string(evidence, "note")?;

        let start_byte = required_u64(evidence, "start_byte")?;
        let end_byte = required_u64(evidence, "end_byte")?;
        if end_byte < start_byte {
            return Err(
                "evidence end_byte must be greater than or equal to start_byte".to_string(),
            );
        }

        Ok(())
    }

    fn validate_required_fields(
        object: &Map<String, Value>,
        required_fields: &[&str],
    ) -> Result<(), String> {
        for field in required_fields {
            if !object.contains_key(*field) {
                return Err(format!("missing required field {field}"));
            }
        }
        Ok(())
    }

    fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
        let value = object
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{field} must be a string"))?;
        if value.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
        Ok(value)
    }

    fn required_u64(object: &Map<String, Value>, field: &str) -> Result<u64, String> {
        object
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{field} must be a non-negative integer"))
    }

    fn allowed_message_types() -> [&'static str; 4] {
        [
            SemanticWorkerMessageKind::Fact.as_protocol_str(),
            SemanticWorkerMessageKind::Progress.as_protocol_str(),
            SemanticWorkerMessageKind::WorkerError.as_protocol_str(),
            SemanticWorkerMessageKind::EndOfStream.as_protocol_str(),
        ]
    }

    fn fixture_content(messages: Vec<Value>) -> String {
        messages
            .into_iter()
            .map(|message| message.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn valid_fact_message() -> Value {
        json!({
            "protocol_version": 1,
            "message_type": "fact",
            "request_id": "fixture-test",
            "fact_kind": "RESOLVED_IMPORT",
            "subject": "src/handlers/user.ts#import:express",
            "target": "node_modules/@types/express/index.d.ts#Request",
            "origin": {
                "engine": "typescript",
                "engine_version": "6.0.0",
                "method": "compiler_api"
            },
            "certainty": "SEMANTIC",
            "evidence": {
                "code_unit_id": "unit:src/handlers/user.ts#import:express",
                "path": "src/handlers/user.ts",
                "content_hash": "sha256:7c6e428e33561b59254d2efa13efac30fc391e9dc5d42f6c58132aaa8b2c8a03",
                "repository_revision": "fixture-rev",
                "start_byte": 0,
                "end_byte": 42,
                "note": "compiler resolved Express import target"
            },
            "assumptions": []
        })
    }

    fn valid_end_of_stream_message() -> Value {
        json!({
            "protocol_version": 1,
            "message_type": "end_of_stream",
            "request_id": "fixture-test"
        })
    }
}
