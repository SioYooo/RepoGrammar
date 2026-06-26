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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallExecutionOutcome {
    pub command: &'static str,
    pub target: AgentTarget,
    pub scope: InstallScope,
    pub configured_targets: Vec<AgentTarget>,
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

pub trait NativeAgentConfigurator {
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
    let mut skipped_targets = Vec::new();
    for target in selected_targets {
        let path = receipt_path(context, target, request.scope);
        if path.is_file() {
            validate_receipt_ownership(&path, target, request.scope)?;
            skipped_targets.push(target);
        } else {
            targets_to_configure.push(target);
        }
    }
    let command_record = install_cli_command(context)?;
    if targets_to_configure.is_empty() {
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

    let mut configured_targets = Vec::new();
    let mut receipt_paths = Vec::new();
    for target in targets_to_configure {
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
                    &configured_targets,
                    &receipt_paths,
                    &command_record,
                );
                return Err(install_rollback_error(error, rollback));
            }
        };
        let receipt_path =
            match write_install_receipt(request, context, &action, &command_record.executable_path)
            {
                Ok(receipt_path) => receipt_path,
                Err(error) => {
                    configured_targets.push(target);
                    let rollback = rollback_install_run(
                        request,
                        context,
                        configurator,
                        &configured_targets,
                        &receipt_paths,
                        &command_record,
                    );
                    return Err(install_rollback_error(error, rollback));
                }
            };
        configured_targets.push(target);
        receipt_paths.push(receipt_path);
    }

    Ok(InstallExecutionOutcome {
        command: "install",
        target: request.target,
        scope: request.scope,
        configured_targets,
        skipped_targets,
        receipt_paths,
        installed_executable_path: Some(command_record.executable_path),
        command_path: Some(command_record.command_path),
        command_on_path: command_record.command_on_path,
        message: "agent MCP integration installed after self-test".to_string(),
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
        configurator.remove_mcp_server(target, request.scope, &context.current_dir)?;
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

fn install_cli_command(
    context: &InstallExecutionContext,
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
    if command_path.exists()
        && !same_path(&command_path, &installed_executable)
        && !command_path_was_managed_copy
    {
        return Err(RepoGrammarError::InvalidInput(
            "repogrammar command path already exists and is not managed by RepoGrammar".to_string(),
        ));
    }
    if !same_path(source, &installed_executable) {
        if executable_existed {
            record.previous_executable = Some(read_file_bytes(
                &installed_executable,
                "installed RepoGrammar CLI",
            )?);
        }
        if command_path_was_managed_copy {
            record.previous_command_copy =
                Some(read_file_bytes(&command_path, "repogrammar command")?);
        }
        fs::copy(source, &installed_executable).map_err(|error| {
            RepoGrammarError::InvalidInput(format!("failed to install RepoGrammar CLI: {error}"))
        })?;
        record.created_executable = !executable_existed;
    }

    if command_path.exists() {
        if !same_path(&command_path, &installed_executable) && command_path_was_managed_copy {
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
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        RepoGrammarError::InvalidInput(format!("failed to refresh repogrammar command: {error}"))
    })
}

fn rollback_install_run(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
    configured_targets: &[AgentTarget],
    receipt_paths: &[String],
    command_record: &CommandInstallRecord,
) -> Vec<String> {
    let mut failures = Vec::new();
    for path in receipt_paths.iter().rev() {
        if let Err(error) = fs::remove_file(path) {
            failures.push(format!("receipt cleanup failed: {error}"));
        }
    }
    for target in configured_targets.iter().rev() {
        if let Err(error) =
            configurator.remove_mcp_server(*target, request.scope, &context.current_dir)
        {
            failures.push(format!("native rollback failed: {error}"));
        }
    }
    failures.extend(rollback_command_install(command_record));
    failures
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

fn binary_name() -> &'static str {
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
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
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
        assert_eq!(self_test.calls.borrow().len(), 1);
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
        assert_eq!(self_test.calls.borrow().len(), 1);
        assert_eq!(configurator.actions.borrow().len(), 1);
        let receipt = fs::read_to_string(&outcome.receipt_paths[0]).expect("receipt");
        let value: serde_json::Value = serde_json::from_str(&receipt).expect("receipt JSON");
        assert_eq!(value["managed_by"], "repogrammar");
        assert_eq!(value["target"], "codex");
        assert_eq!(value["telemetry_enabled"], false);
        assert!(workspace.command_path().exists());
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
    }

    impl NativeAgentConfigurator for FakeConfigurator {
        fn add_mcp_server(
            &self,
            target: AgentTarget,
            _scope: InstallScope,
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
            Ok(action)
        }
    }

    #[derive(Default)]
    struct FakeSelfTest {
        fail: bool,
        calls: RefCell<Vec<String>>,
    }

    impl McpSelfTestRunner for FakeSelfTest {
        fn self_test(
            &self,
            executable_path: &str,
            _current_dir: &str,
        ) -> Result<(), RepoGrammarError> {
            self.calls.borrow_mut().push(executable_path.to_string());
            if self.fail {
                Err(RepoGrammarError::InvalidInput(
                    "MCP self-test failed".to_string(),
                ))
            } else {
                Ok(())
            }
        }
    }
}
