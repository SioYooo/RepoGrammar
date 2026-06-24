//! TypeScript semantic worker boundary.
//!
//! The bootstrap records that TypeScript semantic facts should come from a
//! versioned worker using the official TypeScript compiler or language-service
//! API when available. No worker process is launched yet.

use crate::core::model::SemanticFact;
use crate::ports::semantic_worker::{SemanticWorker, SemanticWorkerError, SemanticWorkerRequest};

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
