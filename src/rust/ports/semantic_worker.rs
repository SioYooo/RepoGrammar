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
    pub operations: Vec<SemanticWorkerOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticWorkerOperationKind {
    ResolveModuleSpecifier,
    ResolveExport,
    ResolveReexport,
    ResolvePackageEntry,
}

impl SemanticWorkerOperationKind {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::ResolveModuleSpecifier => "resolve_module_specifier",
            Self::ResolveExport => "resolve_export",
            Self::ResolveReexport => "resolve_reexport",
            Self::ResolvePackageEntry => "resolve_package_entry",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWorkerOperation {
    pub operation_id: String,
    pub operation: SemanticWorkerOperationKind,
    pub path: String,
    pub content_hash: String,
    pub code_unit_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub literal_specifier: String,
    pub project_config_hash: String,
    pub package_json_hash: String,
    pub max_files: usize,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticWorkerError {
    Unavailable(String),
    UnsupportedVersion(String),
    Timeout(String),
    WorkerCrashed(String),
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
    use crate::core::policy::paths::{looks_like_absolute_path, RepoRelativePathError};
    use serde_json::{json, Map, Value};
    use std::{collections::BTreeSet, fs, path::Path};

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
    fn request_schema_documents_rust_stdin_contract() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../protocol/semantic-worker-request.schema.json"
        ))
        .expect("request schema must parse as JSON");
        let required = schema["required"]
            .as_array()
            .expect("request schema must list required fields");

        assert_eq!(schema["additionalProperties"], false);
        for field in [
            "protocol_version",
            "request_id",
            "project_root",
            "changed_files",
            "operations",
        ] {
            assert!(
                required.iter().any(|candidate| candidate == field),
                "request schema must require {field}"
            );
        }
        assert_eq!(
            schema["properties"]["protocol_version"]["const"],
            SEMANTIC_WORKER_PROTOCOL_VERSION
        );
        assert_eq!(schema["properties"]["changed_files"]["uniqueItems"], true);
        assert_eq!(
            schema["properties"]["operations"]["items"]["additionalProperties"],
            false
        );
        assert_eq!(
            schema["properties"]["operations"]["items"]["properties"]["operation"]["enum"],
            json!([
                "resolve_module_specifier",
                "resolve_export",
                "resolve_reexport",
                "resolve_package_entry"
            ])
        );
        let changed_file_pattern = schema["properties"]["changed_files"]["items"]["pattern"]
            .as_str()
            .expect("changed file item must define a path-safety pattern");
        for fragment in [
            "(?!/)",
            "(?![A-Za-z]:)",
            "(?!.*//)",
            "\\u0000-\\u001F",
            "\\.\\.",
            ".*\\\\",
            "://",
        ] {
            assert!(
                changed_file_pattern.contains(fragment),
                "changed file pattern should constrain {fragment}"
            );
        }
    }

    #[test]
    fn schemas_reject_empty_fact_targets() {
        let message_schema: Value = serde_json::from_str(include_str!(
            "../../protocol/semantic-worker-message.schema.json"
        ))
        .expect("message schema must parse as JSON");
        assert_target_schema_rejects_empty_string(
            &message_schema["$defs"]["fact_message"]["properties"]["target"],
        );

        let fact_schema: Value =
            serde_json::from_str(include_str!("../../protocol/semantic-worker.schema.json"))
                .expect("fact schema must parse as JSON");
        assert_target_schema_rejects_empty_string(&fact_schema["properties"]["target"]);
    }

    #[test]
    fn schemas_require_repo_relative_evidence_paths() {
        let message_schema: Value = serde_json::from_str(include_str!(
            "../../protocol/semantic-worker-message.schema.json"
        ))
        .expect("message schema must parse as JSON");
        assert_evidence_path_schema_is_repo_relative(
            &message_schema["$defs"]["evidence"]["properties"]["path"],
        );

        let fact_schema: Value =
            serde_json::from_str(include_str!("../../protocol/semantic-worker.schema.json"))
                .expect("fact schema must parse as JSON");
        assert_evidence_path_schema_is_repo_relative(
            &fact_schema["properties"]["evidence"]["properties"]["path"],
        );
    }

    #[test]
    fn semantic_worker_request_fixture_matches_rust_stdin_contract() {
        let request: Value = serde_json::from_str(include_str!(
            "../../protocol/fixtures/typescript-worker-request.json"
        ))
        .expect("request fixture must parse as JSON");

        validate_worker_request(&request).expect("request fixture must match Rust stdin contract");
        assert_eq!(
            request["request_id"],
            "repogrammar-typescript-semantic-worker"
        );
        assert_eq!(request["changed_files"], json!(["src/a.ts", "src/b.tsx"]));
        assert_eq!(request["operations"], json!([]));
    }

    #[test]
    fn worker_request_validation_rejects_invalid_payloads() {
        let invalid_payloads = [
            Value::Null,
            json!({}),
            {
                let mut request = valid_worker_request();
                request["protocol_version"] = json!(2);
                request
            },
            {
                let mut request = valid_worker_request();
                request["protocol_version"] = json!("1");
                request
            },
            {
                let mut request = valid_worker_request();
                request["extra"] = json!(true);
                request
            },
            {
                let mut request = valid_worker_request();
                request
                    .as_object_mut()
                    .expect("request object")
                    .remove("project_root");
                request
            },
            {
                let mut request = valid_worker_request();
                request["project_root"] = json!("relative/root");
                request
            },
            {
                let mut request = valid_worker_request();
                request["project_root"] = json!("");
                request
            },
            {
                let mut request = valid_worker_request();
                request["project_root"] = json!(null);
                request
            },
            {
                let mut request = valid_worker_request();
                request["project_root"] = json!("/repo\u{0000}bad");
                request
            },
            {
                let mut request = valid_worker_request();
                request["request_id"] = json!(" ");
                request
            },
            {
                let mut request = valid_worker_request();
                request["request_id"] = json!(null);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = Value::Null;
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!([""]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!([null]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["src/a.ts", "src/a.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["/tmp/source.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["../secret.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["src/../secret.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["./src/a.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["src\\a.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["C:tmp/source.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["C:/tmp/source.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["C:\\tmp\\source.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["changed_files"] = json!(["file:///tmp/source.ts"]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"] = Value::Null;
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"] = json!([{"operation":"resolve_module_specifier"}]);
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"][0]["path"] = json!("/tmp/source.ts");
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"][0]["content_hash"] = json!("sha256:fixture");
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"][0]["start_byte"] = json!(2);
                request["operations"][0]["end_byte"] = json!(1);
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"][0]["literal_specifier"] = json!("import x from 'secret'");
                request
            },
            {
                let mut request = valid_worker_request();
                request["operations"][0]["max_files"] = json!(0);
                request
            },
        ];

        for payload in invalid_payloads {
            validate_worker_request(&payload).expect_err("invalid request payload must fail");
        }
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
    fn protocol_fixture_validation_rejects_unsafe_paths_and_text() {
        for bad_path in [
            "/tmp/source.ts",
            "file:///tmp/source.ts",
            "src\\source.ts",
            "../source.ts",
            "src/../source.ts",
            "src//source.ts",
            "src/\u{0008}/source.ts",
            "C:tmp/source.ts",
        ] {
            let mut fact = valid_fact_message();
            fact["evidence"]["path"] = json!(bad_path);
            let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

            let error = validate_protocol_fixture(&fixture)
                .expect_err("unsafe evidence path must be rejected");

            assert!(error.contains("path"), "unexpected error: {error}");
        }

        for target in [
            "/tmp/source.ts#Symbol",
            "file:///tmp/source.ts#Symbol",
            "const secret = true;",
            "const secret = true",
        ] {
            let mut fact = valid_fact_message();
            fact["target"] = json!(target);
            let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

            let error =
                validate_protocol_fixture(&fixture).expect_err("unsafe target must be rejected");

            assert!(error.contains("target"), "unexpected error: {error}");
        }

        for note in [
            "/tmp/source.ts",
            "file:///tmp/source.ts",
            "const secret = true;",
            "const secret = true",
            "import secret from 'secret'",
        ] {
            let mut fact = valid_fact_message();
            fact["evidence"]["note"] = json!(note);
            let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

            let error =
                validate_protocol_fixture(&fixture).expect_err("unsafe note must be rejected");

            assert!(error.contains("note"), "unexpected error: {error}");
        }
    }

    #[test]
    fn protocol_fixture_validation_rejects_empty_target() {
        for target in [json!(""), json!("   ")] {
            let mut fact = valid_fact_message();
            fact["target"] = target;
            let fixture = fixture_content(vec![fact, valid_end_of_stream_message()]);

            let error =
                validate_protocol_fixture(&fixture).expect_err("empty target must be rejected");

            assert!(error.contains("target must not be empty"));
        }
    }

    #[test]
    fn protocol_fixture_validation_accepts_null_and_non_empty_targets() {
        validate_protocol_fixture(&fixture_content(vec![
            valid_fact_message(),
            valid_end_of_stream_message(),
        ]))
        .expect("non-empty target must be accepted");

        let mut fact = valid_fact_message();
        fact["target"] = Value::Null;
        validate_protocol_fixture(&fixture_content(vec![fact, valid_end_of_stream_message()]))
            .expect("null target must be accepted");
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

    fn validate_worker_request(value: &Value) -> Result<(), String> {
        let object = value
            .as_object()
            .ok_or_else(|| "request must be a JSON object".to_string())?;
        validate_allowed_fields(
            object,
            &[
                "protocol_version",
                "request_id",
                "project_root",
                "changed_files",
                "operations",
            ],
        )?;
        validate_required_fields(
            object,
            &[
                "protocol_version",
                "request_id",
                "project_root",
                "changed_files",
                "operations",
            ],
        )?;

        let protocol_version = object
            .get("protocol_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "protocol_version must be an integer".to_string())?;
        if protocol_version != u64::from(SEMANTIC_WORKER_PROTOCOL_VERSION) {
            return Err(format!(
                "protocol_version must be {}",
                SEMANTIC_WORKER_PROTOCOL_VERSION
            ));
        }

        required_string(object, "request_id")?;
        let project_root = required_string(object, "project_root")?;
        if project_root.contains('\0') || !looks_like_absolute_path(project_root) {
            return Err("project_root must be an absolute path string".to_string());
        }

        let changed_files = object
            .get("changed_files")
            .and_then(Value::as_array)
            .ok_or_else(|| "changed_files must be an array".to_string())?;
        let mut seen = BTreeSet::new();
        for changed_file in changed_files {
            let changed_file = changed_file
                .as_str()
                .ok_or_else(|| "changed_files entries must be strings".to_string())?;
            validate_request_changed_file(changed_file)?;
            if !seen.insert(changed_file) {
                return Err("changed_files entries must be unique".to_string());
            }
        }

        let operations = object
            .get("operations")
            .and_then(Value::as_array)
            .ok_or_else(|| "operations must be an array".to_string())?;
        for operation in operations {
            validate_worker_operation(operation)?;
        }

        Ok(())
    }

    fn validate_worker_operation(value: &Value) -> Result<(), String> {
        let object = value
            .as_object()
            .ok_or_else(|| "operation must be a JSON object".to_string())?;
        validate_allowed_fields(
            object,
            &[
                "operation_id",
                "operation",
                "path",
                "content_hash",
                "code_unit_id",
                "start_byte",
                "end_byte",
                "literal_specifier",
                "project_config_hash",
                "package_json_hash",
                "max_files",
                "max_bytes",
            ],
        )?;
        validate_required_fields(
            object,
            &[
                "operation_id",
                "operation",
                "path",
                "content_hash",
                "code_unit_id",
                "start_byte",
                "end_byte",
                "literal_specifier",
                "project_config_hash",
                "package_json_hash",
                "max_files",
                "max_bytes",
            ],
        )?;
        validate_protocol_text(required_string(object, "operation_id")?, "operation_id")?;
        match required_string(object, "operation")? {
            "resolve_module_specifier"
            | "resolve_export"
            | "resolve_reexport"
            | "resolve_package_entry" => {}
            _ => return Err("operation token is not supported".to_string()),
        }
        validate_request_changed_file(required_string(object, "path")?)?;
        ContentHash::new(required_string(object, "content_hash")?)
            .map_err(|error| format!("invalid operation content_hash: {error}"))?;
        required_string(object, "code_unit_id")?;
        validate_protocol_text(
            required_string(object, "literal_specifier")?,
            "literal_specifier",
        )?;
        ContentHash::new(required_string(object, "project_config_hash")?)
            .map_err(|error| format!("invalid project_config_hash: {error}"))?;
        ContentHash::new(required_string(object, "package_json_hash")?)
            .map_err(|error| format!("invalid package_json_hash: {error}"))?;
        let start_byte = required_u64(object, "start_byte")?;
        let end_byte = required_u64(object, "end_byte")?;
        if start_byte > end_byte {
            return Err("operation range is invalid".to_string());
        }
        if required_u64(object, "max_files")? == 0 || required_u64(object, "max_bytes")? == 0 {
            return Err("operation bounds must be positive".to_string());
        }
        Ok(())
    }

    fn validate_allowed_fields(
        object: &Map<String, Value>,
        allowed_fields: &[&str],
    ) -> Result<(), String> {
        for field in object.keys() {
            if !allowed_fields.contains(&field.as_str()) {
                return Err("request contains unsupported field".to_string());
            }
        }
        Ok(())
    }

    fn validate_request_changed_file(path: &str) -> Result<(), String> {
        crate::core::policy::paths::validate_repo_relative_path(path).map_err(|error| match error {
            RepoRelativePathError::Traversal => {
                "changed file must not traverse outside repository".to_string()
            }
            RepoRelativePathError::Empty
            | RepoRelativePathError::Absolute
            | RepoRelativePathError::Backslash
            | RepoRelativePathError::ControlCharacter
            | RepoRelativePathError::UriLike => {
                "changed file must be repository-relative".to_string()
            }
        })
    }

    fn validate_protocol_text(value: &str, field: &str) -> Result<(), String> {
        if value.contains('\0')
            || value.contains('\n')
            || value.contains('\r')
            || value.contains("://")
            || value.split_whitespace().any(looks_like_absolute_path)
            || looks_like_source_snippet(value)
        {
            Err(format!("{field} contains unsupported content"))
        } else {
            Ok(())
        }
    }

    fn looks_like_source_snippet(value: &str) -> bool {
        let trimmed = value.trim_start();
        value.contains("=>")
            || (value.contains('=') && value.contains(';'))
            || value.contains('{')
            || value.contains('}')
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
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
                Value::String(value) if !value.trim().is_empty() => {
                    validate_protocol_text(value, "target")?
                }
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
        validate_request_changed_file(required_string(evidence, "path")?)
            .map_err(|error| format!("path {error}"))?;
        ContentHash::new(required_string(evidence, "content_hash")?)
            .map_err(|error| format!("invalid content_hash: {error}"))?;
        required_string(evidence, "repository_revision")?;
        validate_protocol_text(required_string(evidence, "note")?, "note")?;

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

    fn assert_target_schema_rejects_empty_string(target_schema: &Value) {
        let alternatives = target_schema["anyOf"]
            .as_array()
            .expect("target schema must use anyOf");
        assert!(alternatives.iter().any(|alternative| {
            alternative["type"] == "string"
                && alternative["minLength"].as_u64() == Some(1)
                && alternative["pattern"] == "\\S"
        }));
        assert!(alternatives
            .iter()
            .any(|alternative| alternative["type"] == "null"));
    }

    fn assert_evidence_path_schema_is_repo_relative(path_schema: &Value) {
        let pattern = path_schema["pattern"]
            .as_str()
            .expect("path schema must define a path-safety pattern");
        for fragment in [
            "(?!/)",
            "(?![A-Za-z]:)",
            "(?!.*//)",
            "\\u0000-\\u001F",
            "\\.\\.",
            ".*\\\\",
            "://",
        ] {
            assert!(
                pattern.contains(fragment),
                "evidence path pattern should constrain {fragment}"
            );
        }
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

    fn valid_worker_request() -> Value {
        json!({
            "protocol_version": 1,
            "request_id": "repogrammar-typescript-semantic-worker",
            "project_root": "/repo",
            "changed_files": ["src/a.ts", "src/b.tsx"],
            "operations": [{
                "operation": "resolve_module_specifier",
                "operation_id": "op-000001",
                "path": "src/a.ts",
                "content_hash": "sha256:7c6e428e33561b59254d2efa13efac30fc391e9dc5d42f6c58132aaa8b2c8a03",
                "code_unit_id": "unit:src/a.ts#module:0-42:0",
                "start_byte": 0,
                "end_byte": 42,
                "literal_specifier": "./b",
                "project_config_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "package_json_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "max_files": 100,
                "max_bytes": 1048576
            }]
        })
    }
}
