//! Safe machine-level agent integration planning.

use crate::error::RepoGrammarError;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MCP_SERVER_NAME: &str = "repogrammar";

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
    pub telemetry_explicitly_configured: bool,
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
    pub data_dir: String,
    pub current_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallExecutionOutcome {
    pub command: &'static str,
    pub target: AgentTarget,
    pub scope: InstallScope,
    pub configured_targets: Vec<AgentTarget>,
    pub receipt_paths: Vec<String>,
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
    require_single_live_target(request.target)?;
    require_global_live_scope(request.scope)?;
    validate_execution_context(context)?;
    self_tester.self_test(&context.executable_path, &context.current_dir)?;

    let mut configured_targets = Vec::new();
    let mut receipt_paths = Vec::new();
    for target in concrete_targets(request.target) {
        let action = configurator.add_mcp_server(
            target,
            request.scope,
            &context.executable_path,
            &context.current_dir,
        )?;
        let receipt_path = match write_install_receipt(request, context, &action) {
            Ok(receipt_path) => receipt_path,
            Err(error) => {
                let _ = configurator.remove_mcp_server(target, request.scope, &context.current_dir);
                return Err(error);
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
        receipt_paths,
        message: "agent MCP integration installed after self-test".to_string(),
    })
}

pub fn execute_uninstall(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    configurator: &impl NativeAgentConfigurator,
) -> Result<InstallExecutionOutcome, RepoGrammarError> {
    require_live_write_confirmation(request)?;
    require_single_live_target(request.target)?;
    require_global_live_scope(request.scope)?;
    validate_execution_context(context)?;

    let mut configured_targets = Vec::new();
    let mut receipt_paths = Vec::new();
    for target in concrete_targets(request.target) {
        let receipt_path = receipt_path(context, target, request.scope);
        if !receipt_path.is_file() {
            continue;
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
        receipt_paths,
        message: "managed agent MCP integration removed".to_string(),
    })
}

fn require_live_write_confirmation(request: &InstallRequest) -> Result<(), RepoGrammarError> {
    if request.assume_yes {
        Ok(())
    } else {
        Err(RepoGrammarError::InvalidInput(
            "live install/uninstall writes require --yes; use --dry-run to inspect the plan"
                .to_string(),
        ))
    }
}

fn require_single_live_target(target: AgentTarget) -> Result<(), RepoGrammarError> {
    if target == AgentTarget::AllSupported {
        Err(RepoGrammarError::InvalidInput(
            "live install/uninstall for --target all is deferred; choose --target codex or --target claude-code".to_string(),
        ))
    } else {
        Ok(())
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
        AgentTarget::AllSupported => vec![AgentTarget::Codex, AgentTarget::ClaudeCode],
        AgentTarget::Codex | AgentTarget::ClaudeCode => vec![target],
    }
}

fn write_install_receipt(
    request: &InstallRequest,
    context: &InstallExecutionContext,
    action: &NativeAgentAction,
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
        "executable_path": context.executable_path,
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
    fn live_all_target_is_deferred_to_avoid_partial_agent_writes() {
        let workspace = TempInstallWorkspace::new("all-target-deferred");
        let request = InstallRequest {
            target: AgentTarget::AllSupported,
            assume_yes: true,
            ..InstallRequest::default()
        };
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("all target deferred");

        assert!(error.to_string().contains("--target all is deferred"));
        assert_eq!(self_test.calls.borrow().len(), 0);
        assert_eq!(configurator.actions.borrow().len(), 0);
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
        fs::remove_dir_all(&workspace.data_dir).expect("remove data dir");
        fs::write(&workspace.data_dir, "file-not-directory").expect("replace data dir");
        let configurator = FakeConfigurator::default();
        let self_test = FakeSelfTest::default();

        let error = execute_install(&request, &workspace.context, &configurator, &self_test)
            .expect_err("receipt write failure");

        assert!(error.to_string().contains("install receipt"));
        let actions = configurator.actions.borrow();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].args[1], "add");
        assert_eq!(actions[1].args[1], "remove");
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
            let current_dir = root.join("project");
            fs::create_dir_all(&data_dir).expect("data dir");
            fs::create_dir_all(&current_dir).expect("project dir");
            let context = InstallExecutionContext {
                executable_path: executable.display().to_string(),
                data_dir: data_dir.display().to_string(),
                current_dir: current_dir.display().to_string(),
            };
            Self {
                root,
                data_dir,
                context,
            }
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
