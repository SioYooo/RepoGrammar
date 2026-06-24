//! Repository-level initialization, indexing, status, and generation policy.

use crate::application::progress::{initialization_stages, ProgressStage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInitRequest {
    pub path: String,
    pub progress_json: bool,
    pub quiet: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexGenerationPolicy {
    pub build_new_generation: bool,
    pub atomically_activate_after_validation: bool,
    pub preserve_previous_valid_index_on_failure: bool,
}

impl Default for IndexGenerationPolicy {
    fn default() -> Self {
        Self {
            build_new_generation: true,
            atomically_activate_after_validation: true,
            preserve_previous_valid_index_on_failure: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryStatus {
    NotInitialized,
    Initialized { active_generation: String },
}

impl RepositoryStatus {
    pub fn as_human_message(&self) -> &'static str {
        match self {
            Self::NotInitialized => "RepoGrammar repository status: not initialized",
            Self::Initialized { .. } => "RepoGrammar repository status: initialized",
        }
    }
}

pub fn required_initialization_stages() -> Vec<ProgressStage> {
    initialization_stages()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_generation_policy_preserves_previous_valid_index() {
        let policy = IndexGenerationPolicy::default();

        assert!(policy.build_new_generation);
        assert!(policy.atomically_activate_after_validation);
        assert!(policy.preserve_previous_valid_index_on_failure);
    }

    #[test]
    fn status_can_represent_not_initialized_without_storage() {
        assert_eq!(
            RepositoryStatus::NotInitialized.as_human_message(),
            "RepoGrammar repository status: not initialized"
        );
    }
}
