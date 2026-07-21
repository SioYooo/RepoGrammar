//! Zero-decision onboarding orchestration over existing application boundaries.
//!
//! This module owns ordering and recovery semantics only. Concrete runtimes
//! continue to own agent receipts/native writers, repository lifecycle,
//! indexing, autosync processes, and MCP self-tests.

use crate::application::install::{supported_concrete_targets, AgentTarget};
use crate::application::recovery::{
    classify_recovery, RecoveryAction, RecoveryAgentState, RecoveryAutosyncState, RecoveryContext,
    RecoveryEvidenceState, RecoveryFreshness, RecoveryHealth, RecoveryLockState,
    RecoveryRecommendation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupTarget {
    Auto,
    Codex,
    ClaudeCode,
}

impl SetupTarget {
    fn candidates(self) -> Vec<AgentTarget> {
        match self {
            Self::Auto => supported_concrete_targets(),
            Self::Codex => vec![AgentTarget::Codex],
            Self::ClaudeCode => vec![AgentTarget::ClaudeCode],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRequest {
    pub project: String,
    pub target: SetupTarget,
    pub dry_run: bool,
    pub autosync: bool,
    /// When true (default), setup writes RepoGrammar's marker-fenced pre-flight
    /// gate into each configured agent's global instruction file. `--no-instructions`
    /// registers the MCP server without that write.
    pub write_instructions: bool,
}

impl SetupRequest {
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            target: SetupTarget::Auto,
            dry_run: false,
            autosync: true,
            write_instructions: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAgentIntegrationState {
    Unmanaged,
    OwnedCurrent,
    OwnedOutdated,
    Foreign,
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupAgentState {
    pub target: AgentTarget,
    pub detected: bool,
    pub live_writer: bool,
    pub integration: SetupAgentIntegrationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupRepositoryState {
    pub initialized: bool,
    pub active_index: bool,
    pub freshness: RecoveryFreshness,
    pub autosync_configured: bool,
    pub autosync_running: bool,
    pub storage_health: RecoveryHealth,
    pub lock_state: RecoveryLockState,
    pub family_evidence: RecoveryEvidenceState,
}

impl Default for SetupRepositoryState {
    fn default() -> Self {
        Self {
            initialized: false,
            active_index: false,
            freshness: RecoveryFreshness::NotApplicable,
            autosync_configured: false,
            autosync_running: false,
            storage_health: RecoveryHealth::Unknown,
            lock_state: RecoveryLockState::Clear,
            family_evidence: RecoveryEvidenceState::NotApplicable,
        }
    }
}

pub trait SetupProbe {
    fn inspect_repository(
        &self,
        project: &str,
    ) -> Result<SetupRepositoryState, SetupOperationError>;

    fn inspect_agent(&self, target: AgentTarget) -> Result<SetupAgentState, SetupOperationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStage {
    Inspect,
    Confirm,
    AgentIntegration,
    RepositoryInitialization,
    RepositoryIndex,
    Autosync,
    McpSelfTest,
    RollbackMachineIntegration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupBoundary {
    ReadOnlyInspection,
    MachineAgentIntegration,
    RepositoryLocalState,
    BackgroundProcess,
    ProductSelfTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupDisposition {
    Execute,
    SkipAlreadyComplete,
    Disabled,
    Unavailable,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupAction {
    pub stage: SetupStage,
    pub boundary: SetupBoundary,
    pub disposition: SetupDisposition,
    pub targets: Vec<AgentTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupConfirmation {
    RequiredOnce,
    NotRequiredDryRun,
    NotRequiredNoMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupLimitation {
    AgentMissing(AgentTarget),
    NoLiveAgent,
    ForeignAgentConfiguration(AgentTarget),
    MalformedAgentConfiguration(AgentTarget),
    StorageUnhealthy,
    BlockingLock,
    NoPatternGroups,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPlan {
    request: SetupRequest,
    repository_before: SetupRepositoryState,
    agents: Vec<SetupAgentState>,
    actions: Vec<SetupAction>,
    limitations: Vec<SetupLimitation>,
    confirmation: SetupConfirmation,
}

impl SetupPlan {
    pub fn request(&self) -> &SetupRequest {
        &self.request
    }

    pub fn repository_before(&self) -> SetupRepositoryState {
        self.repository_before
    }

    pub fn agents(&self) -> &[SetupAgentState] {
        &self.agents
    }

    pub fn actions(&self) -> &[SetupAction] {
        &self.actions
    }

    pub fn limitations(&self) -> &[SetupLimitation] {
        &self.limitations
    }

    pub fn confirmation(&self) -> SetupConfirmation {
        self.confirmation
    }

    pub fn action(&self, stage: SetupStage) -> Option<&SetupAction> {
        self.actions.iter().find(|action| action.stage == stage)
    }

    pub fn has_mutations(&self) -> bool {
        self.actions.iter().any(|action| {
            action.disposition == SetupDisposition::Execute
                && matches!(
                    action.boundary,
                    SetupBoundary::MachineAgentIntegration
                        | SetupBoundary::RepositoryLocalState
                        | SetupBoundary::BackgroundProcess
                )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFailureClass {
    ProjectInspectionFailed,
    AgentDetectionFailed,
    AuthorizationRequired,
    NativeAgentConfigurationFailed,
    ReceiptWriteFailed,
    ForeignAgentConfiguration,
    MalformedAgentConfiguration,
    StorageUnhealthy,
    BlockingLock,
    RepositoryInitializationFailed,
    IndexFailed,
    AutosyncFailed,
    McpSelfTestTimedOut,
    McpSelfTestFailed,
    RollbackFailed,
    InvalidOperationResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupOperationError {
    pub class: SetupFailureClass,
}

impl SetupOperationError {
    pub fn new(class: SetupFailureClass) -> Self {
        Self { class }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupPlanningFailure {
    pub stage: SetupStage,
    pub class: SetupFailureClass,
}

pub fn plan_setup(
    request: SetupRequest,
    probe: &impl SetupProbe,
) -> Result<SetupPlan, SetupPlanningFailure> {
    let repository_before = probe
        .inspect_repository(&request.project)
        .map_err(|error| SetupPlanningFailure {
            stage: SetupStage::Inspect,
            class: match error.class {
                SetupFailureClass::ProjectInspectionFailed => error.class,
                _ => SetupFailureClass::ProjectInspectionFailed,
            },
        })?;

    let candidates = request.target.candidates();
    let mut agents = Vec::with_capacity(candidates.len());
    for target in candidates {
        let state = probe
            .inspect_agent(target)
            .map_err(|_| SetupPlanningFailure {
                stage: SetupStage::Inspect,
                class: SetupFailureClass::AgentDetectionFailed,
            })?;
        if state.target != target {
            return Err(SetupPlanningFailure {
                stage: SetupStage::Inspect,
                class: SetupFailureClass::AgentDetectionFailed,
            });
        }
        agents.push(state);
    }

    let (mut agent_action, mut limitations) = plan_agent_integration(request.target, &agents);
    let repository_blocked = if repository_before.storage_health == RecoveryHealth::Unhealthy {
        limitations.push(SetupLimitation::StorageUnhealthy);
        true
    } else if repository_before.initialized
        && matches!(
            repository_before.lock_state,
            RecoveryLockState::Blocking | RecoveryLockState::Unknown
        )
    {
        limitations.push(SetupLimitation::BlockingLock);
        true
    } else {
        false
    };
    if repository_before.active_index
        && repository_before.freshness == RecoveryFreshness::Fresh
        && repository_before.family_evidence == RecoveryEvidenceState::Unavailable
    {
        limitations.push(SetupLimitation::NoPatternGroups);
    }
    if repository_blocked && agent_action.disposition == SetupDisposition::Execute {
        agent_action.disposition = SetupDisposition::Blocked;
    }
    let repository_init_action = SetupAction {
        stage: SetupStage::RepositoryInitialization,
        boundary: SetupBoundary::RepositoryLocalState,
        disposition: if repository_blocked {
            SetupDisposition::Blocked
        } else if repository_before.initialized {
            SetupDisposition::SkipAlreadyComplete
        } else {
            SetupDisposition::Execute
        },
        targets: Vec::new(),
    };
    let repository_index_action = SetupAction {
        stage: SetupStage::RepositoryIndex,
        boundary: SetupBoundary::RepositoryLocalState,
        disposition: if repository_blocked {
            SetupDisposition::Blocked
        } else if repository_before.active_index
            && repository_before.freshness == RecoveryFreshness::Fresh
        {
            SetupDisposition::SkipAlreadyComplete
        } else {
            SetupDisposition::Execute
        },
        targets: Vec::new(),
    };
    let autosync_action = SetupAction {
        stage: SetupStage::Autosync,
        boundary: SetupBoundary::BackgroundProcess,
        disposition: if repository_blocked {
            SetupDisposition::Blocked
        } else if !request.autosync {
            SetupDisposition::Disabled
        } else if repository_before.autosync_running {
            SetupDisposition::SkipAlreadyComplete
        } else {
            SetupDisposition::Execute
        },
        targets: Vec::new(),
    };
    let self_test_targets = agents
        .iter()
        .filter(|agent| {
            agent.detected
                && agent.live_writer
                && matches!(
                    agent.integration,
                    SetupAgentIntegrationState::Unmanaged
                        | SetupAgentIntegrationState::OwnedCurrent
                        | SetupAgentIntegrationState::OwnedOutdated
                )
        })
        .map(|agent| agent.target)
        .collect::<Vec<_>>();
    let self_test_action = SetupAction {
        stage: SetupStage::McpSelfTest,
        boundary: SetupBoundary::ProductSelfTest,
        disposition: if repository_blocked {
            SetupDisposition::Blocked
        } else {
            SetupDisposition::Execute
        },
        targets: self_test_targets,
    };

    let actions = vec![
        SetupAction {
            stage: SetupStage::Inspect,
            boundary: SetupBoundary::ReadOnlyInspection,
            disposition: SetupDisposition::SkipAlreadyComplete,
            targets: Vec::new(),
        },
        agent_action,
        repository_init_action,
        repository_index_action,
        autosync_action,
        self_test_action,
    ];
    let has_mutations = actions.iter().any(|action| {
        action.disposition == SetupDisposition::Execute
            && matches!(
                action.boundary,
                SetupBoundary::MachineAgentIntegration
                    | SetupBoundary::RepositoryLocalState
                    | SetupBoundary::BackgroundProcess
            )
    });
    let confirmation = if request.dry_run {
        SetupConfirmation::NotRequiredDryRun
    } else if has_mutations {
        SetupConfirmation::RequiredOnce
    } else {
        SetupConfirmation::NotRequiredNoMutation
    };

    Ok(SetupPlan {
        request,
        repository_before,
        agents,
        actions,
        limitations,
        confirmation,
    })
}

fn plan_agent_integration(
    selection: SetupTarget,
    agents: &[SetupAgentState],
) -> (SetupAction, Vec<SetupLimitation>) {
    let mut execute_targets = Vec::new();
    let mut owned_targets = Vec::new();
    let mut limitations = Vec::new();
    for agent in agents {
        if !agent.detected || !agent.live_writer {
            if selection != SetupTarget::Auto {
                limitations.push(SetupLimitation::AgentMissing(agent.target));
            }
            continue;
        }
        match agent.integration {
            SetupAgentIntegrationState::Unmanaged | SetupAgentIntegrationState::OwnedOutdated => {
                execute_targets.push(agent.target);
            }
            SetupAgentIntegrationState::OwnedCurrent => owned_targets.push(agent.target),
            SetupAgentIntegrationState::Foreign => {
                limitations.push(SetupLimitation::ForeignAgentConfiguration(agent.target));
            }
            SetupAgentIntegrationState::Malformed => {
                limitations.push(SetupLimitation::MalformedAgentConfiguration(agent.target));
            }
        }
    }

    if selection == SetupTarget::Auto
        && execute_targets.is_empty()
        && owned_targets.is_empty()
        && limitations.is_empty()
    {
        limitations.push(SetupLimitation::NoLiveAgent);
    }

    let (disposition, targets) = if !execute_targets.is_empty() {
        (SetupDisposition::Execute, execute_targets)
    } else if !owned_targets.is_empty() {
        (SetupDisposition::SkipAlreadyComplete, owned_targets)
    } else if limitations.iter().any(|limitation| {
        matches!(
            limitation,
            SetupLimitation::ForeignAgentConfiguration(_)
                | SetupLimitation::MalformedAgentConfiguration(_)
        )
    }) {
        (SetupDisposition::Blocked, Vec::new())
    } else {
        (SetupDisposition::Unavailable, Vec::new())
    };

    (
        SetupAction {
            stage: SetupStage::AgentIntegration,
            boundary: SetupBoundary::MachineAgentIntegration,
            disposition,
            targets,
        },
        limitations,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAuthorization {
    Confirmed,
    NotConfirmed,
    DryRun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupAgentMutation {
    pub newly_configured: Vec<AgentTarget>,
    pub reconfigured: Vec<AgentTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupRepositoryMutation {
    pub created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFamilyInventory {
    Available(usize),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupIndexSummary {
    pub indexed_files: usize,
    pub family_inventory: SetupFamilyInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupAutosyncMutation {
    pub started: bool,
}

pub trait SetupExecutionPort {
    /// Delegate to the existing install application service. That service must
    /// retain receipt ownership, native configuration, and its all-or-rollback
    /// guarantee; setup only records the targets newly owned by this run.
    fn configure_agent_integrations(
        &self,
        targets: &[AgentTarget],
    ) -> Result<SetupAgentMutation, SetupOperationError>;

    fn initialize_repository(&self) -> Result<SetupRepositoryMutation, SetupOperationError>;

    fn index_repository(&self) -> Result<SetupIndexSummary, SetupOperationError>;

    fn start_autosync(&self) -> Result<SetupAutosyncMutation, SetupOperationError>;

    fn mcp_self_test(&self, targets: &[AgentTarget]) -> Result<(), SetupOperationError>;

    fn rollback_agent_integrations(
        &self,
        targets: &[AgentTarget],
    ) -> Result<(), SetupOperationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupOutcomeStatus {
    DryRun,
    Ready,
    ReadyWithLimitations,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStageStatus {
    Planned,
    Completed,
    Skipped,
    Disabled,
    Unavailable,
    Blocked,
    Failed,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupStageReport {
    pub stage: SetupStage,
    pub status: SetupStageStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupPreservedResource {
    PreExistingAgentIntegration(AgentTarget),
    PreExistingRepositoryState,
    ActiveGeneration,
    AutosyncProcess,
    RepositoryStateCreatedThisRun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRollbackReport {
    pub targets: Vec<AgentTarget>,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupFailure {
    pub stage: SetupStage,
    pub class: SetupFailureClass,
    pub rollback_failure: Option<SetupFailureClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupOutcome {
    pub status: SetupOutcomeStatus,
    pub stages: Vec<SetupStageReport>,
    pub limitations: Vec<SetupLimitation>,
    pub preserved: Vec<SetupPreservedResource>,
    pub rollback: Option<SetupRollbackReport>,
    pub index: Option<SetupIndexSummary>,
    pub failure: Option<SetupFailure>,
    pub recovery: RecoveryRecommendation,
}

pub fn execute_setup(
    plan: &SetupPlan,
    authorization: SetupAuthorization,
    operations: &impl SetupExecutionPort,
) -> SetupOutcome {
    let preserved = initial_preserved_resources(plan);
    if plan.request.dry_run {
        return dry_run_outcome(plan, preserved);
    }
    if plan.confirmation == SetupConfirmation::RequiredOnce
        && authorization != SetupAuthorization::Confirmed
    {
        return failure_outcome(
            plan,
            FailureOutcomeState {
                stages: Vec::new(),
                preserved,
                rollback: None,
                failure: SetupFailure {
                    stage: SetupStage::Confirm,
                    class: SetupFailureClass::AuthorizationRequired,
                    rollback_failure: None,
                },
                autosync_started: false,
            },
        );
    }

    let mut stages = vec![SetupStageReport {
        stage: SetupStage::Inspect,
        status: SetupStageStatus::Completed,
    }];
    if plan.confirmation == SetupConfirmation::RequiredOnce {
        stages.push(SetupStageReport {
            stage: SetupStage::Confirm,
            status: SetupStageStatus::Completed,
        });
    }
    if plan
        .limitations
        .contains(&SetupLimitation::StorageUnhealthy)
    {
        stages.push(report_for_action(required_action(
            plan,
            SetupStage::AgentIntegration,
        )));
        return failure_outcome(
            plan,
            FailureOutcomeState {
                stages,
                preserved,
                rollback: None,
                failure: SetupFailure {
                    stage: SetupStage::RepositoryInitialization,
                    class: SetupFailureClass::StorageUnhealthy,
                    rollback_failure: None,
                },
                autosync_started: false,
            },
        );
    }
    if plan.limitations.contains(&SetupLimitation::BlockingLock) {
        stages.push(report_for_action(required_action(
            plan,
            SetupStage::AgentIntegration,
        )));
        return failure_outcome(
            plan,
            FailureOutcomeState {
                stages,
                preserved,
                rollback: None,
                failure: SetupFailure {
                    stage: SetupStage::RepositoryIndex,
                    class: SetupFailureClass::BlockingLock,
                    rollback_failure: None,
                },
                autosync_started: false,
            },
        );
    }

    let mut newly_configured = Vec::new();
    let agent_action = required_action(plan, SetupStage::AgentIntegration);
    if agent_action.disposition == SetupDisposition::Execute {
        match operations.configure_agent_integrations(&agent_action.targets) {
            Ok(mutation) if complete_agent_mutation(&mutation, &agent_action.targets) => {
                newly_configured = mutation.newly_configured;
                stages.push(completed(SetupStage::AgentIntegration));
            }
            Ok(_) => {
                return rollback_and_fail(
                    plan,
                    operations,
                    stages,
                    preserved,
                    newly_configured,
                    SetupStage::AgentIntegration,
                    SetupFailureClass::InvalidOperationResult,
                );
            }
            Err(error) => {
                return failure_outcome(
                    plan,
                    FailureOutcomeState {
                        stages,
                        preserved,
                        rollback: None,
                        failure: SetupFailure {
                            stage: SetupStage::AgentIntegration,
                            class: error.class,
                            rollback_failure: None,
                        },
                        autosync_started: false,
                    },
                );
            }
        }
    } else {
        stages.push(report_for_action(agent_action));
    }

    let init_action = required_action(plan, SetupStage::RepositoryInitialization);
    let mut repository_created = false;
    if init_action.disposition == SetupDisposition::Execute {
        match operations.initialize_repository() {
            Ok(mutation) => {
                repository_created = mutation.created;
                stages.push(completed(SetupStage::RepositoryInitialization));
            }
            Err(error) => {
                return rollback_and_fail(
                    plan,
                    operations,
                    stages,
                    preserved,
                    newly_configured,
                    SetupStage::RepositoryInitialization,
                    error.class,
                );
            }
        }
    } else {
        stages.push(report_for_action(init_action));
    }

    let index_action = required_action(plan, SetupStage::RepositoryIndex);
    let mut index_summary = None;
    if index_action.disposition == SetupDisposition::Execute {
        match operations.index_repository() {
            Ok(summary) => {
                index_summary = Some(summary);
                stages.push(completed(SetupStage::RepositoryIndex));
            }
            Err(error) => {
                let mut preserved = preserved;
                if repository_created {
                    preserved.push(SetupPreservedResource::RepositoryStateCreatedThisRun);
                }
                return rollback_and_fail(
                    plan,
                    operations,
                    stages,
                    preserved,
                    newly_configured,
                    SetupStage::RepositoryIndex,
                    error.class,
                );
            }
        }
    } else {
        stages.push(report_for_action(index_action));
    }

    let autosync_action = required_action(plan, SetupStage::Autosync);
    let mut autosync_started = false;
    if autosync_action.disposition == SetupDisposition::Execute {
        match operations.start_autosync() {
            Ok(SetupAutosyncMutation { started: true }) => {
                autosync_started = true;
                stages.push(completed(SetupStage::Autosync));
            }
            Ok(SetupAutosyncMutation { started: false }) => {
                let mut preserved = preserved;
                preserve_once(&mut preserved, SetupPreservedResource::ActiveGeneration);
                return rollback_and_fail(
                    plan,
                    operations,
                    stages,
                    preserved,
                    newly_configured,
                    SetupStage::Autosync,
                    SetupFailureClass::AutosyncFailed,
                );
            }
            Err(error) => {
                let mut preserved = preserved;
                preserve_once(&mut preserved, SetupPreservedResource::ActiveGeneration);
                return rollback_and_fail(
                    plan,
                    operations,
                    stages,
                    preserved,
                    newly_configured,
                    SetupStage::Autosync,
                    error.class,
                );
            }
        }
    } else {
        stages.push(report_for_action(autosync_action));
    }

    let self_test_action = required_action(plan, SetupStage::McpSelfTest);
    if self_test_action.disposition == SetupDisposition::Execute {
        if let Err(error) = operations.mcp_self_test(&self_test_action.targets) {
            let mut preserved = preserved;
            preserve_once(&mut preserved, SetupPreservedResource::ActiveGeneration);
            if autosync_started || plan.repository_before.autosync_running {
                preserve_once(&mut preserved, SetupPreservedResource::AutosyncProcess);
            }
            return rollback_and_fail(
                plan,
                operations,
                stages,
                preserved,
                newly_configured,
                SetupStage::McpSelfTest,
                error.class,
            );
        }
        stages.push(completed(SetupStage::McpSelfTest));
    } else {
        stages.push(report_for_action(self_test_action));
    }

    let recovery = recovery_for_success(plan, index_summary, autosync_started);
    let mut limitations = plan.limitations.clone();
    if index_summary
        .is_some_and(|index| index.family_inventory == SetupFamilyInventory::Available(0))
    {
        limitations.push(SetupLimitation::NoPatternGroups);
    }
    SetupOutcome {
        status: if limitations.is_empty() && recovery.action == RecoveryAction::None {
            SetupOutcomeStatus::Ready
        } else {
            SetupOutcomeStatus::ReadyWithLimitations
        },
        stages,
        limitations,
        preserved,
        rollback: None,
        index: index_summary,
        failure: None,
        recovery,
    }
}

fn required_action(plan: &SetupPlan, stage: SetupStage) -> &SetupAction {
    plan.action(stage)
        .unwrap_or_else(|| panic!("setup plan is missing required stage {stage:?}"))
}

fn subset_of(subset: &[AgentTarget], superset: &[AgentTarget]) -> bool {
    subset.iter().all(|target| superset.contains(target))
}

fn complete_agent_mutation(mutation: &SetupAgentMutation, expected: &[AgentTarget]) -> bool {
    if !subset_of(&mutation.newly_configured, expected)
        || !subset_of(&mutation.reconfigured, expected)
        || mutation
            .newly_configured
            .iter()
            .any(|target| mutation.reconfigured.contains(target))
    {
        return false;
    }
    expected.iter().all(|target| {
        mutation.newly_configured.contains(target) || mutation.reconfigured.contains(target)
    })
}

fn completed(stage: SetupStage) -> SetupStageReport {
    SetupStageReport {
        stage,
        status: SetupStageStatus::Completed,
    }
}

fn report_for_action(action: &SetupAction) -> SetupStageReport {
    let status = match action.disposition {
        SetupDisposition::Execute => SetupStageStatus::Planned,
        SetupDisposition::SkipAlreadyComplete => SetupStageStatus::Skipped,
        SetupDisposition::Disabled => SetupStageStatus::Disabled,
        SetupDisposition::Unavailable => SetupStageStatus::Unavailable,
        SetupDisposition::Blocked => SetupStageStatus::Blocked,
    };
    SetupStageReport {
        stage: action.stage,
        status,
    }
}

fn dry_run_outcome(plan: &SetupPlan, preserved: Vec<SetupPreservedResource>) -> SetupOutcome {
    let stages = plan
        .actions
        .iter()
        .map(|action| {
            if action.disposition == SetupDisposition::Execute {
                SetupStageReport {
                    stage: action.stage,
                    status: SetupStageStatus::Planned,
                }
            } else {
                report_for_action(action)
            }
        })
        .collect();
    SetupOutcome {
        status: SetupOutcomeStatus::DryRun,
        stages,
        limitations: plan.limitations.clone(),
        preserved,
        rollback: None,
        index: None,
        failure: None,
        recovery: recovery_from_plan(plan, None, None, false, false),
    }
}

fn rollback_and_fail(
    plan: &SetupPlan,
    operations: &impl SetupExecutionPort,
    mut stages: Vec<SetupStageReport>,
    preserved: Vec<SetupPreservedResource>,
    newly_configured: Vec<AgentTarget>,
    failure_stage: SetupStage,
    failure_class: SetupFailureClass,
) -> SetupOutcome {
    let autosync_started = stages.iter().any(|stage| {
        stage.stage == SetupStage::Autosync && stage.status == SetupStageStatus::Completed
    });
    stages.push(SetupStageReport {
        stage: failure_stage,
        status: SetupStageStatus::Failed,
    });
    if newly_configured.is_empty() {
        return failure_outcome(
            plan,
            FailureOutcomeState {
                stages,
                preserved,
                rollback: None,
                failure: SetupFailure {
                    stage: failure_stage,
                    class: failure_class,
                    rollback_failure: None,
                },
                autosync_started,
            },
        );
    }
    match operations.rollback_agent_integrations(&newly_configured) {
        Ok(()) => {
            stages.push(SetupStageReport {
                stage: SetupStage::RollbackMachineIntegration,
                status: SetupStageStatus::RolledBack,
            });
            failure_outcome(
                plan,
                FailureOutcomeState {
                    stages,
                    preserved,
                    rollback: Some(SetupRollbackReport {
                        targets: newly_configured,
                        succeeded: true,
                    }),
                    failure: SetupFailure {
                        stage: failure_stage,
                        class: failure_class,
                        rollback_failure: None,
                    },
                    autosync_started,
                },
            )
        }
        Err(error) => {
            stages.push(SetupStageReport {
                stage: SetupStage::RollbackMachineIntegration,
                status: SetupStageStatus::RollbackFailed,
            });
            failure_outcome(
                plan,
                FailureOutcomeState {
                    stages,
                    preserved,
                    rollback: Some(SetupRollbackReport {
                        targets: newly_configured,
                        succeeded: false,
                    }),
                    failure: SetupFailure {
                        stage: failure_stage,
                        class: failure_class,
                        rollback_failure: Some(error.class),
                    },
                    autosync_started,
                },
            )
        }
    }
}

struct FailureOutcomeState {
    stages: Vec<SetupStageReport>,
    preserved: Vec<SetupPreservedResource>,
    rollback: Option<SetupRollbackReport>,
    failure: SetupFailure,
    autosync_started: bool,
}

fn failure_outcome(plan: &SetupPlan, mut state: FailureOutcomeState) -> SetupOutcome {
    if !state
        .stages
        .iter()
        .any(|report| report.stage == state.failure.stage)
    {
        state.stages.push(SetupStageReport {
            stage: state.failure.stage,
            status: SetupStageStatus::Failed,
        });
    }
    SetupOutcome {
        status: SetupOutcomeStatus::Failed,
        stages: state.stages,
        limitations: plan.limitations.clone(),
        preserved: state.preserved,
        rollback: state.rollback,
        index: None,
        failure: Some(state.failure),
        recovery: recovery_from_plan(
            plan,
            None,
            Some(state.failure),
            false,
            state.autosync_started,
        ),
    }
}

fn initial_preserved_resources(plan: &SetupPlan) -> Vec<SetupPreservedResource> {
    let mut preserved = plan
        .agents
        .iter()
        .filter(|agent| {
            matches!(
                agent.integration,
                SetupAgentIntegrationState::OwnedCurrent
                    | SetupAgentIntegrationState::OwnedOutdated
            )
        })
        .map(|agent| SetupPreservedResource::PreExistingAgentIntegration(agent.target))
        .collect::<Vec<_>>();
    if plan.repository_before.initialized {
        preserved.push(SetupPreservedResource::PreExistingRepositoryState);
    }
    if plan.repository_before.active_index {
        preserved.push(SetupPreservedResource::ActiveGeneration);
    }
    if plan.repository_before.autosync_running {
        preserved.push(SetupPreservedResource::AutosyncProcess);
    }
    preserved
}

fn preserve_once(preserved: &mut Vec<SetupPreservedResource>, resource: SetupPreservedResource) {
    if !preserved.contains(&resource) {
        preserved.push(resource);
    }
}

fn recovery_for_success(
    plan: &SetupPlan,
    index: Option<SetupIndexSummary>,
    autosync_started: bool,
) -> RecoveryRecommendation {
    recovery_from_plan(plan, index, None, true, autosync_started)
}

fn recovery_from_plan(
    plan: &SetupPlan,
    index: Option<SetupIndexSummary>,
    failure: Option<SetupFailure>,
    execution_completed: bool,
    autosync_started: bool,
) -> RecoveryRecommendation {
    let initialized = plan.repository_before.initialized
        || execution_completed
        || failure.is_some_and(|failure| {
            matches!(
                failure.stage,
                SetupStage::RepositoryIndex | SetupStage::Autosync | SetupStage::McpSelfTest
            )
        });
    let active_index = plan.repository_before.active_index
        || index.is_some()
        || failure.is_some_and(|failure| {
            matches!(
                failure.stage,
                SetupStage::Autosync | SetupStage::McpSelfTest
            )
        });
    let autosync_running = plan.repository_before.autosync_running || autosync_started;
    let family_evidence = match index.map(|summary| summary.family_inventory) {
        Some(SetupFamilyInventory::Available(count)) if count > 0 => {
            RecoveryEvidenceState::Available
        }
        Some(SetupFamilyInventory::Available(_)) => RecoveryEvidenceState::Unavailable,
        Some(SetupFamilyInventory::Unknown) => RecoveryEvidenceState::Unknown,
        None if plan.repository_before.active_index => plan.repository_before.family_evidence,
        None => RecoveryEvidenceState::NotApplicable,
    };
    let agent = recovery_agent_state(plan, failure, execution_completed);
    let context = RecoveryContext {
        initialized,
        storage_health: plan.repository_before.storage_health,
        lock_state: if execution_completed
            && !plan.limitations.contains(&SetupLimitation::BlockingLock)
        {
            RecoveryLockState::Clear
        } else {
            plan.repository_before.lock_state
        },
        active_index,
        freshness: if index.is_some() {
            RecoveryFreshness::Fresh
        } else if plan.repository_before.active_index {
            plan.repository_before.freshness
        } else {
            RecoveryFreshness::NotApplicable
        },
        family_evidence,
        autosync: RecoveryAutosyncState {
            configured: plan.repository_before.autosync_configured || autosync_started,
            running: autosync_running,
            recommended: plan.request.autosync && !autosync_running,
        },
        agent,
    };
    classify_recovery(&context)
}

fn recovery_agent_state(
    plan: &SetupPlan,
    failure: Option<SetupFailure>,
    execution_completed: bool,
) -> RecoveryAgentState {
    if let Some(failure) = failure {
        if matches!(
            failure.class,
            SetupFailureClass::McpSelfTestTimedOut | SetupFailureClass::McpSelfTestFailed
        ) {
            return RecoveryAgentState::SelfTestFailed;
        }
        if matches!(
            failure.class,
            SetupFailureClass::NativeAgentConfigurationFailed
                | SetupFailureClass::ReceiptWriteFailed
                | SetupFailureClass::RollbackFailed
                | SetupFailureClass::InvalidOperationResult
        ) {
            let target = plan
                .action(SetupStage::AgentIntegration)
                .and_then(|action| action.targets.first())
                .copied()
                .unwrap_or(AgentTarget::Codex);
            return RecoveryAgentState::ConfigurationFailed(target);
        }
    }
    if let Some(agent) = plan
        .limitations
        .iter()
        .find_map(|limitation| match limitation {
            SetupLimitation::AgentMissing(target) => Some(RecoveryAgentState::Missing(*target)),
            SetupLimitation::NoLiveAgent => Some(RecoveryAgentState::NoLiveAgent),
            SetupLimitation::ForeignAgentConfiguration(target) => {
                Some(RecoveryAgentState::Foreign(*target))
            }
            SetupLimitation::MalformedAgentConfiguration(target) => {
                Some(RecoveryAgentState::Malformed(*target))
            }
            SetupLimitation::StorageUnhealthy
            | SetupLimitation::BlockingLock
            | SetupLimitation::NoPatternGroups => None,
        })
    {
        return agent;
    }
    let preexisting_ready = plan.agents.iter().any(|agent| {
        agent.detected
            && agent.live_writer
            && agent.integration == SetupAgentIntegrationState::OwnedCurrent
    });
    let configured_ready = execution_completed
        && plan
            .action(SetupStage::AgentIntegration)
            .is_some_and(|action| {
                action.disposition == SetupDisposition::Execute && !action.targets.is_empty()
            });
    if preexisting_ready || configured_ready {
        RecoveryAgentState::Ready
    } else {
        RecoveryAgentState::NotRequired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct FakeProbe {
        repository: Result<SetupRepositoryState, SetupOperationError>,
        agents: Vec<SetupAgentState>,
        fail_agent_detection: bool,
    }

    impl SetupProbe for FakeProbe {
        fn inspect_repository(
            &self,
            _project: &str,
        ) -> Result<SetupRepositoryState, SetupOperationError> {
            self.repository
        }

        fn inspect_agent(
            &self,
            target: AgentTarget,
        ) -> Result<SetupAgentState, SetupOperationError> {
            if self.fail_agent_detection {
                return Err(SetupOperationError::new(
                    SetupFailureClass::AgentDetectionFailed,
                ));
            }
            Ok(self
                .agents
                .iter()
                .find(|agent| agent.target == target)
                .copied()
                .unwrap_or(SetupAgentState {
                    target,
                    detected: false,
                    live_writer: target
                        .has_live_writer(crate::application::install::InstallScope::Global),
                    integration: SetupAgentIntegrationState::Unmanaged,
                }))
        }
    }

    struct FakeOperations {
        calls: RefCell<Vec<SetupStage>>,
        fail: Cell<Option<(SetupStage, SetupFailureClass)>>,
        rollback_fails: Cell<bool>,
        autosync_started: Cell<bool>,
        configured: Vec<AgentTarget>,
        index: SetupIndexSummary,
    }

    impl FakeOperations {
        fn successful() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                fail: Cell::new(None),
                rollback_fails: Cell::new(false),
                autosync_started: Cell::new(true),
                configured: vec![AgentTarget::Codex],
                index: SetupIndexSummary {
                    indexed_files: 12,
                    family_inventory: SetupFamilyInventory::Available(3),
                },
            }
        }

        fn record(&self, stage: SetupStage) -> Result<(), SetupOperationError> {
            self.calls.borrow_mut().push(stage);
            if self
                .fail
                .get()
                .is_some_and(|(candidate, _)| candidate == stage)
            {
                return Err(SetupOperationError::new(
                    self.fail.get().expect("failure configured").1,
                ));
            }
            Ok(())
        }
    }

    impl SetupExecutionPort for FakeOperations {
        fn configure_agent_integrations(
            &self,
            _targets: &[AgentTarget],
        ) -> Result<SetupAgentMutation, SetupOperationError> {
            self.record(SetupStage::AgentIntegration)?;
            Ok(SetupAgentMutation {
                newly_configured: self.configured.clone(),
                reconfigured: Vec::new(),
            })
        }

        fn initialize_repository(&self) -> Result<SetupRepositoryMutation, SetupOperationError> {
            self.record(SetupStage::RepositoryInitialization)?;
            Ok(SetupRepositoryMutation { created: true })
        }

        fn index_repository(&self) -> Result<SetupIndexSummary, SetupOperationError> {
            self.record(SetupStage::RepositoryIndex)?;
            Ok(self.index)
        }

        fn start_autosync(&self) -> Result<SetupAutosyncMutation, SetupOperationError> {
            self.record(SetupStage::Autosync)?;
            Ok(SetupAutosyncMutation {
                started: self.autosync_started.get(),
            })
        }

        fn mcp_self_test(&self, _targets: &[AgentTarget]) -> Result<(), SetupOperationError> {
            self.record(SetupStage::McpSelfTest)
        }

        fn rollback_agent_integrations(
            &self,
            _targets: &[AgentTarget],
        ) -> Result<(), SetupOperationError> {
            self.calls
                .borrow_mut()
                .push(SetupStage::RollbackMachineIntegration);
            if self.rollback_fails.get() {
                return Err(SetupOperationError::new(SetupFailureClass::RollbackFailed));
            }
            Ok(())
        }
    }

    fn detected(target: AgentTarget, integration: SetupAgentIntegrationState) -> SetupAgentState {
        SetupAgentState {
            target,
            detected: true,
            live_writer: true,
            integration,
        }
    }

    fn clean_probe() -> FakeProbe {
        FakeProbe {
            repository: Ok(SetupRepositoryState {
                storage_health: RecoveryHealth::Healthy,
                ..SetupRepositoryState::default()
            }),
            agents: vec![
                detected(AgentTarget::Codex, SetupAgentIntegrationState::Unmanaged),
                SetupAgentState {
                    target: AgentTarget::ClaudeCode,
                    detected: false,
                    live_writer: true,
                    integration: SetupAgentIntegrationState::Unmanaged,
                },
            ],
            fail_agent_detection: false,
        }
    }

    #[test]
    fn clean_plan_has_one_confirmation_and_ordered_ownership_boundaries() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        assert_eq!(plan.confirmation, SetupConfirmation::RequiredOnce);
        assert_eq!(
            plan.actions
                .iter()
                .map(|action| action.stage)
                .collect::<Vec<_>>(),
            vec![
                SetupStage::Inspect,
                SetupStage::AgentIntegration,
                SetupStage::RepositoryInitialization,
                SetupStage::RepositoryIndex,
                SetupStage::Autosync,
                SetupStage::McpSelfTest,
            ]
        );
        assert_eq!(
            plan.action(SetupStage::AgentIntegration)
                .expect("agent action")
                .targets,
            vec![AgentTarget::Codex]
        );
    }

    #[test]
    fn clean_execution_follows_the_plan_and_becomes_ready() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::Ready);
        assert_eq!(outcome.failure, None);
        assert_eq!(outcome.recovery.action, RecoveryAction::None);
        assert_eq!(
            *operations.calls.borrow(),
            vec![
                SetupStage::AgentIntegration,
                SetupStage::RepositoryInitialization,
                SetupStage::RepositoryIndex,
                SetupStage::Autosync,
                SetupStage::McpSelfTest,
            ]
        );
    }

    #[test]
    fn dry_run_is_zero_mutation_even_when_every_action_is_planned() {
        let mut request = SetupRequest::new(".");
        request.dry_run = true;
        let plan = plan_setup(request, &clean_probe()).expect("plan");
        assert_eq!(plan.confirmation, SetupConfirmation::NotRequiredDryRun);
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::DryRun, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::DryRun);
        assert!(operations.calls.borrow().is_empty());
        assert!(outcome
            .stages
            .iter()
            .any(|stage| stage.stage == SetupStage::RepositoryIndex
                && stage.status == SetupStageStatus::Planned));
    }

    #[test]
    fn live_mutation_requires_the_single_confirmed_authorization() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::NotConfirmed, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
        assert_eq!(
            outcome.failure.expect("failure").class,
            SetupFailureClass::AuthorizationRequired
        );
        assert_eq!(outcome.recovery.action, RecoveryAction::Setup);
        assert!(operations.calls.borrow().is_empty());
    }

    #[test]
    fn native_and_receipt_failures_keep_their_typed_classification() {
        for class in [
            SetupFailureClass::NativeAgentConfigurationFailed,
            SetupFailureClass::ReceiptWriteFailed,
        ] {
            let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
            let operations = FakeOperations::successful();
            operations
                .fail
                .set(Some((SetupStage::AgentIntegration, class)));
            let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
            assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
            assert_eq!(outcome.failure.expect("failure").class, class);
            assert_eq!(outcome.recovery.action, RecoveryAction::Setup);
            assert!(!operations
                .calls
                .borrow()
                .contains(&SetupStage::RepositoryInitialization));
        }
    }

    #[test]
    fn rerun_preserves_existing_state_and_only_runs_the_self_test() {
        let probe = FakeProbe {
            repository: Ok(SetupRepositoryState {
                initialized: true,
                active_index: true,
                freshness: RecoveryFreshness::Fresh,
                autosync_configured: true,
                autosync_running: true,
                storage_health: RecoveryHealth::Healthy,
                lock_state: RecoveryLockState::Clear,
                family_evidence: RecoveryEvidenceState::Available,
            }),
            agents: vec![
                detected(AgentTarget::Codex, SetupAgentIntegrationState::OwnedCurrent),
                SetupAgentState {
                    target: AgentTarget::ClaudeCode,
                    detected: false,
                    live_writer: true,
                    integration: SetupAgentIntegrationState::Unmanaged,
                },
            ],
            fail_agent_detection: false,
        };
        let plan = plan_setup(SetupRequest::new("."), &probe).expect("plan");
        assert_eq!(plan.confirmation, SetupConfirmation::NotRequiredNoMutation);
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::NotConfirmed, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::Ready);
        assert_eq!(*operations.calls.borrow(), vec![SetupStage::McpSelfTest]);
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::PreExistingAgentIntegration(
                AgentTarget::Codex
            )));
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::PreExistingRepositoryState));
    }

    #[test]
    fn fresh_rerun_without_family_evidence_uses_source_fallback() {
        let probe = FakeProbe {
            repository: Ok(SetupRepositoryState {
                initialized: true,
                active_index: true,
                freshness: RecoveryFreshness::Fresh,
                autosync_configured: true,
                autosync_running: true,
                storage_health: RecoveryHealth::Healthy,
                lock_state: RecoveryLockState::Clear,
                family_evidence: RecoveryEvidenceState::Unavailable,
            }),
            agents: vec![detected(
                AgentTarget::Codex,
                SetupAgentIntegrationState::OwnedCurrent,
            )],
            fail_agent_detection: false,
        };
        let plan = plan_setup(SetupRequest::new("."), &probe).expect("plan");
        assert!(plan.limitations.contains(&SetupLimitation::NoPatternGroups));
        assert_eq!(
            plan.action(SetupStage::RepositoryIndex)
                .expect("index action")
                .disposition,
            SetupDisposition::SkipAlreadyComplete
        );

        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::NotConfirmed, &operations);

        assert_eq!(outcome.status, SetupOutcomeStatus::ReadyWithLimitations);
        assert_eq!(outcome.recovery.action, RecoveryAction::UseSourceFallback);
        assert_eq!(*operations.calls.borrow(), vec![SetupStage::McpSelfTest]);
    }

    #[test]
    fn rerun_refreshes_an_active_index_that_is_not_known_fresh() {
        for freshness in [RecoveryFreshness::Stale, RecoveryFreshness::CannotVerify] {
            let probe = FakeProbe {
                repository: Ok(SetupRepositoryState {
                    initialized: true,
                    active_index: true,
                    freshness,
                    autosync_configured: false,
                    autosync_running: false,
                    storage_health: RecoveryHealth::Healthy,
                    lock_state: RecoveryLockState::Clear,
                    family_evidence: RecoveryEvidenceState::Available,
                }),
                agents: vec![detected(
                    AgentTarget::Codex,
                    SetupAgentIntegrationState::OwnedCurrent,
                )],
                fail_agent_detection: false,
            };
            let mut request = SetupRequest::new(".");
            request.autosync = false;
            let plan = plan_setup(request, &probe).expect("plan");
            assert_eq!(
                plan.action(SetupStage::RepositoryIndex)
                    .expect("index action")
                    .disposition,
                SetupDisposition::Execute
            );

            let operations = FakeOperations::successful();
            let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
            assert_eq!(outcome.status, SetupOutcomeStatus::Ready);
            assert!(operations
                .calls
                .borrow()
                .contains(&SetupStage::RepositoryIndex));
            assert_eq!(outcome.recovery.action, RecoveryAction::None);
        }
    }

    #[test]
    fn missing_agent_does_not_block_repo_only_setup() {
        let mut request = SetupRequest::new(".");
        request.target = SetupTarget::Codex;
        let probe = FakeProbe {
            repository: clean_probe().repository,
            agents: Vec::new(),
            fail_agent_detection: false,
        };
        let plan = plan_setup(request, &probe).expect("plan");
        assert_eq!(
            plan.limitations,
            vec![SetupLimitation::AgentMissing(AgentTarget::Codex)]
        );
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::ReadyWithLimitations);
        assert_eq!(
            outcome.recovery.action,
            RecoveryAction::InstallAgent(AgentTarget::Codex)
        );
        assert!(!operations
            .calls
            .borrow()
            .contains(&SetupStage::AgentIntegration));
        assert!(operations.calls.borrow().contains(&SetupStage::McpSelfTest));
    }

    #[test]
    fn auto_without_live_agents_still_builds_repo_state() {
        let probe = FakeProbe {
            repository: clean_probe().repository,
            agents: Vec::new(),
            fail_agent_detection: false,
        };
        let plan = plan_setup(SetupRequest::new("."), &probe).expect("plan");
        assert_eq!(plan.limitations, vec![SetupLimitation::NoLiveAgent]);
        let operations = FakeOperations::successful();
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(
            outcome.recovery.action,
            RecoveryAction::InstallSupportedAgent
        );
        assert!(operations
            .calls
            .borrow()
            .contains(&SetupStage::RepositoryIndex));
        assert!(operations.calls.borrow().contains(&SetupStage::McpSelfTest));
    }

    #[test]
    fn foreign_and_malformed_integrations_are_never_mutated() {
        for integration in [
            SetupAgentIntegrationState::Foreign,
            SetupAgentIntegrationState::Malformed,
        ] {
            let mut request = SetupRequest::new(".");
            request.target = SetupTarget::Codex;
            let probe = FakeProbe {
                repository: clean_probe().repository,
                agents: vec![detected(AgentTarget::Codex, integration)],
                fail_agent_detection: false,
            };
            let plan = plan_setup(request, &probe).expect("plan");
            assert_eq!(
                plan.action(SetupStage::AgentIntegration)
                    .expect("agent action")
                    .disposition,
                SetupDisposition::Blocked
            );
            let operations = FakeOperations::successful();
            let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
            assert_eq!(outcome.status, SetupOutcomeStatus::ReadyWithLimitations);
            assert!(matches!(
                outcome.recovery.action,
                RecoveryAction::RepairAgentIntegration(AgentTarget::Codex)
            ));
            assert!(!operations
                .calls
                .borrow()
                .contains(&SetupStage::AgentIntegration));
        }
    }

    #[test]
    fn index_failure_rolls_back_only_new_machine_writes_and_keeps_repo_state() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        operations.fail.set(Some((
            SetupStage::RepositoryIndex,
            SetupFailureClass::IndexFailed,
        )));
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
        assert_eq!(
            outcome.failure.expect("failure").class,
            SetupFailureClass::IndexFailed
        );
        assert_eq!(
            outcome.rollback,
            Some(SetupRollbackReport {
                targets: vec![AgentTarget::Codex],
                succeeded: true,
            })
        );
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::RepositoryStateCreatedThisRun));
        assert!(!operations.calls.borrow().contains(&SetupStage::Autosync));
    }

    #[test]
    fn autosync_failure_preserves_active_generation_and_reports_one_recovery() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        operations.fail.set(Some((
            SetupStage::Autosync,
            SetupFailureClass::AutosyncFailed,
        )));
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::ActiveGeneration));
        assert_eq!(outcome.recovery.action, RecoveryAction::StartAutosync);
        assert!(!operations.calls.borrow().contains(&SetupStage::McpSelfTest));
    }

    #[test]
    fn autosync_false_result_is_a_failure_not_a_ready_claim() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        operations.autosync_started.set(false);

        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);

        assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
        assert_eq!(
            outcome.failure.expect("failure").class,
            SetupFailureClass::AutosyncFailed
        );
        assert_eq!(outcome.recovery.action, RecoveryAction::StartAutosync);
        assert!(!operations.calls.borrow().contains(&SetupStage::McpSelfTest));
    }

    #[test]
    fn zero_pattern_groups_is_ready_only_with_an_explicit_limitation() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let mut operations = FakeOperations::successful();
        operations.index.family_inventory = SetupFamilyInventory::Available(0);

        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);

        assert_eq!(outcome.status, SetupOutcomeStatus::ReadyWithLimitations);
        assert!(outcome
            .limitations
            .contains(&SetupLimitation::NoPatternGroups));
        assert_eq!(outcome.recovery.action, RecoveryAction::UseSourceFallback);
    }

    #[test]
    fn unknown_family_inventory_is_not_reported_as_zero_pattern_groups() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let mut operations = FakeOperations::successful();
        operations.index.family_inventory = SetupFamilyInventory::Unknown;

        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);

        assert!(!outcome
            .limitations
            .contains(&SetupLimitation::NoPatternGroups));
        assert_eq!(outcome.recovery.action, RecoveryAction::UseSourceFallback);
    }

    #[test]
    fn partial_agent_configuration_result_is_rejected() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let mut operations = FakeOperations::successful();
        operations.configured.clear();

        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);

        assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
        assert_eq!(
            outcome.failure.expect("failure").class,
            SetupFailureClass::InvalidOperationResult
        );
        assert!(!operations
            .calls
            .borrow()
            .contains(&SetupStage::RepositoryInitialization));
    }

    #[test]
    fn preexisting_agent_integration_is_never_rolled_back() {
        let probe = FakeProbe {
            repository: clean_probe().repository,
            agents: vec![
                detected(AgentTarget::Codex, SetupAgentIntegrationState::OwnedCurrent),
                SetupAgentState {
                    target: AgentTarget::ClaudeCode,
                    detected: false,
                    live_writer: true,
                    integration: SetupAgentIntegrationState::Unmanaged,
                },
            ],
            fail_agent_detection: false,
        };
        let plan = plan_setup(SetupRequest::new("."), &probe).expect("plan");
        let operations = FakeOperations::successful();
        operations.fail.set(Some((
            SetupStage::RepositoryIndex,
            SetupFailureClass::IndexFailed,
        )));
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(outcome.rollback, None);
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::PreExistingAgentIntegration(
                AgentTarget::Codex
            )));
        assert!(!operations
            .calls
            .borrow()
            .contains(&SetupStage::RollbackMachineIntegration));
    }

    #[test]
    fn rollback_failure_is_visible_without_replacing_the_primary_failure() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        operations.fail.set(Some((
            SetupStage::RepositoryIndex,
            SetupFailureClass::IndexFailed,
        )));
        operations.rollback_fails.set(true);
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        let failure = outcome.failure.expect("failure");
        assert_eq!(failure.class, SetupFailureClass::IndexFailed);
        assert_eq!(
            failure.rollback_failure,
            Some(SetupFailureClass::RollbackFailed)
        );
        assert!(!outcome.rollback.expect("rollback").succeeded);
    }

    #[test]
    fn mcp_timeout_keeps_index_and_autosync_but_rolls_back_new_agent_write() {
        let plan = plan_setup(SetupRequest::new("."), &clean_probe()).expect("plan");
        let operations = FakeOperations::successful();
        operations.fail.set(Some((
            SetupStage::McpSelfTest,
            SetupFailureClass::McpSelfTestTimedOut,
        )));
        let outcome = execute_setup(&plan, SetupAuthorization::Confirmed, &operations);
        assert_eq!(outcome.recovery.action, RecoveryAction::Setup);
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::ActiveGeneration));
        assert!(outcome
            .preserved
            .contains(&SetupPreservedResource::AutosyncProcess));
        assert!(outcome.rollback.expect("rollback").succeeded);
    }

    #[test]
    fn inspection_failures_stop_before_a_plan_exists() {
        let probe = FakeProbe {
            repository: Err(SetupOperationError::new(
                SetupFailureClass::ProjectInspectionFailed,
            )),
            agents: Vec::new(),
            fail_agent_detection: false,
        };
        assert_eq!(
            plan_setup(SetupRequest::new("."), &probe),
            Err(SetupPlanningFailure {
                stage: SetupStage::Inspect,
                class: SetupFailureClass::ProjectInspectionFailed,
            })
        );
    }

    #[test]
    fn probe_cannot_substitute_a_different_agent_target() {
        let mut request = SetupRequest::new(".");
        request.target = SetupTarget::Codex;
        let probe = FakeProbe {
            repository: clean_probe().repository,
            agents: vec![detected(
                AgentTarget::ClaudeCode,
                SetupAgentIntegrationState::Unmanaged,
            )],
            fail_agent_detection: false,
        };
        // The fake normally synthesizes a missing Codex result, so use a probe
        // dedicated to returning the mismatched record.
        struct MismatchedProbe(FakeProbe);
        impl SetupProbe for MismatchedProbe {
            fn inspect_repository(
                &self,
                project: &str,
            ) -> Result<SetupRepositoryState, SetupOperationError> {
                self.0.inspect_repository(project)
            }

            fn inspect_agent(
                &self,
                _target: AgentTarget,
            ) -> Result<SetupAgentState, SetupOperationError> {
                Ok(self.0.agents[0])
            }
        }
        assert_eq!(
            plan_setup(request, &MismatchedProbe(probe)),
            Err(SetupPlanningFailure {
                stage: SetupStage::Inspect,
                class: SetupFailureClass::AgentDetectionFailed,
            })
        );
    }

    #[test]
    fn unhealthy_storage_and_blocking_locks_prevent_repo_mutations() {
        let unsafe_states = [
            SetupRepositoryState {
                initialized: true,
                active_index: true,
                storage_health: RecoveryHealth::Unhealthy,
                lock_state: RecoveryLockState::Clear,
                ..SetupRepositoryState::default()
            },
            SetupRepositoryState {
                initialized: true,
                active_index: false,
                storage_health: RecoveryHealth::Healthy,
                lock_state: RecoveryLockState::Blocking,
                ..SetupRepositoryState::default()
            },
        ];
        for repository in unsafe_states {
            let probe = FakeProbe {
                repository: Ok(repository),
                agents: vec![detected(
                    AgentTarget::Codex,
                    SetupAgentIntegrationState::OwnedCurrent,
                )],
                fail_agent_detection: false,
            };
            let plan = plan_setup(SetupRequest::new("."), &probe).expect("plan");
            assert_eq!(
                plan.action(SetupStage::RepositoryIndex)
                    .expect("index action")
                    .disposition,
                SetupDisposition::Blocked
            );
            let operations = FakeOperations::successful();
            let outcome = execute_setup(&plan, SetupAuthorization::NotConfirmed, &operations);
            assert_eq!(outcome.status, SetupOutcomeStatus::Failed);
            assert!(matches!(
                outcome.failure.as_ref().map(|failure| failure.class),
                Some(SetupFailureClass::StorageUnhealthy | SetupFailureClass::BlockingLock)
            ));
            assert!(matches!(
                outcome.recovery.action,
                RecoveryAction::RepairStorage | RecoveryAction::ResolveLock
            ));
            assert!(!operations
                .calls
                .borrow()
                .contains(&SetupStage::RepositoryIndex));
            assert!(!operations.calls.borrow().contains(&SetupStage::Autosync));
        }
    }
}
