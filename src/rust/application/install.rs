//! Safe machine-level agent integration planning.

use crate::error::RepoGrammarError;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MCP_SERVER_NAME: &str = "repogrammar";
pub const CLI_BINARY_NAME: &str = "repogrammar";
pub const LIVE_AGENT_TARGETS: [AgentTarget; 2] = [AgentTarget::Codex, AgentTarget::ClaudeCode];
pub const KNOWN_AGENT_TARGETS: [AgentTarget; 8] = [
    AgentTarget::Codex,
    AgentTarget::ClaudeCode,
    AgentTarget::Cursor,
    AgentTarget::Opencode,
    AgentTarget::Hermes,
    AgentTarget::Gemini,
    AgentTarget::Antigravity,
    AgentTarget::Kiro,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTarget {
    AllSupported,
    None,
    Codex,
    ClaudeCode,
    Cursor,
    Opencode,
    Hermes,
    Gemini,
    Antigravity,
    Kiro,
}

impl AgentTarget {
    pub fn parse(value: &str) -> Result<Self, String> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "all" => Ok(Self::AllSupported),
            "auto" => Ok(Self::AllSupported),
            "none" => Ok(Self::None),
            "codex" => Ok(Self::Codex),
            "claude-code" | "claude" => Ok(Self::ClaudeCode),
            "cursor" => Ok(Self::Cursor),
            "opencode" | "open-code" => Ok(Self::Opencode),
            "hermes" | "hermes-agent" => Ok(Self::Hermes),
            "gemini" | "gemini-cli" => Ok(Self::Gemini),
            "antigravity" | "antigravity-ide" => Ok(Self::Antigravity),
            "kiro" => Ok(Self::Kiro),
            _ => Err(format!(
                "unsupported target {value}; expected auto, all, none, codex, claude-code, cursor, opencode, hermes, gemini, antigravity, or kiro"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllSupported => "all",
            Self::None => "none",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Cursor => "cursor",
            Self::Opencode => "opencode",
            Self::Hermes => "hermes",
            Self::Gemini => "gemini",
            Self::Antigravity => "antigravity",
            Self::Kiro => "kiro",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::AllSupported => "All supported agents",
            Self::None => "No agents",
            Self::Codex => "Codex CLI",
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Opencode => "opencode",
            Self::Hermes => "Hermes Agent",
            Self::Gemini => "Gemini CLI",
            Self::Antigravity => "Antigravity IDE",
            Self::Kiro => "Kiro",
        }
    }

    pub fn detection_binary(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("codex"),
            Self::ClaudeCode => Some("claude"),
            Self::Cursor => Some("cursor"),
            Self::Opencode => Some("opencode"),
            Self::Hermes => Some("hermes"),
            Self::Gemini => Some("gemini"),
            Self::Antigravity => Some("antigravity"),
            Self::Kiro => Some("kiro"),
            Self::AllSupported | Self::None => None,
        }
    }

    pub fn supports_scope(self, scope: InstallScope) -> bool {
        match self {
            Self::Codex | Self::Hermes | Self::Antigravity => scope == InstallScope::Global,
            Self::ClaudeCode | Self::Cursor | Self::Opencode | Self::Gemini | Self::Kiro => true,
            Self::AllSupported | Self::None => false,
        }
    }

    pub fn has_live_writer(self, scope: InstallScope) -> bool {
        matches!(
            (self, scope),
            (Self::Codex, InstallScope::Global) | (Self::ClaudeCode, InstallScope::Global)
        )
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
    pub selected_targets: Vec<AgentTarget>,
    pub scope: InstallScope,
    pub dry_run: bool,
    pub print_config: bool,
    pub print_config_target: Option<AgentTarget>,
    pub assume_yes: bool,
    pub no_permissions: bool,
    pub telemetry_enabled: bool,
    pub telemetry_explicitly_configured: bool,
}

impl Default for InstallRequest {
    fn default() -> Self {
        Self {
            target: AgentTarget::AllSupported,
            selected_targets: Vec::new(),
            scope: InstallScope::Global,
            dry_run: false,
            print_config: false,
            print_config_target: None,
            assume_yes: false,
            no_permissions: false,
            telemetry_enabled: false,
            telemetry_explicitly_configured: false,
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
pub struct InstallExecutionContext {
    pub executable_path: String,
    pub command_dir: String,
    pub command_dir_on_path: bool,
    pub data_dir: String,
    pub current_dir: String,
    /// Resolved absolute instruction-file paths per target. Populated only when a
    /// `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` override resolves to an absolute
    /// path; otherwise instruction writing stays deferred for that target.
    pub instruction_files: Vec<(AgentTarget, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallExecutionOutcome {
    pub command: &'static str,
    pub target: AgentTarget,
    pub scope: InstallScope,
    /// Targets that had no pre-existing RepoGrammar-owned integration before
    /// this execution and can therefore be removed by an outer transaction.
    pub configured_targets: Vec<AgentTarget>,
    /// Targets whose existing RepoGrammar-owned integration was refreshed from
    /// an obsolete executable authority. These targets pre-date this execution
    /// and must never be treated as newly created during outer rollback.
    pub reconfigured_targets: Vec<AgentTarget>,
    pub skipped_targets: Vec<AgentTarget>,
    pub receipt_paths: Vec<String>,
    pub installed_executable_path: Option<String>,
    pub command_path: Option<String>,
    pub command_on_path: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAgentAction {
    pub target: AgentTarget,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMcpServerConfig {
    pub executable_path: String,
    pub args: Vec<String>,
    pub scope: InstallScope,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeMcpServerState {
    Present(NativeMcpServerConfig),
    NotFound,
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentIntegrationInspection {
    Unmanaged,
    OwnedCurrent,
    OwnedOutdated,
    Foreign,
    OwnedDrifted,
    Malformed,
}

pub trait NativeAgentConfigurator {
    fn inspect_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        current_dir: &str,
    ) -> Result<NativeMcpServerState, RepoGrammarError>;

    fn add_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        executable_path: &str,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError>;

    fn remove_mcp_server(
        &self,
        target: AgentTarget,
        scope: InstallScope,
        current_dir: &str,
    ) -> Result<NativeAgentAction, RepoGrammarError>;
}

pub trait McpSelfTestRunner {
    fn self_test(&self, executable_path: &str, current_dir: &str) -> Result<(), RepoGrammarError>;
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

pub fn execute_install(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    self_tester: &impl McpSelfTestRunner,
) -> Result<InstallExecutionOutcome, RepoGrammarError> {
    require_live_write_confirmation(request)?;
    require_global_live_scope(request.scope)?;
    validate_execution_context(context)?;
    let selected_targets = selected_concrete_targets(request)?;
    require_live_writer_support(&selected_targets, request.scope)?;

    let mut targets_to_configure = Vec::new();
    let mut targets_to_reconfigure = Vec::new();
    let mut skipped_targets = Vec::new();
    for target in selected_targets {
        match inspect_agent_integration(context, target, request.scope, configurator)? {
            AgentIntegrationInspection::OwnedCurrent => skipped_targets.push(target),
            AgentIntegrationInspection::OwnedOutdated => targets_to_reconfigure.push(target),
            AgentIntegrationInspection::Unmanaged => targets_to_configure.push(target),
            AgentIntegrationInspection::Foreign => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "native {} MCP entry exists without a RepoGrammar-owned receipt; refusing to overwrite foreign configuration",
                    target.as_str()
                )));
            }
            AgentIntegrationInspection::OwnedDrifted => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "RepoGrammar-owned {} receipt does not match native MCP state; refusing automatic repair",
                    target.as_str()
                )));
            }
            AgentIntegrationInspection::Malformed => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "native {} MCP entry is malformed or unsupported; refusing automatic repair",
                    target.as_str()
                )));
            }
        }
    }
    let reconfiguration_snapshots = targets_to_reconfigure
        .iter()
        .copied()
        .map(|target| capture_owned_integration(context, target, request.scope, configurator))
        .collect::<Result<Vec<_>, _>>()?;
    let command_record = install_cli_command(context)?;

    if targets_to_configure.is_empty() && targets_to_reconfigure.is_empty() {
        if let Err(error) =
            self_tester.self_test(&command_record.executable_path, &context.current_dir)
        {
            let rollback = rollback_command_install(&command_record);
            return Err(install_rollback_error(error, rollback));
        }
        return Ok(InstallExecutionOutcome {
            command: "install",
            target: request.target,
            scope: request.scope,
            configured_targets: Vec::new(),
            reconfigured_targets: Vec::new(),
            skipped_targets,
            receipt_paths: Vec::new(),
            installed_executable_path: Some(command_record.executable_path),
            command_path: Some(command_record.command_path),
            command_on_path: context.command_dir_on_path,
            message: "selected agent MCP integrations are already managed by RepoGrammar"
                .to_string(),
        });
    }

    if let Err(error) = self_tester.self_test(&command_record.executable_path, &context.current_dir)
    {
        let rollback = rollback_command_install(&command_record);
        return Err(install_rollback_error(error, rollback));
    }

    let mut mutations = InstallMutationLog::default();
    let mut receipt_paths = Vec::new();

    // Drop the stale native entry for each drifted target before re-adding it at
    // the authority, because `mcp add` may reject or duplicate an existing name.
    // Capture happened before any write. Once removal succeeds, every later
    // failure restores the exact pre-existing native entry, receipt, receipt
    // backup, and managed-instruction files instead of deleting an integration
    // that predates this execution.
    for (target, snapshot) in targets_to_reconfigure.iter().zip(reconfiguration_snapshots) {
        if let Err(error) =
            configurator.remove_mcp_server(*target, request.scope, &context.current_dir)
        {
            let rollback =
                rollback_install_run(request, context, configurator, &mutations, &command_record);
            return Err(install_rollback_error(error, rollback));
        }
        mutations.reconfigured_snapshots.push(snapshot);
    }

    let targets_to_add: Vec<AgentTarget> = targets_to_reconfigure
        .iter()
        .copied()
        .chain(targets_to_configure.iter().copied())
        .collect();
    for target in targets_to_add {
        let action = match configurator.add_mcp_server(
            target,
            request.scope,
            &command_record.executable_path,
            &context.current_dir,
        ) {
            Ok(action) => action,
            Err(error) => {
                let rollback = rollback_install_run(
                    request,
                    context,
                    configurator,
                    &mutations,
                    &command_record,
                );
                return Err(install_rollback_error(error, rollback));
            }
        };
        let reconfigured = targets_to_reconfigure.contains(&target);
        if !reconfigured {
            mutations.configured_targets.push(target);
        }
        let (instruction_path, instruction_action) = match instruction_file_for(context, target) {
            Some(path) => match write_managed_instruction_section(Path::new(path)) {
                Ok(instruction_action) => (Some(path.to_string()), instruction_action),
                Err(error) => {
                    let rollback = rollback_install_run(
                        request,
                        context,
                        configurator,
                        &mutations,
                        &command_record,
                    );
                    return Err(install_rollback_error(error, rollback));
                }
            },
            None => (None, InstructionAction::Deferred),
        };
        let receipt_path = match write_install_receipt(
            request,
            context,
            &action,
            &command_record.executable_path,
            instruction_path.as_deref(),
            instruction_action,
        ) {
            Ok(receipt_path) => receipt_path,
            Err(error) => {
                if !reconfigured {
                    mutations
                        .configured_instructions
                        .push((instruction_path, instruction_action));
                }
                let rollback = rollback_install_run(
                    request,
                    context,
                    configurator,
                    &mutations,
                    &command_record,
                );
                return Err(install_rollback_error(error, rollback));
            }
        };
        if !reconfigured {
            mutations
                .configured_instructions
                .push((instruction_path, instruction_action));
            mutations.new_receipt_paths.push(receipt_path.clone());
        }
        receipt_paths.push(receipt_path);
    }

    for target in mutations
        .configured_targets
        .iter()
        .chain(targets_to_reconfigure.iter())
        .chain(skipped_targets.iter())
    {
        match configurator.inspect_mcp_server(*target, request.scope, &context.current_dir) {
            Ok(NativeMcpServerState::Present(config))
                if native_config_matches(
                    &config,
                    request.scope,
                    &command_record.executable_path,
                ) => {}
            Ok(NativeMcpServerState::Present(_))
            | Ok(NativeMcpServerState::NotFound)
            | Ok(NativeMcpServerState::Malformed) => {
                let rollback = rollback_install_run(
                    request,
                    context,
                    configurator,
                    &mutations,
                    &command_record,
                );
                return Err(install_rollback_error(
                    RepoGrammarError::InvalidInput(format!(
                        "native {} MCP entry was not present after configuration",
                        target.as_str()
                    )),
                    rollback,
                ));
            }
            Err(error) => {
                let rollback = rollback_install_run(
                    request,
                    context,
                    configurator,
                    &mutations,
                    &command_record,
                );
                return Err(install_rollback_error(error, rollback));
            }
        }
    }
    if let Err(error) = self_tester.self_test(&command_record.executable_path, &context.current_dir)
    {
        let rollback =
            rollback_install_run(request, context, configurator, &mutations, &command_record);
        return Err(install_rollback_error(error, rollback));
    }

    let reconfigured_any = !targets_to_reconfigure.is_empty();
    Ok(InstallExecutionOutcome {
        command: "install",
        target: request.target,
        scope: request.scope,
        configured_targets: mutations.configured_targets,
        reconfigured_targets: targets_to_reconfigure,
        skipped_targets,
        receipt_paths,
        installed_executable_path: Some(command_record.executable_path),
        command_path: Some(command_record.command_path),
        command_on_path: command_record.command_on_path,
        message: if reconfigured_any {
            "agent MCP integration installed or migrated to the managed authority after self-test"
                .to_string()
        } else {
            "agent MCP integration installed after self-test".to_string()
        },
    })
}

pub fn execute_uninstall(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
) -> Result<InstallExecutionOutcome, RepoGrammarError> {
    require_live_write_confirmation(request)?;
    require_global_live_scope(request.scope)?;
    validate_execution_context(context)?;
    let selected_targets = selected_concrete_targets(request)?;
    require_live_writer_support(&selected_targets, request.scope)?;

    let mut configured_targets = Vec::new();
    let mut receipt_paths = Vec::new();
    for target in selected_targets {
        let receipt_path = receipt_path(context, target, request.scope);
        if !receipt_path.is_file() {
            if request.target == AgentTarget::AllSupported || !request.selected_targets.is_empty() {
                continue;
            }
            return Err(RepoGrammarError::InvalidInput(
                "no RepoGrammar-managed install receipt found; refusing to remove unmanaged agent configuration"
                    .to_string(),
            ));
        }
        validate_receipt_ownership(&receipt_path, target, request.scope)?;
        let instruction_path = receipt_instruction_file_path(&receipt_path);
        let instruction_action = receipt_instruction_action(&receipt_path);
        configurator.remove_mcp_server(target, request.scope, &context.current_dir)?;
        if let Some(instruction_path) = instruction_path {
            revert_managed_instruction(Path::new(&instruction_path), instruction_action)?;
        }
        remove_receipt(&receipt_path)?;
        configured_targets.push(target);
        receipt_paths.push(display_path(&receipt_path));
    }
    if configured_targets.is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "no RepoGrammar-managed install receipt found; refusing to remove unmanaged agent configuration"
                .to_string(),
        ));
    }

    Ok(InstallExecutionOutcome {
        command: "uninstall",
        target: request.target,
        scope: request.scope,
        configured_targets,
        reconfigured_targets: Vec::new(),
        skipped_targets: Vec::new(),
        receipt_paths,
        installed_executable_path: None,
        command_path: None,
        command_on_path: context.command_dir_on_path,
        message: "RepoGrammar-owned agent MCP integration removed".to_string(),
    })
}

pub fn owned_install_receipt_exists(
    context: &InstallExecutionContext,
    target: AgentTarget,
    scope: InstallScope,
) -> Result<bool, RepoGrammarError> {
    let path = receipt_path(context, target, scope);
    if !path.is_file() {
        return Ok(false);
    }
    validate_receipt_ownership(&path, target, scope)?;
    Ok(true)
}

pub fn inspect_agent_integration(
    context: &InstallExecutionContext,
    target: AgentTarget,
    scope: InstallScope,
    configurator: &impl NativeAgentConfigurator,
) -> Result<AgentIntegrationInspection, RepoGrammarError> {
    let owned = owned_install_receipt_exists(context, target, scope)?;
    let native = configurator.inspect_mcp_server(target, scope, &context.current_dir)?;
    if !owned {
        return Ok(match native {
            NativeMcpServerState::NotFound => AgentIntegrationInspection::Unmanaged,
            NativeMcpServerState::Present(_) => AgentIntegrationInspection::Foreign,
            NativeMcpServerState::Malformed => AgentIntegrationInspection::Malformed,
        });
    }

    let receipt = receipt_path(context, target, scope);
    let recorded_executable = receipt_executable_path(&receipt).ok_or_else(|| {
        RepoGrammarError::InvalidInput("install receipt is malformed".to_string())
    })?;
    if !Path::new(&recorded_executable).is_absolute() {
        return Ok(AgentIntegrationInspection::Malformed);
    }
    Ok(match native {
        NativeMcpServerState::Present(config)
            if native_config_matches(&config, scope, &recorded_executable) =>
        {
            if receipt_targets_authority(&receipt, &managed_executable_path(context)) {
                AgentIntegrationInspection::OwnedCurrent
            } else {
                AgentIntegrationInspection::OwnedOutdated
            }
        }
        NativeMcpServerState::Present(_) | NativeMcpServerState::NotFound => {
            AgentIntegrationInspection::OwnedDrifted
        }
        NativeMcpServerState::Malformed => AgentIntegrationInspection::Malformed,
    })
}

fn managed_executable_path(context: &InstallExecutionContext) -> String {
    display_path(&Path::new(&context.data_dir).join("bin").join(binary_name()))
}

fn native_config_matches(
    config: &NativeMcpServerConfig,
    scope: InstallScope,
    executable_path: &str,
) -> bool {
    config.scope == scope
        && config.enabled
        && normalized_lexical_path(Path::new(&config.executable_path))
            == normalized_lexical_path(Path::new(executable_path))
        && config.args.len() == 1
        && config.args[0] == "serve"
}

pub fn supported_concrete_targets() -> Vec<AgentTarget> {
    LIVE_AGENT_TARGETS.to_vec()
}

pub fn known_agent_targets() -> Vec<AgentTarget> {
    KNOWN_AGENT_TARGETS.to_vec()
}

fn selected_concrete_targets(
    request: &InstallRequest,
) -> Result<Vec<AgentTarget>, RepoGrammarError> {
    let targets = if request.selected_targets.is_empty() {
        concrete_targets(request.target)
    } else {
        request.selected_targets.clone()
    };
    normalize_concrete_targets(&targets)
}

pub fn normalize_concrete_targets(
    targets: &[AgentTarget],
) -> Result<Vec<AgentTarget>, RepoGrammarError> {
    let mut normalized = Vec::new();
    for supported in KNOWN_AGENT_TARGETS {
        if targets.contains(&supported) && !normalized.contains(&supported) {
            normalized.push(supported);
        }
    }
    if targets.contains(&AgentTarget::AllSupported) {
        return Err(RepoGrammarError::InvalidInput(
            "selected install targets must be concrete coding agents".to_string(),
        ));
    }
    if normalized.is_empty() {
        return Err(RepoGrammarError::InvalidInput(
            "install requires at least one supported coding agent target".to_string(),
        ));
    }
    Ok(normalized)
}

pub fn targets_for_display(request: &InstallRequest) -> Vec<AgentTarget> {
    if !request.selected_targets.is_empty() {
        return normalize_targets_for_display(&request.selected_targets);
    }
    match request.target {
        AgentTarget::AllSupported => supported_concrete_targets(),
        AgentTarget::None => Vec::new(),
        target => vec![target],
    }
}

pub fn target_config_snippet(target: AgentTarget, scope: InstallScope) -> Result<String, String> {
    if target == AgentTarget::AllSupported || target == AgentTarget::None {
        return Err("print-config requires a concrete agent target".to_string());
    }
    if !target.supports_scope(scope) {
        return Ok(format!(
            "{} does not support {} scope in the RepoGrammar installer\n",
            target.display_name(),
            scope.as_str()
        ));
    }
    let command = "<repogrammar-executable>";
    let args = r#"["serve"]"#;
    let json_stdio = format!(
        r#"{{
  "type": "stdio",
  "command": "{command}",
  "args": {args}
}}"#
    );
    let snippet = match target {
        AgentTarget::Codex => format!(
            r#"# ~/.codex/config.toml
[mcp_servers.{MCP_SERVER_NAME}]
command = "{command}"
args = ["serve"]
"#
        ),
        AgentTarget::ClaudeCode => match scope {
            InstallScope::Global => format!(
                r#"# claude mcp add --scope user {MCP_SERVER_NAME} -- {command} serve
{json_stdio}
"#
            ),
            InstallScope::ProjectLocal => format!(
                r#"# ./.mcp.json
{{
  "mcpServers": {{
    "{MCP_SERVER_NAME}": {json_stdio}
  }}
}}
"#
            ),
        },
        AgentTarget::Cursor => format!(
            r#"# {}
{{
  "mcpServers": {{
    "{MCP_SERVER_NAME}": {{
      "type": "stdio",
      "command": "{command}",
      "args": ["serve", "--path", "{}"]
    }}
  }}
}}
"#,
            if scope == InstallScope::Global {
                "~/.cursor/mcp.json"
            } else {
                "./.cursor/mcp.json"
            },
            if scope == InstallScope::Global {
                "${workspaceFolder}"
            } else {
                "<absolute-project-path>"
            }
        ),
        AgentTarget::Opencode => format!(
            r#"# {}
{{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {{
    "{MCP_SERVER_NAME}": {{
      "type": "local",
      "command": ["{command}", "serve"],
      "enabled": true
    }}
  }}
}}
"#,
            if scope == InstallScope::Global {
                "$XDG_CONFIG_HOME/opencode/opencode.jsonc"
            } else {
                "./opencode.jsonc"
            }
        ),
        AgentTarget::Hermes => format!(
            r#"# $HERMES_HOME/config.yaml
mcp_servers:
  {MCP_SERVER_NAME}:
    command: "{command}"
    args: ["serve"]
platform_toolsets:
  cli:
    - "mcp-{MCP_SERVER_NAME}"
"#
        ),
        AgentTarget::Gemini => format!(
            r#"# {}
{{
  "mcpServers": {{
    "{MCP_SERVER_NAME}": {json_stdio}
  }}
}}
"#,
            if scope == InstallScope::Global {
                "~/.gemini/settings.json"
            } else {
                "./.gemini/settings.json"
            }
        ),
        AgentTarget::Antigravity => format!(
            r#"# ~/.gemini/config/mcp_config.json or ~/.gemini/antigravity/mcp_config.json
{{
  "mcpServers": {{
    "{MCP_SERVER_NAME}": {{
      "command": "{command}",
      "args": ["serve"]
    }}
  }}
}}
"#
        ),
        AgentTarget::Kiro => format!(
            r#"# {}
{{
  "mcpServers": {{
    "{MCP_SERVER_NAME}": {json_stdio}
  }}
}}
"#,
            if scope == InstallScope::Global {
                "~/.kiro/settings/mcp.json"
            } else {
                "./.kiro/settings/mcp.json"
            }
        ),
        AgentTarget::AllSupported | AgentTarget::None => unreachable!("checked above"),
    };
    Ok(snippet)
}

pub fn target_plan_line(target: AgentTarget, scope: InstallScope) -> String {
    if !target.supports_scope(scope) {
        return format!(
            "native_mcp: deferred {} {} install is unsupported",
            target.as_str(),
            scope.as_str()
        );
    }
    match (target, scope) {
        (AgentTarget::Codex, InstallScope::Global) => {
            "native_mcp: codex mcp add repogrammar -- <repogrammar-executable> serve".to_string()
        }
        (AgentTarget::ClaudeCode, InstallScope::Global) => {
            "native_mcp: claude mcp add --scope user repogrammar -- <repogrammar-executable> serve"
                .to_string()
        }
        (target, scope) if target.has_live_writer(scope) => {
            format!("native_mcp: {} live writer available", target.as_str())
        }
        (target, scope) => format!(
            "native_mcp: deferred {} {} live writes; use --print-config {} to inspect the MCP snippet",
            target.as_str(),
            scope.as_str(),
            target.as_str()
        ),
    }
}

/// Human-readable instruction-file plan line for dry-run/planning output. Surfaces
/// the deferred default and the env override that enables managed instruction
/// writes, without guessing any instruction-file path.
pub fn target_instruction_plan_line<F>(
    target: AgentTarget,
    scope: InstallScope,
    lookup: &F,
) -> String
where
    F: Fn(&str) -> Option<String>,
{
    if !target.supports_scope(scope) {
        return format!(
            "instruction: deferred {} {} instruction writes are unsupported",
            target.as_str(),
            scope.as_str()
        );
    }
    match resolve_instruction_file(target, lookup) {
        Some(path) => format!("instruction: managed section -> {path}"),
        None => format!(
            "instruction: deferred; set {} to an absolute path to enable managed instruction writes",
            instruction_env_var(target)
        ),
    }
}

/// CodeGraph-style per-target adapter contract. Consolidates the registry's
/// per-target capabilities (scope support, live-writer status, config preview,
/// native and instruction plan lines) behind one cohesive type so callers query
/// the registry through a single contract instead of scattered free functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetAdapter {
    target: AgentTarget,
}

impl TargetAdapter {
    pub fn new(target: AgentTarget) -> Self {
        Self { target }
    }

    pub fn target(&self) -> AgentTarget {
        self.target
    }

    pub fn target_id(&self) -> &'static str {
        self.target.as_str()
    }

    pub fn display_name(&self) -> &'static str {
        self.target.display_name()
    }

    pub fn detection_binary(&self) -> Option<&'static str> {
        self.target.detection_binary()
    }

    pub fn supports_scope(&self, scope: InstallScope) -> bool {
        self.target.supports_scope(scope)
    }

    pub fn has_live_writer(&self, scope: InstallScope) -> bool {
        self.target.has_live_writer(scope)
    }

    pub fn instruction_env_var(&self) -> String {
        instruction_env_var(self.target)
    }

    /// No-write MCP configuration preview for the target/scope.
    pub fn print_config(&self, scope: InstallScope) -> Result<String, String> {
        target_config_snippet(self.target, scope)
    }

    /// Native MCP plan line shown during dry-run planning.
    pub fn native_plan_line(&self, scope: InstallScope) -> String {
        target_plan_line(self.target, scope)
    }

    /// Instruction-file plan line shown during dry-run planning.
    pub fn instruction_plan_line<F>(&self, scope: InstallScope, lookup: &F) -> String
    where
        F: Fn(&str) -> Option<String>,
    {
        target_instruction_plan_line(self.target, scope, lookup)
    }

    /// Full ordered planning description for the target/scope: native MCP plan
    /// line followed by the instruction-file plan line.
    pub fn describe_paths<F>(&self, scope: InstallScope, lookup: &F) -> Vec<String>
    where
        F: Fn(&str) -> Option<String>,
    {
        vec![
            self.native_plan_line(scope),
            self.instruction_plan_line(scope, lookup),
        ]
    }
}

/// Adapter for a single known target.
pub fn target_adapter(target: AgentTarget) -> TargetAdapter {
    TargetAdapter::new(target)
}

/// Adapters for every known target, in registry order.
pub fn known_target_adapters() -> Vec<TargetAdapter> {
    KNOWN_AGENT_TARGETS
        .iter()
        .copied()
        .map(TargetAdapter::new)
        .collect()
}

fn normalize_targets_for_display(targets: &[AgentTarget]) -> Vec<AgentTarget> {
    let mut normalized = Vec::new();
    for known in KNOWN_AGENT_TARGETS {
        if targets.contains(&known) && !normalized.contains(&known) {
            normalized.push(known);
        }
    }
    normalized
}

fn require_live_writer_support(
    targets: &[AgentTarget],
    scope: InstallScope,
) -> Result<(), RepoGrammarError> {
    for target in targets {
        if !target.has_live_writer(scope) {
            return Err(RepoGrammarError::InvalidInput(format!(
                "{} {} live install/uninstall is deferred; use --dry-run or --print-config {}",
                target.as_str(),
                scope.as_str(),
                target.as_str()
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandInstallRecord {
    executable_path: String,
    command_path: String,
    command_on_path: bool,
    created_command: bool,
    created_executable: bool,
    previous_executable: Option<Vec<u8>>,
    previous_command_copy: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedIntegrationSnapshot {
    target: AgentTarget,
    native_executable_path: String,
    receipt: FileSnapshot,
    receipt_backup: FileSnapshot,
    instruction_files: Vec<FileSnapshot>,
}

fn capture_owned_integration(
    context: &InstallExecutionContext,
    target: AgentTarget,
    scope: InstallScope,
    configurator: &impl NativeAgentConfigurator,
) -> Result<OwnedIntegrationSnapshot, RepoGrammarError> {
    let receipt_path = receipt_path(context, target, scope);
    let native_executable_path = receipt_executable_path(&receipt_path).ok_or_else(|| {
        RepoGrammarError::InvalidInput("install receipt is malformed".to_string())
    })?;
    let native = configurator.inspect_mcp_server(target, scope, &context.current_dir)?;
    match native {
        NativeMcpServerState::Present(config)
            if native_config_matches(&config, scope, &native_executable_path) => {}
        _ => {
            return Err(RepoGrammarError::InvalidInput(format!(
                "RepoGrammar-owned {} integration changed during reconciliation; refusing automatic repair",
                target.as_str()
            )));
        }
    }

    let mut instruction_paths = Vec::new();
    if let Some(path) = receipt_instruction_file_path(&receipt_path) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(RepoGrammarError::InvalidInput(
                "install receipt contains a non-absolute instruction path; refusing automatic repair"
                    .to_string(),
            ));
        }
        instruction_paths.push(path);
    }
    if let Some(path) = instruction_file_for(context, target) {
        let path = PathBuf::from(path);
        if !instruction_paths
            .iter()
            .any(|existing| same_path(existing, &path))
        {
            instruction_paths.push(path);
        }
    }

    let receipt_backup_path = receipt_path.with_extension("json.bak");
    Ok(OwnedIntegrationSnapshot {
        target,
        native_executable_path,
        receipt: capture_file_snapshot(&receipt_path, "install receipt")?,
        receipt_backup: capture_file_snapshot(&receipt_backup_path, "install receipt backup")?,
        instruction_files: instruction_paths
            .iter()
            .map(|path| capture_file_snapshot(path, "managed instruction file"))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn capture_file_snapshot(path: &Path, label: &str) -> Result<FileSnapshot, RepoGrammarError> {
    let contents = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(RepoGrammarError::InvalidInput(format!(
                "{label} must be a regular file for safe reconciliation"
            )));
        }
        Ok(_) => Some(fs::read(path).map_err(|error| {
            RepoGrammarError::InvalidInput(format!("failed to snapshot {label}: {error}"))
        })?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(RepoGrammarError::InvalidInput(format!(
                "failed to inspect {label}: {error}"
            )));
        }
    };
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        contents,
    })
}

fn install_cli_command(
    context: &InstallExecutionContext,
) -> Result<CommandInstallRecord, RepoGrammarError> {
    let current_process_executable = std::env::current_exe().ok();
    install_cli_command_with_current_process(context, current_process_executable.as_deref())
}

fn install_cli_command_with_current_process(
    context: &InstallExecutionContext,
    current_process_executable: Option<&Path>,
) -> Result<CommandInstallRecord, RepoGrammarError> {
    let source = Path::new(&context.executable_path);
    let data_bin_dir = Path::new(&context.data_dir).join("bin");
    let installed_executable = data_bin_dir.join(binary_name());
    let command_dir = Path::new(&context.command_dir);
    let command_path = command_dir.join(binary_name());

    let mut record = CommandInstallRecord {
        executable_path: display_path(&installed_executable),
        command_path: display_path(&command_path),
        command_on_path: context.command_dir_on_path,
        created_command: false,
        created_executable: false,
        previous_executable: None,
        previous_command_copy: None,
    };

    fs::create_dir_all(&data_bin_dir).map_err(|error| {
        RepoGrammarError::InvalidInput(format!(
            "failed to create install binary directory: {error}"
        ))
    })?;
    fs::create_dir_all(command_dir).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to create command directory: {error}"))
    })?;

    if path_is_symlink(&installed_executable) {
        return Err(RepoGrammarError::InvalidInput(
            "installed RepoGrammar executable path is a symlink; refusing to overwrite".to_string(),
        ));
    }

    let executable_existed = installed_executable.exists();
    let command_path_was_managed_copy =
        command_path_is_managed_copy(&command_path, &installed_executable);
    let command_path_is_current_executable =
        command_path_matches_current_executable(&command_path, source, current_process_executable);
    let installed_is_current_executable = current_process_executable
        .map(|current| same_path(&installed_executable, current))
        .unwrap_or(false);
    let should_refresh_command_copy = command_path.exists()
        && !same_path(&command_path, &installed_executable)
        && command_path_was_managed_copy
        && !command_path_is_current_executable;
    if command_path.exists()
        && !same_path(&command_path, &installed_executable)
        && !command_path_was_managed_copy
        && !command_path_is_current_executable
    {
        return Err(RepoGrammarError::InvalidInput(
            "repogrammar command path already exists and is not managed by RepoGrammar".to_string(),
        ));
    }
    if !same_path(source, &installed_executable) && !installed_is_current_executable {
        if executable_existed {
            record.previous_executable = Some(read_file_bytes(
                &installed_executable,
                "installed RepoGrammar CLI",
            )?);
        }
        if should_refresh_command_copy {
            record.previous_command_copy =
                Some(read_file_bytes(&command_path, "repogrammar command")?);
        }
        replace_managed_file(source, &installed_executable, "installed RepoGrammar CLI")
            .inspect_err(|_| {
                let _ = rollback_command_install(&record);
            })?;
        record.created_executable = !executable_existed;
    }

    if command_path.exists() {
        if should_refresh_command_copy {
            refresh_command_copy(&installed_executable, &command_path).inspect_err(|_| {
                let _ = rollback_command_install(&record);
            })?;
        }
    } else {
        create_command_link_or_copy(&installed_executable, &command_path).inspect_err(|_| {
            let _ = rollback_command_install(&record);
        })?;
        record.created_command = true;
    }

    Ok(record)
}

fn command_path_matches_current_executable(
    command_path: &Path,
    configured_source: &Path,
    current_process_executable: Option<&Path>,
) -> bool {
    same_path(command_path, configured_source)
        || current_process_executable
            .map(|current| same_path(command_path, current))
            .unwrap_or(false)
}

fn read_file_bytes(path: &Path, label: &str) -> Result<Vec<u8>, RepoGrammarError> {
    fs::read(path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to back up {label}: {error}"))
    })
}

fn command_path_is_managed_copy(command_path: &Path, installed_executable: &Path) -> bool {
    if !command_path.is_file() || !installed_executable.is_file() {
        return false;
    }
    if path_is_symlink(command_path) || path_is_symlink(installed_executable) {
        return false;
    }
    match (fs::read(command_path), fs::read(installed_executable)) {
        (Ok(command), Ok(installed)) => command == installed,
        _ => false,
    }
}

fn refresh_command_copy(source: &Path, destination: &Path) -> Result<(), RepoGrammarError> {
    replace_managed_file(source, destination, "repogrammar command")
}

fn replace_managed_file(
    source: &Path,
    destination: &Path,
    label: &str,
) -> Result<(), RepoGrammarError> {
    let temporary = managed_replace_temp_path(destination);
    fs::copy(source, &temporary).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to stage new {label}: {error}"))
    })?;
    if destination.exists() {
        if let Err(error) = fs::remove_file(destination) {
            let _ = fs::remove_file(&temporary);
            return Err(previous_managed_file_removal_error(label, error));
        }
    }
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        RepoGrammarError::InvalidInput(format!("failed to activate new {label}: {error}"))
    })
}

fn managed_replace_temp_path(destination: &Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CLI_BINARY_NAME);
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    destination.with_file_name(format!("{file_name}.tmp-{}-{suffix}", std::process::id()))
}

fn previous_managed_file_removal_error(label: &str, error: std::io::Error) -> RepoGrammarError {
    RepoGrammarError::InvalidInput(format!(
        "failed to remove previous {label}: {error}; exit any running coding agent sessions that use RepoGrammar MCP, then rerun the install or build command"
    ))
}

#[derive(Default)]
struct InstallMutationLog {
    configured_targets: Vec<AgentTarget>,
    configured_instructions: Vec<(Option<String>, InstructionAction)>,
    new_receipt_paths: Vec<String>,
    reconfigured_snapshots: Vec<OwnedIntegrationSnapshot>,
}

fn rollback_install_run(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    mutations: &InstallMutationLog,
    command_record: &CommandInstallRecord,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut safe_new_cleanup = vec![false; mutations.configured_targets.len()];
    for (index, target) in mutations.configured_targets.iter().enumerate().rev() {
        match remove_new_integration_if_still_owned(
            request,
            context,
            configurator,
            *target,
            &command_record.executable_path,
        ) {
            Ok(()) => safe_new_cleanup[index] = true,
            Err(error) => failures.push(error),
        }
    }
    for snapshot in mutations.reconfigured_snapshots.iter().rev() {
        failures.extend(restore_owned_integration(
            request,
            context,
            configurator,
            snapshot,
            &command_record.executable_path,
        ));
    }
    for (index, path) in mutations.new_receipt_paths.iter().enumerate().rev() {
        if safe_new_cleanup.get(index) == Some(&true) {
            if let Err(error) = fs::remove_file(path) {
                failures.push(format!("receipt cleanup failed: {error}"));
            }
        }
    }
    for (index, (path, action)) in mutations.configured_instructions.iter().enumerate().rev() {
        if safe_new_cleanup.get(index) != Some(&true) {
            continue;
        }
        let wrote_section = matches!(
            action,
            InstructionAction::Created | InstructionAction::Appended | InstructionAction::Replaced
        );
        if let (Some(path), true) = (path, wrote_section) {
            if let Err(error) = revert_managed_instruction(Path::new(path), *action) {
                failures.push(format!("instruction rollback failed: {error}"));
            }
        }
    }
    failures.extend(rollback_command_install(command_record));
    failures
}

fn remove_new_integration_if_still_owned(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    target: AgentTarget,
    expected_executable: &str,
) -> Result<(), String> {
    match configurator.inspect_mcp_server(target, request.scope, &context.current_dir) {
        Ok(NativeMcpServerState::NotFound) => Ok(()),
        Ok(NativeMcpServerState::Present(config))
            if native_config_matches(&config, request.scope, expected_executable) =>
        {
            remove_expected_native_entry(
                request,
                context,
                configurator,
                target,
                expected_executable,
                None,
            )
            .map(|_| ())
        }
        Ok(NativeMcpServerState::Present(_)) | Ok(NativeMcpServerState::Malformed) => Err(format!(
            "native {} rollback refused because the MCP entry changed outside this install",
            target.as_str()
        )),
        Err(_) => Err(format!(
            "native {} rollback could not verify current MCP ownership",
            target.as_str()
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedNativeRemoval {
    Absent,
    AlternateAlreadyPresent,
}

fn remove_expected_native_entry(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    target: AgentTarget,
    expected_executable: &str,
    acceptable_alternate: Option<&str>,
) -> Result<ExpectedNativeRemoval, String> {
    match configurator.inspect_mcp_server(target, request.scope, &context.current_dir) {
        Ok(NativeMcpServerState::NotFound) => return Ok(ExpectedNativeRemoval::Absent),
        Ok(NativeMcpServerState::Present(config))
            if native_config_matches(&config, request.scope, expected_executable) => {}
        Ok(NativeMcpServerState::Present(config))
            if acceptable_alternate.is_some_and(|alternate| {
                native_config_matches(&config, request.scope, alternate)
            }) =>
        {
            return Ok(ExpectedNativeRemoval::AlternateAlreadyPresent);
        }
        Ok(NativeMcpServerState::Present(_)) | Ok(NativeMcpServerState::Malformed) => {
            return Err(format!(
                "native {} rollback refused because the MCP entry changed outside this install",
                target.as_str()
            ));
        }
        Err(_) => {
            return Err(format!(
                "native {} rollback could not verify current MCP ownership",
                target.as_str()
            ));
        }
    }

    let _ = configurator.remove_mcp_server(target, request.scope, &context.current_dir);
    match configurator.inspect_mcp_server(target, request.scope, &context.current_dir) {
        Ok(NativeMcpServerState::NotFound) => Ok(ExpectedNativeRemoval::Absent),
        Ok(NativeMcpServerState::Present(config))
            if acceptable_alternate.is_some_and(|alternate| {
                native_config_matches(&config, request.scope, alternate)
            }) =>
        {
            Ok(ExpectedNativeRemoval::AlternateAlreadyPresent)
        }
        Ok(NativeMcpServerState::Present(_)) | Ok(NativeMcpServerState::Malformed) => Err(format!(
            "native {} rollback cleanup did not produce an absent entry; preserving current state",
            target.as_str()
        )),
        Err(_) => Err(format!(
            "native {} rollback could not verify cleanup",
            target.as_str()
        )),
    }
}

fn restore_owned_integration(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    snapshot: &OwnedIntegrationSnapshot,
    replacement_executable: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let native =
        match configurator.inspect_mcp_server(snapshot.target, request.scope, &context.current_dir)
        {
            Ok(native) => native,
            Err(_) => {
                failures.push(format!(
                    "reconfigured {} rollback could not verify current MCP ownership",
                    snapshot.target.as_str()
                ));
                return failures;
            }
        };
    match native {
        NativeMcpServerState::Present(config)
            if native_config_matches(&config, request.scope, &snapshot.native_executable_path) => {}
        NativeMcpServerState::Present(config)
            if native_config_matches(&config, request.scope, replacement_executable) =>
        {
            let removal = match remove_expected_native_entry(
                request,
                context,
                configurator,
                snapshot.target,
                replacement_executable,
                Some(&snapshot.native_executable_path),
            ) {
                Ok(removal) => removal,
                Err(error) => {
                    failures.push(error);
                    return failures;
                }
            };
            if removal == ExpectedNativeRemoval::Absent {
                if let Err(error) =
                    restore_snapshot_native(request, context, configurator, snapshot)
                {
                    failures.push(error);
                    return failures;
                }
            }
        }
        NativeMcpServerState::NotFound => {
            if let Err(error) = restore_snapshot_native(request, context, configurator, snapshot) {
                failures.push(error);
                return failures;
            }
        }
        NativeMcpServerState::Present(_) | NativeMcpServerState::Malformed => {
            failures.push(format!(
                "reconfigured {} rollback refused because the MCP entry changed outside this install",
                snapshot.target.as_str()
            ));
            return failures;
        }
    }

    for file in snapshot.instruction_files.iter().rev() {
        if let Err(error) = restore_file_snapshot(file, "managed instruction file") {
            failures.push(error);
        }
    }
    if let Err(error) = restore_file_snapshot(&snapshot.receipt_backup, "install receipt backup") {
        failures.push(error);
    }
    if let Err(error) = restore_file_snapshot(&snapshot.receipt, "install receipt") {
        failures.push(error);
    }
    failures
}

fn restore_snapshot_native(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    snapshot: &OwnedIntegrationSnapshot,
) -> Result<(), String> {
    let _ = configurator.add_mcp_server(
        snapshot.target,
        request.scope,
        &snapshot.native_executable_path,
        &context.current_dir,
    );
    match configurator.inspect_mcp_server(snapshot.target, request.scope, &context.current_dir) {
        Ok(NativeMcpServerState::Present(config))
            if native_config_matches(&config, request.scope, &snapshot.native_executable_path) =>
        {
            Ok(())
        }
        Ok(_) => Err(format!(
            "reconfigured {} native restore failed verification; preserving current state",
            snapshot.target.as_str()
        )),
        Err(_) => Err(format!(
            "reconfigured {} native restore could not be verified",
            snapshot.target.as_str()
        )),
    }
}

fn restore_file_snapshot(snapshot: &FileSnapshot, label: &str) -> Result<(), String> {
    match &snapshot.contents {
        Some(contents) => {
            if let Some(parent) = snapshot.path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("{label} restore failed: {error}"))?;
            }
            if snapshot.path.exists() {
                let metadata = fs::symlink_metadata(&snapshot.path)
                    .map_err(|error| format!("{label} restore inspection failed: {error}"))?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(format!("{label} restore refused a non-regular destination"));
                }
            }
            let temporary = managed_replace_temp_path(&snapshot.path);
            fs::write(&temporary, contents)
                .map_err(|error| format!("{label} restore staging failed: {error}"))?;
            if snapshot.path.exists() {
                if let Err(error) = fs::remove_file(&snapshot.path) {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!("{label} restore replacement failed: {error}"));
                }
            }
            fs::rename(&temporary, &snapshot.path).map_err(|error| {
                let _ = fs::remove_file(&temporary);
                format!("{label} restore activation failed: {error}")
            })
        }
        None => match fs::symlink_metadata(&snapshot.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
                format!("{label} rollback refused to remove a non-regular destination"),
            ),
            Ok(_) => fs::remove_file(&snapshot.path)
                .map_err(|error| format!("{label} rollback cleanup failed: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("{label} rollback inspection failed: {error}")),
        },
    }
}

fn rollback_command_install(command_record: &CommandInstallRecord) -> Vec<String> {
    let mut failures = Vec::new();
    if command_record.created_command {
        if let Err(error) = fs::remove_file(&command_record.command_path) {
            failures.push(format!("command cleanup failed: {error}"));
        }
    } else if let Some(previous) = &command_record.previous_command_copy {
        if let Err(error) = fs::write(&command_record.command_path, previous) {
            failures.push(format!("command restore failed: {error}"));
        }
    }
    if command_record.created_executable {
        if let Err(error) = fs::remove_file(&command_record.executable_path) {
            failures.push(format!("installed executable cleanup failed: {error}"));
        }
    } else if let Some(previous) = &command_record.previous_executable {
        if let Err(error) = fs::write(&command_record.executable_path, previous) {
            failures.push(format!("installed executable restore failed: {error}"));
        }
    }
    failures
}

fn install_rollback_error(
    error: RepoGrammarError,
    rollback_failures: Vec<String>,
) -> RepoGrammarError {
    if rollback_failures.is_empty() {
        RepoGrammarError::InvalidInput(format!("{error}; install rolled back"))
    } else {
        RepoGrammarError::InvalidInput(format!(
            "{error}; install rollback attempted but failed: {}",
            rollback_failures.join("; ")
        ))
    }
}

fn create_command_link_or_copy(source: &Path, destination: &Path) -> Result<(), RepoGrammarError> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination).map_err(|error| {
            RepoGrammarError::InvalidInput(format!("failed to create repogrammar command: {error}"))
        })
    }
    #[cfg(not(unix))]
    {
        fs::copy(source, destination).map(|_| ()).map_err(|error| {
            RepoGrammarError::InvalidInput(format!("failed to create repogrammar command: {error}"))
        })
    }
}

pub(crate) fn binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "repogrammar.exe"
    }
    #[cfg(not(windows))]
    {
        CLI_BINARY_NAME
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    if let (Ok(left), Ok(right)) = (fs::canonicalize(left), fs::canonicalize(right)) {
        return left == right;
    }
    // Fallback when canonicalization is unavailable (for example a path
    // component does not exist, or the platform rejects the verbatim form):
    // compare best-effort normalized lexical paths. This stays conservative by
    // reporting a match only when both normalized forms are identical.
    normalized_lexical_path(left) == normalized_lexical_path(right)
}

pub(crate) fn normalized_lexical_path(path: &Path) -> String {
    let unified = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        unified.to_ascii_lowercase()
    } else {
        unified
    }
}

fn path_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn require_live_write_confirmation(request: &InstallRequest) -> Result<(), RepoGrammarError> {
    if request.assume_yes {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(
            "live install/uninstall writes require --yes".to_string(),
        ))
    }
}

fn require_global_live_scope(scope: InstallScope) -> Result<(), RepoGrammarError> {
    if scope == InstallScope::ProjectLocal {
        Err(RepoGrammarError::InvalidInput(
            "live project-local install/uninstall is deferred; use --scope global or --dry-run"
                .to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_execution_context(context: &InstallExecutionContext) -> Result<(), RepoGrammarError> {
    let executable = Path::new(&context.executable_path);
    if !executable.is_absolute() || !executable.is_file() {
        return Err(RepoGrammarError::InvalidInput(
            "installer executable path must be an absolute file path".to_string(),
        ));
    }
    let command_dir = Path::new(&context.command_dir);
    if !command_dir.is_absolute() {
        return Err(RepoGrammarError::InvalidInput(
            "installer command directory must be absolute".to_string(),
        ));
    }
    let data_dir = Path::new(&context.data_dir);
    if !data_dir.is_absolute() {
        return Err(RepoGrammarError::InvalidInput(
            "installer data directory must be absolute".to_string(),
        ));
    }
    let current_dir = Path::new(&context.current_dir);
    if !current_dir.is_absolute() || !current_dir.is_dir() {
        return Err(RepoGrammarError::InvalidInput(
            "installer current directory must be an absolute directory".to_string(),
        ));
    }
    Ok(())
}

fn concrete_targets(target: AgentTarget) -> Vec<AgentTarget> {
    match target {
        AgentTarget::AllSupported => LIVE_AGENT_TARGETS.to_vec(),
        AgentTarget::None => Vec::new(),
        AgentTarget::Codex
        | AgentTarget::ClaudeCode
        | AgentTarget::Cursor
        | AgentTarget::Opencode
        | AgentTarget::Hermes
        | AgentTarget::Gemini
        | AgentTarget::Antigravity
        | AgentTarget::Kiro => vec![target],
    }
}

fn write_install_receipt(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    action: &NativeAgentAction,
    executable_path: &str,
    instruction_file_path: Option<&str>,
    instruction_action: InstructionAction,
) -> Result<String, RepoGrammarError> {
    let receipt_path = receipt_path(context, action.target, request.scope);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RepoGrammarError::InvalidInput(format!(
                "failed to create install receipt directory: {error}"
            ))
        })?;
    }
    if receipt_path.exists() {
        let backup = receipt_path.with_extension("json.bak");
        fs::copy(&receipt_path, &backup).map_err(|error| {
            RepoGrammarError::InvalidInput(format!(
                "failed to back up existing install receipt: {error}"
            ))
        })?;
    }
    let receipt = json!({
        "schema_version": 1,
        "managed_by": "repogrammar",
        "mcp_server": MCP_SERVER_NAME,
        "target": action.target.as_str(),
        "scope": request.scope.as_str(),
        "executable_path": executable_path,
        "native_program": action.program,
        "native_args": action.args,
        "instruction_file_path": instruction_file_path,
        "instruction_action": instruction_action.as_str(),
        "telemetry_enabled": request.telemetry_enabled,
        "created_unix_seconds": unix_seconds(),
    });
    let contents = format!("{receipt}\n");
    let temporary = receipt_path.with_extension("json.tmp");
    fs::write(&temporary, contents).map_err(|error| {
        RepoGrammarError::InvalidInput(format!(
            "failed to write temporary install receipt: {error}"
        ))
    })?;
    validate_receipt_ownership(&temporary, action.target, request.scope)?;
    fs::rename(&temporary, &receipt_path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        RepoGrammarError::InvalidInput(format!(
            "failed to atomically write install receipt: {error}"
        ))
    })?;
    validate_receipt_ownership(&receipt_path, action.target, request.scope)?;
    Ok(display_path(&receipt_path))
}

fn validate_receipt_ownership(
    receipt_path: &Path,
    target: AgentTarget,
    scope: InstallScope,
) -> Result<(), RepoGrammarError> {
    let contents = fs::read_to_string(receipt_path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to read install receipt: {error}"))
    })?;
    let value: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|_| RepoGrammarError::InvalidInput("install receipt is malformed".to_string()))?;
    let valid = value.get("schema_version").and_then(|value| value.as_u64()) == Some(1)
        && value.get("managed_by").and_then(|value| value.as_str()) == Some("repogrammar")
        && value.get("mcp_server").and_then(|value| value.as_str()) == Some(MCP_SERVER_NAME)
        && value.get("target").and_then(|value| value.as_str()) == Some(target.as_str())
        && value.get("scope").and_then(|value| value.as_str()) == Some(scope.as_str());
    if valid {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(
            "install receipt is not owned by this RepoGrammar target/scope".to_string(),
        ))
    }
}

fn remove_receipt(receipt_path: &Path) -> Result<(), RepoGrammarError> {
    fs::remove_file(receipt_path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to remove install receipt: {error}"))
    })
}

fn receipt_executable_path(receipt_path: &Path) -> Option<String> {
    let contents = fs::read_to_string(receipt_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value
        .get("executable_path")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

/// True when the receipt's recorded executable still points at the current
/// managed authority. A receipt that cannot be read, or whose recorded path has
/// drifted from the authority, reports false so the caller reconfigures it.
fn receipt_targets_authority(receipt_path: &Path, authority_executable: &str) -> bool {
    match receipt_executable_path(receipt_path) {
        Some(recorded) => same_path(Path::new(&recorded), Path::new(authority_executable)),
        None => false,
    }
}

fn receipt_instruction_file_path(receipt_path: &Path) -> Option<String> {
    let contents = fs::read_to_string(receipt_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value
        .get("instruction_file_path")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

/// Recover the `instruction_action` recorded at install time so uninstall can
/// reverse the exact write it performed. Falls back to `Deferred` (remove the
/// section but never delete the file) when the receipt is unreadable or records
/// an unrecognized action, keeping uninstall conservative about file deletion.
fn receipt_instruction_action(receipt_path: &Path) -> InstructionAction {
    let Ok(contents) = fs::read_to_string(receipt_path) else {
        return InstructionAction::Deferred;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return InstructionAction::Deferred;
    };
    value
        .get("instruction_action")
        .and_then(|value| value.as_str())
        .and_then(InstructionAction::from_receipt_str)
        .unwrap_or(InstructionAction::Deferred)
}

fn receipt_path(
    context: &InstallExecutionContext,
    target: AgentTarget,
    scope: InstallScope,
) -> PathBuf {
    Path::new(&context.data_dir)
        .join("install")
        .join("receipts")
        .join(format!("{}-{}.json", target.as_str(), scope.as_str()))
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Exact begin marker for the RepoGrammar managed instruction section.
pub const MANAGED_INSTRUCTION_BEGIN: &str = "<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->";
/// Exact end marker for the RepoGrammar managed instruction section.
pub const MANAGED_INSTRUCTION_END: &str = "<!-- END REPOGRAMMAR MANAGED SECTION -->";

/// Outcome of a managed instruction-section write or removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionAction {
    /// No instruction file path was resolved; live writing stays deferred.
    Deferred,
    /// The instruction file did not exist and was created with the managed section.
    Created,
    /// The instruction file existed without markers; the managed section was appended.
    Appended,
    /// A complete managed section already existed and was replaced.
    Replaced,
    /// The existing managed section was already byte-equivalent; nothing was written.
    Unchanged,
    /// Uninstall removed an existing managed section.
    Removed,
    /// Uninstall found no managed section to remove.
    NotPresent,
}

impl InstructionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deferred => "deferred",
            Self::Created => "created",
            Self::Appended => "appended",
            Self::Replaced => "replaced",
            Self::Unchanged => "unchanged",
            Self::Removed => "removed",
            Self::NotPresent => "not_present",
        }
    }

    /// Parse the value previously serialized by [`InstructionAction::as_str`] back
    /// into the enum. Returns `None` for any unrecognized receipt string.
    fn from_receipt_str(value: &str) -> Option<Self> {
        match value {
            "deferred" => Some(Self::Deferred),
            "created" => Some(Self::Created),
            "appended" => Some(Self::Appended),
            "replaced" => Some(Self::Replaced),
            "unchanged" => Some(Self::Unchanged),
            "removed" => Some(Self::Removed),
            "not_present" => Some(Self::NotPresent),
            _ => None,
        }
    }
}

/// Canonical managed instruction block, including both markers. Intentionally
/// short and conditional, and never embeds repository-specific facts.
pub fn managed_instruction_block() -> String {
    format!(
        "{MANAGED_INSTRUCTION_BEGIN}\n\
## RepoGrammar\n\
\n\
In repositories initialized with RepoGrammar (`.repogrammar/` exists), call the MCP tool `repogrammar_context` before grep/find/Read when you need implementation-pattern context, analogous examples, family conformance, deviation explanation, or an edit plan. For find/check/explain operations, pass the repo-relative path, symbol/member id, framework role, or pattern question you have; RepoGrammar discovers candidate families internally and returns family ids as follow-up handles. Use the returned `read_plan`; if line-numbered `source_spans` are included, treat those spans as already read. Read files directly only for spans marked missing, stale, UNKNOWN, omitted, or required before editing outside the shown range.\n\
\n\
Use `show_family` only with an exact family id returned earlier; use compact mode first and do not request `include_source_spans` by default. Stop and fall back to normal Read/Grep on UNKNOWN, FALLBACK, stale, omitted, or insufficient results. Do not run `repogrammar stats` in normal agent loops. Do not silently initialize repositories; run init/resync/autosync only when user or project policy permits repo-local analysis state.\n\
\n\
If no `.repogrammar/` exists, skip RepoGrammar for that repository.\n\
{MANAGED_INSTRUCTION_END}"
    )
}

/// The environment-variable name that overrides a target's instruction-file path.
pub fn instruction_env_var(target: AgentTarget) -> String {
    format!(
        "REPOGRAMMAR_INSTRUCTION_FILE_{}",
        target.as_str().to_ascii_uppercase().replace('-', "_")
    )
}

/// Resolve a target's instruction-file path from an environment override. Returns
/// `None` (deferred) unless the override resolves to an absolute path, because
/// RepoGrammar must not guess real Codex/Claude instruction-file locations.
pub fn resolve_instruction_file<F>(target: AgentTarget, lookup: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    let raw = lookup(&instruction_env_var(target))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if Path::new(trimmed).is_absolute() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn malformed_managed_section_error() -> RepoGrammarError {
    RepoGrammarError::InvalidInput(
        "instruction file has a malformed RepoGrammar managed section; refusing to modify"
            .to_string(),
    )
}

/// Locate the byte span of a complete managed section, refusing malformed or
/// partial markers. Returns `Ok(None)` when no markers exist, `Ok(Some(span))`
/// for a single well-ordered section, and an error for any other arrangement.
fn managed_instruction_span(contents: &str) -> Result<Option<(usize, usize)>, RepoGrammarError> {
    let begin_count = contents.matches(MANAGED_INSTRUCTION_BEGIN).count();
    let end_count = contents.matches(MANAGED_INSTRUCTION_END).count();
    match (begin_count, end_count) {
        (0, 0) => Ok(None),
        (1, 1) => {
            let begin = contents
                .find(MANAGED_INSTRUCTION_BEGIN)
                .ok_or_else(malformed_managed_section_error)?;
            let end_marker = contents
                .find(MANAGED_INSTRUCTION_END)
                .ok_or_else(malformed_managed_section_error)?;
            if end_marker <= begin {
                return Err(malformed_managed_section_error());
            }
            let line_start = contents[..begin]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0);
            let after_end = end_marker + MANAGED_INSTRUCTION_END.len();
            let line_end = contents[after_end..]
                .find('\n')
                .map(|index| after_end + index + 1)
                .unwrap_or(contents.len());
            Ok(Some((line_start, line_end)))
        }
        _ => Err(malformed_managed_section_error()),
    }
}

fn instruction_temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".repogrammar-managed.tmp");
    path.with_file_name(name)
}

fn atomic_write_instruction(path: &Path, contents: &str) -> Result<(), RepoGrammarError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                RepoGrammarError::InvalidInput(format!(
                    "failed to create instruction file directory: {error}"
                ))
            })?;
        }
    }
    let temporary = instruction_temp_path(path);
    fs::write(&temporary, contents).map_err(|error| {
        RepoGrammarError::InvalidInput(format!(
            "failed to write temporary instruction file: {error}"
        ))
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        RepoGrammarError::InvalidInput(format!(
            "failed to atomically write instruction file: {error}"
        ))
    })
}

fn require_regular_instruction_file(path: &Path) -> Result<(), RepoGrammarError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RepoGrammarError::InvalidInput(
                "instruction file path must be a regular file".to_string(),
            ));
        }
    }
    Ok(())
}

/// Idempotently write the managed instruction section into `path` using atomic
/// temp-file replacement and re-read verification. Preserves unrelated content,
/// refuses malformed partial markers, and reports the action taken.
pub fn write_managed_instruction_section(
    path: &Path,
) -> Result<InstructionAction, RepoGrammarError> {
    require_regular_instruction_file(path)?;
    let existed = path.is_file();
    let existing = if existed {
        fs::read_to_string(path).map_err(|error| {
            RepoGrammarError::InvalidInput(format!("failed to read instruction file: {error}"))
        })?
    } else {
        String::new()
    };
    let block = managed_instruction_block();
    let (next, action) = match managed_instruction_span(&existing)? {
        Some((start, end)) => {
            let mut next = String::with_capacity(existing.len());
            next.push_str(&existing[..start]);
            next.push_str(&block);
            next.push('\n');
            next.push_str(&existing[end..]);
            if next == existing {
                return Ok(InstructionAction::Unchanged);
            }
            (next, InstructionAction::Replaced)
        }
        None => {
            if !existed {
                (format!("{block}\n"), InstructionAction::Created)
            } else if existing.is_empty() {
                (format!("{block}\n"), InstructionAction::Appended)
            } else {
                let mut next = existing.clone();
                if !next.ends_with('\n') {
                    next.push('\n');
                }
                next.push('\n');
                next.push_str(&block);
                next.push('\n');
                (next, InstructionAction::Appended)
            }
        }
    };
    atomic_write_instruction(path, &next)?;
    verify_managed_instruction_present(path, &block)?;
    Ok(action)
}

fn verify_managed_instruction_present(path: &Path, block: &str) -> Result<(), RepoGrammarError> {
    let written = fs::read_to_string(path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!(
            "failed to re-read instruction file for verification: {error}"
        ))
    })?;
    match managed_instruction_span(&written)? {
        Some((start, end)) if written[start..end].trim_end_matches('\n') == block => Ok(()),
        _ => Err(RepoGrammarError::InvalidInput(
            "instruction file managed section failed verification after write".to_string(),
        )),
    }
}

fn tidy_after_removal(text: &str) -> String {
    let mut collapsed = text.to_string();
    while collapsed.contains("\n\n\n") {
        collapsed = collapsed.replace("\n\n\n", "\n\n");
    }
    let trimmed = collapsed.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Remove only the RepoGrammar managed section from `path`, preserving all other
/// content. Refuses malformed partial markers and verifies removal by re-read.
pub fn remove_managed_instruction_section(
    path: &Path,
) -> Result<InstructionAction, RepoGrammarError> {
    if !path.exists() {
        return Ok(InstructionAction::NotPresent);
    }
    require_regular_instruction_file(path)?;
    let existing = fs::read_to_string(path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to read instruction file: {error}"))
    })?;
    let Some((start, end)) = managed_instruction_span(&existing)? else {
        return Ok(InstructionAction::NotPresent);
    };
    let mut next = String::with_capacity(existing.len());
    next.push_str(&existing[..start]);
    next.push_str(&existing[end..]);
    let cleaned = tidy_after_removal(&next);
    atomic_write_instruction(path, &cleaned)?;
    let written = fs::read_to_string(path).map_err(|error| {
        RepoGrammarError::InvalidInput(format!(
            "failed to re-read instruction file for verification: {error}"
        ))
    })?;
    if managed_instruction_span(&written)?.is_some() {
        return Err(RepoGrammarError::InvalidInput(
            "instruction file managed section was not removed".to_string(),
        ));
    }
    Ok(InstructionAction::Removed)
}

/// Reverse a managed instruction write recorded with `original_action`. The
/// managed section is always stripped; the file itself is deleted only when
/// RepoGrammar created it from scratch and removing the section leaves it empty,
/// so a created-from-nothing instruction file is not left behind as an empty
/// artifact after rollback or uninstall. Files that pre-existed the install
/// (`Appended`/`Replaced`), or that the user added content to after creation,
/// keep their remaining content and are never deleted.
fn revert_managed_instruction(
    path: &Path,
    original_action: InstructionAction,
) -> Result<(), RepoGrammarError> {
    remove_managed_instruction_section(path)?;
    if original_action == InstructionAction::Created && instruction_file_is_empty(path) {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(RepoGrammarError::InvalidInput(format!(
                    "failed to remove RepoGrammar-created instruction file: {error}"
                )));
            }
        }
    }
    Ok(())
}

fn instruction_file_is_empty(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.len() == 0)
        .unwrap_or(false)
}

fn instruction_file_for(context: &InstallExecutionContext, target: AgentTarget) -> Option<&str> {
    context
        .instruction_files
        .iter()
        .find(|(candidate, _)| *candidate == target)
        .map(|(_, path)| path.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Write;

    #[test]
    fn target_and_scope_parsing_is_explicit() {
        assert_eq!(AgentTarget::parse("codex"), Ok(AgentTarget::Codex));
        assert_eq!(AgentTarget::parse("auto"), Ok(AgentTarget::AllSupported));
        assert_eq!(AgentTarget::parse("all"), Ok(AgentTarget::AllSupported));
        assert_eq!(AgentTarget::parse("none"), Ok(AgentTarget::None));
        assert_eq!(AgentTarget::parse("claude"), Ok(AgentTarget::ClaudeCode));
        assert_eq!(
            AgentTarget::parse("claude-code"),
            Ok(AgentTarget::ClaudeCode)
        );
        assert_eq!(AgentTarget::parse("cursor"), Ok(AgentTarget::Cursor));
        assert_eq!(AgentTarget::parse("opencode"), Ok(AgentTarget::Opencode));
        assert_eq!(AgentTarget::parse("hermes"), Ok(AgentTarget::Hermes));
        assert_eq!(AgentTarget::parse("gemini"), Ok(AgentTarget::Gemini));
        assert_eq!(
            AgentTarget::parse("antigravity"),
            Ok(AgentTarget::Antigravity)
        );
        assert_eq!(AgentTarget::parse("kiro"), Ok(AgentTarget::Kiro));
        assert_eq!(
            InstallScope::parse("project"),
            Ok(InstallScope::ProjectLocal)
        );
        assert!(AgentTarget::parse("unknown").is_err());
    }

    #[test]
    fn target_registry_marks_live_writers_explicitly() {
        assert_eq!(
            supported_concrete_targets(),
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert_eq!(known_agent_targets().len(), 8);
        assert!(AgentTarget::Codex.has_live_writer(InstallScope::Global));
        assert!(AgentTarget::ClaudeCode.has_live_writer(InstallScope::Global));
        assert!(!AgentTarget::Cursor.has_live_writer(InstallScope::Global));
        assert!(!AgentTarget::Codex.has_live_writer(InstallScope::ProjectLocal));
        assert!(!AgentTarget::Hermes.supports_scope(InstallScope::ProjectLocal));
        assert!(AgentTarget::Gemini.supports_scope(InstallScope::ProjectLocal));
    }

    #[test]
    fn target_adapter_contract_covers_known_targets_and_marks_live_writers() {
        let adapters = known_target_adapters();
        assert_eq!(adapters.len(), 8);
        for adapter in &adapters {
            assert!(!adapter.target_id().is_empty());
            assert!(!adapter.display_name().is_empty());
            let snippet = adapter
                .print_config(InstallScope::Global)
                .expect("config preview");
            assert!(snippet.contains("repogrammar"));
            assert!(snippet.contains("serve"));
            assert!(!snippet.contains(".repogrammar/"));
            assert!(adapter
                .instruction_env_var()
                .starts_with("REPOGRAMMAR_INSTRUCTION_FILE_"));
        }
        let live: Vec<&'static str> = adapters
            .iter()
            .filter(|adapter| adapter.has_live_writer(InstallScope::Global))
            .map(|adapter| adapter.target_id())
            .collect();
        assert_eq!(live, vec!["codex", "claude-code"]);
        assert!(!target_adapter(AgentTarget::Codex).has_live_writer(InstallScope::ProjectLocal));
    }

    #[test]
    fn target_adapter_describe_paths_defers_instruction_without_override() {
        let lookup_none = |_: &str| None;
        let codex = target_adapter(AgentTarget::Codex);
        let described = codex.describe_paths(InstallScope::Global, &lookup_none);
        assert_eq!(described.len(), 2);
        assert!(described[0].starts_with("native_mcp: codex mcp add"));
        assert!(described[1].contains("instruction: deferred"));
        assert!(described[1].contains("REPOGRAMMAR_INSTRUCTION_FILE_CODEX"));

        let absolute = if cfg!(windows) {
            "C:\\agents\\AGENTS.md"
        } else {
            "/srv/agents/AGENTS.md"
        };
        let key = instruction_env_var(AgentTarget::Codex);
        let lookup_override = |requested: &str| (requested == key).then(|| absolute.to_string());
        let with_override = codex.instruction_plan_line(InstallScope::Global, &lookup_override);
        assert!(with_override.contains("managed section -> "));
        assert!(with_override.contains(absolute));
    }

    #[test]
    fn config_snippets_cover_known_targets_without_writes() {
        for target in known_agent_targets() {
            let snippet =
                target_config_snippet(target, InstallScope::Global).expect("config snippet");
            assert!(snippet.contains("repogrammar"));
            assert!(snippet.contains("serve"));
            assert!(!snippet.contains(".repogrammar/"));
        }
        let unsupported =
            target_config_snippet(AgentTarget::Codex, InstallScope::ProjectLocal).expect("snippet");
        assert!(unsupported.contains("does not support project-local scope"));
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

    #[test]
    fn install_requires_yes_and_self_test_before_native_config_write() {
        let workspace = TempInstallWorkspace::new("requires-yes");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("missing yes");

        assert!(error.to_string().contains("--yes"));
        assert_eq!(self_test.calls.borrow().len(), 0);
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn live_all_target_installs_supported_agents_transactionally() {
        let workspace = TempInstallWorkspace::new("all-target-transaction");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("all target install");

        assert_eq!(
            outcome.configured_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert_eq!(self_test.calls.borrow().len(), 2);
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].target, AgentTarget::Codex);
        assert_eq!(actions[1].target, AgentTarget::ClaudeCode);
    }

    #[test]
    fn live_project_local_scope_is_deferred_to_avoid_project_config_guessing() {
        let workspace = TempInstallWorkspace::new("project-local-deferred");
        let request = InstallRequest {
            target: AgentTarget::ClaudeCode,
            scope: InstallScope::ProjectLocal,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("project scope deferred");

        assert!(error.to_string().contains("project-local"));
        assert_eq!(self_test.calls.borrow().len(), 0);
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn receipt_write_failure_rolls_back_native_add() {
        let workspace = TempInstallWorkspace::new("receipt-failure-rollback");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        fs::write(workspace.data_dir.join("install"), "file-not-directory")
            .expect("block receipt dir");
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("receipt write failure");

        assert!(error.to_string().contains("install receipt"));
        assert!(error.to_string().contains("rolled back"));
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].args[1], "add");
        assert_eq!(actions[1].args[1], "remove");
    }

    #[test]
    fn second_native_add_failure_rolls_back_first_target_and_command() {
        let workspace = TempInstallWorkspace::new("second-add-failure");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator {
            fail_add_target: Some(AgentTarget::ClaudeCode),
            ..FakeConfigurator::default()
        };
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("second add failure");

        assert!(error.to_string().contains("rolled back"));
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].target, AgentTarget::Codex);
        assert_eq!(actions[1].target, AgentTarget::ClaudeCode);
        assert_eq!(actions[2].target, AgentTarget::Codex);
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn install_writes_receipt_after_self_test_and_native_config() {
        let workspace = TempInstallWorkspace::new("install-receipt");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("install");

        assert_eq!(outcome.configured_targets, vec![AgentTarget::Codex]);
        assert!(outcome.reconfigured_targets.is_empty());
        assert_eq!(self_test.calls.borrow().len(), 2);
        assert_eq!(configurator.actions.borrow().len(), 1);
        let receipt = fs::read_to_string(&outcome.receipt_paths[0]).expect("receipt");
        let value: serde_json::Value = serde_json::from_str(&receipt).expect("receipt JSON");
        assert_eq!(value["managed_by"], "repogrammar");
        assert_eq!(value["target"], "codex");
        assert_eq!(value["telemetry_enabled"], false);
        assert!(workspace.command_path().exists());
    }

    #[test]
    fn install_refuses_native_entry_without_owned_receipt_before_any_write() {
        let workspace = TempInstallWorkspace::new("foreign-native-entry");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        configurator.set_native_present(AgentTarget::Codex, "/foreign/repogrammar");
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("foreign native entry must be preserved");

        assert!(error.to_string().contains("refusing to overwrite foreign"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn install_preserves_malformed_native_entry_before_any_write() {
        let workspace = TempInstallWorkspace::new("malformed-native-entry");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        configurator.set_native_state(AgentTarget::Codex, NativeMcpServerState::Malformed);
        let self_test = FakeSelfTest::default();

        assert_eq!(
            inspect_agent_integration(
                &workspace.context,
                AgentTarget::Codex,
                InstallScope::Global,
                &configurator,
            )
            .expect("inspection"),
            AgentIntegrationInspection::Malformed
        );
        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("malformed native entry must be preserved");

        assert!(error.to_string().contains("malformed or unsupported"));
        assert!(error.to_string().contains("refusing automatic repair"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn owned_receipt_with_missing_native_entry_blocks_automatic_repair() {
        let workspace = TempInstallWorkspace::new("owned-missing-native-entry");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        configurator.set_native_state(AgentTarget::Codex, NativeMcpServerState::NotFound);
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("receipt/native drift must require explicit repair");

        assert!(error
            .to_string()
            .contains("does not match native MCP state"));
        assert!(error.to_string().contains("refusing automatic repair"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert!(
            receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).is_file()
        );
        assert!(workspace.command_path().is_file());
    }

    #[test]
    fn owned_receipt_with_repointed_native_entry_blocks_automatic_repair() {
        let workspace = TempInstallWorkspace::new("owned-repointed-native-entry");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        configurator.set_native_present(AgentTarget::Codex, "/foreign/repogrammar");
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("receipt/native command drift must require explicit repair");

        assert!(error
            .to_string()
            .contains("does not match native MCP state"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert!(
            receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).is_file()
        );
        assert!(workspace.command_path().is_file());
    }

    #[test]
    fn owned_receipt_with_relative_executable_is_malformed_and_preserved() {
        let workspace = TempInstallWorkspace::new("owned-relative-executable");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        let receipt = receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global);
        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&receipt).expect("receipt"))
                .expect("receipt JSON");
        value["executable_path"] = serde_json::json!("relative/repogrammar");
        fs::write(&receipt, format!("{value}\n")).expect("rewrite malformed receipt");
        configurator.set_native_present(AgentTarget::Codex, "relative/repogrammar");
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();
        let receipt_before = fs::read(&receipt).expect("receipt before");

        assert_eq!(
            inspect_agent_integration(
                &workspace.context,
                AgentTarget::Codex,
                InstallScope::Global,
                &configurator,
            )
            .expect("inspection"),
            AgentIntegrationInspection::Malformed
        );
        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("malformed owned integration must not be rewritten");

        assert!(error.to_string().contains("malformed or unsupported"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert_eq!(
            fs::read(&receipt).expect("preserved receipt"),
            receipt_before
        );
        assert_eq!(
            configurator.native_state(AgentTarget::Codex),
            NativeMcpServerState::Present(NativeMcpServerConfig {
                executable_path: "relative/repogrammar".to_string(),
                args: vec!["serve".to_string()],
                scope: InstallScope::Global,
                enabled: true,
            })
        );
    }

    #[test]
    fn native_probe_failure_is_not_treated_as_absent_or_ready() {
        let workspace = TempInstallWorkspace::new("native-probe-failure");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator {
            fail_probe_target: Some(AgentTarget::Codex),
            ..FakeConfigurator::default()
        };
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("probe failure must block writes");

        assert!(error.to_string().contains("native MCP probe failed"));
        assert!(configurator.actions.borrow().is_empty());
        assert!(self_test.calls.borrow().is_empty());
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn missing_native_entry_after_add_rolls_back_owned_changes() {
        let workspace = TempInstallWorkspace::new("post-add-native-missing");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator {
            suppress_add_state_target: Some(AgentTarget::Codex),
            ..FakeConfigurator::default()
        };
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("post-add native verification must fail");

        assert!(error
            .to_string()
            .contains("not present after configuration"));
        assert!(error.to_string().contains("rolled back"));
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].args[1], "add");
        assert_eq!(
            configurator.native_state(AgentTarget::Codex),
            NativeMcpServerState::NotFound,
            "rollback must not issue a blind remove when post-inspection is already absent"
        );
        assert_eq!(self_test.calls.borrow().len(), 1);
        assert!(
            !receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).exists()
        );
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn final_product_self_test_failure_rolls_back_verified_native_entry() {
        let workspace = TempInstallWorkspace::new("final-product-self-test-failure");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest {
            fail_on_call: Some(2),
            ..FakeSelfTest::default()
        };

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("final product self-test must gate success");

        assert!(error.to_string().contains("MCP self-test failed"));
        assert!(error.to_string().contains("rolled back"));
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].args[1], "add");
        assert_eq!(actions[1].args[1], "remove");
        assert_eq!(self_test.calls.borrow().len(), 2);
        assert!(
            !receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).exists()
        );
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn already_managed_install_still_repairs_missing_command_without_native_writes() {
        let workspace = TempInstallWorkspace::new("already-managed-command-repair");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        fs::remove_file(workspace.command_path()).expect("remove command path");
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("repair command");

        assert!(outcome.configured_targets.is_empty());
        assert_eq!(
            outcome.skipped_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        let command_path = workspace.command_path_str();
        assert_eq!(outcome.command_path.as_deref(), Some(command_path.as_str()));
        assert!(workspace.command_path().exists());
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert_eq!(self_test.calls.borrow().len(), 1);
    }

    #[test]
    fn already_managed_install_refreshes_managed_command_copy_without_native_writes() {
        let workspace = TempInstallWorkspace::new("already-managed-command-copy-refresh");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::remove_file(workspace.command_path()).expect("remove command symlink");
        fs::copy(&installed, workspace.command_path()).expect("managed command copy");
        fs::write(&workspace.context.executable_path, "updated stub\n").expect("update source");
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("refresh managed copy");

        assert!(outcome.configured_targets.is_empty());
        assert_eq!(
            outcome.skipped_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert_eq!(self_test.calls.borrow().len(), 1);
        assert_eq!(
            fs::read_to_string(workspace.command_path()).expect("command copy"),
            "updated stub\n"
        );
    }

    #[test]
    fn install_skips_replacing_installed_executable_that_is_current_process() {
        let workspace = TempInstallWorkspace::new("installed-is-current-exe");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(installed.parent().expect("install bin")).expect("install bin");
        fs::write(&installed, "running binary\n").expect("installed executable");
        // The configured source differs from the managed installed executable,
        // but the running process IS that installed executable.
        fs::write(&workspace.context.executable_path, "new source\n").expect("source");

        let record =
            install_cli_command_with_current_process(&workspace.context, Some(installed.as_path()))
                .expect("install must not fail when installed executable is the running process");

        assert!(!record.created_executable);
        assert!(record.previous_executable.is_none());
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable untouched"),
            "running binary\n",
            "must not overwrite the installed executable that is the current process"
        );
    }

    #[test]
    fn same_path_falls_back_to_lexical_when_canonicalize_unavailable() {
        // Non-existent absolute paths cannot be canonicalized; identical lexical
        // forms must still match, and distinct ones must not.
        let path = Path::new("/repogrammar/does/not/exist/bin/repogrammar");
        assert!(same_path(path, path));
        let other = Path::new("/repogrammar/does/not/exist/bin/other");
        assert!(!same_path(path, other));
    }

    #[test]
    fn drifted_managed_install_reconfigures_to_authority() {
        let workspace = TempInstallWorkspace::new("drifted-managed-reconfigure");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");

        // Simulate a previous-layout install: rewrite each receipt's recorded
        // executable to a stale path that no longer matches the authority.
        for target in [AgentTarget::Codex, AgentTarget::ClaudeCode] {
            let path = receipt_path(&workspace.context, target, InstallScope::Global);
            let mut value: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&path).expect("receipt"))
                    .expect("receipt json");
            value["executable_path"] = serde_json::json!("/stale/previous/bin/repogrammar");
            fs::write(&path, format!("{value}\n")).expect("rewrite receipt");
            configurator.set_native_present(target, "/stale/previous/bin/repogrammar");
        }
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();

        assert_eq!(
            inspect_agent_integration(
                &workspace.context,
                AgentTarget::Codex,
                InstallScope::Global,
                &configurator,
            )
            .expect("inspect obsolete owned integration"),
            AgentIntegrationInspection::OwnedOutdated
        );

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("reconcile install");

        assert!(outcome.configured_targets.is_empty());
        assert_eq!(
            outcome.reconfigured_targets,
            vec![AgentTarget::Codex, AgentTarget::ClaudeCode]
        );
        assert!(outcome.skipped_targets.is_empty());
        let actions = configurator.actions.borrow();
        let removes = actions
            .iter()
            .filter(|action| action.args.get(1).map(String::as_str) == Some("remove"))
            .count();
        let adds = actions
            .iter()
            .filter(|action| action.args.get(1).map(String::as_str) == Some("add"))
            .count();
        assert_eq!(removes, 2, "{actions:?}");
        assert_eq!(adds, 2, "{actions:?}");
        drop(actions);
        assert_eq!(self_test.calls.borrow().len(), 2);
        // Receipts now record the authority again, so a follow-up install skips.
        let authority = workspace.data_dir.join("bin").join(binary_name());
        for target in [AgentTarget::Codex, AgentTarget::ClaudeCode] {
            let path = receipt_path(&workspace.context, target, InstallScope::Global);
            let recorded = receipt_executable_path(&path).expect("recorded executable");
            assert!(same_path(Path::new(&recorded), &authority), "{recorded}");
        }
        assert_eq!(
            inspect_agent_integration(
                &workspace.context,
                AgentTarget::Codex,
                InstallScope::Global,
                &configurator,
            )
            .expect("inspect refreshed integration"),
            AgentIntegrationInspection::OwnedCurrent
        );
    }

    #[test]
    fn failed_owned_reconfiguration_restores_native_receipt_and_instruction_state() {
        let workspace = TempInstallWorkspace::new("owned-reconfiguration-transaction");
        let instructions = TempDir::new("owned-reconfiguration-instructions");
        let instruction_path = instructions.file("AGENTS.md");
        fs::write(&instruction_path, "# User instructions\n\nkeep me\n")
            .expect("seed instruction file");
        let mut context = workspace.context.clone();
        context.instruction_files =
            vec![(AgentTarget::Codex, instruction_path.display().to_string())];
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &context, &configurator, &self_test).expect("initial install");

        let receipt_path = receipt_path(&context, AgentTarget::Codex, InstallScope::Global);
        let mut receipt_value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&receipt_path).expect("receipt"))
                .expect("receipt JSON");
        receipt_value["executable_path"] = serde_json::json!("/stale/previous/bin/repogrammar");
        fs::write(&receipt_path, format!("{receipt_value}\n")).expect("rewrite old receipt");
        let receipt_backup = receipt_path.with_extension("json.bak");
        fs::write(&receipt_backup, "pre-existing receipt backup\n").expect("seed receipt backup");
        configurator.set_native_present(AgentTarget::Codex, "/stale/previous/bin/repogrammar");
        configurator.actions.borrow_mut().clear();

        let receipt_before = fs::read(&receipt_path).expect("receipt before");
        let backup_before = fs::read(&receipt_backup).expect("backup before");
        let instruction_before = fs::read(&instruction_path).expect("instruction before");
        let failing_self_test = FakeSelfTest {
            fail_on_call: Some(2),
            ..FakeSelfTest::default()
        };

        let error = execute_install(&request, &context, &configurator, &failing_self_test)
            .expect_err("final self-test failure must restore the owned integration");

        assert!(error.to_string().contains("rolled back"), "{error}");
        assert_eq!(
            fs::read(&receipt_path).expect("restored receipt"),
            receipt_before
        );
        assert_eq!(
            fs::read(&receipt_backup).expect("restored receipt backup"),
            backup_before
        );
        assert_eq!(
            fs::read(&instruction_path).expect("restored instruction"),
            instruction_before
        );
        assert_eq!(
            configurator.native_state(AgentTarget::Codex),
            NativeMcpServerState::Present(NativeMcpServerConfig {
                executable_path: "/stale/previous/bin/repogrammar".to_string(),
                args: vec!["serve".to_string()],
                scope: InstallScope::Global,
                enabled: true,
            })
        );
        assert_eq!(
            inspect_agent_integration(
                &context,
                AgentTarget::Codex,
                InstallScope::Global,
                &configurator,
            )
            .expect("inspect restored integration"),
            AgentIntegrationInspection::OwnedOutdated
        );
    }

    #[test]
    fn rollback_preserves_foreign_native_entry_injected_after_reconfiguration() {
        let workspace = TempInstallWorkspace::new("owned-reconfiguration-foreign-race");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");

        let receipt = receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global);
        let mut receipt_value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&receipt).expect("receipt"))
                .expect("receipt JSON");
        let old_executable = "/stale/previous/bin/repogrammar";
        receipt_value["executable_path"] = serde_json::json!(old_executable);
        fs::write(&receipt, format!("{receipt_value}\n")).expect("rewrite old receipt");
        configurator.set_native_present(AgentTarget::Codex, old_executable);
        configurator.actions.borrow_mut().clear();

        let old_state = NativeMcpServerState::Present(NativeMcpServerConfig {
            executable_path: old_executable.to_string(),
            args: vec!["serve".to_string()],
            scope: InstallScope::Global,
            enabled: true,
        });
        let authority = managed_executable_path(&workspace.context);
        let replacement_state = NativeMcpServerState::Present(NativeMcpServerConfig {
            executable_path: authority,
            args: vec!["serve".to_string()],
            scope: InstallScope::Global,
            enabled: true,
        });
        let foreign_state = NativeMcpServerState::Present(NativeMcpServerConfig {
            executable_path: "/foreign/concurrent/repogrammar".to_string(),
            args: vec!["serve".to_string()],
            scope: InstallScope::Global,
            enabled: true,
        });
        configurator.script_inspections([
            (AgentTarget::Codex, old_state.clone()),
            (AgentTarget::Codex, old_state),
            (AgentTarget::Codex, replacement_state),
            (AgentTarget::Codex, foreign_state.clone()),
        ]);
        let failing_self_test = FakeSelfTest {
            fail_on_call: Some(2),
            ..FakeSelfTest::default()
        };

        let error = execute_install(
            &request,
            &workspace.context,
            &configurator,
            &failing_self_test,
        )
        .expect_err("foreign race must block destructive rollback");

        assert!(error.to_string().contains("rollback attempted but failed"));
        assert!(error.to_string().contains("changed outside this install"));
        assert_eq!(configurator.native_state(AgentTarget::Codex), foreign_state);
        let actions = configurator.actions.borrow();
        assert_eq!(
            actions.len(),
            2,
            "rollback must not remove the foreign entry"
        );
        assert_eq!(actions[0].args[1], "remove");
        assert_eq!(actions[1].args[1], "add");
    }

    #[test]
    fn failed_refresh_self_test_restores_existing_managed_binary_and_command_copy() {
        let workspace = TempInstallWorkspace::new("refresh-self-test-rollback");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("initial install");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::remove_file(workspace.command_path()).expect("remove command symlink");
        fs::copy(&installed, workspace.command_path()).expect("managed command copy");
        fs::write(&workspace.context.executable_path, "updated broken stub\n")
            .expect("update source");
        configurator.actions.borrow_mut().clear();
        self_test.calls.borrow_mut().clear();
        let failing_self_test = FakeSelfTest {
            fail: true,
            ..FakeSelfTest::default()
        };

        let error = execute_install(
            &request,
            &workspace.context,
            &configurator,
            &failing_self_test,
        )
        .expect_err("failed self-test must roll back refresh");

        assert!(error.to_string().contains("rolled back"));
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert_eq!(failing_self_test.calls.borrow().len(), 1);
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable restored"),
            "stub\n"
        );
        assert_eq!(
            fs::read_to_string(workspace.command_path()).expect("command copy restored"),
            "stub\n"
        );
    }

    #[test]
    fn failed_refresh_native_add_restores_existing_managed_binary_and_command_copy() {
        let workspace = TempInstallWorkspace::new("refresh-native-rollback");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(installed.parent().expect("install bin")).expect("install bin");
        fs::write(&installed, "old managed stub\n").expect("old installed executable");
        fs::copy(&installed, workspace.command_path()).expect("old managed command copy");
        fs::write(&workspace.context.executable_path, "new broken stub\n").expect("new source");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator {
            fail_add_target: Some(AgentTarget::ClaudeCode),
            ..FakeConfigurator::default()
        };
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("failed native add must roll back refresh");

        assert!(error.to_string().contains("rolled back"));
        assert_eq!(self_test.calls.borrow().len(), 1);
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].target, AgentTarget::Codex);
        assert_eq!(actions[1].target, AgentTarget::ClaudeCode);
        assert_eq!(actions[2].target, AgentTarget::Codex);
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable restored"),
            "old managed stub\n"
        );
        assert_eq!(
            fs::read_to_string(workspace.command_path()).expect("command copy restored"),
            "old managed stub\n"
        );
    }

    #[test]
    fn managed_file_replacement_refuses_when_previous_file_cannot_be_removed() {
        let workspace = TempInstallWorkspace::new("managed-file-remove-refusal");
        let destination = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(&destination).expect("directory occupying managed path");

        let error = replace_managed_file(
            Path::new(&workspace.context.executable_path),
            &destination,
            "installed RepoGrammar CLI",
        )
        .expect_err("managed replacement must remove the previous path first");

        let message = error.to_string();
        assert!(message.contains("failed to remove previous installed RepoGrammar CLI"));
        assert!(message.contains("exit any running coding agent sessions"));
        assert!(message.contains("rerun the install or build command"));
        assert!(destination.is_dir());
    }

    #[test]
    fn foreign_existing_command_path_is_refused_before_self_test_or_native_writes() {
        let workspace = TempInstallWorkspace::new("foreign-command-refused");
        fs::write(workspace.command_path(), "foreign command\n").expect("foreign command");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("foreign command must be refused");

        assert!(error.to_string().contains("not managed by RepoGrammar"));
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert_eq!(self_test.calls.borrow().len(), 0);
        assert!(!workspace.data_dir.join("bin").join(binary_name()).exists());
    }

    #[test]
    fn current_executable_command_path_is_allowed_for_source_installs() {
        let workspace = TempInstallWorkspace::new("current-executable-command-path");
        let source_command = workspace.command_path();
        fs::copy(&workspace.context.executable_path, &source_command)
            .expect("current executable on PATH");
        let mut context = workspace.context.clone();
        context.executable_path = source_command.display().to_string();
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let outcome =
            execute_install(&request, &context, &configurator, &self_test).expect("install");

        let installed = workspace.data_dir.join("bin").join(binary_name());
        assert!(installed.exists());
        assert_eq!(
            outcome.installed_executable_path.as_deref(),
            Some(installed.display().to_string().as_str())
        );
        assert_eq!(
            outcome.command_path.as_deref(),
            Some(context.executable_path.as_str())
        );
        assert_eq!(
            self_test.calls.borrow().as_slice(),
            &[
                installed.display().to_string(),
                installed.display().to_string(),
            ]
        );
        assert_eq!(configurator.actions.borrow().len(), 1);
    }

    #[test]
    fn current_executable_managed_command_copy_is_not_refreshed_in_place() {
        let workspace = TempInstallWorkspace::new("current-executable-managed-command-copy");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(installed.parent().expect("install bin")).expect("install bin");
        fs::copy(&workspace.context.executable_path, &installed).expect("installed executable");
        fs::copy(&installed, workspace.command_path()).expect("managed command copy");
        let mut context = workspace.context.clone();
        context.executable_path = workspace.command_path_str();

        let record = install_cli_command(&context).expect("install command");

        assert!(!record.created_command);
        assert!(
            record.previous_command_copy.is_none(),
            "current executable command path must not be overwritten or restored in the same run"
        );
        assert_eq!(record.command_path, context.executable_path);
        assert_eq!(
            fs::read_to_string(workspace.command_path()).expect("command copy"),
            "stub\n"
        );
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable"),
            "stub\n"
        );
    }

    #[test]
    fn running_managed_command_copy_is_not_refreshed_when_configured_source_differs() {
        let workspace = TempInstallWorkspace::new("running-managed-command-copy");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(installed.parent().expect("install bin")).expect("install bin");
        fs::write(&installed, "managed stub\n").expect("installed executable");
        fs::copy(&installed, workspace.command_path()).expect("managed command copy");
        let mut context = workspace.context.clone();
        context.executable_path = installed.display().to_string();
        let current_process_executable = workspace.command_path();

        let record =
            install_cli_command_with_current_process(&context, Some(&current_process_executable))
                .expect("install command");

        assert!(!record.created_command);
        assert!(
            record.previous_command_copy.is_none(),
            "running command copy must not be overwritten during the same run"
        );
        assert_eq!(record.command_path, workspace.command_path_str());
        assert_eq!(
            fs::read_to_string(workspace.command_path()).expect("command copy"),
            "managed stub\n"
        );
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable"),
            "managed stub\n"
        );
    }

    #[test]
    fn foreign_existing_command_path_does_not_overwrite_existing_managed_binary() {
        let workspace = TempInstallWorkspace::new("foreign-command-preserves-managed-binary");
        let installed = workspace.data_dir.join("bin").join(binary_name());
        fs::create_dir_all(installed.parent().expect("install bin")).expect("install bin");
        fs::write(&installed, "old managed stub\n").expect("old installed executable");
        fs::write(workspace.command_path(), "foreign command\n").expect("foreign command");
        fs::write(&workspace.context.executable_path, "new source stub\n").expect("new source");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            telemetry_enabled: false,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("foreign command must be refused");

        assert!(error.to_string().contains("not managed by RepoGrammar"));
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert_eq!(self_test.calls.borrow().len(), 0);
        assert_eq!(
            fs::read_to_string(&installed).expect("installed executable preserved"),
            "old managed stub\n"
        );
    }

    #[test]
    fn failed_self_test_prevents_config_write_and_receipt() {
        let workspace = TempInstallWorkspace::new("self-test-failure");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest {
            fail: true,
            ..FakeSelfTest::default()
        };

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("self-test failure");

        assert!(error.to_string().contains("self-test failed"));
        assert_eq!(configurator.actions.borrow().len(), 0);
        assert!(!workspace.context.data_dir.contains(".repogrammar/"));
        assert!(
            !receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).exists()
        );
        assert!(!workspace.command_path().exists());
    }

    #[test]
    fn uninstall_removes_only_receipted_managed_targets() {
        let workspace = TempInstallWorkspace::new("uninstall-receipt");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("install first");
        configurator.actions.borrow_mut().clear();

        let outcome =
            execute_uninstall(&request, &workspace.context, &configurator).expect("uninstall");

        assert_eq!(outcome.configured_targets, vec![AgentTarget::Codex]);
        assert_eq!(configurator.actions.borrow().len(), 1);
        assert!(
            !receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).exists()
        );
    }

    #[test]
    fn uninstall_refuses_foreign_receipt_without_native_remove() {
        let workspace = TempInstallWorkspace::new("foreign-receipt");
        let receipt = receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global);
        fs::create_dir_all(receipt.parent().expect("receipt parent")).expect("receipts dir");
        fs::write(
            receipt,
            r#"{"schema_version":1,"managed_by":"someone-else","target":"codex","scope":"global"}"#,
        )
        .expect("foreign receipt");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();

        let error = execute_uninstall(&request, &workspace.context, &configurator)
            .expect_err("foreign receipt");

        assert!(error.to_string().contains("not owned"));
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn uninstall_refuses_missing_receipt_without_native_remove() {
        let workspace = TempInstallWorkspace::new("missing-receipt");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();

        let error = execute_uninstall(&request, &workspace.context, &configurator)
            .expect_err("missing receipt");

        assert!(error
            .to_string()
            .contains("no RepoGrammar-managed install receipt"));
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn all_target_uninstall_removes_existing_owned_receipts_and_ignores_missing() {
        let workspace = TempInstallWorkspace::new("uninstall-all");
        let install_codex = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(
            &install_codex,
            &workspace.context,
            &configurator,
            &self_test,
        )
        .expect("install codex");
        configurator.actions.borrow_mut().clear();

        let uninstall_all = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let outcome = execute_uninstall(&uninstall_all, &workspace.context, &configurator)
            .expect("uninstall all");

        assert_eq!(outcome.configured_targets, vec![AgentTarget::Codex]);
        assert!(
            !receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global).exists()
        );
        assert_eq!(configurator.actions.borrow().len(), 1);
    }

    #[test]
    fn all_target_uninstall_refuses_when_no_owned_receipts_exist() {
        let workspace = TempInstallWorkspace::new("uninstall-all-missing");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();

        let error = execute_uninstall(&request, &workspace.context, &configurator)
            .expect_err("missing all receipts");

        assert!(error
            .to_string()
            .contains("no RepoGrammar-managed install receipt"));
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn all_target_uninstall_refuses_foreign_receipt_without_native_remove() {
        let workspace = TempInstallWorkspace::new("uninstall-all-foreign");
        let receipt = receipt_path(&workspace.context, AgentTarget::Codex, InstallScope::Global);
        fs::create_dir_all(receipt.parent().expect("receipt parent")).expect("receipts dir");
        fs::write(
            receipt,
            r#"{"schema_version":1,"managed_by":"someone-else","target":"codex","scope":"global"}"#,
        )
        .expect("foreign receipt");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();

        let error = execute_uninstall(&request, &workspace.context, &configurator)
            .expect_err("foreign all receipt");

        assert!(error.to_string().contains("not owned"));
        assert_eq!(configurator.actions.borrow().len(), 0);
    }

    #[test]
    fn resolve_instruction_file_requires_absolute_override() {
        let absolute = if cfg!(windows) {
            "C:\\repogrammar\\AGENTS.md"
        } else {
            "/tmp/repogrammar/AGENTS.md"
        };
        let key = instruction_env_var(AgentTarget::Codex);
        assert_eq!(key, "REPOGRAMMAR_INSTRUCTION_FILE_CODEX");
        assert_eq!(
            instruction_env_var(AgentTarget::ClaudeCode),
            "REPOGRAMMAR_INSTRUCTION_FILE_CLAUDE_CODE"
        );

        let lookup_absolute = |requested: &str| (requested == key).then(|| absolute.to_string());
        assert_eq!(
            resolve_instruction_file(AgentTarget::Codex, &lookup_absolute),
            Some(absolute.to_string())
        );

        let lookup_relative =
            |requested: &str| (requested == key).then(|| "relative/AGENTS.md".to_string());
        assert_eq!(
            resolve_instruction_file(AgentTarget::Codex, &lookup_relative),
            None
        );

        let lookup_missing = |_: &str| None;
        assert_eq!(
            resolve_instruction_file(AgentTarget::Codex, &lookup_missing),
            None
        );
    }

    #[test]
    fn managed_instruction_writer_creates_appends_replaces_and_is_idempotent() {
        let dir = TempDir::new("instruction-lifecycle");

        let create_path = dir.file("CREATE.md");
        assert_eq!(
            write_managed_instruction_section(&create_path).expect("create"),
            InstructionAction::Created
        );
        let created = fs::read_to_string(&create_path).expect("created contents");
        assert!(created.starts_with(MANAGED_INSTRUCTION_BEGIN));
        assert!(created.ends_with(&format!("{MANAGED_INSTRUCTION_END}\n")));
        assert!(created.contains("repogrammar_context"));
        assert!(created.contains("compact mode first"));
        assert!(created.contains("include_source_spans"));
        assert!(created.contains("UNKNOWN, FALLBACK, stale, omitted, or insufficient"));
        assert!(created.contains("repogrammar stats"));
        assert!(created.contains("Do not silently initialize"));

        assert_eq!(
            write_managed_instruction_section(&create_path).expect("idempotent"),
            InstructionAction::Unchanged
        );
        assert_eq!(
            fs::read_to_string(&create_path).expect("unchanged contents"),
            created
        );

        let append_path = dir.file("APPEND.md");
        fs::write(&append_path, "# User guide\n\nkeep this line\n").expect("seed user content");
        assert_eq!(
            write_managed_instruction_section(&append_path).expect("append"),
            InstructionAction::Appended
        );
        let appended = fs::read_to_string(&append_path).expect("appended contents");
        assert!(appended.starts_with("# User guide\n"));
        assert!(appended.contains("keep this line"));
        assert!(appended.contains(MANAGED_INSTRUCTION_BEGIN));

        let replace_path = dir.file("REPLACE.md");
        let seeded = format!(
            "# Top\n\n{MANAGED_INSTRUCTION_BEGIN}\nstale managed body\n{MANAGED_INSTRUCTION_END}\n\n# Bottom\n"
        );
        fs::write(&replace_path, &seeded).expect("seed replace content");
        assert_eq!(
            write_managed_instruction_section(&replace_path).expect("replace"),
            InstructionAction::Replaced
        );
        let replaced = fs::read_to_string(&replace_path).expect("replaced contents");
        assert!(replaced.contains("# Top"));
        assert!(replaced.contains("# Bottom"));
        assert!(replaced.contains("repogrammar_context"));
        assert!(replaced.contains("compact mode first"));
        assert!(!replaced.contains("stale managed body"));
        assert_eq!(
            write_managed_instruction_section(&replace_path).expect("replace idempotent"),
            InstructionAction::Unchanged
        );
    }

    #[test]
    fn managed_instruction_writer_refuses_partial_or_duplicate_markers() {
        let dir = TempDir::new("instruction-malformed");

        let only_begin = dir.file("ONLY_BEGIN.md");
        fs::write(
            &only_begin,
            format!("# Guide\n{MANAGED_INSTRUCTION_BEGIN}\nbody without end\n"),
        )
        .expect("seed partial begin");
        let error = write_managed_instruction_section(&only_begin).expect_err("partial begin");
        assert!(error.to_string().contains("malformed"));
        assert!(fs::read_to_string(&only_begin)
            .expect("unchanged")
            .contains("body without end"));

        let only_end = dir.file("ONLY_END.md");
        fs::write(&only_end, format!("# Guide\n{MANAGED_INSTRUCTION_END}\n")).expect("seed end");
        assert!(write_managed_instruction_section(&only_end)
            .expect_err("partial end")
            .to_string()
            .contains("malformed"));

        let duplicate = dir.file("DUPLICATE.md");
        fs::write(
            &duplicate,
            format!(
                "{MANAGED_INSTRUCTION_BEGIN}\nfirst\n{MANAGED_INSTRUCTION_BEGIN}\nsecond\n{MANAGED_INSTRUCTION_END}\n"
            ),
        )
        .expect("seed duplicate begin");
        assert!(write_managed_instruction_section(&duplicate)
            .expect_err("duplicate begin")
            .to_string()
            .contains("malformed"));
    }

    #[test]
    fn remove_managed_instruction_section_preserves_user_content() {
        let dir = TempDir::new("instruction-remove");
        let path = dir.file("AGENTS.md");
        fs::write(&path, "# Keep top\n").expect("seed");
        write_managed_instruction_section(&path).expect("append section");
        assert!(fs::read_to_string(&path)
            .expect("with section")
            .contains(MANAGED_INSTRUCTION_BEGIN));

        assert_eq!(
            remove_managed_instruction_section(&path).expect("remove"),
            InstructionAction::Removed
        );
        let after = fs::read_to_string(&path).expect("after removal");
        assert!(after.contains("# Keep top"));
        assert!(!after.contains(MANAGED_INSTRUCTION_BEGIN));
        assert!(!after.contains(MANAGED_INSTRUCTION_END));

        assert_eq!(
            remove_managed_instruction_section(&path).expect("remove twice"),
            InstructionAction::NotPresent
        );
    }

    #[test]
    fn install_writes_managed_instruction_when_resolved_and_records_receipt() {
        let workspace = TempInstallWorkspace::new("instruction-install");
        let instructions = TempDir::new("instruction-install-target");
        let instruction_path = instructions.file("AGENTS.md");
        let mut context = workspace.context.clone();
        context.instruction_files =
            vec![(AgentTarget::Codex, instruction_path.display().to_string())];
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let outcome =
            execute_install(&request, &context, &configurator, &self_test).expect("install");

        let written = fs::read_to_string(&instruction_path).expect("instruction file");
        assert!(written.contains(MANAGED_INSTRUCTION_BEGIN));
        assert!(written.contains(MANAGED_INSTRUCTION_END));
        assert!(written.contains("repogrammar_context"));
        assert!(written.contains("compact mode first"));
        assert!(written.contains("repogrammar stats"));
        let receipt = fs::read_to_string(&outcome.receipt_paths[0]).expect("receipt");
        let value: serde_json::Value = serde_json::from_str(&receipt).expect("receipt JSON");
        assert_eq!(value["instruction_action"], "created");
        assert_eq!(
            value["instruction_file_path"],
            instruction_path.display().to_string()
        );
    }

    #[test]
    fn install_defers_instruction_without_override_and_records_deferred() {
        let workspace = TempInstallWorkspace::new("instruction-deferred");
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let outcome = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect("install");

        let receipt = fs::read_to_string(&outcome.receipt_paths[0]).expect("receipt");
        let value: serde_json::Value = serde_json::from_str(&receipt).expect("receipt JSON");
        assert_eq!(value["instruction_action"], "deferred");
        assert!(value["instruction_file_path"].is_null());
    }

    #[test]
    fn uninstall_removes_managed_instruction_section_recorded_in_receipt() {
        let workspace = TempInstallWorkspace::new("instruction-uninstall");
        let instructions = TempDir::new("instruction-uninstall-target");
        let instruction_path = instructions.file("AGENTS.md");
        fs::write(&instruction_path, "# Existing user guide\n\nkeep me\n").expect("seed");
        let mut context = workspace.context.clone();
        context.instruction_files =
            vec![(AgentTarget::Codex, instruction_path.display().to_string())];
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &context, &configurator, &self_test).expect("install");
        assert!(fs::read_to_string(&instruction_path)
            .expect("after install")
            .contains(MANAGED_INSTRUCTION_BEGIN));
        configurator.actions.borrow_mut().clear();

        execute_uninstall(&request, &context, &configurator).expect("uninstall");

        let after = fs::read_to_string(&instruction_path).expect("after uninstall");
        assert!(!after.contains(MANAGED_INSTRUCTION_BEGIN));
        assert!(!after.contains(MANAGED_INSTRUCTION_END));
        assert!(after.contains("keep me"));
    }

    #[test]
    fn revert_managed_instruction_deletes_repogrammar_created_file() {
        let dir = TempDir::new("instruction-revert-created");
        let path = dir.file("AGENTS.md");
        assert_eq!(
            write_managed_instruction_section(&path).expect("create"),
            InstructionAction::Created
        );
        assert!(path.is_file());

        revert_managed_instruction(&path, InstructionAction::Created).expect("revert created");

        assert!(
            !path.exists(),
            "a RepoGrammar-created instruction file must not linger as an empty artifact"
        );
    }

    #[test]
    fn revert_managed_instruction_keeps_created_file_with_later_user_content() {
        let dir = TempDir::new("instruction-revert-created-edited");
        let path = dir.file("AGENTS.md");
        write_managed_instruction_section(&path).expect("create");
        let with_section = fs::read_to_string(&path).expect("created contents");
        fs::write(&path, format!("{with_section}\n# Added by user later\n"))
            .expect("user edits the created file");

        // The receipt still records `Created`, but the file now carries user
        // content, so reversal must keep the file and only strip the section.
        revert_managed_instruction(&path, InstructionAction::Created).expect("revert created");

        let after = fs::read_to_string(&path).expect("file preserved");
        assert!(after.contains("# Added by user later"));
        assert!(!after.contains(MANAGED_INSTRUCTION_BEGIN));
        assert!(!after.contains(MANAGED_INSTRUCTION_END));
    }

    #[test]
    fn revert_managed_instruction_preserves_preexisting_appended_file() {
        let dir = TempDir::new("instruction-revert-appended");
        let path = dir.file("AGENTS.md");
        fs::write(&path, "# User guide\n\nkeep me\n").expect("seed pre-existing file");
        assert_eq!(
            write_managed_instruction_section(&path).expect("append"),
            InstructionAction::Appended
        );

        revert_managed_instruction(&path, InstructionAction::Appended).expect("revert appended");

        let after = fs::read_to_string(&path).expect("pre-existing file preserved");
        assert!(after.contains("keep me"));
        assert!(!after.contains(MANAGED_INSTRUCTION_BEGIN));
        assert!(!after.contains(MANAGED_INSTRUCTION_END));
    }

    #[test]
    fn uninstall_deletes_repogrammar_created_instruction_file() {
        let workspace = TempInstallWorkspace::new("instruction-uninstall-created");
        let instructions = TempDir::new("instruction-uninstall-created-target");
        let instruction_path = instructions.file("AGENTS.md");
        let mut context = workspace.context.clone();
        context.instruction_files =
            vec![(AgentTarget::Codex, instruction_path.display().to_string())];
        let request = InstallRequest {
            target: AgentTarget::Codex,
            scope: InstallScope::Global,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();
        execute_install(&request, &context, &configurator, &self_test).expect("install");
        assert!(
            instruction_path.is_file(),
            "install must create the resolved instruction file"
        );
        configurator.actions.borrow_mut().clear();

        execute_uninstall(&request, &context, &configurator).expect("uninstall");

        assert!(
            !instruction_path.exists(),
            "uninstall must delete an instruction file RepoGrammar created from scratch"
        );
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "repogrammar-{prefix}-{}-{}",
                std::process::id(),
                unique_suffix()
            ));
            fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct TempInstallWorkspace {
        root: PathBuf,
        data_dir: PathBuf,
        context: InstallExecutionContext,
    }

    impl TempInstallWorkspace {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "repogrammar-install-{prefix}-{}-{}",
                std::process::id(),
                unique_suffix()
            ));
            fs::create_dir_all(&root).expect("workspace");
            let executable = root.join("repogrammar");
            let mut file = fs::File::create(&executable).expect("executable");
            writeln!(file, "stub").expect("write executable");
            let data_dir = root.join("data");
            let command_dir = root.join("commands");
            let current_dir = root.join("project");
            fs::create_dir_all(&data_dir).expect("data dir");
            fs::create_dir_all(&command_dir).expect("command dir");
            fs::create_dir_all(&current_dir).expect("project dir");
            let context = InstallExecutionContext {
                executable_path: executable.display().to_string(),
                command_dir: command_dir.display().to_string(),
                command_dir_on_path: true,
                data_dir: data_dir.display().to_string(),
                current_dir: current_dir.display().to_string(),
                instruction_files: Vec::new(),
            };
            Self {
                root,
                data_dir,
                context,
            }
        }

        fn command_path(&self) -> PathBuf {
            Path::new(&self.context.command_dir).join(binary_name())
        }

        fn command_path_str(&self) -> String {
            self.command_path().display().to_string()
        }
    }

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    }

    impl Drop for TempInstallWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[derive(Default)]
    struct FakeConfigurator {
        actions: RefCell<Vec<NativeAgentAction>>,
        fail_add_target: Option<AgentTarget>,
        fail_probe_target: Option<AgentTarget>,
        suppress_add_state_target: Option<AgentTarget>,
        native_states: RefCell<Vec<(AgentTarget, NativeMcpServerState)>>,
        scripted_inspections: RefCell<Vec<(AgentTarget, NativeMcpServerState)>>,
    }

    impl FakeConfigurator {
        fn set_native_present(&self, target: AgentTarget, executable_path: &str) {
            self.set_native_state(
                target,
                NativeMcpServerState::Present(NativeMcpServerConfig {
                    executable_path: executable_path.to_string(),
                    args: vec!["serve".to_string()],
                    scope: InstallScope::Global,
                    enabled: true,
                }),
            );
        }

        fn set_native_state(&self, target: AgentTarget, state: NativeMcpServerState) {
            let mut states = self.native_states.borrow_mut();
            if let Some((_, current)) = states
                .iter_mut()
                .find(|(candidate, _)| *candidate == target)
            {
                *current = state;
            } else {
                states.push((target, state));
            }
        }

        fn native_state(&self, target: AgentTarget) -> NativeMcpServerState {
            self.native_states
                .borrow()
                .iter()
                .find_map(|(candidate, state)| (*candidate == target).then(|| state.clone()))
                .unwrap_or(NativeMcpServerState::NotFound)
        }

        fn script_inspections(
            &self,
            inspections: impl IntoIterator<Item = (AgentTarget, NativeMcpServerState)>,
        ) {
            self.scripted_inspections.borrow_mut().extend(inspections);
        }
    }

    impl NativeAgentConfigurator for FakeConfigurator {
        fn inspect_mcp_server(
            &self,
            target: AgentTarget,
            _scope: InstallScope,
            _current_dir: &str,
        ) -> Result<NativeMcpServerState, RepoGrammarError> {
            if self.fail_probe_target == Some(target) {
                return Err(RepoGrammarError::InvalidInput(
                    "native MCP probe failed".to_string(),
                ));
            }
            let scripted = {
                let mut inspections = self.scripted_inspections.borrow_mut();
                (inspections.first().map(|(candidate, _)| *candidate) == Some(target))
                    .then(|| inspections.remove(0).1)
            };
            if let Some(state) = scripted {
                self.set_native_state(target, state.clone());
                return Ok(state);
            }
            Ok(self.native_state(target))
        }

        fn add_mcp_server(
            &self,
            target: AgentTarget,
            scope: InstallScope,
            executable_path: &str,
            _current_dir: &str,
        ) -> Result<NativeAgentAction, RepoGrammarError> {
            let action = NativeAgentAction {
                target,
                program: target.as_str().to_string(),
                args: vec![
                    "mcp".to_string(),
                    "add".to_string(),
                    MCP_SERVER_NAME.to_string(),
                    executable_path.to_string(),
                    "serve".to_string(),
                ],
            };
            self.actions.borrow_mut().push(action.clone());
            if self.fail_add_target == Some(target) {
                return Err(RepoGrammarError::InvalidInput(
                    "native add failed".to_string(),
                ));
            }
            if self.suppress_add_state_target != Some(target) {
                self.set_native_state(
                    target,
                    NativeMcpServerState::Present(NativeMcpServerConfig {
                        executable_path: executable_path.to_string(),
                        args: vec!["serve".to_string()],
                        scope,
                        enabled: true,
                    }),
                );
            }
            Ok(action)
        }

        fn remove_mcp_server(
            &self,
            target: AgentTarget,
            _scope: InstallScope,
            _current_dir: &str,
        ) -> Result<NativeAgentAction, RepoGrammarError> {
            let action = NativeAgentAction {
                target,
                program: target.as_str().to_string(),
                args: vec![
                    "mcp".to_string(),
                    "remove".to_string(),
                    MCP_SERVER_NAME.to_string(),
                ],
            };
            self.actions.borrow_mut().push(action.clone());
            self.set_native_state(target, NativeMcpServerState::NotFound);
            Ok(action)
        }
    }

    #[derive(Default)]
    struct FakeSelfTest {
        fail: bool,
        fail_on_call: Option<usize>,
        calls: RefCell<Vec<String>>,
    }

    impl McpSelfTestRunner for FakeSelfTest {
        fn self_test(
            &self,
            executable_path: &str,
            _current_dir: &str,
        ) -> Result<(), RepoGrammarError> {
            self.calls.borrow_mut().push(executable_path.to_string());
            let call_number = self.calls.borrow().len();
            if self.fail || self.fail_on_call == Some(call_number) {
                Err(RepoGrammarError::InvalidInput(
                    "MCP self-test failed".to_string(),
                ))
            } else {
                Ok(())
            }
        }
    }
}
