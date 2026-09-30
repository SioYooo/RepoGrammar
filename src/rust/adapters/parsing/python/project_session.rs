//! Private, generation-owned Python transport. No analysis result is committed
//! until the caller observes a clean end-of-stream and successful child exit.

use super::*;
use crate::adapters::filesystem::discovery::sha256_hex;
use crate::adapters::parsing::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{Receiver, SyncSender};

const SESSION_REVISION: u64 = 1;
const MAX_SESSION_HEADER_BYTES: usize = 4096;

enum SessionReadError {
    Eof,
    Invalid,
}

pub(super) struct PythonProjectSession {
    child: Option<Child>,
    writer: Option<SyncSender<Vec<u8>>>,
    writes: Receiver<Result<(), ()>>,
    reader: Option<Receiver<Result<Vec<u8>, SessionReadError>>>,
    timeout: Duration,
    context_hash: String,
    session_id: String,
    original_context_bytes: usize,
    request_id: u64,
}

impl PythonProjectSession {
    #[cfg(test)]
    pub(super) fn set_test_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub(super) fn start(
        parser: &PythonAstParser,
        context: &ParserProjectContext,
    ) -> Result<Self, ParseError> {
        let mut context_text = python_project_context_payload(context).to_string();
        let original_context_bytes = context_text.len();
        // A context this large cannot fit any legacy parse-document envelope.
        // Preserve that exact per-file omission, without transferring it.
        if context_text.len().saturating_add(1) > MAX_PYTHON_FRONTEND_INPUT_BYTES {
            context_text =
                python_project_context_payload(&ParserProjectContext::default()).to_string();
        }
        let context_hash = format!("sha256:{}", sha256_hex(context_text.as_bytes()));
        let started = Instant::now();
        let mut child = Command::new(&parser.executable)
            .args(PYTHON_FRONTEND_ISOLATION_ARGS)
            .arg("-c")
            .arg(PYTHON_FRONTEND_BOOTSTRAP)
            .arg(&parser.worker_script)
            .arg("--project-session")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| session_error())?;
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            terminate_python_frontend(&mut child);
            return Err(session_error());
        };
        let (writer, requests) = mpsc::sync_channel::<Vec<u8>>(1);
        let (write_sender, writes) = mpsc::sync_channel(1);
        if thread::Builder::new()
            .name("repogrammar-python-session-stdin".into())
            .spawn(move || {
                while let Ok(bytes) = requests.recv() {
                    let result = stdin.write_all(&bytes).map_err(|_| ());
                    let failed = result.is_err();
                    if write_sender.send(result).is_err() || failed {
                        break;
                    }
                }
            })
            .is_err()
        {
            terminate_python_frontend(&mut child);
            return Err(session_error());
        }
        let (read_sender, reader) = mpsc::sync_channel(1);
        if thread::Builder::new()
            .name("repogrammar-python-session-stdout".into())
            .spawn(move || {
                let mut stdout = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    let result = Read::by_ref(&mut stdout)
                        .take((MAX_PYTHON_FRONTEND_OUTPUT_BYTES + 1) as u64)
                        .read_until(b'\n', &mut line);
                    if matches!(result, Ok(0)) {
                        let _ = read_sender.send(Err(SessionReadError::Eof));
                        break;
                    }
                    if !matches!(result, Ok(count) if count > 0)
                        || line.len() > MAX_PYTHON_FRONTEND_OUTPUT_BYTES
                        || line.last() != Some(&b'\n')
                    {
                        let _ = read_sender.send(Err(SessionReadError::Invalid));
                        break;
                    }
                    if read_sender.send(Ok(line)).is_err() {
                        break;
                    }
                }
            })
            .is_err()
        {
            terminate_python_frontend(&mut child);
            return Err(session_error());
        }
        let mut session = Self {
            child: Some(child),
            writer: Some(writer),
            writes,
            reader: Some(reader),
            timeout: parser.timeout.saturating_sub(started.elapsed()),
            session_id: format!("index:{}", &context_hash[7..]),
            context_hash,
            original_context_bytes,
            request_id: 0,
        };
        session.exchange("start", Some(&context_text), None, None)?;
        session.timeout = parser.timeout;
        Ok(session)
    }

    fn header(&self, kind: &str, content_hash: Option<&str>, use_context: Option<bool>) -> Value {
        let mut value = json!({
            "protocol_version": PYTHON_PARSE_DOCUMENT_PROTOCOL_VERSION,
            "contract_revision": PYTHON_PARSE_DOCUMENT_CONTRACT_REVISION,
            "project_session_revision": SESSION_REVISION,
            "session_id": self.session_id,
            "context_hash": self.context_hash,
            "request_id": self.request_id,
            "message_type": kind,
        });
        let fields = value.as_object_mut().expect("header object");
        if let Some(hash) = content_hash {
            fields.insert("content_hash".into(), json!(hash));
        }
        if let Some(use_context) = use_context {
            fields.insert("use_context".into(), json!(use_context));
        }
        value
    }

    fn exchange(
        &mut self,
        kind: &str,
        payload: Option<&str>,
        content_hash: Option<&str>,
        use_context: Option<bool>,
    ) -> Result<Option<String>, ParseError> {
        let result = self.exchange_inner(kind, payload, content_hash, use_context);
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn exchange_inner(
        &mut self,
        kind: &str,
        payload: Option<&str>,
        content_hash: Option<&str>,
        use_context: Option<bool>,
    ) -> Result<Option<String>, ParseError> {
        if self.child.is_none() {
            return Err(session_error());
        }
        let deadline = Instant::now() + self.timeout;
        let header = self.header(kind, content_hash, use_context);
        let mut bytes = header.to_string().into_bytes();
        bytes.push(b'\n');
        if bytes.len() > MAX_SESSION_HEADER_BYTES {
            return Err(session_error());
        }
        if let Some(payload) = payload {
            if payload.len().saturating_add(1) > MAX_PYTHON_FRONTEND_INPUT_BYTES {
                return Err(session_error());
            }
            bytes.extend_from_slice(payload.as_bytes());
            bytes.push(b'\n');
        }
        self.writer
            .as_ref()
            .ok_or_else(session_error)?
            .send(bytes)
            .map_err(|_| session_error())?;
        // Read admission/rejection before awaiting the write acknowledgment:
        // unsupported runtimes can close stdin during a large first request.
        let response = self.read_line(deadline)?;
        if response.len() > MAX_SESSION_HEADER_BYTES {
            return Err(session_error());
        }
        let response_text = std::str::from_utf8(&response)
            .map_err(|_| ParseError::PythonFrontendContractMismatch)?;
        if has_duplicate_or_excess_members(
            response_text,
            BoundedJsonLimits {
                max_depth: 1,
                max_members: 9,
                max_key_bytes: 32,
            },
        ) != Ok(false)
        {
            return Err(ParseError::PythonFrontendContractMismatch);
        }
        let value: Value = serde_json::from_str(response_text)
            .map_err(|_| ParseError::PythonFrontendContractMismatch)?;
        let expected_kind = match kind {
            "start" => "ready",
            "parse" => "result",
            "finish" => "end_of_stream",
            _ => return Err(session_error()),
        };
        if value != self.header(expected_kind, content_hash, use_context) {
            return Err(ParseError::PythonFrontendContractMismatch);
        }
        self.writes
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| ParseError::Timeout)?
            .map_err(|_| session_error())?;
        if kind == "parse" {
            let result = self.read_line(deadline)?;
            return String::from_utf8(result)
                .map(Some)
                .map_err(|_| session_error());
        }
        Ok(None)
    }

    fn read_line(&mut self, deadline: Instant) -> Result<Vec<u8>, ParseError> {
        let line = self
            .reader
            .as_ref()
            .ok_or_else(session_error)?
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| ParseError::Timeout)?
            .map_err(|_| session_error())?;
        if line == PYTHON_FRONTEND_UNSUPPORTED_RUNTIME_MARKER {
            let child = self.child.as_mut().ok_or_else(session_error)?;
            if wait_for_python_frontend(child, deadline)?.code()
                == Some(PYTHON_FRONTEND_UNSUPPORTED_RUNTIME_EXIT)
            {
                return Err(ParseError::PythonFrontendInterpreterUnsupported);
            }
            return Err(session_error());
        }
        Ok(line)
    }

    fn abort(&mut self) {
        if let Some(mut child) = self.child.take() {
            terminate_python_frontend(&mut child);
        }
        self.writer.take();
        self.reader.take();
    }
}

impl ParserProjectSession for PythonProjectSession {
    fn parse(&mut self, document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
        if document.language != Language::Python {
            return Err(ParseError::UnsupportedLanguage);
        }
        self.request_id = self.request_id.checked_add(1).ok_or_else(session_error)?;
        let payload = parse_document_payload(&document, None).to_string();
        // Merging two nonempty JSON objects adds exactly context.len()-1 bytes.
        // Match serialize_parse_request's original length decision, independently
        // from the new transport's compact request size.
        let context_omitted = payload
            .len()
            .saturating_add(self.original_context_bytes)
            .saturating_sub(1)
            > MAX_PYTHON_FRONTEND_INPUT_BYTES;
        let response = self
            .exchange(
                "parse",
                Some(&payload),
                Some(document.content_hash.as_str()),
                Some(!context_omitted),
            )?
            .ok_or_else(session_error)?;
        let output = python_document_output(&document, &response, context_omitted);
        if output.is_err() {
            self.abort();
        }
        output
    }

    fn finish(&mut self) -> Result<(), ParseError> {
        let deadline = Instant::now() + self.timeout;
        self.request_id = self.request_id.checked_add(1).ok_or_else(session_error)?;
        self.exchange("finish", None, None, None)?;
        self.writer.take(); // EOF is part of the deterministic shutdown contract.
        let result = match self.child.as_mut() {
            Some(child) => wait_for_python_frontend(child, deadline),
            None => return Err(session_error()),
        };
        match result {
            Ok(status) if status.success() => {
                // The worker must close stdout immediately after EOS, never
                // append untrusted extra frames after the apparent completion.
                let eof = self
                    .reader
                    .as_ref()
                    .ok_or_else(session_error)?
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| ParseError::Timeout)?;
                if !matches!(eof, Err(SessionReadError::Eof)) {
                    self.abort();
                    return Err(ParseError::PythonFrontendContractMismatch);
                }
                self.child.take();
                self.reader.take();
                Ok(())
            }
            Ok(_) => {
                self.abort();
                Err(session_error())
            }
            Err(error) => {
                self.abort();
                Err(error)
            }
        }
    }
}

impl Drop for PythonProjectSession {
    fn drop(&mut self) {
        self.abort();
    }
}

fn session_error() -> ParseError {
    ParseError::Internal("python project frontend session failed".into())
}
