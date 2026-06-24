//! TypeScript semantic worker boundary.
//!
//! TypeScript semantic facts come from a versioned worker process that uses the
//! official TypeScript compiler or language-service API when available. This
//! adapter owns process execution and NDJSON validation; it never exposes
//! TypeScript compiler objects outside the adapter boundary.

use crate::core::model::{
    CodeUnitId, ContentHash, Evidence, FactCertainty, FactOrigin, Provenance, RepositoryRevision,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId,
};
use crate::ports::semantic_worker::{
    SemanticWorker, SemanticWorkerError, SemanticWorkerRequest, SEMANTIC_VERSION_UNSUPPORTED_CODE,
    SEMANTIC_WORKER_PROTOCOL_VERSION,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const PINNED_TYPESCRIPT_MAJOR_VERSION: u16 = 6;
pub const DEFAULT_SEMANTIC_WORKER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_WORKER_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_WORKER_LINE_BYTES: usize = 64 * 1024;
const REQUEST_ID: &str = "repogrammar-typescript-semantic-worker";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeScriptVersionSupport {
    SupportedCompilerApi { major: u16 },
    SyntaxOnlyFallback { reason_code: &'static str },
}

pub fn classify_typescript_version(version: &str) -> TypeScriptVersionSupport {
    match parse_major_version(version) {
        Some(PINNED_TYPESCRIPT_MAJOR_VERSION) => TypeScriptVersionSupport::SupportedCompilerApi {
            major: PINNED_TYPESCRIPT_MAJOR_VERSION,
        },
        _ => TypeScriptVersionSupport::SyntaxOnlyFallback {
            reason_code: SEMANTIC_VERSION_UNSUPPORTED_CODE,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptSemanticWorkerBoundary {
    pub executable: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

impl TypeScriptSemanticWorkerBoundary {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            timeout: DEFAULT_SEMANTIC_WORKER_TIMEOUT,
        }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl SemanticWorker for TypeScriptSemanticWorkerBoundary {
    fn analyze_project(
        &self,
        request: SemanticWorkerRequest,
    ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
        validate_request(&request)?;
        let allowed_paths = normalized_changed_files(&request.changed_files);
        let output = self.run_worker(request)?;
        parse_worker_output(&output, REQUEST_ID, &allowed_paths)
    }
}

fn parse_major_version(version: &str) -> Option<u16> {
    version.split('.').next()?.parse().ok()
}

fn validate_request(request: &SemanticWorkerRequest) -> Result<(), SemanticWorkerError> {
    if request.project_root.trim().is_empty() {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must not be empty".to_string(),
        ));
    }
    for changed_file in &request.changed_files {
        validate_repo_relative_path(changed_file).map_err(|_| {
            SemanticWorkerError::ProtocolViolation(
                "semantic worker changed files must be repository-relative".to_string(),
            )
        })?;
    }
    Ok(())
}

fn normalized_changed_files(changed_files: &[String]) -> BTreeSet<String> {
    changed_files.iter().cloned().collect()
}

impl TypeScriptSemanticWorkerBoundary {
    fn run_worker(&self, request: SemanticWorkerRequest) -> Result<String, SemanticWorkerError> {
        if self.executable.trim().is_empty() {
            return Err(SemanticWorkerError::Unavailable(
                "semantic worker executable is not configured".to_string(),
            ));
        }
        if !Path::new(&self.executable).is_absolute() {
            return Err(SemanticWorkerError::Unavailable(
                "semantic worker executable must be an absolute path".to_string(),
            ));
        }

        let mut changed_files = request.changed_files;
        changed_files.sort();
        changed_files.dedup();
        let payload = json!({
            "protocol_version": SEMANTIC_WORKER_PROTOCOL_VERSION,
            "request_id": REQUEST_ID,
            "project_root": request.project_root,
            "changed_files": changed_files,
        });

        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| {
            SemanticWorkerError::Unavailable(
                "semantic worker process could not be started".to_string(),
            )
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            SemanticWorkerError::Unavailable(
                "semantic worker stdin could not be opened".to_string(),
            )
        })?;
        let request_bytes = serde_json::to_vec(&payload).map_err(|_| {
            SemanticWorkerError::ProtocolViolation(
                "semantic worker request could not be serialized".to_string(),
            )
        })?;
        stdin.write_all(&request_bytes).map_err(|_| {
            SemanticWorkerError::Unavailable(
                "semantic worker request could not be written".to_string(),
            )
        })?;
        stdin.write_all(b"\n").map_err(|_| {
            SemanticWorkerError::Unavailable(
                "semantic worker request could not be written".to_string(),
            )
        })?;
        drop(stdin);

        let stdout = child.stdout.take().ok_or_else(|| {
            SemanticWorkerError::Unavailable(
                "semantic worker stdout could not be opened".to_string(),
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            SemanticWorkerError::Unavailable(
                "semantic worker stderr could not be opened".to_string(),
            )
        })?;
        let stdout_reader = thread::spawn(move || read_pipe(stdout));
        let stderr_reader = thread::spawn(move || read_pipe(stderr));

        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(|_| {
                SemanticWorkerError::Unavailable(
                    "semantic worker status could not be read".to_string(),
                )
            })? {
                let stdout = join_reader(stdout_reader)?;
                let _stderr = join_reader(stderr_reader)?;
                if !status.success() {
                    return Err(SemanticWorkerError::WorkerCrashed(
                        "semantic worker exited unsuccessfully".to_string(),
                    ));
                }
                return String::from_utf8(stdout).map_err(|_| {
                    SemanticWorkerError::ProtocolViolation(
                        "semantic worker stdout was not valid UTF-8".to_string(),
                    )
                });
            }

            if start.elapsed() >= self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(SemanticWorkerError::Timeout(
                    "semantic worker timed out".to_string(),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn read_pipe(mut pipe: impl Read) -> Result<Vec<u8>, SemanticWorkerError> {
    let mut output = Vec::new();
    pipe.by_ref()
        .take((MAX_WORKER_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .map_err(|_| {
            SemanticWorkerError::Unavailable("semantic worker output could not be read".to_string())
        })?;
    if output.len() > MAX_WORKER_OUTPUT_BYTES {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker output exceeded size limit".to_string(),
        ));
    }
    Ok(output)
}

fn join_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, SemanticWorkerError>>,
) -> Result<Vec<u8>, SemanticWorkerError> {
    reader.join().map_err(|_| {
        SemanticWorkerError::Unavailable("semantic worker output reader failed".to_string())
    })?
}

fn parse_worker_output(
    output: &str,
    expected_request_id: &str,
    allowed_paths: &BTreeSet<String>,
) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
    let mut facts = Vec::new();
    let mut saw_end_of_stream = false;

    for (line_index, line) in output.lines().enumerate() {
        let line_number = line_index + 1;
        if line.len() > MAX_WORKER_LINE_BYTES {
            return Err(protocol_error(
                line_number,
                "message line exceeded size limit",
            ));
        }
        if line.trim().is_empty() {
            return Err(protocol_error(
                line_number,
                "message line must not be empty",
            ));
        }
        if saw_end_of_stream {
            return Err(protocol_error(
                line_number,
                "message arrived after end_of_stream",
            ));
        }

        let message: Value = serde_json::from_str(line)
            .map_err(|_| protocol_error(line_number, "message line is not valid JSON"))?;
        let object = message
            .as_object()
            .ok_or_else(|| protocol_error(line_number, "message must be a JSON object"))?;
        validate_envelope(object, line_number, expected_request_id)?;

        match required_string(object, "message_type", line_number)? {
            "fact" => {
                let fact = parse_fact_message(object, line_number)?;
                validate_fact_scope(&fact, allowed_paths, line_number)?;
                facts.push(fact);
            }
            "progress" => validate_progress_message(object, line_number)?,
            "worker_error" => return Err(parse_worker_error(object, line_number)),
            "end_of_stream" => {
                validate_allowed_keys(
                    object,
                    &["protocol_version", "message_type", "request_id"],
                    line_number,
                    "end_of_stream",
                )?;
                saw_end_of_stream = true;
            }
            _ => {
                return Err(protocol_error(
                    line_number,
                    "message_type must be a supported semantic-worker message",
                ));
            }
        }
    }

    if saw_end_of_stream {
        Ok(facts)
    } else {
        Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker output did not include end_of_stream".to_string(),
        ))
    }
}

fn validate_fact_scope(
    fact: &SemanticFact,
    allowed_paths: &BTreeSet<String>,
    line_number: usize,
) -> Result<(), SemanticWorkerError> {
    if allowed_paths.is_empty() || allowed_paths.contains(&fact.evidence.provenance.path) {
        Ok(())
    } else {
        Err(protocol_error(
            line_number,
            "fact evidence path was not requested",
        ))
    }
}

fn validate_envelope(
    object: &Map<String, Value>,
    line_number: usize,
    expected_request_id: &str,
) -> Result<(), SemanticWorkerError> {
    let protocol_version = object
        .get("protocol_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol_error(line_number, "protocol_version must be an integer"))?;
    if protocol_version != u64::from(SEMANTIC_WORKER_PROTOCOL_VERSION) {
        return Err(protocol_error(
            line_number,
            "protocol_version must match semantic-worker protocol v1",
        ));
    }
    if required_string(object, "request_id", line_number)? != expected_request_id {
        return Err(protocol_error(
            line_number,
            "request_id must match the semantic-worker request",
        ));
    }
    Ok(())
}

fn parse_fact_message(
    object: &Map<String, Value>,
    line_number: usize,
) -> Result<SemanticFact, SemanticWorkerError> {
    validate_allowed_keys(
        object,
        &[
            "protocol_version",
            "message_type",
            "request_id",
            "fact_kind",
            "subject",
            "target",
            "origin",
            "certainty",
            "evidence",
            "assumptions",
        ],
        line_number,
        "fact",
    )?;
    let kind =
        SemanticFactKind::parse_protocol_str(required_string(object, "fact_kind", line_number)?)
            .map_err(|_| protocol_error(line_number, "fact_kind is not supported"))?;
    let subject = required_string(object, "subject", line_number)?.to_string();
    let target = parse_target(object.get("target"), line_number)?;
    let origin = parse_origin(object, line_number)?;
    let certainty =
        FactCertainty::parse_protocol_str(required_string(object, "certainty", line_number)?)
            .map_err(|_| protocol_error(line_number, "certainty is not supported"))?;
    let evidence = parse_evidence(object, line_number)?;
    let assumptions = object
        .get("assumptions")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol_error(line_number, "assumptions must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| protocol_error(line_number, "assumptions must contain strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;

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

fn parse_target(
    target: Option<&Value>,
    line_number: usize,
) -> Result<Option<SymbolId>, SemanticWorkerError> {
    match target {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => SymbolId::new(value.clone())
            .map(Some)
            .map_err(|_| protocol_error(line_number, "target must not be empty")),
        Some(Value::String(_)) => Err(protocol_error(line_number, "target must not be empty")),
        Some(_) => Err(protocol_error(
            line_number,
            "target must be a string or null",
        )),
    }
}

fn parse_origin(
    object: &Map<String, Value>,
    line_number: usize,
) -> Result<FactOrigin, SemanticWorkerError> {
    let origin = object
        .get("origin")
        .and_then(Value::as_object)
        .ok_or_else(|| protocol_error(line_number, "origin must be an object"))?;
    validate_allowed_keys(
        origin,
        &["engine", "engine_version", "method"],
        line_number,
        "origin",
    )?;
    Ok(FactOrigin {
        engine: required_string(origin, "engine", line_number)?.to_string(),
        engine_version: required_string(origin, "engine_version", line_number)?.to_string(),
        method: required_string(origin, "method", line_number)?.to_string(),
    })
}

fn parse_evidence(
    object: &Map<String, Value>,
    line_number: usize,
) -> Result<Evidence, SemanticWorkerError> {
    let evidence = object
        .get("evidence")
        .and_then(Value::as_object)
        .ok_or_else(|| protocol_error(line_number, "evidence must be an object"))?;
    validate_allowed_keys(
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
        line_number,
        "evidence",
    )?;
    let code_unit_id = CodeUnitId::new(required_string(evidence, "code_unit_id", line_number)?)
        .map_err(|_| protocol_error(line_number, "code_unit_id must not be empty"))?;
    let path = required_string(evidence, "path", line_number)?;
    validate_repo_relative_path(path)
        .map_err(|_| protocol_error(line_number, "evidence path must be repository-relative"))?;
    let content_hash = ContentHash::new(required_string(evidence, "content_hash", line_number)?)
        .map_err(|_| {
            protocol_error(line_number, "content_hash must match sha256:<64 hex chars>")
        })?;
    let repository_revision = RepositoryRevision::new(required_string(
        evidence,
        "repository_revision",
        line_number,
    )?)
    .map_err(|_| protocol_error(line_number, "repository_revision must not be empty"))?;
    let start_byte = required_usize(evidence, "start_byte", line_number)?;
    let end_byte = required_usize(evidence, "end_byte", line_number)?;
    let range = SourceRange::new(start_byte, end_byte)
        .map_err(|_| protocol_error(line_number, "evidence byte range is invalid"))?;
    let provenance = Provenance::new(path, content_hash, repository_revision)
        .map_err(|_| protocol_error(line_number, "evidence provenance is invalid"))?;
    Evidence::new(
        code_unit_id,
        range,
        provenance,
        required_string(evidence, "note", line_number)?,
    )
    .map_err(|_| protocol_error(line_number, "evidence note must not be empty"))
}

fn validate_progress_message(
    object: &Map<String, Value>,
    line_number: usize,
) -> Result<(), SemanticWorkerError> {
    validate_allowed_keys(
        object,
        &[
            "protocol_version",
            "message_type",
            "request_id",
            "stage",
            "message",
            "work",
        ],
        line_number,
        "progress",
    )?;
    required_string(object, "stage", line_number)?;
    required_string(object, "message", line_number)?;
    let work = object
        .get("work")
        .and_then(Value::as_object)
        .ok_or_else(|| protocol_error(line_number, "work must be an object"))?;
    match required_string(work, "kind", line_number)? {
        "unknown" => {
            validate_allowed_keys(work, &["kind"], line_number, "work")?;
            Ok(())
        }
        "known" => {
            validate_allowed_keys(work, &["kind", "completed", "total"], line_number, "work")?;
            let completed = required_u64(work, "completed", line_number)?;
            let total = required_u64(work, "total", line_number)?;
            if completed > total {
                Err(protocol_error(
                    line_number,
                    "completed work must not exceed total work",
                ))
            } else {
                Ok(())
            }
        }
        _ => Err(protocol_error(line_number, "work kind is not supported")),
    }
}

fn parse_worker_error(object: &Map<String, Value>, line_number: usize) -> SemanticWorkerError {
    if let Err(error) = validate_allowed_keys(
        object,
        &[
            "protocol_version",
            "message_type",
            "request_id",
            "error_code",
            "message",
            "fallback",
        ],
        line_number,
        "worker_error",
    ) {
        return error;
    }
    let error_code = match required_string(object, "error_code", line_number) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if let Err(error) = required_string(object, "message", line_number) {
        return error;
    }
    if let Some(fallback) = object.get("fallback") {
        let fallback = match fallback.as_object() {
            Some(value) => value,
            None => return protocol_error(line_number, "fallback must be an object"),
        };
        if let Err(error) =
            validate_allowed_keys(fallback, &["mode", "certainty"], line_number, "fallback")
        {
            return error;
        }
        match required_string(fallback, "mode", line_number) {
            Ok("syntax_only") => {}
            Ok(_) => return protocol_error(line_number, "fallback mode is not supported"),
            Err(error) => return error,
        }
        match required_string(fallback, "certainty", line_number) {
            Ok("STRUCTURAL" | "UNKNOWN") => {}
            Ok(_) => return protocol_error(line_number, "fallback certainty is not supported"),
            Err(error) => return error,
        }
    }

    match error_code {
        "SEMANTIC_VERSION_UNSUPPORTED" => SemanticWorkerError::UnsupportedVersion(
            "semantic worker reported unsupported TypeScript version".to_string(),
        ),
        "SEMANTIC_WORKER_UNAVAILABLE" => {
            SemanticWorkerError::Unavailable("semantic worker reported unavailable".to_string())
        }
        "SEMANTIC_PROTOCOL_VIOLATION" => SemanticWorkerError::ProtocolViolation(
            "semantic worker reported protocol violation".to_string(),
        ),
        _ => protocol_error(line_number, "worker error_code is not supported"),
    }
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<&'a str, SemanticWorkerError> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_error(line_number, &format!("{field} must be a string")))?;
    if value.trim().is_empty() {
        Err(protocol_error(
            line_number,
            &format!("{field} must not be empty"),
        ))
    } else {
        Ok(value)
    }
}

fn validate_allowed_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    line_number: usize,
    context: &str,
) -> Result<(), SemanticWorkerError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(protocol_error(
                line_number,
                &format!("{context} contains unsupported field {key}"),
            ));
        }
    }
    Ok(())
}

fn required_u64(
    object: &Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<u64, SemanticWorkerError> {
    object.get(field).and_then(Value::as_u64).ok_or_else(|| {
        protocol_error(
            line_number,
            &format!("{field} must be a non-negative integer"),
        )
    })
}

fn required_usize(
    object: &Map<String, Value>,
    field: &str,
    line_number: usize,
) -> Result<usize, SemanticWorkerError> {
    let value = required_u64(object, field, line_number)?;
    usize::try_from(value)
        .map_err(|_| protocol_error(line_number, &format!("{field} is too large")))
}

fn protocol_error(line_number: usize, message: &str) -> SemanticWorkerError {
    SemanticWorkerError::ProtocolViolation(format!("line {line_number}: {message}"))
}

fn validate_repo_relative_path(path: &str) -> Result<(), ()> {
    if path.trim().is_empty() || Path::new(path).is_absolute() {
        return Err(());
    }
    if path.contains('\\')
        || path.contains("://")
        || path.contains('\0')
        || looks_like_windows_absolute_path(path)
    {
        return Err(());
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::Prefix(_)
            | Component::RootDir => return Err(()),
        }
    }
    Ok(())
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::semantic_worker::{SemanticWorker, SemanticWorkerMessageKind};
    use crate::test_support::TempWorkspace;
    use serde_json::json;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    #[test]
    fn typescript_six_uses_supported_compiler_api_boundary() {
        assert_eq!(
            classify_typescript_version("6.0.0"),
            TypeScriptVersionSupport::SupportedCompilerApi { major: 6 }
        );
    }

    #[test]
    fn unsupported_typescript_versions_fall_back_to_syntax_only() {
        for version in ["7.0.0-dev", "5.9.0", "", "not-a-version", "v6.0.0"] {
            assert_eq!(
                classify_typescript_version(version),
                TypeScriptVersionSupport::SyntaxOnlyFallback {
                    reason_code: SEMANTIC_VERSION_UNSUPPORTED_CODE
                }
            );
        }
    }

    #[test]
    fn worker_output_parser_accepts_fact_progress_and_end_of_stream() {
        let facts = parse_worker_output(
            &ndjson(vec![
                valid_progress_message(),
                valid_fact_message(),
                valid_end_of_stream_message(),
            ]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect("worker output should parse");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].kind, SemanticFactKind::ResolvedImport);
        assert_eq!(facts[0].subject, "src/handlers/user.ts#import:express");
        assert_eq!(
            facts[0].target.as_ref().map(SymbolId::as_str),
            Some("node_modules/@types/express/index.d.ts#Request")
        );
        assert_eq!(facts[0].certainty, FactCertainty::Semantic);
        assert_eq!(facts[0].evidence.provenance.path, "src/handlers/user.ts");
    }

    #[test]
    fn worker_output_parser_accepts_null_target() {
        let mut fact = valid_fact_message();
        fact["target"] = Value::Null;
        let facts = parse_worker_output(
            &ndjson(vec![fact, valid_end_of_stream_message()]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect("null target should parse");

        assert_eq!(facts[0].target, None);
    }

    #[test]
    fn worker_output_parser_rejects_malformed_messages() {
        let malformed_outputs = [
            "{not-json}\n".to_string(),
            ndjson(vec![valid_fact_message()]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["target"] = json!("   ");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["content_hash"] = json!("sha256:test");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["path"] = json!("/tmp/source.ts");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["path"] = json!("file:///tmp/source.ts");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["source"] = json!("const secret = true;");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["snippet"] = json!("const secret = true;");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut progress = valid_progress_message();
                    progress["work"]["completed"] = json!(2);
                    progress["work"]["total"] = json!(1);
                    progress
                },
                valid_end_of_stream_message(),
            ]),
        ];

        for output in malformed_outputs {
            let error = parse_worker_output(&output, REQUEST_ID, &BTreeSet::new())
                .expect_err("malformed worker output must be rejected");
            assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
        }
    }

    #[test]
    fn worker_output_parser_rejects_unrequested_fact_paths_and_oversized_output() {
        let allowed_paths = BTreeSet::from(["src/other.ts".to_string()]);
        let error = parse_worker_output(
            &ndjson(vec![valid_fact_message(), valid_end_of_stream_message()]),
            REQUEST_ID,
            &allowed_paths,
        )
        .expect_err("fact outside requested path scope must fail");
        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));

        let oversized = vec![b'a'; MAX_WORKER_OUTPUT_BYTES + 1];
        let error = read_pipe(std::io::Cursor::new(oversized))
            .expect_err("oversized worker output must fail");
        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
    }

    #[test]
    fn worker_error_messages_are_sanitized_and_typed() {
        let unsupported = parse_worker_output(
            &ndjson(vec![
                json!({
                    "protocol_version": 1,
                    "message_type": "worker_error",
                    "request_id": REQUEST_ID,
                    "error_code": "SEMANTIC_VERSION_UNSUPPORTED",
                    "message": "/tmp/secret/project uses unsupported TypeScript",
                    "fallback": {
                        "mode": "syntax_only",
                        "certainty": "UNKNOWN"
                    }
                }),
                valid_end_of_stream_message(),
            ]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("worker_error should return an error");

        assert!(matches!(
            unsupported,
            SemanticWorkerError::UnsupportedVersion(_)
        ));
        assert!(!format!("{unsupported:?}").contains("/tmp/secret"));
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_runs_worker_and_parses_stdout() {
        let workspace = TempWorkspace::new("typescript-worker-process");
        let script = executable_script(
            &workspace,
            "worker.sh",
            &format!(
                "#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'EOF'\n{}\nEOF\n",
                ndjson(vec![valid_fact_message(), valid_end_of_stream_message()])
            ),
        );
        let worker = TypeScriptSemanticWorkerBoundary::new(script.display().to_string())
            .with_timeout(Duration::from_secs(2));

        let facts = worker
            .analyze_project(SemanticWorkerRequest {
                project_root: workspace.path().display().to_string(),
                changed_files: vec!["src/handlers/user.ts".to_string()],
            })
            .expect("worker process should return facts");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].kind, SemanticFactKind::ResolvedImport);
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_rejects_crash_timeout_and_leaky_stderr_without_leaking() {
        let workspace = TempWorkspace::new("typescript-worker-failures");
        let crash = executable_script(
            &workspace,
            "crash.sh",
            "#!/bin/sh\n/bin/cat >/dev/null\necho '/tmp/secret/source.ts UNIQUE_SENTINEL' >&2\nexit 2\n",
        );
        let crashed = TypeScriptSemanticWorkerBoundary::new(crash.display().to_string())
            .with_timeout(Duration::from_secs(2))
            .analyze_project(valid_request(&workspace))
            .expect_err("non-zero worker exit must fail");
        assert!(matches!(crashed, SemanticWorkerError::WorkerCrashed(_)));
        assert!(!format!("{crashed:?}").contains("UNIQUE_SENTINEL"));
        assert!(!format!("{crashed:?}").contains("/tmp/secret"));

        let slow = executable_script(&workspace, "slow.sh", "#!/bin/sh\n/bin/sleep 1\n");
        let timed_out = TypeScriptSemanticWorkerBoundary::new(slow.display().to_string())
            .with_timeout(Duration::from_millis(20))
            .analyze_project(valid_request(&workspace))
            .expect_err("slow worker must time out");
        assert!(matches!(timed_out, SemanticWorkerError::Timeout(_)));
    }

    #[test]
    fn boundary_rejects_invalid_request_paths() {
        let worker = TypeScriptSemanticWorkerBoundary::new("/unused");
        let error = worker
            .analyze_project(SemanticWorkerRequest {
                project_root: ".".to_string(),
                changed_files: vec!["../secret.ts".to_string()],
            })
            .expect_err("traversal changed file must be rejected before spawn");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));

        let error = TypeScriptSemanticWorkerBoundary::new("relative-worker")
            .analyze_project(SemanticWorkerRequest {
                project_root: ".".to_string(),
                changed_files: Vec::new(),
            })
            .expect_err("relative worker executable must be rejected before spawn");
        assert!(matches!(error, SemanticWorkerError::Unavailable(_)));
    }

    fn valid_request(workspace: &TempWorkspace) -> SemanticWorkerRequest {
        SemanticWorkerRequest {
            project_root: workspace.path().display().to_string(),
            changed_files: vec!["src/handlers/user.ts".to_string()],
        }
    }

    #[cfg(unix)]
    fn executable_script(workspace: &TempWorkspace, name: &str, body: &str) -> std::path::PathBuf {
        let path = workspace.path().join(name);
        fs::write(&path, body).expect("write worker script");
        let mut permissions = fs::metadata(&path)
            .expect("read worker script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make worker script executable");
        path
    }

    fn ndjson(messages: Vec<Value>) -> String {
        messages
            .into_iter()
            .map(|message| message.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn valid_progress_message() -> Value {
        json!({
            "protocol_version": 1,
            "message_type": SemanticWorkerMessageKind::Progress.as_protocol_str(),
            "request_id": REQUEST_ID,
            "stage": "semantic_resolution",
            "message": "resolving TypeScript symbols",
            "work": {
                "kind": "known",
                "completed": 1,
                "total": 2
            }
        })
    }

    fn valid_fact_message() -> Value {
        json!({
            "protocol_version": 1,
            "message_type": SemanticWorkerMessageKind::Fact.as_protocol_str(),
            "request_id": REQUEST_ID,
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
            "message_type": SemanticWorkerMessageKind::EndOfStream.as_protocol_str(),
            "request_id": REQUEST_ID
        })
    }
}
