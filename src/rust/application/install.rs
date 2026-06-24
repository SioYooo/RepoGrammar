//! Safe machine-level agent integration planning.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTarget {
    AllSupported,
    Codex,
    ClaudeCode,
}

impl AgentTarget {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "all" => Ok(Self::AllSupported),
            "codex" => Ok(Self::Codex),
            "claude-code" | "claude" => Ok(Self::ClaudeCode),
            _ => Err(format!(
                "unsupported target {value}; expected all, codex, or claude-code"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllSupported => "all",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    Global,
    ProjectLocal,
}

impl InstallScope {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "global" => Ok(Self::Global),
            "project-local" | "project" | "local" => Ok(Self::ProjectLocal),
            _ => Err(format!(
                "unsupported scope {value}; expected global or project-local"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::ProjectLocal => "project-local",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub target: AgentTarget,
    pub scope: InstallScope,
    pub dry_run: bool,
    pub print_config: bool,
    pub assume_yes: bool,
    pub no_permissions: bool,
    pub telemetry_enabled: bool,
}

impl Default for InstallRequest {
    fn default() -> Self {
        Self {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            dry_run: false,
            print_config: false,
            assume_yes: false,
            no_permissions: false,
            telemetry_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub target: AgentTarget,
    pub scope: InstallScope,
    pub telemetry_enabled: bool,
    pub actions: Vec<InstallAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallAction {
    DetectSupportedAgents,
    PreferNativeAgentConfiguration,
    PreserveUnknownConfigurationFields,
    RefuseMalformedConfigurationByDefault,
    BackupBeforeApprovedRepair,
    AtomicWriteAndReparse,
    InstallExecutableInUserWritableDirectory,
    StoreAbsoluteExecutablePath,
    ValidateMcpSelfTest,
    StoreReversibleReceipt,
    RemoveOnlyRepoGrammarManagedConfiguration,
    OptionalMarkerFencedInstructionUpdate,
    DoNotImposeRepoMirrorPolicy,
}

pub fn plan_install(request: &InstallRequest) -> InstallPlan {
    InstallPlan {
        target: request.target,
        scope: request.scope,
        telemetry_enabled: request.telemetry_enabled,
        actions: vec![
            InstallAction::DetectSupportedAgents,
            InstallAction::PreferNativeAgentConfiguration,
            InstallAction::PreserveUnknownConfigurationFields,
            InstallAction::RefuseMalformedConfigurationByDefault,
            InstallAction::BackupBeforeApprovedRepair,
            InstallAction::AtomicWriteAndReparse,
            InstallAction::InstallExecutableInUserWritableDirectory,
            InstallAction::StoreAbsoluteExecutablePath,
            InstallAction::ValidateMcpSelfTest,
            InstallAction::StoreReversibleReceipt,
            InstallAction::RemoveOnlyRepoGrammarManagedConfiguration,
            InstallAction::OptionalMarkerFencedInstructionUpdate,
            InstallAction::DoNotImposeRepoMirrorPolicy,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_and_scope_parsing_is_explicit() {
        assert_eq!(AgentTarget::parse("codex"), Ok(AgentTarget::Codex));
        assert_eq!(
            InstallScope::parse("project"),
            Ok(InstallScope::ProjectLocal)
        );
        assert!(AgentTarget::parse("unknown").is_err());
    }

    #[test]
    fn install_plan_includes_reversibility_and_mirror_policy_guard() {
        let plan = plan_install(&InstallRequest {
            dry_run: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        });

        assert!(!plan.telemetry_enabled);
        assert!(plan
            .actions
            .contains(&InstallAction::StoreReversibleReceipt));
        assert!(plan
            .actions
            .contains(&InstallAction::DoNotImposeRepoMirrorPolicy));
    }
}
