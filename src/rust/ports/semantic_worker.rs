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
    fn ndjson_fixtures_cover_fact_and_unsupported_version_messages() {
        let fact_fixture = include_str!("../../protocol/fixtures/typescript-semantic-fact.ndjson");
        let unsupported_fixture =
            include_str!("../../protocol/fixtures/typescript-unsupported-version.ndjson");

        assert!(fact_fixture.contains("\"message_type\":\"fact\""));
        assert!(fact_fixture.contains("\"certainty\":\"SEMANTIC\""));
        assert!(
            fact_fixture.contains("\"code_unit_id\":\"unit:src/handlers/user.ts#import:express\"")
        );
        assert!(fact_fixture.contains(
            "\"content_hash\":\"sha256:7c6e428e33561b59254d2efa13efac30fc391e9dc5d42f6c58132aaa8b2c8a03\""
        ));
        assert!(!fact_fixture.contains("sha256:fixture"));
        assert!(unsupported_fixture.contains(SEMANTIC_VERSION_UNSUPPORTED_CODE));
        assert!(unsupported_fixture.contains("\"mode\":\"syntax_only\""));
    }
}
