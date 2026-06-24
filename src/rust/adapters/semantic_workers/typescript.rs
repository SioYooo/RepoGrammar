//! TypeScript semantic worker boundary.
//!
//! The bootstrap records that TypeScript semantic facts should come from a
//! versioned worker using the official TypeScript compiler or language-service
//! API when available. No worker process is launched yet.

use crate::core::model::SemanticFact;
use crate::ports::semantic_worker::{
    SemanticWorker, SemanticWorkerError, SemanticWorkerRequest, SEMANTIC_VERSION_UNSUPPORTED_CODE,
};

pub const PINNED_TYPESCRIPT_MAJOR_VERSION: u16 = 6;

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
    pub command: String,
}

impl SemanticWorker for TypeScriptSemanticWorkerBoundary {
    fn analyze_project(
        &self,
        _request: SemanticWorkerRequest,
    ) -> Result<Vec<SemanticFact>, SemanticWorkerError> {
        Err(SemanticWorkerError::Unavailable(
            "TypeScript semantic worker protocol is defined but not implemented".to_string(),
        ))
    }
}

fn parse_major_version(version: &str) -> Option<u16> {
    version.split('.').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_six_uses_supported_compiler_api_boundary() {
        assert_eq!(
            classify_typescript_version("6.0.0"),
            TypeScriptVersionSupport::SupportedCompilerApi { major: 6 }
        );
    }

    #[test]
    fn unsupported_typescript_versions_fall_back_to_syntax_only() {
        assert_eq!(
            classify_typescript_version("7.0.0-dev"),
            TypeScriptVersionSupport::SyntaxOnlyFallback {
                reason_code: SEMANTIC_VERSION_UNSUPPORTED_CODE
            }
        );
        assert_eq!(
            classify_typescript_version("not-a-version"),
            TypeScriptVersionSupport::SyntaxOnlyFallback {
                reason_code: SEMANTIC_VERSION_UNSUPPORTED_CODE
            }
        );
    }
}
