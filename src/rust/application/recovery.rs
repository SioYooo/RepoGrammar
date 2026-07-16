//! Authoritative recovery-action classification shared by product surfaces.

use crate::application::install::AgentTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    Setup,
    Resync,
    StartAutosync,
    UseSourceFallback,
    Unsupported,
    RepairStorage,
    ResolveLock,
    InstallAgent(AgentTarget),
    InstallSupportedAgent,
    RepairAgentIntegration(AgentTarget),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryReason {
    RepositoryNotInitialized,
    StorageUnhealthy,
    BlockingLock,
    ActiveIndexMissing,
    EvidenceStale,
    EvidenceCannotBeVerified,
    TargetUnsupported,
    FamilyEvidenceUnavailable,
    AutosyncRecommended,
    AgentMissing,
    NoLiveAgent,
    AgentIntegrationForeign,
    AgentIntegrationMalformed,
    AgentIntegrationFailed,
    AgentSelfTestFailed,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryRecommendation {
    pub action: RecoveryAction,
    pub reason: RecoveryReason,
}

/// Return the single command users should run for a recovery action.
///
/// Presentation layers may add context, but they must not substitute a
/// different lifecycle command for the same action.
pub fn recovery_command(action: RecoveryAction) -> &'static str {
    match action {
        RecoveryAction::Setup => "repogrammar setup",
        RecoveryAction::Resync => "repogrammar resync",
        RecoveryAction::StartAutosync => "repogrammar autosync start",
        RecoveryAction::RepairStorage | RecoveryAction::ResolveLock => "repogrammar doctor",
        RecoveryAction::InstallAgent(AgentTarget::Codex) => {
            "install Codex, then run repogrammar setup"
        }
        RecoveryAction::InstallAgent(AgentTarget::ClaudeCode) => {
            "install Claude Code, then run repogrammar setup"
        }
        RecoveryAction::InstallAgent(_) | RecoveryAction::InstallSupportedAgent => {
            "install a supported coding agent, then run repogrammar setup"
        }
        RecoveryAction::RepairAgentIntegration(_) => "repogrammar doctor",
        RecoveryAction::UseSourceFallback => "use normal source search",
        RecoveryAction::Unsupported => "use normal source search; this target is unsupported",
        RecoveryAction::None => "ask your coding agent the suggested question",
    }
}

/// Return the canonical sentence-ready guidance for a recovery action.
pub fn recovery_guidance(action: RecoveryAction) -> &'static str {
    match action {
        RecoveryAction::Setup => "run repogrammar setup",
        RecoveryAction::Resync => "run repogrammar resync",
        RecoveryAction::StartAutosync => "run repogrammar autosync start",
        RecoveryAction::RepairStorage | RecoveryAction::ResolveLock => "run repogrammar doctor",
        RecoveryAction::InstallAgent(AgentTarget::Codex) => {
            "install Codex, then run repogrammar setup"
        }
        RecoveryAction::InstallAgent(AgentTarget::ClaudeCode) => {
            "install Claude Code, then run repogrammar setup"
        }
        RecoveryAction::InstallAgent(_) | RecoveryAction::InstallSupportedAgent => {
            "install a supported coding agent, then run repogrammar setup"
        }
        RecoveryAction::RepairAgentIntegration(_) => "run repogrammar doctor",
        RecoveryAction::UseSourceFallback => "use source fallback",
        RecoveryAction::Unsupported => "use source fallback; the target is unsupported",
        RecoveryAction::None => "no recovery action is required",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryHealth {
    Healthy,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryLockState {
    Clear,
    Blocking,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryFreshness {
    Fresh,
    Stale,
    CannotVerify,
    Unsupported,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryEvidenceState {
    Available,
    Unavailable,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryAutosyncState {
    pub configured: bool,
    pub running: bool,
    pub recommended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAgentState {
    NotRequired,
    Ready,
    Missing(AgentTarget),
    NoLiveAgent,
    Foreign(AgentTarget),
    Malformed(AgentTarget),
    ConfigurationFailed(AgentTarget),
    SelfTestFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryContext {
    pub initialized: bool,
    pub storage_health: RecoveryHealth,
    pub lock_state: RecoveryLockState,
    pub active_index: bool,
    pub freshness: RecoveryFreshness,
    pub family_evidence: RecoveryEvidenceState,
    pub autosync: RecoveryAutosyncState,
    pub agent: RecoveryAgentState,
}

impl Default for RecoveryContext {
    fn default() -> Self {
        Self {
            initialized: false,
            storage_health: RecoveryHealth::Unknown,
            lock_state: RecoveryLockState::Unknown,
            active_index: false,
            freshness: RecoveryFreshness::NotApplicable,
            family_evidence: RecoveryEvidenceState::NotApplicable,
            autosync: RecoveryAutosyncState {
                configured: false,
                running: false,
                recommended: false,
            },
            agent: RecoveryAgentState::NotRequired,
        }
    }
}

/// Choose one recovery action from transport-neutral facts.
///
/// The order is intentional: destructive or unsafe repository conditions take
/// precedence over convenience actions, and target-level evidence limitations
/// never get upgraded into a claim that re-indexing will necessarily resolve.
pub fn classify_recovery(context: &RecoveryContext) -> RecoveryRecommendation {
    if context.storage_health == RecoveryHealth::Unhealthy {
        return recommendation(
            RecoveryAction::RepairStorage,
            RecoveryReason::StorageUnhealthy,
        );
    }
    if matches!(
        context.lock_state,
        RecoveryLockState::Blocking | RecoveryLockState::Unknown
    ) && context.initialized
    {
        return recommendation(RecoveryAction::ResolveLock, RecoveryReason::BlockingLock);
    }
    if !context.initialized {
        return recommendation(
            RecoveryAction::Setup,
            RecoveryReason::RepositoryNotInitialized,
        );
    }
    if !context.active_index {
        return recommendation(RecoveryAction::Resync, RecoveryReason::ActiveIndexMissing);
    }

    match context.freshness {
        RecoveryFreshness::Stale => {
            if context.autosync.configured && !context.autosync.running {
                return recommendation(
                    RecoveryAction::StartAutosync,
                    RecoveryReason::EvidenceStale,
                );
            }
            return recommendation(RecoveryAction::Resync, RecoveryReason::EvidenceStale);
        }
        RecoveryFreshness::CannotVerify => {
            return recommendation(
                RecoveryAction::UseSourceFallback,
                RecoveryReason::EvidenceCannotBeVerified,
            );
        }
        RecoveryFreshness::Unsupported => {
            return recommendation(
                RecoveryAction::Unsupported,
                RecoveryReason::TargetUnsupported,
            );
        }
        RecoveryFreshness::Fresh | RecoveryFreshness::NotApplicable => {}
    }

    if let RecoveryAgentState::Malformed(target) = context.agent {
        return recommendation(
            RecoveryAction::RepairAgentIntegration(target),
            RecoveryReason::AgentIntegrationMalformed,
        );
    }

    if matches!(
        context.family_evidence,
        RecoveryEvidenceState::Unavailable | RecoveryEvidenceState::Unknown
    ) {
        return recommendation(
            RecoveryAction::UseSourceFallback,
            RecoveryReason::FamilyEvidenceUnavailable,
        );
    }

    match context.agent {
        RecoveryAgentState::Missing(target) => {
            return recommendation(
                RecoveryAction::InstallAgent(target),
                RecoveryReason::AgentMissing,
            );
        }
        RecoveryAgentState::NoLiveAgent => {
            return recommendation(
                RecoveryAction::InstallSupportedAgent,
                RecoveryReason::NoLiveAgent,
            );
        }
        RecoveryAgentState::Foreign(target) => {
            return recommendation(
                RecoveryAction::RepairAgentIntegration(target),
                RecoveryReason::AgentIntegrationForeign,
            );
        }
        RecoveryAgentState::Malformed(target) => {
            return recommendation(
                RecoveryAction::RepairAgentIntegration(target),
                RecoveryReason::AgentIntegrationMalformed,
            );
        }
        RecoveryAgentState::ConfigurationFailed(target) => {
            return recommendation(
                RecoveryAction::RepairAgentIntegration(target),
                RecoveryReason::AgentIntegrationFailed,
            );
        }
        RecoveryAgentState::SelfTestFailed => {
            return recommendation(RecoveryAction::Setup, RecoveryReason::AgentSelfTestFailed);
        }
        RecoveryAgentState::NotRequired | RecoveryAgentState::Ready => {}
    }

    if context.autosync.recommended && !context.autosync.running {
        return recommendation(
            RecoveryAction::StartAutosync,
            RecoveryReason::AutosyncRecommended,
        );
    }

    recommendation(RecoveryAction::None, RecoveryReason::Ready)
}

fn recommendation(action: RecoveryAction, reason: RecoveryReason) -> RecoveryRecommendation {
    RecoveryRecommendation { action, reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_context() -> RecoveryContext {
        RecoveryContext {
            initialized: true,
            storage_health: RecoveryHealth::Healthy,
            lock_state: RecoveryLockState::Clear,
            active_index: true,
            freshness: RecoveryFreshness::Fresh,
            family_evidence: RecoveryEvidenceState::Available,
            autosync: RecoveryAutosyncState {
                configured: true,
                running: true,
                recommended: false,
            },
            agent: RecoveryAgentState::Ready,
        }
    }

    #[test]
    fn unsafe_repository_conditions_take_priority() {
        let mut context = ready_context();
        context.storage_health = RecoveryHealth::Unhealthy;
        context.lock_state = RecoveryLockState::Blocking;
        context.freshness = RecoveryFreshness::Stale;
        assert_eq!(
            classify_recovery(&context),
            recommendation(
                RecoveryAction::RepairStorage,
                RecoveryReason::StorageUnhealthy
            )
        );

        context.storage_health = RecoveryHealth::Healthy;
        assert_eq!(
            classify_recovery(&context),
            recommendation(RecoveryAction::ResolveLock, RecoveryReason::BlockingLock)
        );
    }

    #[test]
    fn initialization_and_active_index_have_one_deterministic_action() {
        let context = RecoveryContext::default();
        assert_eq!(classify_recovery(&context).action, RecoveryAction::Setup);

        let mut initialized = ready_context();
        initialized.active_index = false;
        assert_eq!(
            classify_recovery(&initialized).action,
            RecoveryAction::Resync
        );
    }

    #[test]
    fn stale_evidence_never_recommends_two_commands() {
        let mut context = ready_context();
        context.freshness = RecoveryFreshness::Stale;
        context.autosync.running = false;
        assert_eq!(
            classify_recovery(&context).action,
            RecoveryAction::StartAutosync
        );

        context.autosync.configured = false;
        assert_eq!(classify_recovery(&context).action, RecoveryAction::Resync);
    }

    #[test]
    fn unknown_and_unsupported_evidence_are_not_overclaimed() {
        let mut context = ready_context();
        context.freshness = RecoveryFreshness::CannotVerify;
        assert_eq!(
            classify_recovery(&context).action,
            RecoveryAction::UseSourceFallback
        );

        context.freshness = RecoveryFreshness::Unsupported;
        assert_eq!(
            classify_recovery(&context).action,
            RecoveryAction::Unsupported
        );
    }

    #[test]
    fn agent_recovery_is_typed_and_lower_priority_than_repository_safety() {
        let mut context = ready_context();
        context.agent = RecoveryAgentState::Malformed(AgentTarget::Codex);
        assert_eq!(
            classify_recovery(&context).action,
            RecoveryAction::RepairAgentIntegration(AgentTarget::Codex)
        );

        context.storage_health = RecoveryHealth::Unhealthy;
        assert_eq!(
            classify_recovery(&context).action,
            RecoveryAction::RepairStorage
        );
    }

    #[test]
    fn ready_context_requires_no_recovery() {
        assert_eq!(
            classify_recovery(&ready_context()),
            recommendation(RecoveryAction::None, RecoveryReason::Ready)
        );
    }

    #[test]
    fn recovery_rendering_has_one_setup_and_refresh_path() {
        assert_eq!(recovery_command(RecoveryAction::Setup), "repogrammar setup");
        assert_eq!(
            recovery_guidance(RecoveryAction::Setup),
            "run repogrammar setup"
        );
        assert_eq!(
            recovery_command(RecoveryAction::Resync),
            "repogrammar resync"
        );
        assert_eq!(
            recovery_command(RecoveryAction::StartAutosync),
            "repogrammar autosync start"
        );
    }
}
