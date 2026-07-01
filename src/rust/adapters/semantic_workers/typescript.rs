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
use crate::core::policy::paths::looks_like_absolute_path;
use crate::ports::semantic_worker::{
    SemanticWorker, SemanticWorkerError, SemanticWorkerRequest, SEMANTIC_VERSION_UNSUPPORTED_CODE,
    SEMANTIC_WORKER_PROTOCOL_VERSION,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const PINNED_TYPESCRIPT_MAJOR_VERSION: u16 = 6;
pub const DEFAULT_SEMANTIC_WORKER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_WORKER_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_WORKER_LINE_BYTES: usize = 64 * 1024;
const MAX_WORKER_STDIN_BYTES: usize = 1024 * 1024;
const WORKER_REQUEST_TERMINATOR_BYTES: usize = 1;
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
        mut request: SemanticWorkerRequest,
    ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
        validate_request(&mut request)?;
        let allowed_paths = normalized_changed_files(&request.changed_files);
        let output = self.run_worker(request)?;
        parse_worker_output(&output, REQUEST_ID, &allowed_paths)
    }
}

fn parse_major_version(version: &str) -> Option<u16> {
    version.split('.').next()?.parse().ok()
}

fn validate_request(request: &mut SemanticWorkerRequest) -> Result<(), SemanticWorkerError> {
    request.project_root = validate_project_root(&request.project_root)?;
    for changed_file in &request.changed_files {
        validate_repo_relative_path(changed_file).map_err(|_| {
            SemanticWorkerError::ProtocolViolation(
                "semantic worker changed files must be repository-relative".to_string(),
            )
        })?;
    }
    Ok(())
}

fn validate_project_root(project_root: &str) -> Result<String, SemanticWorkerError> {
    if project_root.trim().is_empty() || project_root.contains('\0') {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must be a valid absolute directory".to_string(),
        ));
    }
    let path = Path::new(project_root);
    if !path.is_absolute() {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must be absolute".to_string(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| {
        SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must be a readable directory".to_string(),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must be a real directory".to_string(),
        ));
    }
    let canonical = path.canonicalize().map_err(|_| {
        SemanticWorkerError::ProtocolViolation(
            "semantic worker project root must be canonicalizable".to_string(),
        )
    })?;
    Ok(canonical.display().to_string())
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

        let request_bytes = worker_request_bytes(request)?;

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
                let stdout = join_reader_before_deadline(stdout_reader, start, self.timeout)?;
                let _stderr = join_reader_before_deadline(stderr_reader, start, self.timeout)?;
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
                let _ = join_reader_before_deadline(stdout_reader, start, self.timeout);
                let _ = join_reader_before_deadline(stderr_reader, start, self.timeout);
                return Err(SemanticWorkerError::Timeout(
                    "semantic worker timed out".to_string(),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn worker_request_bytes(
    mut request: SemanticWorkerRequest,
) -> Result<Vec<u8>, SemanticWorkerError> {
    request.changed_files.sort();
    request.changed_files.dedup();
    let payload = json!({
        "protocol_version": SEMANTIC_WORKER_PROTOCOL_VERSION,
        "request_id": REQUEST_ID,
        "project_root": request.project_root,
        "changed_files": request.changed_files,
    });
    let request_bytes = serde_json::to_vec(&payload).map_err(|_| {
        SemanticWorkerError::ProtocolViolation(
            "semantic worker request could not be serialized".to_string(),
        )
    })?;
    if request_bytes
        .len()
        .saturating_add(WORKER_REQUEST_TERMINATOR_BYTES)
        > MAX_WORKER_STDIN_BYTES
    {
        return Err(SemanticWorkerError::ProtocolViolation(
            "semantic worker request exceeded stdin size limit".to_string(),
        ));
    }
    Ok(request_bytes)
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

fn join_reader_before_deadline(
    reader: thread::JoinHandle<Result<Vec<u8>, SemanticWorkerError>>,
    start: Instant,
    timeout: Duration,
) -> Result<Vec<u8>, SemanticWorkerError> {
    let reader = reader;
    while !reader.is_finished() {
        if start.elapsed() >= timeout {
            return Err(SemanticWorkerError::Timeout(
                "semantic worker timed out".to_string(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    join_reader(reader)
}

fn parse_worker_output(
    output: &str,
    expected_request_id: &str,
    allowed_paths: &BTreeSet<String>,
) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
    let mut facts = Vec::new();
    let mut saw_end_of_stream = false;
    let mut worker_error = None;

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
        let message_type = required_string(object, "message_type", line_number)?;

        if worker_error.is_some() && message_type != "end_of_stream" {
            return Err(protocol_error(
                line_number,
                "only end_of_stream may follow worker_error",
            ));
        }

        match message_type {
            "fact" => {
                let fact = parse_fact_message(object, line_number)?;
                validate_fact_scope(&fact, allowed_paths, line_number)?;
                facts.push(fact);
            }
            "progress" => validate_progress_message(object, line_number)?,
            "worker_error" => worker_error = Some(parse_worker_error(object, line_number)),
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
        match worker_error {
            Some(error) => Err(error),
            None => Ok(facts),
        }
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
    if allowed_paths.contains(&fact.evidence.provenance.path) {
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
    let subject = protocol_text(
        required_string(object, "subject", line_number)?,
        line_number,
    )?;
    let target = parse_target(object.get("target"), line_number)?;
    let origin = parse_origin(object, line_number)?;
    let certainty =
        FactCertainty::parse_protocol_str(required_string(object, "certainty", line_number)?)
            .map_err(|_| protocol_error(line_number, "certainty is not supported"))?;
    validate_semantic_version_support(&origin, certainty, line_number)?;
    let evidence = parse_evidence(object, line_number)?;
    let assumptions = object
        .get("assumptions")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol_error(line_number, "assumptions must be an array"))?
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .ok_or_else(|| protocol_error(line_number, "assumptions must contain strings"))?;
            protocol_text(value, line_number)
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

fn validate_semantic_version_support(
    origin: &FactOrigin,
    certainty: FactCertainty,
    line_number: usize,
) -> Result<(), SemanticWorkerError> {
    if origin.engine == "typescript"
        && certainty == FactCertainty::Semantic
        && !matches!(
            classify_typescript_version(&origin.engine_version),
            TypeScriptVersionSupport::SupportedCompilerApi { .. }
        )
    {
        Err(SemanticWorkerError::UnsupportedVersion(format!(
            "line {line_number}: semantic worker reported unsupported TypeScript version"
        )))
    } else {
        Ok(())
    }
}

fn parse_target(
    target: Option<&Value>,
    line_number: usize,
) -> Result<Option<SymbolId>, SemanticWorkerError> {
    match target {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => {
            let value = protocol_text(value, line_number)?;
            SymbolId::new(value)
                .map(Some)
                .map_err(|_| protocol_error(line_number, "target must not be empty"))
        }
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
        engine: protocol_text(required_string(origin, "engine", line_number)?, line_number)?,
        engine_version: protocol_text(
            required_string(origin, "engine_version", line_number)?,
            line_number,
        )?,
        method: protocol_text(required_string(origin, "method", line_number)?, line_number)?,
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
        protocol_text(required_string(evidence, "note", line_number)?, line_number)?,
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
                &format!("{context} contains unsupported field"),
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

fn protocol_text(value: &str, line_number: usize) -> Result<String, SemanticWorkerError> {
    if value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains("://")
        || looks_like_embedded_absolute_path(value)
        || looks_like_source_snippet(value)
    {
        Err(protocol_error(
            line_number,
            "text field contains unsupported content",
        ))
    } else {
        Ok(value.to_string())
    }
}

fn looks_like_embedded_absolute_path(value: &str) -> bool {
    value.split_whitespace().any(looks_like_absolute_path)
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

fn validate_repo_relative_path(path: &str) -> Result<(), ()> {
    crate::core::policy::paths::validate_repo_relative_path(path).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::semantic_worker::{SemanticWorker, SemanticWorkerMessageKind};
    use crate::test_support::TempWorkspace;
    use serde_json::json;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[cfg(unix)]
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
            &requested_fact_paths(),
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
            &requested_fact_paths(),
        )
        .expect("null target should parse");

        assert_eq!(facts[0].target, None);
    }

    #[test]
    fn worker_output_parser_accepts_framework_role_facts() {
        let mut fact = valid_fact_message();
        fact["fact_kind"] = json!("FRAMEWORK_ROLE");
        fact["subject"] = json!("unit:src/handlers/user.ts#react_component:UserCard");
        fact["target"] = json!("framework:react.component");
        fact["origin"]["engine"] = json!("repogrammar-frameworks");
        fact["origin"]["engine_version"] = json!(env!("CARGO_PKG_VERSION"));
        fact["origin"]["method"] = json!("syntax_code_unit_kind");
        fact["certainty"] = json!("FRAMEWORK_HEURISTIC");
        fact["evidence"]["code_unit_id"] =
            json!("unit:src/handlers/user.ts#react_component:UserCard");
        fact["evidence"]["note"] = json!("syntax code unit indicates React component role");
        fact["assumptions"] = json!(["component runtime behavior unresolved"]);

        let facts = parse_worker_output(
            &ndjson(vec![fact, valid_end_of_stream_message()]),
            REQUEST_ID,
            &requested_fact_paths(),
        )
        .expect("framework role fact should parse");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].kind, SemanticFactKind::FrameworkRole);
        assert_eq!(facts[0].certainty, FactCertainty::FrameworkHeuristic);
        assert_eq!(
            facts[0].target.as_ref().map(SymbolId::as_str),
            Some("framework:react.component")
        );
        assert_eq!(facts[0].origin.engine, "repogrammar-frameworks");
        assert_eq!(facts[0].origin.method, "syntax_code_unit_kind");
        assert_eq!(facts[0].evidence.provenance.path, "src/handlers/user.ts");
        assert_eq!(
            facts[0].assumptions,
            vec!["component runtime behavior unresolved".to_string()]
        );
    }

    #[test]
    fn worker_output_parser_rejects_unsupported_semantic_typescript_versions() {
        let mut fact = valid_fact_message();
        fact["origin"]["engine_version"] = json!("7.0.0-dev");

        let error = parse_worker_output(
            &ndjson(vec![fact, valid_end_of_stream_message()]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("unsupported TypeScript SEMANTIC fact must fail");

        assert!(matches!(error, SemanticWorkerError::UnsupportedVersion(_)));
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
                    let mut fact = valid_fact_message();
                    fact["evidence"]["note"] = json!("const secret = true;");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["note"] = json!("const secret = true");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["evidence"]["note"] = json!("import secret from 'secret'");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["target"] = json!("/tmp/secret/source.ts#Symbol");
                    fact
                },
                valid_end_of_stream_message(),
            ]),
            ndjson(vec![
                {
                    let mut fact = valid_fact_message();
                    fact["assumptions"] = json!(["read /tmp/secret"]);
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
    fn worker_output_parser_accepts_normalized_symbol_text() {
        let mut fact = valid_fact_message();
        fact["target"] = json!("symbol:src/handlers/user.ts#export:UserService");
        fact["evidence"]["note"] = json!("compiler resolved normalized symbol id");

        let facts = parse_worker_output(
            &ndjson(vec![fact, valid_end_of_stream_message()]),
            REQUEST_ID,
            &requested_fact_paths(),
        )
        .expect("normalized symbol text should parse");

        assert_eq!(
            facts[0].target.as_ref().map(SymbolId::as_str),
            Some("symbol:src/handlers/user.ts#export:UserService")
        );
    }

    #[test]
    fn worker_output_parser_rejects_unrequested_fact_paths_and_oversized_output() {
        let empty_scope_error = parse_worker_output(
            &ndjson(vec![valid_fact_message(), valid_end_of_stream_message()]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("facts must not be accepted for an empty request scope");
        assert!(matches!(
            empty_scope_error,
            SemanticWorkerError::ProtocolViolation(_)
        ));

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
    fn unsupported_protocol_fields_do_not_leak_field_names() {
        let mut fact = valid_fact_message();
        fact["/tmp/secret UNIQUE_SENTINEL"] = json!(true);
        let error = parse_worker_output(
            &ndjson(vec![fact, valid_end_of_stream_message()]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("unsupported field must fail");

        let debug = format!("{error:?}");
        assert!(!debug.contains("/tmp/secret"));
        assert!(!debug.contains("UNIQUE_SENTINEL"));
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

    #[test]
    fn worker_error_must_still_close_with_end_of_stream() {
        let error = parse_worker_output(
            &ndjson(vec![json!({
                "protocol_version": 1,
                "message_type": "worker_error",
                "request_id": REQUEST_ID,
                "error_code": "SEMANTIC_WORKER_UNAVAILABLE",
                "message": "semantic worker unavailable",
                "fallback": {
                    "mode": "syntax_only",
                    "certainty": "UNKNOWN"
                }
            })]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("worker_error without EOS must fail");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));

        let error = parse_worker_output(
            &ndjson(vec![
                json!({
                    "protocol_version": 1,
                    "message_type": "worker_error",
                    "request_id": REQUEST_ID,
                    "error_code": "SEMANTIC_WORKER_UNAVAILABLE",
                    "message": "semantic worker unavailable",
                    "fallback": {
                        "mode": "syntax_only",
                        "certainty": "UNKNOWN"
                    }
                }),
                valid_progress_message(),
                valid_end_of_stream_message(),
            ]),
            REQUEST_ID,
            &BTreeSet::new(),
        )
        .expect_err("only EOS may follow worker_error");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
    }

    #[test]
    fn request_fixture_matches_worker_stdin_payload_and_sorts_files() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../protocol/fixtures/typescript-worker-request.json"
        ))
        .expect("request fixture must parse as JSON");
        let payload = worker_request_bytes(SemanticWorkerRequest {
            project_root: "/repo".to_string(),
            changed_files: vec![
                "src/b.tsx".to_string(),
                "src/a.ts".to_string(),
                "src/b.tsx".to_string(),
            ],
        })
        .expect("request payload must serialize");
        let payload: Value = serde_json::from_slice(&payload).expect("payload must parse");

        assert_eq!(payload, fixture);
    }

    #[test]
    fn request_serialization_accepts_many_changed_files_below_worker_stdin_limit() {
        let changed_files = (0..10_000)
            .map(|index| format!("src/file-{index:05}.ts"))
            .collect::<Vec<_>>();

        let payload = worker_request_bytes(SemanticWorkerRequest {
            project_root: "/repo".to_string(),
            changed_files,
        })
        .expect("many changed files below the worker stdin limit should serialize");

        assert!(payload.len() + WORKER_REQUEST_TERMINATOR_BYTES > 4 * 1024);
        assert!(payload.len() + WORKER_REQUEST_TERMINATOR_BYTES <= MAX_WORKER_STDIN_BYTES);
        let value: Value = serde_json::from_slice(&payload).expect("payload must parse");
        let changed_files = value["changed_files"]
            .as_array()
            .expect("changed files must be an array");
        assert_eq!(changed_files.len(), 10_000);
        assert_eq!(
            changed_files.first().and_then(serde_json::Value::as_str),
            Some("src/file-00000.ts")
        );
        assert_eq!(
            changed_files.last().and_then(serde_json::Value::as_str),
            Some("src/file-09999.ts")
        );
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
    fn process_boundary_preserves_timeout_after_child_exits_with_inherited_pipe() {
        let workspace = TempWorkspace::new("typescript-worker-inherited-pipe");
        let inherited_pipe = executable_script(
            &workspace,
            "inherited-pipe.sh",
            "#!/bin/sh\nexec 3>&1\n/usr/bin/nohup /bin/sleep 1 >&3 2>&3 </dev/null &\nexec 3>&-\nexit 0\n",
        );

        let error = TypeScriptSemanticWorkerBoundary::new(inherited_pipe.display().to_string())
            .with_timeout(Duration::from_millis(20))
            .analyze_project(valid_request(&workspace))
            .expect_err("inherited pipe must still honor worker timeout");

        assert!(matches!(error, SemanticWorkerError::Timeout(_)));
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_timeout_does_not_wait_for_inherited_pipes_after_kill() {
        let workspace = TempWorkspace::new("typescript-worker-timeout-inherited-pipe");
        let inherited_pipe = executable_script(
            &workspace,
            "timeout-inherited-pipe.sh",
            "#!/bin/sh\nexec 3>&1\n/usr/bin/nohup /bin/sleep 1 >&3 2>&3 </dev/null &\nexec 3>&-\n/bin/sleep 1\n",
        );
        let worker = TypeScriptSemanticWorkerBoundary::new(inherited_pipe.display().to_string())
            .with_timeout(Duration::from_millis(20));

        let started = Instant::now();
        let error = worker
            .analyze_project(valid_request(&workspace))
            .expect_err("timed-out worker with inherited pipe must return");

        assert!(matches!(error, SemanticWorkerError::Timeout(_)));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "timeout path waited for inherited pipe holder"
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_sends_sorted_deduplicated_changed_files() {
        let workspace = TempWorkspace::new("typescript-worker-request-order");
        let request_path = workspace.path().join("request.json");
        let script = executable_script(
            &workspace,
            "capture.sh",
            "#!/bin/sh\n/bin/cat > \"$1\"\n/bin/cat <<'EOF'\n{\"protocol_version\":1,\"message_type\":\"end_of_stream\",\"request_id\":\"repogrammar-typescript-semantic-worker\"}\nEOF\n",
        );
        let worker = TypeScriptSemanticWorkerBoundary::new(script.display().to_string())
            .with_args([request_path.display().to_string()])
            .with_timeout(Duration::from_secs(2));

        let facts = worker
            .analyze_project(SemanticWorkerRequest {
                project_root: workspace.path().display().to_string(),
                changed_files: vec![
                    "src/z.ts".to_string(),
                    "src/a.ts".to_string(),
                    "src/z.ts".to_string(),
                ],
            })
            .expect("worker should accept EOS-only response");

        assert!(facts.is_empty());
        let request_json: Value = serde_json::from_str(
            &fs::read_to_string(request_path).expect("captured worker request"),
        )
        .expect("request should be JSON");
        assert_eq!(
            request_json["changed_files"],
            json!(["src/a.ts", "src/z.ts"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_boundary_rejects_facts_for_empty_request_scope() {
        let workspace = TempWorkspace::new("typescript-worker-empty-scope-facts");
        let script = executable_script(
            &workspace,
            "fact-for-empty-scope.sh",
            &format!(
                "#!/bin/sh\n/bin/cat >/dev/null\n/bin/cat <<'EOF'\n{}\nEOF\n",
                ndjson(vec![valid_fact_message(), valid_end_of_stream_message()])
            ),
        );
        let worker = TypeScriptSemanticWorkerBoundary::new(script.display().to_string())
            .with_timeout(Duration::from_secs(2));

        let error = worker
            .analyze_project(SemanticWorkerRequest {
                project_root: workspace.path().display().to_string(),
                changed_files: Vec::new(),
            })
            .expect_err("facts for an empty request scope must fail");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
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
        let workspace = TempWorkspace::new("typescript-worker-invalid-request");
        for changed_file in [
            "/tmp/source.ts",
            "../secret.ts",
            "src/../secret.ts",
            "./src/a.ts",
            "src\\a.ts",
            "file:///tmp/source.ts",
            "C:tmp/source.ts",
            "C:/tmp/source.ts",
            "C:\\tmp\\source.ts",
        ] {
            let error = worker
                .analyze_project(SemanticWorkerRequest {
                    project_root: workspace.path().display().to_string(),
                    changed_files: vec![changed_file.to_string()],
                })
                .expect_err("unsafe changed file must be rejected before spawn");

            assert!(
                matches!(error, SemanticWorkerError::ProtocolViolation(_)),
                "unexpected error for {changed_file}: {error:?}"
            );
        }

        let error = TypeScriptSemanticWorkerBoundary::new("relative-worker")
            .analyze_project(SemanticWorkerRequest {
                project_root: workspace.path().display().to_string(),
                changed_files: Vec::new(),
            })
            .expect_err("relative worker executable must be rejected before spawn");
        assert!(matches!(error, SemanticWorkerError::Unavailable(_)));
    }

    #[test]
    fn boundary_request_limit_matches_worker_stdin_limit() {
        let exact = request_with_serialized_len("/repo", MAX_WORKER_STDIN_BYTES - 1);
        let exact_bytes = worker_request_bytes(exact).expect("exact stdin limit should serialize");
        assert_eq!(
            exact_bytes.len() + WORKER_REQUEST_TERMINATOR_BYTES,
            MAX_WORKER_STDIN_BYTES
        );

        let oversized = request_with_serialized_len("/repo", MAX_WORKER_STDIN_BYTES);
        let error = worker_request_bytes(oversized).expect_err("limit plus newline must fail");
        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
    }

    #[test]
    fn boundary_rejects_oversized_requests_before_spawn() {
        let workspace = TempWorkspace::new("typescript-worker-request-too-large");
        let worker = TypeScriptSemanticWorkerBoundary::new(
            workspace.path().join("unused-worker").display().to_string(),
        );
        let request = request_with_serialized_len(
            &workspace.path().display().to_string(),
            MAX_WORKER_STDIN_BYTES,
        );

        let error = worker
            .analyze_project(request)
            .expect_err("oversized request should be rejected");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
    }

    #[test]
    fn boundary_rejects_invalid_project_roots() {
        let workspace = TempWorkspace::new("typescript-worker-invalid-root");
        let worker = TypeScriptSemanticWorkerBoundary::new("/unused");
        for project_root in [
            ".".to_string(),
            workspace.path().join("missing").display().to_string(),
            format!("{}\0bad", workspace.path().display()),
        ] {
            let error = worker
                .analyze_project(SemanticWorkerRequest {
                    project_root,
                    changed_files: Vec::new(),
                })
                .expect_err("invalid project root must be rejected");
            assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn boundary_rejects_symlink_project_root() {
        let workspace = TempWorkspace::new("typescript-worker-symlink-root");
        let link = workspace.path().join("root-link");
        symlink(workspace.path(), &link).expect("create symlink root");

        let error = TypeScriptSemanticWorkerBoundary::new("/unused")
            .analyze_project(SemanticWorkerRequest {
                project_root: link.display().to_string(),
                changed_files: Vec::new(),
            })
            .expect_err("symlink project root must be rejected");

        assert!(matches!(error, SemanticWorkerError::ProtocolViolation(_)));
    }

    #[cfg(unix)]
    fn valid_request(workspace: &TempWorkspace) -> SemanticWorkerRequest {
        SemanticWorkerRequest {
            project_root: workspace.path().display().to_string(),
            changed_files: vec!["src/handlers/user.ts".to_string()],
        }
    }

    fn request_with_serialized_len(project_root: &str, target_len: usize) -> SemanticWorkerRequest {
        let mut changed_files = Vec::new();
        let mut next_index = 0usize;
        loop {
            let current_len = serialized_request_len(project_root, &changed_files);
            if current_len == target_len {
                return SemanticWorkerRequest {
                    project_root: project_root.to_string(),
                    changed_files,
                };
            }
            assert!(
                current_len < target_len,
                "request serialization overshot target length"
            );

            let empty_delta =
                serialized_request_len_with_extra_path(project_root, &changed_files, "")
                    - current_len;
            let min_path_len = fixed_width_path(next_index, 0).len();
            let min_delta = empty_delta + min_path_len;
            let max_delta = empty_delta + 4096;
            let remaining = target_len - current_len;

            if remaining >= min_delta && remaining <= max_delta {
                let path_len = remaining - empty_delta;
                changed_files.push(fixed_width_path(next_index, path_len - min_path_len));
                continue;
            }

            let delta = if remaining > max_delta && remaining - max_delta < min_delta {
                remaining - min_delta
            } else {
                max_delta
            };
            assert!(delta >= min_delta && delta <= max_delta);
            changed_files.push(fixed_width_path(
                next_index,
                delta - empty_delta - min_path_len,
            ));
            next_index += 1;
        }
    }

    fn serialized_request_len(project_root: &str, changed_files: &[String]) -> usize {
        let mut changed_files = changed_files.to_vec();
        changed_files.sort();
        changed_files.dedup();
        serde_json::to_vec(&json!({
            "protocol_version": SEMANTIC_WORKER_PROTOCOL_VERSION,
            "request_id": REQUEST_ID,
            "project_root": project_root,
            "changed_files": changed_files,
        }))
        .expect("request serialization should not fail")
        .len()
    }

    fn serialized_request_len_with_extra_path(
        project_root: &str,
        changed_files: &[String],
        path: &str,
    ) -> usize {
        let mut changed_files = changed_files.to_vec();
        changed_files.push(path.to_string());
        serialized_request_len(project_root, &changed_files)
    }

    fn fixed_width_path(index: usize, filler_len: usize) -> String {
        format!("src/{index:06}/{}.ts", "a".repeat(filler_len))
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

    fn requested_fact_paths() -> BTreeSet<String> {
        BTreeSet::from(["src/handlers/user.ts".to_string()])
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
